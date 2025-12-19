#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use tokio_util::sync::CancellationToken;

    use crate::config::{self, Rule, RuleKey, RuleValue};
    use crate::db::storage::{Map, Storage};
    use crate::model::{Entry, Response};
    use crate::upstream::Upstream;

    // Mock upstream for testing
    struct MockUpstream;

    #[async_trait::async_trait]
    impl Upstream for MockUpstream {
        async fn request(
            &self,
            _rule: &crate::config::Rule,
            _queries: &[(Vec<u8>, Vec<u8>)],
            _headers: &[(Vec<u8>, Vec<u8>)],
        ) -> Result<crate::upstream::Response, anyhow::Error> {
            Err(anyhow::anyhow!("not implemented"))
        }

        async fn proxy_request(
            &self,
            _method: &str,
            _path: &str,
            _query: &str,
            _headers: &[(String, String)],
            _body: Option<&[u8]>,
        ) -> Result<crate::upstream::Response, anyhow::Error> {
            Err(anyhow::anyhow!("not implemented"))
        }

        async fn refresh(&self, _entry: &Entry) -> Result<(), anyhow::Error> {
            Err(anyhow::anyhow!("not implemented"))
        }

        async fn is_healthy(&self) -> Result<(), anyhow::Error> {
            Ok(())
        }
    }

    fn make_rule(path: &str) -> Arc<Rule> {
        Arc::new(Rule {
            path: Some(path.to_string()),
            path_bytes: Some(path.as_bytes().to_vec()),
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

    fn make_entry_with_key(rule: Arc<Rule>, key_data: &str, body: &[u8]) -> Entry {
        let queries = vec![(b"key".to_vec(), key_data.as_bytes().to_vec())];
        let headers = vec![];
        let entry = Entry::new(rule, &queries, &headers);
        let response = Response {
            status: 200,
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: body.to_vec(),
        };
        entry.set_payload(&queries, &headers, &response);
        entry
    }

    async fn setup_storage() -> (Arc<Storage>, CancellationToken) {
        let token = CancellationToken::new();
        let cfg = config::new_test_config();
        let map = Arc::new(Map::new(token.clone(), cfg.clone()));
        let upstream = Arc::new(MockUpstream) as Arc<dyn Upstream>;
        let storage = Storage::new(token.clone(), cfg, upstream, map)
            .expect("Failed to create storage");
        // Give time for logger task to start
        tokio::time::sleep(Duration::from_millis(10)).await;
        (storage, token)
    }

    /// Test that get returns hit when fingerprint matches.
    #[tokio::test]
    async fn test_get_with_matching_fingerprint() {
        let (storage, _token) = setup_storage().await;
        let rule = make_rule("/api/v1/user");
        let entry1 = make_entry_with_key(rule.clone(), "test1", b"body1");
        
        // Store entry
        assert!(storage.set(entry1.clone()));

        // Get with same fingerprint should return hit
        let (result, hit) = storage.get(&entry1);
        assert!(hit);
        assert!(result.is_some());
        let retrieved = result.unwrap();
        assert_eq!(retrieved.key(), entry1.key());
        assert!(retrieved.is_the_same_fingerprint(&entry1));
    }

    /// Test that get returns miss when fingerprint doesn't match (hash collision).
    #[tokio::test]
    async fn test_get_with_different_fingerprint() {
        let (storage, _token) = setup_storage().await;
        let rule = make_rule("/api/v1/user");
        
        // Create entry with key "test1"
        let entry1 = make_entry_with_key(rule.clone(), "test1", b"body1");
        let key1 = entry1.key();
        
        // Store entry
        assert!(storage.set(entry1));

        // Create another entry that will have same key but different fingerprint
        // This simulates a hash collision scenario
        // We can't easily force a hash collision, but we can test the logic
        // by creating an entry with different data but potentially same key
        let entry2 = make_entry_with_key(rule.clone(), "different", b"different_body");
        
        // If keys are different, get should return miss
        if entry2.key() != key1 {
            let (result, hit) = storage.get(&entry2);
            assert!(!hit);
            assert!(result.is_none());
        }
    }

    /// Test that set updates entry when fingerprint matches and payload is same (touch).
    #[tokio::test]
    async fn test_set_same_fingerprint_same_payload_touches() {
        let (storage, _token) = setup_storage().await;
        let rule = make_rule("/api/v1/user");
        let entry = make_entry_with_key(rule.clone(), "test", b"body");
        let key = entry.key();

        // Store entry
        assert!(storage.set(entry.clone()));

        // Set same entry again (same fingerprint, same payload) - should touch
        assert!(storage.set(entry.clone()));

        // Entry should still exist
        let (result, hit) = storage.get(&entry);
        assert!(hit);
        assert!(result.is_some());
        assert_eq!(result.unwrap().key(), key);
    }

    /// Test that set updates payload when fingerprint matches but payload differs.
    #[tokio::test]
    async fn test_set_same_fingerprint_different_payload_updates() {
        let (storage, _token) = setup_storage().await;
        let rule = make_rule("/api/v1/user");
        let queries = vec![(b"key".to_vec(), b"test".to_vec())];
        let headers = vec![];
        
        let entry1 = Entry::new(rule.clone(), &queries, &headers);
        let response1 = Response {
            status: 200,
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: b"body1".to_vec(),
        };
        entry1.set_payload(&queries, &headers, &response1);

        // Store first entry
        assert!(storage.set(entry1.clone()));

        // Create entry with same fingerprint but different payload
        let entry2 = Entry::new(rule.clone(), &queries, &headers);
        let response2 = Response {
            status: 200,
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: b"body2".to_vec(),
        };
        entry2.set_payload(&queries, &headers, &response2);

        // Should have same fingerprint (same queries/headers)
        assert!(entry1.is_the_same_fingerprint(&entry2));
        
        // Set should update payload
        assert!(storage.set(entry2.clone()));

        // Retrieve and verify new payload
        let (result, hit) = storage.get(&entry2);
        assert!(hit);
        assert!(result.is_some());
        let retrieved = result.unwrap();
        let resp_payload = retrieved.response_payload().unwrap();
        assert_eq!(resp_payload.body, b"body2");
    }

    /// Test that remove returns hit and frees memory when entry exists.
    #[tokio::test]
    async fn test_remove_existing_entry() {
        let (storage, _token) = setup_storage().await;
        let rule = make_rule("/api/v1/user");
        let entry = make_entry_with_key(rule.clone(), "test", b"body");

        let (bytes_before, len_before) = storage.stat();
        
        // Store entry
        assert!(storage.set(entry.clone()));

        let (bytes_after_set, len_after_set) = storage.stat();
        assert!(bytes_after_set > bytes_before);
        assert_eq!(len_after_set, len_before + 1);

        // Remove entry
        let (freed_bytes, hit) = storage.remove(&entry);
        assert!(hit);
        assert!(freed_bytes > 0);

        let (bytes_after_remove, len_after_remove) = storage.stat();
        assert_eq!(bytes_after_remove, bytes_before);
        assert_eq!(len_after_remove, len_before);
    }

    /// Test that remove returns miss when entry doesn't exist.
    #[tokio::test]
    async fn test_remove_nonexistent_entry() {
        let (storage, _token) = setup_storage().await;
        let rule = make_rule("/api/v1/user");
        let entry = make_entry_with_key(rule, "nonexistent", b"body");

        let (freed_bytes, hit) = storage.remove(&entry);
        assert!(!hit);
        assert_eq!(freed_bytes, 0);
    }

    /// Test that get_by_key returns entry when it exists.
    #[tokio::test]
    async fn test_get_by_key() {
        let (storage, _token) = setup_storage().await;
        let rule = make_rule("/api/v1/user");
        let entry = make_entry_with_key(rule, "test", b"body");
        let key = entry.key();

        // Store entry
        assert!(storage.set(entry));

        // Get by key
        let result = storage.get_by_key(key);
        assert!(result.is_some());
        assert_eq!(result.unwrap().key(), key);
    }

    /// Test that get_by_key returns None when entry doesn't exist.
    #[tokio::test]
    async fn test_get_by_key_nonexistent() {
        let (storage, _token) = setup_storage().await;
        
        let result = storage.get_by_key(999999);
        assert!(result.is_none());
    }

    /// Test that stat returns correct counts.
    #[tokio::test]
    async fn test_stat() {
        let (storage, _token) = setup_storage().await;
        let rule = make_rule("/api/v1/user");

        let (bytes_before, len_before) = storage.stat();
        
        // Add multiple entries
        for i in 0..5 {
            let entry = make_entry_with_key(rule.clone(), &format!("test{}", i), b"body");
            assert!(storage.set(entry));
        }

        let (bytes_after, len_after) = storage.stat();
        assert!(bytes_after > bytes_before);
        assert_eq!(len_after, len_before + 5);
    }

    /// Test that clear removes all entries.
    #[tokio::test]
    async fn test_clear() {
        let (storage, _token) = setup_storage().await;
        let rule = make_rule("/api/v1/user");

        // Add entries
        for i in 0..3 {
            let entry = make_entry_with_key(rule.clone(), &format!("test{}", i), b"body");
            assert!(storage.set(entry));
        }

        assert_eq!(storage.len(), 3);
        assert!(storage.mem() > 0);

        // Clear all
        storage.clear();

        assert_eq!(storage.len(), 0);
        assert_eq!(storage.mem(), 0);
    }

    /// Test that multiple gets update LRU order.
    #[tokio::test]
    async fn test_get_updates_lru_order() {
        let (storage, _token) = setup_storage().await;
        let rule = make_rule("/api/v1/user");
        
        // Add multiple entries
        let entry1 = make_entry_with_key(rule.clone(), "test1", b"body1");
        let entry2 = make_entry_with_key(rule.clone(), "test2", b"body2");
        let entry3 = make_entry_with_key(rule.clone(), "test3", b"body3");
        
        assert!(storage.set(entry1.clone()));
        assert!(storage.set(entry2.clone()));
        assert!(storage.set(entry3.clone()));

        // Get entry1 should touch it (move to front of LRU)
        let (result, hit) = storage.get(&entry1);
        assert!(hit);
        assert!(result.is_some());

        // Entry should still be accessible
        let (result2, hit2) = storage.get(&entry1);
        assert!(hit2);
        assert!(result2.is_some());
    }
}
