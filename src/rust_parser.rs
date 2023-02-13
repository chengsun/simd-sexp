use crate::{parser, visitor, rust_generator, utils};

pub enum Sexp {
    Atom(Vec<u8>),
    List(Vec<Sexp>),
}

impl Sexp {
    fn visit_internal<VisitorT: visitor::ReadVisitor>(&self, visitor: &mut VisitorT) {
        match self {
            Sexp::Atom(a) => visitor.atom(a),
            Sexp::List(l) => {
                visitor.list_open();
                for s in l.iter() {
                    s.visit_internal(visitor);
                }
                visitor.list_close();
            }
        }
    }
}

impl visitor::ReadVisitable for Sexp {
    fn visit<VisitorT: visitor::ReadVisitor>(&self, visitor: &mut VisitorT) {
        visitor.reset();
        self.visit_internal(visitor);
        visitor.eof();
    }
}

struct SexpMulti(Vec<Sexp>);

impl std::fmt::Display for Sexp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use visitor::ReadVisitable;
        let mut output = Vec::new();
        let mut generator = rust_generator::Generator::new(&mut output);
        self.visit(&mut generator);
        f.write_str(std::str::from_utf8(&output[..]).unwrap())
    }
}

impl std::fmt::Display for SexpMulti {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use visitor::ReadVisitor;
        let mut output = Vec::new();
        let mut generator = rust_generator::Generator::new(&mut output);
        generator.reset();
        for s in self.0.iter() {
            s.visit_internal(&mut generator);
        }
        generator.eof();
        f.write_str(std::str::from_utf8(&output[..]).unwrap())
    }
}

pub struct SexpFactory();

impl SexpFactory {
    pub fn new() -> Self {
        SexpFactory()
    }
}

impl visitor::SexpFactory for SexpFactory {
    type Sexp = Sexp;

    fn atom(&self, a: Vec<u8>) -> Self::Sexp {
        Sexp::Atom(a)
    }

    fn list(&self, xs: Vec<Self::Sexp>) -> Self::Sexp {
        Sexp::List(xs)
    }
}

/**
Split tape representation (atoms are on a separate tape)
Atom: <len*2 as u32><offset as u32>
List: <len*2+1 as u32>Repr(X1)Repr(X2)...
*/

#[derive(Default)]
pub struct SplitTape {
    pub tape: Vec<u32>,
    pub atoms: Vec<u8>,
}

impl SplitTape {
    fn new() -> Self {
        Self {
            tape: Vec::new(),
            atoms: Vec::new(),
        }
    }
}

impl visitor::ReadVisitable for SplitTape {
    fn visit<VisitorT: visitor::ReadVisitor>(&self, visitor: &mut VisitorT) {
        let mut i = 0usize;
        let mut list_ends: Vec<usize> = Vec::new();
        visitor.reset();
        while i < self.tape.len() {
            let x = self.tape[i];
            i += 1;
            let is_atom = x % 2 == 0;
            let len = (x / 2) as usize;
            if is_atom {
                let y = self.tape[i] as usize;
                i += 1;
                visitor.atom(&self.atoms[y..(y + len)]);
            } else {
                visitor.list_open();
                list_ends.push(i + len);
            }
            while list_ends.last() == Some(&i) {
                list_ends.pop().unwrap();
                visitor.list_close();
            }
        }
        debug_assert!(list_ends.is_empty());
        visitor.eof();
        ()
    }
}

impl std::fmt::Display for SplitTape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use visitor::ReadVisitable;
        let mut output = Vec::new();
        let mut generator = rust_generator::Generator::new(&mut output);
        self.visit(&mut generator);
        f.write_str(std::str::from_utf8(&output[..]).unwrap())
    }
}

pub struct SplitTapeVisitor {
    tape: SplitTape,
}

impl SplitTapeVisitor {
    pub fn new() -> SplitTapeVisitor {
        Self {
            tape: SplitTape::new(),
        }
    }
}

pub struct SplitTapeVisitorContext {
    tape_start_index: usize,
}

