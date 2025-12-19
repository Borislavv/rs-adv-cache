//! LRU list operations with O(1) complexity.
//
// Uses a doubly-linked list with raw pointers for O(1) operations:
// - move_to_front: O(1)
// - push_front: O(1)
// - remove: O(1)
// - pop_tail: O(1)
//
// HashMap provides O(1) lookup from key to node pointer.

use std::collections::HashMap;
use std::ptr::{self, NonNull};

/// LRU node in the doubly-linked list.
/// Uses raw pointers for O(1) operations without indices recalculation.
#[repr(C)]
struct LruNode {
    key: u64,
    prev: *mut LruNode,
    next: *mut LruNode,
}

impl LruNode {
    /// Creates a new node with the given key.
    fn new(key: u64) -> Box<Self> {
        Box::new(LruNode {
            key,
            prev: ptr::null_mut(),
            next: ptr::null_mut(),
        })
    }
}

/// LRU list structure for tracking access order with O(1) operations.
pub struct LRUList {
    /// Head of the list (most recently used)
    head: *mut LruNode,
    /// Tail of the list (least recently used)
    tail: *mut LruNode,
    /// Map from key to node pointer for O(1) lookup
    nodes: HashMap<u64, NonNull<LruNode>>,
}

// Safety: LRUList manages raw pointers but ensures they always point to valid Box<LruNode>
// allocated on the heap. Nodes are only deallocated in Drop, and all operations maintain
// list invariants.
unsafe impl Send for LRUList {}
unsafe impl Sync for LRUList {}

impl LRUList {
    /// Creates a new empty LRU list.
    pub fn new() -> Self {
        Self {
            head: ptr::null_mut(),
            tail: ptr::null_mut(),
            nodes: HashMap::new(),
        }
    }

    /// Moves a key to the front (most recently used).
    /// If key doesn't exist, adds it to the front.
    /// O(1) operation.
    pub fn move_to_front(&mut self, key: u64) {
        if let Some(node_ptr) = self.nodes.get(&key).copied() {
            unsafe {
                let node = node_ptr.as_ptr();
                self.remove_node(node);
                self.push_node_front(node);
            }
        } else {
            self.push_front(key);
        }
    }

    /// Pushes a key to the front (most recently used).
    /// If key already exists, moves it to front.
    /// O(1) operation.
    pub fn push_front(&mut self, key: u64) {
        if let Some(node_ptr) = self.nodes.get(&key).copied() {
            unsafe {
                let node = node_ptr.as_ptr();
                self.remove_node(node);
                self.push_node_front(node);
            }
        } else {
            let node = LruNode::new(key);
            let node_ptr = NonNull::from(Box::leak(node));
            self.nodes.insert(key, node_ptr);
            
            unsafe {
                self.push_node_front(node_ptr.as_ptr());
            }
        }
    }

    /// Removes a key from the LRU list.
    /// O(1) operation.
    pub fn remove(&mut self, key: u64) {
        if let Some(node_ptr) = self.nodes.remove(&key) {
            unsafe {
                let node = node_ptr.as_ptr();
                self.remove_node(node);
                drop(Box::from_raw(node));
            }
        }
    }

    /// Peeks at the tail (least recently used) without removing it.
    /// O(1) operation.
    pub fn peek_tail(&self) -> Option<u64> {
        if self.tail.is_null() {
            None
        } else {
            unsafe {
                Some((*self.tail).key)
            }
        }
    }

    /// Pops the tail (least recently used).
    /// O(1) operation.
    pub fn pop_tail(&mut self) -> Option<u64> {
        if self.tail.is_null() {
            return None;
        }

        unsafe {
            let node = self.tail;
            let key = (*node).key;
            
            self.remove_node(node);
            self.nodes.remove(&key);
            drop(Box::from_raw(node));
            
            Some(key)
        }
    }

    /// Clears the LRU list.
    pub fn clear(&mut self) {
        unsafe {
            let mut current = self.head;
            while !current.is_null() {
                let next = (*current).next;
                drop(Box::from_raw(current));
                current = next;
            }
        }
        
        self.head = ptr::null_mut();
        self.tail = ptr::null_mut();
        self.nodes.clear();
    }

    /// Checks if the list is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.head.is_null()
    }

    /// Helper: Removes a node from the list (does not deallocate).
    unsafe fn remove_node(&mut self, node: *mut LruNode) {
        let prev = (*node).prev;
        let next = (*node).next;

        if prev.is_null() {
            self.head = next;
        } else {
            (*prev).next = next;
        }

        if next.is_null() {
            self.tail = prev;
        } else {
            (*next).prev = prev;
        }

        (*node).prev = ptr::null_mut();
        (*node).next = ptr::null_mut();
    }

    /// Helper: Pushes a node to the front of the list.
    unsafe fn push_node_front(&mut self, node: *mut LruNode) {
        (*node).next = self.head;
        (*node).prev = ptr::null_mut();

        if !self.head.is_null() {
            (*self.head).prev = node;
        } else {
            self.tail = node;
        }

        self.head = node;
    }
}

impl Default for LRUList {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LRUList {
    fn drop(&mut self) {
        self.clear();
    }
}

/// Thread-safe LRU wrapper (just the list, locking is handled by shard).
#[allow(dead_code)]
pub type ThreadSafeLRU = LRUList;
