use crate::UiHintsConfig;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationCondition, IUIAutomationElement,
    IUIAutomationElementArray, TreeScope_Children, TreeScope_Descendants, UIA_ButtonControlTypeId,
    UIA_CheckBoxControlTypeId, UIA_ComboBoxControlTypeId, UIA_ControlTypePropertyId,
    UIA_EditControlTypeId, UIA_HyperlinkControlTypeId, UIA_IsContentElementPropertyId,
    UIA_IsControlElementPropertyId, UIA_IsEnabledPropertyId,
    UIA_IsInvokePatternAvailablePropertyId, UIA_IsKeyboardFocusablePropertyId,
    UIA_IsOffscreenPropertyId, UIA_IsSelectionItemPatternAvailablePropertyId,
    UIA_IsValuePatternAvailablePropertyId, UIA_ListItemControlTypeId, UIA_MenuItemControlTypeId,
    UIA_RadioButtonControlTypeId, UIA_TabItemControlTypeId, UIA_TreeItemControlTypeId,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumThreadWindows, GetWindow, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
    GW_OWNER,
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
    UiAutomationInitFailed,
    UiAutomationQueryFailed,
}

pub fn find_ui_hint_targets_for_window(
    foreground_hwnd: isize,
    config: UiHintsConfig,
) -> Result<Vec<RawUiElement>, UiHintQueryError> {
    let fg = HWND(foreground_hwnd as *mut core::ffi::c_void);
    if fg.0.is_null() {
        return Err(UiHintQueryError::NoForegroundWindow);
    }

    let windows = discover_windows_in_scope(
        fg,
        config.include_thread_windows,
        config.include_owned_popups,
    )?;

    let _com = ComGuard::init()?;
    let automation: IUIAutomation = unsafe {
        CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            .map_err(|_| UiHintQueryError::UiAutomationInitFailed)?
    };

    let condition = build_interactive_condition(&automation)?;
    let mut raw = Vec::new();
    for hwnd in windows {
        let mut elements = collect_window_elements(&automation, hwnd, &condition, &config)?;
        raw.append(&mut elements);
    }

    raw.retain(stage1_filter);
    raw.sort_by_key(|e| (e.bounds.1, e.bounds.0, e.bounds.2 * e.bounds.3));
    raw.dedup_by_key(|e| e.bounds);
    Ok(raw)
}

struct ComGuard;
impl ComGuard {
    fn init() -> Result<Self, UiHintQueryError> {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|_| UiHintQueryError::UiAutomationInitFailed)?;
        Ok(Self)
    }
}
impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe { CoUninitialize() }
    }
}

fn bool_variant(value: bool) -> VARIANT {
    VARIANT::from(value)
}

fn i32_variant(value: i32) -> VARIANT {
    VARIANT::from(value)
}

