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
    Exit,
    SlowMouse,
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
            "exit" => Some(Self::Exit),
            "slow_mouse" => Some(Self::SlowMouse),
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
        // Add or remove the key from the active set
        if is_keydown {
            self.active_keys.insert(key);
        } else {
            self.active_keys.remove(&key);
        }

        // Reset dx and dy for movement tracking
        let mut dx: i32 = 0;
        let mut dy: i32 = 0;

        // Check if Shift is held
        let shift_held = self.active_keys.contains(&Action::SlowMouse);

        // Use starting speed if Shift is held, otherwise use current speed
        let mut speed: i32 = if shift_held {
            self.mouse_master.config.starting_speed
        } else {
            self.mouse_master.current_speed
        };

        // Determine movement directions
        let moving_up = self.active_keys.contains(&Action::MoveUp);
        let moving_down = self.active_keys.contains(&Action::MoveDown);
        let moving_left = self.active_keys.contains(&Action::MoveLeft);
        let moving_right = self.active_keys.contains(&Action::MoveRight);

        // Compute movement deltas based on key inputs
        if moving_up {
            dy -= 1;
        }
        if moving_down {
            dy += 1;
        }
        if moving_left {
            dx -= 1;
        }
        if moving_right {
            dx += 1;
        }

        // Track if there is movement
        let movement_detected = dx != 0 || dy != 0;

        if movement_detected {
            if !shift_held {
                // Only increase the acceleration counter if movement is sustained and Shift is NOT held
                self.mouse_master.acceleration_counter += 1;

                // Apply acceleration if enough polling cycles have passed
                if self.mouse_master.acceleration_counter
                    >= self.mouse_master.config.acceleration_rate
                {
                    speed += self.mouse_master.config.acceleration;
                    self.mouse_master.current_speed = speed; // Update current speed
                    self.mouse_master.acceleration_counter =
                        self.mouse_master.config.acceleration_rate / 2; // Reduce but don't reset
                }
            }

            // Scale movement by the current speed
            dx *= speed;
            dy *= speed;

            // Perform the mouse movement
            self.mouse_master.move_mouse(dx, dy);
        } else {
            // Reset speed and acceleration if no movement keys are active
            self.mouse_master.reset_speed();
        }

        // Collect non-movement actions to execute
        let non_movement_actions: Vec<Action> = self
            .active_keys
            .iter()
            .copied()
            .filter(|action| {
                !matches!(
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
            })
            .collect();

        // Execute collected non-movement actions
        for action in &non_movement_actions {
            self.execute_action(action);
        }

        // DEBUGGING OUTPUT
        println!(
    "[DEBUG] Keys: {:?} | DX: {} | DY: {} | Speed: {} | Accel_Counter: {} | Shift_Held: {} | Movement: {} | Non-Movement Actions: {:?}",
    self.active_keys,
    dx,
    dy,
    self.mouse_master.current_speed,
    self.mouse_master.acceleration_counter,
    shift_held,
    movement_detected,
    non_movement_actions
    );
    }
}
