use super::super::*;
use std::collections::HashMap;

// GetIteratorDirect that uses get_object_property (invokes getters/Proxy traps)
fn get_iterator_direct_getter(
    interp: &mut Interpreter,
    obj: &JsValue,
) -> Result<(JsValue, JsValue), JsValue> {
    match obj {
        JsValue::Object(o) => {
            let next_method = match interp.get_object_property(o.id, "next", obj) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::Undefined,
            };
            Ok((obj.clone(), next_method))
        }
        _ => Err(interp.create_type_error("Iterator is not an object")),
    }
}

// IteratorClose that uses get_object_property for .return (invokes getters)
fn iterator_close_getter(interp: &mut Interpreter, iterator: &JsValue) -> Result<(), JsValue> {
    if let JsValue::Object(io) = iterator {
        let return_method = match interp.get_object_property(io.id, "return", iterator) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => return Ok(()),
        };
        if matches!(return_method, JsValue::Undefined | JsValue::Null) {
            return Ok(());
        }
        match interp.call_function(&return_method, iterator, &[]) {
            Completion::Normal(inner_result) => {
                if !matches!(inner_result, JsValue::Object(_)) {
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

// GetIteratorFlattenable(obj, primitiveHandling) per spec
// primitiveHandling is either "reject-primitives" or "iterate-strings"
fn get_iterator_flattenable(
    interp: &mut Interpreter,
    obj: &JsValue,
    reject_primitives: bool,
) -> Result<(JsValue, JsValue), JsValue> {
    if !matches!(obj, JsValue::Object(_)) {
        if reject_primitives {
            return Err(
                interp.create_type_error("Iterator.prototype.flatMap mapper returned a non-object")
            );
        }
        // iterate-strings mode: handle string primitives
        if matches!(obj, JsValue::String(_)) {
            match interp.get_iterator(obj) {
                Ok(iter) => return get_iterator_direct_getter(interp, &iter),
                Err(e) => return Err(e),
            }
        }
        return Err(interp.create_type_error("value is not an object"));
    }

    // Get @@iterator method
    let sym_key = interp.get_symbol_iterator_key();
    let iter_method = if let JsValue::Object(o) = obj {
        if let Some(ref key) = sym_key {
            match interp.get_object_property(o.id, key, obj) {
                Completion::Normal(v) => Some(v),
                Completion::Throw(e) => return Err(e),
                _ => Some(JsValue::Undefined),
            }
        } else {
            Some(JsValue::Undefined)
        }
    } else {
        Some(JsValue::Undefined)
    };

    if let Some(method) = iter_method
        && !matches!(method, JsValue::Undefined | JsValue::Null)
    {
        // Has @@iterator - check it's callable and call it
        if let JsValue::Object(mo) = &method {
            if !interp
                .get_object(mo.id)
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
        if !matches!(iter_obj, JsValue::Object(_)) {
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
    let method = match obj {
        JsValue::Object(o) => {
            if let Some(ref key) = sym_key {
                match interp.get_object_property(o.id, key, obj) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::Undefined,
                }
            } else {
                return Err(interp.create_type_error("is not iterable"));
            }
        }
        JsValue::String(_) => {
            // For strings, use the string prototype's @@iterator
            return match interp.get_iterator(obj) {
                Ok(iter) => get_iterator_direct_getter(interp, &iter),
                Err(e) => Err(e),
            };
        }
        _ => return Err(interp.create_type_error("is not iterable")),
    };
    if matches!(method, JsValue::Undefined | JsValue::Null) {
        return Err(interp.create_type_error("is not iterable"));
    }
    // Call the method
    let iterator = match interp.call_function(&method, obj, &[]) {
        Completion::Normal(v) => v,
        Completion::Throw(e) => return Err(e),
        _ => return Err(interp.create_type_error("Symbol.iterator did not return a value")),
    };
    if !matches!(iterator, JsValue::Object(_)) {
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
    let obj_ref = match &result {
        JsValue::Object(o) => o,
        _ => return Err(interp.create_type_error("Iterator result is not an object")),
    };
    // Read .done via getter
    let done = match interp.get_object_property(obj_ref.id, "done", &result) {
        Completion::Normal(v) => v,
        Completion::Throw(e) => return Err(e),
        _ => JsValue::Undefined,
    };
    if to_boolean(&done) {
        return Ok(None);
    }
    // Read .value via getter
    let value = match interp.get_object_property(obj_ref.id, "value", &result) {
        Completion::Normal(v) => v,
        Completion::Throw(e) => return Err(e),
        _ => JsValue::Undefined,
    };
    Ok(Some(value))
}

// IteratorClose per spec, taking a completion and returning updated completion.
// If completion is Err (throw), the original error is preserved even if .return() throws.
fn iterator_close_with_completion(
    interp: &mut Interpreter,
    iterator: &JsValue,
    completion: Result<(), JsValue>,
) -> Result<(), JsValue> {
    if let JsValue::Object(io) = iterator {
        let return_method = match interp.get_object_property(io.id, "return", iterator) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => {
                // Step 5: If completion is a throw completion, return ? completion.
                if let Err(orig) = completion {
                    return Err(orig);
                }
                return Err(e);
            }
            _ => JsValue::Undefined,
        };
        if matches!(return_method, JsValue::Undefined | JsValue::Null) {
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
                if !matches!(v, JsValue::Object(_)) {
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
        let iter_proto = self.create_object();
        iter_proto.borrow_mut().class_name = "Iterator".to_string();

        // %IteratorPrototype%[@@iterator]() returns this
        let iter_self_fn = self.create_function(JsFunction::native(
            "[Symbol.iterator]".to_string(),
            0,
            |_interp, this, _args| Completion::Normal(this.clone()),
        ));
        if let Some(key) = self.get_symbol_iterator_key() {
            iter_proto.borrow_mut().insert_property(
                key,
                PropertyDescriptor::data(iter_self_fn, true, false, true),
            );
        }
        // @@toStringTag on %IteratorPrototype% — accessor property per spec
        {
            let iter_proto_id = iter_proto.borrow().id.unwrap();
            let tst_getter = self.create_function(JsFunction::native(
                "get [Symbol.toStringTag]".to_string(),
                0,
                |_interp, _this, _args| {
                    Completion::Normal(JsValue::String(JsString::from_str("Iterator")))
                },
            ));
            let ip_id = iter_proto_id;
            let tst_key_for_setter = self.get_symbol_key("toStringTag");
            let tst_setter = self.create_function(JsFunction::native(
                "set [Symbol.toStringTag]".to_string(),
                1,
                move |interp, this, args| {
                    let v = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let this_id = match this {
                        JsValue::Object(o) => o.id,
                        _ => {
                            let err = interp.create_type_error("setter called on non-object");
                            return Completion::Throw(err);
                        }
                    };
                    if this_id == ip_id {
                        let err = interp.create_type_error(
                            "Cannot set Symbol.toStringTag on Iterator.prototype",
                        );
                        return Completion::Throw(err);
                    }
                    let prop_key = tst_key_for_setter
                        .clone()
                        .unwrap_or_else(|| "Symbol(Symbol.toStringTag)".to_string());
                    let has_own = if let Some(od) = interp.get_object(this_id) {
                        od.borrow().properties.contains_key(&prop_key)
                    } else {
                        false
                    };
                    if !has_own {
                        if let Some(od) = interp.get_object(this_id) {
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
                        if let Some(od) = interp.get_object(this_id) {
                            if !od.borrow_mut().set_property_value(&prop_key, v) {
                                let err = interp.create_type_error("Cannot set property");
                                return Completion::Throw(err);
                            }
                        }
                    }
                    Completion::Normal(JsValue::Undefined)
                },
            ));
            iter_proto.borrow_mut().insert_property(
                "Symbol(Symbol.toStringTag)".to_string(),
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
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id) {
                        let return_method = obj.borrow().get_property("return");
                        if matches!(&return_method, JsValue::Object(ro) if interp.get_object(ro.id).map(|od| od.borrow().callable.is_some()).unwrap_or(false))
                        {
                            return interp.call_function(&return_method, this, &[]);
                        }
                    }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        if let Some(key) = self.get_symbol_key("dispose") {
            iter_proto
                .borrow_mut()
                .insert_property(key, PropertyDescriptor::data(dispose_fn, true, false, true));
        }

        self.iterator_prototype = Some(iter_proto.clone());

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
                if let Some(JsValue::Object(nt)) = &interp.new_target {
                    // Check if new.target is the Iterator constructor by checking if
                    // looking up "Iterator" from global gives the same object
                    let global_iter = interp.global_env.borrow().get("Iterator");
                    if let Some(JsValue::Object(gi)) = global_iter
                        && gi.id == nt.id
                    {
                        let err = interp.create_type_error(
                            "Abstract class Iterator not directly constructable",
                        );
                        return Completion::Throw(err);
                    }
                }
                Completion::Normal(this.clone())
            },
        ));

        // Set Iterator.prototype
        if let JsValue::Object(ctor_obj) = &iterator_ctor
            && let Some(obj) = self.get_object(ctor_obj.id)
        {
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(
                    JsValue::Object(crate::types::JsObject {
                        id: iter_proto.borrow().id.unwrap(),
                    }),
                    false,
                    false,
                    false,
                ),
            );
        }

        // Set %IteratorPrototype%.constructor as accessor property per spec
        // Getter returns Iterator, setter implements SetterThatIgnoresPrototypeProperties
        {
            let iter_proto_id = iter_proto.borrow().id.unwrap();
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
                    let v = args.first().cloned().unwrap_or(JsValue::Undefined);
                    // Step 1: If this is not an Object, throw TypeError
                    let this_id = match this {
                        JsValue::Object(o) => o.id,
                        _ => {
                            let err = interp.create_type_error("setter called on non-object");
                            return Completion::Throw(err);
                        }
                    };
                    // Step 2: If this is home (Iterator.prototype), throw TypeError
                    if this_id == ip_id {
                        let err = interp
                            .create_type_error("Cannot set constructor on Iterator.prototype");
                        return Completion::Throw(err);
                    }
                    // Step 3: Check if this has own "constructor" property
                    let has_own = if let Some(od) = interp.get_object(this_id) {
                        od.borrow().properties.contains_key("constructor")
                    } else {
                        false
                    };
                    if !has_own {
                        // CreateDataPropertyOrThrow(this, "constructor", v)
                        if let Some(od) = interp.get_object(this_id) {
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
                        if let Some(od) = interp.get_object(this_id) {
                            if !od.borrow_mut().set_property_value("constructor", v) {
                                let err =
                                    interp.create_type_error("Cannot set property constructor");
                                return Completion::Throw(err);
                            }
                        }
                    }
                    Completion::Normal(JsValue::Undefined)
                },
            ));
            iter_proto.borrow_mut().insert_property(
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
        self.global_env
            .borrow_mut()
            .declare("Iterator", BindingKind::Var);
        let _ = self
            .global_env
            .borrow_mut()
            .set("Iterator", iterator_ctor.clone());

        // Setup consuming and lazy helper methods on %IteratorPrototype%
        self.setup_iterator_helper_methods(&iter_proto);

        // Setup static methods on Iterator constructor
        self.setup_iterator_static_methods(&iterator_ctor);

        // %ArrayIteratorPrototype% (§23.1.5.1)
        let arr_iter_proto = self.create_object();
        arr_iter_proto.borrow_mut().prototype = Some(iter_proto.clone());
        arr_iter_proto.borrow_mut().class_name = "Array Iterator".to_string();

        let arr_iter_next = self.create_function(JsFunction::native(
            "next".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this {
                    if let Some(obj) = interp.get_object(o.id) {
                        let state = obj.borrow().iterator_state.clone();
                        if let Some(IteratorState::ArrayIterator {
                            array_id,
                            index,
                            kind,
                            done,
                        }) = state
                        {
                            if done {
                                return Completion::Normal(
                                    interp.create_iter_result_object(JsValue::Undefined, true),
                                );
                            }
                            let (len, val) = if let Some(arr_obj) = interp.get_object(array_id) {
                                let borrowed = arr_obj.borrow();
                                let len = borrowed
                                    .array_elements
                                    .as_ref()
                                    .map(|e| e.len())
                                    .unwrap_or_else(|| {
                                        if let Some(JsValue::Number(n)) =
                                            borrowed.get_property_value("length")
                                        {
                                            n as usize
                                        } else {
                                            0
                                        }
                                    });
                                if index >= len {
                                    (len, None)
                                } else {
                                    let v = match kind {
                                        IteratorKind::Key => JsValue::Number(index as f64),
                                        IteratorKind::Value => borrowed
                                            .array_elements
                                            .as_ref()
                                            .and_then(|e| e.get(index).cloned())
                                            .unwrap_or_else(|| {
                                                borrowed.get_property(&index.to_string())
                                            }),
                                        IteratorKind::KeyValue => {
                                            let elem = borrowed
                                                .array_elements
                                                .as_ref()
                                                .and_then(|e| e.get(index).cloned())
                                                .unwrap_or_else(|| {
                                                    borrowed.get_property(&index.to_string())
                                                });
                                            drop(borrowed);
                                            let pair = interp.create_array(vec![
                                                JsValue::Number(index as f64),
                                                elem,
                                            ]);
                                            return {
                                                obj.borrow_mut().iterator_state =
                                                    Some(IteratorState::ArrayIterator {
                                                        array_id,
                                                        index: index + 1,
                                                        kind,
                                                        done: false,
                                                    });
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
                                    obj.borrow_mut().iterator_state =
                                        Some(IteratorState::ArrayIterator {
                                            array_id,
                                            index: index + 1,
                                            kind,
                                            done: false,
                                        });
                                    Completion::Normal(interp.create_iter_result_object(v, false))
                                }
                                None => {
                                    obj.borrow_mut().iterator_state =
                                        Some(IteratorState::ArrayIterator {
                                            array_id,
                                            index: len,
                                            kind,
                                            done: true,
                                        });
                                    Completion::Normal(
                                        interp.create_iter_result_object(JsValue::Undefined, true),
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
                                    interp.create_iter_result_object(JsValue::Undefined, true),
                                );
                            }
                            let ta_obj = interp.get_object(typed_array_id);
                            if ta_obj.is_none() {
                                obj.borrow_mut().iterator_state =
                                    Some(IteratorState::TypedArrayIterator {
                                        typed_array_id,
                                        index,
                                        kind,
                                        done: true,
                                    });
                                return Completion::Normal(
                                    interp.create_iter_result_object(JsValue::Undefined, true),
                                );
                            }
                            let ta_obj = ta_obj.unwrap();
                            let ta_info = ta_obj.borrow().typed_array_info.clone();
                            if let Some(ref ta) = ta_info {
                                if ta.is_detached.get() {
                                    return Completion::Throw(
                                        interp.create_type_error("typed array is detached"),
                                    );
                                }
                                if is_typed_array_out_of_bounds(ta) {
                                    obj.borrow_mut().iterator_state =
                                        Some(IteratorState::TypedArrayIterator {
                                            typed_array_id,
                                            index,
                                            kind,
                                            done: true,
                                        });
                                    return Completion::Normal(
                                        interp.create_iter_result_object(JsValue::Undefined, true),
                                    );
                                }
                                let len = typed_array_length(ta);
                                if index >= len {
                                    obj.borrow_mut().iterator_state =
                                        Some(IteratorState::TypedArrayIterator {
                                            typed_array_id,
                                            index,
                                            kind,
                                            done: true,
                                        });
                                    return Completion::Normal(
                                        interp.create_iter_result_object(JsValue::Undefined, true),
                                    );
                                }
                                let v = match kind {
                                    IteratorKind::Key => JsValue::Number(index as f64),
                                    IteratorKind::Value => typed_array_get_index(ta, index),
                                    IteratorKind::KeyValue => {
                                        let elem = typed_array_get_index(ta, index);
                                        let pair = interp.create_array(vec![
                                            JsValue::Number(index as f64),
                                            elem,
                                        ]);
                                        obj.borrow_mut().iterator_state =
                                            Some(IteratorState::TypedArrayIterator {
                                                typed_array_id,
                                                index: index + 1,
                                                kind,
                                                done: false,
                                            });
                                        return Completion::Normal(
                                            interp.create_iter_result_object(pair, false),
                                        );
                                    }
                                };
                                obj.borrow_mut().iterator_state =
                                    Some(IteratorState::TypedArrayIterator {
                                        typed_array_id,
                                        index: index + 1,
                                        kind,
                                        done: false,
                                    });
                                Completion::Normal(interp.create_iter_result_object(v, false))
                            } else {
                                Completion::Throw(
                                    interp.create_type_error("not a TypedArray"),
                                )
                            }
                        } else {
                            let err = interp.create_type_error("next called on non-array iterator");
                            Completion::Throw(err)
                        }
                    } else {
                        Completion::Normal(JsValue::Undefined)
                    }
                } else {
                    let err = interp.create_type_error("next called on non-object");
                    Completion::Throw(err)
                }
            },
        ));
        arr_iter_proto
            .borrow_mut()
            .insert_builtin("next".to_string(), arr_iter_next);

        // Set @@toStringTag
        arr_iter_proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str("Array Iterator")),
                false,
                false,
                true,
            ),
        );

        self.array_iterator_prototype = Some(arr_iter_proto);

        // %StringIteratorPrototype% (§22.1.5.1)
        let str_iter_proto = self.create_object();
        str_iter_proto.borrow_mut().prototype = Some(iter_proto.clone());
        str_iter_proto.borrow_mut().class_name = "String Iterator".to_string();

        let str_iter_next = self.create_function(JsFunction::native(
            "next".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this {
                    if let Some(obj) = interp.get_object(o.id) {
                        let state = obj.borrow().iterator_state.clone();
                        if let Some(IteratorState::StringIterator {
                            ref string,
                            position,
                            done,
                        }) = state
                        {
                            if done || position >= string.code_units.len() {
                                obj.borrow_mut().iterator_state =
                                    Some(IteratorState::StringIterator {
                                        string: string.clone(),
                                        position,
                                        done: true,
                                    });
                                return Completion::Normal(
                                    interp.create_iter_result_object(JsValue::Undefined, true),
                                );
                            }
                            let cu = string.code_units[position];
                            // Handle surrogate pairs for full Unicode code points
                            let (result_str, advance) = if (0xD800..=0xDBFF).contains(&cu)
                                && position + 1 < string.code_units.len()
                            {
                                let next_cu = string.code_units[position + 1];
                                if (0xDC00..=0xDFFF).contains(&next_cu) {
                                    let s = String::from_utf16_lossy(
                                        &string.code_units[position..position + 2],
                                    );
                                    (s, 2)
                                } else {
                                    (String::from_utf16_lossy(&[cu]), 1)
                                }
                            } else {
                                (String::from_utf16_lossy(&[cu]), 1)
                            };
                            obj.borrow_mut().iterator_state = Some(IteratorState::StringIterator {
                                string: string.clone(),
                                position: position + advance,
                                done: false,
                            });
                            Completion::Normal(interp.create_iter_result_object(
                                JsValue::String(JsString::from_str(&result_str)),
                                false,
                            ))
                        } else {
                            let err =
                                interp.create_type_error("next called on non-string iterator");
                            Completion::Throw(err)
                        }
                    } else {
                        Completion::Normal(JsValue::Undefined)
                    }
                } else {
                    let err = interp.create_type_error("next called on non-object");
                    Completion::Throw(err)
                }
            },
        ));
        str_iter_proto
            .borrow_mut()
            .insert_builtin("next".to_string(), str_iter_next);

        str_iter_proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str("String Iterator")),
                false,
                false,
                true,
            ),
        );

        self.string_iterator_prototype = Some(str_iter_proto);
    }

    fn create_iterator_helper_object(&mut self, next_fn: JsValue, return_fn: JsValue) -> JsValue {
        let obj = self.create_object();
        obj.borrow_mut().prototype = self.iterator_prototype.clone();
        obj.borrow_mut().class_name = "Iterator Helper".to_string();
        obj.borrow_mut().insert_builtin("next".to_string(), next_fn);
        obj.borrow_mut()
            .insert_builtin("return".to_string(), return_fn);
        // Add @@iterator returning this
        let iter_self_fn = self.create_function(JsFunction::native(
            "[Symbol.iterator]".to_string(),
            0,
            |_interp, this, _args| Completion::Normal(this.clone()),
        ));
        if let Some(key) = self.get_symbol_iterator_key() {
            obj.borrow_mut().insert_property(
                key,
                PropertyDescriptor::data(iter_self_fn, true, false, true),
            );
        }
        // Add @@toStringTag
        obj.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str("Iterator Helper")),
                false,
                false,
                true,
            ),
        );
        let id = obj.borrow().id.unwrap();
        JsValue::Object(crate::types::JsObject { id })
    }

    fn setup_iterator_helper_methods(&mut self, iter_proto: &Rc<RefCell<JsObjectData>>) {
        // toArray()
        let to_array_fn = self.create_function(JsFunction::native(
            "toArray".to_string(),
            0,
            |interp, this, _args| {
                let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let mut values = Vec::new();
                loop {
                    match interp.iterator_step_direct(&iter, &next_method) {
                        Ok(Some(result)) => match interp.iterator_value(&result) {
                            Ok(v) => values.push(v),
                            Err(e) => {
                                let _ = iterator_close_getter(interp, &iter);
                                return Completion::Throw(e);
                            }
                        },
                        Ok(None) => break,
                        Err(e) => {
                            let _ = iterator_close_getter(interp, &iter);
                            return Completion::Throw(e);
                        }
                    }
                }
                let arr = interp.create_array(values);
                Completion::Normal(arr)
            },
        ));
        iter_proto
            .borrow_mut()
            .insert_builtin("toArray".to_string(), to_array_fn);

        // forEach(fn)
        let for_each_fn = self.create_function(JsFunction::native(
            "forEach".to_string(),
            1,
            |interp, this, args| {
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(&callback, JsValue::Object(o) if interp.get_object(o.id).map(|od| od.borrow().callable.is_some()).unwrap_or(false))
                {
                    let _ = iterator_close_getter(interp, this);
                    let err = interp.create_type_error("callback is not a function");
                    return Completion::Throw(err);
                }
                let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let mut counter = 0.0;
                loop {
                    match interp.iterator_step_direct(&iter, &next_method) {
                        Ok(Some(result)) => {
                            let value = match interp.iterator_value(&result) {
                                    Ok(v) => v,
                                    Err(e) => return Completion::Throw(e),
                                };
                            if let Completion::Throw(e) = interp.call_function(
                                &callback,
                                &JsValue::Undefined,
                                &[value, JsValue::Number(counter)],
                            ) {
                                let _ = iterator_close_getter(interp, &iter);
                                return Completion::Throw(e);
                            }
                            counter += 1.0;
                        }
                        Ok(None) => break,
                        Err(e) => {
                            let _ = iterator_close_getter(interp, &iter);
                            return Completion::Throw(e);
                        }
                    }
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        iter_proto
            .borrow_mut()
            .insert_builtin("forEach".to_string(), for_each_fn);

        // some(predicate)
        let some_fn = self.create_function(JsFunction::native(
            "some".to_string(),
            1,
            |interp, this, args| {
                let predicate = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(&predicate, JsValue::Object(o) if interp.get_object(o.id).map(|od| od.borrow().callable.is_some()).unwrap_or(false))
                {
                    let _ = iterator_close_getter(interp, this);
                    let err = interp.create_type_error("predicate is not a function");
                    return Completion::Throw(err);
                }
                let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let mut counter = 0.0;
                loop {
                    match interp.iterator_step_direct(&iter, &next_method) {
                        Ok(Some(result)) => {
                            let value = match interp.iterator_value(&result) {
                                    Ok(v) => v,
                                    Err(e) => return Completion::Throw(e),
                                };
                            match interp.call_function(
                                &predicate,
                                &JsValue::Undefined,
                                &[value, JsValue::Number(counter)],
                            ) {
                                Completion::Normal(v) => {
                                    if to_boolean(&v) {
                                        // Propagate IteratorClose errors
                                        if let Err(e) = iterator_close_getter(interp, &iter) {
                                            return Completion::Throw(e);
                                        }
                                        return Completion::Normal(JsValue::Boolean(true));
                                    }
                                }
                                Completion::Throw(e) => {
                                    let _ = iterator_close_with_completion(interp, &iter, Err(e.clone()));
                                    return Completion::Throw(e);
                                }
                                _ => {}
                            }
                            counter += 1.0;
                        }
                        Ok(None) => return Completion::Normal(JsValue::Boolean(false)),
                        Err(e) => {
                            let _ = iterator_close_getter(interp, &iter);
                            return Completion::Throw(e);
                        }
                    }
                }
            },
        ));
        iter_proto
            .borrow_mut()
            .insert_builtin("some".to_string(), some_fn);

        // every(predicate)
        let every_fn = self.create_function(JsFunction::native(
            "every".to_string(),
            1,
            |interp, this, args| {
                let predicate = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(&predicate, JsValue::Object(o) if interp.get_object(o.id).map(|od| od.borrow().callable.is_some()).unwrap_or(false))
                {
                    let _ = iterator_close_getter(interp, this);
                    let err = interp.create_type_error("predicate is not a function");
                    return Completion::Throw(err);
                }
                let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let mut counter = 0.0;
                loop {
                    match interp.iterator_step_direct(&iter, &next_method) {
                        Ok(Some(result)) => {
                            let value = match interp.iterator_value(&result) {
                                    Ok(v) => v,
                                    Err(e) => return Completion::Throw(e),
                                };
                            match interp.call_function(
                                &predicate,
                                &JsValue::Undefined,
                                &[value, JsValue::Number(counter)],
                            ) {
                                Completion::Normal(v) => {
                                    if !to_boolean(&v) {
                                        if let Err(e) = iterator_close_getter(interp, &iter) {
                                            return Completion::Throw(e);
                                        }
                                        return Completion::Normal(JsValue::Boolean(false));
                                    }
                                }
                                Completion::Throw(e) => {
                                    let _ = iterator_close_with_completion(interp, &iter, Err(e.clone()));
                                    return Completion::Throw(e);
                                }
                                _ => {}
                            }
                            counter += 1.0;
                        }
                        Ok(None) => return Completion::Normal(JsValue::Boolean(true)),
                        Err(e) => {
                            let _ = iterator_close_getter(interp, &iter);
                            return Completion::Throw(e);
                        }
                    }
                }
            },
        ));
        iter_proto
            .borrow_mut()
            .insert_builtin("every".to_string(), every_fn);

        // find(predicate)
        let find_fn = self.create_function(JsFunction::native(
            "find".to_string(),
            1,
            |interp, this, args| {
                let predicate = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(&predicate, JsValue::Object(o) if interp.get_object(o.id).map(|od| od.borrow().callable.is_some()).unwrap_or(false))
                {
                    let _ = iterator_close_getter(interp, this);
                    let err = interp.create_type_error("predicate is not a function");
                    return Completion::Throw(err);
                }
                let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let mut counter = 0.0;
                loop {
                    match interp.iterator_step_direct(&iter, &next_method) {
                        Ok(Some(result)) => {
                            let value = match interp.iterator_value(&result) {
                                    Ok(v) => v,
                                    Err(e) => return Completion::Throw(e),
                                };
                            match interp.call_function(
                                &predicate,
                                &JsValue::Undefined,
                                &[value.clone(), JsValue::Number(counter)],
                            ) {
                                Completion::Normal(v) => {
                                    if to_boolean(&v) {
                                        if let Err(e) = iterator_close_getter(interp, &iter) {
                                            return Completion::Throw(e);
                                        }
                                        return Completion::Normal(value);
                                    }
                                }
                                Completion::Throw(e) => {
                                    let _ = iterator_close_with_completion(interp, &iter, Err(e.clone()));
                                    return Completion::Throw(e);
                                }
                                _ => {}
                            }
                            counter += 1.0;
                        }
                        Ok(None) => return Completion::Normal(JsValue::Undefined),
                        Err(e) => {
                            let _ = iterator_close_getter(interp, &iter);
                            return Completion::Throw(e);
                        }
                    }
                }
            },
        ));
        iter_proto
            .borrow_mut()
            .insert_builtin("find".to_string(), find_fn);

        // reduce(reducer, [initial])
        let reduce_fn = self.create_function(JsFunction::native(
            "reduce".to_string(),
            1,
            |interp, this, args| {
                let reducer = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(&reducer, JsValue::Object(o) if interp.get_object(o.id).map(|od| od.borrow().callable.is_some()).unwrap_or(false))
                {
                    let _ = iterator_close_getter(interp, this);
                    let err = interp.create_type_error("reducer is not a function");
                    return Completion::Throw(err);
                }
                let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let mut accumulator;
                let mut counter;
                if args.len() >= 2 {
                    accumulator = args[1].clone();
                    counter = 0.0;
                } else {
                    match interp.iterator_step_direct(&iter, &next_method) {
                        Ok(Some(result)) => {
                            accumulator = match interp.iterator_value(&result) {
                                Ok(v) => v,
                                Err(e) => return Completion::Throw(e),
                            };
                            counter = 1.0;
                        }
                        Ok(None) => {
                            let err =
                                interp.create_type_error("Reduce of empty iterator with no initial value");
                            return Completion::Throw(err);
                        }
                        Err(e) => {
                            let _ = iterator_close_getter(interp, &iter);
                            return Completion::Throw(e);
                        }
                    }
                }
                loop {
                    match interp.iterator_step_direct(&iter, &next_method) {
                        Ok(Some(result)) => {
                            let value = match interp.iterator_value(&result) {
                                    Ok(v) => v,
                                    Err(e) => return Completion::Throw(e),
                                };
                            match interp.call_function(
                                &reducer,
                                &JsValue::Undefined,
                                &[accumulator.clone(), value, JsValue::Number(counter)],
                            ) {
                                Completion::Normal(v) => accumulator = v,
                                Completion::Throw(e) => {
                                    let _ = iterator_close_getter(interp, &iter);
                                    return Completion::Throw(e);
                                }
                                _ => {}
                            }
                            counter += 1.0;
                        }
                        Ok(None) => return Completion::Normal(accumulator),
                        Err(e) => {
                            let _ = iterator_close_getter(interp, &iter);
                            return Completion::Throw(e);
                        }
                    }
                }
            },
        ));
        iter_proto
            .borrow_mut()
            .insert_builtin("reduce".to_string(), reduce_fn);

        // Lazy helpers: map, filter, take, drop, flatMap
        self.setup_iterator_lazy_helpers(iter_proto);
    }

    fn setup_iterator_lazy_helpers(&mut self, iter_proto: &Rc<RefCell<JsObjectData>>) {
        // map(mapper)
        let map_fn = self.create_function(JsFunction::native(
            "map".to_string(),
            1,
            |interp, this, args| {
                // Step 1-2: Require this to be an object
                if !matches!(this, JsValue::Object(_)) {
                    let err = interp.create_type_error("Iterator.prototype.map called on non-object");
                    return Completion::Throw(err);
                }
                let mapper = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(&mapper, JsValue::Object(o) if interp.get_object(o.id).map(|od| od.borrow().callable.is_some()).unwrap_or(false))
                {
                    let _ = iterator_close_getter(interp, this);
                    let err = interp.create_type_error("mapper is not a function");
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
                                interp.create_iter_result_object(JsValue::Undefined, true),
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
                                        &JsValue::Undefined,
                                        &[value, JsValue::Number(counter)],
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
                                            interp.create_iter_result_object(JsValue::Undefined, true),
                                        ),
                                    }
                                }
                                Ok(None) => {
                                    state_next.borrow_mut().4 = false;
                                    Completion::Normal(
                                        interp.create_iter_result_object(JsValue::Undefined, true),
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
                                    interp.create_iter_result_object(JsValue::Undefined, true),
                                )
                            };
                        state_ret.borrow_mut().5 = false;
                        result
                    },
                ));

                let helper = interp.create_iterator_helper_object(next_fn, return_fn);
                Completion::Normal(helper)
            },
        ));
        iter_proto
            .borrow_mut()
            .insert_builtin("map".to_string(), map_fn);

        // filter(predicate)
        let filter_fn = self.create_function(JsFunction::native(
            "filter".to_string(),
            1,
            |interp, this, args| {
                if !matches!(this, JsValue::Object(_)) {
                    let err = interp.create_type_error("Iterator.prototype.filter called on non-object");
                    return Completion::Throw(err);
                }
                let predicate = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(&predicate, JsValue::Object(o) if interp.get_object(o.id).map(|od| od.borrow().callable.is_some()).unwrap_or(false))
                {
                    let _ = iterator_close_getter(interp, this);
                    let err = interp.create_type_error("predicate is not a function");
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
                                interp.create_iter_result_object(JsValue::Undefined, true),
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
                                            &JsValue::Undefined,
                                            &[value.clone(), JsValue::Number(counter)],
                                        );
                                        counter += 1.0;
                                        state_next.borrow_mut().3 = counter;
                                        match test_result {
                                            Completion::Normal(v) => {
                                                if to_boolean(&v) {
                                                    return Completion::Normal(
                                                        interp.create_iter_result_object(value, false),
                                                    );
                                                }
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
                                            interp.create_iter_result_object(JsValue::Undefined, true),
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
                                    interp.create_iter_result_object(JsValue::Undefined, true),
                                )
                            }
                        } else {
                            Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            )
                        };
                        state_ret.borrow_mut().5 = false;
                        result
                    },
                ));

                let helper = interp.create_iterator_helper_object(next_fn, return_fn);
                Completion::Normal(helper)
            },
        ));
        iter_proto
            .borrow_mut()
            .insert_builtin("filter".to_string(), filter_fn);

        // take(limit)
        let take_fn = self.create_function(JsFunction::native(
            "take".to_string(),
            1,
            |interp, this, args| {
                // Step 2: If this is not an Object, throw TypeError
                if !matches!(this, JsValue::Object(_)) {
                    let err =
                        interp.create_type_error("Iterator.prototype.take called on non-object");
                    return Completion::Throw(err);
                }
                // Step 3: numLimit = ToNumber(limit) — can throw via valueOf
                let limit_val = args.first().cloned().unwrap_or(JsValue::Undefined);
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
                    let _ = iterator_close_getter(interp, this);
                    let err = interp
                        .create_error("RangeError", "take limit must be a non-negative number");
                    return Completion::Throw(err);
                }
                // Step 5-6: integerLimit = ToIntegerOrInfinity, check < 0
                let integer_limit = if num_limit.is_infinite() {
                    num_limit
                } else {
                    num_limit.trunc()
                };
                if integer_limit < 0.0 {
                    let _ = iterator_close_getter(interp, this);
                    let err = interp
                        .create_error("RangeError", "take limit must be a non-negative number");
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
                                interp.create_iter_result_object(JsValue::Undefined, true),
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
                                    interp.create_iter_result_object(JsValue::Undefined, true),
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
                                        interp.create_iter_result_object(JsValue::Undefined, true),
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
                                    interp.create_iter_result_object(JsValue::Undefined, true),
                                )
                            }
                        } else {
                            Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            )
                        };
                        state_ret.borrow_mut().4 = false;
                        result
                    },
                ));

                let helper = interp.create_iterator_helper_object(next_fn, return_fn);
                Completion::Normal(helper)
            },
        ));
        iter_proto
            .borrow_mut()
            .insert_builtin("take".to_string(), take_fn);

        // drop(limit)
        let drop_fn = self.create_function(JsFunction::native(
            "drop".to_string(),
            1,
            |interp, this, args| {
                // Step 2: If this is not an Object, throw TypeError
                if !matches!(this, JsValue::Object(_)) {
                    let err =
                        interp.create_type_error("Iterator.prototype.drop called on non-object");
                    return Completion::Throw(err);
                }
                // Step 3: numLimit = ToNumber(limit)
                let limit_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let num_limit = match interp.to_number_value(&limit_val) {
                    Ok(n) => n,
                    Err(e) => {
                        let _ = iterator_close_getter(interp, this);
                        return Completion::Throw(e);
                    }
                };
                // Step 4: If numLimit is NaN, throw RangeError
                if num_limit.is_nan() {
                    let _ = iterator_close_getter(interp, this);
                    let err = interp
                        .create_error("RangeError", "drop limit must be a non-negative number");
                    return Completion::Throw(err);
                }
                // Step 5-6: integerLimit = ToIntegerOrInfinity, check < 0
                let integer_limit = if num_limit.is_infinite() {
                    num_limit
                } else {
                    num_limit.trunc()
                };
                if integer_limit < 0.0 {
                    let _ = iterator_close_getter(interp, this);
                    let err = interp
                        .create_error("RangeError", "drop limit must be a non-negative number");
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
                                interp.create_iter_result_object(JsValue::Undefined, true),
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
                                                    JsValue::Undefined,
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
                                        interp.create_iter_result_object(JsValue::Undefined, true),
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
                                    interp.create_iter_result_object(JsValue::Undefined, true),
                                )
                            }
                        } else {
                            Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            )
                        };
                        state_ret.borrow_mut().5 = false;
                        result
                    },
                ));

                let helper = interp.create_iterator_helper_object(next_fn, return_fn);
                Completion::Normal(helper)
            },
        ));
        iter_proto
            .borrow_mut()
            .insert_builtin("drop".to_string(), drop_fn);

        // flatMap(mapper)
        let flat_map_fn = self.create_function(JsFunction::native(
            "flatMap".to_string(),
            1,
            |interp, this, args| {
                if !matches!(this, JsValue::Object(_)) {
                    let err = interp.create_type_error("Iterator.prototype.flatMap called on non-object");
                    return Completion::Throw(err);
                }
                let mapper = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(&mapper, JsValue::Object(o) if interp.get_object(o.id).map(|od| od.borrow().callable.is_some()).unwrap_or(false))
                {
                    let _ = iterator_close_getter(interp, this);
                    let err = interp.create_type_error("mapper is not a function");
                    return Completion::Throw(err);
                }
                let (iter, next_method) = match get_iterator_direct_getter(interp, this) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // state: (outer_iter, outer_next, mapper, counter, inner_iter, inner_next, alive, running)
                #[allow(clippy::type_complexity)]
                let state: Rc<
                    RefCell<(
                        JsValue,
                        JsValue,
                        JsValue,
                        f64,
                        Option<JsValue>,
                        Option<JsValue>,
                        bool,
                        bool,
                    )>,
                > = Rc::new(RefCell::new((
                    iter,
                    next_method,
                    mapper,
                    0.0,
                    None,
                    None,
                    true,
                    false,
                )));

                let state_next = state.clone();
                let next_fn = interp.create_function(JsFunction::native(
                    "next".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let alive = state_next.borrow().6;
                        let running = state_next.borrow().7;
                        if !alive {
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            );
                        }
                        if running {
                            let err = interp.create_type_error("Iterator helper method called while iterator is already being iterated");
                            return Completion::Throw(err);
                        }
                        state_next.borrow_mut().7 = true;
                        let result = (|| {
                            loop {
                                let (
                                    outer_iter,
                                    outer_next,
                                    mapper,
                                    counter,
                                    inner_iter,
                                    inner_next,
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
                                        s.5.clone(),
                                        s.6,
                                        s.7,
                                    )
                                };

                                // If we have an inner iterator, drain it
                                if let (Some(ii), Some(in_next)) = (&inner_iter, &inner_next) {
                                    match interp.iterator_step_direct(ii, in_next) {
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
                                            state_next.borrow_mut().4 = None;
                                            state_next.borrow_mut().5 = None;
                                            continue;
                                        }
                                        Err(e) => {
                                            state_next.borrow_mut().6 = false;
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
                                            &JsValue::Undefined,
                                            &[value, JsValue::Number(counter)],
                                        );
                                        state_next.borrow_mut().3 = counter + 1.0;
                                        match mapped {
                                            Completion::Normal(mapped_val) => {
                                                match get_iterator_flattenable(interp, &mapped_val, true) {
                                                    Ok((new_inner, inner_next_method)) => {
                                                        state_next.borrow_mut().4 = Some(new_inner);
                                                        state_next.borrow_mut().5 = Some(inner_next_method);
                                                        continue;
                                                    }
                                                    Err(e) => {
                                                        state_next.borrow_mut().6 = false;
                                                        let _ = iterator_close_getter(interp, &outer_iter);
                                                        return Completion::Throw(e);
                                                    }
                                                }
                                            }
                                            Completion::Throw(e) => {
                                                state_next.borrow_mut().6 = false;
                                                let _ = iterator_close_getter(interp, &outer_iter);
                                                return Completion::Throw(e);
                                            }
                                            _ => {
                                                state_next.borrow_mut().6 = false;
                                                return Completion::Normal(
                                                    interp.create_iter_result_object(
                                                        JsValue::Undefined,
                                                        true,
                                                    ),
                                                );
                                            }
                                        }
                                    }
                                    Ok(None) => {
                                        state_next.borrow_mut().6 = false;
                                        return Completion::Normal(
                                            interp
                                                .create_iter_result_object(JsValue::Undefined, true),
                                        );
                                    }
                                    Err(e) => {
                                        state_next.borrow_mut().6 = false;
                                        return Completion::Throw(e);
                                    }
                                }
                            }
                        })();
                        state_next.borrow_mut().7 = false;
                        result
                    },
                ));

                let state_ret = state.clone();
                let return_fn = interp.create_function(JsFunction::native(
                    "return".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (outer_iter, inner_iter, alive, running) = {
                            let s = state_ret.borrow();
                            (s.0.clone(), s.4.clone(), s.6, s.7)
                        };
                        if running {
                            let err = interp.create_type_error("Iterator helper method called while iterator is already being iterated");
                            return Completion::Throw(err);
                        }
                        state_ret.borrow_mut().7 = true;
                        state_ret.borrow_mut().6 = false;
                        state_ret.borrow_mut().4 = None;
                        state_ret.borrow_mut().5 = None;
                        let result = if alive {
                            if let Some(ref ii) = inner_iter {
                                let _ = iterator_close_getter(interp, ii);
                            }
                            if let Err(e) = iterator_close_getter(interp, &outer_iter) {
                                Completion::Throw(e)
                            } else {
                                Completion::Normal(
                                    interp.create_iter_result_object(JsValue::Undefined, true),
                                )
                            }
                        } else {
                            Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            )
                        };
                        state_ret.borrow_mut().7 = false;
                        result
                    },
                ));

                let helper = interp.create_iterator_helper_object(next_fn, return_fn);
                Completion::Normal(helper)
            },
        ));
        iter_proto
            .borrow_mut()
            .insert_builtin("flatMap".to_string(), flat_map_fn);
    }

    fn setup_iterator_static_methods(&mut self, iterator_ctor: &JsValue) {
        // Iterator.from(obj) — per spec §27.1.4.2
        // Shared state map: wrapper_id -> (iter, next_method, alive)
        #[allow(clippy::type_complexity)]
        let wrap_state_map: Rc<RefCell<HashMap<u64, (JsValue, JsValue, bool)>>> =
            Rc::new(RefCell::new(HashMap::new()));

        // Create shared WrapForValidIteratorPrototype
        let wrap_valid_proto = self.create_object();
        wrap_valid_proto.borrow_mut().prototype = self.iterator_prototype.clone();

        let map_for_next = wrap_state_map.clone();
        let wrap_next_fn = self.create_function(JsFunction::native(
            "next".to_string(),
            0,
            move |interp, this, _args| {
                let this_id = match this {
                    JsValue::Object(o) => o.id,
                    _ => {
                        let err = interp.create_type_error("next requires an Iterator wrapper");
                        return Completion::Throw(err);
                    }
                };
                let entry = map_for_next.borrow().get(&this_id).cloned();
                let (iter, next_method, alive) = match entry {
                    Some(s) => s,
                    None => {
                        let err = interp.create_type_error("next requires an Iterator wrapper");
                        return Completion::Throw(err);
                    }
                };
                if !alive {
                    return Completion::Normal(
                        interp.create_iter_result_object(JsValue::Undefined, true),
                    );
                }
                match interp.call_function(&next_method, &iter, &[]) {
                    Completion::Normal(v) => {
                        if !matches!(v, JsValue::Object(_)) {
                            let err = interp.create_type_error("Iterator result is not an object");
                            return Completion::Throw(err);
                        }
                        match interp.iterator_complete(&v) {
                            Ok(true) => {
                                if let Some(s) = map_for_next.borrow_mut().get_mut(&this_id) {
                                    s.2 = false;
                                }
                            }
                            Err(e) => return Completion::Throw(e),
                            _ => {}
                        }
                        Completion::Normal(v)
                    }
                    Completion::Throw(e) => {
                        if let Some(s) = map_for_next.borrow_mut().get_mut(&this_id) {
                            s.2 = false;
                        }
                        Completion::Throw(e)
                    }
                    _ => Completion::Normal(
                        interp.create_iter_result_object(JsValue::Undefined, true),
                    ),
                }
            },
        ));
        wrap_valid_proto
            .borrow_mut()
            .insert_builtin("next".to_string(), wrap_next_fn);

        let map_for_ret = wrap_state_map.clone();
        let wrap_return_fn = self.create_function(JsFunction::native(
            "return".to_string(),
            0,
            move |interp, this, _args| {
                let this_id = match this {
                    JsValue::Object(o) => o.id,
                    _ => {
                        let err = interp.create_type_error("return requires an Iterator wrapper");
                        return Completion::Throw(err);
                    }
                };
                let entry = map_for_ret.borrow().get(&this_id).cloned();
                let (iter, _next_method, alive) = match entry {
                    Some(s) => s,
                    None => {
                        let err = interp.create_type_error("return requires an Iterator wrapper");
                        return Completion::Throw(err);
                    }
                };
                if let Some(s) = map_for_ret.borrow_mut().get_mut(&this_id) {
                    s.2 = false;
                }
                if !alive {
                    return Completion::Normal(
                        interp.create_iter_result_object(JsValue::Undefined, true),
                    );
                }
                // Per spec: Get returnMethod = GetMethod(iterator, "return")
                // If undefined, return CreateIterResult(undefined, true)
                // Otherwise return Call(returnMethod, iterator)
                if let JsValue::Object(io) = &iter {
                    let return_method = match interp.get_object_property(io.id, "return", &iter) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => JsValue::Undefined,
                    };
                    if matches!(return_method, JsValue::Undefined | JsValue::Null) {
                        return Completion::Normal(
                            interp.create_iter_result_object(JsValue::Undefined, true),
                        );
                    }
                    interp.call_function(&return_method, &iter, &[])
                } else {
                    Completion::Normal(interp.create_iter_result_object(JsValue::Undefined, true))
                }
            },
        ));
        wrap_valid_proto
            .borrow_mut()
            .insert_builtin("return".to_string(), wrap_return_fn);

        let wrap_valid_proto_rc = Some(wrap_valid_proto);

        let map_for_from = wrap_state_map.clone();
        let wvp_for_from = wrap_valid_proto_rc.clone();
        let from_fn = self.create_function(JsFunction::native(
            "from".to_string(),
            1,
            move |interp, _this, args| {
                let obj = args.first().cloned().unwrap_or(JsValue::Undefined);

                // Use GetIteratorFlattenable(obj, iterate-strings) per spec
                let (iter_val, next_method) = match get_iterator_flattenable(interp, &obj, false) {
                    Ok(pair) => pair,
                    Err(e) => return Completion::Throw(e),
                };

                // OrdinaryHasInstance: check if iter_val has %IteratorPrototype% in chain
                let has_iter_proto = if let JsValue::Object(io) = &iter_val {
                    if let Some(iter_obj) = interp.get_object(io.id) {
                        let ip = interp.iterator_prototype.clone();
                        if let Some(ref ip) = ip {
                            let mut proto = iter_obj.borrow().prototype.clone();
                            let mut found = false;
                            while let Some(p) = proto {
                                if Rc::ptr_eq(&p, ip) {
                                    found = true;
                                    break;
                                }
                                proto = p.borrow().prototype.clone();
                            }
                            found
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                if has_iter_proto {
                    return Completion::Normal(iter_val);
                }

                // Create wrapper with shared WrapForValidIteratorPrototype
                let wrapper = interp.create_object();
                wrapper.borrow_mut().prototype = wvp_for_from.clone();
                wrapper.borrow_mut().class_name = "Iterator".to_string();

                let wrapper_id = wrapper.borrow().id.unwrap();
                map_for_from
                    .borrow_mut()
                    .insert(wrapper_id, (iter_val, next_method, true));

                Completion::Normal(JsValue::Object(crate::types::JsObject { id: wrapper_id }))
            },
        ));

        if let JsValue::Object(ctor_obj) = iterator_ctor
            && let Some(obj) = self.get_object(ctor_obj.id)
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
                    if !matches!(arg, JsValue::Object(_)) {
                        let err = interp.create_type_error("value is not iterable");
                        return Completion::Throw(err);
                    }
                    if let Some(ref key) = sym_key {
                        let iter_fn = if let JsValue::Object(o) = arg {
                            match interp.get_object_property(o.id, key, arg) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => JsValue::Undefined,
                            }
                        } else {
                            JsValue::Undefined
                        };
                        if matches!(iter_fn, JsValue::Undefined | JsValue::Null) {
                            let err = interp.create_type_error("value is not iterable");
                            return Completion::Throw(err);
                        }
                        // Verify it's callable
                        let is_callable = if let JsValue::Object(fo) = &iter_fn {
                            interp
                                .get_object(fo.id)
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
                                interp.create_iter_result_object(JsValue::Undefined, true),
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
                                    interp.create_iter_result_object(JsValue::Undefined, true),
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
                                    interp.create_iter_result_object(JsValue::Undefined, true),
                                );
                            }

                            let (ref iterable, ref iter_fn) = iterables[idx];
                            match interp.call_function(iter_fn, iterable, &[]) {
                                Completion::Normal(new_iter) => {
                                    if !matches!(new_iter, JsValue::Object(_)) {
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
                                        interp.create_iter_result_object(JsValue::Undefined, true),
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
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            )
                        };
                        state_ret.borrow_mut().5 = false; // clear running
                        result
                    },
                ));

                let helper = interp.create_iterator_helper_object(next_fn, return_fn);
                Completion::Normal(helper)
            },
        ));

        // Fix concat.length to 0 (spec says rest parameter = length 0)
        if let JsValue::Object(concat_obj) = &concat_fn
            && let Some(obj) = self.get_object(concat_obj.id)
        {
            obj.borrow_mut().insert_property(
                "length".to_string(),
                PropertyDescriptor::data(JsValue::Number(0.0), false, false, true),
            );
        }

        if let JsValue::Object(ctor_obj) = iterator_ctor
            && let Some(obj) = self.get_object(ctor_obj.id)
        {
            obj.borrow_mut()
                .insert_builtin("concat".to_string(), concat_fn);
        }

        // Iterator.zip(iterables [, options])
        let zip_fn = self.create_function(JsFunction::native(
            "zip".to_string(),
            1,
            |interp, _this, args| {
                let iterables_arg = args.first().cloned().unwrap_or(JsValue::Undefined);

                // Step 1: If iterables is not an Object, throw a TypeError
                if !matches!(iterables_arg, JsValue::Object(_)) {
                    let err = interp.create_type_error("iterables is not an object");
                    return Completion::Throw(err);
                }

                // Step 2: GetOptionsObject(options)
                let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                if !matches!(options, JsValue::Undefined | JsValue::Object(_)) {
                    let err = interp.create_type_error("options must be an object or undefined");
                    return Completion::Throw(err);
                }

                // Step 3: Get mode — NOT ToString, direct string comparison
                let mode = if matches!(options, JsValue::Undefined) {
                    "shortest".to_string()
                } else if let JsValue::Object(o) = &options {
                    let mode_val = match interp.get_object_property(o.id, "mode", &options) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => JsValue::Undefined,
                    };
                    if matches!(mode_val, JsValue::Undefined) {
                        "shortest".to_string()
                    } else if let JsValue::String(ref s) = mode_val {
                        let rs = s.to_rust_string();
                        match rs.as_str() {
                            "shortest" | "longest" | "strict" => rs,
                            _ => {
                                let err = interp.create_type_error(
                                    "mode must be 'shortest', 'longest', or 'strict'");
                                return Completion::Throw(err);
                            }
                        }
                    } else {
                        let err = interp.create_type_error(
                            "mode must be 'shortest', 'longest', or 'strict'");
                        return Completion::Throw(err);
                    }
                } else {
                    "shortest".to_string()
                };

                // Step 7: Get padding from options (for "longest" mode)
                let padding_option = if mode == "longest" {
                    if let JsValue::Object(o) = &options {
                        let p = match interp.get_object_property(o.id, "padding", &options) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => JsValue::Undefined,
                        };
                        if !matches!(p, JsValue::Undefined) {
                            if !matches!(p, JsValue::Object(_)) {
                                let err = interp.create_type_error(
                                    "padding must be an object or undefined");
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
                let mut iters: Vec<(JsValue, JsValue)> = Vec::new();

                loop {
                    match iterator_step_value_getter(interp, &input_iter, &input_next) {
                        Ok(Some(next_val)) => {
                            match get_iterator_flattenable(interp, &next_val, true) {
                                Ok(pair) => iters.push(pair),
                                Err(e) => {
                                    // IfAbruptCloseIterators(iter, « inputIter » + iters) — reverse order
                                    let mut all = vec![(input_iter.clone(), input_next.clone())];
                                    all.extend(iters.iter().cloned());
                                    let _ = iterator_close_all(interp, &all, Err(e.clone()));
                                    return Completion::Throw(e);
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            // IfAbruptCloseIterators(next, iters) — just the collected iters
                            let _ = iterator_close_all(interp, &iters, Err(e.clone()));
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
                                        pads.push(JsValue::Undefined);
                                    }
                                    Err(e) => {
                                        let _ = iterator_close_all(interp, &iters, Err(e.clone()));
                                        return Completion::Throw(e);
                                    }
                                }
                            } else {
                                pads.push(JsValue::Undefined);
                            }
                        }
                        if using_iterator
                            && let Err(e) = iterator_close_getter(interp, &pi) {
                                let _ = iterator_close_all(interp, &iters, Err(e.clone()));
                                return Completion::Throw(e);
                            }
                        pads
                    } else {
                        vec![JsValue::Undefined; iter_count]
                    }
                } else {
                    vec![JsValue::Undefined; iter_count]
                };

                // State: (iters, exhausted, mode, padding, alive)
                // exhausted tracks which iterators in openIters are exhausted
                #[allow(clippy::type_complexity)]
                let state: Rc<RefCell<(Vec<(JsValue, JsValue)>, Vec<bool>, String, Vec<JsValue>, bool)>> =
                    Rc::new(RefCell::new((iters, vec![false; iter_count], mode, padding_values, true)));

                let state_next = state.clone();
                let next_fn = interp.create_function(JsFunction::native(
                    "next".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (ref iters, ref exhausted, ref mode, ref padding_values, alive) = {
                            let s = state_next.borrow();
                            (s.0.clone(), s.1.clone(), s.2.clone(), s.3.clone(), s.4)
                        };
                        if !alive {
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            );
                        }
                        if iters.is_empty() {
                            state_next.borrow_mut().4 = false;
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            );
                        }

                        let mut values = Vec::with_capacity(iters.len());
                        let mut new_exhausted = exhausted.clone();

                        for (i, (it, nm)) in iters.iter().enumerate() {
                            if exhausted[i] {
                                values.push(padding_values.get(i).cloned().unwrap_or(JsValue::Undefined));
                                continue;
                            }
                            match iterator_step_value_getter(interp, it, nm) {
                                Ok(Some(v)) => values.push(v),
                                Ok(None) => {
                                    // Iterator done — remove from openIters
                                    new_exhausted[i] = true;

                                    if mode == "shortest" {
                                        // IteratorCloseAll(openIters, ReturnCompletion(undefined))
                                        state_next.borrow_mut().4 = false;
                                        let open: Vec<(JsValue, JsValue)> = iters.iter().enumerate()
                                            .filter(|(j, _)| !new_exhausted[*j])
                                            .map(|(_, pair)| pair.clone())
                                            .collect();
                                        if let Err(e) = iterator_close_all(interp, &open, Ok(())) {
                                            return Completion::Throw(e);
                                        }
                                        return Completion::Normal(
                                            interp.create_iter_result_object(JsValue::Undefined, true),
                                        );
                                    } else if mode == "strict" {
                                        if i != 0 {
                                            // i ≠ 0: immediately close all open iters with TypeError
                                            state_next.borrow_mut().4 = false;
                                            let open: Vec<(JsValue, JsValue)> = iters.iter().enumerate()
                                                .filter(|(j, _)| !new_exhausted[*j])
                                                .map(|(_, pair)| pair.clone())
                                                .collect();
                                            let err = interp.create_type_error(
                                                "Iterators passed to Iterator.zip with { mode: \"strict\" } have different lengths");
                                            let _ = iterator_close_all(interp, &open, Err(err.clone()));
                                            return Completion::Throw(err);
                                        }
                                        // i = 0: check all other iterators
                                        for k in 1..iters.len() {
                                            if new_exhausted[k] { continue; }
                                            match iterator_step_value_getter(interp, &iters[k].0, &iters[k].1) {
                                                Ok(None) => {
                                                    new_exhausted[k] = true;
                                                }
                                                Ok(Some(_)) => {
                                                    // Mismatch — not all done
                                                    state_next.borrow_mut().4 = false;
                                                    let open: Vec<(JsValue, JsValue)> = iters.iter().enumerate()
                                                        .filter(|(j, _)| !new_exhausted[*j])
                                                        .map(|(_, pair)| pair.clone())
                                                        .collect();
                                                    let err = interp.create_type_error(
                                                        "Iterators passed to Iterator.zip with { mode: \"strict\" } have different lengths");
                                                    let _ = iterator_close_all(interp, &open, Err(err.clone()));
                                                    return Completion::Throw(err);
                                                }
                                                Err(e) => {
                                                    // Remove iters[k] from openIters
                                                    new_exhausted[k] = true;
                                                    state_next.borrow_mut().4 = false;
                                                    let open: Vec<(JsValue, JsValue)> = iters.iter().enumerate()
                                                        .filter(|(j, _)| !new_exhausted[*j])
                                                        .map(|(_, pair)| pair.clone())
                                                        .collect();
                                                    let _ = iterator_close_all(interp, &open, Err(e.clone()));
                                                    return Completion::Throw(e);
                                                }
                                            }
                                        }
                                        // All done together
                                        state_next.borrow_mut().4 = false;
                                        return Completion::Normal(
                                            interp.create_iter_result_object(JsValue::Undefined, true),
                                        );
                                    } else {
                                        // longest mode: append padding value
                                        values.push(padding_values.get(i).cloned().unwrap_or(JsValue::Undefined));
                                    }
                                }
                                Err(e) => {
                                    state_next.borrow_mut().4 = false;
                                    state_next.borrow_mut().1 = new_exhausted.clone();
                                    let open: Vec<(JsValue, JsValue)> = iters.iter().enumerate()
                                        .filter(|(j, _)| !new_exhausted[*j] && *j != i)
                                        .map(|(_, pair)| pair.clone())
                                        .collect();
                                    let _ = iterator_close_all(interp, &open, Err(e.clone()));
                                    return Completion::Throw(e);
                                }
                            }
                        }

                        state_next.borrow_mut().1 = new_exhausted.clone();

                        // For longest mode: check if ALL are now exhausted
                        if mode == "longest" && new_exhausted.iter().all(|e| *e) {
                            state_next.borrow_mut().4 = false;
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            );
                        }

                        let arr = interp.create_array(values);
                        Completion::Normal(interp.create_iter_result_object(arr, false))
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
                            let open: Vec<(JsValue, JsValue)> = iters.iter().enumerate()
                                .filter(|(i, _)| !exhausted[*i])
                                .map(|(_, pair)| pair.clone())
                                .collect();
                            if let Err(e) = iterator_close_all(interp, &open, Ok(())) {
                                return Completion::Throw(e);
                            }
                        }
                        Completion::Normal(
                            interp.create_iter_result_object(JsValue::Undefined, true),
                        )
                    },
                ));

                let helper = interp.create_iterator_helper_object(next_fn, return_fn);
                Completion::Normal(helper)
            },
        ));

        if let JsValue::Object(ctor_obj) = iterator_ctor
            && let Some(obj) = self.get_object(ctor_obj.id)
        {
            obj.borrow_mut().insert_builtin("zip".to_string(), zip_fn);
        }

        // Iterator.zipKeyed(iterables [, options])
        let zip_keyed_fn = self.create_function(JsFunction::native(
            "zipKeyed".to_string(),
            1,
            |interp, _this, args| {
                let iterables_obj = args.first().cloned().unwrap_or(JsValue::Undefined);

                // Step 1: iterables must be an object
                let obj_id = match &iterables_obj {
                    JsValue::Object(o) => o.id,
                    _ => {
                        let err = interp.create_type_error("iterables must be an object");
                        return Completion::Throw(err);
                    }
                };

                // Step 2: GetOptionsObject(options)
                let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                if !matches!(options, JsValue::Undefined | JsValue::Object(_)) {
                    let err = interp.create_type_error("options must be an object or undefined");
                    return Completion::Throw(err);
                }

                // Step 3: Get mode — direct string comparison, no ToString
                let mode = if matches!(options, JsValue::Undefined) {
                    "shortest".to_string()
                } else if let JsValue::Object(o) = &options {
                    let mode_val = match interp.get_object_property(o.id, "mode", &options) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => JsValue::Undefined,
                    };
                    if matches!(mode_val, JsValue::Undefined) {
                        "shortest".to_string()
                    } else if let JsValue::String(ref s) = mode_val {
                        let rs = s.to_rust_string();
                        match rs.as_str() {
                            "shortest" | "longest" | "strict" => rs,
                            _ => {
                                let err = interp.create_type_error(
                                    "mode must be 'shortest', 'longest', or 'strict'");
                                return Completion::Throw(err);
                            }
                        }
                    } else {
                        let err = interp.create_type_error(
                            "mode must be 'shortest', 'longest', or 'strict'");
                        return Completion::Throw(err);
                    }
                } else {
                    "shortest".to_string()
                };

                // Step 7: Get padding from options (for "longest" mode)
                let padding_option = if mode == "longest" {
                    if let JsValue::Object(o) = &options {
                        let p = match interp.get_object_property(o.id, "padding", &options) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => JsValue::Undefined,
                        };
                        if !matches!(p, JsValue::Undefined) {
                            if !matches!(p, JsValue::Object(_)) {
                                let err = interp.create_type_error(
                                    "padding must be an object or undefined");
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

                // Step 10: Get own property keys from iterables
                let all_keys: Vec<String> = if let Some(od) = interp.get_object(obj_id) {
                    od.borrow().property_order.clone()
                } else {
                    Vec::new()
                };

                // Step 11-12: Filter to enumerable string keys, get values, open iterators
                let mut key_names: Vec<String> = Vec::new();
                let mut iters: Vec<(JsValue, JsValue)> = Vec::new();

                for key in &all_keys {
                    // [[GetOwnProperty]](key) to check enumerable
                    let is_enumerable = if let Some(od) = interp.get_object(obj_id) {
                        od.borrow().properties.get(key)
                            .and_then(|pd| pd.enumerable)
                            .unwrap_or(false)
                    } else {
                        false
                    };
                    if !is_enumerable { continue; }

                    // Get(iterables, key) — via getter-aware access
                    let iterable = match interp.get_object_property(obj_id, key, &iterables_obj) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            let _ = iterator_close_all(interp, &iters, Err(e.clone()));
                            return Completion::Throw(e);
                        }
                        _ => JsValue::Undefined,
                    };

                    // Step c.iii: If value is not undefined
                    if matches!(iterable, JsValue::Undefined) { continue; }

                    // GetIteratorFlattenable(iterable, reject-primitives)
                    match get_iterator_flattenable(interp, &iterable, true) {
                        Ok(pair) => {
                            key_names.push(key.clone());
                            iters.push(pair);
                        }
                        Err(e) => {
                            let _ = iterator_close_all(interp, &iters, Err(e.clone()));
                            return Completion::Throw(e);
                        }
                    }
                }

                let iter_count = iters.len();

                // Step 14: Get padding values per key (for longest mode)
                let padding_values: Vec<JsValue> = if mode == "longest" {
                    if let Some(ref pad_obj) = padding_option {
                        if let JsValue::Object(po) = pad_obj {
                            let mut pads = Vec::with_capacity(iter_count);
                            for key in &key_names {
                                let val = match interp.get_object_property(po.id, key, pad_obj) {
                                    Completion::Normal(v) => v,
                                    Completion::Throw(e) => {
                                        let _ = iterator_close_all(interp, &iters, Err(e.clone()));
                                        return Completion::Throw(e);
                                    }
                                    _ => JsValue::Undefined,
                                };
                                pads.push(val);
                            }
                            pads
                        } else {
                            vec![JsValue::Undefined; iter_count]
                        }
                    } else {
                        vec![JsValue::Undefined; iter_count]
                    }
                } else {
                    vec![JsValue::Undefined; iter_count]
                };

                // state: (key_names, iters, exhausted, mode, padding_values, alive)
                #[allow(clippy::type_complexity)]
                let state: Rc<RefCell<(Vec<String>, Vec<(JsValue, JsValue)>, Vec<bool>, String, Vec<JsValue>, bool)>> =
                    Rc::new(RefCell::new((key_names, iters, vec![false; iter_count], mode, padding_values, true)));

                let state_next = state.clone();
                let next_fn = interp.create_function(JsFunction::native(
                    "next".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (ref keys, ref iters, ref exhausted, ref mode, ref padding_values, alive) = {
                            let s = state_next.borrow();
                            (s.0.clone(), s.1.clone(), s.2.clone(), s.3.clone(), s.4.clone(), s.5)
                        };
                        if !alive {
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            );
                        }
                        if iters.is_empty() {
                            state_next.borrow_mut().5 = false;
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            );
                        }

                        let mut values: Vec<(String, JsValue)> = Vec::with_capacity(iters.len());
                        let mut new_exhausted = exhausted.clone();

                        for (i, (it, nm)) in iters.iter().enumerate() {
                            if exhausted[i] {
                                values.push((keys[i].clone(), padding_values.get(i).cloned().unwrap_or(JsValue::Undefined)));
                                continue;
                            }
                            match iterator_step_value_getter(interp, it, nm) {
                                Ok(Some(v)) => values.push((keys[i].clone(), v)),
                                Ok(None) => {
                                    new_exhausted[i] = true;

                                    if mode == "shortest" {
                                        state_next.borrow_mut().5 = false;
                                        let open: Vec<(JsValue, JsValue)> = iters.iter().enumerate()
                                            .filter(|(j, _)| !new_exhausted[*j])
                                            .map(|(_, pair)| pair.clone())
                                            .collect();
                                        if let Err(e) = iterator_close_all(interp, &open, Ok(())) {
                                            return Completion::Throw(e);
                                        }
                                        return Completion::Normal(
                                            interp.create_iter_result_object(JsValue::Undefined, true),
                                        );
                                    } else if mode == "strict" {
                                        if i != 0 {
                                            state_next.borrow_mut().5 = false;
                                            let open: Vec<(JsValue, JsValue)> = iters.iter().enumerate()
                                                .filter(|(j, _)| !new_exhausted[*j])
                                                .map(|(_, pair)| pair.clone())
                                                .collect();
                                            let err = interp.create_type_error(
                                                "Iterators passed to Iterator.zipKeyed with { mode: \"strict\" } have different lengths");
                                            let _ = iterator_close_all(interp, &open, Err(err.clone()));
                                            return Completion::Throw(err);
                                        }
                                        // i = 0: check all other iterators
                                        for k in 1..iters.len() {
                                            if new_exhausted[k] { continue; }
                                            match iterator_step_value_getter(interp, &iters[k].0, &iters[k].1) {
                                                Ok(None) => { new_exhausted[k] = true; }
                                                Ok(Some(_)) => {
                                                    state_next.borrow_mut().5 = false;
                                                    let open: Vec<(JsValue, JsValue)> = iters.iter().enumerate()
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
                                                    state_next.borrow_mut().5 = false;
                                                    let open: Vec<(JsValue, JsValue)> = iters.iter().enumerate()
                                                        .filter(|(j, _)| !new_exhausted[*j])
                                                        .map(|(_, pair)| pair.clone())
                                                        .collect();
                                                    let _ = iterator_close_all(interp, &open, Err(e.clone()));
                                                    return Completion::Throw(e);
                                                }
                                            }
                                        }
                                        state_next.borrow_mut().5 = false;
                                        return Completion::Normal(
                                            interp.create_iter_result_object(JsValue::Undefined, true),
                                        );
                                    } else {
                                        // longest mode: append padding value
                                        values.push((keys[i].clone(), padding_values.get(i).cloned().unwrap_or(JsValue::Undefined)));
                                    }
                                }
                                Err(e) => {
                                    state_next.borrow_mut().5 = false;
                                    state_next.borrow_mut().2 = new_exhausted.clone();
                                    let open: Vec<(JsValue, JsValue)> = iters.iter().enumerate()
                                        .filter(|(j, _)| !new_exhausted[*j] && *j != i)
                                        .map(|(_, pair)| pair.clone())
                                        .collect();
                                    let _ = iterator_close_all(interp, &open, Err(e.clone()));
                                    return Completion::Throw(e);
                                }
                            }
                        }

                        state_next.borrow_mut().2 = new_exhausted.clone();

                        // For longest mode: check if ALL are now exhausted
                        if mode == "longest" && new_exhausted.iter().all(|e| *e) {
                            state_next.borrow_mut().5 = false;
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            );
                        }

                        // Create null-prototype result object with key-value pairs
                        let result_obj = interp.create_object();
                        result_obj.borrow_mut().prototype = None;
                        for (key, val) in &values {
                            result_obj.borrow_mut().insert_property(
                                key.clone(),
                                PropertyDescriptor::data(val.clone(), true, true, true),
                            );
                        }
                        let result_id = result_obj.borrow().id.unwrap();
                        let result_val = JsValue::Object(crate::types::JsObject { id: result_id });
                        Completion::Normal(interp.create_iter_result_object(result_val, false))
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
                            let open: Vec<(JsValue, JsValue)> = iters.iter().enumerate()
                                .filter(|(i, _)| !exhausted[*i])
                                .map(|(_, pair)| pair.clone())
                                .collect();
                            if let Err(e) = iterator_close_all(interp, &open, Ok(())) {
                                return Completion::Throw(e);
                            }
                        }
                        Completion::Normal(
                            interp.create_iter_result_object(JsValue::Undefined, true),
                        )
                    },
                ));

                let helper = interp.create_iterator_helper_object(next_fn, return_fn);
                Completion::Normal(helper)
            },
        ));

        if let JsValue::Object(ctor_obj) = iterator_ctor
            && let Some(obj) = self.get_object(ctor_obj.id)
        {
            obj.borrow_mut()
                .insert_builtin("zipKeyed".to_string(), zip_keyed_fn);
        }
    }

    pub(crate) fn create_array_iterator(&mut self, array_id: u64, kind: IteratorKind) -> JsValue {
        let mut obj_data = JsObjectData::new();
        obj_data.prototype = self
            .array_iterator_prototype
            .clone()
            .or(self.iterator_prototype.clone())
            .or(self.object_prototype.clone());
        obj_data.class_name = "Array Iterator".to_string();
        obj_data.iterator_state = Some(IteratorState::ArrayIterator {
            array_id,
            index: 0,
            kind,
            done: false,
        });
        let obj = Rc::new(RefCell::new(obj_data));
        let id = self.allocate_object_slot(obj);
        JsValue::Object(crate::types::JsObject { id })
    }

    pub(crate) fn create_typed_array_iterator(
        &mut self,
        typed_array_id: u64,
        kind: IteratorKind,
    ) -> JsValue {
        let mut obj_data = JsObjectData::new();
        obj_data.prototype = self
            .array_iterator_prototype
            .clone()
            .or(self.iterator_prototype.clone())
            .or(self.object_prototype.clone());
        obj_data.class_name = "Array Iterator".to_string();
        obj_data.iterator_state = Some(IteratorState::TypedArrayIterator {
            typed_array_id,
            index: 0,
            kind,
            done: false,
        });
        let obj = Rc::new(RefCell::new(obj_data));
        let id = self.allocate_object_slot(obj);
        JsValue::Object(crate::types::JsObject { id })
    }

    pub(crate) fn create_string_iterator(&mut self, string: JsString) -> JsValue {
        let mut obj_data = JsObjectData::new();
        obj_data.prototype = self
            .string_iterator_prototype
            .clone()
            .or(self.iterator_prototype.clone())
            .or(self.object_prototype.clone());
        obj_data.class_name = "String Iterator".to_string();
        obj_data.iterator_state = Some(IteratorState::StringIterator {
            string,
            position: 0,
            done: false,
        });
        let obj = Rc::new(RefCell::new(obj_data));
        let id = self.allocate_object_slot(obj);
        JsValue::Object(crate::types::JsObject { id })
    }

    pub(crate) fn setup_generator_prototype(&mut self) {
        let gen_proto = self.create_object();
        gen_proto.borrow_mut().class_name = "Generator".to_string();
        gen_proto.borrow_mut().prototype = self.iterator_prototype.clone();

        // next(value)
        let next_fn = self.create_function(JsFunction::native(
            "next".to_string(),
            0,
            |interp, this, args| {
                let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                // Check which variant we have
                if let JsValue::Object(o) = this
                    && let Some(obj_rc) = interp.get_object(o.id)
                {
                    let is_state_machine = matches!(
                        obj_rc.borrow().iterator_state,
                        Some(IteratorState::StateMachineGenerator { .. })
                    );
                    if is_state_machine {
                        return interp.generator_next_state_machine(this, value);
                    }
                }
                interp.generator_next(this, value)
            },
        ));
        gen_proto.borrow_mut().insert_property(
            "next".to_string(),
            PropertyDescriptor::data(next_fn, true, false, true),
        );

        // return(value)
        let return_fn = self.create_function(JsFunction::native(
            "return".to_string(),
            0,
            |interp, this, args| {
                let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                // Check which variant we have
                if let JsValue::Object(o) = this
                    && let Some(obj_rc) = interp.get_object(o.id)
                {
                    let is_state_machine = matches!(
                        obj_rc.borrow().iterator_state,
                        Some(IteratorState::StateMachineGenerator { .. })
                    );
                    if is_state_machine {
                        return interp.generator_return_state_machine(this, value);
                    }
                }
                interp.generator_return(this, value)
            },
        ));
        gen_proto.borrow_mut().insert_property(
            "return".to_string(),
            PropertyDescriptor::data(return_fn, true, false, true),
        );

        // throw(exception)
        let throw_fn = self.create_function(JsFunction::native(
            "throw".to_string(),
            1,
            |interp, this, args| {
                let exception = args.first().cloned().unwrap_or(JsValue::Undefined);
                // Check which variant we have
                if let JsValue::Object(o) = this
                    && let Some(obj_rc) = interp.get_object(o.id)
                {
                    let is_state_machine = matches!(
                        obj_rc.borrow().iterator_state,
                        Some(IteratorState::StateMachineGenerator { .. })
                    );
                    if is_state_machine {
                        return interp.generator_throw_state_machine(this, exception);
                    }
                }
                interp.generator_throw(this, exception)
            },
        ));
        gen_proto.borrow_mut().insert_property(
            "throw".to_string(),
            PropertyDescriptor::data(throw_fn, true, false, true),
        );

        // Symbol.iterator returns this
        let iter_self_fn = self.create_function(JsFunction::native(
            "[Symbol.iterator]".to_string(),
            0,
            |_interp, this, _args| Completion::Normal(this.clone()),
        ));
        if let Some(key) = self.get_symbol_iterator_key() {
            gen_proto.borrow_mut().insert_property(
                key,
                PropertyDescriptor::data(iter_self_fn, true, false, true),
            );
        }

        // Symbol.toStringTag
        gen_proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str("Generator")),
                false,
                false,
                true,
            ),
        );

        self.generator_prototype = Some(gen_proto.clone());

        // %GeneratorFunction.prototype% - the prototype of generator function objects
        let gf_proto = self.create_object();
        gf_proto.borrow_mut().class_name = "GeneratorFunction".to_string();

        // [[Prototype]] = Function.prototype
        // Get Function.prototype from global Function
        if let Some(func_val) = self.global_env.borrow().get("Function")
            && let JsValue::Object(func_obj) = func_val
            && let Some(func_data) = self.get_object(func_obj.id)
            && let JsValue::Object(func_proto_obj) = func_data.borrow().get_property("prototype")
            && let Some(func_proto) = self.get_object(func_proto_obj.id)
        {
            gf_proto.borrow_mut().prototype = Some(func_proto);
        }

        // GeneratorFunction.prototype.prototype = Generator.prototype
        let gen_proto_id = gen_proto.borrow().id.unwrap();
        gf_proto.borrow_mut().insert_property(
            "prototype".to_string(),
            PropertyDescriptor::data(
                JsValue::Object(crate::types::JsObject { id: gen_proto_id }),
                false,
                false,
                true,
            ),
        );

        // Symbol.toStringTag = "GeneratorFunction"
        gf_proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str("GeneratorFunction")),
                false,
                false,
                true,
            ),
        );

        // Set constructor on Generator.prototype pointing back to GeneratorFunction.prototype
        let gf_proto_id = gf_proto.borrow().id.unwrap();
        gen_proto.borrow_mut().insert_property(
            "constructor".to_string(),
            PropertyDescriptor::data(
                JsValue::Object(crate::types::JsObject { id: gf_proto_id }),
                false,
                false,
                true,
            ),
        );

        self.generator_function_prototype = Some(gf_proto);
    }

    pub(crate) fn setup_async_generator_prototype(&mut self) {
        // %AsyncIteratorPrototype% — has [Symbol.asyncIterator]() returning this
        let async_iter_proto = self.create_object();
        async_iter_proto.borrow_mut().class_name = "AsyncIterator".to_string();

        let async_iter_self_fn = self.create_function(JsFunction::native(
            "[Symbol.asyncIterator]".to_string(),
            0,
            |_interp, this, _args| Completion::Normal(this.clone()),
        ));
        if let Some(key) = self.get_symbol_key("asyncIterator") {
            async_iter_proto.borrow_mut().insert_property(
                key,
                PropertyDescriptor::data(async_iter_self_fn, true, false, true),
            );
        }
        self.async_iterator_prototype = Some(async_iter_proto.clone());

        // %AsyncGeneratorPrototype%
        let gen_proto = self.create_object();
        gen_proto.borrow_mut().prototype = Some(async_iter_proto);
        gen_proto.borrow_mut().class_name = "AsyncGenerator".to_string();

        // next(value)
        let next_fn = self.create_function(JsFunction::native(
            "next".to_string(),
            1,
            |interp, this, args| {
                let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                interp.async_generator_next(this, value)
            },
        ));
        gen_proto.borrow_mut().insert_property(
            "next".to_string(),
            PropertyDescriptor::data(next_fn, true, false, true),
        );

        // return(value)
        let return_fn = self.create_function(JsFunction::native(
            "return".to_string(),
            1,
            |interp, this, args| {
                let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                interp.async_generator_return(this, value)
            },
        ));
        gen_proto.borrow_mut().insert_property(
            "return".to_string(),
            PropertyDescriptor::data(return_fn, true, false, true),
        );

        // throw(exception)
        let throw_fn = self.create_function(JsFunction::native(
            "throw".to_string(),
            1,
            |interp, this, args| {
                let exception = args.first().cloned().unwrap_or(JsValue::Undefined);
                interp.async_generator_throw(this, exception)
            },
        ));
        gen_proto.borrow_mut().insert_property(
            "throw".to_string(),
            PropertyDescriptor::data(throw_fn, true, false, true),
        );

        // Symbol.toStringTag
        gen_proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str("AsyncGenerator")),
                false,
                false,
                true,
            ),
        );

        self.async_generator_prototype = Some(gen_proto.clone());

        // %AsyncGeneratorFunction.prototype%
        let agf_proto = self.create_object();
        agf_proto.borrow_mut().class_name = "AsyncGeneratorFunction".to_string();
        // prototype property points to AsyncGenerator.prototype
        let gen_proto_id = gen_proto.borrow().id.unwrap();
        agf_proto.borrow_mut().insert_property(
            "prototype".to_string(),
            PropertyDescriptor::data(
                JsValue::Object(crate::types::JsObject { id: gen_proto_id }),
                false,
                false,
                true,
            ),
        );
        // Symbol.toStringTag
        agf_proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str("AsyncGeneratorFunction")),
                false,
                false,
                true,
            ),
        );
        // Set constructor on AsyncGenerator.prototype pointing back to AsyncGeneratorFunction.prototype
        let agf_proto_id = agf_proto.borrow().id.unwrap();
        gen_proto.borrow_mut().insert_property(
            "constructor".to_string(),
            PropertyDescriptor::data(
                JsValue::Object(crate::types::JsObject { id: agf_proto_id }),
                false,
                false,
                true,
            ),
        );
        self.async_generator_function_prototype = Some(agf_proto);
    }
}
