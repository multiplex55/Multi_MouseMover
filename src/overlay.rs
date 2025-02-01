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
    hwnd: Arc<Mutex<Option<isize>>>, // ✅ Store HWND as `isize`
    is_green: bool,
}

impl OverlayWindow {
    /// Creates the overlay window
    pub fn new() -> Self {
        println!("🚀 Overlay: Starting Initialization");

        let h_instance = unsafe { GetModuleHandleW(None).unwrap() };
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

        // Create window
        println!("🔹 Overlay: Creating Overlay Window...");
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                w!("OverlayClass"),
                w!("OverlayWindow"),
                WS_POPUP,
                50,  // Default X position
                50,  // Default Y position
                100, // Width
                100, // Height
                None,
                None,
                Some(h_instance.into()),
                None,
            )
        };

        if hwnd.is_err() {
            panic!("❌ Overlay: Failed to create overlay window!");
        }
        println!("✅ Overlay: Window Created Successfully!");

        // Store HWND as `isize`
        let hwnd_ptr = hwnd.ok().map(|h| h.0 as isize);
        println!("🔹 Overlay: HWND Stored as isize");

        // Ensure window is visible
        if let Some(h) = hwnd_ptr {
            unsafe {
                println!("🔹 Overlay: Showing Window...");
                ShowWindow(HWND(h as *mut _), SW_SHOW);
                println!("🔹 Overlay: Updating Window...");
                UpdateWindow(HWND(h as *mut _));
                println!("🔹 Overlay: Setting Layered Window Attributes...");
                SetLayeredWindowAttributes(HWND(h as *mut _), COLORREF(0), 255, LWA_ALPHA);
            }
        }

        println!("✅ Overlay: Initialization Completed!");
        Self {
            hwnd: Arc::new(Mutex::new(hwnd_ptr)),
            is_green: false,
        }
    }

    /// Moves the overlay to follow the mouse cursor
    pub fn move_to_mouse(&self) {
        let hwnd_lock = self.hwnd.lock().unwrap();
        if let Some(h) = *hwnd_lock {
            let hwnd = HWND(h as *mut _);
            unsafe {
                let mut point = POINT::default();
                if unsafe { GetCursorPos(&mut point) }.is_ok() {
                    let x = point.x + 20; // Offset the overlay to the right of the cursor
                    let y = point.y + 20; // Offset the overlay below the cursor

                    println!("🖱 Overlay Moving to: ({}, {})", x, y);

                    SetWindowPos(
                        hwnd,
                        Some(HWND_TOPMOST),
                        x,
                        y,
                        100,
                        100,
                        SWP_NOZORDER | SWP_NOSIZE | SWP_SHOWWINDOW,
                    );
                }
            }
        }
    }

    /// Updates the color of the square and moves it
    pub fn update_color(&mut self, is_green: bool) {
        self.is_green = is_green;
        self.repaint();
        self.move_to_mouse(); // 🟢 Move the overlay when color updates
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
            // println!("🖌 Overlay WM_PAINT triggered!");
            OVERLAY.lock().unwrap().repaint();
            LRESULT(0)
        }
        WM_DESTROY => {
            println!("🛑 Overlay Window Destroyed!");
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        WM_NCHITTEST => {
            // ✅ Ensure mouse clicks go through the overlay
            LRESULT(HTTRANSPARENT as isize)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, _wparam, _lparam) },
    }
}
