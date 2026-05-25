use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopMetadata {
    pub virtual_desktop_id: Option<String>,
    pub anchor_hwnd: Option<isize>,
    pub anchor_process_id: Option<u32>,
    pub anchor_window_title: Option<String>,
}

pub fn capture_foreground_desktop_metadata() -> DesktopMetadata {
    let hwnd = unsafe { GetForegroundWindow() };
    let anchor = if hwnd.is_invalid() {
        None
    } else {
        Some(hwnd.0 as isize)
    };
    DesktopMetadata {
        virtual_desktop_id: None,
        anchor_hwnd: anchor,
        anchor_process_id: None,
        anchor_window_title: None,
    }
}

pub fn current_virtual_desktop_id() -> Option<String> {
    None
}

pub fn focus_anchor_window(_anchor_hwnd: Option<isize>) -> Result<(), String> {
    Err("virtual desktop backend unavailable".to_string())
}

pub fn switch_to_desktop(_desktop_id: Option<&str>) -> Result<(), String> {
    Err("virtual desktop backend unavailable".to_string())
}
