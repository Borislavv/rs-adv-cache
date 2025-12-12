// Package upstream provides backend functionality.

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio_util::sync::CancellationToken;

use crate::config::{Rule};
use crate::model::Entry;

/// Policy for handling upstream requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// Deny policy: fail-fast when backend is busy
    Deny,
    /// Await policy: wait for backend availability
    Await,
}

impl Policy {
    /// Converts policy to u64 for hashing/comparison.
    pub fn to_u64(self) -> u64 {
        match self {
            Policy::Deny => 0,
            Policy::Await => 1,
        }
    }

    /// Parses policy from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "deny" => Some(Policy::Deny),
            "await" => Some(Policy::Await),
            _ => None,
        }
    }
}

/// Global policy state.
static IS_AWAIT_POLICY: AtomicBool = AtomicBool::new(false);

/// Gets the actual policy.
pub fn actual_policy() -> Policy {
    if IS_AWAIT_POLICY.load(Ordering::Relaxed) {
        Policy::Await
    } else {
        Policy::Deny
    }
}

/// Changes the global policy.
pub fn change_policy(policy: Policy) -> Result<()> {
    match policy {
        Policy::Deny => IS_AWAIT_POLICY.store(false, Ordering::Relaxed),
        Policy::Await => IS_AWAIT_POLICY.store(true, Ordering::Relaxed),
    }
    Ok(())
}

/// Upstream defines the interface for external backends.
#[async_trait::async_trait]
pub trait Upstream: Send + Sync {
    /// Makes a request to the upstream backend.
    async fn request(
        &self,
        rule: &Rule,
        queries: &[(Vec<u8>, Vec<u8>)],
        headers: &[(Vec<u8>, Vec<u8>)],
        trace_ctx: CancellationToken,
    ) -> Result<Response>;

    /// Proxies a request to the upstream backend.
    async fn proxy_request(
        &self,
        method: &str,
        path: &str,
        query: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
        trace_ctx: CancellationToken,
    ) -> Result<Response>;

    /// Refreshes an entry by fetching new data from upstream.
    async fn refresh(&self, entry: &mut Entry) -> Result<()>;

    /// Checks if the upstream backend is healthy.
    async fn is_healthy(&self) -> Result<()>;
}

/// HTTP Response wrapper.
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn new(status: u16, headers: Vec<(String, String)>, body: Vec<u8>) -> Self {
        Self {
            status,
            headers,
            body,
        }
    }

    #[allow(dead_code)]
    pub fn is_ok(&self) -> bool {
        self.status == 200
    }
}

