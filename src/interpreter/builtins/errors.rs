use super::super::*;

impl Interpreter {
    pub(super) fn setup_error_builtins(&mut self) {
        let error_prototype = self.setup_error_constructor();
        self.setup_error_is_error();
        self.setup_test262_error(error_prototype.clone());
        self.setup_native_error_constructors(error_prototype.clone());
        self.setup_suppressed_error(error_prototype.clone());
        self.setup_aggregate_error(error_prototype);
    }

    fn setup_error_constructor(&mut self) -> Option<ObjectHandle> {
        // Error constructor
        {
            let error_name = "Error".to_string();
            self.register_global_fn(
                "Error",
                BindingKind::Var,
                JsFunction::constructor(error_name.clone(), 1, move |interp, this, args| {
                    let msg_raw = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let options = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);

                    // OrdinaryCreateFromConstructor — realm-aware prototype
                    let proto = match interp
                        .get_prototype_from_new_target_realm(|realm| realm.error_prototype)
                    {
                        Ok(p) => p,
                        Err(e) => return Completion::Throw(e),
                    };
                    let msg_str = if !(msg_raw).is_undefined() {
                        match interp.to_string_value(&msg_raw) {
                            Ok(s) => Some(JsValue::string(JsString::from_str(&s))),
                            Err(e) => return Completion::Throw(e),
                        }
                    } else {
                        None
                    };
                    // §20.5.8.1 InstallErrorCause — use proxy-aware HasProperty
                    let cause_val = if let Some(options_id) = options.as_object_id() {
                        match interp.proxy_has_property(options_id, "cause") {
                            Ok(true) => {
                                match interp.get_object_property(options_id, "cause", &options) {
                                    Completion::Normal(v) => Some(v),
                                    c => return c,
                                }
                            }
                            Ok(false) => None,
                            Err(e) => return Completion::Throw(e),
                        }
                    } else {
                        None
                    };

                    macro_rules! init_error {
                        ($o:expr) => {
                            $o.class_name = "Error".to_string();
                            if let Some(p) = proto {
                                $o.prototype_id = Some(p);
                            }
                            if let Some(ref ms) = msg_str {
                                $o.insert_builtin("message".to_string(), ms.clone());
                            }
                            if let Some(ref cv) = cause_val {
                                $o.insert_builtin("cause".to_string(), cv.clone());
                            }
                        };
                    }

                    // §20.5.1.1 never reuses `this` — it always allocates via
                    // OrdinaryCreateFromConstructor. A plain [[Call]] (e.g.
                    // `Error.call(obj, msg)`) must not mutate an arbitrary `this`.
                    if interp.new_target.is_some()
                        && let Some(this_id) = this.as_object_id()
                    {
                        if let Some(obj) = interp.get_object_cell(this_id) {
                            let mut o = obj.borrow_mut();
                            init_error!(o);
                        }
                        return Completion::Normal(this.clone());
                    }
                    let obj_id = interp.create_object_id();
                    {
                        let mut o = interp.get_object_cell_expect(obj_id).borrow_mut();
                        init_error!(o);
                    }
                    let id = obj_id;
                    Completion::Normal(JsValue::object(id))
                }),
            );
            // Mark Error constructor as deferred_construct so construct_with_new_target
            // doesn't do an early prototype lookup (the constructor body handles it via
            // get_prototype_from_new_target_realm, per spec §20.5.1.1 step 2).
            if let Some(error_val) = self.get_global_var("Error")
                && let Some(error_id) = error_val.as_object_id()
                && let Some(func_obj) = self.get_object(error_id)
            {
                func_obj.borrow_mut().deferred_construct = true;
            }
        }

        // Get Error.prototype for inheritance
        let error_prototype_id: Option<u64> = {
            if let Some(error_val) = self.get_global_var("Error")
                && let Some(ctor_id) = error_val.as_object_id()
            {
                let proto_val = self.get_property_on_id(ctor_id, "prototype");
                proto_val.as_object_id()
            } else {
                None
            }
        };
        let error_prototype = error_prototype_id.and_then(|id| self.get_object(id));
        self.realm_mut().error_prototype = error_prototype_id;

