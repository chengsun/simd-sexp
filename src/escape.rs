pub fn escape_is_necessary(input: &[u8]) -> bool {
    let vector_classifier = vector_classifier::GenericBuilder::new().build(&structural::not_atom_like_lookup_tables());
    for ch in input {
        let mut ch_copy = [ch.clone()];
        vector_classifier.classify(&mut ch_copy);
        if ch_copy[0] != 0 || *ch == b'\\' || *ch < 0x20 || *ch >= 0x80 {
            return true;
        }
    }
    false
}

pub fn escape(input: &[u8]) -> Vec<u8> {
    let mut output: Vec<u8> = Vec::new();
    fn hex_char(i: u8) -> u8 {
        match i {
            (0..=9) => i + b'0',
            (10..=15) => i - 10 + b'a',
            _ => panic!("invalid integer to encode into single hex char: {}", i),
        }
    }
    for ch in input {
        match ch {
            b'"' => output.extend(b"\\\""),
            b'\\' => output.extend(b"\\\\"),
            b'\x07' => output.extend(b"\\b"),
            b'\n' => output.extend(b"\\n"),
            b'\r' => output.extend(b"\\r"),
            b'\t' => output.extend(b"\\t"),
            (0x00..=0x1F) | (0x80..=0xFF) => output.extend([b'\\', b'x', hex_char(ch / 0x10), hex_char(ch % 0x10)]),
            _ => output.push(*ch),
        }
    }
    output
}

pub trait Unescape {
    /// Consumes all the way up to the next unescaped double quote
    fn unescape(&self, input: &[u8], output: &mut [u8]) -> Option<(usize, usize)>;
}

#[derive(Copy, Clone, Debug)]
pub struct GenericUnescape {}

impl GenericUnescape {
    pub fn new() -> Self {
        Self {}
    }
}

impl Unescape for GenericUnescape {
    fn unescape(&self, input: &[u8], output: &mut [u8]) -> Option<(usize, usize)> {
        let mut input_index = 0;
        let mut output_index = 0;
        loop {
            let copy_len = memchr::memchr2(b'\"', b'\\', &input[input_index..])?;
            unsafe { std::ptr::copy_nonoverlapping(&input[input_index] as *const u8, &mut output[output_index] as *mut u8, copy_len); }
            input_index += copy_len;
            output_index += copy_len;
            match input[input_index] {
                b'\\' => {
                    input_index = input_index + 1;
                    if input_index >= input.len() {
                        return None;
                    }
                    match input[input_index] {
                        ch @ (b'"' | b'\'' | b'\\') => {
                            output[output_index] = ch;
                            input_index += 1;
                            output_index += 1;
                        },
                        b'b' => {
                            output[output_index] = b'\x07';
                            input_index += 1;
                            output_index += 1;
                        },
                        b'n' => {
                            output[output_index] = b'\n';
                            input_index += 1;
                            output_index += 1;
                        },
                        b'r' => {
                            output[output_index] = b'\r';
                            input_index += 1;
                            output_index += 1;
                        },
                        b't' => {
                            output[output_index] = b'\t';
                            input_index += 1;
                            output_index += 1;
                        },
                        b'x' => {
                            if input_index + 3 > input.len() {
                                return None;
                            }
                            fn digit_of_char(ch: u8) -> Option<u8> {
                                match ch {
                                    b'0'..=b'9' => Some(ch - b'0'),
                                    b'a'..=b'f' => Some(ch - b'a' + 10),
                                    b'A'..=b'F' => Some(ch - b'A' + 10),
                                    _ => None,
                                }
                            }
                            output[output_index] =
                                match (digit_of_char(input[input_index + 1]), digit_of_char(input[input_index + 2])) {
                                    (Some(d1), Some(d2)) => Some(d1 * 16 + d2),
                                    _ => None
                                }?;
                            input_index += 3;
                            output_index += 1;
                        },
                        b'0'..=b'9' => {
                            if input_index + 3 > input.len() {
                                return None;
                            }
                            fn digit_of_char(ch: u8) -> Option<usize> {
                                match ch {
                                    b'0'..=b'9' => Some((ch - b'0') as usize),
                                    _ => None,
                                }
                            }
                            output[output_index] =
                                match (digit_of_char(input[input_index + 0]),
                                       digit_of_char(input[input_index + 1]),
                                       digit_of_char(input[input_index + 2])) {
                                    (Some(d1), Some(d2), Some(d3)) => (d1 * 100 + d2 * 10 + d3).try_into().ok(),
                                    _ => None
                                }?;
                            input_index += 3;
                            output_index += 1;
                        },
                        _ => {
                            output[output_index] = b'\\';
                            output_index += 1;
                        }
                    }
                },
                b'"' => {
                    return Some((input_index, output_index));
                },
                _ => panic!("Unexpected char"),
            }
        }
    }
}