impl visitor::Visitor for SplitTapeVisitor {
    type IntermediateAtom = u32;
    type Context = SplitTapeVisitorContext;
    type Return = SplitTape;

    fn reset(&mut self, _input_size_hint: Option<usize>) {
    }

    #[inline(always)]
    fn atom_reserve(&mut self, length_upper_bound: usize) -> Self::IntermediateAtom {
        let atoms_start_index: u32 = self.tape.atoms.len().try_into().unwrap();
        self.tape.tape.push(0);
        self.tape.tape.push(atoms_start_index);
        self.tape.atoms.extend((0..length_upper_bound).map(|_| 0u8));
        atoms_start_index
    }

    #[inline(always)]
    fn atom_borrow<'a, 'b : 'a>(&'b mut self, atoms_start_index: &'a mut Self::IntermediateAtom) -> &'a mut [u8] {
        &mut self.tape.atoms[(*atoms_start_index as usize)..]
    }

    #[inline(always)]
    fn atom(&mut self, atoms_start_index: Self::IntermediateAtom, length: usize, _: Option<&mut SplitTapeVisitorContext>) {
        let tape_len = self.tape.tape.len();
        self.tape.tape[tape_len - 2] = (length * 2).try_into().unwrap();
        self.tape.atoms.truncate(atoms_start_index as usize + length);
    }

    #[inline(always)]
    fn list_open(&mut self, _: Option<&mut SplitTapeVisitorContext>) -> SplitTapeVisitorContext {
        let tape_start_index = self.tape.tape.len();
        self.tape.tape.push(0);
        SplitTapeVisitorContext {
            tape_start_index,
        }
    }

    #[inline(always)]
    fn list_close(&mut self, context: SplitTapeVisitorContext, _: Option<&mut SplitTapeVisitorContext>) {
        let x: u32 = ((self.tape.tape.len() - context.tape_start_index - 1) * 2 + 1).try_into().unwrap();
        self.tape.tape[context.tape_start_index] = x;
    }

    #[inline(always)]
    fn eof(&mut self) -> Self::Return {
        std::mem::take(&mut self.tape)
    }
}

/**
Single tape representation
Atom: <words_len*2 as u32><data as [u8] padded like ocaml strings>
List: <words_len*2+1 as u32>Repr(X1)Repr(X2)...
*/

#[derive(Default)]
pub struct SingleTape {
    pub tape: Vec<u32>,
}

impl SingleTape {
    fn new() -> Self {
        Self {
            tape: Vec::new(),
        }
    }
}

impl visitor::ReadVisitable for SingleTape {
    fn visit<VisitorT: visitor::ReadVisitor>(&self, visitor: &mut VisitorT) {
        let mut i = 0usize;
        let mut list_ends: Vec<usize> = Vec::new();
        visitor.reset();
        while i < self.tape.len() {
            let x = self.tape[i];
            i += 1;
            let is_atom = x % 2 == 0;
            let len = (x / 2) as usize;
            if is_atom {
                let padded_atom_slice = utils::slice_u32_to_u8(&self.tape[i..(i + len)]);
                let atom_len =
                    padded_atom_slice.len()
                    - (padded_atom_slice[padded_atom_slice.len() - 1] as usize)
                    - 1;
                i += len;
                visitor.atom(&padded_atom_slice[..atom_len]);
            } else {
                visitor.list_open();
                list_ends.push(i + len);
            }
            while list_ends.last() == Some(&i) {
                list_ends.pop().unwrap();
                visitor.list_close();
            }
        }
        debug_assert!(list_ends.is_empty());
        visitor.eof();
        ()
    }
}

impl std::fmt::Display for SingleTape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use visitor::ReadVisitable;
        let mut output = Vec::new();
        let mut generator = rust_generator::Generator::new(&mut output);
        self.visit(&mut generator);
        f.write_str(std::str::from_utf8(&output[..]).unwrap())
    }
}

// TODO: why is this a separate strucxt to just SingleTape?
// TODO: rename visitor::Visitor to visitor::Builder maybe?
pub struct SingleTapeVisitor {
    tape: SingleTape,
    len_of_valid_partial_result_prefix: usize,
}

