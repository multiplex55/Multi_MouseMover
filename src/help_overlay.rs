use crate::action::Action;
use crate::key_chord::KeyChord;
use std::cell::RefCell;
use std::ptr;
use std::time::{Duration, Instant};
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

thread_local! {
    static HELP_OVERLAY: RefCell<HelpOverlay> = RefCell::new(HelpOverlay::new());
}

const OVERLAY_WIDTH: i32 = 520;
const MAX_OVERLAY_HEIGHT: i32 = 720;
const PADDING_X: i32 = 18;
const PADDING_Y: i32 = 14;
const TITLE_BODY_GAP: i32 = 8;
const LINE_HEIGHT: i32 = 22;
const CURSOR_OFFSET_X: i32 = 18;
const CURSOR_OFFSET_Y: i32 = 18;
const OVERLAY_ALPHA: u8 = 232;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpBinding {
    pub key: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpOverlayView {
    pub bindings: Vec<HelpBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporaryMessage {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelpOverlayContent {
    Hidden,
    Temporary {
        message: TemporaryMessage,
        expires_at: Instant,
    },
    Help {
        view: HelpOverlayView,
    },
}

#[derive(Debug, Clone)]
struct HelpOverlayState {
    content: HelpOverlayContent,
}

impl HelpOverlayState {
    fn new() -> Self {
        Self {
            content: HelpOverlayContent::Hidden,
        }
    }

    fn show_temporary_tooltip(
        &mut self,
        title: impl Into<String>,
        body: impl Into<String>,
        duration: Duration,
        now: Instant,
    ) {
        if matches!(self.content, HelpOverlayContent::Help { .. }) {
            return;
        }

        self.content = HelpOverlayContent::Temporary {
            message: TemporaryMessage {
                title: title.into(),
                body: body.into(),
            },
            expires_at: now + duration,
        };
    }

    fn show_help_overlay(&mut self, view: HelpOverlayView) {
        self.content = HelpOverlayContent::Help { view };
    }

    fn hide(&mut self) {
        self.content = HelpOverlayContent::Hidden;
    }

    fn update_overlay(&mut self, now: Instant) -> bool {
        if matches!(
            &self.content,
            HelpOverlayContent::Temporary { expires_at, .. } if now >= *expires_at
        ) {
            self.hide();
            return true;
        }
        false
    }

    fn visible_content(&self) -> Option<&HelpOverlayContent> {
        match &self.content {
            HelpOverlayContent::Hidden => None,
            content => Some(content),
        }
    }
}

pub struct HelpOverlay {
    hwnd: Option<HWND>,
    state: HelpOverlayState,
    visible: bool,
}

impl HelpOverlay {
    fn new() -> Self {
        Self {
            hwnd: None,
            state: HelpOverlayState::new(),
            visible: false,
        }
    }

    fn create_window(&mut self) {
        if self.hwnd.is_some() {
            return;
        }

        unsafe {
            let h_instance = GetModuleHandleW(None).unwrap();
            let class = w!("HelpOverlayClass");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(help_window_proc),
                hInstance: h_instance.into(),
                lpszClassName: class,
                style: CS_HREDRAW | CS_VREDRAW,
                hbrBackground: HBRUSH(ptr::null_mut()),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);

            match CreateWindowExW(
                help_overlay_ex_style(),
                class,
                w!("HelpOverlay"),
                WS_POPUP,
                80,
                80,
                OVERLAY_WIDTH,
                160,
                None,
                None,
                Some(h_instance.into()),
                None,
            ) {
                Ok(hwnd) => {
                    apply_layered_attributes(hwnd);
                    let _ = ShowWindow(hwnd, SW_HIDE);
                    self.hwnd = Some(hwnd);
                }
                Err(err) => eprintln!("[help] CreateWindowExW failed: {err:?}"),
            }
        }
    }

    fn show_temporary_tooltip(
        &mut self,
        title: impl Into<String>,
        body: impl Into<String>,
        duration: Duration,
        now: Instant,
    ) {
        self.state
            .show_temporary_tooltip(title, body, duration, now);
        self.sync_window();
    }

    fn show_help_overlay(&mut self, view: HelpOverlayView) {
        self.state.show_help_overlay(view);
        self.sync_window();
    }

    fn hide(&mut self) {
        self.state.hide();
        self.sync_window();
    }

    fn update_overlay(&mut self, now: Instant) {
        if self.state.update_overlay(now) {
            self.sync_window();
        }
    }

    fn sync_window(&mut self) {
        let Some(content) = self.state.visible_content().cloned() else {
            if let Some(hwnd) = self.hwnd {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
            self.visible = false;
            return;
        };

        self.create_window();
        let Some(hwnd) = self.hwnd else {
            return;
        };

        let size = content_size(&content);
        let (x, y) = overlay_position(size);
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                size.0,
                size.1,
                SWP_NOACTIVATE,
            );
            if !self.visible {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                self.visible = true;
            }
        }
        self.request_repaint();
    }

    fn request_repaint(&self) {
        if let Some(hwnd) = self.hwnd {
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
        }
    }

    fn draw(&self, hdc: HDC) {
        let Some(content) = self.state.visible_content() else {
            return;
        };
        let size = content_size(content);
        let rect = RECT {
            left: 0,
            top: 0,
            right: size.0,
            bottom: size.1,
        };

        unsafe {
            let background = CreateSolidBrush(RGB(20, 22, 28));
            if !background.0.is_null() {
                let _ = FillRect(hdc, &rect, background);
                let _ = DeleteObject(background.into());
            }

            let old_bk_mode = SetBkMode(hdc, TRANSPARENT);
            match content {
                HelpOverlayContent::Temporary { message, .. } => {
                    draw_title(hdc, PADDING_X, PADDING_Y, &message.title);
                    draw_body_line(
                        hdc,
                        PADDING_X,
                        PADDING_Y + LINE_HEIGHT + TITLE_BODY_GAP,
                        &message.body,
                    );
                }
                HelpOverlayContent::Help { view } => {
                    draw_title(hdc, PADDING_X, PADDING_Y, "Multi MouseMover Help");
                    let mut y = PADDING_Y + LINE_HEIGHT + TITLE_BODY_GAP;
                    for binding in &view.bindings {
                        let line = format!("{}  -  {}", binding.key, binding.action);
                        draw_body_line(hdc, PADDING_X, y, &line);
                        y += LINE_HEIGHT;
                    }
                }
                HelpOverlayContent::Hidden => {}
            }
            let _ = SetBkMode(hdc, BACKGROUND_MODE(old_bk_mode as u32));
        }
    }
}

fn help_overlay_ex_style() -> WINDOW_EX_STYLE {
    WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE
}

fn apply_layered_attributes(hwnd: HWND) {
    unsafe {
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), OVERLAY_ALPHA, LWA_ALPHA);
    }
}

