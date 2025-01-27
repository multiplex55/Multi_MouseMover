use crate::{action, Config};
use action::Action;
use enigo::*;

pub struct MouseMaster {
    enigo: Enigo,
    config: Config,
    current_mode: String,
}

impl MouseMaster {
    /// Creates a new `MouseMaster` instance
    pub fn new(config: Config) -> Self {
        Self {
            enigo: Enigo::new(&Settings::default()).unwrap(),
            config,
            current_mode: "default".to_string(),
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

    /// Simulates a left mouse click
    fn left_click(&mut self) {
        println!("Performing Left Click!");
        self.enigo.button(Button::Left, Direction::Click).unwrap();
    }

    /// Simulates a right mouse click
    fn right_click(&mut self) {
        println!("Performing Right Click!");
        self.enigo.button(Button::Right, Direction::Click).unwrap();
    }

    /// Moves the mouse by the given `dx` and `dy` offsets
    fn move_mouse(&mut self, dx: i32, dy: i32) {
        if let Ok((current_x, current_y)) = self.enigo.location() {
            self.enigo
                .move_mouse(current_x + dx, current_y + dy, Coordinate::Abs)
                .unwrap();
        } else {
            println!("Failed to retrieve mouse location.");
        }
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
        self.current_mode = mode.to_string();
        println!("Switched to mode: {}", mode);
        // FUTURE GROWTH
    }
}
