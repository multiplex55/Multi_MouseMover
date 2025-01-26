use std::collections::HashMap;

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
            "OEM_PLUS" => Some(Self::OemPlus),
            "OEM_COMMA" => Some(Self::OemComma),
            "OEM_MINUS" => Some(Self::OemMinus),
            "OEM_PERIOD" => Some(Self::OemPeriod),
            "OEM_1" => Some(Self::Oem1),
            "OEM_2" => Some(Self::Oem2),
            "OEM_3" => Some(Self::Oem3),
            "OEM_4" => Some(Self::Oem4),
            "OEM_5" => Some(Self::Oem5),
            "OEM_6" => Some(Self::Oem6),
            "OEM_7" => Some(Self::Oem7),

            // Additional keys
            "PRINTSCREEN" => Some(Self::PrintScreen),
            "SCROLLLOCK" => Some(Self::ScrollLock),
            "NUMLOCK" => Some(Self::NumLock),
            "LEFTSHIFT" => Some(Self::LeftShift),
            "RIGHTSHIFT" => Some(Self::RightShift),
            "LEFTCTRL" => Some(Self::LeftCtrl),
            "RIGHTCTRL" => Some(Self::RightCtrl),
            "LEFTALT" => Some(Self::LeftAlt),
            "RIGHTALT" => Some(Self::RightAlt),

            _ => None,
        }
    }

    /// Convert a `VirtualKey` to its virtual key code
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
}

/// Struct for managing keybindings
#[derive(Debug)]
pub struct KeyBindings {
    bindings: HashMap<VirtualKey, String>,
}

impl KeyBindings {
    /// Create a new KeyBindings instance
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    /// Add a keybinding
    pub fn add_binding(&mut self, key: VirtualKey, action: String) {
        self.bindings.insert(key, action);
    }

    /// Get the action for a key
    pub fn get_action(&self, key: VirtualKey) -> Option<&String> {
        self.bindings.get(&key)
    }
}

/// Manages actions associated with key presses
pub struct ActionHandler {
    actions: HashMap<String, Box<dyn Fn() + Send + Sync>>,
}

impl ActionHandler {
    /// Create a new ActionHandler
    pub fn new() -> Self {
        Self {
            actions: HashMap::new(),
        }
    }

    /// Add an action
    pub fn add_action<F>(&mut self, name: &str, action: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.actions.insert(name.to_string(), Box::new(action));
    }

    /// Execute an action by name
    pub fn execute_action(&self, name: &str) {
        if let Some(action) = self.actions.get(name) {
            action();
        }
    }
}
