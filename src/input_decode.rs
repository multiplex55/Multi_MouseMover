use crate::keyboard::VirtualKey;

pub const LLKHF_EXTENDED_BIT: u32 = 0x01;
const LEFT_SHIFT_SCANCODE: u32 = 0x2A;
const RIGHT_SHIFT_SCANCODE: u32 = 0x36;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawKeyboardHookEvent {
    pub vk_code: u32,
    pub scan_code: u32,
    pub flags: u32,
    pub is_down: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModifierSnapshot {
    pub left_alt: bool,
    pub right_alt: bool,
    pub left_ctrl: bool,
    pub right_ctrl: bool,
    pub left_shift: bool,
    pub right_shift: bool,
    pub left_win: bool,
    pub right_win: bool,
}

pub fn decode_virtual_key_from_hook(raw: RawKeyboardHookEvent) -> Option<VirtualKey> {
    match raw.vk_code {
        0x10 => match raw.scan_code {
            RIGHT_SHIFT_SCANCODE => Some(VirtualKey::RightShift),
            LEFT_SHIFT_SCANCODE => Some(VirtualKey::LeftShift),
            _ => Some(VirtualKey::Shift),
        },
        0x11 => {
            if raw.flags & LLKHF_EXTENDED_BIT != 0 {
                Some(VirtualKey::RightCtrl)
            } else {
                Some(VirtualKey::LeftCtrl)
            }
        }
        0x12 => {
            if raw.flags & LLKHF_EXTENDED_BIT != 0 {
                Some(VirtualKey::RightAlt)
            } else {
                Some(VirtualKey::LeftAlt)
            }
        }
        0x5B => Some(VirtualKey::LeftWin),
        0x5C => Some(VirtualKey::RightWin),
        _ => VirtualKey::from_vk_code(raw.vk_code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_vk_menu_extended_is_rightalt() {
        let key = decode_virtual_key_from_hook(RawKeyboardHookEvent {
            vk_code: 0x12,
            scan_code: 0,
            flags: LLKHF_EXTENDED_BIT,
            is_down: true,
        });
        assert_eq!(key, Some(VirtualKey::RightAlt));
    }

    #[test]
    fn decode_vk_menu_nonextended_is_leftalt() {
        let key = decode_virtual_key_from_hook(RawKeyboardHookEvent {
            vk_code: 0x12,
            scan_code: 0,
            flags: 0,
            is_down: true,
        });
        assert_eq!(key, Some(VirtualKey::LeftAlt));
    }

    #[test]
    fn decode_vk_control_extended_is_rightctrl() {
        let key = decode_virtual_key_from_hook(RawKeyboardHookEvent {
            vk_code: 0x11,
            scan_code: 0,
            flags: LLKHF_EXTENDED_BIT,
            is_down: true,
        });
        assert_eq!(key, Some(VirtualKey::RightCtrl));
    }

    #[test]
    fn decode_vk_control_nonextended_is_leftctrl() {
        let key = decode_virtual_key_from_hook(RawKeyboardHookEvent {
            vk_code: 0x11,
            scan_code: 0,
            flags: 0,
            is_down: true,
        });
        assert_eq!(key, Some(VirtualKey::LeftCtrl));
    }

    #[test]
    fn decode_shift_left_right_by_scancode() {
        let left = decode_virtual_key_from_hook(RawKeyboardHookEvent {
            vk_code: 0x10,
            scan_code: 0x2A,
            flags: 0,
            is_down: true,
        });
        let right = decode_virtual_key_from_hook(RawKeyboardHookEvent {
            vk_code: 0x10,
            scan_code: 0x36,
            flags: 0,
            is_down: true,
        });

        assert_eq!(left, Some(VirtualKey::LeftShift));
        assert_eq!(right, Some(VirtualKey::RightShift));
    }

    #[test]
    fn bug_altgr_hook_snapshot_keeps_synthetic_ctrl_side_unspecified() {
        let event = crate::app_state::KeyEvent::with_modifier_state(
            VirtualKey::E,
            true,
            ModifierSnapshot {
                right_alt: true,
                left_ctrl: false,
                right_ctrl: false,
                ..ModifierSnapshot::default()
            },
        );

        assert!(event.alt_down);
        assert!(event.right_alt_down);
        assert!(!event.ctrl_down);
        assert!(!event.left_ctrl_down);
        assert!(!event.right_ctrl_down);
    }

    #[test]
    fn bug_altgr_synthetic_ctrl_snapshot_is_distinguishable_from_physical_ctrl() {
        let event = crate::app_state::KeyEvent::new(VirtualKey::E, true);
        let mut synthetic = event;
        synthetic.alt_down = true;
        synthetic.right_alt_down = true;
        synthetic.ctrl_down = true;

        assert!(synthetic.ctrl_down);
        assert!(!synthetic.left_ctrl_down);
        assert!(!synthetic.right_ctrl_down);
    }
}
