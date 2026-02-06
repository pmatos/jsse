use super::super::*;

impl Interpreter {
    pub(crate) fn setup_map_prototype(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Map".to_string();

        // Map iterator prototype
        let map_iter_proto = self.create_object();
        map_iter_proto.borrow_mut().prototype = self.iterator_prototype.clone();
        map_iter_proto.borrow_mut().class_name = "Map Iterator".to_string();

        map_iter_proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str("Map Iterator")),
                false,
                false,
                true,
            ),
        );

        let map_iter_next = self.create_function(JsFunction::native(
            "next".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let state = obj.borrow().iterator_state.clone();
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
                            let map_data = map_obj.borrow().map_data.clone();
                            if let Some(entries) = map_data {
                                let mut i = index;
                                while i < entries.len() {
                                    if let Some(ref entry) = entries[i] {
                                        let result = match kind {
                                            IteratorKind::Key => entry.0.clone(),
                                            IteratorKind::Value => entry.1.clone(),
                                            IteratorKind::KeyValue => interp.create_array(vec![
                                                entry.0.clone(),
                                                entry.1.clone(),
                                            ]),
                                        };
                                        obj.borrow_mut().iterator_state =
                                            Some(IteratorState::MapIterator {
                                                map_id,
                                                index: i + 1,
                                                kind,
                                                done: false,
                                            });
                                        return Completion::Normal(
                                            interp.create_iter_result_object(result, false),
                                        );
                                    }
                                    i += 1;
                                }
                            }
                        }
                        obj.borrow_mut().iterator_state = Some(IteratorState::MapIterator {
                            map_id,
                            index,
                            kind,
                            done: true,
                        });
                        return Completion::Normal(
                            interp.create_iter_result_object(JsValue::Undefined, true),
                        );
                    }
                }
                let err =
                    interp.create_type_error("Map Iterator.prototype.next requires a Map Iterator");
                Completion::Throw(err)
            },
        ));
        map_iter_proto
            .borrow_mut()
            .insert_builtin("next".to_string(), map_iter_next);

        if let Some(key) = self.get_symbol_iterator_key() {
            let iter_self_fn = self.create_function(JsFunction::native(
                "[Symbol.iterator]".to_string(),
                0,
                |_interp, this, _args| Completion::Normal(this.clone()),
            ));
            map_iter_proto.borrow_mut().insert_property(
                key,
                PropertyDescriptor::data(iter_self_fn, true, false, true),
            );
        }

        self.map_iterator_prototype = Some(map_iter_proto);

        // Helper to create map iterators
        fn create_map_iterator(
            interp: &mut Interpreter,
            map_id: u64,
            kind: IteratorKind,
        ) -> JsValue {
            let mut obj_data = JsObjectData::new();
            obj_data.prototype = interp
                .map_iterator_prototype
                .clone()
                .or(interp.iterator_prototype.clone())
                .or(interp.object_prototype.clone());
            obj_data.class_name = "Map Iterator".to_string();
            obj_data.iterator_state = Some(IteratorState::MapIterator {
                map_id,
                index: 0,
                kind,
                done: false,
            });
            let obj = Rc::new(RefCell::new(obj_data));
            let id = interp.allocate_object_slot(obj);
            JsValue::Object(crate::types::JsObject { id })
        }

        // Map.prototype.entries
        let entries_fn = self.create_function(JsFunction::native(
            "entries".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                    && obj.borrow().map_data.is_some()
                {
                    return Completion::Normal(create_map_iterator(
                        interp,
                        o.id,
                        IteratorKind::KeyValue,
                    ));
                }
                let err = interp.create_type_error("Map.prototype.entries requires a Map");
                Completion::Throw(err)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("entries".to_string(), entries_fn.clone());

        // Map.prototype[@@iterator] = entries
        if let Some(key) = self.get_symbol_iterator_key() {
            proto
                .borrow_mut()
                .insert_property(key, PropertyDescriptor::data(entries_fn, true, false, true));
        }

        // Map.prototype.keys
        let keys_fn = self.create_function(JsFunction::native(
            "keys".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                    && obj.borrow().map_data.is_some()
                {
                    return Completion::Normal(create_map_iterator(
                        interp,
                        o.id,
                        IteratorKind::Key,
                    ));
                }
                let err = interp.create_type_error("Map.prototype.keys requires a Map");
                Completion::Throw(err)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("keys".to_string(), keys_fn);

        // Map.prototype.values
        let values_fn = self.create_function(JsFunction::native(
            "values".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                    && obj.borrow().map_data.is_some()
                {
                    return Completion::Normal(create_map_iterator(
                        interp,
                        o.id,
                        IteratorKind::Value,
                    ));
                }
                let err = interp.create_type_error("Map.prototype.values requires a Map");
                Completion::Throw(err)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("values".to_string(), values_fn);

        // Map.prototype.get
        let get_fn = self.create_function(JsFunction::native(
            "get".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let map_data = obj.borrow().map_data.clone();
                    if let Some(entries) = map_data {
                        let key = args.first().cloned().unwrap_or(JsValue::Undefined);
                        for entry in entries.iter().flatten() {
                            if same_value_zero(&entry.0, &key) {
                                return Completion::Normal(entry.1.clone());
                            }
                        }
                        return Completion::Normal(JsValue::Undefined);
                    }
                }
                let err = interp.create_type_error("Map.prototype.get requires a Map");
                Completion::Throw(err)
            },
        ));
        proto.borrow_mut().insert_builtin("get".to_string(), get_fn);

        // Map.prototype.set
        let set_fn = self.create_function(JsFunction::native(
            "set".to_string(),
            2,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let has_map = obj.borrow().map_data.is_some();
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
                        let entries = borrowed.map_data.as_mut().unwrap();
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
            },
        ));
        proto.borrow_mut().insert_builtin("set".to_string(), set_fn);

        // Map.prototype.has
        let has_fn = self.create_function(JsFunction::native(
            "has".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let map_data = obj.borrow().map_data.clone();
                    if let Some(entries) = map_data {
                        let key = args.first().cloned().unwrap_or(JsValue::Undefined);
                        for entry in entries.iter().flatten() {
                            if same_value_zero(&entry.0, &key) {
                                return Completion::Normal(JsValue::Boolean(true));
                            }
                        }
                        return Completion::Normal(JsValue::Boolean(false));
                    }
                }
                let err = interp.create_type_error("Map.prototype.has requires a Map");
                Completion::Throw(err)
            },
        ));
        proto.borrow_mut().insert_builtin("has".to_string(), has_fn);

        // Map.prototype.delete
        let delete_fn = self.create_function(JsFunction::native(
            "delete".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let has_map = obj.borrow().map_data.is_some();
                    if has_map {
                        let key = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let mut borrowed = obj.borrow_mut();
                        let entries = borrowed.map_data.as_mut().unwrap();
                        for entry in entries.iter_mut() {
                            let matches =
                                entry.as_ref().is_some_and(|e| same_value_zero(&e.0, &key));
                            if matches {
                                *entry = None;
                                return Completion::Normal(JsValue::Boolean(true));
                            }
                        }
                        return Completion::Normal(JsValue::Boolean(false));
                    }
                }
                let err = interp.create_type_error("Map.prototype.delete requires a Map");
                Completion::Throw(err)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("delete".to_string(), delete_fn);

        // Map.prototype.clear
        let clear_fn = self.create_function(JsFunction::native(
            "clear".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let has_map = obj.borrow().map_data.is_some();
                    if has_map {
                        obj.borrow_mut().map_data = Some(Vec::new());
                        return Completion::Normal(JsValue::Undefined);
                    }
                }
                let err = interp.create_type_error("Map.prototype.clear requires a Map");
                Completion::Throw(err)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("clear".to_string(), clear_fn);

        // Map.prototype.forEach
        let foreach_fn = self.create_function(JsFunction::native(
            "forEach".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id) {
                        let has_map = obj.borrow().map_data.is_some();
                        if has_map {
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
                                    let entries = borrowed.map_data.as_ref().unwrap();
                                    if i >= entries.len() { break; }
                                    entries[i].clone()
                                };
                                if let Some((k, v)) = entry {
                                    let result = interp.call_function(&callback, &this_arg, &[v, k, this.clone()]);
                                    if result.is_abrupt() { return result; }
                                }
                                i += 1;
                            }
                            return Completion::Normal(JsValue::Undefined);
                        }
                    }
                let err = interp.create_type_error("Map.prototype.forEach requires a Map");
                Completion::Throw(err)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("forEach".to_string(), foreach_fn);

        // Map.prototype.size (getter)
        let size_getter = self.create_function(JsFunction::native(
            "get size".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let map_data = obj.borrow().map_data.clone();
                    if let Some(entries) = map_data {
                        let count = entries.iter().filter(|e| e.is_some()).count();
                        return Completion::Normal(JsValue::Number(count as f64));
                    }
                }
                let err = interp.create_type_error("Map.prototype.size requires a Map");
                Completion::Throw(err)
            },
        ));
        proto.borrow_mut().insert_property(
            "size".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(size_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );

        // @@toStringTag
        proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str("Map")),
                false,
                false,
                true,
            ),
        );

        // constructor property
        let proto_id = proto.borrow().id.unwrap();
        let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });

        // Map constructor
        let map_proto_clone = proto.clone();
        let map_ctor = self.create_function(JsFunction::constructor(
            "Map".to_string(),
            0,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    let err = interp.create_type_error("Constructor Map requires 'new'");
                    return Completion::Throw(err);
                }

                let obj = interp.create_object();
                obj.borrow_mut().prototype = Some(map_proto_clone.clone());
                obj.borrow_mut().class_name = "Map".to_string();
                obj.borrow_mut().map_data = Some(Vec::new());
                let obj_id = obj.borrow().id.unwrap();
                let this_val = JsValue::Object(crate::types::JsObject { id: obj_id });

                let iterable = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !iterable.is_undefined() && !iterable.is_null() {
                    // Get adder = this.set
                    let adder = obj.borrow().get_property("set");
                    if !matches!(&adder, JsValue::Object(ao) if interp.get_object(ao.id).is_some_and(|o| o.borrow().callable.is_some())) {
                        let err = interp.create_type_error("Map.prototype.set is not a function");
                        return Completion::Throw(err);
                    }

                    // Get iterator from iterable
                    let iter_key = interp.get_symbol_iterator_key();
                    let iterator_fn = if let Some(ref key) = iter_key {
                        if let JsValue::Object(io) = &iterable {
                            if let Some(iter_obj) = interp.get_object(io.id) {
                                let v = iter_obj.borrow().get_property(key);
                                if v.is_undefined() { JsValue::Undefined } else { v }
                            } else { JsValue::Undefined }
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

                    // Iterate
                    loop {
                        let next_fn = if let JsValue::Object(io) = &iterator {
                            if let Some(iter_obj) = interp.get_object(io.id) {
                                iter_obj.borrow().get_property("next")
                            } else { JsValue::Undefined }
                        } else { JsValue::Undefined };

                        let next_result = match interp.call_function(&next_fn, &iterator, &[]) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };

                        let done = if let JsValue::Object(ro) = &next_result {
                            if let Some(result_obj) = interp.get_object(ro.id) {
                                let d = result_obj.borrow().get_property("done");
                                matches!(d, JsValue::Boolean(true))
                            } else { false }
                        } else { false };

                        if done { break; }

                        let value = if let JsValue::Object(ro) = &next_result {
                            if let Some(result_obj) = interp.get_object(ro.id) {
                                result_obj.borrow().get_property("value")
                            } else { JsValue::Undefined }
                        } else { JsValue::Undefined };

                        // value should be [key, value]
                        let (k, v) = if let JsValue::Object(vo) = &value {
                            if let Some(val_obj) = interp.get_object(vo.id) {
                                let borrowed = val_obj.borrow();
                                let k = borrowed.get_property("0");
                                let v = borrowed.get_property("1");
                                (k, v)
                            } else {
                                (JsValue::Undefined, JsValue::Undefined)
                            }
                        } else {
                            let err = interp.create_type_error("Iterator value is not an object");
                            return Completion::Throw(err);
                        };

                        match interp.call_function(&adder, &this_val, &[k, v]) {
                            Completion::Normal(_) => {}
                            other => return other,
                        }
                    }
                }

                Completion::Normal(this_val)
            },
        ));

        // Set Map.prototype on ctor, ctor on prototype
        if let JsValue::Object(ctor_obj) = &map_ctor
            && let Some(obj) = self.get_object(ctor_obj.id)
        {
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val.clone(), false, false, false),
            );
        }
        proto.borrow_mut().insert_property(
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
        }

        // Map.groupBy static method
        if let JsValue::Object(ref ctor_ref) = map_ctor
            && let Some(ctor_obj) = self.get_object(ctor_ref.id)
        {
            let map_proto_for_groupby = proto.clone();
            let group_by_fn = self.create_function(JsFunction::native(
                "groupBy".to_string(),
                2,
                move |interp, _this, args| {
                    let items = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let callback = args.get(1).cloned().unwrap_or(JsValue::Undefined);

                    // 1. Validate callback is callable
                    if !matches!(&callback, JsValue::Object(o) if interp.get_object(o.id)
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
                    let result_map = interp.create_object();
                    result_map.borrow_mut().prototype = Some(map_proto_for_groupby.clone());
                    result_map.borrow_mut().class_name = "Map".to_string();
                    result_map.borrow_mut().map_data = Some(Vec::new());
                    let result_id = result_map.borrow().id.unwrap();
                    let result_val = JsValue::Object(crate::types::JsObject { id: result_id });

                    // 4. Iterate and group
                    let mut k: u64 = 0;
                    loop {
                        let next = match interp.iterator_step(&iterator) {
                            Ok(Some(v)) => v,
                            Ok(None) => break,
                            Err(e) => return Completion::Throw(e),
                        };
                        let value = interp.iterator_value(&next);

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
                        if let Some(map_obj) = interp.get_object(result_id) {
                            let mut borrowed = map_obj.borrow_mut();
                            let entries = borrowed.map_data.as_mut().unwrap();

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
                                if let Some((_, arr_val)) = entries[idx].as_ref() {
                                    if let JsValue::Object(arr_obj) = arr_val {
                                        let arr_id = arr_obj.id;
                                        drop(borrowed);
                                        if let Some(arr) = interp.get_object(arr_id) {
                                            let len_val = arr.borrow().get_property("length");
                                            let len = to_number(&len_val) as usize;
                                            arr.borrow_mut().insert_builtin(len.to_string(), value);
                                            arr.borrow_mut().insert_builtin(
                                                "length".to_string(),
                                                JsValue::Number((len + 1) as f64),
                                            );
                                        }
                                    }
                                }
                            } else {
                                // Create new array and add entry
                                drop(borrowed);
                                let new_arr = interp.create_array(vec![value]);
                                if let Some(map_obj) = interp.get_object(result_id) {
                                    let mut borrowed = map_obj.borrow_mut();
                                    let entries = borrowed.map_data.as_mut().unwrap();
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

        self.global_env
            .borrow_mut()
            .declare("Map", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("Map", map_ctor);

        self.map_prototype = Some(proto);
    }

    pub(crate) fn setup_set_prototype(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Set".to_string();

        // Set iterator prototype
        let set_iter_proto = self.create_object();
        set_iter_proto.borrow_mut().prototype = self.iterator_prototype.clone();
        set_iter_proto.borrow_mut().class_name = "Set Iterator".to_string();

        set_iter_proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str("Set Iterator")),
                false,
                false,
                true,
            ),
        );

        let set_iter_next = self.create_function(JsFunction::native(
            "next".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let state = obj.borrow().iterator_state.clone();
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
                            let set_data = set_obj.borrow().set_data.clone();
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
                                        obj.borrow_mut().iterator_state =
                                            Some(IteratorState::SetIterator {
                                                set_id,
                                                index: i + 1,
                                                kind,
                                                done: false,
                                            });
                                        return Completion::Normal(
                                            interp.create_iter_result_object(result, false),
                                        );
                                    }
                                    i += 1;
                                }
                            }
                        }
                        obj.borrow_mut().iterator_state = Some(IteratorState::SetIterator {
                            set_id,
                            index,
                            kind,
                            done: true,
                        });
                        return Completion::Normal(
                            interp.create_iter_result_object(JsValue::Undefined, true),
                        );
                    }
                }
                let err =
                    interp.create_type_error("Set Iterator.prototype.next requires a Set Iterator");
                Completion::Throw(err)
            },
        ));
        set_iter_proto
            .borrow_mut()
            .insert_builtin("next".to_string(), set_iter_next);

        if let Some(key) = self.get_symbol_iterator_key() {
            let iter_self_fn = self.create_function(JsFunction::native(
                "[Symbol.iterator]".to_string(),
                0,
                |_interp, this, _args| Completion::Normal(this.clone()),
            ));
            set_iter_proto.borrow_mut().insert_property(
                key,
                PropertyDescriptor::data(iter_self_fn, true, false, true),
            );
        }

        self.set_iterator_prototype = Some(set_iter_proto);

        fn create_set_iterator(
            interp: &mut Interpreter,
            set_id: u64,
            kind: IteratorKind,
        ) -> JsValue {
            let mut obj_data = JsObjectData::new();
            obj_data.prototype = interp
                .set_iterator_prototype
                .clone()
                .or(interp.iterator_prototype.clone())
                .or(interp.object_prototype.clone());
            obj_data.class_name = "Set Iterator".to_string();
            obj_data.iterator_state = Some(IteratorState::SetIterator {
                set_id,
                index: 0,
                kind,
                done: false,
            });
            let obj = Rc::new(RefCell::new(obj_data));
            let id = interp.allocate_object_slot(obj);
            JsValue::Object(crate::types::JsObject { id })
        }

        // Set.prototype.values
        let values_fn = self.create_function(JsFunction::native(
            "values".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                    && obj.borrow().set_data.is_some()
                {
                    return Completion::Normal(create_set_iterator(
                        interp,
                        o.id,
                        IteratorKind::Value,
                    ));
                }
                let err = interp.create_type_error("Set.prototype.values requires a Set");
                Completion::Throw(err)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("values".to_string(), values_fn.clone());

        // Set.prototype.keys = Set.prototype.values
        proto
            .borrow_mut()
            .insert_builtin("keys".to_string(), values_fn.clone());

        // Set.prototype[@@iterator] = values
        if let Some(key) = self.get_symbol_iterator_key() {
            proto
                .borrow_mut()
                .insert_property(key, PropertyDescriptor::data(values_fn, true, false, true));
        }

        // Set.prototype.entries
        let entries_fn = self.create_function(JsFunction::native(
            "entries".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                    && obj.borrow().set_data.is_some()
                {
                    return Completion::Normal(create_set_iterator(
                        interp,
                        o.id,
                        IteratorKind::KeyValue,
                    ));
                }
                let err = interp.create_type_error("Set.prototype.entries requires a Set");
                Completion::Throw(err)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("entries".to_string(), entries_fn);

        // Set.prototype.add
        let add_fn = self.create_function(JsFunction::native(
            "add".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let has_set = obj.borrow().set_data.is_some();
                    if has_set {
                        let mut value = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if let JsValue::Number(n) = &value
                            && *n == 0.0
                            && n.is_sign_negative()
                        {
                            value = JsValue::Number(0.0);
                        }
                        let mut borrowed = obj.borrow_mut();
                        let entries = borrowed.set_data.as_mut().unwrap();
                        for entry in entries.iter().flatten() {
                            if same_value_zero(entry, &value) {
                                return Completion::Normal(this.clone());
                            }
                        }
                        entries.push(Some(value));
                        return Completion::Normal(this.clone());
                    }
                }
                let err = interp.create_type_error("Set.prototype.add requires a Set");
                Completion::Throw(err)
            },
        ));
        proto.borrow_mut().insert_builtin("add".to_string(), add_fn);

        // Set.prototype.has
        let has_fn = self.create_function(JsFunction::native(
            "has".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let set_data = obj.borrow().set_data.clone();
                    if let Some(entries) = set_data {
                        let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                        for entry in entries.iter().flatten() {
                            if same_value_zero(entry, &value) {
                                return Completion::Normal(JsValue::Boolean(true));
                            }
                        }
                        return Completion::Normal(JsValue::Boolean(false));
                    }
                }
                let err = interp.create_type_error("Set.prototype.has requires a Set");
                Completion::Throw(err)
            },
        ));
        proto.borrow_mut().insert_builtin("has".to_string(), has_fn);

        // Set.prototype.delete
        let delete_fn = self.create_function(JsFunction::native(
            "delete".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let has_set = obj.borrow().set_data.is_some();
                    if has_set {
                        let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let mut borrowed = obj.borrow_mut();
                        let entries = borrowed.set_data.as_mut().unwrap();
                        for entry in entries.iter_mut() {
                            let matches =
                                entry.as_ref().is_some_and(|e| same_value_zero(e, &value));
                            if matches {
                                *entry = None;
                                return Completion::Normal(JsValue::Boolean(true));
                            }
                        }
                        return Completion::Normal(JsValue::Boolean(false));
                    }
                }
                let err = interp.create_type_error("Set.prototype.delete requires a Set");
                Completion::Throw(err)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("delete".to_string(), delete_fn);

        // Set.prototype.clear
        let clear_fn = self.create_function(JsFunction::native(
            "clear".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let has_set = obj.borrow().set_data.is_some();
                    if has_set {
                        obj.borrow_mut().set_data = Some(Vec::new());
                        return Completion::Normal(JsValue::Undefined);
                    }
                }
                let err = interp.create_type_error("Set.prototype.clear requires a Set");
                Completion::Throw(err)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("clear".to_string(), clear_fn);

        // Set.prototype.forEach
        let foreach_fn = self.create_function(JsFunction::native(
            "forEach".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id) {
                        let has_set = obj.borrow().set_data.is_some();
                        if has_set {
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
                                    let entries = borrowed.set_data.as_ref().unwrap();
                                    if i >= entries.len() { break; }
                                    entries[i].clone()
                                };
                                if let Some(v) = entry {
                                    let result = interp.call_function(&callback, &this_arg, &[v.clone(), v, this.clone()]);
                                    if result.is_abrupt() { return result; }
                                }
                                i += 1;
                            }
                            return Completion::Normal(JsValue::Undefined);
                        }
                    }
                let err = interp.create_type_error("Set.prototype.forEach requires a Set");
                Completion::Throw(err)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("forEach".to_string(), foreach_fn);

        // Set.prototype.size (getter)
        let size_getter = self.create_function(JsFunction::native(
            "get size".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let set_data = obj.borrow().set_data.clone();
                    if let Some(entries) = set_data {
                        let count = entries.iter().filter(|e| e.is_some()).count();
                        return Completion::Normal(JsValue::Number(count as f64));
                    }
                }
                let err = interp.create_type_error("Set.prototype.size requires a Set");
                Completion::Throw(err)
            },
        ));
        proto.borrow_mut().insert_property(
            "size".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(size_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );

        // ES2025 Set methods

        // Set.prototype.union
        let union_fn = self.create_function(JsFunction::native(
            "union".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let set_data = obj.borrow().set_data.clone();
                    if let Some(entries) = set_data {
                        let other = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let other_rec = match get_set_record(interp, &other) {
                            Ok(r) => r,
                            Err(e) => return Completion::Throw(e),
                        };
                        let new_obj = interp.create_object();
                        new_obj.borrow_mut().prototype = interp.set_prototype.clone();
                        new_obj.borrow_mut().class_name = "Set".to_string();
                        let mut new_entries: Vec<Option<JsValue>> = Vec::new();
                        for entry in entries.iter().flatten() {
                            new_entries.push(Some(entry.clone()));
                        }
                        // Iterate other's keys
                        let keys_iter = match interp.call_function(&other_rec.keys, &other, &[]) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        loop {
                            let next_fn = if let JsValue::Object(io) = &keys_iter {
                                if let Some(iter_obj) = interp.get_object(io.id) {
                                    iter_obj.borrow().get_property("next")
                                } else {
                                    JsValue::Undefined
                                }
                            } else {
                                JsValue::Undefined
                            };
                            let next_result = match interp.call_function(&next_fn, &keys_iter, &[])
                            {
                                Completion::Normal(v) => v,
                                other => return other,
                            };
                            let (done, value) = extract_iter_result(interp, &next_result);
                            if done {
                                break;
                            }
                            let mut val = value;
                            if let JsValue::Number(n) = &val
                                && *n == 0.0
                                && n.is_sign_negative()
                            {
                                val = JsValue::Number(0.0);
                            }
                            let exists = new_entries
                                .iter()
                                .flatten()
                                .any(|e| same_value_zero(e, &val));
                            if !exists {
                                new_entries.push(Some(val));
                            }
                        }
                        new_obj.borrow_mut().set_data = Some(new_entries);
                        let id = new_obj.borrow().id.unwrap();
                        return Completion::Normal(JsValue::Object(crate::types::JsObject { id }));
                    }
                }
                let err = interp.create_type_error("Set.prototype.union requires a Set");
                Completion::Throw(err)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("union".to_string(), union_fn);

        // Set.prototype.intersection
        let intersection_fn = self.create_function(JsFunction::native(
            "intersection".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let set_data = obj.borrow().set_data.clone();
                    if let Some(entries) = set_data {
                        let other = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let other_rec = match get_set_record(interp, &other) {
                            Ok(r) => r,
                            Err(e) => return Completion::Throw(e),
                        };
                        let new_obj = interp.create_object();
                        new_obj.borrow_mut().prototype = interp.set_prototype.clone();
                        new_obj.borrow_mut().class_name = "Set".to_string();
                        let mut new_entries: Vec<Option<JsValue>> = Vec::new();
                        let this_size = entries.iter().filter(|e| e.is_some()).count();

                        if this_size as f64 <= other_rec.size {
                            for entry in entries.iter().flatten() {
                                let has_result = match interp.call_function(
                                    &other_rec.has,
                                    &other,
                                    &[entry.clone()],
                                ) {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                };
                                if matches!(has_result, JsValue::Boolean(true))
                                    || (!matches!(has_result, JsValue::Boolean(false))
                                        && !has_result.is_undefined()
                                        && !has_result.is_null()
                                        && !matches!(has_result, JsValue::Number(n) if n == 0.0))
                                {
                                    let mut val = entry.clone();
                                    if let JsValue::Number(n) = &val
                                        && *n == 0.0
                                        && n.is_sign_negative()
                                    {
                                        val = JsValue::Number(0.0);
                                    }
                                    new_entries.push(Some(val));
                                }
                            }
                        } else {
                            let keys_iter = match interp.call_function(&other_rec.keys, &other, &[])
                            {
                                Completion::Normal(v) => v,
                                other => return other,
                            };
                            loop {
                                let next_fn = if let JsValue::Object(io) = &keys_iter {
                                    if let Some(iter_obj) = interp.get_object(io.id) {
                                        iter_obj.borrow().get_property("next")
                                    } else {
                                        JsValue::Undefined
                                    }
                                } else {
                                    JsValue::Undefined
                                };
                                let next_result =
                                    match interp.call_function(&next_fn, &keys_iter, &[]) {
                                        Completion::Normal(v) => v,
                                        other => return other,
                                    };
                                let (done, value) = extract_iter_result(interp, &next_result);
                                if done {
                                    break;
                                }
                                // Re-read entries from this set since it may have changed
                                let current = obj.borrow().set_data.clone().unwrap_or_default();
                                let in_this =
                                    current.iter().flatten().any(|e| same_value_zero(e, &value));
                                if in_this {
                                    let mut val = value;
                                    if let JsValue::Number(n) = &val
                                        && *n == 0.0
                                        && n.is_sign_negative()
                                    {
                                        val = JsValue::Number(0.0);
                                    }
                                    if !new_entries
                                        .iter()
                                        .flatten()
                                        .any(|e| same_value_zero(e, &val))
                                    {
                                        new_entries.push(Some(val));
                                    }
                                }
                            }
                        }
                        new_obj.borrow_mut().set_data = Some(new_entries);
                        let id = new_obj.borrow().id.unwrap();
                        return Completion::Normal(JsValue::Object(crate::types::JsObject { id }));
                    }
                }
                let err = interp.create_type_error("Set.prototype.intersection requires a Set");
                Completion::Throw(err)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("intersection".to_string(), intersection_fn);

        // Set.prototype.difference
        let difference_fn = self.create_function(JsFunction::native(
            "difference".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let set_data = obj.borrow().set_data.clone();
                    if let Some(entries) = set_data {
                        let other = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let other_rec = match get_set_record(interp, &other) {
                            Ok(r) => r,
                            Err(e) => return Completion::Throw(e),
                        };
                        let new_obj = interp.create_object();
                        new_obj.borrow_mut().prototype = interp.set_prototype.clone();
                        new_obj.borrow_mut().class_name = "Set".to_string();
                        let mut new_entries: Vec<Option<JsValue>> = Vec::new();
                        let this_size = entries.iter().filter(|e| e.is_some()).count();

                        if this_size as f64 <= other_rec.size {
                            for entry in entries.iter().flatten() {
                                let has_result = match interp.call_function(
                                    &other_rec.has,
                                    &other,
                                    &[entry.clone()],
                                ) {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                };
                                let is_true = matches!(has_result, JsValue::Boolean(true));
                                if !is_true {
                                    new_entries.push(Some(entry.clone()));
                                }
                            }
                        } else {
                            // Copy all, then remove
                            for entry in entries.iter().flatten() {
                                new_entries.push(Some(entry.clone()));
                            }
                            let keys_iter = match interp.call_function(&other_rec.keys, &other, &[])
                            {
                                Completion::Normal(v) => v,
                                other => return other,
                            };
                            loop {
                                let next_fn = if let JsValue::Object(io) = &keys_iter {
                                    if let Some(iter_obj) = interp.get_object(io.id) {
                                        iter_obj.borrow().get_property("next")
                                    } else {
                                        JsValue::Undefined
                                    }
                                } else {
                                    JsValue::Undefined
                                };
                                let next_result =
                                    match interp.call_function(&next_fn, &keys_iter, &[]) {
                                        Completion::Normal(v) => v,
                                        other => return other,
                                    };
                                let (done, value) = extract_iter_result(interp, &next_result);
                                if done {
                                    break;
                                }
                                for entry in new_entries.iter_mut() {
                                    let matches =
                                        entry.as_ref().is_some_and(|e| same_value_zero(e, &value));
                                    if matches {
                                        *entry = None;
                                        break;
                                    }
                                }
                            }
                        }
                        new_obj.borrow_mut().set_data = Some(new_entries);
                        let id = new_obj.borrow().id.unwrap();
                        return Completion::Normal(JsValue::Object(crate::types::JsObject { id }));
                    }
                }
                let err = interp.create_type_error("Set.prototype.difference requires a Set");
                Completion::Throw(err)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("difference".to_string(), difference_fn);

        // Set.prototype.symmetricDifference
        let sym_diff_fn = self.create_function(JsFunction::native(
            "symmetricDifference".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let set_data = obj.borrow().set_data.clone();
                    if let Some(entries) = set_data {
                        let other = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let other_rec = match get_set_record(interp, &other) {
                            Ok(r) => r,
                            Err(e) => return Completion::Throw(e),
                        };
                        let new_obj = interp.create_object();
                        new_obj.borrow_mut().prototype = interp.set_prototype.clone();
                        new_obj.borrow_mut().class_name = "Set".to_string();
                        let mut new_entries: Vec<Option<JsValue>> = Vec::new();
                        for entry in entries.iter().flatten() {
                            new_entries.push(Some(entry.clone()));
                        }
                        let keys_iter = match interp.call_function(&other_rec.keys, &other, &[]) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        loop {
                            let next_fn = if let JsValue::Object(io) = &keys_iter {
                                if let Some(iter_obj) = interp.get_object(io.id) {
                                    iter_obj.borrow().get_property("next")
                                } else {
                                    JsValue::Undefined
                                }
                            } else {
                                JsValue::Undefined
                            };
                            let next_result = match interp.call_function(&next_fn, &keys_iter, &[])
                            {
                                Completion::Normal(v) => v,
                                other => return other,
                            };
                            let (done, value) = extract_iter_result(interp, &next_result);
                            if done {
                                break;
                            }
                            let mut val = value;
                            if let JsValue::Number(n) = &val
                                && *n == 0.0
                                && n.is_sign_negative()
                            {
                                val = JsValue::Number(0.0);
                            }
                            // Re-read this set data
                            let current = obj.borrow().set_data.clone().unwrap_or_default();
                            let in_this =
                                current.iter().flatten().any(|e| same_value_zero(e, &val));
                            if in_this {
                                // Remove from result
                                for entry in new_entries.iter_mut() {
                                    let matches =
                                        entry.as_ref().is_some_and(|e| same_value_zero(e, &val));
                                    if matches {
                                        *entry = None;
                                        break;
                                    }
                                }
                            } else {
                                new_entries.push(Some(val));
                            }
                        }
                        new_obj.borrow_mut().set_data = Some(new_entries);
                        let id = new_obj.borrow().id.unwrap();
                        return Completion::Normal(JsValue::Object(crate::types::JsObject { id }));
                    }
                }
                let err =
                    interp.create_type_error("Set.prototype.symmetricDifference requires a Set");
                Completion::Throw(err)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("symmetricDifference".to_string(), sym_diff_fn);

        // Set.prototype.isSubsetOf
        let is_subset_fn = self.create_function(JsFunction::native(
            "isSubsetOf".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let set_data = obj.borrow().set_data.clone();
                    if let Some(entries) = set_data {
                        let other = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let other_rec = match get_set_record(interp, &other) {
                            Ok(r) => r,
                            Err(e) => return Completion::Throw(e),
                        };
                        let this_size = entries.iter().filter(|e| e.is_some()).count();
                        if this_size as f64 > other_rec.size {
                            return Completion::Normal(JsValue::Boolean(false));
                        }
                        for entry in entries.iter().flatten() {
                            let has_result = match interp.call_function(
                                &other_rec.has,
                                &other,
                                &[entry.clone()],
                            ) {
                                Completion::Normal(v) => v,
                                other => return other,
                            };
                            let is_true = matches!(has_result, JsValue::Boolean(true));
                            if !is_true {
                                return Completion::Normal(JsValue::Boolean(false));
                            }
                        }
                        return Completion::Normal(JsValue::Boolean(true));
                    }
                }
                let err = interp.create_type_error("Set.prototype.isSubsetOf requires a Set");
                Completion::Throw(err)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("isSubsetOf".to_string(), is_subset_fn);

        // Set.prototype.isSupersetOf
        let is_superset_fn = self.create_function(JsFunction::native(
            "isSupersetOf".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let set_data = obj.borrow().set_data.clone();
                    if let Some(entries) = set_data {
                        let other = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let other_rec = match get_set_record(interp, &other) {
                            Ok(r) => r,
                            Err(e) => return Completion::Throw(e),
                        };
                        let this_size = entries.iter().filter(|e| e.is_some()).count();
                        if (this_size as f64) < other_rec.size {
                            return Completion::Normal(JsValue::Boolean(false));
                        }
                        let keys_iter = match interp.call_function(&other_rec.keys, &other, &[]) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        loop {
                            let next_fn = if let JsValue::Object(io) = &keys_iter {
                                if let Some(iter_obj) = interp.get_object(io.id) {
                                    iter_obj.borrow().get_property("next")
                                } else {
                                    JsValue::Undefined
                                }
                            } else {
                                JsValue::Undefined
                            };
                            let next_result = match interp.call_function(&next_fn, &keys_iter, &[])
                            {
                                Completion::Normal(v) => v,
                                other => return other,
                            };
                            let (done, value) = extract_iter_result(interp, &next_result);
                            if done {
                                break;
                            }
                            let current = obj.borrow().set_data.clone().unwrap_or_default();
                            let in_this =
                                current.iter().flatten().any(|e| same_value_zero(e, &value));
                            if !in_this {
                                return Completion::Normal(JsValue::Boolean(false));
                            }
                        }
                        return Completion::Normal(JsValue::Boolean(true));
                    }
                }
                let err = interp.create_type_error("Set.prototype.isSupersetOf requires a Set");
                Completion::Throw(err)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("isSupersetOf".to_string(), is_superset_fn);

        // Set.prototype.isDisjointFrom
        let is_disjoint_fn = self.create_function(JsFunction::native(
            "isDisjointFrom".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let set_data = obj.borrow().set_data.clone();
                    if let Some(entries) = set_data {
                        let other = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let other_rec = match get_set_record(interp, &other) {
                            Ok(r) => r,
                            Err(e) => return Completion::Throw(e),
                        };
                        let this_size = entries.iter().filter(|e| e.is_some()).count();
                        if this_size as f64 <= other_rec.size {
                            for entry in entries.iter().flatten() {
                                let has_result = match interp.call_function(
                                    &other_rec.has,
                                    &other,
                                    &[entry.clone()],
                                ) {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                };
                                let is_true = matches!(has_result, JsValue::Boolean(true));
                                if is_true {
                                    return Completion::Normal(JsValue::Boolean(false));
                                }
                            }
                        } else {
                            let keys_iter = match interp.call_function(&other_rec.keys, &other, &[])
                            {
                                Completion::Normal(v) => v,
                                other => return other,
                            };
                            loop {
                                let next_fn = if let JsValue::Object(io) = &keys_iter {
                                    if let Some(iter_obj) = interp.get_object(io.id) {
                                        iter_obj.borrow().get_property("next")
                                    } else {
                                        JsValue::Undefined
                                    }
                                } else {
                                    JsValue::Undefined
                                };
                                let next_result =
                                    match interp.call_function(&next_fn, &keys_iter, &[]) {
                                        Completion::Normal(v) => v,
                                        other => return other,
                                    };
                                let (done, value) = extract_iter_result(interp, &next_result);
                                if done {
                                    break;
                                }
                                let current = obj.borrow().set_data.clone().unwrap_or_default();
                                let in_this =
                                    current.iter().flatten().any(|e| same_value_zero(e, &value));
                                if in_this {
                                    return Completion::Normal(JsValue::Boolean(false));
                                }
                            }
                        }
                        return Completion::Normal(JsValue::Boolean(true));
                    }
                }
                let err = interp.create_type_error("Set.prototype.isDisjointFrom requires a Set");
                Completion::Throw(err)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("isDisjointFrom".to_string(), is_disjoint_fn);

        // @@toStringTag
        proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str("Set")),
                false,
                false,
                true,
            ),
        );

        let proto_id = proto.borrow().id.unwrap();
        let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });

        // Set constructor
        let set_proto_clone = proto.clone();
        let set_ctor = self.create_function(JsFunction::constructor(
            "Set".to_string(),
            0,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    let err = interp.create_type_error("Constructor Set requires 'new'");
                    return Completion::Throw(err);
                }

                let obj = interp.create_object();
                obj.borrow_mut().prototype = Some(set_proto_clone.clone());
                obj.borrow_mut().class_name = "Set".to_string();
                obj.borrow_mut().set_data = Some(Vec::new());
                let obj_id = obj.borrow().id.unwrap();
                let this_val = JsValue::Object(crate::types::JsObject { id: obj_id });

                let iterable = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !iterable.is_undefined() && !iterable.is_null() {
                    let adder = obj.borrow().get_property("add");
                    if !matches!(&adder, JsValue::Object(ao) if interp.get_object(ao.id).is_some_and(|o| o.borrow().callable.is_some())) {
                        let err = interp.create_type_error("Set.prototype.add is not a function");
                        return Completion::Throw(err);
                    }

                    let iter_key = interp.get_symbol_iterator_key();
                    let iterator_fn = if let Some(ref key) = iter_key {
                        if let JsValue::Object(io) = &iterable {
                            if let Some(iter_obj) = interp.get_object(io.id) {
                                let v = iter_obj.borrow().get_property(key);
                                if v.is_undefined() { JsValue::Undefined } else { v }
                            } else { JsValue::Undefined }
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
                            if let Some(iter_obj) = interp.get_object(io.id) {
                                iter_obj.borrow().get_property("next")
                            } else { JsValue::Undefined }
                        } else { JsValue::Undefined };

                        let next_result = match interp.call_function(&next_fn, &iterator, &[]) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };

                        let (done, value) = extract_iter_result(interp, &next_result);
                        if done { break; }

                        match interp.call_function(&adder, &this_val, &[value]) {
                            Completion::Normal(_) => {}
                            other => return other,
                        }
                    }
                }

                Completion::Normal(this_val)
            },
        ));

        if let JsValue::Object(ctor_obj) = &set_ctor
            && let Some(obj) = self.get_object(ctor_obj.id)
        {
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val.clone(), false, false, false),
            );
        }
        proto.borrow_mut().insert_property(
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
        }

        self.global_env
            .borrow_mut()
            .declare("Set", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("Set", set_ctor);

        self.set_prototype = Some(proto);
    }

    pub(crate) fn create_type_error(&mut self, msg: &str) -> JsValue {
        self.create_error("TypeError", msg)
    }

    pub(crate) fn setup_weakmap_prototype(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "WeakMap".to_string();

        // WeakMap.prototype.get
        let get_fn = self.create_function(JsFunction::native(
            "get".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let map_data = obj.borrow().map_data.clone();
                    if let Some(entries) = map_data {
                        let key = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if !matches!(key, JsValue::Object(_)) {
                            let err =
                                interp.create_type_error("Invalid value used as weak map key");
                            return Completion::Throw(err);
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
            },
        ));
        proto.borrow_mut().insert_builtin("get".to_string(), get_fn);

        // WeakMap.prototype.set
        let set_fn = self.create_function(JsFunction::native(
            "set".to_string(),
            2,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let has_map = obj.borrow().map_data.is_some();
                    if has_map {
                        let key = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if !matches!(key, JsValue::Object(_)) {
                            let err =
                                interp.create_type_error("Invalid value used as weak map key");
                            return Completion::Throw(err);
                        }
                        let value = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                        let mut borrowed = obj.borrow_mut();
                        let entries = borrowed.map_data.as_mut().unwrap();
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
            },
        ));
        proto.borrow_mut().insert_builtin("set".to_string(), set_fn);

        // WeakMap.prototype.has
        let has_fn = self.create_function(JsFunction::native(
            "has".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let map_data = obj.borrow().map_data.clone();
                    if let Some(entries) = map_data {
                        let key = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if !matches!(key, JsValue::Object(_)) {
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
            },
        ));
        proto.borrow_mut().insert_builtin("has".to_string(), has_fn);

        // WeakMap.prototype.delete
        let delete_fn = self.create_function(JsFunction::native(
            "delete".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let has_map = obj.borrow().map_data.is_some();
                    if has_map {
                        let key = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if !matches!(key, JsValue::Object(_)) {
                            return Completion::Normal(JsValue::Boolean(false));
                        }
                        let mut borrowed = obj.borrow_mut();
                        let entries = borrowed.map_data.as_mut().unwrap();
                        for entry in entries.iter_mut() {
                            let matches =
                                entry.as_ref().is_some_and(|e| strict_equality(&e.0, &key));
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
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("delete".to_string(), delete_fn);

        // @@toStringTag
        proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str("WeakMap")),
                false,
                false,
                true,
            ),
        );

        let proto_id = proto.borrow().id.unwrap();
        let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });

        // WeakMap constructor
        let weakmap_proto_clone = proto.clone();
        let weakmap_ctor = self.create_function(JsFunction::constructor(
            "WeakMap".to_string(),
            0,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    let err = interp.create_type_error("Constructor WeakMap requires 'new'");
                    return Completion::Throw(err);
                }

                let obj = interp.create_object();
                obj.borrow_mut().prototype = Some(weakmap_proto_clone.clone());
                obj.borrow_mut().class_name = "WeakMap".to_string();
                obj.borrow_mut().map_data = Some(Vec::new());
                let obj_id = obj.borrow().id.unwrap();
                let this_val = JsValue::Object(crate::types::JsObject { id: obj_id });

                let iterable = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !iterable.is_undefined() && !iterable.is_null() {
                    let adder = obj.borrow().get_property("set");
                    if !matches!(&adder, JsValue::Object(ao) if interp.get_object(ao.id).is_some_and(|o| o.borrow().callable.is_some())) {
                        let err = interp.create_type_error("WeakMap.prototype.set is not a function");
                        return Completion::Throw(err);
                    }

                    let iter_key = interp.get_symbol_iterator_key();
                    let iterator_fn = if let Some(ref key) = iter_key {
                        if let JsValue::Object(io) = &iterable {
                            if let Some(iter_obj) = interp.get_object(io.id) {
                                let v = iter_obj.borrow().get_property(key);
                                if v.is_undefined() { JsValue::Undefined } else { v }
                            } else { JsValue::Undefined }
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
                            if let Some(iter_obj) = interp.get_object(io.id) {
                                iter_obj.borrow().get_property("next")
                            } else { JsValue::Undefined }
                        } else { JsValue::Undefined };

                        let next_result = match interp.call_function(&next_fn, &iterator, &[]) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };

                        let done = if let JsValue::Object(ro) = &next_result {
                            if let Some(result_obj) = interp.get_object(ro.id) {
                                let d = result_obj.borrow().get_property("done");
                                matches!(d, JsValue::Boolean(true))
                            } else { false }
                        } else { false };

                        if done { break; }

                        let value = if let JsValue::Object(ro) = &next_result {
                            if let Some(result_obj) = interp.get_object(ro.id) {
                                result_obj.borrow().get_property("value")
                            } else { JsValue::Undefined }
                        } else { JsValue::Undefined };

                        let (k, v) = if let JsValue::Object(vo) = &value {
                            if let Some(val_obj) = interp.get_object(vo.id) {
                                let borrowed = val_obj.borrow();
                                let k = borrowed.get_property("0");
                                let v = borrowed.get_property("1");
                                (k, v)
                            } else {
                                (JsValue::Undefined, JsValue::Undefined)
                            }
                        } else {
                            let err = interp.create_type_error("Iterator value is not an object");
                            return Completion::Throw(err);
                        };

                        match interp.call_function(&adder, &this_val, &[k, v]) {
                            Completion::Normal(_) => {}
                            other => return other,
                        }
                    }
                }

                Completion::Normal(this_val)
            },
        ));

        if let JsValue::Object(ctor_obj) = &weakmap_ctor
            && let Some(obj) = self.get_object(ctor_obj.id)
        {
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val.clone(), false, false, false),
            );
        }
        proto.borrow_mut().insert_property(
            "constructor".to_string(),
            PropertyDescriptor::data(weakmap_ctor.clone(), true, false, true),
        );

        self.global_env
            .borrow_mut()
            .declare("WeakMap", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("WeakMap", weakmap_ctor);

        self.weakmap_prototype = Some(proto);
    }

    pub(crate) fn setup_weakset_prototype(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "WeakSet".to_string();

        // WeakSet.prototype.add
        let add_fn = self.create_function(JsFunction::native(
            "add".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let has_set = obj.borrow().set_data.is_some();
                    if has_set {
                        let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if !matches!(value, JsValue::Object(_)) {
                            let err = interp.create_type_error("Invalid value used in weak set");
                            return Completion::Throw(err);
                        }
                        let mut borrowed = obj.borrow_mut();
                        let entries = borrowed.set_data.as_mut().unwrap();
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
            },
        ));
        proto.borrow_mut().insert_builtin("add".to_string(), add_fn);

        // WeakSet.prototype.has
        let has_fn = self.create_function(JsFunction::native(
            "has".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let set_data = obj.borrow().set_data.clone();
                    if let Some(entries) = set_data {
                        let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if !matches!(value, JsValue::Object(_)) {
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
            },
        ));
        proto.borrow_mut().insert_builtin("has".to_string(), has_fn);

        // WeakSet.prototype.delete
        let delete_fn = self.create_function(JsFunction::native(
            "delete".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let has_set = obj.borrow().set_data.is_some();
                    if has_set {
                        let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if !matches!(value, JsValue::Object(_)) {
                            return Completion::Normal(JsValue::Boolean(false));
                        }
                        let mut borrowed = obj.borrow_mut();
                        let entries = borrowed.set_data.as_mut().unwrap();
                        for entry in entries.iter_mut() {
                            let matches =
                                entry.as_ref().is_some_and(|e| strict_equality(e, &value));
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
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("delete".to_string(), delete_fn);

        // @@toStringTag
        proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str("WeakSet")),
                false,
                false,
                true,
            ),
        );

        let proto_id = proto.borrow().id.unwrap();
        let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });

        // WeakSet constructor
        let weakset_proto_clone = proto.clone();
        let weakset_ctor = self.create_function(JsFunction::constructor(
            "WeakSet".to_string(),
            0,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    let err = interp.create_type_error("Constructor WeakSet requires 'new'");
                    return Completion::Throw(err);
                }

                let obj = interp.create_object();
                obj.borrow_mut().prototype = Some(weakset_proto_clone.clone());
                obj.borrow_mut().class_name = "WeakSet".to_string();
                obj.borrow_mut().set_data = Some(Vec::new());
                let obj_id = obj.borrow().id.unwrap();
                let this_val = JsValue::Object(crate::types::JsObject { id: obj_id });

                let iterable = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !iterable.is_undefined() && !iterable.is_null() {
                    let adder = obj.borrow().get_property("add");
                    if !matches!(&adder, JsValue::Object(ao) if interp.get_object(ao.id).is_some_and(|o| o.borrow().callable.is_some())) {
                        let err = interp.create_type_error("WeakSet.prototype.add is not a function");
                        return Completion::Throw(err);
                    }

                    let iter_key = interp.get_symbol_iterator_key();
                    let iterator_fn = if let Some(ref key) = iter_key {
                        if let JsValue::Object(io) = &iterable {
                            if let Some(iter_obj) = interp.get_object(io.id) {
                                let v = iter_obj.borrow().get_property(key);
                                if v.is_undefined() { JsValue::Undefined } else { v }
                            } else { JsValue::Undefined }
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
                            if let Some(iter_obj) = interp.get_object(io.id) {
                                iter_obj.borrow().get_property("next")
                            } else { JsValue::Undefined }
                        } else { JsValue::Undefined };

                        let next_result = match interp.call_function(&next_fn, &iterator, &[]) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };

                        let (done, value) = extract_iter_result(interp, &next_result);
                        if done { break; }

                        match interp.call_function(&adder, &this_val, &[value]) {
                            Completion::Normal(_) => {}
                            other => return other,
                        }
                    }
                }

                Completion::Normal(this_val)
            },
        ));

        if let JsValue::Object(ctor_obj) = &weakset_ctor
            && let Some(obj) = self.get_object(ctor_obj.id)
        {
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val.clone(), false, false, false),
            );
        }
        proto.borrow_mut().insert_property(
            "constructor".to_string(),
            PropertyDescriptor::data(weakset_ctor.clone(), true, false, true),
        );

        self.global_env
            .borrow_mut()
            .declare("WeakSet", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("WeakSet", weakset_ctor);

        self.weakset_prototype = Some(proto);
    }

    pub(crate) fn setup_weakref(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "WeakRef".to_string();

        // WeakRef.prototype.deref
        let deref_fn = self.create_function(JsFunction::native(
            "deref".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this {
                    if let Some(obj) = interp.get_object(o.id) {
                        if obj.borrow().class_name == "WeakRef" {
                            return Completion::Normal(
                                obj.borrow()
                                    .primitive_value
                                    .clone()
                                    .unwrap_or(JsValue::Undefined),
                            );
                        }
                    }
                }
                Completion::Throw(
                    interp.create_type_error("WeakRef.prototype.deref requires a WeakRef"),
                )
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("deref".to_string(), deref_fn);

        // @@toStringTag
        {
            let desc = PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("WeakRef"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            };
            let key = "Symbol(Symbol.toStringTag)".to_string();
            proto.borrow_mut().property_order.push(key.clone());
            proto.borrow_mut().properties.insert(key, desc);
        }

        // WeakRef constructor
        let proto_clone = proto.clone();
        let weakref_ctor = self.create_function(JsFunction::constructor(
            "WeakRef".to_string(),
            1,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    let err = interp.create_type_error("Constructor WeakRef requires 'new'");
                    return Completion::Throw(err);
                }
                let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                match &target {
                    JsValue::Object(_) | JsValue::Symbol(_) => {}
                    _ => {
                        let err =
                            interp.create_type_error("WeakRef requires a target object or symbol");
                        return Completion::Throw(err);
                    }
                }
                let obj = interp.create_object();
                obj.borrow_mut().class_name = "WeakRef".to_string();
                obj.borrow_mut().prototype = Some(proto_clone.clone());
                obj.borrow_mut().primitive_value = Some(target);
                let id = obj.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
            },
        ));

        if let JsValue::Object(ref ctor_obj) = weakref_ctor {
            if let Some(obj) = self.get_object(ctor_obj.id) {
                obj.borrow_mut().insert_property(
                    "prototype".to_string(),
                    PropertyDescriptor::data(
                        JsValue::Object(crate::types::JsObject {
                            id: proto.borrow().id.unwrap(),
                        }),
                        false,
                        false,
                        false,
                    ),
                );
            }
        }

        proto.borrow_mut().insert_property(
            "constructor".to_string(),
            PropertyDescriptor::data(weakref_ctor.clone(), true, false, true),
        );

        self.global_env
            .borrow_mut()
            .declare("WeakRef", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("WeakRef", weakref_ctor);

        self.weakref_prototype = Some(proto);
    }

    pub(crate) fn setup_finalization_registry(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "FinalizationRegistry".to_string();

        // FinalizationRegistry.prototype.register
        let register_fn = self.create_function(JsFunction::native(
            "register".to_string(),
            2,
            |interp, this, args| {
                if let JsValue::Object(o) = this {
                    if let Some(obj) = interp.get_object(o.id) {
                        if obj.borrow().class_name == "FinalizationRegistry" {
                            let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                            match &target {
                                JsValue::Object(_) | JsValue::Symbol(_) => {}
                                _ => {
                                    return Completion::Throw(interp.create_type_error(
                                        "FinalizationRegistry.register requires an object target",
                                    ));
                                }
                            }
                            // We store registrations but GC finalization callbacks are not triggered
                            return Completion::Normal(JsValue::Undefined);
                        }
                    }
                }
                Completion::Throw(interp.create_type_error(
                    "FinalizationRegistry.prototype.register requires a FinalizationRegistry",
                ))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("register".to_string(), register_fn);

        // FinalizationRegistry.prototype.unregister
        let unregister_fn = self.create_function(JsFunction::native(
            "unregister".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this {
                    if let Some(obj) = interp.get_object(o.id) {
                        if obj.borrow().class_name == "FinalizationRegistry" {
                            let token = args.first().cloned().unwrap_or(JsValue::Undefined);
                            match &token {
                                JsValue::Object(_) | JsValue::Symbol(_) => {}
                                _ => {
                                    return Completion::Throw(interp.create_type_error(
                                        "FinalizationRegistry.unregister requires an object token",
                                    ));
                                }
                            }
                            return Completion::Normal(JsValue::Boolean(false));
                        }
                    }
                }
                Completion::Throw(interp.create_type_error(
                    "FinalizationRegistry.prototype.unregister requires a FinalizationRegistry",
                ))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("unregister".to_string(), unregister_fn);

        // @@toStringTag
        {
            let desc = PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("FinalizationRegistry"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            };
            let key = "Symbol(Symbol.toStringTag)".to_string();
            proto.borrow_mut().property_order.push(key.clone());
            proto.borrow_mut().properties.insert(key, desc);
        }

        // FinalizationRegistry constructor
        let proto_clone = proto.clone();
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
                // Check callable
                if let JsValue::Object(ref o) = callback {
                    if let Some(obj) = interp.get_object(o.id) {
                        if obj.borrow().callable.is_none() {
                            return Completion::Throw(interp.create_type_error(
                                "FinalizationRegistry requires a callable cleanup callback",
                            ));
                        }
                    }
                }
                let obj = interp.create_object();
                obj.borrow_mut().class_name = "FinalizationRegistry".to_string();
                obj.borrow_mut().prototype = Some(proto_clone.clone());
                obj.borrow_mut().primitive_value = Some(callback);
                let id = obj.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
            },
        ));

        if let JsValue::Object(ref ctor_obj) = fr_ctor {
            if let Some(obj) = self.get_object(ctor_obj.id) {
                obj.borrow_mut().insert_property(
                    "prototype".to_string(),
                    PropertyDescriptor::data(
                        JsValue::Object(crate::types::JsObject {
                            id: proto.borrow().id.unwrap(),
                        }),
                        false,
                        false,
                        false,
                    ),
                );
            }
        }

        proto.borrow_mut().insert_property(
            "constructor".to_string(),
            PropertyDescriptor::data(fr_ctor.clone(), true, false, true),
        );

        self.global_env
            .borrow_mut()
            .declare("FinalizationRegistry", BindingKind::Var);
        let _ = self
            .global_env
            .borrow_mut()
            .set("FinalizationRegistry", fr_ctor);

        self.finalization_registry_prototype = Some(proto);
    }
}
