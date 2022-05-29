use crate::vector_classifier;

pub enum CallbackResult {
    Continue,
    Finish,
}

pub trait Classifier: Clone + Send {
    const NAME: &'static str;

    /// Returns a bitmask for start/end of every unquoted atom; start/end of every quoted atom; parens
    /// Consumes all of input_buf, up to 64 bytes at a time.
    /// Returns the bitmask as well as the number of bits that were consumed
    fn structural_indices_bitmask<F: FnMut(u64, usize) -> CallbackResult>
        (&mut self, input_buf: &[u8], f: F);
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Generic {
    escape: bool,
    quote_state: bool,
    atom_like: bool,
}

impl Generic {
    pub fn new() -> Self {
        Self {
            escape: false,
            quote_state: false,
            atom_like: false,
        }
    }

    #[inline(always)]
    fn structural_indices_bitmask_one(&mut self, input_buf: &[u8]) -> (u64, usize) {
        let chunk_len = std::cmp::min(64, input_buf.len());
        let mut result = 0u64;
        for (i, &ch) in input_buf[0..chunk_len].iter().enumerate() {
            let quote_state_change = ch == b'"' && !(self.quote_state && self.escape);
            let escape = ch == b'\\' && !self.escape;
            let atom_like = match ch {
                b'"' | b' ' | b'\n' | b'\t' | b'(' | b')' => false,
                _ => !self.quote_state,
            };
            let paren = match ch {
                b'(' | b')' => !self.quote_state,
                _ => false,
            };
            let atom_like_state_change = atom_like ^ self.atom_like;
            self.escape = escape;
            self.atom_like = atom_like;
            self.quote_state = self.quote_state ^ quote_state_change;
            if (self.quote_state && quote_state_change) || (!self.quote_state && atom_like_state_change) || paren {
                result = result | (1u64 << i);
            }
        }
        (result, chunk_len)
    }
}

impl Classifier for Generic {
    const NAME: &'static str = "Generic";

    #[inline(always)]
    fn structural_indices_bitmask<F: FnMut(u64, usize) -> CallbackResult>(&mut self, input_buf: &[u8], mut f: F) {
        for chunk in input_buf.chunks(64) {
            let (result, chunk_len) = self.structural_indices_bitmask_one(chunk);
            debug_assert!(chunk_len == chunk.len());
            match f(result, chunk_len) {
                CallbackResult::Continue => (),
                CallbackResult::Finish => { return; },
            }
        }
    }
}

pub fn not_atom_like_lookup_tables() -> vector_classifier::LookupTables {
    vector_classifier::LookupTables::from_accepting_chars(b" \t\n()\"").unwrap()
}

#[cfg(target_arch = "x86_64")]
mod x86 {
    use core::arch::x86_64::*;

    use crate::{clmul, vector_classifier, xor_masked_adjacent, utils, find_quote_transitions, ranges};
    use vector_classifier::ClassifierBuilder;
    use clmul::Clmul;

    use super::{Classifier, CallbackResult, Generic, not_atom_like_lookup_tables};

    #[derive(Clone, Debug)]
    pub struct Avx2 {
        /* constants */
        clmul: clmul::Sse2Pclmulqdq,
        atom_terminator_classifier: vector_classifier::Avx2Classifier,
        xor_masked_adjacent: xor_masked_adjacent::Bmi2,

        /* fallback */
        generic: Generic,

        /* varying */
        escape: bool,
        quote_state: bool,
        atom_like: bool,
    }

    struct ClassifyOneAvx2 {
        parens: __m256i,
        quote: __m256i,
        backslash: __m256i,
        atom_like: __m256i,
    }

    impl Avx2 {
        pub fn new() -> Option<Self> {
            let clmul = clmul::Sse2Pclmulqdq::new()?;
            let vector_classifier_builder = vector_classifier::Avx2Builder::new()?;
            let xor_masked_adjacent = xor_masked_adjacent::Bmi2::new()?;

            let lookup_tables = not_atom_like_lookup_tables();
            let atom_terminator_classifier = vector_classifier_builder.build(&lookup_tables);

            let generic = Generic::new();

            Some(Self {
                clmul,
                atom_terminator_classifier,
                xor_masked_adjacent,
                generic,
                escape: false,
                quote_state: false,
                atom_like: false,
            })
        }

        fn copy_state_from_generic(&mut self) {
            self.escape = self.generic.escape;
            self.quote_state = self.generic.quote_state;
            self.atom_like = self.generic.atom_like;
        }

