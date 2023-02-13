use std::ffi::{OsStr, OsString};

use simd_sexp::*;

struct LoopReader<'a> {
    source: &'a [u8],
    current: &'a [u8],
    loops_remaining: usize,
}

impl<'a> LoopReader<'a> {
    fn new(source: &'a [u8], loops: usize) -> Self {
        Self { source, current: &[][..], loops_remaining: loops }
    }
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.current.is_empty() && self.loops_remaining > 0 {
            self.loops_remaining -= 1;
            self.current = self.source;
        }
        Ok(self.current)
    }
    fn consume(&mut self, amt: usize) {
        self.current = &self.current[amt..];
    }
}

impl<'a> std::io::Read for LoopReader<'a> {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let my_buf = self.fill_buf()?;
        let amt = std::cmp::min(buf.len(), my_buf.len());
        buf[..amt].copy_from_slice(my_buf);
        std::mem::drop(my_buf);
        self.consume(amt);
        Ok(amt)
    }
}

impl<'a> std::io::BufRead for LoopReader<'a> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        self.fill_buf()
    }
    fn consume(&mut self, amt: usize) {
        self.consume(amt)
    }
}

fn main() {
    let input_pp = std::fs::read_to_string("/home/user/simd-sexp/test_data.mach.sexp").unwrap();
    let input_pp = input_pp.as_bytes();

    {
        let mut sexp_parser = parser::parser_from_sexp_factory(rust_parser::SexpFactory::new());
        let sexp_result = sexp_parser.process(&input_pp[..]).unwrap();
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
        let mut tape_parser = parser::parser_from_visitor(rust_parser::TapeVisitor::new());
        let tape_result = tape_parser.process(&input_pp[..]).unwrap();
        println!("Input size:     {}", input_pp.len());
        println!("Of which atoms: {}", sexp_result.iter().map(count_atom_size).sum::<usize>());
        println!("Tape size:      {}", std::mem::size_of_val(&*tape_result.tape) + std::mem::size_of_val(&*tape_result.atoms));
        println!("# atoms:        {}", sexp_result.iter().map(count_atoms).sum::<usize>());
        println!("# lists:        {}", sexp_result.iter().map(count_lists).sum::<usize>());
    }

    match std::env::args().nth(1).as_deref() {
        Some("tape") => {
            let event_frame = ittapi::Event::new("frame");

            println!("Warmup");

            for _i in 0..10000 {
                let mut parser = parser::parser_from_visitor(rust_parser::TapeVisitor::new());
                let result = parser.process(&input_pp[..]);
                criterion::black_box(result.unwrap());
            }

            println!("Profiling");

            for _i in 0..10000 {
                let e = event_frame.start();
                let mut parser = parser::parser_from_visitor(rust_parser::TapeVisitor::new());
                let result = parser.process(&input_pp[..]);
                criterion::black_box(result.unwrap());
                std::mem::drop(e);
            }
        },

        Some("select") => {
            let keys = [&b"name"[..], &b"libraries"[..]];

            let event_frame = ittapi::Event::new("frame");

            println!("Warmup");

            {
                let mut read = LoopReader::new(&input_pp[..], 40000);
                let mut result = Vec::new();
                let mut parser = select::make_parser(keys, &mut result, select::OutputKind::Csv { atoms_as_sexps: false }, true);
                let () = parser.process_streaming(&mut read).unwrap();
                std::mem::drop(parser);
                criterion::black_box(result);
            }

            println!("Profiling");

            {
                let mut read = LoopReader::new(&input_pp[..], 40000);
                let e = event_frame.start();
                let mut result = Vec::new();
                let mut parser = select::make_parser(keys, &mut result, select::OutputKind::Csv { atoms_as_sexps: false }, true);
                let () = parser.process_streaming(&mut read).unwrap();
                std::mem::drop(parser);
                criterion::black_box(result);
                std::mem::drop(e);
            }
        },

        Some("exec") => {
            let event_frame = ittapi::Event::new("frame");

            let prog = OsString::from("sexp".to_owned());
            let args = vec![OsString::from("select".to_owned()), OsString::from("libraries".to_owned())];
            let args: Vec<&OsStr> = args.iter().map(|s| &s[..]).collect();
            let exec_worker = exec_parallel::ExecWorker::new(&prog, &args);

            println!("Warmup");

            {
                let mut read = LoopReader::new(&input_pp[..], 4000);
                let mut result = Vec::new();
                let mut parser = exec_parallel::make_parser(exec_worker.clone(), &mut result);
                let () = parser.process_streaming(&mut read).unwrap();
                std::mem::drop(parser);
                criterion::black_box(result);
            }

            println!("Profiling");

            {
                let mut read = LoopReader::new(&input_pp[..], 4000);
                let e = event_frame.start();
                let mut result = Vec::new();
                let mut parser = exec_parallel::make_parser(exec_worker.clone(), &mut result);
                let () = parser.process_streaming(&mut read).unwrap();
                std::mem::drop(parser);
                criterion::black_box(result);
                std::mem::drop(e);
            }
        },


        arg => {
            println!("Not profiling. In order to profile, pass \"tape\" or \"select\" or \"exec\" as an argument.");
            println!("You passed: {:?}", arg);
        },
    }

}
