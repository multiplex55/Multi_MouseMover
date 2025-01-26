use std::collections::HashMap;

/// Enum representing all possible actions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    LeftClick,
    RightClick,
    // Add more actions as needed
}

impl Action {
    /// Convert a string to an `Action` enum
    pub fn from_string(action: &str) -> Option<Self> {
        match action.to_lowercase().as_str() {
            "move_up" => Some(Self::MoveUp),
            "move_down" => Some(Self::MoveDown),
            "move_left" => Some(Self::MoveLeft),
            "move_right" => Some(Self::MoveRight),
            "left_click" => Some(Self::LeftClick),
            "right_click" => Some(Self::RightClick),
            _ => None,
        }
    }
}

/// Manages actions associated with key presses
pub struct ActionHandler {
    actions: HashMap<Action, Box<dyn Fn() + Send + Sync>>,
}

impl ActionHandler {
    /// Create a new ActionHandler
    pub fn new() -> Self {
        Self {
            actions: HashMap::new(),
        }
    }

    /// Add an action
    pub fn add_action<F>(&mut self, action: Action, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.actions.insert(action, Box::new(callback));
    }

    /// Execute an action by name
    pub fn execute_action(&self, action: &Action) {
        if let Some(callback) = self.actions.get(action) {
            callback();
        }
    }
}