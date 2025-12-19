//! Cache rule matching functionality.

use anyhow::{anyhow, Result};
use std::sync::Arc;

use crate::config::{Config, ConfigTrait, Rule};

/// Error returned when cache rule is not found.
#[derive(Debug, Clone, thiserror::Error)]
#[error("cache rule not found")]
pub struct CacheRuleNotFoundError;

/// Matches a cache rule for the given path.
pub fn match_cache_rule(cfg: &Config, path: &[u8]) -> Result<Arc<Rule>> {
    let path_str = String::from_utf8_lossy(path);
    if let Some(rule) = cfg.rule(&path_str) {
        Ok(rule)
    } else {
        Err(anyhow!(CacheRuleNotFoundError))
    }
}

/// Checks if an error is a cache rule not found error.
pub fn is_cache_rule_not_found_err(err: &(dyn std::error::Error + 'static)) -> bool {
    err.downcast_ref::<CacheRuleNotFoundError>().is_some()
}