impl SingleTapeVisitor {
    pub fn new() -> SingleTapeVisitor {
        Self {
            tape: SingleTape::new(),
            len_of_valid_partial_result_prefix: 0,
        }
    }
}

impl parser::ExtractPartialResult for SingleTapeVisitor {
    type PartialReturn = SingleTape;

    fn extract_partial_result(&mut self) -> Self::PartialReturn {
        let mut split_tape = self.tape.tape.split_off(self.len_of_valid_partial_result_prefix);
        std::mem::swap(&mut split_tape, &mut self.tape.tape);
        // now split_tape contains elements in the range
        // [0..self.len_of_valid_partial_result_prefix]

        self.len_of_valid_partial_result_prefix = 0;
        SingleTape { tape: split_tape }
    }
}

pub struct SingleTapeVisitorContext {
    tape_start_index: usize,
}

impl visitor::Visitor for SingleTapeVisitor {
    type IntermediateAtom = usize;
    type Context = SingleTapeVisitorContext;
    type Return = SingleTape;

    fn reset(&mut self, _input_size_hint: Option<usize>) {
    }

    #[inline(always)]
    fn atom_reserve(&mut self, length_upper_bound: usize) -> Self::IntermediateAtom {
        self.tape.tape.push(0);
        let atoms_start_index = self.tape.tape.len();
        self.tape.tape.extend((0..(length_upper_bound / 4 + 1)).map(|_| 0u32));
        atoms_start_index
    }

    #[inline(always)]
    fn atom_borrow<'a, 'b : 'a>(&'b mut self, atoms_start_index: &'a mut Self::IntermediateAtom) -> &'a mut [u8] {
        utils::slice_u32_to_u8_mut(&mut self.tape.tape[(*atoms_start_index as usize)..])
    }

    #[inline(always)]
    fn atom(&mut self, atoms_start_index: Self::IntermediateAtom, length: usize, parent_context: Option<&mut SingleTapeVisitorContext>) {
        let padded_length_in_words = length / 4 + 1;
        self.tape.tape[atoms_start_index - 1] = (padded_length_in_words * 2).try_into().unwrap();
        {
            // add padding
            let mut atoms_start_index = atoms_start_index;
            let padded_atom = &mut self.atom_borrow(&mut atoms_start_index)[..(padded_length_in_words*4)];
            let padding_bytes = padded_atom.len() - length;
            for i in 1..padding_bytes {
                padded_atom[padded_atom.len() - 1 - i] = 0;
            }
            padded_atom[padded_atom.len() - 1] = (padding_bytes - 1).try_into().unwrap();
        }
        self.tape.tape.truncate(atoms_start_index as usize + padded_length_in_words);

        if let None = parent_context {
            // We know this atom is at the top-level.
            self.len_of_valid_partial_result_prefix = self.tape.tape.len();
        }
    }

    #[inline(always)]
    fn list_open(&mut self, _: Option<&mut SingleTapeVisitorContext>) -> SingleTapeVisitorContext {
        let tape_start_index = self.tape.tape.len();
        self.tape.tape.push(0);
        SingleTapeVisitorContext {
            tape_start_index,
        }
    }

    #[inline(always)]
    fn list_close(&mut self, context: SingleTapeVisitorContext, parent_context: Option<&mut SingleTapeVisitorContext>) {
        let x: u32 = ((self.tape.tape.len() - context.tape_start_index - 1) * 2 + 1).try_into().unwrap();
        self.tape.tape[context.tape_start_index] = x;

        if let None = parent_context {
            // We know this list_close is closing a top-level sexp.
            self.len_of_valid_partial_result_prefix = self.tape.tape.len();
        }
    }

    #[inline(always)]
    fn eof(&mut self) -> Self::Return {
        std::mem::take(&mut self.tape)
    }
}

#[cfg(feature = "ocaml")]
mod ocaml_ffi {
    use super::*;
    use crate::{parser, utils};

    struct ByteString(Vec<u8>);

