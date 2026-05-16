use crate::action::Action;
use crate::key_chord::RuntimeSystemBindings;
use crate::keyboard::VirtualKey;
use std::collections::{HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: VirtualKey,
    pub is_down: bool,
    pub alt_down: bool,
    pub ctrl_down: bool,
    pub shift_down: bool,
    pub win_down: bool,
}

impl KeyEvent {
    #[cfg(test)]
    pub fn new(key: VirtualKey, is_down: bool) -> Self {
        Self {
            key,
            is_down,
            alt_down: false,
            ctrl_down: false,
            shift_down: false,
            win_down: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCommand {
    ToggleActiveMode,
    Exit,
    EnterJumpMode { activation_key: VirtualKey },
    KeyAction { action: Action, is_down: bool },
    JumpInput(KeyEvent),
}

#[derive(Debug)]
pub struct AppState {
    key_events: VecDeque<KeyEvent>,
    commands: VecDeque<AppCommand>,
    bound_keys: HashSet<VirtualKey>,
    active_keys: HashSet<VirtualKey>,
    jump_active: bool,
    activation_key: Option<VirtualKey>,
    activation_key_released: bool,
    active_mode: bool,
    preserve_global_shortcuts: bool,
    system_bindings: RuntimeSystemBindings,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            key_events: VecDeque::new(),
            commands: VecDeque::new(),
            bound_keys: HashSet::new(),
            active_keys: HashSet::new(),
            jump_active: false,
            activation_key: None,
            activation_key_released: true,
            active_mode: true,
            preserve_global_shortcuts: true,
            system_bindings: RuntimeSystemBindings::default(),
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

    pub fn set_bound_keys<I>(&mut self, keys: I)
    where
        I: IntoIterator<Item = VirtualKey>,
    {
        self.bound_keys = keys.into_iter().collect();
    }

    pub fn set_active_mode(&mut self, active_mode: bool) {
        self.active_mode = active_mode;
        if !active_mode {
            self.active_keys.clear();
        }
    }

    pub fn set_system_bindings(&mut self, system_bindings: RuntimeSystemBindings) {
        self.system_bindings = system_bindings;
    }

    pub fn is_toggle_active_key_down_event(&self, event: &KeyEvent) -> bool {
        event.is_down && self.system_bindings.toggle_active.matches_event(event)
    }

    pub fn is_exit_key_down_event(&self, event: &KeyEvent) -> bool {
        event.is_down && self.system_bindings.exit.matches_event(event)
    }

    pub fn is_system_key_down_event(&self, event: &KeyEvent) -> bool {
        self.is_toggle_active_key_down_event(event) || self.is_exit_key_down_event(event)
    }

    #[cfg(test)]
    pub fn set_preserve_global_shortcuts(&mut self, preserve: bool) {
        self.preserve_global_shortcuts = preserve;
    }

    pub fn enter_jump_mode(&mut self, activation_key: VirtualKey) {
        self.jump_active = true;
        self.activation_key = Some(activation_key);
        self.activation_key_released = false;
    }

    pub fn exit_jump_mode(&mut self) {
        self.jump_active = false;
        self.activation_key = None;
        self.activation_key_released = true;
    }

    pub fn should_swallow_key(&self, event: &KeyEvent) -> bool {
        if self.is_preserved_shortcut(event) {
            return false;
        }

        if self.jump_active {
            return true;
        }

        if self.is_system_key_down_event(event) {
            return true;
        }

        self.active_mode && self.bound_keys.contains(&event.key)
    }

    pub fn route_key_event(&mut self, event: KeyEvent, action: Option<Action>) {
        if self.is_preserved_shortcut(&event) {
            return;
        }

        if self.jump_active {
            if self.is_activation_key_event(&event) {
                return;
            }

            self.enqueue_command(AppCommand::JumpInput(event));
            return;
        }

        if self.is_toggle_active_key_down_event(&event) {
            self.enqueue_command(AppCommand::ToggleActiveMode);
            return;
        }

        if self.is_exit_key_down_event(&event) {
            self.enqueue_command(AppCommand::Exit);
            return;
        }

        if !self.active_mode {
            return;
        }

        if event.is_down {
            self.active_keys.insert(event.key);
        } else {
            self.active_keys.remove(&event.key);
        }

        if event.is_down && action == Some(Action::JumpMode) {
            self.enqueue_command(AppCommand::EnterJumpMode {
                activation_key: event.key,
            });
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
            commands.push(AppCommand::KeyAction {
                action,
                is_down: event.is_down,
            });
        }

        commands
    }

    fn is_activation_key_event(&mut self, event: &KeyEvent) -> bool {
        if Some(event.key) != self.activation_key {
            return false;
        }

        if event.is_down && !self.activation_key_released {
            return true;
        }

        if !event.is_down {
            self.activation_key_released = true;
            return true;
        }

        false
    }

    fn is_preserved_shortcut(&self, event: &KeyEvent) -> bool {
        if !self.preserve_global_shortcuts {
            return false;
        }

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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with_bound_key(key: VirtualKey) -> AppState {
        let mut state = AppState::default();
        state.set_bound_keys([key]);
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
        state.enter_jump_mode(VirtualKey::J);

        assert!(state.should_swallow_key(&KeyEvent::new(VirtualKey::A, true)));
    }

    #[test]
    fn global_shortcuts_not_swallowed_unless_configured() {
        let mut state = AppState::default();
        state.enter_jump_mode(VirtualKey::J);
        let mut ctrl_w = KeyEvent::new(VirtualKey::W, true);
        ctrl_w.ctrl_down = true;

        assert!(!state.should_swallow_key(&ctrl_w));

        state.set_preserve_global_shortcuts(false);
        assert!(state.should_swallow_key(&ctrl_w));
    }

    #[test]
    fn jump_binding_routes_to_enter_command() {
        let mut state = state_with_bound_key(VirtualKey::J);
        let event = KeyEvent::new(VirtualKey::J, true);

        state.route_key_event(event, Some(Action::JumpMode));

        assert_eq!(
            state.pop_command(),
            Some(AppCommand::EnterJumpMode {
                activation_key: VirtualKey::J
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
}
