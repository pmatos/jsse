use std::borrow::Borrow;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

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

/// Eight-way value tag, used by sites that need exhaustive enum dispatch
/// while remaining decoupled from the underlying `JsValue` representation.
/// The future NaN-boxed `JsValue` (issue #69) will continue to expose this
/// kind via `JsValue::discriminant()` so sites like `Display`,
/// `JSON.stringify`, and `strict_equality` keep compile-time exhaustiveness.
// Consumers land in follow-up #69 NaN-box migration PRs.
#[allow(dead_code)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum ValueKind {
    Undefined,
    Null,
    Boolean,
    Number,
    String,
    Symbol,
    BigInt,
    Object,
}

// UTF-16 code unit string per spec §6.1.4
// Uses Arc<Vec<u16>> so cloning (e.g. env.get) is O(1).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct JsString {
    pub code_units: Arc<Vec<u16>>,
}

impl JsString {
    pub fn from_str(s: &str) -> Self {
        Self {
            code_units: Arc::new(s.encode_utf16().collect()),
        }
    }

    pub fn from_vec(v: Vec<u16>) -> Self {
        Self {
            code_units: Arc::new(v),
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

    /// Get mutable access to code_units, cloning only if shared.
    /// Take ownership of the inner Vec (clones if shared).
    pub fn into_vec(self) -> Vec<u16> {
        Arc::try_unwrap(self.code_units).unwrap_or_else(|arc| (*arc).clone())
    }
}

impl fmt::Display for JsString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_rust_string())
    }
}

/// Exact internal representation of an ECMAScript property key.
///
/// String keys are stored as canonical WTF-8: well-formed UTF-16 has its usual
/// UTF-8 encoding, while lone surrogates use the corresponding three-byte
/// WTF-8 sequence. This makes ordinary Rust `str` keys directly borrowable as
/// bytes while retaining every possible ECMAScript String value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct JsPropertyKey {
    bytes: Arc<[u8]>,
}

pub enum JsPropertyKeyParseError<E> {
    IllFormedUtf16,
    Value(E),
}

impl JsPropertyKey {
    pub fn from_str(s: &str) -> Self {
        Self {
            bytes: Arc::from(s.as_bytes()),
        }
    }

    pub fn from_js_string(s: &JsString) -> Self {
        let units = &s.code_units;
        let mut bytes = Vec::with_capacity(units.len() * 3);
        let mut i = 0;
        while i < units.len() {
            let unit = units[i];
            if (0xD800..=0xDBFF).contains(&unit)
                && i + 1 < units.len()
                && (0xDC00..=0xDFFF).contains(&units[i + 1])
            {
                let code_point =
                    ((unit as u32 - 0xD800) << 10) + (units[i + 1] as u32 - 0xDC00) + 0x10000;
                bytes.push((0xF0 | (code_point >> 18)) as u8);
                bytes.push((0x80 | ((code_point >> 12) & 0x3F)) as u8);
                bytes.push((0x80 | ((code_point >> 6) & 0x3F)) as u8);
                bytes.push((0x80 | (code_point & 0x3F)) as u8);
                i += 2;
            } else if unit < 0x80 {
                bytes.push(unit as u8);
                i += 1;
            } else if unit < 0x800 {
                bytes.push((0xC0 | (unit >> 6)) as u8);
                bytes.push((0x80 | (unit & 0x3F)) as u8);
                i += 1;
            } else {
                // This is ordinary three-byte UTF-8 for BMP scalars and the
                // canonical WTF-8 encoding for a lone surrogate.
                bytes.push((0xE0 | (unit >> 12)) as u8);
                bytes.push((0x80 | ((unit >> 6) & 0x3F)) as u8);
                bytes.push((0x80 | (unit & 0x3F)) as u8);
                i += 1;
            }
        }
        Self {
            bytes: Arc::from(bytes),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.bytes).ok()
    }

    pub fn eq_str(&self, other: &str) -> bool {
        self.bytes.as_ref() == other.as_bytes()
    }

