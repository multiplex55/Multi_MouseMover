use crate::app_state::KeyEvent;
use crate::keyboard::VirtualKey;
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub key: VirtualKey,
    pub ctrl: bool,
    pub alt: bool,
    pub right_alt: bool,
    pub shift: bool,
    pub win: bool,
}

impl KeyChord {
    pub fn parse(input: &str) -> Result<Self, KeyChordParseError> {
        let mut ctrl = false;
        let mut alt = false;
        let mut right_alt = false;
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
                "RIGHTALT" | "RALT" | "RIGHT_ALT" => {
                    set_modifier(input, "RightAlt", &mut right_alt)?;
                    if !alt {
                        alt = true;
                    }
                }
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
            right_alt,
            shift,
            win,
        })
    }

    pub fn from_key(key: VirtualKey) -> Self {
        Self {
            key,
            ctrl: false,
            alt: false,
            right_alt: false,
            shift: false,
            win: false,
        }
    }

    pub fn specificity(&self) -> u8 {
        self.ctrl as u8 + self.alt as u8 + self.right_alt as u8 + self.shift as u8 + self.win as u8
    }

    pub fn matches_event(&self, event: &KeyEvent) -> bool {
        self.key == event.key
            && modifier_matches(self.ctrl, event.ctrl_down, is_ctrl_key(self.key))
            && modifier_matches(self.alt, event.alt_down, is_alt_key(self.key))
            && (!self.right_alt || event.right_alt_down)
            && modifier_matches(self.shift, event.shift_down, is_shift_key(self.key))
            && self.win == event.win_down
    }

    pub fn matches_event_allowing_shift_modifier(&self, event: &KeyEvent) -> bool {
        self.key == event.key
            && self.is_unmodified()
            && event.shift_down
            && !event.ctrl_down
            && !event.alt_down
            && !event.right_alt_down
            && !event.win_down
    }

    pub fn matches_key_down_event(&self, event: &KeyEvent) -> bool {
        self.matches_event(event) || self.matches_event_allowing_shift_modifier(event)
    }

    pub fn matches_dispatch_event(&self, event: &KeyEvent) -> bool {
        if event.is_down {
            self.matches_key_down_event(event)
        } else {
            self.matches_event(event)
        }
    }

    fn is_unmodified(&self) -> bool {
        !self.ctrl && !self.alt && !self.right_alt && !self.shift && !self.win
    }
}

fn modifier_matches(chord_modifier: bool, event_modifier: bool, self_key: bool) -> bool {
    chord_modifier == event_modifier || (!chord_modifier && event_modifier && self_key)
}

fn is_shift_key(key: VirtualKey) -> bool {
    matches!(
        key,
        VirtualKey::Shift | VirtualKey::LeftShift | VirtualKey::RightShift
    )
}

fn is_ctrl_key(key: VirtualKey) -> bool {
    matches!(
        key,
        VirtualKey::Ctrl | VirtualKey::LeftCtrl | VirtualKey::RightCtrl
    )
}

