//! Atom-keyed binding storage for `Environment` (issue #898).
//!
//! Replaces `HashMap<String, Binding>`. Entries live in an insertion-ordered
//! `Vec<(Atom, Binding)>`. Small scopes (the overwhelmingly common case for
//! function calls) are probed by a linear scan: pointer equality for an `Atom`
//! key, length + `memcmp` for a `&str` key. Scopes that outgrow
//! [`LINEAR_MAX`] additionally maintain an index keyed by `Atom` whose hash is
//! the interner's precomputed, randomised value, so script-controlled names are
//! never hashed by a predictable hasher (see `atom.rs`).

use std::collections::HashMap;

use super::atom::{Atom, IdentityBuild};
use super::types::Binding;

/// Entries up to this count are found by scanning; above it an index is kept.
pub(crate) const LINEAR_MAX: usize = 16;

pub(crate) struct BindingMap {
    entries: Vec<(Atom, Binding)>,
    index: Option<HashMap<Atom, u32, IdentityBuild>>,
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
                let atom = Atom::lookup(self)?;
                index.get(&atom).map(|&i| i as usize)
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
            None => map.entries.iter().position(|(a, _)| a.ptr_eq(self)),
            Some(index) => index.get(self).map(|&i| i as usize),
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

impl BindingMap {
    pub(crate) fn new() -> Self {
        BindingMap {
            entries: Vec::new(),
            index: None,
        }
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        BindingMap {
            entries: Vec::with_capacity(capacity),
            index: None,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

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
    }

    #[inline]
    pub(crate) fn position<K: BindingKey + ?Sized>(&self, key: &K) -> Option<usize> {
        key.find_in(self)
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
        debug_assert!(!name.find_in(self).is_some(), "push_unique of a bound name");
        let slot = self.entries.len();
        if let Some(index) = &mut self.index {
            index.insert(name.clone(), slot as u32);
        }
        self.entries.push((name, binding));
        if self.index.is_none() && self.entries.len() > LINEAR_MAX {
            self.build_index();
        }
    }

    pub(crate) fn remove<K: BindingKey + ?Sized>(&mut self, key: &K) -> Option<Binding> {
        let i = key.find_in(self)?;
        let (removed_atom, removed) = self.entries.swap_remove(i);
        if let Some(index) = &mut self.index {
            index.remove(&removed_atom);
            if let Some((moved, _)) = self.entries.get(i) {
                index.insert(moved.clone(), i as u32);
            }
        }
        Some(removed)
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &Binding> {
        self.entries.iter().map(|(_, b)| b)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&Atom, &Binding)> {
        self.entries.iter().map(|(a, b)| (a, b))
    }

    fn build_index(&mut self) {
        let mut index =
            HashMap::with_capacity_and_hasher(self.entries.len() * 2, IdentityBuild::default());
        for (i, (atom, _)) in self.entries.iter().enumerate() {
            index.insert(atom.clone(), i as u32);
        }
        self.index = Some(index);
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
}
