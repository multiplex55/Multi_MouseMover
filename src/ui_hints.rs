use crate::{jump_grid::index_to_code_with_keys, keyboard::VirtualKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiHintOverflowBehavior {
    IncreaseLength,
    Cap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiHintTargetPoint {
    Center,
    ClickablePoint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiHintConfig {
    pub selection_keys: Vec<char>,
    pub label_length: usize,
    pub overflow_behavior: UiHintOverflowBehavior,
    pub max_hints: Option<usize>,
    pub min_hint_spacing_px: i32,
    pub target_point: UiHintTargetPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawUiElement {
    pub id: u64,
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
    pub clickable_point: Option<(i32, i32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiHintTarget {
    pub id: u64,
    pub label: String,
    pub bounds: (i32, i32, i32, i32),
    pub target_x: i32,
    pub target_y: i32,
    pub metadata: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiHintSession {
    pub targets: Vec<UiHintTarget>,
    pub input: String,
    pub selection_keys: Vec<char>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiHintInputUpdate {
    Consumed,
    PrefixChanged,
    Invalid,
    Cancelled,
    Completed { target: UiHintTarget },
}

impl UiHintSession {
    pub fn new(targets: Vec<UiHintTarget>, selection_keys: Vec<char>) -> Option<Self> {
        if targets.is_empty() || selection_keys.is_empty() {
            return None;
        }
        Some(Self {
            targets,
            input: String::new(),
            selection_keys,
        })
    }

    pub fn handle_key(&mut self, key: VirtualKey) -> UiHintInputUpdate {
        match key {
            VirtualKey::Escape => return UiHintInputUpdate::Cancelled,
            VirtualKey::Backspace => {
                self.input.pop();
                return UiHintInputUpdate::PrefixChanged;
            }
            _ => {}
        }

        let Some(ch) = alpha_key_to_char(key) else {
            return UiHintInputUpdate::Consumed;
        };

        if !self.selection_keys.contains(&ch) {
            return UiHintInputUpdate::Consumed;
        }

        let mut candidate = self.input.clone();
        candidate.push(ch);
        let matches: Vec<&UiHintTarget> = self
            .targets
            .iter()
            .filter(|target| target.label.starts_with(&candidate))
            .collect();

        if matches.is_empty() {
            return UiHintInputUpdate::Invalid;
        }

        self.input = candidate;
        if matches.len() == 1 && matches[0].label == self.input {
            return UiHintInputUpdate::Completed {
                target: matches[0].clone(),
            };
        }

        UiHintInputUpdate::PrefixChanged
    }
}

pub fn generate_ui_hint_labels(
    count: usize,
    selection_keys: &[char],
    label_length: usize,
    overflow_behavior: UiHintOverflowBehavior,
    max_hints: Option<usize>,
) -> Vec<String> {
    if count == 0 || selection_keys.is_empty() || label_length == 0 {
        return Vec::new();
    }

    let base = selection_keys.len();
    let mut len = label_length;
    let mut capacity = base.saturating_pow(len as u32);

    let mut target_count = count;
    if let Some(max) = max_hints {
        target_count = target_count.min(max);
    }

    if target_count > capacity {
        match overflow_behavior {
            UiHintOverflowBehavior::IncreaseLength => {
                while target_count > capacity {
                    len += 1;
                    capacity = base.saturating_pow(len as u32);
                }
            }
            UiHintOverflowBehavior::Cap => {
                target_count = target_count.min(capacity);
            }
        }
    }

    (0..target_count)
        .filter_map(|index| index_to_code_with_keys(index, len, selection_keys))
        .collect()
}

pub fn build_ui_hint_targets(
    raw_elements: Vec<RawUiElement>,
    config: &UiHintConfig,
) -> Vec<UiHintTarget> {
    let mut elements: Vec<RawUiElement> = raw_elements
        .into_iter()
        .filter(|e| e.width > 0 && e.height > 0)
        .collect();

    elements.sort_by_key(|e| (e.top, e.left, e.width * e.height, e.width, e.height));

    let mut deduped = Vec::new();
    for element in elements {
        let center = center_of(&element);
        let too_close = deduped.iter().any(|existing: &RawUiElement| {
            let other_center = center_of(existing);
            squared_distance(center, other_center)
                < (config.min_hint_spacing_px * config.min_hint_spacing_px) as i64
        });
        if !too_close {
            deduped.push(element);
        }
    }

    if let Some(limit) = config.max_hints {
        deduped.truncate(limit);
    }

    let labels = generate_ui_hint_labels(
        deduped.len(),
        &config.selection_keys,
        config.label_length,
        config.overflow_behavior,
        config.max_hints,
    );

    deduped
        .into_iter()
        .zip(labels)
        .map(|(element, label)| {
            let (target_x, target_y) = match (config.target_point, element.clickable_point) {
                (UiHintTargetPoint::ClickablePoint, Some(point)) => point,
                _ => center_of(&element),
            };

            UiHintTarget {
                id: element.id,
                label,
                bounds: (element.left, element.top, element.width, element.height),
                target_x,
                target_y,
                metadata: Some(format!("{}x{}", element.width, element.height)),
            }
        })
        .collect()
}

pub fn generate_debug_fake_targets(config: &UiHintConfig) -> Vec<UiHintTarget> {
    let labels = generate_ui_hint_labels(
        5,
        &config.selection_keys,
        config.label_length,
        UiHintOverflowBehavior::IncreaseLength,
        Some(5),
    );
    if labels.len() < 5 {
        return Vec::new();
    }

    let center = (960, 540);
    let spread = 120;
    let points = [
        center,
        (center.0, center.1 - spread),
        (center.0, center.1 + spread),
        (center.0 - spread, center.1),
        (center.0 + spread, center.1),
    ];

    points
        .into_iter()
        .zip(labels)
        .enumerate()
        .map(|(index, ((x, y), label))| UiHintTarget {
            id: (index + 1) as u64,
            label,
            bounds: (x - 8, y - 8, 16, 16),
            target_x: x,
            target_y: y,
            metadata: Some("debug_fake".to_string()),
        })
        .collect()
}

fn center_of(element: &RawUiElement) -> (i32, i32) {
    (
        element.left + element.width / 2,
        element.top + element.height / 2,
    )
}

fn squared_distance(a: (i32, i32), b: (i32, i32)) -> i64 {
    let dx = (a.0 - b.0) as i64;
    let dy = (a.1 - b.1) as i64;
    dx * dx + dy * dy
}

fn alpha_key_to_char(key: VirtualKey) -> Option<char> {
    match key {
        VirtualKey::A => Some('A'),
        VirtualKey::B => Some('B'),
        VirtualKey::C => Some('C'),
        VirtualKey::D => Some('D'),
        VirtualKey::E => Some('E'),
        VirtualKey::F => Some('F'),
        VirtualKey::G => Some('G'),
        VirtualKey::H => Some('H'),
        VirtualKey::I => Some('I'),
        VirtualKey::J => Some('J'),
        VirtualKey::K => Some('K'),
        VirtualKey::L => Some('L'),
        VirtualKey::M => Some('M'),
        VirtualKey::N => Some('N'),
        VirtualKey::O => Some('O'),
        VirtualKey::P => Some('P'),
        VirtualKey::Q => Some('Q'),
        VirtualKey::R => Some('R'),
        VirtualKey::S => Some('S'),
        VirtualKey::T => Some('T'),
        VirtualKey::U => Some('U'),
        VirtualKey::V => Some('V'),
        VirtualKey::W => Some('W'),
        VirtualKey::X => Some('X'),
        VirtualKey::Y => Some('Y'),
        VirtualKey::Z => Some('Z'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_fixed_two_letter_labels() {
        let labels = generate_ui_hint_labels(
            27,
            &[
                'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P',
                'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
            ],
            2,
            UiHintOverflowBehavior::Cap,
            None,
        );
        assert_eq!(labels[0], "AA");
        assert_eq!(labels[25], "AZ");
        assert_eq!(labels[26], "BA");
    }

    #[test]
    fn increases_label_length_on_overflow() {
        let labels = generate_ui_hint_labels(
            5,
            &['A', 'B'],
            2,
            UiHintOverflowBehavior::IncreaseLength,
            None,
        );
        assert_eq!(labels, vec!["AAA", "AAB", "ABA", "ABB", "BAA"]);
    }

    #[test]
    fn caps_labels_when_overflow_behavior_cap() {
        let labels = generate_ui_hint_labels(5, &['A', 'B'], 2, UiHintOverflowBehavior::Cap, None);
        assert_eq!(labels, vec!["AA", "AB", "BA", "BB"]);
    }

    #[test]
    fn sorts_targets_top_left_to_bottom_right() {
        let config = sample_config();
        let targets = build_ui_hint_targets(
            vec![
                raw(1, 50, 50, 10, 10, None),
                raw(2, 10, 10, 10, 10, None),
                raw(3, 20, 10, 10, 10, None),
            ],
            &config,
        );
        assert_eq!(
            targets.iter().map(|t| t.id).collect::<Vec<_>>(),
            vec![2, 3, 1]
        );
    }

    #[test]
    fn deduplicates_nearby_targets() {
        let mut config = sample_config();
        config.min_hint_spacing_px = 10;
        let targets = build_ui_hint_targets(
            vec![
                raw(1, 0, 0, 10, 10, None),
                raw(2, 3, 4, 10, 10, None),
                raw(3, 30, 30, 10, 10, None),
            ],
            &config,
        );
        assert_eq!(targets.iter().map(|t| t.id).collect::<Vec<_>>(), vec![1, 3]);
    }

    #[test]
    fn exact_label_completes_session() {
        let mut session = UiHintSession::new(vec![target(1, "AA")], vec!['A', 'B']).unwrap();
        assert_eq!(
            session.handle_key(VirtualKey::A),
            UiHintInputUpdate::PrefixChanged
        );
        match session.handle_key(VirtualKey::A) {
            UiHintInputUpdate::Completed { target } => assert_eq!(target.id, 1),
            other => panic!("unexpected update: {other:?}"),
        }
    }

    #[test]
    fn partial_label_updates_prefix() {
        let mut session =
            UiHintSession::new(vec![target(1, "AA"), target(2, "AB")], vec!['A', 'B']).unwrap();
        assert_eq!(
            session.handle_key(VirtualKey::A),
            UiHintInputUpdate::PrefixChanged
        );
        assert_eq!(session.input, "A");
    }

    #[test]
    fn invalid_key_does_not_complete() {
        let mut session = UiHintSession::new(vec![target(1, "AA")], vec!['A', 'B']).unwrap();
        assert_eq!(
            session.handle_key(VirtualKey::Num1),
            UiHintInputUpdate::Consumed
        );
        assert_eq!(session.input, "");
    }

    #[test]
    fn backspace_removes_input() {
        let mut session = UiHintSession::new(vec![target(1, "AA")], vec!['A', 'B']).unwrap();
        session.input = "A".to_string();
        assert_eq!(
            session.handle_key(VirtualKey::Backspace),
            UiHintInputUpdate::PrefixChanged
        );
        assert_eq!(session.input, "");
    }

    #[test]
    fn escape_cancels() {
        let mut session = UiHintSession::new(vec![target(1, "AA")], vec!['A', 'B']).unwrap();
        assert_eq!(
            session.handle_key(VirtualKey::Escape),
            UiHintInputUpdate::Cancelled
        );
    }

    #[test]
    fn empty_target_list_creates_no_session() {
        assert!(UiHintSession::new(Vec::new(), vec!['A']).is_none());
    }

    #[test]
    fn debug_fake_target_generator_is_deterministic() {
        let config = sample_config();
        let targets = generate_debug_fake_targets(&config);
        assert_eq!(
            targets.iter().map(|t| t.label.as_str()).collect::<Vec<_>>(),
            vec!["AA", "AB", "AC", "AD", "AE"]
        );
        assert_eq!(targets[0].target_x, 960);
        assert_eq!(targets[0].target_y, 540);
        assert_eq!(targets[1].target_y, 420);
        assert_eq!(targets[2].target_y, 660);
        assert_eq!(targets[3].target_x, 840);
        assert_eq!(targets[4].target_x, 1080);
    }

    #[test]
    fn debug_fake_targets_support_prefix_and_completion_flow() {
        let config = sample_config();
        let targets = generate_debug_fake_targets(&config);
        let mut session = UiHintSession::new(targets, vec!['A', 'B', 'C']).unwrap();
        assert_eq!(
            session.handle_key(VirtualKey::A),
            UiHintInputUpdate::PrefixChanged
        );
        assert_eq!(
            session.handle_key(VirtualKey::E),
            UiHintInputUpdate::Completed {
                target: session.targets[4].clone()
            }
        );
    }

    fn sample_config() -> UiHintConfig {
        UiHintConfig {
            selection_keys: vec!['A', 'B', 'C'],
            label_length: 2,
            overflow_behavior: UiHintOverflowBehavior::IncreaseLength,
            max_hints: None,
            min_hint_spacing_px: 0,
            target_point: UiHintTargetPoint::Center,
        }
    }

    fn raw(
        id: u64,
        left: i32,
        top: i32,
        width: i32,
        height: i32,
        clickable_point: Option<(i32, i32)>,
    ) -> RawUiElement {
        RawUiElement {
            id,
            left,
            top,
            width,
            height,
            clickable_point,
        }
    }

    fn target(id: u64, label: &str) -> UiHintTarget {
        UiHintTarget {
            id,
            label: label.to_string(),
            bounds: (0, 0, 1, 1),
            target_x: 0,
            target_y: 0,
            metadata: None,
        }
    }
}
