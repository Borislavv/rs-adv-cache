//! Upstream backend functionality for proxying requests to origin servers.

pub mod backend;
mod backend_headers;
mod backend_hyper_impl;
pub mod probe;
pub mod proxy;
pub mod sanitize;
pub mod trace;
pub mod upstream;

#[cfg(test)]
mod proxy_test;

#[cfg(test)]
mod backend_hyper_impl_test;

// Re-export main types
pub use backend::BackendImpl;
pub use upstream::{actual_policy, change_policy, Policy, Response, Upstream};
