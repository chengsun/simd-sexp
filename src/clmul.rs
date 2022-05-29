pub trait Clmul {
    fn clmul(&self, input: u64) -> u64;
}

#[derive(Copy, Clone, Debug)]
pub struct Generic {}

impl Generic {
    pub fn new() -> Self {
        Self {}
    }
}

impl Clmul for Generic {
    fn clmul(&self, input: u64) -> u64 {
        let mut output = 0u64;
        let mut cur_state = 0u64;
        for bit_index in 0..64 {
            if (input & (1 << bit_index)) != 0 {
                cur_state = cur_state ^ 1;
            }
            output = output | (cur_state << bit_index)
        }
        output
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod x86 {
    use super::Clmul;
    #[cfg(target_arch = "x86")]
    use core::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::*;

    #[derive(Copy, Clone, Debug)]
    pub struct Sse2Pclmulqdq { _feature_detected_witness: () }

    impl Sse2Pclmulqdq {
        pub fn new() -> Option<Self> {
            if is_x86_feature_detected!("sse2") && is_x86_feature_detected!("pclmulqdq") {
                return Some(Self { _feature_detected_witness: () });
            }
            None
        }

        #[target_feature(enable = "sse2,pclmulqdq")]
        #[inline]
        unsafe fn _clmul(&self, input: u64) -> u64 {
            _mm_cvtsi128_si64(_mm_clmulepi64_si128(_mm_set_epi64x(0i64, input as i64), _mm_set1_epi8(0xFFu8 as i8), 0x00)) as u64
        }
    }

    impl Clmul for Sse2Pclmulqdq {
        #[inline(always)]
        fn clmul(&self, input: u64) -> u64 {
            let () = self._feature_detected_witness;
            unsafe { self._clmul(input) }
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub use x86::*;

#[cfg(target_arch = "aarch64")]
mod aarch64 {
    use super::Clmul;
    use std::arch::is_aarch64_feature_detected;
    use std::arch::aarch64::*;

    #[derive(Copy, Clone, Debug)]
    pub struct Neon { _feature_detected_witness: () }

    impl Neon {
        pub fn new() -> Option<Self> {
            if is_aarch64_feature_detected!("neon") && is_aarch64_feature_detected!("aes") {
                return Some(Self { _feature_detected_witness: () });
            }
            None
        }

        #[target_feature(enable = "neon,aes")]
        #[inline]
        unsafe fn _clmul(&self, input: u64) -> u64 {
            vmull_p64(input, !0u64) as u64
        }
    }

    impl Clmul for Neon {
        #[inline(always)]
        fn clmul(&self, input: u64) -> u64 {
            let () = self._feature_detected_witness;
            unsafe { self._clmul(input) }
        }
    }
}

#[cfg(target_arch = "aarch64")]
pub use aarch64::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::*;

    trait Testable {
        fn run_test(&self, input: u64, output: u64);
    }

    impl<T: Clmul> Testable for T {
        fn run_test(&self, input: u64, output: u64) {
            let input = bitrev64(input);
            let output = bitrev64(output);
            let actual_output = self.clmul(input);
            if output != actual_output {
                print!("input:      ");
                print_bitmask_le(input, 64);
                print!("expect out: ");
                print_bitmask_le(output, 64);
                print!("actual out: ");
                print_bitmask_le(actual_output, 64);
                panic!("clmul test failed");
            }
        }
    }

    fn run_test(input: u64, output: u64) {
        let generic = Generic::new();
        generic.run_test(input, output);

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        match Sse2Pclmulqdq::new() {
            Some(sse2_pclmulqdq) => sse2_pclmulqdq.run_test(input, output),
            None => (),
        }

        #[cfg(target_arch = "aarch64")]
        match Neon::new() {
            Some(sse2_pclmulqdq) => sse2_pclmulqdq.run_test(input, output),
            None => (),
        }
    }

    #[test]
    fn test_1() {
        let input_ = 0b0;
        let output = 0b0;
        run_test(input_, output);
    }

    #[test]
    fn test_2() {
        let input_ = 0b1111111111111111111111111111111111111111111111111111111111111111;
        let output = 0b1010101010101010101010101010101010101010101010101010101010101010;
        run_test(input_, output);
    }

    #[test]
    fn test_3() {
        let input_ = 0b0111111111111111111111111111111111111111111111111111111111111111;
        let output = 0b0101010101010101010101010101010101010101010101010101010101010101;
        run_test(input_, output);
    }

    #[test]
    fn test_4() {
        let input_ = 0b11010010;
        let output = 0b10011100;
        run_test(input_, output);
    }
}
