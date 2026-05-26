use crate::indicator::{IndicatorFlashReason, IndicatorSnapshot, IndicatorState};
use serde::Deserialize;
use std::ptr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
#[cfg(debug_assertions)]
use std::time::Instant;
use windows::core::{w, Error};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOZORDER,
};
use windows::Win32::{
    Foundation::*, Graphics::Gdi::*, System::LibraryLoader::*, UI::WindowsAndMessaging::*,
};

lazy_static::lazy_static! {
    /// Global overlay instance wrapped in `Arc<Mutex<Option<OverlayWindow>>>`.
    ///
    /// Initialization may fail, in which case the value will be `None` and
    /// overlay features will be disabled.
    pub static ref OVERLAY: Arc<Mutex<Option<OverlayWindow>>> = Arc::new(Mutex::new(match OverlayWindow::new() {
        Ok(ov) => Some(ov),
        Err(e) => {
            eprintln!("Failed to initialize overlay: {e}");
            None
        }
    }));
}

#[cfg(debug_assertions)]
lazy_static::lazy_static! {
    static ref PAINT_SMOKE_LOG: Mutex<PaintSmokeLog> = Mutex::new(PaintSmokeLog::new());
}

#[derive(Clone)]
pub struct OverlayWindow {
    hwnd: Arc<Mutex<Option<isize>>>, // ✅ Store HWND as `isize`
    snapshot: IndicatorSnapshot,
    config: StatusOverlayConfig,
    visible: bool,
    last_cursor_position: Option<OverlayPosition>,
    last_visual_state: Option<OverlayVisualState>,
    #[cfg(debug_assertions)]
    debug_counters: Arc<Mutex<OverlayDebugCounters>>,
}

const OVERLAY_WIDTH: i32 = 25;
const OVERLAY_HEIGHT: i32 = 25;
const TEXT_OVERLAY_WIDTH: i32 = 150;
const TEXT_OVERLAY_HEIGHT: i32 = 25;
const CURSOR_OFFSET_X: i32 = 5;
const CURSOR_OFFSET_Y: i32 = 5;
const INDICATOR_SQUARE_SIZE: i32 = 25;
const PIP_SIZE: i32 = 4;
const PIP_GAP: i32 = 2;
const PIP_BOTTOM_MARGIN: i32 = 4;

