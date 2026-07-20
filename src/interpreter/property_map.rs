use std::collections::HashMap;

use smallvec::SmallVec;

use super::types::PropertyDescriptor;
use crate::types::{JsPropertyKey, PropertyKeyLike};

// Inline up to this many properties before spilling to a HashMap. 8 was the
// threshold proposed in issue #68; measurement showed it pushes JsObjectData
// past 3.5 KB, so we use 4 — covers the splay-node case (4 fields) without
// blowing up per-object size.
const INLINE_CAP: usize = 4;

// Hybrid storage for `JsObjectData.properties`: inline up to `INLINE_CAP`
// entries (linear scan, no heap alloc), then spill to a randomised `HashMap`
// once the inline buffer overflows. Spill is one-way; deletes never collapse
// back to inline. Iteration order is unspecified — ordered iteration must
// continue to go through `JsObjectData.property_order`.
pub struct PropertyMap {
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
    Spilled(HashMap<JsPropertyKey, PropertyDescriptor>),
}

impl PropertyMap {
    pub fn new() -> Self {
        Self {
            inner: PropertyMapInner::Inline(SmallVec::new()),
        }
    }

    pub fn contains_key<K: PropertyKeyLike + ?Sized>(&self, key: &K) -> bool {
        match &self.inner {
            PropertyMapInner::Inline(v) => v
                .iter()
                .any(|(k, _)| k.as_bytes() == key.as_property_key_bytes()),
            PropertyMapInner::Spilled(m) => m.contains_key(key.as_property_key_bytes()),
        }
    }

    pub fn get<K: PropertyKeyLike + ?Sized>(&self, key: &K) -> Option<&PropertyDescriptor> {
        match &self.inner {
            PropertyMapInner::Inline(v) => v
                .iter()
                .find(|(k, _)| k.as_bytes() == key.as_property_key_bytes())
                .map(|(_, d)| d),
            PropertyMapInner::Spilled(m) => m.get(key.as_property_key_bytes()),
        }
    }

    pub fn get_mut<K: PropertyKeyLike + ?Sized>(
        &mut self,
        key: &K,
    ) -> Option<&mut PropertyDescriptor> {
        match &mut self.inner {
            PropertyMapInner::Inline(v) => v
                .iter_mut()
                .find(|(k, _)| k.as_bytes() == key.as_property_key_bytes())
                .map(|(_, d)| d),
            PropertyMapInner::Spilled(m) => m.get_mut(key.as_property_key_bytes()),
        }
    }

    pub fn insert(
        &mut self,
        key: JsPropertyKey,
        value: PropertyDescriptor,
    ) -> Option<PropertyDescriptor> {
        match &mut self.inner {
            PropertyMapInner::Inline(v) => {
                if let Some(slot) = v.iter_mut().find(|(k, _)| *k == key) {
                    return Some(std::mem::replace(&mut slot.1, value));
                }
                if v.len() < INLINE_CAP {
                    v.push((key, value));
                    None
                } else {
                    // Spill: drain inline entries into a fresh randomised HashMap,
                    // then add the new entry.
                    let mut map: HashMap<JsPropertyKey, PropertyDescriptor> =
                        HashMap::with_capacity(INLINE_CAP + 1);
                    for (k, d) in v.drain(..) {
                        map.insert(k, d);
                    }
                    map.insert(key, value);
                    self.inner = PropertyMapInner::Spilled(map);
                    None
                }
            }
            PropertyMapInner::Spilled(m) => m.insert(key, value),
        }
    }

    pub fn remove<K: PropertyKeyLike + ?Sized>(&mut self, key: &K) -> Option<PropertyDescriptor> {
        match &mut self.inner {
            PropertyMapInner::Inline(v) => {
                let pos = v
                    .iter()
                    .position(|(k, _)| k.as_bytes() == key.as_property_key_bytes())?;
                Some(v.remove(pos).1)
            }
            PropertyMapInner::Spilled(m) => m.remove(key.as_property_key_bytes()),
        }
    }

    pub fn iter(&self) -> PropertyMapIter<'_> {
        let inner = match &self.inner {
            PropertyMapInner::Inline(v) => PropertyMapIterInner::Inline(v.iter()),
            PropertyMapInner::Spilled(m) => PropertyMapIterInner::Spilled(m.iter()),
        };
        PropertyMapIter { inner }
    }

    pub fn keys(&self) -> impl Iterator<Item = &JsPropertyKey> {
        self.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &PropertyDescriptor> {
        self.iter().map(|(_, v)| v)
    }
}

