//! Global test lock for serializing tests that modify global state.
//! 
//! This module provides a global async mutex to ensure tests that modify
//! global admin toggles (admission, bypass, compression, etc.) don't interfere
//! with each other when running in parallel.

use std::sync::OnceLock;
use tokio::sync::Mutex;

static GLOBAL_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Executes a test function while holding a global lock.
/// 
/// This ensures that tests modifying global state (admin toggles, cache config, etc.)
/// run serially and don't interfere with each other.
/// 
/// # Example
/// 
/// ```rust
/// #[tokio::test]
/// async fn test_that_modifies_global_state() {
///     with_global_lock(|| async {
///         // Test code that modifies global state
///     }).await;
/// }
/// ```
pub async fn with_global_lock<F, Fut, T>(f: F) -> T
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let lock = GLOBAL_TEST_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().await;
    f().await
}
