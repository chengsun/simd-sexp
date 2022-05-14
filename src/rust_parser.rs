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

pub enum TapeElementHeader {
    Atom(usize),
    List(usize),
}

impl Tape {
    // TODO: rewrite as visitor
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

    fn process<VisitorT: parser::Visitor>(&self, visitor: &mut VisitorT) {
        let mut i = 0usize;
        let mut list_ends: Vec<(usize, VisitorT::Context)> = Vec::new();
        loop {
            while list_ends.last().map(|(size, _)| size) == Some(&i) {
                let (_, context) = list_ends.pop().unwrap();
                let parent_context = list_ends.last_mut ().map(|(_, context)| context);
                visitor.list_close(context, parent_context);
            }
            if i >= self.0.len() {
                break;
            }
            let x = utils::read_u32(&self.0[i..]);
            i += 4;
            let is_atom = x % 2 == 0;
            let len = (x / 2) as usize;
            let parent_context = list_ends.last_mut().map(|(_, context)| context);
            if is_atom {
                let mut atom = visitor.atom_reserve(len);
                {
                    let output = visitor.atom_borrow(&mut atom);
                    unsafe { std::ptr::copy_nonoverlapping(&self.0[i] as *const u8, output[0] as *mut u8, len) };
                }
                visitor.atom(atom, len, parent_context);
                i += len;
            } else {
                let context = visitor.list_open(parent_context);
                list_ends.push((i + len, context));
            }
        }
        assert!(list_ends.is_empty());
        visitor.eof();
        ()
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
    type IntermediateAtom = usize;
    type Context = TapeVisitorContext;
    type FinalReturnType = Tape;
    fn atom_reserve(&mut self, length_upper_bound: usize) -> Self::IntermediateAtom {
        let tape_start_index = self.tape.0.len();
        self.tape.0.extend((0..(length_upper_bound + 4)).map(|_| 0u8));
        tape_start_index
    }
    fn atom_borrow<'a, 'b : 'a>(&'b mut self, tape_start_index: &'a mut Self::IntermediateAtom) -> &'a mut [u8] {
        &mut self.tape.0[(*tape_start_index + 4)..]
    }
    fn atom(&mut self, tape_start_index: Self::IntermediateAtom, length: usize, _: Option<&mut TapeVisitorContext>) {
        utils::write_u32(&mut self.tape.0[tape_start_index..], (length * 2).try_into().unwrap());
        self.tape.0.truncate(tape_start_index + 4 + length);
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
            let sexp_or_error = parser.process_all(&input[..]);
            let output = sexp_or_error.map(|sexps| SexpMulti(sexps).to_string());
            validate("SimpleVisitor<SexpFactory>", output);
        }

        {
            let mut parser = parser::State::new(TapeVisitor::new());
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
}
