use super::super::*;

/// §24.1.3 RequireInternalSlot(M, [[MapData]]): confirm `this` is a Map receiver
/// — an object carrying Map data under the "Map" brand (a WeakMap shares the
/// Map data kind but is branded "WeakMap", so it is rejected). This is the
/// sibling of `string.rs`'s `this_string_value` / `this_js_string`: it
/// concentrates the receiver brand-check that every `Map.prototype` method would
/// otherwise open-code, returning the receiver's object id and cell, or a
/// TypeError completion naming the method. The cell is an owned handle (via
/// `get_object`) so callers may re-enter the interpreter — e.g. `forEach`
/// invoking a callback — while holding it.
fn this_map(
    interp: &mut Interpreter,
    this: &JsValue,
    method: &str,
) -> Result<(u64, ObjectHandle), Completion> {
    if let JsValue::Object(o) = this
        && let Some(obj) = interp.get_object(o.id)
        && {
            let b = obj.borrow();
            b.map_data().is_some() && b.class_name == "Map"
        }
    {
        return Ok((o.id, obj));
    }
    Err(Completion::Throw(interp.create_type_error(&format!(
        "Map.prototype.{method} requires a Map"
    ))))
}

/// §24.2.3 RequireInternalSlot(S, [[SetData]]): confirm `this` is a Set receiver
/// — an object carrying Set data that is not branded "WeakSet". The sibling of
/// [`this_map`]; see its documentation for the ownership rationale.
fn this_set(
    interp: &mut Interpreter,
    this: &JsValue,
    method: &str,
) -> Result<(u64, ObjectHandle), Completion> {
    if let JsValue::Object(o) = this
        && let Some(obj) = interp.get_object(o.id)
        && {
            let b = obj.borrow();
            b.set_data().is_some() && b.class_name != "WeakSet"
        }
    {
        return Ok((o.id, obj));
    }
    Err(Completion::Throw(interp.create_type_error(&format!(
        "Set.prototype.{method} requires a Set"
    ))))
}

