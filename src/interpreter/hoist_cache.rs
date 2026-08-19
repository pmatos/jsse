//! Bounded memoisation of per-function-body hoisting analysis.
//!
//! `dispatch_body` runs the var/Annex-B *name collection* of
//! `super::hoisting` once per function body and reuses it on later calls (#72),
//! keyed by the identity of the body's statement `Rc`. Each entry pins that
//! `Rc` so its address cannot be reused by a freed-then-reallocated body
//! (ABA), which means an unbounded map would retain every body it has ever
//! seen — including the short-lived bodies of `new Function` / `eval` (#165).
//!
//! (Bounding this cache is necessary but not sufficient to let such a body be
//! freed: `super::ic_store` pins the same bodies and does not yet evict — #468.)
//!
//! The cache is therefore capacity-bounded with approximate-LRU eviction: when
//! a new body would exceed `DEFAULT_CAPACITY`, the least-recently-used half of
//! the entries is dropped in one sweep. Entries are pure memoisation, so eviction only ever
//! costs a re-walk of the body. Eviction removes the map key and its pinned
//! body together, so every surviving key still names a live body and the ABA
//! invariant holds.

use rustc_hash::FxHashMap;
use std::rc::Rc;

use crate::ast::Statement;

/// Maximum number of memoised bodies. Large enough that a program with a fixed
/// set of functions never evicts — the bound exists for body-churning workloads
/// (`new Function` / `eval` in a loop). An entry costs a few hundred bytes plus
/// the body it pins, so a full cache stays in the low megabytes for the small
/// bodies such workloads produce.
pub(crate) const DEFAULT_CAPACITY: usize = 8192;

/// Cached output of the var/Annex-B hoisting *name collection* for a single
/// function body (#72).
///
/// Only the raw name collection is cached. The Annex-B post-processing that
/// inspects live env/parameter/lexical state still runs per call, and function
/// declarations are never cached as values (fresh closures are built per call).
/// The body `Rc` is pinned to prevent pointer (ABA) reuse from aliasing a
/// freed body onto a fresh one with the same address.
pub(crate) struct HoistAnalysis {
    /// Deduped output of `collect_var_names_from_stmts`.
    pub(crate) var_names: Vec<String>,
    /// Raw `names` output of `collect_annexb_function_names`. (The companion
    /// `blocked` accumulator is internal to the walk and discarded afterwards
    /// by the original code, so it is intentionally not cached.)
    pub(crate) annexb_names: Vec<String>,
    /// Pins the body so its `Rc::as_ptr` key cannot be reused for another body.
    _body: Rc<Vec<Statement>>,
}

struct Entry {
    analysis: Rc<HoistAnalysis>,
    /// Clock value of this entry's most recent lookup or insertion.
    last_used: u64,
}

