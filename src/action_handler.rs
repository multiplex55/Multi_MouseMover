use crate::monitor::{
    all_monitor_rects, current_monitor_rect_for_cursor, monitor_containing_point,
    next_monitor_with_wrap, MonitorEdge, MonitorRect,
};
use crate::{
    action,
    app_state::KeyEvent,
    keyboard::VirtualKey,
    window_geometry::{foreground_window_snap_points, WindowSnapPoints},
    zoom_overlay::{update_zoom_state, SurgicalZoomConfig, SurgicalZoomState},
    Config, FinalAdjustConfig,
};
use action::{Action, Direction2D, StepMoveTier};
use enigo::*;
use std::collections::{HashSet, VecDeque};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MouseRuntimeSnapshot {
    pub mode: String,
    pub drag_active: bool,
    pub movement_profile: Option<String>,
    pub wheel_profile: Option<String>,
    pub mouse_speed_current: i32,
    pub mouse_speed_default: i32,
    pub mouse_speed_min: i32,
    pub mouse_speed_max: i32,
    pub mouse_speed_step: i32,
    pub wheel_speed_current: i32,
    pub wheel_speed_default: i32,
    pub wheel_speed_min: i32,
    pub wheel_speed_max: i32,
    pub wheel_speed_step: i32,
    pub acceleration: i32,
    pub acceleration_rate: u32,
    pub slow_strategy: String,
    pub slow_effective_speed: i32,
    pub slow_min_speed: i32,
    pub slow_max_speed: i32,
    pub slow_acceleration: i32,
    pub slow_acceleration_rate: u32,
    pub top_speed: i32,
    pub polling_rate_ms: u64,
    pub wheel_tick_interval_ms: u64,
    pub wheel_vertical_multiplier: i32,
    pub wheel_horizontal_multiplier: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeNotificationKind {
    MouseSpeed,
    WheelSpeed,
    MovementProfile,
    WheelProfile,
    Drag,
    ConfigReload,
    PanicReset,
    StepMove,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeNotification {
    pub kind: RuntimeNotificationKind,
    pub title: String,
    pub body: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalAdjustControl {
    Nudge { dx: i32, dy: i32 },
    Confirm,
    Cancel,
    Back,
}

pub trait MouseBackend {
    fn click(&mut self, button: Button) -> Result<(), String>;
    fn button_down(&mut self, button: Button) -> Result<(), String>;
    fn button_up(&mut self, button: Button) -> Result<(), String>;
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

    fn button_down(&mut self, button: Button) -> Result<(), String> {
        self.enigo
            .button(button, Direction::Press)
            .map_err(|e| e.to_string())
    }

    fn button_up(&mut self, button: Button) -> Result<(), String> {
        self.enigo
            .button(button, Direction::Release)
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
    pub mouse_speed_baseline: i32,
    pub current_speed: i32,
    pub current_wheel_speed: i32,
    pub active_movement_profile: Option<String>,
    pub active_wheel_profile: Option<String>,
    pub effective_mouse_speed: crate::MouseSpeedConfig,
    pub effective_wheel: crate::WheelConfig,
    pub acceleration_counter: u32,
    pub top_speed_behavior: TopSpeedBehavior,
    pub left_button_held: bool,
    pub surgical_zoom_state: SurgicalZoomState,
    last_wheel_tick: Option<Instant>,
    last_wheel_direction: Option<WheelDirection>,
    mouse_speed_flash_until: Option<Instant>,
    wheel_speed_flash_until: Option<Instant>,
    pending_notifications: VecDeque<RuntimeNotification>,
    monitor_rects_provider: fn(bool) -> Vec<MonitorRect>,
    window_snap_points_provider: fn(bool, i32, i32, bool) -> Option<WindowSnapPoints>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WheelDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopSpeedBehavior {
    acceleration_headroom: i32,
}

impl TopSpeedBehavior {
    fn from_config(config: &Config) -> Self {
        Self {
            // Preserve legacy top_speed semantics when the configured baseline moves:
            // top_speed contributes ramp headroom above [mouse_speed].default_speed.
            acceleration_headroom: (config.top_speed - config.starting_speed).max(0),
        }
    }

    fn top_speed_for_baseline(self, baseline: i32) -> i32 {
        baseline + self.acceleration_headroom
    }
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
            mouse_speed_baseline: config.mouse_speed.default_speed,
            current_speed: config.mouse_speed.default_speed,
            current_wheel_speed: config.wheel.default_speed,
            active_movement_profile: None,
            active_wheel_profile: None,
            effective_mouse_speed: config.mouse_speed,
            effective_wheel: config.wheel,
            acceleration_counter: 0,
            top_speed_behavior: TopSpeedBehavior::from_config(&config),
            left_button_held: false,
            surgical_zoom_state: SurgicalZoomState::default(),
            last_wheel_tick: None,
            last_wheel_direction: None,
            mouse_speed_flash_until: None,
            wheel_speed_flash_until: None,
            pending_notifications: VecDeque::new(),
            monitor_rects_provider: all_monitor_rects,
            window_snap_points_provider: foreground_window_snap_points,
        }
    }

    #[cfg(test)]
    fn set_monitor_rects_provider(&mut self, provider: fn(bool) -> Vec<MonitorRect>) {
        self.monitor_rects_provider = provider;
    }

    #[cfg(test)]
    fn set_window_snap_points_provider(
        &mut self,
        provider: fn(bool, i32, i32, bool) -> Option<WindowSnapPoints>,
    ) {
        self.window_snap_points_provider = provider;
    }

    pub fn runtime_snapshot(&self) -> MouseRuntimeSnapshot {
        MouseRuntimeSnapshot {
            mode: format!("{:?}", self.current_mode).to_lowercase(),
            drag_active: self.left_button_held,
            movement_profile: self.active_movement_profile.clone(),
            wheel_profile: self.active_wheel_profile.clone(),
            mouse_speed_current: self.mouse_speed_baseline,
            mouse_speed_default: self.effective_mouse_speed.default_speed,
            mouse_speed_min: self.effective_mouse_speed.min_speed,
            mouse_speed_max: self.effective_mouse_speed.max_speed,
            mouse_speed_step: self.effective_mouse_speed.speed_step,
            wheel_speed_current: self.current_wheel_speed,
            wheel_speed_default: self.effective_wheel.default_speed,
            wheel_speed_min: self.effective_wheel.min_speed,
            wheel_speed_max: self.effective_wheel.max_speed,
            wheel_speed_step: self.effective_wheel.speed_step,
            acceleration: self.config.acceleration,
            acceleration_rate: self.config.acceleration_rate,
            slow_strategy: format!("{:?}", self.config.slow_mouse.strategy).to_lowercase(),
            slow_effective_speed: effective_slow_speed(&self.config, self.mouse_speed_baseline),
            slow_min_speed: self.config.slow_mouse.min_speed,
            slow_max_speed: self.config.slow_mouse.max_speed,
            slow_acceleration: self.config.slow_mouse.acceleration,
            slow_acceleration_rate: self.config.slow_mouse.acceleration_rate,
            top_speed: self
                .top_speed_behavior
                .top_speed_for_baseline(self.mouse_speed_baseline),
            polling_rate_ms: self.config.polling_rate,
            wheel_tick_interval_ms: self.effective_wheel.tick_interval,
            wheel_vertical_multiplier: self.effective_wheel.vertical_multiplier,
            wheel_horizontal_multiplier: self.effective_wheel.horizontal_multiplier,
        }
    }

    pub fn push_notification(&mut self, notification: RuntimeNotification) {
        self.pending_notifications.push_back(notification);
    }

    pub fn take_notifications(&mut self) -> Vec<RuntimeNotification> {
        self.pending_notifications.drain(..).collect()
    }

    /// Handles an action and executes the corresponding behavior
    pub fn handle_action(&mut self, action: Action) {
        match action {
            Action::MoveUp => self.move_mouse(0, -10),
            Action::MoveDown => self.move_mouse(0, 10),
            Action::MoveLeft => self.move_mouse(-10, 0),
            Action::MoveRight => self.move_mouse(10, 0),
            Action::LeftClick => {
                if self.left_button_held() {
                    self.release_left_button_if_held();
                } else {
                    self.left_click();
                }
            }
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
            Action::MoveToWindowTopEdge => self.move_to_window_snap(|p| p.top_edge),
            Action::MoveToWindowBottomEdge => self.move_to_window_snap(|p| p.bottom_edge),
            Action::MoveToWindowLeftEdge => self.move_to_window_snap(|p| p.left_edge),
            Action::MoveToWindowRightEdge => self.move_to_window_snap(|p| p.right_edge),
            Action::MoveToWindowCenter => self.move_to_window_snap(|p| p.center),
            Action::MoveToWindowTitlebar => self.move_to_window_snap(|p| p.titlebar),
            Action::ScreenSelect => self.select_next_screen(),
            Action::StepMove { direction, tier } => self.step_move(direction, tier),
            Action::ClickThenDisable => {
                self.release_left_button_if_held();
                self.left_click();
                self.set_active_mode(false);
            }
            Action::ToggleDragMode => self.toggle_drag_mode(),
            Action::WheelUp => self.wheel_up(),
            Action::WheelDown => self.wheel_down(),
            Action::WheelLeft => self.wheel_left(),
            Action::WheelRight => self.wheel_right(),
            Action::WheelSpeedUp => self.increase_wheel_speed(),
            Action::WheelSpeedDown => self.decrease_wheel_speed(),
            Action::WheelSpeedReset => self.reset_wheel_speed_tier(),
            Action::WheelProfileNext => {
                self.select_next_wheel_profile();
            }
            Action::WheelProfilePrevious => {
                self.select_previous_wheel_profile();
            }
            Action::WheelProfileSelect(profile) => {
                self.select_wheel_profile(&profile);
            }
            Action::MouseSpeedUp => self.increase_mouse_speed(),
            Action::MouseSpeedDown => self.decrease_mouse_speed(),
            Action::MouseSpeedReset => self.reset_mouse_speed_tier(),
            Action::MovementProfileNext => {
                self.select_next_movement_profile();
            }
            Action::MovementProfilePrevious => {
                self.select_previous_movement_profile();
            }
            Action::MovementProfileSelect(profile) => {
                self.select_movement_profile(&profile);
            }
            Action::Exit => self.exit(),
            Action::SlowMouse => {
                // println!("[DEBUG] SlowMouse triggered - No acceleration");
            }
            Action::SurgicalMode => {}
            Action::ScrollModifier => {}
            Action::ReloadConfig => self.push_config_reload_notification(),
            Action::JumpMode
            | Action::JumpModeProfile(_)
            | Action::GridMode
            | Action::NavigateBack
            | Action::NavigateForward
            | Action::ShowHelp
            | Action::UiHintMode
            | Action::SaveMousePosition
            | Action::ClearMousePositions
            | Action::PositionHistoryMode
            | Action::BookmarkMode
            | Action::BookmarkSlot(_)
            | Action::ClearBookmarkSlot(_)
            | Action::ClearAllBookmarks => {}
            Action::Disable => self.set_active_mode(false),
            Action::PanicReset => {
                self.hard_reset_runtime();
                self.push_panic_reset_notification();
            }
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

    pub fn set_active_mode(&mut self, active: bool) {
        if !active {
            self.release_left_button_if_held();
        }

        self.current_mode = if active {
            ModeState::Active
        } else {
            ModeState::Idle
        };
    }

    /// Simulates a left mouse click
    fn left_click(&mut self) {
        println!("[DEBUG] Left Click Pressed!");
        if let Err(e) = self.backend.click(Button::Left) {
            eprintln!("Failed to perform left click: {e}");
        }
    }
    pub fn left_click_once(&mut self) {
        self.left_click();
    }

    pub fn left_button_held(&self) -> bool {
        self.left_button_held
    }

    pub fn toggle_drag_mode(&mut self) {
        let was_held = self.left_button_held;
        if self.left_button_held {
            self.release_left_button_if_held();
        } else {
            self.press_left_button_for_drag();
        }
        if self.left_button_held != was_held {
            self.push_drag_notification();
        }
    }

    pub fn press_left_button_for_drag(&mut self) {
        if self.left_button_held {
            return;
        }

        if let Err(e) = self.backend.button_down(Button::Left) {
            eprintln!("Failed to hold left button: {e}");
            return;
        }
        self.left_button_held = true;
    }

    pub fn release_left_button_if_held(&mut self) {
        if !self.left_button_held {
            return;
        }

        if let Err(e) = self.backend.button_up(Button::Left) {
            eprintln!("Failed to release left button: {e}");
            return;
        }
        self.left_button_held = false;
    }

    /// Simulates a right mouse click
    fn right_click(&mut self) {
        // println!("Performing Right Click!");
        if let Err(e) = self.backend.click(Button::Right) {
            eprintln!("Failed to perform right click: {e}");
        }
    }
    pub fn right_click_once(&mut self) {
        self.right_click();
    }

    fn middle_click(&mut self) {
        if let Err(e) = self.backend.click(Button::Middle) {
            eprintln!("Failed to perform middle click: {e}");
        }
    }
    pub fn middle_click_once(&mut self) {
        self.middle_click();
    }

    fn scroll_wheel(&mut self, length: i32, axis: Axis) {
        if let Err(e) = self.backend.scroll(length, axis) {
            eprintln!("Failed to scroll wheel: {e}");
        }
    }

    fn wheel_units_for_direction(&self, direction: WheelDirection) -> i32 {
        let axis_multiplier = match direction {
            WheelDirection::Up | WheelDirection::Down => self.effective_wheel.vertical_multiplier,
            WheelDirection::Left | WheelDirection::Right => self.effective_wheel.horizontal_multiplier,
        };
        let signed_multiplier = match direction {
            WheelDirection::Up | WheelDirection::Left => -axis_multiplier,
            WheelDirection::Down | WheelDirection::Right => axis_multiplier,
        };
        self.current_wheel_speed.saturating_mul(signed_multiplier)
    }

    fn dispatch_wheel_direction(&mut self, direction: WheelDirection) {
        let axis = match direction {
            WheelDirection::Up | WheelDirection::Down => Axis::Vertical,
            WheelDirection::Left | WheelDirection::Right => Axis::Horizontal,
        };
        self.scroll_wheel(self.wheel_units_for_direction(direction), axis);
    }

    pub fn wheel_up(&mut self) {
        self.dispatch_wheel_direction(WheelDirection::Up);
    }

    pub fn wheel_down(&mut self) {
        self.dispatch_wheel_direction(WheelDirection::Down);
    }

    pub fn wheel_left(&mut self) {
        self.dispatch_wheel_direction(WheelDirection::Left);
    }

    pub fn wheel_right(&mut self) {
        self.dispatch_wheel_direction(WheelDirection::Right);
    }

    pub fn increase_wheel_speed(&mut self) {
        self.current_wheel_speed = (self.current_wheel_speed + self.effective_wheel.speed_step)
            .min(self.effective_wheel.max_speed);
        self.flash_wheel_speed_indicator();
        self.push_wheel_speed_notification();
    }

    pub fn decrease_wheel_speed(&mut self) {
        self.current_wheel_speed = (self.current_wheel_speed - self.effective_wheel.speed_step)
            .max(self.effective_wheel.min_speed);
        self.flash_wheel_speed_indicator();
        self.push_wheel_speed_notification();
    }

    pub fn reset_wheel_speed_tier(&mut self) {
        self.current_wheel_speed = self.effective_wheel.default_speed;
        self.flash_wheel_speed_indicator();
        self.push_wheel_speed_notification();
    }

    pub fn increase_mouse_speed(&mut self) {
        self.mouse_speed_baseline = (self.mouse_speed_baseline
            + self.effective_mouse_speed.speed_step)
            .min(self.effective_mouse_speed.max_speed);
        self.reset_acceleration_to_baseline();
        self.flash_mouse_speed_indicator();
        self.push_mouse_speed_notification();
    }

    pub fn decrease_mouse_speed(&mut self) {
        self.mouse_speed_baseline = (self.mouse_speed_baseline
            - self.effective_mouse_speed.speed_step)
            .max(self.effective_mouse_speed.min_speed);
        self.reset_acceleration_to_baseline();
        self.flash_mouse_speed_indicator();
        self.push_mouse_speed_notification();
    }

    pub fn reset_mouse_speed_tier(&mut self) {
        self.mouse_speed_baseline = self.effective_mouse_speed.default_speed;
        self.reset_acceleration_to_baseline();
        self.flash_mouse_speed_indicator();
        self.push_mouse_speed_notification();
    }

    pub fn flash_mouse_speed_indicator(&mut self) {
        self.flash_mouse_speed_indicator_at(Instant::now());
    }

    pub fn flash_mouse_speed_indicator_at(&mut self, now: Instant) {
        self.mouse_speed_flash_until =
            Some(now + Duration::from_millis(self.effective_mouse_speed.flash_indicator_ms));
    }

    pub fn mouse_speed_indicator_active(&self) -> bool {
        self.mouse_speed_indicator_active_at(Instant::now())
    }

    pub fn mouse_speed_indicator_active_at(&self, now: Instant) -> bool {
        self.mouse_speed_flash_until
            .is_some_and(|flash_until| now < flash_until)
    }

    pub fn flash_wheel_speed_indicator(&mut self) {
        self.flash_wheel_speed_indicator_at(Instant::now());
    }

    pub fn flash_wheel_speed_indicator_at(&mut self, now: Instant) {
        self.wheel_speed_flash_until =
            Some(now + Duration::from_millis(self.effective_wheel.speed_indicator_ms));
    }

    pub fn wheel_speed_indicator_active(&self) -> bool {
        self.wheel_speed_indicator_active_at(Instant::now())
    }

    pub fn wheel_speed_indicator_active_at(&self, now: Instant) -> bool {
        self.wheel_speed_flash_until
            .is_some_and(|flash_until| now < flash_until)
    }

    pub fn tick_movement(
        &mut self,
        active_actions: &HashSet<Action>,
        _dt: Duration,
    ) -> MovementTick {
        let effective_actions = self.effective_actions_for_tick(active_actions);
        let tick = calculate_movement(
            &effective_actions,
            &self.config,
            self.top_speed_behavior,
            self.mouse_speed_baseline,
            &mut self.current_speed,
            &mut self.acceleration_counter,
        );

        if tick.moving {
            self.move_mouse(tick.dx.round() as i32, tick.dy.round() as i32);
        }
        let surgical_active = active_actions.contains(&Action::SurgicalMode);
        if let Ok((x, y)) = self.backend.location() {
            update_zoom_state(
                &mut self.surgical_zoom_state,
                SurgicalZoomConfig {
                    enabled: self.config.surgical_mode.enabled,
                    zoom_enabled: self.config.surgical_mode.zoom_enabled,
                    zoom_scale: self.config.surgical_mode.zoom_scale,
                    zoom_size_px: self.config.surgical_mode.zoom_size_px,
                    overlay_offset_x: self.config.surgical_mode.overlay_offset_x,
                    overlay_offset_y: self.config.surgical_mode.overlay_offset_y,
                },
                surgical_active,
                x,
                y,
            );
        }

        self.tick_wheel(&effective_actions);

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

    fn effective_actions_for_tick(&self, active_actions: &HashSet<Action>) -> HashSet<Action> {
        let mut effective = active_actions.clone();
        if let Some(modifier) = Action::from_string(&self.config.scroll_mode.modifier_action) {
            if self.config.scroll_mode.enabled && active_actions.contains(&modifier) {
                for movement in active_actions.iter().filter(|a| a.is_movement()) {
                    match movement {
                        Action::MoveUp => {
                            effective.insert(Action::WheelUp);
                        }
                        Action::MoveDown => {
                            effective.insert(Action::WheelDown);
                        }
                        Action::MoveLeft => {
                            effective.insert(Action::WheelLeft);
                        }
                        Action::MoveRight => {
                            effective.insert(Action::WheelRight);
                        }
                        _ => {}
                    }
                    effective.remove(movement);
                }
            }
        }
        effective
    }
    fn tick_wheel(&mut self, active_actions: &HashSet<Action>) {
        self.tick_wheel_at(active_actions, Instant::now());
    }

    fn tick_wheel_at(&mut self, active_actions: &HashSet<Action>, now: Instant) {
        let Some(wheel_action) = active_actions
            .iter()
            .find(|action| action.is_wheel_direction())
            .cloned()
        else {
            self.last_wheel_tick = None;
            self.last_wheel_direction = None;
            return;
        };
        let direction = wheel_direction_for_action(&wheel_action);
        if self.last_wheel_direction != Some(direction) {
            self.last_wheel_direction = Some(direction);
            self.last_wheel_tick = Some(now);
            self.handle_action(wheel_action);
            return;
        }
        let interval = Duration::from_millis(self.effective_wheel.tick_interval);
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
        if let Some(rect) = current_monitor_rect_for_cursor(self.config.edge_jump.use_work_area) {
            let (x, y) = rect.center();
            self.move_mouse_to(x, y);
        }
    }

    pub fn move_to_monitor_edge(&mut self, edge: MonitorEdge) {
        if let Some(rect) = current_monitor_rect_for_cursor(self.config.edge_jump.use_work_area) {
            let (x, y) = edge_target(rect, edge, self.config.edge_jump.offset_px);
            self.move_mouse_to(x, y);
        }
    }

    #[allow(dead_code)]
    fn window_snap_for_action(action: &Action, points: WindowSnapPoints) -> Option<(i32, i32)> {
        match action {
            Action::MoveToWindowTopEdge => Some(points.top_edge),
            Action::MoveToWindowBottomEdge => Some(points.bottom_edge),
            Action::MoveToWindowLeftEdge => Some(points.left_edge),
            Action::MoveToWindowRightEdge => Some(points.right_edge),
            Action::MoveToWindowCenter => Some(points.center),
            Action::MoveToWindowTitlebar => Some(points.titlebar),
            _ => None,
        }
    }

    fn move_to_window_snap(&mut self, map: fn(WindowSnapPoints) -> (i32, i32)) {
        if !self.config.window_jump.enabled {
            return;
        }
        if let Some(points) = (self.window_snap_points_provider)(
            self.config.window_jump.use_extended_frame_bounds,
            self.config.window_jump.edge_offset_px,
            self.config.window_jump.titlebar_y_offset_px,
            self.config.window_jump.clamp_to_window,
        ) {
            let (x, y) = map(points);
            self.move_mouse_to(x, y);
        }
    }

    pub fn select_next_screen(&mut self) {
        let Ok(cursor) = self.backend.location() else {
            return;
        };

        let monitors = (self.monitor_rects_provider)(self.config.edge_jump.use_work_area);
        let Some(current_monitor) = monitor_containing_point(&monitors, cursor) else {
            return;
        };
        let Some(target_monitor) = next_monitor_with_wrap(&monitors, current_monitor) else {
            return;
        };

        let (x, y) = target_monitor.center();
        self.move_mouse_to(x, y);
    }

    pub fn step_move(&mut self, direction: Direction2D, tier: StepMoveTier) {
        if !self.config.step_move.enabled {
            return;
        }
        let step = match tier {
            StepMoveTier::Small => self.config.step_move.small_step_px,
            StepMoveTier::Normal => self.config.step_move.normal_step_px,
            StepMoveTier::Large => self.config.step_move.large_step_px,
        };
        let Ok((x, y)) = self.backend.location() else {
            return;
        };
        let (dx, dy) = match direction {
            Direction2D::Up => (0, -step),
            Direction2D::Down => (0, step),
            Direction2D::Left => (-step, 0),
            Direction2D::Right => (step, 0),
        };
        let mut target = (x + dx, y + dy);
        match self.config.step_move.clamp_mode {
            crate::StepMoveClampMode::None => {}
            crate::StepMoveClampMode::VirtualScreen => {
                if let Some(rect) = virtual_screen_rect((self.monitor_rects_provider)(false)) {
                    target.0 = target.0.clamp(rect.left, rect.right - 1);
                    target.1 = target.1.clamp(rect.top, rect.bottom - 1);
                }
            }
            crate::StepMoveClampMode::CurrentMonitor => {
                if let Some(rect) = current_monitor_rect_for_cursor(false) {
                    target.0 = target.0.clamp(rect.left, rect.right - 1);
                    target.1 = target.1.clamp(rect.top, rect.bottom - 1);
                }
            }
            crate::StepMoveClampMode::CurrentWorkArea => {
                if let Some(rect) = current_monitor_rect_for_cursor(true) {
                    target.0 = target.0.clamp(rect.left, rect.right - 1);
                    target.1 = target.1.clamp(rect.top, rect.bottom - 1);
                }
            }
        }
        self.move_mouse_to(target.0, target.1);
        self.reset_acceleration_to_baseline();
        if self.config.step_move.show_tooltip {
            self.push_notification(RuntimeNotification {
                kind: RuntimeNotificationKind::StepMove,
                title: "Step move".to_string(),
                body: format!("Step {:?} {}px", direction, step),
                duration_ms: self.config.tooltip_overlay.duration_ms,
            });
        }
    }

    /// Resets the speed and acceleration counter when motion stops
    pub fn reset_speed(&mut self) {
        self.reset_acceleration_to_baseline();
        self.last_wheel_tick = None;
        self.last_wheel_direction = None;
    }

    fn reset_acceleration_to_baseline(&mut self) {
        self.current_speed = self.mouse_speed_baseline;
        self.acceleration_counter = 0;
    }

    pub fn apply_config_preserving_mode(&mut self, config: Config) {
        let active = self.current_mode == ModeState::Active;
        self.config = config;
        self.top_speed_behavior = TopSpeedBehavior::from_config(&self.config);
        let movement_profile = self.active_movement_profile.clone();
        if !self.apply_movement_profile(movement_profile.as_deref()) {
            self.apply_movement_profile(None);
        }
        let wheel_profile = self.active_wheel_profile.clone();
        if !self.apply_wheel_profile(wheel_profile.as_deref()) {
            self.apply_wheel_profile(None);
        }
        self.set_active_mode(active);
        self.reset_speed();
    }

    pub fn hard_reset_runtime(&mut self) {
        self.release_left_button_if_held();
        self.apply_movement_profile(None);
        self.apply_wheel_profile(None);
        self.reset_speed();
        self.mouse_speed_flash_until = None;
        self.wheel_speed_flash_until = None;
    }

    pub fn select_movement_profile(&mut self, name: &str) -> bool {
        let selected = self.apply_movement_profile(Some(name));
        if selected {
            self.push_movement_profile_notification();
        }
        selected
    }

    pub fn select_next_movement_profile(&mut self) -> bool {
        let profiles = self.profile_names(self.config.movement_profiles.keys());
        let next = next_profile_name(&profiles, self.active_movement_profile.as_deref(), 1);
        let selected = self.apply_movement_profile(next.as_deref());
        if selected {
            self.push_movement_profile_notification();
        }
        selected
    }

    pub fn select_previous_movement_profile(&mut self) -> bool {
        let profiles = self.profile_names(self.config.movement_profiles.keys());
        let next = next_profile_name(&profiles, self.active_movement_profile.as_deref(), -1);
        let selected = self.apply_movement_profile(next.as_deref());
        if selected {
            self.push_movement_profile_notification();
        }
        selected
    }

    pub fn select_wheel_profile(&mut self, name: &str) -> bool {
        let selected = self.apply_wheel_profile(Some(name));
        if selected {
            self.push_wheel_profile_notification();
        }
        selected
    }

    pub fn select_next_wheel_profile(&mut self) -> bool {
        let profiles = self.profile_names(self.config.wheel_profiles.keys());
        let next = next_profile_name(&profiles, self.active_wheel_profile.as_deref(), 1);
        let selected = self.apply_wheel_profile(next.as_deref());
        if selected {
            self.push_wheel_profile_notification();
        }
        selected
    }

    pub fn select_previous_wheel_profile(&mut self) -> bool {
        let profiles = self.profile_names(self.config.wheel_profiles.keys());
        let next = next_profile_name(&profiles, self.active_wheel_profile.as_deref(), -1);
        let selected = self.apply_wheel_profile(next.as_deref());
        if selected {
            self.push_wheel_profile_notification();
        }
        selected
    }

    pub fn push_config_reload_notification(&mut self) {
        self.push_notification(RuntimeNotification {
            kind: RuntimeNotificationKind::ConfigReload,
            title: "Config reloaded".to_string(),
            body: "Runtime settings updated".to_string(),
            duration_ms: self.config.tooltip_overlay.duration_ms,
        });
    }

    pub fn push_panic_reset_notification(&mut self) {
        self.push_notification(RuntimeNotification {
            kind: RuntimeNotificationKind::PanicReset,
            title: "Panic reset".to_string(),
            body: "Runtime state restored".to_string(),
            duration_ms: self.config.tooltip_overlay.duration_ms,
        });
    }

    fn push_mouse_speed_notification(&mut self) {
        self.push_notification(RuntimeNotification {
            kind: RuntimeNotificationKind::MouseSpeed,
            title: "Mouse speed".to_string(),
            body: format!("Speed {}", self.mouse_speed_baseline),
            duration_ms: self.effective_mouse_speed.flash_indicator_ms,
        });
    }

    fn push_wheel_speed_notification(&mut self) {
        self.push_notification(RuntimeNotification {
            kind: RuntimeNotificationKind::WheelSpeed,
            title: "Wheel speed".to_string(),
            body: format!("Speed {}", self.current_wheel_speed),
            duration_ms: self.effective_wheel.speed_indicator_ms,
        });
    }

    fn push_movement_profile_notification(&mut self) {
        self.push_notification(RuntimeNotification {
            kind: RuntimeNotificationKind::MovementProfile,
            title: "Movement profile".to_string(),
            body: self
                .active_movement_profile
                .clone()
                .unwrap_or_else(|| "default".to_string()),
            duration_ms: self.effective_mouse_speed.flash_indicator_ms,
        });
    }

    fn push_wheel_profile_notification(&mut self) {
        self.push_notification(RuntimeNotification {
            kind: RuntimeNotificationKind::WheelProfile,
            title: "Wheel profile".to_string(),
            body: self
                .active_wheel_profile
                .clone()
                .unwrap_or_else(|| "default".to_string()),
            duration_ms: self.effective_wheel.speed_indicator_ms,
        });
    }

    fn push_drag_notification(&mut self) {
        self.push_notification(RuntimeNotification {
            kind: RuntimeNotificationKind::Drag,
            title: "Drag".to_string(),
            body: if self.left_button_held {
                "Enabled".to_string()
            } else {
                "Disabled".to_string()
            },
            duration_ms: self.config.tooltip_overlay.duration_ms,
        });
    }

    fn apply_movement_profile(&mut self, profile: Option<&str>) -> bool {
        let Ok(settings) = self.config.resolved_movement_profile(profile) else {
            eprintln!(
                "[movement] profile '{}' does not exist",
                profile.unwrap_or_default()
            );
            return false;
        };
        self.active_movement_profile = profile.map(str::to_string);
        self.effective_mouse_speed = settings;
        self.mouse_speed_baseline = settings.default_speed;
        self.reset_acceleration_to_baseline();
        self.flash_mouse_speed_indicator();
        true
    }

    fn apply_wheel_profile(&mut self, profile: Option<&str>) -> bool {
        let Ok(settings) = self.config.resolved_wheel_profile(profile) else {
            eprintln!(
                "[wheel] profile '{}' does not exist",
                profile.unwrap_or_default()
            );
            return false;
        };
        self.active_wheel_profile = profile.map(str::to_string);
        self.effective_wheel = settings;
        self.current_wheel_speed = settings.default_speed;
        self.last_wheel_tick = None;
        self.last_wheel_direction = None;
        self.flash_wheel_speed_indicator();
        true
    }

    fn profile_names<'a>(&self, names: impl Iterator<Item = &'a String>) -> Vec<String> {
        let mut profiles: Vec<String> = names.cloned().collect();
        profiles.sort();
        profiles
    }

    pub fn prepare_exit(&mut self) {
        self.release_left_button_if_held();
    }

    pub fn exit(&mut self) {
        println!("Exiting");
        self.prepare_exit();
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

fn virtual_screen_rect(monitors: Vec<MonitorRect>) -> Option<MonitorRect> {
    let mut iter = monitors.into_iter();
    let first = iter.next()?;
    Some(iter.fold(first, |acc, rect| MonitorRect {
        left: acc.left.min(rect.left),
        top: acc.top.min(rect.top),
        right: acc.right.max(rect.right),
        bottom: acc.bottom.max(rect.bottom),
    }))
}

pub fn edge_target(rect: MonitorRect, edge: MonitorEdge, offset_px: i32) -> (i32, i32) {
    rect.edge_midpoint(edge, offset_px)
}

pub fn final_adjust_control_for_event(
    event: &KeyEvent,
    action: Option<Action>,
    config: &FinalAdjustConfig,
) -> Option<FinalAdjustControl> {
    if !event.is_down {
        return None;
    }

    if matches_configured_key(event.key, &config.confirm_key) {
        return Some(FinalAdjustControl::Confirm);
    }
    if matches_configured_key(event.key, &config.cancel_key) {
        return Some(FinalAdjustControl::Cancel);
    }
    if matches_configured_key(event.key, &config.back_key) {
        return Some(FinalAdjustControl::Back);
    }

    let (dx, dy) = final_adjust_direction(event.key, action)?;
    let step = if final_adjust_modifier_down(event, &config.modifier_key) {
        config.large_step_px
    } else {
        config.small_step_px
    };
    Some(FinalAdjustControl::Nudge {
        dx: dx * step,
        dy: dy * step,
    })
}

fn matches_configured_key(key: VirtualKey, configured: &str) -> bool {
    VirtualKey::from_string(configured) == Some(key)
}

fn final_adjust_direction(key: VirtualKey, action: Option<Action>) -> Option<(i32, i32)> {
    match action {
        Some(Action::MoveUp) => Some((0, -1)),
        Some(Action::MoveDown) => Some((0, 1)),
        Some(Action::MoveLeft) => Some((-1, 0)),
        Some(Action::MoveRight) => Some((1, 0)),
        Some(Action::MoveUpRight) => Some((1, -1)),
        Some(Action::MoveUpLeft) => Some((-1, -1)),
        Some(Action::MoveDownRight) => Some((1, 1)),
        Some(Action::MoveDownLeft) => Some((-1, 1)),
        _ => match key {
            VirtualKey::Up => Some((0, -1)),
            VirtualKey::Down => Some((0, 1)),
            VirtualKey::Left => Some((-1, 0)),
            VirtualKey::Right => Some((1, 0)),
            _ => None,
        },
    }
}

fn final_adjust_modifier_down(event: &KeyEvent, configured: &str) -> bool {
    match VirtualKey::from_string(configured) {
        Some(VirtualKey::Shift | VirtualKey::LeftShift | VirtualKey::RightShift) => {
            event.shift_down
                || matches!(
                    event.key,
                    VirtualKey::Shift | VirtualKey::LeftShift | VirtualKey::RightShift
                )
        }
        Some(VirtualKey::Ctrl | VirtualKey::LeftCtrl | VirtualKey::RightCtrl) => {
            event.ctrl_down
                || matches!(
                    event.key,
                    VirtualKey::Ctrl | VirtualKey::LeftCtrl | VirtualKey::RightCtrl
                )
        }
        Some(VirtualKey::Alt | VirtualKey::LeftAlt) => {
            event.alt_down || matches!(event.key, VirtualKey::Alt | VirtualKey::LeftAlt)
        }
        Some(VirtualKey::RightAlt) => event.right_alt_down || event.key == VirtualKey::RightAlt,
        Some(key) => event.key == key,
        None => false,
    }
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

fn wheel_direction_for_action(action: &Action) -> WheelDirection {
    match action {
        Action::WheelUp => WheelDirection::Up,
        Action::WheelDown => WheelDirection::Down,
        Action::WheelLeft => WheelDirection::Left,
        Action::WheelRight => WheelDirection::Right,
        _ => unreachable!("non-wheel action passed to wheel_direction_for_action"),
    }
}

pub(crate) fn effective_slow_speed(config: &Config, baseline_speed: i32) -> i32 {
    let resolved = match config.slow_mouse.strategy {
        crate::SlowMouseStrategy::Fixed => config.slow_mouse.fixed_speed,
        crate::SlowMouseStrategy::Multiplier => {
            (f64::from(baseline_speed) * f64::from(config.slow_mouse.multiplier)).round() as i32
        }
        crate::SlowMouseStrategy::Subtract => baseline_speed - config.slow_mouse.subtract_speed,
    };

    resolved.clamp(config.slow_mouse.min_speed, config.slow_mouse.max_speed)
}

fn calculate_movement(
    active_actions: &HashSet<Action>,
    config: &Config,
    top_speed_behavior: TopSpeedBehavior,
    baseline_speed: i32,
    current_speed: &mut i32,
    acceleration_counter: &mut u32,
) -> MovementTick {
    let mut dx = 0;
    let mut dy = 0;

    for action in active_actions {
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
        *current_speed = baseline_speed;
        *acceleration_counter = 0;
        return MovementTick {
            dx: 0.0,
            dy: 0.0,
            speed: *current_speed,
            moving: false,
        };
    }

    let speed = if active_actions.contains(&Action::SlowMouse) {
        if active_actions.contains(&Action::SurgicalMode) && config.surgical_mode.enabled {
            *current_speed = baseline_speed;
            *acceleration_counter = 0;
            config.surgical_mode.speed_px
        } else {
            *current_speed = baseline_speed;
            *acceleration_counter = 0;
            effective_slow_speed(config, baseline_speed)
        }
    } else if active_actions.contains(&Action::SurgicalMode) && config.surgical_mode.enabled {
        *current_speed = baseline_speed;
        *acceleration_counter = 0;
        config.surgical_mode.speed_px
    } else {
        advance_speed(
            config,
            top_speed_behavior.top_speed_for_baseline(baseline_speed),
            current_speed,
            acceleration_counter,
        )
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

    // These legacy top-level fields are still active runtime ramp controls.
    if *acceleration_counter >= config.acceleration_rate {
        *current_speed += config.acceleration;
        *acceleration_counter = 0;
    }

    *current_speed = (*current_speed).min(top_speed);
    *current_speed
}

fn next_profile_name(profiles: &[String], current: Option<&str>, direction: i32) -> Option<String> {
    if profiles.is_empty() {
        return None;
    }

    let current_index = current
        .and_then(|name| profiles.iter().position(|profile| profile == name))
        .unwrap_or(if direction >= 0 {
            profiles.len() - 1
        } else {
            0
        });
    let len = profiles.len() as i32;
    let next_index = (current_index as i32 + direction).rem_euclid(len) as usize;
    profiles.get(next_index).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        Config {
            starting_speed: 2,
            mouse_speed: crate::MouseSpeedConfig {
                default_speed: 2,
                min_speed: 1,
                max_speed: 12,
                speed_step: 1,
                flash_indicator_ms: 700,
            },
            acceleration: 3,
            acceleration_rate: 2,
            top_speed: 10,
            ..Config::default()
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    enum MouseOperation {
        Click(Button),
        ButtonDown(Button),
        ButtonUp(Button),
    }

    #[derive(Default)]
    struct FakeBackend {
        location: (i32, i32),
        clicks: Vec<Button>,
        button_downs: Vec<Button>,
        button_ups: Vec<Button>,
        operations: Vec<MouseOperation>,
        moves: Vec<(i32, i32)>,
        scrolls: Vec<(i32, Axis)>,
    }

    impl MouseBackend for FakeBackend {
        fn click(&mut self, button: Button) -> Result<(), String> {
            self.clicks.push(button);
            self.operations.push(MouseOperation::Click(button));
            Ok(())
        }

        fn button_down(&mut self, button: Button) -> Result<(), String> {
            self.button_downs.push(button);
            self.operations.push(MouseOperation::ButtonDown(button));
            Ok(())
        }

        fn button_up(&mut self, button: Button) -> Result<(), String> {
            self.button_ups.push(button);
            self.operations.push(MouseOperation::ButtonUp(button));
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

    fn single_monitor_provider(_use_work_area: bool) -> Vec<MonitorRect> {
        vec![MonitorRect {
            left: -100,
            top: -50,
            right: 300,
            bottom: 350,
        }]
    }

    fn negative_layout_monitor_provider(_use_work_area: bool) -> Vec<MonitorRect> {
        vec![
            MonitorRect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            MonitorRect {
                left: -1600,
                top: -900,
                right: 0,
                bottom: 0,
            },
            MonitorRect {
                left: 1920,
                top: 0,
                right: 3520,
                bottom: 900,
            },
        ]
    }

    fn fake_window_snap_points_provider(
        _use_extended_frame_bounds: bool,
        _edge_offset_px: i32,
        _titlebar_y_offset_px: i32,
        _clamp_to_window: bool,
    ) -> Option<WindowSnapPoints> {
        Some(WindowSnapPoints {
            top_edge: (10, 11),
            bottom_edge: (20, 21),
            left_edge: (30, 31),
            right_edge: (40, 41),
            center: (50, 51),
            titlebar: (60, 61),
        })
    }

    fn actions(actions: &[Action]) -> HashSet<Action> {
        actions.iter().cloned().collect()
    }

    fn movement_profile(name: &str) -> (String, crate::MouseSpeedConfig) {
        (
            name.to_string(),
            crate::MouseSpeedConfig {
                default_speed: 5,
                min_speed: 1,
                max_speed: 12,
                speed_step: 1,
                flash_indicator_ms: 700,
            },
        )
    }

    fn wheel_profile(name: &str) -> (String, crate::WheelProfileConfig) {
        (
            name.to_string(),
            crate::WheelProfileConfig {
                default_speed: Some(4),
                min_speed: Some(1),
                max_speed: Some(12),
                speed_step: Some(1),
                tick_interval: Some(8),
                speed_indicator_ms: Some(700),
                vertical_multiplier: Some(1),
                horizontal_multiplier: Some(1),
            },
        )
    }

    fn notification_kinds(mouse: &mut MouseMaster<FakeBackend>) -> Vec<RuntimeNotificationKind> {
        mouse
            .take_notifications()
            .into_iter()
            .map(|notification| notification.kind)
            .collect()
    }

    #[test]
    fn speed_actions_enqueue_runtime_notifications() {
        for (action, expected_kind) in [
            (Action::MouseSpeedUp, RuntimeNotificationKind::MouseSpeed),
            (Action::MouseSpeedDown, RuntimeNotificationKind::MouseSpeed),
            (Action::MouseSpeedReset, RuntimeNotificationKind::MouseSpeed),
            (Action::WheelSpeedUp, RuntimeNotificationKind::WheelSpeed),
            (Action::WheelSpeedDown, RuntimeNotificationKind::WheelSpeed),
            (Action::WheelSpeedReset, RuntimeNotificationKind::WheelSpeed),
        ] {
            let mut mouse = MouseMaster::new_with_backend(test_config(), FakeBackend::default());

            mouse.handle_action(action);

            assert_eq!(notification_kinds(&mut mouse), vec![expected_kind]);
        }
    }

    #[test]
    fn profile_actions_enqueue_runtime_notifications() {
        let mut config = test_config();
        config
            .movement_profiles
            .extend([movement_profile("fast"), movement_profile("slow")]);
        config
            .wheel_profiles
            .extend([wheel_profile("coarse"), wheel_profile("fine")]);

        for (action, expected_kind) in [
            (
                Action::MovementProfileSelect("fast".to_string()),
                RuntimeNotificationKind::MovementProfile,
            ),
            (
                Action::MovementProfileNext,
                RuntimeNotificationKind::MovementProfile,
            ),
            (
                Action::MovementProfilePrevious,
                RuntimeNotificationKind::MovementProfile,
            ),
            (
                Action::WheelProfileSelect("coarse".to_string()),
                RuntimeNotificationKind::WheelProfile,
            ),
            (
                Action::WheelProfileNext,
                RuntimeNotificationKind::WheelProfile,
            ),
            (
                Action::WheelProfilePrevious,
                RuntimeNotificationKind::WheelProfile,
            ),
        ] {
            let mut mouse = MouseMaster::new_with_backend(config.clone(), FakeBackend::default());

            mouse.handle_action(action);

            assert_eq!(notification_kinds(&mut mouse), vec![expected_kind]);
        }
    }

    #[test]
    fn drag_reload_and_panic_actions_enqueue_runtime_notifications() {
        for (action, expected_kind) in [
            (Action::ToggleDragMode, RuntimeNotificationKind::Drag),
            (Action::ReloadConfig, RuntimeNotificationKind::ConfigReload),
            (Action::PanicReset, RuntimeNotificationKind::PanicReset),
        ] {
            let mut mouse = MouseMaster::new_with_backend(test_config(), FakeBackend::default());

            mouse.handle_action(action);

            assert_eq!(notification_kinds(&mut mouse), vec![expected_kind]);
        }
    }

    #[test]
    fn take_notifications_drains_queue_in_order() {
        let mut mouse = MouseMaster::new_with_backend(test_config(), FakeBackend::default());

        mouse.handle_action(Action::MouseSpeedUp);
        mouse.handle_action(Action::WheelSpeedUp);

        assert_eq!(
            notification_kinds(&mut mouse),
            vec![
                RuntimeNotificationKind::MouseSpeed,
                RuntimeNotificationKind::WheelSpeed,
            ]
        );
        assert!(mouse.take_notifications().is_empty());
    }

    fn final_adjust_config() -> FinalAdjustConfig {
        FinalAdjustConfig {
            enabled: true,
            small_step_px: 2,
            large_step_px: 9,
            modifier_key: "Shift".to_string(),
            confirm_key: "Enter".to_string(),
            cancel_key: "Escape".to_string(),
            back_key: "Backspace".to_string(),
            show_hint: true,
        }
    }

    fn tick(
        active_actions: &HashSet<Action>,
        config: &Config,
        baseline_speed: i32,
        current_speed: &mut i32,
        acceleration_counter: &mut u32,
    ) -> MovementTick {
        calculate_movement(
            active_actions,
            config,
            TopSpeedBehavior::from_config(config),
            baseline_speed,
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
            config.starting_speed,
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
            config.starting_speed,
            &mut current_speed,
            &mut acceleration_counter,
        );
        let second = tick(
            &active_actions,
            &config,
            config.starting_speed,
            &mut current_speed,
            &mut acceleration_counter,
        );

        assert_eq!(first.speed, config.starting_speed);
        assert_eq!(second.speed, config.starting_speed + config.acceleration);
    }

    #[test]
    fn legacy_acceleration_fields_still_affect_runtime_speed_ramp() {
        let config = Config {
            starting_speed: 3,
            mouse_speed: crate::MouseSpeedConfig {
                default_speed: 3,
                min_speed: 1,
                max_speed: 12,
                speed_step: 1,
                flash_indicator_ms: 700,
            },
            acceleration: 4,
            acceleration_rate: 3,
            top_speed: 9,
            ..Config::default()
        };
        let mut current_speed = config.mouse_speed.default_speed;
        let mut acceleration_counter = 0;
        let active_actions = actions(&[Action::MoveRight]);

        let first = tick(
            &active_actions,
            &config,
            config.mouse_speed.default_speed,
            &mut current_speed,
            &mut acceleration_counter,
        );
        let second = tick(
            &active_actions,
            &config,
            config.mouse_speed.default_speed,
            &mut current_speed,
            &mut acceleration_counter,
        );
        let third = tick(
            &active_actions,
            &config,
            config.mouse_speed.default_speed,
            &mut current_speed,
            &mut acceleration_counter,
        );
        let fourth = tick(
            &active_actions,
            &config,
            config.mouse_speed.default_speed,
            &mut current_speed,
            &mut acceleration_counter,
        );
        let fifth = tick(
            &active_actions,
            &config,
            config.mouse_speed.default_speed,
            &mut current_speed,
            &mut acceleration_counter,
        );
        let sixth = tick(
            &active_actions,
            &config,
            config.mouse_speed.default_speed,
            &mut current_speed,
            &mut acceleration_counter,
        );

        assert_eq!(first.speed, 3);
        assert_eq!(second.speed, 3);
        assert_eq!(third.speed, 7);
        assert_eq!(fourth.speed, 7);
        assert_eq!(fifth.speed, 7);
        assert_eq!(sixth.speed, 9);
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
            config.starting_speed,
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
            config.starting_speed,
            &mut current_speed,
            &mut acceleration_counter,
        );

        assert!(!movement.moving);
        assert_eq!(movement.speed, config.starting_speed);
        assert_eq!(current_speed, config.starting_speed);
        assert_eq!(acceleration_counter, 0);
    }

    #[test]
    fn slow_mouse_uses_starting_speed() {
        let config = test_config();
        let mut current_speed = 9;
        let mut acceleration_counter = 1;
        let active_actions = actions(&[Action::MoveRight, Action::SlowMouse]);

        let movement = tick(
            &active_actions,
            &config,
            config.starting_speed,
            &mut current_speed,
            &mut acceleration_counter,
        );

        assert_eq!(
            movement.speed,
            effective_slow_speed(&config, config.starting_speed)
        );
        assert_eq!(
            movement.dx,
            f64::from(effective_slow_speed(&config, config.starting_speed))
        );
        assert_eq!(movement.dy, 0.0);
    }

    #[test]
    fn slow_mouse_resets_speed_state() {
        let config = test_config();
        let mut current_speed = 9;
        let mut acceleration_counter = 1;
        let active_actions = actions(&[Action::MoveRight, Action::SlowMouse]);

        let movement = tick(
            &active_actions,
            &config,
            config.starting_speed,
            &mut current_speed,
            &mut acceleration_counter,
        );

        assert!(movement.moving);
        assert_eq!(current_speed, config.starting_speed);
        assert_eq!(acceleration_counter, 0);
    }

    #[test]
    fn releasing_slow_mouse_resumes_acceleration_from_baseline() {
        let config = test_config();
        let mut current_speed = 9;
        let mut acceleration_counter = 1;
        let slow_actions = actions(&[Action::MoveRight, Action::SlowMouse]);
        let movement_actions = actions(&[Action::MoveRight]);

        let slow_tick = tick(
            &slow_actions,
            &config,
            config.starting_speed,
            &mut current_speed,
            &mut acceleration_counter,
        );
        let first_after_release = tick(
            &movement_actions,
            &config,
            config.starting_speed,
            &mut current_speed,
            &mut acceleration_counter,
        );
        let second_after_release = tick(
            &movement_actions,
            &config,
            config.starting_speed,
            &mut current_speed,
            &mut acceleration_counter,
        );

        assert_eq!(
            slow_tick.speed,
            effective_slow_speed(&config, config.starting_speed)
        );
        assert_eq!(first_after_release.speed, config.starting_speed);
        assert_eq!(
            second_after_release.speed,
            config.starting_speed + config.acceleration
        );
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
                speed_indicator_ms: 700,
                vertical_multiplier: 1,
                horizontal_multiplier: 1,
            },
            ..Config::default()
        };
        let mut mouse = MouseMaster::new_with_backend(config, FakeBackend::default());

        mouse.increase_wheel_speed();

        assert_eq!(mouse.current_wheel_speed, 10);
    }

    #[test]
    fn wheel_speed_increase_triggers_flash_indicator() {
        let mut mouse = MouseMaster::new_with_backend(test_config(), FakeBackend::default());
        let now = Instant::now();

        mouse.increase_wheel_speed();

        assert!(mouse.wheel_speed_indicator_active_at(now));
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
                speed_indicator_ms: 700,
                vertical_multiplier: 1,
                horizontal_multiplier: 1,
            },
            ..Config::default()
        };
        let mut mouse = MouseMaster::new_with_backend(config, FakeBackend::default());

        mouse.decrease_wheel_speed();

        assert_eq!(mouse.current_wheel_speed, 1);
    }

    #[test]
    fn wheel_speed_decrease_triggers_flash_indicator() {
        let mut mouse = MouseMaster::new_with_backend(test_config(), FakeBackend::default());
        let now = Instant::now();

        mouse.decrease_wheel_speed();

        assert!(mouse.wheel_speed_indicator_active_at(now));
    }

    #[test]
    fn wheel_speed_flash_expires_at_configured_time() {
        let mut mouse = MouseMaster::new_with_backend(test_config(), FakeBackend::default());
        let now = Instant::now();
        let flash_duration = Duration::from_millis(mouse.config.wheel.speed_indicator_ms);

        mouse.flash_wheel_speed_indicator_at(now);

        assert!(
            mouse.wheel_speed_indicator_active_at(now + flash_duration - Duration::from_millis(1))
        );
        assert!(!mouse.wheel_speed_indicator_active_at(now + flash_duration));
    }

    #[test]
    fn movement_profile_selection_accepts_valid_names_and_rejects_invalid_names() {
        let mut config = test_config();
        config.movement_profiles.insert(
            "fast".to_string(),
            crate::MouseSpeedConfig {
                default_speed: 6,
                min_speed: 1,
                max_speed: 12,
                speed_step: 2,
                flash_indicator_ms: 700,
            },
        );
        let mut mouse = MouseMaster::new_with_backend(config, FakeBackend::default());

        assert!(mouse.select_movement_profile("fast"));
        assert_eq!(mouse.active_movement_profile.as_deref(), Some("fast"));
        assert_eq!(mouse.mouse_speed_baseline, 6);

        assert!(!mouse.select_movement_profile("missing"));
        assert_eq!(mouse.active_movement_profile.as_deref(), Some("fast"));
        assert_eq!(mouse.mouse_speed_baseline, 6);
    }

    #[test]
    fn wheel_profile_selection_applies_axis_multipliers_and_rejects_invalid_names() {
        let mut config = test_config();
        config.wheel_profiles.insert(
            "horizontal".to_string(),
            crate::WheelProfileConfig {
                default_speed: Some(4),
                min_speed: Some(1),
                max_speed: Some(9),
                speed_step: Some(1),
                tick_interval: Some(8),
                speed_indicator_ms: Some(700),
                vertical_multiplier: Some(1),
                horizontal_multiplier: Some(3),
            },
        );
        let mut mouse = MouseMaster::new_with_backend(config, FakeBackend::default());

        assert!(mouse.select_wheel_profile("horizontal"));
        mouse.wheel_right();
        assert_eq!(mouse.backend.scrolls, vec![(12, Axis::Horizontal)]);

        assert!(!mouse.select_wheel_profile("missing"));
        assert_eq!(mouse.active_wheel_profile.as_deref(), Some("horizontal"));
        assert_eq!(mouse.current_wheel_speed, 4);
    }

    #[test]
    fn wheel_delta_math_uses_speed_multiplier_and_direction_sign() {
        let mut up_config = test_config();
        up_config.wheel.default_speed = 1;
        up_config.wheel.vertical_multiplier = 1;
        let mut up_mouse = MouseMaster::new_with_backend(up_config, FakeBackend::default());
        up_mouse.wheel_up();
        assert_eq!(up_mouse.backend.scrolls, vec![(-1, Axis::Vertical)]);

        let mut down_config = test_config();
        down_config.wheel.default_speed = 10;
        down_config.wheel.vertical_multiplier = 1;
        let mut down_mouse = MouseMaster::new_with_backend(down_config, FakeBackend::default());
        down_mouse.wheel_down();
        assert_eq!(down_mouse.backend.scrolls, vec![(10, Axis::Vertical)]);

        let mut horizontal_config = test_config();
        horizontal_config.wheel.default_speed = 3;
        horizontal_config.wheel.horizontal_multiplier = 2;
        let mut horizontal_mouse =
            MouseMaster::new_with_backend(horizontal_config, FakeBackend::default());
        horizontal_mouse.wheel_right();
        horizontal_mouse.wheel_left();
        assert_eq!(
            horizontal_mouse.backend.scrolls,
            vec![(6, Axis::Horizontal), (-6, Axis::Horizontal)]
        );
    }

    #[test]
    fn wheel_repeat_timing_handles_initial_repeat_direction_change_and_release() {
        let mut config = test_config();
        config.wheel.tick_interval = 120;
        let mut mouse = MouseMaster::new_with_backend(config, FakeBackend::default());
        let base = Instant::now();

        mouse.tick_wheel_at(&actions(&[Action::WheelUp]), base);
        assert_eq!(mouse.backend.scrolls, vec![(-1, Axis::Vertical)]);

        mouse.tick_wheel_at(&actions(&[Action::WheelUp]), base + Duration::from_millis(50));
        assert_eq!(mouse.backend.scrolls.len(), 1);

        mouse.tick_wheel_at(&actions(&[Action::WheelUp]), base + Duration::from_millis(140));
        assert_eq!(mouse.backend.scrolls.len(), 2);

        mouse.tick_wheel_at(&actions(&[Action::WheelDown]), base + Duration::from_millis(150));
        assert_eq!(mouse.backend.scrolls.len(), 3);
        assert_eq!(mouse.backend.scrolls[2], (1, Axis::Vertical));

        mouse.tick_wheel_at(&HashSet::new(), base + Duration::from_millis(160));
        assert_eq!(mouse.last_wheel_tick, None);
        assert_eq!(mouse.last_wheel_direction, None);
    }

    #[test]
    fn panic_reset_releases_drag_and_resets_runtime_speeds() {
        let mut config = test_config();
        config.movement_profiles.insert(
            "fast".to_string(),
            crate::MouseSpeedConfig {
                default_speed: 7,
                min_speed: 1,
                max_speed: 12,
                speed_step: 1,
                flash_indicator_ms: 700,
            },
        );
        let mut mouse = MouseMaster::new_with_backend(config, FakeBackend::default());

        mouse.handle_action(Action::ToggleDragMode);
        mouse.select_movement_profile("fast");
        mouse.increase_wheel_speed();
        mouse.hard_reset_runtime();

        assert!(!mouse.left_button_held());
        assert_eq!(mouse.active_movement_profile, None);
        assert_eq!(mouse.active_wheel_profile, None);
        assert_eq!(
            mouse.mouse_speed_baseline,
            mouse.config.mouse_speed.default_speed
        );
        assert_eq!(mouse.current_wheel_speed, mouse.config.wheel.default_speed);
        assert_eq!(
            mouse.backend.operations,
            vec![
                MouseOperation::ButtonDown(Button::Left),
                MouseOperation::ButtonUp(Button::Left),
            ]
        );
    }

    #[test]
    fn mouse_speed_increase_and_decrease_clamp_to_configured_bounds() {
        let config = Config {
            mouse_speed: crate::MouseSpeedConfig {
                default_speed: 5,
                min_speed: 3,
                max_speed: 7,
                speed_step: 4,
                flash_indicator_ms: 700,
            },
            ..Config::default()
        };
        let mut mouse = MouseMaster::new_with_backend(config, FakeBackend::default());

        mouse.increase_mouse_speed();
        assert_eq!(mouse.mouse_speed_baseline, 7);
        assert_eq!(mouse.current_speed, 7);

        mouse.decrease_mouse_speed();
        assert_eq!(mouse.mouse_speed_baseline, 3);
        assert_eq!(mouse.current_speed, 3);
    }

    #[test]
    fn mouse_speed_reset_returns_to_default_tier() {
        let config = Config {
            mouse_speed: crate::MouseSpeedConfig {
                default_speed: 5,
                min_speed: 1,
                max_speed: 9,
                speed_step: 2,
                flash_indicator_ms: 700,
            },
            ..Config::default()
        };
        let mut mouse = MouseMaster::new_with_backend(config, FakeBackend::default());

        mouse.increase_mouse_speed();
        mouse.increase_mouse_speed();
        mouse.reset_mouse_speed_tier();

        assert_eq!(mouse.mouse_speed_baseline, 5);
        assert_eq!(mouse.current_speed, 5);
        assert_eq!(mouse.acceleration_counter, 0);
    }

    #[test]
    fn mouse_speed_change_triggers_flash_indicator() {
        let mut mouse = MouseMaster::new_with_backend(test_config(), FakeBackend::default());
        let now = Instant::now();

        mouse.increase_mouse_speed();

        assert!(mouse.mouse_speed_indicator_active_at(now));
    }

    #[test]
    fn slow_mouse_fixed_speed_ignores_high_mouse_speed_tier() {
        let mut config = test_config();
        config.mouse_speed.default_speed = 9;
        config.starting_speed = 9;
        config.slow_mouse.strategy = crate::SlowMouseStrategy::Fixed;
        config.slow_mouse.fixed_speed = 3;
        config.slow_mouse.min_speed = 1;
        config.slow_mouse.max_speed = 12;
        let mut mouse = MouseMaster::new_with_backend(config, FakeBackend::default());
        let slow_actions = actions(&[Action::MoveRight, Action::SlowMouse]);

        let slow_tick = mouse.tick_movement(&slow_actions, Duration::from_millis(8));

        assert_eq!(slow_tick.speed, 3);
        assert_eq!(slow_tick.dx, 3.0);
        assert_eq!(mouse.mouse_speed_baseline, 9);
        assert_eq!(mouse.current_speed, 9);
        assert_eq!(mouse.acceleration_counter, 0);
    }

    #[test]
    fn slow_mouse_multiplier_clamps_to_configured_max() {
        let mut config = test_config();
        config.slow_mouse.strategy = crate::SlowMouseStrategy::Multiplier;
        config.slow_mouse.multiplier = 3.0;
        config.slow_mouse.min_speed = 1;
        config.slow_mouse.max_speed = 6;
        let expected_baseline = config.mouse_speed.default_speed;

        let mut mouse = MouseMaster::new_with_backend(config, FakeBackend::default());
        let slow_actions = actions(&[Action::MoveRight, Action::SlowMouse]);

        let slow_tick = mouse.tick_movement(&slow_actions, Duration::from_millis(8));

        assert_eq!(slow_tick.speed, 6);
        assert_eq!(slow_tick.dx, 6.0);
        assert_eq!(mouse.mouse_speed_baseline, expected_baseline);
        assert_eq!(mouse.current_speed, expected_baseline);
        assert_eq!(mouse.acceleration_counter, 0);
    }

    #[test]
    fn slow_mouse_subtract_clamps_to_min() {
        let mut config = test_config();
        config.slow_mouse.strategy = crate::SlowMouseStrategy::Subtract;
        config.slow_mouse.subtract_speed = 50;
        config.slow_mouse.min_speed = 2;
        config.slow_mouse.max_speed = 12;
        let expected_baseline = config.mouse_speed.default_speed;

        let mut mouse = MouseMaster::new_with_backend(config, FakeBackend::default());
        let slow_actions = actions(&[Action::MoveRight, Action::SlowMouse]);

        let slow_tick = mouse.tick_movement(&slow_actions, Duration::from_millis(8));

        assert_eq!(slow_tick.speed, 2);
        assert_eq!(slow_tick.dx, 2.0);
        assert_eq!(mouse.mouse_speed_baseline, expected_baseline);
        assert_eq!(mouse.current_speed, expected_baseline);
        assert_eq!(mouse.acceleration_counter, 0);
    }

    #[test]
    fn slow_mouse_release_uses_slow_tick_then_returns_to_selected_tier_baseline() {
        let mut config = test_config();
        config.mouse_speed.default_speed = 4;
        config.starting_speed = 4;
        let mut mouse = MouseMaster::new_with_backend(config, FakeBackend::default());
        let slow_actions = actions(&[Action::MoveRight, Action::SlowMouse]);
        let movement_actions = actions(&[Action::MoveRight]);

        let slow_tick = mouse.tick_movement(&slow_actions, Duration::from_millis(8));
        let release_tick = mouse.tick_movement(&movement_actions, Duration::from_millis(8));

        assert_eq!(slow_tick.speed, effective_slow_speed(&mouse.config, 4));
        assert_eq!(release_tick.speed, 4);
        assert_eq!(mouse.mouse_speed_baseline, 4);
        assert_eq!(mouse.current_speed, 4);
        assert_eq!(mouse.acceleration_counter, 1);
    }

    #[test]
    fn slow_mouse_diagonal_movement_uses_slow_speed_magnitude() {
        let mut config = test_config();
        config.mouse_speed.default_speed = 7;
        config.starting_speed = 7;
        config.slow_mouse.strategy = crate::SlowMouseStrategy::Fixed;
        config.slow_mouse.fixed_speed = 5;
        config.slow_mouse.min_speed = 1;
        config.slow_mouse.max_speed = 12;
        let mut mouse = MouseMaster::new_with_backend(config, FakeBackend::default());
        let slow_diagonal_actions =
            actions(&[Action::MoveUp, Action::MoveRight, Action::SlowMouse]);

        let tick = mouse.tick_movement(&slow_diagonal_actions, Duration::from_millis(8));
        let magnitude = (tick.dx.powi(2) + tick.dy.powi(2)).sqrt();

        assert_eq!(tick.speed, 5);
        assert!((magnitude - 5.0).abs() < f64::EPSILON);
        assert!((tick.dx - (5.0 * DIAGONAL_NORMALIZATION)).abs() < f64::EPSILON);
        assert!((tick.dy + (5.0 * DIAGONAL_NORMALIZATION)).abs() < f64::EPSILON);
        assert_eq!(mouse.mouse_speed_baseline, 7);
        assert_eq!(mouse.current_speed, 7);
        assert_eq!(mouse.acceleration_counter, 0);
    }

    #[test]
    fn surgical_mode_precedence_over_slow_and_normal() {
        let mut config = test_config();
        config.surgical_mode.enabled = true;
        config.surgical_mode.speed_px = 2;
        config.slow_mouse.strategy = crate::SlowMouseStrategy::Fixed;
        config.slow_mouse.fixed_speed = 5;
        let mut mouse = MouseMaster::new_with_backend(config, FakeBackend::default());
        let actions = actions(&[Action::MoveRight, Action::SlowMouse, Action::SurgicalMode]);
        let tick = mouse.tick_movement(&actions, Duration::from_millis(8));
        assert_eq!(tick.speed, 2);
    }

    #[test]
    fn surgical_mode_fixed_speed_invariant() {
        let mut config = test_config();
        config.surgical_mode.enabled = true;
        config.surgical_mode.speed_px = 3;
        let mut mouse = MouseMaster::new_with_backend(config, FakeBackend::default());
        let actions = actions(&[Action::MoveRight, Action::SurgicalMode]);
        let a = mouse.tick_movement(&actions, Duration::from_millis(8));
        let b = mouse.tick_movement(&actions, Duration::from_millis(8));
        assert_eq!(a.speed, 3);
        assert_eq!(b.speed, 3);
        assert_eq!(mouse.acceleration_counter, 0);
    }

    #[test]
    fn surgical_mode_resets_acceleration_state() {
        let mut config = test_config();
        config.surgical_mode.enabled = true;
        config.surgical_mode.speed_px = 2;
        let mut mouse = MouseMaster::new_with_backend(config, FakeBackend::default());
        let normal = actions(&[Action::MoveRight]);
        mouse.tick_movement(&normal, Duration::from_millis(8));
        assert!(mouse.acceleration_counter > 0);
        let surgical = actions(&[Action::MoveRight, Action::SurgicalMode]);
        mouse.tick_movement(&surgical, Duration::from_millis(8));
        assert_eq!(mouse.acceleration_counter, 0);
    }

    #[test]
    fn effective_slow_speed_applies_strategy_and_clamps() {
        let mut config = test_config();
        config.slow_mouse.min_speed = 2;
        config.slow_mouse.max_speed = 6;

        config.slow_mouse.strategy = crate::SlowMouseStrategy::Fixed;
        config.slow_mouse.fixed_speed = 9;
        assert_eq!(effective_slow_speed(&config, 5), 6);

        config.slow_mouse.strategy = crate::SlowMouseStrategy::Multiplier;
        config.slow_mouse.multiplier = 0.49;
        assert_eq!(effective_slow_speed(&config, 9), 4);

        config.slow_mouse.strategy = crate::SlowMouseStrategy::Subtract;
        config.slow_mouse.subtract_speed = 10;
        assert_eq!(effective_slow_speed(&config, 9), 2);
    }

    #[test]
    fn movement_key_release_resets_acceleration_to_selected_tier() {
        let mut config = test_config();
        config.mouse_speed.default_speed = 4;
        config.starting_speed = 4;
        let mut mouse = MouseMaster::new_with_backend(config, FakeBackend::default());
        let movement_actions = actions(&[Action::MoveRight]);

        mouse.tick_movement(&movement_actions, Duration::from_millis(8));
        mouse.tick_movement(&movement_actions, Duration::from_millis(8));
        assert!(mouse.current_speed > mouse.mouse_speed_baseline);

        let release_tick = mouse.tick_movement(&HashSet::new(), Duration::from_millis(8));

        assert!(!release_tick.moving);
        assert_eq!(release_tick.speed, 4);
        assert_eq!(mouse.current_speed, 4);
        assert_eq!(mouse.acceleration_counter, 0);
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
    fn toggle_drag_mode_on_records_left_button_down_and_marks_held() {
        let mut mouse = MouseMaster::new_with_backend(test_config(), FakeBackend::default());

        mouse.handle_action(Action::ToggleDragMode);
        assert!(mouse.left_button_held());
        assert_eq!(mouse.backend.button_downs, vec![Button::Left]);
        assert!(mouse.backend.button_ups.is_empty());
    }

    #[test]
    fn toggle_drag_mode_off_records_left_button_up_and_marks_released() {
        let mut mouse = MouseMaster::new_with_backend(test_config(), FakeBackend::default());

        mouse.handle_action(Action::ToggleDragMode);
        mouse.handle_action(Action::ToggleDragMode);

        assert!(!mouse.left_button_held());
        assert_eq!(mouse.backend.button_ups, vec![Button::Left]);
    }

    #[test]
    fn toggle_drag_mode_records_button_down_before_button_up() {
        let mut mouse = MouseMaster::new_with_backend(test_config(), FakeBackend::default());

        mouse.handle_action(Action::ToggleDragMode);
        mouse.handle_action(Action::ToggleDragMode);

        assert_eq!(
            mouse.backend.operations,
            vec![
                MouseOperation::ButtonDown(Button::Left),
                MouseOperation::ButtonUp(Button::Left),
            ]
        );
    }

    #[test]
    fn toggle_drag_mode_does_not_emit_implicit_click() {
        let mut mouse = MouseMaster::new_with_backend(test_config(), FakeBackend::default());

        mouse.handle_action(Action::ToggleDragMode);
        mouse.handle_action(Action::ToggleDragMode);

        assert!(mouse.backend.clicks.is_empty());
        assert_eq!(
            mouse.backend.operations,
            vec![
                MouseOperation::ButtonDown(Button::Left),
                MouseOperation::ButtonUp(Button::Left),
            ]
        );
    }

    #[test]
    fn left_click_during_drag_releases_only_without_clicking() {
        let mut mouse = MouseMaster::new_with_backend(test_config(), FakeBackend::default());

        mouse.handle_action(Action::ToggleDragMode);
        mouse.handle_action(Action::LeftClick);

        assert!(!mouse.left_button_held());
        assert!(mouse.backend.clicks.is_empty());
        assert_eq!(
            mouse.backend.operations,
            vec![
                MouseOperation::ButtonDown(Button::Left),
                MouseOperation::ButtonUp(Button::Left),
            ]
        );
    }

    #[test]
    fn left_click_outside_drag_performs_normal_click_only() {
        let mut mouse = MouseMaster::new_with_backend(test_config(), FakeBackend::default());

        mouse.handle_action(Action::LeftClick);

        assert!(!mouse.left_button_held());
        assert_eq!(mouse.backend.clicks, vec![Button::Left]);
        assert_eq!(
            mouse.backend.operations,
            vec![MouseOperation::Click(Button::Left)]
        );
    }

    #[test]
    fn release_left_button_if_held_noops_when_already_released() {
        let mut mouse = MouseMaster::new_with_backend(test_config(), FakeBackend::default());

        mouse.release_left_button_if_held();

        assert!(!mouse.left_button_held());
        assert!(mouse.backend.operations.is_empty());
    }

    #[test]
    fn clear_runtime_input_state_clears_active_continuous_actions() {
        let mouse = MouseMaster::new_with_backend(test_config(), FakeBackend::default());
        let mut handler = crate::action::ActionHandler::new(mouse);
        handler.process_active_keys(Action::MoveUp, true);
        handler.process_active_keys(Action::SurgicalMode, true);

        handler.clear_runtime_input_state();

        assert!(handler.active_keys.is_empty());
    }

    #[test]
    fn clear_runtime_input_state_releases_held_left_button() {
        let mouse = MouseMaster::new_with_backend(test_config(), FakeBackend::default());
        let mut handler = crate::action::ActionHandler::new(mouse);
        handler.mouse_master.handle_action(Action::ToggleDragMode);
        assert!(handler.mouse_master.left_button_held());

        handler.clear_runtime_input_state();

        assert!(!handler.mouse_master.left_button_held());
        assert!(handler
            .mouse_master
            .backend
            .button_ups
            .contains(&Button::Left));
    }

    #[test]
    fn click_then_disable_releases_drag_then_clicks_normally() {
        let mut mouse = MouseMaster::new_with_backend(test_config(), FakeBackend::default());

        mouse.handle_action(Action::ToggleDragMode);
        mouse.handle_action(Action::ClickThenDisable);

        assert!(!mouse.left_button_held());
        assert_eq!(mouse.current_mode, ModeState::Idle);
        assert_eq!(
            mouse.backend.operations,
            vec![
                MouseOperation::ButtonDown(Button::Left),
                MouseOperation::ButtonUp(Button::Left),
                MouseOperation::Click(Button::Left),
            ]
        );
    }

    #[test]
    fn prepare_exit_releases_held_left_button_without_exiting() {
        let mut mouse = MouseMaster::new_with_backend(test_config(), FakeBackend::default());

        mouse.handle_action(Action::ToggleDragMode);
        mouse.prepare_exit();

        assert!(!mouse.left_button_held());
        assert_eq!(
            mouse.backend.operations,
            vec![
                MouseOperation::ButtonDown(Button::Left),
                MouseOperation::ButtonUp(Button::Left),
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
    fn screen_select_single_monitor_centers_current_monitor() {
        let mut backend = FakeBackend::default();
        backend.location = (0, 0);
        let mut mouse = MouseMaster::new_with_backend(test_config(), backend);
        mouse.set_monitor_rects_provider(single_monitor_provider);

        mouse.handle_action(Action::ScreenSelect);

        assert_eq!(mouse.backend.moves, vec![(100, 150)]);
    }

    #[test]
    fn screen_select_moves_to_next_monitor_center_with_wrap() {
        let mut backend = FakeBackend::default();
        backend.location = (2000, 100);
        let mut mouse = MouseMaster::new_with_backend(test_config(), backend);
        mouse.set_monitor_rects_provider(negative_layout_monitor_provider);

        mouse.handle_action(Action::ScreenSelect);

        assert_eq!(mouse.backend.moves, vec![(-800, -450)]);
    }

    #[test]
    fn enabled_mode_movement_key_moves_backend_on_tick() {
        let mut backend = FakeBackend::default();
        backend.location = (50, 60);
        let mouse_master = MouseMaster::new_with_backend(test_config(), backend);
        let mut handler = crate::action::ActionHandler::new(mouse_master);

        handler.process_active_keys(Action::MoveRight, true);
        let tick = handler.tick_movement();

        assert!(tick.moving);
        assert_eq!(handler.mouse_master.backend.moves, vec![(52, 60)]);
    }

    #[test]
    fn edge_target_coordinates_use_one_pixel_monitor_inset() {
        let rect = MonitorRect {
            left: 10,
            top: 20,
            right: 210,
            bottom: 120,
        };

        assert_eq!(edge_target(rect, MonitorEdge::Top, 1), (110, 21));
        assert_eq!(edge_target(rect, MonitorEdge::Bottom, 1), (110, 118));
        assert_eq!(edge_target(rect, MonitorEdge::Left, 1), (11, 70));
        assert_eq!(edge_target(rect, MonitorEdge::Right, 1), (208, 70));
        assert_eq!(edge_target(rect, MonitorEdge::Top, 7), (110, 27));
    }

    #[test]
    fn step_move_emits_one_shot_absolute_move() {
        let mut backend = FakeBackend::default();
        backend.location = (100, 100);
        let mut mouse = MouseMaster::new_with_backend(test_config(), backend);
        mouse.handle_action(Action::StepMove {
            direction: Direction2D::Right,
            tier: StepMoveTier::Normal,
        });
        assert_eq!(mouse.backend.moves, vec![(180, 100)]);
    }

    #[test]
    fn step_move_virtual_screen_clamps_bounds() {
        let mut backend = FakeBackend::default();
        backend.location = (95, 95);
        let mut cfg = test_config();
        cfg.step_move.large_step_px = 500;
        cfg.step_move.clamp_mode = crate::StepMoveClampMode::VirtualScreen;
        let mut mouse = MouseMaster::new_with_backend(cfg, backend);
        mouse.set_monitor_rects_provider(single_monitor_provider);
        mouse.handle_action(Action::StepMove {
            direction: Direction2D::Right,
            tier: StepMoveTier::Large,
        });
        assert_eq!(mouse.backend.moves, vec![(299, 95)]);
    }

    #[test]
    fn window_snap_actions_map_to_expected_points() {
        let p = WindowSnapPoints {
            top_edge: (1, 2),
            bottom_edge: (3, 4),
            left_edge: (5, 6),
            right_edge: (7, 8),
            center: (9, 10),
            titlebar: (11, 12),
        };
        assert_eq!(
            MouseMaster::<FakeBackend>::window_snap_for_action(&Action::MoveToWindowTopEdge, p),
            Some((1, 2))
        );
        assert_eq!(
            MouseMaster::<FakeBackend>::window_snap_for_action(&Action::MoveToWindowBottomEdge, p),
            Some((3, 4))
        );
        assert_eq!(
            MouseMaster::<FakeBackend>::window_snap_for_action(&Action::MoveToWindowLeftEdge, p),
            Some((5, 6))
        );
        assert_eq!(
            MouseMaster::<FakeBackend>::window_snap_for_action(&Action::MoveToWindowRightEdge, p),
            Some((7, 8))
        );
        assert_eq!(
            MouseMaster::<FakeBackend>::window_snap_for_action(&Action::MoveToWindowCenter, p),
            Some((9, 10))
        );
        assert_eq!(
            MouseMaster::<FakeBackend>::window_snap_for_action(&Action::MoveToWindowTitlebar, p),
            Some((11, 12))
        );
        assert_eq!(
            MouseMaster::<FakeBackend>::window_snap_for_action(&Action::MoveUp, p),
            None
        );
    }

    #[test]
    fn window_snap_actions_dispatch_to_expected_absolute_points() {
        let actions = [
            (Action::MoveToWindowTopEdge, (10, 11)),
            (Action::MoveToWindowBottomEdge, (20, 21)),
            (Action::MoveToWindowLeftEdge, (30, 31)),
            (Action::MoveToWindowRightEdge, (40, 41)),
            (Action::MoveToWindowCenter, (50, 51)),
            (Action::MoveToWindowTitlebar, (60, 61)),
        ];

        for (action, expected) in actions {
            let mut mouse = MouseMaster::new_with_backend(test_config(), FakeBackend::default());
            mouse.set_window_snap_points_provider(fake_window_snap_points_provider);
            mouse.handle_action(action);
            assert_eq!(mouse.backend.moves, vec![expected]);
        }
    }

    #[test]
    fn window_snap_actions_do_not_move_when_feature_disabled() {
        let mut config = test_config();
        config.window_jump.enabled = false;
        let mut mouse = MouseMaster::new_with_backend(config, FakeBackend::default());
        mouse.set_window_snap_points_provider(fake_window_snap_points_provider);

        mouse.handle_action(Action::MoveToWindowCenter);

        assert!(mouse.backend.moves.is_empty());
    }

    #[test]
    fn final_adjust_nudge_uses_small_step_without_modifier() {
        let event = KeyEvent::new(VirtualKey::Right, true);

        assert_eq!(
            final_adjust_control_for_event(&event, Some(Action::MoveRight), &final_adjust_config()),
            Some(FinalAdjustControl::Nudge { dx: 2, dy: 0 })
        );
    }

    #[test]
    fn final_adjust_nudge_uses_large_step_with_modifier() {
        let mut event = KeyEvent::new(VirtualKey::Down, true);
        event.shift_down = true;

        assert_eq!(
            final_adjust_control_for_event(&event, Some(Action::MoveDown), &final_adjust_config()),
            Some(FinalAdjustControl::Nudge { dx: 0, dy: 9 })
        );
    }

    #[test]
    fn final_adjust_control_keys_are_matched_from_config() {
        let config = final_adjust_config();

        assert_eq!(
            final_adjust_control_for_event(&KeyEvent::new(VirtualKey::Enter, true), None, &config),
            Some(FinalAdjustControl::Confirm)
        );
        assert_eq!(
            final_adjust_control_for_event(&KeyEvent::new(VirtualKey::Escape, true), None, &config),
            Some(FinalAdjustControl::Cancel)
        );
        assert_eq!(
            final_adjust_control_for_event(
                &KeyEvent::new(VirtualKey::Backspace, true),
                None,
                &config
            ),
            Some(FinalAdjustControl::Back)
        );
    }

    #[test]
    fn effective_actions_remap_all_movement_directions_when_scroll_modifier_active() {
        let mut cfg = test_config();
        cfg.scroll_mode.enabled = true;
        cfg.scroll_mode.modifier_action = "scroll_modifier".to_string();
        let mouse = MouseMaster::new_with_backend(cfg, FakeBackend::default());

        let actions = HashSet::from([
            Action::MoveUp,
            Action::MoveDown,
            Action::MoveLeft,
            Action::MoveRight,
            Action::ScrollModifier,
        ]);
        let effective = mouse.effective_actions_for_tick(&actions);

        assert!(!effective.contains(&Action::MoveUp));
        assert!(!effective.contains(&Action::MoveDown));
        assert!(!effective.contains(&Action::MoveLeft));
        assert!(!effective.contains(&Action::MoveRight));
        assert!(effective.contains(&Action::WheelUp));
        assert!(effective.contains(&Action::WheelDown));
        assert!(effective.contains(&Action::WheelLeft));
        assert!(effective.contains(&Action::WheelRight));
    }

    #[test]
    fn movement_with_modifier_yields_wheel_and_no_move_event() {
        let mut backend = FakeBackend::default();
        backend.location = (10, 10);
        let mut cfg = test_config();
        cfg.scroll_mode.enabled = true;
        cfg.scroll_mode.modifier_action = "scroll_modifier".to_string();
        let mut mouse = MouseMaster::new_with_backend(cfg, backend);
        let actions = HashSet::from([Action::MoveUp, Action::ScrollModifier]);
        let tick = mouse.tick_movement(&actions, Duration::from_millis(8));
        assert!(!tick.moving);
        assert!(mouse.backend.moves.is_empty());
        assert_eq!(mouse.backend.scrolls.len(), 1);
    }

    #[test]
    fn movement_without_modifier_preserves_current_movement_behavior() {
        let mut backend = FakeBackend::default();
        backend.location = (10, 10);
        let mut cfg = test_config();
        cfg.scroll_mode.enabled = true;
        cfg.scroll_mode.modifier_action = "scroll_modifier".to_string();
        let mut mouse = MouseMaster::new_with_backend(cfg, backend);
        let actions = HashSet::from([Action::MoveUp]);
        let tick = mouse.tick_movement(&actions, Duration::from_millis(8));
        assert!(tick.moving);
        assert_eq!(mouse.backend.scrolls.len(), 0);
        assert_eq!(mouse.backend.moves.len(), 1);
    }
}
