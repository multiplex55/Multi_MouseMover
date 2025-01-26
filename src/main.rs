use enigo::*;
use rdev::{listen, EventType, Key as RdevKey};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Deserialize)]
struct Config {
    key_bindings: HashMap<String, String>,
    grid_size: GridSize, // Use GridSize struct
    #[serde(default)] // Provide a default value if the field is missing
    smooth_jump_speed: f32,
}

#[derive(Debug, Deserialize)]
struct GridSize {
    width: u32,
    height: u32,
}

impl Config {
    /// Load the configuration from a TOML file
    fn load_from_file(path: &str) -> Self {
        let config_str = fs::read_to_string(path).expect("Failed to read config file");
        toml::from_str(&config_str).expect("Failed to parse config file")
    }
}

struct MouseMaster {
    enigo: Enigo,
    config: Config,
    current_mode: String,
}

impl MouseMaster {
    fn new(config: Config) -> Self {
        Self {
            enigo: Enigo::new(&Settings::default()).unwrap(),
            config,
            current_mode: "default".to_string(),
        }
    }

    fn handle_keyboard_input(&mut self, key: RdevKey) {
        if let Some(action) = self.config.key_bindings.get(&format!("{:?}", key)) {
            match action.as_str() {
                "move_up" => self.move_mouse(0, -10),
                "move_down" => self.move_mouse(0, 10),
                "move_left" => self.move_mouse(-10, 0),
                "move_right" => self.move_mouse(10, 0),
                "left_click" => self
                    .enigo
                    .button(enigo::Button::Left, enigo::Direction::Press)
                    .unwrap(),
                "right_click" => self
                    .enigo
                    .button(enigo::Button::Right, enigo::Direction::Press)
                    .unwrap(),
                _ => eprintln!("Unknown action: {}", action),
            }
        }
    }

    fn move_mouse(&mut self, dx: i32, dy: i32) {
        let (current_x, current_y) = self.enigo.location().unwrap();
        self.enigo
            .move_mouse(current_x + dx, current_y + dy, enigo::Coordinate::Abs);
    }

    fn display_grid(&self) {
        println!(
            "Displaying grid of size {}x{}",
            self.config.grid_size.width, self.config.grid_size.height
        );
    }

    fn switch_mode(&mut self, mode: &str) {
        self.current_mode = mode.to_string();
        println!("Switched to mode: {}", mode);
    }
}

fn main() {
    // Load the configuration file
    let config = Config::load_from_file("config.toml");
    let mut mouse_master = MouseMaster::new(config);

    println!("MouseMaster is running. Press CTRL+C to exit.");

    if let Err(error) = listen(move |event| {
        if let EventType::KeyPress(key) = event.event_type {
            mouse_master.handle_keyboard_input(key);
        }
    }) {
        eprintln!("Error: {:?}", error);
    }
}
