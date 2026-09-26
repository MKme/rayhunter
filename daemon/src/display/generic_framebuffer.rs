use async_trait::async_trait;
use image::{AnimationDecoder, DynamicImage, codecs::gif::GifDecoder, imageops::FilterType};
use std::io::Cursor;
use std::time::Duration;

use crate::config::{self, UiLevel};
use crate::display::screen_alert::{FlashPhase, ScreenAlertController, render_message};
use crate::display::{DisplayState, ScreenAlertCommand};
use rayhunter::analysis::analyzer::EventType;

use log::{error, info};
use tokio::sync::mpsc::Receiver;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use include_dir::{Dir, include_dir};

const NORMAL_REFRESH_RATE: u64 = 1000;
const ALERT_FLASH_RATE: u64 = 500;

#[derive(Copy, Clone)]
pub struct Dimensions {
    pub height: u32,
    pub width: u32,
}

#[derive(Copy, Clone)]
pub enum LinePattern {
    Solid,
    Dashed, // _ _ _ _
    Dotted, // . . . .
}

#[allow(dead_code)]
#[derive(Copy, Clone)]
pub enum Color {
    Red,
    Green,
    Blue,
    White,
    Black,
    Cyan,
    Yellow,
    Pink,
    Orange,
}

impl Color {
    fn rgb(self) -> (u8, u8, u8) {
        match self {
            Color::Red => (0xff, 0, 0),
            Color::Green => (0, 0xff, 0),
            Color::Blue => (0, 0, 0xff),
            Color::White => (0xff, 0xff, 0xff),
            Color::Black => (0, 0, 0),
            Color::Cyan => (0, 0xff, 0xff),
            Color::Yellow => (0xff, 0xff, 0),
            Color::Pink => (0xfe, 0x24, 0xff),
            Color::Orange => (0xff, 0xa5, 0),
        }
    }
}

fn display_style_from_state(state: DisplayState, colorblind_mode: bool) -> (Color, LinePattern) {
    match state {
        DisplayState::Paused => (Color::White, LinePattern::Solid),
        DisplayState::Recording => {
            if colorblind_mode {
                (Color::Blue, LinePattern::Solid)
            } else {
                (Color::Green, LinePattern::Solid)
            }
        }
        DisplayState::WarningDetected { event_type } => match event_type {
            EventType::Informational => {
                if colorblind_mode {
                    (Color::Blue, LinePattern::Solid)
                } else {
                    (Color::Green, LinePattern::Solid)
                }
            }
            EventType::Low => (Color::Yellow, LinePattern::Dotted),
            EventType::Medium => (Color::Orange, LinePattern::Dashed),
            EventType::High => (Color::Red, LinePattern::Solid),
        },
    }
}

#[async_trait]
pub trait GenericFramebuffer: Send + 'static {
    fn dimensions(&self) -> Dimensions;

    async fn write_buffer(&mut self, buffer: Vec<(u8, u8, u8)>); // rgb, row-wise, left-to-right, top-to-bottom

    /// Save any state needed before the alert takes over the full display.
    async fn begin_screen_alert(&mut self) {}

    /// Reassert the display's awake state. The OEM UI may blank it again.
    async fn keep_screen_alert_awake(&mut self) {}

    /// Restore display/backlight state after acknowledgement.
    async fn end_screen_alert(&mut self) {}

    /// Returns true when this device supports the full tactical dashboard.
    async fn draw_tactical(&mut self) -> bool {
        false
    }

    async fn write_dynamic_image(&mut self, img: DynamicImage) {
        let dimensions = self.dimensions();
        let mut width = img.width();
        let mut height = img.height();
        let resized_img: DynamicImage;
        if height > dimensions.height || width > dimensions.width {
            resized_img = img.resize(dimensions.width, dimensions.height, FilterType::CatmullRom);
            width = dimensions.width.min(resized_img.width());
            height = dimensions.height.min(resized_img.height());
        } else {
            resized_img = img;
        }
        let img_rgba8 = resized_img.as_rgba8().unwrap();
        let mut buf = Vec::with_capacity((height * width).try_into().unwrap());
        for y in 0..height {
            for x in 0..width {
                let px = img_rgba8.get_pixel(x, y);
                buf.push((px[0], px[1], px[2]));
            }
        }

        self.write_buffer(buf).await
    }

    async fn draw_gif(&mut self, img_buffer: &[u8]) {
        let cursor = Cursor::new(img_buffer);
        if let Ok(decoder) = GifDecoder::new(cursor) {
            let frames: Vec<_> = decoder
                .into_frames()
                .filter_map(|f| f.ok())
                .map(|frame| {
                    let (numerator, _) = frame.delay().numer_denom_ms();
                    let img = DynamicImage::from(frame.into_buffer());
                    (img, numerator as u64)
                })
                .collect();

            for (img, delay_ms) in frames {
                self.write_dynamic_image(img).await;
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
        }
    }

    async fn draw_img(&mut self, img_buffer: &[u8]) {
        let img = image::load_from_memory(img_buffer).unwrap();
        self.write_dynamic_image(img).await
    }

    async fn draw_line(&mut self, color: Color, height: u32) {
        self.draw_patterned_line(color, height, LinePattern::Solid)
            .await
    }

    async fn draw_patterned_line(&mut self, color: Color, height: u32, pattern: LinePattern) {
        let width = self.dimensions().width;
        let mut buffer = Vec::with_capacity((height * width).try_into().unwrap());

        for _row in 0..height {
            for col in 0..width {
                let should_draw = match pattern {
                    LinePattern::Solid => true,
                    LinePattern::Dashed => (col / 4) % 2 == 0, // 4 pixels on, 4 pixels off
                    LinePattern::Dotted => col % 4 == 0,       // 1 pixel on, 3 pixels off
                };

                if should_draw {
                    buffer.push(color.rgb());
                } else {
                    buffer.push((0, 0, 0)); // Black background
                }
            }
        }

        self.write_buffer(buffer).await
    }

    async fn draw_screen_alert(&mut self, message: &str, phase: FlashPhase) {
        let dimensions = self.dimensions();
        self.write_buffer(render_message(
            dimensions.width as usize,
            dimensions.height as usize,
            message,
            phase,
        ))
        .await;
    }
}

