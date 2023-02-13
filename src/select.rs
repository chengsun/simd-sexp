use crate::escape::{self, Unescape};
use crate::escape_csv;
use crate::parser;
#[cfg(feature = "threads")]
use crate::parser_parallel;
use crate::utils;
use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::ops::Range;

#[derive(Copy, Clone, Debug)]
enum State {
    Start,
    SelectNext(u32),
    Selected(u32, usize),
    Ignore,
}

#[derive(Copy, Clone, Debug)]
pub enum OutputKind {
    Values,
    Labeled,
    Csv { atoms_as_sexps: bool },
}

pub trait Output {
    fn reset(&mut self, keys: &Vec<&[u8]>);
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
    fn reset(&mut self, _keys: &Vec<&[u8]>) {
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
    fn reset(&mut self, _keys: &Vec<&[u8]>) {
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

    pub fn print_header<'a, KeysT: Iterator<Item=&'a [u8]>, WriteT: Write>(keys: KeysT, writer: &mut WriteT) {
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
    }
}

impl Output for OutputCsv {
    fn reset(&mut self, keys: &Vec<&[u8]>) {
        self.row.resize(keys.len(), 0..0);
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
    stack: Vec<State>,
    has_output_on_line: bool,

    // static
    output: OutputT,
    select_tree: BTreeMap<&'a [u8], u32>,
    select_vec: Vec<&'a [u8]>,
    unescape: escape::GenericUnescape,
}

impl<'a, OutputT> Stage2<'a, OutputT> {
    pub fn new(select_vec: Vec<&'a [u8]>, output: OutputT) -> Self {
        let mut select_tree: BTreeMap<&'a [u8], u32> = BTreeMap::new();
        for (key_id, key) in select_vec.iter().enumerate() {
            select_tree.insert(key, key_id.try_into().unwrap());
        }
        Self {
            stack: Vec::with_capacity(64),
            has_output_on_line: false,
            output,
            select_tree,
            select_vec,
            unescape: escape::GenericUnescape::new(),
        }
    }
}

impl<'a, OutputT: Output> parser::WritingStage2 for Stage2<'a, OutputT> {
    fn reset(&mut self) {
        self.output.reset(&self.select_vec);
    }

    #[inline]
    fn process_one<WriteT: Write>(&mut self, writer: &mut WriteT, input: parser::Input, this_index: usize, next_index: usize, is_eof: bool) -> Result<usize, parser::Error> {
        let ch = input.input[this_index - input.offset];

        let input_index_to_keep = if self.stack.len() == 0 { next_index } else { input.offset };

        match ch {
            b'(' => {
                let stack_index = self.stack.len().wrapping_sub(1);
                match self.stack.get_mut(stack_index) {
                    Some(stack_element) => {
                        match stack_element.clone() {
                            State::SelectNext(key_id) => {
                                *stack_element = State::Selected(key_id, this_index);
                            },
                            State::Selected(_, _) => {
                                *stack_element = State::Ignore;
                            },
                            State::Start => {
                                *stack_element = State::Ignore;
                            },
                            State::Ignore => (),
                        }
                    },
                    None => (),
                }
                self.stack.push(State::Start);
            }
            b')' => {
                match self.stack.pop() {
                    Some(State::Selected(key_id, start_offset)) => {
                        self.output.select(writer, &self.select_vec, key_id as usize, &input, start_offset..this_index, self.has_output_on_line);
                        self.has_output_on_line = true;
                    },
                    None => {
                        utils::cold();
                        return Err(parser::Error::UnmatchedCloseParen);
                    },
                    Some(_) => (),
                }

                if self.stack.len() == 0 && self.has_output_on_line {
                    self.output.eol(writer, &input);
                    self.has_output_on_line = false;
                }
            },
            b' ' | b'\t' | b'\n' => (),
            _ => {
                if is_eof && ch == b'"' {
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
                let stack_index = self.stack.len().wrapping_sub(1);
                match self.stack.get_mut(stack_index) {
                    Some(stack_element) => {
                        match stack_element.clone() {
                            State::SelectNext(key_id) => {
                                *stack_element = State::Selected(key_id, this_index);
                            },
                            State::Selected(_, _) => {
                                *stack_element = State::Ignore;
                            },
                            State::Start => {
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
                                *stack_element = match key_id {
                                    None => State::Ignore,
                                    Some(key_id) => State::SelectNext(key_id),
                                }
                            },
                            State::Ignore => (),
                        }
                    },
                    None => (),
                }
            },
        }

        Ok(input_index_to_keep)
    }

    fn process_eof<WriteT: Write>(&mut self, _writer: &mut WriteT) -> Result<(), parser::Error> {
        if self.stack.len() > 0 {
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
                    Stage2::new(keys.clone(), OutputValues::new())
                }, stdout, chunk_size),
            OutputKind::Labeled =>
                parser_parallel::streaming_from_writing_stage2(move || {
                    Stage2::new(keys.clone(), OutputLabeled::new())
                }, stdout, chunk_size),
            OutputKind::Csv { atoms_as_sexps } => {
                OutputCsv::print_header(keys.iter().map(|x| *x), stdout);
                parser_parallel::streaming_from_writing_stage2(move || {
                    Stage2::new(keys.clone(), OutputCsv::new(atoms_as_sexps))
                }, stdout, chunk_size)
            },
        };
    }

    #[cfg(not(feature = "threads"))]
    let _ = threads;

    match output_kind {
        OutputKind::Values =>
            parser::streaming_from_writing_stage2(Stage2::new(keys, OutputValues::new()), stdout),
        OutputKind::Labeled =>
            parser::streaming_from_writing_stage2(Stage2::new(keys, OutputLabeled::new()), stdout),
        OutputKind::Csv { atoms_as_sexps } => {
            OutputCsv::print_header(keys.iter().map(|x| *x), stdout);
            parser::streaming_from_writing_stage2(Stage2::new(keys, OutputCsv::new(atoms_as_sexps)), stdout)
        },
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
        let () = parser.process_streaming(&mut stdin).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_test(output_kind: OutputKind, input: &[u8], keys: &[&[u8]], expected_output: Result<&[u8], parser::Error>) {
        let mut output = Vec::new();
        let mut parser = make_parser(keys.iter().map(|x| *x), &mut output, output_kind, false);
        let ok = parser.process_streaming(&mut std::io::BufReader::new(input));
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
    fn test_10() {
        let input = b"foo bar";
        let keys = &[&b"foo"[..]];
        run_test(
            OutputKind::Csv { atoms_as_sexps: false },
            input,
            keys,
            Ok(br#"foo
"#));
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
