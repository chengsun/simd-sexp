use simd_sexp::*;
use std::collections::BTreeSet;

pub struct SelectVisitor {
    select: BTreeSet<Vec<u8>>,
    atom_buffer: Option<Vec<u8>>,
}

impl SelectVisitor {
    fn new(select: BTreeSet<Vec<u8>>) -> Self {
        Self {
            select,
            atom_buffer: Some(Vec::with_capacity(128)),
        }
    }
}

pub enum SelectVisitorContext {
    Start,
    SelectNext(Vec<u8>),
    Selected(Vec<u8>, Vec<u8>),
    Ignore,
}

impl parser::Visitor for SelectVisitor {
    type IntermediateAtom = Vec<u8>;
    type Context = SelectVisitorContext;
    type FinalReturnType = ();
    fn atom_reserve(&mut self, length_upper_bound: usize) -> Self::IntermediateAtom {
        let mut atom_buffer = self.atom_buffer.take().unwrap();
        atom_buffer.resize(length_upper_bound, 0u8);
        atom_buffer
    }
    fn atom_borrow<'a, 'b : 'a>(&'b mut self, atom_buffer: &'a mut Self::IntermediateAtom) -> &'a mut [u8] {
        &mut atom_buffer[..]
    }
    fn atom(&mut self, mut atom_buffer: Self::IntermediateAtom, length: usize, parent_context: Option<&mut Self::Context>) {
        atom_buffer.truncate(length);
        match parent_context {
            None => (),
            Some(parent_context) => {
                *parent_context = match parent_context {
                    SelectVisitorContext::Start =>
                        if self.select.contains(&atom_buffer) {
                            SelectVisitorContext::SelectNext(atom_buffer.clone())
                        } else {
                            SelectVisitorContext::Ignore
                        },
                    SelectVisitorContext::SelectNext(key) => {
                        let key = std::mem::take(key);
                        SelectVisitorContext::Selected(key, atom_buffer.clone())
                    },
                    SelectVisitorContext::Selected(_, _) => SelectVisitorContext::Ignore,
                    SelectVisitorContext::Ignore => SelectVisitorContext::Ignore,
                };
            },
        };
        self.atom_buffer = Some(atom_buffer);
    }

    fn list_open(&mut self, parent_context: Option<&mut Self::Context>) -> Self::Context {
        match parent_context {
            None => (),
            Some(parent_context) => {
                *parent_context = match *parent_context {
                    SelectVisitorContext::Start => SelectVisitorContext::Ignore,
                    SelectVisitorContext::SelectNext(_) => {
                        // TODO
                        SelectVisitorContext::Ignore
                    },
                    SelectVisitorContext::Selected(_, _) => SelectVisitorContext::Ignore,
                    SelectVisitorContext::Ignore => SelectVisitorContext::Ignore,
                };
            },
        };
        SelectVisitorContext::Start
    }

    fn list_close(&mut self, context: Self::Context, _parent_context: Option<&mut Self::Context>) {
        match context {
            SelectVisitorContext::Start |
            SelectVisitorContext::SelectNext(_) |
            SelectVisitorContext::Ignore => (),
            SelectVisitorContext::Selected(_, value) => {
                println!("{}", String::from_utf8(value).unwrap());
            },
        };
    }

    fn eof(&mut self) {
    }
}

fn main() {
    let mut args = std::env::args();

    args.next();

    let select: BTreeSet<Vec<u8>> = args.map(|s| s.as_bytes().to_owned()).collect();

    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();
    let mut parser = parser::State::new(SelectVisitor::new(select));
    let () = parser.process_streaming(&mut stdin).unwrap();
}
