use std::ptr;
use std::sync::{Arc, Mutex};
use windows::core::w;
use windows::Win32::{
    Foundation::*, Graphics::Gdi::*, System::LibraryLoader::*, UI::WindowsAndMessaging::*,
};

lazy_static::lazy_static! {
    /// Global overlay instance wrapped in `Arc<Mutex<OverlayWindow>>`
    pub static ref OVERLAY: Arc<Mutex<OverlayWindow>> = Arc::new(Mutex::new(OverlayWindow::new()));
}

pub struct OverlayWindow {
    hwnd: Arc<Mutex<Option<isize>>>, // ✅ Store HWND as `isize` to avoid `Send` issues
    is_green: bool,
}

impl OverlayWindow {
    /// Creates the overlay window
    pub fn new() -> Self {
        let h_instance = unsafe { GetModuleHandleW(None).unwrap() };

        // ✅ Register window class
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: h_instance.into(),
            lpszClassName: w!("OverlayClass"),
            style: CS_HREDRAW | CS_VREDRAW,
            hbrBackground: HBRUSH(ptr::null_mut()),
            ..Default::default()
        };
        unsafe { RegisterClassW(&wc) };

        // ✅ Create window
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                w!("OverlayClass"),
                w!("OverlayWindow"),
                WS_POPUP,
                50,
                50,
                100,
                100, // (x, y, width, height)
                None,
                None,
                Some(h_instance.into()), // ✅ Convert `HMODULE` to `HINSTANCE`
                None,
            )
        };

        // ✅ Store HWND as `isize` for thread safety
        let hwnd_ptr = hwnd.ok().map(|h| h.0 as isize);

        // ✅ Set transparency if window is created successfully
        if let Some(h) = hwnd_ptr {
            unsafe {
                SetLayeredWindowAttributes(HWND(h as *mut _), COLORREF(0), 255, LWA_ALPHA);
            }
        }

        Self {
            hwnd: Arc::new(Mutex::new(hwnd_ptr)), // ✅ Store HWND as `isize`
            is_green: false,
        }
    }

    /// Updates the color of the square
    pub fn update_color(&mut self, is_green: bool) {
        self.is_green = is_green;
        self.repaint();
    }

    /// Repaints the square
    pub fn repaint(&self) {
        let hwnd_lock = self.hwnd.lock().unwrap();
        if let Some(h) = *hwnd_lock {
            let hwnd = HWND(h as *mut _); // ✅ Convert `isize` back to `HWND`
            unsafe {
                let hdc = GetDC(Some(hwnd));
                let color = if self.is_green {
                    RGB(0, 255, 0) // Green when left-click is pressed
                } else {
                    RGB(255, 0, 0) // Red otherwise
                };
                let hbrush = CreateSolidBrush(color);

                let rect = RECT {
                    left: 0,
                    top: 0,
                    right: 100,
                    bottom: 100,
                };
                FillRect(hdc, &rect, hbrush);

                DeleteObject(hbrush.into());
                ReleaseDC(Some(hwnd), hdc);
            }
        }
    }
}

/// Helper function to create a `COLORREF`
fn RGB(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(((b as u32) << 16) | ((g as u32) << 8) | (r as u32))
}

/// Window procedure for overlay
extern "system" fn window_proc(hwnd: HWND, msg: u32, _wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            OVERLAY.lock().unwrap().repaint();
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, _wparam, _lparam) },
    }
}
