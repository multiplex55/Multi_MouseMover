use std::cell::RefCell;
use std::ptr;
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::{keyboard::VirtualKey, overlay::RGB, Config};

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

fn calculate_target_center(
    screen_rect: ScreenRect,
    grid_size: (u32, u32),
    row: usize,
    col: usize,
) -> Option<(i32, i32)> {
    let (cols, rows) = grid_size;
    if cols == 0 || rows == 0 {
        return None;
    }
    if row >= rows as usize || col >= cols as usize {
        return None;
    }

    let x = screen_rect.left as f64
        + (((col as f64) + 0.5f64) * screen_rect.width as f64 / cols as f64);
    let y = screen_rect.top as f64
        + (((row as f64) + 0.5f64) * screen_rect.height as f64 / rows as f64);

    Some((x.round() as i32, y.round() as i32))
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
        }
    }
}

fn format_jump_indicator(input: &str) -> String {
    format!("Jump: {}_", input)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JumpKeyResult {
    Ignored,
    Consumed,
    Cancelled,
    Completed { x: i32, y: i32 },
    Invalid,
}

pub struct JumpOverlay {
    hwnd: Option<HWND>,
    grid_size: (u32, u32),
    visible: bool,
    input: String,
    repaint_requested: bool,
}

impl JumpOverlay {
    pub fn new() -> Self {
        Self {
            hwnd: None,
            grid_size: (10, 10),
            visible: false,
            input: String::new(),
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

    pub fn initialize(&mut self, config: &Config) {
        self.grid_size = (config.grid_size.width, config.grid_size.height);
        self.create_window();
        self.input.clear();
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
        self.input.clear();
        if let Some(h) = self.hwnd {
            unsafe {
                let _ = ShowWindow(h, SW_HIDE);
            }
        }
        self.visible = false;
    }

    fn draw(&self, hdc: HDC) {
        if let Some(hwnd) = self.hwnd {
            if self.grid_size.0 == 0 || self.grid_size.1 == 0 {
                return;
            }
            unsafe {
                let mut rect = RECT::default();
                if GetClientRect(hwnd, &mut rect).is_err() {
                    return;
                }
                let width = rect.right - rect.left;
                let height = rect.bottom - rect.top;
                let cell_w = width / self.grid_size.0 as i32;
                let cell_h = height / self.grid_size.1 as i32;

                let bg_brush = CreateSolidBrush(overlay_colorkey());
                let _ = FillRect(hdc, &rect, bg_brush);

                let old_bk_mode = SetBkMode(hdc, TRANSPARENT);
                let old_text_color = SetTextColor(hdc, label_color());

                let pen = CreatePen(PS_SOLID, 1, grid_color());
                let old_pen = SelectObject(hdc, pen.into());

                for x in 0..=self.grid_size.0 {
                    let pos = rect.left + (x as i32 * cell_w);
                    let _ = MoveToEx(hdc, pos, rect.top, None);
                    let _ = LineTo(hdc, pos, rect.bottom);
                }
                for y in 0..=self.grid_size.1 {
                    let pos = rect.top + (y as i32 * cell_h);
                    let _ = MoveToEx(hdc, rect.left, pos, None);
                    let _ = LineTo(hdc, rect.right, pos);
                }

                let row_len = Self::letters_needed(self.grid_size.1);
                let col_len = Self::letters_needed(self.grid_size.0);
                for row in 0..self.grid_size.1 {
                    let row_code = Self::index_to_code(row as usize, row_len);
                    for col in 0..self.grid_size.0 {
                        let col_code = Self::index_to_code(col as usize, col_len);
                        let code = format!("{}{}", row_code, col_code);
                        let text: Vec<u16> = code.encode_utf16().collect();
                        let x = rect.left + col as i32 * cell_w + cell_w / 2 - 8;
                        let y = rect.top + row as i32 * cell_h + cell_h / 2 - 8;
                        let _ = TextOutW(hdc, x, y, &text);
                    }
                }

                let _ = SetTextColor(hdc, input_color());
                let indicator = format_jump_indicator(&self.input);
                let indicator_utf16: Vec<u16> = indicator.encode_utf16().collect();
                let indicator_x = rect.left + (width / 2) - 60;
                let indicator_y = rect.top + 16;
                let _ = TextOutW(hdc, indicator_x, indicator_y, &indicator_utf16);

                let _ = SetTextColor(hdc, old_text_color);
                let _ = SetBkMode(hdc, BACKGROUND_MODE(old_bk_mode as u32));
                let _ = SelectObject(hdc, old_pen);
                let _ = DeleteObject(pen.into());
                let _ = DeleteObject(bg_brush.into());
            }
        }
    }

    fn letters_needed(value: u32) -> usize {
        if value <= 26 {
            1
        } else if value <= 26 * 26 {
            2
        } else {
            3
        }
    }

    fn expected_len(&self) -> usize {
        Self::letters_needed(self.grid_size.1) + Self::letters_needed(self.grid_size.0)
    }

    fn code_to_index(code: &[char]) -> usize {
        let mut idx = 0usize;
        for &ch in code {
            idx = idx * 26 + ((ch as u8 - b'A') as usize);
        }
        idx
    }

    fn index_to_code(mut index: usize, len: usize) -> String {
        let mut chars = vec!['A'; len];
        for i in (0..len).rev() {
            chars[i] = (b'A' + (index % 26) as u8) as char;
            index /= 26;
        }
        chars.into_iter().collect()
    }

    fn target_position(&self, row: usize, col: usize) -> Option<(i32, i32)> {
        calculate_target_center(ScreenRect::from_virtual_screen(), self.grid_size, row, col)
    }

    pub fn handle_key(&mut self, key: VirtualKey, is_keydown: bool) -> JumpKeyResult {
        if !is_keydown {
            return JumpKeyResult::Ignored;
        }
        match key {
            VirtualKey::Escape => {
                self.input.clear();
                JumpKeyResult::Cancelled
            }
            VirtualKey::Backspace => {
                self.input.pop();
                self.request_repaint();
                JumpKeyResult::Consumed
            }
            _ => {
                if let Some(ch) = key.to_char() {
                    self.input.push(ch);
                    self.request_repaint();
                    if self.input.len() >= self.expected_len() {
                        let row_len = Self::letters_needed(self.grid_size.1);
                        let row_code: Vec<char> = self.input.chars().take(row_len).collect();
                        let col_code: Vec<char> = self
                            .input
                            .chars()
                            .skip(row_len)
                            .take(Self::letters_needed(self.grid_size.0))
                            .collect();
                        let row = Self::code_to_index(&row_code);
                        let col = Self::code_to_index(&col_code);
                        if let Some((x, y)) = self.target_position(row, col) {
                            self.input.clear();
                            self.hide();
                            return JumpKeyResult::Completed { x, y };
                        }
                        self.input.clear();
                        self.request_repaint();
                        return JumpKeyResult::Invalid;
                    }
                    JumpKeyResult::Consumed
                } else {
                    JumpKeyResult::Ignored
                }
            }
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

pub fn show_jump_overlay(config: &Config) {
    JUMP_OVERLAY.with(|overlay| {
        let mut ov = overlay.borrow_mut();
        ov.initialize(config);
        ov.show();
    });
}

pub fn hide_jump_overlay() {
    JUMP_OVERLAY.with(|overlay| overlay.borrow_mut().hide());
}

#[cfg(test)]
mod tests {
    use super::{
        calculate_target_center, format_jump_indicator, overlay_colorkey, overlay_ex_style, transparency_mode,
        JumpKeyResult, JumpOverlay, ScreenRect, TransparencyMode, OVERLAY_ALPHA,
    };
    use crate::keyboard::VirtualKey;
    use windows::Win32::UI::WindowsAndMessaging::{WS_EX_NOACTIVATE, WS_EX_TRANSPARENT};

    #[test]
    fn style_contains_non_activate_and_click_through_bits() {
        let style = overlay_ex_style();
        assert!(style.contains(WS_EX_NOACTIVATE));
        assert!(style.contains(WS_EX_TRANSPARENT));
    }

    #[test]
    fn jump_indicator_formatting() {
        assert_eq!(format_jump_indicator(""), "Jump: _");
        assert_eq!(format_jump_indicator("A"), "Jump: A_");
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
    fn key_up_is_ignored() {
        let mut overlay = JumpOverlay::new();
        assert_eq!(
            overlay.handle_key(VirtualKey::A, false),
            JumpKeyResult::Ignored
        );
        assert!(overlay.input.is_empty());
    }

    #[test]
    fn target_center_handles_non_zero_virtual_screen_origin() {
        let rect = ScreenRect {
            left: -1920,
            top: 120,
            width: 3840,
            height: 2160,
        };
        assert_eq!(calculate_target_center(rect, (4, 3), 1, 2), Some((480, 1200)));
    }

    #[test]
    fn target_center_uses_edge_cell_centers() {
        let rect = ScreenRect {
            left: 100,
            top: 200,
            width: 1000,
            height: 800,
        };
        assert_eq!(calculate_target_center(rect, (10, 8), 0, 0), Some((150, 250)));
        assert_eq!(calculate_target_center(rect, (10, 8), 7, 9), Some((1050, 950)));
    }

    #[test]
    fn target_center_rejects_invalid_row_or_col() {
        let rect = ScreenRect {
            left: 0,
            top: 0,
            width: 1920,
            height: 1080,
        };
        assert_eq!(calculate_target_center(rect, (10, 10), 10, 0), None);
        assert_eq!(calculate_target_center(rect, (10, 10), 0, 10), None);
    }
}
