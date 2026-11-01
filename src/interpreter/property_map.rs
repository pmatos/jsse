use std::cell::Cell;
mod exact_capacity;
mod record;
mod shape;
#[cfg(test)]
mod shape_tests;
mod shaped;
mod slot_index;
#[cfg(test)]
mod slot_index_tests;
pub(crate) use record::{PropertyRead, PropertyRecord};
use shape::Shape;
pub(crate) use shape::ShapesPaused;
use slot_index::SlotIndex;
use std::collections::HashMap;
use std::rc::Rc;

use smallvec::SmallVec;

use super::types::PropertyDescriptor;
use crate::types::{JsPropertyKey, JsValue, PropertyKeyLike};

// Properties held inline before spilling to the heap; `JsObjectData` is capped
// at 272 bytes (#909, #1401), which leaves room for three entries.
const INLINE_CAP: usize = 3;

// Largest entry count kept in the linear (scan-only) representation; entries
// beyond `INLINE_CAP` live in the SmallVec's heap buffer, cheaper than hashing
// for a sloppy function's 5 properties (#901).
const LINEAR_MAX: usize = 8;

// A spilled map compacts once tombstones outnumber live entries by this much
// (and the entry list is long enough for the rebuild to matter).
const COMPACT_MIN_ENTRIES: usize = 32;

type Entry = (JsPropertyKey, PropertyRecord);

/// Property-key byte equality: interned keys share an allocation, so a hit is
/// settled by address and most mismatches by length or first byte, without a
/// `memcmp` call per entry.
#[inline(always)]
fn key_bytes_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len()
        && (std::ptr::eq(a.as_ptr(), b.as_ptr()) || (a.first() == b.first() && a == b))
}

// Property names come from script, so the index is keyed with foldhash's
// randomly seeded state (minimally DoS-resistant, unlike the SipHash kept for
// identifier maps by #393); see #911.
type IndexState = foldhash::fast::RandomState;

// Hybrid storage for `JsObjectData.properties`, in property creation order
// (the order [[OwnPropertyKeys]] reports String and Symbol keys in): up to
// `LINEAR_MAX` entries in a linear-scan `SmallVec` (the first `INLINE_CAP`
// without a heap alloc), then a dense entry list plus a key->slot `SlotIndex`
// (tag bytes up to 64 slots, a randomised hash table beyond) once that
// overflows. Spill is one-way.
//
// Entries are addressable by slot (`slot_of` / `entry_at`) so a hot site can
// remember where a key lived and skip the hash. A slot is only ever a *hint*:
// an inline `remove` shifts later entries down, a spilled `remove` leaves a
// tombstone that a later compaction squeezes out, so a remembered slot must be
// re-verified by key before it is trusted. `entry_at` returns the key for
// exactly that purpose.
pub(crate) struct PropertyMap {
    inner: PropertyMapInner,
    // Number of present keys that begin with an ASCII digit. Zero proves no
    // array-index key lives here, which lets Array element reads and writes
    // skip formatting the index and probing the map. A "0x" or "1e3" name
    // also counts; that only costs the slower probe.
    digit_leading_keys: u32,
}

// The inline variant is intentionally large — eliminating the heap allocation
// for small objects is the entire point of issue #68. Boxing it would defeat
// the optimisation.
// Keys are `JsPropertyKey` (issue #317): interned keys share their backing
// bytes with every other object that has the property. Its WTF-8 bytes
// preserve arbitrary UTF-16 keys, and ordinary `&str` lookups borrow their
// UTF-8 bytes directly.
#[allow(clippy::large_enum_variant)]
enum PropertyMapInner {
    Inline(SmallVec<[Entry; INLINE_CAP]>),
    Spilled {
        // Creation order. `None` is a deleted entry whose slot the index no
        // longer names; `live` counts the `Some`s.
        entries: Vec<Option<Entry>>,
        index: SlotIndex,
        live: usize,
    },
    // Keys shared with the objects built the same way, one record per key,
    // in creation order (see `shape.rs`).
    Shaped {
        shape: Rc<Shape>,
        records: shaped::Records,
    },
}

