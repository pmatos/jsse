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

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hash, Hasher, RandomState};
use std::rc::{Rc, Weak};

struct AtomData {
    hash: u64,
    text: Box<str>,
}

/// A canonical identifier. Constructible only through [`Atom::intern`], so two
/// atoms with equal text are always `Rc::ptr_eq`.
#[derive(Clone)]
pub(crate) struct Atom(Rc<AtomData>);

impl Atom {
    pub(crate) fn intern(text: &str) -> Atom {
        #[cfg(feature = "perf-counters")]
        INTERN_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        INTERNER.with(|i| i.borrow_mut().intern(text))
    }

    /// The atom for `text` if one is currently alive. A `None` means no
    /// environment/chunk anywhere holds that name, and inserts nothing.
    pub(crate) fn lookup(text: &str) -> Option<Atom> {
        #[cfg(feature = "perf-counters")]
        LOOKUP_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        INTERNER.with(|i| i.borrow().lookup(text))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0.text
    }

    #[cfg(test)]
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

struct Interner {
    map: HashMap<Box<str>, Weak<AtomData>>,
    hasher: RandomState,
    purge_at: usize,
}

impl Interner {
    fn new() -> Self {
        Interner {
            map: HashMap::new(),
            hasher: RandomState::new(),
            purge_at: MIN_PURGE_LEN,
        }
    }

    fn lookup(&self, text: &str) -> Option<Atom> {
        self.map.get(text).and_then(Weak::upgrade).map(Atom)
    }

    fn intern(&mut self, text: &str) -> Atom {
        if let Some(weak) = self.map.get_mut(text) {
            if let Some(live) = weak.upgrade() {
                return Atom(live);
            }
            let data = Rc::new(AtomData {
                hash: self.hasher.hash_one(text),
                text: Box::from(text),
            });
            *weak = Rc::downgrade(&data);
            return Atom(data);
        }
        if self.map.len() >= self.purge_at {
            self.map.retain(|_, w| w.strong_count() > 0);
            self.purge_at = (self.map.len() * 2).max(MIN_PURGE_LEN);
        }
        let data = Rc::new(self.new_data(text));
        self.map.insert(Box::from(text), Rc::downgrade(&data));
        Atom(data)
    }

    fn new_data(&self, text: &str) -> AtomData {
        AtomData {
            hash: self.hasher.hash_one(text),
            text: Box::from(text),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn interner_len() -> usize {
        INTERNER.with(|i| i.borrow().map.len())
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
    fn lookup_of_unknown_text_inserts_nothing() {
        let before = interner_len();
        assert!(Atom::lookup("never-interned-atom-test").is_none());
        assert_eq!(interner_len(), before);
        let keep = Atom::intern("interned-atom-test");
        assert!(Atom::lookup("interned-atom-test").is_some_and(|a| a.ptr_eq(&keep)));
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
        assert!(Atom::lookup("π日本\u{1F600}").is_some());
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
