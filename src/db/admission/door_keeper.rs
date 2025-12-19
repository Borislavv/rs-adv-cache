//! Doorkeeper (Bloom-like admission filter).
//

use std::hint;
use std::sync::atomic::{AtomicU64, Ordering};

use super::helper::{mix64, next_pow2};

const MAX_CAS_TRIES: usize = 64;
const YIELD_EVERY_TRIES: usize = 8;
const SLEEP_AFTER_TRIES: usize = 32;

/// Doorkeeper is a lightweight, Bloom-like admission filter.
pub struct Doorkeeper {
    /// Packed bit-array (64 bits per word).
    bits: Vec<AtomicU64>,
    /// Index mask: (numBitsRoundedToPow2 - 1).
    mask: u32,
}

impl Doorkeeper {
    /// Initializes the doorkeeper.
    pub fn new(total_bits: u32) -> Self {
        let total_bits = if total_bits == 0 { 1 } else { total_bits };
        let n = next_pow2(total_bits as usize) as u32;
        let word_count = ((n + 63) / 64) as usize;

        let bits: Vec<AtomicU64> = (0..word_count).map(|_| AtomicU64::new(0)).collect();

        Self { bits, mask: n - 1 }
    }

    /// Clears all bits (best-effort full reset).
    #[allow(dead_code)]
    pub fn reset(&self) {
        for bit in &self.bits {
            bit.store(0, Ordering::Relaxed);
        }
    }

    /// Returns true if all k (here: 3) probed bits are set.
    pub fn probably_seen(&self, h: u64) -> bool {
        let mut hash = h;
        let i0 = (hash as u32) & self.mask;
        hash = mix64(hash);
        let i1 = (hash as u32) & self.mask;
        hash = mix64(hash);
        let i2 = (hash as u32) & self.mask;

        self.get(i0) && self.get(i1) && self.get(i2)
    }

    /// Returns true if the key was probably seen already. Otherwise, sets the k bits and returns false.
    pub fn seen_or_add(&self, h: u64) -> bool {
        let mut hash = h;
        let i0 = (hash as u32) & self.mask;
        hash = mix64(hash);
        let i1 = (hash as u32) & self.mask;
        hash = mix64(hash);
        let i2 = (hash as u32) & self.mask;

        let b0 = self.get(i0);
        let b1 = self.get(i1);
        let b2 = self.get(i2);

        if b0 && b1 && b2 {
            return true;
        }

        self.set(i0);
        self.set(i1);
        self.set(i2);
        false
    }

    /// Maps a flat bit index to (wordIndex, bitMask) within d.bits.
    fn word_bit(&self, i: u32) -> (usize, u64) {
        let w = i >> 6; // i / 64
        let b = 1u64 << (i & 63); // 1 << (i % 64)
        (w as usize, b)
    }

    /// Atomically checks if a single bit is set.
    fn get(&self, i: u32) -> bool {
        let (w, b) = self.word_bit(i);
        let v = self.bits[w].load(Ordering::Relaxed);
        (v & b) != 0
    }

    /// Atomically sets a single bit using a bounded CAS loop.
    fn set(&self, i: u32) {
        let (w, b) = self.word_bit(i);
        let ptr = &self.bits[w];

        for tries in 1..=MAX_CAS_TRIES {
            let old = ptr.load(Ordering::Relaxed);
            let neu = old | b;
            // Fast path: already set or CAS succeeds
            if neu == old
                || ptr
                    .compare_exchange(old, neu, Ordering::SeqCst, Ordering::Relaxed)
                    .is_ok()
            {
                return;
            }
            // Cooperative backoff
            if tries % YIELD_EVERY_TRIES == 0 {
                hint::spin_loop();
                if tries >= SLEEP_AFTER_TRIES {
                    std::thread::yield_now();
                }
            }
        }
        // Best-effort semantics: if we fail all attempts, we give up
    }
}
