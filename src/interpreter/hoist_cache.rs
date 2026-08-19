//! Bounded memoisation of per-Body hoisting analysis.
//!
//! `dispatch_body` runs the var/Annex-B *name collection* of `super::hoisting`
//! once per Body and reuses it on later calls (#72), keyed by the identity of
//! the Body's statement `Rc`. Each entry pins that `Rc`, so an unbounded map
//! retains every Body it has ever seen — including the short-lived Bodies that
//! `new Function` and `eval` produce (#165). The cache is therefore
//! capacity-bounded with approximate-LRU eviction: when a new Body would exceed
//! `DEFAULT_CAPACITY`, the older half of the entries is dropped in one sweep.
//! Entries are pure memoisation, so eviction only ever costs a re-walk.
//!
//! Bounding this cache is necessary but not sufficient to let a dead Body be
//! freed: `super::ic_store` pins the same Bodies and does not yet evict (#468).

use rustc_hash::FxHashMap;
use std::rc::Rc;

use crate::ast::{Body, Statement};

/// Maximum number of memoised Bodies. Large enough that a program with a fixed
/// set of functions never evicts; the bound is there for Body-churning
/// workloads (`new Function` / `eval` in a loop).
pub(crate) const DEFAULT_CAPACITY: usize = 8192;

/// Cached output of the var/Annex-B hoisting *name collection* for a single
/// Body (#72).
///
/// Only the raw name collection is cached. The Annex-B post-processing that
/// inspects live env/parameter/lexical state still runs per call, and function
/// declarations are never cached as values (fresh closures are built per call).
pub(crate) struct HoistAnalysis {
    /// Deduped output of `collect_var_names_from_stmts`.
    pub(crate) var_names: Vec<String>,
    /// Raw `names` output of `collect_annexb_function_names`. (The companion
    /// `blocked` accumulator is internal to the walk and discarded afterwards
    /// by the original code, so it is intentionally not cached.)
    pub(crate) annexb_names: Vec<String>,
    /// Pins the Body so its `Rc::as_ptr` — this entry's key — cannot be reused
    /// by a freed-then-reallocated Body (ABA). Entries are built only here and
    /// eviction drops key and pin together, so a key names a live Body for as
    /// long as the key exists.
    _body: Rc<Vec<Statement>>,
}

struct Entry {
    analysis: Rc<HoistAnalysis>,
    /// Clock value of this entry's most recent lookup or insertion.
    last_used: u64,
}

/// Capacity-bounded, approximate-LRU cache of `HoistAnalysis` keyed by Body
/// identity. The key `*const Vec<Statement>` is an identity only — never
/// dereferenced — and cannot go stale, ASTs being immutable post-parse.
pub(crate) struct HoistCache {
    entries: FxHashMap<*const Vec<Statement>, Entry>,
    /// Entry ceiling, floored at 2: a sweep keeps `ceil(len / 2)`, so only from
    /// two entries up is it guaranteed to free a slot for the incoming one.
    capacity: usize,
    clock: u64,
    #[cfg(test)]
    hits: u64,
}

