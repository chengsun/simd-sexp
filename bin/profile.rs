use simd_sexp::*;

fn main() {
    let input_pp = std::fs::read_to_string("/home/user/simd-sexp/test_data.mach.sexp").unwrap();
    let input_pp = input_pp.as_bytes();

    {
        let mut input_pp_v = input_pp.to_vec();
        let mut sexp_parser = parser::State::new(parser::SimpleVisitor::new(rust_parser::SexpFactory::new()));
        let sexp_result = sexp_parser.process_all(&mut input_pp_v[..]).unwrap();
        fn count_atoms(sexp: &rust_parser::Sexp) -> usize {
            match sexp {
                rust_parser::Sexp::Atom(_) => 1,
                rust_parser::Sexp::List(l) => l.iter().map(count_atoms).sum::<usize>(),
            }
        }
        fn count_lists(sexp: &rust_parser::Sexp) -> usize {
            match sexp {
                rust_parser::Sexp::Atom(_) => 0,
                rust_parser::Sexp::List(l) => 1usize + l.iter().map(count_lists).sum::<usize>(),
            }
        }
        fn count_atom_size(sexp: &rust_parser::Sexp) -> usize {
            match sexp {
                rust_parser::Sexp::Atom(a) => a.len(),
                rust_parser::Sexp::List(l) => l.iter().map(count_atom_size).sum(),
            }
        }
        input_pp_v.copy_from_slice(input_pp);
        let mut tape_parser = parser::State::new(rust_parser::TapeVisitor::new());
        let tape_result = tape_parser.process_all(&mut input_pp_v[..]).unwrap();
        input_pp_v.copy_from_slice(input_pp);
        let mut phase1_parser = parser::State::new(rust_parser::two_phase::Phase1Visitor::new());
        let phase1_result = phase1_parser.process_all(&mut input_pp_v[..]).unwrap();
        input_pp_v.copy_from_slice(input_pp);
        let mut phase2_parser = parser::State::new(rust_parser::two_phase::Phase2Visitor::new(phase1_result));
        let phase2_result = phase2_parser.process_all(&mut input_pp_v[..]).unwrap();
        println!("Input size:     {}", input_pp.len());
        println!("Of which atoms: {}", sexp_result.iter().map(count_atom_size).sum::<usize>());
        println!("Tape size:      {}", tape_result.0.len());
        println!("Vartape size:   {}", phase2_result.0.len());
        println!("# atoms:        {}", sexp_result.iter().map(count_atoms).sum::<usize>());
        println!("# lists:        {}", sexp_result.iter().map(count_lists).sum::<usize>());
    }

    match std::env::args().nth(1).as_deref() {
        Some("tape") => {
            let event_frame = ittapi::Event::new("frame");

            println!("Warmup");

            for _i in 0..10000 {
                let mut input_pp_v = input_pp.to_vec();
                let mut parser = parser::State::new(rust_parser::TapeVisitor::new());
                let result = parser.process_all(&mut input_pp_v[..]);
                criterion::black_box(result.unwrap());
            }

            println!("Profiling");

            for _i in 0..10000 {
                let e = event_frame.start();
                let mut input_pp_v = input_pp.to_vec();
                let mut parser = parser::State::new(rust_parser::TapeVisitor::new());
                let result = parser.process_all(&mut input_pp_v[..]);
                criterion::black_box(result.unwrap());
                std::mem::drop(e);
            }
        },

        Some("two_phase") => {
            let event_phase1 = ittapi::Event::new("phase1");
            let event_phase2 = ittapi::Event::new("phase2");

            println!("Warmup");

            for _i in 0..10000 {
                let mut input_pp_v = input_pp.to_vec();
                let mut parser = parser::State::new(rust_parser::two_phase::Phase1Visitor::new());
                let result = parser.process_all(&mut input_pp_v[..]).unwrap();
                input_pp_v.copy_from_slice(input_pp);
                let mut parser = parser::State::new(rust_parser::two_phase::Phase2Visitor::new(result));
                let result = parser.process_all(&mut input_pp_v[..]).unwrap();
                criterion::black_box(result);
            }

            println!("Profiling");

            for _i in 0..10000 {
                let (mut input_pp_v, result) = {
                    let e = event_phase1.start();
                    let mut input_pp_v = input_pp.to_vec();
                    let mut parser = parser::State::new(rust_parser::two_phase::Phase1Visitor::new());
                    let result = parser.process_all(&mut input_pp_v[..]).unwrap();
                    std::mem::drop(e);
                    (input_pp_v, result)
                };
                let result = {
                    let e = event_phase2.start();
                    input_pp_v.copy_from_slice(input_pp);
                    let mut parser = parser::State::new(rust_parser::two_phase::Phase2Visitor::new(result));
                    let result = parser.process_all(&mut input_pp_v[..]).unwrap();
                    std::mem::drop(e);
                    result
                };
                criterion::black_box(result);
            }
        },

        arg => {
            println!("Not profiling. In order to profile, pass \"tape\" or \"two_phase\" as an argument.");
            println!("You passed: {:?}", arg);
            unsafe {
                println!("something: {:?}", start_stop_transitions::stg_asdf(
                    utils::bitrev64(0b0010100),
                    utils::bitrev64(0b0100000),
                    utils::bitrev64(0b0001000)));
            }
        },
    }

}
