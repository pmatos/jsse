use super::super::*;

impl Interpreter {
    pub(super) fn setup_object_statics(&mut self) {
        // Get the Object function from global env
        let obj_func_val = self
            .realm()
            .global_env
            .borrow()
            .get("Object")
            .unwrap_or(JsValue::UNDEFINED);
        if let Some(object_id) = obj_func_val.as_object_id()
            && let Some(obj_func) = self.get_object(object_id)
        {
            // Get prototype property
            let proto_val = obj_func.borrow().get_property_value("prototype");
            if let Some(proto_id) = proto_val.as_ref().and_then(JsValue::as_object_id)
                && let Some(proto_obj) = self.get_object(proto_id)
            {
                self.realm_mut().object_prototype = Some(proto_obj.borrow().id.unwrap());
                // Object.prototype is an immutable prototype exotic object (§10.4.7)
                proto_obj.borrow_mut().is_immutable_prototype = true;

                // Fix Error.prototype chain - created before object_prototype was available
                for name in [
                    "Error",
                    "SyntaxError",
                    "TypeError",
                    "ReferenceError",
                    "RangeError",
                    "URIError",
                    "EvalError",
                    "Test262Error",
                ] {
                    if let Some(error_val) = self.get_global_var(name)
                        && let Some(error_id) = error_val.as_object_id()
                    {
                        let pv = self.get_property_on_id(error_id, "prototype");
                        if let Some(error_proto_id) = pv.as_object_id()
                            && let Some(ep) = self.get_object(error_proto_id)
                            && ep.borrow().prototype_id.is_none()
                        {
                            ep.borrow_mut().prototype_id = Some(proto_obj.borrow().id.unwrap());
                        }
                    }
                }

                // Add hasOwnProperty to Object.prototype — §20.1.3.2
                let has_own_fn = self.create_function(JsFunction::native(
                    "hasOwnProperty".to_string(),
                    1,
                    |interp, this_val, args| {
                        // Step 1: ToPropertyKey(V) first
                        let key_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        let key = match interp.to_property_key(&key_val) {
                            Ok(k) => k,
                            Err(e) => return Completion::Throw(e),
                        };
                        // Step 2: ToObject(this value)
                        let o = match interp.to_object(this_val) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        if let Some(obj_id) = o.as_object_id() {
                            match interp.proxy_get_own_property_descriptor(obj_id, &key) {
                                Ok(desc_val) => {
                                    return Completion::Normal(JsValue::boolean(
                                        !(desc_val).is_undefined(),
                                    ));
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        Completion::Normal(JsValue::boolean(false))
                    },
                ));
                proto_obj
                    .borrow_mut()
                    .insert_builtin("hasOwnProperty".to_string(), has_own_fn);

                // Object.prototype.toString — §20.1.3.6
                let obj_tostring_fn = self.create_function(JsFunction::native(
                    "toString".to_string(),
                    0,
                    |interp, this_val, _args| {
                        if (this_val).is_undefined() {
                            return Completion::Normal(JsValue::string(JsString::from_str(
                                "[object Undefined]",
                            )));
                        }
                        if (this_val).is_null() {
                            return Completion::Normal(JsValue::string(JsString::from_str(
                                "[object Null]",
                            )));
                        }
                        let o = match interp.to_object(this_val) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        if let Some(obj_id) = o.as_object_id() {
                            // Step 4: IsArray (recursive for proxies)
                            let is_array = match is_array_value(interp, obj_id) {
                                Ok(v) => v,
                                Err(e) => return Completion::Throw(e),
                            };
                            let builtin_tag = if is_array {
                                "Array"
                            } else if let Some(obj) = interp.get_object(obj_id) {
                                let ob = obj.borrow();
                                if ob.class_name == "Arguments" {
                                    "Arguments"
                                } else if ob.callable.is_some() {
                                    "Function"
                                } else if ob.class_name == "Error"
                                    || ob.class_name == "TypeError"
                                    || ob.class_name == "RangeError"
                                    || ob.class_name == "ReferenceError"
                                    || ob.class_name == "SyntaxError"
                                    || ob.class_name == "URIError"
                                    || ob.class_name == "EvalError"
                                {
                                    "Error"
                                } else if ob.class_name == "Boolean" && ob.primitive_value.is_some()
                                {
                                    "Boolean"
                                } else if ob.class_name == "Number" && ob.primitive_value.is_some()
                                {
                                    "Number"
                                } else if ob.class_name == "String" && ob.primitive_value.is_some()
                                {
                                    "String"
                                } else if ob.class_name == "Date" {
                                    "Date"
                                } else if ob.class_name == "RegExp" {
                                    "RegExp"
                                } else {
                                    "Object"
                                }
                            } else {
                                "Object"
                            };
                            // Step 15: Let tag be ? Get(O, @@toStringTag).
                            let tag_key = JsPropertyKey::well_known_symbol("toStringTag");
                            let tag_result = interp.get_object_property(obj_id, &tag_key, &o);
                            let tag = match tag_result {
                                Completion::Normal(value) => value
                                    .as_string()
                                    .map_or_else(|| builtin_tag.to_string(), |s| s.to_string()),
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => builtin_tag.to_string(),
                            };
                            Completion::Normal(JsValue::string(JsString::from_str(&format!(
                                "[object {tag}]"
                            ))))
                        } else {
                            Completion::Normal(JsValue::string(JsString::from_str(
                                "[object Object]",
                            )))
                        }
                    },
                ));
                self.realm_mut().object_prototype_tostring = Some(obj_tostring_fn.clone());
                proto_obj
                    .borrow_mut()
                    .insert_builtin("toString".to_string(), obj_tostring_fn);

                // Object.prototype.valueOf
                let obj_valueof_fn = self.create_function(JsFunction::native(
                    "valueOf".to_string(),
                    0,
                    |interp, this_val, _args| match interp.to_object(this_val) {
                        Completion::Normal(o) => Completion::Normal(o),
                        other => other,
                    },
                ));
                proto_obj
                    .borrow_mut()
                    .insert_builtin("valueOf".to_string(), obj_valueof_fn);

                // Object.prototype.toLocaleString
                let obj_tolocalestring_fn = self.create_function(JsFunction::native(
                    "toLocaleString".to_string(),
                    0,
                    |interp, this_val, _args| {
                        // 1. Let O be the this value.
                        let o = this_val.clone();
                        // 2. Return ? Invoke(O, "toString").
                        // Invoke(V, P): func = GetV(V, P); Return Call(func, V).
                        // GetV(V, P): obj = ToObject(V); Return obj.[[Get]](P, V).
                        let obj = match interp.to_object(&o) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        if let Some(obj_id) = obj.as_object_id() {
                            // [[Get]] with receiver = O (the original value, possibly primitive)
                            let to_string_fn =
                                match interp.get_object_property(obj_id, "toString", &o) {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                };
                            if interp.is_callable(&to_string_fn) {
                                // Call(func, O) — this is the original value
                                return interp.call_function(&to_string_fn, &o, &[]);
                            }
                        }
                        Completion::Throw(interp.create_type_error("toString is not a function"))
                    },
                ));
                proto_obj
                    .borrow_mut()
                    .insert_builtin("toLocaleString".to_string(), obj_tolocalestring_fn);

                // Object.prototype.propertyIsEnumerable — §20.1.3.4
                let pie_fn = self.create_function(JsFunction::native(
                    "propertyIsEnumerable".to_string(),
                    1,
                    |interp, this_val, args| {
                        let key_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        let key = match interp.to_property_key(&key_val) {
                            Ok(k) => k,
                            Err(e) => return Completion::Throw(e),
                        };
                        let o = match interp.to_object(this_val) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        if let Some(obj_id) = o.as_object_id() {
                            // Use proxy-aware [[GetOwnProperty]]
                            match interp.proxy_get_own_property_descriptor(obj_id, &key) {
                                Ok(desc_val) => {
                                    if (desc_val).is_undefined() {
                                        return Completion::Normal(JsValue::boolean(false));
                                    }
                                    // Convert result to PropertyDescriptor to check enumerable
                                    if let Ok(desc) = interp.to_property_descriptor(&desc_val) {
                                        return Completion::Normal(JsValue::boolean(
                                            desc.enumerable != Some(false),
                                        ));
                                    }
                                    return Completion::Normal(JsValue::boolean(false));
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        Completion::Normal(JsValue::boolean(false))
                    },
                ));
                proto_obj
                    .borrow_mut()
                    .insert_builtin("propertyIsEnumerable".to_string(), pie_fn);

                // Object.prototype.isPrototypeOf
                let ipof_fn = self.create_function(JsFunction::native(
                    "isPrototypeOf".to_string(),
                    1,
                    |interp, this_val, args| {
                        let mut v = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        if !(v).is_object() {
                            return Completion::Normal(JsValue::boolean(false));
                        }
                        let o = match interp.to_object(this_val) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        while let Some(v_id) = v.as_object_id() {
                            // Use proxy-aware [[GetPrototypeOf]]
                            let proto = if let Some(obj) = interp.get_object(v_id) {
                                if obj.borrow().is_proxy() || obj.borrow().is_proxy_revoked() {
                                    match interp.proxy_get_prototype_of(v_id) {
                                        Ok(p) => p,
                                        Err(e) => return Completion::Throw(e),
                                    }
                                } else {
                                    match obj.borrow().prototype_id {
                                        Some(p) => {
                                            let pid = p;
                                            JsValue::object(pid)
                                        }
                                        None => JsValue::NULL,
                                    }
                                }
                            } else {
                                break;
                            };
                            if (proto).is_null() {
                                break;
                            }
                            if proto
                                .as_object_id()
                                .zip(o.as_object_id())
                                .is_some_and(|(proto_id, object_id)| proto_id == object_id)
                            {
                                return Completion::Normal(JsValue::boolean(true));
                            }
                            v = proto;
                        }
                        Completion::Normal(JsValue::boolean(false))
                    },
                ));
                proto_obj
                    .borrow_mut()
                    .insert_builtin("isPrototypeOf".to_string(), ipof_fn);

                // Object.prototype.__defineGetter__
                let define_getter_fn = self.create_function(JsFunction::native(
                    "__defineGetter__".to_string(),
                    2,
                    |interp, this_val, args| {
                        // Step 1: ToObject(this) first
                        let o = match interp.to_object(this_val) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let getter = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                        let getter_is_callable = getter
                            .as_object_id()
                            .and_then(|id| interp.get_object_cell(id))
                            .is_some_and(|obj| obj.borrow().callable.is_some());
                        if !getter_is_callable {
                            return Completion::Throw(
                                interp.create_type_error("Getter must be a function"),
                            );
                        }
                        // ToPropertyKey(P)
                        let key_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        let key = match interp.to_property_key(&key_val) {
                            Ok(k) => k,
                            Err(e) => return Completion::Throw(e),
                        };
                        // Step 5: DefinePropertyOrThrow
                        let desc = PropertyDescriptor {
                            value: None,
                            writable: None,
                            get: Some(getter),
                            set: None,
                            enumerable: Some(true),
                            configurable: Some(true),
                        };
                        if let Some(obj_id) = o.as_object_id() {
                            let is_proxy = interp
                                .get_object_cell(obj_id)
                                .map(|obj| {
                                    let b = obj.borrow();
                                    b.is_proxy() || b.is_proxy_revoked()
                                })
                                .unwrap_or(false);
                            if is_proxy {
                                let desc_val = interp.from_property_descriptor(&desc);
                                match interp.proxy_define_own_property(obj_id, key, &desc_val) {
                                    Ok(true) => {}
                                    Ok(false) => {
                                        return Completion::Throw(
                                            interp.create_type_error("Cannot define property"),
                                        );
                                    }
                                    Err(e) => return Completion::Throw(e),
                                }
                            } else if let Some(obj) = interp.get_object(obj_id)
                                && !obj.borrow_mut().define_own_property(key, desc)
                            {
                                return Completion::Throw(
                                    interp.create_type_error("Cannot define property"),
                                );
                            }
                        }
                        Completion::Normal(JsValue::UNDEFINED)
                    },
                ));
                proto_obj
                    .borrow_mut()
                    .insert_builtin("__defineGetter__".to_string(), define_getter_fn);

                // Object.prototype.__defineSetter__
                let define_setter_fn = self.create_function(JsFunction::native(
                    "__defineSetter__".to_string(),
                    2,
                    |interp, this_val, args| {
                        // Step 1: ToObject(this) first
                        let o = match interp.to_object(this_val) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let setter = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                        let setter_is_callable = setter
                            .as_object_id()
                            .and_then(|id| interp.get_object_cell(id))
                            .is_some_and(|obj| obj.borrow().callable.is_some());
                        if !setter_is_callable {
                            return Completion::Throw(
                                interp.create_type_error("Setter must be a function"),
                            );
                        }
                        let key_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        let key = match interp.to_property_key(&key_val) {
                            Ok(k) => k,
                            Err(e) => return Completion::Throw(e),
                        };
                        let desc = PropertyDescriptor {
                            value: None,
                            writable: None,
                            get: None,
                            set: Some(setter),
                            enumerable: Some(true),
                            configurable: Some(true),
                        };
                        if let Some(obj_id) = o.as_object_id() {
                            let is_proxy = interp
                                .get_object_cell(obj_id)
                                .map(|obj| {
                                    let b = obj.borrow();
                                    b.is_proxy() || b.is_proxy_revoked()
                                })
                                .unwrap_or(false);
                            if is_proxy {
                                let desc_val = interp.from_property_descriptor(&desc);
                                match interp.proxy_define_own_property(obj_id, key, &desc_val) {
                                    Ok(true) => {}
                                    Ok(false) => {
                                        return Completion::Throw(
                                            interp.create_type_error("Cannot define property"),
                                        );
                                    }
                                    Err(e) => return Completion::Throw(e),
                                }
                            } else if let Some(obj) = interp.get_object(obj_id)
                                && !obj.borrow_mut().define_own_property(key, desc)
                            {
                                return Completion::Throw(
                                    interp.create_type_error("Cannot define property"),
                                );
                            }
                        }
                        Completion::Normal(JsValue::UNDEFINED)
                    },
                ));
                proto_obj
                    .borrow_mut()
                    .insert_builtin("__defineSetter__".to_string(), define_setter_fn);

                // Object.prototype.__lookupGetter__
                let lookup_getter_fn = self.create_function(JsFunction::native(
                    "__lookupGetter__".to_string(),
                    1,
                    |interp, this_val, args| {
                        // Step 1: ToObject(this) first
                        let mut current = match interp.to_object(this_val) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let key_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        let key = match interp.to_property_key(&key_val) {
                            Ok(k) => k,
                            Err(e) => return Completion::Throw(e),
                        };
                        while let Some(obj_id) = current.as_object_id() {
                            // Step 4a: O.[[GetOwnProperty]](key) (proxy-aware)
                            let desc_val =
                                match interp.proxy_get_own_property_descriptor(obj_id, &key) {
                                    Ok(v) => v,
                                    Err(e) => return Completion::Throw(e),
                                };
                            if !(desc_val).is_undefined() {
                                if let Ok(desc) = interp.to_property_descriptor(&desc_val)
                                    && let Some(g) = desc.get
                                {
                                    return Completion::Normal(g);
                                }
                                return Completion::Normal(JsValue::UNDEFINED);
                            }
                            // Step 4c: O.[[GetPrototypeOf]]() (proxy-aware)
                            let is_proxy = interp
                                .get_object_cell(obj_id)
                                .map(|o| {
                                    let b = o.borrow();
                                    b.is_proxy() || b.is_proxy_revoked()
                                })
                                .unwrap_or(false);
                            let proto = if is_proxy {
                                match interp.proxy_get_prototype_of(obj_id) {
                                    Ok(p) => p,
                                    Err(e) => return Completion::Throw(e),
                                }
                            } else if let Some(obj) = interp.get_object(obj_id) {
                                match obj.borrow().prototype_id {
                                    Some(p) => {
                                        let pid = p;
                                        JsValue::object(pid)
                                    }
                                    None => JsValue::NULL,
                                }
                            } else {
                                break;
                            };
                            if (proto).is_null() {
                                break;
                            }
                            current = proto;
                        }
                        Completion::Normal(JsValue::UNDEFINED)
                    },
                ));
                proto_obj
                    .borrow_mut()
                    .insert_builtin("__lookupGetter__".to_string(), lookup_getter_fn);

                // Object.prototype.__lookupSetter__
                let lookup_setter_fn = self.create_function(JsFunction::native(
                    "__lookupSetter__".to_string(),
                    1,
                    |interp, this_val, args| {
                        // Step 1: ToObject(this) first
                        let mut current = match interp.to_object(this_val) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let key_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        let key = match interp.to_property_key(&key_val) {
                            Ok(k) => k,
                            Err(e) => return Completion::Throw(e),
                        };
                        while let Some(obj_id) = current.as_object_id() {
                            let desc_val =
                                match interp.proxy_get_own_property_descriptor(obj_id, &key) {
                                    Ok(v) => v,
                                    Err(e) => return Completion::Throw(e),
                                };
                            if !(desc_val).is_undefined() {
                                if let Ok(desc) = interp.to_property_descriptor(&desc_val)
                                    && let Some(s) = desc.set
                                {
                                    return Completion::Normal(s);
                                }
                                return Completion::Normal(JsValue::UNDEFINED);
                            }
                            let is_proxy = interp
                                .get_object_cell(obj_id)
                                .map(|o| {
                                    let b = o.borrow();
                                    b.is_proxy() || b.is_proxy_revoked()
                                })
                                .unwrap_or(false);
                            let proto = if is_proxy {
                                match interp.proxy_get_prototype_of(obj_id) {
                                    Ok(p) => p,
                                    Err(e) => return Completion::Throw(e),
                                }
                            } else if let Some(obj) = interp.get_object(obj_id) {
                                match obj.borrow().prototype_id {
                                    Some(p) => {
                                        let pid = p;
                                        JsValue::object(pid)
                                    }
                                    None => JsValue::NULL,
                                }
                            } else {
                                break;
                            };
                            if (proto).is_null() {
                                break;
                            }
                            current = proto;
                        }
                        Completion::Normal(JsValue::UNDEFINED)
                    },
                ));
                proto_obj
                    .borrow_mut()
                    .insert_builtin("__lookupSetter__".to_string(), lookup_setter_fn);

                // Object.prototype.__proto__ accessor (Annex B §B.2.2.1)
                let proto_getter = self.create_function(JsFunction::native(
                    "get __proto__".to_string(),
                    0,
                    |interp, this_val, _args| {
                        // 1. Let O be ? ToObject(this value).
                        let obj_val = match interp.to_object(this_val) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => return Completion::Normal(JsValue::UNDEFINED),
                        };
                        // 2. Return ? O.[[GetPrototypeOf]]()
                        if let Some(obj_id) = obj_val.as_object_id()
                            && let Some(obj) = interp.get_object(obj_id)
                        {
                            // Proxy getPrototypeOf trap
                            let res = {
                                let _b = obj.borrow();
                                _b.is_proxy() || _b.is_proxy_revoked()
                            };
                            if res {
                                match interp.proxy_get_prototype_of(obj_id) {
                                    Ok(v) => return Completion::Normal(v),
                                    Err(e) => return Completion::Throw(e),
                                }
                            }
                            return if let Some(pid) = obj.borrow().prototype_id {
                                Completion::Normal(JsValue::object(pid))
                            } else {
                                Completion::Normal(JsValue::NULL)
                            };
                        }
                        Completion::Normal(JsValue::NULL)
                    },
                ));
                let proto_setter = self.create_function(JsFunction::native(
                    "set __proto__".to_string(),
                    1,
                    |interp, this_val, args| {
                        // 1. Let O be ? RequireObjectCoercible(this value).
                        if (this_val).is_nullish() {
                            return Completion::Throw(interp.create_type_error(
                                "Cannot convert undefined or null to object",
                            ));
                        }
                        let proto = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        // 2. If Type(proto) is neither Object nor Null, return undefined.
                        if !proto.is_object() && !proto.is_null() {
                            return Completion::Normal(JsValue::UNDEFINED);
                        }
                        // 3. If Type(O) is not Object, return undefined.
                        if !(this_val).is_object() {
                            return Completion::Normal(JsValue::UNDEFINED);
                        }
                        // 4. Let status be ? O.[[SetPrototypeOf]](proto).
                        if let Some(this_id) = this_val.as_object_id()
                            && let Some(obj) = interp.get_object(this_id) {
                                let res = { let _b = obj.borrow(); _b.is_proxy() || _b.is_proxy_revoked() }; if res {
                                    match interp.proxy_set_prototype_of(this_id, &proto) {
                                        Ok(success) => {
                                            if !success {
                                                return Completion::Throw(interp.create_type_error(
                                                    "Object.prototype.__proto__: proxy setPrototypeOf returned false",
                                                ));
                                            }
                                            return Completion::Normal(JsValue::UNDEFINED);
                                        }
                                        Err(e) => return Completion::Throw(e),
                                    }
                                }
                                // OrdinarySetPrototypeOf: SameValue(V, current) check
                                let current_proto_id = obj.borrow().prototype_id;
                                let same = current_proto_id.is_none() && proto.is_null()
                                    || current_proto_id
                                        .zip(proto.as_object_id())
                                        .is_some_and(|(current_id, proto_id)| current_id == proto_id);
                                if same {
                                    return Completion::Normal(JsValue::UNDEFINED);
                                }
                                if !obj.borrow().extensible {
                                    return Completion::Throw(interp.create_type_error(
                                        "Object is not extensible",
                                    ));
                                }
                                if proto.is_null() {
                                    obj.borrow_mut().prototype_id = None;
                                } else if let Some(proto_id) = proto.as_object_id() {
                                        let obj_id = obj.borrow().id;
                                        let mut check = Some(proto_id);
                                        while let Some(c_id) = check {
                                            if Some(c_id) == obj_id {
                                                return Completion::Throw(
                                                    interp.create_type_error(
                                                        "Cyclic __proto__ value",
                                                    ),
                                                );
                                            }
                                            check = interp.get_object_cell_expect(c_id).borrow().prototype_id;
                                        }
                                        obj.borrow_mut().prototype_id = Some(proto_id);
                                }
                            }
                        Completion::Normal(JsValue::UNDEFINED)
                    },
                ));
                proto_obj.borrow_mut().insert_property(
                    "__proto__".to_string(),
                    PropertyDescriptor::accessor(
                        Some(proto_getter),
                        Some(proto_setter),
                        false,
                        true,
                    ),
                );
            }

            // Add Object.defineProperty
            let define_property_fn = self.create_function(JsFunction::native(
                "defineProperty".to_string(),
                3,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    if !(target).is_object() {
                        return Completion::Throw(interp.create_type_error(
                            "Object.defineProperty called on non-object",
                        ));
                    }
                    let key_raw = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                    let key = match interp.to_property_key(&key_raw) {
                        Ok(s) => s,
                        Err(e) => return Completion::Throw(e),
                    };
                    let desc_val = args.get(2).cloned().unwrap_or(JsValue::UNDEFINED);
                    if let Some(target_id) = target.as_object_id()
                        && let Some(obj) = interp.get_object(target_id)
                    {
                        // Deferred namespace: trigger evaluation on [[DefineOwnProperty]]
                        {
                            let is_deferred_ns = obj.borrow().module_namespace().is_some_and(|ns| ns.deferred);
                            if is_deferred_ns && !Interpreter::is_symbol_like_namespace_key(&key, true)
                                && let Err(e) = interp.ensure_deferred_namespace_evaluation(target_id) {
                                    return Completion::Throw(e);
                                }
                        }
                        let obj = interp.get_object(target_id).unwrap();
                        // Proxy defineProperty trap
                        let res = { let _b = obj.borrow(); _b.is_proxy() || _b.is_proxy_revoked() }; if res {
                            // Re-parse descriptor so proxy trap gets a fresh copy with coerced booleans
                            let reparsed_desc = match interp.to_property_descriptor(&desc_val) {
                                Ok(pd) => interp.from_property_descriptor(&pd),
                                Err(Some(e)) => return Completion::Throw(e),
                                Err(None) => desc_val.clone(),
                            };
                            match interp.proxy_define_own_property(target_id, key, &reparsed_desc) {
                                Ok(success) => {
                                    if !success {
                                        return Completion::Throw(interp.create_type_error(
                                            "Cannot define property, object is not extensible or property is non-configurable",
                                        ));
                                    }
                                    return Completion::Normal(target);
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        // Module namespace exotic: [[DefineOwnProperty]]
                        if obj.borrow().module_namespace().is_some() {
                            match interp.to_property_descriptor(&desc_val) {
                                Ok(desc) => {
                                    let success = obj.borrow_mut().define_own_property(key, desc);
                                    if !success {
                                        return Completion::Throw(interp.create_type_error(
                                            "Cannot define property on a module namespace object",
                                        ));
                                    }
                                    return Completion::Normal(target);
                                }
                                Err(Some(e)) => return Completion::Throw(e),
                                Err(None) => return Completion::Normal(target),
                            }
                        }
                        match interp.to_property_descriptor(&desc_val) {
                            Ok(desc) => {
                                let is_array = obj.borrow().class_name == "Array";
                                let is_ta = obj.borrow().typed_array_info().is_some();
                                if is_array {
                                    match interp.array_define_own_property(target_id as usize, &key, desc) {
                                        Ok(true) => {}
                                        Ok(false) => {
                                            return Completion::Throw(interp.create_type_error(
                                                "Cannot define property, object is not extensible or property is non-configurable",
                                            ));
                                        }
                                        Err(e) => return Completion::Throw(e),
                                    }
                                } else if is_ta {
                                    match interp.typed_array_define_own_property(target_id, &key, &desc) {
                                        Ok(Some(true)) => {}
                                        Ok(Some(false)) => {
                                            return Completion::Throw(interp.create_type_error(
                                                "Cannot define property, object is not extensible or property is non-configurable",
                                            ));
                                        }
                                        Ok(None) => {
                                            if !obj.borrow_mut().define_own_property(key, desc) {
                                                return Completion::Throw(interp.create_type_error(
                                                    "Cannot define property, object is not extensible or property is non-configurable",
                                                ));
                                            }
                                        }
                                        Err(e) => return Completion::Throw(e),
                                    }
                                } else if !obj.borrow_mut().define_own_property(key, desc) {
                                    return Completion::Throw(interp.create_type_error(
                                        "Cannot define property, object is not extensible or property is non-configurable",
                                    ));
                                }
                            }
                            Err(Some(e)) => return Completion::Throw(e),
                            Err(None) => {}
                        }
                    }
                    Completion::Normal(target)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("defineProperty".to_string(), define_property_fn);

            // Add Object.getOwnPropertyDescriptor
            let get_own_prop_desc_fn = self.create_function(JsFunction::native(
                "getOwnPropertyDescriptor".to_string(),
                2,
                |interp, _this, args| {
                    let target_arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let target = match interp.to_object(&target_arg) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => return Completion::Normal(JsValue::UNDEFINED),
                    };
                    let key_raw = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                    let key = match interp.to_property_key(&key_raw) {
                        Ok(s) => s,
                        Err(e) => return Completion::Throw(e),
                    };
                    if let Some(target_id) = target.as_object_id() {
                        // Deferred namespace: trigger evaluation on [[GetOwnProperty]] with non-symbol-like key
                        {
                            let deferred_ns = interp.get_object_cell(target_id).and_then(|obj| {
                                let b = obj.borrow();
                                b.module_namespace().map(|ns| ns.deferred)
                            });
                            if deferred_ns == Some(true)
                                && !Interpreter::is_symbol_like_namespace_key(&key, true)
                                && let Err(e) =
                                    interp.ensure_deferred_namespace_evaluation(target_id)
                            {
                                return Completion::Throw(e);
                            }
                        }
                        // Proxy getOwnPropertyDescriptor trap
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
                                .get_object(target_id)
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
                            // Non-export key on namespace (e.g. Symbol.toStringTag): fall through
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
            obj_func
                .borrow_mut()
                .insert_builtin("getOwnPropertyDescriptor".to_string(), get_own_prop_desc_fn);

            // Add Object.keys
            let keys_fn = self.create_function(JsFunction::native(
                "keys".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let obj_val = match interp.to_object(&target) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if let Some(obj_id) = obj_val.as_object_id() {
                        // [[OwnPropertyKeys]] for all keys
                        let all_keys = match interp.proxy_own_keys(obj_id) {
                            Ok(k) => k,
                            Err(e) => return Completion::Throw(e),
                        };
                        // For non-proxy, also include string wrapper char indices
                        let mut extra_str_keys: Vec<JsPropertyKey> = Vec::new();
                        if interp
                            .get_object_cell(obj_id)
                            .map(|ob| !ob.borrow().is_proxy())
                            .unwrap_or(false)
                            && let Some(obj) = interp.get_object_cell(obj_id)
                        {
                            let b = obj.borrow();
                            if let Some(s) = b.primitive_value.as_ref().and_then(JsValue::as_string)
                            {
                                for i in 0..s.len() {
                                    extra_str_keys.push(JsPropertyKey::from(i.to_string()));
                                }
                            }
                        }
                        let mut result = Vec::new();
                        // Add extra char index keys first (string wrapper exotic)
                        for k in &extra_str_keys {
                            result.push(JsValue::string(k.to_js_string()));
                        }
                        // [[OwnPropertyKeys]] already supplies the required order for ordinary
                        // objects and preserves the trap order for proxies.
                        for kv in &all_keys {
                            if let Some(s) = kv.as_string() {
                                let key = JsPropertyKey::from_js_string(&s);
                                if !extra_str_keys.contains(&key) {
                                    result.push(JsValue::string(s));
                                }
                            }
                        }
                        // For each string key call [[GetOwnProperty]] and filter enumerable
                        let mut enum_keys: Vec<JsValue> = Vec::new();
                        // Extra char index keys are always enumerable (string exotic)
                        for k in &extra_str_keys {
                            enum_keys.push(JsValue::string(k.to_js_string()));
                        }
                        for kv in result.iter().skip(extra_str_keys.len()) {
                            if let Some(s) = kv.as_string() {
                                let k = JsPropertyKey::from_js_string(&s);
                                let desc_val =
                                    match interp.proxy_get_own_property_descriptor(obj_id, &k) {
                                        Ok(v) => v,
                                        Err(e) => return Completion::Throw(e),
                                    };
                                if (desc_val).is_undefined() {
                                    continue;
                                }
                                if let Ok(desc) = interp.to_property_descriptor(&desc_val)
                                    && desc.enumerable != Some(false)
                                {
                                    enum_keys.push(kv.clone());
                                }
                            }
                        }
                        return Completion::Normal(interp.create_array(enum_keys));
                    }
                    Completion::Normal(interp.create_array(Vec::new()))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("keys".to_string(), keys_fn);

            // Add Object.freeze
            let freeze_fn = self.create_function(JsFunction::native(
                "freeze".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    if let Some(obj_id) = target.as_object_id() {
                        let is_proxy = interp
                            .get_object_cell(obj_id)
                            .map(|obj| {
                                let b = obj.borrow();
                                b.is_proxy() || b.is_proxy_revoked()
                            })
                            .unwrap_or(false);
                        if is_proxy {
                            // SetIntegrityLevel: preventExtensions
                            match interp.proxy_prevent_extensions(obj_id) {
                                Ok(true) => {}
                                Ok(false) => {
                                    return Completion::Throw(interp.create_type_error(
                                        "Cannot freeze: preventExtensions returned false",
                                    ));
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                            // Get own keys
                            let keys = match interp.proxy_own_keys(obj_id) {
                                Ok(k) => k,
                                Err(e) => return Completion::Throw(e),
                            };
                            for key_val in keys {
                                let key = to_property_key_string(&key_val);
                                // GetOwnProperty to determine accessor vs data
                                let desc_val =
                                    match interp.proxy_get_own_property_descriptor(obj_id, &key) {
                                        Ok(v) => v,
                                        Err(e) => return Completion::Throw(e),
                                    };
                                if (desc_val).is_undefined() {
                                    continue;
                                }
                                let desc = match interp.to_property_descriptor(&desc_val) {
                                    Ok(d) => d,
                                    Err(_) => continue,
                                };
                                let new_desc = if desc.is_accessor_descriptor() {
                                    PropertyDescriptor {
                                        configurable: Some(false),
                                        value: None,
                                        writable: None,
                                        get: None,
                                        set: None,
                                        enumerable: None,
                                    }
                                } else {
                                    PropertyDescriptor {
                                        configurable: Some(false),
                                        writable: Some(false),
                                        value: None,
                                        get: None,
                                        set: None,
                                        enumerable: None,
                                    }
                                };
                                let new_desc_val = interp.from_property_descriptor(&new_desc);
                                match interp.proxy_define_own_property(obj_id, key, &new_desc_val) {
                                    Ok(_) => {}
                                    Err(e) => return Completion::Throw(e),
                                }
                            }
                        } else if let Some(obj) = interp.get_object(obj_id) {
                            // TypedArray [[PreventExtensions]] — §10.4.5.2
                            {
                                let b = obj.borrow();
                                if let Some(ta) = b.typed_array_info() {
                                    use crate::interpreter::types::is_typed_array_fixed_length;
                                    let is_fixed = b
                                        .view_buffer_object_id()
                                        .and_then(|buf_id| interp.get_object(buf_id))
                                        .map(|buf| is_typed_array_fixed_length(ta, &buf.borrow()))
                                        .unwrap_or(true);
                                    if !is_fixed {
                                        return Completion::Throw(interp.create_type_error(
                                            "Cannot freeze array buffer views with elements",
                                        ));
                                    }
                                }
                            }
                            // §7.3.16 step 1: preventExtensions succeeds
                            obj.borrow_mut().extensible = false;
                            // TA elements can't be made non-configurable/non-writable (§10.4.5.3)
                            {
                                let b = obj.borrow();
                                if let Some(ta) = b.typed_array_info() {
                                    use crate::interpreter::types::typed_array_length;
                                    if typed_array_length(ta) > 0 {
                                        return Completion::Throw(interp.create_type_error(
                                            "Cannot freeze array buffer views with elements",
                                        ));
                                    }
                                }
                            }
                            let is_array = obj.borrow().array_elements().is_some();
                            if is_array {
                                let keys = match interp.proxy_own_keys(obj_id) {
                                    Ok(keys) => keys,
                                    Err(e) => return Completion::Throw(e),
                                };
                                for key_val in keys {
                                    let key = to_property_key_string(&key_val);
                                    let desc_val = match interp
                                        .proxy_get_own_property_descriptor(obj_id, &key)
                                    {
                                        Ok(v) => v,
                                        Err(e) => return Completion::Throw(e),
                                    };
                                    if desc_val.is_undefined() {
                                        continue;
                                    }
                                    let desc = match interp.to_property_descriptor(&desc_val) {
                                        Ok(desc) => desc,
                                        Err(Some(e)) => return Completion::Throw(e),
                                        Err(None) => continue,
                                    };
                                    let new_desc = if desc.is_accessor_descriptor() {
                                        PropertyDescriptor {
                                            configurable: Some(false),
                                            value: None,
                                            writable: None,
                                            get: None,
                                            set: None,
                                            enumerable: None,
                                        }
                                    } else {
                                        PropertyDescriptor {
                                            configurable: Some(false),
                                            writable: Some(false),
                                            value: None,
                                            get: None,
                                            set: None,
                                            enumerable: None,
                                        }
                                    };
                                    let new_desc_val = interp.from_property_descriptor(&new_desc);
                                    // §7.3.8 DefinePropertyOrThrow: throw if [[DefineOwnProperty]] returns false
                                    match interp.proxy_define_own_property(
                                        obj_id,
                                        key.clone(),
                                        &new_desc_val,
                                    ) {
                                        Ok(true) => {}
                                        Ok(false) => {
                                            return Completion::Throw(interp.create_type_error(
                                                &format!("Cannot freeze property '{key}'"),
                                            ));
                                        }
                                        Err(e) => return Completion::Throw(e),
                                    }
                                }
                            } else {
                                let keys_and_descs: Vec<(JsPropertyKey, PropertyDescriptor)> = {
                                    let b = obj.borrow();
                                    b.properties
                                        .iter()
                                        .map(|(k, d)| (k.clone(), d.clone()))
                                        .collect()
                                };
                                for (key, desc) in keys_and_descs {
                                    let new_desc = if desc.is_accessor_descriptor() {
                                        PropertyDescriptor {
                                            configurable: Some(false),
                                            value: None,
                                            writable: None,
                                            get: None,
                                            set: None,
                                            enumerable: None,
                                        }
                                    } else {
                                        PropertyDescriptor {
                                            configurable: Some(false),
                                            writable: Some(false),
                                            value: None,
                                            get: None,
                                            set: None,
                                            enumerable: None,
                                        }
                                    };
                                    if !obj.borrow_mut().define_own_property(key.clone(), new_desc)
                                    {
                                        return Completion::Throw(interp.create_type_error(
                                            &format!("Cannot freeze property '{key}'"),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    Completion::Normal(target)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("freeze".to_string(), freeze_fn);

            // Add Object.getPrototypeOf
            let get_proto_fn = self.create_function(JsFunction::native(
                "getPrototypeOf".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    // ES6 §19.1.2.9: Let obj = ? ToObject(O)
                    let obj_val = match interp.to_object(&target) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        other => return other,
                    };
                    if let Some(obj_id) = obj_val.as_object_id()
                        && let Some(obj) = interp.get_object(obj_id)
                    {
                        // Proxy getPrototypeOf trap
                        let res = {
                            let _b = obj.borrow();
                            _b.is_proxy() || _b.is_proxy_revoked()
                        };
                        if res {
                            match interp.proxy_get_prototype_of(obj_id) {
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
            obj_func
                .borrow_mut()
                .insert_builtin("getPrototypeOf".to_string(), get_proto_fn);

            // Add Object.create
            let create_fn = self.create_function(JsFunction::native(
                "create".to_string(),
                2,
                |interp, _this, args| {
                    let proto_arg = args.first().cloned().unwrap_or(JsValue::NULL);
                    if !proto_arg.is_object() && !proto_arg.is_null() {
                        return Completion::Throw(
                            interp.create_type_error(
                                "Object prototype may only be an Object or null",
                            ),
                        );
                    }
                    let new_obj_id = interp.create_object_id();
                    if let Some(proto_id) = proto_arg.as_object_id() {
                        interp
                            .get_object_cell_expect(new_obj_id)
                            .borrow_mut()
                            .prototype_id = Some(proto_id);
                    } else if proto_arg.is_null() {
                        interp
                            .get_object_cell_expect(new_obj_id)
                            .borrow_mut()
                            .prototype_id = None;
                    } else {
                        unreachable!()
                    }
                    let id = new_obj_id;
                    let target = JsValue::object(id);

                    let props_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                    if !(props_arg).is_undefined() {
                        // ObjectDefineProperties(target, props_arg)
                        let props_obj_val = match interp.to_object(&props_arg) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => return Completion::Normal(target),
                        };
                        if let Some(d_id) = props_obj_val.as_object_id() {
                            let all_keys = match interp.proxy_own_keys(d_id) {
                                Ok(k) => k,
                                Err(e) => return Completion::Throw(e),
                            };
                            for key_val in all_keys {
                                let key = to_property_key_string(&key_val);
                                // Check enumerability via [[GetOwnProperty]]
                                let desc_check =
                                    match interp.proxy_get_own_property_descriptor(d_id, &key) {
                                        Ok(v) => v,
                                        Err(e) => return Completion::Throw(e),
                                    };
                                if (desc_check).is_undefined() {
                                    continue;
                                }
                                if let Ok(chk) = interp.to_property_descriptor(&desc_check)
                                    && chk.enumerable == Some(false)
                                {
                                    continue;
                                }
                                let prop_desc_val =
                                    match interp.get_object_property(d_id, &key, &props_obj_val) {
                                        Completion::Normal(v) => v,
                                        Completion::Throw(e) => return Completion::Throw(e),
                                        _ => continue,
                                    };
                                match interp.to_property_descriptor(&prop_desc_val) {
                                    Ok(desc) => {
                                        if let Some(target_obj) = interp.get_object(id)
                                            && !target_obj
                                                .borrow_mut()
                                                .define_own_property(key, desc)
                                        {
                                            return Completion::Throw(interp.create_type_error(
                                                "Cannot define property on non-extensible object",
                                            ));
                                        }
                                    }
                                    Err(Some(e)) => return Completion::Throw(e),
                                    Err(None) => {
                                        return Completion::Throw(interp.create_type_error(
                                            "Property description must be an object",
                                        ));
                                    }
                                }
                            }
                        }
                    }

                    Completion::Normal(target)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("create".to_string(), create_fn);

            // Object.entries
            let entries_fn = self.create_function(JsFunction::native(
                "entries".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let obj_val = match interp.to_object(&target) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if let Some(obj_id) = obj_val.as_object_id() {
                        // EnumerableOwnProperties: call [[OwnPropertyKeys]], then [[GetOwnProperty]] once per string key
                        let all_keys = match interp.proxy_own_keys(obj_id) {
                            Ok(k) => k,
                            Err(e) => return Completion::Throw(e),
                        };
                        let mut pairs = Vec::new();
                        for key_val in all_keys {
                            let Some(key_string) = key_val.as_string() else {
                                continue;
                            };
                            let k = JsPropertyKey::from_js_string(&key_string);
                            let desc_val =
                                match interp.proxy_get_own_property_descriptor(obj_id, &k) {
                                    Ok(v) => v,
                                    Err(e) => return Completion::Throw(e),
                                };
                            if (desc_val).is_undefined() {
                                continue;
                            }
                            if let Ok(desc) = interp.to_property_descriptor(&desc_val) {
                                if desc.enumerable == Some(false) {
                                    continue;
                                }
                            } else {
                                continue;
                            }
                            let val = match interp.get_object_property(obj_id, &k, &obj_val) {
                                Completion::Normal(v) => v,
                                other => return other,
                            };
                            pairs.push(interp.create_array(vec![JsValue::string(key_string), val]));
                        }
                        let arr = interp.create_array(pairs);
                        return Completion::Normal(arr);
                    }
                    Completion::Normal(interp.create_array(Vec::new()))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("entries".to_string(), entries_fn);

            // Object.values
            let values_fn = self.create_function(JsFunction::native(
                "values".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let obj_val = match interp.to_object(&target) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if let Some(obj_id) = obj_val.as_object_id() {
                        // EnumerableOwnProperties: call [[OwnPropertyKeys]], then [[GetOwnProperty]] once per string key
                        let all_keys = match interp.proxy_own_keys(obj_id) {
                            Ok(k) => k,
                            Err(e) => return Completion::Throw(e),
                        };
                        let mut values = Vec::new();
                        for key_val in all_keys {
                            let Some(key_string) = key_val.as_string() else {
                                continue;
                            };
                            let k = JsPropertyKey::from_js_string(&key_string);
                            let desc_val =
                                match interp.proxy_get_own_property_descriptor(obj_id, &k) {
                                    Ok(v) => v,
                                    Err(e) => return Completion::Throw(e),
                                };
                            if (desc_val).is_undefined() {
                                continue;
                            }
                            if let Ok(desc) = interp.to_property_descriptor(&desc_val) {
                                if desc.enumerable == Some(false) {
                                    continue;
                                }
                            } else {
                                continue;
                            }
                            let val = match interp.get_object_property(obj_id, &k, &obj_val) {
                                Completion::Normal(v) => v,
                                other => return other,
                            };
                            values.push(val);
                        }
                        let arr = interp.create_array(values);
                        return Completion::Normal(arr);
                    }
                    Completion::Normal(interp.create_array(Vec::new()))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("values".to_string(), values_fn);

            // Object.assign
            let assign_fn = self.create_function(JsFunction::native(
                "assign".to_string(),
                2,
                |interp, _this, args| {
                    let target_arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let target = match interp.to_object(&target_arg) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => return Completion::Normal(JsValue::UNDEFINED),
                    };
                    let Some(t_id) = target.as_object_id() else {
                        return Completion::Normal(target);
                    };
                    for source in args.iter().skip(1) {
                        if (source).is_nullish() {
                            continue;
                        }
                        let src_obj_val = match interp.to_object(source) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => continue,
                        };
                        let Some(s_id) = src_obj_val.as_object_id() else {
                            continue;
                        };
                        let raw_keys = match interp.proxy_own_keys(s_id) {
                            Ok(k) => k,
                            Err(e) => return Completion::Throw(e),
                        };
                        for key_val in raw_keys {
                            let key_str = to_property_key_string(&key_val);
                            // [[GetOwnProperty]] to check enumerability
                            let desc_result =
                                interp.proxy_get_own_property_descriptor(s_id, &key_str);
                            let desc_obj = match desc_result {
                                Ok(v) => v,
                                Err(e) => return Completion::Throw(e),
                            };
                            // If descriptor is undefined or not enumerable, skip
                            if (desc_obj).is_undefined() {
                                continue;
                            }
                            let desc = interp.to_property_descriptor(&desc_obj);
                            let is_enum = match &desc {
                                Ok(d) => d.enumerable != Some(false),
                                Err(_) => false,
                            };
                            if !is_enum {
                                continue;
                            }
                            // [[Get]] from source
                            let val = match interp.get_object_property(s_id, &key_str, &src_obj_val)
                            {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => JsValue::UNDEFINED,
                            };
                            // [[Set]] on target with Throw=true
                            match interp.proxy_set(t_id, &key_str, val, &target) {
                                Ok(true) => {}
                                Ok(false) => {
                                    return Completion::Throw(
                                        interp.create_type_error(
                                            "Cannot assign to read only property",
                                        ),
                                    );
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                    }
                    Completion::Normal(target)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("assign".to_string(), assign_fn);

            // Object.groupBy
            let group_by_fn = self.create_function(JsFunction::native(
                "groupBy".to_string(),
                2,
                |interp, _this, args| {
                    let items = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let callback = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                    let callback_is_callable = callback
                        .as_object_id()
                        .and_then(|id| interp.get_object_cell(id))
                        .is_some_and(|obj| obj.borrow().callable.is_some());
                    if !callback_is_callable {
                        return Completion::Throw(
                            interp.create_type_error("callbackfn is not a function"),
                        );
                    }
                    let iterator = match interp.get_iterator(&items) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };
                    interp.with_gc_root_scope(|interp| {
                        interp.gc_root_value(&iterator);
                        let result_obj_id = interp.create_object_id();
                        interp
                            .get_object_cell_expect(result_obj_id)
                            .borrow_mut()
                            .prototype_id = None;
                        let result_id = result_obj_id;
                        let result_val = JsValue::object(result_id);
                        interp.gc_root_value(&result_val);
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
                            let key_val = match interp.call_function(
                                &callback,
                                &JsValue::UNDEFINED,
                                &[value.clone(), JsValue::number(k as f64)],
                            ) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => JsValue::UNDEFINED,
                            };
                            // ToPropertyKey (with error propagation)
                            let key_str = match interp.to_property_key(&key_val) {
                                Ok(k) => k,
                                Err(e) => {
                                    // IfAbruptCloseIterator
                                    let _ = interp.iterator_close(&iterator, e.clone());
                                    return Completion::Throw(e);
                                }
                            };
                            if let Some(obj) = interp.get_object(result_id) {
                                let existing = interp.get_property_on_id(result_id, &key_str);
                                if let Some(array_id) = existing.as_object_id()
                                    && let Some(arr) = interp.get_object(array_id)
                                {
                                    let len_val = interp.get_property_on_id(array_id, "length");
                                    let len = to_number(&len_val) as usize;
                                    // Use enumerable property insertion
                                    arr.borrow_mut().insert_property(
                                        len.to_string(),
                                        PropertyDescriptor::data(value, true, true, true),
                                    );
                                    arr.borrow_mut().insert_property(
                                        "length".to_string(),
                                        PropertyDescriptor::data(
                                            JsValue::number((len + 1) as f64),
                                            true,
                                            false,
                                            false,
                                        ),
                                    );
                                } else {
                                    let new_arr = interp.create_array(vec![value]);
                                    // Create enumerable property
                                    obj.borrow_mut().insert_property(
                                        key_str,
                                        PropertyDescriptor::data(new_arr, true, true, true),
                                    );
                                }
                            }
                            k += 1;
                        }
                        Completion::Normal(result_val)
                    })
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("groupBy".to_string(), group_by_fn);

            // Object.is
            let is_fn = self.create_function(JsFunction::native(
                "is".to_string(),
                2,
                |_interp, _this, args| {
                    let a = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let b = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                    let result = match (a.as_number(), b.as_number()) {
                        (Some(x), Some(y)) => number_ops::same_value(x, y),
                        _ => strict_equality(&a, &b),
                    };
                    Completion::Normal(JsValue::boolean(result))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("is".to_string(), is_fn);

            // Object.getOwnPropertyNames
            let gopn_fn = self.create_function(JsFunction::native(
                "getOwnPropertyNames".to_string(),
                1,
                |interp, _this, args| {
                    let target_arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let target = match interp.to_object(&target_arg) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => return Completion::Normal(interp.create_array(Vec::new())),
                    };
                    let Some(obj_id) = target.as_object_id() else {
                        return Completion::Normal(interp.create_array(Vec::new()));
                    };
                    let keys = match interp.proxy_own_keys(obj_id) {
                        Ok(keys) => keys,
                        Err(e) => return Completion::Throw(e),
                    };
                    let names = keys.into_iter().filter(JsValue::is_string).collect();
                    Completion::Normal(interp.create_array(names))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("getOwnPropertyNames".to_string(), gopn_fn);

            // Object.getOwnPropertySymbols
            let gops_fn = self.create_function(JsFunction::native(
                "getOwnPropertySymbols".to_string(),
                1,
                |interp, _this, args| {
                    let target_arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let target = match interp.to_object(&target_arg) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => return Completion::Normal(interp.create_array(Vec::new())),
                    };
                    if let Some(obj_id) = target.as_object_id() {
                        // Deferred namespace: trigger evaluation on [[OwnPropertyKeys]]
                        if let Some(obj) = interp.get_object_cell(obj_id) {
                            let is_deferred_ns = obj
                                .borrow()
                                .module_namespace()
                                .as_ref()
                                .is_some_and(|ns| ns.deferred);
                            if is_deferred_ns
                                && let Err(e) = interp.ensure_deferred_namespace_evaluation(obj_id)
                            {
                                return Completion::Throw(e);
                            }
                        }
                        // Use [[OwnPropertyKeys]] for proxy support
                        let all_keys = if let Some(obj) = interp.get_object(obj_id) {
                            if obj.borrow().is_proxy() || obj.borrow().is_proxy_revoked() {
                                match interp.proxy_own_keys(obj_id) {
                                    Ok(keys) => {
                                        let sym_keys: Vec<JsValue> =
                                            keys.into_iter().filter(|k| (k).is_symbol()).collect();
                                        return Completion::Normal(interp.create_array(sym_keys));
                                    }
                                    Err(e) => return Completion::Throw(e),
                                }
                            } else {
                                // Return symbol keys in property_order first, then any not in order
                                let b = obj.borrow();
                                let mut sym_keys: Vec<JsPropertyKey> = b
                                    .property_order
                                    .iter()
                                    .filter(|k| k.is_symbol())
                                    .cloned()
                                    .collect();
                                for k in b.properties.keys() {
                                    if k.is_symbol() && !sym_keys.contains(k) {
                                        sym_keys.push(k.clone());
                                    }
                                }
                                sym_keys
                            }
                        } else {
                            Vec::new()
                        };
                        let symbols: Vec<JsValue> = all_keys
                            .iter()
                            .map(|k| interp.symbol_key_to_jsvalue(k))
                            .collect();
                        return Completion::Normal(interp.create_array(symbols));
                    }
                    Completion::Normal(interp.create_array(Vec::new()))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("getOwnPropertySymbols".to_string(), gops_fn);

            // Object.preventExtensions
            let pe_fn = self.create_function(JsFunction::native(
                "preventExtensions".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    if let Some(target_id) = target.as_object_id()
                        && let Some(obj) = interp.get_object(target_id)
                    {
                        // Proxy preventExtensions trap
                        let res = {
                            let _b = obj.borrow();
                            _b.is_proxy() || _b.is_proxy_revoked()
                        };
                        if res {
                            match interp.proxy_prevent_extensions(target_id) {
                                Ok(true) => return Completion::Normal(target),
                                Ok(false) => {
                                    return Completion::Throw(interp.create_type_error(
                                        "Object.preventExtensions returned false",
                                    ));
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        // TypedArray [[PreventExtensions]] — §10.4.5.2
                        {
                            let b = obj.borrow();
                            if let Some(ta) = b.typed_array_info() {
                                let is_fixed = b.view_buffer_object_id()
                                    .and_then(|buf_id| interp.get_object(buf_id))
                                    .map(|buf| {
                                        use crate::interpreter::types::is_typed_array_fixed_length;
                                        is_typed_array_fixed_length(ta, &buf.borrow())
                                    })
                                    .unwrap_or(true);
                                if !is_fixed {
                                    return Completion::Throw(interp.create_type_error(
                                        "Cannot prevent extensions on a TypedArray backed by a resizable buffer"
                                    ));
                                }
                            }
                        }
                        obj.borrow_mut().extensible = false;
                    }
                    Completion::Normal(target)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("preventExtensions".to_string(), pe_fn);

            // Object.isExtensible
            let ie_fn = self.create_function(JsFunction::native(
                "isExtensible".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    if let Some(target_id) = target.as_object_id()
                        && let Some(obj) = interp.get_object(target_id)
                    {
                        // Proxy isExtensible trap
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
            obj_func
                .borrow_mut()
                .insert_builtin("isExtensible".to_string(), ie_fn);

            // Object.isFrozen
            let frozen_fn = self.create_function(JsFunction::native(
                "isFrozen".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    if let Some(obj_id) = target.as_object_id() {
                        // TestIntegrityLevel: check extensible first, then each key
                        let is_proxy = interp
                            .get_object_cell(obj_id)
                            .map(|ob| {
                                let b = ob.borrow();
                                b.is_proxy() || b.is_proxy_revoked()
                            })
                            .unwrap_or(false);
                        if is_proxy {
                            // Check via proxy isExtensible trap
                            match interp.proxy_is_extensible(obj_id) {
                                Ok(true) => return Completion::Normal(JsValue::boolean(false)),
                                Ok(false) => {}
                                Err(e) => return Completion::Throw(e),
                            }
                        } else if let Some(obj) = interp.get_object(obj_id)
                            && obj.borrow().extensible
                        {
                            return Completion::Normal(JsValue::boolean(false));
                        }
                        // Get all own keys via proxy_own_keys
                        let all_keys = match interp.proxy_own_keys(obj_id) {
                            Ok(k) => k,
                            Err(e) => return Completion::Throw(e),
                        };
                        for key_val in all_keys {
                            let key = to_property_key_string(&key_val);
                            let desc_val =
                                match interp.proxy_get_own_property_descriptor(obj_id, &key) {
                                    Ok(v) => v,
                                    Err(e) => return Completion::Throw(e),
                                };
                            if let Ok(desc) = interp.to_property_descriptor(&desc_val) {
                                if desc.configurable != Some(false) {
                                    return Completion::Normal(JsValue::boolean(false));
                                }
                                if desc.is_data_descriptor() && desc.writable != Some(false) {
                                    return Completion::Normal(JsValue::boolean(false));
                                }
                            }
                        }
                        return Completion::Normal(JsValue::boolean(true));
                    }
                    Completion::Normal(JsValue::boolean(true))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("isFrozen".to_string(), frozen_fn);

            // Object.isSealed
            let sealed_fn = self.create_function(JsFunction::native(
                "isSealed".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    if let Some(obj_id) = target.as_object_id() {
                        let is_proxy = interp
                            .get_object_cell(obj_id)
                            .map(|ob| {
                                let b = ob.borrow();
                                b.is_proxy() || b.is_proxy_revoked()
                            })
                            .unwrap_or(false);
                        if is_proxy {
                            match interp.proxy_is_extensible(obj_id) {
                                Ok(true) => return Completion::Normal(JsValue::boolean(false)),
                                Ok(false) => {}
                                Err(e) => return Completion::Throw(e),
                            }
                        } else if let Some(obj) = interp.get_object(obj_id)
                            && obj.borrow().extensible
                        {
                            return Completion::Normal(JsValue::boolean(false));
                        }
                        let all_keys = match interp.proxy_own_keys(obj_id) {
                            Ok(k) => k,
                            Err(e) => return Completion::Throw(e),
                        };
                        for key_val in all_keys {
                            let key = to_property_key_string(&key_val);
                            let desc_val =
                                match interp.proxy_get_own_property_descriptor(obj_id, &key) {
                                    Ok(v) => v,
                                    Err(e) => return Completion::Throw(e),
                                };
                            if let Ok(desc) = interp.to_property_descriptor(&desc_val)
                                && desc.configurable != Some(false)
                            {
                                return Completion::Normal(JsValue::boolean(false));
                            }
                        }
                        return Completion::Normal(JsValue::boolean(true));
                    }
                    Completion::Normal(JsValue::boolean(true))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("isSealed".to_string(), sealed_fn);

            // Object.seal
            let seal_fn = self.create_function(JsFunction::native(
                "seal".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    if let Some(obj_id) = target.as_object_id() {
                        let is_proxy = interp
                            .get_object_cell(obj_id)
                            .map(|obj| {
                                let b = obj.borrow();
                                b.is_proxy() || b.is_proxy_revoked()
                            })
                            .unwrap_or(false);
                        if is_proxy {
                            match interp.proxy_prevent_extensions(obj_id) {
                                Ok(true) => {}
                                Ok(false) => {
                                    return Completion::Throw(interp.create_type_error(
                                        "Cannot seal: preventExtensions returned false",
                                    ));
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                            let keys = match interp.proxy_own_keys(obj_id) {
                                Ok(k) => k,
                                Err(e) => return Completion::Throw(e),
                            };
                            for key_val in keys {
                                let key = to_property_key_string(&key_val);
                                let desc_val =
                                    match interp.proxy_get_own_property_descriptor(obj_id, &key) {
                                        Ok(v) => v,
                                        Err(e) => return Completion::Throw(e),
                                    };
                                if (desc_val).is_undefined() {
                                    continue;
                                }
                                let new_desc = PropertyDescriptor {
                                    configurable: Some(false),
                                    value: None,
                                    writable: None,
                                    get: None,
                                    set: None,
                                    enumerable: None,
                                };
                                let new_desc_val = interp.from_property_descriptor(&new_desc);
                                match interp.proxy_define_own_property(obj_id, key, &new_desc_val) {
                                    Ok(_) => {}
                                    Err(e) => return Completion::Throw(e),
                                }
                            }
                        } else if let Some(obj) = interp.get_object(obj_id) {
                            // TypedArray [[PreventExtensions]] — §10.4.5.2
                            {
                                let b = obj.borrow();
                                if let Some(ta) = b.typed_array_info() {
                                    let is_fixed = b.view_buffer_object_id()
                                        .and_then(|buf_id| interp.get_object(buf_id))
                                        .map(|buf| {
                                            use crate::interpreter::types::is_typed_array_fixed_length;
                                            is_typed_array_fixed_length(ta, &buf.borrow())
                                        })
                                        .unwrap_or(true);
                                    if !is_fixed {
                                        return Completion::Throw(interp.create_type_error(
                                            "Cannot seal: preventExtensions returned false"
                                        ));
                                    }
                                }
                            }
                            // §7.3.16 step 1: preventExtensions succeeds, set non-extensible
                            obj.borrow_mut().extensible = false;
                            // TA elements can't be made non-configurable (§10.4.5.3)
                            {
                                let b = obj.borrow();
                                if let Some(ta) = b.typed_array_info() {
                                    use crate::interpreter::types::typed_array_length;
                                    if typed_array_length(ta) > 0 {
                                        return Completion::Throw(interp.create_type_error(
                                            "Cannot seal a TypedArray with elements"
                                        ));
                                    }
                                }
                            }
                            let is_array = obj.borrow().array_elements().is_some();
                            if is_array {
                                let keys = match interp.proxy_own_keys(obj_id) {
                                    Ok(keys) => keys,
                                    Err(e) => return Completion::Throw(e),
                                };
                                for key_val in keys {
                                    let key = to_property_key_string(&key_val);
                                    let new_desc = PropertyDescriptor {
                                        configurable: Some(false),
                                        value: None,
                                        writable: None,
                                        get: None,
                                        set: None,
                                        enumerable: None,
                                    };
                                    let new_desc_val = interp.from_property_descriptor(&new_desc);
                                    // §7.3.8 DefinePropertyOrThrow: throw if [[DefineOwnProperty]] returns false
                                    match interp.proxy_define_own_property(
                                        obj_id,
                                        key.clone(),
                                        &new_desc_val,
                                    ) {
                                        Ok(true) => {}
                                        Ok(false) => {
                                            return Completion::Throw(interp.create_type_error(
                                                &format!("Cannot seal property '{key}'"),
                                            ));
                                        }
                                        Err(e) => return Completion::Throw(e),
                                    }
                                }
                            } else {
                                let keys: Vec<JsPropertyKey> =
                                    obj.borrow().properties.keys().cloned().collect();
                                for key in keys {
                                    let new_desc = PropertyDescriptor {
                                        configurable: Some(false),
                                        value: None,
                                        writable: None,
                                        get: None,
                                        set: None,
                                        enumerable: None,
                                    };
                                    obj.borrow_mut().define_own_property(key, new_desc);
                                }
                            }
                        }
                    }
                    Completion::Normal(target)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("seal".to_string(), seal_fn);

            // Object.hasOwn
            let has_own_fn = self.create_function(JsFunction::native(
                "hasOwn".to_string(),
                2,
                |interp, _this, args| {
                    // Step 1: ToObject(O)
                    let target_arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let obj_val = match interp.to_object(&target_arg) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    // Step 2: ToPropertyKey(P)
                    let key_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                    let key = match interp.to_property_key(&key_arg) {
                        Ok(k) => k,
                        Err(e) => return Completion::Throw(e),
                    };
                    if let Some(obj_id) = obj_val.as_object_id() {
                        match interp.proxy_get_own_property_descriptor(obj_id, &key) {
                            Ok(desc_val) => {
                                return Completion::Normal(JsValue::boolean(
                                    !(desc_val).is_undefined(),
                                ));
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    Completion::Normal(JsValue::boolean(false))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("hasOwn".to_string(), has_own_fn);

            // Object.setPrototypeOf
            let set_proto_fn = self.create_function(JsFunction::native(
                "setPrototypeOf".to_string(),
                2,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    // Step 1: RequireObjectCoercible(O)
                    if (target).is_nullish() {
                        return Completion::Throw(interp.create_type_error(
                            "Object.setPrototypeOf called on null or undefined",
                        ));
                    }
                    let proto = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                    // Step 3: If Type(proto) is neither Object nor Null, throw TypeError
                    if !proto.is_object() && !proto.is_null() {
                        return Completion::Throw(interp.create_type_error(
                            "Object prototype may only be an Object or null",
                        ));
                    }
                    if let Some(target_id) = target.as_object_id()
                        && let Some(obj) = interp.get_object_cell(target_id)
                    {
                        // Proxy setPrototypeOf trap
                        let res = {
                            let _b = obj.borrow();
                            _b.is_proxy() || _b.is_proxy_revoked()
                        };
                        if res {
                            match interp.proxy_set_prototype_of(target_id, &proto) {
                                Ok(success) => {
                                    if !success {
                                        return Completion::Throw(interp.create_type_error(
                                            "'setPrototypeOf' on proxy: trap returned falsish",
                                        ));
                                    }
                                    return Completion::Normal(target);
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        // Immutable prototype exotic object check (Object.prototype)
                        if obj.borrow().is_immutable_prototype {
                            let current_proto_id = obj.borrow().prototype_id;
                            let new_proto_id = proto.as_object_id();
                            let same = ((proto).is_null() && current_proto_id.is_none())
                                || matches!((new_proto_id, current_proto_id), (Some(a), Some(b)) if a == b);
                            if !same {
                                return Completion::Throw(interp.create_type_error(
                                    "Immutable prototype object's [[Prototype]] may not be set",
                                ));
                            }
                            return Completion::Normal(target);
                        }
                        // OrdinarySetPrototypeOf checks
                        let current_proto_id = obj.borrow().prototype_id;
                        let new_proto_id = proto.as_object_id();
                        // Same value check
                        let same = ((proto).is_null() && current_proto_id.is_none())
                            || matches!((new_proto_id, current_proto_id), (Some(a), Some(b)) if a == b);
                        if !same {
                            if !obj.borrow().extensible {
                                return Completion::Throw(interp.create_type_error(
                                    "Object.setPrototypeOf called on non-extensible object",
                                ));
                            }
                            // Circular check
                            if let Some(new_pid) = new_proto_id {
                                let mut p_id = Some(new_pid);
                                while let Some(pid) = p_id {
                                    if pid == target_id {
                                        return Completion::Throw(interp.create_type_error(
                                            "Cyclic __proto__ value",
                                        ));
                                    }
                                    if let Some(p_obj) = interp.get_object(pid) {
                                        if p_obj.borrow().is_proxy() {
                                            break;
                                        }
                                        p_id = p_obj.borrow().prototype_id;
                                    } else {
                                        break;
                                    }
                                }
                            }
                        }
                        if proto.is_null() {
                            obj.borrow_mut().prototype_id = None;
                        } else if let Some(proto_id) = proto.as_object_id() {
                            obj.borrow_mut().prototype_id = Some(proto_id);
                        }
                    }
                    Completion::Normal(target)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("setPrototypeOf".to_string(), set_proto_fn);

            // Object.defineProperties
            let def_props_fn = self.create_function(JsFunction::native(
                "defineProperties".to_string(),
                2,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    if !(target).is_object() {
                        return Completion::Throw(interp.create_type_error(
                            "Object.defineProperties called on non-object",
                        ));
                    }
                    let descs_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                    let descs = match interp.to_object(&descs_arg) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => return Completion::Normal(target),
                    };
                    if let Some(target_id) = target.as_object_id()
                        && let Some(d_id) = descs.as_object_id()
                    {
                        // Use proxy_own_keys to call ownKeys trap if present
                        let all_keys = match interp.proxy_own_keys(d_id) {
                            Ok(k) => k,
                            Err(e) => return Completion::Throw(e),
                        };
                        // Collect all descriptors first, calling [[GetOwnProperty]] per key
                        let mut descriptors: Vec<(JsPropertyKey, PropertyDescriptor)> = Vec::new();
                        for key_val in all_keys {
                            let key = to_property_key_string(&key_val);
                            // Call [[GetOwnProperty]] to check enumerability
                            let desc_val = match interp.proxy_get_own_property_descriptor(d_id, &key) {
                                Ok(v) => v,
                                Err(e) => return Completion::Throw(e),
                            };
                            if (desc_val).is_undefined() { continue; }
                            // Only process enumerable properties (spec: ObjectDefineProperties)
                            if let Ok(desc) = interp.to_property_descriptor(&desc_val)
                                && desc.enumerable == Some(false) { continue; }
                            let prop_desc_val = match interp.get_object_property(d_id, &key, &descs) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => continue,
                            };
                            match interp.to_property_descriptor(&prop_desc_val) {
                                Ok(desc) => descriptors.push((key, desc)),
                                Err(Some(e)) => return Completion::Throw(e),
                                Err(None) => {}
                            }
                        }
                        // Apply all descriptors via proxy-aware defineOwnProperty
                        for (key, desc) in descriptors {
                            let desc_val = interp.from_property_descriptor(&desc);
                            match interp.proxy_define_own_property(target_id, key, &desc_val) {
                                Ok(true) => {}
                                Ok(false) => {
                                    return Completion::Throw(interp.create_type_error(
                                        "Cannot define property, object is not extensible or property is non-configurable",
                                    ));
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                    }
                    Completion::Normal(target)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("defineProperties".to_string(), def_props_fn);

            // Object.getOwnPropertyDescriptors
            let get_descs_fn = self.create_function(JsFunction::native(
                "getOwnPropertyDescriptors".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    // §22.1.2.8 step 1: RequireObjectCoercible then ToObject
                    if (target).is_nullish() {
                        return Completion::Throw(
                            interp.create_type_error("Cannot convert undefined or null to object"),
                        );
                    }
                    let obj_val = match interp.to_object(&target) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if let Some(obj_id) = obj_val.as_object_id() {
                        // Use proxy_own_keys to invoke ownKeys trap (proxy-aware)
                        let all_keys = match interp.proxy_own_keys(obj_id) {
                            Ok(k) => k,
                            Err(e) => return Completion::Throw(e),
                        };
                        let result_id = interp.create_object_id();
                        for key_val in all_keys {
                            let key = to_property_key_string(&key_val);
                            // Use proxy_get_own_property_descriptor to invoke trap
                            let desc_val =
                                match interp.proxy_get_own_property_descriptor(obj_id, &key) {
                                    Ok(v) => v,
                                    Err(e) => return Completion::Throw(e),
                                };
                            if !(desc_val).is_undefined() {
                                interp
                                    .get_object_cell_expect(result_id)
                                    .borrow_mut()
                                    .insert_value(key, desc_val);
                            }
                        }
                        let id = result_id;
                        return Completion::Normal(JsValue::object(id));
                    }
                    // Primitive wrapped to object with no own properties
                    let result_id = interp.create_object_id();
                    let id = result_id;
                    Completion::Normal(JsValue::object(id))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("getOwnPropertyDescriptors".to_string(), get_descs_fn);

            // Object.fromEntries
            let from_entries_fn = self.create_function(JsFunction::native(
                "fromEntries".to_string(),
                1,
                |interp, _this, args| {
                    let iterable = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    interp.with_gc_root_scope(|interp| {
                        let obj_id = interp.create_object_id();
                        let obj_val = JsValue::object(obj_id);
                        interp.gc_root_value(&obj_val);
                        let iterator = match interp.get_iterator(&iterable) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };
                        interp.gc_root_value(&iterator);
                        loop {
                            let step = match interp.iterator_step(&iterator) {
                                Ok(Some(result)) => result,
                                Ok(None) => break,
                                Err(e) => return Completion::Throw(e),
                            };
                            interp.gc_root_value(&step);
                            let next_item = match interp.iterator_value(&step) {
                                Ok(v) => v,
                                Err(e) => {
                                    interp.gc_unroot_value(&step);
                                    return Completion::Throw(e);
                                }
                            };
                            interp.gc_unroot_value(&step);
                            interp.gc_root_value(&next_item);
                            // Step d: If Type(nextItem) is not Object, close and throw TypeError
                            let Some(item_id) = next_item.as_object_id() else {
                                let err =
                                    interp.create_type_error("Iterator value is not an object");
                                interp.iterator_close(&iterator, err.clone());
                                return Completion::Throw(err);
                            };
                            // Step e: Get key from entry[0]
                            let key_raw = match interp.get_object_property(item_id, "0", &next_item)
                            {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => {
                                    interp.iterator_close(&iterator, e.clone());
                                    return Completion::Throw(e);
                                }
                                _ => JsValue::UNDEFINED,
                            };
                            interp.gc_root_value(&key_raw);
                            // Step g: Get value from entry[1]
                            let value = match interp.get_object_property(item_id, "1", &next_item) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => {
                                    interp.iterator_close(&iterator, e.clone());
                                    return Completion::Throw(e);
                                }
                                _ => JsValue::UNDEFINED,
                            };
                            interp.gc_root_value(&value);
                            // Step f: ToPropertyKey(key)
                            let key = match interp.to_property_key(&key_raw) {
                                Ok(k) => k,
                                Err(e) => {
                                    interp.iterator_close(&iterator, e.clone());
                                    return Completion::Throw(e);
                                }
                            };
                            interp.gc_unroot_value(&value);
                            interp.gc_unroot_value(&key_raw);
                            interp.gc_unroot_value(&next_item);
                            if let Some(obj_data) = interp.get_object_cell(obj_id) {
                                obj_data.borrow_mut().insert_value(key, value);
                            }
                        }
                        Completion::Normal(obj_val)
                    })
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("fromEntries".to_string(), from_entries_fn);
        }
    }
}
