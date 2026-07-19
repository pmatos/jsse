//! Property-key interning (issue #74).
//!
//! Property names are stored twice per object: once as the `PropertyMap` key
//! and once in `JsObjectData.property_order`. Both copies used to be owned
//! `String`s, so every property cost two heap allocations of the same bytes.
//!
//! This module interns ordinary and symbol-encoded keys into a process-thread
//! cache of `JsPropertyKey`. The storage layer (`PropertyMap` + `property_order`)
//! holds `JsPropertyKey` rather than `String`, so the two stored copies share one
//! allocation and "cloning" a key is a refcount bump.
//!
//! A `thread_local!` cache is used so `JsObjectData` methods — which only have
//! `&mut self`, not `&mut Interpreter` — can intern without threading any extra
//! state through the ~1100 property-write call sites.
//!
//! ## Integer-index gate
//! Canonical array-index strings (`"0"`, `"1"`, `"42"`, …) have unbounded
//! cardinality and would bloat the cache without sharing benefit (each index
//! appears on at most a handful of objects). They are NOT cached: `intern_key`
//! returns a fresh `JsPropertyKey` for them. The gate mirrors `parse_array_index`
//! in `types.rs` (all-ASCII-digits, no leading zero except "0", value < 2^32-1).

use std::cell::RefCell;

use crate::types::JsPropertyKey;

thread_local! {
    static KEY_CACHE: RefCell<KeyCache> = RefCell::new(KeyCache::new());
}

struct KeyCache {
    map: std::collections::HashMap<Box<[u8]>, JsPropertyKey>,
}

// Common property names worth pre-seeding so the very first lookups hit the
// cache. Symbol-encoded well-known keys are included because they recur on
// nearly every object that participates in iteration / coercion protocols.
const SEED_KEYS: &[&str] = &[
    "length",
    "prototype",
    "constructor",
    "name",
    "value",
    "key",
    "left",
    "right",
    "next",
    "done",
    "__proto__",
    "toString",
    "valueOf",
    "get",
    "set",
    "enumerable",
    "configurable",
    "writable",
    "Symbol(Symbol.iterator)",
    "Symbol(Symbol.toPrimitive)",
    "Symbol(Symbol.toStringTag)",
];

impl KeyCache {
    fn new() -> Self {
        let mut map = std::collections::HashMap::with_capacity(SEED_KEYS.len().next_power_of_two());
        for &s in SEED_KEYS {
            let key = JsPropertyKey::from_str(s);
            map.insert(Box::from(s.as_bytes()), key);
        }
        Self { map }
    }

    fn intern(&mut self, key: JsPropertyKey) -> JsPropertyKey {
        if let Some(existing) = self.map.get(key.as_bytes()) {
            return existing.clone();
        }
        self.map.insert(Box::from(key.as_bytes()), key.clone());
        key
    }
}

/// Returns true if `s` is a canonical array-index string: all ASCII digits,
/// no leading zero except the single "0", and numeric value < 2^32-1.
/// Mirrors `parse_array_index` in `types.rs` — keep them in sync.
#[inline]
fn is_canonical_array_index(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    // No leading zeros (except "0" itself).
    if s.len() > 1 && s.as_bytes()[0] == b'0' {
        return false;
    }
    match s.parse::<u32>() {
        // 0xFFFFFFFF is not a valid array index (spec §6.1.7).
        Ok(n) => n != u32::MAX,
        Err(_) => false,
    }
}

/// Intern a property key into shared WTF-8 storage.
///
/// Ordinary names and symbol-encoded keys (`"Symbol(...)#id"`) are cached so
/// repeated uses share one allocation. Canonical array-index strings are NOT
/// cached (see the integer-index gate in the module docs) — a fresh key
/// is returned so the cache never accumulates unbounded numeric keys.
#[inline]
pub(crate) fn intern_key(s: &str) -> JsPropertyKey {
    if is_canonical_array_index(s) {
        return JsPropertyKey::from_str(s);
    }
    KEY_CACHE.with(|c| c.borrow_mut().intern(JsPropertyKey::from_str(s)))
}

#[inline]
pub(crate) fn intern_js_key(key: JsPropertyKey) -> JsPropertyKey {
    if key.as_str().is_some_and(is_canonical_array_index) {
        return key;
    }
    KEY_CACHE.with(|c| c.borrow_mut().intern(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interns_ordinary_names_share_allocation() {
        let a = intern_key("fooBarBaz");
        let b = intern_key("fooBarBaz");
        assert!(
            a.shares_storage_with(&b),
            "ordinary names must share one allocation"
        );
        assert!(a.eq_str("fooBarBaz"));
    }

    #[test]
    fn seed_keys_are_interned() {
        let a = intern_key("length");
        let b = intern_key("length");
        assert!(a.shares_storage_with(&b));
    }

    #[test]
    fn symbol_keys_are_interned_and_preserved() {
        let key = "Symbol(desc)#42";
        let a = intern_key(key);
        let b = intern_key(key);
        assert!(a.shares_storage_with(&b), "symbol-encoded keys must intern");
        // Byte content must be preserved exactly for symbol_key_to_jsvalue.
        assert!(a.eq_str(key));
        assert!(a.starts_with("Symbol("));
    }

    #[test]
    fn integer_index_keys_not_cached() {
        let a = intern_key("42");
        let b = intern_key("42");
        assert!(
            !a.shares_storage_with(&b),
            "canonical array-index keys must NOT be cached"
        );
        assert!(a.eq_str("42"));
    }

    #[test]
    fn integer_index_gate_matches_array_index_rules() {
        // Canonical indices: not cached.
        assert!(is_canonical_array_index("0"));
        assert!(is_canonical_array_index("1"));
        assert!(is_canonical_array_index("42"));
        assert!(is_canonical_array_index("4294967294")); // 2^32-2, max valid index
        // Not canonical indices: interned.
        assert!(!is_canonical_array_index("")); // empty
        assert!(!is_canonical_array_index("00")); // leading zero
        assert!(!is_canonical_array_index("01")); // leading zero
        assert!(!is_canonical_array_index("-1")); // negative
        assert!(!is_canonical_array_index("4294967295")); // 2^32-1, NOT an index
        assert!(!is_canonical_array_index("4294967296")); // 2^32, overflows u32
        assert!(!is_canonical_array_index("1.5")); // non-integer
        assert!(!is_canonical_array_index("length"));
        assert!(!is_canonical_array_index("Symbol(x)#1"));
    }

    #[test]
    fn non_canonical_numeric_names_are_interned() {
        // "01" is a non-canonical numeric string — it is a real property name,
        // not an array index, so it should be interned (and shared).
        let a = intern_key("01");
        let b = intern_key("01");
        assert!(a.shares_storage_with(&b));
    }
}
