use super::*;
use std::cell::Cell;

/// PromiseCapability record: {promise, resolve, reject}
pub(crate) struct PromiseCapability {
    pub promise: JsValue,
    pub resolve: JsValue,
    pub reject: JsValue,
}

impl Interpreter {
    /// NewPromiseCapability(C) - spec 27.2.1.5
    pub(crate) fn new_promise_capability(
        &mut self,
        constructor: &JsValue,
    ) -> Result<PromiseCapability, JsValue> {
        if !self.is_constructor(constructor) {
            return Err(
                self.create_type_error("Promise resolve or reject function is not callable")
            );
        }

        // Check if C is the built-in Promise constructor - fast path
        let promise_ctor = self.global_env.borrow().get("Promise");
        if let Some(ref ctor_val) = promise_ctor
            && same_value(constructor, ctor_val)
        {
            let promise = self.create_promise_object();
            let promise_id = if let JsValue::Object(ref o) = promise {
                o.id
            } else {
                0
            };
            let (resolve, reject) = self.create_resolving_functions(promise_id);
            return Ok(PromiseCapability {
                promise,
                resolve,
                reject,
            });
        }

        // General case: call new C(executor) where executor captures resolve/reject
        let resolve_slot: Rc<RefCell<JsValue>> = Rc::new(RefCell::new(JsValue::Undefined));
        let reject_slot: Rc<RefCell<JsValue>> = Rc::new(RefCell::new(JsValue::Undefined));

        let rs = resolve_slot.clone();
        let rj = reject_slot.clone();
        let executor = self.create_function(JsFunction::native(
            "".to_string(),
            2,
            move |interp, _this, args| {
                let resolve_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let reject_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);

                // Spec: If promiseCapability.[[Resolve]] is not undefined, throw TypeError
                if !matches!(*rs.borrow(), JsValue::Undefined) {
                    return Completion::Throw(
                        interp.create_type_error("Promise executor has already been resolved"),
                    );
                }
                // Spec: If promiseCapability.[[Reject]] is not undefined, throw TypeError
                if !matches!(*rj.borrow(), JsValue::Undefined) {
                    return Completion::Throw(
                        interp.create_type_error("Promise executor has already been resolved"),
                    );
                }

                // Spec: Set promiseCapability.[[Resolve]] to resolve
                *rs.borrow_mut() = resolve_arg;
                // Spec: Set promiseCapability.[[Reject]] to reject
                *rj.borrow_mut() = reject_arg;

                Completion::Normal(JsValue::Undefined)
            },
        ));

