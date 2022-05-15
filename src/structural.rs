#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

use crate::{clmul, xor_masked_adjacent, vector_classifier, utils, find_quote_transitions, ranges};
use vector_classifier::ClassifierBuilder;
use clmul::Clmul;

pub enum CallbackResult {
    Continue,
    Finish,
}

pub trait Classifier {
    const NAME: &'static str;

    /// Returns a bitmask for start/end of every unquoted atom; start/end of every quoted atom; parens
    /// Consumes up to 64 bytes
    /// Returns the bitmask as well as the number of bits that were consumed
    fn structural_indices_bitmask<F: FnMut(u64, usize) -> CallbackResult>
        (&mut self, input_buf: &[u8], f: F);
}

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

    fn structural_indices_bitmask<F: FnMut(u64, usize) -> CallbackResult>(&mut self, input_buf: &[u8], mut f: F) {
        for chunk in input_buf.chunks(64) {
            let (result, chunk_len) = self.structural_indices_bitmask_one(chunk);
            assert!(chunk_len == chunk.len());
            match f(result, chunk_len) {
                CallbackResult::Continue => (),
                CallbackResult::Finish => { return; },
            }
        }
    }
}

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

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
struct ClassifyOneAvx2 {
    parens: __m256i,
    quote: __m256i,
    backslash: __m256i,
    atom_like: __m256i,
}

pub fn not_atom_like_lookup_tables() -> vector_classifier::LookupTables {
    vector_classifier::LookupTables::from_accepting_chars(b" \t\n()\"").unwrap()
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

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[target_feature(enable = "avx2,sse2,ssse3,pclmulqdq")]
    unsafe fn classify_one_avx2(&self, input: __m256i) -> ClassifyOneAvx2
    {
        let lparen = _mm256_cmpeq_epi8(input, _mm256_set1_epi8('(' as i8));
        let rparen = _mm256_cmpeq_epi8(input, _mm256_set1_epi8(')' as i8));
        let quote = _mm256_cmpeq_epi8(input, _mm256_set1_epi8('"' as i8));
        let backslash = _mm256_cmpeq_epi8(input, _mm256_set1_epi8('\\' as i8));

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

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[target_feature(enable = "avx2,sse2,ssse3,pclmulqdq")]
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

    fn structural_indices_bitmask<F: FnMut(u64, usize) -> CallbackResult>(&mut self, input_buf: &[u8], mut f: F) {
        let (prefix, aligned, suffix) = unsafe { input_buf.align_to::<(__m256i, __m256i)>() };
        if utils::unlikely(prefix.len() > 0) {
            self.copy_state_to_generic();
            let (bitmask, len) = self.generic.structural_indices_bitmask_one(prefix);
            assert!(len == prefix.len());
            match f(bitmask, len) {
                CallbackResult::Continue => (),
                CallbackResult::Finish => { return; },
            }
            self.copy_state_from_generic();
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
            assert!(len == suffix.len());
            match f(bitmask, len) {
                CallbackResult::Continue => (),
                CallbackResult::Finish => { return; },
            }
            self.copy_state_from_generic();
        }
    }
}

#[cfg(test)]
mod structural_tests {
    use rand::prelude::Distribution;

    use super::*;
    use crate::utils::*;

    trait Testable {
        fn run_test(self: Self, input: &[u8], output: &[bool]);
    }

    impl<T: Classifier> Testable for T {
        fn run_test(mut self: Self, input: &[u8], output: &[bool]) {
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
        let generic = Generic::new();
        generic.run_test(input, output);

        match Avx2::new() {
            Some(classifier) => classifier.run_test(input, output),
            None => (),
        }
    }

    #[test]
    fn test_1() {
        run_test(br#""foo""#, &[true, false, false, false, false]);
    }

    #[repr(align(64))]
    struct TestInput([u8; 128]);

    #[test]
    fn test_random() {
        let chars = b"() \n\t\"\\.";
        let random_char = rand::distributions::Uniform::new(0, chars.len()).map(|i| chars[i]);

        for iteration in 0..1000 {
            let mut input = TestInput([0u8; 128]);
            let alignment = iteration % 64;
            let input = &mut input.0[alignment..];
            for i in 0..input.len() {
                input[i] = random_char.sample(&mut rand::thread_rng());
            }
            let generic_output = {
                let mut generic = Generic::new();
                let mut output: Vec<bool> = Vec::new();
                generic.structural_indices_bitmask(&input[..], |bitmask, bitmask_len| {
                    for i in 0..bitmask_len {
                        output.push(bitmask & (1 << i) != 0);
                    }
                    CallbackResult::Continue
                });
                output
            };
            run_test(&input[..], &generic_output[..]);
        }
    }
}
