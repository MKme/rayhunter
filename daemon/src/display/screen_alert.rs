use super::bitmap_font::glyph;
use crate::config::DEFAULT_SCREEN_ALERT_MESSAGE;
use crate::display::DisplayState;

const GLYPH_WIDTH: usize = 5;
const GLYPH_HEIGHT: usize = 7;
const SCALE: usize = 2;
const CHARACTER_GAP: usize = 2;
const LINE_GAP: usize = 1;
const MAX_COLUMNS: usize = 10;
const MAX_LINES: usize = 8;
pub const MAX_MESSAGE_CHARACTERS: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlashPhase {
    Warning,
    Dark,
}

pub struct ScreenAlertController {
    enabled: bool,
    active: bool,
    phase: FlashPhase,
}

impl ScreenAlertController {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            active: false,
            phase: FlashPhase::Warning,
        }
    }

    pub fn handle_display_state(&mut self, state: DisplayState) {
        if self.enabled && matches!(state, DisplayState::WarningDetected { .. }) {
            self.activate();
        }
    }

    pub fn test(&mut self) {
        self.activate();
    }

    fn activate(&mut self) {
        self.active = true;
        self.phase = FlashPhase::Warning;
    }

    pub fn acknowledge(&mut self) -> bool {
        let was_active = self.active;
        self.active = false;
        self.phase = FlashPhase::Warning;
        was_active
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn phase(&self) -> FlashPhase {
        self.phase
    }

    pub fn advance_phase(&mut self) {
        self.phase = match self.phase {
            FlashPhase::Warning => FlashPhase::Dark,
            FlashPhase::Dark => FlashPhase::Warning,
        };
    }
}

pub fn validate_message(message: &str) -> Result<Vec<String>, String> {
    if message.chars().count() > MAX_MESSAGE_CHARACTERS {
        return Err(format!(
            "screen alert message must be {MAX_MESSAGE_CHARACTERS} characters or fewer"
        ));
    }

    let normalized = message
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase();
    if normalized.is_empty() {
        return Err("screen alert message cannot be empty".to_string());
    }
    if let Some(character) = normalized
        .chars()
        .find(|character| !is_supported(*character))
    {
        return Err(format!(
            "screen alert message contains unsupported character {character:?}"
        ));
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    for word in normalized.split(' ') {
        if word.len() > MAX_COLUMNS {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            let mut remainder = word;
            while remainder.len() > MAX_COLUMNS {
                let (line, rest) = remainder.split_at(MAX_COLUMNS);
                lines.push(line.to_string());
                remainder = rest;
            }
            if !remainder.is_empty() {
                current.push_str(remainder);
            }
        } else if current.is_empty() {
            current.push_str(word);
        } else if current.len() + 1 + word.len() <= MAX_COLUMNS {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }

    if lines.len() > MAX_LINES {
        return Err(format!(
            "screen alert message wraps to more than {MAX_LINES} lines"
        ));
    }
    Ok(lines)
}

pub fn render_message(
    width: usize,
    height: usize,
    message: &str,
    phase: FlashPhase,
) -> Vec<(u8, u8, u8)> {
    let lines = validate_message(message)
        .or_else(|_| validate_message(DEFAULT_SCREEN_ALERT_MESSAGE))
        .expect("default screen alert message must be renderable");
    let (background, foreground) = match phase {
        FlashPhase::Warning => ((0xff, 0, 0), (0xff, 0xff, 0xff)),
        FlashPhase::Dark => ((0, 0, 0), (0xff, 0, 0)),
    };
    let mut pixels = vec![background; width * height];
    let glyph_width = GLYPH_WIDTH * SCALE;
    let glyph_height = GLYPH_HEIGHT * SCALE;
    let cell_width = glyph_width + CHARACTER_GAP;
    let line_height = glyph_height + LINE_GAP;
    let text_height = lines.len() * line_height - LINE_GAP;
    let start_y = height.saturating_sub(text_height) / 2;

    for (line_index, line) in lines.iter().enumerate() {
        let line_width = line.len() * cell_width - CHARACTER_GAP;
        let start_x = width.saturating_sub(line_width) / 2;
        let y = start_y + line_index * line_height;
        for (character_index, character) in line.chars().enumerate() {
            draw_glyph(
                &mut pixels,
                width,
                height,
                start_x + character_index * cell_width,
                y,
                glyph(character),
                foreground,
            );
        }
    }
    pixels
}

fn draw_glyph(
    pixels: &mut [(u8, u8, u8)],
    width: usize,
    height: usize,
    origin_x: usize,
    origin_y: usize,
    rows: [u8; GLYPH_HEIGHT],
    color: (u8, u8, u8),
) {
    for (row_index, row) in rows.into_iter().enumerate() {
        for column in 0..GLYPH_WIDTH {
            if row & (1 << (GLYPH_WIDTH - 1 - column)) == 0 {
                continue;
            }
            for scale_y in 0..SCALE {
                for scale_x in 0..SCALE {
                    let x = origin_x + column * SCALE + scale_x;
                    let y = origin_y + row_index * SCALE + scale_y;
                    if x < width && y < height {
                        pixels[y * width + x] = color;
                    }
                }
            }
        }
    }
}

fn is_supported(character: char) -> bool {
    character == ' '
        || character.is_ascii_uppercase()
        || character.is_ascii_digit()
        || matches!(character, '-' | '.' | ',' | '!' | '?' | ':' | '/' | '\'')
}

#[cfg(test)]
mod tests {
    use super::*;
    use rayhunter::analysis::analyzer::EventType;

    #[test]
    fn default_message_fits_the_orbic_display() {
        let lines = validate_message(DEFAULT_SCREEN_ALERT_MESSAGE).unwrap();
        assert!(lines.len() <= MAX_LINES);
        assert!(lines.iter().all(|line| line.len() <= MAX_COLUMNS));
    }

    #[test]
    fn rejects_messages_that_cannot_be_rendered_safely() {
        assert!(validate_message("   ").is_err());
        assert!(validate_message("warning 🚨").is_err());
        assert!(validate_message(&"A".repeat(MAX_MESSAGE_CHARACTERS + 1)).is_err());
        assert!(validate_message(&["AAAAAA"; MAX_LINES + 1].join(" ")).is_err());
        // Six-character words cannot share a ten-character line. The old
        // fixture wrapped to exactly eight lines, which is valid, not overflow.
        assert_eq!(
            validate_message(&["AAAAAA"; MAX_LINES].join(" "))
                .unwrap()
                .len(),
            MAX_LINES
        );
    }

    #[test]
    fn renderer_fills_the_frame_and_keeps_text_visible_in_both_phases() {
        for phase in [FlashPhase::Warning, FlashPhase::Dark] {
            let pixels = render_message(128, 128, "TEST ALERT", phase);
            assert_eq!(pixels.len(), 128 * 128);
            let background = pixels[0];
            assert!(pixels.iter().any(|pixel| *pixel != background));
        }
    }

    #[test]
    fn warning_and_test_latch_until_acknowledged() {
        let warning = DisplayState::WarningDetected {
            event_type: EventType::High,
        };
        let mut enabled = ScreenAlertController::new(true);
        enabled.handle_display_state(warning);
        assert!(enabled.is_active());
        enabled.advance_phase();
        assert_eq!(enabled.phase(), FlashPhase::Dark);
        assert!(enabled.acknowledge());
        assert!(!enabled.is_active());
        assert!(!enabled.acknowledge());

        let mut disabled = ScreenAlertController::new(false);
        disabled.handle_display_state(warning);
        assert!(!disabled.is_active());
        disabled.test();
        assert!(disabled.is_active());
    }
}
