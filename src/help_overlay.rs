use crate::action::{Action, Direction2D, StepMoveTier};
use crate::action_handler::{RuntimeNotification, RuntimeNotificationKind};
use crate::app_state::ModeContext;
use crate::key_chord::KeyChord;
use crate::{TooltipOverlayConfig, TooltipOverlayHelpConfig};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ptr;
use std::time::{Duration, Instant};
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

thread_local! {
    static HELP_OVERLAY: RefCell<HelpOverlay> = RefCell::new(HelpOverlay::new());
}

const OVERLAY_WIDTH: i32 = 520;
const MAX_OVERLAY_HEIGHT: i32 = 720;
const PADDING_X: i32 = 18;
const PADDING_Y: i32 = 14;
const TITLE_BODY_GAP: i32 = 8;
const LINE_HEIGHT: i32 = 22;
const CURSOR_OFFSET_X: i32 = 18;
const CURSOR_OFFSET_Y: i32 = 18;
const OVERLAY_ALPHA: u8 = 232;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpBinding {
    pub key: String,
    pub action: String,
    pub action_id: String,
    pub section: HelpBindingSection,
    pub scope: String,
    pub description: String,
    pub warning: Option<HelpBindingWarning>,
    pub bound: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpBindingWarning {
    pub message: String,
    pub severity: OverlayWarningSeverity,
    pub winning_key: Option<String>,
    pub winning_action_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(dead_code)]
pub enum HelpBindingSection {
    Movement,
    ClickDrag,
    Wheel,
    JumpGrid,
    Bookmarks,
    UiHints,
    StepMove,
    Surgical,
    ProfilesRuntime,
    Help,
}

impl std::fmt::Display for HelpBindingSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.title())
    }
}

