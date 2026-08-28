use log::{error, info};
use std::time::{Duration, Instant};
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::{mpsc::Sender, oneshot};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::config::{self, KeyInputMode};
use crate::diag::DiagDeviceCtrlMessage;
use crate::display::ScreenAlertCommand;

#[derive(Debug, Clone, Copy)]
enum Event {
    KeyDown,
    KeyUp,
    Other,
}

const INPUT_EVENT_SIZE: usize = 32;
const SCREEN_ALERT_RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Default)]
struct InputState {
    last_keyup: Option<Instant>,
    last_event_time: Option<Instant>,
    consume_next_keyup: bool,
}

pub fn run_key_input_thread(
    task_tracker: &TaskTracker,
    config: &config::Config,
    diag_tx: Sender<DiagDeviceCtrlMessage>,
    screen_alert_tx: Option<Sender<ScreenAlertCommand>>,
    cancellation_token: CancellationToken,
) {
    let screen_alert_tx = if config.screen_alert.enabled {
        screen_alert_tx
    } else {
        None
    };
    if config.key_input_mode == KeyInputMode::Disabled && screen_alert_tx.is_none() {
        return;
    }

    // The Orbic exposes its Power/OK and Menu buttons through additional
    // evdev nodes used by the stock Qt UI. Keep the original event0 behavior
    // (including double-tap recording control), but let either front-panel
    // button acknowledge a latched screen alert.
    if let Some(screen_alert_tx) = screen_alert_tx.clone() {
        for path in ["/dev/input/event1", "/dev/input/event2"] {
            run_alert_ack_input_thread(
                task_tracker,
                path,
                screen_alert_tx.clone(),
                cancellation_token.clone(),
            );
        }
    }

    let key_input_mode = config.key_input_mode;

    task_tracker.spawn(async move {
        // Open the input device
        let mut file = match File::open("/dev/input/event0").await {
            Ok(file) => file,
            Err(e) => {
                error!("Failed to open /dev/input/event0: {e}");
                return;
            }
        };

        let mut buffer = [0u8; INPUT_EVENT_SIZE];
        let mut input_state = InputState::default();

        loop {
            tokio::select! {
               _ = cancellation_token.cancelled() => {
                    info!("received key input shutdown");
                    return;
                }
                result = file.read_exact(&mut buffer) => {
                    if let Err(e) = result {
                        error!("failed to read key input: {e}");
                        return;
                    }
                }
            }

            let event = parse_event(buffer);

            process_event(
                event,
                Instant::now(),
                key_input_mode,
                &mut input_state,
                &diag_tx,
                &screen_alert_tx,
            )
            .await;
        }
    });
}

fn run_alert_ack_input_thread(
    task_tracker: &TaskTracker,
    path: &'static str,
    screen_alert_tx: Sender<ScreenAlertCommand>,
    cancellation_token: CancellationToken,
) {
    task_tracker.spawn(async move {
        let mut file = match File::open(path).await {
            Ok(file) => file,
            Err(e) => {
                error!("Failed to open {path} for screen alert acknowledgement: {e}");
                return;
            }
        };
        let mut buffer = [0u8; INPUT_EVENT_SIZE];
        let screen_alert_tx = Some(screen_alert_tx);

        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => return,
                result = file.read_exact(&mut buffer) => {
                    if let Err(e) = result {
                        error!("failed to read {path} for screen alert acknowledgement: {e}");
                        return;
                    }
                }
            }

            if matches!(parse_event(buffer), Event::KeyDown) {
                acknowledge_screen_alert(&screen_alert_tx).await;
            }
        }
    });
}

async fn process_event(
    event: Event,
    now: Instant,
    key_input_mode: KeyInputMode,
    state: &mut InputState,
    diag_tx: &Sender<DiagDeviceCtrlMessage>,
    screen_alert_tx: &Option<Sender<ScreenAlertCommand>>,
) {
    // Always consume the release paired with an alert acknowledgement, even if
    // it follows the key-down inside the normal debounce window.
    if matches!(event, Event::KeyUp) && state.consume_next_keyup {
        state.consume_next_keyup = false;
        state.last_keyup = None;
        state.last_event_time = Some(now);
        return;
    }

    // On Orbic it was observed that pressing the power button can trigger many
    // successive events. Drop events that are too close together.
    if let Some(last_time) = state.last_event_time
        && now.duration_since(last_time) < Duration::from_millis(50)
    {
        state.last_event_time = Some(now);
        return;
    }
    state.last_event_time = Some(now);

    match event {
        Event::KeyUp => {
            if let Some(last_keyup_instant) = state.last_keyup {
                let elapsed = now.duration_since(last_keyup_instant);

                if key_input_mode == KeyInputMode::DoubleTapPower
                    && elapsed >= Duration::from_millis(100)
                    && elapsed <= Duration::from_millis(800)
                {
                    if let Err(e) = diag_tx.send(DiagDeviceCtrlMessage::StopRecording).await {
                        error!("Failed to send StopRecording: {e}");
                    }
                    if let Err(e) = diag_tx
                        .send(DiagDeviceCtrlMessage::StartRecording { response_tx: None })
                        .await
                    {
                        error!("Failed to send StartRecording: {e}");
                    }
                    state.last_keyup = None;
                    return;
                }
            }

            state.last_keyup = Some(now);
        }
        Event::KeyDown => {
            if acknowledge_screen_alert(screen_alert_tx).await {
                state.consume_next_keyup = true;
                state.last_keyup = None;
            }
        }
        Event::Other => {}
    }
}

