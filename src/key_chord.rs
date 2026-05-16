use crate::app_state::KeyEvent;
use crate::keyboard::VirtualKey;
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyChord {
    pub key: VirtualKey,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

impl KeyChord {
    pub fn parse(input: &str) -> Result<Self, KeyChordParseError> {
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut win = false;
        let mut key = None;

        for raw_token in input.split('+') {
            let token = raw_token.trim();
            if token.is_empty() {
                return Err(KeyChordParseError::new(
                    input,
                    "empty token; use forms like Ctrl+E or Escape",
                ));
            }

            match token.to_ascii_uppercase().as_str() {
                "CTRL" | "CONTROL" => set_modifier(input, "Ctrl", &mut ctrl)?,
                "ALT" => set_modifier(input, "Alt", &mut alt)?,
                "SHIFT" => set_modifier(input, "Shift", &mut shift)?,
                "WIN" | "SUPER" | "META" => set_modifier(input, "Win", &mut win)?,
                _ => {
                    let parsed_key = VirtualKey::from_string(token).ok_or_else(|| {
                        KeyChordParseError::new(input, format!("invalid key token '{token}'"))
                    })?;

                    if key.replace(parsed_key).is_some() {
                        return Err(KeyChordParseError::new(
                            input,
                            format!("duplicate non-modifier token '{token}'"),
                        ));
                    }
                }
            }
        }

        let key =
            key.ok_or_else(|| KeyChordParseError::new(input, "missing non-modifier key token"))?;

        Ok(Self {
            key,
            ctrl,
            alt,
            shift,
            win,
        })
    }

    pub fn matches_event(&self, event: &KeyEvent) -> bool {
        self.key == event.key
            && self.ctrl == event.ctrl_down
            && self.alt == event.alt_down
            && self.shift == event.shift_down
            && self.win == event.win_down
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeSystemBindings {
    pub toggle_active: KeyChord,
    pub exit: KeyChord,
}

impl RuntimeSystemBindings {
    pub fn new(toggle_active: KeyChord, exit: KeyChord) -> Self {
        Self {
            toggle_active,
            exit,
        }
    }
}

impl Default for RuntimeSystemBindings {
    fn default() -> Self {
        Self {
            toggle_active: KeyChord {
                key: VirtualKey::E,
                ctrl: true,
                alt: false,
                shift: false,
                win: false,
            },
            exit: KeyChord {
                key: VirtualKey::Escape,
                ctrl: false,
                alt: false,
                shift: false,
                win: false,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChordParseError {
    input: String,
    reason: String,
}

impl KeyChordParseError {
    fn new(input: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            reason: reason.into(),
        }
    }
}

impl fmt::Display for KeyChordParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid key chord '{}': {}", self.input, self.reason)
    }
}

impl Error for KeyChordParseError {}

fn set_modifier(input: &str, modifier: &str, target: &mut bool) -> Result<(), KeyChordParseError> {
    if *target {
        return Err(KeyChordParseError::new(
            input,
            format!("duplicate modifier '{modifier}'"),
        ));
    }
    *target = true;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(key: VirtualKey, ctrl: bool, alt: bool, shift: bool, win: bool) -> KeyChord {
        KeyChord {
            key,
            ctrl,
            alt,
            shift,
            win,
        }
    }

    #[test]
    fn parses_supported_chords() {
        assert_eq!(
            KeyChord::parse("Ctrl+E").unwrap(),
            chord(VirtualKey::E, true, false, false, false)
        );
        assert_eq!(
            KeyChord::parse("Alt+E").unwrap(),
            chord(VirtualKey::E, false, true, false, false)
        );
        assert_eq!(
            KeyChord::parse("Ctrl+Shift+J").unwrap(),
            chord(VirtualKey::J, true, false, true, false)
        );
        assert_eq!(
            KeyChord::parse("Escape").unwrap(),
            chord(VirtualKey::Escape, false, false, false, false)
        );
    }

    #[test]
    fn parses_modifier_aliases() {
        assert_eq!(
            KeyChord::parse("Control+E").unwrap(),
            chord(VirtualKey::E, true, false, false, false)
        );
        assert_eq!(
            KeyChord::parse("Super+E").unwrap(),
            chord(VirtualKey::E, false, false, false, true)
        );
        assert_eq!(
            KeyChord::parse("Meta+E").unwrap(),
            chord(VirtualKey::E, false, false, false, true)
        );
    }

    #[test]
    fn parses_case_insensitively() {
        assert_eq!(
            KeyChord::parse("ctrl+shift+j").unwrap(),
            chord(VirtualKey::J, true, false, true, false)
        );
        assert_eq!(
            KeyChord::parse("eScApE").unwrap(),
            chord(VirtualKey::Escape, false, false, false, false)
        );
    }

    #[test]
    fn matches_exact_modifiers() {
        let chord = KeyChord::parse("Ctrl+E").unwrap();
        let mut event = KeyEvent::new(VirtualKey::E, true);
        event.ctrl_down = true;

        assert!(chord.matches_event(&event));
        assert!(!event.alt_down);
        assert!(!event.shift_down);
        assert!(!event.win_down);

        event.shift_down = true;
        assert!(!chord.matches_event(&event));
    }

    #[test]
    fn rejects_invalid_key_token() {
        assert!(KeyChord::parse("Ctrl+Nope").is_err());
    }

    #[test]
    fn rejects_duplicate_non_modifier_token() {
        assert!(KeyChord::parse("Ctrl+E+E").is_err());
    }

    #[test]
    fn rejects_modifier_only_chord() {
        assert!(KeyChord::parse("Ctrl+Shift").is_err());
    }
}
