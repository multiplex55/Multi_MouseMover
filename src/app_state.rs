use crate::action::Action;
use crate::action_handler::{final_adjust_control_for_event, FinalAdjustControl};
use crate::jump_session::{
    expand_region_within, JumpRegion, JumpSession, JumpSessionUpdate, JumpStage,
};
use crate::jump_view::{FinalAdjustOverlayView, JumpOverlayView, JumpStageMetadata};
use crate::key_chord::{KeyChord, RuntimeSystemBindings};
use crate::keyboard::VirtualKey;
use crate::{Config, JumpConfig};
use std::collections::{HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: VirtualKey,
    pub is_down: bool,
    pub alt_down: bool,
    pub right_alt_down: bool,
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
            right_alt_down: false,
            ctrl_down: false,
            shift_down: false,
            win_down: false,
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
    EnterJumpMode {
        activation_key: VirtualKey,
        profile: Option<String>,
    },
    KeyAction {
        action: Action,
        is_down: bool,
    },
    JumpInput(KeyEvent, Option<Action>),
}

#[derive(Debug)]
pub struct AppState {
    key_events: VecDeque<KeyEvent>,
    commands: VecDeque<AppCommand>,
    bound_chords: HashSet<KeyChord>,
    active_keys: HashSet<VirtualKey>,
    active_trigger_chords: HashSet<KeyChord>,
    jump: JumpState,
    active_mode: bool,
    preserve_global_shortcuts: bool,
    system_bindings: RuntimeSystemBindings,
}

#[derive(Debug)]
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