        fn copy_state_to_generic(&mut self) {
            self.generic.escape = self.escape;
            self.generic.quote_state = self.quote_state;
            self.generic.atom_like = self.atom_like;
        }

        pub fn get_generic_state(&mut self) -> Generic {
            self.copy_state_to_generic();
            self.generic
        }

        #[target_feature(enable = "avx2,bmi2,sse2,ssse3,pclmulqdq")]
        #[inline]
        unsafe fn classify_one_avx2(&self, input: __m256i) -> ClassifyOneAvx2
        {
            let lparen = _mm256_cmpeq_epi8(input, _mm256_set1_epi8(b'(' as i8));
            let rparen = _mm256_cmpeq_epi8(input, _mm256_set1_epi8(b')' as i8));
            let quote = _mm256_cmpeq_epi8(input, _mm256_set1_epi8(b'"' as i8));
            let backslash = _mm256_cmpeq_epi8(input, _mm256_set1_epi8(b'\\' as i8));

            let parens = _mm256_or_si256(lparen, rparen);

            let mut atom_like = input.clone();
            self.atom_terminator_classifier.classify_avx2(std::slice::from_mut(&mut atom_like));
            let atom_like = _mm256_cmpeq_epi8(atom_like, _mm256_set1_epi8(0));

            ClassifyOneAvx2 {
                parens,
                quote,
                backslash,
                atom_like,
            }
        }

        #[target_feature(enable = "avx2,bmi2,sse2,ssse3,pclmulqdq")]
        #[inline]
        unsafe fn structural_indices_bitmask_one_avx2(&mut self, input_lo: __m256i, input_hi: __m256i) -> u64 {
            let classify_lo = self.classify_one_avx2(input_lo);
            let parens_lo = classify_lo.parens;
            let quote_lo = classify_lo.quote;
            let backslash_lo = classify_lo.backslash;
            let atom_like_lo = classify_lo.atom_like;

            let classify_hi = self.classify_one_avx2(input_hi);
            let parens_hi = classify_hi.parens;
            let quote_hi = classify_hi.quote;
            let backslash_hi = classify_hi.backslash;
            let atom_like_hi = classify_hi.atom_like;

            let bm_parens = utils::make_bitmask(parens_lo, parens_hi);
            let bm_quote = utils::make_bitmask(quote_lo, quote_hi);
            let bm_backslash = utils::make_bitmask(backslash_lo, backslash_hi);
            let bm_atom_like = utils::make_bitmask(atom_like_lo, atom_like_hi);
            let (escaped, escape_state) = ranges::odd_range_ends(bm_backslash, self.escape);
            self.escape = escape_state;

            let escaped_quotes = bm_quote & escaped;
            let unescaped_quotes = bm_quote & !escaped;
            let prev_quote_state = self.quote_state;
            let (quote_transitions, quote_state) = find_quote_transitions::find_quote_transitions(&self.clmul, &self.xor_masked_adjacent, unescaped_quotes, escaped_quotes, self.quote_state);
            self.quote_state = quote_state;
            let quoted_areas = self.clmul.clmul(quote_transitions) ^ (if prev_quote_state { !0u64 } else { 0u64 });

            let bm_atom_like = bm_atom_like & !quoted_areas;

            let special = (quote_transitions & quoted_areas) | (!quoted_areas & (bm_parens | ranges::range_transitions(bm_atom_like, self.atom_like)));

            self.atom_like = bm_atom_like >> 63 != 0;

            special
        }
    }

    impl Classifier for Avx2 {
        const NAME: &'static str = "AVX2";

