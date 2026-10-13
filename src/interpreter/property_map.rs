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

// A spilled map compacts once tombstones outnumber live entries by this much
// (and the entry list is long enough for the rebuild to matter).
const COMPACT_MIN_ENTRIES: usize = 32;

type Entry = (JsPropertyKey, PropertyDescriptor);

// Property names come from script, so the index is keyed with foldhash's
// randomly seeded state (minimally DoS-resistant, unlike the SipHash kept for
// identifier maps by #393); see #911.
type IndexState = foldhash::fast::RandomState;

// Hybrid storage for `JsObjectData.properties`, in property creation order
// (the order [[OwnPropertyKeys]] reports String and Symbol keys in): up to
// `LINEAR_MAX` entries in a linear-scan `SmallVec` (the first `INLINE_CAP`
// without a heap alloc), then a dense entry list plus a randomised key->slot
// index once that overflows. Spill is one-way.
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
        index: HashMap<JsPropertyKey, u32, IndexState>,
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
            PropertyMapInner::Spilled { entries, .. } => entries.get(slot)?.as_ref(),
        };
        entry.map(|(k, d)| (k, d))
    }

    #[inline]
    pub(crate) fn entry_at_mut(&mut self, slot: usize) -> Option<&mut PropertyDescriptor> {
        let entry = match &mut self.inner {
            PropertyMapInner::Inline(v) => v.get_mut(slot),
            PropertyMapInner::Spilled { entries, .. } => entries.get_mut(slot)?.as_mut(),
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

    /// Insert or replace `key`. A new key is appended in creation order; an
    /// existing key keeps its position.
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
            }
            PropertyMapInner::Spilled {
                entries,
                index,
                live,
            } => {
                match index.entry(key.clone()) {
                    std::collections::hash_map::Entry::Occupied(e) => {
                        let entry = entries[*e.get() as usize]
                            .as_mut()
                            .expect("indexed slot is live");
                        return Some(std::mem::replace(&mut entry.1, value));
                    }
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert(entries.len() as u32);
                    }
                }
                self.digit_leading_keys += u32::from(Self::is_digit_leading(key.as_bytes()));
                entries.push(Some((key, value)));
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
        self.append(key, value);
    }

    #[inline]
    fn append(&mut self, key: JsPropertyKey, value: PropertyDescriptor) {
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
                index.insert(key.clone(), entries.len() as u32);
                entries.push(Some((key, value)));
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
                let mut index: HashMap<JsPropertyKey, u32, IndexState> =
                    HashMap::with_capacity_and_hasher(total, IndexState::default());
                for (k, d) in v.drain(..) {
                    index.insert(k.clone(), entries.len() as u32);
                    entries.push(Some((k, d)));
                }
                let live = entries.len();
                self.inner = PropertyMapInner::Spilled {
                    entries,
                    index,
                    live,
                };
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
        value: PropertyDescriptor,
    ) -> PropertyMapInner {
        let mut entries: Vec<Option<Entry>> = Vec::with_capacity(LINEAR_MAX + 1);
        let mut index: HashMap<JsPropertyKey, u32, IndexState> =
            HashMap::with_capacity_and_hasher(LINEAR_MAX + 1, IndexState::default());
        for (k, d) in v.drain(..) {
            index.insert(k.clone(), entries.len() as u32);
            entries.push(Some((k, d)));
        }
        index.insert(key.clone(), entries.len() as u32);
        entries.push(Some((key, value)));
        let live = entries.len();
        PropertyMapInner::Spilled {
            entries,
            index,
            live,
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
                    .position(|(k, _)| k.as_bytes() == key.as_property_key_bytes())?;
                v.remove(pos).1
            }
            PropertyMapInner::Spilled {
                entries,
                index,
                live,
            } => {
                let slot = index.remove(key.as_property_key_bytes())? as usize;
                let (_, removed) = entries[slot].take()?;
                *live -= 1;
                if slot + 1 == entries.len() {
                    while matches!(entries.last(), Some(None)) {
                        entries.pop();
                    }
                } else if entries.len() >= COMPACT_MIN_ENTRIES && entries.len() > 2 * *live {
                    let mut remap = Vec::with_capacity(entries.len());
                    let mut next = 0u32;
                    for entry in entries.iter() {
                        remap.push(next);
                        next += u32::from(entry.is_some());
                    }
                    entries.retain(Option::is_some);
                    for slot in index.values_mut() {
                        *slot = remap[*slot as usize];
                    }
                }
                removed
            }
        };
        if Self::is_digit_leading(key.as_property_key_bytes()) {
            self.digit_leading_keys -= 1;
        }
        Some(removed)
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

    pub(crate) fn values(&self) -> impl Iterator<Item = &PropertyDescriptor> {
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
    type Item = (&'a JsPropertyKey, &'a PropertyDescriptor);

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
