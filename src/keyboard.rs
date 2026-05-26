use crate::action::Action;
use crate::app_state::KeyEvent;
use crate::key_chord::KeyChord;
use std::collections::HashSet;
use std::mem::size_of;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY,
};

const LLKHF_INJECTED_BITS: u32 = 0x10;
const LLKHF_LOWER_IL_INJECTED_BITS: u32 = 0x02;

/// Enum representing virtual key codes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VirtualKey {
    // Function keys
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,

    // Alphabet keys
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,

    // Number keys
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,

    // Numpad keys
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadMultiply,
    NumpadAdd,
    NumpadSeparator,
    NumpadSubtract,
    NumpadDot,
    NumpadDivide,

    // Arrow keys
    Up,
    Down,
    Left,
    Right,

    // Special keys
    Backspace,
    Tab,
    Enter,
    Shift,
    Ctrl,
    Alt,
    Pause,
    CapsLock,
    Escape,
    Space,
    PageUp,
    PageDown,
    End,
    Home,
    Insert,
    Delete,

    // Symbols
    OemPlus,
    OemComma,
    OemMinus,
    OemPeriod,
    Oem1,
    Oem2,
    Oem3,
    Oem4,
    Oem5,
    Oem6,
    Oem7,

    // Additional keys
    PrintScreen,
    ScrollLock,
    NumLock,
    LeftShift,
    RightShift,
    LeftCtrl,
    RightCtrl,
    LeftAlt,
    RightAlt,
}

