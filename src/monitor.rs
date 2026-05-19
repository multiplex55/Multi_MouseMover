use windows::Win32::Foundation::{BOOL, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, HDC, HMONITOR,
    MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetForegroundWindow, GetWindowRect};

use crate::{jump_session::JumpRegion, JumpStartRegion};

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

pub fn all_monitor_rects(use_work_area: bool) -> Vec<MonitorRect> {
    let mut monitors = Vec::new();
    let context = MonitorEnumContext {
        monitors: &mut monitors,
        use_work_area,
    };
    let mut context = context;

    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(enum_monitor_rects),
            LPARAM(&mut context as *mut MonitorEnumContext<'_> as isize),
        );
    }

    sort_monitors_by_position(monitors)
}

pub fn sort_monitors_by_position(mut monitors: Vec<MonitorRect>) -> Vec<MonitorRect> {
    monitors.sort_by_key(|monitor| (monitor.left, monitor.top));
    monitors
}

pub fn point_in_monitor(point: (i32, i32), monitor: MonitorRect) -> bool {
    let (x, y) = point;
    x >= monitor.left && x < monitor.right && y >= monitor.top && y < monitor.bottom
}

pub fn monitor_containing_point(
    monitors: &[MonitorRect],
    point: (i32, i32),
) -> Option<MonitorRect> {
    monitors
        .iter()
        .copied()
        .find(|monitor| point_in_monitor(point, *monitor))
}

pub fn next_monitor_with_wrap(
    monitors: &[MonitorRect],
    current_monitor: MonitorRect,
) -> Option<MonitorRect> {
    let monitors = sort_monitors_by_position(monitors.to_vec());
    let current_index = monitors
        .iter()
        .position(|monitor| *monitor == current_monitor)?;
    let next_index = (current_index + 1) % monitors.len();
    monitors.get(next_index).copied()
}

pub fn current_monitor_rect_for_cursor(use_work_area: bool) -> Option<MonitorRect> {
    let mut point = POINT::default();

    unsafe {
        GetCursorPos(&mut point).ok()?;
        monitor_rect_for_point(point, use_work_area)
    }
}

pub fn resolve_jump_start_region(
    mode: JumpStartRegion,
    virtual_screen: JumpRegion,
) -> Option<JumpRegion> {
    let cursor_monitor = current_monitor_rect_for_cursor(false).map(Into::into);
    let active_window_bounds = active_window_bounds();
    let active_window_monitor = active_window_monitor_rect(false).map(Into::into);

    resolve_jump_start_region_from_parts(
        mode,
        virtual_screen,
        cursor_monitor,
        active_window_monitor,
        active_window_bounds,
    )
}

pub fn resolve_jump_start_region_from_parts(
    mode: JumpStartRegion,
    virtual_screen: JumpRegion,
    cursor_monitor: Option<JumpRegion>,
    active_window_monitor: Option<JumpRegion>,
    active_window_bounds: Option<JumpRegion>,
) -> Option<JumpRegion> {
    match mode {
        JumpStartRegion::VirtualScreen => Some(virtual_screen),
        JumpStartRegion::CurrentMonitor => cursor_monitor.or(Some(virtual_screen)),
        JumpStartRegion::ActiveWindowMonitor => active_window_monitor
            .or(cursor_monitor)
            .or(Some(virtual_screen)),
        JumpStartRegion::ActiveWindowBounds => active_window_bounds
            .filter(|region| region.is_valid())
            .or(active_window_monitor)
            .or(cursor_monitor)
            .or(Some(virtual_screen)),
    }
}

struct MonitorEnumContext<'a> {
    monitors: &'a mut Vec<MonitorRect>,
    use_work_area: bool,
}

unsafe extern "system" fn enum_monitor_rects(
    monitor: HMONITOR,
    _dc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let context = &mut *(data.0 as *mut MonitorEnumContext<'_>);
    if let Some(rect) = monitor_rect_from_handle(monitor, context.use_work_area) {
        context.monitors.push(rect);
    }
    true.into()
}

fn active_window_bounds() -> Option<JumpRegion> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect).ok()?;
        Some(MonitorRect::from(rect).into())
    }
}

fn active_window_monitor_rect(use_work_area: bool) -> Option<MonitorRect> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        monitor_rect_from_handle(monitor, use_work_area)
    }
}

unsafe fn monitor_rect_for_point(point: POINT, use_work_area: bool) -> Option<MonitorRect> {
    let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
    monitor_rect_from_handle(monitor, use_work_area)
}

