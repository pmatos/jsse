// Dense-array fast paths for Array.prototype.
//
// Every helper here either performs the whole spec algorithm step on a *plain
// dense Array* (see `JsObjectData::is_plain_dense_array`) or returns
// `None`/`false` without having changed anything, so the caller falls back to
// the spec-literal per-element loop. Guards are evaluated at the point the
// element loop would start (after every argument conversion and species
// lookup), and none of these helpers can run user code, so the state they
// checked is the state they act on.
//
// Representation recap: an exotic Array keeps default-attribute data elements
// in `array_elements()`; `undefined` there is the HOLE sentinel unless the
// index also owns a descriptor in `properties`. A plain dense Array owns no
// descriptor besides `length`, so `undefined` always means "absent".
use super::super::*;
use crate::interpreter::types::{ObjectKind, fresh_shape_id, is_array_index_property_key};

// Values above this are left to the slow path so a hostile `length` cannot
// make a fast path allocate an enormous backing vector.
const MAX_FAST_FILL: usize = 1 << 24;

/// [[HasProperty]] followed by [[Get]] for an index that is an own present
/// element of a plain-attribute Array. `None` means "not provably an own
/// present data element": the caller must take the generic path.
#[inline]
pub(super) fn dense_element(interp: &Interpreter, o: &JsValue, idx: usize) -> Option<JsValue> {
    let cell = array_cell(interp, o)?;
    let b = cell.borrow();
    let v = b.array_elements()?.get(idx)?;
    if v.is_undefined() || b.properties.len() != 1 {
        return None;
    }
    Some(v.clone())
}

/// True when nothing on the prototype chain starting at `start` can answer an
/// array-index [[HasProperty]]/[[Get]]/[[Set]]: every link is an ordinary
/// object (or an empty Array such as `Array.prototype`) that owns no
/// array-index key. Holes then stay absent, and writes of new indices need no
/// setter, non-writable or proxy-trap consultation.
pub(crate) fn proto_chain_index_free(interp: &Interpreter, start: Option<u64>) -> bool {
    walk_index_free_chain(interp, start, true)
}

fn walk_index_free_chain(interp: &Interpreter, start: Option<u64>, scan_keys: bool) -> bool {
    let mut next = start;
    let mut hops = 0usize;
    while let Some(id) = next {
        let Some(cell) = interp.get_object_cell(id) else {
            return false;
        };
        let b = cell.borrow();
        match &b.kind {
            // String wrappers are Ordinary but expose virtual index
            // properties that never reach `properties`.
            ObjectKind::Ordinary if !b.primitive_value.as_ref().is_some_and(JsValue::is_string) => {
            }
            ObjectKind::Array(_) if b.array_elements().is_some_and(Vec::is_empty) => {}
            _ => return false,
        }
        if b.properties.may_hold_index_key()
            && (!scan_keys || b.properties.keys().any(is_array_index_property_key))
        {
            return false;
        }
        next = b.prototype_id;
        hops += 1;
        if hops > 64 {
            return false;
        }
    }
    true
}

/// [`proto_chain_index_free`] for per-read use: a link owning any digit-leading
/// key declines instead of scanning its key list.
pub(crate) fn proto_chain_without_index_keys(interp: &Interpreter, start: Option<u64>) -> bool {
    walk_index_free_chain(interp, start, false)
}

/// The values a plain dense Array yields when its iteration is entirely intrinsic (original
/// `@@iterator` and `%ArrayIteratorPrototype%.next`), copied without creating an iterator.
/// A hole reads `undefined` only when no prototype can answer the index.
pub(crate) fn try_intrinsic_iteration(
    interp: &Interpreter,
    iterable: &JsValue,
) -> Option<Vec<JsValue>> {
    let b = array_cell(interp, iterable)?.borrow();
    let elements = b.array_elements()?;
    let realm = interp.realm();
    if !b.is_plain_dense_array(elements.len()) || b.prototype_id != realm.array_prototype {
        return None;
    }
    let iterator_key = JsPropertyKey::cached_well_known_symbol("iterator")?;
    let values_id = realm.array_prototype_values.as_ref()?.as_object_id()?;
    let proto = interp.get_object_cell(realm.array_prototype?)?.borrow();
    let iterator_method = proto.properties.get(iterator_key)?;
    if iterator_method.value().and_then(JsValue::as_object_id) != Some(values_id) {
        return None;
    }
    drop(proto);
    let next_id = realm.iterator_next_intrinsics.array?;
    let iterator_proto = interp
        .get_object_cell(realm.array_iterator_prototype?)?
        .borrow();
    let next = iterator_proto.properties.get("next")?;
    if next.value().and_then(JsValue::as_object_id) != Some(next_id) {
        return None;
    }
    if has_hole(elements) && !proto_chain_index_free(interp, b.prototype_id) {
        return None;
    }
    Some(elements.clone())
}

