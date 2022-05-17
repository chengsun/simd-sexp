#[inline(always)]
pub fn range_starts_single(bm: u64) -> u64 {
  bm & !(bm << 1)
}

/** input:  xxx xxx|x   x
    output: x   x  |    x
 */
#[inline(always)]
pub fn range_starts(bm: u64, prev: bool) -> u64 {
  return bm & !(bm << 1 | prev as u64);
}

/** input:  xxx xxx|x   x
    output: x  xx  | x  x
 */
#[inline(always)]
pub fn range_transitions(bm: u64, prev: bool) -> u64 {
    return range_starts(bm, prev) | range_starts(!bm, !prev);
}

/** input:  xxx xxx|x   x
    output:  xx  xx|x
 */
#[inline(always)]
pub fn range_tails(bm: u64, prev_bm: u64) -> u64 {
    return bm & (bm << 1 | prev_bm >> 63);
}

/** input:  xxx xxx|x   x
    output:    x   |     x

    the end is *exclusive*, i.e. one-past-the end character-wise.
 */
#[inline(always)]
pub fn odd_range_ends(bm: u64, prev_overflow: bool) -> (u64, bool) {
  const BM_EVEN: u64 = 0xAAAAAAAAAAAAAAAA;
  const BM_ODD:  u64 = 0x5555555555555555;

  let bm_start = range_starts_single(bm & !(bm << 1)) | (prev_overflow as u64);

  let bm_start_even = bm_start & (BM_EVEN ^ (prev_overflow as u64));
  let next_overflow = bm_start_even.wrapping_add(bm) < bm;
  let bm_end_1 = (bm_start_even.wrapping_add(bm)) & !bm;
  let bm_end_oddlen_1 = bm_end_1 & BM_ODD;

  let bm_start_odd = bm_start & (BM_ODD ^ (prev_overflow as u64));
  let bm_end_2 = (bm_start_odd.wrapping_add(bm)) & !bm;
  let bm_end_oddlen_2 = bm_end_2 & BM_EVEN;

  let bm_end_oddlen = bm_end_oddlen_1 | bm_end_oddlen_2;

  return (bm_end_oddlen, next_overflow);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::*;

    fn run_test(input: u64, output: u64, in_ovf: bool, out_ovf: bool) {
        let input = bitrev64(input);
        let output = bitrev64(output);
        let (actual_output, actual_out_ovf) = odd_range_ends(input, in_ovf);

        if output != actual_output || out_ovf != actual_out_ovf {
            print!("input:        ");
            print_bitmask_le(input, 64);
            print!("in ovf:       {}", in_ovf);
            print!("expected out: ");
            print_bitmask_le(output, 64);
            print!("actual out:   ");
            print_bitmask_le(actual_output, 64);
            print!("expected ovf: {}", out_ovf);
            print!("actual ovf:   {}", actual_out_ovf);
            panic!("odd_range_ends test failed");
        }
    }

    /* NB: the following tests are written with bits in string order (i.e. reversed)! */

    #[test]
    fn test_1() {
        let input__ = 0b0000000000000000000000000000000000000000000000000000000000000000;
        let in_ovf_ = false;
        let output_ = 0b0000000000000000000000000000000000000000000000000000000000000000;
        let out_ovf = false;
        run_test(input__, output_, in_ovf_, out_ovf);
    }

    #[test]
    fn test_2() {
        let input__ = 0b1000000000000000000000000000000000000000000000000000000000000000;
        let in_ovf_ = false;
        let output_ = 0b0100000000000000000000000000000000000000000000000000000000000000;
        let out_ovf = false;
        run_test(input__, output_, in_ovf_, out_ovf);
    }

    #[test]
    fn test_3() {
        let input__ = 0b1100000000000000000000000000000000000000000000000000000000000000;
        let in_ovf_ = false;
        let output_ = 0b0000000000000000000000000000000000000000000000000000000000000000;
        let out_ovf = false;
        run_test(input__, output_, in_ovf_, out_ovf);
    }

    #[test]
    fn test_4() {
        let input__ = 0b1110000000000000000000000000000000000000000000000000000000000000;
        let in_ovf_ = false;
        let output_ = 0b0001000000000000000000000000000000000000000000000000000000000000;
        let out_ovf = false;
        run_test(input__, output_, in_ovf_, out_ovf);
    }

    #[test]
    fn test_5() {
        let input__ = 0b1110110111011011100000000000000000000000000000000000000000000000;
        let in_ovf_ = false;
        let output_ = 0b0001000000100000010000000000000000000000000000000000000000000000;
        let out_ovf = false;
        run_test(input__, output_, in_ovf_, out_ovf);
    }

    #[test]
    fn test_6() {
        let input__ = 0b0000000000000000000000000000000000000000000000000000000000000000;
        let in_ovf_ = true;
        let output_ = 0b1000000000000000000000000000000000000000000000000000000000000000;
        let out_ovf = false;
        run_test(input__, output_, in_ovf_, out_ovf);
    }

    #[test]
    fn test_7() {
        let input__ = 0b1000000000000000000000000000000000000000000000000000000000000000;
        let in_ovf_ = true;
        let output_ = 0b0000000000000000000000000000000000000000000000000000000000000000;
        let out_ovf = false;
        run_test(input__, output_, in_ovf_, out_ovf);
    }

    #[test]
    fn test_8() {
        let input__ = 0b1100000000000000000000000000000000000000000000000000000000000000;
        let in_ovf_ = true;
        let output_ = 0b0010000000000000000000000000000000000000000000000000000000000000;
        let out_ovf = false;
        run_test(input__, output_, in_ovf_, out_ovf);
    }

    #[test]
    fn test_9() {
        let input__ = 0b1110000000000000000000000000000000000000000000000000000000000000;
        let in_ovf_ = true;
        let output_ = 0b0000000000000000000000000000000000000000000000000000000000000000;
        let out_ovf = false;
        run_test(input__, output_, in_ovf_, out_ovf);
    }

    #[test]
    fn test_10() {
        let input__ = 0b1110110111011011100000000000000000000000000000000000000000000000;
        let in_ovf_ = true;
        let output_ = 0b0000000000100000010000000000000000000000000000000000000000000000;
        let out_ovf = false;
        run_test(input__, output_, in_ovf_, out_ovf);
    }

    #[test]
    fn test_11() {
        let input__ = 0b0000000000000000000000000000000000000000000000000000000000000011;
        let in_ovf_ = false;
        let output_ = 0b0000000000000000000000000000000000000000000000000000000000000000;
        let out_ovf = false;
        run_test(input__, output_, in_ovf_, out_ovf);
    }

    #[test]
    fn test_12() {
        let input__ = 0b0000000000000000000000000000000000000000000000000000000000000001;
        let in_ovf_ = false;
        let output_ = 0b0000000000000000000000000000000000000000000000000000000000000000;
        let out_ovf = true;
        run_test(input__, output_, in_ovf_, out_ovf);
    }

    #[test]
    fn test_13() {
        let input__ = 0b0000000000000000000000000000000000000000000000000000000000000001;
        let in_ovf_ = true;
        let output_ = 0b1000000000000000000000000000000000000000000000000000000000000000;
        let out_ovf = true;
        run_test(input__, output_, in_ovf_, out_ovf);
    }
}
