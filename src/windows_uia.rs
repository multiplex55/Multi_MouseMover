use crate::UiHintsConfig;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumThreadWindows, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId,
    IsWindowVisible, PtInRect, ScreenToClient,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawUiElement {
    pub bounds: (i32, i32, i32, i32),
    pub clickable_point: Option<(i32, i32)>,
    pub name: String,
    pub control_type: String,
}

#[derive(Debug, Clone)]
pub enum UiHintQueryError {
    NoForegroundWindow,
    EnumerationFailed,
}

pub fn find_ui_hint_targets(config: &UiHintsConfig) -> Result<Vec<RawUiElement>, UiHintQueryError> {
    let fg = unsafe { GetForegroundWindow() };
    if fg.0 == 0 {
        return Err(UiHintQueryError::NoForegroundWindow);
    }

    let mut windows = vec![fg];
    if config.include_thread_windows {
        let thread_id = unsafe { GetWindowThreadProcessId(fg, None) };
        let mut thread_windows = Vec::<HWND>::new();
        unsafe {
            EnumThreadWindows(thread_id, Some(enum_collect_windows), LPARAM((&mut thread_windows as *mut Vec<HWND>) as isize))
                .map_err(|_| UiHintQueryError::EnumerationFailed)?;
        }
        for w in thread_windows {
            if !windows.iter().any(|existing| existing.0 == w.0) {
                windows.push(w);
            }
        }
    }

    let mut raw = Vec::new();
    for hwnd in windows {
        collect_window_targets(hwnd, &mut raw);
    }

    raw.retain(|e| e.bounds.2 > 0 && e.bounds.3 > 0);
    raw.sort_by_key(|e| (e.bounds.1, e.bounds.0, e.bounds.2 * e.bounds.3));
    raw.dedup_by_key(|e| e.bounds);

    Ok(raw)
}

fn collect_window_targets(root: HWND, out: &mut Vec<RawUiElement>) {
    unsafe {
        let _ = EnumChildWindows(root, Some(enum_collect_elements), LPARAM((out as *mut Vec<RawUiElement>) as isize));
    }
}

unsafe extern "system" fn enum_collect_windows(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let windows = &mut *(lparam.0 as *mut Vec<HWND>);
    if IsWindowVisible(hwnd).as_bool() {
        windows.push(hwnd);
    }
    BOOL(1)
}

unsafe extern "system" fn enum_collect_elements(hwnd: HWND, lparam: LPARAM) -> BOOL {
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }

    let mut rect = RECT::default();
    if !GetWindowRect(hwnd, &mut rect).as_bool() {
        return BOOL(1);
    }

    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width <= 0 || height <= 0 {
        return BOOL(1);
    }

    let mut center = POINT {
        x: rect.left + (width / 2),
        y: rect.top + (height / 2),
    };
    let click = if PtInRect(&rect, center).as_bool() {
        Some((center.x.clamp(rect.left, rect.right), center.y.clamp(rect.top, rect.bottom)))
    } else {
        None
    };

    let out = &mut *(lparam.0 as *mut Vec<RawUiElement>);
    out.push(RawUiElement {
        bounds: (rect.left, rect.top, width, height),
        clickable_point: click,
        name: String::new(),
        control_type: "interactable".to_string(),
    });

    let _ = ScreenToClient(hwnd, &mut center);
    BOOL(1)
}
