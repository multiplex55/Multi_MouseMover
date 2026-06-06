use std::ptr;

use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::bookmark_markers::{BookmarkMarkerOverlayView, BookmarkMarkerView, RgbColor};
use crate::BookmarkMarkerShape;

const TRANSPARENT_COLOR: RgbColor = RgbColor {
    red: 0,
    green: 0,
    blue: 0,
};
const WINDOW_CLASS_NAME: PCWSTR = w!("BookmarkMarkerOverlayWindowClass");
const WINDOW_TITLE: PCWSTR = w!("Bookmark Marker Overlay");

pub fn bookmark_marker_overlay_ex_style() -> WINDOW_EX_STYLE {
    WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE
}

pub fn bookmark_marker_overlay_style() -> WINDOW_STYLE {
    WS_POPUP
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BookmarkMarkerVirtualScreen {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

impl BookmarkMarkerVirtualScreen {
    fn current() -> Self {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkerRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

pub fn marker_client_origin(
    marker_x: i32,
    marker_y: i32,
    virtual_screen: BookmarkMarkerVirtualScreen,
) -> (i32, i32) {
    (
        marker_x - virtual_screen.left,
        marker_y - virtual_screen.top,
    )
}

pub fn marker_rect(
    marker_x: i32,
    marker_y: i32,
    size_px: i32,
    offset_x: i32,
    offset_y: i32,
    center_on_bookmark: bool,
    virtual_screen: BookmarkMarkerVirtualScreen,
) -> MarkerRect {
    let (client_x, client_y) = marker_client_origin(marker_x, marker_y, virtual_screen);
    let mut left = client_x + offset_x;
    let mut top = client_y + offset_y;
    if center_on_bookmark {
        left -= size_px / 2;
        top -= size_px / 2;
    }

    MarkerRect {
        left,
        top,
        right: left + size_px,
        bottom: top + size_px,
    }
}

#[allow(non_snake_case)]
fn RGB(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(((b as u32) << 16) | ((g as u32) << 8) | (r as u32))
}

fn colorref(color: RgbColor) -> COLORREF {
    RGB(color.red, color.green, color.blue)
}

fn alpha_from_opacity(opacity: f32) -> u8 {
    let opacity = if opacity.is_finite() { opacity } else { 1.0 };
    (opacity.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn view_alpha(view: &BookmarkMarkerOverlayView) -> u8 {
    view.markers
        .iter()
        .map(|marker| alpha_from_opacity(marker.opacity))
        .max()
        .unwrap_or(255)
}

pub struct BookmarkMarkerOverlay {
    hwnd: Option<HWND>,
    virtual_screen: BookmarkMarkerVirtualScreen,
    visible: bool,
}

impl BookmarkMarkerOverlay {
    pub fn new() -> Self {
        Self {
            hwnd: None,
            virtual_screen: BookmarkMarkerVirtualScreen::current(),
            visible: false,
        }
    }

    fn ensure_window(&mut self) -> Result<()> {
        if self.hwnd.is_some() {
            return Ok(());
        }

        unsafe {
            let hinstance = GetModuleHandleW(None)?;
            let cursor = LoadCursorW(None, IDC_ARROW).ok().unwrap_or_default();
            let wc = WNDCLASSW {
                lpfnWndProc: Some(bookmark_marker_overlay_proc),
                hInstance: hinstance.into(),
                lpszClassName: WINDOW_CLASS_NAME,
                hCursor: cursor,
                hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);

            let virtual_screen = BookmarkMarkerVirtualScreen::current();
            let hwnd = CreateWindowExW(
                bookmark_marker_overlay_ex_style(),
                WINDOW_CLASS_NAME,
                WINDOW_TITLE,
                bookmark_marker_overlay_style(),
                virtual_screen.left,
                virtual_screen.top,
                virtual_screen.width,
                virtual_screen.height,
                None,
                None,
                Some(HINSTANCE(hinstance.0)),
                Some(ptr::null_mut()),
            )?;

            SetLayeredWindowAttributes(
                hwnd,
                colorref(TRANSPARENT_COLOR),
                255,
                LWA_COLORKEY | LWA_ALPHA,
            )?;
            let _ = ShowWindow(hwnd, SW_HIDE);
            self.hwnd = Some(hwnd);
            self.virtual_screen = virtual_screen;
            self.visible = false;
        }

        Ok(())
    }

    pub fn show(&mut self, view: BookmarkMarkerOverlayView) -> Result<()> {
        if view.markers.is_empty() {
            self.hide();
            return Ok(());
        }
        self.render(view, true)
    }

    pub fn hide(&mut self) {
        if let Some(hwnd) = self.hwnd {
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
        self.visible = false;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn refresh(&mut self, view: BookmarkMarkerOverlayView) -> Result<()> {
        if view.markers.is_empty() {
            self.hide();
            return Ok(());
        }
        self.render(view, self.visible)
    }

    fn render(&mut self, view: BookmarkMarkerOverlayView, show_window: bool) -> Result<()> {
        self.ensure_window()?;
        let Some(hwnd) = self.hwnd else {
            return Ok(());
        };

        let screen = BookmarkMarkerVirtualScreen::current();
        self.virtual_screen = screen;

        unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                screen.left,
                screen.top,
                screen.width,
                screen.height,
                SWP_NOACTIVATE,
            )?;
            SetLayeredWindowAttributes(
                hwnd,
                colorref(TRANSPARENT_COLOR),
                view_alpha(&view),
                LWA_COLORKEY | LWA_ALPHA,
            )?;

            if show_window {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                self.visible = true;
            }

            self.draw(hwnd, &view)?;
        }

        Ok(())
    }

    unsafe fn draw(&self, hwnd: HWND, view: &BookmarkMarkerOverlayView) -> Result<()> {
        let hdc = GetDC(Some(hwnd));
        if hdc.is_invalid() {
            return Ok(());
        }

        let draw_result = (|| -> Result<()> {
            clear_window(hwnd, hdc)?;

            for marker in &view.markers {
                draw_marker(hdc, marker, view, self.virtual_screen)?;
            }

            let _ = InvalidateRect(Some(hwnd), None, false);
            let _ = UpdateWindow(hwnd);
            Ok(())
        })();

        let _ = ReleaseDC(Some(hwnd), hdc);
        draw_result
    }
}

impl Drop for BookmarkMarkerOverlay {
    fn drop(&mut self) {
        if let Some(hwnd) = self.hwnd.take() {
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
                let _ = DestroyWindow(hwnd);
            }
        }
        self.visible = false;
    }
}

unsafe fn clear_window(hwnd: HWND, hdc: HDC) -> Result<()> {
    let mut rect = RECT::default();
    GetClientRect(hwnd, &mut rect)?;
    let brush = CreateSolidBrush(colorref(TRANSPARENT_COLOR));
    if brush.is_invalid() {
        return Ok(());
    }
    let _ = FillRect(hdc, &rect, brush);
    let _ = DeleteObject(brush.into());
    Ok(())
}

unsafe fn draw_marker(
    hdc: HDC,
    marker: &BookmarkMarkerView,
    view: &BookmarkMarkerOverlayView,
    virtual_screen: BookmarkMarkerVirtualScreen,
) -> Result<()> {
    let rect = marker_rect(
        marker.x,
        marker.y,
        view.size_px,
        view.offset_x,
        view.offset_y,
        view.center_on_bookmark,
        virtual_screen,
    );

    let fill_brush = CreateSolidBrush(colorref(marker.fill_color));
    if fill_brush.is_invalid() {
        return Ok(());
    }

    let pen = if view.border_width_px > 0 {
        CreatePen(
            PS_SOLID,
            view.border_width_px,
            colorref(marker.border_color),
        )
    } else {
        HPEN(GetStockObject(NULL_PEN).0)
    };

    let old_brush = SelectObject(hdc, fill_brush.into());
    let old_pen = if pen.is_invalid() {
        SelectObject(hdc, GetStockObject(NULL_PEN))
    } else {
        SelectObject(hdc, pen.into())
    };

    match view.shape {
        BookmarkMarkerShape::Square => {
            let _ = Rectangle(hdc, rect.left, rect.top, rect.right, rect.bottom);
        }
        BookmarkMarkerShape::Circle => {
            let _ = Ellipse(hdc, rect.left, rect.top, rect.right, rect.bottom);
        }
    }

    let _ = SelectObject(hdc, old_pen);
    let _ = SelectObject(hdc, old_brush);
    let _ = DeleteObject(fill_brush.into());
    if view.border_width_px > 0 && !pen.is_invalid() {
        let _ = DeleteObject(pen.into());
    }

    draw_marker_label(hdc, marker, view, rect)
}

unsafe fn draw_marker_label(
    hdc: HDC,
    marker: &BookmarkMarkerView,
    view: &BookmarkMarkerOverlayView,
    rect: MarkerRect,
) -> Result<()> {
    let font_px = (14.0 * view.font_scale.max(0.2)).round().max(1.0) as i32;
    let font = CreateFontW(
        -font_px,
        0,
        0,
        0,
        FW_BOLD.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        DEFAULT_PITCH.0 as u32 | FF_DONTCARE.0 as u32,
        w!("Segoe UI"),
    );

    let old_font = if font.is_invalid() {
        HGDIOBJ::default()
    } else {
        SelectObject(hdc, font.into())
    };
    let _ = SetBkMode(hdc, TRANSPARENT);
    let _ = SetTextColor(hdc, colorref(marker.text_color));

    let mut text_rect = RECT {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    };
    let mut text: Vec<u16> = marker.label.encode_utf16().collect();
    let flags = DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX;
    let _ = DrawTextW(hdc, &mut text, &mut text_rect, flags);

    if !font.is_invalid() {
        let _ = SelectObject(hdc, old_font);
        let _ = DeleteObject(font.into());
    }

    Ok(())
}

extern "system" fn bookmark_marker_overlay_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_PAINT => unsafe {
            let mut ps = PAINTSTRUCT::default();
            let _ = BeginPaint(hwnd, &mut ps);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        },
        WM_ERASEBKGND => LRESULT(1),
        WM_DESTROY => LRESULT(0),
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn virtual_screen() -> BookmarkMarkerVirtualScreen {
        BookmarkMarkerVirtualScreen {
            left: -1920,
            top: -200,
            width: 3840,
            height: 2160,
        }
    }

    #[test]
    fn screen_to_overlay_client_conversion_handles_negative_virtual_origin() {
        assert_eq!(
            marker_client_origin(-1900, -150, virtual_screen()),
            (20, 50)
        );
    }

    #[test]
    fn marker_rect_uses_top_left_when_not_centering() {
        assert_eq!(
            marker_rect(-1900, -150, 32, 3, -4, false, virtual_screen()),
            MarkerRect {
                left: 23,
                top: 46,
                right: 55,
                bottom: 78,
            }
        );
    }

    #[test]
    fn marker_rect_centers_on_bookmark_coordinate() {
        assert_eq!(
            marker_rect(-1900, -150, 32, 3, -4, true, virtual_screen()),
            MarkerRect {
                left: 7,
                top: 30,
                right: 39,
                bottom: 62,
            }
        );
    }

    #[test]
    fn overlay_styles_are_click_through_topmost_and_no_activate() {
        let style = bookmark_marker_overlay_ex_style();
        assert_ne!(style & WS_EX_LAYERED, WINDOW_EX_STYLE(0));
        assert_ne!(style & WS_EX_TOPMOST, WINDOW_EX_STYLE(0));
        assert_ne!(style & WS_EX_TOOLWINDOW, WINDOW_EX_STYLE(0));
        assert_ne!(style & WS_EX_TRANSPARENT, WINDOW_EX_STYLE(0));
        assert_ne!(style & WS_EX_NOACTIVATE, WINDOW_EX_STYLE(0));
        assert_eq!(bookmark_marker_overlay_style(), WS_POPUP);
    }
}
