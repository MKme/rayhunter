use super::tactical::{self, SharedStats, Telemetry};
use crate::config;
use crate::display::generic_framebuffer::{self, Dimensions, GenericFramebuffer};
use crate::display::{DisplayState, ScreenAlertCommand};
use async_trait::async_trait;
use log::warn;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

use tokio::sync::mpsc::Receiver;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

const FB_PATH: &str = "/dev/fb0";
const VENDOR_DISPLAY_TOOL: &str = "/usr/bin/qt_test";
const WAKE_REFRESH: Duration = Duration::from_secs(5);
// Two queued wake requests must still fit the API's two-second response budget.
const WAKE_TIMEOUT: Duration = Duration::from_millis(750);

#[derive(Default)]
struct Framebuffer {
    panel: PanelPaths,
    tactical: Option<TacticalDisplay>,
    saved_frame: Option<Vec<u8>>,
    last_wake: Option<Instant>,
    wake_failure_logged: bool,
}

struct PanelPaths {
    framebuffer: PathBuf,
    vendor_tool: PathBuf,
}

impl Default for PanelPaths {
    fn default() -> Self {
        Self {
            framebuffer: FB_PATH.into(),
            vendor_tool: VENDOR_DISPLAY_TOOL.into(),
        }
    }
}

// Ask the existing Orbic service to wake the display. It owns the LCD sleep,
// reset and backlight sequence. Direct sysfs writes can race that service and
// leave the physical panel white even while framebuffer readback is correct.
async fn wake_vendor_display(tool: &Path) -> bool {
    let request = async {
        let mut child = tokio::process::Command::new(tool)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .ok()?;
        // 24: Set Screen Status; 1: awake; -1: exit. No key/reset events.
        let mut stdin = child.stdin.take()?;
        stdin.write_all(b"24\n1\n-1\n").await.ok()?;
        drop(stdin);
        let output = child.wait_with_output().await.ok()?;
        Some(
            output.status.success()
                && String::from_utf8_lossy(&output.stdout).contains("Successed, rc:12"),
        )
    };
    tokio::time::timeout(WAKE_TIMEOUT, request)
        .await
        .ok()
        .flatten()
        .unwrap_or(false)
}

struct TacticalDisplay {
    stats: SharedStats,
    telemetry: Telemetry,
    refreshed: Option<Instant>,
    started: Instant,
    port: u16,
    device: rayhunter::Device,
}

#[async_trait]
impl GenericFramebuffer for Framebuffer {
    fn dimensions(&self) -> Dimensions {
        // TODO actually poll for this, maybe w/ fbset?
        Dimensions {
            height: 128,
            width: 128,
        }
    }

    async fn write_buffer(&mut self, buffer: Vec<(u8, u8, u8)>) {
        let mut raw_buffer = Vec::with_capacity(buffer.len() * 2);
        for (r, g, b) in buffer {
            let mut rgb565: u16 = (r as u16 & 0b11111000) << 8;
            rgb565 |= (g as u16 & 0b11111100) << 3;
            rgb565 |= (b as u16) >> 3;
            raw_buffer.extend(rgb565.to_le_bytes());
        }

        tokio::fs::write(&self.panel.framebuffer, &raw_buffer)
            .await
            .unwrap();
    }

    async fn draw_tactical(&mut self) -> bool {
        let Some(display) = &mut self.tactical else {
            return false;
        };
        let now = Instant::now();
        if display
            .refreshed
            .is_none_or(|at| now.duration_since(at) >= Duration::from_secs(5))
        {
            display.telemetry.battery = crate::battery::get_battery_status(&display.device)
                .await
                .ok()
                .map(|b| (b.level, b.is_plugged_in));
            display.telemetry.addresses.clear();
            match if_addrs::get_if_addrs() {
                Ok(interfaces) => {
                    display.telemetry.network_error = false;
                    for interface in interfaces {
                        if let std::net::IpAddr::V4(ip) = interface.ip() {
                            if let Some(address) = tactical::config_address(&interface.name, ip) {
                                display.telemetry.addresses.push(address);
                            }
                        }
                    }
                    display.telemetry.addresses.sort();
                    display.telemetry.addresses.dedup();
                }
                Err(_) => display.telemetry.network_error = true,
            }
            display.refreshed = Some(now);
        }
        let stats = display.stats.lock().unwrap().clone();
        let pixels = tactical::render(
            &stats,
            &display.telemetry,
            display.port,
            now,
            (now.duration_since(display.started).as_secs() / 5) as usize,
        );
        self.write_buffer(pixels).await;
        true
    }

    async fn begin_screen_alert(&mut self) {
        self.wake_failure_logged = false;
        self.last_wake = None;
        if self.tactical.is_none() && self.saved_frame.is_none() {
            self.saved_frame = tokio::fs::read(&self.panel.framebuffer).await.ok();
        }
        self.keep_screen_alert_awake().await;
    }