    pub fn starts_with(&self, prefix: &str) -> bool {
        self.bytes.starts_with(prefix.as_bytes())
    }

    pub fn parse<T: FromStr>(&self) -> Result<T, JsPropertyKeyParseError<T::Err>> {
        let text = self
            .as_str()
            .ok_or(JsPropertyKeyParseError::IllFormedUtf16)?;
        text.parse().map_err(JsPropertyKeyParseError::Value)
    }

    #[cfg(test)]
    pub(crate) fn shares_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.bytes, &other.bytes)
    }

    pub fn to_js_string(&self) -> JsString {
        let mut units = Vec::with_capacity(self.bytes.len());
        let mut i = 0;
        while i < self.bytes.len() {
            let first = self.bytes[i];
            if first < 0x80 {
                units.push(first as u16);
                i += 1;
            } else if first < 0xE0 {
                debug_assert!(i + 1 < self.bytes.len());
                let code_point = ((first as u32 & 0x1F) << 6) | (self.bytes[i + 1] as u32 & 0x3F);
                units.push(code_point as u16);
                i += 2;
            } else if first < 0xF0 {
                debug_assert!(i + 2 < self.bytes.len());
                let code_point = ((first as u32 & 0x0F) << 12)
                    | ((self.bytes[i + 1] as u32 & 0x3F) << 6)
                    | (self.bytes[i + 2] as u32 & 0x3F);
                units.push(code_point as u16);
                i += 3;
            } else {
                debug_assert!(i + 3 < self.bytes.len());
                let code_point = ((first as u32 & 0x07) << 18)
                    | ((self.bytes[i + 1] as u32 & 0x3F) << 12)
                    | ((self.bytes[i + 2] as u32 & 0x3F) << 6)
                    | (self.bytes[i + 3] as u32 & 0x3F);
                let offset = code_point - 0x10000;
                units.push((0xD800 + (offset >> 10)) as u16);
                units.push((0xDC00 + (offset & 0x3FF)) as u16);
                i += 4;
            }
        }
        JsString::from_vec(units)
    }
}

impl Borrow<[u8]> for JsPropertyKey {
    fn borrow(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl From<&str> for JsPropertyKey {
    fn from(value: &str) -> Self {
        Self::from_str(value)
    }
}

impl From<String> for JsPropertyKey {
    fn from(value: String) -> Self {
        Self {
            bytes: Arc::from(value.into_bytes()),
        }
    }
}

impl From<JsString> for JsPropertyKey {
    fn from(value: JsString) -> Self {
        Self::from_js_string(&value)
    }
}

impl fmt::Display for JsPropertyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_js_string())
    }
}

pub trait PropertyKeyLike {
    fn as_property_key_bytes(&self) -> &[u8];

    fn as_property_key_str(&self) -> Option<&str> {
        std::str::from_utf8(self.as_property_key_bytes()).ok()
    }

    fn to_js_property_key(&self) -> JsPropertyKey;
}

impl PropertyKeyLike for str {
    fn as_property_key_bytes(&self) -> &[u8] {
        self.as_bytes()
    }

    fn as_property_key_str(&self) -> Option<&str> {
        Some(self)
    }

    fn to_js_property_key(&self) -> JsPropertyKey {
        JsPropertyKey::from_str(self)
    }
}

impl PropertyKeyLike for String {
    fn as_property_key_bytes(&self) -> &[u8] {
        self.as_bytes()
    }

    fn as_property_key_str(&self) -> Option<&str> {
        Some(self)
    }

    fn to_js_property_key(&self) -> JsPropertyKey {
        JsPropertyKey::from_str(self)
    }
}

impl PropertyKeyLike for JsPropertyKey {
    fn as_property_key_bytes(&self) -> &[u8] {
        self.as_bytes()
    }

    fn as_property_key_str(&self) -> Option<&str> {
        self.as_str()
    }

    fn to_js_property_key(&self) -> JsPropertyKey {
        self.clone()
    }
}

impl<T: PropertyKeyLike + ?Sized> PropertyKeyLike for &T {
    fn as_property_key_bytes(&self) -> &[u8] {
        (*self).as_property_key_bytes()
    }

