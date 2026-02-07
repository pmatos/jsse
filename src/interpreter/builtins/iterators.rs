use super::super::*;

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
        // @@toStringTag on %IteratorPrototype%
        iter_proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str("Iterator")),
                false,
                false,
                true,
            ),
        );

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

        // Set %IteratorPrototype%.constructor = Iterator
        iter_proto.borrow_mut().insert_property(
            "constructor".to_string(),
            PropertyDescriptor::data(iterator_ctor.clone(), true, false, true),
        );

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
                let (iter, next_method) = match interp.get_iterator_direct(this) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let mut values = Vec::new();
                loop {
                    match interp.iterator_step_direct(&iter, &next_method) {
                        Ok(Some(result)) => match interp.iterator_value(&result) {
                            Ok(v) => values.push(v),
                            Err(e) => {
                                interp.iterator_close(&iter, JsValue::Undefined);
                                return Completion::Throw(e);
                            }
                        },
                        Ok(None) => break,
                        Err(e) => {
                            interp.iterator_close(&iter, JsValue::Undefined);
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
                    interp.iterator_close(this, JsValue::Undefined);
                    let err = interp.create_type_error("callback is not a function");
                    return Completion::Throw(err);
                }
                let (iter, next_method) = match interp.get_iterator_direct(this) {
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
                                interp.iterator_close(&iter, JsValue::Undefined);
                                return Completion::Throw(e);
                            }
                            counter += 1.0;
                        }
                        Ok(None) => break,
                        Err(e) => {
                            interp.iterator_close(&iter, JsValue::Undefined);
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
                    interp.iterator_close(this, JsValue::Undefined);
                    let err = interp.create_type_error("predicate is not a function");
                    return Completion::Throw(err);
                }
                let (iter, next_method) = match interp.get_iterator_direct(this) {
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
                                        interp.iterator_close(&iter, JsValue::Undefined);
                                        return Completion::Normal(JsValue::Boolean(true));
                                    }
                                }
                                Completion::Throw(e) => {
                                    interp.iterator_close(&iter, JsValue::Undefined);
                                    return Completion::Throw(e);
                                }
                                _ => {}
                            }
                            counter += 1.0;
                        }
                        Ok(None) => return Completion::Normal(JsValue::Boolean(false)),
                        Err(e) => {
                            interp.iterator_close(&iter, JsValue::Undefined);
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
                    interp.iterator_close(this, JsValue::Undefined);
                    let err = interp.create_type_error("predicate is not a function");
                    return Completion::Throw(err);
                }
                let (iter, next_method) = match interp.get_iterator_direct(this) {
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
                                        interp.iterator_close(&iter, JsValue::Undefined);
                                        return Completion::Normal(JsValue::Boolean(false));
                                    }
                                }
                                Completion::Throw(e) => {
                                    interp.iterator_close(&iter, JsValue::Undefined);
                                    return Completion::Throw(e);
                                }
                                _ => {}
                            }
                            counter += 1.0;
                        }
                        Ok(None) => return Completion::Normal(JsValue::Boolean(true)),
                        Err(e) => {
                            interp.iterator_close(&iter, JsValue::Undefined);
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
                    interp.iterator_close(this, JsValue::Undefined);
                    let err = interp.create_type_error("predicate is not a function");
                    return Completion::Throw(err);
                }
                let (iter, next_method) = match interp.get_iterator_direct(this) {
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
                                        interp.iterator_close(&iter, JsValue::Undefined);
                                        return Completion::Normal(value);
                                    }
                                }
                                Completion::Throw(e) => {
                                    interp.iterator_close(&iter, JsValue::Undefined);
                                    return Completion::Throw(e);
                                }
                                _ => {}
                            }
                            counter += 1.0;
                        }
                        Ok(None) => return Completion::Normal(JsValue::Undefined),
                        Err(e) => {
                            interp.iterator_close(&iter, JsValue::Undefined);
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
                    interp.iterator_close(this, JsValue::Undefined);
                    let err = interp.create_type_error("reducer is not a function");
                    return Completion::Throw(err);
                }
                let (iter, next_method) = match interp.get_iterator_direct(this) {
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
                            interp.iterator_close(&iter, JsValue::Undefined);
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
                                    interp.iterator_close(&iter, JsValue::Undefined);
                                    return Completion::Throw(e);
                                }
                                _ => {}
                            }
                            counter += 1.0;
                        }
                        Ok(None) => return Completion::Normal(accumulator),
                        Err(e) => {
                            interp.iterator_close(&iter, JsValue::Undefined);
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
                let mapper = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(&mapper, JsValue::Object(o) if interp.get_object(o.id).map(|od| od.borrow().callable.is_some()).unwrap_or(false))
                {
                    interp.iterator_close(this, JsValue::Undefined);
                    let err = interp.create_type_error("mapper is not a function");
                    return Completion::Throw(err);
                }
                let (iter, next_method) = match interp.get_iterator_direct(this) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let state: Rc<RefCell<(JsValue, JsValue, JsValue, f64, bool)>> =
                    Rc::new(RefCell::new((iter, next_method, mapper, 0.0, true)));

                let state_next = state.clone();
                let next_fn = interp.create_function(JsFunction::native(
                    "next".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (iter, next_method, mapper, counter, alive) = {
                            let s = state_next.borrow();
                            (s.0.clone(), s.1.clone(), s.2.clone(), s.3, s.4)
                        };
                        if !alive {
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            );
                        }
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
                                        interp.iterator_close(&iter, JsValue::Undefined);
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
                    },
                ));

                let state_ret = state.clone();
                let return_fn = interp.create_function(JsFunction::native(
                    "return".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (iter, alive) = {
                            let s = state_ret.borrow();
                            (s.0.clone(), s.4)
                        };
                        state_ret.borrow_mut().4 = false;
                        if alive {
                            interp.iterator_close(&iter, JsValue::Undefined);
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
        iter_proto
            .borrow_mut()
            .insert_builtin("map".to_string(), map_fn);

        // filter(predicate)
        let filter_fn = self.create_function(JsFunction::native(
            "filter".to_string(),
            1,
            |interp, this, args| {
                let predicate = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(&predicate, JsValue::Object(o) if interp.get_object(o.id).map(|od| od.borrow().callable.is_some()).unwrap_or(false))
                {
                    interp.iterator_close(this, JsValue::Undefined);
                    let err = interp.create_type_error("predicate is not a function");
                    return Completion::Throw(err);
                }
                let (iter, next_method) = match interp.get_iterator_direct(this) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let state: Rc<RefCell<(JsValue, JsValue, JsValue, f64, bool)>> =
                    Rc::new(RefCell::new((iter, next_method, predicate, 0.0, true)));

                let state_next = state.clone();
                let next_fn = interp.create_function(JsFunction::native(
                    "next".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (iter, next_method, pred, mut counter, alive) = {
                            let s = state_next.borrow();
                            (s.0.clone(), s.1.clone(), s.2.clone(), s.3, s.4)
                        };
                        if !alive {
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            );
                        }
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
                                            interp.iterator_close(&iter, JsValue::Undefined);
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
                    },
                ));

                let state_ret = state.clone();
                let return_fn = interp.create_function(JsFunction::native(
                    "return".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (iter, alive) = {
                            let s = state_ret.borrow();
                            (s.0.clone(), s.4)
                        };
                        state_ret.borrow_mut().4 = false;
                        if alive {
                            interp.iterator_close(&iter, JsValue::Undefined);
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
        iter_proto
            .borrow_mut()
            .insert_builtin("filter".to_string(), filter_fn);

        // take(limit)
        let take_fn = self.create_function(JsFunction::native(
            "take".to_string(),
            1,
            |interp, this, args| {
                let limit_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let limit = interp.to_number_coerce(&limit_val);
                if limit.is_nan() || limit < 0.0 {
                    interp.iterator_close(this, JsValue::Undefined);
                    let err = interp
                        .create_error("RangeError", "take limit must be a non-negative number");
                    return Completion::Throw(err);
                }
                let limit = if limit.is_infinite() {
                    f64::INFINITY
                } else {
                    limit.trunc()
                };
                let (iter, next_method) = match interp.get_iterator_direct(this) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // state: (iter, next_method, remaining, alive)
                let state: Rc<RefCell<(JsValue, JsValue, f64, bool)>> =
                    Rc::new(RefCell::new((iter, next_method, limit, true)));

                let state_next = state.clone();
                let next_fn = interp.create_function(JsFunction::native(
                    "next".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (iter, next_method, remaining, alive) = {
                            let s = state_next.borrow();
                            (s.0.clone(), s.1.clone(), s.2, s.3)
                        };
                        if !alive {
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            );
                        }
                        if remaining <= 0.0 {
                            state_next.borrow_mut().3 = false;
                            interp.iterator_close(&iter, JsValue::Undefined);
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            );
                        }
                        state_next.borrow_mut().2 = remaining - 1.0;
                        match interp.iterator_step_direct(&iter, &next_method) {
                            Ok(Some(result)) => {
                                let value = match interp.iterator_value(&result) {
                                    Ok(v) => v,
                                    Err(e) => return Completion::Throw(e),
                                };
                                // If we just took the last one, close
                                if remaining - 1.0 <= 0.0 {
                                    state_next.borrow_mut().3 = false;
                                    interp.iterator_close(&iter, JsValue::Undefined);
                                }
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
                        if alive {
                            interp.iterator_close(&iter, JsValue::Undefined);
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
        iter_proto
            .borrow_mut()
            .insert_builtin("take".to_string(), take_fn);

        // drop(limit)
        let drop_fn = self.create_function(JsFunction::native(
            "drop".to_string(),
            1,
            |interp, this, args| {
                let limit_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let limit = interp.to_number_coerce(&limit_val);
                if limit.is_nan() || limit < 0.0 {
                    interp.iterator_close(this, JsValue::Undefined);
                    let err = interp
                        .create_error("RangeError", "drop limit must be a non-negative number");
                    return Completion::Throw(err);
                }
                let limit = if limit.is_infinite() {
                    f64::INFINITY
                } else {
                    limit.trunc()
                };
                let (iter, next_method) = match interp.get_iterator_direct(this) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // state: (iter, next_method, to_skip, skipped, alive)
                let state: Rc<RefCell<(JsValue, JsValue, f64, bool, bool)>> =
                    Rc::new(RefCell::new((iter, next_method, limit, false, true)));

                let state_next = state.clone();
                let next_fn = interp.create_function(JsFunction::native(
                    "next".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (iter, next_method, to_skip, skipped, alive) = {
                            let s = state_next.borrow();
                            (s.0.clone(), s.1.clone(), s.2, s.3, s.4)
                        };
                        if !alive {
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            );
                        }
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
                    },
                ));

                let state_ret = state.clone();
                let return_fn = interp.create_function(JsFunction::native(
                    "return".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (iter, alive) = {
                            let s = state_ret.borrow();
                            (s.0.clone(), s.4)
                        };
                        state_ret.borrow_mut().4 = false;
                        if alive {
                            interp.iterator_close(&iter, JsValue::Undefined);
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
        iter_proto
            .borrow_mut()
            .insert_builtin("drop".to_string(), drop_fn);

        // flatMap(mapper)
        let flat_map_fn = self.create_function(JsFunction::native(
            "flatMap".to_string(),
            1,
            |interp, this, args| {
                let mapper = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(&mapper, JsValue::Object(o) if interp.get_object(o.id).map(|od| od.borrow().callable.is_some()).unwrap_or(false))
                {
                    interp.iterator_close(this, JsValue::Undefined);
                    let err = interp.create_type_error("mapper is not a function");
                    return Completion::Throw(err);
                }
                let (iter, next_method) = match interp.get_iterator_direct(this) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // state: (outer_iter, outer_next, mapper, counter, inner_iter, inner_next, alive)
                let state: Rc<
                    RefCell<(
                        JsValue,
                        JsValue,
                        JsValue,
                        f64,
                        Option<JsValue>,
                        Option<JsValue>,
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
                )));

                let state_next = state.clone();
                let next_fn = interp.create_function(JsFunction::native(
                    "next".to_string(),
                    0,
                    move |interp, _this, _args| {
                        loop {
                            let (
                                outer_iter,
                                outer_next,
                                mapper,
                                counter,
                                inner_iter,
                                inner_next,
                                alive,
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
                                )
                            };
                            if !alive {
                                return Completion::Normal(
                                    interp
                                        .create_iter_result_object(JsValue::Undefined, true),
                                );
                            }

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
                                        interp
                                            .iterator_close(&outer_iter, JsValue::Undefined);
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
                                            // Try to get an iterator from mapped_val
                                            match interp.get_iterator(&mapped_val) {
                                                Ok(new_inner) => {
                                                    let inner_next_method = if let JsValue::Object(io) = &new_inner {
                                                        interp.get_object(io.id).map(|od| od.borrow().get_property("next")).unwrap_or(JsValue::Undefined)
                                                    } else {
                                                        JsValue::Undefined
                                                    };
                                                    state_next.borrow_mut().4 = Some(new_inner);
                                                    state_next.borrow_mut().5 = Some(inner_next_method);
                                                    continue;
                                                }
                                                Err(e) => {
                                                    state_next.borrow_mut().6 = false;
                                                    interp.iterator_close(
                                                        &outer_iter,
                                                        JsValue::Undefined,
                                                    );
                                                    return Completion::Throw(e);
                                                }
                                            }
                                        }
                                        Completion::Throw(e) => {
                                            state_next.borrow_mut().6 = false;
                                            interp
                                                .iterator_close(&outer_iter, JsValue::Undefined);
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
                    },
                ));

                let state_ret = state.clone();
                let return_fn = interp.create_function(JsFunction::native(
                    "return".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (outer_iter, inner_iter, alive) = {
                            let s = state_ret.borrow();
                            (s.0.clone(), s.4.clone(), s.6)
                        };
                        state_ret.borrow_mut().6 = false;
                        state_ret.borrow_mut().4 = None;
                        state_ret.borrow_mut().5 = None;
                        if alive {
                            if let Some(ref ii) = inner_iter {
                                interp.iterator_close(ii, JsValue::Undefined);
                            }
                            interp.iterator_close(&outer_iter, JsValue::Undefined);
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
        iter_proto
            .borrow_mut()
            .insert_builtin("flatMap".to_string(), flat_map_fn);
    }

    fn setup_iterator_static_methods(&mut self, iterator_ctor: &JsValue) {
        // Iterator.from(obj)
        let from_fn = self.create_function(JsFunction::native(
            "from".to_string(),
            1,
            |interp, _this, args| {
                let obj = args.first().cloned().unwrap_or(JsValue::Undefined);

                // Try Symbol.iterator first
                let sym_key = interp.get_symbol_iterator_key();
                let mut iterator = None;

                if let JsValue::Object(o) = &obj
                    && let Some(ref key) = sym_key
                    && let Some(obj_data) = interp.get_object(o.id)
                {
                    let iter_fn = obj_data.borrow().get_property(key);
                    if !matches!(iter_fn, JsValue::Undefined) {
                        match interp.call_function(&iter_fn, &obj, &[]) {
                            Completion::Normal(v) if matches!(v, JsValue::Object(_)) => {
                                iterator = Some(v);
                            }
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => {
                                let err = interp.create_type_error(
                                    "Result of the Symbol.iterator method is not an object",
                                );
                                return Completion::Throw(err);
                            }
                        }
                    }
                }

                let iter_val = if let Some(it) = iterator {
                    it
                } else {
                    // Treat obj as iterator directly — must have .next
                    if !matches!(&obj, JsValue::Object(_)) {
                        let err = interp.create_type_error("value is not an object");
                        return Completion::Throw(err);
                    }
                    obj.clone()
                };

                // Check if iter_val has %IteratorPrototype% in its chain
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

                // Wrap with a forwarding iterator
                let (iter, next_method) = match interp.get_iterator_direct(&iter_val) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let state: Rc<RefCell<(JsValue, JsValue, bool)>> =
                    Rc::new(RefCell::new((iter, next_method, true)));

                let state_next = state.clone();
                let next_fn = interp.create_function(JsFunction::native(
                    "next".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (iter, next_method, alive) = {
                            let s = state_next.borrow();
                            (s.0.clone(), s.1.clone(), s.2)
                        };
                        if !alive {
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            );
                        }
                        match interp.call_function(&next_method, &iter, &[]) {
                            Completion::Normal(v) => {
                                if !matches!(v, JsValue::Object(_)) {
                                    let err = interp
                                        .create_type_error("Iterator result is not an object");
                                    return Completion::Throw(err);
                                }
                                if interp.iterator_complete(&v) {
                                    state_next.borrow_mut().2 = false;
                                }
                                Completion::Normal(v)
                            }
                            Completion::Throw(e) => {
                                state_next.borrow_mut().2 = false;
                                Completion::Throw(e)
                            }
                            _ => Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            ),
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
                            (s.0.clone(), s.2)
                        };
                        state_ret.borrow_mut().2 = false;
                        if alive {
                            interp.iterator_close(&iter, JsValue::Undefined);
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
            obj.borrow_mut().insert_builtin("from".to_string(), from_fn);
        }

        // Iterator.concat(...iterables)
        let concat_fn = self.create_function(JsFunction::native(
            "concat".to_string(),
            1,
            |interp, _this, args| {
                // Validate all args are iterable first
                let sym_key = interp.get_symbol_iterator_key();
                let mut iterables: Vec<(JsValue, JsValue)> = Vec::new();
                for arg in args {
                    if let Some(ref key) = sym_key {
                        let iter_fn = match arg {
                            JsValue::Object(o) => interp
                                .get_object(o.id)
                                .map(|od| od.borrow().get_property(key))
                                .unwrap_or(JsValue::Undefined),
                            JsValue::String(_) => {
                                let str_proto = interp.string_prototype.clone();
                                str_proto
                                    .map(|p| p.borrow().get_property(key))
                                    .unwrap_or(JsValue::Undefined)
                            }
                            _ => JsValue::Undefined,
                        };
                        if matches!(iter_fn, JsValue::Undefined) {
                            let err = interp.create_type_error("value is not iterable");
                            return Completion::Throw(err);
                        }
                        iterables.push((arg.clone(), iter_fn));
                    } else {
                        let err = interp.create_type_error("value is not iterable");
                        return Completion::Throw(err);
                    }
                }

                // state: (iterables, current_index, current_iter, current_next, alive)
                let state: Rc<
                    RefCell<(
                        Vec<(JsValue, JsValue)>,
                        usize,
                        Option<JsValue>,
                        Option<JsValue>,
                        bool,
                    )>,
                > = Rc::new(RefCell::new((iterables, 0, None, None, true)));

                let state_next = state.clone();
                let next_fn = interp.create_function(JsFunction::native(
                    "next".to_string(),
                    0,
                    move |interp, _this, _args| {
                        loop {
                            let (ref iterables, idx, ref cur_iter, ref cur_next, alive) = {
                                let s = state_next.borrow();
                                (s.0.clone(), s.1, s.2.clone(), s.3.clone(), s.4)
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
                                    let next_method = if let JsValue::Object(io) = &new_iter {
                                        interp
                                            .get_object(io.id)
                                            .map(|od| od.borrow().get_property("next"))
                                            .unwrap_or(JsValue::Undefined)
                                    } else {
                                        JsValue::Undefined
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
                    },
                ));

                let state_ret = state.clone();
                let return_fn = interp.create_function(JsFunction::native(
                    "return".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let (cur_iter, alive) = {
                            let s = state_ret.borrow();
                            (s.2.clone(), s.4)
                        };
                        state_ret.borrow_mut().4 = false;
                        state_ret.borrow_mut().2 = None;
                        state_ret.borrow_mut().3 = None;
                        if alive && let Some(ref ci) = cur_iter {
                            interp.iterator_close(ci, JsValue::Undefined);
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
                .insert_builtin("concat".to_string(), concat_fn);
        }

        // Iterator.zip(iterables [, options])
        let zip_fn = self.create_function(JsFunction::native(
            "zip".to_string(),
            1,
            |interp, _this, args| {
                let iterables_arg = args.first().cloned().unwrap_or(JsValue::Undefined);

                // Step 1: If iterables is not an Object, throw TypeError
                if !matches!(&iterables_arg, JsValue::Object(_)) {
                    let err = interp.create_type_error("iterables is not an object");
                    return Completion::Throw(err);
                }

                // Get mode from options
                let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let mode = if matches!(options, JsValue::Undefined) {
                    "shortest".to_string()
                } else if let JsValue::Object(o) = &options {
                    if let Some(od) = interp.get_object(o.id) {
                        let mode_val = od.borrow().get_property("mode");
                        if matches!(mode_val, JsValue::Undefined) {
                            "shortest".to_string()
                        } else {
                            let s = to_js_string(&mode_val);
                            match s.as_str() {
                                "shortest" | "longest" | "strict" => s,
                                _ => {
                                    let err = interp.create_error("RangeError",
                                        "mode must be 'shortest', 'longest', or 'strict'");
                                    return Completion::Throw(err);
                                }
                            }
                        }
                    } else {
                        "shortest".to_string()
                    }
                } else {
                    let err = interp.create_type_error("options must be an object or undefined");
                    return Completion::Throw(err);
                };

                // Get padding from options (for "longest" mode)
                let padding = if mode == "longest" {
                    if let JsValue::Object(o) = &options {
                        if let Some(od) = interp.get_object(o.id) {
                            let p = od.borrow().get_property("padding");
                            if matches!(p, JsValue::Undefined) {
                                None
                            } else {
                                Some(p)
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

                // iterables must be iterable - get iterator from it
                let iter_of_iterables = match interp.get_iterator(&iterables_arg) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // Collect all iterables into (iterator, next_method) pairs
                let mut iters: Vec<(JsValue, JsValue)> = Vec::new();
                let iter_next = if let JsValue::Object(io) = &iter_of_iterables {
                    interp.get_object(io.id)
                        .map(|od| od.borrow().get_property("next"))
                        .unwrap_or(JsValue::Undefined)
                } else {
                    JsValue::Undefined
                };

                loop {
                    match interp.iterator_step_direct(&iter_of_iterables, &iter_next) {
                        Ok(Some(result)) => {
                            let iterable = match interp.iterator_value(&result) {
                                Ok(v) => v,
                                Err(e) => return Completion::Throw(e),
                            };
                            match interp.get_iterator(&iterable) {
                                Ok(it) => {
                                    let nm = if let JsValue::Object(io) = &it {
                                        interp.get_object(io.id)
                                            .map(|od| od.borrow().get_property("next"))
                                            .unwrap_or(JsValue::Undefined)
                                    } else {
                                        JsValue::Undefined
                                    };
                                    iters.push((it, nm));
                                }
                                Err(e) => {
                                    // Close all already-opened iterators
                                    for (it, _) in &iters {
                                        interp.iterator_close(it, JsValue::Undefined);
                                    }
                                    return Completion::Throw(e);
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            for (it, _) in &iters {
                                interp.iterator_close(it, JsValue::Undefined);
                            }
                            return Completion::Throw(e);
                        }
                    }
                }

                // Collect padding values (for longest mode)
                let padding_values: Vec<JsValue> = if let Some(ref pad_iterable) = padding {
                    let mut pads = Vec::new();
                    match interp.get_iterator(pad_iterable) {
                        Ok(pad_iter) => {
                            let pad_next = if let JsValue::Object(io) = &pad_iter {
                                interp.get_object(io.id)
                                    .map(|od| od.borrow().get_property("next"))
                                    .unwrap_or(JsValue::Undefined)
                            } else {
                                JsValue::Undefined
                            };
                            loop {
                                match interp.iterator_step_direct(&pad_iter, &pad_next) {
                                    Ok(Some(result)) => match interp.iterator_value(&result) {
                                        Ok(v) => pads.push(v),
                                        Err(e) => {
                                            for (it, _) in &iters {
                                                interp.iterator_close(it, JsValue::Undefined);
                                            }
                                            return Completion::Throw(e);
                                        }
                                    },
                                    Ok(None) => break,
                                    Err(e) => {
                                        for (it, _) in &iters {
                                            interp.iterator_close(it, JsValue::Undefined);
                                        }
                                        return Completion::Throw(e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            for (it, _) in &iters {
                                interp.iterator_close(it, JsValue::Undefined);
                            }
                            return Completion::Throw(e);
                        }
                    }
                    pads
                } else {
                    Vec::new()
                };

                let num_iters = iters.len();
                // state: (iters, exhausted, mode, padding_values, alive)
                let state: Rc<RefCell<(Vec<(JsValue, JsValue)>, Vec<bool>, String, Vec<JsValue>, bool)>> =
                    Rc::new(RefCell::new((iters, vec![false; num_iters], mode, padding_values, true)));

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
                        let mut any_done = false;
                        let mut all_done = true;

                        for (i, (it, nm)) in iters.iter().enumerate() {
                            if exhausted[i] {
                                let pad = padding_values.get(i).cloned().unwrap_or(JsValue::Undefined);
                                values.push(pad);
                                continue;
                            }
                            all_done = false;
                            match interp.iterator_step_direct(it, nm) {
                                Ok(Some(result)) => match interp.iterator_value(&result) {
                                    Ok(v) => values.push(v),
                                    Err(e) => return Completion::Throw(e),
                                },
                                Ok(None) => {
                                    any_done = true;
                                    new_exhausted[i] = true;
                                    let pad = padding_values.get(i).cloned().unwrap_or(JsValue::Undefined);
                                    values.push(pad);
                                }
                                Err(e) => {
                                    state_next.borrow_mut().4 = false;
                                    // Close remaining non-exhausted iterators
                                    for (j, (jt, _)) in iters.iter().enumerate() {
                                        if j != i && !new_exhausted[j] {
                                            interp.iterator_close(jt, JsValue::Undefined);
                                        }
                                    }
                                    return Completion::Throw(e);
                                }
                            }
                        }

                        if all_done {
                            state_next.borrow_mut().4 = false;
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            );
                        }

                        state_next.borrow_mut().1 = new_exhausted.clone();

                        match mode.as_str() {
                            "shortest" => {
                                if any_done {
                                    state_next.borrow_mut().4 = false;
                                    // Close non-exhausted iterators
                                    for (i, (it, _)) in iters.iter().enumerate() {
                                        if !new_exhausted[i] {
                                            interp.iterator_close(it, JsValue::Undefined);
                                        }
                                    }
                                    return Completion::Normal(
                                        interp.create_iter_result_object(JsValue::Undefined, true),
                                    );
                                }
                            }
                            "longest" => {
                                if new_exhausted.iter().all(|e| *e) {
                                    state_next.borrow_mut().4 = false;
                                    return Completion::Normal(
                                        interp.create_iter_result_object(JsValue::Undefined, true),
                                    );
                                }
                            }
                            "strict" => {
                                if any_done {
                                    // Check if all are done
                                    if !new_exhausted.iter().all(|e| *e) {
                                        state_next.borrow_mut().4 = false;
                                        // Close remaining non-exhausted
                                        for (i, (it, _)) in iters.iter().enumerate() {
                                            if !new_exhausted[i] {
                                                interp.iterator_close(it, JsValue::Undefined);
                                            }
                                        }
                                        let err = interp.create_type_error(
                                            "Iterators passed to Iterator.zip with { mode: \"strict\" } have different lengths",
                                        );
                                        return Completion::Throw(err);
                                    }
                                    state_next.borrow_mut().4 = false;
                                    return Completion::Normal(
                                        interp.create_iter_result_object(JsValue::Undefined, true),
                                    );
                                }
                            }
                            _ => {}
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
                            for (i, (it, _)) in iters.iter().enumerate() {
                                if !exhausted[i] {
                                    interp.iterator_close(it, JsValue::Undefined);
                                }
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

                // Must be an object
                let obj_id = match &iterables_obj {
                    JsValue::Object(o) => o.id,
                    _ => {
                        let err = interp.create_type_error("iterables must be an object");
                        return Completion::Throw(err);
                    }
                };

                // Get mode from options
                let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let mode = if matches!(options, JsValue::Undefined) {
                    "shortest".to_string()
                } else if let JsValue::Object(o) = &options {
                    if let Some(od) = interp.get_object(o.id) {
                        let mode_val = od.borrow().get_property("mode");
                        if matches!(mode_val, JsValue::Undefined) {
                            "shortest".to_string()
                        } else {
                            let s = to_js_string(&mode_val);
                            match s.as_str() {
                                "shortest" | "longest" | "strict" => s,
                                _ => {
                                    let err = interp.create_error("RangeError",
                                        "mode must be 'shortest', 'longest', or 'strict'");
                                    return Completion::Throw(err);
                                }
                            }
                        }
                    } else {
                        "shortest".to_string()
                    }
                } else {
                    let err = interp.create_type_error("options must be an object or undefined");
                    return Completion::Throw(err);
                };

                // Get padding from options (for "longest" mode)
                let padding_obj = if mode == "longest" {
                    if let JsValue::Object(o) = &options {
                        if let Some(od) = interp.get_object(o.id) {
                            let p = od.borrow().get_property("padding");
                            if matches!(p, JsValue::Undefined) {
                                None
                            } else {
                                Some(p)
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

                // Get own enumerable string keys from iterables object
                let keys: Vec<String> = if let Some(od) = interp.get_object(obj_id) {
                    let borrowed = od.borrow();
                    borrowed.property_order.iter()
                        .filter(|k| {
                            borrowed.properties.get(*k)
                                .and_then(|pd| pd.enumerable)
                                .unwrap_or(false)
                        })
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                };

                // Open iterators for each key
                let mut key_names: Vec<String> = Vec::new();
                let mut iters: Vec<(JsValue, JsValue)> = Vec::new();

                for key in &keys {
                    let iterable = if let Some(od) = interp.get_object(obj_id) {
                        od.borrow().get_property(key)
                    } else {
                        JsValue::Undefined
                    };
                    match interp.get_iterator(&iterable) {
                        Ok(it) => {
                            let nm = if let JsValue::Object(io) = &it {
                                interp.get_object(io.id)
                                    .map(|od| od.borrow().get_property("next"))
                                    .unwrap_or(JsValue::Undefined)
                            } else {
                                JsValue::Undefined
                            };
                            key_names.push(key.clone());
                            iters.push((it, nm));
                        }
                        Err(e) => {
                            for (it, _) in &iters {
                                interp.iterator_close(it, JsValue::Undefined);
                            }
                            return Completion::Throw(e);
                        }
                    }
                }

                // Get padding values per key
                let padding_values: Vec<JsValue> = if let Some(ref pad_obj_val) = padding_obj {
                    if let JsValue::Object(po) = pad_obj_val {
                        key_names.iter().map(|k| {
                            interp.get_object(po.id)
                                .map(|od| od.borrow().get_property(k))
                                .unwrap_or(JsValue::Undefined)
                        }).collect()
                    } else {
                        vec![JsValue::Undefined; key_names.len()]
                    }
                } else {
                    vec![JsValue::Undefined; key_names.len()]
                };

                let num_iters = iters.len();
                // state: (key_names, iters, exhausted, mode, padding_values, alive)
                let state: Rc<RefCell<(Vec<String>, Vec<(JsValue, JsValue)>, Vec<bool>, String, Vec<JsValue>, bool)>> =
                    Rc::new(RefCell::new((key_names, iters, vec![false; num_iters], mode, padding_values, true)));

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
                        let mut any_done = false;
                        let mut all_done = true;

                        for (i, (it, nm)) in iters.iter().enumerate() {
                            if exhausted[i] {
                                let pad = padding_values.get(i).cloned().unwrap_or(JsValue::Undefined);
                                values.push((keys[i].clone(), pad));
                                continue;
                            }
                            all_done = false;
                            match interp.iterator_step_direct(it, nm) {
                                Ok(Some(result)) => match interp.iterator_value(&result) {
                                    Ok(v) => values.push((keys[i].clone(), v)),
                                    Err(e) => return Completion::Throw(e),
                                },
                                Ok(None) => {
                                    any_done = true;
                                    new_exhausted[i] = true;
                                    let pad = padding_values.get(i).cloned().unwrap_or(JsValue::Undefined);
                                    values.push((keys[i].clone(), pad));
                                }
                                Err(e) => {
                                    state_next.borrow_mut().5 = false;
                                    for (j, (jt, _)) in iters.iter().enumerate() {
                                        if j != i && !new_exhausted[j] {
                                            interp.iterator_close(jt, JsValue::Undefined);
                                        }
                                    }
                                    return Completion::Throw(e);
                                }
                            }
                        }

                        if all_done {
                            state_next.borrow_mut().5 = false;
                            return Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            );
                        }

                        state_next.borrow_mut().2 = new_exhausted.clone();

                        match mode.as_str() {
                            "shortest" => {
                                if any_done {
                                    state_next.borrow_mut().5 = false;
                                    for (i, (it, _)) in iters.iter().enumerate() {
                                        if !new_exhausted[i] {
                                            interp.iterator_close(it, JsValue::Undefined);
                                        }
                                    }
                                    return Completion::Normal(
                                        interp.create_iter_result_object(JsValue::Undefined, true),
                                    );
                                }
                            }
                            "longest" => {
                                if new_exhausted.iter().all(|e| *e) {
                                    state_next.borrow_mut().5 = false;
                                    return Completion::Normal(
                                        interp.create_iter_result_object(JsValue::Undefined, true),
                                    );
                                }
                            }
                            "strict" => {
                                if any_done {
                                    if !new_exhausted.iter().all(|e| *e) {
                                        state_next.borrow_mut().5 = false;
                                        for (i, (it, _)) in iters.iter().enumerate() {
                                            if !new_exhausted[i] {
                                                interp.iterator_close(it, JsValue::Undefined);
                                            }
                                        }
                                        let err = interp.create_type_error(
                                            "Iterators passed to Iterator.zipKeyed with { mode: \"strict\" } have different lengths",
                                        );
                                        return Completion::Throw(err);
                                    }
                                    state_next.borrow_mut().5 = false;
                                    return Completion::Normal(
                                        interp.create_iter_result_object(JsValue::Undefined, true),
                                    );
                                }
                            }
                            _ => {}
                        }

                        // Create result object with key-value pairs
                        let result_obj = interp.create_object();
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
                            for (i, (it, _)) in iters.iter().enumerate() {
                                if !exhausted[i] {
                                    interp.iterator_close(it, JsValue::Undefined);
                                }
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
                if let JsValue::Object(o) = this {
                    if let Some(obj_rc) = interp.get_object(o.id) {
                        let is_state_machine = matches!(
                            obj_rc.borrow().iterator_state,
                            Some(IteratorState::StateMachineGenerator { .. })
                        );
                        if is_state_machine {
                            return interp.generator_next_state_machine(this, value);
                        }
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
                if let JsValue::Object(o) = this {
                    if let Some(obj_rc) = interp.get_object(o.id) {
                        let is_state_machine = matches!(
                            obj_rc.borrow().iterator_state,
                            Some(IteratorState::StateMachineGenerator { .. })
                        );
                        if is_state_machine {
                            return interp.generator_return_state_machine(this, value);
                        }
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
                if let JsValue::Object(o) = this {
                    if let Some(obj_rc) = interp.get_object(o.id) {
                        let is_state_machine = matches!(
                            obj_rc.borrow().iterator_state,
                            Some(IteratorState::StateMachineGenerator { .. })
                        );
                        if is_state_machine {
                            return interp.generator_throw_state_machine(this, exception);
                        }
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
        if let Some(func_val) = self.global_env.borrow().get("Function") {
            if let JsValue::Object(func_obj) = func_val {
                if let Some(func_data) = self.get_object(func_obj.id) {
                    if let JsValue::Object(func_proto_obj) =
                        func_data.borrow().get_property("prototype")
                    {
                        if let Some(func_proto) = self.get_object(func_proto_obj.id) {
                            gf_proto.borrow_mut().prototype = Some(func_proto);
                        }
                    }
                }
            }
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