impl Interpreter {
    pub(crate) fn setup_map_prototype(&mut self) {
        let proto_id = self.create_object_id();
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .class_name = "Map".to_string();

        // Map iterator prototype
        let map_iter_proto_id = self.create_object_id();
        self.get_object_cell_expect(map_iter_proto_id)
            .borrow_mut()
            .prototype_id = self.realm().iterator_prototype;
        self.get_object_cell_expect(map_iter_proto_id)
            .borrow_mut()
            .class_name = "Map Iterator".to_string();

        self.define_to_string_tag(map_iter_proto_id, "Map Iterator");

        self.define_method(map_iter_proto_id, "next", 0, |interp, this, _args| {
            if let JsValue::Object(o) = this
                && let Some(obj) = interp.get_object(o.id)
            {
                let state = obj.borrow().iterator_state().cloned();
                if let Some(IteratorState::MapIterator {
                    map_id,
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
                    if let Some(map_obj) = interp.get_object(map_id) {
                        let map_data = map_obj.borrow().map_data().cloned();
                        if let Some(entries) = map_data {
                            let mut i = index;
                            while i < entries.len() {
                                if let Some(ref entry) = entries[i] {
                                    let result = match kind {
                                        IteratorKind::Key => entry.0.clone(),
                                        IteratorKind::Value => entry.1.clone(),
                                        IteratorKind::KeyValue => interp
                                            .create_array(vec![entry.0.clone(), entry.1.clone()]),
                                    };
                                    obj.borrow_mut().kind =
                                        crate::interpreter::types::ObjectKind::Iterator(
                                            IteratorState::MapIterator {
                                                map_id,
                                                index: i + 1,
                                                kind,
                                                done: false,
                                            },
                                        );
                                    return Completion::Normal(
                                        interp.create_iter_result_object(result, false),
                                    );
                                }
                                i += 1;
                            }
                        }
                    }
                    obj.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::MapIterator {
                            map_id,
                            index,
                            kind,
                            done: true,
                        },
                    );
                    return Completion::Normal(
                        interp.create_iter_result_object(JsValue::Undefined, true),
                    );
                }
            }
            let err =
                interp.create_type_error("Map Iterator.prototype.next requires a Map Iterator");
            Completion::Throw(err)
        });

        if let Some(key) = self.get_symbol_iterator_key() {
            let iter_self_fn = self.create_function(JsFunction::native(
                "[Symbol.iterator]".to_string(),
                0,
                |_interp, this, _args| Completion::Normal(this.clone()),
            ));
            self.get_object_cell_expect(map_iter_proto_id)
                .borrow_mut()
                .insert_property(
                    key,
                    PropertyDescriptor::data(iter_self_fn, true, false, true),
                );
        }

        self.realm_mut().map_iterator_prototype = Some(map_iter_proto_id);

        // Helper to create map iterators
        fn create_map_iterator(
            interp: &mut Interpreter,
            map_id: u64,
            kind: IteratorKind,
        ) -> JsValue {
            let mut obj_data = JsObjectData::new();
            obj_data.prototype_id = interp
                .realm()
                .map_iterator_prototype
                .or(interp.realm().iterator_prototype)
                .or(interp.realm().object_prototype);
            obj_data.class_name = "Map Iterator".to_string();
            obj_data.kind =
                crate::interpreter::types::ObjectKind::Iterator(IteratorState::MapIterator {
                    map_id,
                    index: 0,
                    kind,
                    done: false,
                });
            let id = interp.alloc_object(obj_data);
            JsValue::Object(crate::types::JsObject { id })
        }

        // Map.prototype.entries
        let entries_fn = self.define_method(proto_id, "entries", 0, |interp, this, _args| {
            let (id, _obj) = match this_map(interp, this, "entries") {
                Ok(t) => t,
                Err(c) => return c,
            };
            Completion::Normal(create_map_iterator(interp, id, IteratorKind::KeyValue))
        });

        // Map.prototype[@@iterator] = entries
        if let Some(key) = self.get_symbol_iterator_key() {
            self.get_object_cell_expect(proto_id)
                .borrow_mut()
                .insert_property(key, PropertyDescriptor::data(entries_fn, true, false, true));
        }

        // Map.prototype.keys
        self.define_method(proto_id, "keys", 0, |interp, this, _args| {
            let (id, _obj) = match this_map(interp, this, "keys") {
                Ok(t) => t,
                Err(c) => return c,
            };
            Completion::Normal(create_map_iterator(interp, id, IteratorKind::Key))
        });

        // Map.prototype.values
        self.define_method(proto_id, "values", 0, |interp, this, _args| {
            let (id, _obj) = match this_map(interp, this, "values") {
                Ok(t) => t,
                Err(c) => return c,
            };
            Completion::Normal(create_map_iterator(interp, id, IteratorKind::Value))
        });

        // Map.prototype.get
        self.define_method(proto_id, "get", 1, |interp, this, args| {
            let (_id, obj) = match this_map(interp, this, "get") {
                Ok(t) => t,
                Err(c) => return c,
            };
            let entries = obj.borrow().map_data().cloned().unwrap();
            let key = args.first().cloned().unwrap_or(JsValue::Undefined);
            for entry in entries.iter().flatten() {
                if same_value_zero(&entry.0, &key) {
                    return Completion::Normal(entry.1.clone());
                }
            }
            Completion::Normal(JsValue::Undefined)
        });

        // Map.prototype.set
        self.define_method(proto_id, "set", 2, |interp, this, args| {
            if let JsValue::Object(o) = this
                && let Some(obj) = interp.get_object_cell(o.id)
            {
                let has_map = {
                    let b = obj.borrow();
                    b.map_data().is_some() && b.class_name == "Map"
                };
                if has_map {
                    let mut key = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let value = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    // Normalize -0 to +0
                    if let JsValue::Number(n) = &key
                        && *n == 0.0
                        && n.is_sign_negative()
                    {
                        key = JsValue::Number(0.0);
                    }
                    let mut borrowed = obj.borrow_mut();
                    let entries = borrowed.map_data_mut().unwrap();
                    for entry in entries.iter_mut().flatten() {
                        if same_value_zero(&entry.0, &key) {
                            entry.1 = value;
                            return Completion::Normal(this.clone());
                        }
                    }
                    entries.push(Some((key, value)));
                    return Completion::Normal(this.clone());
                }
            }
            let err = interp.create_type_error("Map.prototype.set requires a Map");
            Completion::Throw(err)
        });

        // Map.prototype.has
        self.define_method(proto_id, "has", 1, |interp, this, args| {
            let (_id, obj) = match this_map(interp, this, "has") {
                Ok(t) => t,
                Err(c) => return c,
            };
            let entries = obj.borrow().map_data().cloned().unwrap();
            let key = args.first().cloned().unwrap_or(JsValue::Undefined);
            for entry in entries.iter().flatten() {
                if same_value_zero(&entry.0, &key) {
                    return Completion::Normal(JsValue::Boolean(true));
                }
            }
            Completion::Normal(JsValue::Boolean(false))
        });

        // Map.prototype.delete
        self.define_method(proto_id, "delete", 1, |interp, this, args| {
            let (_id, obj) = match this_map(interp, this, "delete") {
                Ok(t) => t,
                Err(c) => return c,
            };
            let key = args.first().cloned().unwrap_or(JsValue::Undefined);
            let mut borrowed = obj.borrow_mut();
            let entries = borrowed.map_data_mut().unwrap();
            for entry in entries.iter_mut() {
                let matches = entry.as_ref().is_some_and(|e| same_value_zero(&e.0, &key));
                if matches {
                    *entry = None;
                    return Completion::Normal(JsValue::Boolean(true));
                }
            }
            Completion::Normal(JsValue::Boolean(false))
        });

        // Map.prototype.clear
        self.define_method(proto_id, "clear", 0, |interp, this, _args| {
            let (_id, obj) = match this_map(interp, this, "clear") {
                Ok(t) => t,
                Err(c) => return c,
            };
            obj.borrow_mut().kind = crate::interpreter::types::ObjectKind::Map(Vec::new());
            Completion::Normal(JsValue::Undefined)
        });

        // Map.prototype.forEach
        self.define_method(proto_id, "forEach", 1,
            |interp, this, args| {
                let (_id, obj) = match this_map(interp, this, "forEach") {
                    Ok(t) => t,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(&callback, JsValue::Object(co) if interp.get_object(co.id).is_some_and(|o| o.borrow().callable.is_some())) {
                    let err = interp.create_type_error("Map.prototype.forEach callback is not a function");
                    return Completion::Throw(err);
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let mut i = 0;
                loop {
                    let entry = {
                        let borrowed = obj.borrow();
                        let entries = borrowed.map_data().unwrap();
                        if i >= entries.len() { break; }
                        entries[i].clone()
                    };
                    if let Some((k, v)) = entry {
                        let result = interp.call_function(&callback, &this_arg, &[v, k, this.clone()]);
                        if result.is_abrupt() { return result; }
                    }
                    i += 1;
                }
                Completion::Normal(JsValue::Undefined)
            },
        );

        // Map.prototype.size (getter)
        self.define_getter(proto_id, "size", |interp, this, _args| {
            let (_id, obj) = match this_map(interp, this, "size") {
                Ok(t) => t,
                Err(c) => return c,
            };
            let count = obj
                .borrow()
                .map_data()
                .unwrap()
                .iter()
                .filter(|e| e.is_some())
                .count();
            Completion::Normal(JsValue::Number(count as f64))
        });

        // Map.prototype.getOrInsert
        self.define_method(proto_id, "getOrInsert", 2, |interp, this, args| {
            let (_id, obj) = match this_map(interp, this, "getOrInsert") {
                Ok(t) => t,
                Err(c) => return c,
            };
            let mut key = args.first().cloned().unwrap_or(JsValue::Undefined);
            let value = args.get(1).cloned().unwrap_or(JsValue::Undefined);
            // CanonicalizeKeyedCollectionKey: normalize -0 to +0
            if let JsValue::Number(n) = &key
                && *n == 0.0
                && n.is_sign_negative()
            {
                key = JsValue::Number(0.0);
            }
            // Search existing entries
            {
                let borrowed = obj.borrow();
                let entries = borrowed.map_data().unwrap();
                for entry in entries.iter().flatten() {
                    if same_value_zero(&entry.0, &key) {
                        return Completion::Normal(entry.1.clone());
                    }
                }
            }
            // Key not found - append new entry
            let mut borrowed = obj.borrow_mut();
            let entries = borrowed.map_data_mut().unwrap();
            entries.push(Some((key, value.clone())));
            Completion::Normal(value)
        });

        // Map.prototype.getOrInsertComputed
        self.define_method(proto_id, "getOrInsertComputed", 2,
            |interp, this, args| {
                let (_id, obj) = match this_map(interp, this, "getOrInsertComputed") {
                    Ok(t) => t,
                    Err(c) => return c,
                };
                let mut key = args.first().cloned().unwrap_or(JsValue::Undefined);
                let callbackfn = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                // Step 3: IsCallable check BEFORE anything else
                if !matches!(&callbackfn, JsValue::Object(co) if interp.get_object_cell(co.id).is_some_and(|o| o.borrow().callable.is_some())) {
                    let err = interp.create_type_error("callbackfn is not a function");
                    return Completion::Throw(err);
                }
                // CanonicalizeKeyedCollectionKey: normalize -0 to +0
                if let JsValue::Number(n) = &key
                    && *n == 0.0 && n.is_sign_negative() {
                        key = JsValue::Number(0.0);
                    }
                // Step 5: Search existing entries
                {
                    let borrowed = obj.borrow();
                    let entries = borrowed.map_data().unwrap();
                    for entry in entries.iter().flatten() {
                        if same_value_zero(&entry.0, &key) {
                            return Completion::Normal(entry.1.clone());
                        }
                    }
                }
                // Step 6: Call(callbackfn, undefined, « key »)
                let value = match interp.call_function(&callbackfn, &JsValue::Undefined, &[key.clone()]) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                // Step 7: Re-check if key was inserted by callback
                {
                    let mut borrowed = obj.borrow_mut();
                    let entries = borrowed.map_data_mut().unwrap();
                    for entry in entries.iter_mut().flatten() {
                        if same_value_zero(&entry.0, &key) {
                            entry.1 = value.clone();
                            return Completion::Normal(value);
                        }
                    }
                    // Step 8-9: Not found, append
                    entries.push(Some((key, value.clone())));
                }
                Completion::Normal(value)
            },
        );

        // @@toStringTag
        self.define_to_string_tag(proto_id, "Map");

        // constructor property
        let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });

        // Map constructor
        let map_proto_clone_id = proto_id;
        let map_ctor = self.create_function(JsFunction::constructor(
            "Map".to_string(),
            0,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    let err = interp.create_type_error("Constructor Map requires 'new'");
                    return Completion::Throw(err);
                }

                // OrdinaryCreateFromConstructor — realm-aware prototype
                let proto = match interp.get_prototype_from_new_target_realm(|realm| {
                    realm.map_prototype
                }) {
                    Ok(p) => p.unwrap_or(map_proto_clone_id),
                    Err(e) => return Completion::Throw(e),
                };
                let obj_id = interp.create_object_id();
                interp.get_object_cell_expect(obj_id).borrow_mut().prototype_id = Some(proto);
                interp.get_object_cell_expect(obj_id).borrow_mut().class_name = "Map".to_string();
                interp.get_object_cell_expect(obj_id).borrow_mut().kind = crate::interpreter::types::ObjectKind::Map(Vec::new());
                let this_val = JsValue::Object(crate::types::JsObject { id: obj_id });

                let iterable = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !iterable.is_undefined() && !iterable.is_null() {
                    // Step 7a: Get adder = Get(map, "set") — must invoke getters
                    let adder = match interp.get_object_property(obj_id, "set", &this_val) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if !matches!(&adder, JsValue::Object(ao) if interp.get_object_cell(ao.id).is_some_and(|o| o.borrow().callable.is_some())) {
                        let err = interp.create_type_error("Map.prototype.set is not a function");
                        return Completion::Throw(err);
                    }

                    // Get iterator from iterable
                    let iterator = match interp.get_iterator(&iterable) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };

                    // Iterate
                    loop {
                        let next = match interp.iterator_step(&iterator) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };
                        let next = match next {
                            Some(v) => v,
                            None => break,
                        };

                        let value = match interp.iterator_value(&next) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };

                        // value should be [key, value] — must be Object
                        if !matches!(&value, JsValue::Object(_)) {
                            let err = interp.create_type_error("Iterator value is not an object");
                            let _ = interp.iterator_close(&iterator, err.clone());
                            return Completion::Throw(err);
                        }

                        // Get(nextItem, "0") — invoke getters, close on abrupt
                        let val_id = if let JsValue::Object(vo) = &value { vo.id } else { unreachable!() };
                        let k = match interp.get_object_property(val_id, "0", &value) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                let _ = interp.iterator_close(&iterator, e.clone());
                                return Completion::Throw(e);
                            }
                            other => return other,
                        };
                        // Get(nextItem, "1") — invoke getters, close on abrupt
                        let v = match interp.get_object_property(val_id, "1", &value) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                let _ = interp.iterator_close(&iterator, e.clone());
                                return Completion::Throw(e);
                            }
                            other => return other,
                        };

                        // Call(adder, map, « k, v ») — close on abrupt
                        match interp.call_function(&adder, &this_val, &[k, v]) {
                            Completion::Normal(_) => {}
                            Completion::Throw(e) => {
                                let _ = interp.iterator_close(&iterator, e.clone());
                                return Completion::Throw(e);
                            }
                            other => return other,
                        }
                    }
                }

                Completion::Normal(this_val)
            },
        ));

        // Set Map.prototype on ctor, ctor on prototype
        if let JsValue::Object(ctor_obj) = &map_ctor
            && let Some(obj) = self.get_object_cell(ctor_obj.id)
        {
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val.clone(), false, false, false),
            );
        }
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_property(
                "constructor".to_string(),
                PropertyDescriptor::data(map_ctor.clone(), true, false, true),
            );

        // Map[Symbol.species] getter
        if let JsValue::Object(ref ctor_ref) = map_ctor
            && let Some(ctor_obj) = self.get_object(ctor_ref.id)
        {
            let species_getter = self.create_function(JsFunction::native(
                "get [Symbol.species]".to_string(),
                0,
                |_interp, this_val, _args| Completion::Normal(this_val.clone()),
            ));
            ctor_obj.borrow_mut().insert_property(
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
        }

        // Map.groupBy static method
        if let JsValue::Object(ref ctor_ref) = map_ctor
            && let Some(ctor_obj) = self.get_object(ctor_ref.id)
        {
            let map_proto_for_groupby = proto_id;
            let group_by_fn = self.create_function(JsFunction::native(
                "groupBy".to_string(),
                2,
                move |interp, _this, args| {
                    let items = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let callback = args.get(1).cloned().unwrap_or(JsValue::Undefined);

                    // 1. Validate callback is callable
                    if !matches!(&callback, JsValue::Object(o) if interp.get_object_cell(o.id)
                        .map(|obj| obj.borrow().callable.is_some()).unwrap_or(false))
                    {
                        return Completion::Throw(
                            interp.create_type_error("callbackfn is not a function"),
                        );
                    }

                    // 2. Get iterator from items
                    let iterator = match interp.get_iterator(&items) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };

                    // 3. Create result Map
                    let result_map_id = interp.create_object_id();
                    interp
                        .get_object_cell_expect(result_map_id)
                        .borrow_mut()
                        .prototype_id = Some(map_proto_for_groupby);
                    interp
                        .get_object_cell_expect(result_map_id)
                        .borrow_mut()
                        .class_name = "Map".to_string();
                    interp
                        .get_object_cell_expect(result_map_id)
                        .borrow_mut()
                        .kind = crate::interpreter::types::ObjectKind::Map(Vec::new());
                    let result_id = result_map_id;
                    let result_val = JsValue::Object(crate::types::JsObject { id: result_id });

                    // 4. Iterate and group
                    let mut k: u64 = 0;
                    loop {
                        let next = match interp.iterator_step(&iterator) {
                            Ok(Some(v)) => v,
                            Ok(None) => break,
                            Err(e) => return Completion::Throw(e),
                        };
                        let value = match interp.iterator_value(&next) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };

                        // Call callback with (value, index)
                        let key_val = match interp.call_function(
                            &callback,
                            &JsValue::Undefined,
                            &[value.clone(), JsValue::Number(k as f64)],
                        ) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => JsValue::Undefined,
                        };

                        // Per spec: If key is -0, set key to +0
                        let key_val = if let JsValue::Number(n) = &key_val {
                            if *n == 0.0 {
                                JsValue::Number(0.0)
                            } else {
                                key_val
                            }
                        } else {
                            key_val
                        };

                        // Add value to the group for this key (using Map's SameValueZero semantics)
                        if let Some(map_obj) = interp.get_object_cell(result_id) {
                            let mut borrowed = map_obj.borrow_mut();
                            let entries = borrowed.map_data_mut().unwrap();

                            // Find existing entry with SameValueZero key equality
                            let existing_idx = entries.iter().position(|entry| {
                                if let Some((k, _)) = entry {
                                    same_value_zero(k, &key_val)
                                } else {
                                    false
                                }
                            });

                            if let Some(idx) = existing_idx {
                                // Append to existing array
                                if let Some((_, arr_val)) = entries[idx].as_ref()
                                    && let JsValue::Object(arr_obj) = arr_val
                                {
                                    let arr_id = arr_obj.id;
                                    drop(borrowed);
                                    if let Some(arr) = interp.get_object(arr_id) {
                                        let len_val = interp.get_property_on_id(arr_id, "length");
                                        let len = interp.to_number_coerce(&len_val) as usize;
                                        arr.borrow_mut().insert_builtin(len.to_string(), value);
                                        arr.borrow_mut().insert_builtin(
                                            "length".to_string(),
                                            JsValue::Number((len + 1) as f64),
                                        );
                                    }
                                }
                            } else {
                                // Create new array and add entry
                                drop(borrowed);
                                let new_arr = interp.create_array(vec![value]);
                                if let Some(map_obj) = interp.get_object_cell(result_id) {
                                    let mut borrowed = map_obj.borrow_mut();
                                    let entries = borrowed.map_data_mut().unwrap();
                                    entries.push(Some((key_val, new_arr)));
                                }
                            }
                        }
                        k += 1;
                    }

                    Completion::Normal(result_val)
                },
            ));
            ctor_obj
                .borrow_mut()
                .insert_builtin("groupBy".to_string(), group_by_fn);
        }

        self.realm()
            .global_env
            .borrow_mut()
            .declare("Map", BindingKind::Var);
        let env = self.realm().global_env.clone();
        let _ = self.env_set(&env, "Map", map_ctor);

        self.realm_mut().map_prototype = Some(proto_id);
    }

    pub(crate) fn setup_set_prototype(&mut self) {
        let proto_id = self.create_object_id();
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .class_name = "Set".to_string();

        // Set iterator prototype
        let set_iter_proto_id = self.create_object_id();
        self.get_object_cell_expect(set_iter_proto_id)
            .borrow_mut()
            .prototype_id = self.realm().iterator_prototype;
        self.get_object_cell_expect(set_iter_proto_id)
            .borrow_mut()
            .class_name = "Set Iterator".to_string();

        self.define_to_string_tag(set_iter_proto_id, "Set Iterator");

        self.define_method(set_iter_proto_id, "next", 0, |interp, this, _args| {
            if let JsValue::Object(o) = this
                && let Some(obj) = interp.get_object(o.id)
            {
                let state = obj.borrow().iterator_state().cloned();
                if let Some(IteratorState::SetIterator {
                    set_id,
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
                    if let Some(set_obj) = interp.get_object(set_id) {
                        let set_data = set_obj.borrow().set_data().cloned();
                        if let Some(entries) = set_data {
                            let mut i = index;
                            while i < entries.len() {
                                if let Some(ref val) = entries[i] {
                                    let result = match kind {
                                        IteratorKind::Value | IteratorKind::Key => val.clone(),
                                        IteratorKind::KeyValue => {
                                            interp.create_array(vec![val.clone(), val.clone()])
                                        }
                                    };
                                    obj.borrow_mut().kind =
                                        crate::interpreter::types::ObjectKind::Iterator(
                                            IteratorState::SetIterator {
                                                set_id,
                                                index: i + 1,
                                                kind,
                                                done: false,
                                            },
                                        );
                                    return Completion::Normal(
                                        interp.create_iter_result_object(result, false),
                                    );
                                }
                                i += 1;
                            }
                        }
                    }
                    obj.borrow_mut().kind = crate::interpreter::types::ObjectKind::Iterator(
                        IteratorState::SetIterator {
                            set_id,
                            index,
                            kind,
                            done: true,
                        },
                    );
                    return Completion::Normal(
                        interp.create_iter_result_object(JsValue::Undefined, true),
                    );
                }
            }
            let err =
                interp.create_type_error("Set Iterator.prototype.next requires a Set Iterator");
            Completion::Throw(err)
        });

        if let Some(key) = self.get_symbol_iterator_key() {
            let iter_self_fn = self.create_function(JsFunction::native(
                "[Symbol.iterator]".to_string(),
                0,
                |_interp, this, _args| Completion::Normal(this.clone()),
            ));
            self.get_object_cell_expect(set_iter_proto_id)
                .borrow_mut()
                .insert_property(
                    key,
                    PropertyDescriptor::data(iter_self_fn, true, false, true),
                );
        }

        self.realm_mut().set_iterator_prototype = Some(set_iter_proto_id);

        fn create_set_iterator(
            interp: &mut Interpreter,
            set_id: u64,
            kind: IteratorKind,
        ) -> JsValue {
            let mut obj_data = JsObjectData::new();
            obj_data.prototype_id = interp
                .realm()
                .set_iterator_prototype
                .or(interp.realm().iterator_prototype)
                .or(interp.realm().object_prototype);
            obj_data.class_name = "Set Iterator".to_string();
            obj_data.kind =
                crate::interpreter::types::ObjectKind::Iterator(IteratorState::SetIterator {
                    set_id,
                    index: 0,
                    kind,
                    done: false,
                });
            let id = interp.alloc_object(obj_data);
            JsValue::Object(crate::types::JsObject { id })
        }

        // Set.prototype.values
        let values_fn = self.define_method(proto_id, "values", 0, |interp, this, _args| {
            let (id, _obj) = match this_set(interp, this, "values") {
                Ok(t) => t,
                Err(c) => return c,
            };
            Completion::Normal(create_set_iterator(interp, id, IteratorKind::Value))
        });

        // Set.prototype.keys = Set.prototype.values
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_builtin("keys".to_string(), values_fn.clone());

        // Set.prototype[@@iterator] = values
        if let Some(key) = self.get_symbol_iterator_key() {
            self.get_object_cell_expect(proto_id)
                .borrow_mut()
                .insert_property(key, PropertyDescriptor::data(values_fn, true, false, true));
        }

        // Set.prototype.entries
        self.define_method(proto_id, "entries", 0, |interp, this, _args| {
            let (id, _obj) = match this_set(interp, this, "entries") {
                Ok(t) => t,
                Err(c) => return c,
            };
            Completion::Normal(create_set_iterator(interp, id, IteratorKind::KeyValue))
        });

        // Set.prototype.add
        self.define_method(proto_id, "add", 1, |interp, this, args| {
            let (_id, obj) = match this_set(interp, this, "add") {
                Ok(t) => t,
                Err(c) => return c,
            };
            let mut value = args.first().cloned().unwrap_or(JsValue::Undefined);
            if let JsValue::Number(n) = &value
                && *n == 0.0
                && n.is_sign_negative()
            {
                value = JsValue::Number(0.0);
            }
            let mut borrowed = obj.borrow_mut();
            let entries = borrowed.set_data_mut().unwrap();
            for entry in entries.iter().flatten() {
                if same_value_zero(entry, &value) {
                    return Completion::Normal(this.clone());
                }
            }
            entries.push(Some(value));
            Completion::Normal(this.clone())
        });

        // Set.prototype.has
        self.define_method(proto_id, "has", 1, |interp, this, args| {
            let (_id, obj) = match this_set(interp, this, "has") {
                Ok(t) => t,
                Err(c) => return c,
            };
            let entries = obj.borrow().set_data().cloned().unwrap();
            let value = args.first().cloned().unwrap_or(JsValue::Undefined);
            for entry in entries.iter().flatten() {
                if same_value_zero(entry, &value) {
                    return Completion::Normal(JsValue::Boolean(true));
                }
            }
            Completion::Normal(JsValue::Boolean(false))
        });

        // Set.prototype.delete
        self.define_method(proto_id, "delete", 1, |interp, this, args| {
            let (_id, obj) = match this_set(interp, this, "delete") {
                Ok(t) => t,
                Err(c) => return c,
            };
            let value = args.first().cloned().unwrap_or(JsValue::Undefined);
            let mut borrowed = obj.borrow_mut();
            let entries = borrowed.set_data_mut().unwrap();
            for entry in entries.iter_mut() {
                let matches = entry.as_ref().is_some_and(|e| same_value_zero(e, &value));
                if matches {
                    *entry = None;
                    return Completion::Normal(JsValue::Boolean(true));
                }
            }
            Completion::Normal(JsValue::Boolean(false))
        });

        // Set.prototype.clear
        self.define_method(proto_id, "clear", 0, |interp, this, _args| {
            let (_id, obj) = match this_set(interp, this, "clear") {
                Ok(t) => t,
                Err(c) => return c,
            };
            obj.borrow_mut().kind = crate::interpreter::types::ObjectKind::Set(Vec::new());
            Completion::Normal(JsValue::Undefined)
        });

        // Set.prototype.forEach
        self.define_method(proto_id, "forEach", 1,
            |interp, this, args| {
                let (_id, obj) = match this_set(interp, this, "forEach") {
                    Ok(t) => t,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(&callback, JsValue::Object(co) if interp.get_object(co.id).is_some_and(|o| o.borrow().callable.is_some())) {
                    let err = interp.create_type_error("Set.prototype.forEach callback is not a function");
                    return Completion::Throw(err);
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let mut i = 0;
                loop {
                    let entry = {
                        let borrowed = obj.borrow();
                        let entries = borrowed.set_data().unwrap();
                        if i >= entries.len() { break; }
                        entries[i].clone()
                    };
                    if let Some(v) = entry {
                        let result = interp.call_function(&callback, &this_arg, &[v.clone(), v, this.clone()]);
                        if result.is_abrupt() { return result; }
                    }
                    i += 1;
                }
                Completion::Normal(JsValue::Undefined)
            },
        );

        // Set.prototype.size (getter)
        self.define_getter(proto_id, "size", |interp, this, _args| {
            let (_id, obj) = match this_set(interp, this, "size") {
                Ok(t) => t,
                Err(c) => return c,
            };
            let count = obj
                .borrow()
                .set_data()
                .unwrap()
                .iter()
                .filter(|e| e.is_some())
                .count();
            Completion::Normal(JsValue::Number(count as f64))
        });

        // ES2025 Set methods

        // Spec-compliant GetSetRecord: uses get_object_property for getters/proxies
        fn spec_get_set_record(
            interp: &mut Interpreter,
            obj: &JsValue,
        ) -> Result<SetRecord, JsValue> {
            let o_id = match obj {
                JsValue::Object(o) => o.id,
                _ => return Err(interp.create_type_error("GetSetRecord requires an object")),
            };

            // Get size via property access (invokes getters)
            let raw_size = match interp.get_object_property(o_id, "size", obj) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::Undefined,
            };
            let size = interp.to_number_coerce(&raw_size);
            if size.is_nan() {
                return Err(interp.create_type_error("Set-like size is not a number"));
            }
            if size < 0.0 {
                return Err(interp.create_range_error("Set-like size is negative"));
            }
            let size = size.trunc();

            // Get has via property access (invokes getters)
            let has = match interp.get_object_property(o_id, "has", obj) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::Undefined,
            };
            if !interp.is_callable(&has) {
                return Err(
                    interp.create_type_error("Set-like object must have a callable has method")
                );
            }

            // Get keys via property access (invokes getters)
            let keys = match interp.get_object_property(o_id, "keys", obj) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::Undefined,
            };
            if !interp.is_callable(&keys) {
                return Err(
                    interp.create_type_error("Set-like object must have a callable keys method")
                );
            }

            Ok(SetRecord { has, keys, size })
        }

        // Get iterator record: call keys(), read next once
        fn get_keys_iterator(
            interp: &mut Interpreter,
            keys_fn: &JsValue,
            other: &JsValue,
        ) -> Result<(JsValue, JsValue), Completion> {
            let keys_iter = match interp.call_function(keys_fn, other, &[]) {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            if !matches!(&keys_iter, JsValue::Object(_)) {
                return Err(Completion::Throw(
                    interp.create_type_error("keys() must return an object"),
                ));
            }
            let iter_id = match &keys_iter {
                JsValue::Object(io) => io.id,
                _ => unreachable!(),
            };
            // Read next ONCE (per spec GetIterator)
            let next_fn = match interp.get_object_property(iter_id, "next", &keys_iter) {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            Ok((keys_iter, next_fn))
        }

        // Spec-compliant IteratorStepValue: uses get_object_property for done/value
        fn iter_step_value(
            interp: &mut Interpreter,
            keys_iter: &JsValue,
            next_fn: &JsValue,
        ) -> Result<Option<JsValue>, Completion> {
            let next_result = match interp.call_function(next_fn, keys_iter, &[]) {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let result_id = match &next_result {
                JsValue::Object(ro) => ro.id,
                _ => {
                    return Err(Completion::Throw(
                        interp.create_type_error("Iterator result is not an object"),
                    ));
                }
            };
            let done = match interp.get_object_property(result_id, "done", &next_result) {
                Completion::Normal(v) => interp.to_boolean_val(&v),
                other => return Err(other),
            };
            if done {
                return Ok(None);
            }
            let value = match interp.get_object_property(result_id, "value", &next_result) {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            Ok(Some(value))
        }

        fn canonicalize_key(val: JsValue) -> JsValue {
            if let JsValue::Number(n) = &val
                && *n == 0.0
                && n.is_sign_negative()
            {
                return JsValue::Number(0.0);
            }
            val
        }

        fn set_data_has(entries: &[Option<JsValue>], val: &JsValue) -> bool {
            entries.iter().flatten().any(|e| same_value_zero(e, val))
        }

        fn make_result_set(interp: &mut Interpreter, entries: Vec<Option<JsValue>>) -> Completion {
            let new_obj_id = interp.create_object_id();
            interp
                .get_object_cell_expect(new_obj_id)
                .borrow_mut()
                .prototype_id = interp.realm().set_prototype;
            interp
                .get_object_cell_expect(new_obj_id)
                .borrow_mut()
                .class_name = "Set".to_string();
            interp.get_object_cell_expect(new_obj_id).borrow_mut().kind =
                crate::interpreter::types::ObjectKind::Set(entries);
            let id = new_obj_id;
            Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
        }

        // Set.prototype.union
        self.define_method(proto_id, "union", 1, |interp, this, args| {
            let (_id, obj) = match this_set(interp, this, "union") {
                Ok(t) => t,
                Err(c) => return c,
            };
            let other = args.first().cloned().unwrap_or(JsValue::Undefined);
            let other_rec = match spec_get_set_record(interp, &other) {
                Ok(r) => r,
                Err(e) => return Completion::Throw(e),
            };
            // Step 5: GetIteratorFromMethod (may trigger .next getter side effects)
            let (keys_iter, next_fn) = match get_keys_iterator(interp, &other_rec.keys, &other) {
                Ok(r) => r,
                Err(c) => return c,
            };
            // Step 7: Copy O.[[SetData]] AFTER GetIteratorFromMethod
            let entries = obj.borrow().set_data().cloned().unwrap();
            let mut new_entries: Vec<Option<JsValue>> = Vec::new();
            for entry in entries.iter().flatten() {
                new_entries.push(Some(entry.clone()));
            }
            loop {
                let value = match iter_step_value(interp, &keys_iter, &next_fn) {
                    Ok(Some(v)) => v,
                    Ok(None) => break,
                    Err(c) => return c,
                };
                let val = canonicalize_key(value);
                if !set_data_has(&new_entries, &val) {
                    new_entries.push(Some(val));
                }
            }
            make_result_set(interp, new_entries)
        });

        // Set.prototype.intersection
        self.define_method(proto_id, "intersection", 1, |interp, this, args| {
            let (_id, obj) = match this_set(interp, this, "intersection") {
                Ok(t) => t,
                Err(c) => return c,
            };
            let other = args.first().cloned().unwrap_or(JsValue::Undefined);
            let other_rec = match spec_get_set_record(interp, &other) {
                Ok(r) => r,
                Err(e) => return Completion::Throw(e),
            };
            let mut new_entries: Vec<Option<JsValue>> = Vec::new();
            // Re-read entries after GetSetRecord (side-effects may mutate this)
            let entries = obj.borrow().set_data().cloned().unwrap();
            let this_size = entries.iter().filter(|e| e.is_some()).count();

            if this_size as f64 <= other_rec.size {
                let mut index = 0;
                loop {
                    let entry = {
                        let borrowed = obj.borrow();
                        let data = borrowed.set_data().unwrap();
                        if index >= data.len() {
                            break;
                        }
                        data[index].clone()
                    };
                    index += 1;
                    if let Some(entry) = entry {
                        let has_result = match interp.call_function(
                            &other_rec.has,
                            &other,
                            std::slice::from_ref(&entry),
                        ) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        if interp.to_boolean_val(&has_result) {
                            let val = canonicalize_key(entry);
                            if !set_data_has(&new_entries, &val) {
                                new_entries.push(Some(val));
                            }
                        }
                    }
                }
            } else {
                let (keys_iter, next_fn) = match get_keys_iterator(interp, &other_rec.keys, &other)
                {
                    Ok(r) => r,
                    Err(c) => return c,
                };
                loop {
                    let value = match iter_step_value(interp, &keys_iter, &next_fn) {
                        Ok(Some(v)) => v,
                        Ok(None) => break,
                        Err(c) => return c,
                    };
                    let current = obj.borrow().set_data().cloned().unwrap_or_default();
                    if set_data_has(&current, &value) {
                        let val = canonicalize_key(value);
                        if !set_data_has(&new_entries, &val) {
                            new_entries.push(Some(val));
                        }
                    }
                }
            }
            make_result_set(interp, new_entries)
        });

        // Set.prototype.difference
        self.define_method(proto_id, "difference", 1, |interp, this, args| {
            let (_id, obj) = match this_set(interp, this, "difference") {
                Ok(t) => t,
                Err(c) => return c,
            };
            let other = args.first().cloned().unwrap_or(JsValue::Undefined);
            let other_rec = match spec_get_set_record(interp, &other) {
                Ok(r) => r,
                Err(e) => return Completion::Throw(e),
            };
            // Re-read entries after GetSetRecord
            let entries = obj.borrow().set_data().cloned().unwrap();
            let this_size = entries.iter().filter(|e| e.is_some()).count();
            let mut new_entries: Vec<Option<JsValue>> = Vec::new();

            if this_size as f64 <= other_rec.size {
                for entry in entries.iter().flatten() {
                    let has_result = match interp.call_function(
                        &other_rec.has,
                        &other,
                        std::slice::from_ref(entry),
                    ) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if !interp.to_boolean_val(&has_result) {
                        new_entries.push(Some(entry.clone()));
                    }
                }
            } else {
                for entry in entries.iter().flatten() {
                    new_entries.push(Some(entry.clone()));
                }
                let (keys_iter, next_fn) = match get_keys_iterator(interp, &other_rec.keys, &other)
                {
                    Ok(r) => r,
                    Err(c) => return c,
                };
                loop {
                    let value = match iter_step_value(interp, &keys_iter, &next_fn) {
                        Ok(Some(v)) => v,
                        Ok(None) => break,
                        Err(c) => return c,
                    };
                    for entry in new_entries.iter_mut() {
                        if entry.as_ref().is_some_and(|e| same_value_zero(e, &value)) {
                            *entry = None;
                            break;
                        }
                    }
                }
            }
            make_result_set(interp, new_entries)
        });

        // Set.prototype.symmetricDifference
        self.define_method(proto_id, "symmetricDifference", 1, |interp, this, args| {
            let (_id, obj) = match this_set(interp, this, "symmetricDifference") {
                Ok(t) => t,
                Err(c) => return c,
            };
            let other = args.first().cloned().unwrap_or(JsValue::Undefined);
            let other_rec = match spec_get_set_record(interp, &other) {
                Ok(r) => r,
                Err(e) => return Completion::Throw(e),
            };
            // Step 5: GetIteratorFromMethod (may trigger .next getter side effects)
            let (keys_iter, next_fn) = match get_keys_iterator(interp, &other_rec.keys, &other) {
                Ok(r) => r,
                Err(c) => return c,
            };
            // Step 6: Copy O.[[SetData]] AFTER GetIteratorFromMethod
            let entries = obj.borrow().set_data().cloned().unwrap();
            let mut new_entries: Vec<Option<JsValue>> = Vec::new();
            for entry in entries.iter().flatten() {
                new_entries.push(Some(entry.clone()));
            }
            loop {
                let value = match iter_step_value(interp, &keys_iter, &next_fn) {
                    Ok(Some(v)) => v,
                    Ok(None) => break,
                    Err(c) => return c,
                };
                let val = canonicalize_key(value);
                // Check against live O.[[SetData]]
                let current = obj.borrow().set_data().cloned().unwrap_or_default();
                let in_this = set_data_has(&current, &val);
                if in_this {
                    for entry in new_entries.iter_mut() {
                        if entry.as_ref().is_some_and(|e| same_value_zero(e, &val)) {
                            *entry = None;
                            break;
                        }
                    }
                } else if !set_data_has(&new_entries, &val) {
                    new_entries.push(Some(val));
                }
            }
            make_result_set(interp, new_entries)
        });

        // Set.prototype.isSubsetOf
        self.define_method(proto_id, "isSubsetOf", 1, |interp, this, args| {
            let (_id, obj) = match this_set(interp, this, "isSubsetOf") {
                Ok(t) => t,
                Err(c) => return c,
            };
            let other = args.first().cloned().unwrap_or(JsValue::Undefined);
            let other_rec = match spec_get_set_record(interp, &other) {
                Ok(r) => r,
                Err(e) => return Completion::Throw(e),
            };
            let entries = obj.borrow().set_data().cloned().unwrap();
            let this_size = entries.iter().filter(|e| e.is_some()).count();
            if this_size as f64 > other_rec.size {
                return Completion::Normal(JsValue::Boolean(false));
            }
            // Iterate live set data (re-read each iteration for mutation support)
            let mut i = 0;
            loop {
                let entry = {
                    let borrowed = obj.borrow();
                    let data = borrowed.set_data().unwrap();
                    if i >= data.len() {
                        break;
                    }
                    data[i].clone()
                };
                i += 1;
                if let Some(e) = entry {
                    let has_result = match interp.call_function(&other_rec.has, &other, &[e]) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if !interp.to_boolean_val(&has_result) {
                        return Completion::Normal(JsValue::Boolean(false));
                    }
                }
            }
            Completion::Normal(JsValue::Boolean(true))
        });

        // Set.prototype.isSupersetOf
        self.define_method(proto_id, "isSupersetOf", 1, |interp, this, args| {
            let (_id, obj) = match this_set(interp, this, "isSupersetOf") {
                Ok(t) => t,
                Err(c) => return c,
            };
            let other = args.first().cloned().unwrap_or(JsValue::Undefined);
            let other_rec = match spec_get_set_record(interp, &other) {
                Ok(r) => r,
                Err(e) => return Completion::Throw(e),
            };
            let entries = obj.borrow().set_data().cloned().unwrap();
            let this_size = entries.iter().filter(|e| e.is_some()).count();
            if (this_size as f64) < other_rec.size {
                return Completion::Normal(JsValue::Boolean(false));
            }
            let (keys_iter, next_fn) = match get_keys_iterator(interp, &other_rec.keys, &other) {
                Ok(r) => r,
                Err(c) => return c,
            };
            loop {
                let value = match iter_step_value(interp, &keys_iter, &next_fn) {
                    Ok(Some(v)) => v,
                    Ok(None) => break,
                    Err(c) => return c,
                };
                let current = obj.borrow().set_data().cloned().unwrap_or_default();
                if !set_data_has(&current, &value) {
                    interp.iterator_close(&keys_iter, JsValue::Undefined);
                    return Completion::Normal(JsValue::Boolean(false));
                }
            }
            Completion::Normal(JsValue::Boolean(true))
        });

        // Set.prototype.isDisjointFrom
        self.define_method(proto_id, "isDisjointFrom", 1, |interp, this, args| {
            let (_id, obj) = match this_set(interp, this, "isDisjointFrom") {
                Ok(t) => t,
                Err(c) => return c,
            };
            let other = args.first().cloned().unwrap_or(JsValue::Undefined);
            let other_rec = match spec_get_set_record(interp, &other) {
                Ok(r) => r,
                Err(e) => return Completion::Throw(e),
            };
            let entries = obj.borrow().set_data().cloned().unwrap();
            let this_size = entries.iter().filter(|e| e.is_some()).count();
            if this_size as f64 <= other_rec.size {
                // Iterate live set data (re-read each iteration for mutation support)
                let mut i = 0;
                loop {
                    let entry = {
                        let borrowed = obj.borrow();
                        let data = borrowed.set_data().unwrap();
                        if i >= data.len() {
                            break;
                        }
                        data[i].clone()
                    };
                    i += 1;
                    if let Some(e) = entry {
                        let has_result = match interp.call_function(&other_rec.has, &other, &[e]) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        if interp.to_boolean_val(&has_result) {
                            return Completion::Normal(JsValue::Boolean(false));
                        }
                    }
                }
            } else {
                let (keys_iter, next_fn) = match get_keys_iterator(interp, &other_rec.keys, &other)
                {
                    Ok(r) => r,
                    Err(c) => return c,
                };
                loop {
                    let value = match iter_step_value(interp, &keys_iter, &next_fn) {
                        Ok(Some(v)) => v,
                        Ok(None) => break,
                        Err(c) => return c,
                    };
                    let current = obj.borrow().set_data().cloned().unwrap_or_default();
                    if set_data_has(&current, &value) {
                        interp.iterator_close(&keys_iter, JsValue::Undefined);
                        return Completion::Normal(JsValue::Boolean(false));
                    }
                }
            }
            Completion::Normal(JsValue::Boolean(true))
        });

        // @@toStringTag
        self.define_to_string_tag(proto_id, "Set");

        let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });

        // Set constructor
        let set_proto_clone_id = proto_id;
        let set_ctor = self.create_function(JsFunction::constructor(
            "Set".to_string(),
            0,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    let err = interp.create_type_error("Constructor Set requires 'new'");
                    return Completion::Throw(err);
                }

                // OrdinaryCreateFromConstructor — realm-aware prototype
                let proto = match interp.get_prototype_from_new_target_realm(|realm| {
                    realm.set_prototype
                }) {
                    Ok(p) => p.unwrap_or(set_proto_clone_id),
                    Err(e) => return Completion::Throw(e),
                };
                let obj_id = interp.create_object_id();
                interp.get_object_cell_expect(obj_id).borrow_mut().prototype_id = Some(proto);
                interp.get_object_cell_expect(obj_id).borrow_mut().class_name = "Set".to_string();
                interp.get_object_cell_expect(obj_id).borrow_mut().kind = crate::interpreter::types::ObjectKind::Set(Vec::new());
                let this_val = JsValue::Object(crate::types::JsObject { id: obj_id });

                let iterable = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !iterable.is_undefined() && !iterable.is_null() {
                    // §24.2.1.1 step 7a: Let adder be ? Get(set, "add").
                    let adder = match interp.get_object_property(obj_id, "add", &this_val) {
                        Completion::Normal(v) => v,
                        c => return c,
                    };
                    if !matches!(&adder, JsValue::Object(ao) if interp.get_object_cell(ao.id).is_some_and(|o| o.borrow().callable.is_some())) {
                        let err = interp.create_type_error("Set.prototype.add is not a function");
                        return Completion::Throw(err);
                    }

                    let iter_key = interp.get_symbol_iterator_key();
                    let iterator_fn = if let Some(ref key) = iter_key {
                        if let JsValue::Object(io) = &iterable {
                            let v = interp.get_property_on_id(io.id, key);
                            if v.is_undefined() { JsValue::Undefined } else { v }
                        } else { JsValue::Undefined }
                    } else { JsValue::Undefined };

                    if iterator_fn.is_undefined() {
                        let err = interp.create_type_error("object is not iterable");
                        return Completion::Throw(err);
                    }

                    let iterator = match interp.call_function(&iterator_fn, &iterable, &[]) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };

                    loop {
                        let next_fn = if let JsValue::Object(io) = &iterator {
                            interp.get_property_on_id(io.id, "next")
                        } else { JsValue::Undefined };

                        let next_result = match interp.call_function(&next_fn, &iterator, &[]) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };

                        // Use getter-aware property access for done/value
                        let done = if let JsValue::Object(ro) = &next_result {
                            match interp.get_object_property(ro.id, "done", &next_result) {
                                Completion::Normal(v) => interp.to_boolean_val(&v),
                                Completion::Throw(e) => return Completion::Throw(e),
                                other => return other,
                            }
                        } else { false };
                        if done { break; }
                        // Per IteratorStepValue, if accessing .value throws, the error
                        // propagates directly without closing the iterator.
                        let value = if let JsValue::Object(ro) = &next_result {
                            match interp.get_object_property(ro.id, "value", &next_result) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                other => return other,
                            }
                        } else { JsValue::Undefined };

                        match interp.call_function(&adder, &this_val, &[value]) {
                            Completion::Normal(_) => {}
                            Completion::Throw(e) => {
                                interp.iterator_close(&iterator, e.clone());
                                return Completion::Throw(e);
                            }
                            other => return other,
                        }
                    }
                }

                Completion::Normal(this_val)
            },
        ));

        if let JsValue::Object(ctor_obj) = &set_ctor
            && let Some(obj) = self.get_object_cell(ctor_obj.id)
        {
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val.clone(), false, false, false),
            );
        }
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_property(
                "constructor".to_string(),
                PropertyDescriptor::data(set_ctor.clone(), true, false, true),
            );

        // Set[Symbol.species] getter
        if let JsValue::Object(ref ctor_ref) = set_ctor
            && let Some(ctor_obj) = self.get_object(ctor_ref.id)
        {
            let species_getter = self.create_function(JsFunction::native(
                "get [Symbol.species]".to_string(),
                0,
                |_interp, this_val, _args| Completion::Normal(this_val.clone()),
            ));
            ctor_obj.borrow_mut().insert_property(
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
        }

        self.realm()
            .global_env
            .borrow_mut()
            .declare("Set", BindingKind::Var);
        let env = self.realm().global_env.clone();
        let _ = self.env_set(&env, "Set", set_ctor);

        self.realm_mut().set_prototype = Some(proto_id);
    }

    pub(crate) fn create_type_error(&mut self, msg: &str) -> JsValue {
        self.create_error("TypeError", msg)
    }

    pub(crate) fn setup_weakmap_prototype(&mut self) {
        let proto_id = self.create_object_id();
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .class_name = "WeakMap".to_string();

        // WeakMap.prototype.get
        self.define_method(proto_id, "get", 1, |interp, this, args| {
            if let JsValue::Object(o) = this
                && let Some(obj) = interp.get_object_cell(o.id)
            {
                let is_weakmap = obj.borrow().class_name == "WeakMap";
                let map_data = obj.borrow().map_data().cloned();
                if is_weakmap && let Some(entries) = map_data {
                    let key = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if !interp.can_be_held_weakly(&key) {
                        return Completion::Normal(JsValue::Undefined);
                    }
                    for entry in entries.iter().flatten() {
                        if strict_equality(&entry.0, &key) {
                            return Completion::Normal(entry.1.clone());
                        }
                    }
                    return Completion::Normal(JsValue::Undefined);
                }
            }
            let err = interp.create_type_error("WeakMap.prototype.get requires a WeakMap");
            Completion::Throw(err)
        });

        // WeakMap.prototype.set
        self.define_method(proto_id, "set", 2, |interp, this, args| {
            if let JsValue::Object(o) = this
                && let Some(obj) = interp.get_object_cell(o.id)
            {
                let has_map = obj.borrow().map_data().is_some();
                if has_map && obj.borrow().class_name == "WeakMap" {
                    let key = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if !interp.can_be_held_weakly(&key) {
                        let err = interp.create_type_error("Invalid value used as weak map key");
                        return Completion::Throw(err);
                    }
                    let value = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    let mut borrowed = obj.borrow_mut();
                    let entries = borrowed.map_data_mut().unwrap();
                    for entry in entries.iter_mut().flatten() {
                        if strict_equality(&entry.0, &key) {
                            entry.1 = value;
                            return Completion::Normal(this.clone());
                        }
                    }
                    entries.push(Some((key, value)));
                    return Completion::Normal(this.clone());
                }
            }
            let err = interp.create_type_error("WeakMap.prototype.set requires a WeakMap");
            Completion::Throw(err)
        });

        // WeakMap.prototype.has
        self.define_method(proto_id, "has", 1, |interp, this, args| {
            if let JsValue::Object(o) = this
                && let Some(obj) = interp.get_object_cell(o.id)
            {
                let is_weakmap = obj.borrow().class_name == "WeakMap";
                let map_data = obj.borrow().map_data().cloned();
                if is_weakmap && let Some(entries) = map_data {
                    let key = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if !interp.can_be_held_weakly(&key) {
                        return Completion::Normal(JsValue::Boolean(false));
                    }
                    for entry in entries.iter().flatten() {
                        if strict_equality(&entry.0, &key) {
                            return Completion::Normal(JsValue::Boolean(true));
                        }
                    }
                    return Completion::Normal(JsValue::Boolean(false));
                }
            }
            let err = interp.create_type_error("WeakMap.prototype.has requires a WeakMap");
            Completion::Throw(err)
        });

        // WeakMap.prototype.delete
        self.define_method(proto_id, "delete", 1, |interp, this, args| {
            if let JsValue::Object(o) = this
                && let Some(obj) = interp.get_object_cell(o.id)
            {
                let is_weakmap = obj.borrow().class_name == "WeakMap";
                let has_map = obj.borrow().map_data().is_some();
                if is_weakmap && has_map {
                    let key = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if !interp.can_be_held_weakly(&key) {
                        return Completion::Normal(JsValue::Boolean(false));
                    }
                    let mut borrowed = obj.borrow_mut();
                    let entries = borrowed.map_data_mut().unwrap();
                    for entry in entries.iter_mut() {
                        let matches = entry.as_ref().is_some_and(|e| strict_equality(&e.0, &key));
                        if matches {
                            *entry = None;
                            return Completion::Normal(JsValue::Boolean(true));
                        }
                    }
                    return Completion::Normal(JsValue::Boolean(false));
                }
            }
            let err = interp.create_type_error("WeakMap.prototype.delete requires a WeakMap");
            Completion::Throw(err)
        });

        // WeakMap.prototype.getOrInsert
        self.define_method(proto_id, "getOrInsert", 2, |interp, this, args| {
            if let JsValue::Object(o) = &this
                && let Some(obj) = interp.get_object_cell(o.id)
            {
                let is_weakmap =
                    obj.borrow().map_data().is_some() && obj.borrow().class_name == "WeakMap";
                if is_weakmap {
                    let key = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let value = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    if !interp.can_be_held_weakly(&key) {
                        let err = interp.create_type_error("Invalid value used as weak map key");
                        return Completion::Throw(err);
                    }
                    {
                        let borrowed = obj.borrow();
                        let entries = borrowed.map_data().unwrap();
                        for entry in entries.iter().flatten() {
                            if strict_equality(&entry.0, &key) {
                                return Completion::Normal(entry.1.clone());
                            }
                        }
                    }
                    let mut borrowed = obj.borrow_mut();
                    let entries = borrowed.map_data_mut().unwrap();
                    entries.push(Some((key, value.clone())));
                    return Completion::Normal(value);
                }
            }
            let err = interp.create_type_error("WeakMap.prototype.getOrInsert requires a WeakMap");
            Completion::Throw(err)
        });

        // WeakMap.prototype.getOrInsertComputed
        self.define_method(proto_id, "getOrInsertComputed", 2,
            |interp, this, args| {
                if let JsValue::Object(o) = &this
                    && let Some(obj) = interp.get_object_cell(o.id)
                {
                    let is_weakmap = obj.borrow().map_data().is_some()
                        && obj.borrow().class_name == "WeakMap";
                    if is_weakmap {
                        let key = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let callbackfn = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                        if !interp.can_be_held_weakly(&key) {
                            let err = interp
                                .create_type_error("Invalid value used as weak map key");
                            return Completion::Throw(err);
                        }
                        let is_callable = matches!(&callbackfn, JsValue::Object(co)
                            if interp.get_object_cell(co.id).is_some_and(|ob| ob.borrow().callable.is_some()));
                        if !is_callable {
                            let err = interp
                                .create_type_error("callbackfn is not a function");
                            return Completion::Throw(err);
                        }
                        {
                            let borrowed = obj.borrow();
                            let entries = borrowed.map_data().unwrap();
                            for entry in entries.iter().flatten() {
                                if strict_equality(&entry.0, &key) {
                                    return Completion::Normal(entry.1.clone());
                                }
                            }
                        }
                        let value = match interp.call_function(
                            &callbackfn,
                            &JsValue::Undefined,
                            std::slice::from_ref(&key),
                        ) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let obj = interp.get_object_cell(o.id).unwrap();
                        let mut borrowed = obj.borrow_mut();
                        let entries = borrowed.map_data_mut().unwrap();
                        for entry in entries.iter_mut().flatten() {
                            if strict_equality(&entry.0, &key) {
                                entry.1 = value.clone();
                                return Completion::Normal(value);
                            }
                        }
                        entries.push(Some((key, value.clone())));
                        return Completion::Normal(value);
                    }
                }
                let err = interp.create_type_error(
                    "WeakMap.prototype.getOrInsertComputed requires a WeakMap",
                );
                Completion::Throw(err)
            },
        );

        // @@toStringTag
        self.define_to_string_tag(proto_id, "WeakMap");

        let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });

        // WeakMap constructor
        let weakmap_proto_clone_id = proto_id;
        let weakmap_ctor = self.create_function(JsFunction::constructor(
            "WeakMap".to_string(),
            0,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    let err = interp.create_type_error("Constructor WeakMap requires 'new'");
                    return Completion::Throw(err);
                }

                // OrdinaryCreateFromConstructor — realm-aware prototype
                let proto = match interp.get_prototype_from_new_target_realm(|realm| {
                    realm.weakmap_prototype
                }) {
                    Ok(p) => p.unwrap_or(weakmap_proto_clone_id),
                    Err(e) => return Completion::Throw(e),
                };
                let obj_id = interp.create_object_id();
                interp.get_object_cell_expect(obj_id).borrow_mut().prototype_id = Some(proto);
                interp.get_object_cell_expect(obj_id).borrow_mut().class_name = "WeakMap".to_string();
                interp.get_object_cell_expect(obj_id).borrow_mut().kind = crate::interpreter::types::ObjectKind::Map(Vec::new());
                let this_val = JsValue::Object(crate::types::JsObject { id: obj_id });

                let iterable = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !iterable.is_undefined() && !iterable.is_null() {
                    // §24.3.1.1 step 7a: Let adder be ? Get(map, "set").
                    let adder = match interp.get_object_property(obj_id, "set", &this_val) {
                        Completion::Normal(v) => v,
                        c => return c,
                    };
                    if !matches!(&adder, JsValue::Object(ao) if interp.get_object_cell(ao.id).is_some_and(|o| o.borrow().callable.is_some())) {
                        let err = interp.create_type_error("WeakMap.prototype.set is not a function");
                        return Completion::Throw(err);
                    }

                    let iter_key = interp.get_symbol_iterator_key();
                    let iterator_fn = if let Some(ref key) = iter_key {
                        if let JsValue::Object(io) = &iterable {
                            let v = interp.get_property_on_id(io.id, key);
                            if v.is_undefined() { JsValue::Undefined } else { v }
                        } else { JsValue::Undefined }
                    } else { JsValue::Undefined };

                    if iterator_fn.is_undefined() {
                        let err = interp.create_type_error("object is not iterable");
                        return Completion::Throw(err);
                    }

                    let iterator = match interp.call_function(&iterator_fn, &iterable, &[]) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };

                    loop {
                        let next_fn = if let JsValue::Object(io) = &iterator {
                            interp.get_property_on_id(io.id, "next")
                        } else { JsValue::Undefined };

                        let next_result = match interp.call_function(&next_fn, &iterator, &[]) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };

                        // IteratorStep: access .done via getter-aware Get (not raw get_property)
                        // Per spec, if accessing .done throws, the error propagates directly
                        // without closing the iterator.
                        let done = if let JsValue::Object(ro) = &next_result {
                            match interp.get_object_property(ro.id, "done", &next_result) {
                                Completion::Normal(d) => interp.to_boolean_val(&d),
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => false,
                            }
                        } else { false };

                        if done { break; }

                        // §24.3.1.1 step 9d: Get value via Get (invokes getters).
                        // If accessing .value throws, the error propagates without closing the iterator.
                        let value = if let JsValue::Object(ro) = &next_result {
                            match interp.get_object_property(ro.id, "value", &next_result) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                other => return other,
                            }
                        } else { JsValue::Undefined };

                        // §24.3.1.1 step 9e: If value is not Object, close iterator + throw
                        if !matches!(&value, JsValue::Object(_)) {
                            let err = interp.create_type_error("Iterator value is not an object");
                            let e2 = interp.iterator_close(&iterator, err);
                            return Completion::Throw(e2);
                        }

                        // Get key and value from the [key, value] pair
                        let (k, v) = if let JsValue::Object(vo) = &value {
                            let k = match interp.get_object_property(vo.id, "0", &value) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => {
                                    let e2 = interp.iterator_close(&iterator, e);
                                    return Completion::Throw(e2);
                                }
                                other => return other,
                            };
                            let v = match interp.get_object_property(vo.id, "1", &value) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => {
                                    let e2 = interp.iterator_close(&iterator, e);
                                    return Completion::Throw(e2);
                                }
                                other => return other,
                            };
                            (k, v)
                        } else {
                            unreachable!()
                        };

                        // §24.3.1.1 step 9f-g: Call adder, IteratorClose on failure
                        match interp.call_function(&adder, &this_val, &[k, v]) {
                            Completion::Normal(_) => {}
                            Completion::Throw(e) => {
                                let e2 = interp.iterator_close(&iterator, e);
                                return Completion::Throw(e2);
                            }
                            other => return other,
                        }
                    }
                }

                Completion::Normal(this_val)
            },
        ));

        if let JsValue::Object(ctor_obj) = &weakmap_ctor
            && let Some(obj) = self.get_object_cell(ctor_obj.id)
        {
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val.clone(), false, false, false),
            );
        }
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_property(
                "constructor".to_string(),
                PropertyDescriptor::data(weakmap_ctor.clone(), true, false, true),
            );

        self.realm()
            .global_env
            .borrow_mut()
            .declare("WeakMap", BindingKind::Var);
        let _ = self
            .realm()
            .global_env
            .borrow_mut()
            .set("WeakMap", weakmap_ctor);

        self.realm_mut().weakmap_prototype = Some(proto_id);
    }

    pub(crate) fn setup_weakset_prototype(&mut self) {
        let proto_id = self.create_object_id();
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .class_name = "WeakSet".to_string();

        // WeakSet.prototype.add
        self.define_method(proto_id, "add", 1, |interp, this, args| {
            if let JsValue::Object(o) = this
                && let Some(obj) = interp.get_object_cell(o.id)
            {
                let has_set =
                    obj.borrow().set_data().is_some() && obj.borrow().class_name == "WeakSet";
                if has_set {
                    let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if !interp.can_be_held_weakly(&value) {
                        let err = interp.create_type_error("Invalid value used in weak set");
                        return Completion::Throw(err);
                    }
                    let mut borrowed = obj.borrow_mut();
                    let entries = borrowed.set_data_mut().unwrap();
                    for entry in entries.iter().flatten() {
                        if strict_equality(entry, &value) {
                            return Completion::Normal(this.clone());
                        }
                    }
                    entries.push(Some(value));
                    return Completion::Normal(this.clone());
                }
            }
            let err = interp.create_type_error("WeakSet.prototype.add requires a WeakSet");
            Completion::Throw(err)
        });

        // WeakSet.prototype.has
        self.define_method(proto_id, "has", 1, |interp, this, args| {
            if let JsValue::Object(o) = this
                && let Some(obj) = interp.get_object_cell(o.id)
            {
                let is_weakset = obj.borrow().class_name == "WeakSet";
                let set_data = obj.borrow().set_data().cloned();
                if is_weakset && let Some(entries) = set_data {
                    let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if !interp.can_be_held_weakly(&value) {
                        return Completion::Normal(JsValue::Boolean(false));
                    }
                    for entry in entries.iter().flatten() {
                        if strict_equality(entry, &value) {
                            return Completion::Normal(JsValue::Boolean(true));
                        }
                    }
                    return Completion::Normal(JsValue::Boolean(false));
                }
            }
            let err = interp.create_type_error("WeakSet.prototype.has requires a WeakSet");
            Completion::Throw(err)
        });

        // WeakSet.prototype.delete
        self.define_method(proto_id, "delete", 1, |interp, this, args| {
            if let JsValue::Object(o) = this
                && let Some(obj) = interp.get_object_cell(o.id)
            {
                let is_weakset = obj.borrow().class_name == "WeakSet";
                let has_set = obj.borrow().set_data().is_some();
                if is_weakset && has_set {
                    let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if !interp.can_be_held_weakly(&value) {
                        return Completion::Normal(JsValue::Boolean(false));
                    }
                    let mut borrowed = obj.borrow_mut();
                    let entries = borrowed.set_data_mut().unwrap();
                    for entry in entries.iter_mut() {
                        let matches = entry.as_ref().is_some_and(|e| strict_equality(e, &value));
                        if matches {
                            *entry = None;
                            return Completion::Normal(JsValue::Boolean(true));
                        }
                    }
                    return Completion::Normal(JsValue::Boolean(false));
                }
            }
            let err = interp.create_type_error("WeakSet.prototype.delete requires a WeakSet");
            Completion::Throw(err)
        });

        // @@toStringTag
        self.define_to_string_tag(proto_id, "WeakSet");

        let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });

        // WeakSet constructor
        let weakset_proto_clone_id = proto_id;
        let weakset_ctor = self.create_function(JsFunction::constructor(
            "WeakSet".to_string(),
            0,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    let err = interp.create_type_error("Constructor WeakSet requires 'new'");
                    return Completion::Throw(err);
                }

                // OrdinaryCreateFromConstructor — realm-aware prototype
                let proto = match interp.get_prototype_from_new_target_realm(|realm| {
                    realm.weakset_prototype
                }) {
                    Ok(p) => p.unwrap_or(weakset_proto_clone_id),
                    Err(e) => return Completion::Throw(e),
                };
                let obj_id = interp.create_object_id();
                interp.get_object_cell_expect(obj_id).borrow_mut().prototype_id = Some(proto);
                interp.get_object_cell_expect(obj_id).borrow_mut().class_name = "WeakSet".to_string();
                interp.get_object_cell_expect(obj_id).borrow_mut().kind = crate::interpreter::types::ObjectKind::Set(Vec::new());
                let this_val = JsValue::Object(crate::types::JsObject { id: obj_id });

                let iterable = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !iterable.is_undefined() && !iterable.is_null() {
                    // §24.4.1.1 step 7a: Let adder be ? Get(set, "add").
                    let adder = match interp.get_object_property(obj_id, "add", &this_val) {
                        Completion::Normal(v) => v,
                        c => return c,
                    };
                    if !matches!(&adder, JsValue::Object(ao) if interp.get_object_cell(ao.id).is_some_and(|o| o.borrow().callable.is_some())) {
                        let err = interp.create_type_error("WeakSet.prototype.add is not a function");
                        return Completion::Throw(err);
                    }

                    let iter_key = interp.get_symbol_iterator_key();
                    let iterator_fn = if let Some(ref key) = iter_key {
                        if let JsValue::Object(io) = &iterable {
                            let v = interp.get_property_on_id(io.id, key);
                            if v.is_undefined() { JsValue::Undefined } else { v }
                        } else { JsValue::Undefined }
                    } else { JsValue::Undefined };

                    if iterator_fn.is_undefined() {
                        let err = interp.create_type_error("object is not iterable");
                        return Completion::Throw(err);
                    }

                    let iterator = match interp.call_function(&iterator_fn, &iterable, &[]) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };

                    loop {
                        let next_fn = if let JsValue::Object(io) = &iterator {
                            interp.get_property_on_id(io.id, "next")
                        } else { JsValue::Undefined };

                        let next_result = match interp.call_function(&next_fn, &iterator, &[]) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };

                        if !matches!(next_result, JsValue::Object(_)) {
                            return Completion::Throw(interp.create_type_error("Iterator result is not an object"));
                        }

                        // Use getter-aware property access for done/value.
                        // Per IteratorStepValue, errors from .done/.value propagate
                        // directly without closing the iterator.
                        let done = if let JsValue::Object(ro) = &next_result {
                            match interp.get_object_property(ro.id, "done", &next_result) {
                                Completion::Normal(d) => interp.to_boolean_val(&d),
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => false,
                            }
                        } else { false };
                        if done { break; }

                        let value = if let JsValue::Object(ro) = &next_result {
                            match interp.get_object_property(ro.id, "value", &next_result) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                other => return other,
                            }
                        } else {
                            JsValue::Undefined
                        };

                        // §24.4.1.1 step 9f-g: Call adder, IteratorClose on failure
                        match interp.call_function(&adder, &this_val, &[value]) {
                            Completion::Normal(_) => {}
                            Completion::Throw(e) => {
                                let e2 = interp.iterator_close(&iterator, e);
                                return Completion::Throw(e2);
                            }
                            other => return other,
                        }
                    }
                }

                Completion::Normal(this_val)
            },
        ));

        if let JsValue::Object(ctor_obj) = &weakset_ctor
            && let Some(obj) = self.get_object_cell(ctor_obj.id)
        {
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val.clone(), false, false, false),
            );
        }
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_property(
                "constructor".to_string(),
                PropertyDescriptor::data(weakset_ctor.clone(), true, false, true),
            );

        self.realm()
            .global_env
            .borrow_mut()
            .declare("WeakSet", BindingKind::Var);
        let _ = self
            .realm()
            .global_env
            .borrow_mut()
            .set("WeakSet", weakset_ctor);

        self.realm_mut().weakset_prototype = Some(proto_id);
    }

    pub(crate) fn setup_weakref(&mut self) {
        let proto_id = self.create_object_id();
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .class_name = "WeakRef".to_string();

        // WeakRef.prototype.deref
        self.define_method(proto_id, "deref", 0, |interp, this, _args| {
            // Require this to be an object with [[WeakRefTarget]] internal slot
            // (indicated by class_name == "WeakRef" AND primitive_value.is_some())
            if let JsValue::Object(o) = this
                && let Some(obj) = interp.get_object_cell(o.id)
            {
                let b = obj.borrow();
                if b.class_name == "WeakRef" && b.primitive_value.is_some() {
                    return Completion::Normal(
                        b.primitive_value.clone().unwrap_or(JsValue::Undefined),
                    );
                }
            }
            Completion::Throw(
                interp.create_type_error("WeakRef.prototype.deref requires a WeakRef"),
            )
        });

        // @@toStringTag
        self.define_to_string_tag(proto_id, "WeakRef");

        // WeakRef constructor
        let weakref_ctor = self.create_function(JsFunction::constructor(
            "WeakRef".to_string(),
            1,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    let err = interp.create_type_error("Constructor WeakRef requires 'new'");
                    return Completion::Throw(err);
                }
                let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !interp.can_be_held_weakly(&target) {
                    return Completion::Throw(interp.create_type_error(
                        "WeakRef: target must be an object or non-registered symbol",
                    ));
                }
                // OrdinaryCreateFromConstructor(NewTarget, "%WeakRef.prototype%")
                let proto = match interp
                    .get_prototype_from_new_target_realm(|realm| realm.weakref_prototype)
                {
                    Ok(p) => p,
                    Err(e) => return Completion::Throw(e),
                };
                let obj_id = interp.create_object_id();
                interp
                    .get_object_cell_expect(obj_id)
                    .borrow_mut()
                    .class_name = "WeakRef".to_string();
                if let Some(p) = proto {
                    interp
                        .get_object_cell_expect(obj_id)
                        .borrow_mut()
                        .prototype_id = Some(p);
                }
                interp
                    .get_object_cell_expect(obj_id)
                    .borrow_mut()
                    .primitive_value = Some(target);
                let id = obj_id;
                Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
            },
        ));

        if let JsValue::Object(ref ctor_obj) = weakref_ctor
            && let Some(obj) = self.get_object_cell(ctor_obj.id)
        {
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(
                    JsValue::Object(crate::types::JsObject { id: proto_id }),
                    false,
                    false,
                    false,
                ),
            );
        }

        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_property(
                "constructor".to_string(),
                PropertyDescriptor::data(weakref_ctor.clone(), true, false, true),
            );

        self.realm()
            .global_env
            .borrow_mut()
            .declare("WeakRef", BindingKind::Var);
        let _ = self
            .realm()
            .global_env
            .borrow_mut()
            .set("WeakRef", weakref_ctor);

        self.realm_mut().weakref_prototype = Some(proto_id);
    }

    pub(crate) fn setup_finalization_registry(&mut self) {
        let proto_id = self.create_object_id();
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .class_name = "FinalizationRegistry".to_string();

        // FinalizationRegistry.prototype.register
        self.define_method(proto_id, "register", 2,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object_cell(o.id)
                {
                    // Check [[Cells]] internal slot: class_name + map_data.is_some()
                    let has_cells = {
                        let b = obj.borrow();
                        b.class_name == "FinalizationRegistry"
                            && b.finalization_registry().is_some()
                    };
                    if has_cells {
                        let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if !interp.can_be_held_weakly(&target) {
                            return Completion::Throw(interp.create_type_error(
                                "FinalizationRegistry.register: target must be an object or non-registered symbol",
                            ));
                        }
                        let held_value = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                        // SameValue(target, heldValue) => TypeError
                        if same_value(&target, &held_value) {
                            return Completion::Throw(interp.create_type_error(
                                "FinalizationRegistry.register: target and heldValue must not be the same",
                            ));
                        }
                        let unregister_token = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                        // If CanBeHeldWeakly(unregisterToken) is false:
                        //   If unregisterToken is not undefined, throw TypeError
                        if !interp.can_be_held_weakly(&unregister_token)
                            && !matches!(unregister_token, JsValue::Undefined) {
                                return Completion::Throw(interp.create_type_error(
                                    "FinalizationRegistry.register: unregisterToken must be an object, non-registered symbol, or undefined",
                                ));
                            }
                        // Store cell: map_data stores (target, heldValue), set_data stores unregisterToken
                        let token_entry = if matches!(unregister_token, JsValue::Undefined) {
                            None
                        } else {
                            Some(unregister_token)
                        };
                        if let Some(obj_rc) = interp.get_object_cell(o.id)
                            && let Some((cells, tokens)) =
                                obj_rc.borrow_mut().finalization_registry_mut()
                        {
                            cells.push(Some((target, held_value)));
                            tokens.push(token_entry);
                        }
                        return Completion::Normal(JsValue::Undefined);
                    }
                }
                Completion::Throw(interp.create_type_error(
                    "FinalizationRegistry.prototype.register requires a FinalizationRegistry",
                ))
            },
        );

        // FinalizationRegistry.prototype.unregister
        self.define_method(proto_id, "unregister", 1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object_cell(o.id)
                {
                    let has_cells = {
                        let b = obj.borrow();
                        b.class_name == "FinalizationRegistry"
                            && b.finalization_registry().is_some()
                    };
                    if has_cells {
                        let token = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if !interp.can_be_held_weakly(&token) {
                            return Completion::Throw(interp.create_type_error(
                                "FinalizationRegistry.unregister: unregisterToken must be an object or non-registered symbol",
                            ));
                        }
                        // Remove cells whose unregisterToken matches
                        let mut removed = false;
                        if let Some(obj_rc) = interp.get_object_cell(o.id)
                            && let Some((cells, tokens)) =
                                obj_rc.borrow_mut().finalization_registry_mut()
                        {
                            for i in 0..cells.len() {
                                let tok_matches = tokens
                                    .get(i)
                                    .and_then(|t| t.as_ref())
                                    .is_some_and(|tok| same_value(tok, &token));
                                let cell_some = cells.get(i).is_some_and(|c| c.is_some());
                                if cell_some && tok_matches {
                                    cells[i] = None;
                                    tokens[i] = None;
                                    removed = true;
                                }
                            }
                        }
                        return Completion::Normal(JsValue::Boolean(removed));
                    }
                }
                Completion::Throw(interp.create_type_error(
                    "FinalizationRegistry.prototype.unregister requires a FinalizationRegistry",
                ))
            },
        );

        // FinalizationRegistry.prototype.cleanupSome
        self.define_method(proto_id, "cleanupSome", 0, |interp, this, _args| {
            if let JsValue::Object(o) = this
                && let Some(obj) = interp.get_object_cell(o.id)
            {
                let has_cells = {
                    let b = obj.borrow();
                    b.class_name == "FinalizationRegistry" && b.finalization_registry().is_some()
                };
                if has_cells {
                    return Completion::Normal(JsValue::Undefined);
                }
            }
            Completion::Throw(interp.create_type_error(
                "FinalizationRegistry.prototype.cleanupSome requires a FinalizationRegistry",
            ))
        });

        // @@toStringTag
        self.define_to_string_tag(proto_id, "FinalizationRegistry");

        // FinalizationRegistry constructor
        let fr_ctor = self.create_function(JsFunction::constructor(
            "FinalizationRegistry".to_string(),
            1,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Constructor FinalizationRegistry requires 'new'"),
                    );
                }
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(&callback, JsValue::Object(_)) {
                    return Completion::Throw(interp.create_type_error(
                        "FinalizationRegistry requires a callable cleanup callback",
                    ));
                }
                if let JsValue::Object(ref o) = callback
                    && let Some(obj) = interp.get_object_cell(o.id)
                    && obj.borrow().callable.is_none()
                {
                    return Completion::Throw(interp.create_type_error(
                        "FinalizationRegistry requires a callable cleanup callback",
                    ));
                }
                // OrdinaryCreateFromConstructor(NewTarget, "%FinalizationRegistry.prototype%")
                let proto = match interp.get_prototype_from_new_target_realm(|realm| {
                    realm.finalization_registry_prototype
                }) {
                    Ok(p) => p,
                    Err(e) => return Completion::Throw(e),
                };
                let obj_id = interp.create_object_id();
                interp
                    .get_object_cell_expect(obj_id)
                    .borrow_mut()
                    .class_name = "FinalizationRegistry".to_string();
                if let Some(p) = proto {
                    interp
                        .get_object_cell_expect(obj_id)
                        .borrow_mut()
                        .prototype_id = Some(p);
                }
                interp
                    .get_object_cell_expect(obj_id)
                    .borrow_mut()
                    .primitive_value = Some(callback);
                // Initialize [[Cells]] as empty FinalizationRegistry slot data.
                interp.get_object_cell_expect(obj_id).borrow_mut().kind =
                    crate::interpreter::types::ObjectKind::FinalizationRegistry {
                        cells: Vec::new(),
                        tokens: Vec::new(),
                    };
                let id = obj_id;
                Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
            },
        ));

        if let JsValue::Object(ref ctor_obj) = fr_ctor
            && let Some(obj) = self.get_object_cell(ctor_obj.id)
        {
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(
                    JsValue::Object(crate::types::JsObject { id: proto_id }),
                    false,
                    false,
                    false,
                ),
            );
        }

        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_property(
                "constructor".to_string(),
                PropertyDescriptor::data(fr_ctor.clone(), true, false, true),
            );

        self.realm()
            .global_env
            .borrow_mut()
            .declare("FinalizationRegistry", BindingKind::Var);
        let _ = self
            .realm()
            .global_env
            .borrow_mut()
            .set("FinalizationRegistry", fr_ctor);

        self.realm_mut().finalization_registry_prototype = Some(proto_id);
    }
}
