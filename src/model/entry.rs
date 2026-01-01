//! Cache entry models.

use std::sync::atomic::{AtomicBool, AtomicI64};
use std::sync::Arc;
use bytes::Bytes;

use crate::config::Rule;

/// Helper struct for key building result.
struct KeyHash {
    key: u64,
    fingerprint_hi: u64,
    fingerprint_lo: u64,
}

/// Response structure for HTTP responses.
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Payload structure containing all entry data.
pub struct Payload {
    pub queries: Vec<(Vec<u8>, Vec<u8>)>,
    pub req_headers: Vec<(Vec<u8>, Vec<u8>)>,
    pub rsp_headers: Vec<(Vec<u8>, Vec<u8>)>,
    pub body: Vec<u8>,
    pub code: u16,
}

/// Request payload structure.
pub struct RequestPayload {
    pub queries: Vec<(Vec<u8>, Vec<u8>)>,
    pub headers: Vec<(Vec<u8>, Vec<u8>)>,
}

/// Response payload structure.
pub struct ResponsePayload {
    pub headers: Vec<(Vec<u8>, Vec<u8>)>,
    pub body: Vec<u8>,
    pub code: u16,
}

/// Internal structure for entry data.
/// All fields are stored directly (not in Arc) since Entry itself wraps this in Arc.
pub struct EntryInner {
    pub(crate) key: u64,
    pub(crate) fingerprint_hi: u64,
    pub(crate) fingerprint_lo: u64,
    pub(crate) rule: Arc<Rule>,
    // Payload stored as Bytes - zero-copy, no capacity overhead, efficient cloning
    // Use ArcSwapOption for atomic updates without locks, Option allows empty payload
    // ArcSwapOption wraps the value in Arc internally, so we store Bytes directly
    pub(crate) payload: arc_swap::ArcSwapOption<Bytes>,
    pub(crate) touched_at: AtomicI64,
    pub(crate) updated_at: AtomicI64,
    pub(crate) refresh_queued: AtomicBool,
}

/// Entry represents a cache entry.
#[derive(Clone)]
pub struct Entry(pub Arc<EntryInner>);

impl Entry {
    /// Gets reference to inner EntryInner.
    #[allow(dead_code)]
    pub fn inner(&self) -> &EntryInner {
        &self.0
    }

    /// Initializes a new entry inner.
    fn init_inner() -> EntryInner {
        EntryInner {
            key: 0,
            fingerprint_hi: 0,
            fingerprint_lo: 0,
            rule: Arc::new(Rule {
                path: None,
                path_bytes: None,
                cache_key: crate::config::RuleKey {
                    query: None,
                    query_bytes: None,
                    headers: None,
                    headers_map: None,
                },
                cache_value: crate::config::RuleValue {
                    headers: None,
                    headers_map: None,
                },
                refresh: None,
            }),
            payload: arc_swap::ArcSwapOption::empty(),
            touched_at: AtomicI64::new(0),
            updated_at: AtomicI64::new(0),
            refresh_queued: AtomicBool::new(false),
        }
    }

    /// Initializes a new entry.
    pub fn init() -> Self {
        Self(Arc::new(Self::init_inner()))
    }

    /// Creates a new entry with a rule (for tests - allows setting rule after creation).
    #[cfg(test)]
    pub fn with_rule(self, rule: Arc<Rule>) -> Self {
        // Create new EntryInner with updated rule
        use std::sync::atomic::Ordering;
        let payload_guard = self.0.payload.load();
        let payload_clone = payload_guard.as_ref().map(|arc_bytes| Arc::clone(arc_bytes)); // Arc::clone() is cheap
        let inner = EntryInner {
            key: self.0.key,
            fingerprint_hi: self.0.fingerprint_hi,
            fingerprint_lo: self.0.fingerprint_lo,
            rule,
            payload: arc_swap::ArcSwapOption::from(payload_clone),
            touched_at: AtomicI64::new(self.0.touched_at.load(Ordering::Relaxed)),
            updated_at: AtomicI64::new(self.0.updated_at.load(Ordering::Relaxed)),
            refresh_queued: AtomicBool::new(self.0.refresh_queued.load(Ordering::Relaxed)),
        };
        Self(Arc::new(inner))
    }

