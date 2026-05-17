use crate::{
    jump_grid::{code_to_index, expected_len, letters_needed},
    keyboard::VirtualKey,
    JumpTargetRegionMode,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JumpRegion {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

impl JumpRegion {
    pub fn center(self) -> (i32, i32) {
        (
            (self.left as f64 + self.width as f64 / 2.0).round() as i32,
            (self.top as f64 + self.height as f64 / 2.0).round() as i32,
        )
    }

    pub fn is_valid(self) -> bool {
        self.width > 0 && self.height > 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JumpStage {
    pub width: u32,
    pub height: u32,
    pub target_margin_percent: u8,
    pub visual_context_margin_percent: u8,
    pub target_region_mode: JumpTargetRegionMode,
}

impl JumpStage {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            target_margin_percent: 0,
            visual_context_margin_percent: 0,
            target_region_mode: JumpTargetRegionMode::ExactRegion,
        }
    }

    pub fn with_target_margin(width: u32, height: u32, target_margin_percent: u8) -> Self {
        Self {
            width,
            height,
            target_margin_percent,
            visual_context_margin_percent: 0,
            target_region_mode: JumpTargetRegionMode::ExpandedTarget,
        }
    }

    pub fn with_target_region_mode(
        width: u32,
        height: u32,
        target_margin_percent: u8,
        visual_context_margin_percent: u8,
        target_region_mode: JumpTargetRegionMode,
    ) -> Self {
        Self {
            width,
            height,
            target_margin_percent,
            visual_context_margin_percent,
            target_region_mode,
        }
    }

    pub fn grid(self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn expected_len(self) -> usize {
        expected_len(self.grid())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JumpSession {
    pub stages: Vec<JumpStage>,
    pub stage_index: usize,
    pub session_region: JumpRegion,
    pub current_region: JumpRegion,
    pub input: String,
    pub path: Vec<(usize, usize)>,
    pub region_history: Vec<JumpRegion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JumpSessionUpdate {
    Consumed,
    Invalid,
    Cancelled,
    StageAdvanced {
        stage_index: usize,
        region: JumpRegion,
    },
    Completed {
        x: i32,
        y: i32,
        region: JumpRegion,
    },
}

pub fn subdivide_region(
    region: JumpRegion,
    grid: (u32, u32),
    row: usize,
    col: usize,
) -> Option<JumpRegion> {
    let (cols, rows) = grid;
    if !region.is_valid() || cols == 0 || rows == 0 {
        return None;
    }
    if row >= rows as usize || col >= cols as usize {
        return None;
    }

    let left = region.left as f64;
    let top = region.top as f64;
    let right = (region.left + region.width) as f64;
    let bottom = (region.top + region.height) as f64;
    let width = right - left;
    let height = bottom - top;

    let child_left = (left + width * col as f64 / cols as f64).round() as i32;
    let child_right = (left + width * (col + 1) as f64 / cols as f64).round() as i32;
    let child_top = (top + height * row as f64 / rows as f64).round() as i32;
    let child_bottom = (top + height * (row + 1) as f64 / rows as f64).round() as i32;

    let child = JumpRegion {
        left: child_left,
        top: child_top,
        width: child_right - child_left,
        height: child_bottom - child_top,
    };
    child.is_valid().then_some(child)
}

pub fn expand_region_within(
    region: JumpRegion,
    margin_percent: u8,
    bounds: JumpRegion,
) -> Option<JumpRegion> {
    if !region.is_valid() || !bounds.is_valid() {
        return None;
    }

    let margin_x = region.width * margin_percent as i32 / 100;
    let margin_y = region.height * margin_percent as i32 / 100;
    let bounds_right = bounds.left + bounds.width;
    let bounds_bottom = bounds.top + bounds.height;
    let left = (region.left - margin_x).max(bounds.left);
    let top = (region.top - margin_y).max(bounds.top);
    let right = (region.left + region.width + margin_x).min(bounds_right);
    let bottom = (region.top + region.height + margin_y).min(bounds_bottom);
    let expanded = JumpRegion {
        left,
        top,
        width: right - left,
        height: bottom - top,
    };
    expanded.is_valid().then_some(expanded)
}

impl JumpSession {
    pub fn new(region: JumpRegion, stages: Vec<JumpStage>) -> Option<Self> {
        if !region.is_valid() || stages.is_empty() || stages.iter().any(|stage| !stage.is_valid()) {
            return None;
        }
        let current_region = target_region_for_stage(region, stages[0], region)?;

        Some(Self {
            stages,
            stage_index: 0,
            session_region: region,
            current_region,
            input: String::new(),
            path: Vec::new(),
            region_history: vec![current_region],
        })
    }

    pub fn current_stage(&self) -> JumpStage {
        self.stages[self.stage_index]
    }

    pub fn current_grid(&self) -> (u32, u32) {
        self.current_stage().grid()
    }

    pub fn handle_key(&mut self, key: VirtualKey, is_keydown: bool) -> Option<JumpSessionUpdate> {
        if !is_keydown {
            return None;
        }

        match key {
            VirtualKey::Escape => {
                self.input.clear();
                Some(JumpSessionUpdate::Cancelled)
            }
            VirtualKey::Backspace => {
                self.input.pop();
                Some(JumpSessionUpdate::Consumed)
            }
            _ => self.handle_character_key(key),
        }
    }

    fn handle_character_key(&mut self, key: VirtualKey) -> Option<JumpSessionUpdate> {
        let ch = key.to_char()?;
        if !ch.is_ascii_uppercase() {
            return None;
        }

        self.input.push(ch);
        if self.input.len() < self.current_stage().expected_len() {
            return Some(JumpSessionUpdate::Consumed);
        }

        Some(self.evaluate_input())
    }

    fn evaluate_input(&mut self) -> JumpSessionUpdate {
        let stage = self.current_stage();
        let row_len = letters_needed(stage.height);
        let col_len = letters_needed(stage.width);
        let row_code: String = self.input.chars().take(row_len).collect();
        let col_code: String = self.input.chars().skip(row_len).take(col_len).collect();
        self.input.clear();

        let Some(row) = code_to_index(&row_code) else {
            return JumpSessionUpdate::Invalid;
        };
        let Some(col) = code_to_index(&col_code) else {
            return JumpSessionUpdate::Invalid;
        };
        let Some(region) = subdivide_region(self.current_region, stage.grid(), row, col) else {
            return JumpSessionUpdate::Invalid;
        };

        self.path.push((row, col));

        if self.stage_index + 1 < self.stages.len() {
            self.stage_index += 1;
            let region =
                match target_region_for_stage(region, self.current_stage(), self.session_region) {
                    Some(region) => region,
                    None => return JumpSessionUpdate::Invalid,
                };
            self.current_region = region;
            self.region_history.push(region);
            JumpSessionUpdate::StageAdvanced {
                stage_index: self.stage_index,
                region,
            }
        } else {
            self.current_region = region;
            self.region_history.push(region);
            let (x, y) = region.center();
            JumpSessionUpdate::Completed { x, y, region }
        }
    }
}

fn target_region_for_stage(
    selected_region: JumpRegion,
    stage: JumpStage,
    session_region: JumpRegion,
) -> Option<JumpRegion> {
    match stage.target_region_mode {
        JumpTargetRegionMode::ExactRegion => Some(selected_region),
        JumpTargetRegionMode::RegionWithContext
        | JumpTargetRegionMode::ExpandedTarget
        | JumpTargetRegionMode::CursorCenteredZoom => {
            expand_region_within(selected_region, stage.target_margin_percent, session_region)
        }
    }
}

impl JumpStage {
    fn is_valid(self) -> bool {
        self.width > 0 && self.height > 0
    }
}

#[cfg(test)]
mod tests {
    use super::{
        expand_region_within, subdivide_region, JumpRegion, JumpSession, JumpSessionUpdate,
        JumpStage,
    };
    use crate::{keyboard::VirtualKey, JumpTargetRegionMode};

    fn base_region() -> JumpRegion {
        JumpRegion {
            left: 0,
            top: 0,
            width: 1000,
            height: 1000,
        }
    }

    fn rounded_region() -> JumpRegion {
        JumpRegion {
            left: 0,
            top: 0,
            width: 100,
            height: 90,
        }
    }

    fn staged_grids() -> Vec<JumpStage> {
        vec![
            JumpStage::new(10, 10),
            JumpStage::new(5, 5),
            JumpStage::new(2, 2),
        ]
    }

    fn staged_session() -> JumpSession {
        JumpSession::new(base_region(), staged_grids()).unwrap()
    }

    fn press(session: &mut JumpSession, key: VirtualKey) -> Option<JumpSessionUpdate> {
        session.handle_key(key, true)
    }

    fn enter_code(session: &mut JumpSession, keys: &[VirtualKey]) -> Option<JumpSessionUpdate> {
        let mut update = None;
        for &key in keys {
            update = press(session, key);
        }
        update
    }

    #[test]
    fn region_center_rounds_half_pixels() {
        assert_eq!(
            JumpRegion {
                left: -5,
                top: 10,
                width: 10,
                height: 11,
            }
            .center(),
            (0, 16)
        );
    }

    #[test]
    fn region_validity_requires_positive_area() {
        assert!(base_region().is_valid());
        assert!(!JumpRegion {
            width: 0,
            ..base_region()
        }
        .is_valid());
        assert!(!JumpRegion {
            height: -1,
            ..base_region()
        }
        .is_valid());
    }

    #[test]
    fn subdivide_region_uses_rounded_float_boundaries() {
        assert_eq!(
            subdivide_region(rounded_region(), (3, 2), 1, 2),
            Some(JumpRegion {
                left: 67,
                top: 45,
                width: 33,
                height: 45,
            })
        );
    }

    #[test]
    fn subdivide_region_splits_even_grid_uniformly() {
        let region = JumpRegion {
            left: 20,
            top: 40,
            width: 120,
            height: 80,
        };

        assert_eq!(
            subdivide_region(region, (4, 2), 0, 0),
            Some(JumpRegion {
                left: 20,
                top: 40,
                width: 30,
                height: 40,
            })
        );
        assert_eq!(
            subdivide_region(region, (4, 2), 1, 3),
            Some(JumpRegion {
                left: 110,
                top: 80,
                width: 30,
                height: 40,
            })
        );
    }

    #[test]
    fn subdivide_region_keeps_non_even_rounding_stable_across_full_span() {
        let region = JumpRegion {
            left: 10,
            top: 20,
            width: 101,
            height: 103,
        };

        let expected_cols = [
            JumpRegion {
                left: 10,
                top: 20,
                width: 14,
                height: 103,
            },
            JumpRegion {
                left: 24,
                top: 20,
                width: 15,
                height: 103,
            },
            JumpRegion {
                left: 39,
                top: 20,
                width: 14,
                height: 103,
            },
            JumpRegion {
                left: 53,
                top: 20,
                width: 15,
                height: 103,
            },
            JumpRegion {
                left: 68,
                top: 20,
                width: 14,
                height: 103,
            },
            JumpRegion {
                left: 82,
                top: 20,
                width: 15,
                height: 103,
            },
            JumpRegion {
                left: 97,
                top: 20,
                width: 14,
                height: 103,
            },
        ];

        let mut next_left = region.left;
        for (col, expected) in expected_cols.into_iter().enumerate() {
            let child = subdivide_region(region, (7, 1), 0, col).unwrap();
            assert_eq!(child, expected);
            assert_eq!(child.left, next_left);
            next_left += child.width;
        }
        assert_eq!(next_left, region.left + region.width);

        let mut next_top = region.top;
        for row in 0..9 {
            let child = subdivide_region(region, (1, 9), row, 0).unwrap();
            assert_eq!(child.top, next_top);
            assert!(matches!(child.height, 11 | 12));
            next_top += child.height;
        }
        assert_eq!(next_top, region.top + region.height);
    }

    #[test]
    fn subdivide_region_guarantees_minimum_positive_cell_size_when_possible() {
        let region = JumpRegion {
            left: 5,
            top: 7,
            width: 3,
            height: 2,
        };

        for row in 0..2 {
            for col in 0..3 {
                let child = subdivide_region(region, (3, 2), row, col).unwrap();
                assert!(child.width >= 1, "{child:?}");
                assert!(child.height >= 1, "{child:?}");
            }
        }
    }

    #[test]
    fn subdivide_region_handles_negative_virtual_origin() {
        let region = JumpRegion {
            left: -1920,
            top: 0,
            width: 3840,
            height: 1080,
        };

        let left_child = subdivide_region(region, (4, 3), 1, 0).unwrap();
        assert_eq!(
            left_child,
            JumpRegion {
                left: -1920,
                top: 360,
                width: 960,
                height: 360,
            }
        );
        assert_eq!(left_child.center(), (-1440, 540));

        let middle_left_child = subdivide_region(region, (4, 3), 0, 1).unwrap();
        assert_eq!(
            middle_left_child,
            JumpRegion {
                left: -960,
                top: 0,
                width: 960,
                height: 360,
            }
        );
        assert_eq!(middle_left_child.center(), (-480, 180));
    }

    #[test]
    fn subdivide_region_rejects_invalid_inputs() {
        assert_eq!(subdivide_region(base_region(), (0, 2), 0, 0), None);
        assert_eq!(subdivide_region(base_region(), (2, 0), 0, 0), None);
        assert_eq!(subdivide_region(base_region(), (2, 2), 2, 0), None);
        assert_eq!(subdivide_region(base_region(), (2, 2), 0, 2), None);
        assert_eq!(
            subdivide_region(
                JumpRegion {
                    width: 0,
                    ..base_region()
                },
                (2, 2),
                0,
                0
            ),
            None
        );
    }

    #[test]
    fn subdivide_region_rejects_each_out_of_range_row_and_column_edge() {
        assert_eq!(subdivide_region(base_region(), (3, 2), 2, 0), None);
        assert_eq!(subdivide_region(base_region(), (3, 2), 0, 3), None);
        assert_eq!(subdivide_region(base_region(), (3, 2), 2, 3), None);
    }

    #[test]
    fn expand_region_within_clamps_to_session_bounds() {
        assert_eq!(
            expand_region_within(
                JumpRegion {
                    left: 10,
                    top: 20,
                    width: 100,
                    height: 50,
                },
                10,
                JumpRegion {
                    left: 0,
                    top: 0,
                    width: 120,
                    height: 80,
                },
            ),
            Some(JumpRegion {
                left: 0,
                top: 15,
                width: 120,
                height: 60,
            })
        );
    }

    #[test]
    fn letters_are_consumed_until_single_stage_completes() {
        let mut session = JumpSession::new(base_region(), vec![JumpStage::new(10, 10)]).unwrap();

        assert_eq!(
            session.handle_key(VirtualKey::A, true),
            Some(JumpSessionUpdate::Consumed)
        );
        assert_eq!(
            session.handle_key(VirtualKey::J, true),
            Some(JumpSessionUpdate::Completed {
                x: 950,
                y: 50,
                region: JumpRegion {
                    left: 900,
                    top: 0,
                    width: 100,
                    height: 100,
                },
            })
        );
        assert_eq!(session.input, "");
        assert_eq!(session.path, vec![(0, 9)]);
    }

    #[test]
    fn invalid_code_clears_input_without_advancing() {
        let mut session = staged_session();
        assert_eq!(
            enter_code(&mut session, &[VirtualKey::B, VirtualKey::C]),
            Some(JumpSessionUpdate::StageAdvanced {
                stage_index: 1,
                region: JumpRegion {
                    left: 200,
                    top: 100,
                    width: 100,
                    height: 100,
                },
            })
        );

        assert_eq!(
            session.handle_key(VirtualKey::F, true),
            Some(JumpSessionUpdate::Consumed)
        );
        assert_eq!(
            session.handle_key(VirtualKey::A, true),
            Some(JumpSessionUpdate::Invalid)
        );
        assert_eq!(session.input, "");
        assert_eq!(session.stage_index, 1);
        assert_eq!(
            session.current_region,
            JumpRegion {
                left: 200,
                top: 100,
                width: 100,
                height: 100,
            }
        );
    }

    #[test]
    fn cancel_backspace_and_ignored_keys_are_deterministic() {
        let mut session = JumpSession::new(base_region(), vec![JumpStage::new(10, 10)]).unwrap();

        assert_eq!(session.handle_key(VirtualKey::Num1, true), None);
        assert_eq!(session.handle_key(VirtualKey::A, false), None);
        assert_eq!(
            session.handle_key(VirtualKey::A, true),
            Some(JumpSessionUpdate::Consumed)
        );
        assert_eq!(
            session.handle_key(VirtualKey::Backspace, true),
            Some(JumpSessionUpdate::Consumed)
        );
        assert_eq!(session.input, "");
        assert_eq!(
            session.handle_key(VirtualKey::Escape, true),
            Some(JumpSessionUpdate::Cancelled)
        );
    }

    #[test]
    fn multi_stage_session_advances_then_completes() {
        let mut session = staged_session();

        assert_eq!(
            enter_code(&mut session, &[VirtualKey::B, VirtualKey::C]),
            Some(JumpSessionUpdate::StageAdvanced {
                stage_index: 1,
                region: JumpRegion {
                    left: 200,
                    top: 100,
                    width: 100,
                    height: 100,
                },
            })
        );
        assert_eq!(session.current_grid(), (5, 5));

        assert_eq!(
            enter_code(&mut session, &[VirtualKey::D, VirtualKey::E]),
            Some(JumpSessionUpdate::StageAdvanced {
                stage_index: 2,
                region: JumpRegion {
                    left: 280,
                    top: 160,
                    width: 20,
                    height: 20,
                },
            })
        );
        assert_eq!(session.current_grid(), (2, 2));

        assert_eq!(
            enter_code(&mut session, &[VirtualKey::B, VirtualKey::B]),
            Some(JumpSessionUpdate::Completed {
                x: 295,
                y: 175,
                region: JumpRegion {
                    left: 290,
                    top: 170,
                    width: 10,
                    height: 10,
                },
            })
        );
        assert_eq!(session.input, "");
        assert_eq!(session.path, vec![(1, 2), (3, 4), (1, 1)]);
        assert_eq!(session.region_history.len(), 4);
    }

    #[test]
    fn subdivision_uses_target_region_state_independent_of_preview_context() {
        let stages = vec![
            JumpStage::new(5, 5),
            JumpStage::with_target_margin(5, 5, 10),
            JumpStage::new(2, 2),
        ];
        let mut session = JumpSession::new(base_region(), stages).unwrap();

        assert_eq!(
            enter_code(&mut session, &[VirtualKey::B, VirtualKey::B]),
            Some(JumpSessionUpdate::StageAdvanced {
                stage_index: 1,
                region: JumpRegion {
                    left: 180,
                    top: 180,
                    width: 240,
                    height: 240,
                },
            })
        );
        assert_eq!(session.current_region, session.region_history[1]);

        assert_eq!(
            enter_code(&mut session, &[VirtualKey::A, VirtualKey::A]),
            Some(JumpSessionUpdate::StageAdvanced {
                stage_index: 2,
                region: JumpRegion {
                    left: 180,
                    top: 180,
                    width: 48,
                    height: 48,
                },
            })
        );
    }

    #[test]
    fn visual_context_margin_does_not_expand_next_stage_landing_region() {
        let stages = vec![
            JumpStage::new(5, 5),
            JumpStage::with_target_region_mode(
                5,
                5,
                0,
                25,
                JumpTargetRegionMode::RegionWithContext,
            ),
            JumpStage::new(2, 2),
        ];
        let mut session = JumpSession::new(base_region(), stages).unwrap();

        assert_eq!(
            enter_code(&mut session, &[VirtualKey::B, VirtualKey::B]),
            Some(JumpSessionUpdate::StageAdvanced {
                stage_index: 1,
                region: JumpRegion {
                    left: 200,
                    top: 200,
                    width: 200,
                    height: 200,
                },
            })
        );
        assert_eq!(session.current_region, session.region_history[1]);

        assert_eq!(
            enter_code(&mut session, &[VirtualKey::A, VirtualKey::A]),
            Some(JumpSessionUpdate::StageAdvanced {
                stage_index: 2,
                region: JumpRegion {
                    left: 200,
                    top: 200,
                    width: 40,
                    height: 40,
                },
            })
        );
    }

    #[test]
    fn cancel_key_cancels_at_every_stage() {
        let stage_entries = [
            Vec::new(),
            vec![VirtualKey::B, VirtualKey::C],
            vec![VirtualKey::B, VirtualKey::C, VirtualKey::D, VirtualKey::E],
        ];

        for (expected_stage, entry_keys) in stage_entries.into_iter().enumerate() {
            let mut session = staged_session();
            for chunk in entry_keys.chunks(2) {
                enter_code(&mut session, chunk);
            }

            assert_eq!(session.stage_index, expected_stage);
            assert_eq!(
                press(&mut session, VirtualKey::A),
                Some(JumpSessionUpdate::Consumed)
            );
            assert_eq!(
                press(&mut session, VirtualKey::Escape),
                Some(JumpSessionUpdate::Cancelled)
            );
            assert_eq!(session.input, "");
            assert_eq!(session.stage_index, expected_stage);
        }
    }

    #[test]
    fn backspace_removes_only_the_current_stage_input() {
        let mut session = staged_session();

        assert_eq!(
            press(&mut session, VirtualKey::A),
            Some(JumpSessionUpdate::Consumed)
        );
        assert_eq!(
            press(&mut session, VirtualKey::Backspace),
            Some(JumpSessionUpdate::Consumed)
        );
        assert_eq!(session.input, "");
        assert_eq!(session.stage_index, 0);
        assert_eq!(session.current_region, base_region());

        assert_eq!(
            enter_code(&mut session, &[VirtualKey::C, VirtualKey::D]),
            Some(JumpSessionUpdate::StageAdvanced {
                stage_index: 1,
                region: JumpRegion {
                    left: 300,
                    top: 200,
                    width: 100,
                    height: 100,
                },
            })
        );
    }

    #[test]
    fn empty_input_backspace_stays_on_the_current_stage() {
        let mut session = staged_session();
        assert_eq!(
            enter_code(&mut session, &[VirtualKey::B, VirtualKey::C]),
            Some(JumpSessionUpdate::StageAdvanced {
                stage_index: 1,
                region: JumpRegion {
                    left: 200,
                    top: 100,
                    width: 100,
                    height: 100,
                },
            })
        );

        assert_eq!(
            press(&mut session, VirtualKey::Backspace),
            Some(JumpSessionUpdate::Consumed)
        );
        assert_eq!(session.input, "");
        assert_eq!(session.stage_index, 1);
        assert_eq!(
            session.current_region,
            JumpRegion {
                left: 200,
                top: 100,
                width: 100,
                height: 100,
            }
        );
    }
}
