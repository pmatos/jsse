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
use std::cell::Cell;

/// A remembered property-map slot for one name site. Only a hint: it is
/// re-verified by key on every use, so a stale value (property deleted,
/// object spilled or compacted, a different realm's global object) costs one
/// failed key comparison and is retrained from a fresh probe.
pub(crate) type GlobalHint = Cell<u32>;

#[inline]
fn locate(props: &PropertyMap, name: &str, hint: Option<&GlobalHint>) -> Option<usize> {
    let Some(hint) = hint else {
        return props.slot_of(name);
    };
    let remembered = hint.get() as usize;
    if let Some((key, _)) = props.entry_at(remembered)
        && key.as_bytes() == name.as_bytes()
    {
        return Some(remembered);
    }
    let slot = props.slot_of(name)?;
    hint.set(slot as u32);
    Some(slot)
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
        if desc.get.is_some() || desc.set.is_some() {
            return None;
        }
        Some(desc.value.clone().unwrap_or(JsValue::UNDEFINED))
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
                    if desc.get.is_none() && desc.set.is_none() && desc.writable != Some(false) =>
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
                desc.value = Some(value.clone());
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