    fn as_property_key_str(&self) -> Option<&str> {
        (*self).as_property_key_str()
    }

    fn to_js_property_key(&self) -> JsPropertyKey {
        (*self).to_js_property_key()
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

#[derive(Clone, Debug)]
pub struct JsBigInt {
    pub value: num_bigint::BigInt,
}

// Placeholder — full object model comes in Phase 5
#[derive(Clone, Debug)]
pub struct JsObject {
    pub id: u64,
}

// Constructor / accessor surface for `JsValue`. The methods here are
// representation-neutral: a future NaN-boxed storage (issue #69) will
// re-implement them in terms of bit operations while keeping the same
// signatures, so callers do not need to change.
// Consumers land in follow-up #69 NaN-box migration PRs.
#[allow(dead_code)]
impl JsValue {
    pub const UNDEFINED: JsValue = JsValue::Undefined;
    pub const NULL: JsValue = JsValue::Null;
    pub const TRUE: JsValue = JsValue::Boolean(true);
    pub const FALSE: JsValue = JsValue::Boolean(false);

    pub fn boolean(b: bool) -> Self {
        JsValue::Boolean(b)
    }

    /// Construct a Number value. NaN canonicalisation lands here in Phase 3
    /// (issue #69) — for now this is a thin wrapper.
    pub fn number(n: f64) -> Self {
        JsValue::Number(n)
    }

    pub fn string(s: JsString) -> Self {
        JsValue::String(s)
    }

    /// Sugar for `JsValue::string(JsString::from_str(s))`.
    pub fn from_str(s: &str) -> Self {
        JsValue::String(JsString::from_str(s))
    }

    pub fn symbol(s: JsSymbol) -> Self {
        JsValue::Symbol(s)
    }

    pub fn bigint(b: JsBigInt) -> Self {
        JsValue::BigInt(b)
    }

    pub fn object(id: u64) -> Self {
        JsValue::Object(JsObject { id })
    }

    // ----- typed accessors --------------------------------------------------
    // Copy-typed payloads return by value. Heap-payload variants (String,
    // Symbol, BigInt) provide both a clone-returning form (`as_string` etc.)
    // and a callback-borrowing form (`with_string` etc.). Under the future
    // NaN-box layout (issue #69), borrow-returning accessors of the form
    // `&JsString` are unsound (no Rust-level borrowee exists), so the
    // `with_*` form is the only zero-refcount-bump path.

    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            JsValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            JsValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_object_id(&self) -> Option<u64> {
        match self {
            JsValue::Object(o) => Some(o.id),
            _ => None,
        }
    }

    /// Cloning accessor — under the future NaN-box this becomes an Arc
    /// refcount bump, so it stays O(1).
    pub fn as_string(&self) -> Option<JsString> {
        match self {
            JsValue::String(s) => Some(s.clone()),
            _ => None,
        }
    }

    pub fn as_symbol(&self) -> Option<JsSymbol> {
        match self {
            JsValue::Symbol(s) => Some(s.clone()),
            _ => None,
        }
    }

    pub fn as_bigint(&self) -> Option<JsBigInt> {
        match self {
            JsValue::BigInt(b) => Some(b.clone()),
            _ => None,
        }
    }

    pub fn with_string<R>(&self, f: impl FnOnce(&[u16]) -> R) -> Option<R> {
        match self {
            JsValue::String(s) => Some(f(&s.code_units)),
            _ => None,
        }
    }

    pub fn with_symbol<R>(&self, f: impl FnOnce(&JsSymbol) -> R) -> Option<R> {
        match self {
            JsValue::Symbol(s) => Some(f(s)),
            _ => None,
        }
    }

    pub fn with_bigint<R>(&self, f: impl FnOnce(&num_bigint::BigInt) -> R) -> Option<R> {
        match self {
            JsValue::BigInt(b) => Some(f(&b.value)),
            _ => None,
        }
    }

    pub fn into_string(self) -> Option<JsString> {
        match self {
            JsValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn into_bigint(self) -> Option<JsBigInt> {
        match self {
            JsValue::BigInt(b) => Some(b),
            _ => None,
        }
    }

    /// Eight-way value tag for exhaustive dispatch. See `ValueKind`.
    pub fn discriminant(&self) -> ValueKind {
        match self {
            JsValue::Undefined => ValueKind::Undefined,
            JsValue::Null => ValueKind::Null,
            JsValue::Boolean(_) => ValueKind::Boolean,
            JsValue::Number(_) => ValueKind::Number,
            JsValue::String(_) => ValueKind::String,
            JsValue::Symbol(_) => ValueKind::Symbol,
            JsValue::BigInt(_) => ValueKind::BigInt,
            JsValue::Object(_) => ValueKind::Object,
        }
    }

    /// Alias for `discriminant()` — the canonical `ValueKind` accessor.
    pub fn kind(&self) -> ValueKind {
        self.discriminant()
    }

    pub fn is_object(&self) -> bool {
        matches!(self, JsValue::Object(_))
    }
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

    pub fn is_nullish(&self) -> bool {
        matches!(self, JsValue::Undefined | JsValue::Null)
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
        // §6.1.6.1.4 step 3: if exponent is NaN, return NaN
        if exp.is_nan() {
            return f64::NAN;
        }
        // §6.1.6.1.4 step 10: if abs(base) is 1 and exponent is +/-∞, return NaN
        if (base == 1.0 || base == -1.0) && exp.is_infinite() {
            return f64::NAN;
        }
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

    // §7.1.7 ToUint32 — reduce the truncated real value modulo 2^32. The modular
    // step is done in f64 (exact for integer-valued doubles) so it stays correct
    // for magnitudes beyond the i64 range, where an `as i64` cast would saturate.
    pub fn to_uint32(x: f64) -> u32 {
        if !x.is_finite() || x == 0.0 {
            return 0;
        }
        let int_val = x.trunc();
        let modulo = int_val % 4294967296.0; // 2^32
        let int32bit = if modulo < 0.0 {
            modulo + 4294967296.0
        } else {
            modulo
        };
        int32bit as u32
    }

    // §7.1.6 ToInt32 — the same int32bit as ToUint32, reinterpreted as signed.
    pub fn to_int32(x: f64) -> i32 {
        to_uint32(x) as i32
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
    fn to_int32_basics() {
        assert_eq!(number_ops::to_int32(f64::NAN), 0);
        assert_eq!(number_ops::to_int32(f64::INFINITY), 0);
        assert_eq!(number_ops::to_int32(0.0), 0);
        assert_eq!(number_ops::to_int32(42.9), 42);
        assert_eq!(number_ops::to_int32(-42.9), -42);
    }

    // §7.1.6 ToInt32 / §7.1.7 ToUint32 — spec-correct modular reduction over the
    // full f64 range. Expected values cross-checked against Node (`x | 0` and
    // `x >>> 0`). The large-magnitude cases (>= 2^63) are the ones a saturating
    // `as i64` cast gets wrong.
    #[test]
    fn to_uint32_spec_values() {
        // NaN / +/-Inf / +/-0 -> +0
        assert_eq!(number_ops::to_uint32(f64::NAN), 0);
        assert_eq!(number_ops::to_uint32(f64::INFINITY), 0);
        assert_eq!(number_ops::to_uint32(f64::NEG_INFINITY), 0);
        assert_eq!(number_ops::to_uint32(-0.0), 0);
        // truncation toward zero
        assert_eq!(number_ops::to_uint32(3.9), 3);
        assert_eq!(number_ops::to_uint32(-2.5), 4294967294);
        // around the 2^31 / 2^32 boundaries
        assert_eq!(number_ops::to_uint32(-1.0), 4294967295);
        assert_eq!(number_ops::to_uint32(2147483648.0), 2147483648); // 2^31
        assert_eq!(number_ops::to_uint32(4294967295.0), 4294967295); // 2^32-1
        assert_eq!(number_ops::to_uint32(4294967296.0), 0); // 2^32
        assert_eq!(number_ops::to_uint32(4294967301.0), 5); // 2^32+5
        assert_eq!(number_ops::to_uint32(9007199254740992.0), 0); // 2^53
        // large magnitudes beyond i64 range (saturating cast gets these wrong)
        assert_eq!(number_ops::to_uint32(9223372036854775808.0), 0); // 2^63
        assert_eq!(number_ops::to_uint32(18446744073709551616.0), 0); // 2^64
        assert_eq!(number_ops::to_uint32(-9223372036854775808.0), 0); // -(2^63)
        assert_eq!(number_ops::to_uint32(1e21), 3735027712);
    }

    #[test]
    fn to_int32_spec_values() {
        assert_eq!(number_ops::to_int32(-1.0), -1);
        assert_eq!(number_ops::to_int32(2147483647.0), 2147483647); // 2^31-1
        assert_eq!(number_ops::to_int32(2147483648.0), -2147483648); // 2^31 wraps
        assert_eq!(number_ops::to_int32(4294967295.0), -1); // 2^32-1
        assert_eq!(number_ops::to_int32(4294967296.0), 0); // 2^32
        assert_eq!(number_ops::to_int32(4294967301.0), 5); // 2^32+5
        assert_eq!(number_ops::to_int32(9007199254740992.0), 0); // 2^53
        // large magnitudes beyond i64 range (saturating cast gets these wrong)
        assert_eq!(number_ops::to_int32(9223372036854775808.0), 0); // 2^63
        assert_eq!(number_ops::to_int32(18446744073709551616.0), 0); // 2^64
        assert_eq!(number_ops::to_int32(-9223372036854775808.0), 0); // -(2^63)
        assert_eq!(number_ops::to_int32(1e21), -559939584);
    }

    #[test]
    fn bitwise_and_shift_large_values() {
        // The bitwise/shift operators feed operands through ToInt32/ToUint32, so
        // large magnitudes must reduce modulo 2^32 (cross-checked against Node).
        assert_eq!(number_ops::bitwise_or(18446744073709551616.0, 0.0), 0.0); // (2^64)|0
        assert_eq!(number_ops::bitwise_or(1e21, 0.0), -559939584.0); // (1e21)|0
        assert_eq!(number_ops::bitwise_and(4294967301.0, 4294967295.0), 5.0);
        assert_eq!(
            number_ops::unsigned_right_shift(18446744073709551616.0, 0.0),
            0.0
        );
        assert_eq!(number_ops::unsigned_right_shift(1e21, 0.0), 3735027712.0);
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

    #[test]
    fn property_key_wtf8_round_trips_all_utf16_shapes() {
        let units = vec![0x0061, 0xD834, 0x0062, 0xDF06, 0xD834, 0xDF06];
        let key = JsPropertyKey::from_js_string(&JsString::from_vec(units.clone()));
        assert_eq!(&*key.to_js_string().code_units, &units);
        assert!(key.as_str().is_none(), "lone surrogates are not UTF-8");
    }

    #[test]
    fn property_key_well_formed_text_keeps_utf8_bytes() {
        let text = "plain-𝌆";
        let key = JsPropertyKey::from_js_string(&JsString::from_str(text));
        assert_eq!(key.as_bytes(), text.as_bytes());
        assert_eq!(key.as_str(), Some(text));
        assert_eq!(key.to_js_string(), JsString::from_str(text));
    }

    #[test]
    fn property_key_lone_surrogates_do_not_collide_with_replacement() {
        let replacement = JsPropertyKey::from_str("\u{FFFD}");
        let high = JsPropertyKey::from_js_string(&JsString::from_vec(vec![0xD834]));
        let low = JsPropertyKey::from_js_string(&JsString::from_vec(vec![0xDF06]));
        assert_ne!(replacement, high);
        assert_ne!(replacement, low);
        assert_ne!(high, low);
    }

    // ----- JsValue method surface (issue #69 NaN-box migration) -------------

    #[test]
    fn value_constructors() {
        assert!(matches!(JsValue::UNDEFINED, JsValue::Undefined));
        assert!(matches!(JsValue::NULL, JsValue::Null));
        assert!(matches!(JsValue::TRUE, JsValue::Boolean(true)));
        assert!(matches!(JsValue::FALSE, JsValue::Boolean(false)));
        assert_eq!(JsValue::boolean(true).as_boolean(), Some(true));
        assert_eq!(JsValue::number(3.5).as_number(), Some(3.5));
        assert_eq!(JsValue::object(7).as_object_id(), Some(7));
        assert_eq!(
            JsValue::from_str("hi")
                .as_string()
                .unwrap()
                .to_rust_string(),
            "hi"
        );
        assert_eq!(
            JsValue::string(JsString::from_str("yo"))
                .as_string()
                .unwrap()
                .to_rust_string(),
            "yo"
        );
        let sym = JsSymbol {
            id: 1,
            description: Some(JsString::from_str("s")),
        };
        assert_eq!(JsValue::symbol(sym).as_symbol().unwrap().id, 1);
        let big = JsBigInt {
            value: num_bigint::BigInt::from(42),
        };
        assert_eq!(
            JsValue::bigint(big).as_bigint().unwrap().value,
            num_bigint::BigInt::from(42)
        );
    }

    #[test]
    fn typed_accessors_return_none_on_mismatch() {
        let n = JsValue::Number(1.0);
        assert_eq!(n.as_boolean(), None);
        assert_eq!(n.as_object_id(), None);
        assert!(n.as_string().is_none());
        assert!(n.as_symbol().is_none());
        assert!(n.as_bigint().is_none());
        assert_eq!(JsValue::Boolean(true).as_number(), None);
    }

    #[test]
    fn with_accessors() {
        let s = JsValue::from_str("abc");
        assert_eq!(s.with_string(|cu| cu.len()), Some(3));
        assert_eq!(JsValue::Null.with_string(|cu| cu.len()), None);

        let sym = JsValue::Symbol(JsSymbol {
            id: 9,
            description: None,
        });
        assert_eq!(sym.with_symbol(|s| s.id), Some(9));
        assert_eq!(JsValue::Null.with_symbol(|s| s.id), None);

        let big = JsValue::BigInt(JsBigInt {
            value: num_bigint::BigInt::from(5),
        });
        assert_eq!(
            big.with_bigint(|b| b.clone()),
            Some(num_bigint::BigInt::from(5))
        );
        assert_eq!(JsValue::Null.with_bigint(|b| b.clone()), None);
    }

    #[test]
    fn into_accessors() {
        let s = JsValue::from_str("x");
        assert_eq!(s.into_string().unwrap().to_rust_string(), "x");
        assert!(JsValue::Null.into_string().is_none());

        let big = JsValue::BigInt(JsBigInt {
            value: num_bigint::BigInt::from(11),
        });
        assert_eq!(
            big.into_bigint().unwrap().value,
            num_bigint::BigInt::from(11)
        );
        assert!(JsValue::Number(1.0).into_bigint().is_none());
    }

    #[test]
    fn discriminant_and_kind() {
        let cases = [
            (JsValue::Undefined, ValueKind::Undefined),
            (JsValue::Null, ValueKind::Null),
            (JsValue::Boolean(true), ValueKind::Boolean),
            (JsValue::Number(1.0), ValueKind::Number),
            (JsValue::from_str("s"), ValueKind::String),
            (
                JsValue::Symbol(JsSymbol {
                    id: 0,
                    description: None,
                }),
                ValueKind::Symbol,
            ),
            (
                JsValue::BigInt(JsBigInt {
                    value: num_bigint::BigInt::from(0),
                }),
                ValueKind::BigInt,
            ),
            (JsValue::object(1), ValueKind::Object),
        ];
        for (v, expected) in &cases {
            assert_eq!(v.discriminant(), *expected);
            assert_eq!(v.kind(), *expected);
        }
    }

    #[test]
    fn is_object_predicate() {
        assert!(JsValue::object(3).is_object());
        assert!(!JsValue::Null.is_object());
        assert!(!JsValue::Number(0.0).is_object());
    }
}
