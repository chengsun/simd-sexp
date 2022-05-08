#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

use crate::clmul;
use crate::ranges::{range_starts, range_transitions};
use crate::utils::print_bitmask_le;
use crate::xor_masked_adjacent;

pub trait StartStopTransitions {
    fn start_stop_transitions(&self, start: u64, stop: u64, prev_state: bool) -> (u64, bool);
}

#[derive(Copy, Clone, Debug)]
pub struct Generic<ClmulT, XorMaskedAdjacentT> {
    clmul: ClmulT,
    xor_masked_adjacent: XorMaskedAdjacentT,
}

impl<ClmulT, XorMaskedAdjacentT> Generic<ClmulT, XorMaskedAdjacentT> {
    pub fn new(clmul: ClmulT, xor_masked_adjacent: XorMaskedAdjacentT) -> Self {
        Generic { clmul, xor_masked_adjacent }
    }
}

impl<ClmulT: clmul::Clmul, XorMaskedAdjacentT: xor_masked_adjacent::XorMaskedAdjacent> StartStopTransitions for Generic<ClmulT, XorMaskedAdjacentT> {
    fn start_stop_transitions(&self, start: u64, stop: u64, prev_state: bool) -> (u64, bool) {
        let transitions = (!start.wrapping_sub(stop | !prev_state as u64) & start) ^ (!stop.wrapping_sub(start | prev_state as u64) & stop);
        let ranges = self.clmul.clmul(transitions);
        let next_transitions = self.xor_masked_adjacent.xor_masked_adjacent(ranges, start | stop, false);
        let next_state = prev_state ^ ((next_transitions.count_ones() & 1) != 0);
        return (next_transitions, next_state);
    }
}

pub struct Bmi2 { _feature_detected_witness: () }

