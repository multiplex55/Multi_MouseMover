use crate::action::Action;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndicatorState {
    Hidden,
    ActiveNormal,
    ActiveSlow,
    DraggingLeft,
    JumpMode,
    MouseSpeedSlow,
    MouseSpeedNormal,
    MouseSpeedFast,
    WheelScrollingSlow,
    WheelScrollingNormal,
    WheelScrollingFast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndicatorFlashReason {
    None,
    MouseSpeed,
    WheelSpeed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndicatorSnapshot {
    pub state: IndicatorState,
    pub app_active: bool,
    pub dragging_left: bool,
    pub slow: bool,
    pub surgical: bool,
    pub jump_active: bool,
    pub jump_stage: Option<(usize, usize)>,
    pub mouse_speed: i32,
    pub default_mouse_speed: i32,
    pub wheel_speed: i32,
    pub default_wheel_speed: i32,
    pub flash_reason: IndicatorFlashReason,
    pub final_adjust_active: bool,
    pub bookmark_mode_active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WheelIndicatorInput {
    pub active: bool,
    pub current_speed: i32,
    pub default_speed: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseIndicatorInput {
    pub active: bool,
    pub current_speed: i32,
    pub default_speed: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndicatorInput<'a> {
    pub app_active: bool,
    pub jump_active: bool,
    pub jump_stage: Option<(usize, usize)>,
    pub final_adjust_active: bool,
    pub active_actions: &'a HashSet<Action>,
    pub mouse: MouseIndicatorInput,
    pub wheel: WheelIndicatorInput,
    pub left_button_held: bool,
    pub bookmark_mode_active: bool,
}

pub fn resolve_indicator_snapshot(input: IndicatorInput<'_>) -> IndicatorSnapshot {
    let slow = input.active_actions.contains(&Action::SlowMouse);
    let surgical = input.active_actions.contains(&Action::SurgicalMode);
    let wheel_direction_active = input
        .active_actions
        .iter()
        .any(|action| action.is_wheel_direction());
    let flash_reason = if input.wheel.active && !wheel_direction_active {
        IndicatorFlashReason::WheelSpeed
    } else if input.mouse.active {
        IndicatorFlashReason::MouseSpeed
    } else {
        IndicatorFlashReason::None
    };

    let state = resolve_legacy_state(&input, wheel_direction_active);

    IndicatorSnapshot {
        state,
        app_active: input.app_active,
        dragging_left: input.left_button_held,
        slow,
        surgical,
        jump_active: input.jump_active,
        jump_stage: input.jump_stage,
        mouse_speed: input.mouse.current_speed,
        default_mouse_speed: input.mouse.default_speed,
        wheel_speed: input.wheel.current_speed,
        default_wheel_speed: input.wheel.default_speed,
        flash_reason,
        final_adjust_active: input.final_adjust_active,
        bookmark_mode_active: input.bookmark_mode_active,
    }
}

fn resolve_legacy_state(
    input: &IndicatorInput<'_>,
    wheel_direction_active: bool,
) -> IndicatorState {
    if !input.app_active {
        return IndicatorState::Hidden;
    }

    if input.final_adjust_active || input.jump_active {
        return IndicatorState::JumpMode;
    }

    if input.left_button_held {
        return IndicatorState::DraggingLeft;
    }

    if input.wheel.active || wheel_direction_active {
        return wheel_state(input.wheel.current_speed, input.wheel.default_speed);
    }

    if input.mouse.active {
        return mouse_state(input.mouse.current_speed, input.mouse.default_speed);
    }

    if input.active_actions.contains(&Action::SlowMouse) {
        return IndicatorState::ActiveSlow;
    }

    IndicatorState::ActiveNormal
}

fn mouse_state(current_speed: i32, default_speed: i32) -> IndicatorState {
    if current_speed < default_speed {
        IndicatorState::MouseSpeedSlow
    } else if current_speed > default_speed {
        IndicatorState::MouseSpeedFast
    } else {
        IndicatorState::MouseSpeedNormal
    }
}

fn wheel_state(current_speed: i32, default_speed: i32) -> IndicatorState {
    if current_speed < default_speed {
        IndicatorState::WheelScrollingSlow
    } else if current_speed > default_speed {
        IndicatorState::WheelScrollingFast
    } else {
        IndicatorState::WheelScrollingNormal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actions(actions: &[Action]) -> HashSet<Action> {
        actions.iter().cloned().collect()
    }

    fn resolve(
        app_active: bool,
        jump_active: bool,
        active_actions: HashSet<Action>,
        wheel_active: bool,
        current_wheel_speed: i32,
        default_wheel_speed: i32,
        mouse_active: bool,
        current_mouse_speed: i32,
        default_mouse_speed: i32,
        left_button_held: bool,
    ) -> IndicatorState {
        resolve_indicator_snapshot(IndicatorInput {
            app_active,
            jump_active,
            jump_stage: jump_active.then_some((1, 1)),
            final_adjust_active: false,
            active_actions: &active_actions,
            mouse: MouseIndicatorInput {
                active: mouse_active,
                current_speed: current_mouse_speed,
                default_speed: default_mouse_speed,
            },
            wheel: WheelIndicatorInput {
                active: wheel_active,
                current_speed: current_wheel_speed,
                default_speed: default_wheel_speed,
            },
            left_button_held,
        })
        .state
    }

    #[test]
    fn disabled_mode_is_hidden_even_with_conflicting_inputs() {
        assert_eq!(
            resolve(
                false,
                true,
                actions(&[Action::SlowMouse, Action::WheelDown]),
                true,
                12,
                3,
                true,
                5,
                3,
                true,
            ),
            IndicatorState::Hidden
        );
    }

    #[test]
    fn jump_mode_takes_priority_over_wheel_drag_and_slow() {
        assert_eq!(
            resolve(
                true,
                true,
                actions(&[Action::SlowMouse, Action::WheelDown]),
                true,
                1,
                3,
                true,
                5,
                3,
                true,
            ),
            IndicatorState::JumpMode
        );
    }

    #[test]
    fn jump_mode_overrides_normal_active_indicator_state() {
        assert_eq!(
            resolve(true, true, HashSet::new(), false, 3, 3, false, 3, 3, false),
            IndicatorState::JumpMode
        );
    }

    #[test]
    fn final_adjust_takes_priority_over_drag_wheel_and_slow() {
        let active_actions = actions(&[Action::SlowMouse, Action::WheelDown]);
        let snapshot = resolve_indicator_snapshot(IndicatorInput {
            app_active: true,
            jump_active: true,
            jump_stage: Some((3, 3)),
            final_adjust_active: true,
            active_actions: &active_actions,
            mouse: MouseIndicatorInput {
                active: true,
                current_speed: 5,
                default_speed: 3,
            },
            wheel: WheelIndicatorInput {
                active: true,
                current_speed: 1,
                default_speed: 3,
            },
            left_button_held: true,
            bookmark_mode_active: false,
        });

        assert_eq!(snapshot.state, IndicatorState::JumpMode);
        assert!(snapshot.final_adjust_active);
        assert_eq!(snapshot.jump_stage, Some((3, 3)));
    }

    #[test]
    fn surgical_action_sets_snapshot_flag() {
        let active_actions = actions(&[Action::SurgicalMode]);
        let snapshot = resolve_indicator_snapshot(IndicatorInput {
            app_active: true,
            jump_active: false,
            jump_stage: None,
            final_adjust_active: false,
            active_actions: &active_actions,
            mouse: MouseIndicatorInput {
                active: false,
                current_speed: 1,
                default_speed: 1,
            },
            wheel: WheelIndicatorInput {
                active: false,
                current_speed: 1,
                default_speed: 1,
            },
            left_button_held: false,
            bookmark_mode_active: false,
        });
        assert!(snapshot.surgical);
    }

    #[test]
    fn dragging_left_takes_priority_over_wheel_and_slow() {
        assert_eq!(
            resolve(
                true,
                false,
                actions(&[Action::SlowMouse, Action::WheelDown]),
                true,
                1,
                3,
                true,
                5,
                3,
                true,
            ),
            IndicatorState::DraggingLeft
        );
    }

    #[test]
    fn wheel_takes_priority_over_slow_and_classifies_speed() {
        assert_eq!(
            resolve(
                true,
                false,
                actions(&[Action::SlowMouse]),
                true,
                1,
                3,
                false,
                3,
                3,
                false
            ),
            IndicatorState::WheelScrollingSlow
        );
        assert_eq!(
            resolve(
                true,
                false,
                actions(&[Action::WheelDown]),
                false,
                3,
                3,
                false,
                3,
                3,
                false,
            ),
            IndicatorState::WheelScrollingNormal
        );
        assert_eq!(
            resolve(
                true,
                false,
                actions(&[Action::WheelDown]),
                false,
                5,
                3,
                false,
                3,
                3,
                false,
            ),
            IndicatorState::WheelScrollingFast
        );
    }

    #[test]
    fn slow_action_takes_priority_over_default_active() {
        assert_eq!(
            resolve(
                true,
                false,
                actions(&[Action::SlowMouse]),
                false,
                3,
                3,
                false,
                3,
                3,
                false,
            ),
            IndicatorState::ActiveSlow
        );
    }

    #[test]
    fn active_without_more_specific_state_is_normal() {
        assert_eq!(
            resolve(true, false, HashSet::new(), false, 3, 3, false, 3, 3, false),
            IndicatorState::ActiveNormal
        );
    }

    #[test]
    fn mouse_speed_flash_classifies_selected_tier() {
        assert_eq!(
            resolve(true, false, HashSet::new(), false, 3, 3, true, 1, 3, false,),
            IndicatorState::MouseSpeedSlow
        );
        assert_eq!(
            resolve(true, false, HashSet::new(), false, 3, 3, true, 3, 3, false,),
            IndicatorState::MouseSpeedNormal
        );
        assert_eq!(
            resolve(true, false, HashSet::new(), false, 3, 3, true, 5, 3, false,),
            IndicatorState::MouseSpeedFast
        );
    }
}
