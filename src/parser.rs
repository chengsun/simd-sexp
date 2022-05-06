use crate::{escape, extract, sexp_structure};


pub trait Visitor {
    type Context;
    type IntermediateReturnType;
    type FinalReturnType;
    fn atom(&mut self, atom: &[u8], parent_context: Option<&mut Self::Context>) -> Self::IntermediateReturnType;
    fn list_open(&mut self, parent_context: Option<&mut Self::Context>) -> Self::Context;
    fn list_close(&mut self, context: Self::Context) -> Self::IntermediateReturnType;
    fn eof(&mut self) -> Self::FinalReturnType;
}

pub trait SexpFactory {
    type Sexp;
    fn atom(&self, a: &[u8]) -> Self::Sexp;
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
    type Context = usize;
    type IntermediateReturnType = ();
    type FinalReturnType = Vec<SexpFactoryT::Sexp>;
    fn atom(&mut self, atom: &[u8], _: Option<&mut Self::Context>) -> Self::IntermediateReturnType {
        self.sexp_stack.push(self.sexp_factory.atom(atom));
    }
    fn list_open(&mut self, _: Option<&mut Self::Context>) -> Self::Context {
        self.sexp_stack.len()
    }
    fn list_close(&mut self, context: Self::Context) -> Self::IntermediateReturnType {
        let open_index = context;
        let inner = self.sexp_stack.split_off(open_index);
        let sexp = self.sexp_factory.list(inner);
        self.sexp_stack.push(sexp);
    }
    fn eof(&mut self) -> Self::FinalReturnType {
        std::mem::take(&mut self.sexp_stack)
    }
}


const INDICES_BUFFER_MAX_LEN: usize = 512;

pub struct State<VisitorT: Visitor> {
    visitor: VisitorT,
    sexp_structure_classifier: sexp_structure::Avx2,
    unescape: escape::GenericUnescape,
    context_stack: Vec<VisitorT::Context>,
    indices_buffer: [usize; INDICES_BUFFER_MAX_LEN],
}

#[derive(Copy, Clone, Debug)]
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
        let sexp_structure_classifier = sexp_structure::Avx2::new().unwrap();

        let unescape = escape::GenericUnescape::new();

        State {
            visitor,
            sexp_structure_classifier,
            unescape,
            context_stack: Vec::new(),
            indices_buffer: [0; INDICES_BUFFER_MAX_LEN]
        }
    }

    fn process_eof(&mut self) -> Result<VisitorT::FinalReturnType, Error> {
        if self.context_stack.len() > 0 {
            return Err(Error::UnmatchedOpenParen);
        }
        Ok (self.visitor.eof())
    }

    fn process_one(&mut self, input: &[u8], indices_index: usize, indices_len: usize) -> Result<usize, Error> {
        let indices_buffer = &self.indices_buffer[indices_index..indices_len];
        match input[indices_buffer[0]] {
            b'(' => {
                let new_context = self.visitor.list_open(self.context_stack.last_mut());
                self.context_stack.push(new_context);
                Ok(1)
            },
            b')' => {
                let context = self.context_stack.pop().ok_or(Error::UnmatchedCloseParen)?;
                self.visitor.list_close(context);
                Ok(1)
            },
            b' ' | b'\t' | b'\n' => Ok(1),
            b'"' => {
                use escape::Unescape;
                let mut atom_string: Vec<u8> = (0..input.len()).map(|_| 0).collect();
                let start_index = indices_buffer[0] + 1;
                let end_index =
                    if indices_buffer.len() < 2 {
                        return Err(Error::UnclosedQuote);
                    } else {
                        indices_buffer[1]
                    };
                let atom_string_len =
                    self.unescape.unescape(
                        &input[start_index..end_index],
                        &mut atom_string[..])
                    .ok_or(Error::InvalidEscape)?;
                atom_string.truncate(atom_string_len);
                self.visitor.atom(&atom_string[..], self.context_stack.last_mut());
                Ok(2)
            },
            _ => {
                let start_index = indices_buffer[0];
                let end_index =
                    if indices_buffer.len() < 2 {
                        input.len()
                    } else {
                        indices_buffer[1]
                    };
                self.visitor.atom(&input[start_index..end_index], self.context_stack.last_mut());
                Ok(1)
            }
        }
    }

    pub fn process_all(&mut self, input: &[u8]) -> Result<VisitorT::FinalReturnType, Error> {
        use sexp_structure::Classifier;

        let mut input_index = 0;
        let mut indices_index = 0;
        let mut indices_len = 0;

        loop {
            if indices_index + 2 > indices_len && input_index < input.len() {
                let n_unconsumed_indices = indices_len - indices_index;
                for i in 0..n_unconsumed_indices {
                    self.indices_buffer[i] = self.indices_buffer[indices_index + i];
                }

                indices_index = 0;
                indices_len = n_unconsumed_indices;

                self.sexp_structure_classifier.structural_indices_bitmask(
                    &input[input_index..],
                    |bitmask, bitmask_len| {
                        extract::safe_generic(|bit_offset| {
                            self.indices_buffer[indices_len] = input_index + bit_offset;
                            indices_len += 1;
                        }, bitmask);

                        input_index += bitmask_len;
                        if indices_len + 64 <= INDICES_BUFFER_MAX_LEN {
                            sexp_structure::CallbackResult::Continue
                        } else {
                            sexp_structure::CallbackResult::Finish
                        }
                    });
            }

            if indices_index >= indices_len {
                assert!(input_index == input.len());
                return self.process_eof();
            } else {
                let indices_consumed = self.process_one(&input, indices_index, indices_len)?;
                indices_index = indices_index + indices_consumed;
            }
        }
    }
}
