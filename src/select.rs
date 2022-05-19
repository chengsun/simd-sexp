use crate::escape::{self, Unescape};
use crate::escape_csv;
use crate::parser;
use crate::utils::unlikely;
use std::collections::BTreeMap;
use std::io::Write;
use std::ops::Range;

#[derive(Copy, Clone, Debug)]
enum SelectStage2Context {
    Start,
    SelectNext(u16),
    Selected(u16, usize),
    Ignore,
}

pub enum SelectStage2OutputKind {
    Values,
    Labeled,
    Csv { atoms_as_sexps: bool },
}

trait SelectStage2Output {
    fn bof(&mut self, keys: &Vec<&[u8]>);
    fn select(&mut self, keys: &Vec<&[u8]>, key_id: usize, input: &parser::Input, value_range: Range<usize>);
    fn eol(&mut self, input: &parser::Input);
}

pub struct SelectStage2OutputValues<'a, StdoutT> {
    has_output_on_line: bool,
    stdout: &'a mut StdoutT,
}

impl<'a, StdoutT> SelectStage2OutputValues<'a, StdoutT> {
    pub fn new(stdout: &'a mut StdoutT) -> Self {
        Self { has_output_on_line: false, stdout }
    }
}

impl<'a, StdoutT: Write> SelectStage2Output for SelectStage2OutputValues<'a, StdoutT> {
    fn bof(&mut self, _keys: &Vec<&[u8]>) {
    }
    fn select(&mut self, _keys: &Vec<&[u8]>, _key_id: usize, input: &parser::Input, value_range: Range<usize>) {
        let value = &input.input[(value_range.start - input.offset)..(value_range.end - input.offset)];
        self.stdout.write_all(if self.has_output_on_line { &b" "[..] } else { &b"("[..] }).unwrap();
        self.stdout.write_all(&value[..]).unwrap();
        self.has_output_on_line = true;
    }
    fn eol(&mut self, _input: &parser::Input) {
        if self.has_output_on_line {
            self.stdout.write(&b")\n"[..]).unwrap();
            self.has_output_on_line = false;
        }
    }
}

pub struct SelectStage2OutputLabeled<'a, StdoutT> {
    has_output_on_line: bool,
    stdout: &'a mut StdoutT,
}

impl<'a, StdoutT> SelectStage2OutputLabeled<'a, StdoutT> {
    pub fn new(stdout: &'a mut StdoutT) -> Self {
        Self { has_output_on_line: false, stdout }
    }
}

impl<'a, StdoutT: Write> SelectStage2Output for SelectStage2OutputLabeled<'a, StdoutT> {
    fn bof(&mut self, _keys: &Vec<&[u8]>) {
    }
    fn select(&mut self, keys: &Vec<&[u8]>, key_id: usize, input: &parser::Input, value_range: Range<usize>) {
        let key = keys[key_id];
        let value = &input.input[(value_range.start - input.offset)..(value_range.end - input.offset)];
        self.stdout.write_all(&b"(("[(self.has_output_on_line as usize)..]).unwrap();
        // TODO: escape key if necessary
        self.stdout.write_all(&key[..]).unwrap();
        self.stdout.write_all(&b" "[..]).unwrap();
        self.stdout.write_all(&value[..]).unwrap();
        self.stdout.write_all(&b")"[..]).unwrap();
        self.has_output_on_line = true;
    }
    fn eol(&mut self, _input: &parser::Input) {
        if self.has_output_on_line {
            self.stdout.write(&b")\n"[..]).unwrap();
            self.has_output_on_line = false;
        }
    }
}

pub struct SelectStage2OutputCsv<'a, StdoutT> {
    atoms_as_sexps: bool,
    has_output_on_line: bool,
    row: Vec<Range<usize>>,
    stdout: &'a mut StdoutT,
}

impl<'a, StdoutT> SelectStage2OutputCsv<'a, StdoutT> {
    pub fn new(stdout: &'a mut StdoutT, atoms_as_sexps: bool) -> Self {
        Self {
            atoms_as_sexps,
            has_output_on_line: false,
            row: Vec::new(),
            stdout,
        }
    }
}

