use super::*;
use std::cell::Cell;

/// PromiseCapability record: {promise, resolve, reject}
pub(crate) struct PromiseCapability {
    pub promise: JsValue,
    pub resolve: JsValue,
    pub reject: JsValue,
}

impl Interpreter {
    /// IfAbruptRejectPromise: reject with error and return the promise.
    fn if_abrupt_reject_promise(&mut self, error: JsValue, cap: &PromiseCapability) -> Completion {
        let _ = self.call_function(&cap.reject, &JsValue::UNDEFINED, &[error]);
        Completion::Normal(cap.promise.clone())
    }

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
        let promise_ctor = self.get_global_var("Promise");
        if let Some(ref ctor_val) = promise_ctor
            && same_value(constructor, ctor_val)
        {
            let promise = self.create_promise_object();
            let promise_id = promise.as_object_id().unwrap_or_default();
            let (resolve, reject) = self.create_resolving_functions(promise_id);
            return Ok(PromiseCapability {
                promise,
                resolve,
                reject,
            });
        }

        // General case: call new C(executor) where executor captures resolve/reject
        let resolve_slot: Rc<RefCell<JsValue>> = Rc::new(RefCell::new(JsValue::UNDEFINED));
        let reject_slot: Rc<RefCell<JsValue>> = Rc::new(RefCell::new(JsValue::UNDEFINED));

        let rs = resolve_slot.clone();
        let rj = reject_slot.clone();
        let executor = self.create_function(JsFunction::native(
            "".to_string(),
            2,
            move |interp, _this, args| {
                let resolve_arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let reject_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);

                // Spec: If promiseCapability.[[Resolve]] is not undefined, throw TypeError
                if !(*rs.borrow()).is_undefined() {
                    return Completion::Throw(
                        interp.create_type_error("Promise executor has already been resolved"),
                    );
                }
                // Spec: If promiseCapability.[[Reject]] is not undefined, throw TypeError
                if !(*rj.borrow()).is_undefined() {
                    return Completion::Throw(
                        interp.create_type_error("Promise executor has already been resolved"),
                    );
                }

                // Spec: Set promiseCapability.[[Resolve]] to resolve
                *rs.borrow_mut() = resolve_arg;
                // Spec: Set promiseCapability.[[Reject]] to reject
                *rj.borrow_mut() = reject_arg;

                Completion::Normal(JsValue::UNDEFINED)
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
        let obj_id = if let Some(o) = obj.as_object_id() {
            o
        } else {
            return Ok(default_ctor.clone());
        };

        // Get O.constructor
        let ctor = match self.get_object_property(obj_id, "constructor", obj) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => return Ok(default_ctor.clone()),
        };

        if (ctor).is_undefined() {
            return Ok(default_ctor.clone());
        }

        if !(ctor).is_object() {
            return Err(self.create_type_error("Species constructor is not an object"));
        }

        // Get constructor[Symbol.species]
        let ctor_id = if let Some(o) = ctor.as_object_id() {
            o
        } else {
            return Ok(default_ctor.clone());
        };
        let species = match self.get_object_property(
            ctor_id,
            &JsPropertyKey::well_known_symbol("species"),
            &ctor,
        ) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => return Ok(default_ctor.clone()),
        };

        if (species).is_nullish() {
            return Ok(default_ctor.clone());
        }

        if self.is_constructor(&species) {
            return Ok(species);
        }