    /// Creates a new entry.
    pub fn new(
        rule: Arc<Rule>,
        queries: &[(Vec<u8>, Vec<u8>)],
        headers: &[(Vec<u8>, Vec<u8>)],
    ) -> Self {
        let key_hash = Self::build_key_hash(queries, headers, &rule);
        
        let inner = EntryInner {
            key: key_hash.key,
            fingerprint_hi: key_hash.fingerprint_hi,
            fingerprint_lo: key_hash.fingerprint_lo,
            rule,
            payload: arc_swap::ArcSwapOption::empty(),
            touched_at: AtomicI64::new(0),
            updated_at: AtomicI64::new(0),
            refresh_queued: AtomicBool::new(false),
        };
        Self(Arc::new(inner))
    }

    /// Builds key hash from queries and headers (static helper).
    fn build_key_hash(
        filtered_queries: &[(Vec<u8>, Vec<u8>)],
        filtered_headers: &[(Vec<u8>, Vec<u8>)],
        rule: &Rule,
    ) -> KeyHash {
        use xxhash_rust::xxh3::xxh3_128;
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        // Calculate buffer size
        let mut buf_len = rule.path_bytes.as_ref().map(|p| p.len()).unwrap_or(0);
        for (k, v) in filtered_queries {
            buf_len += k.len() + v.len();
        }
        for (k, v) in filtered_headers {
            buf_len += k.len() + v.len();
        }

        let mut buf = Vec::with_capacity(buf_len);
        if let Some(ref path_bytes) = rule.path_bytes {
            buf.extend_from_slice(path_bytes);
        }
        for (k, v) in filtered_queries {
            buf.extend_from_slice(k);
            buf.extend_from_slice(v);
        }
        for (k, v) in filtered_headers {
            buf.extend_from_slice(k);
            buf.extend_from_slice(v);
        }

        // Calculate hash using DefaultHasher
        let mut hasher = DefaultHasher::new();
        buf.hash(&mut hasher);
        let key = hasher.finish();
        
        // For 128-bit fingerprint, use xxh3_128 directly
        let fingerprint = xxh3_128(&buf);
        let fingerprint_hi = (fingerprint >> 64) as u64;
        let fingerprint_lo = fingerprint as u64;

        // buf is dropped here - memory freed

        KeyHash {
            key,
            fingerprint_hi,
            fingerprint_lo,
        }
    }

    /// Creates a new entry from fields.
    pub fn from_field(
        key: u64,
        f_hi: u64,
        f_lo: u64,
        payload: Vec<u8>,
        rule: Arc<Rule>,
        updated_at: i64,
    ) -> Self {
        let payload_opt = if payload.is_empty() {
            None
        } else {
            Some(Arc::new(Bytes::from(payload)))
        };
        let inner = EntryInner {
            key,
            fingerprint_hi: f_hi,
            fingerprint_lo: f_lo,
            rule,
            payload: arc_swap::ArcSwapOption::from(payload_opt),
            touched_at: AtomicI64::new(0),
            updated_at: AtomicI64::new(updated_at),
            refresh_queued: AtomicBool::new(false),
        };
        Self(Arc::new(inner))
    }
}

impl Default for Entry {
    fn default() -> Self {
        Self::init()
    }
}

// Convenience methods that delegate to inner
impl Entry {
    /// Gets fingerprint high part.
    #[allow(dead_code)]
    pub fn fingerprint_hi(&self) -> u64 {
        self.0.fingerprint_hi
    }

    /// Gets fingerprint low part.
    #[allow(dead_code)]
    pub fn fingerprint_lo(&self) -> u64 {
        self.0.fingerprint_lo
    }

    /// Gets the rule.
    pub(crate) fn rule(&self) -> &Arc<Rule> {
        &self.0.rule
    }

    /// Gets updated_at atomic reference (internal use).
    pub(crate) fn updated_at_ref(&self) -> &AtomicI64 {
        &self.0.updated_at
    }
}

