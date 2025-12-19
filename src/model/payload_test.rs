#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::config::{Rule, RuleKey, RuleValue};
    use crate::model::{Entry, Response};

    fn make_rule() -> Arc<Rule> {
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

    fn make_response(status: u16, body: &[u8]) -> Response {
        Response {
            status,
            headers: vec![
                ("Content-Type".to_string(), "application/json".to_string()),
                ("Content-Length".to_string(), body.len().to_string()),
            ],
            body: body.to_vec(),
        }
    }

    /// Test that set_payload stores payload correctly.
    #[test]
    fn test_set_payload_stores_data() {
        let rule = make_rule();
        let queries = vec![(b"user[id]".to_vec(), b"123".to_vec())];
        let headers = vec![];
        let entry = Entry::new(rule, &queries, &headers);

        let response = make_response(200, b"test body");
        entry.set_payload(&queries, &headers, &response);

        // Verify payload can be retrieved
        let payload_bytes = entry.payload_bytes();
        assert!(!payload_bytes.is_empty());

        // Verify we can decode the payload
        let req_payload = entry.request_payload().unwrap();
        assert_eq!(req_payload.queries.len(), 1);
        assert_eq!(req_payload.queries[0].0, b"user[id]");
        assert_eq!(req_payload.queries[0].1, b"123");

        let resp_payload = entry.response_payload().unwrap();
        assert_eq!(resp_payload.code, 200);
        assert_eq!(resp_payload.body, b"test body");
    }

    /// Test that is_the_same_payload correctly identifies identical payloads.
    #[test]
    fn test_is_the_same_payload_identical() {
        let rule = make_rule();
        let queries = vec![(b"user[id]".to_vec(), b"123".to_vec())];
        let headers = vec![];
        let entry1 = Entry::new(rule.clone(), &queries, &headers);
        let entry2 = Entry::new(rule, &queries, &headers);

        let response = make_response(200, b"same body");
        entry1.set_payload(&queries, &headers, &response);
        entry2.set_payload(&queries, &headers, &response);

        assert!(entry1.is_the_same_payload(&entry2));
    }

    /// Test that is_the_same_payload correctly identifies different payloads.
    #[test]
    fn test_is_the_same_payload_different() {
        let rule = make_rule();
        let queries = vec![(b"user[id]".to_vec(), b"123".to_vec())];
        let headers = vec![];
        let entry1 = Entry::new(rule.clone(), &queries, &headers);
        let entry2 = Entry::new(rule, &queries, &headers);

        entry1.set_payload(&queries, &headers, &make_response(200, b"body1"));
        entry2.set_payload(&queries, &headers, &make_response(200, b"body2"));

        assert!(!entry1.is_the_same_payload(&entry2));
    }

    /// Test that is_the_same_payload handles empty payloads.
    #[test]
    fn test_is_the_same_payload_empty() {
        let rule = make_rule();
        let queries = vec![];
        let headers = vec![];
        let entry1 = Entry::new(rule.clone(), &queries, &headers);
        let entry2 = Entry::new(rule.clone(), &queries, &headers);

        // Both have empty payloads
        assert!(entry1.is_the_same_payload(&entry2));

        // One has payload, other doesn't
        let entry3 = Entry::new(rule.clone(), &queries, &headers);
        entry3.set_payload(&queries, &headers, &make_response(200, b"body"));
        assert!(!entry1.is_the_same_payload(&entry3));
    }

    /// Test that swap_payloads correctly swaps payloads between entries.
    #[test]
    fn test_swap_payloads() {
        let rule = make_rule();
        let queries = vec![(b"user[id]".to_vec(), b"123".to_vec())];
        let headers = vec![];
        let entry1 = Entry::new(rule.clone(), &queries, &headers);
        let mut entry2 = Entry::new(rule, &queries, &headers);

        entry1.set_payload(&queries, &headers, &make_response(200, b"body1"));
        entry2.set_payload(&queries, &headers, &make_response(200, b"body2"));

        let body1_before = entry1.response_payload().unwrap().body.clone();
        let body2_before = entry2.response_payload().unwrap().body.clone();

        entry1.swap_payloads(&mut entry2);

        let body1_after = entry1.response_payload().unwrap().body;
        let body2_after = entry2.response_payload().unwrap().body;

        assert_eq!(body1_after, body2_before);
        assert_eq!(body2_after, body1_before);
    }

    /// Test that swap_payloads returns correct weight difference.
    #[test]
    fn test_swap_payloads_weight_difference() {
        let rule = make_rule();
        let queries = vec![];
        let headers = vec![];
        let entry1 = Entry::new(rule.clone(), &queries, &headers);
        let mut entry2 = Entry::new(rule, &queries, &headers);

        // Create different sized payloads
        entry1.set_payload(&queries, &headers, &make_response(200, &vec![b'a'; 100]));
        entry2.set_payload(&queries, &headers, &make_response(200, &vec![b'b'; 200]));

        let weight1_before = entry1.weight();
        let weight2_before = entry2.weight();

        let weight_diff = entry1.swap_payloads(&mut entry2);

        let weight1_after = entry1.weight();
        let weight2_after = entry2.weight();

        // Weight difference should reflect the swap
        let expected_diff = weight2_before - weight1_before;
        assert_eq!(weight_diff, expected_diff);

        // Weights should be swapped
        assert_eq!(weight1_after, weight2_before);
        assert_eq!(weight2_after, weight1_before);
    }

    /// Test that weight calculation includes payload size.
    #[test]
    fn test_weight_includes_payload() {
        let rule = make_rule();
        let queries = vec![];
        let headers = vec![];
        let entry_empty = Entry::new(rule.clone(), &queries, &headers);
        let entry_with_payload = Entry::new(rule, &queries, &headers);

        let weight_empty = entry_empty.weight();

        entry_with_payload.set_payload(&queries, &headers, &make_response(200, &vec![b'a'; 1000]));
        let weight_with_payload = entry_with_payload.weight();

        assert!(weight_with_payload > weight_empty);
        assert!(weight_with_payload >= weight_empty + 1000);
    }

    /// Test payload with large body.
    #[test]
    fn test_set_payload_large_body() {
        let rule = make_rule();
        let queries = vec![];
        let headers = vec![];
        let entry = Entry::new(rule, &queries, &headers);

        let large_body = vec![b'x'; 100_000];
        let response = make_response(200, &large_body);
        entry.set_payload(&queries, &headers, &response);

        let resp_payload = entry.response_payload().unwrap();
        assert_eq!(resp_payload.body.len(), 100_000);
        assert_eq!(resp_payload.body, large_body);
    }

    /// Test payload with multiple query parameters and headers.
    #[test]
    fn test_set_payload_multiple_components() {
        let rule = make_rule();
        let queries = vec![
            (b"user[id]".to_vec(), b"123".to_vec()),
            (b"domain".to_vec(), b"example.com".to_vec()),
        ];
        let headers = vec![
            (b"accept-encoding".to_vec(), b"gzip".to_vec()),
            (b"x-custom".to_vec(), b"value".to_vec()),
        ];
        let entry = Entry::new(rule, &queries, &headers);

        let response = Response {
            status: 200,
            headers: vec![
                ("Content-Type".to_string(), "application/json".to_string()),
                ("X-Custom-Resp".to_string(), "resp-value".to_string()),
            ],
            body: b"response body".to_vec(),
        };
        entry.set_payload(&queries, &headers, &response);

        let req_payload = entry.request_payload().unwrap();
        assert_eq!(req_payload.queries.len(), 2);
        assert_eq!(req_payload.headers.len(), 2);

        let resp_payload = entry.response_payload().unwrap();
        assert_eq!(resp_payload.code, 200);
        assert_eq!(resp_payload.headers.len(), 2);
        assert_eq!(resp_payload.body, b"response body");
    }

    /// Test that payload_bytes returns empty for unset payload.
    #[test]
    fn test_payload_bytes_unset() {
        let rule = make_rule();
        let queries = vec![];
        let headers = vec![];
        let entry = Entry::new(rule, &queries, &headers);

        assert_eq!(entry.payload_bytes().len(), 0);
    }

    /// Test that payload_bytes returns correct data after set_payload.
    #[test]
    fn test_payload_bytes_after_set() {
        let rule = make_rule();
        let queries = vec![(b"user[id]".to_vec(), b"123".to_vec())];
        let headers = vec![];
        let entry = Entry::new(rule, &queries, &headers);

        let response = make_response(200, b"test");
        entry.set_payload(&queries, &headers, &response);

        let payload_bytes = entry.payload_bytes();
        assert!(!payload_bytes.is_empty());

        // Verify we can decode it back
        let decoded = entry.response_payload().unwrap();
        assert_eq!(decoded.body, b"test");
    }
}
