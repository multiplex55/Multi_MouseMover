/// Jump-grid labels use fixed-width, zero-based base-N.
///
/// This is intentionally not spreadsheet style. For a one-letter axis, labels
/// are `A` through `Z`. For a two-letter label, index 0 is `AA`, index 25 is
/// `AZ`, index 26 is `BA`, and so on.
#[allow(dead_code)]
pub const DEFAULT_SELECTION_KEYS: &[char] = &[
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S',
    'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
];

pub fn letters_needed(value: u32) -> usize {
    letters_needed_for_base(value as usize, DEFAULT_SELECTION_KEYS.len())
}

pub fn letters_needed_for_base(value: usize, base: usize) -> usize {
    if base == 0 {
        return 0;
    }

    let mut len = 1usize;
    let mut capacity = base;

    while value > capacity {
        len += 1;
        capacity = capacity.saturating_mul(base);
    }

    len
}

pub fn index_to_code(index: usize, len: usize) -> String {
    index_to_code_with_keys(index, len, DEFAULT_SELECTION_KEYS).unwrap_or_default()
}

pub fn index_to_code_with_keys(
    mut index: usize,
    len: usize,
    selection_keys: &[char],
) -> Option<String> {
    if len == 0 || selection_keys.is_empty() {
        return None;
    }

    let base = selection_keys.len();
    let mut chars = vec![selection_keys[0]; len];

    for i in (0..len).rev() {
        chars[i] = selection_keys[index % base];
        index /= base;
    }

    Some(chars.into_iter().collect())
}

pub fn code_to_index(code: &str) -> Option<usize> {
    code_to_index_with_keys(code, DEFAULT_SELECTION_KEYS)
}

pub fn code_to_index_with_keys(code: &str, selection_keys: &[char]) -> Option<usize> {
    if code.is_empty() || selection_keys.is_empty() {
        return None;
    }

    let mut idx = 0usize;

    for ch in code.chars() {
        let digit = selection_keys.iter().position(|&key| key == ch)?;
        idx = idx * selection_keys.len() + digit;
    }

    Some(idx)
}

pub fn expected_len(grid_size: (u32, u32)) -> usize {
    let (cols, rows) = grid_size;
    letters_needed(rows) + letters_needed(cols)
}

pub fn generate_default_labels(grid_size: (u32, u32)) -> Option<Vec<String>> {
    generate_labels(grid_size, DEFAULT_SELECTION_KEYS)
}

pub fn generate_labels(grid_size: (u32, u32), selection_keys: &[char]) -> Option<Vec<String>> {
    let (cols, rows) = grid_size;
    if cols == 0 || rows == 0 || !selection_keys_are_unique(selection_keys) {
        return None;
    }

    let cell_count = (cols as usize).checked_mul(rows as usize)?;
    let label_len = letters_needed_for_base(cell_count, selection_keys.len());
    let mut labels = Vec::with_capacity(cell_count);

    for index in 0..cell_count {
        labels.push(index_to_code_with_keys(index, label_len, selection_keys)?);
    }

    Some(labels)
}

fn selection_keys_are_unique(selection_keys: &[char]) -> bool {
    if selection_keys.is_empty() {
        return false;
    }

    for (index, key) in selection_keys.iter().enumerate() {
        if selection_keys[index + 1..].contains(key) {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::{
        code_to_index, expected_len, generate_default_labels, generate_labels, index_to_code,
        letters_needed,
    };
    use std::collections::HashSet;

    #[test]
    fn letters_needed_scales_at_base_26_boundaries() {
        assert_eq!(letters_needed(1), 1);
        assert_eq!(letters_needed(10), 1);
        assert_eq!(letters_needed(26), 1);
        assert_eq!(letters_needed(27), 2);
    }

    #[test]
    fn index_to_code_uses_fixed_width_zero_based_base_26() {
        assert_eq!(index_to_code(0, 1), "A");
        assert_eq!(index_to_code(25, 1), "Z");
        assert_eq!(index_to_code(0, 2), "AA");
        assert_eq!(index_to_code(25, 2), "AZ");
        assert_eq!(index_to_code(26, 2), "BA");
        assert_eq!(index_to_code(27, 2), "BB");
    }

    #[test]
    fn index_and_code_round_trip() {
        for len in 1..=3 {
            let max = 26usize.pow(len as u32);
            for index in [0, 1, 25, 26, 27, max / 2, max - 1] {
                if index >= max {
                    continue;
                }
                let code = index_to_code(index, len);
                assert_eq!(code_to_index(&code), Some(index));
            }
        }
    }

    #[test]
    fn ten_by_ten_validity_matrix() {
        for code in ["AA", "AJ", "JA", "JJ"] {
            assert!(code_to_index(code).is_some(), "{code}");
        }

        for code in ["KA", "AZ", "ZZ"] {
            let row = code_to_index(&code[..1]).unwrap();
            let col = code_to_index(&code[1..]).unwrap();
            assert!(row >= 10 || col >= 10, "{code}");
        }
    }

    #[test]
    fn expected_len_combines_row_and_column_widths() {
        assert_eq!(expected_len((10, 10)), 2);
        assert_eq!(expected_len((27, 10)), 3);
        assert_eq!(expected_len((10, 27)), 3);
    }

    #[test]
    fn generated_labels_are_unique_and_deterministic() {
        let first = generate_default_labels((10, 10)).unwrap();
        let second = generate_default_labels((10, 10)).unwrap();
        let unique: HashSet<_> = first.iter().collect();

        assert_eq!(first, second);
        assert_eq!(first.len(), 100);
        assert_eq!(unique.len(), first.len());
    }

    #[test]
    fn generated_width_two_labels_follow_zero_based_base_26_sequence() {
        let labels = generate_default_labels((27, 1)).unwrap();

        assert_eq!(labels[0], "AA");
        assert_eq!(labels[1], "AB");
        assert_eq!(labels[25], "AZ");
        assert_eq!(labels[26], "BA");
    }

    #[test]
    fn generated_labels_accept_custom_selection_keys() {
        assert_eq!(
            generate_labels((3, 1), &['X', 'Y', 'Z']).unwrap(),
            vec!["X", "Y", "Z"]
        );
    }

    #[test]
    fn small_custom_keyset_generates_expected_label_sequence() {
        assert_eq!(
            generate_labels((2, 2), &['X', 'Y']).unwrap(),
            vec!["XX", "XY", "YX", "YY"]
        );
    }

    #[test]
    fn generated_labels_reject_duplicate_selection_keys() {
        assert_eq!(generate_labels((3, 1), &['A', 'A', 'B']), None);
    }
}
