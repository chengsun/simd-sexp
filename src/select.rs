use crate::escape::{self, Unescape};
use crate::escape_csv;
use crate::parser;
#[cfg(feature = "threads")]
use crate::parser_parallel;
use crate::utils::unlikely;
use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::ops::Range;

#[derive(Copy, Clone, Debug)]
enum StateKind {
    Start,
    SelectNext,
    Selected,
    Ignore,
}

#[derive(Copy, Clone, Debug)]
pub struct State {
    kind: StateKind,
    key_id: u16,
    start_offset: usize,
}

#[derive(Copy, Clone, Debug)]
pub enum OutputKind {
    Values,
    Labeled,
    Csv { atoms_as_sexps: bool },
}

pub trait Output {
    fn bof<WriteT: Write>(&mut self, writer: &mut WriteT, keys: &Vec<&[u8]>, segment_index: parser::SegmentIndex);
    fn select<WriteT: Write>(&mut self, writer: &mut WriteT, keys: &Vec<&[u8]>, key_id: usize, input: &parser::Input, value_range: Range<usize>, has_output_on_line: bool);
    fn eol<WriteT: Write>(&mut self, writer: &mut WriteT, input: &parser::Input);
}

pub struct OutputValues {
}

impl OutputValues {
    pub fn new() -> Self {
        Self { }
    }
}

impl Output for OutputValues {
    fn bof<WriteT: Write>(&mut self, _writer: &mut WriteT, _keys: &Vec<&[u8]>, _segment_index: parser::SegmentIndex) {
    }
    fn select<WriteT: Write>(&mut self, writer: &mut WriteT, _keys: &Vec<&[u8]>, _key_id: usize, input: &parser::Input, value_range: Range<usize>, has_output_on_line: bool) {
        let value = &input.input[(value_range.start - input.offset)..(value_range.end - input.offset)];
        writer.write_all(if has_output_on_line { &b" "[..] } else { &b"("[..] }).unwrap();
        writer.write_all(&value[..]).unwrap();
    }
    fn eol<WriteT: Write>(&mut self, writer: &mut WriteT, _input: &parser::Input) {
        writer.write(&b")\n"[..]).unwrap();
    }
}

pub struct OutputLabeled {
}

impl OutputLabeled {
    pub fn new() -> Self {
        Self { }
    }
}

impl Output for OutputLabeled {
    fn bof<WriteT: Write>(&mut self, _writer: &mut WriteT, _keys: &Vec<&[u8]>, _segment_index: parser::SegmentIndex) {
    }
    fn select<WriteT: Write>(&mut self, writer: &mut WriteT, keys: &Vec<&[u8]>, key_id: usize, input: &parser::Input, value_range: Range<usize>, has_output_on_line: bool) {
        let key = keys[key_id];
        let value = &input.input[(value_range.start - input.offset)..(value_range.end - input.offset)];
        writer.write_all(&b"(("[(has_output_on_line as usize)..]).unwrap();
        // TODO: escape key if necessary
        writer.write_all(&key[..]).unwrap();
        writer.write_all(&b" "[..]).unwrap();
        writer.write_all(&value[..]).unwrap();
        writer.write_all(&b")"[..]).unwrap();
    }
    fn eol<WriteT: Write>(&mut self, writer: &mut WriteT, _input: &parser::Input) {
        writer.write(&b")\n"[..]).unwrap();
    }
}

pub struct OutputCsv {
    atoms_as_sexps: bool,
    row: Vec<Range<usize>>,
}

