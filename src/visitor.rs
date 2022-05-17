pub trait SexpFactory {
    type Sexp;
    fn atom(&self, a: Vec<u8>) -> Self::Sexp;
    fn list(&self, xs: Vec<Self::Sexp>) -> Self::Sexp;
}

pub trait Visitor {
    type IntermediateAtom;
    type Context;
    type FinalReturnType;
    fn bof(&mut self, input_size_hint: Option<usize>);
    fn atom_reserve(&mut self, length_upper_bound: usize) -> Self::IntermediateAtom;
    fn atom_borrow<'a, 'b : 'a>(&'b mut self, atom: &'a mut Self::IntermediateAtom) -> &'a mut [u8];
    fn atom(&mut self, atom: Self::IntermediateAtom, length: usize, parent_context: Option<&mut Self::Context>);
    fn list_open(&mut self, parent_context: Option<&mut Self::Context>) -> Self::Context;
    fn list_close(&mut self, context: Self::Context, parent_context: Option<&mut Self::Context>);
    fn eof(&mut self) -> Self::FinalReturnType;
}
