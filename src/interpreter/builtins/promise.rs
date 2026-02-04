use super::*;
use std::cell::Cell;

impl Interpreter {
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

        // Promise.prototype.catch
        let catch_fn = self.create_function(JsFunction::native(
            "catch".to_string(),
            1,
            |interp, this, args| {
                let on_rejected = args.first().cloned().unwrap_or(JsValue::Undefined);
                interp.promise_then(this, &JsValue::Undefined, &on_rejected)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("catch".to_string(), catch_fn);

        // Promise.prototype.finally
        let finally_fn = self.create_function(JsFunction::native(
            "finally".to_string(),
            1,
            |interp, this, args| {
                let on_finally = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !interp.is_callable(&on_finally) {
                    return interp.promise_then(this, &on_finally, &on_finally);
                }
                let on_finally_clone = on_finally.clone();
                let then_finally = interp.create_function(JsFunction::native(
                    "".to_string(),
                    1,
                    move |interp, _this, args| {
                        let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let result =
                            interp.call_function(&on_finally_clone, &JsValue::Undefined, &[]);
                        match result {
                            Completion::Throw(e) => Completion::Throw(e),
                            Completion::Normal(r) => {
                                let p = interp.promise_resolve_value(&r);
                                let value_clone = value.clone();
                                let return_fn = interp.create_function(JsFunction::native(
                                    "".to_string(),
                                    0,
                                    move |_interp, _this, _args| {
                                        Completion::Normal(value_clone.clone())
                                    },
                                ));
                                interp.promise_then(&p, &return_fn, &JsValue::Undefined)
                            }
                            _ => Completion::Normal(JsValue::Undefined),
                        }
                    },
                ));
                let on_finally_clone2 = on_finally.clone();
                let catch_finally = interp.create_function(JsFunction::native(
                    "".to_string(),
                    1,
                    move |interp, _this, args| {
                        let reason = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let result =
                            interp.call_function(&on_finally_clone2, &JsValue::Undefined, &[]);
                        match result {
                            Completion::Throw(e) => Completion::Throw(e),
                            Completion::Normal(r) => {
                                let p = interp.promise_resolve_value(&r);
                                let reason_clone = reason.clone();
                                let throw_fn = interp.create_function(JsFunction::native(
                                    "".to_string(),
                                    0,
                                    move |_interp, _this, _args| {
                                        Completion::Throw(reason_clone.clone())
                                    },
                                ));
                                interp.promise_then(&p, &throw_fn, &JsValue::Undefined)
                            }
                            _ => Completion::Normal(JsValue::Undefined),
                        }
                    },
                ));
                interp.promise_then(this, &then_finally, &catch_finally)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("finally".to_string(), finally_fn);

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
                if let Completion::Throw(e) = result {
                    let _ = interp.call_function(&reject_fn, &JsValue::Undefined, &[e]);
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
            |interp, _this, args| {
                let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                Completion::Normal(interp.promise_resolve_value(&value))
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
            |interp, _this, args| {
                let reason = args.first().cloned().unwrap_or(JsValue::Undefined);
                let promise = interp.create_promise_object();
                let promise_id = if let JsValue::Object(ref o) = promise {
                    o.id
                } else {
                    0
                };
                interp.reject_promise(promise_id, reason);
                Completion::Normal(promise)
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
            |interp, _this, args| {
                let iterable = args.first().cloned().unwrap_or(JsValue::Undefined);
                interp.promise_all(&iterable)
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
            |interp, _this, args| {
                let iterable = args.first().cloned().unwrap_or(JsValue::Undefined);
                interp.promise_all_settled(&iterable)
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
            |interp, _this, args| {
                let iterable = args.first().cloned().unwrap_or(JsValue::Undefined);
                interp.promise_race(&iterable)
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
            |interp, _this, args| {
                let iterable = args.first().cloned().unwrap_or(JsValue::Undefined);
                interp.promise_any(&iterable)
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
            |interp, _this, _args| {
                if !interp.is_constructor(_this) {
                    return Completion::Throw(
                        interp.create_type_error("Promise.withResolvers requires a constructor"),
                    );
                }
                let promise = interp.create_promise_object();
                let promise_id = if let JsValue::Object(ref o) = promise {
                    o.id
                } else {
                    0
                };
                let (resolve_fn, reject_fn) = interp.create_resolving_functions(promise_id);
                let result = interp.create_object();
                result
                    .borrow_mut()
                    .insert_builtin("promise".to_string(), promise);
                result
                    .borrow_mut()
                    .insert_builtin("resolve".to_string(), resolve_fn);
                result
                    .borrow_mut()
                    .insert_builtin("reject".to_string(), reject_fn);
                let id = result.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
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
            |interp, _this, args| {
                if !interp.is_constructor(_this) {
                    return Completion::Throw(
                        interp.create_type_error("Promise.try requires a constructor"),
                    );
                }
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                let call_args: Vec<JsValue> = if args.len() > 1 {
                    args[1..].to_vec()
                } else {
                    vec![]
                };
                let promise = interp.create_promise_object();
                let promise_id = if let JsValue::Object(ref o) = promise {
                    o.id
                } else {
                    0
                };
                let (resolve_fn, reject_fn) = interp.create_resolving_functions(promise_id);
                if !interp.is_callable(&callback) {
                    let err = interp.create_type_error("Promise.try requires a callable");
                    let _ = interp.call_function(&reject_fn, &JsValue::Undefined, &[err]);
                    return Completion::Normal(promise);
                }
                let result = interp.call_function(&callback, &JsValue::Undefined, &call_args);
                match result {
                    Completion::Normal(v) => {
                        let _ = interp.call_function(&resolve_fn, &JsValue::Undefined, &[v]);
                    }
                    Completion::Throw(e) => {
                        let _ = interp.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                    }
                    _ => {}
                }
                Completion::Normal(promise)
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
                if let JsValue::Object(ref o) = value
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let then_val = obj.borrow().get_property("then");
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
                        match interp.call_function(handler, &JsValue::Undefined, &[arg.clone()]) {
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
                            let _ = interp.call_function(
                                &reaction.resolve,
                                &JsValue::Undefined,
                                &[value],
                            );
                        }
                        Err(reason) => {
                            let _ = interp.call_function(
                                &reaction.reject,
                                &JsValue::Undefined,
                                &[reason],
                            );
                        }
                    }
                }
                Completion::Normal(JsValue::Undefined)
            }));
        }
    }

    fn promise_resolve_thenable(&mut self, promise_id: u64, thenable: JsValue, then_fn: JsValue) {
        let (resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);
        self.microtask_queue.push(Box::new(move |interp| {
            let result =
                interp.call_function(&then_fn, &thenable, &[resolve_fn, reject_fn.clone()]);
            if let Completion::Throw(e) = result {
                let _ = interp.call_function(&reject_fn, &JsValue::Undefined, &[e]);
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

        let derived = self.create_promise_object();
        let derived_id = if let JsValue::Object(ref o) = derived {
            o.id
        } else {
            0
        };
        let (resolve_fn, reject_fn) = self.create_resolving_functions(derived_id);

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
        // Check if value is a thenable
        if let JsValue::Object(o) = value
            && let Some(obj) = self.get_object(o.id)
        {
            let then_val = obj.borrow().get_property("then");
            if self.is_callable(&then_val) {
                self.promise_resolve_thenable(promise_id, value.clone(), then_val);
                return promise;
            }
        }
        self.fulfill_promise(promise_id, value.clone());
        promise
    }

    fn promise_all(&mut self, iterable: &JsValue) -> Completion {
        let items = match self.iterate_to_vec(iterable) {
            Ok(v) => v,
            Err(e) => {
                let promise = self.create_promise_object();
                let pid = if let JsValue::Object(ref o) = promise {
                    o.id
                } else {
                    0
                };
                self.reject_promise(pid, e);
                return Completion::Normal(promise);
            }
        };

        let promise = self.create_promise_object();
        let promise_id = if let JsValue::Object(ref o) = promise {
            o.id
        } else {
            0
        };
        let (resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);

        if items.is_empty() {
            let arr = self.create_array(vec![]);
            let _ = self.call_function(&resolve_fn, &JsValue::Undefined, &[arr]);
            return Completion::Normal(promise);
        }

        let count = items.len();
        let remaining = Rc::new(Cell::new(count));
        let results = Rc::new(RefCell::new(vec![JsValue::Undefined; count]));

        for (i, item) in items.into_iter().enumerate() {
            let p = self.promise_resolve_value(&item);
            let remaining = remaining.clone();
            let results = results.clone();
            let resolve_fn = resolve_fn.clone();
            let reject_fn_clone = reject_fn.clone();

            let on_fulfilled = self.create_function(JsFunction::native(
                "".to_string(),
                1,
                move |interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    results.borrow_mut()[i] = val;
                    let r = remaining.get() - 1;
                    remaining.set(r);
                    if r == 0 {
                        let values = results.borrow().clone();
                        let arr = interp.create_array(values);
                        let _ = interp.call_function(&resolve_fn, &JsValue::Undefined, &[arr]);
                    }
                    Completion::Normal(JsValue::Undefined)
                },
            ));
            let _ = self.promise_then(&p, &on_fulfilled, &reject_fn_clone);
        }

        Completion::Normal(promise)
    }

    fn promise_all_settled(&mut self, iterable: &JsValue) -> Completion {
        let items = match self.iterate_to_vec(iterable) {
            Ok(v) => v,
            Err(e) => {
                let promise = self.create_promise_object();
                let pid = if let JsValue::Object(ref o) = promise {
                    o.id
                } else {
                    0
                };
                self.reject_promise(pid, e);
                return Completion::Normal(promise);
            }
        };

        let promise = self.create_promise_object();
        let promise_id = if let JsValue::Object(ref o) = promise {
            o.id
        } else {
            0
        };
        let (resolve_fn, _reject_fn) = self.create_resolving_functions(promise_id);

        if items.is_empty() {
            let arr = self.create_array(vec![]);
            let _ = self.call_function(&resolve_fn, &JsValue::Undefined, &[arr]);
            return Completion::Normal(promise);
        }

        let count = items.len();
        let remaining = Rc::new(Cell::new(count));
        let results = Rc::new(RefCell::new(vec![JsValue::Undefined; count]));

        for (i, item) in items.into_iter().enumerate() {
            let p = self.promise_resolve_value(&item);
            let remaining_f = remaining.clone();
            let results_f = results.clone();
            let resolve_fn_f = resolve_fn.clone();
            let remaining_r = remaining.clone();
            let results_r = results.clone();
            let resolve_fn_r = resolve_fn.clone();

            let on_fulfilled = self.create_function(JsFunction::native(
                "".to_string(),
                1,
                move |interp, _this, args| {
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
                        let _ = interp.call_function(&resolve_fn_f, &JsValue::Undefined, &[arr]);
                    }
                    Completion::Normal(JsValue::Undefined)
                },
            ));
            let on_rejected = self.create_function(JsFunction::native(
                "".to_string(),
                1,
                move |interp, _this, args| {
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
                        let _ = interp.call_function(&resolve_fn_r, &JsValue::Undefined, &[arr]);
                    }
                    Completion::Normal(JsValue::Undefined)
                },
            ));
            let _ = self.promise_then(&p, &on_fulfilled, &on_rejected);
        }

        Completion::Normal(promise)
    }

    fn promise_race(&mut self, iterable: &JsValue) -> Completion {
        let items = match self.iterate_to_vec(iterable) {
            Ok(v) => v,
            Err(e) => {
                let promise = self.create_promise_object();
                let pid = if let JsValue::Object(ref o) = promise {
                    o.id
                } else {
                    0
                };
                self.reject_promise(pid, e);
                return Completion::Normal(promise);
            }
        };

        let promise = self.create_promise_object();
        let promise_id = if let JsValue::Object(ref o) = promise {
            o.id
        } else {
            0
        };
        let (resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);

        for item in items {
            let p = self.promise_resolve_value(&item);
            let _ = self.promise_then(&p, &resolve_fn, &reject_fn);
        }

        Completion::Normal(promise)
    }

    fn promise_any(&mut self, iterable: &JsValue) -> Completion {
        let items = match self.iterate_to_vec(iterable) {
            Ok(v) => v,
            Err(e) => {
                let promise = self.create_promise_object();
                let pid = if let JsValue::Object(ref o) = promise {
                    o.id
                } else {
                    0
                };
                self.reject_promise(pid, e);
                return Completion::Normal(promise);
            }
        };

        let promise = self.create_promise_object();
        let promise_id = if let JsValue::Object(ref o) = promise {
            o.id
        } else {
            0
        };
        let (resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);

        if items.is_empty() {
            // Reject with AggregateError
            let err = self.create_aggregate_error(vec![], "All promises were rejected");
            let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[err]);
            return Completion::Normal(promise);
        }

        let count = items.len();
        let remaining = Rc::new(Cell::new(count));
        let errors = Rc::new(RefCell::new(vec![JsValue::Undefined; count]));

        for (i, item) in items.into_iter().enumerate() {
            let p = self.promise_resolve_value(&item);
            let remaining = remaining.clone();
            let errors = errors.clone();
            let reject_fn_clone = reject_fn.clone();

            let on_rejected = self.create_function(JsFunction::native(
                "".to_string(),
                1,
                move |interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    errors.borrow_mut()[i] = val;
                    let r = remaining.get() - 1;
                    remaining.set(r);
                    if r == 0 {
                        let errs = errors.borrow().clone();
                        let err = interp.create_aggregate_error(errs, "All promises were rejected");
                        let _ = interp.call_function(&reject_fn_clone, &JsValue::Undefined, &[err]);
                    }
                    Completion::Normal(JsValue::Undefined)
                },
            ));
            let _ = self.promise_then(&p, &resolve_fn, &on_rejected);
        }

        Completion::Normal(promise)
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
            return obj.borrow().callable.is_some();
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
                    JsFunction::User { .. } => true,
                };
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