fn content_size(content: &HelpOverlayContent) -> (i32, i32) {
    let body_lines = match content {
        HelpOverlayContent::Hidden => 0,
        HelpOverlayContent::Temporary { .. } => 1,
        HelpOverlayContent::Help { view } => view.bindings.len().max(1) as i32,
    };
    let height = PADDING_Y * 2 + LINE_HEIGHT + TITLE_BODY_GAP + body_lines * LINE_HEIGHT;
    (OVERLAY_WIDTH, height.min(MAX_OVERLAY_HEIGHT))
}

fn overlay_position(size: (i32, i32)) -> (i32, i32) {
    unsafe {
        let mut point = POINT::default();
        if GetCursorPos(&mut point).is_err() {
            return (80, 80);
        }

        let screen_left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let screen_top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let screen_width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let screen_height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        let max_x = (screen_left + screen_width - size.0).max(screen_left);
        let max_y = (screen_top + screen_height - size.1).max(screen_top);
        (
            (point.x + CURSOR_OFFSET_X).clamp(screen_left, max_x),
            (point.y + CURSOR_OFFSET_Y).clamp(screen_top, max_y),
        )
    }
}

fn draw_title(hdc: HDC, x: i32, y: i32, text: &str) {
    unsafe {
        let old_text_color = SetTextColor(hdc, RGB(255, 255, 255));
        let text: Vec<u16> = text.encode_utf16().collect();
        let _ = TextOutW(hdc, x, y, &text);
        let _ = SetTextColor(hdc, old_text_color);
    }
}

fn draw_body_line(hdc: HDC, x: i32, y: i32, text: &str) {
    unsafe {
        let old_text_color = SetTextColor(hdc, RGB(218, 224, 235));
        let text: Vec<u16> = text.encode_utf16().collect();
        let _ = TextOutW(hdc, x, y, &text);
        let _ = SetTextColor(hdc, old_text_color);
    }
}

#[allow(non_snake_case)]
fn RGB(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(((b as u32) << 16) | ((g as u32) << 8) | (r as u32))
}

extern "system" fn help_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let ps = &mut PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, ps) };
            HELP_OVERLAY.with(|overlay| overlay.borrow().draw(hdc));
            unsafe {
                let _ = EndPaint(hwnd, ps);
            }
            LRESULT(0)
        }
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_DESTROY => LRESULT(0),
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

pub fn help_view_from_bindings<I>(bindings: I) -> HelpOverlayView
where
    I: IntoIterator<Item = (KeyChord, Action)>,
{
    let mut bindings: Vec<HelpBinding> = bindings
        .into_iter()
        .map(|(chord, action)| HelpBinding {
            key: format_key_chord(chord),
            action: format_action(action),
        })
        .collect();
    bindings.sort_by(|left, right| {
        left.action
            .cmp(&right.action)
            .then(left.key.cmp(&right.key))
    });
    HelpOverlayView { bindings }
}

pub fn show_temporary_tooltip(
    title: impl Into<String>,
    body: impl Into<String>,
    duration: Duration,
) {
    show_temporary_tooltip_at(title, body, duration, Instant::now());
}

