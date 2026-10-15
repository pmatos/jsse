use super::super::*;
use super::array_dense;
use crate::interpreter::gc::RootedSlots;

// §7.2.2 IsArray — Proxy-aware check
fn is_array_check(interp: &mut Interpreter, obj_id: u64) -> Result<bool, JsValue> {
    let snapshot = interp.get_object_cell(obj_id).map(|cell| {
        let b = cell.borrow();
        // `is_proxy()` is active-only; revoked proxies still need IsArray to throw, so we
        // also surface the revoked flag.
        (
            b.is_proxy_revoked(),
            b.is_proxy() || b.is_proxy_revoked(),
            b.proxy_target_id(),
            b.class_name,
        )
    });
    if let Some((is_revoked, is_proxy, target_id, class)) = snapshot {
        if is_revoked {
            return Err(interp
                .create_type_error("Cannot perform 'IsArray' on a proxy that has been revoked"));
        }
        if is_proxy {
            if let Some(tid) = target_id {
                return is_array_check(interp, tid);
            }
            return Ok(false);
        }
        Ok(class == "Array")
    } else {
        Ok(false)
    }
}

fn to_object_val(interp: &mut Interpreter, this: &JsValue) -> Result<JsValue, Completion> {
    match interp.to_object(this) {
        Completion::Normal(v) => Ok(v),
        other => Err(other),
    }
}

fn length_of_array_like(interp: &mut Interpreter, o: &JsValue) -> Result<usize, Completion> {
    // An exotic Array's `length` is always an own data property, so reading
    // it directly is exactly `Get(O, "length")`.
    let array_length = o
        .as_object_id()
        .and_then(|id| interp.get_object_cell(id))
        .and_then(|cell| {
            let b = cell.borrow();
            b.array_elements()?;
            b.properties
                .get("length")
                .and_then(|d| d.value())
                .and_then(JsValue::as_number)
        });
    let n = match array_length {
        Some(n) => n,
        None => {
            let len_val = if let Some(obj_id) = o.as_object_id() {
                match interp.get_object_property(obj_id, "length", o) {
                    Completion::Normal(v) => v,
                    other => return Err(other),
                }
            } else {
                JsValue::UNDEFINED
            };
            // ? ToLength(lenProperty) — must propagate errors from ToNumber/ToPrimitive
            match interp.to_number_value(&len_val) {
                Ok(v) => v,
                Err(e) => return Err(Completion::Throw(e)),
            }
        }
    };
    let len = to_integer_or_infinity(n);
    if len < 0.0 {
        return Ok(0);
    }
    Ok(len.min(9007199254740991.0) as usize)
}

fn get_obj<'a>(interp: &'a Interpreter, o: &JsValue) -> Option<&'a ObjectHandle> {
    if let Some(obj_id) = o.as_object_id() {
        interp.get_object_cell(obj_id)
    } else {
        None
    }
}

fn obj_get(interp: &mut Interpreter, o: &JsValue, key: &str) -> Result<JsValue, Completion> {
    if let Some(obj_id) = o.as_object_id() {
        match interp.get_object_property(obj_id, key, o) {
            Completion::Normal(v) => Ok(v),
            other => Err(other),
        }
    } else {
        Ok(JsValue::UNDEFINED)
    }
}

// [[Get]](O, ToString(idx)) with an own-present-element fast path.
fn get_index(interp: &mut Interpreter, o: &JsValue, idx: usize) -> Result<JsValue, Completion> {
    if let Some(v) = array_dense::dense_element(interp, o, idx) {
        return Ok(v);
    }
    obj_get(interp, o, &idx.to_string())
}

// HasProperty(O, ToString(idx)) followed by Get when present.
fn get_if_present(
    interp: &mut Interpreter,
    o: &JsValue,
    idx: usize,
) -> Result<Option<JsValue>, Completion> {
    if let Some(v) = array_dense::dense_element(interp, o, idx) {
        return Ok(Some(v));
    }
    let pk = idx.to_string();
    if !obj_has_throw(interp, o, &pk).map_err(Completion::Throw)? {
        return Ok(None);
    }
    obj_get(interp, o, &pk).map(Some)
}

// CreateDataPropertyOrThrow(A, ToString(idx), value) appending in place when
// A is a plain dense Array whose next index is `idx`.
fn create_data_property_at(
    interp: &mut Interpreter,
    a: &JsValue,
    idx: usize,
    value: JsValue,
) -> Result<(), JsValue> {
    if let Some(cell) = a.as_object_id().and_then(|id| interp.get_object_cell(id))
        && cell
            .borrow_mut()
            .try_append_dense_array_element(idx, &value)
    {
        return Ok(());
    }
    create_data_property_or_throw(interp, a, &idx.to_string(), value)
}