fn has_hole(elements: &[JsValue]) -> bool {
    elements.iter().any(JsValue::is_undefined)
}

fn array_cell<'a>(interp: &'a Interpreter, o: &JsValue) -> Option<&'a ObjectHandle> {
    interp.get_object_cell(o.as_object_id()?)
}

// Moving elements across holes lands present elements on holes (creating
// properties, so the receiver must be extensible) and must keep holes absent
// (so nothing inherited may supply an index).
fn holes_movable(interp: &Interpreter, b: &JsObjectData, holes: bool) -> bool {
    !holes || (b.extensible && proto_chain_index_free(interp, b.prototype_id))
}

pub(super) fn try_pop(interp: &Interpreter, o: &JsValue, len: usize) -> Option<JsValue> {
    let cell = array_cell(interp, o)?;
    {
        let b = cell.borrow();
        if len == 0 || !b.is_plain_dense_array(len) || b.array_elements()?[len - 1].is_undefined() {
            return None;
        }
    }
    let mut b = cell.borrow_mut();
    let v = b.array_elements_mut()?.pop()?;
    b.set_dense_array_length(len - 1);
    Some(v)
}

/// Returns the removed first element (`undefined` when it was a hole).
pub(super) fn try_shift(interp: &Interpreter, o: &JsValue, len: usize) -> Option<JsValue> {
    let cell = array_cell(interp, o)?;
    {
        let b = cell.borrow();
        if len == 0
            || !b.is_plain_dense_array(len)
            || !holes_movable(interp, &b, has_hole(b.array_elements()?))
        {
            return None;
        }
    }
    let mut b = cell.borrow_mut();
    let first = b.array_elements_mut()?.remove(0);
    b.set_dense_array_length(len - 1);
    Some(first)
}

/// Returns the new length.
pub(super) fn try_unshift(
    interp: &Interpreter,
    o: &JsValue,
    len: usize,
    items: &[JsValue],
) -> Option<usize> {
    // A present `undefined` item needs a descriptor, which dense storage
    // cannot express.
    if items.is_empty() || items.iter().any(JsValue::is_undefined) {
        return None;
    }
    let new_len = len.checked_add(items.len())?;
    if new_len > 0xFFFF_FFFE {
        return None;
    }
    let cell = array_cell(interp, o)?;
    let proto = {
        let b = cell.borrow();
        if !b.extensible || !b.is_plain_dense_array(len) {
            return None;
        }
        b.prototype_id
    };
    if !proto_chain_index_free(interp, proto) {
        return None;
    }
    let mut b = cell.borrow_mut();
    b.array_elements_mut()?.splice(0..0, items.iter().cloned());
    b.set_dense_array_length(new_len);
    Some(new_len)
}

/// The result array of a splice/slice: an empty, extensible plain Array whose
/// `length` already equals `expected_len`, distinct from the receiver.
fn is_fresh_result_array(b: &JsObjectData, expected_len: usize) -> bool {
    b.extensible
        && b.properties.len() == 1
        && b.array_elements().is_some_and(Vec::is_empty)
        && b.has_writable_array_length(expected_len)
}

/// The element-moving body of Array.prototype.splice (steps 11-17), for a
/// plain dense receiver and a fresh result array. `a` is the object returned
/// by ArraySpeciesCreate(O, actualDeleteCount). Returns whether it ran.
pub(super) fn try_splice(
    interp: &Interpreter,
    o: &JsValue,
    a: &JsValue,
    len: usize,
    start: usize,
    delete_count: usize,
    items: &[JsValue],
) -> bool {
    if items.iter().any(JsValue::is_undefined) || start + delete_count > len {
        return false;
    }
    let Some(o_id) = o.as_object_id() else {
        return false;
    };
    let Some(a_id) = a.as_object_id() else {
        return false;
    };
    if o_id == a_id {
        return false;
    }
    let (Some(o_cell), Some(a_cell)) = (interp.get_object_cell(o_id), interp.get_object_cell(a_id))
    else {
        return false;
    };
    let new_len = len - delete_count + items.len();
    if new_len > 0xFFFF_FFFE {
        return false;
    }
    let (needs_chain, proto) = {
        let ob = o_cell.borrow();
        if !ob.extensible || !ob.is_plain_dense_array(len) {
            return false;
        }
        let ab = a_cell.borrow();
        if !is_fresh_result_array(&ab, delete_count) {
            return false;
        }
        // Without growth and without holes in the moved range, every Set hits
        // an own present element and no inherited index is consulted.
        let needs_chain =
            items.len() > delete_count || has_hole(&ob.array_elements().unwrap()[start..]);
        (needs_chain, ob.prototype_id)
    };
    if needs_chain && !proto_chain_index_free(interp, proto) {
        return false;
    }
    let mut ob = o_cell.borrow_mut();
    let mut ab = a_cell.borrow_mut();
    let removed: Vec<JsValue> = ob
        .array_elements_mut()
        .unwrap()
        .splice(start..start + delete_count, items.iter().cloned())
        .collect();
    ob.set_dense_array_length(new_len);
    if !removed.is_empty() {
        ab.array_elements_mut().unwrap().extend(removed);
        ab.shape_id = fresh_shape_id();
    }
    true
}

