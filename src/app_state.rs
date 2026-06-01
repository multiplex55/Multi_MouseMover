use crate::action::Action;
use crate::action_handler::{final_adjust_control_for_event, FinalAdjustControl};
use crate::grid_session::GridSession;
use crate::jump_grid::generate_labels;
use crate::jump_session::{JumpRegion, JumpSession, JumpSessionUpdate, JumpStage};
use crate::jump_view::{
    FinalAdjustOverlayView, GridOverlayMetadata, JumpOverlayView, JumpStageMetadata, JumpVisuals,
};
use crate::key_chord::{KeyChord, RuntimeSystemBindings};
use crate::keyboard::{
    preserved_shortcut_risk_for_chord, BindingCandidate, KeyBindings, OwnedModifierKeys, VirtualKey,
};
#[cfg(test)]
use crate::Config;
use crate::JumpConfig;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: VirtualKey,
    pub is_down: bool,
    pub alt_down: bool,
    pub left_alt_down: bool,
    pub right_alt_down: bool,
    pub ctrl_down: bool,
    pub left_ctrl_down: bool,
    pub right_ctrl_down: bool,
    pub shift_down: bool,
    pub left_shift_down: bool,
    pub right_shift_down: bool,
    pub win_down: bool,
    pub left_win_down: bool,
    pub right_win_down: bool,
}

impl KeyEvent {
    pub fn new(key: VirtualKey, is_down: bool) -> Self {
        Self {
            key,
            is_down,
            alt_down: false,
            left_alt_down: false,
            right_alt_down: false,
            ctrl_down: false,
            left_ctrl_down: false,
            right_ctrl_down: false,
            shift_down: false,
            left_shift_down: false,
            right_shift_down: false,
            win_down: false,
            left_win_down: false,
            right_win_down: false,
        }
    }