impl VirtualKey {
    /// Convert a string to a `VirtualKey` enum
    pub fn from_string(key: &str) -> Option<Self> {
        match key.to_uppercase().as_str() {
            // Function keys
            "F1" => Some(Self::F1),
            "F2" => Some(Self::F2),
            "F3" => Some(Self::F3),
            "F4" => Some(Self::F4),
            "F5" => Some(Self::F5),
            "F6" => Some(Self::F6),
            "F7" => Some(Self::F7),
            "F8" => Some(Self::F8),
            "F9" => Some(Self::F9),
            "F10" => Some(Self::F10),
            "F11" => Some(Self::F11),
            "F12" => Some(Self::F12),
            "F13" => Some(Self::F13),
            "F14" => Some(Self::F14),
            "F15" => Some(Self::F15),
            "F16" => Some(Self::F16),
            "F17" => Some(Self::F17),
            "F18" => Some(Self::F18),
            "F19" => Some(Self::F19),
            "F20" => Some(Self::F20),
            "F21" => Some(Self::F21),
            "F22" => Some(Self::F22),
            "F23" => Some(Self::F23),
            "F24" => Some(Self::F24),

            // Alphabet keys
            "A" => Some(Self::A),
            "B" => Some(Self::B),
            "C" => Some(Self::C),
            "D" => Some(Self::D),
            "E" => Some(Self::E),
            "F" => Some(Self::F),
            "G" => Some(Self::G),
            "H" => Some(Self::H),
            "I" => Some(Self::I),
            "J" => Some(Self::J),
            "K" => Some(Self::K),
            "L" => Some(Self::L),
            "M" => Some(Self::M),
            "N" => Some(Self::N),
            "O" => Some(Self::O),
            "P" => Some(Self::P),
            "Q" => Some(Self::Q),
            "R" => Some(Self::R),
            "S" => Some(Self::S),
            "T" => Some(Self::T),
            "U" => Some(Self::U),
            "V" => Some(Self::V),
            "W" => Some(Self::W),
            "X" => Some(Self::X),
            "Y" => Some(Self::Y),
            "Z" => Some(Self::Z),

            // Number keys
            "0" => Some(Self::Num0),
            "1" => Some(Self::Num1),
            "2" => Some(Self::Num2),
            "3" => Some(Self::Num3),
            "4" => Some(Self::Num4),
            "5" => Some(Self::Num5),
            "6" => Some(Self::Num6),
            "7" => Some(Self::Num7),
            "8" => Some(Self::Num8),
            "9" => Some(Self::Num9),

            // Numpad keys
            "NUMPAD0" => Some(Self::Numpad0),
            "NUMPAD1" => Some(Self::Numpad1),
            "NUMPAD2" => Some(Self::Numpad2),
            "NUMPAD3" => Some(Self::Numpad3),
            "NUMPAD4" => Some(Self::Numpad4),
            "NUMPAD5" => Some(Self::Numpad5),
            "NUMPAD6" => Some(Self::Numpad6),
            "NUMPAD7" => Some(Self::Numpad7),
            "NUMPAD8" => Some(Self::Numpad8),
            "NUMPAD9" => Some(Self::Numpad9),
            "NUMPADMULTIPLY" => Some(Self::NumpadMultiply),
            "NUMPADADD" => Some(Self::NumpadAdd),
            "NUMPADSEPARATOR" => Some(Self::NumpadSeparator),
            "NUMPADSUBTRACT" => Some(Self::NumpadSubtract),
            "NUMPADDOT" => Some(Self::NumpadDot),
            "NUMPADDIVIDE" => Some(Self::NumpadDivide),

            // Arrow keys
            "UP" => Some(Self::Up),
            "DOWN" => Some(Self::Down),
            "LEFT" => Some(Self::Left),
            "RIGHT" => Some(Self::Right),

            // Special keys
            "BACKSPACE" => Some(Self::Backspace),
            "TAB" => Some(Self::Tab),
            "ENTER" => Some(Self::Enter),
            "SHIFT" => Some(Self::Shift),
            "CTRL" => Some(Self::Ctrl),
            "ALT" => Some(Self::Alt),
            "PAUSE" => Some(Self::Pause),
            "CAPSLOCK" => Some(Self::CapsLock),
            "ESCAPE" => Some(Self::Escape),
            "SPACE" => Some(Self::Space),
            "PAGEUP" => Some(Self::PageUp),
            "PAGEDOWN" => Some(Self::PageDown),
            "END" => Some(Self::End),
            "HOME" => Some(Self::Home),
            "INSERT" => Some(Self::Insert),
            "DELETE" => Some(Self::Delete),

            // Symbols
            "+" | "=" | "PLUS" | "EQUALS" | "OEM_PLUS" => Some(Self::OemPlus),
            "," | "COMMA" | "OEM_COMMA" => Some(Self::OemComma),
            "-" | "MINUS" | "OEM_MINUS" => Some(Self::OemMinus),
            "." | "PERIOD" | "DOT" | "OEM_PERIOD" => Some(Self::OemPeriod),
            ";" | "SEMICOLON" | "OEM_1" => Some(Self::Oem1),
            "/" | "SLASH" | "FORWARD_SLASH" | "OEM_2" => Some(Self::Oem2),
            "`" | "BACKTICK" | "GRAVE" | "OEM_3" => Some(Self::Oem3),
            "[" | "LEFT_BRACKET" | "LBRACKET" | "OEM_4" => Some(Self::Oem4),
            "\\" | "BACKSLASH" | "OEM_5" => Some(Self::Oem5),
            "]" | "RIGHT_BRACKET" | "RBRACKET" | "OEM_6" => Some(Self::Oem6),
            "'" | "QUOTE" | "APOSTROPHE" | "OEM_7" => Some(Self::Oem7),

            // Additional keys
            "PRINTSCREEN" => Some(Self::PrintScreen),
            "SCROLLLOCK" => Some(Self::ScrollLock),
            "NUMLOCK" => Some(Self::NumLock),
            "LEFTSHIFT" => Some(Self::LeftShift),
            "RIGHTSHIFT" => Some(Self::RightShift),
            "LEFTCTRL" => Some(Self::LeftCtrl),
            "RIGHTCTRL" => Some(Self::RightCtrl),
            "LEFTALT" => Some(Self::LeftAlt),
            "RIGHTALT" | "RIGHT_ALT" | "RALT" => Some(Self::RightAlt),

            _ => None,
        }
    }

