use crate::jump_session::JumpRegion;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridSession {
    pub monitor_bounds: JumpRegion,
    pub current_region: JumpRegion,
    pub history: Vec<JumpRegion>,
    pub min_width: i32,
    pub min_height: i32,
    pub move_cursor_each_step: Option<bool>,
}

impl GridSession {
    pub fn start(
        monitor_bounds: JumpRegion,
        min_width: i32,
        min_height: i32,
        move_cursor_each_step: Option<bool>,
    ) -> Option<Self> {
        if !monitor_bounds.is_valid() || min_width <= 0 || min_height <= 0 {
            return None;
        }

        Some(Self {
            monitor_bounds,
            current_region: monitor_bounds,
            history: Vec::new(),
            min_width,
            min_height,
            move_cursor_each_step,
        })
    }

    pub fn shrink_up(&mut self) -> Option<JumpRegion> {
        let top = split_top(self.current_region)?;
        self.apply_region(top)
    }

    pub fn shrink_down(&mut self) -> Option<JumpRegion> {
        let bottom = split_bottom(self.current_region)?;
        self.apply_region(bottom)
    }

    pub fn shrink_left(&mut self) -> Option<JumpRegion> {
        let left = split_left(self.current_region)?;
        self.apply_region(left)
    }

    pub fn shrink_right(&mut self) -> Option<JumpRegion> {
        let right = split_right(self.current_region)?;
        self.apply_region(right)
    }

    pub fn undo(&mut self) -> Option<JumpRegion> {
        let previous = self.history.pop()?;
        self.current_region = previous;
        Some(previous)
    }

    pub fn center(&self) -> (i32, i32) {
        self.current_region.center()
    }

    pub fn is_resolved(&self) -> bool {
        self.current_region.width <= self.min_width || self.current_region.height <= self.min_height
    }

    fn apply_region(&mut self, region: JumpRegion) -> Option<JumpRegion> {
        if !region.is_valid() {
            return None;
        }

        self.history.push(self.current_region);
        self.current_region = region;
        Some(region)
    }
}

fn split_left(region: JumpRegion) -> Option<JumpRegion> {
    let left_width = region.width / 2;
    (left_width > 0).then_some(JumpRegion {
        width: left_width,
        ..region
    })
}

fn split_right(region: JumpRegion) -> Option<JumpRegion> {
    let left_width = region.width / 2;
    let right_width = region.width - left_width;
    (left_width > 0 && right_width > 0).then_some(JumpRegion {
        left: region.left + left_width,
        width: right_width,
        ..region
    })
}

fn split_top(region: JumpRegion) -> Option<JumpRegion> {
    let top_height = region.height / 2;
    (top_height > 0).then_some(JumpRegion {
        height: top_height,
        ..region
    })
}

fn split_bottom(region: JumpRegion) -> Option<JumpRegion> {
    let top_height = region.height / 2;
    let bottom_height = region.height - top_height;
    (top_height > 0 && bottom_height > 0).then_some(JumpRegion {
        top: region.top + top_height,
        height: bottom_height,
        ..region
    })
}

#[cfg(test)]
mod tests {
    use super::{split_bottom, split_left, split_right, split_top, GridSession};
    use crate::jump_session::JumpRegion;

    fn region(left: i32, top: i32, width: i32, height: i32) -> JumpRegion {
        JumpRegion {
            left,
            top,
            width,
            height,
        }
    }

    fn session() -> GridSession {
        GridSession::start(region(10, 20, 101, 51), 4, 3, Some(true)).unwrap()
    }

    #[test]
    fn start_initializes_session_state() {
        let bounds = region(-100, 50, 300, 200);
        let session = GridSession::start(bounds, 8, 6, Some(false)).unwrap();

        assert_eq!(session.monitor_bounds, bounds);
        assert_eq!(session.current_region, bounds);
        assert!(session.history.is_empty());
        assert_eq!(session.min_width, 8);
        assert_eq!(session.min_height, 6);
        assert_eq!(session.move_cursor_each_step, Some(false));
    }

