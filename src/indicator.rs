use crate::action::Action;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndicatorState {
    Hidden,
    ActiveNormal,
    ActiveSlow,
    DraggingLeft,
    JumpMode,
    WheelScrollingSlow,
    WheelScrollingNormal,
    WheelScrollingFast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WheelIndicatorInput {
    pub active: bool,
    pub current_speed: i32,
    pub default_speed: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndicatorInput<'a> {
    pub app_active: bool,
    pub jump_active: bool,
    pub active_actions: &'a HashSet<Action>,
    pub wheel: WheelIndicatorInput,
    pub left_button_held: bool,
}

pub fn resolve_indicator_state(input: IndicatorInput<'_>) -> IndicatorState {
    if !input.app_active {
        return IndicatorState::Hidden;
    }

    if input.jump_active {
        return IndicatorState::JumpMode;
    }

    if input.left_button_held {
        return IndicatorState::DraggingLeft;
    }

    if input.wheel.active
        || input
            .active_actions
            .iter()
            .any(|action| action.is_wheel_direction())
    {
        return wheel_state(input.wheel.current_speed, input.wheel.default_speed);
    }

    if input.active_actions.contains(&Action::SlowMouse) {
        return IndicatorState::ActiveSlow;
    }

    IndicatorState::ActiveNormal
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
        left_button_held: bool,
    ) -> IndicatorState {
        resolve_indicator_state(IndicatorInput {
            app_active,
            jump_active,
            active_actions: &active_actions,
            wheel: WheelIndicatorInput {
                active: wheel_active,
                current_speed: current_wheel_speed,
                default_speed: default_wheel_speed,
            },
            left_button_held,
        })
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
            ),
            IndicatorState::JumpMode
        );
    }

    #[test]
    fn jump_mode_overrides_normal_active_indicator_state() {
        assert_eq!(
            resolve(true, true, HashSet::new(), false, 3, 3, false),
            IndicatorState::JumpMode
        );
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
            ),
            IndicatorState::ActiveSlow
        );
    }

    #[test]
    fn active_without_more_specific_state_is_normal() {
        assert_eq!(
            resolve(true, false, HashSet::new(), false, 3, 3, false),
            IndicatorState::ActiveNormal
        );
    }
}
