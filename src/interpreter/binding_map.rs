//! Atom-keyed binding storage for `Environment` (issue #898).
//!
//! Replaces `HashMap<String, Binding>`. Entries live in an insertion-ordered
//! `Vec<(Atom, Binding)>`. Small scopes (the overwhelmingly common case for
//! function calls) are probed by a linear scan: pointer equality for an `Atom`
//! key, length + `memcmp` for a `&str` key. Scopes that outgrow
//! [`LINEAR_MAX`] additionally maintain an index from the name's *randomised*
//! 64-bit hash (`atom::hash_text`, the same keyed hash every atom stores) to
//! its slot. An `Atom` probe reuses the stored hash, a `&str` probe hashes the
//! text once with that keyed hash, so script-controlled names are never hashed
//! by a predictable function and the index itself only ever sees uniformly
//! random `u64`s (identity-hashed).
//!
//! Two different names can share a 64-bit hash (vanishingly unlikely, but it
//! must stay correct): the loser of the `by_hash` slot is recorded in
//! `overflow` and found by a short scan.

use std::collections::HashMap;

use super::atom::{Atom, IdentityBuild, hash_text};
use super::types::Binding;

/// Entries up to this count are found by scanning; above it an index is kept.
pub(crate) const LINEAR_MAX: usize = 16;

struct HashIndex {
    by_hash: HashMap<u64, u32, IdentityBuild>,
    /// Slots whose hash is already claimed in `by_hash` by another name.
    overflow: Vec<u32>,
}

pub(crate) struct BindingMap {
    entries: Vec<(Atom, Binding)>,
    index: Option<HashIndex>,
    /// One bit per name ever pushed, selected by the top six bits of the
    /// atom's keyed hash. An atom whose bit is clear is definitely absent, so
    /// the (common) free-variable miss on a scope costs one `and`, not a scan.
    /// Removals leave their bit set: the filter may only err towards "maybe".
    name_filter: u64,
    /// Stamp of the key set while `index` exists, 0 otherwise. Every change to
    /// an indexed map's key set takes a fresh value from a per-thread counter,
    /// so two maps (or two states of one map) never share a stamp, and a
    /// reader that probed for a name once can later prove it is still absent
    /// by comparing stamps instead of probing again.
    epoch: u64,
}

thread_local! {
    static NEXT_EPOCH: std::cell::Cell<u64> = const { std::cell::Cell::new(1) };
}

fn fresh_epoch() -> u64 {
    NEXT_EPOCH.with(|next| {
        let epoch = next.get();
        next.set(epoch + 1);
        epoch
    })
}

#[inline]
fn filter_bit(hash: u64) -> u64 {
    1u64 << (hash >> 58)
}

/// A lookup key: either script text (`str`) or an already-interned `Atom`.
pub(crate) trait BindingKey {
    fn find_in(&self, map: &BindingMap) -> Option<usize>;
}

impl BindingKey for str {
    #[inline]
    fn find_in(&self, map: &BindingMap) -> Option<usize> {
        match &map.index {
            None => map
                .entries
                .iter()
                .position(|(a, _)| a.as_str().len() == self.len() && a.as_str() == self),
            Some(index) => {
                #[cfg(feature = "perf-counters")]
                super::atom::count_text_probe();
                map.find_hashed(index, hash_text(self), |a| a.as_str() == self)
            }
        }
    }
}

impl BindingKey for String {
    #[inline]
    fn find_in(&self, map: &BindingMap) -> Option<usize> {
        self.as_str().find_in(map)
    }
}

impl BindingKey for Atom {
    #[inline]
    fn find_in(&self, map: &BindingMap) -> Option<usize> {
        match &map.index {
            None => {
                if map.name_filter & filter_bit(self.precomputed_hash()) == 0 {
                    return None;
                }
                map.entries.iter().position(|(a, _)| a.ptr_eq(self))
            }
            Some(index) => map.find_hashed(index, self.precomputed_hash(), |a| a.ptr_eq(self)),
        }
    }
}

impl<T: BindingKey + ?Sized> BindingKey for &T {
    #[inline]
    fn find_in(&self, map: &BindingMap) -> Option<usize> {
        (**self).find_in(map)
    }
}

impl BindingKey for std::rc::Rc<str> {
    #[inline]
    fn find_in(&self, map: &BindingMap) -> Option<usize> {
        (**self).find_in(map)
    }
}

