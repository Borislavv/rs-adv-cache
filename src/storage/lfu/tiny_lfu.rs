// Package lfu provides TinyLFU implementation.

use crate::config::Admission as AdmissionConfig;

use super::count_min_sketch::Sketch;
use super::door_keeper::Doorkeeper;
use super::helper::next_pow2;

/// Sharded admitter for TinyLFU.
pub struct ShardedAdmitter {
    mask: u32,
    shards: Vec<Shard>,
}

/// Shard contains sketch and doorkeeper.
struct Shard {
    /// 4-bit counters packed in 64-bit words (16 counters per word).
    sketch: Sketch,
    /// Simple Bloom-like bitset; reset with sketch aging.
    door: Doorkeeper,
}

impl ShardedAdmitter {
    /// Creates a new sharded admitter.
    pub fn new(cfg: &AdmissionConfig) -> Self {
        let capacity = cfg.capacity.unwrap_or(10000);
        let shards = cfg.shards.unwrap_or(4) as u32;
        let min_table_len = cfg.min_table_len_per_shard.unwrap_or(256) as u32;
        let sample_multiplier = cfg.sample_multiplier.unwrap_or(10) as u32;
        let door_bits_per_counter = cfg.door_bits_per_counter.unwrap_or(8) as u32;

        let per_shard_cap = capacity / shards as usize;
        let per_shard_cap = per_shard_cap.max(1);

        // Table length is a power-of-two >= perShardCap, clamped by MinTableLenPerShard
        let mut tbl_len = next_pow2(per_shard_cap) as u32;
        if tbl_len < min_table_len {
            tbl_len = min_table_len;
        }

        // Doorkeeper size is proportional to the counter space
        let door_bits = tbl_len * door_bits_per_counter;

        let num_shards = shards as usize;
        let mut shards_vec = Vec::with_capacity(num_shards);
        for _ in 0..num_shards {
            let sketch = Sketch::new(tbl_len, sample_multiplier);
            let door = Doorkeeper::new(door_bits);
            shards_vec.push(Shard {
                sketch,
                door,
            });
        }

        Self {
            mask: (shards_vec.len() - 1) as u32,
            shards: shards_vec,
        }
    }

    /// Records a key access.
    pub fn record(&self, h: u64) {
        let sh = &self.shards[(h & self.mask as u64) as usize];
        if sh.door.seen_or_add(h) {
            sh.sketch.increment(h);
        }
    }

    /// Returns true if the candidate should replace a victim.
    pub fn allow(&self, candidate: u64, victim: u64) -> bool {
        let sh = &self.shards[(candidate & self.mask as u64) as usize];
        if !sh.door.probably_seen(candidate) {
            return false;
        }
        let cf = sh.sketch.estimate(candidate);
        let vf = sh.sketch.estimate(victim);
        cf > vf
    }
}

impl Default for ShardedAdmitter {
    fn default() -> Self {
        // Create a minimal default configuration
        use std::sync::Arc;
        use std::sync::atomic::AtomicBool;
        let default_cfg = AdmissionConfig {
            enabled: false,
            is_enabled: Arc::new(AtomicBool::new(false)),
            capacity: Some(10000),
            shards: Some(4),
            min_table_len_per_shard: Some(256),
            door_bits_per_counter: Some(8),
            sample_multiplier: Some(10),
        };
        Self::new(&default_cfg)
    }
}