fn build_interactive_condition(
    automation: &IUIAutomation,
) -> Result<IUIAutomationCondition, UiHintQueryError> {
    unsafe {
        let enabled = automation
            .CreatePropertyCondition(UIA_IsEnabledPropertyId, &bool_variant(true))
            .map_err(|_| UiHintQueryError::UiAutomationQueryFailed)?;
        let visible = automation
            .CreatePropertyCondition(UIA_IsOffscreenPropertyId, &bool_variant(false))
            .map_err(|_| UiHintQueryError::UiAutomationQueryFailed)?;
        let focusable = automation
            .CreatePropertyCondition(UIA_IsKeyboardFocusablePropertyId, &bool_variant(true))
            .map_err(|_| UiHintQueryError::UiAutomationQueryFailed)?;
        let is_control = automation
            .CreatePropertyCondition(UIA_IsControlElementPropertyId, &bool_variant(true))
            .map_err(|_| UiHintQueryError::UiAutomationQueryFailed)?;
        let is_content = automation
            .CreatePropertyCondition(UIA_IsContentElementPropertyId, &bool_variant(true))
            .map_err(|_| UiHintQueryError::UiAutomationQueryFailed)?;

        let interactive_pattern = or_conditions(
            automation,
            &[
                automation.CreatePropertyCondition(
                    UIA_IsInvokePatternAvailablePropertyId,
                    &bool_variant(true),
                ),
                automation.CreatePropertyCondition(
                    UIA_IsSelectionItemPatternAvailablePropertyId,
                    &bool_variant(true),
                ),
                automation.CreatePropertyCondition(
                    UIA_IsValuePatternAvailablePropertyId,
                    &bool_variant(true),
                ),
            ],
        )?;

        let interactive_control_type = or_conditions(
            automation,
            &[
                automation.CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &i32_variant(UIA_ButtonControlTypeId.0),
                ),
                automation.CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &i32_variant(UIA_HyperlinkControlTypeId.0),
                ),
                automation.CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &i32_variant(UIA_EditControlTypeId.0),
                ),
                automation.CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &i32_variant(UIA_ComboBoxControlTypeId.0),
                ),
                automation.CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &i32_variant(UIA_ListItemControlTypeId.0),
                ),
                automation.CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &i32_variant(UIA_MenuItemControlTypeId.0),
                ),
                automation.CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &i32_variant(UIA_CheckBoxControlTypeId.0),
                ),
                automation.CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &i32_variant(UIA_RadioButtonControlTypeId.0),
                ),
                automation.CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &i32_variant(UIA_TabItemControlTypeId.0),
                ),
                automation.CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &i32_variant(UIA_TreeItemControlTypeId.0),
                ),
            ],
        )?;

        let interactive = automation
            .CreateOrCondition(&interactive_pattern, &interactive_control_type)
            .map_err(|_| UiHintQueryError::UiAutomationQueryFailed)?;
        let control_or_content = automation
            .CreateOrCondition(&is_control, &is_content)
            .map_err(|_| UiHintQueryError::UiAutomationQueryFailed)?;

        let base = automation
            .CreateAndCondition(&enabled, &visible)
            .map_err(|_| UiHintQueryError::UiAutomationQueryFailed)?;
        let base = automation
            .CreateAndCondition(&base, &control_or_content)
            .map_err(|_| UiHintQueryError::UiAutomationQueryFailed)?;
        let focusable_or_interactive = automation
            .CreateOrCondition(&focusable, &interactive)
            .map_err(|_| UiHintQueryError::UiAutomationQueryFailed)?;
        automation
            .CreateAndCondition(&base, &focusable_or_interactive)
            .map_err(|_| UiHintQueryError::UiAutomationQueryFailed)
    }
}

fn or_conditions(
    automation: &IUIAutomation,
    conditions: &[windows::core::Result<IUIAutomationCondition>],
) -> Result<IUIAutomationCondition, UiHintQueryError> {
    let mut iter = conditions.iter();
    let first = iter
        .next()
        .ok_or(UiHintQueryError::UiAutomationQueryFailed)?
        .as_ref()
        .map_err(|_| UiHintQueryError::UiAutomationQueryFailed)?
        .clone();
    let mut acc = first;
    for next in iter {
        let next = next
            .as_ref()
            .map_err(|_| UiHintQueryError::UiAutomationQueryFailed)?;
        acc = unsafe {
            automation
                .CreateOrCondition(&acc, next)
                .map_err(|_| UiHintQueryError::UiAutomationQueryFailed)?
        };
    }
    Ok(acc)
}

fn choose_query_scope(strategy: crate::UiHintQueryStrategy, should_fallback: bool) -> i32 {
    match strategy {
        crate::UiHintQueryStrategy::Descendants => TreeScope_Descendants.0,
        crate::UiHintQueryStrategy::ChildrenThenDescendants => {
            if should_fallback {
                TreeScope_Descendants.0
            } else {
                TreeScope_Children.0
            }
        }
    }
}

