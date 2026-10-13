//! Property-key interning (issue #74).
//!
//! Property names used to be owned `String`s per object, so every property
//! cost a heap allocation of the same bytes.
//!
//! This module interns ordinary and tagged Symbol keys into a bounded
//! process-thread cache of `JsPropertyKey`. The storage layer (`PropertyMap`)
//! holds `JsPropertyKey` rather than `String`, so objects share one allocation
//! per name and "cloning" a key is a refcount bump. A key that already is the
//! cached allocation is recognised by address (`KeyCache::canonical`), so
//! re-interning it - what every store of a compile-time-interned name does -
//! costs no hash.
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
//! The one exception is `JsPropertyKey::from_index`, which shares a fixed table
//! of the first few index keys for arguments objects.
//!
//! ## Admission bounds
//! The cache retains at most `MAX_CACHE_ENTRIES` keys, each no longer than
//! `MAX_CACHEABLE_KEY_BYTES`. A miss outside either bound is returned without
//! being retained. This keeps thread-lifetime memory bounded without adding
//! recency bookkeeping to cache hits.

use std::cell::RefCell;

use crate::types::{JsPropertyKey, JsString};

thread_local! {
    static KEY_CACHE: RefCell<KeyCache> = RefCell::new(KeyCache::new());
    static KEY_STRING_CACHE: RefCell<KeyStringCache> = RefCell::new(KeyStringCache::new());
}

const KEY_STRING_CACHE_SLOTS: usize = 1024;

/// Direct-mapped `JsPropertyKey` -> `JsString` cache, indexed by the key's
/// backing-allocation address. Property keys are interned, so the same name
/// keeps hitting the same allocation; materialising its string (a UTF-16
/// `Vec` plus two `Arc`s) is otherwise repeated for every `Object.keys`,
/// for-in step and similar. A slot holds a clone of the key, which keeps the
/// address from being recycled while the entry lives; a hit is verified by
/// allocation identity, so a colliding key simply replaces the slot.
struct KeyStringCache {
    slots: Vec<Option<(JsPropertyKey, JsString)>>,
}

impl KeyStringCache {
    fn new() -> Self {
        Self {
            slots: vec![None; KEY_STRING_CACHE_SLOTS],
        }
    }

    #[inline]
    fn slot_index(key: &JsPropertyKey) -> usize {
        let h = (key.allocation_addr() >> 4).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        h >> (usize::BITS - KEY_STRING_CACHE_SLOTS.trailing_zeros())
    }
}

/// The String value of a non-Symbol property key. Repeated requests for the
/// same interned key share one `JsString`.
#[inline]
pub(crate) fn key_to_js_string(key: &JsPropertyKey) -> JsString {
    if key.is_symbol() || key.as_bytes().len() > MAX_CACHEABLE_KEY_BYTES {
        return key.to_js_string();
    }
    KEY_STRING_CACHE
        .try_with(|c| {
            let mut c = c.borrow_mut();
            let idx = KeyStringCache::slot_index(key);
            if let Some((k, s)) = &c.slots[idx]
                && k.same_allocation(key)
            {
                return s.clone();
            }
            let s = key.to_js_string();
            c.slots[idx] = Some((key.clone(), s.clone()));
            s
        })
        .unwrap_or_else(|_| key.to_js_string())
}

/// Randomly seeded hasher; see `IndexState` in `property_map.rs`.
type InternState = foldhash::fast::RandomState;

const CANONICAL_SLOTS: usize = 1024;