impl PropertyMap {
    pub(crate) fn new() -> Self {
        Self {
            inner: PropertyMapInner::Inline(SmallVec::new()),
            digit_leading_keys: 0,
        }
    }

    /// Whether any array-index key may be present. `false` guarantees that no
    /// key of the form `"0"`..`"4294967294"` is.
    #[inline]
    pub(crate) fn may_hold_index_key(&self) -> bool {
        self.digit_leading_keys != 0
    }

    #[inline]
    fn is_digit_leading(key: &[u8]) -> bool {
        key.first().is_some_and(u8::is_ascii_digit)
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        match &self.inner {
            PropertyMapInner::Inline(v) => v.len(),
            PropertyMapInner::Spilled { live, .. } => *live,
            PropertyMapInner::Shaped { records, .. } => records.len(),
        }
    }

    pub(crate) fn contains_key<K: PropertyKeyLike + ?Sized>(&self, key: &K) -> bool {
        self.slot_of(key).is_some()
    }

    /// Slot currently holding `key`. Valid only until the next `insert` of a
    /// new key or any `remove`.
    #[inline]
    pub(crate) fn slot_of<K: PropertyKeyLike + ?Sized>(&self, key: &K) -> Option<usize> {
        match &self.inner {
            PropertyMapInner::Inline(v) => v
                .iter()
                .position(|(k, _)| key_bytes_eq(k.as_bytes(), key.as_property_key_bytes())),
            PropertyMapInner::Spilled { entries, index, .. } => {
                index.find(entries, key.as_property_key_bytes())
            }
            PropertyMapInner::Shaped { shape, .. } => shape.find(key.as_property_key_bytes()),
        }
    }

    /// The entry at `slot`, if any. A remembered slot must be checked against
    /// the returned key before use.
    #[inline]
    pub(crate) fn entry_at(&self, slot: usize) -> Option<(&JsPropertyKey, &PropertyRecord)> {
        let entry = match &self.inner {
            PropertyMapInner::Inline(v) => v.get(slot),
            PropertyMapInner::Spilled { entries, .. } => entries.get(slot)?.as_ref(),
            PropertyMapInner::Shaped { shape, records } => {
                return Some((shape.keys().get(slot)?, records.get(slot)?));
            }
        };
        entry.map(|(k, d)| (k, d))
    }

    #[inline]
    pub(crate) fn entry_at_mut(&mut self, slot: usize) -> Option<&mut PropertyRecord> {
        let entry = match &mut self.inner {
            PropertyMapInner::Inline(v) => v.get_mut(slot),
            PropertyMapInner::Spilled { entries, .. } => entries.get_mut(slot)?.as_mut(),
            PropertyMapInner::Shaped { records, .. } => return records.get_mut(slot),
        };
        entry.map(|(_, d)| d)
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
    ) -> Option<&PropertyRecord> {
        let slot = self.slot_hinted(key, hint)?;
        self.entry_at(slot).map(|(_, record)| record)
    }

    #[inline]
    pub(crate) fn get_mut_hinted(
        &mut self,
        key: &JsPropertyKey,
        hint: Option<&Cell<u32>>,
    ) -> Option<&mut PropertyRecord> {
        let slot = self.slot_hinted(key, hint)?;
        self.entry_at_mut(slot)
    }

    pub(crate) fn get<K: PropertyKeyLike + ?Sized>(&self, key: &K) -> Option<&PropertyRecord> {
        self.slot_of(key)
            .and_then(|slot| self.entry_at(slot))
            .map(|(_, d)| d)
    }

    pub(crate) fn get_mut<K: PropertyKeyLike + ?Sized>(
        &mut self,
        key: &K,
    ) -> Option<&mut PropertyRecord> {
        let slot = self.slot_of(key)?;
        self.entry_at_mut(slot)
    }

