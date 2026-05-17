use crate::monitor::{current_monitor_rect_for_cursor, MonitorEdge, MonitorRect};
use crate::overlay::OVERLAY;
use crate::{action, Config};
use action::Action;
use enigo::*;
use std::collections::HashSet;
use std::env;
use std::time::{Duration, Instant};

const DIAGONAL_NORMALIZATION: f64 = std::f64::consts::FRAC_1_SQRT_2;
const DEBUG_DIAGNOSTICS_ENV: &str = "MULTI_MOUSEMOVER_DEBUG";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementTick {
    pub dx: f64,
    pub dy: f64,
    pub speed: i32,
    pub moving: bool,
}

pub trait MouseBackend {
    fn click(&mut self, button: Button) -> Result<(), String>;
    fn move_abs(&mut self, x: i32, y: i32) -> Result<(), String>;
    fn location(&self) -> Result<(i32, i32), String>;
    fn scroll(&mut self, length: i32, axis: Axis) -> Result<(), String>;
}

pub struct EnigoMouseBackend {
    enigo: Enigo,
}

impl EnigoMouseBackend {
    fn new() -> Self {
        Self {
            enigo: Enigo::new(&Settings::default()).unwrap(),
        }
    }
}

impl MouseBackend for EnigoMouseBackend {
    fn click(&mut self, button: Button) -> Result<(), String> {
        self.enigo
            .button(button, Direction::Click)
            .map_err(|e| e.to_string())
    }

    fn move_abs(&mut self, x: i32, y: i32) -> Result<(), String> {
        self.enigo
            .move_mouse(x, y, Coordinate::Abs)
            .map_err(|e| e.to_string())
    }

    fn location(&self) -> Result<(i32, i32), String> {
        self.enigo.location().map_err(|e| e.to_string())
    }

    fn scroll(&mut self, length: i32, axis: Axis) -> Result<(), String> {
        self.enigo.scroll(length, axis).map_err(|e| e.to_string())
    }
}

pub struct MouseMaster<B: MouseBackend = EnigoMouseBackend> {
    pub backend: B,
    pub config: Config,
    pub current_mode: ModeState,
    pub current_speed: i32,
    pub current_wheel_speed: i32,
    pub acceleration_counter: u32,
    pub top_speed: i32,
    pub left_click_held: bool,
    last_wheel_tick: Option<Instant>,
}

#[derive(Debug, PartialEq)]
pub enum ModeState {
    Idle,   // Default state where no keybinds are processed
    Active, // Mode where keybinds are processed
}

impl MouseMaster<EnigoMouseBackend> {
    /// Creates a new `MouseMaster` instance
    pub fn new(config: Config) -> Self {
        Self::new_with_backend(config, EnigoMouseBackend::new())
    }
}