unsafe fn monitor_rect_from_handle(
    monitor: windows::Win32::Graphics::Gdi::HMONITOR,
    use_work_area: bool,
) -> Option<MonitorRect> {
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

impl From<MonitorRect> for JumpRegion {
    fn from(rect: MonitorRect) -> Self {
        Self {
            left: rect.left,
            top: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
        }
    }
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

    #[test]
    fn sort_monitors_orders_by_left_then_top() {
        let monitors = vec![
            MonitorRect {
                left: 1920,
                top: 0,
                right: 3840,
                bottom: 1080,
            },
            MonitorRect {
                left: -1280,
                top: 200,
                right: 0,
                bottom: 920,
            },
            MonitorRect {
                left: -1280,
                top: -520,
                right: 0,
                bottom: 200,
            },
            MonitorRect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
        ];

        let sorted = sort_monitors_by_position(monitors);

        assert_eq!(
            sorted
                .iter()
                .map(|monitor| (monitor.left, monitor.top))
                .collect::<Vec<_>>(),
            vec![(-1280, -520), (-1280, 200), (0, 0), (1920, 0)]
        );
    }

    #[test]
    fn point_in_monitor_includes_top_left_and_excludes_bottom_right_edges() {
        let monitor = MonitorRect {
            left: -100,
            top: -50,
            right: 100,
            bottom: 150,
        };

        assert!(point_in_monitor((-100, -50), monitor));
        assert!(point_in_monitor((99, 149), monitor));
        assert!(!point_in_monitor((100, 149), monitor));
        assert!(!point_in_monitor((99, 150), monitor));
        assert!(!point_in_monitor((-101, -50), monitor));
    }

    #[test]
    fn next_monitor_wraps_after_last_sorted_monitor() {
        let left = MonitorRect {
            left: -1280,
            top: 0,
            right: 0,
            bottom: 720,
        };
        let primary = MonitorRect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let right = MonitorRect {
            left: 1920,
            top: -200,
            right: 3520,
            bottom: 700,
        };
        let monitors = vec![right, primary, left];

        assert_eq!(next_monitor_with_wrap(&monitors, left), Some(primary));
        assert_eq!(next_monitor_with_wrap(&monitors, primary), Some(right));
        assert_eq!(next_monitor_with_wrap(&monitors, right), Some(left));
    }

    #[test]
    fn monitor_containment_handles_negative_coordinate_layouts() {
        let monitors = vec![
            MonitorRect {
                left: -1600,
                top: -900,
                right: 0,
                bottom: 0,
            },
            MonitorRect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
        ];

        assert_eq!(
            monitor_containing_point(&monitors, (-800, -450)),
            Some(monitors[0])
        );
        assert_eq!(
            monitor_containing_point(&monitors, (1200, 900)),
            Some(monitors[1])
        );
        assert_eq!(monitor_containing_point(&monitors, (-1, 0)), None);
    }

    fn region(left: i32, top: i32, width: i32, height: i32) -> JumpRegion {
        JumpRegion {
            left,
            top,
            width,
            height,
        }
    }

    #[test]
    fn start_region_resolver_uses_requested_region_with_fallbacks() {
        let virtual_screen = region(0, 0, 300, 200);
        let cursor_monitor = region(10, 10, 100, 100);
        let window_monitor = region(150, 0, 150, 200);
        let window_bounds = region(170, 20, 50, 60);

        assert_eq!(
            resolve_jump_start_region_from_parts(
                JumpStartRegion::VirtualScreen,
                virtual_screen,
                Some(cursor_monitor),
                Some(window_monitor),
                Some(window_bounds),
            ),
            Some(virtual_screen)
        );
        assert_eq!(
            resolve_jump_start_region_from_parts(
                JumpStartRegion::CurrentMonitor,
                virtual_screen,
                Some(cursor_monitor),
                Some(window_monitor),
                Some(window_bounds),
            ),
            Some(cursor_monitor)
        );
        assert_eq!(
            resolve_jump_start_region_from_parts(
                JumpStartRegion::ActiveWindowMonitor,
                virtual_screen,
                Some(cursor_monitor),
                Some(window_monitor),
                Some(window_bounds),
            ),
            Some(window_monitor)
        );
        assert_eq!(
            resolve_jump_start_region_from_parts(
                JumpStartRegion::ActiveWindowBounds,
                virtual_screen,
                Some(cursor_monitor),
                Some(window_monitor),
                Some(window_bounds),
            ),
            Some(window_bounds)
        );
    }

    #[test]
    fn start_region_resolver_falls_back_when_window_data_is_missing_or_invalid() {
        let virtual_screen = region(0, 0, 300, 200);
        let cursor_monitor = region(10, 10, 100, 100);
        let window_monitor = region(150, 0, 150, 200);

        assert_eq!(
            resolve_jump_start_region_from_parts(
                JumpStartRegion::ActiveWindowBounds,
                virtual_screen,
                Some(cursor_monitor),
                Some(window_monitor),
                Some(region(0, 0, 0, 10)),
            ),
            Some(window_monitor)
        );
        assert_eq!(
            resolve_jump_start_region_from_parts(
                JumpStartRegion::ActiveWindowMonitor,
                virtual_screen,
                Some(cursor_monitor),
                None,
                None,
            ),
            Some(cursor_monitor)
        );
    }
}