/// Array.prototype.slice copy loop (steps 10-12) for a plain-attribute
/// receiver and a fresh result array. `a` was created with length
/// `fin - k`. Returns whether it ran.
pub(super) fn try_slice(
    interp: &Interpreter,
    o: &JsValue,
    a: &JsValue,
    k: usize,
    fin: usize,
) -> bool {
    let count = fin.saturating_sub(k);
    let (Some(o_id), Some(a_id)) = (o.as_object_id(), a.as_object_id()) else {
        return false;
    };
    if o_id == a_id || count == 0 {
        return false;
    }
    let (Some(o_cell), Some(a_cell)) = (interp.get_object_cell(o_id), interp.get_object_cell(a_id))
    else {
        return false;
    };
    {
        let ab = a_cell.borrow();
        if !is_fresh_result_array(&ab, count) {
            return false;
        }
    }
    let (holes, proto) = {
        let ob = o_cell.borrow();
        let Some(elements) = ob.array_elements() else {
            return false;
        };
        if ob.properties.len() != 1 || elements.len() < fin {
            return false;
        }
        (has_hole(&elements[k..fin]), ob.prototype_id)
    };
    if holes && !proto_chain_index_free(interp, proto) {
        return false;
    }
    let ob = o_cell.borrow();
    let mut ab = a_cell.borrow_mut();
    let src = &ob.array_elements().unwrap()[k..fin];
    ab.array_elements_mut().unwrap().extend_from_slice(src);
    ab.shape_id = fresh_shape_id();
    true
}

/// One spreadable Array item of Array.prototype.concat, appended to the
/// result array `a` that currently holds `n` indices. Returns whether it ran.
pub(super) fn try_concat_item(
    interp: &Interpreter,
    a: &JsValue,
    item: &JsValue,
    n: usize,
    len: usize,
) -> bool {
    let (Some(a_id), Some(i_id)) = (a.as_object_id(), item.as_object_id()) else {
        return false;
    };
    if a_id == i_id || len == 0 {
        return false;
    }
    let (Some(a_cell), Some(i_cell)) = (interp.get_object_cell(a_id), interp.get_object_cell(i_id))
    else {
        return false;
    };
    {
        let ab = a_cell.borrow();
        let Some(elements) = ab.array_elements() else {
            return false;
        };
        if !ab.extensible
            || elements.len() != n
            || ab.properties.len() != 1
            || n + len > 0xFFFF_FFFE
            || ab
                .properties
                .get("length")
                .is_none_or(|d| d.writable() == Some(false))
        {
            return false;
        }
    }
    let (holes, proto) = {
        let ib = i_cell.borrow();
        if !ib.is_plain_dense_array(len) {
            return false;
        }
        let elements = ib.array_elements().unwrap();
        // A trailing hole would leave the result's backing vector longer than
        // the length the spec has reached at this point.
        if elements.last().is_some_and(JsValue::is_undefined) {
            return false;
        }
        (has_hole(elements), ib.prototype_id)
    };
    if holes && !proto_chain_index_free(interp, proto) {
        return false;
    }
    let ib = i_cell.borrow();
    let mut ab = a_cell.borrow_mut();
    ab.array_elements_mut()
        .unwrap()
        .extend_from_slice(ib.array_elements().unwrap());
    let old_len = ab
        .properties
        .get("length")
        .and_then(|d| d.value())
        .and_then(JsValue::as_number)
        .unwrap_or(0.0);
    if ((n + len) as f64) > old_len {
        ab.set_dense_array_length(n + len);
    } else {
        ab.shape_id = fresh_shape_id();
    }
    true
}

pub(super) fn try_reverse(interp: &Interpreter, o: &JsValue, len: usize) -> bool {
    let Some(cell) = array_cell(interp, o) else {
        return false;
    };
    let holes = {
        let b = cell.borrow();
        if !b.is_plain_dense_array(len) {
            return false;
        }
        let holes = has_hole(b.array_elements().unwrap());
        if !holes_movable(interp, &b, holes) {
            return false;
        }
        holes
    };
    let mut b = cell.borrow_mut();
    b.array_elements_mut().unwrap().reverse();
    // Holes move with the elements, so the set of own indices changes.
    if holes {
        b.shape_id = fresh_shape_id();
    }
    true
}

