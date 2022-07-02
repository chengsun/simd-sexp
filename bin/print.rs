use simd_sexp::*;

fn main() {
    let mut stdin = utils::stdin();
    let mut stdout = utils::stdout();

    let mut print = print::make(&mut stdout, true);
    let () = print.process_streaming(parser::SegmentIndex::EntireFile, &mut stdin).unwrap();
}
