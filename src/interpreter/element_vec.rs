//! `ElementVec`: Array element storage with an O(1) amortised front removal.
//!
//! A `Vec<JsValue>` plus a count of dead slots at the front. Dead slots hold
//! `undefined`, so a collection never sees a stale reference there, and are
//! reclaimed once they outnumber the live elements, which keeps
//! `Array.prototype.shift` on a long queue linear overall instead of moving
//! every remaining element per call. Everything else reads and writes the live
//! window through slice dereferencing.
use super::Interpreter;
use super::types::JsObjectData;
use crate::types::JsValue;
use std::ops::{Deref, DerefMut, Range};

/// Below this many dead slots a compaction is never worth the move.
const MIN_COMPACT_HEAD: usize = 32;

#[derive(Default)]
pub(crate) struct ElementVec {
    buf: Vec<JsValue>,
    head: usize,
}

impl ElementVec {
    #[inline]
    pub(crate) fn push(&mut self, value: JsValue) {
        self.buf.push(value);
    }

    #[inline]
    pub(crate) fn pop(&mut self) -> Option<JsValue> {
        if self.buf.len() == self.head {
            return None;
        }
        let last = self.buf.pop();
        self.reset_if_empty();
        last
    }

    /// Removes and returns the first element; `None` when empty.
    pub(crate) fn pop_front(&mut self) -> Option<JsValue> {
        let slot = self.buf.get_mut(self.head)?;
        let first = std::mem::replace(slot, JsValue::UNDEFINED);
        self.head += 1;
        if self.head == self.buf.len() {
            self.buf.clear();
            self.head = 0;
        } else if self.head >= MIN_COMPACT_HEAD && self.head >= self.buf.len() - self.head {
            self.buf.drain(..self.head);
            self.head = 0;
        }
        Some(first)
    }

    pub(crate) fn extend<I: IntoIterator<Item = JsValue>>(&mut self, iter: I) {
        self.buf.extend(iter);
    }

    pub(crate) fn extend_from_slice(&mut self, other: &[JsValue]) {
        self.buf.extend_from_slice(other);
    }

    pub(crate) fn truncate(&mut self, len: usize) {
        self.buf.truncate(self.head + len);
        self.reset_if_empty();
    }

    pub(crate) fn resize(&mut self, len: usize, value: JsValue) {
        self.buf.resize(self.head + len, value);
    }

    /// Replaces `range` of the live window, returning the removed elements.
    pub(crate) fn splice<I: IntoIterator<Item = JsValue>>(
        &mut self,
        range: Range<usize>,
        replace_with: I,
    ) -> Vec<JsValue> {
        let range = self.head + range.start..self.head + range.end;
        self.buf.splice(range, replace_with).collect()
    }

    pub(crate) fn remove(&mut self, index: usize) -> JsValue {
        if index == 0 && !self.is_empty() {
            return self.pop_front().expect("non-empty");
        }
        self.buf.remove(self.head + index)
    }

    pub(crate) fn into_vec(mut self) -> Vec<JsValue> {
        self.compact();
        self.buf
    }

    /// Moves the live elements out, leaving the vector empty.
    pub(crate) fn take(&mut self) -> Vec<JsValue> {
        std::mem::take(self).into_vec()
    }

    fn compact(&mut self) {
        if self.head != 0 {
            self.buf.drain(..self.head);
            self.head = 0;
        }
    }

    fn reset_if_empty(&mut self) {
        if self.buf.len() == self.head {
            self.buf.clear();
            self.head = 0;
        }
    }
}

impl Clone for ElementVec {
    /// Copies only the live window, not the dead prefix.
    fn clone(&self) -> Self {
        Self::from(self.to_vec())
    }
}

impl Deref for ElementVec {
    type Target = [JsValue];

    #[inline]
    fn deref(&self) -> &[JsValue] {
        &self.buf[self.head..]
    }
}

impl DerefMut for ElementVec {
    #[inline]
    fn deref_mut(&mut self) -> &mut [JsValue] {
        &mut self.buf[self.head..]
    }
}

impl From<Vec<JsValue>> for ElementVec {
    #[inline]
    fn from(buf: Vec<JsValue>) -> Self {
        Self { buf, head: 0 }
    }
}

impl<'a> IntoIterator for &'a ElementVec {
    type Item = &'a JsValue;
    type IntoIter = std::slice::Iter<'a, JsValue>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl JsObjectData {
    /// Replaces the elements of an Array object.
    pub(crate) fn set_array_elements(&mut self, values: Vec<JsValue>) {
        *self.array_elements_mut().expect("Array object") = values.into();
    }

