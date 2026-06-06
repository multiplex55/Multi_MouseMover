use windows::Win32::Foundation::HWND;
#[cfg(windows)]
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
#[cfg(windows)]
use windows::Win32::UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, IsWindow, IsWindowVisible,
    SetForegroundWindow, ShowWindow, SW_RESTORE,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopMetadata {
    pub virtual_desktop_id: Option<String>,
    pub anchor_hwnd: Option<isize>,
    pub anchor_process_id: Option<u32>,
    pub anchor_window_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusAnchorFailureReason {
    AnchorMissing,
    HwndNotFound,
    RestoreFailed,
    ForegroundDenied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusAnchorResult {
    pub success: bool,
    pub reason: Option<FocusAnchorFailureReason>,
}

pub fn capture_foreground_desktop_metadata() -> DesktopMetadata {
    let hwnd = unsafe { GetForegroundWindow() };
    let anchor = if hwnd.is_invalid() {
        None
    } else {
        Some(hwnd.0 as isize)
    };
    let mut pid: u32 = 0;
    let anchor_process_id = if hwnd.is_invalid() {
        None
    } else {
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        (pid != 0).then_some(pid)
    };
    let anchor_window_title = read_window_title(hwnd);
    DesktopMetadata {
        virtual_desktop_id: desktop_id_for_window(hwnd),
        anchor_hwnd: anchor,
        anchor_process_id,
        anchor_window_title,
    }
}

pub fn current_virtual_desktop_id() -> Option<String> {
    let hwnd = unsafe { GetForegroundWindow() };
    desktop_id_for_window(hwnd)
}

#[cfg(windows)]
fn virtual_desktop_manager() -> Option<IVirtualDesktopManager> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL).ok()
    }
}

#[cfg(windows)]
pub fn desktop_id_for_window(hwnd: HWND) -> Option<String> {
    if hwnd.is_invalid() || !unsafe { IsWindow(Some(hwnd)).as_bool() } {
        return None;
    }
    let manager = virtual_desktop_manager()?;
    let guid = unsafe { manager.GetWindowDesktopId(hwnd).ok()? };
    Some(format!("{guid:?}"))
}

#[cfg(not(windows))]
pub fn desktop_id_for_window(_hwnd: HWND) -> Option<String> {
    None
}

#[cfg(windows)]
pub fn is_window_on_current_virtual_desktop(hwnd: HWND) -> bool {
    if hwnd.is_invalid() || !unsafe { IsWindow(Some(hwnd)).as_bool() } {
        return false;
    }
    let Some(manager) = virtual_desktop_manager() else {
        return false;
    };
    unsafe {
        manager
            .IsWindowOnCurrentVirtualDesktop(hwnd)
            .map(|on_current| on_current.as_bool())
            .unwrap_or(false)
    }
}

#[cfg(not(windows))]
pub fn is_window_on_current_virtual_desktop(_hwnd: HWND) -> bool {
    false
}

pub fn focus_anchor_window(anchor_hwnd: Option<isize>) -> FocusAnchorResult {
    let Some(raw) = anchor_hwnd else {
        return FocusAnchorResult {
            success: false,
            reason: Some(FocusAnchorFailureReason::AnchorMissing),
        };
    };
    let hwnd = HWND(raw as *mut core::ffi::c_void);
    let exists = unsafe { IsWindow(Some(hwnd)).as_bool() };
    if !exists {
        return FocusAnchorResult {
            success: false,
            reason: Some(FocusAnchorFailureReason::HwndNotFound),
        };
    }
    let restored_or_visible =
        unsafe { ShowWindow(hwnd, SW_RESTORE).as_bool() || IsWindowVisible(hwnd).as_bool() };
    if !restored_or_visible {
        return FocusAnchorResult {
            success: false,
            reason: Some(FocusAnchorFailureReason::RestoreFailed),
        };
    }
    let focused = unsafe { SetForegroundWindow(hwnd).as_bool() };
    if !focused {
        return FocusAnchorResult {
            success: false,
            reason: Some(FocusAnchorFailureReason::ForegroundDenied),
        };
    }
    FocusAnchorResult {
        success: true,
        reason: None,
    }
}

pub fn switch_to_desktop(_desktop_id: Option<&str>) -> Result<(), String> {
    Err(
        "virtual desktop switching is not supported by the public IVirtualDesktopManager API"
            .to_string(),
    )
}

fn read_window_title(hwnd: HWND) -> Option<String> {
    if hwnd.is_invalid() {
        return None;
    }
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return None;
    }
    let mut buffer = vec![0u16; len as usize + 1];
    let copied = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    if copied <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..copied as usize]))
}
