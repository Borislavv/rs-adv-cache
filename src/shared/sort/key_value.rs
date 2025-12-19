//! Key-value slice sorting.

/// Compares two byte slices lexicographically, without allocations.
fn less(a: &[u8], b: &[u8]) -> bool {
    let m = a.len().min(b.len());
    for i in 0..m {
        if a[i] < b[i] {
            return true;
        }
        if a[i] > b[i] {
            return false;
        }
    }
    a.len() < b.len()
}

/// Sorts a slice of key-value pairs in-place by key using insertion sort.
/// Sorts a slice of key-value pairs in-place by key using insertion sort.
pub fn kv_slice(kv: &mut [(Vec<u8>, Vec<u8>)]) {
    for i in 1..kv.len() {
        let mut j = i;
        while j > 0 && less(&kv[j].0, &kv[j - 1].0) {
            kv.swap(j, j - 1);
            j -= 1;
        }
    }
}
