use super::super::*;

impl Interpreter {
    pub(super) fn setup_reflect(&mut self) {
        let reflect_obj_id = self.create_object_id();
        let reflect_id = reflect_obj_id;

        self.add_reflect_call_and_define_methods(reflect_obj_id);
        self.add_reflect_query_methods(reflect_obj_id);
        self.add_reflect_mutation_methods(reflect_obj_id);

        // @@toStringTag
        self.define_to_string_tag(reflect_obj_id, "Reflect");

        // Register Reflect as global
        let reflect_val = JsValue::object(reflect_id);
        self.realm()
            .global_env
            .borrow_mut()
            .declare("Reflect", BindingKind::Const);
        self.realm()
            .global_env
            .borrow_mut()
            .initialize_binding("Reflect", reflect_val);
    }

    fn add_reflect_call_and_define_methods(&mut self, reflect_obj_id: u64) {
        // Reflect.apply(target, thisArg, argsList)
        let apply_fn = self.create_function(JsFunction::native(
            "apply".to_string(),
            3,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if !interp.is_callable(&target) {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.apply requires a function target"),
                    );
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                let args_list = args.get(2).cloned().unwrap_or(JsValue::UNDEFINED);
                // CreateListFromArrayLike
                if !(args_list).is_object() {
                    return Completion::Throw(
                        interp.create_type_error("CreateListFromArrayLike called on non-object"),
                    );
                }
                let call_args = match interp.create_list_from_array_like(&args_list) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                interp.call_function(&target, &this_arg, &call_args)
            },
        ));
        self.get_object_cell_expect(reflect_obj_id)
            .borrow_mut()
            .insert_builtin("apply".to_string(), apply_fn);