fn should_fallback_to_descendants(
    strategy: crate::UiHintQueryStrategy,
    child_count: i32,
    min_targets_before_fallback: i32,
) -> bool {
    matches!(
        strategy,
        crate::UiHintQueryStrategy::ChildrenThenDescendants
    ) && child_count < min_targets_before_fallback
}

fn discover_windows_in_scope(
    foreground: HWND,
    include_thread_windows: bool,
    include_owned_popups: bool,
) -> Result<Vec<HWND>, UiHintQueryError> {
    let fg_thread = unsafe { GetWindowThreadProcessId(foreground, None) };
    let fg_monitor = unsafe { monitor_rect(foreground) };

    let mut windows = vec![foreground];
    if include_thread_windows {
        let mut thread_windows = Vec::<HWND>::new();
        let ok = unsafe {
            EnumThreadWindows(
                fg_thread,
                Some(enum_collect_windows),
                LPARAM((&mut thread_windows as *mut Vec<HWND>) as isize),
            )
        };
        if !ok.as_bool() {
            return Err(UiHintQueryError::EnumerationFailed);
        }
        for hwnd in thread_windows {
            if should_include_thread_window(hwnd, foreground, include_owned_popups, fg_monitor)
                && !windows.iter().any(|w| w.0 == hwnd.0)
            {
                windows.push(hwnd);
            }
        }
    }
    Ok(windows)
}

fn should_include_thread_window(
    hwnd: HWND,
    foreground: HWND,
    include_owned_popups: bool,
    fg_monitor_rect: Option<RECT>,
) -> bool {
    if hwnd.0 == foreground.0 {
        return true;
    }
    if unsafe { !IsWindowVisible(hwnd).as_bool() } {
        return false;
    }
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() || !is_valid_rect(&rect) {
        return false;
    }
    if let Some(fgmr) = fg_monitor_rect {
        if !rects_intersect(rect, fgmr) {
            return false;
        }
    }
    let owner = unsafe { GetWindow(hwnd, GW_OWNER) }.ok();
    if include_owned_popups && owner.map(|h| h.0) == Some(foreground.0) {
        return true;
    }
    owner.map(|h| h.0.is_null()).unwrap_or(true)
}

unsafe extern "system" fn enum_collect_windows(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let windows = &mut *(lparam.0 as *mut Vec<HWND>);
    windows.push(hwnd);
    BOOL(1)
}

fn collect_window_elements(
    automation: &IUIAutomation,
    hwnd: HWND,
    condition: &IUIAutomationCondition,
    config: &UiHintsConfig,
) -> Result<Vec<RawUiElement>, UiHintQueryError> {
    unsafe {
        let root = automation
            .ElementFromHandle(hwnd)
            .map_err(|_| UiHintQueryError::UiAutomationQueryFailed)?;
        let arr: IUIAutomationElementArray = match config.query_strategy {
            crate::UiHintQueryStrategy::Descendants => root
                .FindAll(TreeScope_Descendants, condition)
                .map_err(|_| UiHintQueryError::UiAutomationQueryFailed)?,
            crate::UiHintQueryStrategy::ChildrenThenDescendants => {
                let children = root
                    .FindAll(TreeScope_Children, condition)
                    .map_err(|_| UiHintQueryError::UiAutomationQueryFailed)?;
                let child_len = children
                    .Length()
                    .map_err(|_| UiHintQueryError::UiAutomationQueryFailed)?;
                if !should_fallback_to_descendants(
                    config.query_strategy,
                    child_len,
                    config.min_targets_before_fallback,
                ) {
                    children
                } else {
                    root.FindAll(TreeScope_Descendants, condition)
                        .map_err(|_| UiHintQueryError::UiAutomationQueryFailed)?
                }
            }
        };
        let len = arr
            .Length()
            .map_err(|_| UiHintQueryError::UiAutomationQueryFailed)?;
        let mut out = Vec::new();
        for i in 0..len {
            if let Ok(el) = arr.GetElement(i) {
                if let Some(raw) = normalize_element(&el) {
                    out.push(raw);
                }
            }
        }
        Ok(out)
    }
}

