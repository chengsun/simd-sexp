use crate::{visitor, rust_generator};

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
Atom: <len*2 as u32><offset as u32>
List: <len*2+1 as u32>Repr(X1)Repr(X2)...
*/

#[derive(Default)]
pub struct Tape {
    pub tape: Vec<u32>,
    pub atoms: Vec<u8>,
}

impl Tape {
    fn new() -> Self {
        Self {
            tape: Vec::new(),
            atoms: Vec::new(),
        }
    }
}

impl visitor::ReadVisitable for Tape {
    fn visit<VisitorT: visitor::ReadVisitor>(&self, visitor: &mut VisitorT) {
        let mut i = 0usize;
        let mut list_ends: Vec<usize> = Vec::new();
        visitor.bof();
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
            tape: Tape::new(),
        }
    }
}

pub struct TapeVisitorContext {
    tape_start_index: usize,
}

impl visitor::Visitor for TapeVisitor {
    type IntermediateAtom = u32;
    type Context = TapeVisitorContext;
    type Return = Tape;

    fn bof(&mut self, _input_size_hint: Option<usize>) {
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
    fn atom(&mut self, atoms_start_index: Self::IntermediateAtom, length: usize, _: Option<&mut TapeVisitorContext>) {
        let tape_len = self.tape.tape.len();
        self.tape.tape[tape_len - 2] = (length * 2).try_into().unwrap();
        self.tape.atoms.truncate(atoms_start_index as usize + length);
    }

    #[inline(always)]
    fn list_open(&mut self, _: Option<&mut TapeVisitorContext>) -> TapeVisitorContext {
        let tape_start_index = self.tape.tape.len();
        self.tape.tape.push(0);
        TapeVisitorContext {
            tape_start_index,
        }
    }

    #[inline(always)]
    fn list_close(&mut self, context: TapeVisitorContext, _: Option<&mut TapeVisitorContext>) {
        let x: u32 = ((self.tape.tape.len() - context.tape_start_index - 1) * 2 + 1).try_into().unwrap();
        self.tape.tape[context.tape_start_index] = x;
    }

    #[inline(always)]
    fn eof(&mut self) -> Self::Return {
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
            let mut parser = parser::parser_from_sexp_factory(SexpFactory::new());
            let sexp_or_error = parser.process(parser::SegmentIndex::EntireFile, &input[..]);
            let output = sexp_or_error.map(|sexps| SexpMulti(sexps).to_string());
            validate("SimpleVisitor<SexpFactory>", output);
        }

        {
            let mut parser = parser::streaming_from_sexp_factory(SexpFactory::new());
            let mut buf_reader = std::io::BufReader::with_capacity(1, input);
            let sexp_or_error = parser.process_streaming(parser::SegmentIndex::EntireFile, &mut buf_reader);
            let output = sexp_or_error.map(|sexps| SexpMulti(sexps).to_string());
            validate("SimpleVisitor<SexpFactory> (process_streaming)", output);
        }

        {
            let mut parser = parser::parser_from_visitor(TapeVisitor::new());
            let sexp_or_error = parser.process(parser::SegmentIndex::EntireFile, &input[..]);
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
