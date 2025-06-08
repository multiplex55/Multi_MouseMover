use crate::overlay::OVERLAY;
use crate::{action, Config};
use action::Action;
use enigo::*;

pub struct MouseMaster {
    pub enigo: Enigo,
    pub config: Config,
    pub current_mode: ModeState,
    pub current_speed: i32,
    pub acceleration_counter: u32,
    pub top_speed: i32,
    pub left_click_held: bool,
}

#[derive(Debug, PartialEq)]
pub enum ModeState {
    Idle,   // Default state where no keybinds are processed
    Active, // Mode where keybinds are processed
}

impl MouseMaster {
    /// Creates a new `MouseMaster` instance
    pub fn new(config: Config) -> Self {
        Self {
            enigo: Enigo::new(&Settings::default()).unwrap(),
            config: config.clone(),
            current_mode: ModeState::Active,
            current_speed: config.starting_speed,
            acceleration_counter: 0,
            top_speed: config.top_speed,
            left_click_held: false,
        }
    }

    /// Handles an action and executes the corresponding behavior
    pub fn handle_action(&mut self, action: Action) {
        match action {
            Action::MoveUp => self.move_up(),
            Action::MoveDown => self.move_down(),
            Action::MoveLeft => self.move_left(),
            Action::MoveRight => self.move_right(),
            Action::LeftClick => self.left_click(),
            Action::RightClick => self.right_click(),
            Action::MoveUpRight => self.move_up_right(),
            Action::MoveUpLeft => self.move_up_left(),
            Action::MoveDownRight => self.move_down_right(),
            Action::MoveDownLeft => self.move_down_left(),
            Action::Exit => self.exit(),
            Action::SlowMouse => {
                // println!("[DEBUG] SlowMouse triggered - No acceleration");
            }
            Action::JumpMode => self.activate_jump_mode(),
        }
    }
    /// Toggles between `Idle` and `Active` mode
    pub fn toggle_mode(&mut self) {
        if self.current_mode == ModeState::Active {
            self.current_mode = ModeState::Idle;
            println!("Switched to: Idle Mode");
        } else {
            self.current_mode = ModeState::Active;
            println!("Switched to: Active Mode");
        }
    }

    /// Moves the mouse up
    fn move_up(&mut self) {
        self.move_mouse(0, -10);
        println!("Moving up!");
    }

    /// Moves the mouse down
    fn move_down(&mut self) {
        self.move_mouse(0, 10);
        println!("Moving down!");
    }

    /// Moves the mouse left
    fn move_left(&mut self) {
        self.move_mouse(-10, 0);
        println!("Moving left!");
    }

    /// Moves the mouse right
    fn move_right(&mut self) {
        self.move_mouse(10, 0);
        println!("Moving right!");
    }

    /// Moves the mouse up-right
    fn move_up_right(&mut self) {
        self.move_mouse(10, -10);
        println!("Moving up-right!");
    }

    /// Moves the mouse up-right
    fn move_up_left(&mut self) {
        self.move_mouse(-10, -10);
        println!("Moving up-left");
    }

    /// Moves the mouse down-left
    fn move_down_right(&mut self) {
        self.move_mouse(10, 10);
        println!("Moving down-right!");
    }

    /// Moves the mouse down-left
    fn move_down_left(&mut self) {
        self.move_mouse(-10, 10);
        println!("Moving down-left!");
    }

    /// Simulates a left mouse click
    fn left_click(&mut self) {
        println!("[DEBUG] Left Click Pressed!");
        self.left_click_held = true; // ✅ Update state
        self.update_overlay(); // ✅ Notify the overlay
        self.enigo.button(Button::Left, Direction::Click).unwrap();
    }

    /// Detect when left click is released
    fn release_left_click(&mut self) {
        println!("[DEBUG] Left Click Released!");
        self.left_click_held = false; // ✅ Reset state
        self.update_overlay(); // ✅ Notify the overlay
    }

    /// Function to update the overlay window
    fn update_overlay(&self) {
        OVERLAY.lock().unwrap().update_color(self.left_click_held);
    }

    /// Simulates a right mouse click
    fn right_click(&mut self) {
        // println!("Performing Right Click!");
        self.enigo.button(Button::Right, Direction::Click).unwrap();
    }

    /// Moves the mouse by the given `dx` and `dy` offsets with immediate response
    pub fn move_mouse(&mut self, dx: i32, dy: i32) {
        // If no movement, reset speed & acceleration
        if dx == 0 && dy == 0 {
            self.reset_speed();
            return;
        }

        self.acceleration_counter += 1;

        // Apply acceleration only after enough polling cycles
        if self.acceleration_counter >= self.config.acceleration_rate {
            self.current_speed += self.config.acceleration;
            self.acceleration_counter = 0;
        }

        // Calculate the actual movement based on the current speed
        let actual_dx = dx * self.current_speed;
        let actual_dy = dy * self.current_speed;

        // Perform the mouse movement
        if let Ok((current_x, current_y)) = self.enigo.location() {
            self.enigo
                .move_mouse(
                    current_x + actual_dx,
                    current_y + actual_dy,
                    Coordinate::Abs,
                )
                .unwrap();
        } else {
            println!("Failed to retrieve mouse location.");
        }
    }

    /// Resets the speed and acceleration counter when motion stops
    pub fn reset_speed(&mut self) {
        self.current_speed = self.config.starting_speed;
        self.acceleration_counter = 0;
    }

    pub fn exit(&mut self) {
        println!("Exiting");
        std::process::exit(0)
    }

    /// Displays a grid on the screen (for future extensions)
    pub fn display_grid(&self) {
        println!(
            "Displaying grid of size {}x{}",
            self.config.grid_size.width, self.config.grid_size.height
        );
        // FUTURE GROWTH
    }

    /// Switches to a different mode
    pub fn switch_mode(&mut self, mode: &str) {
        if (self.current_mode == ModeState::Active) {
            self.current_mode = ModeState::Idle;
        } else {
            self.current_mode = ModeState::Active;
        }

        println!("Switched to mode: {}", mode);
        // FUTURE GROWTH
    }

    /// Activates jump mode
    fn activate_jump_mode(&mut self) {
        println!("Jump Mode Activated!");
    }
}
