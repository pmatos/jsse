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
    scope: Cell<ScopeHint>,
}

/// Where the last full scope walk for this name site found it: `hops` scopes
/// up from the site's environment, at binding slot `slot` of that scope's
/// binding vector, or (`GLOBAL_OBJECT_SLOT`) in no scope at all because the
/// walk reached the global environment. The walk is re-verified on every use,
/// so the shape of the chain can change freely; it only costs a full walk.
#[derive(Clone, Copy, Debug)]
struct ScopeHint {
    hops: u32,
    slot: u32,
}

const NO_SCOPE_HINT: ScopeHint = ScopeHint {
    hops: u32::MAX,
    slot: 0,
};

const GLOBAL_OBJECT_SLOT: u32 = u32::MAX;

impl GlobalHint {
    pub(crate) fn new(name: &str) -> Self {
        GlobalHint {
            slot: Cell::new(0),
            key: crate::interpreter::key_intern::intern_key(name),
            scope: Cell::new(NO_SCOPE_HINT),
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

/// [`walk_scope_chain`], following the site's remembered scope when the key is
/// an interned name and the site has a hint.
#[inline]
pub(super) fn walk_scope<N: BindingName + ?Sized>(
    env: &EnvRef,
    key: &N,
    want_env: bool,
    hint: Option<&GlobalHint>,
) -> NameWalk {
    match (key.as_atom(), hint) {
        (Some(atom), Some(hint)) => walk_scope_chain_hinted(env, atom, want_env, hint),
        _ => walk_scope_chain(env, key, want_env, 0),
    }
}

/// [`walk_scope_chain`] for an interned name site with a [`GlobalHint`].
///
/// The remembered `(hops, slot)` is checked against the live chain: every
/// scope before the remembered one must be a plain declarative scope that
/// does not bind the name (one filter probe each), and the remembered scope
/// must hold the name at the remembered slot. That is exactly the scope the
/// full walk would stop at, so a hit answers identically; anything else runs
/// the full walk, which retrains the hint.
pub(super) fn walk_scope_chain_hinted(
    env: &EnvRef,
    key: &Atom,
    want_env: bool,
    hint: &GlobalHint,
) -> NameWalk {
    let remembered = hint.scope.get();
    if remembered.hops != NO_SCOPE_HINT.hops
        && let Some(walk) = verify_scope_hint(env, key, remembered.hops, remembered.slot, want_env)
    {
        return walk;
    }
    train_scope_hint(env, key, want_env, hint, 0)
}

fn verify_scope_hint(
    env: &EnvRef,
    key: &Atom,
    hops: u32,
    slot: u32,
    want_env: bool,
) -> Option<NameWalk> {
    let scope = env.borrow();
    if scope.with_object.is_some() || scope.indirect_bindings.is_some() {
        return None;
    }
    if hops == 0 {
        if slot == GLOBAL_OBJECT_SLOT {
            let gid = scope.global_object_id?;
            if scope.bindings.contains_key(key) {
                return None;
            }
            return Some(NameWalk::Global(gid, want_env.then(|| env.clone())));
        }
        let (atom, binding) = scope.bindings.entry_at(slot as usize)?;
        if !atom.ptr_eq(key) {
            return None;
        }
        return Some(if want_env {
            NameWalk::Bound(env.clone())
        } else if binding.initialized {
            NameWalk::Value(binding.value.clone())
        } else {
            NameWalk::Slow
        });
    }
    if scope.global_object_id.is_some() || scope.bindings.contains_key(key) {
        return None;
    }
    verify_scope_hint(scope.parent.as_ref()?, key, hops - 1, slot, want_env)
}

/// The full walk of [`walk_scope_chain`], remembering where it ended.
fn train_scope_hint(
    env: &EnvRef,
    key: &Atom,
    want_env: bool,
    hint: &GlobalHint,
    depth: u32,
) -> NameWalk {
    let scope = env.borrow();
    if scope.with_object.is_some() || scope.indirect_bindings.is_some() {
        return NameWalk::Slow;
    }
    if let Some(slot) = scope.bindings.position(key) {
        hint.scope.set(ScopeHint {
            hops: depth,
            slot: slot as u32,
        });
        if want_env {
            return NameWalk::Bound(env.clone());
        }
        let binding = &scope
            .bindings
            .entry_at(slot)
            .expect("position is a live slot")
            .1;
        return if binding.initialized {
            NameWalk::Value(binding.value.clone())
        } else {
            NameWalk::Slow
        };
    }
    if let Some(gid) = scope.global_object_id {
        hint.scope.set(ScopeHint {
            hops: depth,
            slot: GLOBAL_OBJECT_SLOT,
        });
        return NameWalk::Global(gid, want_env.then(|| env.clone()));
    }
    match &scope.parent {
        Some(parent) if depth < MAX_WALK_DEPTH => {
            train_scope_hint(parent, key, want_env, hint, depth + 1)
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
