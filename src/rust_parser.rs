use crate::parser;

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
