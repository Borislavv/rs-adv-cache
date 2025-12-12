// Package upstream provides backend functionality.

pub mod upstream;
pub mod backend;
pub mod probe;
pub mod copy;
pub mod proxy;
pub mod sanitize;
pub mod trace;

#[cfg(test)]
mod proxy_test;

// Re-export main types
pub use upstream::{Policy, Response, Upstream, actual_policy, change_policy};
pub use backend::BackendImpl;

