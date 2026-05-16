use std::ptr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
#[cfg(debug_assertions)]
use std::time::Instant;
use windows::core::{w, Error};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
};
use windows::Win32::{
    Foundation::*, Graphics::Gdi::*, System::LibraryLoader::*, UI::WindowsAndMessaging::*,
};

lazy_static::lazy_static! {
    /// Global overlay instance wrapped in `Arc<Mutex<Option<OverlayWindow>>>`.
    ///
    /// Initialization may fail, in which case the value will be `None` and
    /// overlay features will be disabled.
    pub static ref OVERLAY: Arc<Mutex<Option<OverlayWindow>>> = Arc::new(Mutex::new(match OverlayWindow::new() {
        Ok(ov) => Some(ov),
        Err(e) => {
            eprintln!("Failed to initialize overlay: {e}");
            None
        }
    }));

    #[cfg(debug_assertions)]
    static ref PAINT_SMOKE_LOG: Mutex<PaintSmokeLog> = Mutex::new(PaintSmokeLog::new());
}

#[derive(Clone)]
pub struct OverlayWindow {
    hwnd: Arc<Mutex<Option<isize>>>, // ✅ Store HWND as `isize`
    is_green: bool,
    last_cursor_position: Option<OverlayPosition>,
    last_visual_state: Option<OverlayVisualState>,
    #[cfg(debug_assertions)]
    debug_counters: Arc<Mutex<OverlayDebugCounters>>,
}

