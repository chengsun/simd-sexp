use simd_sexp::*;
use std::collections::BTreeSet;
use std::io::{stdin, stdout, StdoutLock, Write};

pub struct SelectVisitor<'a> {
    select: BTreeSet<&'a [u8]>,
    stdout: std::io::StdoutLock<'a>,
    atom_buffer: Option<Vec<u8>>,
}

impl<'a> SelectVisitor<'a> {
    fn new(select: BTreeSet<&'a [u8]>, stdout: StdoutLock<'a>) -> Self {
        Self {
            select,
            stdout,
            atom_buffer: Some(Vec::with_capacity(128)),
        }
    }
}

pub enum SelectVisitorContext<'a> {
    Start,
    SelectNext(&'a [u8]),
    Selected(&'a [u8], Vec<u8>),
    Ignore,
}

impl<'c> parser::Visitor for SelectVisitor<'c> {
    type IntermediateAtom = Vec<u8>;
    type Context = SelectVisitorContext<'c>;
    type FinalReturnType = ();

    #[inline(always)]
    fn atom_reserve(&mut self, length_upper_bound: usize) -> Self::IntermediateAtom {
        let mut atom_buffer = self.atom_buffer.take().unwrap();
        atom_buffer.resize(length_upper_bound, 0u8);
        atom_buffer
    }

    #[inline(always)]
    fn atom_borrow<'a, 'b : 'a>(&'b mut self, atom_buffer: &'a mut Self::IntermediateAtom) -> &'a mut [u8] {
        &mut atom_buffer[..]
    }

    #[inline(always)]
    fn atom(&mut self, mut atom_buffer: Self::IntermediateAtom, length: usize, parent_context: Option<&mut Self::Context>) {
        atom_buffer.truncate(length);
        match parent_context {
            None => (),
            Some(parent_context) => {
                *parent_context = match parent_context {
                    SelectVisitorContext::Start =>
                        match self.select.get(&atom_buffer[..]) {
                            Some(key) => SelectVisitorContext::SelectNext(key.clone()),
                            None => SelectVisitorContext::Ignore,
                        },
                    SelectVisitorContext::SelectNext(key) => {
                        SelectVisitorContext::Selected(key.clone(), atom_buffer.clone())
                    },
                    SelectVisitorContext::Selected(_, _) => SelectVisitorContext::Ignore,
                    SelectVisitorContext::Ignore => SelectVisitorContext::Ignore,
                };
            },
        };
        self.atom_buffer = Some(atom_buffer);
    }

    #[inline(always)]
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

    #[inline(always)]
    fn list_close(&mut self, context: Self::Context, _parent_context: Option<&mut Self::Context>) {
        match context {
            SelectVisitorContext::Start |
            SelectVisitorContext::SelectNext(_) |
            SelectVisitorContext::Ignore => (),
            SelectVisitorContext::Selected(_, value) => {
                self.stdout.write(&value[..]).unwrap();
            },
        };
    }

    #[inline(always)]
    fn eof(&mut self) {
    }
}

fn main() {
    let mut args = std::env::args();

    args.next();

    let select_vec: Vec<Vec<u8>> = args.map(|s| s.as_bytes().to_owned()).collect();
    let mut select: BTreeSet<&[u8]> = BTreeSet::new();
    for key in select_vec.iter() {
        select.insert(key);
    }

    let stdin = stdin();
    let mut stdin = stdin.lock();
    let stdout = stdout();
    let stdout = stdout.lock();
    let mut parser = parser::State::from_visitor(SelectVisitor::new(select, stdout));
    let () = parser.process_streaming(&mut stdin).unwrap();
}
