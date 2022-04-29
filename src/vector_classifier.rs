use std::collections::HashMap;
#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

#[derive(Clone, Debug)]
pub struct LookupTables {
    pub shuffle_table_lo: [u8; 16],
    pub shuffle_table_hi: [u8; 16],
}

impl LookupTables {
    pub fn new(accept: &[bool; 256]) -> Option<Self> {
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
        Some(Self { shuffle_table_lo, shuffle_table_hi })
    }

    pub fn from_accepting_chars(chars: &[u8]) -> Option<Self> {
        let mut accept = [false; 256];
        for &char_ in chars {
            accept[char_ as usize] = true;
        };
        Self::new(&accept)
    }
}

pub trait Classifier {
    /// Transforms the bytes in [in_out] so that it is non-zero if the original
    /// byte matched a character in the lookup table.
    fn classify(&self, in_out: &mut [u8]);
}

pub struct Generic {
    lookup_tables: LookupTables,
}

impl Generic {
    pub fn new(lookup_tables: LookupTables) -> Self {
        Self { lookup_tables: lookup_tables.clone() }
    }
}

impl Classifier for Generic {
    fn classify(&self, in_out: &mut [u8]) {
        for i in 0..in_out.len() {
            in_out[i] =
                self.lookup_tables.shuffle_table_lo[(in_out[i] & 0xF) as usize]
                & self.lookup_tables.shuffle_table_hi[((in_out[i] >> 4) & 0xF) as usize];
        }
    }
}

pub struct Ssse3 {
    generic: Generic,
    shuffle_table_lo: __m128i,
    shuffle_table_hi: __m128i,
}

impl Ssse3 {
    pub unsafe fn new(lookup_tables: LookupTables) -> Self {
        let generic = Generic::new(lookup_tables.clone());
        let shuffle_table_lo = _mm_loadu_si128(&lookup_tables.shuffle_table_lo as *const _ as *const __m128i);
        let shuffle_table_hi = _mm_loadu_si128(&lookup_tables.shuffle_table_hi as *const _ as *const __m128i);
        Self { generic, shuffle_table_lo, shuffle_table_hi }
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[target_feature(enable = "ssse3")]
    unsafe fn classify_ssse3(&self, in_out: &mut [__m128i]) {
        let lo_nibble_epi8 = _mm_set1_epi8(0xF);

        for i in 0..in_out.len() {
            in_out[i] =
                _mm_and_si128(
                    _mm_shuffle_epi8(self.shuffle_table_lo, _mm_and_si128(in_out[i], lo_nibble_epi8)),
                    _mm_shuffle_epi8(self.shuffle_table_hi, _mm_and_si128(_mm_srli_epi16(in_out[i], 4), lo_nibble_epi8)));
        }
    }

}

impl Classifier for Ssse3 {
    fn classify(&self, in_out: &mut [u8]) {
        let (prefix, aligned, suffix) =
            unsafe { in_out.align_to_mut::<__m128i>() };
        self.generic.classify(prefix);
        unsafe { self.classify_ssse3(aligned); }
        self.generic.classify(suffix);
    }
}

pub fn new_via_runtime_detection(lookup_tables: LookupTables) -> Box<dyn Classifier> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if is_x86_feature_detected!("ssse3") {
        unsafe {
            return Box::new(Ssse3::new(lookup_tables));
        }
    }
    return Box::new(Generic::new(lookup_tables));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_test(accept: &[bool; 256], expect_constructible: bool) {
        match (LookupTables::new(accept), expect_constructible) {
            (Some(lookup_tables), true) => {
                for i in 0u8..=255 {
                    let expected_result = accept[i as usize];
                    let generic_result = {
                        let classifier = Generic::new(lookup_tables.clone());
                        let mut in_out = [i; 1];
                        classifier.classify(&mut in_out);
                        in_out[0] != 0
                    };
                    let ssse3_result = unsafe {
                        let classifier = Ssse3::new(lookup_tables.clone());
                        let mut in_out = [i; 128];
                        classifier.classify(&mut in_out);
                        let results = in_out.map(|x| x != 0);
                        for result in results {
                            if result != results[0] {
                                panic!("Expected all bytes to be the same classification");
                            }
                        }
                        results[0]
                    };
                    assert_eq!(expected_result, generic_result, "at index {}", i);
                    assert_eq!(expected_result, ssse3_result, "at index {}", i);
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
        run_test(&accept.try_into().unwrap(), true);
    }

    #[test]
    fn pattern_2() {
        let accept: Vec<bool> = (0..256).map(|i| i >= 128).collect();
        run_test(&accept.try_into().unwrap(), true);
    }

    #[test]
    fn pattern_3() {
        let accept: Vec<bool> = (0..256).map(|i| i % 9 < 4).collect();
        run_test(&accept.try_into().unwrap(), false);
    }

    #[test]
    fn pattern_4() {
        let accept: Vec<bool> = (0u8..=255).map(|i| {
            "0123456789MKLF, \t\r\n".as_bytes().contains(&i)
        }).collect();
        run_test(&accept.try_into().unwrap(), true);
    }

    #[test]
    fn pattern_5() {
        let accept: Vec<bool> = (0u8..=255).map(|i| {
            " \t\r\n()\\\"".as_bytes().contains(&i)
        }).collect();
        run_test(&accept.try_into().unwrap(), true);
    }

}
