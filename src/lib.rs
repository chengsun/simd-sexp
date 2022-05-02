pub mod clmul;
pub mod extract;
pub mod find_quote_transitions;
pub mod ranges;
pub mod start_stop_transitions;
pub mod utils;
pub mod vector_classifier;
pub mod xor_masked_adjacent;

#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

use std::convert::TryInto;
use std::slice;
use ocaml::bigarray;

use crate::find_quote_transitions::*;
use crate::ranges::*;
use crate::utils::*;

struct State<ClmulT, VectorClassifierT, XorMaskedAdjacentT> {
    /* constants */
    clmul: ClmulT,
    whitespace_classifier: VectorClassifierT,
    xor_masked_adjacent: XorMaskedAdjacentT,

    /* varying */
    escape: bool,
    quote: bool,
    bm_atom: u64,
    bm_whitespace: u64,
}

impl<ClmulT: clmul::Clmul,
     VectorClassifierT: vector_classifier::Classifier,
     XorMaskedAdjacentT: xor_masked_adjacent::XorMaskedAdjacent>
    State<ClmulT, VectorClassifierT, XorMaskedAdjacentT> {
        fn new<VectorClassifierBuilderT: vector_classifier::ClassifierBuilder<Classifier = VectorClassifierT>>
            (clmul: ClmulT, vector_classifier_builder: VectorClassifierBuilderT, xor_masked_adjacent: XorMaskedAdjacentT) -> Self {
        let lookup_tables = vector_classifier::LookupTables::from_accepting_chars(b" \t\n").unwrap();
        let whitespace_classifier = vector_classifier_builder.build(&lookup_tables);

        Self {
            clmul,
            whitespace_classifier,
            xor_masked_adjacent,
            escape: false,
            quote: false,
            bm_atom: 0u64,
            bm_whitespace: 0u64,
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
struct ClassifyOneAvx2 {
    parens: __m256i,
    quote: __m256i,
    backslash: __m256i,
    whitespace: __m256i,
    other: __m256i,
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn classify_one_avx2 (input: __m256i) -> ClassifyOneAvx2 {
    let lparen = _mm256_cmpeq_epi8(input, _mm256_set1_epi8('(' as i8));
    let rparen = _mm256_cmpeq_epi8(input, _mm256_set1_epi8(')' as i8));
    let quote = _mm256_cmpeq_epi8(input, _mm256_set1_epi8('"' as i8));
    let backslash = _mm256_cmpeq_epi8(input, _mm256_set1_epi8('\\' as i8));

    let space = _mm256_cmpeq_epi8(input, _mm256_set1_epi8(' ' as i8));
    let tab = _mm256_cmpeq_epi8(input, _mm256_set1_epi8('\t' as i8));
    let newline = _mm256_cmpeq_epi8(input, _mm256_set1_epi8('\n' as i8));
    let whitespace = _mm256_set1_epi8(0x00);
    let whitespace = _mm256_or_si256(whitespace, space);
    let whitespace = _mm256_or_si256(whitespace, tab);
    let whitespace = _mm256_or_si256(whitespace, newline);

    let parens = _mm256_or_si256(lparen, rparen);

    let other = _mm256_set1_epi8(0xFFu8 as i8);
    let other = _mm256_andnot_si256(lparen, other);
    let other = _mm256_andnot_si256(rparen, other);
    let other = _mm256_andnot_si256(quote, other);
    let other = _mm256_andnot_si256(whitespace, other);

    ClassifyOneAvx2 {
        parens,
        quote,
        backslash,
        whitespace,
        other,
    }
}

// Returns a bitmask for start/end of every unquoted atom; start/end of every quoted atom; parens
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn structural_indices_bitmask<ClmulT, VectorClassifierT, XorMaskedAdjacentT>(input_buf: &[u8], state: &mut State<ClmulT, VectorClassifierT, XorMaskedAdjacentT>) -> u64
    where ClmulT: clmul::Clmul, XorMaskedAdjacentT: xor_masked_adjacent::XorMaskedAdjacent
{
    let input_lo = _mm256_loadu_si256(input_buf[0..].as_ptr() as *const _);
    let input_hi = _mm256_loadu_si256(input_buf[32..].as_ptr() as *const _);

    let classify_lo = classify_one_avx2(input_lo);
    let parens_lo = classify_lo.parens;
    let quote_lo = classify_lo.quote;
    let backslash_lo = classify_lo.backslash;
    let whitespace_lo = classify_lo.whitespace;
    let other_lo = classify_lo.other;

    let classify_hi = classify_one_avx2(input_hi);
    let parens_hi = classify_hi.parens;
    let quote_hi = classify_hi.quote;
    let backslash_hi = classify_hi.backslash;
    let whitespace_hi = classify_hi.whitespace;
    let other_hi = classify_hi.other;

    let bm_other = make_bitmask(other_lo, other_hi);
    let bm_whitespace = make_bitmask(whitespace_lo, whitespace_hi);

    let parens_bitmask = make_bitmask(parens_lo, parens_hi);
    let quote_bitmask = make_bitmask(quote_lo, quote_hi);

    let bm_backslash = make_bitmask(backslash_lo, backslash_hi);
    /* print_bitmask(bm_backslash, 64); */
    let (escaped, escape_state) = odd_range_ends(bm_backslash, state.escape);
    state.escape = escape_state;

    /* print_bitmask(escaped, 64); */

    let escaped_quotes = quote_bitmask & escaped;
    let unescaped_quotes = quote_bitmask & !escaped;
    let prev_quote_state = state.quote;
    let (quote_transitions, quote_state) = find_quote_transitions(&state.clmul, &state.xor_masked_adjacent, unescaped_quotes, escaped_quotes, state.quote);
    state.quote = quote_state;
    let quoted_areas = state.clmul.clmul(quote_transitions) ^ (if prev_quote_state { !0u64 } else { 0u64 });

    /* print_bitmask(unescaped_quotes, 64); */
    /* print_bitmask(escaped_quotes, 64); */
    /* print_bitmask(quote_transitions, 64); */
    /* print_bitmask(quoted_areas, 64); */

    let bm_atom = bm_other | bm_backslash;

    let special = quote_transitions | (!quoted_areas & (parens_bitmask | range_transitions(bm_atom, state.bm_atom)));

    state.bm_atom = bm_atom;
    state.bm_whitespace = bm_whitespace;
    /* print_bitmask(special, 64); */

    special
}

pub fn extract_structural_indices(input: &[u8], output: &mut [usize], start_offset: usize) -> usize {
    let n = input.len();

    let clmul = clmul::runtime_detect();
    let vector_classifier_builder = vector_classifier::runtime_detect();
    let xor_masked_adjacent = xor_masked_adjacent::runtime_detect();
    let mut state = State::new(clmul, vector_classifier_builder, xor_masked_adjacent);

    let mut output_write = 0;

    assert!(n % 64 == 0);
    assert!(output.len() >= n);

    let mut i = 0;
    while i < n {
        let bitmask = unsafe { structural_indices_bitmask(&input[i..], &mut state) };

        output_write += extract::fast(&mut output[output_write..], start_offset + i, bitmask);

        i += 64;
    }

    output_write
}

#[ocaml::func]
pub fn ml_extract_structural_indices(input: bigarray::Array1<u8>, mut output: bigarray::Array1<i64>, start_offset: i64) -> i64 {
    let input = input.data();
    let output =
        if cfg!(target_pointer_width = "64") {
            let data = output.data_mut();
            unsafe { slice::from_raw_parts_mut(data.as_mut_ptr() as *mut usize, data.len()) }
        } else {
            unimplemented!()
        };
    let start_offset =
        if cfg!(target_pointer_width = "64") {
            start_offset as usize
        } else {
            unimplemented!()
        };
    let result = extract_structural_indices(input, output, start_offset);
    result.try_into().unwrap()
}
