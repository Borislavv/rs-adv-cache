//! Tests for TinyLFU implementation.
//

#[cfg(test)]
mod tests {
    use crate::config::Admission as AdmissionConfig;
    use crate::db::admission::tiny_lfu::ShardedAdmitter;
    use crate::db::storage::NUM_OF_SHARDS;
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::timeout;

    // Compact, high-signal config for unit tests (not for production).
    // Increase per-shard event density so frequency differences become visible quickly.
    fn cfg_test() -> AdmissionConfig {
        AdmissionConfig {
            enabled: true,
            is_enabled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            capacity: Some(100_000),
            shards: Some(512),
            min_table_len_per_shard: Some(NUM_OF_SHARDS as usize),
            door_bits_per_counter: Some(16),
            sample_multiplier: Some(12),
        }
    }

    // key returns a deterministic key for index i (1-based to avoid zero).
    fn key(i: usize) -> u64 {
        (i + 1) as u64
    }

    // record_twice simulates two observations per key:
    //  1. set doorkeeper bit
    //  2. increment sketch (frequency++)
    fn record_twice(tlfu: &ShardedAdmitter, keys: &[u64]) {
        for &k in keys {
            tlfu.record(k);
            tlfu.record(k);
        }
    }

    // record_once simulates a single observation per key:
    // sets the doorkeeper bit; may not reach the sketch yet.
    fn record_once(tlfu: &ShardedAdmitter, keys: &[u64]) {
        for &k in keys {
            tlfu.record(k);
        }
    }

    // AdmitStats tracks Allow() outcomes.
    struct AdmitStats {
        yes: usize,
        no: usize,
    }

    impl AdmitStats {
        fn new() -> Self {
            Self { yes: 0, no: 0 }
        }

        fn rate(&self) -> f64 {
            let total = self.yes + self.no;
            if total == 0 {
                return 0.0;
            }
            self.yes as f64 / total as f64
        }
    }

    /// TestTinyLFU_UniqueStreamRejectsAfterWarmup
    /// Warm up with a set of keys that have freq >= 1 (victims are "warm").
    /// Then submit brand-new unique candidates against random warm victims.
    /// Expect a very low admit rate for uniques (reject-on-tie policy).
    #[test]
    fn test_tiny_lfu_unique_stream_rejects_after_warmup() {
        let cfg = cfg_test();
        let tlfu = ShardedAdmitter::new(&cfg);

        const WARM_N: usize = 80_000;
        const TRIALS: usize = 50_000;

        // Warm up: each warm key observed twice -> sketch frequency >= 1.
        let mut warm = Vec::with_capacity(WARM_N);
        for i in 0..WARM_N {
            warm.push(key(i));
        }
        record_twice(&tlfu, &warm);

        // Unique candidates (not present in warm set).
        let mut stats = AdmitStats::new();
        let mut rng = StdRng::seed_from_u64(1);

        for i in 0..TRIALS {
            let candidate = key(WARM_N + 1 + i); // guaranteed new
            let victim = warm[rng.gen_range(0..WARM_N)];
            if tlfu.allow(candidate, victim) {
                stats.yes += 1;
            } else {
                stats.no += 1;
            }
        }

        // On a unique stream we expect a very low admit rate (< 10%).
        let rate = stats.rate();
        if rate >= 0.10 {
            panic!(
                "unique-stream admit rate too high: got={:.2}% want<10% (yes={} no={})",
                100.0 * rate,
                stats.yes,
                stats.no
            );
        }
        println!(
            "unique-stream admit rate: {:.2}% (yes={} no={})",
            100.0 * stats.rate(),
            stats.yes,
            stats.no
        );
    }

