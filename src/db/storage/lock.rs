//! Lock utilities with spin attempts.
//

use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::hint;

pub const EVICTION_RLOCK_SPINS: usize = 4;
pub const EVICTION_LOCK_SPINS: usize = 4;
pub const REFRESH_RLOCK_SPINS: usize = 8;
pub const REFRESH_GUARD_FACTOR: usize = 16;

/// Tries to acquire a read lock with spin attempts.
pub fn try_rlock<T>(lock: &RwLock<T>, spins: usize) -> Option<RwLockReadGuard<'_, T>> {
    for _ in 0..spins {
        if let Some(guard) = lock.try_read() {
            return Some(guard);
        }
        hint::spin_loop();
    }
    None
}

/// Tries to acquire a write lock with spin attempts.
pub fn try_lock<T>(lock: &RwLock<T>, spins: usize) -> Option<RwLockWriteGuard<'_, T>> {
    for _ in 0..spins {
        if let Some(guard) = lock.try_write() {
            return Some(guard);
        }
        hint::spin_loop();
    }
    None
}
