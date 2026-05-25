use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::UI::WindowsAndMessaging::{
    ClientToScreen, GetClientRect, GetForegroundWindow, GetWindowRect,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl WindowRect {
    pub fn width(self) -> i32 {
        self.right - self.left
    }
    pub fn height(self) -> i32 {
        self.bottom - self.top
    }
    pub fn center(self) -> (i32, i32) {
        (self.left + self.width() / 2, self.top + self.height() / 2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowSnapPoints {
    pub top_edge: (i32, i32),
    pub bottom_edge: (i32, i32),
    pub left_edge: (i32, i32),
    pub right_edge: (i32, i32),
    pub center: (i32, i32),
    pub titlebar: (i32, i32),
}

pub fn foreground_window_snap_points(
    use_extended_frame_bounds: bool,
    edge_offset_px: i32,
    titlebar_y_offset_px: i32,
    clamp_to_window: bool,
) -> Option<WindowSnapPoints> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0 == 0 {
        return None;
    }
    let rect = foreground_rect(hwnd, use_extended_frame_bounds)?;
    Some(derive_snap_points(
        rect,
        edge_offset_px,
        titlebar_y_offset_px,
        clamp_to_window,
    ))
}

fn foreground_rect(hwnd: HWND, use_extended_frame_bounds: bool) -> Option<WindowRect> {
    if use_extended_frame_bounds {
        if let Some(r) = extended_frame_bounds(hwnd) {
            return Some(r);
        }
    }
    window_rect(hwnd)
}

fn window_rect(hwnd: HWND) -> Option<WindowRect> {
    let mut rect = RECT::default();
    let ok = unsafe { GetWindowRect(hwnd, &mut rect) }.as_bool();
    ok.then_some(from_win_rect(rect))
}

fn extended_frame_bounds(hwnd: HWND) -> Option<WindowRect> {
    let mut rect = RECT::default();
    let hr = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut _ as *mut _,
            std::mem::size_of::<RECT>() as u32,
        )
    };
    hr.is_ok().then_some(from_win_rect(rect))
}

fn from_win_rect(rect: RECT) -> WindowRect {
    WindowRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

#[allow(dead_code)]
fn client_rect_in_screen(hwnd: HWND) -> Option<WindowRect> {
    let mut client = RECT::default();
    if !unsafe { GetClientRect(hwnd, &mut client) }.as_bool() {
        return None;
    }
    let mut origin = POINT {
        x: client.left,
        y: client.top,
    };
    if !unsafe { ClientToScreen(hwnd, &mut origin) }.as_bool() {
        return None;
    }
    Some(WindowRect {
        left: origin.x,
        top: origin.y,
        right: origin.x + (client.right - client.left),
        bottom: origin.y + (client.bottom - client.top),
    })
}

pub fn derive_snap_points(
    rect: WindowRect,
    edge_offset_px: i32,
    titlebar_y_offset_px: i32,
    clamp_to_window: bool,
) -> WindowSnapPoints {
    let offset = edge_offset_px.max(0);
    let title_off = titlebar_y_offset_px.max(0);
    let (cx, cy) = rect.center();
    let mut points = WindowSnapPoints {
        top_edge: (cx, rect.top + offset),
        bottom_edge: (cx, rect.bottom - 1 - offset),
        left_edge: (rect.left + offset, cy),
        right_edge: (rect.right - 1 - offset, cy),
        center: (cx, cy),
        titlebar: (cx, rect.top + title_off),
    };
    if clamp_to_window {
        let clamp = |(x, y): (i32, i32)| {
            (
                x.clamp(rect.left, rect.right - 1),
                y.clamp(rect.top, rect.bottom - 1),
            )
        };
        points.top_edge = clamp(points.top_edge);
        points.bottom_edge = clamp(points.bottom_edge);
        points.left_edge = clamp(points.left_edge);
        points.right_edge = clamp(points.right_edge);
        points.titlebar = clamp(points.titlebar);
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn derive_edges_center_and_titlebar() {
        let rect = WindowRect {
            left: 100,
            top: 200,
            right: 500,
            bottom: 700,
        };
        let p = derive_snap_points(rect, 10, 30, true);
        assert_eq!(p.top_edge, (300, 210));
        assert_eq!(p.bottom_edge, (300, 689));
        assert_eq!(p.left_edge, (110, 450));
        assert_eq!(p.right_edge, (489, 450));
        assert_eq!(p.center, (300, 450));
        assert_eq!(p.titlebar, (300, 230));
    }

    #[test]
    fn clamp_prevents_out_of_bounds_offsets() {
        let rect = WindowRect {
            left: 0,
            top: 0,
            right: 10,
            bottom: 10,
        };
        let p = derive_snap_points(rect, 999, 999, true);
        assert_eq!(p.top_edge, (5, 9));
        assert_eq!(p.bottom_edge, (5, 0));
        assert_eq!(p.left_edge, (9, 5));
        assert_eq!(p.right_edge, (0, 5));
        assert_eq!(p.titlebar, (5, 9));
    }
}
