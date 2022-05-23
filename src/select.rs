use crate::escape::{self, Unescape};
use crate::escape_csv;
use crate::parser;
use crate::utils::unlikely;
use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::ops::Range;

#[derive(Copy, Clone, Debug)]
enum State {
    Start,
    SelectNext(u16),
    Selected(u16, usize),
    Ignore,
}

#[derive(Copy, Clone, Debug)]
pub enum OutputKind {
    Values,
    Labeled,
    Csv { atoms_as_sexps: bool },
}

pub trait Output {
    fn bof<WriteT: Write>(&mut self, writer: &mut WriteT, keys: &Vec<&[u8]>, segment_index: parser::SegmentIndex);
    fn select<WriteT: Write>(&mut self, writer: &mut WriteT, keys: &Vec<&[u8]>, key_id: usize, input: &parser::Input, value_range: Range<usize>);
    fn eol<WriteT: Write>(&mut self, writer: &mut WriteT, input: &parser::Input);
}

pub struct OutputValues {
    has_output_on_line: bool,
}

impl OutputValues {
    pub fn new() -> Self {
        Self { has_output_on_line: false }
    }
}

impl Output for OutputValues {
    fn bof<WriteT: Write>(&mut self, _writer: &mut WriteT, _keys: &Vec<&[u8]>, _segment_index: parser::SegmentIndex) {
    }
    fn select<WriteT: Write>(&mut self, writer: &mut WriteT, _keys: &Vec<&[u8]>, _key_id: usize, input: &parser::Input, value_range: Range<usize>) {
        let value = &input.input[(value_range.start - input.offset)..(value_range.end - input.offset)];
        writer.write_all(if self.has_output_on_line { &b" "[..] } else { &b"("[..] }).unwrap();
        writer.write_all(&value[..]).unwrap();
        self.has_output_on_line = true;
    }
    fn eol<WriteT: Write>(&mut self, writer: &mut WriteT, _input: &parser::Input) {
        if self.has_output_on_line {
            writer.write(&b")\n"[..]).unwrap();
            self.has_output_on_line = false;
        }
    }
}

pub struct OutputLabeled {
    has_output_on_line: bool,
}

impl OutputLabeled {
    pub fn new() -> Self {
        Self { has_output_on_line: false }
    }
}

impl Output for OutputLabeled {
    fn bof<WriteT: Write>(&mut self, _writer: &mut WriteT, _keys: &Vec<&[u8]>, _segment_index: parser::SegmentIndex) {
    }
    fn select<WriteT: Write>(&mut self, writer: &mut WriteT, keys: &Vec<&[u8]>, key_id: usize, input: &parser::Input, value_range: Range<usize>) {
        let key = keys[key_id];
        let value = &input.input[(value_range.start - input.offset)..(value_range.end - input.offset)];
        writer.write_all(&b"(("[(self.has_output_on_line as usize)..]).unwrap();
        // TODO: escape key if necessary
        writer.write_all(&key[..]).unwrap();
        writer.write_all(&b" "[..]).unwrap();
        writer.write_all(&value[..]).unwrap();
        writer.write_all(&b")"[..]).unwrap();
        self.has_output_on_line = true;
    }
    fn eol<WriteT: Write>(&mut self, writer: &mut WriteT, _input: &parser::Input) {
        if self.has_output_on_line {
            writer.write(&b")\n"[..]).unwrap();
            self.has_output_on_line = false;
        }
    }
}

pub struct OutputCsv {
    atoms_as_sexps: bool,
    has_output_on_line: bool,
    row: Vec<Range<usize>>,
}

impl OutputCsv {
    pub fn new(atoms_as_sexps: bool) -> Self {
        Self {
            atoms_as_sexps,
            has_output_on_line: false,
            row: Vec::new(),
        }
    }
}

