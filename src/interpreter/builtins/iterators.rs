use super::super::*;

type ZipState = Rc<
    RefCell<(
        Vec<(JsValue, JsValue)>,
        Vec<bool>,
        String,
        Vec<JsValue>,
        bool,
    )>,
>;

fn zip_next_inner(interp: &mut Interpreter, state: &ZipState) -> Completion {
    let (ref iters, ref exhausted, ref mode, ref padding_values, alive) = {
        let s = state.borrow();
        (s.0.clone(), s.1.clone(), s.2.clone(), s.3.clone(), s.4)
    };
    if !alive {
        return Completion::Normal(interp.create_iter_result_object(JsValue::UNDEFINED, true));
    }
    if iters.is_empty() {
        state.borrow_mut().4 = false;
        return Completion::Normal(interp.create_iter_result_object(JsValue::UNDEFINED, true));
    }

    let mut values = Vec::with_capacity(iters.len());
    let mut new_exhausted = exhausted.clone();

    for (i, (it, nm)) in iters.iter().enumerate() {
        if exhausted[i] {
            values.push(padding_values.get(i).cloned().unwrap_or(JsValue::UNDEFINED));
            continue;
        }
        match iterator_step_value_getter(interp, it, nm) {
            Ok(Some(v)) => values.push(v),
            Ok(None) => {
                new_exhausted[i] = true;

                if mode == "shortest" {
                    state.borrow_mut().4 = false;
                    let open: Vec<(JsValue, JsValue)> = iters
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| !new_exhausted[*j])
                        .map(|(_, pair)| pair.clone())
                        .collect();
                    if let Err(e) = iterator_close_all(interp, &open, Ok(())) {
                        return Completion::Throw(e);
                    }
                    return Completion::Normal(
                        interp.create_iter_result_object(JsValue::UNDEFINED, true),
                    );
                } else if mode == "strict" {
                    if i != 0 {
                        state.borrow_mut().4 = false;
                        let open: Vec<(JsValue, JsValue)> = iters
                            .iter()
                            .enumerate()
                            .filter(|(j, _)| !new_exhausted[*j])
                            .map(|(_, pair)| pair.clone())
                            .collect();
                        let err = interp.create_type_error(
                            "Iterators passed to Iterator.zip with { mode: \"strict\" } have different lengths");
                        let _ = iterator_close_all(interp, &open, Err(err.clone()));
                        return Completion::Throw(err);
                    }
                    for k in 1..iters.len() {
                        if new_exhausted[k] {
                            continue;
                        }
                        match iterator_step_value_getter(interp, &iters[k].0, &iters[k].1) {
                            Ok(None) => {
                                new_exhausted[k] = true;
                            }
                            Ok(Some(_)) => {
                                state.borrow_mut().4 = false;
                                let open: Vec<(JsValue, JsValue)> = iters
                                    .iter()
                                    .enumerate()
                                    .filter(|(j, _)| !new_exhausted[*j])
                                    .map(|(_, pair)| pair.clone())
                                    .collect();
                                let err = interp.create_type_error(
                                    "Iterators passed to Iterator.zip with { mode: \"strict\" } have different lengths");
                                let _ = iterator_close_all(interp, &open, Err(err.clone()));
                                return Completion::Throw(err);
                            }
                            Err(e) => {
                                new_exhausted[k] = true;
                                state.borrow_mut().4 = false;
                                let open: Vec<(JsValue, JsValue)> = iters
                                    .iter()
                                    .enumerate()
                                    .filter(|(j, _)| !new_exhausted[*j])
                                    .map(|(_, pair)| pair.clone())
                                    .collect();
                                let _ = iterator_close_all(interp, &open, Err(e.clone()));
                                return Completion::Throw(e);
                            }
                        }
                    }
                    state.borrow_mut().4 = false;
                    return Completion::Normal(
                        interp.create_iter_result_object(JsValue::UNDEFINED, true),
                    );
                } else {
                    values.push(padding_values.get(i).cloned().unwrap_or(JsValue::UNDEFINED));
                }
            }
            Err(e) => {
                state.borrow_mut().4 = false;
                state.borrow_mut().1 = new_exhausted.clone();
                let open: Vec<(JsValue, JsValue)> = iters
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| !new_exhausted[*j] && *j != i)
                    .map(|(_, pair)| pair.clone())
                    .collect();
                let _ = iterator_close_all(interp, &open, Err(e.clone()));
                return Completion::Throw(e);
            }
        }
    }

    state.borrow_mut().1 = new_exhausted.clone();

    if mode == "longest" && new_exhausted.iter().all(|e| *e) {
        state.borrow_mut().4 = false;
        return Completion::Normal(interp.create_iter_result_object(JsValue::UNDEFINED, true));
    }

    let arr = interp.create_array(values);
    Completion::Normal(interp.create_iter_result_object(arr, false))
}

type ZipKeyedState = Rc<
    RefCell<(
        Vec<JsPropertyKey>,
        Vec<(JsValue, JsValue)>,
        Vec<bool>,
        String,
        Vec<JsValue>,
        bool,
    )>,
>;

fn zip_keyed_next_inner(interp: &mut Interpreter, state: &ZipKeyedState) -> Completion {
    let (ref keys, ref iters, ref exhausted, ref mode, ref padding_values, alive) = {
        let s = state.borrow();
        (
            s.0.clone(),
            s.1.clone(),
            s.2.clone(),
            s.3.clone(),
            s.4.clone(),
            s.5,
        )
    };
    if !alive {
        return Completion::Normal(interp.create_iter_result_object(JsValue::UNDEFINED, true));
    }
    if iters.is_empty() {
        state.borrow_mut().5 = false;
        return Completion::Normal(interp.create_iter_result_object(JsValue::UNDEFINED, true));
    }

    let mut values: Vec<(JsPropertyKey, JsValue)> = Vec::with_capacity(iters.len());
    let mut new_exhausted = exhausted.clone();

    for (i, (it, nm)) in iters.iter().enumerate() {
        if exhausted[i] {
            values.push((
                keys[i].clone(),
                padding_values.get(i).cloned().unwrap_or(JsValue::UNDEFINED),
            ));
            continue;
        }
        match iterator_step_value_getter(interp, it, nm) {
            Ok(Some(v)) => values.push((keys[i].clone(), v)),
            Ok(None) => {
                new_exhausted[i] = true;

                if mode == "shortest" {
                    state.borrow_mut().5 = false;
                    let open: Vec<(JsValue, JsValue)> = iters
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| !new_exhausted[*j])
                        .map(|(_, pair)| pair.clone())
                        .collect();
                    if let Err(e) = iterator_close_all(interp, &open, Ok(())) {
                        return Completion::Throw(e);
                    }
                    return Completion::Normal(
                        interp.create_iter_result_object(JsValue::UNDEFINED, true),
                    );
                } else if mode == "strict" {
                    if i != 0 {
                        state.borrow_mut().5 = false;
                        let open: Vec<(JsValue, JsValue)> = iters
                            .iter()
                            .enumerate()
                            .filter(|(j, _)| !new_exhausted[*j])
                            .map(|(_, pair)| pair.clone())
                            .collect();
                        let err = interp.create_type_error(
                            "Iterators passed to Iterator.zipKeyed with { mode: \"strict\" } have different lengths");
                        let _ = iterator_close_all(interp, &open, Err(err.clone()));
                        return Completion::Throw(err);
                    }
                    for k in 1..iters.len() {
                        if new_exhausted[k] {
                            continue;
                        }
                        match iterator_step_value_getter(interp, &iters[k].0, &iters[k].1) {
                            Ok(None) => {
                                new_exhausted[k] = true;
                            }
                            Ok(Some(_)) => {
                                state.borrow_mut().5 = false;
                                let open: Vec<(JsValue, JsValue)> = iters
                                    .iter()
                                    .enumerate()
                                    .filter(|(j, _)| !new_exhausted[*j])
                                    .map(|(_, pair)| pair.clone())
                                    .collect();
                                let err = interp.create_type_error(
                                    "Iterators passed to Iterator.zipKeyed with { mode: \"strict\" } have different lengths");
                                let _ = iterator_close_all(interp, &open, Err(err.clone()));
                                return Completion::Throw(err);
                            }
                            Err(e) => {
                                new_exhausted[k] = true;
                                state.borrow_mut().5 = false;
                                let open: Vec<(JsValue, JsValue)> = iters
                                    .iter()
                                    .enumerate()
                                    .filter(|(j, _)| !new_exhausted[*j])
                                    .map(|(_, pair)| pair.clone())
                                    .collect();
                                let _ = iterator_close_all(interp, &open, Err(e.clone()));
                                return Completion::Throw(e);
                            }
                        }
                    }
                    state.borrow_mut().5 = false;
                    return Completion::Normal(
                        interp.create_iter_result_object(JsValue::UNDEFINED, true),
                    );
                } else {
                    // longest mode: append padding value
                    values.push((
                        keys[i].clone(),
                        padding_values.get(i).cloned().unwrap_or(JsValue::UNDEFINED),
                    ));
                }
            }
            Err(e) => {
                state.borrow_mut().5 = false;
                state.borrow_mut().2 = new_exhausted.clone();
                let open: Vec<(JsValue, JsValue)> = iters
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| !new_exhausted[*j] && *j != i)
                    .map(|(_, pair)| pair.clone())
                    .collect();
                let _ = iterator_close_all(interp, &open, Err(e.clone()));
                return Completion::Throw(e);
            }
        }
    }

    state.borrow_mut().2 = new_exhausted.clone();

    // For longest mode: check if ALL are now exhausted
    if mode == "longest" && new_exhausted.iter().all(|e| *e) {
        state.borrow_mut().5 = false;
        return Completion::Normal(interp.create_iter_result_object(JsValue::UNDEFINED, true));
    }

    // Create null-prototype result object with key-value pairs
    let result_obj_id = interp.create_object_id();
    interp
        .get_object_cell_expect(result_obj_id)
        .borrow_mut()
        .prototype_id = None;
    for (key, val) in &values {
        interp
            .get_object_cell_expect(result_obj_id)
            .borrow_mut()
            .insert_property(
                key.clone(),
                PropertyDescriptor::data(val.clone(), true, true, true),
            );
    }
    let result_id = result_obj_id;
    let result_val = JsValue::object(result_id);
    Completion::Normal(interp.create_iter_result_object(result_val, false))
}

// GetIteratorDirect that uses get_object_property (invokes getters/Proxy traps)
fn get_iterator_direct_getter(
    interp: &mut Interpreter,
    obj: &JsValue,
) -> Result<(JsValue, JsValue), JsValue> {
    if let Some(obj_id) = obj.as_object_id() {
        let next_method = match interp.get_object_property(obj_id, "next", obj) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => JsValue::UNDEFINED,
        };
        Ok((obj.clone(), next_method))
    } else {
        Err(interp.create_type_error("Iterator is not an object"))
    }
}

// IteratorClose that uses get_object_property for .return (invokes getters)
fn iterator_close_getter(interp: &mut Interpreter, iterator: &JsValue) -> Result<(), JsValue> {
    if let Some(io_id) = iterator.as_object_id() {
        let return_method = match interp.get_object_property(io_id, "return", iterator) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => return Ok(()),
        };
        if return_method.is_nullish() {
            return Ok(());
        }
        match interp.call_function(&return_method, iterator, &[]) {
            Completion::Normal(inner_result) => {
                if !inner_result.is_object() {
                    return Err(interp.create_type_error("Iterator result is not an object"));
                }
                Ok(())
            }
            Completion::Throw(e) => Err(e),
            _ => Ok(()),
        }
    } else {
        Ok(())
    }
}

fn close_iterator_for_error(
    interp: &mut Interpreter,
    iterator: &JsValue,
    error: JsValue,
) -> JsValue {
    interp.gc_root_value(&error);
    let _ = iterator_close_getter(interp, iterator);
    interp.gc_unroot_value(&error);
    error
}

// GetIteratorFlattenable(obj, primitiveHandling) per spec
// primitiveHandling is either "reject-primitives" or "iterate-strings"
fn get_iterator_flattenable(
    interp: &mut Interpreter,
    obj: &JsValue,
    reject_primitives: bool,
) -> Result<(JsValue, JsValue), JsValue> {
    if !obj.is_object() {
        if reject_primitives {
            return Err(
                interp.create_type_error("Iterator.prototype.flatMap mapper returned a non-object")
            );
        }
        // iterate-strings mode: GetMethod(obj, @@iterator) on primitive
        if obj.is_string() {
            // GetMethod on primitive: ToObject for lookup, but use original as receiver
            let wrapped = match interp.to_object(obj) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => return Err(interp.create_type_error("Cannot convert to object")),
            };
            let sym_key = interp.get_symbol_iterator_key();
            let method = if let Some(wrapped_id) = wrapped.as_object_id()
                && let Some(ref key) = sym_key
            {
                match interp.get_object_property(wrapped_id, key, obj) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::UNDEFINED,
                }
            } else {
                JsValue::UNDEFINED
            };
            if method.is_nullish() {
                return Err(interp.create_type_error("String iterator method is undefined"));
            }
            // Call(method, obj) — use original primitive as this
            let iter_obj = match interp.call_function(&method, obj, &[]) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => return Err(interp.create_type_error("Iterator did not return a value")),
            };
            if !iter_obj.is_object() {
                return Err(interp.create_type_error("Result of Symbol.iterator is not an object"));
            }
            return get_iterator_direct_getter(interp, &iter_obj);
        }
        return Err(interp.create_type_error("value is not an object"));
    }

    // Get @@iterator method
    let sym_key = interp.get_symbol_iterator_key();
    let iter_method = if let Some(obj_id) = obj.as_object_id() {
        if let Some(ref key) = sym_key {
            match interp.get_object_property(obj_id, key, obj) {
                Completion::Normal(v) => Some(v),
                Completion::Throw(e) => return Err(e),
                _ => Some(JsValue::UNDEFINED),
            }
        } else {
            Some(JsValue::UNDEFINED)
        }
    } else {
        Some(JsValue::UNDEFINED)
    };

    if let Some(method) = iter_method
        && !method.is_nullish()
    {
        // Has @@iterator - check it's callable and call it
        if let Some(method_id) = method.as_object_id() {
            if !interp
                .get_object_cell(method_id)
                .map(|od| od.borrow().callable.is_some())
                .unwrap_or(false)
            {
                return Err(interp.create_type_error("Symbol.iterator is not a function"));
            }
        } else {
            return Err(interp.create_type_error("Symbol.iterator is not a function"));
        }
        let iter_obj = match interp.call_function(&method, obj, &[]) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => return Err(interp.create_type_error("Symbol.iterator did not return a value")),
        };
        if !iter_obj.is_object() {
            return Err(interp.create_type_error("Result of Symbol.iterator is not an object"));
        }
        return get_iterator_direct_getter(interp, &iter_obj);
    }

    // @@iterator is null/undefined: use obj as iterator directly
    get_iterator_direct_getter(interp, obj)
}

// GetIterator(obj, sync) using getter-aware property access for @@iterator
fn get_iterator_getter(
    interp: &mut Interpreter,
    obj: &JsValue,
) -> Result<(JsValue, JsValue), JsValue> {
    let sym_key = interp.get_symbol_iterator_key();
    let method = if let Some(obj_id) = obj.as_object_id() {
        if let Some(ref key) = sym_key {
            match interp.get_object_property(obj_id, key, obj) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::UNDEFINED,
            }
        } else {
            return Err(interp.create_type_error("is not iterable"));
        }
    } else if obj.is_string() {
        // For strings, use the string prototype's @@iterator
        return match interp.get_iterator(obj) {
            Ok(iter) => get_iterator_direct_getter(interp, &iter),
            Err(e) => Err(e),
        };
    } else {
        return Err(interp.create_type_error("is not iterable"));
    };
    if method.is_nullish() {
        return Err(interp.create_type_error("is not iterable"));
    }
    // Call the method
    let iterator = match interp.call_function(&method, obj, &[]) {
        Completion::Normal(v) => v,
        Completion::Throw(e) => return Err(e),
        _ => return Err(interp.create_type_error("Symbol.iterator did not return a value")),
    };
    if !iterator.is_object() {
        return Err(interp.create_type_error("Result of Symbol.iterator is not an object"));
    }
    get_iterator_direct_getter(interp, &iterator)
}

// IteratorStepValue using getter-aware property access for .done and .value
// Returns Ok(Some(value)) if iterator produced a value, Ok(None) if done
fn iterator_step_value_getter(
    interp: &mut Interpreter,
    iterator: &JsValue,
    next_method: &JsValue,
) -> Result<Option<JsValue>, JsValue> {
    let result = match interp.call_function(next_method, iterator, &[]) {
        Completion::Normal(v) => v,
        Completion::Throw(e) => return Err(e),
        _ => return Err(interp.create_type_error("Iterator next failed")),
    };
    let frame = interp.gc_root_frame();
    interp.gc_root_value(&result);
    let outcome = 'step: {
        let Some(result_id) = result.as_object_id() else {
            break 'step Err(interp.create_type_error("Iterator result is not an object"));
        };
        // Read .done via getter
        let done = match interp.get_object_property(result_id, "done", &result) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => break 'step Err(e),
            _ => JsValue::UNDEFINED,
        };
        if interp.to_boolean_val(&done) {
            break 'step Ok(None);
        }
        // Read .value via getter
        let value = match interp.get_object_property(result_id, "value", &result) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => break 'step Err(e),
            _ => JsValue::UNDEFINED,
        };
        Ok(Some(value))
    };
    interp.gc_unroot_frame(frame);
    outcome
}

fn iterator_join_to_string(interp: &mut Interpreter, value: &JsValue) -> Result<JsString, JsValue> {
    let primitive = if value.is_object() {
        interp.to_primitive(value, "string")?
    } else {
        value.clone()
    };
    interp.to_js_string(&primitive)
}

// IteratorClose per spec, taking a completion and returning updated completion.
// If completion is Err (throw), the original error is preserved even if .return() throws.
fn iterator_close_with_completion(
    interp: &mut Interpreter,
    iterator: &JsValue,
    completion: Result<(), JsValue>,
) -> Result<(), JsValue> {
    if let Some(io_id) = iterator.as_object_id() {
        let return_method = match interp.get_object_property(io_id, "return", iterator) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => {
                // Step 5: If completion is a throw completion, return ? completion.
                if let Err(orig) = completion {
                    return Err(orig);
                }
                return Err(e);
            }
            _ => JsValue::UNDEFINED,
        };
        if return_method.is_nullish() {
            return completion;
        }
        let inner_result = interp.call_function(&return_method, iterator, &[]);
        match inner_result {
            Completion::Normal(v) => {
                // Step 5: If completion is throw, return completion
                if let Err(e) = completion {
                    return Err(e);
                }
                // Step 7: If innerResult.[[Value]] is not an Object, throw TypeError
                if !v.is_object() {
                    return Err(interp.create_type_error("Iterator result is not an object"));
                }
                // Step 8: Return completion
                completion
            }
            Completion::Throw(e) => {
                // Step 5: If completion is throw, return original completion
                if let Err(orig) = completion {
                    return Err(orig);
                }
                // Step 6: innerResult is throw, return it
                Err(e)
            }
            _ => completion,
        }
    } else {
        completion
    }
}

// IteratorCloseAll per spec: close iterators in reverse order, accumulating errors
fn iterator_close_all(
    interp: &mut Interpreter,
    open_iters: &[(JsValue, JsValue)],
    initial_completion: Result<(), JsValue>,
) -> Result<(), JsValue> {
    let mut completion = initial_completion;
    for (iter, _) in open_iters.iter().rev() {
        completion = iterator_close_with_completion(interp, iter, completion);
    }
    completion
}

