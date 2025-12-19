//! Query parameter parsing and filtering.
//

use byteorder::{ByteOrder, LittleEndian};

use super::Entry;

/// Error types for query parsing.
#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    #[error("malformed or nil payload")]
    MalformedOrNilPayload,
    #[error("corrupted queries section")]
    CorruptedQueriesSection,
}

impl Entry {
    /// Parses, filters, and sorts query parameters from a byte slice.
    #[allow(dead_code)]
    pub fn parse_filter_and_sort_query(&self, query_str: &str) -> Vec<(Vec<u8>, Vec<u8>)> {
        // Remove leading '?'
        let query_str = query_str.trim_start_matches('?');
        if query_str.is_empty() {
            return Vec::new();
        }

        let mut out = Vec::new();
        let mut state = QueryState::default();

        let bytes = query_str.as_bytes();
        for (idx, &bt) in bytes.iter().enumerate() {
            if bt == b'&' {
                if state.k_found {
                    let (key, val) = self.extract_kv(bytes, &state, idx);
                    out.push((key, val));
                    state.reset_for_next(idx + 1);
                }
            } else if bt == b'=' && !state.v_found {
                state.v_idx = idx + 1;
                state.v_found = true;
            } else if !state.k_found {
                state.k_idx = idx;
                state.k_found = true;
            }
        }

        // Handle last key-value pair
        if state.k_found {
            let (key, val) = self.extract_kv(bytes, &state, bytes.len());
            out.push((key, val));
        }

        // Filter by allowed query keys
        let mut filtered = self.filter_queries(&out);

        if filtered.len() > 1 {
            crate::sort::key_value::kv_slice(&mut filtered);
        }
        filtered
    }

    /// Gets filtered and sorted key queries from a query string.
    #[allow(dead_code)]
    pub fn get_filtered_and_sorted_key_queries(&self, query_str: &str) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut out = Vec::new();

        if let Some(ref query_bytes) = self.0.rule.cache_key.query_bytes {
            // Parse query string manually
            for pair in query_str.split('&') {
                if let Some((key, value)) = pair.split_once('=') {
                    let key_bytes = key.as_bytes();
                    // Check if key matches any allowed key
                    for allowed_key in query_bytes {
                        if key_bytes.starts_with(allowed_key) {
                            out.push((key_bytes.to_vec(), value.as_bytes().to_vec()));
                            break;
                        }
                    }
                }
            }
        }

        if out.len() > 1 {
            crate::sort::key_value::kv_slice(&mut out);
        }

        out
    }

    /// Filters queries in place (modifies the input slice).
    #[allow(dead_code)]
    pub fn filter_and_sort_key_queries_in_place(&self, queries: &mut Vec<(Vec<u8>, Vec<u8>)>) {
        if let Some(ref allowed) = self.0.rule.cache_key.query_bytes {
            let mut n = 0;
            for i in 0..queries.len() {
                let key = &queries[i].0;
                let keep = allowed.iter().any(|ak| key.starts_with(ak));
                if keep {
                    if n != i {
                        queries.swap(n, i);
                    }
                    n += 1;
                }
            }
            queries.truncate(n);

            if queries.len() > 1 {
                crate::sort::key_value::kv_slice(queries);
            }
        }
    }

    /// Walks over query parameters directly from encoded payload.
    pub fn walk_query<F>(&self, mut callback: F) -> Result<(), QueryError>
    where
        F: FnMut(&[u8], &[u8]) -> bool,
    {
        use super::payload::{OFFSETS_MAP_SIZE, OFF_QUERY, OFF_REQ_HDRS, OFF_WEIGHT};

        let payload_guard = self.0.payload.load();
        let arc_bytes = match payload_guard.as_ref() {
            Some(arc_bytes) => arc_bytes,
            None => return Err(QueryError::MalformedOrNilPayload),
        };
        let data = &**arc_bytes;
        
        if data.is_empty() {
            return Err(QueryError::MalformedOrNilPayload);
        }

        if data.len() < OFFSETS_MAP_SIZE {
            return Err(QueryError::MalformedOrNilPayload);
        }

        let offset_from = LittleEndian::read_u32(&data[OFF_QUERY..OFF_QUERY + OFF_WEIGHT]) as usize;
        let offset_to =
            LittleEndian::read_u32(&data[OFF_REQ_HDRS..OFF_REQ_HDRS + OFF_WEIGHT]) as usize;

        let mut pos = offset_from;
        while pos < offset_to {
            if pos + OFF_WEIGHT > data.len() {
                return Err(QueryError::CorruptedQueriesSection);
            }

            let k_len = LittleEndian::read_u32(&data[pos..pos + OFF_WEIGHT]) as usize;
            pos += OFF_WEIGHT;
            if pos + k_len > data.len() {
                return Err(QueryError::CorruptedQueriesSection);
            }
            let k = &data[pos..pos + k_len];
            pos += k_len;

            let v_len = LittleEndian::read_u32(&data[pos..pos + OFF_WEIGHT]) as usize;
            pos += OFF_WEIGHT;
            if pos + v_len > data.len() {
                return Err(QueryError::CorruptedQueriesSection);
            }
            let v = &data[pos..pos + v_len];
            pos += v_len;

            if !callback(k, v) {
                break;
            }
        }

        Ok(())
    }

    /// Extracts key-value pair from bytes based on state.
    /// Helper function for parse_filter_and_sort_query.
    fn extract_kv(&self, bytes: &[u8], state: &QueryState, end: usize) -> (Vec<u8>, Vec<u8>) {
        if state.v_found {
            let key = bytes[state.k_idx..state.v_idx - 1].to_vec();
            let val = bytes[state.v_idx..end].to_vec();
            (key, val)
        } else {
            let key = bytes[state.k_idx..end].to_vec();
            (key, Vec::new())
        }
    }

    /// Filters queries by allowed keys.
    /// Helper function for parse_filter_and_sort_query.
    fn filter_queries(&self, queries: &[(Vec<u8>, Vec<u8>)]) -> Vec<(Vec<u8>, Vec<u8>)> {
        if let Some(ref allowed) = self.0.rule.cache_key.query_bytes {
            queries
                .iter()
                .filter(|(key, _)| allowed.iter().any(|ak| key.starts_with(ak)))
                .cloned()
                .collect()
        } else {
            queries.to_vec()
        }
    }
}

/// State for parsing query string.
/// Helper struct for parse_filter_and_sort_query.
#[derive(Default)]
struct QueryState {
    k_idx: usize,
    v_idx: usize,
    k_found: bool,
    v_found: bool,
}

impl QueryState {
    fn reset_for_next(&mut self, new_k_idx: usize) {
        self.k_idx = new_k_idx;
        self.k_found = true;
        self.v_idx = 0;
        self.v_found = false;
    }
}
