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

// Needs to be in a Box<> for alignment guarantees (which the OCaml GC cannot
// provide when it's relocating stuff)
pub struct ExtractStructuralIndicesState(Box<sexp_structure::Avx2>);

ocaml::custom! (ExtractStructuralIndicesState);

#[ocaml::func]
pub fn ml_extract_structural_indices_create_state(_unit: ocaml::Value) -> ExtractStructuralIndicesState {
    // TODO
    let clmul = clmul::Sse2Pclmulqdq::new().unwrap();
    let vector_classifier_builder = vector_classifier::Avx2Builder::new().unwrap();
    let xor_masked_adjacent = xor_masked_adjacent::Bmi2::new().unwrap();
    ExtractStructuralIndicesState(Box::new(sexp_structure::Avx2::new(clmul, vector_classifier_builder, xor_masked_adjacent)))
}

#[ocaml::func(runtime)]
pub fn ml_extract_structural_indices(
    mut extract_structural_indices_state: ocaml::Pointer<ExtractStructuralIndicesState>,
    input: &[u8],
    mut input_index: ocaml::Uint,
    mut output: ocaml::Array<ocaml::Uint>,
    mut output_index: ocaml::Uint)
    -> (ocaml::Uint, ocaml::Uint)
{
    use sexp_structure::Classifier;

    while input_index + 64 <= input.len() && output_index + 64 <= output.len() {
        let bitmask = extract_structural_indices_state.as_mut().0.structural_indices_bitmask(&input[input_index..]);

        extract::safe_generic(|bit_offset| {
            unsafe {
                output.set_unchecked(runtime, output_index, input_index + bit_offset);
            }
            output_index += 1;
        }, bitmask);

        input_index += 64;
    }

    (input_index, output_index)
}

struct ByteString(Vec<u8>);

unsafe impl ocaml::IntoValue for ByteString {
    fn into_value(self, _rt: &ocaml::Runtime) -> ocaml::Value {
        unsafe { ocaml::Value::bytes(&self.0[..]) }
    }
}


#[ocaml::func]
pub fn ml_unescape(input: &[u8], pos: ocaml::Uint, len: ocaml::Uint) -> Option<ByteString> {
    use escape::Unescape;

    let input = &input[pos..pos+len];
    let mut output: Vec<u8> = (0..input.len()).map(|_| 0u8).collect();

    // TODO: nongeneric version
    let unescape = escape::GenericUnescape::new();
    match unescape.unescape(input, &mut output[..]) {
        None => None,
        Some(output_len) => {
            output.truncate(output_len);
            Some(ByteString(output))
        }
    }
}