// Steps 6.c.i-ii of Array.prototype.toLocaleString, shared with
// %TypedArray%.prototype.toLocaleString: appends
// ? ToString(? Invoke(element, "toLocaleString", args)) to `out`.
pub(super) fn append_locale_string(
    interp: &mut Interpreter,
    out: &mut Vec<u16>,
    element: &JsValue,
    args: &[JsValue],
) -> Result<(), Completion> {
    let element_obj = match interp.to_object(element) {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let obj_id = element_obj
        .as_object_id()
        .expect("ToObject must return an object");
    let method = match interp.get_object_property(obj_id, "toLocaleString", element) {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    if !interp.is_callable(&method) {
        return Err(Completion::Throw(
            interp.create_type_error("toLocaleString is not a function"),
        ));
    }
    let value = match interp.call_function(&method, element, args) {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    interp
        .append_js_string(out, &value)
        .map_err(Completion::Throw)
}

// Set(O, P, V, true) — invoke setters on prototype, check writable, throw on failure
fn obj_set_throw(
    interp: &mut Interpreter,
    o: &JsValue,
    key: &str,
    value: JsValue,
) -> Result<(), JsValue> {
    if let Some(obj_id) = o.as_object_id() {
        // Check for Proxy
        let is_proxy = interp.get_object_cell(obj_id).is_some_and(|cell| {
            let b = cell.borrow();
            b.is_proxy() || b.is_proxy_revoked()
        });
        if is_proxy {
            match interp.proxy_set(obj_id, key, value, o) {
                Ok(true) => return Ok(()),
                Ok(false) => {
                    return Err(interp.create_type_error(&format!(
                        "Cannot assign to read only property '{key}'"
                    )));
                }
                Err(e) => return Err(e),
            }
        }
        // OrdinarySet (§10.1.9.2): when the receiver has no own property for the
        // key, the parent is consulted. If a Proxy sits in the prototype chain,
        // its [[Set]] trap must run with the original receiver — a bare Proxy
        // prototype exposes no own descriptor, so the descriptor walk below would
        // otherwise skip the trap and wrongly create an own property. proxy_set is
        // a full proxy-aware OrdinarySet, so delegating the whole [[Set]] here also
        // honours inherited setters / non-writable data along the way.
        {
            let has_own = interp
                .get_object_cell(obj_id)
                .is_some_and(|cell| cell.borrow().has_own_property(key));
            if !has_own && interp.has_proxy_in_prototype_chain(obj_id) {
                return match interp.proxy_set(obj_id, key, value, o) {
                    Ok(true) => Ok(()),
                    Ok(false) => Err(interp.create_type_error(&format!(
                        "Cannot assign to read only property '{key}'"
                    ))),
                    Err(e) => Err(e),
                };
            }
        }
        // Check for setter in prototype chain
        {
            let desc = interp.get_property_descriptor_on_id(obj_id, key);
            if let Some(ref d) = desc {
                if let Some(ref setter) = d.set
                    && !(setter).is_undefined()
                {
                    let setter = setter.clone();
                    let this = o.clone();
                    return match interp.call_function(&setter, &this, &[value]) {
                        Completion::Normal(_) => Ok(()),
                        Completion::Throw(e) => Err(e),
                        _ => Ok(()),
                    };
                }
                // Accessor with no setter
                if d.is_accessor_descriptor() {
                    return Err(interp.create_type_error(&format!(
                        "Cannot set property '{key}' which has only a getter"
                    )));
                }
                // Non-writable data property
                if d.writable == Some(false) {
                    return Err(interp.create_type_error(&format!(
                        "Cannot assign to read only property '{key}'"
                    )));
                }
            }
        }
        // TypedArray [[Set]]: must call ToNumber/ToBigInt before writing (§10.4.5.5)
        {
            let ta_info = interp.get_object_cell(obj_id).and_then(|cell| {
                let b = cell.borrow();
                b.typed_array_info()
                    .as_ref()
                    .map(|ta| (ta.kind.is_bigint(), ta.kind))
            });
            if let Some((is_bigint, _kind)) = ta_info
                && let Some(index) = canonical_numeric_index_string(key)
            {
                let num_val = if is_bigint {
                    interp.to_bigint_value(&value)?
                } else {
                    {
                        let n = interp.to_number_value(&value)?;
                        JsValue::number(n)
                    }
                };
                if let Some(cell) = interp.get_object_cell(obj_id)
                    && let Some(ta) = cell.borrow().typed_array_info()
                    && is_valid_integer_index(ta, index)
                {
                    typed_array_set_index(ta, index as usize, &num_val);
                }
                return Ok(());
            }
        }
        // Normal set
        if let Some(cell) = interp.get_object_cell(obj_id) {
            let needs_error = {
                let borrow = cell.borrow();
                !borrow.extensible && !borrow.has_own_property(key)
            };
            if needs_error {
                return Err(interp.create_type_error(&format!(
                    "Cannot add property {key}, object is not extensible"
                )));
            }
            interp.gc_write_barrier_value(cell, &value);
            cell.borrow_mut_untracked().set_property_value(key, value);
        }
    }
    Ok(())
}

pub(crate) fn create_data_property_or_throw<K: PropertyKeyLike + ?Sized>(
    interp: &mut Interpreter,
    o: &JsValue,
    key: &K,
    value: JsValue,
) -> Result<(), JsValue> {
    let key = key.to_js_property_key();
    if let Some(obj_id) = o.as_object_id() {
        // Deferred namespace: trigger evaluation on [[DefineOwnProperty]]
        {
            let is_deferred_ns = interp.get_object_cell(obj_id).is_some_and(|cell| {
                cell.borrow()
                    .module_namespace()
                    .is_some_and(|ns| ns.deferred)
            });
            if is_deferred_ns && !Interpreter::is_symbol_like_namespace_key(&key, true) {
                interp.ensure_deferred_namespace_evaluation(obj_id)?;
            }
        }
        // Check for Proxy defineProperty trap
        let is_proxy = interp
            .get_object_cell(obj_id)
            .is_some_and(|cell| cell.borrow().is_proxy());
        if is_proxy {
            // Build descriptor object for proxy trap
            let desc_obj_id = interp.create_object_id();
            {
                let mut borrow = interp.get_object_cell_expect(desc_obj_id).borrow_mut();
                borrow.set_property_value("value", value);
                borrow.set_property_value("writable", JsValue::TRUE);
                borrow.set_property_value("enumerable", JsValue::TRUE);
                borrow.set_property_value("configurable", JsValue::TRUE);
            }
            let desc_id = desc_obj_id;
            let desc_val = JsValue::object(desc_id);
            return match interp.proxy_define_own_property(obj_id, key.clone(), &desc_val) {
                Ok(true) => Ok(()),
                Ok(false) => {
                    Err(interp.create_type_error(&format!("Cannot define property: {key}")))
                }
                Err(e) => Err(e),
            };
        }
        if interp.get_object_cell(obj_id).is_some() {
            // Check extensibility — CreateDataProperty fails when the object is
            // not extensible and lacks an OWN property of this key (an inherited
            // property must not satisfy the check; cf. OrdinaryDefineOwnProperty).
            let (not_extensible, has_own) = {
                let borrow = interp.get_object_cell_expect(obj_id).borrow();
                (!borrow.extensible, borrow.has_own_property(&key))
            };
            if not_extensible && !has_own {
                return Err(interp.create_type_error(&format!(
                    "Cannot add property {key}, object is not extensible"
                )));
            }
            // Check for non-configurable existing property
            let non_configurable = {
                let borrow = interp.get_object_cell_expect(obj_id).borrow();
                borrow
                    .get_own_property(&key)
                    .is_some_and(|desc| desc.configurable == Some(false))
            };
            if non_configurable {
                return Err(interp.create_type_error(&format!("Cannot redefine property: {key}")));
            }
            let cell = interp.get_object_cell_expect(obj_id);
            interp.gc_write_barrier_value(cell, &value);
            let mut borrow = cell.borrow_mut_untracked();
            if borrow.array_elements().is_some() {
                if borrow.try_append_dense_array_data_property(&key, &value) {
                    return Ok(());
                }
                drop(borrow);
                return match interp.array_define_own_property(
                    obj_id as usize,
                    &key,
                    PropertyDescriptor::data_default(value),
                )? {
                    true => Ok(()),
                    false => {
                        Err(interp.create_type_error(&format!("Cannot define property: {key}")))
                    }
                };
            }
            borrow.define_own_property(key, PropertyDescriptor::data_default(value));
        }
    }
    Ok(())
}

// Validates the comparefn argument shared by `sort` and `toSorted`, returning
// it only when one was given (undefined means the default order).
pub(crate) fn sort_compare_fn_arg(
    interp: &mut Interpreter,
    args: &[JsValue],
) -> Result<Option<JsValue>, Completion> {
    match args.first() {
        None => Ok(None),
        Some(cf) if cf.is_undefined() => Ok(None),
        Some(cf) if interp.is_callable(cf) => Ok(Some(cf.clone())),
        Some(_) => Err(Completion::Throw(
            interp.create_type_error("compareFn is not a function"),
        )),
    }
}

// Reads elements 0..len of `o` for sorting, rooting each one as it is read:
// later getters can reach GC safepoints while earlier elements are only
// reachable from the returned `Vec`. Must run inside a GC root scope. With
// `skip_holes`, absent indices are left out (SortIndexedProperties with
// SKIP-HOLES); otherwise they read as undefined.
fn gather_rooted_elements(
    interp: &mut Interpreter,
    o: &JsValue,
    len: usize,
    skip_holes: bool,
) -> Result<Vec<JsValue>, Completion> {
    // `len` can be up to 2^53-1 with few (or no) present elements.
    let mut items: Vec<JsValue> = Vec::with_capacity(len.min(1 << 16));
    for i in 0..len {
        let v = if skip_holes {
            match get_if_present(interp, o, i)? {
                Some(v) => v,
                None => continue,
            }
        } else {
            get_index(interp, o, i)?
        };
        interp.gc_root_value(&v);
        items.push(v);
    }
    Ok(items)
}

// §23.1.3.30.1 SortIndexedProperties + §23.1.3.30.2 CompareArrayElements,
// shared by `sort` and `toSorted`. Undefineds sort last without reaching the
// comparator. Without a comparator, each remaining element is converted with
// ToString once (the spec leaves the number and order of SortCompare calls
// implementation-defined) and the keys are compared by UTF-16 code units,
// which is the order IsLessThan defines for Strings.
//
// The caller must keep every element of `items` and `compare_fn` GC-rooted:
// the comparator and ToString can run arbitrary JS (and so reach safepoints)
// while elements are only reachable from this Rust `Vec`.
fn sort_values(
    interp: &mut Interpreter,
    items: &mut Vec<JsValue>,
    compare_fn: Option<&JsValue>,
) -> Result<(), JsValue> {
    let before = items.len();
    items.retain(|v| !v.is_undefined());
    let undefined_count = before - items.len();
    if let Some(cf) = compare_fn {
        sort_with_comparefn(interp, items, cf)?;
    } else if items.len() > 1 {
        let mut keyed: Vec<(JsString, JsValue)> = Vec::with_capacity(items.len());
        for v in items.drain(..) {
            let key = interp.to_js_string(&v)?;
            keyed.push((key, v));
        }
        // Code-unit slice comparison is a total order, so `sort_by` cannot
        // hit its ordering-violation panic here.
        keyed.sort_by(|(a, _), (b, _)| a.as_units().cmp(b.as_units()));
        items.extend(keyed.into_iter().map(|(_, v)| v));
    }
    items.extend(std::iter::repeat_n(JsValue::UNDEFINED, undefined_count));
    Ok(())
}

// Sorts `items` with a user comparefn (CompareArrayElements step 4, shared
// with %TypedArray%.prototype.sort/toSorted): call it, ToNumber the result,
// and treat NaN as +0. Uses `merge_sort_by`, so an inconsistent comparefn
// cannot panic, and stops at the first abrupt completion.
pub(crate) fn sort_with_comparefn(
    interp: &mut Interpreter,
    items: &mut Vec<JsValue>,
    compare_fn: &JsValue,
) -> Result<(), JsValue> {
    merge_sort_by(items, |x, y| {
        let v = match interp.call_function(compare_fn, &JsValue::UNDEFINED, &[x.clone(), y.clone()])
        {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => return Ok(std::cmp::Ordering::Equal),
        };
        let n = interp.to_number_value(&v)?;
        Ok(n.partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal))
    })
}

// Stable bottom-up merge sort driven by a fallible comparator. A user
// comparefn need not be a consistent comparator (the resulting order is then
// implementation-defined), and `slice::sort_by` panics when it detects such a
// total-order violation, so user-driven sorts must go through this instead.
// Stops at the first `Err`, leaving `items` a permutation of its input.
fn merge_sort_by<E>(
    items: &mut Vec<JsValue>,
    mut cmp: impl FnMut(&JsValue, &JsValue) -> Result<std::cmp::Ordering, E>,
) -> Result<(), E> {
    let n = items.len();
    if n < 2 {
        return Ok(());
    }
    let mut src = std::mem::take(items);
    let mut dst: Vec<JsValue> = Vec::with_capacity(n);
    let mut width = 1;
    while width < n {
        dst.clear();
        if let Err(e) = merge_pass(&src, &mut dst, width, &mut cmp) {
            *items = src;
            return Err(e);
        }
        std::mem::swap(&mut src, &mut dst);
        width = width.saturating_mul(2);
    }
    *items = src;
    Ok(())
}

// One bottom-up pass of `merge_sort_by`: merges each pair of adjacent
// `width`-long runs of `src` into `dst`.
fn merge_pass<E>(
    src: &[JsValue],
    dst: &mut Vec<JsValue>,
    width: usize,
    cmp: &mut impl FnMut(&JsValue, &JsValue) -> Result<std::cmp::Ordering, E>,
) -> Result<(), E> {
    let n = src.len();
    let mut lo = 0;
    while lo < n {
        let mid = (lo + width).min(n);
        let hi = (lo + 2 * width).min(n);
        if mid == hi {
            dst.extend_from_slice(&src[lo..hi]);
            lo = hi;
            continue;
        }
        // Already in order: one comparison instead of a full merge, so
        // sorted input costs about n comparefn calls, not (n/2)*log2(n).
        if cmp(&src[mid - 1], &src[mid])? != std::cmp::Ordering::Greater {
            dst.extend_from_slice(&src[lo..hi]);
            lo = hi;
            continue;
        }
        // With a one-element left run that check was the merge's first
        // comparison, so reuse its result instead of calling cmp again.
        let mut known_first = (mid - lo == 1).then_some(std::cmp::Ordering::Greater);
        let (mut i, mut j) = (lo, mid);
        while i < mid && j < hi {
            let ord = match known_first.take() {
                Some(ord) => ord,
                None => cmp(&src[i], &src[j])?,
            };
            if ord == std::cmp::Ordering::Greater {
                dst.push(src[j].clone());
                j += 1;
            } else {
                dst.push(src[i].clone());
                i += 1;
            }
        }
        dst.extend_from_slice(&src[i..mid]);
        dst.extend_from_slice(&src[j..hi]);
        lo = hi;
    }
    Ok(())
}

fn obj_has_throw(interp: &mut Interpreter, o: &JsValue, key: &str) -> Result<bool, JsValue> {
    if let Some(obj_id) = o.as_object_id() {
        interp.proxy_has_property(obj_id, key)
    } else {
        Ok(false)
    }
}

// AsyncIteratorClose: call iterator.return() if it exists (best-effort)
fn close_async_iterator(interp: &mut Interpreter, iterator: &JsValue) {
    if let Some(iterator_id) = iterator.as_object_id()
        && let Completion::Normal(return_fn) =
            interp.get_object_property(iterator_id, "return", iterator)
        && interp.is_callable(&return_fn)
    {
        let _ = interp.call_function(&return_fn, iterator, &[]);
    }
}

// --- Array.fromAsync continuation-passing machinery ---
// Shared state threaded through non-blocking continuations. The object-valued
// captures live in one RootedSlots container: it is rooted for the synchronous
// first step, then pinned on the fulfill/reject handlers of a pending await or
// passed to the microtask root list for an already-settled one, so no
// persistent entry on gc_temp_roots outlives the call.
const FA_ITERATOR: usize = 0;
const FA_ARR: usize = 1;
const FA_MAP_FN: usize = 2;
const FA_THIS_ARG: usize = 3;
const FA_RESOLVE_FN: usize = 4;
const FA_REJECT_FN: usize = 5;
const FA_ARRAY_LIKE: usize = 6;
const FA_SLOT_COUNT: usize = 7;

struct FromAsyncState {
    slots: RootedSlots,
    k: u64,
    has_map: bool,
    is_array_like: bool,
    len: u64,
}

fn fa_get(interp: &Interpreter, state: &Rc<RefCell<FromAsyncState>>, slot: usize) -> JsValue {
    let slots = state.borrow().slots.clone();
    slots.get(interp, slot)
}

fn from_async_reject(
    interp: &mut Interpreter,
    state: &Rc<RefCell<FromAsyncState>>,
    error: JsValue,
    close_iter: bool,
) {
    let iterator = fa_get(interp, state, FA_ITERATOR);
    let reject_fn = fa_get(interp, state, FA_REJECT_FN);
    let is_array_like = state.borrow().is_array_like;
    if close_iter && !is_array_like {
        interp.with_gc_root_scope(|interp| {
            interp.gc_root_value(&error);
            close_async_iterator(interp, &iterator);
        });
    }
    let _ = interp.call_function(&reject_fn, &JsValue::UNDEFINED, &[error]);
}

fn from_async_resolve(interp: &mut Interpreter, state: &Rc<RefCell<FromAsyncState>>) {
    let arr = fa_get(interp, state, FA_ARR);
    let k = state.borrow().k;
    if let Err(e) = set_length_throw(interp, &arr, k as usize) {
        let reject_fn = fa_get(interp, state, FA_REJECT_FN);
        let _ = interp.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
        return;
    }
    let resolve_fn = fa_get(interp, state, FA_RESOLVE_FN);
    let _ = interp.call_function(&resolve_fn, &JsValue::UNDEFINED, &[arr]);
}

// Non-blocking Await: wraps value in a promise, then schedules the continuation.
fn from_async_attach_await(
    interp: &mut Interpreter,
    value: &JsValue,
    state: Rc<RefCell<FromAsyncState>>,
    on_fulfill: fn(&mut Interpreter, JsValue, Rc<RefCell<FromAsyncState>>),
    close_iter_on_reject: bool,
) {
    let promise = interp.promise_resolve_value(value);
    let promise_id = promise.as_object_id().unwrap_or(0);

    let pstate = interp.get_promise_state(promise_id);
    match pstate {
        Some(PromiseState::Fulfilled(v)) => {
            let value = v.clone();
            let state_c = state.clone();
            let roots = vec![state.borrow().slots.root(), value.clone()];
            interp.scheduler.enqueue_microtask((
                roots,
                Box::new(move |interp| {
                    on_fulfill(interp, value, state_c);
                    Completion::Normal(JsValue::UNDEFINED)
                }),
            ));
        }
        Some(PromiseState::Rejected(e)) => {
            let err = e.clone();
            from_async_reject(interp, &state, err, close_iter_on_reject);
        }
        Some(PromiseState::Pending) => {
            let state_f = state.clone();
            let state_r = state.clone();
            let close = close_iter_on_reject;

            let fulfill_handler = interp.create_function(JsFunction::native(
                "fromAsyncFulfill".to_string(),
                1,
                move |interp, _this, args| {
                    let v = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    on_fulfill(interp, v, state_f.clone());
                    Completion::Normal(JsValue::UNDEFINED)
                },
            ));

            let reject_handler = interp.create_function(JsFunction::native(
                "fromAsyncReject".to_string(),
                1,
                move |interp, _this, args| {
                    let e = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    from_async_reject(interp, &state_r, e, close);
                    Completion::Normal(JsValue::UNDEFINED)
                },
            ));

            let slots = state.borrow().slots.clone();
            slots.pin_on(interp, &fulfill_handler);
            slots.pin_on(interp, &reject_handler);

            if let Some(cell) = interp.get_object_cell(promise_id) {
                let mut ob = cell.borrow_mut();
                if let Some(pd) = ob.promise_data_mut() {
                    pd.is_handled = true;
                    pd.fulfill_reactions.push(PromiseReaction {
                        handler: Some(fulfill_handler),
                        target: ReactionTarget::None,
                        reaction_type: PromiseReactionType::Fulfill,
                    });
                    pd.reject_reactions.push(PromiseReaction {
                        handler: Some(reject_handler),
                        target: ReactionTarget::None,
                        reaction_type: PromiseReactionType::Reject,
                    });
                }
            }
        }
        None => {
            // Not a promise — treat as immediately fulfilled
            on_fulfill(interp, value.clone(), state);
        }
    }
}

// Iterator path: call .next() synchronously, then Await the result non-blockingly.
fn from_async_iter_step(interp: &mut Interpreter, state: Rc<RefCell<FromAsyncState>>) {
    {
        if state.borrow().k >= 0x1FFFFFFFFFFFFF {
            let err = interp.create_type_error("Array.fromAsync: too many elements");
            from_async_reject(interp, &state, err, true);
            return;
        }
    }
    let iterator = fa_get(interp, &state, FA_ITERATOR);

    let next_fn = if let Some(iterator_id) = iterator.as_object_id() {
        match interp.get_object_property(iterator_id, "next", &iterator) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => {
                from_async_reject(interp, &state, e, false);
                return;
            }
            _ => JsValue::UNDEFINED,
        }
    } else {
        JsValue::UNDEFINED
    };

    let next_result = match interp.call_function(&next_fn, &iterator, &[]) {
        Completion::Normal(v) => v,
        Completion::Throw(e) => {
            from_async_reject(interp, &state, e, false);
            return;
        }
        _ => JsValue::UNDEFINED,
    };

    from_async_attach_await(interp, &next_result, state, from_async_process_next, true);
}

// Process the awaited iterator result: check done, extract value, apply mapFn.
fn from_async_process_next(
    interp: &mut Interpreter,
    next_result: JsValue,
    state: Rc<RefCell<FromAsyncState>>,
) {
    let done = if let Some(result_id) = next_result.as_object_id() {
        match interp.get_object_property(result_id, "done", &next_result) {
            Completion::Normal(v) => interp.to_boolean_val(&v),
            _ => false,
        }
    } else {
        false
    };

    if done {
        from_async_resolve(interp, &state);
        return;
    }

    let value = if let Some(result_id) = next_result.as_object_id() {
        match interp.get_object_property(result_id, "value", &next_result) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => {
                from_async_reject(interp, &state, e, true);
                return;
            }
            _ => JsValue::UNDEFINED,
        }
    } else {
        JsValue::UNDEFINED
    };

    let has_map = state.borrow().has_map;
    if has_map {
        let map_fn = fa_get(interp, &state, FA_MAP_FN);
        let this_arg = fa_get(interp, &state, FA_THIS_ARG);
        let k = state.borrow().k;

        let mapped =
            match interp.call_function(&map_fn, &this_arg, &[value, JsValue::number(k as f64)]) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    from_async_reject(interp, &state, e, true);
                    return;
                }
                _ => JsValue::UNDEFINED,
            };
        from_async_attach_await(
            interp,
            &mapped,
            state,
            from_async_store_iter_and_continue,
            true,
        );
    } else {
        from_async_store_iter_and_continue(interp, value, state);
    }
}

