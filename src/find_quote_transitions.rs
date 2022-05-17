use crate::clmul;
use crate::xor_masked_adjacent;

#[inline(always)]
pub fn find_quote_transitions<ClmulT: clmul::Clmul, XorMaskedAdjacentT: xor_masked_adjacent::XorMaskedAdjacent>
    (clmul: &ClmulT, xor_masked_adjacent: &XorMaskedAdjacentT, unescaped: u64, escaped: u64, prev_state: bool) -> (u64, bool) {
    debug_assert!(unescaped & escaped == 0);
    let c = clmul.clmul(unescaped);
    let d = xor_masked_adjacent.xor_masked_adjacent(c, escaped, !prev_state);
    let next_transitions = unescaped | d;
    let next_state = prev_state ^ ((next_transitions.count_ones() & 1) != 0);
    return (next_transitions, next_state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::*;

    fn run_test(unescaped: u64, escaped: u64, prev_state: bool, output: u64) {
        let unescaped = bitrev64(unescaped);
        let escaped = bitrev64(escaped);
        let output = bitrev64(output);
        let (actual_output, actual_next_state) =
            find_quote_transitions(&clmul::Generic::new(), &xor_masked_adjacent::Generic::new(), unescaped, escaped, prev_state);
        let next_state = prev_state ^ (output.count_ones() & 1 != 0);
        if output != actual_output || next_state != actual_next_state {
            print!("unescaped:  ");
            print_bitmask_le(unescaped, 64);
            print!("escaped:    ");
            print_bitmask_le(escaped, 64);
            println!("prev_state: {}", prev_state);
            print!("expect out: ");
            print_bitmask_le(output, 64);
            print!("actual out: ");
            print_bitmask_le(actual_output, 64);
            println!("expect next_state: {}", next_state);
            println!("actual next_state: {}", actual_next_state);
            panic!("find_quote_transitions test failed");
        }
    }

    /* NB: the following tests are written with bits in string order (i.e. reversed)! */

    #[test]
    fn test_1() {
        let unescaped_ = 0b10001010;
        let escaped___ = 0b00100000;
        let prev_state = false;
        let output____ = 0b10001010;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_2() {
        let unescaped_ = 0b1010001010;
        let escaped___ = 0b0000100000;
        let prev_state = false;
        let output____ = 0b1010101010;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_3() {
        let unescaped_ = 0b1000001010;
        let escaped___ = 0b0010100000;
        let prev_state = false;
        let output____ = 0b1000001010;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_4() {
        let unescaped_ = 0b1000001010;
        let escaped___ = 0b0010100000;
        let prev_state = false;
        let output____ = 0b1000001010;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_5() {
        let unescaped_ = 0b101000001010;
        let escaped___ = 0b000010100000;
        let prev_state = false;
        let output____ = 0b101010001010;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_6() {
        let unescaped_ = 0b10001000;
        let escaped___ = 0b00100010;
        let prev_state = false;
        let output____ = 0b10001010;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_7() {
        let unescaped_ = 0b1010001000;
        let escaped___ = 0b0000100010;
        let prev_state = false;
        let output____ = 0b1010101010;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_8() {
        let unescaped_ = 0b1000101000;
        let escaped___ = 0b0010000010;
        let prev_state = false;
        let output____ = 0b1000101000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_9() {
        let unescaped_ = 0b101000101000;
        let escaped___ = 0b000010000010;
        let prev_state = false;
        let output____ = 0b101010101000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    /* prev_state = 1 */

    #[test]
    fn test_10() {
        let unescaped_ = 0b10001010;
        let escaped___ = 0b00100000;
        let prev_state = true;
        let output____ = 0b10101010;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_11() {
        let unescaped_ = 0b1010001010;
        let escaped___ = 0b0000100000;
        let prev_state = true;
        let output____ = 0b1010001010;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_12() {
        let unescaped_ = 0b1000001010;
        let escaped___ = 0b0010100000;
        let prev_state = true;
        let output____ = 0b1010001010;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_13() {
        let unescaped_ = 0b1000001010;
        let escaped___ = 0b0010100000;
        let prev_state = true;
        let output____ = 0b1010001010;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_14() {
        let unescaped_ = 0b101000001010;
        let escaped___ = 0b000010100000;
        let prev_state = true;
        let output____ = 0b101000001010;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_15() {
        let unescaped_ = 0b10001000;
        let escaped___ = 0b00100010;
        let prev_state = true;
        let output____ = 0b10101010;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_16() {
        let unescaped_ = 0b1010001000;
        let escaped___ = 0b0000100010;
        let prev_state = true;
        let output____ = 0b1010001010;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_17() {
        let unescaped_ = 0b1000101000;
        let escaped___ = 0b0010000010;
        let prev_state = true;
        let output____ = 0b1010101000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_18() {
        let unescaped_ = 0b101000101000;
        let escaped___ = 0b000010000010;
        let prev_state = true;
        let output____ = 0b101000101000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    /* compressed. note that this is not actually realistic as all escaped quotes
    need to have at least one char before them. But the current implementation
    copes fine without. */

    #[test]
    fn test_19() {
        let unescaped_ = 0b1011;
        let escaped___ = 0b0100;
        let prev_state = false;
        let output____ = 0b1011;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_20() {
        let unescaped_ = 0b11011;
        let escaped___ = 0b00100;
        let prev_state = false;
        let output____ = 0b11111;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_21() {
        let unescaped_ = 0b10011;
        let escaped___ = 0b01100;
        let prev_state = false;
        let output____ = 0b10011;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_22() {
        let unescaped_ = 0b10011;
        let escaped___ = 0b01100;
        let prev_state = false;
        let output____ = 0b10011;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_23() {
        let unescaped_ = 0b110011;
        let escaped___ = 0b001100;
        let prev_state = false;
        let output____ = 0b111011;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_24() {
        let unescaped_ = 0b1010;
        let escaped___ = 0b0101;
        let prev_state = false;
        let output____ = 0b1011;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_25() {
        let unescaped_ = 0b11010;
        let escaped___ = 0b00101;
        let prev_state = false;
        let output____ = 0b11111;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_26() {
        let unescaped_ = 0b10110;
        let escaped___ = 0b01001;
        let prev_state = false;
        let output____ = 0b10110;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_27() {
        let unescaped_ = 0b110110;
        let escaped___ = 0b001001;
        let prev_state = false;
        let output____ = 0b111110;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    /* prev_state = 1 */

    #[test]
    fn test_28() {
        let unescaped_ = 0b1011;
        let escaped___ = 0b0100;
        let prev_state = true;
        let output____ = 0b1111;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_29() {
        let unescaped_ = 0b11011;
        let escaped___ = 0b00100;
        let prev_state = true;
        let output____ = 0b11011;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_30() {
        let unescaped_ = 0b10011;
        let escaped___ = 0b01100;
        let prev_state = true;
        let output____ = 0b11011;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_31() {
        let unescaped_ = 0b10011;
        let escaped___ = 0b01100;
        let prev_state = true;
        let output____ = 0b11011;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_32() {
        let unescaped_ = 0b110011;
        let escaped___ = 0b001100;
        let prev_state = true;
        let output____ = 0b110011;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_33() {
        let unescaped_ = 0b1010;
        let escaped___ = 0b0101;
        let prev_state = true;
        let output____ = 0b1111;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_34() {
        let unescaped_ = 0b11010;
        let escaped___ = 0b00101;
        let prev_state = true;
        let output____ = 0b11011;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_35() {
        let unescaped_ = 0b10110;
        let escaped___ = 0b01001;
        let prev_state = true;
        let output____ = 0b11110;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_36() {
        let unescaped_ = 0b110110;
        let escaped___ = 0b001001;
        let prev_state = true;
        let output____ = 0b110110;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    /* put at the front */

    #[test]
    fn test_37() {
        let unescaped_ = 0b1011000000000000000000000000000000000000000000000000000000000000;
        let escaped___ = 0b0100000000000000000000000000000000000000000000000000000000000000;
        let prev_state = false;
        let output____ = 0b1011000000000000000000000000000000000000000000000000000000000000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }


    #[test]
    fn test_38() {
        let unescaped_ = 0b1101100000000000000000000000000000000000000000000000000000000000;
        let escaped___ = 0b0010000000000000000000000000000000000000000000000000000000000000;
        let prev_state = false;
        let output____ = 0b1111100000000000000000000000000000000000000000000000000000000000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }
    #[test]
    fn test_39() {
        let unescaped_ = 0b1001100000000000000000000000000000000000000000000000000000000000;
        let escaped___ = 0b0110000000000000000000000000000000000000000000000000000000000000;
        let prev_state = false;
        let output____ = 0b1001100000000000000000000000000000000000000000000000000000000000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_40() {
        let unescaped_ = 0b1001100000000000000000000000000000000000000000000000000000000000;
        let escaped___ = 0b0110000000000000000000000000000000000000000000000000000000000000;
        let prev_state = false;
        let output____ = 0b1001100000000000000000000000000000000000000000000000000000000000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_41() {
        let unescaped_ = 0b1100110000000000000000000000000000000000000000000000000000000000;
        let escaped___ = 0b0011000000000000000000000000000000000000000000000000000000000000;
        let prev_state = false;
        let output____ = 0b1110110000000000000000000000000000000000000000000000000000000000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_42() {
        let unescaped_ = 0b1010000000000000000000000000000000000000000000000000000000000000;
        let escaped___ = 0b0101000000000000000000000000000000000000000000000000000000000000;
        let prev_state = false;
        let output____ = 0b1011000000000000000000000000000000000000000000000000000000000000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_43() {
        let unescaped_ = 0b1101000000000000000000000000000000000000000000000000000000000000;
        let escaped___ = 0b0010100000000000000000000000000000000000000000000000000000000000;
        let prev_state = false;
        let output____ = 0b1111100000000000000000000000000000000000000000000000000000000000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_44() {
        let unescaped_ = 0b1011000000000000000000000000000000000000000000000000000000000000;
        let escaped___ = 0b0100100000000000000000000000000000000000000000000000000000000000;
        let prev_state = false;
        let output____ = 0b1011000000000000000000000000000000000000000000000000000000000000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_45() {
        let unescaped_ = 0b1101100000000000000000000000000000000000000000000000000000000000;
        let escaped___ = 0b0010010000000000000000000000000000000000000000000000000000000000;
        let prev_state = false;
        let output____ = 0b1111100000000000000000000000000000000000000000000000000000000000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    /* prev_state = 1 */

    #[test]
    fn test_46() {
        let unescaped_ = 0b1011000000000000000000000000000000000000000000000000000000000000;
        let escaped___ = 0b0100000000000000000000000000000000000000000000000000000000000000;
        let prev_state = true;
        let output____ = 0b1111000000000000000000000000000000000000000000000000000000000000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_47() {
        let unescaped_ = 0b1101100000000000000000000000000000000000000000000000000000000000;
        let escaped___ = 0b0010000000000000000000000000000000000000000000000000000000000000;
        let prev_state = true;
        let output____ = 0b1101100000000000000000000000000000000000000000000000000000000000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_48() {
        let unescaped_ = 0b1001100000000000000000000000000000000000000000000000000000000000;
        let escaped___ = 0b0110000000000000000000000000000000000000000000000000000000000000;
        let prev_state = true;
        let output____ = 0b1101100000000000000000000000000000000000000000000000000000000000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_49() {
        let unescaped_ = 0b1001100000000000000000000000000000000000000000000000000000000000;
        let escaped___ = 0b0110000000000000000000000000000000000000000000000000000000000000;
        let prev_state = true;
        let output____ = 0b1101100000000000000000000000000000000000000000000000000000000000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_50() {
        let unescaped_ = 0b1100110000000000000000000000000000000000000000000000000000000000;
        let escaped___ = 0b0011000000000000000000000000000000000000000000000000000000000000;
        let prev_state = true;
        let output____ = 0b1100110000000000000000000000000000000000000000000000000000000000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_51() {
        let unescaped_ = 0b1010000000000000000000000000000000000000000000000000000000000000;
        let escaped___ = 0b0101000000000000000000000000000000000000000000000000000000000000;
        let prev_state = true;
        let output____ = 0b1111000000000000000000000000000000000000000000000000000000000000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_52() {
        let unescaped_ = 0b1101000000000000000000000000000000000000000000000000000000000000;
        let escaped___ = 0b0010100000000000000000000000000000000000000000000000000000000000;
        let prev_state = true;
        let output____ = 0b1101100000000000000000000000000000000000000000000000000000000000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_53() {
        let unescaped_ = 0b1011000000000000000000000000000000000000000000000000000000000000;
        let escaped___ = 0b0100100000000000000000000000000000000000000000000000000000000000;
        let prev_state = true;
        let output____ = 0b1111000000000000000000000000000000000000000000000000000000000000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_54() {
        let unescaped_ = 0b1101100000000000000000000000000000000000000000000000000000000000;
        let escaped___ = 0b0010010000000000000000000000000000000000000000000000000000000000;
        let prev_state = true;
        let output____ = 0b1101100000000000000000000000000000000000000000000000000000000000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    /* all */

    #[test]
    fn test_55() {
        let unescaped_ = 0b1111111111111111111111111111111111111111111111111111111111111111;
        let escaped___ = 0b0000000000000000000000000000000000000000000000000000000000000000;
        let prev_state = false;
        let output____ = 0b1111111111111111111111111111111111111111111111111111111111111111;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_56() {
        let unescaped_ = 0b0000000000000000000000000000000000000000000000000000000000000000;
        let escaped___ = 0b1111111111111111111111111111111111111111111111111111111111111111;
        let prev_state = false;
        let output____ = 0b1000000000000000000000000000000000000000000000000000000000000000;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_57() {
        let unescaped_ = 0b1010101010101010101010101010101010101010101010101010101010101010;
        let escaped___ = 0b0101010101010101010101010101010101010101010101010101010101010101;
        let prev_state = false;
        let output____ = 0b1011111111111111111111111111111111111111111111111111111111111111;
        run_test(unescaped_, escaped___, prev_state, output____);
    }

    #[test]
    fn test_58() {
        let unescaped_ = 0b0101010101010101010101010101010101010101010101010101010101010101;
        let escaped___ = 0b1010101010101010101010101010101010101010101010101010101010101010;
        let prev_state = false;
        let output____ = 0b1111111111111111111111111111111111111111111111111111111111111111;
        run_test(unescaped_, escaped___, prev_state, output____);
    }
}
