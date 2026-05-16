use std::cell::RefCell;
use std::ptr;
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::{
    jump_grid::{index_to_code, letters_needed},
    jump_session::JumpRegion,
    jump_view::JumpOverlayView,
    overlay::RGB,
    screen_capture::{capture_virtual_screen, ScreenSnapshot},
};

thread_local! {
    /// Thread-local jump overlay state.
    ///
    /// Win32 window handles (`HWND`) are thread-affine and must be created and
    /// manipulated from the UI/hook thread that owns the window. We intentionally
    /// keep `JumpOverlay` in TLS to prevent accidental cross-thread access.
    pub static JUMP_OVERLAY: RefCell<JumpOverlay> = RefCell::new(JumpOverlay::new());
}

const OVERLAY_ALPHA: u8 = 255;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScreenRect {
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

pub fn virtual_screen_region() -> JumpRegion {
    ScreenRect::from_virtual_screen().into()
}

impl ScreenRect {
    fn from_virtual_screen() -> Self {
        unsafe {
            Self {
                left: GetSystemMetrics(SM_XVIRTUALSCREEN),
                top: GetSystemMetrics(SM_YVIRTUALSCREEN),
                width: GetSystemMetrics(SM_CXVIRTUALSCREEN),
                height: GetSystemMetrics(SM_CYVIRTUALSCREEN),
            }
        }
    }
}

fn overlay_colorkey() -> COLORREF {
    RGB(0, 0, 0)
}

fn grid_color() -> COLORREF {
    RGB(255, 255, 255)
}

fn label_color() -> COLORREF {
    RGB(255, 255, 0)
}

fn input_color() -> COLORREF {
    RGB(0, 255, 255)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransparencyMode {
    ColorKey { color: COLORREF, alpha: u8 },
    Opaque { alpha: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DrawMode {
    TransparentGrid,
    MagnifiedPreview,
}

fn overlay_ex_style() -> WINDOW_EX_STYLE {
    WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE
}

fn transparency_mode() -> TransparencyMode {
    TransparencyMode::ColorKey {
        color: overlay_colorkey(),
        alpha: OVERLAY_ALPHA,
    }
}

fn apply_layered_attributes(hwnd: HWND, mode: TransparencyMode) {
    unsafe {
        match mode {
            TransparencyMode::ColorKey { color, alpha } => {
                let _ = SetLayeredWindowAttributes(hwnd, color, alpha, LWA_COLORKEY);
            }
            TransparencyMode::Opaque { alpha } => {
                let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
            }
        }
    }
}

fn draw_mode_for_view(view: &JumpOverlayView) -> DrawMode {
    if view.stage_index > 0 {
        DrawMode::MagnifiedPreview
    } else {
        DrawMode::TransparentGrid
    }
}

fn format_jump_indicator(view: &JumpOverlayView) -> String {
    format!(
        "Jump {}/{}: {}_",
        view.stage_index + 1,
        view.stage_count,
        view.input
    )
}

fn preview_source_rect(view: &JumpOverlayView, snapshot: &ScreenSnapshot) -> RECT {
    let margin_x = view.region.width * view.preview_margin_percent as i32 / 100;
    let margin_y = view.region.height * view.preview_margin_percent as i32 / 100;
    let snapshot_right = snapshot.left + snapshot.width;
    let snapshot_bottom = snapshot.top + snapshot.height;

    RECT {
        left: (view.region.left - margin_x).max(snapshot.left),
        top: (view.region.top - margin_y).max(snapshot.top),
        right: (view.region.left + view.region.width + margin_x).min(snapshot_right),
        bottom: (view.region.top + view.region.height + margin_y).min(snapshot_bottom),
    }
}

pub struct JumpOverlay {
    hwnd: Option<HWND>,
    visible: bool,
    view: Option<JumpOverlayView>,
    snapshot: Option<ScreenSnapshot>,
    repaint_requested: bool,
}

impl JumpOverlay {
    pub fn new() -> Self {
        Self {
            hwnd: None,
            visible: false,
            view: None,
            snapshot: None,
            repaint_requested: false,
        }
    }

    fn request_repaint(&mut self) {
        self.repaint_requested = true;
        if let Some(h) = self.hwnd {
            unsafe {
                let _ = InvalidateRect(Some(h), None, false);
            }
        }
    }

    fn create_window(&mut self) {
        if self.hwnd.is_some() {
            return;
        }
        unsafe {
            let h_instance = GetModuleHandleW(None).unwrap();
            let class = w!("JumpOverlayClass");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(jump_window_proc),
                hInstance: h_instance.into(),
                lpszClassName: class,
                style: CS_HREDRAW | CS_VREDRAW,
                hbrBackground: HBRUSH(ptr::null_mut()),
                ..Default::default()
            };
            let atom = RegisterClassW(&wc);
            if atom == 0 {
                println!("RegisterClassW failed: {:?}", GetLastError());
            }
            let screen_rect = ScreenRect::from_virtual_screen();
            let hwnd = CreateWindowExW(
                overlay_ex_style(),
                class,
                w!("JumpOverlay"),
                WS_POPUP,
                screen_rect.left,
                screen_rect.top,
                screen_rect.width,
                screen_rect.height,
                None,
                None,
                Some(h_instance.into()),
                None,
            );
            match hwnd {
                Ok(h) => {
                    apply_layered_attributes(h, transparency_mode());
                    let _ = ShowWindow(h, SW_HIDE);
                    self.hwnd = Some(h);
                }
                Err(e) => {
                    println!("CreateWindowExW failed: {:?}", e);
                }
            }
        }
    }

    pub fn initialize(&mut self, view: JumpOverlayView) {
        self.create_window();
        self.snapshot = capture_virtual_screen();
        self.view = Some(view);
        self.apply_view_layering();
        self.repaint_requested = false;
    }

    pub fn show(&mut self) {
        if let Some(h) = self.hwnd {
            unsafe {
                let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
            }
            self.request_repaint();
            self.visible = true;
        }
    }

    pub fn hide(&mut self) {
        self.view = None;
        self.snapshot = None;
        if let Some(h) = self.hwnd {
            unsafe {
                let _ = ShowWindow(h, SW_HIDE);
            }
        }
        self.visible = false;
    }

    fn effective_draw_mode(&self) -> DrawMode {
        let Some(view) = &self.view else {
            return DrawMode::TransparentGrid;
        };
        match (draw_mode_for_view(view), self.snapshot.as_ref()) {
            (DrawMode::MagnifiedPreview, Some(_)) => DrawMode::MagnifiedPreview,
            _ => DrawMode::TransparentGrid,
        }
    }

    fn apply_view_layering(&self) {
        if let Some(hwnd) = self.hwnd {
            let mode = match self.effective_draw_mode() {
                DrawMode::TransparentGrid => transparency_mode(),
                DrawMode::MagnifiedPreview => TransparencyMode::Opaque {
                    alpha: OVERLAY_ALPHA,
                },
            };
            apply_layered_attributes(hwnd, mode);
        }
    }

    fn client_rect(&self) -> Option<RECT> {
        let hwnd = self.hwnd?;
        unsafe {
            let mut rect = RECT::default();
            GetClientRect(hwnd, &mut rect).ok()?;
            Some(rect)
        }
    }

    fn grid_rect(&self, view: &JumpOverlayView, client_rect: RECT) -> RECT {
        match self.effective_draw_mode() {
            DrawMode::TransparentGrid => {
                let virtual_screen = ScreenRect::from_virtual_screen();
                RECT {
                    left: view.region.left - virtual_screen.left,
                    top: view.region.top - virtual_screen.top,
                    right: view.region.left - virtual_screen.left + view.region.width,
                    bottom: view.region.top - virtual_screen.top + view.region.height,
                }
            }
            DrawMode::MagnifiedPreview => client_rect,
        }
    }

    fn draw_background(&self, hdc: HDC, client_rect: &RECT, view: &JumpOverlayView) {
        unsafe {
            match self.effective_draw_mode() {
                DrawMode::TransparentGrid => {
                    let bg_brush = CreateSolidBrush(overlay_colorkey());
                    let _ = FillRect(hdc, client_rect, bg_brush);
                    let _ = DeleteObject(bg_brush.into());
                }
                DrawMode::MagnifiedPreview => {
                    if let Some(snapshot) = &self.snapshot {
                        let _ = BitBlt(
                            hdc,
                            0,
                            0,
                            snapshot.width,
                            snapshot.height,
                            Some(snapshot.hdc()),
                            0,
                            0,
                            SRCCOPY,
                        );

                        let src = preview_source_rect(view, snapshot);
                        let src_w = src.right - src.left;
                        let src_h = src.bottom - src.top;
                        if src_w > 0 && src_h > 0 {
                            let _ = SetStretchBltMode(hdc, HALFTONE);
                            let _ = StretchBlt(
                                hdc,
                                client_rect.left,
                                client_rect.top,
                                client_rect.right - client_rect.left,
                                client_rect.bottom - client_rect.top,
                                Some(snapshot.hdc()),
                                snapshot.source_x(src.left),
                                snapshot.source_y(src.top),
                                src_w,
                                src_h,
                                SRCCOPY,
                            );
                        }
                    }
                }
            }
        }
    }

    fn draw(&self, hdc: HDC) {
        if self.hwnd.is_some() {
            let Some(view) = &self.view else {
                return;
            };
            let grid_size = view.grid_size;
            if grid_size.0 == 0 || grid_size.1 == 0 {
                return;
            }
            unsafe {
                let Some(rect) = self.client_rect() else {
                    return;
                };
                self.draw_background(hdc, &rect, view);

                let grid_rect = self.grid_rect(view, rect);
                let width = grid_rect.right - grid_rect.left;
                let height = grid_rect.bottom - grid_rect.top;
                let cell_w = width / grid_size.0 as i32;
                let cell_h = height / grid_size.1 as i32;
                if cell_w <= 0 || cell_h <= 0 {
                    return;
                }

                let old_bk_mode = SetBkMode(hdc, TRANSPARENT);
                let old_text_color = SetTextColor(hdc, label_color());

                let pen = CreatePen(PS_SOLID, 1, grid_color());
                let old_pen = SelectObject(hdc, pen.into());

                for x in 0..=grid_size.0 {
                    let pos = grid_rect.left + (x as i32 * cell_w);
                    let _ = MoveToEx(hdc, pos, grid_rect.top, None);
                    let _ = LineTo(hdc, pos, grid_rect.bottom);
                }
                for y in 0..=grid_size.1 {
                    let pos = grid_rect.top + (y as i32 * cell_h);
                    let _ = MoveToEx(hdc, grid_rect.left, pos, None);
                    let _ = LineTo(hdc, grid_rect.right, pos);
                }

                let row_len = letters_needed(grid_size.1);
                let col_len = letters_needed(grid_size.0);
                for row in 0..grid_size.1 {
                    let row_code = index_to_code(row as usize, row_len);
                    for col in 0..grid_size.0 {
                        let col_code = index_to_code(col as usize, col_len);
                        let code = format!("{}{}", row_code, col_code);
                        let text: Vec<u16> = code.encode_utf16().collect();
                        let x = grid_rect.left + col as i32 * cell_w + cell_w / 2 - 8;
                        let y = grid_rect.top + row as i32 * cell_h + cell_h / 2 - 8;
                        let _ = TextOutW(hdc, x, y, &text);
                    }
                }

                let _ = SetTextColor(hdc, input_color());
                let indicator = format_jump_indicator(view);
                let indicator_utf16: Vec<u16> = indicator.encode_utf16().collect();
                let indicator_x = grid_rect.left + (width / 2) - 60;
                let indicator_y = grid_rect.top + 16;
                let _ = TextOutW(hdc, indicator_x, indicator_y, &indicator_utf16);

                let _ = SetTextColor(hdc, old_text_color);
                let _ = SetBkMode(hdc, BACKGROUND_MODE(old_bk_mode as u32));
                let _ = SelectObject(hdc, old_pen);
                let _ = DeleteObject(pen.into());
            }
        }
    }

    pub fn update_view(&mut self, view: JumpOverlayView) {
        self.view = Some(view);
        self.apply_view_layering();
        self.request_repaint();
    }
}

impl Drop for JumpOverlay {
    fn drop(&mut self) {
        self.snapshot = None;
    }
}

impl From<ScreenRect> for JumpRegion {
    fn from(rect: ScreenRect) -> Self {
        Self {
            left: rect.left,
            top: rect.top,
            width: rect.width,
            height: rect.height,
        }
    }
}

extern "system" fn jump_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let ps = &mut PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, ps) };
            JUMP_OVERLAY.with(|overlay| {
                let mut ov = overlay.borrow_mut();
                ov.draw(hdc);
                ov.repaint_requested = false;
            });
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

pub fn show_jump_overlay(view: JumpOverlayView) {
    JUMP_OVERLAY.with(|overlay| {
        let mut ov = overlay.borrow_mut();
        ov.initialize(view);
        ov.show();
    });
}

pub fn update_jump_overlay(view: JumpOverlayView) {
    JUMP_OVERLAY.with(|overlay| overlay.borrow_mut().update_view(view));
}

pub fn hide_jump_overlay() {
    JUMP_OVERLAY.with(|overlay| overlay.borrow_mut().hide());
}

#[cfg(test)]
mod tests {
    use super::{
        draw_mode_for_view, format_jump_indicator, overlay_colorkey, overlay_ex_style,
        transparency_mode, DrawMode, TransparencyMode, OVERLAY_ALPHA,
    };
    use crate::jump_view::JumpOverlayView;
    use windows::Win32::UI::WindowsAndMessaging::{WS_EX_NOACTIVATE, WS_EX_TRANSPARENT};

    #[test]
    fn style_contains_non_activate_and_click_through_bits() {
        let style = overlay_ex_style();
        assert!(style.contains(WS_EX_NOACTIVATE));
        assert!(style.contains(WS_EX_TRANSPARENT));
    }

    #[test]
    fn jump_indicator_formatting() {
        assert_eq!(format_jump_indicator(&view(0, 1, "")), "Jump 1/1: _");
        assert_eq!(format_jump_indicator(&view(1, 3, "AB")), "Jump 2/3: AB_");
    }

    #[test]
    fn transparency_mode_is_colorkey_black() {
        assert_eq!(
            transparency_mode(),
            TransparencyMode::ColorKey {
                color: overlay_colorkey(),
                alpha: OVERLAY_ALPHA
            }
        );
    }

    #[test]
    fn first_stage_uses_transparent_grid() {
        assert_eq!(
            draw_mode_for_view(&view(0, 2, "")),
            DrawMode::TransparentGrid
        );
    }

    #[test]
    fn later_stages_use_magnified_preview() {
        assert_eq!(
            draw_mode_for_view(&view(1, 2, "")),
            DrawMode::MagnifiedPreview
        );
    }

    fn view(stage_index: usize, stage_count: usize, input: &str) -> JumpOverlayView {
        JumpOverlayView {
            stage_index,
            stage_count,
            stages: Vec::new(),
            region: crate::jump_session::JumpRegion {
                left: -100,
                top: 50,
                width: 200,
                height: 100,
            },
            grid_size: (10, 10),
            input: input.to_string(),
            preview_margin_percent: 10,
        }
    }
}
