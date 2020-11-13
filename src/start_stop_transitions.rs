#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

use crate::xor_masked_adjacent::*;

pub fn start_stop_transitions(start: u64, stop: u64, prev_state: bool) -> (u64, bool) {
    assert!(start & stop == 0);

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if is_x86_feature_detected!("bmi2") {
        return unsafe { start_stop_transitions_bmi2(start, stop, prev_state) };
    }

    start_stop_transitions_generic(start, stop, prev_state)
}

pub fn start_stop_transitions_generic(start: u64, stop: u64, prev_state: bool) -> (u64, bool) {
    use crate::utils::*;
    let transitions = (!start.wrapping_sub(stop | !prev_state as u64) & start) ^ (!stop.wrapping_sub(start | prev_state as u64) & stop);
    let ranges = unsafe { clmul(transitions) };
    let next_transitions = xor_masked_adjacent(ranges, start | stop, false);
    let next_state = prev_state ^ ((next_transitions.count_ones() & 1) != 0);
    return (next_transitions, next_state);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "bmi2")]
pub unsafe fn start_stop_transitions_bmi2(start: u64, stop: u64, prev_state: bool) -> (u64, bool) {
    let mask = start | stop;
    let compressed_start = _pext_u64(start, mask);
    let compressed_transitions = compressed_start ^ (compressed_start << 1 | prev_state as u64);
    let next_transitions = _pdep_u64(compressed_transitions, mask);
    let next_state = prev_state ^ ((next_transitions.count_ones() & 1) != 0);
    return (next_transitions, next_state);
}


#[cfg(test)]
mod start_stop_transitions_tests {
    use super::*;
    use crate::utils::*;

    fn run_test(start: u64, stop: u64, prev_state: bool, output: u64) {
        let start = bitrev64(start);
        let stop = bitrev64(stop);
        let output = bitrev64(output);
        let (actual_output, actual_next_state) = start_stop_transitions(start, stop, prev_state);
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
}
