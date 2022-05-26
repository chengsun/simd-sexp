use simd_sexp::*;
use std::ffi::{OsStr, OsString};

fn main() {
    let mut args = std::env::args();

    args.next();

    let prog: OsString = args.next().expect("required PROG argument").to_owned().into();
    let args: Vec<OsString> = args.map(|s| s.to_owned().into()).collect();

    let args: Vec<&OsStr> = args.iter().map(|s| &s[..]).collect();
    let exec_worker = exec_parallel::ExecWorker::new(&prog, &args);

    let mut stdin = utils::stdin();
    let mut stdout = utils::stdout();

    let mut parser = exec_parallel::make_parser(exec_worker, &mut stdout);
    let () = parser.process_streaming(parser::SegmentIndex::EntireFile, &mut stdin).unwrap();
}
