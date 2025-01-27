use std::collections::{HashMap, HashSet};

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
            "move_up_right" => Some(Self::MoveUpRight),
            "move_up_left" => Some(Self::MoveUpLeft),
            "move_down_right" => Some(Self::MoveDownRight),

            "left_click" => Some(Self::LeftClick),
            "right_click" => Some(Self::RightClick),
            _ => None,
        }
    }
}

/// Manages actions associated with key presses
pub struct ActionHandler {
    actions: HashMap<Action, Box<dyn Fn() + Send + Sync>>,
    active_keys: HashSet<Action>, // Tracks currently held actions
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
    /// Track key presses and compute movement based on active keys
    pub fn process_active_keys(&mut self, key: Action, is_keydown: bool) {
        if is_keydown {
            self.active_keys.insert(key); // Add key to active set
        } else {
            self.active_keys.remove(&key); // Remove key from active set
        }

        // Check for combinations first, fallback to individual keys
        let mut actions_to_execute = Vec::new();

        if self.active_keys.contains(&Action::MoveUp)
            && self.active_keys.contains(&Action::MoveRight)
        {
            actions_to_execute.push(Action::MoveUpRight);
        } else if self.active_keys.contains(&Action::MoveUp)
            && self.active_keys.contains(&Action::MoveLeft)
        {
            actions_to_execute.push(Action::MoveUpLeft);
        } else if self.active_keys.contains(&Action::MoveDown)
            && self.active_keys.contains(&Action::MoveRight)
        {
            actions_to_execute.push(Action::MoveDownRight);
        } else if self.active_keys.contains(&Action::MoveDown)
            && self.active_keys.contains(&Action::MoveLeft)
        {
            actions_to_execute.push(Action::MoveDownLeft);
        }

        // If no combinations are detected, execute all active single-direction actions
        if actions_to_execute.is_empty() {
            actions_to_execute.extend(self.active_keys.iter().copied());
        }

        // Execute the collected actions
        for action in actions_to_execute {
            self.execute_action(&action);
        }
    }
}
