use crate::{
    jump_grid::{code_to_index, expected_len, letters_needed},
    keyboard::VirtualKey,
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
}

impl JumpStage {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
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

impl JumpSession {
    pub fn new(region: JumpRegion, stages: Vec<JumpStage>) -> Option<Self> {
        if !region.is_valid() || stages.is_empty() || stages.iter().any(|stage| !stage.is_valid()) {
            return None;
        }

        Some(Self {
            stages,
            stage_index: 0,
            current_region: region,
            input: String::new(),
            path: Vec::new(),
            region_history: vec![region],
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

        self.current_region = region;
        self.path.push((row, col));
        self.region_history.push(region);

        if self.stage_index + 1 < self.stages.len() {
            self.stage_index += 1;
            JumpSessionUpdate::StageAdvanced {
                stage_index: self.stage_index,
                region,
            }
        } else {
            let (x, y) = region.center();
            JumpSessionUpdate::Completed { x, y, region }
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
    use super::{subdivide_region, JumpRegion, JumpSession, JumpSessionUpdate, JumpStage};
    use crate::keyboard::VirtualKey;

    fn region() -> JumpRegion {
        JumpRegion {
            left: 0,
            top: 0,
            width: 100,
            height: 90,
        }
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
        assert!(region().is_valid());
        assert!(!JumpRegion {
            width: 0,
            ..region()
        }
        .is_valid());
        assert!(!JumpRegion {
            height: -1,
            ..region()
        }
        .is_valid());
    }

    #[test]
    fn subdivide_region_uses_rounded_float_boundaries() {
        assert_eq!(
            subdivide_region(region(), (3, 2), 1, 2),
            Some(JumpRegion {
                left: 67,
                top: 45,
                width: 33,
                height: 45,
            })
        );
    }

    #[test]
    fn subdivide_region_rejects_invalid_inputs() {
        assert_eq!(subdivide_region(region(), (0, 2), 0, 0), None);
        assert_eq!(subdivide_region(region(), (2, 2), 2, 0), None);
        assert_eq!(subdivide_region(region(), (2, 2), 0, 2), None);
        assert_eq!(
            subdivide_region(
                JumpRegion {
                    width: 0,
                    ..region()
                },
                (2, 2),
                0,
                0
            ),
            None
        );
    }

    #[test]
    fn letters_are_consumed_until_single_stage_completes() {
        let mut session = JumpSession::new(region(), vec![JumpStage::new(10, 10)]).unwrap();

        assert_eq!(
            session.handle_key(VirtualKey::A, true),
            Some(JumpSessionUpdate::Consumed)
        );
        assert_eq!(
            session.handle_key(VirtualKey::J, true),
            Some(JumpSessionUpdate::Completed {
                x: 95,
                y: 5,
                region: JumpRegion {
                    left: 90,
                    top: 0,
                    width: 10,
                    height: 9,
                },
            })
        );
        assert_eq!(session.input, "");
        assert_eq!(session.path, vec![(0, 9)]);
    }

    #[test]
    fn invalid_code_clears_input_without_advancing() {
        let mut session = JumpSession::new(region(), vec![JumpStage::new(10, 10)]).unwrap();

        assert_eq!(
            session.handle_key(VirtualKey::K, true),
            Some(JumpSessionUpdate::Consumed)
        );
        assert_eq!(
            session.handle_key(VirtualKey::A, true),
            Some(JumpSessionUpdate::Invalid)
        );
        assert_eq!(session.input, "");
        assert_eq!(session.stage_index, 0);
        assert_eq!(session.current_region, region());
    }

    #[test]
    fn cancel_backspace_and_ignored_keys_are_deterministic() {
        let mut session = JumpSession::new(region(), vec![JumpStage::new(10, 10)]).unwrap();

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
        let mut session =
            JumpSession::new(region(), vec![JumpStage::new(10, 10), JumpStage::new(5, 5)]).unwrap();

        assert_eq!(
            session.handle_key(VirtualKey::B, true),
            Some(JumpSessionUpdate::Consumed)
        );
        assert_eq!(
            session.handle_key(VirtualKey::C, true),
            Some(JumpSessionUpdate::StageAdvanced {
                stage_index: 1,
                region: JumpRegion {
                    left: 20,
                    top: 9,
                    width: 10,
                    height: 9,
                },
            })
        );
        assert_eq!(session.current_grid(), (5, 5));

        assert_eq!(
            session.handle_key(VirtualKey::A, true),
            Some(JumpSessionUpdate::Consumed)
        );
        assert_eq!(
            session.handle_key(VirtualKey::A, true),
            Some(JumpSessionUpdate::Completed {
                x: 21,
                y: 10,
                region: JumpRegion {
                    left: 20,
                    top: 9,
                    width: 2,
                    height: 2,
                },
            })
        );
        assert_eq!(session.region_history.len(), 3);
    }
}
