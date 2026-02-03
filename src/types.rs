use std::fmt;

#[derive(Clone, Debug)]
pub enum JsValue {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    String(JsString),
    Symbol(JsSymbol),
    BigInt(JsBigInt),
    Object(JsObject),
}

// UTF-16 code unit string per spec §6.1.4
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct JsString {
    pub code_units: Vec<u16>,
}

impl JsString {
    pub fn from_str(s: &str) -> Self {
        Self {
            code_units: s.encode_utf16().collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.code_units.is_empty()
    }

    pub fn len(&self) -> usize {
        self.code_units.len()
    }

    pub fn to_rust_string(&self) -> String {
        String::from_utf16_lossy(&self.code_units)
    }

    // §6.1.4.1 StringIndexOf(string, searchValue, fromIndex)
    pub fn index_of(&self, search: &JsString, from: usize) -> Option<usize> {
        let s_len = self.code_units.len();
        let search_len = search.code_units.len();
        if search_len == 0 {
            return if from <= s_len { Some(from) } else { None };
        }
        if from + search_len > s_len {
            return None;
        }
        (from..=(s_len - search_len))
            .find(|&i| self.code_units[i..i + search_len] == search.code_units[..])
    }

    pub fn slice_utf16(&self, start: usize, end: usize) -> JsString {
        let s = start.min(self.code_units.len());
        let e = end.min(self.code_units.len());
        if s >= e {
            return JsString { code_units: vec![] };
        }
        JsString {
            code_units: self.code_units[s..e].to_vec(),
        }
    }

    // §6.1.4.2 StringLastIndexOf
    pub fn last_index_of(&self, search: &JsString, from: usize) -> Option<usize> {
        let s_len = self.code_units.len();
        let search_len = search.code_units.len();
        if search_len == 0 {
            return Some(from.min(s_len));
        }
        if search_len > s_len {
            return None;
        }
        let max_start = from.min(s_len - search_len);
        (0..=max_start)
            .rev()
            .find(|&i| self.code_units[i..i + search_len] == search.code_units[..])
    }
}

impl fmt::Display for JsString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_rust_string())
    }
}

#[derive(Clone, Debug)]
pub struct JsSymbol {
    pub id: u64,
    pub description: Option<JsString>,
}

impl JsSymbol {
    /// Convert to the internal property key string.
    /// Well-known symbols (description starts with "Symbol.") use a stable format
    /// without id, so hardcoded lookups like "Symbol(Symbol.iterator)" still work.
    /// User-created symbols include the unique id to avoid collisions.
    pub fn to_property_key(&self) -> String {
        match &self.description {
            Some(desc) if desc.to_string().starts_with("Symbol.") => {
                format!("Symbol({})", desc)
            }
            Some(desc) => format!("Symbol({})#{}", desc, self.id),
            None => format!("Symbol()#{}", self.id),
        }
    }
}

// Well-known symbols (§6.1.5.1)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WellKnownSymbol {
    AsyncIterator,
    HasInstance,
    IsConcatSpreadable,
    Iterator,
    Match,
    MatchAll,
    Replace,
    Search,
    Species,
    Split,
    ToPrimitive,
    ToStringTag,
    Unscopables,
    Dispose,
    AsyncDispose,
}

#[derive(Clone, Debug)]
pub struct JsBigInt {
    pub value: num_bigint::BigInt,
}

// Placeholder — full object model comes in Phase 5
#[derive(Clone, Debug)]
pub struct JsObject {
    pub id: u64,
}

// §6.1.6.1 — Number type operations
impl JsValue {
    pub fn is_undefined(&self) -> bool {
        matches!(self, JsValue::Undefined)
    }

    pub fn is_null(&self) -> bool {
        matches!(self, JsValue::Null)
    }

    pub fn is_boolean(&self) -> bool {
        matches!(self, JsValue::Boolean(_))
    }

    pub fn is_number(&self) -> bool {
        matches!(self, JsValue::Number(_))
    }

    pub fn is_string(&self) -> bool {
        matches!(self, JsValue::String(_))
    }

    pub fn is_symbol(&self) -> bool {
        matches!(self, JsValue::Symbol(_))
    }

    pub fn is_bigint(&self) -> bool {
        matches!(self, JsValue::BigInt(_))
    }

    pub fn is_object(&self) -> bool {
        matches!(self, JsValue::Object(_))
    }

    pub fn is_nullish(&self) -> bool {
        matches!(self, JsValue::Undefined | JsValue::Null)
    }

