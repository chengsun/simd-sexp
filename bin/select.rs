use simd_sexp::*;

fn main() {
    let mut args = std::env::args();

    args.next();

    let select: Vec<Vec<u8>> = args.map(|s| s.as_bytes().to_owned()).collect();
    let select = select.iter().map(|s| &s[..]);

    let mut stdin = utils::stdin();
    let mut stdout = utils::stdout();

    /*
    let mut parser = parser::State::from_visitor(select::SelectVisitor::new(select, &mut stdout));
    let () = parser.process_streaming(&mut stdin).unwrap();
    */

    let mut parser = parser::State::new(select::SelectStage2::new(select, select::SelectStage2OutputValues::new(&mut stdout)));
    let () = parser.process_streaming(&mut stdin).unwrap();
}
