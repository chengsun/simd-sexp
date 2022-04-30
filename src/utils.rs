#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
pub unsafe fn make_bitmask(lo: __m256i, hi: __m256i) -> u64 {
    (_mm256_movemask_epi8(lo) as u32 as u64) |
    ((_mm256_movemask_epi8(hi) as u32 as u64) << 32)
}

pub fn print64(i: u64) {
  println!("{:016x}", i);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "sse2")]
pub unsafe fn print128(i: __m128i) {
    let mut buf = [0u64; 2];
    _mm_storeu_si128(buf.as_mut_ptr() as *mut _, i);
    println!("{:016x}{:016x}\n", buf[1], buf[0]);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx")]
pub unsafe fn print256(i: __m256i) {
    let mut buf = [0u64; 4];
    _mm256_storeu_si256(buf.as_mut_ptr() as *mut _, i);
    println!("{:016x}{:016x}{:016x}{:016x}\n", buf[3], buf[2], buf[1], buf[0]);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx")]
pub unsafe fn print_char256(i: __m256i) {
    let mut buf = [0u8; 0x20];
    _mm256_storeu_si256(buf.as_mut_ptr() as *mut _, i);
    print!("[");
    for &x in buf.iter() {
        print!("{}", x as char);
    }
    println!("]");
}

fn print_bitmask(m: u64, n_bits: usize, little_endian: bool) {
    print!("[");
    let mut iter_le = 0..n_bits;
    let mut iter_be = iter_le.clone().rev();
    let iter: &mut dyn Iterator<Item = usize> = if little_endian { &mut iter_le } else { &mut iter_be };
    for i in iter {
        if m & (1 << i) != 0 {
            print!("x");
        } else {
            print!(" ");
        }
    }
    println!("]");
}

pub fn print_bitmask_be(m: u64, n_bits: usize) {
    print_bitmask(m, n_bits, false)
}

pub fn print_bitmask_le(m: u64, n_bits: usize) {
    print_bitmask(m, n_bits, true)
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx")]
pub unsafe fn print_mask256(i: __m256i) {
    let m: i32 = _mm256_movemask_epi8(i);
    print_bitmask_le(m as u32 as u64, 32);
}

pub fn bitrev64(x: u64) -> u64 {
    let x = x.swap_bytes();
    let x = ((x & 0xF0F0F0F0F0F0F0F0u64) >> 4) | ((x & 0x0F0F0F0F0F0F0F0Fu64) << 4);
    let x = ((x & 0xCCCCCCCCCCCCCCCCu64) >> 2) | ((x & 0x3333333333333333u64) << 2);
    let x = ((x & 0xAAAAAAAAAAAAAAAAu64) >> 1) | ((x & 0x5555555555555555u64) << 1);
    x
}