impl Bmi2 {
    pub fn new() -> Option<Self> {
        if is_x86_feature_detected!("bmi2") {
            return Some(Bmi2{ _feature_detected_witness: () });
        }
        None
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[target_feature(enable = "bmi2")]
    #[inline]
    unsafe fn _start_stop_transitions(&self, start: u64, stop: u64, prev_state: bool) -> (u64, bool) {
        assert!(start & stop == 0);

        let mask = start | stop;
        let compressed_start = _pext_u64(start, mask);
        let compressed_transitions = compressed_start ^ (compressed_start << 1 | prev_state as u64);
        let next_transitions = _pdep_u64(compressed_transitions, mask);
        let next_state = prev_state ^ ((next_transitions.count_ones() & 1) != 0);
        return (next_transitions, next_state);
    }
}

impl StartStopTransitions for Bmi2 {
    #[inline(always)]
    fn start_stop_transitions(&self, start: u64, stop: u64, prev_state: bool) -> (u64, bool) {
        let () = self._feature_detected_witness;
        unsafe { self._start_stop_transitions(start, stop, prev_state) }
    }
}

impl StartStopTransitions for Box<dyn StartStopTransitions> {
    fn start_stop_transitions(&self, start: u64, stop: u64, prev_state: bool) -> (u64, bool) {
        (**self).start_stop_transitions(start, stop, prev_state)
    }
}

pub fn runtime_detect() -> Box<dyn StartStopTransitions> {
    match Bmi2::new () {
        None => (),
        Some(start_stop_transitions) => { return Box::new(start_stop_transitions); }
    }
    Box::new(Generic::new(clmul::runtime_detect(), xor_masked_adjacent::runtime_detect()))
}

pub fn naive_stg_asdf(a: u64, bplus: u64, bminus: u64, verbose: bool) -> u64 {
    let mut state = 0;
    let mut result = 0u64;
    for i in 0..64 {
        let bit = 1u64 << i;
        if a & bit != 0 {
            if state == 0 {
                state = 1;
                result |= bit;
            } else if state == 1 {
                state = 0;
                result |= bit;
            }
        } else if bplus & bit != 0 {
            if state == 0 {
                state = 2;
                result |= bit;
            }
        } else if bminus & bit != 0 {
            if state == 2 {
                state = 0;
                result |= bit;
            }
        }
    }

    if verbose {
        print!("A:         ");
        print_bitmask_le(a, 64);
        print!("B+:        ");
        print_bitmask_le(bplus, 64);
        print!("B-:        ");
        print_bitmask_le(bminus, 64);
        print!("Result:    ");
        print_bitmask_le(result, 64);
    }

    result
}

#[inline(never)]
#[target_feature(enable = "bmi2,sse2,pclmulqdq")]
pub unsafe fn stg_asdf(a: u64, bplus: u64, bminus: u64, verbose: bool) -> u64 {
    use crate::clmul::Clmul;
    let clmul = clmul::Sse2Pclmulqdq::new().unwrap();
    let start_stop_transitions = Bmi2::new().unwrap();
    let j_a_assume_aplus = clmul.clmul(a);
    let j_a_assume_aminus = clmul.clmul(a) ^ !0;
    let aplus_assume_aplus = range_starts(j_a_assume_aplus, false);
    let aplus_assume_aminus = range_starts(j_a_assume_aminus, true);
    let aminus_assume_aplus = aplus_assume_aminus;
    let aminus_assume_aminus = aplus_assume_aplus;
    let (tj_b, _) = start_stop_transitions.start_stop_transitions(bplus, bminus, false);
    let j_b = clmul.clmul(tj_b);
    let j_assume_aplus = j_a_assume_aplus | j_b;
    let j_assume_aminus = j_a_assume_aminus | j_b;
    let k_assume_aplus = range_starts(j_assume_aplus, false);
    let k_assume_aminus = range_starts(j_assume_aminus, false);
    let (tl_a_assume_aplus, _) = start_stop_transitions.start_stop_transitions(aplus_assume_aplus & k_assume_aplus, aminus_assume_aplus, false);
    let (tl_a_assume_aminus, _) = start_stop_transitions.start_stop_transitions(aplus_assume_aminus & k_assume_aminus, aminus_assume_aminus, false);
    let (tl_b_assume_aplus, _) = start_stop_transitions.start_stop_transitions(bplus & k_assume_aplus, bminus, false);
    let (tl_b_assume_aminus, _) = start_stop_transitions.start_stop_transitions(bplus & k_assume_aminus, bminus, false);
    let l_a_assume_aplus = clmul.clmul(tl_a_assume_aplus);
    let l_a_assume_aminus = clmul.clmul(tl_a_assume_aminus);
    let l_b_assume_aplus = clmul.clmul(tl_b_assume_aplus);
    let l_b_assume_aminus = clmul.clmul(tl_b_assume_aminus);
    let l_assume_aplus = l_a_assume_aplus | l_b_assume_aplus;
    let l_assume_aminus = l_a_assume_aminus | l_b_assume_aminus;
    let m_assume_aplus = range_transitions(l_assume_aplus, false);
    let m_assume_aminus = range_transitions(l_assume_aminus, false);
    let switch_assume_aplus = aplus_assume_aplus & !m_assume_aplus;
    let switch_assume_aminus = aplus_assume_aminus & !m_assume_aminus;
    let (taplus_is_what_to_assume, _) = start_stop_transitions.start_stop_transitions(switch_assume_aminus, switch_assume_aplus, true);
    let aplus_is_what_to_assume = clmul.clmul(taplus_is_what_to_assume) ^ !0;
    let result = (m_assume_aplus & aplus_is_what_to_assume) | (m_assume_aminus & !aplus_is_what_to_assume);

    if verbose {

        print!("If a+, a+: ");
        print_bitmask_le(aplus_assume_aplus, 64);
        print!("If a+, a-: ");
        print_bitmask_le(aminus_assume_aplus, 64);
        print!("If a+, b+: ");
        print_bitmask_le(bplus, 64);
        print!("If a+, b-: ");
        print_bitmask_le(bminus, 64);
        print!("If a+, Ja: ");
        print_bitmask_le(j_a_assume_aplus, 64);
        print!("If a+, Jb: ");
        print_bitmask_le(j_b, 64);
        print!("If a+, J:  ");
        print_bitmask_le(j_assume_aplus, 64);
        print!("If a+, K:  ");
        print_bitmask_le(k_assume_aplus, 64);
        print!("If a+, La: ");
        print_bitmask_le(l_a_assume_aplus, 64);
        print!("If a+, Lb: ");
        print_bitmask_le(l_b_assume_aplus, 64);
        print!("If a+, L:  ");
        print_bitmask_le(l_assume_aplus, 64);
        print!("If a+, M:  ");
        print_bitmask_le(m_assume_aplus, 64);
        print!("If a+, Sw: ");
        print_bitmask_le(switch_assume_aplus, 64);

        println!("");

        print!("If a-, a+: ");
        print_bitmask_le(aplus_assume_aminus, 64);
        print!("If a-, a-: ");
        print_bitmask_le(aminus_assume_aminus, 64);
        print!("If a-, b+: ");
        print_bitmask_le(bplus, 64);
        print!("If a-, b-: ");
        print_bitmask_le(bminus, 64);
        print!("If a-, Ja: ");
        print_bitmask_le(j_a_assume_aminus, 64);
        print!("If a-, Jb: ");
        print_bitmask_le(j_b, 64);
        print!("If a-, J:  ");
        print_bitmask_le(j_assume_aminus, 64);
        print!("If a-, K:  ");
        print_bitmask_le(k_assume_aminus, 64);
        print!("If a-, La: ");
        print_bitmask_le(l_a_assume_aminus, 64);
        print!("If a-, Lb: ");
        print_bitmask_le(l_b_assume_aminus, 64);
        print!("If a-, L:  ");
        print_bitmask_le(l_assume_aminus, 64);
        print!("If a-, M:  ");
        print_bitmask_le(m_assume_aminus, 64);
        print!("If a-, Sw: ");
        print_bitmask_le(switch_assume_aminus, 64);

        println!("");

        print!("Assume a+? ");
        print_bitmask_le(aplus_is_what_to_assume, 64);
        print!("Result:    ");
        print_bitmask_le(result, 64);

    }

    result
}

#[cfg(test)]
mod start_stop_transitions_tests {
    use super::*;
    use crate::utils::*;

