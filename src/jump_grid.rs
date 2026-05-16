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

pub fn target_position(
    screen_left: i32,
    screen_top: i32,
    screen_width: i32,
    screen_height: i32,
    grid_size: (u32, u32),
    row: usize,
    col: usize,
) -> Option<(i32, i32)> {
    let (cols, rows) = grid_size;
    if cols == 0 || rows == 0 {
        return None;
    }
    if row >= rows as usize || col >= cols as usize {
        return None;
    }

    let x = screen_left as f64 + (((col as f64) + 0.5f64) * screen_width as f64 / cols as f64);
    let y = screen_top as f64 + (((row as f64) + 0.5f64) * screen_height as f64 / rows as f64);

    Some((x.round() as i32, y.round() as i32))
}

#[cfg(test)]
mod tests {
    use super::{code_to_index, expected_len, index_to_code, letters_needed, target_position};

    fn target_for_code(code: &str, grid_size: (u32, u32)) -> Option<(i32, i32)> {
        let row_len = letters_needed(grid_size.1);
        let col_len = letters_needed(grid_size.0);

        if code.chars().count() != row_len + col_len {
            return None;
        }

        let row_code: String = code.chars().take(row_len).collect();
        let col_code: String = code.chars().skip(row_len).take(col_len).collect();
        let row = code_to_index(&row_code)?;
        let col = code_to_index(&col_code)?;

        target_position(0, 0, 1000, 1000, grid_size, row, col)
    }

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
            assert!(target_for_code(code, (10, 10)).is_some(), "{code}");
        }

        for code in ["KA", "AZ", "ZZ"] {
            assert!(target_for_code(code, (10, 10)).is_none(), "{code}");
        }
    }

    #[test]
    fn expected_len_combines_row_and_column_widths() {
        assert_eq!(expected_len((10, 10)), 2);
        assert_eq!(expected_len((27, 10)), 3);
        assert_eq!(expected_len((10, 27)), 3);
    }
}