impl HoistCache {
    pub(crate) fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: FxHashMap::default(),
            capacity: capacity.max(2),
            clock: 0,
            #[cfg(test)]
            hits: 0,
        }
    }

    /// Return `body`'s memoised analysis, running `analyse` over its statements
    /// and memoising the `(var_names, annexb_names)` it returns on a miss. A
    /// miss that would exceed the capacity evicts first.
    pub(crate) fn get_or_insert_with(
        &mut self,
        body: &Body,
        analyse: impl FnOnce(&[Statement]) -> (Vec<String>, Vec<String>),
    ) -> Rc<HoistAnalysis> {
        let key = Rc::as_ptr(&body.statements);
        self.clock += 1;
        let clock = self.clock;
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_used = clock;
            let analysis = entry.analysis.clone();
            #[cfg(test)]
            {
                self.hits += 1;
            }
            return analysis;
        }
        let (var_names, annexb_names) = analyse(body.as_slice());
        if self.entries.len() >= self.capacity {
            self.evict_older_half();
        }
        let analysis = Rc::new(HoistAnalysis {
            var_names,
            annexb_names,
            _body: body.statements.clone(),
        });
        self.clock += 1;
        self.entries.insert(
            key,
            Entry {
                analysis: analysis.clone(),
                last_used: self.clock,
            },
        );
        analysis
    }

    /// Number of memoised Bodies.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Number of lookups served from the cache.
    #[cfg(test)]
    pub(crate) fn hits(&self) -> u64 {
        self.hits
    }

    /// Drop the older half of the entries by `last_used`, keeping
    /// `ceil(len / 2)` — so the most-recently-used entry always survives.
    /// Halving rather than evicting a single entry keeps this an amortised
    /// O(1)-per-insert sweep. `last_used` values are unique, the clock advancing
    /// on every lookup and insertion, so the cutoff cannot tie.
    fn evict_older_half(&mut self) {
        let mut ticks: Vec<u64> = self.entries.values().map(|e| e.last_used).collect();
        let mid = ticks.len() / 2;
        let (_, &mut cutoff, _) = ticks.select_nth_unstable(mid);
        self.entries.retain(|_, e| e.last_used >= cutoff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(n: usize) -> Body {
        // Distinct allocations; the contents are irrelevant to the cache.
        Body::new(vec![Statement::Empty; n])
    }

    fn analyse(_: &[Statement]) -> (Vec<String>, Vec<String>) {
        (vec!["x".to_string()], Vec::new())
    }

    fn insert(cache: &mut HoistCache, b: &Body) {
        cache.get_or_insert_with(b, analyse);
    }

    /// A lookup that must not be a miss: the analyser panics if it runs.
    fn expect_cached(cache: &mut HoistCache, b: &Body) -> Rc<HoistAnalysis> {
        cache.get_or_insert_with(b, |_| panic!("expected a cached entry"))
    }

    #[test]
    fn memoises_by_body_identity() {
        let mut cache = HoistCache::new();
        let b = body(1);
        insert(&mut cache, &b);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.hits(), 0);
        let hit = expect_cached(&mut cache, &b);
        assert_eq!(hit.var_names, vec!["x".to_string()]);
        assert_eq!(cache.hits(), 1);
    }

    #[test]
    fn distinct_bodies_get_distinct_entries() {
        let mut cache = HoistCache::new();
        let a = body(1);
        let b = body(2);
        insert(&mut cache, &a);
        insert(&mut cache, &b);
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.hits(), 0);
    }

    #[test]
    fn stays_within_capacity_across_many_bodies() {
        let mut cache = HoistCache::with_capacity(8);
        // Bodies are dropped immediately, exactly as the short-lived Bodies of
        // dynamic functions are; only the cache's own pin could retain them.
        for i in 0..1000 {
            let b = body(i % 4 + 1);
            insert(&mut cache, &b);
            assert!(cache.len() <= 8, "cache grew to {}", cache.len());
        }
    }

    #[test]
    fn eviction_releases_the_pinned_body() {
        let mut cache = HoistCache::with_capacity(2);
        let pinned = body(1);
        let weak = Rc::downgrade(&pinned.statements);
        insert(&mut cache, &pinned);
        drop(pinned);
        assert!(weak.upgrade().is_some(), "cache must pin a live entry");
        for i in 0..8 {
            let b = body(i + 2);
            insert(&mut cache, &b);
        }
        assert!(
            weak.upgrade().is_none(),
            "evicted entry must release its Body"
        );
    }

    #[test]
    fn recently_used_entries_survive_eviction() {
        let mut cache = HoistCache::with_capacity(4);
        let hot = body(1);
        insert(&mut cache, &hot);
        let cold: Vec<Body> = (0..3).map(|i| body(i + 2)).collect();
        for b in &cold {
            insert(&mut cache, b);
        }
        // Touch the oldest entry so it becomes the most-recently used, then
        // force a sweep with a new Body.
        expect_cached(&mut cache, &hot);
        insert(&mut cache, &body(99));
        expect_cached(&mut cache, &hot);
    }

    #[test]
    fn a_sweep_at_the_capacity_floor_still_keeps_the_newest_entry() {
        // The floor is where a naive "drop everything at or below the median"
        // cutoff would evict the most-recently-used entry along with the rest.
        let mut cache = HoistCache::with_capacity(2);
        let first = body(1);
        let second = body(2);
        insert(&mut cache, &first);
        insert(&mut cache, &second);
        insert(&mut cache, &body(3)); // sweeps
        assert_eq!(cache.len(), 2);
        expect_cached(&mut cache, &second);
    }
}