fn overlay_ex_style() -> WINDOW_EX_STYLE {
    WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OverlayPosition {
    x: i32,
    y: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OverlayVisualState {
    snapshot_state: IndicatorState,
    text: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IndicatorVisual {
    base_color: COLORREF,
    pip_count: u8,
    draw_outline: bool,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StatusOverlayVisibility {
    #[default]
    Visible,
    Hidden,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StatusOverlayMode {
    #[default]
    Minimal,
    Compact,
    Detailed,
    Hidden,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StatusOverlayPositioning {
    #[default]
    Cursor,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(default)]
pub struct StatusOverlayFields {
    pub active: bool,
    pub drag: bool,
    pub slow: bool,
    pub surgical: bool,
    pub jump: bool,
    pub mouse_speed: bool,
    pub wheel_speed: bool,
    pub flash: bool,
    pub final_adjust: bool,
    pub bookmark_mode: bool,
}

impl Default for StatusOverlayFields {
    fn default() -> Self {
        Self {
            active: true,
            drag: true,
            slow: true,
            surgical: true,
            jump: true,
            mouse_speed: true,
            wheel_speed: true,
            flash: true,
            final_adjust: true,
            bookmark_mode: true,
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(default)]
pub struct StatusOverlayConfig {
    pub visibility: StatusOverlayVisibility,
    pub mode: StatusOverlayMode,
    pub positioning: StatusOverlayPositioning,
    pub fields: StatusOverlayFields,
    pub flash_duration_ms: u64,
}

impl Default for StatusOverlayConfig {
    fn default() -> Self {
        Self {
            visibility: StatusOverlayVisibility::Visible,
            mode: StatusOverlayMode::Minimal,
            positioning: StatusOverlayPositioning::Cursor,
            fields: StatusOverlayFields::default(),
            flash_duration_ms: 700,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OverlayRenderPlan {
    visible: bool,
    width: i32,
    height: i32,
    text: Option<String>,
}

#[cfg(debug_assertions)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct OverlayDebugCounters {
    set_window_pos_calls: u64,
    repaint_requests: u64,
}

fn should_move(previous: Option<&OverlayPosition>, current: &OverlayPosition) -> bool {
    previous != Some(current)
}

fn should_repaint(previous: Option<&OverlayVisualState>, current: &OverlayVisualState) -> bool {
    previous != Some(current)
}

fn should_display(state: IndicatorState) -> bool {
    state != IndicatorState::Hidden
}

fn render_plan(snapshot: &IndicatorSnapshot, config: StatusOverlayConfig) -> OverlayRenderPlan {
    if config.visibility == StatusOverlayVisibility::Hidden
        || config.mode == StatusOverlayMode::Hidden
        || !should_display(snapshot.state)
    {
        return OverlayRenderPlan {
            visible: false,
            width: OVERLAY_WIDTH,
            height: OVERLAY_HEIGHT,
            text: None,
        };
    }

    let text = match config.mode {
        StatusOverlayMode::Minimal | StatusOverlayMode::Hidden => None,
        StatusOverlayMode::Compact => compact_status_text(snapshot, config.fields),
        StatusOverlayMode::Detailed => detailed_status_text(snapshot, config.fields),
    };

    OverlayRenderPlan {
        visible: true,
        width: if text.is_some() {
            TEXT_OVERLAY_WIDTH
        } else {
            OVERLAY_WIDTH
        },
        height: if text.is_some() {
            TEXT_OVERLAY_HEIGHT
        } else {
            OVERLAY_HEIGHT
        },
        text,
    }
}

fn compact_status_text(
    snapshot: &IndicatorSnapshot,
    fields: StatusOverlayFields,
) -> Option<String> {
    if fields.bookmark_mode && snapshot.bookmark_mode_active {
        return Some("BOOKMARK MODE | Press 1-9 to save | Esc cancel".to_string());
    }
    let mut parts = Vec::new();
    if fields.active {
        parts.push(
            if snapshot.app_active {
                "A:ON"
            } else {
                "A:IDLE"
            }
            .to_string(),
        );
    }
    if fields.mouse_speed {
        parts.push(format!(
            "M:{}/{}",
            snapshot.mouse_speed, snapshot.default_mouse_speed
        ));
    }
    if fields.wheel_speed {
        parts.push(format!(
            "W:{}/{}",
            snapshot.wheel_speed, snapshot.default_wheel_speed
        ));
    }
    if fields.drag {
        parts.push(format!(
            "DRG:{}",
            if snapshot.dragging_left { "ON" } else { "OFF" }
        ));
    }
    if fields.jump {
        parts.push(format!(
            "JMP:{}",
            if snapshot.jump_active { "ON" } else { "OFF" }
        ));
    }
    if fields.final_adjust {
        parts.push(format!(
            "SUR:{}",
            if snapshot.final_adjust_active {
                "ON"
            } else {
                "OFF"
            }
        ));
    }
    if fields.slow {
        parts.push(format!("SLW:{}", if snapshot.slow { "ON" } else { "OFF" }));
    }
    if fields.surgical {
        parts.push(format!(
            "SRG:{}",
            if snapshot.surgical { "ON" } else { "OFF" }
        ));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn detailed_status_text(
    snapshot: &IndicatorSnapshot,
    fields: StatusOverlayFields,
) -> Option<String> {
    if fields.flash && snapshot.flash_reason != IndicatorFlashReason::None {
        match snapshot.flash_reason {
            IndicatorFlashReason::MouseSpeed => Some("flash:mouse_speed".to_string()),
            IndicatorFlashReason::WheelSpeed => Some("flash:wheel_speed".to_string()),
            IndicatorFlashReason::None => None,
        }
    } else {
        compact_status_text(snapshot, fields).map(|compact| {
            format!(
                "{} | hint:{} jump:{} grid:{} surgical:{}",
                compact,
                if snapshot.app_active { "ON" } else { "OFF" },
                if snapshot.jump_active { "ON" } else { "OFF" },
                if matches!(snapshot.state, IndicatorState::JumpMode) {
                    "ON"
                } else {
                    "OFF"
                },
                if snapshot.final_adjust_active {
                    "ON"
                } else {
                    "OFF"
                }
            )
        })
    }
}

fn hidden_snapshot() -> IndicatorSnapshot {
    snapshot_from_state(IndicatorState::Hidden)
}

fn snapshot_from_state(state: IndicatorState) -> IndicatorSnapshot {
    IndicatorSnapshot {
        state,
        app_active: state != IndicatorState::Hidden,
        dragging_left: state == IndicatorState::DraggingLeft,
        slow: state == IndicatorState::ActiveSlow,
        surgical: false,
        jump_active: state == IndicatorState::JumpMode,
        jump_stage: None,
        mouse_speed: 0,
        default_mouse_speed: 0,
        wheel_speed: 0,
        default_wheel_speed: 0,
        flash_reason: IndicatorFlashReason::None,
        final_adjust_active: false,
        bookmark_mode_active: false,
    }
}

fn overlay_rect() -> RECT {
    RECT {
        left: 0,
        top: 0,
        right: OVERLAY_WIDTH,
        bottom: OVERLAY_HEIGHT,
    }
}

fn indicator_square_rect() -> RECT {
    RECT {
        left: 0,
        top: 0,
        right: INDICATOR_SQUARE_SIZE.min(OVERLAY_WIDTH),
        bottom: INDICATOR_SQUARE_SIZE.min(OVERLAY_HEIGHT),
    }
}

fn pip_rect(index: u8, pip_count: u8) -> RECT {
    let pip_count = i32::from(pip_count);
    let total_width = pip_count * PIP_SIZE + (pip_count - 1).max(0) * PIP_GAP;
    let left = (OVERLAY_WIDTH - total_width) / 2 + i32::from(index) * (PIP_SIZE + PIP_GAP);
    let top = OVERLAY_HEIGHT - PIP_BOTTOM_MARGIN - PIP_SIZE;

    RECT {
        left,
        top,
        right: left + PIP_SIZE,
        bottom: top + PIP_SIZE,
    }
}

#[cfg(debug_assertions)]
struct PaintSmokeLog {
    window_start: Instant,
    count: u32,
}

#[cfg(debug_assertions)]
impl PaintSmokeLog {
    fn new() -> Self {
        Self {
            window_start: Instant::now(),
            count: 0,
        }
    }

    fn record_paint(&mut self) {
        self.count += 1;
        let elapsed = self.window_start.elapsed();

        if elapsed >= Duration::from_secs(1) {
            println!(
                "[overlay] WM_PAINT smoke: {} paints/sec",
                self.count as f64 / elapsed.as_secs_f64()
            );
            self.window_start = Instant::now();
            self.count = 0;
        }
    }
}

impl OverlayWindow {
    /// Creates the overlay window
    pub fn new() -> Result<Self, Error> {
        println!("🚀 Overlay: Starting Initialization");

        let h_instance = unsafe { GetModuleHandleW(None)? };
        println!("✅ Overlay: Got Module Handle");

        // Register window class
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: h_instance.into(),
            lpszClassName: w!("OverlayClass"),
            style: CS_HREDRAW | CS_VREDRAW,
            hbrBackground: HBRUSH(ptr::null_mut()),
            ..Default::default()
        };

        println!("🔹 Overlay: Registering Window Class...");
        unsafe { RegisterClassW(&wc) };
        println!("✅ Overlay: Window Class Registered");

        // Create window hidden; the caller shows it after global state is ready.
        println!("🔹 Overlay: Creating Overlay Window...");
        let hwnd = unsafe {
            CreateWindowExW(
                overlay_ex_style(),
                w!("OverlayClass"),
                w!("OverlayWindow"),
                WS_POPUP,
                50, // Default X position
                50, // Default Y position
                OVERLAY_WIDTH,
                OVERLAY_HEIGHT,
                None,
                None,
                Some(h_instance.into()),
                None,
            )?
        };
        println!("✅ Overlay: Window Created Successfully!");

        // Store HWND as `isize`
        let hwnd_ptr = Some(hwnd.0 as isize);
        println!("🔹 Overlay: HWND Stored as isize");

        // Apply layered attributes before the first show. The window remains
        // hidden until `show` is called after application globals are ready.
        if let Some(h) = hwnd_ptr {
            unsafe {
                println!("🔹 Overlay: Setting Layered Window Attributes...");
                let _ = SetLayeredWindowAttributes(HWND(h as *mut _), COLORREF(0), 255, LWA_ALPHA);
                let _ = ShowWindow(HWND(h as *mut _), SW_HIDE);
            }
        }

        println!("✅ Overlay: Initialization Completed!");
        let overlay = Self {
            hwnd: Arc::new(Mutex::new(hwnd_ptr)),
            snapshot: hidden_snapshot(),
            config: StatusOverlayConfig::default(),
            visible: false,
            last_cursor_position: None,
            last_visual_state: None,
            #[cfg(debug_assertions)]
            debug_counters: Arc::new(Mutex::new(OverlayDebugCounters::default())),
        };

        // ✅ **Add this line to start tracking the mouse!**

        // overlay.follow_cursor();

        Ok(overlay)
    }

    fn hide(&mut self) {
        if self.visible {
            let hwnd_lock = self.hwnd.lock().unwrap();
            if let Some(h) = *hwnd_lock {
                unsafe {
                    let _ = ShowWindow(HWND(h as *mut _), SW_HIDE);
                }
            }
        }
        self.visible = false;
        self.snapshot = hidden_snapshot();
        self.last_visual_state = Some(OverlayVisualState {
            snapshot_state: IndicatorState::Hidden,
            text: None,
        });
    }

    #[allow(dead_code)]
    pub fn update_overlay_status(&mut self, indicator_state: IndicatorState) {
        self.update_overlay_snapshot(
            snapshot_from_state(indicator_state),
            StatusOverlayConfig::default(),
        );
    }

    pub fn update_overlay_snapshot(
        &mut self,
        snapshot: IndicatorSnapshot,
        config: StatusOverlayConfig,
    ) {
        let plan = render_plan(&snapshot, config);
        if !plan.visible {
            self.hide();
            return;
        }
        self.config = config;

        let hwnd = *self.hwnd.lock().unwrap();
        if let Some(h) = hwnd {
            let hwnd = HWND(h as *mut _);
            let mut point = POINT::default();

            if unsafe { GetCursorPos(&mut point) }.is_ok() {
                let current_position = OverlayPosition {
                    x: point.x + CURSOR_OFFSET_X,
                    y: point.y + CURSOR_OFFSET_Y,
                };
                let current_visual = OverlayVisualState {
                    snapshot_state: snapshot.state,
                    text: plan.text,
                };
                let was_hidden = !self.visible;

                if should_move(self.last_cursor_position.as_ref(), &current_position) {
                    unsafe {
                        let _ = SetWindowPos(
                            hwnd,
                            Some(HWND_TOPMOST),
                            current_position.x,
                            current_position.y,
                            plan.width,
                            plan.height,
                            SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                    self.record_set_window_pos_call();
                }

                if was_hidden {
                    unsafe {
                        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                    }
                    self.visible = true;
                }

                if should_repaint(self.last_visual_state.as_ref(), &current_visual) {
                    self.snapshot = snapshot;
                    self.request_repaint();
                }

                self.last_cursor_position = Some(current_position);
                self.last_visual_state = Some(current_visual);
            }
        }
    }

    /// Draws the square into the caller-provided paint device context.
    pub fn draw(&self, hdc: HDC) {
        let plan = render_plan(&self.snapshot, self.config);
        let visual = indicator_visual(self.snapshot.state);

        unsafe {
            let hbrush = CreateSolidBrush(visual.base_color);
            if !hbrush.0.is_null() {
                let rect = indicator_square_rect();
                let _ = FillRect(hdc, &rect, hbrush);
                let _ = DeleteObject(hbrush.into());
            }

            if visual.draw_outline {
                let outline_brush = CreateSolidBrush(RGB(0, 0, 0));
                if !outline_brush.0.is_null() {
                    let rect = overlay_rect();
                    let _ = FrameRect(hdc, &rect, outline_brush);
                    let _ = DeleteObject(outline_brush.into());
                }
            }

            if visual.pip_count > 0 {
                let pip_brush = CreateSolidBrush(RGB(255, 255, 255));
                if !pip_brush.0.is_null() {
                    for index in 0..visual.pip_count {
                        let rect = pip_rect(index, visual.pip_count);
                        let _ = FillRect(hdc, &rect, pip_brush);
                    }
                    let _ = DeleteObject(pip_brush.into());
                }
            }

            if let Some(text) = plan.text {
                let old_text_color = SetTextColor(hdc, RGB(255, 255, 255));
                let old_bk_mode = SetBkMode(hdc, TRANSPARENT);
                let text: Vec<u16> = text.encode_utf16().collect();
                let _ = TextOutW(hdc, INDICATOR_SQUARE_SIZE + 6, 5, &text);
                let _ = SetTextColor(hdc, old_text_color);
                let _ = SetBkMode(hdc, BACKGROUND_MODE(old_bk_mode as u32));
            }
        }
    }

    /// Requests a repaint by invalidating the client region.
    pub fn request_repaint(&self) {
        let hwnd_lock = self.hwnd.lock().unwrap();
        if let Some(h) = *hwnd_lock {
            let hwnd = HWND(h as *mut _); // ✅ Convert `isize` back to `HWND`
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            self.record_repaint_request();
        }
    }

    #[allow(dead_code)]
    pub fn follow_cursor(&self) {
        let hwnd_arc = Arc::clone(&self.hwnd); // Clone Arc for safe access in the thread
        let is_moving_arc = Arc::new(Mutex::new(false)); // Prevent unnecessary movement updates
        let is_moving_clone = Arc::clone(&is_moving_arc);

        thread::spawn(move || {
            loop {
                let hwnd_lock = hwnd_arc.lock().unwrap();
                if let Some(h) = *hwnd_lock {
                    let hwnd = HWND(h as *mut _);
                    let mut point = POINT::default();

                    if unsafe { GetCursorPos(&mut point) }.is_ok() {
                        let x = point.x + CURSOR_OFFSET_X;
                        let y = point.y + CURSOR_OFFSET_Y;

                        // Only update if the position is different to avoid unnecessary SetWindowPos calls
                        let mut is_moving = is_moving_clone.lock().unwrap();
                        if !*is_moving {
                            *is_moving = true;
                            unsafe {
                                let _ = SetWindowPos(
                                    hwnd,
                                    Some(HWND_TOPMOST),
                                    x,
                                    y,
                                    OVERLAY_WIDTH,
                                    OVERLAY_HEIGHT,
                                    SWP_NOZORDER | SWP_NOACTIVATE,
                                );
                            }
                            *is_moving = false;
                        }
                    }
                }
                drop(hwnd_lock);
                thread::sleep(Duration::from_millis(200)); // Lower update rate to reduce CPU usage
            }
        });
    }

    #[cfg(debug_assertions)]
    fn record_set_window_pos_call(&self) {
        self.debug_counters
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .set_window_pos_calls += 1;
    }

    #[cfg(not(debug_assertions))]
    fn record_set_window_pos_call(&self) {}

    #[cfg(debug_assertions)]
    fn record_repaint_request(&self) {
        self.debug_counters
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .repaint_requests += 1;
    }

    #[cfg(not(debug_assertions))]
    fn record_repaint_request(&self) {}

    #[cfg(debug_assertions)]
    #[allow(dead_code)]
    fn debug_counters(&self) -> OverlayDebugCounters {
        *self
            .debug_counters
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }
}

/// Helper function to create a `COLORREF`
#[allow(non_snake_case)]
pub fn RGB(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(((b as u32) << 16) | ((g as u32) << 8) | (r as u32))
}

fn indicator_visual(state: IndicatorState) -> IndicatorVisual {
    match state {
        IndicatorState::Hidden => IndicatorVisual {
            base_color: RGB(0, 0, 0),
            pip_count: 0,
            draw_outline: false,
        },
        IndicatorState::ActiveNormal => IndicatorVisual {
            base_color: RGB(255, 0, 0),
            pip_count: 0,
            draw_outline: true,
        },
        IndicatorState::ActiveSlow => IndicatorVisual {
            base_color: RGB(0, 255, 0),
            pip_count: 0,
            draw_outline: true,
        },
        IndicatorState::DraggingLeft => IndicatorVisual {
            base_color: RGB(0, 255, 255),
            pip_count: 0,
            draw_outline: false,
        },
        IndicatorState::JumpMode => IndicatorVisual {
            base_color: RGB(0, 120, 255),
            pip_count: 0,
            draw_outline: true,
        },
        IndicatorState::MouseSpeedSlow => IndicatorVisual {
            base_color: RGB(80, 180, 255),
            pip_count: 1,
            draw_outline: true,
        },
        IndicatorState::MouseSpeedNormal => IndicatorVisual {
            base_color: RGB(80, 255, 180),
            pip_count: 2,
            draw_outline: true,
        },
        IndicatorState::MouseSpeedFast => IndicatorVisual {
            base_color: RGB(255, 80, 120),
            pip_count: 3,
            draw_outline: true,
        },
        IndicatorState::WheelScrollingSlow => IndicatorVisual {
            base_color: RGB(255, 180, 0),
            pip_count: 1,
            draw_outline: true,
        },
        IndicatorState::WheelScrollingNormal => IndicatorVisual {
            base_color: RGB(255, 255, 0),
            pip_count: 2,
            draw_outline: true,
        },
        IndicatorState::WheelScrollingFast => IndicatorVisual {
            base_color: RGB(255, 0, 255),
            pip_count: 3,
            draw_outline: true,
        },
    }
}

/// Window procedure for overlay
extern "system" fn window_proc(hwnd: HWND, msg: u32, _wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            // BeginPaint must be paired with EndPaint so Windows validates the
            // dirty region; otherwise the same region remains invalid and can
            // dispatch WM_PAINT repeatedly.
            let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
            #[cfg(debug_assertions)]
            {
                PAINT_SMOKE_LOG
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .record_paint();
            }
            if let Some(ref mut ov) = *OVERLAY.lock().unwrap_or_else(|e| e.into_inner()) {
                ov.draw(hdc);
            }
            // EndPaint completes the validation started by BeginPaint; keep all
            // rendering for this message inside that pair.
            unsafe {
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            println!("🛑 Overlay Window Destroyed!");
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        WM_NCHITTEST => {
            // Defense in depth for click-through behavior: the extended
            // WS_EX_TRANSPARENT style should keep this overlay out of mouse
            // targeting, and HTTRANSPARENT preserves that behavior if Windows
            // still asks the non-client hit-test path about this window.
            LRESULT(HTTRANSPARENT as isize)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, _wparam, _lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::{
        indicator_visual, overlay_ex_style, render_plan, should_display, should_move,
        should_repaint, snapshot_from_state, IndicatorVisual, OverlayPosition, OverlayVisualState,
        StatusOverlayConfig, StatusOverlayMode, RGB,
    };
    use crate::indicator::IndicatorState;
    use windows::Win32::UI::WindowsAndMessaging::{
        WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    };

    #[test]
    fn style_contains_click_through_and_non_activate_bits() {
        let style = overlay_ex_style();

        assert!(style.contains(WS_EX_LAYERED));
        assert!(style.contains(WS_EX_TOPMOST));
        assert!(style.contains(WS_EX_TOOLWINDOW));
        assert!(style.contains(WS_EX_TRANSPARENT));
        assert!(style.contains(WS_EX_NOACTIVATE));
    }

    #[test]
    fn hidden_state_suppresses_display_path() {
        assert!(!should_display(IndicatorState::Hidden));
        assert!(should_display(IndicatorState::ActiveNormal));
    }

    #[test]
    fn should_move_when_there_is_no_previous_position() {
        let current = OverlayPosition { x: 10, y: 20 };

        assert!(should_move(None, &current));
    }

    #[test]
    fn should_move_only_when_position_changes() {
        let previous = OverlayPosition { x: 10, y: 20 };
        let same = OverlayPosition { x: 10, y: 20 };
        let changed = OverlayPosition { x: 11, y: 20 };

        assert!(!should_move(Some(&previous), &same));
        assert!(should_move(Some(&previous), &changed));
    }

    #[test]
    fn should_repaint_when_there_is_no_previous_visual_state() {
        let current = OverlayVisualState {
            snapshot_state: IndicatorState::ActiveNormal,
            text: None,
        };

        assert!(should_repaint(None, &current));
    }

    #[test]
    fn should_repaint_only_when_visual_state_changes() {
        let previous = OverlayVisualState {
            snapshot_state: IndicatorState::ActiveNormal,
            text: None,
        };
        let same = OverlayVisualState {
            snapshot_state: IndicatorState::ActiveNormal,
            text: None,
        };
        let changed = OverlayVisualState {
            snapshot_state: IndicatorState::ActiveSlow,
            text: None,
        };

        assert!(!should_repaint(Some(&previous), &same));
        assert!(should_repaint(Some(&previous), &changed));
    }

    #[test]
    fn hidden_state_has_a_distinct_non_display_color_branch() {
        assert_eq!(
            indicator_visual(IndicatorState::Hidden).base_color,
            RGB(0, 0, 0)
        );
    }

    #[test]
    fn minimal_render_plan_matches_legacy_square() {
        let plan = render_plan(
            &snapshot_from_state(IndicatorState::ActiveNormal),
            StatusOverlayConfig::default(),
        );

        assert!(plan.visible);
        assert_eq!(plan.width, super::OVERLAY_WIDTH);
        assert_eq!(plan.height, super::OVERLAY_HEIGHT);
        assert_eq!(plan.text, None);
    }

    #[test]
    fn compact_render_plan_uses_short_value_text() {
        let mut config = StatusOverlayConfig::default();
        config.mode = StatusOverlayMode::Compact;
        let plan = render_plan(&snapshot_from_state(IndicatorState::JumpMode), config);

        assert!(plan.visible);
        let text = plan.text.unwrap_or_default();
        assert!(text.contains("A:ON"));
        assert!(text.contains("JMP:ON"));
        assert!(plan.width > super::OVERLAY_WIDTH);
    }

    #[test]
    fn compact_render_plan_shows_bookmark_mode_indicator_when_enabled() {
        let mut config = StatusOverlayConfig::default();
        config.mode = StatusOverlayMode::Compact;
        config.fields.bookmark_mode = true;
        let mut snapshot = snapshot_from_state(IndicatorState::ActiveNormal);
        snapshot.bookmark_mode_active = true;

        let plan = render_plan(&snapshot, config);
        assert_eq!(
            plan.text.as_deref(),
            Some("BOOKMARK MODE | Press 1-9 to save | Esc cancel")
        );
    }

    #[test]
    fn active_slow_and_jump_states_render_distinct_colors() {
        assert_ne!(
            indicator_visual(IndicatorState::ActiveNormal).base_color,
            indicator_visual(IndicatorState::ActiveSlow).base_color
        );
        assert_ne!(
            indicator_visual(IndicatorState::ActiveSlow).base_color,
            indicator_visual(IndicatorState::JumpMode).base_color
        );
    }

    #[test]
    fn wheel_speed_states_render_distinct_colors() {
        assert_ne!(
            indicator_visual(IndicatorState::WheelScrollingSlow).base_color,
            indicator_visual(IndicatorState::WheelScrollingNormal).base_color
        );
        assert_ne!(
            indicator_visual(IndicatorState::WheelScrollingNormal).base_color,
            indicator_visual(IndicatorState::WheelScrollingFast).base_color
        );
    }

    #[test]
    fn wheel_speed_states_map_to_speed_pip_counts() {
        assert_eq!(
            indicator_visual(IndicatorState::WheelScrollingSlow).pip_count,
            1
        );
        assert_eq!(
            indicator_visual(IndicatorState::WheelScrollingNormal).pip_count,
            2
        );
        assert_eq!(
            indicator_visual(IndicatorState::WheelScrollingFast).pip_count,
            3
        );
    }

    #[test]
    fn mouse_speed_states_render_distinct_colors_and_pips() {
        assert_ne!(
            indicator_visual(IndicatorState::MouseSpeedSlow).base_color,
            indicator_visual(IndicatorState::MouseSpeedNormal).base_color
        );
        assert_ne!(
            indicator_visual(IndicatorState::MouseSpeedNormal).base_color,
            indicator_visual(IndicatorState::MouseSpeedFast).base_color
        );
        assert_eq!(
            indicator_visual(IndicatorState::MouseSpeedSlow).pip_count,
            1
        );
        assert_eq!(
            indicator_visual(IndicatorState::MouseSpeedNormal).pip_count,
            2
        );
        assert_eq!(
            indicator_visual(IndicatorState::MouseSpeedFast).pip_count,
            3
        );
    }

    #[test]
    fn non_wheel_states_map_to_zero_pips() {
        let non_wheel_states = [
            IndicatorState::Hidden,
            IndicatorState::ActiveNormal,
            IndicatorState::ActiveSlow,
            IndicatorState::DraggingLeft,
            IndicatorState::JumpMode,
        ];

        for state in non_wheel_states {
            assert_eq!(indicator_visual(state).pip_count, 0);
        }
    }

    #[test]
    fn drag_visual_is_distinct_from_active_normal() {
        assert_ne!(
            indicator_visual(IndicatorState::DraggingLeft),
            indicator_visual(IndicatorState::ActiveNormal)
        );
        assert_eq!(
            indicator_visual(IndicatorState::DraggingLeft),
            IndicatorVisual {
                base_color: RGB(0, 255, 255),
                pip_count: 0,
                draw_outline: false,
            }
        );
    }
}
