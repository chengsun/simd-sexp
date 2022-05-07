use crate::{escape, parser, utils};

fn write_atom(f: &mut std::fmt::Formatter<'_>, a: &[u8], space_separator_needed: &mut bool) -> std::fmt::Result {
    if escape::escape_is_necessary(a) {
        write!(f, "\"")?;
        write!(f, "{}", String::from_utf8(escape::escape(a)).unwrap())?;
        write!(f, "\"")?;
        *space_separator_needed = false;
    } else {
        if *space_separator_needed {
            write!(f, " ")?;
        }
        write!(f, "{}", std::str::from_utf8(a).unwrap())?;
        *space_separator_needed = true;
    }
    Ok(())
}

pub enum Sexp {
    Atom(Vec<u8>),
    List(Vec<Sexp>),
}

impl Sexp {
    fn fmt_mach(&self, f: &mut std::fmt::Formatter<'_>, space_separator_needed: &mut bool) -> std::fmt::Result {
        match self {
            Sexp::Atom(a) => write_atom(f, a, space_separator_needed)?,
            Sexp::List(l) => {
                write!(f, "(")?;
                *space_separator_needed = false;
                for s in l.iter() {
                    s.fmt_mach(f, space_separator_needed)?;
                }
                write!(f, ")")?;
                *space_separator_needed = false;
            }
        }
        Ok(())
    }
}

struct SexpMulti(Vec<Sexp>);

impl std::fmt::Display for Sexp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt_mach(f, &mut false)
    }
}

impl std::fmt::Display for SexpMulti {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut space_separator_needed = false;
        for s in self.0.iter() {
            s.fmt_mach(f, &mut space_separator_needed)?;
        }
        Ok(())
    }
}

pub struct SexpFactory();

impl SexpFactory {
    pub fn new() -> Self {
        SexpFactory()
    }
}

impl parser::SexpFactory for SexpFactory {
    type Sexp = Sexp;

    fn atom(&self, a: &[u8]) -> Self::Sexp {
        Sexp::Atom(a.to_vec())
    }

    fn list(&self, xs: Vec<Self::Sexp>) -> Self::Sexp {
        Sexp::List(xs)
    }
}

/**
Atom: <len*2 as u32>string
List: <len*2+1 as u32>Repr(X1)Repr(X2)...
*/

#[derive(Default)]
pub struct Tape(pub Vec<u8>);

impl Tape {
    fn fmt_mach(&self, f: &mut std::fmt::Formatter<'_>, space_separator_needed: &mut bool) -> std::fmt::Result {
        let mut i = 0usize;
        let mut list_ends: Vec<usize> = Vec::new();
        loop {
            while list_ends.last() == Some(&i) {
                list_ends.pop();
                write!(f, ")")?;
                *space_separator_needed = false;
            }
            if i >= self.0.len() {
                break;
            }
            let x = utils::read_u32(&self.0[i..]);
            i += 4;
            let is_atom = x % 2 == 0;
            let len = (x / 2) as usize;
            if is_atom {
                write_atom(f, &self.0[i..(i + len)], space_separator_needed)?;
                i += len;
            } else {
                write!(f, "(")?;
                *space_separator_needed = false;
                list_ends.push(i + len);
            }
        }
        assert!(list_ends.is_empty());
        Ok(())
    }
}

impl std::fmt::Display for Tape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt_mach(f, &mut false)
    }
}

pub struct TapeVisitor {
    tape: Tape,
}

impl TapeVisitor {
    pub fn new() -> TapeVisitor {
        Self {
            tape: Tape(Vec::new()),
        }
    }
}

pub struct TapeVisitorContext {
    tape_start_index: usize,
}