/// Array.prototype.fill over `[k, fin)` with a non-undefined `value`, on an
/// extensible plain Array whose backing vector does not exceed `len` (a
/// `new Array(n)` receiver has no backing elements yet).
pub(super) fn try_fill(
    interp: &Interpreter,
    o: &JsValue,
    len: usize,
    k: usize,
    fin: usize,
    value: &JsValue,
) -> bool {
    if value.is_undefined() || k >= fin || fin > len || fin > MAX_FAST_FILL {
        return false;
    }
    let Some(cell) = array_cell(interp, o) else {
        return false;
    };
    let (needs_chain, proto) = {
        let b = cell.borrow();
        let Some(elements) = b.array_elements() else {
            return false;
        };
        if !b.extensible
            || b.properties.len() != 1
            || elements.len() > len
            || elements.len() < k
            || !b.has_writable_array_length(len)
        {
            return false;
        }
        (
            fin > elements.len() || has_hole(&elements[k..fin]),
            b.prototype_id,
        )
    };
    if needs_chain && !proto_chain_index_free(interp, proto) {
        return false;
    }
    let mut b = cell.borrow_mut();
    let elements = b.array_elements_mut().unwrap();
    let grew = fin > elements.len();
    let filled_hole = has_hole(&elements[k..fin.min(elements.len())]);
    if grew {
        elements.resize(fin, JsValue::UNDEFINED);
    }
    elements[k..fin].fill(value.clone());
    if grew || filled_hole {
        b.shape_id = fresh_shape_id();
    }
    true
}

/// The write-back of Array.prototype.sort: assigns `sorted` (all present
/// values) over indices `0..len` of a plain dense receiver that the
/// comparator left in a state where every `Set` is an in-place store or a
/// plain new-property creation.
pub(super) fn try_store_sorted(
    interp: &Interpreter,
    o: &JsValue,
    len: usize,
    sorted: &[JsValue],
) -> bool {
    if sorted.len() != len || has_hole(sorted) {
        return false;
    }
    let Some(cell) = array_cell(interp, o) else {
        return false;
    };
    let holes = {
        let b = cell.borrow();
        if !b.is_plain_dense_array(len) {
            return false;
        }
        let holes = has_hole(b.array_elements().unwrap());
        if !holes_movable(interp, &b, holes) {
            return false;
        }
        holes
    };
    let mut b = cell.borrow_mut();
    b.array_elements_mut().unwrap().clone_from_slice(sorted);
    if holes {
        b.shape_id = fresh_shape_id();
    }
    true
}

/// Index of the first element equal to `search` under `eq`, scanning
/// ascending from `from`, stopping at the first hole or at the end of the
/// backing vector. Returns `(found, resume)`: `resume` is the first index the
/// generic loop must still examine.
pub(super) fn scan_forward(
    interp: &Interpreter,
    o: &JsValue,
    from: usize,
    to: usize,
    eq: impl Fn(&JsValue) -> bool,
) -> (Option<usize>, usize) {
    let Some(cell) = array_cell(interp, o) else {
        return (None, from);
    };
    let b = cell.borrow();
    let Some(elements) = b.array_elements() else {
        return (None, from);
    };
    if b.properties.len() != 1 {
        return (None, from);
    }
    let end = to.min(elements.len());
    let mut i = from;
    while i < end {
        let v = &elements[i];
        if v.is_undefined() {
            break;
        }
        if eq(v) {
            return (Some(i), i);
        }
        i += 1;
    }
    (None, i)
}

/// Descending counterpart of `scan_forward` over indices `<= from`; the
/// returned `resume` is an exclusive upper bound for the generic loop.
pub(super) fn scan_backward(
    interp: &Interpreter,
    o: &JsValue,
    from: usize,
    eq: impl Fn(&JsValue) -> bool,
) -> (Option<usize>, usize) {
    let Some(cell) = array_cell(interp, o) else {
        return (None, from + 1);
    };
    let b = cell.borrow();
    let Some(elements) = b.array_elements() else {
        return (None, from + 1);
    };
    if b.properties.len() != 1 || from >= elements.len() {
        return (None, from + 1);
    }
    let mut i = from + 1;
    while i > 0 {
        let v = &elements[i - 1];
        if v.is_undefined() {
            break;
        }
        if eq(v) {
            return (Some(i - 1), i);
        }
        i -= 1;
    }
    (None, i)
}
