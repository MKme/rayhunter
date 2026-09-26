//! Dependency-free LCD model and renderer. Counts are actual analyzer events in
//! the current/last recording, never severity transitions or test notifications.
use super::bitmap_font::glyph;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub type SharedStats = Arc<Mutex<TacticalStats>>;
pub type Pixel = (u8, u8, u8);
const BG: Pixel = (3, 10, 12);
const DIM: Pixel = (100, 153, 158);
const WHITE: Pixel = (225, 244, 239);
const GREEN: Pixel = (64, 240, 130);
const AMBER: Pixel = (255, 182, 48);
const RED: Pixel = (255, 75, 65);

#[derive(Default, Clone)]
pub struct TacticalStats {
    pub recording: bool,
    pub started: Option<Instant>,
    pub stopped: Option<Instant>,
    pub last_rx: Option<Instant>,
    /// Low, medium, high. Informational events are not alerts.
    pub counts: [u64; 3],
    pub analysis_failed: bool,
    pub capture_failed: bool,
}

impl TacticalStats {
    pub fn start(&mut self, now: Instant) {
        *self = Self {
            recording: true,
            started: Some(now),
            ..Self::default()
        };
    }
    pub fn stop(&mut self, now: Instant) {
        self.recording = false;
        self.stopped = Some(now);
    }
    pub fn receive(&mut self, now: Instant) {
        self.last_rx = Some(now);
    }
    pub fn detect(&mut self, severity: usize) {
        if let Some(count) = self.counts.get_mut(severity) {
            *count = count.saturating_add(1);
        }
    }
    pub fn status(&self, now: Instant) -> (&'static str, Pixel) {
        if self.capture_failed {
            return ("DIAG ERROR", RED);
        }
        if self.started.is_none() && self.stopped.is_none() {
            return ("STARTING", AMBER);
        }
        if !self.recording {
            return ("PAUSED", AMBER);
        }
        if self.analysis_failed {
            return ("ANALYSIS!", RED);
        }
        if self.counts[2] > 0 {
            return ("HIGH ALERT", RED);
        }
        if self.counts[1] > 0 {
            return ("MED ALERT", AMBER);
        }
        if self.counts[0] > 0 {
            return ("LOW ALERT", AMBER);
        }
        if self
            .last_rx
            .is_none_or(|rx| now.saturating_duration_since(rx) > Duration::from_secs(30))
        {
            return ("WAIT DATA", AMBER);
        }
        ("RECORDING", GREEN)
    }
}

/// A failed/ended capture task must not leave a green recording indicator.
pub struct CaptureGuard(pub SharedStats);
impl Drop for CaptureGuard {
    fn drop(&mut self) {
        let mut stats = self.0.lock().unwrap();
        stats.stop(Instant::now());
        stats.capture_failed = true;
    }
}

#[derive(Default)]
pub struct Telemetry {
    pub addresses: Vec<(String, Ipv4Addr)>,
    pub network_error: bool,
    pub battery: Option<(u8, bool)>,
}

/// Only addresses served by the IPv4 web listener. Never advertise a cellular
/// bearer as a configuration LAN, or invent a default when enumeration fails.
pub fn config_address(name: &str, ip: Ipv4Addr) -> Option<(String, Ipv4Addr)> {
    if !(ip.is_private() || ip.is_link_local())
        || name.starts_with("rmnet")
        || name.starts_with("wwan")
    {
        return None;
    }
    let label = if name.starts_with("rndis") || name.starts_with("usb") {
        "USB"
    } else if name.starts_with("wlan") || name.starts_with("wlp") {
        "WIFI"
    } else {
        "LAN"
    };
    Some((label.to_string(), ip))
}

fn count(value: u64) -> String {
    if value > 999 {
        "999+".to_string()
    } else {
        value.to_string()
    }
}
fn age(seconds: u64) -> String {
    if seconds < 60 {
        format!("{seconds}S")
    } else if seconds < 3600 {
        format!("{}M", seconds / 60)
    } else if seconds < 360000 {
        format!("{}H", seconds / 3600)
    } else {
        "99H+".to_string()
    }
}

