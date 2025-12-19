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

    fn make_entry_with_payload(
        queries: &[(Vec<u8>, Vec<u8>)],
        headers: &[(Vec<u8>, Vec<u8>)],
        status: u16,
        body: &[u8],
    ) -> Entry {
        let rule = make_rule();
        let entry = Entry::new(rule, queries, headers);
        let response = Response {
            status,
            headers: vec![
                ("Content-Type".to_string(), "application/json".to_string()),
                ("Content-Length".to_string(), body.len().to_string()),
            ],
            body: body.to_vec(),
        };
        entry.set_payload(queries, headers, &response);
        entry
    }

    /// Test that encoding and decoding preserves queries.
    #[test]
    fn test_encode_decode_queries() {
        let queries = vec![
            (b"user[id]".to_vec(), b"123".to_vec()),
            (b"domain".to_vec(), b"example.com".to_vec()),
        ];
        let headers = vec![];
        let entry = make_entry_with_payload(&queries, &headers, 200, b"body");

        let req_payload = entry.request_payload().unwrap();
        assert_eq!(req_payload.queries.len(), 2);
        assert_eq!(req_payload.queries[0].0, b"user[id]");
        assert_eq!(req_payload.queries[0].1, b"123");
        assert_eq!(req_payload.queries[1].0, b"domain");
        assert_eq!(req_payload.queries[1].1, b"example.com");
    }

    /// Test that encoding and decoding preserves request headers.
    #[test]
    fn test_encode_decode_request_headers() {
        let queries = vec![];
        let headers = vec![
            (b"accept-encoding".to_vec(), b"gzip".to_vec()),
            (b"x-custom".to_vec(), b"value".to_vec()),
        ];
        let entry = make_entry_with_payload(&queries, &headers, 200, b"body");

        let req_payload = entry.request_payload().unwrap();
        assert_eq!(req_payload.headers.len(), 2);
        assert!(req_payload.headers.iter().any(|(k, v)| k == b"accept-encoding" && v == b"gzip"));
        assert!(req_payload.headers.iter().any(|(k, v)| k == b"x-custom" && v == b"value"));
    }

    /// Test that encoding and decoding preserves response status code.
    #[test]
    fn test_encode_decode_status_code() {
        let queries = vec![];
        let headers = vec![];
        
        for status in &[200, 201, 404, 500, 503] {
            let entry = make_entry_with_payload(&queries, &headers, *status, b"body");
            let resp_payload = entry.response_payload().unwrap();
            assert_eq!(resp_payload.code, *status);
        }
    }

    /// Test that encoding and decoding preserves response body.
    #[test]
    fn test_encode_decode_response_body() {
        let queries = vec![];
        let headers = vec![];
        let body = b"test response body";
        let entry = make_entry_with_payload(&queries, &headers, 200, body);

        let resp_payload = entry.response_payload().unwrap();
        assert_eq!(resp_payload.body, body);
    }

    /// Test that encoding and decoding preserves response headers.
    #[test]
    fn test_encode_decode_response_headers() {
        let queries = vec![];
        let headers = vec![];
        let rule = make_rule();
        let entry = Entry::new(rule, &queries, &headers);
        
        let response = Response {
            status: 200,
            headers: vec![
                ("Content-Type".to_string(), "application/json".to_string()),
                ("Cache-Control".to_string(), "max-age=3600".to_string()),
                ("X-Custom-Resp".to_string(), "resp-value".to_string()),
            ],
            body: b"body".to_vec(),
        };
        entry.set_payload(&queries, &headers, &response);

        let resp_payload = entry.response_payload().unwrap();
        assert_eq!(resp_payload.headers.len(), 3);
        assert!(resp_payload.headers.iter().any(|(k, v)| 
            k == b"Content-Type" && v == b"application/json"
        ));
        assert!(resp_payload.headers.iter().any(|(k, v)| 
            k == b"Cache-Control" && v == b"max-age=3600"
        ));
        assert!(resp_payload.headers.iter().any(|(k, v)| 
            k == b"X-Custom-Resp" && v == b"resp-value"
        ));
    }

    /// Test that full payload round-trip preserves all data.
    #[test]
    fn test_full_payload_round_trip() {
        let queries = vec![
            (b"user[id]".to_vec(), b"123".to_vec()),
            (b"domain".to_vec(), b"example.com".to_vec()),
        ];
        let headers = vec![
            (b"accept-encoding".to_vec(), b"gzip, deflate".to_vec()),
        ];
        let response_headers = vec![
            ("Content-Type".to_string(), "application/json".to_string()),
            ("Vary".to_string(), "Accept-Encoding".to_string()),
        ];
        let body = b"{\"data\":\"test\"}";

        let rule = make_rule();
        let entry = Entry::new(rule, &queries, &headers);
        let response = Response {
            status: 200,
            headers: response_headers,
            body: body.to_vec(),
        };
        entry.set_payload(&queries, &headers, &response);

        // Decode full payload
        let payload = entry.payload().unwrap();
        
        // Verify queries
        assert_eq!(payload.queries.len(), 2);
        assert_eq!(payload.queries, queries);
        
        // Verify request headers
        assert_eq!(payload.req_headers.len(), 1);
        assert_eq!(payload.req_headers, headers);
        
        // Verify response
        assert_eq!(payload.code, 200);
        assert_eq!(payload.body, body);
        assert_eq!(payload.rsp_headers.len(), 2);
    }

    /// Test encoding with empty components.
    #[test]
    fn test_encode_decode_empty_components() {
        let queries: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let headers: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let entry = make_entry_with_payload(&queries, &headers, 204, b"");

        let req_payload = entry.request_payload().unwrap();
        assert_eq!(req_payload.queries.len(), 0);
        assert_eq!(req_payload.headers.len(), 0);

        let resp_payload = entry.response_payload().unwrap();
        assert_eq!(resp_payload.code, 204);
        assert_eq!(resp_payload.body.len(), 0);
    }

    /// Test encoding with large payload.
    #[test]
    fn test_encode_decode_large_payload() {
        let queries = vec![(b"key".to_vec(), b"value".to_vec())];
        let headers = vec![];
        let large_body = vec![b'x'; 100_000];
        let entry = make_entry_with_payload(&queries, &headers, 200, &large_body);

        let resp_payload = entry.response_payload().unwrap();
        assert_eq!(resp_payload.body.len(), 100_000);
        assert_eq!(resp_payload.body, large_body);
    }

    /// Test encoding with special characters in values.
    #[test]
    fn test_encode_decode_special_characters() {
        let queries = vec![
            (b"key".to_vec(), b"value%20with%2Fspaces".to_vec()),
        ];
        let headers = vec![
            (b"x-header".to_vec(), b"value\nwith\tspecial\rchars".to_vec()),
        ];
        let body = b"body with\nnewlines\tand\ttabs";
        let entry = make_entry_with_payload(&queries, &headers, 200, body);

        let req_payload = entry.request_payload().unwrap();
        assert_eq!(req_payload.queries[0].1, b"value%20with%2Fspaces");
        assert_eq!(req_payload.headers[0].1, b"value\nwith\tspecial\rchars");

        let resp_payload = entry.response_payload().unwrap();
        assert_eq!(resp_payload.body, body);
    }

    /// Test that payload() returns full payload structure.
    #[test]
    fn test_payload_returns_full_structure() {
        let queries = vec![(b"user[id]".to_vec(), b"123".to_vec())];
        let headers = vec![(b"accept".to_vec(), b"application/json".to_vec())];
        let entry = make_entry_with_payload(&queries, &headers, 200, b"body");

        let payload = entry.payload().unwrap();
        assert_eq!(payload.queries, queries);
        assert_eq!(payload.req_headers, headers);
        assert_eq!(payload.code, 200);
        assert_eq!(payload.body, b"body");
        assert!(!payload.rsp_headers.is_empty());
    }

    /// Test decoding with malformed payload (empty payload).
    #[test]
    fn test_decode_empty_payload() {
        let rule = make_rule();
        let queries: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let headers: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let entry = Entry::new(rule, &queries, &headers);

        // Entry without payload should return error
        assert!(entry.request_payload().is_err());
        assert!(entry.response_payload().is_err());
        assert!(entry.payload().is_err());
    }
}
