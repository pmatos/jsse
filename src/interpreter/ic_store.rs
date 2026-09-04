//! Interpreter-side inline-cache store.
//!
//! Inline-cache slots used to live on the AST nodes in `Cell`s. The ADR in
//! `docs/adr/0001-inline-cache-ast-seam.md` moved them into the interpreter,
//! keyed by the identity of the executing `Body`. Each `Body` gets a dense
//! namespace of `CallSiteId` / `PropSiteId` values assigned by
//! `ast::assign_ic_sites`; this module creates a `BodyIcStore` per body on
//! first execution and shares it across all closures of that body.

use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::{Body, BodyIcInfo, CallSiteId, PropSiteId};
use crate::interpreter::Interpreter;
use crate::interpreter::ic::{CallIcSlot, PropIcSlot};

/// Maximum number of inactive per-Body stores retained by default. Active
/// stores can temporarily take the table above this limit when call nesting
/// exceeds it; they are discarded as those Bodies return.
pub(crate) const DEFAULT_CAPACITY: usize = 8192;

/// Stable handle to a per-Body cache returned by `IcStore::enter_body`.
///
/// The generation prevents a handle saved by an outer evaluator frame from
/// silently aliasing a different Body if a lifecycle bug ever reuses its slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BodyStoreHandle {
    slot: usize,
    generation: u64,
}

/// Interpreter-side side table that maps a body identity to its cache.
pub(crate) struct IcStore {
    /// Map from `Body::key` to the store index, so cloned ASTs sharing a Body
    /// share its cache. Each `BodyIcStore` pins the Body for as long as its key
    /// lives, per the ABA contract on `Body::key`.
    ///
    /// Eviction removes this mapping while its entry still pins the Body, then
    /// drops the entry. Every surviving pointer key therefore names a live Body.
    index: HashMap<*const Vec<crate::ast::Statement>, usize>,
    stores: Vec<StoreSlot>,
    free_slots: Vec<usize>,
    capacity: usize,
    clock: u64,
}

struct StoreSlot {
    generation: u64,
    entry: Option<StoreEntry>,
}

struct StoreEntry {
    store: BodyIcStore,
    last_used: u64,
    /// Number of evaluator frames that have installed this entry. A saved
    /// parent handle remains counted while a nested Body is current.
    in_flight: usize,
}

impl IcStore {
    pub(crate) fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    fn with_capacity(capacity: usize) -> Self {
        Self {
            index: HashMap::new(),
            stores: Vec::new(),
            free_slots: Vec::new(),
            capacity: capacity.max(2),
            clock: 0,
        }
    }

    /// Mark a Body's cache as in flight and return its stable handle, creating
    /// an empty cache on a miss. The caller must pair this with `exit_body`.
    /// The Body must already have had its IC sites assigned.
    pub(crate) fn enter_body(&mut self, body: &Body) -> BodyStoreHandle {
        let key = body.key();
        if let Some(&idx) = self.index.get(&key) {
            let tick = self.next_tick();
            let slot = &mut self.stores[idx];
            let entry = slot
                .entry
                .as_mut()
                .expect("IcStore index pointed to a free slot");
            entry.last_used = tick;
            entry.in_flight = entry
                .in_flight
                .checked_add(1)
                .expect("IcStore in-flight count overflowed");
            return BodyStoreHandle {
                slot: idx,
                generation: slot.generation,
            };
        }

        if self.index.len() >= self.capacity {
            self.evict_inactive_older_half();
        }

        let tick = self.next_tick();
        let entry = StoreEntry {
            store: BodyIcStore::new(body.ic, body.statements.clone()),
            last_used: tick,
            in_flight: 1,
        };
        let (slot_idx, generation) = match self.free_slots.pop() {
            Some(idx) => {
                let slot = &mut self.stores[idx];
                debug_assert!(slot.entry.is_none());
                slot.generation = slot
                    .generation
                    .checked_add(1)
                    .expect("IcStore slot generation overflowed");
                slot.entry = Some(entry);
                (idx, slot.generation)
            }
            None => {
                let idx = self.stores.len();
                self.stores.push(StoreSlot {
                    generation: 0,
                    entry: Some(entry),
                });
                (idx, 0)
            }
        };
        assert!(self.index.insert(key, slot_idx).is_none());
        BodyStoreHandle {
            slot: slot_idx,
            generation,
        }
    }

