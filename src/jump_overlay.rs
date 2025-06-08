use std::ptr;
use std::sync::{Arc, Mutex};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::{Config, keyboard::VirtualKey, overlay::RGB};

lazy_static::lazy_static! {
    /// Global instance of the jump overlay
    pub static ref JUMP_OVERLAY: Arc<Mutex<JumpOverlay>> = Arc::new(Mutex::new(JumpOverlay::new()));
}

pub struct JumpOverlay {
    hwnd: Option<HWND>,
    grid_size: (u32, u32),
    visible: bool,
    input: String,
}

unsafe impl Send for JumpOverlay {}
unsafe impl Sync for JumpOverlay {}

impl JumpOverlay {
    pub fn new() -> Self {
        Self { hwnd: None, grid_size: (10, 10), visible: false, input: String::new() }
    }

    fn create_window(&mut self) {
        if self.hwnd.is_some() { return; }
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
                    SetLayeredWindowAttributes(h, COLORREF(0), 180, LWA_ALPHA);
                    ShowWindow(h, SW_HIDE);
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
    }

    pub fn show(&mut self) {
        if let Some(h) = self.hwnd {
            unsafe {
                ShowWindow(h, SW_SHOW);
                UpdateWindow(h);
            }
            self.visible = true;
        }
    }

    pub fn hide(&mut self) {
        if let Some(h) = self.hwnd {
            unsafe { ShowWindow(h, SW_HIDE); }
            self.visible = false;
        }
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
                if !GetClientRect(hwnd, &mut rect).as_bool() {
                    println!("GetClientRect failed: {:?}", GetLastError());
                    return;
                }
                let width = rect.right - rect.left;
                let height = rect.bottom - rect.top;
                let cell_w = width / self.grid_size.0 as i32;
                let cell_h = height / self.grid_size.1 as i32;

                let pen = CreatePen(PS_SOLID, 1, RGB(255, 255, 255));
                let old_pen = SelectObject(hdc, pen.into());
                if old_pen.0 == 0 {
                    println!("SelectObject failed: {:?}", GetLastError());
                    DeleteObject(pen.into());
                    return;
                }

                // draw vertical lines
                for x in 0..=self.grid_size.0 {
                    let pos = rect.left + (x as i32 * cell_w);
                    MoveToEx(hdc, pos, rect.top, None);
                    LineTo(hdc, pos, rect.bottom);
                }

                // draw horizontal lines
                for y in 0..=self.grid_size.1 {
                    let pos = rect.top + (y as i32 * cell_h);
                    MoveToEx(hdc, rect.left, pos, None);
                    LineTo(hdc, rect.right, pos);
                }

                // draw labels
                let row_len = Self::letters_needed(self.grid_size.1);
                let col_len = Self::letters_needed(self.grid_size.0);
                for row in 0..self.grid_size.1 {
                    let row_code = Self::index_to_code(row as usize, row_len);
                    for col in 0..self.grid_size.0 {
                        let col_code = Self::index_to_code(col as usize, col_len);
                        let code = format!("{}{}", row_code, col_code);
                        let mut text: Vec<u16> = code
                            .encode_utf16()
                            .chain(std::iter::once(0))
                            .collect();
                        let x = rect.left + col as i32 * cell_w + cell_w / 2 - 8;
                        let y = rect.top + row as i32 * cell_h + cell_h / 2 - 8;
                        TextOutW(
                            hdc,
                            x,
                            y,
                            PCWSTR(text.as_ptr()),
                            (text.len() - 1) as i32,
                        );
                    }
                }

                SelectObject(hdc, old_pen);
                DeleteObject(pen.into());
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

    pub fn handle_key(&mut self, key: VirtualKey) -> Option<(i32, i32)> {
        if let Some(ch) = key.to_char() {
            self.input.push(ch);
            println!("JumpOverlay sequence: {}", self.input);
            if let Some(hwnd) = self.hwnd {
                unsafe {
                    let hdc = GetDC(Some(hwnd));
                    if hdc.0 == 0 {
                        println!("GetDC failed: {:?}", GetLastError());
                    } else {
                        self.draw(hdc);
                        ReleaseDC(Some(hwnd), hdc);
                    }
                }
            }
            if self.input.len() >= self.expected_len() {
                let row_len = Self::letters_needed(self.grid_size.1);
                let row_code: Vec<char> = self.input.chars().take(row_len).collect();
                let col_code: Vec<char> = self.input.chars().skip(row_len).take(Self::letters_needed(self.grid_size.0)).collect();
                let row = Self::code_to_index(&row_code);
                let col = Self::code_to_index(&col_code);
                self.input.clear();
                self.hide();
                return self.target_position(row, col);
            }
        }
        None
    }
}

extern "system" fn jump_window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let ps = &mut PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, ps) };
            JUMP_OVERLAY.lock().unwrap().draw(hdc);
            unsafe { EndPaint(hwnd, ps) };
            LRESULT(0)
        }
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_DESTROY => LRESULT(0),
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

pub fn show_jump_overlay(config: &Config) {
    let mut ov = JUMP_OVERLAY.lock().unwrap();
    ov.initialize(config);
    ov.show();
}

pub fn hide_jump_overlay() {
    JUMP_OVERLAY.lock().unwrap().hide();
}