// #[derive(Copy, Clone, Debug)]
// pub struct Sse2Pclmulqdq { _feature_detected_witness: () }

// impl Sse2Pclmulqdq {
//     pub fn new() -> Option<Self> {
//         #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
//         if is_x86_feature_detected!("sse2") && is_x86_feature_detected!("pclmulqdq") {
//             return Some(Self { _feature_detected_witness: () });
//         }
//         None
//     }

//     #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
//     #[target_feature(enable = "sse2,pclmulqdq")]
//     unsafe fn _clmul(&self, input: u64) -> u64 {
//         _mm_cvtsi128_si64(_mm_clmulepi64_si128(_mm_set_epi64x(0i64, input as i64), _mm_set1_epi8(0xFFu8 as i8), 0x00)) as u64
//     }
// }

// impl Clmul for Sse2Pclmulqdq {
//     fn clmul(&self, input: u64) -> u64 {
//         let () = self._feature_detected_witness;
//         return unsafe { self._clmul(input) };
//     }
// }

// impl Clmul for Box<dyn Clmul> {
//     fn clmul(&self, input: u64) -> u64 {
//         (**self).clmul(input)
//     }
// }

// pub fn runtime_detect() -> Box<dyn Clmul> {
//     match Sse2Pclmulqdq::new () {
//         None => (),
//         Some(clmul) => { return Box::new(clmul); }
//     }
//     Box::new(Generic::new())
// }

use crate::{vector_classifier::{self, ClassifierBuilder, Classifier}, structural};

#[cfg(test)]
mod unescape_tests {
    use super::*;

    trait Testable {
        fn run_test(&self, input: &[u8], output: Option<&[u8]>);
    }

    impl<T: Unescape> Testable for T {
        fn run_test(&self, input: &[u8], output: Option<&[u8]>) {
            let mut input = input.to_vec();
            input.push(b'\"');
            let mut actual_output_scratch: Vec<u8> = (0..input.len()).map(|_| 0u8).collect();
            let actual_output =
                match self.unescape(&input[..], &mut actual_output_scratch[..]) {
                    Some((_, actual_output_count)) => Some(&actual_output_scratch[0..actual_output_count]),
                    None => None
                };
            if output != actual_output {
                println!("input:      {:?}", input);
                println!("expect out: {:?}", output);
                println!("actual out: {:?}", actual_output);
                panic!("unescape test failed");
            }
        }
    }

    fn run_test(input: &[u8], output: Option<&[u8]>) {
        let generic = GenericUnescape::new();
        generic.run_test(input, output);

        // match Sse2Pclmulqdq::new() {
        //     Some(sse2_pclmulqdq) => sse2_pclmulqdq.run_test(input, output),
        //     None => (),
        // }
    }

    #[test] fn test_backslash_b() { run_test(b"\\b", Some(&b"\x07"[..])); }
    #[test] fn test_backslash_n() { run_test(b"\\n", Some(&b"\n"[..])); }
    #[test] fn test_backslash_r() { run_test(b"\\r", Some(&b"\r"[..])); }
    #[test] fn test_backslash_t() { run_test(b"\\t", Some(&b"\t"[..])); }
    #[test] fn test_backslash_backslash() { run_test(b"\\\\", Some(&b"\\"[..])); }
    #[test] fn test_backslash_misc() { run_test(b"\\q", Some(&b"\\q"[..])); }
    #[test] fn test_backslash_dec_1() { run_test(b"\\123", Some(&b"\x7b"[..])); }
    #[test] fn test_backslash_dec_2() { run_test(b"\\256", None); }
    #[test] fn test_backslash_dec_3() { run_test(b"\\000", Some(&b"\x00"[..])); }
    #[test] fn test_backslash_dec_4() { run_test(b"\\00", None); }
    #[test] fn test_backslash_hex_1() { run_test(b"\\xaC", Some(&b"\xac"[..])); }
    #[test] fn test_backslash_hex_2() { run_test(b"\\xgg", None); }
    #[test] fn test_backslash_hex_3() { run_test(b"\\x00", Some(&b"\x00"[..])); }
    #[test] fn test_backslash_hex_4() { run_test(b"\\x2", None); }


    #[test]
    fn test_1() {
        let input_ = b"foo bar";
        let output = Some(&b"foo bar"[..]);
        run_test(input_, output);
    }

    #[test]
    fn test_2() {
        let input_ = b"foo\nbar";
        let output = Some(&b"foo\nbar"[..]);
        run_test(input_, output);
    }

    #[test]
    fn test_3() {
        let input_ = b"foo\\nbar";
        let output = Some(&b"foo\nbar"[..]);
        run_test(input_, output);
    }
}
