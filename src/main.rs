mod action;
mod action_handler;
mod app_state;
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
use app_state::{AppCommand, AppState, JumpOverlayResolution, KeyEvent};
use indicator::{resolve_indicator_state, IndicatorInput, WheelIndicatorInput};
use jump_overlay::{
    hide_jump_overlay, show_jump_overlay, update_jump_overlay, virtual_screen_region,
};
use jump_session::JumpSessionUpdate;
use jump_view::{JumpLabelMetadata, JumpVisuals};
use key_chord::{KeyChord, RuntimeSystemBindings};
use keyboard::*;
use lazy_static::lazy_static;
use overlay::OVERLAY;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::thread::sleep;
use std::time::{Duration, Instant};
use std::{env, error::Error, fs, io};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::*;

const DEFAULT_POLLING_RATE_MS: u64 = 8;
const MAX_MESSAGES_PER_TICK: usize = 64;
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
const DEBUG_HEARTBEAT_ENV: &str = "MULTI_MOUSEMOVER_DEBUG";
const MIN_JUMP_STAGE_SIZE: u32 = 1;
const MAX_JUMP_STAGE_SIZE: u32 = 26;
const MAX_JUMP_REGION_MARGIN_PERCENT: u8 = 50;
const MIN_JUMP_ZOOM_SCALE: f32 = 1.0;
const MAX_JUMP_ZOOM_SCALE: f32 = 10.0;
const MAX_EDGE_JUMP_OFFSET_PX: i32 = 10_000;
const MAX_JUMP_AIM_OFFSET_PX: i32 = 10_000;
const MAX_FINAL_ADJUST_STEP_PX: i32 = 10_000;

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
    jump: JumpConfig,
    final_adjust: FinalAdjustConfig,
    wheel: WheelConfig,
    edge_jump: EdgeJumpConfig,
    starting_speed: i32,    // Initial speed in pixels
    acceleration: i32,      // Increment value for acceleration
    acceleration_rate: u32, // Polling cycles before applying acceleration
    top_speed: i32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            key_bindings: Vec::new(),
            system_bindings: SystemBindings::default(),
            polling_rate: DEFAULT_POLLING_RATE_MS,
            grid_size: GridSize::default(),
            jump: JumpConfig::default(),
            final_adjust: FinalAdjustConfig::default(),
            wheel: WheelConfig::default(),
            edge_jump: EdgeJumpConfig::default(),
            starting_speed: 1,
            acceleration: 2,
            acceleration_rate: 1,
            top_speed: 6,
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JumpAimPoint {
    Center,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    CustomOffset,
}

