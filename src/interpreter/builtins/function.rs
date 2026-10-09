use super::super::*;

impl Interpreter {
    pub(super) fn setup_function_prototype(&mut self, obj_proto_id: u64) {
        self.add_function_prototype_call_and_apply(obj_proto_id);
        self.add_function_prototype_bind(obj_proto_id);
        self.add_function_prototype_to_string(obj_proto_id);
    }

    fn add_function_prototype_call_and_apply(&mut self, obj_proto_id: u64) {
        let fn_proto_realm_id = self.current_realm_id;
        // Add call to Object.prototype (simplified - applies to all functions via prototype chain)
        let call_fn = self.create_function(JsFunction::native(
            "call".to_string(),
            1,
            |interp, _this, args| {
                let this_arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let call_args = if args.len() > 1 { &args[1..] } else { &[] };
                interp.call_function(_this, &this_arg, call_args)
            },
        ));
        self.get_object_cell_expect(obj_proto_id)
            .borrow_mut()
            .insert_builtin("call".to_string(), call_fn);

        // Add apply
        let apply_fn = self.create_function(JsFunction::native(
            "apply".to_string(),
            2,
            move |interp, _this, args| {
                if !interp.is_callable(_this) {
                    return Completion::Throw(interp.create_error_in_realm(
                        fn_proto_realm_id,
                        "TypeError",
                        "Function.prototype.apply called on non-callable",
                    ));
                }
                let this_arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let arr_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                let call_args = if arr_arg.is_nullish() {
                    vec![]
                } else if let Some(array_like_id) = arr_arg.as_object_id() {
                    let len_val =
                        match interp.get_object_property(array_like_id, "length", &arr_arg) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => JsValue::UNDEFINED,
                        };
                    let len_num = match interp.to_number_value(&len_val) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let len = if len_num.is_nan() || len_num <= 0.0 {
                        0usize
                    } else {
                        (len_num.min(9007199254740991.0).floor()) as usize
                    };
                    let mut list = Vec::with_capacity(len);
                    for i in 0..len {
                        match interp.get_object_property(array_like_id, &i.to_string(), &arr_arg) {
                            Completion::Normal(v) => list.push(v),
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => list.push(JsValue::UNDEFINED),
                        }
                    }
                    list
                } else {
                    return Completion::Throw(interp.create_error_in_realm(
                        fn_proto_realm_id,
                        "TypeError",
                        "CreateListFromArrayLike called on non-object",
                    ));
                };
                interp.call_function(_this, &this_arg, &call_args)
            },
        ));
        self.get_object_cell_expect(obj_proto_id)
            .borrow_mut()
            .insert_builtin("apply".to_string(), apply_fn);
    }

    fn add_function_prototype_bind(&mut self, obj_proto_id: u64) {
        // Function.prototype.bind
        let bind_fn = self.create_function(JsFunction::native(
            "bind".to_string(),
            1,
            |interp, this_val, args: &[JsValue]| {
                if !(this_val).is_object() {
                    return Completion::Throw(
                        interp.create_type_error("Bind must be called on a function"),
                    );
                }
                // Check if target is callable
                let is_callable = if let Some(target_id) = this_val.as_object_id()
                    && let Some(obj) = interp.get_object_cell(target_id)
                {
                    obj.borrow().callable.is_some()
                } else {
                    false
                };
                if !is_callable {
                    return Completion::Throw(
                        interp.create_type_error("Bind must be called on a function"),
                    );
                }

                let bind_this = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let bound_args: Vec<JsValue> = args.iter().skip(1).cloned().collect();

                // Spec §20.2.3.2: HasOwnProperty(Target, "length"), then Get, then type check
                // For proxy targets, use invoke_proxy_trap to trigger getOwnPropertyDescriptor
                let target_length_f64: f64 = if let Some(target_id) = this_val.as_object_id() {
                    let is_proxy = interp
                        .get_object_cell(target_id)
                        .is_some_and(|obj| obj.borrow().is_proxy());
                    let has_own_length = if is_proxy {
                        match interp.invoke_proxy_trap(
                            target_id,
                            "getOwnPropertyDescriptor",
                            vec![
                                interp.get_proxy_target_val(target_id),
                                JsValue::string(crate::types::JsString::from_str("length")),
                            ],
                        ) {
                            Ok(Some(v)) => !(v).is_undefined(),
                            Ok(None) => {
                                let target_val = interp.get_proxy_target_val(target_id);
                                if let Some(proxy_target_id) = target_val.as_object_id()
                                    && let Some(target_obj) =
                                        interp.get_object_cell(proxy_target_id)
                                {
                                    target_obj.borrow().get_own_property("length").is_some()
                                } else {
                                    false
                                }
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    } else if let Some(obj) = interp.get_object_cell(target_id) {
                        obj.borrow().get_own_property("length").is_some()
                    } else {
                        false
                    };
                    if has_own_length {
                        match interp.get_object_property(target_id, "length", this_val) {
                            Completion::Normal(value) => value.as_number().map_or(0.0, |n| {
                                let int = to_integer_or_infinity(n);
                                if int < 0.0 { 0.0 } else { int }
                            }),
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => 0.0,
                        }
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };
                let bound_length_f64 = (target_length_f64 - bound_args.len() as f64).max(0.0);
                let bound_length = if bound_length_f64.is_finite() {
                    bound_length_f64 as usize
                } else {
                    0
                };

                // Read target name using getter-aware access
                let target_name = if let Some(target_id) = this_val.as_object_id() {
                    match interp.get_object_property(target_id, "name", this_val) {
                        Completion::Normal(value) => value
                            .as_string()
                            .map_or_else(String::new, |s| s.to_string()),
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => String::new(),
                    }
                } else {
                    String::new()
                };
                let bound_name = format!("bound {}", target_name);

                let is_ctor = interp.is_constructor(this_val);

                // Use Rc<Cell<u64>> so the closure can read the bound function's
                // own object ID at call time (chicken-and-egg: closure goes into
                // the object, so we can't know the ID until after creation).
                let bound_id_cell = Rc::new(std::cell::Cell::new(0u64));
                let id_cell = bound_id_cell.clone();
                let bound = JsFunction::Native(
                    bound_name,
                    bound_length,
                    Rc::new(
                        move |interp2: &mut Interpreter, this: &JsValue, call_args: &[JsValue]| {
                            let obj_id = id_cell.get();
                            let (target, bt, ba) = {
                                let obj = interp2.get_object_cell(obj_id).unwrap();
                                let b = obj.borrow();
                                match b.bound() {
                                    Some(bd) => {
                                        (bd.target.clone(), bd.this.clone(), bd.args.clone())
                                    }
                                    None => (JsValue::UNDEFINED, JsValue::UNDEFINED, Vec::new()),
                                }
                            };
                            let mut all_args = ba;
                            all_args.extend_from_slice(call_args);
                            if interp2.new_target.is_some() {
                                interp2.call_function(&target, this, &all_args)
                            } else {
                                interp2.call_function(&target, &bt, &all_args)
                            }
                        },
                    ),
                    is_ctor,
                );
                let result = interp.create_function(bound);
                if let Some(result_id) = result.as_object_id()
                    && let Some(obj) = interp.get_object_cell(result_id)
                {
                    bound_id_cell.set(result_id);
                    // §20.2.3.2 step 4-5: Set bound function's [[Prototype]] to target's [[Prototype]]
                    if let Some(target_id) = this_val.as_object_id()
                        && let Some(target_obj) = interp.get_object_cell(target_id)
                    {
                        obj.borrow_mut().prototype_id = target_obj.borrow().prototype_id;
                    }
                    // Per spec, bound functions do not have own .prototype property
                    obj.borrow_mut().remove_property("prototype");
                    // Store [[BoundTargetFunction]] / [[BoundThis]] / [[BoundArguments]].
                    let stored_bound_args: Vec<JsValue> = args.iter().skip(1).cloned().collect();
                    obj.borrow_mut().kind = crate::interpreter::types::ObjectKind::BoundFunction(
                        crate::interpreter::types::BoundFunctionData {
                            target: this_val.clone(),
                            this: bind_this.clone(),
                            args: stored_bound_args,
                        },
                    );
                    // Overwrite length with correct f64 value (handles Infinity)
                    obj.borrow_mut().insert_property(
                        "length".to_string(),
                        PropertyDescriptor::data(
                            JsValue::number(bound_length_f64),
                            false,
                            false,
                            true,
                        ),
                    );
                }
                Completion::Normal(result)
            },
        ));
        self.get_object_cell_expect(obj_proto_id)
            .borrow_mut()
            .insert_builtin("bind".to_string(), bind_fn);
    }

    fn add_function_prototype_to_string(&mut self, obj_proto_id: u64) {
        // Function.prototype.toString — §20.2.3.5
        let fn_tostring = self.create_function(JsFunction::native(
            "toString".to_string(),
            0,
            |interp, this_val, _args: &[JsValue]| {
                if let Some(target_id) = this_val.as_object_id()
                    && let Some(obj) = interp.get_object_cell(target_id)
                {
                    let b = obj.borrow();
                    if b.is_proxy() || b.is_proxy_revoked() {
                        if b.is_proxy_revoked() {
                            drop(b);
                            return Completion::Throw(interp.create_type_error(
                                "Function.prototype.toString requires that 'this' be a Function",
                            ));
                        }
                        drop(b);
                        // Only callable proxies return NativeFunction string
                        if interp.is_callable(this_val) {
                            return Completion::Normal(JsValue::string(JsString::from_str(
                                "function () { [native code] }",
                            )));
                        }
                        // Non-callable proxy falls through to TypeError
                    } else {
                        drop(b);
                    }
                    if let Some(ref func) = obj.borrow().callable {
                        let s = match func {
                            JsFunction::User {
                                source_text: Some(text),
                                ..
                            } => text.to_string(),
                            JsFunction::User {
                                name,
                                is_arrow,
                                is_async,
                                is_generator,
                                ..
                            } => {
                                let n = name.clone().unwrap_or_default();
                                if *is_arrow {
                                    "() => { [native code] }".to_string()
                                } else {
                                    let mut prefix = String::new();
                                    if *is_async {
                                        prefix.push_str("async ");
                                    }
                                    if *is_generator {
                                        format!("{prefix}function* {n}() {{ [native code] }}")
                                    } else {
                                        format!("{prefix}function {n}() {{ [native code] }}")
                                    }
                                }
                            }
                            JsFunction::Native(name, _, _, _) => {
                                let sanitized = super::sanitize_native_fn_name(name);
                                format!("function {}() {{ [native code] }}", sanitized)
                            }
                        };
                        return Completion::Normal(JsValue::string(JsString::from_str(&s)));
                    }
                }
                // Step 2: If this is not callable, throw TypeError
                Completion::Throw(interp.create_type_error(
                    "Function.prototype.toString requires that 'this' be a Function",
                ))
            },
        ));
        self.get_object_cell_expect(obj_proto_id)
            .borrow_mut()
            .insert_builtin("toString".to_string(), fn_tostring);
    }
}