    pub fn is_nan(&self) -> bool {
        matches!(self, JsValue::Number(n) if n.is_nan())
    }
}

// §6.1.6.1 Number type operations
pub mod number_ops {
    pub fn unary_minus(x: f64) -> f64 {
        if x.is_nan() { f64::NAN } else { -x }
    }

    pub fn bitwise_not(x: f64) -> f64 {
        let n = to_int32(x);
        f64::from(!n)
    }

    pub fn exponentiate(base: f64, exp: f64) -> f64 {
        base.powf(exp)
    }

    pub fn multiply(x: f64, y: f64) -> f64 {
        x * y
    }

    pub fn divide(x: f64, y: f64) -> f64 {
        x / y
    }

    pub fn remainder(x: f64, y: f64) -> f64 {
        // IEEE 754 remainder
        x % y
    }

    pub fn add(x: f64, y: f64) -> f64 {
        x + y
    }

    pub fn subtract(x: f64, y: f64) -> f64 {
        x - y
    }

    pub fn left_shift(x: f64, y: f64) -> f64 {
        let lnum = to_int32(x);
        let rnum = to_uint32(y);
        let shift = rnum & 0x1F;
        f64::from(lnum.wrapping_shl(shift))
    }

    pub fn signed_right_shift(x: f64, y: f64) -> f64 {
        let lnum = to_int32(x);
        let rnum = to_uint32(y);
        let shift = rnum & 0x1F;
        f64::from(lnum.wrapping_shr(shift))
    }

    pub fn unsigned_right_shift(x: f64, y: f64) -> f64 {
        let lnum = to_uint32(x);
        let rnum = to_uint32(y);
        let shift = rnum & 0x1F;
        lnum.wrapping_shr(shift) as f64
    }

    pub fn less_than(x: f64, y: f64) -> Option<bool> {
        if x.is_nan() || y.is_nan() {
            None // undefined
        } else {
            Some(x < y)
        }
    }

    pub fn equal(x: f64, y: f64) -> bool {
        if x.is_nan() || y.is_nan() {
            return false;
        }
        x == y
    }

    pub fn same_value(x: f64, y: f64) -> bool {
        if x.is_nan() && y.is_nan() {
            return true;
        }
        if x == 0.0 && y == 0.0 {
            return x.is_sign_positive() == y.is_sign_positive();
        }
        x == y
    }

    pub fn same_value_zero(x: f64, y: f64) -> bool {
        if x.is_nan() && y.is_nan() {
            return true;
        }
        x == y
    }

    pub fn bitwise_and(x: f64, y: f64) -> f64 {
        f64::from(to_int32(x) & to_int32(y))
    }

    pub fn bitwise_xor(x: f64, y: f64) -> f64 {
        f64::from(to_int32(x) ^ to_int32(y))
    }

    pub fn bitwise_or(x: f64, y: f64) -> f64 {
        f64::from(to_int32(x) | to_int32(y))
    }

    pub fn to_string(x: f64) -> String {
        if x.is_nan() {
            return "NaN".to_string();
        }
        if x == 0.0 {
            return "0".to_string();
        }
        if x.is_infinite() {
            return if x > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
        }
        // Use ryu for spec-compliant shortest representation
        let mut buf = ryu_js::Buffer::new();
        buf.format(x).to_string()
    }

    // §7.1.6 ToInt32
    pub fn to_int32(x: f64) -> i32 {
        if x.is_nan() || x.is_infinite() || x == 0.0 {
            return 0;
        }
        let int_val = x.trunc();
        (int_val as i64 as u32) as i32
    }

    // §7.1.7 ToUint32
    pub fn to_uint32(x: f64) -> u32 {
        if x.is_nan() || x.is_infinite() || x == 0.0 {
            return 0;
        }
        let int_val = x.trunc();
        int_val as i64 as u32
    }
}

// §6.1.6.2 BigInt type operations
pub mod bigint_ops {
    use num_bigint::BigInt;

    pub fn unary_minus(x: &BigInt) -> BigInt {
        -x
    }

    pub fn bitwise_not(x: &BigInt) -> BigInt {
        // ~x = -(x + 1) for arbitrary precision
        let result: BigInt = x + 1;
        -result
    }

