//! Eviction logic.
//

use super::lock::{try_lock, try_rlock, EVICTION_LOCK_SPINS, EVICTION_RLOCK_SPINS};
use super::map::{Map, NUM_OF_SHARDS, SHARD_MASK};
use super::mode::LRUMode;
use super::shard::Value;
use std::hint;
use std::sync::atomic::Ordering;

const SHARDS_SAMPLE: i64 = 4;
const KEYS_SAMPLE: i64 = 8;

impl<V: Value> Map<V> {
    /// Evicts entries until within the specified limit.
    /// Returns (freed_bytes, evicted_count).
    pub fn evict_until_within_limit(&self, limit: i64, backoff: i64) -> (i64, i64) {
        match self.mode {
            LRUMode::Listing => self.evict_until_within_limit_by_list(limit, backoff),
            LRUMode::Sampling => self.evict_until_within_limit_by_sample(limit, backoff),
        }
    }

    /// Evicts using LRU list (listing mode).
    fn evict_until_within_limit_by_list(&self, limit: i64, mut backoff: i64) -> (i64, i64) {
        if !matches!(self.mode, LRUMode::Listing) {
            return (0, 0);
        }

        const MIN_LIMIT: i64 = 8 << 20; // 8 MiB

        let mut freed = 0i64;
        let mut evicted = 0i64;

        while backoff > 0 {
            let cur_usage = self.mem.load(Ordering::Relaxed);
            // Stop if we're within limit AND haven't freed much yet, or if storage is empty
            // The condition (cur_usage <= limit && freed <= MIN_LIMIT) means:
            // - we've reached the limit AND we haven't freed much yet (early stop optimization)
            // - if we're still above limit, we continue evicting
            if (cur_usage <= limit && freed <= MIN_LIMIT) || self.len() == 0 {
                return (freed, evicted);
            }

            let sh = self.next_shard();
            if sh.len() == 0 {
                backoff -= 1;
                hint::spin_loop();
                continue;
            }

            let (bytes_freed, did_remove) = sh.evict_one_lru_tail();
            if did_remove {
                self.mem.fetch_sub(bytes_freed, Ordering::Relaxed);
                self.len.fetch_sub(1, Ordering::Relaxed);
                freed += bytes_freed;
                evicted += 1;
            }
            backoff -= 1;
        }

        (freed, evicted)
    }

    /// Evicts using sampling (sampling mode).
    fn evict_until_within_limit_by_sample(&self, limit: i64, mut backoff: i64) -> (i64, i64) {
        if !matches!(self.mode, LRUMode::Sampling) || self.mem() <= limit || self.len() <= 0 {
            return (0, 0);
        }

        let mut freed = 0i64;
        let mut evicted = 0i64;

        while self.mem.load(Ordering::Relaxed) > limit && backoff > 0 {
            if let Some((sh, _victim)) = self.pick_victim_by_sample(SHARDS_SAMPLE, KEYS_SAMPLE) {
                if let Some(mut data_guard) = try_lock(&sh.data, EVICTION_LOCK_SPINS) {
                    let (bytes_freed, hit) = sh.remove_unlocked(&mut data_guard, _victim.key());
                    // Guard is automatically released when dropped
                    if bytes_freed > 0 || hit {
                        self.mem.fetch_sub(bytes_freed, Ordering::Relaxed);
                        self.len.fetch_sub(1, Ordering::Relaxed);
                        freed += bytes_freed;
                        evicted += 1;
                    }
                }
            }
            backoff -= 1;
        }

        (freed, evicted)
    }

    /// Picks a victim for eviction.
    pub fn pick_victim(&self, shards_sample: i64, keys_sample: i64) -> Option<(&super::Shard<V>, V)>
    where
        V: Clone,
    {
        match self.mode {
            LRUMode::Listing => self.pick_victim_by_list(),
            LRUMode::Sampling => self.pick_victim_by_sample(shards_sample, keys_sample),
        }
    }

    /// Picks a victim using LRU list.
    fn pick_victim_by_list(&self) -> Option<(&super::Shard<V>, V)>
    where
        V: Clone,
    {
        if !matches!(self.mode, LRUMode::Listing) {
            return None;
        }

        const PROBES: usize = 8;
        let start =
            (self.iter.fetch_add(1, Ordering::Relaxed).wrapping_sub(1) & SHARD_MASK) as usize;

        let mut best_at: Option<i64> = None;
        let mut best_v: Option<V> = None;
        let mut best_sh: Option<&super::Shard<V>> = None;

        for i in 0..PROBES {
            let idx = (start + i) & (NUM_OF_SHARDS - 1);
            let sh = &self.shards[idx];
            if sh.len() == 0 {
                continue;
            }

            if let Some(key) = sh.lru_peek_tail() {
                if let Some(v) = sh.get(key) {
                    let at = v.touched_at();
                    if best_at.is_none() || best_at.unwrap() > at {
                        best_at = Some(at);
                        best_v = Some(v);
                        best_sh = Some(sh);
                    }
                }
            }
        }

        if let (Some(sh), Some(v)) = (best_sh, best_v) {
            Some((sh, v))
        } else {
            None
        }
    }

    /// Picks a victim using sampling.
    fn pick_victim_by_sample(
        &self,
        shards_sample: i64,
        keys_sample: i64,
    ) -> Option<(&super::Shard<V>, V)>
    where
        V: Clone,
    {
        if !matches!(self.mode, LRUMode::Sampling) {
            return None;
        }

        let mut best_at: Option<i64> = None;
        let mut best_v: Option<V> = None;
        let mut best_sh: Option<&super::Shard<V>> = None;

        for _ in 0..shards_sample {
            let sh = self.next_shard();
            if sh.len() == 0 {
                continue;
            }

            let data_guard = try_rlock(&sh.data, EVICTION_RLOCK_SPINS);
            if data_guard.is_none() {
                hint::spin_loop();
                continue;
            }

            let data = data_guard.unwrap();
            let shard_len = sh.len();
            if shard_len == 0 {
                continue;
            }

            let mut to_scan_per_shard = keys_sample.min(shard_len);

            for (_, review_entry) in data.items.iter() {
                let at = review_entry.touched_at();
                if best_at.is_none() || best_at.unwrap() > at {
                    best_at = Some(at);
                    best_v = Some(review_entry.clone());
                    best_sh = Some(sh);
                }

                to_scan_per_shard -= 1;
                if to_scan_per_shard <= 0 {
                    break;
                }
            }
        }

        if let (Some(sh), Some(v)) = (best_sh, best_v) {
            Some((sh, v))
        } else {
            None
        }
    }
}