impl HelpBindingSection {
    fn title(self) -> &'static str {
        match self {
            Self::Movement => "Movement",
            Self::ClickDrag => "Click/Drag",
            Self::Wheel => "Wheel",
            Self::JumpGrid => "Jump/Grid",
            Self::Bookmarks => "Bookmarks",
            Self::UiHints => "UI Hints",
            Self::StepMove => "Step Move",
            Self::Surgical => "Surgical",
            Self::ProfilesRuntime => "Profiles/Runtime",
            Self::Help => "Help",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpOverlayView {
    pub stats: HelpRuntimeStats,
    pub bindings: Vec<HelpBinding>,
    pub config_warnings: Vec<OverlayConfigWarning>,
    pub help_max_bindings: i32,
    pub help_config: TooltipOverlayHelpConfig,
    pub state: HelpState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelpState {
    Hidden,
    Visible {
        filter: String,
        section: Option<HelpBindingSection>,
        page: usize,
    },
}

impl Default for HelpState {
    fn default() -> Self {
        Self::Hidden
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelpInput {
    Search,
    NextSection,
    PreviousSection,
    NextPage,
    PreviousPage,
    Append(char),
    Backspace,
    Escape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpSection {
    General,
    Jump,
    Grid,
    UiHints,
    Bookmarks,
    Disabled,
}

pub fn resolve_help_section(mode: ModeContext) -> HelpSection {
    match mode {
        ModeContext::Inactive => HelpSection::Disabled,
        ModeContext::Jump => HelpSection::Jump,
        ModeContext::Grid => HelpSection::Grid,
        ModeContext::UiHintQuerying | ModeContext::UiHintActive => HelpSection::UiHints,
        ModeContext::Bookmark => HelpSection::Bookmarks,
        ModeContext::Active => HelpSection::General,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OverlayWarningSeverity {
    Info,
    Warning,
    Deprecated,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OverlayConfigWarning {
    pub path: String,
    pub severity: OverlayWarningSeverity,
    pub message: String,
    pub fix_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpRuntimeStats {
    pub app_mode: String,
    pub drag_active: bool,
    pub slow_active: bool,
    pub jump_active: bool,
    pub movement_profile: String,
    pub wheel_profile: String,
    pub mouse_speed: HelpSpeedTier,
    pub wheel_speed: HelpSpeedTier,
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
    pub mode_context: String,
}

impl Default for HelpRuntimeStats {
    fn default() -> Self {
        Self {
            app_mode: "unknown".to_string(),
            drag_active: false,
            slow_active: false,
            jump_active: false,
            movement_profile: "default".to_string(),
            wheel_profile: "default".to_string(),
            mouse_speed: HelpSpeedTier::default(),
            wheel_speed: HelpSpeedTier::default(),
            acceleration: 0,
            acceleration_rate: 0,
            slow_strategy: "fixed".to_string(),
            slow_effective_speed: 1,
            slow_min_speed: 1,
            slow_max_speed: 2,
            slow_acceleration: 0,
            slow_acceleration_rate: 1,
            top_speed: 0,
            polling_rate_ms: 0,
            wheel_tick_interval_ms: 0,
            wheel_vertical_multiplier: 1,
            wheel_horizontal_multiplier: 1,
            mode_context: "general".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HelpSpeedTier {
    pub current: i32,
    pub default: i32,
    pub min: i32,
    pub max: i32,
    pub step: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporaryMessage {
    pub title: String,
    pub body: String,
}

pub type TooltipMessage = TemporaryMessage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelpOverlayContent {
    Hidden,
    Temporary {
        message: TemporaryMessage,
        expires_at: Instant,
    },
    Help {
        view: Box<HelpOverlayView>,
    },
}

#[derive(Debug, Clone)]
struct HelpOverlayState {
    content: HelpOverlayContent,
}

impl HelpOverlayState {
    fn new() -> Self {
        Self {
            content: HelpOverlayContent::Hidden,
        }
    }

    fn show_temporary_tooltip(
        &mut self,
        title: impl Into<String>,
        body: impl Into<String>,
        duration: Duration,
        now: Instant,
    ) {
        if matches!(self.content, HelpOverlayContent::Help { .. }) {
            return;
        }

        self.content = HelpOverlayContent::Temporary {
            message: TemporaryMessage {
                title: title.into(),
                body: body.into(),
            },
            expires_at: now + duration,
        };
    }

    fn show_help_overlay(&mut self, mut view: HelpOverlayView) {
        if matches!(view.state, HelpState::Hidden) {
            view.state = HelpState::Visible {
                filter: String::new(),
                section: None,
                page: 0,
            };
        }
        self.content = HelpOverlayContent::Help {
            view: Box::new(view),
        };
    }

    fn hide(&mut self) {
        if let HelpOverlayContent::Help { view } = &mut self.content {
            view.state = HelpState::Hidden;
        }
        self.content = HelpOverlayContent::Hidden;
    }

    fn handle_help_input(&mut self, input: HelpInput) -> bool {
        let HelpOverlayContent::Help { view } = &mut self.content else {
            return false;
        };
        apply_help_input(view, input)
    }

    fn update_overlay(&mut self, now: Instant) -> bool {
        if matches!(
            &self.content,
            HelpOverlayContent::Temporary { expires_at, .. } if now >= *expires_at
        ) {
            self.hide();
            return true;
        }
        false
    }

    fn visible_content(&self) -> Option<&HelpOverlayContent> {
        match &self.content {
            HelpOverlayContent::Hidden => None,
            content => Some(content),
        }
    }
}

pub struct HelpOverlay {
    hwnd: Option<HWND>,
    state: HelpOverlayState,
    visible: bool,
}

impl HelpOverlay {
    fn new() -> Self {
        Self {
            hwnd: None,
            state: HelpOverlayState::new(),
            visible: false,
        }
    }

    fn create_window(&mut self) {
        if self.hwnd.is_some() {
            return;
        }

        unsafe {
            let h_instance = GetModuleHandleW(None).unwrap();
            let class = w!("HelpOverlayClass");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(help_window_proc),
                hInstance: h_instance.into(),
                lpszClassName: class,
                style: CS_HREDRAW | CS_VREDRAW,
                hbrBackground: HBRUSH(ptr::null_mut()),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);

            match CreateWindowExW(
                help_overlay_ex_style(),
                class,
                w!("HelpOverlay"),
                WS_POPUP,
                80,
                80,
                OVERLAY_WIDTH,
                160,
                None,
                None,
                Some(h_instance.into()),
                None,
            ) {
                Ok(hwnd) => {
                    apply_layered_attributes(hwnd);
                    let _ = ShowWindow(hwnd, SW_HIDE);
                    self.hwnd = Some(hwnd);
                }
                Err(err) => eprintln!("[help] CreateWindowExW failed: {err:?}"),
            }
        }
    }

    fn show_temporary_tooltip(
        &mut self,
        title: impl Into<String>,
        body: impl Into<String>,
        duration: Duration,
        now: Instant,
    ) {
        self.state
            .show_temporary_tooltip(title, body, duration, now);
        self.sync_window();
    }

    fn show_help_overlay(&mut self, view: HelpOverlayView) {
        self.state.show_help_overlay(view);
        self.sync_window();
    }

    fn hide(&mut self) {
        self.state.hide();
        self.sync_window();
    }

    fn handle_help_input(&mut self, input: HelpInput) -> bool {
        let changed = self.state.handle_help_input(input);
        if changed {
            self.sync_window();
        }
        changed
    }

    fn update_overlay(&mut self, now: Instant) {
        if self.state.update_overlay(now) {
            self.sync_window();
        }
    }

    fn sync_window(&mut self) {
        let Some(content) = self.state.visible_content().cloned() else {
            if let Some(hwnd) = self.hwnd {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
            self.visible = false;
            return;
        };

        self.create_window();
        let Some(hwnd) = self.hwnd else {
            return;
        };

        let size = content_size(&content);
        let (x, y) = overlay_position(size);
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                size.0,
                size.1,
                SWP_NOACTIVATE,
            );
            if !self.visible {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                self.visible = true;
            }
        }
        self.request_repaint();
    }

    fn request_repaint(&self) {
        if let Some(hwnd) = self.hwnd {
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
        }
    }

    fn draw(&self, hdc: HDC) {
        let Some(content) = self.state.visible_content() else {
            return;
        };
        let size = content_size(content);
        let rect = RECT {
            left: 0,
            top: 0,
            right: size.0,
            bottom: size.1,
        };

        unsafe {
            let background = CreateSolidBrush(RGB(20, 22, 28));
            if !background.0.is_null() {
                let _ = FillRect(hdc, &rect, background);
                let _ = DeleteObject(background.into());
            }

            let old_bk_mode = SetBkMode(hdc, TRANSPARENT);
            match content {
                HelpOverlayContent::Temporary { message, .. } => {
                    draw_title(hdc, PADDING_X, PADDING_Y, &message.title);
                    let mut y = PADDING_Y + LINE_HEIGHT + TITLE_BODY_GAP;
                    for line in message.body.lines() {
                        draw_body_line(hdc, PADDING_X, y, line);
                        y += LINE_HEIGHT;
                    }
                }
                HelpOverlayContent::Help { view } => {
                    draw_title(hdc, PADDING_X, PADDING_Y, "Multi MouseMover Help");
                    let mut y = PADDING_Y + LINE_HEIGHT + TITLE_BODY_GAP;
                    for line in format_help_lines(view, view_format_config(view)) {
                        draw_body_line(hdc, PADDING_X, y, &line);
                        y += LINE_HEIGHT;
                    }
                }
                HelpOverlayContent::Hidden => {}
            }
            let _ = SetBkMode(hdc, BACKGROUND_MODE(old_bk_mode as u32));
        }
    }
}

fn help_overlay_ex_style() -> WINDOW_EX_STYLE {
    WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE
}

fn apply_layered_attributes(hwnd: HWND) {
    unsafe {
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), OVERLAY_ALPHA, LWA_ALPHA);
    }
}

fn content_size(content: &HelpOverlayContent) -> (i32, i32) {
    let body_lines = match content {
        HelpOverlayContent::Hidden => 0,
        HelpOverlayContent::Temporary { message, .. } => message.body.lines().count().max(1) as i32,
        HelpOverlayContent::Help { view } => format_help_lines(view, view_format_config(view))
            .len()
            .max(1) as i32,
    };
    let height = PADDING_Y * 2 + LINE_HEIGHT + TITLE_BODY_GAP + body_lines * LINE_HEIGHT;
    (OVERLAY_WIDTH, height.min(MAX_OVERLAY_HEIGHT))
}

fn help_stats_lines(stats: &HelpRuntimeStats) -> Vec<String> {
    vec![
        format!("Section: {}", stats.mode_context),
        format!(
            "Mode: {} | Drag: {} | Slow: {} | Jump: {}",
            stats.app_mode,
            on_off(stats.drag_active),
            on_off(stats.slow_active),
            on_off(stats.jump_active)
        ),
        format!(
            "Movement profile: {} | Level {} / {} (default {}, range {}..{}, step {})",
            stats.movement_profile,
            stats.mouse_speed.current,
            stats.mouse_speed.max,
            stats.mouse_speed.default,
            stats.mouse_speed.min,
            stats.mouse_speed.max,
            stats.mouse_speed.step
        ),
        format!(
            "Wheel profile: {} | Level {} / {} (default {}, range {}..{}, step {}) | Repeat {}ms",
            stats.wheel_profile,
            stats.wheel_speed.current,
            stats.wheel_speed.max,
            stats.wheel_speed.default,
            stats.wheel_speed.min,
            stats.wheel_speed.max,
            stats.wheel_speed.step,
            stats.wheel_tick_interval_ms
        ),
        format!(
            "Slow: {} | Speed: {} (range {}..{}) | Acceleration: {} every {} tick(s)",
            stats.slow_strategy,
            stats.slow_effective_speed,
            stats.slow_min_speed,
            stats.slow_max_speed,
            stats.slow_acceleration,
            stats.slow_acceleration_rate
        ),
        format!(
            "Acceleration: {} every {} tick(s) | Top speed: {} | Polling: {}ms | Wheel tick: {}ms",
            stats.acceleration,
            stats.acceleration_rate,
            stats.top_speed,
            stats.polling_rate_ms,
            stats.wheel_tick_interval_ms
        ),
        format!(
            "Wheel multipliers: vertical {} | horizontal {}",
            stats.wheel_vertical_multiplier, stats.wheel_horizontal_multiplier
        ),
    ]
}

fn overlay_warning_badge(severity: OverlayWarningSeverity) -> &'static str {
    match severity {
        OverlayWarningSeverity::Info => "INFO",
        OverlayWarningSeverity::Warning => "WARN",
        OverlayWarningSeverity::Deprecated => "DEPR",
    }
}

pub fn collapsed_config_warnings(
    warnings: &[OverlayConfigWarning],
    max_warnings: usize,
) -> Vec<OverlayConfigWarning> {
    let mut seen = HashSet::new();
    let mut collapsed = Vec::new();

    for warning in warnings.iter().rev() {
        if seen.insert(warning.clone()) {
            collapsed.push(warning.clone());
        }
        if collapsed.len() == max_warnings {
            break;
        }
    }

    collapsed.reverse();
    collapsed
}

pub fn format_config_warning_lines(
    warnings: &[OverlayConfigWarning],
    max_warnings: usize,
) -> Vec<String> {
    let collapsed = collapsed_config_warnings(warnings, max_warnings);
    if collapsed.is_empty() {
        return Vec::new();
    }

    let mut lines = vec![String::new(), "Config warnings:".to_string()];
    lines.extend(collapsed.iter().map(|warning| {
        format!(
            "  [{}] {} - {} Fix: {}",
            overlay_warning_badge(warning.severity),
            warning.path,
            warning.message,
            warning.fix_path
        )
    }));

    let unique_count = warnings.iter().cloned().collect::<HashSet<_>>().len();
    if unique_count > collapsed.len() {
        lines.push(format!(
            "  ... {} more config warning(s)",
            unique_count - collapsed.len()
        ));
    }

    lines
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

pub fn format_tooltip_message(
    notification: &RuntimeNotification,
    stats: &HelpRuntimeStats,
) -> TooltipMessage {
    match notification.kind {
        RuntimeNotificationKind::MouseSpeed => TooltipMessage {
            title: "Mouse level".to_string(),
            body: format!(
                "Level {} / {} (default {}, range {}..{})",
                stats.mouse_speed.current,
                stats.mouse_speed.max,
                stats.mouse_speed.default,
                stats.mouse_speed.min,
                stats.mouse_speed.max
            ),
        },
        RuntimeNotificationKind::WheelSpeed => TooltipMessage {
            title: "Wheel level".to_string(),
            body: format!(
                "Level {} / {} (default {}, range {}..{})",
                stats.wheel_speed.current,
                stats.wheel_speed.max,
                stats.wheel_speed.default,
                stats.wheel_speed.min,
                stats.wheel_speed.max
            ),
        },
        RuntimeNotificationKind::MovementProfile => TooltipMessage {
            title: "Movement profile".to_string(),
            body: stats.movement_profile.clone(),
        },
        RuntimeNotificationKind::WheelProfile => TooltipMessage {
            title: "Wheel profile".to_string(),
            body: format!(
                "{} | Level {} / {} | Repeat {}ms",
                stats.wheel_profile,
                stats.wheel_speed.current,
                stats.wheel_speed.max,
                stats.wheel_tick_interval_ms
            ),
        },
        RuntimeNotificationKind::Drag => TooltipMessage {
            title: "Drag".to_string(),
            body: if stats.drag_active {
                "Left button held".to_string()
            } else {
                "Left button released".to_string()
            },
        },
        RuntimeNotificationKind::ConfigReload => TooltipMessage {
            title: "Config reloaded".to_string(),
            body: "Runtime settings updated".to_string(),
        },
        RuntimeNotificationKind::PanicReset => TooltipMessage {
            title: "Panic reset".to_string(),
            body: "Runtime state restored".to_string(),
        },
        RuntimeNotificationKind::StepMove => TooltipMessage {
            title: "Step move".to_string(),
            body: notification.body.clone(),
        },
    }
}

pub fn format_help_lines(view: &HelpOverlayView, config: TooltipOverlayConfig) -> Vec<String> {
    let mut lines = help_stats_lines(&view.stats);
    lines.extend(format_config_warning_lines(&view.config_warnings, 3));
    let help_config = merged_help_config(view, &config);
    let state = visible_help_state(view);
    let filtered = filtered_bindings(view, &state, &help_config);
    let sections = visible_sections_from_bindings(&filtered);
    let selected_section = selected_section(&state, &sections);
    let section_bindings: Vec<&HelpBinding> = filtered
        .into_iter()
        .filter(|binding| selected_section.is_none_or(|section| binding.section == section))
        .collect();
    let page_size = help_config.page_size.max(1) as usize;
    let page_count = section_bindings.len().max(1).div_ceil(page_size).max(1);
    let page = state_page(&state).min(page_count - 1);
    let start = page * page_size;
    let end = (start + page_size).min(section_bindings.len());
    let page_bindings = &section_bindings[start..end];

    lines.push(String::new());
    lines.push(format!(
        "Help: / search | Tab sections | PageUp/PageDown pages | Esc close"
    ));
    lines.push(format!(
        "Filter: {} | Section: {} | Page {}/{} | Showing {}-{} of {}",
        state_filter(&state).map_or("".to_string(), ToString::to_string),
        selected_section.map_or("All".to_string(), |section| section.title().to_string()),
        page + 1,
        page_count,
        if section_bindings.is_empty() {
            0
        } else {
            start + 1
        },
        end,
        section_bindings.len()
    ));

    if page_bindings.is_empty() {
        lines.push("No help bindings match the current filter.".to_string());
        return lines;
    }

    let mut grouped: BTreeMap<HelpBindingSection, Vec<&HelpBinding>> = BTreeMap::new();
    for binding in page_bindings.iter().copied() {
        grouped.entry(binding.section).or_default().push(binding);
    }
    for section in section_order() {
        let Some(entries) = grouped.get(&section) else {
            continue;
        };
        lines.push(format!("{}:", section.title()));
        for binding in entries {
            let unbound = if binding.bound { "" } else { " (unbound)" };
            let warning = binding
                .warning
                .as_ref()
                .map(|warning| format!(" [WARN: {}]", warning.message))
                .unwrap_or_default();
            lines.push(format!(
                "  {}  -  {}{} — {} [{}]{}",
                binding.key, binding.action, unbound, binding.description, binding.scope, warning
            ));
        }
    }

    lines
}

fn merged_help_config(
    view: &HelpOverlayView,
    config: &TooltipOverlayConfig,
) -> TooltipOverlayHelpConfig {
    let mut help = config.help.clone();
    if view.help_config != TooltipOverlayHelpConfig::default() {
        help = view.help_config.clone();
    }
    help
}

fn visible_help_state(view: &HelpOverlayView) -> HelpState {
    match &view.state {
        HelpState::Visible { .. } => view.state.clone(),
        HelpState::Hidden => HelpState::Visible {
            filter: String::new(),
            section: None,
            page: 0,
        },
    }
}

fn state_filter(state: &HelpState) -> Option<&str> {
    match state {
        HelpState::Visible { filter, .. } => Some(filter.as_str()),
        HelpState::Hidden => None,
    }
}

fn state_page(state: &HelpState) -> usize {
    match state {
        HelpState::Visible { page, .. } => *page,
        HelpState::Hidden => 0,
    }
}

fn selected_section(
    state: &HelpState,
    sections: &[HelpBindingSection],
) -> Option<HelpBindingSection> {
    match state {
        HelpState::Visible { section, .. } => section.filter(|s| sections.contains(s)),
        HelpState::Hidden => None,
    }
}

fn filtered_bindings<'a>(
    view: &'a HelpOverlayView,
    state: &HelpState,
    config: &TooltipOverlayHelpConfig,
) -> Vec<&'a HelpBinding> {
    let filter = state_filter(state)
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    let mut bindings: Vec<_> = view
        .bindings
        .iter()
        .filter(|binding| config.show_unbound_actions || binding.bound)
        .filter(|binding| config.show_conflicts || binding.warning.is_none())
        .filter(|binding| filter.is_empty() || help_binding_matches(binding, &filter))
        .collect();
    bindings.sort_by(|left, right| {
        left.section
            .cmp(&right.section)
            .then(left.action.cmp(&right.action))
            .then(left.key.cmp(&right.key))
    });
    bindings
}

pub fn help_binding_matches(binding: &HelpBinding, filter: &str) -> bool {
    let filter = filter.trim().to_lowercase();
    if filter.is_empty() {
        return true;
    }
    [
        binding.key.as_str(),
        binding.action.as_str(),
        binding.action_id.as_str(),
        binding.section.title(),
        binding.scope.as_str(),
        binding.description.as_str(),
    ]
    .iter()
    .any(|value| value.to_lowercase().contains(&filter))
}

fn visible_sections_from_bindings(bindings: &[&HelpBinding]) -> Vec<HelpBindingSection> {
    bindings
        .iter()
        .map(|binding| binding.section)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn section_order() -> [HelpBindingSection; 10] {
    [
        HelpBindingSection::Movement,
        HelpBindingSection::ClickDrag,
        HelpBindingSection::Wheel,
        HelpBindingSection::JumpGrid,
        HelpBindingSection::Bookmarks,
        HelpBindingSection::UiHints,
        HelpBindingSection::StepMove,
        HelpBindingSection::Surgical,
        HelpBindingSection::Help,
        HelpBindingSection::ProfilesRuntime,
    ]
}

fn view_format_config(view: &HelpOverlayView) -> TooltipOverlayConfig {
    TooltipOverlayConfig {
        help_max_bindings: view.help_max_bindings,
        help: view.help_config.clone(),
        ..TooltipOverlayConfig::default()
    }
}

pub fn apply_help_input(view: &mut HelpOverlayView, input: HelpInput) -> bool {
    if matches!(input, HelpInput::Escape) {
        view.state = HelpState::Hidden;
        return true;
    }
    if matches!(view.state, HelpState::Hidden) {
        view.state = HelpState::Visible {
            filter: String::new(),
            section: None,
            page: 0,
        };
    }
    let (mut filter, mut section, mut page) = match &view.state {
        HelpState::Visible {
            filter,
            section,
            page,
        } => (filter.clone(), *section, *page),
        HelpState::Hidden => unreachable!(),
    };

    match input {
        HelpInput::Search => {
            page = 0;
        }
        HelpInput::NextSection | HelpInput::PreviousSection => {
            let filtered = filtered_bindings(view, &view.state, &view.help_config);
            let sections = visible_sections_from_bindings(&filtered);
            if sections.is_empty() {
                section = None;
            } else {
                let current = section
                    .and_then(|s| sections.iter().position(|candidate| *candidate == s))
                    .unwrap_or(0);
                let next = match input {
                    HelpInput::NextSection => (current + 1) % sections.len(),
                    _ => (current + sections.len() - 1) % sections.len(),
                };
                section = Some(sections[next]);
            }
            page = 0;
        }
        HelpInput::NextPage | HelpInput::PreviousPage => {
            let filtered = filtered_bindings(view, &view.state, &view.help_config);
            let sections = visible_sections_from_bindings(&filtered);
            let selected = section.filter(|s| sections.contains(s));
            let count = filtered
                .into_iter()
                .filter(|binding| selected.is_none_or(|s| binding.section == s))
                .count();
            let page_count = count
                .max(1)
                .div_ceil(view.help_config.page_size.max(1) as usize)
                .max(1);
            page = match input {
                HelpInput::NextPage => (page + 1).min(page_count - 1),
                _ => page.saturating_sub(1),
            };
        }
        HelpInput::Append(ch) => {
            filter.push(ch);
            page = 0;
        }
        HelpInput::Backspace => {
            filter.pop();
            page = 0;
        }
        HelpInput::Escape => unreachable!(),
    }
    view.state = HelpState::Visible {
        filter,
        section,
        page,
    };
    true
}

fn overlay_position(size: (i32, i32)) -> (i32, i32) {
    unsafe {
        let mut point = POINT::default();
        if GetCursorPos(&mut point).is_err() {
            return (80, 80);
        }

        let screen_left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let screen_top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let screen_width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let screen_height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        let max_x = (screen_left + screen_width - size.0).max(screen_left);
        let max_y = (screen_top + screen_height - size.1).max(screen_top);
        (
            (point.x + CURSOR_OFFSET_X).clamp(screen_left, max_x),
            (point.y + CURSOR_OFFSET_Y).clamp(screen_top, max_y),
        )
    }
}

fn draw_title(hdc: HDC, x: i32, y: i32, text: &str) {
    unsafe {
        let old_text_color = SetTextColor(hdc, RGB(255, 255, 255));
        let text: Vec<u16> = text.encode_utf16().collect();
        let _ = TextOutW(hdc, x, y, &text);
        let _ = SetTextColor(hdc, old_text_color);
    }
}

fn draw_body_line(hdc: HDC, x: i32, y: i32, text: &str) {
    unsafe {
        let old_text_color = SetTextColor(hdc, RGB(218, 224, 235));
        let text: Vec<u16> = text.encode_utf16().collect();
        let _ = TextOutW(hdc, x, y, &text);
        let _ = SetTextColor(hdc, old_text_color);
    }
}

#[allow(non_snake_case)]
fn RGB(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(((b as u32) << 16) | ((g as u32) << 8) | (r as u32))
}

extern "system" fn help_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let ps = &mut PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, ps) };
            HELP_OVERLAY.with(|overlay| overlay.borrow().draw(hdc));
            unsafe {
                let _ = EndPaint(hwnd, ps);
            }
            LRESULT(0)
        }
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_DESTROY => LRESULT(0),
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

pub fn help_view_from_bindings<I>(bindings: I, mode: ModeContext) -> HelpOverlayView
where
    I: IntoIterator<Item = (KeyChord, Action)>,
{
    let mut bindings: Vec<HelpBinding> = bindings
        .into_iter()
        .filter(|(_, action)| mode_includes_action(mode, action))
        .map(|(chord, action)| help_binding(format_key_chord(chord), action, true, None))
        .collect();
    bindings.sort_by(|left, right| {
        left.section
            .cmp(&right.section)
            .then(left.action.cmp(&right.action))
            .then(left.key.cmp(&right.key))
    });
    HelpOverlayView {
        stats: HelpRuntimeStats::default(),
        bindings,
        config_warnings: Vec::new(),
        help_max_bindings: TooltipOverlayConfig::default().help_max_bindings,
        help_config: TooltipOverlayHelpConfig::default(),
        state: HelpState::Visible {
            filter: String::new(),
            section: None,
            page: 0,
        },
    }
}

pub fn show_temporary_tooltip(
    title: impl Into<String>,
    body: impl Into<String>,
    duration: Duration,
) {
    show_temporary_tooltip_at(title, body, duration, Instant::now());
}

pub fn show_temporary_tooltip_at(
    title: impl Into<String>,
    body: impl Into<String>,
    duration: Duration,
    now: Instant,
) {
    HELP_OVERLAY.with(|overlay| {
        overlay
            .borrow_mut()
            .show_temporary_tooltip(title, body, duration, now)
    });
}

pub fn show_help_overlay(view: HelpOverlayView) {
    HELP_OVERLAY.with(|overlay| overlay.borrow_mut().show_help_overlay(view));
}

pub fn hide_help_overlay() {
    HELP_OVERLAY.with(|overlay| overlay.borrow_mut().hide());
}

pub fn handle_help_input(input: HelpInput) -> bool {
    HELP_OVERLAY.with(|overlay| overlay.borrow_mut().handle_help_input(input))
}

pub fn update_overlay(now: Instant) {
    HELP_OVERLAY.with(|overlay| overlay.borrow_mut().update_overlay(now));
}

fn help_binding(
    key: String,
    action: Action,
    bound: bool,
    warning: Option<HelpBindingWarning>,
) -> HelpBinding {
    HelpBinding {
        key,
        action: format_action(&action),
        action_id: action_id(&action),
        section: action_section(&action),
        scope: action_scope(&action).to_string(),
        description: action_description(&action),
        warning,
        bound,
    }
}

pub fn append_unbound_actions(view: &mut HelpOverlayView) {
    let bound_ids: HashSet<String> = view
        .bindings
        .iter()
        .map(|binding| binding.action_id.clone())
        .collect();
    for action in all_help_actions() {
        let id = action_id(&action);
        if !bound_ids.contains(&id) {
            view.bindings
                .push(help_binding("—".to_string(), action, false, None));
        }
    }
    view.bindings.sort_by(|left, right| {
        left.section
            .cmp(&right.section)
            .then(left.action.cmp(&right.action))
            .then(left.key.cmp(&right.key))
    });
}

fn all_help_actions() -> Vec<Action> {
    vec![
        Action::MoveUp,
        Action::MoveDown,
        Action::MoveLeft,
        Action::MoveRight,
        Action::MoveUpRight,
        Action::MoveUpLeft,
        Action::MoveDownRight,
        Action::MoveDownLeft,
        Action::LeftClick,
        Action::RightClick,
        Action::MiddleClick,
        Action::ClickThenDisable,
        Action::ToggleDragMode,
        Action::WheelUp,
        Action::WheelDown,
        Action::WheelLeft,
        Action::WheelRight,
        Action::WheelSpeedUp,
        Action::WheelSpeedDown,
        Action::WheelSpeedReset,
        Action::MouseSpeedUp,
        Action::MouseSpeedDown,
        Action::MouseSpeedReset,
        Action::JumpMode,
        Action::GridMode,
        Action::ScreenSelect,
        Action::UiHintMode,
        Action::BookmarkMode,
        Action::ShowBookmarks,
        Action::ReloadConfig,
        Action::PanicReset,
        Action::Disable,
        Action::ShowHelp,
        Action::HelpSearch,
        Action::HelpNextSection,
        Action::HelpPreviousSection,
        Action::HelpNextPage,
        Action::HelpPreviousPage,
    ]
}

fn action_id(action: &Action) -> String {
    match action {
        Action::StepMove { direction, tier } => {
            let dir = match direction {
                Direction2D::Up => "up",
                Direction2D::Down => "down",
                Direction2D::Left => "left",
                Direction2D::Right => "right",
            };
            let tier = match tier {
                StepMoveTier::Normal => "",
                StepMoveTier::Small => "small_",
                StepMoveTier::Large => "large_",
            };
            format!("step_move_{tier}{dir}")
        }
        Action::WheelProfileSelect(profile) => format!("wheel_profile:{profile}"),
        Action::MovementProfileSelect(profile) => format!("movement_profile:{profile}"),
        Action::JumpModeProfile(profile) => format!("jump_mode:{profile}"),
        Action::BookmarkSlot(slot) => format!("bookmark_slot_{slot}"),
        Action::ClearBookmarkSlot(slot) => format!("clear_bookmark_{slot}"),
        other => debug_action_id(other),
    }
}

fn debug_action_id(action: &Action) -> String {
    let raw = format!("{action:?}");
    let mut output = String::new();
    for (idx, ch) in raw.chars().enumerate() {
        if ch == ' ' || ch == '{' || ch == '(' {
            break;
        }
        if ch.is_ascii_uppercase() {
            if idx > 0 {
                output.push('_');
            }
            output.push(ch.to_ascii_lowercase());
        } else {
            output.push(ch);
        }
    }
    output
}

fn action_scope(action: &Action) -> &'static str {
    match action {
        Action::NavigateBack | Action::NavigateForward => "jump/grid",
        Action::BookmarkMode
        | Action::ShowBookmarks
        | Action::BookmarkSlot(_)
        | Action::ClearBookmarkSlot(_)
        | Action::ClearAllBookmarks => "bookmarks",
        Action::UiHintMode => "ui-hints",
        Action::HelpMode
        | Action::HelpSearch
        | Action::HelpNextSection
        | Action::HelpPreviousSection
        | Action::HelpNextPage
        | Action::HelpPreviousPage => "help",
        _ => "global",
    }
}

fn action_description(action: &Action) -> String {
    match action {
        Action::HelpSearch => "Focus or edit the interactive help search filter".to_string(),
        Action::HelpNextSection => "Move to the next help section".to_string(),
        Action::HelpPreviousSection => "Move to the previous help section".to_string(),
        Action::HelpNextPage => "Show the next help page without dropping results".to_string(),
        Action::HelpPreviousPage => "Show the previous help page".to_string(),
        Action::HelpMode | Action::ShowHelp => {
            "Open or close this searchable help overlay".to_string()
        }
        Action::StepMove { .. } => "Move once by the configured step size".to_string(),
        _ => format_action(action),
    }
}

fn format_action(action: &Action) -> String {
    match action {
        Action::MoveUp => "Move up".to_string(),
        Action::MoveDown => "Move down".to_string(),
        Action::MoveLeft => "Move left".to_string(),
        Action::MoveRight => "Move right".to_string(),
        Action::MoveUpRight => "Move up-right".to_string(),
        Action::MoveUpLeft => "Move up-left".to_string(),
        Action::MoveDownRight => "Move down-right".to_string(),
        Action::MoveDownLeft => "Move down-left".to_string(),
        Action::MoveToTopEdge => "Jump to monitor top edge".to_string(),
        Action::MoveToBottomEdge => "Jump to monitor bottom edge".to_string(),
        Action::MoveToLeftEdge => "Jump to monitor left edge".to_string(),
        Action::MoveToRightEdge => "Jump to monitor right edge".to_string(),
        Action::MoveToWindowTopEdge => "Jump to active-window top edge".to_string(),
        Action::MoveToWindowBottomEdge => "Jump to active-window bottom edge".to_string(),
        Action::MoveToWindowLeftEdge => "Jump to active-window left edge".to_string(),
        Action::MoveToWindowRightEdge => "Jump to active-window right edge".to_string(),
        Action::MoveToWindowCenter => "Jump to active-window center".to_string(),
        Action::MoveToWindowTitlebar => "Jump to active-window titlebar".to_string(),
        Action::CenterCurrentMonitor => "Center current monitor".to_string(),
        Action::LeftClick => "Left click".to_string(),
        Action::RightClick => "Right click".to_string(),
        Action::MiddleClick => "Middle click".to_string(),
        Action::ClickThenDisable => "Click then disable".to_string(),
        Action::ToggleDragMode => "Toggle drag mode".to_string(),
        Action::WheelUp => "Wheel up".to_string(),
        Action::WheelDown => "Wheel down".to_string(),
        Action::WheelLeft => "Wheel left".to_string(),
        Action::WheelRight => "Wheel right".to_string(),
        Action::WheelSpeedUp => "Wheel level up".to_string(),
        Action::WheelSpeedDown => "Wheel level down".to_string(),
        Action::WheelSpeedReset => "Reset wheel level".to_string(),
        Action::WheelProfileNext => "Next wheel profile".to_string(),
        Action::WheelProfilePrevious => "Previous wheel profile".to_string(),
        Action::WheelProfileSelect(profile) => format!("Wheel profile: {profile}"),
        Action::MouseSpeedUp => "Mouse speed up".to_string(),
        Action::MouseSpeedDown => "Mouse speed down".to_string(),
        Action::MouseSpeedReset => "Reset mouse speed".to_string(),
        Action::MovementProfileNext => "Next movement profile".to_string(),
        Action::MovementProfilePrevious => "Previous movement profile".to_string(),
        Action::MovementProfileSelect(profile) => format!("Movement profile: {profile}"),
        Action::Exit => "Exit".to_string(),
        Action::ReloadConfig => "Reload config".to_string(),
        Action::PanicReset => "Panic reset".to_string(),
        Action::SlowMouse => "Slow mouse".to_string(),
        Action::SurgicalMode => "Held precision modifier (surgical)".to_string(),
        Action::ScrollModifier => "Scroll modifier".to_string(),
        Action::JumpMode => "Jump".to_string(),
        Action::JumpModeProfile(profile) => format!("Jump mode profile: {profile}"),
        Action::GridMode => "Grid".to_string(),
        Action::ScreenSelect => "Screen select".to_string(),
        Action::NavigateBack => "Navigate back".to_string(),
        Action::NavigateForward => "Navigate forward".to_string(),
        Action::Disable => "Disable".to_string(),
        Action::ShowHelp | Action::HelpMode => "Hints / Help".to_string(),
        Action::HelpSearch => "Help search".to_string(),
        Action::HelpNextSection => "Help next section".to_string(),
        Action::HelpPreviousSection => "Help previous section".to_string(),
        Action::HelpNextPage => "Help next page".to_string(),
        Action::HelpPreviousPage => "Help previous page".to_string(),
        Action::UiHintMode => "UI Hints".to_string(),
        Action::BookmarkMode => "Bookmark mode".to_string(),
        Action::ShowBookmarks => "Show bookmark list".to_string(),
        Action::BookmarkSlot(slot) => format!("Jump to bookmark slot {slot}"),
        Action::ClearBookmarkSlot(slot) => format!("Clear bookmark slot {slot}"),
        Action::ClearAllBookmarks => "Clear all bookmarks".to_string(),
        Action::StepMove { direction, tier } => {
            let direction = match direction {
                Direction2D::Up => "up",
                Direction2D::Down => "down",
                Direction2D::Left => "left",
                Direction2D::Right => "right",
            };
            match tier {
                StepMoveTier::Normal => format!("Step move {direction}"),
                StepMoveTier::Small => format!("Step move small {direction}"),
                StepMoveTier::Large => format!("Step move large {direction}"),
            }
        }
    }
}

fn action_section(action: &Action) -> HelpBindingSection {
    match action {
        Action::MoveUp
        | Action::MoveDown
        | Action::MoveLeft
        | Action::MoveRight
        | Action::MoveUpRight
        | Action::MoveUpLeft
        | Action::MoveDownRight
        | Action::MoveDownLeft
        | Action::MouseSpeedUp
        | Action::MouseSpeedDown
        | Action::MouseSpeedReset
        | Action::MovementProfileNext
        | Action::MovementProfilePrevious
        | Action::MovementProfileSelect(_)
        | Action::SlowMouse => HelpBindingSection::Movement,
        Action::SurgicalMode | Action::ScrollModifier => HelpBindingSection::Surgical,
        Action::StepMove { .. } => HelpBindingSection::StepMove,
        Action::LeftClick
        | Action::RightClick
        | Action::MiddleClick
        | Action::ClickThenDisable
        | Action::ToggleDragMode => HelpBindingSection::ClickDrag,
        Action::WheelUp
        | Action::WheelDown
        | Action::WheelLeft
        | Action::WheelRight
        | Action::WheelSpeedUp
        | Action::WheelSpeedDown
        | Action::WheelSpeedReset
        | Action::WheelProfileNext
        | Action::WheelProfilePrevious
        | Action::WheelProfileSelect(_) => HelpBindingSection::Wheel,
        Action::MoveToTopEdge
        | Action::MoveToBottomEdge
        | Action::MoveToLeftEdge
        | Action::MoveToRightEdge
        | Action::MoveToWindowTopEdge
        | Action::MoveToWindowBottomEdge
        | Action::MoveToWindowLeftEdge
        | Action::MoveToWindowRightEdge
        | Action::MoveToWindowCenter
        | Action::MoveToWindowTitlebar
        | Action::CenterCurrentMonitor
        | Action::JumpMode
        | Action::JumpModeProfile(_)
        | Action::GridMode
        | Action::ScreenSelect
        | Action::NavigateBack
        | Action::NavigateForward
        | Action::UiHintMode => HelpBindingSection::JumpGrid,
        Action::BookmarkMode
        | Action::ShowBookmarks
        | Action::BookmarkSlot(_)
        | Action::ClearBookmarkSlot(_)
        | Action::ClearAllBookmarks => HelpBindingSection::Bookmarks,
        Action::Exit
        | Action::ReloadConfig
        | Action::PanicReset
        | Action::Disable
        | Action::ShowHelp => HelpBindingSection::ProfilesRuntime,
        Action::HelpMode
        | Action::HelpSearch
        | Action::HelpNextSection
        | Action::HelpPreviousSection
        | Action::HelpNextPage
        | Action::HelpPreviousPage => HelpBindingSection::Help,
    }
}

fn mode_includes_action(mode: ModeContext, action: &Action) -> bool {
    match mode {
        ModeContext::Inactive | ModeContext::Active => true,
        ModeContext::Jump | ModeContext::Grid => matches!(
            action,
            Action::NavigateBack
                | Action::Disable
                | Action::ShowHelp
                | Action::HelpMode
                | Action::HelpSearch
                | Action::HelpNextSection
                | Action::HelpPreviousSection
                | Action::HelpNextPage
                | Action::HelpPreviousPage
                | Action::JumpMode
                | Action::GridMode
        ),
        ModeContext::UiHintQuerying | ModeContext::UiHintActive => {
            matches!(
                action,
                Action::NavigateBack
                    | Action::Disable
                    | Action::ShowHelp
                    | Action::HelpMode
                    | Action::HelpSearch
                    | Action::HelpNextSection
                    | Action::HelpPreviousSection
                    | Action::HelpNextPage
                    | Action::HelpPreviousPage
                    | Action::UiHintMode
            )
        }
        ModeContext::Bookmark => matches!(
            action,
            Action::BookmarkMode
                | Action::ShowBookmarks
                | Action::BookmarkSlot(_)
                | Action::ClearBookmarkSlot(_)
                | Action::ClearAllBookmarks
                | Action::Disable
                | Action::ShowHelp
                | Action::HelpMode
                | Action::HelpSearch
                | Action::HelpNextSection
                | Action::HelpPreviousSection
                | Action::HelpNextPage
                | Action::HelpPreviousPage
        ),
    }
}

fn format_key_chord(chord: KeyChord) -> String {
    chord.display_label()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::VirtualKey;

    fn test_binding(key: &str, action: &str, section: HelpBindingSection) -> HelpBinding {
        HelpBinding {
            key: key.to_string(),
            action: action.to_string(),
            action_id: action.to_lowercase().replace(' ', "_"),
            section,
            scope: "global".to_string(),
            description: action.to_string(),
            warning: None,
            bound: true,
        }
    }

    fn sample_view() -> HelpOverlayView {
        HelpOverlayView {
            stats: HelpRuntimeStats::default(),
            bindings: vec![test_binding(
                "H",
                "Hints / Help",
                HelpBindingSection::ProfilesRuntime,
            )],
            config_warnings: Vec::new(),
            help_max_bindings: TooltipOverlayConfig::default().help_max_bindings,
            help_config: TooltipOverlayHelpConfig::default(),
            state: HelpState::Visible {
                filter: String::new(),
                section: None,
                page: 0,
            },
        }
    }

    #[test]
    fn format_action_uses_ui_labels_for_mode_navigation_and_help_actions() {
        let cases = [
            (Action::JumpMode, "Jump"),
            (Action::GridMode, "Grid"),
            (Action::ScreenSelect, "Screen select"),
            (Action::NavigateBack, "Navigate back"),
            (Action::NavigateForward, "Navigate forward"),
            (Action::Disable, "Disable"),
            (Action::ShowHelp, "Hints / Help"),
        ];

        for (action, expected) in cases {
            let label = format_action(&action);
            assert_eq!(label, expected);
        }

        for (action, raw_name) in [
            (Action::JumpMode, "JumpMode"),
            (Action::GridMode, "GridMode"),
            (Action::ScreenSelect, "ScreenSelect"),
            (Action::NavigateBack, "NavigateBack"),
            (Action::NavigateForward, "NavigateForward"),
            (Action::ShowHelp, "ShowHelp"),
        ] {
            assert_ne!(format_action(&action), raw_name);
        }
    }

    #[test]
    fn help_contents_are_generated_from_keybindings() {
        let view = help_view_from_bindings(
            [
                (KeyChord::from_key(VirtualKey::F), Action::JumpMode),
                (KeyChord::from_key(VirtualKey::H), Action::ShowHelp),
            ],
            ModeContext::Active,
        );

        assert!(view
            .bindings
            .iter()
            .any(|binding| { binding.key == "F" && binding.action == "Jump" }));
        assert!(view
            .bindings
            .iter()
            .any(|binding| { binding.key == "H" && binding.action == "Hints / Help" }));
        assert_eq!(view.stats, HelpRuntimeStats::default());
    }

    #[test]
    fn help_contents_use_canonical_parseable_chord_labels() {
        let view = help_view_from_bindings(
            [(KeyChord::parse("Meta+Period").unwrap(), Action::ShowHelp)],
            ModeContext::Active,
        );
        let binding = view
            .bindings
            .iter()
            .find(|binding| binding.action == "Hints / Help")
            .expect("expected ShowHelp binding");
        assert_eq!(binding.key, "Win+Period");
        assert_eq!(
            KeyChord::parse(&binding.key).unwrap(),
            KeyChord::parse("Win+Period").unwrap()
        );
    }

    #[test]
    fn mode_context_maps_to_expected_help_sections() {
        assert_eq!(
            resolve_help_section(ModeContext::Inactive),
            HelpSection::Disabled
        );
        assert_eq!(
            resolve_help_section(ModeContext::Active),
            HelpSection::General
        );
        assert_eq!(resolve_help_section(ModeContext::Jump), HelpSection::Jump);
        assert_eq!(resolve_help_section(ModeContext::Grid), HelpSection::Grid);
        assert_eq!(
            resolve_help_section(ModeContext::UiHintQuerying),
            HelpSection::UiHints
        );
        assert_eq!(
            resolve_help_section(ModeContext::UiHintActive),
            HelpSection::UiHints
        );
    }

    #[test]
    fn help_lines_include_formatted_action_labels() {
        let view = help_view_from_bindings(
            [
                (KeyChord::from_key(VirtualKey::F), Action::JumpMode),
                (KeyChord::from_key(VirtualKey::G), Action::GridMode),
                (KeyChord::from_key(VirtualKey::S), Action::ScreenSelect),
                (KeyChord::from_key(VirtualKey::H), Action::ShowHelp),
            ],
            ModeContext::Active,
        );

        let lines = format_help_lines(&view, TooltipOverlayConfig::default()).join("\n");

        for expected in [
            "F  -  Jump",
            "G  -  Grid",
            "S  -  Screen select",
            "H  -  Hints / Help",
        ] {
            assert!(lines.contains(expected), "{expected}");
        }
        for raw_name in ["JumpMode", "GridMode", "ScreenSelect", "ShowHelp"] {
            assert!(!lines.contains(raw_name), "{raw_name}");
        }
    }

    #[test]
    fn help_content_size_includes_runtime_stats() {
        let size = content_size(&HelpOverlayContent::Help {
            view: Box::new(sample_view()),
        });

        assert!(size.1 > PADDING_Y * 2 + LINE_HEIGHT + TITLE_BODY_GAP + LINE_HEIGHT);
    }

    #[test]
    fn format_help_lines_include_runtime_flags_and_speed_settings() {
        let mut view = sample_view();
        view.stats = HelpRuntimeStats {
            app_mode: "active".to_string(),
            drag_active: true,
            slow_active: true,
            jump_active: true,
            movement_profile: "fast".to_string(),
            wheel_profile: "precise".to_string(),
            mouse_speed: HelpSpeedTier {
                current: 7,
                default: 5,
                min: 2,
                max: 12,
                step: 2,
            },
            wheel_speed: HelpSpeedTier {
                current: 4,
                default: 3,
                min: 1,
                max: 9,
                step: 1,
            },
            acceleration: 3,
            acceleration_rate: 2,
            slow_strategy: "fixed".to_string(),
            slow_effective_speed: 1,
            slow_min_speed: 1,
            slow_max_speed: 2,
            slow_acceleration: 0,
            slow_acceleration_rate: 1,
            top_speed: 15,
            polling_rate_ms: 8,
            wheel_tick_interval_ms: 12,
            wheel_vertical_multiplier: 2,
            wheel_horizontal_multiplier: 4,
            mode_context: "general".to_string(),
        };

        let lines = format_help_lines(&view, TooltipOverlayConfig::default()).join("\n");

        assert!(lines.contains("Mode: active | Drag: on | Slow: on | Jump: on"));
        assert!(lines
            .contains("Movement profile: fast | Level 7 / 12 (default 5, range 2..12, step 2)"));
        assert!(lines.contains(
            "Wheel profile: precise | Level 4 / 9 (default 3, range 1..9, step 1) | Repeat 12ms"
        ));
        assert!(
            lines.contains("Slow: fixed | Speed: 1 (range 1..2) | Acceleration: 0 every 1 tick(s)")
        );
        assert!(lines.contains(
            "Acceleration: 3 every 2 tick(s) | Top speed: 15 | Polling: 8ms | Wheel tick: 12ms"
        ));
        assert!(lines.contains("Wheel multipliers: vertical 2 | horizontal 4"));
    }

    #[test]
    fn format_help_lines_group_bindings_and_custom_actions() {
        let view = HelpOverlayView {
            stats: HelpRuntimeStats::default(),
            bindings: vec![
                test_binding("C", "My custom action", HelpBindingSection::ProfilesRuntime),
                test_binding("W", "Move up", HelpBindingSection::Movement),
                test_binding("L", "Left click", HelpBindingSection::ClickDrag),
                test_binding("J", "Jump", HelpBindingSection::JumpGrid),
                test_binding("U", "Wheel up", HelpBindingSection::Wheel),
                test_binding("H", "Hints / Help", HelpBindingSection::ProfilesRuntime),
            ],
            config_warnings: Vec::new(),
            help_max_bindings: 40,
            help_config: TooltipOverlayHelpConfig::default(),
            state: HelpState::Visible {
                filter: String::new(),
                section: None,
                page: 0,
            },
        };

        let lines = format_help_lines(&view, TooltipOverlayConfig::default()).join("\n");

        for heading in [
            "Movement:",
            "Click/Drag:",
            "Wheel:",
            "Jump/Grid:",
            "Profiles/Runtime:",
        ] {
            assert!(lines.contains(heading), "{heading}");
        }
        assert!(lines.contains("C  -  My custom action"));
    }

    #[test]
    fn format_help_lines_pages_bindings_deterministically() {
        let mut config = TooltipOverlayConfig::default();
        config.help.page_size = 2;
        let view = help_view_from_bindings(
            [
                (KeyChord::from_key(VirtualKey::H), Action::ShowHelp),
                (KeyChord::from_key(VirtualKey::W), Action::MoveUp),
                (KeyChord::from_key(VirtualKey::A), Action::MoveLeft),
                (KeyChord::from_key(VirtualKey::D), Action::MoveRight),
            ],
            ModeContext::Active,
        );

        let lines = format_help_lines(&view, config).join("\n");

        assert!(lines.contains("A  -  Move left"));
        assert!(lines.contains("D  -  Move right"));
        assert!(!lines.contains("... 2 more binding(s)"));
        assert!(lines.contains("Page 1/"));
    }

    #[test]
    fn help_model_is_mode_sensitive() {
        let bindings = [
            (KeyChord::from_key(VirtualKey::J), Action::JumpMode),
            (KeyChord::from_key(VirtualKey::U), Action::UiHintMode),
            (KeyChord::from_key(VirtualKey::Escape), Action::Disable),
            (KeyChord::from_key(VirtualKey::W), Action::MoveUp),
        ];
        let normal = help_view_from_bindings(bindings.clone(), ModeContext::Active);
        let jump = help_view_from_bindings(bindings.clone(), ModeContext::Jump);
        let ui_hint = help_view_from_bindings(bindings, ModeContext::UiHintActive);
        assert!(normal.bindings.iter().any(|b| b.action == "Move up"));
        assert!(!jump.bindings.iter().any(|b| b.action == "Move up"));
        assert!(jump.bindings.iter().any(|b| b.action == "Disable"));
        assert!(ui_hint.bindings.iter().any(|b| b.action == "UI Hints"));
    }

    #[test]
    fn help_section_order_stable() {
        let view = help_view_from_bindings(
            [
                (KeyChord::from_key(VirtualKey::W), Action::MoveUp),
                (KeyChord::from_key(VirtualKey::L), Action::LeftClick),
                (KeyChord::from_key(VirtualKey::U), Action::WheelUp),
                (KeyChord::from_key(VirtualKey::J), Action::JumpMode),
                (KeyChord::from_key(VirtualKey::H), Action::ShowHelp),
            ],
            ModeContext::Active,
        );
        let lines = format_help_lines(&view, TooltipOverlayConfig::default()).join("\n");
        assert!(lines.find("Movement:").unwrap() < lines.find("Click/Drag:").unwrap());
        assert!(lines.find("Click/Drag:").unwrap() < lines.find("Wheel:").unwrap());
        assert!(lines.find("Wheel:").unwrap() < lines.find("Jump/Grid:").unwrap());
    }

    #[test]
    fn bookmark_entries_appear_in_help_bindings() {
        let view = help_view_from_bindings(
            [
                (KeyChord::parse("Shift+B").unwrap(), Action::BookmarkMode),
                (
                    KeyChord::from_key(VirtualKey::Num1),
                    Action::BookmarkSlot(1),
                ),
                (
                    KeyChord::from_key(VirtualKey::Backspace),
                    Action::ClearBookmarkSlot(1),
                ),
                (KeyChord::from_key(VirtualKey::Escape), Action::Disable),
            ],
            ModeContext::Active,
        );

        let lines = format_help_lines(&view, TooltipOverlayConfig::default()).join("\n");
        assert!(lines.contains("Bookmarks:"));
        assert!(lines.contains("Shift+B  -  Bookmark mode"));
        assert!(lines.contains("Num1  -  Jump to bookmark slot 1"));
        assert!(lines.contains("Backspace  -  Clear bookmark slot 1"));
    }

    #[test]
    fn overlay_warning_formatter_includes_path_and_severity() {
        let warning = OverlayConfigWarning {
            path: "jump.coarse.preview_margin_percent".to_string(),
            severity: OverlayWarningSeverity::Deprecated,
            message: "legacy field is ignored".to_string(),
            fix_path: "Use jump.coarse.visual_context_margin_percent.".to_string(),
        };

        let lines = format_config_warning_lines(&[warning], 3).join("\n");

        assert!(lines.contains("[DEPR]"));
        assert!(lines.contains("jump.coarse.preview_margin_percent"));
        assert!(lines.contains("Fix: Use jump.coarse.visual_context_margin_percent."));
    }

    #[test]
    fn duplicate_warning_collapse_behavior() {
        let duplicate = OverlayConfigWarning {
            path: "wheel.min_speed".to_string(),
            severity: OverlayWarningSeverity::Warning,
            message: "wheel.min_speed is below 1".to_string(),
            fix_path: "Review this config value and update config.toml.".to_string(),
        };
        let distinct = OverlayConfigWarning {
            path: "wheel.vertical_multiplier".to_string(),
            severity: OverlayWarningSeverity::Warning,
            message: "wheel.vertical_multiplier is below 1".to_string(),
            fix_path: "Review this config value and update config.toml.".to_string(),
        };

        let collapsed =
            collapsed_config_warnings(&[duplicate.clone(), distinct.clone(), duplicate.clone()], 3);

        assert_eq!(collapsed, vec![distinct, duplicate]);
    }

    #[test]
    fn tooltip_formatting_uses_kind_specific_title_and_body() {
        let stats = HelpRuntimeStats {
            drag_active: true,
            movement_profile: "fast".to_string(),
            wheel_profile: "precise".to_string(),
            mouse_speed: HelpSpeedTier {
                current: 6,
                default: 3,
                min: 1,
                max: 9,
                step: 1,
            },
            wheel_speed: HelpSpeedTier {
                current: 5,
                default: 4,
                min: 2,
                max: 8,
                step: 1,
            },
            ..HelpRuntimeStats::default()
        };
        let cases = [
            (
                RuntimeNotificationKind::MouseSpeed,
                "Mouse level",
                "Level 6 / 9",
            ),
            (
                RuntimeNotificationKind::WheelSpeed,
                "Wheel level",
                "Level 5 / 8",
            ),
            (
                RuntimeNotificationKind::MovementProfile,
                "Movement profile",
                "fast",
            ),
            (
                RuntimeNotificationKind::WheelProfile,
                "Wheel profile",
                "precise | Level 5 / 8 | Repeat 0ms",
            ),
            (RuntimeNotificationKind::Drag, "Drag", "held"),
            (
                RuntimeNotificationKind::ConfigReload,
                "Config reloaded",
                "updated",
            ),
            (
                RuntimeNotificationKind::PanicReset,
                "Panic reset",
                "restored",
            ),
        ];

        for (kind, title, body) in cases {
            let notification = RuntimeNotification {
                kind,
                title: "raw title".to_string(),
                body: "raw body".to_string(),
                duration_ms: 1,
            };
            let message = format_tooltip_message(&notification, &stats);

            assert!(message.title.contains(title));
            assert!(message.body.contains(body));
        }
    }

    #[test]
    fn tooltip_drag_body_changes_for_on_and_off() {
        let notification = RuntimeNotification {
            kind: RuntimeNotificationKind::Drag,
            title: String::new(),
            body: String::new(),
            duration_ms: 1,
        };
        let on = format_tooltip_message(
            &notification,
            &HelpRuntimeStats {
                drag_active: true,
                ..HelpRuntimeStats::default()
            },
        );
        let off = format_tooltip_message(
            &notification,
            &HelpRuntimeStats {
                drag_active: false,
                ..HelpRuntimeStats::default()
            },
        );

        assert!(on.body.contains("held"));
        assert!(off.body.contains("released"));
    }

    #[test]
    fn help_filter_matches_action_name() {
        let binding = test_binding("H", "Hints / Help", HelpBindingSection::ProfilesRuntime);
        assert!(help_binding_matches(&binding, "hints"));
    }

    #[test]
    fn help_filter_matches_key_name() {
        let binding = test_binding("Ctrl+PageDown", "Help next page", HelpBindingSection::Help);
        assert!(help_binding_matches(&binding, "pagedown"));
    }

    #[test]
    fn help_filter_matches_description() {
        let mut binding = test_binding("/", "Help search", HelpBindingSection::Help);
        binding.description = "Focus or edit the interactive help search filter".to_string();
        assert!(help_binding_matches(&binding, "interactive help search"));
    }

    #[test]
    fn help_pages_do_not_drop_bindings() {
        let mut view = sample_view();
        view.help_config.page_size = 2;
        view.bindings = (0..5)
            .map(|idx| {
                test_binding(
                    &format!("K{idx}"),
                    &format!("Action {idx}"),
                    HelpBindingSection::Movement,
                )
            })
            .collect();

        let mut seen = HashSet::new();
        for page in 0..3 {
            view.state = HelpState::Visible {
                filter: String::new(),
                section: None,
                page,
            };
            let lines = format_help_lines(&view, TooltipOverlayConfig::default()).join("\n");
            for idx in 0..5 {
                if lines.contains(&format!("Action {idx}")) {
                    seen.insert(idx);
                }
            }
        }
        assert_eq!(seen.len(), 5);
    }

    #[test]
    fn help_groups_mode_specific_bindings() {
        let view = help_view_from_bindings(
            [
                (KeyChord::from_key(VirtualKey::H), Action::NavigateBack),
                (KeyChord::from_key(VirtualKey::W), Action::MoveUp),
            ],
            ModeContext::Jump,
        );
        let back = view
            .bindings
            .iter()
            .find(|binding| binding.action == "Navigate back")
            .unwrap();
        assert_eq!(back.scope, "jump/grid");
        assert!(!view
            .bindings
            .iter()
            .any(|binding| binding.action == "Move up"));
    }

    #[test]
    fn help_shows_conflict_warning_for_shadowed_key() {
        let mut view = sample_view();
        view.bindings = vec![HelpBinding {
            warning: Some(HelpBindingWarning {
                message: "Plain binding is shadowed by Shift+H".to_string(),
                severity: OverlayWarningSeverity::Warning,
                winning_key: Some("Shift+H".to_string()),
                winning_action_id: Some("help_search".to_string()),
            }),
            ..test_binding("H", "Hints / Help", HelpBindingSection::ProfilesRuntime)
        }];
        view.help_config.show_conflicts = true;
        let lines = format_help_lines(&view, TooltipOverlayConfig::default()).join("\n");
        assert!(lines.contains("WARN"));
        assert!(lines.contains("shadowed"));
    }

    #[test]
    fn style_contains_click_through_topmost_and_non_activate_bits() {
        let style = help_overlay_ex_style();

        assert!(style.contains(WS_EX_LAYERED));
        assert!(style.contains(WS_EX_TOPMOST));
        assert!(style.contains(WS_EX_TOOLWINDOW));
        assert!(style.contains(WS_EX_TRANSPARENT));
        assert!(style.contains(WS_EX_NOACTIVATE));
    }

    #[test]
    fn temporary_tooltip_multiline_body_uses_multiple_lines_for_size() {
        let msg = TemporaryMessage {
            title: "Bookmarks".to_string(),
            body: "1. A
2. B
3. C"
                .to_string(),
        };
        let content = HelpOverlayContent::Temporary {
            message: msg,
            expires_at: Instant::now() + Duration::from_secs(1),
        };
        let size = content_size(&content);
        let expected_height = PADDING_Y * 2 + LINE_HEIGHT + TITLE_BODY_GAP + 3 * LINE_HEIGHT;
        assert_eq!(size.1, expected_height.min(MAX_OVERLAY_HEIGHT));
    }
    #[test]
    fn temporary_tooltip_visible_before_expiry() {
        let now = Instant::now();
        let mut state = HelpOverlayState::new();

        state.show_temporary_tooltip("Speed", "Mouse speed 2", Duration::from_millis(700), now);
        state.update_overlay(now + Duration::from_millis(699));

        assert!(matches!(
            state.content,
            HelpOverlayContent::Temporary { .. }
        ));
    }

    #[test]
    fn temporary_tooltip_hidden_after_expiry() {
        let now = Instant::now();
        let mut state = HelpOverlayState::new();

        state.show_temporary_tooltip("Speed", "Mouse speed 2", Duration::from_millis(700), now);
        state.update_overlay(now + Duration::from_millis(700));

        assert_eq!(state.content, HelpOverlayContent::Hidden);
    }

    #[test]
    fn help_overlay_does_not_auto_expire() {
        let now = Instant::now();
        let mut state = HelpOverlayState::new();

        state.show_help_overlay(sample_view());
        state.update_overlay(now + Duration::from_secs(60));

        assert!(matches!(state.content, HelpOverlayContent::Help { .. }));
    }

    #[test]
    fn slash_help_replaces_temporary_content() {
        let now = Instant::now();
        let mut state = HelpOverlayState::new();
        let view = sample_view();

        state.show_temporary_tooltip("Speed", "Mouse speed 2", Duration::from_millis(700), now);
        state.show_help_overlay(view.clone());

        assert_eq!(
            state.content,
            HelpOverlayContent::Help {
                view: Box::new(view)
            }
        );
    }

    #[test]
    fn temporary_notifications_do_not_override_visible_help() {
        let now = Instant::now();
        let mut state = HelpOverlayState::new();
        let view = sample_view();

        state.show_help_overlay(view.clone());
        state.show_temporary_tooltip("Speed", "Mouse speed 2", Duration::from_millis(700), now);

        assert_eq!(
            state.content,
            HelpOverlayContent::Help {
                view: Box::new(view)
            }
        );
    }

    #[test]
    fn hide_clears_visible_temporary_tooltip() {
        let now = Instant::now();
        let mut state = HelpOverlayState::new();

        state.show_temporary_tooltip("Speed", "Mouse speed 2", Duration::from_millis(700), now);
        state.hide();

        assert_eq!(state.content, HelpOverlayContent::Hidden);
    }

    #[test]
    fn hide_clears_visible_help_overlay() {
        let mut state = HelpOverlayState::new();

        state.show_help_overlay(sample_view());
        state.hide();

        assert_eq!(state.content, HelpOverlayContent::Hidden);
    }
}
