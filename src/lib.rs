pub mod clmul;
pub mod extract;
pub mod escape;
pub mod find_quote_transitions;
pub mod ranges;
pub mod sexp_structure;
pub mod start_stop_transitions;
pub mod utils;
pub mod vector_classifier;
pub mod xor_masked_adjacent;

pub fn extract_structural_indices<OutF: FnMut(usize) -> ()>(input: &[u8], mut output: OutF) {
    use sexp_structure::Classifier;

    let n = input.len();

    // TODO
    let clmul = clmul::Sse2Pclmulqdq::new().unwrap();
    let vector_classifier_builder = vector_classifier::Avx2Builder::new().unwrap();
    let xor_masked_adjacent = xor_masked_adjacent::Bmi2::new().unwrap();
    let mut sexp_structure_classifier = sexp_structure::Avx2::new(clmul, vector_classifier_builder, xor_masked_adjacent);

    assert!(n % 64 == 0);

    let mut i = 0;
    while i < n {
        let bitmask = sexp_structure_classifier.structural_indices_bitmask(&input[i..]);
        extract::safe_generic(|bit_offset| { output(i + bit_offset) }, bitmask);
        i += 64;
    }
}

#[ocaml::func(runtime)]
pub fn ml_extract_structural_indices(
    input: &[u8],
    mut output: ocaml::Array<ocaml::Uint>,
    mut output_index: ocaml::Uint,
    start_offset: ocaml::Uint)
    -> ocaml::Uint
{
    assert!(output.len() >= output_index + input.len());
    extract_structural_indices(input, |bit_offset| {
        unsafe { output.set_unchecked(runtime, output_index, start_offset + bit_offset); }
        output_index += 1;
    });
    output_index
}

#[ocaml::func]
pub fn ml_unescape(input: &[u8], pos: ocaml::Uint, len: ocaml::Uint, output: &mut [u8]) -> ocaml::Int {
    use escape::Unescape;

    let input = &input[pos..pos+len];

    // TODO: nongeneric version
    let unescape = escape::GenericUnescape::new();
    match unescape.unescape(input, output) {
        None => -1,
        Some(output_len) => output_len.try_into().unwrap()
    }
}
