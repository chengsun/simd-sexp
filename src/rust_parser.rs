use crate::{parser, varint};

pub enum RustSexp {
    Atom(Vec<u8>),
    List(Vec<RustSexp>),
}

pub struct RustSexpFactory();

impl RustSexpFactory {
    pub fn new() -> Self {
        RustSexpFactory()
    }
}

impl parser::SexpFactory for RustSexpFactory {
    type Sexp = RustSexp;

    fn atom(&self, a: &[u8]) -> Self::Sexp {
        RustSexp::Atom(a.to_vec())
    }

    fn list(&self, xs: Vec<Self::Sexp>) -> Self::Sexp {
        RustSexp::List(xs)
    }
}

/**
Atom: <len*2 as u64>string
List: <len*2+1 as u64>Repr(X1)Repr(X2)...
*/

pub struct TapeVisitor {
    tape: Vec<u8>,
    varint_encoder: varint::GenericEncoder,
}

impl TapeVisitor {
    pub fn new() -> TapeVisitor {
        Self {
            tape: Vec::new(),
            varint_encoder: varint::GenericEncoder::new(),
        }
    }
}

pub struct TapeVisitorContext {
    tape_start_index: usize,
    child_count: usize,
}

impl parser::Visitor for TapeVisitor {
    type Context = TapeVisitorContext;
    type IntermediateReturnType = ();
    type FinalReturnType = Vec<u8>;
    fn atom(&mut self, atom: &[u8], parent_context: Option<&mut TapeVisitorContext>) {
        // TODO: lol
        let tape_start_index = self.tape.len();
        self.tape.extend([0u8; 8]);
        self.varint_encoder.encode_one(atom.len() * 2, &mut self.tape[tape_start_index..]).unwrap();
        self.tape.extend_from_slice(atom);
        match parent_context {
            Some(parent_context) => { parent_context.child_count += 1; },
            None => (),
        }
    }
    fn list_open(&mut self, parent_context: Option<&mut TapeVisitorContext>) -> TapeVisitorContext {
        match parent_context {
            Some(parent_context) => { parent_context.child_count += 1; },
            None => (),
        }
        let tape_start_index = self.tape.len();
        self.tape.extend([0u8; 8]);
        TapeVisitorContext {
            tape_start_index,
            child_count: 0,
        }
    }
    fn list_close(&mut self, context: TapeVisitorContext) {
        self.varint_encoder.encode_one((self.tape.len() - context.tape_start_index) * 2 + 1, &mut self.tape[context.tape_start_index..]);
    }
    fn eof(&mut self) -> Self::FinalReturnType {
        std::mem::take(&mut self.tape)
    }
}
