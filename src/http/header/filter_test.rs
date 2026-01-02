#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::config::{Rule, RuleKey, RuleValue};
    use crate::http::header::filter_and_sort_request;

    fn make_rule_with_header_keys(keys: Vec<&str>) -> Rule {
        let mut headers_map = HashMap::new();
        for key in &keys {
            headers_map.insert(key.to_string(), vec![]);
        }

        Rule {
            path: None,
            path_bytes: None,
            cache_key: RuleKey {
                query: None,
                query_bytes: None,
                headers: Some(keys.into_iter().map(|s| s.to_string()).collect()),
                headers_map: Some(headers_map),
            },
            cache_value: RuleValue {
                headers: None,
                headers_map: None,
            },
            refresh: None,
        }
    }

    /// Test that only whitelisted headers are included.
    #[test]
    fn test_filter_only_whitelisted() {
        let rule = make_rule_with_header_keys(vec!["accept-encoding", "content-type"]);
        let headers = vec![
            ("Accept-Encoding".to_string(), "gzip".to_string()),
            ("Content-Type".to_string(), "application/json".to_string()),
            ("X-Custom".to_string(), "ignored".to_string()),
            ("Authorization".to_string(), "token".to_string()),
        ];

        let result = filter_and_sort_request(Some(&rule), &headers);

        assert_eq!(result.len(), 2);
        // Function preserves original case of header keys from input
        assert!(result.iter().any(|(k, v)| k == b"Accept-Encoding" && v == b"gzip"));
        assert!(result.iter().any(|(k, v)| k == b"Content-Type" && v == b"application/json"));
        assert!(!result.iter().any(|(k, _)| k == b"X-Custom"));
        assert!(!result.iter().any(|(k, _)| k == b"Authorization"));
    }

    /// Test that header matching is case-insensitive.
    #[test]
    fn test_filter_case_insensitive_matching() {
        let rule = make_rule_with_header_keys(vec!["accept-encoding"]);
        let headers = vec![
            ("Accept-Encoding".to_string(), "gzip".to_string()),
            ("ACCEPT-ENCODING".to_string(), "deflate".to_string()),
            ("accept-encoding".to_string(), "br".to_string()),
        ];

        let result = filter_and_sort_request(Some(&rule), &headers);

        // All variations should match and preserve original case
        assert_eq!(result.len(), 3);
        assert!(result.iter().any(|(k, v)| k == b"Accept-Encoding" && v == b"gzip"));
        assert!(result.iter().any(|(k, v)| k == b"ACCEPT-ENCODING" && v == b"deflate"));
        assert!(result.iter().any(|(k, v)| k == b"accept-encoding" && v == b"br"));
    }

    /// Test that filtered results are sorted.
    #[test]
    fn test_filter_sorts_results() {
        let rule = make_rule_with_header_keys(vec!["zebra", "alpha", "middle"]);
        let headers = vec![
            ("Zebra".to_string(), "z".to_string()),
            ("Alpha".to_string(), "a".to_string()),
            ("Middle".to_string(), "m".to_string()),
        ];

        let result = filter_and_sort_request(Some(&rule), &headers);

        assert_eq!(result.len(), 3);
        // Should be sorted lexicographically by key bytes (preserving original case)
        // 'A' < 'M' < 'Z' in ASCII
        assert_eq!(result[0].0, b"Alpha");
        assert_eq!(result[1].0, b"Middle");
        assert_eq!(result[2].0, b"Zebra");
    }

    /// Test that empty rule returns empty result.
    #[test]
    fn test_filter_no_rule() {
        let headers = vec![
            ("Accept-Encoding".to_string(), "gzip".to_string()),
        ];

        let result = filter_and_sort_request(None, &headers);

        assert_eq!(result.len(), 0);
    }

    /// Test that empty whitelist returns empty result.
    #[test]
    fn test_filter_empty_whitelist() {
        let rule = make_rule_with_header_keys(vec![]);
        let headers = vec![
            ("Accept-Encoding".to_string(), "gzip".to_string()),
        ];

        let result = filter_and_sort_request(Some(&rule), &headers);

        assert_eq!(result.len(), 0);
    }

    /// Test that header values preserve case and keys preserve original case.
    #[test]
    fn test_filter_preserves_value_case() {
        let rule = make_rule_with_header_keys(vec!["accept-encoding"]);
        let headers = vec![
            ("Accept-Encoding".to_string(), "gZip, DeFlaTe".to_string()),
        ];

        let result = filter_and_sort_request(Some(&rule), &headers);

        assert_eq!(result.len(), 1);
        // Keys preserve original case from input
        assert_eq!(result[0].0, b"Accept-Encoding");
        assert_eq!(result[0].1, b"gZip, DeFlaTe");
    }

    /// Test that duplicate headers are all included.
    #[test]
    fn test_filter_duplicate_headers() {
        let rule = make_rule_with_header_keys(vec!["accept-encoding"]);
        let headers = vec![
            ("Accept-Encoding".to_string(), "gzip".to_string()),
            ("Accept-Encoding".to_string(), "deflate".to_string()),
        ];

        let result = filter_and_sort_request(Some(&rule), &headers);

        // Both should be included with original case preserved
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|(k, _)| k == b"Accept-Encoding"));
        assert!(result.iter().any(|(_, v)| v == b"gzip"));
        assert!(result.iter().any(|(_, v)| v == b"deflate"));
    }

    /// Test that empty header values are included.
    #[test]
    fn test_filter_empty_values() {
        let rule = make_rule_with_header_keys(vec!["x-custom", "x-empty"]);
        let headers = vec![
            ("X-Custom".to_string(), "value".to_string()),
            ("X-Empty".to_string(), "".to_string()),
        ];

        let result = filter_and_sort_request(Some(&rule), &headers);

        assert_eq!(result.len(), 2);
        // After sorting, keys are sorted lexicographically by bytes (A < X in ASCII)
        // So order is: "X-Custom", "X-Empty" (both start with 'X')
        assert_eq!(result[0].0, b"X-Custom");
        assert_eq!(result[0].1, b"value");
        assert_eq!(result[1].0, b"X-Empty");
        assert_eq!(result[1].1, b"");
    }

    /// Test that single entry doesn't require sorting.
    #[test]
    fn test_filter_single_entry_no_sort() {
        let rule = make_rule_with_header_keys(vec!["accept-encoding"]);
        let headers = vec![
            ("Accept-Encoding".to_string(), "gzip".to_string()),
        ];

        let result = filter_and_sort_request(Some(&rule), &headers);

        assert_eq!(result.len(), 1);
        // Key preserves original case from input
        assert_eq!(result[0].0, b"Accept-Encoding");
        assert_eq!(result[0].1, b"gzip");
    }

    /// Test that special characters in header values are preserved.
    #[test]
    fn test_filter_special_characters() {
        let rule = make_rule_with_header_keys(vec!["x-custom"]);
        let headers = vec![
            ("X-Custom".to_string(), "value\nwith\tspecial\rchars".to_string()),
        ];

        let result = filter_and_sort_request(Some(&rule), &headers);

        assert_eq!(result.len(), 1);
        // Key preserves original case from input
        assert_eq!(result[0].0, b"X-Custom");
        assert_eq!(result[0].1, b"value\nwith\tspecial\rchars");
    }
}