impl parser::Visitor for TapeVisitor {
    type Context = TapeVisitorContext;
    type IntermediateReturnType = ();
    type FinalReturnType = Tape;
    fn atom(&mut self, atom: &[u8], _: Option<&mut TapeVisitorContext>) {
        let tape_start_index = self.tape.0.len();
        self.tape.0.extend([0u8; 4]);
        utils::write_u32(&mut self.tape.0[tape_start_index..], (atom.len() * 2).try_into().unwrap());
        self.tape.0.extend_from_slice(atom);
    }
    fn list_open(&mut self, _: Option<&mut TapeVisitorContext>) -> TapeVisitorContext {
        let tape_start_index = self.tape.0.len();
        self.tape.0.extend([0u8; 4]);
        TapeVisitorContext {
            tape_start_index,
        }
    }
    fn list_close(&mut self, context: TapeVisitorContext, _: Option<&mut TapeVisitorContext>) {
        let x: u32 = ((self.tape.0.len() - context.tape_start_index - 4) * 2 + 1).try_into().unwrap();
        utils::write_u32(&mut self.tape.0[context.tape_start_index..], x);
    }
    fn eof(&mut self) -> Self::FinalReturnType {
        std::mem::take(&mut self.tape)
    }
}

pub mod two_phase {
    use crate::{parser, varint};

    /**
    Atom: <len*2>string
    List: <len*2+1>Repr(X1)Repr(X2)...
    */

    #[derive(Default)]
    pub struct Tape(pub Vec<u8>);

    impl Tape {
        fn fmt_mach(&self, f: &mut std::fmt::Formatter<'_>, varint_decoder: &varint::GenericDecoder, space_separator_needed: &mut bool) -> std::fmt::Result {
            let mut i = 0usize;
            let mut list_ends: Vec<usize> = Vec::new();
            loop {
                while list_ends.last() == Some(&i) {
                    list_ends.pop();
                    write!(f, ")")?;
                    *space_separator_needed = false;
                }
                if i >= self.0.len() {
                    break;
                }
                let mut x = 0usize;
                i += varint_decoder.decode_one(&self.0[i..], &mut x).unwrap();
                let is_atom = x % 2 == 0;
                let len = (x / 2) as usize;
                if is_atom {
                    super::write_atom(f, &self.0[i..(i + len)], space_separator_needed)?;
                    i += len;
                } else {
                    write!(f, "(")?;
                    *space_separator_needed = false;
                    list_ends.push(i + len);
                }
            }
            assert!(list_ends.is_empty());
            Ok(())
        }
    }

