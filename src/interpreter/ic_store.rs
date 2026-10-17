//! Interpreter-side inline-cache store.
//!
//! Inline-cache slots used to live on the AST nodes in `Cell`s. The ADR in
//! `docs/adr/0001-inline-cache-ast-seam.md` moved them into the interpreter.
//! Each `Body` gets a dense namespace of `CallSiteId` / `PropSiteId` values
//! assigned by `ast::assign_ic_sites`, and a `BodyIcStore` holds the slots.
//! A function body's store belongs to its literal's `LiteralFrame` and is
//! shared by every closure of the literal; script, module and eval bodies keep
//! theirs in the `IcStore` table, keyed by the identity of the executing
//! `Body`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rustc_hash::FxHashMap;

use crate::ast::{Body, CallSiteId, PropSiteId};
use crate::interpreter::Interpreter;
use crate::interpreter::ic::{CallIcSlot, PropIcSlot};
use crate::interpreter::literal_frame::LiteralFrame;

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
    ///
    /// Keys are `Rc` addresses, never script-controlled strings, so the
    /// HashDoS resistance of SipHash buys nothing here and `enter_body` runs on
    /// every call.
    index: FxHashMap<*const Vec<crate::ast::Statement>, usize>,
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

fn tick_clock(clock: &mut u64) -> u64 {
    *clock = clock
        .checked_add(1)
        .expect("IcStore recency clock overflowed");
    *clock
}

impl StoreEntry {
    fn mark_entered(&mut self, tick: u64) {
        self.last_used = tick;
        self.in_flight = self
            .in_flight
            .checked_add(1)
            .expect("IcStore in-flight count overflowed");
    }
}

