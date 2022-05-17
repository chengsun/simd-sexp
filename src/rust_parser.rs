use crate::{utils, visitor, rust_generator};

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
        visitor.bof();
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
        generator.bof();
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
Atom: <len*2 as u32>string
List: <len*2+1 as u32>Repr(X1)Repr(X2)...
*/

#[derive(Default)]
pub struct Tape(pub Vec<u8>);

impl visitor::ReadVisitable for Tape {
    fn visit<VisitorT: visitor::ReadVisitor>(&self, visitor: &mut VisitorT) {
        let mut i = 0usize;
        let mut list_ends: Vec<usize> = Vec::new();
        visitor.bof();
        loop {
            while list_ends.last() == Some(&i) {
                list_ends.pop().unwrap();
                visitor.list_close();
            }
            if i >= self.0.len() {
                break;
            }
            let x = utils::read_u32(&self.0[i..]);
            i += 4;
            let is_atom = x % 2 == 0;
            let len = (x / 2) as usize;
            if is_atom {
                visitor.atom(&self.0[i..(i+len)]);
                i += len;
            } else {
                visitor.list_open();
                list_ends.push(i + len);
            }
        }
        debug_assert!(list_ends.is_empty());
        visitor.eof();
        ()
    }
}

impl std::fmt::Display for Tape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use visitor::ReadVisitable;
        let mut output = Vec::new();
        let mut generator = rust_generator::Generator::new(&mut output);
        self.visit(&mut generator);
        f.write_str(std::str::from_utf8(&output[..]).unwrap())
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

impl visitor::Visitor for TapeVisitor {
    type IntermediateAtom = usize;
    type Context = TapeVisitorContext;
    type FinalReturnType = Tape;

    fn bof(&mut self, _input_size_hint: Option<usize>) {
    }

    #[inline(always)]
    fn atom_reserve(&mut self, length_upper_bound: usize) -> Self::IntermediateAtom {
        let tape_start_index = self.tape.0.len();
        self.tape.0.extend((0..(length_upper_bound + 4)).map(|_| 0u8));
        tape_start_index
    }

    #[inline(always)]
    fn atom_borrow<'a, 'b : 'a>(&'b mut self, tape_start_index: &'a mut Self::IntermediateAtom) -> &'a mut [u8] {
        &mut self.tape.0[(*tape_start_index + 4)..]
    }

    #[inline(always)]
    fn atom(&mut self, tape_start_index: Self::IntermediateAtom, length: usize, _: Option<&mut TapeVisitorContext>) {
        utils::write_u32(&mut self.tape.0[tape_start_index..], (length * 2).try_into().unwrap());
        self.tape.0.truncate(tape_start_index + 4 + length);
    }

    #[inline(always)]
    fn list_open(&mut self, _: Option<&mut TapeVisitorContext>) -> TapeVisitorContext {
        let tape_start_index = self.tape.0.len();
        self.tape.0.extend([0u8; 4]);
        TapeVisitorContext {
            tape_start_index,
        }
    }

    #[inline(always)]
    fn list_close(&mut self, context: TapeVisitorContext, _: Option<&mut TapeVisitorContext>) {
        let x: u32 = ((self.tape.0.len() - context.tape_start_index - 4) * 2 + 1).try_into().unwrap();
        utils::write_u32(&mut self.tape.0[context.tape_start_index..], x);
    }

    #[inline(always)]
    fn eof(&mut self) -> Self::FinalReturnType {
        std::mem::take(&mut self.tape)
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
            let mut parser = parser::State::from_sexp_factory(SexpFactory::new());
            let sexp_or_error = parser.process_all(&input[..]);
            let output = sexp_or_error.map(|sexps| SexpMulti(sexps).to_string());
            validate("SimpleVisitor<SexpFactory>", output);
        }

        {
            let mut parser = parser::State::from_sexp_factory(SexpFactory::new());
            let mut buf_reader = std::io::BufReader::with_capacity(1, input);
            let sexp_or_error = parser.process_streaming(&mut buf_reader);
            let output = sexp_or_error.map(|sexps| SexpMulti(sexps).to_string());
            validate("SimpleVisitor<SexpFactory> (process_streaming)", output);
        }

        {
            let mut parser = parser::State::from_visitor(TapeVisitor::new());
            let sexp_or_error = parser.process_all(&input[..]);
            let output = sexp_or_error.map(|tape| tape.to_string());
            validate("TapeVisitor", output);
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
}
