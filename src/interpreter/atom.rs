//! Identifier atoms (issue #898).
//!
//! Binding names come from script source, so they must never be hashed by a
//! predictable hasher (#393, `docs/specs/2026-07-25-environment-binding-hashing-design.md`).
//! An [`Atom`] is a canonical, reference-counted identifier whose hash is
//! computed exactly once, by the randomised std `RandomState` of the interner
//! below. Everything downstream (environment index maps, slot hints) compares
//! atoms by pointer and hashes them by writing that stored `u64` through an
//! identity hasher, so no unkeyed hash ever sees script-controlled bytes.
//!
//! The interner is a `thread_local!` (like `key_intern`) holding only `Weak`
//! references: an atom lives exactly as long as some environment, chunk or
//! layout holds it. Dead entries are purged whenever the table doubles, which
//! bounds memory under `eval`/`new Function` churn.

use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::hash::{BuildHasher, Hash, Hasher, RandomState};
use std::rc::{Rc, Weak};
use std::sync::OnceLock;

use crate::types::JsPropertyKey;

/// The process-wide keyed hash for identifier text. Every atom stores
/// `hash_text(its text)`, and `BindingMap` hashes a `&str` probe the same way,
/// so an index probe needs no interner access at all.
static TEXT_HASHER: OnceLock<RandomState> = OnceLock::new();

#[inline]
pub(crate) fn hash_text(text: &str) -> u64 {
    TEXT_HASHER.get_or_init(RandomState::new).hash_one(text)
}

struct AtomData {
    hash: u64,
    text: Box<str>,
    /// The interned property key for this text, built on first use so a
    /// member access by identifier name never re-hashes or re-allocates it.
    key: OnceCell<JsPropertyKey>,
}

/// A canonical identifier. Constructible only through [`Atom::intern`], so two
/// atoms with equal text are always `Rc::ptr_eq`.
#[derive(Clone)]
pub(crate) struct Atom(Rc<AtomData>);

impl Atom {
    /// An uninterned atom carrying a chosen hash, to exercise hash-collision
    /// handling in `BindingMap`. Compares by pointer like any other atom.
    #[cfg(test)]
    pub(crate) fn with_hash_for_test(text: &str, hash: u64) -> Atom {
        Atom(Rc::new(AtomData {
            hash,
            text: Box::from(text),
            key: OnceCell::new(),
        }))
    }

    pub(crate) fn intern(text: &str) -> Atom {
        #[cfg(feature = "perf-counters")]
        INTERN_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        INTERNER.with(|i| i.borrow_mut().intern(text))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0.text
    }

    /// This identifier as an interned property key, computed once per atom.
    #[inline]
    pub(crate) fn property_key(&self) -> &JsPropertyKey {
        self.0
            .key
            .get_or_init(|| crate::interpreter::key_intern::intern_key(&self.0.text))
    }

    #[inline]
    pub(crate) fn precomputed_hash(&self) -> u64 {
        self.0.hash
    }

    #[inline]
    pub(crate) fn ptr_eq(&self, other: &Atom) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl PartialEq for Atom {
    fn eq(&self, other: &Atom) -> bool {
        self.ptr_eq(other)
    }
}

impl Eq for Atom {}

impl Hash for Atom {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.0.hash);
    }
}

impl std::fmt::Debug for Atom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&*self.0.text, f)
    }
}

impl std::ops::Deref for Atom {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0.text
    }
}

impl AsRef<str> for Atom {
    fn as_ref(&self) -> &str {
        &self.0.text
    }
}

/// Hashes an `Atom` by passing its precomputed `u64` straight through.
#[derive(Default, Clone, Copy)]
pub(crate) struct IdentityHasher(u64);

impl Hasher for IdentityHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, _bytes: &[u8]) {
        unreachable!("IdentityHasher is only used with Atom keys");
    }

    fn write_u64(&mut self, n: u64) {
        self.0 = n;
    }
}

pub(crate) type IdentityBuild = std::hash::BuildHasherDefault<IdentityHasher>;

const MIN_PURGE_LEN: usize = 1024;

/// Buckets are keyed by the atom's own randomised hash (already uniformly
/// random, so the table hashes the key by identity). Almost every bucket holds
/// a single weak reference.
struct Interner {
    buckets: HashMap<u64, Vec<Weak<AtomData>>, IdentityBuild>,
    /// Weak entries across all buckets, dead ones included.
    entries: usize,
    purge_at: usize,
}

impl Interner {
    fn new() -> Self {
        Interner {
            buckets: HashMap::default(),
            entries: 0,
            purge_at: MIN_PURGE_LEN,
        }
    }

