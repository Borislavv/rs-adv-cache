#[cfg(test)]
mod tests {
    use crate::config::{Rule, RuleKey, RuleValue};
    use crate::http::query::filter_and_sort_request;

    fn make_rule_with_query_keys(keys: Vec<&str>) -> Rule {
        let query_bytes: Vec<Vec<u8>> = keys.iter().map(|k| k.as_bytes().to_vec()).collect();
        Rule {
            path: None,
            path_bytes: None,
            cache_key: RuleKey {
                query: Some(keys.into_iter().map(|s| s.to_string()).collect()),
                query_bytes: Some(query_bytes),
                headers: None,
                headers_map: None,
            },
            cache_value: RuleValue {
                headers: None,
                headers_map: None,
            },
            refresh: None,
        }
    }

    /// Test that only whitelisted query parameters are included.
    #[test]
    fn test_filter_only_whitelisted() {
        let rule = make_rule_with_query_keys(vec!["user[id]", "domain"]);
        let query_str = "user[id]=123&domain=example.com&ignored=value&another=param";

        let result = filter_and_sort_request(Some(&rule), query_str.as_bytes());

        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|(k, v)| k == b"user[id]" && v == b"123"));
        assert!(result.iter().any(|(k, v)| k == b"domain" && v == b"example.com"));
        assert!(!result.iter().any(|(k, _)| k == b"ignored"));
        assert!(!result.iter().any(|(k, _)| k == b"another"));
    }

    /// Test that filtered results are sorted.
    #[test]
    fn test_filter_sorts_results() {
        let rule = make_rule_with_query_keys(vec!["zebra", "alpha", "middle"]);
        let query_str = "zebra=z&alpha=a&middle=m";

        let result = filter_and_sort_request(Some(&rule), query_str.as_bytes());

        assert_eq!(result.len(), 3);
        // Should be sorted lexicographically by key
        assert_eq!(result[0].0, b"alpha");
        assert_eq!(result[1].0, b"middle");
        assert_eq!(result[2].0, b"zebra");
    }

    /// Test that empty rule returns empty result.
    #[test]
    fn test_filter_no_rule() {
        let query_str = "user[id]=123&domain=example.com";

        let result = filter_and_sort_request(None, query_str.as_bytes());

        assert_eq!(result.len(), 0);
    }

    /// Test that empty whitelist returns empty result.
    #[test]
    fn test_filter_empty_whitelist() {
        let rule = make_rule_with_query_keys(vec![]);
        let query_str = "user[id]=123&domain=example.com";

        let result = filter_and_sort_request(Some(&rule), query_str.as_bytes());

        assert_eq!(result.len(), 0);
    }

    /// Test that query string without '?' prefix works.
    #[test]
    fn test_filter_no_question_mark() {
        let rule = make_rule_with_query_keys(vec!["user[id]"]);
        let query_str = "user[id]=123"; // No leading '?'

        let result = filter_and_sort_request(Some(&rule), query_str.as_bytes());

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, b"user[id]");
        assert_eq!(result[0].1, b"123");
    }

    /// Test that query string with '?' prefix works.
    #[test]
    fn test_filter_with_question_mark() {
        let rule = make_rule_with_query_keys(vec!["user[id]"]);
        let query_str = "?user[id]=123"; // With leading '?'

        let result = filter_and_sort_request(Some(&rule), query_str.as_bytes());

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, b"user[id]");
        assert_eq!(result[0].1, b"123");
    }

    /// Test URL-encoded values are properly decoded.
    #[test]
    fn test_filter_url_encoded_values() {
        let rule = make_rule_with_query_keys(vec!["domain"]);
        let query_str = "domain=example%2Ecom%20with%20spaces";

        let result = filter_and_sort_request(Some(&rule), query_str.as_bytes());

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, b"domain");
        // Values should be URL-decoded
        assert_eq!(result[0].1, b"example.com with spaces");
    }

    /// Test that duplicate keys are all included (not deduplicated).
    #[test]
    fn test_filter_duplicate_keys() {
        let rule = make_rule_with_query_keys(vec!["user[id]"]);
        let query_str = "user[id]=123&user[id]=456";

        let result = filter_and_sort_request(Some(&rule), query_str.as_bytes());

        // url::form_urlencoded::parse returns all pairs, including duplicates
        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|(k, v)| k == b"user[id]" && v == b"123"));
        assert!(result.iter().any(|(k, v)| k == b"user[id]" && v == b"456"));
    }

    /// Test that empty values are included.
    #[test]
    fn test_filter_empty_values() {
        let rule = make_rule_with_query_keys(vec!["user[id]", "domain"]);
        let query_str = "user[id]=&domain=example.com";

        let result = filter_and_sort_request(Some(&rule), query_str.as_bytes());

        assert_eq!(result.len(), 2);
        let id_entry = result.iter().find(|(k, _)| k == b"user[id]").unwrap();
        assert_eq!(id_entry.1, b"");
        let domain_entry = result.iter().find(|(k, _)| k == b"domain").unwrap();
        assert_eq!(domain_entry.1, b"example.com");
    }

    /// Test case-sensitive key matching.
    #[test]
    fn test_filter_case_sensitive_keys() {
        let rule = make_rule_with_query_keys(vec!["user[id]", "Domain"]);
        let query_str = "user[id]=123&Domain=example.com&domain=ignored";

        let result = filter_and_sort_request(Some(&rule), query_str.as_bytes());

        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|(k, v)| k == b"user[id]" && v == b"123"));
        assert!(result.iter().any(|(k, v)| k == b"Domain" && v == b"example.com"));
        // "domain" (lowercase) should not match "Domain"
        assert!(!result.iter().any(|(k, _)| k == b"domain"));
    }

    /// Test that single entry doesn't require sorting.
    #[test]
    fn test_filter_single_entry_no_sort() {
        let rule = make_rule_with_query_keys(vec!["user[id]"]);
        let query_str = "user[id]=123";

        let result = filter_and_sort_request(Some(&rule), query_str.as_bytes());

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, b"user[id]");
        assert_eq!(result[0].1, b"123");
    }
}