    /// Number of Array elements, `0` for a non-Array object.
    pub(crate) fn array_len(&self) -> usize {
        self.array_elements().map_or(0, |e| e.len())
    }
}

impl Interpreter {
    /// Borrow `array`'s element vector immutably for the duration of `f`.
    /// Panics if `array` is not an engine-allocated Array object.
    pub(crate) fn with_array_elements<R>(
        &self,
        array: &JsValue,
        f: impl FnOnce(&ElementVec) -> R,
    ) -> R {
        let id = array
            .as_object_id()
            .expect("with_array_elements: value must be an Array object");
        let obj = self.get_object_cell_expect(id).borrow();
        let elements = obj
            .array_elements()
            .expect("with_array_elements: object must have Array elements");
        f(elements)
    }

    /// Borrow `array`'s element vector mutably for the duration of `f`, going
    /// through `ObjectHandle::borrow_mut` so the generational write barrier
    /// still runs.
    /// Panics if `array` is not an engine-allocated Array object.
    pub(crate) fn with_array_elements_mut<R>(
        &self,
        array: &JsValue,
        f: impl FnOnce(&mut ElementVec) -> R,
    ) -> R {
        let id = array
            .as_object_id()
            .expect("with_array_elements_mut: value must be an Array object");
        let mut obj = self.get_object_cell_expect(id).borrow_mut();
        let elements = obj
            .array_elements_mut()
            .expect("with_array_elements_mut: object must have Array elements");
        f(elements)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(n: i32) -> JsValue {
        JsValue::number(f64::from(n))
    }

    #[test]
    fn pop_front_preserves_order_and_compacts() {
        let mut v = ElementVec::from((0..200).map(num).collect::<Vec<_>>());
        for i in 0..150 {
            assert_eq!(v.pop_front().unwrap().as_number(), Some(f64::from(i)));
            assert_eq!(v.len(), 199 - i as usize);
        }
        assert_eq!(v[0].as_number(), Some(150.0));
        assert_eq!(v.pop().unwrap().as_number(), Some(199.0));
        v.push(num(7));
        assert_eq!(v.last().unwrap().as_number(), Some(7.0));
    }

    #[test]
    fn pop_to_empty_resets_the_dead_prefix() {
        let mut v = ElementVec::from((0..40).map(num).collect::<Vec<_>>());
        for _ in 0..10 {
            v.pop_front();
        }
        while v.pop().is_some() {}
        assert!(v.is_empty());
        v.push(num(1));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].as_number(), Some(1.0));
    }

    #[test]
    fn splice_resize_extend_and_clone_see_only_live_window() {
        let mut v = ElementVec::from((0..40).map(num).collect::<Vec<_>>());
        for _ in 0..10 {
            v.pop_front();
        }
        let removed = v.splice(0..2, [num(-1)]);
        assert_eq!(removed.len(), 2);
        assert_eq!(removed[0].as_number(), Some(10.0));
        assert_eq!(v.len(), 29);
        assert_eq!(v[0].as_number(), Some(-1.0));
        v.extend([num(100)]);
        v.resize(32, JsValue::UNDEFINED);
        assert_eq!(v.len(), 32);
        assert!(v[31].is_undefined());
        assert_eq!(v[28].as_number(), Some(39.0));
        assert_eq!(v[29].as_number(), Some(100.0));
        let c = v.clone();
        assert_eq!(c.len(), 32);
        assert_eq!(c[0].as_number(), Some(-1.0));
        assert_eq!(v.take().len(), 32);
        assert!(v.is_empty());
    }

    #[test]
    fn drains_to_empty_and_resets() {
        let mut v = ElementVec::from(vec![num(1), num(2)]);
        assert!(v.pop_front().is_some());
        assert!(v.pop_front().is_some());
        assert!(v.pop_front().is_none());
        assert!(v.is_empty());
        v.push(num(3));
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn indexed_ops_see_only_live_window() {
        let mut v = ElementVec::from((0..40).map(num).collect::<Vec<_>>());
        for _ in 0..10 {
            v.pop_front();
        }
        assert_eq!(v.remove(1).as_number(), Some(11.0));
        assert_eq!(v[0].as_number(), Some(10.0));
        v.truncate(3);
        assert_eq!(v.len(), 3);
        assert_eq!(v.into_vec().len(), 3);
    }
}