pub fn show_temporary_tooltip_at(
    title: impl Into<String>,
    body: impl Into<String>,
    duration: Duration,
    now: Instant,
) {
    HELP_OVERLAY.with(|overlay| {
        overlay
            .borrow_mut()
            .show_temporary_tooltip(title, body, duration, now)
    });
}

pub fn show_help_overlay(view: HelpOverlayView) {
    HELP_OVERLAY.with(|overlay| overlay.borrow_mut().show_help_overlay(view));
}

pub fn hide_help_overlay() {
    HELP_OVERLAY.with(|overlay| overlay.borrow_mut().hide());
}

pub fn update_overlay(now: Instant) {
    HELP_OVERLAY.with(|overlay| overlay.borrow_mut().update_overlay(now));
}

fn format_action(action: Action) -> String {
    match action {
        Action::JumpModeProfile(profile) => format!("jump_mode_profile:{profile}"),
        Action::MovementProfileSelect(profile) => format!("movement_profile:{profile}"),
        Action::WheelProfileSelect(profile) => format!("wheel_profile:{profile}"),
        other => format!("{other:?}")
            .chars()
            .enumerate()
            .flat_map(|(index, ch)| {
                if index > 0 && ch.is_ascii_uppercase() {
                    vec!['_', ch.to_ascii_lowercase()]
                } else {
                    vec![ch.to_ascii_lowercase()]
                }
            })
            .collect(),
    }
}

fn format_key_chord(chord: KeyChord) -> String {
    let mut parts = Vec::new();
    if chord.ctrl {
        parts.push("Ctrl".to_string());
    }
    if chord.right_alt {
        parts.push("RightAlt".to_string());
    } else if chord.alt {
        parts.push("Alt".to_string());
    }
    if chord.shift {
        parts.push("Shift".to_string());
    }
    if chord.win {
        parts.push("Win".to_string());
    }
    parts.push(format!("{:?}", chord.key));
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::VirtualKey;

    fn sample_view() -> HelpOverlayView {
        HelpOverlayView {
            bindings: vec![HelpBinding {
                key: "H".to_string(),
                action: "show_help".to_string(),
            }],
        }
    }

    #[test]
    fn help_contents_are_generated_from_keybindings() {
        let view = help_view_from_bindings([
            (KeyChord::from_key(VirtualKey::F), Action::JumpMode),
            (KeyChord::from_key(VirtualKey::H), Action::ShowHelp),
        ]);

        assert!(view
            .bindings
            .iter()
            .any(|binding| { binding.key == "F" && binding.action == "jump_mode" }));
        assert!(view
            .bindings
            .iter()
            .any(|binding| { binding.key == "H" && binding.action == "show_help" }));
    }

    #[test]
    fn style_contains_click_through_topmost_and_non_activate_bits() {
        let style = help_overlay_ex_style();

        assert!(style.contains(WS_EX_LAYERED));
        assert!(style.contains(WS_EX_TOPMOST));
        assert!(style.contains(WS_EX_TOOLWINDOW));
        assert!(style.contains(WS_EX_TRANSPARENT));
        assert!(style.contains(WS_EX_NOACTIVATE));
    }

    #[test]
    fn temporary_tooltip_visible_before_expiry() {
        let now = Instant::now();
        let mut state = HelpOverlayState::new();

        state.show_temporary_tooltip("Speed", "Mouse speed 2", Duration::from_millis(700), now);
        state.update_overlay(now + Duration::from_millis(699));

        assert!(matches!(
            state.content,
            HelpOverlayContent::Temporary { .. }
        ));
    }

    #[test]
    fn temporary_tooltip_hidden_after_expiry() {
        let now = Instant::now();
        let mut state = HelpOverlayState::new();

        state.show_temporary_tooltip("Speed", "Mouse speed 2", Duration::from_millis(700), now);
        state.update_overlay(now + Duration::from_millis(700));

        assert_eq!(state.content, HelpOverlayContent::Hidden);
    }

    #[test]
    fn help_overlay_does_not_auto_expire() {
        let now = Instant::now();
        let mut state = HelpOverlayState::new();

        state.show_help_overlay(sample_view());
        state.update_overlay(now + Duration::from_secs(60));

        assert!(matches!(state.content, HelpOverlayContent::Help { .. }));
    }

    #[test]
    fn slash_help_replaces_temporary_content() {
        let now = Instant::now();
        let mut state = HelpOverlayState::new();
        let view = sample_view();

        state.show_temporary_tooltip("Speed", "Mouse speed 2", Duration::from_millis(700), now);
        state.show_help_overlay(view.clone());

        assert_eq!(state.content, HelpOverlayContent::Help { view });
    }

    #[test]
    fn temporary_notifications_do_not_override_visible_help() {
        let now = Instant::now();
        let mut state = HelpOverlayState::new();
        let view = sample_view();

        state.show_help_overlay(view.clone());
        state.show_temporary_tooltip("Speed", "Mouse speed 2", Duration::from_millis(700), now);

        assert_eq!(state.content, HelpOverlayContent::Help { view });
    }
}
