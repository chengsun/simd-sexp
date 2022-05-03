use crate::{clmul, escape, extract, sexp_structure, vector_classifier, xor_masked_adjacent};

pub trait SexpFactory {
    type Sexp;
    fn atom(&self, a: &[u8]) -> Self::Sexp;
    fn list(&self, xs: &[Self::Sexp]) -> Self::Sexp;
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
            Error::InvalidEscape => { write!(f, "Invalid escape") }
        }
    }
}

impl<SexpFactoryT: SexpFactory> State<SexpFactoryT> {
    pub fn new(sexp_factory: SexpFactoryT) -> Self {
        // TODO
        let clmul = clmul::Sse2Pclmulqdq::new().unwrap();
        let vector_classifier_builder = vector_classifier::Avx2Builder::new().unwrap();
        let xor_masked_adjacent = xor_masked_adjacent::Bmi2::new().unwrap();
        let sexp_structure_classifier = sexp_structure::Avx2::new(clmul, vector_classifier_builder, xor_masked_adjacent);

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
                let sexp = self.sexp_factory.list(&self.sexp_stack[open_index..]);
                self.sexp_stack.truncate(open_index);
                self.sexp_stack.push(sexp);
                Ok(1)
            },
            b' ' | b'\t' | b'\n' => Ok(1),
            b'"' => {
                use escape::Unescape;
                let mut atom_string: Vec<u8> = (0..input.len()).map(|_| 0).collect();
                let atom_string_len =
                    self.unescape.unescape(
                        &input[(indices_buffer[0] + 1)..indices_buffer[1]],
                        &mut atom_string[..])
                    .ok_or(Error::InvalidEscape)?;
                atom_string.truncate(atom_string_len);
                self.sexp_stack.push(self.sexp_factory.atom(&atom_string[..]));
                Ok(2)
            },
            _ => {
                self.sexp_stack.push(self.sexp_factory.atom(&input[indices_buffer[0]..indices_buffer[1]]));
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

                while input_index + 64 <= input.len() && indices_len + 64 <= INDICES_BUFFER_MAX_LEN {
                    let bitmask = self.sexp_structure_classifier.structural_indices_bitmask(&input[input_index..]);

                    extract::safe_generic(|bit_offset| {
                        self.indices_buffer[indices_len] = input_index + bit_offset;
                        indices_len += 1;
                    }, bitmask);

                    input_index += 64;
                }
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
