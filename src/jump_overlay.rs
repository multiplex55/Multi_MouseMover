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
            let width = GetSystemMetrics(SM_CXSCREEN);
            let height = GetSystemMetrics(SM_CYSCREEN);
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                class,
                w!("JumpOverlay"),
                WS_POPUP,
                0,
                0,
                width,
                height,
                None,
                None,
                Some(h_instance.into()),
                None,
            );
            match hwnd {
                Ok(h) => {
                    let _ = SetLayeredWindowAttributes(h, COLORREF(0), 180, LWA_ALPHA);
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
                // Use non-activating show mode so jump overlay never steals focus.
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
                println!(
                    "JumpOverlay::draw aborted due to zero grid size: ({}, {})",
                    self.grid_size.0, self.grid_size.1
                );
                return;
            }
            unsafe {
                let mut rect = RECT::default();
                if GetClientRect(hwnd, &mut rect).is_err() {
                    println!("GetClientRect failed: {:?}", GetLastError());
                    return;
                }
                let width = rect.right - rect.left;
                let height = rect.bottom - rect.top;
                let cell_w = width / self.grid_size.0 as i32;
                let cell_h = height / self.grid_size.1 as i32;

                let pen = CreatePen(PS_SOLID, 1, RGB(255, 255, 255));
                let old_pen = SelectObject(hdc, pen.into());
                if old_pen.0.is_null() {
                    println!("SelectObject failed: {:?}", GetLastError());
                    let _ = DeleteObject(pen.into());
                    return;
                }

                // draw vertical lines
                for x in 0..=self.grid_size.0 {
                    let pos = rect.left + (x as i32 * cell_w);
                    let _ = MoveToEx(hdc, pos, rect.top, None);
                    let _ = LineTo(hdc, pos, rect.bottom);
                }

                // draw horizontal lines
                for y in 0..=self.grid_size.1 {
                    let pos = rect.top + (y as i32 * cell_h);
                    let _ = MoveToEx(hdc, rect.left, pos, None);
                    let _ = LineTo(hdc, rect.right, pos);
                }

                // draw labels
                let row_len = Self::letters_needed(self.grid_size.1);
                let col_len = Self::letters_needed(self.grid_size.0);
                for row in 0..self.grid_size.1 {
                    let row_code = Self::index_to_code(row as usize, row_len);
                    for col in 0..self.grid_size.0 {
                        let col_code = Self::index_to_code(col as usize, col_len);
                        let code = format!("{}{}", row_code, col_code);
                        let text: Vec<u16> =
                            code.encode_utf16().chain(std::iter::once(0)).collect();
                        let x = rect.left + col as i32 * cell_w + cell_w / 2 - 8;
                        let y = rect.top + row as i32 * cell_h + cell_h / 2 - 8;
                        let _ = TextOutW(hdc, x, y, &text[..text.len() - 1]);
                    }
                }

                SelectObject(hdc, old_pen);
                let _ = DeleteObject(pen.into());
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
        let width = unsafe { GetSystemMetrics(SM_CXSCREEN) } as i32;
        let height = unsafe { GetSystemMetrics(SM_CYSCREEN) } as i32;
        let cell_w = width / self.grid_size.0 as i32;
        let cell_h = height / self.grid_size.1 as i32;
        if row < self.grid_size.1 as usize && col < self.grid_size.0 as usize {
            let x = col as i32 * cell_w + cell_w / 2;
            let y = row as i32 * cell_h + cell_h / 2;
            Some((x, y))
        } else {
            None
        }
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
                    println!("JumpOverlay sequence: {}", self.input);
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
                    return JumpKeyResult::Consumed;
                }

                // Policy: unsupported keys are ignored so they do not mutate jump input state.
                JumpKeyResult::Ignored
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
            };
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
    fn escape_cancels() {
        let mut overlay = JumpOverlay::new();
        overlay.input.push('A');
        assert_eq!(
            overlay.handle_key(VirtualKey::Escape, true),
            JumpKeyResult::Cancelled
        );
        assert!(overlay.input.is_empty());
    }

    #[test]
    fn valid_two_letter_code_completes() {
        let mut overlay = JumpOverlay::new();
        overlay.grid_size = (10, 10);
        assert_eq!(
            overlay.handle_key(VirtualKey::A, true),
            JumpKeyResult::Consumed
        );
        match overlay.handle_key(VirtualKey::A, true) {
            JumpKeyResult::Completed { .. } => {}
            other => panic!("expected completed, got {:?}", other),
        }
        assert!(overlay.input.is_empty());
        assert!(!overlay.visible);
    }

    #[test]
    fn invalid_code_returns_invalid_and_stays_visible() {
        let mut overlay = JumpOverlay::new();
        overlay.grid_size = (27, 10);
        overlay.visible = true;
        assert_eq!(
            overlay.handle_key(VirtualKey::A, true),
            JumpKeyResult::Consumed
        );
        assert_eq!(
            overlay.handle_key(VirtualKey::Z, true),
            JumpKeyResult::Invalid
        );
        assert!(overlay.visible);
        assert!(overlay.input.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::{JumpKeyResult, JumpOverlay};
    use crate::keyboard::VirtualKey;

    #[test]
    fn hiding_clears_input_state() {
        let mut overlay = JumpOverlay::new();
        overlay.input.push('A');
        overlay.visible = true;
        overlay.hide();
        assert!(overlay.input.is_empty());
        assert!(!overlay.visible);
    }

    #[test]
    fn key_input_requests_repaint() {
        let mut overlay = JumpOverlay::new();
        assert!(!overlay.repaint_requested);
        let _ = overlay.handle_key(VirtualKey::A, true);
        assert!(overlay.repaint_requested);
    }

    #[test]
    fn key_handling_does_not_require_window_for_repaint_state() {
        let mut overlay = JumpOverlay::new();
        overlay.hwnd = None;
        let _ = overlay.handle_key(VirtualKey::B, true);
        assert_eq!(overlay.input, "B");
        assert!(overlay.repaint_requested);
    }
}
