//! Integration tests for AdvCache.
//! 
//! This module contains end-to-end tests that verify cache behavior,
//! key isolation, whitelists, and other integration scenarios.

mod cases_admin_endpoints_test;
mod cases_brackets_canonicalization_test;
mod cases_cache_test;
mod cases_cache_behavior_test;
mod cases_concurrent_test;
mod cases_error_handling_test;
mod cases_integration_test;
mod cases_invalidation_test;
mod cases_key_isolation_test;
mod cases_order_and_negative_test;
mod cases_percent_encoding_test;
mod cases_proxy_test;
mod cases_whitelist_test;
mod cases_workers_test;

pub mod support;