        // Reflect.construct(target, argsList, newTarget?)
        let construct_fn = self.create_function(JsFunction::native(
            "construct".to_string(),
            2,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if !interp.is_constructor(&target) {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.construct requires a constructor"),
                    );
                }
                let args_list = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                let new_target = args.get(2).cloned().unwrap_or(target.clone());
                if !interp.is_constructor(&new_target) {
                    return Completion::Throw(
                        interp.create_type_error("newTarget is not a constructor"),
                    );
                }
                // CreateListFromArrayLike
                if !(args_list).is_object() {
                    return Completion::Throw(
                        interp.create_type_error("CreateListFromArrayLike called on non-object"),
                    );
                }
                let call_args = match interp.create_list_from_array_like(&args_list) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                interp.construct_with_new_target(&target, &call_args, new_target)
            },
        ));
        self.get_object_cell_expect(reflect_obj_id)
            .borrow_mut()
            .insert_builtin("construct".to_string(), construct_fn);

        // Reflect.defineProperty(target, key, desc)
        let def_prop_fn = self.create_function(JsFunction::native(
            "defineProperty".to_string(),
            3,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if !(target).is_object() {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.defineProperty requires an object"),
                    );
                }
                let key_raw = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                let key = match interp.to_property_key(&key_raw) {
                    Ok(k) => k,
                    Err(e) => return Completion::Throw(e),
                };
                let desc_val = args.get(2).cloned().unwrap_or(JsValue::UNDEFINED);
                if let Some(target_id) = target.as_object_id()
                    && let Some(obj) = interp.get_object(target_id)
                {
                    // Deferred namespace: trigger evaluation on [[DefineOwnProperty]]
                    {
                        let is_deferred_ns = obj
                            .borrow()
                            .module_namespace()
                            .as_ref()
                            .is_some_and(|ns| ns.deferred);
                        if is_deferred_ns
                            && !Interpreter::is_symbol_like_namespace_key(&key, true)
                            && let Err(e) = interp.ensure_deferred_namespace_evaluation(target_id)
                        {
                            return Completion::Throw(e);
                        }
                    }
                    let obj = interp.get_object(target_id).unwrap();
                    let res = {
                        let _b = obj.borrow();
                        _b.is_proxy() || _b.is_proxy_revoked()
                    };
                    if res {
                        // Re-parse descriptor so proxy trap gets a fresh copy with coerced booleans
                        let reparsed_desc = match interp.to_property_descriptor(&desc_val) {
                            Ok(pd) => interp.from_property_descriptor(&pd),
                            Err(Some(e)) => return Completion::Throw(e),
                            Err(None) => desc_val.clone(),
                        };
                        match interp.proxy_define_own_property(target_id, key, &reparsed_desc) {
                            Ok(result) => return Completion::Normal(JsValue::boolean(result)),
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    let is_ta = obj.borrow().typed_array_info().is_some();
                    match interp.to_property_descriptor(&desc_val) {
                        Ok(desc) => {
                            if is_ta {
                                match interp.typed_array_define_own_property(target_id, &key, &desc)
                                {
                                    Ok(Some(result)) => {
                                        return Completion::Normal(JsValue::boolean(result));
                                    }
                                    Ok(None) => {
                                        let result =
                                            obj.borrow_mut().define_own_property(key, desc);
                                        return Completion::Normal(JsValue::boolean(result));
                                    }
                                    Err(e) => return Completion::Throw(e),
                                }
                            } else {
                                // Array [[DefineOwnProperty]] §10.4.2.1
                                let is_array = obj.borrow().class_name == "Array";
                                if is_array {
                                    match interp.array_define_own_property(
                                        target_id as usize,
                                        &key,
                                        desc,
                                    ) {
                                        Ok(result) => {
                                            return Completion::Normal(JsValue::boolean(result));
                                        }
                                        Err(e) => return Completion::Throw(e),
                                    }
                                }
                                let result = obj.borrow_mut().define_own_property(key, desc);
                                return Completion::Normal(JsValue::boolean(result));
                            }
                        }
                        Err(Some(e)) => return Completion::Throw(e),
                        Err(None) => {}
                    }
                }
                Completion::Normal(JsValue::boolean(false))
            },
        ));
        self.get_object_cell_expect(reflect_obj_id)
            .borrow_mut()
            .insert_builtin("defineProperty".to_string(), def_prop_fn);

        // Reflect.deleteProperty(target, key)
        let del_prop_fn = self.create_function(JsFunction::native(
            "deleteProperty".to_string(),
            2,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if !(target).is_object() {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.deleteProperty requires an object"),
                    );
                }
                let key_raw = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                let key = match interp.to_property_key(&key_raw) {
                    Ok(k) => k,
                    Err(e) => return Completion::Throw(e),
                };
                if let Some(target_id) = target.as_object_id()
                    && let Some(obj) = interp.get_object_cell(target_id)
                {
                    let res = {
                        let _b = obj.borrow();
                        _b.is_proxy() || _b.is_proxy_revoked()
                    };
                    if res {
                        match interp.proxy_delete_property(target_id, &key) {
                            Ok(result) => return Completion::Normal(JsValue::boolean(result)),
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    // Module namespace exotic: [[Delete]] — only for string keys (not symbols)
                    if !key.is_symbol() {
                        let is_ns = obj.borrow().module_namespace().is_some();
                        if is_ns {
                            let export_names = obj
                                .borrow()
                                .module_namespace()
                                .as_ref()
                                .unwrap()
                                .export_names
                                .clone();
                            if key
                                .as_str()
                                .is_some_and(|key| export_names.iter().any(|name| name == key))
                            {
                                return Completion::Normal(JsValue::boolean(false));
                            }
                            return Completion::Normal(JsValue::boolean(true));
                        }
                    }
                    // String exotic [[Delete]] (§10.4.3): "length" and own
                    // character indices are non-configurable.
                    {
                        let b = obj.borrow();
                        if b.class_name == "String"
                            && let Some(s) = b.primitive_value.as_ref().and_then(JsValue::as_string)
                            && (key.as_str() == Some("length")
                                || crate::interpreter::types::string_exotic_index(
                                    &key,
                                    s.code_units.len(),
                                )
                                .is_some())
                        {
                            return Completion::Normal(JsValue::boolean(false));
                        }
                    }
                    let mut obj_mut = obj.borrow_mut();
                    if let Some(desc) = obj_mut.properties.get(&key)
                        && desc.configurable == Some(false)
                    {
                        return Completion::Normal(JsValue::boolean(false));
                    }
                    obj_mut.remove_property(&key);
                    if let Some(key_str) = key.as_str()
                        && let Some(map) = obj_mut.parameter_map_mut()
                    {
                        map.remove(key_str);
                    }
                    if let Ok(idx) = key.parse::<usize>()
                        && let Some(elems) = obj_mut.array_elements_mut()
                        && idx < elems.len()
                    {
                        elems[idx] = JsValue::UNDEFINED;
                    }
                    return Completion::Normal(JsValue::boolean(true));
                }
                Completion::Normal(JsValue::boolean(false))
            },
        ));
        self.get_object_cell_expect(reflect_obj_id)
            .borrow_mut()
            .insert_builtin("deleteProperty".to_string(), del_prop_fn);
    }

    fn add_reflect_query_methods(&mut self, reflect_obj_id: u64) {
        // Reflect.get(target, key, receiver?)
        let get_fn = self.create_function(JsFunction::native(
            "get".to_string(),
            2,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if !(target).is_object() {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.get requires an object"),
                    );
                }
                let key_raw = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                let key = match interp.to_property_key(&key_raw) {
                    Ok(k) => k,
                    Err(e) => return Completion::Throw(e),
                };
                let receiver = args.get(2).cloned().unwrap_or(target.clone());
                if let Some(target_id) = target.as_object_id() {
                    interp.get_object_property(target_id, &key, &receiver)
                } else {
                    Completion::Normal(JsValue::UNDEFINED)
                }
            },
        ));
        self.get_object_cell_expect(reflect_obj_id)
            .borrow_mut()
            .insert_builtin("get".to_string(), get_fn);

        // Reflect.getOwnPropertyDescriptor(target, key)
        let gopd_fn = self.create_function(JsFunction::native(
            "getOwnPropertyDescriptor".to_string(),
            2,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if !(target).is_object() {
                    return Completion::Throw(
                        interp.create_type_error(
                            "Reflect.getOwnPropertyDescriptor requires an object",
                        ),
                    );
                }
                let key_raw = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                let key = match interp.to_property_key(&key_raw) {
                    Ok(k) => k,
                    Err(e) => return Completion::Throw(e),
                };
                if let Some(target_id) = target.as_object_id() {
                    // Deferred namespace: trigger evaluation
                    {
                        let deferred_ns = interp.get_object_cell(target_id).and_then(|obj| {
                            let b = obj.borrow();
                            b.module_namespace().map(|ns| ns.deferred)
                        });
                        if deferred_ns == Some(true)
                            && !Interpreter::is_symbol_like_namespace_key(&key, true)
                            && let Err(e) = interp.ensure_deferred_namespace_evaluation(target_id)
                        {
                            return Completion::Throw(e);
                        }
                    }
                    if let Some(obj) = interp.get_object_cell(target_id)
                        && {
                            let _b = obj.borrow();
                            _b.is_proxy() || _b.is_proxy_revoked()
                        }
                    {
                        match interp.proxy_get_own_property_descriptor(target_id, &key) {
                            Ok(v) => return Completion::Normal(v),
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    // Module namespace [[GetOwnProperty]] (§10.4.6.4): live binding
                    let is_ns = interp
                        .get_object_cell(target_id)
                        .map(|obj| obj.borrow().module_namespace().is_some())
                        .unwrap_or(false);
                    if is_ns {
                        let is_export = interp
                            .get_object_cell(target_id)
                            .and_then(|obj| {
                                let b = obj.borrow();
                                let ns = b.module_namespace()?;
                                Some(key.as_str().is_some_and(|key| {
                                    ns.export_names.iter().any(|name| name == key)
                                }))
                            })
                            .unwrap_or(false);
                        if is_export {
                            let live_val = match interp.get_object_property(
                                target_id,
                                &key,
                                &target.clone(),
                            ) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                other => return other,
                            };
                            let desc = crate::interpreter::types::PropertyDescriptor {
                                value: Some(live_val),
                                writable: Some(true),
                                enumerable: Some(true),
                                configurable: Some(false),
                                get: None,
                                set: None,
                            };
                            return Completion::Normal(interp.from_property_descriptor(&desc));
                        }
                        if let Some(obj) = interp.get_object(target_id)
                            && let Some(desc) = obj.borrow().get_own_property(&key)
                        {
                            return Completion::Normal(interp.from_property_descriptor(&desc));
                        }
                        return Completion::Normal(JsValue::UNDEFINED);
                    }
                    if let Some(obj) = interp.get_object(target_id)
                        && let Some(desc) = obj.borrow().get_own_property(&key)
                    {
                        return Completion::Normal(interp.from_property_descriptor(&desc));
                    }
                }
                Completion::Normal(JsValue::UNDEFINED)
            },
        ));
        self.get_object_cell_expect(reflect_obj_id)
            .borrow_mut()
            .insert_builtin("getOwnPropertyDescriptor".to_string(), gopd_fn);

        // Reflect.getPrototypeOf(target)
        let gpo_fn = self.create_function(JsFunction::native(
            "getPrototypeOf".to_string(),
            1,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if !(target).is_object() {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.getPrototypeOf requires an object"),
                    );
                }
                if let Some(target_id) = target.as_object_id()
                    && let Some(obj) = interp.get_object_cell(target_id)
                {
                    let res = {
                        let _b = obj.borrow();
                        _b.is_proxy() || _b.is_proxy_revoked()
                    };
                    if res {
                        match interp.proxy_get_prototype_of(target_id) {
                            Ok(v) => return Completion::Normal(v),
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    if let Some(id) = obj.borrow().prototype_id {
                        return Completion::Normal(JsValue::object(id));
                    }
                }
                Completion::Normal(JsValue::NULL)
            },
        ));
        self.get_object_cell_expect(reflect_obj_id)
            .borrow_mut()
            .insert_builtin("getPrototypeOf".to_string(), gpo_fn);

        // Reflect.has(target, key)
        let has_fn = self.create_function(JsFunction::native(
            "has".to_string(),
            2,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if !(target).is_object() {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.has requires an object"),
                    );
                }
                let key_raw = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                let key = match interp.to_property_key(&key_raw) {
                    Ok(k) => k,
                    Err(e) => return Completion::Throw(e),
                };
                if let Some(target_id) = target.as_object_id() {
                    match interp.proxy_has_property(target_id, &key) {
                        Ok(result) => return Completion::Normal(JsValue::boolean(result)),
                        Err(e) => return Completion::Throw(e),
                    }
                }
                Completion::Normal(JsValue::boolean(false))
            },
        ));
        self.get_object_cell_expect(reflect_obj_id)
            .borrow_mut()
            .insert_builtin("has".to_string(), has_fn);

        // Reflect.isExtensible(target)
        let is_ext_fn = self.create_function(JsFunction::native(
            "isExtensible".to_string(),
            1,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if !(target).is_object() {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.isExtensible requires an object"),
                    );
                }
                if let Some(target_id) = target.as_object_id()
                    && let Some(obj) = interp.get_object_cell(target_id)
                {
                    let res = {
                        let _b = obj.borrow();
                        _b.is_proxy() || _b.is_proxy_revoked()
                    };
                    if res {
                        match interp.proxy_is_extensible(target_id) {
                            Ok(result) => return Completion::Normal(JsValue::boolean(result)),
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    return Completion::Normal(JsValue::boolean(obj.borrow().extensible));
                }
                Completion::Normal(JsValue::boolean(false))
            },
        ));
        self.get_object_cell_expect(reflect_obj_id)
            .borrow_mut()
            .insert_builtin("isExtensible".to_string(), is_ext_fn);

        // Reflect.ownKeys(target)
        let own_keys_fn = self.create_function(JsFunction::native(
            "ownKeys".to_string(),
            1,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if !(target).is_object() {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.ownKeys requires an object"),
                    );
                }
                let target_id = target.as_object_id().unwrap();
                let keys = match interp.proxy_own_keys(target_id) {
                    Ok(keys) => keys,
                    Err(e) => return Completion::Throw(e),
                };
                Completion::Normal(interp.create_array(keys))
            },
        ));
        self.get_object_cell_expect(reflect_obj_id)
            .borrow_mut()
            .insert_builtin("ownKeys".to_string(), own_keys_fn);
    }

    fn add_reflect_mutation_methods(&mut self, reflect_obj_id: u64) {
        // Reflect.preventExtensions(target)
        let pe_fn = self.create_function(JsFunction::native(
            "preventExtensions".to_string(),
            1,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if !(target).is_object() {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.preventExtensions requires an object"),
                    );
                }
                if let Some(target_id) = target.as_object_id()
                    && let Some(obj) = interp.get_object_cell(target_id)
                {
                    let res = {
                        let _b = obj.borrow();
                        _b.is_proxy() || _b.is_proxy_revoked()
                    };
                    if res {
                        match interp.proxy_prevent_extensions(target_id) {
                            Ok(result) => return Completion::Normal(JsValue::boolean(result)),
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    // TypedArray [[PreventExtensions]] — §10.4.5.2
                    {
                        let b = obj.borrow();
                        if let Some(ta) = b.typed_array_info() {
                            let is_fixed = b
                                .view_buffer_object_id()
                                .and_then(|buf_id| interp.get_object_cell(buf_id))
                                .map(|buf| {
                                    use crate::interpreter::types::is_typed_array_fixed_length;
                                    is_typed_array_fixed_length(ta, &buf.borrow())
                                })
                                .unwrap_or(true);
                            if !is_fixed {
                                return Completion::Normal(JsValue::boolean(false));
                            }
                        }
                    }
                    obj.borrow_mut().extensible = false;
                }
                Completion::Normal(JsValue::boolean(true))
            },
        ));
        self.get_object_cell_expect(reflect_obj_id)
            .borrow_mut()
            .insert_builtin("preventExtensions".to_string(), pe_fn);

        // Reflect.set(target, key, value, receiver?)
        let set_fn = self.create_function(JsFunction::native(
            "set".to_string(),
            3,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if !(target).is_object() {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.set requires an object"),
                    );
                }
                let key_raw = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                let key = match interp.to_property_key(&key_raw) {
                    Ok(k) => k,
                    Err(e) => return Completion::Throw(e),
                };
                let value = args.get(2).cloned().unwrap_or(JsValue::UNDEFINED);
                let receiver = args.get(3).cloned().unwrap_or(target.clone());
                // Check if target is a proxy
                if let Some(target_id) = target.as_object_id()
                    && let Some(obj) = interp.get_object_cell(target_id)
                    && {
                        let _b = obj.borrow();
                        _b.is_proxy() || _b.is_proxy_revoked()
                    }
                {
                    match interp.proxy_set(target_id, &key, value.clone(), &receiver) {
                        Ok(result) => return Completion::Normal(JsValue::boolean(result)),
                        Err(e) => return Completion::Throw(e),
                    }
                }
                // Module namespace exotic: [[Set]] always returns false
                if let Some(target_id) = target.as_object_id()
                    && let Some(obj) = interp.get_object_cell(target_id)
                    && obj.borrow().module_namespace().is_some()
                {
                    return Completion::Normal(JsValue::boolean(false));
                }
                // TypedArray [[Set]] exotic — §10.4.5.5
                if let Some(target_id) = target.as_object_id() {
                    let ta_info_opt = interp
                        .get_object_cell(target_id)
                        .and_then(|obj| obj.borrow().typed_array_info().cloned());
                    if let Some(ta_info) = ta_info_opt
                        && let Some(index) = canonical_numeric_index_string(&key)
                    {
                        let same = receiver.as_object_id() == Some(target_id);
                        if same {
                            let is_bigint = ta_info.kind.is_bigint();
                            let num_val = if is_bigint {
                                match interp.to_bigint_value(&value) {
                                    Ok(v) => v,
                                    Err(e) => return Completion::Throw(e),
                                }
                            } else {
                                JsValue::number(match interp.to_number_value(&value) {
                                    Ok(n) => n,
                                    Err(e) => return Completion::Throw(e),
                                })
                            };
                            if is_valid_integer_index(&ta_info, index) {
                                typed_array_set_index(&ta_info, index as usize, &num_val);
                            }
                            return Completion::Normal(JsValue::boolean(true));
                        }
                        // Not same: if index is out of bounds in target, silently succeed
                        if !is_valid_integer_index(&ta_info, index) {
                            return Completion::Normal(JsValue::boolean(true));
                        }
                        // Index is in bounds in target: OrdinarySet to receiver
                        if let Some(receiver_id) = receiver.as_object_id() {
                            let recv_ta_opt = interp
                                .get_object_cell(receiver_id)
                                .and_then(|obj| obj.borrow().typed_array_info().cloned());
                            if let Some(recv_ta) = recv_ta_opt {
                                // Receiver is TypedArray: IntegerIndexedElementSet
                                if !is_valid_integer_index(&recv_ta, index) {
                                    return Completion::Normal(JsValue::boolean(false));
                                }
                                let is_bigint = recv_ta.kind.is_bigint();
                                let num_val = if is_bigint {
                                    match interp.to_bigint_value(&value) {
                                        Ok(v) => v,
                                        Err(e) => return Completion::Throw(e),
                                    }
                                } else {
                                    JsValue::number(match interp.to_number_value(&value) {
                                        Ok(n) => n,
                                        Err(e) => return Completion::Throw(e),
                                    })
                                };
                                typed_array_set_index(&recv_ta, index as usize, &num_val);
                                return Completion::Normal(JsValue::boolean(true));
                            } else if let Some(recv_obj) = interp.get_object_cell(receiver_id) {
                                // Non-TypedArray receiver: create plain property, no coercion
                                let existing = recv_obj.borrow().get_own_property(&key);
                                if let Some(ref ed) = existing {
                                    if ed.get.is_some() || ed.set.is_some() {
                                        return Completion::Normal(JsValue::boolean(false));
                                    }
                                    if ed.writable == Some(false) {
                                        return Completion::Normal(JsValue::boolean(false));
                                    }
                                    let update_desc = PropertyDescriptor {
                                        value: Some(value),
                                        writable: None,
                                        enumerable: None,
                                        configurable: None,
                                        get: None,
                                        set: None,
                                    };
                                    recv_obj.borrow_mut().define_own_property(key, update_desc);
                                } else {
                                    let success =
                                        recv_obj.borrow_mut().set_property_value(&key, value);
                                    return Completion::Normal(JsValue::boolean(success));
                                }
                                return Completion::Normal(JsValue::boolean(true));
                            }
                        }
                        return Completion::Normal(JsValue::boolean(false));
                    }
                }
                // OrdinarySet: find ownDesc on target, walking prototype chain
                let mut own_desc: Option<PropertyDescriptor> = None;
                if let Some(target_id) = target.as_object_id() {
                    let mut cur_id = Some(target_id);
                    'proto_walk: while let Some(cid) = cur_id {
                        // If cur_obj is a Proxy, delegate to proxy_set
                        if interp.get_proxy_info(cid).is_some() {
                            match interp.proxy_set(cid, &key, value.clone(), &receiver) {
                                Ok(success) => {
                                    return Completion::Normal(JsValue::boolean(success));
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        if let Some(cur_obj) = interp.get_object_cell(cid) {
                            // TypedArray [[Set]] §10.4.5.5 via prototype chain
                            {
                                let borrow = cur_obj.borrow();
                                if let Some(ta) = borrow.typed_array_info()
                                    && let Some(index) = canonical_numeric_index_string(&key)
                                {
                                    let same = receiver.as_object_id() == Some(cid);
                                    if same {
                                        let is_bigint = ta.kind.is_bigint();
                                        let ta_clone = ta.clone();
                                        drop(borrow);
                                        let num_val = if is_bigint {
                                            match interp.to_bigint_value(&value) {
                                                Ok(v) => v,
                                                Err(e) => return Completion::Throw(e),
                                            }
                                        } else {
                                            JsValue::number(match interp.to_number_value(&value) {
                                                Ok(n) => n,
                                                Err(e) => return Completion::Throw(e),
                                            })
                                        };
                                        if is_valid_integer_index(&ta_clone, index) {
                                            typed_array_set_index(
                                                &ta_clone,
                                                index as usize,
                                                &num_val,
                                            );
                                        }
                                        return Completion::Normal(JsValue::boolean(true));
                                    } else if !is_valid_integer_index(ta, index) {
                                        return Completion::Normal(JsValue::boolean(true));
                                    }
                                    // Valid index, not same: fall through to get_own_property
                                }
                            }
                            if let Some(d) = cur_obj.borrow().get_own_property(&key) {
                                own_desc = Some(d);
                                break 'proto_walk;
                            }
                            cur_id = cur_obj.borrow().prototype_id.as_ref().copied();
                        } else {
                            break;
                        }
                    }
                }
                // If no own desc found, treat as default data descriptor
                let own_desc = own_desc.unwrap_or(PropertyDescriptor {
                    value: Some(JsValue::UNDEFINED),
                    writable: Some(true),
                    enumerable: Some(true),
                    configurable: Some(true),
                    get: None,
                    set: None,
                });
                // If accessor descriptor
                if own_desc.get.is_some() || own_desc.set.is_some() {
                    if let Some(ref setter) = own_desc.set
                        && !(setter).is_undefined()
                    {
                        let setter = setter.clone();
                        return match interp.call_function(&setter, &receiver, &[value]) {
                            Completion::Normal(_) => Completion::Normal(JsValue::boolean(true)),
                            Completion::Throw(e) => Completion::Throw(e),
                            _ => Completion::Normal(JsValue::boolean(true)),
                        };
                    }
                    return Completion::Normal(JsValue::boolean(false));
                }
                // Data descriptor
                if own_desc.writable == Some(false) {
                    return Completion::Normal(JsValue::boolean(false));
                }
                if !(receiver).is_object() {
                    return Completion::Normal(JsValue::boolean(false));
                }
                if let Some(receiver_id) = receiver.as_object_id() {
                    let is_proxy_recv = interp.get_proxy_info(receiver_id).is_some();
                    if is_proxy_recv {
                        // §10.1.9.2: Receiver.[[GetOwnProperty]](P)
                        let existing_val =
                            match interp.proxy_get_own_property_descriptor(receiver_id, &key) {
                                Ok(v) => v,
                                Err(e) => return Completion::Throw(e),
                            };
                        if (existing_val).is_undefined() {
                            // CreateDataProperty(Receiver, P, V)
                            let create_desc = PropertyDescriptor {
                                value: Some(value),
                                writable: Some(true),
                                enumerable: Some(true),
                                configurable: Some(true),
                                get: None,
                                set: None,
                            };
                            let desc_val = interp.from_property_descriptor(&create_desc);
                            match interp.proxy_define_own_property(receiver_id, key, &desc_val) {
                                Ok(success) => {
                                    return Completion::Normal(JsValue::boolean(success));
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        } else {
                            let existing_desc = match interp.to_property_descriptor(&existing_val) {
                                Ok(d) => d,
                                Err(Some(e)) => return Completion::Throw(e),
                                Err(None) => return Completion::Normal(JsValue::boolean(false)),
                            };
                            if existing_desc.is_accessor_descriptor() {
                                return Completion::Normal(JsValue::boolean(false));
                            }
                            if existing_desc.writable == Some(false) {
                                return Completion::Normal(JsValue::boolean(false));
                            }
                            // Receiver.[[DefineOwnProperty]](P, {[[Value]]: V})
                            let val_desc = PropertyDescriptor {
                                value: Some(value),
                                writable: None,
                                enumerable: None,
                                configurable: None,
                                get: None,
                                set: None,
                            };
                            let desc_val = interp.from_property_descriptor(&val_desc);
                            match interp.proxy_define_own_property(receiver_id, key, &desc_val) {
                                Ok(success) => {
                                    return Completion::Normal(JsValue::boolean(success));
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                    }
                    if let Some(recv_obj) = interp.get_object_cell(receiver_id) {
                        let existing = recv_obj.borrow().get_own_property(&key);
                        if let Some(ref ed) = existing {
                            if ed.get.is_some() || ed.set.is_some() {
                                return Completion::Normal(JsValue::boolean(false));
                            }
                            if ed.writable == Some(false) {
                                return Completion::Normal(JsValue::boolean(false));
                            }
                            let update_desc = PropertyDescriptor {
                                value: Some(value),
                                writable: None,
                                enumerable: None,
                                configurable: None,
                                get: None,
                                set: None,
                            };
                            // Array [[DefineOwnProperty]] §10.4.2.1
                            let is_array = recv_obj.borrow().class_name == "Array";
                            if is_array {
                                match interp.array_define_own_property(
                                    receiver_id as usize,
                                    &key,
                                    update_desc,
                                ) {
                                    Ok(success) => {
                                        return Completion::Normal(JsValue::boolean(success));
                                    }
                                    Err(e) => return Completion::Throw(e),
                                }
                            }
                            recv_obj.borrow_mut().define_own_property(key, update_desc);
                        } else {
                            recv_obj.borrow_mut().set_property_value(&key, value);
                        }
                    }
                    return Completion::Normal(JsValue::boolean(true));
                }
                Completion::Normal(JsValue::boolean(false))
            },
        ));
        self.get_object_cell_expect(reflect_obj_id)
            .borrow_mut()
            .insert_builtin("set".to_string(), set_fn);

        // Reflect.setPrototypeOf(target, proto)
        let spo_fn = self.create_function(JsFunction::native(
            "setPrototypeOf".to_string(),
            2,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if !(target).is_object() {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.setPrototypeOf requires an object"),
                    );
                }
                let proto = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                // Step 2: If Type(proto) is not Object and proto is not null, throw TypeError
                if !proto.is_object() && !proto.is_null() {
                    return Completion::Throw(interp.create_type_error(
                        "Reflect.setPrototypeOf: proto must be Object or null",
                    ));
                }
                if let Some(target_id) = target.as_object_id()
                    && let Some(obj) = interp.get_object_cell(target_id)
                {
                    let res = {
                        let _b = obj.borrow();
                        _b.is_proxy() || _b.is_proxy_revoked()
                    };
                    if res {
                        match interp.proxy_set_prototype_of(target_id, &proto) {
                            Ok(result) => return Completion::Normal(JsValue::boolean(result)),
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    // OrdinarySetPrototypeOf (§10.1.2)
                    let current_proto_id = obj.borrow().prototype_id;
                    let new_proto_id = proto.as_object_id();
                    // Step 4: If SameValue(V, current), return true
                    if (proto).is_null() && current_proto_id.is_none() {
                        return Completion::Normal(JsValue::boolean(true));
                    }
                    if let (Some(new_id), Some(cur_id)) = (new_proto_id, current_proto_id)
                        && new_id == cur_id
                    {
                        return Completion::Normal(JsValue::boolean(true));
                    }
                    // Immutable prototype check
                    if obj.borrow().is_immutable_prototype {
                        return Completion::Normal(JsValue::boolean(false));
                    }
                    // Step 5: If not extensible, return false
                    if !obj.borrow().extensible {
                        return Completion::Normal(JsValue::boolean(false));
                    }
                    // Steps 6-8: Check for circular prototype chain
                    if let Some(new_pid) = new_proto_id {
                        let mut p_id = Some(new_pid);
                        while let Some(pid) = p_id {
                            if pid == target_id {
                                return Completion::Normal(JsValue::boolean(false));
                            }
                            if let Some(p_obj) = interp.get_object_cell(pid) {
                                // If p is a Proxy, stop the loop (done = true per spec step 8.c.i)
                                if p_obj.borrow().is_proxy() {
                                    break;
                                }
                                p_id = p_obj.borrow().prototype_id.as_ref().copied();
                            } else {
                                break;
                            }
                        }
                    }
                    // Actually set the prototype
                    if proto.is_null() {
                        obj.borrow_mut().prototype_id = None;
                    } else if let Some(proto_id) = proto.as_object_id() {
                        if let Some(proto_obj) = interp.get_object_cell(proto_id) {
                            obj.borrow_mut().prototype_id = Some(proto_obj.borrow().id.unwrap());
                        }
                    } else {
                        unreachable!()
                    }
                    return Completion::Normal(JsValue::boolean(true));
                }
                Completion::Normal(JsValue::boolean(false))
            },
        ));
        self.get_object_cell_expect(reflect_obj_id)
            .borrow_mut()
            .insert_builtin("setPrototypeOf".to_string(), spo_fn);
    }
}