// Store value in result array and proceed to next iteration.
fn from_async_store_iter_and_continue(
    interp: &mut Interpreter,
    value: JsValue,
    state: Rc<RefCell<FromAsyncState>>,
) {
    let k = state.borrow().k;
    let arr = fa_get(interp, &state, FA_ARR);
    let key_str = k.to_string();
    if let Err(e) = create_data_property_or_throw(interp, &arr, &key_str, value) {
        from_async_reject(interp, &state, e, true);
        return;
    }
    state.borrow_mut().k += 1;
    from_async_iter_step(interp, state);
}

// Array-like path: get property k, Await it.
fn from_async_arraylike_step(interp: &mut Interpreter, state: Rc<RefCell<FromAsyncState>>) {
    let (k, len) = {
        let s = state.borrow();
        (s.k, s.len)
    };
    if k >= len {
        from_async_resolve(interp, &state);
        return;
    }
    let array_like = fa_get(interp, &state, FA_ARRAY_LIKE);

    let key_str = k.to_string();
    let value = if let Some(array_like_id) = array_like.as_object_id() {
        match interp.get_object_property(array_like_id, &key_str, &array_like) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => {
                from_async_reject(interp, &state, e, false);
                return;
            }
            _ => JsValue::UNDEFINED,
        }
    } else {
        JsValue::UNDEFINED
    };

    from_async_attach_await(interp, &value, state, from_async_arraylike_process, false);
}

// Process the awaited array-like element value.
fn from_async_arraylike_process(
    interp: &mut Interpreter,
    value: JsValue,
    state: Rc<RefCell<FromAsyncState>>,
) {
    let has_map = state.borrow().has_map;
    if has_map {
        let map_fn = fa_get(interp, &state, FA_MAP_FN);
        let this_arg = fa_get(interp, &state, FA_THIS_ARG);
        let k = state.borrow().k;

        let mapped =
            match interp.call_function(&map_fn, &this_arg, &[value, JsValue::number(k as f64)]) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    from_async_reject(interp, &state, e, false);
                    return;
                }
                _ => JsValue::UNDEFINED,
            };
        from_async_attach_await(
            interp,
            &mapped,
            state,
            from_async_store_arraylike_and_continue,
            false,
        );
    } else {
        from_async_store_arraylike_and_continue(interp, value, state);
    }
}

// Store value for array-like path and advance to next index.
fn from_async_store_arraylike_and_continue(
    interp: &mut Interpreter,
    value: JsValue,
    state: Rc<RefCell<FromAsyncState>>,
) {
    let k = state.borrow().k;
    let arr = fa_get(interp, &state, FA_ARR);
    let key_str = k.to_string();
    if let Err(e) = create_data_property_or_throw(interp, &arr, &key_str, value) {
        from_async_reject(interp, &state, e, false);
        return;
    }
    state.borrow_mut().k += 1;
    from_async_arraylike_step(interp, state);
}

fn obj_delete(interp: &mut Interpreter, o: &JsValue, key: &str) {
    if let Some(cell) = get_obj(interp, o) {
        let mut borrow = cell.borrow_mut_untracked();
        borrow.remove_property(key);
        if let Some(elems) = borrow.array_elements_mut()
            && let Some(idx) = crate::interpreter::types::array_element_index(key)
            && idx < elems.len()
        {
            elems[idx] = JsValue::UNDEFINED;
        }
    }
}

// DeletePropertyOrThrow(O, P) - throws TypeError if delete fails
fn obj_delete_throw(interp: &mut Interpreter, o: &JsValue, key: &str) -> Result<(), JsValue> {
    if let Some(obj_id) = o.as_object_id() {
        // Check for Proxy deleteProperty trap
        let is_proxy = interp.get_object_cell(obj_id).is_some_and(|cell| {
            let b = cell.borrow();
            b.is_proxy() || b.is_proxy_revoked()
        });
        if is_proxy {
            match interp.proxy_delete_property(obj_id, key) {
                Ok(true) => return Ok(()),
                Ok(false) => {
                    return Err(
                        interp.create_type_error(&format!("Cannot delete property '{key}'"))
                    );
                }
                Err(e) => return Err(e),
            }
        }
        // Check if property is non-configurable
        let non_configurable = interp.get_object_cell(obj_id).is_some_and(|cell| {
            cell.borrow()
                .get_own_property(key)
                .is_some_and(|desc| desc.configurable == Some(false))
        });
        if non_configurable {
            return Err(
                interp.create_type_error(&format!("Cannot delete property '{key}' of object"))
            );
        }
    }
    obj_delete(interp, o, key);
    Ok(())
}

fn set_length(interp: &mut Interpreter, o: &JsValue, len: usize) {
    if let Some(cell) = get_obj(interp, o) {
        let mut borrow = cell.borrow_mut_untracked();
        if let Some(elems) = borrow.array_elements_mut()
            && len <= elems.len()
        {
            elems.truncate(len);
        }
        // Don't pre-allocate for sparse arrays
        borrow.set_property_value("length", JsValue::number(len as f64));
    }
}

// Set(O, "length", len, true) — uses obj_set_throw for setter invocation
fn set_length_throw(interp: &mut Interpreter, o: &JsValue, len: usize) -> Result<(), JsValue> {
    let snapshot = get_obj(interp, o).and_then(|cell| {
        let b = cell.borrow();
        b.array_elements()?;
        let length_desc = b.get_own_property("length");
        let length_writable_false = length_desc
            .as_ref()
            .is_some_and(|d| d.writable == Some(false));
        let length_frozen = length_desc
            .as_ref()
            .is_some_and(|d| d.configurable == Some(false) && d.writable == Some(false));
        Some((b.extensible, length_writable_false, length_frozen))
    });
    if let Some((extensible, length_writable_false, length_frozen)) = snapshot {
        // ArraySetLength §10.4.2.4: ToUint32(len) must equal ToNumber(len)
        let len_f64 = len as f64;
        let len_u32 = len_f64 as u32;
        if (len_u32 as f64) != len_f64 {
            return Err(interp.create_range_error("Invalid array length"));
        }
        if length_writable_false {
            return Err(interp.create_type_error("Cannot assign to read only property 'length'"));
        }
        if !extensible && length_frozen {
            return Err(interp.create_type_error("Cannot assign to read only property 'length'"));
        }
        if let Some(cell) = get_obj(interp, o) {
            let mut borrow = cell.borrow_mut_untracked();
            if let Some(elems) = borrow.array_elements_mut()
                && len <= elems.len()
            {
                elems.truncate(len);
            }
            // Don't pre-allocate for sparse arrays
            borrow.set_property_value("length", JsValue::number(len as f64));
        }
        return Ok(());
    }
    obj_set_throw(interp, o, "length", JsValue::number(len as f64))
}

fn require_callable(interp: &mut Interpreter, val: &JsValue, msg: &str) -> Result<(), Completion> {
    if !interp.is_callable(val) {
        Err(Completion::Throw(interp.create_type_error(msg)))
    } else {
        Ok(())
    }
}

// ArraySpeciesCreate (§23.1.3.7.1)
fn array_species_create(
    interp: &mut Interpreter,
    original_array: &JsValue,
    length: usize,
) -> Result<JsValue, Completion> {
    let is_array = if let Some(original_id) = original_array.as_object_id() {
        match is_array_check(interp, original_id) {
            Ok(v) => v,
            Err(e) => return Err(Completion::Throw(e)),
        }
    } else {
        false
    };
    if !is_array {
        if length as u64 > 0xFFFF_FFFF {
            return Err(Completion::Throw(
                interp.create_range_error("Invalid array length"),
            ));
        }
        // Pre-size the backing Vec (capacity only — `create_array` populates by
        // iterating, and the passed Vec is empty, so behaviour is unchanged).
        // Cap guards against hostile huge lengths, mirroring
        // `create_array_with_length` which deliberately avoids huge allocations.
        let arr = interp.create_array(Vec::with_capacity(length.min(1 << 16)));
        set_length(interp, &arr, length);
        return Ok(arr);
    }
    // Get constructor
    let mut c = if let Some(original_id) = original_array.as_object_id() {
        match interp.get_object_property(original_id, "constructor", original_array) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(Completion::Throw(e)),
            other => return Err(other),
        }
    } else {
        JsValue::UNDEFINED
    };
    // §9.4.2.3 step 5-6: If C is a constructor from a different realm and
    // equals that realm's intrinsic %Array%, set C to undefined
    if interp.is_constructor(&c) && c.is_object() {
        let c_realm = match interp.get_function_realm(&c) {
            Ok(r) => r,
            Err(e) => return Err(Completion::Throw(e)),
        };
        if c_realm != interp.current_realm_id {
            // Look up "Array" from the c_realm's global object (its env.bindings
            // had "Array" stripped at setup_globals; the binding now lives only
            // on the global object).
            let realm_array = interp.realms[c_realm]
                .global_object
                .and_then(|gid| interp.get_object_cell(gid))
                .and_then(|cell| cell.borrow().own_property_lookup("Array"));
            if let Some(ref ra) = realm_array
                && same_value(&c, ra)
            {
                c = JsValue::UNDEFINED;
            }
        }
    }
    // If C is Object, get C[@@species]
    let c = if let Some(c_id) = c.as_object_id() {
        let sym_key = interp.get_symbol_key("species");
        let species = if let Some(key) = &sym_key {
            match interp.get_object_property(c_id, key, &c) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(Completion::Throw(e)),
                other => return Err(other),
            }
        } else {
            JsValue::UNDEFINED
        };
        // If species is null, treat as undefined
        if (species).is_null() {
            JsValue::UNDEFINED
        } else {
            species
        }
    } else {
        c
    };
    // If C is undefined, create a default array
    if (c).is_undefined() {
        if length as u64 > 0xFFFF_FFFF {
            return Err(Completion::Throw(
                interp.create_range_error("Invalid array length"),
            ));
        }
        // Pre-size the backing Vec (capacity only — `create_array` populates by
        // iterating, and the passed Vec is empty, so behaviour is unchanged).
        // Cap guards against hostile huge lengths, mirroring
        // `create_array_with_length` which deliberately avoids huge allocations.
        let arr = interp.create_array(Vec::with_capacity(length.min(1 << 16)));
        set_length(interp, &arr, length);
        return Ok(arr);
    }
    // If C is not a constructor, throw TypeError
    if !interp.is_constructor(&c) {
        return Err(Completion::Throw(
            interp.create_type_error("Species constructor is not a constructor"),
        ));
    }
    // Construct(C, [length])
    match interp.construct(&c, &[JsValue::number(length as f64)]) {
        Completion::Normal(v) => Ok(v),
        Completion::Throw(e) => Err(Completion::Throw(e)),
        other => Err(other),
    }
}

