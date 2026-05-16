use crate::action_handler::MovementTick;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// Enum representing all possible actions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    MoveUpRight,
    MoveUpLeft,
    MoveDownRight,
    MoveDownLeft,
    LeftClick,
    RightClick,
    Exit,
    SlowMouse,
    JumpMode,
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
            "left_click" => Some(Self::LeftClick),
            "right_click" => Some(Self::RightClick),
            "exit" => Some(Self::Exit),
            "slow_mouse" => Some(Self::SlowMouse),
            "jump_mode" => Some(Self::JumpMode),
            _ => None,
        }
    }
}

/// Manages actions associated with key presses
pub struct ActionHandler {
    pub actions: HashMap<Action, Box<dyn Fn() + Send + Sync>>,
    pub active_keys: HashSet<Action>, // Tracks currently held actions
    pub mouse_master: crate::action_handler::MouseMaster, // Reference to MouseMaster
}

impl ActionHandler {
    /// Create a new ActionHandler
    pub fn new(mouse_master: crate::action_handler::MouseMaster) -> Self {
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
            self.mouse_master.handle_action(*action);
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

    pub fn is_movement_action(action: Action) -> bool {
        matches!(
            action,
            Action::MoveUp
                | Action::MoveDown
                | Action::MoveLeft
                | Action::MoveRight
                | Action::MoveUpRight
                | Action::MoveUpLeft
                | Action::MoveDownRight
                | Action::MoveDownLeft
        )
    }
}