    /// Insert or replace `key`. A new key is appended in creation order; an
    /// existing key keeps its position.
    pub(crate) fn insert(
        &mut self,
        key: JsPropertyKey,
        value: PropertyDescriptor,
    ) -> Option<PropertyDescriptor> {
        let value = PropertyRecord::from(value);
        match &mut self.inner {
            PropertyMapInner::Inline(v) => {
                if let Some(slot) = v.iter_mut().find(|(k, _)| *k == key) {
                    return Some(std::mem::replace(&mut slot.1, value).into_descriptor());
                }
            }
            PropertyMapInner::Shaped { shape, records } => {
                if let Some(slot) = shape.find(key.as_bytes()) {
                    return Some(std::mem::replace(&mut records[slot], value).into_descriptor());
                }
            }
            PropertyMapInner::Spilled {
                entries,
                index,
                live,
            } => {
                if let Some(slot) = index.find(entries, key.as_bytes()) {
                    let entry = entries[slot].as_mut().expect("indexed slot is live");
                    return Some(std::mem::replace(&mut entry.1, value).into_descriptor());
                }
                self.digit_leading_keys += u32::from(Self::is_digit_leading(key.as_bytes()));
                index.push(entries, key, value);
                *live += 1;
                return None;
            }
        }
        self.append(key, value);
        None
    }

    /// Append an entry whose key the caller knows is absent. Skips the
    /// duplicate scan of [`insert`]; used on the fresh-object construction
    /// path (issue #901). Debug builds assert the key really is absent.
    #[inline]
    pub(crate) fn push_fresh(&mut self, key: JsPropertyKey, value: PropertyDescriptor) {
        debug_assert!(
            !self.contains_key(&key),
            "push_fresh called with a key that is already present"
        );
        self.append(key, value.into());
    }

    #[inline]
    fn append(&mut self, key: JsPropertyKey, value: PropertyRecord) {
        self.digit_leading_keys += u32::from(Self::is_digit_leading(key.as_bytes()));
        match &mut self.inner {
            PropertyMapInner::Shaped { .. } => self.append_shaped(key, value),
            PropertyMapInner::Inline(v) if v.is_empty() => match self.begin_shape(&key, value) {
                Ok(()) => {}
                Err(value) => self.append_per_object(key, value),
            },
            _ => self.append_per_object(key, value),
        }
    }

    #[inline]
    fn append_per_object(&mut self, key: JsPropertyKey, value: PropertyRecord) {
        match &mut self.inner {
            PropertyMapInner::Inline(v) if v.len() < LINEAR_MAX => v.push((key, value)),
            PropertyMapInner::Inline(v) => {
                self.inner = Self::spill(v, key, value);
            }
            PropertyMapInner::Spilled {
                entries,
                index,
                live,
            } => {
                index.push(entries, key, value);
                *live += 1;
            }
            PropertyMapInner::Shaped { .. } => unreachable!("a shaped map appends on its shape"),
        }
    }

    /// Make room for `additional` more entries, spilling an inline map that
    /// would overflow `LINEAR_MAX` straight to a right-sized index so a bulk
    /// copy neither regrows the entry list nor rehashes the index.
    pub(crate) fn reserve(&mut self, additional: usize) {
        match &mut self.inner {
            PropertyMapInner::Inline(v)
                if v.is_empty()
                    && (1..=shape::SHAPED_MAX).contains(&additional)
                    && shape::shapes_enabled() =>
            {
                self.begin_shape_with_room(additional);
            }
            PropertyMapInner::Shaped { .. } => self.reserve_shaped(additional),
            PropertyMapInner::Inline(v) => {
                let total = v.len() + additional;
                if total <= LINEAR_MAX {
                    if total > INLINE_CAP {
                        v.reserve_exact(additional);
                    }
                    return;
                }
                let mut entries: Vec<Option<Entry>> =
                    Vec::with_capacity(slot_index::round_capacity(total));
                entries.extend(v.drain(..).map(Some));
                self.inner = Self::spilled(entries, total);
            }
            PropertyMapInner::Spilled { entries, index, .. } => {
                index.reserve_entries(entries, additional);
                index.reserve(entries, additional);
            }
        }
    }