    pub fn exponentiate(base: &BigInt, exp: &BigInt) -> Result<BigInt, &'static str> {
        use num_bigint::Sign;
        if exp.sign() == Sign::Minus {
            return Err("BigInt exponent must be non-negative");
        }
        let exp_u32: u32 = exp.try_into().map_err(|_| "BigInt exponent too large")?;
        Ok(base.pow(exp_u32))
    }

    pub fn multiply(x: &BigInt, y: &BigInt) -> BigInt {
        x * y
    }

    pub fn divide(x: &BigInt, y: &BigInt) -> Result<BigInt, &'static str> {
        if y.sign() == num_bigint::Sign::NoSign {
            return Err("Division by zero");
        }
        Ok(x / y)
    }

    pub fn remainder(x: &BigInt, y: &BigInt) -> Result<BigInt, &'static str> {
        if y.sign() == num_bigint::Sign::NoSign {
            return Err("Division by zero");
        }
        Ok(x % y)
    }

    pub fn add(x: &BigInt, y: &BigInt) -> BigInt {
        x + y
    }

    pub fn subtract(x: &BigInt, y: &BigInt) -> BigInt {
        x - y
    }

    pub fn left_shift(x: &BigInt, y: &BigInt) -> BigInt {
        let shift: i64 = y.try_into().unwrap_or(0);
        if shift >= 0 {
            x << (shift as u64)
        } else {
            x >> ((-shift) as u64)
        }
    }

    pub fn signed_right_shift(x: &BigInt, y: &BigInt) -> BigInt {
        let shift: i64 = y.try_into().unwrap_or(0);
        if shift >= 0 {
            x >> (shift as u64)
        } else {
            x << ((-shift) as u64)
        }
    }

    pub fn unsigned_right_shift(_x: &BigInt, _y: &BigInt) -> Result<BigInt, &'static str> {
        Err("Cannot use >>> on BigInt")
    }

    pub fn less_than(x: &BigInt, y: &BigInt) -> Option<bool> {
        Some(x < y)
    }

    pub fn equal(x: &BigInt, y: &BigInt) -> bool {
        x == y
    }

    pub fn bitwise_and(x: &BigInt, y: &BigInt) -> BigInt {
        x & y
    }

    pub fn bitwise_xor(x: &BigInt, y: &BigInt) -> BigInt {
        x ^ y
    }

    pub fn bitwise_or(x: &BigInt, y: &BigInt) -> BigInt {
        x | y
    }

    pub fn to_string(x: &BigInt) -> String {
        x.to_string()
    }
}

