use simd_sexp::*;

fn main() {
    let mut args = std::env::args();

    args.next();

    let select: Vec<Vec<u8>> = args.map(|s| s.as_bytes().to_owned()).collect();
    let select = select.iter().map(|s| &s[..]);

    let mut stdin = utils::stdin();
    let mut stdout = utils::stdout();

    /*
    let mut parser = parser::State::from_writing_stage2(select::Stage2::new(select, select::OutputCsv::new(false)), &mut stdout);
    let () = parser.process_streaming(&mut stdin).unwrap();
    */

    let mut parser = select::make_parser(select, &mut stdout, select::OutputKind::Csv { atoms_as_sexps: false }, true);
    let () = parser.process_streaming(&mut stdin).unwrap();
}
