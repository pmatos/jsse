use super::*;
use crate::interpreter::gc::RootedSlots;
use std::cell::Cell;

mod construct;
use construct::promise_construct;

fn is_same_object(intrinsic: Option<&JsValue>, value: &JsValue) -> bool {
    match (
        value.as_object_id(),
        intrinsic.and_then(JsValue::as_object_id),
    ) {
        (Some(id), Some(intrinsic)) => id == intrinsic,
        _ => false,
    }
}

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

    /// Shared synchronous setup and result convention of the Promise combinators.
    /// NewPromiseCapability failures throw; subsequent setup or operation errors
    /// reject the created capability. The operation retains its IteratorClose
    /// policy and pins any captures that must outlive this GC Root Scope.
    fn run_promise_combinator(
        &mut self,
        constructor: &JsValue,
        operation: impl FnOnce(&mut Self, &PromiseCapability, &JsValue) -> Result<(), JsValue>,
    ) -> Completion {
        let cap = match self.new_promise_capability(constructor) {
            Ok(cap) => cap,
            Err(error) => return Completion::Throw(error),
        };
        self.with_gc_root_scope(|interp| {
            interp.gc_root_value(&cap.promise);
            interp.gc_root_value(&cap.resolve);
            interp.gc_root_value(&cap.reject);

            // GetPromiseResolve runs user code after capability creation, and
            // may return a fresh function owned only by this native operation.
            let ctor_id = constructor.as_object_id().unwrap_or_default();
            let promise_resolve = match interp.get_object_property(ctor_id, "resolve", constructor)
            {
                Completion::Normal(value) => value,
                Completion::Throw(error) => return interp.if_abrupt_reject_promise(error, &cap),
                _ => JsValue::UNDEFINED,
            };
            interp.gc_root_value(&promise_resolve);
            if !interp.is_callable(&promise_resolve) {
                let error = interp.create_type_error("Promise resolve is not a function");
                return interp.if_abrupt_reject_promise(error, &cap);
            }

            match operation(interp, &cap, &promise_resolve) {
                Ok(()) => Completion::Normal(cap.promise),
                Err(error) => interp.if_abrupt_reject_promise(error, &cap),
            }
        })
    }

    pub(crate) fn intrinsic_promise_constructor(&self) -> JsValue {
        self.realm()
            .promise_constructor
            .clone()
            .unwrap_or(JsValue::UNDEFINED)
    }

    pub(crate) fn is_intrinsic_promise_constructor(&self, value: &JsValue) -> bool {
        is_same_object(self.realm().promise_constructor.as_ref(), value)
    }

    fn is_intrinsic_promise_then(&self, value: &JsValue) -> bool {
        is_same_object(self.realm().promise_then.as_ref(), value)
    }

    fn callable_or_none(&self, value: &JsValue) -> Option<JsValue> {
        self.is_callable(value).then(|| value.clone())
    }

    /// True while `Promise[@@species]` is still the original getter, which
    /// returns its `this` value and has no other effect.
    fn promise_species_getter_is_original(&self, ctor_id: u64) -> bool {
        let Some(ctor) = self.get_object_cell(ctor_id) else {
            return false;
        };
        let key = JsPropertyKey::well_known_symbol("species");
        let ctor = ctor.borrow();
        let getter = ctor.properties.get(&key).and_then(|desc| desc.getter());
        match getter {
            Some(getter) => is_same_object(self.realm().promise_species_getter.as_ref(), getter),
            None => false,
        }
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

        // C is %Promise% itself: no user code can observe the construction.
        if self.is_intrinsic_promise_constructor(constructor) {
            let promise = self.create_promise_object();
            let promise_id = promise.as_object_id().unwrap_or_default();
            let (resolve, reject) = self.create_resolving_functions(promise_id);
            return Ok(PromiseCapability {
                promise,
                resolve,
                reject,
            });
        }

        // General case: call new C(executor) where executor captures resolve/reject.
        // The captured functions live in Rooted Slots pinned on the executor: a
        // user constructor can collect (every statement boundary) after calling
        // the executor and before returning.
        let slots = RootedSlots::new(self, 2);
        let capture = slots.clone();

        let executor = self.create_function(JsFunction::native(
            "".to_string(),
            2,
            move |interp, _this, args| {
                let resolve_arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let reject_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);

                // Spec: If promiseCapability.[[Resolve]] is not undefined, throw TypeError
                // Spec: If promiseCapability.[[Reject]] is not undefined, throw TypeError
                if !capture.get(interp, 0).is_undefined() || !capture.get(interp, 1).is_undefined()
                {
                    return Completion::Throw(
                        interp.create_type_error("Promise executor has already been resolved"),
                    );
                }

                // Spec: Set promiseCapability.[[Resolve]] to resolve
                // Spec: Set promiseCapability.[[Reject]] to reject
                capture.set(interp, 0, resolve_arg);
                capture.set(interp, 1, reject_arg);

                Completion::Normal(JsValue::UNDEFINED)
            },
        ));
        slots.pin_on(self, &executor);

        let promise = match self.construct(constructor, &[executor]) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => return Err(self.create_type_error("Promise constructor returned abnormally")),
        };

        let resolve = slots.get(self, 0);
        let reject = slots.get(self, 1);

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
        // `Promise[@@species]` is `get [Symbol.species]() { return this; }`;
        // skip the call while it is still the original accessor.
        if self.is_intrinsic_promise_constructor(&ctor)
            && self.promise_species_getter_is_original(ctor_id)
        {
            return Ok(ctor);
        }
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

        if self.is_intrinsic_promise_constructor(constructor) {
            let promise = self.create_promise_object();
            let promise_id = promise.as_object_id().unwrap_or_default();
            self.resolve_promise_internal(promise_id, value.clone());
            return Ok(promise);
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
        let then_fn = self.define_method(proto_id, "then", 2, |interp, this, args| {
            let on_fulfilled = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
            let on_rejected = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
            interp.promise_then(this, &on_fulfilled, &on_rejected)
        });
        self.realm_mut().promise_then = Some(then_fn);

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
            let promise_ctor = interp.intrinsic_promise_constructor();
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
            move |interp, _this, args| promise_construct(interp, args),
        ));

        // Mark Promise constructor as deferred_construct so construct_with_new_target
        // skips early prototype access (Promise checks callable before OrdinaryCreateFromConstructor).
        if let Some(o) = ctor.as_object_id()
            && let Some(func_obj) = self.get_object_cell(o)
        {
            func_obj.borrow_mut().deferred_construct = true;
        }

        self.realm_mut().promise_constructor = Some(ctor.clone());

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
                        get: Some(species_getter.clone()),
                        set: None,
                        enumerable: Some(false),
                        configurable: Some(true),
                    },
                );
            self.realm_mut().promise_species_getter = Some(species_getter);
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
                if interp.is_intrinsic_promise_constructor(this) {
                    return interp.create_rejected_promise(reason);
                }
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
                if !this.is_object() {
                    return Completion::Throw(
                        interp.create_type_error("Promise.try requires an object"),
                    );
                }
                // Validate the receiver up front, matching NewPromiseCapability's own
                // check, so a non-constructor `this` throws before the callback runs
                // (and before promise_resolve_with_constructor's same-constructor fast
                // path could otherwise skip validation entirely).
                if !interp.is_constructor(this) {
                    return Completion::Throw(
                        interp.create_type_error("Promise.try requires a constructor"),
                    );
                }
                let callback = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let call_args: Vec<JsValue> = if args.len() > 1 {
                    args[1..].to_vec()
                } else {
                    vec![]
                };
                let result = interp.call_function(&callback, &JsValue::UNDEFINED, &call_args);
                match result {
                    Completion::Normal(value) => {
                        // promise_resolve_with_constructor may run a custom
                        // constructor via NewPromiseCapability; root `value` across
                        // it since it is otherwise only a Rust local.
                        let frame = interp.gc_root_frame();
                        interp.gc_root_value(&value);
                        let outcome = interp.promise_resolve_with_constructor(this, &value);
                        interp.gc_unroot_frame(frame);
                        match outcome {
                            Ok(promise) => Completion::Normal(promise),
                            Err(e) => Completion::Throw(e),
                        }
                    }
                    Completion::Throw(e) => {
                        // Same reasoning: `e` must stay rooted while
                        // new_promise_capability runs a custom constructor, and
                        // while it is passed to reject.
                        let frame = interp.gc_root_frame();
                        interp.gc_root_value(&e);
                        let cap = match interp.new_promise_capability(this) {
                            Ok(cap) => cap,
                            Err(err) => {
                                interp.gc_unroot_frame(frame);
                                return Completion::Throw(err);
                            }
                        };
                        let reject_result =
                            interp.call_function(&cap.reject, &JsValue::UNDEFINED, &[e]);
                        interp.gc_unroot_frame(frame);
                        // `cap.reject` can be a custom constructor's own
                        // executor-provided function; a `__host_exit` inside it
                        // (issue #242) must propagate rather than be discarded.
                        if let Completion::Exit(code) = reject_result {
                            return Completion::Exit(code);
                        }
                        if let Completion::Throw(e2) = reject_result {
                            return Completion::Throw(e2);
                        }
                        Completion::Normal(cap.promise)
                    }
                    // A `__host_exit` inside the callback (issue #242) must
                    // propagate before `new_promise_capability` gets a chance to
                    // run arbitrary constructor code.
                    Completion::Exit(code) => Completion::Exit(code),
                    _ => {
                        let cap = match interp.new_promise_capability(this) {
                            Ok(cap) => cap,
                            Err(e) => return Completion::Throw(e),
                        };
                        Completion::Normal(cap.promise)
                    }
                }
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
        data.class_name = "Promise";
        data.kind = crate::interpreter::types::ObjectKind::Promise(Box::default());
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
                interp.resolve_promise_internal(promise_id, value);
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
            if let Some(promise) = self.get_object_cell(promise_id) {
                let mut promise = promise.borrow_mut();
                if let Some(pd) = promise.promise_data_mut()
                    && matches!(pd.state, PromiseState::Pending)
                {
                    pd.resolving_functions
                        .push((resolve_fn.clone(), reject_fn.clone()));
                }
            }
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
                pd.resolving_functions.clear();
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
                pd.resolving_functions.clear();
                let reactions = std::mem::take(&mut pd.reject_reactions);
                pd.fulfill_reactions.clear();
                reactions
            } else {
                return;
            }
        } else {
            return;
        };
        self.on_reject(promise_id, reactions, reason);
    }

    /// The Promise Resolve Functions steps (ECMA-262 §27.2.1.3.2) for a promise
    /// whose resolving functions are not exposed. The caller supplies the
    /// `[[AlreadyResolved]]` discipline; this never throws.
    pub(crate) fn resolve_promise_internal(&mut self, promise_id: u64, value: JsValue) {
        let Some(value_id) = value.as_object_id() else {
            self.fulfill_promise(promise_id, value);
            return;
        };
        if value_id == promise_id {
            let err = self.create_type_error("A promise cannot be resolved with itself.");
            self.reject_promise(promise_id, err);
            return;
        }
        // `Get(resolution, "then")` runs user code, so both the promise and
        // the resolution must be reachable meanwhile.
        let frame = self.gc_root_frame();
        self.gc_root_value(&JsValue::object(promise_id));
        self.gc_root_value(&value);
        let then_val = self.get_object_property(value_id, "then", &value);
        self.gc_unroot_frame(frame);
        match then_val {
            Completion::Normal(then_fn) if self.is_callable(&then_fn) => {
                self.scheduler
                    .enqueue_resolve_thenable_job(promise_id, value, then_fn);
            }
            Completion::Throw(e) => self.reject_promise(promise_id, e),
            _ => self.fulfill_promise(promise_id, value),
        }
    }

    fn claim_internal_resolution(&mut self, promise_id: u64) -> bool {
        let Some(cell) = self.get_object_cell(promise_id) else {
            return false;
        };
        let mut obj = cell.borrow_mut();
        match obj.promise_data_mut() {
            Some(pd) if !pd.already_resolved => {
                pd.already_resolved = true;
                true
            }
            _ => false,
        }
    }

    pub(crate) fn promise_resolvers_resolve(
        &mut self,
        resolvers: &PromiseResolvers,
        value: JsValue,
    ) {
        match resolvers {
            PromiseResolvers::Internal(promise_id) => {
                if self.claim_internal_resolution(*promise_id) {
                    self.resolve_promise_internal(*promise_id, value);
                }
            }
            PromiseResolvers::Functions { resolve, .. } => {
                let _ = self.call_function(resolve, &JsValue::UNDEFINED, &[value]);
            }
        }
    }

    pub(crate) fn promise_resolvers_reject(
        &mut self,
        resolvers: &PromiseResolvers,
        reason: JsValue,
    ) {
        match resolvers {
            PromiseResolvers::Internal(promise_id) => {
                if self.claim_internal_resolution(*promise_id) {
                    self.reject_promise(*promise_id, reason);
                }
            }
            PromiseResolvers::Functions { reject, .. } => {
                let _ = self.call_function(reject, &JsValue::UNDEFINED, &[reason]);
            }
        }
    }

    pub(crate) fn trigger_promise_reactions(
        &mut self,
        reactions: Vec<PromiseReaction>,
        argument: JsValue,
    ) {
        for reaction in reactions {
            self.scheduler
                .enqueue_reaction_job(reaction, argument.clone());
        }
    }

    pub(crate) fn run_promise_reaction_job(
        &mut self,
        reaction: PromiseReaction,
        argument: JsValue,
    ) -> Completion {
        let PromiseReaction {
            handler,
            target,
            reaction_type,
        } = reaction;
        if let ReactionTarget::AsyncResume(async_id) = target {
            // Return the resume completion so a `__host_exit` in the resumed
            // body (issue #242) reaches the drain loop as `Completion::Exit`.
            return self.async_function_resume(
                async_id,
                argument,
                reaction_type == PromiseReactionType::Reject,
            );
        }
        if let ReactionTarget::AsyncGen(gen_id, step) = target {
            self.async_gen_reaction(
                gen_id,
                step,
                argument,
                reaction_type == PromiseReactionType::Reject,
            );
            return Completion::Normal(JsValue::UNDEFINED);
        }
        let handler_result = match handler {
            Some(ref handler) if self.is_callable(handler) => {
                match self.call_function(
                    handler,
                    &JsValue::UNDEFINED,
                    std::slice::from_ref(&argument),
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
            }
            _ => match reaction_type {
                PromiseReactionType::Fulfill => Ok(argument),
                PromiseReactionType::Reject => Err(argument),
            },
        };

        match target {
            ReactionTarget::None
            | ReactionTarget::AsyncResume(_)
            | ReactionTarget::AsyncGen(..) => {}
            ReactionTarget::Internal(promise_id) => match handler_result {
                Ok(value) => self.resolve_promise_internal(promise_id, value),
                Err(reason) => self.reject_promise(promise_id, reason),
            },
            ReactionTarget::Capability {
                resolve, reject, ..
            } => {
                let (function, argument) = match handler_result {
                    Ok(value) => (resolve, value),
                    Err(reason) => (reject, reason),
                };
                if let Completion::Throw(e) =
                    self.call_function(&function, &JsValue::UNDEFINED, &[argument])
                {
                    return Completion::Throw(e);
                }
            }
        }
        Completion::Normal(JsValue::UNDEFINED)
    }

    pub(crate) fn run_promise_resolve_thenable_job(
        &mut self,
        promise_id: u64,
        thenable: JsValue,
        then_fn: JsValue,
    ) -> Completion {
        // A native promise whose `then` is the original: perform the steps of
        // `thenable.then(resolve, reject)` directly. `SpeciesConstructor` stays
        // observable; only the unobservable resolving-function objects and the
        // dropped derived promise are skipped.
        let native_thenable_id = thenable.as_object_id().filter(|&id| {
            self.is_intrinsic_promise_then(&then_fn)
                && self
                    .get_object_cell(id)
                    .is_some_and(|obj| obj.borrow().promise_data().is_some())
        });
        let frame = self.gc_root_frame();
        let outcome = if let Some(thenable_id) = native_thenable_id {
            let default_ctor = self.intrinsic_promise_constructor();
            match self.species_constructor(&thenable, &default_ctor) {
                Err(e) => self.reject_promise(promise_id, e),
                Ok(c) if self.is_intrinsic_promise_constructor(&c) => {
                    self.perform_promise_then_internal(
                        thenable_id,
                        None,
                        None,
                        ReactionTarget::Internal(promise_id),
                    );
                }
                Ok(c) => {
                    let (resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);
                    self.gc_root_value(&resolve_fn);
                    self.gc_root_value(&reject_fn);
                    if let Err(e) =
                        self.promise_then_with_constructor(thenable_id, &c, &resolve_fn, &reject_fn)
                    {
                        let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
                    }
                }
            }
            Completion::Normal(JsValue::UNDEFINED)
        } else {
            let (resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);
            self.gc_root_value(&resolve_fn);
            self.gc_root_value(&reject_fn);
            match self.call_function(&then_fn, &thenable, &[resolve_fn, reject_fn.clone()]) {
                // A `__host_exit` inside the user `then` (issue #242)
                // propagates out of the job as `Completion::Exit` rather than
                // being turned into a rejection.
                Completion::Exit(code) => Completion::Exit(code),
                Completion::Throw(e) => {
                    match self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]) {
                        Completion::Throw(e2) => Completion::Throw(e2),
                        _ => Completion::Normal(JsValue::UNDEFINED),
                    }
                }
                _ => Completion::Normal(JsValue::UNDEFINED),
            }
        };
        self.gc_unroot_frame(frame);
        outcome
    }

    /// Promise.prototype.then steps 4-5 and 7: NewPromiseCapability(`ctor`),
    /// then PerformPromiseThen on `promise_id`. For `%Promise%` the derived
    /// promise is settled directly and no resolving functions exist.
    fn promise_then_with_constructor(
        &mut self,
        promise_id: u64,
        ctor: &JsValue,
        on_fulfilled: &JsValue,
        on_rejected: &JsValue,
    ) -> Result<JsValue, JsValue> {
        let (derived, target) = if self.is_intrinsic_promise_constructor(ctor) {
            let derived = self.create_promise_object();
            let derived_id = derived.as_object_id().unwrap_or_default();
            (derived, ReactionTarget::Internal(derived_id))
        } else {
            let cap = self.new_promise_capability(ctor)?;
            let target = ReactionTarget::Capability {
                promise_id: cap.promise.as_object_id().unwrap_or_default(),
                resolve: cap.resolve,
                reject: cap.reject,
            };
            (cap.promise, target)
        };
        let fulfill_handler = self.callable_or_none(on_fulfilled);
        let reject_handler = self.callable_or_none(on_rejected);
        self.perform_promise_then_internal(promise_id, fulfill_handler, reject_handler, target);
        Ok(derived)
    }

    /// §27.2.5.4.1 PerformPromiseThen with a pre-created result capability.
    /// Skips SpeciesConstructor — uses the provided promise/resolve/reject directly.
    /// A non-object `result_promise` means there is no result capability.
    pub(crate) fn perform_promise_then(
        &mut self,
        promise_val: &JsValue,
        on_fulfilled: &JsValue,
        on_rejected: &JsValue,
        result_promise: JsValue,
        result_resolve: JsValue,
        result_reject: JsValue,
    ) -> Completion {
        let Some(promise_id) = promise_val.as_object_id() else {
            return Completion::Throw(
                self.create_type_error("PerformPromiseThen called on non-promise"),
            );
        };
        let target = match result_promise.as_object_id() {
            Some(derived_id) => ReactionTarget::Capability {
                promise_id: derived_id,
                resolve: result_resolve,
                reject: result_reject,
            },
            None => ReactionTarget::None,
        };
        let fulfill_handler = self.callable_or_none(on_fulfilled);
        let reject_handler = self.callable_or_none(on_rejected);
        self.perform_promise_then_internal(promise_id, fulfill_handler, reject_handler, target);
        Completion::Normal(result_promise)
    }

    /// PerformPromiseThen steps 9-13 for pre-validated handlers: register the
    /// reactions on a pending promise or queue the matching job at once.
    pub(crate) fn perform_promise_then_internal(
        &mut self,
        promise_id: u64,
        fulfill_handler: Option<JsValue>,
        reject_handler: Option<JsValue>,
        target: ReactionTarget,
    ) {
        let (reaction_type, argument) = {
            let Some(cell) = self.get_object_cell(promise_id) else {
                return;
            };
            let mut obj = cell.borrow_mut();
            let Some(pd) = obj.promise_data_mut() else {
                return;
            };
            pd.is_handled = true;
            match &pd.state {
                PromiseState::Pending => {
                    pd.fulfill_reactions.push(PromiseReaction {
                        handler: fulfill_handler,
                        target: target.clone(),
                        reaction_type: PromiseReactionType::Fulfill,
                    });
                    pd.reject_reactions.push(PromiseReaction {
                        handler: reject_handler,
                        target,
                        reaction_type: PromiseReactionType::Reject,
                    });
                    return;
                }
                PromiseState::Fulfilled(v) => (PromiseReactionType::Fulfill, v.clone()),
                PromiseState::Rejected(r) => (PromiseReactionType::Reject, r.clone()),
            }
        };
        let handler = match reaction_type {
            PromiseReactionType::Fulfill => fulfill_handler,
            PromiseReactionType::Reject => reject_handler,
        };
        self.scheduler.enqueue_reaction_job(
            PromiseReaction {
                handler,
                target,
                reaction_type,
            },
            argument,
        );
    }

    /// §6.2.3.1 Await step 2. A non-object can only produce a promise that is
    /// already fulfilled with it, so no promise is built for it (`None`).
    pub(crate) fn await_promise_resolve(&mut self, await_val: &JsValue) -> Option<u64> {
        if await_val.is_object() {
            self.promise_resolve_value(await_val).as_object_id()
        } else {
            None
        }
    }

    /// §6.2.3.1 Await steps 5-9 for a continuation with a typed `target` and no
    /// handler: PerformPromiseThen on `awaited_promise`, or, for a non-object
    /// (`None`), the one reaction job it would have queued, carrying the value.
    pub(crate) fn await_then_internal(
        &mut self,
        awaited_promise: Option<u64>,
        await_val: &JsValue,
        target: ReactionTarget,
    ) {
        match awaited_promise {
            Some(promise_id) => {
                self.perform_promise_then_internal(promise_id, None, None, target);
            }
            None => self.scheduler.enqueue_reaction_job(
                PromiseReaction {
                    handler: None,
                    target,
                    reaction_type: PromiseReactionType::Fulfill,
                },
                await_val.clone(),
            ),
        }
    }

    pub(crate) fn promise_then(
        &mut self,
        promise_val: &JsValue,
        on_fulfilled: &JsValue,
        on_rejected: &JsValue,
    ) -> Completion {
        let promise_id = match promise_val.as_object_id() {
            Some(o)
                if self
                    .get_object_cell(o)
                    .is_some_and(|obj| obj.borrow().promise_data().is_some()) =>
            {
                o
            }
            _ => {
                let err = self.create_type_error("Promise.prototype.then called on non-promise");
                return Completion::Throw(err);
            }
        };

        // SpeciesConstructor(promise, %Promise%)
        let default_ctor = self.intrinsic_promise_constructor();
        let c = match self.species_constructor(promise_val, &default_ctor) {
            Ok(c) => c,
            Err(e) => return Completion::Throw(e),
        };
        match self.promise_then_with_constructor(promise_id, &c, on_fulfilled, on_rejected) {
            Ok(derived) => Completion::Normal(derived),
            Err(e) => Completion::Throw(e),
        }
    }

    pub(crate) fn promise_resolve_value(&mut self, value: &JsValue) -> JsValue {
        // §27.2.4.7.1 PromiseResolve(C, x): if IsPromise(x), check x.constructor === C
        if let Some(o) = value.as_object_id()
            && let Some(obj) = self.get_object_cell(o)
            && obj.borrow().promise_data().is_some()
        {
            match self.get_object_property(o, "constructor", value) {
                Completion::Normal(ctor) => {
                    if self.is_intrinsic_promise_constructor(&ctor) {
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
        self.resolve_promise_internal(promise_id, value.clone());
        promise
    }

    fn promise_all(&mut self, constructor: &JsValue, iterable: &JsValue) -> Completion {
        self.run_promise_combinator(constructor, |interp, cap, promise_resolve| {
            // GetIterator(iterable)
            let iterator = interp.get_iterator(iterable)?;
            interp.gc_root_value(&iterator);

            // Accumulated element values live in these slots rather than being
            // pinned on the capability function. A custom constructor may hand the
            // same resolving function to every capability it builds, and pins are
            // never removed, so anchoring there would retain one settled value per
            // call on a long-lived object — unbounded growth. Every element function
            // pins the slots, so they outlive them all and die with them.
            let slots = RootedSlots::new(interp, 0);
            interp.gc_root_value(&slots.root());
            let remaining = Rc::new(Cell::new(1)); // starts at 1 per spec (decremented at end)
            let mut index = 0usize;

            loop {
                // IteratorStep — per spec, if this throws, iteratorRecord.[[done]] = true,
                // so we do NOT call IteratorClose.
                let next = match interp.iterator_step(&iterator) {
                    Ok(Some(result)) => result,
                    Ok(None) => {
                        let r = remaining.get() - 1;
                        remaining.set(r);
                        if r == 0 {
                            let values = slots.snapshot(interp);
                            let arr = interp.create_array(values);
                            if let Completion::Throw(e) =
                                interp.call_function(&cap.resolve, &JsValue::UNDEFINED, &[arr])
                            {
                                return Err(e);
                            }
                        }
                        return Ok(());
                    }
                    Err(e) => {
                        return Err(e);
                    }
                };

                // IteratorValue — per spec, if this throws, iteratorRecord.[[done]] = true,
                // so we do NOT call IteratorClose.
                let next_value = interp.iterator_value(&next)?;

                slots.push(interp, JsValue::UNDEFINED);
                remaining.set(remaining.get() + 1);

                let p = match interp.call_function(promise_resolve, constructor, &[next_value]) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => {
                        interp.iterator_close(&iterator, e.clone());
                        return Err(e);
                    }
                    _ => JsValue::UNDEFINED,
                };

                let i = index;
                let remaining = remaining.clone();
                let resolve_fn = cap.resolve.clone();
                let element_slots = slots.clone();
                let already_called = Rc::new(Cell::new(false));

                let ac = already_called.clone();
                let on_fulfilled = interp.create_function(JsFunction::native(
                    "".to_string(),
                    1,
                    move |interp, _this, args| {
                        if ac.get() {
                            return Completion::Normal(JsValue::UNDEFINED);
                        }
                        ac.set(true);
                        let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        element_slots.set(interp, i, val);
                        let r = remaining.get() - 1;
                        remaining.set(r);
                        if r == 0 {
                            let values = element_slots.snapshot(interp);
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
                interp.pin_native_root(&on_fulfilled, &cap.resolve);
                slots.pin_on(interp, &on_fulfilled);

                let reject_fn_clone = cap.reject.clone();
                let p_id = p.as_object_id().unwrap_or_default();
                let then_fn = match interp.get_object_property(p_id, "then", &p) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => {
                        interp.iterator_close(&iterator, e.clone());
                        return Err(e);
                    }
                    _ => JsValue::UNDEFINED,
                };
                if let Completion::Throw(e) =
                    interp.call_function(&then_fn, &p, &[on_fulfilled, reject_fn_clone])
                {
                    interp.iterator_close(&iterator, e.clone());
                    return Err(e);
                }

                index += 1;
            }
        })
    }

    fn promise_all_settled(&mut self, constructor: &JsValue, iterable: &JsValue) -> Completion {
        self.run_promise_combinator(constructor, |interp, cap, promise_resolve| {
            let iterator = interp.get_iterator(iterable)?;
            interp.gc_root_value(&iterator);

            // Slots for the accumulated records; see promise_all for why they must
            // not hang off the capability function.
            let slots = RootedSlots::new(interp, 0);
            interp.gc_root_value(&slots.root());
            let remaining = Rc::new(Cell::new(1));
            let mut index = 0usize;

            loop {
                // IteratorStep — no IteratorClose on error (done = true)
                let next = match interp.iterator_step(&iterator) {
                    Ok(Some(result)) => result,
                    Ok(None) => {
                        let r = remaining.get() - 1;
                        remaining.set(r);
                        if r == 0 {
                            let values = slots.snapshot(interp);
                            let arr = interp.create_array(values);
                            if let Completion::Throw(e) =
                                interp.call_function(&cap.resolve, &JsValue::UNDEFINED, &[arr])
                            {
                                return Err(e);
                            }
                        }
                        return Ok(());
                    }
                    Err(e) => {
                        return Err(e);
                    }
                };

                // IteratorValue — no IteratorClose on error (done = true)
                let next_value = interp.iterator_value(&next)?;

                slots.push(interp, JsValue::UNDEFINED);
                remaining.set(remaining.get() + 1);

                let p = match interp.call_function(promise_resolve, constructor, &[next_value]) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => {
                        interp.iterator_close(&iterator, e.clone());
                        return Err(e);
                    }
                    _ => JsValue::UNDEFINED,
                };

                let i = index;
                let remaining_f = remaining.clone();
                let resolve_fn_f = cap.resolve.clone();
                let remaining_r = remaining.clone();
                let resolve_fn_r = cap.resolve.clone();
                let slots_f = slots.clone();
                let slots_r = slots.clone();
                let already_called = Rc::new(Cell::new(false));

                let ac_f = already_called.clone();
                let on_fulfilled = interp.create_function(JsFunction::native(
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
                        slots_f.set(interp, i, record);
                        let r = remaining_f.get() - 1;
                        remaining_f.set(r);
                        if r == 0 {
                            let values = slots_f.snapshot(interp);
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
                let on_rejected = interp.create_function(JsFunction::native(
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
                        slots_r.set(interp, i, record);
                        let r = remaining_r.get() - 1;
                        remaining_r.set(r);
                        if r == 0 {
                            let values = slots_r.snapshot(interp);
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

                interp.pin_native_root(&on_fulfilled, &cap.resolve);
                interp.pin_native_root(&on_rejected, &cap.resolve);
                slots.pin_on(interp, &on_fulfilled);
                slots.pin_on(interp, &on_rejected);

                let p_id = p.as_object_id().unwrap_or_default();
                let then_fn = match interp.get_object_property(p_id, "then", &p) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => {
                        interp.iterator_close(&iterator, e.clone());
                        return Err(e);
                    }
                    _ => JsValue::UNDEFINED,
                };
                if let Completion::Throw(e) =
                    interp.call_function(&then_fn, &p, &[on_fulfilled, on_rejected])
                {
                    interp.iterator_close(&iterator, e.clone());
                    return Err(e);
                }

                index += 1;
            }
        })
    }

    /// Promise.allKeyed (await-dictionary Stage-3 proposal).
    fn promise_all_keyed(&mut self, constructor: &JsValue, promises: &JsValue) -> Completion {
        self.run_promise_combinator(constructor, |interp, cap, promise_resolve| {
            // If `promises` is not an Object, reject the returned promise.
            let Some(promises_obj_id) = promises.as_object_id() else {
                let err = interp.create_type_error("Promise.allKeyed argument is not an object");
                return Err(err);
            };

            // allKeys = ? promises.[[OwnPropertyKeys]]()
            let all_keys = interp.proxy_own_keys(promises_obj_id)?;

            // Slots for the accumulated values; see promise_all for why they must
            // not hang off the capability function. The keys are not GC values, so
            // they stay a plain Rust list.
            let slots = RootedSlots::new(interp, 0);
            interp.gc_root_value(&slots.root());
            let remaining = Rc::new(Cell::new(1u64));
            let keys: Rc<RefCell<Vec<JsPropertyKey>>> = Rc::new(RefCell::new(Vec::new()));

            for key_val in all_keys {
                let key_str = to_property_key_string(&key_val);

                // desc = ? promises.[[GetOwnProperty]](key)
                let desc_val =
                    match interp.proxy_get_own_property_descriptor(promises_obj_id, &key_str) {
                        Ok(v) => v,
                        Err(e) => return Err(e),
                    };
                if (desc_val).is_undefined() {
                    continue;
                }
                // Spec: process only when desc.[[Enumerable]] is true. After
                // ToPropertyDescriptor an absent enumerable defaults to false
                // (via CompletePropertyDescriptor), so a missing field must skip.
                let is_enumerable = match interp.to_property_descriptor(&desc_val) {
                    Ok(d) => d.enumerable == Some(true),
                    Err(Some(e)) => return Err(e),
                    Err(None) => false,
                };
                if !is_enumerable {
                    continue;
                }

                // nextValue = ? Get(promises, key)
                let next_value =
                    match interp.get_object_property(promises_obj_id, &key_str, promises) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Err(e),
                        _ => JsValue::UNDEFINED,
                    };

                let i = slots.len(interp);
                keys.borrow_mut().push(key_str.clone());
                slots.push(interp, JsValue::UNDEFINED);
                remaining.set(remaining.get() + 1);

                let p = match interp.call_function(promise_resolve, constructor, &[next_value]) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::UNDEFINED,
                };

                let remaining_c = remaining.clone();
                let keys_c = keys.clone();
                let resolve_fn = cap.resolve.clone();
                let slots_c = slots.clone();
                let already_called = Rc::new(Cell::new(false));
                let ac = already_called.clone();

                let on_fulfilled = interp.create_function(JsFunction::native(
                    "".to_string(),
                    1,
                    move |interp, _this, args| {
                        if ac.get() {
                            return Completion::Normal(JsValue::UNDEFINED);
                        }
                        ac.set(true);
                        let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        slots_c.set(interp, i, val);
                        let r = remaining_c.get() - 1;
                        remaining_c.set(r);
                        if r == 0 {
                            let keys_snapshot = keys_c.borrow().clone();
                            let values_snapshot = slots_c.snapshot(interp);
                            let result =
                                interp.build_keyed_result(&keys_snapshot, &values_snapshot);
                            if let Completion::Throw(e) =
                                interp.call_function(&resolve_fn, &JsValue::UNDEFINED, &[result])
                            {
                                return Completion::Throw(e);
                            }
                        }
                        Completion::Normal(JsValue::UNDEFINED)
                    },
                ));

                interp.pin_native_root(&on_fulfilled, &cap.resolve);
                slots.pin_on(interp, &on_fulfilled);

                let p_id = p.as_object_id().unwrap_or_default();
                let then_fn = match interp.get_object_property(p_id, "then", &p) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::UNDEFINED,
                };
                if let Completion::Throw(e) =
                    interp.call_function(&then_fn, &p, &[on_fulfilled, cap.reject.clone()])
                {
                    return Err(e);
                }
            }

            // Final decrement: matches the spec's "starts at 1" pattern.
            let r = remaining.get() - 1;
            remaining.set(r);
            if r == 0 {
                let keys_snapshot = keys.borrow().clone();
                let values_snapshot = slots.snapshot(interp);
                let result = interp.build_keyed_result(&keys_snapshot, &values_snapshot);
                if let Completion::Throw(e) =
                    interp.call_function(&cap.resolve, &JsValue::UNDEFINED, &[result])
                {
                    return Err(e);
                }
            }

            Ok(())
        })
    }

    /// Promise.allSettledKeyed (await-dictionary Stage-3 proposal).
    fn promise_all_settled_keyed(
        &mut self,
        constructor: &JsValue,
        promises: &JsValue,
    ) -> Completion {
        self.run_promise_combinator(constructor, |interp, cap, promise_resolve| {
            let Some(promises_obj_id) = promises.as_object_id() else {
                let err =
                    interp.create_type_error("Promise.allSettledKeyed argument is not an object");
                return Err(err);
            };

            let all_keys = interp.proxy_own_keys(promises_obj_id)?;

            // Slots for the accumulated values; see promise_all for why they must
            // not hang off the capability function. The keys are not GC values, so
            // they stay a plain Rust list.
            let slots = RootedSlots::new(interp, 0);
            interp.gc_root_value(&slots.root());
            let remaining = Rc::new(Cell::new(1u64));
            let keys: Rc<RefCell<Vec<JsPropertyKey>>> = Rc::new(RefCell::new(Vec::new()));

            for key_val in all_keys {
                let key_str = to_property_key_string(&key_val);

                let desc_val =
                    match interp.proxy_get_own_property_descriptor(promises_obj_id, &key_str) {
                        Ok(v) => v,
                        Err(e) => return Err(e),
                    };
                if (desc_val).is_undefined() {
                    continue;
                }
                // Spec: process only when desc.[[Enumerable]] is true (absent
                // defaults to false via CompletePropertyDescriptor).
                let is_enumerable = match interp.to_property_descriptor(&desc_val) {
                    Ok(d) => d.enumerable == Some(true),
                    Err(Some(e)) => return Err(e),
                    Err(None) => false,
                };
                if !is_enumerable {
                    continue;
                }

                let next_value =
                    match interp.get_object_property(promises_obj_id, &key_str, promises) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Err(e),
                        _ => JsValue::UNDEFINED,
                    };

                let i = slots.len(interp);
                keys.borrow_mut().push(key_str.clone());
                slots.push(interp, JsValue::UNDEFINED);
                remaining.set(remaining.get() + 1);

                let p = match interp.call_function(promise_resolve, constructor, &[next_value]) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::UNDEFINED,
                };

                let already_called = Rc::new(Cell::new(false));

                let remaining_f = remaining.clone();
                let keys_f = keys.clone();
                let resolve_fn_f = cap.resolve.clone();
                let slots_f = slots.clone();
                let ac_f = already_called.clone();
                let on_fulfilled = interp.create_function(JsFunction::native(
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
                        slots_f.set(interp, i, record);
                        let r = remaining_f.get() - 1;
                        remaining_f.set(r);
                        if r == 0 {
                            let keys_snapshot = keys_f.borrow().clone();
                            let values_snapshot = slots_f.snapshot(interp);
                            let result =
                                interp.build_keyed_result(&keys_snapshot, &values_snapshot);
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
                let resolve_fn_r = cap.resolve.clone();
                let slots_r = slots.clone();
                let ac_r = already_called.clone();
                let on_rejected = interp.create_function(JsFunction::native(
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
                        slots_r.set(interp, i, record);
                        let r = remaining_r.get() - 1;
                        remaining_r.set(r);
                        if r == 0 {
                            let keys_snapshot = keys_r.borrow().clone();
                            let values_snapshot = slots_r.snapshot(interp);
                            let result =
                                interp.build_keyed_result(&keys_snapshot, &values_snapshot);
                            if let Completion::Throw(e) =
                                interp.call_function(&resolve_fn_r, &JsValue::UNDEFINED, &[result])
                            {
                                return Completion::Throw(e);
                            }
                        }
                        Completion::Normal(JsValue::UNDEFINED)
                    },
                ));

                interp.pin_native_root(&on_fulfilled, &cap.resolve);
                interp.pin_native_root(&on_rejected, &cap.resolve);
                slots.pin_on(interp, &on_fulfilled);
                slots.pin_on(interp, &on_rejected);

                let p_id = p.as_object_id().unwrap_or_default();
                let then_fn = match interp.get_object_property(p_id, "then", &p) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::UNDEFINED,
                };
                if let Completion::Throw(e) =
                    interp.call_function(&then_fn, &p, &[on_fulfilled, on_rejected])
                {
                    return Err(e);
                }
            }

            let r = remaining.get() - 1;
            remaining.set(r);
            if r == 0 {
                let keys_snapshot = keys.borrow().clone();
                let values_snapshot = slots.snapshot(interp);
                let result = interp.build_keyed_result(&keys_snapshot, &values_snapshot);
                if let Completion::Throw(e) =
                    interp.call_function(&cap.resolve, &JsValue::UNDEFINED, &[result])
                {
                    return Err(e);
                }
            }

            Ok(())
        })
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
        self.run_promise_combinator(constructor, |interp, cap, promise_resolve| {
            let iterator = interp.get_iterator(iterable)?;
            interp.gc_root_value(&iterator);

            loop {
                // IteratorStep — no IteratorClose on error (done = true)
                let next = match interp.iterator_step(&iterator) {
                    Ok(Some(result)) => result,
                    Ok(None) => return Ok(()),
                    Err(e) => {
                        return Err(e);
                    }
                };

                // IteratorValue — no IteratorClose on error (done = true)
                let next_value = interp.iterator_value(&next)?;

                let p = match interp.call_function(promise_resolve, constructor, &[next_value]) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => {
                        interp.iterator_close(&iterator, e.clone());
                        return Err(e);
                    }
                    _ => JsValue::UNDEFINED,
                };

                let p_id = p.as_object_id().unwrap_or_default();
                let then_fn = match interp.get_object_property(p_id, "then", &p) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => {
                        interp.iterator_close(&iterator, e.clone());
                        return Err(e);
                    }
                    _ => JsValue::UNDEFINED,
                };
                if let Completion::Throw(e) =
                    interp.call_function(&then_fn, &p, &[cap.resolve.clone(), cap.reject.clone()])
                {
                    interp.iterator_close(&iterator, e.clone());
                    return Err(e);
                }
            }
        })
    }

    fn promise_any(&mut self, constructor: &JsValue, iterable: &JsValue) -> Completion {
        self.run_promise_combinator(constructor, |interp, cap, promise_resolve| {
            let iterator = interp.get_iterator(iterable)?;
            interp.gc_root_value(&iterator);

            // Slots for the accumulated errors; see promise_all for why they must
            // not hang off the capability function.
            let slots = RootedSlots::new(interp, 0);
            interp.gc_root_value(&slots.root());
            let remaining = Rc::new(Cell::new(1));
            let mut index = 0usize;

            loop {
                // IteratorStep — no IteratorClose on error (done = true)
                let next = match interp.iterator_step(&iterator) {
                    Ok(Some(result)) => result,
                    Ok(None) => {
                        let r = remaining.get() - 1;
                        remaining.set(r);
                        if r == 0 {
                            let errs = slots.snapshot(interp);
                            let err =
                                interp.create_aggregate_error(errs, "All promises were rejected");
                            return Err(err);
                        }
                        return Ok(());
                    }
                    Err(e) => {
                        return Err(e);
                    }
                };

                // IteratorValue — no IteratorClose on error (done = true)
                let next_value = interp.iterator_value(&next)?;

                slots.push(interp, JsValue::UNDEFINED);
                remaining.set(remaining.get() + 1);

                let p = match interp.call_function(promise_resolve, constructor, &[next_value]) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => {
                        interp.iterator_close(&iterator, e.clone());
                        return Err(e);
                    }
                    _ => JsValue::UNDEFINED,
                };

                let i = index;
                let remaining = remaining.clone();
                let reject_fn_clone = cap.reject.clone();
                let element_slots = slots.clone();
                let already_called = Rc::new(Cell::new(false));

                let ac = already_called.clone();
                let on_rejected = interp.create_function(JsFunction::native(
                    "".to_string(),
                    1,
                    move |interp, _this, args| {
                        if ac.get() {
                            return Completion::Normal(JsValue::UNDEFINED);
                        }
                        ac.set(true);
                        let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        element_slots.set(interp, i, val);
                        let r = remaining.get() - 1;
                        remaining.set(r);
                        if r == 0 {
                            let errs = element_slots.snapshot(interp);
                            let err =
                                interp.create_aggregate_error(errs, "All promises were rejected");
                            if let Completion::Throw(e) =
                                interp.call_function(&reject_fn_clone, &JsValue::UNDEFINED, &[err])
                            {
                                return Completion::Throw(e);
                            }
                        }
                        Completion::Normal(JsValue::UNDEFINED)
                    },
                ));

                interp.pin_native_root(&on_rejected, &cap.reject);
                slots.pin_on(interp, &on_rejected);

                let p_id = p.as_object_id().unwrap_or_default();
                let then_fn = match interp.get_object_property(p_id, "then", &p) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => {
                        interp.iterator_close(&iterator, e.clone());
                        return Err(e);
                    }
                    _ => JsValue::UNDEFINED,
                };
                if let Completion::Throw(e) =
                    interp.call_function(&then_fn, &p, &[cap.resolve.clone(), on_rejected])
                {
                    interp.iterator_close(&iterator, e.clone());
                    return Err(e);
                }

                index += 1;
            }
        })
    }

    fn create_aggregate_error(&mut self, errors: Vec<JsValue>, message: &str) -> JsValue {
        let obj_id = self.create_object_id();
        {
            let mut o = self.get_object_cell_expect(obj_id).borrow_mut();
            o.class_name = "AggregateError";
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
            if let Some(func) = obj.borrow().callable.as_deref() {
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

#[cfg(test)]
mod combinator_setup_tests {
    use super::*;
    use crate::parser::Parser;

    fn interpreter(source: &str) -> Interpreter {
        let program = Parser::new(source).unwrap().parse_program().unwrap();
        let mut interp = Interpreter::new();
        let result = interp.run(&program);
        assert!(matches!(result, Completion::Normal(_) | Completion::Empty));
        interp
    }

    fn global(interp: &Interpreter, name: &str) -> JsValue {
        interp.get_global_var_ref(name).unwrap()
    }

    const CAPABILITY: &str = r#"
        var events = '';
        var created, rejected;
        var error = {};
        function C(executor) {
            events += 'construct;';
            created = this;
            executor(function () {}, function (reason) { rejected = reason; });
        }
    "#;

    #[test]
    fn combinator_setup_runs_once_after_resolve_and_restores_roots() {
        let mut interp = interpreter(&format!(
            r#"{CAPABILITY}
            Object.defineProperty(C, 'resolve', {{
                get: function () {{ events += 'resolve;'; return function () {{}}; }}
            }});
            "#
        ));
        let constructor = global(&interp, "C");
        let depth = interp.gc_root_frame();
        let mut calls = 0;
        let result = interp.run_promise_combinator(&constructor, |interp, cap, resolve| {
            calls += 1;
            assert_eq!(
                global(interp, "events")
                    .as_string()
                    .unwrap()
                    .to_rust_string(),
                "construct;resolve;"
            );
            assert_eq!(
                cap.promise.as_object_id(),
                global(interp, "created").as_object_id()
            );
            assert!(interp.is_callable(resolve));
            // The scope also releases roots created by the operation itself.
            let temporary = JsValue::object(interp.create_object_id());
            interp.gc_root_value(&temporary);
            Ok(())
        });
        assert_eq!(calls, 1);
        assert!(
            matches!(result, Completion::Normal(ref value) if value.as_object_id() == global(&interp, "created").as_object_id())
        );
        assert_eq!(interp.gc_root_frame(), depth);
    }

    #[test]
    fn combinator_setup_rejects_getter_and_callability_errors_without_running_body() {
        for resolve in ["get: function () { throw error; }", "value: 0"] {
            let mut interp = interpreter(&format!(
                "{CAPABILITY} Object.defineProperty(C, 'resolve', {{ {resolve} }});"
            ));
            let constructor = global(&interp, "C");
            let depth = interp.gc_root_frame();
            let result = interp.run_promise_combinator(&constructor, |_, _, _| {
                panic!("setup failure must skip the operation")
            });
            assert!(
                matches!(result, Completion::Normal(ref value) if value.as_object_id() == global(&interp, "created").as_object_id())
            );
            let rejected = global(&interp, "rejected");
            if resolve.starts_with("get:") {
                assert_eq!(
                    rejected.as_object_id(),
                    global(&interp, "error").as_object_id()
                );
            } else {
                assert!(
                    interp
                        .format_value(&rejected)
                        .contains("Promise resolve is not a function")
                );
            }
            assert_eq!(interp.gc_root_frame(), depth);
        }
    }

    #[test]
    fn combinator_setup_constructor_error_throws_before_resolve_lookup() {
        let mut interp = interpreter(
            r#"
            var error = {};
            var lookedUp = false;
            function C() { throw error; }
            Object.defineProperty(C, 'resolve', {
                get: function () { lookedUp = true; return function () {}; }
            });
        "#,
        );
        let constructor = global(&interp, "C");
        let depth = interp.gc_root_frame();
        let result = interp.run_promise_combinator(&constructor, |_, _, _| {
            panic!("failed capability creation must skip the operation")
        });
        assert!(
            matches!(result, Completion::Throw(ref value) if value.as_object_id() == global(&interp, "error").as_object_id())
        );
        assert_eq!(global(&interp, "lookedUp").as_boolean(), Some(false));
        assert_eq!(interp.gc_root_frame(), depth);
    }

    #[test]
    fn combinator_setup_operation_error_rejects_and_ignores_reject_callback_throw() {
        let mut interp = interpreter(
            r#"
            var error = {}, other = {};
            var created, rejected;
            function C(executor) {
                created = this;
                executor(function () {}, function (reason) { rejected = reason; throw other; });
            }
            C.resolve = function () {};
        "#,
        );
        let constructor = global(&interp, "C");
        let error = global(&interp, "error");
        let depth = interp.gc_root_frame();
        let result = interp.run_promise_combinator(&constructor, |interp, _, _| {
            let temporary = JsValue::object(interp.create_object_id());
            interp.gc_root_value(&temporary);
            Err(error.clone())
        });
        assert!(
            matches!(result, Completion::Normal(ref value) if value.as_object_id() == global(&interp, "created").as_object_id())
        );
        assert_eq!(
            global(&interp, "rejected").as_object_id(),
            error.as_object_id()
        );
        assert_eq!(interp.gc_root_frame(), depth);
    }
}
