// Package sharded provides LRU list operations.

use std::collections::HashMap;

/// LRU list structure for tracking access order.
/// Uses a Vec for ordering and HashMap for O(1) lookups.
pub struct LRUList {
    /// Ordered list of keys (most recently used at front)
    order: Vec<u64>,
    /// Map from key to index in order vector
    indices: HashMap<u64, usize>,
}

impl LRUList {
    pub fn new() -> Self {
        Self {
            order: Vec::new(),
            indices: HashMap::new(),
        }
    }

    /// Moves a key to the front (most recently used).
    pub fn move_to_front(&mut self, key: u64) {
        if let Some(&idx) = self.indices.get(&key) {
            // Remove from current position
            self.order.remove(idx);
            // Update indices for all elements after the removed one
            for i in idx..self.order.len() {
                *self.indices.get_mut(&self.order[i]).unwrap() = i;
            }
            // Add to front
            self.order.insert(0, key);
            // Update all indices
            for (i, k) in self.order.iter().enumerate() {
                self.indices.insert(*k, i);
            }
        } else {
            // New key, add to front
            self.order.insert(0, key);
            // Update all indices
            for (i, k) in self.order.iter().enumerate() {
                self.indices.insert(*k, i);
            }
        }
    }

    /// Removes a key from the LRU list.
    pub fn remove(&mut self, key: u64) {
        if let Some(&idx) = self.indices.get(&key) {
            self.order.remove(idx);
            self.indices.remove(&key);
            // Update indices for remaining elements
            for i in idx..self.order.len() {
                *self.indices.get_mut(&self.order[i]).unwrap() = i;
            }
        }
    }

    /// Pushes a key to the front.
    pub fn push_front(&mut self, key: u64) {
        if !self.indices.contains_key(&key) {
            self.order.insert(0, key);
            // Update all indices
            for (i, k) in self.order.iter().enumerate() {
                self.indices.insert(*k, i);
            }
        } else {
            self.move_to_front(key);
        }
    }

    /// Peeks at the tail (least recently used).
    pub fn peek_tail(&self) -> Option<u64> {
        self.order.last().copied()
    }

    /// Pops the tail (least recently used).
    pub fn pop_tail(&mut self) -> Option<u64> {
        if let Some(key) = self.order.pop() {
            self.indices.remove(&key);
            Some(key)
        } else {
            None
        }
    }

    /// Clears the LRU list.
    pub fn clear(&mut self) {
        self.order.clear();
        self.indices.clear();
    }

    /// Checks if the list is empty.
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

impl Default for LRUList {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe LRU wrapper (just the list, locking is handled by shard).
#[allow(dead_code)]
pub type ThreadSafeLRU = LRUList;

