use crate::{parser, varint, escape};

pub enum RustSexp {
    Atom(Vec<u8>),
    List(Vec<RustSexp>),
}

impl RustSexp {
    fn fmt_internal(&self, f: &mut std::fmt::Formatter<'_>, space_separator_needed: &mut bool) -> std::fmt::Result {
        match self {
            RustSexp::Atom(a) =>
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
                },
            RustSexp::List(l) => {
                write!(f, "(")?;
                *space_separator_needed = false;
                for s in l.iter() {
                    s.fmt_internal(f, space_separator_needed)?;
                }
                write!(f, ")")?;
                *space_separator_needed = false;
            }
        }
        Ok(())
    }
}

impl std::fmt::Display for RustSexp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt_internal(f, &mut false)
    }
}

struct RustSexps(Vec<RustSexp>);

impl std::fmt::Display for RustSexps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut space_separator_needed = false;
        for s in self.0.iter() {
            s.fmt_internal(f, &mut space_separator_needed)?;
        }
        Ok(())
    }
}

pub struct RustSexpFactory();

impl RustSexpFactory {
    pub fn new() -> Self {
        RustSexpFactory()
    }
}

impl parser::SexpFactory for RustSexpFactory {
    type Sexp = RustSexp;

    fn atom(&self, a: &[u8]) -> Self::Sexp {
        RustSexp::Atom(a.to_vec())
    }

    fn list(&self, xs: Vec<Self::Sexp>) -> Self::Sexp {
        RustSexp::List(xs)
    }
}

/**
Atom: <len*2 as u64>string
List: <len*2+1 as u64>Repr(X1)Repr(X2)...
*/

pub struct TapeVisitor {
    tape: Vec<u8>,
    varint_encoder: varint::GenericEncoder,
}

impl TapeVisitor {
    pub fn new() -> TapeVisitor {
        Self {
            tape: Vec::new(),
            varint_encoder: varint::GenericEncoder::new(),
        }
    }
}

pub struct TapeVisitorContext {
    tape_start_index: usize,
}

impl parser::Visitor for TapeVisitor {
    type Context = TapeVisitorContext;
    type IntermediateReturnType = ();
    type FinalReturnType = Vec<u8>;
    fn atom(&mut self, atom: &[u8], _: Option<&mut TapeVisitorContext>) {
        // TODO: lol
        let tape_start_index = self.tape.len();
        self.tape.extend([0u8; 4]);
        self.varint_encoder.encode_one(atom.len() * 2, &mut self.tape[tape_start_index..]).unwrap();
        self.tape.extend_from_slice(atom);
    }
    fn list_open(&mut self, _: Option<&mut TapeVisitorContext>) -> TapeVisitorContext {
        let tape_start_index = self.tape.len();
        self.tape.extend([0u8; 4]);
        TapeVisitorContext {
            tape_start_index,
        }
    }
    fn list_close(&mut self, context: TapeVisitorContext) {
        self.varint_encoder.encode_one((self.tape.len() - context.tape_start_index) * 2 + 1, &mut self.tape[context.tape_start_index..]);
    }
    fn eof(&mut self) -> Self::FinalReturnType {
        std::mem::take(&mut self.tape)
    }
}

#[cfg(test)]
mod parser_tests {
    use super::*;

    fn run_test(input: &[u8], expected_output: Result<&str, parser::Error>) {
        let mut parser = parser::State::new(parser::SimpleVisitor::new(RustSexpFactory::new()));
        let sexp_or_error = parser.process_all(input);
        let output = sexp_or_error.map(|sexp| RustSexps(sexp).to_string());
        let output = match output {
            Ok(ref x) => Ok(x.as_str()),
            Err(e) => Err(e),
        };

        if output != expected_output {
                println!("input:      {:?}", input);
                println!("expect out: {:?}", expected_output);
                println!("actual out: {:?}", output);
                panic!("parser test failed");
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
}
