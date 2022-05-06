pub trait Unescape {
    fn unescape_in_place(&self, in_out: &mut [u8]) -> Option<usize>;
}

#[derive(Copy, Clone, Debug)]
pub struct GenericUnescape {}

impl GenericUnescape {
    pub fn new() -> Self {
        Self {}
    }
}

impl Unescape for GenericUnescape {
    fn unescape_in_place(&self, in_out: &mut [u8]) -> Option<usize> {
        let mut input_index = 0;
        let mut output_index = 0;
        while input_index < in_out.len() {
            if in_out[input_index] == b'\\' {
                input_index = input_index + 1;
                if input_index >= in_out.len() {
                    return None;
                }
                match in_out[input_index] {
                    ch @ (b'"' | b'\'' | b'\\') => {
                        in_out[output_index] = ch;
                        input_index += 1;
                        output_index += 1;
                    },
                    b'b' => {
                        in_out[output_index] = b'\x07';
                        input_index += 1;
                        output_index += 1;
                    },
                    b'n' => {
                        in_out[output_index] = b'\n';
                        input_index += 1;
                        output_index += 1;
                    },
                    b'r' => {
                        in_out[output_index] = b'\r';
                        input_index += 1;
                        output_index += 1;
                    },
                    b't' => {
                        in_out[output_index] = b'\t';
                        input_index += 1;
                        output_index += 1;
                    },
                    b'x' => {
                        if input_index + 3 > in_out.len() {
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
                        in_out[output_index] =
                            match (digit_of_char(in_out[input_index + 1]), digit_of_char(in_out[input_index + 2])) {
                                (Some(d1), Some(d2)) => Some(d1 * 16 + d2),
                                _ => None
                            }?;
                        input_index += 3;
                        output_index += 1;
                    },
                    b'0'..=b'9' => {
                        if input_index + 3 > in_out.len() {
                            return None;
                        }
                        fn digit_of_char(ch: u8) -> Option<usize> {
                            match ch {
                                b'0'..=b'9' => Some((ch - b'0') as usize),
                                _ => None,
                            }
                        }
                        in_out[output_index] =
                            match (digit_of_char(in_out[input_index + 0]),
                                   digit_of_char(in_out[input_index + 1]),
                                   digit_of_char(in_out[input_index + 2])) {
                                (Some(d1), Some(d2), Some(d3)) => (d1 * 100 + d2 * 10 + d3).try_into().ok(),
                                _ => None
                            }?;
                        input_index += 3;
                        output_index += 1;
                    },
                    _ => {
                        in_out[output_index] = b'\\';
                        output_index += 1;
                    }
                }
            } else {
                in_out[output_index] = in_out[input_index];
                input_index += 1;
                output_index += 1;
            }
        }
        Some(output_index)
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

#[cfg(test)]
mod unescape_tests {
    use super::*;

    trait Testable {
        fn run_test(&self, input: &[u8], output: Option<&[u8]>);
    }

    impl<T: Unescape> Testable for T {
        fn run_test(&self, input: &[u8], output: Option<&[u8]>) {
            let mut actual_output_scratch = input.to_vec();
            let actual_output =
                match self.unescape_in_place(&mut actual_output_scratch[..]) {
                    Some(actual_output_count) => Some(&actual_output_scratch[0..actual_output_count]),
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
