use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct LookupTables {
    pub shuffle_table_lo: [u8; 16],
    pub shuffle_table_hi: [u8; 16],
}

impl LookupTables {
    pub fn empty() -> Self {
        Self {
            shuffle_table_lo: [0u8; 16],
            shuffle_table_hi: [0u8; 16],
        }
    }

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

#[derive(Clone, Debug)]
pub struct GenericClassifier {
    lookup_tables: LookupTables,
}

impl GenericClassifier {
    fn new(lookup_tables: &LookupTables) -> Self {
        Self { lookup_tables: lookup_tables.clone() }
    }
}

impl Classifier for GenericClassifier {
    fn classify(&self, in_out: &mut [u8]) {
        for i in 0..in_out.len() {
            in_out[i] =
                self.lookup_tables.shuffle_table_lo[(in_out[i] & 0xF) as usize]
                & self.lookup_tables.shuffle_table_hi[((in_out[i] >> 4) & 0xF) as usize];
        }
    }
}

pub trait ClassifierBuilder {
    type Classifier;
    fn build(&self, lookup_tables: &LookupTables) -> Self::Classifier;
}

pub struct GenericBuilder {}

impl GenericBuilder {
    pub fn new() -> Self {
        GenericBuilder {}
    }
}

impl ClassifierBuilder for GenericBuilder {
    type Classifier = GenericClassifier;
    fn build(&self, lookup_tables: &LookupTables) -> Self::Classifier {
        GenericClassifier::new(lookup_tables)
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod x86 {
    #[cfg(target_arch = "x86")]
    use core::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::*;

    use super::{Classifier, ClassifierBuilder, GenericClassifier, LookupTables};

    #[derive(Clone, Debug)]
    pub struct Ssse3Classifier {
        generic: GenericClassifier,
        shuffle_table_lo: __m128i,
        shuffle_table_hi: __m128i,
        _feature_detected_witness: (),
    }

    impl Ssse3Classifier {
        #[target_feature(enable = "sse2,ssse3")]
        pub unsafe fn new(lookup_tables: &LookupTables) -> Self {
            let generic = GenericClassifier::new(lookup_tables);
            let shuffle_table_lo = _mm_loadu_si128(&generic.lookup_tables.shuffle_table_lo as *const _ as *const __m128i);
            let shuffle_table_hi = _mm_loadu_si128(&generic.lookup_tables.shuffle_table_hi as *const _ as *const __m128i);
            return Self { generic, shuffle_table_lo, shuffle_table_hi, _feature_detected_witness: () };
        }

        #[target_feature(enable = "sse2,ssse3")]
        pub unsafe fn classify_ssse3(&self, in_out: &mut [__m128i]) {
            let lo_nibble_epi8 = _mm_set1_epi8(0xF);

            for i in 0..in_out.len() {
                in_out[i] =
                    _mm_and_si128(
                        _mm_shuffle_epi8(self.shuffle_table_lo, _mm_and_si128(in_out[i], lo_nibble_epi8)),
                        _mm_shuffle_epi8(self.shuffle_table_hi, _mm_and_si128(_mm_srli_epi16(in_out[i], 4), lo_nibble_epi8)));
            }
        }
    }

    impl Classifier for Ssse3Classifier {
        fn classify(&self, in_out: &mut [u8]) {
            let (prefix, aligned, suffix) = unsafe { in_out.align_to_mut::<__m128i>() };
            self.generic.classify(prefix);
            unsafe { self.classify_ssse3(aligned); }
            self.generic.classify(suffix);
        }
    }

    #[derive(Clone, Debug)]
    pub struct Avx2Classifier {
        generic: GenericClassifier,
        shuffle_table_lo: __m256i,
        shuffle_table_hi: __m256i,
        _feature_detected_witness: (),
    }

    impl Avx2Classifier {
        #[target_feature(enable = "avx,avx2")]
        pub unsafe fn new(lookup_tables: &LookupTables) -> Self {
            let generic = GenericClassifier::new(lookup_tables);
            let shuffle_table_lo_128 = _mm_loadu_si128(&generic.lookup_tables.shuffle_table_lo as *const _ as *const __m128i);
            let shuffle_table_lo = _mm256_inserti128_si256(_mm256_castsi128_si256(shuffle_table_lo_128), shuffle_table_lo_128, 1);
            let shuffle_table_hi_128 = _mm_loadu_si128(&generic.lookup_tables.shuffle_table_hi as *const _ as *const __m128i);
            let shuffle_table_hi = _mm256_inserti128_si256(_mm256_castsi128_si256(shuffle_table_hi_128), shuffle_table_hi_128, 1);
            return Self { generic, shuffle_table_lo, shuffle_table_hi, _feature_detected_witness: () };
        }

        #[target_feature(enable = "avx,avx2")]
        #[inline]
        pub unsafe fn classify_avx2(&self, in_out: &mut [__m256i]) {
            let lo_nibble_epi8 = _mm256_set1_epi8(0xF);

            for i in 0..in_out.len() {
                in_out[i] =
                    _mm256_and_si256(
                        _mm256_shuffle_epi8(self.shuffle_table_lo, _mm256_and_si256(in_out[i], lo_nibble_epi8)),
                        _mm256_shuffle_epi8(self.shuffle_table_hi, _mm256_and_si256(_mm256_srli_epi16(in_out[i], 4), lo_nibble_epi8)));
            }
        }
    }

    impl Classifier for Avx2Classifier {
        fn classify(&self, in_out: &mut [u8]) {
            let (prefix, aligned, suffix) = unsafe { in_out.align_to_mut::<__m256i>() };
            self.generic.classify(prefix);
            unsafe { self.classify_avx2(aligned); }
            self.generic.classify(suffix);
        }
    }

    pub struct Ssse3Builder {
        _feature_detected_witness: (),
    }

    impl Ssse3Builder {
        pub fn new() -> Option<Self> {
            if is_x86_feature_detected!("sse2") && is_x86_feature_detected!("ssse3") {
                return Some(Ssse3Builder { _feature_detected_witness: () });
            }
            None
        }
    }

    impl ClassifierBuilder for Ssse3Builder {
        type Classifier = Ssse3Classifier;
        fn build(&self, lookup_tables: &LookupTables) -> Self::Classifier {
            let _ = self._feature_detected_witness;
            unsafe { Ssse3Classifier::new(lookup_tables) }
        }
    }

    pub struct Avx2Builder {
        _feature_detected_witness: (),
    }

    impl Avx2Builder {
        pub fn new() -> Option<Self> {
            if is_x86_feature_detected!("sse2") && is_x86_feature_detected!("avx2") {
                return Some(Avx2Builder { _feature_detected_witness: () });
            }
            None
        }
    }

    impl ClassifierBuilder for Avx2Builder {
        type Classifier = Avx2Classifier;
        fn build(&self, lookup_tables: &LookupTables) -> Self::Classifier {
            let _ = self._feature_detected_witness;
            unsafe { Avx2Classifier::new(lookup_tables) }
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub use x86::*;

pub struct RuntimeDetectBuilder {}

impl RuntimeDetectBuilder {
    pub fn new() -> Self { RuntimeDetectBuilder {} }
}

impl ClassifierBuilder for RuntimeDetectBuilder {
    type Classifier = Box<dyn Classifier>;
    fn build(&self, lookup_tables: &LookupTables) -> Self::Classifier {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        match Avx2Builder::new () {
            None => (),
            Some(builder) => { return Box::new(builder.build(lookup_tables)); }
        }
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        match Ssse3Builder::new () {
            None => (),
            Some(builder) => { return Box::new(builder.build(lookup_tables)); }
        }
        Box::new(GenericBuilder::new().build(lookup_tables))
    }
}

pub fn runtime_detect() -> RuntimeDetectBuilder {
    RuntimeDetectBuilder::new()
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
                        let classifier = GenericBuilder::new().build(&lookup_tables);
                        let mut in_out = [i; 1];
                        classifier.classify(&mut in_out);
                        in_out[0] != 0
                    };
                    assert_eq!(expected_result, generic_result, "at index {}", i);


                    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                    match Ssse3Builder::new() {
                        None => (),
                        Some(classifier_builder) => {
                            let classifier = classifier_builder.build(&lookup_tables);
                            let mut in_out = [i; 128];
                            classifier.classify(&mut in_out);
                            let results = in_out.map(|x| x != 0);
                            for result in results {
                                if result != results[0] {
                                    panic!("Expected all bytes to be the same classification");
                                }
                            }
                            let ssse3_result = results[0];
                            assert_eq!(expected_result, ssse3_result, "at index {}", i);
                        }
                    }


                    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                    match Avx2Builder::new() {
                        None => (),
                        Some(classifier_builder) => {
                            let classifier = classifier_builder.build(&lookup_tables);
                            let mut in_out = [i; 128];
                            classifier.classify(&mut in_out);
                            let results = in_out.map(|x| x != 0);
                            for result in results {
                                if result != results[0] {
                                    panic!("Expected all bytes to be the same classification");
                                }
                            }
                            let ssse3_result = results[0];
                            assert_eq!(expected_result, ssse3_result, "at index {}", i);
                        }
                    }
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