    trait Testable {
        fn run_test(&self, start: u64, stop: u64, prev_state: bool, output: u64);
    }

    impl<T: StartStopTransitions> Testable for T {
        fn run_test(&self, start: u64, stop: u64, prev_state: bool, output: u64) {
            let start = bitrev64(start);
            let stop = bitrev64(stop);
            let output = bitrev64(output);
            let (actual_output, actual_next_state) = self.start_stop_transitions(start, stop, prev_state);
            let next_state = prev_state ^ (output.count_ones() & 1 != 0);
            if output != actual_output || next_state != actual_next_state {
                print!("start:      ");
                print_bitmask_le(start, 64);
                print!("stop:       ");
                print_bitmask_le(stop, 64);
                println!("prev_state: {}", prev_state);
                print!("expect out: ");
                print_bitmask_le(output, 64);
                print!("actual out: ");
                print_bitmask_le(actual_output, 64);
                println!("expect next_state: {}", next_state);
                println!("actual next_state: {}", actual_next_state);
                panic!("start_stop_transitions test failed");
            }
        }
    }

    fn run_test(start: u64, stop: u64, prev_state: bool, output: u64) {
        let generic = Generic::new(clmul::Generic::new(), xor_masked_adjacent::Generic::new());
        generic.run_test(start, stop, prev_state, output);

        match Bmi2::new() {
            Some(bmi2) => bmi2.run_test(start, stop, prev_state, output),
            None => (),
        }
    }

    /* NB: the following tests are written with bits in string order (i.e. reversed)! */

    #[test]
    fn test_1() {
        let start_____ = 0b100010100010101000;
        let stop______ = 0b001000001000000010;
        let prev_state = false;
        let output____ = 0b101010001010000010;
        run_test(start_____, stop______, prev_state, output____);
    }

    #[test]
    fn test_2() {
        let start_____ = 0b001000001000000010;
        let stop______ = 0b100010100010101000;
        let prev_state = false;
        let output____ = 0b001010001010000010;
        run_test(start_____, stop______, prev_state, output____);
    }

    #[test]
    fn test_3() {
        let start_____ = 0b101101110;
        let stop______ = 0b010010001;
        let prev_state = false;
        let output____ = 0b111011001;
        run_test(start_____, stop______, prev_state, output____);
    }

    #[test]
    fn test_4() {
        let start_____ = 0b010010001;
        let stop______ = 0b101101110;
        let prev_state = false;
        let output____ = 0b011011001;
        run_test(start_____, stop______, prev_state, output____);
    }