    /// Convert a `VirtualKey` to its virtual key code
    #[allow(dead_code)]
    pub fn to_vk_code(self) -> u32 {
        match self {
            // Function keys
            Self::F1 => 0x70,
            Self::F2 => 0x71,
            Self::F3 => 0x72,
            Self::F4 => 0x73,
            Self::F5 => 0x74,
            Self::F6 => 0x75,
            Self::F7 => 0x76,
            Self::F8 => 0x77,
            Self::F9 => 0x78,
            Self::F10 => 0x79,
            Self::F11 => 0x7A,
            Self::F12 => 0x7B,
            Self::F13 => 0x7C,
            Self::F14 => 0x7D,
            Self::F15 => 0x7E,
            Self::F16 => 0x7F,
            Self::F17 => 0x80,
            Self::F18 => 0x81,
            Self::F19 => 0x82,
            Self::F20 => 0x83,
            Self::F21 => 0x84,
            Self::F22 => 0x85,
            Self::F23 => 0x86,
            Self::F24 => 0x87,

            // Alphabet keys
            Self::A => 0x41,
            Self::B => 0x42,
            Self::C => 0x43,
            Self::D => 0x44,
            Self::E => 0x45,
            Self::F => 0x46,
            Self::G => 0x47,
            Self::H => 0x48,
            Self::I => 0x49,
            Self::J => 0x4A,
            Self::K => 0x4B,
            Self::L => 0x4C,
            Self::M => 0x4D,
            Self::N => 0x4E,
            Self::O => 0x4F,
            Self::P => 0x50,
            Self::Q => 0x51,
            Self::R => 0x52,
            Self::S => 0x53,
            Self::T => 0x54,
            Self::U => 0x55,
            Self::V => 0x56,
            Self::W => 0x57,
            Self::X => 0x58,
            Self::Y => 0x59,
            Self::Z => 0x5A,

            // Number keys
            Self::Num0 => 0x30,
            Self::Num1 => 0x31,
            Self::Num2 => 0x32,
            Self::Num3 => 0x33,
            Self::Num4 => 0x34,
            Self::Num5 => 0x35,
            Self::Num6 => 0x36,
            Self::Num7 => 0x37,
            Self::Num8 => 0x38,
            Self::Num9 => 0x39,

            // Numpad keys
            Self::Numpad0 => 0x60,
            Self::Numpad1 => 0x61,
            Self::Numpad2 => 0x62,
            Self::Numpad3 => 0x63,
            Self::Numpad4 => 0x64,
            Self::Numpad5 => 0x65,
            Self::Numpad6 => 0x66,
            Self::Numpad7 => 0x67,
            Self::Numpad8 => 0x68,
            Self::Numpad9 => 0x69,
            Self::NumpadMultiply => 0x6A,
            Self::NumpadAdd => 0x6B,
            Self::NumpadSeparator => 0x6C,
            Self::NumpadSubtract => 0x6D,
            Self::NumpadDot => 0x6E,
            Self::NumpadDivide => 0x6F,

            // Arrow keys
            Self::Up => 0x26,
            Self::Down => 0x28,
            Self::Left => 0x25,
            Self::Right => 0x27,

            // Special keys
            Self::Backspace => 0x08,
            Self::Tab => 0x09,
            Self::Enter => 0x0D,
            Self::Shift => 0x10,
            Self::Ctrl => 0x11,
            Self::Alt => 0x12,
            Self::Pause => 0x13,
            Self::CapsLock => 0x14,
            Self::Escape => 0x1B,
            Self::Space => 0x20,
            Self::PageUp => 0x21,
            Self::PageDown => 0x22,
            Self::End => 0x23,
            Self::Home => 0x24,
            Self::Insert => 0x2D,
            Self::Delete => 0x2E,

            // Symbols
            Self::OemPlus => 0xBB,
            Self::OemComma => 0xBC,
            Self::OemMinus => 0xBD,
            Self::OemPeriod => 0xBE,
            Self::Oem1 => 0xBA,
            Self::Oem2 => 0xBF,
            Self::Oem3 => 0xC0,
            Self::Oem4 => 0xDB,
            Self::Oem5 => 0xDC,
            Self::Oem6 => 0xDD,
            Self::Oem7 => 0xDE,

            // Additional keys
            Self::PrintScreen => 0x2C,
            Self::ScrollLock => 0x91,
            Self::NumLock => 0x90,
            Self::LeftShift => 0xA0,
            Self::RightShift => 0xA1,
            Self::LeftCtrl => 0xA2,
            Self::RightCtrl => 0xA3,
            Self::LeftAlt => 0xA4,
            Self::RightAlt => 0xA5,
        }
    }

