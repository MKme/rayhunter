use crate::config;
use crate::display::generic_framebuffer::{self, Dimensions, GenericFramebuffer};
use crate::display::{DisplayState, ScreenAlertCommand};
use async_trait::async_trait;
use log::warn;

use tokio::sync::mpsc::Receiver;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

const FB_PATH: &str = "/dev/fb0";
const FB_BLANK_PATH: &str = "/sys/class/graphics/fb0/blank";
const BACKLIGHT_BRIGHTNESS_PATH: &str = "/sys/class/leds/lcd-backlight/brightness";
const BACKLIGHT_MAX_PATH: &str = "/sys/class/leds/lcd-backlight/max_brightness";
const DISPLAY_ON_PATH: &str = "/sys/devices/78b6000.spi/spi_master/spi1/spi1.0/display_on";
const BACKLIGHT_GPIO_PATH: &str = "/sys/devices/78b6000.spi/spi_master/spi1/spi1.0/bl_gpio";
const DISPLAY_INIT_PATH: &str = "/sys/devices/78b6000.spi/spi_master/spi1/spi1.0/init";

#[derive(Default)]
struct Framebuffer {
    saved_frame: Option<Vec<u8>>,
    saved_brightness: Option<Vec<u8>>,
    saved_blank: Option<Vec<u8>>,
    saved_display_on: Option<Vec<u8>>,
    saved_backlight_gpio: Option<Vec<u8>>,
    alert_brightness: Option<Vec<u8>>,
    vendor_wake_ready: bool,
    wake_failure_logged: bool,
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

        tokio::fs::write(FB_PATH, &raw_buffer).await.unwrap();
    }

    async fn begin_screen_alert(&mut self) {
        self.wake_failure_logged = false;
        if self.saved_frame.is_none() {
            self.saved_frame = tokio::fs::read(FB_PATH).await.ok();
        }
        if self.saved_brightness.is_none() {
            self.saved_brightness = tokio::fs::read(BACKLIGHT_BRIGHTNESS_PATH).await.ok();
            self.alert_brightness = tokio::fs::read(BACKLIGHT_MAX_PATH).await.ok();
        }
        if self.saved_blank.is_none() {
            self.saved_blank = tokio::fs::read(FB_BLANK_PATH).await.ok();
        }
        if self.saved_display_on.is_none() {
            self.saved_display_on = tokio::fs::read(DISPLAY_ON_PATH).await.ok();
        }
        if self.saved_backlight_gpio.is_none() {
            self.saved_backlight_gpio = tokio::fs::read(BACKLIGHT_GPIO_PATH).await.ok();
        }

        // Waking only the RC400L backlight can leave the ST7735S controller's
        // display RAM solid white. Reinitialize the panel after a real screen
        // timeout, but avoid the visible clear when the screen is already on.
        let panel_ready = if self
            .saved_backlight_gpio
            .as_deref()
            .is_some_and(sysfs_value_is_on)
        {
            true
        } else {
            tokio::fs::write(DISPLAY_INIT_PATH, b"1").await.is_ok()
        };
        let display_on = tokio::fs::write(DISPLAY_ON_PATH, b"1").await.is_ok();
        self.vendor_wake_ready = panel_ready && display_on;
        self.keep_screen_alert_awake().await;
    }

    async fn keep_screen_alert_awake(&mut self) {
        if let Some(brightness) = &self.alert_brightness {
            tokio::fs::write(BACKLIGHT_BRIGHTNESS_PATH, brightness)
                .await
                .ok();
        }
        // The RC400L's vendor fbtft driver exposes its active-low backlight
        // control on the SPI device. The generic framebuffer blank ioctl is not
        // implemented by this driver, so reassert this while the alert flashes.
        let backlight_on = tokio::fs::write(BACKLIGHT_GPIO_PATH, b"1").await.is_ok();
        let sysfs_unblanked = tokio::fs::write(FB_BLANK_PATH, b"0").await.is_ok();
        let ioctl_unblanked = ioctl_unblank_framebuffer();
        let vendor_unblanked = self.vendor_wake_ready && backlight_on;
        let wake_succeeded = vendor_unblanked || sysfs_unblanked || ioctl_unblanked;
        if !wake_succeeded && !self.wake_failure_logged {
            warn!("unable to unblank Orbic framebuffer for screen alert");
            self.wake_failure_logged = true;
        }
    }

    async fn end_screen_alert(&mut self) {
        if let Some(frame) = self.saved_frame.take() {
            tokio::fs::write(FB_PATH, frame).await.ok();
        }
        if let Some(brightness) = self.saved_brightness.take() {
            tokio::fs::write(BACKLIGHT_BRIGHTNESS_PATH, brightness)
                .await
                .ok();
        }
        if let Some(blank) = self.saved_blank.take() {
            tokio::fs::write(FB_BLANK_PATH, blank).await.ok();
        }
        if let Some(display_on) = self.saved_display_on.take() {
            tokio::fs::write(DISPLAY_ON_PATH, display_on).await.ok();
        }
        if let Some(backlight_gpio) = self.saved_backlight_gpio.take() {
            tokio::fs::write(BACKLIGHT_GPIO_PATH, backlight_gpio)
                .await
                .ok();
        }
        self.alert_brightness = None;
        self.vendor_wake_ready = false;
    }
}

fn sysfs_value_is_on(value: &[u8]) -> bool {
    value.iter().copied().any(|byte| byte == b'1')
}

#[cfg(target_os = "linux")]
fn ioctl_unblank_framebuffer() -> bool {
    use std::fs::OpenOptions;
    use std::os::fd::AsRawFd;

    const FBIOBLANK: u64 = 0x4611;
    let Ok(framebuffer) = OpenOptions::new().read(true).write(true).open(FB_PATH) else {
        return false;
    };
    // FB_BLANK_UNBLANK is zero. msm_fb routes this ioctl to the panel/backlight driver.
    unsafe { libc::ioctl(framebuffer.as_raw_fd(), FBIOBLANK as _, 0) >= 0 }
}

#[cfg(not(target_os = "linux"))]
fn ioctl_unblank_framebuffer() -> bool {
    false
}

pub fn update_ui(
    task_tracker: &TaskTracker,
    config: &config::Config,
    shutdown_token: CancellationToken,
    ui_update_rx: Receiver<DisplayState>,
    screen_alert_rx: Receiver<ScreenAlertCommand>,
) {
    generic_framebuffer::update_ui(
        task_tracker,
        config,
        Framebuffer::default(),
        shutdown_token,
        ui_update_rx,
        Some(screen_alert_rx),
    )
}

#[cfg(test)]
mod tests {
    use super::sysfs_value_is_on;

    #[test]
    fn parses_vendor_backlight_state() {
        assert!(sysfs_value_is_on(b"1\n"));
        assert!(!sysfs_value_is_on(b"0\n"));
        assert!(!sysfs_value_is_on(b""));
    }
}
