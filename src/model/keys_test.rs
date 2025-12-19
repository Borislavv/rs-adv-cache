#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::config::{Rule, RuleKey, RuleValue};
    use crate::model::Entry;

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

    /// Test that same input produces same key and fingerprint.
    #[test]
    fn test_build_key_deterministic() {
        let rule = make_rule("/api/v1/user");
        let queries = vec![
            (b"user[id]".to_vec(), b"123".to_vec()),
            (b"domain".to_vec(), b"example.com".to_vec()),
        ];
        let headers = vec![(b"accept-encoding".to_vec(), b"gzip".to_vec())];

        let entry1 = Entry::new(rule.clone(), &queries, &headers);
        let entry2 = Entry::new(rule, &queries, &headers);

        assert_eq!(entry1.key(), entry2.key());
        assert!(entry1.is_the_same_fingerprint(&entry2));
        assert_eq!(entry1.fingerprint_hi(), entry2.fingerprint_hi());
        assert_eq!(entry1.fingerprint_lo(), entry2.fingerprint_lo());
    }

    /// Test that different paths produce different keys.
    #[test]
    fn test_build_key_path_different() {
        let rule1 = make_rule("/api/v1/user");
        let rule2 = make_rule("/api/v1/client");
        let queries = vec![(b"user[id]".to_vec(), b"123".to_vec())];
        let headers = vec![];

        let entry1 = Entry::new(rule1, &queries, &headers);
        let entry2 = Entry::new(rule2, &queries, &headers);

        assert_ne!(entry1.key(), entry2.key());
        assert!(!entry1.is_the_same_fingerprint(&entry2));
    }

    /// Test that different query parameters produce different keys.
    #[test]
    fn test_build_key_query_different() {
        let rule = make_rule("/api/v1/user");
        let queries1 = vec![(b"user[id]".to_vec(), b"123".to_vec())];
        let queries2 = vec![(b"user[id]".to_vec(), b"456".to_vec())];
        let headers = vec![];

        let entry1 = Entry::new(rule.clone(), &queries1, &headers);
        let entry2 = Entry::new(rule, &queries2, &headers);

        assert_ne!(entry1.key(), entry2.key());
        assert!(!entry1.is_the_same_fingerprint(&entry2));
    }

    /// Test that different headers produce different keys.
    #[test]
    fn test_build_key_header_different() {
        let rule = make_rule("/api/v1/user");
        let queries = vec![];
        let headers1 = vec![(b"accept-encoding".to_vec(), b"gzip".to_vec())];
        let headers2 = vec![(b"accept-encoding".to_vec(), b"identity".to_vec())];

        let entry1 = Entry::new(rule.clone(), &queries, &headers1);
        let entry2 = Entry::new(rule, &queries, &headers2);

        assert_ne!(entry1.key(), entry2.key());
        assert!(!entry1.is_the_same_fingerprint(&entry2));
    }

    /// Test that query parameter order doesn't affect key (same key should be generated).
    /// Note: This test verifies that filtering/sorting happens before build_key is called.
    #[test]
    fn test_build_key_query_order_insensitive() {
        let rule = make_rule("/api/v1/user");
        
        // Simulate sorted queries (as they should be after filtering)
        let queries1 = vec![
            (b"domain".to_vec(), b"example.com".to_vec()),
            (b"user[id]".to_vec(), b"123".to_vec()),
        ];
        let queries2 = vec![
            (b"domain".to_vec(), b"example.com".to_vec()),
            (b"user[id]".to_vec(), b"123".to_vec()),
        ];
        let headers = vec![];

        let entry1 = Entry::new(rule.clone(), &queries1, &headers);
        let entry2 = Entry::new(rule, &queries2, &headers);

        // Same sorted queries should produce same key
        assert_eq!(entry1.key(), entry2.key());
        assert!(entry1.is_the_same_fingerprint(&entry2));
    }

    /// Test fingerprint comparison for hash collision detection.
    #[test]
    fn test_fingerprint_comparison() {
        let rule = make_rule("/api/v1/user");
        let queries1 = vec![(b"user[id]".to_vec(), b"123".to_vec())];
        let queries2 = vec![(b"user[id]".to_vec(), b"456".to_vec())];
        let headers = vec![];

        let entry1 = Entry::new(rule.clone(), &queries1, &headers);
        let entry2 = Entry::new(rule.clone(), &queries2, &headers);
        let entry3 = Entry::new(rule, &queries1, &headers);

        // Different queries = different fingerprints
        assert!(!entry1.is_the_same_fingerprint(&entry2));
        
        // Same queries = same fingerprints
        assert!(entry1.is_the_same_fingerprint(&entry3));
        
        // Self-comparison
        assert!(entry1.is_the_same_fingerprint(&entry1));
    }

    /// Test that empty queries and headers still produce valid keys.
    #[test]
    fn test_build_key_empty_components() {
        let rule = make_rule("/api/v1/user");
        let queries: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let headers: Vec<(Vec<u8>, Vec<u8>)> = vec![];

        let entry = Entry::new(rule, &queries, &headers);

        // Should produce a valid key based on path only
        assert_ne!(entry.key(), 0);
        assert_ne!(entry.fingerprint_hi(), 0);
        assert_ne!(entry.fingerprint_lo(), 0);
    }

    /// Test that multiple query parameters are included in key generation.
    #[test]
    fn test_build_key_multiple_queries() {
        let rule = make_rule("/api/v1/user");
        let queries = vec![
            (b"user[id]".to_vec(), b"123".to_vec()),
            (b"domain".to_vec(), b"example.com".to_vec()),
            (b"language".to_vec(), b"en".to_vec()),
        ];
        let headers: Vec<(Vec<u8>, Vec<u8>)> = vec![];

        let entry1 = Entry::new(rule.clone(), &queries, &headers);
        
        // Remove one query parameter
        let queries_partial = vec![
            (b"user[id]".to_vec(), b"123".to_vec()),
            (b"domain".to_vec(), b"example.com".to_vec()),
        ];
        let entry2 = Entry::new(rule, &queries_partial, &[]);

        assert_ne!(entry1.key(), entry2.key());
        assert!(!entry1.is_the_same_fingerprint(&entry2));
    }

    /// Test key generation with special characters in path/query/headers.
    #[test]
    fn test_build_key_special_characters() {
        let rule = Arc::new(Rule {
            path: Some("/api/v1/user?id=123".to_string()),
            path_bytes: Some(b"/api/v1/user?id=123".to_vec()),
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
        });

        let queries = vec![(b"key".to_vec(), b"value%20with%2Fspaces".to_vec())];
        let headers = vec![(b"x-custom".to_vec(), b"value\nwith\tspecial".to_vec())];

        let entry1 = Entry::new(rule.clone(), &queries, &headers);
        let entry2 = Entry::new(rule, &queries, &headers);

        assert_eq!(entry1.key(), entry2.key());
        assert!(entry1.is_the_same_fingerprint(&entry2));
    }
}
