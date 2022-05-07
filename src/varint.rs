trait Decoder<T> {
    /// returns number of inputs consumed and number of outputs produced
    fn decode(&self, input: &[u8], output: &mut [T]) -> (usize, usize);
}

pub struct GenericDecoder {}

impl GenericDecoder {
    pub fn new() -> Self {
        Self {}
    }

    pub fn decode_one(&self, input: &[u8], output: &mut usize) -> Option<usize> {
        let mut i = 0;
        let mut result = 0usize;
        let mut shift = 0;
        loop {
            if i >= input.len() {
                return None;
            }
            let this_byte = input[i];
            i += 1;
            result |= ((this_byte & (!(1u8 << 7))) as usize) << shift;
            shift += 7;
            if (this_byte >> 7) == 0 {
                break;
            }
        }
        *output = result;
        Some(i)
    }
}

impl Decoder<usize> for GenericDecoder {
    fn decode(&self, input: &[u8], output: &mut [usize]) -> (usize, usize) {
        let mut i = 0;
        let mut o = 0;
        while o < output.len() {
            match self.decode_one(&input[i..], &mut output[o]) {
                Some(input_consumed) => {
                    i += input_consumed;
                    o += 1;
                },
                None => { break; },
            }
        }
        (i, o)
    }
}

trait Encoder<T> {
    /// returns number of inputs consumed and number of outputs produced
    fn encode(&self, input: &[T], output: &mut [u8]) -> (usize, usize);
}

pub struct GenericEncoder {}

impl GenericEncoder {
    pub fn new() -> Self {
        Self {}
    }

    pub fn encode_one_generic<O: ?Sized, F>(&self, mut input: usize, output: &mut O, mut get_output: F) -> Option<()>
    where for<'b> F : FnMut(&'b mut O) -> Option<&'b mut u8>
    {
        loop {
            let o = get_output(output)?;
            let mut this_byte = input & ((1 << 7) - 1);
            input >>= 7;
            if input != 0 {
                this_byte |= 1 << 7;
            }
            *o = this_byte as u8;
            if input == 0 {
                break;
            }
        }
        Some(())
    }

    pub fn encode_one(&self, input: usize, output: &mut [u8]) -> Option<usize> {
        let mut o = 0;
        match self.encode_one_generic(input, output, |output| {
            if o < output.len() {
                let output_ref = &mut output[o];
                o += 1;
                Some(output_ref)
            } else {
                None
            }
        }) {
            None => None,
            Some(()) => Some(o),
        }
    }

    pub fn encode_one_vec(&self, input: usize, output: &mut Vec<u8>) {
        let mut o = 0;
        self.encode_one_generic(input, output, |output| {
            output.push(0u8);
            o += 1;
            output.last_mut()
        });
    }

    pub fn encode_length(&self, input: usize) -> usize {
        let mut o = 0;
        let mut scratch = 0u8;
        self.encode_one_generic(input, &mut scratch, |scratch_mut_ref| {
            o += 1;
            Some(scratch_mut_ref)
        });
        o
    }
}

impl Encoder<usize> for GenericEncoder {
    fn encode(&self, input: &[usize], output: &mut [u8]) -> (usize, usize) {
        let mut i = 0;
        let mut o = 0;
        while i < input.len() {
            match self.encode_one(input[i], &mut output[o..]) {
                Some(output_generated) => {
                    i += 1;
                    o += output_generated;
                },
                None => { break; },
            }
        }
        (i, o)
    }
}

#[cfg(test)]
mod varint_tests {
    use super::*;

    trait Testable {
        fn run_test(&self, decoded: &[usize], encoded: &[u8]);
    }

    impl<D: Decoder<usize>, E: Encoder<usize>> Testable for (D, E) {
        fn run_test(&self, decoded: &[usize], encoded: &[u8]) {
            let mut actual_decoded_scratch = vec![0usize; decoded.len() * 10];
            let mut actual_encoded_scratch = vec![0u8; encoded.len() * 10];
            let actual_decoded = {
                let (e, d) = self.0.decode(encoded, &mut actual_decoded_scratch[..]);
                assert_eq!(e, encoded.len());
                &actual_decoded_scratch[0..d]
            };
            let actual_encoded = {
                let (d, e) = self.1.encode(decoded, &mut actual_encoded_scratch[..]);
                assert_eq!(d, decoded.len());
                &actual_encoded_scratch[0..e]
            };
            if decoded != actual_decoded || encoded != actual_encoded {
                println!("expect decoded: {:?}", decoded);
                println!("actual decoded: {:?}", actual_decoded);
                println!("expect encoded: {:?}", encoded);
                println!("actual encoded: {:?}", actual_encoded);
                panic!("varint test failed");
            }
        }
    }

    fn run_test(decoded: &[usize], encoded: &[u8]) {
        (GenericDecoder::new(), GenericEncoder::new()).run_test(decoded, encoded);
    }

    #[test]
    fn test_1() {
        let decoded = [1usize];
        let encoded = [0b00000001u8];
        run_test(&decoded, &encoded);
    }

    #[test]
    fn test_2() {
        let decoded = [300usize];
        let encoded = [0b10101100, 0b00000010];
        run_test(&decoded, &encoded);
    }
}
