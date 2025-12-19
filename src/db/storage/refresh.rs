use std::sync::atomic::Ordering;

use super::lock::{try_rlock, REFRESH_GUARD_FACTOR, REFRESH_RLOCK_SPINS};
use super::map::{Map, NUM_OF_SHARDS, SHARD_MASK};
use super::shard::Value;

impl<V: Value> Map<V> {
    /// Peeks at an expired entry with TTL.
    pub fn peek_expired_ttl(&self) -> Option<V>
    where
        V: Clone,
    {
        if let Some(v) = self.next_queued_with_expired_ttl() {
            Some(v)
        } else {
            const DEFAULT_SAMPLE: usize = 32;
            self.peek_expired(DEFAULT_SAMPLE)
        }
    }

    /// Enqueues an expired key for refresh.
    pub fn enqueue_expired(&self, key: u64) -> bool {
        self.shard(key).enqueue_refresh(key)
    }

    /// Gets the next queued entry with expired TTL.
    pub fn next_queued_with_expired_ttl(&self) -> Option<V>
    where
        V: Clone,
    {
        let start =
            ((self.iter.fetch_add(1, Ordering::Relaxed).wrapping_sub(1)) & SHARD_MASK) as usize;

        for i in 0..NUM_OF_SHARDS {
            let idx = (start + i) & (NUM_OF_SHARDS - 1);
            let sh = &self.shards[idx];
            if let Some(k) = sh.dequeue_expired() {
                if let Some(v) = sh.get(k) {
                    // Double-check freshness
                    if v.is_expired(&self.cfg) {
                        return Some(v);
                    } else {
                        // Not ready; reset flag
                        v.clear_refresh_queued();
                    }
                }
            }
        }

        None
    }

    /// Peeks at expired entries by sampling.
    /// Uses is_probably_expired() which implements probabilistic refresh logic:
    /// - If elapsed < coefficient * ttl: returns false (no refresh)
    /// - If elapsed >= coefficient * ttl: uses probabilistic logic with beta
    ///   probability = 1 - exp(-beta * (elapsed / ttl))
    /// This allows gradual refresh starting from coefficient * ttl with low probability,
    /// preventing thundering herd while ensuring entries are refreshed before full TTL expiration.
    fn peek_expired(&self, sample: usize) -> Option<V>
    where
        V: Clone,
    {
        let max_seen = sample * REFRESH_GUARD_FACTOR;
        let shards = max_seen;

        let mut best: Option<V> = None;
        let mut seen = 0;
        let mut hit_seen = 0;

        'outer: for _shard in 0..shards {
            let sh = self.next_shard();
            if sh.len() == 0 {
                continue;
            }

            let data_guard = try_rlock(&sh.data, REFRESH_RLOCK_SPINS);
            if data_guard.is_none() {
                continue;
            }

            let data = data_guard.unwrap();
            for (_, entry) in data.items.iter() {
                if seen >= max_seen || hit_seen >= sample {
                    break 'outer;
                }

                // Use is_probably_expired() for probabilistic refresh logic
                if entry.is_probably_expired(&self.cfg) {
                    hit_seen += 1;
                    if best.is_none() {
                        best = Some(entry.clone());
                    } else if let (Some(ref b), Some(ref e)) = (best.as_ref(), Some(entry)) {
                        if b.fresh_at() > e.fresh_at() {
                            best = Some(entry.clone());
                        }
                    }
                }
                seen += 1;
            }
        }

        best
    }
}
