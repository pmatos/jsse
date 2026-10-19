//! Exact-size construction of the small maps function objects start with.

use super::*;

impl PropertyMap {
    /// An empty map that will hold exactly `total` entries (at most
    /// `LINEAR_MAX`), filled by `push_non_index` without growing.
    pub(crate) fn with_exact_capacity(total: usize) -> Self {
        assert!(total <= LINEAR_MAX);
        let entries = if total > INLINE_CAP {
            SmallVec::from_vec(Vec::with_capacity(total))
        } else {
            SmallVec::new()
        };
        Self {
            inner: PropertyMapInner::Inline(entries),
            digit_leading_keys: 0,
        }
    }

    /// Append an entry whose key is absent and does not begin with a digit.
    pub(crate) fn push_non_index(&mut self, key: JsPropertyKey, value: PropertyDescriptor) {
        debug_assert!(!self.contains_key(&key) && !Self::is_digit_leading(key.as_bytes()));
        debug_assert!(self.len() < LINEAR_MAX);
        if let PropertyMapInner::Inline(entries) = &mut self.inner {
            entries.push((key, value.into()));
        } else {
            self.push_fresh(key, value);
        }
    }
}
