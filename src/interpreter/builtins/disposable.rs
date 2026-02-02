use super::*;

impl Interpreter {
    pub(crate) fn setup_disposable_stack(&mut self) {
        let ds_proto = self.create_object();
        if let Some(ref op) = self.object_prototype {
            ds_proto.borrow_mut().prototype = Some(op.clone());
        }

        // Symbol.toStringTag
        if let Some(key) = self.get_symbol_key("toStringTag") {
            ds_proto.borrow_mut().insert_property(
                key,
                PropertyDescriptor::data(
                    JsValue::String(JsString::from_str("DisposableStack")),
                    false,
                    false,
                    true,
                ),
            );
        }

        // dispose()
        let dispose_fn = self.create_function(JsFunction::native(
            "dispose".to_string(),
            0,
            |interp, this, _args| interp.disposable_stack_dispose(this, false),
        ));
        ds_proto
            .borrow_mut()
            .insert_value("dispose".to_string(), dispose_fn.clone());

        // Symbol.dispose = dispose
        if let Some(key) = self.get_symbol_key("dispose") {
            ds_proto
                .borrow_mut()
                .insert_property(key, PropertyDescriptor::data(dispose_fn, true, false, true));
        }

        // get disposed
        let disposed_getter = self.create_function(JsFunction::native(
            "get disposed".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let b = obj.borrow();
                    if let Some(ref ds) = b.disposable_stack {
                        return Completion::Normal(JsValue::Boolean(ds.disposed));
                    }
                }
                Completion::Throw(interp.create_type_error(
                    "DisposableStack.prototype.disposed called on non-DisposableStack",
                ))
            },
        ));
        ds_proto.borrow_mut().insert_property(
            "disposed".to_string(),
            PropertyDescriptor::accessor(Some(disposed_getter), None, true, true),
        );

        // use(value)
        let use_fn = self.create_function(JsFunction::native(
            "use".to_string(),
            1,
            |interp, this, args| {
                let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let disposed = {
                        let b = obj.borrow();
                        match &b.disposable_stack {
                            Some(ds) => {
                                if ds.disposed {
                                    return Completion::Throw(interp.create_reference_error(
                                        "DisposableStack has already been disposed",
                                    ));
                                }
                                false
                            }
                            None => {
                                return Completion::Throw(
                                    interp.create_type_error("Not a DisposableStack"),
                                );
                            }
                        }
                    };
                    let _ = disposed;
                    if matches!(value, JsValue::Null | JsValue::Undefined) {
                        return Completion::Normal(value);
                    }
                    // Get Symbol.dispose method
                    let method =
                        if let Some(key) = interp.get_symbol_key("dispose") {
                            if let JsValue::Object(vo) = &value {
                                if let Some(vobj) = interp.get_object(vo.id) {
                                    let m = vobj.borrow().get_property(&key);
                                    if matches!(m, JsValue::Undefined) {
                                        return Completion::Throw(interp.create_type_error(
                                            "Object does not have a [Symbol.dispose] method",
                                        ));
                                    }
                                    m
                                } else {
                                    return Completion::Throw(
                                        interp.create_type_error("Invalid object"),
                                    );
                                }
                            } else {
                                return Completion::Throw(interp.create_type_error(
                                    "Using requires an object or null/undefined",
                                ));
                            }
                        } else {
                            return Completion::Throw(
                                interp.create_type_error("Symbol.dispose not available"),
                            );
                        };
                    if !interp.is_callable(&method) {
                        return Completion::Throw(
                            interp.create_type_error("[Symbol.dispose] is not a function"),
                        );
                    }
                    let resource = DisposableResource {
                        value: value.clone(),
                        hint: DisposeHint::Sync,
                        dispose_method: method,
                    };
                    if let Some(obj) = interp.get_object(o.id) {
                        let mut b = obj.borrow_mut();
                        if let Some(ref mut ds) = b.disposable_stack {
                            ds.stack.push(resource);
                        }
                    }
                    return Completion::Normal(value);
                }
                Completion::Throw(interp.create_type_error("Not a DisposableStack"))
            },
        ));
        ds_proto
            .borrow_mut()
            .insert_value("use".to_string(), use_fn);

        // adopt(value, onDispose)
        let adopt_fn = self.create_function(JsFunction::native(
            "adopt".to_string(),
            2,
            |interp, this, args| {
                let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                let on_dispose = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    {
                        let b = obj.borrow();
                        match &b.disposable_stack {
                            Some(ds) if ds.disposed => {
                                return Completion::Throw(interp.create_reference_error(
                                    "DisposableStack has already been disposed",
                                ));
                            }
                            Some(_) => {}
                            None => {
                                return Completion::Throw(
                                    interp.create_type_error("Not a DisposableStack"),
                                );
                            }
                        }
                    }
                    if !interp.is_callable(&on_dispose) {
                        return Completion::Throw(
                            interp.create_type_error("onDispose must be a function"),
                        );
                    }
                    // Create a wrapper that calls onDispose(value)
                    let wrapper_val = value.clone();
                    let wrapper_dispose = on_dispose.clone();
                    let wrapper_fn = interp.create_function(JsFunction::native(
                        "".to_string(),
                        0,
                        move |interp2, _this2, _args2| {
                            interp2.call_function(
                                &wrapper_dispose,
                                &JsValue::Undefined,
                                &[wrapper_val.clone()],
                            )
                        },
                    ));
                    let resource = DisposableResource {
                        value: JsValue::Undefined,
                        hint: DisposeHint::Sync,
                        dispose_method: wrapper_fn,
                    };
                    if let Some(obj2) = interp.get_object(o.id) {
                        let mut b = obj2.borrow_mut();
                        if let Some(ref mut ds) = b.disposable_stack {
                            ds.stack.push(resource);
                        }
                    }
                    return Completion::Normal(value);
                }
                Completion::Throw(interp.create_type_error("Not a DisposableStack"))
            },
        ));
        ds_proto
            .borrow_mut()
            .insert_value("adopt".to_string(), adopt_fn);

        // defer(onDispose)
        let defer_fn = self.create_function(JsFunction::native(
            "defer".to_string(),
            1,
            |interp, this, args| {
                let on_dispose = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    {
                        let b = obj.borrow();
                        match &b.disposable_stack {
                            Some(ds) if ds.disposed => {
                                return Completion::Throw(interp.create_reference_error(
                                    "DisposableStack has already been disposed",
                                ));
                            }
                            Some(_) => {}
                            None => {
                                return Completion::Throw(
                                    interp.create_type_error("Not a DisposableStack"),
                                );
                            }
                        }
                    }
                    if !interp.is_callable(&on_dispose) {
                        return Completion::Throw(
                            interp.create_type_error("onDispose must be a function"),
                        );
                    }
                    let resource = DisposableResource {
                        value: JsValue::Undefined,
                        hint: DisposeHint::Sync,
                        dispose_method: on_dispose,
                    };
                    if let Some(obj2) = interp.get_object(o.id) {
                        let mut b = obj2.borrow_mut();
                        if let Some(ref mut ds) = b.disposable_stack {
                            ds.stack.push(resource);
                        }
                    }
                    return Completion::Normal(JsValue::Undefined);
                }
                Completion::Throw(interp.create_type_error("Not a DisposableStack"))
            },
        ));
        ds_proto
            .borrow_mut()
            .insert_value("defer".to_string(), defer_fn);

        // move()
        let move_fn = self.create_function(JsFunction::native(
            "move".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let stack = {
                        let mut b = obj.borrow_mut();
                        match &mut b.disposable_stack {
                            Some(ds) if ds.disposed => {
                                return Completion::Throw(interp.create_reference_error(
                                    "DisposableStack has already been disposed",
                                ));
                            }
                            Some(ds) => {
                                let s = std::mem::take(&mut ds.stack);
                                ds.disposed = true;
                                s
                            }
                            None => {
                                return Completion::Throw(
                                    interp.create_type_error("Not a DisposableStack"),
                                );
                            }
                        }
                    };
                    // Create a new DisposableStack with the moved resources
                    let new_obj = interp.create_object();
                    {
                        // Get DisposableStack prototype
                        let env = interp.global_env.borrow();
                        if let Some(ctor_val) = env.get("DisposableStack")
                            && let JsValue::Object(ctor) = &ctor_val
                            && let Some(ctor_obj) = interp.get_object(ctor.id)
                        {
                            let proto_val = ctor_obj.borrow().get_property("prototype");
                            if let JsValue::Object(p) = &proto_val
                                && let Some(proto_rc) = interp.get_object(p.id)
                            {
                                new_obj.borrow_mut().prototype = Some(proto_rc);
                            }
                        }
                    }
                    new_obj.borrow_mut().disposable_stack = Some(DisposableStackData {
                        stack,
                        disposed: false,
                    });
                    let id = new_obj.borrow().id.unwrap();
                    return Completion::Normal(JsValue::Object(crate::types::JsObject { id }));
                }
                Completion::Throw(interp.create_type_error("Not a DisposableStack"))
            },
        ));
        ds_proto
            .borrow_mut()
            .insert_value("move".to_string(), move_fn);

        // Set constructor property on prototype
        let ds_proto_clone = ds_proto.clone();
        self.register_global_fn(
            "DisposableStack",
            BindingKind::Var,
            JsFunction::constructor(
                "DisposableStack".to_string(),
                0,
                move |interp, this, _args| {
                    if let JsValue::Object(o) = this {
                        if let Some(obj) = interp.get_object(o.id) {
                            let mut b = obj.borrow_mut();
                            b.class_name = "DisposableStack".to_string();
                            b.prototype = Some(ds_proto_clone.clone());
                            b.disposable_stack = Some(DisposableStackData {
                                stack: Vec::new(),
                                disposed: false,
                            });
                        }
                        return Completion::Normal(this.clone());
                    }
                    let obj = interp.create_object();
                    obj.borrow_mut().disposable_stack = Some(DisposableStackData {
                        stack: Vec::new(),
                        disposed: false,
                    });
                    let id = obj.borrow().id.unwrap();
                    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
                },
            ),
        );

        // Wire up constructor and prototype
        {
            let env = self.global_env.borrow();
            if let Some(ctor_val) = env.get("DisposableStack") {
                ds_proto
                    .borrow_mut()
                    .insert_builtin("constructor".to_string(), ctor_val);
            }
        }
        {
            let env = self.global_env.borrow();
            if let Some(ctor_val) = env.get("DisposableStack")
                && let JsValue::Object(o) = &ctor_val
                && let Some(ctor_obj) = self.get_object(o.id)
            {
                let proto_id = ds_proto.borrow().id.unwrap();
                ctor_obj.borrow_mut().insert_builtin(
                    "prototype".to_string(),
                    JsValue::Object(crate::types::JsObject { id: proto_id }),
                );
            }
        }
    }

    pub(crate) fn disposable_stack_dispose(
        &mut self,
        this: &JsValue,
        _is_async: bool,
    ) -> Completion {
        if let JsValue::Object(o) = this
            && let Some(obj) = self.get_object(o.id)
        {
            let stack = {
                let mut b = obj.borrow_mut();
                match &mut b.disposable_stack {
                    Some(ds) => {
                        if ds.disposed {
                            return Completion::Normal(JsValue::Undefined);
                        }
                        ds.disposed = true;
                        std::mem::take(&mut ds.stack)
                    }
                    None => {
                        return Completion::Throw(self.create_type_error("Not a DisposableStack"));
                    }
                }
            };

            let mut current_error: Option<JsValue> = None;
            for resource in stack.iter().rev() {
                let result = self.call_function(&resource.dispose_method, &resource.value, &[]);
                match result {
                    Completion::Normal(_) => {}
                    Completion::Throw(e) => {
                        current_error = Some(self.wrap_suppressed_error(e, current_error));
                    }
                    _ => {}
                }
            }

            if let Some(err) = current_error {
                Completion::Throw(err)
            } else {
                Completion::Normal(JsValue::Undefined)
            }
        } else {
            Completion::Throw(self.create_type_error(
                "DisposableStack.prototype.dispose called on non-DisposableStack",
            ))
        }
    }

    pub(crate) fn setup_async_disposable_stack(&mut self) {
        let ads_proto = self.create_object();
        if let Some(ref op) = self.object_prototype {
            ads_proto.borrow_mut().prototype = Some(op.clone());
        }

        // Symbol.toStringTag
        if let Some(key) = self.get_symbol_key("toStringTag") {
            ads_proto.borrow_mut().insert_property(
                key,
                PropertyDescriptor::data(
                    JsValue::String(JsString::from_str("AsyncDisposableStack")),
                    false,
                    false,
                    true,
                ),
            );
        }

        // disposeAsync()
        let dispose_async_fn = self.create_function(JsFunction::native(
            "disposeAsync".to_string(),
            0,
            |interp, this, _args| {
                let result = interp.async_disposable_stack_dispose(this);
                match result {
                    Completion::Normal(_) => interp.create_resolved_promise(JsValue::Undefined),
                    Completion::Throw(e) => interp.create_rejected_promise(e),
                    other => other,
                }
            },
        ));
        ads_proto
            .borrow_mut()
            .insert_value("disposeAsync".to_string(), dispose_async_fn.clone());

        // Symbol.asyncDispose = disposeAsync
        if let Some(key) = self.get_symbol_key("asyncDispose") {
            ads_proto.borrow_mut().insert_property(
                key,
                PropertyDescriptor::data(dispose_async_fn, true, false, true),
            );
        }

        // get disposed
        let disposed_getter = self.create_function(JsFunction::native(
            "get disposed".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let b = obj.borrow();
                    if let Some(ref ds) = b.disposable_stack {
                        return Completion::Normal(JsValue::Boolean(ds.disposed));
                    }
                }
                Completion::Throw(interp.create_type_error(
                    "AsyncDisposableStack.prototype.disposed called on non-AsyncDisposableStack",
                ))
            },
        ));
        ads_proto.borrow_mut().insert_property(
            "disposed".to_string(),
            PropertyDescriptor::accessor(Some(disposed_getter), None, true, true),
        );

        // use(value)
        let use_fn = self.create_function(JsFunction::native(
            "use".to_string(),
            1,
            |interp, this, args| {
                let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    {
                        let b = obj.borrow();
                        match &b.disposable_stack {
                            Some(ds) if ds.disposed => {
                                return Completion::Throw(interp.create_reference_error(
                                    "AsyncDisposableStack has already been disposed",
                                ));
                            }
                            Some(_) => {}
                            None => {
                                return Completion::Throw(
                                    interp.create_type_error("Not an AsyncDisposableStack"),
                                );
                            }
                        }
                    }
                    if matches!(value, JsValue::Null | JsValue::Undefined) {
                        return Completion::Normal(value);
                    }
                    // Try Symbol.asyncDispose first, then Symbol.dispose
                    let mut method = JsValue::Undefined;
                    let mut hint = DisposeHint::Async;
                    if let Some(key) = interp.get_symbol_key("asyncDispose") {
                        if let JsValue::Object(vo) = &value {
                            if let Some(vobj) = interp.get_object(vo.id) {
                                let m = vobj.borrow().get_property(&key);
                                if !matches!(m, JsValue::Undefined) {
                                    method = m;
                                }
                            }
                        }
                    }
                    if matches!(method, JsValue::Undefined) {
                        if let Some(key) = interp.get_symbol_key("dispose") {
                            if let JsValue::Object(vo) = &value {
                                if let Some(vobj) = interp.get_object(vo.id) {
                                    let m = vobj.borrow().get_property(&key);
                                    if !matches!(m, JsValue::Undefined) {
                                        method = m;
                                        hint = DisposeHint::Sync;
                                    }
                                }
                            }
                        }
                    }
                    if matches!(method, JsValue::Undefined) {
                        return Completion::Throw(
                            interp.create_type_error("Object is not disposable"),
                        );
                    }
                    if !interp.is_callable(&method) {
                        return Completion::Throw(
                            interp.create_type_error("dispose method is not a function"),
                        );
                    }
                    let resource = DisposableResource {
                        value: value.clone(),
                        hint,
                        dispose_method: method,
                    };
                    if let Some(obj2) = interp.get_object(o.id) {
                        let mut b = obj2.borrow_mut();
                        if let Some(ref mut ds) = b.disposable_stack {
                            ds.stack.push(resource);
                        }
                    }
                    return Completion::Normal(value);
                }
                Completion::Throw(interp.create_type_error("Not an AsyncDisposableStack"))
            },
        ));
        ads_proto
            .borrow_mut()
            .insert_value("use".to_string(), use_fn);

        // adopt(value, onDispose)
        let adopt_fn = self.create_function(JsFunction::native(
            "adopt".to_string(),
            2,
            |interp, this, args| {
                let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                let on_dispose = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    {
                        let b = obj.borrow();
                        match &b.disposable_stack {
                            Some(ds) if ds.disposed => {
                                return Completion::Throw(interp.create_reference_error(
                                    "AsyncDisposableStack has already been disposed",
                                ));
                            }
                            Some(_) => {}
                            None => {
                                return Completion::Throw(
                                    interp.create_type_error("Not an AsyncDisposableStack"),
                                );
                            }
                        }
                    }
                    if !interp.is_callable(&on_dispose) {
                        return Completion::Throw(
                            interp.create_type_error("onDispose must be a function"),
                        );
                    }
                    let wrapper_val = value.clone();
                    let wrapper_dispose = on_dispose.clone();
                    let wrapper_fn = interp.create_function(JsFunction::native(
                        "".to_string(),
                        0,
                        move |interp2, _this2, _args2| {
                            interp2.call_function(
                                &wrapper_dispose,
                                &JsValue::Undefined,
                                &[wrapper_val.clone()],
                            )
                        },
                    ));
                    let resource = DisposableResource {
                        value: JsValue::Undefined,
                        hint: DisposeHint::Async,
                        dispose_method: wrapper_fn,
                    };
                    if let Some(obj2) = interp.get_object(o.id) {
                        let mut b = obj2.borrow_mut();
                        if let Some(ref mut ds) = b.disposable_stack {
                            ds.stack.push(resource);
                        }
                    }
                    return Completion::Normal(value);
                }
                Completion::Throw(interp.create_type_error("Not an AsyncDisposableStack"))
            },
        ));
        ads_proto
            .borrow_mut()
            .insert_value("adopt".to_string(), adopt_fn);

        // defer(onDispose)
        let defer_fn = self.create_function(JsFunction::native(
            "defer".to_string(),
            1,
            |interp, this, args| {
                let on_dispose = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    {
                        let b = obj.borrow();
                        match &b.disposable_stack {
                            Some(ds) if ds.disposed => {
                                return Completion::Throw(interp.create_reference_error(
                                    "AsyncDisposableStack has already been disposed",
                                ));
                            }
                            Some(_) => {}
                            None => {
                                return Completion::Throw(
                                    interp.create_type_error("Not an AsyncDisposableStack"),
                                );
                            }
                        }
                    }
                    if !interp.is_callable(&on_dispose) {
                        return Completion::Throw(
                            interp.create_type_error("onDispose must be a function"),
                        );
                    }
                    let resource = DisposableResource {
                        value: JsValue::Undefined,
                        hint: DisposeHint::Async,
                        dispose_method: on_dispose,
                    };
                    if let Some(obj2) = interp.get_object(o.id) {
                        let mut b = obj2.borrow_mut();
                        if let Some(ref mut ds) = b.disposable_stack {
                            ds.stack.push(resource);
                        }
                    }
                    return Completion::Normal(JsValue::Undefined);
                }
                Completion::Throw(interp.create_type_error("Not an AsyncDisposableStack"))
            },
        ));
        ads_proto
            .borrow_mut()
            .insert_value("defer".to_string(), defer_fn);

        // move()
        let move_fn = self.create_function(JsFunction::native(
            "move".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let stack = {
                        let mut b = obj.borrow_mut();
                        match &mut b.disposable_stack {
                            Some(ds) if ds.disposed => {
                                return Completion::Throw(interp.create_reference_error(
                                    "AsyncDisposableStack has already been disposed",
                                ));
                            }
                            Some(ds) => {
                                let s = std::mem::take(&mut ds.stack);
                                ds.disposed = true;
                                s
                            }
                            None => {
                                return Completion::Throw(
                                    interp.create_type_error("Not an AsyncDisposableStack"),
                                );
                            }
                        }
                    };
                    let new_obj = interp.create_object();
                    {
                        let env = interp.global_env.borrow();
                        if let Some(ctor_val) = env.get("AsyncDisposableStack")
                            && let JsValue::Object(ctor) = &ctor_val
                            && let Some(ctor_obj) = interp.get_object(ctor.id)
                        {
                            let proto_val = ctor_obj.borrow().get_property("prototype");
                            if let JsValue::Object(p) = &proto_val
                                && let Some(proto_rc) = interp.get_object(p.id)
                            {
                                new_obj.borrow_mut().prototype = Some(proto_rc);
                            }
                        }
                    }
                    new_obj.borrow_mut().disposable_stack = Some(DisposableStackData {
                        stack,
                        disposed: false,
                    });
                    let id = new_obj.borrow().id.unwrap();
                    return Completion::Normal(JsValue::Object(crate::types::JsObject { id }));
                }
                Completion::Throw(interp.create_type_error("Not an AsyncDisposableStack"))
            },
        ));
        ads_proto
            .borrow_mut()
            .insert_value("move".to_string(), move_fn);

        // Constructor
        let ads_proto_clone = ads_proto.clone();
        self.register_global_fn(
            "AsyncDisposableStack",
            BindingKind::Var,
            JsFunction::constructor(
                "AsyncDisposableStack".to_string(),
                0,
                move |interp, this, _args| {
                    if let JsValue::Object(o) = this {
                        if let Some(obj) = interp.get_object(o.id) {
                            let mut b = obj.borrow_mut();
                            b.class_name = "AsyncDisposableStack".to_string();
                            b.prototype = Some(ads_proto_clone.clone());
                            b.disposable_stack = Some(DisposableStackData {
                                stack: Vec::new(),
                                disposed: false,
                            });
                        }
                        return Completion::Normal(this.clone());
                    }
                    let obj = interp.create_object();
                    obj.borrow_mut().disposable_stack = Some(DisposableStackData {
                        stack: Vec::new(),
                        disposed: false,
                    });
                    let id = obj.borrow().id.unwrap();
                    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
                },
            ),
        );

        // Wire up
        {
            let env = self.global_env.borrow();
            if let Some(ctor_val) = env.get("AsyncDisposableStack") {
                ads_proto
                    .borrow_mut()
                    .insert_builtin("constructor".to_string(), ctor_val);
            }
        }
        {
            let env = self.global_env.borrow();
            if let Some(ctor_val) = env.get("AsyncDisposableStack")
                && let JsValue::Object(o) = &ctor_val
                && let Some(ctor_obj) = self.get_object(o.id)
            {
                let proto_id = ads_proto.borrow().id.unwrap();
                ctor_obj.borrow_mut().insert_builtin(
                    "prototype".to_string(),
                    JsValue::Object(crate::types::JsObject { id: proto_id }),
                );
            }
        }
    }

    fn async_disposable_stack_dispose(&mut self, this: &JsValue) -> Completion {
        if let JsValue::Object(o) = this
            && let Some(obj) = self.get_object(o.id)
        {
            let stack = {
                let mut b = obj.borrow_mut();
                match &mut b.disposable_stack {
                    Some(ds) => {
                        if ds.disposed {
                            return Completion::Normal(JsValue::Undefined);
                        }
                        ds.disposed = true;
                        std::mem::take(&mut ds.stack)
                    }
                    None => {
                        return Completion::Throw(
                            self.create_type_error("Not an AsyncDisposableStack"),
                        );
                    }
                }
            };

            let mut current_error: Option<JsValue> = None;
            for resource in stack.iter().rev() {
                let result = self.call_function(&resource.dispose_method, &resource.value, &[]);
                match result {
                    Completion::Normal(v) => {
                        if resource.hint == DisposeHint::Async {
                            match self.await_value(&v) {
                                Completion::Normal(_) => {}
                                Completion::Throw(e) => {
                                    current_error =
                                        Some(self.wrap_suppressed_error(e, current_error));
                                }
                                _ => {}
                            }
                        }
                    }
                    Completion::Throw(e) => {
                        current_error = Some(self.wrap_suppressed_error(e, current_error));
                    }
                    _ => {}
                }
            }

            if let Some(err) = current_error {
                Completion::Throw(err)
            } else {
                Completion::Normal(JsValue::Undefined)
            }
        } else {
            Completion::Throw(self.create_type_error("Not an AsyncDisposableStack"))
        }
    }
}
