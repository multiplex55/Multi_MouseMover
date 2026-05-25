use crate::ui_hints::{generate_ui_hint_labels, UiHintOverflowBehavior};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedMousePosition {
    pub id: u64,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PositionHistoryUpdate {
    Added(SavedMousePosition),
    Cleared,
    RemovedOldest(SavedMousePosition),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionHistory {
    max_positions: usize,
    next_id: u64,
    positions: Vec<SavedMousePosition>,
}

impl PositionHistory {
    pub fn new(max_positions: usize) -> Self {
        Self {
            max_positions: max_positions.max(1),
            next_id: 1,
            positions: Vec::new(),
        }
    }

    pub fn add(&mut self, x: i32, y: i32) -> Vec<PositionHistoryUpdate> {
        let mut updates = Vec::new();
        if self.positions.len() >= self.max_positions {
            let removed = self.positions.remove(0);
            updates.push(PositionHistoryUpdate::RemovedOldest(removed));
        }
        let added = SavedMousePosition {
            id: self.next_id,
            x,
            y,
        };
        self.next_id += 1;
        self.positions.push(added);
        updates.push(PositionHistoryUpdate::Added(added));
        updates
    }

    pub fn clear(&mut self) -> bool {
        if self.positions.is_empty() {
            return false;
        }
        self.positions.clear();
        true
    }

    pub fn positions(&self) -> &[SavedMousePosition] {
        &self.positions
    }

    pub fn labeled_positions(
        &self,
        selection_keys: &[char],
        label_length: usize,
        show_numbers: bool,
    ) -> Vec<(String, SavedMousePosition)> {
        let labels = generate_ui_hint_labels(
            self.positions.len(),
            selection_keys,
            label_length,
            UiHintOverflowBehavior::IncreaseLength,
            Some(self.positions.len()),
        );
        self.positions
            .iter()
            .copied()
            .zip(labels)
            .map(|(p, label)| {
                if show_numbers {
                    (format!("{}:{}", p.id, label), p)
                } else {
                    (label, p)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_remove_clear_behavior() {
        let mut h = PositionHistory::new(2);
        h.add(1, 2);
        h.add(3, 4);
        assert_eq!(h.positions().len(), 2);
        assert!(h.clear());
        assert!(h.positions().is_empty());
    }

    #[test]
    fn evicts_oldest_at_capacity() {
        let mut h = PositionHistory::new(2);
        h.add(1, 1);
        h.add(2, 2);
        let updates = h.add(3, 3);
        assert!(matches!(
            updates[0],
            PositionHistoryUpdate::RemovedOldest(_)
        ));
        assert_eq!(h.positions()[0].x, 2);
        assert_eq!(h.positions()[1].x, 3);
    }

    #[test]
    fn label_assignment_is_deterministic() {
        let mut h = PositionHistory::new(3);
        h.add(1, 1);
        h.add(2, 2);
        let first = h.labeled_positions(&['A', 'B'], 2, false);
        let second = h.labeled_positions(&['A', 'B'], 2, false);
        assert_eq!(first, second);
    }
}