/// A [`BindingKey`] that is also script text: it can be shown in messages,
/// handed to property operations, and inserted as a new binding.
pub(crate) trait BindingName: BindingKey {
    fn as_name(&self) -> &str;
    /// The key as an interned atom, when it is one.
    #[inline]
    fn as_atom(&self) -> Option<&Atom> {
        None
    }
    fn insert_into(&self, map: &mut BindingMap, binding: Binding) -> Option<Binding>;
}

impl BindingName for str {
    #[inline]
    fn as_name(&self) -> &str {
        self
    }
    fn insert_into(&self, map: &mut BindingMap, binding: Binding) -> Option<Binding> {
        map.insert(self, binding)
    }
}

impl BindingName for String {
    #[inline]
    fn as_name(&self) -> &str {
        self
    }
    fn insert_into(&self, map: &mut BindingMap, binding: Binding) -> Option<Binding> {
        map.insert(self, binding)
    }
}

impl BindingName for Atom {
    #[inline]
    fn as_name(&self) -> &str {
        self.as_str()
    }
    #[inline]
    fn as_atom(&self) -> Option<&Atom> {
        Some(self)
    }
    fn insert_into(&self, map: &mut BindingMap, binding: Binding) -> Option<Binding> {
        map.insert_atom(self, binding)
    }
}

impl<T: BindingName + ?Sized> BindingName for &T {
    #[inline]
    fn as_name(&self) -> &str {
        (**self).as_name()
    }
    #[inline]
    fn as_atom(&self) -> Option<&Atom> {
        (**self).as_atom()
    }
    fn insert_into(&self, map: &mut BindingMap, binding: Binding) -> Option<Binding> {
        (**self).insert_into(map, binding)
    }
}

impl HashIndex {
    fn add(&mut self, hash: u64, slot: u32) {
        match self.by_hash.entry(hash) {
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(slot);
            }
            std::collections::hash_map::Entry::Occupied(_) => self.overflow.push(slot),
        }
    }
}