    fn spill(
        v: &mut SmallVec<[Entry; INLINE_CAP]>,
        key: JsPropertyKey,
        value: PropertyRecord,
    ) -> PropertyMapInner {
        let mut entries: Vec<Option<Entry>> = Vec::with_capacity(slot_index::SPILL_CAPACITY);
        entries.extend(v.drain(..).map(Some));
        entries.push(Some((key, value)));
        Self::spilled(entries, LINEAR_MAX + 1)
    }

    fn spilled(entries: Vec<Option<Entry>>, capacity: usize) -> PropertyMapInner {
        let index = SlotIndex::build(&entries, capacity);
        PropertyMapInner::Spilled {
            live: entries.len(),
            entries,
            index,
        }
    }

    pub(crate) fn remove<K: PropertyKeyLike + ?Sized>(
        &mut self,
        key: &K,
    ) -> Option<PropertyDescriptor> {
        if matches!(self.inner, PropertyMapInner::Shaped { .. }) {
            self.slot_of(key)?;
            if let Some(removed) = self.pop_shaped(key.as_property_key_bytes()) {
                self.digit_leading_keys -=
                    u32::from(Self::is_digit_leading(key.as_property_key_bytes()));
                return Some(removed.into_descriptor());
            }
            self.leave_shapes();
        }
        let removed = match &mut self.inner {
            PropertyMapInner::Shaped { .. } => unreachable!("left shapes above"),
            PropertyMapInner::Inline(v) => {
                let pos = v
                    .iter()
                    .position(|(k, _)| key_bytes_eq(k.as_bytes(), key.as_property_key_bytes()))?;
                v.remove(pos).1
            }
            PropertyMapInner::Spilled {
                entries,
                index,
                live,
            } => {
                let slot = index.remove(entries, key.as_property_key_bytes())?;
                let (_, removed) = entries[slot].take()?;
                *live -= 1;
                if slot + 1 == entries.len() {
                    while matches!(entries.last(), Some(None)) {
                        entries.pop();
                    }
                    index.truncated(entries.len());
                } else if entries.len() >= COMPACT_MIN_ENTRIES && entries.len() > 2 * *live {
                    index.compact(entries);
                }
                removed
            }
        };
        if Self::is_digit_leading(key.as_property_key_bytes()) {
            self.digit_leading_keys -= 1;
        }
        Some(removed.into_descriptor())
    }

    pub(crate) fn iter(&self) -> PropertyMapIter<'_> {
        match &self.inner {
            PropertyMapInner::Inline(v) => PropertyMapIter::Inline(v.iter()),
            PropertyMapInner::Spilled { entries, .. } => PropertyMapIter::Spilled(entries.iter()),
            PropertyMapInner::Shaped { shape, records } => {
                PropertyMapIter::Shaped(shape.keys().iter().zip(records.iter()))
            }
        }
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = &JsPropertyKey> {
        self.iter().map(|(k, _)| k)
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &PropertyRecord> {
        self.iter().map(|(_, v)| v)
    }
}

impl Default for PropertyMap {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) enum PropertyMapIter<'a> {
    Inline(std::slice::Iter<'a, Entry>),
    Spilled(std::slice::Iter<'a, Option<Entry>>),
    Shaped(
        std::iter::Zip<std::slice::Iter<'a, JsPropertyKey>, std::slice::Iter<'a, PropertyRecord>>,
    ),
}

impl<'a> Iterator for PropertyMapIter<'a> {
    type Item = (&'a JsPropertyKey, &'a PropertyRecord);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Inline(it) => it.next().map(|(k, v)| (k, v)),
            Self::Spilled(it) => it.find_map(|e| e.as_ref().map(|(k, v)| (k, v))),
            Self::Shaped(it) => it.next(),
        }
    }
}

#[cfg(test)]
mod map_tests;
#[cfg(test)]
mod record_and_slot_tests;