    #[test]
    fn start_rejects_invalid_bounds_and_thresholds() {
        assert!(GridSession::start(region(0, 0, 0, 100), 1, 1, None).is_none());
        assert!(GridSession::start(region(0, 0, 100, 100), 0, 1, None).is_none());
        assert!(GridSession::start(region(0, 0, 100, 100), 1, -1, None).is_none());
    }

    #[test]
    fn directional_splits_select_expected_half() {
        let mut up = session();
        assert_eq!(up.shrink_up(), Some(region(10, 20, 101, 25)));

        let mut down = session();
        assert_eq!(down.shrink_down(), Some(region(10, 45, 101, 26)));

        let mut left = session();
        assert_eq!(left.shrink_left(), Some(region(10, 20, 50, 51)));

        let mut right = session();
        assert_eq!(right.shrink_right(), Some(region(60, 20, 51, 51)));
    }

    #[test]
    fn odd_pixel_splits_preserve_area_without_gaps() {
        let original = region(10, 20, 101, 51);

        let left = split_left(original).unwrap();
        let right = split_right(original).unwrap();
        assert_eq!(left.left, original.left);
        assert_eq!(left.left + left.width, right.left);
        assert_eq!(right.left + right.width, original.left + original.width);
        assert_eq!(left.width + right.width, original.width);
        assert!(left.width > 0);
        assert!(right.width > 0);

        let top = split_top(original).unwrap();
        let bottom = split_bottom(original).unwrap();
        assert_eq!(top.top, original.top);
        assert_eq!(top.top + top.height, bottom.top);
        assert_eq!(bottom.top + bottom.height, original.top + original.height);
        assert_eq!(top.height + bottom.height, original.height);
        assert!(top.height > 0);
        assert!(bottom.height > 0);
    }

    #[test]
    fn one_pixel_dimension_cannot_split_into_zero_sized_regions() {
        let mut narrow = GridSession::start(region(0, 0, 1, 10), 1, 1, None).unwrap();
        assert_eq!(narrow.shrink_left(), None);
        assert_eq!(narrow.shrink_right(), None);
        assert_eq!(narrow.current_region, region(0, 0, 1, 10));
        assert!(narrow.history.is_empty());

        let mut short = GridSession::start(region(0, 0, 10, 1), 1, 1, None).unwrap();
        assert_eq!(short.shrink_up(), None);
        assert_eq!(short.shrink_down(), None);
        assert_eq!(short.current_region, region(0, 0, 10, 1));
        assert!(short.history.is_empty());
    }

    #[test]
    fn completion_uses_width_or_height_threshold() {
        let mut width_resolved = GridSession::start(region(0, 0, 8, 100), 8, 4, None).unwrap();
        assert!(width_resolved.is_resolved());

        let height_resolved = GridSession::start(region(0, 0, 100, 4), 8, 4, None).unwrap();
        assert!(height_resolved.is_resolved());

        let unresolved = GridSession::start(region(0, 0, 9, 5), 8, 4, None).unwrap();
        assert!(!unresolved.is_resolved());

        width_resolved.min_height = 200;
        assert!(width_resolved.is_resolved());
    }

    #[test]
    fn undo_restores_previous_regions_in_reverse_order() {
        let mut session = session();
        assert_eq!(session.shrink_right(), Some(region(60, 20, 51, 51)));
        assert_eq!(session.shrink_down(), Some(region(60, 45, 51, 26)));
        assert_eq!(
            session.history,
            vec![region(10, 20, 101, 51), region(60, 20, 51, 51)]
        );

        assert_eq!(session.undo(), Some(region(60, 20, 51, 51)));
        assert_eq!(session.current_region, region(60, 20, 51, 51));
        assert_eq!(session.undo(), Some(region(10, 20, 101, 51)));
        assert_eq!(session.current_region, region(10, 20, 101, 51));
        assert_eq!(session.undo(), None);
        assert_eq!(session.current_region, region(10, 20, 101, 51));
    }

    #[test]
    fn center_returns_current_region_center() {
        let mut session = GridSession::start(region(10, 20, 11, 10), 1, 1, None).unwrap();
        assert_eq!(session.center(), (16, 25));

        session.shrink_left();
        assert_eq!(session.current_region, region(10, 20, 5, 10));
        assert_eq!(session.center(), (13, 25));
    }
}
