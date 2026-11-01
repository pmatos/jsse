//! The shaped representation of a [`PropertyMap`]: an `Rc<Shape>` for the keys
//! and attributes and one value per key, see [`Shape`].

use super::shape::{SHAPED_MAX, Shape, shapes_enabled};
use super::slot_index::round_capacity;
use super::*;

/// Values a shaped map holds inline: with its `Rc` the same size as the
/// spilled variant of the map.
const VALUES_INLINE: usize = 4;

pub(super) type Values = SmallVec<[JsValue; VALUES_INLINE]>;

/// `n` values rounded up to a count whose byte size is a size class of
/// jemalloc (16 bytes apart up to 128, then 32, 64, 128): the 16-byte
/// classes are even counts, and `round_capacity` rounds the rest.
fn round_values(n: usize) -> usize {
    round_capacity(n.next_multiple_of(2))
}

#[inline]
fn push_value(values: &mut Values, value: JsValue) {
    let cap = values.capacity();
    if values.len() == cap && cap >= VALUES_INLINE {
        values.reserve_exact(round_values(cap + cap / 4 + 1) - cap);
    }
    values.push(value);
}

impl PropertyMap {
    /// Adds the first key of an empty map on a shape, if shapes are on and
    /// the property is a complete data property. Gives the record back
    /// otherwise.
    #[inline]
    pub(super) fn begin_shape(
        &mut self,
        key: &JsPropertyKey,
        record: PropertyRecord,
    ) -> Result<(), PropertyRecord> {
        if !shapes_enabled() {
            return Err(record);
        }
        let (attrs, value) = record.into_plain()?;
        let Some(shape) = Shape::root().with_key(key, attrs) else {
            return Err(PropertyRecord::from_plain(attrs, value));
        };
        let mut values = Values::new();
        values.push(value);
        self.inner = PropertyMapInner::Shaped { shape, values };
        Ok(())
    }

    /// Adds a key the map does not have to a shaped map, moving it to the
    /// per-object representation when its shape cannot take the key.
    #[inline]
    pub(super) fn append_shaped(&mut self, key: JsPropertyKey, record: PropertyRecord) {
        let PropertyMapInner::Shaped { shape, values } = &mut self.inner else {
            return self.append_per_object(key, record);
        };
        let record = match record.into_plain() {
            Ok((attrs, value)) => match shape.with_key(&key, attrs) {
                Some(next) => {
                    *shape = next;
                    push_value(values, value);
                    return;
                }
                None => PropertyRecord::from_plain(attrs, value),
            },
            Err(record) => record,
        };
        self.leave_shapes();
        self.append_per_object(key, record);
    }

    /// Room for `additional` more values of a shaped map.
    pub(super) fn reserve_shaped(&mut self, additional: usize) {
        let PropertyMapInner::Shaped { shape, values } = &mut self.inner else {
            return;
        };
        if shape.len() + additional > SHAPED_MAX {
            self.leave_shapes();
            self.reserve(additional);
        } else {
            values.reserve_exact(additional);
        }
    }

    /// An empty map that will take `additional` keys, as a shaped one.
    pub(super) fn begin_shape_with_room(&mut self, additional: usize) {
        let mut values = Values::new();
        values.reserve_exact(additional);
        self.inner = PropertyMapInner::Shaped {
            shape: Shape::root(),
            values,
        };
    }

    /// Replaces the property in `slot` of a shaped map. The map stays shaped
    /// when the new property is a complete data property with the attributes
    /// of the old one.
    pub(super) fn replace_shaped(&mut self, slot: usize, record: PropertyRecord) -> PropertyRecord {
        let record = match &mut self.inner {
            PropertyMapInner::Shaped { shape, values } => match record.into_plain() {
                Ok((attrs, new)) if attrs == shape.attrs()[slot] => {
                    let old = std::mem::replace(&mut values[slot], new);
                    return PropertyRecord::from_plain(attrs, old);
                }
                Ok((attrs, new)) => PropertyRecord::from_plain(attrs, new),
                Err(record) => record,
            },
            _ => record,
        };
        self.leave_shapes();
        match &mut self.inner {
            PropertyMapInner::Inline(v) => std::mem::replace(&mut v[slot].1, record),
            PropertyMapInner::Spilled { entries, .. } => {
                let entry = entries[slot]
                    .as_mut()
                    .expect("slot of a shaped key is live");
                std::mem::replace(&mut entry.1, record)
            }
            PropertyMapInner::Shaped { .. } => unreachable!("left shapes above"),
        }
    }

    /// Removes `key`, which is the last one added, by returning to the parent
    /// shape. `None` when `key` is not last (or the map is not shaped), in
    /// which case the caller moves to the per-object representation.
    #[inline]
    pub(super) fn pop_shaped(&mut self, key: &[u8]) -> Option<PropertyRecord> {
        let PropertyMapInner::Shaped { shape, values } = &mut self.inner else {
            return None;
        };
        let slot = shape.find(key)?;
        if slot + 1 != values.len() {
            return None;
        }
        let attrs = shape.attrs()[slot];
        *shape = shape.parent()?.clone();
        Some(PropertyRecord::from_plain(attrs, values.pop()?))
    }

    /// Converts a shaped map to the per-object representation.
    pub(super) fn leave_shapes(&mut self) {
        let PropertyMapInner::Shaped { shape, values } = &mut self.inner else {
            return;
        };
        let len = values.len();
        let pairs = shape
            .keys()
            .iter()
            .cloned()
            .zip(shape.attrs().iter().zip(values.drain(..)))
            .map(|(key, (&attrs, value))| (key, PropertyRecord::from_plain(attrs, value)));
        let inner = if len <= LINEAR_MAX {
            PropertyMapInner::Inline(pairs.collect())
        } else {
            let mut entries = Vec::with_capacity(round_capacity(len + 1));
            entries.extend(pairs.map(Some));
            Self::spilled(entries, len + 1)
        };
        self.inner = inner;
    }
}

/// The creation-order iterator of a shaped map.
pub(crate) struct ShapedIter<'a> {
    keys: std::slice::Iter<'a, JsPropertyKey>,
    attrs: std::slice::Iter<'a, u8>,
    values: std::slice::Iter<'a, JsValue>,
}

impl<'a> ShapedIter<'a> {
    pub(super) fn new(shape: &'a Shape, values: &'a [JsValue]) -> Self {
        Self {
            keys: shape.keys().iter(),
            attrs: shape.attrs().iter(),
            values: values.iter(),
        }
    }
}

impl<'a> Iterator for ShapedIter<'a> {
    type Item = (&'a JsPropertyKey, RecordRef<'a>);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        Some((
            self.keys.next()?,
            RecordRef::Plain {
                attrs: *self.attrs.next()?,
                value: self.values.next()?,
            },
        ))
    }
}