    pub fn with_modifier_state(
        key: VirtualKey,
        is_down: bool,
        modifiers: crate::input_decode::ModifierSnapshot,
    ) -> Self {
        Self {
            key,
            is_down,
            alt_down: modifiers.left_alt || modifiers.right_alt,
            left_alt_down: modifiers.left_alt,
            right_alt_down: modifiers.right_alt,
            ctrl_down: modifiers.left_ctrl || modifiers.right_ctrl,
            left_ctrl_down: modifiers.left_ctrl,
            right_ctrl_down: modifiers.right_ctrl,
            shift_down: modifiers.left_shift || modifiers.right_shift,
            left_shift_down: modifiers.left_shift,
            right_shift_down: modifiers.right_shift,
            win_down: modifiers.left_win || modifiers.right_win,
            left_win_down: modifiers.left_win,
            right_win_down: modifiers.right_win,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppCommand {
    ToggleActiveMode,
    SetActiveMode {
        active: bool,
    },
    Exit,
    ReloadConfig,
    PanicReset,
    ToggleHelp,
    HideHelp,
    HelpInput(crate::help_overlay::HelpInput),
    EnterJumpMode {
        activation_key: VirtualKey,
        profile: Option<String>,
    },
    EnterGridMode {
        activation_key: VirtualKey,
    },
    EnterUiHintMode {
        activation_key: VirtualKey,
    },
    KeyAction {
        action: Action,
        is_down: bool,
    },
    JumpInput(KeyEvent, Option<Action>),
    GridInput(KeyEvent, Option<Action>),
    UiHintInput(KeyEvent),
    EnterKeybindLookupMode,
    KeybindLookupInput(KeyEvent),
    EnterBookmarkMode {
        activation_key: VirtualKey,
    },
    ShowBookmarks,
    RecallBookmarkSlot(u8),
    SetBookmarkSlot(u8),
    ClearBookmarkSlot(u8),
    ClearAllBookmarks,
    CancelBookmarkMode,
    ApplyBookmarkName {
        slot: u8,
        name: Option<String>,
    },
    UiHintQueryCompleted {
        query_id: u64,
        elements: Vec<crate::windows_uia::RawUiElement>,
    },
    UiHintQueryFailed {
        query_id: u64,
    },
}

#[derive(Debug)]
pub struct AppState {
    key_events: VecDeque<KeyEvent>,
    commands: VecDeque<AppCommand>,
    bound_chords: HashSet<KeyChord>,
    active_keys: HashSet<VirtualKey>,
    active_trigger_chords: HashSet<KeyChord>,
    active_triggers: std::collections::HashMap<VirtualKey, ActiveTrigger>,
    owned_modifiers: OwnedModifierKeys,
    reconciled_released_keys: HashSet<VirtualKey>,
    held_physical_keys: HashMap<VirtualKey, HeldKeyState>,
    swallow_owned_modifiers: bool,
    debug_input: bool,
    jump: JumpState,
    grid: GridState,
    ui_hints: UiHintState,
    bookmark_mode: BookmarkModeState,
    keybind_lookup: KeybindLookupState,
    active_mode: bool,
    preserve_global_shortcuts: bool,
    right_alt_suppresses_synthetic_ctrl: bool,
    system_bindings: RuntimeSystemBindings,
    help_visible: bool,
    grid_direction_labels: GridDirectionLabels,
    bookmark_cancel_key: VirtualKey,
    bookmark_clear_modifier_key: VirtualKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeldKeyState {
    pub pressed_at: std::time::Instant,
    pub last_event_at: std::time::Instant,
    pub physically_up_since: Option<std::time::Instant>,
    pub was_action_trigger: bool,
    pub was_owned_modifier: bool,
    pub reconciled_release_sent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveTrigger {
    pub trigger_key: VirtualKey,
    pub resolved_chord: KeyChord,
    pub resolved_action: Action,
    pub is_continuous: bool,
}

#[derive(Debug)]
pub enum UiHintState {
    Inactive,
    Querying {
        activation_key: VirtualKey,
        query_id: u64,
        foreground_hwnd: isize,
    },
    Active {
        #[allow(dead_code)]
        activation_key: VirtualKey,
        query_id: u64,
        foreground_hwnd: isize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeybindLookupState {
    Inactive,
    WaitingForChord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeybindLookupResult {
    pub chord_display: String,
    pub matched_action: Option<Action>,
    pub matched_binding: Option<KeyChord>,
    pub swallowed: bool,
    pub preserved_shortcut: bool,
    pub competing_bindings: Vec<KeybindLookupBinding>,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeybindLookupBinding {
    pub chord: KeyChord,
    pub action: Action,
}

#[derive(Debug)]
pub enum BookmarkModeState {
    Inactive,
    Active {
        #[allow(dead_code)]
        activation_key: VirtualKey,
        #[allow(dead_code)]
        activation_key_released: bool,
        clear_modifier_held: bool,
    },
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum JumpState {
    Inactive,
    Active {
        session: JumpSession,
        activation_key: VirtualKey,
        activation_key_released: bool,
        stage_metadata: Vec<JumpStageMetadata>,
        visuals: crate::JumpVisualsConfig,
        final_adjust_config: crate::FinalAdjustConfig,
    },
}

#[derive(Debug)]
pub enum GridState {
    Inactive,
    Active {
        session: GridSession,
        activation_key: VirtualKey,
        activation_key_released: bool,
        line_visible: bool,
        show_direction_labels: bool,
        direction_labels: GridDirectionLabels,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeContext {
    Inactive,
    Active,
    Jump,
    Grid,
    UiHintQuerying,
    UiHintActive,
    Bookmark,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridDirectionLabels {
    pub up: String,
    pub left: String,
    pub down: String,
    pub right: String,
}

fn help_input_from_event(
    event: &KeyEvent,
    action: Option<&Action>,
) -> Option<crate::help_overlay::HelpInput> {
    if event.key == VirtualKey::Escape {
        return Some(crate::help_overlay::HelpInput::Escape);
    }
    match action {
        Some(Action::HelpSearch) | Some(Action::ShowHelp) | Some(Action::HelpMode) => {
            return Some(crate::help_overlay::HelpInput::Search);
        }
        Some(Action::HelpNextSection) => return Some(crate::help_overlay::HelpInput::NextSection),
        Some(Action::HelpPreviousSection) => {
            return Some(crate::help_overlay::HelpInput::PreviousSection)
        }
        Some(Action::HelpNextPage) => return Some(crate::help_overlay::HelpInput::NextPage),
        Some(Action::HelpPreviousPage) => {
            return Some(crate::help_overlay::HelpInput::PreviousPage)
        }
        _ => {}
    }
    match event.key {
        VirtualKey::Oem2 => Some(crate::help_overlay::HelpInput::Search),
        VirtualKey::Tab if event.shift_down => {
            Some(crate::help_overlay::HelpInput::PreviousSection)
        }
        VirtualKey::Tab => Some(crate::help_overlay::HelpInput::NextSection),
        VirtualKey::PageDown => Some(crate::help_overlay::HelpInput::NextPage),
        VirtualKey::PageUp => Some(crate::help_overlay::HelpInput::PreviousPage),
        VirtualKey::Backspace => Some(crate::help_overlay::HelpInput::Backspace),
        _ => help_text_char(event).map(crate::help_overlay::HelpInput::Append),
    }
}

fn help_text_char(event: &KeyEvent) -> Option<char> {
    if event.ctrl_down || event.alt_down || event.win_down {
        return None;
    }
    let shifted = event.shift_down;
    match event.key {
        VirtualKey::A => Some(if shifted { 'A' } else { 'a' }),
        VirtualKey::B => Some(if shifted { 'B' } else { 'b' }),
        VirtualKey::C => Some(if shifted { 'C' } else { 'c' }),
        VirtualKey::D => Some(if shifted { 'D' } else { 'd' }),
        VirtualKey::E => Some(if shifted { 'E' } else { 'e' }),
        VirtualKey::F => Some(if shifted { 'F' } else { 'f' }),
        VirtualKey::G => Some(if shifted { 'G' } else { 'g' }),
        VirtualKey::H => Some(if shifted { 'H' } else { 'h' }),
        VirtualKey::I => Some(if shifted { 'I' } else { 'i' }),
        VirtualKey::J => Some(if shifted { 'J' } else { 'j' }),
        VirtualKey::K => Some(if shifted { 'K' } else { 'k' }),
        VirtualKey::L => Some(if shifted { 'L' } else { 'l' }),
        VirtualKey::M => Some(if shifted { 'M' } else { 'm' }),
        VirtualKey::N => Some(if shifted { 'N' } else { 'n' }),
        VirtualKey::O => Some(if shifted { 'O' } else { 'o' }),
        VirtualKey::P => Some(if shifted { 'P' } else { 'p' }),
        VirtualKey::Q => Some(if shifted { 'Q' } else { 'q' }),
        VirtualKey::R => Some(if shifted { 'R' } else { 'r' }),
        VirtualKey::S => Some(if shifted { 'S' } else { 's' }),
        VirtualKey::T => Some(if shifted { 'T' } else { 't' }),
        VirtualKey::U => Some(if shifted { 'U' } else { 'u' }),
        VirtualKey::V => Some(if shifted { 'V' } else { 'v' }),
        VirtualKey::W => Some(if shifted { 'W' } else { 'w' }),
        VirtualKey::X => Some(if shifted { 'X' } else { 'x' }),
        VirtualKey::Y => Some(if shifted { 'Y' } else { 'y' }),
        VirtualKey::Z => Some(if shifted { 'Z' } else { 'z' }),
        VirtualKey::Num0 => Some('0'),
        VirtualKey::Num1 => Some('1'),
        VirtualKey::Num2 => Some('2'),
        VirtualKey::Num3 => Some('3'),
        VirtualKey::Num4 => Some('4'),
        VirtualKey::Num5 => Some('5'),
        VirtualKey::Num6 => Some('6'),
        VirtualKey::Num7 => Some('7'),
        VirtualKey::Num8 => Some('8'),
        VirtualKey::Num9 => Some('9'),
        VirtualKey::Space => Some(' '),
        VirtualKey::OemMinus => Some(if shifted { '_' } else { '-' }),
        VirtualKey::OemPeriod => Some('.'),
        VirtualKey::OemComma => Some(','),
        _ => None,
    }
}

impl Default for GridDirectionLabels {
    fn default() -> Self {
        Self {
            up: "W".to_string(),
            left: "A".to_string(),
            down: "S".to_string(),
            right: "D".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GridInputUpdate {
    Consumed,
    Updated {
        region: JumpRegion,
        move_cursor: bool,
    },
    Cancelled,
    Completed {
        x: i32,
        y: i32,
        region: JumpRegion,
    },
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum JumpOverlayResolution {
    Hidden,
    Visible(JumpOverlayView),
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            key_events: VecDeque::new(),
            commands: VecDeque::new(),
            bound_chords: HashSet::new(),
            active_keys: HashSet::new(),
            active_trigger_chords: HashSet::new(),
            active_triggers: std::collections::HashMap::new(),
            owned_modifiers: OwnedModifierKeys::default(),
            reconciled_released_keys: HashSet::new(),
            held_physical_keys: HashMap::new(),
            swallow_owned_modifiers: true,
            debug_input: false,
            jump: JumpState::Inactive,
            grid: GridState::Inactive,
            ui_hints: UiHintState::Inactive,
            bookmark_mode: BookmarkModeState::Inactive,
            keybind_lookup: KeybindLookupState::Inactive,
            active_mode: true,
            preserve_global_shortcuts: true,
            right_alt_suppresses_synthetic_ctrl: true,
            system_bindings: RuntimeSystemBindings::default(),
            help_visible: false,
            grid_direction_labels: GridDirectionLabels::default(),
            bookmark_cancel_key: VirtualKey::Escape,
            bookmark_clear_modifier_key: VirtualKey::Backspace,
        }
    }
}

impl AppState {
    pub fn enqueue_key_event(&mut self, event: KeyEvent) {
        self.key_events.push_back(event);
    }

    pub fn pop_key_event(&mut self) -> Option<KeyEvent> {
        self.key_events.pop_front()
    }

    pub fn enqueue_command(&mut self, command: AppCommand) {
        self.commands.push_back(command);
    }

    pub fn pop_command(&mut self) -> Option<AppCommand> {
        self.commands.pop_front()
    }

    #[cfg(test)]
    pub fn set_bound_keys<I>(&mut self, keys: I)
    where
        I: IntoIterator<Item = VirtualKey>,
    {
        self.bound_chords = keys.into_iter().map(KeyChord::from_key).collect();
    }

    pub fn set_bound_chords<I>(&mut self, chords: I)
    where
        I: IntoIterator<Item = KeyChord>,
    {
        self.bound_chords = chords.into_iter().collect();
    }

    pub fn set_owned_modifiers<I>(&mut self, modifiers: I)
    where
        I: IntoIterator<Item = VirtualKey>,
    {
        self.owned_modifiers = OwnedModifierKeys::default();
        for key in modifiers {
            self.owned_modifiers.insert(key);
        }
    }

    pub fn set_debug_input(&mut self, enabled: bool) {
        self.debug_input = enabled;
    }

    fn is_modifier_reconcile_candidate(key: VirtualKey) -> bool {
        matches!(
            key,
            VirtualKey::LeftShift
                | VirtualKey::RightShift
                | VirtualKey::LeftCtrl
                | VirtualKey::RightCtrl
                | VirtualKey::LeftAlt
                | VirtualKey::RightAlt
                | VirtualKey::LeftWin
                | VirtualKey::RightWin
        )
    }

    pub fn reconcile_stale_keys<F>(&mut self, stale_after: Duration, mut is_key_physically_down: F)
    where
        F: FnMut(VirtualKey) -> bool,
    {
        let now = std::time::Instant::now();
        let mut stale_keys = Vec::new();

        for (key, state) in self.held_physical_keys.iter_mut() {
            if !Self::is_modifier_reconcile_candidate(*key) {
                continue;
            }

            if is_key_physically_down(*key) {
                state.physically_up_since = None;
                continue;
            }

            let up_since = state.physically_up_since.get_or_insert(now);
            if now.duration_since(*up_since) >= stale_after {
                stale_keys.push((*key, *state));
            }
        }

        for (key, held_state) in stale_keys {
            self.release_reconciled_stale_key(key, held_state);
        }
    }

    fn release_reconciled_stale_key(&mut self, key: VirtualKey, held_state: HeldKeyState) {
        let stale_owned_modifier = held_state.was_owned_modifier;
        let stale_trigger = held_state.was_action_trigger;
        let mut synthesized_action = None;

        self.held_physical_keys.remove(&key);
        self.active_keys.remove(&key);
        self.reconciled_released_keys.insert(key);
        if stale_trigger {
            synthesized_action = self.resolve_key_up_action(KeyEvent::new(key, false), None);
        }

        if let Some(action) = synthesized_action {
            self.enqueue_command(AppCommand::KeyAction {
                action,
                is_down: false,
            });
        }

        if self.debug_input {
            eprintln!(
                "[debug-input] reconciled stale key={:?} trigger={} owned_modifier={} held_ms={} reconciled_release_sent={}",
                key,
                stale_trigger,
                stale_owned_modifier,
                held_state.pressed_at.elapsed().as_millis(),
                held_state.reconciled_release_sent
            );
        }
    }

    pub fn set_active_mode(&mut self, active_mode: bool) {
        self.active_mode = active_mode;
        if !active_mode {
            self.clear_active_action_keys_and_exit_exclusive_modes();
            self.hide_help();
        }
    }

    pub fn active_mode(&self) -> bool {
        self.active_mode
    }

    #[cfg(test)]
    pub fn has_active_action_keys(&self) -> bool {
        !self.active_keys.is_empty() || !self.active_trigger_chords.is_empty()
    }

    pub fn clear_active_action_keys(&mut self) {
        self.active_keys.clear();
        self.active_trigger_chords.clear();
        self.active_triggers.clear();
    }

    pub fn clear_active_action_keys_and_exit_exclusive_modes(&mut self) {
        self.clear_active_action_keys();
        self.exit_jump_mode();
        self.exit_grid_mode();
        self.exit_ui_hint_mode();
        self.exit_bookmark_mode();
        self.exit_keybind_lookup_mode();
    }

    pub fn clear_runtime_input_state(&mut self) {
        self.clear_active_action_keys_and_exit_exclusive_modes();
        self.held_physical_keys.clear();
        self.reconciled_released_keys.clear();
        self.hide_help();
    }

    #[cfg(test)]
    pub fn held_physical_key_count(&self) -> usize {
        self.held_physical_keys.len()
    }

    #[cfg(test)]
    pub fn reconciled_released_key_count(&self) -> usize {
        self.reconciled_released_keys.len()
    }

    pub fn set_system_bindings(&mut self, system_bindings: RuntimeSystemBindings) {
        self.system_bindings = system_bindings;
    }
    pub fn set_bookmark_keys(&mut self, cancel_key: VirtualKey, clear_modifier_key: VirtualKey) {
        self.bookmark_cancel_key = cancel_key;
        self.bookmark_clear_modifier_key = clear_modifier_key;
    }

    pub fn set_grid_direction_labels(&mut self, labels: GridDirectionLabels) {
        self.grid_direction_labels = labels;
    }

    pub fn is_toggle_active_binding(&self, event: &KeyEvent) -> bool {
        self.system_bindings.toggle_active.matches_event(event)
    }

    pub fn is_panic_reset_binding(&self, event: &KeyEvent) -> bool {
        if self.system_bindings.panic_reset_ignore_extra_modifiers {
            self.system_bindings
                .panic_reset
                .matches_event_ignoring_extra_modifiers(event)
        } else {
            self.system_bindings.panic_reset.matches_event(event)
        }
    }

    pub fn is_exit_binding(&self, event: &KeyEvent) -> bool {
        if self.system_bindings.exit_ignore_extra_modifiers {
            self.system_bindings
                .exit
                .matches_event_ignoring_extra_modifiers(event)
        } else {
            self.system_bindings.exit.matches_event(event)
        }
    }

    pub fn is_system_binding(&self, event: &KeyEvent) -> bool {
        let matched = self.is_toggle_active_binding(event)
            || self.is_exit_binding(event)
            || self.is_panic_reset_binding(event);
        if matched && self.debug_input {
            eprintln!(
                "[debug-input] system-match key={:?} down={}",
                event.key, event.is_down
            );
        }
        matched
    }

    pub fn is_toggle_active_key_down_event(&self, event: &KeyEvent) -> bool {
        event.is_down && self.is_toggle_active_binding(event)
    }

    pub fn is_exit_key_down_event(&self, event: &KeyEvent) -> bool {
        event.is_down && self.is_exit_binding(event)
    }

    pub fn is_system_key_down_event(&self, event: &KeyEvent) -> bool {
        self.is_toggle_active_key_down_event(event)
            || self.is_exit_key_down_event(event)
            || (event.is_down && self.is_panic_reset_binding(event))
    }

    pub fn is_jump_active(&self) -> bool {
        matches!(self.jump, JumpState::Active { .. })
    }

    pub fn is_grid_active(&self) -> bool {
        matches!(self.grid, GridState::Active { .. })
    }

    pub fn is_ui_hint_active(&self) -> bool {
        matches!(self.ui_hints, UiHintState::Active { .. })
    }

    pub fn is_ui_hint_querying(&self) -> bool {
        matches!(self.ui_hints, UiHintState::Querying { .. })
    }

    pub fn exit_ui_hint_mode(&mut self) {
        self.ui_hints = UiHintState::Inactive;
    }
    pub fn enter_ui_hint_querying(
        &mut self,
        activation_key: VirtualKey,
        query_id: u64,
        foreground_hwnd: isize,
    ) {
        self.exit_jump_mode();
        self.exit_grid_mode();
        self.ui_hints = UiHintState::Querying {
            activation_key,
            query_id,
            foreground_hwnd,
        };
    }

    pub fn activate_ui_hint_if_querying(&mut self, query_id: u64) -> bool {
        if let UiHintState::Querying {
            activation_key,
            query_id: active_query_id,
            foreground_hwnd,
        } = self.ui_hints
        {
            if active_query_id == query_id {
                self.ui_hints = UiHintState::Active {
                    activation_key,
                    query_id,
                    foreground_hwnd,
                };
                return true;
            }
        }
        false
    }

    pub fn current_ui_hint_query_id(&self) -> Option<u64> {
        match self.ui_hints {
            UiHintState::Querying { query_id, .. } | UiHintState::Active { query_id, .. } => {
                Some(query_id)
            }
            UiHintState::Inactive => None,
        }
    }

    pub fn current_ui_hint_foreground_hwnd(&self) -> Option<isize> {
        match self.ui_hints {
            UiHintState::Querying {
                foreground_hwnd, ..
            }
            | UiHintState::Active {
                foreground_hwnd, ..
            } => Some(foreground_hwnd),
            UiHintState::Inactive => None,
        }
    }

    #[allow(dead_code)]
    pub fn is_ui_hint_active_or_querying(&self) -> bool {
        self.is_ui_hint_active() || self.is_ui_hint_querying()
    }

    pub fn is_exclusive_mode_active(&self) -> bool {
        self.is_jump_active()
            || self.is_grid_active()
            || self.is_ui_hint_active()
            || self.is_ui_hint_querying()
            || self.is_bookmark_mode_active()
            || self.is_keybind_lookup_waiting()
    }

    pub fn is_bookmark_mode_active(&self) -> bool {
        matches!(self.bookmark_mode, BookmarkModeState::Active { .. })
    }

    pub fn is_keybind_lookup_waiting(&self) -> bool {
        matches!(self.keybind_lookup, KeybindLookupState::WaitingForChord)
    }

    pub fn enter_keybind_lookup_mode(&mut self) {
        self.clear_active_action_keys();
        self.exit_jump_mode();
        self.exit_grid_mode();
        self.exit_ui_hint_mode();
        self.exit_bookmark_mode();
        self.keybind_lookup = KeybindLookupState::WaitingForChord;
    }

    pub fn exit_keybind_lookup_mode(&mut self) {
        self.keybind_lookup = KeybindLookupState::Inactive;
    }

    pub fn enter_bookmark_mode(&mut self, activation_key: VirtualKey) {
        self.clear_active_action_keys();
        self.exit_jump_mode();
        self.exit_grid_mode();
        self.exit_ui_hint_mode();
        self.bookmark_mode = BookmarkModeState::Active {
            activation_key,
            activation_key_released: false,
            clear_modifier_held: false,
        };
    }

    pub fn exit_bookmark_mode(&mut self) {
        self.bookmark_mode = BookmarkModeState::Inactive;
    }

    pub fn jump_stage_status(&self) -> Option<(usize, usize)> {
        match &self.jump {
            JumpState::Active { session, .. } => {
                Some((session.stage_index + 1, session.stages.len()))
            }
            JumpState::Inactive => None,
        }
    }

    pub fn final_adjust_active(&self) -> bool {
        match &self.jump {
            JumpState::Active { session, .. } => session.final_adjust.is_some(),
            JumpState::Inactive => false,
        }
    }

    pub fn help_visible(&self) -> bool {
        self.help_visible
    }

    pub fn mode_context(&self) -> ModeContext {
        if !self.active_mode {
            return ModeContext::Inactive;
        }
        if self.is_bookmark_mode_active() {
            return ModeContext::Bookmark;
        }

        if self.is_jump_active() {
            return ModeContext::Jump;
        }
        if self.is_grid_active() {
            return ModeContext::Grid;
        }
        if self.is_ui_hint_querying() {
            return ModeContext::UiHintQuerying;
        }
        if self.is_ui_hint_active() {
            return ModeContext::UiHintActive;
        }
        ModeContext::Active
    }

    pub fn toggle_help(&mut self) {
        self.help_visible = !self.help_visible;
    }

    pub fn hide_help(&mut self) {
        self.help_visible = false;
    }

    pub fn enter_jump_mode(
        &mut self,
        config: &JumpConfig,
        final_adjust_config: crate::FinalAdjustConfig,
        virtual_screen_region: JumpRegion,
        activation_key: VirtualKey,
    ) -> bool {
        let stage_metadata = jump_stage_metadata(config);
        let stages = stage_metadata
            .iter()
            .map(|stage| {
                JumpStage::with_selection_keys(
                    stage.grid_size.0,
                    stage.grid_size.1,
                    stage.aim_point,
                    stage.aim_offset_x_px,
                    stage.aim_offset_y_px,
                    stage.target_margin_percent,
                    stage.visual_context_margin_percent,
                    stage.target_region_mode,
                    config.hints.selection_keys.chars().collect(),
                )
            })
            .collect();

        let Some(session) = JumpSession::new(virtual_screen_region, stages) else {
            self.exit_jump_mode();
            return false;
        };

        self.exit_grid_mode();
        self.jump = JumpState::Active {
            session,
            activation_key,
            activation_key_released: false,
            stage_metadata,
            visuals: config.visuals,
            final_adjust_config,
        };
        true
    }

    #[allow(clippy::too_many_arguments)]
    pub fn enter_grid_mode(
        &mut self,
        monitor_bounds: JumpRegion,
        initial_region: JumpRegion,
        min_width: i32,
        min_height: i32,
        move_cursor_each_step: bool,
        line_visible: bool,
        show_direction_labels: bool,
        activation_key: VirtualKey,
    ) -> bool {
        let Some(mut session) = GridSession::start(
            monitor_bounds,
            min_width,
            min_height,
            Some(move_cursor_each_step),
        ) else {
            self.exit_grid_mode();
            return false;
        };

        if !initial_region.is_valid() {
            self.exit_grid_mode();
            return false;
        }
        session.current_region = initial_region;

        self.exit_jump_mode();
        self.grid = GridState::Active {
            session,
            activation_key,
            activation_key_released: false,
            line_visible,
            show_direction_labels,
            direction_labels: self.grid_direction_labels.clone(),
        };
        true
    }

    pub fn handle_jump_input(
        &mut self,
        event: KeyEvent,
        action: Option<Action>,
    ) -> Option<JumpSessionUpdate> {
        if self.is_activation_key_event(&event) {
            return Some(JumpSessionUpdate::Consumed);
        }

        let JumpState::Active {
            session,
            final_adjust_config,
            ..
        } = &mut self.jump
        else {
            return None;
        };

        if session.final_adjust.is_some() {
            let update = match final_adjust_control_for_event(&event, action, final_adjust_config) {
                Some(FinalAdjustControl::Nudge { dx, dy }) => session.nudge_final_adjust(dx, dy),
                Some(FinalAdjustControl::Confirm) => session.confirm_final_adjust(),
                Some(FinalAdjustControl::Cancel) => session.cancel_final_adjust(),
                Some(FinalAdjustControl::Back) => session.back_from_final_adjust(),
                None => Some(JumpSessionUpdate::Consumed),
            };
            if matches!(update, Some(JumpSessionUpdate::Cancelled)) {
                self.exit_jump_mode();
            }
            return update;
        }

        let update = session.handle_key(event.key, event.is_down);
        if matches!(update, Some(JumpSessionUpdate::Cancelled)) {
            self.exit_jump_mode();
            return update;
        }
        if final_adjust_config.enabled {
            if let Some(JumpSessionUpdate::Completed { x, y, region }) = update {
                return Some(session.begin_final_adjust(x, y, region));
            }
        }
        update
    }

    pub fn jump_view(&self) -> Option<JumpOverlayView> {
        let JumpState::Active {
            session,
            stage_metadata,
            visuals,
            final_adjust_config,
            ..
        } = &self.jump
        else {
            return None;
        };

        let session_region = session.session_region;
        let target_region = session.current_region;
        let preview_source_region = target_region;
        let client_draw_region = target_region;

        Some(JumpOverlayView {
            stage_index: session.stage_index,
            stage_count: session.stages.len(),
            stages: stage_metadata.clone(),
            session_region,
            target_region,
            preview_source_region,
            client_draw_region,
            grid_size: session.current_grid(),
            input: session.input.clone(),
            visuals: (*visuals).into(),
            final_adjust: session.final_adjust.map(|adjust| FinalAdjustOverlayView {
                original_point: (adjust.original_x, adjust.original_y),
                candidate_point: (adjust.x, adjust.y),
                region: adjust.region,
                small_step_px: final_adjust_config.small_step_px,
                large_step_px: final_adjust_config.large_step_px,
                modifier_key: final_adjust_config.modifier_key.clone(),
                confirm_key: final_adjust_config.confirm_key.clone(),
                cancel_key: final_adjust_config.cancel_key.clone(),
                back_key: final_adjust_config.back_key.clone(),
                show_hint: final_adjust_config.show_hint,
            }),
            grid: None,
        })
    }

    pub fn handle_grid_input(
        &mut self,
        event: KeyEvent,
        action: Option<Action>,
    ) -> Option<GridInputUpdate> {
        if self.is_grid_activation_key_event(&event) {
            return Some(GridInputUpdate::Consumed);
        }

        let GridState::Active { session, .. } = &mut self.grid else {
            return None;
        };

        if !event.is_down {
            return Some(GridInputUpdate::Consumed);
        }

        let move_cursor = session.move_cursor_each_step.unwrap_or(false);
        let update = match event.key {
            VirtualKey::Backspace => session
                .undo()
                .map(|region| GridInputUpdate::Updated {
                    region,
                    move_cursor,
                })
                .or(Some(GridInputUpdate::Consumed)),
            VirtualKey::Escape => Some(GridInputUpdate::Cancelled),
            _ => Self::apply_grid_direction(session, action, move_cursor),
        }?;

        if matches!(update, GridInputUpdate::Updated { .. }) && session.is_resolved() {
            let (x, y) = session.center();
            let region = session.current_region;
            return Some(GridInputUpdate::Completed { x, y, region });
        }

        Some(update)
    }

    fn apply_grid_direction(
        session: &mut GridSession,
        action: Option<Action>,
        move_cursor: bool,
    ) -> Option<GridInputUpdate> {
        match action {
            Some(Action::MoveUp) => session.shrink_up().map(|region| GridInputUpdate::Updated {
                region,
                move_cursor,
            }),
            Some(Action::MoveLeft) => {
                session
                    .shrink_left()
                    .map(|region| GridInputUpdate::Updated {
                        region,
                        move_cursor,
                    })
            }
            Some(Action::MoveDown) => {
                session
                    .shrink_down()
                    .map(|region| GridInputUpdate::Updated {
                        region,
                        move_cursor,
                    })
            }
            Some(Action::MoveRight) => {
                session
                    .shrink_right()
                    .map(|region| GridInputUpdate::Updated {
                        region,
                        move_cursor,
                    })
            }
            _ => Some(GridInputUpdate::Consumed),
        }
    }

    pub fn grid_view(&self) -> Option<JumpOverlayView> {
        let GridState::Active {
            session,
            line_visible,
            show_direction_labels,
            direction_labels,
            ..
        } = &self.grid
        else {
            return None;
        };

        Some(JumpOverlayView {
            stage_index: 0,
            stage_count: 1,
            stages: Vec::new(),
            session_region: session.monitor_bounds,
            target_region: session.current_region,
            preview_source_region: session.current_region,
            client_draw_region: session.current_region,
            grid_size: (2, 2),
            input: String::new(),
            visuals: JumpVisuals {
                selected_region_outline: true,
                preview_outline: false,
                active_grid_outline: true,
                cell_centers: false,
                final_crosshair: false,
            },
            final_adjust: None,
            grid: Some(GridOverlayMetadata {
                line_visible: *line_visible,
                show_direction_labels: *show_direction_labels,
                up_label: direction_labels.up.clone(),
                left_label: direction_labels.left.clone(),
                down_label: direction_labels.down.clone(),
                right_label: direction_labels.right.clone(),
            }),
        })
    }

    pub fn resolve_jump_overlay(&self) -> JumpOverlayResolution {
        self.jump_view()
            .or_else(|| self.grid_view())
            .map(JumpOverlayResolution::Visible)
            .unwrap_or(JumpOverlayResolution::Hidden)
    }

    pub fn exit_jump_mode(&mut self) {
        self.jump = JumpState::Inactive;
    }

    pub fn exit_grid_mode(&mut self) {
        self.grid = GridState::Inactive;
    }

    pub fn cancel_grid_mode(&mut self) {
        self.clear_active_action_keys();
        self.exit_grid_mode();
    }

    pub fn complete_grid_mode(&mut self) {
        self.clear_active_action_keys();
        self.exit_grid_mode();
    }

    pub fn should_swallow_key(&self, event: &KeyEvent) -> bool {
        if self.debug_input {
            eprintln!(
                "[debug-input] raw key={:?} down={} mods: alt={} ralt={} ctrl={} shift={} win={}",
                event.key,
                event.is_down,
                event.alt_down,
                event.right_alt_down,
                event.ctrl_down,
                event.shift_down,
                event.win_down
            );
        }
        if self.is_exclusive_mode_active() {
            self.debug_swallow("exclusive_mode", event, true);
            return true;
        }

        if self.is_system_binding(event) {
            self.debug_swallow("system_binding_match", event, true);
            return true;
        }

        if self.active_mode
            && self.swallow_owned_modifiers
            && self.owned_modifiers.owns_key(event.key)
            && (self.held_physical_keys.contains_key(&event.key)
                || (event.is_down && !self.reconciled_released_keys.contains(&event.key)))
        {
            self.debug_swallow("owned_modifier_active", event, true);
            return true;
        }

        let has_explicit_binding = self.has_explicit_configured_binding_for_event(event);
        if has_explicit_binding {
            self.debug_swallow("explicit_configured_binding", event, true);
            return true;
        }

        if self.is_preserved_shortcut(event) {
            self.debug_swallow("preserved_shortcut", event, false);
            return false;
        }

        let swallow = self.active_mode
            && (!event.is_down
                && self
                    .active_trigger_chords
                    .iter()
                    .any(|chord| chord.key == event.key));
        self.debug_swallow("binding_resolution", event, swallow);
        swallow
    }

    pub fn has_explicit_configured_binding_for_event(&self, event: &KeyEvent) -> bool {
        if !self.active_mode {
            return false;
        }
        let event = self.with_effective_ctrl_state(event);
        self.bound_chords.iter().any(|chord| {
            (event.is_down && chord.matches_dispatch_event(&event))
                || (!event.is_down && self.active_trigger_chords.contains(chord))
        })
    }

    pub fn route_key_event(&mut self, event: KeyEvent, mut action: Option<Action>) {
        if event.is_down && self.is_panic_reset_binding(&event) {
            if self.system_bindings.panic_reset_sets_idle {
                self.enqueue_command(AppCommand::SetActiveMode { active: false });
            }
            self.enqueue_command(AppCommand::PanicReset);
            return;
        }
        if self.is_keybind_lookup_waiting() {
            if event.is_down {
                self.exit_keybind_lookup_mode();
                self.enqueue_command(AppCommand::KeybindLookupInput(event));
            }
            return;
        }
        if self.help_visible && event.is_down {
            if let Some(input) = help_input_from_event(&event, action.as_ref()) {
                match input {
                    crate::help_overlay::HelpInput::Escape => {
                        self.enqueue_command(AppCommand::HideHelp)
                    }
                    other => self.enqueue_command(AppCommand::HelpInput(other)),
                }
                return;
            }
        }
        if self.active_mode && event.is_down && matches!(action.as_ref(), Some(Action::Disable)) {
            self.exit_ui_hint_mode();
            self.exit_bookmark_mode();
            self.enqueue_command(AppCommand::SetActiveMode { active: false });
            return;
        }

        if self.is_toggle_active_key_down_event(&event) {
            self.enqueue_command(AppCommand::ToggleActiveMode);
            return;
        }

        if event.is_down
            && !self.is_jump_active()
            && !self.is_grid_active()
            && (self.is_exit_binding(&event) || matches!(action.as_ref(), Some(Action::Exit)))
        {
            self.enqueue_command(AppCommand::Exit);
            return;
        }
        if (self.is_jump_active() || self.is_grid_active())
            && self.is_toggle_active_key_down_event(&event)
        {
            self.enqueue_command(AppCommand::ToggleActiveMode);
            return;
        }

        if self.is_ui_hint_active() || self.is_ui_hint_querying() {
            if matches!(
                action.as_ref(),
                Some(Action::ReloadConfig | Action::PanicReset)
            ) && event.is_down
            {
                self.exit_ui_hint_mode();
                self.exit_bookmark_mode();
            }
            self.enqueue_command(AppCommand::UiHintInput(event));
            return;
        }

        if self.is_bookmark_mode_active() {
            if !event.is_down {
                if let BookmarkModeState::Active {
                    clear_modifier_held,
                    ..
                } = &mut self.bookmark_mode
                {
                    if event.key == self.bookmark_clear_modifier_key {
                        *clear_modifier_held = false;
                    }
                }
                return;
            }
            if event.key == self.bookmark_cancel_key {
                self.exit_bookmark_mode();
                self.enqueue_command(AppCommand::CancelBookmarkMode);
                return;
            }
            if let BookmarkModeState::Active {
                clear_modifier_held,
                ..
            } = &mut self.bookmark_mode
            {
                if event.key == self.bookmark_clear_modifier_key {
                    *clear_modifier_held = true;
                    return;
                }
                match action {
                    Some(Action::BookmarkSlot(slot)) => {
                        if *clear_modifier_held {
                            self.enqueue_command(AppCommand::ClearBookmarkSlot(slot));
                        } else {
                            self.enqueue_command(AppCommand::SetBookmarkSlot(slot));
                        }
                    }
                    Some(Action::ClearAllBookmarks) => {
                        self.enqueue_command(AppCommand::ClearAllBookmarks)
                    }
                    _ => {}
                }
            }
            return;
        }

        if self.is_jump_active() {
            if self.is_activation_key_event(&event) {
                return;
            }

            self.enqueue_command(AppCommand::JumpInput(event, action));
            return;
        }

        if self.is_grid_active() {
            if self.is_grid_activation_key_event(&event) {
                return;
            }

            self.enqueue_command(AppCommand::GridInput(event, action));
            return;
        }

        if self.is_preserved_shortcut(&event)
            && !self.has_explicit_configured_binding_for_event(&event)
        {
            return;
        }

        if !self.active_mode {
            return;
        }

        if event.is_down {
            self.reconciled_released_keys.remove(&event.key);
            self.active_keys.insert(event.key);
            let now = std::time::Instant::now();
            let mut was_action_trigger = false;
            if let Some(resolved_action) = action.clone() {
                self.track_active_trigger(event, resolved_action);
                was_action_trigger = true;
            }
            self.held_physical_keys
                .entry(event.key)
                .and_modify(|state| {
                    state.last_event_at = now;
                    state.physically_up_since = None;
                    state.was_action_trigger |= was_action_trigger;
                    state.was_owned_modifier |= self.owned_modifiers.owns_key(event.key);
                    state.reconciled_release_sent = false;
                })
                .or_insert(HeldKeyState {
                    pressed_at: now,
                    last_event_at: now,
                    physically_up_since: None,
                    was_action_trigger,
                    was_owned_modifier: self.owned_modifiers.owns_key(event.key),
                    reconciled_release_sent: false,
                });
        } else {
            self.active_keys.remove(&event.key);
            self.held_physical_keys.remove(&event.key);
            action = self.resolve_key_up_action(event, action);
        }

        if event.is_down && matches!(action.as_ref(), Some(Action::KeybindLookupMode)) {
            self.enqueue_command(AppCommand::EnterKeybindLookupMode);
            return;
        }

        if event.is_down && matches!(action.as_ref(), Some(Action::ShowHelp)) {
            self.enqueue_command(AppCommand::ToggleHelp);
            return;
        }

        if event.is_down && matches!(action.as_ref(), Some(Action::ReloadConfig)) {
            self.exit_ui_hint_mode();
            self.exit_bookmark_mode();
            self.enqueue_command(AppCommand::ReloadConfig);
            return;
        }

        if event.is_down
            && matches!(
                action.as_ref(),
                Some(Action::JumpMode | Action::JumpModeProfile(_))
            )
        {
            let profile = match action.as_ref() {
                Some(Action::JumpModeProfile(profile)) => Some(profile.clone()),
                _ => None,
            };
            self.enqueue_command(AppCommand::EnterJumpMode {
                activation_key: event.key,
                profile,
            });
            return;
        }

        if event.is_down && matches!(action.as_ref(), Some(Action::GridMode)) {
            self.enqueue_command(AppCommand::EnterGridMode {
                activation_key: event.key,
            });
            return;
        }

        if event.is_down && matches!(action.as_ref(), Some(Action::UiHintMode)) {
            self.enqueue_command(AppCommand::EnterUiHintMode {
                activation_key: event.key,
            });
            return;
        }

        if event.is_down && matches!(action.as_ref(), Some(Action::BookmarkMode)) {
            self.enqueue_command(AppCommand::EnterBookmarkMode {
                activation_key: event.key,
            });
            return;
        }
        if event.is_down && matches!(action.as_ref(), Some(Action::ShowBookmarks)) {
            self.enqueue_command(AppCommand::ShowBookmarks);
            return;
        }

        if event.is_down && matches!(action.as_ref(), Some(Action::BookmarkSlot(_))) {
            if let Some(Action::BookmarkSlot(slot)) = action {
                self.enqueue_command(AppCommand::RecallBookmarkSlot(slot));
            }
            return;
        }

        for command in self.commands_for_active_keys(event, action) {
            self.enqueue_command(command);
        }
    }

    fn commands_for_active_keys(
        &self,
        event: KeyEvent,
        event_action: Option<Action>,
    ) -> Vec<AppCommand> {
        let mut commands = Vec::new();

        if let Some(action) = event_action {
            if !event.is_down && !action.is_continuous() {
                return commands;
            }
            commands.push(AppCommand::KeyAction {
                action,
                is_down: event.is_down,
            });
        }

        commands
    }

    fn is_activation_key_event(&mut self, event: &KeyEvent) -> bool {
        let JumpState::Active {
            activation_key,
            activation_key_released,
            ..
        } = &mut self.jump
        else {
            return false;
        };

        if event.key != *activation_key {
            return false;
        }

        if event.is_down && !*activation_key_released {
            return true;
        }

        if !event.is_down {
            *activation_key_released = true;
            return true;
        }

        false
    }

    fn is_grid_activation_key_event(&mut self, event: &KeyEvent) -> bool {
        let GridState::Active {
            activation_key,
            activation_key_released,
            ..
        } = &mut self.grid
        else {
            return false;
        };

        if event.key != *activation_key {
            return false;
        }

        if event.is_down && !*activation_key_released {
            return true;
        }

        if !event.is_down {
            *activation_key_released = true;
            return true;
        }

        false
    }

    fn track_active_trigger(&mut self, event: KeyEvent, action: Action) {
        if let Some(chord) = self
            .bound_chords
            .iter()
            .filter(|chord| chord.matches_key_down_event(&event))
            .max_by_key(|chord| chord.specificity())
            .copied()
        {
            self.active_triggers.insert(
                event.key,
                ActiveTrigger {
                    trigger_key: event.key,
                    resolved_chord: chord,
                    resolved_action: action.clone(),
                    is_continuous: action.is_continuous(),
                },
            );
            self.active_trigger_chords.insert(chord);
            if self.debug_input {
                eprintln!("[debug-input] active-trigger add: {:?}", chord);
            }
        }
    }

    fn resolve_key_up_action(
        &mut self,
        event: KeyEvent,
        fallback: Option<Action>,
    ) -> Option<Action> {
        if let Some(active_trigger) = self.active_triggers.remove(&event.key) {
            self.active_trigger_chords
                .remove(&active_trigger.resolved_chord);
            if self.debug_input {
                eprintln!(
                    "[debug-input] active-trigger remove: {:?}",
                    active_trigger.resolved_chord
                );
            }
            if active_trigger.is_continuous {
                return Some(active_trigger.resolved_action);
            }
            return None;
        }
        fallback
    }

    fn debug_swallow(&self, reason: &str, event: &KeyEvent, swallow: bool) {
        if self.debug_input {
            eprintln!(
                "[debug-input] swallow={} reason={} key={:?} down={}",
                swallow, reason, event.key, event.is_down
            );
        }
    }

    fn is_preserved_shortcut(&self, event: &KeyEvent) -> bool {
        if !self.preserve_global_shortcuts {
            return false;
        }
        let event = self.with_effective_ctrl_state(event);

        event.win_down
            || (event.ctrl_down
                && matches!(
                    event.key,
                    VirtualKey::A
                        | VirtualKey::C
                        | VirtualKey::F
                        | VirtualKey::N
                        | VirtualKey::O
                        | VirtualKey::P
                        | VirtualKey::S
                        | VirtualKey::T
                        | VirtualKey::V
                        | VirtualKey::W
                        | VirtualKey::X
                        | VirtualKey::Y
                        | VirtualKey::Z
                ))
            || (event.alt_down && matches!(event.key, VirtualKey::F4 | VirtualKey::Tab))
    }

    fn with_effective_ctrl_state(&self, event: &KeyEvent) -> KeyEvent {
        let mut effective = *event;
        let synthetic_ctrl = event.ctrl_down && !event.left_ctrl_down && !event.right_ctrl_down;
        if self.right_alt_suppresses_synthetic_ctrl && event.right_alt_down && synthetic_ctrl {
            effective.ctrl_down = false;
        }
        effective
    }

    pub fn set_right_alt_suppresses_synthetic_ctrl(&mut self, enabled: bool) {
        self.right_alt_suppresses_synthetic_ctrl = enabled;
    }

    pub fn resolve_keybind_lookup(
        &self,
        event: &KeyEvent,
        bindings: &KeyBindings,
        shift_can_modify_plain_movement: bool,
    ) -> KeybindLookupResult {
        let normalized = self.with_effective_ctrl_state(event);
        let chord_display = lookup_chord_display(&normalized);
        let system_binding = self.lookup_system_binding(&normalized);
        let candidates = bindings
            .resolve_candidates_for_key_down_event(&normalized, shift_can_modify_plain_movement);
        let winning_action_binding = candidates.first().cloned();
        let competing_bindings: Vec<KeybindLookupBinding> = candidates
            .iter()
            .skip(1)
            .map(|candidate| KeybindLookupBinding {
                chord: candidate.chord,
                action: candidate.action.clone(),
            })
            .collect();

        let preserved_shortcut = self.preserve_global_shortcuts
            && preserved_shortcut_risk_for_chord(&event_chord_for_lookup(&normalized)).is_some();

        let (matched_action, matched_binding, swallowed, explanation) = if let Some((name, chord)) =
            system_binding
        {
            let explanation = format!(
                "{chord_display} matches system binding {name} ({}), so system routing wins before action dispatch and the key is swallowed.",
                chord.display_label()
            );
            (None, Some(chord), true, explanation)
        } else if let Some(BindingCandidate { chord, action }) = winning_action_binding {
            let specificity_note = if competing_bindings.is_empty() {
                String::new()
            } else {
                format!(
                    " It wins over {} competing binding(s) by KeyChord specificity/order.",
                    competing_bindings.len()
                )
            };
            if self.active_mode {
                let explanation = format!(
                    "{chord_display} resolves to action {action:?} via binding {} while active mode is enabled, so the event is swallowed and dispatched.{specificity_note}",
                    chord.display_label()
                );
                (Some(action.clone()), Some(chord), true, explanation)
            } else {
                let explanation = format!(
                    "{chord_display} has action binding {} -> {action:?}, but active mode is disabled, so action bindings are inactive and the key passes through.",
                    chord.display_label()
                );
                (Some(action.clone()), Some(chord), false, explanation)
            }
        } else if self.active_mode && preserved_shortcut {
            let explanation = format!(
                "{chord_display} is a preserved Windows/app shortcut and no explicit action binding matched, so it passes through."
            );
            (None, None, false, explanation)
        } else if self.active_mode {
            let explanation = format!(
                "{chord_display} is unbound in active mode and is not a system binding, so it passes through."
            );
            (None, None, false, explanation)
        } else {
            let explanation = format!(
                "{chord_display} was looked up while active mode is disabled; no system binding matched, so it passes through."
            );
            (None, None, false, explanation)
        };

        KeybindLookupResult {
            chord_display,
            matched_action,
            matched_binding,
            swallowed,
            preserved_shortcut,
            competing_bindings,
            explanation,
        }
    }

    fn lookup_system_binding(&self, event: &KeyEvent) -> Option<(String, KeyChord)> {
        if self.system_bindings.toggle_active.matches_event(event) {
            return Some((
                "toggle_active".to_string(),
                self.system_bindings.toggle_active,
            ));
        }
        let exit_matches = if self.system_bindings.exit_ignore_extra_modifiers {
            self.system_bindings
                .exit
                .matches_event_ignoring_extra_modifiers(event)
        } else {
            self.system_bindings.exit.matches_event(event)
        };
        if exit_matches {
            return Some(("exit".to_string(), self.system_bindings.exit));
        }
        let panic_matches = if self.system_bindings.panic_reset_ignore_extra_modifiers {
            self.system_bindings
                .panic_reset
                .matches_event_ignoring_extra_modifiers(event)
        } else {
            self.system_bindings.panic_reset.matches_event(event)
        };
        panic_matches.then_some(("panic_reset".to_string(), self.system_bindings.panic_reset))
    }
}

fn lookup_chord_display(event: &KeyEvent) -> String {
    let mut parts = Vec::new();
    if event.left_ctrl_down {
        parts.push("LeftCtrl".to_string());
    } else if event.right_ctrl_down {
        parts.push("RightCtrl".to_string());
    } else if event.ctrl_down {
        parts.push("Ctrl".to_string());
    }
    if event.left_alt_down {
        parts.push("LeftAlt".to_string());
    } else if event.right_alt_down {
        parts.push("RightAlt".to_string());
    } else if event.alt_down {
        parts.push("Alt".to_string());
    }
    if event.left_shift_down {
        parts.push("LeftShift".to_string());
    } else if event.right_shift_down {
        parts.push("RightShift".to_string());
    } else if event.shift_down {
        parts.push("Shift".to_string());
    }
    if event.left_win_down {
        parts.push("LeftWin".to_string());
    } else if event.right_win_down {
        parts.push("RightWin".to_string());
    } else if event.win_down {
        parts.push("Win".to_string());
    }
    parts.push(KeyChord::from_key(event.key).display_label());
    parts.join("+")
}

fn event_chord_for_lookup(event: &KeyEvent) -> KeyChord {
    use crate::key_chord::{ModifierRequirements, ModifierSideRequirement};
    fn req(any: bool, left: bool, right: bool) -> ModifierSideRequirement {
        if right {
            ModifierSideRequirement::Right
        } else if left {
            ModifierSideRequirement::Left
        } else if any {
            ModifierSideRequirement::Any
        } else {
            ModifierSideRequirement::NotRequired
        }
    }
    KeyChord {
        key: event.key,
        modifiers: ModifierRequirements::new(
            req(event.ctrl_down, event.left_ctrl_down, event.right_ctrl_down),
            req(event.alt_down, event.left_alt_down, event.right_alt_down),
            req(
                event.shift_down,
                event.left_shift_down,
                event.right_shift_down,
            ),
            req(event.win_down, event.left_win_down, event.right_win_down),
        ),
    }
}

fn jump_stage_metadata(config: &JumpConfig) -> Vec<JumpStageMetadata> {
    let selection_keys: Vec<char> = config.hints.selection_keys.chars().collect();
    let mut stages = vec![JumpStageMetadata {
        index: 0,
        grid_size: (config.coarse.width, config.coarse.height),
        aim_point: config.coarse.aim_point,
        aim_offset_x_px: config.coarse.aim_offset_x_px,
        aim_offset_y_px: config.coarse.aim_offset_y_px,
        target_margin_percent: config.coarse.target_margin_percent,
        visual_context_margin_percent: config.coarse.visual_context_margin_percent,
        zoom_scale: config.coarse.zoom_scale,
        target_region_mode: config.coarse.target_region_mode,
        preview_edge_behavior: config
            .coarse
            .preview_edge_behavior
            .unwrap_or(config.preview_edge_behavior),
        labels: config.coarse.labels.into(),
        cell_labels: generate_labels((config.coarse.width, config.coarse.height), &selection_keys)
            .unwrap_or_default(),
    }];

    if config.fine.enabled {
        stages.push(JumpStageMetadata {
            index: stages.len(),
            grid_size: (config.fine.width, config.fine.height),
            aim_point: config.fine.aim_point,
            aim_offset_x_px: config.fine.aim_offset_x_px,
            aim_offset_y_px: config.fine.aim_offset_y_px,
            target_margin_percent: config.fine.target_margin_percent,
            visual_context_margin_percent: config.fine.visual_context_margin_percent,
            zoom_scale: config.fine.zoom_scale,
            target_region_mode: config.fine.target_region_mode,
            preview_edge_behavior: config
                .fine
                .preview_edge_behavior
                .unwrap_or(config.preview_edge_behavior),
            labels: config.fine.labels.into(),
            cell_labels: generate_labels((config.fine.width, config.fine.height), &selection_keys)
                .unwrap_or_default(),
        });
    }

    if config.precise.enabled {
        stages.push(JumpStageMetadata {
            index: stages.len(),
            grid_size: (config.precise.width, config.precise.height),
            aim_point: config.precise.aim_point,
            aim_offset_x_px: config.precise.aim_offset_x_px,
            aim_offset_y_px: config.precise.aim_offset_y_px,
            target_margin_percent: config.precise.target_margin_percent,
            visual_context_margin_percent: config.precise.visual_context_margin_percent,
            zoom_scale: config.precise.zoom_scale,
            target_region_mode: config.precise.target_region_mode,
            preview_edge_behavior: config
                .precise
                .preview_edge_behavior
                .unwrap_or(config.preview_edge_behavior),
            labels: config.precise.labels.into(),
            cell_labels: generate_labels(
                (config.precise.width, config.precise.height),
                &selection_keys,
            )
            .unwrap_or_default(),
        });
    }

    stages
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicator::{
        resolve_indicator_snapshot, IndicatorInput, IndicatorState, MouseIndicatorInput,
        WheelIndicatorInput,
    };
    use crate::key_chord::KeyChord;

    fn jump_region() -> JumpRegion {
        JumpRegion {
            left: 0,
            top: 0,
            width: 100,
            height: 100,
        }
    }

    fn enter_jump_mode(state: &mut AppState, activation_key: VirtualKey) {
        let config = Config::default().normalize().unwrap();
        assert!(state.enter_jump_mode(
            &config.jump,
            config.final_adjust.clone(),
            jump_region(),
            activation_key
        ));
    }

    fn enter_grid_mode(state: &mut AppState, activation_key: VirtualKey) {
        assert!(state.enter_grid_mode(
            jump_region(),
            jump_region(),
            25,
            25,
            true,
            true,
            true,
            activation_key,
        ));
    }

    fn enter_ui_hint_mode(state: &mut AppState, activation_key: VirtualKey) {
        state.ui_hints = UiHintState::Active {
            activation_key,
            query_id: 1,
            foreground_hwnd: 0,
        };
    }

    fn final_adjust_config(enabled: bool) -> Config {
        let mut config = Config::default();
        config.jump.mode = crate::JumpMode::Single;
        config.final_adjust.enabled = enabled;
        config.final_adjust.small_step_px = 2;
        config.final_adjust.large_step_px = 9;
        config.normalize().unwrap()
    }

    fn state_with_bound_key(key: VirtualKey) -> AppState {
        let mut state = AppState::default();
        state.set_bound_keys([key]);
        state
    }

    fn state_with_bound_chords(chords: impl IntoIterator<Item = KeyChord>) -> AppState {
        let mut state = AppState::default();
        state.set_bound_chords(chords);
        state
    }

    fn collect_commands(state: &mut AppState) -> Vec<AppCommand> {
        let mut commands = Vec::new();
        while let Some(command) = state.pop_command() {
            commands.push(command);
        }
        commands
    }

    fn ctrl_e_down() -> KeyEvent {
        let mut event = KeyEvent::new(VirtualKey::E, true);
        event.ctrl_down = true;
        event
    }

    fn ctrl_w_down() -> KeyEvent {
        let mut event = KeyEvent::new(VirtualKey::W, true);
        event.ctrl_down = true;
        event
    }

    fn right_alt_e_with_synthetic_ctrl() -> KeyEvent {
        let mut event = KeyEvent::new(VirtualKey::E, true);
        event.alt_down = true;
        event.right_alt_down = true;
        event.ctrl_down = true;
        event
    }

    fn lookup_test_action(event: KeyEvent) -> Option<Action> {
        match event.key {
            VirtualKey::Left => Some(Action::MoveLeft),
            _ => None,
        }
    }

    #[test]
    fn idle_unrelated_key_passthrough() {
        let mut state = state_with_bound_key(VirtualKey::A);
        state.set_active_mode(false);

        assert!(!state.should_swallow_key(&KeyEvent::new(VirtualKey::B, true)));
    }

    #[test]
    fn jump_input_swallowed() {
        let mut state = AppState::default();
        enter_jump_mode(&mut state, VirtualKey::J);

        assert!(state.should_swallow_key(&KeyEvent::new(VirtualKey::A, true)));
    }

    #[test]
    fn grid_input_swallowed() {
        let mut state = AppState::default();
        enter_grid_mode(&mut state, VirtualKey::G);

        assert!(state.should_swallow_key(&KeyEvent::new(VirtualKey::W, true)));
    }

    #[test]
    fn ui_hint_mode_swallows_normal_letters() {
        let mut state = AppState::default();
        enter_ui_hint_mode(&mut state, VirtualKey::U);

        assert!(state.should_swallow_key(&KeyEvent::new(VirtualKey::A, true)));
    }

    #[test]
    fn global_shortcuts_swallowed_while_jump_mode_is_active() {
        let mut state = AppState::default();
        enter_jump_mode(&mut state, VirtualKey::J);
        let mut ctrl_w = KeyEvent::new(VirtualKey::W, true);
        ctrl_w.ctrl_down = true;

        assert!(state.should_swallow_key(&ctrl_w));
    }

    #[test]
    fn global_shortcuts_not_swallowed_when_jump_mode_is_inactive() {
        let state = AppState::default();
        let mut ctrl_w = KeyEvent::new(VirtualKey::W, true);
        ctrl_w.ctrl_down = true;

        assert!(!state.should_swallow_key(&ctrl_w));
    }

    #[test]
    fn configured_system_binding_wins_over_preserved_shortcut() {
        let mut state = AppState::default();
        state.set_system_bindings(RuntimeSystemBindings::new(
            KeyChord::parse("Ctrl+W").unwrap(),
            KeyChord::parse("Escape").unwrap(),
        ));
        let mut ctrl_w = KeyEvent::new(VirtualKey::W, true);
        ctrl_w.ctrl_down = true;

        assert!(state.should_swallow_key(&ctrl_w));
    }

    #[test]
    fn shifted_plain_key_is_swallowed_like_action_lookup() {
        let state = state_with_bound_key(VirtualKey::W);
        let mut event = KeyEvent::new(VirtualKey::W, true);
        event.shift_down = true;

        assert!(state.should_swallow_key(&event));
    }

    #[test]
    fn right_alt_chord_is_swallowed_but_ctrl_plain_key_is_not() {
        let state = state_with_bound_chords([
            KeyChord::parse("W").unwrap(),
            KeyChord::parse("RightAlt+W").unwrap(),
        ]);
        let mut right_alt_w = KeyEvent::new(VirtualKey::W, true);
        right_alt_w.alt_down = true;
        right_alt_w.right_alt_down = true;
        let mut ctrl_w = KeyEvent::new(VirtualKey::W, true);
        ctrl_w.ctrl_down = true;

        assert!(state.should_swallow_key(&right_alt_w));
        assert!(!state.should_swallow_key(&ctrl_w));
    }

    #[test]
    fn shift_self_keys_are_swallowed_like_action_lookup() {
        let state = state_with_bound_chords([
            KeyChord::parse("LeftShift").unwrap(),
            KeyChord::parse("RightShift").unwrap(),
        ]);

        for key in [VirtualKey::LeftShift, VirtualKey::RightShift] {
            let mut event = KeyEvent::new(key, true);
            event.shift_down = true;

            assert!(state.should_swallow_key(&event), "{key:?}");
        }
    }

    #[test]
    fn owned_modifier_swallowed_while_active() {
        let mut state = AppState::default();
        state.set_owned_modifiers([VirtualKey::Alt]);
        state.set_active_mode(true);
        let event = KeyEvent::new(VirtualKey::Alt, true);
        assert!(state.should_swallow_key(&event));
    }

    #[test]
    fn owned_modifier_not_swallowed_while_idle() {
        let mut state = AppState::default();
        state.set_owned_modifiers([VirtualKey::Alt]);
        state.set_active_mode(false);
        let event = KeyEvent::new(VirtualKey::Alt, true);
        assert!(!state.should_swallow_key(&event));
    }

    #[test]
    fn exit_binding_matches_with_owned_modifier_held() {
        let mut state = AppState::default();
        state.set_owned_modifiers([VirtualKey::Alt]);
        state.set_system_bindings(RuntimeSystemBindings::new(
            KeyChord::parse("Ctrl+E").unwrap(),
            KeyChord::parse("Escape").unwrap(),
        ));
        let mut event = KeyEvent::new(VirtualKey::Escape, true);
        event.alt_down = true;
        assert!(state.is_system_binding(&event));
        assert!(state.should_swallow_key(&event));
    }

    #[test]
    fn jump_binding_routes_to_enter_command() {
        let mut state = state_with_bound_key(VirtualKey::J);
        let event = KeyEvent::new(VirtualKey::J, true);

        state.route_key_event(event, Some(Action::JumpMode));

        assert_eq!(
            state.pop_command(),
            Some(AppCommand::EnterJumpMode {
                activation_key: VirtualKey::J,
                profile: None
            })
        );
    }

    #[test]
    fn grid_binding_routes_to_enter_command() {
        let mut state = state_with_bound_key(VirtualKey::G);
        let event = KeyEvent::new(VirtualKey::G, true);

        state.route_key_event(event, Some(Action::GridMode));

        assert_eq!(
            state.pop_command(),
            Some(AppCommand::EnterGridMode {
                activation_key: VirtualKey::G,
            })
        );
    }

    #[test]
    fn ui_hint_binding_queues_enter_ui_hint_mode() {
        let mut state = state_with_bound_key(VirtualKey::U);
        let event = KeyEvent::new(VirtualKey::U, true);

        state.route_key_event(event, Some(Action::UiHintMode));

        assert_eq!(
            state.pop_command(),
            Some(AppCommand::EnterUiHintMode {
                activation_key: VirtualKey::U,
            })
        );
    }

    #[test]
    fn profile_jump_binding_routes_profile_name() {
        let mut state = state_with_bound_key(VirtualKey::J);

        state.route_key_event(
            KeyEvent::new(VirtualKey::J, true),
            Some(Action::JumpModeProfile("wide".to_string())),
        );

        assert_eq!(
            state.pop_command(),
            Some(AppCommand::EnterJumpMode {
                activation_key: VirtualKey::J,
                profile: Some("wide".to_string())
            })
        );
    }

    #[test]
    fn bound_movement_key_down_in_active_mode_emits_key_action_down() {
        let mut state = state_with_bound_key(VirtualKey::Left);

        state.route_key_event(
            KeyEvent::new(VirtualKey::Left, true),
            Some(Action::MoveLeft),
        );

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::KeyAction {
                action: Action::MoveLeft,
                is_down: true
            }]
        );
    }

    #[test]
    fn bound_movement_key_up_in_active_mode_emits_key_action_up() {
        let mut state = state_with_bound_key(VirtualKey::Left);

        state.route_key_event(
            KeyEvent::new(VirtualKey::Left, false),
            Some(Action::MoveLeft),
        );

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::KeyAction {
                action: Action::MoveLeft,
                is_down: false
            }]
        );
    }

    #[test]
    fn escape_key_down_emits_exactly_one_exit_even_when_configured() {
        let mut state = state_with_bound_key(VirtualKey::Escape);
        let event = KeyEvent::new(VirtualKey::Escape, true);

        state.route_key_event(event, Some(Action::Exit));

        assert_eq!(collect_commands(&mut state), vec![AppCommand::Exit]);
    }

    #[test]
    fn escape_key_down_exit_is_consistent_across_active_modes() {
        for active_mode in [true, false] {
            let mut state = state_with_bound_key(VirtualKey::Escape);
            state.set_active_mode(active_mode);

            state.route_key_event(KeyEvent::new(VirtualKey::Escape, true), Some(Action::Exit));

            assert_eq!(
                collect_commands(&mut state),
                vec![AppCommand::Exit],
                "active_mode={active_mode}"
            );
        }
    }

    #[test]
    fn system_binding_key_up_is_swallowed_without_command() {
        let mut state = AppState::default();
        let mut event = KeyEvent::new(VirtualKey::E, false);
        event.ctrl_down = true;

        assert!(state.should_swallow_key(&event));

        state.route_key_event(event, None);

        assert_eq!(collect_commands(&mut state), Vec::new());
    }

    #[test]
    fn toggle_binding_key_down_queues_toggle_active_mode() {
        let mut state = state_with_bound_key(VirtualKey::E);

        state.route_key_event(ctrl_e_down(), Some(Action::MoveLeft));

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::ToggleActiveMode]
        );
    }

    #[test]
    fn toggle_binding_key_up_does_not_queue_toggle_command() {
        let mut state = AppState::default();
        let mut event = KeyEvent::new(VirtualKey::E, false);
        event.ctrl_down = true;

        state.route_key_event(event, None);

        assert_eq!(collect_commands(&mut state), Vec::new());
    }

    #[test]
    fn toggle_binding_still_queues_while_active_mode_is_false() {
        let mut state = state_with_bound_key(VirtualKey::E);
        state.set_active_mode(false);

        state.route_key_event(ctrl_e_down(), Some(Action::MoveLeft));

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::ToggleActiveMode]
        );
    }

    #[test]
    fn movement_key_down_while_disabled_does_not_queue_movement_command() {
        let mut state = state_with_bound_key(VirtualKey::Left);
        state.set_active_mode(false);

        state.route_key_event(
            KeyEvent::new(VirtualKey::Left, true),
            Some(Action::MoveLeft),
        );

        assert_eq!(collect_commands(&mut state), Vec::new());
    }

    #[test]
    fn disabled_mode_suppresses_movement_and_resolves_hidden_indicator() {
        let mut state = state_with_bound_key(VirtualKey::W);
        state.set_active_mode(false);

        state.route_key_event(KeyEvent::new(VirtualKey::W, true), Some(Action::MoveUp));

        let active_actions = HashSet::from([Action::MoveUp]);
        let indicator = resolve_indicator_snapshot(IndicatorInput {
            app_active: state.active_mode(),
            jump_active: state.is_jump_active(),
            jump_stage: state.jump_stage_status(),
            final_adjust_active: state.final_adjust_active(),
            active_actions: &active_actions,
            mouse: MouseIndicatorInput {
                active: true,
                current_speed: 5,
                default_speed: 3,
            },
            wheel: WheelIndicatorInput {
                active: true,
                current_speed: 12,
                default_speed: 3,
            },
            left_button_held: true,
            bookmark_mode_active: false,
        })
        .state;

        assert_eq!(collect_commands(&mut state), Vec::new());
        assert_eq!(indicator, IndicatorState::Hidden);
    }

    #[test]
    fn jump_mode_routes_escape_to_jump_input_before_exit_binding() {
        let mut state = AppState::default();
        enter_jump_mode(&mut state, VirtualKey::J);
        let event = KeyEvent::new(VirtualKey::Escape, true);

        state.route_key_event(event, None);

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::JumpInput(event, None)]
        );
    }

    #[test]
    fn ui_hint_mode_routes_letters_to_ui_hint_input() {
        let mut state = AppState::default();
        enter_ui_hint_mode(&mut state, VirtualKey::U);
        let event = KeyEvent::new(VirtualKey::A, true);

        state.route_key_event(event, None);

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::UiHintInput(event)]
        );
    }

    #[test]
    fn toggle_active_in_ui_hint_mode_disables_app() {
        let mut state = AppState::default();
        enter_ui_hint_mode(&mut state, VirtualKey::U);

        state.route_key_event(ctrl_e_down(), None);

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::ToggleActiveMode]
        );
    }

    #[test]
    fn disabled_app_does_not_enter_ui_hint_mode() {
        let mut state = state_with_bound_key(VirtualKey::U);
        state.set_active_mode(false);

        state.route_key_event(KeyEvent::new(VirtualKey::U, true), Some(Action::UiHintMode));

        assert_eq!(collect_commands(&mut state), Vec::new());
    }

    #[test]
    fn reload_or_panic_exits_ui_hint_mode() {
        let mut reload_state = state_with_bound_key(VirtualKey::R);
        enter_ui_hint_mode(&mut reload_state, VirtualKey::U);
        reload_state.route_key_event(
            KeyEvent::new(VirtualKey::R, true),
            Some(Action::ReloadConfig),
        );
        assert!(!reload_state.is_ui_hint_active());

        let mut panic_state = state_with_bound_key(VirtualKey::P);
        enter_ui_hint_mode(&mut panic_state, VirtualKey::U);
        panic_state.route_key_event(KeyEvent::new(VirtualKey::P, true), Some(Action::PanicReset));
        assert!(!panic_state.is_ui_hint_active());
    }

    #[test]
    fn ui_hint_querying_to_active_and_stale_rejection() {
        let mut state = AppState::default();
        state.enter_ui_hint_querying(VirtualKey::U, 7, 123);
        assert_eq!(state.current_ui_hint_query_id(), Some(7));
        assert_eq!(state.current_ui_hint_foreground_hwnd(), Some(123));
        assert!(!state.activate_ui_hint_if_querying(8));
        assert!(state.is_ui_hint_querying());
        assert!(state.activate_ui_hint_if_querying(7));
        assert!(state.is_ui_hint_active());
        assert_eq!(state.current_ui_hint_foreground_hwnd(), Some(123));
    }

    #[test]
    fn ui_hint_querying_to_inactive() {
        let mut state = AppState::default();
        state.enter_ui_hint_querying(VirtualKey::U, 2, 0);
        assert!(state.is_ui_hint_active_or_querying());
        state.exit_ui_hint_mode();
        assert!(!state.is_ui_hint_active_or_querying());
        assert_eq!(state.current_ui_hint_query_id(), None);
    }

    #[test]
    fn jump_mode_routes_toggle_active_to_disable_transition() {
        let mut state = AppState::default();
        enter_jump_mode(&mut state, VirtualKey::J);

        state.route_key_event(ctrl_e_down(), None);

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::ToggleActiveMode]
        );
    }

    #[test]
    fn jump_input_escape_cancels_active_jump_session() {
        let mut state = AppState::default();
        enter_jump_mode(&mut state, VirtualKey::J);

        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::Escape, true), None),
            Some(JumpSessionUpdate::Cancelled)
        );
        assert!(!state.is_jump_active());
        assert_eq!(state.resolve_jump_overlay(), JumpOverlayResolution::Hidden);
    }

    #[test]
    fn default_jump_config_starts_with_two_enabled_stages() {
        let mut state = AppState::default();
        enter_jump_mode(&mut state, VirtualKey::F);

        let view = state.jump_view().unwrap();
        assert_eq!(view.stage_count, 2);
        assert_eq!(view.stages.len(), 2);
        assert_eq!(view.stage_index, 0);
    }

    #[test]
    fn valid_default_label_advances_then_completes() {
        let config = Config::default().normalize().unwrap();
        let mut state = AppState::default();
        assert!(state.enter_jump_mode(
            &config.jump,
            config.final_adjust.clone(),
            jump_region(),
            VirtualKey::F
        ));

        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::A, true), None),
            Some(JumpSessionUpdate::Consumed)
        );
        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::J, true), None),
            Some(JumpSessionUpdate::StageAdvanced {
                stage_index: 1,
                region: JumpRegion {
                    left: 90,
                    top: 0,
                    width: 10,
                    height: 10,
                },
            })
        );
        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::A, true), None),
            Some(JumpSessionUpdate::Completed {
                x: 91,
                y: 1,
                region: JumpRegion {
                    left: 90,
                    top: 0,
                    width: 2,
                    height: 2,
                },
            })
        );
    }

    #[test]
    fn default_stage_two_view_region_is_selected_stage_one_cell() {
        let mut state = AppState::default();
        enter_jump_mode(&mut state, VirtualKey::F);

        state.handle_jump_input(KeyEvent::new(VirtualKey::A, true), None);
        state.handle_jump_input(KeyEvent::new(VirtualKey::J, true), None);

        let selected_cell = JumpRegion {
            left: 90,
            top: 0,
            width: 10,
            height: 10,
        };
        let view = state.jump_view().unwrap();
        assert_eq!(view.stage_index, 1);
        assert_eq!(view.target_region, selected_cell);
        assert_eq!(view.preview_source_region, selected_cell);
        assert_eq!(view.client_draw_region, selected_cell);
    }

    #[test]
    fn completed_jump_finishes_immediately_when_final_adjust_is_disabled() {
        let config = final_adjust_config(false);
        let mut state = AppState::default();
        assert!(state.enter_jump_mode(
            &config.jump,
            config.final_adjust.clone(),
            jump_region(),
            VirtualKey::F
        ));

        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::A, true), None),
            Some(JumpSessionUpdate::Consumed)
        );
        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::J, true), None),
            Some(JumpSessionUpdate::Completed {
                x: 95,
                y: 5,
                region: JumpRegion {
                    left: 90,
                    top: 0,
                    width: 10,
                    height: 10,
                },
            })
        );
        assert!(state.jump_view().unwrap().final_adjust.is_none());
    }

    #[test]
    fn completed_jump_enters_final_adjust_when_enabled() {
        let config = final_adjust_config(true);
        let mut state = AppState::default();
        assert!(state.enter_jump_mode(
            &config.jump,
            config.final_adjust.clone(),
            jump_region(),
            VirtualKey::F
        ));

        state.handle_jump_input(KeyEvent::new(VirtualKey::A, true), None);

        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::J, true), None),
            Some(JumpSessionUpdate::AwaitingFinalAdjust {
                x: 95,
                y: 5,
                region: JumpRegion {
                    left: 90,
                    top: 0,
                    width: 10,
                    height: 10,
                },
            })
        );

        let view = state.jump_view().unwrap();
        let adjust = view.final_adjust.unwrap();
        assert_eq!(adjust.original_point, (95, 5));
        assert_eq!(adjust.candidate_point, (95, 5));
    }

    #[test]
    fn final_adjust_nudge_and_confirm_complete_at_candidate_point() {
        let config = final_adjust_config(true);
        let mut state = AppState::default();
        assert!(state.enter_jump_mode(
            &config.jump,
            config.final_adjust.clone(),
            jump_region(),
            VirtualKey::F
        ));
        state.handle_jump_input(KeyEvent::new(VirtualKey::A, true), None);
        state.handle_jump_input(KeyEvent::new(VirtualKey::J, true), None);

        assert_eq!(
            state.handle_jump_input(
                KeyEvent::new(VirtualKey::Right, true),
                Some(Action::MoveRight)
            ),
            Some(JumpSessionUpdate::AwaitingFinalAdjust {
                x: 97,
                y: 5,
                region: JumpRegion {
                    left: 90,
                    top: 0,
                    width: 10,
                    height: 10,
                },
            })
        );

        let mut large_down = KeyEvent::new(VirtualKey::Down, true);
        large_down.shift_down = true;
        assert_eq!(
            state.handle_jump_input(large_down, Some(Action::MoveDown)),
            Some(JumpSessionUpdate::AwaitingFinalAdjust {
                x: 97,
                y: 14,
                region: JumpRegion {
                    left: 90,
                    top: 0,
                    width: 10,
                    height: 10,
                },
            })
        );

        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::Enter, true), None),
            Some(JumpSessionUpdate::Completed {
                x: 97,
                y: 14,
                region: JumpRegion {
                    left: 90,
                    top: 0,
                    width: 10,
                    height: 10,
                },
            })
        );
    }

    #[test]
    fn jump_labels_use_configured_keyset_not_hardcoded_default() {
        let mut config = Config::default();
        config.jump.hints.selection_keys = "XY".to_string();
        config.jump.coarse.width = 2;
        config.jump.coarse.height = 2;
        config.jump.fine.enabled = false;
        config.jump.precise.enabled = false;
        let config = config.normalize().unwrap();
        let mut state = AppState::default();

        assert!(state.enter_jump_mode(
            &config.jump,
            config.final_adjust.clone(),
            jump_region(),
            VirtualKey::F
        ));

        let view = state.jump_view().unwrap();
        assert_eq!(view.stages[0].cell_labels, vec!["XX", "XY", "YX", "YY"]);

        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::X, true), None),
            Some(JumpSessionUpdate::Consumed)
        );
        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::Y, true), None),
            Some(JumpSessionUpdate::Completed {
                x: 75,
                y: 25,
                region: JumpRegion {
                    left: 50,
                    top: 0,
                    width: 50,
                    height: 50,
                },
            })
        );
    }

    #[test]
    fn jump_activation_key_is_gated_until_release() {
        let mut state = AppState::default();
        enter_jump_mode(&mut state, VirtualKey::J);

        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::J, true), None),
            Some(JumpSessionUpdate::Consumed)
        );
        assert_eq!(state.jump_view().unwrap().input, "");
        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::J, false), None),
            Some(JumpSessionUpdate::Consumed)
        );
        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::J, true), None),
            Some(JumpSessionUpdate::Consumed)
        );
        assert_eq!(state.jump_view().unwrap().input, "J");
    }

    #[test]
    fn jump_view_exposes_stage_region_input_and_preview_settings() {
        let mut state = AppState::default();
        enter_jump_mode(&mut state, VirtualKey::J);

        let view = state.jump_view().unwrap();

        assert_eq!(view.stage_index, 0);
        assert_eq!(view.stage_count, 2);
        assert_eq!(view.session_region, jump_region());
        assert_eq!(view.target_region, jump_region());
        assert_eq!(view.preview_source_region, jump_region());
        assert_eq!(
            view.client_draw_region,
            JumpRegion {
                left: 0,
                top: 0,
                width: 100,
                height: 100,
            }
        );
        assert_eq!(view.grid_size, (10, 10));
        assert_eq!(view.input, "");
        assert_eq!(view.stages.len(), 2);
        assert_eq!(
            view.stages[0].target_region_mode,
            crate::JumpTargetRegionMode::ExactRegion
        );
        assert_eq!(view.stages[0].target_margin_percent, 0);
        assert_eq!(view.stages[0].visual_context_margin_percent, 0);
        assert_eq!(view.stages[0].zoom_scale, 1.0);
        assert_eq!(view.stages[1].visual_context_margin_percent, 0);
        assert_eq!(view.stages[1].zoom_scale, 1.0);
    }

    #[test]
    fn jump_view_propagates_configured_zoom_scale_to_metadata() {
        let mut config = Config::default();
        config.jump.mode = crate::JumpMode::Precision;
        config.jump.coarse.zoom_scale = 1.25;
        config.jump.fine.enabled = true;
        config.jump.fine.zoom_scale = 4.0;
        let config = config.normalize().unwrap();

        let mut state = AppState::default();
        assert!(state.enter_jump_mode(
            &config.jump,
            config.final_adjust.clone(),
            jump_region(),
            VirtualKey::J
        ));

        let view = state.jump_view().unwrap();
        assert_eq!(view.stages.len(), 2);
        assert_eq!(view.stages[0].zoom_scale, 1.25);
        assert_eq!(view.stages[1].zoom_scale, 4.0);
    }

    #[test]
    fn jump_view_propagates_configured_target_region_modes_to_metadata() {
        let mut config = Config::default();
        config.jump.mode = crate::JumpMode::Precision;
        config.jump.coarse.target_region_mode = crate::JumpTargetRegionMode::RegionWithContext;
        config.jump.fine.enabled = true;
        config.jump.fine.target_region_mode = crate::JumpTargetRegionMode::ExpandedTarget;
        let config = config.normalize().unwrap();

        let mut state = AppState::default();
        assert!(state.enter_jump_mode(
            &config.jump,
            config.final_adjust.clone(),
            jump_region(),
            VirtualKey::J
        ));

        let view = state.jump_view().unwrap();
        assert_eq!(view.stages.len(), 2);
        assert_eq!(
            view.stages[0].target_region_mode,
            crate::JumpTargetRegionMode::RegionWithContext
        );
        assert_eq!(
            view.stages[1].target_region_mode,
            crate::JumpTargetRegionMode::ExpandedTarget
        );
    }

    #[test]
    fn jump_view_propagates_configured_visual_toggles() {
        let mut config = Config::default();
        config.jump.visuals.selected_region_outline = false;
        config.jump.visuals.preview_outline = true;
        config.jump.visuals.active_grid_outline = false;
        config.jump.visuals.cell_centers = true;
        config.jump.visuals.final_crosshair = false;
        let config = config.normalize().unwrap();

        let mut state = AppState::default();
        assert!(state.enter_jump_mode(
            &config.jump,
            config.final_adjust.clone(),
            jump_region(),
            VirtualKey::J
        ));

        let view = state.jump_view().unwrap();
        assert!(!view.visuals.selected_region_outline);
        assert!(view.visuals.preview_outline);
        assert!(!view.visuals.active_grid_outline);
        assert!(view.visuals.cell_centers);
        assert!(!view.visuals.final_crosshair);
    }

    #[test]
    fn jump_view_keeps_stage_two_preview_region_exact_despite_context_config() {
        let mut config = Config::default();
        config.jump.mode = crate::JumpMode::Precision;
        config.jump.coarse.width = 5;
        config.jump.coarse.height = 5;
        config.jump.fine.enabled = true;
        config.jump.fine.width = 5;
        config.jump.fine.height = 5;
        config.jump.fine.target_margin_percent = 0;
        config.jump.fine.visual_context_margin_percent = 5;
        let config = config.normalize().unwrap();

        let mut state = AppState::default();
        assert!(state.enter_jump_mode(
            &config.jump,
            config.final_adjust.clone(),
            jump_region(),
            VirtualKey::J
        ));
        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::G, true), None),
            Some(JumpSessionUpdate::StageAdvanced {
                stage_index: 1,
                region: JumpRegion {
                    left: 20,
                    top: 20,
                    width: 20,
                    height: 20,
                },
            })
        );

        let view = state.jump_view().unwrap();
        assert_eq!(view.session_region, jump_region());
        assert_eq!(
            view.target_region,
            JumpRegion {
                left: 20,
                top: 20,
                width: 20,
                height: 20,
            }
        );
        assert_eq!(view.preview_source_region, view.target_region);
        assert_eq!(view.client_draw_region, view.target_region);
    }

    #[test]
    fn jump_view_rehydrates_input_and_region_after_stage_backtrack() {
        let mut config = Config::default();
        config.jump.mode = crate::JumpMode::Precision;
        config.jump.coarse.width = 5;
        config.jump.coarse.height = 5;
        config.jump.fine.enabled = true;
        config.jump.fine.width = 10;
        config.jump.fine.height = 10;
        let config = config.normalize().unwrap();

        let mut state = AppState::default();
        assert!(state.enter_jump_mode(
            &config.jump,
            config.final_adjust.clone(),
            jump_region(),
            VirtualKey::J
        ));
        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::G, true), None),
            Some(JumpSessionUpdate::StageAdvanced {
                stage_index: 1,
                region: JumpRegion {
                    left: 20,
                    top: 20,
                    width: 20,
                    height: 20,
                },
            })
        );
        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::D, true), None),
            Some(JumpSessionUpdate::Consumed)
        );

        let before_backtrack = state.jump_view().unwrap();
        assert_eq!(before_backtrack.stage_index, 1);
        assert_eq!(before_backtrack.input, "D");

        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::Backspace, true), None),
            Some(JumpSessionUpdate::Consumed)
        );
        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::Backspace, true), None),
            Some(JumpSessionUpdate::StageBacktracked {
                stage_index: 0,
                region: jump_region(),
            })
        );

        let view = state.jump_view().unwrap();
        assert_eq!(view.stage_index, 0);
        assert_eq!(view.input, "");
        assert_eq!(view.session_region, jump_region());
        assert_eq!(view.target_region, jump_region());
        assert_eq!(view.grid_size, (5, 5));
    }

    #[test]
    fn zoom_scale_does_not_change_jump_subdivision_math() {
        let mut low_zoom = Config::default();
        low_zoom.jump.mode = crate::JumpMode::Precision;
        low_zoom.jump.coarse.width = 5;
        low_zoom.jump.coarse.height = 5;
        low_zoom.jump.fine.enabled = true;
        low_zoom.jump.fine.width = 5;
        low_zoom.jump.fine.height = 5;
        low_zoom.jump.fine.zoom_scale = 1.0;

        let mut high_zoom = low_zoom.clone();
        high_zoom.jump.fine.zoom_scale = 8.0;

        let low_zoom = low_zoom.normalize().unwrap();
        let high_zoom = high_zoom.normalize().unwrap();
        let mut low_state = AppState::default();
        let mut high_state = AppState::default();
        assert!(low_state.enter_jump_mode(
            &low_zoom.jump,
            low_zoom.final_adjust.clone(),
            jump_region(),
            VirtualKey::J
        ));
        assert!(high_state.enter_jump_mode(
            &high_zoom.jump,
            high_zoom.final_adjust.clone(),
            jump_region(),
            VirtualKey::J
        ));

        for key in [VirtualKey::B, VirtualKey::B] {
            assert_eq!(
                low_state.handle_jump_input(KeyEvent::new(key, true), None),
                high_state.handle_jump_input(KeyEvent::new(key, true), None)
            );
        }

        let low_view = low_state.jump_view().unwrap();
        let high_view = high_state.jump_view().unwrap();
        assert_eq!(low_view.target_region, high_view.target_region);
        assert_ne!(
            low_view.stages[low_view.stage_index].zoom_scale,
            high_view.stages[high_view.stage_index].zoom_scale
        );
    }

    #[test]
    fn preserved_ctrl_w_passes_through_when_not_configured_as_system_binding() {
        let mut state = state_with_bound_key(VirtualKey::W);
        let ctrl_w = ctrl_w_down();

        assert!(!state.should_swallow_key(&ctrl_w));

        state.route_key_event(ctrl_w, Some(Action::MoveLeft));

        assert_eq!(collect_commands(&mut state), Vec::new());
    }

    #[test]
    fn rightalt_e_matches_when_synthetic_ctrl_present() {
        let state = state_with_bound_chords([KeyChord::parse("RightAlt+E").unwrap()]);
        let event = right_alt_e_with_synthetic_ctrl();

        assert!(state.has_explicit_configured_binding_for_event(&event));
        assert!(state.should_swallow_key(&event));
    }

    #[test]
    fn rightalt_ctrl_e_requires_physical_ctrl() {
        let state = state_with_bound_chords([KeyChord::parse("RightAlt+Ctrl+E").unwrap()]);
        let synthetic = right_alt_e_with_synthetic_ctrl();
        assert!(!state.has_explicit_configured_binding_for_event(&synthetic));

        let mut physical = synthetic;
        physical.left_ctrl_down = true;
        assert!(state.has_explicit_configured_binding_for_event(&physical));
    }

    #[test]
    fn ctrl_w_unconfigured_passthrough() {
        let state = state_with_bound_key(VirtualKey::W);
        let ctrl_w = ctrl_w_down();
        assert!(!state.should_swallow_key(&ctrl_w));
    }

    #[test]
    fn ctrl_w_configured_swallowed() {
        let mut state = state_with_bound_key(VirtualKey::W);
        state.set_system_bindings(RuntimeSystemBindings::new(
            KeyChord::parse("Ctrl+W").unwrap(),
            KeyChord::parse("Escape").unwrap(),
        ));
        let ctrl_w = ctrl_w_down();
        assert!(state.should_swallow_key(&ctrl_w));
    }

    #[test]
    fn alt_tab_unconfigured_passthrough() {
        let state = state_with_bound_key(VirtualKey::Tab);
        let mut event = KeyEvent::new(VirtualKey::Tab, true);
        event.alt_down = true;
        assert!(!state.should_swallow_key(&event));
    }

    #[test]
    fn alt_f4_unconfigured_passthrough() {
        let state = state_with_bound_key(VirtualKey::F4);
        let mut event = KeyEvent::new(VirtualKey::F4, true);
        event.alt_down = true;
        assert!(!state.should_swallow_key(&event));
    }

    #[test]
    fn toggle_active_ctrl_w_is_swallowed_and_routed_as_toggle() {
        let mut state = state_with_bound_key(VirtualKey::W);
        state.set_system_bindings(RuntimeSystemBindings::new(
            KeyChord::parse("Ctrl+W").unwrap(),
            KeyChord::parse("Escape").unwrap(),
        ));
        let ctrl_w = ctrl_w_down();

        assert!(state.should_swallow_key(&ctrl_w));

        state.route_key_event(ctrl_w, Some(Action::MoveLeft));

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::ToggleActiveMode]
        );
    }

    #[test]
    fn escape_outside_jump_queues_exit() {
        let mut state = AppState::default();

        state.route_key_event(KeyEvent::new(VirtualKey::Escape, true), None);

        assert_eq!(collect_commands(&mut state), vec![AppCommand::Exit]);
    }

    #[test]
    fn escape_inside_jump_queues_jump_input_cancel_path_not_exit() {
        let mut state = AppState::default();
        enter_jump_mode(&mut state, VirtualKey::J);
        let event = KeyEvent::new(VirtualKey::Escape, true);

        state.route_key_event(event, None);

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::JumpInput(event, None)]
        );
    }

    #[test]
    fn movement_keys_route_to_jump_input_while_jump_is_active() {
        let mut state = AppState::default();
        enter_jump_mode(&mut state, VirtualKey::F);
        let event = KeyEvent::new(VirtualKey::Left, true);

        state.route_key_event(event, Some(Action::MoveLeft));

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::JumpInput(event, Some(Action::MoveLeft))]
        );
    }

    #[test]
    fn movement_keys_route_to_grid_input_while_grid_is_active() {
        let mut state = AppState::default();
        enter_grid_mode(&mut state, VirtualKey::G);
        let event = KeyEvent::new(VirtualKey::W, true);

        state.route_key_event(event, Some(Action::MoveUp));

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::GridInput(event, Some(Action::MoveUp))]
        );
    }

    #[test]
    fn entering_grid_exits_jump_and_entering_jump_exits_grid() {
        let mut state = AppState::default();
        enter_jump_mode(&mut state, VirtualKey::J);
        assert!(state.is_jump_active());

        enter_grid_mode(&mut state, VirtualKey::G);
        assert!(!state.is_jump_active());
        assert!(state.is_grid_active());

        enter_jump_mode(&mut state, VirtualKey::J);
        assert!(state.is_jump_active());
        assert!(!state.is_grid_active());
    }

    #[test]
    fn grid_mode_owns_directional_actions_plus_escape_and_backspace_keys() {
        let mut state = AppState::default();
        enter_grid_mode(&mut state, VirtualKey::G);

        for (key, action) in [
            (VirtualKey::F, Some(Action::MoveRight)),
            (VirtualKey::J, Some(Action::MoveLeft)),
            (VirtualKey::I, Some(Action::MoveUp)),
            (VirtualKey::K, Some(Action::MoveDown)),
            (VirtualKey::Backspace, None),
            (VirtualKey::Escape, None),
        ] {
            let event = KeyEvent::new(key, true);
            state.route_key_event(event, action.clone());
            assert_eq!(
                state.pop_command(),
                Some(AppCommand::GridInput(event, action)),
                "{key:?}"
            );
        }
    }

    #[test]
    fn grid_mode_still_owns_wasd_when_default_action_mapping_is_used() {
        let mut state = AppState::default();
        enter_grid_mode(&mut state, VirtualKey::G);

        for (key, action) in [
            (VirtualKey::W, Some(Action::MoveUp)),
            (VirtualKey::A, Some(Action::MoveLeft)),
            (VirtualKey::S, Some(Action::MoveDown)),
            (VirtualKey::D, Some(Action::MoveRight)),
        ] {
            let event = KeyEvent::new(key, true);
            state.route_key_event(event, action.clone());
            assert_eq!(
                state.pop_command(),
                Some(AppCommand::GridInput(event, action))
            );
        }
    }

    #[test]
    fn grid_activation_key_does_not_immediately_re_exit() {
        let mut state = AppState::default();
        enter_grid_mode(&mut state, VirtualKey::G);

        state.route_key_event(KeyEvent::new(VirtualKey::G, true), Some(Action::GridMode));
        assert_eq!(collect_commands(&mut state), Vec::new());
        assert!(state.is_grid_active());
    }

    #[test]
    fn grid_escape_cancels_and_backspace_undoes() {
        let mut state = AppState::default();
        enter_grid_mode(&mut state, VirtualKey::G);

        assert_eq!(
            state.handle_grid_input(KeyEvent::new(VirtualKey::D, true), Some(Action::MoveRight)),
            Some(GridInputUpdate::Updated {
                region: JumpRegion {
                    left: 50,
                    top: 0,
                    width: 50,
                    height: 100,
                },
                move_cursor: true,
            })
        );
        assert_eq!(
            state.handle_grid_input(KeyEvent::new(VirtualKey::Backspace, true), None),
            Some(GridInputUpdate::Updated {
                region: jump_region(),
                move_cursor: true,
            })
        );
        assert_eq!(
            state.handle_grid_input(KeyEvent::new(VirtualKey::Escape, true), None),
            Some(GridInputUpdate::Cancelled)
        );
    }

    #[test]
    fn grid_movement_uses_action_mapping_for_all_directions() {
        let cases = [
            (
                VirtualKey::F,
                Action::MoveRight,
                JumpRegion {
                    left: 50,
                    top: 0,
                    width: 50,
                    height: 100,
                },
            ),
            (
                VirtualKey::J,
                Action::MoveLeft,
                JumpRegion {
                    left: 0,
                    top: 0,
                    width: 50,
                    height: 100,
                },
            ),
            (
                VirtualKey::I,
                Action::MoveUp,
                JumpRegion {
                    left: 0,
                    top: 0,
                    width: 100,
                    height: 50,
                },
            ),
            (
                VirtualKey::K,
                Action::MoveDown,
                JumpRegion {
                    left: 0,
                    top: 50,
                    width: 100,
                    height: 50,
                },
            ),
        ];

        for (key, action, expected_region) in cases {
            let mut state = AppState::default();
            enter_grid_mode(&mut state, VirtualKey::G);
            let update = state.handle_grid_input(KeyEvent::new(key, true), Some(action.clone()));
            assert_eq!(
                update,
                Some(GridInputUpdate::Updated {
                    region: expected_region,
                    move_cursor: true,
                }),
                "{key:?} should map via {action:?}"
            );
        }
    }

    #[test]
    fn grid_input_non_movement_action_is_consumed_without_splitting() {
        let mut state = AppState::default();
        enter_grid_mode(&mut state, VirtualKey::G);

        assert_eq!(
            state.handle_grid_input(KeyEvent::new(VirtualKey::F, true), Some(Action::ShowHelp)),
            Some(GridInputUpdate::Consumed)
        );
    }

    #[test]
    fn grid_completion_clears_active_keys_when_exiting() {
        let mut state = AppState::default();
        assert!(state.enter_grid_mode(
            jump_region(),
            JumpRegion {
                left: 0,
                top: 0,
                width: 50,
                height: 100,
            },
            25,
            25,
            true,
            true,
            true,
            VirtualKey::G,
        ));
        state.active_keys.insert(VirtualKey::W);
        state
            .active_trigger_chords
            .insert(KeyChord::from_key(VirtualKey::W));

        assert_eq!(
            state.handle_grid_input(KeyEvent::new(VirtualKey::A, true), Some(Action::MoveLeft)),
            Some(GridInputUpdate::Completed {
                x: 13,
                y: 50,
                region: JumpRegion {
                    left: 0,
                    top: 0,
                    width: 25,
                    height: 100,
                },
            })
        );
        state.complete_grid_mode();

        assert!(!state.has_active_action_keys());
        assert_eq!(state.resolve_jump_overlay(), JumpOverlayResolution::Hidden);
    }

    #[test]
    fn grid_completion_triggers_with_remapped_movement_action() {
        let mut state = AppState::default();
        assert!(state.enter_grid_mode(
            jump_region(),
            JumpRegion {
                left: 50,
                top: 0,
                width: 50,
                height: 100,
            },
            25,
            25,
            true,
            true,
            true,
            VirtualKey::G,
        ));

        assert_eq!(
            state.handle_grid_input(KeyEvent::new(VirtualKey::F, true), Some(Action::MoveRight)),
            Some(GridInputUpdate::Completed {
                x: 88,
                y: 50,
                region: JumpRegion {
                    left: 75,
                    top: 0,
                    width: 25,
                    height: 100,
                },
            })
        );
    }

    #[test]
    fn help_action_toggles_help_visibility() {
        let mut state = AppState::default();

        state.route_key_event(KeyEvent::new(VirtualKey::H, true), Some(Action::ShowHelp));
        assert_eq!(collect_commands(&mut state), vec![AppCommand::ToggleHelp]);

        state.toggle_help();
        assert!(state.help_visible());
        state.toggle_help();
        assert!(!state.help_visible());
    }

    #[test]
    fn slash_while_active_toggles_help() {
        let mut state = state_with_bound_key(VirtualKey::Oem2);

        state.route_key_event(
            KeyEvent::new(VirtualKey::Oem2, true),
            Some(Action::ShowHelp),
        );

        assert_eq!(collect_commands(&mut state), vec![AppCommand::ToggleHelp]);
    }

    #[test]
    fn slash_key_up_does_not_retrigger_help() {
        let mut state = state_with_bound_key(VirtualKey::Oem2);

        state.route_key_event(
            KeyEvent::new(VirtualKey::Oem2, false),
            Some(Action::ShowHelp),
        );

        assert_eq!(collect_commands(&mut state), Vec::new());
    }

    #[test]
    fn escape_dismisses_visible_help() {
        let mut state = AppState::default();
        state.toggle_help();

        state.route_key_event(KeyEvent::new(VirtualKey::Escape, true), None);

        assert_eq!(collect_commands(&mut state), vec![AppCommand::HideHelp]);
    }

    #[test]
    fn escape_hides_help_before_exit() {
        let mut state = AppState::default();
        state.toggle_help();

        state.route_key_event(KeyEvent::new(VirtualKey::Escape, true), Some(Action::Exit));
        assert_eq!(collect_commands(&mut state), vec![AppCommand::HideHelp]);

        state.hide_help();
        state.route_key_event(KeyEvent::new(VirtualKey::Escape, true), Some(Action::Exit));
        assert_eq!(collect_commands(&mut state), vec![AppCommand::Exit]);
    }

    #[test]
    fn disabling_active_mode_hides_help_and_ignores_slash_help() {
        let mut state = state_with_bound_key(VirtualKey::Oem2);
        state.toggle_help();

        state.set_active_mode(false);
        state.route_key_event(
            KeyEvent::new(VirtualKey::Oem2, true),
            Some(Action::ShowHelp),
        );

        assert!(!state.help_visible());
        assert_eq!(collect_commands(&mut state), Vec::new());
    }

    #[test]
    fn mode_context_tracks_state_transitions_for_status_and_help_payloads() {
        let mut state = AppState::default();
        assert_eq!(state.mode_context(), ModeContext::Active);

        enter_jump_mode(&mut state, VirtualKey::J);
        assert_eq!(state.mode_context(), ModeContext::Jump);

        state.exit_jump_mode();
        enter_grid_mode(&mut state, VirtualKey::G);
        assert_eq!(state.mode_context(), ModeContext::Grid);

        state.exit_grid_mode();
        enter_ui_hint_mode(&mut state, VirtualKey::U);
        assert_eq!(state.mode_context(), ModeContext::UiHintActive);

        state.exit_ui_hint_mode();
        state.enter_ui_hint_querying(VirtualKey::U, 1, 0);
        assert_eq!(state.mode_context(), ModeContext::UiHintQuerying);
        assert!(state.activate_ui_hint_if_querying(1));
        assert_eq!(state.mode_context(), ModeContext::UiHintActive);
        state.exit_ui_hint_mode();

        state.set_active_mode(false);
        assert_eq!(state.mode_context(), ModeContext::Inactive);
    }

    #[test]
    fn system_binding_routing_order_precedes_preserved_shortcut_filter() {
        let mut state = state_with_bound_key(VirtualKey::W);
        state.set_system_bindings(RuntimeSystemBindings::new(
            KeyChord::parse("Ctrl+W").unwrap(),
            KeyChord::parse("Escape").unwrap(),
        ));
        let ctrl_w = ctrl_w_down();

        assert!(!state.is_jump_active());
        assert!(state.is_preserved_shortcut(&ctrl_w));
        assert!(state.is_system_binding(&ctrl_w));

        state.route_key_event(ctrl_w, Some(Action::MoveLeft));

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::ToggleActiveMode]
        );
    }

    #[test]
    fn configured_toggle_routes_before_preserved_shortcut() {
        let mut state = AppState::default();
        state.set_system_bindings(RuntimeSystemBindings::new(
            KeyChord::parse("Ctrl+W").unwrap(),
            KeyChord::parse("Escape").unwrap(),
        ));
        let mut ctrl_w = KeyEvent::new(VirtualKey::W, true);
        ctrl_w.ctrl_down = true;

        state.route_key_event(ctrl_w, None);

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::ToggleActiveMode]
        );
    }

    #[test]
    fn inactive_mode_ignores_normal_movement_key_down() {
        let mut state = state_with_bound_key(VirtualKey::Left);
        state.set_active_mode(false);

        state.route_key_event(
            KeyEvent::new(VirtualKey::Left, true),
            Some(Action::MoveLeft),
        );

        assert_eq!(collect_commands(&mut state), Vec::new());
    }

    #[test]
    fn inactive_mode_ignores_normal_movement_key_up_without_mutating_active_keys() {
        let mut state = state_with_bound_key(VirtualKey::Left);
        state.active_keys.insert(VirtualKey::Left);
        state.set_active_mode(false);

        state.route_key_event(
            KeyEvent::new(VirtualKey::Left, false),
            Some(Action::MoveLeft),
        );

        assert_eq!(collect_commands(&mut state), Vec::new());
        assert!(state.active_keys.is_empty());
    }

    #[test]
    fn inactive_mode_ignores_click_and_jump_bindings() {
        let mut state = state_with_bound_key(VirtualKey::J);
        state.set_bound_keys([VirtualKey::J, VirtualKey::A]);
        state.set_active_mode(false);

        state.route_key_event(KeyEvent::new(VirtualKey::A, true), Some(Action::LeftClick));
        state.route_key_event(KeyEvent::new(VirtualKey::J, true), Some(Action::JumpMode));

        assert_eq!(collect_commands(&mut state), Vec::new());
        assert!(!state.is_jump_active());
    }

    #[test]
    fn disable_action_routes_one_way_set_instead_of_toggle() {
        let mut state = state_with_bound_key(VirtualKey::Q);

        state.route_key_event(KeyEvent::new(VirtualKey::Q, true), Some(Action::Disable));
        state.route_key_event(KeyEvent::new(VirtualKey::Q, false), Some(Action::Disable));

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::SetActiveMode { active: false }]
        );
    }

    #[test]
    fn disable_action_routes_before_exclusive_mode_inputs() {
        for mode in ["jump", "grid"] {
            let mut state = state_with_bound_key(VirtualKey::Q);
            match mode {
                "jump" => enter_jump_mode(&mut state, VirtualKey::J),
                "grid" => enter_grid_mode(&mut state, VirtualKey::G),
                _ => unreachable!(),
            }

            state.route_key_event(KeyEvent::new(VirtualKey::Q, true), Some(Action::Disable));

            assert_eq!(
                collect_commands(&mut state),
                vec![AppCommand::SetActiveMode { active: false }],
                "{mode}"
            );
        }
    }

    #[test]
    fn inactive_disable_action_does_not_toggle_back_on() {
        let mut state = state_with_bound_key(VirtualKey::Q);
        state.set_active_mode(false);

        state.route_key_event(KeyEvent::new(VirtualKey::Q, true), Some(Action::Disable));

        assert_eq!(collect_commands(&mut state), Vec::new());
        assert!(!state.active_mode());
    }

    #[test]
    fn inactive_mode_still_routes_system_commands() {
        let mut state = state_with_bound_key(VirtualKey::E);
        state.set_active_mode(false);

        state.route_key_event(ctrl_e_down(), Some(Action::MoveLeft));
        state.route_key_event(KeyEvent::new(VirtualKey::Escape, true), Some(Action::Exit));

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::ToggleActiveMode, AppCommand::Exit]
        );
    }

    #[test]
    fn inactive_mode_preserves_os_shortcuts_unless_configured_as_system_bindings() {
        let mut state = state_with_bound_key(VirtualKey::S);
        state.set_active_mode(false);
        let mut ctrl_s = KeyEvent::new(VirtualKey::S, true);
        ctrl_s.ctrl_down = true;

        assert!(!state.should_swallow_key(&ctrl_s));
        state.route_key_event(ctrl_s, Some(Action::MoveLeft));
        assert_eq!(collect_commands(&mut state), Vec::new());

        state.set_system_bindings(RuntimeSystemBindings::new(
            KeyChord::parse("Ctrl+S").unwrap(),
            KeyChord::parse("Escape").unwrap(),
        ));

        assert!(state.should_swallow_key(&ctrl_s));
        state.route_key_event(ctrl_s, Some(Action::MoveLeft));
        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::ToggleActiveMode]
        );
    }

    #[test]
    fn ctrl_e_key_down_toggles_active_mode_without_movement_action() {
        let mut state = state_with_bound_key(VirtualKey::E);

        state.route_key_event(ctrl_e_down(), Some(Action::MoveLeft));

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::ToggleActiveMode]
        );
    }

    #[test]
    fn toggle_active_requires_exact_modifiers() {
        let mut state = state_with_bound_key(VirtualKey::E);
        let mut event = ctrl_e_down();
        event.shift_down = true;

        state.route_key_event(event, Some(Action::MoveLeft));

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::KeyAction {
                action: Action::MoveLeft,
                is_down: true,
            }]
        );
    }

    #[test]
    fn unbound_key_down_generates_no_command() {
        let mut state = state_with_bound_key(VirtualKey::Left);

        state.route_key_event(KeyEvent::new(VirtualKey::Right, true), None);

        assert_eq!(collect_commands(&mut state), Vec::new());
    }

    #[test]
    fn unbound_key_up_generates_no_command() {
        let mut state = state_with_bound_key(VirtualKey::Left);

        state.route_key_event(KeyEvent::new(VirtualKey::Right, false), None);

        assert_eq!(collect_commands(&mut state), Vec::new());
    }

    #[test]
    fn queued_key_events_route_commands_in_fifo_order() {
        let mut state = state_with_bound_key(VirtualKey::Left);
        state.enqueue_key_event(KeyEvent::new(VirtualKey::Left, true));
        state.enqueue_key_event(KeyEvent::new(VirtualKey::Left, false));
        state.enqueue_key_event(KeyEvent::new(VirtualKey::Escape, true));

        while let Some(event) = state.pop_key_event() {
            state.route_key_event(event, lookup_test_action(event));
        }

        assert_eq!(
            collect_commands(&mut state),
            vec![
                AppCommand::KeyAction {
                    action: Action::MoveLeft,
                    is_down: true,
                },
                AppCommand::KeyAction {
                    action: Action::MoveLeft,
                    is_down: false,
                },
                AppCommand::Exit,
            ]
        );
    }

    #[test]
    fn queued_mixed_system_and_movement_commands_preserve_fifo_order() {
        let mut state = state_with_bound_key(VirtualKey::Left);
        state.enqueue_key_event(ctrl_e_down());
        state.enqueue_key_event(KeyEvent::new(VirtualKey::Escape, true));
        state.enqueue_key_event(KeyEvent::new(VirtualKey::Left, true));

        while let Some(event) = state.pop_key_event() {
            state.route_key_event(event, lookup_test_action(event));
        }

        assert_eq!(
            collect_commands(&mut state),
            vec![
                AppCommand::ToggleActiveMode,
                AppCommand::Exit,
                AppCommand::KeyAction {
                    action: Action::MoveLeft,
                    is_down: true,
                },
            ]
        );
    }
    #[test]
    fn mode_context_precedence_remains_stable_for_existing_modes() {
        let mut state = AppState::default();
        assert_eq!(state.mode_context(), ModeContext::Active);

        state.enter_bookmark_mode(VirtualKey::B);
        assert_eq!(state.mode_context(), ModeContext::Bookmark);

        enter_ui_hint_mode(&mut state, VirtualKey::U);
        assert_eq!(state.mode_context(), ModeContext::Bookmark);

        state.exit_bookmark_mode();
        assert_eq!(state.mode_context(), ModeContext::UiHintActive);
    }

    #[test]
    fn bookmark_mode_routes_recall_and_set_and_clear() {
        let mut state = AppState::default();
        state.route_key_event(
            KeyEvent::new(VirtualKey::B, true),
            Some(Action::BookmarkSlot(1)),
        );
        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::RecallBookmarkSlot(1)]
        );

        state.route_key_event(
            KeyEvent::new(VirtualKey::B, true),
            Some(Action::BookmarkMode),
        );
        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::EnterBookmarkMode {
                activation_key: VirtualKey::B
            }]
        );

        state.enter_bookmark_mode(VirtualKey::B);
        state.route_key_event(
            KeyEvent::new(VirtualKey::Num1, true),
            Some(Action::BookmarkSlot(1)),
        );
        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::SetBookmarkSlot(1)]
        );

        state.route_key_event(KeyEvent::new(VirtualKey::Backspace, true), None);
        state.route_key_event(
            KeyEvent::new(VirtualKey::Num1, true),
            Some(Action::BookmarkSlot(1)),
        );
        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::ClearBookmarkSlot(1)]
        );
    }

    #[test]
    fn show_bookmarks_command_routes_on_keydown() {
        let mut state = AppState::default();
        state.route_key_event(
            KeyEvent::new(VirtualKey::B, true),
            Some(Action::ShowBookmarks),
        );
        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::ShowBookmarks]
        );
    }

    #[test]
    fn enter_bookmark_mode_sets_active_flag() {
        let mut state = AppState::default();
        assert!(!state.is_bookmark_mode_active());
        state.enter_bookmark_mode(VirtualKey::B);
        assert!(state.is_bookmark_mode_active());
    }

    #[test]
    fn exit_bookmark_mode_clears_active_flag() {
        let mut state = AppState::default();
        state.enter_bookmark_mode(VirtualKey::B);
        assert!(state.is_bookmark_mode_active());
        state.exit_bookmark_mode();
        assert!(!state.is_bookmark_mode_active());
    }
    #[test]
    fn exit_routed_before_bookmark_mode_cancel() {
        let mut state = AppState::default();
        state.enter_bookmark_mode(VirtualKey::B);
        state.route_key_event(KeyEvent::new(VirtualKey::Escape, true), None);
        assert_eq!(collect_commands(&mut state), vec![AppCommand::Exit]);
    }

    #[test]
    fn exit_routed_before_ui_hint_mode_handling() {
        let mut state = AppState::default();
        state.enter_ui_hint_querying(VirtualKey::U, 1, 0);
        state.route_key_event(KeyEvent::new(VirtualKey::Escape, true), None);
        assert_eq!(collect_commands(&mut state), vec![AppCommand::Exit]);
    }

    #[test]
    fn panic_reset_routed_before_exclusive_modes() {
        let mut state = AppState::default();
        state.enter_bookmark_mode(VirtualKey::B);
        let mut event = KeyEvent::new(VirtualKey::Escape, true);
        event.alt_down = true;
        event.right_alt_down = true;
        state.route_key_event(event, Some(Action::PanicReset));
        assert_eq!(
            collect_commands(&mut state),
            vec![
                AppCommand::SetActiveMode { active: false },
                AppCommand::PanicReset
            ]
        );
    }

    #[test]
    fn escape_exit_wins_over_bookmark_cancel_when_both_escape() {
        let mut state = AppState::default();
        state.set_system_bindings(RuntimeSystemBindings::default());
        state.enter_bookmark_mode(VirtualKey::B);
        state.route_key_event(KeyEvent::new(VirtualKey::Escape, true), None);
        assert_eq!(collect_commands(&mut state), vec![AppCommand::Exit]);
    }

    #[test]
    fn ctrl_alt_escape_exit_does_not_override_plain_escape_bookmark_cancel() {
        let mut state = AppState::default();
        state.set_system_bindings(RuntimeSystemBindings {
            exit: KeyChord::parse("Ctrl+Alt+Escape").unwrap(),
            ..RuntimeSystemBindings::default()
        });
        state.enter_bookmark_mode(VirtualKey::B);
        state.route_key_event(KeyEvent::new(VirtualKey::Escape, true), None);
        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::CancelBookmarkMode]
        );
    }

    #[test]
    fn bookmark_mode_uses_configured_cancel_key() {
        let mut state = AppState::default();
        state.set_bookmark_keys(VirtualKey::Q, VirtualKey::Backspace);
        state.enter_bookmark_mode(VirtualKey::B);
        state.route_key_event(KeyEvent::new(VirtualKey::Q, true), None);
        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::CancelBookmarkMode]
        );
    }

    #[test]
    fn bookmark_mode_uses_configured_clear_modifier_key() {
        let mut state = AppState::default();
        state.set_bookmark_keys(VirtualKey::Escape, VirtualKey::Q);
        state.enter_bookmark_mode(VirtualKey::B);
        state.route_key_event(KeyEvent::new(VirtualKey::Q, true), None);
        state.route_key_event(
            KeyEvent::new(VirtualKey::Num1, true),
            Some(Action::BookmarkSlot(1)),
        );
        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::ClearBookmarkSlot(1)]
        );
    }

    #[test]
    fn plain_escape_cancels_bookmark_mode_when_exit_is_ctrl_escape() {
        let mut state = AppState::default();
        state.set_system_bindings(RuntimeSystemBindings {
            exit: KeyChord::parse("Ctrl+Escape").unwrap(),
            ..RuntimeSystemBindings::default()
        });
        state.enter_bookmark_mode(VirtualKey::B);
        state.route_key_event(KeyEvent::new(VirtualKey::Escape, true), None);
        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::CancelBookmarkMode]
        );
    }

    #[test]
    fn ctrl_escape_exits_even_when_bookmark_mode_active() {
        let mut state = AppState::default();
        state.set_system_bindings(RuntimeSystemBindings {
            exit: KeyChord::parse("Ctrl+Escape").unwrap(),
            ..RuntimeSystemBindings::default()
        });
        state.enter_bookmark_mode(VirtualKey::B);
        let mut ev = KeyEvent::new(VirtualKey::Escape, true);
        ev.ctrl_down = true;
        state.route_key_event(ev, Some(Action::Exit));
        assert_eq!(collect_commands(&mut state), vec![AppCommand::Exit]);
    }

    #[test]
    fn panic_reset_wins_over_bookmark_cancel() {
        let mut state = AppState::default();
        state.enter_bookmark_mode(VirtualKey::B);
        let mut ev = KeyEvent::new(VirtualKey::Escape, true);
        ev.alt_down = true;
        ev.right_alt_down = true;
        state.route_key_event(ev, Some(Action::PanicReset));
        assert_eq!(
            collect_commands(&mut state),
            vec![
                AppCommand::SetActiveMode { active: false },
                AppCommand::PanicReset
            ]
        );
    }

    #[test]
    fn panic_reset_works_with_extra_modifiers() {
        let mut state = AppState::default();
        let mut event = KeyEvent::new(VirtualKey::Escape, true);
        event.alt_down = true;
        event.right_alt_down = true;
        event.ctrl_down = true;
        event.left_ctrl_down = true;
        event.shift_down = true;
        event.left_shift_down = true;

        assert!(state.should_swallow_key(&event));
        state.route_key_event(event, None);

        assert_eq!(
            collect_commands(&mut state),
            vec![
                AppCommand::SetActiveMode { active: false },
                AppCommand::PanicReset
            ]
        );
    }

    #[test]
    fn panic_reset_optionally_sets_idle() {
        for (sets_idle, expected) in [
            (
                true,
                vec![
                    AppCommand::SetActiveMode { active: false },
                    AppCommand::PanicReset,
                ],
            ),
            (false, vec![AppCommand::PanicReset]),
        ] {
            let mut state = AppState::default();
            state.set_system_bindings(RuntimeSystemBindings {
                panic_reset_sets_idle: sets_idle,
                ..RuntimeSystemBindings::default()
            });
            let mut event = KeyEvent::new(VirtualKey::Escape, true);
            event.alt_down = true;
            event.right_alt_down = true;

            state.route_key_event(event, None);

            assert_eq!(collect_commands(&mut state), expected);
        }
    }

    fn age_held_key(state: &mut AppState, key: VirtualKey, age: Duration) {
        let held_state = state
            .held_physical_keys
            .get_mut(&key)
            .expect("held key should be tracked");
        held_state.pressed_at = std::time::Instant::now() - age;
    }

    fn age_physically_up_since(state: &mut AppState, key: VirtualKey, age: Duration) {
        let held_state = state
            .held_physical_keys
            .get_mut(&key)
            .expect("held key should be tracked");
        held_state.physically_up_since = Some(std::time::Instant::now() - age);
    }

    #[test]
    fn reconcile_does_not_release_before_timeout() {
        let mut state = state_with_bound_key(VirtualKey::LeftShift);
        state.route_key_event(
            KeyEvent::new(VirtualKey::LeftShift, true),
            Some(Action::SlowMouse),
        );
        assert_eq!(collect_commands(&mut state).len(), 1);
        age_held_key(
            &mut state,
            VirtualKey::LeftShift,
            Duration::from_millis(100),
        );

        state.reconcile_stale_keys(Duration::from_secs(60), |_| false);

        assert_eq!(collect_commands(&mut state), Vec::new());
        assert!(state
            .held_physical_keys
            .contains_key(&VirtualKey::LeftShift));
        assert!(!state
            .reconciled_released_keys
            .contains(&VirtualKey::LeftShift));
        assert!(state.should_swallow_key(&KeyEvent::new(VirtualKey::LeftShift, false)));
    }

    #[test]
    fn reconcile_first_physically_up_observation_does_not_release_modifier_before_timeout() {
        let mut state = state_with_bound_key(VirtualKey::LeftShift);
        state.route_key_event(
            KeyEvent::new(VirtualKey::LeftShift, true),
            Some(Action::SlowMouse),
        );
        assert_eq!(collect_commands(&mut state).len(), 1);

        state.reconcile_stale_keys(Duration::from_millis(500), |_| false);

        assert_eq!(collect_commands(&mut state), Vec::new());
        assert!(state
            .held_physical_keys
            .contains_key(&VirtualKey::LeftShift));
        assert!(state
            .held_physical_keys
            .get(&VirtualKey::LeftShift)
            .expect("held key should be tracked")
            .physically_up_since
            .is_some());
        assert!(!state
            .reconciled_released_keys
            .contains(&VirtualKey::LeftShift));
        assert!(state.should_swallow_key(&KeyEvent::new(VirtualKey::LeftShift, false)));
    }

    #[test]
    fn reconcile_releases_stale_modifier_after_physically_up_timeout() {
        let mut state = state_with_bound_key(VirtualKey::LeftShift);
        state.route_key_event(
            KeyEvent::new(VirtualKey::LeftShift, true),
            Some(Action::SlowMouse),
        );
        assert_eq!(collect_commands(&mut state).len(), 1);

        state.reconcile_stale_keys(Duration::from_millis(500), |_| false);
        age_physically_up_since(
            &mut state,
            VirtualKey::LeftShift,
            Duration::from_millis(500),
        );
        state.reconcile_stale_keys(Duration::from_millis(500), |_| false);

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::KeyAction {
                action: Action::SlowMouse,
                is_down: false,
            }]
        );
        assert!(!state
            .held_physical_keys
            .contains_key(&VirtualKey::LeftShift));
        assert!(state
            .reconciled_released_keys
            .contains(&VirtualKey::LeftShift));
        assert!(!state.should_swallow_key(&KeyEvent::new(VirtualKey::LeftShift, false)));
    }

    #[test]
    fn reconcile_releases_after_timeout() {
        let mut state = state_with_bound_key(VirtualKey::LeftShift);
        state.route_key_event(
            KeyEvent::new(VirtualKey::LeftShift, true),
            Some(Action::SlowMouse),
        );
        assert_eq!(collect_commands(&mut state).len(), 1);
        state.reconcile_stale_keys(Duration::from_millis(500), |_| false);
        age_physically_up_since(
            &mut state,
            VirtualKey::LeftShift,
            Duration::from_millis(500),
        );

        state.reconcile_stale_keys(Duration::from_millis(500), |_| false);

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::KeyAction {
                action: Action::SlowMouse,
                is_down: false,
            }]
        );
        assert!(!state
            .held_physical_keys
            .contains_key(&VirtualKey::LeftShift));
        assert!(state
            .reconciled_released_keys
            .contains(&VirtualKey::LeftShift));
        assert!(!state.should_swallow_key(&KeyEvent::new(VirtualKey::LeftShift, false)));
    }

    #[test]
    fn panic_reset_ignores_stuck_timeout() {
        let mut state = state_with_bound_key(VirtualKey::E);
        state.route_key_event(KeyEvent::new(VirtualKey::E, true), Some(Action::MoveUp));
        assert_eq!(collect_commands(&mut state).len(), 1);

        state.clear_runtime_input_state();

        assert_eq!(state.held_physical_key_count(), 0);
        assert_eq!(state.reconciled_released_key_count(), 0);
        assert!(!state.has_active_action_keys());
        assert!(!state.should_swallow_key(&KeyEvent::new(VirtualKey::E, false)));
    }

    #[test]
    fn reconcile_ignores_stale_move_trigger() {
        let mut state = state_with_bound_key(VirtualKey::E);
        state.route_key_event(KeyEvent::new(VirtualKey::E, true), Some(Action::MoveUp));
        assert_eq!(collect_commands(&mut state).len(), 1);

        state.reconcile_stale_keys(Duration::ZERO, |_| {
            panic!("non-modifier keys should not be physically queried")
        });

        assert_eq!(collect_commands(&mut state), Vec::new());
        assert!(state.held_physical_keys.contains_key(&VirtualKey::E));
        assert!(!state.reconciled_released_keys.contains(&VirtualKey::E));
        assert!(state.should_swallow_key(&KeyEvent::new(VirtualKey::E, false)));
    }

    #[test]
    fn reconcile_releases_stale_slow_mouse_trigger() {
        let mut state = state_with_bound_key(VirtualKey::LeftShift);
        state.route_key_event(
            KeyEvent::new(VirtualKey::LeftShift, true),
            Some(Action::SlowMouse),
        );
        assert_eq!(collect_commands(&mut state).len(), 1);

        state.reconcile_stale_keys(Duration::ZERO, |_| false);

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::KeyAction {
                action: Action::SlowMouse,
                is_down: false,
            }]
        );
        assert!(!state
            .held_physical_keys
            .contains_key(&VirtualKey::LeftShift));
        assert!(state
            .reconciled_released_keys
            .contains(&VirtualKey::LeftShift));
        assert!(!state.should_swallow_key(&KeyEvent::new(VirtualKey::LeftShift, false)));
    }

    #[test]
    fn reconcile_clears_owned_modifier_when_physically_up() {
        let mut state = AppState::default();
        state.set_owned_modifiers([VirtualKey::RightAlt]);
        state.route_key_event(KeyEvent::new(VirtualKey::RightAlt, true), None);
        state.route_key_event(KeyEvent::new(VirtualKey::A, true), Some(Action::MoveLeft));
        assert!(state.should_swallow_key(&KeyEvent::new(VirtualKey::RightAlt, true)));

        state.reconcile_stale_keys(Duration::ZERO, |key| key != VirtualKey::RightAlt);

        assert_eq!(collect_commands(&mut state), Vec::new());
        assert!(!state.held_physical_keys.contains_key(&VirtualKey::RightAlt));
        assert!(state
            .reconciled_released_keys
            .contains(&VirtualKey::RightAlt));
        assert!(!state.should_swallow_key(&KeyEvent::new(VirtualKey::RightAlt, true)));
    }

    #[test]
    fn held_key_insert_on_down_remove_on_up() {
        let mut state = state_with_bound_key(VirtualKey::E);
        state.route_key_event(KeyEvent::new(VirtualKey::E, true), Some(Action::MoveUp));
        assert!(state.held_physical_keys.contains_key(&VirtualKey::E));
        state.route_key_event(KeyEvent::new(VirtualKey::E, false), None);
        assert!(!state.held_physical_keys.contains_key(&VirtualKey::E));
    }

    #[test]
    fn reconcile_releases_only_physically_up_modifier_keys() {
        let mut state = AppState::default();
        state.set_bound_keys([VirtualKey::LeftShift, VirtualKey::RightAlt]);
        state.route_key_event(
            KeyEvent::new(VirtualKey::LeftShift, true),
            Some(Action::SlowMouse),
        );
        state.route_key_event(
            KeyEvent::new(VirtualKey::RightAlt, true),
            Some(Action::MoveDown),
        );
        let _ = collect_commands(&mut state);

        state.reconcile_stale_keys(Duration::ZERO, |key| key == VirtualKey::RightAlt);

        let cmds = collect_commands(&mut state);
        assert_eq!(cmds.len(), 1);
        assert_eq!(
            cmds[0],
            AppCommand::KeyAction {
                action: Action::SlowMouse,
                is_down: false,
            }
        );
        assert!(state.held_physical_keys.contains_key(&VirtualKey::RightAlt));
        assert!(!state
            .held_physical_keys
            .contains_key(&VirtualKey::LeftShift));
        assert!(!state
            .reconciled_released_keys
            .contains(&VirtualKey::RightAlt));
        assert!(state
            .reconciled_released_keys
            .contains(&VirtualKey::LeftShift));
        assert!(!state.should_swallow_key(&KeyEvent::new(VirtualKey::LeftShift, false)));
    }

    #[test]
    fn reconcile_ignores_non_modifier_movement_keys_entirely() {
        let mut state = state_with_bound_key(VirtualKey::E);
        state.route_key_event(KeyEvent::new(VirtualKey::E, true), Some(Action::MoveUp));
        let _ = collect_commands(&mut state);

        state.reconcile_stale_keys(Duration::ZERO, |_| {
            panic!("non-modifier keys should not be physically queried")
        });

        assert_eq!(collect_commands(&mut state), Vec::new());
        assert!(state.held_physical_keys.contains_key(&VirtualKey::E));
        assert!(!state.reconciled_released_keys.contains(&VirtualKey::E));
        assert!(state.should_swallow_key(&KeyEvent::new(VirtualKey::E, false)));
    }

    #[test]
    fn reconcile_removes_active_trigger_for_stale_modifier_key() {
        let mut state = state_with_bound_key(VirtualKey::LeftShift);
        state.route_key_event(
            KeyEvent::new(VirtualKey::LeftShift, true),
            Some(Action::SlowMouse),
        );
        assert_eq!(collect_commands(&mut state).len(), 1);
        let up = KeyEvent::new(VirtualKey::LeftShift, false);
        assert!(state.should_swallow_key(&up));

        state.reconcile_stale_keys(Duration::ZERO, |_| false);

        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::KeyAction {
                action: Action::SlowMouse,
                is_down: false,
            }]
        );
        assert!(!state
            .held_physical_keys
            .contains_key(&VirtualKey::LeftShift));
        assert!(state
            .reconciled_released_keys
            .contains(&VirtualKey::LeftShift));
        assert!(!state.should_swallow_key(&up));
    }

    #[test]
    fn bug_preserved_shortcut_passes_through_without_explicit_binding_but_binding_precedes_filter()
    {
        let mut unbound = state_with_bound_key(VirtualKey::S);
        let mut ctrl_s = KeyEvent::new(VirtualKey::S, true);
        ctrl_s.ctrl_down = true;
        ctrl_s.left_ctrl_down = true;

        assert!(!unbound.should_swallow_key(&ctrl_s));
        unbound.route_key_event(ctrl_s, Some(Action::MoveUp));
        assert_eq!(collect_commands(&mut unbound), Vec::new());

        let mut bound = state_with_bound_chords([KeyChord::parse("Ctrl+S").unwrap()]);
        assert!(bound.should_swallow_key(&ctrl_s));
        bound.route_key_event(ctrl_s, Some(Action::MoveUp));
        assert_eq!(
            collect_commands(&mut bound),
            vec![AppCommand::KeyAction {
                action: Action::MoveUp,
                is_down: true,
            }]
        );
    }

    #[test]
    fn bug_altgr_preserved_shortcut_normalizes_synthetic_ctrl_before_precedence() {
        let mut synthetic_ctrl_s = KeyEvent::new(VirtualKey::S, true);
        synthetic_ctrl_s.alt_down = true;
        synthetic_ctrl_s.right_alt_down = true;
        synthetic_ctrl_s.ctrl_down = true;

        let unbound = state_with_bound_key(VirtualKey::S);
        assert!(!unbound.should_swallow_key(&synthetic_ctrl_s));

        let bound = state_with_bound_chords([KeyChord::parse("RightAlt+S").unwrap()]);
        assert!(bound.should_swallow_key(&synthetic_ctrl_s));
        assert!(bound.has_explicit_configured_binding_for_event(&synthetic_ctrl_s));
    }

    #[test]
    fn bug_panic_reset_precedes_preserved_shortcuts_and_clears_runtime_state() {
        let mut state = state_with_bound_key(VirtualKey::S);
        state.route_key_event(KeyEvent::new(VirtualKey::S, true), Some(Action::MoveUp));
        assert_eq!(collect_commands(&mut state).len(), 1);

        let mut panic = KeyEvent::new(VirtualKey::Escape, true);
        panic.alt_down = true;
        panic.right_alt_down = true;
        panic.ctrl_down = true;
        panic.left_ctrl_down = true;
        panic.shift_down = true;
        panic.left_shift_down = true;

        assert!(state.should_swallow_key(&panic));
        state.route_key_event(panic, None);

        assert_eq!(
            collect_commands(&mut state),
            vec![
                AppCommand::SetActiveMode { active: false },
                AppCommand::PanicReset,
            ]
        );
    }

    #[test]
    fn bug_alt_stuck_reconcile_interval_altgr_sequence_recovers_missed_right_alt_up() {
        let mut state = state_with_bound_chords([KeyChord::parse("RightAlt+E").unwrap()]);
        state.set_owned_modifiers([VirtualKey::RightAlt]);

        state.route_key_event(KeyEvent::new(VirtualKey::RightAlt, true), None);
        assert!(state.should_swallow_key(&KeyEvent::new(VirtualKey::RightAlt, true)));

        let mut e_down = right_alt_e_with_synthetic_ctrl();
        assert!(state.should_swallow_key(&e_down));
        state.route_key_event(e_down, Some(Action::MoveRight));
        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::KeyAction {
                action: Action::MoveRight,
                is_down: true,
            }]
        );

        e_down.is_down = false;
        state.route_key_event(e_down, None);
        assert_eq!(
            collect_commands(&mut state),
            vec![AppCommand::KeyAction {
                action: Action::MoveRight,
                is_down: false,
            }]
        );

        assert!(state.should_swallow_key(&KeyEvent::new(VirtualKey::RightAlt, true)));
        state.reconcile_stale_keys(Duration::ZERO, |key| key != VirtualKey::RightAlt);
        assert!(!state.should_swallow_key(&KeyEvent::new(VirtualKey::RightAlt, true)));
        assert_eq!(collect_commands(&mut state), Vec::new());
    }

    #[test]
    fn bug_altgr_active_trigger_uses_right_alt_specific_chord_for_key_up() {
        let mut state = state_with_bound_chords([
            KeyChord::parse("Alt+E").unwrap(),
            KeyChord::parse("RightAlt+E").unwrap(),
        ]);
        let e_down = right_alt_e_with_synthetic_ctrl();

        state.route_key_event(e_down, Some(Action::MoveUp));
        assert!(state
            .active_trigger_chords
            .contains(&KeyChord::parse("RightAlt+E").unwrap()));
        assert!(!state
            .active_trigger_chords
            .contains(&KeyChord::parse("Alt+E").unwrap()));

        let mut e_up = e_down;
        e_up.is_down = false;
        assert!(state.should_swallow_key(&e_up));
        state.route_key_event(e_up, None);
        assert_eq!(
            collect_commands(&mut state),
            vec![
                AppCommand::KeyAction {
                    action: Action::MoveUp,
                    is_down: true,
                },
                AppCommand::KeyAction {
                    action: Action::MoveUp,
                    is_down: false,
                },
            ]
        );
    }
}