        // Add toString to Error.prototype_id
        if let Some(ref ep) = error_prototype {
            let tostring_fn = self.create_function(JsFunction::native(
                "toString".to_string(),
                0,
                |interp, this_val, _args| {
                    // §20.5.3.4 step 2: If Type(O) is not Object, throw TypeError
                    if let Some(this_id) = this_val.as_object_id() {
                        let name_val = interp.get_object_property(this_id, "name", this_val);
                        let name = match name_val {
                            Completion::Normal(v) if v.is_undefined() => "Error".to_string(),
                            Completion::Normal(v) => match interp.to_js_string(&v) {
                                Ok(s) => s.to_rust_string(),
                                Err(e) => return Completion::Throw(e),
                            },
                            other => return other,
                        };
                        let msg_val = interp.get_object_property(this_id, "message", this_val);
                        let msg = match msg_val {
                            Completion::Normal(v) if v.is_undefined() => String::new(),
                            Completion::Normal(v) => match interp.to_js_string(&v) {
                                Ok(s) => s.to_rust_string(),
                                Err(e) => return Completion::Throw(e),
                            },
                            other => return other,
                        };
                        return if name.is_empty() {
                            Completion::Normal(JsValue::string(JsString::from_str(&msg)))
                        } else if msg.is_empty() {
                            Completion::Normal(JsValue::string(JsString::from_str(&name)))
                        } else {
                            Completion::Normal(JsValue::string(JsString::from_str(&format!(
                                "{name}: {msg}"
                            ))))
                        };
                    }
                    Completion::Throw(interp.create_type_error(
                        "Error.prototype.toString requires that 'this' be an Object",
                    ))
                },
            ));
            ep.borrow_mut()
                .insert_builtin("toString".to_string(), tostring_fn);
            ep.borrow_mut().insert_builtin(
                "name".to_string(),
                JsValue::string(JsString::from_str("Error")),
            );
            ep.borrow_mut().insert_builtin(
                "message".to_string(),
                JsValue::string(JsString::from_str("")),
            );

            // error-stack-accessor: Error.prototype.stack is an own accessor of
            // %Error.prototype% only; NativeError/AggregateError/SuppressedError
            // prototypes inherit it. §sec-get/set-error.prototype.stack.
            let stack_getter = self.create_function(JsFunction::native(
                "get stack".to_string(),
                0,
                |interp, this_val, _args| {
                    // 2. If E is not an Object, throw a TypeError exception.
                    let Some(o_id) = this_val.as_object_id() else {
                        return Completion::Throw(interp.create_type_error(
                            "Error.prototype.stack getter called on non-object",
                        ));
                    };
                    // 3. If E does not have an [[ErrorData]] internal slot,
                    //    return undefined. [[ErrorData]] is modeled as
                    //    class_name.contains("Error") (same as Error.isError);
                    //    read directly off the receiver so Proxy traps don't fire.
                    let has_error_data = interp
                        .get_object(o_id)
                        .is_some_and(|obj| obj.borrow().class_name.contains("Error"));
                    if !has_error_data {
                        return Completion::Normal(JsValue::UNDEFINED);
                    }
                    // 4. Return an implementation-defined trace string.
                    Completion::Normal(JsValue::string(JsString::from_str("")))
                },
            ));
            // Capture %Error.prototype% per-realm for the SameValue check.
            let stack_home_id = error_prototype_id;
            let stack_setter = self.create_function(JsFunction::native(
                "set stack".to_string(),
                1,
                move |interp, this_val, args| {
                    let v = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    // 2. If E is not an Object, throw a TypeError exception.
                    let Some(o_id) = this_val.as_object_id() else {
                        return Completion::Throw(interp.create_type_error(
                            "Error.prototype.stack setter called on non-object",
                        ));
                    };
                    // 3. If v is not a String, throw a TypeError exception.
                    //    (A String wrapper object is not a String value.)
                    if !(v).is_string() {
                        return Completion::Throw(interp.create_type_error(
                            "Error.prototype.stack setter requires a string value",
                        ));
                    }
                    // SetterThatIgnoresPrototypeProperties:
                    // 2. If SameValue(this, home) is true, throw a TypeError.
                    if Some(o_id) == stack_home_id {
                        return Completion::Throw(interp.create_type_error(
                            "Error.prototype.stack setter cannot set on %Error.prototype%",
                        ));
                    }
                    // 3. desc = this.[[GetOwnProperty]]("stack") — route through
                    //    the real internal methods so Proxy traps fire.
                    let is_proxy = interp
                        .get_object_cell(o_id)
                        .is_some_and(|cell| cell.borrow().is_proxy());
                    if is_proxy {
                        let desc = match interp.proxy_get_own_property_descriptor(o_id, "stack") {
                            Ok(d) => d,
                            Err(e) => return Completion::Throw(e),
                        };
                        if (desc).is_undefined() {
                            // 4a. CreateDataPropertyOrThrow (fires defineProperty trap).
                            return match super::array::create_data_property_or_throw(
                                interp, this_val, "stack", v,
                            ) {
                                Ok(()) => Completion::Normal(JsValue::UNDEFINED),
                                Err(e) => Completion::Throw(e),
                            };
                        }
                        // 5a. Set(this, "stack", v, true) (fires set trap).
                        return match interp.set_object_property(o_id, "stack", v, this_val) {
                            Ok(true) => Completion::Normal(JsValue::UNDEFINED),
                            Ok(false) => Completion::Throw(
                                interp.create_type_error("Cannot set property 'stack'"),
                            ),
                            Err(e) => Completion::Throw(e),
                        };
                    }
                    let own = interp
                        .get_object_cell(o_id)
                        .and_then(|cell| cell.borrow().get_own_property("stack"));
                    match own {
                        // 4a. CreateDataPropertyOrThrow.
                        None => {
                            match super::array::create_data_property_or_throw(
                                interp, this_val, "stack", v,
                            ) {
                                Ok(()) => Completion::Normal(JsValue::UNDEFINED),
                                Err(e) => Completion::Throw(e),
                            }
                        }
                        // 5a. Set(this, "stack", v, true) with receiver == this.
                        Some(_) => match interp.set_object_property(o_id, "stack", v, this_val) {
                            Ok(true) => Completion::Normal(JsValue::UNDEFINED),
                            Ok(false) => Completion::Throw(
                                interp.create_type_error("Cannot set property 'stack'"),
                            ),
                            Err(e) => Completion::Throw(e),
                        },
                    }
                },
            ));
            ep.borrow_mut().insert_property(
                "stack".to_string(),
                PropertyDescriptor::accessor(Some(stack_getter), Some(stack_setter), false, true),
            );

            // Set constructor on Error.prototype_id
            if let Some(error_ctor) = self.get_global_var("Error") {
                ep.borrow_mut()
                    .insert_builtin("constructor".to_string(), error_ctor);
            }
        }
        error_prototype
    }

    fn setup_error_is_error(&mut self) {
        // Error.isError() static method
        {
            let is_error_fn = self.create_function(JsFunction::native(
                "isError".to_string(),
                1,
                |interp, _this, args| {
                    let arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    if let Some(arg_id) = arg.as_object_id()
                        && let Some(obj) = interp.get_object(arg_id)
                    {
                        let cn = &obj.borrow().class_name;
                        if cn.contains("Error") {
                            return Completion::Normal(JsValue::boolean(true));
                        }
                    }
                    Completion::Normal(JsValue::boolean(false))
                },
            ));
            if let Some(error_ctor) = self.get_global_var("Error")
                && let Some(error_ctor_id) = error_ctor.as_object_id()
                && let Some(obj) = self.get_object(error_ctor_id)
            {
                obj.borrow_mut()
                    .insert_builtin("isError".to_string(), is_error_fn);
            }
        }
    }

    fn setup_test262_error(&mut self, error_prototype: Option<ObjectHandle>) {
        // Test262Error
        {
            let error_proto_clone = error_prototype.clone();
            self.register_global_fn(
                "Test262Error",
                BindingKind::Var,
                JsFunction::constructor(
                    "Test262Error".to_string(),
                    1,
                    move |interp, this, args| {
                        let msg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        // Only box into `this` when invoked via [[Construct]] — a plain
                        // [[Call]] (e.g. `Test262Error.call(obj, msg)`) must not mutate
                        // an arbitrary `this`.
                        if interp.new_target.is_some()
                            && let Some(this_id) = this.as_object_id()
                        {
                            if let Some(obj) = interp.get_object(this_id) {
                                let mut o = obj.borrow_mut();
                                o.class_name = "Test262Error".to_string();
                                if let Some(ref ep) = error_proto_clone {
                                    o.prototype_id = Some(ep.borrow().id.unwrap());
                                }
                                if !(msg).is_undefined() {
                                    o.insert_builtin("message".to_string(), msg);
                                }
                                o.insert_builtin(
                                    "name".to_string(),
                                    JsValue::string(JsString::from_str("Test262Error")),
                                );
                            }
                            return Completion::Normal(this.clone());
                        }
                        let obj_id = interp.create_object_id();
                        {
                            let mut o = interp.get_object_cell_expect(obj_id).borrow_mut();
                            o.class_name = "Test262Error".to_string();
                            if let Some(ref ep) = error_proto_clone {
                                o.prototype_id = Some(ep.borrow().id.unwrap());
                            }
                            if !(msg).is_undefined() {
                                o.insert_builtin("message".to_string(), msg);
                            }
                            o.insert_builtin(
                                "name".to_string(),
                                JsValue::string(JsString::from_str("Test262Error")),
                            );
                        }
                        let id = obj_id;
                        Completion::Normal(JsValue::object(id))
                    },
                ),
            );
        }
    }

    fn setup_native_error_constructors(&mut self, error_prototype: Option<ObjectHandle>) {
        // Error subtype constructors
        for name in [
            "SyntaxError",
            "TypeError",
            "ReferenceError",
            "RangeError",
            "URIError",
            "EvalError",
        ] {
            let error_name = name.to_string();

            // Create per-type prototype inheriting from Error.prototype_id
            let native_proto_id = self.create_object_id();
            if let Some(ref ep) = error_prototype {
                self.get_object_cell_expect(native_proto_id)
                    .borrow_mut()
                    .prototype_id = Some(ep.borrow().id.unwrap());
            }
            self.get_object_cell_expect(native_proto_id)
                .borrow_mut()
                .insert_builtin(
                    "name".to_string(),
                    JsValue::string(JsString::from_str(name)),
                );
            self.get_object_cell_expect(native_proto_id)
                .borrow_mut()
                .insert_builtin(
                    "message".to_string(),
                    JsValue::string(JsString::from_str("")),
                );

            // Store native error prototype on realm
            match name {
                "SyntaxError" => self.realm_mut().syntax_error_prototype = Some(native_proto_id),
                "TypeError" => self.realm_mut().type_error_prototype = Some(native_proto_id),
                "ReferenceError" => {
                    self.realm_mut().reference_error_prototype = Some(native_proto_id)
                }
                "RangeError" => self.realm_mut().range_error_prototype = Some(native_proto_id),
                "URIError" => self.realm_mut().uri_error_prototype = Some(native_proto_id),
                "EvalError" => self.realm_mut().eval_error_prototype = Some(native_proto_id),
                _ => {}
            }

            let native_proto_clone_id = native_proto_id;
            let error_name_clone = error_name.clone();
            self.register_global_fn(
                name,
                BindingKind::Var,
                JsFunction::constructor(error_name.clone(), 1, move |interp, this, args| {
                    let msg_raw = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let options = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);

                    // OrdinaryCreateFromConstructor — realm-aware prototype
                    let proto = match interp.get_prototype_from_new_target_realm(|realm| {
                        match error_name_clone.as_str() {
                            "SyntaxError" => realm.syntax_error_prototype,
                            "TypeError" => realm.type_error_prototype,
                            "ReferenceError" => realm.reference_error_prototype,
                            "RangeError" => realm.range_error_prototype,
                            "URIError" => realm.uri_error_prototype,
                            "EvalError" => realm.eval_error_prototype,
                            _ => None,
                        }
                    }) {
                        Ok(p) => p.unwrap_or(native_proto_clone_id),
                        Err(e) => return Completion::Throw(e),
                    };

                    let msg_str = if !(msg_raw).is_undefined() {
                        match interp.to_string_value(&msg_raw) {
                            Ok(s) => Some(JsValue::string(JsString::from_str(&s))),
                            Err(e) => return Completion::Throw(e),
                        }
                    } else {
                        None
                    };
                    // §20.5.8.1 InstallErrorCause — use proxy-aware HasProperty
                    let cause_val = if let Some(options_id) = options.as_object_id() {
                        match interp.proxy_has_property(options_id, "cause") {
                            Ok(true) => {
                                match interp.get_object_property(options_id, "cause", &options) {
                                    Completion::Normal(v) => Some(v),
                                    c => return c,
                                }
                            }
                            Ok(false) => None,
                            Err(e) => return Completion::Throw(e),
                        }
                    } else {
                        None
                    };

                    macro_rules! init_native_error {
                        ($o:expr) => {
                            $o.class_name = error_name_clone.clone();
                            $o.prototype_id = Some(proto);
                            if let Some(ref ms) = msg_str {
                                $o.insert_builtin("message".to_string(), ms.clone());
                            }
                            if let Some(ref cv) = cause_val {
                                $o.insert_builtin("cause".to_string(), cv.clone());
                            }
                        };
                    }

                    // §20.5.6.1.1 never reuses `this` — it always allocates via
                    // OrdinaryCreateFromConstructor. A plain [[Call]] (e.g.
                    // `TypeError.call(obj, msg)`) must not mutate an arbitrary `this`.
                    if interp.new_target.is_some()
                        && let Some(this_id) = this.as_object_id()
                    {
                        if let Some(obj) = interp.get_object(this_id) {
                            let mut o = obj.borrow_mut();
                            init_native_error!(o);
                        }
                        return Completion::Normal(this.clone());
                    }
                    let obj_id = interp.create_object_id();
                    {
                        let mut o = interp.get_object_cell_expect(obj_id).borrow_mut();
                        init_native_error!(o);
                    }
                    let id = obj_id;
                    Completion::Normal(JsValue::object(id))
                }),
            );

            // Set constructor on the per-type prototype
            if let Some(ctor_val) = self.get_global_var(name) {
                self.get_object_cell_expect(native_proto_id)
                    .borrow_mut()
                    .insert_builtin("constructor".to_string(), ctor_val.clone());
            }
            // Set constructor's .prototype to the per-type prototype
            {
                if let Some(ctor_val) = self.get_global_var(name)
                    && let Some(ctor_id) = ctor_val.as_object_id()
                    && let Some(ctor_obj) = self.get_object(ctor_id)
                {
                    let proto_id = native_proto_id;
                    ctor_obj.borrow_mut().insert_property(
                        "prototype".to_string(),
                        PropertyDescriptor::data(JsValue::object(proto_id), false, false, false),
                    );
                    ctor_obj.borrow_mut().deferred_construct = true;
                }
            }
        }
    }

    fn setup_suppressed_error(&mut self, error_prototype: Option<ObjectHandle>) {
        // SuppressedError constructor
        {
            let suppressed_proto_id = self.create_object_id();
            if let Some(ref ep) = error_prototype {
                self.get_object_cell_expect(suppressed_proto_id)
                    .borrow_mut()
                    .prototype_id = Some(ep.borrow().id.unwrap());
            }
            self.get_object_cell_expect(suppressed_proto_id)
                .borrow_mut()
                .insert_builtin(
                    "name".to_string(),
                    JsValue::string(JsString::from_str("SuppressedError")),
                );
            self.get_object_cell_expect(suppressed_proto_id)
                .borrow_mut()
                .insert_builtin(
                    "message".to_string(),
                    JsValue::string(JsString::from_str("")),
                );

            self.realm_mut().suppressed_error_prototype = Some(suppressed_proto_id);

            let suppressed_ctor = self.create_function(JsFunction::constructor(
                "SuppressedError".to_string(),
                3,
                move |interp, _this, args| {
                    let error_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let suppressed_val = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                    let msg_raw = args.get(2).cloned().unwrap_or(JsValue::UNDEFINED);

                    // OrdinaryCreateFromConstructor — realm-aware prototype
                    let proto = match interp.get_prototype_from_new_target_realm(|realm| {
                        realm.suppressed_error_prototype
                    }) {
                        Ok(p) => p,
                        Err(e) => return Completion::Throw(e),
                    };
                    // Extract msg_str BEFORE the mut borrow on obj's RefCell
                    // (lifetime is tied to &interp; can't hold across &mut interp).
                    let msg_str = if !(msg_raw).is_undefined() {
                        match interp.to_string_value(&msg_raw) {
                            Ok(s) => Some(JsValue::string(JsString::from_str(&s))),
                            Err(e) => return Completion::Throw(e),
                        }
                    } else {
                        None
                    };
                    let obj_id = interp.create_object_id();
                    {
                        let mut o = interp.get_object_cell_expect(obj_id).borrow_mut();
                        o.class_name = "SuppressedError".to_string();
                        if let Some(p) = proto {
                            o.prototype_id = Some(p);
                        }
                        // Per spec: message first, then error, then suppressed
                        if let Some(msg_str) = msg_str {
                            o.insert_builtin("message".to_string(), msg_str);
                        }
                        o.insert_builtin("error".to_string(), error_val.clone());
                        o.insert_builtin("suppressed".to_string(), suppressed_val.clone());
                    }
                    let id = obj_id;
                    Completion::Normal(JsValue::object(id))
                },
            ));

            self.get_object_cell_expect(suppressed_proto_id)
                .borrow_mut()
                .insert_builtin("constructor".to_string(), suppressed_ctor.clone());

            if let Some(ctor_id) = suppressed_ctor.as_object_id()
                && let Some(ctor_obj) = self.get_object(ctor_id)
            {
                let proto_id = suppressed_proto_id;
                ctor_obj.borrow_mut().insert_property(
                    "prototype".to_string(),
                    PropertyDescriptor::data(JsValue::object(proto_id), false, false, false),
                );
            }

            let global_env = self.realm().global_env.clone();
            global_env
                .borrow_mut()
                .declare("SuppressedError", BindingKind::Var);
            let _ = self.env_set(&global_env, "SuppressedError", suppressed_ctor);
        }
    }

    fn setup_aggregate_error(&mut self, error_prototype: Option<ObjectHandle>) {
        // AggregateError constructor
        {
            let agg_proto_id = self.create_object_id();
            if let Some(ref ep) = error_prototype {
                self.get_object_cell_expect(agg_proto_id)
                    .borrow_mut()
                    .prototype_id = Some(ep.borrow().id.unwrap());
            }
            self.get_object_cell_expect(agg_proto_id)
                .borrow_mut()
                .insert_builtin(
                    "name".to_string(),
                    JsValue::string(JsString::from_str("AggregateError")),
                );
            self.get_object_cell_expect(agg_proto_id)
                .borrow_mut()
                .insert_builtin(
                    "message".to_string(),
                    JsValue::string(JsString::from_str("")),
                );
            let agg_proto_clone_id = agg_proto_id;
            self.realm_mut().aggregate_error_prototype = Some(agg_proto_id);
            self.register_global_fn(
                "AggregateError",
                BindingKind::Var,
                JsFunction::constructor(
                    "AggregateError".to_string(),
                    2,
                    move |interp, this, args| {
                        let errors_arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        let msg_raw = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                        let options = args.get(2).cloned().unwrap_or(JsValue::UNDEFINED);

                        // OrdinaryCreateFromConstructor — realm-aware prototype
                        let proto = match interp.get_prototype_from_new_target_realm(|realm| {
                            realm.aggregate_error_prototype
                        }) {
                            Ok(p) => p.unwrap_or(agg_proto_clone_id),
                            Err(e) => return Completion::Throw(e),
                        };

                        // §20.5.7.1 step 3: ToString(message) BEFORE iterating errors
                        let msg_str = if !(msg_raw).is_undefined() {
                            match interp.to_string_value(&msg_raw) {
                                Ok(s) => Some(JsValue::string(JsString::from_str(&s))),
                                Err(e) => return Completion::Throw(e),
                            }
                        } else {
                            None
                        };

                        // §20.5.7.1 step 6: IteratorToList(GetIterator(errors))
                        let errors_vec = match interp.iterate_to_vec(&errors_arg) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };
                        let errors_arr = interp.create_array(errors_vec);
                        // §20.5.8.1 InstallErrorCause — use proxy-aware HasProperty
                        let cause_val = if let Some(options_id) = options.as_object_id() {
                            match interp.proxy_has_property(options_id, "cause") {
                                Ok(true) => {
                                    match interp.get_object_property(options_id, "cause", &options)
                                    {
                                        Completion::Normal(v) => Some(v),
                                        c => return c,
                                    }
                                }
                                Ok(false) => None,
                                Err(e) => return Completion::Throw(e),
                            }
                        } else {
                            None
                        };

                        macro_rules! init_agg_error {
                            ($o:expr) => {
                                $o.class_name = "AggregateError".to_string();
                                $o.prototype_id = Some(proto);
                                $o.insert_builtin("errors".to_string(), errors_arr.clone());
                                if let Some(ref ms) = msg_str {
                                    $o.insert_builtin("message".to_string(), ms.clone());
                                }
                                if let Some(ref cv) = cause_val {
                                    $o.insert_builtin("cause".to_string(), cv.clone());
                                }
                            };
                        }

                        // §20.5.7.1 never reuses `this` — it always allocates via
                        // OrdinaryCreateFromConstructor. A plain [[Call]] (e.g.
                        // `AggregateError.call(obj, errors, msg)`) must not mutate an
                        // arbitrary `this`.
                        if interp.new_target.is_some()
                            && let Some(this_id) = this.as_object_id()
                        {
                            if let Some(obj) = interp.get_object(this_id) {
                                let mut o = obj.borrow_mut();
                                init_agg_error!(o);
                            }
                            return Completion::Normal(this.clone());
                        }
                        let obj_id = interp.create_object_id();
                        {
                            let mut o = interp.get_object_cell_expect(obj_id).borrow_mut();
                            init_agg_error!(o);
                        }
                        let id = obj_id;
                        Completion::Normal(JsValue::object(id))
                    },
                ),
            );
            {
                if let Some(ctor_val) = self.get_global_var("AggregateError") {
                    self.get_object_cell_expect(agg_proto_id)
                        .borrow_mut()
                        .insert_builtin("constructor".to_string(), ctor_val);
                }
            }
            {
                if let Some(ctor_val) = self.get_global_var("AggregateError")
                    && let Some(ctor_id) = ctor_val.as_object_id()
                    && let Some(ctor_obj) = self.get_object(ctor_id)
                {
                    let proto_id = agg_proto_id;
                    ctor_obj.borrow_mut().insert_property(
                        "prototype".to_string(),
                        PropertyDescriptor::data(JsValue::object(proto_id), false, false, false),
                    );
                }
            }
        }
    }
}