pub fn render(
    stats: &TacticalStats,
    telemetry: &Telemetry,
    port: u16,
    now: Instant,
    page: usize,
) -> Vec<Pixel> {
    let mut canvas = Canvas(vec![BG; 128 * 128]);
    let (status, color) = stats.status(now);
    canvas.line(0, color);
    canvas.text(4, 4, "RAYHUNTER", 1, DIM);
    canvas.text(
        100,
        4,
        if stats.recording { "REC" } else { "---" },
        1,
        color,
    );
    canvas.text(4, 17, status, 2, color);
    canvas.text(
        4,
        35,
        if stats.recording {
            "SESSION"
        } else {
            "LAST RUN"
        },
        1,
        DIM,
    );
    canvas.text(4, 44, "ALERTS", 1, WHITE);
    let total = stats.counts.iter().fold(0u64, |a, b| a.saturating_add(*b));
    canvas.text(
        76,
        35,
        &count(total),
        2,
        if total > 0 { AMBER } else { GREEN },
    );
    canvas.text(
        4,
        56,
        &format!("H{}", count(stats.counts[2])),
        1,
        if stats.counts[2] > 0 { RED } else { DIM },
    );
    canvas.text(
        46,
        56,
        &format!("M{}", count(stats.counts[1])),
        1,
        if stats.counts[1] > 0 { AMBER } else { DIM },
    );
    canvas.text(
        88,
        56,
        &format!("L{}", count(stats.counts[0])),
        1,
        if stats.counts[0] > 0 { AMBER } else { DIM },
    );
    let rx = stats
        .last_rx
        .map(|rx| age(now.saturating_duration_since(rx).as_secs()))
        .unwrap_or("NONE".into());
    let elapsed = stats
        .started
        .map(|start| {
            age(stats
                .stopped
                .unwrap_or(now)
                .saturating_duration_since(start)
                .as_secs())
        })
        .unwrap_or("--".into());
    canvas.text(4, 68, &format!("RX {rx}   RUN {elapsed}"), 1, WHITE);
    let battery = telemetry
        .battery
        .map(|(level, charging)| {
            format!(
                "BAT {level}% {}",
                if charging { "CHARGING" } else { "BATTERY" }
            )
        })
        .unwrap_or("BAT UNKNOWN".into());
    canvas.text(
        4,
        80,
        &battery,
        1,
        if telemetry.battery.is_some_and(|(level, _)| level <= 10) {
            RED
        } else {
            DIM
        },
    );
    canvas.line(90, (25, 65, 70));
    canvas.text(4, 94, &format!("CONFIG HTTP :{port}"), 1, GREEN);
    if telemetry.addresses.is_empty() {
        canvas.text(
            4,
            105,
            if telemetry.network_error {
                "IP READ FAILED"
            } else {
                "NO LAN IP"
            },
            1,
            AMBER,
        );
        canvas.text(4, 117, "CHECK USB / WIFI", 1, DIM);
    } else {
        let pages = telemetry.addresses.len().div_ceil(2);
        for (i, (label, ip)) in telemetry
            .addresses
            .iter()
            .skip((page % pages) * 2)
            .take(2)
            .enumerate()
        {
            canvas.text(4, 105 + i * 12, &format!("{label} {ip}"), 1, WHITE);
        }
    }
    canvas.0
}

