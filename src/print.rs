use crate::escape::{self, Unescape};
use crate::parser;
#[cfg(feature = "threads")]
use crate::parser_parallel;
use std::io::{BufRead, Write};

pub struct Stage2 {
    escape_is_necessary: escape::IsNecessary,
    unescape: escape::GenericUnescape,
    naked_atom_needs_space: bool,
    depth: usize,
}

impl Stage2 {
    pub fn new() -> Self {
        Self {
            escape_is_necessary: escape::IsNecessary::new(),
            unescape: escape::GenericUnescape::new(),
            naked_atom_needs_space: false,
            depth: 0,
        }
    }
}

impl parser::WritingStage2 for Stage2 {
    fn process_bof<WriteT: Write>(&mut self, _: &mut WriteT) {
        self.naked_atom_needs_space = false;
        self.depth = 0;
    }

    #[inline]
    fn process_one<WriteT: Write>(&mut self, writer: &mut WriteT, input: parser::Input, this_index: usize, next_index: usize, _is_eof: bool) -> Result<usize, parser::Error> {
        let ch = input.input[this_index - input.offset];

        let mut output_atom = |atom: &[u8]| {
            if self.escape_is_necessary.eval(&atom[..]) {
                writer.write_all(&b"\""[..]).unwrap();
                escape::escape(&atom[..], writer).unwrap();
                writer.write_all(&b"\""[..]).unwrap();
                self.naked_atom_needs_space = false;
            } else {
                if self.naked_atom_needs_space {
                    writer.write_all(&b" "[..]).unwrap();
                }
                writer.write_all(&atom[..]).unwrap();
                self.naked_atom_needs_space = true;
            }
            if self.depth == 0 {
                writer.write_all(&b"\n"[..]).unwrap();
                self.naked_atom_needs_space = false;
            }
        };


        match ch {
            b'(' => {
                writer.write_all(&b"("[..]).unwrap();
                self.depth = self.depth.checked_add(1).unwrap();
                self.naked_atom_needs_space = false;
            }
            b')' => {
                writer.write_all(&b")"[..]).unwrap();
                self.depth = self.depth.checked_sub(1).unwrap();
                self.naked_atom_needs_space = false;
                if self.depth == 0 {
                    writer.write_all(&b"\n"[..]).unwrap();
                }
            },
            b' ' | b'\t' | b'\n' => (),
            b'"' => {
                let mut buf: Vec<u8> = (0..(next_index - this_index)).map(|_| 0u8).collect();
                let (_, output_index) =
                    self.unescape.unescape(
                        &input.input[(this_index + 1 - input.offset)..(next_index - input.offset)],
                        &mut buf[..]).unwrap();
                output_atom(&buf[..output_index]);
            },
            _ => {
                output_atom(&input.input[(this_index - input.offset)..(next_index - input.offset)]);
            },
        }

        Ok(next_index)
    }

    fn process_eof<WriteT: Write>(&mut self, _writer: &mut WriteT) -> Result<(), parser::Error> {
        Ok(())
    }
}

pub fn make<'a, ReadT: BufRead + Send, WriteT: Write>
    (stdout: &'a mut WriteT, threads: bool)
    -> Box<dyn parser::Stream<ReadT, Return = ()> + 'a>
{
    #[cfg(feature = "threads")]
    if threads {
        let chunk_size = 256 * 1024;
        return parser_parallel::streaming_from_writing_stage2(|| { Stage2::new() }, stdout, chunk_size);
    }

    #[cfg(not(feature = "threads"))]
    let _ = threads;

    parser::streaming_from_writing_stage2(Stage2::new(), stdout)
}

#[cfg(feature = "ocaml")]
mod ocaml_ffi {
    use super::*;
    use crate::utils;

    #[ocaml::func]
    pub fn ml_print(threads: bool) {
        let mut stdin = utils::stdin();
        let mut stdout = utils::stdout();

        let mut printer = make(&mut stdout, threads);
        let () = printer.process_streaming(&mut stdin).unwrap();
    }
}
