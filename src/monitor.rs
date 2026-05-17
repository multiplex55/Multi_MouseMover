use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorEdge {
    Top,
    Bottom,
    Left,
    Right,
}

impl MonitorRect {
    pub fn center(self) -> (i32, i32) {
        (
            self.left + (self.right - self.left) / 2,
            self.top + (self.bottom - self.top) / 2,
        )
    }

    pub fn edge_midpoint(self, edge: MonitorEdge, offset: i32) -> (i32, i32) {
        let offset = offset.max(0);
        let (center_x, center_y) = self.center();

        match edge {
            MonitorEdge::Top => (center_x, self.top + offset),
            MonitorEdge::Bottom => (center_x, self.bottom - 1 - offset),
            MonitorEdge::Left => (self.left + offset, center_y),
            MonitorEdge::Right => (self.right - 1 - offset, center_y),
        }
    }
}

impl From<RECT> for MonitorRect {
    fn from(rect: RECT) -> Self {
        Self {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        }
    }
}

pub fn current_monitor_rect_for_cursor(use_work_area: bool) -> Option<MonitorRect> {
    let mut point = POINT::default();

    unsafe {
        GetCursorPos(&mut point).ok()?;
        monitor_rect_for_point(point, use_work_area)
    }
}

unsafe fn monitor_rect_for_point(point: POINT, use_work_area: bool) -> Option<MonitorRect> {
    let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
    if monitor.is_invalid() {
        return None;
    }

    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..MONITORINFO::default()
    };

    GetMonitorInfoW(monitor, &mut info).as_bool().then(|| {
        if use_work_area {
            info.rcWork.into()
        } else {
            info.rcMonitor.into()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> MonitorRect {
        MonitorRect {
            left: 100,
            top: 200,
            right: 500,
            bottom: 800,
        }
    }

    #[test]
    fn center_uses_monitor_midpoint() {
        assert_eq!(rect().center(), (300, 500));
    }

    #[test]
    fn edge_targets_use_midpoints_with_offsets() {
        let rect = rect();

        assert_eq!(rect.edge_midpoint(MonitorEdge::Top, 8), (300, 208));
        assert_eq!(rect.edge_midpoint(MonitorEdge::Bottom, 8), (300, 791));
        assert_eq!(rect.edge_midpoint(MonitorEdge::Left, 8), (108, 500));
        assert_eq!(rect.edge_midpoint(MonitorEdge::Right, 8), (491, 500));
    }
}
