use std::ptr;
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::ui_hints::UiHintSession;

pub fn overlay_ex_style() -> WINDOW_EX_STYLE {
    WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE
}

pub fn overlay_style() -> WINDOW_STYLE {
    WS_POPUP
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiHintLabelPlacement {
    Anchor,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiHintOverlayView {
    pub placement: UiHintLabelPlacement,
    pub input: String,
    pub targets: Vec<UiHintOverlayTarget>,
    pub dim_non_matching: bool,
    pub font_scale: f32,
    pub offset_x: i32,
    pub offset_y: i32,
    pub show_background: bool,
    pub show_border: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiHintOverlayTarget {
    pub id: u64,
    pub label: String,
    pub screen_x: i32,
    pub screen_y: i32,
    pub matches_input: bool,
    pub exact_match: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VirtualScreen {
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

impl VirtualScreen {
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

pub fn build_ui_hint_overlay_view(
    session: &UiHintSession,
    font_scale: f32,
    offset_x: i32,
    offset_y: i32,
    dim_non_matching: bool,
    show_background: bool,
    show_border: bool,
) -> UiHintOverlayView {
    let input = session.input.clone();
    let targets = session
        .targets
        .iter()
        .map(|target| {
            let matches_input = input.is_empty() || target.label.starts_with(&input);
            let exact_match = !input.is_empty() && target.label == input;
            UiHintOverlayTarget {
                id: target.id,
                label: target.label.clone(),
                screen_x: target.target_x,
                screen_y: target.target_y,
                matches_input,
                exact_match,
            }
        })
        .collect();

    UiHintOverlayView {
        placement: UiHintLabelPlacement::Anchor,
        input,
        targets,
        dim_non_matching,
        font_scale,
        offset_x,
        offset_y,
        show_background,
        show_border,
    }
}

fn screen_to_client(screen_x: i32, screen_y: i32, virtual_screen: VirtualScreen) -> (i32, i32) {
    (screen_x - virtual_screen.left, screen_y - virtual_screen.top)
}

#[allow(non_snake_case)]
fn RGB(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(((b as u32) << 16) | ((g as u32) << 8) | (r as u32))
}

pub struct UiHintOverlay {
    hwnd: Option<HWND>,
    virtual_screen: VirtualScreen,
}

impl UiHintOverlay {
    pub fn new() -> Self {
        Self {
            hwnd: None,
            virtual_screen: VirtualScreen::current(),
        }
    }

    pub fn ensure_window(&mut self) {
        if self.hwnd.is_some() {
            return;
        }

        unsafe {
            let hinstance = GetModuleHandleW(None).ok().unwrap_or_default();
            let class_name = w!("UiHintOverlayWindowClass");
            let cursor = LoadCursorW(None, IDC_ARROW).ok().unwrap_or_default();
            let wc = WNDCLASSW {
                lpfnWndProc: Some(ui_hint_overlay_proc),
                hInstance: hinstance.into(),
                lpszClassName: class_name,
                hCursor: cursor,
                hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);

            let virtual_screen = VirtualScreen::current();
            let hwnd = CreateWindowExW(
                overlay_ex_style(),
                class_name,
                w!("Ui Hint Overlay"),
                overlay_style(),
                virtual_screen.left,
                virtual_screen.top,
                virtual_screen.width,
                virtual_screen.height,
                None,
                None,
                Some(HINSTANCE(hinstance.0)),
                Some(ptr::null_mut()),
            );

            if let Ok(hwnd) = hwnd {
                let _ = SetLayeredWindowAttributes(hwnd, RGB(0, 0, 0), 255, LWA_COLORKEY);
                self.hwnd = Some(hwnd);
                self.virtual_screen = virtual_screen;
            }
        }
    }

    pub fn render(&self, view: &UiHintOverlayView) {
        let Some(hwnd) = self.hwnd else {
            return;
        };
        unsafe {
            let hdc = GetDC(Some(hwnd));
            if hdc.is_invalid() {
                return;
            }

            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            let brush = CreateSolidBrush(RGB(0, 0, 0));
            let _ = FillRect(hdc, &rect, brush);
            let _ = DeleteObject(brush.into());

            let font_px = (18.0 * view.font_scale.max(0.2)).round() as i32;
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
            let old = SelectObject(hdc, font.into());
            let _ = SetBkMode(hdc, TRANSPARENT);

            for target in &view.targets {
                let (client_x, client_y) = screen_to_client(target.screen_x, target.screen_y, self.virtual_screen);
                let x = client_x + view.offset_x;
                let y = client_y + view.offset_y;

                let text: Vec<u16> = target.label.encode_utf16().chain(std::iter::once(0)).collect();
                let mut text_rect = RECT {
                    left: x,
                    top: y,
                    right: x + 120,
                    bottom: y + font_px + 10,
                };

                if view.show_background {
                    let bg = CreateSolidBrush(RGB(20, 20, 20));
                    let _ = FillRect(hdc, &text_rect, bg);
                    let _ = DeleteObject(bg.into());
                }
                if view.show_border {
                    let border = CreatePen(PS_SOLID, 1, RGB(120, 120, 120));
                    let old_pen = SelectObject(hdc, border.into());
                    let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH));
                    let _ = Rectangle(hdc, text_rect.left, text_rect.top, text_rect.right, text_rect.bottom);
                    let _ = SelectObject(hdc, old_pen);
                    let _ = SelectObject(hdc, old_brush);
                    let _ = DeleteObject(border.into());
                }

                let color = if target.exact_match {
                    RGB(0, 255, 255)
                } else if target.matches_input {
                    RGB(255, 255, 0)
                } else if view.dim_non_matching {
                    RGB(120, 120, 120)
                } else {
                    RGB(255, 255, 0)
                };
                let _ = SetTextColor(hdc, color);
                let _ = TextOutW(hdc, text_rect.left + 4, text_rect.top + 2, &text[..text.len().saturating_sub(1)]);
            }

            let _ = SelectObject(hdc, old);
            let _ = DeleteObject(font.into());
            let _ = ReleaseDC(Some(hwnd), hdc);
        }
    }
}

extern "system" fn ui_hint_overlay_proc(
    hwnd: HWND,
    msg: u32,
    _wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_PAINT => unsafe {
            let mut ps = PAINTSTRUCT::default();
            let _ = BeginPaint(hwnd, &mut ps);
            EndPaint(hwnd, &ps);
            LRESULT(0)
        },
        WM_ERASEBKGND => LRESULT(1),
        WM_DESTROY => LRESULT(0),
        _ => unsafe { DefWindowProcW(hwnd, msg, _wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_hints::{UiHintSession, UiHintTarget};

    #[test]
    fn matching_labels_marked_for_input_prefix() {
        let session = UiHintSession {
            targets: vec![target(1, "AA", 10, 10), target(2, "AB", 20, 20), target(3, "BC", 30, 30)],
            input: "A".to_string(),
            selection_keys: vec!['A', 'B', 'C'],
        };

        let view = build_ui_hint_overlay_view(&session, 1.0, 0, 0, true, true, true);
        assert!(view.targets[0].matches_input);
        assert!(view.targets[1].matches_input);
        assert!(!view.targets[2].matches_input);
    }

    #[test]
    fn non_matching_labels_dimmed_when_enabled() {
        let session = UiHintSession {
            targets: vec![target(1, "AA", 10, 10), target(2, "BC", 20, 20)],
            input: "A".to_string(),
            selection_keys: vec!['A', 'B', 'C'],
        };

        let view = build_ui_hint_overlay_view(&session, 1.0, 0, 0, true, true, false);
        assert!(view.dim_non_matching);
        assert!(view.targets[0].matches_input);
        assert!(!view.targets[1].matches_input);
    }

    #[test]
    fn screen_to_client_translation_handles_negative_virtual_coords() {
        let virtual_screen = VirtualScreen {
            left: -1920,
            top: -200,
            width: 3840,
            height: 2160,
        };
        assert_eq!(screen_to_client(-1900, -100, virtual_screen), (20, 100));
    }

    fn target(id: u64, label: &str, x: i32, y: i32) -> UiHintTarget {
        UiHintTarget {
            id,
            label: label.to_string(),
            bounds: (0, 0, 10, 10),
            target_x: x,
            target_y: y,
            metadata: None,
        }
    }
}
