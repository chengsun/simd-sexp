use crate::{escape, extract, sexp_structure};

pub trait SexpFactory {
    type Sexp;
    fn atom(&self, a: &[u8]) -> Self::Sexp;
    fn list(&self, xs: Vec<Self::Sexp>) -> Self::Sexp;
}

const INDICES_BUFFER_MAX_LEN: usize = 512;

pub struct State<SexpFactoryT: SexpFactory> {
    sexp_factory: SexpFactoryT,
    sexp_structure_classifier: sexp_structure::Avx2,
    unescape: escape::GenericUnescape,
    sexp_stack: Vec<SexpFactoryT::Sexp>,
    depth_stack: Vec<usize>,
    indices_buffer: [usize; INDICES_BUFFER_MAX_LEN],
}

#[derive(Copy, Clone, Debug)]
pub enum Error {
    UnmatchedOpenParen,
    UnmatchedCloseParen,
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

impl<SexpFactoryT: SexpFactory> State<SexpFactoryT> {
    pub fn new(sexp_factory: SexpFactoryT) -> Self {
        let sexp_structure_classifier = sexp_structure::Avx2::new().unwrap();

        let unescape = escape::GenericUnescape::new();

        let sexp_stack: Vec<SexpFactoryT::Sexp> = Vec::new();
        let depth_stack: Vec<usize> = Vec::new();

        State {
            sexp_factory,
            sexp_structure_classifier,
            unescape,
            sexp_stack,
            depth_stack,
            indices_buffer: [0; INDICES_BUFFER_MAX_LEN]
        }
    }

    fn process_eof(&mut self) -> Result<Vec<SexpFactoryT::Sexp>, Error> {
        if self.depth_stack.len() > 0 {
            return Err(Error::UnmatchedOpenParen);
        }
        Ok (std::mem::take(&mut self.sexp_stack))
    }

    fn process_one(&mut self, input: &[u8], indices_index: usize, indices_len: usize) -> Result<usize, Error> {
        let indices_buffer = &self.indices_buffer[indices_index..indices_len];
        match input[indices_buffer[0]] {
            b'(' => {
                self.depth_stack.push(self.sexp_stack.len());
                Ok(1)
            },
            b')' => {
                let open_index = self.depth_stack.pop().ok_or(Error::UnmatchedCloseParen)?;
                let inner = self.sexp_stack.split_off(open_index);
                let sexp = self.sexp_factory.list(inner);
                self.sexp_stack.push(sexp);
                Ok(1)
            },
            b' ' | b'\t' | b'\n' => Ok(1),
            b'"' => {
                use escape::Unescape;
                let mut atom_string: Vec<u8> = (0..input.len()).map(|_| 0).collect();
                let start_index = indices_buffer[0] + 1;
                let end_index =
                    if indices_buffer.len() < 2 {
                        return Error::UnclosedQuote
                    } else {
                        indices_buffer[1]
                    };
                let atom_string_len =
                    self.unescape.unescape(
                        &input[start_index..end_index],
                        &mut atom_string[..])
                    .ok_or(Error::InvalidEscape)?;
                atom_string.truncate(atom_string_len);
                self.sexp_stack.push(self.sexp_factory.atom(&atom_string[..]));
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
                self.sexp_stack.push(self.sexp_factory.atom(&input[start_index..end_index]));
                Ok(1)
            }
        }
    }

    pub fn process_all(&mut self, input: &[u8]) -> Result<Vec<SexpFactoryT::Sexp>, Error> {
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
