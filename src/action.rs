use crate::action_handler::MovementTick;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// Enum representing all possible actions
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Action {
    StepMove {
        direction: Direction2D,
        tier: StepMoveTier,
    },
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
    MoveToWindowTopEdge,
    MoveToWindowBottomEdge,
    MoveToWindowLeftEdge,
    MoveToWindowRightEdge,
    MoveToWindowCenter,
    MoveToWindowTitlebar,

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
    WheelSpeedReset,
    WheelProfileNext,
    WheelProfilePrevious,
    WheelProfileSelect(String),
    MouseSpeedUp,
    MouseSpeedDown,
    MouseSpeedReset,
    MovementProfileNext,
    MovementProfilePrevious,
    MovementProfileSelect(String),

    // Control actions.
    Exit,
    ReloadConfig,
    PanicReset,
    SlowMouse,
    SurgicalMode,
    ScrollModifier,
    JumpMode,
    JumpModeProfile(String),
    GridMode,
    ScreenSelect,
    NavigateBack,
    NavigateForward,
    Disable,
    ShowHelp,
    UiHintMode,
    SaveMousePosition,
    ClearMousePositions,
    PositionHistoryMode,
    BookmarkMode,
    BookmarkSlot(u8),
    ClearBookmarkSlot(u8),
    ClearAllBookmarks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction2D {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StepMoveTier {
    Small,
    Normal,
    Large,
}

impl Action {
    /// Convert a string to an `Action` enum
    pub fn from_string(action: &str) -> Option<Self> {
        match action.to_lowercase().as_str() {
            "move_up" => Some(Self::MoveUp),
            "step_move_up" => Some(Self::StepMove {
                direction: Direction2D::Up,
                tier: StepMoveTier::Normal,
            }),
            "step_move_down" => Some(Self::StepMove {
                direction: Direction2D::Down,
                tier: StepMoveTier::Normal,
            }),
            "step_move_left" => Some(Self::StepMove {
                direction: Direction2D::Left,
                tier: StepMoveTier::Normal,
            }),
            "step_move_right" => Some(Self::StepMove {
                direction: Direction2D::Right,
                tier: StepMoveTier::Normal,
            }),
            "step_move_small_up" => Some(Self::StepMove {
                direction: Direction2D::Up,
                tier: StepMoveTier::Small,
            }),
            "step_move_small_down" => Some(Self::StepMove {
                direction: Direction2D::Down,
                tier: StepMoveTier::Small,
            }),
            "step_move_small_left" => Some(Self::StepMove {
                direction: Direction2D::Left,
                tier: StepMoveTier::Small,
            }),
            "step_move_small_right" => Some(Self::StepMove {
                direction: Direction2D::Right,
                tier: StepMoveTier::Small,
            }),
            "step_move_large_up" => Some(Self::StepMove {
                direction: Direction2D::Up,
                tier: StepMoveTier::Large,
            }),
            "step_move_large_down" => Some(Self::StepMove {
                direction: Direction2D::Down,
                tier: StepMoveTier::Large,
            }),
            "step_move_large_left" => Some(Self::StepMove {
                direction: Direction2D::Left,
                tier: StepMoveTier::Large,
            }),
            "step_move_large_right" => Some(Self::StepMove {
                direction: Direction2D::Right,
                tier: StepMoveTier::Large,
            }),
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
            "move_to_window_top_edge" => Some(Self::MoveToWindowTopEdge),
            "move_to_window_bottom_edge" => Some(Self::MoveToWindowBottomEdge),
            "move_to_window_left_edge" => Some(Self::MoveToWindowLeftEdge),
            "move_to_window_right_edge" => Some(Self::MoveToWindowRightEdge),
            "move_to_window_center" => Some(Self::MoveToWindowCenter),
            "move_to_window_titlebar" => Some(Self::MoveToWindowTitlebar),
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
            "wheel_speed_reset" => Some(Self::WheelSpeedReset),
            "wheel_profile_next" => Some(Self::WheelProfileNext),
            "wheel_profile_previous" | "wheel_profile_prev" => Some(Self::WheelProfilePrevious),
            "mouse_speed_up" => Some(Self::MouseSpeedUp),
            "mouse_speed_down" => Some(Self::MouseSpeedDown),
            "mouse_speed_reset" => Some(Self::MouseSpeedReset),
            "movement_profile_next" | "mouse_profile_next" => Some(Self::MovementProfileNext),
            "movement_profile_previous"
            | "movement_profile_prev"
            | "mouse_profile_previous"
            | "mouse_profile_prev" => Some(Self::MovementProfilePrevious),
            "exit" => Some(Self::Exit),
            "reload_config" => Some(Self::ReloadConfig),
            "panic_reset" => Some(Self::PanicReset),
            "slow_mouse" => Some(Self::SlowMouse),
            "surgical_mode" => Some(Self::SurgicalMode),
            "scroll_modifier" => Some(Self::ScrollModifier),
            "jump_mode" => Some(Self::JumpMode),
            "grid_mode" | "grid" => Some(Self::GridMode),
            "screen_select" | "select_screen" => Some(Self::ScreenSelect),
            "navigate_back" | "browser_back" => Some(Self::NavigateBack),
            "navigate_forward" | "browser_forward" => Some(Self::NavigateForward),
            "disable" | "disable_app" | "idle_mode" => Some(Self::Disable),
            "show_help" | "help" | "toggle_help" | "hints" | "show_hints" => Some(Self::ShowHelp),
            "ui_hint_mode" | "ui_hints" | "show_ui_hints" | "hint_mode" => Some(Self::UiHintMode),
            "save_mouse_position" => Some(Self::SaveMousePosition),
            "clear_mouse_positions" => Some(Self::ClearMousePositions),
            "position_history_mode" => Some(Self::PositionHistoryMode),
            "bookmark_mode" => Some(Self::BookmarkMode),
            "clear_all_bookmarks" => Some(Self::ClearAllBookmarks),
            action if action.starts_with("bookmark_slot_") => action
                .trim_start_matches("bookmark_slot_")
                .parse::<u8>()
                .ok()
                .filter(|slot| (1..=9).contains(slot))
                .map(Self::BookmarkSlot),
            action if action.starts_with("bookmark_") => action
                .trim_start_matches("bookmark_")
                .parse::<u8>()
                .ok()
                .filter(|slot| (1..=9).contains(slot))
                .map(Self::BookmarkSlot),
            action if action.starts_with("jump_to_bookmark_") => action
                .trim_start_matches("jump_to_bookmark_")
                .parse::<u8>()
                .ok()
                .filter(|slot| (1..=9).contains(slot))
                .map(Self::BookmarkSlot),
            action if action.starts_with("clear_bookmark_") => action
                .trim_start_matches("clear_bookmark_")
                .parse::<u8>()
                .ok()
                .filter(|slot| (1..=9).contains(slot))
                .map(Self::ClearBookmarkSlot),
            action
                if action.starts_with("movement_profile:")
                    || action.starts_with("mouse_profile:")
                    || action.starts_with("select_movement_profile:") =>
            {
                action.split_once(':').and_then(|(_, profile)| {
                    (!profile.is_empty()).then(|| Self::MovementProfileSelect(profile.to_string()))
                })
            }
            action
                if action.starts_with("movement_profile.")
                    || action.starts_with("mouse_profile.") =>
            {
                action.split_once('.').and_then(|(_, profile)| {
                    (!profile.is_empty()).then(|| Self::MovementProfileSelect(profile.to_string()))
                })
            }
            action
                if action.starts_with("wheel_profile:")
                    || action.starts_with("select_wheel_profile:") =>
            {
                action.split_once(':').and_then(|(_, profile)| {
                    (!profile.is_empty()).then(|| Self::WheelProfileSelect(profile.to_string()))
                })
            }
            action if action.starts_with("wheel_profile.") => {
                action.split_once('.').and_then(|(_, profile)| {
                    (!profile.is_empty()).then(|| Self::WheelProfileSelect(profile.to_string()))
                })
            }
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
        self.is_movement()
            || self == &Self::SlowMouse
            || self == &Self::SurgicalMode
            || self == &Self::ScrollModifier
            || self.is_wheel_direction()
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
            ("wheel_speed_reset", Action::WheelSpeedReset),
            ("mouse_speed_up", Action::MouseSpeedUp),
            ("mouse_speed_down", Action::MouseSpeedDown),
            ("mouse_speed_reset", Action::MouseSpeedReset),
            ("movement_profile_next", Action::MovementProfileNext),
            ("movement_profile_previous", Action::MovementProfilePrevious),
            ("wheel_profile_next", Action::WheelProfileNext),
            ("wheel_profile_previous", Action::WheelProfilePrevious),
            ("reload_config", Action::ReloadConfig),
            ("panic_reset", Action::PanicReset),
            ("surgical_mode", Action::SurgicalMode),
            ("scroll_modifier", Action::ScrollModifier),
            ("center_current_monitor", Action::CenterCurrentMonitor),
            ("click_then_disable", Action::ClickThenDisable),
            ("toggle_drag_mode", Action::ToggleDragMode),
            ("move_to_top_edge", Action::MoveToTopEdge),
            ("move_to_bottom_edge", Action::MoveToBottomEdge),
            ("move_to_left_edge", Action::MoveToLeftEdge),
            ("move_to_right_edge", Action::MoveToRightEdge),
            ("move_to_window_top_edge", Action::MoveToWindowTopEdge),
            ("move_to_window_bottom_edge", Action::MoveToWindowBottomEdge),
            ("move_to_window_left_edge", Action::MoveToWindowLeftEdge),
            ("move_to_window_right_edge", Action::MoveToWindowRightEdge),
            ("move_to_window_center", Action::MoveToWindowCenter),
            ("move_to_window_titlebar", Action::MoveToWindowTitlebar),
            ("grid_mode", Action::GridMode),
            ("screen_select", Action::ScreenSelect),
            ("navigate_back", Action::NavigateBack),
            ("navigate_forward", Action::NavigateForward),
            ("disable", Action::Disable),
            ("show_help", Action::ShowHelp),
            ("ui_hint_mode", Action::UiHintMode),
            (
                "step_move_up",
                Action::StepMove {
                    direction: Direction2D::Up,
                    tier: StepMoveTier::Normal,
                },
            ),
            (
                "step_move_small_left",
                Action::StepMove {
                    direction: Direction2D::Left,
                    tier: StepMoveTier::Small,
                },
            ),
            (
                "step_move_large_right",
                Action::StepMove {
                    direction: Direction2D::Right,
                    tier: StepMoveTier::Large,
                },
            ),
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
            ("grid", Action::GridMode),
            ("select_screen", Action::ScreenSelect),
            ("browser_back", Action::NavigateBack),
            ("browser_forward", Action::NavigateForward),
            ("disable_app", Action::Disable),
            ("idle_mode", Action::Disable),
            ("help", Action::ShowHelp),
            ("toggle_help", Action::ShowHelp),
            ("hints", Action::ShowHelp),
            ("show_hints", Action::ShowHelp),
            ("ui_hints", Action::UiHintMode),
            ("show_ui_hints", Action::UiHintMode),
            ("hint_mode", Action::UiHintMode),
            ("bookmark_1", Action::BookmarkSlot(1)),
            ("jump_to_bookmark_1", Action::BookmarkSlot(1)),
            ("clear_bookmark_1", Action::ClearBookmarkSlot(1)),
            ("clear_all_bookmarks", Action::ClearAllBookmarks),
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
            ("GRID_MODE", Action::GridMode),
            ("Select_Screen", Action::ScreenSelect),
            ("BROWSER_BACK", Action::NavigateBack),
            ("browser_FORWARD", Action::NavigateForward),
            ("Idle_Mode", Action::Disable),
            ("SHOW_HINTS", Action::ShowHelp),
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
    fn parses_profile_action_strings() {
        assert_eq!(
            Action::from_string("movement_profile:fast"),
            Some(Action::MovementProfileSelect("fast".to_string()))
        );
        assert_eq!(
            Action::from_string("wheel_profile.precise"),
            Some(Action::WheelProfileSelect("precise".to_string()))
        );
        assert_eq!(
            Action::from_string("jump_mode_profile:window"),
            Some(Action::JumpModeProfile("window".to_string()))
        );
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
            Action::WheelSpeedReset,
            Action::Exit,
            Action::ReloadConfig,
            Action::PanicReset,
            Action::SlowMouse,
            Action::SurgicalMode,
            Action::ScrollModifier,
            Action::JumpMode,
            Action::GridMode,
            Action::ScreenSelect,
            Action::NavigateBack,
            Action::NavigateForward,
            Action::Disable,
            Action::ShowHelp,
            Action::UiHintMode,
            Action::StepMove {
                direction: Direction2D::Up,
                tier: StepMoveTier::Normal,
            },
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
            Action::ScrollModifier,
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
            Action::WheelSpeedReset,
            Action::Exit,
            Action::ReloadConfig,
            Action::PanicReset,
            Action::JumpMode,
            Action::GridMode,
            Action::ScreenSelect,
            Action::NavigateBack,
            Action::NavigateForward,
            Action::Disable,
            Action::ShowHelp,
            Action::UiHintMode,
        ];

        for action in one_shot_actions {
            assert!(!action.is_continuous(), "{action:?}");
        }
    }

    #[test]
    fn ui_hint_mode_parses() {
        assert_eq!(
            Action::from_string("ui_hint_mode"),
            Some(Action::UiHintMode)
        );
    }

    #[test]
    fn ui_hints_parses() {
        assert_eq!(Action::from_string("ui_hints"), Some(Action::UiHintMode));
    }

    #[test]
    fn show_ui_hints_parses() {
        assert_eq!(
            Action::from_string("show_ui_hints"),
            Some(Action::UiHintMode)
        );
    }

    #[test]
    fn hint_mode_parses() {
        assert_eq!(Action::from_string("hint_mode"), Some(Action::UiHintMode));
    }

    #[test]
    fn ui_hint_mode_is_not_continuous() {
        assert!(!Action::UiHintMode.is_continuous());
    }

    #[test]
    fn parses_position_history_actions() {
        assert_eq!(
            Action::from_string("save_mouse_position"),
            Some(Action::SaveMousePosition)
        );
        assert_eq!(
            Action::from_string("clear_mouse_positions"),
            Some(Action::ClearMousePositions)
        );
        assert_eq!(
            Action::from_string("position_history_mode"),
            Some(Action::PositionHistoryMode)
        );
    }

    #[test]
    fn parses_bookmark_actions() {
        assert_eq!(
            Action::from_string("bookmark_mode"),
            Some(Action::BookmarkMode)
        );
        assert_eq!(
            Action::from_string("bookmark_slot_1"),
            Some(Action::BookmarkSlot(1))
        );
        assert_eq!(
            Action::from_string("bookmark_slot_9"),
            Some(Action::BookmarkSlot(9))
        );
        assert_eq!(
            Action::from_string("bookmark_1"),
            Some(Action::BookmarkSlot(1))
        );
        assert_eq!(
            Action::from_string("jump_to_bookmark_1"),
            Some(Action::BookmarkSlot(1))
        );
    }

    #[test]
    fn bookmark_actions_are_one_shot_not_continuous() {
        let actions = [
            Action::BookmarkMode,
            Action::BookmarkSlot(1),
            Action::ClearBookmarkSlot(1),
            Action::ClearAllBookmarks,
        ];
        for action in actions {
            assert!(!action.is_continuous(), "{action:?}");
            assert!(!action.is_movement(), "{action:?}");
            assert!(!action.is_wheel_direction(), "{action:?}");
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

    pub fn clear_runtime_input_state(&mut self) {
        self.clear_active_keys();
        self.mouse_master.release_left_button_if_held();
        self.mouse_master.surgical_zoom_state = Default::default();
    }

    pub fn tick_movement(&mut self) -> MovementTick {
        self.mouse_master
            .tick_movement(&self.active_keys, Duration::from_millis(0))
    }
}