/// Capacity-bounded, approximate-LRU cache of `HoistAnalysis` keyed by body
/// identity. The key `*const Vec<Statement>` is an identity only — never
/// dereferenced. Entries cannot go stale: ASTs are immutable post-parse and
/// each entry pins the body it is keyed by.
pub(crate) struct HoistCache {
    entries: FxHashMap<*const Vec<Statement>, Entry>,
    capacity: usize,
    clock: u64,
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
            hits: 0,
        }
    }

    /// Return the memoised analysis for `body`, marking it most-recently used.
    pub(crate) fn get(&mut self, body: &Rc<Vec<Statement>>) -> Option<Rc<HoistAnalysis>> {
        let key = Rc::as_ptr(body);
        self.clock += 1;
        let clock = self.clock;
        let entry = self.entries.get_mut(&key)?;
        entry.last_used = clock;
        let analysis = entry.analysis.clone();
        self.hits += 1;
        Some(analysis)
    }

    /// Memoise the analysis of `body`, evicting the least-recently-used half of
    /// the cache first if this entry would exceed the capacity.
    pub(crate) fn insert(
        &mut self,
        body: &Rc<Vec<Statement>>,
        var_names: Vec<String>,
        annexb_names: Vec<String>,
    ) -> Rc<HoistAnalysis> {
        let key = Rc::as_ptr(body);
        if self.entries.len() >= self.capacity && !self.entries.contains_key(&key) {
            self.evict_lru_half();
        }
        let analysis = Rc::new(HoistAnalysis {
            var_names,
            annexb_names,
            _body: body.clone(),
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

    /// Number of memoised bodies. Whitebox accessor used by the tests that
    /// pin down the capacity bound; not exposed to JS.
    #[allow(dead_code)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Number of lookups served from the cache. Whitebox accessor used by the
    /// test that pins down that repeated calls still reuse one analysis; not
    /// exposed to JS.
    #[allow(dead_code)]
    pub(crate) fn hits(&self) -> u64 {
        self.hits
    }

    /// Drop the older half of the entries by `last_used` (the median entry
    /// included, so at least `capacity / 2` slots come free). Halving rather
    /// than evicting a single entry keeps this an amortised O(1)-per-insert
    /// sweep. Each removal drops the entry's pinned body together with its key,
    /// so no surviving key can be aliased by a later body at the same address.
    fn evict_lru_half(&mut self) {
        let mut ticks: Vec<u64> = self.entries.values().map(|e| e.last_used).collect();
        let mid = ticks.len() / 2;
        let (_, &mut cutoff, _) = ticks.select_nth_unstable(mid);
        self.entries.retain(|_, e| e.last_used > cutoff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(n: usize) -> Rc<Vec<Statement>> {
        // Distinct allocations; contents are irrelevant to the cache.
        Rc::new(vec![Statement::Empty; n])
    }

    fn insert(cache: &mut HoistCache, b: &Rc<Vec<Statement>>) {
        cache.insert(b, vec!["x".to_string()], Vec::new());
    }

    #[test]
    fn memoises_by_body_identity() {
        let mut cache = HoistCache::new();
        let b = body(1);
        assert!(cache.get(&b).is_none());
        insert(&mut cache, &b);
        let hit = cache.get(&b).expect("cached analysis");
        assert_eq!(hit.var_names, vec!["x".to_string()]);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn hits_count_only_lookups_served_from_the_cache() {
        let mut cache = HoistCache::new();
        let b = body(1);
        assert!(cache.get(&b).is_none());
        assert_eq!(cache.hits(), 0);
        insert(&mut cache, &b);
        assert!(cache.get(&b).is_some());
        assert!(cache.get(&b).is_some());
        assert_eq!(cache.hits(), 2);
    }

    #[test]
    fn distinct_bodies_get_distinct_entries() {
        let mut cache = HoistCache::new();
        let a = body(1);
        let b = body(2);
        insert(&mut cache, &a);
        insert(&mut cache, &b);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn stays_within_capacity_across_many_bodies() {
        let mut cache = HoistCache::with_capacity(8);
        // Bodies are dropped immediately, exactly as short-lived dynamic
        // function bodies are; only the cache's own pin could retain them.
        for i in 0..1000 {
            let b = body(i % 4 + 1);
            insert(&mut cache, &b);
            assert!(cache.len() <= 8, "cache grew to {}", cache.len());
        }
        assert!(cache.len() <= 8);
    }

    #[test]
    fn eviction_releases_the_pinned_body() {
        let mut cache = HoistCache::with_capacity(2);
        let pinned = body(1);
        let weak = Rc::downgrade(&pinned);
        insert(&mut cache, &pinned);
        drop(pinned);
        assert!(weak.upgrade().is_some(), "cache must pin a live entry");
        for i in 0..8 {
            let b = body(i + 2);
            insert(&mut cache, &b);
        }
        assert!(
            weak.upgrade().is_none(),
            "evicted entry must release its body"
        );
    }

    #[test]
    fn recently_used_entries_survive_eviction() {
        let mut cache = HoistCache::with_capacity(4);
        let hot = body(1);
        insert(&mut cache, &hot);
        let mut cold = Vec::new();
        for i in 0..3 {
            let b = body(i + 2);
            insert(&mut cache, &b);
            cold.push(b);
        }
        // Touch the first body so it is the most-recently used, then force a
        // sweep by inserting a new body.
        assert!(cache.get(&hot).is_some());
        let fresh = body(99);
        insert(&mut cache, &fresh);
        assert!(
            cache.get(&hot).is_some(),
            "most-recently-used entry must survive eviction"
        );
    }

    #[test]
    fn reinsert_of_a_cached_body_does_not_evict() {
        let mut cache = HoistCache::with_capacity(2);
        let a = body(1);
        let b = body(2);
        insert(&mut cache, &a);
        insert(&mut cache, &b);
        insert(&mut cache, &b);
        assert_eq!(cache.len(), 2);
        assert!(cache.get(&a).is_some());
    }
}
