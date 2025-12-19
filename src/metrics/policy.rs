//! Policy types for metrics.
//

/// Policy represents a lifetime policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// Remove policy - remove items on TTL expiration.
    Remove,
    /// Refresh policy - refresh items on TTL expiration.
    Refresh,
}

impl Policy {
    /// Creates a new lifetime policy.
    pub fn new_lifetime_policy(is_remove_on_ttl: bool) -> Self {
        if is_remove_on_ttl {
            Self::Remove
        } else {
            Self::Refresh
        }
    }

    /// Converts policy to u64 for metrics.
    pub fn to_u64(self) -> u64 {
        match self {
            Self::Remove => 0,
            Self::Refresh => 1,
        }
    }
}

impl From<Policy> for u64 {
    fn from(policy: Policy) -> Self {
        policy.to_u64()
    }
}
