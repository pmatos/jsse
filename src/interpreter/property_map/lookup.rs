//! Reading and updating the property at a key or a slot.

use super::*;

impl PropertyMap {
    /// The entry at `slot`, if any. A remembered slot must be checked against
    /// the returned key before use.
    #[inline]
    pub(crate) fn entry_at(&self, slot: usize) -> Option<(&JsPropertyKey, RecordRef<'_>)> {
        match &self.inner {
            PropertyMapInner::Inline(v) => v.get(slot).map(|(k, r)| (k, RecordRef::Stored(r))),
            PropertyMapInner::Spilled { entries, .. } => entries
                .get(slot)?
                .as_ref()
                .map(|(k, r)| (k, RecordRef::Stored(r))),
            PropertyMapInner::Shaped { shape, values } => Some((
                shape.keys().get(slot)?,
                RecordRef::Plain {
                    value: values.get(slot)?,
                    attrs: shape.attrs()[slot],
                },
            )),
        }
    }

    #[inline]
    pub(crate) fn entry_at_mut(&mut self, slot: usize) -> Option<RecordMut<'_>> {
        match &mut self.inner {
            PropertyMapInner::Inline(v) => v.get_mut(slot).map(|(_, r)| RecordMut::Stored(r)),
            PropertyMapInner::Spilled { entries, .. } => entries
                .get_mut(slot)?
                .as_mut()
                .map(|(_, r)| RecordMut::Stored(r)),
            PropertyMapInner::Shaped { shape, values } => Some(RecordMut::Plain {
                value: values.get_mut(slot)?,
                attrs: shape.attrs()[slot],
            }),
        }
    }

    /// [`Self::slot_of`] for a key that stays the same at one access site,
    /// trying the slot `hint` remembers first. The hint is only a guess:
    /// it is accepted only if that slot holds this very key allocation, and
    /// it is retrained on a hit elsewhere, so it can never go stale.
    #[inline]
    pub(crate) fn slot_hinted(
        &self,
        key: &JsPropertyKey,
        hint: Option<&Cell<u32>>,
    ) -> Option<usize> {
        let Some(hint) = hint else {
            return self.slot_of(key);
        };
        let remembered = hint.get() as usize;
        if let Some((k, _)) = self.entry_at(remembered)
            && k.same_allocation(key)
        {
            return Some(remembered);
        }
        let slot = self.slot_of(key)?;
        if self
            .entry_at(slot)
            .is_some_and(|(stored, _)| stored.same_allocation(key))
        {
            hint.set(slot as u32);
        }
        Some(slot)
    }

    #[inline]
    pub(crate) fn get_hinted(
        &self,
        key: &JsPropertyKey,
        hint: Option<&Cell<u32>>,
    ) -> Option<RecordRef<'_>> {
        let slot = self.slot_hinted(key, hint)?;
        self.entry_at(slot).map(|(_, record)| record)
    }

    #[inline]
    pub(crate) fn get_mut_hinted(
        &mut self,
        key: &JsPropertyKey,
        hint: Option<&Cell<u32>>,
    ) -> Option<RecordMut<'_>> {
        let slot = self.slot_hinted(key, hint)?;
        self.entry_at_mut(slot)
    }

    pub(crate) fn get<K: PropertyKeyLike + ?Sized>(&self, key: &K) -> Option<RecordRef<'_>> {
        self.slot_of(key)
            .and_then(|slot| self.entry_at(slot))
            .map(|(_, d)| d)
    }

    pub(crate) fn get_mut<K: PropertyKeyLike + ?Sized>(
        &mut self,
        key: &K,
    ) -> Option<RecordMut<'_>> {
        let slot = self.slot_of(key)?;
        self.entry_at_mut(slot)
    }

    /// The stored record of `key`, which a caller may give any attributes.
    /// A shaped map becomes a per-object one first, since its shape fixes the
    /// attributes of its keys.
    pub(crate) fn get_stored_mut<K: PropertyKeyLike + ?Sized>(
        &mut self,
        key: &K,
    ) -> Option<&mut PropertyRecord> {
        self.slot_of(key)?;
        self.leave_shapes();
        let slot = self.slot_of(key)?;
        match &mut self.inner {
            PropertyMapInner::Inline(v) => v.get_mut(slot).map(|(_, r)| r),
            PropertyMapInner::Spilled { entries, .. } => {
                entries.get_mut(slot)?.as_mut().map(|(_, r)| r)
            }
            PropertyMapInner::Shaped { .. } => unreachable!("left shapes above"),
        }
    }
}
