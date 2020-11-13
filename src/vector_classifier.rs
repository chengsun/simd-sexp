use std::collections::HashMap;

#[derive(Debug)]
pub struct VectorClassifier {
    pub shuffle_table_lo: [u8; 16],
    pub shuffle_table_hi: [u8; 16],
}

impl VectorClassifier {
    pub fn new(accept: &[bool]) -> Option<Self> {
        let mut hi = [0u16; 16];
        let mut lo = [0u16; 16];
        for i in 0..256 {
            if accept[i] {
                hi[i / 16] |= 1 << (i % 16);
                lo[i % 16] |= 1 << (i / 16);
            }
        }
        let mut unique_hi = HashMap::new();
        let mut unique_lo = HashMap::new();
        for index in 0..16 {
            if hi[index] != 0 {
                unique_hi.entry(hi[index]).or_insert_with(|| Vec::new()).push(index);
            }
            if lo[index] != 0 {
                unique_lo.entry(lo[index]).or_insert_with(|| Vec::new()).push(index);
            }
        }
        // relabel (hi, lo) as (x, y) where x.len() <= 8
        let (unique_x, _unique_y, transposed) =
            if unique_hi.len() <= 8 {
                (unique_hi, unique_lo, false)
            } else if unique_lo.len() <= 8 {
                (unique_lo, unique_hi, true)
            } else {
                return None;
            };
        let mut shuffle_table_x = [0u8; 16];
        let mut shuffle_table_y = [0u8; 16];
        for (bit_i, (y_pattern, x_indexes)) in unique_x.iter().enumerate() {
            assert!(bit_i < 8);
            let bit = 1u8 << bit_i;
            for &x_index in x_indexes {
                shuffle_table_x[x_index] = bit;
            }
            for y_index in 0..16 {
                if (y_pattern & (1u16 << y_index)) != 0 {
                    shuffle_table_y[y_index] |= bit;
                }
            }
        }
        let (shuffle_table_hi, shuffle_table_lo) =
            if !transposed {
                (shuffle_table_x, shuffle_table_y)
            } else {
                (shuffle_table_y, shuffle_table_x)
            };
        Some(VectorClassifier { shuffle_table_lo, shuffle_table_hi })
    }

    pub fn classify_one(&self, i: u8) -> bool {
        self.shuffle_table_lo[(i & 0xF) as usize]
            & self.shuffle_table_hi[((i >> 4) & 0xF) as usize] != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_test(accept: &[bool], expect_constructible: bool) {
        match (VectorClassifier::new(accept), expect_constructible) {
            (Some(classifier), true) => {
                for i in 0u8..=255 {
                    assert_eq!(accept[i as usize], classifier.classify_one(i), "at index {}", i);
                }
            },
            (None, false) => (),
            (None, true) => panic!("Expected to be able to construct classifier"),
            (Some(_), false) => panic!("Expected not to be able to construct classifier"),
        }
    }

    #[test]
    fn const_false() {
        run_test(&[false; 256], true);
    }

    #[test]
    fn const_true() {
        run_test(&[true; 256], true);
    }

    #[test]
    fn pattern_1() {
        let accept: Vec<bool> = (0..256).map(|i| i % 2 == 0).collect();
        run_test(&accept[..], true);
    }

    #[test]
    fn pattern_2() {
        let accept: Vec<bool> = (0..256).map(|i| i >= 128).collect();
        run_test(&accept[..], true);
    }

    #[test]
    fn pattern_3() {
        let accept: Vec<bool> = (0..256).map(|i| i % 9 < 4).collect();
        run_test(&accept[..], false);
    }

    #[test]
    fn pattern_4() {
        let accept: Vec<bool> = (0u8..=255).map(|i| {
            "0123456789MKLF, \t\r\n".as_bytes().contains(&i)
        }).collect();
        run_test(&accept[..], true);
    }

}
