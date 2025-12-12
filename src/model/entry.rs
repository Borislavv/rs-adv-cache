// Package model provides cache entry models.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;

use crate::config::Rule;

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

/// Entry represents a cache entry.
#[derive(Clone)]
pub struct Entry {
    pub(crate) key: u64,
    pub(crate) fingerprint_hi: u64,
    pub(crate) fingerprint_lo: u64,
    pub(crate) rule: Arc<Rule>,
    pub(crate) payload: Arc<std::sync::Mutex<Option<Vec<u8>>>>,
    pub(crate) touched_at: Arc<AtomicI64>,
    pub(crate) updated_at: Arc<AtomicI64>,
    pub(crate) refresh_queued: Arc<AtomicBool>,
}

impl Entry {
    /// Initializes a new entry.
    pub fn init() -> Self {
        Self {
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
            payload: Arc::new(std::sync::Mutex::new(None)),
            touched_at: Arc::new(AtomicI64::new(0)),
            updated_at: Arc::new(AtomicI64::new(0)),
            refresh_queued: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Creates a new entry.
    pub fn new(
        rule: Arc<Rule>,
        queries: &[(Vec<u8>, Vec<u8>)],
        headers: &[(Vec<u8>, Vec<u8>)],
    ) -> Self {
        let mut entry = Self::init();
        entry.rule = rule;
        entry.build_key(queries, headers);
        entry
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
        let mut entry = Self::init();
        entry.key = key;
        entry.fingerprint_hi = f_hi;
        entry.fingerprint_lo = f_lo;
        entry.rule = rule;
        *entry.payload.lock().unwrap() = Some(payload);
        entry.updated_at.store(updated_at, Ordering::Relaxed);
        entry
    }

}

impl Default for Entry {
    fn default() -> Self {
        Self::init()
    }
}