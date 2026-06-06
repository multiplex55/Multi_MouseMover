use std::ptr;

use serde::Deserialize;
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::action::Action;
use crate::bookmarks::{BookmarkRecord, BookmarkStore};
use crate::keyboard::KeyBindings;
use crate::Config;

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BookmarkMarkerShape {
    #[default]
    Square,
    Circle,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BookmarkMarkerPositionSource {
    #[default]
    SavedCoordinate,
    ResolvedRecallTarget,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct BookmarkMarkerSlotStyle {
    pub fill_color: Option<String>,
    pub text_color: Option<String>,
    pub border_color: Option<String>,
    pub opacity: Option<f32>,
}

impl Default for BookmarkMarkerSlotStyle {
    fn default() -> Self {
        Self {
            fill_color: None,
            text_color: None,
            border_color: None,
            opacity: None,
        }
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct BookmarkMarkersConfig {
    pub enabled: bool,
    pub show_with_help: bool,
    pub show_with_show_bookmarks: bool,
    pub show_with_bookmark_mode: bool,
    pub filter_current_virtual_desktop: bool,
    pub hide_offscreen: bool,
    pub position_source: BookmarkMarkerPositionSource,
    pub shape: BookmarkMarkerShape,
    pub size_px: i32,
    pub opacity: f32,
    pub fill_color: String,
    pub text_color: String,
    pub border_color: String,
    pub border_width_px: i32,
    pub font_scale: f32,
    pub offset_x: i32,
    pub offset_y: i32,
    pub center_on_bookmark: bool,
    pub slot_styles: std::collections::HashMap<String, BookmarkMarkerSlotStyle>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtualScreenRect {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

impl VirtualScreenRect {
    pub fn current() -> Self {
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

#[derive(Debug, Clone, PartialEq)]
pub struct BookmarkMarkerOverlayView {
    pub markers: Vec<BookmarkMarkerView>,
    pub shape: BookmarkMarkerShape,
    pub size_px: i32,
    pub border_width_px: i32,
    pub font_scale: f32,
    pub offset_x: i32,
    pub offset_y: i32,
    pub center_on_bookmark: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BookmarkMarkerView {
    pub slot: u8,
    pub label: String,
    pub screen_x: i32,
    pub screen_y: i32,
    pub fill_color: RgbColor,
    pub text_color: RgbColor,
    pub border_color: RgbColor,
    pub opacity: f32,
}

pub fn build_bookmark_marker_overlay_view(
    config: &Config,
    store: &BookmarkStore,
    key_bindings: &KeyBindings,
    current_desktop_id: Option<&str>,
    virtual_screen: VirtualScreenRect,
    desktop_checker: impl Fn(&BookmarkRecord) -> bool,
) -> BookmarkMarkerOverlayView {
    let marker_cfg = &config.bookmark_markers;
    let mut markers = Vec::new();
    if marker_cfg.enabled {
        for slot in 1..=config.bookmarks.slot_count.min(u8::MAX as u32) as u8 {
            let Some(record) = store.get_slot(slot) else {
                continue;
            };
            let Some(label) = key_bindings.display_for_bookmark_slot(slot) else {
                continue;
            };
            if marker_cfg.filter_current_virtual_desktop {
                let Some(current) = current_desktop_id else {
                    continue;
                };
                if record.virtual_desktop_id.as_deref() != Some(current) {
                    continue;
                }
            }
            if record.anchor_hwnd.is_none() || !desktop_checker(record) {
                continue;
            }
            let (screen_x, screen_y) = match marker_cfg.position_source {
                BookmarkMarkerPositionSource::SavedCoordinate => (record.x, record.y),
                BookmarkMarkerPositionSource::ResolvedRecallTarget => {
                    crate::resolve_recall_target(record, &config.bookmarks)
                }
            };
            if marker_cfg.hide_offscreen
                && !point_inside_virtual_screen(screen_x, screen_y, virtual_screen)
            {
                continue;
            }
            let style = style_for_slot(slot, marker_cfg);
            markers.push(BookmarkMarkerView {
                slot,
                label,
                screen_x,
                screen_y,
                fill_color: style.fill_color,
                text_color: style.text_color,
                border_color: style.border_color,
                opacity: style.opacity,
            });
        }
    }
    BookmarkMarkerOverlayView {
        markers,
        shape: marker_cfg.shape,
        size_px: marker_cfg.size_px,
        border_width_px: marker_cfg.border_width_px,
        font_scale: marker_cfg.font_scale,
        offset_x: marker_cfg.offset_x,
        offset_y: marker_cfg.offset_y,
        center_on_bookmark: marker_cfg.center_on_bookmark,
    }
}

fn point_inside_virtual_screen(x: i32, y: i32, screen: VirtualScreenRect) -> bool {
    x >= screen.left
        && x < screen.left + screen.width
        && y >= screen.top
        && y < screen.top + screen.height
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct BookmarkMarkerResolvedStyle {
    fill_color: RgbColor,
    text_color: RgbColor,
    border_color: RgbColor,
    opacity: f32,
}

fn style_for_slot(slot: u8, config: &BookmarkMarkersConfig) -> BookmarkMarkerResolvedStyle {
    let override_style = config.slot_styles.get(&slot.to_string());
    BookmarkMarkerResolvedStyle {
        fill_color: parse_rgb(
            override_style
                .and_then(|s| s.fill_color.as_deref())
                .unwrap_or(&config.fill_color),
        ),
        text_color: parse_rgb(
            override_style
                .and_then(|s| s.text_color.as_deref())
                .unwrap_or(&config.text_color),
        ),
        border_color: parse_rgb(
            override_style
                .and_then(|s| s.border_color.as_deref())
                .unwrap_or(&config.border_color),
        ),
        opacity: override_style
            .and_then(|s| s.opacity)
            .unwrap_or(config.opacity),
    }
}

fn parse_rgb(value: &str) -> RgbColor {
    let parse = |idx| u8::from_str_radix(&value[idx..idx + 2], 16).unwrap_or(0);
    if value.len() == 7 && value.starts_with('#') {
        RgbColor {
            r: parse(1),
            g: parse(3),
            b: parse(5),
        }
    } else {
        RgbColor { r: 0, g: 0, b: 0 }
    }
}

fn screen_to_client(screen_x: i32, screen_y: i32, virtual_screen: VirtualScreenRect) -> (i32, i32) {
    (
        screen_x - virtual_screen.left,
        screen_y - virtual_screen.top,
    )
}

#[allow(non_snake_case)]
fn RGB(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(((b as u32) << 16) | ((g as u32) << 8) | (r as u32))
}

fn colorref(color: RgbColor) -> COLORREF {
    RGB(color.r, color.g, color.b)
}

pub struct BookmarkMarkerOverlay {
    hwnd: Option<HWND>,
    virtual_screen: VirtualScreenRect,
    visible: bool,
}

impl BookmarkMarkerOverlay {
    pub fn new() -> Self {
        Self {
            hwnd: None,
            virtual_screen: VirtualScreenRect::current(),
            visible: false,
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn ensure_window(&mut self) {
        if self.hwnd.is_some() {
            return;
        }
        unsafe {
            let hinstance = GetModuleHandleW(None).ok().unwrap_or_default();
            let class_name = w!("BookmarkMarkerOverlayWindowClass");
            let cursor = LoadCursorW(None, IDC_ARROW).ok().unwrap_or_default();
            let wc = WNDCLASSW {
                lpfnWndProc: Some(bookmark_marker_overlay_proc),
                hInstance: hinstance.into(),
                lpszClassName: class_name,
                hCursor: cursor,
                hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
            let screen = VirtualScreenRect::current();
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOACTIVATE,
                class_name,
                w!("Bookmark Marker Overlay"),
                WS_POPUP,
                screen.left,
                screen.top,
                screen.width,
                screen.height,
                None,
                None,
                Some(HINSTANCE(hinstance.0)),
                Some(ptr::null_mut()),
            );
            if let Ok(hwnd) = hwnd {
                let _ =
                    SetLayeredWindowAttributes(hwnd, RGB(0, 0, 0), 255, LWA_COLORKEY | LWA_ALPHA);
                self.hwnd = Some(hwnd);
                self.virtual_screen = screen;
                let _ = ShowWindow(hwnd, SW_HIDE);
                self.visible = false;
            }
        }
    }

    pub fn hide(&mut self) -> bool {
        let Some(hwnd) = self.hwnd else {
            self.visible = false;
            return true;
        };
        let hidden = unsafe { ShowWindow(hwnd, SW_HIDE).as_bool() };
        self.visible = false;
        hidden
    }

    pub fn render(&mut self, view: &BookmarkMarkerOverlayView) {
        self.ensure_window();
        let Some(hwnd) = self.hwnd else {
            return;
        };
        let screen = VirtualScreenRect::current();
        self.virtual_screen = screen;
        unsafe {
            let max_opacity = view
                .markers
                .iter()
                .map(|m| m.opacity)
                .fold(1.0_f32, f32::max);
            let alpha = (max_opacity.clamp(0.10, 1.00) * 255.0).round() as u8;
            let _ = SetLayeredWindowAttributes(hwnd, RGB(0, 0, 0), alpha, LWA_COLORKEY | LWA_ALPHA);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                screen.left,
                screen.top,
                screen.width,
                screen.height,
                SWP_NOACTIVATE,
            );
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            self.visible = true;
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
            let old_font = SelectObject(hdc, font.into());
            let _ = SetBkMode(hdc, TRANSPARENT);

            for marker in &view.markers {
                let (client_x, client_y) =
                    screen_to_client(marker.screen_x, marker.screen_y, screen);
                let mut left = client_x + view.offset_x;
                let mut top = client_y + view.offset_y;
                if view.center_on_bookmark {
                    left -= view.size_px / 2;
                    top -= view.size_px / 2;
                }
                let right = left + view.size_px;
                let bottom = top + view.size_px;
                let fill = CreateSolidBrush(colorref(marker.fill_color));
                let border = CreatePen(
                    PS_SOLID,
                    view.border_width_px,
                    colorref(marker.border_color),
                );
                let old_brush = SelectObject(hdc, fill.into());
                let old_pen = if view.border_width_px > 0 {
                    SelectObject(hdc, border.into())
                } else {
                    SelectObject(hdc, GetStockObject(NULL_PEN))
                };
                match view.shape {
                    BookmarkMarkerShape::Square => {
                        let _ = Rectangle(hdc, left, top, right, bottom);
                    }
                    BookmarkMarkerShape::Circle => {
                        let _ = Ellipse(hdc, left, top, right, bottom);
                    }
                }
                let _ = SelectObject(hdc, old_pen);
                let _ = SelectObject(hdc, old_brush);
                let _ = DeleteObject(border.into());
                let _ = DeleteObject(fill.into());

                let _ = SetTextColor(hdc, colorref(marker.text_color));
                let mut text: Vec<u16> = marker.label.encode_utf16().collect();
                let mut text_rect = RECT {
                    left,
                    top,
                    right,
                    bottom,
                };
                let _ = DrawTextW(
                    hdc,
                    &mut text,
                    &mut text_rect,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
            }

            let _ = SelectObject(hdc, old_font);
            let _ = DeleteObject(font.into());
            let _ = ReleaseDC(Some(hwnd), hdc);
            let _ = InvalidateRect(Some(hwnd), None, false);
            let _ = UpdateWindow(hwnd);
        }
    }
}

extern "system" fn bookmark_marker_overlay_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
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
    use crate::bookmarks::{BookmarkRecord, MonitorRect};
    use crate::key_chord::KeyChord;

    fn record(
        slot: u8,
        x: i32,
        y: i32,
        desktop: Option<&str>,
        anchor: Option<isize>,
    ) -> BookmarkRecord {
        BookmarkRecord {
            slot,
            name: Some("test".to_string()),
            x,
            y,
            monitor_device_name: "test".to_string(),
            monitor_rect: MonitorRect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            virtual_desktop_id: desktop.map(str::to_string),
            anchor_hwnd: anchor,
            anchor_process_id: None,
            anchor_window_title: None,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
        }
    }

    fn screen() -> VirtualScreenRect {
        VirtualScreenRect {
            left: 0,
            top: 0,
            width: 1920,
            height: 1080,
        }
    }

    fn base_config() -> Config {
        let mut cfg = Config::default();
        cfg.bookmark_markers.filter_current_virtual_desktop = false;
        cfg.bookmark_markers.slot_styles.clear();
        cfg
    }

    #[test]
    fn active_slots_only_and_labels_do_not_use_bookmark_names() {
        let cfg = base_config();
        let mut store = BookmarkStore::new(9);
        store.set_slot(1, record(1, 10, 20, None, Some(1)));
        store.set_slot(3, record(3, 30, 40, None, Some(3)));
        let mut keys = KeyBindings::new();
        keys.add_chord_binding_with_display(
            KeyChord::parse("1").unwrap(),
            Action::BookmarkSlot(1),
            "1",
        );
        keys.add_chord_binding_with_display(
            KeyChord::parse("3").unwrap(),
            Action::BookmarkSlot(3),
            "3",
        );

        let view =
            build_bookmark_marker_overlay_view(&cfg, &store, &keys, None, screen(), |_| true);

        assert_eq!(
            view.markers.iter().map(|m| m.slot).collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert_eq!(view.markers[0].label, "1");
        assert_ne!(view.markers[0].label, "test");
    }

    #[test]
    fn full_chord_shown_when_only_binding_and_numpad_distinct() {
        let cfg = base_config();
        let mut store = BookmarkStore::new(9);
        store.set_slot(1, record(1, 10, 20, None, Some(1)));
        store.set_slot(2, record(2, 30, 40, None, Some(2)));
        let mut keys = KeyBindings::new();
        keys.add_chord_binding_with_display(
            KeyChord::parse("Numpad1").unwrap(),
            Action::BookmarkSlot(1),
            "Numpad1",
        );
        keys.add_chord_binding_with_display(
            KeyChord::parse("RightAlt+1").unwrap(),
            Action::BookmarkSlot(2),
            "RightAlt+1",
        );

        let view =
            build_bookmark_marker_overlay_view(&cfg, &store, &keys, None, screen(), |_| true);

        assert_eq!(view.markers[0].label, "Numpad1");
        assert_eq!(view.markers[1].label, "RightAlt+1");
    }

    #[test]
    fn current_desktop_filter_hides_mismatches_and_missing_ids() {
        let mut cfg = base_config();
        cfg.bookmark_markers.filter_current_virtual_desktop = true;
        let mut store = BookmarkStore::new(9);
        store.set_slot(1, record(1, 10, 20, Some("desktop-a"), Some(1)));
        store.set_slot(2, record(2, 30, 40, Some("desktop-b"), Some(2)));
        store.set_slot(3, record(3, 50, 60, None, Some(3)));
        let mut keys = KeyBindings::new();
        for slot in 1..=3 {
            keys.add_chord_binding_with_display(
                KeyChord::parse(&slot.to_string()).unwrap(),
                Action::BookmarkSlot(slot),
                slot.to_string(),
            );
        }

        let view = build_bookmark_marker_overlay_view(
            &cfg,
            &store,
            &keys,
            Some("desktop-a"),
            screen(),
            |_| true,
        );

        assert_eq!(
            view.markers.iter().map(|m| m.slot).collect::<Vec<_>>(),
            vec![1]
        );
    }

    #[test]
    fn anchor_unavailable_and_offscreen_bookmarks_are_hidden() {
        let cfg = base_config();
        let mut store = BookmarkStore::new(9);
        store.set_slot(1, record(1, 10, 20, None, None));
        store.set_slot(2, record(2, 2500, 500, None, Some(2)));
        store.set_slot(3, record(3, 100, 100, None, Some(3)));
        let mut keys = KeyBindings::new();
        for slot in 1..=3 {
            keys.add_chord_binding_with_display(
                KeyChord::parse(&slot.to_string()).unwrap(),
                Action::BookmarkSlot(slot),
                slot.to_string(),
            );
        }

        let view = build_bookmark_marker_overlay_view(&cfg, &store, &keys, None, screen(), |r| {
            r.slot != 3
        });

        assert!(view.markers.is_empty());
    }

    #[test]
    fn per_slot_color_override_is_applied() {
        let mut cfg = base_config();
        cfg.bookmark_markers.fill_color = "#FFD400".to_string();
        cfg.bookmark_markers.slot_styles.insert(
            "2".to_string(),
            BookmarkMarkerSlotStyle {
                fill_color: Some("#00E676".to_string()),
                ..BookmarkMarkerSlotStyle::default()
            },
        );
        let mut store = BookmarkStore::new(9);
        store.set_slot(1, record(1, 10, 20, None, Some(1)));
        store.set_slot(2, record(2, 30, 40, None, Some(2)));
        let mut keys = KeyBindings::new();
        keys.add_chord_binding_with_display(
            KeyChord::parse("1").unwrap(),
            Action::BookmarkSlot(1),
            "1",
        );
        keys.add_chord_binding_with_display(
            KeyChord::parse("2").unwrap(),
            Action::BookmarkSlot(2),
            "2",
        );

        let view =
            build_bookmark_marker_overlay_view(&cfg, &store, &keys, None, screen(), |_| true);

        assert_eq!(
            view.markers[0].fill_color,
            RgbColor {
                r: 0xFF,
                g: 0xD4,
                b: 0x00
            }
        );
        assert_eq!(
            view.markers[1].fill_color,
            RgbColor {
                r: 0x00,
                g: 0xE6,
                b: 0x76
            }
        );
    }
}