pub fn update_ui(
    task_tracker: &TaskTracker,
    config: &config::Config,
    mut fb: impl GenericFramebuffer,
    shutdown_token: CancellationToken,
    mut ui_update_rx: Receiver<DisplayState>,
    mut screen_alert_rx: Option<Receiver<ScreenAlertCommand>>,
) {
    static IMAGE_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/images/");
    let display_level = config.ui_level;
    if display_level == UiLevel::Invisible && screen_alert_rx.is_none() {
        info!("Invisible mode, not spawning UI.");
        return;
    }

    let colorblind_mode = config.colorblind_mode;
    let mut display_style = display_style_from_state(DisplayState::Recording, colorblind_mode);
    let screen_alert_enabled = config.screen_alert.enabled;
    let screen_alert_message = config.screen_alert.message.clone();

    task_tracker.spawn(async move {
        // this feels wrong, is there a more rusty way to do this?
        let mut img: Option<&[u8]> = None;
        if display_level == UiLevel::Demo {
            img = Some(
                IMAGE_DIR
                    .get_file("orca.gif")
                    .expect("failed to read orca.gif")
                    .contents(),
            );
        } else if display_level == UiLevel::EffLogo {
            img = Some(
                IMAGE_DIR
                    .get_file("eff.png")
                    .expect("failed to read eff.png")
                    .contents(),
            );
        }
        let mut alert = ScreenAlertController::new(screen_alert_enabled);
        let refresh_rate = if display_level == UiLevel::Tactical {
            250
        } else {
            NORMAL_REFRESH_RATE
        };
        let mut normal_ticker = tokio::time::interval(Duration::from_millis(refresh_rate));
        normal_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut alert_ticker = tokio::time::interval(Duration::from_millis(ALERT_FLASH_RATE));
        alert_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = shutdown_token.cancelled() => {
                    if alert.is_active() {
                        fb.end_screen_alert().await;
                    }
                    info!("received UI shutdown");
                    break;
                }
                maybe_state = ui_update_rx.recv() => {
                    let Some(state) = maybe_state else {
                        error!("framebuffer update channel closed");
                        break;
                    };
                    display_style = display_style_from_state(state, colorblind_mode);
                    let was_active = alert.is_active();
                    alert.handle_display_state(state);
                    if !was_active && alert.is_active() {
                        fb.begin_screen_alert().await;
                        draw_alert_frame(&mut fb, &screen_alert_message, &mut alert).await;
                        alert_ticker.reset();
                    }
                }
                maybe_command = receive_screen_alert_command(&mut screen_alert_rx) => {
                    let Some(command) = maybe_command else {
                        screen_alert_rx = None;
                        continue;
                    };
                    match command {
                        ScreenAlertCommand::Test { response_tx } => {
                            let was_active = alert.is_active();
                            alert.test();
                            if !was_active {
                                fb.begin_screen_alert().await;
                            }
                            draw_alert_frame(&mut fb, &screen_alert_message, &mut alert).await;
                            alert_ticker.reset();
                            response_tx.send(()).ok();
                        }
                        ScreenAlertCommand::Acknowledge { response_tx } => {
                            let acknowledged = alert.acknowledge();
                            if acknowledged {
                                fb.end_screen_alert().await;
                            }
                            if let Some(response_tx) = response_tx {
                                response_tx.send(acknowledged).ok();
                            }
                        }
                    }
                }
                _ = alert_ticker.tick(), if alert.is_active() => {
                    draw_alert_frame(&mut fb, &screen_alert_message, &mut alert).await;
                }
                _ = normal_ticker.tick(), if !alert.is_active() => {
                    if display_level == UiLevel::Invisible {
                        continue;
                    }
                    let mut status_bar_height = 2;
                    match display_level {
                        UiLevel::Tactical => {
                            if fb.draw_tactical().await { continue; }
                        }
                        UiLevel::Demo => fb.draw_gif(img.unwrap()).await,
                        UiLevel::EffLogo => fb.draw_img(img.unwrap()).await,
                        UiLevel::HighVisibility => {
                            status_bar_height = fb.dimensions().height;
                        }
                        UiLevel::TransFlag => {
                            fb.draw_line(Color::Cyan, 128).await;
                            fb.draw_line(Color::Pink, 102).await;
                            fb.draw_line(Color::White, 76).await;
                            fb.draw_line(Color::Pink, 50).await;
                            fb.draw_line(Color::Cyan, 25).await;
                        }
                        // UiLevel::Subtle (1) and anything else: just the status bar line
                        _ => {}
                    };
                    let (color, pattern) = display_style;
                    fb.draw_patterned_line(color, status_bar_height, pattern).await;
                }
            }
        }
    });
}