        Err(self.create_type_error("Species constructor is not a constructor"))
    }

    /// PromiseResolve(C, x) - spec 27.2.4.7
    /// Like promise_resolve_value but uses C as the constructor
    pub(crate) fn promise_resolve_with_constructor(
        &mut self,
        constructor: &JsValue,
        value: &JsValue,
    ) -> Result<JsValue, JsValue> {
        // If value is a promise and its constructor matches C, return it
        if let Some(o) = value.as_object_id()
            && let Some(obj) = self.get_object_cell(o)
            && obj.borrow().promise_data().is_some()
        {
            let ctor_val = match self.get_object_property(o, "constructor", value) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::UNDEFINED,
            };
            if same_value(&ctor_val, constructor) {
                return Ok(value.clone());
            }
        }

        let cap = self.new_promise_capability(constructor)?;
        if let Completion::Throw(e) = self.call_function(
            &cap.resolve,
            &JsValue::UNDEFINED,
            std::slice::from_ref(value),
        ) {
            return Err(e);
        }
        Ok(cap.promise)
    }

    pub(crate) fn setup_promise(&mut self) {
        let proto_id = self.create_object_id();
        self.realm_mut().promise_prototype = Some(proto_id);

        // Promise.prototype.then
        self.define_method(proto_id, "then", 2, |interp, this, args| {
            let on_fulfilled = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            let on_rejected = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
            interp.promise_then(this, &on_fulfilled, &on_rejected)
        });

        // Promise.prototype.catch — spec 27.2.5.1
        self.define_method(proto_id, "catch", 1, |interp, this, args| {
            let on_rejected = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            // Spec 27.2.5.1: Return ? Invoke(this, "then", « undefined, onRejected »).
            // Invoke calls GetV(V, P) which calls ToObject(V).
            let obj = match interp.to_object(this) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Completion::Throw(e),
                _ => return Completion::Normal(JsValue::UNDEFINED),
            };
            let obj_id = obj.as_object_id().unwrap_or_default();
            let then_method = match interp.get_object_property(obj_id, "then", &obj) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Completion::Throw(e),
                _ => JsValue::UNDEFINED,
            };
            interp.call_function(&then_method, this, &[JsValue::UNDEFINED, on_rejected])
        });

        // Promise.prototype.finally — spec 27.2.5.3
        self.define_method(proto_id, "finally", 1, |interp, this, args| {
            // Step 1-2: Let promise be the this value. If not Object, throw TypeError.
            let Some(promise_id) = this.as_object_id() else {
                return Completion::Throw(
                    interp.create_type_error("Promise.prototype.finally called on non-object"),
                );
            };

            // Step 3: Let C = ? SpeciesConstructor(promise, %Promise%).
            let promise_ctor = interp
                .get_global_var("Promise")
                .unwrap_or(JsValue::UNDEFINED);
            let c = match interp.species_constructor(this, &promise_ctor) {
                Ok(c) => c,
                Err(e) => return Completion::Throw(e),
            };

            let on_finally = args.first().cloned().unwrap_or(JsValue::UNDEFINED);

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
                        let value = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        // Step 6a.i: Let result = ? Call(onFinally, undefined).
                        let result =
                            interp.call_function(&on_finally_clone, &JsValue::UNDEFINED, &[]);
                        match result {
                            Completion::Throw(e) => Completion::Throw(e),
                            Completion::Normal(r) => {
                                // Step 6a.ii: Let promise = ? PromiseResolve(C, result).
                                let p = match interp.promise_resolve_with_constructor(&c_clone, &r)
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
                                interp.pin_native_root(&return_fn, &value);
                                // Step 6a.v: Return ? Invoke(promise, "then", « valueThunk »).
                                let p_id = p.as_object_id().unwrap_or_default();
                                let then_method = match interp.get_object_property(p_id, "then", &p)
                                {
                                    Completion::Normal(v) => v,
                                    Completion::Throw(e) => return Completion::Throw(e),
                                    _ => JsValue::UNDEFINED,
                                };
                                interp.call_function(&then_method, &p, &[return_fn])
                            }
                            _ => Completion::Normal(JsValue::UNDEFINED),
                        }
                    },
                ));

                // Step 6c: catchFinally closure
                let on_finally_clone2 = on_finally.clone();
                let c_clone2 = c.clone();
                let catch_finally = interp.create_function(JsFunction::native(
                    "".to_string(),
                    1,
                    move |interp, _this, args| {
                        let reason = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        // Step 6c.i: Let result = ? Call(onFinally, undefined).
                        let result =
                            interp.call_function(&on_finally_clone2, &JsValue::UNDEFINED, &[]);
                        match result {
                            Completion::Throw(e) => Completion::Throw(e),
                            Completion::Normal(r) => {
                                // Step 6c.ii: Let promise = ? PromiseResolve(C, result).
                                let p = match interp.promise_resolve_with_constructor(&c_clone2, &r)
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
                                interp.pin_native_root(&throw_fn, &reason);
                                // Step 6c.v: Return ? Invoke(promise, "then", « thrower »).
                                let p_id = p.as_object_id().unwrap_or_default();
                                let then_method = match interp.get_object_property(p_id, "then", &p)
                                {
                                    Completion::Normal(v) => v,
                                    Completion::Throw(e) => return Completion::Throw(e),
                                    _ => JsValue::UNDEFINED,
                                };
                                interp.call_function(&then_method, &p, &[throw_fn])
                            }
                            _ => Completion::Normal(JsValue::UNDEFINED),
                        }
                    },
                ));

                interp.pin_native_root(&then_finally, &on_finally);
                interp.pin_native_root(&then_finally, &c);
                interp.pin_native_root(&catch_finally, &on_finally);
                interp.pin_native_root(&catch_finally, &c);

                (then_finally, catch_finally)
            };

            // Step 7: Return ? Invoke(promise, "then", « thenFinally, catchFinally »).
            let then_method = match interp.get_object_property(promise_id, "then", this) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Completion::Throw(e),
                _ => JsValue::UNDEFINED,
            };
            interp.call_function(&then_method, this, &[then_finally, catch_finally])
        });

        // @@toStringTag
        self.define_to_string_tag(proto_id, "Promise");

        // Promise constructor
        let _promise_proto = self.realm().promise_prototype;
        let ctor = self.create_function(JsFunction::constructor(
            "Promise".to_string(),
            1,
            move |interp, _this, args| {
                // Step 1: If NewTarget is undefined, throw a TypeError
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Promise constructor requires 'new'"),
                    );
                }
                // Step 2: If IsCallable(executor) is false, throw a TypeError
                let executor = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if !interp.is_callable(&executor) {
                    let err = interp.create_type_error("Promise resolver is not a function");
                    return Completion::Throw(err);
                }
                // OrdinaryCreateFromConstructor — realm-aware prototype
                let proto = match interp
                    .get_prototype_from_new_target_realm(|realm| realm.promise_prototype)
                {
                    Ok(p) => p,
                    Err(e) => return Completion::Throw(e),
                };
                let promise = interp.create_promise_object();
                if let Some(p) = proto
                    && let Some(po) = promise.as_object_id()
                    && let Some(pobj) = interp.get_object_cell(po)
                {
                    pobj.borrow_mut().prototype_id = Some(p);
                }
                let promise_id = promise.as_object_id().unwrap_or_default();
                let (resolve_fn, reject_fn) = interp.create_resolving_functions(promise_id);
                let result = interp.call_function(
                    &executor,
                    &JsValue::UNDEFINED,
                    &[resolve_fn.clone(), reject_fn.clone()],
                );
                if let Completion::Throw(e) = result
                    && let Completion::Throw(e2) =
                        interp.call_function(&reject_fn, &JsValue::UNDEFINED, &[e])
                {
                    return Completion::Throw(e2);
                }
                Completion::Normal(promise)
            },
        ));

        // Mark Promise constructor as deferred_construct so construct_with_new_target
        // skips early prototype access (Promise checks callable before OrdinaryCreateFromConstructor).
        if let Some(o) = ctor.as_object_id()
            && let Some(func_obj) = self.get_object_cell(o)
        {
            func_obj.borrow_mut().deferred_construct = true;
        }

        // Set Promise.prototype on constructor
        if let Some(o) = ctor.as_object_id()
            && self.get_object_cell(o).is_some()
        {
            let ctor_id = o;
            self.get_object_cell_expect(ctor_id)
                .borrow_mut()
                .insert_property(
                    "prototype".to_string(),
                    PropertyDescriptor::data(JsValue::object(proto_id), false, false, false),
                );

            // Promise[Symbol.species] getter
            let species_getter = self.create_function(JsFunction::native(
                "get [Symbol.species]".to_string(),
                0,
                |_interp, this_val, _args| Completion::Normal(this_val.clone()),
            ));
            self.get_object_cell_expect(ctor_id)
                .borrow_mut()
                .insert_property(
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

        // Set constructor on prototype
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_property(
                "constructor".to_string(),
                PropertyDescriptor::data(ctor.clone(), true, false, true),
            );

        // Promise.resolve
        let resolve_fn = self.create_function(JsFunction::native(
            "resolve".to_string(),
            1,
            |interp, this, args| {
                if !(this).is_object() {
                    return Completion::Throw(
                        interp.create_type_error("Promise.resolve requires an object"),
                    );
                }
                let value = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                match interp.promise_resolve_with_constructor(this, &value) {
                    Ok(p) => Completion::Normal(p),
                    Err(e) => Completion::Throw(e),
                }
            },
        ));
        if let Some(o) = ctor.as_object_id()
            && let Some(func_obj) = self.get_object_cell(o)
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
                if !(this).is_object() {
                    return Completion::Throw(
                        interp.create_type_error("Promise.reject requires an object"),
                    );
                }
                let reason = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                match interp.new_promise_capability(this) {
                    Ok(cap) => {
                        if let Completion::Throw(e) =
                            interp.call_function(&cap.reject, &JsValue::UNDEFINED, &[reason])
                        {
                            return Completion::Throw(e);
                        }
                        Completion::Normal(cap.promise)
                    }
                    Err(e) => Completion::Throw(e),
                }
            },
        ));
        if let Some(o) = ctor.as_object_id()
            && let Some(func_obj) = self.get_object_cell(o)
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
                let iterable = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                interp.promise_all(this, &iterable)
            },
        ));
        if let Some(o) = ctor.as_object_id()
            && let Some(func_obj) = self.get_object_cell(o)
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
                let iterable = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                interp.promise_all_settled(this, &iterable)
            },
        ));
        if let Some(o) = ctor.as_object_id()
            && let Some(func_obj) = self.get_object_cell(o)
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
                let iterable = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                interp.promise_race(this, &iterable)
            },
        ));
        if let Some(o) = ctor.as_object_id()
            && let Some(func_obj) = self.get_object_cell(o)
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
                let iterable = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                interp.promise_any(this, &iterable)
            },
        ));
        if let Some(o) = ctor.as_object_id()
            && let Some(func_obj) = self.get_object_cell(o)
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
                    let result_id = interp.create_object_id();
                    interp
                        .get_object_cell_expect(result_id)
                        .borrow_mut()
                        .insert_value("promise".to_string(), cap.promise);
                    interp
                        .get_object_cell_expect(result_id)
                        .borrow_mut()
                        .insert_value("resolve".to_string(), cap.resolve);
                    interp
                        .get_object_cell_expect(result_id)
                        .borrow_mut()
                        .insert_value("reject".to_string(), cap.reject);
                    let id = result_id;
                    Completion::Normal(JsValue::object(id))
                }
                Err(e) => Completion::Throw(e),
            },
        ));
        if let Some(o) = ctor.as_object_id()
            && let Some(func_obj) = self.get_object_cell(o)
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
                let callback = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let call_args: Vec<JsValue> = if args.len() > 1 {
                    args[1..].to_vec()
                } else {
                    vec![]
                };
                if !interp.is_callable(&callback) {
                    let err = interp.create_type_error("Promise.try requires a callable");
                    if let Completion::Throw(e) =
                        interp.call_function(&cap.reject, &JsValue::UNDEFINED, &[err])
                    {
                        return Completion::Throw(e);
                    }
                    return Completion::Normal(cap.promise);
                }
                let result = interp.call_function(&callback, &JsValue::UNDEFINED, &call_args);
                match result {
                    Completion::Normal(v) => {
                        if let Completion::Throw(e) =
                            interp.call_function(&cap.resolve, &JsValue::UNDEFINED, &[v])
                        {
                            return Completion::Throw(e);
                        }
                    }
                    Completion::Throw(e) => {
                        if let Completion::Throw(e2) =
                            interp.call_function(&cap.reject, &JsValue::UNDEFINED, &[e])
                        {
                            return Completion::Throw(e2);
                        }
                    }
                    _ => {}
                }
                Completion::Normal(cap.promise)
            },
        ));
        if let Some(o) = ctor.as_object_id()
            && let Some(func_obj) = self.get_object_cell(o)
        {
            func_obj
                .borrow_mut()
                .insert_builtin("try".to_string(), try_fn);
        }

        // Promise.allKeyed (await-dictionary Stage-3 proposal)
        let all_keyed_fn = self.create_function(JsFunction::native(
            "allKeyed".to_string(),
            1,
            |interp, this, args| {
                let promises = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                interp.promise_all_keyed(this, &promises)
            },
        ));
        if let Some(o) = ctor.as_object_id()
            && let Some(func_obj) = self.get_object_cell(o)
        {
            func_obj
                .borrow_mut()
                .insert_builtin("allKeyed".to_string(), all_keyed_fn);
        }

        // Promise.allSettledKeyed (await-dictionary Stage-3 proposal)
        let all_settled_keyed_fn = self.create_function(JsFunction::native(
            "allSettledKeyed".to_string(),
            1,
            |interp, this, args| {
                let promises = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                interp.promise_all_settled_keyed(this, &promises)
            },
        ));
        if let Some(o) = ctor.as_object_id()
            && let Some(func_obj) = self.get_object_cell(o)
        {
            func_obj
                .borrow_mut()
                .insert_builtin("allSettledKeyed".to_string(), all_settled_keyed_fn);
        }

        // Register Promise as global
        self.realm()
            .global_env
            .borrow_mut()
            .declare("Promise", BindingKind::Var);
        let env = self.realm().global_env.clone();
        let _ = self.env_set(&env, "Promise", ctor);
    }

    pub(crate) fn create_promise_object(&mut self) -> JsValue {
        let mut data = JsObjectData::new();
        data.prototype_id = self.realm().promise_prototype;
        data.class_name = "Promise".to_string();
        data.kind = crate::interpreter::types::ObjectKind::Promise(PromiseData::new());
        let id = self.alloc_object(data);
        JsValue::object(id)
    }

    pub(crate) fn create_resolved_promise(&mut self, value: JsValue) -> Completion {
        let promise = self.create_promise_object();
        if let Some(o) = promise.as_object_id() {
            self.fulfill_promise(o, value);
        }
        Completion::Normal(promise)
    }

    pub(crate) fn create_rejected_promise(&mut self, reason: JsValue) -> Completion {
        let promise = self.create_promise_object();
        if let Some(o) = promise.as_object_id() {
            self.reject_promise(o, reason);
        }
        Completion::Normal(promise)
    }

    pub(crate) fn create_promise_parts(&mut self) -> (JsValue, JsValue, JsValue) {
        let promise = self.create_promise_object();
        let promise_id = promise.as_object_id().unwrap_or_default();
        let (resolve, reject) = self.create_resolving_functions(promise_id);
        (resolve, reject, promise)
    }

    pub(crate) fn create_resolving_functions(&mut self, promise_id: u64) -> (JsValue, JsValue) {
        let already_resolved = Rc::new(Cell::new(false));

        let ar1 = already_resolved.clone();
        let resolve_fn = self.create_function(JsFunction::native(
            "".to_string(),
            1,
            move |interp, _this, args| {
                if ar1.get() {
                    return Completion::Normal(JsValue::UNDEFINED);
                }
                ar1.set(true);
                let value = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                // If resolving with self, reject with TypeError
                if let Some(o) = value.as_object_id()
                    && o == promise_id
                {
                    let err = interp.create_type_error("A promise cannot be resolved with itself.");
                    interp.reject_promise(promise_id, err);
                    return Completion::Normal(JsValue::UNDEFINED);
                }
                // Check if value is a thenable
                if let Some(o) = value.as_object_id() {
                    // Spec step 8: Let then be Completion(Get(resolution, "then")).
                    // Step 9: If then is an abrupt completion, then
                    //   a. Return RejectPromise(promise, then.[[Value]]).
                    let then_val = match interp.get_object_property(o, "then", &value) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            interp.reject_promise(promise_id, e);
                            return Completion::Normal(JsValue::UNDEFINED);
                        }
                        _ => JsValue::UNDEFINED,
                    };
                    if interp.is_callable(&then_val) {
                        interp.promise_resolve_thenable(promise_id, value.clone(), then_val);
                        return Completion::Normal(JsValue::UNDEFINED);
                    }
                }
                interp.fulfill_promise(promise_id, value);
                Completion::Normal(JsValue::UNDEFINED)
            },
        ));

        let ar2 = already_resolved.clone();
        let reject_fn = self.create_function(JsFunction::native(
            "".to_string(),
            1,
            move |interp, _this, args| {
                if ar2.get() {
                    return Completion::Normal(JsValue::UNDEFINED);
                }
                ar2.set(true);
                let reason = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                interp.reject_promise(promise_id, reason);
                Completion::Normal(JsValue::UNDEFINED)
            },
        ));

        // The native closures above capture promise_id as a bare u64, which is
        // invisible to the GC, so the Promise must survive as long as either
        // resolving function is reachable.
        if promise_id != 0 {
            let pin = JsValue::object(promise_id);
            self.pin_native_root(&resolve_fn, &pin);
            self.pin_native_root(&reject_fn, &pin);
        }

        (resolve_fn, reject_fn)
    }

    pub(crate) fn fulfill_promise(&mut self, promise_id: u64, value: JsValue) {
        let reactions = if let Some(obj) = self.get_object_cell(promise_id) {
            let mut o = obj.borrow_mut();
            if let Some(pd) = o.promise_data_mut() {
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
        let reactions = if let Some(obj) = self.get_object_cell(promise_id) {
            let mut o = obj.borrow_mut();
            if let Some(pd) = o.promise_data_mut() {
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

    pub(crate) fn trigger_promise_reactions(
        &mut self,
        reactions: Vec<PromiseReaction>,
        argument: JsValue,
    ) {
        for reaction in reactions {
            let arg = argument.clone();
            let mut roots = vec![
                arg.clone(),
                reaction.resolve.clone(),
                reaction.reject.clone(),
            ];
            if let Some(ref h) = reaction.handler {
                roots.push(h.clone());
            }
            // Root the downstream promise so GC can trace it from the microtask queue.
            // The resolve/reject closures only capture promise_id as a u64, which is
            // invisible to the GC tracer.
            if let Some(pid) = reaction.promise_id {
                roots.push(JsValue::object(pid));
            }
            self.scheduler.enqueue_microtask((
                roots,
                Box::new(move |interp| {
                    let handler_result = if let Some(ref handler) = reaction.handler {
                        if interp.is_callable(handler) {
                            match interp.call_function(
                                handler,
                                &JsValue::UNDEFINED,
                                std::slice::from_ref(&arg),
                            ) {
                                Completion::Throw(e) => Err(e),
                                Completion::Normal(v) => Ok(v),
                                // A `__host_exit` inside the reaction handler
                                // (issue #242) propagates out of the job as
                                // `Completion::Exit` — the Promise reaction must
                                // not consume it into a fulfillment/rejection.
                                Completion::Exit(code) => return Completion::Exit(code),
                                _ => Ok(JsValue::UNDEFINED),
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
                                    &JsValue::UNDEFINED,
                                    &[value],
                                ) {
                                    return Completion::Throw(e);
                                }
                            }
                            Err(reason) => {
                                if let Completion::Throw(e) = interp.call_function(
                                    &reaction.reject,
                                    &JsValue::UNDEFINED,
                                    &[reason],
                                ) {
                                    return Completion::Throw(e);
                                }
                            }
                        }
                    }
                    Completion::Normal(JsValue::UNDEFINED)
                }),
            ));
        }
    }

    pub(crate) fn promise_resolve_thenable(
        &mut self,
        promise_id: u64,
        thenable: JsValue,
        then_fn: JsValue,
    ) {
        let (resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);
        let roots = vec![
            thenable.clone(),
            then_fn.clone(),
            resolve_fn.clone(),
            reject_fn.clone(),
            JsValue::object(promise_id),
        ];
        self.scheduler.enqueue_microtask((
            roots,
            Box::new(move |interp| {
                let result =
                    interp.call_function(&then_fn, &thenable, &[resolve_fn, reject_fn.clone()]);
                // A `__host_exit` inside the user `then` (issue #242) propagates
                // out of the job as `Completion::Exit` rather than being turned
                // into a rejection.
                if let Completion::Exit(code) = result {
                    return Completion::Exit(code);
                }
                if let Completion::Throw(e) = result
                    && let Completion::Throw(e2) =
                        interp.call_function(&reject_fn, &JsValue::UNDEFINED, &[e])
                {
                    return Completion::Throw(e2);
                }
                Completion::Normal(JsValue::UNDEFINED)
            }),
        ));
    }

    /// §27.2.5.4.1 PerformPromiseThen with a pre-created result capability.
    /// Skips SpeciesConstructor — uses the provided promise/resolve/reject directly.
    pub(crate) fn perform_promise_then(
        &mut self,
        promise_val: &JsValue,
        on_fulfilled: &JsValue,
        on_rejected: &JsValue,
        result_promise: JsValue,
        result_resolve: JsValue,
        result_reject: JsValue,
    ) -> Completion {
        let promise_id = if let Some(o) = promise_val.as_object_id() {
            o
        } else {
            return Completion::Throw(
                self.create_type_error("PerformPromiseThen called on non-promise"),
            );
        };
        let derived_id = result_promise.as_object_id().unwrap_or_default();

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
            resolve: result_resolve.clone(),
            reject: result_reject.clone(),
            reaction_type: PromiseReactionType::Fulfill,
        };
        let reject_reaction = PromiseReaction {
            handler: reject_handler,
            promise_id: Some(derived_id),
            resolve: result_resolve,
            reject: result_reject,
            reaction_type: PromiseReactionType::Reject,
        };

        let fulfill_reaction2 = fulfill_reaction.clone();
        let reject_reaction2 = reject_reaction.clone();
        let state = if let Some(obj) = self.get_object_cell(promise_id) {
            let mut o = obj.borrow_mut();
            if let Some(pd) = o.promise_data_mut() {
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

        Completion::Normal(result_promise)
    }

    pub(crate) fn promise_then(
        &mut self,
        promise_val: &JsValue,
        on_fulfilled: &JsValue,
        on_rejected: &JsValue,
    ) -> Completion {
        let promise_id = if let Some(o) = promise_val.as_object_id() {
            if let Some(obj) = self.get_object_cell(o) {
                if obj.borrow().promise_data().is_some() {
                    o
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
        let promise_ctor = self.get_global_var("Promise").unwrap_or(JsValue::UNDEFINED);
        let c = match self.species_constructor(promise_val, &promise_ctor) {
            Ok(c) => c,
            Err(e) => return Completion::Throw(e),
        };

        let cap = match self.new_promise_capability(&c) {
            Ok(cap) => cap,
            Err(e) => return Completion::Throw(e),
        };
        let derived = cap.promise;
        let derived_id = derived.as_object_id().unwrap_or_default();
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
        let state = if let Some(obj) = self.get_object_cell(promise_id) {
            let mut o = obj.borrow_mut();
            if let Some(pd) = o.promise_data_mut() {
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
        // §27.2.4.7.1 PromiseResolve(C, x): if IsPromise(x), check x.constructor === C
        if let Some(o) = value.as_object_id()
            && let Some(obj) = self.get_object_cell(o)
            && obj.borrow().promise_data().is_some()
        {
            match self.get_object_property(o, "constructor", value) {
                Completion::Normal(ctor) => {
                    let promise_ctor = self.get_global_var("Promise");
                    if let Some(ref pc) = promise_ctor
                        && strict_equality(&ctor, pc)
                    {
                        return value.clone();
                    }
                }
                Completion::Throw(e) => {
                    // Get(x, "constructor") threw — create rejected promise
                    let promise = self.create_promise_object();
                    let promise_id = promise.as_object_id().unwrap_or_default();
                    self.reject_promise(promise_id, e);
                    return promise;
                }
                _ => {}
            }
        }
        let promise = self.create_promise_object();
        let promise_id = promise.as_object_id().unwrap_or_default();
        // Check if value is a thenable using [[Get]] to trigger Proxy traps/getters
        if let Some(o) = value.as_object_id() {
            let then_val = match self.get_object_property(o, "then", value) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    self.reject_promise(promise_id, e);
                    return promise;
                }
                _ => JsValue::UNDEFINED,
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
        let ctor_id = constructor.as_object_id().unwrap_or_default();
        let promise_resolve = match self.get_object_property(ctor_id, "resolve", constructor) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => {
                return self.if_abrupt_reject_promise(e, &cap);
            }
            _ => JsValue::UNDEFINED,
        };
        if !self.is_callable(&promise_resolve) {
            let err = self.create_type_error("Promise resolve is not a function");
            return self.if_abrupt_reject_promise(err, &cap);
        }

        // GetIterator(iterable)
        let iterator = match self.get_iterator(iterable) {
            Ok(iter) => iter,
            Err(e) => return self.if_abrupt_reject_promise(e, &cap),
        };

        // Accumulated element values are pinned on this fresh, JS-unreachable
        // object rather than on the capability function. A custom constructor may
        // hand the same resolving function to every capability it builds, and
        // pins are never removed, so anchoring there would retain one settled
        // value per call on a long-lived object — unbounded growth. Every element
        // function pins this anchor, so it outlives them all and dies with them.
        let value_anchor = JsValue::object(self.create_object_id());
        let remaining = Rc::new(Cell::new(1)); // starts at 1 per spec (decremented at end)
        let results: Rc<RefCell<Vec<JsValue>>> = Rc::new(RefCell::new(Vec::new()));
        let mut index = 0usize;

        loop {
            // IteratorStep — per spec, if this throws, iteratorRecord.[[done]] = true,
            // so we do NOT call IteratorClose.
            let next = match self.iterator_step(&iterator) {
                Ok(Some(result)) => result,
                Ok(None) => {
                    let r = remaining.get() - 1;
                    remaining.set(r);
                    if r == 0 {
                        let values = results.borrow().clone();
                        let arr = self.create_array(values);
                        if let Completion::Throw(e) =
                            self.call_function(&cap.resolve, &JsValue::UNDEFINED, &[arr])
                        {
                            return self.if_abrupt_reject_promise(e, &cap);
                        }
                    }
                    return Completion::Normal(cap.promise);
                }
                Err(e) => {
                    return self.if_abrupt_reject_promise(e, &cap);
                }
            };

            // IteratorValue — per spec, if this throws, iteratorRecord.[[done]] = true,
            // so we do NOT call IteratorClose.
            let next_value = match self.iterator_value(&next) {
                Ok(v) => v,
                Err(e) => {
                    return self.if_abrupt_reject_promise(e, &cap);
                }
            };

            results.borrow_mut().push(JsValue::UNDEFINED);
            remaining.set(remaining.get() + 1);

            let p = match self.call_function(&promise_resolve, constructor, &[next_value]) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    self.iterator_close(&iterator, e.clone());
                    return self.if_abrupt_reject_promise(e, &cap);
                }
                _ => JsValue::UNDEFINED,
            };

            let i = index;
            let remaining = remaining.clone();
            let results = results.clone();
            let resolve_fn = cap.resolve.clone();
            let anchor = value_anchor.clone();
            let already_called = Rc::new(Cell::new(false));

            let ac = already_called.clone();
            let on_fulfilled = self.create_function(JsFunction::native(
                "".to_string(),
                1,
                move |interp, _this, args| {
                    if ac.get() {
                        return Completion::Normal(JsValue::UNDEFINED);
                    }
                    ac.set(true);
                    let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    interp.pin_native_root(&anchor, &val);
                    results.borrow_mut()[i] = val;
                    let r = remaining.get() - 1;
                    remaining.set(r);
                    if r == 0 {
                        let values = results.borrow().clone();
                        let arr = interp.create_array(values);
                        if let Completion::Throw(e) =
                            interp.call_function(&resolve_fn, &JsValue::UNDEFINED, &[arr])
                        {
                            return Completion::Throw(e);
                        }
                    }
                    Completion::Normal(JsValue::UNDEFINED)
                },
            ));
            self.pin_native_root(&on_fulfilled, &cap.resolve);
            self.pin_native_root(&on_fulfilled, &value_anchor);

            let reject_fn_clone = cap.reject.clone();
            let p_id = p.as_object_id().unwrap_or_default();
            let then_fn = match self.get_object_property(p_id, "then", &p) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    self.iterator_close(&iterator, e.clone());
                    return self.if_abrupt_reject_promise(e, &cap);
                }
                _ => JsValue::UNDEFINED,
            };
            if let Completion::Throw(e) =
                self.call_function(&then_fn, &p, &[on_fulfilled, reject_fn_clone])
            {
                self.iterator_close(&iterator, e.clone());
                return self.if_abrupt_reject_promise(e, &cap);
            }

            index += 1;
        }
    }

    fn promise_all_settled(&mut self, constructor: &JsValue, iterable: &JsValue) -> Completion {
        let cap = match self.new_promise_capability(constructor) {
            Ok(cap) => cap,
            Err(e) => return Completion::Throw(e),
        };

        let ctor_id = constructor.as_object_id().unwrap_or_default();
        let promise_resolve = match self.get_object_property(ctor_id, "resolve", constructor) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return self.if_abrupt_reject_promise(e, &cap),
            _ => JsValue::UNDEFINED,
        };
        if !self.is_callable(&promise_resolve) {
            let err = self.create_type_error("Promise resolve is not a function");
            return self.if_abrupt_reject_promise(err, &cap);
        }

        let iterator = match self.get_iterator(iterable) {
            Ok(iter) => iter,
            Err(e) => return self.if_abrupt_reject_promise(e, &cap),
        };

        // Fresh, JS-unreachable anchor for the accumulated values; see promise_all
        // for why they must not hang off the capability function.
        let value_anchor = JsValue::object(self.create_object_id());
        let remaining = Rc::new(Cell::new(1));
        let results: Rc<RefCell<Vec<JsValue>>> = Rc::new(RefCell::new(Vec::new()));
        let mut index = 0usize;

        loop {
            // IteratorStep — no IteratorClose on error (done = true)
            let next = match self.iterator_step(&iterator) {
                Ok(Some(result)) => result,
                Ok(None) => {
                    let r = remaining.get() - 1;
                    remaining.set(r);
                    if r == 0 {
                        let values = results.borrow().clone();
                        let arr = self.create_array(values);
                        if let Completion::Throw(e) =
                            self.call_function(&cap.resolve, &JsValue::UNDEFINED, &[arr])
                        {
                            return self.if_abrupt_reject_promise(e, &cap);
                        }
                    }
                    return Completion::Normal(cap.promise);
                }
                Err(e) => {
                    return self.if_abrupt_reject_promise(e, &cap);
                }
            };

            // IteratorValue — no IteratorClose on error (done = true)
            let next_value = match self.iterator_value(&next) {
                Ok(v) => v,
                Err(e) => {
                    return self.if_abrupt_reject_promise(e, &cap);
                }
            };

            results.borrow_mut().push(JsValue::UNDEFINED);
            remaining.set(remaining.get() + 1);

            let p = match self.call_function(&promise_resolve, constructor, &[next_value]) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    self.iterator_close(&iterator, e.clone());
                    return self.if_abrupt_reject_promise(e, &cap);
                }
                _ => JsValue::UNDEFINED,
            };

            let i = index;
            let remaining_f = remaining.clone();
            let results_f = results.clone();
            let resolve_fn_f = cap.resolve.clone();
            let remaining_r = remaining.clone();
            let results_r = results.clone();
            let resolve_fn_r = cap.resolve.clone();
            let anchor_f = value_anchor.clone();
            let anchor_r = value_anchor.clone();
            let already_called = Rc::new(Cell::new(false));

            let ac_f = already_called.clone();
            let on_fulfilled = self.create_function(JsFunction::native(
                "".to_string(),
                1,
                move |interp, _this, args| {
                    if ac_f.get() {
                        return Completion::Normal(JsValue::UNDEFINED);
                    }
                    ac_f.set(true);
                    let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let obj_id = interp.create_object_id();
                    {
                        let mut o = interp.get_object_cell_expect(obj_id).borrow_mut();
                        o.insert_value(
                            "status".to_string(),
                            JsValue::string(JsString::from_str("fulfilled")),
                        );
                        o.insert_value("value".to_string(), val);
                    }
                    let record = JsValue::object(obj_id);
                    interp.pin_native_root(&anchor_f, &record);
                    results_f.borrow_mut()[i] = record;
                    let r = remaining_f.get() - 1;
                    remaining_f.set(r);
                    if r == 0 {
                        let values = results_f.borrow().clone();
                        let arr = interp.create_array(values);
                        if let Completion::Throw(e) =
                            interp.call_function(&resolve_fn_f, &JsValue::UNDEFINED, &[arr])
                        {
                            return Completion::Throw(e);
                        }
                    }
                    Completion::Normal(JsValue::UNDEFINED)
                },
            ));
            let ac_r = already_called.clone();
            let on_rejected = self.create_function(JsFunction::native(
                "".to_string(),
                1,
                move |interp, _this, args| {
                    if ac_r.get() {
                        return Completion::Normal(JsValue::UNDEFINED);
                    }
                    ac_r.set(true);
                    let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let obj_id = interp.create_object_id();
                    {
                        let mut o = interp.get_object_cell_expect(obj_id).borrow_mut();
                        o.insert_value(
                            "status".to_string(),
                            JsValue::string(JsString::from_str("rejected")),
                        );
                        o.insert_value("reason".to_string(), val);
                    }
                    let record = JsValue::object(obj_id);
                    interp.pin_native_root(&anchor_r, &record);
                    results_r.borrow_mut()[i] = record;
                    let r = remaining_r.get() - 1;
                    remaining_r.set(r);
                    if r == 0 {
                        let values = results_r.borrow().clone();
                        let arr = interp.create_array(values);
                        if let Completion::Throw(e) =
                            interp.call_function(&resolve_fn_r, &JsValue::UNDEFINED, &[arr])
                        {
                            return Completion::Throw(e);
                        }
                    }
                    Completion::Normal(JsValue::UNDEFINED)
                },
            ));

            self.pin_native_root(&on_fulfilled, &cap.resolve);
            self.pin_native_root(&on_rejected, &cap.resolve);
            self.pin_native_root(&on_fulfilled, &value_anchor);
            self.pin_native_root(&on_rejected, &value_anchor);

            let p_id = p.as_object_id().unwrap_or_default();
            let then_fn = match self.get_object_property(p_id, "then", &p) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    self.iterator_close(&iterator, e.clone());
                    return self.if_abrupt_reject_promise(e, &cap);
                }
                _ => JsValue::UNDEFINED,
            };
            if let Completion::Throw(e) =
                self.call_function(&then_fn, &p, &[on_fulfilled, on_rejected])
            {
                self.iterator_close(&iterator, e.clone());
                return self.if_abrupt_reject_promise(e, &cap);
            }

            index += 1;
        }
    }

    /// Promise.allKeyed (await-dictionary Stage-3 proposal).
    fn promise_all_keyed(&mut self, constructor: &JsValue, promises: &JsValue) -> Completion {
        let cap = match self.new_promise_capability(constructor) {
            Ok(cap) => cap,
            Err(e) => return Completion::Throw(e),
        };

        // GetPromiseResolve(C) + IfAbruptRejectPromise
        let ctor_id = constructor.as_object_id().unwrap_or_default();
        let promise_resolve = match self.get_object_property(ctor_id, "resolve", constructor) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return self.if_abrupt_reject_promise(e, &cap),
            _ => JsValue::UNDEFINED,
        };
        if !self.is_callable(&promise_resolve) {
            let err = self.create_type_error("Promise resolve is not a function");
            return self.if_abrupt_reject_promise(err, &cap);
        }

        // If `promises` is not an Object, reject the returned promise.
        let Some(promises_obj_id) = promises.as_object_id() else {
            let err = self.create_type_error("Promise.allKeyed argument is not an object");
            return self.if_abrupt_reject_promise(err, &cap);
        };

        // allKeys = ? promises.[[OwnPropertyKeys]]()
        let all_keys = match self.proxy_own_keys(promises_obj_id) {
            Ok(k) => k,
            Err(e) => return self.if_abrupt_reject_promise(e, &cap),
        };

        // Fresh, JS-unreachable anchor for the accumulated values; see promise_all
        // for why they must not hang off the capability function.
        let value_anchor = JsValue::object(self.create_object_id());
        let remaining = Rc::new(Cell::new(1u64));
        let keys: Rc<RefCell<Vec<JsPropertyKey>>> = Rc::new(RefCell::new(Vec::new()));
        let values: Rc<RefCell<Vec<JsValue>>> = Rc::new(RefCell::new(Vec::new()));

        for key_val in all_keys {
            let key_str = to_property_key_string(&key_val);

            // desc = ? promises.[[GetOwnProperty]](key)
            let desc_val = match self.proxy_get_own_property_descriptor(promises_obj_id, &key_str) {
                Ok(v) => v,
                Err(e) => return self.if_abrupt_reject_promise(e, &cap),
            };
            if (desc_val).is_undefined() {
                continue;
            }
            // Spec: process only when desc.[[Enumerable]] is true. After
            // ToPropertyDescriptor an absent enumerable defaults to false
            // (via CompletePropertyDescriptor), so a missing field must skip.
            let is_enumerable = match self.to_property_descriptor(&desc_val) {
                Ok(d) => d.enumerable == Some(true),
                Err(Some(e)) => return self.if_abrupt_reject_promise(e, &cap),
                Err(None) => false,
            };
            if !is_enumerable {
                continue;
            }

            // nextValue = ? Get(promises, key)
            let next_value = match self.get_object_property(promises_obj_id, &key_str, promises) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return self.if_abrupt_reject_promise(e, &cap),
                _ => JsValue::UNDEFINED,
            };

            let i = values.borrow().len();
            keys.borrow_mut().push(key_str.clone());
            values.borrow_mut().push(JsValue::UNDEFINED);
            remaining.set(remaining.get() + 1);

            let p = match self.call_function(&promise_resolve, constructor, &[next_value]) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return self.if_abrupt_reject_promise(e, &cap),
                _ => JsValue::UNDEFINED,
            };

            let remaining_c = remaining.clone();
            let keys_c = keys.clone();
            let values_c = values.clone();
            let resolve_fn = cap.resolve.clone();
            let anchor = value_anchor.clone();
            let already_called = Rc::new(Cell::new(false));
            let ac = already_called.clone();

            let on_fulfilled = self.create_function(JsFunction::native(
                "".to_string(),
                1,
                move |interp, _this, args| {
                    if ac.get() {
                        return Completion::Normal(JsValue::UNDEFINED);
                    }
                    ac.set(true);
                    let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    interp.pin_native_root(&anchor, &val);
                    values_c.borrow_mut()[i] = val;
                    let r = remaining_c.get() - 1;
                    remaining_c.set(r);
                    if r == 0 {
                        let keys_snapshot = keys_c.borrow().clone();
                        let values_snapshot = values_c.borrow().clone();
                        let result = interp.build_keyed_result(&keys_snapshot, &values_snapshot);
                        if let Completion::Throw(e) =
                            interp.call_function(&resolve_fn, &JsValue::UNDEFINED, &[result])
                        {
                            return Completion::Throw(e);
                        }
                    }
                    Completion::Normal(JsValue::UNDEFINED)
                },
            ));

            self.pin_native_root(&on_fulfilled, &cap.resolve);
            self.pin_native_root(&on_fulfilled, &value_anchor);

            let p_id = p.as_object_id().unwrap_or_default();
            let then_fn = match self.get_object_property(p_id, "then", &p) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return self.if_abrupt_reject_promise(e, &cap),
                _ => JsValue::UNDEFINED,
            };
            if let Completion::Throw(e) =
                self.call_function(&then_fn, &p, &[on_fulfilled, cap.reject.clone()])
            {
                return self.if_abrupt_reject_promise(e, &cap);
            }
        }

        // Final decrement: matches the spec's "starts at 1" pattern.
        let r = remaining.get() - 1;
        remaining.set(r);
        if r == 0 {
            let keys_snapshot = keys.borrow().clone();
            let values_snapshot = values.borrow().clone();
            let result = self.build_keyed_result(&keys_snapshot, &values_snapshot);
            if let Completion::Throw(e) =
                self.call_function(&cap.resolve, &JsValue::UNDEFINED, &[result])
            {
                return self.if_abrupt_reject_promise(e, &cap);
            }
        }

        Completion::Normal(cap.promise)
    }

    /// Promise.allSettledKeyed (await-dictionary Stage-3 proposal).
    fn promise_all_settled_keyed(
        &mut self,
        constructor: &JsValue,
        promises: &JsValue,
    ) -> Completion {
        let cap = match self.new_promise_capability(constructor) {
            Ok(cap) => cap,
            Err(e) => return Completion::Throw(e),
        };

        let ctor_id = constructor.as_object_id().unwrap_or_default();
        let promise_resolve = match self.get_object_property(ctor_id, "resolve", constructor) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return self.if_abrupt_reject_promise(e, &cap),
            _ => JsValue::UNDEFINED,
        };
        if !self.is_callable(&promise_resolve) {
            let err = self.create_type_error("Promise resolve is not a function");
            return self.if_abrupt_reject_promise(err, &cap);
        }

        let Some(promises_obj_id) = promises.as_object_id() else {
            let err = self.create_type_error("Promise.allSettledKeyed argument is not an object");
            return self.if_abrupt_reject_promise(err, &cap);
        };

        let all_keys = match self.proxy_own_keys(promises_obj_id) {
            Ok(k) => k,
            Err(e) => return self.if_abrupt_reject_promise(e, &cap),
        };

        // Fresh, JS-unreachable anchor for the accumulated values; see promise_all
        // for why they must not hang off the capability function.
        let value_anchor = JsValue::object(self.create_object_id());
        let remaining = Rc::new(Cell::new(1u64));
        let keys: Rc<RefCell<Vec<JsPropertyKey>>> = Rc::new(RefCell::new(Vec::new()));
        let values: Rc<RefCell<Vec<JsValue>>> = Rc::new(RefCell::new(Vec::new()));

        for key_val in all_keys {
            let key_str = to_property_key_string(&key_val);

            let desc_val = match self.proxy_get_own_property_descriptor(promises_obj_id, &key_str) {
                Ok(v) => v,
                Err(e) => return self.if_abrupt_reject_promise(e, &cap),
            };
            if (desc_val).is_undefined() {
                continue;
            }
            // Spec: process only when desc.[[Enumerable]] is true (absent
            // defaults to false via CompletePropertyDescriptor).
            let is_enumerable = match self.to_property_descriptor(&desc_val) {
                Ok(d) => d.enumerable == Some(true),
                Err(Some(e)) => return self.if_abrupt_reject_promise(e, &cap),
                Err(None) => false,
            };
            if !is_enumerable {
                continue;
            }

            let next_value = match self.get_object_property(promises_obj_id, &key_str, promises) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return self.if_abrupt_reject_promise(e, &cap),
                _ => JsValue::UNDEFINED,
            };

            let i = values.borrow().len();
            keys.borrow_mut().push(key_str.clone());
            values.borrow_mut().push(JsValue::UNDEFINED);
            remaining.set(remaining.get() + 1);

            let p = match self.call_function(&promise_resolve, constructor, &[next_value]) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return self.if_abrupt_reject_promise(e, &cap),
                _ => JsValue::UNDEFINED,
            };

            let already_called = Rc::new(Cell::new(false));

            let remaining_f = remaining.clone();
            let keys_f = keys.clone();
            let values_f = values.clone();
            let resolve_fn_f = cap.resolve.clone();
            let anchor_f = value_anchor.clone();
            let ac_f = already_called.clone();
            let on_fulfilled = self.create_function(JsFunction::native(
                "".to_string(),
                1,
                move |interp, _this, args| {
                    if ac_f.get() {
                        return Completion::Normal(JsValue::UNDEFINED);
                    }
                    ac_f.set(true);
                    let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let obj_id = interp.create_object_id();
                    {
                        let mut o = interp.get_object_cell_expect(obj_id).borrow_mut();
                        o.insert_value(
                            "status".to_string(),
                            JsValue::string(JsString::from_str("fulfilled")),
                        );
                        o.insert_value("value".to_string(), val);
                    }
                    let record = JsValue::object(obj_id);
                    interp.pin_native_root(&anchor_f, &record);
                    values_f.borrow_mut()[i] = record;
                    let r = remaining_f.get() - 1;
                    remaining_f.set(r);
                    if r == 0 {
                        let keys_snapshot = keys_f.borrow().clone();
                        let values_snapshot = values_f.borrow().clone();
                        let result = interp.build_keyed_result(&keys_snapshot, &values_snapshot);
                        if let Completion::Throw(e) =
                            interp.call_function(&resolve_fn_f, &JsValue::UNDEFINED, &[result])
                        {
                            return Completion::Throw(e);
                        }
                    }
                    Completion::Normal(JsValue::UNDEFINED)
                },
            ));

            let remaining_r = remaining.clone();
            let keys_r = keys.clone();
            let values_r = values.clone();
            let resolve_fn_r = cap.resolve.clone();
            let anchor_r = value_anchor.clone();
            let ac_r = already_called.clone();
            let on_rejected = self.create_function(JsFunction::native(
                "".to_string(),
                1,
                move |interp, _this, args| {
                    if ac_r.get() {
                        return Completion::Normal(JsValue::UNDEFINED);
                    }
                    ac_r.set(true);
                    let reason = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let obj_id = interp.create_object_id();
                    {
                        let mut o = interp.get_object_cell_expect(obj_id).borrow_mut();
                        o.insert_value(
                            "status".to_string(),
                            JsValue::string(JsString::from_str("rejected")),
                        );
                        o.insert_value("reason".to_string(), reason);
                    }
                    let record = JsValue::object(obj_id);
                    interp.pin_native_root(&anchor_r, &record);
                    values_r.borrow_mut()[i] = record;
                    let r = remaining_r.get() - 1;
                    remaining_r.set(r);
                    if r == 0 {
                        let keys_snapshot = keys_r.borrow().clone();
                        let values_snapshot = values_r.borrow().clone();
                        let result = interp.build_keyed_result(&keys_snapshot, &values_snapshot);
                        if let Completion::Throw(e) =
                            interp.call_function(&resolve_fn_r, &JsValue::UNDEFINED, &[result])
                        {
                            return Completion::Throw(e);
                        }
                    }
                    Completion::Normal(JsValue::UNDEFINED)
                },
            ));

            self.pin_native_root(&on_fulfilled, &cap.resolve);
            self.pin_native_root(&on_rejected, &cap.resolve);
            self.pin_native_root(&on_fulfilled, &value_anchor);
            self.pin_native_root(&on_rejected, &value_anchor);

            let p_id = p.as_object_id().unwrap_or_default();
            let then_fn = match self.get_object_property(p_id, "then", &p) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return self.if_abrupt_reject_promise(e, &cap),
                _ => JsValue::UNDEFINED,
            };
            if let Completion::Throw(e) =
                self.call_function(&then_fn, &p, &[on_fulfilled, on_rejected])
            {
                return self.if_abrupt_reject_promise(e, &cap);
            }
        }

        let r = remaining.get() - 1;
        remaining.set(r);
        if r == 0 {
            let keys_snapshot = keys.borrow().clone();
            let values_snapshot = values.borrow().clone();
            let result = self.build_keyed_result(&keys_snapshot, &values_snapshot);
            if let Completion::Throw(e) =
                self.call_function(&cap.resolve, &JsValue::UNDEFINED, &[result])
            {
                return self.if_abrupt_reject_promise(e, &cap);
            }
        }

        Completion::Normal(cap.promise)
    }

    /// Build a null-prototype object mapping `keys[i] → values[i]`.
    /// Used by Promise.allKeyed / Promise.allSettledKeyed to produce the
    /// CreateKeyedPromiseCombinatorResultObject result.
    fn build_keyed_result(&mut self, keys: &[JsPropertyKey], values: &[JsValue]) -> JsValue {
        let id = self.create_object_id();
        {
            let cell = self.get_object_cell_expect(id);
            let mut o = cell.borrow_mut();
            o.prototype_id = None;
            for (k, v) in keys.iter().zip(values.iter()) {
                o.insert_value(k.clone(), v.clone());
            }
        }
        JsValue::object(id)
    }

    fn promise_race(&mut self, constructor: &JsValue, iterable: &JsValue) -> Completion {
        let cap = match self.new_promise_capability(constructor) {
            Ok(cap) => cap,
            Err(e) => return Completion::Throw(e),
        };

        let ctor_id = constructor.as_object_id().unwrap_or_default();
        let promise_resolve = match self.get_object_property(ctor_id, "resolve", constructor) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return self.if_abrupt_reject_promise(e, &cap),
            _ => JsValue::UNDEFINED,
        };
        if !self.is_callable(&promise_resolve) {
            let err = self.create_type_error("Promise resolve is not a function");
            return self.if_abrupt_reject_promise(err, &cap);
        }

        let iterator = match self.get_iterator(iterable) {
            Ok(iter) => iter,
            Err(e) => return self.if_abrupt_reject_promise(e, &cap),
        };

        loop {
            // IteratorStep — no IteratorClose on error (done = true)
            let next = match self.iterator_step(&iterator) {
                Ok(Some(result)) => result,
                Ok(None) => return Completion::Normal(cap.promise),
                Err(e) => {
                    return self.if_abrupt_reject_promise(e, &cap);
                }
            };

            // IteratorValue — no IteratorClose on error (done = true)
            let next_value = match self.iterator_value(&next) {
                Ok(v) => v,
                Err(e) => {
                    return self.if_abrupt_reject_promise(e, &cap);
                }
            };

            let p = match self.call_function(&promise_resolve, constructor, &[next_value]) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    self.iterator_close(&iterator, e.clone());
                    return self.if_abrupt_reject_promise(e, &cap);
                }
                _ => JsValue::UNDEFINED,
            };

            let p_id = p.as_object_id().unwrap_or_default();
            let then_fn = match self.get_object_property(p_id, "then", &p) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    self.iterator_close(&iterator, e.clone());
                    return self.if_abrupt_reject_promise(e, &cap);
                }
                _ => JsValue::UNDEFINED,
            };
            if let Completion::Throw(e) =
                self.call_function(&then_fn, &p, &[cap.resolve.clone(), cap.reject.clone()])
            {
                self.iterator_close(&iterator, e.clone());
                return self.if_abrupt_reject_promise(e, &cap);
            }
        }
    }

    fn promise_any(&mut self, constructor: &JsValue, iterable: &JsValue) -> Completion {
        let cap = match self.new_promise_capability(constructor) {
            Ok(cap) => cap,
            Err(e) => return Completion::Throw(e),
        };

        let ctor_id = constructor.as_object_id().unwrap_or_default();
        let promise_resolve = match self.get_object_property(ctor_id, "resolve", constructor) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return self.if_abrupt_reject_promise(e, &cap),
            _ => JsValue::UNDEFINED,
        };
        if !self.is_callable(&promise_resolve) {
            let err = self.create_type_error("Promise resolve is not a function");
            return self.if_abrupt_reject_promise(err, &cap);
        }

        let iterator = match self.get_iterator(iterable) {
            Ok(iter) => iter,
            Err(e) => return self.if_abrupt_reject_promise(e, &cap),
        };

        // Fresh, JS-unreachable anchor for the accumulated values; see promise_all
        // for why they must not hang off the capability function.
        let value_anchor = JsValue::object(self.create_object_id());
        let remaining = Rc::new(Cell::new(1));
        let errors: Rc<RefCell<Vec<JsValue>>> = Rc::new(RefCell::new(Vec::new()));
        let mut index = 0usize;

        loop {
            // IteratorStep — no IteratorClose on error (done = true)
            let next = match self.iterator_step(&iterator) {
                Ok(Some(result)) => result,
                Ok(None) => {
                    let r = remaining.get() - 1;
                    remaining.set(r);
                    if r == 0 {
                        let errs = errors.borrow().clone();
                        let err = self.create_aggregate_error(errs, "All promises were rejected");
                        return self.if_abrupt_reject_promise(err, &cap);
                    }
                    return Completion::Normal(cap.promise);
                }
                Err(e) => {
                    return self.if_abrupt_reject_promise(e, &cap);
                }
            };

            // IteratorValue — no IteratorClose on error (done = true)
            let next_value = match self.iterator_value(&next) {
                Ok(v) => v,
                Err(e) => {
                    return self.if_abrupt_reject_promise(e, &cap);
                }
            };

            errors.borrow_mut().push(JsValue::UNDEFINED);
            remaining.set(remaining.get() + 1);

            let p = match self.call_function(&promise_resolve, constructor, &[next_value]) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    self.iterator_close(&iterator, e.clone());
                    return self.if_abrupt_reject_promise(e, &cap);
                }
                _ => JsValue::UNDEFINED,
            };

            let i = index;
            let remaining = remaining.clone();
            let errors = errors.clone();
            let reject_fn_clone = cap.reject.clone();
            let anchor = value_anchor.clone();
            let already_called = Rc::new(Cell::new(false));

            let ac = already_called.clone();
            let on_rejected = self.create_function(JsFunction::native(
                "".to_string(),
                1,
                move |interp, _this, args| {
                    if ac.get() {
                        return Completion::Normal(JsValue::UNDEFINED);
                    }
                    ac.set(true);
                    let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    interp.pin_native_root(&anchor, &val);
                    errors.borrow_mut()[i] = val;
                    let r = remaining.get() - 1;
                    remaining.set(r);
                    if r == 0 {
                        let errs = errors.borrow().clone();
                        let err = interp.create_aggregate_error(errs, "All promises were rejected");
                        if let Completion::Throw(e) =
                            interp.call_function(&reject_fn_clone, &JsValue::UNDEFINED, &[err])
                        {
                            return Completion::Throw(e);
                        }
                    }
                    Completion::Normal(JsValue::UNDEFINED)
                },
            ));

            self.pin_native_root(&on_rejected, &cap.reject);
            self.pin_native_root(&on_rejected, &value_anchor);

            let p_id = p.as_object_id().unwrap_or_default();
            let then_fn = match self.get_object_property(p_id, "then", &p) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => {
                    self.iterator_close(&iterator, e.clone());
                    return self.if_abrupt_reject_promise(e, &cap);
                }
                _ => JsValue::UNDEFINED,
            };
            if let Completion::Throw(e) =
                self.call_function(&then_fn, &p, &[cap.resolve.clone(), on_rejected])
            {
                self.iterator_close(&iterator, e.clone());
                return self.if_abrupt_reject_promise(e, &cap);
            }

            index += 1;
        }
    }

    fn create_aggregate_error(&mut self, errors: Vec<JsValue>, message: &str) -> JsValue {
        let obj_id = self.create_object_id();
        {
            let mut o = self.get_object_cell_expect(obj_id).borrow_mut();
            o.class_name = "AggregateError".to_string();
            if let Some(proto_id) = self.realm().aggregate_error_prototype {
                o.prototype_id = Some(proto_id);
            }
            o.insert_builtin(
                "message".to_string(),
                JsValue::string(JsString::from_str(message)),
            );
        }
        let errors_arr = self.create_array(errors);
        self.get_object_cell_expect(obj_id)
            .borrow_mut()
            .insert_builtin("errors".to_string(), errors_arr);
        let id = obj_id;
        JsValue::object(id)
    }

    pub(crate) fn is_callable(&self, val: &JsValue) -> bool {
        if let Some(o) = val.as_object_id()
            && let Some(obj) = self.get_object_cell(o)
        {
            if obj.borrow().callable.is_some() {
                return true;
            }
            // Proxy wrapping a callable is callable
            if obj.borrow().is_proxy() {
                let target_val = self.get_proxy_target_val(o);
                return self.is_callable(&target_val);
            }
        }
        false
    }

    pub(crate) fn is_constructor(&self, val: &JsValue) -> bool {
        if let Some(o) = val.as_object_id()
            && let Some(obj) = self.get_object_cell(o)
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
                let target_val = self.get_proxy_target_val(o);
                return self.is_constructor(&target_val);
            }
        }
        false
    }

    pub(crate) fn is_promise(&self, val: &JsValue) -> bool {
        if let Some(o) = val.as_object_id()
            && let Some(obj) = self.get_object_cell(o)
        {
            return obj.borrow().promise_data().is_some();
        }
        false
    }

    pub(crate) fn get_promise_state(&self, promise_id: u64) -> Option<PromiseState> {
        if let Some(obj) = self.get_object_cell(promise_id)
            && let Some(pd) = obj.borrow().promise_data()
        {
            return Some(pd.state.clone());
        }
        None
    }
}
