mod action;
mod action_handler;
mod app_state;
mod config_audit;
mod grid_session;
mod help_overlay;
mod indicator;
mod jump_grid;
mod jump_overlay;
mod jump_session;
mod jump_view;
mod key_chord;
mod keyboard;
mod monitor;
mod overlay;
mod screen_capture;

use action::*;
use action_handler::*;
use app_state::{AppCommand, AppState, GridInputUpdate, JumpOverlayResolution, KeyEvent};
use config_audit::{audit_config_toml, ConfigAuditSeverity, ConfigAuditWarning};
#[cfg(test)]
use indicator::IndicatorState;
use indicator::{
    resolve_indicator_snapshot, IndicatorInput, MouseIndicatorInput, WheelIndicatorInput,
};
use jump_overlay::{
    hide_jump_overlay, show_jump_overlay, update_jump_overlay, virtual_screen_region,
};
use jump_session::JumpSessionUpdate;
use jump_view::{JumpLabelMetadata, JumpVisuals};
use key_chord::{KeyChord, RuntimeSystemBindings};
use keyboard::*;
use lazy_static::lazy_static;
use overlay::{StatusOverlayConfig, OVERLAY};
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};
use std::thread::sleep;
use std::time::{Duration, Instant};
use std::{env, error::Error, fs, io};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::*;

const DEFAULT_POLLING_RATE_MS: u64 = 8;
const DEFAULT_WHEEL_SPEED_INDICATOR_MS: u64 = 700;
const DEFAULT_MOUSE_SPEED_FLASH_MS: u64 = 700;
const MAX_MESSAGES_PER_TICK: usize = 64;
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
const DEBUG_HEARTBEAT_ENV: &str = "MULTI_MOUSEMOVER_DEBUG";
const MIN_JUMP_STAGE_SIZE: u32 = 1;
const MAX_JUMP_STAGE_SIZE: u32 = 26;
const MAX_JUMP_REGION_MARGIN_PERCENT: u8 = 50;
const MIN_JUMP_ZOOM_SCALE: f32 = 1.0;
const MAX_JUMP_ZOOM_SCALE: f32 = 10.0;
const MIN_GRID_MODE_REGION_PERCENT: f32 = 0.05;
const MAX_GRID_MODE_REGION_PERCENT: f32 = 1.0;
const MIN_GRID_MODE_SIZE_PX: i32 = 1;
const MAX_GRID_MODE_SIZE_PX: i32 = 500;
const MAX_EDGE_JUMP_OFFSET_PX: i32 = 10_000;
const MAX_JUMP_AIM_OFFSET_PX: i32 = 10_000;
const MAX_FINAL_ADJUST_STEP_PX: i32 = 10_000;
const DEFAULT_TOOLTIP_DURATION_MS: u64 = 900;
const MIN_TOOLTIP_HELP_WIDTH: i32 = 240;
const MAX_TOOLTIP_HELP_WIDTH: i32 = 800;
const MIN_TOOLTIP_HELP_BINDINGS: i32 = 0;
const MAX_TOOLTIP_HELP_BINDINGS: i32 = 200;
const MIN_TOOLTIP_OFFSET: i32 = -200;
const MAX_TOOLTIP_OFFSET: i32 = 200;
const APP_CRATE_ID: &str = env!("CARGO_PKG_NAME");
const APP_DISPLAY_NAME: &str = "Multi MouseMover";

static HOOK_EVENTS_SEEN: AtomicU64 = AtomicU64::new(0);
static HOOK_EVENTS_DECODED: AtomicU64 = AtomicU64::new(0);
static HOOK_EVENTS_SWALLOWED: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct LoopDiagnostics {
    loop_iterations: u64,
    messages_processed: u64,
    queued_key_events_processed: u64,
    commands_executed: u64,
    movement_ticks: u64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct HookDiagnostics {
    events_seen: u64,
    events_decoded: u64,
    events_swallowed: u64,
}

fn hook_diagnostics_snapshot() -> HookDiagnostics {
    HookDiagnostics {
        events_seen: HOOK_EVENTS_SEEN.load(Ordering::Relaxed),
        events_decoded: HOOK_EVENTS_DECODED.load(Ordering::Relaxed),
        events_swallowed: HOOK_EVENTS_SWALLOWED.load(Ordering::Relaxed),
    }
}

impl LoopDiagnostics {
    fn add(&mut self, other: Self) {
        self.loop_iterations += other.loop_iterations;
        self.messages_processed += other.messages_processed;
        self.queued_key_events_processed += other.queued_key_events_processed;
        self.commands_executed += other.commands_executed;
        self.movement_ticks += other.movement_ticks;
    }
}

#[derive(Debug, Default)]
struct HeartbeatDiagnostics {
    pending: LoopDiagnostics,
    elapsed: Duration,
}

impl HeartbeatDiagnostics {
    fn record(
        &mut self,
        diagnostics: LoopDiagnostics,
        elapsed: Duration,
    ) -> Option<LoopDiagnostics> {
        self.pending.add(diagnostics);
        self.elapsed += elapsed;

        if self.elapsed < HEARTBEAT_INTERVAL {
            return None;
        }

        let snapshot = self.pending;
        self.pending = LoopDiagnostics::default();
        while self.elapsed >= HEARTBEAT_INTERVAL {
            self.elapsed -= HEARTBEAT_INTERVAL;
        }
        Some(snapshot)
    }
}

/// RAII guard for the installed keyboard hook.
struct KeyboardHook(HHOOK);

// SAFETY: `KeyboardHook` is only accessed through `Mutex<Option<KeyboardHook>>` and
// represents an opaque Win32 hook handle. Transferring ownership of the wrapper
// between threads does not permit concurrent use of the raw handle.
unsafe impl Send for KeyboardHook {}

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        unsafe {
            if let Err(err) = UnhookWindowsHookEx(self.0) {
                eprintln!("[Win32] UnhookWindowsHookEx failed: {:?}", err);
            }
        }
    }
}

lazy_static! {
    static ref ACTION_HANDLER: RwLock<ActionHandler> = {
        let config = match Config::load_from_file("config.toml") {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("Failed to load configuration: {}", e);
                std::process::exit(1);
            }
        };
        let mouse_master = MouseMaster::new(config.clone());
        let handler = ActionHandler::new(mouse_master);

        RwLock::new(handler)
    };
    static ref KEY_ACTIONS: RwLock<KeyBindings> = RwLock::new(KeyBindings::new());
    static ref APP_STATE: RwLock<AppState> = RwLock::new(AppState::default());
    static ref CONFIG_WARNINGS: Mutex<Vec<StoredConfigWarning>> = Mutex::new(Vec::new());
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredConfigWarning {
    path: String,
    severity: ConfigAuditSeverity,
    message: String,
    suggestion: String,
}

impl StoredConfigWarning {
    fn runtime_warning(message: &str) -> Self {
        let path = message
            .split_once(' ')
            .map(|(path, _)| path)
            .unwrap_or("<config>");

        Self {
            path: path.to_string(),
            severity: ConfigAuditSeverity::Warning,
            message: message.to_string(),
            suggestion: "Review this config value and update config.toml.".to_string(),
        }
    }

    fn runtime_info(message: &str) -> Self {
        Self {
            path: "<config>".to_string(),
            severity: ConfigAuditSeverity::Info,
            message: message.to_string(),
            suggestion: "Review the preferred config path in config.toml.".to_string(),
        }
    }

    fn from_audit(warning: &ConfigAuditWarning) -> Self {
        Self {
            path: warning.path.clone(),
            severity: warning.severity,
            message: warning.message.clone(),
            suggestion: warning.suggestion.clone(),
        }
    }

    fn summary_text(&self) -> String {
        if self.suggestion.is_empty()
            || self.path == "<config>"
            || self.message.starts_with(&self.path)
        {
            self.message.clone()
        } else {
            format!(
                "{}: {} Suggestion: {}",
                self.path, self.message, self.suggestion
            )
        }
    }

    fn to_overlay_warning(&self) -> help_overlay::OverlayConfigWarning {
        help_overlay::OverlayConfigWarning {
            path: self.path.clone(),
            severity: overlay_warning_severity(self.severity),
            message: self.message.clone(),
            fix_path: self.suggestion.clone(),
        }
    }
}