impl Interpreter {
    pub(crate) fn setup_iterator_prototypes(&mut self) {
        // %IteratorPrototype% (§27.1.2)
        let iter_proto_id = self.create_object_id();
        self.get_object_cell_expect(iter_proto_id)
            .borrow_mut()
            .class_name = "Iterator".to_string();

        // %IteratorPrototype%[@@iterator]() returns this
        let iter_self_fn = self.create_function(JsFunction::native(
            "[Symbol.iterator]".to_string(),
            0,
            |_interp, this, _args| Completion::Normal(this.clone()),
        ));
        if let Some(key) = self.get_symbol_iterator_key() {
            self.get_object_cell_expect(iter_proto_id)
                .borrow_mut()
                .insert_property(
                    key,
                    PropertyDescriptor::data(iter_self_fn, true, false, true),
                );
        }
        // @@toStringTag on %IteratorPrototype% — accessor property per spec
        {
            let tst_getter = self.create_function(JsFunction::native(
                "get [Symbol.toStringTag]".to_string(),
                0,
                |_interp, _this, _args| Completion::Normal(JsValue::from_str("Iterator")),
            ));
            let ip_id = iter_proto_id;
            let tst_key_for_setter = self.get_symbol_key("toStringTag");
            let tst_setter = self.create_function(JsFunction::native(
                "set [Symbol.toStringTag]".to_string(),
                1,
                move |interp, this, args| {
                    let v = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let Some(this_id) = this.as_object_id() else {
                        let err = interp.create_type_error("setter called on non-object");
                        return Completion::Throw(err);
                    };
                    if this_id == ip_id {
                        let err = interp.create_type_error(
                            "Cannot set Symbol.toStringTag on Iterator.prototype",
                        );
                        return Completion::Throw(err);
                    }
                    let prop_key = tst_key_for_setter
                        .clone()
                        .unwrap_or_else(|| JsPropertyKey::well_known_symbol("toStringTag"));
                    let has_own = if let Some(od) = interp.get_object_cell(this_id) {
                        od.borrow().properties.contains_key(&prop_key)
                    } else {
                        false
                    };
                    if !has_own {
                        if let Some(od) = interp.get_object_cell(this_id) {
                            let frozen = !od.borrow().extensible;
                            if frozen {
                                let err = interp.create_type_error(
                                    "Cannot define property on a non-extensible object",
                                );
                                return Completion::Throw(err);
                            }
                            od.borrow_mut().insert_property(
                                prop_key,
                                PropertyDescriptor::data(v, true, true, true),
                            );
                        }
                    } else {
                        match interp.set_object_property(this_id, &prop_key, v, this) {
                            Ok(true) => {}
                            Ok(false) => {
                                let err = interp.create_type_error("Cannot set property");
                                return Completion::Throw(err);
                            }
                            Err(err) => return Completion::Throw(err),
                        }
                    }
                    Completion::Normal(JsValue::UNDEFINED)
                },
            ));
            self.get_object_cell_expect(iter_proto_id)
                .borrow_mut()
                .insert_property(
                    JsPropertyKey::well_known_symbol("toStringTag"),
                    PropertyDescriptor {
                        value: None,
                        writable: None,
                        get: Some(tst_getter),
                        set: Some(tst_setter),
                        enumerable: Some(false),
                        configurable: Some(true),
                    },
                );
        }

        // [Symbol.dispose]() — calls this.return() if it exists
        let dispose_fn = self.create_function(JsFunction::native(
            "[Symbol.dispose]".to_string(),
            0,
            |interp, this, _args| {
                if let Some(this_id) = this.as_object_id() {
                    let return_method = interp.get_property_on_id(this_id, "return");
                    if return_method.as_object_id().is_some_and(|ro_id| {
                        interp
                            .get_object_cell(ro_id)
                            .map(|od| od.borrow().callable.is_some())
                            .unwrap_or(false)
                    }) {
                        return interp.call_function(&return_method, this, &[]);
                    }
                }
                Completion::Normal(JsValue::UNDEFINED)
            },
        ));
        if let Some(key) = self.get_symbol_key("dispose") {
            self.get_object_cell_expect(iter_proto_id)
                .borrow_mut()
                .insert_property(key, PropertyDescriptor::data(dispose_fn, true, false, true));
        }

        self.realm_mut().iterator_prototype = Some(iter_proto_id);

        // Iterator constructor (abstract — throws TypeError when called directly)
        let iterator_ctor = self.create_function(JsFunction::constructor(
            "Iterator".to_string(),
            0,
            move |interp, this, _args| {
                // §27.1.1.1: If NewTarget is undefined, throw TypeError
                // If NewTarget === Iterator, throw TypeError (abstract class)
                if interp.new_target.is_none() {
                    let err = interp.create_type_error("Iterator is not a constructor");
                    return Completion::Throw(err);
                }
                // If new_target is the Iterator constructor itself, throw TypeError
                // (abstract class cannot be instantiated directly)
                let nt_id = interp.new_target.as_ref().and_then(JsValue::as_object_id);
                if let Some(nt_id) = nt_id {
                    // Check if new.target is the Iterator constructor by checking if
                    // looking up "Iterator" from global gives the same object
                    let global_iter = interp.get_global_var("Iterator");
                    if let Some(gi_id) = global_iter.as_ref().and_then(JsValue::as_object_id)
                        && gi_id == nt_id
                    {
                        let err = interp.create_type_error(
                            "Abstract class Iterator not directly constructable",
                        );
                        return Completion::Throw(err);
                    }
                }
                // OrdinaryCreateFromConstructor — realm-aware prototype
                if let Some(this_id) = this.as_object_id()
                    && let Some(obj) = interp.get_object(this_id)
                {
                    let proto = match interp
                        .get_prototype_from_new_target_realm(|realm| realm.iterator_prototype)
                    {
                        Ok(p) => p,
                        Err(e) => return Completion::Throw(e),
                    };
                    if let Some(p) = proto {
                        obj.borrow_mut().prototype_id = Some(p);
                    }
                }
                Completion::Normal(this.clone())
            },
        ));

        // Set Iterator.prototype_id
        if let Some(ctor_id) = iterator_ctor.as_object_id()
            && let Some(obj) = self.get_object_cell(ctor_id)
        {
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(JsValue::object(iter_proto_id), false, false, false),
            );
        }

        // Set %IteratorPrototype%.constructor as accessor property per spec
        // Getter returns Iterator, setter implements SetterThatIgnoresPrototypeProperties
        {
            let ctor_val = iterator_ctor.clone();
            let getter = self.create_function(JsFunction::native(
                "get constructor".to_string(),
                0,
                move |_interp, _this, _args| Completion::Normal(ctor_val.clone()),
            ));
            let ip_id = iter_proto_id;
            let setter = self.create_function(JsFunction::native(
                "set constructor".to_string(),
                1,
                move |interp, this, args| {
                    let v = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    // Step 1: If this is not an Object, throw TypeError
                    let Some(this_id) = this.as_object_id() else {
                        let err = interp.create_type_error("setter called on non-object");
                        return Completion::Throw(err);
                    };
                    // Step 2: If this is home (Iterator.prototype), throw TypeError
                    if this_id == ip_id {
                        let err = interp
                            .create_type_error("Cannot set constructor on Iterator.prototype");
                        return Completion::Throw(err);
                    }
                    // Step 3: Check if this has own "constructor" property
                    let has_own = if let Some(od) = interp.get_object_cell(this_id) {
                        od.borrow().properties.contains_key("constructor")
                    } else {
                        false
                    };
                    if !has_own {
                        // CreateDataPropertyOrThrow(this, "constructor", v)
                        if let Some(od) = interp.get_object_cell(this_id) {
                            let frozen = !od.borrow().extensible;
                            if frozen {
                                let err = interp.create_type_error(
                                    "Cannot define property constructor on a non-extensible object",
                                );
                                return Completion::Throw(err);
                            }
                            od.borrow_mut().insert_property(
                                "constructor".to_string(),
                                PropertyDescriptor::data(v, true, true, true),
                            );
                        }
                    } else {
                        // Set(this, "constructor", v, true)
                        match interp.set_object_property(this_id, "constructor", v, this) {
                            Ok(true) => {}
                            Ok(false) => {
                                let err =
                                    interp.create_type_error("Cannot set property constructor");
                                return Completion::Throw(err);
                            }
                            Err(err) => return Completion::Throw(err),
                        }
                    }
                    Completion::Normal(JsValue::UNDEFINED)
                },
            ));
            self.get_object_cell_expect(iter_proto_id)
                .borrow_mut()
                .insert_property(
                    "constructor".to_string(),
                    PropertyDescriptor {
                        value: None,
                        writable: None,
                        get: Some(getter),
                        set: Some(setter),
                        enumerable: Some(false),
                        configurable: Some(true),
                    },
                );
        }

        // Register Iterator as global
        self.realm()
            .global_env
            .borrow_mut()
            .declare("Iterator", BindingKind::Var);
        let env = self.realm().global_env.clone();
        let _ = self.env_set(&env, "Iterator", iterator_ctor.clone());

        // Setup consuming and lazy helper methods on %IteratorPrototype%
        self.setup_iterator_helper_methods(iter_proto_id);

        // Setup static methods on Iterator constructor
        self.setup_iterator_static_methods(&iterator_ctor);

        // %ArrayIteratorPrototype% (§23.1.5.1)
        let arr_iter_proto_id = self.create_object_id();
        self.get_object_cell_expect(arr_iter_proto_id)
            .borrow_mut()
            .prototype_id = Some(iter_proto_id);
        self.get_object_cell_expect(arr_iter_proto_id)
            .borrow_mut()
            .class_name = "Array Iterator".to_string();

