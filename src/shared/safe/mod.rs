// Package safe provides safe arithmetic operations.

/// Safely divides two integers, returning 0.0 if denominator is zero.
/// Equivalent to Go's safe.Divide function.
pub fn divide(a: i64, b: i64) -> f64 {
    if b == 0 {
        return 0.0;
    }
    a as f64 / b as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_divide() {
        assert_eq!(divide(10, 2), 5.0);
        assert_eq!(divide(10, 0), 0.0);
        assert_eq!(divide(0, 5), 0.0);
    }
}