    #[test]
    fn test_5() {
        let start_____ = 0b100010100010101000;
        let stop______ = 0b001000001000000010;
        let prev_state = true;
        let output____ = 0b001010001010000010;
        run_test(start_____, stop______, prev_state, output____);
    }

    #[test]
    fn test_6() {
        let start_____ = 0b001000001000000010;
        let stop______ = 0b100010100010101000;
        let prev_state = true;
        let output____ = 0b101010001010000010;
        run_test(start_____, stop______, prev_state, output____);
    }

    #[test]
    fn test_7() {
        let start_____ = 0b101101110;
        let stop______ = 0b010010001;
        let prev_state = true;
        let output____ = 0b011011001;
        run_test(start_____, stop______, prev_state, output____);
    }

    #[test]
    fn test_8() {
        let start_____ = 0b010010001;
        let stop______ = 0b101101110;
        let prev_state = true;
        let output____ = 0b111011001;
        run_test(start_____, stop______, prev_state, output____);
    }

    #[test]
    fn test_9() {
        let start_____ = 0b11011001100011000011;
        let stop______ = 0b00100110011100111100;
        let prev_state = false;
        let output____ = 0b10110101010010100010;
        run_test(start_____, stop______, prev_state, output____);
    }

    #[test]
    fn test_10() {
        let start_____ = 0b1000101000101010000000000000000000000000000000000000000000000000;
        let stop______ = 0b0010000010000000100000000000000000000000000000000000000000000000;
        let prev_state = false;
        let output____ = 0b1010100010100000100000000000000000000000000000000000000000000000;
        run_test(start_____, stop______, prev_state, output____);
    }

    #[test]
    fn test_11() {
        let start_____ = 0b0010000010000000100000000000000000000000000000000000000000000000;
        let stop______ = 0b1000101000101010000000000000000000000000000000000000000000000000;
        let prev_state = false;
        let output____ = 0b0010100010100000100000000000000000000000000000000000000000000000;
        run_test(start_____, stop______, prev_state, output____);
    }

    #[test]
    fn test_12() {
        let start_____ = 0b1011011100000000000000000000000000000000000000000000000000000000;
        let stop______ = 0b0100100010000000000000000000000000000000000000000000000000000000;
        let prev_state = false;
        let output____ = 0b1110110010000000000000000000000000000000000000000000000000000000;
        run_test(start_____, stop______, prev_state, output____);
    }

    #[test]
    fn test_13() {
        let start_____ = 0b0100100010000000000000000000000000000000000000000000000000000000;
        let stop______ = 0b1011011100000000000000000000000000000000000000000000000000000000;
        let prev_state = false;
        let output____ = 0b0110110010000000000000000000000000000000000000000000000000000000;
        run_test(start_____, stop______, prev_state, output____);
    }

    #[test]
    fn test_14() {
        let start_____ = 0b1000101000101010000000000000000000000000000000000000000000000000;
        let stop______ = 0b0010000010000000100000000000000000000000000000000000000000000000;
        let prev_state = true;
        let output____ = 0b0010100010100000100000000000000000000000000000000000000000000000;
        run_test(start_____, stop______, prev_state, output____);
    }

    #[test]
    fn test_15() {
        let start_____ = 0b0010000010000000100000000000000000000000000000000000000000000000;
        let stop______ = 0b1000101000101010000000000000000000000000000000000000000000000000;
        let prev_state = true;
        let output____ = 0b1010100010100000100000000000000000000000000000000000000000000000;
        run_test(start_____, stop______, prev_state, output____);
    }

    #[test]
    fn test_16() {
        let start_____ = 0b1011011100000000000000000000000000000000000000000000000000000000;
        let stop______ = 0b0100100010000000000000000000000000000000000000000000000000000000;
        let prev_state = true;
        let output____ = 0b0110110010000000000000000000000000000000000000000000000000000000;
        run_test(start_____, stop______, prev_state, output____);
    }

    #[test]
    fn test_17() {
        let start_____ = 0b0100100010000000000000000000000000000000000000000000000000000000;
        let stop______ = 0b1011011100000000000000000000000000000000000000000000000000000000;
        let prev_state = true;
        let output____ = 0b1110110010000000000000000000000000000000000000000000000000000000;
        run_test(start_____, stop______, prev_state, output____);
    }

