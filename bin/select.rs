use simd_sexp::*;
use simd_sexp::escape::Unescape;
use simd_sexp::utils::unlikely;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{stdin, stdout, IoSlice, StdoutLock, Write};

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
                self.stdout.write_vectored(&[IoSlice::new(&value[..]), IoSlice::new(&b"\n"[..])]).unwrap();
            },
        };
    }

    #[inline(always)]
    fn eof(&mut self) {
    }
}

#[derive(Copy, Clone, Debug)]
enum SelectStage2Context {
    Start,
    SelectNext(u16),
    Selected(u16, u32),
    Ignore,
}

struct SelectStage2<'a> {
    labeled: bool,

    stack_pointer: i8,

    stack: [SelectStage2Context; 64],
    first_interesting_stack_pointer: i8,

    select_tree: BTreeMap<&'a [u8], u16>,
    select_vec: Vec<&'a [u8]>,
    stdout: std::io::StdoutLock<'a>,
    unescape: escape::GenericUnescape,
}

impl<'a> SelectStage2<'a> {
    fn new<T: IntoIterator<Item = &'a [u8]>>(iter: T, stdout: StdoutLock<'a>, labeled: bool) -> Self {
        let select_vec: Vec<&'a [u8]> = iter.into_iter().collect();
        let mut select_tree: BTreeMap<&'a [u8], u16> = BTreeMap::new();
        for (key_id, key) in select_vec.iter().enumerate() {
            select_tree.insert(key, key_id.try_into().unwrap());
        }
        Self {
            labeled,
            stack_pointer: 0i8,
            stack: [SelectStage2Context::Start; 64],
            first_interesting_stack_pointer: -1i8,
            select_tree,
            select_vec,
            stdout,
            unescape: escape::GenericUnescape::new(),
        }
    }
}

impl<'a> parser::Stage2 for SelectStage2<'a> {
    type FinalReturnType = ();

    fn process_one(&mut self, input: parser::Input, this_index: usize, next_index: usize) -> Result<usize, parser::Error> {
        let _: u32 = next_index.try_into().expect("This code currently only supports input up to 4GB in size.");

        let ch = input.input[this_index - input.offset];
        match ch {
            b'(' => (),
            b')' => {
                match self.stack[self.stack_pointer as usize] {
                    SelectStage2Context::Selected(key_id, start_offset) => {
                        // TODO: escape key if necessary
                        let key = self.select_vec[key_id as usize];
                        let value = &input.input[(start_offset as usize - input.offset)..(this_index as usize - input.offset)];
                        if self.labeled {
                            self.stdout.write_vectored(&[
                                IoSlice::new(&b"("[..]),
                                IoSlice::new(&key[..]),
                                IoSlice::new(&b" "[..]),
                                IoSlice::new(&value[..]),
                                IoSlice::new(&b")\n"[..]),
                            ]).unwrap();
                        } else {
                            self.stdout.write_vectored(&[
                                IoSlice::new(&value[..]),
                                IoSlice::new(&b"\n"[..]),
                            ]).unwrap();
                        }
                    },
                    _ => (),
                }
                self.stack[self.stack_pointer as usize] = SelectStage2Context::Start;
            },
            b' ' | b'\t' | b'\n' => (),
            _ => {
                match self.stack[self.stack_pointer as usize] {
                    SelectStage2Context::SelectNext(key_id) => {
                        self.stack[self.stack_pointer as usize] = SelectStage2Context::Selected(key_id, this_index.try_into().unwrap());
                        if self.first_interesting_stack_pointer < 0 {
                            self.first_interesting_stack_pointer = self.stack_pointer;
                        }
                    },
                    SelectStage2Context::Selected(_, _) => {
                        self.stack[self.stack_pointer as usize] = SelectStage2Context::Ignore;
                        if self.first_interesting_stack_pointer == self.stack_pointer {
                            self.first_interesting_stack_pointer = -1;
                        }
                    },
                    SelectStage2Context::Start => {
                        let mut buf = [0u8; 64];
                        let key_id =
                            if ch == b'"' {
                                self.unescape.unescape(
                                    &input.input[(this_index - input.offset)..std::cmp::min(next_index, this_index - input.offset + 64)],
                                    &mut buf[..])
                                    .and_then(|(_, output_len)| self.select_tree.get(&buf[..output_len])).map(|x| *x)
                            } else {
                                self.select_tree.get(&input.input[(this_index - input.offset)..(next_index - input.offset)]).map(|x| *x)
                            };
                        self.stack[self.stack_pointer as usize] = match key_id {
                            None => SelectStage2Context::Ignore,
                            Some(key_id) => SelectStage2Context::SelectNext(key_id),
                        }
                    },
                    _ => (),
                }
            },
        }
        self.stack_pointer += (ch == b'(') as i8;
        self.stack_pointer -= (ch == b')') as i8;

        self.first_interesting_stack_pointer =
            if self.stack_pointer < self.first_interesting_stack_pointer {
                -1i8
            } else {
                self.first_interesting_stack_pointer
            };

        assert!((self.stack_pointer as usize) < self.stack.len(), "Too deeply nested");
        if unlikely(self.stack_pointer < 0) {
            return Err(parser::Error::UnmatchedCloseParen);
        }

        let input_index_to_keep =
            if self.first_interesting_stack_pointer < 0 {
                next_index
            } else {
                match self.stack[self.first_interesting_stack_pointer as usize] {
                    SelectStage2Context::Selected(_, start_offset) => {
                        start_offset as usize
                    },
                    state => panic!("unexpected state for interesting stack pointer: {:?}", state),
                }
            };

        Ok(input_index_to_keep)
    }

    fn process_eof(&mut self) -> Result<Self::FinalReturnType, parser::Error> {
        Ok(())
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

    /*
    let mut parser = parser::State::from_visitor(SelectVisitor::new(select, stdout));
    let () = parser.process_streaming(&mut stdin).unwrap();
    */

    let mut parser = parser::State::new(SelectStage2::new(select, stdout, false));
    let () = parser.process_streaming(&mut stdin).unwrap();
}
