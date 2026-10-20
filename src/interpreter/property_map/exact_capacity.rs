//! Exact-size construction of the small maps function objects start with.

use super::*;

impl PropertyMap {
    /// An empty map that will hold exactly `total` entries (at most
    /// `LINEAR_MAX`), filled by `push_non_index_record` without growing.
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
    #[inline]
    pub(crate) fn push_non_index_record(&mut self, key: JsPropertyKey, record: PropertyRecord) {
        debug_assert!(!self.contains_key(&key) && !Self::is_digit_leading(key.as_bytes()));
        debug_assert!(self.len() < LINEAR_MAX);
        if let PropertyMapInner::Inline(entries) = &mut self.inner {
            entries.push((key, record));
        } else {
            self.append(key, record);
        }
    }
}

impl PropertyRecord {
    /// The record of `PropertyDescriptor::data(value, writable, enumerable,
    /// configurable)`, built without the per-field `Option` packing.
    #[inline(always)]
    pub(crate) fn data(
        value: JsValue,
        writable: bool,
        enumerable: bool,
        configurable: bool,
    ) -> Self {
        Self {
            value,
            get: JsValue::UNDEFINED,
            set: JsValue::UNDEFINED,
            flags: HAS_VALUE
                | HAS_WRITABLE
                | HAS_ENUMERABLE
                | HAS_CONFIGURABLE
                | if writable { IS_WRITABLE } else { 0 }
                | if enumerable { IS_ENUMERABLE } else { 0 }
                | if configurable { IS_CONFIGURABLE } else { 0 },
        }
    }

    /// The record of `PropertyDescriptor::accessor(Some(getter), None,
    /// enumerable, configurable)`.
    #[inline(always)]
    pub(crate) fn getter_only(getter: JsValue, enumerable: bool, configurable: bool) -> Self {
        Self {
            value: JsValue::UNDEFINED,
            get: getter,
            set: JsValue::UNDEFINED,
            flags: HAS_GET
                | HAS_ENUMERABLE
                | HAS_CONFIGURABLE
                | if enumerable { IS_ENUMERABLE } else { 0 }
                | if configurable { IS_CONFIGURABLE } else { 0 },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::helpers::same_value;

    fn same_record(direct: &PropertyRecord, packed: &PropertyRecord) -> bool {
        direct.flags == packed.flags
            && same_value(&direct.value, &packed.value)
            && same_value(&direct.get, &packed.get)
            && same_value(&direct.set, &packed.set)
    }

    #[test]
    fn data_record_matches_the_packed_descriptor() {
        for bits in 0..8u8 {
            let (w, e, c) = (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0);
            let direct = PropertyRecord::data(JsValue::number(3.0), w, e, c);
            let packed =
                PropertyRecord::from(PropertyDescriptor::data(JsValue::number(3.0), w, e, c));
            assert!(same_record(&direct, &packed), "data({w}, {e}, {c})");
        }
    }

    #[test]
    fn getter_record_matches_the_packed_descriptor() {
        for bits in 0..4u8 {
            let (e, c) = (bits & 1 != 0, bits & 2 != 0);
            let getter = JsValue::number(7.0);
            let direct = PropertyRecord::getter_only(getter.clone(), e, c);
            let packed =
                PropertyRecord::from(PropertyDescriptor::accessor(Some(getter), None, e, c));
            assert!(same_record(&direct, &packed), "getter({e}, {c})");
        }
    }
}
