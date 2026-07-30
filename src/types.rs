use std::borrow::Borrow;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

/// One-word ECMAScript value representation.
///
/// Non-matching bit patterns are IEEE-754 Numbers. Negative quiet-NaN
/// patterns carry a three-bit tag and a 48-bit scalar or exposed-pointer
/// payload. Heap payloads own one `Arc` strong reference.
#[repr(transparent)]
pub(crate) struct NanBoxedValue(u64);

pub(crate) type JsValue = NanBoxedValue;

#[repr(u64)]
#[derive(Copy, Clone, Eq, PartialEq)]
enum NanTag {
    Undefined = 0,
    Null = 1,
    False = 2,
    True = 3,
    Object = 4,
    String = 5,
    Symbol = 6,
    BigInt = 7,
}

/// Eight-way value tag, used by sites that need exhaustive enum dispatch
/// while remaining decoupled from the underlying `JsValue` representation.
/// The NaN-boxed `JsValue` exposes this kind via `JsValue::discriminant()` so
/// sites like `Display`,
/// `JSON.stringify`, and `strict_equality` keep compile-time exhaustiveness.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub(crate) enum ValueKind {
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
pub(crate) struct JsString {
    pub code_units: Arc<Vec<u16>>,
}

impl JsString {
    pub(crate) fn from_str(s: &str) -> Self {
        Self {
            code_units: Arc::new(s.encode_utf16().collect()),
        }
    }

