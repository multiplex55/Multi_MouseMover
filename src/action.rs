use std::collections::{HashMap, HashSet};

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

        // Compute resultant movement
        let mut dx = 0;
        let mut dy = 0;

        if self.active_keys.contains(&Action::MoveUp) {
            dy -= 10; // Move up
        }
        if self.active_keys.contains(&Action::MoveDown) {
            dy += 10; // Move down
        }
        if self.active_keys.contains(&Action::MoveLeft) {
            dx -= 10; // Move left
        }
        if self.active_keys.contains(&Action::MoveRight) {
            dx += 10; // Move right
        }

        // Apply the calculated movement
        if dx != 0 || dy != 0 {
            self.mouse_master.move_mouse(dx, dy);
        }
    }
}
