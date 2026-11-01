//! The shaped representation of a [`PropertyMap`]: an `Rc<Shape>` for the keys
//! and one record per key, see [`Shape`].

use super::shape::{SHAPED_MAX, Shape, shapes_enabled};
use super::slot_index::{grown_capacity, round_capacity};
use super::*;

pub(super) type Records = SmallVec<[PropertyRecord; INLINE_CAP]>;

#[inline]
fn push_record(records: &mut Records, value: PropertyRecord) {
    let cap = records.capacity();
    if records.len() == cap && cap >= INLINE_CAP {
        records.reserve_exact(grown_capacity(cap) - cap);
    }
    records.push(value);
}

impl PropertyMap {
    /// Adds the first key of an empty map on a shape, if shapes are on. Gives
    /// the record back when the map stays per-object.
    #[inline]
    pub(super) fn begin_shape(
        &mut self,
        key: &JsPropertyKey,
        value: PropertyRecord,
    ) -> Result<(), PropertyRecord> {
        if !shapes_enabled() {
            return Err(value);
        }
        let Some(shape) = Shape::root().with_key(key) else {
            return Err(value);
        };
        let mut records = Records::new();
        records.push(value);
        self.inner = PropertyMapInner::Shaped { shape, records };
        Ok(())
    }

    /// Adds a key the map does not have to a shaped map, moving it to the
    /// per-object representation when its shape cannot take the key.
    #[inline]
    pub(super) fn append_shaped(&mut self, key: JsPropertyKey, value: PropertyRecord) {
        let PropertyMapInner::Shaped { shape, records } = &mut self.inner else {
            return self.append_per_object(key, value);
        };
        match shape.with_key(&key) {
            Some(next) => {
                *shape = next;
                push_record(records, value);
            }
            None => {
                self.leave_shapes();
                self.append_per_object(key, value);
            }
        }
    }

    /// Room for `additional` more records of a shaped map.
    pub(super) fn reserve_shaped(&mut self, additional: usize) {
        let PropertyMapInner::Shaped { shape, records } = &mut self.inner else {
            return;
        };
        if shape.len() + additional > SHAPED_MAX {
            self.leave_shapes();
            self.reserve(additional);
        } else {
            records.reserve_exact(additional);
        }
    }

    /// An empty map that will take `additional` keys, as a shaped one.
    pub(super) fn begin_shape_with_room(&mut self, additional: usize) {
        let mut records = Records::new();
        records.reserve_exact(additional);
        self.inner = PropertyMapInner::Shaped {
            shape: Shape::root(),
            records,
        };
    }

    /// Removes `key`, which is the last one added, by returning to the parent
    /// shape. `None` when `key` is not last (or the map is not shaped), in
    /// which case the caller moves to the per-object representation.
    #[inline]
    pub(super) fn pop_shaped(&mut self, key: &[u8]) -> Option<PropertyRecord> {
        let PropertyMapInner::Shaped { shape, records } = &mut self.inner else {
            return None;
        };
        let slot = shape.find(key)?;
        if slot + 1 != records.len() {
            return None;
        }
        *shape = shape.parent()?.clone();
        records.pop()
    }

    /// Converts a shaped map to the per-object representation.
    pub(super) fn leave_shapes(&mut self) {
        let PropertyMapInner::Shaped { shape, records } = &mut self.inner else {
            return;
        };
        let len = records.len();
        let pairs = shape.keys().iter().cloned().zip(records.drain(..));
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
