//! Bounded memoisation of per-Body hoisting analysis.
//!
//! `dispatch_body` needs the var and Annex-B declared names of the Body it is
//! about to run, and re-walking the statements on every call is pure waste, so
//! the collection is memoised per Body (#72). Each entry pins its Body — the
//! ABA contract on `Body::key` — which means an unbounded map would retain
//! every Body it has ever seen, including the short-lived ones that
//! `new Function` and `eval` produce (#165). The cache is therefore
//! capacity-bounded with approximate-LRU eviction. Entries are pure
//! memoisation, so eviction only ever costs a re-walk.
//!
//! Bounding this cache is necessary but not sufficient to let a dead Body be
//! freed: `super::ic_store` pins the same Bodies and does not yet evict (#468).

use rustc_hash::FxHashMap;
use std::collections::HashSet;
use std::rc::Rc;

use crate::ast::{Body, Statement};
use crate::interpreter::Interpreter;

/// Maximum number of memoised Bodies. Large enough that a program with a fixed
/// set of functions never evicts; the bound is there for Body-churning
/// workloads (`new Function` / `eval` in a loop), which retain a Body per entry.
pub(crate) const DEFAULT_CAPACITY: usize = 8192;

/// The cached half of one Body's hoisting analysis (#72).
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
}

struct Entry {
    analysis: Rc<HoistAnalysis>,
    /// Recency tick of this entry's most recent call.
    last_used: u64,
    /// Pins the Body for as long as this entry's key lives, per `Body::key`.
    _body: Rc<Vec<Statement>>,
}

/// Capacity-bounded, approximate-LRU memo of `HoistAnalysis`, keyed by
/// `Body::key`. It owns the key, the recency metadata, the pin, and the
/// analysis itself, so no caller can pair a key with a different analysis or
/// update one part of an entry without the others.
pub(crate) struct HoistCache {
    entries: FxHashMap<*const Vec<Statement>, Entry>,
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

    /// Return `body`'s analysis, collecting and memoising it on a miss — which
    /// is either a Body not seen before or one whose entry has been evicted.
    pub(crate) fn analysis_for(&mut self, body: &Body) -> Rc<HoistAnalysis> {
        let key = body.key();
        self.clock += 1;
        let tick = self.clock;
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_used = tick;
            let analysis = entry.analysis.clone();
            #[cfg(test)]
            {
                self.hits += 1;
            }
            return analysis;
        }
        let analysis = Rc::new(collect_names(body.as_slice()));
        if self.entries.len() >= self.capacity {
            self.evict_older_half();
        }
        self.entries.insert(
            key,
            Entry {
                analysis: analysis.clone(),
                last_used: tick,
                _body: body.statements.clone(),
            },
        );
        analysis
    }

    /// Number of memoised Bodies.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Number of calls served from the memo.
    #[cfg(test)]
    pub(crate) fn hits(&self) -> u64 {
        self.hits
    }

    /// Drop the older half of the entries by `last_used`, keeping the newer
    /// `ceil(len / 2)` — so the most-recently-used entry always survives, and
    /// with the capacity floored at two a sweep always frees a slot for the
    /// incoming Body. Halving rather than evicting a single entry keeps this an
    /// amortised O(1)-per-insert sweep. Ticks are unique, one per call, so the
    /// cutoff cannot tie. Each removal drops the entry's pin with its key.
    fn evict_older_half(&mut self) {
        let mut ticks: Vec<u64> = self.entries.values().map(|e| e.last_used).collect();
        let mid = ticks.len() / 2;
        let (_, &mut cutoff, _) = ticks.select_nth_unstable(mid);
        self.entries.retain(|_, e| e.last_used >= cutoff);
    }
}

/// Collect the var and Annex-B declared names of one Body: the analysis this
/// module memoises, and the only thing it ever stores under a Body's key.
fn collect_names(stmts: &[Statement]) -> HoistAnalysis {
    let mut var_set = HashSet::new();
    Interpreter::collect_var_names_from_stmts(stmts, &mut var_set);
    let mut annexb_names = Vec::new();
    let mut blocked = Vec::new();
    Interpreter::collect_annexb_function_names(stmts, &mut annexb_names, &mut blocked);
    HoistAnalysis {
        var_names: var_set.into_iter().collect(),
        annexb_names,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(n: usize) -> Body {
        // Distinct allocations; the statements are irrelevant to the cache, and
        // what the analysis computes for real ones is covered by the
        // interpreter-level tests and test262.
        Body::new(vec![Statement::Empty; n])
    }

    /// A call that must be served from the memo, not recollected.
    fn assert_hit(cache: &mut HoistCache, b: &Body) {
        let before = cache.hits();
        cache.analysis_for(b);
        assert_eq!(cache.hits(), before + 1, "expected a memoised entry");
    }

    #[test]
    fn memoises_by_body_identity() {
        let mut cache = HoistCache::new();
        let b = body(1);
        let first = cache.analysis_for(&b);
        assert_eq!(cache.len(), 1);
        let second = cache.analysis_for(&b);
        assert!(
            Rc::ptr_eq(&first, &second),
            "a second call must reuse the memoised analysis"
        );
    }

    #[test]
    fn hits_count_only_calls_served_from_the_memo() {
        let mut cache = HoistCache::new();
        let b = body(1);
        cache.analysis_for(&b);
        assert_eq!(cache.hits(), 0, "the first call is a miss");
        cache.analysis_for(&b);
        cache.analysis_for(&b);
        assert_eq!(cache.hits(), 2);
    }

    #[test]
    fn distinct_bodies_get_distinct_entries() {
        let mut cache = HoistCache::new();
        let a = body(1);
        let b = body(2);
        cache.analysis_for(&a);
        cache.analysis_for(&b);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn stays_within_capacity_across_many_bodies() {
        let mut cache = HoistCache::with_capacity(8);
        // Bodies are dropped immediately, exactly as the short-lived Bodies of
        // dynamic functions are; only the cache's own pin could retain them.
        for i in 0..1000 {
            let b = body(i % 4 + 1);
            cache.analysis_for(&b);
            assert!(cache.len() <= 8, "cache grew to {}", cache.len());
        }
    }

    #[test]
    fn eviction_releases_the_pinned_body() {
        let mut cache = HoistCache::with_capacity(2);
        let pinned = body(1);
        let weak = Rc::downgrade(&pinned.statements);
        cache.analysis_for(&pinned);
        drop(pinned);
        assert!(weak.upgrade().is_some(), "cache must pin a live entry");
        for i in 0..8 {
            cache.analysis_for(&body(i + 2));
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
        cache.analysis_for(&hot);
        let cold: Vec<Body> = (0..3).map(|i| body(i + 2)).collect();
        for b in &cold {
            cache.analysis_for(b);
        }
        // Touch the oldest entry so it becomes the most-recently used, then
        // force a sweep with a new Body.
        assert_hit(&mut cache, &hot);
        cache.analysis_for(&body(99));
        assert_hit(&mut cache, &hot);
    }

    #[test]
    fn a_sweep_at_the_capacity_floor_still_keeps_the_newest_entry() {
        // A cutoff that dropped its own median tick would empty the cache here,
        // evicting the newest entry along with the oldest.
        let mut cache = HoistCache::with_capacity(2);
        let first = body(1);
        let second = body(2);
        cache.analysis_for(&first);
        cache.analysis_for(&second);
        cache.analysis_for(&body(3)); // sweeps
        assert_eq!(cache.len(), 2);
        assert_hit(&mut cache, &second);
    }
}