    unsafe impl<'a> ocaml::FromValue<'a> for ByteString {
        fn from_value(value: ocaml::Value) -> Self {
            Self(<&[u8]>::from_value(value).to_owned())
        }
    }

    unsafe impl ocaml::IntoValue for ByteString {
        fn into_value(self, _rt: &ocaml::Runtime) -> ocaml::Value {
            unsafe { ocaml::Value::bytes(&self.0[..]) }
        }
    }

    struct ResultWrapper<T, E>(Result<T, E>);

    unsafe impl<T: ocaml::IntoValue, E: ocaml::IntoValue> ocaml::IntoValue for ResultWrapper<T, E> {
        fn into_value(self, rt: &ocaml::Runtime) -> ocaml::Value {
            unsafe {
                match self.0 {
                    Ok(x) => ocaml::Value::result_ok(rt, x.into_value(rt)),
                    Err(e) => ocaml::Value::result_error(rt, e.into_value(rt)),
                }
            }
        }
    }

    #[ocaml::func]
    pub fn ml_rust_parser_single_tape(input: ByteString) -> ResultWrapper<OCamlSingleTape, ByteString> {
        let mut parser = parser::parser_from_visitor(SingleTapeVisitor::new());
        let sexp_or_error = parser.process(&input.0[..]);
        ResultWrapper(
            match sexp_or_error {
                Ok(tape) => {
                    let slice = utils::slice_u32_to_i32(&tape.tape[..]);
                    Ok(unsafe { ocaml::bigarray::Array1::from_slice(slice) })
                },
                Err(e) => Err(ByteString(format!("{}", e).into_bytes())),
            })
    }

    type OCamlSingleTape = ocaml::bigarray::Array1<i32>;

    #[ocaml::func]
    pub fn ml_rust_parser_output_single_tape(tape: OCamlSingleTape) -> ByteString {
        let tape = SingleTape { tape: utils::slice_i32_to_u32(tape.data()).to_vec() };
        ByteString(format!("{}", tape).into_bytes())
    }

    #[ocaml::func]
    pub fn ml_rust_parser_unsafe_blit_words(src: OCamlSingleTape, src_pos: usize, len_in_words: usize, dst: ocaml::Value) {
        // do some shenanigans to work around the fact that the tape is 32-byte elements but dst is 64-byte
        // TODO: assumes 64-bit ocaml
        let src = utils::slice_u32_to_u8(utils::slice_i32_to_u32(&src.data()[src_pos..(src_pos + len_in_words)]));
        let dst_len_in_words = ((len_in_words + 1) / 2) * 2;
        let dst_extra_padding_in_bytes = (dst_len_in_words - len_in_words) * 4;
        let dst = unsafe { core::slice::from_raw_parts_mut(ocaml::sys::string_val(dst.raw().0), dst_len_in_words * 4) };
        dst[..src.len()].copy_from_slice(src);
        dst[dst.len() - 1] = src[src.len() - 1] + (dst_extra_padding_in_bytes as u8);
    }

    pub struct OCamlSingleTapeParserState(Box<dyn parser::ParsePartial<Return=SingleTape, PartialReturn=SingleTape>>);
    ocaml::custom! (OCamlSingleTapeParserState);

    #[ocaml::func]
    pub fn ml_rust_parser_single_tape_parser_state_create() -> OCamlSingleTapeParserState {
        OCamlSingleTapeParserState(parser::partial_parser_from_visitor(SingleTapeVisitor::new()))
    }

    #[ocaml::func]
    pub fn ml_rust_parser_single_tape_parse_partial(mut parser_state: ocaml::Pointer<OCamlSingleTapeParserState>, mut input: ByteString) -> ResultWrapper<OCamlSingleTape, ByteString> {
        match parser_state.as_mut().0.process_partial(&mut input.0[..]) {
            Ok(()) => (),
            Err(e) => { return ResultWrapper(Err(ByteString(format!("{}", e).into_bytes()))); },
        }
        ResultWrapper(Ok({
            let partial_tape = parser_state.as_mut().0.extract_partial_result();
            let slice = utils::slice_u32_to_i32(&partial_tape.tape[..]);
            unsafe { ocaml::bigarray::Array1::from_slice(slice) }
        }))
    }

