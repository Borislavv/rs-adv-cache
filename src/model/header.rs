//! Header filtering functionality.
//

use super::Entry;

impl Entry {
    /// Gets filtered and sorted key headers from an axum request.
    #[allow(dead_code)]
    pub fn get_filtered_and_sorted_key_headers(
        &self,
        request_headers: &[(String, String)],
    ) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut out = Vec::new();

        if let Some(ref headers_map) = self.0.rule.cache_key.headers_map {
            for (key, key_bytes) in headers_map {
                // Find matching header value
                if let Some((_, value)) = request_headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(key))
                {
                    out.push((key_bytes.clone(), value.as_bytes().to_vec()));
                }
            }
        }

        if out.len() > 1 {
            crate::sort::key_value::kv_slice(&mut out);
        }

        out
    }

    /// Gets filtered and sorted key headers from raw headers.
    #[allow(dead_code)]
    pub fn get_filtered_and_sorted_key_headers_raw(
        &self,
        headers: &[(Vec<u8>, Vec<u8>)],
    ) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut out = Vec::new();

        if let Some(ref headers_map) = self.0.rule.cache_key.headers_map {
            for (key, key_bytes) in headers_map {
                // Find matching header value (case-insensitive)
                let key_str = key.clone();
                if let Some((_, value)) = headers
                    .iter()
                    .find(|(k, _)| String::from_utf8_lossy(k).eq_ignore_ascii_case(&key_str))
                {
                    out.push((key_bytes.clone(), value.clone()));
                }
            }
        }

        if out.len() > 1 {
            crate::sort::key_value::kv_slice(&mut out);
        }

        out
    }
}
