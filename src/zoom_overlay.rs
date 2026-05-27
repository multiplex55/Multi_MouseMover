use std::cell::RefCell;
use std::ptr;
use std::time::{Duration, Instant};
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::screen_capture::capture_region_around_cursor;

thread_local! {
    pub static SURGICAL_ZOOM_OVERLAY: RefCell<SurgicalZoomOverlay> = RefCell::new(SurgicalZoomOverlay::new());
}

unsafe extern "system" fn surgical_zoom_overlay_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurgicalZoomConfig {
    pub enabled: bool,
    pub zoom_enabled: bool,
    pub zoom_scale: f32,
    pub zoom_size_px: i32,
    pub overlay_offset_x: i32,
    pub overlay_offset_y: i32,
    pub refresh_interval_ms: u64,
    pub center_crosshair: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SurgicalZoomState {
    pub visible: bool,
    pub x: i32,
    pub y: i32,
    pub source_left: i32,
    pub source_top: i32,
    pub source_size_px: i32,
    pub center_crosshair: bool,
    pub pending_refresh: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtualScreenBounds {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

pub struct SurgicalZoomOverlay {
    hwnd: Option<HWND>,
    last_refresh: Option<Instant>,
}

impl SurgicalZoomOverlay {
    pub fn new() -> Self {
        Self {
            hwnd: None,
            last_refresh: None,
        }
    }
    fn ensure_window(&mut self) {
        if self.hwnd.is_some() {
            return;
        }
        unsafe {
            let hinstance = GetModuleHandleW(None).ok().unwrap_or_default();
            let class_name = w!("SurgicalZoomOverlayWindowClass");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(surgical_zoom_overlay_proc),
                hInstance: hinstance.into(),
                lpszClassName: class_name,
                hCursor: LoadCursorW(None, IDC_ARROW).ok().unwrap_or_default(),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOACTIVATE,
                class_name,
                w!("Surgical Zoom"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(HINSTANCE(hinstance.0)),
                Some(ptr::null_mut()),
            );
            if let Ok(hwnd) = hwnd {
                let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
                self.hwnd = Some(hwnd);
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    }

    fn hide(&mut self) {
        if let Some(hwnd) = self.hwnd {
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    }

    fn render(
        &mut self,
        state: &SurgicalZoomState,
        refresh_interval_ms: u64,
        overlay_size_px: i32,
    ) {
        self.ensure_window();
        let Some(hwnd) = self.hwnd else {
            return;
        };
        let now = Instant::now();
        if !zoom_refresh_throttles_to_configured_interval(
            self.last_refresh,
            now,
            refresh_interval_ms,
        ) {
            return;
        }
        let Some(snapshot) = capture_region_around_cursor(
            state.source_left + state.source_size_px / 2,
            state.source_top + state.source_size_px / 2,
            state.source_size_px,
        ) else {
            return;
        };
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                state.x,
                state.y,
                overlay_size_px,
                overlay_size_px,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let hdc = GetDC(Some(hwnd));
            if hdc.is_invalid() {
                return;
            }
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            let w = rect.right - rect.left;
            let h = rect.bottom - rect.top;
            let _ = StretchBlt(
                hdc,
                0,
                0,
                w,
                h,
                Some(snapshot.hdc()),
                0,
                0,
                snapshot.width,
                snapshot.height,
                SRCCOPY,
            );
            if state.center_crosshair {
                let pen = CreatePen(PS_SOLID, 1, COLORREF(0x0000FF));
                let old = SelectObject(hdc, pen.into());
                let cx = w / 2;
                let cy = h / 2;
                let _ = MoveToEx(hdc, cx - 10, cy, None);
                let _ = LineTo(hdc, cx + 10, cy);
                let _ = MoveToEx(hdc, cx, cy - 10, None);
                let _ = LineTo(hdc, cx, cy + 10);
                let _ = SelectObject(hdc, old);
                let _ = DeleteObject(pen.into());
            }
            let _ = ReleaseDC(Some(hwnd), hdc);
        }
        self.last_refresh = Some(now);
    }
}

pub fn sync_surgical_zoom_overlay(state: SurgicalZoomState, config: SurgicalZoomConfig) {
    SURGICAL_ZOOM_OVERLAY.with(|ov| {
        let mut ov = ov.borrow_mut();
        if !(state.visible && config.enabled && config.zoom_enabled) {
            ov.hide();
            return;
        }
        ov.render(&state, config.refresh_interval_ms, config.zoom_size_px);
    });
}

pub fn update_zoom_state(
    state: &mut SurgicalZoomState,
    config: SurgicalZoomConfig,
    surgical_active: bool,
    cursor_x: i32,
    cursor_y: i32,
    virtual_screen: VirtualScreenBounds,
) {
    if !(config.enabled && config.zoom_enabled && surgical_active) {
        state.visible = false;
        state.pending_refresh = false;
        return;
    }
    state.visible = true;
    state.x = cursor_x + config.overlay_offset_x;
    state.y = cursor_y + config.overlay_offset_y;
    let source_size = ((config.zoom_size_px as f32) / config.zoom_scale)
        .round()
        .max(1.0) as i32;
    let (source_left, source_top) = surgical_zoom_source_rect(
        cursor_x,
        cursor_y,
        source_size,
        virtual_screen.left,
        virtual_screen.top,
        virtual_screen.width,
        virtual_screen.height,
    );
    state.source_left = source_left;
    state.source_top = source_top;
    state.source_size_px = source_size;
    state.center_crosshair = config.center_crosshair;
    state.pending_refresh = true;
}

pub fn surgical_zoom_source_rect(
    cursor_x: i32,
    cursor_y: i32,
    source_size_px: i32,
    virtual_left: i32,
    virtual_top: i32,
    virtual_width: i32,
    virtual_height: i32,
) -> (i32, i32) {
    let half = source_size_px / 2;
    let source_width = source_size_px.min(virtual_width.max(1));
    let source_height = source_size_px.min(virtual_height.max(1));
    let max_left = virtual_left + virtual_width - source_width;
    let max_top = virtual_top + virtual_height - source_height;
    (
        (cursor_x - half).clamp(virtual_left, max_left),
        (cursor_y - half).clamp(virtual_top, max_top),
    )
}

pub fn zoom_refresh_throttles_to_configured_interval(
    last_refresh: Option<Instant>,
    now: Instant,
    refresh_interval_ms: u64,
) -> bool {
    if refresh_interval_ms == 0 {
        return true;
    }
    let interval = Duration::from_millis(refresh_interval_ms);
    last_refresh.is_none_or(|ts| now.duration_since(ts) >= interval)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn surgical_zoom_source_rect_clamps_at_screen_edges() {
        assert_eq!(
            surgical_zoom_source_rect(2, 2, 100, 0, 0, 1920, 1080),
            (0, 0)
        );
        assert_eq!(
            surgical_zoom_source_rect(1919, 1079, 100, 0, 0, 1920, 1080),
            (1820, 980)
        );
    }
    #[test]
    fn overlay_position_applies_configured_offsets() {
        let mut state = SurgicalZoomState::default();
        let cfg = SurgicalZoomConfig {
            enabled: true,
            zoom_enabled: true,
            zoom_scale: 2.0,
            zoom_size_px: 180,
            overlay_offset_x: 24,
            overlay_offset_y: 12,
            refresh_interval_ms: 16,
            center_crosshair: true,
        };
        update_zoom_state(
            &mut state,
            cfg,
            true,
            100,
            200,
            VirtualScreenBounds {
                left: 0,
                top: 0,
                width: 1920,
                height: 1080,
            },
        );
        assert_eq!((state.x, state.y), (124, 212));
    }
    #[test]
    fn surgical_mode_visibility_toggles_overlay_state() {
        let mut state = SurgicalZoomState::default();
        let cfg = SurgicalZoomConfig {
            enabled: true,
            zoom_enabled: true,
            zoom_scale: 2.0,
            zoom_size_px: 180,
            overlay_offset_x: 0,
            overlay_offset_y: 0,
            refresh_interval_ms: 16,
            center_crosshair: false,
        };
        update_zoom_state(
            &mut state,
            cfg,
            true,
            10,
            20,
            VirtualScreenBounds {
                left: 0,
                top: 0,
                width: 100,
                height: 100,
            },
        );
        assert!(state.visible);
        update_zoom_state(
            &mut state,
            cfg,
            false,
            10,
            20,
            VirtualScreenBounds {
                left: 0,
                top: 0,
                width: 100,
                height: 100,
            },
        );
        assert!(!state.visible);
    }
    #[test]
    fn zoom_refresh_interval_predicate() {
        let now = Instant::now();
        assert!(zoom_refresh_throttles_to_configured_interval(None, now, 16));
        assert!(!zoom_refresh_throttles_to_configured_interval(
            Some(now),
            now + Duration::from_millis(8),
            16
        ));
        assert!(zoom_refresh_throttles_to_configured_interval(
            Some(now),
            now + Duration::from_millis(16),
            16
        ));
    }
}