        self.define_method(arr_iter_proto_id, "next", 0, |interp, this, _args| {
            if let Some(this_id) = this.as_object_id() {
                if let Some(obj) = interp.get_object(this_id) {
                    let state = obj.borrow().iterator_state().cloned();
                    if let Some(IteratorState::ArrayIterator {
                        array_id,
                        index,
                        kind,
                        done,
                    }) = state
                    {
                        if done {
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::UNDEFINED, true),
                            );
                        }
                        let is_proxy = interp
                            .get_object_cell(array_id)
                            .is_some_and(|o| o.borrow().is_proxy());
                        if is_proxy {
                            let arr_val = JsValue::object(array_id);
                            let len = match interp.get_object_property(array_id, "length", &arr_val)
                            {
                                Completion::Normal(v) => v.as_number().map_or(0, |n| n as usize),
                                c => return c,
                            };
                            if index >= len {
                                if let Some(obj) = interp.get_object_cell(this_id) {
                                    obj.borrow_mut().kind =
                                        crate::interpreter::types::ObjectKind::Iterator(
                                            IteratorState::ArrayIterator {
                                                array_id,
                                                index: len,
                                                kind,
                                                done: true,
                                            },
                                        );
                                }
                                return Completion::Normal(
                                    interp.create_iter_result_object(JsValue::UNDEFINED, true),
                                );
                            }
                            let idx_str = index.to_string();
                            let v = match kind {
                                IteratorKind::Key => JsValue::number(index as f64),
                                IteratorKind::Value => {
                                    match interp.get_object_property(array_id, &idx_str, &arr_val) {
                                        Completion::Normal(v) => v,
                                        c => return c,
                                    }
                                }
                                IteratorKind::KeyValue => {
                                    let elem = match interp
                                        .get_object_property(array_id, &idx_str, &arr_val)
                                    {
                                        Completion::Normal(v) => v,
                                        c => return c,
                                    };
                                    let pair = interp
                                        .create_array(vec![JsValue::number(index as f64), elem]);
                                    if let Some(obj) = interp.get_object_cell(this_id) {
                                        obj.borrow_mut().kind =
                                            crate::interpreter::types::ObjectKind::Iterator(
                                                IteratorState::ArrayIterator {
                                                    array_id,
                                                    index: index + 1,
                                                    kind,
                                                    done: false,
                                                },
                                            );
                                    }
                                    return Completion::Normal(
                                        interp.create_iter_result_object(pair, false),
                                    );
                                }
                            };
                            if let Some(obj) = interp.get_object_cell(this_id) {
                                obj.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::ArrayIterator {
                                            array_id,
                                            index: index + 1,
                                            kind,
                                            done: false,
                                        },
                                    );
                            }
                            return Completion::Normal(interp.create_iter_result_object(v, false));
                        }
                        // §23.1.5.1.1 step 3: TypedArray OOB check
                        if let Some(arr_obj) = interp.get_object(array_id) {
                            let borrowed = arr_obj.borrow();
                            if let Some(ta) = borrowed.typed_array_info()
                                && is_typed_array_out_of_bounds(ta)
                            {
                                drop(borrowed);
                                if let Some(obj) = interp.get_object_cell(this_id) {
                                    obj.borrow_mut().kind =
                                        crate::interpreter::types::ObjectKind::Iterator(
                                            IteratorState::ArrayIterator {
                                                array_id,
                                                index,
                                                kind,
                                                done: true,
                                            },
                                        );
                                }
                                return Completion::Throw(
                                    interp.create_type_error("TypedArray is out of bounds"),
                                );
                            }
                        }
                        let (len, val) = if let Some(arr_obj) = interp.get_object(array_id) {
                            let borrowed = arr_obj.borrow();
                            let len = if let Some(ta) = borrowed.typed_array_info() {
                                typed_array_length(ta)
                            } else if let Some(n) = borrowed
                                .get_property_value("length")
                                .and_then(|v| v.as_number())
                            {
                                n as usize
                            } else if let Some(elems) = borrowed.array_elements() {
                                elems.len()
                            } else {
                                0
                            };
                            if index >= len {
                                (len, None)
                            } else {
                                let idx_str = index.to_string();
                                let has_accessor = borrowed
                                    .properties
                                    .get(&idx_str)
                                    .is_some_and(|d| d.get.is_some());
                                let is_hole =
                                    borrowed.array_elements().is_some_and(|e| index < e.len())
                                        && !borrowed.properties.contains_key(&idx_str);
                                let fast_val = if !has_accessor && !is_hole {
                                    borrowed
                                        .array_elements()
                                        .and_then(|e| e.get(index).cloned())
                                } else {
                                    None
                                };
                                drop(borrowed);
                                let arr_val = JsValue::object(array_id);
                                let v = match kind {
                                    IteratorKind::Key => JsValue::number(index as f64),
                                    IteratorKind::Value => {
                                        if let Some(fv) = fast_val {
                                            fv
                                        } else {
                                            match interp
                                                .get_object_property(array_id, &idx_str, &arr_val)
                                            {
                                                Completion::Normal(v) => v,
                                                c => return c,
                                            }
                                        }
                                    }
                                    IteratorKind::KeyValue => {
                                        let elem = if let Some(fv) = fast_val {
                                            fv
                                        } else {
                                            match interp
                                                .get_object_property(array_id, &idx_str, &arr_val)
                                            {
                                                Completion::Normal(v) => v,
                                                c => return c,
                                            }
                                        };
                                        let pair = interp.create_array(vec![
                                            JsValue::number(index as f64),
                                            elem,
                                        ]);
                                        return {
                                            obj.borrow_mut().kind =
                                                crate::interpreter::types::ObjectKind::Iterator(
                                                    IteratorState::ArrayIterator {
                                                        array_id,
                                                        index: index + 1,
                                                        kind,
                                                        done: false,
                                                    },
                                                );
                                            Completion::Normal(
                                                interp.create_iter_result_object(pair, false),
                                            )
                                        };
                                    }
                                };
                                (len, Some(v))
                            }
                        } else {
                            (0, None)
                        };
                        match val {
                            Some(v) => {
                                obj.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::ArrayIterator {
                                            array_id,
                                            index: index + 1,
                                            kind,
                                            done: false,
                                        },
                                    );
                                Completion::Normal(interp.create_iter_result_object(v, false))
                            }
                            None => {
                                obj.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::ArrayIterator {
                                            array_id,
                                            index: len,
                                            kind,
                                            done: true,
                                        },
                                    );
                                Completion::Normal(
                                    interp.create_iter_result_object(JsValue::UNDEFINED, true),
                                )
                            }
                        }
                    } else if let Some(IteratorState::TypedArrayIterator {
                        typed_array_id,
                        index,
                        kind,
                        done,
                    }) = state
                    {
                        if done {
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::UNDEFINED, true),
                            );
                        }
                        let ta_obj = interp.get_object(typed_array_id);
                        if ta_obj.is_none() {
                            obj.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                                IteratorState::TypedArrayIterator {
                                    typed_array_id,
                                    index,
                                    kind,
                                    done: true,
                                },
                            );
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::UNDEFINED, true),
                            );
                        }
                        let ta_obj = ta_obj.unwrap();
                        let ta_info = ta_obj.borrow().typed_array_info().cloned();
                        if let Some(ref ta) = ta_info {
                            if ta.is_detached.get() {
                                return Completion::Throw(
                                    interp.create_type_error("typed array is detached"),
                                );
                            }
                            if is_typed_array_out_of_bounds(ta) {
                                return Completion::Throw(
                                    interp.create_type_error("typed array is out of bounds"),
                                );
                            }
                            let len = typed_array_length(ta);
                            if index >= len {
                                obj.borrow_mut().kind =
                                    crate::interpreter::types::ObjectKind::Iterator(
                                        IteratorState::TypedArrayIterator {
                                            typed_array_id,
                                            index,
                                            kind,
                                            done: true,
                                        },
                                    );
                                return Completion::Normal(
                                    interp.create_iter_result_object(JsValue::UNDEFINED, true),
                                );
                            }
                            let v = match kind {
                                IteratorKind::Key => JsValue::number(index as f64),
                                IteratorKind::Value => typed_array_get_index(ta, index),
                                IteratorKind::KeyValue => {
                                    let elem = typed_array_get_index(ta, index);
                                    let pair = interp
                                        .create_array(vec![JsValue::number(index as f64), elem]);
                                    obj.borrow_mut().kind =
                                        crate::interpreter::types::ObjectKind::Iterator(
                                            IteratorState::TypedArrayIterator {
                                                typed_array_id,
                                                index: index + 1,
                                                kind,
                                                done: false,
                                            },
                                        );
                                    return Completion::Normal(
                                        interp.create_iter_result_object(pair, false),
                                    );
                                }
                            };
                            obj.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                                IteratorState::TypedArrayIterator {
                                    typed_array_id,
                                    index: index + 1,
                                    kind,
                                    done: false,
                                },
                            );
                            Completion::Normal(interp.create_iter_result_object(v, false))
                        } else {
                            Completion::Throw(interp.create_type_error("not a TypedArray"))
                        }
                    } else {
                        let err = interp.create_type_error("next called on non-array iterator");
                        Completion::Throw(err)
                    }
                } else {
                    Completion::Normal(JsValue::UNDEFINED)
                }
            } else {
                let err = interp.create_type_error("next called on non-object");
                Completion::Throw(err)
            }
        });

        // Set @@toStringTag
        self.define_to_string_tag(arr_iter_proto_id, "Array Iterator");

        self.realm_mut().array_iterator_prototype = Some(arr_iter_proto_id);

        // %StringIteratorPrototype% (§22.1.5.1)
        let str_iter_proto_id = self.create_object_id();
        self.get_object_cell_expect(str_iter_proto_id)
            .borrow_mut()
            .prototype_id = Some(iter_proto_id);
        self.get_object_cell_expect(str_iter_proto_id)
            .borrow_mut()
            .class_name = "String Iterator".to_string();

        self.define_method(str_iter_proto_id, "next", 0, |interp, this, _args| {
            if let Some(this_id) = this.as_object_id() {
                if let Some(obj) = interp.get_object_cell(this_id) {
                    let state = obj.borrow().iterator_state().cloned();
                    if let Some(IteratorState::StringIterator {
                        ref string,
                        position,
                        done,
                    }) = state
                    {
                        if done || position >= string.code_units.len() {
                            obj.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                                IteratorState::StringIterator {
                                    string: string.clone(),
                                    position,
                                    done: true,
                                },
                            );
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::UNDEFINED, true),
                            );
                        }
                        let cu = string.code_units[position];
                        let (result_units, advance) = if (0xD800..=0xDBFF).contains(&cu)
                            && position + 1 < string.code_units.len()
                        {
                            let next_cu = string.code_units[position + 1];
                            if (0xDC00..=0xDFFF).contains(&next_cu) {
                                (vec![cu, next_cu], 2)
                            } else {
                                (vec![cu], 1)
                            }
                        } else {
                            (vec![cu], 1)
                        };
                        obj.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                            IteratorState::StringIterator {
                                string: string.clone(),
                                position: position + advance,
                                done: false,
                            },
                        );
                        let result_js_str = JsString::from_vec(result_units);
                        Completion::Normal(
                            interp.create_iter_result_object(JsValue::string(result_js_str), false),
                        )
                    } else {
                        let err = interp.create_type_error("next called on non-string iterator");
                        Completion::Throw(err)
                    }
                } else {
                    Completion::Normal(JsValue::UNDEFINED)
                }
            } else {
                let err = interp.create_type_error("next called on non-object");
                Completion::Throw(err)
            }
        });

        self.define_to_string_tag(str_iter_proto_id, "String Iterator");

        self.realm_mut().string_iterator_prototype = Some(str_iter_proto_id);
    }

    fn ensure_iterator_helper_prototype(&mut self) {
        if self.realm().iterator_helper_prototype.is_some() {
            return;
        }
        let proto_id = self.create_object_id();
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .prototype_id = self.realm().iterator_prototype;
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .class_name = "Iterator Helper".to_string();

        // next() — reads next + gen_state from this object's IterHelperData::Helper
        self.define_method(proto_id, "next", 0, |interp, this, args| {
            let Some(this_id) = this.as_object_id() else {
                return Completion::Throw(
                    interp.create_type_error("Iterator Helper next called on non-object"),
                );
            };
            let (next_closure, gen_state) = {
                let obj = match interp.get_object(this_id) {
                    Some(o) => o,
                    None => {
                        return Completion::Throw(
                            interp
                                .create_type_error("Iterator Helper next called on invalid object"),
                        );
                    }
                };
                let b = obj.borrow();
                match b.iter_helper() {
                    Some(crate::interpreter::types::IterHelperData::Helper {
                        next,
                        gen_state,
                        ..
                    }) => (next.clone(), gen_state.clone()),
                    _ => {
                        return Completion::Throw(
                            interp.create_type_error("next method called on incompatible object"),
                        );
                    }
                }
            };
            let s = gen_state.get();
            if s == 2 {
                return Completion::Throw(interp.create_type_error("Generator is already running"));
            }
            if s == 3 {
                return Completion::Normal(
                    interp.create_iter_result_object(JsValue::UNDEFINED, true),
                );
            }
            gen_state.set(2);
            let result = interp.call_function(&next_closure, this, args);
            let is_done = if let Completion::Normal(ref v) = result {
                if let Some(v_id) = v.as_object_id() {
                    match interp.get_object_property(v_id, "done", v) {
                        Completion::Normal(d) => d.as_boolean() == Some(true),
                        _ => false,
                    }
                } else {
                    false
                }
            } else {
                false
            };
            gen_state.set(if is_done { 3 } else { 1 });
            result
        });

        // return() — reads return_closure + gen_state from this object's IterHelperData::Helper
        self.define_method(proto_id, "return", 0, |interp, this, args| {
            let Some(this_id) = this.as_object_id() else {
                return Completion::Throw(
                    interp.create_type_error("Iterator Helper return called on non-object"),
                );
            };
            let (return_closure, gen_state) = {
                let obj =
                    match interp.get_object(this_id) {
                        Some(o) => o,
                        None => {
                            return Completion::Throw(interp.create_type_error(
                                "Iterator Helper return called on invalid object",
                            ));
                        }
                    };
                let b = obj.borrow();
                match b.iter_helper() {
                    Some(crate::interpreter::types::IterHelperData::Helper {
                        return_closure,
                        gen_state,
                        ..
                    }) => (return_closure.clone(), gen_state.clone()),
                    _ => {
                        return Completion::Throw(
                            interp.create_type_error("return method called on incompatible object"),
                        );
                    }
                }
            };
            let s = gen_state.get();
            if s == 2 {
                return Completion::Throw(interp.create_type_error("Generator is already running"));
            }
            if s == 3 {
                return Completion::Normal(
                    interp.create_iter_result_object(JsValue::UNDEFINED, true),
                );
            }
            if s == 0 {
                gen_state.set(3);
                return interp.call_function(&return_closure, this, args);
            }
            gen_state.set(2);
            let result = interp.call_function(&return_closure, this, args);
            gen_state.set(3);
            result
        });

        // @@iterator returning this
        let iter_self_fn = self.create_function(JsFunction::native(
            "[Symbol.iterator]".to_string(),
            0,
            |_interp, this, _args| Completion::Normal(this.clone()),
        ));
        if let Some(key) = self.get_symbol_iterator_key() {
            self.get_object_cell_expect(proto_id)
                .borrow_mut()
                .insert_property(
                    key,
                    PropertyDescriptor::data(iter_self_fn, true, false, true),
                );
        }
        // @@toStringTag
        self.define_to_string_tag(proto_id, "Iterator Helper");

        self.realm_mut().iterator_helper_prototype = Some(proto_id);
    }

    fn create_iterator_helper_object(&mut self, next_fn: JsValue, return_fn: JsValue) -> JsValue {
        self.ensure_iterator_helper_prototype();
        let state = Rc::new(std::cell::Cell::new(0u8));

        let obj_id = self.create_object_id();
        self.get_object_cell_expect(obj_id)
            .borrow_mut()
            .prototype_id = self.realm().iterator_helper_prototype;
        self.get_object_cell_expect(obj_id).borrow_mut().class_name = "Iterator Helper".to_string();
        {
            let mut obj = self.get_object_cell_expect(obj_id).borrow_mut();
            obj.kind = crate::interpreter::types::ObjectKind::IterHelper(
                crate::interpreter::types::IterHelperData::Helper {
                    next: next_fn.clone(),
                    return_closure: return_fn.clone(),
                    gen_state: state,
                },
            );
            obj.gc_native_roots = Some(vec![next_fn, return_fn]);
        }

        let id = obj_id;
        JsValue::object(id)
    }

    /// Pin every value an iterator-helper closure captured. A thin adapter over
    /// `pin_native_root` so `gc_native_roots` has a single mutator; each helper
    /// object is fresh here, so appending matches the previous assignment.
    /// Captures that are reassigned after construction must instead live in a
    /// traced container rooted here and be mutated in place. Repeatedly pinning
    /// replacements would retain every superseded value until the helper dies.
    fn set_helper_gc_roots(&mut self, helper: &JsValue, roots: Vec<JsValue>) {
        for root in &roots {
            self.pin_native_root(helper, root);
        }
    }

    fn setup_iterator_helper_methods(&mut self, iter_proto_id: u64) {
        // toArray()
        self.define_method(iter_proto_id, "toArray", 0, |interp, this, _args| {
            let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                Ok(v) => v,
                Err(e) => return Completion::Throw(e),
            };
            let frame = interp.gc_root_frame();
            interp.gc_root_value(&iter);
            interp.gc_root_value(&next_method);
            let mut values = Vec::new();
            let outcome = loop {
                match interp.iterator_step_direct(&iter, &next_method) {
                    Ok(Some(result)) => {
                        interp.gc_root_value(&result);
                        let value = interp.iterator_value(&result);
                        interp.gc_unroot_value(&result);
                        match value {
                            Ok(v) => values.push(v),
                            Err(e) => {
                                let _ = iterator_close_getter(interp, &iter);
                                break Completion::Throw(e);
                            }
                        }
                    }
                    Ok(None) => {
                        break Completion::Normal(interp.create_array(values));
                    }
                    Err(e) => {
                        let _ = iterator_close_getter(interp, &iter);
                        break Completion::Throw(e);
                    }
                }
            };
            interp.gc_unroot_frame(frame);
            outcome
        });

        // forEach(fn)
        self.define_method(iter_proto_id, "forEach", 1, |interp, this, args| {
            let callback = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            if !callback.as_object_id().is_some_and(|id| {
                interp
                    .get_object_cell(id)
                    .map(|od| od.borrow().callable.is_some())
                    .unwrap_or(false)
            }) {
                let err = interp.create_type_error("callback is not a function");
                let err = close_iterator_for_error(interp, this, err);
                return Completion::Throw(err);
            }
            let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                Ok(v) => v,
                Err(e) => return Completion::Throw(e),
            };
            let frame = interp.gc_root_frame();
            interp.gc_root_value(&iter);
            interp.gc_root_value(&next_method);
            let mut counter = 0.0;
            let outcome = loop {
                match interp.iterator_step_direct(&iter, &next_method) {
                    Ok(Some(result)) => {
                        interp.gc_root_value(&result);
                        let value = interp.iterator_value(&result);
                        interp.gc_unroot_value(&result);
                        let value = match value {
                            Ok(v) => v,
                            Err(e) => break Completion::Throw(e),
                        };
                        if let Completion::Throw(e) = interp.call_function(
                            &callback,
                            &JsValue::UNDEFINED,
                            &[value, JsValue::number(counter)],
                        ) {
                            let _ = iterator_close_getter(interp, &iter);
                            break Completion::Throw(e);
                        }
                        counter += 1.0;
                    }
                    Ok(None) => break Completion::Normal(JsValue::UNDEFINED),
                    Err(e) => break Completion::Throw(e),
                }
            };
            interp.gc_unroot_frame(frame);
            outcome
        });

        // some(predicate)
        self.define_method(iter_proto_id, "some", 1, |interp, this, args| {
            let predicate = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            if !predicate.as_object_id().is_some_and(|id| {
                interp
                    .get_object_cell(id)
                    .map(|od| od.borrow().callable.is_some())
                    .unwrap_or(false)
            }) {
                let err = interp.create_type_error("predicate is not a function");
                let err = close_iterator_for_error(interp, this, err);
                return Completion::Throw(err);
            }
            let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                Ok(v) => v,
                Err(e) => return Completion::Throw(e),
            };
            let frame = interp.gc_root_frame();
            interp.gc_root_value(&iter);
            interp.gc_root_value(&next_method);
            let mut counter = 0.0;
            let outcome = loop {
                match interp.iterator_step_direct(&iter, &next_method) {
                    Ok(Some(result)) => {
                        interp.gc_root_value(&result);
                        let value = interp.iterator_value(&result);
                        interp.gc_unroot_value(&result);
                        let value = match value {
                            Ok(v) => v,
                            Err(e) => break Completion::Throw(e),
                        };
                        match interp.call_function(
                            &predicate,
                            &JsValue::UNDEFINED,
                            &[value, JsValue::number(counter)],
                        ) {
                            Completion::Normal(v) if interp.to_boolean_val(&v) => {
                                // Propagate IteratorClose errors
                                if let Err(e) = iterator_close_getter(interp, &iter) {
                                    break Completion::Throw(e);
                                }
                                break Completion::Normal(JsValue::TRUE);
                            }
                            Completion::Throw(e) => {
                                let _ =
                                    iterator_close_with_completion(interp, &iter, Err(e.clone()));
                                break Completion::Throw(e);
                            }
                            _ => {}
                        }
                        counter += 1.0;
                    }
                    Ok(None) => break Completion::Normal(JsValue::FALSE),
                    Err(e) => break Completion::Throw(e),
                }
            };
            interp.gc_unroot_frame(frame);
            outcome
        });

        // every(predicate)
        self.define_method(iter_proto_id, "every", 1, |interp, this, args| {
            let predicate = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            if !predicate.as_object_id().is_some_and(|id| {
                interp
                    .get_object_cell(id)
                    .map(|od| od.borrow().callable.is_some())
                    .unwrap_or(false)
            }) {
                let err = interp.create_type_error("predicate is not a function");
                let err = close_iterator_for_error(interp, this, err);
                return Completion::Throw(err);
            }
            let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                Ok(v) => v,
                Err(e) => return Completion::Throw(e),
            };
            let frame = interp.gc_root_frame();
            interp.gc_root_value(&iter);
            interp.gc_root_value(&next_method);
            let mut counter = 0.0;
            let outcome = loop {
                match interp.iterator_step_direct(&iter, &next_method) {
                    Ok(Some(result)) => {
                        interp.gc_root_value(&result);
                        let value = interp.iterator_value(&result);
                        interp.gc_unroot_value(&result);
                        let value = match value {
                            Ok(v) => v,
                            Err(e) => break Completion::Throw(e),
                        };
                        match interp.call_function(
                            &predicate,
                            &JsValue::UNDEFINED,
                            &[value, JsValue::number(counter)],
                        ) {
                            Completion::Normal(v) if !interp.to_boolean_val(&v) => {
                                if let Err(e) = iterator_close_getter(interp, &iter) {
                                    break Completion::Throw(e);
                                }
                                break Completion::Normal(JsValue::FALSE);
                            }
                            Completion::Throw(e) => {
                                let _ =
                                    iterator_close_with_completion(interp, &iter, Err(e.clone()));
                                break Completion::Throw(e);
                            }
                            _ => {}
                        }
                        counter += 1.0;
                    }
                    Ok(None) => break Completion::Normal(JsValue::TRUE),
                    Err(e) => break Completion::Throw(e),
                }
            };
            interp.gc_unroot_frame(frame);
            outcome
        });

        // find(predicate)
        self.define_method(iter_proto_id, "find", 1, |interp, this, args| {
            let predicate = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            if !predicate.as_object_id().is_some_and(|id| {
                interp
                    .get_object_cell(id)
                    .map(|od| od.borrow().callable.is_some())
                    .unwrap_or(false)
            }) {
                let err = interp.create_type_error("predicate is not a function");
                let err = close_iterator_for_error(interp, this, err);
                return Completion::Throw(err);
            }
            let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                Ok(v) => v,
                Err(e) => return Completion::Throw(e),
            };
            let frame = interp.gc_root_frame();
            interp.gc_root_value(&iter);
            interp.gc_root_value(&next_method);
            let mut counter = 0.0;
            let outcome = loop {
                match interp.iterator_step_direct(&iter, &next_method) {
                    Ok(Some(result)) => {
                        interp.gc_root_value(&result);
                        let value = interp.iterator_value(&result);
                        interp.gc_unroot_value(&result);
                        let value = match value {
                            Ok(v) => v,
                            Err(e) => break Completion::Throw(e),
                        };
                        match interp.call_function(
                            &predicate,
                            &JsValue::UNDEFINED,
                            &[value.clone(), JsValue::number(counter)],
                        ) {
                            Completion::Normal(v) if interp.to_boolean_val(&v) => {
                                if let Err(e) = iterator_close_getter(interp, &iter) {
                                    break Completion::Throw(e);
                                }
                                break Completion::Normal(value);
                            }
                            Completion::Throw(e) => {
                                let _ =
                                    iterator_close_with_completion(interp, &iter, Err(e.clone()));
                                break Completion::Throw(e);
                            }
                            _ => {}
                        }
                        counter += 1.0;
                    }
                    Ok(None) => break Completion::Normal(JsValue::UNDEFINED),
                    Err(e) => break Completion::Throw(e),
                }
            };
            interp.gc_unroot_frame(frame);
            outcome
        });

        // includes(searchElement, [skippedElements])
        self.define_method(iter_proto_id, "includes", 1, |interp, this, args| {
            if !this.is_object() {
                let err = interp
                    .create_type_error("Iterator.prototype.includes called on non-object");
                return Completion::Throw(err);
            }

            let skipped_elements = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
            let to_skip = if skipped_elements.is_undefined() {
                0.0
            } else {
                match skipped_elements.as_number() {
                    Some(value) if value.is_infinite() || value.trunc() == value => value,
                    _ => {
                        // The error object is created before IteratorClose so that a
                        // `return()` method replacing the global TypeError binding cannot
                        // change the prototype of the error we throw. It must be rooted
                        // across the close, which runs arbitrary JS and can trigger GC.
                        let err = interp.create_type_error(
                            "Iterator.prototype.includes skippedElements must be an integral Number",
                        );
                        interp.gc_root_value(&err);
                        let _ = iterator_close_getter(interp, this);
                        interp.gc_unroot_value(&err);
                        return Completion::Throw(err);
                    }
                }
            };

            if to_skip < 0.0 {
                let err = interp.create_error(
                    "RangeError",
                    "Iterator.prototype.includes skippedElements must be non-negative",
                );
                interp.gc_root_value(&err);
                let _ = iterator_close_getter(interp, this);
                interp.gc_unroot_value(&err);
                return Completion::Throw(err);
            }
            if to_skip.is_finite() && to_skip > 9007199254740991.0 {
                let err = interp.create_error(
                    "RangeError",
                    "Iterator.prototype.includes skippedElements must not exceed 2**53 - 1",
                );
                interp.gc_root_value(&err);
                let _ = iterator_close_getter(interp, this);
                interp.gc_unroot_value(&err);
                return Completion::Throw(err);
            }

            let search_element = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                Ok(value) => value,
                Err(err) => return Completion::Throw(err),
            };
            // A `next` accessor may hand back a freshly created function that no
            // heap object owns, so the cached `next_method` (and the iterator
            // itself) must stay rooted for the whole loop: user code reached from
            // `next()` or a `value` getter can trigger a major GC mid-iteration.
            let frame = interp.gc_root_frame();
            interp.gc_root_value(&iter);
            interp.gc_root_value(&next_method);
            let mut skipped = 0.0;
            let outcome = loop {
                match interp.iterator_step_direct(&iter, &next_method) {
                    Ok(Some(result)) => {
                        // `result` is likewise only held here while its own
                        // `value` getter runs.
                        interp.gc_root_value(&result);
                        let value = interp.iterator_value(&result);
                        interp.gc_unroot_value(&result);
                        let value = match value {
                            Ok(value) => value,
                            Err(err) => break Completion::Throw(err),
                        };
                        if skipped < to_skip {
                            skipped += 1.0;
                            continue;
                        }
                        if same_value_zero(&value, &search_element) {
                            if let Err(err) = iterator_close_getter(interp, &iter) {
                                break Completion::Throw(err);
                            }
                            break Completion::Normal(JsValue::TRUE);
                        }
                    }
                    Ok(None) => break Completion::Normal(JsValue::FALSE),
                    Err(err) => break Completion::Throw(err),
                }
            };
            interp.gc_unroot_frame(frame);
            outcome
        });

        // reduce(reducer, [initial])
        self.define_method(iter_proto_id, "reduce", 1, |interp, this, args| {
            let reducer = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            if !reducer.as_object_id().is_some_and(|id| {
                interp
                    .get_object_cell(id)
                    .map(|od| od.borrow().callable.is_some())
                    .unwrap_or(false)
            }) {
                let err = interp.create_type_error("reducer is not a function");
                let err = close_iterator_for_error(interp, this, err);
                return Completion::Throw(err);
            }
            let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                Ok(v) => v,
                Err(e) => return Completion::Throw(e),
            };
            let frame = interp.gc_root_frame();
            interp.gc_root_value(&iter);
            interp.gc_root_value(&next_method);
            let outcome = 'reduce: {
                let mut accumulator;
                let mut counter;
                if args.len() >= 2 {
                    accumulator = args[1].clone();
                    counter = 0.0;
                } else {
                    match interp.iterator_step_direct(&iter, &next_method) {
                        Ok(Some(result)) => {
                            interp.gc_root_value(&result);
                            let value = interp.iterator_value(&result);
                            interp.gc_unroot_value(&result);
                            accumulator = match value {
                                Ok(v) => v,
                                Err(e) => break 'reduce Completion::Throw(e),
                            };
                            counter = 1.0;
                        }
                        Ok(None) => {
                            let err = interp.create_type_error(
                                "Reduce of empty iterator with no initial value",
                            );
                            break 'reduce Completion::Throw(err);
                        }
                        Err(e) => break 'reduce Completion::Throw(e),
                    }
                }
                loop {
                    match interp.iterator_step_direct(&iter, &next_method) {
                        Ok(Some(result)) => {
                            interp.gc_root_value(&result);
                            let value = interp.iterator_value(&result);
                            interp.gc_unroot_value(&result);
                            let value = match value {
                                Ok(v) => v,
                                Err(e) => break 'reduce Completion::Throw(e),
                            };
                            match interp.call_function(
                                &reducer,
                                &JsValue::UNDEFINED,
                                &[accumulator.clone(), value, JsValue::number(counter)],
                            ) {
                                Completion::Normal(v) => accumulator = v,
                                Completion::Throw(e) => {
                                    let _ = iterator_close_getter(interp, &iter);
                                    break 'reduce Completion::Throw(e);
                                }
                                _ => {}
                            }
                            counter += 1.0;
                        }
                        Ok(None) => break 'reduce Completion::Normal(accumulator),
                        Err(e) => break 'reduce Completion::Throw(e),
                    }
                }
            };
            interp.gc_unroot_frame(frame);
            outcome
        });

        // join(separator)
        self.define_method(iter_proto_id, "join", 1, |interp, this, args| {
            if !this.is_object() {
                return Completion::Throw(
                    interp.create_type_error("Iterator.prototype.join called on non-object"),
                );
            }

            let separator = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            let separator = if separator.is_undefined() {
                JsString::from_str(",")
            } else {
                match iterator_join_to_string(interp, &separator) {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = iterator_close_with_completion(interp, this, Err(error.clone()));
                        return Completion::Throw(error);
                    }
                }
            };

            let (iterator, next_method) = match get_iterator_direct_getter(interp, this) {
                Ok(record) => record,
                Err(error) => return Completion::Throw(error),
            };
            let frame = interp.gc_root_frame();
            interp.gc_root_value(&iterator);
            interp.gc_root_value(&next_method);
            let mut result = Vec::new();
            let mut first = true;

            let outcome = loop {
                let value = match iterator_step_value_getter(interp, &iterator, &next_method) {
                    Ok(Some(value)) => value,
                    Ok(None) => {
                        break Completion::Normal(JsValue::string(JsString::from_vec(result)));
                    }
                    Err(error) => break Completion::Throw(error),
                };

                if first {
                    first = false;
                } else {
                    result.extend(separator.code_units.iter().copied());
                }

                if !value.is_nullish() {
                    match iterator_join_to_string(interp, &value) {
                        Ok(value_string) => {
                            result.extend(value_string.code_units.iter().copied());
                        }
                        Err(error) => {
                            let _ = iterator_close_with_completion(
                                interp,
                                &iterator,
                                Err(error.clone()),
                            );
                            break Completion::Throw(error);
                        }
                    }
                }
            };
            interp.gc_unroot_frame(frame);
            outcome
        });

        // Lazy helpers: map, filter, take, drop, flatMap
        self.setup_iterator_lazy_helpers(iter_proto_id);
    }

    fn setup_iterator_lazy_helpers(&mut self, iter_proto_id: u64) {
        // map(mapper)
        self.define_method(
            iter_proto_id,
            "map",
            1,
            |interp, this, args| {
                // Step 1-2: Require this to be an object
                if !this.is_object() {
                    let err = interp.create_type_error("Iterator.prototype.map called on non-object");
                    return Completion::Throw(err);
                }
                let mapper = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if !mapper.as_object_id().is_some_and(|id| {
                    interp
                        .get_object_cell(id)
                        .map(|od| od.borrow().callable.is_some())
                        .unwrap_or(false)
                }) {
                    let err = interp.create_type_error("mapper is not a function");
                    let err = close_iterator_for_error(interp, this, err);
                    return Completion::Throw(err);
                }
                let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // state: (iter, next_method, mapper, counter, alive, running)
                #[allow(clippy::type_complexity)]
                let state: Rc<RefCell<(JsValue, JsValue, JsValue, f64, bool, bool)>> =
                    Rc::new(RefCell::new((iter, next_method, mapper, 0.0, true, false)));

                let state_next = state.clone();
                let next_fn = interp.create_function(JsFunction::native(
                    "next".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (iter, next_method, mapper, counter, alive, running) = {
                            let s = state_next.borrow();
                            (s.0.clone(), s.1.clone(), s.2.clone(), s.3, s.4, s.5)
                        };
                        if !alive {
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::UNDEFINED, true),
                            );
                        }
                        if running {
                            let err = interp.create_type_error("Iterator helper method called while iterator is already being iterated");
                            return Completion::Throw(err);
                        }
                        state_next.borrow_mut().5 = true;
                        let result = (|| {
                            match interp.iterator_step_direct(&iter, &next_method) {
                                Ok(Some(result)) => {
                                    let value = match interp.iterator_value(&result) {
                                        Ok(v) => v,
                                        Err(e) => return Completion::Throw(e),
                                    };
                                    let mapped = interp.call_function(
                                        &mapper,
                                        &JsValue::UNDEFINED,
                                        &[value, JsValue::number(counter)],
                                    );
                                    state_next.borrow_mut().3 = counter + 1.0;
                                    match mapped {
                                        Completion::Normal(v) => Completion::Normal(
                                            interp.create_iter_result_object(v, false),
                                        ),
                                        Completion::Throw(e) => {
                                            state_next.borrow_mut().4 = false;
                                            let _ = iterator_close_getter(interp, &iter);
                                            Completion::Throw(e)
                                        }
                                        _ => Completion::Normal(
                                            interp.create_iter_result_object(JsValue::UNDEFINED, true),
                                        ),
                                    }
                                }
                                Ok(None) => {
                                    state_next.borrow_mut().4 = false;
                                    Completion::Normal(
                                        interp.create_iter_result_object(JsValue::UNDEFINED, true),
                                    )
                                }
                                Err(e) => {
                                    state_next.borrow_mut().4 = false;
                                    Completion::Throw(e)
                                }
                            }
                        })();
                        state_next.borrow_mut().5 = false;
                        result
                    },
                ));

                let state_ret = state.clone();
                let return_fn = interp.create_function(JsFunction::native(
                    "return".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (iter, alive, running) = {
                            let s = state_ret.borrow();
                            (s.0.clone(), s.4, s.5)
                        };
                        if running {
                            let err = interp.create_type_error("Iterator helper method called while iterator is already being iterated");
                            return Completion::Throw(err);
                        }
                        state_ret.borrow_mut().5 = true;
                        state_ret.borrow_mut().4 = false;
                        let result = if alive
                            && let Err(e) = iterator_close_getter(interp, &iter) {
                                Completion::Throw(e)
                            } else {
                                Completion::Normal(
                                    interp.create_iter_result_object(JsValue::UNDEFINED, true),
                                )
                            };
                        state_ret.borrow_mut().5 = false;
                        result
                    },
                ));

                let helper = interp.create_iterator_helper_object(next_fn, return_fn);
                {
                    let b = state.borrow();
                    interp.set_helper_gc_roots(&helper, vec![b.0.clone(), b.1.clone(), b.2.clone()]);
                }
                Completion::Normal(helper)
            },
        );

        // filter(predicate)
        self.define_method(
            iter_proto_id,
            "filter",
            1,
            |interp, this, args| {
                if !this.is_object() {
                    let err = interp.create_type_error("Iterator.prototype.filter called on non-object");
                    return Completion::Throw(err);
                }
                let predicate = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if !predicate.as_object_id().is_some_and(|id| {
                    interp
                        .get_object_cell(id)
                        .map(|od| od.borrow().callable.is_some())
                        .unwrap_or(false)
                }) {
                    let err = interp.create_type_error("predicate is not a function");
                    let err = close_iterator_for_error(interp, this, err);
                    return Completion::Throw(err);
                }
                let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // state: (iter, next_method, predicate, counter, alive, running)
                #[allow(clippy::type_complexity)]
                let state: Rc<RefCell<(JsValue, JsValue, JsValue, f64, bool, bool)>> =
                    Rc::new(RefCell::new((iter, next_method, predicate, 0.0, true, false)));

                let state_next = state.clone();
                let next_fn = interp.create_function(JsFunction::native(
                    "next".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (iter, next_method, pred, mut counter, alive, running) = {
                            let s = state_next.borrow();
                            (s.0.clone(), s.1.clone(), s.2.clone(), s.3, s.4, s.5)
                        };
                        if !alive {
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::UNDEFINED, true),
                            );
                        }
                        if running {
                            let err = interp.create_type_error("Iterator helper method called while iterator is already being iterated");
                            return Completion::Throw(err);
                        }
                        state_next.borrow_mut().5 = true;
                        let result = (|| {
                            loop {
                                match interp.iterator_step_direct(&iter, &next_method) {
                                    Ok(Some(result)) => {
                                        let value = match interp.iterator_value(&result) {
                                        Ok(v) => v,
                                        Err(e) => return Completion::Throw(e),
                                    };
                                        let test_result = interp.call_function(
                                            &pred,
                                            &JsValue::UNDEFINED,
                                            &[value.clone(), JsValue::number(counter)],
                                        );
                                        counter += 1.0;
                                        state_next.borrow_mut().3 = counter;
                                        match test_result {
                                            Completion::Normal(v)
                                                if interp.to_boolean_val(&v) => {
                                                    return Completion::Normal(
                                                        interp.create_iter_result_object(value, false),
                                                    );
                                                }
                                            Completion::Throw(e) => {
                                                state_next.borrow_mut().4 = false;
                                                let _ = iterator_close_getter(interp, &iter);
                                                return Completion::Throw(e);
                                            }
                                            _ => {}
                                        }
                                    }
                                    Ok(None) => {
                                        state_next.borrow_mut().4 = false;
                                        return Completion::Normal(
                                            interp.create_iter_result_object(JsValue::UNDEFINED, true),
                                        );
                                    }
                                    Err(e) => {
                                        state_next.borrow_mut().4 = false;
                                        return Completion::Throw(e);
                                    }
                                }
                            }
                        })();
                        state_next.borrow_mut().5 = false;
                        result
                    },
                ));

                let state_ret = state.clone();
                let return_fn = interp.create_function(JsFunction::native(
                    "return".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (iter, alive, running) = {
                            let s = state_ret.borrow();
                            (s.0.clone(), s.4, s.5)
                        };
                        if running {
                            let err = interp.create_type_error("Iterator helper method called while iterator is already being iterated");
                            return Completion::Throw(err);
                        }
                        state_ret.borrow_mut().5 = true;
                        state_ret.borrow_mut().4 = false;
                        let result = if alive {
                            if let Err(e) = iterator_close_getter(interp, &iter) {
                                Completion::Throw(e)
                            } else {
                                Completion::Normal(
                                    interp.create_iter_result_object(JsValue::UNDEFINED, true),
                                )
                            }
                        } else {
                            Completion::Normal(
                                interp.create_iter_result_object(JsValue::UNDEFINED, true),
                            )
                        };
                        state_ret.borrow_mut().5 = false;
                        result
                    },
                ));

                let helper = interp.create_iterator_helper_object(next_fn, return_fn);
                {
                    let b = state.borrow();
                    interp.set_helper_gc_roots(&helper, vec![b.0.clone(), b.1.clone(), b.2.clone()]);
                }
                Completion::Normal(helper)
            },
        );

        // take(limit)
        self.define_method(
            iter_proto_id,
            "take",
            1,
            |interp, this, args| {
                // Step 2: If this is not an Object, throw TypeError
                if !this.is_object() {
                    let err =
                        interp.create_type_error("Iterator.prototype.take called on non-object");
                    return Completion::Throw(err);
                }
                // Step 3: numLimit = ToNumber(limit) — can throw via valueOf
                let limit_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let num_limit = match interp.to_number_value(&limit_val) {
                    Ok(n) => n,
                    Err(e) => {
                        // Close underlying iterator before propagating error
                        let _ = iterator_close_getter(interp, this);
                        return Completion::Throw(e);
                    }
                };
                // Step 4: If numLimit is NaN, throw RangeError
                if num_limit.is_nan() {
                    let err = interp
                        .create_error("RangeError", "take limit must be a non-negative number");
                    let err = close_iterator_for_error(interp, this, err);
                    return Completion::Throw(err);
                }
                // Step 5: If numLimit is finite and numLimit > 2**53 - 1, throw RangeError
                if num_limit.is_finite() && num_limit > 9007199254740991.0 {
                    let err = interp.create_error(
                        "RangeError",
                        "take limit must not exceed 2**53 - 1",
                    );
                    let err = close_iterator_for_error(interp, this, err);
                    return Completion::Throw(err);
                }
                // Step 6-7: integerLimit = ToIntegerOrInfinity, check < 0
                let integer_limit = if num_limit.is_infinite() {
                    num_limit
                } else {
                    num_limit.trunc()
                };
                if integer_limit < 0.0 {
                    let err = interp
                        .create_error("RangeError", "take limit must be a non-negative number");
                    let err = close_iterator_for_error(interp, this, err);
                    return Completion::Throw(err);
                }
                // Step 7: GetIteratorDirect
                let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // state: (iter, next_method, remaining, alive, running)
                #[allow(clippy::type_complexity)]
                let state: Rc<RefCell<(JsValue, JsValue, f64, bool, bool)>> =
                    Rc::new(RefCell::new((iter, next_method, integer_limit, true, false)));

                let state_next = state.clone();
                let next_fn = interp.create_function(JsFunction::native(
                    "next".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (iter, next_method, remaining, alive, running) = {
                            let s = state_next.borrow();
                            (s.0.clone(), s.1.clone(), s.2, s.3, s.4)
                        };
                        if !alive {
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::UNDEFINED, true),
                            );
                        }
                        if running {
                            let err = interp.create_type_error("Iterator helper method called while iterator is already being iterated");
                            return Completion::Throw(err);
                        }
                        state_next.borrow_mut().4 = true;
                        let result = (|| {
                            // Per spec: check remaining FIRST, close on the call AFTER exhaustion
                            if remaining <= 0.0 {
                                state_next.borrow_mut().3 = false;
                                if let Err(e) = iterator_close_getter(interp, &iter) {
                                    return Completion::Throw(e);
                                }
                                return Completion::Normal(
                                    interp.create_iter_result_object(JsValue::UNDEFINED, true),
                                );
                            }
                            // Decrement remaining
                            state_next.borrow_mut().2 = remaining - 1.0;
                            match interp.iterator_step_direct(&iter, &next_method) {
                                Ok(Some(result)) => {
                                    let value = match interp.iterator_value(&result) {
                                        Ok(v) => v,
                                        Err(e) => return Completion::Throw(e),
                                    };
                                    // Don't close here — close on NEXT call when remaining hits 0
                                    Completion::Normal(interp.create_iter_result_object(value, false))
                                }
                                Ok(None) => {
                                    state_next.borrow_mut().3 = false;
                                    Completion::Normal(
                                        interp.create_iter_result_object(JsValue::UNDEFINED, true),
                                    )
                                }
                                Err(e) => {
                                    state_next.borrow_mut().3 = false;
                                    Completion::Throw(e)
                                }
                            }
                        })();
                        state_next.borrow_mut().4 = false;
                        result
                    },
                ));

                let state_ret = state.clone();
                let return_fn = interp.create_function(JsFunction::native(
                    "return".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (iter, alive, running) = {
                            let s = state_ret.borrow();
                            (s.0.clone(), s.3, s.4)
                        };
                        if running {
                            let err = interp.create_type_error("Iterator helper method called while iterator is already being iterated");
                            return Completion::Throw(err);
                        }
                        state_ret.borrow_mut().4 = true;
                        state_ret.borrow_mut().3 = false;
                        let result = if alive {
                            if let Err(e) = iterator_close_getter(interp, &iter) {
                                Completion::Throw(e)
                            } else {
                                Completion::Normal(
                                    interp.create_iter_result_object(JsValue::UNDEFINED, true),
                                )
                            }
                        } else {
                            Completion::Normal(
                                interp.create_iter_result_object(JsValue::UNDEFINED, true),
                            )
                        };
                        state_ret.borrow_mut().4 = false;
                        result
                    },
                ));

                let helper = interp.create_iterator_helper_object(next_fn, return_fn);
                {
                    let b = state.borrow();
                    interp.set_helper_gc_roots(&helper, vec![b.0.clone(), b.1.clone()]);
                }
                Completion::Normal(helper)
            },
        );

        // drop(limit)
        self.define_method(
            iter_proto_id,
            "drop",
            1,
            |interp, this, args| {
                // Step 2: If this is not an Object, throw TypeError
                if !this.is_object() {
                    let err =
                        interp.create_type_error("Iterator.prototype.drop called on non-object");
                    return Completion::Throw(err);
                }
                // Step 3: numLimit = ToNumber(limit)
                let limit_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let num_limit = match interp.to_number_value(&limit_val) {
                    Ok(n) => n,
                    Err(e) => {
                        let _ = iterator_close_getter(interp, this);
                        return Completion::Throw(e);
                    }
                };
                // Step 4: If numLimit is NaN, throw RangeError
                if num_limit.is_nan() {
                    let err = interp
                        .create_error("RangeError", "drop limit must be a non-negative number");
                    let err = close_iterator_for_error(interp, this, err);
                    return Completion::Throw(err);
                }
                // Step 5: If numLimit is finite and numLimit > 2**53 - 1, throw RangeError
                if num_limit.is_finite() && num_limit > 9007199254740991.0 {
                    let err = interp.create_error(
                        "RangeError",
                        "drop limit must not exceed 2**53 - 1",
                    );
                    let err = close_iterator_for_error(interp, this, err);
                    return Completion::Throw(err);
                }
                // Step 6-7: integerLimit = ToIntegerOrInfinity, check < 0
                let integer_limit = if num_limit.is_infinite() {
                    num_limit
                } else {
                    num_limit.trunc()
                };
                if integer_limit < 0.0 {
                    let err = interp
                        .create_error("RangeError", "drop limit must be a non-negative number");
                    let err = close_iterator_for_error(interp, this, err);
                    return Completion::Throw(err);
                }
                // Step 7: GetIteratorDirect
                let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // state: (iter, next_method, to_skip, skipped, alive, running)
                #[allow(clippy::type_complexity)]
                let state: Rc<RefCell<(JsValue, JsValue, f64, bool, bool, bool)>> = Rc::new(
                    RefCell::new((iter, next_method, integer_limit, false, true, false)),
                );

                let state_next = state.clone();
                let next_fn = interp.create_function(JsFunction::native(
                    "next".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (iter, next_method, to_skip, skipped, alive, running) = {
                            let s = state_next.borrow();
                            (s.0.clone(), s.1.clone(), s.2, s.3, s.4, s.5)
                        };
                        if !alive {
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::UNDEFINED, true),
                            );
                        }
                        if running {
                            let err = interp.create_type_error("Iterator helper method called while iterator is already being iterated");
                            return Completion::Throw(err);
                        }
                        state_next.borrow_mut().5 = true;
                        let result = (|| {
                            if !skipped {
                                let mut remaining = to_skip;
                                while remaining > 0.0 {
                                    match interp.iterator_step_direct(&iter, &next_method) {
                                        Ok(Some(_)) => {
                                            remaining -= 1.0;
                                        }
                                        Ok(None) => {
                                            state_next.borrow_mut().4 = false;
                                            return Completion::Normal(
                                                interp.create_iter_result_object(
                                                    JsValue::UNDEFINED,
                                                    true,
                                                ),
                                            );
                                        }
                                        Err(e) => {
                                            state_next.borrow_mut().4 = false;
                                            return Completion::Throw(e);
                                        }
                                    }
                                }
                                state_next.borrow_mut().3 = true;
                            }
                            match interp.iterator_step_direct(&iter, &next_method) {
                                Ok(Some(result)) => {
                                    let value = match interp.iterator_value(&result) {
                                        Ok(v) => v,
                                        Err(e) => return Completion::Throw(e),
                                    };
                                    Completion::Normal(interp.create_iter_result_object(value, false))
                                }
                                Ok(None) => {
                                    state_next.borrow_mut().4 = false;
                                    Completion::Normal(
                                        interp.create_iter_result_object(JsValue::UNDEFINED, true),
                                    )
                                }
                                Err(e) => {
                                    state_next.borrow_mut().4 = false;
                                    Completion::Throw(e)
                                }
                            }
                        })();
                        state_next.borrow_mut().5 = false;
                        result
                    },
                ));

                let state_ret = state.clone();
                let return_fn = interp.create_function(JsFunction::native(
                    "return".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (iter, alive, running) = {
                            let s = state_ret.borrow();
                            (s.0.clone(), s.4, s.5)
                        };
                        if running {
                            let err = interp.create_type_error("Iterator helper method called while iterator is already being iterated");
                            return Completion::Throw(err);
                        }
                        state_ret.borrow_mut().5 = true;
                        state_ret.borrow_mut().4 = false;
                        let result = if alive {
                            if let Err(e) = iterator_close_getter(interp, &iter) {
                                Completion::Throw(e)
                            } else {
                                Completion::Normal(
                                    interp.create_iter_result_object(JsValue::UNDEFINED, true),
                                )
                            }
                        } else {
                            Completion::Normal(
                                interp.create_iter_result_object(JsValue::UNDEFINED, true),
                            )
                        };
                        state_ret.borrow_mut().5 = false;
                        result
                    },
                ));

                let helper = interp.create_iterator_helper_object(next_fn, return_fn);
                {
                    let b = state.borrow();
                    interp.set_helper_gc_roots(&helper, vec![b.0.clone(), b.1.clone()]);
                }
                Completion::Normal(helper)
            },
        );

        // chunks(chunkSize)
        self.define_method(iter_proto_id, "chunks", 1, |interp, this, args| {
            if !this.is_object() {
                return Completion::Throw(
                    interp.create_type_error("Iterator.prototype.chunks called on non-object"),
                );
            }

            let chunk_size = args
                .first()
                .and_then(JsValue::as_number)
                .filter(|size| size.is_finite() && size.fract() == 0.0);
            let Some(chunk_size) = chunk_size else {
                let err = interp.create_type_error("chunkSize must be an integral Number");
                let _ = iterator_close_with_completion(interp, this, Err(err.clone()));
                return Completion::Throw(err);
            };
            if !(1.0..=u32::MAX as f64).contains(&chunk_size) {
                let err =
                    interp.create_error("RangeError", "chunkSize must be between 1 and 2**32 - 1");
                let _ = iterator_close_with_completion(interp, this, Err(err.clone()));
                return Completion::Throw(err);
            }

            let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                Ok(v) => v,
                Err(e) => return Completion::Throw(e),
            };

            let chunk_size = chunk_size as usize;
            let buffer = interp.create_array(vec![]);
            // state: (iter, next_method, rooted buffer, alive)
            #[allow(clippy::type_complexity)]
            let state: Rc<RefCell<(JsValue, JsValue, JsValue, bool)>> =
                Rc::new(RefCell::new((iter, next_method, buffer, true)));

            let state_next = state.clone();
            let next_fn = interp.create_function(JsFunction::native(
                "next".to_string(),
                0,
                move |interp, _this, _args| {
                    let (iter, next_method, buffer, alive) = {
                        let s = state_next.borrow();
                        (s.0.clone(), s.1.clone(), s.2.clone(), s.3)
                    };
                    if !alive {
                        return Completion::Normal(
                            interp.create_iter_result_object(JsValue::UNDEFINED, true),
                        );
                    }

                    loop {
                        match iterator_step_value_getter(interp, &iter, &next_method) {
                            Ok(Some(value)) => {
                                let buffer_id = buffer
                                    .as_object_id()
                                    .expect("chunks buffer must be an Array object");
                                let values = {
                                    let cell = interp.get_object_cell_expect(buffer_id);
                                    let mut obj = cell.borrow_mut();
                                    let elements = obj
                                        .array_elements_mut()
                                        .expect("chunks buffer must have Array elements");
                                    elements.push(value);
                                    (elements.len() == chunk_size).then(|| elements.clone())
                                };
                                if let Some(values) = values {
                                    interp
                                        .get_object_cell_expect(buffer_id)
                                        .borrow_mut()
                                        .array_elements_mut()
                                        .expect("chunks buffer must have Array elements")
                                        .clear();
                                    let chunk = interp.create_array(values);
                                    return Completion::Normal(
                                        interp.create_iter_result_object(chunk, false),
                                    );
                                }
                            }
                            Ok(None) => {
                                state_next.borrow_mut().3 = false;
                                let buffer_id = buffer
                                    .as_object_id()
                                    .expect("chunks buffer must be an Array object");
                                let values = {
                                    let cell = interp.get_object_cell_expect(buffer_id);
                                    cell.borrow()
                                        .array_elements()
                                        .expect("chunks buffer must have Array elements")
                                        .clone()
                                };
                                if values.is_empty() {
                                    return Completion::Normal(
                                        interp.create_iter_result_object(JsValue::UNDEFINED, true),
                                    );
                                }
                                interp
                                    .get_object_cell_expect(buffer_id)
                                    .borrow_mut()
                                    .array_elements_mut()
                                    .expect("chunks buffer must have Array elements")
                                    .clear();
                                let chunk = interp.create_array(values);
                                return Completion::Normal(
                                    interp.create_iter_result_object(chunk, false),
                                );
                            }
                            Err(e) => {
                                state_next.borrow_mut().3 = false;
                                return Completion::Throw(e);
                            }
                        }
                    }
                },
            ));

            let state_ret = state.clone();
            let return_fn = interp.create_function(JsFunction::native(
                "return".to_string(),
                0,
                move |interp, _this, _args| {
                    let (iter, alive) = {
                        let s = state_ret.borrow();
                        (s.0.clone(), s.3)
                    };
                    state_ret.borrow_mut().3 = false;
                    if alive && let Err(e) = iterator_close_getter(interp, &iter) {
                        return Completion::Throw(e);
                    }
                    Completion::Normal(interp.create_iter_result_object(JsValue::UNDEFINED, true))
                },
            ));

            let helper = interp.create_iterator_helper_object(next_fn, return_fn);
            {
                let s = state.borrow();
                interp.set_helper_gc_roots(&helper, vec![s.0.clone(), s.1.clone(), s.2.clone()]);
            }
            Completion::Normal(helper)
        });

        // windows(windowSize [, undersized])
        self.define_method(iter_proto_id, "windows", 1, |interp, this, args| {
            if !this.is_object() {
                return Completion::Throw(
                    interp.create_type_error("Iterator.prototype.windows called on non-object"),
                );
            }

            let window_size = args
                .first()
                .and_then(JsValue::as_number)
                .filter(|size| size.is_finite() && size.fract() == 0.0);
            let Some(window_size) = window_size else {
                let err = interp.create_type_error("windowSize must be an integral Number");
                let _ = iterator_close_with_completion(interp, this, Err(err.clone()));
                return Completion::Throw(err);
            };
            if !(1.0..=u32::MAX as f64).contains(&window_size) {
                let err =
                    interp.create_error("RangeError", "windowSize must be between 1 and 2**32 - 1");
                let _ = iterator_close_with_completion(interp, this, Err(err.clone()));
                return Completion::Throw(err);
            }

            let undersized = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
            let allow_partial = if undersized.is_undefined() {
                false
            } else {
                match undersized.as_string().map(|s| s.to_rust_string()) {
                    Some(mode) if mode == "only-full" => false,
                    Some(mode) if mode == "allow-partial" => true,
                    _ => {
                        let err = interp
                            .create_type_error("undersized must be 'only-full' or 'allow-partial'");
                        let _ = iterator_close_with_completion(interp, this, Err(err.clone()));
                        return Completion::Throw(err);
                    }
                }
            };

            let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                Ok(v) => v,
                Err(e) => return Completion::Throw(e),
            };

            let window_size = window_size as usize;
            let buffer = interp.create_array(vec![]);
            // state: (iter, next_method, rooted buffer, alive)
            #[allow(clippy::type_complexity)]
            let state: Rc<RefCell<(JsValue, JsValue, JsValue, bool)>> =
                Rc::new(RefCell::new((iter, next_method, buffer, true)));

            let state_next = state.clone();
            let next_fn = interp.create_function(JsFunction::native(
                "next".to_string(),
                0,
                move |interp, _this, _args| {
                    let (iter, next_method, buffer, alive) = {
                        let s = state_next.borrow();
                        (s.0.clone(), s.1.clone(), s.2.clone(), s.3)
                    };
                    if !alive {
                        return Completion::Normal(
                            interp.create_iter_result_object(JsValue::UNDEFINED, true),
                        );
                    }

                    loop {
                        match iterator_step_value_getter(interp, &iter, &next_method) {
                            Ok(Some(value)) => {
                                let buffer_id = buffer
                                    .as_object_id()
                                    .expect("windows buffer must be an Array object");
                                let values = {
                                    let cell = interp.get_object_cell_expect(buffer_id);
                                    let mut obj = cell.borrow_mut();
                                    let elements = obj
                                        .array_elements_mut()
                                        .expect("windows buffer must have Array elements");
                                    if elements.len() == window_size {
                                        elements.remove(0);
                                    }
                                    elements.push(value);
                                    (elements.len() == window_size).then(|| elements.clone())
                                };
                                if let Some(values) = values {
                                    let window = interp.create_array(values);
                                    return Completion::Normal(
                                        interp.create_iter_result_object(window, false),
                                    );
                                }
                            }
                            Ok(None) => {
                                state_next.borrow_mut().3 = false;
                                let buffer_id = buffer
                                    .as_object_id()
                                    .expect("windows buffer must be an Array object");
                                let values = {
                                    let cell = interp.get_object_cell_expect(buffer_id);
                                    cell.borrow()
                                        .array_elements()
                                        .expect("windows buffer must have Array elements")
                                        .clone()
                                };
                                if allow_partial && !values.is_empty() && values.len() < window_size
                                {
                                    let window = interp.create_array(values);
                                    return Completion::Normal(
                                        interp.create_iter_result_object(window, false),
                                    );
                                }
                                return Completion::Normal(
                                    interp.create_iter_result_object(JsValue::UNDEFINED, true),
                                );
                            }
                            Err(e) => {
                                state_next.borrow_mut().3 = false;
                                return Completion::Throw(e);
                            }
                        }
                    }
                },
            ));

            let state_ret = state.clone();
            let return_fn = interp.create_function(JsFunction::native(
                "return".to_string(),
                0,
                move |interp, _this, _args| {
                    let (iter, alive) = {
                        let s = state_ret.borrow();
                        (s.0.clone(), s.3)
                    };
                    state_ret.borrow_mut().3 = false;
                    if alive && let Err(e) = iterator_close_getter(interp, &iter) {
                        return Completion::Throw(e);
                    }
                    Completion::Normal(interp.create_iter_result_object(JsValue::UNDEFINED, true))
                },
            ));

            let helper = interp.create_iterator_helper_object(next_fn, return_fn);
            {
                let s = state.borrow();
                interp.set_helper_gc_roots(&helper, vec![s.0.clone(), s.1.clone(), s.2.clone()]);
            }
            Completion::Normal(helper)
        });

        // flatMap(mapper)
        self.define_method(
            iter_proto_id,
            "flatMap",
            1,
            |interp, this, args| {
                if !this.is_object() {
                    let err = interp.create_type_error("Iterator.prototype.flatMap called on non-object");
                    return Completion::Throw(err);
                }
                let mapper = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if !mapper.as_object_id().is_some_and(|id| {
                    interp
                        .get_object_cell(id)
                        .map(|od| od.borrow().callable.is_some())
                        .unwrap_or(false)
                }) {
                    let err = interp.create_type_error("mapper is not a function");
                    let err = close_iterator_for_error(interp, this, err);
                    return Completion::Throw(err);
                }
                let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let inner_roots = interp.create_array(vec![
                    JsValue::UNDEFINED,
                    JsValue::UNDEFINED,
                ]);
                // state: (outer_iter, outer_next, mapper, counter, rooted inner pair, alive, running)
                #[allow(clippy::type_complexity)]
                let state: Rc<
                    RefCell<(
                        JsValue,
                        JsValue,
                        JsValue,
                        f64,
                        JsValue,
                        bool,
                        bool,
                    )>,
                > = Rc::new(RefCell::new((
                    iter,
                    next_method,
                    mapper,
                    0.0,
                    inner_roots,
                    true,
                    false,
                )));

                let state_next = state.clone();
                let next_fn = interp.create_function(JsFunction::native(
                    "next".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let alive = state_next.borrow().5;
                        let running = state_next.borrow().6;
                        if !alive {
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::UNDEFINED, true),
                            );
                        }
                        if running {
                            let err = interp.create_type_error("Iterator helper method called while iterator is already being iterated");
                            return Completion::Throw(err);
                        }
                        state_next.borrow_mut().6 = true;
                        let result = (|| {
                            loop {
                                let (
                                    outer_iter,
                                    outer_next,
                                    mapper,
                                    counter,
                                    inner_roots,
                                    _alive,
                                    _running,
                                ) = {
                                    let s = state_next.borrow();
                                    (
                                        s.0.clone(),
                                        s.1.clone(),
                                        s.2.clone(),
                                        s.3,
                                        s.4.clone(),
                                        s.5,
                                        s.6,
                                    )
                                };

                                let (inner_iter, inner_next) = {
                                    let roots_id = inner_roots
                                        .as_object_id()
                                        .expect("flatMap inner roots must be an Array object");
                                    let roots_cell = interp.get_object_cell_expect(roots_id);
                                    let roots = roots_cell.borrow();
                                    let elements = roots
                                        .array_elements()
                                        .expect("flatMap inner roots must have Array elements");
                                    (
                                        elements
                                            .first()
                                            .filter(|value| !value.is_undefined())
                                            .cloned(),
                                        elements
                                            .get(1)
                                            .filter(|value| !value.is_undefined())
                                            .cloned(),
                                    )
                                };

                                // If we have an inner iterator, drain it
                                if let (Some(ii), Some(in_next)) = (&inner_iter, &inner_next) {
                                    match interp.iterator_step_direct(ii, in_next) {
                                        Ok(Some(result)) => {
                                            let value = match interp.iterator_value(&result) {
                                                Ok(v) => v,
                                                Err(e) => {
                                                    state_next.borrow_mut().5 = false;
                                                    let _ = iterator_close_getter(interp, &outer_iter);
                                                    return Completion::Throw(e);
                                                }
                                            };
                                            return Completion::Normal(
                                                interp.create_iter_result_object(value, false),
                                            );
                                        }
                                        Ok(None) => {
                                            let roots_id = inner_roots
                                                .as_object_id()
                                                .expect("flatMap inner roots must be an Array object");
                                            let roots_cell = interp.get_object_cell_expect(roots_id);
                                            let mut roots = roots_cell.borrow_mut();
                                            let elements = roots
                                                .array_elements_mut()
                                                .expect("flatMap inner roots must have Array elements");
                                            elements[0] = JsValue::UNDEFINED;
                                            elements[1] = JsValue::UNDEFINED;
                                            continue;
                                        }
                                        Err(e) => {
                                            state_next.borrow_mut().5 = false;
                                            let _ = iterator_close_getter(interp, &outer_iter);
                                            return Completion::Throw(e);
                                        }
                                    }
                                }

                                // Get next from outer
                                match interp.iterator_step_direct(&outer_iter, &outer_next) {
                                    Ok(Some(result)) => {
                                        let value = match interp.iterator_value(&result) {
                                            Ok(v) => v,
                                            Err(e) => return Completion::Throw(e),
                                        };
                                        let mapped = interp.call_function(
                                            &mapper,
                                            &JsValue::UNDEFINED,
                                            &[value, JsValue::number(counter)],
                                        );
                                        state_next.borrow_mut().3 = counter + 1.0;
                                        match mapped {
                                            Completion::Normal(mapped_val) => {
                                                match get_iterator_flattenable(interp, &mapped_val, true) {
                                                    Ok((new_inner, inner_next_method)) => {
                                                        let roots_id = inner_roots
                                                            .as_object_id()
                                                            .expect("flatMap inner roots must be an Array object");
                                                        let roots_cell =
                                                            interp.get_object_cell_expect(roots_id);
                                                        let mut roots = roots_cell.borrow_mut();
                                                        let elements = roots
                                                            .array_elements_mut()
                                                            .expect("flatMap inner roots must have Array elements");
                                                        elements[0] = new_inner;
                                                        elements[1] = inner_next_method;
                                                        continue;
                                                    }
                                                    Err(e) => {
                                                        state_next.borrow_mut().5 = false;
                                                        let _ = iterator_close_getter(interp, &outer_iter);
                                                        return Completion::Throw(e);
                                                    }
                                                }
                                            }
                                            Completion::Throw(e) => {
                                                state_next.borrow_mut().5 = false;
                                                let _ = iterator_close_getter(interp, &outer_iter);
                                                return Completion::Throw(e);
                                            }
                                            _ => {
                                                state_next.borrow_mut().5 = false;
                                                return Completion::Normal(
                                                    interp.create_iter_result_object(
                                                        JsValue::UNDEFINED,
                                                        true,
                                                    ),
                                                );
                                            }
                                        }
                                    }
                                    Ok(None) => {
                                        state_next.borrow_mut().5 = false;
                                        return Completion::Normal(
                                            interp
                                                .create_iter_result_object(JsValue::UNDEFINED, true),
                                        );
                                    }
                                    Err(e) => {
                                        state_next.borrow_mut().5 = false;
                                        return Completion::Throw(e);
                                    }
                                }
                            }
                        })();
                        state_next.borrow_mut().6 = false;
                        result
                    },
                ));

                let state_ret = state.clone();
                let return_fn = interp.create_function(JsFunction::native(
                    "return".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (outer_iter, inner_roots, alive, running) = {
                            let s = state_ret.borrow();
                            (s.0.clone(), s.4.clone(), s.5, s.6)
                        };
                        let inner_iter = {
                            let roots_id = inner_roots
                                .as_object_id()
                                .expect("flatMap inner roots must be an Array object");
                            let roots_cell = interp.get_object_cell_expect(roots_id);
                            let roots = roots_cell.borrow();
                            roots
                                .array_elements()
                                .expect("flatMap inner roots must have Array elements")
                                .first()
                                .filter(|value| !value.is_undefined())
                                .cloned()
                        };
                        if running {
                            let err = interp.create_type_error("Iterator helper method called while iterator is already being iterated");
                            return Completion::Throw(err);
                        }
                        state_ret.borrow_mut().6 = true;
                        state_ret.borrow_mut().5 = false;
                        let result = if alive {
                            if let Some(ref ii) = inner_iter {
                                let _ = iterator_close_getter(interp, ii);
                            }
                            if let Err(e) = iterator_close_getter(interp, &outer_iter) {
                                Completion::Throw(e)
                            } else {
                                Completion::Normal(
                                    interp.create_iter_result_object(JsValue::UNDEFINED, true),
                                )
                            }
                        } else {
                            Completion::Normal(
                                interp.create_iter_result_object(JsValue::UNDEFINED, true),
                            )
                        };
                        let roots_id = inner_roots
                            .as_object_id()
                            .expect("flatMap inner roots must be an Array object");
                        let roots_cell = interp.get_object_cell_expect(roots_id);
                        let mut roots = roots_cell.borrow_mut();
                        let elements = roots
                            .array_elements_mut()
                            .expect("flatMap inner roots must have Array elements");
                        elements[0] = JsValue::UNDEFINED;
                        elements[1] = JsValue::UNDEFINED;
                        state_ret.borrow_mut().6 = false;
                        result
                    },
                ));

                let helper = interp.create_iterator_helper_object(next_fn, return_fn);
                {
                    let b = state.borrow();
                    interp.set_helper_gc_roots(
                        &helper,
                        vec![b.0.clone(), b.1.clone(), b.2.clone(), b.4.clone()],
                    );
                }
                Completion::Normal(helper)
            },
        );
    }

    fn setup_iterator_static_methods(&mut self, iterator_ctor: &JsValue) {
        // Iterator.from(obj) — per spec §27.1.4.2
        // Create shared WrapForValidIteratorPrototype
        let wrap_valid_proto_id = self.create_object_id();
        self.get_object_cell_expect(wrap_valid_proto_id)
            .borrow_mut()
            .prototype_id = self.realm().iterator_prototype;

        self.define_method(
            wrap_valid_proto_id,
            "next",
            0,
            move |interp, this, _args| {
                let Some(this_id) = this.as_object_id() else {
                    let err = interp.create_type_error("next requires an Iterator wrapper");
                    return Completion::Throw(err);
                };
                let record = interp.get_object_cell(this_id).and_then(|o| {
                    if let Some(crate::interpreter::types::IterHelperData::Delegation {
                        iter,
                        next,
                    }) = o.borrow().iter_helper()
                    {
                        Some((iter.clone(), next.clone()))
                    } else {
                        None
                    }
                });
                let (iter, next_method) = match record {
                    Some(r) => r,
                    None => {
                        let err = interp.create_type_error("next requires an Iterator wrapper");
                        return Completion::Throw(err);
                    }
                };
                interp.call_function(&next_method, &iter, &[])
            },
        );

        self.define_method(
            wrap_valid_proto_id,
            "return",
            0,
            move |interp, this, _args| {
                let Some(this_id) = this.as_object_id() else {
                    let err = interp.create_type_error("return requires an Iterator wrapper");
                    return Completion::Throw(err);
                };
                let record = interp.get_object_cell(this_id).and_then(|o| {
                    if let Some(crate::interpreter::types::IterHelperData::Delegation {
                        iter,
                        next,
                    }) = o.borrow().iter_helper()
                    {
                        Some((iter.clone(), next.clone()))
                    } else {
                        None
                    }
                });
                let (iter, _next_method) = match record {
                    Some(r) => r,
                    None => {
                        let err = interp.create_type_error("return requires an Iterator wrapper");
                        return Completion::Throw(err);
                    }
                };
                if let Some(iter_id) = iter.as_object_id() {
                    let return_method = match interp.get_object_property(iter_id, "return", &iter) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => JsValue::UNDEFINED,
                    };
                    if return_method.is_nullish() {
                        return Completion::Normal(
                            interp.create_iter_result_object(JsValue::UNDEFINED, true),
                        );
                    }
                    interp.call_function(&return_method, &iter, &[])
                } else {
                    Completion::Normal(interp.create_iter_result_object(JsValue::UNDEFINED, true))
                }
            },
        );

        let wvp_for_from: Option<u64> = Some(wrap_valid_proto_id);
        let iterator_ctor_for_from = iterator_ctor.clone();
        let from_fn = self.create_function(JsFunction::native(
            "from".to_string(),
            1,
            move |interp, _this, args| {
                let obj = args.first().cloned().unwrap_or(JsValue::UNDEFINED);

                // Use GetIteratorFlattenable(obj, iterate-strings) per spec
                let (iter_val, next_method) = match get_iterator_flattenable(interp, &obj, false) {
                    Ok(pair) => pair,
                    Err(e) => return Completion::Throw(e),
                };

                // OrdinaryHasInstance(%Iterator%, iteratorRecord.[[Iterator]])
                let has_iter_proto =
                    match interp.ordinary_has_instance(&iterator_ctor_for_from, &iter_val) {
                        Completion::Normal(v) => v.as_boolean() == Some(true),
                        _ => false,
                    };

                if has_iter_proto {
                    return Completion::Normal(iter_val);
                }

                // Create wrapper with shared WrapForValidIteratorPrototype
                let wrapper_id = interp.create_object_id();
                interp
                    .get_object_cell_expect(wrapper_id)
                    .borrow_mut()
                    .prototype_id = wvp_for_from;
                interp
                    .get_object_cell_expect(wrapper_id)
                    .borrow_mut()
                    .class_name = "Iterator".to_string();
                {
                    let mut wrapper = interp.get_object_cell_expect(wrapper_id).borrow_mut();
                    wrapper.kind = crate::interpreter::types::ObjectKind::IterHelper(
                        crate::interpreter::types::IterHelperData::Delegation {
                            iter: iter_val.clone(),
                            next: next_method.clone(),
                        },
                    );
                    wrapper.gc_native_roots = Some(vec![iter_val, next_method]);
                }

                Completion::Normal(JsValue::object(wrapper_id))
            },
        ));

        if let Some(ctor_id) = iterator_ctor.as_object_id()
            && let Some(obj) = self.get_object_cell(ctor_id)
        {
            obj.borrow_mut().insert_builtin("from".to_string(), from_fn);
        }

        // Iterator.concat(...iterables)
        let concat_fn = self.create_function(JsFunction::native(
            "concat".to_string(),
            1,
            |interp, _this, args| {
                // Validate all args are iterable first (must be objects with @@iterator)
                let sym_key = interp.get_symbol_iterator_key();
                let mut iterables: Vec<(JsValue, JsValue)> = Vec::new();
                for arg in args {
                    // Per spec: each argument must NOT be a primitive (reject-primitives)
                    if !arg.is_object() {
                        let err = interp.create_type_error("value is not iterable");
                        return Completion::Throw(err);
                    }
                    if let Some(ref key) = sym_key {
                        let iter_fn = if let Some(arg_id) = arg.as_object_id() {
                            match interp.get_object_property(arg_id, key, arg) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => JsValue::UNDEFINED,
                            }
                        } else {
                            JsValue::UNDEFINED
                        };
                        if iter_fn.is_nullish() {
                            let err = interp.create_type_error("value is not iterable");
                            return Completion::Throw(err);
                        }
                        // Verify it's callable
                        let is_callable = if let Some(iter_fn_id) = iter_fn.as_object_id() {
                            interp
                                .get_object_cell(iter_fn_id)
                                .map(|od| od.borrow().callable.is_some())
                                .unwrap_or(false)
                        } else {
                            false
                        };
                        if !is_callable {
                            let err = interp.create_type_error("Symbol.iterator is not a function");
                            return Completion::Throw(err);
                        }
                        iterables.push((arg.clone(), iter_fn));
                    } else {
                        let err = interp.create_type_error("value is not iterable");
                        return Completion::Throw(err);
                    }
                }

                // state: (iterables, current_index, current_iter, current_next, alive, running)
                #[allow(clippy::type_complexity)]
                let state: Rc<
                    RefCell<(
                        Vec<(JsValue, JsValue)>,
                        usize,
                        Option<JsValue>,
                        Option<JsValue>,
                        bool,
                        bool,
                    )>,
                > = Rc::new(RefCell::new((iterables, 0, None, None, true, false)));

                let state_next = state.clone();
                let next_fn = interp.create_function(JsFunction::native(
                    "next".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let alive = state_next.borrow().4;
                        let running = state_next.borrow().5;
                        if !alive {
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::UNDEFINED, true),
                            );
                        }
                        if running {
                            let err = interp.create_type_error("Iterator helper method called while iterator is already being iterated");
                            return Completion::Throw(err);
                        }
                        state_next.borrow_mut().5 = true;
                        let result = (|| {
                        loop {
                            let (ref iterables, idx, ref cur_iter, ref cur_next, alive, _running) = {
                                let s = state_next.borrow();
                                (s.0.clone(), s.1, s.2.clone(), s.3.clone(), s.4, s.5)
                            };
                            if !alive {
                                return Completion::Normal(
                                    interp.create_iter_result_object(JsValue::UNDEFINED, true),
                                );
                            }

                            // If we have a current iterator, try getting next
                            if let (Some(ci), Some(cn)) = (cur_iter, cur_next) {
                                match interp.iterator_step_direct(ci, cn) {
                                    Ok(Some(result)) => {
                                        let value = match interp.iterator_value(&result) {
                                            Ok(v) => v,
                                            Err(e) => return Completion::Throw(e),
                                        };
                                        return Completion::Normal(
                                            interp.create_iter_result_object(value, false),
                                        );
                                    }
                                    Ok(None) => {
                                        // Current exhausted, move to next
                                        state_next.borrow_mut().1 = idx + 1;
                                        state_next.borrow_mut().2 = None;
                                        state_next.borrow_mut().3 = None;
                                        continue;
                                    }
                                    Err(e) => {
                                        state_next.borrow_mut().4 = false;
                                        return Completion::Throw(e);
                                    }
                                }
                            }

                            // Open next iterable
                            if idx >= iterables.len() {
                                state_next.borrow_mut().4 = false;
                                return Completion::Normal(
                                    interp.create_iter_result_object(JsValue::UNDEFINED, true),
                                );
                            }

                            let (ref iterable, ref iter_fn) = iterables[idx];
                            match interp.call_function(iter_fn, iterable, &[]) {
                                Completion::Normal(new_iter) => {
                                    if !new_iter.is_object() {
                                        state_next.borrow_mut().4 = false;
                                        let err = interp.create_type_error(
                                            "Result of Symbol.iterator is not an object",
                                        );
                                        return Completion::Throw(err);
                                    }
                                    let next_method =
                                        match get_iterator_direct_getter(interp, &new_iter) {
                                            Ok((_, nm)) => nm,
                                            Err(e) => {
                                                state_next.borrow_mut().4 = false;
                                                return Completion::Throw(e);
                                            }
                                        };
                                    state_next.borrow_mut().2 = Some(new_iter);
                                    state_next.borrow_mut().3 = Some(next_method);
                                    continue;
                                }
                                Completion::Throw(e) => {
                                    state_next.borrow_mut().4 = false;
                                    return Completion::Throw(e);
                                }
                                _ => {
                                    state_next.borrow_mut().4 = false;
                                    return Completion::Normal(
                                        interp.create_iter_result_object(JsValue::UNDEFINED, true),
                                    );
                                }
                            }
                        }
                        })();
                        state_next.borrow_mut().5 = false;
                        result
                    },
                ));

                let state_ret = state.clone();
                let return_fn = interp.create_function(JsFunction::native(
                    "return".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (cur_iter, alive, running) = {
                            let s = state_ret.borrow();
                            (s.2.clone(), s.4, s.5)
                        };
                        if running {
                            let err = interp.create_type_error("Iterator helper method called while iterator is already being iterated");
                            return Completion::Throw(err);
                        }
                        state_ret.borrow_mut().5 = true; // set running
                        state_ret.borrow_mut().4 = false;
                        state_ret.borrow_mut().2 = None;
                        state_ret.borrow_mut().3 = None;
                        let result = if alive
                            && let Some(ref ci) = cur_iter
                            && let Err(e) = iterator_close_getter(interp, ci)
                        {
                            Completion::Throw(e)
                        } else {
                            Completion::Normal(
                                interp.create_iter_result_object(JsValue::UNDEFINED, true),
                            )
                        };
                        state_ret.borrow_mut().5 = false; // clear running
                        result
                    },
                ));

                let helper = interp.create_iterator_helper_object(next_fn, return_fn);
                {
                    let b = state.borrow();
                    let mut roots = Vec::with_capacity(b.0.len() * 2 + 2);
                    for (io, nm) in &b.0 { roots.push(io.clone()); roots.push(nm.clone()); }
                    if let Some(ref v) = b.2 { roots.push(v.clone()); }
                    if let Some(ref v) = b.3 { roots.push(v.clone()); }
                    interp.set_helper_gc_roots(&helper, roots);
                }
                Completion::Normal(helper)
            },
        ));

        // Fix concat.length to 0 (spec says rest parameter = length 0)
        if let Some(concat_id) = concat_fn.as_object_id()
            && let Some(obj) = self.get_object_cell(concat_id)
        {
            obj.borrow_mut().insert_property(
                "length".to_string(),
                PropertyDescriptor::data(JsValue::number(0.0), false, false, true),
            );
        }

        if let Some(ctor_id) = iterator_ctor.as_object_id()
            && let Some(obj) = self.get_object_cell(ctor_id)
        {
            obj.borrow_mut()
                .insert_builtin("concat".to_string(), concat_fn);
        }

        // Iterator.zip(iterables [, options])
        let zip_fn = self.create_function(JsFunction::native(
            "zip".to_string(),
            1,
            |interp, _this, args| {
                let iterables_arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);

                // Step 1: If iterables is not an Object, throw a TypeError
                if !iterables_arg.is_object() {
                    let err = interp.create_type_error("iterables is not an object");
                    return Completion::Throw(err);
                }

                // Step 2: GetOptionsObject(options)
                let options = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                if !options.is_undefined() && !options.is_object() {
                    let err = interp.create_type_error("options must be an object or undefined");
                    return Completion::Throw(err);
                }

                // Step 3: Get mode — NOT ToString, direct string comparison
                let mode = if options.is_undefined() {
                    "shortest".to_string()
                } else if let Some(options_id) = options.as_object_id() {
                    let mode_val = match interp.get_object_property(options_id, "mode", &options) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => JsValue::UNDEFINED,
                    };
                    if mode_val.is_undefined() {
                        "shortest".to_string()
                    } else if let Some(s) = mode_val.as_string() {
                        let rs = s.to_rust_string();
                        match rs.as_str() {
                            "shortest" | "longest" | "strict" => rs,
                            _ => {
                                let err = interp.create_type_error(
                                    "mode must be 'shortest', 'longest', or 'strict'",
                                );
                                return Completion::Throw(err);
                            }
                        }
                    } else {
                        let err = interp
                            .create_type_error("mode must be 'shortest', 'longest', or 'strict'");
                        return Completion::Throw(err);
                    }
                } else {
                    "shortest".to_string()
                };

                // Step 7: Get padding from options (for "longest" mode)
                let padding_option = if mode == "longest" {
                    if let Some(options_id) = options.as_object_id() {
                        let p = match interp.get_object_property(options_id, "padding", &options) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => JsValue::UNDEFINED,
                        };
                        if !p.is_undefined() {
                            if !p.is_object() {
                                let err = interp
                                    .create_type_error("padding must be an object or undefined");
                                return Completion::Throw(err);
                            }
                            Some(p)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Step 10: GetIterator(iterables, sync)
                let (input_iter, input_next) = match get_iterator_getter(interp, &iterables_arg) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // Step 12: Collect all iterables using GetIteratorFlattenable(next, reject-strings)
                // Temp-root each inner iterator as it's collected (subsequent iterations can trigger GC)
                let mut iters: Vec<(JsValue, JsValue)> = Vec::new();
                let mut collection_temp_ids: Vec<u64> = Vec::new();

                loop {
                    match iterator_step_value_getter(interp, &input_iter, &input_next) {
                        Ok(Some(next_val)) => {
                            match get_iterator_flattenable(interp, &next_val, true) {
                                Ok(pair) => {
                                    if let Some(id) = pair.0.as_object_id() {
                                        collection_temp_ids.push(id);
                                        interp.gc_temp_roots.push(id);
                                    }
                                    if let Some(id) = pair.1.as_object_id() {
                                        collection_temp_ids.push(id);
                                        interp.gc_temp_roots.push(id);
                                    }
                                    iters.push(pair);
                                }
                                Err(e) => {
                                    // IfAbruptCloseIterators(iter, « inputIter » + iters) — reverse order
                                    let mut all = vec![(input_iter.clone(), input_next.clone())];
                                    all.extend(iters.iter().cloned());
                                    let _ = iterator_close_all(interp, &all, Err(e.clone()));
                                    for id in &collection_temp_ids {
                                        if let Some(pos) =
                                            interp.gc_temp_roots.iter().position(|x| *x == *id)
                                        {
                                            interp.gc_temp_roots.swap_remove(pos);
                                        }
                                    }
                                    return Completion::Throw(e);
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            // IfAbruptCloseIterators(next, iters) — just the collected iters
                            let _ = iterator_close_all(interp, &iters, Err(e.clone()));
                            for id in &collection_temp_ids {
                                if let Some(pos) =
                                    interp.gc_temp_roots.iter().position(|x| *x == *id)
                                {
                                    interp.gc_temp_roots.swap_remove(pos);
                                }
                            }
                            return Completion::Throw(e);
                        }
                    }
                }

                let iter_count = iters.len();

                // Step 14: Collect padding values (exactly iter_count values)
                let padding_values: Vec<JsValue> = if mode == "longest" {
                    if let Some(pad_iterable) = padding_option {
                        let (pi, pn) = match get_iterator_getter(interp, &pad_iterable) {
                            Ok(v) => v,
                            Err(e) => {
                                let _ = iterator_close_all(interp, &iters, Err(e.clone()));
                                return Completion::Throw(e);
                            }
                        };
                        let mut pads = Vec::with_capacity(iter_count);
                        let mut using_iterator = true;
                        for _ in 0..iter_count {
                            if using_iterator {
                                match iterator_step_value_getter(interp, &pi, &pn) {
                                    Ok(Some(v)) => pads.push(v),
                                    Ok(None) => {
                                        using_iterator = false;
                                        pads.push(JsValue::UNDEFINED);
                                    }
                                    Err(e) => {
                                        let _ = iterator_close_all(interp, &iters, Err(e.clone()));
                                        return Completion::Throw(e);
                                    }
                                }
                            } else {
                                pads.push(JsValue::UNDEFINED);
                            }
                        }
                        if using_iterator && let Err(e) = iterator_close_getter(interp, &pi) {
                            let _ = iterator_close_all(interp, &iters, Err(e.clone()));
                            return Completion::Throw(e);
                        }
                        pads
                    } else {
                        vec![JsValue::UNDEFINED; iter_count]
                    }
                } else {
                    vec![JsValue::UNDEFINED; iter_count]
                };

                // Also temp-root padding object ids during padding collection
                for pad_val in &padding_values {
                    if let Some(id) = pad_val.as_object_id() {
                        collection_temp_ids.push(id);
                        interp.gc_temp_roots.push(id);
                    }
                }

                // State: (iters, exhausted, mode, padding, alive)
                #[allow(clippy::type_complexity)]
                let state: Rc<
                    RefCell<(
                        Vec<(JsValue, JsValue)>,
                        Vec<bool>,
                        String,
                        Vec<JsValue>,
                        bool,
                    )>,
                > = Rc::new(RefCell::new((
                    iters,
                    vec![false; iter_count],
                    mode,
                    padding_values,
                    true,
                )));

                let state_next = state.clone();
                let next_fn = interp.create_function(JsFunction::native(
                    "next".to_string(),
                    0,
                    move |interp, _this, _args| {
                        // Temp-root all inner iterators during next() execution
                        let gc_ids: Vec<u64> = {
                            let s = state_next.borrow();
                            let mut ids = Vec::new();
                            for (io, nm) in &s.0 {
                                if let Some(id) = io.as_object_id() {
                                    ids.push(id);
                                }
                                if let Some(id) = nm.as_object_id() {
                                    ids.push(id);
                                }
                            }
                            for pad in &s.3 {
                                if let Some(id) = pad.as_object_id() {
                                    ids.push(id);
                                }
                            }
                            ids
                        };
                        for &id in &gc_ids {
                            interp.gc_temp_roots.push(id);
                        }

                        let result = zip_next_inner(interp, &state_next);

                        for id in &gc_ids {
                            if let Some(pos) = interp.gc_temp_roots.iter().position(|x| x == id) {
                                interp.gc_temp_roots.swap_remove(pos);
                            }
                        }
                        result
                    },
                ));

                let state_ret = state.clone();
                let return_fn = interp.create_function(JsFunction::native(
                    "return".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (ref iters, ref exhausted, alive) = {
                            let s = state_ret.borrow();
                            (s.0.clone(), s.1.clone(), s.4)
                        };
                        state_ret.borrow_mut().4 = false;
                        if alive {
                            let open: Vec<(JsValue, JsValue)> = iters
                                .iter()
                                .enumerate()
                                .filter(|(i, _)| !exhausted[*i])
                                .map(|(_, pair)| pair.clone())
                                .collect();
                            if let Err(e) = iterator_close_all(interp, &open, Ok(())) {
                                return Completion::Throw(e);
                            }
                        }
                        Completion::Normal(
                            interp.create_iter_result_object(JsValue::UNDEFINED, true),
                        )
                    },
                ));

                let helper = interp.create_iterator_helper_object(next_fn, return_fn);
                {
                    let b = state.borrow();
                    let mut roots = Vec::with_capacity(b.0.len() * 2 + b.3.len());
                    for (io, nm) in &b.0 {
                        roots.push(io.clone());
                        roots.push(nm.clone());
                    }
                    for pad in &b.3 {
                        roots.push(pad.clone());
                    }
                    interp.set_helper_gc_roots(&helper, roots);
                }
                // Remove all temp roots from collection and padding phases
                for id in &collection_temp_ids {
                    if let Some(pos) = interp.gc_temp_roots.iter().position(|x| *x == *id) {
                        interp.gc_temp_roots.swap_remove(pos);
                    }
                }
                Completion::Normal(helper)
            },
        ));

        if let Some(ctor_id) = iterator_ctor.as_object_id()
            && let Some(obj) = self.get_object_cell(ctor_id)
        {
            obj.borrow_mut().insert_builtin("zip".to_string(), zip_fn);
        }

        // Iterator.zipKeyed(iterables [, options])
        let zip_keyed_fn = self.create_function(JsFunction::native(
            "zipKeyed".to_string(),
            1,
            |interp, _this, args| {
                let iterables_obj = args.first().cloned().unwrap_or(JsValue::UNDEFINED);

                // Step 1: iterables must be an object
                let Some(obj_id) = iterables_obj.as_object_id() else {
                    let err = interp.create_type_error("iterables must be an object");
                    return Completion::Throw(err);
                };

                // Step 2: GetOptionsObject(options)
                let options = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                if !options.is_undefined() && !options.is_object() {
                    let err = interp.create_type_error("options must be an object or undefined");
                    return Completion::Throw(err);
                }

                // Step 3: Get mode — direct string comparison, no ToString
                let mode = if options.is_undefined() {
                    "shortest".to_string()
                } else if let Some(options_id) = options.as_object_id() {
                    let mode_val = match interp.get_object_property(options_id, "mode", &options) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => JsValue::UNDEFINED,
                    };
                    if mode_val.is_undefined() {
                        "shortest".to_string()
                    } else if let Some(s) = mode_val.as_string() {
                        let rs = s.to_rust_string();
                        match rs.as_str() {
                            "shortest" | "longest" | "strict" => rs,
                            _ => {
                                let err = interp.create_type_error(
                                    "mode must be 'shortest', 'longest', or 'strict'",
                                );
                                return Completion::Throw(err);
                            }
                        }
                    } else {
                        let err = interp
                            .create_type_error("mode must be 'shortest', 'longest', or 'strict'");
                        return Completion::Throw(err);
                    }
                } else {
                    "shortest".to_string()
                };

                // Step 7: Get padding from options (for "longest" mode)
                let padding_option = if mode == "longest" {
                    if let Some(options_id) = options.as_object_id() {
                        let p = match interp.get_object_property(options_id, "padding", &options) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => JsValue::UNDEFINED,
                        };
                        if !p.is_undefined() {
                            if !p.is_object() {
                                let err = interp
                                    .create_type_error("padding must be an object or undefined");
                                return Completion::Throw(err);
                            }
                            Some(p)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Step 10: allKeys = iterables.[[OwnPropertyKeys]]()
                let all_keys = match interp.proxy_own_keys(obj_id) {
                    Ok(keys) => keys,
                    Err(e) => return Completion::Throw(e),
                };

                // Step 11-12: For each key, [[GetOwnProperty]], check enumerable, Get value
                // Temp-root each inner iterator as it's collected (subsequent iterations can trigger GC)
                let mut key_names: Vec<JsPropertyKey> = Vec::new();
                let mut iters: Vec<(JsValue, JsValue)> = Vec::new();
                let mut collection_temp_ids: Vec<u64> = Vec::new();

                for key_val in &all_keys {
                    let key = crate::interpreter::helpers::to_property_key_string(key_val);

                    // Step 12.a: desc = iterables.[[GetOwnProperty]](key)
                    let is_enumerable = match interp.proxy_get_own_property_descriptor(obj_id, &key)
                    {
                        Ok(desc_val) => {
                            if desc_val.is_undefined() {
                                false
                            } else if let Some(desc_id) = desc_val.as_object_id() {
                                // Read enumerable from descriptor object
                                match interp.get_object_property(desc_id, "enumerable", &desc_val) {
                                    Completion::Normal(v) => {
                                        crate::interpreter::helpers::to_boolean(&v)
                                    }
                                    _ => false,
                                }
                            } else {
                                // Non-proxy: proxy_get_own_property_descriptor returns
                                // the descriptor directly for ordinary objects
                                false
                            }
                        }
                        Err(e) => {
                            // Step 12.b: IfAbruptCloseIterators
                            let _ = iterator_close_all(interp, &iters, Err(e.clone()));
                            for id in &collection_temp_ids {
                                if let Some(pos) =
                                    interp.gc_temp_roots.iter().position(|x| *x == *id)
                                {
                                    interp.gc_temp_roots.swap_remove(pos);
                                }
                            }
                            return Completion::Throw(e);
                        }
                    };
                    if !is_enumerable {
                        continue;
                    }

                    // Step 12.c.i: value = Get(iterables, key)
                    let iterable = match interp.get_object_property(obj_id, &key, &iterables_obj) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            let _ = iterator_close_all(interp, &iters, Err(e.clone()));
                            for id in &collection_temp_ids {
                                if let Some(pos) =
                                    interp.gc_temp_roots.iter().position(|x| *x == *id)
                                {
                                    interp.gc_temp_roots.swap_remove(pos);
                                }
                            }
                            return Completion::Throw(e);
                        }
                        _ => JsValue::UNDEFINED,
                    };

                    // Step 12.c.iii: If value is not undefined
                    if iterable.is_undefined() {
                        continue;
                    }

                    match get_iterator_flattenable(interp, &iterable, true) {
                        Ok(pair) => {
                            if let Some(id) = pair.0.as_object_id() {
                                collection_temp_ids.push(id);
                                interp.gc_temp_roots.push(id);
                            }
                            if let Some(id) = pair.1.as_object_id() {
                                collection_temp_ids.push(id);
                                interp.gc_temp_roots.push(id);
                            }
                            key_names.push(key.clone());
                            iters.push(pair);
                        }
                        Err(e) => {
                            let _ = iterator_close_all(interp, &iters, Err(e.clone()));
                            for id in &collection_temp_ids {
                                if let Some(pos) =
                                    interp.gc_temp_roots.iter().position(|x| *x == *id)
                                {
                                    interp.gc_temp_roots.swap_remove(pos);
                                }
                            }
                            return Completion::Throw(e);
                        }
                    }
                }

                let iter_count = iters.len();

                // Step 14: Get padding values per key (for longest mode)
                let padding_values: Vec<JsValue> = if mode == "longest" {
                    if let Some(ref pad_obj) = padding_option {
                        if let Some(pad_id) = pad_obj.as_object_id() {
                            let mut pads = Vec::with_capacity(iter_count);
                            for key in &key_names {
                                let val = match interp.get_object_property(pad_id, key, pad_obj) {
                                    Completion::Normal(v) => v,
                                    Completion::Throw(e) => {
                                        let _ = iterator_close_all(interp, &iters, Err(e.clone()));
                                        return Completion::Throw(e);
                                    }
                                    _ => JsValue::UNDEFINED,
                                };
                                pads.push(val);
                            }
                            pads
                        } else {
                            vec![JsValue::UNDEFINED; iter_count]
                        }
                    } else {
                        vec![JsValue::UNDEFINED; iter_count]
                    }
                } else {
                    vec![JsValue::UNDEFINED; iter_count]
                };

                // Also temp-root padding object ids
                for pad_val in &padding_values {
                    if let Some(id) = pad_val.as_object_id() {
                        collection_temp_ids.push(id);
                        interp.gc_temp_roots.push(id);
                    }
                }

                let state: ZipKeyedState = Rc::new(RefCell::new((
                    key_names,
                    iters,
                    vec![false; iter_count],
                    mode,
                    padding_values,
                    true,
                )));

                let state_next = state.clone();
                let next_fn = interp.create_function(JsFunction::native(
                    "next".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let gc_ids: Vec<u64> = {
                            let s = state_next.borrow();
                            let mut ids = Vec::new();
                            for (io, nm) in &s.1 {
                                if let Some(id) = io.as_object_id() {
                                    ids.push(id);
                                }
                                if let Some(id) = nm.as_object_id() {
                                    ids.push(id);
                                }
                            }
                            for pad in &s.4 {
                                if let Some(id) = pad.as_object_id() {
                                    ids.push(id);
                                }
                            }
                            ids
                        };
                        for &id in &gc_ids {
                            interp.gc_temp_roots.push(id);
                        }
                        let result = zip_keyed_next_inner(interp, &state_next);
                        for id in &gc_ids {
                            if let Some(pos) = interp.gc_temp_roots.iter().position(|x| x == id) {
                                interp.gc_temp_roots.swap_remove(pos);
                            }
                        }
                        result
                    },
                ));

                let state_ret = state.clone();
                let return_fn = interp.create_function(JsFunction::native(
                    "return".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (ref iters, ref exhausted, alive) = {
                            let s = state_ret.borrow();
                            (s.1.clone(), s.2.clone(), s.5)
                        };
                        state_ret.borrow_mut().5 = false;
                        if alive {
                            let open: Vec<(JsValue, JsValue)> = iters
                                .iter()
                                .enumerate()
                                .filter(|(i, _)| !exhausted[*i])
                                .map(|(_, pair)| pair.clone())
                                .collect();
                            if let Err(e) = iterator_close_all(interp, &open, Ok(())) {
                                return Completion::Throw(e);
                            }
                        }
                        Completion::Normal(
                            interp.create_iter_result_object(JsValue::UNDEFINED, true),
                        )
                    },
                ));

                // collection_temp_ids already pushed during collection loop
                let helper = interp.create_iterator_helper_object(next_fn, return_fn);
                {
                    let b = state.borrow();
                    let mut roots = Vec::with_capacity(b.1.len() * 2 + b.4.len());
                    for (io, nm) in &b.1 {
                        roots.push(io.clone());
                        roots.push(nm.clone());
                    }
                    for pad in &b.4 {
                        roots.push(pad.clone());
                    }
                    interp.set_helper_gc_roots(&helper, roots);
                }
                for id in &collection_temp_ids {
                    if let Some(pos) = interp.gc_temp_roots.iter().position(|x| *x == *id) {
                        interp.gc_temp_roots.swap_remove(pos);
                    }
                }
                Completion::Normal(helper)
            },
        ));

        if let Some(ctor_id) = iterator_ctor.as_object_id()
            && let Some(obj) = self.get_object_cell(ctor_id)
        {
            obj.borrow_mut()
                .insert_builtin("zipKeyed".to_string(), zip_keyed_fn);
        }
    }

    pub(crate) fn create_array_iterator(&mut self, array_id: u64, kind: IteratorKind) -> JsValue {
        let mut obj_data = JsObjectData::new();
        obj_data.prototype_id = self
            .realm()
            .array_iterator_prototype
            .or(self.realm().iterator_prototype)
            .or(self.realm().object_prototype);
        obj_data.class_name = "Array Iterator".to_string();
        obj_data.kind =
            crate::interpreter::types::ObjectKind::Iterator(IteratorState::ArrayIterator {
                array_id,
                index: 0,
                kind,
                done: false,
            });
        let id = self.alloc_object(obj_data);
        JsValue::object(id)
    }

    pub(crate) fn create_typed_array_iterator(
        &mut self,
        typed_array_id: u64,
        kind: IteratorKind,
    ) -> JsValue {
        let mut obj_data = JsObjectData::new();
        obj_data.prototype_id = self
            .realm()
            .array_iterator_prototype
            .or(self.realm().iterator_prototype)
            .or(self.realm().object_prototype);
        obj_data.class_name = "Array Iterator".to_string();
        obj_data.kind =
            crate::interpreter::types::ObjectKind::Iterator(IteratorState::TypedArrayIterator {
                typed_array_id,
                index: 0,
                kind,
                done: false,
            });
        let id = self.alloc_object(obj_data);
        JsValue::object(id)
    }

    pub(crate) fn create_string_iterator(&mut self, string: JsString) -> JsValue {
        let mut obj_data = JsObjectData::new();
        obj_data.prototype_id = self
            .realm()
            .string_iterator_prototype
            .or(self.realm().iterator_prototype)
            .or(self.realm().object_prototype);
        obj_data.class_name = "String Iterator".to_string();
        obj_data.kind =
            crate::interpreter::types::ObjectKind::Iterator(IteratorState::StringIterator {
                string,
                position: 0,
                done: false,
            });
        let id = self.alloc_object(obj_data);
        JsValue::object(id)
    }

    pub(crate) fn setup_generator_prototype(&mut self) {
        let gen_proto_id = self.create_object_id();
        self.get_object_cell_expect(gen_proto_id)
            .borrow_mut()
            .class_name = "Generator".to_string();
        self.get_object_cell_expect(gen_proto_id)
            .borrow_mut()
            .prototype_id = self.realm().iterator_prototype;

        // next(value)
        let next_fn = self.create_function(JsFunction::native(
            "next".to_string(),
            1,
            |interp, this, args| {
                let value = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                // Check which variant we have
                if let Some(this_id) = this.as_object_id()
                    && let Some(obj_rc) = interp.get_object_cell(this_id)
                {
                    let is_state_machine = matches!(
                        obj_rc.borrow().iterator_state(),
                        Some(IteratorState::StateMachineGenerator { .. })
                    );
                    if is_state_machine {
                        return interp.generator_next_state_machine(this, value);
                    }
                }
                interp.generator_next(this, value)
            },
        ));
        self.get_object_cell_expect(gen_proto_id)
            .borrow_mut()
            .insert_property(
                "next".to_string(),
                PropertyDescriptor::data(next_fn, true, false, true),
            );

        // return(value)
        let return_fn = self.create_function(JsFunction::native(
            "return".to_string(),
            1,
            |interp, this, args| {
                let value = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                // Check which variant we have
                if let Some(this_id) = this.as_object_id()
                    && let Some(obj_rc) = interp.get_object_cell(this_id)
                {
                    let is_state_machine = matches!(
                        obj_rc.borrow().iterator_state(),
                        Some(IteratorState::StateMachineGenerator { .. })
                    );
                    if is_state_machine {
                        return interp.generator_return_state_machine(this, value);
                    }
                }
                interp.generator_return(this, value)
            },
        ));
        self.get_object_cell_expect(gen_proto_id)
            .borrow_mut()
            .insert_property(
                "return".to_string(),
                PropertyDescriptor::data(return_fn, true, false, true),
            );

        // throw(exception)
        let throw_fn = self.create_function(JsFunction::native(
            "throw".to_string(),
            1,
            |interp, this, args| {
                let exception = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                // Check which variant we have
                if let Some(this_id) = this.as_object_id()
                    && let Some(obj_rc) = interp.get_object_cell(this_id)
                {
                    let is_state_machine = matches!(
                        obj_rc.borrow().iterator_state(),
                        Some(IteratorState::StateMachineGenerator { .. })
                    );
                    if is_state_machine {
                        return interp.generator_throw_state_machine(this, exception);
                    }
                }
                interp.generator_throw(this, exception)
            },
        ));
        self.get_object_cell_expect(gen_proto_id)
            .borrow_mut()
            .insert_property(
                "throw".to_string(),
                PropertyDescriptor::data(throw_fn, true, false, true),
            );

        // @@iterator is inherited from %IteratorPrototype%

        // Symbol.toStringTag
        self.define_to_string_tag(gen_proto_id, "Generator");

        self.realm_mut().generator_prototype = Some(gen_proto_id);

        // %GeneratorFunction.prototype% - the prototype of generator function objects
        let gf_proto_id = self.create_object_id();
        self.get_object_cell_expect(gf_proto_id)
            .borrow_mut()
            .class_name = "GeneratorFunction".to_string();

        // [[Prototype]] = Function.prototype_id
        // Get Function.prototype from global Function
        if let Some(func_val) = self.get_global_var("Function")
            && let Some(func_id) = func_val.as_object_id()
            && let Some(func_proto_id) =
                self.get_property_on_id(func_id, "prototype").as_object_id()
            && let Some(func_proto) = self.get_object_cell(func_proto_id)
        {
            self.get_object_cell_expect(gf_proto_id)
                .borrow_mut()
                .prototype_id = Some(func_proto.borrow().id.unwrap());
        }

        // GeneratorFunction.prototype.prototype_id = Generator.prototype_id
        self.get_object_cell_expect(gf_proto_id)
            .borrow_mut()
            .insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(JsValue::object(gen_proto_id), false, false, true),
            );

        // Symbol.toStringTag = "GeneratorFunction"
        self.define_to_string_tag(gf_proto_id, "GeneratorFunction");

        // Set constructor on Generator.prototype pointing back to GeneratorFunction.prototype_id
        self.get_object_cell_expect(gen_proto_id)
            .borrow_mut()
            .insert_property(
                "constructor".to_string(),
                PropertyDescriptor::data(JsValue::object(gf_proto_id), false, false, true),
            );

        self.realm_mut().generator_function_prototype = Some(gf_proto_id);
    }

    pub(crate) fn setup_async_generator_prototype(&mut self) {
        // %AsyncIteratorPrototype% — has [Symbol.asyncIterator]() returning this
        let async_iter_proto_id = self.create_object_id();
        self.get_object_cell_expect(async_iter_proto_id)
            .borrow_mut()
            .class_name = "AsyncIterator".to_string();

        let async_iter_self_fn = self.create_function(JsFunction::native(
            "[Symbol.asyncIterator]".to_string(),
            0,
            |_interp, this, _args| Completion::Normal(this.clone()),
        ));
        if let Some(key) = self.get_symbol_key("asyncIterator") {
            self.get_object_cell_expect(async_iter_proto_id)
                .borrow_mut()
                .insert_property(
                    key,
                    PropertyDescriptor::data(async_iter_self_fn, true, false, true),
                );
        }

        // [Symbol.asyncDispose]() — §27.1.3.2
        let async_dispose_fn = self.create_function(JsFunction::native(
            "[Symbol.asyncDispose]".to_string(),
            0,
            |interp, this, _args| {
                // 1. Let O be the this value.
                // 2. Let promiseCapability be ! NewPromiseCapability(%Promise%).
                let promise_ctor = interp
                    .get_global_var("Promise")
                    .unwrap_or(JsValue::UNDEFINED);
                let cap = match interp.new_promise_capability(&promise_ctor) {
                    Ok(c) => c,
                    Err(e) => return Completion::Throw(e),
                };
                let cap_promise_id = cap.promise.as_object_id().unwrap_or(0);

                // 3. Let return be GetMethod(O, "return").
                // 4. IfAbruptRejectPromise(return, promiseCapability).
                let return_method = match interp.obj_get(this, "return") {
                    Ok(v) => v,
                    Err(e) => {
                        interp.reject_promise(cap_promise_id, e);
                        return Completion::Normal(cap.promise);
                    }
                };
                let return_method = if interp.is_callable(&return_method) {
                    Some(return_method)
                } else if return_method.is_nullish() {
                    None
                } else if !return_method.is_nullish() {
                    let e = interp.create_type_error("return is not a function");
                    interp.reject_promise(cap_promise_id, e);
                    return Completion::Normal(cap.promise);
                } else {
                    None
                };

                // 5. If return is undefined, then
                //   a. Perform ! Call(promiseCapability.[[Resolve]], undefined, « undefined »).
                if return_method.is_none() {
                    let _ = interp.call_function(
                        &cap.resolve,
                        &JsValue::UNDEFINED,
                        &[JsValue::UNDEFINED],
                    );
                    return Completion::Normal(cap.promise);
                }

                // 6. Else,
                let return_method = return_method.unwrap();
                //   a. Let result be Call(return, O, « »).
                //   b. IfAbruptRejectPromise(result, promiseCapability).
                let result = match interp.call_function(&return_method, this, &[]) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => {
                        interp.reject_promise(cap_promise_id, e);
                        return Completion::Normal(cap.promise);
                    }
                    c => return c,
                };

                //   c. Let resultWrapper be Completion(PromiseResolve(%Promise%, result)).
                //   d. IfAbruptRejectPromise(resultWrapper, promiseCapability).
                let result_wrapper =
                    match interp.promise_resolve_with_constructor(&promise_ctor, &result) {
                        Ok(v) => v,
                        Err(e) => {
                            interp.reject_promise(cap_promise_id, e);
                            return Completion::Normal(cap.promise);
                        }
                    };

                //   e-f. Let onFulfilled be a function that returns undefined.
                let on_fulfilled = interp.create_function(JsFunction::native(
                    "".to_string(),
                    1,
                    |_interp, _this, _args| Completion::Normal(JsValue::UNDEFINED),
                ));

                //   g. Perform PerformPromiseThen(resultWrapper, onFulfilled, undefined, promiseCapability).
                let wrapper_id = result_wrapper.as_object_id().unwrap_or(0);
                let fulfill_reaction = crate::interpreter::types::PromiseReaction {
                    handler: Some(on_fulfilled),
                    promise_id: Some(cap_promise_id),
                    resolve: cap.resolve.clone(),
                    reject: cap.reject.clone(),
                    reaction_type: crate::interpreter::types::PromiseReactionType::Fulfill,
                };
                let reject_reaction = crate::interpreter::types::PromiseReaction {
                    handler: None,
                    promise_id: Some(cap_promise_id),
                    resolve: cap.resolve,
                    reject: cap.reject,
                    reaction_type: crate::interpreter::types::PromiseReactionType::Reject,
                };

                let fulfill_reaction2 = fulfill_reaction.clone();
                let reject_reaction2 = reject_reaction.clone();
                let state = if let Some(obj) = interp.get_object_cell(wrapper_id) {
                    let mut o = obj.borrow_mut();
                    if let Some(pd) = o.promise_data_mut() {
                        pd.is_handled = true;
                        match &pd.state {
                            crate::interpreter::types::PromiseState::Pending => {
                                pd.fulfill_reactions.push(fulfill_reaction);
                                pd.reject_reactions.push(reject_reaction);
                                None
                            }
                            crate::interpreter::types::PromiseState::Fulfilled(v) => {
                                Some((true, v.clone()))
                            }
                            crate::interpreter::types::PromiseState::Rejected(r) => {
                                Some((false, r.clone()))
                            }
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some((is_fulfilled, value)) = state {
                    if is_fulfilled {
                        interp.trigger_promise_reactions(vec![fulfill_reaction2], value);
                    } else {
                        interp.trigger_promise_reactions(vec![reject_reaction2], value);
                    }
                }

                // 7. Return promiseCapability.[[Promise]].
                Completion::Normal(cap.promise)
            },
        ));
        if let Some(key) = self.get_symbol_key("asyncDispose") {
            self.get_object_cell_expect(async_iter_proto_id)
                .borrow_mut()
                .insert_property(
                    key,
                    PropertyDescriptor::data(async_dispose_fn, true, false, true),
                );
        }

        self.realm_mut().async_iterator_prototype = Some(async_iter_proto_id);

        // %AsyncGeneratorPrototype%
        let gen_proto_id = self.create_object_id();
        self.get_object_cell_expect(gen_proto_id)
            .borrow_mut()
            .prototype_id = Some(async_iter_proto_id);
        self.get_object_cell_expect(gen_proto_id)
            .borrow_mut()
            .class_name = "AsyncGenerator".to_string();

        // next(value)
        let next_fn = self.create_function(JsFunction::native(
            "next".to_string(),
            1,
            |interp, this, args| {
                let value = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                interp.async_generator_next(this, value)
            },
        ));
        self.get_object_cell_expect(gen_proto_id)
            .borrow_mut()
            .insert_property(
                "next".to_string(),
                PropertyDescriptor::data(next_fn, true, false, true),
            );

        // return(value)
        let return_fn = self.create_function(JsFunction::native(
            "return".to_string(),
            1,
            |interp, this, args| {
                let value = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                interp.async_generator_return(this, value)
            },
        ));
        self.get_object_cell_expect(gen_proto_id)
            .borrow_mut()
            .insert_property(
                "return".to_string(),
                PropertyDescriptor::data(return_fn, true, false, true),
            );

        // throw(exception)
        let throw_fn = self.create_function(JsFunction::native(
            "throw".to_string(),
            1,
            |interp, this, args| {
                let exception = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                interp.async_generator_throw(this, exception)
            },
        ));
        self.get_object_cell_expect(gen_proto_id)
            .borrow_mut()
            .insert_property(
                "throw".to_string(),
                PropertyDescriptor::data(throw_fn, true, false, true),
            );

        // Symbol.toStringTag
        self.define_to_string_tag(gen_proto_id, "AsyncGenerator");

        self.realm_mut().async_generator_prototype = Some(gen_proto_id);

        // %AsyncGeneratorFunction.prototype%
        let agf_proto_id = self.create_object_id();
        self.get_object_cell_expect(agf_proto_id)
            .borrow_mut()
            .class_name = "AsyncGeneratorFunction".to_string();
        // prototype property points to AsyncGenerator.prototype_id
        self.get_object_cell_expect(agf_proto_id)
            .borrow_mut()
            .insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(JsValue::object(gen_proto_id), false, false, true),
            );
        // Symbol.toStringTag
        self.define_to_string_tag(agf_proto_id, "AsyncGeneratorFunction");
        // Set constructor on AsyncGenerator.prototype pointing back to AsyncGeneratorFunction.prototype_id
        self.get_object_cell_expect(gen_proto_id)
            .borrow_mut()
            .insert_property(
                "constructor".to_string(),
                PropertyDescriptor::data(JsValue::object(agf_proto_id), false, false, true),
            );
        self.realm_mut().async_generator_function_prototype = Some(agf_proto_id);
    }
}