impl Default for JumpAimPoint {
    fn default() -> Self {
        Self::Center
    }
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
pub struct WheelConfig {
    default_speed: i32,
    min_speed: i32,
    max_speed: i32,
    speed_step: i32,
    tick_interval: u64,
}

impl Default for WheelConfig {
    fn default() -> Self {
        Self {
            default_speed: 3,
            min_speed: 1,
            max_speed: 12,
            speed_step: 1,
            tick_interval: DEFAULT_POLLING_RATE_MS,
        }
    }
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

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum JumpMode {
    Single,
    Precision,
}

impl Default for JumpMode {
    fn default() -> Self {
        Self::Single
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CursorBetweenStagesMode {
    None,
    MoveToRegionCenter,
    PreviewOnly,
    WarpAndContinue,
}

impl Default for CursorBetweenStagesMode {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PreviewEdgeBehavior {
    Clamp,
    ShiftIntoBounds,
    AllowAsymmetricContext,
    DisableContextNearEdges,
}

impl Default for PreviewEdgeBehavior {
    fn default() -> Self {
        Self::Clamp
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JumpStartRegion {
    VirtualScreen,
    CurrentMonitor,
    ActiveWindowMonitor,
    ActiveWindowBounds,
}

impl Default for JumpStartRegion {
    fn default() -> Self {
        Self::VirtualScreen
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JumpTargetRegionMode {
    ExactRegion,
    RegionWithContext,
    ExpandedTarget,
    CursorCenteredZoom,
}

impl Default for JumpTargetRegionMode {
    fn default() -> Self {
        Self::ExactRegion
    }
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
    visuals: JumpVisualsConfig,
    coarse: JumpStageConfig,
    fine: JumpStageConfig,
    precise: JumpStageConfig,
    profiles: HashMap<String, JumpProfileConfig>,
}

impl Default for JumpConfig {
    fn default() -> Self {
        Self {
            mode: JumpMode::Single,
            cursor_between_stages: CursorBetweenStagesMode::None,
            start_region: JumpStartRegion::VirtualScreen,
            preview_edge_behavior: PreviewEdgeBehavior::Clamp,
            visuals: JumpVisualsConfig::default(),
            coarse: JumpStageConfig::missing_coarse(),
            fine: JumpStageConfig {
                enabled: false,
                width: 5,
                height: 5,
                aim_point: JumpAimPoint::Center,
                aim_offset_x_px: 0,
                aim_offset_y_px: 0,
                target_margin_percent: 0,
                visual_context_margin_percent: 10,
                zoom_scale: 1.5,
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
        self.normalize_jump_config();
        self.normalize_final_adjust_config();
        self.normalize_wheel_config();
        self.normalize_edge_jump_config();
        self.runtime_system_bindings()?;
        Ok(self)
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
            Ok(config_str) => return toml::from_str::<Self>(&config_str)?.normalize(),
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
                Ok(config_str) => return toml::from_str::<Self>(&config_str)?.normalize(),
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

        APP_STATE
            .write()
            .unwrap()
            .set_bound_chords(key_actions.bound_chords());
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
    eprintln!("[config warning] {message}");
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

unsafe extern "system" fn keyboard_hook(code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if code == HC_ACTION.try_into().unwrap()
        && (w_param.0 as u32 == WM_KEYDOWN
            || w_param.0 as u32 == WM_SYSKEYDOWN
            || w_param.0 as u32 == WM_KEYUP
            || w_param.0 as u32 == WM_SYSKEYUP)
    {
        HOOK_EVENTS_SEEN.fetch_add(1, Ordering::Relaxed);
        let kbd = *(l_param.0 as *const KBDLLHOOKSTRUCT);
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

fn set_active_mode(active: bool) {
    {
        let mut action_handler = ACTION_HANDLER.write().unwrap();
        if !active {
            action_handler.clear_active_keys();
        }
        action_handler.mouse_master.set_active_mode(active);
    }

    let resolution = {
        let mut app_state = APP_STATE.write().unwrap();
        if !active {
            app_state.clear_active_action_keys_and_exit_jump_mode();
        }
        app_state.set_active_mode(active);
        app_state.resolve_jump_overlay()
    };
    sync_jump_overlay(resolution);
}

fn execute_key_action_command<B: MouseBackend>(
    action_handler: &mut ActionHandler<B>,
    app_state: &mut AppState,
    action: Action,
    is_down: bool,
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
            app_state.clear_active_action_keys_and_exit_jump_mode();
            action_handler.mouse_master.set_active_mode(false);
            app_state.set_active_mode(false);
            return Some(app_state.resolve_jump_overlay());
        }
        return None;
    }

    if is_down {
        action_handler.execute_action(&action);
    }
    None
}

fn execute_app_command(command: AppCommand, debug_diagnostics: bool) {
    if debug_diagnostics {
        match &command {
            AppCommand::ToggleActiveMode => println!("[command] ToggleActiveMode"),
            AppCommand::SetActiveMode { active } => {
                println!("[command] SetActiveMode active={active}")
            }
            AppCommand::Exit => println!("[command] Exit"),
            AppCommand::EnterJumpMode {
                activation_key,
                profile,
            } => {
                println!(
                    "[command] EnterJumpMode activation_key={activation_key:?} profile={profile:?}"
                )
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
        }
        AppCommand::Exit => {
            ACTION_HANDLER.write().unwrap().mouse_master.exit();
        }
        AppCommand::EnterJumpMode {
            activation_key,
            profile,
        } => {
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
    println!("🚀 Program Start!");

    // Set a panic hook to ensure we clean up resources on unexpected errors
    std::panic::set_hook(Box::new(|info| {
        eprintln!("Application panicked: {}", info);
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

        let movement_tick = ACTION_HANDLER.write().unwrap().tick_movement();
        if movement_tick.moving {
            loop_diagnostics.movement_ticks += 1;
        }

        let indicator_state = {
            let action_handler = ACTION_HANDLER.read().unwrap();
            let app_state = APP_STATE.read().unwrap();
            resolve_indicator_state(IndicatorInput {
                app_active: app_state.active_mode(),
                jump_active: app_state.is_jump_active(),
                active_actions: &action_handler.active_keys,
                wheel: WheelIndicatorInput {
                    active: action_handler
                        .active_keys
                        .iter()
                        .any(|action| action.is_wheel_direction()),
                    current_speed: action_handler.mouse_master.current_wheel_speed,
                    default_speed: action_handler.mouse_master.config.wheel.default_speed,
                },
                left_button_held: action_handler.mouse_master.left_button_held(),
            })
        };
        if let Ok(mut maybe_ov) = OVERLAY.lock() {
            if let Some(ref mut ov) = *maybe_ov {
                ov.update_overlay_status(indicator_state);
            }
        }

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

    #[derive(Default)]
    struct FakeBackend {
        location: (i32, i32),
        clicks: Vec<Button>,
        button_downs: Vec<Button>,
        button_ups: Vec<Button>,
        moves: Vec<(i32, i32)>,
        scrolls: Vec<(i32, Axis)>,
    }

    impl MouseBackend for FakeBackend {
        fn click(&mut self, button: Button) -> Result<(), String> {
            self.clicks.push(button);
            Ok(())
        }

        fn button_down(&mut self, button: Button) -> Result<(), String> {
            self.button_downs.push(button);
            Ok(())
        }

        fn button_up(&mut self, button: Button) -> Result<(), String> {
            self.button_ups.push(button);
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

    fn parse_config(toml: &str) -> Config {
        toml::from_str::<Config>(toml).unwrap().normalize().unwrap()
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
        assert_eq!(config.jump.mode, defaults.jump.mode);
        assert_eq!(
            config.jump.cursor_between_stages,
            defaults.jump.cursor_between_stages
        );
        assert_eq!(config.jump.visuals, defaults.jump.visuals);
        assert_eq!(config.jump.coarse.width, defaults.grid_size.width);
        assert_eq!(config.jump.coarse.height, defaults.grid_size.height);
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
        assert!(config.key_bindings.is_empty());
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
                ["B", "wheel_speed_down"]
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
        let config = parse_config(
            r#"
            [jump]
            cursor_between_stages = "preview_only"
            "#,
        );

        assert_eq!(
            config.jump.cursor_between_stages,
            CursorBetweenStagesMode::PreviewOnly
        );
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
    fn jump_stage_zoom_defaults_are_per_stage() {
        let config = Config::default().normalize().unwrap();

        assert_eq!(config.jump.coarse.zoom_scale, 1.0);
        assert_eq!(config.jump.fine.zoom_scale, 1.5);
        assert_eq!(config.jump.precise.zoom_scale, 2.5);
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

        let profile = config.resolved_jump_config(Some("window")).unwrap();
        assert_eq!(profile.start_region, JumpStartRegion::ActiveWindowBounds);
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
            "#,
        );

        assert_eq!(config.wheel.default_speed, 8);
        assert_eq!(config.wheel.min_speed, 2);
        assert_eq!(config.wheel.max_speed, 8);
        assert_eq!(config.wheel.speed_step, 1);
        assert_eq!(config.wheel.tick_interval, config.polling_rate);
    }

    #[test]
    fn wheel_and_edge_jump_sections_parse_with_sane_defaults() {
        let config = parse_config(
            r#"
            [wheel]
            default_speed = 5

            [edge_jump]
            offset_px = 4
            use_work_area = true
            "#,
        );

        assert_eq!(config.wheel.default_speed, 5);
        assert_eq!(config.wheel.min_speed, WheelConfig::default().min_speed);
        assert_eq!(config.wheel.max_speed, WheelConfig::default().max_speed);
        assert_eq!(config.wheel.speed_step, WheelConfig::default().speed_step);
        assert_eq!(
            config.wheel.tick_interval,
            WheelConfig::default().tick_interval
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
}