impl fmt::Display for JsValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsValue::Undefined => write!(f, "undefined"),
            JsValue::Null => write!(f, "null"),
            JsValue::Boolean(b) => write!(f, "{b}"),
            JsValue::Number(n) => write!(f, "{}", number_ops::to_string(*n)),
            JsValue::String(s) => write!(f, "{s}"),
            JsValue::Symbol(s) => {
                if let Some(desc) = &s.description {
                    write!(f, "Symbol({desc})")
                } else {
                    write!(f, "Symbol()")
                }
            }
            JsValue::BigInt(b) => write!(f, "{}n", b.value),
            JsValue::Object(_) => write!(f, "[object Object]"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_string_index_of() {
        let s = JsString::from_str("hello world");
        let search = JsString::from_str("world");
        assert_eq!(s.index_of(&search, 0), Some(6));
        assert_eq!(s.index_of(&search, 7), None);

        let empty = JsString::from_str("");
        assert_eq!(s.index_of(&empty, 5), Some(5));
    }

    #[test]
    fn js_string_last_index_of() {
        let s = JsString::from_str("abcabc");
        let search = JsString::from_str("abc");
        assert_eq!(s.last_index_of(&search, 5), Some(3));
        assert_eq!(s.last_index_of(&search, 2), Some(0));
    }

    #[test]
    fn number_special_values() {
        assert_eq!(number_ops::to_string(f64::NAN), "NaN");
        assert_eq!(number_ops::to_string(0.0), "0");
        assert_eq!(number_ops::to_string(-0.0), "0");
        assert_eq!(number_ops::to_string(f64::INFINITY), "Infinity");
        assert_eq!(number_ops::to_string(f64::NEG_INFINITY), "-Infinity");
    }

    #[test]
    fn number_same_value() {
        assert!(number_ops::same_value(f64::NAN, f64::NAN));
        assert!(!number_ops::same_value(0.0, -0.0));
        assert!(number_ops::same_value(0.0, 0.0));
    }

    #[test]
    fn number_same_value_zero() {
        assert!(number_ops::same_value_zero(f64::NAN, f64::NAN));
        assert!(number_ops::same_value_zero(0.0, -0.0));
    }

    #[test]
    fn to_int32_basics() {
        assert_eq!(number_ops::to_int32(f64::NAN), 0);
        assert_eq!(number_ops::to_int32(f64::INFINITY), 0);
        assert_eq!(number_ops::to_int32(0.0), 0);
        assert_eq!(number_ops::to_int32(42.9), 42);
        assert_eq!(number_ops::to_int32(-42.9), -42);
    }

    #[test]
    fn bitwise_ops() {
        assert_eq!(number_ops::bitwise_and(15.0, 9.0), 9.0);
        assert_eq!(number_ops::bitwise_or(15.0, 9.0), 15.0);
        assert_eq!(number_ops::bitwise_xor(15.0, 9.0), 6.0);
        assert_eq!(number_ops::bitwise_not(0.0), -1.0);
    }

    #[test]
    fn shift_ops() {
        assert_eq!(number_ops::left_shift(1.0, 4.0), 16.0);
        assert_eq!(number_ops::signed_right_shift(16.0, 2.0), 4.0);
        assert_eq!(number_ops::unsigned_right_shift(-1.0, 0.0), 4294967295.0);
    }

    #[test]
    fn bigint_basic_ops() {
        use num_bigint::BigInt;
        let a = BigInt::from(10);
        let b = BigInt::from(3);
        assert_eq!(bigint_ops::add(&a, &b), BigInt::from(13));
        assert_eq!(bigint_ops::subtract(&a, &b), BigInt::from(7));
        assert_eq!(bigint_ops::multiply(&a, &b), BigInt::from(30));
        assert_eq!(bigint_ops::divide(&a, &b).unwrap(), BigInt::from(3));
        assert_eq!(bigint_ops::remainder(&a, &b).unwrap(), BigInt::from(1));
        assert_eq!(bigint_ops::unary_minus(&a), BigInt::from(-10));
    }

    #[test]
    fn bigint_bitwise_ops() {
        use num_bigint::BigInt;
        let a = BigInt::from(15);
        let b = BigInt::from(9);
        assert_eq!(bigint_ops::bitwise_and(&a, &b), BigInt::from(9));
        assert_eq!(bigint_ops::bitwise_or(&a, &b), BigInt::from(15));
        assert_eq!(bigint_ops::bitwise_xor(&a, &b), BigInt::from(6));
        assert_eq!(bigint_ops::bitwise_not(&BigInt::from(0)), BigInt::from(-1));
    }

    #[test]
    fn bigint_shift_ops() {
        use num_bigint::BigInt;
        assert_eq!(
            bigint_ops::left_shift(&BigInt::from(1), &BigInt::from(4)),
            BigInt::from(16)
        );
        assert_eq!(
            bigint_ops::signed_right_shift(&BigInt::from(16), &BigInt::from(2)),
            BigInt::from(4)
        );
        assert!(bigint_ops::unsigned_right_shift(&BigInt::from(1), &BigInt::from(1)).is_err());
    }

    #[test]
    fn bigint_exponentiate() {
        use num_bigint::BigInt;
        assert_eq!(
            bigint_ops::exponentiate(&BigInt::from(2), &BigInt::from(10)).unwrap(),
            BigInt::from(1024)
        );
        assert!(bigint_ops::exponentiate(&BigInt::from(2), &BigInt::from(-1)).is_err());
    }

    #[test]
    fn bigint_comparison() {
        use num_bigint::BigInt;
        assert_eq!(
            bigint_ops::less_than(&BigInt::from(1), &BigInt::from(2)),
            Some(true)
        );
        assert!(bigint_ops::equal(&BigInt::from(5), &BigInt::from(5)));
        assert!(!bigint_ops::equal(&BigInt::from(5), &BigInt::from(6)));
    }

    #[test]
    fn bigint_division_by_zero() {
        use num_bigint::BigInt;
        assert!(bigint_ops::divide(&BigInt::from(1), &BigInt::from(0)).is_err());
        assert!(bigint_ops::remainder(&BigInt::from(1), &BigInt::from(0)).is_err());
    }

    #[test]
    fn display_values() {
        assert_eq!(format!("{}", JsValue::Undefined), "undefined");
        assert_eq!(format!("{}", JsValue::Null), "null");
        assert_eq!(format!("{}", JsValue::Boolean(true)), "true");
        assert_eq!(format!("{}", JsValue::Number(42.0)), "42");
        assert_eq!(
            format!("{}", JsValue::String(JsString::from_str("hi"))),
            "hi"
        );
    }
}
