use std::ptr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use windows::core::w;
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, SetWindowPos, HWND_TOPMOST, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW,
};
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
                50, // Default X position
                50, // Default Y position
                25, // Width
                25, // Height
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
        let overlay = Self {
            hwnd: Arc::new(Mutex::new(hwnd_ptr)),
            is_green: false,
        };

        // ✅ **Add this line to start tracking the mouse!**

        // overlay.follow_cursor();

        overlay
    }
    /// Updates overlay position and color based on interaction state
    pub fn update_overlay_status(&mut self, is_left_click_held: bool) {
        // ✅ Lock HWND only once
        let hwnd = *self.hwnd.lock().unwrap();
        if let Some(h) = hwnd {
            let hwnd = HWND(h as *mut _);
            let mut point = POINT::default();

            // ✅ Move overlay window to mouse position
            if unsafe { GetCursorPos(&mut point) }.is_ok() {
                let x = point.x + 5;
                let y = point.y + 5;

                unsafe {
                    SetWindowPos(
                        hwnd,
                        Some(HWND_TOPMOST),
                        x,
                        y,
                        5,
                        5,
                        SWP_NOZORDER | SWP_NOSIZE | SWP_SHOWWINDOW,
                    );
                }
            }

            // ✅ Avoid deadlock by checking `is_green` separately
            if self.is_green != is_left_click_held {
                self.is_green = is_left_click_held;
                self.repaint();
            }
        }
    }
    /// Moves the overlay to follow the mouse cursor
    pub fn move_to_mouse(&self) {
        let hwnd_lock = self.hwnd.lock().unwrap();
        if let Some(h) = *hwnd_lock {
            let hwnd = HWND(h as *mut _);
            let mut point = POINT::default();

            if unsafe { GetCursorPos(&mut point) }.is_ok() {
                let x = point.x + 5; // Offset to the right
                let y = point.y + 5; // Offset below

                unsafe {
                    SetWindowPos(
                        hwnd,
                        Some(HWND_TOPMOST),
                        x,
                        y,
                        5, // Small overlay width
                        5, // Small overlay height
                        SWP_NOZORDER | SWP_NOSIZE | SWP_SHOWWINDOW,
                    );
                }
            }
        }
    }

    pub fn follow_cursor(&self) {
        let hwnd_arc = Arc::clone(&self.hwnd); // Clone Arc for safe access in the thread
        let is_moving_arc = Arc::new(Mutex::new(false)); // Prevent unnecessary movement updates
        let is_moving_clone = Arc::clone(&is_moving_arc);

        thread::spawn(move || {
            loop {
                let hwnd_lock = hwnd_arc.lock().unwrap();
                if let Some(h) = *hwnd_lock {
                    let hwnd = HWND(h as *mut _);
                    let mut point = POINT::default();

                    if unsafe { GetCursorPos(&mut point) }.is_ok() {
                        let x = point.x + 10;
                        let y = point.y + 10;

                        // Only update if the position is different to avoid unnecessary SetWindowPos calls
                        let mut is_moving = is_moving_clone.lock().unwrap();
                        if *is_moving == false {
                            *is_moving = true;
                            unsafe {
                                SetWindowPos(
                                    hwnd,
                                    Some(HWND_TOPMOST),
                                    x,
                                    y,
                                    20, // Small overlay width
                                    20, // Small overlay height
                                    SWP_NOZORDER | SWP_NOSIZE | SWP_SHOWWINDOW,
                                );
                            }
                            *is_moving = false;
                        }
                    }
                }
                drop(hwnd_lock);
                thread::sleep(Duration::from_millis(200)); // Lower update rate to reduce CPU usage
            }
        });
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
