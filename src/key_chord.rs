use crate::app_state::KeyEvent;
use crate::keyboard::VirtualKey;
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModifierSideRequirement {
    NotRequired,
    Any,
    Left,
    Right,
}

impl Default for ModifierSideRequirement {
    fn default() -> Self {
        Self::NotRequired
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ModifierRequirements {
    pub ctrl: ModifierSideRequirement,
    pub alt: ModifierSideRequirement,
    pub shift: ModifierSideRequirement,
    pub win: ModifierSideRequirement,
}

impl ModifierRequirements {
    pub const fn new(
        ctrl: ModifierSideRequirement,
        alt: ModifierSideRequirement,
        shift: ModifierSideRequirement,
        win: ModifierSideRequirement,
    ) -> Self {
        Self {
            ctrl,
            alt,
            shift,
            win,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub key: VirtualKey,
    pub modifiers: ModifierRequirements,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ParsedChordModifiers {
    pub left_ctrl: bool,
    pub right_ctrl: bool,
    pub left_alt: bool,
    pub right_alt: bool,
    pub left_shift: bool,
    pub right_shift: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedKeyChord {
    pub chord: KeyChord,
    pub modifiers: ParsedChordModifiers,
}
impl KeyChord {
    pub fn parse(input: &str) -> Result<Self, KeyChordParseError> {
        Self::parse_with_details(input).map(|parsed| parsed.chord)
    }

    pub fn parse_with_details(input: &str) -> Result<ParsedKeyChord, KeyChordParseError> {
        let tokens: Vec<&str> = input.split('+').map(str::trim).collect();
        let mut key = None;
        let mut modifiers = ModifierRequirements::default();
        let mut parsed_modifiers = ParsedChordModifiers::default();

        for token in tokens.iter().copied() {
            if token.is_empty() {
                return Err(KeyChordParseError::new(
                    input,
                    "empty token; use forms like Ctrl+E or Escape",
                ));
            }

            if tokens.len() > 1 {
                if let Some((family, req)) = parse_modifier_token(token) {
                    set_modifier(input, family, req, &mut modifiers)?;
                    mark_parsed_modifier_side(family, req, &mut parsed_modifiers);
                    continue;
                }
            }

            let parsed_key = VirtualKey::from_string(token)
                .ok_or_else(|| KeyChordParseError::new(input, format!("invalid key token '{token}'")))?;

            if key.replace(parsed_key).is_some() {
                return Err(KeyChordParseError::new(
                    input,
                    format!("duplicate non-modifier token '{token}'"),
                ));
            }
        }

        let key =
            key.ok_or_else(|| KeyChordParseError::new(input, "missing non-modifier key token"))?;

        Ok(ParsedKeyChord {
            chord: Self { key, modifiers },
            modifiers: parsed_modifiers,
        })
    }

    pub fn from_key(key: VirtualKey) -> Self {
        Self {
            key,
            modifiers: ModifierRequirements::default(),
        }
    }

    pub fn specificity(&self) -> u8 {
        family_specificity(self.modifiers.ctrl)
            + family_specificity(self.modifiers.alt)
            + family_specificity(self.modifiers.shift)
            + family_specificity(self.modifiers.win)
    }

    pub fn display_label(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        append_modifier_label(&mut parts, "Ctrl", "LeftCtrl", "RightCtrl", self.modifiers.ctrl);
        append_modifier_label(&mut parts, "Alt", "LeftAlt", "RightAlt", self.modifiers.alt);
        append_modifier_label(&mut parts, "Shift", "LeftShift", "RightShift", self.modifiers.shift);
        append_modifier_label(&mut parts, "Win", "LeftWin", "RightWin", self.modifiers.win);
        parts.push(canonical_key_label(self.key));
        parts.join("+")
    }

    pub fn matches_ctrl(&self, event: &KeyEvent) -> bool {
        matches_family(self.modifiers.ctrl, event.ctrl_down, event.left_ctrl_down, event.right_ctrl_down, is_ctrl_key(self.key), key_is_left_ctrl(self.key), key_is_right_ctrl(self.key))
    }

    pub fn matches_alt(&self, event: &KeyEvent) -> bool {
        matches_family(self.modifiers.alt, event.alt_down, event.left_alt_down, event.right_alt_down, is_alt_key(self.key), key_is_left_alt(self.key), key_is_right_alt(self.key))
    }

    pub fn matches_shift(&self, event: &KeyEvent) -> bool {
        matches_family(self.modifiers.shift, event.shift_down, event.left_shift_down, event.right_shift_down, is_shift_key(self.key), key_is_left_shift(self.key), key_is_right_shift(self.key))
    }

    pub fn matches_win(&self, event: &KeyEvent) -> bool {
        matches_family(self.modifiers.win, event.win_down, event.left_win_down, event.right_win_down, is_win_key(self.key), key_is_left_win(self.key), key_is_right_win(self.key))
    }

    pub fn matches_event(&self, event: &KeyEvent) -> bool {
        self.key == event.key && self.matches_ctrl(event) && self.matches_alt(event) && self.matches_shift(event) && self.matches_win(event)
    }

    pub fn matches_event_ignoring_extra_modifiers(&self, event: &KeyEvent) -> bool {
        self.key == event.key
            && matches_family_ignoring_extra(self.modifiers.ctrl, event.ctrl_down, event.left_ctrl_down, event.right_ctrl_down, is_ctrl_key(self.key), key_is_left_ctrl(self.key), key_is_right_ctrl(self.key))
            && matches_family_ignoring_extra(self.modifiers.alt, event.alt_down, event.left_alt_down, event.right_alt_down, is_alt_key(self.key), key_is_left_alt(self.key), key_is_right_alt(self.key))
            && matches_family_ignoring_extra(self.modifiers.shift, event.shift_down, event.left_shift_down, event.right_shift_down, is_shift_key(self.key), key_is_left_shift(self.key), key_is_right_shift(self.key))
            && matches_family_ignoring_extra(self.modifiers.win, event.win_down, event.left_win_down, event.right_win_down, is_win_key(self.key), key_is_left_win(self.key), key_is_right_win(self.key))
    }

    pub fn matches_event_allowing_shift_modifier(&self, event: &KeyEvent) -> bool {
        self.key == event.key
            && self.is_unmodified()
            && event.shift_down
            && !event.ctrl_down
            && !event.alt_down
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
        self.modifiers == ModifierRequirements::default()
    }
}

fn family_specificity(req: ModifierSideRequirement) -> u8 { match req { ModifierSideRequirement::NotRequired => 0, ModifierSideRequirement::Any => 1, ModifierSideRequirement::Left | ModifierSideRequirement::Right => 2 } }
fn append_modifier_label(parts: &mut Vec<String>, any: &str, left: &str, right: &str, req: ModifierSideRequirement){ match req { ModifierSideRequirement::NotRequired=>{}, ModifierSideRequirement::Any=>parts.push(any.to_string()), ModifierSideRequirement::Left=>parts.push(left.to_string()), ModifierSideRequirement::Right=>parts.push(right.to_string())} }
fn canonical_key_label(key: VirtualKey) -> String {
    match key {
        VirtualKey::OemPlus => "Plus".to_string(),
        VirtualKey::OemComma => "Comma".to_string(),
        VirtualKey::OemMinus => "Minus".to_string(),
        VirtualKey::OemPeriod => "Period".to_string(),
        VirtualKey::Oem1 => "Semicolon".to_string(),
        VirtualKey::Oem2 => "Slash".to_string(),
        VirtualKey::Oem3 => "Backtick".to_string(),
        VirtualKey::Oem4 => "LeftBracket".to_string(),
        VirtualKey::Oem5 => "Backslash".to_string(),
        VirtualKey::Oem6 => "RightBracket".to_string(),
        VirtualKey::Oem7 => "Apostrophe".to_string(),
        _ => match key {
            VirtualKey::A => "A".to_string(),
            VirtualKey::B => "B".to_string(),
            VirtualKey::C => "C".to_string(),
            VirtualKey::D => "D".to_string(),
            VirtualKey::E => "E".to_string(),
            VirtualKey::F => "F".to_string(),
            VirtualKey::G => "G".to_string(),
            VirtualKey::H => "H".to_string(),
            VirtualKey::I => "I".to_string(),
            VirtualKey::J => "J".to_string(),
            VirtualKey::K => "K".to_string(),
            VirtualKey::L => "L".to_string(),
            VirtualKey::M => "M".to_string(),
            VirtualKey::N => "N".to_string(),
            VirtualKey::O => "O".to_string(),
            VirtualKey::P => "P".to_string(),
            VirtualKey::Q => "Q".to_string(),
            VirtualKey::R => "R".to_string(),
            VirtualKey::S => "S".to_string(),
            VirtualKey::T => "T".to_string(),
            VirtualKey::U => "U".to_string(),
            VirtualKey::V => "V".to_string(),
            VirtualKey::W => "W".to_string(),
            VirtualKey::X => "X".to_string(),
            VirtualKey::Y => "Y".to_string(),
            VirtualKey::Z => "Z".to_string(),
            _ => format!("{:?}", key),
        },
    }
}
fn matches_family(req: ModifierSideRequirement, any_down: bool, left_down: bool, right_down: bool, self_key: bool, self_left: bool, self_right: bool) -> bool { match req { ModifierSideRequirement::NotRequired => !any_down || self_key, ModifierSideRequirement::Any => any_down || self_key, ModifierSideRequirement::Left => left_down || self_left, ModifierSideRequirement::Right => right_down || self_right } }
fn matches_family_ignoring_extra(req: ModifierSideRequirement, any_down: bool, left_down: bool, right_down: bool, self_key: bool, self_left: bool, self_right: bool) -> bool { match req { ModifierSideRequirement::NotRequired => true, ModifierSideRequirement::Any => any_down || self_key, ModifierSideRequirement::Left => left_down || self_left, ModifierSideRequirement::Right => right_down || self_right } }

fn parse_modifier_token(token: &str) -> Option<(&'static str, ModifierSideRequirement)> {
    match token.to_ascii_uppercase().as_str() {
        "CTRL" | "CONTROL" => Some(("Ctrl", ModifierSideRequirement::Any)),
        "LEFTCTRL" | "LCTRL" | "LEFT_CTRL" => Some(("Ctrl", ModifierSideRequirement::Left)),
        "RIGHTCTRL" | "RCTRL" | "RIGHT_CTRL" => Some(("Ctrl", ModifierSideRequirement::Right)),
        "ALT" => Some(("Alt", ModifierSideRequirement::Any)),
        "LEFTALT" | "LALT" | "LEFT_ALT" => Some(("Alt", ModifierSideRequirement::Left)),
        "RIGHTALT" | "RALT" | "RIGHT_ALT" => Some(("Alt", ModifierSideRequirement::Right)),
        "SHIFT" => Some(("Shift", ModifierSideRequirement::Any)),
        "LEFTSHIFT" | "LSHIFT" | "LEFT_SHIFT" => Some(("Shift", ModifierSideRequirement::Left)),
        "RIGHTSHIFT" | "RSHIFT" | "RIGHT_SHIFT" => Some(("Shift", ModifierSideRequirement::Right)),
        "WIN" | "SUPER" | "META" => Some(("Win", ModifierSideRequirement::Any)),
        "LEFTWIN" | "LWIN" | "LEFT_WIN" => Some(("Win", ModifierSideRequirement::Left)),
        "RIGHTWIN" | "RWIN" | "RIGHT_WIN" => Some(("Win", ModifierSideRequirement::Right)),
        _ => None,
    }
}
fn set_modifier(input: &str, family: &str, requirement: ModifierSideRequirement, modifiers: &mut ModifierRequirements) -> Result<(), KeyChordParseError> { let slot = match family {"Ctrl"=> &mut modifiers.ctrl, "Alt"=> &mut modifiers.alt, "Shift"=> &mut modifiers.shift, "Win"=> &mut modifiers.win, _=>unreachable!()}; if *slot != ModifierSideRequirement::NotRequired { return Err(KeyChordParseError::new(input, format!("duplicate modifier '{family}'"))); } *slot = requirement; Ok(()) }
fn mark_parsed_modifier_side(family: &str, req: ModifierSideRequirement, m: &mut ParsedChordModifiers) { match (family, req) { ("Ctrl", ModifierSideRequirement::Left)=>m.left_ctrl=true, ("Ctrl", ModifierSideRequirement::Right)=>m.right_ctrl=true, ("Alt", ModifierSideRequirement::Left)=>m.left_alt=true, ("Alt", ModifierSideRequirement::Right)=>m.right_alt=true, ("Shift", ModifierSideRequirement::Left)=>m.left_shift=true, ("Shift", ModifierSideRequirement::Right)=>m.right_shift=true, _=>{} } }

fn is_shift_key(key: VirtualKey) -> bool { matches!(key, VirtualKey::Shift | VirtualKey::LeftShift | VirtualKey::RightShift) }
fn is_ctrl_key(key: VirtualKey) -> bool { matches!(key, VirtualKey::Ctrl | VirtualKey::LeftCtrl | VirtualKey::RightCtrl) }
fn is_alt_key(key: VirtualKey) -> bool { matches!(key, VirtualKey::Alt | VirtualKey::LeftAlt | VirtualKey::RightAlt) }
fn is_win_key(key: VirtualKey) -> bool { matches!(key, VirtualKey::LeftWin | VirtualKey::RightWin) }
fn key_is_left_ctrl(key: VirtualKey) -> bool { matches!(key, VirtualKey::LeftCtrl) }
fn key_is_right_ctrl(key: VirtualKey) -> bool { matches!(key, VirtualKey::RightCtrl) }
fn key_is_left_alt(key: VirtualKey) -> bool { matches!(key, VirtualKey::LeftAlt) }
fn key_is_right_alt(key: VirtualKey) -> bool { matches!(key, VirtualKey::RightAlt) }
fn key_is_left_shift(key: VirtualKey) -> bool { matches!(key, VirtualKey::LeftShift) }
fn key_is_right_shift(key: VirtualKey) -> bool { matches!(key, VirtualKey::RightShift) }
fn key_is_left_win(key: VirtualKey) -> bool { matches!(key, VirtualKey::LeftWin) }
fn key_is_right_win(key: VirtualKey) -> bool { matches!(key, VirtualKey::RightWin) }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeSystemBindings { pub toggle_active: KeyChord, pub exit: KeyChord, pub exit_ignore_extra_modifiers: bool, pub panic_reset: KeyChord, pub panic_reset_ignore_extra_modifiers: bool, pub panic_reset_sets_idle: bool }
impl RuntimeSystemBindings { #[allow(dead_code)] pub fn new(toggle_active: KeyChord, exit: KeyChord) -> Self { Self { toggle_active, exit, ..Self::default() } } }
impl Default for RuntimeSystemBindings { fn default() -> Self { Self { toggle_active: KeyChord { key: VirtualKey::E, modifiers: ModifierRequirements::new(ModifierSideRequirement::Any, ModifierSideRequirement::NotRequired, ModifierSideRequirement::NotRequired, ModifierSideRequirement::NotRequired)}, exit: KeyChord::from_key(VirtualKey::Escape), exit_ignore_extra_modifiers: true, panic_reset: KeyChord { key: VirtualKey::Escape, modifiers: ModifierRequirements::new(ModifierSideRequirement::NotRequired, ModifierSideRequirement::Right, ModifierSideRequirement::NotRequired, ModifierSideRequirement::NotRequired)}, panic_reset_ignore_extra_modifiers: true, panic_reset_sets_idle: true } } }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChordParseError { input: String, reason: String }
impl KeyChordParseError { fn new(input: impl Into<String>, reason: impl Into<String>) -> Self { Self { input: input.into(), reason: reason.into() } } }
impl fmt::Display for KeyChordParseError { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "invalid key chord '{}': {}", self.input, self.reason) } }
impl Error for KeyChordParseError {}

#[cfg(test)]
mod tests { use super::*;
fn chord(key: VirtualKey, ctrl: ModifierSideRequirement, alt: ModifierSideRequirement, shift: ModifierSideRequirement, win: ModifierSideRequirement) -> KeyChord { KeyChord { key, modifiers: ModifierRequirements::new(ctrl, alt, shift, win) } }
#[test] fn parse_side_and_generic_modifiers(){ let cases=[("Ctrl+E",chord(VirtualKey::E,ModifierSideRequirement::Any,ModifierSideRequirement::NotRequired,ModifierSideRequirement::NotRequired,ModifierSideRequirement::NotRequired)),("LeftCtrl+E",chord(VirtualKey::E,ModifierSideRequirement::Left,ModifierSideRequirement::NotRequired,ModifierSideRequirement::NotRequired,ModifierSideRequirement::NotRequired)),("RightCtrl+E",chord(VirtualKey::E,ModifierSideRequirement::Right,ModifierSideRequirement::NotRequired,ModifierSideRequirement::NotRequired,ModifierSideRequirement::NotRequired)),("Alt+E",chord(VirtualKey::E,ModifierSideRequirement::NotRequired,ModifierSideRequirement::Any,ModifierSideRequirement::NotRequired,ModifierSideRequirement::NotRequired)),("LeftAlt+E",chord(VirtualKey::E,ModifierSideRequirement::NotRequired,ModifierSideRequirement::Left,ModifierSideRequirement::NotRequired,ModifierSideRequirement::NotRequired)),("RightAlt+E",chord(VirtualKey::E,ModifierSideRequirement::NotRequired,ModifierSideRequirement::Right,ModifierSideRequirement::NotRequired,ModifierSideRequirement::NotRequired)),("Shift+E",chord(VirtualKey::E,ModifierSideRequirement::NotRequired,ModifierSideRequirement::NotRequired,ModifierSideRequirement::Any,ModifierSideRequirement::NotRequired)),("LeftShift+E",chord(VirtualKey::E,ModifierSideRequirement::NotRequired,ModifierSideRequirement::NotRequired,ModifierSideRequirement::Left,ModifierSideRequirement::NotRequired)),("RightShift+E",chord(VirtualKey::E,ModifierSideRequirement::NotRequired,ModifierSideRequirement::NotRequired,ModifierSideRequirement::Right,ModifierSideRequirement::NotRequired)),("Win+E",chord(VirtualKey::E,ModifierSideRequirement::NotRequired,ModifierSideRequirement::NotRequired,ModifierSideRequirement::NotRequired,ModifierSideRequirement::Any)),("LeftWin+E",chord(VirtualKey::E,ModifierSideRequirement::NotRequired,ModifierSideRequirement::NotRequired,ModifierSideRequirement::NotRequired,ModifierSideRequirement::Left)),("RightWin+E",chord(VirtualKey::E,ModifierSideRequirement::NotRequired,ModifierSideRequirement::NotRequired,ModifierSideRequirement::NotRequired,ModifierSideRequirement::Right))]; for (i,e) in cases { assert_eq!(KeyChord::parse(i).unwrap(),e,"{i}"); } }
#[test] fn standalone_side_modifier_is_key(){ assert_eq!(KeyChord::parse("LeftShift").unwrap(), KeyChord::from_key(VirtualKey::LeftShift)); }
#[test] fn matching_any_left_right(){ let any=KeyChord::parse("Alt+E").unwrap(); let left=KeyChord::parse("LeftAlt+E").unwrap(); let right=KeyChord::parse("RightAlt+E").unwrap(); let mut e=KeyEvent::new(VirtualKey::E,true); e.alt_down=true; e.left_alt_down=true; assert!(any.matches_event(&e)); assert!(left.matches_event(&e)); assert!(!right.matches_event(&e)); e.left_alt_down=false; e.right_alt_down=true; assert!(right.matches_event(&e)); }
#[test] fn specificity_ordering(){ assert!(KeyChord::parse("RightAlt+E").unwrap().specificity() > KeyChord::parse("Alt+E").unwrap().specificity()); assert!(KeyChord::parse("Shift+E").unwrap().specificity() > KeyChord::parse("E").unwrap().specificity()); }
#[test] fn display_roundtrip_matrix(){ let inputs=["E","Shift+E","RightAlt+E","LeftCtrl+RightShift+Q","Meta+Period","LeftShift"]; for input in inputs { let c=KeyChord::parse(input).unwrap(); let label=c.display_label(); assert_eq!(KeyChord::parse(&label).unwrap(), c, "{input} -> {label}"); } }
#[test] fn rejects_duplicate_conflicting_modifiers(){ assert!(KeyChord::parse("Ctrl+LeftCtrl+E").is_err()); assert!(KeyChord::parse("LeftAlt+RightAlt+E").is_err()); assert!(KeyChord::parse("E+E").is_err()); }
}