    /// Convert a virtual key code to a `VirtualKey` enum
    pub fn from_vk_code(vk_code: u32) -> Option<Self> {
        match vk_code {
            // Function keys
            0x70 => Some(Self::F1),
            0x71 => Some(Self::F2),
            0x72 => Some(Self::F3),
            0x73 => Some(Self::F4),
            0x74 => Some(Self::F5),
            0x75 => Some(Self::F6),
            0x76 => Some(Self::F7),
            0x77 => Some(Self::F8),
            0x78 => Some(Self::F9),
            0x79 => Some(Self::F10),
            0x7A => Some(Self::F11),
            0x7B => Some(Self::F12),
            0x7C => Some(Self::F13),
            0x7D => Some(Self::F14),
            0x7E => Some(Self::F15),
            0x7F => Some(Self::F16),
            0x80 => Some(Self::F17),
            0x81 => Some(Self::F18),
            0x82 => Some(Self::F19),
            0x83 => Some(Self::F20),
            0x84 => Some(Self::F21),
            0x85 => Some(Self::F22),
            0x86 => Some(Self::F23),
            0x87 => Some(Self::F24),

            // Alphabet keys
            0x41 => Some(Self::A),
            0x42 => Some(Self::B),
            0x43 => Some(Self::C),
            0x44 => Some(Self::D),
            0x45 => Some(Self::E),
            0x46 => Some(Self::F),
            0x47 => Some(Self::G),
            0x48 => Some(Self::H),
            0x49 => Some(Self::I),
            0x4A => Some(Self::J),
            0x4B => Some(Self::K),
            0x4C => Some(Self::L),
            0x4D => Some(Self::M),
            0x4E => Some(Self::N),
            0x4F => Some(Self::O),
            0x50 => Some(Self::P),
            0x51 => Some(Self::Q),
            0x52 => Some(Self::R),
            0x53 => Some(Self::S),
            0x54 => Some(Self::T),
            0x55 => Some(Self::U),
            0x56 => Some(Self::V),
            0x57 => Some(Self::W),
            0x58 => Some(Self::X),
            0x59 => Some(Self::Y),
            0x5A => Some(Self::Z),

            // Number keys
            0x30 => Some(Self::Num0),
            0x31 => Some(Self::Num1),
            0x32 => Some(Self::Num2),
            0x33 => Some(Self::Num3),
            0x34 => Some(Self::Num4),
            0x35 => Some(Self::Num5),
            0x36 => Some(Self::Num6),
            0x37 => Some(Self::Num7),
            0x38 => Some(Self::Num8),
            0x39 => Some(Self::Num9),

            // Numpad keys
            0x60 => Some(Self::Numpad0),
            0x61 => Some(Self::Numpad1),
            0x62 => Some(Self::Numpad2),
            0x63 => Some(Self::Numpad3),
            0x64 => Some(Self::Numpad4),
            0x65 => Some(Self::Numpad5),
            0x66 => Some(Self::Numpad6),
            0x67 => Some(Self::Numpad7),
            0x68 => Some(Self::Numpad8),
            0x69 => Some(Self::Numpad9),
            0x6A => Some(Self::NumpadMultiply),
            0x6B => Some(Self::NumpadAdd),
            0x6C => Some(Self::NumpadSeparator),
            0x6D => Some(Self::NumpadSubtract),
            0x6E => Some(Self::NumpadDot),
            0x6F => Some(Self::NumpadDivide),

            // Arrow keys
            0x26 => Some(Self::Up),
            0x28 => Some(Self::Down),
            0x25 => Some(Self::Left),
            0x27 => Some(Self::Right),

            // Special keys
            0x08 => Some(Self::Backspace),
            0x09 => Some(Self::Tab),
            0x0D => Some(Self::Enter),
            0x10 => Some(Self::Shift),
            0x11 => Some(Self::Ctrl),
            0x12 => Some(Self::Alt),
            0x13 => Some(Self::Pause),
            0x14 => Some(Self::CapsLock),
            0x1B => Some(Self::Escape),
            0x20 => Some(Self::Space),
            0x21 => Some(Self::PageUp),
            0x22 => Some(Self::PageDown),
            0x23 => Some(Self::End),
            0x24 => Some(Self::Home),
            0x2D => Some(Self::Insert),
            0x2E => Some(Self::Delete),

            // Symbols
            0xBB => Some(Self::OemPlus),
            0xBC => Some(Self::OemComma),
            0xBD => Some(Self::OemMinus),
            0xBE => Some(Self::OemPeriod),
            0xBA => Some(Self::Oem1),
            0xBF => Some(Self::Oem2),
            0xC0 => Some(Self::Oem3),
            0xDB => Some(Self::Oem4),
            0xDC => Some(Self::Oem5),
            0xDD => Some(Self::Oem6),
            0xDE => Some(Self::Oem7),

            // Additional keys
            0x2C => Some(Self::PrintScreen),
            0x91 => Some(Self::ScrollLock),
            0x90 => Some(Self::NumLock),
            0xA0 => Some(Self::LeftShift),
            0xA1 => Some(Self::RightShift),
            0xA2 => Some(Self::LeftCtrl),
            0xA3 => Some(Self::RightCtrl),
            0xA4 => Some(Self::LeftAlt),
            0xA5 => Some(Self::RightAlt),

            _ => None,
        }
    }

