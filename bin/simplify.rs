use simd_sexp::*;
use structural::Classifier;
use std::io::Read;

fn main() {
    let mut stdin = utils::stdin();
    let mut input = Vec::new();
    stdin.read_to_end(&mut input).unwrap();
    if input.last() == Some(&b'\n') {
        input.pop().unwrap();
    }
    let mut output: Vec<bool> = (0..input.len()).map(|_| false).collect();
    let mut classifier = structural::Generic::new();
    let mut index = 0;
    let mut total_bits = 0;
    classifier.structural_indices_bitmask(&input[..], |bitmask, bitmask_len| {
        extract::safe_generic(|bit_offset| {
            output[index + bit_offset] = true;
            total_bits += 1;
        }, bitmask);
        index += bitmask_len;
        structural::CallbackResult::Continue
    });
    let mut next_char = if total_bits % 2 == 0 { '(' } else { 'a' };
    for ov in output {
        if ov {
            print!("{}", next_char as char);
            next_char = match next_char {
                '(' => ')',
                _ => '(',
            };
        } else {
            print!(" ");
        }
    }
}
