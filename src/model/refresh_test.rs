#[cfg(test)]
mod tests {
    use crate::config;
    use crate::time;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::sleep;

    // Initialize ctime in test setup
    fn init_test() {
        time::start(Duration::from_millis(1));
    }

    /// TestShouldBeRefreshed_FloorGuard validates the hard floor: until elapsed >= coeff*ttl, result is false.
    #[tokio::test]
    async fn test_should_be_refreshed_floor_guard() {
        init_test();
        
        let rule = Arc::new(config::Rule {
            path: None,
            path_bytes: None,
            cache_key: config::RuleKey {
                query: None,
                query_bytes: None,
                headers: None,
                headers_map: None,
            },
            cache_value: config::RuleValue {
                headers: None,
                headers_map: None,
            },
            refresh: Some(config::LifetimeRule {
                enabled: true,
                ttl: Some(Duration::from_secs(1)),
                beta: Some(0.5),
                coefficient: Some(0.5),
            }),
        });
        
        let mut e = crate::model::Entry::init();
        e.rule = rule.clone();

        let cfg = config::new_test_config();

        e.updated_at.store(time::unix_nano(), Ordering::Relaxed);
        // Sleep slightly less than minStale to account for timing precision
        // minStale = 0.5 * 1s = 500ms, so sleep 450ms to ensure we're well below the floor
        // This accounts for ctime resolution (1ms) and potential sleep inaccuracy
        sleep(Duration::from_millis(450)).await;
        
        // Verify that elapsed time is still below minStale threshold
        // This should guarantee that is_probably_expired returns false
        if e.is_probably_expired(&cfg) {
            panic!("should be false before minStale floor");
        }

        sleep(Duration::from_secs(2)).await;
        let mut hits = 0;
        for i in 0..100 {
            if i % 10 == 0 {
                sleep(Duration::from_millis(1)).await;
            }
            if e.is_probably_expired(&cfg) {
                hits += 1;
            }
        }
        if hits == 0 {
            panic!("expected some refresh events after floor; got 0/100");
        }
    }

    /// TestShouldBeRefreshed_HighProbability checks that probability becomes high for very stale entries with large beta.
    #[tokio::test]
    async fn test_should_be_refreshed_high_probability() {
        init_test();
        
        let rule = Arc::new(config::Rule {
            path: None,
            path_bytes: None,
            cache_key: config::RuleKey {
                query: None,
                query_bytes: None,
                headers: None,
                headers_map: None,
            },
            cache_value: config::RuleValue {
                headers: None,
                headers_map: None,
            },
            refresh: Some(config::LifetimeRule {
                enabled: true,
                ttl: Some(Duration::from_secs(1)),
                beta: Some(8.0),
                coefficient: Some(0.0),
            }),
        });
        
        let mut e = crate::model::Entry::init();
        e.rule = rule.clone();
        
        // Set updated_at to 2 seconds ago
        let two_seconds_ago = time::unix_nano() - Duration::from_secs(2).as_nanos() as i64;
        e.updated_at.store(two_seconds_ago, Ordering::Relaxed);

        let cfg = config::new_test_config();

        let mut hits = 0;
        const TRIALS: usize = 200;
        for _ in 0..TRIALS {
            if e.is_probably_expired(&cfg) {
                hits += 1;
            }
        }
        if hits < (0.6 * TRIALS as f64) as usize {
            panic!("too few refresh events: {}/{}", hits, TRIALS);
        }
    }
}

