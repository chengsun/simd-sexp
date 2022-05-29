pub trait XorMaskedAdjacent {
    fn xor_masked_adjacent(&self, bitstring: u64, mask: u64, lo_fill: bool) -> u64;
}

#[derive(Copy, Clone, Debug)]
pub struct Generic {}

impl Generic {
    pub fn new() -> Self { Self {} }
}

impl XorMaskedAdjacent for Generic {
    #[inline(always)]
    fn xor_masked_adjacent(&self, bitstring: u64, mask: u64, lo_fill: bool) -> u64 {
        let bitstring = bitstring & mask;
        let i1 = mask.wrapping_sub(bitstring << 1);
        let lsb = mask & (-(mask as i64) as u64);
        let i2 = i1 & !(if lo_fill { lsb } else { 0 });
        (!i2 ^ bitstring) & mask
    }
}

#[cfg(target_arch = "x86_64")]
mod x86 {
    use core::arch::x86_64::*;

    use super::XorMaskedAdjacent;

    #[derive(Copy, Clone, Debug)]
    pub struct Bmi2 {
        _feature_detected_witness: ()
    }

    impl Bmi2 {
        pub fn new() -> Option<Self> {
            if is_x86_feature_detected!("bmi2") {
                return Some(Self { _feature_detected_witness: () });
            }
            None
        }

        #[target_feature(enable = "bmi2")]
        #[inline]
        unsafe fn _xor_masked_adjacent(&self, bitstring: u64, mask: u64, lo_fill: bool) -> u64 {
            let d1 = _pext_u64(bitstring, mask);
            let d2 = d1 ^ ((d1 << 1) | (lo_fill as u64));
            _pdep_u64(d2, mask)
        }
    }

    impl XorMaskedAdjacent for Bmi2 {
        #[inline(always)]
        fn xor_masked_adjacent(&self, bitstring: u64, mask: u64, lo_fill: bool) -> u64 {
            let () = self._feature_detected_witness;
            unsafe {
                self._xor_masked_adjacent(bitstring, mask, lo_fill)
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
pub use x86::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::*;

    trait Testable {
        fn run_test(&self, bitstring: u64, mask: u64, lo_fill: bool, output: u64);
    }

    impl<T: XorMaskedAdjacent> Testable for T {
        fn run_test(&self, bitstring: u64, mask: u64, lo_fill: bool, output: u64) {
            let bitstring = bitrev64(bitstring);
            let mask = bitrev64(mask);
            let output = bitrev64(output);
            let actual_output = self.xor_masked_adjacent(bitstring, mask, lo_fill);
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
    }

    fn run_test(bitstring: u64, mask: u64, lo_fill: bool, output: u64) {
        let generic = Generic {};
        generic.run_test(bitstring, mask, lo_fill, output);
        #[cfg(target_arch = "x86_64")]
        match Bmi2::new() {
            None => (),
            Some(bmi2) => bmi2.run_test(bitstring, mask, lo_fill, output)
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
