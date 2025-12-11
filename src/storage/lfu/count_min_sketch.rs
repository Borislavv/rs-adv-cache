// Package lfu provides Count-Min Sketch implementation.

use std::sync::atomic::{AtomicU64, AtomicU32, Ordering};
use std::hint;

use super::helper::mix64;

const NIBBLE_MASK: u64 = 0xF;
const MASK_NIBBLES_64: u64 = 0x7777777777777777;

const MAX_CAS_TRIES: usize = 64;
const YIELD_EVERY_TRIES: usize = 8;
const SLEEP_AFTER_TRIES: usize = 32;
const DEFAULT_SAMPLES: u32 = 10;

/// Sketch is a TinyLFU-style Count-Min Sketch using 4-bit (nibble) counters.
pub struct Sketch {
    /// Words holds packed 4-bit counters: 16 counters per uint64.
    words: Vec<AtomicU64>,
    /// Mask is numCounters-1; numCounters must be a power of two.
    mask: u32,
    /// Adds is the total number of successful increments.
    adds: AtomicU64,
    /// ResetAt defines the logical aging window.
    reset_at: u64,
    /// AgingActive is a best-effort guard to avoid concurrent full-table aging.
    aging_active: AtomicU32,
}

impl Sketch {
    /// Initializes the sketch.
    pub fn new(table_len_pow2: u32, sample_multiplier: u32) -> Self {
        if table_len_pow2 == 0 || (table_len_pow2 & (table_len_pow2 - 1)) != 0 {
            panic!("sketch: tableLen must be power-of-two and > 0");
        }

        let num_counters = table_len_pow2 as u64;
        let word_count = ((num_counters + 15) / 16) as usize;
        let words: Vec<AtomicU64> = (0..word_count)
            .map(|_| AtomicU64::new(0))
            .collect();

        let sample_mult = if sample_multiplier == 0 {
            DEFAULT_SAMPLES
        } else {
            sample_multiplier
        };

        Self {
            words,
            mask: table_len_pow2 - 1,
            adds: AtomicU64::new(0),
            reset_at: sample_mult as u64 * num_counters,
            aging_active: AtomicU32::new(0),
        }
    }

    /// Increments 4 counters chosen by 4 mixed indices (min-of-4 scheme).
    pub fn increment(&self, h: u64) {
        self.maybe_reset();

        let mut hash = h;
        let i0 = (hash as u32) & self.mask;
        hash = mix64(hash);
        let i1 = (hash as u32) & self.mask;
        hash = mix64(hash);
        let i2 = (hash as u32) & self.mask;
        hash = mix64(hash);
        let i3 = (hash as u32) & self.mask;

        self.inc_at(i0);
        self.inc_at(i1);
        self.inc_at(i2);
        self.inc_at(i3);

        self.adds.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns the min of 4 counters for the 4 mixed indices of hash h.
    pub fn estimate(&self, h: u64) -> u8 {
        let mut hash = h;
        let i0 = (hash as u32) & self.mask;
        hash = mix64(hash);
        let i1 = (hash as u32) & self.mask;
        hash = mix64(hash);
        let i2 = (hash as u32) & self.mask;
        hash = mix64(hash);
        let i3 = (hash as u32) & self.mask;

        let c0 = self.get_at(i0);
        let c1 = self.get_at(i1);
        let c0 = c0.min(c1);
        let c2 = self.get_at(i2);
        let c0 = c0.min(c2);
        let c3 = self.get_at(i3);
        c0.min(c3)
    }

    /// Increments a single 4-bit lane at index idx, saturating at 15.
    fn inc_at(&self, idx: u32) {
        let (w, sh) = self.word_shift(idx);
        let ptr = &self.words[w];

        for tries in 1..=MAX_CAS_TRIES {
            let old = ptr.load(Ordering::Relaxed);
            let n = (old >> sh) & NIBBLE_MASK;
            if n == NIBBLE_MASK {
                return; // Already saturated (15)
            }
            let neu = old + (1 << sh);

            if ptr.compare_exchange(old, neu, Ordering::SeqCst, Ordering::Relaxed).is_ok() {
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
        // Give up after bounded attempts (lossy by design under contention)
    }

    /// Reads a single 4-bit lane at index idx.
    fn get_at(&self, idx: u32) -> u8 {
        let (w, sh) = self.word_shift(idx);
        let val = self.words[w].load(Ordering::Relaxed);
        ((val >> sh) & NIBBLE_MASK) as u8
    }

    /// Maps a counter index to (word index, bit shift) inside words[].
    fn word_shift(&self, idx: u32) -> (usize, u32) {
        // 16 nibbles per word => word = idx / 16, shift = (idx % 16) * 4
        (idx as usize >> 4, (idx & 0xF) << 2)
    }

    /// Triggers aging once per window in a best-effort manner.
    fn maybe_reset(&self) {
        if self.adds.load(Ordering::Relaxed) < self.reset_at {
            return;
        }
        if self.aging_active.compare_exchange(0, 1, Ordering::SeqCst, Ordering::Relaxed).is_ok() {
            // Double-check under the guard
            if self.adds.load(Ordering::Relaxed) >= self.reset_at {
                self.reset();
                self.adds.store(0, Ordering::Relaxed);
            }
            self.aging_active.store(0, Ordering::Relaxed);
        }
    }

    /// Halves all 4-bit lanes: new = (old >> 1) & maskNibbles64.
    pub fn reset(&self) {
        for word in &self.words {
            for tries in 1..=MAX_CAS_TRIES {
                let old = word.load(Ordering::Relaxed);
                let neu = (old >> 1) & MASK_NIBBLES_64;
                if word.compare_exchange(old, neu, Ordering::SeqCst, Ordering::Relaxed).is_ok() {
                    break;
                }
                if tries % YIELD_EVERY_TRIES == 0 {
                    hint::spin_loop();
                    if tries >= SLEEP_AFTER_TRIES {
                        std::thread::yield_now();
                    }
                }
            }
        }
    }
}