impl Interpreter {
    pub(crate) fn get_symbol_iterator_key(&self) -> Option<JsPropertyKey> {
        self.get_symbol_key("iterator")
    }

    pub(crate) fn create_iter_result_object(&mut self, value: JsValue, done: bool) -> JsValue {
        let obj_id = self.create_object_id();
        self.get_object_cell_expect(obj_id)
            .borrow_mut()
            .insert_value("value".to_string(), value);
        self.get_object_cell_expect(obj_id)
            .borrow_mut()
            .insert_value("done".to_string(), JsValue::boolean(done));
        let id = obj_id;
        JsValue::object(id)
    }

    pub(crate) fn get_iterator(&mut self, obj: &JsValue) -> Result<JsValue, JsValue> {
        let sym_key = self.get_symbol_iterator_key();
        let iter_fn = if let Some(obj_id) = obj.as_object_id() {
            if let Some(key) = &sym_key {
                let val = match self.get_object_property(obj_id, key, obj) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::UNDEFINED,
                };
                if val.is_undefined() {
                    return Err(self.create_type_error("is not iterable"));
                }
                val
            } else {
                return Err(self.create_type_error("is not iterable"));
            }
        } else if obj.is_string() {
            if let Some(key) = &sym_key {
                let str_proto_id = self.realm().string_prototype;
                if let Some(proto_id) = str_proto_id {
                    let proto_val = JsValue::object(proto_id);
                    let val = match self.get_object_property(proto_id, key, &proto_val) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Err(e),
                        _ => JsValue::UNDEFINED,
                    };
                    if !val.is_undefined() {
                        val
                    } else {
                        return Err(self.create_type_error("is not iterable"));
                    }
                } else {
                    return Err(self.create_type_error("is not iterable"));
                }
            } else {
                return Err(self.create_type_error("is not iterable"));
            }
        } else if let Some(key) = &sym_key {
            let wrapped = match self.to_object(obj) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => return Err(self.create_type_error("is not iterable")),
            };
            if let Some(wrapped_id) = wrapped.as_object_id() {
                let val = match self.get_object_property(wrapped_id, key, &wrapped) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::UNDEFINED,
                };
                if !val.is_nullish() {
                    val
                } else {
                    return Err(self.create_type_error("is not iterable"));
                }
            } else {
                return Err(self.create_type_error("is not iterable"));
            }
        } else {
            return Err(self.create_type_error("is not iterable"));
        };
        match self.call_function(&iter_fn, obj, &[]) {
            Completion::Normal(v) => {
                if let Some(v_id) = v.as_object_id() {
                    let next_method = match self.get_object_property(v_id, "next", &v) {
                        Completion::Normal(n) => n,
                        Completion::Throw(e) => return Err(e),
                        _ => JsValue::UNDEFINED,
                    };
                    self.iterator_next_cache.insert(v_id, next_method);
                    Ok(v)
                } else {
                    Err(self
                        .create_type_error("Result of the Symbol.iterator method is not an object"))
                }
            }
            Completion::Throw(e) => Err(e),
            _ => Err(self.create_type_error("is not iterable")),
        }
    }

    pub(crate) fn get_async_iterator(&mut self, obj: &JsValue) -> Result<JsValue, JsValue> {
        let async_sym_key = self.get_symbol_key("asyncIterator");
        if let Some(key) = &async_sym_key {
            let iter_fn = if let Some(obj_id) = obj.as_object_id() {
                let val = match self.get_object_property(obj_id, key, obj) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::UNDEFINED,
                };
                if !val.is_nullish() { Some(val) } else { None }
            } else {
                None
            };
            if let Some(iter_fn) = iter_fn {
                return match self.call_function(&iter_fn, obj, &[]) {
                    Completion::Normal(v) => {
                        if let Some(v_id) = v.as_object_id() {
                            let next_method = match self.get_object_property(v_id, "next", &v) {
                                Completion::Normal(n) => n,
                                Completion::Throw(e) => return Err(e),
                                _ => JsValue::UNDEFINED,
                            };
                            self.iterator_next_cache.insert(v_id, next_method);
                            Ok(v)
                        } else {
                            Err(self.create_type_error(
                                "Result of the Symbol.asyncIterator method is not an object",
                            ))
                        }
                    }
                    Completion::Throw(e) => Err(e),
                    _ => Err(self.create_type_error("is not async iterable")),
                };
            }
        }
        // Fallback: wrap sync iterator
        let sync_iter = self.get_iterator(obj)?;
        Ok(self.create_async_from_sync_iterator(sync_iter))
    }

    pub(crate) fn create_async_from_sync_iterator(&mut self, sync_iter: JsValue) -> JsValue {
        let wrapper_id = self.create_object_id();
        let cached_next = if let Some(sync_id) = sync_iter.as_object_id() {
            if let Some(cached) = self.iterator_next_cache.get(&sync_id).cloned() {
                cached
            } else {
                match self.get_object_property(sync_id, "next", &sync_iter) {
                    Completion::Normal(v) if !v.is_undefined() => v,
                    _ => JsValue::UNDEFINED,
                }
            }
        } else {
            JsValue::UNDEFINED
        };

        // §27.1.2.1 next()
        let sync_for_next = sync_iter.clone();
        self.define_method(wrapper_id, "next", 1, move |interp, _this, args| {
            let call_args: &[JsValue] = if args.is_empty() {
                &[]
            } else {
                std::slice::from_ref(&args[0])
            };
            let result = match interp.call_function(&cached_next, &sync_for_next, call_args) {
                Completion::Normal(v) if v.is_object() => v,
                Completion::Normal(_) => {
                    let e = interp.create_type_error("Iterator result is not an object");
                    return interp.create_rejected_promise(e);
                }
                Completion::Throw(e) => return interp.create_rejected_promise(e),
                _ => {
                    let e = interp.create_type_error("Iterator next failed");
                    return interp.create_rejected_promise(e);
                }
            };
            // AsyncFromSyncIteratorContinuation(result, promiseCap, syncIterRec, closeOnRejection=true)
            interp.async_from_sync_continuation(result, sync_for_next.clone(), true)
        });

        // §27.1.2.2 return()
        let sync_for_return = sync_iter.clone();
        self.define_method(wrapper_id, "return", 1, move |interp, _this, args| {
            let value = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            let value_present = !args.is_empty();
            if let Some(sync_id) = sync_for_return.as_object_id() {
                // GetMethod(syncIterator, "return")
                let ret_fn = match interp.get_object_property(sync_id, "return", &sync_for_return) {
                    Completion::Normal(v) if interp.is_callable(&v) => Some(v),
                    Completion::Throw(e) => return interp.create_rejected_promise(e),
                    _ => None,
                };
                if let Some(ret_fn) = ret_fn {
                    // §27.1.2.2 step 8-9: pass value only if present
                    let call_args: &[JsValue] = if value_present {
                        std::slice::from_ref(&value)
                    } else {
                        &[]
                    };
                    let return_result =
                        match interp.call_function(&ret_fn, &sync_for_return, call_args) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                return interp.create_rejected_promise(e);
                            }
                            _ => JsValue::UNDEFINED,
                        };
                    if !return_result.is_object() {
                        let e = interp.create_type_error("Iterator result is not an object");
                        return interp.create_rejected_promise(e);
                    }
                    // AsyncFromSyncIteratorContinuation(returnResult, promiseCap, syncIterRec, closeOnRejection=false)
                    interp.async_from_sync_continuation(
                        return_result,
                        sync_for_return.clone(),
                        false,
                    )
                } else {
                    // return is undefined: resolve with {value, done: true}
                    let iter_result = interp.create_iter_result_object(value, true);
                    let promise = interp.create_promise_object();
                    if let Some(promise_id) = promise.as_object_id() {
                        interp.fulfill_promise(promise_id, iter_result);
                    }
                    Completion::Normal(promise)
                }
            } else {
                let iter_result = interp.create_iter_result_object(JsValue::UNDEFINED, true);
                let promise = interp.create_promise_object();
                if let Some(promise_id) = promise.as_object_id() {
                    interp.fulfill_promise(promise_id, iter_result);
                }
                Completion::Normal(promise)
            }
        });

        // §27.1.2.3 throw()
        let sync_for_throw = sync_iter;
        self.define_method(wrapper_id, "throw", 1, move |interp, _this, args| {
            let value = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            if let Some(sync_id) = sync_for_throw.as_object_id() {
                // GetMethod(syncIterator, "throw")
                let throw_method =
                    match interp.get_object_property(sync_id, "throw", &sync_for_throw) {
                        Completion::Normal(v) if interp.is_callable(&v) => Some(v),
                        Completion::Throw(e) => return interp.create_rejected_promise(e),
                        _ => None,
                    };
                if let Some(throw_method) = throw_method {
                    let throw_result =
                        match interp.call_function(&throw_method, &sync_for_throw, &[value]) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return interp.create_rejected_promise(e),
                            _ => JsValue::UNDEFINED,
                        };
                    if !throw_result.is_object() {
                        let e = interp.create_type_error("Iterator result is not an object");
                        return interp.create_rejected_promise(e);
                    }
                    // AsyncFromSyncIteratorContinuation(throwResult, promiseCap, syncIterRec, closeOnRejection=true)
                    interp.async_from_sync_continuation(throw_result, sync_for_throw.clone(), true)
                } else {
                    // §27.1.2.3 step 8: throw is undefined
                    // Close the iterator, then reject with TypeError
                    let close_err = interp.iterator_close_result(&sync_for_throw);
                    if let Err(e) = close_err {
                        return interp.create_rejected_promise(e);
                    }
                    let e =
                        interp.create_type_error("The iterator does not provide a 'throw' method");
                    interp.create_rejected_promise(e)
                }
            } else {
                let e = interp.create_type_error("Iterator is not an object");
                interp.create_rejected_promise(e)
            }
        });

        let id = wrapper_id;
        JsValue::object(id)
    }

    /// §27.1.2.4 AsyncFromSyncIteratorContinuation(result, promiseCap, syncIterRec, closeOnRejection)
    fn async_from_sync_continuation(
        &mut self,
        result: JsValue,
        sync_iter: JsValue,
        close_on_rejection: bool,
    ) -> Completion {
        // Step 1-2: IteratorComplete(result) — get done
        let done = match self.obj_get(&result, "done") {
            Ok(v) => v,
            Err(e) => return self.create_rejected_promise(e),
        };
        // Step 3-4: IteratorValue(result) — get value
        let value = match self.obj_get(&result, "value") {
            Ok(v) => v,
            Err(e) => return self.create_rejected_promise(e),
        };
        let done_bool = done.as_boolean() == Some(true);

        // Step 5: valueWrapper = PromiseResolve(%Promise%, value)
        let promise_ctor = self.get_global_var("Promise").unwrap_or(JsValue::UNDEFINED);
        let value_wrapper = match self.promise_resolve_with_constructor(&promise_ctor, &value) {
            Ok(w) => w,
            Err(e) => {
                // Step 6: If abrupt, !done, closeOnRejection → IteratorClose(syncIterRec, valueWrapper)
                // Per §7.4.7 step 4: since completion is a throw, IteratorClose returns completion
                // regardless of whether return() succeeds or fails (original error takes priority)
                if !done_bool && close_on_rejection {
                    let _ = self.iterator_close_result(&sync_iter);
                }
                // Step 7: IfAbruptRejectPromise
                return self.create_rejected_promise(e);
            }
        };

        // Steps 8-12: Chain .then(onFulfilled, onRejected) on valueWrapper
        let outer_promise = self.create_promise_object();

        // onFulfilled: unwrap resolved value into {value: v, done}
        let outer_clone1 = outer_promise.clone();
        let on_fulfilled = self.create_function(JsFunction::native(
            "".to_string(),
            1,
            move |interp, _this, args| {
                let resolved_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let iter_result = interp.create_iter_result_object(resolved_val, done_bool);
                if let Some(op_id) = outer_clone1.as_object_id() {
                    interp.fulfill_promise(op_id, iter_result);
                }
                Completion::Normal(JsValue::UNDEFINED)
            },
        ));

        // onRejected: if !done && closeOnRejection → close iterator, then reject
        let outer_clone2 = outer_promise.clone();
        let on_rejected = if !done_bool && close_on_rejection {
            let sync_for_close = sync_iter.clone();
            self.create_function(JsFunction::native(
                "".to_string(),
                1,
                move |interp, _this, args| {
                    let err = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let _ = interp.iterator_close_result(&sync_for_close);
                    if let Some(op_id) = outer_clone2.as_object_id() {
                        interp.reject_promise(op_id, err);
                    }
                    Completion::Normal(JsValue::UNDEFINED)
                },
            ))
        } else {
            self.create_function(JsFunction::native(
                "".to_string(),
                1,
                move |interp, _this, args| {
                    let err = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    if let Some(op_id) = outer_clone2.as_object_id() {
                        interp.reject_promise(op_id, err);
                    }
                    Completion::Normal(JsValue::UNDEFINED)
                },
            ))
        };

        let outer_id = outer_promise.as_object_id().unwrap_or(0);
        let (outer_resolve, outer_reject) = self.create_resolving_functions(outer_id);
        let _ = self.perform_promise_then(
            &value_wrapper,
            &on_fulfilled,
            &on_rejected,
            outer_promise.clone(),
            outer_resolve,
            outer_reject,
        );
        Completion::Normal(outer_promise)
    }

    pub(crate) fn iterator_next(&mut self, iterator: &JsValue) -> Result<JsValue, JsValue> {
        if let Some(iter_id) = iterator.as_object_id() {
            let next_fn = if let Some(cached) = self.iterator_next_cache.get(&iter_id).cloned() {
                if cached.is_undefined() {
                    None
                } else {
                    Some(cached)
                }
            } else {
                match self.get_object_property(iter_id, "next", iterator) {
                    Completion::Normal(v) if !v.is_undefined() => Some(v),
                    Completion::Throw(e) => return Err(e),
                    _ => None,
                }
            };
            if let Some(next_fn) = next_fn {
                match self.call_function(&next_fn, iterator, &[]) {
                    Completion::Normal(v) => {
                        if v.is_object() {
                            Ok(v)
                        } else {
                            Err(self.create_type_error("Iterator result is not an object"))
                        }
                    }
                    Completion::Throw(e) => Err(e),
                    _ => Err(self.create_type_error("Iterator next failed")),
                }
            } else {
                Err(self.create_type_error("Iterator does not have a next method"))
            }
        } else {
            Err(self.create_type_error("Iterator is not an object"))
        }
    }

    pub(crate) fn iterator_complete(&mut self, result: &JsValue) -> Result<bool, JsValue> {
        if let Some(result_id) = result.as_object_id() {
            let done = match self.get_object_property(result_id, "done", result) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::UNDEFINED,
            };
            return Ok(self.to_boolean_val(&done));
        }
        Ok(true)
    }

    pub(crate) fn iterator_value(&mut self, result: &JsValue) -> Result<JsValue, JsValue> {
        if let Some(result_id) = result.as_object_id() {
            match self.get_object_property(result_id, "value", result) {
                Completion::Normal(v) => Ok(v),
                Completion::Throw(e) => Err(e),
                _ => Ok(JsValue::UNDEFINED),
            }
        } else {
            Ok(JsValue::UNDEFINED)
        }
    }

    pub(crate) fn iterator_step(&mut self, iterator: &JsValue) -> Result<Option<JsValue>, JsValue> {
        let result = self.iterator_next(iterator)?;
        if self.iterator_complete(&result)? {
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }

    pub(crate) fn iterator_return(
        &mut self,
        iterator: &JsValue,
        value: &JsValue,
    ) -> Result<Option<JsValue>, JsValue> {
        if let Some(iter_id) = iterator.as_object_id() {
            let return_fn = match self.get_object_property(iter_id, "return", iterator) {
                Completion::Normal(v) if v.is_object() => Some(v),
                Completion::Normal(_) => None,
                Completion::Throw(e) => return Err(e),
                _ => None,
            };
            if let Some(return_fn) = return_fn {
                match self.call_function(&return_fn, iterator, std::slice::from_ref(value)) {
                    Completion::Normal(v) => {
                        if v.is_object() {
                            Ok(Some(v))
                        } else {
                            Err(self.create_type_error("Iterator return result is not an object"))
                        }
                    }
                    Completion::Throw(e) => Err(e),
                    _ => Err(self.create_type_error("Iterator return failed")),
                }
            } else {
                Ok(None)
            }
        } else {
            Err(self.create_type_error("Iterator is not an object"))
        }
    }

    pub(crate) fn iterator_throw(
        &mut self,
        iterator: &JsValue,
        exception: &JsValue,
    ) -> Result<Option<JsValue>, JsValue> {
        if let Some(iter_id) = iterator.as_object_id() {
            let throw_fn = match self.get_object_property(iter_id, "throw", iterator) {
                Completion::Normal(v) if v.is_object() => Some(v),
                Completion::Normal(_) => None,
                Completion::Throw(e) => return Err(e),
                _ => None,
            };
            if let Some(throw_fn) = throw_fn {
                match self.call_function(&throw_fn, iterator, std::slice::from_ref(exception)) {
                    Completion::Normal(v) => {
                        if v.is_object() {
                            Ok(Some(v))
                        } else {
                            Err(self.create_type_error("Iterator throw result is not an object"))
                        }
                    }
                    Completion::Throw(e) => Err(e),
                    _ => Err(self.create_type_error("Iterator throw failed")),
                }
            } else {
                Ok(None)
            }
        } else {
            Err(self.create_type_error("Iterator is not an object"))
        }
    }

    /// IteratorClose per §7.4.6 - called during abrupt completion (e.g., break/throw in for-of).
    /// The original completion takes priority over errors from return().
    pub(crate) fn iterator_close(&mut self, iterator: &JsValue, _completion: JsValue) -> JsValue {
        // Note (issue #242): when a for-of *body* calls `__host_exit`, its
        // `Completion::Exit` takes the loop's `other` arm and never reaches
        // here, so the iterator's `return()` is not run. This path only runs
        // when the loop is unwinding a genuine throw/break; if `return()` then
        // itself calls `__host_exit`, that boundary returns a `JsValue` and so
        // cannot carry the exit — record it in the terminal `pending_exit`
        // sink instead. Inert unless the node host floor is enabled.
        if let Some(iter_id) = iterator.as_object_id() {
            // GetMethod(iterator, "return"): undefined/null → no-op, non-callable → TypeError
            let return_val = match self.get_object_property(iter_id, "return", iterator) {
                Completion::Normal(v) => v,
                Completion::Throw(_e) => return _completion, // original completion takes priority
                _ => return _completion,
            };
            if return_val.is_undefined() || return_val.is_null() {
                return _completion;
            }
            if !self.is_callable(&return_val) {
                // Non-callable return: throw TypeError, but original completion takes priority
                return _completion;
            }
            // Call return(), but original completion takes priority over errors
            if let Completion::Exit(code) = self.call_function(&return_val, iterator, &[]) {
                self.pending_exit = Some(code);
            }
        }
        _completion
    }

    /// IteratorClose for normal completion paths (no abrupt completion to prioritize).
    pub(crate) fn iterator_close_result(&mut self, iterator: &JsValue) -> Result<(), JsValue> {
        // See `iterator_close` (issue #242): only reached when the loop is not
        // unwinding a body `Completion::Exit`. If `return()` itself calls
        // `__host_exit`, record it in the terminal sink — this `Result`-typed
        // boundary cannot carry a `Completion::Exit`. Inert off-path.
        if let Some(iter_id) = iterator.as_object_id() {
            // GetMethod(iterator, "return"): undefined/null → no-op, non-callable → TypeError
            let return_val = match self.get_object_property(iter_id, "return", iterator) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => return Ok(()),
            };
            if return_val.is_undefined() || return_val.is_null() {
                return Ok(());
            }
            if !self.is_callable(&return_val) {
                return Err(self.create_type_error("iterator.return is not a function"));
            }
            match self.call_function(&return_val, iterator, &[]) {
                Completion::Normal(inner_result) if !inner_result.is_object() => {
                    return Err(self.create_type_error("Iterator result is not an object"));
                }
                Completion::Throw(e) => return Err(e),
                Completion::Exit(code) => self.pending_exit = Some(code),
                _ => {}
            }
        }
        Ok(())
    }

    fn iterator_step_direct(
        &mut self,
        iterator: &JsValue,
        next_method: &JsValue,
    ) -> Result<Option<JsValue>, JsValue> {
        match self.call_function(next_method, iterator, &[]) {
            Completion::Normal(result) => {
                if !result.is_object() {
                    return Err(self.create_type_error("Iterator result is not an object"));
                }
                if self.iterator_complete(&result)? {
                    Ok(None)
                } else {
                    Ok(Some(result))
                }
            }
            Completion::Throw(e) => Err(e),
            _ => Err(self.create_type_error("Iterator next failed")),
        }
    }

    pub(crate) fn iterate_to_vec(&mut self, iterable: &JsValue) -> Result<Vec<JsValue>, JsValue> {
        let gc_frame = self.gc_root_frame();
        self.gc_root_value(iterable);
        let result = (|| {
            let iterator = self.get_iterator(iterable)?;
            self.gc_root_value(&iterator);
            let mut values = Vec::new();
            loop {
                let iterator_result = self.iterator_next(&iterator)?;
                self.gc_root_value(&iterator_result);
                if self.iterator_complete(&iterator_result)? {
                    break;
                }
                let value = self.iterator_value(&iterator_result)?;
                self.gc_unroot_value(&iterator_result);
                self.gc_root_value(&value);
                values.push(value);
            }
            Ok(values)
        })();
        self.gc_unroot_frame(gc_frame);
        result
    }

    // CreateListFromArrayLike (§7.3.18)
    pub(crate) fn create_list_from_array_like(
        &mut self,
        obj: &JsValue,
    ) -> Result<Vec<JsValue>, JsValue> {
        let Some(obj_id) = obj.as_object_id() else {
            return Err(self.create_type_error("CreateListFromArrayLike called on non-object"));
        };
        let len_val = match self.get_object_property(obj_id, "length", obj) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => JsValue::UNDEFINED,
        };
        let n = self.to_number_value(&len_val)?;
        let len = if n.is_nan() || n <= 0.0 {
            0u64
        } else {
            (n.min(9007199254740991.0).floor()) as u64
        };
        let mut list = Vec::with_capacity(len.min(65536) as usize);
        for i in 0..len {
            let index_name = i.to_string();
            let next = match self.get_object_property(obj_id, &index_name, obj) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::UNDEFINED,
            };
            list.push(next);
        }
        Ok(list)
    }
}