    #[ocaml::func]
    pub fn ml_rust_parser_single_tape_parse_eof(mut parser_state: ocaml::Pointer<OCamlSingleTapeParserState>) -> ResultWrapper<OCamlSingleTape, ByteString> {
        ResultWrapper(
            match parser_state.as_mut().0.process_eof() {
                Ok(tape) => Ok({
                    let slice = utils::slice_u32_to_i32(&tape.tape[..]);
                    unsafe { ocaml::bigarray::Array1::from_slice(slice) }
                }),
                Err(e) => Err(ByteString(format!("{}", e).into_bytes())),
            })
    }

    // TODO: move this somewhere generic
    // TODO: when visitor::Visitor is renamed to visitor::Builder, rename this too
    struct VisitorBuilder<V: visitor::Visitor> {
        tape: V,
        context_stack: Vec<V::Context>,
    }

    impl<V: visitor::Visitor> VisitorBuilder<V> {
        fn new(v: V) -> Self {
            let mut builder = Self { tape: v, context_stack: Vec::new() };
            builder.tape.reset(None);
            builder
        }

        fn atom(&mut self, s: Vec<u8>) {
            let mut intermediate_atom = self.tape.atom_reserve(s.len());
            self.tape.atom_borrow(&mut intermediate_atom).copy_from_slice(&s[..]);
            self.tape.atom(intermediate_atom, s.len(), self.context_stack.first_mut());
        }

        fn list_open(&mut self) {
            let context = self.tape.list_open(self.context_stack.first_mut());
            self.context_stack.push(context);
        }

        fn list_close(&mut self) {
            let context = self.context_stack.pop().unwrap();
            self.tape.list_close(context, self.context_stack.first_mut());
        }

        fn finalize(&mut self) -> V::Return {
            assert!(self.context_stack.is_empty());
            self.tape.eof()
        }
    }

    type SingleTapeBuilder = VisitorBuilder<SingleTapeVisitor>;

    // Needs to be in a Box<> for alignment guarantees (which the OCaml GC cannot
    // provide when it's relocating stuff)
    pub struct SingleTapeBuilderBox(Box<SingleTapeBuilder>);

    impl ocaml::Custom for SingleTapeBuilderBox {
        const NAME: &'static str = "SingleTapeBuilderBox";
        const OPS: ocaml::custom::CustomOps = ocaml::custom::DEFAULT_CUSTOM_OPS;
        const FIXED_LENGTH: Option<ocaml::sys::custom_fixed_length> = None;
        const USED: usize = 1usize;
        const MAX: usize = 1024usize;
    }

    #[ocaml::func]
    pub fn ml_rust_parser_single_tape_builder_create() -> SingleTapeBuilderBox {
        SingleTapeBuilderBox(Box::new(SingleTapeBuilder::new(SingleTapeVisitor::new())))
    }

    #[ocaml::func]
    pub fn ml_rust_parser_single_tape_builder_append_atom(mut b: ocaml::Pointer<SingleTapeBuilderBox>, s: ByteString) {
        b.as_mut().0.atom(s.0)
    }

    #[ocaml::func]
    pub fn ml_rust_parser_single_tape_builder_append_list_open(mut b: ocaml::Pointer<SingleTapeBuilderBox>) {
        b.as_mut().0.list_open()
    }

    #[ocaml::func]
    pub fn ml_rust_parser_single_tape_builder_append_list_close(mut b: ocaml::Pointer<SingleTapeBuilderBox>) {
        b.as_mut().0.list_close()
    }