    impl std::fmt::Display for Tape {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let varint_decoder = varint::GenericDecoder::new();
            self.fmt_mach(f, &varint_decoder, &mut false)
        }
    }

    pub struct Phase1Visitor {
        varint_encoder: varint::GenericEncoder,
        varint_length_tape: Vec<u8>,
        size: usize,
    }

    impl Phase1Visitor {
        pub fn new() -> Self {
            Self {
                varint_encoder: varint::GenericEncoder::new(),
                varint_length_tape: Vec::new(),
                size: 0,
            }
        }
    }

    pub struct Phase1Context {
        size: usize,
        varint_length_tape_index: usize,
    }

    impl parser::Visitor for Phase1Visitor {
        type Context = Phase1Context;
        type IntermediateReturnType = ();
        type FinalReturnType = (usize, Vec<u8>);
        fn atom(&mut self, atom: &[u8], mut parent_context: Option<&mut Phase1Context>) {
            let varint_length = self.varint_encoder.encode_length(2 * atom.len());
            let this_size = varint_length + atom.len();
            match parent_context {
                Some (ref mut parent_context) => { parent_context.size += this_size; },
                None => { self.size += this_size; },
            }
        }
        fn list_open(&mut self, _: Option<&mut Phase1Context>) -> Phase1Context {
            let varint_length_tape_index = self.varint_length_tape.len();
            self.varint_length_tape.push(0);
            Phase1Context {
                size: 0,
                varint_length_tape_index,
            }
        }
        fn list_close(&mut self, context: Phase1Context, mut parent_context: Option<&mut Phase1Context>) {
            let varint_length = self.varint_encoder.encode_length(2 * context.size + 1);
            self.varint_length_tape[context.varint_length_tape_index] = varint_length as u8;
            let this_size = varint_length + context.size;
            match parent_context {
                Some (ref mut parent_context) => { parent_context.size += this_size; },
                None => { self.size += this_size; },
            }
        }
        fn eof(&mut self) -> Self::FinalReturnType {
            (self.size, std::mem::take(&mut self.varint_length_tape))
        }
    }

    pub struct Phase2Visitor {
        tape: Tape,
        tape_index: usize,
        varint_length_tape: Vec<u8>,
        varint_length_tape_index: usize,
        varint_encoder: varint::GenericEncoder,
    }

    impl Phase2Visitor {
        pub fn new(phase1_result: <Phase1Visitor as parser::Visitor>::FinalReturnType) -> Phase2Visitor {
            let (size, varint_length_tape) = phase1_result;
            Self {
                tape: Tape(vec![0u8; size]),
                tape_index: 0usize,
                varint_length_tape,
                varint_length_tape_index: 0usize,
                varint_encoder: varint::GenericEncoder::new(),
            }
        }
    }

    pub struct Phase2Context {
        tape_start_index: usize,
        varint_length: u8,
    }

    impl parser::Visitor for Phase2Visitor {
        type Context = Phase2Context;
        type IntermediateReturnType = ();
        type FinalReturnType = Tape;
        fn atom(&mut self, atom: &[u8], _: Option<&mut Phase2Context>) {
            let varint_len =
                self.varint_encoder.encode_one(
                    atom.len() * 2,
                    &mut self.tape.0[self.tape_index..]).unwrap();
            self.tape_index += varint_len;
            for &a in atom {
                self.tape.0[self.tape_index] = a;
                self.tape_index += 1;
            }
        }
        fn list_open(&mut self, _: Option<&mut Phase2Context>) -> Phase2Context {
            let varint_length = self.varint_length_tape[self.varint_length_tape_index];
            self.varint_length_tape_index += 1;
            let tape_start_index = self.tape_index;
            self.tape_index += varint_length as usize;
            Phase2Context {
                tape_start_index,
                varint_length,
            }
        }
        fn list_close(&mut self, context: Phase2Context, _: Option<&mut Phase2Context>) {
            let varint_length = context.varint_length as usize;
            self.varint_encoder.encode_one(
                (self.tape_index - context.tape_start_index - varint_length) * 2 + 1,
                &mut self.tape.0[context.tape_start_index..(context.tape_start_index + varint_length)]
            ).unwrap();
        }
        fn eof(&mut self) -> Self::FinalReturnType {
            std::mem::take(&mut self.tape)
        }
    }
}

#[cfg(test)]
mod parser_tests {
    use super::*;

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
            let mut parser = parser::State::new(parser::SimpleVisitor::new(SexpFactory::new()));
            let sexp_or_error = parser.process_all(input);
            let output = sexp_or_error.map(|sexps| SexpMulti(sexps).to_string());
            validate("SimpleVisitor<SexpFactory>", output);
        }

        {
            let mut parser = parser::State::new(TapeVisitor::new());
            let sexp_or_error = parser.process_all(input);
            let output = sexp_or_error.map(|tape| tape.to_string());
            validate("TapeVisitor", output);
        }

        {
            let mut phase1 = parser::State::new(two_phase::Phase1Visitor::new());
            let phase1_result = phase1.process_all(input);
            let tape_or_error = match phase1_result {
                Err(e) => Err(e),
                Ok(phase1_result) => {
                    let mut phase2 = parser::State::new(two_phase::Phase2Visitor::new(phase1_result));
                    phase2.process_all(input)
                },
            };
            let output = tape_or_error.map(|tape| tape.to_string());
            validate("two_phase", output);
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
}
