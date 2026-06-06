use windows::core::GUID;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
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

pub fn desktop_id_for_window(hwnd: HWND) -> Option<String> {
    if !valid_hwnd(hwnd) {
        return None;
    }
    with_virtual_desktop_manager(|manager| unsafe { manager.GetWindowDesktopId(hwnd).ok() })
        .flatten()
        .map(normalize_guid)
}

#[allow(dead_code)]
pub fn is_window_on_current_virtual_desktop(hwnd: HWND) -> bool {
    if !valid_hwnd(hwnd) {
        return false;
    }
    with_virtual_desktop_manager(|manager| unsafe {
        manager
            .IsWindowOnCurrentVirtualDesktop(hwnd)
            .ok()
            .map(|on_current| on_current.as_bool())
    })
    .flatten()
    .unwrap_or(false)
}

pub fn current_virtual_desktop_id() -> Option<String> {
    let hwnd = unsafe { GetForegroundWindow() };
    if !valid_hwnd(hwnd) {
        return None;
    }
    desktop_id_for_window(hwnd)
}

#[allow(dead_code)]
pub fn is_anchor_window_on_current_virtual_desktop(anchor_hwnd: Option<isize>) -> bool {
    let Some(raw) = anchor_hwnd else {
        return false;
    };
    let hwnd = HWND(raw as *mut core::ffi::c_void);
    if !valid_hwnd(hwnd) {
        return false;
    }
    is_window_on_current_virtual_desktop(hwnd)
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
    Err("virtual desktop switching is not supported by the Windows Shell API backend".to_string())
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

fn valid_hwnd(hwnd: HWND) -> bool {
    !hwnd.is_invalid() && unsafe { IsWindow(Some(hwnd)).as_bool() }
}

fn with_virtual_desktop_manager<T>(f: impl FnOnce(&IVirtualDesktopManager) -> T) -> Option<T> {
    let com = ComApartment::initialize()?;
    let manager = unsafe {
        CoCreateInstance::<_, IVirtualDesktopManager>(&VirtualDesktopManager, None, CLSCTX_ALL)
            .ok()?
    };
    let result = f(&manager);
    drop(manager);
    drop(com);
    Some(result)
}

struct ComApartment;

impl ComApartment {
    fn initialize() -> Option<Self> {
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if hr.is_ok() {
            Some(Self)
        } else {
            None
        }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

fn normalize_guid(guid: GUID) -> String {
    format!("{guid:?}").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_guid_uses_comparable_lowercase_hyphenated_id() {
        let guid = GUID::from_u128(0xAABBCCDD_EEFF_0011_2233_445566778899);
        assert_eq!(normalize_guid(guid), "aabbccdd-eeff-0011-2233-445566778899");
    }
}
