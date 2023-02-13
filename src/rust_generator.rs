use crate::{escape, visitor};
use std::io::Write;

pub struct Generator<'a, WriteT> {
    writer: &'a mut WriteT,
    escape_is_necessary: escape::IsNecessary,

    // varying
    needs_space_before_naked_atom: bool,
}

impl<'a, WriteT: Write> Generator<'a, WriteT> {
    pub fn new(writer: &'a mut WriteT) -> Self {
        Self {
            writer,
            escape_is_necessary: escape::IsNecessary::new(),
            needs_space_before_naked_atom: false,
        }
    }
}

impl<'a, WriteT: Write> visitor::ReadVisitor for Generator<'a, WriteT> {
    fn reset(&mut self) {
        self.needs_space_before_naked_atom = false;
    }
    fn atom(&mut self, atom: &[u8]) {
        if self.escape_is_necessary.eval(atom) {
            self.writer.write_all(b"\"").unwrap();
            escape::escape(atom, self.writer).unwrap();
            self.writer.write_all(b"\"").unwrap();
            self.needs_space_before_naked_atom = false;
        } else {
            if self.needs_space_before_naked_atom {
                self.writer.write_all(b" ").unwrap();
            }
            self.writer.write_all(atom).unwrap();
            self.needs_space_before_naked_atom = true;
        }
    }
    fn list_open(&mut self) {
        self.writer.write_all(b"(").unwrap();
        self.needs_space_before_naked_atom = false;
    }
    fn list_close(&mut self) {
        self.writer.write_all(b")").unwrap();
        self.needs_space_before_naked_atom = false;
    }
    fn eof(&mut self) {
    }
}

pub fn fmt<SexpT: visitor::ReadVisitable>(f: &mut std::fmt::Formatter<'_>, sexp: &SexpT) -> std::fmt::Result {
    let mut output = Vec::new();
    let mut generator = Generator::new(&mut output);
    sexp.visit(&mut generator);
    f.write_str(std::str::from_utf8(&output[..]).unwrap())
}
