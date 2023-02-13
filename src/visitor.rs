pub trait SexpFactory {
    type Sexp;
    fn atom(&self, a: Vec<u8>) -> Self::Sexp;
    fn list(&self, xs: Vec<Self::Sexp>) -> Self::Sexp;
}

/// Visitor for traversing a sexp type that is already fully parsed and
/// validated
pub trait ReadVisitor {
    fn reset(&mut self);
    fn atom(&mut self, atom: &[u8]);
    fn list_open(&mut self);
    fn list_close(&mut self);
    fn eof(&mut self);
}

pub trait ReadVisitable {
    fn visit<VisitorT: ReadVisitor>(&self, visitor: &mut VisitorT);
}

/// Visitor for constructing a sexp type by parsing a potentially invalid string
pub trait Visitor {
    type IntermediateAtom;
    type Context;
    type Return;
    fn reset(&mut self, input_size_hint: Option<usize>);
    fn atom_reserve(&mut self, length_upper_bound: usize) -> Self::IntermediateAtom;
    fn atom_borrow<'a, 'b : 'a>(&'b mut self, atom: &'a mut Self::IntermediateAtom) -> &'a mut [u8];
    fn atom(&mut self, atom: Self::IntermediateAtom, length: usize, parent_context: Option<&mut Self::Context>);
    fn list_open(&mut self, parent_context: Option<&mut Self::Context>) -> Self::Context;
    fn list_close(&mut self, context: Self::Context, parent_context: Option<&mut Self::Context>);
    fn eof(&mut self) -> Self::Return;
}

/// Adapter to allow a SexpFactory to become a Visitor
pub struct SimpleVisitor<SexpFactoryT: SexpFactory> {
    sexp_factory: SexpFactoryT,
    sexp_stack: Vec<SexpFactoryT::Sexp>,
}

impl<SexpFactoryT: SexpFactory> SimpleVisitor<SexpFactoryT> {
    pub fn new(sexp_factory: SexpFactoryT) -> Self {
        SimpleVisitor {
            sexp_factory,
            sexp_stack: Vec::new(),
        }
    }
}

impl<SexpFactoryT: SexpFactory> Visitor for SimpleVisitor<SexpFactoryT> {
    type IntermediateAtom = Vec<u8>;
    type Context = usize;
    type Return = Vec<SexpFactoryT::Sexp>;
    fn reset(&mut self, _input_size_hint: Option<usize>) {
    }
    fn atom_reserve(&mut self, length_upper_bound: usize) -> Self::IntermediateAtom {
        (0..length_upper_bound).map(|_| 0u8).collect()
    }
    fn atom_borrow<'a, 'b : 'a>(&'b mut self, atom: &'a mut Self::IntermediateAtom) -> &'a mut [u8] {
        &mut atom[..]
    }
    fn atom(&mut self, mut atom: Self::IntermediateAtom, length: usize, _: Option<&mut Self::Context>) {
        atom.truncate(length);
        self.sexp_stack.push(self.sexp_factory.atom(atom));
    }
    fn list_open(&mut self, _: Option<&mut Self::Context>) -> Self::Context {
        self.sexp_stack.len()
    }
    fn list_close(&mut self, context: Self::Context, _: Option<&mut Self::Context>) {
        let open_index = context;
        let inner = self.sexp_stack.split_off(open_index);
        let sexp = self.sexp_factory.list(inner);
        self.sexp_stack.push(sexp);
    }
    fn eof(&mut self) -> Self::Return {
        std::mem::take(&mut self.sexp_stack)
    }
}
