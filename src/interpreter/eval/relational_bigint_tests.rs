use super::compare_bigint_number;
use num_bigint::BigInt;

fn big(n: i64) -> BigInt {
    BigInt::from(n)
}

// Expected values below were cross-checked against Node's evaluation of the
// corresponding `<` expressions, an independent source of truth.

#[test]
fn bigint_left_finite_number() {
    // 1n < 1.5 -> true
    assert_eq!(compare_bigint_number(&big(1), 1.5, true), Some(true));
    // 2n < 1.5 -> false
    assert_eq!(compare_bigint_number(&big(2), 1.5, true), Some(false));
}

#[test]
fn bigint_left_nan_is_undefined() {
    // 1n < NaN -> undefined
    assert_eq!(compare_bigint_number(&big(1), f64::NAN, true), None);
}

#[test]
fn bigint_left_infinities() {
    // 1n < Infinity -> true; 1n < -Infinity -> false
    assert_eq!(
        compare_bigint_number(&big(1), f64::INFINITY, true),
        Some(true)
    );
    assert_eq!(
        compare_bigint_number(&big(1), f64::NEG_INFINITY, true),
        Some(false)
    );
}

#[test]
fn bigint_left_negative_fraction_tiebreak() {
    // -1n < -0.5 -> true; -1n < -1.5 -> false; 0n < -0.5 -> false
    assert_eq!(compare_bigint_number(&big(-1), -0.5, true), Some(true));
    assert_eq!(compare_bigint_number(&big(-1), -1.5, true), Some(false));
    assert_eq!(compare_bigint_number(&big(0), -0.5, true), Some(false));
}

#[test]
fn number_left_finite_number() {
    // 1.5 < 2n -> true; 1.5 < 1n -> false
    assert_eq!(compare_bigint_number(&big(2), 1.5, false), Some(true));
    assert_eq!(compare_bigint_number(&big(1), 1.5, false), Some(false));
}

#[test]
fn number_left_equal_integer_parts() {
    // 2.0 < 2n -> false; 1.9 < 2n -> true; -0.5 < 0n -> true
    assert_eq!(compare_bigint_number(&big(2), 2.0, false), Some(false));
    assert_eq!(compare_bigint_number(&big(2), 1.9, false), Some(true));
    assert_eq!(compare_bigint_number(&big(0), -0.5, false), Some(true));
}

#[test]
fn number_left_nan_and_infinities() {
    // NaN < 1n -> undefined; Infinity < 1n -> false; -Infinity < 1n -> true
    assert_eq!(compare_bigint_number(&big(1), f64::NAN, false), None);
    assert_eq!(
        compare_bigint_number(&big(1), f64::INFINITY, false),
        Some(false)
    );
    assert_eq!(
        compare_bigint_number(&big(1), f64::NEG_INFINITY, false),
        Some(true)
    );
}

#[test]
fn precision_beyond_f64_integer_range() {
    // 2^53 = 9007199254740992 is exactly representable; 2^53 + 1 is not and
    // rounds to 2^53 as an f64 literal, but the BigInt stays exact.
    let two_53 = 9_007_199_254_740_992.0_f64;
    let two_53_plus_1 = big(9_007_199_254_740_993);
    // 9007199254740993n < 9007199254740992 -> false
    assert_eq!(
        compare_bigint_number(&two_53_plus_1, two_53, true),
        Some(false)
    );
    // 9007199254740993 (rounds to 2^53) < 9007199254740993n -> true
    assert_eq!(
        compare_bigint_number(&two_53_plus_1, two_53, false),
        Some(true)
    );
}