struct KeyCache {
    map: std::collections::HashMap<Box<[u8]>, JsPropertyKey, InternState>,
    /// Direct-mapped, by allocation address: slot `i` may hold a key that is
    /// the canonical (cached) allocation for its text, so re-interning a
    /// key that already is one - every compile-time-interned name - costs a
    /// multiply and a pointer compare instead of a hash. Holding the key
    /// keeps its address from being recycled. A key is only recorded once it
    /// has been found in or admitted to `map`, which never evicts, so a hit
    /// stays valid for the life of the thread.
    canonical: Vec<Option<JsPropertyKey>>,
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
        let mut map = std::collections::HashMap::with_capacity_and_hasher(
            SEED_KEYS.len().next_power_of_two(),
            InternState::default(),
        );
        for &s in SEED_KEYS {
            let key = JsPropertyKey::from_str(s);
            map.insert(Box::from(s.as_bytes()), key);
        }
        for &name in SEED_SYMBOL_KEYS {
            let key = JsPropertyKey::well_known_symbol(name);
            map.insert(Box::from(key.as_bytes()), key);
        }
        Self {
            map,
            canonical: vec![None; CANONICAL_SLOTS],
        }
    }

    #[inline]
    fn canonical_slot(key: &JsPropertyKey) -> usize {
        let h = (key.allocation_addr() >> 4).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        h >> (usize::BITS - CANONICAL_SLOTS.trailing_zeros())
    }

    #[inline]
    fn is_canonical(&self, key: &JsPropertyKey) -> bool {
        self.canonical[Self::canonical_slot(key)]
            .as_ref()
            .is_some_and(|k| k.same_allocation(key))
    }

    #[inline]
    fn remember(&mut self, key: &JsPropertyKey) {
        if !self.is_canonical(key) {
            self.canonical[Self::canonical_slot(key)] = Some(key.clone());
        }
    }

    fn intern(&mut self, key: JsPropertyKey) -> JsPropertyKey {
        if key.as_bytes().len() > MAX_CACHEABLE_KEY_BYTES {
            return key;
        }
        if let Some(existing) = self.map.get(key.as_bytes()) {
            let existing = existing.clone();
            self.remember(&existing);
            return existing;
        }
        if self.map.len() >= MAX_CACHE_ENTRIES {
            return key;
        }
        self.map.insert(Box::from(key.as_bytes()), key.clone());
        self.remember(&key);
        key
    }

    /// `intern` for text that has no `JsPropertyKey` yet: a hit builds none.
    fn intern_str(&mut self, s: &str) -> JsPropertyKey {
        if let Some(existing) = self.map.get(s.as_bytes()) {
            let existing = existing.clone();
            self.remember(&existing);
            return existing;
        }
        self.intern(JsPropertyKey::from_str(s))
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

/// Calls into the (hashing) property-key interner, for the `perf-counters`
/// report: a path that interns keys per object shows up as a count that scales
/// with the object count (#901). A key that already is the canonical
/// allocation is recognised by address and not counted (#911).
#[cfg(feature = "perf-counters")]
static KEY_INTERN_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(feature = "perf-counters")]
pub(crate) fn key_intern_calls() -> u64 {
    KEY_INTERN_CALLS.load(std::sync::atomic::Ordering::Relaxed)
}

#[inline(always)]
fn count_key_intern() {
    #[cfg(feature = "perf-counters")]
    KEY_INTERN_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Intern a property key into shared WTF-8 storage.
///
/// Eligible ordinary names and tagged Symbol keys are cached so repeated uses
/// share one allocation. Canonical array-index strings are NOT cached (see the
/// integer-index gate in the module docs), and misses outside the admission
/// bounds are returned without being retained.
#[inline]
pub(crate) fn intern_key(s: &str) -> JsPropertyKey {
    count_key_intern();
    if is_canonical_array_index(s) {
        return JsPropertyKey::from_str(s);
    }
    KEY_CACHE.with(|c| c.borrow_mut().intern_str(s))
}

#[inline]
pub(crate) fn intern_js_key(key: JsPropertyKey) -> JsPropertyKey {
    KEY_CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_canonical(&key) {
            return key;
        }
        count_key_intern();
        if key.as_str().is_some_and(is_canonical_array_index) {
            return key;
        }
        c.intern(key)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_strings_are_cached_per_allocation() {
        let k = intern_key("cachedKeyString");
        let a = key_to_js_string(&k);
        let b = key_to_js_string(&k);
        assert!(a.ptr_eq(&b), "same interned key must share one JsString");
        assert_eq!(a.to_rust_string(), "cachedKeyString");
        let other = JsPropertyKey::from_str("cachedKeyString");
        let c = key_to_js_string(&other);
        assert_eq!(c.as_units(), a.as_units());
    }

    #[test]
    fn key_string_cache_preserves_lone_surrogates() {
        let s = JsString::from_vec(vec![0xD800, 0x61]);
        let k = JsPropertyKey::from_js_string(&s);
        assert_eq!(key_to_js_string(&k).as_units(), s.as_units());
        assert_eq!(key_to_js_string(&k).as_units(), s.as_units());
    }

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
        let symbol =
            crate::types::JsSymbol::new(42, Some(crate::types::JsString::from_str("desc")));
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
        let symbol = crate::types::JsSymbol::new(
            99,
            Some(crate::types::JsString::from_str(
                &"s".repeat(MAX_CACHEABLE_KEY_BYTES),
            )),
        );
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
