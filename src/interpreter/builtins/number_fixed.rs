//! `Number.prototype.toFixed` digit generation (ECMA-262 §21.1.3.3 step 10).

/// Formats finite `x` (with `|x| < 10^21`) with exactly `digits` fraction
/// digits. The spec picks the integer n closest to `x * 10^digits` and, when
/// two are equally close, the larger one, so exact ties round away from zero
/// on the magnitude. Rust's `{:.N}` rounds ties to even instead
/// (`(0.5).toFixed(0)` would be "0", not "1"), so take the exact decimal
/// expansion and round it here. 1100 fraction digits hold every finite f64
/// exactly (the longest expansion is 1074 digits).
pub(super) fn to_fixed(x: f64, digits: usize) -> String {
    let exact = format!("{:.1100}", x.abs());
    let (int_part, frac) = exact.split_once('.').unwrap_or((&exact, ""));
    let mut kept: Vec<u8> = int_part.bytes().chain(frac.bytes().take(digits)).collect();
    if frac.as_bytes().get(digits).is_some_and(|&d| d >= b'5') {
        let mut i = kept.len();
        loop {
            if i == 0 {
                kept.insert(0, b'1');
                break;
            }
            i -= 1;
            if kept[i] == b'9' {
                kept[i] = b'0';
            } else {
                kept[i] += 1;
                break;
            }
        }
    }
    let split = kept.len() - digits;
    let mut out = String::with_capacity(kept.len() + 2);
    if x < 0.0 {
        out.push('-');
    }
    out.push_str(std::str::from_utf8(&kept[..split]).unwrap());
    if digits > 0 {
        out.push('.');
        out.push_str(std::str::from_utf8(&kept[split..]).unwrap());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::to_fixed;

    #[test]
    fn exact_ties_round_up() {
        assert_eq!(to_fixed(0.5, 0), "1");
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(-2.5, 0), "-3");
        assert_eq!(to_fixed(0.125, 2), "0.13");
        assert_eq!(to_fixed(123456789.125, 2), "123456789.13");
    }

    #[test]
    fn non_ties_use_the_exact_binary_value() {
        assert_eq!(to_fixed(1.005, 2), "1.00");
        assert_eq!(to_fixed(1.45, 1), "1.4");
        assert_eq!(to_fixed(8.345, 2), "8.35");
        assert_eq!(to_fixed(9.995, 2), "9.99");
    }

    #[test]
    fn carries_and_padding() {
        assert_eq!(to_fixed(9.5, 0), "10");
        assert_eq!(to_fixed(0.0, 3), "0.000");
        assert_eq!(to_fixed(1e20, 2), "100000000000000000000.00");
        assert_eq!(to_fixed(5e-324, 2), "0.00");
        assert_eq!(to_fixed(0.000001, 7), "0.0000010");
    }
}