async fn acknowledge_screen_alert(screen_alert_tx: &Option<Sender<ScreenAlertCommand>>) -> bool {
    let Some(screen_alert_tx) = screen_alert_tx else {
        return false;
    };
    let (response_tx, response_rx) = oneshot::channel();
    match tokio::time::timeout(
        SCREEN_ALERT_RESPONSE_TIMEOUT,
        screen_alert_tx.send(ScreenAlertCommand::Acknowledge {
            response_tx: Some(response_tx),
        }),
    )
    .await
    {
        Ok(Ok(())) => {}
        _ => return false,
    }
    tokio::time::timeout(SCREEN_ALERT_RESPONSE_TIMEOUT, response_rx)
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or(false)
}

fn parse_event(input: [u8; INPUT_EVENT_SIZE]) -> Event {
    let event_type = u16::from_ne_bytes([input[8], input[9]]);
    if event_type != 1 {
        return Event::Other;
    }
    match i32::from_ne_bytes([input[12], input[13], input[14], input[15]]) {
        0 => Event::KeyUp,
        1 | 2 => Event::KeyDown,
        _ => Event::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[test]
    fn test_parse_event_keydown_m7350_v5() {
        let input = [
            0x57, 0x6c, 0x09, 0x00, 0x7c, 0xfb, 0x03, 0x00, 0x01, 0x00, 0x74, 0x00, 0x01, 0x00,
            0x00, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        assert!(matches!(parse_event(input), Event::KeyDown));
    }

    #[test]
    fn test_parse_event_keyup_m7350_v5() {
        let input = [
            0x57, 0x6c, 0x09, 0x00, 0x1b, 0x15, 0x05, 0x00, 0x01, 0x00, 0x74, 0x00, 0x00, 0x00,
            0x00, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        assert!(matches!(parse_event(input), Event::KeyUp));
    }

    #[test]
    fn ignores_non_key_events() {
        let input = [0u8; INPUT_EVENT_SIZE];
        assert!(matches!(parse_event(input), Event::Other));
    }

    #[tokio::test]
    async fn acknowledgement_press_cannot_restart_recording() {
        let (diag_tx, mut diag_rx) = mpsc::channel(2);
        let (alert_tx, mut alert_rx) = mpsc::channel(1);
        let alert_task = tokio::spawn(async move {
            let Some(ScreenAlertCommand::Acknowledge { response_tx }) = alert_rx.recv().await
            else {
                panic!("expected acknowledgement command");
            };
            response_tx.unwrap().send(true).unwrap();
        });
        let now = Instant::now();
        let mut state = InputState {
            last_keyup: Some(now - Duration::from_millis(300)),
            ..InputState::default()
        };

        process_event(
            Event::KeyDown,
            now,
            KeyInputMode::DoubleTapPower,
            &mut state,
            &diag_tx,
            &Some(alert_tx),
        )
        .await;
        process_event(
            Event::KeyUp,
            now + Duration::from_millis(25),
            KeyInputMode::DoubleTapPower,
            &mut state,
            &diag_tx,
            &None,
        )
        .await;

        alert_task.await.unwrap();
        assert!(diag_rx.try_recv().is_err());
        assert!(state.last_keyup.is_none());
        assert!(!state.consume_next_keyup);
    }

    #[tokio::test]
    async fn inactive_double_tap_behavior_is_unchanged() {
        let (diag_tx, mut diag_rx) = mpsc::channel(2);
        let now = Instant::now();
        let mut state = InputState::default();

        process_event(
            Event::KeyUp,
            now,
            KeyInputMode::DoubleTapPower,
            &mut state,
            &diag_tx,
            &None,
        )
        .await;
        process_event(
            Event::KeyUp,
            now + Duration::from_millis(200),
            KeyInputMode::DoubleTapPower,
            &mut state,
            &diag_tx,
            &None,
        )
        .await;

        assert!(matches!(
            diag_rx.try_recv().unwrap(),
            DiagDeviceCtrlMessage::StopRecording
        ));
        assert!(matches!(
            diag_rx.try_recv().unwrap(),
            DiagDeviceCtrlMessage::StartRecording { response_tx: None }
        ));
    }
}
