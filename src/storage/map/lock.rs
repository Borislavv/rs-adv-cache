// Package sharded provides lock-related constants and utilities.

/// Number of spin cycles for eviction read lock attempts.
pub const EVICTION_RLOCK_SPINS: usize = 4;

/// Number of spin cycles for eviction write lock attempts.
pub const EVICTION_LOCK_SPINS: usize = 4;

/// Number of spin cycles for refresh read lock attempts.
pub const REFRESH_RLOCK_SPINS: usize = 8;

/// Guard factor for refresh sampling (limits max iterations).
pub const REFRESH_GUARD_FACTOR: usize = 16;

