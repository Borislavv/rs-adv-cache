// Package sharded provides a circular queue for refresh operations.

use std::sync::Mutex;

/// Circular queue for refresh operations.
pub struct Queue {
    mu: Mutex<QueueInner>,
}

struct QueueInner {
    buf: Vec<u64>,
    head: usize,
    tail: usize,
}

impl Queue {
    /// Creates a new queue with the specified size.
    pub fn new(size: usize) -> Self {
        let size = size.max(2);
        Self {
            mu: Mutex::new(QueueInner {
                buf: vec![0; size],
                head: 0,
                tail: 0,
            }),
        }
    }

    /// Tries to push a key into the queue.
    /// Returns true if successful, false if queue is full.
    pub fn try_push(&self, k: u64) -> bool {
        let mut inner = self.mu.lock().unwrap();
        let head = inner.head;
        let next = (head + 1) % inner.buf.len();
        if next == inner.tail {
            // Full
            return false;
        }
        inner.buf[head] = k;
        inner.head = next;
        true
    }

    /// Tries to pop a key from the queue.
    /// Returns Some(key) if successful, None if queue is empty.
    pub fn try_pop(&self) -> Option<u64> {
        let mut inner = self.mu.lock().unwrap();
        if inner.head == inner.tail {
            return None;
        }
        let v = inner.buf[inner.tail];
        inner.tail = (inner.tail + 1) % inner.buf.len();
        Some(v)
    }
}

impl Default for Queue {
    fn default() -> Self {
        Self::new(4096)
    }
}