    async fn keep_screen_alert_awake(&mut self) {
        if self.last_wake.is_some_and(|at| at.elapsed() < WAKE_REFRESH) {
            return;
        }
        self.last_wake = Some(Instant::now());
        if !wake_vendor_display(&self.panel.vendor_tool).await && !self.wake_failure_logged {
            warn!(
                "vendor LCD wake unavailable; alert is rendered but may require Power/OK to wake the screen"
            );
            self.wake_failure_logged = true;
        }
    }

    async fn end_screen_alert(&mut self) {
        // Leave the display awake for review and let the OEM timeout sleep it.
        // Never restore stale power controls or reinitialize the LCD here.
        self.last_wake = None;
        self.keep_screen_alert_awake().await;
        if self.tactical.is_some() {
            self.draw_tactical().await;
            self.saved_frame = None;
        } else if let Some(frame) = self.saved_frame.take() {
            tokio::fs::write(&self.panel.framebuffer, frame).await.ok();
        }
        self.last_wake = None;
    }
}

pub fn update_ui(
    task_tracker: &TaskTracker,
    config: &config::Config,
    shutdown_token: CancellationToken,
    ui_update_rx: Receiver<DisplayState>,
    screen_alert_rx: Receiver<ScreenAlertCommand>,
    tactical_stats: SharedStats,
) {
    let framebuffer = Framebuffer {
        tactical: (config.ui_level == config::UiLevel::Tactical).then(|| TacticalDisplay {
            stats: tactical_stats,
            telemetry: Telemetry::default(),
            refreshed: None,
            started: Instant::now(),
            port: config.port,
            device: config.device.clone(),
        }),
        ..Framebuffer::default()
    };
    generic_framebuffer::update_ui(
        task_tracker,
        config,
        framebuffer,
        shutdown_token,
        ui_update_rx,
        Some(screen_alert_rx),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn fake_vendor(dir: &Path, response: &str) -> PathBuf {
        let tool = dir.join("qt_test");
        std::fs::write(&tool, format!(
            "#!/bin/sh\nread choice\nread state\nread quit\n[ \"$choice:$state:$quit\" = \"24:1:-1\" ] || exit 1\nprintf '%s' '{}'\n",
            response
        )).unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o700)).unwrap();
        tool
    }

    #[tokio::test]
    async fn vendor_wake_requires_successful_service_reply() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!wake_vendor_display(&dir.path().join("missing")).await);
        let tool = fake_vendor(dir.path(), "Failed");
        assert!(!wake_vendor_display(&tool).await);
        let tool = fake_vendor(dir.path(), "Successed, rc:12");
        assert!(wake_vendor_display(&tool).await);
        std::fs::write(&tool, "#!/bin/sh\nprintf 'Successed, rc:12'\nexit 1\n").unwrap();
        assert!(!wake_vendor_display(&tool).await);
        // A hung vendor utility must not indefinitely block acknowledgment.
        std::fs::write(&tool, "#!/bin/sh\nexec sleep 30\n").unwrap();
        let started = Instant::now();
        assert!(!wake_vendor_display(&tool).await);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn tactical_alert_acknowledgment_repaints_and_leaves_power_to_vendor() {
        let dir = tempfile::tempdir().unwrap();
        let now = Instant::now();
        let stats = SharedStats::default();
        stats.lock().unwrap().start(now);
        stats.lock().unwrap().receive(now);
        let mut fb = Framebuffer {
            panel: PanelPaths {
                framebuffer: dir.path().join("fb"),
                vendor_tool: fake_vendor(dir.path(), "Successed, rc:12"),
            },
            tactical: Some(TacticalDisplay {
                stats,
                telemetry: Telemetry::default(),
                refreshed: Some(now),
                started: now,
                port: 8080,
                device: rayhunter::Device::Orbic,
            }),
            saved_frame: Some(vec![255; 32768]),
            ..Framebuffer::default()
        };
        fb.begin_screen_alert().await;
        assert!(!fb.wake_failure_logged);
        let first_wake = fb.last_wake;
        fb.keep_screen_alert_awake().await;
        assert_eq!(
            fb.last_wake, first_wake,
            "wake must be throttled between frames"
        );
        fb.end_screen_alert().await;
        let frame = tokio::fs::read(&fb.panel.framebuffer).await.unwrap();
        assert_eq!(frame.len(), 32768);
        assert_ne!(frame, vec![255; 32768]);
        assert!(frame.windows(2).any(|pixel| pixel != &frame[..2]));
        assert!(fb.saved_frame.is_none());
        assert!(fb.last_wake.is_none());
        assert!(!fb.wake_failure_logged);
    }
}