impl Interpreter {
    pub(crate) fn setup_array_prototype(&mut self) {
        let proto_id = self.create_object_id();
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .class_name = "Array";
        self.get_object_cell_expect(proto_id).borrow_mut().kind =
            crate::interpreter::types::ObjectKind::Array(
                crate::interpreter::types::ArrayData::default(),
            );
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_property(
                "length".to_string(),
                PropertyDescriptor::data(JsValue::number(0.0), true, false, false),
            );

        // Array.prototype.push
        self.define_method(proto_id, "push", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let mut len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            // Step 5: If len + argCount > 2^53 - 1, throw TypeError
            if (len + args.len()) as u64 > 9007199254740991 {
                return Completion::Throw(interp.create_type_error("Invalid array length"));
            }
            // Fast path for genuine dense Arrays: append directly into the
            // backing Vec, bypassing per-element Set (proxy/setter/typed-array
            // checks + len.to_string()). Falls through to the slow loop on any
            // guard miss so proxies, array-likes/arguments, frozen/non-writable
            // -length, and sparse arrays keep correct behaviour.
            if let Some(obj_id) = o.as_object_id()
                && let Some(cell) = interp.get_object(obj_id)
            {
                let (fast_ok, proto_id) = {
                    let b = cell.borrow();
                    let len_desc = b.properties.get("length");
                    let len_writable = len_desc
                        .map(|d| d.writable() != Some(false))
                        .unwrap_or(false);
                    let desc_len = len_desc
                        .and_then(|d| d.value())
                        .and_then(|v| v.as_number())
                        .map(|n| n as usize);
                    let ok = b.class_name == "Array"
                        && !b.is_proxy()
                        && b.extensible
                        && len_desc.is_some()
                        && len_writable
                        && desc_len == Some(len)
                        && b.array_elements().map(|v| v.len()) == Some(len);
                    (ok, b.prototype_id)
                };
                // §23.1.3.25 push performs Set, which creates *present* data
                // properties. In this engine a `JsValue::UNDEFINED` slot in
                // `array_elements` with no `properties` entry is the HOLE
                // sentinel (see the indexed-write fast path in eval.rs and
                // `get_own_property_full`), so writing `undefined` straight
                // into the Vec would make `push(undefined)` create a hole
                // instead of a present element. Bail to the slow path when any
                // argument is `undefined`; the slow path adds the presence
                // marker.
                let has_undefined_arg = args.iter().any(|a| (a).is_undefined());
                // §23.1.3.25 push performs Set(O, ToString(i), …, true) =
                // OrdinarySet, which walks the prototype chain. If any
                // appended index ToString is inherited (setter or data) we
                // must invoke/honour it via the slow loop rather than write
                // straight into the Vec — bail to the slow path.
                // A bare Proxy prototype exposes no own descriptor, so the
                // get_property_descriptor_on_id probe below would miss it and
                // the fast path would write straight into the Vec, skipping the
                // proxy [[Set]] trap (issue #166). Bail to the slow path (which
                // routes through obj_set_throw → proxy_set) whenever a Proxy is
                // anywhere in the prototype chain.
                let inherited = fast_ok
                    && !has_undefined_arg
                    && proto_id.is_some_and(|pid| {
                        interp.has_proxy_in_prototype_chain(pid)
                            || (len..len + args.len()).any(|i| {
                                interp
                                    .get_property_descriptor_on_id(pid, &i.to_string())
                                    .is_some()
                            })
                    });
                if fast_ok && !has_undefined_arg && !inherited {
                    let new_len = len + args.len();
                    {
                        for (offset, arg) in args.iter().enumerate() {
                            interp.gc_write_barrier_element(&cell, len + offset, arg);
                        }
                        let mut b = cell.borrow_mut_untracked();
                        b.extend_array_elements_precise(args);
                        if let Some(len_desc) = b.properties.get_mut("length") {
                            len_desc.set_value(Some(JsValue::number(new_len as f64)));
                        }
                        b.shape_id = crate::interpreter::types::fresh_shape_id();
                    }
                    return Completion::Normal(JsValue::number(new_len as f64));
                }
            }
            for arg in args {
                if let Err(e) = obj_set_throw(interp, &o, &len.to_string(), arg.clone()) {
                    return Completion::Throw(e);
                }
                len += 1;
            }
            if let Err(e) = set_length_throw(interp, &o, len) {
                return Completion::Throw(e);
            }
            Completion::Normal(JsValue::number(len as f64))
        });

