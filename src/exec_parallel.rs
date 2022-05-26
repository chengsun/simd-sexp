use crate::parser;
use crate::parser_parallel;
use std::ffi::OsStr;
use std::io::{BufRead, Write};
use std::process::{Command, Stdio};

#[derive(Clone)]
pub struct ExecWorker<'a> {
    prog: &'a OsStr,
    args: &'a [&'a OsStr],
}

impl<'a> ExecWorker<'a> {
    pub fn new(prog: &'a OsStr, args:&'a [&'a OsStr]) -> Self {
        Self { prog, args }
    }
}

impl<'a> parser::Parse for ExecWorker<'a> {
    type Return = Vec<u8>;
    fn process(&mut self, _segment_index: parser::SegmentIndex, input: &[u8]) -> Result<Self::Return, parser::Error> {
        let threads_result = crossbeam_utils::thread::scope(|scope| {
            let mut command =
                Command::new(self.prog)
                .args(self.args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("Failed to spawn child process");

            let mut stdin = command.stdin.take().expect("Failed to open stdin");

            // TODO: thread-pool?
            scope.spawn(move |_| {
                stdin.write_all(input).expect("Failed to write to stdin");
            });

            let output = command.wait_with_output().expect("Failed to read stdout");
            output.stdout
        });

        match threads_result {
            Err(e) => std::panic::resume_unwind(e),
            Ok(result) => Ok(result),
        }
    }
}

pub fn make_parser<'a, ReadT: BufRead + Send, WriteT: Write>
    (params: ExecWorker<'a>, stdout: &'a mut WriteT)
     -> Box<dyn parser::Stream<ReadT, Return = ()> + 'a>
{
    Box::new(parser_parallel::State::from_worker(move || {
        params.clone()
    }, stdout, 10 * 1024 * 1024))
}

#[cfg(feature = "ocaml")]
mod ocaml_ffi {
    use super::*;
    use std::collections::LinkedList;
    use std::ffi::OsString;
    use crate::utils;

    struct OCamlOsString(OsString);

    unsafe impl<'a> ocaml::FromValue<'a> for OCamlOsString {
        fn from_value(value: ocaml::Value) -> Self {
            Self(<&str>::from_value(value).to_owned().into())
        }
    }

    unsafe impl ocaml::IntoValue for OCamlOsString {
        fn into_value(self, _rt: &ocaml::Runtime) -> ocaml::Value {
            unsafe { ocaml::Value::string(self.0.to_str().unwrap()) }
        }
    }

    #[ocaml::func]
    pub fn ml_exec_parallel(prog: OCamlOsString, args: LinkedList<OCamlOsString>) {
        let mut stdin = utils::stdin();
        let mut stdout = utils::stdout();

        let args: Vec<_> = args.iter().map(|s| &s.0[..]).collect();
        let params = ExecWorker::new(&prog.0[..], &args[..]);

        let mut parser = make_parser(params, &mut stdout);
        let () = parser.process_streaming(parser::SegmentIndex::EntireFile, &mut stdin).unwrap();
    }
}
