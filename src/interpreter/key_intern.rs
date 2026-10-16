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

use std::cell::{Cell, RefCell};

use crate::types::{JsPropertyKey, JsString};

thread_local! {
    static KEY_CACHE: RefCell<KeyCache> = RefCell::new(KeyCache::new());
    static KEY_STRING_CACHE: RefCell<KeyStringCache> = RefCell::new(KeyStringCache::new());
    /// First-unit address of the string held by each `KeyStringCache::by_string`
    /// slot (0 when empty). Plain `Cell`s in a const-initialised thread local
    /// cost no lazy-init or borrow check, so the reverse lookup that misses -
    /// every `ToPropertyKey` of a string that no key built - stays a few
    /// instructions.
    static REVERSE_TAGS: [Cell<usize>; KEY_STRING_CACHE_SLOTS] =
        const { [const { Cell::new(0) }; KEY_STRING_CACHE_SLOTS] };
}

const KEY_STRING_CACHE_SLOTS: usize = 1024;

/// Direct-mapped `JsPropertyKey` -> `JsString` cache, indexed by the key's
/// backing-allocation address. Property keys are interned, so the same name
/// keeps hitting the same allocation; materialising its string (a UTF-16
/// `Vec` plus two `Arc`s) is otherwise repeated for every `Object.keys`,
/// for-in step and similar. A slot holds a clone of the key, which keeps the
/// address from being recycled while the entry lives; a hit is verified by
/// allocation identity, so a colliding key simply replaces the slot.
///
/// `by_string` is the reverse direction, indexed by the string's first-unit
/// address and filled whenever a string is built here: a name handed to user
/// code (a Proxy trap argument, an `Object.keys` entry) usually comes back
/// through `ToPropertyKey` (`Reflect.get(t, k)`, `o[k]`), which then reuses
/// the key instead of re-encoding the string. A slot holds a clone of the
/// string, so its units cannot be freed or rewritten while it lives, and a
/// hit is verified by `ptr_eq` (same units, same length).
///
/// `by_name` fronts both for a name that arrives as `&str` (the name operand of
/// a member access): hashing it through the interner costs more than the rest
/// of the conversion, and an interner that has reached its entry limit hands
/// out a fresh key every time. A slot is found by the `&str`'s address and
/// length and then confirmed by comparing its bytes, so a recycled address
/// only ever misses.
struct KeyStringCache {
    slots: Vec<Option<(JsPropertyKey, JsString)>>,
    by_string: Vec<Option<(JsString, JsPropertyKey)>>,
    by_name: Vec<Option<NameEntry>>,
}

struct NameEntry {
    addr: usize,
    len: usize,
    key: JsPropertyKey,
    string: JsString,
}

impl KeyStringCache {
    fn new() -> Self {
        Self {
            slots: vec![None; KEY_STRING_CACHE_SLOTS],
            by_string: vec![None; KEY_STRING_CACHE_SLOTS],
            by_name: (0..KEY_STRING_CACHE_SLOTS).map(|_| None).collect(),
        }
    }

    #[inline]
    fn slot_index(addr: usize) -> usize {
        let h = (addr >> 4).wrapping_mul(0x9E37_79B9_7F4A_7C15);
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
            let idx = KeyStringCache::slot_index(key.allocation_addr());
            if let Some((k, s)) = &c.slots[idx]
                && k.same_allocation(key)
            {
                return s.clone();
            }
            let s = key.to_js_string();
            let rev = KeyStringCache::slot_index(s.units_addr());
            REVERSE_TAGS.with(|t| t[rev].set(s.units_addr()));
            c.by_string[rev] = Some((s.clone(), key.clone()));
            c.slots[idx] = Some((key.clone(), s.clone()));
            s
        })
        .unwrap_or_else(|_| key.to_js_string())
}

/// The String value of a property name given as text; the same JsString
/// `key_to_js_string` returns for the name's interned key.
pub(crate) fn name_to_js_string(name: &str) -> JsString {
    if name.len() > MAX_CACHEABLE_KEY_BYTES {
        return key_to_js_string(&intern_key(name));
    }
    let addr = name.as_ptr() as usize;
    let idx = KeyStringCache::slot_index(addr);
    let hit = KEY_STRING_CACHE
        .try_with(|c| match &c.borrow().by_name[idx] {
            Some(e)
                if e.addr == addr && e.len == name.len() && e.key.as_bytes() == name.as_bytes() =>
            {
                Some(e.string.clone())
            }
            _ => None,
        })
        .ok()
        .flatten();
    if let Some(string) = hit {
        return string;
    }
    let key = intern_key(name);
    let string = key_to_js_string(&key);
    let _ = KEY_STRING_CACHE.try_with(|c| {
        c.borrow_mut().by_name[idx] = Some(NameEntry {
            addr,
            len: name.len(),
            key,
            string: string.clone(),
        });
    });
    string
}