    /// Convert a `VirtualKey` representing alphanumeric keys into a `char`
    pub fn to_char(self) -> Option<char> {
        match self {
            Self::A => Some('A'),
            Self::B => Some('B'),
            Self::C => Some('C'),
            Self::D => Some('D'),
            Self::E => Some('E'),
            Self::F => Some('F'),
            Self::G => Some('G'),
            Self::H => Some('H'),
            Self::I => Some('I'),
            Self::J => Some('J'),
            Self::K => Some('K'),
            Self::L => Some('L'),
            Self::M => Some('M'),
            Self::N => Some('N'),
            Self::O => Some('O'),
            Self::P => Some('P'),
            Self::Q => Some('Q'),
            Self::R => Some('R'),
            Self::S => Some('S'),
            Self::T => Some('T'),
            Self::U => Some('U'),
            Self::V => Some('V'),
            Self::W => Some('W'),
            Self::X => Some('X'),
            Self::Y => Some('Y'),
            Self::Z => Some('Z'),
            Self::Num0 => Some('0'),
            Self::Num1 => Some('1'),
            Self::Num2 => Some('2'),
            Self::Num3 => Some('3'),
            Self::Num4 => Some('4'),
            Self::Num5 => Some('5'),
            Self::Num6 => Some('6'),
            Self::Num7 => Some('7'),
            Self::Num8 => Some('8'),
            Self::Num9 => Some('9'),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntheticKeyEvent {
    Press(VirtualKey),
    Release(VirtualKey),
}

pub trait KeyboardSender {
    fn send_key_event(&mut self, event: SyntheticKeyEvent) -> Result<(), String>;

    fn send_sequence(&mut self, events: &[SyntheticKeyEvent]) -> Result<(), String> {
        for event in events {
            self.send_key_event(*event)?;
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct WindowsKeyboardSender;

impl KeyboardSender for WindowsKeyboardSender {
    fn send_key_event(&mut self, event: SyntheticKeyEvent) -> Result<(), String> {
        let (key, flags) = match event {
            SyntheticKeyEvent::Press(key) => (key, KEYBD_EVENT_FLAGS(0)),
            SyntheticKeyEvent::Release(key) => (key, KEYEVENTF_KEYUP),
        };
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(key.to_vk_code() as u16),
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };

        let sent = unsafe { SendInput(&[input], size_of::<INPUT>() as i32) };
        if sent == 1 {
            Ok(())
        } else {
            Err(format!("SendInput sent {sent} of 1 keyboard events"))
        }
    }
}

pub fn navigation_sequence(action: &Action) -> Option<[SyntheticKeyEvent; 4]> {
    match action {
        Action::NavigateBack => Some([
            SyntheticKeyEvent::Press(VirtualKey::Alt),
            SyntheticKeyEvent::Press(VirtualKey::Left),
            SyntheticKeyEvent::Release(VirtualKey::Left),
            SyntheticKeyEvent::Release(VirtualKey::Alt),
        ]),
        Action::NavigateForward => Some([
            SyntheticKeyEvent::Press(VirtualKey::Alt),
            SyntheticKeyEvent::Press(VirtualKey::Right),
            SyntheticKeyEvent::Release(VirtualKey::Right),
            SyntheticKeyEvent::Release(VirtualKey::Alt),
        ]),
        _ => None,
    }
}

pub fn send_navigation_action<S: KeyboardSender>(
    sender: &mut S,
    action: &Action,
) -> Result<bool, String> {
    let Some(sequence) = navigation_sequence(action) else {
        return Ok(false);
    };

    sender.send_sequence(&sequence)?;
    Ok(true)
}

pub fn is_injected_keyboard_hook_flags(flags: u32) -> bool {
    flags & (LLKHF_INJECTED_BITS | LLKHF_LOWER_IL_INJECTED_BITS) != 0
}

/// Struct for managing keybindings
#[derive(Debug)]
pub struct KeyBindings {
    bindings: Vec<(KeyChord, Action)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBinding {
    pub chord: KeyChord,
    pub action: Action,
}

impl KeyBindings {
    /// Create a new KeyBindings instance
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    /// Add a keybinding
    #[allow(dead_code)]
    pub fn add_binding(&mut self, key: VirtualKey, action: Action) {
        self.add_chord_binding(KeyChord::from_key(key), action);
    }

    pub fn add_chord_binding(&mut self, chord: KeyChord, action: Action) {
        if let Some((_, existing_action)) = self
            .bindings
            .iter_mut()
            .find(|(existing_chord, _)| *existing_chord == chord)
        {
            *existing_action = action;
        } else {
            self.bindings.push((chord, action));
        }
        self.bindings
            .sort_by_key(|(chord, _)| std::cmp::Reverse(chord.specificity()));
    }

    pub fn clear(&mut self) {
        self.bindings.clear();
    }

    /// Get the action for a key
    #[allow(dead_code)]
    pub fn get_action(&self, key: VirtualKey) -> Option<&Action> {
        let chord = KeyChord::from_key(key);
        self.bindings
            .iter()
            .find_map(|(bound_chord, action)| (*bound_chord == chord).then_some(action))
    }

    pub fn resolve_for_key_down_event(
        &self,
        event: &KeyEvent,
        shift_can_modify_plain_movement: bool,
    ) -> Option<ResolvedBinding> {
        if !event.is_down {
            return None;
        }

        if let Some((chord, action)) = self
            .bindings
            .iter()
            .find(|(chord, _)| chord.matches_event(event))
        {
            return Some(ResolvedBinding {
                chord: *chord,
                action: action.clone(),
            });
        }

        if let Some((chord, action)) = self
            .bindings
            .iter()
            .find(|(chord, _)| chord.matches_key_down_event(event))
        {
            return Some(ResolvedBinding {
                chord: *chord,
                action: action.clone(),
            });
        }

        if !shift_can_modify_plain_movement {
            return None;
        }

        self.bindings.iter().find_map(|(chord, action)| {
            chord
                .matches_event_allowing_shift_modifier(event)
                .then(|| ResolvedBinding {
                    chord: *chord,
                    action: action.clone(),
                })
        })
    }

    pub fn get_action_for_event(
        &self,
        event: &KeyEvent,
        shift_can_modify_plain_movement: bool,
    ) -> Option<Action> {
        self.resolve_for_key_down_event(event, shift_can_modify_plain_movement)
            .map(|resolved| resolved.action)
    }

    pub fn bound_chords(&self) -> impl Iterator<Item = KeyChord> + '_ {
        self.bindings.iter().map(|(chord, _)| *chord)
    }

    pub fn entries(&self) -> impl Iterator<Item = (KeyChord, &Action)> + '_ {
        self.bindings.iter().map(|(chord, action)| (*chord, action))
    }

    pub fn owned_modifiers(&self) -> HashSet<VirtualKey> {
        let mut owned = HashSet::new();
        for (chord, _) in &self.bindings {
            if chord.right_alt {
                owned.insert(VirtualKey::RightAlt);
                owned.insert(VirtualKey::Alt);
            }
            if chord.alt {
                owned.insert(VirtualKey::Alt);
            }
            if chord.ctrl {
                owned.insert(VirtualKey::Ctrl);
            }
            if chord.shift {
                owned.insert(VirtualKey::Shift);
            }
            // There is currently no generic Win virtual key variant in `VirtualKey`.
            // We still track Win in chord matching via the event's `win_down` flag,
            // but owned-modifier swallowing only applies to representable key events.
            let _ = chord.win;
        }
        owned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MockKeyboardSender {
        events: Vec<SyntheticKeyEvent>,
    }

    impl KeyboardSender for MockKeyboardSender {
        fn send_key_event(&mut self, event: SyntheticKeyEvent) -> Result<(), String> {
            self.events.push(event);
            Ok(())
        }
    }

    #[test]
    fn navigate_back_emits_alt_left_sequence() {
        let mut sender = MockKeyboardSender::default();

        assert_eq!(
            send_navigation_action(&mut sender, &Action::NavigateBack),
            Ok(true)
        );

        assert_eq!(
            sender.events,
            vec![
                SyntheticKeyEvent::Press(VirtualKey::Alt),
                SyntheticKeyEvent::Press(VirtualKey::Left),
                SyntheticKeyEvent::Release(VirtualKey::Left),
                SyntheticKeyEvent::Release(VirtualKey::Alt),
            ]
        );
    }

    #[test]
    fn navigate_forward_emits_alt_right_sequence() {
        let mut sender = MockKeyboardSender::default();

        assert_eq!(
            send_navigation_action(&mut sender, &Action::NavigateForward),
            Ok(true)
        );

        assert_eq!(
            sender.events,
            vec![
                SyntheticKeyEvent::Press(VirtualKey::Alt),
                SyntheticKeyEvent::Press(VirtualKey::Right),
                SyntheticKeyEvent::Release(VirtualKey::Right),
                SyntheticKeyEvent::Release(VirtualKey::Alt),
            ]
        );
    }

    #[test]
    fn injected_keyboard_hook_flags_are_detected() {
        assert!(is_injected_keyboard_hook_flags(0x10));
        assert!(is_injected_keyboard_hook_flags(0x02));
        assert!(is_injected_keyboard_hook_flags(0x12));
        assert!(!is_injected_keyboard_hook_flags(0));
    }

    #[test]
    fn parses_required_punctuation_aliases() {
        let cases = [
            (";", VirtualKey::Oem1),
            ("SEMICOLON", VirtualKey::Oem1),
            (",", VirtualKey::OemComma),
            ("COMMA", VirtualKey::OemComma),
            (".", VirtualKey::OemPeriod),
            ("PERIOD", VirtualKey::OemPeriod),
            ("DOT", VirtualKey::OemPeriod),
        ];

        for (input, expected) in cases {
            assert_eq!(VirtualKey::from_string(input), Some(expected));
        }
    }

    #[test]
    fn parses_optional_oem_punctuation_aliases() {
        let cases = [
            ("/", VirtualKey::Oem2),
            ("SLASH", VirtualKey::Oem2),
            ("FORWARD_SLASH", VirtualKey::Oem2),
            ("`", VirtualKey::Oem3),
            ("BACKTICK", VirtualKey::Oem3),
            ("GRAVE", VirtualKey::Oem3),
            ("[", VirtualKey::Oem4),
            ("LEFT_BRACKET", VirtualKey::Oem4),
            ("LBRACKET", VirtualKey::Oem4),
            ("\\", VirtualKey::Oem5),
            ("BACKSLASH", VirtualKey::Oem5),
            ("]", VirtualKey::Oem6),
            ("RIGHT_BRACKET", VirtualKey::Oem6),
            ("RBRACKET", VirtualKey::Oem6),
            ("'", VirtualKey::Oem7),
            ("QUOTE", VirtualKey::Oem7),
            ("APOSTROPHE", VirtualKey::Oem7),
            ("-", VirtualKey::OemMinus),
            ("MINUS", VirtualKey::OemMinus),
            ("=", VirtualKey::OemPlus),
            ("PLUS", VirtualKey::OemPlus),
            ("EQUALS", VirtualKey::OemPlus),
        ];

        for (input, expected) in cases {
            assert_eq!(VirtualKey::from_string(input), Some(expected));
        }
    }

    #[test]
    fn parses_modifier_key_aliases_case_insensitively() {
        let cases = [
            ("ctrl", VirtualKey::Ctrl),
            ("ALT", VirtualKey::Alt),
            ("leftshift", VirtualKey::LeftShift),
            ("RIGHTSHIFT", VirtualKey::RightShift),
            ("leftctrl", VirtualKey::LeftCtrl),
            ("RIGHTCTRL", VirtualKey::RightCtrl),
            ("leftalt", VirtualKey::LeftAlt),
            ("RIGHT_ALT", VirtualKey::RightAlt),
            ("ralt", VirtualKey::RightAlt),
        ];

        for (input, expected) in cases {
            assert_eq!(VirtualKey::from_string(input), Some(expected), "{input}");
        }
    }

    #[test]
    fn chord_specificity_prefers_right_alt_over_plain_key() {
        let mut bindings = KeyBindings::new();
        bindings.add_chord_binding(KeyChord::parse("W").unwrap(), Action::MoveUp);
        bindings.add_chord_binding(
            KeyChord::parse("RightAlt+W").unwrap(),
            Action::MoveToTopEdge,
        );

        let mut event = KeyEvent::new(VirtualKey::W, true);
        event.alt_down = true;
        event.right_alt_down = true;

        assert_eq!(
            bindings.get_action_for_event(&event, true),
            Some(Action::MoveToTopEdge)
        );

        event.alt_down = false;
        event.right_alt_down = false;

        assert_eq!(
            bindings.get_action_for_event(&event, true),
            Some(Action::MoveUp)
        );
    }

    #[test]
    fn shifted_plain_key_resolves_to_plain_binding() {
        let mut bindings = KeyBindings::new();
        bindings.add_chord_binding(KeyChord::parse("W").unwrap(), Action::MoveUp);

        let mut event = KeyEvent::new(VirtualKey::W, true);
        event.shift_down = true;

        assert_eq!(
            bindings.get_action_for_event(&event, true),
            Some(Action::MoveUp)
        );
    }

    #[test]
    fn right_alt_binding_wins_over_shift_relaxed_plain_key() {
        let mut bindings = KeyBindings::new();
        bindings.add_chord_binding(KeyChord::parse("W").unwrap(), Action::MoveUp);
        bindings.add_chord_binding(
            KeyChord::parse("RightAlt+W").unwrap(),
            Action::MoveToTopEdge,
        );

        let mut event = KeyEvent::new(VirtualKey::W, true);
        event.alt_down = true;
        event.right_alt_down = true;

        assert_eq!(
            bindings.get_action_for_event(&event, true),
            Some(Action::MoveToTopEdge)
        );
    }

    #[test]
    fn ctrl_plain_key_does_not_resolve_to_plain_binding() {
        let mut bindings = KeyBindings::new();
        bindings.add_chord_binding(KeyChord::parse("W").unwrap(), Action::MoveUp);

        let mut event = KeyEvent::new(VirtualKey::W, true);
        event.ctrl_down = true;

        assert_eq!(bindings.get_action_for_event(&event, true), None);
    }

    #[test]
    fn shift_exact_binding_wins_over_plain_binding() {
        let mut bindings = KeyBindings::new();
        bindings.add_chord_binding(KeyChord::parse("E").unwrap(), Action::MoveRight);
        bindings.add_chord_binding(KeyChord::parse("Shift+E").unwrap(), Action::MoveLeft);

        let mut event = KeyEvent::new(VirtualKey::E, true);
        event.shift_down = true;

        assert_eq!(
            bindings
                .resolve_for_key_down_event(&event, true)
                .map(|r| r.action),
            Some(Action::MoveLeft)
        );
    }

    #[test]
    fn shift_falls_back_to_plain_when_enabled_and_shift_binding_absent() {
        let mut bindings = KeyBindings::new();
        bindings.add_chord_binding(KeyChord::parse("E").unwrap(), Action::MoveRight);
        let mut event = KeyEvent::new(VirtualKey::E, true);
        event.shift_down = true;
        assert_eq!(
            bindings
                .resolve_for_key_down_event(&event, true)
                .map(|r| r.action),
            Some(Action::MoveRight)
        );
    }

    #[test]
    fn shift_does_not_fallback_to_plain_when_disabled() {
        let mut bindings = KeyBindings::new();
        bindings.add_chord_binding(KeyChord::parse("E").unwrap(), Action::MoveRight);
        let mut event = KeyEvent::new(VirtualKey::E, true);
        event.shift_down = true;
        assert_eq!(bindings.resolve_for_key_down_event(&event, false), None);
    }

    #[test]
    fn right_alt_then_alt_then_plain_precedence() {
        let mut bindings = KeyBindings::new();
        bindings.add_chord_binding(KeyChord::parse("W").unwrap(), Action::MoveUp);
        bindings.add_chord_binding(KeyChord::parse("Alt+W").unwrap(), Action::MoveDown);
        bindings.add_chord_binding(KeyChord::parse("RightAlt+W").unwrap(), Action::MoveLeft);

        let mut ralt = KeyEvent::new(VirtualKey::W, true);
        ralt.alt_down = true;
        ralt.right_alt_down = true;
        assert_eq!(
            bindings
                .resolve_for_key_down_event(&ralt, true)
                .map(|r| r.action),
            Some(Action::MoveLeft)
        );

        let mut alt = KeyEvent::new(VirtualKey::W, true);
        alt.alt_down = true;
        assert_eq!(
            bindings
                .resolve_for_key_down_event(&alt, true)
                .map(|r| r.action),
            Some(Action::MoveDown)
        );

        let plain = KeyEvent::new(VirtualKey::W, true);
        assert_eq!(
            bindings
                .resolve_for_key_down_event(&plain, true)
                .map(|r| r.action),
            Some(Action::MoveUp)
        );
    }

    #[test]
    fn shift_self_keys_resolve_to_slow_mouse() {
        let mut bindings = KeyBindings::new();
        bindings.add_chord_binding(KeyChord::parse("LeftShift").unwrap(), Action::SlowMouse);
        bindings.add_chord_binding(KeyChord::parse("RightShift").unwrap(), Action::SlowMouse);

        for key in [VirtualKey::LeftShift, VirtualKey::RightShift] {
            let mut event = KeyEvent::new(key, true);
            event.shift_down = true;

            assert_eq!(
                bindings.get_action_for_event(&event, true),
                Some(Action::SlowMouse),
                "{key:?}"
            );
        }
    }

    #[test]
    fn right_alt_in_chord_marks_right_alt_and_alt_owned() {
        let mut bindings = KeyBindings::new();
        bindings.add_chord_binding(KeyChord::parse("RightAlt+W").unwrap(), Action::MoveUp);
        let owned = bindings.owned_modifiers();
        assert!(owned.contains(&VirtualKey::RightAlt));
        assert!(owned.contains(&VirtualKey::Alt));
    }

    #[test]
    fn alt_chord_marks_alt_owned() {
        let mut bindings = KeyBindings::new();
        bindings.add_chord_binding(KeyChord::parse("Alt+W").unwrap(), Action::MoveUp);
        let owned = bindings.owned_modifiers();
        assert!(owned.contains(&VirtualKey::Alt));
    }
}