        #[inline(always)]
        fn structural_indices_bitmask<F: FnMut(u64, usize) -> CallbackResult>(&mut self, input_buf: &[u8], mut f: F) {
            let (prefix, aligned, suffix) = unsafe { input_buf.align_to::<(__m256i, __m256i)>() };
            if utils::unlikely(prefix.len() > 0) {
                self.copy_state_to_generic();
                let (bitmask, len) = self.generic.structural_indices_bitmask_one(prefix);
                self.copy_state_from_generic();
                debug_assert!(len == prefix.len());
                match f(bitmask, len) {
                    CallbackResult::Continue => (),
                    CallbackResult::Finish => { return; },
                }
            }
            for (lo, hi) in aligned {
                unsafe {
                    let bitmask = self.structural_indices_bitmask_one_avx2(*lo, *hi);
                    match f(bitmask, 64) {
                        CallbackResult::Continue => (),
                        CallbackResult::Finish => { return; },
                    }
                }
            }
            if utils::unlikely(suffix.len() > 0) {
                self.copy_state_to_generic();
                let (bitmask, len) = self.generic.structural_indices_bitmask_one(suffix);
                self.copy_state_from_generic();
                debug_assert!(len == suffix.len());
                match f(bitmask, len) {
                    CallbackResult::Continue => (),
                    CallbackResult::Finish => { return; },
                }
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
pub use x86::*;

#[cfg(target_arch = "aarch64")]
mod aarch64 {
    use core::arch::aarch64::*;

    use crate::{clmul, vector_classifier, xor_masked_adjacent, utils, find_quote_transitions, ranges};
    use vector_classifier::ClassifierBuilder;
    use clmul::Clmul;

    use super::{Classifier, CallbackResult, Generic, not_atom_like_lookup_tables};

    #[derive(Clone, Debug)]
    pub struct Neon {
        /* constants */
        clmul: clmul::Neon,
        atom_terminator_classifier: vector_classifier::NeonClassifier,
        xor_masked_adjacent: xor_masked_adjacent::Generic,

        /* fallback */
        generic: Generic,

        /* varying */
        escape: bool,
        quote_state: bool,
        atom_like: bool,
    }

    struct ClassifyOneNeon {
        parens: uint8x16_t,
        quote: uint8x16_t,
        backslash: uint8x16_t,
        atom_like: uint8x16_t,
    }

    impl Neon {
        pub fn new() -> Option<Self> {
            let clmul = clmul::Neon::new()?;
            let vector_classifier_builder = vector_classifier::NeonBuilder::new()?;
            let xor_masked_adjacent = xor_masked_adjacent::Generic::new();

            let lookup_tables = not_atom_like_lookup_tables();
            let atom_terminator_classifier = vector_classifier_builder.build(&lookup_tables);

            let generic = Generic::new();

            Some(Self {
                clmul,
                atom_terminator_classifier,
                xor_masked_adjacent,
                generic,
                escape: false,
                quote_state: false,
                atom_like: false,
            })
        }

        fn copy_state_from_generic(&mut self) {
            self.escape = self.generic.escape;
            self.quote_state = self.generic.quote_state;
            self.atom_like = self.generic.atom_like;
        }

        fn copy_state_to_generic(&mut self) {
            self.generic.escape = self.escape;
            self.generic.quote_state = self.quote_state;
            self.generic.atom_like = self.atom_like;
        }

        pub fn get_generic_state(&mut self) -> Generic {
            self.copy_state_to_generic();
            self.generic
        }

        #[target_feature(enable = "neon,aes")]
        #[inline]
        unsafe fn classify_one_neon(&self, input: uint8x16_t) -> ClassifyOneNeon {
            use vector_classifier::Classifier;

            let lparen = vceqq_u8(input, vdupq_n_u8(b'('));
            let rparen = vceqq_u8(input, vdupq_n_u8(b')'));
            let quote = vceqq_u8(input, vdupq_n_u8(b'"'));
            let backslash = vceqq_u8(input, vdupq_n_u8(b'\\'));

            let parens = vorrq_u8(lparen, rparen);

            let atom_like = input.clone();
            self.atom_terminator_classifier.classify_neon(&mut [atom_like]);
            let atom_like = vceqq_u8(atom_like, vdupq_n_u8(0));

            ClassifyOneNeon {
                parens,
                quote,
                backslash,
                atom_like,
            }
        }

        #[target_feature(enable = "neon,aes")]
        #[inline]
        unsafe fn structural_indices_bitmask_one_neon(&mut self, input: &uint8x16x4_t) -> u64 {
            let ld4 = vld4q_u8(input as *const _ as *const u8);
            let classify_0 = self.classify_one_neon(ld4.0);
            let classify_1 = self.classify_one_neon(ld4.1);
            let classify_2 = self.classify_one_neon(ld4.2);
            let classify_3 = self.classify_one_neon(ld4.3);

            let bm_parens = utils::make_bitmask_ld4_interleaved(uint8x16x4_t(classify_0.parens, classify_1.parens, classify_2.parens, classify_3.parens));
            let bm_quote = utils::make_bitmask_ld4_interleaved(uint8x16x4_t(classify_0.quote, classify_1.quote, classify_2.quote, classify_3.quote));
            let bm_backslash = utils::make_bitmask_ld4_interleaved(uint8x16x4_t(classify_0.backslash, classify_1.backslash, classify_2.backslash, classify_3.backslash));
            let bm_atom_like = utils::make_bitmask_ld4_interleaved(uint8x16x4_t(classify_0.atom_like, classify_1.atom_like, classify_2.atom_like, classify_3.atom_like));
            let (escaped, escape_state) = ranges::odd_range_ends(bm_backslash, self.escape);
            self.escape = escape_state;

            let escaped_quotes = bm_quote & escaped;
            let unescaped_quotes = bm_quote & !escaped;
            let prev_quote_state = self.quote_state;
            let (quote_transitions, quote_state) = find_quote_transitions::find_quote_transitions(&self.clmul, &self.xor_masked_adjacent, unescaped_quotes, escaped_quotes, self.quote_state);
            self.quote_state = quote_state;
            let quoted_areas = self.clmul.clmul(quote_transitions) ^ (if prev_quote_state { !0u64 } else { 0u64 });

            let bm_atom_like = bm_atom_like & !quoted_areas;

            let special = (quote_transitions & quoted_areas) | (!quoted_areas & (bm_parens | ranges::range_transitions(bm_atom_like, self.atom_like)));

            self.atom_like = bm_atom_like >> 63 != 0;

            special
        }
    }

    impl Classifier for Neon {
        const NAME: &'static str = "NEON";

        #[inline(always)]
        fn structural_indices_bitmask<F: FnMut(u64, usize) -> CallbackResult>(&mut self, input_buf: &[u8], mut f: F) {
            let (prefix, aligned, suffix) = unsafe { input_buf.align_to::<uint8x16x4_t>() };
            if utils::unlikely(prefix.len() > 0) {
                self.copy_state_to_generic();
                let (bitmask, len) = self.generic.structural_indices_bitmask_one(prefix);
                self.copy_state_from_generic();
                debug_assert!(len == prefix.len());
                match f(bitmask, len) {
                    CallbackResult::Continue => (),
                    CallbackResult::Finish => { return; },
                }
            }
            for input in aligned {
                unsafe {
                    let bitmask = self.structural_indices_bitmask_one_neon(input);
                    match f(bitmask, 64) {
                        CallbackResult::Continue => (),
                        CallbackResult::Finish => { return; },
                    }
                }
            }
            if utils::unlikely(suffix.len() > 0) {
                self.copy_state_to_generic();
                let (bitmask, len) = self.generic.structural_indices_bitmask_one(suffix);
                self.copy_state_from_generic();
                debug_assert!(len == suffix.len());
                match f(bitmask, len) {
                    CallbackResult::Continue => (),
                    CallbackResult::Finish => { return; },
                }
            }
        }
    }
}

#[cfg(target_arch = "aarch64")]
pub use aarch64::*;

pub trait MakeClassifierCps<'a> {
    type Return;
    fn f<ClassifierT: Classifier + 'a>(self: Self, classifier: ClassifierT) -> Self::Return;
}

pub fn make_classifier_cps<'a, Cps: MakeClassifierCps<'a>>(cps: Cps) -> Cps::Return {
    #[cfg(target_arch = "x86_64")]
    {
        match Avx2::new() {
            Some(classifier) => {
                return cps.f(classifier);
            },
            None => (),
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        match Neon::new() {
            Some(classifier) => {
                return cps.f(classifier);
            },
            None => (),
        }
    }

    cps.f(Generic::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::*;

    trait Testable {
        fn run_test(&mut self, input: &[u8], output: &[bool]);
    }

    impl<T: Classifier> Testable for T {
        fn run_test(&mut self, input: &[u8], output: &[bool]) {
            let mut actual_output: Vec<bool> = Vec::new();
            let mut lens = Vec::new();
            self.structural_indices_bitmask(input, |bitmask, bitmask_len| {
                lens.push(bitmask_len);
                for i in 0..bitmask_len {
                    actual_output.push(bitmask & (1 << i) != 0);
                }
                CallbackResult::Continue
            });
            if output != actual_output {
                println!("input:      [{}]", String::from_utf8(input.iter().map(|ch| match ch {
                    b'\n' => b'N',
                    b'\t' => b'T',
                    b'\0' => b'0',
                    _ => *ch,
                }).collect()).unwrap());
                print!("expect out: ");
                print_bool_bitmask(output);
                print!("actual out: ");
                print_bool_bitmask(&actual_output[..]);
                println!("lens: {:?}", lens);
                panic!("structural test failed for {}", Self::NAME);
            }
        }
    }


    fn run_test(input: &[u8], output: &[bool]) {
        let mut generic = Generic::new();
        generic.run_test(input, output);

        #[cfg(target_arch = "x86_64")]
        match Avx2::new() {
            Some(mut avx2) => {
                avx2.run_test(input, output);
                assert_eq!(generic, avx2.get_generic_state());
            },
            None => (),
        }

        #[cfg(target_arch = "aarch64")]
        match Neon::new() {
            Some(mut avx2) => {
                avx2.run_test(input, output);
                assert_eq!(generic, avx2.get_generic_state());
            },
            None => (),
        }
    }

    #[test]
    fn test_1() {
        run_test(br#""foo""#, &[true, false, false, false, false]);
    }

    #[repr(align(64))]
    struct TestInput<T>(T);


    #[test]
    fn test_2() {
        let input_1 = [b'"'];
        let mut input_2 = TestInput([b' '; 64]);
        input_2.0[0] = b'"';
        input_2.0[1] = b'(';
        let mut expected_output = [false; 65];
        expected_output[0] = true;
        expected_output[2] = true;

        {
            let mut generic = Generic::new();
            let mut output: Vec<bool> = Vec::new();
            for input in [&input_1[..], &input_2.0[..]] {
                generic.structural_indices_bitmask(&input[..], |bitmask, bitmask_len| {
                    for i in 0..bitmask_len {
                        output.push(bitmask & (1 << i) != 0);
                    }
                    CallbackResult::Finish
                });
            }
            assert_eq!(output, expected_output);
        }

        #[cfg(target_arch = "x86_64")]
        {
            let mut avx2 = Avx2::new().unwrap();
            let mut output: Vec<bool> = Vec::new();
            for input in [&input_1[..], &input_2.0[..]] {
                avx2.structural_indices_bitmask(&input[..], |bitmask, bitmask_len| {
                    for i in 0..bitmask_len {
                        output.push(bitmask & (1 << i) != 0);
                    }
                    CallbackResult::Finish
                });
            }
            assert_eq!(output, expected_output);
        }

        #[cfg(target_arch = "aarch64")]
        {
            let mut neon = Neon::new().unwrap();
            let mut output: Vec<bool> = Vec::new();
            for input in [&input_1[..], &input_2.0[..]] {
                neon.structural_indices_bitmask(&input[..], |bitmask, bitmask_len| {
                    for i in 0..bitmask_len {
                        output.push(bitmask & (1 << i) != 0);
                    }
                    CallbackResult::Finish
                });
            }
            assert_eq!(output, expected_output);
        }
    }

    //#[test]
    fn test_random() {
        use rand::{prelude::Distribution, SeedableRng};

        let mut rng = rand::rngs::StdRng::seed_from_u64(0);

        let chars = b"() \n\"\\a";
        let random_char = rand::distributions::Uniform::new(0, chars.len()).map(|i| chars[i]);
        let random_alignment = rand::distributions::Uniform::new(0, 64);

        for _ in 0..1000 {
            let mut input_bufs = [TestInput([0u8; 192]), TestInput([0u8; 192])];
            let inputs: Vec<&[u8]> = input_bufs.iter_mut().map(|input_buf| {
                let alignment = random_alignment.sample(&mut rng);
                let input = &mut input_buf.0[alignment..];
                for i in 0..input.len() {
                    input[i] = random_char.sample(&mut rng);
                }
                &*input
            }).collect();
            let generic_output = {
                let mut generic = Generic::new();
                let mut output: Vec<bool> = Vec::new();
                for input in inputs.iter() {
                    generic.structural_indices_bitmask(&input[..], |bitmask, bitmask_len| {
                        for i in 0..bitmask_len {
                            output.push(bitmask & (1 << i) != 0);
                        }
                        CallbackResult::Continue
                    });
                }
                output
            };

            #[cfg(target_arch = "x86_64")]
            {
                let mut avx2 = Avx2::new().unwrap();
                let mut output: Vec<bool> = Vec::new();
                for input in inputs {
                    avx2.structural_indices_bitmask(&input[..], |bitmask, bitmask_len| {
                        for i in 0..bitmask_len {
                            output.push(bitmask & (1 << i) != 0);
                        }
                        CallbackResult::Continue
                    });
                }
                assert_eq!(generic_output, output);
            }

            #[cfg(target_arch = "aarch64")]
            {
                let mut neon = Neon::new().unwrap();
                let mut output: Vec<bool> = Vec::new();
                for input in inputs {
                    neon.structural_indices_bitmask(&input[..], |bitmask, bitmask_len| {
                        for i in 0..bitmask_len {
                            output.push(bitmask & (1 << i) != 0);
                        }
                        CallbackResult::Continue
                    });
                }
                assert_eq!(generic_output, output);
            }
        }
    }
}