/// `ToPropertyKey` of a String value: the key `key_to_js_string` built it
/// from when it still is in the reverse cache, else a fresh encoding.
#[inline]
pub(crate) fn key_from_js_string(s: &JsString) -> JsPropertyKey {
    let addr = s.units_addr();
    let idx = KeyStringCache::slot_index(addr);
    if REVERSE_TAGS.with(|t| t[idx].get()) == addr
        && let Some(key) = reverse_lookup(s, idx)
    {
        return key;
    }
    JsPropertyKey::from_js_string(s)
}

#[inline(never)]
fn reverse_lookup(s: &JsString, idx: usize) -> Option<JsPropertyKey> {
    KEY_STRING_CACHE
        .try_with(|c| match &c.borrow().by_string[idx] {
            Some((cached, key)) if cached.ptr_eq(s) => Some(key.clone()),
            _ => None,
        })
        .ok()
        .flatten()
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
    fn string_built_from_a_key_converts_back_to_that_key() {
        let k = intern_key("reverseCachedName");
        let s = key_to_js_string(&k);
        assert!(key_from_js_string(&s).same_allocation(&k));
        assert!(key_from_js_string(&s.clone()).same_allocation(&k));
        let equal = JsString::from_str("reverseCachedName");
        let back = key_from_js_string(&equal);
        assert_eq!(back, k);
        assert!(!back.same_allocation(&k), "only the built string hits");
    }

    #[test]
    fn reverse_cache_is_exact_for_lone_surrogates_and_index_keys() {
        for units in [
            vec![0xD800u16],
            vec![0xDC00, 0x61],
            vec![0x61, 0xD83D, 0xDE00, 0xD83D],
            vec![0x31, 0x32, 0x38],
            vec![],
        ] {
            let original = JsString::from_vec(units.clone());
            let k = JsPropertyKey::from_js_string(&original);
            let built = key_to_js_string(&k);
            assert_eq!(built.as_units(), units.as_slice());
            assert_eq!(key_from_js_string(&built), k);
        }
    }

    #[test]
    fn reverse_cache_distinguishes_views_sharing_a_buffer() {
        let k = intern_key("sharedBufferStem");
        let stem = key_to_js_string(&k);
        let longer = stem.concat_units(&"Suffix".encode_utf16().collect::<Vec<u16>>());
        assert_eq!(
            key_from_js_string(&longer).as_bytes(),
            b"sharedBufferStemSuffix"
        );
        assert_eq!(key_from_js_string(&stem), k);
    }

    #[test]
    fn name_strings_follow_the_text_not_its_address() {
        let mut text = String::from("nameCacheAAAA");
        let first = name_to_js_string(&text);
        assert_eq!(first.to_rust_string(), "nameCacheAAAA");
        assert!(name_to_js_string(&text).ptr_eq(&first));
        assert!(first.ptr_eq(&key_to_js_string(&intern_key("nameCacheAAAA"))));
        text.replace_range(.., "nameCacheBBBB");
        assert_eq!(name_to_js_string(&text).to_rust_string(), "nameCacheBBBB");
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
        assert_eq!(a.symbol_encoding(), Some(&b"Symbol(desc)#42"[..]));
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
        let description = crate::types::JsString::from_str(&"s".repeat(MAX_CACHEABLE_KEY_BYTES));
        let original = crate::types::JsSymbol::new(99, Some(description.clone())).to_property_key();
        let again = crate::types::JsSymbol::new(99, Some(description));
        let a = cache.intern(again.to_property_key());
        let b = cache.intern(
            crate::types::JsSymbol::new(99, again.description().cloned()).to_property_key(),
        );

        assert_eq!(a, original);
        assert!(a.is_symbol());
        assert_eq!(a.symbol_encoding(), original.symbol_encoding());
        assert!(
            !a.shares_storage_with(&b),
            "oversized Symbol keys must follow the non-retaining path"
        );
    }
}
