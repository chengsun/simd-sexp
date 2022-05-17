use crate::escape::{self, Unescape};
use crate::parser;
use crate::utils::unlikely;
use crate::visitor;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

pub struct SelectVisitor<'a, StdoutT> {
    select: BTreeSet<&'a [u8]>,
    stdout: &'a mut StdoutT,
    atom_buffer: Option<Vec<u8>>,
}

impl<'a, StdoutT> SelectVisitor<'a, StdoutT> {
    pub fn new<T: IntoIterator<Item = &'a [u8]>>(iter: T, stdout: &'a mut StdoutT) -> Self {
        Self {
            select: iter.into_iter().collect(),
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

impl<'c, StdoutT: Write> visitor::Visitor for SelectVisitor<'c, StdoutT> {
    type IntermediateAtom = Vec<u8>;
    type Context = SelectVisitorContext<'c>;
    type FinalReturnType = ();

    fn bof(&mut self, _input_size_hint: Option<usize>) {
    }

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
                self.stdout.write_all(&value[..]).unwrap();
                self.stdout.write_all(&b"\n"[..]).unwrap();
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
    Selected(u16, usize),
    Ignore,
}

pub struct SelectStage2<'a, StdoutT> {
    // varying
    stack_pointer: i32,

    stack: [SelectStage2Context; 64],
    input_index_to_keep: usize,
    has_output: bool,

    // static
    labeled: bool,
    select_tree: BTreeMap<&'a [u8], u16>,
    select_vec: Vec<&'a [u8]>,
    stdout: &'a mut StdoutT,
    unescape: escape::GenericUnescape,
}

impl<'a, StdoutT> SelectStage2<'a, StdoutT> {
    pub fn new<T: IntoIterator<Item = &'a [u8]>>(iter: T, stdout: &'a mut StdoutT, labeled: bool) -> Self {
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

impl<'a, StdoutT: Write> parser::Stage2 for SelectStage2<'a, StdoutT> {
    type FinalReturnType = ();

    fn process_bof(&mut self, _input_size_hint: Option<usize>) {
    }

    #[inline(always)]
    fn process_one(&mut self, input: parser::Input, this_index: usize, next_index: usize) -> Result<usize, parser::Error> {
        let ch = input.input[this_index - input.offset];
        match ch {
            b')' => {
                match self.stack[self.stack_pointer as usize] {
                    SelectStage2Context::Selected(key_id, start_offset) => {
                        // TODO: escape key if necessary
                        let key = self.select_vec[key_id as usize];
                        let value = &input.input[(start_offset - input.offset)..(this_index - input.offset)];
                        if self.labeled {
                            self.stdout.write_all(&b"(("[(self.has_output as usize)..]).unwrap();
                            self.stdout.write_all(&key[..]).unwrap();
                            self.stdout.write_all(&b" "[..]).unwrap();
                            self.stdout.write_all(&value[..]).unwrap();
                            self.stdout.write_all(&b")"[..]).unwrap();
                        } else {
                            self.stdout.write_all(if self.has_output { &b" "[..] } else { &b"("[..] }).unwrap();
                            self.stdout.write_all(&value[..]).unwrap();
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
                        self.stack[self.stack_pointer as usize] = SelectStage2Context::Selected(key_id, this_index);
                    },
                    SelectStage2Context::Selected(_, _) => {
                        self.stack[self.stack_pointer as usize] = SelectStage2Context::Ignore;
                    },
                    SelectStage2Context::Start => {
                        if ch == b'(' {
                            self.stack[self.stack_pointer as usize] = SelectStage2Context::Ignore;
                        } else {
                            // TODO: this is currently a silent failure in the
                            // case where an atom (which matches a selected key)
                            // is represented in the sexp being parsed as more
                            // than 64 bytes
                            let mut buf = [0u8; 64];
                            let key_id =
                                if ch == b'"' {
                                    self.unescape.unescape(
                                        &input.input[(this_index - input.offset)..std::cmp::min(next_index, this_index - input.offset + 64)],
                                        &mut buf[..])
                                        .and_then(|(_, output_len)| self.select_tree.get(&buf[..output_len]))
                                        .map(|x| *x)
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

        self.input_index_to_keep = if self.stack_pointer == 0 { next_index } else { self.input_index_to_keep };

        self.stack_pointer += (ch == b'(') as i32;
        self.stack_pointer -= (ch == b')') as i32;

        if unlikely(self.stack_pointer < 0) {
            return Err(parser::Error::UnmatchedCloseParen);
        }
        assert!((self.stack_pointer as usize) < self.stack.len(), "Too deeply nested");

        if self.stack_pointer == 0 && self.has_output {
            self.stdout.write(&b")\n"[..]).unwrap();
            self.has_output = false;
        }

        Ok(self.input_index_to_keep)
    }

    fn process_eof(&mut self) -> Result<Self::FinalReturnType, parser::Error> {
        Ok(())
    }
}
