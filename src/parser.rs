use crate::{escape, extract, structural};

pub trait Visitor {
    type IntermediateAtom;
    type Context;
    type FinalReturnType;
    fn atom_reserve(&mut self, length_upper_bound: usize) -> Self::IntermediateAtom;
    fn atom_borrow<'a, 'b : 'a>(&'b mut self, atom: &'a mut Self::IntermediateAtom) -> &'a mut [u8];
    fn atom(&mut self, atom: Self::IntermediateAtom, length: usize, parent_context: Option<&mut Self::Context>);
    fn list_open(&mut self, parent_context: Option<&mut Self::Context>) -> Self::Context;
    fn list_close(&mut self, context: Self::Context, parent_context: Option<&mut Self::Context>);
    fn eof(&mut self) -> Self::FinalReturnType;
}

pub trait SexpFactory {
    type Sexp;
    fn atom(&self, a: Vec<u8>) -> Self::Sexp;
    fn list(&self, xs: Vec<Self::Sexp>) -> Self::Sexp;
}

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
    type FinalReturnType = Vec<SexpFactoryT::Sexp>;
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
    fn eof(&mut self) -> Self::FinalReturnType {
        std::mem::take(&mut self.sexp_stack)
    }
}

/// Must be >= 64. Doesn't affect correctness or impose limitations on sexp being parsed.
const INDICES_BUFFER_MAX_LEN: usize = 512;

pub struct State<VisitorT: Visitor> {
    visitor: VisitorT,
    structural_classifier: structural::Avx2,
    unescape: escape::GenericUnescape,
    context_stack: Vec<VisitorT::Context>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Error {
    UnmatchedOpenParen,
    UnmatchedCloseParen,
    UnclosedQuote,
    InvalidEscape,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::UnmatchedOpenParen => { write!(f, "Unmatched open paren") }
            Error::UnmatchedCloseParen => { write!(f, "Unmatched close paren") }
            Error::UnclosedQuote => { write!(f, "Unclosed quote") }
            Error::InvalidEscape => { write!(f, "Invalid escape") }
        }
    }
}

impl<VisitorT: Visitor> State<VisitorT> {
    pub fn new(visitor: VisitorT) -> Self {
        let structural_classifier = structural::Avx2::new().unwrap();

        let unescape = escape::GenericUnescape::new();

        State {
            visitor,
            structural_classifier,
            unescape,
            context_stack: Vec::new(),
        }
    }

    fn process_eof(&mut self) -> Result<VisitorT::FinalReturnType, Error> {
        if self.context_stack.len() > 0 {
            return Err(Error::UnmatchedOpenParen);
        }
        Ok (self.visitor.eof())
    }

    #[inline(always)]
    fn process_one(&mut self, input: &[u8], this_index: usize, next_index: usize) -> Result<(), Error> {
        match input[this_index] {
            b'(' => {
                let new_context = self.visitor.list_open(self.context_stack.last_mut());
                self.context_stack.push(new_context);
            },
            b')' => {
                let context = self.context_stack.pop().ok_or(Error::UnmatchedCloseParen)?;
                self.visitor.list_close(context, self.context_stack.last_mut());
            },
            b'"' => {
                use escape::Unescape;
                let start_index = this_index + 1;
                let end_index =
                    // NOTE: can't subtract one here because of the case where
                    // there is an EOF before closing quote
                    next_index;
                let length_upper_bound = end_index - start_index;
                let mut atom = self.visitor.atom_reserve(length_upper_bound);
                let (_input_consumed, atom_string_len) =
                    self.unescape.unescape(&input[start_index..], self.visitor.atom_borrow(&mut atom))
                    .ok_or(Error::InvalidEscape)?;
                self.visitor.atom(atom, atom_string_len, self.context_stack.last_mut());
            },
            ch => {
                if ch != b' ' && ch != b'\t' && ch != b'\n' {
                    let length = next_index - this_index;
                    let mut atom = self.visitor.atom_reserve(length);
                    {
                        let output = self.visitor.atom_borrow(&mut atom);
                        unsafe { std::ptr::copy_nonoverlapping(&input[this_index] as *const u8, &mut output[0] as *mut u8, length) };
                    }
                    self.visitor.atom(atom, length, self.context_stack.last_mut());
                }
            }
        }
        Ok(())
    }

    pub fn process_all(&mut self, input: &[u8]) -> Result<VisitorT::FinalReturnType, Error> {
        use structural::Classifier;

        let mut input_index = 0;
        let mut indices_len = 0;
        let mut indices_buffer = [0; INDICES_BUFFER_MAX_LEN];

        loop {
            self.structural_classifier.structural_indices_bitmask(
                &input[input_index..],
                |bitmask, bitmask_len| {
                    extract::safe_generic(|bit_offset| {
                        indices_buffer[indices_len] = input_index + bit_offset;
                        indices_len += 1;
                    }, bitmask);

                    input_index += bitmask_len;
                    if indices_len + 64 <= INDICES_BUFFER_MAX_LEN {
                        structural::CallbackResult::Continue
                    } else {
                        structural::CallbackResult::Finish
                    }
                });

            let input_fully_consumed = input_index >= input.len();

            for indices_index in 0..(indices_len - 1) {
                self.process_one(input, indices_buffer[indices_index], indices_buffer[indices_index + 1])?;
            }
            if input_fully_consumed {
                self.process_one(input, indices_buffer[indices_len - 1], input.len())?;
                return self.process_eof();
            }

            indices_buffer[0] = indices_buffer[indices_len - 1];
            indices_len = 1;
        }
    }
}