#[derive(Debug, Clone, PartialEq)]
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
            jump: JumpState::Inactive,
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
        self.bound_chords = keys.into_iter().map(KeyChord::from_key).collect();
    }

    pub fn set_bound_chords<I>(&mut self, chords: I)
    where
        I: IntoIterator<Item = KeyChord>,
    {
        self.bound_chords = chords.into_iter().collect();
    }

    pub fn set_active_mode(&mut self, active_mode: bool) {
        self.active_mode = active_mode;
        if !active_mode {
            self.clear_active_action_keys();
        }
    }

    pub fn active_mode(&self) -> bool {
        self.active_mode
    }

    pub fn has_active_action_keys(&self) -> bool {
        !self.active_keys.is_empty() || !self.active_trigger_chords.is_empty()
    }

    pub fn clear_active_action_keys(&mut self) {
        self.active_keys.clear();
        self.active_trigger_chords.clear();
    }

    pub fn clear_active_action_keys_and_exit_jump_mode(&mut self) {
        self.clear_active_action_keys();
        self.exit_jump_mode();
    }

    pub fn set_system_bindings(&mut self, system_bindings: RuntimeSystemBindings) {
        self.system_bindings = system_bindings;
    }

    pub fn is_toggle_active_binding(&self, event: &KeyEvent) -> bool {
        self.system_bindings.toggle_active.matches_event(event)
    }

    pub fn is_exit_binding(&self, event: &KeyEvent) -> bool {
        self.system_bindings.exit.matches_event(event)
    }

    pub fn is_system_binding(&self, event: &KeyEvent) -> bool {
        self.is_toggle_active_binding(event) || self.is_exit_binding(event)
    }

    pub fn is_toggle_active_key_down_event(&self, event: &KeyEvent) -> bool {
        event.is_down && self.is_toggle_active_binding(event)
    }

    pub fn is_exit_key_down_event(&self, event: &KeyEvent) -> bool {
        event.is_down && self.is_exit_binding(event)
    }

    pub fn is_system_key_down_event(&self, event: &KeyEvent) -> bool {
        self.is_toggle_active_key_down_event(event) || self.is_exit_key_down_event(event)
    }

    pub fn is_jump_active(&self) -> bool {
        matches!(self.jump, JumpState::Active { .. })
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
                JumpStage::with_target_region_mode(
                    stage.grid_size.0,
                    stage.grid_size.1,
                    stage.aim_point,
                    stage.aim_offset_x_px,
                    stage.aim_offset_y_px,
                    stage.target_margin_percent,
                    stage.visual_context_margin_percent,
                    stage.target_region_mode,
                )
            })
            .collect();

        let Some(session) = JumpSession::new(virtual_screen_region, stages) else {
            self.exit_jump_mode();
            return false;
        };

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
            return match final_adjust_control_for_event(&event, action, final_adjust_config) {
                Some(FinalAdjustControl::Nudge { dx, dy }) => session.nudge_final_adjust(dx, dy),
                Some(FinalAdjustControl::Confirm) => session.confirm_final_adjust(),
                Some(FinalAdjustControl::Cancel) => session.cancel_final_adjust(),
                Some(FinalAdjustControl::Back) => session.back_from_final_adjust(),
                None => Some(JumpSessionUpdate::Consumed),
            };
        }

        let update = session.handle_key(event.key, event.is_down);
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

        let visual_context_margin_percent = stage_metadata
            .get(session.stage_index)
            .map(|stage| stage.visual_context_margin_percent)
            .unwrap_or(0);
        let session_region = session.current_region;
        let target_region = session.current_region;
        let preview_source_region = expand_region_within(
            target_region,
            visual_context_margin_percent,
            session.session_region,
        )
        .unwrap_or(target_region);
        let client_draw_region = JumpRegion {
            left: 0,
            top: 0,
            width: session.session_region.width,
            height: session.session_region.height,
        };

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
            }),
        })
    }

    pub fn resolve_jump_overlay(&self) -> JumpOverlayResolution {
        self.jump_view()
            .map(JumpOverlayResolution::Visible)
            .unwrap_or(JumpOverlayResolution::Hidden)
    }

    pub fn exit_jump_mode(&mut self) {
        self.jump = JumpState::Inactive;
    }

    pub fn should_swallow_key(&self, event: &KeyEvent) -> bool {
        if self.is_jump_active() {
            return true;
        }

        if self.is_system_binding(event) {
            return true;
        }

        if self.is_preserved_shortcut(event) {
            return false;
        }

        self.active_mode
            && (self.bound_chords.iter().any(|chord| {
                chord.matches_dispatch_event(event)
                    || (!event.is_down && self.active_trigger_chords.contains(chord))
            }) || (!event.is_down
                && self
                    .active_trigger_chords
                    .iter()
                    .any(|chord| chord.key == event.key)))
    }

    pub fn route_key_event(&mut self, event: KeyEvent, action: Option<Action>) {
        if self.is_jump_active() && self.is_toggle_active_key_down_event(&event) {
            self.enqueue_command(AppCommand::ToggleActiveMode);
            return;
        }

        if self.is_jump_active() {
            if self.is_activation_key_event(&event) {
                return;
            }

            self.enqueue_command(AppCommand::JumpInput(event, action));
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

        if self.is_preserved_shortcut(&event) {
            return;
        }

        if !self.active_mode {
            return;
        }

        if event.is_down {
            self.active_keys.insert(event.key);
            if action.is_some() {
                self.track_active_trigger_chord(event);
            }
        } else {
            self.active_keys.remove(&event.key);
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

        for command in self.commands_for_active_keys(event, action) {
            self.enqueue_command(command);
        }

        if !event.is_down {
            self.active_trigger_chords
                .retain(|chord| chord.key != event.key);
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

    fn track_active_trigger_chord(&mut self, event: KeyEvent) {
        if let Some(chord) = self
            .bound_chords
            .iter()
            .filter(|chord| chord.matches_key_down_event(&event))
            .max_by_key(|chord| chord.specificity())
            .copied()
        {
            self.active_trigger_chords.insert(chord);
        }
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

fn jump_stage_metadata(config: &JumpConfig) -> Vec<JumpStageMetadata> {
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
        });
    }

    stages
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicator::{
        resolve_indicator_state, IndicatorInput, IndicatorState, WheelIndicatorInput,
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

    fn final_adjust_config(enabled: bool) -> Config {
        let mut config = Config::default();
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
        let indicator = resolve_indicator_state(IndicatorInput {
            app_active: state.active_mode(),
            jump_active: state.is_jump_active(),
            active_actions: &active_actions,
            wheel: WheelIndicatorInput {
                active: true,
                current_speed: 12,
                default_speed: 3,
            },
            left_button_held: true,
        });

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
        assert_eq!(view.stage_count, 1);
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
        assert_eq!(view.stages.len(), 1);
        assert_eq!(
            view.stages[0].target_region_mode,
            crate::JumpTargetRegionMode::ExactRegion
        );
        assert_eq!(view.stages[0].target_margin_percent, 0);
        assert_eq!(view.stages[0].visual_context_margin_percent, 0);
        assert_eq!(view.stages[0].zoom_scale, 1.0);
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
    fn jump_view_exposes_distinct_context_region_without_expanding_target() {
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
            state.handle_jump_input(KeyEvent::new(VirtualKey::B, true), None),
            Some(JumpSessionUpdate::Consumed)
        );
        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::B, true), None),
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
        assert_eq!(view.session_region, view.target_region);
        assert_ne!(view.preview_source_region, view.target_region);
        assert_eq!(
            view.preview_source_region,
            JumpRegion {
                left: 19,
                top: 19,
                width: 22,
                height: 22,
            }
        );
    }

    #[test]
    fn jump_view_rehydrates_input_and_region_after_stage_backtrack() {
        let mut config = Config::default();
        config.jump.mode = crate::JumpMode::Precision;
        config.jump.coarse.width = 5;
        config.jump.coarse.height = 5;
        config.jump.fine.enabled = true;
        config.jump.fine.width = 5;
        config.jump.fine.height = 5;
        let config = config.normalize().unwrap();

        let mut state = AppState::default();
        assert!(state.enter_jump_mode(
            &config.jump,
            config.final_adjust.clone(),
            jump_region(),
            VirtualKey::J
        ));
        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::B, true), None),
            Some(JumpSessionUpdate::Consumed)
        );
        assert_eq!(
            state.handle_jump_input(KeyEvent::new(VirtualKey::B, true), None),
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
}
