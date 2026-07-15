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

/// Handle to a per-body cache returned by `IcStore::for_body`. It is a plain
/// index so it can be passed down through the evaluator without borrowing the
/// interpreter.
#[derive(Clone, Copy, Debug)]
pub struct BodyStoreHandle(pub usize);

/// Interpreter-side side table that maps a body identity to its cache.
pub struct IcStore {
    /// Map from the body statement-vector pointer to the store index. The key
    /// is the `Rc` pointer so that cloned ASTs sharing the same body share the
    /// same cache. Each `BodyIcStore` pins a clone of the body's statement `Rc`
    /// so the pointer key cannot be reused by a freed-then-reallocated body
    /// (ABA), matching the `HoistAnalysis` cache pattern.
    index: HashMap<*const Vec<crate::ast::Statement>, usize>,
    stores: Vec<BodyIcStore>,
}

impl IcStore {
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
            stores: Vec::new(),
        }
    }

    /// Return the handle for a body's cache, creating it on first request.
    /// The body must already have had its IC sites assigned (e.g. by the
    /// parser or by `ast::assign_ic_sites` for dynamic code).
    pub fn for_body(&mut self, body: &Body) -> BodyStoreHandle {
        let key = Rc::as_ptr(&body.statements);
        if let Some(&idx) = self.index.get(&key) {
            return BodyStoreHandle(idx);
        }
        let idx = self.stores.len();
        self.stores
            .push(BodyIcStore::new(body.ic, body.statements.clone()));
        self.index.insert(key, idx);
        BodyStoreHandle(idx)
    }

    /// Return a mutable reference to the cache for a handle.
    pub fn store_mut(&mut self, handle: BodyStoreHandle) -> &mut BodyIcStore {
        &mut self.stores[handle.0]
    }
}

/// Per-body cache for call and property IC slots. Sized once from the
/// `BodyIcInfo` produced by `ast::assign_ic_sites`.
pub struct BodyIcStore {
    call_slots: Vec<CallIcSlot>,
    prop_slots: Vec<PropIcSlot>,
    /// Pins the body's statement `Rc` alive so its `Rc::as_ptr` address (used as
    /// the `IcStore` key) cannot be reused by an unrelated body after this one
    /// is dropped, which would otherwise alias a stale, wrongly-sized store.
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
    pub fn call_slot(&mut self, id: CallSiteId) -> &mut CallIcSlot {
        &mut self.call_slots[id.0 as usize]
    }

    /// Return a mutable reference to the property slot for a site id.
    #[inline]
    pub fn prop_slot(&mut self, id: PropSiteId) -> &mut PropIcSlot {
        &mut self.prop_slots[id.0 as usize]
    }
}

impl Interpreter {
    /// Return a mutable reference to the current body's call slot for `id`.
    #[inline]
    pub(crate) fn call_slot(&mut self, id: CallSiteId) -> &mut CallIcSlot {
        self.ic_store
            .store_mut(self.current_ic_handle)
            .call_slot(id)
    }

    /// Return a mutable reference to the current body's property slot for `id`.
    #[inline]
    pub(crate) fn prop_slot(&mut self, id: PropSiteId) -> &mut PropIcSlot {
        self.ic_store
            .store_mut(self.current_ic_handle)
            .prop_slot(id)
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
    fn for_body_creates_store_and_returns_handle() {
        let mut store = IcStore::new();
        let body = body_with_calls_props(2, 1);
        let h = store.for_body(&body);
        assert_eq!(h.0, 0);
        assert_eq!(
            store.store_mut(h).call_slot(CallSiteId(0)) as *mut _,
            store.store_mut(h).call_slot(CallSiteId(0)) as *mut _
        );
        *store.store_mut(h).call_slot(CallSiteId(0)) = CallIcSlot::Megamorphic;
        assert!(matches!(
            *store.store_mut(h).call_slot(CallSiteId(0)),
            CallIcSlot::Megamorphic
        ));
    }

    #[test]
    fn cloned_body_shares_store_handle() {
        let mut store = IcStore::new();
        let body = body_with_calls_props(1, 0);
        let clone = body.clone();
        let h1 = store.for_body(&body);
        let h2 = store.for_body(&clone);
        assert_eq!(h1.0, h2.0, "cloned body must share the same IC store");
    }

    #[test]
    fn distinct_bodies_get_distinct_handles() {
        let mut store = IcStore::new();
        let a = body_with_calls_props(1, 0);
        let b = body_with_calls_props(1, 0);
        let ha = store.for_body(&a);
        let hb = store.for_body(&b);
        assert_ne!(ha.0, hb.0);
    }

    #[test]
    fn interpreter_call_slot_uses_current_handle() {
        let mut interp = Interpreter::new();
        let body = body_with_calls_props(1, 1);
        let handle = interp.ic_store.for_body(&body);
        interp.current_ic_handle = handle;
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
        let h = store.for_body(&body);
        let slots = &store.store_mut(h).call_slots;
        assert_eq!(slots.len(), 1);
        assert!(matches!(slots[0], CallIcSlot::Empty));
    }
}