    /// Release one in-flight use. If all capacity slots were active during a
    /// miss, the table temporarily grew; discard the returning overflow entry
    /// as soon as it becomes inactive.
    pub(crate) fn exit_body(&mut self, handle: BodyStoreHandle) {
        let became_inactive = {
            let entry = self.entry_mut(handle);
            assert!(
                entry.in_flight > 0,
                "BodyStoreHandle exited more times than it was entered"
            );
            entry.in_flight -= 1;
            entry.in_flight == 0
        };
        if became_inactive && self.index.len() > self.capacity {
            self.evict_slot(handle.slot);
        }
    }

    /// Return a mutable reference to an active cache for a validated handle.
    pub(crate) fn store_mut(&mut self, handle: BodyStoreHandle) -> &mut BodyIcStore {
        let entry = self.entry_mut(handle);
        assert!(
            entry.in_flight > 0,
            "BodyStoreHandle used while its Body is not in flight"
        );
        &mut entry.store
    }

    fn entry_mut(&mut self, handle: BodyStoreHandle) -> &mut StoreEntry {
        let slot = self
            .stores
            .get_mut(handle.slot)
            .unwrap_or_else(|| panic!("stale BodyStoreHandle: missing slot {}", handle.slot));
        assert_eq!(
            slot.generation, handle.generation,
            "stale BodyStoreHandle: slot generation changed"
        );
        slot.entry
            .as_mut()
            .expect("stale BodyStoreHandle: slot was evicted")
    }

    fn next_tick(&mut self) -> u64 {
        self.clock = self
            .clock
            .checked_add(1)
            .expect("IcStore recency clock overflowed");
        self.clock
    }

    /// Evict enough old inactive entries to retain roughly the newer half.
    /// Entries in flight are never candidates, even when that means allowing a
    /// temporary capacity overflow for a deeply nested set of distinct Bodies.
    fn evict_inactive_older_half(&mut self) {
        let mut candidates: Vec<(usize, u64)> = self
            .stores
            .iter()
            .enumerate()
            .filter_map(|(idx, slot)| {
                slot.entry
                    .as_ref()
                    .filter(|entry| entry.in_flight == 0)
                    .map(|entry| (idx, entry.last_used))
            })
            .collect();
        if candidates.is_empty() {
            return;
        }
        candidates.sort_unstable_by_key(|&(_, last_used)| last_used);
        let target_len = self.capacity / 2;
        let remove_count = self
            .index
            .len()
            .saturating_sub(target_len)
            .max(1)
            .min(candidates.len());
        for (idx, _) in candidates.into_iter().take(remove_count) {
            self.evict_slot(idx);
        }
    }

    /// Remove the map key while the entry still owns its Body pin, then release
    /// the entry and make its stable slot available for a new generation.
    fn evict_slot(&mut self, slot_idx: usize) {
        let entry = self.stores[slot_idx]
            .entry
            .take()
            .expect("attempted to evict a free IcStore slot");
        assert_eq!(entry.in_flight, 0, "attempted to evict an active Body");
        let key = entry.store.body_key();
        assert_eq!(
            self.index.remove(&key),
            Some(slot_idx),
            "IcStore entry and pointer index diverged"
        );
        drop(entry);
        self.free_slots.push(slot_idx);
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.index.len()
    }

    #[cfg(test)]
    pub(crate) fn slot_count(&self) -> usize {
        self.stores.len()
    }
}

/// Per-body cache for call and property IC slots. Sized once from the
/// `BodyIcInfo` produced by `ast::assign_ic_sites`.
pub(crate) struct BodyIcStore {
    call_slots: Vec<CallIcSlot>,
    prop_slots: Vec<PropIcSlot>,
    /// Pins the Body so its `Body::key` address cannot be reused by an unrelated
    /// Body after this one is dropped, which would alias a stale, wrongly-sized
    /// store.
    _body: Rc<Vec<crate::ast::Statement>>,
}

impl BodyIcStore {
    fn new(info: BodyIcInfo, body: Rc<Vec<crate::ast::Statement>>) -> Self {
        Self {
            call_slots: vec![CallIcSlot::Empty; info.call_site_count as usize],
            prop_slots: vec![PropIcSlot::Empty; info.prop_site_count as usize],
            _body: body,
        }
    }

    /// Return a mutable reference to the call slot for a site id.
    #[inline]
    pub(crate) fn call_slot(&mut self, id: CallSiteId) -> &mut CallIcSlot {
        &mut self.call_slots[id.0 as usize]
    }

