use crate::{escape, extract, parser, rust_parser, structural};

use ocaml::IntoValue;

// Needs to be in a Box<> for alignment guarantees (which the OCaml GC cannot
// provide when it's relocating stuff)
pub struct ExtractStructuralIndicesState(Box<structural::Avx2>);

ocaml::custom! (ExtractStructuralIndicesState);

#[ocaml::func]
pub fn ml_extract_structural_indices_create_state(_unit: ocaml::Value) -> ExtractStructuralIndicesState {
    ExtractStructuralIndicesState(Box::new(structural::Avx2::new().unwrap()))
}

#[ocaml::func]
pub fn ml_extract_structural_indices(
    mut extract_structural_indices_state: ocaml::Pointer<ExtractStructuralIndicesState>,
    input: &[u8],
    mut input_index: ocaml::Uint,
    mut output: ocaml::bigarray::Array1<i32>,
    mut output_index: ocaml::Uint)
    -> (ocaml::Uint, ocaml::Uint)
{
    use structural::Classifier;

    let output_len = output.len();
    let output_data = output.data_mut();
    assert!(output_index + std::cmp::min(64, input.len() - input_index) <= output_len);

    extract_structural_indices_state.as_mut().0.structural_indices_bitmask(&input[input_index..], |bitmask, bitmask_len| {
        extract::safe_generic(|bit_offset| {
            unsafe {
                *output_data.get_unchecked_mut(output_index) = (input_index + bit_offset) as i32;
            }
            output_index += 1;
        }, bitmask);

        input_index += bitmask_len;
        if output_index + std::cmp::min(64, input.len() - input_index) <= output_len {
            structural::CallbackResult::Continue
        } else {
            structural::CallbackResult::Finish
        }
    });

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

    let mut output: Vec<u8> = (0..len).map(|_| 0u8).collect();

    // TODO: nongeneric version
    let unescape = escape::GenericUnescape::new();
    match unescape.unescape(&input[pos..], &mut output[..]) {
        None => None,
        Some((_, output_len)) => {
            output.truncate(output_len);
            Some(ByteString(output))
        }
    }
}

struct OCamlSexpFactory<'a> (&'a ocaml::Runtime);

impl<'a> OCamlSexpFactory<'a> {
    fn new(rt: &'a ocaml::Runtime) -> Self {
        OCamlSexpFactory(rt)
    }
}

impl<'a> parser::SexpFactory for OCamlSexpFactory<'a> {
    type Sexp = ocaml::Value;

    fn atom(&self, a: Vec<u8>) -> Self::Sexp {
        unsafe {
            let inner_value = ocaml::Value::bytes(a);
            let atom_value = ocaml::Value::alloc_small(1, ocaml::Tag(0));
            //atom_value.store_field(&self.0, 0, inner_value);
            *ocaml::sys::field(atom_value.raw().0, 0) = inner_value.raw().0;
            atom_value
        }
    }

    fn list(&self, xs: Vec<Self::Sexp>) -> Self::Sexp {
        unsafe {
            let mut inner = ocaml::Value::unit();
            for x in xs.iter().rev() {
                let dest = ocaml::Value::alloc_small(2, ocaml::Tag(0));
                *ocaml::sys::field(dest.raw().0, 0) = x.raw().0;
                *ocaml::sys::field(dest.raw().0, 1) = inner.raw().0;
                inner = dest;
            }
            let inner_value = inner.into_value(&self.0);
            let list_value = ocaml::Value::alloc_small(1, ocaml::Tag(1));
            //list_value.store_field(&self.0, 0, inner_value);
            *ocaml::sys::field(list_value.raw().0, 0) = inner_value.raw().0;
            list_value
        }
    }
}

struct OCamlResult<T>(Result<T, String>);

unsafe impl<T: ocaml::IntoValue> ocaml::IntoValue for OCamlResult<T> {
    fn into_value(self, rt: &ocaml::Runtime) -> ocaml::Value {
        unsafe {
            match self.0 {
                Ok(ok) => ocaml::Value::result_ok(rt, ok.into_value(rt)),
                Err(err) => ocaml::Value::result_error(rt, err.into_value(rt)),
            }
        }
    }
}

#[ocaml::func(rt)]
pub fn ml_parse_sexp(input: &[u8]) -> OCamlResult<Vec<ocaml::Value>> {

    let mut parser = parser::State::from_sexp_factory(OCamlSexpFactory::new(rt));
    let result = parser.process_all(input);
    OCamlResult(result.map_err(|err| err.to_string()))
}

impl ocaml::Custom for rust_parser::Tape {
    const NAME: &'static str = "Tape";
    const OPS: ocaml::custom::CustomOps = ocaml::custom::DEFAULT_CUSTOM_OPS;
    const FIXED_LENGTH: Option<ocaml::sys::custom_fixed_length> = None;
    const USED: usize = 32usize;
    const MAX: usize = 10485760usize;
}

#[ocaml::func]
pub fn ml_parse_sexp_to_rust(input: &[u8]) -> OCamlResult<rust_parser::Tape> {

    let mut parser = parser::State::from_visitor(rust_parser::TapeVisitor::new());
    let result = parser.process_all(input);
    OCamlResult(result.map_err(|err| err.to_string()))
}
