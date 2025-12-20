// Shared test support code for integration tests.
// This module provides common utilities that all test files can use.

pub mod cache;
pub mod common;
pub mod harness;
pub mod lock;
pub mod upstream;

#[allow(unused_imports)] // Re-exports are used via crate::support in test files
pub use common::*;
#[allow(unused_imports)] // Re-exports are used via crate::support in test files
pub use harness::{cache_addr, init_test_harness};
#[allow(unused_imports)] // Re-exports are used via crate::support in test files
pub use lock::with_global_lock;
