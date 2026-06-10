//! Property-key interning (issue #74).
//!
//! Property names are stored twice per object: once as the `PropertyMap` key
//! and once in `JsObjectData.property_order`. Both copies used to be owned
//! `String`s, so every property cost two heap allocations of the same bytes.
//!
//! This module interns ordinary and symbol-encoded keys into a process-thread
//! cache of `Rc<str>`. The storage layer (`PropertyMap` + `property_order`)
//! holds `Rc<str>` rather than `String`, so the two stored copies share one
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
//! returns a fresh `Rc<str>` for them. The gate mirrors `parse_array_index`
//! in `types.rs` (all-ASCII-digits, no leading zero except "0", value < 2^32-1).

use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    static KEY_CACHE: RefCell<KeyCache> = RefCell::new(KeyCache::new());
}

struct KeyCache {
    map: std::collections::HashMap<Box<str>, Rc<str>>,
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
            let rc: Rc<str> = Rc::from(s);
            map.insert(Box::from(s), rc);
        }
        Self { map }
    }

    fn intern(&mut self, s: &str) -> Rc<str> {
        if let Some(rc) = self.map.get(s) {
            return Rc::clone(rc);
        }
        let rc: Rc<str> = Rc::from(s);
        self.map.insert(Box::from(s), Rc::clone(&rc));
        rc
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

/// Intern a property key into a shared `Rc<str>`.
///
/// Ordinary names and symbol-encoded keys (`"Symbol(...)#id"`) are cached so
/// repeated uses share one allocation. Canonical array-index strings are NOT
/// cached (see the integer-index gate in the module docs) — a fresh `Rc<str>`
/// is returned so the cache never accumulates unbounded numeric keys.
#[inline]
pub(crate) fn intern_key(s: &str) -> Rc<str> {
    if is_canonical_array_index(s) {
        return Rc::from(s);
    }
    KEY_CACHE.with(|c| c.borrow_mut().intern(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interns_ordinary_names_share_allocation() {
        let a = intern_key("fooBarBaz");
        let b = intern_key("fooBarBaz");
        assert!(Rc::ptr_eq(&a, &b), "ordinary names must share one Rc<str>");
        assert_eq!(a.as_ref(), "fooBarBaz");
    }

    #[test]
    fn seed_keys_are_interned() {
        let a = intern_key("length");
        let b = intern_key("length");
        assert!(Rc::ptr_eq(&a, &b));
    }

    #[test]
    fn symbol_keys_are_interned_and_preserved() {
        let key = "Symbol(desc)#42";
        let a = intern_key(key);
        let b = intern_key(key);
        assert!(Rc::ptr_eq(&a, &b), "symbol-encoded keys must intern");
        // Byte content must be preserved exactly for symbol_key_to_jsvalue.
        assert_eq!(a.as_ref(), key);
        assert!(a.starts_with("Symbol("));
    }

    #[test]
    fn integer_index_keys_not_cached() {
        let a = intern_key("42");
        let b = intern_key("42");
        assert!(
            !Rc::ptr_eq(&a, &b),
            "canonical array-index keys must NOT be cached"
        );
        assert_eq!(a.as_ref(), "42");
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
        assert!(Rc::ptr_eq(&a, &b));
    }
}