    fn intern(&mut self, text: &str) -> Atom {
        let hash = hash_text(text);
        if let Some(bucket) = self.buckets.get_mut(&hash) {
            let mut dead_slot = None;
            for (i, weak) in bucket.iter().enumerate() {
                match weak.upgrade() {
                    Some(live) if &*live.text == text => return Atom(live),
                    Some(_) => {}
                    None => dead_slot = dead_slot.or(Some(i)),
                }
            }
            let data = Rc::new(AtomData {
                hash,
                text: Box::from(text),
                key: OnceCell::new(),
            });
            let weak = Rc::downgrade(&data);
            match dead_slot {
                Some(i) => bucket[i] = weak,
                None => {
                    bucket.push(weak);
                    self.entries += 1;
                }
            }
            return Atom(data);
        }
        if self.entries >= self.purge_at {
            self.purge();
        }
        let data = Rc::new(AtomData {
            hash,
            text: Box::from(text),
            key: OnceCell::new(),
        });
        self.buckets.insert(hash, vec![Rc::downgrade(&data)]);
        self.entries += 1;
        Atom(data)
    }

    /// Drops every dead weak reference (and emptied bucket), then lets the
    /// table grow to twice what survived before the next purge.
    fn purge(&mut self) {
        self.buckets.retain(|_, bucket| {
            bucket.retain(|w| w.strong_count() > 0);
            !bucket.is_empty()
        });
        self.entries = self.buckets.values().map(Vec::len).sum();
        self.purge_at = (self.entries * 2).max(MIN_PURGE_LEN);
    }
}

thread_local! {
    static INTERNER: RefCell<Interner> = RefCell::new(Interner::new());
}

/// Calls into the (hashing) interner, for the `perf-counters` report: a call
/// path that interns or looks up per call shows up as counts that scale with
/// the call count.
#[cfg(feature = "perf-counters")]
static INTERN_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(feature = "perf-counters")]
static LOOKUP_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(feature = "perf-counters")]
pub(crate) fn intern_calls() -> u64 {
    INTERN_CALLS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(feature = "perf-counters")]
pub(crate) fn lookup_calls() -> u64 {
    LOOKUP_CALLS.load(std::sync::atomic::Ordering::Relaxed)
}

/// A `&str` probe of an indexed binding map (it hashes the text).
#[cfg(feature = "perf-counters")]
pub(crate) fn count_text_probe() {
    LOOKUP_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interner_len() -> usize {
        INTERNER.with(|i| i.borrow().entries)
    }

    #[test]
    fn same_text_is_pointer_equal() {
        let a = Atom::intern("alpha");
        let b = Atom::intern("alpha");
        let c = Atom::intern("beta");
        assert!(a.ptr_eq(&b));
        assert!(!a.ptr_eq(&c));
        assert_eq!(a.as_str(), "alpha");
    }

    #[test]
    fn stored_hash_is_the_keyed_text_hash() {
        let a = Atom::intern("keyed-hash-atom-test");
        assert_eq!(a.precomputed_hash(), hash_text("keyed-hash-atom-test"));
        assert_ne!(a.precomputed_hash(), hash_text("keyed-hash-atom-test2"));
    }

    #[test]
    fn dropped_atoms_are_purged() {
        for i in 0..100_000 {
            let _ = Atom::intern(&format!("churn-{i}"));
        }
        assert!(interner_len() <= 2 * MIN_PURGE_LEN + 64);
    }

    #[test]
    fn dead_entry_is_reinterned_canonically() {
        drop(Atom::intern("revived-atom-test"));
        let a = Atom::intern("revived-atom-test");
        let b = Atom::intern("revived-atom-test");
        assert!(a.ptr_eq(&b));
    }

    #[test]
    fn non_ascii_round_trips() {
        let a = Atom::intern("π日本\u{1F600}");
        assert_eq!(a.as_str(), "π日本\u{1F600}");
        assert!(a.ptr_eq(&Atom::intern("π日本\u{1F600}")));
    }

    #[test]
    fn hash_writes_the_stored_value() {
        struct Capture(Option<u64>);
        impl Hasher for Capture {
            fn finish(&self) -> u64 {
                self.0.unwrap_or(0)
            }
            fn write(&mut self, _: &[u8]) {
                panic!("atom hashing must not feed bytes");
            }
            fn write_u64(&mut self, n: u64) {
                self.0 = Some(n);
            }
        }
        let a = Atom::intern("hashed-atom-test");
        let mut h = Capture(None);
        a.hash(&mut h);
        assert_eq!(h.0, Some(a.precomputed_hash()));
    }
}