impl IcStore {
    pub(crate) fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    fn with_capacity(capacity: usize) -> Self {
        Self {
            index: FxHashMap::default(),
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
            let tick = tick_clock(&mut self.clock);
            let slot = &mut self.stores[idx];
            slot.entry
                .as_mut()
                .expect("IcStore index pointed to a free slot")
                .mark_entered(tick);
            return BodyStoreHandle {
                slot: idx,
                generation: slot.generation,
            };
        }

        if self.index.len() >= self.capacity {
            self.evict_inactive_older_half();
        }

        let tick = tick_clock(&mut self.clock);
        let entry = StoreEntry {
            store: BodyIcStore::new(body),
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
#[derive(Debug)]
pub(crate) struct BodyIcStore {
    call_slots: Vec<Cell<CallIcSlot>>,
    prop_slots: RefCell<Vec<PropIcSlot>>,
    /// Pins the Body so its `Body::key` address cannot be reused by an unrelated
    /// Body after this one is dropped, which would alias a stale, wrongly-sized
    /// store.
    _body: Rc<Vec<crate::ast::Statement>>,
}

/// A function literal's IC store, owned by its `LiteralFrame` and shared by
/// every closure of the literal: entering the body is one `Rc` clone and
/// needs no table probe, generation check or in-flight accounting, because
/// the store lives as long as the frame that hands it out.
pub(crate) type FrameIcStore = Rc<BodyIcStore>;

impl BodyIcStore {
    fn new(body: &Body) -> Self {
        Self {
            call_slots: vec![Cell::new(CallIcSlot::Empty); body.ic.call_site_count as usize],
            prop_slots: RefCell::new(vec![PropIcSlot::Empty; body.ic.prop_site_count as usize]),
            _body: body.statements.clone(),
        }
    }

    /// A frame is paired with one literal's Body for its whole life; the
    /// frame's entry points assert it in debug builds.
    pub(crate) fn for_frame(body: &Body) -> FrameIcStore {
        Rc::new(Self::new(body))
    }

    #[inline]
    fn call_slot(&self, id: CallSiteId) -> CallIcSlot {
        self.call_slots[id.0 as usize].get()
    }

    #[inline]
    fn set_call_slot(&self, id: CallSiteId, slot: CallIcSlot) {
        self.call_slots[id.0 as usize].set(slot);
    }

    #[inline]
    fn with_prop_slot<R>(&self, id: PropSiteId, access: impl FnOnce(&mut PropIcSlot) -> R) -> R {
        access(&mut self.prop_slots.borrow_mut()[id.0 as usize])
    }

    fn body_key(&self) -> *const Vec<crate::ast::Statement> {
        Rc::as_ptr(&self._body)
    }
}

/// The IC store of the body being executed.
#[derive(Default)]
pub(crate) enum CurrentIc {
    #[default]
    None,
    /// A script, module or eval body, held in the interpreter's table.
    Table(BodyStoreHandle),
    /// A function body, held by its literal's frame.
    Frame(FrameIcStore),
}

impl Interpreter {
    /// Run `execute` with `body`'s IC store installed, then restore the
    /// previously active Body store.
    ///
    /// The Body must already have had its IC sites assigned. JavaScript abrupt
    /// completions are ordinary return values, so they restore the parent just
    /// like successful execution. A Rust panic leaves the interpreter unusable,
    /// matching the other internal Interpreter scope combinators.
    #[inline]
    pub(crate) fn with_ic_body<T>(
        &mut self,
        body: &Body,
        execute: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let active = self.ic_store.enter_body(body);
        let previous = std::mem::replace(&mut self.current_ic, CurrentIc::Table(active));
        let result = execute(self);
        let current = std::mem::replace(&mut self.current_ic, previous);
        assert!(
            matches!(current, CurrentIc::Table(handle) if handle == active),
            "IC Body scope exited with a different active Body"
        );
        self.ic_store.exit_body(active);
        result
    }

    /// [`with_ic_body`](Self::with_ic_body) for a function body, whose store
    /// the literal's frame owns (#1023).
    #[inline]
    pub(crate) fn with_ic_body_in_frame<T>(
        &mut self,
        body: &Body,
        frame: &LiteralFrame,
        execute: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let store = frame.ic_store(body).clone();
        let previous = std::mem::replace(&mut self.current_ic, CurrentIc::Frame(store));
        let result = execute(self);
        let current = std::mem::replace(&mut self.current_ic, previous);
        debug_assert!(
            matches!(&current, CurrentIc::Frame(store) if store.body_key() == body.key()),
            "IC Body scope exited with a different active Body"
        );
        result
    }

    #[inline]
    fn current_store(&mut self) -> &BodyIcStore {
        match &self.current_ic {
            CurrentIc::Frame(store) => store,
            CurrentIc::Table(handle) => self.ic_store.store_mut(*handle),
            CurrentIc::None => panic!("IC site evaluated outside a Body"),
        }
    }

    /// The current body's call slot for `id`.
    #[inline]
    pub(crate) fn call_slot(&mut self, id: CallSiteId) -> CallIcSlot {
        self.current_store().call_slot(id)
    }

    #[inline]
    pub(crate) fn set_call_slot(&mut self, id: CallSiteId, slot: CallIcSlot) {
        self.current_store().set_call_slot(id, slot);
    }

    /// Runs `access` on the current body's property slot for `id`. `access`
    /// must not run script or touch another IC slot: a frame store holds its
    /// property slots behind a `RefCell` for the duration.
    #[inline]
    pub(crate) fn with_prop_slot<R>(
        &mut self,
        id: PropSiteId,
        access: impl FnOnce(&mut PropIcSlot) -> R,
    ) -> R {
        self.current_store().with_prop_slot(id, access)
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;
    use crate::ast::{self, Body, BodyIcInfo, ExprBox, Statement};

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
        assert!(matches!(
            store.store_mut(h).call_slot(CallSiteId(0)),
            CallIcSlot::Empty
        ));
        store
            .store_mut(h)
            .set_call_slot(CallSiteId(0), CallIcSlot::Megamorphic);
        assert!(matches!(
            store.store_mut(h).call_slot(CallSiteId(0)),
            CallIcSlot::Megamorphic
        ));
        store.exit_body(h);
    }

    #[test]
    fn frame_store_is_shared_by_nested_entries_and_restores_the_outer_store() {
        let mut interp = Interpreter::new();
        let body = body_with_calls_props(2, 0);
        let frame = LiteralFrame::default();
        interp.with_ic_body_in_frame(&body, &frame, |interp| {
            interp.set_call_slot(CallSiteId(1), CallIcSlot::Megamorphic);
            interp.with_ic_body_in_frame(&body, &frame, |interp| {
                assert!(matches!(
                    interp.call_slot(CallSiteId(1)),
                    CallIcSlot::Megamorphic
                ));
                assert!(matches!(interp.call_slot(CallSiteId(0)), CallIcSlot::Empty));
            });
            assert!(matches!(
                interp.call_slot(CallSiteId(1)),
                CallIcSlot::Megamorphic
            ));
        });
        assert!(matches!(interp.current_ic, CurrentIc::None));
        assert_eq!(
            interp.ic_store.len(),
            0,
            "a frame store never enters the table"
        );
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
    fn with_ic_body_restores_the_active_parent_store() {
        let mut interp = Interpreter::new();
        let parent = body_with_calls_props(1, 0);
        let child = body_with_calls_props(1, 0);

        interp.with_ic_body(&parent, |interp| {
            interp.set_call_slot(CallSiteId(0), CallIcSlot::Megamorphic);

            interp.with_ic_body(&child, |interp| {
                assert!(matches!(interp.call_slot(CallSiteId(0)), CallIcSlot::Empty));
            });

            assert!(matches!(
                interp.call_slot(CallSiteId(0)),
                CallIcSlot::Megamorphic
            ));
        });

        interp.with_ic_body(&child, |interp| {
            assert!(matches!(interp.call_slot(CallSiteId(0)), CallIcSlot::Empty));
        });
    }

    #[test]
    fn assign_ic_sites_sized_store() {
        let mut body = Body::new(vec![Statement::Expression(crate::ast::Expression::Call(
            ExprBox::new(crate::ast::Expression::Identifier("f".to_string())),
            vec![],
            CallSiteId::UNASSIGNED,
        ))]);
        ast::assign_ic_sites(&mut body);
        let mut store = IcStore::new();
        let h = store.enter_body(&body);
        let slots = &store.store_mut(h).call_slots;
        assert_eq!(slots.len(), 1);
        assert!(matches!(slots[0].get(), CallIcSlot::Empty));
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
        store
            .store_mut(ha)
            .set_call_slot(CallSiteId(0), CallIcSlot::Megamorphic);
        assert!(matches!(
            store.store_mut(ha).call_slot(CallSiteId(0)),
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
        store
            .store_mut(outer)
            .set_call_slot(CallSiteId(0), CallIcSlot::Megamorphic);
        store.exit_body(outer);
    }
}