        let promise = match self.construct(constructor, &[executor]) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => return Err(self.create_type_error("Promise constructor returned abnormally")),
        };

        let resolve = resolve_slot.borrow().clone();
        let reject = reject_slot.borrow().clone();

        if !self.is_callable(&resolve) {
            return Err(self.create_type_error("Promise resolve function is not callable"));
        }
        if !self.is_callable(&reject) {
            return Err(self.create_type_error("Promise reject function is not callable"));
        }

        Ok(PromiseCapability {
            promise,
            resolve,
            reject,
        })
    }

    /// SpeciesConstructor(O, defaultConstructor) - spec 7.3.22
    pub(crate) fn species_constructor(
        &mut self,
        obj: &JsValue,
        default_ctor: &JsValue,
    ) -> Result<JsValue, JsValue> {
        let obj_id = if let JsValue::Object(o) = obj {
            o.id
        } else {
            return Ok(default_ctor.clone());
        };

        // Get O.constructor
        let ctor = match self.get_object_property(obj_id, "constructor", obj) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => return Ok(default_ctor.clone()),
        };

        if matches!(ctor, JsValue::Undefined) {
            return Ok(default_ctor.clone());
        }

        if !matches!(ctor, JsValue::Object(_)) {
            return Err(self.create_type_error("Species constructor is not an object"));
        }

        // Get constructor[Symbol.species]
        let ctor_id = if let JsValue::Object(o) = &ctor {
            o.id
        } else {
            return Ok(default_ctor.clone());
        };
        let species = match self.get_object_property(ctor_id, "Symbol(Symbol.species)", &ctor) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => return Ok(default_ctor.clone()),
        };

        if matches!(species, JsValue::Undefined | JsValue::Null) {
            return Ok(default_ctor.clone());
        }

        if self.is_constructor(&species) {
            return Ok(species);
        }

        Err(self.create_type_error("Species constructor is not a constructor"))
    }

    /// PromiseResolve(C, x) - spec 27.2.4.7
    /// Like promise_resolve_value but uses C as the constructor
    fn promise_resolve_with_constructor(
        &mut self,
        constructor: &JsValue,
        value: &JsValue,
    ) -> Result<JsValue, JsValue> {
        // If value is a promise and its constructor matches C, return it
        if let JsValue::Object(o) = value
            && let Some(obj) = self.get_object(o.id)
            && obj.borrow().promise_data.is_some()
        {
            let ctor_val = match self.get_object_property(o.id, "constructor", value) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::Undefined,
            };
            if same_value(&ctor_val, constructor) {
                return Ok(value.clone());
            }
        }

        let cap = self.new_promise_capability(constructor)?;
        if let Completion::Throw(e) = self.call_function(
            &cap.resolve,
            &JsValue::Undefined,
            std::slice::from_ref(value),
        ) {
            return Err(e);
        }
        Ok(cap.promise)
    }

    pub(crate) fn setup_promise(&mut self) {
        let proto = self.create_object();
        self.promise_prototype = Some(proto.clone());

        // Promise.prototype.then
        let then_fn = self.create_function(JsFunction::native(
            "then".to_string(),
            2,
            |interp, this, args| {
                let on_fulfilled = args.first().cloned().unwrap_or(JsValue::Undefined);
                let on_rejected = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                interp.promise_then(this, &on_fulfilled, &on_rejected)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("then".to_string(), then_fn);

        // Promise.prototype.catch — spec 27.2.5.1
        let catch_fn = self.create_function(JsFunction::native(
            "catch".to_string(),
            1,
            |interp, this, args| {
                let on_rejected = args.first().cloned().unwrap_or(JsValue::Undefined);
                // Spec: Return ? Invoke(this, "then", « undefined, onRejected »).
                let this_id = match this {
                    JsValue::Object(o) => o.id,
                    _ => {
                        return Completion::Throw(
                            interp
                                .create_type_error("Promise.prototype.catch called on non-object"),
                        );
                    }
                };
                let then_method = match interp.get_object_property(this_id, "then", this) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Completion::Throw(e),
                    _ => JsValue::Undefined,
                };
                interp.call_function(&then_method, this, &[JsValue::Undefined, on_rejected])
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("catch".to_string(), catch_fn);

        // Promise.prototype.finally — spec 27.2.5.3
        let finally_fn = self.create_function(JsFunction::native(
            "finally".to_string(),
            1,
            |interp, this, args| {
                // Step 1-2: Let promise be the this value. If not Object, throw TypeError.
                let promise_id =
                    match this {
                        JsValue::Object(o) => o.id,
                        _ => {
                            return Completion::Throw(interp.create_type_error(
                                "Promise.prototype.finally called on non-object",
                            ));
                        }
                    };

                // Step 3: Let C = ? SpeciesConstructor(promise, %Promise%).
                let promise_ctor = interp
                    .global_env
                    .borrow()
                    .get("Promise")
                    .unwrap_or(JsValue::Undefined);
                let c = match interp.species_constructor(this, &promise_ctor) {
                    Ok(c) => c,
                    Err(e) => return Completion::Throw(e),
                };

                let on_finally = args.first().cloned().unwrap_or(JsValue::Undefined);

                // Steps 5-6: Create thenFinally and catchFinally
                let (then_finally, catch_finally) = if !interp.is_callable(&on_finally) {
                    // Step 5: If IsCallable(onFinally) is false, pass through
                    (on_finally.clone(), on_finally)
                } else {
                    // Step 6a: thenFinally closure
                    let on_finally_clone = on_finally.clone();
                    let c_clone = c.clone();
                    let then_finally = interp.create_function(JsFunction::native(
                        "".to_string(),
                        1,
                        move |interp, _this, args| {
                            let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                            // Step 6a.i: Let result = ? Call(onFinally, undefined).
                            let result =
                                interp.call_function(&on_finally_clone, &JsValue::Undefined, &[]);
                            match result {
                                Completion::Throw(e) => Completion::Throw(e),
                                Completion::Normal(r) => {
                                    // Step 6a.ii: Let promise = ? PromiseResolve(C, result).
                                    let p = match interp
                                        .promise_resolve_with_constructor(&c_clone, &r)
                                    {
                                        Ok(p) => p,
                                        Err(e) => return Completion::Throw(e),
                                    };
                                    // Step 6a.iii-iv: valueThunk that returns value
                                    let value_clone = value.clone();
                                    let return_fn = interp.create_function(JsFunction::native(
                                        "".to_string(),
                                        0,
                                        move |_interp, _this, _args| {
                                            Completion::Normal(value_clone.clone())
                                        },
                                    ));
                                    // Step 6a.v: Return ? Invoke(promise, "then", « valueThunk »).
                                    let p_id = if let JsValue::Object(ref o) = p {
                                        o.id
                                    } else {
                                        0
                                    };
                                    let then_method =
                                        match interp.get_object_property(p_id, "then", &p) {
                                            Completion::Normal(v) => v,
                                            Completion::Throw(e) => return Completion::Throw(e),
                                            _ => JsValue::Undefined,
                                        };
                                    interp.call_function(&then_method, &p, &[return_fn])
                                }
                                _ => Completion::Normal(JsValue::Undefined),
                            }
                        },
                    ));

                    // Step 6c: catchFinally closure
                    let on_finally_clone2 = on_finally.clone();
                    let c_clone2 = c;
                    let catch_finally = interp.create_function(JsFunction::native(
                        "".to_string(),
                        1,
                        move |interp, _this, args| {
                            let reason = args.first().cloned().unwrap_or(JsValue::Undefined);
                            // Step 6c.i: Let result = ? Call(onFinally, undefined).
                            let result =
                                interp.call_function(&on_finally_clone2, &JsValue::Undefined, &[]);
                            match result {
                                Completion::Throw(e) => Completion::Throw(e),
                                Completion::Normal(r) => {
                                    // Step 6c.ii: Let promise = ? PromiseResolve(C, result).
                                    let p = match interp
                                        .promise_resolve_with_constructor(&c_clone2, &r)
                                    {
                                        Ok(p) => p,
                                        Err(e) => return Completion::Throw(e),
                                    };
                                    // Step 6c.iii-iv: thrower that throws reason
                                    let reason_clone = reason.clone();
                                    let throw_fn = interp.create_function(JsFunction::native(
                                        "".to_string(),
                                        0,
                                        move |_interp, _this, _args| {
                                            Completion::Throw(reason_clone.clone())
                                        },
                                    ));
                                    // Step 6c.v: Return ? Invoke(promise, "then", « thrower »).
                                    let p_id = if let JsValue::Object(ref o) = p {
                                        o.id
                                    } else {
                                        0
                                    };
                                    let then_method =
                                        match interp.get_object_property(p_id, "then", &p) {
                                            Completion::Normal(v) => v,
                                            Completion::Throw(e) => return Completion::Throw(e),
                                            _ => JsValue::Undefined,
                                        };
                                    interp.call_function(&then_method, &p, &[throw_fn])
                                }
                                _ => Completion::Normal(JsValue::Undefined),
                            }
                        },
                    ));

                    (then_finally, catch_finally)
                };

                // Step 7: Return ? Invoke(promise, "then", « thenFinally, catchFinally »).
                let then_method = match interp.get_object_property(promise_id, "then", this) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Completion::Throw(e),
                    _ => JsValue::Undefined,
                };
                interp.call_function(&then_method, this, &[then_finally, catch_finally])
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("finally".to_string(), finally_fn);

        // @@toStringTag
        proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str("Promise")),
                false,
                false,
                true,
            ),
        );

        // Promise constructor
        let _promise_proto = self.promise_prototype.clone();
        let ctor = self.create_function(JsFunction::constructor(
            "Promise".to_string(),
            1,
            move |interp, _this, args| {
                let executor = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !interp.is_callable(&executor) {
                    let err = interp.create_type_error("Promise resolver is not a function");
                    return Completion::Throw(err);
                }
                let promise = interp.create_promise_object();
                let promise_id = if let JsValue::Object(ref o) = promise {
                    o.id
                } else {
                    0
                };
                let (resolve_fn, reject_fn) = interp.create_resolving_functions(promise_id);
                let result = interp.call_function(
                    &executor,
                    &JsValue::Undefined,
                    &[resolve_fn.clone(), reject_fn.clone()],
                );
                if let Completion::Throw(e) = result
                    && let Completion::Throw(e2) =
                        interp.call_function(&reject_fn, &JsValue::Undefined, &[e])
                {
                    return Completion::Throw(e2);
                }
                Completion::Normal(promise)
            },
        ));

        // Set Promise.prototype on constructor
        if let JsValue::Object(ref o) = ctor
            && let Some(func_obj) = self.get_object(o.id)
        {
            let proto_id = proto.borrow().id.unwrap();
            func_obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(
                    JsValue::Object(crate::types::JsObject { id: proto_id }),
                    false,
                    false,
                    false,
                ),
            );

            // Promise[Symbol.species] getter
            let species_getter = self.create_function(JsFunction::native(
                "get [Symbol.species]".to_string(),
                0,
                |_interp, this_val, _args| Completion::Normal(this_val.clone()),
            ));
            func_obj.borrow_mut().insert_property(
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

        // Set constructor on prototype
        proto.borrow_mut().insert_property(
            "constructor".to_string(),
            PropertyDescriptor::data(ctor.clone(), true, false, true),
        );

        // Promise.resolve
        let resolve_fn = self.create_function(JsFunction::native(
            "resolve".to_string(),
            1,
            |interp, this, args| {
                if !matches!(this, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("Promise.resolve requires an object"),
                    );
                }
                let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                match interp.promise_resolve_with_constructor(this, &value) {
                    Ok(p) => Completion::Normal(p),
                    Err(e) => Completion::Throw(e),
                }
            },
        ));
        if let JsValue::Object(ref o) = ctor
            && let Some(func_obj) = self.get_object(o.id)
        {
            func_obj
                .borrow_mut()
                .insert_builtin("resolve".to_string(), resolve_fn);
        }

        // Promise.reject
        let reject_fn = self.create_function(JsFunction::native(
            "reject".to_string(),
            1,
            |interp, this, args| {
                if !matches!(this, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("Promise.reject requires an object"),
                    );
                }
                let reason = args.first().cloned().unwrap_or(JsValue::Undefined);
                match interp.new_promise_capability(this) {
                    Ok(cap) => {
                        if let Completion::Throw(e) =
                            interp.call_function(&cap.reject, &JsValue::Undefined, &[reason])
                        {
                            return Completion::Throw(e);
                        }
                        Completion::Normal(cap.promise)
                    }
                    Err(e) => Completion::Throw(e),
                }
            },
        ));
        if let JsValue::Object(ref o) = ctor
            && let Some(func_obj) = self.get_object(o.id)
        {
            func_obj
                .borrow_mut()
                .insert_builtin("reject".to_string(), reject_fn);
        }

        // Promise.all
        let all_fn = self.create_function(JsFunction::native(
            "all".to_string(),
            1,
            |interp, this, args| {
                let iterable = args.first().cloned().unwrap_or(JsValue::Undefined);
                interp.promise_all(this, &iterable)
            },
        ));
        if let JsValue::Object(ref o) = ctor
            && let Some(func_obj) = self.get_object(o.id)
        {
            func_obj
                .borrow_mut()
                .insert_builtin("all".to_string(), all_fn);
        }

        // Promise.allSettled
        let all_settled_fn = self.create_function(JsFunction::native(
            "allSettled".to_string(),
            1,
            |interp, this, args| {
                let iterable = args.first().cloned().unwrap_or(JsValue::Undefined);
                interp.promise_all_settled(this, &iterable)
            },
        ));
        if let JsValue::Object(ref o) = ctor
            && let Some(func_obj) = self.get_object(o.id)
        {
            func_obj
                .borrow_mut()
                .insert_builtin("allSettled".to_string(), all_settled_fn);
        }

        // Promise.race
        let race_fn = self.create_function(JsFunction::native(
            "race".to_string(),
            1,
            |interp, this, args| {
                let iterable = args.first().cloned().unwrap_or(JsValue::Undefined);
                interp.promise_race(this, &iterable)
            },
        ));
        if let JsValue::Object(ref o) = ctor
            && let Some(func_obj) = self.get_object(o.id)
        {
            func_obj
                .borrow_mut()
                .insert_builtin("race".to_string(), race_fn);
        }

        // Promise.any
        let any_fn = self.create_function(JsFunction::native(
            "any".to_string(),
            1,
            |interp, this, args| {
                let iterable = args.first().cloned().unwrap_or(JsValue::Undefined);
                interp.promise_any(this, &iterable)
            },
        ));
        if let JsValue::Object(ref o) = ctor
            && let Some(func_obj) = self.get_object(o.id)
        {
            func_obj
                .borrow_mut()
                .insert_builtin("any".to_string(), any_fn);
        }

        // Promise.withResolvers
        let with_resolvers_fn = self.create_function(JsFunction::native(
            "withResolvers".to_string(),
            0,
            |interp, this, _args| match interp.new_promise_capability(this) {
                Ok(cap) => {
                    let result = interp.create_object();
                    result
                        .borrow_mut()
                        .insert_value("promise".to_string(), cap.promise);
                    result
                        .borrow_mut()
                        .insert_value("resolve".to_string(), cap.resolve);
                    result
                        .borrow_mut()
                        .insert_value("reject".to_string(), cap.reject);
                    let id = result.borrow().id.unwrap();
                    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
                }
                Err(e) => Completion::Throw(e),
            },
        ));
        if let JsValue::Object(ref o) = ctor
            && let Some(func_obj) = self.get_object(o.id)
        {
            func_obj
                .borrow_mut()
                .insert_builtin("withResolvers".to_string(), with_resolvers_fn);
        }

        // Promise.try
        let try_fn = self.create_function(JsFunction::native(
            "try".to_string(),
            1,
            |interp, this, args| {
                let cap = match interp.new_promise_capability(this) {
                    Ok(cap) => cap,
                    Err(e) => return Completion::Throw(e),
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                let call_args: Vec<JsValue> = if args.len() > 1 {
                    args[1..].to_vec()
                } else {
                    vec![]
                };
                if !interp.is_callable(&callback) {
                    let err = interp.create_type_error("Promise.try requires a callable");
                    if let Completion::Throw(e) =
                        interp.call_function(&cap.reject, &JsValue::Undefined, &[err])
                    {
                        return Completion::Throw(e);
                    }
                    return Completion::Normal(cap.promise);
                }
                let result = interp.call_function(&callback, &JsValue::Undefined, &call_args);
                match result {
                    Completion::Normal(v) => {
                        if let Completion::Throw(e) =
                            interp.call_function(&cap.resolve, &JsValue::Undefined, &[v])
                        {
                            return Completion::Throw(e);
                        }
                    }
                    Completion::Throw(e) => {
                        if let Completion::Throw(e2) =
                            interp.call_function(&cap.reject, &JsValue::Undefined, &[e])
                        {
                            return Completion::Throw(e2);
                        }
                    }
                    _ => {}
                }
                Completion::Normal(cap.promise)
            },
        ));
        if let JsValue::Object(ref o) = ctor
            && let Some(func_obj) = self.get_object(o.id)
        {
            func_obj
                .borrow_mut()
                .insert_builtin("try".to_string(), try_fn);
        }

        // Register Promise as global
        self.global_env
            .borrow_mut()
            .declare("Promise", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("Promise", ctor);
    }

    pub(crate) fn create_promise_object(&mut self) -> JsValue {
        let mut data = JsObjectData::new();
        data.prototype = self.promise_prototype.clone();
        data.class_name = "Promise".to_string();
        data.promise_data = Some(PromiseData::new());
        let obj = Rc::new(RefCell::new(data));
        let id = self.allocate_object_slot(obj);
        JsValue::Object(crate::types::JsObject { id })
    }

    pub(crate) fn create_resolved_promise(&mut self, value: JsValue) -> Completion {
        let promise = self.create_promise_object();
        if let JsValue::Object(ref o) = promise {
            self.fulfill_promise(o.id, value);
        }
        Completion::Normal(promise)
    }

    pub(crate) fn create_rejected_promise(&mut self, reason: JsValue) -> Completion {
        let promise = self.create_promise_object();
        if let JsValue::Object(ref o) = promise {
            self.reject_promise(o.id, reason);
        }
        Completion::Normal(promise)
    }

    pub(crate) fn create_resolving_functions(&mut self, promise_id: u64) -> (JsValue, JsValue) {
        let already_resolved = Rc::new(Cell::new(false));

        let ar1 = already_resolved.clone();
        let resolve_fn = self.create_function(JsFunction::native(
            "".to_string(),
            1,
            move |interp, _this, args| {
                if ar1.get() {
                    return Completion::Normal(JsValue::Undefined);
                }
                ar1.set(true);
                let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                // If resolving with self, reject with TypeError
                if let JsValue::Object(ref o) = value
                    && o.id == promise_id
                {
                    let err = interp.create_type_error("A promise cannot be resolved with itself.");
                    interp.reject_promise(promise_id, err);
                    return Completion::Normal(JsValue::Undefined);
                }
                // Check if value is a thenable
                if let JsValue::Object(ref o) = value {
                    // Spec step 8: Let then be Completion(Get(resolution, "then")).
                    // Step 9: If then is an abrupt completion, then
                    //   a. Return RejectPromise(promise, then.[[Value]]).
                    let then_val = match interp.get_object_property(o.id, "then", &value) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            interp.reject_promise(promise_id, e);
                            return Completion::Normal(JsValue::Undefined);
                        }
                        _ => JsValue::Undefined,
                    };
                    if interp.is_callable(&then_val) {
                        interp.promise_resolve_thenable(promise_id, value.clone(), then_val);
                        return Completion::Normal(JsValue::Undefined);
                    }
                }
                interp.fulfill_promise(promise_id, value);
                Completion::Normal(JsValue::Undefined)
            },
        ));

        let ar2 = already_resolved.clone();
        let reject_fn = self.create_function(JsFunction::native(
            "".to_string(),
            1,
            move |interp, _this, args| {
                if ar2.get() {
                    return Completion::Normal(JsValue::Undefined);
                }
                ar2.set(true);
                let reason = args.first().cloned().unwrap_or(JsValue::Undefined);
                interp.reject_promise(promise_id, reason);
                Completion::Normal(JsValue::Undefined)
            },
        ));

        (resolve_fn, reject_fn)
    }

    pub(crate) fn fulfill_promise(&mut self, promise_id: u64, value: JsValue) {
        let reactions = if let Some(obj) = self.get_object(promise_id) {
            let mut o = obj.borrow_mut();
            if let Some(ref mut pd) = o.promise_data {
                if !matches!(pd.state, PromiseState::Pending) {
                    return;
                }
                pd.state = PromiseState::Fulfilled(value.clone());
                let reactions = std::mem::take(&mut pd.fulfill_reactions);
                pd.reject_reactions.clear();
                reactions
            } else {
                return;
            }
        } else {
            return;
        };
        self.trigger_promise_reactions(reactions, value);
    }

    pub(crate) fn reject_promise(&mut self, promise_id: u64, reason: JsValue) {
        let reactions = if let Some(obj) = self.get_object(promise_id) {
            let mut o = obj.borrow_mut();
            if let Some(ref mut pd) = o.promise_data {
                if !matches!(pd.state, PromiseState::Pending) {
                    return;
                }
                pd.state = PromiseState::Rejected(reason.clone());
                let reactions = std::mem::take(&mut pd.reject_reactions);
                pd.fulfill_reactions.clear();
                reactions
            } else {
                return;
            }
        } else {
            return;
        };
        self.trigger_promise_reactions(reactions, reason);
    }

    fn trigger_promise_reactions(&mut self, reactions: Vec<PromiseReaction>, argument: JsValue) {
        for reaction in reactions {
            let arg = argument.clone();
            self.microtask_queue.push(Box::new(move |interp| {
                let handler_result = if let Some(ref handler) = reaction.handler {
                    if interp.is_callable(handler) {
                        match interp.call_function(
                            handler,
                            &JsValue::Undefined,
                            std::slice::from_ref(&arg),
                        ) {
                            Completion::Throw(e) => Err(e),
                            Completion::Normal(v) => Ok(v),
                            _ => Ok(JsValue::Undefined),
                        }
                    } else {
                        match reaction.reaction_type {
                            PromiseReactionType::Fulfill => Ok(arg.clone()),
                            PromiseReactionType::Reject => Err(arg.clone()),
                        }
                    }
                } else {
                    match reaction.reaction_type {
                        PromiseReactionType::Fulfill => Ok(arg.clone()),
                        PromiseReactionType::Reject => Err(arg.clone()),
                    }
                };

                if let Some(_pid) = reaction.promise_id {
                    match handler_result {
                        Ok(value) => {
                            if let Completion::Throw(e) = interp.call_function(
                                &reaction.resolve,
                                &JsValue::Undefined,
                                &[value],
                            ) {
                                return Completion::Throw(e);
                            }
                        }
                        Err(reason) => {
                            if let Completion::Throw(e) = interp.call_function(
                                &reaction.reject,
                                &JsValue::Undefined,
                                &[reason],
                            ) {
                                return Completion::Throw(e);
                            }
                        }
                    }
                }
                Completion::Normal(JsValue::Undefined)
            }));
        }
    }

    pub(crate) fn promise_resolve_thenable(
        &mut self,
        promise_id: u64,
        thenable: JsValue,
        then_fn: JsValue,
    ) {
        let (resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);
        self.microtask_queue.push(Box::new(move |interp| {
            let result =
                interp.call_function(&then_fn, &thenable, &[resolve_fn, reject_fn.clone()]);
            if let Completion::Throw(e) = result
                && let Completion::Throw(e2) =
                    interp.call_function(&reject_fn, &JsValue::Undefined, &[e])
            {
                return Completion::Throw(e2);
            }
            Completion::Normal(JsValue::Undefined)
        }));
    }

    fn promise_then(
        &mut self,
        promise_val: &JsValue,
        on_fulfilled: &JsValue,
        on_rejected: &JsValue,
    ) -> Completion {
        let promise_id = if let JsValue::Object(o) = promise_val {
            if let Some(obj) = self.get_object(o.id) {
                if obj.borrow().promise_data.is_some() {
                    o.id
                } else {
                    let err =
                        self.create_type_error("Promise.prototype.then called on non-promise");
                    return Completion::Throw(err);
                }
            } else {
                let err = self.create_type_error("Promise.prototype.then called on non-promise");
                return Completion::Throw(err);
            }
        } else {
            let err = self.create_type_error("Promise.prototype.then called on non-promise");
            return Completion::Throw(err);
        };

        // SpeciesConstructor(promise, %Promise%)
        let promise_ctor = self
            .global_env
            .borrow()
            .get("Promise")
            .unwrap_or(JsValue::Undefined);
        let c = match self.species_constructor(promise_val, &promise_ctor) {
            Ok(c) => c,
            Err(e) => return Completion::Throw(e),
        };

        let cap = match self.new_promise_capability(&c) {
            Ok(cap) => cap,
            Err(e) => return Completion::Throw(e),
        };
        let derived = cap.promise;
        let derived_id = if let JsValue::Object(ref o) = derived {
            o.id
        } else {
            0
        };
        let resolve_fn = cap.resolve;
        let reject_fn = cap.reject;

        let fulfill_handler = if self.is_callable(on_fulfilled) {
            Some(on_fulfilled.clone())
        } else {
            None
        };
        let reject_handler = if self.is_callable(on_rejected) {
            Some(on_rejected.clone())
        } else {
            None
        };

        let fulfill_reaction = PromiseReaction {
            handler: fulfill_handler,
            promise_id: Some(derived_id),
            resolve: resolve_fn.clone(),
            reject: reject_fn.clone(),
            reaction_type: PromiseReactionType::Fulfill,
        };
        let reject_reaction = PromiseReaction {
            handler: reject_handler,
            promise_id: Some(derived_id),
            resolve: resolve_fn,
            reject: reject_fn,
            reaction_type: PromiseReactionType::Reject,
        };

        let fulfill_reaction2 = fulfill_reaction.clone();
        let reject_reaction2 = reject_reaction.clone();
        let state = if let Some(obj) = self.get_object(promise_id) {
            let mut o = obj.borrow_mut();
            if let Some(ref mut pd) = o.promise_data {
                pd.is_handled = true;
                match &pd.state {
                    PromiseState::Pending => {
                        pd.fulfill_reactions.push(fulfill_reaction);
                        pd.reject_reactions.push(reject_reaction);
                        None
                    }
                    PromiseState::Fulfilled(v) => Some((true, v.clone())),
                    PromiseState::Rejected(r) => Some((false, r.clone())),
                }
            } else {
                None
            }
        } else {
            None
        };

        if let Some((is_fulfilled, value)) = state {
            if is_fulfilled {
                self.trigger_promise_reactions(vec![fulfill_reaction2], value);
            } else {
                self.trigger_promise_reactions(vec![reject_reaction2], value);
            }
        }

        Completion::Normal(derived)
    }

    pub(crate) fn promise_resolve_value(&mut self, value: &JsValue) -> JsValue {
        // If already a promise, return it
        if let JsValue::Object(o) = value
            && let Some(obj) = self.get_object(o.id)
            && obj.borrow().promise_data.is_some()
        {
            return value.clone();
        }
        let promise = self.create_promise_object();
        let promise_id = if let JsValue::Object(ref o) = promise {
            o.id
        } else {
            0
        };
        // Check if value is a thenable using [[Get]] to trigger getters
        if matches!(value, JsValue::Object(_)) {
            let then_val = match self.obj_get(value, "then") {
                Ok(v) => v,
                Err(e) => {
                    self.reject_promise(promise_id, e);
                    return promise;
                }
            };
            if self.is_callable(&then_val) {
                self.promise_resolve_thenable(promise_id, value.clone(), then_val);
                return promise;
            }
        }
        self.fulfill_promise(promise_id, value.clone());
        promise
    }

    fn promise_all(&mut self, constructor: &JsValue, iterable: &JsValue) -> Completion {
        let cap = match self.new_promise_capability(constructor) {
            Ok(cap) => cap,
            Err(e) => return Completion::Throw(e),
        };

        // GetPromiseResolve(C) + IfAbruptRejectPromise
        let ctor_id = if let JsValue::Object(o) = constructor {
            o.id
        } else {
            0
        };
        let promise_resolve = match self.get_object_property(ctor_id, "resolve", constructor) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => {
                if let Completion::Throw(e2) =
                    self.call_function(&cap.reject, &JsValue::Undefined, &[e])
                {
                    return Completion::Throw(e2);
                }
                return Completion::Normal(cap.promise);
            }
            _ => JsValue::Undefined,
        };
        if !self.is_callable(&promise_resolve) {
            let err = self.create_type_error("Promise resolve is not a function");
            if let Completion::Throw(e2) =
                self.call_function(&cap.reject, &JsValue::Undefined, &[err])
            {
                return Completion::Throw(e2);
            }
            return Completion::Normal(cap.promise);
        }

        let items = match self.iterate_to_vec(iterable) {
            Ok(v) => v,
            Err(e) => {
                if let Completion::Throw(e2) =
                    self.call_function(&cap.reject, &JsValue::Undefined, &[e])
                {
                    return Completion::Throw(e2);
                }
                return Completion::Normal(cap.promise);
            }
        };

        if items.is_empty() {
            let arr = self.create_array(vec![]);
            if let Completion::Throw(e) =
                self.call_function(&cap.resolve, &JsValue::Undefined, &[arr])
                && let Completion::Throw(e2) =
                    self.call_function(&cap.reject, &JsValue::Undefined, &[e])
            {
                return Completion::Throw(e2);
            }
            return Completion::Normal(cap.promise);
        }

        let count = items.len();
        let remaining = Rc::new(Cell::new(count));
        let results = Rc::new(RefCell::new(vec![JsValue::Undefined; count]));

        for (i, item) in items.into_iter().enumerate() {
            let p = match self.call_function(&promise_resolve, constructor, &[item]) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    if let Completion::Throw(e2) =
                        self.call_function(&cap.reject, &JsValue::Undefined, &[e])
                    {
                        return Completion::Throw(e2);
                    }
                    return Completion::Normal(cap.promise);
                }
                _ => JsValue::Undefined,
            };
            let remaining = remaining.clone();
            let results = results.clone();
            let resolve_fn = cap.resolve.clone();
            let reject_fn_clone = cap.reject.clone();
            let already_called = Rc::new(Cell::new(false));

            let ac = already_called.clone();
            let on_fulfilled = self.create_function(JsFunction::native(
                "".to_string(),
                1,
                move |interp, _this, args| {
                    if ac.get() {
                        return Completion::Normal(JsValue::Undefined);
                    }
                    ac.set(true);
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    results.borrow_mut()[i] = val;
                    let r = remaining.get() - 1;
                    remaining.set(r);
                    if r == 0 {
                        let values = results.borrow().clone();
                        let arr = interp.create_array(values);
                        if let Completion::Throw(e) =
                            interp.call_function(&resolve_fn, &JsValue::Undefined, &[arr])
                        {
                            return Completion::Throw(e);
                        }
                    }
                    Completion::Normal(JsValue::Undefined)
                },
            ));
            // Invoke(nextPromise, "then", « onFulfilled, rejectElement »)
            let p_id = if let JsValue::Object(ref o) = p {
                o.id
            } else {
                0
            };
            let then_fn = match self.get_object_property(p_id, "then", &p) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    if let Completion::Throw(e2) =
                        self.call_function(&cap.reject, &JsValue::Undefined, &[e])
                    {
                        return Completion::Throw(e2);
                    }
                    return Completion::Normal(cap.promise);
                }
                _ => JsValue::Undefined,
            };
            if let Completion::Throw(e) =
                self.call_function(&then_fn, &p, &[on_fulfilled, reject_fn_clone])
            {
                if let Completion::Throw(e2) =
                    self.call_function(&cap.reject, &JsValue::Undefined, &[e])
                {
                    return Completion::Throw(e2);
                }
                return Completion::Normal(cap.promise);
            }
        }

        Completion::Normal(cap.promise)
    }

    fn promise_all_settled(&mut self, constructor: &JsValue, iterable: &JsValue) -> Completion {
        let cap = match self.new_promise_capability(constructor) {
            Ok(cap) => cap,
            Err(e) => return Completion::Throw(e),
        };

        let ctor_id = if let JsValue::Object(o) = constructor {
            o.id
        } else {
            0
        };
        let promise_resolve = match self.get_object_property(ctor_id, "resolve", constructor) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => {
                if let Completion::Throw(e2) =
                    self.call_function(&cap.reject, &JsValue::Undefined, &[e])
                {
                    return Completion::Throw(e2);
                }
                return Completion::Normal(cap.promise);
            }
            _ => JsValue::Undefined,
        };
        if !self.is_callable(&promise_resolve) {
            let err = self.create_type_error("Promise resolve is not a function");
            if let Completion::Throw(e2) =
                self.call_function(&cap.reject, &JsValue::Undefined, &[err])
            {
                return Completion::Throw(e2);
            }
            return Completion::Normal(cap.promise);
        }

        let items = match self.iterate_to_vec(iterable) {
            Ok(v) => v,
            Err(e) => {
                if let Completion::Throw(e2) =
                    self.call_function(&cap.reject, &JsValue::Undefined, &[e])
                {
                    return Completion::Throw(e2);
                }
                return Completion::Normal(cap.promise);
            }
        };

        if items.is_empty() {
            let arr = self.create_array(vec![]);
            if let Completion::Throw(e) =
                self.call_function(&cap.resolve, &JsValue::Undefined, &[arr])
                && let Completion::Throw(e2) =
                    self.call_function(&cap.reject, &JsValue::Undefined, &[e])
            {
                return Completion::Throw(e2);
            }
            return Completion::Normal(cap.promise);
        }

        let count = items.len();
        let remaining = Rc::new(Cell::new(count));
        let results = Rc::new(RefCell::new(vec![JsValue::Undefined; count]));

        for (i, item) in items.into_iter().enumerate() {
            let p = match self.call_function(&promise_resolve, constructor, &[item]) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    if let Completion::Throw(e2) =
                        self.call_function(&cap.reject, &JsValue::Undefined, &[e])
                    {
                        return Completion::Throw(e2);
                    }
                    return Completion::Normal(cap.promise);
                }
                _ => JsValue::Undefined,
            };
            let remaining_f = remaining.clone();
            let results_f = results.clone();
            let resolve_fn_f = cap.resolve.clone();
            let remaining_r = remaining.clone();
            let results_r = results.clone();
            let resolve_fn_r = cap.resolve.clone();
            let already_called = Rc::new(Cell::new(false));

            let ac_f = already_called.clone();
            let on_fulfilled = self.create_function(JsFunction::native(
                "".to_string(),
                1,
                move |interp, _this, args| {
                    if ac_f.get() {
                        return Completion::Normal(JsValue::Undefined);
                    }
                    ac_f.set(true);
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let obj = interp.create_object();
                    {
                        let mut o = obj.borrow_mut();
                        o.insert_value(
                            "status".to_string(),
                            JsValue::String(JsString::from_str("fulfilled")),
                        );
                        o.insert_value("value".to_string(), val);
                    }
                    let oid = obj.borrow().id.unwrap();
                    results_f.borrow_mut()[i] = JsValue::Object(crate::types::JsObject { id: oid });
                    let r = remaining_f.get() - 1;
                    remaining_f.set(r);
                    if r == 0 {
                        let values = results_f.borrow().clone();
                        let arr = interp.create_array(values);
                        if let Completion::Throw(e) =
                            interp.call_function(&resolve_fn_f, &JsValue::Undefined, &[arr])
                        {
                            return Completion::Throw(e);
                        }
                    }
                    Completion::Normal(JsValue::Undefined)
                },
            ));
            let ac_r = already_called.clone();
            let on_rejected = self.create_function(JsFunction::native(
                "".to_string(),
                1,
                move |interp, _this, args| {
                    if ac_r.get() {
                        return Completion::Normal(JsValue::Undefined);
                    }
                    ac_r.set(true);
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let obj = interp.create_object();
                    {
                        let mut o = obj.borrow_mut();
                        o.insert_value(
                            "status".to_string(),
                            JsValue::String(JsString::from_str("rejected")),
                        );
                        o.insert_value("reason".to_string(), val);
                    }
                    let oid = obj.borrow().id.unwrap();
                    results_r.borrow_mut()[i] = JsValue::Object(crate::types::JsObject { id: oid });
                    let r = remaining_r.get() - 1;
                    remaining_r.set(r);
                    if r == 0 {
                        let values = results_r.borrow().clone();
                        let arr = interp.create_array(values);
                        if let Completion::Throw(e) =
                            interp.call_function(&resolve_fn_r, &JsValue::Undefined, &[arr])
                        {
                            return Completion::Throw(e);
                        }
                    }
                    Completion::Normal(JsValue::Undefined)
                },
            ));
            // Invoke(nextPromise, "then", « onFulfilled, onRejected »)
            let p_id = if let JsValue::Object(ref o) = p {
                o.id
            } else {
                0
            };
            let then_fn = match self.get_object_property(p_id, "then", &p) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    if let Completion::Throw(e2) =
                        self.call_function(&cap.reject, &JsValue::Undefined, &[e])
                    {
                        return Completion::Throw(e2);
                    }
                    return Completion::Normal(cap.promise);
                }
                _ => JsValue::Undefined,
            };
            if let Completion::Throw(e) =
                self.call_function(&then_fn, &p, &[on_fulfilled, on_rejected])
            {
                if let Completion::Throw(e2) =
                    self.call_function(&cap.reject, &JsValue::Undefined, &[e])
                {
                    return Completion::Throw(e2);
                }
                return Completion::Normal(cap.promise);
            }
        }

        Completion::Normal(cap.promise)
    }

    fn promise_race(&mut self, constructor: &JsValue, iterable: &JsValue) -> Completion {
        let cap = match self.new_promise_capability(constructor) {
            Ok(cap) => cap,
            Err(e) => return Completion::Throw(e),
        };

        let ctor_id = if let JsValue::Object(o) = constructor {
            o.id
        } else {
            0
        };
        let promise_resolve = match self.get_object_property(ctor_id, "resolve", constructor) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => {
                if let Completion::Throw(e2) =
                    self.call_function(&cap.reject, &JsValue::Undefined, &[e])
                {
                    return Completion::Throw(e2);
                }
                return Completion::Normal(cap.promise);
            }
            _ => JsValue::Undefined,
        };
        if !self.is_callable(&promise_resolve) {
            let err = self.create_type_error("Promise resolve is not a function");
            if let Completion::Throw(e2) =
                self.call_function(&cap.reject, &JsValue::Undefined, &[err])
            {
                return Completion::Throw(e2);
            }
            return Completion::Normal(cap.promise);
        }

        let items = match self.iterate_to_vec(iterable) {
            Ok(v) => v,
            Err(e) => {
                if let Completion::Throw(e2) =
                    self.call_function(&cap.reject, &JsValue::Undefined, &[e])
                {
                    return Completion::Throw(e2);
                }
                return Completion::Normal(cap.promise);
            }
        };

        for item in items {
            let p = match self.call_function(&promise_resolve, constructor, &[item]) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    if let Completion::Throw(e2) =
                        self.call_function(&cap.reject, &JsValue::Undefined, &[e])
                    {
                        return Completion::Throw(e2);
                    }
                    return Completion::Normal(cap.promise);
                }
                _ => JsValue::Undefined,
            };
            // Invoke(nextPromise, "then", « resolve, reject »)
            let p_id = if let JsValue::Object(ref o) = p {
                o.id
            } else {
                0
            };
            let then_fn = match self.get_object_property(p_id, "then", &p) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    if let Completion::Throw(e2) =
                        self.call_function(&cap.reject, &JsValue::Undefined, &[e])
                    {
                        return Completion::Throw(e2);
                    }
                    return Completion::Normal(cap.promise);
                }
                _ => JsValue::Undefined,
            };
            if let Completion::Throw(e) =
                self.call_function(&then_fn, &p, &[cap.resolve.clone(), cap.reject.clone()])
            {
                if let Completion::Throw(e2) =
                    self.call_function(&cap.reject, &JsValue::Undefined, &[e])
                {
                    return Completion::Throw(e2);
                }
                return Completion::Normal(cap.promise);
            }
        }

        Completion::Normal(cap.promise)
    }

    fn promise_any(&mut self, constructor: &JsValue, iterable: &JsValue) -> Completion {
        let cap = match self.new_promise_capability(constructor) {
            Ok(cap) => cap,
            Err(e) => return Completion::Throw(e),
        };

        let ctor_id = if let JsValue::Object(o) = constructor {
            o.id
        } else {
            0
        };
        let promise_resolve = match self.get_object_property(ctor_id, "resolve", constructor) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => {
                if let Completion::Throw(e2) =
                    self.call_function(&cap.reject, &JsValue::Undefined, &[e])
                {
                    return Completion::Throw(e2);
                }
                return Completion::Normal(cap.promise);
            }
            _ => JsValue::Undefined,
        };
        if !self.is_callable(&promise_resolve) {
            let err = self.create_type_error("Promise resolve is not a function");
            if let Completion::Throw(e2) =
                self.call_function(&cap.reject, &JsValue::Undefined, &[err])
            {
                return Completion::Throw(e2);
            }
            return Completion::Normal(cap.promise);
        }

        let items = match self.iterate_to_vec(iterable) {
            Ok(v) => v,
            Err(e) => {
                if let Completion::Throw(e2) =
                    self.call_function(&cap.reject, &JsValue::Undefined, &[e])
                {
                    return Completion::Throw(e2);
                }
                return Completion::Normal(cap.promise);
            }
        };

        if items.is_empty() {
            let err = self.create_aggregate_error(vec![], "All promises were rejected");
            if let Completion::Throw(e2) =
                self.call_function(&cap.reject, &JsValue::Undefined, &[err])
            {
                return Completion::Throw(e2);
            }
            return Completion::Normal(cap.promise);
        }

        let count = items.len();
        let remaining = Rc::new(Cell::new(count));
        let errors = Rc::new(RefCell::new(vec![JsValue::Undefined; count]));

        for (i, item) in items.into_iter().enumerate() {
            let p = match self.call_function(&promise_resolve, constructor, &[item]) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    if let Completion::Throw(e2) =
                        self.call_function(&cap.reject, &JsValue::Undefined, &[e])
                    {
                        return Completion::Throw(e2);
                    }
                    return Completion::Normal(cap.promise);
                }
                _ => JsValue::Undefined,
            };
            let remaining = remaining.clone();
            let errors = errors.clone();
            let reject_fn_clone = cap.reject.clone();
            let already_called = Rc::new(Cell::new(false));

            let ac = already_called.clone();
            let on_rejected = self.create_function(JsFunction::native(
                "".to_string(),
                1,
                move |interp, _this, args| {
                    if ac.get() {
                        return Completion::Normal(JsValue::Undefined);
                    }
                    ac.set(true);
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    errors.borrow_mut()[i] = val;
                    let r = remaining.get() - 1;
                    remaining.set(r);
                    if r == 0 {
                        let errs = errors.borrow().clone();
                        let err = interp.create_aggregate_error(errs, "All promises were rejected");
                        if let Completion::Throw(e) =
                            interp.call_function(&reject_fn_clone, &JsValue::Undefined, &[err])
                        {
                            return Completion::Throw(e);
                        }
                    }
                    Completion::Normal(JsValue::Undefined)
                },
            ));
            // Invoke(nextPromise, "then", « resolve, onRejected »)
            let p_id = if let JsValue::Object(ref o) = p {
                o.id
            } else {
                0
            };
            let then_fn = match self.get_object_property(p_id, "then", &p) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    if let Completion::Throw(e2) =
                        self.call_function(&cap.reject, &JsValue::Undefined, &[e])
                    {
                        return Completion::Throw(e2);
                    }
                    return Completion::Normal(cap.promise);
                }
                _ => JsValue::Undefined,
            };
            if let Completion::Throw(e) =
                self.call_function(&then_fn, &p, &[cap.resolve.clone(), on_rejected])
            {
                if let Completion::Throw(e2) =
                    self.call_function(&cap.reject, &JsValue::Undefined, &[e])
                {
                    return Completion::Throw(e2);
                }
                return Completion::Normal(cap.promise);
            }
        }

        Completion::Normal(cap.promise)
    }

    fn create_aggregate_error(&mut self, errors: Vec<JsValue>, message: &str) -> JsValue {
        let obj = self.create_object();
        {
            let mut o = obj.borrow_mut();
            o.class_name = "AggregateError".to_string();
            if let Some(ref proto) = self.aggregate_error_prototype {
                o.prototype = Some(proto.clone());
            }
            o.insert_builtin(
                "message".to_string(),
                JsValue::String(JsString::from_str(message)),
            );
        }
        let errors_arr = self.create_array(errors);
        obj.borrow_mut()
            .insert_builtin("errors".to_string(), errors_arr);
        let id = obj.borrow().id.unwrap();
        JsValue::Object(crate::types::JsObject { id })
    }

    pub(crate) fn is_callable(&self, val: &JsValue) -> bool {
        if let JsValue::Object(o) = val
            && let Some(obj) = self.get_object(o.id)
        {
            if obj.borrow().callable.is_some() {
                return true;
            }
            // Proxy wrapping a callable is callable
            if obj.borrow().is_proxy() {
                let target_val = self.get_proxy_target_val(o.id);
                return self.is_callable(&target_val);
            }
        }
        false
    }

    pub(crate) fn is_constructor(&self, val: &JsValue) -> bool {
        if let JsValue::Object(o) = val
            && let Some(obj) = self.get_object(o.id)
        {
            if let Some(ref func) = obj.borrow().callable {
                return match func {
                    JsFunction::Native(_, _, _, is_ctor) => *is_ctor,
                    JsFunction::User {
                        is_arrow,
                        is_method,
                        is_async,
                        is_generator,
                        ..
                    } => !is_arrow && !is_method && !*is_generator && !*is_async,
                };
            }
            // Proxy wrapping a constructor is a constructor
            if obj.borrow().is_proxy() {
                let target_val = self.get_proxy_target_val(o.id);
                return self.is_constructor(&target_val);
            }
        }
        false
    }

    pub(crate) fn is_promise(&self, val: &JsValue) -> bool {
        if let JsValue::Object(o) = val
            && let Some(obj) = self.get_object(o.id)
        {
            return obj.borrow().promise_data.is_some();
        }
        false
    }

    pub(crate) fn get_promise_state(&self, promise_id: u64) -> Option<PromiseState> {
        if let Some(obj) = self.get_object(promise_id)
            && let Some(ref pd) = obj.borrow().promise_data
        {
            return Some(pd.state.clone());
        }
        None
    }
}