    /// TestTinyLFU_PrefersHotOverCold
    /// Make a small "hot" set truly hot (many observations) and a large "cold" set
    /// barely seen once. Then:
    ///   a) candidate=hot vs victim=cold  => expect high admit rate
    ///   b) candidate=cold vs victim=hot  => expect low admit rate
    #[test]
    fn test_tiny_lfu_prefers_hot_over_cold() {
        let cfg = cfg_test();
        let tlfu = ShardedAdmitter::new(&cfg);

        const HOT_N: usize = 2_000;
        const COLD_N: usize = 60_000;
        const TRIALS: usize = 50_000;

        let mut hot = Vec::with_capacity(HOT_N);
        for i in 0..HOT_N {
            hot.push(key(i + 1));
        }
        let mut cold = Vec::with_capacity(COLD_N);
        for i in 0..COLD_N {
            cold.push(key(10_000 + i + 1));
        }

        // Make hot keys truly hot: multiple passes of record_twice.
        for _ in 0..8 {
            record_twice(&tlfu, &hot);
        }
        // Cold keys: mark once (mostly doorkeeper), minimal frequency lift.
        record_once(&tlfu, &cold);

        let mut rng = StdRng::seed_from_u64(2);

        // a) hot candidate vs cold victim
        let mut hot_wins = AdmitStats::new();
        for _ in 0..TRIALS {
            let candidate = hot[rng.gen_range(0..HOT_N)];
            let victim = cold[rng.gen_range(0..COLD_N)];
            if tlfu.allow(candidate, victim) {
                hot_wins.yes += 1;
            } else {
                hot_wins.no += 1;
            }
        }

        // b) cold candidate vs hot victim
        let mut cold_wins = AdmitStats::new();
        for _ in 0..TRIALS {
            let candidate = cold[rng.gen_range(0..COLD_N)];
            let victim = hot[rng.gen_range(0..HOT_N)];
            if tlfu.allow(candidate, victim) {
                cold_wins.yes += 1;
            } else {
                cold_wins.no += 1;
            }
        }

        let hot_rate = hot_wins.rate();
        let cold_rate = cold_wins.rate();

        // Expect a clear advantage for hot and clear disadvantage for cold.
        if hot_rate < 0.85 {
            panic!(
                "hot vs cold admit too low: got={:.2}% want>=85% (yes={} no={})",
                100.0 * hot_rate,
                hot_wins.yes,
                hot_wins.no
            );
        }
        if cold_rate > 0.15 {
            panic!(
                "cold vs hot admit too high: got={:.2}% want<=15% (yes={} no={})",
                100.0 * cold_rate,
                cold_wins.yes,
                cold_wins.no
            );
        }

        println!(
            "hot vs cold admit: {:.2}% (yes={} no={})",
            100.0 * hot_rate,
            hot_wins.yes,
            hot_wins.no
        );
        println!(
            "cold vs hot admit: {:.2}% (yes={} no={})",
            100.0 * cold_rate,
            cold_wins.yes,
            cold_wins.no
        );
    }

    /// TestTinyLFU_ConcurrentSmoke
    /// Not a correctness proof for frequencies, but a fast concurrency check.
    /// It should finish within a short time without panics/data races.
    #[tokio::test]
    async fn test_tiny_lfu_concurrent_smoke() {
        let cfg = cfg_test();
        let tlfu = Arc::new(ShardedAdmitter::new(&cfg));

        let workers = num_cpus::get().max(2);

        let mut join_set = tokio::task::JoinSet::new();

        // Writers: Record()
        for i in 0..workers {
            let tlfu_clone = tlfu.clone();
            join_set.spawn(async move {
                let mut rng = StdRng::seed_from_u64((i + 1) as u64);
                for _ in 0..200_000 {
                    tlfu_clone.record(rng.gen());
                }
            });
        }

        // Arbiters: Allow()
        for i in 0..(workers / 2 + 1) {
            let tlfu_clone = tlfu.clone();
            join_set.spawn(async move {
                let mut rng = StdRng::seed_from_u64((1u64 << 32) + (i + 1) as u64);
                for _ in 0..200_000 {
                    let a: u64 = rng.gen();
                    let b: u64 = rng.gen();
                    let _ = tlfu_clone.allow(a, b);
                }
            });
        }

        // Wait for all tasks with timeout
        let result = timeout(Duration::from_secs(10), async {
            while let Some(result) = join_set.join_next().await {
                result.unwrap();
            }
        })
        .await;

        if result.is_err() {
            panic!("timeout: concurrent smoke took too long");
        }
    }
}