thread_local! {
    /// Thread-local keyboard hook guard.
    ///
    /// The low-level hook handle (`HHOOK`) should be installed and unhooked on the
    /// same thread. Keeping the RAII guard in TLS preserves that ownership model
    /// and avoids sharing the Win32 handle wrapper across threads.
    static KEYBOARD_HOOK_HANDLE: RefCell<Option<KeyboardHook>> = const { RefCell::new(None) };
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
struct Config {
    key_bindings: Vec<(String, String)>,
    system_bindings: SystemBindings,
    polling_rate: u64,
    grid_size: GridSize,
    grid_mode: GridModeConfig,
    jump: JumpConfig,
    final_adjust: FinalAdjustConfig,
    status_overlay: StatusOverlayConfig,
    tooltip_overlay: TooltipOverlayConfig,
    mouse_speed: MouseSpeedConfig,
    slow_mouse: SlowMouseConfig,
    movement_profiles: HashMap<String, MouseSpeedConfig>,
    wheel: WheelConfig,
    wheel_profiles: HashMap<String, WheelProfileConfig>,
    edge_jump: EdgeJumpConfig,
    // Legacy compatibility alias for the movement baseline. New configs should use
    // [mouse_speed].default_speed; normalization keeps starting_speed synchronized.
    starting_speed: i32,
    acceleration: i32,      // Active ramp increment applied while movement is held.
    acceleration_rate: u32, // Active polling cycles before applying acceleration.
    top_speed: i32,         // Active legacy cap; treated as headroom above the baseline.
}

impl Default for Config {
    fn default() -> Self {
        Self {
            key_bindings: default_key_bindings(),
            system_bindings: SystemBindings::default(),
            polling_rate: DEFAULT_POLLING_RATE_MS,
            grid_size: GridSize::default(),
            grid_mode: GridModeConfig::default(),
            jump: JumpConfig::default(),
            final_adjust: FinalAdjustConfig::default(),
            status_overlay: StatusOverlayConfig::default(),
            tooltip_overlay: TooltipOverlayConfig::default(),
            mouse_speed: MouseSpeedConfig::default(),
            slow_mouse: SlowMouseConfig::default(),
            movement_profiles: HashMap::new(),
            wheel: WheelConfig::default(),
            wheel_profiles: HashMap::new(),
            edge_jump: EdgeJumpConfig::default(),
            starting_speed: 1,
            acceleration: 2,
            acceleration_rate: 1,
            top_speed: 6,
        }
    }
}

fn default_key_bindings() -> Vec<(String, String)> {
    [
        ("W", "move_up"),
        ("A", "move_left"),
        ("S", "move_down"),
        ("D", "move_right"),
        ("LeftShift", "slow_mouse"),
        ("SPACE", "left_click"),
        ("L", "right_click"),
        ("RightShift", "middle_click"),
        ("N", "toggle_drag_mode"),
        (".", "click_then_disable"),
        (",", "wheel_up"),
        ("M", "wheel_down"),
        ("I", "wheel_left"),
        ("O", "wheel_right"),
        ("X", "mouse_speed_down"),
        ("Z", "mouse_speed_reset"),
        ("V", "wheel_speed_up"),
        ("B", "wheel_speed_down"),
        ("RightAlt+C", "movement_profile_next"),
        ("RightAlt+X", "movement_profile_previous"),
        ("RightAlt+V", "wheel_profile_next"),
        ("RightAlt+B", "wheel_profile_previous"),
        ("F", "jump_mode"),
        ("G", "grid_mode"),
        ("C", "screen_select"),
        ("H", "navigate_back"),
        ("Y", "navigate_forward"),
        ("Q", "disable"),
        ("P", "disable"),
        ("RightAlt+W", "move_to_top_edge"),
        ("RightAlt+A", "move_to_left_edge"),
        ("RightAlt+S", "move_to_bottom_edge"),
        ("RightAlt+D", "move_to_right_edge"),
        ("RightAlt+R", "reload_config"),
        ("RightAlt+Escape", "panic_reset"),
        ("/", "show_help"),
    ]
    .into_iter()
    .map(|(key, action)| (key.to_string(), action.to_string()))
    .collect()
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TooltipOverlayPositioning {
    #[default]
    Cursor,
    Center,
    TopRight,
    BottomRight,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(default)]
pub struct TooltipOverlayEvents {
    pub mouse: bool,
    pub wheel: bool,
    pub profile: bool,
    pub drag: bool,
    pub reload: bool,
    pub panic: bool,
}

impl Default for TooltipOverlayEvents {
    fn default() -> Self {
        Self {
            mouse: true,
            wheel: true,
            profile: true,
            drag: true,
            reload: true,
            panic: true,
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(default)]
pub struct TooltipOverlayConfig {
    pub enabled: bool,
    pub show_temporary_tooltips: bool,
    pub show_help: bool,
    pub positioning: TooltipOverlayPositioning,
    pub offset_x: i32,
    pub offset_y: i32,
    pub duration_ms: u64,
    pub help_positioning: TooltipOverlayPositioning,
    pub help_width: i32,
    pub help_max_bindings: i32,
    pub events: TooltipOverlayEvents,
}

impl Default for TooltipOverlayConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            show_temporary_tooltips: true,
            show_help: true,
            positioning: TooltipOverlayPositioning::Cursor,
            offset_x: 18,
            offset_y: 18,
            duration_ms: DEFAULT_TOOLTIP_DURATION_MS,
            help_positioning: TooltipOverlayPositioning::Center,
            help_width: 420,
            help_max_bindings: 40,
            events: TooltipOverlayEvents::default(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum JumpAimPoint {
    #[default]
    Center,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    CustomOffset,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct FinalAdjustConfig {
    enabled: bool,
    small_step_px: i32,
    large_step_px: i32,
    modifier_key: String,
    confirm_key: String,
    cancel_key: String,
    back_key: String,
    show_hint: bool,
}

impl Default for FinalAdjustConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            small_step_px: 1,
            large_step_px: 10,
            modifier_key: "Shift".to_string(),
            confirm_key: "Enter".to_string(),
            cancel_key: "Escape".to_string(),
            back_key: "Backspace".to_string(),
            show_hint: true,
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(default)]
pub struct EdgeJumpConfig {
    offset_px: i32,
    use_work_area: bool,
}

impl Default for EdgeJumpConfig {
    fn default() -> Self {
        Self {
            offset_px: 1,
            use_work_area: false,
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(default)]
pub struct MouseSpeedConfig {
    default_speed: i32,
    min_speed: i32,
    max_speed: i32,
    speed_step: i32,
    flash_indicator_ms: u64,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SlowMouseStrategy {
    #[default]
    Fixed,
    Multiplier,
    Subtract,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
#[serde(default)]
pub struct SlowMouseConfig {
    strategy: SlowMouseStrategy,
    fixed_speed: i32,
    multiplier: f32,
    subtract_speed: i32,
    min_speed: i32,
    max_speed: i32,
    acceleration: i32,
    acceleration_rate: u32,
}

impl Default for SlowMouseConfig {
    fn default() -> Self {
        Self {
            strategy: SlowMouseStrategy::Fixed,
            fixed_speed: 1,
            multiplier: 0.25,
            subtract_speed: 4,
            min_speed: 1,
            max_speed: 2,
            acceleration: 0,
            acceleration_rate: 1,
        }
    }
}

impl Default for MouseSpeedConfig {
    fn default() -> Self {
        Self {
            default_speed: 1,
            min_speed: 1,
            max_speed: 12,
            speed_step: 1,
            flash_indicator_ms: DEFAULT_MOUSE_SPEED_FLASH_MS,
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(default)]
pub struct WheelConfig {
    default_speed: i32,
    min_speed: i32,
    max_speed: i32,
    speed_step: i32,
    tick_interval: u64,
    speed_indicator_ms: u64,
    vertical_multiplier: i32,
    horizontal_multiplier: i32,
}

impl Default for WheelConfig {
    fn default() -> Self {
        Self {
            default_speed: 3,
            min_speed: 1,
            max_speed: 12,
            speed_step: 1,
            tick_interval: DEFAULT_POLLING_RATE_MS,
            speed_indicator_ms: DEFAULT_WHEEL_SPEED_INDICATOR_MS,
            vertical_multiplier: 1,
            horizontal_multiplier: 1,
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(default)]
pub struct WheelProfileConfig {
    default_speed: Option<i32>,
    min_speed: Option<i32>,
    max_speed: Option<i32>,
    speed_step: Option<i32>,
    tick_interval: Option<u64>,
    speed_indicator_ms: Option<u64>,
    vertical_multiplier: Option<i32>,
    horizontal_multiplier: Option<i32>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
struct SystemBindings {
    toggle_active: String,
    exit: String,
}

impl Default for SystemBindings {
    fn default() -> Self {
        Self {
            toggle_active: "Ctrl+E".to_string(),
            exit: "Escape".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
struct GridSize {
    width: u32,
    height: u32,
}

impl Default for GridSize {
    fn default() -> Self {
        Self {
            width: 10,
            height: 10,
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
#[serde(default)]
struct GridModeConfig {
    enabled: bool,
    start_region: JumpStartRegion,
    width_percent: f32,
    height_percent: f32,
    center_on_cursor: bool,
    min_width_px: i32,
    min_height_px: i32,
    move_cursor_each_step: bool,
    line_visible: bool,
    show_direction_labels: bool,
}

impl Default for GridModeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            start_region: JumpStartRegion::CurrentMonitor,
            width_percent: 0.5,
            height_percent: 0.5,
            center_on_cursor: true,
            min_width_px: 80,
            min_height_px: 80,
            move_cursor_each_step: true,
            line_visible: true,
            show_direction_labels: true,
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
enum JumpMode {
    Single,
    #[default]
    Precision,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
enum CursorBetweenStagesMode {
    #[default]
    None,
    MoveToRegionCenter,
    PreviewOnly,
    WarpAndContinue,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PreviewEdgeBehavior {
    #[default]
    Clamp,
    ShiftIntoBounds,
    AllowAsymmetricContext,
    DisableContextNearEdges,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum JumpStartRegion {
    VirtualScreen,
    #[default]
    CurrentMonitor,
    ActiveWindowMonitor,
    ActiveWindowBounds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JumpTargetRegionMode {
    #[default]
    ExactRegion,
    RegionWithContext,
    ExpandedTarget,
    CursorCenteredZoom,
}

fn parse_jump_target_region_mode(value: &str) -> Option<JumpTargetRegionMode> {
    match value {
        "exact_region" => Some(JumpTargetRegionMode::ExactRegion),
        "region_with_context" => Some(JumpTargetRegionMode::RegionWithContext),
        "expanded_target" => Some(JumpTargetRegionMode::ExpandedTarget),
        "cursor_centered_zoom" => Some(JumpTargetRegionMode::CursorCenteredZoom),
        _ => None,
    }
}

#[derive(Debug, Clone)]
struct JumpConfig {
    mode: JumpMode,
    cursor_between_stages: CursorBetweenStagesMode,
    start_region: JumpStartRegion,
    preview_edge_behavior: PreviewEdgeBehavior,
    hints: JumpHintsConfig,
    visuals: JumpVisualsConfig,
    coarse: JumpStageConfig,
    fine: JumpStageConfig,
    precise: JumpStageConfig,
    profiles: HashMap<String, JumpProfileConfig>,
}

impl Default for JumpConfig {
    fn default() -> Self {
        Self {
            mode: JumpMode::Precision,
            cursor_between_stages: CursorBetweenStagesMode::None,
            start_region: JumpStartRegion::CurrentMonitor,
            preview_edge_behavior: PreviewEdgeBehavior::Clamp,
            hints: JumpHintsConfig::default(),
            visuals: JumpVisualsConfig::default(),
            coarse: JumpStageConfig::missing_coarse(),
            fine: JumpStageConfig {
                enabled: true,
                width: 5,
                height: 5,
                aim_point: JumpAimPoint::Center,
                aim_offset_x_px: 0,
                aim_offset_y_px: 0,
                target_margin_percent: 0,
                visual_context_margin_percent: 0,
                zoom_scale: 1.0,
                target_region_mode: JumpTargetRegionMode::ExactRegion,
                preview_edge_behavior: None,
                labels: JumpLabelConfig::default(),
                legacy_preview_margin_percent: None,
                legacy_preview_margin_percent_used: false,
                target_region_mode_invalid: false,
            },
            precise: JumpStageConfig {
                enabled: false,
                width: 3,
                height: 3,
                aim_point: JumpAimPoint::Center,
                aim_offset_x_px: 0,
                aim_offset_y_px: 0,
                target_margin_percent: 0,
                visual_context_margin_percent: 5,
                zoom_scale: 2.5,
                target_region_mode: JumpTargetRegionMode::ExactRegion,
                preview_edge_behavior: None,
                labels: JumpLabelConfig::default(),
                legacy_preview_margin_percent: None,
                legacy_preview_margin_percent_used: false,
                target_region_mode_invalid: false,
            },
            profiles: HashMap::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct JumpConfigToml {
    mode: JumpMode,
    cursor_between_stages: Option<CursorBetweenStagesMode>,
    move_cursor_after_each_stage: Option<bool>,
    start_region: JumpStartRegion,
    preview_edge_behavior: PreviewEdgeBehavior,
    hints: JumpHintsConfig,
    visuals: JumpVisualsConfig,
    coarse: JumpStageConfig,
    fine: JumpStageConfig,
    precise: JumpStageConfig,
    profiles: HashMap<String, JumpProfileConfig>,
}

impl Default for JumpConfigToml {
    fn default() -> Self {
        let default = JumpConfig::default();
        Self {
            mode: default.mode,
            cursor_between_stages: None,
            move_cursor_after_each_stage: None,
            start_region: default.start_region,
            preview_edge_behavior: default.preview_edge_behavior,
            hints: default.hints,
            visuals: default.visuals,
            coarse: default.coarse,
            fine: default.fine,
            precise: default.precise,
            profiles: default.profiles,
        }
    }
}

impl<'de> Deserialize<'de> for JumpConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let fields = JumpConfigToml::deserialize(deserializer)?;
        let cursor_between_stages = fields.cursor_between_stages.unwrap_or_else(|| {
            if fields.move_cursor_after_each_stage.unwrap_or(false) {
                CursorBetweenStagesMode::MoveToRegionCenter
            } else {
                CursorBetweenStagesMode::None
            }
        });

        Ok(Self {
            mode: fields.mode,
            cursor_between_stages,
            start_region: fields.start_region,
            preview_edge_behavior: fields.preview_edge_behavior,
            hints: fields.hints,
            visuals: fields.visuals,
            coarse: fields.coarse,
            fine: fields.fine,
            precise: fields.precise,
            profiles: fields.profiles,
        })
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(default)]
struct JumpProfileConfig {
    mode: Option<JumpMode>,
    cursor_between_stages: Option<CursorBetweenStagesMode>,
    start_region: Option<JumpStartRegion>,
    preview_edge_behavior: Option<PreviewEdgeBehavior>,
    visuals: Option<JumpVisualsConfig>,
    coarse: Option<JumpStageConfig>,
    fine: Option<JumpStageConfig>,
    precise: Option<JumpStageConfig>,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(default)]
struct JumpHintsConfig {
    selection_keys: String,
}

impl Default for JumpHintsConfig {
    fn default() -> Self {
        Self {
            selection_keys: "ABCDEFGHIJKLMNOPQRSTUVWXYZ".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(default)]
pub struct JumpVisualsConfig {
    selected_region_outline: bool,
    preview_outline: bool,
    active_grid_outline: bool,
    cell_centers: bool,
    final_crosshair: bool,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
#[serde(default)]
pub struct JumpLabelConfig {
    font_scale: f32,
    center_marker: bool,
    separators: bool,
    hide_threshold_px: i32,
}

impl Default for JumpLabelConfig {
    fn default() -> Self {
        Self {
            font_scale: 1.0,
            center_marker: false,
            separators: true,
            hide_threshold_px: 0,
        }
    }
}

impl From<JumpLabelConfig> for JumpLabelMetadata {
    fn from(config: JumpLabelConfig) -> Self {
        Self {
            font_scale: config.font_scale,
            center_marker: config.center_marker,
            separators: config.separators,
            hide_threshold_px: config.hide_threshold_px,
        }
    }
}

impl Default for JumpVisualsConfig {
    fn default() -> Self {
        Self {
            selected_region_outline: true,
            preview_outline: true,
            active_grid_outline: true,
            cell_centers: false,
            final_crosshair: true,
        }
    }
}

impl From<JumpVisualsConfig> for JumpVisuals {
    fn from(config: JumpVisualsConfig) -> Self {
        Self {
            selected_region_outline: config.selected_region_outline,
            preview_outline: config.preview_outline,
            active_grid_outline: config.active_grid_outline,
            cell_centers: config.cell_centers,
            final_crosshair: config.final_crosshair,
        }
    }
}

#[derive(Debug, Clone)]
struct JumpStageConfig {
    enabled: bool,
    width: u32,
    height: u32,
    aim_point: JumpAimPoint,
    aim_offset_x_px: i32,
    aim_offset_y_px: i32,
    target_margin_percent: u8,
    visual_context_margin_percent: u8,
    zoom_scale: f32,
    target_region_mode: JumpTargetRegionMode,
    preview_edge_behavior: Option<PreviewEdgeBehavior>,
    labels: JumpLabelConfig,
    legacy_preview_margin_percent: Option<u8>,
    legacy_preview_margin_percent_used: bool,
    target_region_mode_invalid: bool,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct JumpStageConfigToml {
    enabled: bool,
    width: u32,
    height: u32,
    aim_point: JumpAimPoint,
    aim_offset_x_px: i32,
    aim_offset_y_px: i32,
    target_margin_percent: u8,
    visual_context_margin_percent: Option<u8>,
    zoom_scale: f32,
    target_region_mode: Option<String>,
    preview_edge_behavior: Option<PreviewEdgeBehavior>,
    labels: JumpLabelConfig,
    #[serde(default, rename = "preview_margin_percent")]
    legacy_preview_margin_percent: Option<u8>,
}

impl Default for JumpStageConfigToml {
    fn default() -> Self {
        let default = JumpStageConfig::default();
        Self {
            enabled: default.enabled,
            width: default.width,
            height: default.height,
            aim_point: default.aim_point,
            aim_offset_x_px: default.aim_offset_x_px,
            aim_offset_y_px: default.aim_offset_y_px,
            target_margin_percent: default.target_margin_percent,
            visual_context_margin_percent: None,
            zoom_scale: default.zoom_scale,
            target_region_mode: None,
            preview_edge_behavior: default.preview_edge_behavior,
            labels: default.labels,
            legacy_preview_margin_percent: default.legacy_preview_margin_percent,
        }
    }
}

impl<'de> Deserialize<'de> for JumpStageConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let fields = JumpStageConfigToml::deserialize(deserializer)?;
        let (target_region_mode, target_region_mode_invalid) = fields
            .target_region_mode
            .as_deref()
            .map(|value| {
                parse_jump_target_region_mode(value)
                    .map(|mode| (mode, false))
                    .unwrap_or((JumpTargetRegionMode::ExactRegion, true))
            })
            .unwrap_or((JumpTargetRegionMode::ExactRegion, false));
        let legacy_preview_margin_percent_used = fields.visual_context_margin_percent.is_none()
            && fields.legacy_preview_margin_percent.is_some();
        let visual_context_margin_percent = fields
            .visual_context_margin_percent
            .or(fields.legacy_preview_margin_percent)
            .unwrap_or_default();

        Ok(Self {
            enabled: fields.enabled,
            width: fields.width,
            height: fields.height,
            aim_point: fields.aim_point,
            aim_offset_x_px: fields.aim_offset_x_px,
            aim_offset_y_px: fields.aim_offset_y_px,
            target_margin_percent: fields.target_margin_percent,
            visual_context_margin_percent,
            zoom_scale: fields.zoom_scale,
            target_region_mode,
            preview_edge_behavior: fields.preview_edge_behavior,
            labels: fields.labels,
            legacy_preview_margin_percent: fields.legacy_preview_margin_percent,
            legacy_preview_margin_percent_used,
            target_region_mode_invalid,
        })
    }
}

impl JumpStageConfig {
    fn missing_coarse() -> Self {
        Self {
            enabled: true,
            width: 0,
            height: 0,
            aim_point: JumpAimPoint::Center,
            aim_offset_x_px: 0,
            aim_offset_y_px: 0,
            target_margin_percent: 0,
            visual_context_margin_percent: 0,
            zoom_scale: 1.0,
            target_region_mode: JumpTargetRegionMode::ExactRegion,
            preview_edge_behavior: None,
            labels: JumpLabelConfig::default(),
            legacy_preview_margin_percent: None,
            legacy_preview_margin_percent_used: false,
            target_region_mode_invalid: false,
        }
    }
}

impl Default for JumpStageConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            width: 1,
            height: 1,
            aim_point: JumpAimPoint::Center,
            aim_offset_x_px: 0,
            aim_offset_y_px: 0,
            target_margin_percent: 0,
            visual_context_margin_percent: 0,
            zoom_scale: 1.0,
            target_region_mode: JumpTargetRegionMode::ExactRegion,
            preview_edge_behavior: None,
            labels: JumpLabelConfig::default(),
            legacy_preview_margin_percent: None,
            legacy_preview_margin_percent_used: false,
            target_region_mode_invalid: false,
        }
    }
}

impl Config {
    fn normalize(mut self) -> Result<Self, Box<dyn Error>> {
        if self.polling_rate == 0 {
            self.polling_rate = DEFAULT_POLLING_RATE_MS;
        }
        self.normalize_grid_mode_config();
        self.normalize_jump_config();
        self.normalize_final_adjust_config();
        self.normalize_mouse_speed_config();
        self.normalize_slow_mouse_config();
        self.normalize_wheel_config();
        self.normalize_runtime_profiles();
        self.normalize_edge_jump_config();
        self.normalize_tooltip_overlay_config();
        // Keep the legacy field stable for downstream code and diagnostics after the
        // modern baseline has won normalization.
        self.starting_speed = self.mouse_speed.default_speed;
        self.runtime_system_bindings()?;
        Ok(self)
    }

    fn normalize_grid_mode_config(&mut self) {
        self.grid_mode.width_percent = normalize_f32_range(
            "grid_mode.width_percent",
            self.grid_mode.width_percent,
            MIN_GRID_MODE_REGION_PERCENT,
            MAX_GRID_MODE_REGION_PERCENT,
        );
        self.grid_mode.height_percent = normalize_f32_range(
            "grid_mode.height_percent",
            self.grid_mode.height_percent,
            MIN_GRID_MODE_REGION_PERCENT,
            MAX_GRID_MODE_REGION_PERCENT,
        );
        self.grid_mode.min_width_px = normalize_i32_range(
            "grid_mode.min_width_px",
            self.grid_mode.min_width_px,
            MIN_GRID_MODE_SIZE_PX,
            MAX_GRID_MODE_SIZE_PX,
        );
        self.grid_mode.min_height_px = normalize_i32_range(
            "grid_mode.min_height_px",
            self.grid_mode.min_height_px,
            MIN_GRID_MODE_SIZE_PX,
            MAX_GRID_MODE_SIZE_PX,
        );
    }

    fn normalize_tooltip_overlay_config(&mut self) {
        if self.tooltip_overlay.duration_ms == 0 {
            warn_config_normalized("tooltip_overlay.duration_ms is 0; using default");
            self.tooltip_overlay.duration_ms = DEFAULT_TOOLTIP_DURATION_MS;
        }
        self.tooltip_overlay.help_width = normalize_i32_range(
            "tooltip_overlay.help_width",
            self.tooltip_overlay.help_width,
            MIN_TOOLTIP_HELP_WIDTH,
            MAX_TOOLTIP_HELP_WIDTH,
        );
        self.tooltip_overlay.help_max_bindings = normalize_i32_range(
            "tooltip_overlay.help_max_bindings",
            self.tooltip_overlay.help_max_bindings,
            MIN_TOOLTIP_HELP_BINDINGS,
            MAX_TOOLTIP_HELP_BINDINGS,
        );
        self.tooltip_overlay.offset_x = normalize_i32_range(
            "tooltip_overlay.offset_x",
            self.tooltip_overlay.offset_x,
            MIN_TOOLTIP_OFFSET,
            MAX_TOOLTIP_OFFSET,
        );
        self.tooltip_overlay.offset_y = normalize_i32_range(
            "tooltip_overlay.offset_y",
            self.tooltip_overlay.offset_y,
            MIN_TOOLTIP_OFFSET,
            MAX_TOOLTIP_OFFSET,
        );
    }

    fn normalize_edge_jump_config(&mut self) {
        if self.edge_jump.offset_px < 0 {
            warn_config_normalized("edge_jump.offset_px is below 0; clamping to 0");
            self.edge_jump.offset_px = 0;
        } else if self.edge_jump.offset_px > MAX_EDGE_JUMP_OFFSET_PX {
            warn_config_normalized(&format!(
                "edge_jump.offset_px is above {}; clamping to {}",
                MAX_EDGE_JUMP_OFFSET_PX, MAX_EDGE_JUMP_OFFSET_PX
            ));
            self.edge_jump.offset_px = MAX_EDGE_JUMP_OFFSET_PX;
        }
    }

    fn normalize_final_adjust_config(&mut self) {
        self.final_adjust.small_step_px = normalize_final_adjust_step(
            "final_adjust.small_step_px",
            self.final_adjust.small_step_px,
            FinalAdjustConfig::default().small_step_px,
        );
        self.final_adjust.large_step_px = normalize_final_adjust_step(
            "final_adjust.large_step_px",
            self.final_adjust.large_step_px,
            FinalAdjustConfig::default().large_step_px,
        );
        if self.final_adjust.large_step_px < self.final_adjust.small_step_px {
            warn_config_normalized(
                "final_adjust.large_step_px is below final_adjust.small_step_px; using small step",
            );
            self.final_adjust.large_step_px = self.final_adjust.small_step_px;
        }

        let defaults = FinalAdjustConfig::default();
        if VirtualKey::from_string(&self.final_adjust.modifier_key).is_none() {
            warn_config_normalized("final_adjust.modifier_key is invalid; using Shift");
            self.final_adjust.modifier_key = defaults.modifier_key;
        }
        if VirtualKey::from_string(&self.final_adjust.confirm_key).is_none() {
            warn_config_normalized("final_adjust.confirm_key is invalid; using Enter");
            self.final_adjust.confirm_key = defaults.confirm_key;
        }
        if VirtualKey::from_string(&self.final_adjust.cancel_key).is_none() {
            warn_config_normalized("final_adjust.cancel_key is invalid; using Escape");
            self.final_adjust.cancel_key = defaults.cancel_key;
        }
        if VirtualKey::from_string(&self.final_adjust.back_key).is_none() {
            warn_config_normalized("final_adjust.back_key is invalid; using Backspace");
            self.final_adjust.back_key = defaults.back_key;
        }
    }

    fn normalize_mouse_speed_config(&mut self) {
        let legacy_starting_speed = self.starting_speed;
        let default_mouse_speed = MouseSpeedConfig::default().default_speed;
        let legacy_starting_speed_set = legacy_starting_speed != Config::default().starting_speed;

        if self.mouse_speed.min_speed < 1 {
            warn_config_normalized("mouse_speed.min_speed is below 1; clamping to 1");
            self.mouse_speed.min_speed = 1;
        }

        if self.mouse_speed.max_speed < self.mouse_speed.min_speed {
            warn_config_normalized(
                "mouse_speed.max_speed is below mouse_speed.min_speed; clamping to min",
            );
            self.mouse_speed.max_speed = self.mouse_speed.min_speed;
        }

        if self.mouse_speed.speed_step < 1 {
            warn_config_normalized("mouse_speed.speed_step is below 1; clamping to 1");
            self.mouse_speed.speed_step = 1;
        }

        self.mouse_speed.default_speed = self
            .mouse_speed
            .default_speed
            .clamp(self.mouse_speed.min_speed, self.mouse_speed.max_speed);

        if self.mouse_speed.flash_indicator_ms == 0 {
            warn_config_normalized("mouse_speed.flash_indicator_ms is 0; using default");
            self.mouse_speed.flash_indicator_ms = DEFAULT_MOUSE_SPEED_FLASH_MS;
        }

        if legacy_starting_speed_set && self.mouse_speed.default_speed == default_mouse_speed {
            warn_config_info(
                "starting_speed is a compatibility field; prefer [mouse_speed].default_speed",
            );
            self.mouse_speed.default_speed = self
                .starting_speed
                .clamp(self.mouse_speed.min_speed, self.mouse_speed.max_speed);
        } else if legacy_starting_speed_set
            && legacy_starting_speed != self.mouse_speed.default_speed
        {
            warn_config_info(&format!(
                "starting_speed={} is ignored because [mouse_speed].default_speed={} is the preferred movement baseline",
                legacy_starting_speed, self.mouse_speed.default_speed
            ));
        }
    }

    fn normalize_wheel_config(&mut self) {
        if self.wheel.min_speed < 1 {
            warn_config_normalized("wheel.min_speed is below 1; clamping to 1");
            self.wheel.min_speed = 1;
        }

        if self.wheel.max_speed < self.wheel.min_speed {
            warn_config_normalized("wheel.max_speed is below wheel.min_speed; clamping to min");
            self.wheel.max_speed = self.wheel.min_speed;
        }

        if self.wheel.speed_step < 1 {
            warn_config_normalized("wheel.speed_step is below 1; clamping to 1");
            self.wheel.speed_step = 1;
        }

        self.wheel.default_speed = self
            .wheel
            .default_speed
            .clamp(self.wheel.min_speed, self.wheel.max_speed);

        if self.wheel.tick_interval == 0 {
            warn_config_normalized("wheel.tick_interval is 0; using polling_rate");
            self.wheel.tick_interval = self.polling_rate;
        }

        if self.wheel.speed_indicator_ms == 0 {
            warn_config_normalized("wheel.speed_indicator_ms is 0; using default");
            self.wheel.speed_indicator_ms = DEFAULT_WHEEL_SPEED_INDICATOR_MS;
        }

        if self.wheel.vertical_multiplier < 1 {
            warn_config_normalized("wheel.vertical_multiplier is below 1; clamping to 1");
            self.wheel.vertical_multiplier = 1;
        }

        if self.wheel.horizontal_multiplier < 1 {
            warn_config_normalized("wheel.horizontal_multiplier is below 1; clamping to 1");
            self.wheel.horizontal_multiplier = 1;
        }
    }

    fn normalize_slow_mouse_config(&mut self) {
        if self.slow_mouse.min_speed < 1 {
            warn_config_normalized("slow_mouse.min_speed is below 1; clamping to 1");
            self.slow_mouse.min_speed = 1;
        }

        if self.slow_mouse.max_speed < self.slow_mouse.min_speed {
            warn_config_normalized(
                "slow_mouse.max_speed is below slow_mouse.min_speed; clamping to min",
            );
            self.slow_mouse.max_speed = self.slow_mouse.min_speed;
        }

        let clamped_fixed = self
            .slow_mouse
            .fixed_speed
            .clamp(self.slow_mouse.min_speed, self.slow_mouse.max_speed);
        if clamped_fixed != self.slow_mouse.fixed_speed {
            warn_config_normalized(
                "slow_mouse.fixed_speed is outside slow_mouse min/max range; clamping",
            );
            self.slow_mouse.fixed_speed = clamped_fixed;
        }

        if self.slow_mouse.multiplier <= 0.0 {
            warn_config_normalized("slow_mouse.multiplier is <= 0.0; using default 0.25");
            self.slow_mouse.multiplier = SlowMouseConfig::default().multiplier;
        } else if self.slow_mouse.multiplier > 1.0 {
            warn_config_normalized("slow_mouse.multiplier is above 1.0; clamping to 1.0");
            self.slow_mouse.multiplier = 1.0;
        }

        if self.slow_mouse.subtract_speed < 0 {
            warn_config_normalized("slow_mouse.subtract_speed is below 0; clamping to 0");
            self.slow_mouse.subtract_speed = 0;
        }

        if self.slow_mouse.acceleration < 0 {
            warn_config_normalized("slow_mouse.acceleration is below 0; clamping to 0");
            self.slow_mouse.acceleration = 0;
        }

        if self.slow_mouse.acceleration_rate == 0 {
            warn_config_normalized("slow_mouse.acceleration_rate is 0; clamping to 1");
            self.slow_mouse.acceleration_rate = 1;
        }
    }

    fn normalize_runtime_profiles(&mut self) {
        let movement_profile_names: Vec<String> = self.movement_profiles.keys().cloned().collect();
        for name in movement_profile_names {
            if let Some(profile) = self.movement_profiles.get_mut(&name) {
                normalize_mouse_speed_settings(&format!("movement_profiles.{name}"), profile);
            }
        }

        let wheel_profile_names: Vec<String> = self.wheel_profiles.keys().cloned().collect();
        for name in wheel_profile_names {
            if let Some(profile) = self.wheel_profiles.get(&name).copied() {
                let mut resolved = self.resolve_wheel_profile_from(profile);
                normalize_wheel_settings(
                    &format!("wheel_profiles.{name}"),
                    &mut resolved,
                    self.polling_rate,
                );
            }
        }
    }

    fn normalize_jump_config(&mut self) {
        if self.jump.coarse.width == 0 && self.jump.coarse.height == 0 {
            warn_config_normalized(&format!(
                "jump.coarse missing; deriving coarse size from legacy grid_size {}x{}",
                self.grid_size.width, self.grid_size.height
            ));
            self.jump.coarse.width = self.grid_size.width;
            self.jump.coarse.height = self.grid_size.height;
        }

        self.jump.hints.selection_keys = normalize_jump_selection_keys(
            "jump.hints.selection_keys",
            &self.jump.hints.selection_keys,
        );
        normalize_jump_stage("jump.coarse", &mut self.jump.coarse);
        normalize_jump_stage("jump.fine", &mut self.jump.fine);
        normalize_jump_stage("jump.precise", &mut self.jump.precise);

        match self.jump.mode {
            JumpMode::Single => {
                if self.jump.fine.enabled {
                    warn_config_normalized("jump.mode=single; disabling jump.fine");
                }
                if self.jump.precise.enabled {
                    warn_config_normalized("jump.mode=single; disabling jump.precise");
                }
                self.jump.fine.enabled = false;
                self.jump.precise.enabled = false;
            }
            JumpMode::Precision => {
                if !self.jump.fine.enabled && self.jump.precise.enabled {
                    warn_config_normalized("jump.precise disabled because jump.fine is disabled");
                    self.jump.precise.enabled = false;
                }
            }
        }

        self.grid_size = GridSize {
            width: self.jump.coarse.width,
            height: self.jump.coarse.height,
        };

        let profile_names: Vec<String> = self.jump.profiles.keys().cloned().collect();
        for name in profile_names {
            if let Some(profile) = self.jump.profiles.get_mut(&name) {
                if let Some(stage) = &mut profile.coarse {
                    normalize_jump_stage(&format!("jump.profiles.{name}.coarse"), stage);
                }
                if let Some(stage) = &mut profile.fine {
                    normalize_jump_stage(&format!("jump.profiles.{name}.fine"), stage);
                }
                if let Some(stage) = &mut profile.precise {
                    normalize_jump_stage(&format!("jump.profiles.{name}.precise"), stage);
                }
            }
        }
    }

    fn resolved_jump_config(&self, profile_name: Option<&str>) -> Result<JumpConfig, String> {
        let Some(profile_name) = profile_name else {
            return Ok(self.jump.clone());
        };
        let Some(profile) = self.jump.profiles.get(profile_name) else {
            return Err(format!("jump profile '{profile_name}' does not exist"));
        };

        let mut jump = self.jump.clone();
        jump.profiles.clear();
        if let Some(mode) = profile.mode {
            jump.mode = mode;
        }
        if let Some(cursor_between_stages) = profile.cursor_between_stages {
            jump.cursor_between_stages = cursor_between_stages;
        }
        if let Some(start_region) = profile.start_region {
            jump.start_region = start_region;
        }
        if let Some(preview_edge_behavior) = profile.preview_edge_behavior {
            jump.preview_edge_behavior = preview_edge_behavior;
        }
        if let Some(visuals) = profile.visuals {
            jump.visuals = visuals;
        }
        if let Some(stage) = &profile.coarse {
            jump.coarse = stage.clone();
        }
        if let Some(stage) = &profile.fine {
            jump.fine = stage.clone();
        }
        if let Some(stage) = &profile.precise {
            jump.precise = stage.clone();
        }
        Ok(jump)
    }

    fn resolved_movement_profile(
        &self,
        profile_name: Option<&str>,
    ) -> Result<MouseSpeedConfig, String> {
        let Some(profile_name) = profile_name else {
            return Ok(self.mouse_speed);
        };
        self.movement_profiles
            .get(profile_name)
            .copied()
            .ok_or_else(|| format!("movement profile '{profile_name}' does not exist"))
    }

    fn resolved_wheel_profile(&self, profile_name: Option<&str>) -> Result<WheelConfig, String> {
        let Some(profile_name) = profile_name else {
            return Ok(self.wheel);
        };
        let profile = self
            .wheel_profiles
            .get(profile_name)
            .copied()
            .ok_or_else(|| format!("wheel profile '{profile_name}' does not exist"))?;
        let mut wheel = self.resolve_wheel_profile_from(profile);
        normalize_wheel_settings(
            &format!("wheel_profiles.{profile_name}"),
            &mut wheel,
            self.polling_rate,
        );
        Ok(wheel)
    }

    fn resolve_wheel_profile_from(&self, profile: WheelProfileConfig) -> WheelConfig {
        let mut wheel = self.wheel;
        if let Some(value) = profile.default_speed {
            wheel.default_speed = value;
        }
        if let Some(value) = profile.min_speed {
            wheel.min_speed = value;
        }
        if let Some(value) = profile.max_speed {
            wheel.max_speed = value;
        }
        if let Some(value) = profile.speed_step {
            wheel.speed_step = value;
        }
        if let Some(value) = profile.tick_interval {
            wheel.tick_interval = value;
        }
        if let Some(value) = profile.speed_indicator_ms {
            wheel.speed_indicator_ms = value;
        }
        if let Some(value) = profile.vertical_multiplier {
            wheel.vertical_multiplier = value;
        }
        if let Some(value) = profile.horizontal_multiplier {
            wheel.horizontal_multiplier = value;
        }
        wheel
    }

    fn load_from_file(path: &str) -> Result<Self, Box<dyn Error>> {
        // Try to read the config from the provided path relative to the current
        // working directory.  If that fails, fall back to looking in the same
        // directory as the executable.  This allows running the binary from any
        // location as long as `config.toml` sits next to it.

        // DEBUG: print current working directory and executable path
        if let Ok(cwd) = env::current_dir() {
            println!("[DEBUG] current_dir: {}", cwd.display());
        } else {
            println!("[DEBUG] current_dir: <failed>");
        }

        if let Ok(exe) = env::current_exe() {
            println!("[DEBUG] current_exe: {}", exe.display());
        } else {
            println!("[DEBUG] current_exe: <failed>");
        }

        // First attempt: path relative to current directory
        println!("[DEBUG] trying path: {}", path);
        match fs::read_to_string(path) {
            Ok(config_str) => return Self::parse_audited_config(&config_str),
            Err(e) => {
                if e.kind() != io::ErrorKind::NotFound {
                    return Err(e.into());
                }
            }
        }

        // Second attempt: path relative to the executable location
        if let Ok(mut exe_path) = env::current_exe() {
            exe_path.pop();
            exe_path.push(path);
            println!("[DEBUG] trying exe path: {}", exe_path.display());
            match fs::read_to_string(&exe_path) {
                Ok(config_str) => return Self::parse_audited_config(&config_str),
                Err(e) => {
                    if e.kind() != io::ErrorKind::NotFound {
                        return Err(e.into());
                    }
                }
            }
        }

        eprintln!("Config file not found, using defaults");
        Self::default().normalize()
    }

    fn parse_audited_config(config_str: &str) -> Result<Self, Box<dyn Error>> {
        emit_config_audit_warnings(&audit_config_toml(config_str).warnings);
        toml::from_str::<Self>(config_str)?.normalize()
    }

    fn runtime_system_bindings(&self) -> Result<RuntimeSystemBindings, Box<dyn Error>> {
        Ok(RuntimeSystemBindings::new(
            KeyChord::parse(&self.system_bindings.toggle_active)
                .map_err(|e| format!("system_bindings.toggle_active: {e}"))?,
            KeyChord::parse(&self.system_bindings.exit)
                .map_err(|e| format!("system_bindings.exit: {e}"))?,
        ))
    }

    fn initialize_bindings(&self) {
        let mut key_actions = KEY_ACTIONS.write().unwrap(); // Acquire write lock
        key_actions.clear();

        for (key, action_str) in &self.key_bindings {
            if let Ok(chord) = KeyChord::parse(key) {
                if let Some(action) = Action::from_string(action_str) {
                    println!("✅ Binding key: {:?} -> {:?}", chord, action);
                    key_actions.add_chord_binding(chord, action);
                } else {
                    println!(
                        "❌ Action '{}' does not exist for key '{}'",
                        action_str, key
                    );
                }
            } else {
                println!("❌ Key '{}' is not recognized", key);
            }
        }

        let mut grid_direction_labels = crate::app_state::GridDirectionLabels::default();
        for (chord, action) in key_actions.entries() {
            let label = format!("{:?}", chord.key);
            match action {
                Action::MoveUp => grid_direction_labels.up = label,
                Action::MoveLeft => grid_direction_labels.left = label,
                Action::MoveDown => grid_direction_labels.down = label,
                Action::MoveRight => grid_direction_labels.right = label,
                _ => {}
            }
        }

        let mut app_state = APP_STATE.write().unwrap();
        app_state.set_grid_direction_labels(grid_direction_labels);
        app_state.set_bound_chords(key_actions.bound_chords());
    }

    fn initialize_system_bindings(&self) -> Result<(), Box<dyn Error>> {
        let system_bindings = self.runtime_system_bindings()?;

        println!(
            "✅ System bindings resolved: toggle_active={:?}, exit={:?}",
            system_bindings.toggle_active, system_bindings.exit
        );

        APP_STATE
            .write()
            .unwrap()
            .set_system_bindings(system_bindings);

        Ok(())
    }
}

fn warn_config_normalized(message: &str) {
    if let Ok(mut warnings) = CONFIG_WARNINGS.lock() {
        warnings.push(StoredConfigWarning::runtime_warning(message));
    }
    eprintln!("[config warning] {message}");
}

fn warn_config_info(message: &str) {
    let message = format!("info: {message}");
    if let Ok(mut warnings) = CONFIG_WARNINGS.lock() {
        warnings.push(StoredConfigWarning::runtime_info(&message));
    }
    eprintln!("[config warning] {message}");
}

fn emit_config_audit_warnings(warnings: &[ConfigAuditWarning]) {
    for warning in warnings {
        let message = format_config_audit_warning(warning);
        if let Ok(mut stored_warnings) = CONFIG_WARNINGS.lock() {
            stored_warnings.push(StoredConfigWarning::from_audit(warning));
        }
        eprintln!(
            "{} {message}",
            config_audit_warning_prefix(warning.severity)
        );
    }
}

fn config_audit_warning_prefix(severity: ConfigAuditSeverity) -> &'static str {
    match severity {
        ConfigAuditSeverity::Info | ConfigAuditSeverity::Warning => "[config warning]",
        ConfigAuditSeverity::Deprecated => "[config deprecated]",
    }
}

fn format_config_audit_warning(warning: &ConfigAuditWarning) -> String {
    format!(
        "{}: {} Suggestion: {}",
        warning.path, warning.message, warning.suggestion
    )
}

fn take_config_warnings() -> Vec<String> {
    CONFIG_WARNINGS
        .lock()
        .map(|mut warnings| {
            std::mem::take(&mut *warnings)
                .into_iter()
                .map(|warning| warning.summary_text())
                .collect()
        })
        .unwrap_or_default()
}

fn overlay_warning_severity(severity: ConfigAuditSeverity) -> help_overlay::OverlayWarningSeverity {
    match severity {
        ConfigAuditSeverity::Info => help_overlay::OverlayWarningSeverity::Info,
        ConfigAuditSeverity::Warning => help_overlay::OverlayWarningSeverity::Warning,
        ConfigAuditSeverity::Deprecated => help_overlay::OverlayWarningSeverity::Deprecated,
    }
}

fn recent_config_warnings_for_overlay() -> Vec<help_overlay::OverlayConfigWarning> {
    CONFIG_WARNINGS
        .lock()
        .map(|warnings| {
            warnings
                .iter()
                .map(StoredConfigWarning::to_overlay_warning)
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_i32_range(name: &str, value: i32, min: i32, max: i32) -> i32 {
    if value < min {
        warn_config_normalized(&format!("{name} is below {min}; clamping to {min}"));
        min
    } else if value > max {
        warn_config_normalized(&format!("{name} is above {max}; clamping to {max}"));
        max
    } else {
        value
    }
}

fn normalize_f32_range(name: &str, value: f32, min: f32, max: f32) -> f32 {
    if !value.is_finite() {
        warn_config_normalized(&format!("{name} is not finite; using {min}"));
        min
    } else if value < min {
        warn_config_normalized(&format!("{name} is below {min}; clamping to {min}"));
        min
    } else if value > max {
        warn_config_normalized(&format!("{name} is above {max}; clamping to {max}"));
        max
    } else {
        value
    }
}

fn normalize_mouse_speed_settings(name: &str, settings: &mut MouseSpeedConfig) {
    if settings.min_speed < 1 {
        warn_config_normalized(&format!("{name}.min_speed is below 1; clamping to 1"));
        settings.min_speed = 1;
    }
    if settings.max_speed < settings.min_speed {
        warn_config_normalized(&format!(
            "{name}.max_speed is below {name}.min_speed; clamping to min"
        ));
        settings.max_speed = settings.min_speed;
    }
    if settings.speed_step < 1 {
        warn_config_normalized(&format!("{name}.speed_step is below 1; clamping to 1"));
        settings.speed_step = 1;
    }
    settings.default_speed = settings
        .default_speed
        .clamp(settings.min_speed, settings.max_speed);
    if settings.flash_indicator_ms == 0 {
        warn_config_normalized(&format!("{name}.flash_indicator_ms is 0; using default"));
        settings.flash_indicator_ms = DEFAULT_MOUSE_SPEED_FLASH_MS;
    }
}

fn normalize_wheel_settings(name: &str, settings: &mut WheelConfig, polling_rate: u64) {
    if settings.min_speed < 1 {
        warn_config_normalized(&format!("{name}.min_speed is below 1; clamping to 1"));
        settings.min_speed = 1;
    }
    if settings.max_speed < settings.min_speed {
        warn_config_normalized(&format!(
            "{name}.max_speed is below {name}.min_speed; clamping to min"
        ));
        settings.max_speed = settings.min_speed;
    }
    if settings.speed_step < 1 {
        warn_config_normalized(&format!("{name}.speed_step is below 1; clamping to 1"));
        settings.speed_step = 1;
    }
    settings.default_speed = settings
        .default_speed
        .clamp(settings.min_speed, settings.max_speed);
    if settings.tick_interval == 0 {
        warn_config_normalized(&format!("{name}.tick_interval is 0; using polling_rate"));
        settings.tick_interval = polling_rate;
    }
    if settings.speed_indicator_ms == 0 {
        warn_config_normalized(&format!("{name}.speed_indicator_ms is 0; using default"));
        settings.speed_indicator_ms = DEFAULT_WHEEL_SPEED_INDICATOR_MS;
    }
    if settings.vertical_multiplier < 1 {
        warn_config_normalized(&format!(
            "{name}.vertical_multiplier is below 1; clamping to 1"
        ));
        settings.vertical_multiplier = 1;
    }
    if settings.horizontal_multiplier < 1 {
        warn_config_normalized(&format!(
            "{name}.horizontal_multiplier is below 1; clamping to 1"
        ));
        settings.horizontal_multiplier = 1;
    }
}

fn normalize_jump_stage(name: &str, stage: &mut JumpStageConfig) {
    stage.width = normalize_jump_stage_size(name, "width", stage.width);
    stage.height = normalize_jump_stage_size(name, "height", stage.height);
    stage.aim_offset_x_px =
        normalize_jump_aim_offset(name, "aim_offset_x_px", stage.aim_offset_x_px);
    stage.aim_offset_y_px =
        normalize_jump_aim_offset(name, "aim_offset_y_px", stage.aim_offset_y_px);
    if stage.target_region_mode_invalid {
        warn_config_normalized(&format!(
            "{name}.target_region_mode is invalid; using exact_region"
        ));
        stage.target_region_mode = JumpTargetRegionMode::ExactRegion;
        stage.target_region_mode_invalid = false;
    }
    if stage.legacy_preview_margin_percent.is_some() {
        if stage.legacy_preview_margin_percent_used {
            warn_config_normalized(&format!(
                "{name}.preview_margin_percent is deprecated; using it as {name}.visual_context_margin_percent"
            ));
        } else {
            warn_config_normalized(&format!(
                "{name}.preview_margin_percent is deprecated and ignored because {name}.visual_context_margin_percent is set"
            ));
        }
    }
    if stage.target_margin_percent > MAX_JUMP_REGION_MARGIN_PERCENT {
        warn_config_normalized(&format!(
            "{name}.target_margin_percent={} is above {}; clamping to {}",
            stage.target_margin_percent,
            MAX_JUMP_REGION_MARGIN_PERCENT,
            MAX_JUMP_REGION_MARGIN_PERCENT
        ));
        stage.target_margin_percent = MAX_JUMP_REGION_MARGIN_PERCENT;
    }
    if stage.visual_context_margin_percent > MAX_JUMP_REGION_MARGIN_PERCENT {
        warn_config_normalized(&format!(
            "{name}.visual_context_margin_percent={} is above {}; clamping to {}",
            stage.visual_context_margin_percent,
            MAX_JUMP_REGION_MARGIN_PERCENT,
            MAX_JUMP_REGION_MARGIN_PERCENT
        ));
        stage.visual_context_margin_percent = MAX_JUMP_REGION_MARGIN_PERCENT;
    }
    stage.zoom_scale = normalize_jump_zoom_scale(name, stage.zoom_scale);
    stage.labels.font_scale = normalize_jump_label_font_scale(name, stage.labels.font_scale);
    if stage.labels.hide_threshold_px < 0 {
        warn_config_normalized(&format!(
            "{name}.labels.hide_threshold_px is below 0; clamping to 0"
        ));
        stage.labels.hide_threshold_px = 0;
    }
}

fn normalize_jump_selection_keys(name: &str, value: &str) -> String {
    let mut normalized = String::new();

    for ch in value.chars().flat_map(char::to_uppercase) {
        if ch.is_whitespace() || normalized.contains(ch) {
            continue;
        }
        normalized.push(ch);
    }

    if normalized.chars().count() < 2 {
        warn_config_normalized(&format!(
            "{name} must contain at least 2 unique keys; using default"
        ));
        JumpHintsConfig::default().selection_keys
    } else {
        normalized
    }
}

fn normalize_jump_label_font_scale(name: &str, value: f32) -> f32 {
    if !value.is_finite() || value <= 0.0 {
        warn_config_normalized(&format!("{name}.labels.font_scale is invalid; using 1.0"));
        1.0
    } else {
        value.clamp(0.25, 4.0)
    }
}

fn normalize_jump_aim_offset(name: &str, field: &str, value: i32) -> i32 {
    value
        .clamp(-MAX_JUMP_AIM_OFFSET_PX, MAX_JUMP_AIM_OFFSET_PX)
        .tap(|clamped| {
            if *clamped != value {
                warn_config_normalized(&format!(
                    "{name}.{field}={value} is outside +/-{}; clamping",
                    MAX_JUMP_AIM_OFFSET_PX
                ));
            }
        })
}

trait Tap: Sized {
    fn tap<F: FnOnce(&Self)>(self, f: F) -> Self {
        f(&self);
        self
    }
}

impl<T> Tap for T {}

fn normalize_final_adjust_step(name: &str, value: i32, default_value: i32) -> i32 {
    if value < 1 {
        warn_config_normalized(&format!("{name} is below 1; using {default_value}"));
        default_value
    } else if value > MAX_FINAL_ADJUST_STEP_PX {
        warn_config_normalized(&format!(
            "{name} is above {}; clamping to {}",
            MAX_FINAL_ADJUST_STEP_PX, MAX_FINAL_ADJUST_STEP_PX
        ));
        MAX_FINAL_ADJUST_STEP_PX
    } else {
        value
    }
}

fn normalize_jump_stage_size(name: &str, field: &str, value: u32) -> u32 {
    if value < MIN_JUMP_STAGE_SIZE {
        warn_config_normalized(&format!(
            "{name}.{field}={value} is below {}; clamping to {}",
            MIN_JUMP_STAGE_SIZE, MIN_JUMP_STAGE_SIZE
        ));
        MIN_JUMP_STAGE_SIZE
    } else if value > MAX_JUMP_STAGE_SIZE {
        warn_config_normalized(&format!(
            "{name}.{field}={value} is above {}; clamping to {}",
            MAX_JUMP_STAGE_SIZE, MAX_JUMP_STAGE_SIZE
        ));
        MAX_JUMP_STAGE_SIZE
    } else {
        value
    }
}

fn normalize_jump_zoom_scale(name: &str, value: f32) -> f32 {
    if !value.is_finite() {
        warn_config_normalized(&format!(
            "{name}.zoom_scale is not finite; using {}",
            MIN_JUMP_ZOOM_SCALE
        ));
        MIN_JUMP_ZOOM_SCALE
    } else if value < MIN_JUMP_ZOOM_SCALE {
        warn_config_normalized(&format!(
            "{name}.zoom_scale={value} is below {}; clamping to {}",
            MIN_JUMP_ZOOM_SCALE, MIN_JUMP_ZOOM_SCALE
        ));
        MIN_JUMP_ZOOM_SCALE
    } else if value > MAX_JUMP_ZOOM_SCALE {
        warn_config_normalized(&format!(
            "{name}.zoom_scale={value} is above {}; clamping to {}",
            MAX_JUMP_ZOOM_SCALE, MAX_JUMP_ZOOM_SCALE
        ));
        MAX_JUMP_ZOOM_SCALE
    } else {
        value
    }
}

fn modifier_down(vk_code: i32) -> bool {
    unsafe { (GetAsyncKeyState(vk_code) & i16::MIN) != 0 }
}

fn decode_key_event(w_param: WPARAM, kbd: KBDLLHOOKSTRUCT) -> Option<KeyEvent> {
    let key = VirtualKey::from_vk_code(kbd.vkCode)?;
    let is_down = w_param.0 as u32 == WM_KEYDOWN || w_param.0 as u32 == WM_SYSKEYDOWN;
    let right_alt_down =
        modifier_down(VirtualKey::RightAlt.to_vk_code() as i32) || key == VirtualKey::RightAlt;
    let alt_down = right_alt_down
        || (kbd.flags & LLKHF_ALTDOWN)
            != windows::Win32::UI::WindowsAndMessaging::KBDLLHOOKSTRUCT_FLAGS(0);

    Some(KeyEvent {
        key,
        is_down,
        alt_down,
        right_alt_down,
        ctrl_down: modifier_down(0x11)
            || key == VirtualKey::Ctrl
            || key == VirtualKey::LeftCtrl
            || key == VirtualKey::RightCtrl,
        shift_down: modifier_down(0x10)
            || key == VirtualKey::Shift
            || key == VirtualKey::LeftShift
            || key == VirtualKey::RightShift,
        win_down: modifier_down(0x5B) || modifier_down(0x5C),
    })
}

fn is_keyboard_hook_key_message(code: i32, w_param: WPARAM) -> bool {
    code == HC_ACTION.try_into().unwrap()
        && (w_param.0 as u32 == WM_KEYDOWN
            || w_param.0 as u32 == WM_SYSKEYDOWN
            || w_param.0 as u32 == WM_KEYUP
            || w_param.0 as u32 == WM_SYSKEYUP)
}

#[allow(dead_code)]
fn is_keyboard_hook_routing_event(code: i32, w_param: WPARAM, flags: u32) -> bool {
    is_keyboard_hook_key_message(code, w_param) && !is_injected_keyboard_hook_flags(flags)
}

unsafe extern "system" fn keyboard_hook(code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if is_keyboard_hook_key_message(code, w_param) {
        let kbd = *(l_param.0 as *const KBDLLHOOKSTRUCT);
        if is_injected_keyboard_hook_flags(kbd.flags.0) {
            return CallNextHookEx(None, code, w_param, l_param);
        }

        HOOK_EVENTS_SEEN.fetch_add(1, Ordering::Relaxed);
        if let Some(event) = decode_key_event(w_param, kbd) {
            HOOK_EVENTS_DECODED.fetch_add(1, Ordering::Relaxed);
            let swallow = {
                let mut app_state = APP_STATE.write().unwrap();
                let swallow = app_state.should_swallow_key(&event);
                app_state.enqueue_key_event(event);
                swallow
            };

            if swallow {
                HOOK_EVENTS_SWALLOWED.fetch_add(1, Ordering::Relaxed);
                return LRESULT(1);
            }
        }
    }
    CallNextHookEx(None, code, w_param, l_param)
}

fn should_log_routing_event(event: &KeyEvent, action: Option<Action>) -> bool {
    action.is_some() || APP_STATE.read().unwrap().is_system_key_down_event(event)
}

fn routing_action_label(event: &KeyEvent, action: Option<Action>) -> String {
    if APP_STATE.read().unwrap().is_exit_key_down_event(event) {
        "Exit".to_string()
    } else if APP_STATE
        .read()
        .unwrap()
        .is_toggle_active_key_down_event(event)
    {
        "ToggleActiveMode".to_string()
    } else if let Some(action) = action {
        format!("{action:?}")
    } else {
        "None".to_string()
    }
}

fn process_queued_key_events(debug_diagnostics: bool) -> LoopDiagnostics {
    let mut diagnostics = LoopDiagnostics::default();

    loop {
        let event = {
            let mut app_state = APP_STATE.write().unwrap();
            app_state.pop_key_event()
        };

        let Some(event) = event else {
            break;
        };

        let action = KEY_ACTIONS.read().unwrap().get_action_for_event(&event);

        if debug_diagnostics && should_log_routing_event(&event, action.clone()) {
            println!(
                "[routing] key={:?} state={} action={}",
                event.key,
                if event.is_down { "down" } else { "up" },
                routing_action_label(&event, action.clone())
            );
        }

        APP_STATE.write().unwrap().route_key_event(event, action);
        diagnostics.queued_key_events_processed += 1;
    }

    loop {
        let command = APP_STATE.write().unwrap().pop_command();

        let Some(command) = command else {
            break;
        };

        execute_app_command(command, debug_diagnostics);
        diagnostics.commands_executed += 1;
    }

    diagnostics
}

fn sync_jump_overlay(resolution: JumpOverlayResolution) {
    match resolution {
        JumpOverlayResolution::Hidden => hide_jump_overlay(),
        JumpOverlayResolution::Visible(view) => update_jump_overlay(view),
    }
}

fn runtime_notification_enabled(
    notification: &RuntimeNotification,
    config: TooltipOverlayConfig,
    help_visible: bool,
) -> bool {
    if help_visible {
        return false;
    }

    if !config.enabled || !config.show_temporary_tooltips {
        return false;
    }

    match notification.kind {
        RuntimeNotificationKind::MouseSpeed => config.events.mouse,
        RuntimeNotificationKind::WheelSpeed => config.events.wheel,
        RuntimeNotificationKind::MovementProfile | RuntimeNotificationKind::WheelProfile => {
            config.events.profile
        }
        RuntimeNotificationKind::Drag => config.events.drag,
        RuntimeNotificationKind::ConfigReload => config.events.reload,
        RuntimeNotificationKind::PanicReset => config.events.panic,
    }
}

fn drain_runtime_notifications<B: MouseBackend>(action_handler: &mut ActionHandler<B>) {
    let config = action_handler.mouse_master.config.tooltip_overlay;
    let Some(notification) = action_handler.mouse_master.take_notifications().pop() else {
        return;
    };

    let help_visible = APP_STATE.read().unwrap().help_visible();
    if runtime_notification_enabled(&notification, config, help_visible) {
        let stats = help_stats_from_snapshot(
            action_handler.mouse_master.runtime_snapshot(),
            action_handler.active_keys.contains(&Action::SlowMouse),
            APP_STATE.read().unwrap().is_jump_active(),
        );
        let message = help_overlay::format_tooltip_message(&notification, &stats);
        help_overlay::show_temporary_tooltip(
            message.title,
            message.body,
            Duration::from_millis(notification.duration_ms),
        );
    }
}

fn apply_cursor_between_stages<B: MouseBackend>(
    action_handler: &mut ActionHandler<B>,
    mode: CursorBetweenStagesMode,
    region: jump_session::JumpRegion,
) {
    match mode {
        CursorBetweenStagesMode::None | CursorBetweenStagesMode::PreviewOnly => {}
        CursorBetweenStagesMode::MoveToRegionCenter | CursorBetweenStagesMode::WarpAndContinue => {
            let (x, y) = region.center();
            action_handler.mouse_master.move_mouse_to(x, y);
        }
    }
}

fn initial_grid_region(
    monitor_region: jump_session::JumpRegion,
    cursor: (i32, i32),
    width_percent: f32,
    height_percent: f32,
    center_on_cursor: bool,
) -> Option<jump_session::JumpRegion> {
    if !monitor_region.is_valid() {
        return None;
    }

    let width = ((monitor_region.width as f32 * width_percent).round() as i32)
        .clamp(1, monitor_region.width);
    let height = ((monitor_region.height as f32 * height_percent).round() as i32)
        .clamp(1, monitor_region.height);

    let (center_x, center_y) = if center_on_cursor {
        cursor
    } else {
        monitor_region.center()
    };
    let max_left = monitor_region.left + monitor_region.width - width;
    let max_top = monitor_region.top + monitor_region.height - height;

    Some(jump_session::JumpRegion {
        left: (center_x - width / 2).clamp(monitor_region.left, max_left),
        top: (center_y - height / 2).clamp(monitor_region.top, max_top),
        width,
        height,
    })
}

fn current_cursor_position() -> Option<(i32, i32)> {
    unsafe {
        let mut point = POINT::default();
        GetCursorPos(&mut point).ok()?;
        Some((point.x, point.y))
    }
}

fn set_active_mode(active: bool) {
    let resolution = {
        let mut action_handler = ACTION_HANDLER.write().unwrap();
        let mut app_state = APP_STATE.write().unwrap();
        apply_active_mode_transition(&mut action_handler, &mut app_state, active)
    };
    sync_jump_overlay(resolution);
}

fn apply_active_mode_transition<B: MouseBackend>(
    action_handler: &mut ActionHandler<B>,
    app_state: &mut AppState,
    active: bool,
) -> JumpOverlayResolution {
    if !active {
        action_handler.mouse_master.release_left_button_if_held();
        action_handler.clear_active_keys();
        app_state.clear_active_action_keys_and_exit_exclusive_modes();
        app_state.hide_help();
    }

    action_handler.mouse_master.set_active_mode(active);
    app_state.set_active_mode(active);
    app_state.resolve_jump_overlay()
}

fn clear_help_for_exclusive_mode(app_state: &mut AppState) {
    app_state.hide_help();
    help_overlay::hide_help_overlay();
}

fn execute_key_action_command<B: MouseBackend>(
    action_handler: &mut ActionHandler<B>,
    app_state: &mut AppState,
    action: Action,
    is_down: bool,
) -> Option<JumpOverlayResolution> {
    let mut keyboard_sender = WindowsKeyboardSender;
    execute_key_action_command_with_keyboard(
        action_handler,
        app_state,
        action,
        is_down,
        &mut keyboard_sender,
    )
}

fn execute_key_action_command_with_keyboard<B: MouseBackend, K: KeyboardSender>(
    action_handler: &mut ActionHandler<B>,
    app_state: &mut AppState,
    action: Action,
    is_down: bool,
    keyboard_sender: &mut K,
) -> Option<JumpOverlayResolution> {
    if action_handler.mouse_master.current_mode != ModeState::Active {
        return None;
    }

    if action.is_continuous() {
        action_handler.process_active_keys(action, is_down);
        return None;
    }

    if action == Action::ClickThenDisable {
        if is_down {
            action_handler.execute_action(&Action::ClickThenDisable);
            action_handler.clear_active_keys();
            app_state.clear_active_action_keys_and_exit_exclusive_modes();
            action_handler.mouse_master.set_active_mode(false);
            app_state.set_active_mode(false);
            clear_help_for_exclusive_mode(app_state);
            return Some(app_state.resolve_jump_overlay());
        }
        return None;
    }

    if action == Action::Disable {
        if is_down {
            return Some(apply_active_mode_transition(
                action_handler,
                app_state,
                false,
            ));
        }
        return None;
    }

    if is_down {
        if action == Action::ReloadConfig || action == Action::PanicReset {
            return None;
        }
        match send_navigation_action(keyboard_sender, &action) {
            Ok(true) => return None,
            Ok(false) => {}
            Err(error) => {
                eprintln!("[keyboard] failed to send {action:?}: {error}");
                return None;
            }
        }
        action_handler.execute_action(&action);
    }
    None
}

fn apply_loaded_config<B: MouseBackend>(
    action_handler: &mut ActionHandler<B>,
    app_state: &mut AppState,
    config: Config,
) -> Result<JumpOverlayResolution, Box<dyn Error>> {
    let active = action_handler.mouse_master.current_mode == ModeState::Active;
    action_handler.mouse_master.release_left_button_if_held();
    action_handler.clear_active_keys();
    app_state.clear_active_action_keys_and_exit_exclusive_modes();
    app_state.hide_help();
    action_handler
        .mouse_master
        .apply_config_preserving_mode(config.clone());
    action_handler.mouse_master.set_active_mode(active);
    action_handler
        .mouse_master
        .push_config_reload_notification();
    app_state.set_active_mode(active);
    Ok(app_state.resolve_jump_overlay())
}

fn reload_config() -> Result<(), Box<dyn Error>> {
    let config = Config::load_from_file("config.toml")?;
    config.runtime_system_bindings()?;
    let resolution = {
        let mut action_handler = ACTION_HANDLER.write().unwrap();
        let mut app_state = APP_STATE.write().unwrap();
        apply_loaded_config(&mut action_handler, &mut app_state, config.clone())?
    };
    config.initialize_bindings();
    config.initialize_system_bindings()?;
    sync_jump_overlay(resolution);
    println!("[reload] config reloaded");
    Ok(())
}

fn panic_reset() -> JumpOverlayResolution {
    let mut action_handler = ACTION_HANDLER.write().unwrap();
    let mut app_state = APP_STATE.write().unwrap();
    action_handler.mouse_master.hard_reset_runtime();
    action_handler.mouse_master.push_panic_reset_notification();
    action_handler.clear_active_keys();
    app_state.clear_active_action_keys_and_exit_exclusive_modes();
    app_state.hide_help();
    app_state.resolve_jump_overlay()
}

fn build_help_overlay_view() -> help_overlay::HelpOverlayView {
    let (snapshot, slow_active, help_config) = {
        let action_handler = ACTION_HANDLER.read().unwrap();
        (
            action_handler.mouse_master.runtime_snapshot(),
            action_handler.active_keys.contains(&Action::SlowMouse),
            action_handler.mouse_master.config.tooltip_overlay,
        )
    };
    let jump_active = APP_STATE.read().unwrap().is_jump_active();
    let mut view = help_overlay::help_view_from_bindings(
        KEY_ACTIONS
            .read()
            .unwrap()
            .entries()
            .map(|(chord, action)| (chord, action.clone())),
    );
    view.stats = help_stats_from_snapshot(snapshot, slow_active, jump_active);
    view.help_max_bindings = help_config.help_max_bindings;
    view.config_warnings = recent_config_warnings_for_overlay();
    view
}

fn help_stats_from_snapshot(
    snapshot: MouseRuntimeSnapshot,
    slow_active: bool,
    jump_active: bool,
) -> help_overlay::HelpRuntimeStats {
    help_overlay::HelpRuntimeStats {
        app_mode: snapshot.mode,
        drag_active: snapshot.drag_active,
        slow_active,
        jump_active,
        movement_profile: snapshot
            .movement_profile
            .unwrap_or_else(|| "default".to_string()),
        wheel_profile: snapshot
            .wheel_profile
            .unwrap_or_else(|| "default".to_string()),
        mouse_speed: help_overlay::HelpSpeedTier {
            current: snapshot.mouse_speed_current,
            default: snapshot.mouse_speed_default,
            min: snapshot.mouse_speed_min,
            max: snapshot.mouse_speed_max,
            step: snapshot.mouse_speed_step,
        },
        wheel_speed: help_overlay::HelpSpeedTier {
            current: snapshot.wheel_speed_current,
            default: snapshot.wheel_speed_default,
            min: snapshot.wheel_speed_min,
            max: snapshot.wheel_speed_max,
            step: snapshot.wheel_speed_step,
        },
        acceleration: snapshot.acceleration,
        acceleration_rate: snapshot.acceleration_rate,
        slow_strategy: snapshot.slow_strategy,
        slow_effective_speed: snapshot.slow_effective_speed,
        slow_min_speed: snapshot.slow_min_speed,
        slow_max_speed: snapshot.slow_max_speed,
        slow_acceleration: snapshot.slow_acceleration,
        slow_acceleration_rate: snapshot.slow_acceleration_rate,
        top_speed: snapshot.top_speed,
        polling_rate_ms: snapshot.polling_rate_ms,
        wheel_tick_interval_ms: snapshot.wheel_tick_interval_ms,
        wheel_vertical_multiplier: snapshot.wheel_vertical_multiplier,
        wheel_horizontal_multiplier: snapshot.wheel_horizontal_multiplier,
    }
}

fn execute_app_command(command: AppCommand, debug_diagnostics: bool) {
    if debug_diagnostics {
        match &command {
            AppCommand::ToggleActiveMode => println!("[command] ToggleActiveMode"),
            AppCommand::SetActiveMode { active } => {
                println!("[command] SetActiveMode active={active}")
            }
            AppCommand::Exit => println!("[command] Exit"),
            AppCommand::ReloadConfig => println!("[command] ReloadConfig"),
            AppCommand::PanicReset => println!("[command] PanicReset"),
            AppCommand::ToggleHelp => println!("[command] ToggleHelp"),
            AppCommand::HideHelp => println!("[command] HideHelp"),
            AppCommand::EnterJumpMode {
                activation_key,
                profile,
            } => {
                println!(
                    "[command] EnterJumpMode activation_key={activation_key:?} profile={profile:?}"
                )
            }
            AppCommand::EnterGridMode { activation_key } => {
                println!("[command] EnterGridMode activation_key={activation_key:?}")
            }
            AppCommand::KeyAction { action, is_down } => {
                println!(
                    "[command] KeyAction action={action:?} state={}",
                    if *is_down { "down" } else { "up" }
                )
            }
            AppCommand::JumpInput(event, action) => {
                println!(
                    "[command] JumpInput key={:?} state={} action={:?}",
                    event.key,
                    if event.is_down { "down" } else { "up" },
                    action
                )
            }
            AppCommand::GridInput(event, action) => {
                println!(
                    "[command] GridInput key={:?} state={} action={:?}",
                    event.key,
                    if event.is_down { "down" } else { "up" },
                    action
                )
            }
        }
    }

    match command {
        AppCommand::ToggleActiveMode => {
            let active_mode = {
                let mut action_handler = ACTION_HANDLER.write().unwrap();
                action_handler.mouse_master.toggle_mode();
                action_handler.mouse_master.current_mode == ModeState::Active
            };
            execute_app_command(
                AppCommand::SetActiveMode {
                    active: active_mode,
                },
                debug_diagnostics,
            );
        }
        AppCommand::SetActiveMode { active } => {
            set_active_mode(active);
            if !active {
                clear_help_for_exclusive_mode(&mut APP_STATE.write().unwrap());
            }
        }
        AppCommand::Exit => {
            ACTION_HANDLER.write().unwrap().mouse_master.exit();
        }
        AppCommand::ReloadConfig => {
            if let Err(err) = reload_config() {
                eprintln!("[reload] keeping existing config: {err}");
            } else {
                APP_STATE.write().unwrap().hide_help();
                help_overlay::hide_help_overlay();
            }
        }
        AppCommand::PanicReset => {
            let resolution = panic_reset();
            sync_jump_overlay(resolution);
            clear_help_for_exclusive_mode(&mut APP_STATE.write().unwrap());
        }
        AppCommand::ToggleHelp => {
            let help_config = ACTION_HANDLER
                .read()
                .unwrap()
                .mouse_master
                .config
                .tooltip_overlay;
            if !help_config.enabled || !help_config.show_help {
                APP_STATE.write().unwrap().hide_help();
                help_overlay::hide_help_overlay();
                return;
            }

            let visible = {
                let mut app_state = APP_STATE.write().unwrap();
                app_state.toggle_help();
                app_state.help_visible()
            };
            if visible {
                let view = build_help_overlay_view();
                help_overlay::show_help_overlay(view);
            } else {
                help_overlay::hide_help_overlay();
            }
        }
        AppCommand::HideHelp => {
            APP_STATE.write().unwrap().hide_help();
            help_overlay::hide_help_overlay();
        }
        AppCommand::EnterJumpMode {
            activation_key,
            profile,
        } => {
            clear_help_for_exclusive_mode(&mut APP_STATE.write().unwrap());
            let config = ACTION_HANDLER.read().unwrap().mouse_master.config.clone();
            let jump_config = match config.resolved_jump_config(profile.as_deref()) {
                Ok(jump_config) => jump_config,
                Err(err) => {
                    eprintln!("[jump] {err}; using default jump settings");
                    config.jump.clone()
                }
            };
            let fallback_region = virtual_screen_region();
            let region =
                monitor::resolve_jump_start_region(jump_config.start_region, fallback_region)
                    .unwrap_or(fallback_region);
            let view = {
                let mut app_state = APP_STATE.write().unwrap();
                app_state
                    .enter_jump_mode(
                        &jump_config,
                        config.final_adjust.clone(),
                        region,
                        activation_key,
                    )
                    .then(|| app_state.jump_view())
                    .flatten()
            };

            if let Some(view) = view {
                show_jump_overlay(view);
            }
        }
        AppCommand::EnterGridMode { activation_key } => {
            clear_help_for_exclusive_mode(&mut APP_STATE.write().unwrap());
            let config = ACTION_HANDLER.read().unwrap().mouse_master.config.clone();
            if !config.grid_mode.enabled {
                return;
            }

            let fallback_region = virtual_screen_region();
            let monitor_region =
                monitor::resolve_jump_start_region(config.grid_mode.start_region, fallback_region)
                    .unwrap_or(fallback_region);
            let cursor = current_cursor_position().unwrap_or_else(|| monitor_region.center());
            let Some(initial_region) = initial_grid_region(
                monitor_region,
                cursor,
                config.grid_mode.width_percent,
                config.grid_mode.height_percent,
                config.grid_mode.center_on_cursor,
            ) else {
                return;
            };

            let view = {
                let mut app_state = APP_STATE.write().unwrap();
                app_state
                    .enter_grid_mode(
                        monitor_region,
                        initial_region,
                        config.grid_mode.min_width_px,
                        config.grid_mode.min_height_px,
                        config.grid_mode.move_cursor_each_step,
                        config.grid_mode.line_visible,
                        config.grid_mode.show_direction_labels,
                        activation_key,
                    )
                    .then(|| app_state.grid_view())
                    .flatten()
            };

            if let Some(view) = view {
                show_jump_overlay(view);
            }
        }
        AppCommand::KeyAction { action, is_down } => {
            let resolution = {
                let mut action_handler = ACTION_HANDLER.write().unwrap();
                let mut app_state = APP_STATE.write().unwrap();
                execute_key_action_command(&mut action_handler, &mut app_state, action, is_down)
            };
            if let Some(resolution) = resolution {
                sync_jump_overlay(resolution);
            }
        }
        AppCommand::JumpInput(event, action) => {
            let (jump_result, view) = {
                let mut app_state = APP_STATE.write().unwrap();
                let jump_result = app_state.handle_jump_input(event, action);
                let view = match jump_result {
                    None
                    | Some(JumpSessionUpdate::Consumed)
                    | Some(JumpSessionUpdate::Invalid)
                    | Some(JumpSessionUpdate::AwaitingFinalAdjust { .. })
                    | Some(JumpSessionUpdate::StageAdvanced { .. })
                    | Some(JumpSessionUpdate::StageBacktracked { .. }) => app_state.jump_view(),
                    Some(JumpSessionUpdate::Cancelled | JumpSessionUpdate::Completed { .. }) => {
                        None
                    }
                };
                (jump_result, view)
            };

            match jump_result {
                None
                | Some(JumpSessionUpdate::Consumed)
                | Some(JumpSessionUpdate::Invalid)
                | Some(JumpSessionUpdate::AwaitingFinalAdjust { .. }) => {
                    if let Some(view) = view {
                        update_jump_overlay(view);
                    }
                }
                Some(JumpSessionUpdate::StageAdvanced { region, .. }) => {
                    let mut action_handler = ACTION_HANDLER.write().unwrap();
                    let mode = action_handler
                        .mouse_master
                        .config
                        .jump
                        .cursor_between_stages;
                    apply_cursor_between_stages(&mut action_handler, mode, region);
                    if let Some(view) = view {
                        update_jump_overlay(view);
                    }
                }
                Some(JumpSessionUpdate::StageBacktracked { .. }) => {
                    if let Some(view) = view {
                        update_jump_overlay(view);
                    }
                }
                Some(JumpSessionUpdate::Cancelled) => {
                    let resolution = {
                        let mut app_state = APP_STATE.write().unwrap();
                        app_state.exit_jump_mode();
                        app_state.resolve_jump_overlay()
                    };
                    sync_jump_overlay(resolution);
                }
                Some(JumpSessionUpdate::Completed { x, y, .. }) => {
                    ACTION_HANDLER
                        .write()
                        .unwrap()
                        .mouse_master
                        .move_mouse_to(x, y);
                    let resolution = {
                        let mut app_state = APP_STATE.write().unwrap();
                        app_state.exit_jump_mode();
                        app_state.resolve_jump_overlay()
                    };
                    sync_jump_overlay(resolution);
                }
            }
        }
        AppCommand::GridInput(event, action) => {
            let (grid_result, view) = {
                let mut app_state = APP_STATE.write().unwrap();
                let grid_result = app_state.handle_grid_input(event, action);
                let view = match grid_result {
                    Some(GridInputUpdate::Consumed) | Some(GridInputUpdate::Updated { .. }) => {
                        app_state.grid_view()
                    }
                    Some(GridInputUpdate::Cancelled | GridInputUpdate::Completed { .. }) | None => {
                        None
                    }
                };
                (grid_result, view)
            };

            match grid_result {
                Some(GridInputUpdate::Updated {
                    region,
                    move_cursor,
                }) => {
                    if move_cursor {
                        let (x, y) = region.center();
                        ACTION_HANDLER
                            .write()
                            .unwrap()
                            .mouse_master
                            .move_mouse_to(x, y);
                    }
                    if let Some(view) = view {
                        update_jump_overlay(view);
                    }
                }
                Some(GridInputUpdate::Consumed) => {
                    if let Some(view) = view {
                        update_jump_overlay(view);
                    }
                }
                Some(GridInputUpdate::Cancelled) => {
                    let resolution = {
                        let mut action_handler = ACTION_HANDLER.write().unwrap();
                        let mut app_state = APP_STATE.write().unwrap();
                        action_handler.clear_active_keys();
                        app_state.cancel_grid_mode();
                        app_state.resolve_jump_overlay()
                    };
                    sync_jump_overlay(resolution);
                }
                Some(GridInputUpdate::Completed { x, y, .. }) => {
                    let resolution = {
                        let mut action_handler = ACTION_HANDLER.write().unwrap();
                        let mut app_state = APP_STATE.write().unwrap();
                        action_handler.mouse_master.move_mouse_to(x, y);
                        action_handler.clear_active_keys();
                        app_state.complete_grid_mode();
                        app_state.resolve_jump_overlay()
                    };
                    sync_jump_overlay(resolution);
                }
                None => {}
            }
        }
    }
}

unsafe fn drain_windows_messages(max_messages: usize) -> u64 {
    let mut messages_processed = 0;
    let mut msg = MSG::default();

    // Bound message pumping so an endless stream of Win32 messages cannot
    // starve queued key handling, movement ticks, or overlay updates. If the
    // loop later needs true wait-based scheduling, move this to
    // MsgWaitForMultipleObjectsEx so input and timers can share one wake path.
    while messages_processed < max_messages as u64
        && PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool()
    {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
        messages_processed += 1;
    }

    messages_processed
}

fn debug_heartbeat_enabled() -> bool {
    env::var(DEBUG_HEARTBEAT_ENV)
        .map(|value| {
            matches!(
                value.as_str(),
                "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON"
            )
        })
        .unwrap_or(false)
}

fn print_heartbeat(diagnostics: LoopDiagnostics, hook_diagnostics: HookDiagnostics) {
    println!(
        "[heartbeat] hook_seen={} hook_decoded={} hook_swallowed={} queue={} commands={} ticks={} loops={} messages={}",
        hook_diagnostics.events_seen,
        hook_diagnostics.events_decoded,
        hook_diagnostics.events_swallowed,
        diagnostics.queued_key_events_processed,
        diagnostics.commands_executed,
        diagnostics.movement_ticks,
        diagnostics.loop_iterations,
        diagnostics.messages_processed
    );
}

fn startup_summary_line() -> String {
    format!("{APP_DISPLAY_NAME} ({APP_CRATE_ID}) Program Start!")
}

fn startup_validation_summary(config: &Config, warnings: &[String]) -> String {
    let mut lines = vec![
        format!("polling_rate={}ms", config.polling_rate),
        format!(
            "mouse_speed default={} range={}..{} step={} profiles={}",
            config.mouse_speed.default_speed,
            config.mouse_speed.min_speed,
            config.mouse_speed.max_speed,
            config.mouse_speed.speed_step,
            config.movement_profiles.len()
        ),
        format!(
            "wheel default={} range={}..{} step={} tick={}ms axis_multipliers=v{} h{} profiles={}",
            config.wheel.default_speed,
            config.wheel.min_speed,
            config.wheel.max_speed,
            config.wheel.speed_step,
            config.wheel.tick_interval,
            config.wheel.vertical_multiplier,
            config.wheel.horizontal_multiplier,
            config.wheel_profiles.len()
        ),
        format!(
            "slow_mouse strategy={:?} speed={} (default {}, range {}..{}) acceleration={} every {} tick(s)",
            config.slow_mouse.strategy,
            crate::action_handler::effective_slow_speed(config, config.mouse_speed.default_speed),
            config.slow_mouse.fixed_speed,
            config.slow_mouse.min_speed,
            config.slow_mouse.max_speed,
            config.slow_mouse.acceleration,
            config.slow_mouse.acceleration_rate,
        ),
        format!(
            "jump mode={:?} start_region={:?} profiles={}",
            config.jump.mode,
            config.jump.start_region,
            config.jump.profiles.len()
        ),
    ];

    if warnings.is_empty() {
        lines.push("warnings=0".to_string());
    } else {
        lines.push(format!("warnings={}", warnings.len()));
        lines.extend(warnings.iter().map(|warning| format!("warning: {warning}")));
    }

    lines.join("\n")
}

fn print_startup_validation_summary(config: &Config) {
    println!(
        "[startup validation]\n{}",
        startup_validation_summary(config, &take_config_warnings())
    );
}

unsafe fn install_keyboard_hook() -> windows::core::Result<()> {
    println!("🔹 Attempting to Get Module Handle...");
    let h_instance = GetModuleHandleW(None)?;
    println!("✅ Module Handle Retrieved");

    println!("🔹 Setting Up Keyboard Hook...");
    let hook = SetWindowsHookExW(
        WH_KEYBOARD_LL,
        Some(keyboard_hook),
        Some(h_instance.into()),
        0,
    )?;

    // Store the hook guard for cleanup on panic
    KEYBOARD_HOOK_HANDLE.with(|slot| *slot.borrow_mut() = Some(KeyboardHook(hook)));

    Ok(())
}

fn main() {
    println!("🚀 {}", startup_summary_line());

    // Set a panic hook to ensure we clean up resources on unexpected errors
    std::panic::set_hook(Box::new(|info| {
        eprintln!("Application panicked: {}", info);
        if let Ok(mut action_handler) = ACTION_HANDLER.write() {
            action_handler.mouse_master.hard_reset_runtime();
            action_handler.clear_active_keys();
        }
        if let Ok(mut app_state) = APP_STATE.write() {
            app_state.clear_active_action_keys_and_exit_exclusive_modes();
            app_state.hide_help();
        }
        // Drop the hook guard so the keyboard is unhooked
        KEYBOARD_HOOK_HANDLE.with(|slot| {
            slot.borrow_mut().take();
        });
        hide_jump_overlay();
        std::process::exit(1);
    }));

    // Ensure Rust backtrace is enabled
    env::set_var("RUST_BACKTRACE", "1");
    println!("🔹 Backtrace Enabled");

    let config = match Config::load_from_file("config.toml") {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Error loading configuration: {}", e);
            std::process::exit(1);
        }
    };
    println!("✅ Config Loaded");
    print_startup_validation_summary(&config);

    config.initialize_bindings();
    println!("✅ Key Bindings Initialized");
    if let Err(e) = config.initialize_system_bindings() {
        eprintln!("❌ System Binding Initialization Failed: {e}");
        std::process::exit(1);
    }

    if let Err(e) = unsafe { install_keyboard_hook() } {
        eprintln!("❌ Keyboard Hook Failed to Install: {e}");
        return;
    }
    println!("✅ Keyboard Hook Installed Successfully!");

    println!("🔹 Attempting Overlay Initialization...");
    let overlay = match OVERLAY.lock() {
        Ok(maybe_ov) => maybe_ov.as_ref().cloned(),
        Err(e) => {
            eprintln!("❌ Overlay Lock Failed: {e}");
            None
        }
    };
    if let Some(ov) = overlay {
        println!("✅ Overlay Initialized Successfully");
        drop(ov);
    } else {
        eprintln!("Overlay disabled due to initialization failure");
    }

    println!("🔄 Entering Main Event Loop...");
    let debug_diagnostics = debug_heartbeat_enabled();
    let mut heartbeat = HeartbeatDiagnostics::default();
    let mut last_heartbeat_sample = Instant::now();

    loop {
        let mut loop_diagnostics = LoopDiagnostics {
            loop_iterations: 1,
            messages_processed: unsafe { drain_windows_messages(MAX_MESSAGES_PER_TICK) },
            ..LoopDiagnostics::default()
        };

        loop_diagnostics.add(process_queued_key_events(debug_diagnostics));
        {
            let mut action_handler = ACTION_HANDLER.write().unwrap();
            drain_runtime_notifications(&mut action_handler);
        }

        let movement_tick = ACTION_HANDLER.write().unwrap().tick_movement();
        if movement_tick.moving {
            loop_diagnostics.movement_ticks += 1;
        }

        let (indicator_snapshot, status_overlay_config) = {
            let action_handler = ACTION_HANDLER.read().unwrap();
            let app_state = APP_STATE.read().unwrap();
            let wheel_direction_active = action_handler
                .active_keys
                .iter()
                .any(|action| action.is_wheel_direction());
            (
                resolve_indicator_snapshot(IndicatorInput {
                    app_active: app_state.active_mode(),
                    jump_active: app_state.is_jump_active(),
                    jump_stage: app_state.jump_stage_status(),
                    final_adjust_active: app_state.final_adjust_active(),
                    active_actions: &action_handler.active_keys,
                    mouse: MouseIndicatorInput {
                        active: action_handler.mouse_master.mouse_speed_indicator_active(),
                        current_speed: action_handler.mouse_master.mouse_speed_baseline,
                        default_speed: action_handler.mouse_master.config.mouse_speed.default_speed,
                    },
                    wheel: WheelIndicatorInput {
                        active: wheel_direction_active
                            || action_handler.mouse_master.wheel_speed_indicator_active(),
                        current_speed: action_handler.mouse_master.current_wheel_speed,
                        default_speed: action_handler.mouse_master.config.wheel.default_speed,
                    },
                    left_button_held: action_handler.mouse_master.left_button_held(),
                }),
                action_handler.mouse_master.config.status_overlay,
            )
        };
        if let Ok(mut maybe_ov) = OVERLAY.lock() {
            if let Some(ref mut ov) = *maybe_ov {
                ov.update_overlay_snapshot(indicator_snapshot, status_overlay_config);
            }
        }
        help_overlay::update_overlay(Instant::now());

        if debug_diagnostics {
            let now = Instant::now();
            if let Some(snapshot) = heartbeat.record(loop_diagnostics, now - last_heartbeat_sample)
            {
                print_heartbeat(snapshot, hook_diagnostics_snapshot());
            }
            last_heartbeat_sample = now;
        }

        sleep(Duration::from_millis(config.polling_rate));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use enigo::{Axis, Button};

    const LEGACY_PACKAGE_NAME: &str = concat!("Learn", "ing", "_", "Ru", "st");

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

    #[derive(Default)]
    struct MockKeyboardSender {
        events: Vec<SyntheticKeyEvent>,
    }

    impl KeyboardSender for MockKeyboardSender {
        fn send_key_event(&mut self, event: SyntheticKeyEvent) -> Result<(), String> {
            self.events.push(event);
            Ok(())
        }
    }

    fn parse_config(toml: &str) -> Config {
        toml::from_str::<Config>(toml).unwrap().normalize().unwrap()
    }

    fn parse_config_error(toml: &str) -> String {
        match toml::from_str::<Config>(toml) {
            Ok(config) => config
                .normalize()
                .expect_err("config should fail normalization")
                .to_string(),
            Err(err) => err.to_string(),
        }
    }

    fn checked_in_config() -> Config {
        parse_config(include_str!("../config.toml"))
    }

    fn readme_section(heading: &str, next_heading: &str) -> &'static str {
        let readme = include_str!("../README.md");
        let start = readme
            .find(heading)
            .unwrap_or_else(|| panic!("missing README heading {heading}"));
        let after_start = &readme[start..];
        let end = after_start
            .find(next_heading)
            .unwrap_or_else(|| panic!("missing README heading {next_heading}"));
        &after_start[..end]
    }

    fn readme_toml_block_after(marker: &str) -> &'static str {
        let readme = include_str!("../README.md");
        let marker_start = readme
            .find(marker)
            .unwrap_or_else(|| panic!("missing README marker {marker}"));
        let after_marker = &readme[marker_start..];
        let block_start = after_marker
            .find("```toml")
            .unwrap_or_else(|| panic!("missing TOML block after {marker}"))
            + "```toml".len();
        let after_fence = &after_marker[block_start..];
        let block_end = after_fence
            .find("```")
            .unwrap_or_else(|| panic!("unterminated TOML block after {marker}"));
        after_fence[..block_end].trim()
    }

    fn jump_region() -> crate::jump_session::JumpRegion {
        crate::jump_session::JumpRegion {
            left: 0,
            top: 0,
            width: 100,
            height: 100,
        }
    }

    #[test]
    fn initial_grid_region_centers_on_cursor_and_clamps_inside_monitor() {
        let monitor = crate::jump_session::JumpRegion {
            left: 100,
            top: 200,
            width: 400,
            height: 300,
        };

        assert_eq!(
            initial_grid_region(monitor, (120, 220), 0.5, 0.5, true),
            Some(crate::jump_session::JumpRegion {
                left: 100,
                top: 200,
                width: 200,
                height: 150,
            })
        );
        assert_eq!(
            initial_grid_region(monitor, (490, 490), 0.5, 0.5, true),
            Some(crate::jump_session::JumpRegion {
                left: 300,
                top: 350,
                width: 200,
                height: 150,
            })
        );
    }

    #[test]
    fn initial_grid_region_can_center_on_monitor() {
        let monitor = crate::jump_session::JumpRegion {
            left: 100,
            top: 200,
            width: 400,
            height: 300,
        };

        assert_eq!(
            initial_grid_region(monitor, (120, 220), 0.5, 0.5, false),
            Some(crate::jump_session::JumpRegion {
                left: 200,
                top: 275,
                width: 200,
                height: 150,
            })
        );
    }

    #[test]
    fn zero_polling_rate_normalizes_to_default() {
        let config = parse_config("polling_rate = 0");

        assert_eq!(config.polling_rate, DEFAULT_POLLING_RATE_MS);
    }

    #[test]
    fn positive_polling_rate_is_preserved() {
        let config = parse_config("polling_rate = 16");

        assert_eq!(config.polling_rate, 16);
    }

    #[test]
    fn missing_values_fall_back_to_defaults() {
        let config = parse_config("");
        let defaults = Config::default();

        assert_eq!(config.polling_rate, defaults.polling_rate);
        assert_eq!(config.grid_size.width, defaults.grid_size.width);
        assert_eq!(config.grid_size.height, defaults.grid_size.height);
        assert_eq!(config.grid_mode, defaults.grid_mode);
        assert_eq!(config.jump.mode, defaults.jump.mode);
        assert_eq!(
            config.jump.cursor_between_stages,
            defaults.jump.cursor_between_stages
        );
        assert_eq!(config.jump.visuals, defaults.jump.visuals);
        assert_eq!(config.jump.coarse.width, defaults.grid_size.width);
        assert_eq!(config.jump.coarse.height, defaults.grid_size.height);
        assert_eq!(config.mouse_speed, defaults.mouse_speed);
        assert_eq!(config.wheel, defaults.wheel);
        assert_eq!(config.edge_jump, defaults.edge_jump);
        assert_eq!(config.starting_speed, defaults.starting_speed);
        assert_eq!(config.acceleration, defaults.acceleration);
        assert_eq!(config.acceleration_rate, defaults.acceleration_rate);
        assert_eq!(config.top_speed, defaults.top_speed);
        assert_eq!(
            config.system_bindings.toggle_active,
            defaults.system_bindings.toggle_active
        );
        assert_eq!(config.system_bindings.exit, defaults.system_bindings.exit);
        assert_eq!(config.key_bindings, defaults.key_bindings);
    }

    #[test]
    fn default_jump_config_uses_current_monitor_two_stage_grid_without_zoom() {
        let config = parse_config("");

        assert_eq!(config.jump.mode, JumpMode::Precision);
        assert_eq!(config.jump.start_region, JumpStartRegion::CurrentMonitor);
        assert!(config.jump.coarse.enabled);
        assert!(config.jump.fine.enabled);
        assert!(!config.jump.precise.enabled);
        assert_eq!(
            config.jump.coarse.target_region_mode,
            JumpTargetRegionMode::ExactRegion
        );
        assert_eq!(
            config.jump.fine.target_region_mode,
            JumpTargetRegionMode::ExactRegion
        );
        assert_eq!(config.jump.coarse.zoom_scale, 1.0);
        assert_eq!(config.jump.fine.zoom_scale, 1.0);
        assert_eq!(config.jump.fine.visual_context_margin_percent, 0);
    }

    #[test]
    fn tooltip_overlay_defaults_when_section_is_missing() {
        let config = parse_config("");

        assert_eq!(config.tooltip_overlay, TooltipOverlayConfig::default());
    }

    #[test]
    fn tooltip_overlay_defaults_missing_fields() {
        let config = parse_config(
            r#"
            [tooltip_overlay]
            enabled = false
            "#,
        );
        let defaults = TooltipOverlayConfig::default();

        assert!(!config.tooltip_overlay.enabled);
        assert_eq!(
            config.tooltip_overlay.show_temporary_tooltips,
            defaults.show_temporary_tooltips
        );
        assert_eq!(config.tooltip_overlay.show_help, defaults.show_help);
        assert_eq!(config.tooltip_overlay.positioning, defaults.positioning);
        assert_eq!(
            config.tooltip_overlay.help_positioning,
            defaults.help_positioning
        );
        assert_eq!(config.tooltip_overlay.events, defaults.events);
    }

    #[test]
    fn tooltip_overlay_explicit_disable_parses() {
        let config = parse_config(
            r#"
            [tooltip_overlay]
            enabled = false
            show_temporary_tooltips = false
            show_help = false

            [tooltip_overlay.events]
            mouse = false
            wheel = false
            profile = false
            drag = false
            reload = false
            panic = false
            "#,
        );

        assert!(!config.tooltip_overlay.enabled);
        assert!(!config.tooltip_overlay.show_temporary_tooltips);
        assert!(!config.tooltip_overlay.show_help);
        assert_eq!(
            config.tooltip_overlay.events,
            TooltipOverlayEvents {
                mouse: false,
                wheel: false,
                profile: false,
                drag: false,
                reload: false,
                panic: false,
            }
        );
    }

    #[test]
    fn tooltip_overlay_zero_and_invalid_edge_values_normalize() {
        let config = parse_config(
            r#"
            [tooltip_overlay]
            duration_ms = 0
            help_width = 0
            help_max_bindings = -1
            "#,
        );

        assert_eq!(
            config.tooltip_overlay.duration_ms,
            DEFAULT_TOOLTIP_DURATION_MS
        );
        assert_eq!(config.tooltip_overlay.help_width, MIN_TOOLTIP_HELP_WIDTH);
        assert_eq!(
            config.tooltip_overlay.help_max_bindings,
            MIN_TOOLTIP_HELP_BINDINGS
        );
    }

    #[test]
    fn tooltip_overlay_clamps_width_max_bindings_and_offsets() {
        let low = parse_config(
            r#"
            [tooltip_overlay]
            help_width = 120
            help_max_bindings = -5
            offset_x = -250
            offset_y = -201
            "#,
        );
        let high = parse_config(
            r#"
            [tooltip_overlay]
            help_width = 900
            help_max_bindings = 250
            offset_x = 201
            offset_y = 250
            "#,
        );

        assert_eq!(low.tooltip_overlay.help_width, MIN_TOOLTIP_HELP_WIDTH);
        assert_eq!(
            low.tooltip_overlay.help_max_bindings,
            MIN_TOOLTIP_HELP_BINDINGS
        );
        assert_eq!(low.tooltip_overlay.offset_x, MIN_TOOLTIP_OFFSET);
        assert_eq!(low.tooltip_overlay.offset_y, MIN_TOOLTIP_OFFSET);
        assert_eq!(high.tooltip_overlay.help_width, MAX_TOOLTIP_HELP_WIDTH);
        assert_eq!(
            high.tooltip_overlay.help_max_bindings,
            MAX_TOOLTIP_HELP_BINDINGS
        );
        assert_eq!(high.tooltip_overlay.offset_x, MAX_TOOLTIP_OFFSET);
        assert_eq!(high.tooltip_overlay.offset_y, MAX_TOOLTIP_OFFSET);
    }

    #[test]
    fn jump_visual_flags_parse_from_config() {
        let config = parse_config(
            r#"
            [jump.visuals]
            selected_region_outline = false
            preview_outline = true
            active_grid_outline = false
            cell_centers = true
            final_crosshair = false
            "#,
        );

        assert_eq!(
            config.jump.visuals,
            JumpVisualsConfig {
                selected_region_outline: false,
                preview_outline: true,
                active_grid_outline: false,
                cell_centers: true,
                final_crosshair: false,
            }
        );
    }

    #[test]
    fn default_config_bindings_and_grid_mode_match_recommended_values() {
        let config = parse_config("");
        let expected_bindings = default_key_bindings();

        assert_eq!(config.key_bindings, expected_bindings);
        assert_eq!(
            config
                .key_bindings
                .iter()
                .filter(|(_, action)| action == "slow_mouse")
                .count(),
            1
        );
        assert!(config
            .key_bindings
            .iter()
            .any(|(key, action)| key == "LeftShift" && action == "slow_mouse"));
        assert!(config
            .key_bindings
            .iter()
            .any(|(key, action)| key == "RightShift" && action == "middle_click"));
        for (key, action) in &config.key_bindings {
            assert!(KeyChord::parse(key).is_ok(), "{key}");
            assert!(Action::from_string(action).is_some(), "{key} -> {action}");
        }

        assert_eq!(config.grid_mode, GridModeConfig::default());
    }

    #[test]
    fn grid_mode_out_of_range_values_clamp() {
        let low = parse_config(
            r#"
            [grid_mode]
            width_percent = 0.01
            height_percent = 0.0
            min_width_px = 0
            min_height_px = -5
            "#,
        );
        let high = parse_config(
            r#"
            [grid_mode]
            width_percent = 1.5
            height_percent = 2.0
            min_width_px = 501
            min_height_px = 900
            "#,
        );

        assert_eq!(low.grid_mode.width_percent, MIN_GRID_MODE_REGION_PERCENT);
        assert_eq!(low.grid_mode.height_percent, MIN_GRID_MODE_REGION_PERCENT);
        assert_eq!(low.grid_mode.min_width_px, MIN_GRID_MODE_SIZE_PX);
        assert_eq!(low.grid_mode.min_height_px, MIN_GRID_MODE_SIZE_PX);
        assert_eq!(high.grid_mode.width_percent, MAX_GRID_MODE_REGION_PERCENT);
        assert_eq!(high.grid_mode.height_percent, MAX_GRID_MODE_REGION_PERCENT);
        assert_eq!(high.grid_mode.min_width_px, MAX_GRID_MODE_SIZE_PX);
        assert_eq!(high.grid_mode.min_height_px, MAX_GRID_MODE_SIZE_PX);
    }

    #[test]
    fn default_bindings_are_known_and_defaults_are_sane() {
        let config = Config::default().normalize().unwrap();

        assert_eq!(config.system_bindings.exit, "Escape");
        assert_eq!(config.key_bindings, default_key_bindings());
        for (key, action) in &config.key_bindings {
            assert!(KeyChord::parse(key).is_ok(), "{key}");
            assert!(Action::from_string(action).is_some(), "{key} -> {action}");
        }
        assert!(config.grid_mode.enabled);
        assert_eq!(
            config.grid_mode.start_region,
            JumpStartRegion::CurrentMonitor
        );
        assert_eq!(
            config.grid_mode.width_percent,
            GridModeConfig::default().width_percent
        );
        assert_eq!(
            config.grid_mode.height_percent,
            GridModeConfig::default().height_percent
        );
    }

    #[test]
    fn default_config_effective_values_match_expected() {
        let config = Config::default().normalize().unwrap();

        assert_eq!(config.polling_rate, 8);
        assert_eq!(config.acceleration, 2);
        assert_eq!(config.acceleration_rate, 1);
        assert_eq!(config.top_speed, 6);
        assert_eq!(
            config.grid_mode.width_percent,
            GridModeConfig::default().width_percent
        );
        assert_eq!(config.jump.start_region, JumpStartRegion::CurrentMonitor);
        assert!(!config.jump.precise.enabled);
    }

    #[test]
    fn readme_default_binding_keys_match_documented_defaults() {
        let config = Config::default().normalize().unwrap();
        let section = readme_section("## Default Bindings", "## Active And Idle");

        for key in [
            &config.system_bindings.toggle_active,
            &config.system_bindings.exit,
        ] {
            let token = format!("`{key}`");
            assert!(section.contains(&token), "README missing {token}");
        }

        for (key, _) in &config.key_bindings {
            let token = format!("`{key}`");
            assert!(section.contains(&token), "README missing {token}");
        }

        assert!(
            !section.contains("`Alt+E`") && !section.contains("Alt + E"),
            "README should not document stale Alt+E active toggle"
        );
    }

    #[test]
    fn readme_copy_paste_jump_snippet_parses() {
        let snippet = readme_toml_block_after("Copy-pasteable jump configuration:");
        let config = parse_config(snippet);

        assert_eq!(config.jump.mode, JumpMode::Precision);
        assert_eq!(config.jump.coarse.width, 10);
        assert_eq!(config.jump.fine.width, 8);
        assert_eq!(config.jump.precise.width, 5);
        assert_eq!(config.jump.precise.visual_context_margin_percent, 5);
    }

    #[test]
    fn new_default_action_bindings_parse_from_config() {
        let config = parse_config(
            r#"
            key_bindings = [
                ["A", "move_left"],
                ["D", "move_right"],
                ["N", "toggle_drag_mode"],
                [";", "middle_click"],
                [",", "wheel_up"],
                ["M", "wheel_down"],
                ["I", "wheel_left"],
                ["O", "wheel_right"],
                ["H", "center_current_monitor"],
                [".", "click_then_disable"],
                ["RightAlt+W", "move_to_top_edge"],
                ["RightAlt+A", "move_to_left_edge"],
                ["RightAlt+S", "move_to_bottom_edge"],
                ["RightAlt+D", "move_to_right_edge"],
                ["V", "wheel_speed_up"],
                ["B", "wheel_speed_down"],
                ["C", "mouse_speed_up"],
                ["X", "mouse_speed_down"],
                ["Z", "mouse_speed_reset"]
            ]
            "#,
        );

        let expected = [
            ("A", Action::MoveLeft),
            ("D", Action::MoveRight),
            ("N", Action::ToggleDragMode),
            (";", Action::MiddleClick),
            (",", Action::WheelUp),
            ("M", Action::WheelDown),
            ("I", Action::WheelLeft),
            ("O", Action::WheelRight),
            ("H", Action::CenterCurrentMonitor),
            (".", Action::ClickThenDisable),
            ("RightAlt+W", Action::MoveToTopEdge),
            ("RightAlt+A", Action::MoveToLeftEdge),
            ("RightAlt+S", Action::MoveToBottomEdge),
            ("RightAlt+D", Action::MoveToRightEdge),
            ("V", Action::WheelSpeedUp),
            ("B", Action::WheelSpeedDown),
            ("C", Action::MouseSpeedUp),
            ("X", Action::MouseSpeedDown),
            ("Z", Action::MouseSpeedReset),
        ];

        assert_eq!(config.key_bindings.len(), expected.len());
        for (key, action) in expected {
            let parsed = config
                .key_bindings
                .iter()
                .find(|(bound_key, _)| bound_key == key)
                .unwrap_or_else(|| panic!("missing binding for {key}"));
            assert!(KeyChord::parse(&parsed.0).is_ok(), "{key}");
            assert_eq!(Action::from_string(&parsed.1), Some(action));
        }
    }

    #[test]
    fn exit_binding_is_owned_by_system_bindings() {
        let config = parse_config(
            r#"
            key_bindings = [
                ["A", "move_left"],
                ["D", "move_right"]
            ]

            [system_bindings]
            toggle_active = "Ctrl+E"
            exit = "Escape"
            "#,
        );

        assert_eq!(config.system_bindings.exit, "Escape");
        assert!(!config
            .key_bindings
            .iter()
            .any(|(_, action)| action == "exit"));
    }

    #[test]
    fn remapped_grid_movement_routes_by_action_with_wasd_unbound() {
        let config = parse_config(
            r#"
            key_bindings = [
                ["F", "move_right"],
                ["J", "move_left"],
                ["I", "move_up"],
                ["K", "move_down"]
            ]
            "#,
        );

        let mut bindings = KeyBindings::new();
        for (key, action) in &config.key_bindings {
            let chord = KeyChord::parse(key).unwrap();
            let action = Action::from_string(action).unwrap();
            bindings.add_chord_binding(chord, action);
        }

        let mut app_state = AppState::default();
        assert!(app_state.enter_grid_mode(
            JumpRegion {
                left: 0,
                top: 0,
                width: 100,
                height: 100,
            },
            JumpRegion {
                left: 0,
                top: 0,
                width: 100,
                height: 100,
            },
            25,
            25,
            true,
            true,
            true,
            VirtualKey::G,
        ));

        for (key, expected_action) in [
            (VirtualKey::F, Action::MoveRight),
            (VirtualKey::J, Action::MoveLeft),
            (VirtualKey::I, Action::MoveUp),
            (VirtualKey::K, Action::MoveDown),
        ] {
            let event = KeyEvent::new(key, true);
            let resolved = bindings.get_action_for_event(&event);
            app_state.route_key_event(event, resolved);
            assert_eq!(
                app_state.pop_command(),
                Some(AppCommand::GridInput(event, Some(expected_action)))
            );
        }

        assert_eq!(
            bindings.get_action_for_event(&KeyEvent::new(VirtualKey::W, true)),
            None
        );
        assert_eq!(
            app_state.handle_grid_input(KeyEvent::new(VirtualKey::W, true), None),
            Some(GridInputUpdate::Consumed)
        );

        assert_eq!(
            app_state.handle_grid_input(KeyEvent::new(VirtualKey::Backspace, true), None),
            Some(GridInputUpdate::Updated {
                region: JumpRegion {
                    left: 0,
                    top: 0,
                    width: 100,
                    height: 100,
                },
                move_cursor: true,
            })
        );
        assert_eq!(
            app_state.handle_grid_input(KeyEvent::new(VirtualKey::Escape, true), None),
            Some(GridInputUpdate::Cancelled)
        );
    }

    #[test]
    fn invalid_system_binding_fails_normalization() {
        let config = toml::from_str::<Config>(
            r#"
            [system_bindings]
            toggle_active = "Ctrl+Nope"
            "#,
        )
        .unwrap();

        let err = config.normalize().unwrap_err().to_string();
        assert!(err.contains("system_bindings.toggle_active"));
        assert!(err.contains("invalid key token"));
    }

    #[test]
    fn missing_jump_coarse_uses_legacy_grid_size() {
        let config = parse_config(
            r#"
            grid_size = { width = 12, height = 8 }

            [jump]
            mode = "precision"
            "#,
        );

        assert_eq!(config.jump.coarse.width, 12);
        assert_eq!(config.jump.coarse.height, 8);
        assert_eq!(config.grid_size.width, 12);
        assert_eq!(config.grid_size.height, 8);
    }

    #[test]
    fn jump_coarse_overrides_legacy_grid_size_after_normalization() {
        let config = parse_config(
            r#"
            grid_size = { width = 12, height = 8 }

            [jump.coarse]
            width = 9
            height = 7
            "#,
        );

        assert_eq!(config.jump.coarse.width, 9);
        assert_eq!(config.jump.coarse.height, 7);
        assert_eq!(config.grid_size.width, 9);
        assert_eq!(config.grid_size.height, 7);
    }

    #[test]
    fn single_jump_mode_disables_later_stages() {
        let config = parse_config(
            r#"
            [jump]
            mode = "single"

            [jump.coarse]
            width = 10
            height = 10

            [jump.fine]
            enabled = true

            [jump.precise]
            enabled = true
            "#,
        );

        assert!(!config.jump.fine.enabled);
        assert!(!config.jump.precise.enabled);
    }

    #[test]
    fn jump_cursor_between_stages_is_configurable() {
        let cases = [
            ("none", CursorBetweenStagesMode::None),
            (
                "move_to_region_center",
                CursorBetweenStagesMode::MoveToRegionCenter,
            ),
            ("preview_only", CursorBetweenStagesMode::PreviewOnly),
            (
                "warp_and_continue",
                CursorBetweenStagesMode::WarpAndContinue,
            ),
        ];

        for (value, expected) in cases {
            let config = parse_config(&format!(
                r#"
                [jump]
                cursor_between_stages = "{value}"
                "#
            ));

            assert_eq!(config.jump.cursor_between_stages, expected);
        }
    }

    #[test]
    fn legacy_jump_move_cursor_after_each_stage_maps_to_cursor_mode() {
        let enabled = parse_config(
            r#"
            [jump]
            move_cursor_after_each_stage = true
            "#,
        );
        let disabled = parse_config(
            r#"
            [jump]
            move_cursor_after_each_stage = false
            "#,
        );

        assert_eq!(
            enabled.jump.cursor_between_stages,
            CursorBetweenStagesMode::MoveToRegionCenter
        );
        assert_eq!(
            disabled.jump.cursor_between_stages,
            CursorBetweenStagesMode::None
        );
    }

    #[test]
    fn cursor_between_stages_takes_precedence_over_legacy_bool() {
        let config = parse_config(
            r#"
            [jump]
            cursor_between_stages = "preview_only"
            move_cursor_after_each_stage = true
            "#,
        );

        assert_eq!(
            config.jump.cursor_between_stages,
            CursorBetweenStagesMode::PreviewOnly
        );
    }

    #[test]
    fn cursor_between_stage_mode_dispatch_controls_warping() {
        let region = crate::jump_session::JumpRegion {
            left: 20,
            top: 40,
            width: 10,
            height: 20,
        };

        for mode in [
            CursorBetweenStagesMode::None,
            CursorBetweenStagesMode::PreviewOnly,
        ] {
            let mouse_master =
                MouseMaster::new_with_backend(Config::default(), FakeBackend::default());
            let mut action_handler = ActionHandler::new(mouse_master);

            apply_cursor_between_stages(&mut action_handler, mode, region);

            assert!(
                action_handler.mouse_master.backend.moves.is_empty(),
                "{mode:?} must not warp the cursor"
            );
        }

        for mode in [
            CursorBetweenStagesMode::MoveToRegionCenter,
            CursorBetweenStagesMode::WarpAndContinue,
        ] {
            let mouse_master =
                MouseMaster::new_with_backend(Config::default(), FakeBackend::default());
            let mut action_handler = ActionHandler::new(mouse_master);

            apply_cursor_between_stages(&mut action_handler, mode, region);

            assert_eq!(action_handler.mouse_master.backend.moves, vec![(25, 50)]);
        }
    }

    #[test]
    fn jump_stage_zoom_defaults_keep_enabled_stages_unzoomed() {
        let config = Config::default().normalize().unwrap();

        assert_eq!(
            config.jump.coarse.target_region_mode,
            JumpTargetRegionMode::ExactRegion
        );
        assert_eq!(config.jump.coarse.zoom_scale, 1.0);
        assert_eq!(
            config.jump.fine.target_region_mode,
            JumpTargetRegionMode::ExactRegion
        );
        assert_eq!(config.jump.fine.zoom_scale, 1.0);
        assert_eq!(
            config.jump.precise.target_region_mode,
            JumpTargetRegionMode::ExactRegion
        );
        assert_eq!(config.jump.precise.zoom_scale, 2.5);
    }

    #[test]
    fn jump_stage_target_region_mode_and_zoom_parse_per_stage() {
        let config = parse_config(
            r#"
            [jump]
            mode = "precision"

            [jump.coarse]
            target_region_mode = "region_with_context"
            zoom_scale = 1.25

            [jump.fine]
            enabled = true
            target_region_mode = "expanded_target"
            zoom_scale = 2.0

            [jump.precise]
            enabled = true
            target_region_mode = "cursor_centered_zoom"
            zoom_scale = 3.5
            "#,
        );

        assert_eq!(
            config.jump.coarse.target_region_mode,
            JumpTargetRegionMode::RegionWithContext
        );
        assert_eq!(config.jump.coarse.zoom_scale, 1.25);
        assert_eq!(
            config.jump.fine.target_region_mode,
            JumpTargetRegionMode::ExpandedTarget
        );
        assert_eq!(config.jump.fine.zoom_scale, 2.0);
        assert_eq!(
            config.jump.precise.target_region_mode,
            JumpTargetRegionMode::CursorCenteredZoom
        );
        assert_eq!(config.jump.precise.zoom_scale, 3.5);
    }

    #[test]
    fn jump_target_region_modes_parse_from_config() {
        let cases = [
            ("exact_region", JumpTargetRegionMode::ExactRegion),
            (
                "region_with_context",
                JumpTargetRegionMode::RegionWithContext,
            ),
            ("expanded_target", JumpTargetRegionMode::ExpandedTarget),
            (
                "cursor_centered_zoom",
                JumpTargetRegionMode::CursorCenteredZoom,
            ),
        ];

        for (value, expected) in cases {
            let config = parse_config(&format!(
                r#"
                [jump.coarse]
                target_region_mode = "{value}"
                "#
            ));

            assert_eq!(config.jump.coarse.target_region_mode, expected);
        }
    }

    #[test]
    fn jump_start_region_and_preview_edge_modes_parse_from_config() {
        let start_regions = [
            ("virtual_screen", JumpStartRegion::VirtualScreen),
            ("current_monitor", JumpStartRegion::CurrentMonitor),
            (
                "active_window_monitor",
                JumpStartRegion::ActiveWindowMonitor,
            ),
            ("active_window_bounds", JumpStartRegion::ActiveWindowBounds),
        ];
        for (value, expected) in start_regions {
            let config = parse_config(&format!(
                r#"
                [jump]
                start_region = "{value}"
                "#
            ));
            assert_eq!(config.jump.start_region, expected);
        }

        let edge_behaviors = [
            ("clamp", PreviewEdgeBehavior::Clamp),
            ("shift_into_bounds", PreviewEdgeBehavior::ShiftIntoBounds),
            (
                "allow_asymmetric_context",
                PreviewEdgeBehavior::AllowAsymmetricContext,
            ),
            (
                "disable_context_near_edges",
                PreviewEdgeBehavior::DisableContextNearEdges,
            ),
        ];
        for (value, expected) in edge_behaviors {
            let config = parse_config(&format!(
                r#"
                [jump]
                preview_edge_behavior = "{value}"

                [jump.coarse]
                preview_edge_behavior = "{value}"
                "#
            ));
            assert_eq!(config.jump.preview_edge_behavior, expected);
            assert_eq!(config.jump.coarse.preview_edge_behavior, Some(expected));
        }
    }

    #[test]
    fn jump_profiles_resolve_overrides_and_report_missing_profiles() {
        let config = parse_config(
            r#"
            [jump]
            start_region = "virtual_screen"
            preview_edge_behavior = "clamp"

            [jump.hints]
            selection_keys = "xy"

            [jump.profiles.window]
            start_region = "active_window_bounds"
            preview_edge_behavior = "shift_into_bounds"

            [jump.profiles.window.coarse]
            width = 7
            height = 6
            [jump.profiles.window.coarse.labels]
            hide_threshold_px = 18
            "#,
        );

        let default_jump = config.resolved_jump_config(None).unwrap();
        assert_eq!(default_jump.start_region, JumpStartRegion::VirtualScreen);
        assert_eq!(default_jump.hints.selection_keys, "XY");

        let profile = config.resolved_jump_config(Some("window")).unwrap();
        assert_eq!(profile.start_region, JumpStartRegion::ActiveWindowBounds);
        assert_eq!(profile.hints.selection_keys, "XY");
        assert_eq!(
            profile.preview_edge_behavior,
            PreviewEdgeBehavior::ShiftIntoBounds
        );
        assert_eq!(profile.coarse.width, 7);
        assert_eq!(profile.coarse.labels.hide_threshold_px, 18);

        assert!(config
            .resolved_jump_config(Some("missing"))
            .unwrap_err()
            .contains("missing"));
    }

    #[test]
    fn nested_label_config_normalizes() {
        let config = parse_config(
            r#"
            [jump.coarse.labels]
            font_scale = -2.0
            center_marker = true
            separators = false
            hide_threshold_px = -5
            "#,
        );

        assert_eq!(config.jump.coarse.labels.font_scale, 1.0);
        assert!(config.jump.coarse.labels.center_marker);
        assert!(!config.jump.coarse.labels.separators);
        assert_eq!(config.jump.coarse.labels.hide_threshold_px, 0);
    }

    #[test]
    fn nested_label_config_keeps_predictable_defaults() {
        let config = parse_config(
            r#"
            [jump.fine.labels]
            center_marker = true
            "#,
        );

        assert_eq!(config.jump.fine.labels.font_scale, 1.0);
        assert!(config.jump.fine.labels.center_marker);
        assert!(config.jump.fine.labels.separators);
        assert_eq!(config.jump.fine.labels.hide_threshold_px, 0);
    }

    #[test]
    fn selection_keys_normalization_uppercases_and_dedupes() {
        let config = parse_config(
            r#"
            [jump.hints]
            selection_keys = "abcaBCd"
            "#,
        );

        assert_eq!(config.jump.hints.selection_keys, "ABCD");
    }

    #[test]
    fn selection_keys_strips_whitespace() {
        let config = parse_config(
            r#"
            [jump.hints]
            selection_keys = "a b\tc\n d"
            "#,
        );

        assert_eq!(config.jump.hints.selection_keys, "ABCD");
    }

    #[test]
    fn selection_keys_invalid_falls_back_to_default() {
        let _ = take_config_warnings();

        let config = parse_config(
            r#"
            [jump.hints]
            selection_keys = " a A "
            "#,
        );
        let warnings = take_config_warnings();

        assert_eq!(
            config.jump.hints.selection_keys,
            JumpHintsConfig::default().selection_keys
        );
        assert!(warnings.iter().any(|warning| {
            warning.contains("jump.hints.selection_keys")
                && warning.contains("at least 2 unique keys")
                && warning.contains("using default")
        }));
    }

    #[test]
    fn invalid_jump_target_region_mode_falls_back_to_exact_region() {
        let config = parse_config(
            r#"
            [jump.coarse]
            target_region_mode = "legacy_zoom"
            "#,
        );

        assert_eq!(
            config.jump.coarse.target_region_mode,
            JumpTargetRegionMode::ExactRegion
        );
    }

    #[test]
    fn invalid_jump_enum_values_return_clear_parse_errors() {
        for toml in [
            r#"
            [jump]
            cursor_between_stages = "after_each_stage"
            "#,
            r#"
            [jump]
            start_region = "focused_window"
            "#,
            r#"
            [jump]
            preview_edge_behavior = "stretch"
            "#,
            r#"
            [jump.coarse]
            aim_point = "middle"
            "#,
        ] {
            let err = parse_config_error(toml);
            assert!(
                err.contains("unknown variant"),
                "expected unknown variant error, got: {err}"
            );
        }
    }

    #[test]
    fn legacy_preview_margin_maps_to_visual_context_when_new_field_absent() {
        let config = parse_config(
            r#"
            [jump.coarse]
            preview_margin_percent = 12
            "#,
        );

        assert_eq!(config.jump.coarse.visual_context_margin_percent, 12);
        assert_eq!(config.jump.coarse.target_margin_percent, 0);
    }

    #[test]
    fn visual_context_margin_takes_precedence_over_legacy_preview_margin() {
        let config = parse_config(
            r#"
            [jump.coarse]
            preview_margin_percent = 12
            visual_context_margin_percent = 7
            "#,
        );

        assert_eq!(config.jump.coarse.visual_context_margin_percent, 7);
    }

    #[test]
    fn disabled_fine_disables_precise_in_precision_mode() {
        let config = parse_config(
            r#"
            [jump]
            mode = "precision"

            [jump.coarse]
            width = 10
            height = 10

            [jump.fine]
            enabled = false

            [jump.precise]
            enabled = true
            "#,
        );

        assert!(!config.jump.precise.enabled);
    }

    #[test]
    fn jump_stage_sizes_and_margin_fields_are_clamped_independently() {
        let config = parse_config(
            r#"
            [jump.coarse]
            width = 0
            height = 27
            target_margin_percent = 99
            visual_context_margin_percent = 99
            "#,
        );

        assert_eq!(config.jump.coarse.width, 1);
        assert_eq!(config.jump.coarse.height, 26);
        assert_eq!(
            config.jump.coarse.target_margin_percent,
            MAX_JUMP_REGION_MARGIN_PERCENT
        );
        assert_eq!(
            config.jump.coarse.visual_context_margin_percent,
            MAX_JUMP_REGION_MARGIN_PERCENT
        );
    }

    #[test]
    fn legacy_preview_margin_is_clamped_after_alias_mapping() {
        let config = parse_config(
            r#"
            [jump.coarse]
            preview_margin_percent = 99
            "#,
        );

        assert_eq!(
            config.jump.coarse.visual_context_margin_percent,
            MAX_JUMP_REGION_MARGIN_PERCENT
        );
        assert_eq!(config.jump.coarse.target_margin_percent, 0);
    }

    #[test]
    fn jump_stage_zoom_scale_is_clamped_to_supported_range() {
        let min = parse_config(
            r#"
            [jump.coarse]
            zoom_scale = 0.25
            "#,
        );
        let max = parse_config(
            r#"
            [jump.coarse]
            zoom_scale = 12.5
            "#,
        );

        assert_eq!(min.jump.coarse.zoom_scale, MIN_JUMP_ZOOM_SCALE);
        assert_eq!(max.jump.coarse.zoom_scale, MAX_JUMP_ZOOM_SCALE);
    }

    #[test]
    fn wheel_config_parses_and_normalizes_bounds() {
        let config = parse_config(
            r#"
            [wheel]
            default_speed = 99
            min_speed = 2
            max_speed = 8
            speed_step = 0
            tick_interval = 0
            speed_indicator_ms = 0
            "#,
        );

        assert_eq!(config.wheel.default_speed, 8);
        assert_eq!(config.wheel.min_speed, 2);
        assert_eq!(config.wheel.max_speed, 8);
        assert_eq!(config.wheel.speed_step, 1);
        assert_eq!(config.wheel.tick_interval, config.polling_rate);
        assert_eq!(
            config.wheel.speed_indicator_ms,
            DEFAULT_WHEEL_SPEED_INDICATOR_MS
        );
    }

    #[test]
    fn mouse_speed_config_parses_and_normalizes_bounds() {
        let config = parse_config(
            r#"
            [mouse_speed]
            default_speed = 99
            min_speed = 2
            max_speed = 8
            speed_step = 0
            flash_indicator_ms = 0
            "#,
        );

        assert_eq!(config.mouse_speed.default_speed, 8);
        assert_eq!(config.mouse_speed.min_speed, 2);
        assert_eq!(config.mouse_speed.max_speed, 8);
        assert_eq!(config.mouse_speed.speed_step, 1);
        assert_eq!(
            config.mouse_speed.flash_indicator_ms,
            DEFAULT_MOUSE_SPEED_FLASH_MS
        );
        assert_eq!(config.starting_speed, config.mouse_speed.default_speed);
    }

    #[test]
    fn slow_mouse_defaults_are_precision_focused() {
        let config = parse_config("");

        assert_eq!(config.slow_mouse.strategy, SlowMouseStrategy::Fixed);
        assert_eq!(config.slow_mouse.fixed_speed, 1);
        assert_eq!(config.slow_mouse.multiplier, 0.25);
        assert_eq!(config.slow_mouse.subtract_speed, 4);
        assert_eq!(config.slow_mouse.min_speed, 1);
        assert_eq!(config.slow_mouse.max_speed, 2);
        assert_eq!(config.slow_mouse.acceleration, 0);
        assert_eq!(config.slow_mouse.acceleration_rate, 1);
    }

    #[test]
    fn slow_mouse_config_parses_from_toml() {
        let fixed = parse_config(
            r#"
            [slow_mouse]
            strategy = "fixed"
            fixed_speed = 2
            "#,
        );
        assert_eq!(fixed.slow_mouse.strategy, SlowMouseStrategy::Fixed);
        assert_eq!(fixed.slow_mouse.fixed_speed, 2);

        let multiplier = parse_config(
            r#"
            [slow_mouse]
            strategy = "multiplier"
            multiplier = 0.5
            "#,
        );
        assert_eq!(
            multiplier.slow_mouse.strategy,
            SlowMouseStrategy::Multiplier
        );
        assert_eq!(multiplier.slow_mouse.multiplier, 0.5);

        let subtract = parse_config(
            r#"
            [slow_mouse]
            strategy = "subtract"
            subtract_speed = 3
            "#,
        );
        assert_eq!(subtract.slow_mouse.strategy, SlowMouseStrategy::Subtract);
        assert_eq!(subtract.slow_mouse.subtract_speed, 3);
    }

    #[test]
    fn slow_mouse_out_of_range_values_normalize() {
        let _ = take_config_warnings();
        let config = parse_config(
            r#"
            [slow_mouse]
            fixed_speed = -5
            multiplier = 0.0
            subtract_speed = -2
            min_speed = 0
            max_speed = -1
            acceleration = -1
            acceleration_rate = 0
            "#,
        );
        let warnings = take_config_warnings();

        assert_eq!(config.slow_mouse.strategy, SlowMouseStrategy::Fixed);
        assert_eq!(config.slow_mouse.min_speed, 1);
        assert_eq!(config.slow_mouse.max_speed, 1);
        assert_eq!(config.slow_mouse.fixed_speed, 1);
        assert_eq!(config.slow_mouse.multiplier, 0.25);
        assert_eq!(config.slow_mouse.subtract_speed, 0);
        assert_eq!(config.slow_mouse.acceleration, 0);
        assert_eq!(config.slow_mouse.acceleration_rate, 1);

        assert!(warnings
            .iter()
            .any(|warning| warning.contains("slow_mouse.min_speed is below 1")));
        assert!(warnings.iter().any(|warning| {
            warning.contains("slow_mouse.max_speed is below slow_mouse.min_speed")
        }));
        assert!(warnings.iter().any(|warning| {
            warning.contains("slow_mouse.fixed_speed is outside slow_mouse min/max range")
        }));
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("slow_mouse.multiplier is <= 0.0")));
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("slow_mouse.subtract_speed is below 0")));
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("slow_mouse.acceleration is below 0")));
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("slow_mouse.acceleration_rate is 0")));
    }

    #[test]
    fn legacy_starting_speed_seeds_mouse_speed_default_when_section_is_absent() {
        let config = parse_config("starting_speed = 4");

        assert_eq!(config.mouse_speed.default_speed, 4);
        assert_eq!(config.starting_speed, 4);
    }

    #[test]
    fn normalize_prefers_mouse_speed_default_for_starting_speed() {
        let config = parse_config(
            r#"
            starting_speed = 4

            [mouse_speed]
            default_speed = 5
            "#,
        );

        assert_eq!(config.mouse_speed.default_speed, 5);
        assert_eq!(config.starting_speed, 5);
    }

    #[test]
    fn wheel_and_edge_jump_sections_parse_with_sane_defaults() {
        let config = parse_config(
            r#"
            [mouse_speed]
            default_speed = 5

            [wheel]
            default_speed = 5

            [edge_jump]
            offset_px = 4
            use_work_area = true
            "#,
        );

        assert_eq!(config.mouse_speed.default_speed, 5);
        assert_eq!(
            config.mouse_speed.min_speed,
            MouseSpeedConfig::default().min_speed
        );
        assert_eq!(config.wheel.default_speed, 5);
        assert_eq!(config.wheel.min_speed, WheelConfig::default().min_speed);
        assert_eq!(config.wheel.max_speed, WheelConfig::default().max_speed);
        assert_eq!(config.wheel.speed_step, WheelConfig::default().speed_step);
        assert_eq!(
            config.wheel.tick_interval,
            WheelConfig::default().tick_interval
        );
        assert_eq!(
            config.wheel.speed_indicator_ms,
            WheelConfig::default().speed_indicator_ms
        );
        assert_eq!(config.edge_jump.offset_px, 4);
        assert!(config.edge_jump.use_work_area);
    }

    #[test]
    fn edge_jump_offset_is_clamped_to_supported_range() {
        let negative = parse_config(
            r#"
            [edge_jump]
            offset_px = -4
            "#,
        );
        let oversized = parse_config(
            r#"
            [edge_jump]
            offset_px = 20000
            "#,
        );

        assert_eq!(negative.edge_jump.offset_px, 0);
        assert_eq!(oversized.edge_jump.offset_px, MAX_EDGE_JUMP_OFFSET_PX);
    }

    #[test]
    fn continuous_key_actions_track_state_without_immediate_execution() {
        assert!(Action::MoveLeft.is_continuous());
        assert!(Action::SlowMouse.is_continuous());
        assert!(Action::WheelDown.is_continuous());
    }

    #[test]
    fn one_shot_key_actions_execute_on_key_down_only() {
        assert!(!Action::LeftClick.is_continuous());
        assert!(!Action::MoveToTopEdge.is_continuous());
        assert!(!Action::WheelSpeedUp.is_continuous());
        assert!(!Action::MouseSpeedUp.is_continuous());
        assert!(!Action::MouseSpeedDown.is_continuous());
        assert!(!Action::MouseSpeedReset.is_continuous());
        assert!(!Action::ClickThenDisable.is_continuous());
        assert!(!Action::ToggleDragMode.is_continuous());
    }

    #[test]
    fn invalid_jump_mode_fails_deserialization() {
        let err = toml::from_str::<Config>(
            r#"
            [jump]
            mode = "turbo"
            "#,
        )
        .unwrap_err()
        .to_string();

        assert!(err.contains("unknown variant"));
    }

    #[test]
    fn startup_summary_uses_current_crate_id() {
        let summary = startup_summary_line();

        assert!(summary.contains(APP_DISPLAY_NAME));
        assert!(summary.contains("multi_mousemover"));
        assert!(!summary.contains(LEGACY_PACKAGE_NAME));
    }

    #[test]
    fn startup_validation_summary_formats_runtime_settings_and_warnings() {
        let mut config = Config::default().normalize().unwrap();
        config.movement_profiles.insert(
            "fast".to_string(),
            MouseSpeedConfig {
                default_speed: 5,
                min_speed: 1,
                max_speed: 12,
                speed_step: 2,
                flash_indicator_ms: 700,
            },
        );

        let summary = startup_validation_summary(&config, &["wheel.min_speed clamped".to_string()]);

        assert!(summary.contains("polling_rate=8ms"));
        assert!(summary.contains("mouse_speed default=1 range=1..12 step=1 profiles=1"));
        assert!(summary.contains("wheel default=3 range=1..12 step=1 tick=8ms"));
        assert!(summary.contains("slow_mouse strategy=Fixed speed=1 (default 1, range 1..2) acceleration=0 every 1 tick(s)"));
        assert!(summary.contains("warnings=1"));
        assert!(summary.contains("warning: wheel.min_speed clamped"));
    }

    #[test]
    fn normalized_values_emit_collectable_warnings() {
        let _ = take_config_warnings();

        let config = parse_config(
            r#"
            [wheel]
            min_speed = 0
            vertical_multiplier = 0
            "#,
        );
        let warnings = take_config_warnings();

        assert_eq!(config.wheel.min_speed, 1);
        assert_eq!(config.wheel.vertical_multiplier, 1);
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("wheel.min_speed is below 1")));
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("wheel.vertical_multiplier is below 1")));
    }

    #[test]
    fn config_warning_store_accepts_audit_entries() {
        let _ = take_config_warnings();
        let audit_warning = ConfigAuditWarning {
            path: "system_bindings.polling_rate".to_string(),
            severity: ConfigAuditSeverity::Warning,
            message: "top-level polling_rate is preferred".to_string(),
            suggestion: "Move this value to polling_rate.".to_string(),
        };

        emit_config_audit_warnings(&[audit_warning]);
        let overlay_warnings = recent_config_warnings_for_overlay();
        let stored = overlay_warnings
            .iter()
            .find(|warning| warning.path == "system_bindings.polling_rate")
            .expect("expected stored audit warning");

        assert_eq!(
            stored.severity,
            help_overlay::OverlayWarningSeverity::Warning
        );
        assert!(stored.message.contains("top-level polling_rate"));
        assert!(stored.fix_path.contains("Move this value"));
        let _ = take_config_warnings();
    }

    #[test]
    fn movement_policy_warnings_emitted_for_legacy_preferred_paths() {
        let _ = take_config_warnings();

        let legacy_seeded = parse_config("starting_speed = 4");
        let seeded_warnings = take_config_warnings();

        assert_eq!(legacy_seeded.mouse_speed.default_speed, 4);
        assert!(seeded_warnings.iter().any(|warning| {
            warning.contains("info:")
                && warning.contains("starting_speed is a compatibility field")
                && warning.contains("[mouse_speed].default_speed")
        }));

        let modern_preferred = parse_config(
            r#"
            starting_speed = 4

            [mouse_speed]
            default_speed = 5
            "#,
        );
        let preferred_warnings = take_config_warnings();

        assert_eq!(modern_preferred.starting_speed, 5);
        assert!(preferred_warnings.iter().any(|warning| {
            warning.contains("info:")
                && warning.contains("starting_speed=4 is ignored")
                && warning.contains("[mouse_speed].default_speed=5")
        }));
    }

    #[test]
    fn reload_success_applies_config_and_clears_runtime_state() {
        let mut old_config = Config::default().normalize().unwrap();
        old_config.mouse_speed.default_speed = 2;
        let mut new_config = Config::default().normalize().unwrap();
        new_config.mouse_speed.default_speed = 5;
        new_config.starting_speed = 5;

        let mut action_handler = ActionHandler::new(MouseMaster::new_with_backend(
            old_config,
            FakeBackend::default(),
        ));
        let mut app_state = AppState::default();
        app_state.set_bound_keys([VirtualKey::A]);
        app_state.route_key_event(KeyEvent::new(VirtualKey::A, true), Some(Action::MoveLeft));
        action_handler.process_active_keys(Action::MoveLeft, true);
        action_handler.mouse_master.press_left_button_for_drag();

        let resolution =
            apply_loaded_config(&mut action_handler, &mut app_state, new_config).unwrap();

        assert_eq!(resolution, JumpOverlayResolution::Hidden);
        assert!(!action_handler.mouse_master.left_button_held());
        assert!(action_handler.active_keys.is_empty());
        assert!(!app_state.has_active_action_keys());
        assert_eq!(
            action_handler.mouse_master.config.mouse_speed.default_speed,
            5
        );
    }

    #[test]
    fn reload_validation_failure_can_rollback_without_mutating_runtime_config() {
        let old_config = Config::default().normalize().unwrap();
        let action_handler = ActionHandler::new(MouseMaster::new_with_backend(
            old_config.clone(),
            FakeBackend::default(),
        ));
        let _app_state = AppState::default();
        let validation = parse_config_error(
            r#"
            [system_bindings]
            toggle_active = "Ctrl+DefinitelyNotAKey"
            "#,
        );

        assert!(validation.contains("system_bindings.toggle_active"));
        assert_eq!(
            action_handler
                .mouse_master
                .config
                .system_bindings
                .toggle_active,
            old_config.system_bindings.toggle_active
        );
    }

    #[test]
    fn programmatic_app_labels_do_not_emit_legacy_package_name() {
        assert_eq!(APP_DISPLAY_NAME, "Multi MouseMover");
        assert_eq!(APP_CRATE_ID, "multi_mousemover");
        assert_ne!(APP_CRATE_ID, LEGACY_PACKAGE_NAME);
    }

    #[test]
    fn readme_documents_current_artifact_names() {
        let readme = include_str!("../README.md");

        assert!(readme.contains("The crate and binary id is `multi_mousemover`"));
        assert!(readme.contains("target/debug/multi_mousemover.exe"));
        assert!(readme.contains("target/release/multi_mousemover.exe"));
        assert!(!readme.contains(LEGACY_PACKAGE_NAME));
    }

    #[test]
    fn heartbeat_accumulates_until_interval_elapses() {
        let mut heartbeat = HeartbeatDiagnostics::default();

        let first = heartbeat.record(
            LoopDiagnostics {
                loop_iterations: 2,
                messages_processed: 3,
                ..LoopDiagnostics::default()
            },
            Duration::from_millis(400),
        );
        let second = heartbeat.record(
            LoopDiagnostics {
                loop_iterations: 5,
                queued_key_events_processed: 7,
                ..LoopDiagnostics::default()
            },
            Duration::from_millis(500),
        );

        assert_eq!(first, None);
        assert_eq!(second, None);
    }

    #[test]
    fn heartbeat_emits_snapshot_and_resets_after_interval() {
        let mut heartbeat = HeartbeatDiagnostics::default();

        assert_eq!(
            heartbeat.record(
                LoopDiagnostics {
                    loop_iterations: 2,
                    messages_processed: 3,
                    ..LoopDiagnostics::default()
                },
                Duration::from_millis(750),
            ),
            None
        );

        let snapshot = heartbeat.record(
            LoopDiagnostics {
                loop_iterations: 5,
                queued_key_events_processed: 7,
                commands_executed: 11,
                movement_ticks: 13,
                ..LoopDiagnostics::default()
            },
            Duration::from_millis(250),
        );

        assert_eq!(
            snapshot,
            Some(LoopDiagnostics {
                loop_iterations: 7,
                messages_processed: 3,
                queued_key_events_processed: 7,
                commands_executed: 11,
                movement_ticks: 13,
            })
        );
        assert_eq!(heartbeat.pending, LoopDiagnostics::default());
    }

    #[test]
    fn heartbeat_preserves_elapsed_remainder_after_rollover() {
        let mut heartbeat = HeartbeatDiagnostics::default();

        let snapshot = heartbeat.record(
            LoopDiagnostics {
                loop_iterations: 1,
                ..LoopDiagnostics::default()
            },
            Duration::from_millis(1250),
        );

        assert_eq!(
            snapshot,
            Some(LoopDiagnostics {
                loop_iterations: 1,
                ..LoopDiagnostics::default()
            })
        );
        assert_eq!(heartbeat.elapsed, Duration::from_millis(250));

        assert_eq!(
            heartbeat.record(
                LoopDiagnostics {
                    loop_iterations: 2,
                    ..LoopDiagnostics::default()
                },
                Duration::from_millis(749),
            ),
            None
        );
        assert_eq!(
            heartbeat.record(
                LoopDiagnostics {
                    loop_iterations: 3,
                    ..LoopDiagnostics::default()
                },
                Duration::from_millis(1),
            ),
            Some(LoopDiagnostics {
                loop_iterations: 5,
                ..LoopDiagnostics::default()
            })
        );
    }

    #[test]
    fn routing_log_filter_includes_bound_actions() {
        let event = KeyEvent::new(VirtualKey::A, true);

        assert!(should_log_routing_event(&event, Some(Action::MoveLeft)));
        assert_eq!(
            routing_action_label(&event, Some(Action::MoveLeft)),
            "MoveLeft"
        );
    }

    #[test]
    fn routing_log_filter_includes_escape_down_as_exit() {
        APP_STATE
            .write()
            .unwrap()
            .set_system_bindings(RuntimeSystemBindings::default());
        let event = KeyEvent::new(VirtualKey::Escape, true);

        assert!(should_log_routing_event(&event, None));
        assert_eq!(routing_action_label(&event, None), "Exit");
    }

    #[test]
    fn routing_log_filter_excludes_unbound_non_exit_events() {
        let event = KeyEvent::new(VirtualKey::B, true);

        assert!(!should_log_routing_event(&event, None));
    }

    #[test]
    fn routing_log_filter_excludes_escape_up_without_binding() {
        let event = KeyEvent::new(VirtualKey::Escape, false);

        assert!(!should_log_routing_event(&event, None));
    }

    #[test]
    fn injected_keyboard_hook_events_bypass_routing() {
        let code = HC_ACTION.try_into().unwrap();

        assert!(is_keyboard_hook_routing_event(
            code,
            WPARAM(WM_KEYDOWN as usize),
            0
        ));
        assert!(!is_keyboard_hook_routing_event(
            code,
            WPARAM(WM_KEYDOWN as usize),
            0x10
        ));
        assert!(!is_keyboard_hook_routing_event(
            code,
            WPARAM(WM_KEYDOWN as usize),
            0x02
        ));
    }

    #[test]
    fn navigate_back_action_executes_alt_left_only_when_active() {
        let mut app_state = AppState::default();
        let mouse_master = MouseMaster::new_with_backend(Config::default(), FakeBackend::default());
        let mut action_handler = ActionHandler::new(mouse_master);
        let mut keyboard_sender = MockKeyboardSender::default();

        execute_key_action_command_with_keyboard(
            &mut action_handler,
            &mut app_state,
            Action::NavigateBack,
            true,
            &mut keyboard_sender,
        );

        assert_eq!(
            keyboard_sender.events,
            vec![
                SyntheticKeyEvent::Press(VirtualKey::Alt),
                SyntheticKeyEvent::Press(VirtualKey::Left),
                SyntheticKeyEvent::Release(VirtualKey::Left),
                SyntheticKeyEvent::Release(VirtualKey::Alt),
            ]
        );

        action_handler.mouse_master.set_active_mode(false);
        execute_key_action_command_with_keyboard(
            &mut action_handler,
            &mut app_state,
            Action::NavigateBack,
            true,
            &mut keyboard_sender,
        );

        assert_eq!(keyboard_sender.events.len(), 4);
    }

    #[test]
    fn help_visible_suppresses_temporary_runtime_notifications() {
        let config = TooltipOverlayConfig::default();
        let notification = RuntimeNotification {
            kind: RuntimeNotificationKind::MouseSpeed,
            title: "Mouse speed".to_string(),
            body: "Speed 6".to_string(),
            duration_ms: 700,
        };

        assert!(runtime_notification_enabled(&notification, config, false));
        assert!(!runtime_notification_enabled(&notification, config, true));
    }

    #[test]
    fn click_then_disable_key_down_clears_active_transition_state() {
        for start_in_jump_mode in [false, true] {
            let mut app_state = AppState::default();
            app_state.set_bound_keys([VirtualKey::Left, VirtualKey::C]);
            app_state.route_key_event(
                KeyEvent::new(VirtualKey::Left, true),
                Some(Action::MoveLeft),
            );
            if start_in_jump_mode {
                let config = Config::default().normalize().unwrap();
                assert!(app_state.enter_jump_mode(
                    &config.jump,
                    config.final_adjust.clone(),
                    jump_region(),
                    VirtualKey::J
                ));
            }

            let mouse_master =
                MouseMaster::new_with_backend(Config::default(), FakeBackend::default());
            let mut action_handler = ActionHandler::new(mouse_master);
            action_handler.process_active_keys(Action::MoveLeft, true);
            action_handler.process_active_keys(Action::WheelDown, true);

            let resolution = execute_key_action_command(
                &mut action_handler,
                &mut app_state,
                Action::ClickThenDisable,
                true,
            );

            assert_eq!(
                action_handler.mouse_master.backend.clicks,
                vec![Button::Left]
            );
            assert_eq!(action_handler.mouse_master.current_mode, ModeState::Idle);
            assert!(action_handler.active_keys.is_empty());
            assert!(!app_state.active_mode());
            assert!(!app_state.is_jump_active());
            assert!(!app_state.has_active_action_keys());
            assert_eq!(resolution, Some(JumpOverlayResolution::Hidden));
            assert_eq!(
                app_state.resolve_jump_overlay(),
                JumpOverlayResolution::Hidden
            );
        }
    }

    #[test]
    fn indicator_state_reflects_runtime_mouse_speed_increase_flash() {
        let mut config = Config::default();
        config.mouse_speed = MouseSpeedConfig {
            default_speed: 2,
            min_speed: 1,
            max_speed: 5,
            speed_step: 2,
            flash_indicator_ms: 700,
        };
        config.starting_speed = 2;
        let mouse_master = MouseMaster::new_with_backend(config, FakeBackend::default());
        let mut action_handler = ActionHandler::new(mouse_master);

        action_handler
            .mouse_master
            .handle_action(Action::MouseSpeedUp);

        let indicator = resolve_indicator_snapshot(IndicatorInput {
            app_active: true,
            jump_active: false,
            jump_stage: None,
            final_adjust_active: false,
            active_actions: &action_handler.active_keys,
            mouse: MouseIndicatorInput {
                active: action_handler.mouse_master.mouse_speed_indicator_active(),
                current_speed: action_handler.mouse_master.mouse_speed_baseline,
                default_speed: action_handler.mouse_master.config.mouse_speed.default_speed,
            },
            wheel: WheelIndicatorInput {
                active: false,
                current_speed: action_handler.mouse_master.current_wheel_speed,
                default_speed: action_handler.mouse_master.config.wheel.default_speed,
            },
            left_button_held: false,
        })
        .state;

        assert_eq!(indicator, IndicatorState::MouseSpeedFast);
    }

    #[test]
    fn indicator_state_reflects_runtime_mouse_speed_reset_flash() {
        let mut config = Config::default();
        config.mouse_speed = MouseSpeedConfig {
            default_speed: 3,
            min_speed: 1,
            max_speed: 5,
            speed_step: 2,
            flash_indicator_ms: 700,
        };
        config.starting_speed = 3;
        let mouse_master = MouseMaster::new_with_backend(config, FakeBackend::default());
        let mut action_handler = ActionHandler::new(mouse_master);

        action_handler
            .mouse_master
            .handle_action(Action::MouseSpeedDown);
        action_handler
            .mouse_master
            .handle_action(Action::MouseSpeedReset);

        let indicator = resolve_indicator_snapshot(IndicatorInput {
            app_active: true,
            jump_active: false,
            jump_stage: None,
            final_adjust_active: false,
            active_actions: &action_handler.active_keys,
            mouse: MouseIndicatorInput {
                active: action_handler.mouse_master.mouse_speed_indicator_active(),
                current_speed: action_handler.mouse_master.mouse_speed_baseline,
                default_speed: action_handler.mouse_master.config.mouse_speed.default_speed,
            },
            wheel: WheelIndicatorInput {
                active: false,
                current_speed: action_handler.mouse_master.current_wheel_speed,
                default_speed: action_handler.mouse_master.config.wheel.default_speed,
            },
            left_button_held: false,
        })
        .state;

        assert_eq!(indicator, IndicatorState::MouseSpeedNormal);
    }

    #[test]
    fn disable_while_drag_active_releases_button_and_clears_state() {
        let mut app_state = AppState::default();
        app_state.set_bound_keys([VirtualKey::Left]);
        app_state.toggle_help();
        app_state.route_key_event(
            KeyEvent::new(VirtualKey::Left, true),
            Some(Action::MoveLeft),
        );

        let mouse_master = MouseMaster::new_with_backend(Config::default(), FakeBackend::default());
        let mut action_handler = ActionHandler::new(mouse_master);
        action_handler.process_active_keys(Action::MoveLeft, true);
        action_handler
            .mouse_master
            .handle_action(Action::ToggleDragMode);

        let resolution = apply_active_mode_transition(&mut action_handler, &mut app_state, false);

        assert_eq!(
            action_handler.mouse_master.backend.operations,
            vec![
                MouseOperation::ButtonDown(Button::Left),
                MouseOperation::ButtonUp(Button::Left),
            ]
        );
        assert!(!action_handler.mouse_master.left_button_held());
        assert_eq!(action_handler.mouse_master.current_mode, ModeState::Idle);
        assert!(action_handler.active_keys.is_empty());
        assert!(!app_state.active_mode());
        assert!(!app_state.help_visible());
        assert!(!app_state.has_active_action_keys());
        assert_eq!(resolution, JumpOverlayResolution::Hidden);
    }

    #[test]
    fn disable_key_action_clears_navigation_help_active_keys_and_exclusive_modes() {
        for mode in ["jump", "grid"] {
            let mut app_state = AppState::default();
            app_state.set_bound_keys([VirtualKey::Left, VirtualKey::H, VirtualKey::Q]);
            app_state.toggle_help();
            app_state.route_key_event(
                KeyEvent::new(VirtualKey::Left, true),
                Some(Action::MoveLeft),
            );
            app_state.route_key_event(
                KeyEvent::new(VirtualKey::H, true),
                Some(Action::NavigateBack),
            );
            match mode {
                "jump" => {
                    let config = Config::default().normalize().unwrap();
                    assert!(app_state.enter_jump_mode(
                        &config.jump,
                        config.final_adjust.clone(),
                        jump_region(),
                        VirtualKey::J,
                    ));
                }
                "grid" => {
                    assert!(app_state.enter_grid_mode(
                        jump_region(),
                        jump_region(),
                        25,
                        25,
                        true,
                        true,
                        true,
                        VirtualKey::G,
                    ));
                }
                _ => unreachable!(),
            }

            let mouse_master =
                MouseMaster::new_with_backend(Config::default(), FakeBackend::default());
            let mut action_handler = ActionHandler::new(mouse_master);
            action_handler.process_active_keys(Action::MoveLeft, true);
            action_handler.process_active_keys(Action::NavigateBack, true);
            action_handler
                .mouse_master
                .handle_action(Action::ToggleDragMode);

            let resolution = execute_key_action_command(
                &mut action_handler,
                &mut app_state,
                Action::Disable,
                true,
            );

            assert_eq!(
                action_handler.mouse_master.backend.operations,
                vec![
                    MouseOperation::ButtonDown(Button::Left),
                    MouseOperation::ButtonUp(Button::Left),
                ],
                "{mode}"
            );
            assert!(!action_handler.mouse_master.left_button_held(), "{mode}");
            assert_eq!(action_handler.mouse_master.current_mode, ModeState::Idle);
            assert!(action_handler.active_keys.is_empty(), "{mode}");
            assert!(!app_state.active_mode(), "{mode}");
            assert!(!app_state.help_visible(), "{mode}");
            assert!(!app_state.has_active_action_keys(), "{mode}");
            assert!(!app_state.is_jump_active(), "{mode}");
            assert!(!app_state.is_grid_active(), "{mode}");
            assert_eq!(resolution, Some(JumpOverlayResolution::Hidden), "{mode}");
            assert_eq!(
                app_state.resolve_jump_overlay(),
                JumpOverlayResolution::Hidden,
                "{mode}"
            );

            let second_resolution = execute_key_action_command(
                &mut action_handler,
                &mut app_state,
                Action::Disable,
                true,
            );

            assert_eq!(second_resolution, None, "{mode}");
            assert_eq!(action_handler.mouse_master.current_mode, ModeState::Idle);
            assert!(!app_state.active_mode(), "{mode}");
        }
    }

    #[test]
    fn exclusive_mode_transition_clears_visible_help_state() {
        let mut app_state = AppState::default();
        app_state.toggle_help();
        assert!(app_state.help_visible());

        clear_help_for_exclusive_mode(&mut app_state);

        assert!(!app_state.help_visible());
    }

    #[test]
    fn click_then_disable_from_drag_state_releases_clicks_then_disables() {
        let mut app_state = AppState::default();
        app_state.set_bound_keys([VirtualKey::Left, VirtualKey::C]);
        app_state.route_key_event(
            KeyEvent::new(VirtualKey::Left, true),
            Some(Action::MoveLeft),
        );

        let mouse_master = MouseMaster::new_with_backend(Config::default(), FakeBackend::default());
        let mut action_handler = ActionHandler::new(mouse_master);
        action_handler.process_active_keys(Action::MoveLeft, true);
        action_handler
            .mouse_master
            .handle_action(Action::ToggleDragMode);

        let resolution = execute_key_action_command(
            &mut action_handler,
            &mut app_state,
            Action::ClickThenDisable,
            true,
        );

        assert_eq!(
            action_handler.mouse_master.backend.operations,
            vec![
                MouseOperation::ButtonDown(Button::Left),
                MouseOperation::ButtonUp(Button::Left),
                MouseOperation::Click(Button::Left),
            ]
        );
        assert!(!action_handler.mouse_master.left_button_held());
        assert_eq!(action_handler.mouse_master.current_mode, ModeState::Idle);
        assert!(action_handler.active_keys.is_empty());
        assert!(!app_state.active_mode());
        assert!(!app_state.has_active_action_keys());
        assert_eq!(resolution, Some(JumpOverlayResolution::Hidden));
    }
}
