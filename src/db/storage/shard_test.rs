//! Tests for shard operations, including LRU eviction.

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use super::super::shard::Shard;
    use crate::model::Entry;

    fn make_test_entry(_key: u64) -> Entry {
        use crate::config::Rule;
        let rule = Arc::new(Rule {
            path: None,
            path_bytes: None,
            cache_key: crate::config::RuleKey {
                query: None,
                query_bytes: None,
                headers: None,
                headers_map: None,
            },
            cache_value: crate::config::RuleValue {
                headers: None,
                headers_map: None,
            },
            refresh: None,
        });

        let queries = vec![];
        let headers = vec![];
        Entry::new(rule, &queries, &headers)
    }

    #[test]
    fn test_lru_pop_tail_decrements_counters() {
        let shard: Shard<Entry> = Shard::new(0);
        shard.enable_lru();

        // Add entries
        let entry1 = make_test_entry(1);
        let weight1 = entry1.weight();
        shard.set(1, entry1);

        let entry2 = make_test_entry(2);
        shard.set(2, entry2);

        let entry3 = make_test_entry(3);
        shard.set(3, entry3);

        // Check initial state
        let initial_mem = shard.weight();
        let initial_len = shard.len();

        // Evict one entry (should be entry1, the oldest)
        let (freed_bytes, did_remove) = shard.evict_one_lru_tail();

        assert!(did_remove, "Should have removed an entry");
        assert_eq!(freed_bytes, weight1, "Freed bytes should match entry weight");
        assert_eq!(shard.weight(), initial_mem - weight1, "Shard mem should be decremented");
        assert_eq!(shard.len(), initial_len - 1, "Shard len should be decremented");

        // Verify entry is removed from items
        assert!(shard.get(1).is_none(), "Entry 1 should be removed");
        assert!(shard.get(2).is_some(), "Entry 2 should still exist");
        assert!(shard.get(3).is_some(), "Entry 3 should still exist");
    }

    #[test]
    fn test_lru_pop_tail() {
        let shard: Shard<Entry> = Shard::new(0);
        shard.enable_lru();

        // Add an entry
        let entry = make_test_entry(1);
        let weight = entry.weight();
        shard.set(1, entry);

        // Evict it
        let (freed_bytes, did_remove) = shard.evict_one_lru_tail();

        assert!(did_remove);
        assert_eq!(freed_bytes, weight);
        // but the code path is executed (no panic means it worked)
    }

    #[test]
    fn test_lru_pop_tail_empty_shard() {
        let shard: Shard<Entry> = Shard::new(0);
        shard.enable_lru();

        // Try to evict from empty shard
        let (freed_bytes, did_remove) = shard.evict_one_lru_tail();

        assert!(!did_remove, "Should not remove anything from empty shard");
        assert_eq!(freed_bytes, 0, "Should free 0 bytes");
    }

    #[test]
    fn test_lru_pop_tail_lru_disabled() {
        let shard: Shard<Entry> = Shard::new(0);
        // LRU is disabled by default

        // Add an entry
        let entry = make_test_entry(1);
        shard.set(1, entry);

        // Try to evict (should fail because LRU is disabled)
        let (freed_bytes, did_remove) = shard.evict_one_lru_tail();

        assert!(!did_remove, "Should not remove when LRU is disabled");
        assert_eq!(freed_bytes, 0, "Should free 0 bytes");
    }

    #[test]
    fn test_lru_pop_tail_evicts_oldest() {
        let shard: Shard<Entry> = Shard::new(0);
        shard.enable_lru();

        // Add entries in order
        let entry1 = make_test_entry(1);
        shard.set(1, entry1);

        let entry2 = make_test_entry(2);
        shard.set(2, entry2);

        let entry3 = make_test_entry(3);
        shard.set(3, entry3);

        // First eviction should remove entry1 (oldest)
        let (_, did_remove1) = shard.evict_one_lru_tail();
        assert!(did_remove1);
        assert!(shard.get(1).is_none(), "Entry 1 should be evicted first");

        // Second eviction should remove entry2
        let (_, did_remove2) = shard.evict_one_lru_tail();
        assert!(did_remove2);
        assert!(shard.get(2).is_none(), "Entry 2 should be evicted second");

        // Third eviction should remove entry3
        let (_, did_remove3) = shard.evict_one_lru_tail();
        assert!(did_remove3);
        assert!(shard.get(3).is_none(), "Entry 3 should be evicted third");

        // Fourth eviction should fail (empty)
        let (_, did_remove4) = shard.evict_one_lru_tail();
        assert!(!did_remove4, "Should not evict from empty shard");
    }
}
