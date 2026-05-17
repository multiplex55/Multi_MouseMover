use crate::action_handler::MovementTick;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// Enum representing all possible actions
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Action {
    // Continuous cursor movement actions.
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    MoveUpRight,
    MoveUpLeft,
    MoveDownRight,
    MoveDownLeft,

    // Cursor jump actions.
    MoveToTopEdge,
    MoveToBottomEdge,
    MoveToLeftEdge,
    MoveToRightEdge,
    CenterCurrentMonitor,

    // Click actions.
    LeftClick,
    RightClick,
    MiddleClick,
    ClickThenDisable,
    ToggleDragMode,

    // Wheel actions.
    WheelUp,
    WheelDown,
    WheelLeft,
    WheelRight,
    WheelSpeedUp,
    WheelSpeedDown,

    // Control actions.
    Exit,
    SlowMouse,
    JumpMode,
    JumpModeProfile(String),
}

impl Action {
    /// Convert a string to an `Action` enum
    pub fn from_string(action: &str) -> Option<Self> {
        match action.to_lowercase().as_str() {
            "move_up" => Some(Self::MoveUp),
            "move_down" => Some(Self::MoveDown),
            "move_left" => Some(Self::MoveLeft),
            "move_right" => Some(Self::MoveRight),
            "move_up_right" => Some(Self::MoveUpRight),
            "move_up_left" => Some(Self::MoveUpLeft),
            "move_down_right" => Some(Self::MoveDownRight),
            "move_down_left" => Some(Self::MoveDownLeft),
            "move_to_top_edge" => Some(Self::MoveToTopEdge),
            "move_to_bottom_edge" => Some(Self::MoveToBottomEdge),
            "move_to_left_edge" => Some(Self::MoveToLeftEdge),
            "move_to_right_edge" => Some(Self::MoveToRightEdge),
            "center_current_monitor" | "center_monitor" => Some(Self::CenterCurrentMonitor),
            "left_click" => Some(Self::LeftClick),
            "right_click" => Some(Self::RightClick),
            "middle_click" | "middle_mouse" => Some(Self::MiddleClick),
            "click_then_disable" => Some(Self::ClickThenDisable),
            "toggle_drag_mode" | "drag_mode" | "toggle_left_drag" | "toggle_left_button_hold" => {
                Some(Self::ToggleDragMode)
            }
            "wheel_up" | "scroll_up" => Some(Self::WheelUp),
            "wheel_down" | "scroll_down" => Some(Self::WheelDown),
            "wheel_left" | "scroll_left" => Some(Self::WheelLeft),
            "wheel_right" | "scroll_right" => Some(Self::WheelRight),
            "wheel_speed_up" => Some(Self::WheelSpeedUp),
            "wheel_speed_down" => Some(Self::WheelSpeedDown),
            "exit" => Some(Self::Exit),
            "slow_mouse" => Some(Self::SlowMouse),
            "jump_mode" => Some(Self::JumpMode),
            action if action.starts_with("jump_mode_profile:") => {
                action.split_once(':').and_then(|(_, profile)| {
                    (!profile.is_empty()).then(|| Self::JumpModeProfile(profile.to_string()))
                })
            }
            action if action.starts_with("jump_mode_profile.") => {
                action.split_once('.').and_then(|(_, profile)| {
                    (!profile.is_empty()).then(|| Self::JumpModeProfile(profile.to_string()))
                })
            }
            _ => None,
        }
    }

    pub fn is_movement(&self) -> bool {
        matches!(
            self,
            Self::MoveUp
                | Self::MoveDown
                | Self::MoveLeft
                | Self::MoveRight
                | Self::MoveUpRight
                | Self::MoveUpLeft
                | Self::MoveDownRight
                | Self::MoveDownLeft
        )
    }

    pub fn is_wheel_direction(&self) -> bool {
        matches!(
            self,
            Self::WheelUp | Self::WheelDown | Self::WheelLeft | Self::WheelRight
        )
    }