impl Output for OutputCsv {
    fn bof<WriteT: Write>(&mut self, writer: &mut WriteT, keys: &Vec<&[u8]>, segment_index: parser::SegmentIndex) {
        self.row.resize(keys.len(), 0..0);
        match segment_index {
            parser::SegmentIndex::EntireFile | parser::SegmentIndex::Segment(0) => {
                let mut needs_comma = false;
                for key in keys {
                    if needs_comma {
                        writer.write_all(&b","[..]).unwrap();
                    }
                    needs_comma = true;
                    if escape_csv::escape_is_necessary(key) {
                        escape_csv::escape(key, writer).unwrap();
                    } else {
                        writer.write_all(&key[..]).unwrap();
                    }
                }
                writer.write_all(&b"\n"[..]).unwrap();
            },
            parser::SegmentIndex::Segment(_) => (),
        }
    }
    fn select<WriteT: Write>(&mut self, _writer: &mut WriteT, _keys: &Vec<&[u8]>, key_id: usize, _input: &parser::Input, value_range: Range<usize>) {
        self.row[key_id] = value_range;
        self.has_output_on_line = true;
    }
    fn eol<WriteT: Write>(&mut self, writer: &mut WriteT, input: &parser::Input) {
        if self.has_output_on_line {
            let mut needs_comma = false;
            for value_range in self.row.iter_mut() {
                if needs_comma {
                    writer.write_all(&b","[..]).unwrap();
                }
                needs_comma = true;
                if !Range::is_empty(value_range) {
                    let value = &input.input[(value_range.start - input.offset)..(value_range.end - input.offset)];
                    if !self.atoms_as_sexps && value[0] == b'\"' {
                        // quoted atom -> plain string -> quoted CSV. This could be done faster.
                        let mut plain_string: Vec<u8> = value.iter().map(|_| 0u8).collect();
                        let (_, plain_string_len) = escape::GenericUnescape::new().unescape(&value[1..], &mut plain_string[..]).unwrap();
                        plain_string.truncate(plain_string_len);

                        let plain_string = &plain_string[..];
                        if escape_csv::escape_is_necessary(plain_string) {
                            escape_csv::escape(plain_string, writer).unwrap();
                        } else {
                            writer.write_all(plain_string).unwrap();
                        }
                    } else {
                        if escape_csv::escape_is_necessary(value) {
                            escape_csv::escape(value, writer).unwrap();
                        } else {
                            writer.write_all(value).unwrap();
                        }
                    }
                }

                *value_range = 0..0;
            }
            writer.write_all(&b"\n"[..]).unwrap();
            self.has_output_on_line = false;
        }
    }
}

pub struct Stage2<'a, OutputT> {
    // varying
    stack_pointer: i32,

    stack: [State; 64],
    input_index_to_keep: usize,

    // static
    output: OutputT,
    select_tree: BTreeMap<&'a [u8], u16>,
    select_vec: Vec<&'a [u8]>,
    unescape: escape::GenericUnescape,
}

impl<'a, OutputT> Stage2<'a, OutputT> {
    pub fn new<T: IntoIterator<Item = &'a [u8]>>(iter: T, output: OutputT) -> Self {
        let select_vec: Vec<&'a [u8]> = iter.into_iter().collect();
        let mut select_tree: BTreeMap<&'a [u8], u16> = BTreeMap::new();
        for (key_id, key) in select_vec.iter().enumerate() {
            select_tree.insert(key, key_id.try_into().unwrap());
        }
        Self {
            stack_pointer: 0,
            stack: [State::Start; 64],
            input_index_to_keep: 0,
            output,
            select_tree,
            select_vec,
            unescape: escape::GenericUnescape::new(),
        }
    }
}

impl<'a, OutputT: Output> parser::WritingStage2 for Stage2<'a, OutputT> {
    fn process_bof<WriteT: Write>(&mut self, writer: &mut WriteT, segment_index: parser::SegmentIndex) {
        self.output.bof(writer, &self.select_vec, segment_index);
    }

