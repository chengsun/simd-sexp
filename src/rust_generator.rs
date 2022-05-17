use crate::{escape, visitor};
use std::io::Write;

pub struct Generator<'a, WriteT> {
    writer: &'a mut WriteT,

    // varying
    needs_space_before_naked_atom: bool,
}

impl<'a, WriteT: Write> Generator<'a, WriteT> {
    pub fn new(writer: &'a mut WriteT) -> Self {
        Self {
            writer,
            needs_space_before_naked_atom: false,
        }
    }
}

impl<'a, WriteT: Write> visitor::ReadVisitor for Generator<'a, WriteT> {
    fn bof(&mut self) {
        self.needs_space_before_naked_atom = false;
    }
    fn atom(&mut self, atom: &[u8]) {
        if escape::escape_is_necessary(atom) {
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