async fn draw_alert_frame(
    fb: &mut impl GenericFramebuffer,
    message: &str,
    alert: &mut ScreenAlertController,
) {
    fb.keep_screen_alert_awake().await;
    fb.draw_screen_alert(message, alert.phase()).await;
    alert.advance_phase();
}

async fn receive_screen_alert_command(
    receiver: &mut Option<Receiver<ScreenAlertCommand>>,
) -> Option<ScreenAlertCommand> {
    match receiver {
        Some(receiver) => receiver.recv().await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::sync::{mpsc, oneshot};

    #[derive(Default)]
    struct FakeState {
        begins: usize,
        wakes: usize,
        phases: Vec<FlashPhase>,
        ends: usize,
    }

    struct FakeFramebuffer(Arc<Mutex<FakeState>>);

    #[async_trait]
    impl GenericFramebuffer for FakeFramebuffer {
        fn dimensions(&self) -> Dimensions {
            Dimensions {
                width: 128,
                height: 128,
            }
        }

        async fn write_buffer(&mut self, _buffer: Vec<(u8, u8, u8)>) {}

        async fn begin_screen_alert(&mut self) {
            self.0.lock().unwrap().begins += 1;
        }

        async fn keep_screen_alert_awake(&mut self) {
            self.0.lock().unwrap().wakes += 1;
        }

        async fn end_screen_alert(&mut self) {
            self.0.lock().unwrap().ends += 1;
        }

        async fn draw_screen_alert(&mut self, _message: &str, phase: FlashPhase) {
            self.0.lock().unwrap().phases.push(phase);
        }
    }

    #[tokio::test]
    async fn test_alert_flashes_until_acknowledged_and_restores_display() {
        let state = Arc::new(Mutex::new(FakeState::default()));
        let task_tracker = TaskTracker::new();
        let shutdown = CancellationToken::new();
        let (_ui_tx, ui_rx) = mpsc::channel(1);
        let (alert_tx, alert_rx) = mpsc::channel(2);
        let mut config = config::Config::default();
        config.ui_level = UiLevel::Invisible;

        update_ui(
            &task_tracker,
            &config,
            FakeFramebuffer(state.clone()),
            shutdown.clone(),
            ui_rx,
            Some(alert_rx),
        );

        let (test_response_tx, test_response_rx) = oneshot::channel();
        alert_tx
            .send(ScreenAlertCommand::Test {
                response_tx: test_response_tx,
            })
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), test_response_rx)
            .await
            .expect("test frame should be rendered")
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(state.lock().unwrap().phases, vec![FlashPhase::Warning]);
        tokio::time::sleep(Duration::from_millis(ALERT_FLASH_RATE + 100)).await;

        let (ack_response_tx, ack_response_rx) = oneshot::channel();
        alert_tx
            .send(ScreenAlertCommand::Acknowledge {
                response_tx: Some(ack_response_tx),
            })
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), ack_response_rx)
                .await
                .unwrap()
                .unwrap()
        );

        let state = state.lock().unwrap();
        assert_eq!(state.begins, 1);
        assert_eq!(state.ends, 1);
        assert!(state.wakes >= 2);
        assert!(state.phases.contains(&FlashPhase::Warning));
        assert!(state.phases.contains(&FlashPhase::Dark));
        drop(state);

        shutdown.cancel();
        task_tracker.close();
        task_tracker.wait().await;
    }
}