impl Default for PropertyMap {
    fn default() -> Self {
        Self::new()
    }
}

pub struct PropertyMapIter<'a> {
    inner: PropertyMapIterInner<'a>,
}

enum PropertyMapIterInner<'a> {
    Inline(std::slice::Iter<'a, (JsPropertyKey, PropertyDescriptor)>),
    Spilled(std::collections::hash_map::Iter<'a, JsPropertyKey, PropertyDescriptor>),
}

impl<'a> Iterator for PropertyMapIter<'a> {
    type Item = (&'a JsPropertyKey, &'a PropertyDescriptor);

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            PropertyMapIterInner::Inline(it) => it.next().map(|(k, v)| (k, v)),
            PropertyMapIterInner::Spilled(it) => it.next(),
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
        PropertyDescriptor::data_default(JsValue::Number(n))
    }

    fn num(d: &PropertyDescriptor) -> f64 {
        match &d.value {
            Some(JsValue::Number(n)) => *n,
            _ => f64::NAN,
        }
    }

    fn is_inline(map: &PropertyMap) -> bool {
        matches!(map.inner, PropertyMapInner::Inline(_))
    }

    #[test]
    fn inline_insert_and_get() {
        let mut m = PropertyMap::new();
        for i in 0..INLINE_CAP {
            assert!(m.insert(key(format!("k{i}")), desc(i as f64)).is_none());
        }
        assert!(is_inline(&m));
        for i in 0..INLINE_CAP {
            let d = m.get(&format!("k{i}")).expect("present");
            assert_eq!(num(d), i as f64);
        }
        assert!(m.get("missing").is_none());
    }

    #[test]
    fn one_past_capacity_triggers_spill() {
        let mut m = PropertyMap::new();
        for i in 0..INLINE_CAP {
            m.insert(key(format!("k{i}")), desc(i as f64));
        }
        assert!(is_inline(&m));
        m.insert(key(format!("k{INLINE_CAP}")), desc(INLINE_CAP as f64));
        assert!(!is_inline(&m));
        for i in 0..=INLINE_CAP {
            assert!(
                m.get(&format!("k{i}")).is_some(),
                "k{i} missing after spill"
            );
        }
    }

    #[test]
    fn duplicate_insert_returns_previous_and_no_spill() {
        let mut m = PropertyMap::new();
        for i in 0..INLINE_CAP {
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
        for i in 0..=INLINE_CAP {
            m.insert(key(format!("k{i}")), desc(i as f64));
        }
        assert!(!is_inline(&m));
        for i in 0..=INLINE_CAP {
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
                let n = if let Some(JsValue::Number(n)) = d.value {
                    n
                } else {
                    -1.0
                };
                (k.to_string(), n)
            })
            .collect();
        seen.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            seen,
            vec![("k0".into(), 0.0), ("k1".into(), 1.0), ("k2".into(), 2.0)]
        );

        for i in 3..=INLINE_CAP {
            m.insert(key(format!("k{i}")), desc(i as f64));
        }
        assert!(!is_inline(&m));
        let mut seen: Vec<(String, f64)> = m
            .iter()
            .map(|(k, d)| {
                let n = if let Some(JsValue::Number(n)) = d.value {
                    n
                } else {
                    -1.0
                };
                (k.to_string(), n)
            })
            .collect();
        seen.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(seen.len(), INLINE_CAP + 1);
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
            d.value = Some(JsValue::Number(7.0));
        }
        assert_eq!(num(m.get("a").expect("present")), 7.0);

        for i in 0..INLINE_CAP {
            m.insert(key(format!("k{i}")), desc(i as f64));
        }
        assert!(!is_inline(&m));
        if let Some(d) = m.get_mut("a") {
            d.value = Some(JsValue::Number(42.0));
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
        for i in 0..INLINE_CAP {
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
        // either trim a field or lower INLINE_CAP. Keep tight enough to
        // catch regressions but loose enough to absorb routine field adds.
        let size = std::mem::size_of::<JsObjectData>();
        assert!(size <= 3072, "JsObjectData grew to {size} bytes (cap 3072)");
    }
}
