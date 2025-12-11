// Package lfu provides helper functions for LFU.

/// Returns the smallest power-of-two >= x.
pub fn next_pow2(x: usize) -> usize {
    if x <= 1 {
        return 1;
    }
    let mut x = x - 1;
    x |= x >> 1;
    x |= x >> 2;
    x |= x >> 4;
    x |= x >> 8;
    x |= x >> 16;
    #[cfg(target_pointer_width = "64")]
    {
        x |= x >> 32;
    }
    x + 1
}

/// Produces well-diffused pseudo-independent values from a single 64-bit seed.
/// This is the SplitMix64 mixing function (public-domain; Steele et al.).
pub fn mix64(x: u64) -> u64 {
    const SPLITMIX64_INCREMENT: u64 = 0x9E3779B97F4A7C15;
    const SPLITMIX64_MUL1: u64 = 0xBF58476D1CE4E5B9;
    const SPLITMIX64_MUL2: u64 = 0x94D049BB133111EB;

    let mut x = x.wrapping_add(SPLITMIX64_INCREMENT);
    x = (x ^ (x >> 30)).wrapping_mul(SPLITMIX64_MUL1);
    x = (x ^ (x >> 27)).wrapping_mul(SPLITMIX64_MUL2);
    x ^ (x >> 31)
}

