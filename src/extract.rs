pub fn safe(out: &mut [usize], start_offset: usize, mut bitmask: u64) -> usize {
    let mut out_idx = 0;
    while bitmask != 0 {
        out[out_idx] = start_offset + (bitmask.trailing_zeros() as usize);
        out_idx += 1;
        bitmask &= bitmask - 1;
    }
    out_idx
}

pub fn fast(out: &mut [usize], start_offset: usize, mut bitmask: u64) -> usize {
    let end = bitmask.count_ones() as usize;
    let mut out_idx = 0;

    for _ in 0..8 {
        out[out_idx] = start_offset + (bitmask.trailing_zeros() as usize);
        out_idx += 1;
        bitmask &= bitmask - 1;
    }

    if bitmask != 0 {
        for _ in 0..8 {
            out[out_idx] = start_offset + (bitmask.trailing_zeros() as usize);
            out_idx += 1;
            bitmask &= bitmask - 1;
        }
    }

    while bitmask != 0 {
        out[out_idx] = start_offset + (bitmask.trailing_zeros() as usize);
        out_idx += 1;
        bitmask &= bitmask - 1;
    }

    return end;
}
