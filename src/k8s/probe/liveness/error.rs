// Error definitions for liveness probe

use std::fmt;

#[derive(Debug, Clone)]
pub struct TimeoutIsTooShortError;

impl fmt::Display for TimeoutIsTooShortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "liveness probe timeout is too short")
    }
}

impl std::error::Error for TimeoutIsTooShortError {}
