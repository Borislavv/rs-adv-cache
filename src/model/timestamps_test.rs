#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use crate::config::{Rule, RuleKey, RuleValue, LifetimeRule};
    use crate::model::Entry;
    use crate::time;

    fn make_rule_with_ttl(ttl_secs: u64) -> Arc<Rule> {
        Arc::new(Rule {
            path: Some("/api/v1/user".to_string()),
            path_bytes: Some(b"/api/v1/user".to_vec()),
            cache_key: RuleKey {
                query: None,
                query_bytes: None,
                headers: None,
                headers_map: None,
            },
            cache_value: RuleValue {
                headers: None,
                headers_map: None,
            },
            refresh: Some(LifetimeRule {
                enabled: true,
                ttl: Some(Duration::from_secs(ttl_secs)),
                beta: None,
                coefficient: None,
            }),
        })
    }

    fn make_rule_without_ttl() -> Arc<Rule> {
        Arc::new(Rule {
            path: Some("/api/v1/user".to_string()),
            path_bytes: Some(b"/api/v1/user".to_vec()),
            cache_key: RuleKey {
                query: None,
                query_bytes: None,
                headers: None,
                headers_map: None,
            },
            cache_value: RuleValue {
                headers: None,
                headers_map: None,
            },
            refresh: None,
        })
    }

    // Note: Timestamp operations use time::unix_nano() which requires time::start() to be called
    // with a tokio runtime. For unit tests, we test the logic by directly manipulating timestamps
    // using set_refreshed_at_for_tests, or we can test in tokio test context.
    // These tests focus on verifying the timestamp logic rather than time::start integration.
    
    #[tokio::test]
    async fn test_touch_updates_touched_at() {
        // Initialize time with tokio runtime
        let _token = time::start(Duration::from_millis(1));
        
        let rule = make_rule_without_ttl();
        let queries: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let headers: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let entry = Entry::new(rule, &queries, &headers);

        let before = entry.touched_at();
        // Small delay to ensure timestamp changes
        tokio::time::sleep(Duration::from_millis(10)).await;
        entry.touch();
        let after = entry.touched_at();

        assert!(after > before);
    }

    #[tokio::test]
    async fn test_touched_at_returns_correct_value() {
        let _token = time::start(Duration::from_millis(1));
        
        let rule = make_rule_without_ttl();
        let queries: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let headers: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let entry = Entry::new(rule, &queries, &headers);

        entry.touch();
        let touched = entry.touched_at();
        
        // Should be approximately current time (within 10ms due to timing)
        let now = time::unix_nano();
        let diff = (now - touched).abs();
        assert!(diff < 10_000_000); // 10ms in nanoseconds
    }

    #[tokio::test]
    async fn test_touch_refreshed_at_updates_updated_at() {
        let _token = time::start(Duration::from_millis(1));
        
        let rule = make_rule_without_ttl();
        let queries: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let headers: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let entry = Entry::new(rule, &queries, &headers);

        let before = entry.fresh_at();
        tokio::time::sleep(Duration::from_millis(10)).await;
        entry.touch_refreshed_at();
        let after = entry.fresh_at();

        assert!(after > before);
    }

    #[tokio::test]
    async fn test_fresh_at_returns_updated_at() {
        let _token = time::start(Duration::from_millis(1));
        
        let rule = make_rule_without_ttl();
        let queries: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let headers: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let entry = Entry::new(rule, &queries, &headers);

        entry.touch_refreshed_at();
        let fresh = entry.fresh_at();
        let updated = entry.updated_at_ref().load(std::sync::atomic::Ordering::Relaxed);
        
        assert_eq!(fresh, updated);
    }

    #[tokio::test]
    async fn test_untouch_refreshed_at_sets_to_past() {
        let _token = time::start(Duration::from_millis(1));
        
        let rule = make_rule_with_ttl(10); // 10 seconds TTL
        let queries: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let headers: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let entry = Entry::new(rule, &queries, &headers);

        entry.touch_refreshed_at();
        tokio::time::sleep(Duration::from_millis(10)).await;
        let before = entry.fresh_at();
        
        entry.untouch_refreshed_at();
        let after = entry.fresh_at();

        // After should be approximately TTL nanoseconds in the past
        let expected_diff = 10_000_000_000i64; // 10 seconds in nanoseconds
        let actual_diff = before - after;
        
        // Allow some tolerance (within 1 second)
        assert!(actual_diff > expected_diff - 1_000_000_000);
        assert!(actual_diff < expected_diff + 1_000_000_000);
    }

    #[tokio::test]
    async fn test_untouch_refreshed_at_without_ttl() {
        let _token = time::start(Duration::from_millis(1));
        
        let rule = make_rule_without_ttl();
        let queries: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let headers: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let entry = Entry::new(rule, &queries, &headers);

        entry.touch_refreshed_at();
        tokio::time::sleep(Duration::from_millis(10)).await;
        let before = entry.fresh_at();
        
        entry.untouch_refreshed_at();
        let after = entry.fresh_at();

        // Without TTL, should set to past (subtracts 0), so should be same or very close
        // Since we sleep between touch and untouch, there might be small difference
        let diff = (before - after).abs();
        assert!(diff < 100_000_000); // Less than 100ms difference
    }

    #[test]
    fn test_set_refreshed_at_for_tests() {
        // This test doesn't require time::start, it just sets a value
        let rule = make_rule_without_ttl();
        let queries: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let headers: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let entry = Entry::new(rule, &queries, &headers);

        let test_timestamp = 1234567890000000000i64;
        entry.set_refreshed_at_for_tests(test_timestamp);
        
        assert_eq!(entry.fresh_at(), test_timestamp);
    }

    #[tokio::test]
    async fn test_multiple_touch_operations() {
        let _token = time::start(Duration::from_millis(1));
        
        let rule = make_rule_without_ttl();
        let queries: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let headers: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let entry = Entry::new(rule, &queries, &headers);

        let mut timestamps = Vec::new();
        for _ in 0..5 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            entry.touch();
            timestamps.push(entry.touched_at());
        }

        // Each touch should produce a timestamp >= previous
        for i in 1..timestamps.len() {
            assert!(timestamps[i] >= timestamps[i - 1]);
        }
    }

    #[tokio::test]
    async fn test_touch_and_touch_refreshed_at_independent() {
        let _token = time::start(Duration::from_millis(1));
        
        let rule = make_rule_without_ttl();
        let queries: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let headers: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let entry = Entry::new(rule, &queries, &headers);

        entry.touch();
        let touched = entry.touched_at();
        
        tokio::time::sleep(Duration::from_millis(10)).await;
        entry.touch_refreshed_at();
        let fresh = entry.fresh_at();

        // touch_refreshed_at should update fresh_at but not touched_at
        assert!(fresh > touched);
        assert_eq!(entry.touched_at(), touched);
    }

    #[tokio::test]
    async fn test_timestamp_operations_are_atomic() {
        let _token = time::start(Duration::from_millis(1));
        
        let rule = make_rule_without_ttl();
        let queries: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let headers: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let entry = Entry::new(rule, &queries, &headers);

        // Multiple tasks should not cause data races
        use std::sync::Arc;
        let entry = Arc::new(entry);
        let mut handles = vec![];

        for _ in 0..10 {
            let entry = entry.clone();
            let handle = tokio::spawn(async move {
                entry.touch();
                entry.touch_refreshed_at();
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // After all operations, timestamps should be valid
        let touched = entry.touched_at();
        let fresh = entry.fresh_at();
        assert!(touched > 0);
        assert!(fresh > 0);
    }
}
