use std::collections::HashMap;

use smallvec::SmallVec;

use super::types::PropertyDescriptor;
use crate::types::{JsPropertyKey, PropertyKeyLike};

// Properties held inline in the SmallVec before it spills to the heap. 8 would
// push JsObjectData past 3.5 KB (issue #68), so 4 — enough for the splay-node
// case (4 fields).
const INLINE_CAP: usize = 4;

// Largest entry count kept in the linear (scan-only) representation. Entries
// beyond `INLINE_CAP` live in the SmallVec's heap buffer, which is cheaper
// than hashing for the 5-property shape of a sloppy constructable function
// (issue #901).
const LINEAR_MAX: usize = 8;

// Hybrid storage for `JsObjectData.properties`: up to `LINEAR_MAX` entries in a
// linear-scan `SmallVec` (the first `INLINE_CAP` without a heap alloc), then a
// dense entry list plus a randomised key→slot index once that overflows. Spill
// is one-way; deletes never collapse back to linear. Iteration order is
// unspecified — ordered iteration must continue to go through
// `JsObjectData.property_order`.
//
// Entries are addressable by slot (`slot_of` / `entry_at`) so a hot site can
// remember where a key lived and skip the hash. A slot is only ever a *hint*:
// `remove` swap-moves the last entry into the hole (and linear `remove`
// shifts), so a remembered slot must be re-verified by key before it is
// trusted. `entry_at` returns the key for exactly that purpose.
pub(crate) struct PropertyMap {
    inner: PropertyMapInner,
}

// The inline variant is intentionally large — eliminating the heap allocation
// for small objects is the entire point of issue #68. Boxing it would defeat
// the optimisation.
// Keys are `JsPropertyKey` (issue #317): the same interned backing bytes are shared with
// `JsObjectData.property_order`, so the two stored copies of a property name
// share one allocation. Its WTF-8 bytes preserve arbitrary UTF-16 keys, and
// ordinary `&str` lookups borrow their UTF-8 bytes directly.
#[allow(clippy::large_enum_variant)]
enum PropertyMapInner {
    Inline(SmallVec<[(JsPropertyKey, PropertyDescriptor); INLINE_CAP]>),
    Spilled {
        entries: Vec<(JsPropertyKey, PropertyDescriptor)>,
        index: HashMap<JsPropertyKey, u32>,
    },
}

impl PropertyMap {
    pub(crate) fn new() -> Self {
        Self {
            inner: PropertyMapInner::Inline(SmallVec::new()),
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
                .position(|(k, _)| k.as_bytes() == key.as_property_key_bytes()),
            PropertyMapInner::Spilled { index, .. } => index
                .get(key.as_property_key_bytes())
                .map(|&slot| slot as usize),
        }
    }

    /// The entry at `slot`, if any. A remembered slot must be checked against
    /// the returned key before use.
    #[inline]
    pub(crate) fn entry_at(&self, slot: usize) -> Option<(&JsPropertyKey, &PropertyDescriptor)> {
        let entry = match &self.inner {
            PropertyMapInner::Inline(v) => v.get(slot),
            PropertyMapInner::Spilled { entries, .. } => entries.get(slot),
        };
        entry.map(|(k, d)| (k, d))
    }

    #[inline]
    pub(crate) fn entry_at_mut(&mut self, slot: usize) -> Option<&mut PropertyDescriptor> {
        let entry = match &mut self.inner {
            PropertyMapInner::Inline(v) => v.get_mut(slot),
            PropertyMapInner::Spilled { entries, .. } => entries.get_mut(slot),
        };
        entry.map(|(_, d)| d)
    }

    pub(crate) fn get<K: PropertyKeyLike + ?Sized>(&self, key: &K) -> Option<&PropertyDescriptor> {
        self.slot_of(key)
            .and_then(|slot| self.entry_at(slot))
            .map(|(_, d)| d)
    }

    pub(crate) fn get_mut<K: PropertyKeyLike + ?Sized>(
        &mut self,
        key: &K,
    ) -> Option<&mut PropertyDescriptor> {
        let slot = self.slot_of(key)?;
        self.entry_at_mut(slot)
    }

    pub(crate) fn insert(
        &mut self,
        key: JsPropertyKey,
        value: PropertyDescriptor,
    ) -> Option<PropertyDescriptor> {
        match &mut self.inner {
            PropertyMapInner::Inline(v) => {
                if let Some(slot) = v.iter_mut().find(|(k, _)| *k == key) {
                    return Some(std::mem::replace(&mut slot.1, value));
                }
                if v.len() < LINEAR_MAX {
                    v.push((key, value));
                    None
                } else {
                    self.inner = Self::spill(v, key, value);
                    None
                }
            }
            PropertyMapInner::Spilled { entries, index } => {
                if let Some(&slot) = index.get(key.as_bytes()) {
                    return Some(std::mem::replace(&mut entries[slot as usize].1, value));
                }
                index.insert(key.clone(), entries.len() as u32);
                entries.push((key, value));
                None
            }
        }
    }

