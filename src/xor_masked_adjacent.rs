#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

pub fn xor_masked_adjacent(bitstring: u64, mask: u64, lo_fill: bool) -> u64 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if is_x86_feature_detected!("bmi2") {
        return unsafe { xor_masked_adjacent_bmi2(bitstring, mask, lo_fill) };
    }

    xor_masked_adjacent_generic(bitstring, mask, lo_fill)
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "bmi2")]
pub unsafe fn xor_masked_adjacent_bmi2(bitstring: u64, mask: u64, lo_fill: bool) -> u64 {
    let d1 = _pext_u64(bitstring, mask);
    let d2 = d1 ^ ((d1 << 1) | (lo_fill as u64));
    _pdep_u64(d2, mask)
}

pub fn xor_masked_adjacent_generic(bitstring: u64, mask: u64, lo_fill: bool) -> u64 {
    let bitstring = bitstring & mask;
    let i1 = mask.wrapping_sub(bitstring << 1);
    let lsb = mask & (-(mask as i64) as u64);
    let i2 = i1 & !(if lo_fill { lsb } else { 0 });
    (!i2 ^ bitstring) & mask
}

#[cfg(test)]
mod xor_masked_adjacent_tests {
    use super::*;
    use crate::utils::*;

    fn run_test(bitstring: u64, mask: u64, lo_fill: bool, output: u64) {
        let bitstring = bitrev64(bitstring);
        let mask = bitrev64(mask);
        let output = bitrev64(output);
        let actual_output = xor_masked_adjacent(bitstring, mask, lo_fill);
        if output != actual_output {
            print!("bitstring:  ");
            print_bitmask_le(bitstring, 64);
            print!("mask:       ");
            print_bitmask_le(mask, 64);
            println!("lo_fill: {}", lo_fill);
            print!("expect out: ");
            print_bitmask_le(output, 64);
            print!("actual out: ");
            print_bitmask_le(actual_output, 64);
            panic!("xor_masked_adjacent test failed");
        }
    }

    #[test]
    fn test_1() {
        let bitstring = 0b11001010;
        let mask_____ = 0b11111111;
        let lo_fill__ = false;
        let output___ = 0b10101111;
        run_test(bitstring, mask_____, lo_fill__, output___);
    }

    #[test]
    fn test_2() {
        let bitstring = 0b11001010;
        let mask_____ = 0b11111111;
        let lo_fill__ = true;
        let output___ = 0b00101111;
        run_test(bitstring, mask_____, lo_fill__, output___);
    }

    #[test]
    fn test_3() {
        let bitstring = 0b1111010111011101;
        let mask_____ = 0b1010101010101010;
        let lo_fill__ = false;
        let output___ = 0b1000100010101010;
        run_test(bitstring, mask_____, lo_fill__, output___);
    }

    #[test]
    fn test_4() {
        let bitstring = 0b1100101000000000000000000000000000000000000000000000000000000000;
        let mask_____ = 0b1111111100000000000000000000000000000000000000000000000000000000;
        let lo_fill__ = false;
        let output___ = 0b1010111100000000000000000000000000000000000000000000000000000000;
        run_test(bitstring, mask_____, lo_fill__, output___);
    }

    #[test]
    fn test_5() {
        let bitstring = 0b1100101000000000000000000000000000000000000000000000000000000000;
        let mask_____ = 0b1111111100000000000000000000000000000000000000000000000000000000;
        let lo_fill__ = true;
        let output___ = 0b0010111100000000000000000000000000000000000000000000000000000000;
        run_test(bitstring, mask_____, lo_fill__, output___);
    }

    #[test]
    fn test_6() {
        let bitstring = 0b1111010111011101000000000000000000000000000000000000000000000000;
        let mask_____ = 0b1010101010101010000000000000000000000000000000000000000000000000;
        let lo_fill__ = false;
        let output___ = 0b1000100010101010000000000000000000000000000000000000000000000000;
        run_test(bitstring, mask_____, lo_fill__, output___);
    }
}