fn overlay_ex_style() -> WINDOW_EX_STYLE {
    WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OverlayPosition {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OverlayVisualState {
    is_green: bool,
}

#[cfg(debug_assertions)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct OverlayDebugCounters {
    set_window_pos_calls: u64,
    repaint_requests: u64,
}

fn should_move(previous: Option<&OverlayPosition>, current: &OverlayPosition) -> bool {
    previous != Some(current)
}

fn should_repaint(previous: Option<&OverlayVisualState>, current: &OverlayVisualState) -> bool {
    previous != Some(current)
}

#[cfg(debug_assertions)]
struct PaintSmokeLog {
    window_start: Instant,
    count: u32,
}

#[cfg(debug_assertions)]
impl PaintSmokeLog {
    fn new() -> Self {
        Self {
            window_start: Instant::now(),
            count: 0,
        }
    }

    fn record_paint(&mut self) {
        self.count += 1;
        let elapsed = self.window_start.elapsed();

        if elapsed >= Duration::from_secs(1) {
            println!(
                "[overlay] WM_PAINT smoke: {} paints/sec",
                self.count as f64 / elapsed.as_secs_f64()
            );
            self.window_start = Instant::now();
            self.count = 0;
        }
    }
}

impl OverlayWindow {
    /// Creates the overlay window
    pub fn new() -> Result<Self, Error> {
        println!("🚀 Overlay: Starting Initialization");

        let h_instance = unsafe { GetModuleHandleW(None)? };
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

        // Create window hidden; the caller shows it after global state is ready.
        println!("🔹 Overlay: Creating Overlay Window...");
        let hwnd = unsafe {
            CreateWindowExW(
                overlay_ex_style(),
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
            )?
        };
        println!("✅ Overlay: Window Created Successfully!");

        // Store HWND as `isize`
        let hwnd_ptr = Some(hwnd.0 as isize);
        println!("🔹 Overlay: HWND Stored as isize");

        // Apply layered attributes before the first show. The window remains
        // hidden until `show` is called after application globals are ready.
        if let Some(h) = hwnd_ptr {
            unsafe {
                println!("🔹 Overlay: Setting Layered Window Attributes...");
                let _ = SetLayeredWindowAttributes(HWND(h as *mut _), COLORREF(0), 255, LWA_ALPHA);
                let _ = ShowWindow(HWND(h as *mut _), SW_HIDE);
            }
        }

        println!("✅ Overlay: Initialization Completed!");
        let overlay = Self {
            hwnd: Arc::new(Mutex::new(hwnd_ptr)),
            is_green: false,
            last_cursor_position: None,
            last_visual_state: None,
            #[cfg(debug_assertions)]
            debug_counters: Arc::new(Mutex::new(OverlayDebugCounters::default())),
        };

        // ✅ **Add this line to start tracking the mouse!**

        // overlay.follow_cursor();

        Ok(overlay)
    }

    /// Shows the overlay without activating it and schedules its first paint.
    pub fn show(&self) {
        let hwnd_lock = self.hwnd.lock().unwrap();
        if let Some(h) = *hwnd_lock {
            let hwnd = HWND(h as *mut _);
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
        }
    }

    pub fn update_overlay_status(&mut self, is_left_click_held: bool) {
        let hwnd = *self.hwnd.lock().unwrap();
        if let Some(h) = hwnd {
            let hwnd = HWND(h as *mut _);
            let mut point = POINT::default();

            if unsafe { GetCursorPos(&mut point) }.is_ok() {
                let current_position = OverlayPosition {
                    x: point.x + 5,
                    y: point.y + 5,
                };
                let current_visual = OverlayVisualState {
                    is_green: is_left_click_held,
                };

                if should_move(self.last_cursor_position.as_ref(), &current_position) {
                    unsafe {
                        let _ = SetWindowPos(
                            hwnd,
                            Some(HWND_TOPMOST),
                            current_position.x,
                            current_position.y,
                            5, // Small overlay width
                            5, // Small overlay height
                            SWP_NOZORDER | SWP_NOSIZE | SWP_NOACTIVATE,
                        );
                    }
                    self.record_set_window_pos_call();
                }

                if should_repaint(self.last_visual_state.as_ref(), &current_visual) {
                    self.is_green = current_visual.is_green;
                    self.request_repaint();
                }

                self.last_cursor_position = Some(current_position);
                self.last_visual_state = Some(current_visual);
            }
        }
    }

    /// Draws the square into the caller-provided paint device context.
    pub fn draw(&self, hdc: HDC) {
        let color = if self.is_green {
            RGB(0, 255, 0) // Green when left-click is pressed
        } else {
            RGB(255, 0, 0) // Red otherwise
        };

        unsafe {
            let hbrush = CreateSolidBrush(color);
            if !hbrush.0.is_null() {
                let rect = RECT {
                    left: 0,
                    top: 0,
                    right: 100,
                    bottom: 100,
                };
                let _ = FillRect(hdc, &rect, hbrush);
                let _ = DeleteObject(hbrush.into());
            }
        }
    }

    /// Requests a repaint by invalidating the client region.
    pub fn request_repaint(&self) {
        let hwnd_lock = self.hwnd.lock().unwrap();
        if let Some(h) = *hwnd_lock {
            let hwnd = HWND(h as *mut _); // ✅ Convert `isize` back to `HWND`
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            self.record_repaint_request();
        }
    }

    /// Moves the overlay to follow the mouse cursor
    pub fn move_to_mouse(&mut self) {
        let hwnd_lock = self.hwnd.lock().unwrap();
        if let Some(h) = *hwnd_lock {
            let hwnd = HWND(h as *mut _);
            let mut point = POINT::default();

            if unsafe { GetCursorPos(&mut point) }.is_ok() {
                let current_position = OverlayPosition {
                    x: point.x + 5,
                    y: point.y + 5,
                };

                if should_move(self.last_cursor_position.as_ref(), &current_position) {
                    unsafe {
                        let _ = SetWindowPos(
                            hwnd,
                            Some(HWND_TOPMOST),
                            current_position.x,
                            current_position.y,
                            5, // Small overlay width
                            5, // Small overlay height
                            SWP_NOZORDER | SWP_NOSIZE | SWP_NOACTIVATE,
                        );
                    }
                    self.record_set_window_pos_call();
                }

                self.last_cursor_position = Some(current_position);
            }
        }
    }

    #[allow(dead_code)]
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
                                let _ = SetWindowPos(
                                    hwnd,
                                    Some(HWND_TOPMOST),
                                    x,
                                    y,
                                    20, // Small overlay width
                                    20, // Small overlay height
                                    SWP_NOZORDER | SWP_NOSIZE | SWP_NOACTIVATE,
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
        let current_visual = OverlayVisualState { is_green };
        if should_repaint(self.last_visual_state.as_ref(), &current_visual) {
            self.is_green = is_green;
            self.request_repaint();
        }
        self.last_visual_state = Some(current_visual);
        self.move_to_mouse(); // 🟢 Move the overlay when color updates
    }

    #[cfg(debug_assertions)]
    fn record_set_window_pos_call(&self) {
        self.debug_counters
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .set_window_pos_calls += 1;
    }

    #[cfg(not(debug_assertions))]
    fn record_set_window_pos_call(&self) {}

    #[cfg(debug_assertions)]
    fn record_repaint_request(&self) {
        self.debug_counters
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .repaint_requests += 1;
    }

    #[cfg(not(debug_assertions))]
    fn record_repaint_request(&self) {}

    #[cfg(debug_assertions)]
    #[allow(dead_code)]
    fn debug_counters(&self) -> OverlayDebugCounters {
        *self
            .debug_counters
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }
}

/// Helper function to create a `COLORREF`
#[allow(non_snake_case)]
pub fn RGB(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(((b as u32) << 16) | ((g as u32) << 8) | (r as u32))
}

/// Window procedure for overlay
extern "system" fn window_proc(hwnd: HWND, msg: u32, _wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            // BeginPaint must be paired with EndPaint so Windows validates the
            // dirty region; otherwise the same region remains invalid and can
            // dispatch WM_PAINT repeatedly.
            let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
            #[cfg(debug_assertions)]
            {
                PAINT_SMOKE_LOG
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .record_paint();
            }
            if let Some(ref mut ov) = *OVERLAY.lock().unwrap_or_else(|e| e.into_inner()) {
                ov.draw(hdc);
            }
            // EndPaint completes the validation started by BeginPaint; keep all
            // rendering for this message inside that pair.
            unsafe {
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            println!("🛑 Overlay Window Destroyed!");
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        WM_NCHITTEST => {
            // Defense in depth for click-through behavior: the extended
            // WS_EX_TRANSPARENT style should keep this overlay out of mouse
            // targeting, and HTTRANSPARENT preserves that behavior if Windows
            // still asks the non-client hit-test path about this window.
            LRESULT(HTTRANSPARENT as isize)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, _wparam, _lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::{
        overlay_ex_style, should_move, should_repaint, OverlayPosition, OverlayVisualState,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    };

    #[test]
    fn style_contains_click_through_and_non_activate_bits() {
        let style = overlay_ex_style();

        assert!(style.contains(WS_EX_LAYERED));
        assert!(style.contains(WS_EX_TOPMOST));
        assert!(style.contains(WS_EX_TOOLWINDOW));
        assert!(style.contains(WS_EX_TRANSPARENT));
        assert!(style.contains(WS_EX_NOACTIVATE));
    }

    #[test]
    fn should_move_when_there_is_no_previous_position() {
        let current = OverlayPosition { x: 10, y: 20 };

        assert!(should_move(None, &current));
    }

    #[test]
    fn should_move_only_when_position_changes() {
        let previous = OverlayPosition { x: 10, y: 20 };
        let same = OverlayPosition { x: 10, y: 20 };
        let changed = OverlayPosition { x: 11, y: 20 };

        assert!(!should_move(Some(&previous), &same));
        assert!(should_move(Some(&previous), &changed));
    }

    #[test]
    fn should_repaint_when_there_is_no_previous_visual_state() {
        let current = OverlayVisualState { is_green: false };

        assert!(should_repaint(None, &current));
    }

    #[test]
    fn should_repaint_only_when_visual_state_changes() {
        let previous = OverlayVisualState { is_green: false };
        let same = OverlayVisualState { is_green: false };
        let changed = OverlayVisualState { is_green: true };

        assert!(!should_repaint(Some(&previous), &same));
        assert!(should_repaint(Some(&previous), &changed));
    }
}