    /// Append an entry whose key the caller knows is absent. Skips the
    /// duplicate scan of [`insert`]; used on the fresh-object construction
    /// path (issue #901). Debug builds assert the key really is absent.
    pub(crate) fn push_fresh(&mut self, key: JsPropertyKey, value: PropertyDescriptor) {
        debug_assert!(
            !self.contains_key(&key),
            "push_fresh called with a key that is already present"
        );
        match &mut self.inner {
            PropertyMapInner::Inline(v) if v.len() < LINEAR_MAX => v.push((key, value)),
            PropertyMapInner::Inline(v) => {
                self.inner = Self::spill(v, key, value);
            }
            PropertyMapInner::Spilled { entries, index } => {
                index.insert(key.clone(), entries.len() as u32);
                entries.push((key, value));
            }
        }
    }

    fn spill(
        v: &mut SmallVec<[(JsPropertyKey, PropertyDescriptor); INLINE_CAP]>,
        key: JsPropertyKey,
        value: PropertyDescriptor,
    ) -> PropertyMapInner {
        let mut entries: Vec<(JsPropertyKey, PropertyDescriptor)> =
            Vec::with_capacity(LINEAR_MAX + 1);
        let mut index: HashMap<JsPropertyKey, u32> = HashMap::with_capacity(LINEAR_MAX + 1);
        for (k, d) in v.drain(..) {
            index.insert(k.clone(), entries.len() as u32);
            entries.push((k, d));
        }
        index.insert(key.clone(), entries.len() as u32);
        entries.push((key, value));
        PropertyMapInner::Spilled { entries, index }
    }

    pub(crate) fn remove<K: PropertyKeyLike + ?Sized>(
        &mut self,
        key: &K,
    ) -> Option<PropertyDescriptor> {
        match &mut self.inner {
            PropertyMapInner::Inline(v) => {
                let pos = v
                    .iter()
                    .position(|(k, _)| k.as_bytes() == key.as_property_key_bytes())?;
                Some(v.remove(pos).1)
            }
            PropertyMapInner::Spilled { entries, index } => {
                let slot = index.remove(key.as_property_key_bytes())? as usize;
                let (_, removed) = entries.swap_remove(slot);
                if let Some((moved_key, _)) = entries.get(slot)
                    && let Some(moved_slot) = index.get_mut(moved_key.as_bytes())
                {
                    *moved_slot = slot as u32;
                }
                Some(removed)
            }
        }
    }

    pub(crate) fn iter(&self) -> PropertyMapIter<'_> {
        let inner = match &self.inner {
            PropertyMapInner::Inline(v) => v.iter(),
            PropertyMapInner::Spilled { entries, .. } => entries.iter(),
        };
        PropertyMapIter { inner }
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = &JsPropertyKey> {
        self.iter().map(|(k, _)| k)
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &PropertyDescriptor> {
        self.iter().map(|(_, v)| v)
    }
}

impl Default for PropertyMap {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) struct PropertyMapIter<'a> {
    inner: std::slice::Iter<'a, (JsPropertyKey, PropertyDescriptor)>,
}

impl<'a> Iterator for PropertyMapIter<'a> {
    type Item = (&'a JsPropertyKey, &'a PropertyDescriptor);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(k, v)| (k, v))
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

    fn num(d: &PropertyDescriptor) -> f64 {
        d.value
            .as_ref()
            .and_then(JsValue::as_number)
            .unwrap_or(f64::NAN)
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
        assert_eq!(num(&prev), 0.0);
        assert!(is_inline(&m));
        assert_eq!(num(m.get("k0").expect("present")), 99.0);
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
                let n = d
                    .value
                    .as_ref()
                    .and_then(JsValue::as_number)
                    .unwrap_or(-1.0);
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
                let n = d
                    .value
                    .as_ref()
                    .and_then(JsValue::as_number)
                    .unwrap_or(-1.0);
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
            d.value = Some(JsValue::number(7.0));
        }
        assert_eq!(num(m.get("a").expect("present")), 7.0);

        for i in 0..LINEAR_MAX {
            m.insert(key(format!("k{i}")), desc(i as f64));
        }
        assert!(!is_inline(&m));
        if let Some(d) = m.get_mut("a") {
            d.value = Some(JsValue::number(42.0));
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
    fn js_object_data_size_under_3kb() {
        // Tripwire: catch surprise growth of JsObjectData. If this fires,
        // either trim a field or lower LINEAR_MAX. Keep tight enough to
        // catch regressions but loose enough to absorb routine field adds.
        let size = std::mem::size_of::<JsObjectData>();
        assert!(size <= 3072, "JsObjectData grew to {size} bytes (cap 3072)");
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
        m.entry_at_mut(slot).unwrap().value = Some(JsValue::number(99.0));
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
        // The last entry was swap-moved into the hole: the stale slot now
        // holds a different key, which a verifying caller must notice.
        let (k, _) = m.entry_at(stale).expect("slot reused by moved entry");
        assert_ne!(k.as_bytes(), b"k2");
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
