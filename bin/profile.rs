use simd_sexp::*;

fn main() {
    let input_pp = std::fs::read_to_string("/home/user/simd-sexp/test_data.pp.sexp").unwrap();
    let input_pp = input_pp.as_bytes();

    for _i in 0..10000 {
        let mut parser = parser::State::new(rust_parser::TapeVisitor::new());
        let result = parser.process_all(input_pp);
        criterion::black_box(result.unwrap());
    }

    let event_frame = ittapi::Event::new("frame");

    for _i in 0..10000 {
        let e = event_frame.start();
        let mut parser = parser::State::new(rust_parser::TapeVisitor::new());
        let result = parser.process_all(input_pp);
        criterion::black_box(result.unwrap());
        std::mem::drop(e);
    }

    {
        let mut sexp_parser = parser::State::new(parser::SimpleVisitor::new(rust_parser::RustSexpFactory::new()));
        let sexp_result = sexp_parser.process_all(input_pp).unwrap();
        fn count_atoms(sexp: &rust_parser::RustSexp) -> usize {
            match sexp {
                rust_parser::RustSexp::Atom(_) => 1,
                rust_parser::RustSexp::List(l) => l.iter().map(count_atoms).sum::<usize>(),
            }
        }
        fn count_lists(sexp: &rust_parser::RustSexp) -> usize {
            match sexp {
                rust_parser::RustSexp::Atom(_) => 0,
                rust_parser::RustSexp::List(l) => 1usize + l.iter().map(count_lists).sum::<usize>(),
            }
        }
        fn count_atom_size(sexp: &rust_parser::RustSexp) -> usize {
            match sexp {
                rust_parser::RustSexp::Atom(a) => a.len(),
                rust_parser::RustSexp::List(l) => l.iter().map(count_atom_size).sum(),
            }
        }
        let mut tape_parser = parser::State::new(rust_parser::TapeVisitor::new());
        let tape_result = tape_parser.process_all(input_pp).unwrap();
        println!("Input size:     {}", input_pp.len());
        println!("Of which atoms: {}", sexp_result.iter().map(count_atom_size).sum::<usize>());
        println!("Tape size:      {}", tape_result.len());
        println!("# atoms:        {}", sexp_result.iter().map(count_atoms).sum::<usize>());
        println!("# lists:        {}", sexp_result.iter().map(count_lists).sum::<usize>());
    }
}