    pub fn is_continuous(&self) -> bool {
        self.is_movement() || self == &Self::SlowMouse || self.is_wheel_direction()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_new_canonical_action_strings() {
        let cases = [
            ("middle_click", Action::MiddleClick),
            ("wheel_up", Action::WheelUp),
            ("wheel_down", Action::WheelDown),
            ("wheel_left", Action::WheelLeft),
            ("wheel_right", Action::WheelRight),
            ("wheel_speed_up", Action::WheelSpeedUp),
            ("wheel_speed_down", Action::WheelSpeedDown),
            ("center_current_monitor", Action::CenterCurrentMonitor),
            ("click_then_disable", Action::ClickThenDisable),
            ("toggle_drag_mode", Action::ToggleDragMode),
            ("move_to_top_edge", Action::MoveToTopEdge),
            ("move_to_bottom_edge", Action::MoveToBottomEdge),
            ("move_to_left_edge", Action::MoveToLeftEdge),
            ("move_to_right_edge", Action::MoveToRightEdge),
        ];

        for (input, expected) in cases {
            assert_eq!(Action::from_string(input), Some(expected));
        }
    }

    #[test]
    fn parses_new_alias_action_strings() {
        let cases = [
            ("middle_mouse", Action::MiddleClick),
            ("scroll_up", Action::WheelUp),
            ("scroll_down", Action::WheelDown),
            ("scroll_left", Action::WheelLeft),
            ("scroll_right", Action::WheelRight),
            ("center_monitor", Action::CenterCurrentMonitor),
            ("drag_mode", Action::ToggleDragMode),
            ("toggle_left_drag", Action::ToggleDragMode),
            ("toggle_left_button_hold", Action::ToggleDragMode),
        ];

        for (input, expected) in cases {
            assert_eq!(Action::from_string(input), Some(expected));
        }
    }

    #[test]
    fn parses_action_strings_case_insensitively() {
        let cases = [
            ("MOVE_UP", Action::MoveUp),
            ("Move_Left", Action::MoveLeft),
            ("cLiCk_ThEn_DiSaBlE", Action::ClickThenDisable),
            ("WHEEL_SPEED_DOWN", Action::WheelSpeedDown),
            ("Jump_Mode", Action::JumpMode),
        ];

        for (input, expected) in cases {
            assert_eq!(Action::from_string(input), Some(expected), "{input}");
        }
    }

    #[test]
    fn unknown_action_strings_do_not_parse() {
        assert_eq!(Action::from_string("unknown_action"), None);
    }

    #[test]
    fn continuous_move_actions_are_movement() {
        let movement_actions = [
            Action::MoveUp,
            Action::MoveDown,
            Action::MoveLeft,
            Action::MoveRight,
            Action::MoveUpRight,
            Action::MoveUpLeft,
            Action::MoveDownRight,
            Action::MoveDownLeft,
        ];

        for action in movement_actions {
            assert!(action.is_movement());
        }
    }

    #[test]
    fn non_continuous_actions_are_not_movement() {
        let non_movement_actions = [
            Action::MoveToTopEdge,
            Action::MoveToBottomEdge,
            Action::MoveToLeftEdge,
            Action::MoveToRightEdge,
            Action::CenterCurrentMonitor,
            Action::LeftClick,
            Action::RightClick,
            Action::MiddleClick,
            Action::ClickThenDisable,
            Action::ToggleDragMode,
            Action::WheelUp,
            Action::WheelDown,
            Action::WheelLeft,
            Action::WheelRight,
            Action::WheelSpeedUp,
            Action::WheelSpeedDown,
            Action::Exit,
            Action::SlowMouse,
            Action::JumpMode,
        ];

        for action in non_movement_actions {
            assert!(!action.is_movement());
        }
    }

    #[test]
    fn continuous_actions_include_movement_slow_mode_and_wheel_directions() {
        let continuous_actions = [
            Action::MoveUp,
            Action::MoveDown,
            Action::MoveLeft,
            Action::MoveRight,
            Action::MoveUpRight,
            Action::MoveUpLeft,
            Action::MoveDownRight,
            Action::MoveDownLeft,
            Action::SlowMouse,
            Action::WheelUp,
            Action::WheelDown,
            Action::WheelLeft,
            Action::WheelRight,
        ];

        for action in continuous_actions {
            assert!(action.is_continuous(), "{action:?}");
        }
    }

    #[test]
    fn one_shot_actions_are_not_continuous() {
        let one_shot_actions = [
            Action::MoveToTopEdge,
            Action::MoveToBottomEdge,
            Action::MoveToLeftEdge,
            Action::MoveToRightEdge,
            Action::CenterCurrentMonitor,
            Action::LeftClick,
            Action::RightClick,
            Action::MiddleClick,
            Action::ClickThenDisable,
            Action::ToggleDragMode,
            Action::WheelSpeedUp,
            Action::WheelSpeedDown,
            Action::Exit,
            Action::JumpMode,
        ];

        for action in one_shot_actions {
            assert!(!action.is_continuous(), "{action:?}");
        }
    }
}

/// Manages actions associated with key presses
pub struct ActionHandler<
    B: crate::action_handler::MouseBackend = crate::action_handler::EnigoMouseBackend,
> {
    pub actions: HashMap<Action, Box<dyn Fn() + Send + Sync>>,
    pub active_keys: HashSet<Action>, // Tracks currently held actions
    pub mouse_master: crate::action_handler::MouseMaster<B>, // Reference to MouseMaster
}

impl<B: crate::action_handler::MouseBackend> ActionHandler<B> {
    /// Create a new ActionHandler
    pub fn new(mouse_master: crate::action_handler::MouseMaster<B>) -> Self {
        Self {
            actions: HashMap::new(),
            active_keys: HashSet::new(),
            mouse_master,
        }
    }

    /// Add an action to the handler
    #[allow(dead_code)]
    pub fn add_action<F>(&mut self, action: Action, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.actions.insert(action, Box::new(callback));
    }

    /// Execute an action by its enum value
    pub fn execute_action(&mut self, action: &Action) {
        if let Some(callback) = self.actions.get(action) {
            callback();
        } else {
            // Fallback to MouseMaster's handling
            self.mouse_master.handle_action(action.clone());
        }
    }
    pub fn process_active_keys(&mut self, key: Action, is_keydown: bool) {
        if is_keydown {
            self.active_keys.insert(key);
        } else {
            self.active_keys.remove(&key);
        }
    }

    pub fn clear_active_keys(&mut self) {
        self.active_keys.clear();
        self.mouse_master.reset_speed();
    }

    pub fn tick_movement(&mut self) -> MovementTick {
        self.mouse_master
            .tick_movement(&self.active_keys, Duration::from_millis(0))
    }
}