struct Canvas(Vec<Pixel>);
impl Canvas {
    fn line(&mut self, y: usize, color: Pixel) {
        self.0[y * 128..(y + 1) * 128].fill(color);
    }
    fn text(&mut self, x: usize, y: usize, text: &str, scale: usize, color: Pixel) {
        for (index, ch) in text.chars().enumerate() {
            for (row_index, row) in glyph(ch).iter().enumerate() {
                for col in 0..5 {
                    if row & (1 << (4 - col)) == 0 {
                        continue;
                    }
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let px = x + index * 6 * scale + col * scale + dx;
                            let py = y + row_index * scale + dy;
                            if px < 128 && py < 128 {
                                self.0[py * 128 + px] = color;
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_events_count_and_pause_preserves_them_until_new_recording() {
        let now = Instant::now();
        let mut s = TacticalStats::default();
        assert_eq!(s.status(now).0, "STARTING");
        s.start(now);
        assert_eq!(s.status(now).0, "WAIT DATA");
        s.receive(now);
        assert_eq!(s.status(now).0, "RECORDING");
        s.detect(0);
        s.detect(0);
        s.detect(2);
        s.detect(3);
        assert_eq!(s.counts, [2, 0, 1]);
        assert_eq!(s.status(now).0, "HIGH ALERT");
        s.stop(now);
        assert_eq!(s.status(now).0, "PAUSED");
        assert_eq!(s.counts, [2, 0, 1]);
        s.start(now);
        assert_eq!(s.counts, [0; 3]);
        assert!(s.last_rx.is_none());
    }
    #[test]
    fn loss_of_data_and_analysis_failure_never_look_healthy() {
        let now = Instant::now();
        let mut s = TacticalStats::default();
        s.start(now);
        s.receive(now);
        assert_eq!(s.status(now + Duration::from_secs(31)).0, "WAIT DATA");
        s.analysis_failed = true;
        assert_eq!(s.status(now).0, "ANALYSIS!");
        s.counts[2] = u64::MAX;
        s.detect(2);
        assert_eq!(s.counts[2], u64::MAX);
    }
    #[test]
    fn capture_exit_is_reported_even_before_first_packet() {
        let stats = SharedStats::default();
        {
            let _guard = CaptureGuard(stats.clone());
        }
        let stats = stats.lock().unwrap();
        assert_eq!(stats.status(Instant::now()).0, "DIAG ERROR");
        assert!(!stats.recording);
    }
    #[test]
    fn failed_initial_start_can_be_shown_as_paused_without_inventing_a_session() {
        let now = Instant::now();
        let mut stats = TacticalStats::default();
        stats.stop(now);
        assert_eq!(stats.status(now).0, "PAUSED");
        assert!(stats.started.is_none());
        assert_eq!(stats.counts, [0; 3]);
    }
    #[test]
    fn configuration_addresses_exclude_loopback_public_and_cellular_interfaces() {
        assert!(config_address("lo", Ipv4Addr::LOCALHOST).is_none());
        assert!(config_address("rmnet0", Ipv4Addr::new(10, 0, 0, 2)).is_none());
        assert!(config_address("wwan0", Ipv4Addr::new(10, 0, 0, 2)).is_none());
        assert!(config_address("eth0", Ipv4Addr::new(8, 8, 8, 8)).is_none());
        assert_eq!(
            config_address("rndis0", Ipv4Addr::new(192, 168, 1, 1))
                .unwrap()
                .0,
            "USB"
        );
    }
    #[test]
    fn frame_is_bounded_and_pages_do_not_drop_addresses() {
        let now = Instant::now();
        let s = TacticalStats::default();
        let t = Telemetry {
            addresses: vec![
                ("WIFI".into(), Ipv4Addr::new(192, 168, 100, 254)),
                ("USB".into(), Ipv4Addr::new(192, 168, 1, 1)),
                ("LAN".into(), Ipv4Addr::new(10, 0, 0, 1)),
            ],
            ..Telemetry::default()
        };
        let a = render(&s, &t, 65535, now, 0);
        let b = render(&s, &t, 65535, now, 1);
        assert_eq!(a.len(), 16384);
        assert_eq!(a[..105 * 128], b[..105 * 128]);
        assert_ne!(a[105 * 128..], b[105 * 128..]);
        assert_eq!(a, render(&s, &t, 65535, now, 2));
        assert_eq!(4 + "WIFI 192.168.100.254".len() * 6 - 1, 123);
    }
}
