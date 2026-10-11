//! Bounded, Body-keyed memo for results that are pure functions of a Body.
//!
//! Closures created from the same function literal share one `Body` (its
//! statements `Rc`), so anything derived from the Body alone can be computed
//! once and reused by every closure and every call: the bytecode compile
//! verdict (#900) and the generator/async state machine (#914).
//!
//! Each entry pins its Body — the ABA contract on `Body::key` — so the memo is
//! capacity-bounded with the same approximate-LRU halving as `hoist_cache`,
//! letting the short-lived Bodies that `new Function` and `eval` produce be
//! freed (#165). Entries are pure memoisation: eviction only costs a recompute.

use rustc_hash::FxHashMap;
use std::rc::Rc;

use crate::ast::{Body, Statement};

/// Maximum number of memoised Bodies, matching `hoist_cache` and `ic_store`.
pub(crate) const DEFAULT_CAPACITY: usize = 8192;

struct Entry<V> {
    value: V,
    /// Recency tick of this entry's most recent lookup.
    last_used: u64,
    /// Pins the Body for as long as this entry's key lives, per `Body::key`.
    _body: Rc<Vec<Statement>>,
}

pub(crate) struct BodyMemo<V> {
    entries: FxHashMap<*const Vec<Statement>, Entry<V>>,
    capacity: usize,
    clock: u64,
}

impl<V: Clone> BodyMemo<V> {
    pub(crate) fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: FxHashMap::default(),
            capacity: capacity.max(2),
            clock: 0,
        }
    }

    /// The memoised value for `body`, if any.
    pub(crate) fn get(&mut self, body: &Body) -> Option<V> {
        self.clock += 1;
        let tick = self.clock;
        let entry = self.entries.get_mut(&body.key())?;
        entry.last_used = tick;
        Some(entry.value.clone())
    }

    /// Memoise `value` for `body`, replacing any earlier value.
    pub(crate) fn insert(&mut self, body: &Body, value: V) {
        let key = body.key();
        self.clock += 1;
        let tick = self.clock;
        if !self.entries.contains_key(&key) && self.entries.len() >= self.capacity {
            self.evict_older_half();
        }
        self.entries.insert(
            key,
            Entry {
                value,
                last_used: tick,
                _body: body.statements.clone(),
            },
        );
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Drop the older half of the entries by `last_used`, keeping the newer
    /// `ceil(len / 2)`, so a sweep always frees a slot for the incoming Body.
    /// Ticks are unique, so the cutoff cannot tie. Each removal drops the
    /// entry's pin with its key.
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
        Body::new(vec![Statement::Empty; n])
    }

    #[test]
    fn memoises_by_body_identity() {
        let mut memo = BodyMemo::new();
        let a = body(1);
        let shared = a.clone();
        assert_eq!(memo.get(&a), None);
        memo.insert(&a, 7u32);
        assert_eq!(memo.get(&shared), Some(7), "a cloned Body shares the entry");
        assert_eq!(
            memo.get(&body(1)),
            None,
            "an equal but distinct Body does not"
        );
    }

    #[test]
    fn insert_replaces_the_value() {
        let mut memo = BodyMemo::new();
        let a = body(1);
        memo.insert(&a, 1u32);
        memo.insert(&a, 2u32);
        assert_eq!(memo.len(), 1);
        assert_eq!(memo.get(&a), Some(2));
    }

    #[test]
    fn stays_within_capacity_and_keeps_recent_entries() {
        let mut memo = BodyMemo::with_capacity(4);
        let hot = body(1);
        memo.insert(&hot, 0u32);
        for i in 0..20u32 {
            assert_eq!(memo.get(&hot), Some(0));
            memo.insert(&body(1), i);
            assert!(memo.len() <= 4);
        }
        assert_eq!(
            memo.get(&hot),
            Some(0),
            "the most recently used entry survives"
        );
    }

    #[test]
    fn evicted_bodies_are_unpinned() {
        let mut memo = BodyMemo::with_capacity(2);
        let first = body(1);
        let weak = Rc::downgrade(&first.statements);
        memo.insert(&first, 0u32);
        drop(first);
        assert!(weak.upgrade().is_some(), "the memo pins its Body");
        for i in 0..4u32 {
            memo.insert(&body(1), i);
        }
        assert!(weak.upgrade().is_none(), "eviction releases the pin");
    }
}
