use crate::escape::{self, Unescape};
use crate::parser;
use crate::utils::unlikely;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{IoSlice, StdoutLock, Write};

pub struct SelectVisitor<'a> {
    select: BTreeSet<&'a [u8]>,
    stdout: std::io::StdoutLock<'a>,
    atom_buffer: Option<Vec<u8>>,
}

impl<'a> SelectVisitor<'a> {
    pub fn new(select: BTreeSet<&'a [u8]>, stdout: StdoutLock<'a>) -> Self {
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

pub struct SelectStage2<'a> {
    // varying
    stack_pointer: i32,

    stack: [SelectStage2Context; 64],
    input_index_to_keep: u32,
    has_output: bool,

    // static
    labeled: bool,
    select_tree: BTreeMap<&'a [u8], u16>,
    select_vec: Vec<&'a [u8]>,
    stdout: std::io::StdoutLock<'a>,
    unescape: escape::GenericUnescape,
}

impl<'a> SelectStage2<'a> {
    pub fn new<T: IntoIterator<Item = &'a [u8]>>(iter: T, stdout: StdoutLock<'a>, labeled: bool) -> Self {
        let select_vec: Vec<&'a [u8]> = iter.into_iter().collect();
        let mut select_tree: BTreeMap<&'a [u8], u16> = BTreeMap::new();
        for (key_id, key) in select_vec.iter().enumerate() {
            select_tree.insert(key, key_id.try_into().unwrap());
        }
        Self {
            stack_pointer: 0,
            stack: [SelectStage2Context::Start; 64],
            input_index_to_keep: 0,
            has_output: false,
            labeled,
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
            b')' => {
                match self.stack[self.stack_pointer as usize] {
                    SelectStage2Context::Selected(key_id, start_offset) => {
                        // TODO: escape key if necessary
                        let key = self.select_vec[key_id as usize];
                        let value = &input.input[(start_offset as usize - input.offset)..(this_index as usize - input.offset)];
                        if self.labeled {
                            self.stdout.write_vectored(&[
                                IoSlice::new(&b"(("[(self.has_output as usize)..]),
                                IoSlice::new(&key[..]),
                                IoSlice::new(&b" "[..]),
                                IoSlice::new(&value[..]),
                                IoSlice::new(&b")"[..]),
                            ]).unwrap();
                        } else {
                            self.stdout.write_vectored(&[
                                IoSlice::new(if self.has_output { &b" "[..] } else { &b"("[..] }),
                                IoSlice::new(&value[..]),
                            ]).unwrap();
                        }
                        self.has_output = true;
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
                    },
                    SelectStage2Context::Selected(_, _) => {
                        self.stack[self.stack_pointer as usize] = SelectStage2Context::Ignore;
                    },
                    SelectStage2Context::Start => {
                        if ch == b'(' {
                            self.stack[self.stack_pointer as usize] = SelectStage2Context::Ignore;
                        } else {
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
                        }
                    },
                    _ => (),
                }
            },
        }

        self.input_index_to_keep = if self.stack_pointer == 0 { next_index as u32 } else { self.input_index_to_keep };

        self.stack_pointer += (ch == b'(') as i32;
        self.stack_pointer -= (ch == b')') as i32;

        if self.stack_pointer == 0 && self.has_output {
            self.stdout.write(&b")\n"[..]).unwrap();
            self.has_output = false;
        }

        assert!((self.stack_pointer as usize) < self.stack.len(), "Too deeply nested");
        if unlikely(self.stack_pointer < 0) {
            return Err(parser::Error::UnmatchedCloseParen);
        }

        Ok(self.input_index_to_keep as usize)
    }

    fn process_eof(&mut self) -> Result<Self::FinalReturnType, parser::Error> {
        Ok(())
    }
}