impl<'a, StdoutT: Write> SelectStage2Output for SelectStage2OutputCsv<'a, StdoutT> {
    fn bof(&mut self, keys: &Vec<&[u8]>) {
        self.row.resize(keys.len(), 0..0);
        let mut needs_comma = false;
        for key in keys {
            if needs_comma {
                self.stdout.write_all(&b","[..]).unwrap();
            }
            needs_comma = true;
            if escape_csv::escape_is_necessary(key) {
                escape_csv::escape(key, self.stdout).unwrap();
            } else {
                self.stdout.write_all(&key[..]).unwrap();
            }
        }
        self.stdout.write_all(&b"\n"[..]).unwrap();
    }
    fn select(&mut self, _keys: &Vec<&[u8]>, key_id: usize, _input: &parser::Input, value_range: Range<usize>) {
        self.row[key_id] = value_range;
        self.has_output_on_line = true;
    }
    fn eol(&mut self, input: &parser::Input) {
        if self.has_output_on_line {
            let mut needs_comma = false;
            for value_range in self.row.iter_mut() {
                if needs_comma {
                    self.stdout.write_all(&b","[..]).unwrap();
                }
                needs_comma = true;
                if !Range::is_empty(value_range) {
                    let value = &input.input[(value_range.start - input.offset)..(value_range.end - input.offset)];
                    if !self.atoms_as_sexps && value[0] == b'\"' {
                        // quoted atom -> plain string -> quoted CSV. This could be done faster.
                        let mut plain_string: Vec<u8> = value.iter().map(|_| 0u8).collect();
                        let (_, plain_string_len) = escape::GenericUnescape::new().unescape(value, &mut plain_string[..]).unwrap();
                        plain_string.truncate(plain_string_len);

                        let plain_string = &plain_string[..];
                        if escape_csv::escape_is_necessary(plain_string) {
                            escape_csv::escape(plain_string, self.stdout).unwrap();
                        } else {
                            self.stdout.write_all(plain_string).unwrap();
                        }
                    } else {
                        if escape_csv::escape_is_necessary(value) {
                            escape_csv::escape(value, self.stdout).unwrap();
                        } else {
                            self.stdout.write_all(value).unwrap();
                        }
                    }
                }

                *value_range = 0..0;
            }
            self.stdout.write_all(&b"\n"[..]).unwrap();
            self.has_output_on_line = false;
        }
    }
}

pub struct SelectStage2<'a, OutputT> {
    // varying
    stack_pointer: i32,

    stack: [SelectStage2Context; 64],
    input_index_to_keep: usize,

    // static
    output: OutputT,
    select_tree: BTreeMap<&'a [u8], u16>,
    select_vec: Vec<&'a [u8]>,
    unescape: escape::GenericUnescape,
}

impl<'a, OutputT> SelectStage2<'a, OutputT> {
    pub fn new<T: IntoIterator<Item = &'a [u8]>>(iter: T, output: OutputT) -> Self {
        let select_vec: Vec<&'a [u8]> = iter.into_iter().collect();
        let mut select_tree: BTreeMap<&'a [u8], u16> = BTreeMap::new();
        for (key_id, key) in select_vec.iter().enumerate() {
            select_tree.insert(key, key_id.try_into().unwrap());
        }
        Self {
            stack_pointer: 0,
            stack: [SelectStage2Context::Start; 64],
            input_index_to_keep: 0,
            output,
            select_tree,
            select_vec,
            unescape: escape::GenericUnescape::new(),
        }
    }
}

impl<'a, OutputT: SelectStage2Output> parser::Stage2 for SelectStage2<'a, OutputT> {
    type FinalReturnType = ();

    fn process_bof(&mut self, _input_size_hint: Option<usize>) {
        self.output.bof(&self.select_vec);
    }