    pub(crate) fn from_vec(v: Vec<u16>) -> Self {
        Self {
            code_units: Arc::new(v),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.code_units.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.code_units.len()
    }

    pub(crate) fn to_rust_string(&self) -> String {
        String::from_utf16_lossy(&self.code_units)
    }

    /// Get mutable access to code_units, cloning only if shared.
    /// Take ownership of the inner Vec (clones if shared).
    pub(crate) fn into_vec(self) -> Vec<u16> {
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
/// bytes while retaining every possible ECMAScript String value. Symbol keys
/// carry a leading byte that canonical WTF-8 can never produce, keeping their
/// identity disjoint from every possible String key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct JsPropertyKey {
    bytes: Arc<[u8]>,
}

const SYMBOL_PROPERTY_KEY_SIGIL: u8 = 0xFF;

pub(crate) enum JsPropertyKeyParseError<E> {
    IllFormedUtf16,
    Value(E),
}

impl JsPropertyKey {
    pub(crate) fn from_str(s: &str) -> Self {
        Self {
            bytes: Arc::from(s.as_bytes()),
        }
    }

    pub(crate) fn from_js_string(s: &JsString) -> Self {
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

    fn from_symbol_encoding(encoding: String) -> Self {
        let mut bytes = Vec::with_capacity(encoding.len() + 1);
        bytes.push(SYMBOL_PROPERTY_KEY_SIGIL);
        bytes.extend_from_slice(encoding.as_bytes());
        Self {
            bytes: Arc::from(bytes),
        }
    }

    pub(crate) fn well_known_symbol(name: &str) -> Self {
        Self::from_symbol_encoding(format!("Symbol(Symbol.{name})"))
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn as_str(&self) -> Option<&str> {
        if self.is_symbol() {
            return None;
        }
        std::str::from_utf8(&self.bytes).ok()
    }

    pub(crate) fn is_symbol(&self) -> bool {
        self.bytes.first() == Some(&SYMBOL_PROPERTY_KEY_SIGIL)
    }

    pub(crate) fn symbol_encoding(&self) -> Option<&str> {
        self.is_symbol()
            .then(|| std::str::from_utf8(&self.bytes[1..]).expect("Symbol encoding is UTF-8"))
    }

    pub(crate) fn eq_str(&self, other: &str) -> bool {
        self.bytes.as_ref() == other.as_bytes()
    }

    pub(crate) fn parse<T: FromStr>(&self) -> Result<T, JsPropertyKeyParseError<T::Err>> {
        let text = self
            .as_str()
            .ok_or(JsPropertyKeyParseError::IllFormedUtf16)?;
        text.parse().map_err(JsPropertyKeyParseError::Value)
    }

    #[cfg(test)]
    pub(crate) fn shares_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.bytes, &other.bytes)
    }

    pub(crate) fn to_js_string(&self) -> JsString {
        let bytes = if self.is_symbol() {
            &self.bytes[1..]
        } else {
            &self.bytes
        };
        let mut units = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            let first = bytes[i];
            if first < 0x80 {
                units.push(first as u16);
                i += 1;
            } else if first < 0xE0 {
                debug_assert!(i + 1 < bytes.len());
                let code_point = ((first as u32 & 0x1F) << 6) | (bytes[i + 1] as u32 & 0x3F);
                units.push(code_point as u16);
                i += 2;
            } else if first < 0xF0 {
                debug_assert!(i + 2 < bytes.len());
                let code_point = ((first as u32 & 0x0F) << 12)
                    | ((bytes[i + 1] as u32 & 0x3F) << 6)
                    | (bytes[i + 2] as u32 & 0x3F);
                units.push(code_point as u16);
                i += 3;
            } else {
                debug_assert!(i + 3 < bytes.len());
                let code_point = ((first as u32 & 0x07) << 18)
                    | ((bytes[i + 1] as u32 & 0x3F) << 12)
                    | ((bytes[i + 2] as u32 & 0x3F) << 6)
                    | (bytes[i + 3] as u32 & 0x3F);
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

pub(crate) trait PropertyKeyLike {
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

#[derive(Debug)]
struct JsSymbolData {
    id: u64,
    description: Option<JsString>,
}

#[derive(Clone, Debug)]
pub(crate) struct JsSymbol {
    data: Arc<JsSymbolData>,
}

impl JsSymbol {
    pub(crate) fn new(id: u64, description: Option<JsString>) -> Self {
        Self {
            data: Arc::new(JsSymbolData { id, description }),
        }
    }

    pub(crate) fn id(&self) -> u64 {
        self.data.id
    }

    pub(crate) fn description(&self) -> Option<&JsString> {
        self.data.description.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn shares_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.data, &other.data)
    }

    /// Convert to the internal Symbol-valued property key.
    /// Well-known symbols (description starts with "Symbol.") use a stable format
    /// without id, so bootstrap lookups can construct the same key directly.
    /// User-created symbols include the unique id to avoid collisions.
    pub(crate) fn to_property_key(&self) -> JsPropertyKey {
        let encoding = match self.description() {
            Some(desc) if desc.to_string().starts_with("Symbol.") => {
                format!("Symbol({})", desc)
            }
            Some(desc) => format!("Symbol({})#{}", desc, self.id()),
            None => format!("Symbol()#{}", self.id()),
        };
        JsPropertyKey::from_symbol_encoding(encoding)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct JsBigInt {
    pub value: Arc<num_bigint::BigInt>,
}

impl JsBigInt {
    pub(crate) fn new(value: num_bigint::BigInt) -> Self {
        Self {
            value: Arc::new(value),
        }
    }
}

// Placeholder — full object model comes in Phase 5
#[derive(Clone, Debug)]
pub(crate) struct JsObject {
    pub id: u64,
}

// Constructor / accessor surface for `JsValue`. Callers remain decoupled from
// the NaN-box representation and never manipulate tags or raw pointers.
impl NanBoxedValue {
    const BOX_SIGNATURE: u64 = 0xFFF8_0000_0000_0000;
    const BOX_SIGNATURE_MASK: u64 = 0xFFF8_0000_0000_0000;
    const TAG_SHIFT: u32 = 48;
    const PAYLOAD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;
    const CANONICAL_NAN_BITS: u64 = 0x7FF8_0000_0000_0000;

    pub(crate) const UNDEFINED: JsValue = Self::boxed(NanTag::Undefined, 0);
    pub(crate) const NULL: JsValue = Self::boxed(NanTag::Null, 0);
    pub(crate) const TRUE: JsValue = Self::boxed(NanTag::True, 0);
    pub(crate) const FALSE: JsValue = Self::boxed(NanTag::False, 0);

    /// Stable borrow for APIs that need a default argument by reference.
    pub(crate) fn undefined_ref() -> &'static Self {
        static UNDEFINED: NanBoxedValue = NanBoxedValue::boxed(NanTag::Undefined, 0);
        &UNDEFINED
    }

    const fn boxed(tag: NanTag, payload: u64) -> Self {
        Self(Self::BOX_SIGNATURE | ((tag as u64) << Self::TAG_SHIFT) | payload)
    }

    fn is_boxed(&self) -> bool {
        self.0 & Self::BOX_SIGNATURE_MASK == Self::BOX_SIGNATURE
    }

    fn tag(&self) -> Option<NanTag> {
        if !self.is_boxed() {
            return None;
        }
        Some(match (self.0 >> Self::TAG_SHIFT) & 0b111 {
            0 => NanTag::Undefined,
            1 => NanTag::Null,
            2 => NanTag::False,
            3 => NanTag::True,
            4 => NanTag::Object,
            5 => NanTag::String,
            6 => NanTag::Symbol,
            7 => NanTag::BigInt,
            _ => unreachable!(),
        })
    }

    fn payload(&self) -> u64 {
        self.0 & Self::PAYLOAD_MASK
    }

    #[cfg(test)]
    fn raw_bits(&self) -> u64 {
        self.0
    }

    fn from_arc<T>(tag: NanTag, arc: Arc<T>) -> Self {
        let address = Arc::as_ptr(&arc).expose_provenance();
        Self::from_arc_after_address_check(tag, arc, address)
    }

    fn from_arc_after_address_check<T>(tag: NanTag, arc: Arc<T>, address: usize) -> Self {
        assert!(
            address <= Self::PAYLOAD_MASK as usize,
            "NaN-box heap pointer exceeds the 48-bit payload range"
        );

        // The range check deliberately happens before ownership is transferred.
        // No panicking operations may be added between `into_raw` and boxing.
        let raw = Arc::into_raw(arc);
        let payload = raw.expose_provenance() as u64;
        Self::boxed(tag, payload)
    }

    fn payload_ptr<T>(&self) -> *const T {
        std::ptr::with_exposed_provenance(self.payload() as usize)
    }

    pub(crate) fn boolean(b: bool) -> Self {
        if b { Self::TRUE } else { Self::FALSE }
    }

    /// Construct a Number, canonicalising every NaN away from the boxed range.
    pub(crate) fn number(n: f64) -> Self {
        Self(if n.is_nan() {
            Self::CANONICAL_NAN_BITS
        } else {
            n.to_bits()
        })
    }

    pub(crate) fn string(s: JsString) -> Self {
        Self::from_arc(NanTag::String, s.code_units)
    }

    /// Sugar for `JsValue::string(JsString::from_str(s))`.
    pub(crate) fn from_str(s: &str) -> Self {
        Self::string(JsString::from_str(s))
    }

    pub(crate) fn symbol(s: JsSymbol) -> Self {
        Self::from_arc(NanTag::Symbol, s.data)
    }

    pub(crate) fn bigint(b: JsBigInt) -> Self {
        Self::from_arc(NanTag::BigInt, b.value)
    }

    pub(crate) fn object(id: u64) -> Self {
        assert!(
            id <= Self::PAYLOAD_MASK,
            "NaN-box object id exceeds the 48-bit payload range"
        );
        Self::boxed(NanTag::Object, id)
    }

    // ----- typed accessors --------------------------------------------------
    // Copy-typed payloads return by value. Heap-payload variants (String,
    // Symbol, BigInt) provide both a clone-returning form (`as_string` etc.)
    // and a callback-borrowing form (`with_string` etc.). Borrow-returning
    // accessors of the form `&JsString` would be unsound because no wrapper
    // object lives inside the word.

    pub(crate) fn as_boolean(&self) -> Option<bool> {
        match self.tag() {
            Some(NanTag::False) => Some(false),
            Some(NanTag::True) => Some(true),
            _ => None,
        }
    }

    pub(crate) fn as_number(&self) -> Option<f64> {
        (!self.is_boxed()).then(|| f64::from_bits(self.0))
    }

    pub(crate) fn as_object_id(&self) -> Option<u64> {
        (self.tag() == Some(NanTag::Object)).then(|| self.payload())
    }

    pub(crate) fn as_string(&self) -> Option<JsString> {
        if self.tag() != Some(NanTag::String) {
            return None;
        }
        let ptr = self.payload_ptr::<Vec<u16>>();
        // SAFETY: a String-tagged value owns one strong count for this pointer.
        unsafe {
            Arc::increment_strong_count(ptr);
            Some(JsString {
                code_units: Arc::from_raw(ptr),
            })
        }
    }

    pub(crate) fn as_symbol(&self) -> Option<JsSymbol> {
        if self.tag() != Some(NanTag::Symbol) {
            return None;
        }
        let ptr = self.payload_ptr::<JsSymbolData>();
        // SAFETY: a Symbol-tagged value owns one strong count for this pointer.
        unsafe {
            Arc::increment_strong_count(ptr);
            Some(JsSymbol {
                data: Arc::from_raw(ptr),
            })
        }
    }

    pub(crate) fn as_bigint(&self) -> Option<JsBigInt> {
        if self.tag() != Some(NanTag::BigInt) {
            return None;
        }
        let ptr = self.payload_ptr::<num_bigint::BigInt>();
        // SAFETY: a BigInt-tagged value owns one strong count for this pointer.
        unsafe {
            Arc::increment_strong_count(ptr);
            Some(JsBigInt {
                value: Arc::from_raw(ptr),
            })
        }
    }

    pub(crate) fn with_string<R>(&self, f: impl FnOnce(&[u16]) -> R) -> Option<R> {
        if self.tag() != Some(NanTag::String) {
            return None;
        }
        let ptr = self.payload_ptr::<Vec<u16>>();
        // SAFETY: `self` keeps its strong count alive for the callback.
        Some(f(unsafe { &*ptr }))
    }

    pub(crate) fn with_symbol<R>(&self, f: impl FnOnce(&JsSymbol) -> R) -> Option<R> {
        if self.tag() != Some(NanTag::Symbol) {
            return None;
        }
        let ptr = self.payload_ptr::<JsSymbolData>();
        // SAFETY: `self` owns the strong count represented by this temporary
        // Arc. `ManuallyDrop` prevents both normal and unwind paths from
        // consuming that count.
        let symbol = std::mem::ManuallyDrop::new(JsSymbol {
            data: unsafe { Arc::from_raw(ptr) },
        });
        Some(f(&symbol))
    }

    pub(crate) fn with_bigint<R>(&self, f: impl FnOnce(&num_bigint::BigInt) -> R) -> Option<R> {
        if self.tag() != Some(NanTag::BigInt) {
            return None;
        }
        let ptr = self.payload_ptr::<num_bigint::BigInt>();
        // SAFETY: `self` keeps its strong count alive for the callback.
        Some(f(unsafe { &*ptr }))
    }

    pub(crate) fn into_string(self) -> Option<JsString> {
        if self.tag() != Some(NanTag::String) {
            return None;
        }
        let ptr = self.payload_ptr::<Vec<u16>>();
        std::mem::forget(self);
        // SAFETY: forgetting `self` transfers its one strong count.
        Some(JsString {
            code_units: unsafe { Arc::from_raw(ptr) },
        })
    }

    /// Only exercised by tests today (`with_bigint` covers every production
    /// read path) — kept for symmetry with `into_string` and to keep this
    /// tag's forget/`Arc::from_raw` transfer under test.
    #[allow(dead_code)]
    pub(crate) fn into_bigint(self) -> Option<JsBigInt> {
        if self.tag() != Some(NanTag::BigInt) {
            return None;
        }
        let ptr = self.payload_ptr::<num_bigint::BigInt>();
        std::mem::forget(self);
        // SAFETY: forgetting `self` transfers its one strong count.
        Some(JsBigInt {
            value: unsafe { Arc::from_raw(ptr) },
        })
    }

    /// Eight-way value tag for exhaustive dispatch. See `ValueKind`.
    pub(crate) fn discriminant(&self) -> ValueKind {
        match self.tag() {
            None => ValueKind::Number,
            Some(NanTag::Undefined) => ValueKind::Undefined,
            Some(NanTag::Null) => ValueKind::Null,
            Some(NanTag::False | NanTag::True) => ValueKind::Boolean,
            Some(NanTag::Object) => ValueKind::Object,
            Some(NanTag::String) => ValueKind::String,
            Some(NanTag::Symbol) => ValueKind::Symbol,
            Some(NanTag::BigInt) => ValueKind::BigInt,
        }
    }

    /// Alias for `discriminant()` — the canonical `ValueKind` accessor.
    pub(crate) fn kind(&self) -> ValueKind {
        self.discriminant()
    }

    pub(crate) fn is_object(&self) -> bool {
        self.tag() == Some(NanTag::Object)
    }
}

// §6.1.6.1 — Number type operations
impl NanBoxedValue {
    pub(crate) fn is_undefined(&self) -> bool {
        self.tag() == Some(NanTag::Undefined)
    }

    pub(crate) fn is_null(&self) -> bool {
        self.tag() == Some(NanTag::Null)
    }

    pub(crate) fn is_boolean(&self) -> bool {
        matches!(self.tag(), Some(NanTag::False | NanTag::True))
    }

    pub(crate) fn is_number(&self) -> bool {
        !self.is_boxed()
    }

    pub(crate) fn is_string(&self) -> bool {
        self.tag() == Some(NanTag::String)
    }

    pub(crate) fn is_symbol(&self) -> bool {
        self.tag() == Some(NanTag::Symbol)
    }

    pub(crate) fn is_bigint(&self) -> bool {
        self.tag() == Some(NanTag::BigInt)
    }

    pub(crate) fn is_nullish(&self) -> bool {
        matches!(self.tag(), Some(NanTag::Undefined | NanTag::Null))
    }
}

impl Clone for NanBoxedValue {
    fn clone(&self) -> Self {
        match self.tag() {
            Some(NanTag::String) => {
                let ptr = self.payload_ptr::<Vec<u16>>();
                // SAFETY: this value owns a live strong count for `ptr`.
                unsafe { Arc::increment_strong_count(ptr) };
            }
            Some(NanTag::Symbol) => {
                let ptr = self.payload_ptr::<JsSymbolData>();
                // SAFETY: this value owns a live strong count for `ptr`.
                unsafe { Arc::increment_strong_count(ptr) };
            }
            Some(NanTag::BigInt) => {
                let ptr = self.payload_ptr::<num_bigint::BigInt>();
                // SAFETY: this value owns a live strong count for `ptr`.
                unsafe { Arc::increment_strong_count(ptr) };
            }
            _ => {}
        }
        Self(self.0)
    }
}

impl Drop for NanBoxedValue {
    fn drop(&mut self) {
        match self.tag() {
            Some(NanTag::String) => {
                let ptr = self.payload_ptr::<Vec<u16>>();
                // SAFETY: this value owns exactly one strong count for `ptr`.
                unsafe { drop(Arc::from_raw(ptr)) };
            }
            Some(NanTag::Symbol) => {
                let ptr = self.payload_ptr::<JsSymbolData>();
                // SAFETY: this value owns exactly one strong count for `ptr`.
                unsafe { drop(Arc::from_raw(ptr)) };
            }
            Some(NanTag::BigInt) => {
                let ptr = self.payload_ptr::<num_bigint::BigInt>();
                // SAFETY: this value owns exactly one strong count for `ptr`.
                unsafe { drop(Arc::from_raw(ptr)) };
            }
            _ => {}
        }
    }
}

impl fmt::Debug for NanBoxedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind() {
            ValueKind::Undefined => f.write_str("Undefined"),
            ValueKind::Null => f.write_str("Null"),
            ValueKind::Boolean => f
                .debug_tuple("Boolean")
                .field(&self.as_boolean().unwrap())
                .finish(),
            ValueKind::Number => f
                .debug_tuple("Number")
                .field(&self.as_number().unwrap())
                .finish(),
            ValueKind::String => f
                .debug_tuple("String")
                .field(&self.as_string().unwrap())
                .finish(),
            ValueKind::Symbol => f
                .debug_tuple("Symbol")
                .field(&self.as_symbol().unwrap())
                .finish(),
            ValueKind::BigInt => f
                .debug_tuple("BigInt")
                .field(&self.as_bigint().unwrap())
                .finish(),
            ValueKind::Object => f
                .debug_tuple("Object")
                .field(&JsObject {
                    id: self.as_object_id().unwrap(),
                })
                .finish(),
        }
    }
}

// `NanBoxedValue` is structurally `Send + Sync`; keep its hidden pointee types
// under the same contract so moving the owning word between threads stays sound.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Vec<u16>>();
    assert_send_sync::<JsSymbolData>();
    assert_send_sync::<num_bigint::BigInt>();
};

// §6.1.6.1 Number type operations
pub(crate) mod number_ops {
    pub(crate) fn unary_minus(x: f64) -> f64 {
        if x.is_nan() { f64::NAN } else { -x }
    }

    pub(crate) fn bitwise_not(x: f64) -> f64 {
        let n = to_int32(x);
        f64::from(!n)
    }

    pub(crate) fn exponentiate(base: f64, exp: f64) -> f64 {
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

    pub(crate) fn multiply(x: f64, y: f64) -> f64 {
        x * y
    }

    pub(crate) fn divide(x: f64, y: f64) -> f64 {
        x / y
    }

    pub(crate) fn remainder(x: f64, y: f64) -> f64 {
        // IEEE 754 remainder
        x % y
    }

    pub(crate) fn add(x: f64, y: f64) -> f64 {
        x + y
    }

    pub(crate) fn subtract(x: f64, y: f64) -> f64 {
        x - y
    }

    pub(crate) fn left_shift(x: f64, y: f64) -> f64 {
        let lnum = to_int32(x);
        let rnum = to_uint32(y);
        let shift = rnum & 0x1F;
        f64::from(lnum.wrapping_shl(shift))
    }

    pub(crate) fn signed_right_shift(x: f64, y: f64) -> f64 {
        let lnum = to_int32(x);
        let rnum = to_uint32(y);
        let shift = rnum & 0x1F;
        f64::from(lnum.wrapping_shr(shift))
    }

    pub(crate) fn unsigned_right_shift(x: f64, y: f64) -> f64 {
        let lnum = to_uint32(x);
        let rnum = to_uint32(y);
        let shift = rnum & 0x1F;
        lnum.wrapping_shr(shift) as f64
    }

    pub(crate) fn less_than(x: f64, y: f64) -> Option<bool> {
        if x.is_nan() || y.is_nan() {
            None // undefined
        } else {
            Some(x < y)
        }
    }

    pub(crate) fn equal(x: f64, y: f64) -> bool {
        if x.is_nan() || y.is_nan() {
            return false;
        }
        x == y
    }

    pub(crate) fn same_value(x: f64, y: f64) -> bool {
        if x.is_nan() && y.is_nan() {
            return true;
        }
        if x == 0.0 && y == 0.0 {
            return x.is_sign_positive() == y.is_sign_positive();
        }
        x == y
    }

    pub(crate) fn bitwise_and(x: f64, y: f64) -> f64 {
        f64::from(to_int32(x) & to_int32(y))
    }

    pub(crate) fn bitwise_xor(x: f64, y: f64) -> f64 {
        f64::from(to_int32(x) ^ to_int32(y))
    }

    pub(crate) fn bitwise_or(x: f64, y: f64) -> f64 {
        f64::from(to_int32(x) | to_int32(y))
    }

    pub(crate) fn to_string(x: f64) -> String {
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
    pub(crate) fn to_uint32(x: f64) -> u32 {
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
    pub(crate) fn to_int32(x: f64) -> i32 {
        to_uint32(x) as i32
    }
}

// §6.1.6.2 BigInt type operations
pub(crate) mod bigint_ops {
    use num_bigint::BigInt;

    pub(crate) fn unary_minus(x: &BigInt) -> BigInt {
        -x
    }

    pub(crate) fn bitwise_not(x: &BigInt) -> BigInt {
        // ~x = -(x + 1) for arbitrary precision
        let result: BigInt = x + 1;
        -result
    }

    pub(crate) fn exponentiate(base: &BigInt, exp: &BigInt) -> Result<BigInt, &'static str> {
        use num_bigint::Sign;
        if exp.sign() == Sign::Minus {
            return Err("BigInt exponent must be non-negative");
        }
        let exp_u32: u32 = exp.try_into().map_err(|_| "BigInt exponent too large")?;
        Ok(base.pow(exp_u32))
    }

    pub(crate) fn multiply(x: &BigInt, y: &BigInt) -> BigInt {
        x * y
    }

    pub(crate) fn divide(x: &BigInt, y: &BigInt) -> Result<BigInt, &'static str> {
        if y.sign() == num_bigint::Sign::NoSign {
            return Err("Division by zero");
        }
        Ok(x / y)
    }

    pub(crate) fn remainder(x: &BigInt, y: &BigInt) -> Result<BigInt, &'static str> {
        if y.sign() == num_bigint::Sign::NoSign {
            return Err("Division by zero");
        }
        Ok(x % y)
    }

    pub(crate) fn add(x: &BigInt, y: &BigInt) -> BigInt {
        x + y
    }

    pub(crate) fn subtract(x: &BigInt, y: &BigInt) -> BigInt {
        x - y
    }

    pub(crate) fn left_shift(x: &BigInt, y: &BigInt) -> BigInt {
        let shift: i64 = y.try_into().unwrap_or(0);
        if shift >= 0 {
            x << (shift as u64)
        } else {
            x >> ((-shift) as u64)
        }
    }

    pub(crate) fn signed_right_shift(x: &BigInt, y: &BigInt) -> BigInt {
        let shift: i64 = y.try_into().unwrap_or(0);
        if shift >= 0 {
            x >> (shift as u64)
        } else {
            x << ((-shift) as u64)
        }
    }

    pub(crate) fn less_than(x: &BigInt, y: &BigInt) -> Option<bool> {
        Some(x < y)
    }

    pub(crate) fn equal(x: &BigInt, y: &BigInt) -> bool {
        x == y
    }

    pub(crate) fn bitwise_and(x: &BigInt, y: &BigInt) -> BigInt {
        x & y
    }

    pub(crate) fn bitwise_xor(x: &BigInt, y: &BigInt) -> BigInt {
        x ^ y
    }

    pub(crate) fn bitwise_or(x: &BigInt, y: &BigInt) -> BigInt {
        x | y
    }
}

impl fmt::Display for JsValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind() {
            ValueKind::Undefined => write!(f, "undefined"),
            ValueKind::Null => write!(f, "null"),
            ValueKind::Boolean => write!(f, "{}", self.as_boolean().unwrap()),
            ValueKind::Number => {
                write!(f, "{}", number_ops::to_string(self.as_number().unwrap()))
            }
            ValueKind::String => self
                .with_string(|units| write!(f, "{}", String::from_utf16_lossy(units)))
                .unwrap(),
            ValueKind::Symbol => self
                .with_symbol(|s| match s.description() {
                    Some(desc) => write!(f, "Symbol({desc})"),
                    None => write!(f, "Symbol()"),
                })
                .unwrap(),
            ValueKind::BigInt => self.with_bigint(|b| write!(f, "{b}n")).unwrap(),
            ValueKind::Object => write!(f, "[object Object]"),
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
        assert_eq!(format!("{}", JsValue::UNDEFINED), "undefined");
        assert_eq!(format!("{}", JsValue::NULL), "null");
        assert_eq!(format!("{}", JsValue::TRUE), "true");
        assert_eq!(format!("{}", JsValue::number(42.0)), "42");
        assert_eq!(
            format!("{}", JsValue::string(JsString::from_str("hi"))),
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

    #[test]
    fn symbol_property_keys_do_not_collide_with_display_text() {
        let symbol = JsSymbol::new(7, Some(JsString::from_str("x"))).to_property_key();
        let text = JsPropertyKey::from_str("Symbol(x)#7");

        assert!(symbol.is_symbol());
        assert!(!text.is_symbol());
        assert_ne!(symbol, text);
        assert_eq!(symbol.symbol_encoding(), Some("Symbol(x)#7"));
        assert_eq!(symbol.to_string(), "Symbol(x)#7");
    }

    #[test]
    fn well_known_symbol_property_keys_are_tagged() {
        let constructed = JsPropertyKey::well_known_symbol("iterator");
        let symbol =
            JsSymbol::new(1, Some(JsString::from_str("Symbol.iterator"))).to_property_key();

        assert_eq!(constructed, symbol);
        assert!(constructed.is_symbol());
        assert_ne!(
            constructed,
            JsPropertyKey::from_str("Symbol(Symbol.iterator)")
        );
    }

    #[test]
    fn symbol_clones_share_one_word_data_pointer() {
        let symbol = JsSymbol::new(7, Some(JsString::from_str("description")));
        let cloned = symbol.clone();

        assert_eq!(
            std::mem::size_of::<JsSymbol>(),
            std::mem::size_of::<Arc<JsSymbolData>>()
        );
        assert!(symbol.shares_storage_with(&cloned));
        assert_eq!(cloned.id(), 7);
        assert_eq!(
            cloned.description().map(JsString::to_rust_string),
            Some("description".to_string())
        );
    }

    // ----- JsValue method surface -------------------------------------------

    #[test]
    fn value_constructors() {
        assert!(JsValue::UNDEFINED.is_undefined());
        assert!(JsValue::NULL.is_null());
        assert_eq!(JsValue::TRUE.as_boolean(), Some(true));
        assert_eq!(JsValue::FALSE.as_boolean(), Some(false));
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
        let sym = JsSymbol::new(1, Some(JsString::from_str("s")));
        assert_eq!(JsValue::symbol(sym).as_symbol().unwrap().id(), 1);
        let big = JsBigInt::new(num_bigint::BigInt::from(42));
        assert_eq!(
            *JsValue::bigint(big).as_bigint().unwrap().value,
            num_bigint::BigInt::from(42)
        );
    }

    #[test]
    fn typed_accessors_return_none_on_mismatch() {
        let n = JsValue::number(1.0);
        assert_eq!(n.as_boolean(), None);
        assert_eq!(n.as_object_id(), None);
        assert!(n.as_string().is_none());
        assert!(n.as_symbol().is_none());
        assert!(n.as_bigint().is_none());
        assert_eq!(JsValue::TRUE.as_number(), None);
    }

    #[test]
    fn with_accessors() {
        let s = JsValue::from_str("abc");
        assert_eq!(s.with_string(|cu| cu.len()), Some(3));
        assert_eq!(JsValue::NULL.with_string(|cu| cu.len()), None);

        let sym = JsValue::symbol(JsSymbol::new(9, None));
        assert_eq!(sym.with_symbol(JsSymbol::id), Some(9));
        assert_eq!(JsValue::NULL.with_symbol(JsSymbol::id), None);

        let big = JsValue::bigint(JsBigInt::new(num_bigint::BigInt::from(5)));
        assert_eq!(
            big.with_bigint(|b| b.clone()),
            Some(num_bigint::BigInt::from(5))
        );
        assert_eq!(JsValue::NULL.with_bigint(|b| b.clone()), None);
    }

    #[test]
    fn into_accessors() {
        let s = JsValue::from_str("x");
        assert_eq!(s.into_string().unwrap().to_rust_string(), "x");
        assert!(JsValue::NULL.into_string().is_none());

        let big = JsValue::bigint(JsBigInt::new(num_bigint::BigInt::from(11)));
        assert_eq!(
            *big.into_bigint().unwrap().value,
            num_bigint::BigInt::from(11)
        );
        assert!(JsValue::number(1.0).into_bigint().is_none());
    }

    #[test]
    fn discriminant_and_kind() {
        let cases = [
            (JsValue::UNDEFINED, ValueKind::Undefined),
            (JsValue::NULL, ValueKind::Null),
            (JsValue::TRUE, ValueKind::Boolean),
            (JsValue::number(1.0), ValueKind::Number),
            (JsValue::from_str("s"), ValueKind::String),
            (JsValue::symbol(JsSymbol::new(0, None)), ValueKind::Symbol),
            (
                JsValue::bigint(JsBigInt::new(num_bigint::BigInt::from(0))),
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
        assert!(!JsValue::NULL.is_object());
        assert!(!JsValue::number(0.0).is_object());
    }

    #[test]
    fn nan_box_scalar_layout_and_number_bits() {
        const CANONICAL_NAN_BITS: u64 = 0x7FF8_0000_0000_0000;

        assert_eq!(std::mem::size_of::<JsValue>(), 8);
        assert_eq!(std::mem::align_of::<JsValue>(), std::mem::align_of::<u64>());

        let nan_inputs = [
            f64::NAN,
            f64::from_bits(0xFFF8_0000_0000_0000),
            f64::from_bits(0x7FF0_0000_0000_0001),
            f64::from_bits(0xFFF0_1234_5678_9ABC),
            (-1.0_f64).sqrt(),
            std::hint::black_box(0.0_f64) / std::hint::black_box(0.0_f64),
        ];
        for input in nan_inputs {
            assert_eq!(
                JsValue::number(input).as_number().unwrap().to_bits(),
                CANONICAL_NAN_BITS
            );
        }

        let preserved_bits = [
            0x0000_0000_0000_0000,
            0x8000_0000_0000_0000,
            0x0000_0000_0000_0001,
            0x0010_0000_0000_0000,
            0x3FF0_0000_0000_0000,
            0x7FEF_FFFF_FFFF_FFFF,
            0x7FF0_0000_0000_0000,
            0xFFF0_0000_0000_0000,
        ];
        for bits in preserved_bits {
            assert_eq!(
                JsValue::number(f64::from_bits(bits))
                    .as_number()
                    .unwrap()
                    .to_bits(),
                bits
            );
        }
    }

    #[test]
    fn nan_box_layout_matches_ratified_tags() {
        const SIGNATURE: u64 = 0xFFF8_0000_0000_0000;
        const TAG_SHIFT: u32 = 48;

        assert_eq!(JsValue::UNDEFINED.raw_bits(), SIGNATURE);
        assert_eq!(JsValue::NULL.raw_bits(), SIGNATURE | (1 << TAG_SHIFT));
        assert_eq!(JsValue::FALSE.raw_bits(), SIGNATURE | (2 << TAG_SHIFT));
        assert_eq!(JsValue::TRUE.raw_bits(), SIGNATURE | (3 << TAG_SHIFT));
        assert_eq!(
            JsValue::object(0x1234).raw_bits(),
            SIGNATURE | (4 << TAG_SHIFT) | 0x1234
        );

        let string_storage = Arc::new(vec![0x61]);
        let string_address = Arc::as_ptr(&string_storage).expose_provenance() as u64;
        let string = JsValue::string(JsString {
            code_units: string_storage,
        });
        assert_eq!(
            string.raw_bits(),
            SIGNATURE | (5 << TAG_SHIFT) | string_address
        );

        let symbol = JsSymbol::new(1, None);
        let symbol_address = Arc::as_ptr(&symbol.data).expose_provenance() as u64;
        let symbol = JsValue::symbol(symbol);
        assert_eq!(
            symbol.raw_bits(),
            SIGNATURE | (6 << TAG_SHIFT) | symbol_address
        );

        let bigint = JsBigInt::new(num_bigint::BigInt::from(1));
        let bigint_address = Arc::as_ptr(&bigint.value).expose_provenance() as u64;
        let bigint = JsValue::bigint(bigint);
        assert_eq!(
            bigint.raw_bits(),
            SIGNATURE | (7 << TAG_SHIFT) | bigint_address
        );
    }

    #[test]
    fn nan_box_object_payload_bounds() {
        const MAX_PAYLOAD: u64 = (1_u64 << 48) - 1;

        assert_eq!(JsValue::object(0).as_object_id(), Some(0));
        assert_eq!(
            JsValue::object(MAX_PAYLOAD).as_object_id(),
            Some(MAX_PAYLOAD)
        );
        assert!(std::panic::catch_unwind(|| JsValue::object(1_u64 << 48)).is_err());
        assert!(std::panic::catch_unwind(|| JsValue::object(u64::MAX)).is_err());
    }

    #[test]
    fn nan_box_heap_payload_strong_counts() {
        let string_storage = Arc::new(vec![0x61, 0x62]);
        let string = JsValue::string(JsString {
            code_units: string_storage.clone(),
        });
        assert_eq!(Arc::strong_count(&string_storage), 2);
        let string_clone = string.clone();
        assert_eq!(Arc::strong_count(&string_storage), 3);
        drop(string_clone);
        assert_eq!(Arc::strong_count(&string_storage), 2);
        let string_access = string.as_string().unwrap();
        assert_eq!(Arc::strong_count(&string_storage), 3);
        drop(string_access);
        let consumed_string = string.clone().into_string().unwrap();
        assert_eq!(Arc::strong_count(&string_storage), 3);
        drop(consumed_string);
        assert!(JsValue::number(1.0).into_string().is_none());
        drop(string);
        assert_eq!(Arc::strong_count(&string_storage), 1);

        let description_storage = Arc::new(vec![0x73]);
        let symbol = JsSymbol::new(
            17,
            Some(JsString {
                code_units: description_storage.clone(),
            }),
        );
        let symbol_storage = symbol.data.clone();
        let symbol_value = JsValue::symbol(symbol);
        assert_eq!(Arc::strong_count(&symbol_storage), 2);
        let symbol_clone = symbol_value.clone();
        assert_eq!(Arc::strong_count(&symbol_storage), 3);
        drop(symbol_clone);
        let symbol_access = symbol_value.as_symbol().unwrap();
        assert_eq!(symbol_access.id(), 17);
        assert_eq!(symbol_access.description().unwrap().to_rust_string(), "s");
        assert_eq!(Arc::strong_count(&symbol_storage), 3);
        drop(symbol_access);
        drop(symbol_value);
        assert_eq!(Arc::strong_count(&symbol_storage), 1);
        drop(symbol_storage);
        assert_eq!(Arc::strong_count(&description_storage), 1);

        let bigint_storage = Arc::new(num_bigint::BigInt::from(123));
        let bigint = JsValue::bigint(JsBigInt {
            value: bigint_storage.clone(),
        });
        assert_eq!(Arc::strong_count(&bigint_storage), 2);
        let bigint_clone = bigint.clone();
        assert_eq!(Arc::strong_count(&bigint_storage), 3);
        drop(bigint_clone);
        let bigint_access = bigint.as_bigint().unwrap();
        assert_eq!(*bigint_access.value, num_bigint::BigInt::from(123));
        assert_eq!(Arc::strong_count(&bigint_storage), 3);
        drop(bigint_access);
        let consumed_bigint = bigint.clone().into_bigint().unwrap();
        assert_eq!(Arc::strong_count(&bigint_storage), 3);
        drop(consumed_bigint);
        assert!(JsValue::NULL.into_bigint().is_none());
        drop(bigint);
        assert_eq!(Arc::strong_count(&bigint_storage), 1);
    }

    #[test]
    fn nan_box_borrowing_callbacks_survive_unwind() {
        let string_storage = Arc::new(vec![0x78]);
        let string = JsValue::string(JsString {
            code_units: string_storage.clone(),
        });
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                string.with_string(|_| panic!("string callback"));
            }))
            .is_err()
        );
        assert_eq!(string.with_string(|units| units[0]), Some(0x78));
        assert_eq!(Arc::strong_count(&string_storage), 2);

        let symbol = JsSymbol::new(23, None);
        let symbol_storage = symbol.data.clone();
        let symbol_value = JsValue::symbol(symbol);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                symbol_value.with_symbol(|_| panic!("symbol callback"));
            }))
            .is_err()
        );
        assert_eq!(symbol_value.with_symbol(JsSymbol::id), Some(23));
        assert_eq!(Arc::strong_count(&symbol_storage), 2);

        let bigint_storage = Arc::new(num_bigint::BigInt::from(456));
        let bigint = JsValue::bigint(JsBigInt {
            value: bigint_storage.clone(),
        });
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                bigint.with_bigint(|_| panic!("bigint callback"));
            }))
            .is_err()
        );
        assert_eq!(
            bigint.with_bigint(Clone::clone),
            Some(num_bigint::BigInt::from(456))
        );
        assert_eq!(Arc::strong_count(&bigint_storage), 2);
    }

    #[test]
    fn nan_box_mixed_values_move_and_clone_across_threads() {
        let values = vec![
            JsValue::UNDEFINED,
            JsValue::NULL,
            JsValue::FALSE,
            JsValue::TRUE,
            JsValue::number(-0.0),
            JsValue::object(99),
            JsValue::from_str("thread"),
            JsValue::symbol(JsSymbol::new(31, Some(JsString::from_str("nested")))),
            JsValue::bigint(JsBigInt::new(num_bigint::BigInt::from(789))),
        ];

        let cloned = std::thread::spawn(move || {
            let cloned: Vec<_> = values.to_vec();
            assert_eq!(
                cloned[4].as_number().unwrap().to_bits(),
                (-0.0_f64).to_bits()
            );
            assert_eq!(cloned[5].as_object_id(), Some(99));
            assert_eq!(cloned[6].with_string(|units| units.len()), Some(6));
            assert_eq!(cloned[7].with_symbol(JsSymbol::id), Some(31));
            assert_eq!(
                cloned[8].with_bigint(Clone::clone),
                Some(num_bigint::BigInt::from(789))
            );
            cloned
        })
        .join()
        .unwrap();

        assert_eq!(
            cloned.iter().map(JsValue::kind).collect::<Vec<_>>(),
            vec![
                ValueKind::Undefined,
                ValueKind::Null,
                ValueKind::Boolean,
                ValueKind::Boolean,
                ValueKind::Number,
                ValueKind::Object,
                ValueKind::String,
                ValueKind::Symbol,
                ValueKind::BigInt,
            ]
        );
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn nan_box_heap_pointer_range_panics_before_arc_transfer() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct DropSpy(Arc<AtomicUsize>);

        impl Drop for DropSpy {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let drops = Arc::new(AtomicUsize::new(0));
        let payload = Arc::new(DropSpy(drops.clone()));
        let result = std::panic::catch_unwind(|| {
            NanBoxedValue::from_arc_after_address_check(NanTag::String, payload, 1_usize << 48)
        });

        assert!(result.is_err());
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    /// Measurement-only probe kept identical before and after the NaN-box swap.
    #[test]
    #[ignore = "manual representation microbenchmark"]
    fn nan_box_clone_benchmark() {
        use std::hint::black_box;
        use std::time::Instant;

        const ITERATIONS: u64 = 10_000_000;
        let number = JsValue::number(1234.5);
        let object = JsValue::object(0x1234_5678);
        let mut checksum = 0_u64;

        let started = Instant::now();
        for _ in 0..ITERATIONS {
            let number_clone = black_box(number.clone());
            checksum ^= black_box(number_clone.as_number().unwrap().to_bits());
            let object_clone = black_box(object.clone());
            checksum ^= black_box(object_clone.as_object_id().unwrap());
        }
        let elapsed = started.elapsed();

        eprintln!(
            "jsvalue_size={} iterations={} elapsed_ns={} checksum={checksum}",
            std::mem::size_of::<JsValue>(),
            ITERATIONS,
            elapsed.as_nanos()
        );
        black_box(checksum);
    }
}
