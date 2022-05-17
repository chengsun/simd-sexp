use simd_sexp::*;
use std::io::{stdin, stdout};

fn main() {
    let mut args = std::env::args();

    args.next();

    let select: Vec<Vec<u8>> = args.map(|s| s.as_bytes().to_owned()).collect();
    let select = select.iter().map(|s| &s[..]);

    let stdin = stdin();
    let mut stdin = stdin.lock();
    let stdout = stdout();
    let stdout = stdout.lock();

    /*
    let mut parser = parser::State::from_visitor(SelectVisitor::new(select, stdout));
    let () = parser.process_streaming(&mut stdin).unwrap();
     */

    let mut parser = parser::State::new(select::SelectStage2::new(select, stdout, false));
    let () = parser.process_streaming(&mut stdin).unwrap();
}