    #[test]
    fn test_18() {
        let start_____ = 0b1101100110001100001100000000000000000000000000000000000000000000;
        let stop______ = 0b0010011001110011110000000000000000000000000000000000000000000000;
        let prev_state = false;
        let output____ = 0b1011010101001010001000000000000000000000000000000000000000000000;
        run_test(start_____, stop______, prev_state, output____);
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct StgTestcase {
        a: u64,
        bplus: u64,
        bminus: u64,
    }

    enum StgTestcaseShrinkerState {
        ZeroA(usize),
        ZeroBPlus(usize),
        ZeroBMinus(usize),
        RShift(usize),
    }

    impl StgTestcaseShrinkerState {
        fn iter_all() -> Box<dyn Iterator<Item = Self>> {
            Box::new(
                (0..64).flat_map(
                    |i| [Self::ZeroA(i), Self::ZeroBPlus(i), Self::ZeroBMinus(i), Self::RShift(i)])
            )
        }
    }

    impl quickcheck::Arbitrary for StgTestcase {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let mut result = Self { a: 0, bplus: 0, bminus: 0 };
            for i in 0..64 {
                let bit = 1u64 << i;
                match g.choose(&[0, 1, 2, 3]).unwrap() {
                    1 => { result.a |= bit; },
                    2 => { result.bplus |= bit; },
                    3 => { result.bminus |= bit; },
                    _ => (),
                }
            }
            result
        }

        fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
            let value = self.clone();
            Box::new(
                StgTestcaseShrinkerState::iter_all().filter_map(move |shrinker_state| {
                    let mut candidate = value.clone();
                    match shrinker_state {
                        StgTestcaseShrinkerState::ZeroA(i) => { candidate.a &= !(1u64 << i); },
                        StgTestcaseShrinkerState::ZeroBPlus(i) => { candidate.bplus &= !(1u64 << i); },
                        StgTestcaseShrinkerState::ZeroBMinus(i) => { candidate.bminus &= !(1u64 << i); },
                        StgTestcaseShrinkerState::RShift(i) => {
                            let bit = 1u64 << i;
                            if (candidate.a | candidate.bplus | candidate.bminus) & bit == 0 {
                                let bit_below = bit - 1;
                                let bit_above = !(bit | bit_below);
                                let transform = |x| (x & bit_above) >> 1 | (x & bit_below);
                                candidate.a = transform(candidate.a);
                                candidate.bplus = transform(candidate.bplus);
                                candidate.bminus = transform(candidate.bminus);
                            }
                        },
                    }
                    if value != candidate {
                        Some(candidate)
                    } else {
                        None
                    }
                })
            )
        }
    }

    fn run_stg_test(testcase: StgTestcase) {
        let result = unsafe {
                stg_asdf(testcase.a, testcase.bplus, testcase.bminus, false)
        };
        let naive_result = naive_stg_asdf(testcase.a, testcase.bplus, testcase.bminus, false);
        if result != naive_result {
            println!("Test failed.");
            println!("");
            println!("BIT TWIDDLING");
            println!("=============");
            unsafe {
                stg_asdf(testcase.a, testcase.bplus, testcase.bminus, true);
            }
            println!("");
            println!("NAIVE");
            println!("=============");
            naive_stg_asdf(testcase.a, testcase.bplus, testcase.bminus, true);
            panic!("stg test failed");
        }
    }

    #[test] fn stg_test_1() { run_stg_test(StgTestcase{
        a:      bitrev64(0b10101),
        bplus:  bitrev64(0b01000),
        bminus: bitrev64(0b00010) }) }
    #[test] fn stg_test_2() { run_stg_test(StgTestcase{
        a:      bitrev64(0b0101),
        bplus:  bitrev64(0b1000),
        bminus: bitrev64(0b0010) }) }
    #[test] fn stg_test_3() { run_stg_test(StgTestcase{
        a:      bitrev64(0b111101),
        bplus:  bitrev64(0b000010),
        bminus: bitrev64(0b000000) }) }

    // quickcheck::quickcheck! {
    //     fn test_stg(testcase: StgTestcase) {
    //         run_stg_test(testcase)
    //     }
    // }
}
