#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderName, HeaderValue};

    const X_FORWARDED_HOST: &str = "x-forwarded-host";

    /// Test_proxyForwardedHost_PrefersXFH_PassesAsIs
    /// Intentionally leaves spaces/commas — should pass as is.
    #[test]
    fn test_proxy_forwarded_host_prefers_xfh_passes_as_is() {
        let mut in_headers = HeaderMap::new();
        let val = b"  a.example , b.example  ";
        if let Ok(name) = HeaderName::try_from(X_FORWARDED_HOST.as_bytes()) {
            in_headers.insert(name, HeaderValue::from_bytes(val).unwrap());
        }
        in_headers.insert("host", HeaderValue::from_static("ignored.example"));

        let mut out_headers = HeaderMap::new();
        crate::upstream::proxy::proxy_forwarded_host(&mut out_headers, &in_headers);

        let got = out_headers.get("host").map(|v| v.as_bytes()).unwrap_or(&[]);
        assert_eq!(got, val, "out.Host should match X-Forwarded-Host exactly");
    }

    /// Test_proxyForwardedHost_FallbackToHost_WhenXFHEmpty
    #[test]
    fn test_proxy_forwarded_host_fallback_to_host_when_xfh_empty() {
        let mut in_headers = HeaderMap::new();
        if let Ok(name) = HeaderName::try_from(X_FORWARDED_HOST.as_bytes()) {
            in_headers.insert(name, HeaderValue::from_static("")); // empty
        }
        in_headers.insert("host", HeaderValue::from_static("fallback.example:443"));

        let mut out_headers = HeaderMap::new();
        crate::upstream::proxy::proxy_forwarded_host(&mut out_headers, &in_headers);

        let got = out_headers
            .get("host")
            .map(|v| v.to_str().unwrap_or(""))
            .unwrap_or("");
        assert_eq!(
            got, "fallback.example:443",
            "should fallback to Host header"
        );
    }

    /// Test_proxyForwardedHost_FallbackToHost_WhenXFHAbsent
    #[test]
    fn test_proxy_forwarded_host_fallback_to_host_when_xfh_absent() {
        let mut in_headers = HeaderMap::new();
        // XFH not set at all
        in_headers.insert("host", HeaderValue::from_static("[2001:db8::1]:8443"));

        let mut out_headers = HeaderMap::new();
        crate::upstream::proxy::proxy_forwarded_host(&mut out_headers, &in_headers);

        let got = out_headers
            .get("host")
            .map(|v| v.to_str().unwrap_or(""))
            .unwrap_or("");
        assert_eq!(
            got, "[2001:db8::1]:8443",
            "should use Host header when XFH absent"
        );
    }

    /// Test_proxyForwardedHost_NoSources_NoPanic_NoChange
    #[test]
    fn test_proxy_forwarded_host_no_sources_no_panic_no_change() {
        let in_headers = HeaderMap::new(); // empty headers
        let mut out_headers = HeaderMap::new();
        out_headers.insert("host", HeaderValue::from_static("pre.set")); // should not change

        crate::upstream::proxy::proxy_forwarded_host(&mut out_headers, &in_headers);

        let got = out_headers
            .get("host")
            .map(|v| v.to_str().unwrap_or(""))
            .unwrap_or("");
        assert_eq!(
            got, "pre.set",
            "out.Host should remain unchanged when no sources"
        );
    }

    /// Test_proxyForwardedHost_InternalCopy_NoAliasing
    #[test]
    fn test_proxy_forwarded_host_internal_copy_no_aliasing() {
        let mut in_headers = HeaderMap::new();
        if let Ok(name) = HeaderName::try_from(X_FORWARDED_HOST.as_bytes()) {
            in_headers.insert(name, HeaderValue::from_static("alpha.example"));
        }

        let mut out_headers = HeaderMap::new();
        crate::upstream::proxy::proxy_forwarded_host(&mut out_headers, &in_headers);

        // Mutate the original header after proxying
        if let Ok(name) = HeaderName::try_from(X_FORWARDED_HOST.as_bytes()) {
            in_headers.insert(name, HeaderValue::from_static("beta.example"));
        }

        let got = out_headers
            .get("host")
            .map(|v| v.to_str().unwrap_or(""))
            .unwrap_or("");
        assert_eq!(
            got, "alpha.example",
            "out.Host should not change after source mutation (copied)"
        );
    }

    /// Test_proxyForwardedHost_LeavesOutHostIfBothHeadersEmpty
    #[test]
    fn test_proxy_forwarded_host_leaves_out_host_if_both_headers_empty() {
        let in_headers = HeaderMap::new(); // Neither XFH nor Host set
        let mut out_headers = HeaderMap::new();

        // For control, set a preliminary value
        out_headers.insert("host", HeaderValue::from_static("pre.host"));

        crate::upstream::proxy::proxy_forwarded_host(&mut out_headers, &in_headers);

        let got = out_headers
            .get("host")
            .map(|v| v.to_str().unwrap_or(""))
            .unwrap_or("");
        assert_eq!(
            got, "pre.host",
            "out.Host should keep pre.host (no sources -> no change)"
        );
    }

    /// Test: Prefers X-Forwarded-Host over Host when both are present.
    #[test]
    fn test_forwarded_host_value_bytes_prefers_xfh() {
        let headers: Vec<(Vec<u8>, Vec<u8>)> = vec![
            (b"Host".to_vec(), b"ignored.example.com".to_vec()),
            (b"x-forwarded-host".to_vec(), b"forwarded.example.com:8080".to_vec()),
            (b"Content-Type".to_vec(), b"application/json".to_vec()),
        ];

        let result = crate::upstream::proxy::forwarded_host_value_bytes(&headers);
        assert_eq!(
            result,
            Some(b"forwarded.example.com:8080".as_slice()),
            "Should prefer X-Forwarded-Host over Host"
        );
    }

    /// Test: Falls back to Host when X-Forwarded-Host is absent.
    #[test]
    fn test_forwarded_host_value_bytes_fallback_to_host() {
        let headers: Vec<(Vec<u8>, Vec<u8>)> = vec![
            (b"Host".to_vec(), b"fallback.example.com:9090".to_vec()),
            (b"Content-Type".to_vec(), b"application/json".to_vec()),
        ];

        let result = crate::upstream::proxy::forwarded_host_value_bytes(&headers);
        assert_eq!(
            result,
            Some(b"fallback.example.com:9090".as_slice()),
            "Should fallback to Host when X-Forwarded-Host absent"
        );
    }

    /// Test: Returns None when both X-Forwarded-Host and Host are absent or empty.
    #[test]
    fn test_forwarded_host_value_bytes_no_change() {
        // Test 1: No headers
        let headers1: Vec<(Vec<u8>, Vec<u8>)> = vec![
            (b"Content-Type".to_vec(), b"application/json".to_vec()),
        ];
        let result1 = crate::upstream::proxy::forwarded_host_value_bytes(&headers1);
        assert_eq!(result1, None, "Should return None when no Host/XFH headers");

        // Test 2: Empty XFH
        let headers2: Vec<(Vec<u8>, Vec<u8>)> = vec![
            (b"x-forwarded-host".to_vec(), b"".to_vec()),
            (b"Content-Type".to_vec(), b"application/json".to_vec()),
        ];
        let result2 = crate::upstream::proxy::forwarded_host_value_bytes(&headers2);
        assert_eq!(result2, None, "Should return None when XFH is empty");

        // Test 3: Empty Host
        let headers3: Vec<(Vec<u8>, Vec<u8>)> = vec![
            (b"Host".to_vec(), b"".to_vec()),
            (b"Content-Type".to_vec(), b"application/json".to_vec()),
        ];
        let result3 = crate::upstream::proxy::forwarded_host_value_bytes(&headers3);
        assert_eq!(result3, None, "Should return None when Host is empty");
    }
}
