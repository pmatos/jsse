use std::cell::Cell;
mod exact_capacity;
mod slot_index;
#[cfg(test)]
mod slot_index_tests;
use slot_index::SlotIndex;
use std::collections::HashMap;

use smallvec::SmallVec;

use super::types::PropertyDescriptor;
use crate::types::{JsPropertyKey, JsValue, PropertyKeyLike};

// Properties held inline before spilling to the heap; `JsObjectData` is capped
// at 320 bytes (#909), which leaves room for three entries.
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

const HAS_VALUE: u16 = 1 << 0;
const HAS_GET: u16 = 1 << 1;
const HAS_SET: u16 = 1 << 2;
const HAS_WRITABLE: u16 = 1 << 3;
const IS_WRITABLE: u16 = 1 << 4;
const HAS_ENUMERABLE: u16 = 1 << 5;
const IS_ENUMERABLE: u16 = 1 << 6;
const HAS_CONFIGURABLE: u16 = 1 << 7;
const IS_CONFIGURABLE: u16 = 1 << 8;

/// What reading an own property yields, see [`PropertyRecord::read`].
pub(crate) enum PropertyRead<'a> {
    Value(&'a JsValue),
    Getter(&'a JsValue),
}

/// A `PropertyDescriptor` as the map stores it: 32 bytes where the
/// `Option`-per-field descriptor takes 56. Absent values are `undefined` and a
/// flag word records which fields are present, partial descriptors included.
pub(crate) struct PropertyRecord {
    value: JsValue,
    get: JsValue,
    set: JsValue,
    flags: u16,
}

impl PropertyRecord {
    #[inline]
    fn pack(slot: Option<JsValue>, present: u16, flags: &mut u16) -> JsValue {
        match slot {
            Some(v) => {
                *flags |= present;
                v
            }
            None => JsValue::UNDEFINED,
        }
    }

    #[inline]
    fn pack_bool(slot: Option<bool>, has: u16, is: u16, flags: &mut u16) {
        if let Some(b) = slot {
            *flags |= has;
            if b {
                *flags |= is;
            }
        }
    }

    #[inline]
    fn flag(&self, bit: u16) -> bool {
        self.flags & bit != 0
    }

    #[inline]
    fn tri(&self, has: u16, is: u16) -> Option<bool> {
        self.flag(has).then(|| self.flag(is))
    }

    #[inline]
    fn set_tri(&mut self, has: u16, is: u16, value: Option<bool>) {
        self.flags &= !(has | is);
        Self::pack_bool(value, has, is, &mut self.flags);
    }

    #[inline]
    pub(crate) fn value(&self) -> Option<&JsValue> {
        self.flag(HAS_VALUE).then_some(&self.value)
    }

    #[inline]
    pub(crate) fn getter(&self) -> Option<&JsValue> {
        self.flag(HAS_GET).then_some(&self.get)
    }

    #[inline]
    pub(crate) fn setter(&self) -> Option<&JsValue> {
        self.flag(HAS_SET).then_some(&self.set)
    }

    #[inline]
    pub(crate) fn writable(&self) -> Option<bool> {
        self.tri(HAS_WRITABLE, IS_WRITABLE)
    }

    #[inline]
    pub(crate) fn enumerable(&self) -> Option<bool> {
        self.tri(HAS_ENUMERABLE, IS_ENUMERABLE)
    }

    #[inline]
    pub(crate) fn configurable(&self) -> Option<bool> {
        self.tri(HAS_CONFIGURABLE, IS_CONFIGURABLE)
    }

    #[inline]
    pub(crate) fn is_data_descriptor(&self) -> bool {
        self.flags & (HAS_VALUE | HAS_WRITABLE) != 0
    }

    #[inline]
    pub(crate) fn is_accessor_descriptor(&self) -> bool {
        self.flags & (HAS_GET | HAS_SET) != 0
    }

    /// A data property with a value, no accessor half, and `writable: true`.
    #[inline]
    fn is_plain_writable_data(&self) -> bool {
        const MASK: u16 = HAS_VALUE | HAS_GET | HAS_SET | HAS_WRITABLE | IS_WRITABLE;
        self.flags & MASK == HAS_VALUE | HAS_WRITABLE | IS_WRITABLE
    }

    /// What [[Get]] reads from this own property: its value, or the getter
    /// to call with the receiver. An accessor without a getter reads
    /// `undefined`, like a descriptor without a value.
    #[inline]
    pub(crate) fn read(&self) -> PropertyRead<'_> {
        if self.is_accessor_descriptor() {
            return match self.getter() {
                Some(getter) if !getter.is_undefined() => PropertyRead::Getter(getter),
                _ => PropertyRead::Value(JsValue::undefined_ref()),
            };
        }
        PropertyRead::Value(self.value().unwrap_or(JsValue::undefined_ref()))
    }

    /// Adds `delta` to a writable data property that holds a Number, in
    /// place, and returns the old and new values. `None` leaves the record
    /// untouched.
    #[inline]
    pub(crate) fn add_to_writable_number(&mut self, delta: f64) -> Option<(f64, f64)> {
        if !self.is_plain_writable_data() {
            return None;
        }
        let old = self.value.as_number()?;
        let new = old + delta;
        self.value = JsValue::number(new);
        Some((old, new))
    }

    /// Replaces the value of a writable data property in place. Returns
    /// `false`, leaving the record untouched, when it is anything else (an
    /// accessor, or a data property that is not writable).
    #[inline]
    pub(crate) fn overwrite_writable_data(&mut self, value: &JsValue) -> bool {
        if !self.is_plain_writable_data() {
            return false;
        }
        self.value = value.clone();
        true
    }

    #[inline]
    pub(crate) fn set_value(&mut self, value: Option<JsValue>) {
        self.flags &= !HAS_VALUE;
        self.value = Self::pack(value, HAS_VALUE, &mut self.flags);
    }

    #[inline]
    pub(crate) fn set_writable(&mut self, value: Option<bool>) {
        self.set_tri(HAS_WRITABLE, IS_WRITABLE, value);
    }

    /// The descriptor this record stores, cloned.
    pub(crate) fn to_descriptor(&self) -> PropertyDescriptor {
        PropertyDescriptor {
            value: self.value().cloned(),
            writable: self.writable(),
            get: self.getter().cloned(),
            set: self.setter().cloned(),
            enumerable: self.enumerable(),
            configurable: self.configurable(),
        }
    }

    pub(crate) fn into_descriptor(self) -> PropertyDescriptor {
        let (writable, enumerable, configurable) =
            (self.writable(), self.enumerable(), self.configurable());
        let flags = self.flags;
        let has = |bit: u16| flags & bit != 0;
        PropertyDescriptor {
            value: has(HAS_VALUE).then_some(self.value),
            writable,
            get: has(HAS_GET).then_some(self.get),
            set: has(HAS_SET).then_some(self.set),
            enumerable,
            configurable,
        }
    }
}

