use simd_sexp::*;

fn main() {
    let mut args = std::env::args();

    args.next();

    let select: Vec<Vec<u8>> = args.map(|s| s.as_bytes().to_owned()).collect();
    let select = select.iter().map(|s| &s[..]);

    use std::os::unix::io::FromRawFd;
    let stdin = unsafe { std::fs::File::from_raw_fd(0) };
    let stdout = unsafe { std::fs::File::from_raw_fd(1) };
    let mut stdin = std::io::BufReader::with_capacity(1048576, stdin);
    let mut stdout = std::io::BufWriter::with_capacity(1048576, stdout);

    /*
    let mut parser = parser::State::from_visitor(select::SelectVisitor::new(select, &mut stdout));
    let () = parser.process_streaming(&mut stdin).unwrap();
    */

    let mut parser = parser::State::new(select::SelectStage2::new(select, &mut stdout, false));
    let () = parser.process_streaming(&mut stdin).unwrap();
}