    #[inline(always)]
    fn process_one(&mut self, input: parser::Input, this_index: usize, next_index: usize) -> Result<usize, parser::Error> {
        let ch = input.input[this_index - input.offset];
        match ch {
            b')' => {
                match self.stack[self.stack_pointer as usize] {
                    SelectStage2Context::Selected(key_id, start_offset) => {
                        self.output.select(&self.select_vec, key_id as usize, &input, start_offset..this_index);
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
                            let key_id =
                                if ch == b'"' {
                                    // TODO: there are a lot of early-outs we could be applying here.
                                    let mut buf: Vec<u8> = (0..(this_index - input.offset)).map(|_| 0u8).collect();
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

        if self.stack_pointer == 0 {
            self.output.eol(&input);
        }

        Ok(self.input_index_to_keep)
    }

    fn process_eof(&mut self) -> Result<Self::FinalReturnType, parser::Error> {
        Ok(())
    }
}


#[cfg(feature = "ocaml")]
mod ocaml_ffi {
    use super::*;
    use std::collections::LinkedList;
    use crate::utils;

    struct ByteString(Vec<u8>);

    unsafe impl<'a> ocaml::FromValue<'a> for ByteString {
        fn from_value(value: ocaml::Value) -> Self {
            Self(<&[u8]>::from_value(value).to_owned())
        }
    }

    unsafe impl ocaml::IntoValue for ByteString {
        fn into_value(self, _rt: &ocaml::Runtime) -> ocaml::Value {
            unsafe { ocaml::Value::bytes(&self.0[..]) }
        }
    }

    type OutputKind = SelectStage2OutputKind;

    impl OutputKind {
        fn values() -> ocaml::Value {
            unsafe { ocaml::Value::Raw(ocaml::sys::caml_hash_variant(b"Values\0" as *const u8)) }
        }
        fn labeled() -> ocaml::Value {
            unsafe { ocaml::Value::Raw(ocaml::sys::caml_hash_variant(b"Labeled\0" as *const u8)) }
        }
        fn csv() -> ocaml::Value {
            unsafe { ocaml::Value::Raw(ocaml::sys::caml_hash_variant(b"Csv\0" as *const u8)) }
        }
    }

    unsafe impl<'a> ocaml::FromValue<'a> for OutputKind {
        fn from_value(v: ocaml::Value) -> Self {
            unsafe {
                assert!(v.is_long());
                if v.int_val() == Self::values().int_val() {
                    OutputKind::Values
                } else if v.int_val() == Self::labeled().int_val() {
                    OutputKind::Labeled
                } else if v.int_val() == Self::csv().int_val() {
                    OutputKind::Csv { atoms_as_sexps: false }
                } else {
                    panic!("Unknown variant ({}) for OutputKind", v.int_val());
                }
            }
        }
    }

    unsafe impl ocaml::IntoValue for OutputKind {
        fn into_value(self, _rt: &ocaml::Runtime) -> ocaml::Value {
            match self {
                OutputKind::Values => Self::values(),
                OutputKind::Labeled => Self::labeled(),
                OutputKind::Csv { atoms_as_sexps: false } => Self::csv(),
                OutputKind::Csv { atoms_as_sexps: true } =>
                    // TODO
                    unimplemented!(),
            }
        }
    }

    #[ocaml::func]
    pub fn ml_multi_select(keys: LinkedList<ByteString>, assume_machine_input: bool, output_kind: OutputKind) {
        let mut stdin = utils::stdin();
        let mut stdout = utils::stdout();

        let keys = keys.iter().map(|s| &s.0[..]);

        let mut parser: Box<dyn parser::StateI<(), _>> =
            match (assume_machine_input, output_kind) {
                (_, SelectStage2OutputKind::Values) =>
                    Box::new(parser::State::new(SelectStage2::new(keys, SelectStage2OutputValues::new(&mut stdout)))),
                (_, SelectStage2OutputKind::Labeled) =>
                    Box::new(parser::State::new(SelectStage2::new(keys, SelectStage2OutputLabeled::new(&mut stdout)))),
                (_, SelectStage2OutputKind::Csv { atoms_as_sexps }) =>
                    Box::new(parser::State::new(SelectStage2::new(keys, SelectStage2OutputCsv::new(&mut stdout, atoms_as_sexps )))),
            };
        let () = parser.process_streaming(&mut stdin).unwrap();
    }
}
