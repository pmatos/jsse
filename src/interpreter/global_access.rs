//! Global Data Fast Path: the single place that decides whether a bare name
//! that resolved to the global object can be read or written as an ordinary,
//! own, data property with one property-map probe and no descriptor clone.
//!
//! The helpers refuse (return `None` / `false`) for every case the generic
//! Object Environment Record path handles observably — a Proxy or exotic
//! global object, a missing or inherited property, an accessor, a
//! non-writable property — and leave the object untouched, so callers fall
//! through to the unchanged generic code. Nothing is cached across calls: each
//! access re-validates the live own property, so there is no invalidation
//! surface.

use super::*;
use crate::types::JsPropertyKey;
use std::cell::Cell;

/// A remembered property-map slot for one name site, with the interned key it
/// belongs to; it must only ever be used with that name. Only a hint: it is
/// re-verified on every use, so a stale slot (property deleted, object spilled
/// or compacted, a different realm's global object) costs one failed key
/// comparison and is retrained from a fresh probe.
#[derive(Debug)]
pub(crate) struct GlobalHint {
    slot: Cell<u32>,
    key: JsPropertyKey,
}

impl GlobalHint {
    pub(crate) fn new(name: &str) -> Self {
        GlobalHint {
            slot: Cell::new(0),
            key: crate::interpreter::key_intern::intern_key(name),
        }
    }
}

#[inline]
fn locate(props: &PropertyMap, name: &str, hint: Option<&GlobalHint>) -> Option<usize> {
    let Some(hint) = hint else {
        return props.slot_of(name);
    };
    debug_assert!(
        hint.key.as_bytes() == name.as_bytes(),
        "a GlobalHint belongs to exactly one name"
    );
    let remembered = hint.slot.get() as usize;
    if let Some((key, _)) = props.entry_at(remembered)
        && (key.same_allocation(&hint.key) || key.as_bytes() == name.as_bytes())
    {
        return Some(remembered);
    }
    let slot = props.slot_of(name)?;
    hint.slot.set(slot as u32);
    Some(slot)
}

/// Outcome of the clone-free scope walk behind the global data fast path.
pub(super) enum NameWalk {
    /// A plain declarative binding that is initialised: its value.
    Value(JsValue),
    /// No scope on the way binds the name and the chain ends at the global
    /// environment, whose binding object is `gid`. Carries the global
    /// environment itself when the caller asked for it.
    Global(u64, Option<EnvRef>),
    /// Only produced when the caller asked for environments: the scope that
    /// binds the name, initialised or not.
    Bound(EnvRef),
    /// A `with` scope, an import binding, a TDZ binding or an over-deep chain:
    /// run the generic resolution from the start.
    Slow,
}

/// Deepest scope chain the recursive walk follows before giving up.
const MAX_WALK_DEPTH: u32 = 64;

/// Walk `env` outward looking for `key`, borrowing each scope in turn instead
/// of cloning the `Rc` per hop. Never runs user code, so holding the borrows
/// across the recursion is safe. Anything the walk does not model exactly
/// (`with`, indirect bindings, uninitialised bindings) is `Slow`, and the
/// caller redoes the lookup through the generic path.
pub(super) fn walk_scope_chain<N: BindingName + ?Sized>(
    env: &EnvRef,
    key: &N,
    want_env: bool,
    depth: u32,
) -> NameWalk {
    let scope = env.borrow();
    if scope.with_object.is_some() || scope.indirect_bindings.is_some() {
        return NameWalk::Slow;
    }
    if let Some(binding) = scope.bindings.get(key) {
        if want_env {
            return NameWalk::Bound(env.clone());
        }
        return if binding.initialized {
            NameWalk::Value(binding.value.clone())
        } else {
            NameWalk::Slow
        };
    }
    if let Some(gid) = scope.global_object_id {
        return NameWalk::Global(gid, want_env.then(|| env.clone()));
    }
    match &scope.parent {
        Some(parent) if depth < MAX_WALK_DEPTH => {
            walk_scope_chain(parent, key, want_env, depth + 1)
        }
        _ => NameWalk::Slow,
    }
}

impl Interpreter {
    /// Whether `obj` is an ordinary object whose own properties are exactly
    /// its property map: the only shape for which one `properties` probe is
    /// the whole of [[GetOwnProperty]] / own-data [[Set]].
    #[inline]
    fn is_plain_property_holder(obj: &JsObjectData) -> bool {
        matches!(obj.kind, ObjectKind::Ordinary)
    }

    /// Own data property `name` of the global object `gid`, if it is an
    /// ordinary own data property. Mirrors what `resolve_identifier` returns
    /// for such a property (OrdinaryGet with a data descriptor).
    #[inline]
    pub(super) fn global_object_own_data(
        &self,
        gid: u64,
        name: &str,
        hint: Option<&GlobalHint>,
    ) -> Option<JsValue> {
        let cell = self.get_object_cell(gid)?;
        let obj = cell.borrow();
        if !Self::is_plain_property_holder(&obj) {
            return None;
        }
        let slot = locate(&obj.properties, name, hint)?;
        let (_, desc) = obj.properties.entry_at(slot)?;
        if desc.getter().is_some() || desc.setter().is_some() {
            return None;
        }
        Some(desc.value().cloned().unwrap_or(JsValue::UNDEFINED))
    }

    /// Store `value` into the existing own, writable data property `name` of
    /// the global object `gid`. Returns `false` — leaving everything
    /// untouched — unless that is exactly what OrdinarySet would do with the
    /// global object as receiver (OrdinarySetWithOwnDescriptor, data
    /// descriptor, writable, receiver is the object itself). Never creates a
    /// property.
    #[inline]
    pub(super) fn global_object_store_own_data(
        &self,
        gid: u64,
        name: &str,
        value: &JsValue,
        hint: Option<&GlobalHint>,
    ) -> bool {
        let Some(cell) = self.get_object_cell(gid) else {
            return false;
        };
        let slot = {
            let obj = cell.borrow();
            if !Self::is_plain_property_holder(&obj) {
                return false;
            }
            let Some(slot) = locate(&obj.properties, name, hint) else {
                return false;
            };
            match obj.properties.entry_at(slot) {
                Some((_, desc))
                    if desc.getter().is_none()
                        && desc.setter().is_none()
                        && desc.writable() != Some(false) =>
                {
                    slot
                }
                _ => return false,
            }
        };
        self.gc_write_barrier_value(cell, value);
        let mut obj = cell.borrow_mut_untracked();
        match obj.properties.entry_at_mut(slot) {
            Some(desc) => {
                desc.set_value(Some(value.clone()));
                true
            }
            None => false,
        }
    }

    /// Whether the global object `gid` has `name` in its own property map.
    /// Same answer as `properties.contains_key(name)`, with the hint's slot
    /// check standing in for the hash.
    #[inline]
    pub(super) fn global_object_has_own_entry(
        &self,
        gid: u64,
        name: &str,
        hint: Option<&GlobalHint>,
    ) -> bool {
        self.get_object_cell(gid)
            .is_some_and(|cell| locate(&cell.borrow().properties, name, hint).is_some())
    }
}