    /// Return a mutable reference to the property slot for a site id.
    #[inline]
    pub(crate) fn prop_slot(&mut self, id: PropSiteId) -> &mut PropIcSlot {
        &mut self.prop_slots[id.0 as usize]
    }

    fn body_key(&self) -> *const Vec<crate::ast::Statement> {
        Rc::as_ptr(&self._body)
    }
}

impl Interpreter {
    /// Install `body`'s IC store and return the still-active parent handle.
    pub(crate) fn enter_ic_body(&mut self, body: &Body) -> Option<BodyStoreHandle> {
        let handle = self.ic_store.enter_body(body);
        self.current_ic_handle.replace(handle)
    }

    /// Release the current Body and restore its still-active parent handle.
    pub(crate) fn leave_ic_body(&mut self, previous: Option<BodyStoreHandle>) {
        let handle = self
            .current_ic_handle
            .take()
            .expect("left an IC Body without entering one");
        self.ic_store.exit_body(handle);
        self.current_ic_handle = previous;
    }

    /// Return a mutable reference to the current body's call slot for `id`.
    #[inline]
    pub(crate) fn call_slot(&mut self, id: CallSiteId) -> &mut CallIcSlot {
        let handle = self
            .current_ic_handle
            .expect("call IC site evaluated outside a Body");
        self.ic_store.store_mut(handle).call_slot(id)
    }

