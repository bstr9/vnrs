//! Custom test assertions for floating-point and result comparisons

/// Assert that two floats are approximately equal within the given tolerance
///
/// # Panics
/// Panics if `|left - right| > tolerance`
pub fn assert_approx_eq(left: f64, right: f64, tolerance: f64) {
    let diff = (left - right).abs();
    assert!(
        diff <= tolerance,
        "assertion failed: approx_eq({}, {}) with tolerance {} (diff = {})",
        left,
        right,
        tolerance,
        diff
    );
}

/// Assert that a float value falls within a range [min, max]
///
/// # Panics
/// Panics if `value < min || value > max`
pub fn assert_in_range(value: f64, min: f64, max: f64) {
    assert!(
        value >= min && value <= max,
        "assertion failed: {} is not in range [{}, {}]",
        value,
        min,
        max
    );
}

/// Assert that a result is OK and return the inner value
///
/// # Panics
/// Panics if the result is Err
pub fn assert_ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(e) => panic!("assertion failed: expected Ok, got Err({:?})", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assert_approx_eq_equal() {
        assert_approx_eq(1.0, 1.0, 0.001);
    }

    #[test]
    fn test_assert_approx_eq_within_tolerance() {
        assert_approx_eq(1.0, 1.0005, 0.001);
    }

    #[test]
    fn test_assert_in_range() {
        assert_in_range(5.0, 0.0, 10.0);
        assert_in_range(0.0, 0.0, 10.0);
        assert_in_range(10.0, 0.0, 10.0);
    }

    #[test]
    fn test_assert_ok() {
        let result: Result<i32, &str> = Ok(42);
        let value = assert_ok(result);
        assert_eq!(value, 42);
    }

    #[test]
    #[should_panic(expected = "assertion failed: approx_eq")]
    fn test_assert_approx_eq_fails() {
        assert_approx_eq(1.0, 2.0, 0.001);
    }

    #[test]
    #[should_panic(expected = "assertion failed")]
    fn test_assert_in_range_fails() {
        assert_in_range(15.0, 0.0, 10.0);
    }

    #[test]
    #[should_panic(expected = "assertion failed: expected Ok")]
    fn test_assert_ok_fails() {
        let result: Result<i32, &str> = Err("error");
        assert_ok(result);
    }
}