        // Array.prototype.pop
        self.define_method(proto_id, "pop", 0, |interp, this_val, _args: &[JsValue]| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            if len == 0 {
                if let Err(e) = set_length_throw(interp, &o, 0) {
                    return Completion::Throw(e);
                }
                return Completion::Normal(JsValue::UNDEFINED);
            }
            if let Some(v) = array_dense::try_pop(interp, &o, len) {
                return Completion::Normal(v);
            }
            let idx = (len - 1).to_string();
            let val = match obj_get(interp, &o, &idx) {
                Ok(v) => v,
                Err(c) => return c,
            };
            if let Err(e) = obj_delete_throw(interp, &o, &idx) {
                return Completion::Throw(e);
            }
            if let Err(e) = set_length_throw(interp, &o, len - 1) {
                return Completion::Throw(e);
            }
            Completion::Normal(val)
        });

        // Array.prototype.shift
        self.define_method(proto_id, "shift", 0, |interp, this_val, _args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            if len == 0 {
                if let Err(e) = set_length_throw(interp, &o, 0) {
                    return Completion::Throw(e);
                }
                return Completion::Normal(JsValue::UNDEFINED);
            }
            if let Some(first) = array_dense::try_shift(interp, &o, len) {
                return Completion::Normal(first);
            }
            let first = match obj_get(interp, &o, "0") {
                Ok(v) => v,
                Err(c) => return c,
            };
            for k in 1..len {
                let from = k.to_string();
                let to = (k - 1).to_string();
                let from_present = match obj_has_throw(interp, &o, &from) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                if from_present {
                    let val = match obj_get(interp, &o, &from) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    if let Err(e) = obj_set_throw(interp, &o, &to, val) {
                        return Completion::Throw(e);
                    }
                } else if let Err(e) = obj_delete_throw(interp, &o, &to) {
                    return Completion::Throw(e);
                }
            }
            if let Err(e) = obj_delete_throw(interp, &o, &(len - 1).to_string()) {
                return Completion::Throw(e);
            }
            if let Err(e) = set_length_throw(interp, &o, len - 1) {
                return Completion::Throw(e);
            }
            Completion::Normal(first)
        });

        // Array.prototype.unshift
        self.define_method(proto_id, "unshift", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let arg_count = args.len();
            // If len + argCount > 2^53-1, throw TypeError
            if (len + arg_count) as u64 > 9007199254740991 {
                return Completion::Throw(interp.create_type_error("Invalid array length"));
            }
            if let Some(new_len) = array_dense::try_unshift(interp, &o, len, args) {
                return Completion::Normal(JsValue::number(new_len as f64));
            }
            if arg_count > 0 {
                // Shift existing elements
                for k in (0..len).rev() {
                    let from = k.to_string();
                    let to = (k + arg_count).to_string();
                    let from_present = match obj_has_throw(interp, &o, &from) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };
                    if from_present {
                        let val = match obj_get(interp, &o, &from) {
                            Ok(v) => v,
                            Err(c) => return c,
                        };
                        if let Err(e) = obj_set_throw(interp, &o, &to, val) {
                            return Completion::Throw(e);
                        }
                    } else if let Err(e) = obj_delete_throw(interp, &o, &to) {
                        return Completion::Throw(e);
                    }
                }
                for (j, arg) in args.iter().enumerate() {
                    if let Err(e) = obj_set_throw(interp, &o, &j.to_string(), arg.clone()) {
                        return Completion::Throw(e);
                    }
                }
            }
            let new_len = len + arg_count;
            if let Err(e) = set_length_throw(interp, &o, new_len) {
                return Completion::Throw(e);
            }
            Completion::Normal(JsValue::number(new_len as f64))
        });

        // Array.prototype.indexOf
        self.define_method(proto_id, "indexOf", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            if len == 0 {
                return Completion::Normal(JsValue::number(-1.0));
            }
            let search = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            let n = if args.len() >= 2 {
                match interp.to_integer_or_infinity_value(&args[1]) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                }
            } else {
                0.0
            };
            if n >= len as f64 {
                return Completion::Normal(JsValue::number(-1.0));
            }
            let k = if n >= 0.0 {
                n as usize
            } else {
                let calc = len as f64 + n;
                if calc < 0.0 { 0 } else { calc as usize }
            };
            let (found, resume) =
                array_dense::scan_forward(interp, &o, k, len, |v| strict_equality(v, &search));
            if let Some(i) = found {
                return Completion::Normal(JsValue::number(i as f64));
            }
            for i in resume..len {
                match get_if_present(interp, &o, i) {
                    Ok(Some(elem)) => {
                        if strict_equality(&elem, &search) {
                            return Completion::Normal(JsValue::number(i as f64));
                        }
                    }
                    Ok(None) => {}
                    Err(c) => return c,
                }
            }
            Completion::Normal(JsValue::number(-1.0))
        });

        // Array.prototype.lastIndexOf
        self.define_method(proto_id, "lastIndexOf", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            if len == 0 {
                return Completion::Normal(JsValue::number(-1.0));
            }
            let search = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            let n = if args.len() >= 2 {
                match interp.to_integer_or_infinity_value(&args[1]) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                }
            } else {
                len as f64 - 1.0
            };
            let k = if n >= 0.0 {
                (n as usize).min(len - 1)
            } else {
                let calc = len as f64 + n;
                if calc < 0.0 {
                    return Completion::Normal(JsValue::number(-1.0));
                }
                calc as usize
            };
            let (found, resume) =
                array_dense::scan_backward(interp, &o, k, |v| strict_equality(v, &search));
            if let Some(i) = found {
                return Completion::Normal(JsValue::number(i as f64));
            }
            for i in (0..resume).rev() {
                match get_if_present(interp, &o, i) {
                    Ok(Some(elem)) => {
                        if strict_equality(&elem, &search) {
                            return Completion::Normal(JsValue::number(i as f64));
                        }
                    }
                    Ok(None) => {}
                    Err(c) => return c,
                }
            }
            Completion::Normal(JsValue::number(-1.0))
        });

        // Array.prototype.includes
        self.define_method(proto_id, "includes", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            if len == 0 {
                return Completion::Normal(JsValue::boolean(false));
            }
            let search = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            let n = if args.len() >= 2 {
                match interp.to_integer_or_infinity_value(&args[1]) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                }
            } else {
                0.0
            };
            let k = if n >= 0.0 {
                n as usize
            } else {
                let calc = len as f64 + n;
                if calc < 0.0 { 0 } else { calc as usize }
            };
            let (found, resume) =
                array_dense::scan_forward(interp, &o, k, len, |v| same_value_zero(v, &search));
            if found.is_some() {
                return Completion::Normal(JsValue::boolean(true));
            }
            for i in resume..len {
                let elem = match get_index(interp, &o, i) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if same_value_zero(&elem, &search) {
                    return Completion::Normal(JsValue::boolean(true));
                }
            }
            Completion::Normal(JsValue::boolean(false))
        });

        // Array.prototype.join
        self.define_method(proto_id, "join", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let sep = match args.first() {
                Some(s) if !s.is_undefined() => match interp.to_js_string(s) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                },
                _ => JsString::from_str(","),
            };

            let receiver_id = o.as_object_id().expect("ToObject must return an object");
            if interp.active_array_joins.contains(&receiver_id) {
                return Completion::Normal(JsValue::string(JsString::from_str("")));
            }

            interp.active_array_joins.push(receiver_id);
            let result = (|| {
                let mut units: Vec<u16> = Vec::new();
                for i in 0..len {
                    if i > 0
                        && let Err(e) = interp.append_code_units(&mut units, sep.as_units())
                    {
                        return Completion::Throw(e);
                    }
                    let elem = match interp.numeric_index_fast_get(&o, i as f64) {
                        Some(v) => v,
                        None => match obj_get(interp, &o, &i.to_string()) {
                            Ok(v) => v,
                            Err(c) => return c,
                        },
                    };
                    if !elem.is_nullish()
                        && let Err(e) = interp.append_js_string(&mut units, &elem)
                    {
                        return Completion::Throw(e);
                    }
                }
                Completion::Normal(JsValue::string(JsString::from_vec(units)))
            })();
            let popped_receiver = interp.active_array_joins.pop();
            debug_assert_eq!(popped_receiver, Some(receiver_id));
            result
        });

        // Array.prototype.toString
        self.define_method(proto_id, "toString", 0, |interp, this_val, _args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            // Look for a "join" method
            let join_fn = match obj_get(interp, &o, "join") {
                Ok(v) => v,
                Err(c) => return c,
            };
            if interp.is_callable(&join_fn) {
                return interp.call_function(&join_fn, &o, &[]);
            }
            // Fall back to intrinsic %Object.prototype.toString%
            if let Some(intrinsic_tostring) = interp.realm().object_prototype_tostring.clone() {
                return interp.call_function(&intrinsic_tostring, &o, &[]);
            }
            Completion::Normal(JsValue::string(JsString::from_str("[object Array]")))
        });

        // Array.prototype.toLocaleString
        self.define_method(proto_id, "toLocaleString", 0, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let locales = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            let options = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
            let pass_args = [locales, options];
            let mut result: Vec<u16> = Vec::new();
            for k in 0..len {
                if k > 0
                    && let Err(e) = interp.append_code_units(&mut result, &[u16::from(b',')])
                {
                    return Completion::Throw(e);
                }
                let next_element = match obj_get(interp, &o, &k.to_string()) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if !next_element.is_nullish()
                    && let Err(c) =
                        append_locale_string(interp, &mut result, &next_element, &pass_args)
                {
                    return c;
                }
            }
            Completion::Normal(JsValue::string(JsString::from_vec(result)))
        });

        // Array.prototype.concat
        self.define_method(proto_id, "concat", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let a = match array_species_create(interp, &o, 0) {
                Ok(v) => v,
                Err(c) => return c,
            };
            interp.with_gc_root_scope(|interp| {
                interp.gc_root_value(&a);
                let mut n: usize = 0;
                let items: Vec<JsValue> = std::iter::once(o).chain(args.iter().cloned()).collect();
                for item in &items {
                    // IsConcatSpreadable (§23.1.3.1.1)
                    let spreadable = if let Some(item_id) = item.as_object_id() {
                        let sym_key = interp.get_symbol_key("isConcatSpreadable");
                        let spreadable_val = if let Some(key) = &sym_key {
                            match interp.get_object_property(item_id, key, item) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                other => return other,
                            }
                        } else {
                            JsValue::UNDEFINED
                        };
                        if !(spreadable_val).is_undefined() {
                            interp.to_boolean_val(&spreadable_val)
                        } else {
                            // 4. Return ? IsArray(O)
                            match is_array_check(interp, item_id) {
                                Ok(v) => v,
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                    } else {
                        false
                    };
                    if spreadable {
                        let len = match length_of_array_like(interp, item) {
                            Ok(v) => v,
                            Err(c) => return c,
                        };
                        if (n as u64) + (len as u64) > 9007199254740991 {
                            return Completion::Throw(
                                interp
                                    .create_type_error("Array length exceeds the allowed maximum"),
                            );
                        }
                        if array_dense::try_concat_item(interp, &a, item, n, len) {
                            n += len;
                            continue;
                        }
                        for k in 0..len {
                            match get_if_present(interp, item, k) {
                                Ok(Some(val)) => {
                                    if let Err(e) = create_data_property_at(interp, &a, n, val) {
                                        return Completion::Throw(e);
                                    }
                                }
                                Ok(None) => {}
                                Err(c) => return c,
                            }
                            n += 1;
                        }
                    } else {
                        if n as u64 >= 9007199254740991 {
                            return Completion::Throw(
                                interp
                                    .create_type_error("Array length exceeds the allowed maximum"),
                            );
                        }
                        if let Err(e) = create_data_property_at(interp, &a, n, item.clone()) {
                            return Completion::Throw(e);
                        }
                        n += 1;
                    }
                }
                if let Err(e) = obj_set_throw(interp, &a, "length", JsValue::number(n as f64)) {
                    return Completion::Throw(e);
                }
                Completion::Normal(a)
            })
        });

        // Array.prototype.slice
        self.define_method(proto_id, "slice", 2, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            } as i64;
            let k = match resolve_start_index(interp, args.first(), len as usize) {
                Ok(k) => k,
                Err(c) => return c,
            };
            let fin = match resolve_end_index(interp, args.get(1), len as usize) {
                Ok(f) => f,
                Err(c) => return c,
            };
            let count = fin.saturating_sub(k);
            let a = match array_species_create(interp, &o, count) {
                Ok(v) => v,
                Err(c) => return c,
            };
            interp.with_gc_root_scope(|interp| {
                interp.gc_root_value(&a);
                if array_dense::try_slice(interp, &o, &a, k, fin) {
                    return Completion::Normal(a);
                }
                let mut n: usize = 0;
                for i in k..fin {
                    match get_if_present(interp, &o, i) {
                        Ok(Some(val)) => {
                            if let Err(e) = create_data_property_at(interp, &a, n, val) {
                                return Completion::Throw(e);
                            }
                        }
                        Ok(None) => {}
                        Err(c) => return c,
                    }
                    n += 1;
                }
                if let Err(e) = obj_set_throw(interp, &a, "length", JsValue::number(n as f64)) {
                    return Completion::Throw(e);
                }
                Completion::Normal(a)
            })
        });

        // Array.prototype.reverse
        self.define_method(proto_id, "reverse", 0, |interp, this_val, _args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            if array_dense::try_reverse(interp, &o, len) {
                return Completion::Normal(o);
            }
            let middle = len / 2;
            for lower in 0..middle {
                let upper = len - lower - 1;
                let lower_s = lower.to_string();
                let upper_s = upper.to_string();
                // Spec §22.1.3.28 step 7: Has then Get for lower, then Has then Get for upper
                let lower_exists = match obj_has_throw(interp, &o, &lower_s) {
                    Ok(b) => b,
                    Err(e) => return Completion::Throw(e),
                };
                let lower_val = if lower_exists {
                    match obj_get(interp, &o, &lower_s) {
                        Ok(v) => v,
                        Err(c) => return c,
                    }
                } else {
                    JsValue::UNDEFINED
                };
                let upper_exists = match obj_has_throw(interp, &o, &upper_s) {
                    Ok(b) => b,
                    Err(e) => return Completion::Throw(e),
                };
                let upper_val = if upper_exists {
                    match obj_get(interp, &o, &upper_s) {
                        Ok(v) => v,
                        Err(c) => return c,
                    }
                } else {
                    JsValue::UNDEFINED
                };
                if lower_exists && upper_exists {
                    if let Err(e) = obj_set_throw(interp, &o, &lower_s, upper_val) {
                        return Completion::Throw(e);
                    }
                    if let Err(e) = obj_set_throw(interp, &o, &upper_s, lower_val) {
                        return Completion::Throw(e);
                    }
                } else if !lower_exists && upper_exists {
                    if let Err(e) = obj_set_throw(interp, &o, &lower_s, upper_val) {
                        return Completion::Throw(e);
                    }
                    if let Err(e) = obj_delete_throw(interp, &o, &upper_s) {
                        return Completion::Throw(e);
                    }
                } else if lower_exists && !upper_exists {
                    if let Err(e) = obj_delete_throw(interp, &o, &lower_s) {
                        return Completion::Throw(e);
                    }
                    if let Err(e) = obj_set_throw(interp, &o, &upper_s, lower_val) {
                        return Completion::Throw(e);
                    }
                }
            }
            Completion::Normal(o)
        });

        // Array.prototype.toReversed
        self.define_method(proto_id, "toReversed", 0, |interp, this_val, _args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            if len as u64 > 0xFFFF_FFFF {
                return Completion::Throw(interp.create_range_error("Invalid array length"));
            }
            let mut result = Vec::with_capacity(len);
            for i in (0..len).rev() {
                result.push(match obj_get(interp, &o, &i.to_string()) {
                    Ok(v) => v,
                    Err(c) => return c,
                });
            }
            let arr = interp.create_array(result);
            Completion::Normal(arr)
        });

        // Array.prototype.forEach
        self.define_method(proto_id, "forEach", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let callback = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            if let Err(c) =
                require_callable(interp, &callback, "forEach callback is not a function")
            {
                return c;
            }
            let this_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
            for k in 0..len {
                match get_if_present(interp, &o, k) {
                    Ok(Some(kvalue)) => {
                        if let result @ Completion::Throw(_) = interp.call_function(
                            &callback,
                            &this_arg,
                            &[kvalue, JsValue::number(k as f64), o.clone()],
                        ) {
                            return result;
                        }
                    }
                    Ok(None) => {}
                    Err(c) => return c,
                }
            }
            Completion::Normal(JsValue::UNDEFINED)
        });

        // Array.prototype.map
        self.define_method(proto_id, "map", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let callback = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            if let Err(c) = require_callable(interp, &callback, "map callback is not a function") {
                return c;
            }
            let this_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
            let a = match array_species_create(interp, &o, len) {
                Ok(v) => v,
                Err(c) => return c,
            };
            interp.with_gc_root_scope(|interp| {
                interp.gc_root_value(&a);
                for k in 0..len {
                    match get_if_present(interp, &o, k) {
                        Ok(Some(kvalue)) => {
                            match interp.call_function(
                                &callback,
                                &this_arg,
                                &[kvalue, JsValue::number(k as f64), o.clone()],
                            ) {
                                Completion::Normal(v) => {
                                    if let Err(e) = create_data_property_at(interp, &a, k, v) {
                                        return Completion::Throw(e);
                                    }
                                }
                                other => return other,
                            }
                        }
                        Ok(None) => {}
                        Err(c) => return c,
                    }
                }
                Completion::Normal(a)
            })
        });

        // Array.prototype.filter
        self.define_method(proto_id, "filter", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let callback = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            if let Err(c) = require_callable(interp, &callback, "filter callback is not a function")
            {
                return c;
            }
            let this_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
            let a = match array_species_create(interp, &o, 0) {
                Ok(v) => v,
                Err(c) => return c,
            };
            interp.with_gc_root_scope(|interp| {
                interp.gc_root_value(&a);
                let mut to: usize = 0;
                for k in 0..len {
                    match get_if_present(interp, &o, k) {
                        Ok(Some(kvalue)) => {
                            match interp.call_function(
                                &callback,
                                &this_arg,
                                &[kvalue.clone(), JsValue::number(k as f64), o.clone()],
                            ) {
                                Completion::Normal(v) => {
                                    if interp.to_boolean_val(&v) {
                                        if let Err(e) =
                                            create_data_property_at(interp, &a, to, kvalue)
                                        {
                                            return Completion::Throw(e);
                                        }
                                        to += 1;
                                    }
                                }
                                other => return other,
                            }
                        }
                        Ok(None) => {}
                        Err(c) => return c,
                    }
                }
                Completion::Normal(a)
            })
        });

        // Array.prototype.reduce
        self.define_method(proto_id, "reduce", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let callback = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            if let Err(c) = require_callable(interp, &callback, "reduce callback is not a function")
            {
                return c;
            }
            if len == 0 && args.len() < 2 {
                return Completion::Throw(
                    interp.create_type_error("Reduce of empty array with no initial value"),
                );
            }
            let mut k = 0usize;
            let mut acc = if args.len() >= 2 {
                args[1].clone()
            } else {
                // Find first present element
                loop {
                    if k >= len {
                        return Completion::Throw(
                            interp.create_type_error("Reduce of empty array with no initial value"),
                        );
                    }
                    match get_if_present(interp, &o, k) {
                        Ok(Some(val)) => {
                            k += 1;
                            break val;
                        }
                        Ok(None) => {}
                        Err(c) => return c,
                    }
                    k += 1;
                }
            };
            while k < len {
                match get_if_present(interp, &o, k) {
                    Ok(Some(kvalue)) => {
                        match interp.call_function(
                            &callback,
                            &JsValue::UNDEFINED,
                            &[acc, kvalue, JsValue::number(k as f64), o.clone()],
                        ) {
                            Completion::Normal(v) => acc = v,
                            other => return other,
                        }
                    }
                    Ok(None) => {}
                    Err(c) => return c,
                }
                k += 1;
            }
            Completion::Normal(acc)
        });

        // Array.prototype.reduceRight
        self.define_method(proto_id, "reduceRight", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let callback = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            if let Err(c) =
                require_callable(interp, &callback, "reduceRight callback is not a function")
            {
                return c;
            }
            if len == 0 && args.len() < 2 {
                return Completion::Throw(
                    interp.create_type_error("Reduce of empty array with no initial value"),
                );
            }
            let mut k = len as i64 - 1;
            let mut acc = if args.len() >= 2 {
                args[1].clone()
            } else {
                loop {
                    if k < 0 {
                        return Completion::Throw(
                            interp.create_type_error("Reduce of empty array with no initial value"),
                        );
                    }
                    match get_if_present(interp, &o, k as usize) {
                        Ok(Some(val)) => {
                            k -= 1;
                            break val;
                        }
                        Ok(None) => {}
                        Err(c) => return c,
                    }
                    k -= 1;
                }
            };
            while k >= 0 {
                match get_if_present(interp, &o, k as usize) {
                    Ok(Some(kvalue)) => {
                        match interp.call_function(
                            &callback,
                            &JsValue::UNDEFINED,
                            &[acc, kvalue, JsValue::number(k as f64), o.clone()],
                        ) {
                            Completion::Normal(v) => acc = v,
                            other => return other,
                        }
                    }
                    Ok(None) => {}
                    Err(c) => return c,
                }
                k -= 1;
            }
            Completion::Normal(acc)
        });

        // Array.prototype.some
        self.define_method(proto_id, "some", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let callback = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            if let Err(c) = require_callable(interp, &callback, "some callback is not a function") {
                return c;
            }
            let this_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
            for k in 0..len {
                match get_if_present(interp, &o, k) {
                    Ok(Some(kvalue)) => {
                        match interp.call_function(
                            &callback,
                            &this_arg,
                            &[kvalue, JsValue::number(k as f64), o.clone()],
                        ) {
                            Completion::Normal(v) => {
                                if interp.to_boolean_val(&v) {
                                    return Completion::Normal(JsValue::boolean(true));
                                }
                            }
                            other => return other,
                        }
                    }
                    Ok(None) => {}
                    Err(c) => return c,
                }
            }
            Completion::Normal(JsValue::boolean(false))
        });

        // Array.prototype.every
        self.define_method(proto_id, "every", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let callback = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            if let Err(c) = require_callable(interp, &callback, "every callback is not a function")
            {
                return c;
            }
            let this_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
            for k in 0..len {
                match get_if_present(interp, &o, k) {
                    Ok(Some(kvalue)) => {
                        match interp.call_function(
                            &callback,
                            &this_arg,
                            &[kvalue, JsValue::number(k as f64), o.clone()],
                        ) {
                            Completion::Normal(v) => {
                                if !interp.to_boolean_val(&v) {
                                    return Completion::Normal(JsValue::boolean(false));
                                }
                            }
                            other => return other,
                        }
                    }
                    Ok(None) => {}
                    Err(c) => return c,
                }
            }
            Completion::Normal(JsValue::boolean(true))
        });

        // Array.prototype.find
        self.define_method(proto_id, "find", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let callback = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            if let Err(c) = require_callable(interp, &callback, "find predicate is not a function")
            {
                return c;
            }
            let this_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
            for k in 0..len {
                let kvalue = match get_index(interp, &o, k) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                match interp.call_function(
                    &callback,
                    &this_arg,
                    &[kvalue.clone(), JsValue::number(k as f64), o.clone()],
                ) {
                    Completion::Normal(v) => {
                        if interp.to_boolean_val(&v) {
                            return Completion::Normal(kvalue);
                        }
                    }
                    other => return other,
                }
            }
            Completion::Normal(JsValue::UNDEFINED)
        });

        // Array.prototype.findIndex
        self.define_method(proto_id, "findIndex", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let callback = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            if let Err(c) =
                require_callable(interp, &callback, "findIndex predicate is not a function")
            {
                return c;
            }
            let this_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
            for k in 0..len {
                let kvalue = match get_index(interp, &o, k) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                match interp.call_function(
                    &callback,
                    &this_arg,
                    &[kvalue, JsValue::number(k as f64), o.clone()],
                ) {
                    Completion::Normal(v) => {
                        if interp.to_boolean_val(&v) {
                            return Completion::Normal(JsValue::number(k as f64));
                        }
                    }
                    other => return other,
                }
            }
            Completion::Normal(JsValue::number(-1.0))
        });

        // Array.prototype.findLast
        self.define_method(proto_id, "findLast", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let callback = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            if let Err(c) =
                require_callable(interp, &callback, "findLast predicate is not a function")
            {
                return c;
            }
            let this_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
            for k in (0..len).rev() {
                let kvalue = match get_index(interp, &o, k) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                match interp.call_function(
                    &callback,
                    &this_arg,
                    &[kvalue.clone(), JsValue::number(k as f64), o.clone()],
                ) {
                    Completion::Normal(v) => {
                        if interp.to_boolean_val(&v) {
                            return Completion::Normal(kvalue);
                        }
                    }
                    other => return other,
                }
            }
            Completion::Normal(JsValue::UNDEFINED)
        });

        // Array.prototype.findLastIndex
        self.define_method(proto_id, "findLastIndex", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let callback = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            if let Err(c) = require_callable(
                interp,
                &callback,
                "findLastIndex predicate is not a function",
            ) {
                return c;
            }
            let this_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
            for k in (0..len).rev() {
                let kvalue = match get_index(interp, &o, k) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                match interp.call_function(
                    &callback,
                    &this_arg,
                    &[kvalue, JsValue::number(k as f64), o.clone()],
                ) {
                    Completion::Normal(v) => {
                        if interp.to_boolean_val(&v) {
                            return Completion::Normal(JsValue::number(k as f64));
                        }
                    }
                    other => return other,
                }
            }
            Completion::Normal(JsValue::number(-1.0))
        });

        // Array.prototype.splice
        self.define_method(proto_id, "splice", 2, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v as i64,
                Err(c) => return c,
            };
            let actual_start = match resolve_start_index(interp, args.first(), len as usize) {
                Ok(k) => k,
                Err(c) => return c,
            };
            let insert_count = if args.len() > 2 { args.len() - 2 } else { 0 };
            let actual_delete_count = if args.is_empty() {
                0usize
            } else if args.len() == 1 {
                (len - actual_start as i64) as usize
            } else {
                let dc = match interp.to_integer_or_infinity_value(&args[1]) {
                    Ok(n) => n,
                    Err(e) => return Completion::Throw(e),
                };
                dc.max(0.0).min((len - actual_start as i64) as f64) as usize
            };
            // Step 8: If len + insertCount - actualDeleteCount > 2^53-1, throw TypeError
            if (len as i128) + (insert_count as i128) - (actual_delete_count as i128)
                > 9007199254740991
            {
                return Completion::Throw(
                    interp.create_type_error("Array length exceeds the allowed maximum"),
                );
            }
            let a = match array_species_create(interp, &o, actual_delete_count) {
                Ok(v) => v,
                Err(c) => return c,
            };
            interp.with_gc_root_scope(|interp| {
                interp.gc_root_value(&a);
                if array_dense::try_splice(
                    interp,
                    &o,
                    &a,
                    len as usize,
                    actual_start,
                    actual_delete_count,
                    args.get(2..).unwrap_or(&[]),
                ) {
                    return Completion::Normal(a);
                }
                for i in 0..actual_delete_count {
                    let from = (actual_start + i).to_string();
                    match obj_has_throw(interp, &o, &from) {
                        Ok(true) => {
                            let val = match obj_get(interp, &o, &from) {
                                Ok(v) => v,
                                Err(c) => return c,
                            };
                            if let Err(e) =
                                create_data_property_or_throw(interp, &a, &i.to_string(), val)
                            {
                                return Completion::Throw(e);
                            }
                        }
                        Err(e) => return Completion::Throw(e),
                        _ => {}
                    }
                }
                // Step 12: Perform ? Set(A, "length", actualDeleteCount, true).
                if let Err(e) = obj_set_throw(
                    interp,
                    &a,
                    "length",
                    JsValue::number(actual_delete_count as f64),
                ) {
                    return Completion::Throw(e);
                }
                let items: Vec<JsValue> = args.iter().skip(2).cloned().collect();
                if insert_count < actual_delete_count {
                    for k in actual_start..((len as usize) - actual_delete_count) {
                        let from = (k + actual_delete_count).to_string();
                        let to = (k + insert_count).to_string();
                        let from_present = match obj_has_throw(interp, &o, &from) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };
                        if from_present {
                            let val = match obj_get(interp, &o, &from) {
                                Ok(v) => v,
                                Err(c) => return c,
                            };
                            if let Err(e) = obj_set_throw(interp, &o, &to, val) {
                                return Completion::Throw(e);
                            }
                        } else if let Err(e) = obj_delete_throw(interp, &o, &to) {
                            return Completion::Throw(e);
                        }
                    }
                    for k in
                        ((len as usize - actual_delete_count + insert_count)..(len as usize)).rev()
                    {
                        if let Err(e) = obj_delete_throw(interp, &o, &k.to_string()) {
                            return Completion::Throw(e);
                        }
                    }
                } else if insert_count > actual_delete_count {
                    for k in (actual_start..((len as usize) - actual_delete_count)).rev() {
                        let from = (k + actual_delete_count).to_string();
                        let to = (k + insert_count).to_string();
                        let from_present = match obj_has_throw(interp, &o, &from) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };
                        if from_present {
                            let val = match obj_get(interp, &o, &from) {
                                Ok(v) => v,
                                Err(c) => return c,
                            };
                            if let Err(e) = obj_set_throw(interp, &o, &to, val) {
                                return Completion::Throw(e);
                            }
                        } else if let Err(e) = obj_delete_throw(interp, &o, &to) {
                            return Completion::Throw(e);
                        }
                    }
                }
                for (j, item) in items.into_iter().enumerate() {
                    if let Err(e) = obj_set_throw(interp, &o, &(actual_start + j).to_string(), item)
                    {
                        return Completion::Throw(e);
                    }
                }
                let new_len = (len as usize) - actual_delete_count + insert_count;
                if let Err(e) = set_length_throw(interp, &o, new_len) {
                    return Completion::Throw(e);
                }
                Completion::Normal(a)
            })
        });

        // Array.prototype.toSpliced
        self.define_method(proto_id, "toSpliced", 2, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v as i64,
                Err(c) => return c,
            };
            let actual_start = match resolve_start_index(interp, args.first(), len as usize) {
                Ok(k) => k,
                Err(c) => return c,
            };
            let actual_delete_count = if args.is_empty() {
                0usize
            } else if args.len() == 1 {
                (len - actual_start as i64) as usize
            } else {
                let dc = match interp.to_integer_or_infinity_value(&args[1]) {
                    Ok(n) => n,
                    Err(e) => return Completion::Throw(e),
                };
                dc.max(0.0).min((len - actual_start as i64) as f64) as usize
            };
            let items: Vec<JsValue> = args.iter().skip(2).cloned().collect();
            let new_len = (len as i128) - (actual_delete_count as i128) + (items.len() as i128);
            // Step 12: If newLen > 2^53 - 1, throw TypeError
            if new_len > 9007199254740991 {
                return Completion::Throw(
                    interp.create_type_error("Array length exceeds the allowed maximum"),
                );
            }
            let new_len = new_len as usize;
            // Step 13: ArrayCreate(newLen) — If newLen > 2^32-1, throw RangeError
            if new_len as u64 > 0xFFFF_FFFF {
                return Completion::Throw(interp.create_range_error("Invalid array length"));
            }
            let mut result = Vec::with_capacity(new_len);
            for i in 0..actual_start {
                result.push(match obj_get(interp, &o, &i.to_string()) {
                    Ok(v) => v,
                    Err(c) => return c,
                });
            }
            result.extend(items);
            for i in (actual_start + actual_delete_count)..(len as usize) {
                result.push(match obj_get(interp, &o, &i.to_string()) {
                    Ok(v) => v,
                    Err(c) => return c,
                });
            }
            Completion::Normal(interp.create_array(result))
        });

        // Array.prototype.fill
        self.define_method(proto_id, "fill", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v as i64,
                Err(c) => return c,
            };
            let value = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            let k = match resolve_start_index(interp, args.get(1), len as usize) {
                Ok(k) => k,
                Err(c) => return c,
            };
            let fin = match resolve_end_index(interp, args.get(2), len as usize) {
                Ok(f) => f,
                Err(c) => return c,
            };
            if array_dense::try_fill(interp, &o, len as usize, k, fin, &value) {
                return Completion::Normal(o);
            }
            for i in k..fin {
                if let Err(e) = obj_set_throw(interp, &o, &i.to_string(), value.clone()) {
                    return Completion::Throw(e);
                }
            }
            Completion::Normal(o)
        });

        // Array.prototype.sort
        self.define_method(proto_id, "sort", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let compare_fn = match sort_compare_fn_arg(interp, args) {
                Ok(cf) => cf,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            // Gathered elements live only in `items` until written back, and
            // getters, the comparator, ToString and setters can all reach GC
            // safepoints, so keep them rooted from gather through write-back.
            interp.with_gc_root_scope(|interp| {
                interp.gc_root_value(&o);
                if let Some(cf) = &compare_fn {
                    interp.gc_root_value(cf);
                }
                let mut items = match gather_rooted_elements(interp, &o, len, true) {
                    Ok(items) => items,
                    Err(c) => return c,
                };
                if let Err(e) = sort_values(interp, &mut items, compare_fn.as_ref()) {
                    return Completion::Throw(e);
                }
                // Write back
                if array_dense::try_store_sorted(interp, &o, len, &items) {
                    return Completion::Normal(o);
                }
                let item_count = items.len();
                for (i, v) in items.into_iter().enumerate() {
                    if let Err(e) = obj_set_throw(interp, &o, &i.to_string(), v) {
                        return Completion::Throw(e);
                    }
                }
                for i in item_count..len {
                    if let Err(e) = obj_delete_throw(interp, &o, &i.to_string()) {
                        return Completion::Throw(e);
                    }
                }
                Completion::Normal(o)
            })
        });

        // Array.prototype.toSorted
        self.define_method(proto_id, "toSorted", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let compare_fn = match sort_compare_fn_arg(interp, args) {
                Ok(cf) => cf,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            // ArrayCreate(len) — If len > 2^32-1, throw RangeError
            if len as u64 > 0xFFFF_FFFF {
                return Completion::Throw(interp.create_range_error("Invalid array length"));
            }
            // Gathered elements live only in `items` until the result array is
            // created, so keep them rooted across later getters and the sort.
            interp.with_gc_root_scope(|interp| {
                interp.gc_root_value(&o);
                if let Some(cf) = &compare_fn {
                    interp.gc_root_value(cf);
                }
                let mut items = match gather_rooted_elements(interp, &o, len, false) {
                    Ok(items) => items,
                    Err(c) => return c,
                };
                if let Err(e) = sort_values(interp, &mut items, compare_fn.as_ref()) {
                    return Completion::Throw(e);
                }
                Completion::Normal(interp.create_array(items))
            })
        });

        // Array.prototype.flat
        self.define_method(proto_id, "flat", 0, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let depth_num = if let Some(d) = args.first() {
                if (d).is_undefined() {
                    1.0
                } else {
                    match interp.to_integer_or_infinity_value(d) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    }
                }
            } else {
                1.0
            };
            let depth = if depth_num < 0.0 {
                0i64
            } else {
                depth_num as i64
            };
            // Step 5: ArraySpeciesCreate BEFORE flattening (spec order)
            let a = match array_species_create(interp, &o, 0) {
                Ok(v) => v,
                Err(c) => return c,
            };
            interp.with_gc_root_scope(|interp| {
                interp.gc_root_value(&a);
                fn flatten_into(
                    interp: &mut Interpreter,
                    target: &JsValue,
                    target_index: &mut usize,
                    source: &JsValue,
                    source_len: usize,
                    depth: i64,
                ) -> Result<(), Completion> {
                    for k in 0..source_len {
                        let pk = k.to_string();
                        let exists =
                            obj_has_throw(interp, source, &pk).map_err(Completion::Throw)?;
                        if exists {
                            let elem = match obj_get(interp, source, &pk) {
                                Ok(v) => v,
                                Err(c) => return Err(c),
                            };
                            let should_flatten = if depth > 0 {
                                if let Some(elem_id) = elem.as_object_id() {
                                    is_array_check(interp, elem_id).map_err(Completion::Throw)?
                                } else {
                                    false
                                }
                            } else {
                                false
                            };
                            if should_flatten {
                                let elem_len = length_of_array_like(interp, &elem)?;
                                flatten_into(
                                    interp,
                                    target,
                                    target_index,
                                    &elem,
                                    elem_len,
                                    depth - 1,
                                )?;
                            } else {
                                create_data_property_or_throw(
                                    interp,
                                    target,
                                    &target_index.to_string(),
                                    elem,
                                )
                                .map_err(Completion::Throw)?;
                                *target_index += 1;
                            }
                        }
                    }
                    Ok(())
                }
                let mut target_index = 0usize;
                if let Err(c) = flatten_into(interp, &a, &mut target_index, &o, len, depth) {
                    return c;
                }
                Completion::Normal(a)
            })
        });

        // Array.prototype.flatMap
        self.define_method(proto_id, "flatMap", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let callback = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            if let Err(c) =
                require_callable(interp, &callback, "flatMap callback is not a function")
            {
                return c;
            }
            let this_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
            // Step 5: ArraySpeciesCreate BEFORE iteration (spec order)
            let a = match array_species_create(interp, &o, 0) {
                Ok(v) => v,
                Err(c) => return c,
            };
            interp.with_gc_root_scope(|interp| {
                interp.gc_root_value(&a);
                let mut target_index = 0usize;
                for k in 0..len {
                    match get_if_present(interp, &o, k) {
                        Ok(Some(kvalue)) => {
                            let mapped = interp.call_function(
                                &callback,
                                &this_arg,
                                &[kvalue, JsValue::number(k as f64), o.clone()],
                            );
                            match mapped {
                                Completion::Normal(v) => {
                                    let elem_is_array = if let Some(elem_id) = v.as_object_id() {
                                        match is_array_check(interp, elem_id) {
                                            Ok(b) => b,
                                            Err(e) => return Completion::Throw(e),
                                        }
                                    } else {
                                        false
                                    };
                                    if elem_is_array {
                                        let mlen = match length_of_array_like(interp, &v) {
                                            Ok(l) => l,
                                            Err(c) => return c,
                                        };
                                        for j in 0..mlen {
                                            let jpk = j.to_string();
                                            match obj_has_throw(interp, &v, &jpk) {
                                                Ok(true) => {
                                                    let jval = match obj_get(interp, &v, &jpk) {
                                                        Ok(v) => v,
                                                        Err(c) => return c,
                                                    };
                                                    if let Err(e) = create_data_property_or_throw(
                                                        interp,
                                                        &a,
                                                        &target_index.to_string(),
                                                        jval,
                                                    ) {
                                                        return Completion::Throw(e);
                                                    }
                                                    target_index += 1;
                                                }
                                                Err(e) => return Completion::Throw(e),
                                                _ => {}
                                            }
                                        }
                                    } else {
                                        if let Err(e) = create_data_property_or_throw(
                                            interp,
                                            &a,
                                            &target_index.to_string(),
                                            v,
                                        ) {
                                            return Completion::Throw(e);
                                        }
                                        target_index += 1;
                                    }
                                }
                                other => return other,
                            }
                        }
                        Ok(None) => {}
                        Err(c) => return c,
                    }
                }
                Completion::Normal(a)
            })
        });

        // Array.prototype.copyWithin
        self.define_method(proto_id, "copyWithin", 2, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v as i64,
                Err(c) => return c,
            };
            let to_val = match resolve_start_index(interp, args.first(), len as usize) {
                Ok(k) => k as i64,
                Err(c) => return c,
            };
            let from = match resolve_start_index(interp, args.get(1), len as usize) {
                Ok(k) => k as i64,
                Err(c) => return c,
            };
            let fin = match resolve_end_index(interp, args.get(2), len as usize) {
                Ok(f) => f as i64,
                Err(c) => return c,
            };
            let count = (fin - from).min(len - to_val);
            if count <= 0 {
                return Completion::Normal(o);
            }
            let count = count as usize;
            let (mut from_idx, mut to_idx, direction): (i64, i64, i64) =
                if from < to_val && to_val < from + count as i64 {
                    (from + count as i64 - 1, to_val + count as i64 - 1, -1)
                } else {
                    (from, to_val, 1)
                };
            for _ in 0..count {
                let from_s = (from_idx as usize).to_string();
                let to_s = (to_idx as usize).to_string();
                let from_present = match obj_has_throw(interp, &o, &from_s) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                if from_present {
                    let val = match obj_get(interp, &o, &from_s) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    if let Err(e) = obj_set_throw(interp, &o, &to_s, val) {
                        return Completion::Throw(e);
                    }
                } else if let Err(e) = obj_delete_throw(interp, &o, &to_s) {
                    return Completion::Throw(e);
                }
                from_idx += direction;
                to_idx += direction;
            }
            Completion::Normal(o)
        });

        // Array.prototype.at
        self.define_method(proto_id, "at", 1, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let relative_index = if let Some(v) = args.first() {
                match interp.to_integer_or_infinity_value(v) {
                    Ok(n) => n,
                    Err(e) => return Completion::Throw(e),
                }
            } else {
                0.0
            };
            let k = match resolve_element_index(relative_index, len) {
                Some(k) => k,
                None => return Completion::Normal(JsValue::UNDEFINED),
            };
            match obj_get(interp, &o, &k.to_string()) {
                Ok(v) => Completion::Normal(v),
                Err(c) => c,
            }
        });

        // Array.prototype.with
        self.define_method(proto_id, "with", 2, |interp, this_val, args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            let len = match length_of_array_like(interp, &o) {
                Ok(v) => v,
                Err(c) => return c,
            };
            if len as u64 > 0xFFFF_FFFF {
                return Completion::Throw(interp.create_range_error("Invalid array length"));
            }
            let relative_index = if let Some(v) = args.first() {
                match interp.to_integer_or_infinity_value(v) {
                    Ok(n) => n,
                    Err(e) => return Completion::Throw(e),
                }
            } else {
                0.0
            };
            let actual = match resolve_element_index(relative_index, len) {
                Some(k) => k,
                None => return Completion::Throw(interp.create_range_error("Invalid index")),
            };
            let value = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
            let mut result = Vec::with_capacity(len);
            for k in 0..len {
                if k == actual {
                    result.push(value.clone());
                } else {
                    result.push(match obj_get(interp, &o, &k.to_string()) {
                        Ok(v) => v,
                        Err(c) => return c,
                    });
                }
            }
            Completion::Normal(interp.create_array(result))
        });

        // Array.isArray
        let is_array_fn = self.create_function(JsFunction::native(
            "isArray".to_string(),
            1,
            |interp, _this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if let Some(obj_id) = val.as_object_id() {
                    match is_array_check(interp, obj_id) {
                        Ok(result) => Completion::Normal(JsValue::boolean(result)),
                        Err(e) => Completion::Throw(e),
                    }
                } else {
                    Completion::Normal(JsValue::FALSE)
                }
            },
        ));

        // Array.from
        let array_from = self.create_function(JsFunction::native(
            "from".to_string(),
            1,
            |interp, this_val, args: &[JsValue]| {
                let c = this_val.clone();
                let source = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let map_fn = args.get(1).cloned();
                let this_arg = args.get(2).cloned().unwrap_or(JsValue::UNDEFINED);
                let mapping = if let Some(ref mf) = map_fn
                    && !(mf).is_undefined()
                {
                    if !interp.is_callable(mf) {
                        return Completion::Throw(
                            interp.create_type_error("Array.from mapFn is not a function"),
                        );
                    }
                    true
                } else {
                    false
                };

                // Check if source is null/undefined
                if (source).is_nullish() {
                    return Completion::Throw(
                        interp.create_type_error("Cannot convert undefined or null to object"),
                    );
                }

                // §23.1.2.1 step 4: Let usingIterator be ? GetMethod(items, @@iterator).
                let using_iterator = if let Some(key) = interp.get_symbol_key("iterator") {
                    match interp.access_property_on_value(&source, &key) {
                        Completion::Normal(v) if !(v).is_nullish() => Some(v),
                        Completion::Normal(_) => None,
                        other => return other,
                    }
                } else {
                    None
                };

                if let Some(iter_method) = using_iterator {
                    // Iterator path - spec says Construct(C) with no arguments
                    let a = if interp.is_constructor(&c) {
                        match interp.construct(&c, &[]) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            other => return other,
                        }
                    } else {
                        interp.create_array(Vec::new())
                    };
                    interp.with_gc_root_scope(|interp| {
                        interp.gc_root_value(&a);

                        let iterator = match interp.get_iterator_from_method(&iter_method, &source)
                        {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };
                        interp.gc_root_value(&iterator);
                        let mut k: usize = 0;
                        loop {
                            let value = match interp.iterator_step_value(&iterator) {
                                Ok(Some(value)) => value,
                                Ok(None) => {
                                    if let Err(e) = set_length_throw(interp, &a, k) {
                                        return Completion::Throw(e);
                                    }
                                    return Completion::Normal(a);
                                }
                                Err(e) => return Completion::Throw(e),
                            };
                            // Nested scope: `value` only needs to stay rooted for this
                            // one iteration's map/property-set without accumulating
                            // roots across iterations.
                            let abrupt =
                                interp.with_gc_root_scope(|interp| -> Option<Completion> {
                                    interp.gc_root_value(&value);
                                    let mapped_value = if mapping {
                                        match interp.call_function(
                                            map_fn.as_ref().unwrap(),
                                            &this_arg,
                                            &[value, JsValue::number(k as f64)],
                                        ) {
                                            Completion::Normal(v) => v,
                                            other => {
                                                // Pass the real payload (when the abrupt
                                                // completion carries one) rather than
                                                // UNDEFINED so `iterator_close` roots it
                                                // across `return()` (issue #794).
                                                let payload = match &other {
                                                    Completion::Throw(v) => v.clone(),
                                                    _ => JsValue::UNDEFINED,
                                                };
                                                let _ = interp.iterator_close(&iterator, payload);
                                                return Some(other);
                                            }
                                        }
                                    } else {
                                        value
                                    };
                                    if let Err(e) = create_data_property_or_throw(
                                        interp,
                                        &a,
                                        &k.to_string(),
                                        mapped_value,
                                    ) {
                                        let e = interp.iterator_close(&iterator, e);
                                        return Some(Completion::Throw(e));
                                    }
                                    None
                                });
                            if let Some(c) = abrupt {
                                return c;
                            }
                            k += 1;
                        }
                    })
                } else {
                    // Array-like path
                    interp.with_gc_root_scope(|interp| {
                        let array_like = match to_object_val(interp, &source) {
                            Ok(v) => v,
                            Err(c) => return c,
                        };
                        interp.gc_root_value(&array_like);
                        let len = match length_of_array_like(interp, &array_like) {
                            Ok(v) => v,
                            Err(c) => return c,
                        };

                        let a = if interp.is_constructor(&c) {
                            match interp.construct(&c, &[JsValue::number(len as f64)]) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                other => return other,
                            }
                        } else {
                            interp.create_array(Vec::new())
                        };
                        interp.gc_root_value(&a);

                        for k in 0..len {
                            let kvalue = match obj_get(interp, &array_like, &k.to_string()) {
                                Ok(v) => v,
                                Err(c) => return c,
                            };
                            let mapped_value = if mapping {
                                match interp.call_function(
                                    map_fn.as_ref().unwrap(),
                                    &this_arg,
                                    &[kvalue, JsValue::number(k as f64)],
                                ) {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                }
                            } else {
                                kvalue
                            };
                            if let Err(e) = create_data_property_or_throw(
                                interp,
                                &a,
                                &k.to_string(),
                                mapped_value,
                            ) {
                                return Completion::Throw(e);
                            }
                        }
                        if let Err(e) = set_length_throw(interp, &a, len) {
                            return Completion::Throw(e);
                        }
                        Completion::Normal(a)
                    })
                }
            },
        ));

        // Array.of
        let array_of = self.create_function(JsFunction::native(
            "of".to_string(),
            0,
            |interp, this_val, args: &[JsValue]| {
                let c = this_val.clone();
                let len = args.len();
                // With `this` = %Array%, Construct(C, [len]) yields a fresh
                // ordinary Array that CreateDataPropertyOrThrow then fills;
                // no step is observable, so build it directly.
                if let Some(array_ctor) = interp.realm().array_constructor.as_ref()
                    && let Some(ctor_id) = array_ctor.as_object_id()
                    && c.as_object_id() == Some(ctor_id)
                {
                    return Completion::Normal(interp.create_array(args.to_vec()));
                }
                let a = if interp.is_constructor(&c) {
                    match interp.construct(&c, &[JsValue::number(len as f64)]) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        other => return other,
                    }
                } else {
                    interp.create_array(Vec::new())
                };
                for (k, arg) in args.iter().enumerate() {
                    if let Err(e) =
                        create_data_property_or_throw(interp, &a, &k.to_string(), arg.clone())
                    {
                        return Completion::Throw(e);
                    }
                }
                if let Err(e) = set_length_throw(interp, &a, len) {
                    return Completion::Throw(e);
                }
                Completion::Normal(a)
            },
        ));

        // Array.prototype.entries
        self.define_method(proto_id, "entries", 0, |interp, this_val, _args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            if let Some(obj_id) = o.as_object_id() {
                return Completion::Normal(
                    interp.create_array_iterator(obj_id, IteratorKind::KeyValue),
                );
            }
            Completion::Throw(interp.create_type_error("entries called on non-object"))
        });

        // Array.prototype.keys
        self.define_method(proto_id, "keys", 0, |interp, this_val, _args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            if let Some(obj_id) = o.as_object_id() {
                return Completion::Normal(interp.create_array_iterator(obj_id, IteratorKind::Key));
            }
            Completion::Throw(interp.create_type_error("keys called on non-object"))
        });

        // Array.prototype.values
        let values_fn = self.define_method(proto_id, "values", 0, |interp, this_val, _args| {
            let o = match to_object_val(interp, this_val) {
                Ok(v) => v,
                Err(c) => return c,
            };
            if let Some(obj_id) = o.as_object_id() {
                return Completion::Normal(
                    interp.create_array_iterator(obj_id, IteratorKind::Value),
                );
            }
            Completion::Throw(interp.create_type_error("values called on non-object"))
        });

        self.realm_mut().array_prototype_values = Some(values_fn.clone());

        // Array.prototype[@@iterator] is the same function as Array.prototype.values (spec §23.1.3.35)
        if let Some(key) = self.get_symbol_iterator_key() {
            self.get_object_cell_expect(proto_id)
                .borrow_mut()
                .insert_property(key, PropertyDescriptor::data(values_fn, true, false, true));
        }

        // Array.fromAsync
        let from_async_fn = self.create_function(JsFunction::native(
            "fromAsync".to_string(),
            1,
            |interp, this, args| {
                let async_items = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let map_fn = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                let this_arg = args.get(2).cloned().unwrap_or(JsValue::UNDEFINED);

                let has_map = !(map_fn).is_undefined();
                if has_map && !interp.is_callable(&map_fn) {
                    let err = interp.create_type_error("Array.fromAsync mapFn is not a function");
                    return interp.create_rejected_promise(err);
                }

                // Check if this is a constructor (use Array if not)
                let using_iterator;
                // Step 4: Let usingAsyncIterator be ? GetMethod(asyncItems, @@asyncIterator).
                let async_sym_key = interp.get_symbol_key("asyncIterator");
                let mut has_async_iter = false;
                if let Some(ref key) = async_sym_key {
                    if !async_items.is_nullish() {
                        let val = match interp.access_property_on_value(&async_items, key) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return interp.create_rejected_promise(e),
                            _ => JsValue::UNDEFINED,
                        };
                        if !(val).is_nullish() {
                            has_async_iter = true;
                            using_iterator = Some(val);
                        } else {
                            using_iterator = None;
                        }
                    } else {
                        using_iterator = None;
                    }
                } else {
                    using_iterator = None;
                }

                // Step 5: If usingAsyncIterator is undefined, let usingSyncIterator = GetMethod(asyncItems, @@iterator).
                let using_sync_iterator = if !has_async_iter {
                    let sym_key = interp.get_symbol_iterator_key();
                    if let Some(ref key) = sym_key {
                        if !async_items.is_nullish() {
                            match interp.access_property_on_value(&async_items, key) {
                                Completion::Normal(v) if !(v).is_nullish() => Some(v),
                                Completion::Throw(e) => return interp.create_rejected_promise(e),
                                _ => None,
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Create a promise for the result
                let promise = interp.create_promise_object();
                let promise_id = promise.as_object_id().unwrap_or(0);
                let (resolve_fn, reject_fn) = interp.create_resolving_functions(promise_id);

                let slots = RootedSlots::new(interp, FA_SLOT_COUNT);
                slots.set(interp, FA_MAP_FN, map_fn.clone());
                slots.set(interp, FA_THIS_ARG, this_arg.clone());
                slots.set(interp, FA_RESOLVE_FN, resolve_fn.clone());
                slots.set(interp, FA_REJECT_FN, reject_fn.clone());

                interp.with_gc_root_scope(|interp| {
                    interp.gc_root_value(&promise);
                    interp.gc_root_value(&slots.root());

                    // Step 7: If usingAsyncIterator is not undefined OR usingSyncIterator is not undefined
                    if has_async_iter || using_sync_iterator.is_some() {
                        // Get iterator
                        let iterator = if has_async_iter {
                            if let Some(ref iter_fn) = using_iterator {
                                match interp.call_function(iter_fn, &async_items, &[]) {
                                    Completion::Normal(v) if (v).is_object() => v,
                                    Completion::Normal(_) => {
                                        let err = interp.create_type_error("Result of the Symbol.asyncIterator method is not an object");
                                        let _ = interp.call_function(&reject_fn, &JsValue::UNDEFINED, &[err]);
                                        return Completion::Normal(promise);
                                    }
                                    Completion::Throw(e) => {
                                        let _ = interp.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                        return Completion::Normal(promise);
                                    }
                                    _ => {
                                        let err = interp.create_type_error("is not async iterable");
                                        let _ = interp.call_function(&reject_fn, &JsValue::UNDEFINED, &[err]);
                                        return Completion::Normal(promise);
                                    }
                                }
                            } else {
                                unreachable!()
                            }
                        } else {
                            // Sync iterator → wrap in async-from-sync
                            match using_sync_iterator {
                                Some(ref iter_fn) => {
                                    match interp.call_function(iter_fn, &async_items, &[]) {
                                        Completion::Normal(v) if (v).is_object() => {
                                            interp.create_async_from_sync_iterator(v)
                                        }
                                        Completion::Normal(_) => {
                                            let err = interp.create_type_error("Result of the Symbol.iterator method is not an object");
                                            let _ = interp.call_function(&reject_fn, &JsValue::UNDEFINED, &[err]);
                                            return Completion::Normal(promise);
                                        }
                                        Completion::Throw(e) => {
                                            let _ = interp.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                            return Completion::Normal(promise);
                                        }
                                        _ => {
                                            let err = interp.create_type_error("is not iterable");
                                            let _ = interp.call_function(&reject_fn, &JsValue::UNDEFINED, &[err]);
                                            return Completion::Normal(promise);
                                        }
                                    }
                                }
                                None => unreachable!(),
                            }
                        };

                        slots.set(interp, FA_ITERATOR, iterator.clone());
                        // Create the result array — spec: Construct(C) with no args for iterable path
                        let is_constructor = interp.is_constructor(this);
                        let arr = if is_constructor {
                            match interp.construct(this, &[]) {
                                Completion::Normal(v) if (v).is_object() => v,
                                Completion::Throw(e) => {
                                    let _ = interp.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                    return Completion::Normal(promise);
                                }
                                _ => interp.create_array(vec![]),
                            }
                        } else {
                            interp.create_array(vec![])
                        };

                        slots.set(interp, FA_ARR, arr);
                        let state = Rc::new(RefCell::new(FromAsyncState {
                            slots: slots.clone(),
                            k: 0,
                            has_map,
                            is_array_like: false,
                            len: 0,
                        }));
                        from_async_iter_step(interp, state);
                        Completion::Normal(promise)
                    } else {
                        // Array-like path
                        let array_like = match interp.to_object(&async_items) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                let _ = interp.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                return Completion::Normal(promise);
                            }
                            _ => {
                                let err = interp.create_type_error("Cannot convert to object");
                                let _ = interp.call_function(&reject_fn, &JsValue::UNDEFINED, &[err]);
                                return Completion::Normal(promise);
                            }
                        };
                        slots.set(interp, FA_ARRAY_LIKE, array_like.clone());
                        let len = match length_of_array_like(interp, &array_like) {
                            Ok(n) => n as u64,
                            Err(Completion::Throw(e)) => {
                                let _ = interp.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                return Completion::Normal(promise);
                            }
                            Err(_) => 0u64,
                        };

                        let is_constructor = interp.is_constructor(this);
                        let arr = if is_constructor {
                            match interp.construct(this, &[JsValue::number(len as f64)]) {
                                Completion::Normal(v) if (v).is_object() => v,
                                Completion::Throw(e) => {
                                    let _ = interp.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                    return Completion::Normal(promise);
                                }
                                _ => interp.create_array(vec![]),
                            }
                        } else {
                            if len > 0xFFFF_FFFF {
                                let e = interp.create_range_error("Invalid array length");
                                let _ = interp.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                                return Completion::Normal(promise);
                            }
                            interp.create_array(vec![])
                        };

                        slots.set(interp, FA_ARR, arr);
                        let state = Rc::new(RefCell::new(FromAsyncState {
                            slots: slots.clone(),
                            k: 0,
                            has_map,
                            is_array_like: true,
                            len,
                        }));
                        from_async_arraylike_step(interp, state);
                        Completion::Normal(promise)
                    }
                })
            },
        ));

        // Set Array statics on the Array constructor
        let array_val = self.get_global_var("Array");
        if let Some(array_val) = array_val
            && let Some(array_id) = array_val.as_object_id()
            && self.get_object_cell(array_id).is_some()
        {
            self.realm_mut().array_constructor = Some(array_val.clone());
            {
                let mut borrow = self.get_object_cell_expect(array_id).borrow_mut();
                borrow.insert_builtin("isArray".to_string(), is_array_fn);
                borrow.insert_builtin("from".to_string(), array_from);
                borrow.insert_builtin("of".to_string(), array_of);
                borrow.insert_builtin("fromAsync".to_string(), from_async_fn);
                let proto_val = JsValue::object(proto_id);
                borrow.insert_property(
                    "prototype".to_string(),
                    PropertyDescriptor::data(proto_val, false, false, false),
                );
            }

            // Array[Symbol.species] getter - returns `this`
            let species_getter = self.create_function(JsFunction::native(
                "get [Symbol.species]".to_string(),
                0,
                |_interp, this_val, _args| Completion::Normal(this_val.clone()),
            ));
            self.get_object_cell_expect(array_id)
                .borrow_mut()
                .insert_property(
                    JsPropertyKey::well_known_symbol("species"),
                    PropertyDescriptor {
                        value: None,
                        writable: None,
                        get: Some(species_getter),
                        set: None,
                        enumerable: Some(false),
                        configurable: Some(true),
                    },
                );

            // Array.prototype.constructor = Array
            self.get_object_cell_expect(proto_id)
                .borrow_mut()
                .insert_builtin("constructor".to_string(), array_val);
        }

        // Array.prototype[@@unscopables] (§23.1.3.38)
        {
            let unscopables_obj_id = self.create_object_id();
            self.get_object_cell_expect(unscopables_obj_id)
                .borrow_mut()
                .prototype_id = None;
            let names = [
                "at",
                "copyWithin",
                "entries",
                "fill",
                "find",
                "findIndex",
                "findLast",
                "findLastIndex",
                "flat",
                "flatMap",
                "includes",
                "keys",
                "toReversed",
                "toSorted",
                "toSpliced",
                "values",
            ];
            for name in names {
                self.get_object_cell_expect(unscopables_obj_id)
                    .borrow_mut()
                    .insert_value(name.to_string(), JsValue::boolean(true));
            }
            let unscopables_id = unscopables_obj_id;
            let unscopables_val = JsValue::object(unscopables_id);
            self.get_object_cell_expect(proto_id)
                .borrow_mut()
                .insert_property(
                    JsPropertyKey::well_known_symbol("unscopables"),
                    PropertyDescriptor::data(unscopables_val, false, false, true),
                );
        }

        self.realm_mut().array_prototype = Some(proto_id);
    }

    pub(crate) fn create_array(&mut self, values: Vec<JsValue>) -> JsValue {
        let mut obj_data = JsObjectData::new();
        obj_data.prototype_id = self
            .realm()
            .array_prototype
            .or(self.realm().object_prototype);
        obj_data.class_name = "Array";
        obj_data.kind = crate::interpreter::types::ObjectKind::Array(
            crate::interpreter::types::ArrayData::default(),
        );
        // The element vector holds every non-undefined value; only an
        // undefined value needs its own descriptor, because the vector uses
        // undefined as its hole marker.
        for (i, v) in values.iter().enumerate() {
            if v.is_undefined() {
                obj_data.insert_value(i.to_string(), v.clone());
            }
        }
        obj_data.push_fresh_array_length(
            &self.common_keys.length,
            PropertyDescriptor::data(JsValue::number(values.len() as f64), true, false, false),
        );
        *obj_data.array_elements_mut().unwrap() = values;
        let id = self.alloc_object(obj_data);
        JsValue::object(id)
    }

    pub(crate) fn create_array_with_holes(
        &mut self,
        items: impl ExactSizeIterator<Item = Option<JsValue>>,
    ) -> JsValue {
        let len = items.len();
        let mut obj_data = JsObjectData::new();
        obj_data.prototype_id = self
            .realm()
            .array_prototype
            .or(self.realm().object_prototype);
        obj_data.class_name = "Array";
        obj_data.kind = crate::interpreter::types::ObjectKind::Array(
            crate::interpreter::types::ArrayData::default(),
        );
        let mut array_elements = Vec::with_capacity(len);
        for (i, item) in items.enumerate() {
            match item {
                Some(v) => {
                    if v.is_undefined() {
                        obj_data.insert_value(i.to_string(), v.clone());
                    }
                    array_elements.push(v);
                }
                None => {
                    array_elements.push(JsValue::UNDEFINED);
                }
            }
        }
        obj_data.push_fresh_array_length(
            &self.common_keys.length,
            PropertyDescriptor::data(JsValue::number(len as f64), true, false, false),
        );
        *obj_data.array_elements_mut().unwrap() = array_elements;
        let id = self.alloc_object(obj_data);
        JsValue::object(id)
    }

    pub(crate) fn create_array_with_length(&mut self, len: usize) -> JsValue {
        let mut obj_data = JsObjectData::new();
        obj_data.prototype_id = self
            .realm()
            .array_prototype
            .or(self.realm().object_prototype);
        obj_data.class_name = "Array";
        obj_data.kind = crate::interpreter::types::ObjectKind::Array(
            crate::interpreter::types::ArrayData::default(),
        );
        obj_data.push_fresh_array_length(
            &self.common_keys.length,
            PropertyDescriptor::data(JsValue::number(len as f64), true, false, false),
        );
        // Use a small Vec for sparse arrays — don't pre-allocate huge arrays
        let id = self.alloc_object(obj_data);
        JsValue::object(id)
    }

    pub(super) fn setup_array_constructor(&mut self) {
        // Array constructor (must be before setup_array_prototype so statics can be added)
        self.register_global_fn(
            "Array",
            BindingKind::Var,
            JsFunction::constructor("Array".to_string(), 1, |interp, _this, args| {
                let arr = if args.len() == 1
                    && let Some(n) = args[0].as_number()
                {
                    let len = n;
                    let uint32_len = len as u32;
                    if (uint32_len as f64) != len {
                        let err = interp.create_range_error("Invalid array length");
                        return Completion::Throw(err);
                    }
                    interp.create_array_with_length(uint32_len as usize)
                } else {
                    interp.create_array(args.to_vec())
                };
                if let Some(array_id) = arr.as_object_id() {
                    let default_proto_id = interp.realm().array_prototype;
                    interp.apply_new_target_prototype(array_id, default_proto_id, |realm| {
                        realm.array_prototype
                    });
                }
                Completion::Normal(arr)
            }),
        );
    }
}