impl BindingMap {
    pub(crate) fn new() -> Self {
        BindingMap {
            entries: Vec::new(),
            index: None,
            name_filter: 0,
            epoch: 0,
        }
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        BindingMap {
            entries: Vec::with_capacity(capacity),
            index: None,
            name_filter: 0,
            epoch: 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn capacity(&self) -> usize {
        self.entries.capacity()
    }

    pub(crate) fn reserve(&mut self, additional: usize) {
        self.entries.reserve(additional);
    }

    /// Drops every binding but keeps the entry buffer for pooled reuse.
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.index = None;
        self.name_filter = 0;
        self.epoch = 0;
    }

    #[inline]
    fn find_hashed(
        &self,
        index: &HashIndex,
        hash: u64,
        matches: impl Fn(&Atom) -> bool,
    ) -> Option<usize> {
        if let Some(&slot) = index.by_hash.get(&hash)
            && matches(&self.entries[slot as usize].0)
        {
            return Some(slot as usize);
        }
        index
            .overflow
            .iter()
            .map(|&slot| slot as usize)
            .find(|&slot| matches(&self.entries[slot].0))
    }

    #[inline]
    pub(crate) fn position<K: BindingKey + ?Sized>(&self, key: &K) -> Option<usize> {
        key.find_in(self)
    }

    /// The key-set stamp of an indexed map (see the `epoch` field), or 0 for a
    /// map that is scanned linearly.
    #[inline(always)]
    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Whether `name` might be bound here: `false` is definite (one `and` on
    /// the name filter, no probe), `true` only means the filter bit is set.
    #[inline(always)]
    pub(crate) fn may_contain(&self, name: &Atom) -> bool {
        self.name_filter & filter_bit(name.precomputed_hash()) != 0
    }

    #[inline]
    pub(crate) fn get<K: BindingKey + ?Sized>(&self, key: &K) -> Option<&Binding> {
        key.find_in(self).map(|i| &self.entries[i].1)
    }

    #[inline]
    pub(crate) fn get_mut<K: BindingKey + ?Sized>(&mut self, key: &K) -> Option<&mut Binding> {
        key.find_in(self).map(|i| &mut self.entries[i].1)
    }

    #[inline]
    pub(crate) fn contains_key<K: BindingKey + ?Sized>(&self, key: &K) -> bool {
        key.find_in(self).is_some()
    }

    /// The `i`-th entry in insertion order, if any.
    #[inline]
    pub(crate) fn entry_at(&self, i: usize) -> Option<&(Atom, Binding)> {
        self.entries.get(i)
    }

    #[inline]
    pub(crate) fn entry_at_mut(&mut self, i: usize) -> Option<&mut (Atom, Binding)> {
        self.entries.get_mut(i)
    }

    /// Insert-or-replace by script text. Replacement keeps the entry position.
    pub(crate) fn insert(&mut self, name: &str, binding: Binding) -> Option<Binding> {
        if let Some(i) = name.find_in(self) {
            return Some(std::mem::replace(&mut self.entries[i].1, binding));
        }
        self.push_unique(Atom::intern(name), binding);
        None
    }

    /// Insert-or-replace by atom.
    pub(crate) fn insert_atom(&mut self, name: &Atom, binding: Binding) -> Option<Binding> {
        if let Some(i) = name.find_in(self) {
            return Some(std::mem::replace(&mut self.entries[i].1, binding));
        }
        self.push_unique(name.clone(), binding);
        None
    }

    /// Append an entry the caller knows is absent.
    pub(crate) fn push_unique(&mut self, name: Atom, binding: Binding) {
        debug_assert!(name.find_in(self).is_none(), "push_unique of a bound name");
        let slot = self.entries.len() as u32;
        let hash = name.precomputed_hash();
        self.name_filter |= filter_bit(hash);
        self.entries.push((name, binding));
        if let Some(index) = &mut self.index {
            index.add(hash, slot);
            self.epoch = fresh_epoch();
        } else if self.entries.len() > LINEAR_MAX {
            self.build_index();
        }
    }

    pub(crate) fn remove<K: BindingKey + ?Sized>(&mut self, key: &K) -> Option<Binding> {
        let i = key.find_in(self)?;
        let removed_hash = self.entries[i].0.precomputed_hash();
        let (_, removed) = self.entries.swap_remove(i);
        let Some(index) = &mut self.index else {
            return Some(removed);
        };
        self.epoch = fresh_epoch();
        if !index.overflow.is_empty() {
            // Hash collisions are rare enough that rebuilding beats keeping
            // promotion bookkeeping for them.
            self.build_index();
            return Some(removed);
        }
        index.by_hash.remove(&removed_hash);
        if let Some((moved, _)) = self.entries.get(i) {
            // `swap_remove` moved the last entry into slot `i`.
            index.by_hash.insert(moved.precomputed_hash(), i as u32);
        }
        Some(removed)
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &Binding> {
        self.entries.iter().map(|(_, b)| b)
    }

    fn build_index(&mut self) {
        let mut index = HashIndex {
            by_hash: HashMap::with_capacity_and_hasher(
                self.entries.len() * 2,
                IdentityBuild::default(),
            ),
            overflow: Vec::new(),
        };
        for (slot, (atom, _)) in self.entries.iter().enumerate() {
            index.add(atom.precomputed_hash(), slot as u32);
        }
        self.index = Some(index);
        self.epoch = fresh_epoch();
    }
}

impl Default for BindingMap {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for BindingMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map()
            .entries(self.entries.iter().map(|(a, b)| (a, b)))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::types::BindingKind;
    use crate::types::JsValue;

    fn b(n: f64) -> Binding {
        Binding::new(JsValue::number(n), BindingKind::Var, true)
    }

    fn val(map: &BindingMap, name: &str) -> Option<f64> {
        map.get(name).and_then(|b| b.value.as_number())
    }

    #[test]
    fn insert_replaces_in_place_and_keeps_position() {
        let mut m = BindingMap::new();
        assert!(m.insert("a", b(1.0)).is_none());
        assert!(m.insert("b", b(2.0)).is_none());
        let old = m.insert("a", b(3.0)).expect("replaced");
        assert_eq!(old.value.as_number(), Some(1.0));
        assert_eq!(m.len(), 2);
        assert_eq!(m.entry_at(0).map(|(a, _)| a.as_str()), Some("a"));
        assert_eq!(val(&m, "a"), Some(3.0));
    }

    #[test]
    fn str_and_atom_keys_agree() {
        let mut m = BindingMap::new();
        m.insert("x", b(1.0));
        let atom = Atom::intern("x");
        assert!(m.contains_key(&atom));
        assert!(m.contains_key("x"));
        assert!(!m.contains_key("y"));
        assert!(!m.contains_key(&Atom::intern("y")));
        m.get_mut(&atom).expect("bound").value = JsValue::number(9.0);
        assert_eq!(val(&m, "x"), Some(9.0));
        assert!(m.remove("x").is_some());
        assert!(m.remove(&atom).is_none());
        assert!(m.is_empty());
    }

    #[test]
    fn behaves_identically_across_the_linear_threshold() {
        let mut m = BindingMap::new();
        let names: Vec<String> = (0..100).map(|i| format!("name{i}")).collect();
        for (i, n) in names.iter().enumerate() {
            m.insert(n, b(i as f64));
            for (j, earlier) in names[..=i].iter().enumerate() {
                assert_eq!(val(&m, earlier), Some(j as f64));
                assert_eq!(
                    m.get(&Atom::intern(earlier))
                        .and_then(|b| b.value.as_number()),
                    Some(j as f64)
                );
            }
            assert!(!m.contains_key("absent"));
        }
        assert_eq!(m.len(), 100);
        assert_eq!(m.values().count(), 100);
    }

    #[test]
    fn remove_fixes_up_the_moved_entry_in_scan_and_index_modes() {
        for count in [8usize, 64] {
            let mut m = BindingMap::new();
            for i in 0..count {
                m.insert(&format!("v{i}"), b(i as f64));
            }
            assert!(m.remove("v0").is_some());
            assert!(m.get("v0").is_none());
            let last = count - 1;
            assert_eq!(val(&m, &format!("v{last}")), Some(last as f64));
            assert!(m.contains_key(&Atom::intern(&format!("v{last}"))));
            for i in 1..count {
                assert_eq!(val(&m, &format!("v{i}")), Some(i as f64));
            }
        }
    }

    #[test]
    fn removing_thousands_of_names_stays_linear() {
        let mut m = BindingMap::new();
        let names: Vec<String> = (0..5_000).map(|i| format!("builtin{i}")).collect();
        for (i, n) in names.iter().enumerate() {
            m.insert(n, b(i as f64));
        }
        let start = std::time::Instant::now();
        for n in &names {
            assert!(m.remove(n.as_str()).is_some());
        }
        assert!(m.is_empty());
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn clear_keeps_capacity() {
        let mut m = BindingMap::with_capacity(8);
        for i in 0..40 {
            m.insert(&format!("n{i}"), b(0.0));
        }
        let cap = m.capacity();
        m.clear();
        assert!(m.is_empty());
        assert!(m.capacity() >= cap);
        assert!(!m.contains_key("n1"));
        m.insert("n1", b(1.0));
        assert_eq!(val(&m, "n1"), Some(1.0));
    }

    #[test]
    fn push_unique_appends_in_order() {
        let mut m = BindingMap::new();
        m.push_unique(Atom::intern("p"), b(1.0));
        m.push_unique(Atom::intern("q"), b(2.0));
        assert_eq!(m.entry_at(1).map(|(a, _)| a.as_str()), Some("q"));
        assert_eq!(m.position("q"), Some(1));
    }

    #[test]
    fn colliding_hashes_stay_distinct_through_insert_and_remove() {
        let mut m = BindingMap::new();
        let atoms: Vec<Atom> = (0..40)
            .map(|i| Atom::with_hash_for_test(&format!("collide{i}"), 7))
            .collect();
        for (i, atom) in atoms.iter().enumerate() {
            m.insert_atom(atom, b(i as f64));
        }
        for (i, atom) in atoms.iter().enumerate() {
            assert_eq!(
                m.get(atom).and_then(|b| b.value.as_number()),
                Some(i as f64)
            );
        }
        assert!(m.remove(&atoms[0]).is_some());
        assert!(m.remove(&atoms[17]).is_some());
        assert!(m.get(&atoms[0]).is_none());
        for (i, atom) in atoms.iter().enumerate() {
            if i != 0 && i != 17 {
                assert_eq!(
                    m.get(atom).and_then(|b| b.value.as_number()),
                    Some(i as f64),
                    "entry {i}"
                );
            }
        }
        assert_eq!(m.len(), 38);
    }
}
