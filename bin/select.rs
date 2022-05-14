use simd_sexp::*;
use std::collections::HashSet;

pub struct SelectVisitor {
    select: HashSet<Vec<u8>>,
}

pub enum SelectVisitorContext {
    Start,
    SelectNext(Vec<u8>),
    Selected(Vec<u8>, Vec<u8>),
    Ignore,
}

impl parser::Visitor for SelectVisitor {
    type Context = SelectVisitorContext;
    type FinalReturnType = ();
    fn atom(&mut self, atom: &[u8], parent_context: Option<&mut Self::Context>) {
        match parent_context {
            None => (),
            Some(parent_context) => {
                *parent_context = match parent_context {
                    SelectVisitorContext::Start =>
                        if self.select.contains(atom) { SelectVisitorContext::SelectNext(atom.to_owned()) } else { SelectVisitorContext::Ignore },
                    SelectVisitorContext::SelectNext(key) => {
                        let key = std::mem::take(key);
                        SelectVisitorContext::Selected(key, atom.to_owned())
                    },
                    SelectVisitorContext::Selected(_, _) => SelectVisitorContext::Ignore,
                    SelectVisitorContext::Ignore => SelectVisitorContext::Ignore,
                };
            },
        };
    }

    fn list_open(&mut self, parent_context: Option<&mut Self::Context>) -> Self::Context {
        match parent_context {
            None => (),
            Some(parent_context) => {
                *parent_context = match *parent_context {
                    SelectVisitorContext::Start => SelectVisitorContext::Ignore,
                    SelectVisitorContext::SelectNext(_) => {
                        // TODO
                        SelectVisitorContext::Ignore
                    },
                    SelectVisitorContext::Selected(_, _) => SelectVisitorContext::Ignore,
                    SelectVisitorContext::Ignore => SelectVisitorContext::Ignore,
                };
            },
        };
        SelectVisitorContext::Start
    }

    fn list_close(&mut self, context: Self::Context, _parent_context: Option<&mut Self::Context>) {
        match context {
            SelectVisitorContext::Start |
            SelectVisitorContext::SelectNext(_) |
            SelectVisitorContext::Ignore => (),
            SelectVisitorContext::Selected(_, value) => {
                println!("{}", String::from_utf8(value).unwrap());
            },
        };
    }

    fn eof(&mut self) {
    }
}

fn main() {
    let mut args = std::env::args();
    let input_pp = std::fs::read_to_string(args.nth(1).expect("expected filename argument")).expect("couldn't read from filename");

    let select: HashSet<Vec<u8>> = args.map(|s| s.as_bytes().to_owned()).collect();

    let mut input_pp_v = input_pp.as_bytes().to_vec();
    let mut parser = parser::State::new(SelectVisitor { select });
    let () = parser.process_all(&mut input_pp_v[..]).unwrap();
}