    /// Return a mutable reference to the current body's property slot for `id`.
    #[inline]
    pub(crate) fn prop_slot(&mut self, id: PropSiteId) -> &mut PropIcSlot {
        let handle = self
            .current_ic_handle
            .expect("property IC site evaluated outside a Body");
        self.ic_store.store_mut(handle).prop_slot(id)
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;
    use crate::ast::{self, Body, BodyIcInfo, Statement};

    fn body_with_calls_props(calls: u32, props: u32) -> Body {
        Body {
            statements: Rc::new(vec![]),
            ic: BodyIcInfo {
                call_site_count: calls,
                prop_site_count: props,
                assigned: true,
            },
        }
    }

    #[test]
    fn enter_body_creates_store_and_returns_handle() {
        let mut store = IcStore::new();
        let body = body_with_calls_props(2, 1);
        let h = store.enter_body(&body);
        assert_eq!(h.slot, 0);
        assert_eq!(
            store.store_mut(h).call_slot(CallSiteId(0)) as *mut _,
            store.store_mut(h).call_slot(CallSiteId(0)) as *mut _
        );
        *store.store_mut(h).call_slot(CallSiteId(0)) = CallIcSlot::Megamorphic;
        assert!(matches!(
            *store.store_mut(h).call_slot(CallSiteId(0)),
            CallIcSlot::Megamorphic
        ));
        store.exit_body(h);
    }

    #[test]
    fn cloned_body_shares_store_handle() {
        let mut store = IcStore::new();
        let body = body_with_calls_props(1, 0);
        let clone = body.clone();
        let h1 = store.enter_body(&body);
        let h2 = store.enter_body(&clone);
        assert_eq!(h1, h2, "cloned body must share the same IC store");
        store.exit_body(h2);
        store.exit_body(h1);
    }

    #[test]
    fn distinct_bodies_get_distinct_handles() {
        let mut store = IcStore::new();
        let a = body_with_calls_props(1, 0);
        let b = body_with_calls_props(1, 0);
        let ha = store.enter_body(&a);
        let hb = store.enter_body(&b);
        assert_ne!(ha, hb);
        store.exit_body(hb);
        store.exit_body(ha);
    }

    #[test]
    fn interpreter_call_slot_uses_current_handle() {
        let mut interp = Interpreter::new();
        let body = body_with_calls_props(1, 1);
        let prev = interp.enter_ic_body(&body);
        let handle = interp.current_ic_handle.unwrap();
        *interp.call_slot(CallSiteId(0)) = CallIcSlot::Megamorphic;
        *interp.prop_slot(PropSiteId(0)) = PropIcSlot::Megamorphic;
        assert!(matches!(
            *interp.ic_store.store_mut(handle).call_slot(CallSiteId(0)),
            CallIcSlot::Megamorphic
        ));
        assert!(matches!(
            &*interp.ic_store.store_mut(handle).prop_slot(PropSiteId(0)),
            PropIcSlot::Megamorphic
        ));
        interp.leave_ic_body(prev);
    }

    #[test]
    fn assign_ic_sites_sized_store() {
        let mut body = Body::new(vec![Statement::Expression(crate::ast::Expression::Call(
            Box::new(crate::ast::Expression::Identifier("f".to_string())),
            vec![],
            CallSiteId::UNASSIGNED,
        ))]);
        ast::assign_ic_sites(&mut body);
        let mut store = IcStore::new();
        let h = store.enter_body(&body);
        let slots = &store.store_mut(h).call_slots;
        assert_eq!(slots.len(), 1);
        assert!(matches!(slots[0], CallIcSlot::Empty));
        store.exit_body(h);
    }

    #[test]
    fn releases_old_body_pins_after_many_distinct_bodies() {
        const CAPACITY: usize = 8192;

        let mut store = IcStore::new();
        let first = body_with_calls_props(0, 0);
        let weak = Rc::downgrade(&first.statements);
        let handle = store.enter_body(&first);
        store.exit_body(handle);
        drop(first);

        for _ in 0..CAPACITY {
            let body = body_with_calls_props(0, 0);
            let handle = store.enter_body(&body);
            store.exit_body(handle);
        }

        assert!(
            weak.upgrade().is_none(),
            "an old Body must not remain pinned after the cache reaches capacity"
        );
        assert!(
            store.len() <= CAPACITY,
            "IC store retained {} distinct Bodies",
            store.len()
        );
        assert!(
            store.slot_count() <= CAPACITY,
            "IC store allocated {} backing slots",
            store.slot_count()
        );
    }

    #[test]
    fn recently_used_inactive_store_survives_a_sweep() {
        let mut store = IcStore::with_capacity(4);
        let hot = body_with_calls_props(1, 0);
        let hot_handle = store.enter_body(&hot);
        store.exit_body(hot_handle);
        for _ in 0..3 {
            let cold = body_with_calls_props(1, 0);
            let handle = store.enter_body(&cold);
            store.exit_body(handle);
        }

        let touched_handle = store.enter_body(&hot);
        store.exit_body(touched_handle);
        let incoming = body_with_calls_props(1, 0);
        let incoming_handle = store.enter_body(&incoming);
        store.exit_body(incoming_handle);

        let reused_handle = store.enter_body(&hot);
        assert_eq!(
            reused_handle, hot_handle,
            "the most recently used inactive store should stay cached"
        );
        store.exit_body(reused_handle);
    }

    #[test]
    fn reused_slot_changes_generation_and_rejects_a_stale_handle() {
        let mut store = IcStore::with_capacity(2);
        let first = body_with_calls_props(1, 0);
        let stale = store.enter_body(&first);
        store.exit_body(stale);
        let second = body_with_calls_props(1, 0);
        let second_handle = store.enter_body(&second);
        store.exit_body(second_handle);

        let third = body_with_calls_props(1, 0);
        let reused = store.enter_body(&third);
        assert_eq!(reused.slot, stale.slot, "the evicted slot should be reused");
        assert_ne!(
            reused.generation, stale.generation,
            "slot reuse must advance its generation"
        );
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store.store_mut(stale);
        }));
        assert!(panic.is_err(), "a stale handle must fail validation");
        store.exit_body(reused);
    }

    #[test]
    fn active_entries_are_not_evicted_and_overflow_is_reclaimed_on_exit() {
        let mut store = IcStore::with_capacity(2);
        let a = body_with_calls_props(1, 0);
        let b = body_with_calls_props(1, 0);
        let c = body_with_calls_props(1, 0);
        let ha = store.enter_body(&a);
        let hb = store.enter_body(&b);
        let hc = store.enter_body(&c);

        assert_eq!(store.len(), 3, "all capacity entries were active");
        assert_eq!(store.slot_count(), 3);
        *store.store_mut(ha).call_slot(CallSiteId(0)) = CallIcSlot::Megamorphic;
        assert!(matches!(
            *store.store_mut(ha).call_slot(CallSiteId(0)),
            CallIcSlot::Megamorphic
        ));

        store.exit_body(hc);
        assert_eq!(store.len(), 2, "returning overflow entry was not reclaimed");
        store.exit_body(hb);
        store.exit_body(ha);
    }

    #[test]
    fn recursive_entry_keeps_the_shared_store_active_until_the_outer_exit() {
        let mut store = IcStore::with_capacity(2);
        let body = body_with_calls_props(1, 0);
        let outer = store.enter_body(&body);
        let inner = store.enter_body(&body);
        assert_eq!(inner, outer);
        store.exit_body(inner);
        *store.store_mut(outer).call_slot(CallSiteId(0)) = CallIcSlot::Megamorphic;
        store.exit_body(outer);
    }
}