    #[ocaml::func]
    pub fn ml_rust_parser_single_tape_builder_finalize(mut b: ocaml::Pointer<SingleTapeBuilderBox>) -> OCamlSingleTape {
        let tape = b.as_mut().0.finalize();
        let slice = utils::slice_u32_to_i32(&tape.tape[..]);
        unsafe { ocaml::bigarray::Array1::from_slice(slice) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    fn run_test(input: &[u8], expected_output: Result<&str, parser::Error>) {
        let validate = |name: &str, output: Result<String, parser::Error>| {
            let output = match output {
                Ok(ref x) => Ok(x.as_str()),
                Err(e) => Err(e),
            };

            if output != expected_output {
                    println!("input:      {:?}", input);
                    println!("expect out: {:?}", expected_output);
                    println!("actual out: {:?}", output);
                    panic!("parser test failed for {}", name);
            }
        };

        {
            let mut parser = parser::parser_from_sexp_factory(SexpFactory::new());
            let sexp_or_error = parser.process(&input[..]);
            let output = sexp_or_error.map(|sexps| SexpMulti(sexps).to_string());
            validate("SimpleVisitor<SexpFactory>", output);
        }

        {
            let mut parser = parser::streaming_from_sexp_factory(SexpFactory::new());
            let mut buf_reader = std::io::BufReader::with_capacity(1, input);
            let sexp_or_error = parser.process_streaming(&mut buf_reader);
            let output = sexp_or_error.map(|sexps| SexpMulti(sexps).to_string());
            validate("SimpleVisitor<SexpFactory> (process_streaming)", output);
        }

        {
            let mut parser = parser::parser_from_visitor(SplitTapeVisitor::new());
            let sexp_or_error = parser.process(&input[..]);
            let output = sexp_or_error.map(|tape| tape.to_string());
            validate("SplitTapeVisitor", output);
        }

        {
            let mut parser = parser::parser_from_visitor(SingleTapeVisitor::new());
            let sexp_or_error = parser.process(&input[..]);
            let output = sexp_or_error.map(|tape| tape.to_string());
            validate("SingleTapeVisitor", output);
        }
    }

    #[test] fn test_1() { run_test(br#"foo"#, Ok(r#"foo"#)); }
    #[test] fn test_2() { run_test(br#"foo bar"#, Ok(r#"foo bar"#)); }
    #[test] fn test_3() { run_test(br#"foo   bar"#, Ok(r#"foo bar"#)); }
    #[test] fn test_4() { run_test(br#"(foo   bar)"#, Ok(r#"(foo bar)"#)); }
    #[test] fn test_5() { run_test(br#"(fo\o   bar)"#, Ok(r#"("fo\\o"bar)"#)); }
    #[test] fn test_6() { run_test(br#""()""#, Ok(r#""()""#)); }
    #[test] fn test_7() { run_test(br#"" ""#, Ok(r#"" ""#)); }
    #[test] fn test_8() { run_test(br#""fo\"o""#, Ok(r#""fo\"o""#)); }
    #[test] fn test_9() { run_test(br#"fo\"o""#, Ok(r#""fo\\"o"#)); }
    #[test] fn test_10() { run_test(br#"foo"x"bar"#, Ok(r#"foo x bar"#)); }
    #[test] fn test_11() { run_test(br#"foo(x)bar"#, Ok(r#"foo(x)bar"#)); }
    #[test] fn test_12() { run_test(br#""x"foo"y""#, Ok(r#"x foo y"#)); }
    #[test] fn test_13() { run_test(br#"(x)foo(y)"#, Ok(r#"(x)foo(y)"#)); }
    #[test] fn test_14() { run_test(br#""foo\n""#, Ok(r#""foo\n""#)); }
    #[test] fn test_15() { run_test(br#"(foo (bar baz))"#, Ok(r#"(foo(bar baz))"#)); }
    #[test] fn test_16() { run_test(br#"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"#,
                                    Ok(r#"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"#)); }
    #[test] fn test_17() { run_test(br#"                                                                                    "#,
                                    Ok(r#""#)); }
    #[test] fn test_18() { run_test(br#""a\000""#, Ok(r#""a\000""#)); }
    #[test] fn test_19() { run_test(br#""abc\000""#, Ok(r#""abc\000""#)); }
    #[test] fn test_20() { run_test(br#""abcdef\000""#, Ok(r#""abcdef\000""#)); }
    #[test] fn test_21() { run_test(br#""abcdefg\000""#, Ok(r#""abcdefg\000""#)); }
}