    #[inline(always)]
    fn process_one<WriteT: Write>(&mut self, writer: &mut WriteT, input: parser::Input, this_index: usize, next_index: usize) -> Result<usize, parser::Error> {
        let ch = input.input[this_index - input.offset];
        match ch {
            b')' => {
                match self.stack[self.stack_pointer as usize] {
                    State::Selected(key_id, start_offset) => {
                        self.output.select(writer, &self.select_vec, key_id as usize, &input, start_offset..this_index);
                    },
                    _ => (),
                }
                self.stack[self.stack_pointer as usize] = State::Start;
            },
            b' ' | b'\t' | b'\n' => (),
            _ => {
                match self.stack[self.stack_pointer as usize] {
                    State::SelectNext(key_id) => {
                        self.stack[self.stack_pointer as usize] = State::Selected(key_id, this_index);
                    },
                    State::Selected(_, _) => {
                        self.stack[self.stack_pointer as usize] = State::Ignore;
                    },
                    State::Start => {
                        if ch == b'(' {
                            self.stack[self.stack_pointer as usize] = State::Ignore;
                        } else {
                            let key_id =
                                if ch == b'"' {
                                    // TODO: there are a lot of early-outs we could be applying here.
                                    let mut buf: Vec<u8> = (0..(next_index - this_index)).map(|_| 0u8).collect();
                                    self.unescape.unescape(
                                        &input.input[(this_index + 1 - input.offset)..(next_index - input.offset)],
                                        &mut buf[..])
                                        .and_then(|(_, output_len)| self.select_tree.get(&buf[..output_len]))
                                        .map(|x| *x)
                                } else {
                                    self.select_tree.get(&input.input[(this_index - input.offset)..(next_index - input.offset)]).map(|x| *x)
                                };
                            self.stack[self.stack_pointer as usize] = match key_id {
                                None => State::Ignore,
                                Some(key_id) => State::SelectNext(key_id),
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
            self.output.eol(writer, &input);
        }

        Ok(self.input_index_to_keep)
    }

    fn process_eof<WriteT: Write>(&mut self, _writer: &mut WriteT) -> Result<(), parser::Error> {
        Ok(())
    }
}

pub fn make_parser<'a, KeysT: IntoIterator<Item = &'a [u8]>, ReadT: BufRead, WriteT: Write>
    (keys: KeysT, stdout: &'a mut WriteT, assume_machine_input: bool, output_kind: OutputKind)
    -> Box<dyn parser::StateI<(), ReadT> + 'a> {
    match (assume_machine_input, output_kind) {
        (_, OutputKind::Values) =>
            Box::new(parser::State::from_writing_stage2(Stage2::new(keys, OutputValues::new()), stdout)),
        (_, OutputKind::Labeled) =>
            Box::new(parser::State::from_writing_stage2(Stage2::new(keys, OutputLabeled::new()), stdout)),
        (_, OutputKind::Csv { atoms_as_sexps }) =>
            Box::new(parser::State::from_writing_stage2(Stage2::new(keys, OutputCsv::new(atoms_as_sexps)), stdout)),
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

        let mut parser = make_parser(keys, &mut stdout, assume_machine_input, output_kind);
        let () = parser.process_streaming(&mut stdin).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_test(assume_machine_input: bool, output_kind: OutputKind, input: &[u8], keys: &[&[u8]], expected_output: &[u8]) {
        let mut output = Vec::new();
        let mut parser = make_parser(keys.iter().map(|x| *x), &mut output, assume_machine_input, output_kind);
        let () = parser.process_streaming(parser::SegmentIndex::EntireFile, &mut std::io::BufReader::new(input)).unwrap();
        std::mem::drop(parser);

        assert_eq!(std::str::from_utf8(&output[..]).unwrap(),
                   std::str::from_utf8(expected_output).unwrap(),
                   "assume_machine_input: {}, output_kind: {:?}", assume_machine_input, output_kind);
    }

    #[test]
    fn test_1() {
        let input = br#"((foo bar))"#;
        let keys = &[&b"foo"[..]];
        run_test(
            false,
            OutputKind::Csv { atoms_as_sexps: false },
            input,
            keys,
            br#"foo
bar
"#);
        run_test(
            false,
            OutputKind::Csv { atoms_as_sexps: true },
            input,
            keys,
            br#"foo
bar
"#);
    }

    #[test]
    fn test_2() {
        let input = br#"((foo"bar"))"#;
        let keys = &[&b"foo"[..]];
        run_test(
            false,
            OutputKind::Csv { atoms_as_sexps: false },
            input,
            keys,
            br#"foo
bar
"#);
        run_test(
            false,
            OutputKind::Csv { atoms_as_sexps: true },
            input,
            keys,
            br#"foo
"""bar"""
"#);
    }

    #[test]
    fn test_3() {
        let input = br#"(("foo"bar))"#;
        let keys = &[&b"foo"[..]];
        run_test(
            false,
            OutputKind::Csv { atoms_as_sexps: false },
            input,
            keys,
            br#"foo
bar
"#);
        run_test(
            false,
            OutputKind::Csv { atoms_as_sexps: true },
            input,
            keys,
            br#"foo
bar
"#);
    }

    #[test]
    fn test_4() {
        let input = br#"((foo"bar\"baz"))"#;
        let keys = &[&b"foo"[..]];
        run_test(
            false,
            OutputKind::Csv { atoms_as_sexps: false },
            input,
            keys,
            br#"foo
"bar""baz"
"#);
        run_test(
            false,
            OutputKind::Csv { atoms_as_sexps: true },
            input,
            keys,
            br#"foo
"""bar\""baz"""
"#);
    }

    #[test]
    fn test_5() {
        let input = br#"((foo"bar baz"))"#;
        let keys = &[&b"foo"[..]];
        run_test(
            false,
            OutputKind::Csv { atoms_as_sexps: false },
            input,
            keys,
            br#"foo
bar baz
"#);
        run_test(
            false,
            OutputKind::Csv { atoms_as_sexps: true },
            input,
            keys,
            br#"foo
"""bar baz"""
"#);
    }

    #[test]
    fn test_6() {
        let input = br#"((foo bar,baz))"#;
        let keys = &[&b"foo"[..]];
        run_test(
            false,
            OutputKind::Csv { atoms_as_sexps: false },
            input,
            keys,
            br#"foo
"bar,baz"
"#);
        run_test(
            false,
            OutputKind::Csv { atoms_as_sexps: true },
            input,
            keys,
            br#"foo
"bar,baz"
"#);
    }

    #[test]
    fn test_7() {
        let input = br#"((foo "bar, baz"))"#;
        let keys = &[&b"foo"[..]];
        run_test(
            false,
            OutputKind::Csv { atoms_as_sexps: false },
            input,
            keys,
            br#"foo
"bar, baz"
"#);
        run_test(
            false,
            OutputKind::Csv { atoms_as_sexps: true },
            input,
            keys,
            br#"foo
"""bar, baz"""
"#);
    }
}