impl<B: MouseBackend> MouseMaster<B> {
    pub fn new_with_backend(config: Config, backend: B) -> Self {
        Self {
            backend,
            config: config.clone(),
            current_mode: ModeState::Active,
            current_speed: config.starting_speed,
            current_wheel_speed: config.wheel.default_speed,
            acceleration_counter: 0,
            top_speed: config.top_speed,
            left_click_held: false,
            last_wheel_tick: None,
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
            Action::MiddleClick => self.middle_click(),
            Action::MoveUpRight => self.move_mouse(10, -10),
            Action::MoveUpLeft => self.move_mouse(-10, -10),
            Action::MoveDownRight => self.move_mouse(10, 10),
            Action::MoveDownLeft => self.move_mouse(-10, 10),
            Action::MoveToTopEdge => self.move_to_monitor_edge(MonitorEdge::Top),
            Action::MoveToBottomEdge => self.move_to_monitor_edge(MonitorEdge::Bottom),
            Action::MoveToLeftEdge => self.move_to_monitor_edge(MonitorEdge::Left),
            Action::MoveToRightEdge => self.move_to_monitor_edge(MonitorEdge::Right),
            Action::CenterCurrentMonitor => self.center_current_monitor(),
            Action::ClickThenDisable => {
                self.left_click();
                self.current_mode = ModeState::Idle;
            }
            Action::WheelUp => self.wheel_up(),
            Action::WheelDown => self.wheel_down(),
            Action::WheelLeft => self.wheel_left(),
            Action::WheelRight => self.wheel_right(),
            Action::WheelSpeedUp => self.increase_wheel_speed(),
            Action::WheelSpeedDown => self.decrease_wheel_speed(),
            Action::Exit => self.exit(),
            Action::SlowMouse => {
                // println!("[DEBUG] SlowMouse triggered - No acceleration");
            }
            Action::JumpMode => {}
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

    /// Simulates a left mouse click
    fn left_click(&mut self) {
        println!("[DEBUG] Left Click Pressed!");
        self.left_click_held = true; // ✅ Update state
        self.update_overlay(); // ✅ Notify the overlay
        if let Err(e) = self.backend.click(Button::Left) {
            eprintln!("Failed to perform left click: {e}");
        }
    }

    /// Detect when left click is released
    #[allow(dead_code)]
    fn release_left_click(&mut self) {
        println!("[DEBUG] Left Click Released!");
        self.left_click_held = false; // ✅ Reset state
        self.update_overlay(); // ✅ Notify the overlay
    }

    /// Function to update the overlay window
    fn update_overlay(&self) {
        if let Some(ref mut ov) = *OVERLAY.lock().unwrap_or_else(|e| e.into_inner()) {
            ov.update_color(self.left_click_held);
        }
    }

    /// Simulates a right mouse click
    fn right_click(&mut self) {
        // println!("Performing Right Click!");
        if let Err(e) = self.backend.click(Button::Right) {
            eprintln!("Failed to perform right click: {e}");
        }
    }

    fn middle_click(&mut self) {
        if let Err(e) = self.backend.click(Button::Middle) {
            eprintln!("Failed to perform middle click: {e}");
        }
    }

    fn scroll_wheel(&mut self, length: i32, axis: Axis) {
        if let Err(e) = self.backend.scroll(length, axis) {
            eprintln!("Failed to scroll wheel: {e}");
        }
    }

    pub fn wheel_up(&mut self) {
        self.scroll_wheel(-self.current_wheel_speed, Axis::Vertical);
    }

    pub fn wheel_down(&mut self) {
        self.scroll_wheel(self.current_wheel_speed, Axis::Vertical);
    }

    pub fn wheel_left(&mut self) {
        self.scroll_wheel(-self.current_wheel_speed, Axis::Horizontal);
    }

    pub fn wheel_right(&mut self) {
        self.scroll_wheel(self.current_wheel_speed, Axis::Horizontal);
    }

    pub fn increase_wheel_speed(&mut self) {
        self.current_wheel_speed = (self.current_wheel_speed + self.config.wheel.speed_step)
            .min(self.config.wheel.max_speed);
    }

    pub fn decrease_wheel_speed(&mut self) {
        self.current_wheel_speed = (self.current_wheel_speed - self.config.wheel.speed_step)
            .max(self.config.wheel.min_speed);
    }

    pub fn tick_movement(
        &mut self,
        active_actions: &HashSet<Action>,
        _dt: Duration,
    ) -> MovementTick {
        let tick = calculate_movement(
            active_actions,
            &self.config,
            self.top_speed,
            &mut self.current_speed,
            &mut self.acceleration_counter,
        );

        if tick.moving {
            self.move_mouse(tick.dx.round() as i32, tick.dy.round() as i32);
        }

        self.tick_wheel(active_actions);

        if debug_diagnostics_enabled() {
            println!(
                "[DEBUG] Mode: {:?} | Active Keys: {:?} | DX: {:.3} | DY: {:.3} | Speed: {} | Accel_Counter: {} | Shift_Held: {} | Movement: {}",
                self.current_mode,
                active_actions,
                tick.dx,
                tick.dy,
                self.current_speed,
                self.acceleration_counter,
                active_actions.contains(&Action::SlowMouse),
                tick.moving
            );
        }

        tick
    }

    fn tick_wheel(&mut self, active_actions: &HashSet<Action>) {
        let Some(wheel_action) = active_actions
            .iter()
            .find(|action| action.is_wheel_direction())
            .copied()
        else {
            self.last_wheel_tick = None;
            return;
        };

        let now = Instant::now();
        let interval = Duration::from_millis(self.config.wheel.tick_interval);
        if self
            .last_wheel_tick
            .is_some_and(|last_tick| now.duration_since(last_tick) < interval)
        {
            return;
        }

        self.last_wheel_tick = Some(now);
        self.handle_action(wheel_action);
    }

    /// Moves the mouse by the given `dx` and `dy` offsets with immediate response.
    pub fn move_mouse(&mut self, dx: i32, dy: i32) {
        if dx == 0 && dy == 0 {
            self.reset_speed();
            return;
        }

        // Perform the mouse movement
        if let Ok((current_x, current_y)) = self.backend.location() {
            if let Err(e) = self.move_mouse_abs(current_x + dx, current_y + dy) {
                eprintln!("Failed to move mouse: {e}");
            }
        } else {
            println!("Failed to retrieve mouse location.");
        }
    }

    /// Moves the mouse cursor instantly to the given absolute position
    pub fn move_mouse_to(&mut self, x: i32, y: i32) {
        if let Err(e) = self.move_mouse_abs(x, y) {
            eprintln!("Failed to move mouse to position: {e}");
        }
    }

    fn move_mouse_abs(&mut self, x: i32, y: i32) -> Result<(), String> {
        self.backend.move_abs(x, y)
    }

    pub fn center_current_monitor(&mut self) {
        if let Some(rect) = current_monitor_rect_for_cursor() {
            let (x, y) = rect.center();
            self.move_mouse_to(x, y);
        }
    }

    pub fn move_to_monitor_edge(&mut self, edge: MonitorEdge) {
        if let Some(rect) = current_monitor_rect_for_cursor() {
            let (x, y) = edge_target(rect, edge);
            self.move_mouse_to(x, y);
        }
    }

    /// Resets the speed and acceleration counter when motion stops
    pub fn reset_speed(&mut self) {
        self.current_speed = self.config.starting_speed;
        self.acceleration_counter = 0;
        self.last_wheel_tick = None;
    }

    pub fn exit(&mut self) {
        println!("Exiting");
        std::process::exit(0)
    }

    /// Displays a grid on the screen (for future extensions)
    #[allow(dead_code)]
    pub fn display_grid(&self) {
        println!(
            "Displaying grid of size {}x{}",
            self.config.jump.coarse.width, self.config.jump.coarse.height
        );
        // FUTURE GROWTH
    }

    /// Switches to a different mode
    #[allow(dead_code)]
    pub fn switch_mode(&mut self, mode: &str) {
        if self.current_mode == ModeState::Active {
            self.current_mode = ModeState::Idle;
        } else {
            self.current_mode = ModeState::Active;
        }

        println!("Switched to mode: {}", mode);
        // FUTURE GROWTH
    }
}

pub fn edge_target(rect: MonitorRect, edge: MonitorEdge) -> (i32, i32) {
    rect.edge_midpoint(edge, 1)
}

fn debug_diagnostics_enabled() -> bool {
    env::var(DEBUG_DIAGNOSTICS_ENV)
        .map(|value| {
            matches!(
                value.as_str(),
                "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON"
            )
        })
        .unwrap_or(false)
}

fn calculate_movement(
    active_actions: &HashSet<Action>,
    config: &Config,
    top_speed: i32,
    current_speed: &mut i32,
    acceleration_counter: &mut u32,
) -> MovementTick {
    let mut dx = 0;
    let mut dy = 0;

    for &action in active_actions {
        match action {
            Action::MoveUp => dy -= 1,
            Action::MoveDown => dy += 1,
            Action::MoveLeft => dx -= 1,
            Action::MoveRight => dx += 1,
            Action::MoveUpRight => {
                dx += 1;
                dy -= 1;
            }
            Action::MoveUpLeft => {
                dx -= 1;
                dy -= 1;
            }
            Action::MoveDownRight => {
                dx += 1;
                dy += 1;
            }
            Action::MoveDownLeft => {
                dx -= 1;
                dy += 1;
            }
            _ => {}
        }
    }

    if dx == 0 && dy == 0 {
        *current_speed = config.starting_speed;
        *acceleration_counter = 0;
        return MovementTick {
            dx: 0.0,
            dy: 0.0,
            speed: *current_speed,
            moving: false,
        };
    }

    let speed = if active_actions.contains(&Action::SlowMouse) {
        config.starting_speed
    } else {
        advance_speed(config, top_speed, current_speed, acceleration_counter)
    };

    let mut scaled_dx = f64::from(dx * speed);
    let mut scaled_dy = f64::from(dy * speed);

    if dx != 0 && dy != 0 {
        scaled_dx *= DIAGONAL_NORMALIZATION;
        scaled_dy *= DIAGONAL_NORMALIZATION;
    }

    MovementTick {
        dx: scaled_dx,
        dy: scaled_dy,
        speed,
        moving: true,
    }
}

fn advance_speed(
    config: &Config,
    top_speed: i32,
    current_speed: &mut i32,
    acceleration_counter: &mut u32,
) -> i32 {
    *acceleration_counter += 1;

    if *acceleration_counter >= config.acceleration_rate {
        *current_speed += config.acceleration;
        *acceleration_counter = 0;
    }

    *current_speed = (*current_speed).min(top_speed);
    *current_speed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        Config {
            starting_speed: 2,
            acceleration: 3,
            acceleration_rate: 2,
            top_speed: 10,
            ..Config::default()
        }
    }

    #[derive(Default)]
    struct FakeBackend {
        location: (i32, i32),
        clicks: Vec<Button>,
        moves: Vec<(i32, i32)>,
        scrolls: Vec<(i32, Axis)>,
    }

    impl MouseBackend for FakeBackend {
        fn click(&mut self, button: Button) -> Result<(), String> {
            self.clicks.push(button);
            Ok(())
        }

        fn move_abs(&mut self, x: i32, y: i32) -> Result<(), String> {
            self.moves.push((x, y));
            self.location = (x, y);
            Ok(())
        }

        fn location(&self) -> Result<(i32, i32), String> {
            Ok(self.location)
        }

        fn scroll(&mut self, length: i32, axis: Axis) -> Result<(), String> {
            self.scrolls.push((length, axis));
            Ok(())
        }
    }

    fn actions(actions: &[Action]) -> HashSet<Action> {
        actions.iter().copied().collect()
    }

    fn tick(
        active_actions: &HashSet<Action>,
        config: &Config,
        current_speed: &mut i32,
        acceleration_counter: &mut u32,
    ) -> MovementTick {
        calculate_movement(
            active_actions,
            config,
            config.top_speed,
            current_speed,
            acceleration_counter,
        )
    }

    #[test]
    fn diagonal_normalization_preserves_cardinal_magnitude() {
        let config = test_config();
        let mut current_speed = config.starting_speed;
        let mut acceleration_counter = 0;
        let active_actions = actions(&[Action::MoveUp, Action::MoveRight]);

        let movement = tick(
            &active_actions,
            &config,
            &mut current_speed,
            &mut acceleration_counter,
        );

        let magnitude = (movement.dx.powi(2) + movement.dy.powi(2)).sqrt();

        assert!((magnitude - f64::from(config.starting_speed)).abs() < f64::EPSILON);
    }

    #[test]
    fn acceleration_progresses_over_ticks() {
        let config = test_config();
        let mut current_speed = config.starting_speed;
        let mut acceleration_counter = 0;
        let active_actions = actions(&[Action::MoveRight]);

        let first = tick(
            &active_actions,
            &config,
            &mut current_speed,
            &mut acceleration_counter,
        );
        let second = tick(
            &active_actions,
            &config,
            &mut current_speed,
            &mut acceleration_counter,
        );

        assert_eq!(first.speed, config.starting_speed);
        assert_eq!(second.speed, config.starting_speed + config.acceleration);
    }

    #[test]
    fn speed_is_clamped_to_top_speed() {
        let config = Config {
            starting_speed: 4,
            acceleration: 4,
            acceleration_rate: 1,
            top_speed: 6,
            ..Config::default()
        };
        let mut current_speed = config.starting_speed;
        let mut acceleration_counter = 0;
        let active_actions = actions(&[Action::MoveRight]);

        let movement = tick(
            &active_actions,
            &config,
            &mut current_speed,
            &mut acceleration_counter,
        );

        assert_eq!(movement.speed, config.top_speed);
        assert_eq!(current_speed, config.top_speed);
    }

    #[test]
    fn speed_resets_on_idle() {
        let config = test_config();
        let mut current_speed = 9;
        let mut acceleration_counter = 1;
        let active_actions = HashSet::new();

        let movement = tick(
            &active_actions,
            &config,
            &mut current_speed,
            &mut acceleration_counter,
        );

        assert!(!movement.moving);
        assert_eq!(movement.speed, config.starting_speed);
        assert_eq!(current_speed, config.starting_speed);
        assert_eq!(acceleration_counter, 0);
    }

    #[test]
    fn wheel_speed_increase_clamps_to_max() {
        let config = Config {
            wheel: crate::WheelConfig {
                default_speed: 9,
                min_speed: 1,
                max_speed: 10,
                speed_step: 4,
                tick_interval: 8,
            },
            ..Config::default()
        };
        let mut mouse = MouseMaster::new_with_backend(config, FakeBackend::default());

        mouse.increase_wheel_speed();

        assert_eq!(mouse.current_wheel_speed, 10);
    }

    #[test]
    fn wheel_speed_decrease_clamps_to_min() {
        let config = Config {
            wheel: crate::WheelConfig {
                default_speed: 2,
                min_speed: 1,
                max_speed: 10,
                speed_step: 4,
                tick_interval: 8,
            },
            ..Config::default()
        };
        let mut mouse = MouseMaster::new_with_backend(config, FakeBackend::default());

        mouse.decrease_wheel_speed();

        assert_eq!(mouse.current_wheel_speed, 1);
    }

    #[test]
    fn handle_action_routes_clicks_and_wheel_to_backend() {
        let mut mouse = MouseMaster::new_with_backend(test_config(), FakeBackend::default());

        mouse.handle_action(Action::MiddleClick);
        mouse.handle_action(Action::WheelUp);
        mouse.handle_action(Action::WheelRight);

        assert_eq!(mouse.backend.clicks, vec![Button::Middle]);
        assert_eq!(
            mouse.backend.scrolls,
            vec![
                (-mouse.current_wheel_speed, Axis::Vertical),
                (mouse.current_wheel_speed, Axis::Horizontal)
            ]
        );
    }

    #[test]
    fn handle_action_routes_relative_movement_as_absolute_backend_move() {
        let mut backend = FakeBackend::default();
        backend.location = (100, 200);
        let mut mouse = MouseMaster::new_with_backend(test_config(), backend);

        mouse.handle_action(Action::MoveDownRight);

        assert_eq!(mouse.backend.moves, vec![(110, 210)]);
    }

    #[test]
    fn edge_target_coordinates_use_one_pixel_monitor_inset() {
        let rect = MonitorRect {
            left: 10,
            top: 20,
            right: 210,
            bottom: 120,
        };

        assert_eq!(edge_target(rect, MonitorEdge::Top), (110, 21));
        assert_eq!(edge_target(rect, MonitorEdge::Bottom), (110, 118));
        assert_eq!(edge_target(rect, MonitorEdge::Left), (11, 70));
        assert_eq!(edge_target(rect, MonitorEdge::Right), (208, 70));
    }
}
