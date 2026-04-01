use super::super::*;

// §7.2.2 IsArray — Proxy-aware check
fn is_array_check(interp: &mut Interpreter, obj_id: u64) -> Result<bool, JsValue> {
    if let Some(obj) = interp.get_object(obj_id) {
        let (is_revoked, is_proxy, target_id, class) = {
            let b = obj.borrow();
            let tid = b.proxy_target.as_ref().and_then(|t| t.borrow().id);
            // is_proxy() checks proxy_target.is_some(), but revoked proxies have proxy_target=None
            // Use proxy_revoked flag to also detect revoked proxies
            (
                b.proxy_revoked,
                b.is_proxy() || b.proxy_revoked,
                tid,
                b.class_name.clone(),
            )
        };
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
    let len_val = if let JsValue::Object(obj_ref) = o {
        match interp.get_object_property(obj_ref.id, "length", o) {
            Completion::Normal(v) => v,
            other => return Err(other),
        }
    } else {
        JsValue::Undefined
    };
    // ? ToLength(lenProperty) — must propagate errors from ToNumber/ToPrimitive
    let n = match interp.to_number_value(&len_val) {
        Ok(v) => v,
        Err(e) => return Err(Completion::Throw(e)),
    };
    let len = to_integer_or_infinity(n);
    if len < 0.0 {
        return Ok(0);
    }
    Ok(len.min(9007199254740991.0) as usize)
}

fn get_obj(interp: &Interpreter, o: &JsValue) -> Option<Rc<RefCell<JsObjectData>>> {
    if let JsValue::Object(obj_ref) = o {
        interp.get_object(obj_ref.id)
    } else {
        None
    }
}

fn obj_get(interp: &mut Interpreter, o: &JsValue, key: &str) -> Result<JsValue, Completion> {
    if let JsValue::Object(obj_ref) = o {
        match interp.get_object_property(obj_ref.id, key, o) {
            Completion::Normal(v) => Ok(v),
            other => Err(other),
        }
    } else {
        Ok(JsValue::Undefined)
    }
}

// Set(O, P, V, true) — invoke setters on prototype, check writable, throw on failure
fn obj_set_throw(
    interp: &mut Interpreter,
    o: &JsValue,
    key: &str,
    value: JsValue,
) -> Result<(), JsValue> {
    if let JsValue::Object(obj_ref) = o {
        // Check for Proxy
        if let Some(obj) = interp.get_object(obj_ref.id)
            && (obj.borrow().is_proxy() || obj.borrow().proxy_revoked)
        {
            match interp.proxy_set(obj_ref.id, key, value, o) {
                Ok(true) => return Ok(()),
                Ok(false) => {
                    return Err(interp.create_type_error(&format!(
                        "Cannot assign to read only property '{key}'"
                    )));
                }
                Err(e) => return Err(e),
            }
        }
        // Check for setter in prototype chain
        if let Some(obj) = interp.get_object(obj_ref.id) {
            let desc = obj.borrow().get_property_descriptor(key);
            if let Some(ref d) = desc {
                if let Some(ref setter) = d.set
                    && !matches!(setter, JsValue::Undefined)
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
            let ta_info = interp.get_object(obj_ref.id).and_then(|obj| {
                let b = obj.borrow();
                b.typed_array_info
                    .as_ref()
                    .map(|ta| (ta.kind.is_bigint(), ta.kind))
            });
            if let Some((is_bigint, _kind)) = ta_info
                && let Some(index) = canonical_numeric_index_string(key)
            {
                let num_val = if is_bigint {
                    interp.to_bigint_value(&value)?
                } else {
                    match interp.to_number_value(&value) {
                        Ok(n) => JsValue::Number(n),
                        Err(e) => return Err(e),
                    }
                };
                if let Some(obj) = interp.get_object(obj_ref.id)
                    && let Some(ref ta) = obj.borrow().typed_array_info
                    && is_valid_integer_index(ta, index)
                {
                    typed_array_set_index(ta, index as usize, &num_val);
                }
                return Ok(());
            }
        }
        // Normal set
        if let Some(obj) = interp.get_object(obj_ref.id) {
            let mut borrow = obj.borrow_mut();
            if !borrow.extensible && !borrow.has_own_property(key) {
                return Err(interp.create_type_error(&format!(
                    "Cannot add property {key}, object is not extensible"
                )));
            }
            borrow.set_property_value(key, value);
        }
    }
    Ok(())
}

pub(crate) fn create_data_property_or_throw(
    interp: &mut Interpreter,
    o: &JsValue,
    key: &str,
    value: JsValue,
) -> Result<(), JsValue> {
    if let JsValue::Object(obj_ref) = o {
        // Deferred namespace: trigger evaluation on [[DefineOwnProperty]]
        {
            let is_deferred_ns = interp.get_object(obj_ref.id).is_some_and(|obj| {
                obj.borrow()
                    .module_namespace
                    .as_ref()
                    .is_some_and(|ns| ns.deferred)
            });
            if is_deferred_ns && !Interpreter::is_symbol_like_namespace_key(key, true) {
                interp.ensure_deferred_namespace_evaluation(obj_ref.id)?;
            }
        }
        // Check for Proxy defineProperty trap
        if let Some(obj) = interp.get_object(obj_ref.id)
            && obj.borrow().is_proxy()
        {
            // Build descriptor object for proxy trap
            let desc_obj = interp.create_object();
            {
                let mut borrow = desc_obj.borrow_mut();
                borrow.set_property_value("value", value);
                borrow.set_property_value("writable", JsValue::Boolean(true));
                borrow.set_property_value("enumerable", JsValue::Boolean(true));
                borrow.set_property_value("configurable", JsValue::Boolean(true));
            }
            let desc_id = desc_obj.borrow().id.unwrap();
            let desc_val = JsValue::Object(crate::types::JsObject { id: desc_id });
            return match interp.proxy_define_own_property(obj_ref.id, key.to_string(), &desc_val) {
                Ok(true) => Ok(()),
                Ok(false) => {
                    Err(interp.create_type_error(&format!("Cannot define property: {key}")))
                }
                Err(e) => Err(e),
            };
        }
        if let Some(obj) = interp.get_object(obj_ref.id) {
            // Check extensibility — if object is not extensible and property doesn't exist, fail
            {
                let borrow = obj.borrow();
                if !borrow.extensible && !borrow.has_property(key) {
                    return Err(interp.create_type_error(&format!(
                        "Cannot add property {key}, object is not extensible"
                    )));
                }
                // Check for non-configurable existing property
                if let Some(desc) = borrow.get_own_property(key)
                    && desc.configurable == Some(false)
                {
                    return Err(
                        interp.create_type_error(&format!("Cannot redefine property: {key}"))
                    );
                }
            }
            let mut borrow = obj.borrow_mut();
            if let Some(ref mut elems) = borrow.array_elements
                && let Ok(idx) = key.parse::<usize>()
            {
                if idx < elems.len() {
                    elems[idx] = value.clone();
                } else {
                    while elems.len() < idx {
                        elems.push(JsValue::Undefined);
                    }
                    elems.push(value.clone());
                }
                // Update length if index >= current length (exotic array behavior)
                let cur_len = borrow
                    .properties
                    .get("length")
                    .and_then(|d| d.value.as_ref())
                    .and_then(|v| {
                        if let JsValue::Number(n) = v {
                            Some(*n as usize)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);
                if idx >= cur_len {
                    borrow.set_property_value("length", JsValue::Number((idx + 1) as f64));
                }
            }
            borrow.define_own_property(key.to_string(), PropertyDescriptor::data_default(value));
        }
    }
    Ok(())
}

fn obj_has_throw(interp: &mut Interpreter, o: &JsValue, key: &str) -> Result<bool, JsValue> {
    if let JsValue::Object(obj_ref) = o {
        interp.proxy_has_property(obj_ref.id, key)
    } else {
        Ok(false)
    }
}

// AsyncIteratorClose: call iterator.return() if it exists (best-effort)
fn close_async_iterator(interp: &mut Interpreter, iterator: &JsValue) {
    if let JsValue::Object(ref io) = *iterator
        && let Completion::Normal(return_fn) = interp.get_object_property(io.id, "return", iterator)
        && interp.is_callable(&return_fn)
    {
        let _ = interp.call_function(&return_fn, iterator, &[]);
    }
}

// --- Array.fromAsync continuation-passing machinery ---
// Shared state threaded through non-blocking continuations.
struct FromAsyncState {
    iterator: JsValue,
    arr: JsValue,
    k: u64,
    has_map: bool,
    map_fn: JsValue,
    this_arg: JsValue,
    resolve_fn: JsValue,
    reject_fn: JsValue,
    is_array_like: bool,
    array_like: JsValue,
    len: u64,
    gc_roots: Vec<u64>,
}

fn from_async_gc_root(interp: &mut Interpreter, state: &Rc<RefCell<FromAsyncState>>) {
    let mut roots = Vec::new();
    let s = state.borrow();
    for val in [
        &s.iterator,
        &s.arr,
        &s.map_fn,
        &s.this_arg,
        &s.resolve_fn,
        &s.reject_fn,
        &s.array_like,
    ] {
        if let JsValue::Object(o) = val {
            interp.gc_temp_roots.push(o.id);
            roots.push(o.id);
        }
    }
    drop(s);
    state.borrow_mut().gc_roots = roots;
}

fn from_async_gc_unroot(interp: &mut Interpreter, state: &Rc<RefCell<FromAsyncState>>) {
    let s = state.borrow();
    for &id in &s.gc_roots {
        if let Some(pos) = interp.gc_temp_roots.iter().rposition(|&rid| rid == id) {
            interp.gc_temp_roots.remove(pos);
        }
    }
}

fn from_async_collect_roots(state: &Rc<RefCell<FromAsyncState>>) -> Vec<JsValue> {
    let s = state.borrow();
    let mut roots = Vec::new();
    for val in [
        &s.iterator,
        &s.arr,
        &s.map_fn,
        &s.this_arg,
        &s.resolve_fn,
        &s.reject_fn,
        &s.array_like,
    ] {
        if matches!(val, JsValue::Object(_)) {
            roots.push(val.clone());
        }
    }
    roots
}

fn from_async_reject(
    interp: &mut Interpreter,
    state: &Rc<RefCell<FromAsyncState>>,
    error: JsValue,
    close_iter: bool,
) {
    let s = state.borrow();
    let iterator = s.iterator.clone();
    let reject_fn = s.reject_fn.clone();
    let is_array_like = s.is_array_like;
    drop(s);
    if close_iter && !is_array_like {
        close_async_iterator(interp, &iterator);
    }
    from_async_gc_unroot(interp, state);
    let _ = interp.call_function(&reject_fn, &JsValue::Undefined, &[error]);
}

fn from_async_resolve(interp: &mut Interpreter, state: &Rc<RefCell<FromAsyncState>>) {
    let s = state.borrow();
    let arr = s.arr.clone();
    let k = s.k;
    let resolve_fn = s.resolve_fn.clone();
    drop(s);
    if let Err(e) = set_length_throw(interp, &arr, k as usize) {
        from_async_gc_unroot(interp, state);
        let _ = interp.call_function(&state.borrow().reject_fn.clone(), &JsValue::Undefined, &[e]);
        return;
    }
    from_async_gc_unroot(interp, state);
    let _ = interp.call_function(&resolve_fn, &JsValue::Undefined, &[arr]);
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
    let promise_id = if let JsValue::Object(ref o) = promise {
        o.id
    } else {
        0
    };

    let pstate = interp.get_promise_state(promise_id);
    match pstate {
        Some(PromiseState::Fulfilled(v)) => {
            let value = v.clone();
            let state_c = state.clone();
            let roots = from_async_collect_roots(&state);
            interp.microtask_queue.push((
                roots,
                Box::new(move |interp| {
                    on_fulfill(interp, value, state_c);
                    Completion::Normal(JsValue::Undefined)
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
                    let v = args.first().cloned().unwrap_or(JsValue::Undefined);
                    on_fulfill(interp, v, state_f.clone());
                    Completion::Normal(JsValue::Undefined)
                },
            ));

            let reject_handler = interp.create_function(JsFunction::native(
                "fromAsyncReject".to_string(),
                1,
                move |interp, _this, args| {
                    let e = args.first().cloned().unwrap_or(JsValue::Undefined);
                    from_async_reject(interp, &state_r, e, close);
                    Completion::Normal(JsValue::Undefined)
                },
            ));

            if let Some(obj) = interp.get_object(promise_id) {
                let mut ob = obj.borrow_mut();
                if let Some(ref mut pd) = ob.promise_data {
                    pd.is_handled = true;
                    pd.fulfill_reactions.push(PromiseReaction {
                        handler: Some(fulfill_handler),
                        promise_id: None,
                        resolve: JsValue::Undefined,
                        reject: JsValue::Undefined,
                        reaction_type: PromiseReactionType::Fulfill,
                    });
                    pd.reject_reactions.push(PromiseReaction {
                        handler: Some(reject_handler),
                        promise_id: None,
                        resolve: JsValue::Undefined,
                        reject: JsValue::Undefined,
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
        let s = state.borrow();
        if s.k >= 0x1FFFFFFFFFFFFF {
            let iterator = s.iterator.clone();
            let reject_fn = s.reject_fn.clone();
            drop(s);
            let err = interp.create_type_error("Array.fromAsync: too many elements");
            close_async_iterator(interp, &iterator);
            from_async_gc_unroot(interp, &state);
            let _ = interp.call_function(&reject_fn, &JsValue::Undefined, &[err]);
            return;
        }
    }
    let iterator = state.borrow().iterator.clone();

    let next_fn = if let JsValue::Object(ref io) = iterator {
        match interp.get_object_property(io.id, "next", &iterator) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => {
                from_async_reject(interp, &state, e, false);
                return;
            }
            _ => JsValue::Undefined,
        }
    } else {
        JsValue::Undefined
    };

    let next_result = match interp.call_function(&next_fn, &iterator, &[]) {
        Completion::Normal(v) => v,
        Completion::Throw(e) => {
            from_async_reject(interp, &state, e, false);
            return;
        }
        _ => JsValue::Undefined,
    };

    from_async_attach_await(interp, &next_result, state, from_async_process_next, true);
}

// Process the awaited iterator result: check done, extract value, apply mapFn.
fn from_async_process_next(
    interp: &mut Interpreter,
    next_result: JsValue,
    state: Rc<RefCell<FromAsyncState>>,
) {
    let done = if let JsValue::Object(ref o) = next_result {
        match interp.get_object_property(o.id, "done", &next_result) {
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

    let value = if let JsValue::Object(ref o) = next_result {
        match interp.get_object_property(o.id, "value", &next_result) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => {
                from_async_reject(interp, &state, e, true);
                return;
            }
            _ => JsValue::Undefined,
        }
    } else {
        JsValue::Undefined
    };

    let has_map = state.borrow().has_map;
    if has_map {
        let s = state.borrow();
        let map_fn = s.map_fn.clone();
        let this_arg = s.this_arg.clone();
        let k = s.k;
        drop(s);

        let mapped =
            match interp.call_function(&map_fn, &this_arg, &[value, JsValue::Number(k as f64)]) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    from_async_reject(interp, &state, e, true);
                    return;
                }
                _ => JsValue::Undefined,
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
    let arr = state.borrow().arr.clone();
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
    let s = state.borrow();
    let k = s.k;
    let len = s.len;
    if k >= len {
        drop(s);
        from_async_resolve(interp, &state);
        return;
    }
    let array_like = s.array_like.clone();
    drop(s);

    let key_str = k.to_string();
    let value = if let JsValue::Object(ref o) = array_like {
        match interp.get_object_property(o.id, &key_str, &array_like) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => {
                from_async_reject(interp, &state, e, false);
                return;
            }
            _ => JsValue::Undefined,
        }
    } else {
        JsValue::Undefined
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
        let s = state.borrow();
        let map_fn = s.map_fn.clone();
        let this_arg = s.this_arg.clone();
        let k = s.k;
        drop(s);

        let mapped =
            match interp.call_function(&map_fn, &this_arg, &[value, JsValue::Number(k as f64)]) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    from_async_reject(interp, &state, e, false);
                    return;
                }
                _ => JsValue::Undefined,
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
    let arr = state.borrow().arr.clone();
    let key_str = k.to_string();
    if let Err(e) = create_data_property_or_throw(interp, &arr, &key_str, value) {
        from_async_reject(interp, &state, e, false);
        return;
    }
    state.borrow_mut().k += 1;
    from_async_arraylike_step(interp, state);
}

fn obj_delete(interp: &mut Interpreter, o: &JsValue, key: &str) {
    if let Some(obj) = get_obj(interp, o) {
        let mut borrow = obj.borrow_mut();
        borrow.properties.remove(key);
        borrow.property_order.retain(|k| k != key);
        if let Some(ref mut elems) = borrow.array_elements
            && let Ok(idx) = key.parse::<usize>()
            && idx < elems.len()
        {
            elems[idx] = JsValue::Undefined;
        }
    }
}

// DeletePropertyOrThrow(O, P) - throws TypeError if delete fails
fn obj_delete_throw(interp: &mut Interpreter, o: &JsValue, key: &str) -> Result<(), JsValue> {
    if let JsValue::Object(obj_ref) = o {
        // Check for Proxy deleteProperty trap
        if let Some(obj) = interp.get_object(obj_ref.id)
            && (obj.borrow().is_proxy() || obj.borrow().proxy_revoked)
        {
            match interp.proxy_delete_property(obj_ref.id, key) {
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
        if let Some(obj) = interp.get_object(obj_ref.id) {
            let desc = obj.borrow().get_own_property(key);
            if let Some(ref d) = desc
                && d.configurable == Some(false)
            {
                return Err(
                    interp.create_type_error(&format!("Cannot delete property '{key}' of object"))
                );
            }
        }
    }
    obj_delete(interp, o, key);
    Ok(())
}

fn set_length(interp: &mut Interpreter, o: &JsValue, len: usize) {
    if let Some(obj) = get_obj(interp, o) {
        let mut borrow = obj.borrow_mut();
        if let Some(ref mut elems) = borrow.array_elements
            && len <= elems.len()
        {
            elems.truncate(len);
        }
        // Don't pre-allocate for sparse arrays
        borrow.set_property_value("length", JsValue::Number(len as f64));
    }
}

// Set(O, "length", len, true) — uses obj_set_throw for setter invocation
fn set_length_throw(interp: &mut Interpreter, o: &JsValue, len: usize) -> Result<(), JsValue> {
    if let Some(obj) = get_obj(interp, o)
        && obj.borrow().array_elements.is_some()
    {
        // ArraySetLength §10.4.2.4: ToUint32(len) must equal ToNumber(len)
        let len_f64 = len as f64;
        let len_u32 = len_f64 as u32;
        if (len_u32 as f64) != len_f64 {
            return Err(interp.create_range_error("Invalid array length"));
        }
        // Check if length is writable
        let desc = obj.borrow().get_own_property("length");
        if let Some(ref d) = desc
            && d.writable == Some(false)
        {
            return Err(interp.create_type_error("Cannot assign to read only property 'length'"));
        }
        // Check if frozen (not extensible + all props non-configurable)
        if !obj.borrow().extensible {
            let desc = obj.borrow().get_own_property("length");
            if let Some(ref d) = desc
                && d.configurable == Some(false)
                && d.writable == Some(false)
            {
                return Err(
                    interp.create_type_error("Cannot assign to read only property 'length'")
                );
            }
        }
        let mut borrow = obj.borrow_mut();
        if let Some(ref mut elems) = borrow.array_elements
            && len <= elems.len()
        {
            elems.truncate(len);
        }
        // Don't pre-allocate for sparse arrays
        borrow.set_property_value("length", JsValue::Number(len as f64));
        return Ok(());
    }
    obj_set_throw(interp, o, "length", JsValue::Number(len as f64))
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
    let is_array = if let JsValue::Object(o) = original_array {
        match is_array_check(interp, o.id) {
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
        let arr = interp.create_array(Vec::new());
        set_length(interp, &arr, length);
        return Ok(arr);
    }
    // Get constructor
    let mut c = if let JsValue::Object(o) = original_array {
        match interp.get_object_property(o.id, "constructor", original_array) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(Completion::Throw(e)),
            other => return Err(other),
        }
    } else {
        JsValue::Undefined
    };
    // §9.4.2.3 step 5-6: If C is a constructor from a different realm and
    // equals that realm's intrinsic %Array%, set C to undefined
    if interp.is_constructor(&c)
        && let JsValue::Object(co) = &c
    {
        let c_realm = match interp.get_function_realm(&JsValue::Object(co.clone())) {
            Ok(r) => r,
            Err(e) => return Err(Completion::Throw(e)),
        };
        if c_realm != interp.current_realm_id {
            let realm_array = interp.realms[c_realm].global_env.borrow().get("Array");
            if let Some(ref ra) = realm_array
                && same_value(&c, ra)
            {
                c = JsValue::Undefined;
            }
        }
    }
    // If C is Object, get C[@@species]
    let c = if let JsValue::Object(co) = &c {
        let sym_key = interp.get_symbol_key("species");
        let species = if let Some(key) = &sym_key {
            match interp.get_object_property(co.id, key, &c) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(Completion::Throw(e)),
                other => return Err(other),
            }
        } else {
            JsValue::Undefined
        };
        // If species is null, treat as undefined
        if matches!(species, JsValue::Null) {
            JsValue::Undefined
        } else {
            species
        }
    } else {
        c
    };
    // If C is undefined, create a default array
    if matches!(c, JsValue::Undefined) {
        if length as u64 > 0xFFFF_FFFF {
            return Err(Completion::Throw(
                interp.create_range_error("Invalid array length"),
            ));
        }
        let arr = interp.create_array(Vec::new());
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
    match interp.construct(&c, &[JsValue::Number(length as f64)]) {
        Completion::Normal(v) => Ok(v),
        Completion::Throw(e) => Err(Completion::Throw(e)),
        other => Err(other),
    }
}

impl Interpreter {
    pub(crate) fn setup_array_prototype(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Array".to_string();
        proto.borrow_mut().array_elements = Some(Vec::new());
        proto.borrow_mut().insert_property(
            "length".to_string(),
            PropertyDescriptor::data(JsValue::Number(0.0), true, false, false),
        );

        // Array.prototype.push
        let push_fn = self.create_function(JsFunction::native(
            "push".to_string(),
            1,
            |interp, this_val, args| {
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
                for arg in args {
                    if let Err(e) = obj_set_throw(interp, &o, &len.to_string(), arg.clone()) {
                        return Completion::Throw(e);
                    }
                    len += 1;
                }
                if let Err(e) = set_length_throw(interp, &o, len) {
                    return Completion::Throw(e);
                }
                Completion::Normal(JsValue::Number(len as f64))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("push".to_string(), push_fn);

        // Array.prototype.pop
        let pop_fn = self.create_function(JsFunction::native(
            "pop".to_string(),
            0,
            |interp, this_val, _args: &[JsValue]| {
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
                    return Completion::Normal(JsValue::Undefined);
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
            },
        ));
        proto.borrow_mut().insert_builtin("pop".to_string(), pop_fn);

        // Array.prototype.shift
        let shift_fn = self.create_function(JsFunction::native(
            "shift".to_string(),
            0,
            |interp, this_val, _args| {
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
                    return Completion::Normal(JsValue::Undefined);
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
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("shift".to_string(), shift_fn);

        // Array.prototype.unshift
        let unshift_fn = self.create_function(JsFunction::native(
            "unshift".to_string(),
            1,
            |interp, this_val, args| {
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
                Completion::Normal(JsValue::Number(new_len as f64))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("unshift".to_string(), unshift_fn);

        // Array.prototype.indexOf
        let indexof_fn = self.create_function(JsFunction::native(
            "indexOf".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if len == 0 {
                    return Completion::Normal(JsValue::Number(-1.0));
                }
                let search = args.first().cloned().unwrap_or(JsValue::Undefined);
                let n = if args.len() >= 2 {
                    match interp.to_number_value(&args[1]) {
                        Ok(v) => to_integer_or_infinity(v),
                        Err(e) => return Completion::Throw(e),
                    }
                } else {
                    0.0
                };
                if n >= len as f64 {
                    return Completion::Normal(JsValue::Number(-1.0));
                }
                let k = if n >= 0.0 {
                    n as usize
                } else {
                    let calc = len as f64 + n;
                    if calc < 0.0 { 0 } else { calc as usize }
                };
                for i in k..len {
                    let pk = i.to_string();
                    match obj_has_throw(interp, &o, &pk) {
                        Ok(true) => {
                            let elem = match obj_get(interp, &o, &pk) {
                                Ok(v) => v,
                                Err(c) => return c,
                            };
                            if strict_equality(&elem, &search) {
                                return Completion::Normal(JsValue::Number(i as f64));
                            }
                        }
                        Err(e) => return Completion::Throw(e),
                        _ => {}
                    }
                }
                Completion::Normal(JsValue::Number(-1.0))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("indexOf".to_string(), indexof_fn);

        // Array.prototype.lastIndexOf
        let lastindexof_fn = self.create_function(JsFunction::native(
            "lastIndexOf".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if len == 0 {
                    return Completion::Normal(JsValue::Number(-1.0));
                }
                let search = args.first().cloned().unwrap_or(JsValue::Undefined);
                let n = if args.len() >= 2 {
                    match interp.to_number_value(&args[1]) {
                        Ok(v) => to_integer_or_infinity(v),
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
                        return Completion::Normal(JsValue::Number(-1.0));
                    }
                    calc as usize
                };
                for i in (0..=k).rev() {
                    let pk = i.to_string();
                    match obj_has_throw(interp, &o, &pk) {
                        Ok(true) => {
                            let elem = match obj_get(interp, &o, &pk) {
                                Ok(v) => v,
                                Err(c) => return c,
                            };
                            if strict_equality(&elem, &search) {
                                return Completion::Normal(JsValue::Number(i as f64));
                            }
                        }
                        Err(e) => return Completion::Throw(e),
                        _ => {}
                    }
                }
                Completion::Normal(JsValue::Number(-1.0))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("lastIndexOf".to_string(), lastindexof_fn);

        // Array.prototype.includes
        let includes_fn = self.create_function(JsFunction::native(
            "includes".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if len == 0 {
                    return Completion::Normal(JsValue::Boolean(false));
                }
                let search = args.first().cloned().unwrap_or(JsValue::Undefined);
                let n = if args.len() >= 2 {
                    match interp.to_number_value(&args[1]) {
                        Ok(v) => to_integer_or_infinity(v),
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
                for i in k..len {
                    let elem = match obj_get(interp, &o, &i.to_string()) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    if same_value_zero(&elem, &search) {
                        return Completion::Normal(JsValue::Boolean(true));
                    }
                }
                Completion::Normal(JsValue::Boolean(false))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("includes".to_string(), includes_fn);

        // Array.prototype.join
        let join_fn = self.create_function(JsFunction::native(
            "join".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let sep = if let Some(s) = args.first() {
                    if matches!(s, JsValue::Undefined) {
                        ",".to_string()
                    } else {
                        match interp.to_string_value(s) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                } else {
                    ",".to_string()
                };
                let mut parts = Vec::with_capacity(len);
                for i in 0..len {
                    let elem = match obj_get(interp, &o, &i.to_string()) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    if elem.is_undefined() || elem.is_null() {
                        parts.push(String::new());
                    } else {
                        match interp.to_string_value(&elem) {
                            Ok(s) => parts.push(s),
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                }
                Completion::Normal(JsValue::String(JsString::from_str(&parts.join(&sep))))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("join".to_string(), join_fn);

        // Array.prototype.toString
        let tostring_fn = self.create_function(JsFunction::native(
            "toString".to_string(),
            0,
            |interp, this_val, _args| {
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
                Completion::Normal(JsValue::String(JsString::from_str("[object Array]")))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toString".to_string(), tostring_fn);

        // Array.prototype.toLocaleString
        let to_locale_string_fn = self.create_function(JsFunction::native(
            "toLocaleString".to_string(),
            0,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let separator = ",";
                let locales = args.first().cloned().unwrap_or(JsValue::Undefined);
                let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let pass_args = vec![locales, options];
                let mut parts: Vec<String> = Vec::with_capacity(len);
                for k in 0..len {
                    let next_element = match obj_get(interp, &o, &k.to_string()) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    if matches!(next_element, JsValue::Undefined | JsValue::Null) {
                        parts.push(String::new());
                    } else {
                        let element_obj = match interp.to_object(&next_element) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let JsValue::Object(ref obj_ref) = element_obj else {
                            unreachable!()
                        };
                        let to_locale_str_method = match interp.get_object_property(
                            obj_ref.id,
                            "toLocaleString",
                            &next_element,
                        ) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        if interp.is_callable(&to_locale_str_method) {
                            match interp.call_function(
                                &to_locale_str_method,
                                &next_element,
                                &pass_args,
                            ) {
                                Completion::Normal(v) => match interp.to_string_value(&v) {
                                    Ok(s) => parts.push(s),
                                    Err(e) => return Completion::Throw(e),
                                },
                                other => return other,
                            }
                        } else {
                            let err = interp.create_type_error("toLocaleString is not a function");
                            return Completion::Throw(err);
                        }
                    }
                }
                Completion::Normal(JsValue::String(JsString::from_str(&parts.join(separator))))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toLocaleString".to_string(), to_locale_string_fn);

        // Array.prototype.concat
        let concat_fn = self.create_function(JsFunction::native(
            "concat".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let a = match array_species_create(interp, &o, 0) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let gc_frame = interp.gc_root_frame();
                interp.gc_root_value(&a);
                let mut n: usize = 0;
                let items: Vec<JsValue> = std::iter::once(o).chain(args.iter().cloned()).collect();
                for item in &items {
                    // IsConcatSpreadable (§23.1.3.1.1)
                    let spreadable = if let JsValue::Object(obj_ref) = item {
                        let sym_key = interp.get_symbol_key("isConcatSpreadable");
                        let spreadable_val = if let Some(key) = &sym_key {
                            match interp.get_object_property(obj_ref.id, key, item) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                other => return other,
                            }
                        } else {
                            JsValue::Undefined
                        };
                        if !matches!(spreadable_val, JsValue::Undefined) {
                            interp.to_boolean_val(&spreadable_val)
                        } else {
                            // 4. Return ? IsArray(O)
                            match is_array_check(interp, obj_ref.id) {
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
                            Err(c) => {
                                interp.gc_unroot_frame(gc_frame);
                                return c;
                            }
                        };
                        if (n as u64) + (len as u64) > 9007199254740991 {
                            interp.gc_unroot_frame(gc_frame);
                            return Completion::Throw(
                                interp
                                    .create_type_error("Array length exceeds the allowed maximum"),
                            );
                        }
                        for k in 0..len {
                            let pk = k.to_string();
                            let exists = match obj_has_throw(interp, item, &pk) {
                                Ok(b) => b,
                                Err(e) => {
                                    interp.gc_unroot_frame(gc_frame);
                                    return Completion::Throw(e);
                                }
                            };
                            if exists {
                                let val = if let JsValue::Object(obj_ref) = item {
                                    match interp.get_object_property(obj_ref.id, &pk, item) {
                                        Completion::Normal(v) => v,
                                        Completion::Throw(e) => {
                                            interp.gc_unroot_frame(gc_frame);
                                            return Completion::Throw(e);
                                        }
                                        other => {
                                            interp.gc_unroot_frame(gc_frame);
                                            return other;
                                        }
                                    }
                                } else {
                                    match obj_get(interp, item, &pk) {
                                        Ok(v) => v,
                                        Err(c) => {
                                            interp.gc_unroot_frame(gc_frame);
                                            return c;
                                        }
                                    }
                                };
                                if let Err(e) =
                                    create_data_property_or_throw(interp, &a, &n.to_string(), val)
                                {
                                    interp.gc_unroot_frame(gc_frame);
                                    return Completion::Throw(e);
                                }
                            }
                            n += 1;
                        }
                    } else {
                        if n as u64 >= 9007199254740991 {
                            interp.gc_unroot_frame(gc_frame);
                            return Completion::Throw(
                                interp
                                    .create_type_error("Array length exceeds the allowed maximum"),
                            );
                        }
                        if let Err(e) =
                            create_data_property_or_throw(interp, &a, &n.to_string(), item.clone())
                        {
                            interp.gc_unroot_frame(gc_frame);
                            return Completion::Throw(e);
                        }
                        n += 1;
                    }
                }
                if let Err(e) = obj_set_throw(interp, &a, "length", JsValue::Number(n as f64)) {
                    interp.gc_unroot_frame(gc_frame);
                    return Completion::Throw(e);
                }
                interp.gc_unroot_frame(gc_frame);
                Completion::Normal(a)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("concat".to_string(), concat_fn);

        // Array.prototype.slice
        let slice_fn = self.create_function(JsFunction::native(
            "slice".to_string(),
            2,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                } as i64;
                let relative_start = if let Some(v) = args.first() {
                    match interp.to_number_value(v) {
                        Ok(n) => to_integer_or_infinity(n),
                        Err(e) => return Completion::Throw(e),
                    }
                } else {
                    0.0
                };
                let k = if relative_start < 0.0 {
                    (len as f64 + relative_start).max(0.0) as usize
                } else {
                    (relative_start as i64).min(len) as usize
                };
                let relative_end = if let Some(v) = args.get(1) {
                    if matches!(v, JsValue::Undefined) {
                        len as f64
                    } else {
                        match interp.to_number_value(v) {
                            Ok(n) => to_integer_or_infinity(n),
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                } else {
                    len as f64
                };
                let fin = if relative_end < 0.0 {
                    (len as f64 + relative_end).max(0.0) as usize
                } else {
                    (relative_end as i64).min(len) as usize
                };
                let count = fin.saturating_sub(k);
                let a = match array_species_create(interp, &o, count) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let gc_frame = interp.gc_root_frame();
                interp.gc_root_value(&a);
                let mut n: usize = 0;
                for i in k..fin {
                    let pk = i.to_string();
                    match obj_has_throw(interp, &o, &pk) {
                        Ok(true) => {
                            let val = match obj_get(interp, &o, &pk) {
                                Ok(v) => v,
                                Err(c) => {
                                    interp.gc_unroot_frame(gc_frame);
                                    return c;
                                }
                            };
                            if let Err(e) =
                                create_data_property_or_throw(interp, &a, &n.to_string(), val)
                            {
                                interp.gc_unroot_frame(gc_frame);
                                return Completion::Throw(e);
                            }
                        }
                        Err(e) => {
                            interp.gc_unroot_frame(gc_frame);
                            return Completion::Throw(e);
                        }
                        _ => {}
                    }
                    n += 1;
                }
                if let Err(e) = obj_set_throw(interp, &a, "length", JsValue::Number(n as f64)) {
                    interp.gc_unroot_frame(gc_frame);
                    return Completion::Throw(e);
                }
                interp.gc_unroot_frame(gc_frame);
                Completion::Normal(a)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("slice".to_string(), slice_fn);

        // Array.prototype.reverse
        let reverse_fn = self.create_function(JsFunction::native(
            "reverse".to_string(),
            0,
            |interp, this_val, _args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
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
                        JsValue::Undefined
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
                        JsValue::Undefined
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
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("reverse".to_string(), reverse_fn);

        // Array.prototype.toReversed
        let to_reversed_fn = self.create_function(JsFunction::native(
            "toReversed".to_string(),
            0,
            |interp, this_val, _args| {
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
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toReversed".to_string(), to_reversed_fn);

        // Array.prototype.forEach
        let foreach_fn = self.create_function(JsFunction::native(
            "forEach".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "forEach callback is not a function")
                {
                    return c;
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                for k in 0..len {
                    let pk = k.to_string();
                    match obj_has_throw(interp, &o, &pk) {
                        Ok(true) => {
                            let kvalue = match obj_get(interp, &o, &pk) {
                                Ok(v) => v,
                                Err(c) => return c,
                            };
                            let call_args = vec![kvalue, JsValue::Number(k as f64), o.clone()];
                            if let result @ Completion::Throw(_) =
                                interp.call_function(&callback, &this_arg, &call_args)
                            {
                                return result;
                            }
                        }
                        Err(e) => return Completion::Throw(e),
                        _ => {}
                    }
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("forEach".to_string(), foreach_fn);

        // Array.prototype.map
        let map_fn = self.create_function(JsFunction::native(
            "map".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "map callback is not a function")
                {
                    return c;
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let a = match array_species_create(interp, &o, len) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let gc_frame = interp.gc_root_frame();
                interp.gc_root_value(&a);
                for k in 0..len {
                    let pk = k.to_string();
                    match obj_has_throw(interp, &o, &pk) {
                        Ok(true) => {
                            let kvalue = match obj_get(interp, &o, &pk) {
                                Ok(v) => v,
                                Err(c) => {
                                    interp.gc_unroot_frame(gc_frame);
                                    return c;
                                }
                            };
                            match interp.call_function(
                                &callback,
                                &this_arg,
                                &[kvalue, JsValue::Number(k as f64), o.clone()],
                            ) {
                                Completion::Normal(v) => {
                                    if let Err(e) =
                                        create_data_property_or_throw(interp, &a, &pk, v)
                                    {
                                        interp.gc_unroot_frame(gc_frame);
                                        return Completion::Throw(e);
                                    }
                                }
                                other => {
                                    interp.gc_unroot_frame(gc_frame);
                                    return other;
                                }
                            }
                        }
                        Err(e) => {
                            interp.gc_unroot_frame(gc_frame);
                            return Completion::Throw(e);
                        }
                        _ => {}
                    }
                }
                interp.gc_unroot_frame(gc_frame);
                Completion::Normal(a)
            },
        ));
        proto.borrow_mut().insert_builtin("map".to_string(), map_fn);

        // Array.prototype.filter
        let filter_fn = self.create_function(JsFunction::native(
            "filter".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "filter callback is not a function")
                {
                    return c;
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let a = match array_species_create(interp, &o, 0) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let gc_frame = interp.gc_root_frame();
                interp.gc_root_value(&a);
                let mut to: usize = 0;
                for k in 0..len {
                    let pk = k.to_string();
                    match obj_has_throw(interp, &o, &pk) {
                        Ok(true) => {
                            let kvalue = match obj_get(interp, &o, &pk) {
                                Ok(v) => v,
                                Err(c) => {
                                    interp.gc_unroot_frame(gc_frame);
                                    return c;
                                }
                            };
                            match interp.call_function(
                                &callback,
                                &this_arg,
                                &[kvalue.clone(), JsValue::Number(k as f64), o.clone()],
                            ) {
                                Completion::Normal(v) => {
                                    if interp.to_boolean_val(&v) {
                                        if let Err(e) = create_data_property_or_throw(
                                            interp,
                                            &a,
                                            &to.to_string(),
                                            kvalue,
                                        ) {
                                            interp.gc_unroot_frame(gc_frame);
                                            return Completion::Throw(e);
                                        }
                                        to += 1;
                                    }
                                }
                                other => {
                                    interp.gc_unroot_frame(gc_frame);
                                    return other;
                                }
                            }
                        }
                        Err(e) => {
                            interp.gc_unroot_frame(gc_frame);
                            return Completion::Throw(e);
                        }
                        _ => {}
                    }
                }
                interp.gc_unroot_frame(gc_frame);
                Completion::Normal(a)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("filter".to_string(), filter_fn);

        // Array.prototype.reduce
        let reduce_fn = self.create_function(JsFunction::native(
            "reduce".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "reduce callback is not a function")
                {
                    return c;
                }
                if len == 0 && args.len() < 2 {
                    return Completion::Throw(
                        interp.create_type_error("Reduce of empty array with no initial value"),
                    );
                }
                let mut k = 0usize;
                let mut acc =
                    if args.len() >= 2 {
                        args[1].clone()
                    } else {
                        // Find first present element
                        loop {
                            if k >= len {
                                return Completion::Throw(interp.create_type_error(
                                    "Reduce of empty array with no initial value",
                                ));
                            }
                            let pk = k.to_string();
                            match obj_has_throw(interp, &o, &pk) {
                                Ok(true) => {
                                    let val = match obj_get(interp, &o, &pk) {
                                        Ok(v) => v,
                                        Err(c) => return c,
                                    };
                                    k += 1;
                                    break val;
                                }
                                Err(e) => return Completion::Throw(e),
                                _ => {}
                            }
                            k += 1;
                        }
                    };
                while k < len {
                    let pk = k.to_string();
                    match obj_has_throw(interp, &o, &pk) {
                        Ok(true) => {
                            let kvalue = match obj_get(interp, &o, &pk) {
                                Ok(v) => v,
                                Err(c) => return c,
                            };
                            match interp.call_function(
                                &callback,
                                &JsValue::Undefined,
                                &[acc, kvalue, JsValue::Number(k as f64), o.clone()],
                            ) {
                                Completion::Normal(v) => acc = v,
                                other => return other,
                            }
                        }
                        Err(e) => return Completion::Throw(e),
                        _ => {}
                    }
                    k += 1;
                }
                Completion::Normal(acc)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("reduce".to_string(), reduce_fn);

        // Array.prototype.reduceRight
        let reduce_right_fn = self.create_function(JsFunction::native(
            "reduceRight".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
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
                let mut acc =
                    if args.len() >= 2 {
                        args[1].clone()
                    } else {
                        loop {
                            if k < 0 {
                                return Completion::Throw(interp.create_type_error(
                                    "Reduce of empty array with no initial value",
                                ));
                            }
                            let pk = (k as usize).to_string();
                            match obj_has_throw(interp, &o, &pk) {
                                Ok(true) => {
                                    let val = match obj_get(interp, &o, &pk) {
                                        Ok(v) => v,
                                        Err(c) => return c,
                                    };
                                    k -= 1;
                                    break val;
                                }
                                Err(e) => return Completion::Throw(e),
                                _ => {}
                            }
                            k -= 1;
                        }
                    };
                while k >= 0 {
                    let pk = (k as usize).to_string();
                    match obj_has_throw(interp, &o, &pk) {
                        Ok(true) => {
                            let kvalue = match obj_get(interp, &o, &pk) {
                                Ok(v) => v,
                                Err(c) => return c,
                            };
                            match interp.call_function(
                                &callback,
                                &JsValue::Undefined,
                                &[acc, kvalue, JsValue::Number(k as f64), o.clone()],
                            ) {
                                Completion::Normal(v) => acc = v,
                                other => return other,
                            }
                        }
                        Err(e) => return Completion::Throw(e),
                        _ => {}
                    }
                    k -= 1;
                }
                Completion::Normal(acc)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("reduceRight".to_string(), reduce_right_fn);

        // Array.prototype.some
        let some_fn = self.create_function(JsFunction::native(
            "some".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "some callback is not a function")
                {
                    return c;
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                for k in 0..len {
                    let pk = k.to_string();
                    match obj_has_throw(interp, &o, &pk) {
                        Ok(true) => {
                            let kvalue = match obj_get(interp, &o, &pk) {
                                Ok(v) => v,
                                Err(c) => return c,
                            };
                            match interp.call_function(
                                &callback,
                                &this_arg,
                                &[kvalue, JsValue::Number(k as f64), o.clone()],
                            ) {
                                Completion::Normal(v) => {
                                    if interp.to_boolean_val(&v) {
                                        return Completion::Normal(JsValue::Boolean(true));
                                    }
                                }
                                other => return other,
                            }
                        }
                        Err(e) => return Completion::Throw(e),
                        _ => {}
                    }
                }
                Completion::Normal(JsValue::Boolean(false))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("some".to_string(), some_fn);

        // Array.prototype.every
        let every_fn = self.create_function(JsFunction::native(
            "every".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "every callback is not a function")
                {
                    return c;
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                for k in 0..len {
                    let pk = k.to_string();
                    match obj_has_throw(interp, &o, &pk) {
                        Ok(true) => {
                            let kvalue = match obj_get(interp, &o, &pk) {
                                Ok(v) => v,
                                Err(c) => return c,
                            };
                            match interp.call_function(
                                &callback,
                                &this_arg,
                                &[kvalue, JsValue::Number(k as f64), o.clone()],
                            ) {
                                Completion::Normal(v) => {
                                    if !interp.to_boolean_val(&v) {
                                        return Completion::Normal(JsValue::Boolean(false));
                                    }
                                }
                                other => return other,
                            }
                        }
                        Err(e) => return Completion::Throw(e),
                        _ => {}
                    }
                }
                Completion::Normal(JsValue::Boolean(true))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("every".to_string(), every_fn);

        // Array.prototype.find
        let find_fn = self.create_function(JsFunction::native(
            "find".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "find predicate is not a function")
                {
                    return c;
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                for k in 0..len {
                    let kvalue = match obj_get(interp, &o, &k.to_string()) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    match interp.call_function(
                        &callback,
                        &this_arg,
                        &[kvalue.clone(), JsValue::Number(k as f64), o.clone()],
                    ) {
                        Completion::Normal(v) => {
                            if interp.to_boolean_val(&v) {
                                return Completion::Normal(kvalue);
                            }
                        }
                        other => return other,
                    }
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("find".to_string(), find_fn);

        // Array.prototype.findIndex
        let findindex_fn = self.create_function(JsFunction::native(
            "findIndex".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "findIndex predicate is not a function")
                {
                    return c;
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                for k in 0..len {
                    let kvalue = match obj_get(interp, &o, &k.to_string()) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    match interp.call_function(
                        &callback,
                        &this_arg,
                        &[kvalue, JsValue::Number(k as f64), o.clone()],
                    ) {
                        Completion::Normal(v) => {
                            if interp.to_boolean_val(&v) {
                                return Completion::Normal(JsValue::Number(k as f64));
                            }
                        }
                        other => return other,
                    }
                }
                Completion::Normal(JsValue::Number(-1.0))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("findIndex".to_string(), findindex_fn);

        // Array.prototype.findLast
        let findlast_fn = self.create_function(JsFunction::native(
            "findLast".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "findLast predicate is not a function")
                {
                    return c;
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                for k in (0..len).rev() {
                    let kvalue = match obj_get(interp, &o, &k.to_string()) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    match interp.call_function(
                        &callback,
                        &this_arg,
                        &[kvalue.clone(), JsValue::Number(k as f64), o.clone()],
                    ) {
                        Completion::Normal(v) => {
                            if interp.to_boolean_val(&v) {
                                return Completion::Normal(kvalue);
                            }
                        }
                        other => return other,
                    }
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("findLast".to_string(), findlast_fn);

        // Array.prototype.findLastIndex
        let findlastidx_fn = self.create_function(JsFunction::native(
            "findLastIndex".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) = require_callable(
                    interp,
                    &callback,
                    "findLastIndex predicate is not a function",
                ) {
                    return c;
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                for k in (0..len).rev() {
                    let kvalue = match obj_get(interp, &o, &k.to_string()) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    match interp.call_function(
                        &callback,
                        &this_arg,
                        &[kvalue, JsValue::Number(k as f64), o.clone()],
                    ) {
                        Completion::Normal(v) => {
                            if interp.to_boolean_val(&v) {
                                return Completion::Normal(JsValue::Number(k as f64));
                            }
                        }
                        other => return other,
                    }
                }
                Completion::Normal(JsValue::Number(-1.0))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("findLastIndex".to_string(), findlastidx_fn);

        // Array.prototype.splice
        let splice_fn = self.create_function(JsFunction::native(
            "splice".to_string(),
            2,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v as i64,
                    Err(c) => return c,
                };
                let relative_start = if let Some(v) = args.first() {
                    match interp.to_number_value(v) {
                        Ok(n) => to_integer_or_infinity(n),
                        Err(e) => return Completion::Throw(e),
                    }
                } else {
                    0.0
                };
                let actual_start = if relative_start < 0.0 {
                    (len as f64 + relative_start).max(0.0) as usize
                } else {
                    (relative_start as i64).min(len) as usize
                };
                let insert_count = if args.len() > 2 { args.len() - 2 } else { 0 };
                let actual_delete_count = if args.is_empty() {
                    0usize
                } else if args.len() == 1 {
                    (len - actual_start as i64) as usize
                } else {
                    let dc = match interp.to_number_value(&args[1]) {
                        Ok(n) => to_integer_or_infinity(n),
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
                let gc_frame = interp.gc_root_frame();
                interp.gc_root_value(&a);
                for i in 0..actual_delete_count {
                    let from = (actual_start + i).to_string();
                    match obj_has_throw(interp, &o, &from) {
                        Ok(true) => {
                            let val = match obj_get(interp, &o, &from) {
                                Ok(v) => v,
                                Err(c) => {
                                    interp.gc_unroot_frame(gc_frame);
                                    return c;
                                }
                            };
                            if let Err(e) =
                                create_data_property_or_throw(interp, &a, &i.to_string(), val)
                            {
                                interp.gc_unroot_frame(gc_frame);
                                return Completion::Throw(e);
                            }
                        }
                        Err(e) => {
                            interp.gc_unroot_frame(gc_frame);
                            return Completion::Throw(e);
                        }
                        _ => {}
                    }
                }
                // Step 12: Perform ? Set(A, "length", actualDeleteCount, true).
                if let Err(e) = obj_set_throw(
                    interp,
                    &a,
                    "length",
                    JsValue::Number(actual_delete_count as f64),
                ) {
                    interp.gc_unroot_frame(gc_frame);
                    return Completion::Throw(e);
                }
                let items: Vec<JsValue> = args.iter().skip(2).cloned().collect();
                if insert_count < actual_delete_count {
                    for k in actual_start..((len as usize) - actual_delete_count) {
                        let from = (k + actual_delete_count).to_string();
                        let to = (k + insert_count).to_string();
                        let from_present = match obj_has_throw(interp, &o, &from) {
                            Ok(v) => v,
                            Err(e) => {
                                interp.gc_unroot_frame(gc_frame);
                                return Completion::Throw(e);
                            }
                        };
                        if from_present {
                            let val = match obj_get(interp, &o, &from) {
                                Ok(v) => v,
                                Err(c) => {
                                    interp.gc_unroot_frame(gc_frame);
                                    return c;
                                }
                            };
                            if let Err(e) = obj_set_throw(interp, &o, &to, val) {
                                interp.gc_unroot_frame(gc_frame);
                                return Completion::Throw(e);
                            }
                        } else if let Err(e) = obj_delete_throw(interp, &o, &to) {
                            interp.gc_unroot_frame(gc_frame);
                            return Completion::Throw(e);
                        }
                    }
                    for k in
                        ((len as usize - actual_delete_count + insert_count)..(len as usize)).rev()
                    {
                        if let Err(e) = obj_delete_throw(interp, &o, &k.to_string()) {
                            interp.gc_unroot_frame(gc_frame);
                            return Completion::Throw(e);
                        }
                    }
                } else if insert_count > actual_delete_count {
                    for k in (actual_start..((len as usize) - actual_delete_count)).rev() {
                        let from = (k + actual_delete_count).to_string();
                        let to = (k + insert_count).to_string();
                        let from_present = match obj_has_throw(interp, &o, &from) {
                            Ok(v) => v,
                            Err(e) => {
                                interp.gc_unroot_frame(gc_frame);
                                return Completion::Throw(e);
                            }
                        };
                        if from_present {
                            let val = match obj_get(interp, &o, &from) {
                                Ok(v) => v,
                                Err(c) => {
                                    interp.gc_unroot_frame(gc_frame);
                                    return c;
                                }
                            };
                            if let Err(e) = obj_set_throw(interp, &o, &to, val) {
                                interp.gc_unroot_frame(gc_frame);
                                return Completion::Throw(e);
                            }
                        } else if let Err(e) = obj_delete_throw(interp, &o, &to) {
                            interp.gc_unroot_frame(gc_frame);
                            return Completion::Throw(e);
                        }
                    }
                }
                for (j, item) in items.into_iter().enumerate() {
                    if let Err(e) = obj_set_throw(interp, &o, &(actual_start + j).to_string(), item)
                    {
                        interp.gc_unroot_frame(gc_frame);
                        return Completion::Throw(e);
                    }
                }
                let new_len = (len as usize) - actual_delete_count + insert_count;
                if let Err(e) = set_length_throw(interp, &o, new_len) {
                    interp.gc_unroot_frame(gc_frame);
                    return Completion::Throw(e);
                }
                interp.gc_unroot_frame(gc_frame);
                Completion::Normal(a)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("splice".to_string(), splice_fn);

        // Array.prototype.toSpliced
        let to_spliced_fn = self.create_function(JsFunction::native(
            "toSpliced".to_string(),
            2,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v as i64,
                    Err(c) => return c,
                };
                let relative_start = if let Some(v) = args.first() {
                    match interp.to_number_value(v) {
                        Ok(n) => to_integer_or_infinity(n),
                        Err(e) => return Completion::Throw(e),
                    }
                } else {
                    0.0
                };
                let actual_start = if relative_start < 0.0 {
                    (len as f64 + relative_start).max(0.0) as usize
                } else {
                    (relative_start as i64).min(len) as usize
                };
                let actual_delete_count = if args.is_empty() {
                    0usize
                } else if args.len() == 1 {
                    (len - actual_start as i64) as usize
                } else {
                    let dc = match interp.to_number_value(&args[1]) {
                        Ok(n) => to_integer_or_infinity(n),
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
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toSpliced".to_string(), to_spliced_fn);

        // Array.prototype.fill
        let fill_fn = self.create_function(JsFunction::native(
            "fill".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v as i64,
                    Err(c) => return c,
                };
                let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                let relative_start = if let Some(v) = args.get(1) {
                    match interp.to_number_value(v) {
                        Ok(n) => to_integer_or_infinity(n),
                        Err(e) => return Completion::Throw(e),
                    }
                } else {
                    0.0
                };
                let k = if relative_start < 0.0 {
                    (len as f64 + relative_start).max(0.0) as usize
                } else {
                    (relative_start as i64).min(len) as usize
                };
                let relative_end = if let Some(v) = args.get(2) {
                    if matches!(v, JsValue::Undefined) {
                        len as f64
                    } else {
                        match interp.to_number_value(v) {
                            Ok(n) => to_integer_or_infinity(n),
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                } else {
                    len as f64
                };
                let fin = if relative_end < 0.0 {
                    (len as f64 + relative_end).max(0.0) as usize
                } else {
                    (relative_end as i64).min(len) as usize
                };
                for i in k..fin {
                    if let Err(e) = obj_set_throw(interp, &o, &i.to_string(), value.clone()) {
                        return Completion::Throw(e);
                    }
                }
                Completion::Normal(o)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("fill".to_string(), fill_fn);

        // Array.prototype.sort
        let sort_fn = self.create_function(JsFunction::native(
            "sort".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let compare_fn = args.first().cloned();
                if let Some(ref cf) = compare_fn
                    && !matches!(cf, JsValue::Undefined)
                    && !interp.is_callable(cf)
                {
                    return Completion::Throw(
                        interp.create_type_error("compareFn is not a function"),
                    );
                }
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let mut items: Vec<JsValue> = Vec::with_capacity(len);
                for i in 0..len {
                    let pk = i.to_string();
                    match obj_has_throw(interp, &o, &pk) {
                        Ok(true) => {
                            items.push(match obj_get(interp, &o, &pk) {
                                Ok(v) => v,
                                Err(c) => return c,
                            });
                        }
                        Err(e) => return Completion::Throw(e),
                        _ => {}
                    }
                }
                let cmp_fn = compare_fn.clone();
                let mut sort_error: Option<JsValue> = None;
                items.sort_by(|x, y| {
                    if sort_error.is_some() {
                        return std::cmp::Ordering::Equal;
                    }
                    if matches!(x, JsValue::Undefined) && matches!(y, JsValue::Undefined) {
                        return std::cmp::Ordering::Equal;
                    }
                    if matches!(x, JsValue::Undefined) {
                        return std::cmp::Ordering::Greater;
                    }
                    if matches!(y, JsValue::Undefined) {
                        return std::cmp::Ordering::Less;
                    }
                    if let Some(ref cf) = cmp_fn
                        && !matches!(cf, JsValue::Undefined)
                        && interp.is_callable(cf)
                    {
                        let result =
                            interp.call_function(cf, &JsValue::Undefined, &[x.clone(), y.clone()]);
                        match result {
                            Completion::Normal(v) => {
                                let n = match interp.to_number_value(&v) {
                                    Ok(n) => n,
                                    Err(e) => {
                                        sort_error = Some(e);
                                        return std::cmp::Ordering::Equal;
                                    }
                                };
                                if n.is_nan() {
                                    return std::cmp::Ordering::Equal;
                                }
                                if n < 0.0 {
                                    return std::cmp::Ordering::Less;
                                }
                                if n > 0.0 {
                                    return std::cmp::Ordering::Greater;
                                }
                                return std::cmp::Ordering::Equal;
                            }
                            Completion::Throw(e) => {
                                sort_error = Some(e);
                                return std::cmp::Ordering::Equal;
                            }
                            _ => return std::cmp::Ordering::Equal,
                        }
                    }
                    let xs = match interp.to_string_value(x) {
                        Ok(s) => s,
                        Err(e) => {
                            sort_error = Some(e);
                            return std::cmp::Ordering::Equal;
                        }
                    };
                    let ys = match interp.to_string_value(y) {
                        Ok(s) => s,
                        Err(e) => {
                            sort_error = Some(e);
                            return std::cmp::Ordering::Equal;
                        }
                    };
                    xs.cmp(&ys)
                });
                if let Some(e) = sort_error {
                    return Completion::Throw(e);
                }
                // Write back
                for (i, v) in items.iter().enumerate() {
                    if let Err(e) = obj_set_throw(interp, &o, &i.to_string(), v.clone()) {
                        return Completion::Throw(e);
                    }
                }
                for i in items.len()..len {
                    if let Err(e) = obj_delete_throw(interp, &o, &i.to_string()) {
                        return Completion::Throw(e);
                    }
                }
                Completion::Normal(o)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("sort".to_string(), sort_fn);

        // Array.prototype.toSorted
        let to_sorted_fn = self.create_function(JsFunction::native(
            "toSorted".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let compare_fn = args.first().cloned();
                if let Some(ref cf) = compare_fn
                    && !matches!(cf, JsValue::Undefined)
                    && !interp.is_callable(cf)
                {
                    return Completion::Throw(
                        interp.create_type_error("compareFn is not a function"),
                    );
                }
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                // ArrayCreate(len) — If len > 2^32-1, throw RangeError
                if len as u64 > 0xFFFF_FFFF {
                    return Completion::Throw(interp.create_range_error("Invalid array length"));
                }
                let mut items: Vec<JsValue> = Vec::with_capacity(len);
                for i in 0..len {
                    items.push(match obj_get(interp, &o, &i.to_string()) {
                        Ok(v) => v,
                        Err(c) => return c,
                    });
                }
                let cmp_fn = compare_fn.clone();
                let mut sort_error: Option<JsValue> = None;
                items.sort_by(|x, y| {
                    if sort_error.is_some() {
                        return std::cmp::Ordering::Equal;
                    }
                    if matches!(x, JsValue::Undefined) && matches!(y, JsValue::Undefined) {
                        return std::cmp::Ordering::Equal;
                    }
                    if matches!(x, JsValue::Undefined) {
                        return std::cmp::Ordering::Greater;
                    }
                    if matches!(y, JsValue::Undefined) {
                        return std::cmp::Ordering::Less;
                    }
                    if let Some(ref cf) = cmp_fn
                        && !matches!(cf, JsValue::Undefined)
                        && interp.is_callable(cf)
                    {
                        let result =
                            interp.call_function(cf, &JsValue::Undefined, &[x.clone(), y.clone()]);
                        match result {
                            Completion::Normal(v) => {
                                let n = match interp.to_number_value(&v) {
                                    Ok(n) => n,
                                    Err(e) => {
                                        sort_error = Some(e);
                                        return std::cmp::Ordering::Equal;
                                    }
                                };
                                if n.is_nan() {
                                    return std::cmp::Ordering::Equal;
                                }
                                if n < 0.0 {
                                    return std::cmp::Ordering::Less;
                                }
                                if n > 0.0 {
                                    return std::cmp::Ordering::Greater;
                                }
                                return std::cmp::Ordering::Equal;
                            }
                            Completion::Throw(e) => {
                                sort_error = Some(e);
                                return std::cmp::Ordering::Equal;
                            }
                            _ => {}
                        }
                    }
                    let xs = match interp.to_string_value(x) {
                        Ok(s) => s,
                        Err(e) => {
                            sort_error = Some(e);
                            return std::cmp::Ordering::Equal;
                        }
                    };
                    let ys = match interp.to_string_value(y) {
                        Ok(s) => s,
                        Err(e) => {
                            sort_error = Some(e);
                            return std::cmp::Ordering::Equal;
                        }
                    };
                    xs.cmp(&ys)
                });
                if let Some(e) = sort_error {
                    return Completion::Throw(e);
                }
                Completion::Normal(interp.create_array(items))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toSorted".to_string(), to_sorted_fn);

        // Array.prototype.flat
        let flat_fn = self.create_function(JsFunction::native(
            "flat".to_string(),
            0,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let depth_num = if let Some(d) = args.first() {
                    if matches!(d, JsValue::Undefined) {
                        1.0
                    } else {
                        match interp.to_number_value(d) {
                            Ok(n) => to_integer_or_infinity(n),
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
                let gc_frame = interp.gc_root_frame();
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
                                if let JsValue::Object(eo) = &elem {
                                    is_array_check(interp, eo.id).map_err(Completion::Throw)?
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
                    interp.gc_unroot_frame(gc_frame);
                    return c;
                }
                interp.gc_unroot_frame(gc_frame);
                Completion::Normal(a)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("flat".to_string(), flat_fn);

        // Array.prototype.flatMap
        let flatmap_fn = self.create_function(JsFunction::native(
            "flatMap".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "flatMap callback is not a function")
                {
                    return c;
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                // Step 5: ArraySpeciesCreate BEFORE iteration (spec order)
                let a = match array_species_create(interp, &o, 0) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let gc_frame = interp.gc_root_frame();
                interp.gc_root_value(&a);
                let mut target_index = 0usize;
                for k in 0..len {
                    let pk = k.to_string();
                    match obj_has_throw(interp, &o, &pk) {
                        Ok(true) => {
                            let kvalue = match obj_get(interp, &o, &pk) {
                                Ok(v) => v,
                                Err(c) => {
                                    interp.gc_unroot_frame(gc_frame);
                                    return c;
                                }
                            };
                            let mapped = interp.call_function(
                                &callback,
                                &this_arg,
                                &[kvalue, JsValue::Number(k as f64), o.clone()],
                            );
                            match mapped {
                                Completion::Normal(v) => {
                                    let elem_is_array = if let JsValue::Object(mo) = &v {
                                        match is_array_check(interp, mo.id) {
                                            Ok(b) => b,
                                            Err(e) => {
                                                interp.gc_unroot_frame(gc_frame);
                                                return Completion::Throw(e);
                                            }
                                        }
                                    } else {
                                        false
                                    };
                                    if elem_is_array {
                                        let mlen = match length_of_array_like(interp, &v) {
                                            Ok(l) => l,
                                            Err(c) => {
                                                interp.gc_unroot_frame(gc_frame);
                                                return c;
                                            }
                                        };
                                        for j in 0..mlen {
                                            let jpk = j.to_string();
                                            match obj_has_throw(interp, &v, &jpk) {
                                                Ok(true) => {
                                                    let jval = match obj_get(interp, &v, &jpk) {
                                                        Ok(v) => v,
                                                        Err(c) => {
                                                            interp.gc_unroot_frame(gc_frame);
                                                            return c;
                                                        }
                                                    };
                                                    if let Err(e) = create_data_property_or_throw(
                                                        interp,
                                                        &a,
                                                        &target_index.to_string(),
                                                        jval,
                                                    ) {
                                                        interp.gc_unroot_frame(gc_frame);
                                                        return Completion::Throw(e);
                                                    }
                                                    target_index += 1;
                                                }
                                                Err(e) => {
                                                    interp.gc_unroot_frame(gc_frame);
                                                    return Completion::Throw(e);
                                                }
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
                                            interp.gc_unroot_frame(gc_frame);
                                            return Completion::Throw(e);
                                        }
                                        target_index += 1;
                                    }
                                }
                                other => {
                                    interp.gc_unroot_frame(gc_frame);
                                    return other;
                                }
                            }
                        }
                        Err(e) => {
                            interp.gc_unroot_frame(gc_frame);
                            return Completion::Throw(e);
                        }
                        _ => {}
                    }
                }
                interp.gc_unroot_frame(gc_frame);
                Completion::Normal(a)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("flatMap".to_string(), flatmap_fn);

        // Array.prototype.copyWithin
        let copywithin_fn = self.create_function(JsFunction::native(
            "copyWithin".to_string(),
            2,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v as i64,
                    Err(c) => return c,
                };
                let relative_target = if let Some(v) = args.first() {
                    match interp.to_number_value(v) {
                        Ok(n) => to_integer_or_infinity(n),
                        Err(e) => return Completion::Throw(e),
                    }
                } else {
                    0.0
                };
                let to_val = if relative_target < 0.0 {
                    (len as f64 + relative_target).max(0.0) as i64
                } else {
                    (relative_target as i64).min(len)
                };
                let relative_start = if let Some(v) = args.get(1) {
                    match interp.to_number_value(v) {
                        Ok(n) => to_integer_or_infinity(n),
                        Err(e) => return Completion::Throw(e),
                    }
                } else {
                    0.0
                };
                let from = if relative_start < 0.0 {
                    (len as f64 + relative_start).max(0.0) as i64
                } else {
                    (relative_start as i64).min(len)
                };
                let relative_end = if let Some(v) = args.get(2) {
                    if matches!(v, JsValue::Undefined) {
                        len as f64
                    } else {
                        match interp.to_number_value(v) {
                            Ok(n) => to_integer_or_infinity(n),
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                } else {
                    len as f64
                };
                let fin = if relative_end < 0.0 {
                    (len as f64 + relative_end).max(0.0) as i64
                } else {
                    (relative_end as i64).min(len)
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
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("copyWithin".to_string(), copywithin_fn);

        // Array.prototype.at
        let at_fn = self.create_function(JsFunction::native(
            "at".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v as i64,
                    Err(c) => return c,
                };
                let relative_index = if let Some(v) = args.first() {
                    match interp.to_number_value(v) {
                        Ok(n) => to_integer_or_infinity(n) as i64,
                        Err(e) => return Completion::Throw(e),
                    }
                } else {
                    0
                };
                let k = if relative_index >= 0 {
                    relative_index
                } else {
                    len + relative_index
                };
                if k < 0 || k >= len {
                    return Completion::Normal(JsValue::Undefined);
                }
                match obj_get(interp, &o, &(k as usize).to_string()) {
                    Ok(v) => Completion::Normal(v),
                    Err(c) => c,
                }
            },
        ));
        proto.borrow_mut().insert_builtin("at".to_string(), at_fn);

        // Array.prototype.with
        let with_fn = self.create_function(JsFunction::native(
            "with".to_string(),
            2,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v as i64,
                    Err(c) => return c,
                };
                if len as u64 > 0xFFFF_FFFF {
                    return Completion::Throw(interp.create_range_error("Invalid array length"));
                }
                let relative_index = if let Some(v) = args.first() {
                    match interp.to_number_value(v) {
                        Ok(n) => to_integer_or_infinity(n) as i64,
                        Err(e) => return Completion::Throw(e),
                    }
                } else {
                    0
                };
                let actual = if relative_index >= 0 {
                    relative_index
                } else {
                    len + relative_index
                };
                if actual < 0 || actual >= len {
                    return Completion::Throw(interp.create_range_error("Invalid index"));
                }
                let value = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let mut result = Vec::with_capacity(len as usize);
                for k in 0..len as usize {
                    if k == actual as usize {
                        result.push(value.clone());
                    } else {
                        result.push(match obj_get(interp, &o, &k.to_string()) {
                            Ok(v) => v,
                            Err(c) => return c,
                        });
                    }
                }
                Completion::Normal(interp.create_array(result))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("with".to_string(), with_fn);

        // Array.isArray
        let is_array_fn = self.create_function(JsFunction::native(
            "isArray".to_string(),
            1,
            |interp, _this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = &val {
                    match is_array_check(interp, o.id) {
                        Ok(result) => Completion::Normal(JsValue::Boolean(result)),
                        Err(e) => Completion::Throw(e),
                    }
                } else {
                    Completion::Normal(JsValue::Boolean(false))
                }
            },
        ));

        // Array.from
        let array_from = self.create_function(JsFunction::native(
            "from".to_string(),
            1,
            |interp, this_val, args: &[JsValue]| {
                let c = this_val.clone();
                let source = args.first().cloned().unwrap_or(JsValue::Undefined);
                let map_fn = args.get(1).cloned();
                let this_arg = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                let mapping = if let Some(ref mf) = map_fn
                    && !matches!(mf, JsValue::Undefined)
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
                if matches!(source, JsValue::Undefined | JsValue::Null) {
                    return Completion::Throw(
                        interp.create_type_error("Cannot convert undefined or null to object"),
                    );
                }

                // §23.1.2.1 step 4: Let usingIterator be ? GetMethod(items, @@iterator).
                // GetMethod calls GetV which works on any value via ToObject.
                let using_iterator = if let Some(key) = interp.get_symbol_key("iterator") {
                    let obj_val = match interp.to_object(&source) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        other => return other,
                    };
                    if let JsValue::Object(o) = &obj_val {
                        match interp.get_object_property(o.id, &key, &source) {
                            Completion::Normal(v)
                                if !matches!(v, JsValue::Undefined | JsValue::Null) =>
                            {
                                Some(v)
                            }
                            Completion::Normal(_) => None,
                            Completion::Throw(e) => return Completion::Throw(e),
                            other => return other,
                        }
                    } else {
                        None
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
                    let gc_frame = interp.gc_root_frame();
                    interp.gc_root_value(&a);

                    let iterator = match interp.call_function(&iter_method, &source, &[]) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            interp.gc_unroot_frame(gc_frame);
                            return Completion::Throw(e);
                        }
                        other => {
                            interp.gc_unroot_frame(gc_frame);
                            return other;
                        }
                    };
                    interp.gc_root_value(&iterator);
                    let mut k: usize = 0;
                    loop {
                        let next = match interp.iterator_step(&iterator) {
                            Ok(v) => v,
                            Err(e) => {
                                interp.gc_unroot_frame(gc_frame);
                                return Completion::Throw(e);
                            }
                        };
                        let next = match next {
                            Some(result) => result,
                            None => {
                                if let Err(e) = set_length_throw(interp, &a, k) {
                                    interp.gc_unroot_frame(gc_frame);
                                    return Completion::Throw(e);
                                }
                                interp.gc_unroot_frame(gc_frame);
                                return Completion::Normal(a);
                            }
                        };
                        let gc_frame_next = interp.gc_root_frame();
                        interp.gc_root_value(&next);
                        let value = match interp.iterator_value(&next) {
                            Ok(v) => v,
                            Err(e) => {
                                interp.gc_unroot_frame(gc_frame);
                                return Completion::Throw(e);
                            }
                        };
                        let mapped_value = if mapping {
                            match interp.call_function(
                                map_fn.as_ref().unwrap(),
                                &this_arg,
                                &[value, JsValue::Number(k as f64)],
                            ) {
                                Completion::Normal(v) => v,
                                other => {
                                    let _ = interp.iterator_close(&iterator, JsValue::Undefined);
                                    interp.gc_unroot_frame(gc_frame);
                                    return other;
                                }
                            }
                        } else {
                            value
                        };
                        if let Err(e) =
                            create_data_property_or_throw(interp, &a, &k.to_string(), mapped_value)
                        {
                            let _ = interp.iterator_close(&iterator, JsValue::Undefined);
                            interp.gc_unroot_frame(gc_frame);
                            return Completion::Throw(e);
                        }
                        interp.gc_unroot_frame(gc_frame_next);
                        k += 1;
                    }
                } else {
                    // Array-like path
                    let gc_frame = interp.gc_root_frame();
                    let array_like = match to_object_val(interp, &source) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    interp.gc_root_value(&array_like);
                    let len = match length_of_array_like(interp, &array_like) {
                        Ok(v) => v,
                        Err(c) => {
                            interp.gc_unroot_frame(gc_frame);
                            return c;
                        }
                    };

                    let a = if interp.is_constructor(&c) {
                        match interp.construct(&c, &[JsValue::Number(len as f64)]) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                interp.gc_unroot_frame(gc_frame);
                                return Completion::Throw(e);
                            }
                            other => {
                                interp.gc_unroot_frame(gc_frame);
                                return other;
                            }
                        }
                    } else {
                        interp.create_array(Vec::new())
                    };
                    interp.gc_root_value(&a);

                    for k in 0..len {
                        let kvalue = match obj_get(interp, &array_like, &k.to_string()) {
                            Ok(v) => v,
                            Err(c) => {
                                interp.gc_unroot_frame(gc_frame);
                                return c;
                            }
                        };
                        let mapped_value = if mapping {
                            match interp.call_function(
                                map_fn.as_ref().unwrap(),
                                &this_arg,
                                &[kvalue, JsValue::Number(k as f64)],
                            ) {
                                Completion::Normal(v) => v,
                                other => {
                                    interp.gc_unroot_frame(gc_frame);
                                    return other;
                                }
                            }
                        } else {
                            kvalue
                        };
                        if let Err(e) =
                            create_data_property_or_throw(interp, &a, &k.to_string(), mapped_value)
                        {
                            interp.gc_unroot_frame(gc_frame);
                            return Completion::Throw(e);
                        }
                    }
                    if let Err(e) = set_length_throw(interp, &a, len) {
                        interp.gc_unroot_frame(gc_frame);
                        return Completion::Throw(e);
                    }
                    interp.gc_unroot_frame(gc_frame);
                    Completion::Normal(a)
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
                let a = if interp.is_constructor(&c) {
                    match interp.construct(&c, &[JsValue::Number(len as f64)]) {
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
        let entries_fn = self.create_function(JsFunction::native(
            "entries".to_string(),
            0,
            |interp, this_val, _args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if let JsValue::Object(obj_ref) = &o {
                    return Completion::Normal(
                        interp.create_array_iterator(obj_ref.id, IteratorKind::KeyValue),
                    );
                }
                Completion::Throw(interp.create_type_error("entries called on non-object"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("entries".to_string(), entries_fn);

        // Array.prototype.keys
        let keys_fn = self.create_function(JsFunction::native(
            "keys".to_string(),
            0,
            |interp, this_val, _args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if let JsValue::Object(obj_ref) = &o {
                    return Completion::Normal(
                        interp.create_array_iterator(obj_ref.id, IteratorKind::Key),
                    );
                }
                Completion::Throw(interp.create_type_error("keys called on non-object"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("keys".to_string(), keys_fn);

        // Array.prototype.values
        let values_fn = self.create_function(JsFunction::native(
            "values".to_string(),
            0,
            |interp, this_val, _args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if let JsValue::Object(obj_ref) = &o {
                    return Completion::Normal(
                        interp.create_array_iterator(obj_ref.id, IteratorKind::Value),
                    );
                }
                Completion::Throw(interp.create_type_error("values called on non-object"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("values".to_string(), values_fn.clone());

        // Array.prototype[@@iterator] is the same function as Array.prototype.values (spec §23.1.3.35)
        if let Some(key) = self.get_symbol_iterator_key() {
            proto
                .borrow_mut()
                .insert_property(key, PropertyDescriptor::data(values_fn, true, false, true));
        }

        // Array.fromAsync
        let from_async_fn = self.create_function(JsFunction::native(
            "fromAsync".to_string(),
            1,
            |interp, this, args| {
                let async_items = args.first().cloned().unwrap_or(JsValue::Undefined);
                let map_fn = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let this_arg = args.get(2).cloned().unwrap_or(JsValue::Undefined);

                let has_map = !matches!(map_fn, JsValue::Undefined);
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
                    if let JsValue::Object(ref o) = async_items {
                        let val = match interp.get_object_property(o.id, key, &async_items) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return interp.create_rejected_promise(e),
                            _ => JsValue::Undefined,
                        };
                        if !matches!(val, JsValue::Undefined | JsValue::Null) {
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
                        if let JsValue::Object(ref o) = async_items {
                            match interp.get_object_property(o.id, key, &async_items) {
                                Completion::Normal(v) if !matches!(v, JsValue::Undefined | JsValue::Null) => Some(v),
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
                let promise_id = if let JsValue::Object(ref o) = promise { o.id } else { 0 };
                let (resolve_fn, reject_fn) = interp.create_resolving_functions(promise_id);

                // Step 7: If usingAsyncIterator is not undefined OR usingSyncIterator is not undefined
                if has_async_iter || using_sync_iterator.is_some() {
                    // Get iterator
                    let iterator = if has_async_iter {
                        if let Some(ref iter_fn) = using_iterator {
                            match interp.call_function(iter_fn, &async_items, &[]) {
                                Completion::Normal(v) if matches!(v, JsValue::Object(_)) => v,
                                Completion::Normal(_) => {
                                    let err = interp.create_type_error("Result of the Symbol.asyncIterator method is not an object");
                                    let _ = interp.call_function(&reject_fn, &JsValue::Undefined, &[err]);
                                    return Completion::Normal(promise);
                                }
                                Completion::Throw(e) => {
                                    let _ = interp.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                    return Completion::Normal(promise);
                                }
                                _ => {
                                    let err = interp.create_type_error("is not async iterable");
                                    let _ = interp.call_function(&reject_fn, &JsValue::Undefined, &[err]);
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
                                    Completion::Normal(v) if matches!(v, JsValue::Object(_)) => {
                                        interp.create_async_from_sync_iterator(v)
                                    }
                                    Completion::Normal(_) => {
                                        let err = interp.create_type_error("Result of the Symbol.iterator method is not an object");
                                        let _ = interp.call_function(&reject_fn, &JsValue::Undefined, &[err]);
                                        return Completion::Normal(promise);
                                    }
                                    Completion::Throw(e) => {
                                        let _ = interp.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                        return Completion::Normal(promise);
                                    }
                                    _ => {
                                        let err = interp.create_type_error("is not iterable");
                                        let _ = interp.call_function(&reject_fn, &JsValue::Undefined, &[err]);
                                        return Completion::Normal(promise);
                                    }
                                }
                            }
                            None => unreachable!(),
                        }
                    };

                    // Create the result array — spec: Construct(C) with no args for iterable path
                    let is_constructor = interp.is_constructor(this);
                    let arr = if is_constructor {
                        match interp.construct(this, &[]) {
                            Completion::Normal(v) if matches!(v, JsValue::Object(_)) => v,
                            Completion::Throw(e) => {
                                let _ = interp.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                return Completion::Normal(promise);
                            }
                            _ => interp.create_array(vec![]),
                        }
                    } else {
                        interp.create_array(vec![])
                    };

                    let state = Rc::new(RefCell::new(FromAsyncState {
                        iterator,
                        arr,
                        k: 0,
                        has_map,
                        map_fn,
                        this_arg,
                        resolve_fn,
                        reject_fn,
                        is_array_like: false,
                        array_like: JsValue::Undefined,
                        len: 0,
                        gc_roots: Vec::new(),
                    }));
                    from_async_gc_root(interp, &state);
                    from_async_iter_step(interp, state);
                    Completion::Normal(promise)
                } else {
                    // Array-like path
                    let array_like = match interp.to_object(&async_items) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            let _ = interp.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                            return Completion::Normal(promise);
                        }
                        _ => {
                            let err = interp.create_type_error("Cannot convert to object");
                            let _ = interp.call_function(&reject_fn, &JsValue::Undefined, &[err]);
                            return Completion::Normal(promise);
                        }
                    };
                    let len = match length_of_array_like(interp, &array_like) {
                        Ok(n) => n as u64,
                        Err(Completion::Throw(e)) => {
                            let _ = interp.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                            return Completion::Normal(promise);
                        }
                        Err(_) => 0u64,
                    };

                    let is_constructor = interp.is_constructor(this);
                    let arr = if is_constructor {
                        match interp.construct(this, &[JsValue::Number(len as f64)]) {
                            Completion::Normal(v) if matches!(v, JsValue::Object(_)) => v,
                            Completion::Throw(e) => {
                                let _ = interp.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                return Completion::Normal(promise);
                            }
                            _ => interp.create_array(vec![]),
                        }
                    } else {
                        if len > 0xFFFF_FFFF {
                            let e = interp.create_range_error("Invalid array length");
                            let _ = interp.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                            return Completion::Normal(promise);
                        }
                        interp.create_array(vec![])
                    };

                    let state = Rc::new(RefCell::new(FromAsyncState {
                        iterator: JsValue::Undefined,
                        arr,
                        k: 0,
                        has_map,
                        map_fn,
                        this_arg,
                        resolve_fn,
                        reject_fn,
                        is_array_like: true,
                        array_like,
                        len,
                        gc_roots: Vec::new(),
                    }));
                    from_async_gc_root(interp, &state);
                    from_async_arraylike_step(interp, state);
                    Completion::Normal(promise)
                }
            },
        ));

        // Set Array statics on the Array constructor
        let array_val = self.realm().global_env.borrow().get("Array");
        if let Some(array_val) = array_val
            && let JsValue::Object(o) = &array_val
            && let Some(obj) = self.get_object(o.id)
        {
            obj.borrow_mut()
                .insert_builtin("isArray".to_string(), is_array_fn);
            obj.borrow_mut()
                .insert_builtin("from".to_string(), array_from);
            obj.borrow_mut().insert_builtin("of".to_string(), array_of);
            obj.borrow_mut()
                .insert_builtin("fromAsync".to_string(), from_async_fn);
            let proto_val = JsValue::Object(crate::types::JsObject {
                id: proto.borrow().id.unwrap(),
            });
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val, false, false, false),
            );

            // Array[Symbol.species] getter - returns `this`
            let species_getter = self.create_function(JsFunction::native(
                "get [Symbol.species]".to_string(),
                0,
                |_interp, this_val, _args| Completion::Normal(this_val.clone()),
            ));
            obj.borrow_mut().insert_property(
                "Symbol(Symbol.species)".to_string(),
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
            proto
                .borrow_mut()
                .insert_builtin("constructor".to_string(), array_val);
        }

        // Array.prototype[@@unscopables] (§23.1.3.38)
        {
            let unscopables_obj = self.create_object();
            unscopables_obj.borrow_mut().prototype = None;
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
                unscopables_obj
                    .borrow_mut()
                    .insert_value(name.to_string(), JsValue::Boolean(true));
            }
            let unscopables_id = unscopables_obj.borrow().id.unwrap();
            let unscopables_val = JsValue::Object(crate::types::JsObject { id: unscopables_id });
            proto.borrow_mut().insert_property(
                "Symbol(Symbol.unscopables)".to_string(),
                PropertyDescriptor::data(unscopables_val, false, false, true),
            );
        }

        self.realm_mut().array_prototype = Some(proto);
    }

    pub(crate) fn create_array(&mut self, values: Vec<JsValue>) -> JsValue {
        let mut obj_data = JsObjectData::new();
        obj_data.prototype = self
            .realm()
            .array_prototype
            .clone()
            .or(self.realm().object_prototype.clone());
        obj_data.class_name = "Array".to_string();
        for (i, v) in values.iter().enumerate() {
            obj_data.insert_value(i.to_string(), v.clone());
        }
        obj_data.insert_property(
            "length".to_string(),
            PropertyDescriptor::data(JsValue::Number(values.len() as f64), true, false, false),
        );
        obj_data.array_elements = Some(values);
        let obj = Rc::new(RefCell::new(obj_data));
        let id = self.allocate_object_slot(obj);
        JsValue::Object(crate::types::JsObject { id })
    }

    pub(crate) fn create_array_with_holes(&mut self, items: Vec<Option<JsValue>>) -> JsValue {
        let len = items.len();
        let mut obj_data = JsObjectData::new();
        obj_data.prototype = self
            .realm()
            .array_prototype
            .clone()
            .or(self.realm().object_prototype.clone());
        obj_data.class_name = "Array".to_string();
        let mut array_elements = Vec::with_capacity(len);
        for (i, item) in items.into_iter().enumerate() {
            match item {
                Some(v) => {
                    obj_data.insert_value(i.to_string(), v.clone());
                    array_elements.push(v);
                }
                None => {
                    // Elision: no own property, but fill array_elements with undefined for indexing
                    array_elements.push(JsValue::Undefined);
                }
            }
        }
        obj_data.insert_property(
            "length".to_string(),
            PropertyDescriptor::data(JsValue::Number(len as f64), true, false, false),
        );
        obj_data.array_elements = Some(array_elements);
        let obj = Rc::new(RefCell::new(obj_data));
        let id = self.allocate_object_slot(obj);
        JsValue::Object(crate::types::JsObject { id })
    }

    pub(crate) fn create_array_with_length(&mut self, len: usize) -> JsValue {
        let mut obj_data = JsObjectData::new();
        obj_data.prototype = self
            .realm()
            .array_prototype
            .clone()
            .or(self.realm().object_prototype.clone());
        obj_data.class_name = "Array".to_string();
        obj_data.insert_property(
            "length".to_string(),
            PropertyDescriptor::data(JsValue::Number(len as f64), true, false, false),
        );
        // Use a small Vec for sparse arrays — don't pre-allocate huge arrays
        obj_data.array_elements = Some(Vec::new());
        let obj = Rc::new(RefCell::new(obj_data));
        let id = self.allocate_object_slot(obj);
        JsValue::Object(crate::types::JsObject { id })
    }
}