impl From<PropertyDescriptor> for PropertyRecord {
    #[inline]
    fn from(d: PropertyDescriptor) -> Self {
        let mut flags = 0;
        let value = Self::pack(d.value, HAS_VALUE, &mut flags);
        let get = Self::pack(d.get, HAS_GET, &mut flags);
        let set = Self::pack(d.set, HAS_SET, &mut flags);
        Self::pack_bool(d.writable, HAS_WRITABLE, IS_WRITABLE, &mut flags);
        Self::pack_bool(d.enumerable, HAS_ENUMERABLE, IS_ENUMERABLE, &mut flags);
        Self::pack_bool(
            d.configurable,
            HAS_CONFIGURABLE,
            IS_CONFIGURABLE,
            &mut flags,
        );
        Self {
            value,
            get,
            set,
            flags,
        }
    }
}

impl Clone for PropertyRecord {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            get: self.get.clone(),
            set: self.set.clone(),
            flags: self.flags,
        }
    }
}

impl std::fmt::Debug for PropertyRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_descriptor().fmt(f)
    }
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
        }
    }

    /// The entry at `slot`, if any. A remembered slot must be checked against
    /// the returned key before use.
    #[inline]
    pub(crate) fn entry_at(&self, slot: usize) -> Option<(&JsPropertyKey, &PropertyRecord)> {
        let entry = match &self.inner {
            PropertyMapInner::Inline(v) => v.get(slot),
            PropertyMapInner::Spilled { entries, .. } => entries.get(slot)?.as_ref(),
        };
        entry.map(|(k, d)| (k, d))
    }

    #[inline]
    pub(crate) fn entry_at_mut(&mut self, slot: usize) -> Option<&mut PropertyRecord> {
        let entry = match &mut self.inner {
            PropertyMapInner::Inline(v) => v.get_mut(slot),
            PropertyMapInner::Spilled { entries, .. } => entries.get_mut(slot)?.as_mut(),
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
        }
    }

    /// Make room for `additional` more entries, spilling an inline map that
    /// would overflow `LINEAR_MAX` straight to a right-sized index so a bulk
    /// copy neither regrows the entry list nor rehashes the index.
    pub(crate) fn reserve(&mut self, additional: usize) {
        match &mut self.inner {
            PropertyMapInner::Inline(v) => {
                let total = v.len() + additional;
                if total <= LINEAR_MAX {
                    if total > INLINE_CAP {
                        v.reserve(additional);
                    }
                    return;
                }
                let mut entries: Vec<Option<Entry>> = Vec::with_capacity(total);
                entries.extend(v.drain(..).map(Some));
                self.inner = Self::spilled(entries, total);
            }
            PropertyMapInner::Spilled { entries, index, .. } => {
                entries.reserve(additional);
                index.reserve(additional);
            }
        }
    }

    fn spill(
        v: &mut SmallVec<[Entry; INLINE_CAP]>,
        key: JsPropertyKey,
        value: PropertyRecord,
    ) -> PropertyMapInner {
        let mut entries: Vec<Option<Entry>> = Vec::with_capacity(LINEAR_MAX + 1);
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
        let removed = match &mut self.inner {
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
                let slot = index.find(entries, key.as_property_key_bytes())?;
                index.forget(key.as_property_key_bytes());
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
}

impl<'a> Iterator for PropertyMapIter<'a> {
    type Item = (&'a JsPropertyKey, &'a PropertyRecord);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Inline(it) => it.next().map(|(k, v)| (k, v)),
            Self::Spilled(it) => it.find_map(|e| e.as_ref().map(|(k, v)| (k, v))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::types::JsObjectData;
    use crate::types::JsValue;

    fn key(s: impl AsRef<str>) -> JsPropertyKey {
        JsPropertyKey::from_str(s.as_ref())
    }

    fn desc(n: f64) -> PropertyDescriptor {
        PropertyDescriptor::data_default(JsValue::number(n))
    }

    fn num(d: &PropertyRecord) -> f64 {
        d.value().and_then(JsValue::as_number).unwrap_or(f64::NAN)
    }

    fn is_inline(map: &PropertyMap) -> bool {
        matches!(map.inner, PropertyMapInner::Inline(_))
    }

    #[test]
    fn inline_insert_and_get() {
        let mut m = PropertyMap::new();
        for i in 0..LINEAR_MAX {
            assert!(m.insert(key(format!("k{i}")), desc(i as f64)).is_none());
        }
        assert!(is_inline(&m));
        for i in 0..LINEAR_MAX {
            let d = m.get(&format!("k{i}")).expect("present");
            assert_eq!(num(d), i as f64);
        }
        assert!(m.get("missing").is_none());
    }

    #[test]
    fn one_past_linear_max_triggers_spill() {
        let mut m = PropertyMap::new();
        for i in 0..LINEAR_MAX {
            m.insert(key(format!("k{i}")), desc(i as f64));
        }
        assert!(is_inline(&m));
        m.insert(key(format!("k{LINEAR_MAX}")), desc(LINEAR_MAX as f64));
        assert!(!is_inline(&m));
        for i in 0..=LINEAR_MAX {
            assert!(
                m.get(&format!("k{i}")).is_some(),
                "k{i} missing after spill"
            );
        }
    }

    #[test]
    fn duplicate_insert_returns_previous_and_no_spill() {
        let mut m = PropertyMap::new();
        for i in 0..LINEAR_MAX {
            m.insert(key(format!("k{i}")), desc(i as f64));
        }
        let prev = m.insert(key("k0"), desc(99.0)).expect("had previous");
        assert_eq!(prev.value.as_ref().and_then(JsValue::as_number), Some(0.0));
        assert!(is_inline(&m));
        assert_eq!(num(m.get("k0").expect("present")), 99.0);
    }

    #[test]
    fn reserve_spills_only_when_the_total_overflows_linear_max() {
        let mut m = PropertyMap::new();
        m.insert(key("a"), desc(1.0));
        m.reserve(LINEAR_MAX - 1);
        assert!(is_inline(&m), "fits inline: no spill");
        m.reserve(LINEAR_MAX);
        assert!(!is_inline(&m), "would overflow: spilled up front");
        assert_eq!(num(m.get("a").expect("kept across the spill")), 1.0);
        for i in 0..32 {
            m.insert(key(format!("k{i}")), desc(i as f64));
        }
        for i in 0..32 {
            assert_eq!(num(m.get(&format!("k{i}")).expect("present")), i as f64);
        }
    }

    #[test]
    fn remove_after_spill_keeps_spilled() {
        let mut m = PropertyMap::new();
        for i in 0..=LINEAR_MAX {
            m.insert(key(format!("k{i}")), desc(i as f64));
        }
        assert!(!is_inline(&m));
        for i in 0..=LINEAR_MAX {
            assert!(m.remove(&format!("k{i}")).is_some());
        }
        assert!(!is_inline(&m), "spill must not collapse back to inline");
        assert!(m.remove("k0").is_none());
    }

    #[test]
    fn iter_visits_all_entries_in_both_modes() {
        let mut m = PropertyMap::new();
        for i in 0..3 {
            m.insert(key(format!("k{i}")), desc(i as f64));
        }
        let mut seen: Vec<(String, f64)> = m
            .iter()
            .map(|(k, d)| {
                let n = d.value().and_then(JsValue::as_number).unwrap_or(-1.0);
                (k.to_string(), n)
            })
            .collect();
        seen.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            seen,
            vec![("k0".into(), 0.0), ("k1".into(), 1.0), ("k2".into(), 2.0)]
        );

        for i in 3..=LINEAR_MAX {
            m.insert(key(format!("k{i}")), desc(i as f64));
        }
        assert!(!is_inline(&m));
        let mut seen: Vec<(String, f64)> = m
            .iter()
            .map(|(k, d)| {
                let n = d.value().and_then(JsValue::as_number).unwrap_or(-1.0);
                (k.to_string(), n)
            })
            .collect();
        seen.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(seen.len(), LINEAR_MAX + 1);
        for (i, (k, n)) in seen.iter().enumerate() {
            assert_eq!(*k, format!("k{i}"));
            assert_eq!(*n, i as f64);
        }
    }

    #[test]
    fn empty_iteration_is_empty() {
        let m = PropertyMap::new();
        assert_eq!(m.iter().count(), 0);
        assert_eq!(m.keys().count(), 0);
        assert_eq!(m.values().count(), 0);
    }

    #[test]
    fn get_mut_in_both_modes() {
        let mut m = PropertyMap::new();
        m.insert(key("a"), desc(1.0));
        if let Some(d) = m.get_mut("a") {
            d.set_value(Some(JsValue::number(7.0)));
        }
        assert_eq!(num(m.get("a").expect("present")), 7.0);

        for i in 0..LINEAR_MAX {
            m.insert(key(format!("k{i}")), desc(i as f64));
        }
        assert!(!is_inline(&m));
        if let Some(d) = m.get_mut("a") {
            d.set_value(Some(JsValue::number(42.0)));
        }
        assert_eq!(num(m.get("a").expect("present")), 42.0);
    }

    #[test]
    fn keys_keep_symbol_and_symbol_like_string_distinct_in_both_modes() {
        let mut m = PropertyMap::new();
        let symbol = JsPropertyKey::well_known_symbol("iterator");
        let text = key("Symbol(Symbol.iterator)");
        m.insert(symbol.clone(), desc(1.0));
        m.insert(text.clone(), desc(2.0));
        let keys: Vec<&JsPropertyKey> = m.keys().collect();
        assert!(keys.contains(&&symbol));
        assert!(keys.contains(&&text));
        assert_eq!(num(m.get(&symbol).expect("Symbol key present")), 1.0);
        assert_eq!(num(m.get(&text).expect("String key present")), 2.0);
        for i in 0..LINEAR_MAX {
            m.insert(key(format!("k{i}")), desc(i as f64));
        }
        assert!(!is_inline(&m));
        let keys: Vec<&JsPropertyKey> = m.keys().collect();
        assert!(keys.contains(&&symbol), "Symbol key lost on spill");
        assert!(keys.contains(&&text), "String key lost on spill");
    }

    #[test]
    fn index_key_count_tracks_digit_leading_keys() {
        let mut m = PropertyMap::new();
        assert!(!m.may_hold_index_key());
        m.insert(key("length"), desc(0.0));
        m.insert(key(""), desc(0.0));
        m.insert(key("a1"), desc(0.0));
        assert!(
            !m.may_hold_index_key(),
            "names that do not start with a digit"
        );

        m.insert(key("7"), desc(1.0));
        assert!(m.may_hold_index_key());
        m.insert(key("7"), desc(2.0));
        m.remove("7");
        assert!(
            !m.may_hold_index_key(),
            "replacing a key must not count twice"
        );
        assert!(m.remove("7").is_none());
        assert!(
            !m.may_hold_index_key(),
            "removing an absent key changes nothing"
        );

        let mut fresh = PropertyMap::new();
        fresh.push_fresh(key("0"), desc(1.0));
        assert!(fresh.may_hold_index_key(), "push_fresh records it too");

        let mut spilled = PropertyMap::new();
        for i in 0..=LINEAR_MAX {
            spilled.insert(key(format!("k{i}")), desc(i as f64));
        }
        assert!(
            !spilled.may_hold_index_key(),
            "spilling alone does not set it"
        );
        spilled.insert(key("4294967294"), desc(1.0));
        assert!(spilled.may_hold_index_key(), "recorded in spilled mode");
        spilled.remove("4294967294");
        assert!(!spilled.may_hold_index_key(), "cleared in spilled mode");
    }

    #[test]
    fn record_round_trips_partial_descriptors() {
        let cases = [
            PropertyDescriptor {
                value: Some(JsValue::number(1.0)),
                writable: Some(false),
                get: None,
                set: None,
                enumerable: None,
                configurable: Some(true),
            },
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(JsValue::UNDEFINED),
                set: None,
                enumerable: Some(false),
                configurable: None,
            },
            PropertyDescriptor {
                value: None,
                writable: None,
                get: None,
                set: Some(JsValue::number(2.0)),
                enumerable: Some(true),
                configurable: Some(false),
            },
            PropertyDescriptor {
                value: None,
                writable: None,
                get: None,
                set: None,
                enumerable: None,
                configurable: None,
            },
        ];
        for d in cases {
            let record = PropertyRecord::from(d.clone());
            let back = record.to_descriptor();
            assert_eq!(back.value.is_some(), d.value.is_some());
            assert_eq!(back.get.is_some(), d.get.is_some());
            assert_eq!(back.set.is_some(), d.set.is_some());
            assert_eq!(back.writable, d.writable);
            assert_eq!(back.enumerable, d.enumerable);
            assert_eq!(back.configurable, d.configurable);
            assert_eq!(record.is_data_descriptor(), d.is_data_descriptor());
            assert_eq!(record.is_accessor_descriptor(), d.is_accessor_descriptor());
            let moved = record.into_descriptor();
            assert_eq!(moved.writable, d.writable);
            assert_eq!(moved.value.is_some(), d.value.is_some());
        }
    }

    #[test]
    fn record_setters_touch_only_their_field() {
        let mut record = PropertyRecord::from(PropertyDescriptor::data(
            JsValue::number(1.0),
            true,
            false,
            true,
        ));
        record.set_value(Some(JsValue::number(9.0)));
        assert_eq!(num(&record), 9.0);
        assert_eq!(record.writable(), Some(true));
        assert_eq!(record.enumerable(), Some(false));
        assert_eq!(record.configurable(), Some(true));
        record.set_writable(Some(false));
        assert_eq!(record.writable(), Some(false));
        assert_eq!(num(&record), 9.0);
        record.set_writable(None);
        assert_eq!(record.writable(), None);
        record.set_value(None);
        assert!(record.value().is_none());
        assert_eq!(record.enumerable(), Some(false));
    }

    #[test]
    fn data_value_and_in_place_writes_respect_attributes() {
        let mut m = PropertyMap::new();
        m.insert(key("w"), desc(1.0));
        m.insert(
            key("ro"),
            PropertyDescriptor {
                value: Some(JsValue::number(2.0)),
                writable: Some(false),
                enumerable: Some(true),
                configurable: Some(true),
                get: None,
                set: None,
            },
        );
        m.insert(
            key("acc"),
            PropertyDescriptor {
                value: None,
                writable: None,
                enumerable: Some(true),
                configurable: Some(true),
                get: Some(JsValue::UNDEFINED),
                set: None,
            },
        );
        assert!(matches!(m.get("w").unwrap().read(), PropertyRead::Value(_)));
        assert!(matches!(
            m.get("ro").unwrap().read(),
            PropertyRead::Value(_)
        ));
        assert!(matches!(
            m.get("acc").unwrap().read(),
            PropertyRead::Value(v) if v.is_undefined()
        ));

        assert!(
            m.get_mut("w")
                .unwrap()
                .overwrite_writable_data(&JsValue::number(5.0))
        );
        assert_eq!(num(m.get("w").unwrap()), 5.0);
        assert!(
            !m.get_mut("ro")
                .unwrap()
                .overwrite_writable_data(&JsValue::number(5.0))
        );
        assert_eq!(num(m.get("ro").unwrap()), 2.0);
        assert!(
            !m.get_mut("acc")
                .unwrap()
                .overwrite_writable_data(&JsValue::number(5.0))
        );

        assert_eq!(
            m.get_mut("w").unwrap().add_to_writable_number(1.0),
            Some((5.0, 6.0))
        );
        assert_eq!(num(m.get("w").unwrap()), 6.0);
        assert_eq!(m.get_mut("ro").unwrap().add_to_writable_number(1.0), None);
        assert_eq!(num(m.get("ro").unwrap()), 2.0);
        m.get_mut("w")
            .unwrap()
            .set_value(Some(JsValue::string(crate::types::JsString::from_str("s"))));
        assert_eq!(m.get_mut("w").unwrap().add_to_writable_number(1.0), None);
    }

    #[test]
    fn key_lookup_distinguishes_equal_length_keys_and_non_interned_copies() {
        let mut m = PropertyMap::new();
        m.insert(key("ab"), desc(1.0));
        m.insert(key("ac"), desc(2.0));
        m.insert(key("bb"), desc(3.0));
        assert_eq!(num(m.get("ab").unwrap()), 1.0);
        assert_eq!(num(m.get("ac").unwrap()), 2.0);
        assert_eq!(num(m.get("bb").unwrap()), 3.0);
        assert!(m.get("ba").is_none());
        assert!(m.get("a").is_none());
        let fresh_copy = JsPropertyKey::from_str("ac");
        assert_eq!(num(m.get(&fresh_copy).unwrap()), 2.0);
        assert!(m.remove(&fresh_copy).is_some());
        assert!(m.get("ac").is_none());
    }

    #[test]
    fn hinted_lookup_agrees_with_plain_lookup_across_spill_and_removal() {
        let hint = Cell::new(0u32);
        let mut m = PropertyMap::new();
        let probe = key("target");
        assert!(m.get_hinted(&probe, Some(&hint)).is_none(), "empty map");
        for round in 0..2 {
            for i in 0..20 {
                m.insert(key(format!("k{i}")), desc(i as f64));
                assert!(
                    m.get_hinted(&probe, Some(&hint)).is_none(),
                    "round {round}: absent"
                );
            }
            m.insert(probe.clone(), desc(99.0));
            assert_eq!(num(m.get_hinted(&probe, Some(&hint)).unwrap()), 99.0);
            for i in 0..20 {
                let k = key(format!("k{i}"));
                let got = m.get_hinted(&k, Some(&Cell::new(7))).map(num);
                assert_eq!(got, Some(i as f64));
                assert_eq!(m.get_hinted(&k, None).map(num), Some(i as f64));
            }
            assert!(m.remove("target").is_some());
            assert!(m.get_hinted(&probe, Some(&hint)).is_none(), "removed");
            for i in 0..20 {
                m.remove(&format!("k{i}"));
            }
        }
    }

    #[test]
    fn js_object_data_size_cap() {
        // #909: box cold state rather than raising the cap.
        let size = std::mem::size_of::<JsObjectData>();
        assert!(size <= 320, "JsObjectData grew to {size} bytes (cap 320)");
    }

    #[test]
    fn slot_round_trips_in_both_modes() {
        let mut m = PropertyMap::new();
        for i in 0..(LINEAR_MAX + 3) {
            m.insert(key(format!("k{i}")), desc(i as f64));
            let name = format!("k{i}");
            let slot = m.slot_of(&name).expect("slot");
            let (k, d) = m.entry_at(slot).expect("entry");
            assert_eq!(k.as_bytes(), name.as_bytes());
            assert_eq!(num(d), i as f64);
        }
        assert!(!is_inline(&m));
        assert!(m.slot_of("missing").is_none());
        assert!(m.entry_at(10_000).is_none());
        let slot = m.slot_of("k1").unwrap();
        m.entry_at_mut(slot)
            .unwrap()
            .set_value(Some(JsValue::number(99.0)));
        assert_eq!(num(m.get("k1").unwrap()), 99.0);
    }

    #[test]
    fn remembered_slot_is_detectable_after_remove_and_reinsert() {
        let mut m = PropertyMap::new();
        let total = LINEAR_MAX + 4;
        for i in 0..total {
            m.insert(key(format!("k{i}")), desc(i as f64));
        }
        let stale = m.slot_of("k2").unwrap();
        m.remove("k2").expect("removed");
        // The slot is a tombstone, so a verifying caller sees no entry.
        assert!(m.entry_at(stale).is_none());
        assert!(m.get("k2").is_none());
        assert!(m.slot_of("k2").is_none());
        // Every survivor is still reachable and its slot verifies.
        for i in (0..total).filter(|&i| i != 2) {
            let name = format!("k{i}");
            let slot = m.slot_of(&name).expect("survivor");
            assert_eq!(m.entry_at(slot).unwrap().0.as_bytes(), name.as_bytes());
            assert_eq!(num(m.get(&name).unwrap()), i as f64);
        }
        m.insert(key("k2"), desc(2.0));
        assert_eq!(num(m.get("k2").unwrap()), 2.0);
        assert_eq!(m.iter().count(), total);
        assert_eq!(m.len(), total);
    }

    fn order(m: &PropertyMap) -> Vec<String> {
        m.keys().map(|k| k.to_string()).collect()
    }

    #[test]
    fn iteration_is_creation_order_in_both_modes() {
        for total in [3, LINEAR_MAX + 5] {
            let mut m = PropertyMap::new();
            for i in 0..total {
                m.insert(key(format!("k{i}")), desc(i as f64));
            }
            // Replacing keeps the position; removing and re-adding moves to the end.
            m.insert(key("k1"), desc(100.0));
            m.remove("k0");
            m.insert(key("k0"), desc(0.0));
            let mut want: Vec<String> = (1..total).map(|i| format!("k{i}")).collect();
            want.push("k0".into());
            assert_eq!(order(&m), want, "total {total}");
            let values: Vec<f64> = m.values().map(num).collect();
            assert_eq!(values.len(), total);
            assert_eq!(values[0], 100.0);
        }
    }

    #[test]
    fn spilled_removal_keeps_order_through_tombstones_and_compaction() {
        let mut m = PropertyMap::new();
        let total = COMPACT_MIN_ENTRIES * 4;
        for i in 0..total {
            m.insert(key(format!("k{i}")), desc(i as f64));
        }
        assert!(!is_inline(&m));
        let mut want: Vec<usize> = (0..total).collect();
        // Delete every entry but each tenth, in a scattered order, crossing
        // the compaction threshold several times.
        for step in [3usize, 7, 1] {
            for i in (0..total).filter(|i| i % step == 0 && i % 10 != 0) {
                if m.remove(&format!("k{i}")).is_some() {
                    want.retain(|w| *w != i);
                }
            }
        }
        assert_eq!(m.len(), want.len());
        assert_eq!(
            order(&m),
            want.iter().map(|i| format!("k{i}")).collect::<Vec<_>>()
        );
        for i in &want {
            let name = format!("k{i}");
            let slot = m.slot_of(&name).expect("survivor indexed");
            assert_eq!(m.entry_at(slot).unwrap().0.as_bytes(), name.as_bytes());
            assert_eq!(num(m.get(&name).unwrap()), *i as f64);
        }
        m.insert(key("late"), desc(-1.0));
        assert_eq!(order(&m).last().map(String::as_str), Some("late"));
    }

    #[test]
    fn removing_every_spilled_entry_leaves_an_empty_reusable_map() {
        let mut m = PropertyMap::new();
        for i in 0..(LINEAR_MAX + 6) {
            m.insert(key(format!("k{i}")), desc(i as f64));
        }
        for i in 0..(LINEAR_MAX + 6) {
            assert!(m.remove(&format!("k{i}")).is_some());
        }
        assert_eq!(m.len(), 0);
        assert_eq!(m.iter().count(), 0);
        m.insert(key("again"), desc(1.0));
        assert_eq!(order(&m), vec!["again".to_string()]);
        assert_eq!(num(m.get("again").unwrap()), 1.0);
    }

    #[test]
    fn removing_the_last_entry_leaves_index_consistent() {
        let mut m = PropertyMap::new();
        let total = LINEAR_MAX + 2;
        for i in 0..total {
            m.insert(key(format!("k{i}")), desc(i as f64));
        }
        let last = format!("k{}", total - 1);
        assert!(m.remove(&last).is_some());
        assert!(m.get(&last).is_none());
        assert_eq!(m.iter().count(), total - 1);
        for i in 0..total - 1 {
            assert!(m.get(&format!("k{i}")).is_some());
        }
    }

    #[test]
    fn push_fresh_preserves_entries_across_the_spill_boundary() {
        let mut m = PropertyMap::new();
        for i in 0..LINEAR_MAX {
            m.push_fresh(key(format!("k{i}")), desc(i as f64));
        }
        assert!(is_inline(&m));
        m.push_fresh(key(format!("k{LINEAR_MAX}")), desc(LINEAR_MAX as f64));
        assert!(!is_inline(&m));
        for i in 0..=LINEAR_MAX {
            assert_eq!(num(m.get(&format!("k{i}")).expect("present")), i as f64);
        }
    }

    #[test]
    fn heap_backed_linear_entries_beyond_inline_cap_stay_linear() {
        let mut m = PropertyMap::new();
        for i in 0..=INLINE_CAP {
            m.insert(key(format!("k{i}")), desc(i as f64));
        }
        assert!(is_inline(&m), "{} entries must not hash", INLINE_CAP + 1);
    }
}
