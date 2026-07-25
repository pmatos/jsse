//! Property-key interning (issue #74).
//!
//! Property names are stored twice per object: once as the `PropertyMap` key
//! and once in `JsObjectData.property_order`. Both copies used to be owned
//! `String`s, so every property cost two heap allocations of the same bytes.
//!
//! This module interns ordinary and tagged Symbol keys into a bounded
//! process-thread cache of `JsPropertyKey`. The storage layer (`PropertyMap` +
//! `property_order`) holds `JsPropertyKey` rather than `String`, so the two
//! stored copies share one allocation and "cloning" a key is a refcount bump.
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
//!
//! ## Admission bounds
//! The cache retains at most `MAX_CACHE_ENTRIES` keys, each no longer than
//! `MAX_CACHEABLE_KEY_BYTES`. A miss outside either bound is returned without
//! being retained. This keeps thread-lifetime memory bounded without adding
//! recency bookkeeping to cache hits.

use std::cell::RefCell;

use crate::types::JsPropertyKey;

thread_local! {
    static KEY_CACHE: RefCell<KeyCache> = RefCell::new(KeyCache::new());
}

struct KeyCache {
    map: std::collections::HashMap<Box<[u8]>, JsPropertyKey>,
}

/// Includes the seed keys below. With the per-key byte limit, retained key
/// payload is bounded to roughly 2 MiB (one map key and one `JsPropertyKey`).
const MAX_CACHE_ENTRIES: usize = 4_096;
const MAX_CACHEABLE_KEY_BYTES: usize = 256;

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
];

const SEED_SYMBOL_KEYS: &[&str] = &["iterator", "toPrimitive", "toStringTag"];

impl KeyCache {
    fn new() -> Self {
        let mut map = std::collections::HashMap::with_capacity(SEED_KEYS.len().next_power_of_two());
        for &s in SEED_KEYS {
            let key = JsPropertyKey::from_str(s);
            map.insert(Box::from(s.as_bytes()), key);
        }
        for &name in SEED_SYMBOL_KEYS {
            let key = JsPropertyKey::well_known_symbol(name);
            map.insert(Box::from(key.as_bytes()), key);
        }
        Self { map }
    }

    fn intern(&mut self, key: JsPropertyKey) -> JsPropertyKey {
        if key.as_bytes().len() > MAX_CACHEABLE_KEY_BYTES {
            return key;
        }
        if let Some(existing) = self.map.get(key.as_bytes()) {
            return existing.clone();
        }
        if self.map.len() >= MAX_CACHE_ENTRIES {
            return key;
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
/// Eligible ordinary names and tagged Symbol keys are cached so repeated uses
/// share one allocation. Canonical array-index strings are NOT cached (see the
/// integer-index gate in the module docs), and misses outside the admission
/// bounds are returned without being retained.
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
        let symbol = crate::types::JsSymbol {
            id: 42,
            description: Some(crate::types::JsString::from_str("desc")),
        };
        let a = intern_js_key(symbol.to_property_key());
        let b = intern_js_key(symbol.to_property_key());
        assert!(a.shares_storage_with(&b), "Symbol keys must intern");
        // Byte content must be preserved exactly for symbol_key_to_jsvalue.
        assert_eq!(a.symbol_encoding(), Some("Symbol(desc)#42"));
        assert!(a.is_symbol());
        assert_ne!(a, intern_key("Symbol(desc)#42"));
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

    #[test]
    fn cache_stops_retaining_keys_at_entry_limit() {
        let mut cache = KeyCache::new();
        let available = MAX_CACHE_ENTRIES - cache.map.len();
        for i in 0..available {
            cache.intern(JsPropertyKey::from(format!("cache-bound-{i}")));
        }
        assert_eq!(cache.map.len(), MAX_CACHE_ENTRIES);

        let a = cache.intern(JsPropertyKey::from_str("cache-bound-overflow"));
        let b = cache.intern(JsPropertyKey::from_str("cache-bound-overflow"));
        assert!(
            !a.shares_storage_with(&b),
            "keys beyond the entry limit must not be retained"
        );
        assert_eq!(cache.map.len(), MAX_CACHE_ENTRIES);

        let seed = cache.intern(JsPropertyKey::from_str("length"));
        assert!(
            seed.shares_storage_with(cache.map.get(b"length".as_slice()).unwrap()),
            "existing entries must still hit after the cache reaches its limit"
        );
    }

    #[test]
    fn oversized_keys_are_not_retained() {
        let mut cache = KeyCache::new();
        let oversized = "x".repeat(MAX_CACHEABLE_KEY_BYTES + 1);
        let a = cache.intern(JsPropertyKey::from(oversized.clone()));
        let b = cache.intern(JsPropertyKey::from(oversized.clone()));

        assert_eq!(a.as_bytes(), oversized.as_bytes());
        assert!(
            !a.shares_storage_with(&b),
            "oversized keys must not be retained"
        );
    }

    #[test]
    fn oversized_symbol_keys_preserve_their_exact_encoding() {
        let mut cache = KeyCache::new();
        let symbol = crate::types::JsSymbol {
            id: 99,
            description: Some(crate::types::JsString::from_str(
                &"s".repeat(MAX_CACHEABLE_KEY_BYTES),
            )),
        };
        let original = symbol.to_property_key();
        let a = cache.intern(symbol.to_property_key());
        let b = cache.intern(symbol.to_property_key());

        assert_eq!(a, original);
        assert!(a.is_symbol());
        assert_eq!(a.symbol_encoding(), original.symbol_encoding());
        assert!(
            !a.shares_storage_with(&b),
            "oversized Symbol keys must follow the non-retaining path"
        );
    }
}
