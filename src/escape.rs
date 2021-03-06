use crate::vector_classifier::{self, ClassifierBuilder, Classifier};

pub struct IsNecessary {
    vector_classifier: vector_classifier::GenericClassifier,
}

impl IsNecessary {
    pub fn new() -> Self {
        let accept: Vec<bool> = (0..=255).map(|ch| {
            match ch {
                b' ' | b'\t' | b'\n' | b'(' | b')' | b'"' |
                b';' | b'\\' | (0x00..=0x1F) | (0x80..=0xFF) => true,
                _ => false,
            }
        }).collect();
        let lookup_tables = vector_classifier::LookupTables::new(&accept.try_into().unwrap()).unwrap();
        Self {
            vector_classifier: vector_classifier::GenericBuilder::new().build(&lookup_tables),
        }
    }

    pub fn eval(&self, input: &[u8]) -> bool {
        if input.len() == 0 {
            return true;
        }

        // TODO: currently this is super naive; use the vectorised classifier??
        for i in 0..input.len() {
            let mut ch_copy = [input[i]];
            self.vector_classifier.classify(&mut ch_copy);
            if ch_copy[0] != 0 ||
                (i + 1 < input.len() &&
                 (&input[i..(i+2)] == b"#|" ||
                  &input[i..(i+2)] == b"|#"))
            {
                return true;
            }
        }
        false
    }
}

pub fn escape<WriterT: std::io::Write>(input: &[u8], output: &mut WriterT) -> Result<(), std::io::Error> {
    for ch in input {
        match ch {
            b'"' => output.write_all(b"\\\"")?,
            b'\\' => output.write_all(b"\\\\")?,
            b'\x07' => output.write_all(b"\\b")?,
            b'\n' => output.write_all(b"\\n")?,
            b'\r' => output.write_all(b"\\r")?,
            b'\t' => output.write_all(b"\\t")?,
            (0x00..=0x1F) | (0x80..=0xFF) => {
                let (d1, ch) = (ch / 100, ch % 100);
                let (d2, ch) = (ch / 10, ch % 10);
                let d3 = ch;
                output.write_all(&[b'\\', d1 + b'0', d2 + b'0', d3 + b'0'])?
            },
            _ => output.write_all(std::slice::from_ref(ch))?,
        }
    }
    Ok(())
}

pub trait Unescape {
    /// Expects input not to contain the starting double quote
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

#[cfg(test)]
mod tests {
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
