//! Tests for key-value sorting.

#[cfg(test)]
mod tests {
    use rand::Rng;

    fn make_test_slice(n: usize) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut rng = rand::thread_rng();
        let mut s = Vec::with_capacity(n);
        for _ in 0..n {
            let mut b = vec![0u8; 8];
            rng.fill(&mut b[..]);
            s.push((b, b"value".to_vec())); // dummy payload
        }
        s
    }

    fn clone_slice(in_slice: &[(Vec<u8>, Vec<u8>)]) -> Vec<(Vec<u8>, Vec<u8>)> {
        in_slice
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    #[test]
    fn test_sort_kv_custom() {
        let data = make_test_slice(24);
        let mut buf = clone_slice(&data);
        crate::sort::key_value::kv_slice(&mut buf);
        // Verify it's sorted
        for i in 1..buf.len() {
            assert!(buf[i - 1].0 <= buf[i].0, "slice should be sorted");
        }
    }

    #[test]
    fn test_sort_kv_std_slice() {
        let data = make_test_slice(24);
        let mut buf = clone_slice(&data);
        buf.sort_by(|a, b| a.0.cmp(&b.0));
        // Verify it's sorted
        for i in 1..buf.len() {
            assert!(buf[i - 1].0 <= buf[i].0, "slice should be sorted");
        }
    }
}