fn is_alt_key(key: VirtualKey) -> bool {
    matches!(
        key,
        VirtualKey::Alt | VirtualKey::LeftAlt | VirtualKey::RightAlt
    )
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
                right_alt: false,
                shift: false,
                win: false,
            },
            exit: KeyChord {
                key: VirtualKey::Escape,
                ctrl: false,
                alt: false,
                right_alt: false,
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

    fn chord(
        key: VirtualKey,
        ctrl: bool,
        alt: bool,
        right_alt: bool,
        shift: bool,
        win: bool,
    ) -> KeyChord {
        KeyChord {
            key,
            ctrl,
            alt,
            right_alt,
            shift,
            win,
        }
    }

    #[test]
    fn parses_supported_chords() {
        assert_eq!(
            KeyChord::parse("Ctrl+E").unwrap(),
            chord(VirtualKey::E, true, false, false, false, false)
        );
        assert_eq!(
            KeyChord::parse("Alt+E").unwrap(),
            chord(VirtualKey::E, false, true, false, false, false)
        );
        assert_eq!(
            KeyChord::parse("RightAlt+E").unwrap(),
            chord(VirtualKey::E, false, true, true, false, false)
        );
        assert_eq!(
            KeyChord::parse("Ctrl+Shift+J").unwrap(),
            chord(VirtualKey::J, true, false, false, true, false)
        );
        assert_eq!(
            KeyChord::parse("Escape").unwrap(),
            chord(VirtualKey::Escape, false, false, false, false, false)
        );
    }

    #[test]
    fn parses_modifier_aliases() {
        assert_eq!(
            KeyChord::parse("Control+E").unwrap(),
            chord(VirtualKey::E, true, false, false, false, false)
        );
        assert_eq!(
            KeyChord::parse("Super+E").unwrap(),
            chord(VirtualKey::E, false, false, false, false, true)
        );
        assert_eq!(
            KeyChord::parse("Meta+E").unwrap(),
            chord(VirtualKey::E, false, false, false, false, true)
        );
        assert_eq!(
            KeyChord::parse("RAlt+E").unwrap(),
            chord(VirtualKey::E, false, true, true, false, false)
        );
        assert_eq!(
            KeyChord::parse("RIGHT_ALT+E").unwrap(),
            chord(VirtualKey::E, false, true, true, false, false)
        );
    }

    #[test]
    fn parses_case_insensitively() {
        assert_eq!(
            KeyChord::parse("ctrl+shift+j").unwrap(),
            chord(VirtualKey::J, true, false, false, true, false)
        );
        assert_eq!(
            KeyChord::parse("eScApE").unwrap(),
            chord(VirtualKey::Escape, false, false, false, false, false)
        );
    }

    #[test]
    fn parses_punctuation_key_aliases_in_chords() {
        let cases = [
            ("Ctrl+;", VirtualKey::Oem1),
            ("Alt+Semicolon", VirtualKey::Oem1),
            ("Shift+Comma", VirtualKey::OemComma),
            ("Win+Period", VirtualKey::OemPeriod),
            ("Meta+Dot", VirtualKey::OemPeriod),
            ("Ctrl+Forward_Slash", VirtualKey::Oem2),
            ("Alt+Backtick", VirtualKey::Oem3),
            ("Shift+Left_Bracket", VirtualKey::Oem4),
            ("Ctrl+Backslash", VirtualKey::Oem5),
            ("Alt+Right_Bracket", VirtualKey::Oem6),
            ("Shift+Apostrophe", VirtualKey::Oem7),
        ];

        for (input, expected_key) in cases {
            assert_eq!(KeyChord::parse(input).unwrap().key, expected_key, "{input}");
        }
    }

    #[test]
    fn parses_modifier_aliases_table_driven() {
        let cases = [
            (
                "Ctrl+E",
                chord(VirtualKey::E, true, false, false, false, false),
            ),
            (
                "Control+E",
                chord(VirtualKey::E, true, false, false, false, false),
            ),
            (
                "Alt+E",
                chord(VirtualKey::E, false, true, false, false, false),
            ),
            (
                "RightAlt+E",
                chord(VirtualKey::E, false, true, true, false, false),
            ),
            (
                "RAlt+E",
                chord(VirtualKey::E, false, true, true, false, false),
            ),
            (
                "RIGHT_ALT+E",
                chord(VirtualKey::E, false, true, true, false, false),
            ),
            (
                "Shift+E",
                chord(VirtualKey::E, false, false, false, true, false),
            ),
            (
                "Win+E",
                chord(VirtualKey::E, false, false, false, false, true),
            ),
            (
                "Super+E",
                chord(VirtualKey::E, false, false, false, false, true),
            ),
            (
                "Meta+E",
                chord(VirtualKey::E, false, false, false, false, true),
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(KeyChord::parse(input).unwrap(), expected, "{input}");
        }
    }

    #[test]
    fn matches_exact_modifiers() {
        let chord = KeyChord::parse("Ctrl+E").unwrap();
        let mut event = KeyEvent::new(VirtualKey::E, true);
        event.ctrl_down = true;

        assert!(chord.matches_event(&event));
        assert!(!event.alt_down);
        assert!(!event.right_alt_down);
        assert!(!event.shift_down);
        assert!(!event.win_down);

        event.shift_down = true;
        assert!(!chord.matches_event(&event));
    }

    #[test]
    fn modifier_self_keys_match_their_own_down_modifier_state() {
        let cases = [
            (VirtualKey::LeftShift, false, false, false, true),
            (VirtualKey::RightShift, false, false, false, true),
            (VirtualKey::LeftCtrl, true, false, false, false),
            (VirtualKey::RightCtrl, true, false, false, false),
            (VirtualKey::LeftAlt, false, true, false, false),
            (VirtualKey::RightAlt, false, true, true, false),
        ];

        for (key, ctrl_down, alt_down, right_alt_down, shift_down) in cases {
            let chord = KeyChord::from_key(key);
            let mut event = KeyEvent::new(key, true);
            event.ctrl_down = ctrl_down;
            event.alt_down = alt_down;
            event.right_alt_down = right_alt_down;
            event.shift_down = shift_down;

            assert!(chord.matches_event(&event), "{key:?}");
        }
    }

    #[test]
    fn shift_relaxed_match_only_accepts_plain_chord_with_extra_shift() {
        let plain_w = KeyChord::parse("W").unwrap();
        let shift_w = KeyChord::parse("Shift+W").unwrap();
        let mut event = KeyEvent::new(VirtualKey::W, true);
        event.shift_down = true;

        assert!(plain_w.matches_event_allowing_shift_modifier(&event));
        assert!(!shift_w.matches_event_allowing_shift_modifier(&event));

        event.ctrl_down = true;
        assert!(!plain_w.matches_event_allowing_shift_modifier(&event));

        event.ctrl_down = false;
        event.alt_down = true;
        event.right_alt_down = true;
        assert!(!plain_w.matches_event_allowing_shift_modifier(&event));
    }

    #[test]
    fn right_alt_matches_more_specifically_than_alt_and_plain_key() {
        let right_alt_w = KeyChord::parse("RightAlt+W").unwrap();
        let alt_w = KeyChord::parse("Alt+W").unwrap();
        let plain_w = KeyChord::parse("W").unwrap();

        let mut event = KeyEvent::new(VirtualKey::W, true);
        event.alt_down = true;
        event.right_alt_down = true;

        assert!(right_alt_w.matches_event(&event));
        assert!(alt_w.matches_event(&event));
        assert!(!plain_w.matches_event(&event));

        event.right_alt_down = false;
        assert!(!right_alt_w.matches_event(&event));
        assert!(alt_w.matches_event(&event));
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