impl OutputCsv {
    pub fn new(atoms_as_sexps: bool) -> Self {
        Self {
            atoms_as_sexps,
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
    fn select<WriteT: Write>(&mut self, _writer: &mut WriteT, _keys: &Vec<&[u8]>, key_id: usize, _input: &parser::Input, value_range: Range<usize>, _has_output_on_line: bool) {
        self.row[key_id] = value_range;
    }
    fn eol<WriteT: Write>(&mut self, writer: &mut WriteT, input: &parser::Input) {
        let mut needs_comma = false;
        for value_range in self.row.iter_mut() {
            if needs_comma {
                writer.write_all(&b","[..]).unwrap();
            }
            needs_comma = true;
            if !Range::is_empty(value_range) {
                let value = &input.input[(value_range.start - input.offset)..(value_range.end - input.offset)];
                if !self.atoms_as_sexps && value[0] == b'\"' {
                    // quoted atom -> plain string -> quoted CSV.
                    // TODO: This could be done faster.
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
    }
}

pub struct Stage2<'a, OutputT> {
    // varying
    stack_pointer: i32,

    stack: [State; 64],
    has_output_on_line: bool,

    // static
    output: OutputT,
    select_tree: BTreeMap<&'a [u8], u16>,
    select_vec: Vec<&'a [u8]>,
    unescape: escape::GenericUnescape,
    action_lut: ActionLut,
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
            stack: [State { kind: StateKind::Start, key_id: 0, start_offset: 0 }; 64],
            has_output_on_line: false,
            output,
            select_tree,
            select_vec,
            unescape: escape::GenericUnescape::new(),
            action_lut: ActionLut::new(),
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum Action {
    NoOp,
    SetStart,
    MaybeSetSelectNext,
    SetSelected,
    SetIgnore,
    OutputAndSetStart,
}

struct ActionLut {
    lut: Box<[Action; 256 * 4]>,
}

impl ActionLut {
    fn new() -> Self {
        let mut lut = Box::new([Action::NoOp; 256 * 4]);
        for index in 0..(256 * 4) {
            let (ch, state_kind) = Self::unindex(index);

            lut[index] = match (ch, state_kind) {
                (b')', StateKind::Selected) => Action::OutputAndSetStart,
                (b')', _) => Action::SetStart,
                (b' ' | b'\t' | b'\n', _) => Action::NoOp,
                (_, StateKind::SelectNext) => Action::SetSelected,
                (_, StateKind::Selected) => Action::SetIgnore,
                (b'(', StateKind::Start) => Action::SetIgnore,
                (_, StateKind::Start) => Action::MaybeSetSelectNext,
                (_, StateKind::Ignore) => Action::NoOp,
            }
        };

        Self { lut }
    }
    fn unindex(index: usize) -> (u8, StateKind) {
        ((index % 256) as u8, match index >> 8 {
            0 => StateKind::Start,
            1 => StateKind::SelectNext,
            2 => StateKind::Selected,
            3 => StateKind::Ignore,
            _ => panic!("unreachable"),
        })
    }
    fn to_index(ch: u8, state_kind: StateKind) -> usize {
        (ch as usize) | (match state_kind {
            StateKind::Start => 0,
            StateKind::SelectNext => 1,
            StateKind::Selected => 2,
            StateKind::Ignore => 3,
        } << 8)
    }
    fn lookup(&self, ch: u8, state_kind: StateKind) -> Action {
        self.lut[Self::to_index(ch, state_kind)]
    }
}

impl<'a, OutputT: Output> parser::WritingStage2 for Stage2<'a, OutputT> {
    fn process_bof<WriteT: Write>(&mut self, writer: &mut WriteT, segment_index: parser::SegmentIndex) {
        self.output.bof(writer, &self.select_vec, segment_index);
    }

    #[inline(always)]
    fn process_one<WriteT: Write>(&mut self, writer: &mut WriteT, input: parser::Input, this_index: usize, next_index: usize, is_eof: bool) -> Result<usize, parser::Error> {
        let ch = input.input[this_index - input.offset];
        if unlikely(is_eof && ch == b'"') {
            // We have to attempt to unescape the last atom just to
            // check validity.
            // TODO: this implementation is super bad!
            // TODO: the fact we have to do this here at all is fragile
            // and bad!
            let mut out: Vec<u8> = (this_index..next_index).map(|_| 0u8).collect();
            let (_, _) =
                escape::GenericUnescape::new()
                .unescape(
                    &input.input[(this_index + 1 - input.offset)..(next_index - input.offset)],
                    &mut out[..])
                .ok_or(parser::Error::BadQuotedAtom)?;
        }
        let state = &mut self.stack[self.stack_pointer as usize];
        match self.action_lut.lookup(ch, state.kind) {
            Action::NoOp => (),
            Action::SetStart => {
                state.kind = StateKind::Start;
            }
            Action::OutputAndSetStart => {
                self.output.select(writer, &self.select_vec, state.key_id as usize, &input, state.start_offset..this_index, self.has_output_on_line);
                self.has_output_on_line = true;
                state.kind = StateKind::Start;
            },
            Action::SetSelected => {
                self.stack[self.stack_pointer as usize].kind = StateKind::Selected;
                self.stack[self.stack_pointer as usize].start_offset = this_index;
            },
            Action::SetIgnore => {
                self.stack[self.stack_pointer as usize].kind = StateKind::Ignore;
            },
            Action::MaybeSetSelectNext => {
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
                match key_id {
                    None => {
                        self.stack[self.stack_pointer as usize].kind = StateKind::Ignore;
                    },
                    Some(key_id) => {
                        self.stack[self.stack_pointer as usize].kind = StateKind::SelectNext;
                        self.stack[self.stack_pointer as usize].key_id = key_id;
                    },
                }
            },
        }

        let input_index_to_keep = if self.stack_pointer == 0 { next_index } else { input.offset };

        self.stack_pointer += (ch == b'(') as i32;
        self.stack_pointer -= (ch == b')') as i32;

        if unlikely(self.stack_pointer < 0) {
            return Err(parser::Error::UnmatchedCloseParen);
        }
        assert!((self.stack_pointer as usize) < self.stack.len(), "Too deeply nested");

        if self.stack_pointer == 0 && self.has_output_on_line {
            self.output.eol(writer, &input);
            self.has_output_on_line = false;
        }

        Ok(input_index_to_keep)
    }

    fn process_eof<WriteT: Write>(&mut self, _writer: &mut WriteT) -> Result<(), parser::Error> {
        if self.stack_pointer > 0 {
            return Err(parser::Error::UnmatchedOpenParen);
        }
        Ok(())
    }
}

pub fn make_parser<'a, KeysT: IntoIterator<Item = &'a [u8]>, ReadT: BufRead + Send, WriteT: Write>
    (keys: KeysT, stdout: &'a mut WriteT, output_kind: OutputKind, threads: bool)
    -> Box<dyn parser::Stream<ReadT, Return = ()> + 'a>
{
    let keys: Vec<&'a [u8]> = keys.into_iter().collect();

    #[cfg(feature = "threads")]
    if threads {
        let chunk_size = 256 * 1024;
        return match output_kind {
            OutputKind::Values =>
                parser_parallel::streaming_from_writing_stage2(move || {
                    Stage2::new(keys.iter().map(|x| *x), OutputValues::new())
                }, stdout, chunk_size),
            OutputKind::Labeled =>
                parser_parallel::streaming_from_writing_stage2(move || {
                    Stage2::new(keys.iter().map(|x| *x), OutputLabeled::new())
                }, stdout, chunk_size),
            OutputKind::Csv { atoms_as_sexps } =>
                parser_parallel::streaming_from_writing_stage2(move || {
                    Stage2::new(keys.iter().map(|x| *x), OutputCsv::new(atoms_as_sexps))
                }, stdout, chunk_size),
        };
    }

    match output_kind {
        OutputKind::Values =>
            parser::streaming_from_writing_stage2(Stage2::new(keys, OutputValues::new()), stdout),
        OutputKind::Labeled =>
            parser::streaming_from_writing_stage2(Stage2::new(keys, OutputLabeled::new()), stdout),
        OutputKind::Csv { atoms_as_sexps } =>
            parser::streaming_from_writing_stage2(Stage2::new(keys, OutputCsv::new(atoms_as_sexps)), stdout),
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
    pub fn ml_multi_select(keys: LinkedList<ByteString>, output_kind: OutputKind, threads: bool) {
        let mut stdin = utils::stdin();
        let mut stdout = utils::stdout();

        let keys = keys.iter().map(|s| &s.0[..]);

        let mut parser = make_parser(keys, &mut stdout, output_kind, threads);
        let () = parser.process_streaming(parser::SegmentIndex::EntireFile, &mut stdin).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_test(output_kind: OutputKind, input: &[u8], keys: &[&[u8]], expected_output: Result<&[u8], parser::Error>) {
        let mut output = Vec::new();
        let mut parser = make_parser(keys.iter().map(|x| *x), &mut output, output_kind, false);
        let ok = parser.process_streaming(parser::SegmentIndex::EntireFile, &mut std::io::BufReader::new(input));
        std::mem::drop(parser);
        let output = ok.map(move |()| output);

        assert_eq!(output.map(|output| String::from_utf8(output).unwrap()),
                   expected_output.map(|expected_output| String::from_utf8(expected_output.to_owned()).unwrap()),
                   "output_kind: {:?}", output_kind);
    }

    #[test]
    fn test_1() {
        let input = br#"((foo bar))"#;
        let keys = &[&b"foo"[..]];
        run_test(
            OutputKind::Csv { atoms_as_sexps: false },
            input,
            keys,
            Ok(br#"foo
bar
"#));
        run_test(
            OutputKind::Csv { atoms_as_sexps: true },
            input,
            keys,
            Ok(br#"foo
bar
"#));
    }

    #[test]
    fn test_2() {
        let input = br#"((foo"bar"))"#;
        let keys = &[&b"foo"[..]];
        run_test(
            OutputKind::Csv { atoms_as_sexps: false },
            input,
            keys,
            Ok(br#"foo
bar
"#));
        run_test(
            OutputKind::Csv { atoms_as_sexps: true },
            input,
            keys,
            Ok(br#"foo
"""bar"""
"#));
    }

    #[test]
    fn test_3() {
        let input = br#"(("foo"bar))"#;
        let keys = &[&b"foo"[..]];
        run_test(
            OutputKind::Csv { atoms_as_sexps: false },
            input,
            keys,
            Ok(br#"foo
bar
"#));
        run_test(
            OutputKind::Csv { atoms_as_sexps: true },
            input,
            keys,
            Ok(br#"foo
bar
"#));
    }

    #[test]
    fn test_4() {
        let input = br#"((foo"bar\"baz"))"#;
        let keys = &[&b"foo"[..]];
        run_test(
            OutputKind::Csv { atoms_as_sexps: false },
            input,
            keys,
            Ok(br#"foo
"bar""baz"
"#));
        run_test(
            OutputKind::Csv { atoms_as_sexps: true },
            input,
            keys,
            Ok(br#"foo
"""bar\""baz"""
"#));
    }

    #[test]
    fn test_5() {
        let input = br#"((foo"bar baz"))"#;
        let keys = &[&b"foo"[..]];
        run_test(
            OutputKind::Csv { atoms_as_sexps: false },
            input,
            keys,
            Ok(br#"foo
bar baz
"#));
        run_test(
            OutputKind::Csv { atoms_as_sexps: true },
            input,
            keys,
            Ok(br#"foo
"""bar baz"""
"#));
    }

    #[test]
    fn test_6() {
        let input = br#"((foo bar,baz))"#;
        let keys = &[&b"foo"[..]];
        run_test(
            OutputKind::Csv { atoms_as_sexps: false },
            input,
            keys,
            Ok(br#"foo
"bar,baz"
"#));
        run_test(
            OutputKind::Csv { atoms_as_sexps: true },
            input,
            keys,
            Ok(br#"foo
"bar,baz"
"#));
    }

    #[test]
    fn test_7() {
        let input = br#"((foo "bar, baz"))"#;
        let keys = &[&b"foo"[..]];
        run_test(
            OutputKind::Csv { atoms_as_sexps: false },
            input,
            keys,
            Ok(br#"foo
"bar, baz"
"#));
        run_test(
            OutputKind::Csv { atoms_as_sexps: true },
            input,
            keys,
            Ok(br#"foo
"""bar, baz"""
"#));
    }

    #[test]
    fn test_8() {
        let input = b"(";
        let keys = &[];
        run_test(
            OutputKind::Csv { atoms_as_sexps: false },
            input,
            keys,
            Err(parser::Error::UnmatchedOpenParen));
    }

    #[test]
    fn test_9() {
        let input = b"\"";
        let keys = &[];
        run_test(
            OutputKind::Csv { atoms_as_sexps: false },
            input,
            keys,
            Err(parser::Error::BadQuotedAtom));
    }

    #[test]
    fn test_empty() {
        let input = b"";
        let keys = &[];
        run_test(
            OutputKind::Csv { atoms_as_sexps: false },
            input,
            keys,
            Ok(b"\n"));
    }
}
