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
            Action::MoveUp => self.move_mouse(0, -10),
            Action::MoveDown => self.move_mouse(0, 10),
            Action::MoveLeft => self.move_mouse(-10, 0),
            Action::MoveRight => self.move_mouse(10, 0),
            Action::LeftClick => self.left_click(),
            Action::RightClick => self.right_click(),
        }
    }

    /// Moves the mouse by the given `dx` and `dy` offsets
    pub fn move_mouse(&mut self, dx: i32, dy: i32) {
        if let Ok((current_x, current_y)) = self.enigo.location() {
            self.enigo
                .move_mouse(current_x + dx, current_y + dy, Coordinate::Abs)
                .unwrap();
        } else {
            println!("Failed to retrieve mouse location.");
        }
    }

    /// Simulates a left mouse click
    pub fn left_click(&mut self) {
        println!("Performing Left Click!");
        self.enigo.button(Button::Left, Direction::Click).unwrap();
    }

    /// Simulates a right mouse click
    pub fn right_click(&mut self) {
        println!("Performing Right Click!");
        self.enigo.button(Button::Right, Direction::Click).unwrap();
    }

    /// Displays a grid on the screen (for future extensions)
    pub fn display_grid(&self) {
        println!(
            "Displaying grid of size {}x{}",
            self.config.grid_size.width, self.config.grid_size.height
        );
    }

    /// Switches to a different mode
    pub fn switch_mode(&mut self, mode: &str) {
        self.current_mode = mode.to_string();
        println!("Switched to mode: {}", mode);
    }
}
