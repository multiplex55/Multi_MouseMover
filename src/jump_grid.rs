/// Jump-grid labels use fixed-width, zero-based base-26.
///
/// This is intentionally not spreadsheet style. For a one-letter axis, labels
/// are `A` through `Z`. For a two-letter axis, index 0 is `AA`, index 25 is
/// `AZ`, index 26 is `BA`, and so on.
const ALPHABET_SIZE: usize = 26;

pub fn letters_needed(value: u32) -> usize {
    let mut len = 1usize;
    let mut capacity = ALPHABET_SIZE as u32;

    while value > capacity {
        len += 1;
        capacity = capacity.saturating_mul(ALPHABET_SIZE as u32);
    }

    len
}

pub fn index_to_code(mut index: usize, len: usize) -> String {
    let mut chars = vec!['A'; len];

    for i in (0..len).rev() {
        chars[i] = (b'A' + (index % ALPHABET_SIZE) as u8) as char;
        index /= ALPHABET_SIZE;
    }

    chars.into_iter().collect()
}

pub fn code_to_index(code: &str) -> Option<usize> {
    let mut idx = 0usize;

    for ch in code.chars() {
        if !ch.is_ascii_uppercase() {
            return None;
        }
        idx = idx * ALPHABET_SIZE + ((ch as u8 - b'A') as usize);
    }

    Some(idx)
}

pub fn expected_len(grid_size: (u32, u32)) -> usize {
    let (cols, rows) = grid_size;
    letters_needed(rows) + letters_needed(cols)
}

#[cfg(test)]
mod tests {
    use super::{code_to_index, expected_len, index_to_code, letters_needed};

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
}
