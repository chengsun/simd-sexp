#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

use crate::{clmul, xor_masked_adjacent, vector_classifier, utils, find_quote_transitions, ranges};
use vector_classifier::ClassifierBuilder;
use clmul::Clmul;

pub trait Classifier {
    /// Returns a bitmask for start/end of every unquoted atom; start/end of every quoted atom; parens
    fn structural_indices_bitmask(&mut self, input_buf: &[u8]) -> u64;
}

pub struct Avx2 {
    /* constants */
    clmul: clmul::Sse2Pclmulqdq,
    atom_terminator_classifier: vector_classifier::Avx2Classifier,
    xor_masked_adjacent: xor_masked_adjacent::Bmi2,

    /* varying */
    escape: bool,
    quote: bool,
    atom_like: bool,
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
struct ClassifyOneAvx2 {
    parens: __m256i,
    quote: __m256i,
    backslash: __m256i,
    atom_like: __m256i,
}

impl Avx2 {
    pub fn new(clmul: clmul::Sse2Pclmulqdq,
           vector_classifier_builder: vector_classifier::Avx2Builder,
           xor_masked_adjacent: xor_masked_adjacent::Bmi2)
           -> Self
    {

        let lookup_tables = vector_classifier::LookupTables::from_accepting_chars(b" \t\n()\"").unwrap();
        let atom_terminator_classifier = vector_classifier_builder.build(&lookup_tables);

        Self {
            clmul,
            atom_terminator_classifier,
            xor_masked_adjacent,
            escape: false,
            quote: false,
            atom_like: false,
        }
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[target_feature(enable = "avx2")]
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
    #[target_feature(enable = "avx2")]
    unsafe fn structural_indices_bitmask_avx2(&mut self, input_buf: &[u8]) -> u64 {
        let input_lo = _mm256_loadu_si256(input_buf[0..].as_ptr() as *const _);
        let input_hi = _mm256_loadu_si256(input_buf[32..].as_ptr() as *const _);

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
        /* print_bitmask(bm_backslash, 64); */
        let (escaped, escape_state) = ranges::odd_range_ends(bm_backslash, self.escape);
        self.escape = escape_state;

        /* print_bitmask(escaped, 64); */

        let escaped_quotes = bm_quote & escaped;
        let unescaped_quotes = bm_quote & !escaped;
        let prev_quote_state = self.quote;
        let (quote_transitions, quote_state) = find_quote_transitions::find_quote_transitions(&self.clmul, &self.xor_masked_adjacent, unescaped_quotes, escaped_quotes, self.quote);
        self.quote = quote_state;
        let quoted_areas = self.clmul.clmul(quote_transitions) ^ (if prev_quote_state { !0u64 } else { 0u64 });

        /* print_bitmask(unescaped_quotes, 64); */
        /* print_bitmask(escaped_quotes, 64); */
        /* print_bitmask(quote_transitions, 64); */
        /* print_bitmask(quoted_areas, 64); */

        let special = quote_transitions | (!quoted_areas & (bm_parens | ranges::range_transitions(bm_atom_like, self.atom_like)));

        self.atom_like = bm_atom_like >> 63 != 0;
        /* print_bitmask(special, 64); */

        special
    }
}

impl Classifier for Avx2 {
    fn structural_indices_bitmask(&mut self, input_buf: &[u8]) -> u64 {
        // TODO: do aligned split and use generic for edges
        unsafe {
            self.structural_indices_bitmask_avx2(input_buf)
        }
    }
}