fn normalize_element(el: &IUIAutomationElement) -> Option<RawUiElement> {
    unsafe {
        if !el.CurrentIsEnabled().ok()?.as_bool() {
            return None;
        }
        if el.CurrentIsOffscreen().ok()?.as_bool() {
            return None;
        }
        if !el.CurrentIsKeyboardFocusable().ok()?.as_bool() {
            return None;
        }
        let rect = el.CurrentBoundingRectangle().ok()?;
        if !is_valid_rect(&rect) {
            return None;
        }
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        let mut clickable_point = POINT::default();
        let clickable = if el.GetClickablePoint(&mut clickable_point).ok().is_some() {
            Some((clickable_point.x, clickable_point.y))
        } else {
            Some((rect.left + width / 2, rect.top + height / 2))
        };

        let name = el
            .CurrentName()
            .ok()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let control_type = el
            .CurrentControlType()
            .ok()
            .map(|c| c.0.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        Some(RawUiElement {
            bounds: (rect.left, rect.top, width, height),
            clickable_point: clickable,
            name,
            control_type,
        })
    }
}

fn stage1_filter(e: &RawUiElement) -> bool {
    let (_, _, w, h) = e.bounds;
    w > 0 && h > 0
}

fn is_valid_rect(rect: &RECT) -> bool {
    rect.right > rect.left && rect.bottom > rect.top
}

fn rects_intersect(a: RECT, b: RECT) -> bool {
    a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top
}

unsafe fn monitor_rect(hwnd: HWND) -> Option<RECT> {
    let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    if monitor.0.is_null() {
        return None;
    }
    let mut info = MONITORINFO {
        cbSize: core::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !GetMonitorInfoW(monitor, &mut info).as_bool() {
        return None;
    }
    Some(info.rcMonitor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inclusion_filter_requires_visible_nonzero_intersecting() {
        let fg = HWND(1 as *mut _);
        let other = HWND(2 as *mut _);
        let monitor = Some(RECT {
            left: 0,
            top: 0,
            right: 100,
            bottom: 100,
        });
        // logic-only check
        assert!(rects_intersect(
            RECT {
                left: 10,
                top: 10,
                right: 20,
                bottom: 20
            },
            monitor.unwrap()
        ));
        assert!(!is_valid_rect(&RECT {
            left: 5,
            top: 5,
            right: 5,
            bottom: 10
        }));
        assert!(should_include_thread_window(fg, fg, false, monitor));
        let _ = other;
    }

    #[test]
    fn stage1_filter_rejects_zero_size() {
        assert!(!stage1_filter(&RawUiElement {
            bounds: (0, 0, 0, 10),
            clickable_point: None,
            name: String::new(),
            control_type: String::new()
        }));
        assert!(stage1_filter(&RawUiElement {
            bounds: (0, 0, 1, 1),
            clickable_point: None,
            name: String::new(),
            control_type: String::new()
        }));
    }

    #[test]
    fn strategy_selection_prefers_children_until_fallback_threshold() {
        assert!(should_fallback_to_descendants(
            crate::UiHintQueryStrategy::ChildrenThenDescendants,
            2,
            3
        ));
        assert!(!should_fallback_to_descendants(
            crate::UiHintQueryStrategy::ChildrenThenDescendants,
            3,
            3
        ));
        assert!(!should_fallback_to_descendants(
            crate::UiHintQueryStrategy::Descendants,
            0,
            3
        ));
        assert_eq!(
            choose_query_scope(crate::UiHintQueryStrategy::ChildrenThenDescendants, false),
            TreeScope_Children.0
        );
        assert_eq!(
            choose_query_scope(crate::UiHintQueryStrategy::ChildrenThenDescendants, true),
            TreeScope_Descendants.0
        );
        assert_eq!(
            choose_query_scope(crate::UiHintQueryStrategy::Descendants, false),
            TreeScope_Descendants.0
        );
    }
}
