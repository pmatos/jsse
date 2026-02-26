use super::*;

enum IdentifierRef {
    WithObject(u64),
    Binding,
}

impl Interpreter {
    fn resolve_private_name(&self, source_name: &str, env: &EnvRef) -> String {
        let mut current = Some(env.clone());
        while let Some(e) = current {
            let next = {
                let borrowed = e.borrow();
                if let Some(ref names) = borrowed.class_private_names
                    && let Some(branded) = names.get(source_name)
                {
                    return branded.clone();
                }
                borrowed.parent.clone()
            };
            current = next;
        }
        source_name.to_string()
    }

    /// Check if `this` is in TDZ (derived constructor before super() called).
    /// Walks up the environment chain to find the `this` binding.
    fn this_is_in_tdz(env: &EnvRef) -> bool {
        let e = env.borrow();
        if e.bindings.contains_key("this") {
            return e.is_in_tdz("this");
        }
        if let Some(ref parent) = e.parent {
            return Self::this_is_in_tdz(parent);
        }
        false
    }

    /// Initialize the `this` binding in a derived constructor's environment.
    /// Walks up to find the function scope's `this` binding and marks it initialized.
    fn initialize_this_binding(env: &EnvRef, value: JsValue) {
        let mut e = env.borrow_mut();
        if e.bindings.contains_key("this") {
            e.bindings.insert(
                "this".to_string(),
                crate::interpreter::types::Binding {
                    value,
                    kind: crate::interpreter::types::BindingKind::Const,
                    initialized: true,
                    deletable: false,
                },
            );
            return;
        }
        if let Some(ref parent) = e.parent {
            let parent = parent.clone();
            drop(e);
            Self::initialize_this_binding(&parent, value);
        }
    }

    /// Initialize instance elements (private/public fields) after super() in derived constructor.
    fn initialize_instance_elements(
        &mut self,
        this_val: JsValue,
        env: &EnvRef,
    ) -> Result<(), JsValue> {
        // Find the new.target constructor (which has the field defs for the current class)
        let new_target_val = if let Some(ref nt) = self.new_target {
            nt.clone()
        } else {
            return Ok(());
        };
        let instance_field_defs = if let JsValue::Object(ref o) = new_target_val
            && let Some(func_obj) = self.get_object(o.id)
        {
            func_obj.borrow().class_instance_field_defs.clone()
        } else {
            return Ok(());
        };
        let this_obj_id = if let JsValue::Object(ref o) = this_val {
            o.id
        } else {
            return Ok(());
        };
        // Create env for evaluating field initializers.
        // Use the class_env's parent (the outer scope) so that __super__ is NOT
        // accessible via eval() in field initializers (super() should be SyntaxError there).
        let (outer_env, class_pn) = if let JsValue::Object(ref o) = new_target_val
            && let Some(func_obj) = self.get_object(o.id)
        {
            if let Some(JsFunction::User { ref closure, .. }) = func_obj.borrow().callable {
                let cls_env = closure.borrow();
                (cls_env.parent.clone(), cls_env.class_private_names.clone())
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };
        let init_parent = outer_env.unwrap_or_else(|| env.clone());
        let init_env = Environment::new(Some(init_parent));
        init_env.borrow_mut().bindings.insert(
            "this".to_string(),
            crate::interpreter::types::Binding {
                value: this_val.clone(),
                kind: crate::interpreter::types::BindingKind::Const,
                initialized: true,
                deletable: false,
            },
        );
        init_env.borrow_mut().class_private_names = class_pn;
        init_env.borrow_mut().is_field_initializer = true;
        // Set __home_object__ for super property access in field initializers.
        // Instance field HomeObject = class prototype.
        if let JsValue::Object(ref o) = new_target_val
            && let Some(ctor_obj) = self.get_object(o.id)
        {
            let proto_val = ctor_obj.borrow().get_property("prototype");
            if let JsValue::Object(_) = &proto_val {
                init_env.borrow_mut().bindings.insert(
                    "__home_object__".to_string(),
                    crate::interpreter::types::Binding {
                        value: proto_val,
                        kind: crate::interpreter::types::BindingKind::Const,
                        initialized: true,
                        deletable: false,
                    },
                );
            }
        }
        // Pass 1: Install private methods and accessors before any field initializer runs.
        for idef in &instance_field_defs {
            match idef {
                InstanceFieldDef::Private(PrivateFieldDef::Method { name, value }) => {
                    if let Some(obj) = self.get_object(this_obj_id) {
                        if !obj.borrow().extensible {
                            return Err(self.create_type_error(
                                "Cannot define private method on non-extensible object",
                            ));
                        }
                        if obj.borrow().private_fields.contains_key(name) {
                            return Err(
                                self.create_type_error("Cannot add private method to object twice")
                            );
                        }
                        obj.borrow_mut()
                            .private_fields
                            .insert(name.clone(), PrivateElement::Method(value.clone()));
                    }
                }
                InstanceFieldDef::Private(PrivateFieldDef::Accessor { name, get, set }) => {
                    if let Some(obj) = self.get_object(this_obj_id) {
                        if !obj.borrow().extensible {
                            return Err(self.create_type_error(
                                "Cannot define private accessor on non-extensible object",
                            ));
                        }
                        if obj.borrow().private_fields.contains_key(name) {
                            return Err(self.create_type_error(
                                "Cannot add private accessor to object twice",
                            ));
                        }
                        obj.borrow_mut().private_fields.insert(
                            name.clone(),
                            PrivateElement::Accessor {
                                get: get.clone(),
                                set: set.clone(),
                            },
                        );
                    }
                }
                _ => {}
            }
        }
        // Pass 2: Run field initializers in source order.
        for idef in &instance_field_defs {
            match idef {
                InstanceFieldDef::Private(PrivateFieldDef::Field { name, initializer }) => {
                    let val = if let Some(init) = initializer {
                        match self.eval_expr(init, &init_env) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Err(e),
                            _ => JsValue::Undefined,
                        }
                    } else {
                        JsValue::Undefined
                    };
                    if let Some(obj) = self.get_object(this_obj_id) {
                        if !obj.borrow().extensible {
                            return Err(self.create_type_error(
                                "Cannot define private field on non-extensible object",
                            ));
                        }
                        if obj.borrow().private_fields.contains_key(name) {
                            return Err(self.create_type_error(
                                "Cannot initialize private field twice on the same object",
                            ));
                        }
                        obj.borrow_mut()
                            .private_fields
                            .insert(name.clone(), PrivateElement::Field(val));
                    }
                }
                InstanceFieldDef::Public(key, initializer) => {
                    let val = if let Some(init) = initializer {
                        match self.eval_expr(init, &init_env) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Err(e),
                            _ => JsValue::Undefined,
                        }
                    } else {
                        JsValue::Undefined
                    };
                    crate::interpreter::builtins::array::create_data_property_or_throw(
                        self, &this_val, key, val,
                    )?;
                }
                _ => {} // Methods/accessors handled in pass 1
            }
        }
        Ok(())
    }

    pub(crate) fn eval_expr(&mut self, expr: &Expression, env: &EnvRef) -> Completion {
        match expr {
            Expression::Literal(lit) => Completion::Normal(self.eval_literal(lit)),
            Expression::Identifier(name) => {
                let strict = env.borrow().strict;
                match self.resolve_with_has_binding(name, env) {
                    Ok(Some(obj_id)) => {
                        return self.with_get_binding_value(obj_id, name, strict);
                    }
                    Ok(None) => {}
                    Err(e) => return Completion::Throw(e),
                }
                if let Some(result) = self.resolve_global_getter(name, env) {
                    return result;
                }
                match env.borrow().get(name) {
                    Some(val) => Completion::Normal(val),
                    None => {
                        let err = self.create_reference_error(&format!("{name} is not defined"));
                        Completion::Throw(err)
                    }
                }
            }

            Expression::This => {
                match env.borrow().get("this") {
                    Some(v) => Completion::Normal(v),
                    None => {
                        // Check if this is TDZ (derived constructor before super())
                        if Self::this_is_in_tdz(env) {
                            Completion::Throw(self.create_reference_error(
                                "Must call super constructor in derived class before accessing 'this' or returning from derived constructor",
                            ))
                        } else {
                            Completion::Normal(JsValue::Undefined)
                        }
                    }
                }
            }
            Expression::Super => {
                Completion::Normal(env.borrow().get("__super__").unwrap_or(JsValue::Undefined))
            }
            Expression::NewTarget => {
                Completion::Normal(self.new_target.clone().unwrap_or(JsValue::Undefined))
            }
            Expression::PrivateIdentifier(_) => Completion::Throw(
                self.create_type_error("Private identifier can only be used with 'in' operator"),
            ),
            Expression::Unary(op, operand) => {
                let val = match self.eval_expr(operand, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                self.eval_unary(*op, &val)
            }
            Expression::Binary(op, left, right) => {
                if *op == BinaryOp::In
                    && let Expression::PrivateIdentifier(name) = left.as_ref()
                {
                    let branded = self.resolve_private_name(name, env);
                    let rval = match self.eval_expr(right, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    return match &rval {
                            JsValue::Object(o) => {
                                if let Some(obj) = self.get_object(o.id) {
                                    Completion::Normal(JsValue::Boolean(
                                        obj.borrow().private_fields.contains_key(&branded),
                                    ))
                                } else {
                                    Completion::Normal(JsValue::Boolean(false))
                                }
                            }
                            _ => Completion::Throw(self.create_type_error(
                                "Cannot use 'in' operator to search for a private field without an object",
                            )),
                        };
                }
                let lval = match self.eval_expr(left, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let rval = match self.eval_expr(right, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if *op == BinaryOp::Instanceof {
                    return self.eval_instanceof(&lval, &rval);
                }
                self.eval_binary(*op, &lval, &rval)
            }
            Expression::Logical(op, left, right) => self.eval_logical(*op, left, right, env),
            Expression::Update(op, prefix, arg) => self.eval_update(*op, *prefix, arg, env),
            Expression::Assign(op, left, right) => self.eval_assign(*op, left, right, env),
            Expression::Conditional(test, cons, alt) => {
                let test_val = match self.eval_expr(test, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if self.to_boolean_val(&test_val) {
                    self.eval_expr(cons, env)
                } else {
                    self.eval_expr(alt, env)
                }
            }
            Expression::Call(callee, args) => self.eval_call(callee, args, env),
            Expression::New(callee, args) => self.eval_new(callee, args, env),
            Expression::Member(obj, prop) => self.eval_member(obj, prop, env),
            Expression::Array(elements) => self.eval_array_literal(elements, env),
            Expression::Object(props) => self.eval_object_literal(props, env),
            Expression::Function(f) => {
                let closure_env = if let Some(ref name) = f.name {
                    let func_env = Rc::new(RefCell::new(Environment {
                        bindings: HashMap::new(),
                        parent: Some(env.clone()),
                        strict: env.borrow().strict,
                        is_function_scope: false,
                        is_arrow_scope: false,
                        with_object: None,
                        dispose_stack: None,
                        global_object: None,
                        annexb_function_names: None,
                        class_private_names: None,
                        is_field_initializer: false,
                        arguments_immutable: false,
                        has_simple_params: true,
                        is_simple_catch_scope: false,
                    }));
                    func_env
                        .borrow_mut()
                        .declare(name, BindingKind::FunctionName);
                    func_env
                } else {
                    env.clone()
                };
                let enclosing_strict = env.borrow().strict;
                let func = JsFunction::User {
                    name: f.name.clone(),
                    params: f.params.clone(),
                    body: f.body.clone(),
                    closure: closure_env.clone(),
                    is_arrow: false,
                    is_strict: f.body_is_strict || enclosing_strict,
                    is_generator: f.is_generator,
                    is_async: f.is_async,
                    is_method: false,
                    source_text: f.source_text.clone(),
                };
                let func_val = self.create_function(func);
                if let Some(name) = &f.name {
                    let _ = closure_env.borrow_mut().set(name, func_val.clone());
                }
                Completion::Normal(func_val)
            }
            Expression::ArrowFunction(af) => {
                let enclosing_strict = env.borrow().strict;
                let body_stmts = match &af.body {
                    ArrowBody::Block(stmts) => stmts.clone(),
                    ArrowBody::Expression(expr) => {
                        vec![Statement::Return(Some(*expr.clone()))]
                    }
                };
                let func = JsFunction::User {
                    name: None,
                    params: af.params.clone(),
                    body: body_stmts.clone(),
                    closure: env.clone(),
                    is_arrow: true,
                    is_strict: af.body_is_strict || enclosing_strict,
                    is_generator: false,
                    is_async: af.is_async,
                    is_method: false,
                    source_text: af.source_text.clone(),
                };
                Completion::Normal(self.create_function(func))
            }
            Expression::Class(ce) => {
                let name = ce.name.clone().unwrap_or_default();
                self.eval_class(
                    &name,
                    &ce.super_class,
                    &ce.body,
                    env,
                    ce.source_text.clone(),
                )
            }
            Expression::Typeof(operand) => {
                if let Expression::Identifier(name) = operand.as_ref() {
                    let strict = env.borrow().strict;
                    match self.resolve_with_has_binding(name, env) {
                        Ok(Some(obj_id)) => {
                            return match self.with_get_binding_value(obj_id, name, strict) {
                                Completion::Normal(val) => Completion::Normal(JsValue::String(
                                    JsString::from_str(typeof_val(&val, &self.objects)),
                                )),
                                other => other,
                            };
                        }
                        Ok(None) => {}
                        Err(e) => return Completion::Throw(e),
                    }
                    if let Some(result) = self.resolve_global_getter(name, env) {
                        return match result {
                            Completion::Normal(val) => Completion::Normal(JsValue::String(
                                JsString::from_str(typeof_val(&val, &self.objects)),
                            )),
                            other => other,
                        };
                    }
                    match env.borrow().get(name) {
                        Some(val) => {
                            return Completion::Normal(JsValue::String(JsString::from_str(
                                typeof_val(&val, &self.objects),
                            )));
                        }
                        None => {
                            if env.borrow().has(name) {
                                return Completion::Throw(self.create_reference_error(&format!(
                                    "Cannot access '{name}' before initialization"
                                )));
                            }
                            return Completion::Normal(JsValue::String(JsString::from_str(
                                "undefined",
                            )));
                        }
                    }
                }
                let val = match self.eval_expr(operand, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                Completion::Normal(JsValue::String(JsString::from_str(typeof_val(
                    &val,
                    &self.objects,
                ))))
            }
            Expression::Void(operand) => {
                match self.eval_expr(operand, env) {
                    Completion::Normal(_) => {}
                    other => return other,
                }
                Completion::Normal(JsValue::Undefined)
            }
            Expression::Delete(expr) => match expr.as_ref() {
                Expression::Member(obj_expr, prop) => {
                    // §13.5.1.2 step 5a: delete super.property must throw ReferenceError
                    if matches!(obj_expr.as_ref(), Expression::Super) {
                        return Completion::Throw(
                            self.create_reference_error("Unsupported reference to 'super'"),
                        );
                    }
                    let obj_val = match self.eval_expr(obj_expr, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let key = match prop {
                        MemberProperty::Dot(name) => name.clone(),
                        MemberProperty::Computed(expr) => match self.eval_expr(expr, env) {
                            Completion::Normal(v) => match self.to_property_key(&v) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            },
                            other => return other,
                        },
                        MemberProperty::Private(_) => {
                            return Completion::Throw(
                                self.create_type_error("Private fields cannot be deleted"),
                            );
                        }
                    };
                    // TypeError for null/undefined base
                    if obj_val.is_null() || obj_val.is_undefined() {
                        return Completion::Throw(self.create_type_error(&format!(
                            "Cannot delete property '{}' of {}",
                            key,
                            if obj_val.is_null() {
                                "null"
                            } else {
                                "undefined"
                            }
                        )));
                    }
                    // Auto-box primitives via to_object
                    let obj_ref = if let JsValue::Object(o) = &obj_val {
                        o.clone()
                    } else {
                        match self.to_object(&obj_val) {
                            Completion::Normal(JsValue::Object(o)) => o,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => return Completion::Normal(JsValue::Boolean(true)),
                        }
                    };
                    if let Some(obj) = self.get_object(obj_ref.id) {
                        // Proxy deleteProperty trap
                        if obj.borrow().is_proxy() || obj.borrow().proxy_revoked {
                            match self.proxy_delete_property(obj_ref.id, &key) {
                                Ok(result) => return Completion::Normal(JsValue::Boolean(result)),
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        // Module namespace exotic: [[Delete]] — only for string keys (not symbols)
                        if !key.starts_with("Symbol(") {
                            let is_ns = obj.borrow().module_namespace.is_some();
                            if is_ns {
                                let export_names = obj
                                    .borrow()
                                    .module_namespace
                                    .as_ref()
                                    .unwrap()
                                    .export_names
                                    .clone();
                                if export_names.contains(&key) {
                                    if env.borrow().strict {
                                        return Completion::Throw(self.create_type_error(
                                            &format!(
                                                "Cannot delete property '{key}' of module namespace"
                                            ),
                                        ));
                                    }
                                    return Completion::Normal(JsValue::Boolean(false));
                                }
                                return Completion::Normal(JsValue::Boolean(true));
                            }
                        }
                        // TypedArray: §10.4.5.4 [[Delete]]
                        {
                            let obj_borrow = obj.borrow();
                            if let Some(ref ta) = obj_borrow.typed_array_info
                                && let Some(index) = canonical_numeric_index_string(&key)
                            {
                                if is_valid_integer_index(ta, index) {
                                    drop(obj_borrow);
                                    let is_strict = env.borrow().strict;
                                    if is_strict {
                                        return Completion::Throw(self.create_type_error(
                                            &format!(
                                                "Cannot delete property '{key}' of a TypedArray"
                                            ),
                                        ));
                                    }
                                    return Completion::Normal(JsValue::Boolean(false));
                                }
                                return Completion::Normal(JsValue::Boolean(true));
                            }
                        }
                        let is_strict = env.borrow().strict;
                        let mut obj_mut = obj.borrow_mut();
                        if let Some(desc) = obj_mut.properties.get(&key)
                            && desc.configurable == Some(false)
                        {
                            if is_strict {
                                drop(obj_mut);
                                return Completion::Throw(self.create_type_error(&format!(
                                    "Cannot delete property '{key}' of object"
                                )));
                            }
                            return Completion::Normal(JsValue::Boolean(false));
                        }
                        obj_mut.properties.remove(&key);
                        obj_mut.property_order.retain(|k| k != &key);
                        if let Some(ref mut map) = obj_mut.parameter_map {
                            map.remove(&key);
                        }
                        if let Ok(idx) = key.parse::<usize>() {
                            if let Some(ref mut elems) = obj_mut.array_elements {
                                if idx < elems.len() {
                                    elems[idx] = JsValue::Undefined;
                                }
                            }
                        }
                    }
                    Completion::Normal(JsValue::Boolean(true))
                }
                Expression::Identifier(name) => {
                    // Check with-scopes first (Bug C fix)
                    match self.resolve_with_has_binding(name, env) {
                        Ok(Some(obj_id)) => {
                            return match self.proxy_delete_property(obj_id, name) {
                                Ok(b) => Completion::Normal(JsValue::Boolean(b)),
                                Err(e) => Completion::Throw(e),
                            };
                        }
                        Ok(None) => {}
                        Err(e) => return Completion::Throw(e),
                    }

                    let mut current = Some(env.clone());
                    let global_env = self.realm().global_env.clone();
                    while let Some(ref e) = current {
                        if std::rc::Rc::ptr_eq(e, &global_env) {
                            break;
                        }
                        let eb = e.borrow();
                        if eb.with_object.is_some() {
                            let next = eb.parent.clone();
                            drop(eb);
                            current = next;
                            continue;
                        }
                        if let Some(binding) = eb.bindings.get(name) {
                            if binding.deletable {
                                drop(eb);
                                e.borrow_mut().bindings.remove(name);
                                return Completion::Normal(JsValue::Boolean(true));
                            }
                            return Completion::Normal(JsValue::Boolean(false));
                        }
                        let next = eb.parent.clone();
                        drop(eb);
                        current = next;
                    }

                    // At global level — check global object property descriptor
                    let global_obj = self.realm().global_env.borrow().global_object.clone();
                    if let Some(ref global) = global_obj {
                        let gb = global.borrow();
                        if let Some(desc) = gb.properties.get(name) {
                            if desc.configurable == Some(false) {
                                return Completion::Normal(JsValue::Boolean(false));
                            }
                            drop(gb);
                            global.borrow_mut().properties.remove(name);
                            global.borrow_mut().property_order.retain(|k| k != name);
                            self.realm().global_env.borrow_mut().bindings.remove(name);
                            return Completion::Normal(JsValue::Boolean(true));
                        }
                    }
                    // Check if it's a binding in the global env (var declaration not on global object)
                    if self.realm().global_env.borrow().bindings.contains_key(name) {
                        return Completion::Normal(JsValue::Boolean(false));
                    }
                    // Unresolvable reference — return true per spec
                    Completion::Normal(JsValue::Boolean(true))
                }
                _ => {
                    // Evaluate the expression for side effects, then return true
                    match self.eval_expr(expr, env) {
                        Completion::Normal(_) => Completion::Normal(JsValue::Boolean(true)),
                        other => other,
                    }
                }
            },
            Expression::Sequence(exprs) | Expression::Comma(exprs) => {
                let mut result = JsValue::Undefined;
                for e in exprs {
                    result = match self.eval_expr(e, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                }
                Completion::Normal(result)
            }
            Expression::Spread(_) => Completion::Normal(JsValue::Undefined), // handled by caller
            Expression::Yield(expr, delegate) => {
                if *delegate {
                    let iterable = if let Some(e) = expr {
                        match self.eval_expr(e, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        }
                    } else {
                        JsValue::Undefined
                    };
                    let is_async_gen = self
                        .generator_context
                        .as_ref()
                        .map(|c| c.is_async)
                        .unwrap_or(false);
                    let iterator = if is_async_gen {
                        match self.get_async_iterator(&iterable) {
                            Ok(it) => it,
                            Err(e) => return Completion::Throw(e),
                        }
                    } else {
                        match self.get_iterator(&iterable) {
                            Ok(it) => it,
                            Err(e) => return Completion::Throw(e),
                        }
                    };
                    if let JsValue::Object(o) = &iterator {
                        self.gc_temp_roots.push(o.id);
                    }
                    let result = loop {
                        let next_result = match self.iterator_next(&iterator) {
                            Ok(v) => v,
                            Err(e) => {
                                self.gc_unroot_value(&iterator);
                                return Completion::Throw(e);
                            }
                        };
                        let next_result = if is_async_gen {
                            match self.await_value(&next_result) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => {
                                    self.gc_unroot_value(&iterator);
                                    return Completion::Throw(e);
                                }
                                other => {
                                    self.gc_unroot_value(&iterator);
                                    return other;
                                }
                            }
                        } else {
                            next_result
                        };
                        let done = match self.iterator_complete(&next_result) {
                            Ok(d) => d,
                            Err(e) => {
                                self.gc_unroot_value(&iterator);
                                return Completion::Throw(e);
                            }
                        };
                        let value = match self.iterator_value(&next_result) {
                            Ok(v) => v,
                            Err(e) => {
                                self.gc_unroot_value(&iterator);
                                return Completion::Throw(e);
                            }
                        };
                        if done {
                            break Completion::Normal(value);
                        }
                        if let Some(ref mut ctx) = self.generator_context {
                            let current = ctx.current_yield;
                            ctx.current_yield += 1;
                            if current < ctx.target_yield {
                                continue;
                            }
                            if current == ctx.target_yield {
                                self.gc_unroot_value(&iterator);
                                return Completion::Yield(value);
                            }
                        }
                        self.gc_unroot_value(&iterator);
                        return Completion::Yield(value);
                    };
                    self.gc_unroot_value(&iterator);
                    result
                } else {
                    let value = if let Some(e) = expr {
                        match self.eval_expr(e, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        }
                    } else {
                        JsValue::Undefined
                    };
                    if let Some(ctx) = self.generator_context.as_mut() {
                        let current = ctx.current_yield;
                        ctx.current_yield += 1;
                        if current < ctx.target_yield {
                            // Fast-forwarding past this yield - return sent_value
                            return Completion::Normal(ctx.sent_value.clone());
                        }
                    }
                    // Yield the value - callers handle this completion type
                    Completion::Yield(value)
                }
            }
            Expression::Await(expr) => {
                let val = match self.eval_expr(expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                self.await_value(&val)
            }
            Expression::ImportMeta => {
                // Create import.meta object - null prototype per spec §9.4.6
                let meta = self.create_object();
                meta.borrow_mut().prototype = None; // import.meta has null prototype
                // Set url property to the current module's file URL
                if let Some(ref path) = self.current_module_path {
                    let url = format!("file://{}", path.display());
                    meta.borrow_mut().insert_property(
                        "url".to_string(),
                        PropertyDescriptor::data(
                            JsValue::String(JsString::from_str(&url)),
                            true,
                            true,
                            true,
                        ),
                    );
                }
                let id = meta.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
            }
            Expression::Import(source_expr, options_expr) => {
                // Dynamic import() - returns a Promise
                // §2.1.1.1 EvaluateImportCall: evaluate specifier and options synchronously
                let source_val = match self.eval_expr(source_expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                // Evaluate options expression if present (abrupt completions propagate directly)
                if let Some(opts_expr) = options_expr {
                    match self.eval_expr(opts_expr, env) {
                        Completion::Normal(opts_val) => {
                            // Steps 9-10: If options is not undefined, validate it
                            if !opts_val.is_undefined() {
                                if !matches!(opts_val, JsValue::Object(_)) {
                                    let err = self.create_type_error(
                                        "The second argument to import() must be an object",
                                    );
                                    return self.create_rejected_promise(err);
                                }
                                // Step 11: Get "with" property (must use [[Get]] to invoke getters)
                                if let JsValue::Object(o) = &opts_val.clone() {
                                    let wv = match self.get_object_property(o.id, "with", &opts_val) {
                                        Completion::Normal(v) => v,
                                        Completion::Throw(e) => return self.create_rejected_promise(e),
                                        other => return other,
                                    };
                                    if !wv.is_undefined() {
                                        if !matches!(wv, JsValue::Object(_)) {
                                            let err = self.create_type_error(
                                                "The 'with' option must be an object",
                                            );
                                            return self.create_rejected_promise(err);
                                        }
                                        // §2.1.1.1 step 10d: enumerate properties, each value must be a string
                                        if let JsValue::Object(ref with_obj) = wv {
                                            let keys = match crate::interpreter::helpers::enumerable_own_keys(self, with_obj.id) {
                                                Ok(k) => k,
                                                Err(e) => return self.create_rejected_promise(e),
                                            };
                                            for k in keys {
                                                let v = match self.get_object_property(with_obj.id, &k, &wv) {
                                                    Completion::Normal(v) => v,
                                                    Completion::Throw(e) => return self.create_rejected_promise(e),
                                                    other => return other,
                                                };
                                                if !matches!(v, JsValue::String(_)) {
                                                    let err = self.create_type_error(
                                                        "Import attribute values must be strings",
                                                    );
                                                    return self.create_rejected_promise(err);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        other => return other,
                    }
                }
                // Per spec: ToString(specifier) errors produce a rejected promise
                let source = match self.to_string_value(&source_val) {
                    Ok(s) => s,
                    Err(e) => return self.create_rejected_promise(e),
                };
                self.dynamic_import(&source)
            }
            Expression::ImportDefer(source_expr, options_expr) => {
                let source_val = match self.eval_expr(source_expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if let Some(opts_expr) = options_expr {
                    match self.eval_expr(opts_expr, env) {
                        Completion::Normal(opts_val) => {
                            if !opts_val.is_undefined() {
                                if !matches!(opts_val, JsValue::Object(_)) {
                                    let err = self.create_type_error(
                                        "The second argument to import.defer() must be an object",
                                    );
                                    return self.create_rejected_promise(err);
                                }
                                if let JsValue::Object(o) = &opts_val {
                                    if let Some(obj) = self.get_object(o.id) {
                                        let wv = obj.borrow().get_property("with");
                                        if !wv.is_undefined() && !matches!(wv, JsValue::Object(_)) {
                                            let err = self.create_type_error(
                                                "The 'with' option must be an object",
                                            );
                                            return self.create_rejected_promise(err);
                                        }
                                    }
                                }
                            }
                        }
                        other => return other,
                    }
                }
                let source = match self.to_string_value(&source_val) {
                    Ok(s) => s,
                    Err(e) => return self.create_rejected_promise(e),
                };
                self.dynamic_import(&source)
            }
            Expression::ImportSource(source_expr, options_expr) => {
                let source_val = match self.eval_expr(source_expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if let Some(opts_expr) = options_expr {
                    match self.eval_expr(opts_expr, env) {
                        Completion::Normal(opts_val) => {
                            if !opts_val.is_undefined() {
                                if !matches!(opts_val, JsValue::Object(_)) {
                                    let err = self.create_type_error(
                                        "The second argument to import.source() must be an object",
                                    );
                                    return self.create_rejected_promise(err);
                                }
                                if let JsValue::Object(o) = &opts_val {
                                    if let Some(obj) = self.get_object(o.id) {
                                        let wv = obj.borrow().get_property("with");
                                        if !wv.is_undefined() && !matches!(wv, JsValue::Object(_)) {
                                            let err = self.create_type_error(
                                                "The 'with' option must be an object",
                                            );
                                            return self.create_rejected_promise(err);
                                        }
                                    }
                                }
                            }
                        }
                        other => return other,
                    }
                }
                let source = match self.to_string_value(&source_val) {
                    Ok(s) => s,
                    Err(e) => return self.create_rejected_promise(e),
                };
                // Per spec §16.2.1.7.2: GetModuleSource of SourceTextModule always throws SyntaxError
                let _ = source;
                let err = self.create_error(
                    "SyntaxError",
                    "Source phase imports are not available for this module",
                );
                self.create_rejected_promise(err)
            }
            Expression::Template(tmpl) => {
                let mut s = String::new();
                for (i, quasi) in tmpl.quasis.iter().enumerate() {
                    s.push_str(quasi.as_deref().unwrap_or(""));
                    if i < tmpl.expressions.len() {
                        let val = match self.eval_expr(&tmpl.expressions[i], env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let str_val = match self.to_string_value(&val) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };
                        s.push_str(&str_val);
                    }
                }
                Completion::Normal(JsValue::String(JsString::from_str(&s)))
            }
            Expression::OptionalChain(base, prop) => {
                // When base is a Member expr and prop starts with Call(Identifier(""),...),
                // we need to preserve the `this` binding from the member access.
                // E.g., obj.method?.() should call method with this=obj.
                let (base_val, base_this) = match base.as_ref() {
                    Expression::Member(obj_expr, member_prop) => {
                        let obj_val = match self.eval_expr(obj_expr, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let key = match member_prop {
                            MemberProperty::Dot(name) => name.clone(),
                            MemberProperty::Computed(expr) => {
                                let v = match self.eval_expr(expr, env) {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                };
                                match self.to_property_key(&v) {
                                    Ok(s) => s,
                                    Err(e) => return Completion::Throw(e),
                                }
                            }
                            MemberProperty::Private(name) => {
                                let branded = self.resolve_private_name(name, env);
                                if let JsValue::Object(ref o) = obj_val
                                    && let Some(obj) = self.get_object(o.id)
                                {
                                    let elem = obj.borrow().private_fields.get(&branded).cloned();
                                    match elem {
                                        Some(PrivateElement::Field(v))
                                        | Some(PrivateElement::Method(v)) => {
                                            return if matches!(
                                                v,
                                                JsValue::Null | JsValue::Undefined
                                            ) {
                                                Completion::Normal(JsValue::Undefined)
                                            } else {
                                                match self.eval_oc_tail_with_this(&v, prop, env) {
                                                    Ok((result, _)) => Completion::Normal(result),
                                                    Err(c) => c,
                                                }
                                            };
                                        }
                                        Some(PrivateElement::Accessor { get, .. }) => {
                                            if let Some(getter) = get {
                                                let v = match self.call_function(
                                                    &getter,
                                                    &obj_val,
                                                    &[],
                                                ) {
                                                    Completion::Normal(v) => v,
                                                    other => return other,
                                                };
                                                return if matches!(
                                                    v,
                                                    JsValue::Null | JsValue::Undefined
                                                ) {
                                                    Completion::Normal(JsValue::Undefined)
                                                } else {
                                                    match self.eval_oc_tail_with_this(&v, prop, env)
                                                    {
                                                        Ok((result, _)) => {
                                                            Completion::Normal(result)
                                                        }
                                                        Err(c) => c,
                                                    }
                                                };
                                            }
                                            return Completion::Normal(JsValue::Undefined);
                                        }
                                        None => {
                                            return Completion::Throw(self.create_type_error(
                                                &format!("Cannot read private member #{name}"),
                                            ));
                                        }
                                    }
                                } else {
                                    return Completion::Normal(JsValue::Undefined);
                                }
                            }
                        };
                        let prop_val = match self.access_property_on_value(&obj_val, &key) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        (prop_val, obj_val)
                    }
                    _ => {
                        let val = match self.eval_expr(base, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        (val, JsValue::Undefined)
                    }
                };
                if matches!(base_val, JsValue::Null | JsValue::Undefined) {
                    return Completion::Normal(JsValue::Undefined);
                }
                self.eval_optional_chain_tail_with_base_this(&base_val, &base_this, prop, env)
            }
            Expression::TaggedTemplate(tag_expr, tmpl) => {
                let (func_val, this_val) = match tag_expr.as_ref() {
                    Expression::Member(obj_expr, prop) => {
                        let obj_val = match self.eval_expr(obj_expr, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let key = match prop {
                            MemberProperty::Dot(name) => name.clone(),
                            MemberProperty::Computed(expr) => {
                                let v = match self.eval_expr(expr, env) {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                };
                                match self.to_property_key(&v) {
                                    Ok(s) => s,
                                    Err(e) => return Completion::Throw(e),
                                }
                            }
                            MemberProperty::Private(_) => {
                                return Completion::Throw(
                                    self.create_type_error("Private member in tagged template"),
                                );
                            }
                        };
                        let func = match &obj_val {
                            JsValue::Object(o) => self.get_object_property(o.id, &key, &obj_val),
                            _ => Completion::Normal(JsValue::Undefined),
                        };
                        let func = match func {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        (func, obj_val)
                    }
                    _ => {
                        let func = match self.eval_expr(tag_expr, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        (func, JsValue::Undefined)
                    }
                };

                let template_obj = self.get_template_object(tmpl);

                let mut call_args = vec![template_obj];
                for sub_expr in &tmpl.expressions {
                    match self.eval_expr(sub_expr, env) {
                        Completion::Normal(v) => call_args.push(v),
                        other => return other,
                    }
                }

                self.call_function(&func_val, &this_val, &call_args)
            }
        }
    }

    fn access_property_on_value(&mut self, base_val: &JsValue, name: &str) -> Completion {
        match base_val {
            JsValue::Object(o) => self.get_object_property(o.id, name, base_val),
            JsValue::String(s) => {
                if name == "length" {
                    Completion::Normal(JsValue::Number(s.len() as f64))
                } else if let Ok(idx) = name.parse::<usize>() {
                    if idx < s.code_units.len() {
                        Completion::Normal(JsValue::String(JsString {
                            code_units: vec![s.code_units[idx]],
                        }))
                    } else {
                        Completion::Normal(JsValue::Undefined)
                    }
                } else if let Some(ref sp) = self.realm().string_prototype {
                    Completion::Normal(sp.borrow().get_property(name))
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            JsValue::Number(_) => {
                if let Some(ref np) = self.realm().number_prototype {
                    Completion::Normal(np.borrow().get_property(name))
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            JsValue::Boolean(_) => {
                if let Some(ref bp) = self.realm().boolean_prototype {
                    Completion::Normal(bp.borrow().get_property(name))
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            _ => Completion::Normal(JsValue::Undefined),
        }
    }

    #[allow(dead_code)]
    fn eval_optional_chain_tail(
        &mut self,
        base_val: &JsValue,
        prop: &Expression,
        env: &EnvRef,
    ) -> Completion {
        match self.eval_oc_tail_with_this(base_val, prop, env) {
            Ok((v, _)) => Completion::Normal(v),
            Err(c) => c,
        }
    }

    /// Evaluate optional chain tail with a known `this` from the base member access.
    /// This is used when the optional chain base is `obj.method?.()` so that
    /// the call uses `obj` as `this`.
    fn eval_optional_chain_tail_with_base_this(
        &mut self,
        base_val: &JsValue,
        base_this: &JsValue,
        prop: &Expression,
        env: &EnvRef,
    ) -> Completion {
        match self.eval_oc_tail_with_this_ctx(base_val, base_this, prop, env) {
            Ok((v, _)) => Completion::Normal(v),
            Err(c) => c,
        }
    }

    /// Evaluate optional chain tail, returning (value, this_for_call).
    fn eval_oc_tail_with_this(
        &mut self,
        base_val: &JsValue,
        prop: &Expression,
        env: &EnvRef,
    ) -> Result<(JsValue, JsValue), Completion> {
        self.eval_oc_tail_with_this_ctx(base_val, &JsValue::Undefined, prop, env)
    }

    /// Core optional chain tail evaluator with explicit this context.
    /// `chain_this` is the `this` value to use for `?.()` direct calls
    /// (from `obj.method?.()` where chain_this = obj).
    fn eval_oc_tail_with_this_ctx(
        &mut self,
        base_val: &JsValue,
        chain_this: &JsValue,
        prop: &Expression,
        env: &EnvRef,
    ) -> Result<(JsValue, JsValue), Completion> {
        match prop {
            Expression::Identifier(name) => {
                if name.is_empty() {
                    // x?.() — direct call placeholder, base_val IS the value
                    // chain_this is the object for `obj.method?.()` calls
                    Ok((base_val.clone(), chain_this.clone()))
                } else {
                    let val = match self.access_property_on_value(base_val, name) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Err(Completion::Throw(e)),
                        other => return Err(other),
                    };
                    Ok((val, base_val.clone()))
                }
            }
            Expression::Call(callee, args) => {
                let (func_val, this_val) =
                    self.eval_oc_tail_with_this_ctx(base_val, chain_this, callee, env)?;
                let evaluated_args = match self.eval_spread_args(args, env) {
                    Ok(v) => v,
                    Err(e) => return Err(Completion::Throw(e)),
                };
                match self.call_function(&func_val, &this_val, &evaluated_args) {
                    Completion::Normal(v) => Ok((v, JsValue::Undefined)),
                    other => Err(other),
                }
            }
            Expression::Member(inner, mp) => {
                let (inner_val, _) =
                    self.eval_oc_tail_with_this_ctx(base_val, chain_this, inner, env)?;
                match mp {
                    MemberProperty::Dot(name) => {
                        let val = match self.access_property_on_value(&inner_val, name) {
                            Completion::Normal(v) => v,
                            other => return Err(other),
                        };
                        Ok((val, inner_val))
                    }
                    MemberProperty::Computed(expr) => {
                        let key_val = match self.eval_expr(expr, env) {
                            Completion::Normal(v) => v,
                            other => return Err(other),
                        };
                        let key = match self.to_property_key(&key_val) {
                            Ok(s) => s,
                            Err(e) => return Err(Completion::Throw(e)),
                        };
                        let val = match self.access_property_on_value(&inner_val, &key) {
                            Completion::Normal(v) => v,
                            other => return Err(other),
                        };
                        Ok((val, inner_val))
                    }
                    MemberProperty::Private(name) => {
                        let branded = self.resolve_private_name(name, env);
                        if let JsValue::Object(o) = &inner_val
                            && let Some(obj) = self.get_object(o.id)
                        {
                            let elem = obj.borrow().private_fields.get(&branded).cloned();
                            match elem {
                                Some(PrivateElement::Field(v))
                                | Some(PrivateElement::Method(v)) => {
                                    Ok((v, inner_val))
                                }
                                Some(PrivateElement::Accessor { get, .. }) => {
                                    if let Some(getter) = get {
                                        match self.call_function(&getter, &inner_val, &[]) {
                                            Completion::Normal(v) => Ok((v, inner_val)),
                                            other => Err(other),
                                        }
                                    } else {
                                        Err(Completion::Throw(self.create_type_error(&format!(
                                            "Cannot read private member #{name} which has no getter"
                                        ))))
                                    }
                                }
                                None => Err(Completion::Throw(self.create_type_error(&format!(
                                    "Cannot read private member #{name} from an object whose class did not declare it"
                                )))),
                            }
                        } else {
                            Ok((JsValue::Undefined, inner_val))
                        }
                    }
                }
            }
            other => {
                // Computed property access (e.g., x?.[expr])
                let key_val = match self.eval_expr(other, env) {
                    Completion::Normal(v) => v,
                    other => return Err(other),
                };
                let key = match self.to_property_key(&key_val) {
                    Ok(s) => s,
                    Err(e) => return Err(Completion::Throw(e)),
                };
                let val = match self.access_property_on_value(base_val, &key) {
                    Completion::Normal(v) => v,
                    other => return Err(other),
                };
                Ok((val, base_val.clone()))
            }
        }
    }

    fn get_template_object(&mut self, tmpl: &TemplateLiteral) -> JsValue {
        let cache_key = tmpl as *const TemplateLiteral as usize;
        if let Some(&obj_id) = self.realm().template_cache.get(&cache_key)
            && self.get_object(obj_id).is_some()
        {
            return JsValue::Object(crate::types::JsObject { id: obj_id });
        }

        let cooked_vals: Vec<JsValue> = tmpl
            .quasis
            .iter()
            .map(|q| match q {
                Some(s) => JsValue::String(JsString::from_str(s)),
                None => JsValue::Undefined,
            })
            .collect();
        let raw_vals: Vec<JsValue> = tmpl
            .raw_quasis
            .iter()
            .map(|s| JsValue::String(JsString::from_str(s)))
            .collect();

        let raw_arr = self.create_frozen_template_array(raw_vals);
        let template_arr = self.create_frozen_template_array(cooked_vals);

        if let JsValue::Object(o) = &template_arr
            && let Some(obj) = self.get_object(o.id)
        {
            obj.borrow_mut().insert_property(
                "raw".to_string(),
                PropertyDescriptor::data(raw_arr, false, false, false),
            );
        }

        if let JsValue::Object(o) = &template_arr {
            self.realm_mut().template_cache.insert(cache_key, o.id);
        }

        template_arr
    }

    fn create_frozen_template_array(&mut self, values: Vec<JsValue>) -> JsValue {
        let len = values.len();
        let mut obj_data = JsObjectData::new();
        obj_data.prototype = self.realm()
            .array_prototype
            .clone()
            .or(self.realm().object_prototype.clone());
        obj_data.class_name = "Array".to_string();
        for (i, v) in values.iter().enumerate() {
            obj_data.insert_property(
                i.to_string(),
                PropertyDescriptor::data(v.clone(), false, true, false),
            );
        }
        obj_data.insert_property(
            "length".to_string(),
            PropertyDescriptor::data(JsValue::Number(len as f64), false, false, false),
        );
        obj_data.array_elements = Some(values);
        obj_data.extensible = false;
        let obj = Rc::new(RefCell::new(obj_data));
        let id = self.allocate_object_slot(obj);
        JsValue::Object(crate::types::JsObject { id })
    }

    fn eval_literal(&mut self, lit: &Literal) -> JsValue {
        match lit {
            Literal::Null => JsValue::Null,
            Literal::Boolean(b) => JsValue::Boolean(*b),
            Literal::Number(n) => JsValue::Number(*n),
            Literal::String(s) => {
                let code_units =
                    crate::interpreter::builtins::regexp::pua_code_units_to_surrogates(s);
                JsValue::String(JsString { code_units })
            }
            Literal::BigInt(s) => {
                use num_bigint::BigInt;
                let value = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))
                {
                    BigInt::parse_bytes(hex.as_bytes(), 16).unwrap_or_default()
                } else if let Some(oct) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
                    BigInt::parse_bytes(oct.as_bytes(), 8).unwrap_or_default()
                } else if let Some(bin) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
                    BigInt::parse_bytes(bin.as_bytes(), 2).unwrap_or_default()
                } else {
                    s.parse::<BigInt>().unwrap_or_default()
                };
                JsValue::BigInt(JsBigInt { value })
            }
            Literal::RegExp(pattern, flags) => {
                let mut obj = JsObjectData::new();
                obj.prototype = self.realm()
                    .regexp_prototype
                    .clone()
                    .or(self.realm().object_prototype.clone());
                obj.class_name = "RegExp".to_string();
                let source_js = if pattern.is_empty() {
                    JsString::from_str("(?:)")
                } else {
                    crate::interpreter::builtins::regexp::regex_output_to_js_string(pattern)
                };
                obj.insert_property(
                    "__original_source__".to_string(),
                    PropertyDescriptor::data(JsValue::String(source_js), false, false, false),
                );
                obj.insert_property(
                    "__original_flags__".to_string(),
                    PropertyDescriptor::data(
                        JsValue::String(JsString::from_str(flags)),
                        false,
                        false,
                        false,
                    ),
                );
                obj.insert_property(
                    "lastIndex".to_string(),
                    PropertyDescriptor::data(JsValue::Number(0.0), true, false, false),
                );
                let rc = Rc::new(RefCell::new(obj));
                let id = self.allocate_object_slot(rc);
                JsValue::Object(crate::types::JsObject { id })
            }
        }
    }

    // §7.1.14 ToPropertyKey
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_property_key(&mut self, val: &JsValue) -> Result<String, JsValue> {
        match val {
            JsValue::Symbol(s) => Ok(s.to_property_key()),
            JsValue::Object(_) => {
                let prim = self.to_primitive(val, "string")?;
                if let JsValue::Symbol(s) = &prim {
                    return Ok(s.to_property_key());
                }
                self.to_string_value(&prim)
            }
            _ => self.to_string_value(val),
        }
    }

    pub(crate) fn create_regexp(&mut self, pattern: &str, flags: &str) -> JsValue {
        let mut obj = JsObjectData::new();
        obj.prototype = self.realm()
            .regexp_prototype
            .clone()
            .or(self.realm().object_prototype.clone());
        obj.class_name = "RegExp".to_string();
        let source_str = if pattern.is_empty() { "(?:)" } else { pattern };
        obj.insert_property(
            "__original_source__".to_string(),
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str(source_str)),
                false,
                false,
                false,
            ),
        );
        obj.insert_property(
            "__original_flags__".to_string(),
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str(flags)),
                false,
                false,
                false,
            ),
        );
        obj.insert_property(
            "lastIndex".to_string(),
            PropertyDescriptor::data(JsValue::Number(0.0), true, false, false),
        );
        let rc = Rc::new(RefCell::new(obj));
        let id = self.allocate_object_slot(rc);
        JsValue::Object(crate::types::JsObject { id })
    }

    fn eval_unary(&mut self, op: UnaryOp, val: &JsValue) -> Completion {
        match op {
            UnaryOp::Minus => match val {
                JsValue::BigInt(b) => Completion::Normal(JsValue::BigInt(JsBigInt {
                    value: bigint_ops::unary_minus(&b.value),
                })),
                _ => match self.to_number_value(val) {
                    Ok(n) => Completion::Normal(JsValue::Number(number_ops::unary_minus(n))),
                    Err(e) => Completion::Throw(e),
                },
            },
            UnaryOp::Plus => match self.to_number_value(val) {
                Ok(n) => Completion::Normal(JsValue::Number(n)),
                Err(e) => Completion::Throw(e),
            },
            UnaryOp::Not => Completion::Normal(JsValue::Boolean(!self.to_boolean_val(val))),
            UnaryOp::BitNot => match val {
                JsValue::BigInt(b) => Completion::Normal(JsValue::BigInt(JsBigInt {
                    value: bigint_ops::bitwise_not(&b.value),
                })),
                JsValue::Object(_) => Completion::Normal(JsValue::Number(number_ops::bitwise_not(
                    self.to_number_coerce(val),
                ))),
                _ => Completion::Normal(JsValue::Number(number_ops::bitwise_not(to_number(val)))),
            },
        }
    }

    fn require_object_coercible(&mut self, val: &JsValue) -> Completion {
        match val {
            JsValue::Undefined | JsValue::Null => {
                let err = self.create_type_error("Cannot convert undefined or null to object");
                Completion::Throw(err)
            }
            _ => Completion::Normal(val.clone()),
        }
    }

    #[allow(dead_code)]
    fn is_regexp(&self, val: &JsValue) -> bool {
        if let JsValue::Object(o) = val
            && let Some(obj) = self.get_object(o.id)
        {
            return obj.borrow().class_name == "RegExp";
        }
        false
    }

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_index(&mut self, val: &JsValue) -> Completion {
        if val.is_undefined() {
            return Completion::Normal(JsValue::Number(0.0));
        }
        // §7.1.22 ToIndex: Let integerIndex be ! ToIntegerOrInfinity(value).
        // ToIntegerOrInfinity calls ToNumber (which invokes ToPrimitive for objects)
        let integer_index = match self.to_number_value(val) {
            Ok(n) => n,
            Err(e) => return Completion::Throw(e),
        };
        let integer_index = if integer_index.is_nan() {
            0.0
        } else {
            integer_index.trunc()
        };
        if !(0.0..=9007199254740991.0).contains(&integer_index) {
            let err = self.create_error("RangeError", "Invalid index");
            return Completion::Throw(err);
        }
        Completion::Normal(JsValue::Number(integer_index))
    }

    pub(crate) fn to_length(val: &JsValue) -> f64 {
        let len = to_number(val);
        if len.is_nan() || len <= 0.0 {
            return 0.0;
        }
        len.min(9007199254740991.0).floor() // 2^53 - 1
    }

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_object(&mut self, val: &JsValue) -> Completion {
        match val {
            JsValue::Undefined | JsValue::Null => {
                let err = self.create_type_error("Cannot convert undefined or null to object");
                Completion::Throw(err)
            }
            JsValue::Boolean(_)
            | JsValue::Number(_)
            | JsValue::String(_)
            | JsValue::Symbol(_)
            | JsValue::BigInt(_) => {
                let mut obj_data = JsObjectData::new();
                obj_data.primitive_value = Some(val.clone());
                match val {
                    JsValue::String(_) => {
                        obj_data.class_name = "String".to_string();
                        if let Some(ref sp) = self.realm().string_prototype {
                            obj_data.prototype = Some(sp.clone());
                        }
                    }
                    JsValue::Number(_) => {
                        obj_data.class_name = "Number".to_string();
                        if let Some(ref np) = self.realm().number_prototype {
                            obj_data.prototype = Some(np.clone());
                        }
                    }
                    JsValue::Boolean(_) => {
                        obj_data.class_name = "Boolean".to_string();
                        if let Some(ref bp) = self.realm().boolean_prototype {
                            obj_data.prototype = Some(bp.clone());
                        }
                    }
                    JsValue::Symbol(_) => {
                        obj_data.class_name = "Symbol".to_string();
                        if let Some(ref sp) = self.realm().symbol_prototype {
                            obj_data.prototype = Some(sp.clone());
                        }
                    }
                    JsValue::BigInt(_) => {
                        obj_data.class_name = "BigInt".to_string();
                        if let Some(ref bp) = self.realm().bigint_prototype {
                            obj_data.prototype = Some(bp.clone());
                        }
                    }
                    _ => unreachable!(),
                }
                if obj_data.prototype.is_none() {
                    obj_data.prototype = self.realm().object_prototype.clone();
                }
                let obj = Rc::new(RefCell::new(obj_data));
                let id = self.allocate_object_slot(obj);
                Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
            }
            JsValue::Object(_) => Completion::Normal(val.clone()),
        }
    }

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_primitive(
        &mut self,
        val: &JsValue,
        preferred_type: &str,
    ) -> Result<JsValue, JsValue> {
        match val {
            JsValue::Object(o) => {
                // §7.1.1 Step 2-3: Check @@toPrimitive
                let exotic_to_prim = {
                    let key = "Symbol(Symbol.toPrimitive)";
                    match self.get_object_property(o.id, key, val) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Err(e),
                        _ => JsValue::Undefined,
                    }
                };
                if !matches!(exotic_to_prim, JsValue::Undefined | JsValue::Null) {
                    if let JsValue::Object(fo) = &exotic_to_prim
                        && self
                            .get_object(fo.id)
                            .map(|o| o.borrow().callable.is_some())
                            .unwrap_or(false)
                    {
                        let hint = JsValue::String(JsString::from_str(preferred_type));
                        let result = self.call_function(&exotic_to_prim, val, &[hint]);
                        match result {
                            Completion::Normal(v) if !matches!(v, JsValue::Object(_)) => {
                                return Ok(v);
                            }
                            Completion::Normal(_) => {
                                return Err(
                                    self.create_type_error("@@toPrimitive must return a primitive")
                                );
                            }
                            Completion::Throw(e) => return Err(e),
                            _ => {}
                        }
                    } else {
                        return Err(self.create_type_error("@@toPrimitive is not callable"));
                    }
                }

                // §7.1.1.1 OrdinaryToPrimitive
                let methods = if preferred_type == "string" {
                    ["toString", "valueOf"]
                } else {
                    ["valueOf", "toString"]
                };
                for method_name in &methods {
                    let method_val = match self.get_object_property(o.id, method_name, val) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Err(e),
                        _ => JsValue::Undefined,
                    };
                    if let JsValue::Object(fo) = &method_val
                        && self
                            .get_object(fo.id)
                            .map(|o| o.borrow().callable.is_some())
                            .unwrap_or(false)
                    {
                        let result = self.call_function(&method_val, val, &[]);
                        match result {
                            Completion::Normal(v) if !matches!(v, JsValue::Object(_)) => {
                                return Ok(v);
                            }
                            Completion::Throw(e) => return Err(e),
                            _ => {}
                        }
                    }
                }
                // Fallback: check for primitive_value (wrapper objects)
                if let Some(obj) = self.get_object(o.id)
                    && let Some(pv) = obj.borrow().primitive_value.clone()
                {
                    return Ok(pv);
                }
                Err(self.create_type_error("Cannot convert object to primitive value"))
            }
            _ => Ok(val.clone()),
        }
    }

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_number_coerce(&mut self, val: &JsValue) -> f64 {
        match self.to_primitive(val, "number") {
            Ok(prim) => to_number(&prim),
            Err(_) => f64::NAN,
        }
    }

    // §7.1.17 ToString — calls ToPrimitive for objects
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_string_value(&mut self, val: &JsValue) -> Result<String, JsValue> {
        match val {
            JsValue::Undefined => Ok("undefined".to_string()),
            JsValue::Null => Ok("null".to_string()),
            JsValue::Boolean(b) => Ok(if *b { "true" } else { "false" }.to_string()),
            JsValue::Number(n) => Ok(number_ops::to_string(*n)),
            JsValue::String(s) => Ok(s.to_rust_string()),
            JsValue::Symbol(_) => {
                Err(self.create_type_error("Cannot convert a Symbol value to a string"))
            }
            JsValue::BigInt(n) => Ok(n.value.to_string()),
            JsValue::Object(_) => {
                let prim = self.to_primitive(val, "string")?;
                self.to_string_value(&prim)
            }
        }
    }

    pub(crate) fn to_js_string(&mut self, val: &JsValue) -> Result<JsString, JsValue> {
        match val {
            JsValue::String(s) => Ok(s.clone()),
            other => {
                let s = self.to_string_value(other)?;
                Ok(JsString::from_str(&s))
            }
        }
    }

    // §7.1.4 ToNumber — calls ToPrimitive for objects
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_number_value(&mut self, val: &JsValue) -> Result<f64, JsValue> {
        match val {
            JsValue::Object(_) => {
                let prim = self.to_primitive(val, "number")?;
                self.to_number_value(&prim)
            }
            JsValue::Symbol(_) => {
                Err(self.create_type_error("Cannot convert a Symbol value to a number"))
            }
            JsValue::BigInt(_) => {
                Err(self.create_type_error("Cannot convert a BigInt value to a number"))
            }
            _ => Ok(to_number(val)),
        }
    }

    // §7.1.13 ToBigInt
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_bigint_value(&mut self, val: &JsValue) -> Result<JsValue, JsValue> {
        let prim = match val {
            JsValue::Object(_) => self.to_primitive(val, "number")?,
            _ => val.clone(),
        };
        match &prim {
            JsValue::BigInt(_) => Ok(prim),
            JsValue::Boolean(b) => Ok(JsValue::BigInt(crate::types::JsBigInt {
                value: if *b {
                    num_bigint::BigInt::from(1)
                } else {
                    num_bigint::BigInt::from(0)
                },
            })),
            JsValue::String(s) => {
                let text = s.to_rust_string();
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    return Ok(JsValue::BigInt(crate::types::JsBigInt {
                        value: num_bigint::BigInt::from(0),
                    }));
                }
                let parsed = if let Some(hex) =
                    trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X"))
                {
                    num_bigint::BigInt::parse_bytes(hex.as_bytes(), 16)
                } else if let Some(oct) =
                    trimmed.strip_prefix("0o").or_else(|| trimmed.strip_prefix("0O"))
                {
                    num_bigint::BigInt::parse_bytes(oct.as_bytes(), 8)
                } else if let Some(bin) =
                    trimmed.strip_prefix("0b").or_else(|| trimmed.strip_prefix("0B"))
                {
                    num_bigint::BigInt::parse_bytes(bin.as_bytes(), 2)
                } else {
                    trimmed.parse::<num_bigint::BigInt>().ok()
                };
                match parsed {
                    Some(n) => Ok(JsValue::BigInt(crate::types::JsBigInt { value: n })),
                    None => Err(self.create_error(
                        "SyntaxError",
                        &format!("Cannot convert {} to a BigInt", text),
                    )),
                }
            }
            JsValue::Undefined => {
                Err(self.create_type_error("Cannot convert undefined to a BigInt"))
            }
            JsValue::Null => Err(self.create_type_error("Cannot convert null to a BigInt")),
            JsValue::Number(_) => {
                Err(self.create_type_error("Cannot convert a Number to a BigInt"))
            }
            JsValue::Symbol(_) => {
                Err(self.create_type_error("Cannot convert a Symbol to a BigInt"))
            }
            _ => Err(self.create_type_error("Cannot convert to BigInt")),
        }
    }

    fn abstract_equality(&mut self, left: &JsValue, right: &JsValue) -> Result<bool, JsValue> {
        if std::mem::discriminant(left) == std::mem::discriminant(right) {
            return Ok(strict_equality(left, right));
        }
        // B.3.6.2: IsHTMLDDA == null/undefined
        if let JsValue::Object(o) = left {
            if let Some(Some(obj)) = self.objects.get(o.id as usize) {
                if obj.borrow().is_htmldda
                    && (right.is_null() || right.is_undefined())
                {
                    return Ok(true);
                }
            }
        }
        if let JsValue::Object(o) = right {
            if let Some(Some(obj)) = self.objects.get(o.id as usize) {
                if obj.borrow().is_htmldda
                    && (left.is_null() || left.is_undefined())
                {
                    return Ok(true);
                }
            }
        }
        if (left.is_null() && right.is_undefined()) || (left.is_undefined() && right.is_null()) {
            return Ok(true);
        }
        if left.is_number() && right.is_string() {
            return self.abstract_equality(left, &JsValue::Number(to_number(right)));
        }
        if left.is_string() && right.is_number() {
            return self.abstract_equality(&JsValue::Number(to_number(left)), right);
        }
        if left.is_boolean() {
            return self.abstract_equality(&JsValue::Number(to_number(left)), right);
        }
        if right.is_boolean() {
            return self.abstract_equality(left, &JsValue::Number(to_number(right)));
        }
        // BigInt == Number
        if let (JsValue::BigInt(b), JsValue::Number(n)) | (JsValue::Number(n), JsValue::BigInt(b)) =
            (left, right)
        {
            if n.is_nan() || n.is_infinite() {
                return Ok(false);
            }
            if *n != n.trunc() {
                return Ok(false);
            }
            let n_as_bigint = crate::interpreter::builtins::bigint::f64_to_bigint(*n);
            return Ok(bigint_ops::equal(&b.value, &n_as_bigint));
        }
        // BigInt == String
        if let (JsValue::BigInt(b), JsValue::String(s)) = (left, right) {
            if let Ok(parsed) = s.to_rust_string().parse::<num_bigint::BigInt>() {
                return Ok(bigint_ops::equal(&b.value, &parsed));
            }
            return Ok(false);
        }
        if let (JsValue::String(s), JsValue::BigInt(b)) = (left, right) {
            if let Ok(parsed) = s.to_rust_string().parse::<num_bigint::BigInt>() {
                return Ok(bigint_ops::equal(&parsed, &b.value));
            }
            return Ok(false);
        }
        // Object vs primitive (including BigInt)
        if matches!(left, JsValue::Object(_))
            && (right.is_string() || right.is_number() || right.is_symbol() || right.is_bigint())
        {
            let lprim = self.to_primitive(left, "default")?;
            return self.abstract_equality(&lprim, right);
        }
        if matches!(right, JsValue::Object(_))
            && (left.is_string() || left.is_number() || left.is_symbol() || left.is_bigint())
        {
            let rprim = self.to_primitive(right, "default")?;
            return self.abstract_equality(left, &rprim);
        }
        Ok(false)
    }

    fn abstract_relational(
        &mut self,
        left: &JsValue,
        right: &JsValue,
    ) -> Result<Option<bool>, JsValue> {
        let lprim = self.to_primitive(left, "number")?;
        let rprim = self.to_primitive(right, "number")?;
        if is_string(&lprim) && is_string(&rprim) {
            let ls = to_js_string(&lprim);
            let rs = to_js_string(&rprim);
            return Ok(Some(ls < rs));
        }
        // BigInt comparisons
        if let (JsValue::BigInt(a), JsValue::BigInt(b)) = (&lprim, &rprim) {
            return Ok(bigint_ops::less_than(&a.value, &b.value));
        }
        if let (JsValue::BigInt(b), JsValue::Number(n)) = (&lprim, &rprim) {
            if n.is_nan() {
                return Ok(None);
            }
            if *n == f64::INFINITY {
                return Ok(Some(true));
            }
            if *n == f64::NEG_INFINITY {
                return Ok(Some(false));
            }
            let n_trunc = n.trunc();
            let n_floor = crate::interpreter::builtins::bigint::f64_to_bigint(n_trunc);
            if b.value < n_floor {
                return Ok(Some(true));
            }
            if b.value > n_floor {
                return Ok(Some(false));
            }
            // n_floor == b.value, so result depends on fractional part
            return Ok(Some(n_trunc < *n));
        }
        if let (JsValue::Number(n), JsValue::BigInt(b)) = (&lprim, &rprim) {
            if n.is_nan() {
                return Ok(None);
            }
            if *n == f64::NEG_INFINITY {
                return Ok(Some(true));
            }
            if *n == f64::INFINITY {
                return Ok(Some(false));
            }
            let n_trunc = n.trunc();
            let n_floor = crate::interpreter::builtins::bigint::f64_to_bigint(n_trunc);
            if n_floor < b.value {
                return Ok(Some(true));
            }
            if n_floor > b.value {
                return Ok(Some(false));
            }
            // n_floor == b.value, so result depends on fractional part
            return Ok(Some(*n < n_trunc));
        }
        // BigInt vs String: try parsing
        if let (JsValue::BigInt(_), JsValue::String(s)) = (&lprim, &rprim) {
            if let Ok(parsed) = s.to_rust_string().parse::<num_bigint::BigInt>() {
                return self
                    .abstract_relational(&lprim, &JsValue::BigInt(JsBigInt { value: parsed }));
            }
            return Ok(None);
        }
        if let (JsValue::String(s), JsValue::BigInt(_)) = (&lprim, &rprim) {
            if let Ok(parsed) = s.to_rust_string().parse::<num_bigint::BigInt>() {
                return self
                    .abstract_relational(&JsValue::BigInt(JsBigInt { value: parsed }), &rprim);
            }
            return Ok(None);
        }
        // ToNumeric throws TypeError for Symbol
        if matches!(lprim, JsValue::Symbol(_)) || matches!(rprim, JsValue::Symbol(_)) {
            return Err(self.create_type_error("Cannot convert a Symbol value to a number"));
        }
        let ln = to_number(&lprim);
        let rn = to_number(&rprim);
        Ok(number_ops::less_than(ln, rn))
    }

    fn eval_binary(&mut self, op: BinaryOp, left: &JsValue, right: &JsValue) -> Completion {
        match op {
            BinaryOp::Add => {
                let lprim = match self.to_primitive(left, "default") {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let rprim = match self.to_primitive(right, "default") {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                if matches!(lprim, JsValue::Symbol(_)) || matches!(rprim, JsValue::Symbol(_)) {
                    return Completion::Throw(
                        self.create_type_error("Cannot convert a Symbol value to a string"),
                    );
                }
                if is_string(&lprim) || is_string(&rprim) {
                    let mut code_units = js_value_to_code_units(&lprim);
                    code_units.extend(js_value_to_code_units(&rprim));
                    Completion::Normal(JsValue::String(JsString { code_units }))
                } else if let (JsValue::BigInt(a), JsValue::BigInt(b)) = (&lprim, &rprim) {
                    Completion::Normal(JsValue::BigInt(JsBigInt {
                        value: bigint_ops::add(&a.value, &b.value),
                    }))
                } else if lprim.is_bigint() || rprim.is_bigint() {
                    Completion::Throw(self.create_type_error(
                        "Cannot mix BigInt and other types, use explicit conversions",
                    ))
                } else {
                    Completion::Normal(JsValue::Number(number_ops::add(
                        to_number(&lprim),
                        to_number(&rprim),
                    )))
                }
            }
            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod | BinaryOp::Exp => {
                let lprim = match self.to_primitive(left, "number") {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let rprim = match self.to_primitive(right, "number") {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                if let (JsValue::BigInt(a), JsValue::BigInt(b)) = (&lprim, &rprim) {
                    match op {
                        BinaryOp::Sub => Completion::Normal(JsValue::BigInt(JsBigInt {
                            value: bigint_ops::subtract(&a.value, &b.value),
                        })),
                        BinaryOp::Mul => Completion::Normal(JsValue::BigInt(JsBigInt {
                            value: bigint_ops::multiply(&a.value, &b.value),
                        })),
                        BinaryOp::Div => match bigint_ops::divide(&a.value, &b.value) {
                            Ok(v) => Completion::Normal(JsValue::BigInt(JsBigInt { value: v })),
                            Err(_) => Completion::Throw(
                                self.create_error("RangeError", "Division by zero"),
                            ),
                        },
                        BinaryOp::Mod => match bigint_ops::remainder(&a.value, &b.value) {
                            Ok(v) => Completion::Normal(JsValue::BigInt(JsBigInt { value: v })),
                            Err(_) => Completion::Throw(
                                self.create_error("RangeError", "Division by zero"),
                            ),
                        },
                        BinaryOp::Exp => match bigint_ops::exponentiate(&a.value, &b.value) {
                            Ok(v) => Completion::Normal(JsValue::BigInt(JsBigInt { value: v })),
                            Err(_) => Completion::Throw(
                                self.create_error("RangeError", "Exponent must be positive"),
                            ),
                        },
                        _ => unreachable!(),
                    }
                } else if lprim.is_bigint() || rprim.is_bigint() {
                    Completion::Throw(self.create_type_error(
                        "Cannot mix BigInt and other types, use explicit conversions",
                    ))
                } else {
                    let ln = to_number(&lprim);
                    let rn = to_number(&rprim);
                    Completion::Normal(JsValue::Number(match op {
                        BinaryOp::Sub => number_ops::subtract(ln, rn),
                        BinaryOp::Mul => number_ops::multiply(ln, rn),
                        BinaryOp::Div => number_ops::divide(ln, rn),
                        BinaryOp::Mod => number_ops::remainder(ln, rn),
                        BinaryOp::Exp => number_ops::exponentiate(ln, rn),
                        _ => unreachable!(),
                    }))
                }
            }
            BinaryOp::Eq => match self.abstract_equality(left, right) {
                Ok(b) => Completion::Normal(JsValue::Boolean(b)),
                Err(e) => Completion::Throw(e),
            },
            BinaryOp::NotEq => match self.abstract_equality(left, right) {
                Ok(b) => Completion::Normal(JsValue::Boolean(!b)),
                Err(e) => Completion::Throw(e),
            },
            BinaryOp::StrictEq => {
                Completion::Normal(JsValue::Boolean(strict_equality(left, right)))
            }
            BinaryOp::StrictNotEq => {
                Completion::Normal(JsValue::Boolean(!strict_equality(left, right)))
            }
            BinaryOp::Lt => match self.abstract_relational(left, right) {
                Ok(r) => Completion::Normal(JsValue::Boolean(r == Some(true))),
                Err(e) => Completion::Throw(e),
            },
            BinaryOp::Gt => match self.abstract_relational(right, left) {
                Ok(r) => Completion::Normal(JsValue::Boolean(r == Some(true))),
                Err(e) => Completion::Throw(e),
            },
            BinaryOp::LtEq => match self.abstract_relational(right, left) {
                Ok(r) => Completion::Normal(JsValue::Boolean(r == Some(false))),
                Err(e) => Completion::Throw(e),
            },
            BinaryOp::GtEq => match self.abstract_relational(left, right) {
                Ok(r) => Completion::Normal(JsValue::Boolean(r == Some(false))),
                Err(e) => Completion::Throw(e),
            },
            BinaryOp::LShift | BinaryOp::RShift | BinaryOp::URShift => {
                let lprim = match self.to_primitive(left, "number") {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let rprim = match self.to_primitive(right, "number") {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                if lprim.is_bigint() || rprim.is_bigint() {
                    if op == BinaryOp::URShift {
                        return Completion::Throw(self.create_type_error(
                            "Cannot mix BigInt and other types, use explicit conversions",
                        ));
                    }
                    if let (JsValue::BigInt(a), JsValue::BigInt(b)) = (&lprim, &rprim) {
                        Completion::Normal(JsValue::BigInt(JsBigInt {
                            value: match op {
                                BinaryOp::LShift => bigint_ops::left_shift(&a.value, &b.value),
                                BinaryOp::RShift => {
                                    bigint_ops::signed_right_shift(&a.value, &b.value)
                                }
                                _ => unreachable!(),
                            },
                        }))
                    } else {
                        Completion::Throw(self.create_type_error(
                            "Cannot mix BigInt and other types, use explicit conversions",
                        ))
                    }
                } else {
                    let ln = to_number(&lprim);
                    let rn = to_number(&rprim);
                    Completion::Normal(JsValue::Number(match op {
                        BinaryOp::LShift => number_ops::left_shift(ln, rn),
                        BinaryOp::RShift => number_ops::signed_right_shift(ln, rn),
                        BinaryOp::URShift => number_ops::unsigned_right_shift(ln, rn),
                        _ => unreachable!(),
                    }))
                }
            }
            BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => {
                let lprim = match self.to_primitive(left, "number") {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let rprim = match self.to_primitive(right, "number") {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                if let (JsValue::BigInt(a), JsValue::BigInt(b)) = (&lprim, &rprim) {
                    Completion::Normal(JsValue::BigInt(JsBigInt {
                        value: match op {
                            BinaryOp::BitAnd => bigint_ops::bitwise_and(&a.value, &b.value),
                            BinaryOp::BitOr => bigint_ops::bitwise_or(&a.value, &b.value),
                            BinaryOp::BitXor => bigint_ops::bitwise_xor(&a.value, &b.value),
                            _ => unreachable!(),
                        },
                    }))
                } else if lprim.is_bigint() || rprim.is_bigint() {
                    Completion::Throw(self.create_type_error(
                        "Cannot mix BigInt and other types, use explicit conversions",
                    ))
                } else {
                    let ln = to_number(&lprim);
                    let rn = to_number(&rprim);
                    Completion::Normal(JsValue::Number(match op {
                        BinaryOp::BitAnd => number_ops::bitwise_and(ln, rn),
                        BinaryOp::BitOr => number_ops::bitwise_or(ln, rn),
                        BinaryOp::BitXor => number_ops::bitwise_xor(ln, rn),
                        _ => unreachable!(),
                    }))
                }
            }
            BinaryOp::In => {
                if let JsValue::Object(o) = &right {
                    let key = to_property_key_string(left);
                    match self.proxy_has_property(o.id, &key) {
                        Ok(result) => Completion::Normal(JsValue::Boolean(result)),
                        Err(e) => Completion::Throw(e),
                    }
                } else {
                    Completion::Throw(self.create_type_error(
                        "Cannot use 'in' operator to search for a property in a non-object",
                    ))
                }
            }
            BinaryOp::Instanceof => {
                unreachable!("instanceof handled before eval_binary")
            }
        }
    }

    fn eval_logical(
        &mut self,
        op: LogicalOp,
        left: &Expression,
        right: &Expression,
        env: &EnvRef,
    ) -> Completion {
        let lval = match self.eval_expr(left, env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        match op {
            LogicalOp::And => {
                if !self.to_boolean_val(&lval) {
                    Completion::Normal(lval)
                } else {
                    self.eval_expr(right, env)
                }
            }
            LogicalOp::Or => {
                if self.to_boolean_val(&lval) {
                    Completion::Normal(lval)
                } else {
                    self.eval_expr(right, env)
                }
            }
            LogicalOp::NullishCoalescing => {
                if lval.is_nullish() {
                    self.eval_expr(right, env)
                } else {
                    Completion::Normal(lval)
                }
            }
        }
    }

    fn apply_update_numeric(
        &mut self,
        raw_val: &JsValue,
        op: UpdateOp,
    ) -> Result<(JsValue, JsValue), JsValue> {
        // ToNumeric: ToPrimitive(number) then check for BigInt
        let numeric = if matches!(raw_val, JsValue::Object(_)) {
            self.to_primitive(raw_val, "number")?
        } else {
            raw_val.clone()
        };
        if let JsValue::BigInt(ref b) = numeric {
            use num_bigint::BigInt;
            let one = BigInt::from(1);
            let new_bigint = match op {
                UpdateOp::Increment => &b.value + &one,
                UpdateOp::Decrement => &b.value - &one,
            };
            let old_val = JsValue::BigInt(b.clone());
            let new_val = JsValue::BigInt(JsBigInt { value: new_bigint });
            Ok((old_val, new_val))
        } else if let JsValue::Symbol(_) = numeric {
            Err(self.create_type_error("Cannot convert a Symbol value to a number"))
        } else {
            let old_num = to_number(&numeric);
            let new_num = match op {
                UpdateOp::Increment => old_num + 1.0,
                UpdateOp::Decrement => old_num - 1.0,
            };
            Ok((JsValue::Number(old_num), JsValue::Number(new_num)))
        }
    }

    fn eval_update(
        &mut self,
        op: UpdateOp,
        prefix: bool,
        arg: &Expression,
        env: &EnvRef,
    ) -> Completion {
        if let Expression::Identifier(name) = arg {
            let strict = env.borrow().strict;
            let id_ref = match self.resolve_identifier_ref(name, env) {
                Ok(r) => r,
                Err(e) => return Completion::Throw(e),
            };
            let raw_val = match &id_ref {
                IdentifierRef::WithObject(obj_id) => {
                    match self.with_get_binding_value(*obj_id, name, strict) {
                        Completion::Normal(v) => v,
                        other => return other,
                    }
                }
                IdentifierRef::Binding => {
                    if let Some(result) = self.resolve_global_getter(name, env) {
                        match result {
                            Completion::Normal(v) => v,
                            other => return other,
                        }
                    } else {
                        match env.borrow().get(name) {
                            Some(v) => v,
                            None => {
                                let err =
                                    self.create_reference_error(&format!("{name} is not defined"));
                                return Completion::Throw(err);
                            }
                        }
                    }
                }
            };
            let (old_val, new_val) = match self.apply_update_numeric(&raw_val, op) {
                Ok(pair) => pair,
                Err(e) => return Completion::Throw(e),
            };
            match self.put_value_by_ref(name, new_val.clone(), &id_ref, env) {
                Completion::Normal(_) => {}
                other => return other,
            }
            Completion::Normal(if prefix { new_val } else { old_val })
        } else if let Expression::Member(obj_expr, prop) = arg {
            let obj_val = match self.eval_expr(obj_expr, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            if let MemberProperty::Private(name) = prop {
                let branded = self.resolve_private_name(name, env);
                return match &obj_val {
                    JsValue::Object(o) => {
                        if let Some(obj) = self.get_object(o.id) {
                            let elem = obj.borrow().private_fields.get(&branded).cloned();
                            match elem {
                                Some(PrivateElement::Field(cur)) => {
                                    let (old_val, new_val) =
                                        match self.apply_update_numeric(&cur, op) {
                                            Ok(pair) => pair,
                                            Err(e) => return Completion::Throw(e),
                                        };
                                    obj.borrow_mut()
                                        .private_fields
                                        .insert(branded, PrivateElement::Field(new_val.clone()));
                                    Completion::Normal(if prefix { new_val } else { old_val })
                                }
                                _ => Completion::Throw(self.create_type_error(&format!(
                                    "Cannot update private member #{name}"
                                ))),
                            }
                        } else {
                            Completion::Normal(JsValue::Number(f64::NAN))
                        }
                    }
                    _ => Completion::Throw(
                        self.create_type_error("Cannot read private member from a non-object"),
                    ),
                };
            }
            let key = match prop {
                MemberProperty::Dot(name) => name.clone(),
                MemberProperty::Computed(expr) => {
                    let v = match self.eval_expr(expr, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    // ToObject(base) must precede ToPropertyKey(prop) per spec
                    if matches!(&obj_val, JsValue::Null | JsValue::Undefined) {
                        let err = self.create_type_error(&format!(
                            "Cannot read properties of {obj_val} (reading property)"
                        ));
                        return Completion::Throw(err);
                    }
                    match self.to_property_key(&v) {
                        Ok(s) => s,
                        Err(e) => return Completion::Throw(e),
                    }
                }
                MemberProperty::Private(_) => unreachable!(),
            };
            // Get current value
            let cur_val = match &obj_val {
                JsValue::Object(o) => match self.get_object_property(o.id, &key, &obj_val) {
                    Completion::Normal(v) => v,
                    other => return other,
                },
                _ => {
                    // Primitive member access — use eval_member logic indirectly
                    match self.eval_expr(arg, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    }
                }
            };
            let (old_val, new_val) = match self.apply_update_numeric(&cur_val, op) {
                Ok(pair) => pair,
                Err(e) => return Completion::Throw(e),
            };
            // Set value back
            if let JsValue::Object(ref o) = obj_val
                && let Some(obj) = self.get_object(o.id)
            {
                if obj.borrow().is_proxy() || obj.borrow().proxy_revoked {
                    let receiver = obj_val.clone();
                    match self.proxy_set(o.id, &key, new_val.clone(), &receiver) {
                        Ok(_) => {}
                        Err(e) => return Completion::Throw(e),
                    }
                } else {
                    let _ = obj.borrow_mut().set_property_value(&key, new_val.clone());
                }
            }
            Completion::Normal(if prefix { new_val } else { old_val })
        } else if let Expression::Call(_, _) = arg {
            match self.eval_expr(arg, env) {
                Completion::Normal(_) => {}
                other => return other,
            }
            Completion::Throw(
                self.create_reference_error(
                    "Invalid left-hand side expression in update expression",
                ),
            )
        } else {
            Completion::Normal(JsValue::Number(f64::NAN))
        }
    }

    pub(crate) fn assign_to_expr(
        &mut self,
        expr: &Expression,
        value: JsValue,
        env: &EnvRef,
    ) -> Result<(), JsValue> {
        match expr {
            Expression::Member(obj_expr, prop) => {
                let obj_val = match self.eval_expr(obj_expr, env) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => return Ok(()),
                };
                let key = match prop {
                    MemberProperty::Dot(name) => name.clone(),
                    MemberProperty::Computed(cexpr) => {
                        let v = match self.eval_expr(cexpr, env) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Err(e),
                            _ => return Ok(()),
                        };
                        self.to_property_key(&v)?
                    }
                    MemberProperty::Private(_) => return Ok(()),
                };
                if let JsValue::Object(ref o) = obj_val
                    && let Some(obj) = self.get_object(o.id)
                {
                    // TypedArray [[Set]]
                    let is_ta = obj.borrow().typed_array_info.is_some();
                    if is_ta && let Some(index) = canonical_numeric_index_string(&key) {
                        let is_bigint = obj
                            .borrow()
                            .typed_array_info
                            .as_ref()
                            .map(|ta| ta.kind.is_bigint())
                            .unwrap_or(false);
                        let num_val = if is_bigint {
                            self.to_bigint_value(&value)?
                        } else {
                            JsValue::Number(self.to_number_value(&value)?)
                        };
                        let obj_ref = obj.borrow();
                        let ta = obj_ref.typed_array_info.as_ref().unwrap();
                        if is_valid_integer_index(ta, index) {
                            let ta_clone = ta.clone();
                            drop(obj_ref);
                            typed_array_set_index(&ta_clone, index as usize, &num_val);
                        }
                        return Ok(());
                    }
                    obj.borrow_mut().set_property_value(&key, value);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn eval_assign(
        &mut self,
        op: AssignOp,
        left: &Expression,
        right: &Expression,
        env: &EnvRef,
    ) -> Completion {
        // Logical assignments are short-circuit
        if matches!(
            op,
            AssignOp::LogicalAndAssign | AssignOp::LogicalOrAssign | AssignOp::NullishAssign
        ) {
            return self.eval_logical_assign(op, left, right, env);
        }

        match left {
            Expression::Identifier(name) => {
                if op == AssignOp::Assign {
                    // Capture reference BEFORE evaluating RHS (Bug B fix)
                    let id_ref = match self.resolve_identifier_ref(name, env) {
                        Ok(r) => r,
                        Err(e) => return Completion::Throw(e),
                    };
                    let rval = match self.eval_expr(right, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if right.is_anonymous_function_definition() {
                        self.set_function_name(&rval, name);
                    }
                    return self.put_value_by_ref(name, rval, &id_ref, env);
                }
                // Compound assignment: capture reference, read, eval RHS, write
                let id_ref = match self.resolve_identifier_ref(name, env) {
                    Ok(r) => r,
                    Err(e) => return Completion::Throw(e),
                };
                let strict = env.borrow().strict;
                let lval = match &id_ref {
                    IdentifierRef::WithObject(obj_id) => {
                        match self.with_get_binding_value(*obj_id, name, strict) {
                            Completion::Normal(v) => v,
                            other => return other,
                        }
                    }
                    IdentifierRef::Binding => {
                        if let Some(result) = self.resolve_global_getter(name, env) {
                            match result {
                                Completion::Normal(v) => v,
                                other => return other,
                            }
                        } else {
                            match env.borrow().get(name) {
                                Some(v) => v,
                                None => {
                                    return Completion::Throw(self.create_reference_error(
                                        &format!("{name} is not defined"),
                                    ));
                                }
                            }
                        }
                    }
                };
                let rval = match self.eval_expr(right, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let final_val = match self.apply_compound_assign(op, &lval, &rval) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                self.put_value_by_ref(name, final_val, &id_ref, env)
            }
            Expression::Member(obj_expr, prop) => {
                let obj_val = match self.eval_expr(obj_expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if let MemberProperty::Private(name) = prop {
                    let branded = self.resolve_private_name(name, env);
                    let rval = match self.eval_expr(right, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    return match &obj_val {
                        JsValue::Object(o) => {
                            if let Some(obj) = self.get_object(o.id) {
                                let elem = obj.borrow().private_fields.get(&branded).cloned();
                                match elem {
                                    Some(PrivateElement::Field(_)) => {
                                        let final_val = if op == AssignOp::Assign {
                                            rval
                                        } else {
                                            let lval = if let Some(PrivateElement::Field(v)) = obj.borrow().private_fields.get(&branded) {
                                                v.clone()
                                            } else {
                                                JsValue::Undefined
                                            };
                                            match self.apply_compound_assign(op, &lval, &rval) {
                                                Completion::Normal(v) => v,
                                                other => return other,
                                            }
                                        };
                                        obj.borrow_mut()
                                            .private_fields
                                            .insert(branded.clone(), PrivateElement::Field(final_val.clone()));
                                        Completion::Normal(final_val)
                                    }
                                    Some(PrivateElement::Method(_)) => {
                                        Completion::Throw(self.create_type_error(&format!(
                                            "Cannot assign to private method #{name}"
                                        )))
                                    }
                                    Some(PrivateElement::Accessor { get, set }) => {
                                        if let Some(setter) = &set {
                                            let final_val = if op == AssignOp::Assign {
                                                rval
                                            } else {
                                                let lval = if let Some(ref getter) = get {
                                                    match self.call_function(getter, &obj_val, &[]) {
                                                        Completion::Normal(v) => v,
                                                        other => return other,
                                                    }
                                                } else {
                                                    JsValue::Undefined
                                                };
                                                match self.apply_compound_assign(op, &lval, &rval) {
                                                    Completion::Normal(v) => v,
                                                    other => return other,
                                                }
                                            };
                                            let setter = setter.clone();
                                            self.call_function(&setter, &obj_val, std::slice::from_ref(&final_val));
                                            Completion::Normal(final_val)
                                        } else {
                                            Completion::Throw(self.create_type_error(&format!(
                                                "Cannot set private member #{name} which has no setter"
                                            )))
                                        }
                                    }
                                    None => {
                                        Completion::Throw(self.create_type_error(&format!(
                                            "Cannot write private member #{name} to an object whose class did not declare it"
                                        )))
                                    }
                                }
                            } else {
                                Completion::Normal(JsValue::Undefined)
                            }
                        }
                        _ => Completion::Throw(self.create_type_error(&format!(
                            "Cannot write private member #{name} to a non-object"
                        ))),
                    };
                }
                // Evaluate computed key expression before RHS
                let key_val = match prop {
                    MemberProperty::Computed(expr) => {
                        let v = match self.eval_expr(expr, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        Some(v)
                    }
                    _ => None,
                };
                // For compound ops, compute property key and get current value before RHS
                let (key, lval_for_compound) =
                    if op != AssignOp::Assign {
                        // §6.2.5.5 GetValue: if base is null/undefined, throw TypeError
                        // before ToPropertyKey (§13.3.3 EvaluatePropertyAccessWithExpressionKey
                        // stores the uncoerced key in the Reference)
                        if obj_val.is_null() || obj_val.is_undefined() {
                            let base_str = if obj_val.is_null() {
                                "null"
                            } else {
                                "undefined"
                            };
                            return Completion::Throw(self.create_type_error(&format!(
                                "Cannot read properties of {base_str}"
                            )));
                        }
                        let key = match prop {
                            MemberProperty::Dot(name) => name.clone(),
                            MemberProperty::Computed(_) => {
                                match self.to_property_key(key_val.as_ref().unwrap()) {
                                    Ok(s) => s,
                                    Err(e) => return Completion::Throw(e),
                                }
                            }
                            MemberProperty::Private(_) => unreachable!(),
                        };
                        let lval = if let JsValue::Object(ref o) = obj_val
                            && let Some(obj) = self.get_object(o.id)
                        {
                            obj.borrow().get_property(&key)
                        } else {
                            match self.to_object(&obj_val) {
                                Completion::Normal(wrapped) => {
                                    if let JsValue::Object(ref o) = wrapped
                                        && let Some(obj) = self.get_object(o.id)
                                    {
                                        obj.borrow().get_property(&key)
                                    } else {
                                        JsValue::Undefined
                                    }
                                }
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => JsValue::Undefined,
                            }
                        };
                        (key, Some(lval))
                    } else {
                        (String::new(), None) // key computed after RHS for simple assign
                    };
                // Now evaluate RHS
                let rval = match self.eval_expr(right, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                // For simple assign, compute key now
                let key = if op == AssignOp::Assign {
                    match prop {
                        MemberProperty::Dot(name) => name.clone(),
                        MemberProperty::Computed(_) => {
                            match self.to_property_key(key_val.as_ref().unwrap()) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        MemberProperty::Private(_) => unreachable!(),
                    }
                } else {
                    key
                };
                // super.x = val: check that the super reference base is valid,
                // then set the property on `this` (the receiver)
                if matches!(obj_expr.as_ref(), Expression::Super) {
                    // Check HomeObject's prototype exists (the reference base)
                    let home = env.borrow().get("__home_object__");
                    let has_valid_base = if let Some(JsValue::Object(ref ho)) = home
                        && let Some(home_obj) = self.get_object(ho.id)
                    {
                        home_obj.borrow().prototype.is_some()
                    } else {
                        // Fallback: check __super__.prototype
                        if let JsValue::Object(ref o) = obj_val
                            && let Some(sup_obj) = self.get_object(o.id)
                        {
                            matches!(
                                sup_obj.borrow().get_property("prototype"),
                                JsValue::Object(_)
                            )
                        } else {
                            false
                        }
                    };
                    if !has_valid_base {
                        return Completion::Throw(self.create_type_error(&format!(
                            "Cannot set properties of null (setting '{}')",
                            key
                        )));
                    }
                    let this_val = env.borrow().get("this").unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(ref o) = this_val
                        && let Some(obj) = self.get_object(o.id)
                    {
                        let final_val = if op == AssignOp::Assign {
                            rval
                        } else {
                            match self.apply_compound_assign(op, &lval_for_compound.unwrap(), &rval)
                            {
                                Completion::Normal(v) => v,
                                other => return other,
                            }
                        };
                        let success = obj.borrow_mut().set_property_value(&key, final_val.clone());
                        if !success && env.borrow().strict {
                            return Completion::Throw(self.create_type_error(&format!(
                                "Cannot assign to read only property '{key}'"
                            )));
                        }
                        return Completion::Normal(final_val);
                    }
                    return Completion::Throw(self.create_type_error(&format!(
                        "Cannot set properties of {} (setting '{}')",
                        if this_val.is_null() {
                            "null"
                        } else {
                            "undefined"
                        },
                        key
                    )));
                }
                // Throw for null/undefined base
                if obj_val.is_null() || obj_val.is_undefined() {
                    return Completion::Throw(self.create_type_error(&format!(
                        "Cannot set properties of {} (setting '{}')",
                        if obj_val.is_null() {
                            "null"
                        } else {
                            "undefined"
                        },
                        key
                    )));
                }
                if let JsValue::Object(ref o) = obj_val
                    && let Some(obj) = self.get_object(o.id)
                {
                    let final_val = if op == AssignOp::Assign {
                        rval
                    } else {
                        match self.apply_compound_assign(op, &lval_for_compound.unwrap(), &rval) {
                            Completion::Normal(v) => v,
                            other => return other,
                        }
                    };
                    // Proxy set trap
                    if obj.borrow().is_proxy() || obj.borrow().proxy_revoked {
                        let receiver = obj_val.clone();
                        match self.proxy_set(o.id, &key, final_val.clone(), &receiver) {
                            Ok(success) => {
                                if !success && env.borrow().strict {
                                    return Completion::Throw(self.create_type_error(&format!(
                                        "Cannot assign to read only property '{key}'"
                                    )));
                                }
                                return Completion::Normal(final_val);
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    // Module namespace [[Set]] always returns false (§10.4.6.5)
                    if obj.borrow().module_namespace.is_some() {
                        if env.borrow().strict {
                            return Completion::Throw(self.create_type_error(&format!(
                                "Cannot assign to read only property '{key}' of object '[object Module]'"
                            )));
                        }
                        return Completion::Normal(final_val);
                    }
                    // Check for setter
                    let desc = obj.borrow().get_property_descriptor(&key);
                    if let Some(ref d) = desc
                        && let Some(ref setter) = d.set
                        && !matches!(setter, JsValue::Undefined)
                    {
                        let setter = setter.clone();
                        let this = obj_val.clone();
                        return match self.call_function(
                            &setter,
                            &this,
                            std::slice::from_ref(&final_val),
                        ) {
                            Completion::Normal(_) => Completion::Normal(final_val),
                            other => other,
                        };
                    }
                    if desc
                        .as_ref()
                        .map(|d| d.is_accessor_descriptor())
                        .unwrap_or(false)
                    {
                        if env.borrow().strict {
                            return Completion::Throw(self.create_type_error(&format!(
                                "Cannot set property '{key}' which has only a getter"
                            )));
                        }
                        return Completion::Normal(final_val);
                    }
                    // TypedArray [[Set]]: ToNumber/ToBigInt before index check
                    {
                        let is_ta = obj.borrow().typed_array_info.is_some();
                        if is_ta && let Some(index) = canonical_numeric_index_string(&key) {
                            let is_bigint = obj
                                .borrow()
                                .typed_array_info
                                .as_ref()
                                .map(|ta| ta.kind.is_bigint())
                                .unwrap_or(false);
                            // Convert value first (may throw)
                            let num_val = if is_bigint {
                                match self.to_bigint_value(&final_val) {
                                    Ok(v) => v,
                                    Err(e) => return Completion::Throw(e),
                                }
                            } else {
                                match self.to_number_value(&final_val) {
                                    Ok(n) => JsValue::Number(n),
                                    Err(e) => return Completion::Throw(e),
                                }
                            };
                            let obj_ref = obj.borrow();
                            let ta = obj_ref.typed_array_info.as_ref().unwrap();
                            if is_valid_integer_index(ta, index) {
                                let ta_clone = ta.clone();
                                drop(obj_ref);
                                typed_array_set_index(&ta_clone, index as usize, &num_val);
                            }
                            return Completion::Normal(final_val);
                        }
                    }
                    // OrdinarySet (§10.1.9.2): if no own property, walk prototype chain
                    if !obj.borrow().has_own_property(&key) {
                        let mut proto_opt = obj.borrow().prototype.clone();
                        while let Some(proto_rc) = proto_opt {
                            let proto_id = proto_rc.borrow().id.unwrap();
                            if self.has_proxy_in_prototype_chain(proto_id) {
                                let receiver = obj_val.clone();
                                match self.proxy_set(proto_id, &key, final_val.clone(), &receiver) {
                                    Ok(success) => {
                                        if !success && env.borrow().strict {
                                            return Completion::Throw(self.create_type_error(
                                                &format!(
                                                    "Cannot assign to read only property '{key}'"
                                                ),
                                            ));
                                        }
                                        return Completion::Normal(final_val);
                                    }
                                    Err(e) => return Completion::Throw(e),
                                }
                            }
                            let inherited = proto_rc.borrow().get_property_descriptor(&key);
                            if let Some(ref inherited_desc) = inherited {
                                if inherited_desc.is_data_descriptor() {
                                    if inherited_desc.writable == Some(false) {
                                        if env.borrow().strict {
                                            return Completion::Throw(self.create_type_error(
                                                &format!(
                                                    "Cannot assign to read only property '{key}'"
                                                ),
                                            ));
                                        }
                                        return Completion::Normal(final_val);
                                    }
                                    break;
                                }
                                if inherited_desc.is_accessor_descriptor() {
                                    if let Some(ref setter) = inherited_desc.set
                                        && !matches!(setter, JsValue::Undefined)
                                    {
                                        let setter = setter.clone();
                                        let this = obj_val.clone();
                                        return match self.call_function(
                                            &setter,
                                            &this,
                                            std::slice::from_ref(&final_val),
                                        ) {
                                            Completion::Normal(_) => Completion::Normal(final_val),
                                            other => other,
                                        };
                                    }
                                    if env.borrow().strict {
                                        return Completion::Throw(self.create_type_error(
                                            &format!(
                                                "Cannot set property '{key}' which has only a getter"
                                            ),
                                        ));
                                    }
                                    return Completion::Normal(final_val);
                                }
                                break;
                            }
                            proto_opt = proto_rc.borrow().prototype.clone();
                        }
                    }
                    // ArraySetLength validation
                    if key == "length" && obj.borrow().class_name == "Array" {
                        let num = match self.to_number_value(&final_val) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        };
                        let uint32 = num as u32;
                        if (uint32 as f64) != num || num < 0.0 || num.is_nan() || num.is_infinite()
                        {
                            return Completion::Throw(
                                self.create_error("RangeError", "Invalid array length"),
                            );
                        }
                        let length_val = JsValue::Number(uint32 as f64);
                        let success = obj
                            .borrow_mut()
                            .set_property_value(&key, length_val.clone());
                        if !success && env.borrow().strict {
                            return Completion::Throw(self.create_type_error(&format!(
                                "Cannot assign to read only property '{key}'"
                            )));
                        }
                        return Completion::Normal(length_val);
                    }
                    let success = obj.borrow_mut().set_property_value(&key, final_val.clone());
                    if !success && env.borrow().strict {
                        return Completion::Throw(self.create_type_error(&format!(
                            "Cannot assign to read only property '{key}'"
                        )));
                    }
                    return Completion::Normal(final_val);
                }
                // Non-object base: in strict mode, throw TypeError
                if env.borrow().strict {
                    return Completion::Throw(self.create_type_error(&format!(
                        "Cannot create property '{key}' on {obj_val}"
                    )));
                }
                Completion::Normal(rval)
            }
            Expression::Array(elements) if op == AssignOp::Assign => {
                let rval = match self.eval_expr(right, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                match self.destructure_array_assignment(elements, &rval, env) {
                    Completion::Normal(_) => Completion::Normal(rval),
                    other => other,
                }
            }
            Expression::Object(props) if op == AssignOp::Assign => {
                let rval = match self.eval_expr(right, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                match self.destructure_object_assignment(props, &rval, env) {
                    Completion::Normal(_) => Completion::Normal(rval),
                    other => other,
                }
            }
            Expression::Call(_, _) => {
                match self.eval_expr(left, env) {
                    Completion::Normal(_) => {}
                    other => return other,
                }
                Completion::Throw(
                    self.create_reference_error("Invalid left-hand side in assignment"),
                )
            }
            _ => {
                let rval = match self.eval_expr(right, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                Completion::Normal(rval)
            }
        }
    }

    fn eval_logical_assign(
        &mut self,
        op: AssignOp,
        left: &Expression,
        right: &Expression,
        env: &EnvRef,
    ) -> Completion {
        match left {
            Expression::Identifier(name) => {
                let id_ref = match self.resolve_identifier_ref(name, env) {
                    Ok(r) => r,
                    Err(e) => return Completion::Throw(e),
                };
                let strict = env.borrow().strict;
                let lval = match &id_ref {
                    IdentifierRef::WithObject(obj_id) => {
                        match self.with_get_binding_value(*obj_id, name, strict) {
                            Completion::Normal(v) => v,
                            other => return other,
                        }
                    }
                    IdentifierRef::Binding => {
                        if let Some(result) = self.resolve_global_getter(name, env) {
                            match result {
                                Completion::Normal(v) => v,
                                other => return other,
                            }
                        } else {
                            match env.borrow().get(name) {
                                Some(v) => v,
                                None => {
                                    return Completion::Throw(self.create_reference_error(
                                        &format!("{name} is not defined"),
                                    ));
                                }
                            }
                        }
                    }
                };
                let should_assign = match op {
                    AssignOp::LogicalAndAssign => self.to_boolean_val(&lval),
                    AssignOp::LogicalOrAssign => !self.to_boolean_val(&lval),
                    AssignOp::NullishAssign => lval.is_null() || lval.is_undefined(),
                    _ => unreachable!(),
                };
                if !should_assign {
                    return Completion::Normal(lval);
                }
                let rval = match self.eval_expr(right, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if right.is_anonymous_function_definition() {
                    self.set_function_name(&rval, name);
                }
                self.put_value_by_ref(name, rval, &id_ref, env)
            }
            Expression::Member(obj_expr, MemberProperty::Private(name)) => {
                let branded = self.resolve_private_name(name, env);
                let obj_val = match self.eval_expr(obj_expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let lval = match &obj_val {
                    JsValue::Object(o) => {
                        if let Some(obj) = self.get_object(o.id) {
                            let elem = obj.borrow().private_fields.get(&branded).cloned();
                            match elem {
                                Some(PrivateElement::Field(v)) => v,
                                Some(PrivateElement::Method(v)) => v,
                                Some(PrivateElement::Accessor { get, .. }) => {
                                    if let Some(ref getter) = get {
                                        match self.call_function(getter, &obj_val, &[]) {
                                            Completion::Normal(v) => v,
                                            other => return other,
                                        }
                                    } else {
                                        JsValue::Undefined
                                    }
                                }
                                None => {
                                    return Completion::Throw(self.create_type_error(&format!(
                                        "Cannot read private member #{name} from an object whose class did not declare it"
                                    )));
                                }
                            }
                        } else {
                            JsValue::Undefined
                        }
                    }
                    _ => {
                        return Completion::Throw(self.create_type_error(&format!(
                            "Cannot read private member #{name} from a non-object"
                        )));
                    }
                };
                let should_assign = match op {
                    AssignOp::LogicalAndAssign => self.to_boolean_val(&lval),
                    AssignOp::LogicalOrAssign => !self.to_boolean_val(&lval),
                    AssignOp::NullishAssign => lval.is_null() || lval.is_undefined(),
                    _ => unreachable!(),
                };
                if !should_assign {
                    return Completion::Normal(lval);
                }
                let rval = match self.eval_expr(right, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                match &obj_val {
                    JsValue::Object(o) => {
                        if let Some(obj) = self.get_object(o.id) {
                            let elem = obj.borrow().private_fields.get(&branded).cloned();
                            match elem {
                                Some(PrivateElement::Field(_)) => {
                                    obj.borrow_mut().private_fields.insert(
                                        branded.clone(),
                                        PrivateElement::Field(rval.clone()),
                                    );
                                }
                                Some(PrivateElement::Method(_)) => {
                                    return Completion::Throw(self.create_type_error(&format!(
                                        "Cannot assign to private method #{name}"
                                    )));
                                }
                                Some(PrivateElement::Accessor { set, .. }) => {
                                    if let Some(setter) = &set {
                                        let setter = setter.clone();
                                        self.call_function(
                                            &setter,
                                            &obj_val,
                                            std::slice::from_ref(&rval),
                                        );
                                    } else {
                                        return Completion::Throw(self.create_type_error(&format!(
                                            "Cannot set private member #{name} which has no setter"
                                        )));
                                    }
                                }
                                None => {
                                    return Completion::Throw(self.create_type_error(&format!(
                                        "Cannot write private member #{name} to an object whose class did not declare it"
                                    )));
                                }
                            }
                        }
                    }
                    _ => {
                        return Completion::Throw(self.create_type_error(&format!(
                            "Cannot write private member #{name} to a non-object"
                        )));
                    }
                }
                Completion::Normal(rval)
            }
            Expression::Member(obj_expr, prop) => {
                let obj_val = match self.eval_expr(obj_expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                // Evaluate key expression (but defer ToPropertyKey for null/undefined base)
                let key_expr_val = match prop {
                    MemberProperty::Computed(expr) => {
                        let v = match self.eval_expr(expr, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        Some(v)
                    }
                    _ => None,
                };
                // GetValue: ToObject(base) first, then ToPropertyKey
                let (boxed_obj, key) = if let JsValue::Object(ref _o) = obj_val {
                    let key = match prop {
                        MemberProperty::Dot(name) => name.clone(),
                        MemberProperty::Computed(_) => {
                            match self.to_property_key(key_expr_val.as_ref().unwrap()) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        MemberProperty::Private(_) => unreachable!(),
                    };
                    (obj_val.clone(), key)
                } else {
                    let boxed = match self.to_object(&obj_val) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => return Completion::Normal(JsValue::Undefined),
                    };
                    let key = match prop {
                        MemberProperty::Dot(name) => name.clone(),
                        MemberProperty::Computed(_) => {
                            match self.to_property_key(key_expr_val.as_ref().unwrap()) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        MemberProperty::Private(_) => unreachable!(),
                    };
                    (boxed, key)
                };
                let lval = if let JsValue::Object(ref o) = boxed_obj {
                    match self.get_object_property(o.id, &key, &obj_val) {
                        Completion::Normal(v) => v,
                        other => return other,
                    }
                } else {
                    JsValue::Undefined
                };
                let should_assign = match op {
                    AssignOp::LogicalAndAssign => self.to_boolean_val(&lval),
                    AssignOp::LogicalOrAssign => !self.to_boolean_val(&lval),
                    AssignOp::NullishAssign => lval.is_null() || lval.is_undefined(),
                    _ => unreachable!(),
                };
                if !should_assign {
                    return Completion::Normal(lval);
                }
                let rval = match self.eval_expr(right, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                // Write back (boxed_obj is already the ToObject result)
                if let JsValue::Object(ref o) = boxed_obj
                    && let Some(obj) = self.get_object(o.id)
                {
                    if obj.borrow().is_proxy() || obj.borrow().proxy_revoked {
                        let receiver = boxed_obj.clone();
                        match self.proxy_set(o.id, &key, rval.clone(), &receiver) {
                            Ok(success) => {
                                if !success && env.borrow().strict {
                                    return Completion::Throw(self.create_type_error(&format!(
                                        "Cannot assign to read only property '{key}'"
                                    )));
                                }
                                return Completion::Normal(rval);
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    let desc = obj.borrow().get_property_descriptor(&key);
                    if let Some(ref d) = desc
                        && let Some(ref setter) = d.set
                        && !matches!(setter, JsValue::Undefined)
                    {
                        let setter = setter.clone();
                        let this = boxed_obj.clone();
                        return match self.call_function(&setter, &this, std::slice::from_ref(&rval))
                        {
                            Completion::Normal(_) => Completion::Normal(rval),
                            other => other,
                        };
                    }
                    if desc
                        .as_ref()
                        .map(|d| d.is_accessor_descriptor())
                        .unwrap_or(false)
                    {
                        if env.borrow().strict {
                            return Completion::Throw(self.create_type_error(&format!(
                                "Cannot set property '{key}' which has only a getter"
                            )));
                        }
                        return Completion::Normal(rval);
                    }
                    if !obj.borrow().has_own_property(&key) {
                        let proto = obj.borrow().prototype.clone();
                        if let Some(proto_rc) = proto {
                            let proto_id = proto_rc.borrow().id.unwrap();
                            if self.has_proxy_in_prototype_chain(proto_id) {
                                let receiver = boxed_obj.clone();
                                match self.proxy_set(proto_id, &key, rval.clone(), &receiver) {
                                    Ok(success) => {
                                        if !success && env.borrow().strict {
                                            return Completion::Throw(self.create_type_error(
                                                &format!(
                                                    "Cannot assign to read only property '{key}'"
                                                ),
                                            ));
                                        }
                                        return Completion::Normal(rval);
                                    }
                                    Err(e) => return Completion::Throw(e),
                                }
                            }
                        }
                    }
                    // ArraySetLength validation: reject non-integral/negative length values
                    if key == "length" && obj.borrow().class_name == "Array" {
                        let num = match self.to_number_value(&rval) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        };
                        let uint32 = num as u32;
                        if (uint32 as f64) != num || num < 0.0 || num.is_nan() || num.is_infinite()
                        {
                            return Completion::Throw(
                                self.create_error("RangeError", "Invalid array length"),
                            );
                        }
                        let rval = JsValue::Number(uint32 as f64);
                        let success = obj.borrow_mut().set_property_value(&key, rval.clone());
                        if !success && env.borrow().strict {
                            return Completion::Throw(self.create_type_error(&format!(
                                "Cannot assign to read only property '{key}'"
                            )));
                        }
                        return Completion::Normal(rval);
                    }
                    let success = obj.borrow_mut().set_property_value(&key, rval.clone());
                    if !success && env.borrow().strict {
                        return Completion::Throw(self.create_type_error(&format!(
                            "Cannot assign to read only property '{key}'"
                        )));
                    }
                }
                Completion::Normal(rval)
            }
            _ => {
                // Fallback: just evaluate both sides
                let lval = match self.eval_expr(left, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let should_assign = match op {
                    AssignOp::LogicalAndAssign => self.to_boolean_val(&lval),
                    AssignOp::LogicalOrAssign => !self.to_boolean_val(&lval),
                    AssignOp::NullishAssign => lval.is_null() || lval.is_undefined(),
                    _ => unreachable!(),
                };
                if !should_assign {
                    return Completion::Normal(lval);
                }
                match self.eval_expr(right, env) {
                    Completion::Normal(v) => Completion::Normal(v),
                    other => other,
                }
            }
        }
    }

    fn apply_compound_assign(
        &mut self,
        op: AssignOp,
        lval: &JsValue,
        rval: &JsValue,
    ) -> Completion {
        match op {
            AssignOp::AddAssign => self.eval_binary(BinaryOp::Add, lval, rval),
            AssignOp::SubAssign => self.eval_binary(BinaryOp::Sub, lval, rval),
            AssignOp::MulAssign => self.eval_binary(BinaryOp::Mul, lval, rval),
            AssignOp::DivAssign => self.eval_binary(BinaryOp::Div, lval, rval),
            AssignOp::ModAssign => self.eval_binary(BinaryOp::Mod, lval, rval),
            AssignOp::ExpAssign => self.eval_binary(BinaryOp::Exp, lval, rval),
            AssignOp::LShiftAssign => self.eval_binary(BinaryOp::LShift, lval, rval),
            AssignOp::RShiftAssign => self.eval_binary(BinaryOp::RShift, lval, rval),
            AssignOp::URShiftAssign => self.eval_binary(BinaryOp::URShift, lval, rval),
            AssignOp::BitAndAssign => self.eval_binary(BinaryOp::BitAnd, lval, rval),
            AssignOp::BitOrAssign => self.eval_binary(BinaryOp::BitOr, lval, rval),
            AssignOp::BitXorAssign => self.eval_binary(BinaryOp::BitXor, lval, rval),
            _ => Completion::Normal(rval.clone()),
        }
    }

    /// Set a property on an already-evaluated object+key pair (strict controls TypeError on failure).
    fn set_object_with_key(
        &mut self,
        obj_val: JsValue,
        key: &str,
        val: JsValue,
        strict: bool,
    ) -> Result<(), JsValue> {
        // Auto-box primitives for property access
        let obj_val = if !matches!(obj_val, JsValue::Object(_)) {
            match self.to_object(&obj_val) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => return Ok(()),
            }
        } else {
            obj_val
        };

        if let JsValue::Object(ref o) = obj_val
            && let Some(obj) = self.get_object(o.id)
        {
            // Proxy set trap
            if obj.borrow().is_proxy() || obj.borrow().proxy_revoked {
                let receiver = obj_val.clone();
                match self.proxy_set(o.id, key, val, &receiver) {
                    Ok(success) => {
                        if !success && strict {
                            return Err(self.create_type_error(&format!(
                                "Cannot assign to read only property '{key}'"
                            )));
                        }
                        return Ok(());
                    }
                    Err(e) => return Err(e),
                }
            }
            // Module namespace exotic: [[Set]] always returns false
            if obj.borrow().module_namespace.is_some() {
                if strict {
                    return Err(self.create_type_error(&format!(
                        "Cannot assign to read only property '{key}' of module namespace"
                    )));
                }
                return Ok(());
            }
            // Check for setter
            let desc = obj.borrow().get_property_descriptor(key);
            if let Some(ref d) = desc
                && let Some(ref setter) = d.set
                && !matches!(setter, JsValue::Undefined)
            {
                let setter = setter.clone();
                let this = obj_val.clone();
                return match self.call_function(&setter, &this, &[val]) {
                    Completion::Normal(_) => Ok(()),
                    Completion::Throw(e) => Err(e),
                    _ => Ok(()),
                };
            }
            if desc
                .as_ref()
                .map(|d| d.is_accessor_descriptor())
                .unwrap_or(false)
            {
                if strict {
                    return Err(self.create_type_error(&format!(
                        "Cannot set property '{key}' which has only a getter"
                    )));
                }
                return Ok(());
            }
            // TypedArray [[Set]]
            let is_ta = obj.borrow().typed_array_info.is_some();
            if is_ta && let Some(index) = canonical_numeric_index_string(key) {
                let is_bigint = obj
                    .borrow()
                    .typed_array_info
                    .as_ref()
                    .map(|ta| ta.kind.is_bigint())
                    .unwrap_or(false);
                let num_val = if is_bigint {
                    self.to_bigint_value(&val)?
                } else {
                    JsValue::Number(self.to_number_value(&val)?)
                };
                let obj_ref = obj.borrow();
                let ta = obj_ref.typed_array_info.as_ref().unwrap();
                if is_valid_integer_index(ta, index) {
                    let ta_clone = ta.clone();
                    drop(obj_ref);
                    typed_array_set_index(&ta_clone, index as usize, &num_val);
                }
                return Ok(());
            }
            // OrdinarySet (§10.1.9.2): if no own property, walk prototype chain
            if !obj.borrow().has_own_property(key) {
                let mut proto_opt = obj.borrow().prototype.clone();
                while let Some(proto_rc) = proto_opt {
                    let proto_id = proto_rc.borrow().id.unwrap();
                    if self.has_proxy_in_prototype_chain(proto_id) {
                        let receiver = obj_val.clone();
                        match self.proxy_set(proto_id, key, val, &receiver) {
                            Ok(success) => {
                                if !success && strict {
                                    return Err(self.create_type_error(&format!(
                                        "Cannot assign to read only property '{key}'"
                                    )));
                                }
                                return Ok(());
                            }
                            Err(e) => return Err(e),
                        }
                    }
                    let inherited = proto_rc.borrow().get_property_descriptor(key);
                    if let Some(ref inherited_desc) = inherited {
                        if inherited_desc.is_data_descriptor() {
                            if inherited_desc.writable == Some(false) {
                                if strict {
                                    return Err(self.create_type_error(&format!(
                                        "Cannot assign to read only property '{key}'"
                                    )));
                                }
                                return Ok(());
                            }
                            break;
                        }
                        if inherited_desc.is_accessor_descriptor() {
                            if let Some(ref setter) = inherited_desc.set
                                && !matches!(setter, JsValue::Undefined)
                            {
                                let setter = setter.clone();
                                let this = obj_val.clone();
                                return match self.call_function(&setter, &this, &[val]) {
                                    Completion::Normal(_) => Ok(()),
                                    Completion::Throw(e) => Err(e),
                                    _ => Ok(()),
                                };
                            }
                            if strict {
                                return Err(self.create_type_error(&format!(
                                    "Cannot set property '{key}' which has only a getter"
                                )));
                            }
                            return Ok(());
                        }
                        break;
                    }
                    proto_opt = proto_rc.borrow().prototype.clone();
                }
            }
            let success = obj.borrow_mut().set_property_value(key, val);
            if !success && strict {
                return Err(
                    self.create_type_error(&format!("Cannot assign to read only property '{key}'"))
                );
            }
        }
        Ok(())
    }

    fn set_member_property(
        &mut self,
        obj_expr: &Expression,
        prop: &MemberProperty,
        val: JsValue,
        env: &EnvRef,
    ) -> Result<(), JsValue> {
        let obj_val = match self.eval_expr(obj_expr, env) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => return Ok(()),
        };

        if let MemberProperty::Private(name) = prop {
            let branded = self.resolve_private_name(name, env);
            return match &obj_val {
                JsValue::Object(o) => {
                    if let Some(obj) = self.get_object(o.id) {
                        let elem = obj.borrow().private_fields.get(&branded).cloned();
                        match elem {
                            Some(PrivateElement::Field(_)) => {
                                obj.borrow_mut()
                                    .private_fields
                                    .insert(branded, PrivateElement::Field(val));
                                Ok(())
                            }
                            Some(PrivateElement::Method(_)) => Err(self.create_type_error(
                                &format!("Cannot assign to private method #{name}"),
                            )),
                            Some(PrivateElement::Accessor { set, .. }) => {
                                if let Some(setter) = &set {
                                    let setter = setter.clone();
                                    match self.call_function(&setter, &obj_val, &[val]) {
                                        Completion::Normal(_) => Ok(()),
                                        Completion::Throw(e) => Err(e),
                                        _ => Ok(()),
                                    }
                                } else {
                                    Err(self.create_type_error(&format!(
                                        "Cannot set private member #{name} which has no setter"
                                    )))
                                }
                            }
                            None => Err(self.create_type_error(&format!(
                                "Cannot write private member #{name} to an object whose class did not declare it"
                            ))),
                        }
                    } else {
                        Ok(())
                    }
                }
                _ => Err(self.create_type_error(&format!(
                    "Cannot write private member #{name} to a non-object"
                ))),
            };
        }

        let key = match prop {
            MemberProperty::Dot(name) => name.clone(),
            MemberProperty::Computed(expr) => {
                let v = match self.eval_expr(expr, env) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => return Ok(()),
                };
                self.to_property_key(&v)?
            }
            MemberProperty::Private(_) => unreachable!(),
        };

        let strict = env.borrow().strict;
        self.set_object_with_key(obj_val, &key, val, strict)
    }
    pub(crate) fn assign_to_for_pattern(
        &mut self,
        pat: &crate::ast::Pattern,
        val: JsValue,
        env: &EnvRef,
    ) -> Completion {
        let expr = Self::pattern_to_assignment_expr(pat);
        self.put_value_to_target(&expr, val, env)
    }

    fn pattern_to_assignment_expr(pat: &crate::ast::Pattern) -> crate::ast::Expression {
        use crate::ast::*;
        match pat {
            Pattern::Identifier(name) => Expression::Identifier(name.clone()),
            Pattern::Array(elements) => {
                let exprs = elements
                    .iter()
                    .map(|elem| {
                        elem.as_ref().map(|e| match e {
                            ArrayPatternElement::Pattern(p) => Self::pattern_to_assignment_expr(p),
                            ArrayPatternElement::Rest(p) => {
                                Expression::Spread(Box::new(Self::pattern_to_assignment_expr(p)))
                            }
                        })
                    })
                    .collect();
                Expression::Array(exprs)
            }
            Pattern::Object(props) => {
                let obj_props = props
                    .iter()
                    .map(|prop| match prop {
                        ObjectPatternProperty::KeyValue(key, p) => Property {
                            key: key.clone(),
                            value: Self::pattern_to_assignment_expr(p),
                            kind: PropertyKind::Init,
                            computed: matches!(key, PropertyKey::Computed(_)),
                            shorthand: false,
                        },
                        ObjectPatternProperty::Shorthand(name) => Property {
                            key: PropertyKey::Identifier(name.clone()),
                            value: Expression::Identifier(name.clone()),
                            kind: PropertyKind::Init,
                            computed: false,
                            shorthand: true,
                        },
                        ObjectPatternProperty::Rest(p) => Property {
                            key: PropertyKey::Identifier("__rest__".to_string()),
                            value: Expression::Spread(Box::new(Self::pattern_to_assignment_expr(
                                p,
                            ))),
                            kind: PropertyKind::Init,
                            computed: false,
                            shorthand: false,
                        },
                    })
                    .collect();
                Expression::Object(obj_props)
            }
            Pattern::Assign(inner, default) => Expression::Assign(
                AssignOp::Assign,
                Box::new(Self::pattern_to_assignment_expr(inner)),
                default.clone(),
            ),
            Pattern::Rest(inner) => {
                Expression::Spread(Box::new(Self::pattern_to_assignment_expr(inner)))
            }
            Pattern::MemberExpression(expr) => *expr.clone(),
        }
    }

    fn put_value_to_target(
        &mut self,
        target: &Expression,
        val: JsValue,
        env: &EnvRef,
    ) -> Completion {
        let result = match target {
            Expression::Identifier(name) => {
                let id_ref = match self.resolve_identifier_ref(name, env) {
                    Ok(r) => r,
                    Err(e) => return Completion::Throw(e),
                };
                match self.put_value_by_ref(name, val, &id_ref, env) {
                    Completion::Normal(_) => Completion::Normal(JsValue::Undefined),
                    other => other,
                }
            }
            Expression::Member(obj_expr, prop) => {
                match self.set_member_property(obj_expr, prop, val, env) {
                    Ok(()) => Completion::Normal(JsValue::Undefined),
                    Err(e) => Completion::Throw(e),
                }
            }
            Expression::Array(elements) => self.destructure_array_assignment(elements, &val, env),
            Expression::Object(props) => self.destructure_object_assignment(props, &val, env),
            Expression::Assign(AssignOp::Assign, inner_target, default) => {
                let v = if val.is_undefined() {
                    match self.eval_expr(default, env) {
                        Completion::Normal(v) => v,
                        other => {
                            if matches!(other, Completion::Yield(_)) {
                                self.destructuring_yield = true;
                            }
                            return other;
                        }
                    }
                } else {
                    val
                };
                self.put_value_to_target(inner_target, v, env)
            }
            _ => Completion::Normal(JsValue::Undefined),
        };
        if matches!(result, Completion::Yield(_)) {
            self.destructuring_yield = true;
        }
        result
    }

    /// Evaluate a member expression as an LHS reference (base object + key string).
    /// Returns Ok(Some((base, key))) for member expressions,
    /// Ok(None) for non-member expressions (handled lazily),
    /// Err(e) if evaluation throws.
    /// Sets *yield_val if a yield is encountered.
    fn eval_member_lhs_ref(
        &mut self,
        target: &Expression,
        env: &EnvRef,
        yield_val: &mut Option<JsValue>,
    ) -> Result<Option<(JsValue, String)>, JsValue> {
        let Expression::Member(obj_expr, prop) = target else {
            return Ok(None);
        };
        // Skip private members — they can't throw during key evaluation
        if matches!(prop, MemberProperty::Private(_)) {
            return Ok(None);
        }

        let base = match self.eval_expr(obj_expr, env) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            Completion::Yield(v) => {
                *yield_val = Some(v);
                return Ok(None);
            }
            _ => return Ok(None),
        };

        let key = match prop {
            MemberProperty::Dot(name) => name.clone(),
            MemberProperty::Computed(key_expr) => {
                let kv = match self.eval_expr(key_expr, env) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    Completion::Yield(v) => {
                        *yield_val = Some(v);
                        return Ok(None);
                    }
                    _ => return Ok(None),
                };
                self.to_property_key(&kv)?
            }
            MemberProperty::Private(_) => unreachable!(),
        };

        Ok(Some((base, key)))
    }

    fn destructure_array_assignment(
        &mut self,
        elements: &[Option<Expression>],
        rval: &JsValue,
        env: &EnvRef,
    ) -> Completion {
        let iterator = match self.get_iterator(rval) {
            Ok(v) => v,
            Err(e) => return Completion::Throw(e),
        };
        if let JsValue::Object(o) = &iterator {
            self.gc_temp_roots.push(o.id);
        }
        let mut done = false;
        let mut error: Option<JsValue> = None;
        let mut yield_val: Option<JsValue> = None;

        for elem in elements {
            match elem {
                None => {
                    // Elision — skip one iterator position
                    if !done {
                        match self.iterator_step(&iterator) {
                            Ok(None) => done = true,
                            Ok(Some(_)) => {}
                            Err(e) => {
                                done = true;
                                error = Some(e);
                                break;
                            }
                        }
                    }
                }
                Some(Expression::Spread(inner)) => {
                    // §13.15.5.4 AssignmentRestElement: evaluate LHS ref BEFORE collecting
                    let precomp = match self.eval_member_lhs_ref(inner, env, &mut yield_val) {
                        Ok(r) => r,
                        Err(e) => {
                            error = Some(e);
                            break;
                        }
                    };
                    if yield_val.is_some() {
                        break;
                    }

                    // Collect remaining iterator values into rest array
                    let mut rest = Vec::new();
                    if !done {
                        loop {
                            match self.iterator_step(&iterator) {
                                Ok(Some(result)) => match self.iterator_value(&result) {
                                    Ok(v) => rest.push(v),
                                    Err(e) => {
                                        done = true;
                                        error = Some(e);
                                        break;
                                    }
                                },
                                Ok(None) => {
                                    done = true;
                                    break;
                                }
                                Err(e) => {
                                    done = true;
                                    error = Some(e);
                                    break;
                                }
                            }
                        }
                    }
                    if error.is_none() {
                        let arr = self.create_array(rest);
                        match precomp {
                            Some((base, key)) => {
                                let strict = env.borrow().strict;
                                if let Err(e) = self.set_object_with_key(base, &key, arr, strict) {
                                    error = Some(e);
                                }
                            }
                            None => match self.put_value_to_target(inner, arr, env) {
                                Completion::Normal(_) | Completion::Empty => {}
                                Completion::Throw(e) => {
                                    error = Some(e);
                                }
                                Completion::Yield(v) => {
                                    yield_val = Some(v);
                                }
                                _ => {}
                            },
                        }
                    }
                    break;
                }
                Some(expr) => {
                    // Extract target and default
                    let (target, default_expr) =
                        if let Expression::Assign(AssignOp::Assign, target, default) = expr {
                            (target.as_ref(), Some(default.as_ref()))
                        } else {
                            (expr, None)
                        };

                    // §13.15.5.4: evaluate LHS reference BEFORE stepping the iterator
                    let precomp = match self.eval_member_lhs_ref(target, env, &mut yield_val) {
                        Ok(r) => r,
                        Err(e) => {
                            error = Some(e);
                            break;
                        }
                    };
                    if yield_val.is_some() {
                        break;
                    }

                    let item = if done {
                        JsValue::Undefined
                    } else {
                        match self.iterator_step(&iterator) {
                            Ok(Some(result)) => match self.iterator_value(&result) {
                                Ok(v) => v,
                                Err(e) => {
                                    done = true;
                                    error = Some(e);
                                    break;
                                }
                            },
                            Ok(None) => {
                                done = true;
                                JsValue::Undefined
                            }
                            Err(e) => {
                                done = true;
                                error = Some(e);
                                break;
                            }
                        }
                    };

                    let val = if item.is_undefined() {
                        if let Some(default) = default_expr {
                            match self.eval_expr(default, env) {
                                Completion::Normal(v) => {
                                    if let Expression::Identifier(name) = target
                                        && default.is_anonymous_function_definition()
                                    {
                                        self.set_function_name(&v, name);
                                    }
                                    v
                                }
                                Completion::Throw(e) => {
                                    error = Some(e);
                                    break;
                                }
                                Completion::Yield(v) => {
                                    yield_val = Some(v);
                                    break;
                                }
                                other => return other,
                            }
                        } else {
                            item
                        }
                    } else {
                        item
                    };

                    match precomp {
                        Some((base, key)) => {
                            let strict = env.borrow().strict;
                            if let Err(e) = self.set_object_with_key(base, &key, val, strict) {
                                error = Some(e);
                                break;
                            }
                        }
                        None => match self.put_value_to_target(target, val, env) {
                            Completion::Normal(_) | Completion::Empty => {}
                            Completion::Throw(e) => {
                                error = Some(e);
                                break;
                            }
                            Completion::Yield(v) => {
                                yield_val = Some(v);
                                break;
                            }
                            _ => {}
                        },
                    }
                }
            }
        }

        let unroot = |s: &mut Self| {
            if let JsValue::Object(o) = &iterator
                && let Some(pos) = s.gc_temp_roots.iter().rposition(|&id| id == o.id)
            {
                s.gc_temp_roots.remove(pos);
            }
        };

        if let Some(yv) = yield_val {
            // §13.15.5.2: if iterator not done, track it for IteratorClose when generator returns
            if !done {
                self.pending_iter_close.push(iterator.clone());
            }
            unroot(self);
            return Completion::Yield(yv);
        }

        // §13.15.5.2: IteratorClose when done is false
        if !done {
            if let Some(err) = error {
                let _ = self.iterator_close_result(&iterator);
                unroot(self);
                return Completion::Throw(err);
            }
            let r = self.iterator_close_result(&iterator);
            unroot(self);
            return match r {
                Ok(()) => Completion::Normal(JsValue::Undefined),
                Err(e) => Completion::Throw(e),
            };
        }

        unroot(self);
        if let Some(err) = error {
            return Completion::Throw(err);
        }
        Completion::Normal(JsValue::Undefined)
    }

    fn destructure_object_assignment(
        &mut self,
        props: &[Property],
        rval: &JsValue,
        env: &EnvRef,
    ) -> Completion {
        // RequireObjectCoercible
        if let Completion::Throw(e) = self.require_object_coercible(rval) {
            return Completion::Throw(e);
        }

        // ToObject to wrap primitives
        let obj_val = match self.to_object(rval) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Completion::Throw(e),
            _ => unreachable!(),
        };

        let mut excluded_keys: Vec<String> = Vec::new();

        for prop in props {
            // Handle rest: {...rest} = obj
            if let Expression::Spread(inner) = &prop.value {
                let rest_obj = self.create_object();
                if let JsValue::Object(o) = &obj_val {
                    let pairs = match self.copy_data_properties(o.id, &obj_val, &excluded_keys) {
                        Ok(p) => p,
                        Err(e) => return Completion::Throw(e),
                    };
                    for (k, v) in pairs {
                        rest_obj.borrow_mut().insert_value(k, v);
                    }
                }
                let rest_id = rest_obj.borrow().id.unwrap();
                let rest_val = JsValue::Object(crate::types::JsObject { id: rest_id });
                match self.put_value_to_target(inner, rest_val, env) {
                    Completion::Normal(_) | Completion::Empty => {}
                    other => return other,
                }
                continue;
            }

            match &prop.kind {
                PropertyKind::Init => {
                    let key = match &prop.key {
                        PropertyKey::Identifier(s) | PropertyKey::String(s) => s.clone(),
                        PropertyKey::Number(n) => to_js_string(&JsValue::Number(*n)),
                        PropertyKey::Computed(expr) => match self.eval_expr(expr, env) {
                            Completion::Normal(v) => match self.to_property_key(&v) {
                                Ok(k) => k,
                                Err(e) => return Completion::Throw(e),
                            },
                            Completion::Throw(e) => return Completion::Throw(e),
                            Completion::Yield(v) => return Completion::Yield(v),
                            other => return other,
                        },
                        PropertyKey::Private(_) => {
                            return Completion::Throw(self.create_type_error(
                                "Private names are not valid in object patterns",
                            ));
                        }
                    };
                    excluded_keys.push(key.clone());

                    // Get property via get_object_property (invokes getters/Proxy)
                    let val = if let JsValue::Object(o) = &obj_val {
                        match self.get_object_property(o.id, &key, &obj_val) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            Completion::Yield(v) => return Completion::Yield(v),
                            _ => JsValue::Undefined,
                        }
                    } else {
                        JsValue::Undefined
                    };

                    // Extract target and default from value
                    let (target, default_expr) = if let Expression::Assign(
                        AssignOp::Assign,
                        target,
                        default,
                    ) = &prop.value
                    {
                        (target.as_ref(), Some(default.as_ref()))
                    } else {
                        (&prop.value, None)
                    };

                    let val = if val.is_undefined() {
                        if let Some(default) = default_expr {
                            match self.eval_expr(default, env) {
                                Completion::Normal(v) => {
                                    if let Expression::Identifier(name) = target
                                        && default.is_anonymous_function_definition()
                                    {
                                        self.set_function_name(&v, name);
                                    }
                                    v
                                }
                                Completion::Throw(e) => return Completion::Throw(e),
                                Completion::Yield(v) => return Completion::Yield(v),
                                other => return other,
                            }
                        } else {
                            val
                        }
                    } else {
                        val
                    };

                    match self.put_value_to_target(target, val, env) {
                        Completion::Normal(_) | Completion::Empty => {}
                        other => return other,
                    }
                }
                _ => continue,
            }
        }
        Completion::Normal(JsValue::Undefined)
    }

    fn eval_call(&mut self, callee: &Expression, args: &[Expression], env: &EnvRef) -> Completion {
        // Handle super() calls - call parent constructor with current this
        if matches!(callee, Expression::Super) {
            let super_ctor = env.borrow().get("__super__").unwrap_or(JsValue::Undefined);
            let arg_vals = match self.eval_spread_args(args, env) {
                Ok(v) => v,
                Err(e) => return Completion::Throw(e),
            };
            let this_in_tdz = Self::this_is_in_tdz(env);
            if this_in_tdz {
                // Derived constructor: use Construct semantics
                // Per spec §13.3.7.1: super() must forward the derived class's new.target
                let current_new_target = self.new_target.clone().unwrap_or(super_ctor.clone());
                let saved_new_target = self.new_target.clone();
                let result =
                    self.construct_with_new_target(&super_ctor, &arg_vals, current_new_target);
                self.new_target = saved_new_target;
                if let Completion::Normal(ref v) = result {
                    // Set prototype from new.target.prototype (the derived class)
                    if let JsValue::Object(this_obj) = v
                        && let Some(nt) = &self.new_target
                        && let JsValue::Object(nt_o) = nt
                        && let Some(nt_func) = self.get_object(nt_o.id)
                    {
                        let proto_val = nt_func.borrow().get_property_value("prototype");
                        if let Some(JsValue::Object(proto_obj)) = proto_val
                            && let Some(proto_rc) = self.get_object(proto_obj.id)
                            && let Some(obj) = self.get_object(this_obj.id)
                        {
                            obj.borrow_mut().prototype = Some(proto_rc);
                        }
                    }
                    // Bind this in the function environment
                    Self::initialize_this_binding(env, v.clone());
                    // Initialize instance elements (private/public fields) for the current class
                    if let Err(e) = self.initialize_instance_elements(v.clone(), env) {
                        return Completion::Throw(e);
                    }
                }
                return result;
            } else {
                // this is already bound — second super() call in derived constructor
                // Per spec: Construct runs first, then BindThisValue throws
                let current_new_target = self.new_target.clone().unwrap_or(super_ctor.clone());
                let saved_new_target = self.new_target.clone();
                let result =
                    self.construct_with_new_target(&super_ctor, &arg_vals, current_new_target);
                self.new_target = saved_new_target;
                if let Completion::Throw(_) = result {
                    return result;
                }
                return Completion::Throw(self.create_reference_error(
                    "'super()' has already been called in this derived constructor",
                ));
            }
        }

        // Handle member calls: obj.method()
        let (func_val, this_val) = match callee {
            Expression::Member(obj_expr, prop) => {
                let is_super_call = matches!(obj_expr.as_ref(), Expression::Super);
                let obj_val = match self.eval_expr(obj_expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let key = match prop {
                    MemberProperty::Dot(name) => name.clone(),
                    MemberProperty::Computed(expr) => {
                        let v = match self.eval_expr(expr, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        match self.to_property_key(&v) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    MemberProperty::Private(name) => {
                        let branded = self.resolve_private_name(name, env);
                        if let JsValue::Object(ref o) = obj_val
                            && let Some(obj) = self.get_object(o.id)
                        {
                            let elem = obj.borrow().private_fields.get(&branded).cloned();
                            let func_val = match elem {
                                Some(PrivateElement::Field(v))
                                | Some(PrivateElement::Method(v)) => v,
                                Some(PrivateElement::Accessor { get, .. }) => {
                                    if let Some(getter) = get {
                                        match self.call_function(&getter, &obj_val, &[]) {
                                            Completion::Normal(v) => v,
                                            other => return other,
                                        }
                                    } else {
                                        return Completion::Throw(self.create_type_error(&format!(
                                                "Cannot read private member #{name} which has no getter"
                                            )));
                                    }
                                }
                                None => {
                                    return Completion::Throw(self.create_type_error(&format!(
                                            "Cannot read private member #{name} from an object whose class did not declare it"
                                        )));
                                }
                            };
                            let mut evaluated_args = Vec::new();
                            for arg in args {
                                match arg {
                                    Expression::Spread(inner) => {
                                        let spread_val = match self.eval_expr(inner, env) {
                                            Completion::Normal(v) => v,
                                            other => return other,
                                        };
                                        if let Ok(items) = self.iterate_to_vec(&spread_val) {
                                            evaluated_args.extend(items);
                                        }
                                    }
                                    _ => match self.eval_expr(arg, env) {
                                        Completion::Normal(v) => evaluated_args.push(v),
                                        other => return other,
                                    },
                                }
                            }
                            return self.call_function(&func_val, &obj_val, &evaluated_args);
                        }
                        return Completion::Throw(self.create_type_error(&format!(
                            "Cannot read private member #{name} from a non-object"
                        )));
                    }
                };
                // super.method() - look up on [[Prototype]] of HomeObject, bind this
                if is_super_call {
                    let this_val = env.borrow().get("this").unwrap_or(JsValue::Undefined);
                    let home = env.borrow().get("__home_object__");
                    if let Some(JsValue::Object(ref ho)) = home
                        && let Some(home_obj) = self.get_object(ho.id)
                    {
                        if let Some(ref proto_rc) = home_obj.borrow().prototype.clone() {
                            let method = proto_rc.borrow().get_property(&key);
                            (method, this_val)
                        } else {
                            return Completion::Throw(self.create_type_error(&format!(
                                "Cannot read properties of null (reading '{key}')"
                            )));
                        }
                    } else if let JsValue::Object(ref o) = obj_val {
                        // Fallback: __super__.prototype for class super
                        if let Some(obj) = self.get_object(o.id) {
                            let proto_val = obj.borrow().get_property("prototype");
                            if let JsValue::Object(ref p) = proto_val {
                                if let Some(proto) = self.get_object(p.id) {
                                    let method = proto.borrow().get_property(&key);
                                    (method, this_val)
                                } else {
                                    (JsValue::Undefined, JsValue::Undefined)
                                }
                            } else {
                                (JsValue::Undefined, JsValue::Undefined)
                            }
                        } else {
                            (JsValue::Undefined, JsValue::Undefined)
                        }
                    } else {
                        (JsValue::Undefined, JsValue::Undefined)
                    }
                } else if let JsValue::Object(ref o) = obj_val {
                    let oid = o.id;
                    let ov = obj_val.clone();
                    match self.get_object_property(oid, &key, &ov) {
                        Completion::Normal(method) => (method, obj_val),
                        other => return other,
                    }
                } else if let JsValue::String(_) = &obj_val {
                    if let Some(ref sp) = self.realm().string_prototype {
                        let method = sp.borrow().get_property(&key);
                        (method, obj_val)
                    } else {
                        (JsValue::Undefined, obj_val)
                    }
                } else if matches!(&obj_val, JsValue::Number(_)) {
                    let proto = self.realm()
                        .number_prototype
                        .clone()
                        .or(self.realm().object_prototype.clone());
                    if let Some(ref p) = proto {
                        let method = p.borrow().get_property(&key);
                        (method, obj_val)
                    } else {
                        (JsValue::Undefined, obj_val)
                    }
                } else if matches!(&obj_val, JsValue::Boolean(_)) {
                    let proto = self.realm()
                        .boolean_prototype
                        .clone()
                        .or(self.realm().object_prototype.clone());
                    if let Some(ref p) = proto {
                        let method = p.borrow().get_property(&key);
                        (method, obj_val)
                    } else {
                        (JsValue::Undefined, obj_val)
                    }
                } else if matches!(&obj_val, JsValue::Symbol(_)) {
                    if let Some(ref p) = self.realm().symbol_prototype {
                        let desc = p.borrow().get_property_descriptor(&key);
                        let method = match desc {
                            Some(ref d) if d.get.is_some() => {
                                let getter = d.get.clone().unwrap();
                                match self.call_function(&getter, &obj_val, &[]) {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                }
                            }
                            Some(ref d) => d.value.clone().unwrap_or(JsValue::Undefined),
                            None => JsValue::Undefined,
                        };
                        (method, obj_val)
                    } else {
                        (JsValue::Undefined, obj_val)
                    }
                } else if matches!(&obj_val, JsValue::BigInt(_)) {
                    let proto = self.realm()
                        .bigint_prototype
                        .clone()
                        .or(self.realm().object_prototype.clone());
                    if let Some(ref p) = proto {
                        let method = p.borrow().get_property(&key);
                        (method, obj_val)
                    } else {
                        (JsValue::Undefined, obj_val)
                    }
                } else if matches!(&obj_val, JsValue::Undefined | JsValue::Null) {
                    let err = self.create_type_error(&format!(
                        "Cannot read properties of {obj_val} (reading '{key}')"
                    ));
                    return Completion::Throw(err);
                } else {
                    (JsValue::Undefined, obj_val)
                }
            }
            _ => {
                let val = match self.eval_expr(callee, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                (val, JsValue::Undefined)
            }
        };

        // Direct eval: callee is bare `eval` identifier and resolves to built-in eval
        if matches!(callee, Expression::Identifier(n) if n == "eval")
            && self.is_builtin_eval(&func_val)
        {
            let evaluated_args = match self.eval_spread_args(args, env) {
                Ok(args) => args,
                Err(e) => return Completion::Throw(e),
            };
            let caller_strict = env.borrow().strict;
            return self.perform_eval(&evaluated_args, caller_strict, true, env);
        }

        // Root func_val and this_val before evaluating args (which may trigger GC)
        self.gc_root_value(&func_val);
        self.gc_root_value(&this_val);
        let evaluated_args = match self.eval_spread_args(args, env) {
            Ok(args) => args,
            Err(e) => {
                self.gc_unroot_value(&this_val);
                self.gc_unroot_value(&func_val);
                return Completion::Throw(e);
            }
        };

        self.gc_unroot_value(&this_val);
        self.gc_unroot_value(&func_val);
        self.call_function(&func_val, &this_val, &evaluated_args)
    }

    pub(crate) fn generator_next(&mut self, this: &JsValue, sent_value: JsValue) -> Completion {
        let JsValue::Object(o) = this else {
            let err = self.create_type_error("Generator.prototype.next called on non-object");
            return Completion::Throw(err);
        };
        let Some(obj_rc) = self.get_object(o.id) else {
            let err = self.create_type_error("Generator.prototype.next called on non-object");
            return Completion::Throw(err);
        };

        // Extract state (must release borrow before executing body)
        let state = obj_rc.borrow().iterator_state.clone();
        let Some(IteratorState::Generator {
            body,
            func_env,
            is_strict,
            execution_state,
        }) = state
        else {
            let err = self.create_type_error("not a generator object");
            return Completion::Throw(err);
        };

        // Determine target_yield based on execution state
        let target_yield = match &execution_state {
            GeneratorExecutionState::Completed => {
                return Completion::Normal(
                    self.create_iter_result_object(JsValue::Undefined, true),
                );
            }
            GeneratorExecutionState::Executing => {
                return Completion::Throw(self.create_type_error("Generator is already executing"));
            }
            GeneratorExecutionState::SuspendedStart => 0,
            GeneratorExecutionState::SuspendedYield { target_yield } => *target_yield,
        };

        // Mark as executing
        obj_rc.borrow_mut().iterator_state = Some(IteratorState::Generator {
            body: body.clone(),
            func_env: func_env.clone(),
            is_strict,
            execution_state: GeneratorExecutionState::Executing,
        });

        // Set generator context - for yield* delegation and sent values
        self.generator_context = Some(GeneratorContext {
            target_yield,
            current_yield: 0,
            sent_value,
            is_async: false,
        });

        func_env.borrow_mut().strict = is_strict;
        self.call_stack_envs.push(func_env.clone());
        let result = self.exec_statements(&body, &func_env);
        self.call_stack_envs.pop();
        let _ctx = self.generator_context.take();

        match result {
            Completion::Yield(v) => {
                obj_rc.borrow_mut().iterator_state = Some(IteratorState::Generator {
                    body: body.clone(),
                    func_env,
                    is_strict,
                    execution_state: GeneratorExecutionState::SuspendedYield {
                        target_yield: target_yield + 1,
                    },
                });
                Completion::Normal(self.create_iter_result_object(v, false))
            }
            Completion::Return(v) => {
                obj_rc.borrow_mut().iterator_state = Some(IteratorState::Generator {
                    body,
                    func_env,
                    is_strict,
                    execution_state: GeneratorExecutionState::Completed,
                });
                Completion::Normal(self.create_iter_result_object(v, true))
            }
            Completion::Normal(_) | Completion::Empty => {
                obj_rc.borrow_mut().iterator_state = Some(IteratorState::Generator {
                    body,
                    func_env,
                    is_strict,
                    execution_state: GeneratorExecutionState::Completed,
                });
                Completion::Normal(self.create_iter_result_object(JsValue::Undefined, true))
            }
            Completion::Throw(e) => {
                obj_rc.borrow_mut().iterator_state = Some(IteratorState::Generator {
                    body,
                    func_env,
                    is_strict,
                    execution_state: GeneratorExecutionState::Completed,
                });
                Completion::Throw(e)
            }
            other => other,
        }
    }

    pub(crate) fn generator_return(&mut self, this: &JsValue, value: JsValue) -> Completion {
        let JsValue::Object(o) = this else {
            let err = self.create_type_error("Generator.prototype.return called on non-object");
            return Completion::Throw(err);
        };
        let Some(obj_rc) = self.get_object(o.id) else {
            let err = self.create_type_error("Generator.prototype.return called on non-object");
            return Completion::Throw(err);
        };

        let state = obj_rc.borrow().iterator_state.clone();
        if let Some(IteratorState::Generator {
            body,
            func_env,
            is_strict,
            ..
        }) = state
        {
            obj_rc.borrow_mut().iterator_state = Some(IteratorState::Generator {
                body,
                func_env,
                is_strict,
                execution_state: GeneratorExecutionState::Completed,
            });
            Completion::Normal(self.create_iter_result_object(value, true))
        } else {
            Completion::Throw(
                self.create_type_error(
                    "Generator.prototype.return called on incompatible receiver",
                ),
            )
        }
    }

    pub(crate) fn generator_throw(&mut self, this: &JsValue, exception: JsValue) -> Completion {
        let JsValue::Object(o) = this else {
            let err = self.create_type_error("Generator.prototype.throw called on non-object");
            return Completion::Throw(err);
        };
        let Some(obj_rc) = self.get_object(o.id) else {
            let err = self.create_type_error("Generator.prototype.throw called on non-object");
            return Completion::Throw(err);
        };

        let state = obj_rc.borrow().iterator_state.clone();
        if let Some(IteratorState::Generator {
            body,
            func_env,
            is_strict,
            ..
        }) = state
        {
            obj_rc.borrow_mut().iterator_state = Some(IteratorState::Generator {
                body,
                func_env,
                is_strict,
                execution_state: GeneratorExecutionState::Completed,
            });
            Completion::Throw(exception)
        } else {
            Completion::Throw(
                self.create_type_error("Generator.prototype.throw called on incompatible receiver"),
            )
        }
    }

    pub(crate) fn generator_next_state_machine(
        &mut self,
        this: &JsValue,
        sent_value: JsValue,
    ) -> Completion {
        use crate::interpreter::generator_transform::StateTerminator;

        let JsValue::Object(o) = this else {
            return Completion::Throw(
                self.create_type_error("Generator.prototype.next called on non-object"),
            );
        };
        let Some(obj_rc) = self.get_object(o.id) else {
            return Completion::Throw(
                self.create_type_error("Generator.prototype.next called on non-object"),
            );
        };

        let state = obj_rc.borrow().iterator_state.clone();
        let Some(IteratorState::StateMachineGenerator {
            state_machine,
            func_env,
            is_strict,
            execution_state,
            try_stack,
            pending_binding,
            delegated_iterator,
            pending_exception: stored_pending_exception,
            pending_return: stored_pending_return,
            ..
        }) = state
        else {
            return Completion::Throw(self.create_type_error("not a state machine generator"));
        };

        if let Some(ref deleg_info) = delegated_iterator {
            let iterator = deleg_info.iterator.clone();
            let next_method = deleg_info.next_method.clone();
            let resume_state = deleg_info.resume_state;
            let binding = deleg_info.sent_value_binding.clone();

            let result = match self.call_function(
                &next_method,
                &iterator,
                std::slice::from_ref(&sent_value),
            ) {
                Completion::Normal(v) if matches!(v, JsValue::Object(_)) => Ok(v),
                Completion::Normal(_) => {
                    Err(self.create_type_error("Iterator result is not an object"))
                }
                Completion::Throw(e) => Err(e),
                _ => Err(self.create_type_error("Iterator next failed")),
            };
            match result {
                Ok(iter_result) => {
                    let done = match self.iterator_complete(&iter_result) {
                        Ok(d) => d,
                        Err(e) => return Completion::Throw(e),
                    };
                    if done {
                        let value = match self.iterator_value(&iter_result) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };
                        if let Some(ref bind) = binding {
                            use crate::interpreter::generator_transform::SentValueBindingKind;
                            match &bind.kind {
                                SentValueBindingKind::Variable(name) => {
                                    func_env.borrow_mut().set(name, value.clone()).ok();
                                }
                                SentValueBindingKind::Pattern(pattern) => {
                                    let _ = self.bind_pattern(
                                        pattern,
                                        value.clone(),
                                        BindingKind::Var,
                                        &func_env,
                                    );
                                }
                                SentValueBindingKind::Discard
                                | SentValueBindingKind::InlineYield { .. } => {}
                            }
                        }
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineGenerator {
                                state_machine: state_machine.clone(),
                                func_env: func_env.clone(),
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: resume_state,
                                },
                                _sent_value: JsValue::Undefined,
                                try_stack: try_stack.clone(),
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            });
                        return self.generator_next_state_machine(this, JsValue::Undefined);
                    } else {
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: resume_state,
                                },
                                _sent_value: JsValue::Undefined,
                                try_stack,
                                pending_binding: None,
                                delegated_iterator: Some(
                                    crate::interpreter::types::DelegatedIteratorInfo {
                                        iterator,
                                        next_method: next_method.clone(),
                                        resume_state,
                                        sent_value_binding: binding,
                                    },
                                ),
                                pending_exception: None,
                                pending_return: None,
                            });
                        // Per spec §14.4.14: yield innerResult directly
                        return Completion::Normal(iter_result);
                    }
                }
                Err(e) => {
                    // Clear delegation and propagate error through generator's
                    // try-stack so the generator's own catch/finally can handle it
                    obj_rc.borrow_mut().iterator_state =
                        Some(IteratorState::StateMachineGenerator {
                            state_machine: state_machine.clone(),
                            func_env: func_env.clone(),
                            is_strict,
                            execution_state: StateMachineExecutionState::SuspendedAtState {
                                state_id: resume_state,
                            },
                            _sent_value: JsValue::Undefined,
                            try_stack: try_stack.clone(),
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        });
                    return self.generator_throw_state_machine(this, e);
                }
            }
        }

        let current_state_id = match &execution_state {
            StateMachineExecutionState::Completed => {
                return Completion::Normal(
                    self.create_iter_result_object(JsValue::Undefined, true),
                );
            }
            StateMachineExecutionState::Executing => {
                return Completion::Throw(self.create_type_error("Generator is already executing"));
            }
            StateMachineExecutionState::SuspendedStart => 0,
            StateMachineExecutionState::SuspendedAtState { state_id } => *state_id,
        };

        obj_rc.borrow_mut().iterator_state = Some(IteratorState::StateMachineGenerator {
            state_machine: state_machine.clone(),
            func_env: func_env.clone(),
            is_strict,
            execution_state: StateMachineExecutionState::Executing,
            _sent_value: sent_value.clone(),
            try_stack: try_stack.clone(),
            pending_binding: None,
            delegated_iterator: None,
            pending_exception: None,
            pending_return: None,
        });

        use crate::interpreter::generator_transform::SentValueBindingKind;
        let mut initial_inline_yield_target: Option<usize> = None;
        let mut initial_inline_yield_sent: Option<JsValue> = None;
        if let Some(binding) = pending_binding {
            match &binding.kind {
                SentValueBindingKind::Variable(name) => {
                    func_env.borrow_mut().set(name, sent_value.clone()).ok();
                }
                SentValueBindingKind::Pattern(pattern) => {
                    let _ =
                        self.bind_pattern(pattern, sent_value.clone(), BindingKind::Var, &func_env);
                }
                SentValueBindingKind::Discard => {}
                SentValueBindingKind::InlineYield { yield_target } => {
                    initial_inline_yield_target = Some(*yield_target);
                    initial_inline_yield_sent = Some(sent_value.clone());
                }
            }
        }

        func_env.borrow_mut().strict = is_strict;
        let mut current_id = current_state_id;
        let mut current_try_stack = try_stack;
        let mut pending_exception: Option<JsValue> = stored_pending_exception;
        let mut pending_return: Option<JsValue> = stored_pending_return;
        let mut inline_yield_target: Option<usize> = initial_inline_yield_target;
        let mut inline_yield_sent: Option<JsValue> = initial_inline_yield_sent;

        loop {
            let (statements, terminator) = {
                let gen_state = &state_machine.states[current_id];
                (gen_state.statements.clone(), gen_state.terminator.clone())
            };

            let is_inline_replay = inline_yield_target.is_some();
            if let Some(target) = inline_yield_target.take() {
                let sv = inline_yield_sent.take().unwrap_or(JsValue::Undefined);
                self.generator_context = Some(GeneratorContext {
                    target_yield: target,
                    current_yield: 0,
                    sent_value: sv,
                    is_async: false,
                });
            }

            let stmt_result = self.exec_statements(&statements, &func_env);
            let ctx_after = if is_inline_replay {
                self.generator_context.take()
            } else {
                None
            };

            if let Completion::Yield(yield_val) = stmt_result {
                self.destructuring_yield = false;
                let yield_count = ctx_after.map(|c| c.current_yield).unwrap_or(1);
                // Save any iterators that need IteratorClose if generator.return() is called
                let pending = std::mem::take(&mut self.pending_iter_close);
                if !pending.is_empty() {
                    self.generator_inline_iters.insert(o.id, pending);
                }
                obj_rc.borrow_mut().iterator_state = Some(IteratorState::StateMachineGenerator {
                    state_machine: state_machine.clone(),
                    func_env: func_env.clone(),
                    is_strict,
                    execution_state: StateMachineExecutionState::SuspendedAtState {
                        state_id: current_id,
                    },
                    _sent_value: JsValue::Undefined,
                    try_stack: current_try_stack.clone(),
                    pending_binding: Some(
                        crate::interpreter::generator_transform::SentValueBinding {
                            kind: SentValueBindingKind::InlineYield {
                                yield_target: yield_count,
                            },
                        },
                    ),
                    delegated_iterator: None,
                    pending_exception: None,
                    pending_return: None,
                });
                return Completion::Normal(self.create_iter_result_object(yield_val, false));
            }
            if let Completion::Throw(e) = stmt_result {
                if let Some(try_info) = current_try_stack.pop() {
                    if let Some(catch_state) = try_info.catch_state {
                        pending_exception = Some(e);
                        current_id = catch_state;
                        continue;
                    } else if let Some(finally_state) = try_info.finally_state {
                        current_id = finally_state;
                        continue;
                    }
                }
                obj_rc.borrow_mut().iterator_state = Some(IteratorState::StateMachineGenerator {
                    state_machine,
                    func_env,
                    is_strict,
                    execution_state: StateMachineExecutionState::Completed,
                    _sent_value: JsValue::Undefined,
                    try_stack: vec![],
                    pending_binding: None,
                    delegated_iterator: None,
                    pending_exception: None,
                    pending_return: None,
                });
                self.generator_inline_iters.remove(&o.id);
                return Completion::Throw(e);
            }
            if let Completion::Return(v) = stmt_result {
                obj_rc.borrow_mut().iterator_state = Some(IteratorState::StateMachineGenerator {
                    state_machine,
                    func_env,
                    is_strict,
                    execution_state: StateMachineExecutionState::Completed,
                    _sent_value: JsValue::Undefined,
                    try_stack: vec![],
                    pending_binding: None,
                    delegated_iterator: None,
                    pending_exception: None,
                    pending_return: None,
                });
                self.generator_inline_iters.remove(&o.id);
                return Completion::Normal(self.create_iter_result_object(v, true));
            }

            match &terminator {
                StateTerminator::Yield {
                    value,
                    is_delegate,
                    resume_state,
                    sent_value_binding,
                } => {
                    let yield_val = if let Some(expr) = value {
                        match self.eval_expr(expr, &func_env) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                obj_rc.borrow_mut().iterator_state =
                                    Some(IteratorState::StateMachineGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    });
                                return Completion::Throw(e);
                            }
                            other => return other,
                        }
                    } else {
                        JsValue::Undefined
                    };

                    if *is_delegate {
                        let iterator = match self.get_iterator(&yield_val) {
                            Ok(it) => it,
                            Err(e) => {
                                if let Some(try_info) = current_try_stack.last()
                                    && let Some(catch_state) = try_info.catch_state
                                {
                                    let new_try_stack =
                                        current_try_stack[..current_try_stack.len() - 1].to_vec();
                                    obj_rc.borrow_mut().iterator_state =
                                        Some(IteratorState::StateMachineGenerator {
                                            state_machine: state_machine.clone(),
                                            func_env: func_env.clone(),
                                            is_strict,
                                            execution_state:
                                                StateMachineExecutionState::SuspendedAtState {
                                                    state_id: catch_state,
                                                },
                                            _sent_value: JsValue::Undefined,
                                            try_stack: new_try_stack,
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: Some(e),
                                            pending_return: None,
                                        });
                                    return self
                                        .generator_next_state_machine(this, JsValue::Undefined);
                                }
                                obj_rc.borrow_mut().iterator_state =
                                    Some(IteratorState::StateMachineGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    });
                                return Completion::Throw(e);
                            }
                        };

                        let next_method = if let JsValue::Object(io) = &iterator {
                            match self.get_object_property(io.id, "next", &iterator) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => {
                                    // Route through try-stack
                                    if let Some(try_info) = current_try_stack.last()
                                        && let Some(catch_state) = try_info.catch_state
                                    {
                                        let new_try_stack = current_try_stack
                                            [..current_try_stack.len() - 1]
                                            .to_vec();
                                        obj_rc.borrow_mut().iterator_state =
                                            Some(IteratorState::StateMachineGenerator {
                                                state_machine: state_machine.clone(),
                                                func_env: func_env.clone(),
                                                is_strict,
                                                execution_state:
                                                    StateMachineExecutionState::SuspendedAtState {
                                                        state_id: catch_state,
                                                    },
                                                _sent_value: JsValue::Undefined,
                                                try_stack: new_try_stack,
                                                pending_binding: None,
                                                delegated_iterator: None,
                                                pending_exception: Some(e),
                                                pending_return: None,
                                            });
                                        return self.generator_next_state_machine(
                                            this,
                                            JsValue::Undefined,
                                        );
                                    }
                                    return Completion::Throw(e);
                                }
                                _ => JsValue::Undefined,
                            }
                        } else {
                            JsValue::Undefined
                        };

                        let iter_result = match self.call_function(
                            &next_method,
                            &iterator,
                            &[JsValue::Undefined],
                        ) {
                            Completion::Normal(v) if matches!(v, JsValue::Object(_)) => Ok(v),
                            Completion::Normal(_) => {
                                Err(self.create_type_error("Iterator result is not an object"))
                            }
                            Completion::Throw(e) => Err(e),
                            _ => Err(self.create_type_error("Iterator next failed")),
                        };
                        let iter_result = match iter_result {
                            Ok(r) => r,
                            Err(e) => {
                                // Propagate through generator's try-stack
                                if let Some(try_info) = current_try_stack.last()
                                    && let Some(catch_state) = try_info.catch_state
                                {
                                    pending_exception = Some(e);
                                    let new_try_stack =
                                        current_try_stack[..current_try_stack.len() - 1].to_vec();
                                    obj_rc.borrow_mut().iterator_state =
                                        Some(IteratorState::StateMachineGenerator {
                                            state_machine: state_machine.clone(),
                                            func_env: func_env.clone(),
                                            is_strict,
                                            execution_state:
                                                StateMachineExecutionState::SuspendedAtState {
                                                    state_id: catch_state,
                                                },
                                            _sent_value: JsValue::Undefined,
                                            try_stack: new_try_stack,
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: pending_exception.take(),
                                            pending_return: None,
                                        });
                                    return self
                                        .generator_next_state_machine(this, JsValue::Undefined);
                                }
                                obj_rc.borrow_mut().iterator_state =
                                    Some(IteratorState::StateMachineGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    });
                                return Completion::Throw(e);
                            }
                        };

                        let done = match self.iterator_complete(&iter_result) {
                            Ok(d) => d,
                            Err(e) => {
                                if let Some(try_info) = current_try_stack.last()
                                    && let Some(catch_state) = try_info.catch_state
                                {
                                    let new_try_stack =
                                        current_try_stack[..current_try_stack.len() - 1].to_vec();
                                    obj_rc.borrow_mut().iterator_state =
                                        Some(IteratorState::StateMachineGenerator {
                                            state_machine: state_machine.clone(),
                                            func_env: func_env.clone(),
                                            is_strict,
                                            execution_state:
                                                StateMachineExecutionState::SuspendedAtState {
                                                    state_id: catch_state,
                                                },
                                            _sent_value: JsValue::Undefined,
                                            try_stack: new_try_stack,
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: Some(e),
                                            pending_return: None,
                                        });
                                    return self
                                        .generator_next_state_machine(this, JsValue::Undefined);
                                }
                                return Completion::Throw(e);
                            }
                        };

                        if done {
                            let value = match self.iterator_value(&iter_result) {
                                Ok(v) => v,
                                Err(e) => {
                                    if let Some(try_info) = current_try_stack.last()
                                        && let Some(catch_state) = try_info.catch_state
                                    {
                                        let new_try_stack = current_try_stack
                                            [..current_try_stack.len() - 1]
                                            .to_vec();
                                        obj_rc.borrow_mut().iterator_state =
                                            Some(IteratorState::StateMachineGenerator {
                                                state_machine: state_machine.clone(),
                                                func_env: func_env.clone(),
                                                is_strict,
                                                execution_state:
                                                    StateMachineExecutionState::SuspendedAtState {
                                                        state_id: catch_state,
                                                    },
                                                _sent_value: JsValue::Undefined,
                                                try_stack: new_try_stack,
                                                pending_binding: None,
                                                delegated_iterator: None,
                                                pending_exception: Some(e),
                                                pending_return: None,
                                            });
                                        return self.generator_next_state_machine(
                                            this,
                                            JsValue::Undefined,
                                        );
                                    }
                                    return Completion::Throw(e);
                                }
                            };
                            use crate::interpreter::generator_transform::SentValueBindingKind;
                            if let Some(binding) = sent_value_binding {
                                match &binding.kind {
                                    SentValueBindingKind::Variable(name) => {
                                        func_env.borrow_mut().set(name, value.clone()).ok();
                                    }
                                    SentValueBindingKind::Pattern(pattern) => {
                                        let _ = self.bind_pattern(
                                            pattern,
                                            value.clone(),
                                            BindingKind::Var,
                                            &func_env,
                                        );
                                    }
                                    SentValueBindingKind::Discard
                                    | SentValueBindingKind::InlineYield { .. } => {}
                                }
                            }
                            current_id = *resume_state;
                            continue;
                        } else {
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::SuspendedAtState {
                                        state_id: *resume_state,
                                    },
                                    _sent_value: JsValue::Undefined,
                                    try_stack: current_try_stack,
                                    pending_binding: None,
                                    delegated_iterator: Some(
                                        crate::interpreter::types::DelegatedIteratorInfo {
                                            iterator,
                                            next_method: next_method.clone(),
                                            resume_state: *resume_state,
                                            sent_value_binding: sent_value_binding.clone(),
                                        },
                                    ),
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            // Per spec §14.4.14: yield innerResult directly (don't extract value)
                            return Completion::Normal(iter_result);
                        }
                    }

                    obj_rc.borrow_mut().iterator_state =
                        Some(IteratorState::StateMachineGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::SuspendedAtState {
                                state_id: *resume_state,
                            },
                            _sent_value: JsValue::Undefined,
                            try_stack: current_try_stack,
                            pending_binding: sent_value_binding.clone(),
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        });
                    return Completion::Normal(self.create_iter_result_object(yield_val, false));
                }

                StateTerminator::Return(expr) => {
                    let ret_val = if let Some(e) = expr {
                        match self.eval_expr(e, &func_env) {
                            Completion::Normal(v) => v,
                            Completion::Throw(err) => {
                                obj_rc.borrow_mut().iterator_state =
                                    Some(IteratorState::StateMachineGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    });
                                self.generator_inline_iters.remove(&o.id);
                                return Completion::Throw(err);
                            }
                            other => return other,
                        }
                    } else {
                        JsValue::Undefined
                    };

                    obj_rc.borrow_mut().iterator_state =
                        Some(IteratorState::StateMachineGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            _sent_value: JsValue::Undefined,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        });
                    self.generator_inline_iters.remove(&o.id);
                    return Completion::Normal(self.create_iter_result_object(ret_val, true));
                }

                StateTerminator::Throw(expr) => {
                    let throw_val = match self.eval_expr(expr, &func_env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => e,
                        other => return other,
                    };

                    if let Some(try_info) = current_try_stack.pop()
                        && let Some(catch_state) = try_info.catch_state
                    {
                        current_id = catch_state;
                        continue;
                    }

                    obj_rc.borrow_mut().iterator_state =
                        Some(IteratorState::StateMachineGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            _sent_value: JsValue::Undefined,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        });
                    self.generator_inline_iters.remove(&o.id);
                    return Completion::Throw(throw_val);
                }

                StateTerminator::Goto(next_state) => {
                    current_id = *next_state;
                }

                StateTerminator::ConditionalGoto {
                    condition,
                    true_state,
                    false_state,
                } => {
                    let cond_val = match self.eval_expr(condition, &func_env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::Completed,
                                    _sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            return Completion::Throw(e);
                        }
                        other => return other,
                    };
                    current_id = if self.to_boolean_val(&cond_val) {
                        *true_state
                    } else {
                        *false_state
                    };
                }

                StateTerminator::TryEnter {
                    try_state,
                    catch_state,
                    finally_state,
                    after_state,
                } => {
                    current_try_stack.push(TryContextInfo {
                        catch_state: catch_state.as_ref().map(|c| c.state),
                        finally_state: *finally_state,
                        _after_state: *after_state,
                        entered_catch: false,
                        entered_finally: false,
                    });
                    current_id = *try_state;
                }

                StateTerminator::TryExit { after_state } => {
                    current_try_stack.pop();
                    if let Some(ret_val) = pending_return.take() {
                        if current_try_stack.is_empty() {
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::Completed,
                                    _sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            return Completion::Normal(
                                self.create_iter_result_object(ret_val, true),
                            );
                        }
                        pending_return = Some(ret_val);
                    }
                    current_id = *after_state;
                }

                StateTerminator::EnterCatch { body_state, param } => {
                    if let Some(ctx) = current_try_stack.last_mut() {
                        ctx.entered_catch = true;
                    }
                    if let Some(pattern) = param {
                        let exception_val = pending_exception.take().unwrap_or(JsValue::Undefined);
                        let _ =
                            self.bind_pattern(pattern, exception_val, BindingKind::Let, &func_env);
                    }
                    current_id = *body_state;
                }

                StateTerminator::EnterFinally { body_state } => {
                    if let Some(ctx) = current_try_stack.last_mut() {
                        ctx.entered_finally = true;
                    }
                    current_id = *body_state;
                }

                StateTerminator::SwitchDispatch {
                    discriminant,
                    cases,
                    default_state,
                    after_state,
                } => {
                    let disc_val = match self.eval_expr(discriminant, &func_env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::Completed,
                                    _sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            return Completion::Throw(e);
                        }
                        other => return other,
                    };

                    let mut matched = false;
                    for case in cases {
                        let case_val = match self.eval_expr(&case.test, &func_env) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                obj_rc.borrow_mut().iterator_state =
                                    Some(IteratorState::StateMachineGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    });
                                return Completion::Throw(e);
                            }
                            other => return other,
                        };
                        if strict_equality(&disc_val, &case_val) {
                            current_id = case.state;
                            matched = true;
                            break;
                        }
                    }
                    if !matched {
                        current_id = default_state.unwrap_or(*after_state);
                    }
                }

                StateTerminator::Completed => {
                    obj_rc.borrow_mut().iterator_state =
                        Some(IteratorState::StateMachineGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            _sent_value: JsValue::Undefined,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        });
                    return Completion::Normal(
                        self.create_iter_result_object(JsValue::Undefined, true),
                    );
                }
            }
        }
    }

    pub(crate) fn generator_return_state_machine(
        &mut self,
        this: &JsValue,
        value: JsValue,
    ) -> Completion {
        let JsValue::Object(o) = this else {
            return Completion::Throw(
                self.create_type_error("Generator.prototype.return called on non-object"),
            );
        };
        let Some(obj_rc) = self.get_object(o.id) else {
            return Completion::Throw(
                self.create_type_error("Generator.prototype.return called on non-object"),
            );
        };

        let state = obj_rc.borrow().iterator_state.clone();
        if let Some(IteratorState::StateMachineGenerator {
            state_machine,
            func_env,
            is_strict,
            try_stack,
            delegated_iterator,
            ..
        }) = state
        {
            if let Some(ref deleg_info) = delegated_iterator {
                let iterator = deleg_info.iterator.clone();
                let next_method = deleg_info.next_method.clone();
                let resume_state = deleg_info.resume_state;
                let binding = deleg_info.sent_value_binding.clone();

                match self.iterator_return(&iterator, &value) {
                    Ok(Some(iter_result)) => {
                        let done = match self.iterator_complete(&iter_result) {
                            Ok(d) => d,
                            Err(e) => return Completion::Throw(e),
                        };
                        if done {
                            let result_value = match self.iterator_value(&iter_result) {
                                Ok(v) => v,
                                Err(e) => return Completion::Throw(e),
                            };
                            // Clear delegation and propagate return through
                            // generator's try-finally stack
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineGenerator {
                                    state_machine: state_machine.clone(),
                                    func_env: func_env.clone(),
                                    is_strict,
                                    execution_state: StateMachineExecutionState::SuspendedAtState {
                                        state_id: resume_state,
                                    },
                                    _sent_value: JsValue::Undefined,
                                    try_stack: try_stack.clone(),
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            return self.generator_return_state_machine(this, result_value);
                        } else {
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::SuspendedAtState {
                                        state_id: resume_state,
                                    },
                                    _sent_value: JsValue::Undefined,
                                    try_stack: try_stack.clone(),
                                    pending_binding: None,
                                    delegated_iterator: Some(
                                        crate::interpreter::types::DelegatedIteratorInfo {
                                            iterator,
                                            next_method: next_method.clone(),
                                            resume_state,
                                            sent_value_binding: binding,
                                        },
                                    ),
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            // Per spec §14.4.14: yield innerReturnResult directly
                            return Completion::Normal(iter_result);
                        }
                    }
                    Ok(None) => {
                        // Per spec 14.4.14 step 5.c.iii: "If return is undefined,
                        // return Completion(received)." — clear the delegation and
                        // propagate the return through the generator's own body
                        // (which may have try-finally).
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineGenerator {
                                state_machine: state_machine.clone(),
                                func_env: func_env.clone(),
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: resume_state,
                                },
                                _sent_value: JsValue::Undefined,
                                try_stack: try_stack.clone(),
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            });
                        return self.generator_return_state_machine(this, value);
                    }
                    Err(e) => {
                        // Propagate error through generator's try-catch
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineGenerator {
                                state_machine: state_machine.clone(),
                                func_env: func_env.clone(),
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: resume_state,
                                },
                                _sent_value: JsValue::Undefined,
                                try_stack: try_stack.clone(),
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            });
                        return self.generator_throw_state_machine(this, e);
                    }
                }
            }

            if let Some(try_info) = try_stack.last()
                && !try_info.entered_finally
                && let Some(finally_state) = try_info.finally_state
            {
                obj_rc.borrow_mut().iterator_state = Some(IteratorState::StateMachineGenerator {
                    state_machine,
                    func_env,
                    is_strict,
                    execution_state: StateMachineExecutionState::SuspendedAtState {
                        state_id: finally_state,
                    },
                    _sent_value: JsValue::Undefined,
                    try_stack: try_stack[..try_stack.len() - 1].to_vec(),
                    pending_binding: None,
                    delegated_iterator: None,
                    pending_exception: None,
                    pending_return: Some(value.clone()),
                });
                return self.generator_next_state_machine(this, JsValue::Undefined);
            }

            obj_rc.borrow_mut().iterator_state = Some(IteratorState::StateMachineGenerator {
                state_machine,
                func_env,
                is_strict,
                execution_state: StateMachineExecutionState::Completed,
                _sent_value: JsValue::Undefined,
                try_stack: vec![],
                pending_binding: None,
                delegated_iterator: None,
                pending_exception: None,
                pending_return: None,
            });
            // Close any iterators that were open when generator was suspended via InlineYield
            if let Some(iters) = self.generator_inline_iters.remove(&o.id) {
                for iter in iters {
                    if let Err(e) = self.iterator_close_result(&iter) {
                        return Completion::Throw(e);
                    }
                }
            }
        }
        Completion::Normal(self.create_iter_result_object(value, true))
    }

    pub(crate) fn generator_throw_state_machine(
        &mut self,
        this: &JsValue,
        exception: JsValue,
    ) -> Completion {
        let JsValue::Object(o) = this else {
            return Completion::Throw(
                self.create_type_error("Generator.prototype.throw called on non-object"),
            );
        };
        let Some(obj_rc) = self.get_object(o.id) else {
            return Completion::Throw(
                self.create_type_error("Generator.prototype.throw called on non-object"),
            );
        };

        let state = obj_rc.borrow().iterator_state.clone();
        if let Some(IteratorState::StateMachineGenerator {
            state_machine,
            func_env,
            is_strict,
            try_stack,
            delegated_iterator,
            ..
        }) = state
        {
            if let Some(ref deleg_info) = delegated_iterator {
                let iterator = deleg_info.iterator.clone();
                let next_method = deleg_info.next_method.clone();
                let resume_state = deleg_info.resume_state;
                let binding = deleg_info.sent_value_binding.clone();

                match self.iterator_throw(&iterator, &exception) {
                    Ok(Some(iter_result)) => {
                        let done = match self.iterator_complete(&iter_result) {
                            Ok(d) => d,
                            Err(e) => return Completion::Throw(e),
                        };
                        if done {
                            let result_value = match self.iterator_value(&iter_result) {
                                Ok(v) => v,
                                Err(e) => return Completion::Throw(e),
                            };
                            use crate::interpreter::generator_transform::SentValueBindingKind;
                            if let Some(ref bind) = binding {
                                match &bind.kind {
                                    SentValueBindingKind::Variable(name) => {
                                        func_env.borrow_mut().set(name, result_value.clone()).ok();
                                    }
                                    SentValueBindingKind::Pattern(pattern) => {
                                        let _ = self.bind_pattern(
                                            pattern,
                                            result_value.clone(),
                                            BindingKind::Var,
                                            &func_env,
                                        );
                                    }
                                    SentValueBindingKind::Discard
                                    | SentValueBindingKind::InlineYield { .. } => {}
                                }
                            }
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineGenerator {
                                    state_machine: state_machine.clone(),
                                    func_env: func_env.clone(),
                                    is_strict,
                                    execution_state: StateMachineExecutionState::SuspendedAtState {
                                        state_id: resume_state,
                                    },
                                    _sent_value: JsValue::Undefined,
                                    try_stack: try_stack.clone(),
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            return self.generator_next_state_machine(this, JsValue::Undefined);
                        } else {
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::SuspendedAtState {
                                        state_id: resume_state,
                                    },
                                    _sent_value: JsValue::Undefined,
                                    try_stack: try_stack.clone(),
                                    pending_binding: None,
                                    delegated_iterator: Some(
                                        crate::interpreter::types::DelegatedIteratorInfo {
                                            iterator,
                                            next_method: next_method.clone(),
                                            resume_state,
                                            sent_value_binding: binding,
                                        },
                                    ),
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            // Per spec §14.4.14: yield innerResult directly
                            return Completion::Normal(iter_result);
                        }
                    }
                    Ok(None) => {
                        // Per §14.4.14 step 5.b.iii: close iterator with normal
                        // completion, then throw TypeError (yield* protocol violation)
                        if let Err(e) = self.iterator_close_result(&iterator) {
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::Completed,
                                    _sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            return Completion::Throw(e);
                        }
                        let type_err = self
                            .create_type_error("The iterator does not provide a 'throw' method");
                        // Clear delegation and propagate throw through generator body
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineGenerator {
                                state_machine: state_machine.clone(),
                                func_env: func_env.clone(),
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: resume_state,
                                },
                                _sent_value: JsValue::Undefined,
                                try_stack: try_stack.clone(),
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            });
                        return self.generator_throw_state_machine(this, type_err);
                    }
                    Err(e) => {
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::Undefined,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            });
                        return Completion::Throw(e);
                    }
                }
            }

            if let Some(try_info) = try_stack.last() {
                if !try_info.entered_catch
                    && !try_info.entered_finally
                    && let Some(catch_state) = try_info.catch_state
                {
                    obj_rc.borrow_mut().iterator_state =
                        Some(IteratorState::StateMachineGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::SuspendedAtState {
                                state_id: catch_state,
                            },
                            _sent_value: JsValue::Undefined,
                            try_stack: try_stack[..try_stack.len() - 1].to_vec(),
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: Some(exception.clone()),
                            pending_return: None,
                        });
                    return self.generator_next_state_machine(this, JsValue::Undefined);
                }
                if !try_info.entered_finally
                    && let Some(finally_state) = try_info.finally_state
                {
                    obj_rc.borrow_mut().iterator_state =
                        Some(IteratorState::StateMachineGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::SuspendedAtState {
                                state_id: finally_state,
                            },
                            _sent_value: JsValue::Undefined,
                            try_stack: try_stack[..try_stack.len() - 1].to_vec(),
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: Some(exception.clone()),
                            pending_return: None,
                        });
                    return self.generator_next_state_machine(this, JsValue::Undefined);
                }
            }

            obj_rc.borrow_mut().iterator_state = Some(IteratorState::StateMachineGenerator {
                state_machine,
                func_env,
                is_strict,
                execution_state: StateMachineExecutionState::Completed,
                _sent_value: JsValue::Undefined,
                try_stack: vec![],
                pending_binding: None,
                delegated_iterator: None,
                pending_exception: None,
                pending_return: None,
            });
        }
        Completion::Throw(exception)
    }

    fn reject_with_type_error(&mut self, msg: &str) -> Completion {
        let promise = self.create_promise_object();
        let promise_id = if let JsValue::Object(ref po) = promise {
            po.id
        } else {
            0
        };
        let (_resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);
        let err = self.create_type_error(msg);
        let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[err]);
        self.drain_microtasks();
        Completion::Normal(promise)
    }

    fn async_generator_next_state_machine(
        &mut self,
        this: &JsValue,
        sent_value: JsValue,
    ) -> Completion {
        use crate::interpreter::generator_transform::StateTerminator;

        let JsValue::Object(o) = this else {
            return self
                .reject_with_type_error("AsyncGenerator.prototype.next called on non-object");
        };
        let Some(obj_rc) = self.get_object(o.id) else {
            return self
                .reject_with_type_error("AsyncGenerator.prototype.next called on non-object");
        };

        let state = obj_rc.borrow().iterator_state.clone();
        let Some(IteratorState::StateMachineAsyncGenerator {
            state_machine,
            func_env,
            is_strict,
            execution_state,
            try_stack,
            pending_binding,
            delegated_iterator,
            pending_exception: stored_pending_exception,
            pending_return: stored_pending_return,
            ..
        }) = state
        else {
            return self.reject_with_type_error("not a state machine async generator");
        };

        let promise = self.create_promise_object();
        let promise_id = if let JsValue::Object(ref po) = promise {
            po.id
        } else {
            0
        };
        let (resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);

        if let Some(ref deleg_info) = delegated_iterator {
            let iterator = deleg_info.iterator.clone();
            let next_method = deleg_info.next_method.clone();
            let resume_state = deleg_info.resume_state;
            let binding = deleg_info.sent_value_binding.clone();

            let result = match self.call_function(
                &next_method,
                &iterator,
                std::slice::from_ref(&sent_value),
            ) {
                Completion::Normal(v) if matches!(v, JsValue::Object(_)) => Ok(v),
                Completion::Normal(_) => {
                    Err(self.create_type_error("Iterator result is not an object"))
                }
                Completion::Throw(e) => Err(e),
                _ => Err(self.create_type_error("Iterator next failed")),
            };
            match result {
                Ok(iter_result) => {
                    // Await the iterator result (inner async iterators return promises)
                    let awaited_result = match self.await_value(&iter_result) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineAsyncGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::Completed,
                                    _sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                        _ => iter_result,
                    };
                    let done = match self.iterator_complete(&awaited_result) {
                        Ok(d) => d,
                        Err(e) => {
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineAsyncGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::Completed,
                                    _sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                    };
                    let value = match self.iterator_value(&awaited_result) {
                        Ok(v) => v,
                        Err(e) => {
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineAsyncGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::Completed,
                                    _sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                    };
                    if done {
                        if let Some(ref bind) = binding {
                            use crate::interpreter::generator_transform::SentValueBindingKind;
                            match &bind.kind {
                                SentValueBindingKind::Variable(name) => {
                                    func_env.borrow_mut().set(name, value.clone()).ok();
                                }
                                SentValueBindingKind::Pattern(pattern) => {
                                    let _ = self.bind_pattern(
                                        pattern,
                                        value.clone(),
                                        BindingKind::Var,
                                        &func_env,
                                    );
                                }
                                SentValueBindingKind::Discard
                                | SentValueBindingKind::InlineYield { .. } => {}
                            }
                        }
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineAsyncGenerator {
                                state_machine: state_machine.clone(),
                                func_env: func_env.clone(),
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: resume_state,
                                },
                                _sent_value: JsValue::Undefined,
                                try_stack: try_stack.clone(),
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            });
                        return self.async_generator_next_state_machine(this, JsValue::Undefined);
                    } else {
                        let awaited_value = match self.await_value(&value) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                obj_rc.borrow_mut().iterator_state =
                                    Some(IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    });
                                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                            _ => value.clone(),
                        };
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: resume_state,
                                },
                                _sent_value: JsValue::Undefined,
                                try_stack,
                                pending_binding: None,
                                delegated_iterator: Some(
                                    crate::interpreter::types::DelegatedIteratorInfo {
                                        iterator,
                                        next_method: next_method.clone(),
                                        resume_state,
                                        sent_value_binding: binding,
                                    },
                                ),
                                pending_exception: None,
                                pending_return: None,
                            });
                        let iter_result = self.create_iter_result_object(awaited_value, false);
                        let _ =
                            self.call_function(&resolve_fn, &JsValue::Undefined, &[iter_result]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }
                }
                Err(e) => {
                    obj_rc.borrow_mut().iterator_state =
                        Some(IteratorState::StateMachineAsyncGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            _sent_value: JsValue::Undefined,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        });
                    let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                    self.drain_microtasks();
                    return Completion::Normal(promise);
                }
            }
        }

        let current_state_id = match &execution_state {
            StateMachineExecutionState::Completed => {
                let result = self.create_iter_result_object(JsValue::Undefined, true);
                let _ = self.call_function(&resolve_fn, &JsValue::Undefined, &[result]);
                self.drain_microtasks();
                return Completion::Normal(promise);
            }
            StateMachineExecutionState::Executing => {
                let err = self.create_type_error("AsyncGenerator is already executing");
                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[err]);
                self.drain_microtasks();
                return Completion::Normal(promise);
            }
            StateMachineExecutionState::SuspendedStart => 0,
            StateMachineExecutionState::SuspendedAtState { state_id } => *state_id,
        };

        obj_rc.borrow_mut().iterator_state = Some(IteratorState::StateMachineAsyncGenerator {
            state_machine: state_machine.clone(),
            func_env: func_env.clone(),
            is_strict,
            execution_state: StateMachineExecutionState::Executing,
            _sent_value: sent_value.clone(),
            try_stack: try_stack.clone(),
            pending_binding: None,
            delegated_iterator: None,
            pending_exception: None,
            pending_return: None,
        });

        use crate::interpreter::generator_transform::SentValueBindingKind;
        let mut initial_inline_yield_target: Option<usize> = None;
        let mut initial_inline_yield_sent: Option<JsValue> = None;
        if let Some(binding) = pending_binding {
            match &binding.kind {
                SentValueBindingKind::Variable(name) => {
                    func_env.borrow_mut().set(name, sent_value.clone()).ok();
                }
                SentValueBindingKind::Pattern(pattern) => {
                    let _ =
                        self.bind_pattern(pattern, sent_value.clone(), BindingKind::Var, &func_env);
                }
                SentValueBindingKind::Discard => {}
                SentValueBindingKind::InlineYield { yield_target } => {
                    initial_inline_yield_target = Some(*yield_target);
                    initial_inline_yield_sent = Some(sent_value.clone());
                }
            }
        }

        func_env.borrow_mut().strict = is_strict;
        let mut current_id = current_state_id;
        let mut current_try_stack = try_stack;
        let mut pending_exception: Option<JsValue> = stored_pending_exception;
        let mut pending_return: Option<JsValue> = stored_pending_return;
        let mut inline_yield_target: Option<usize> = initial_inline_yield_target;
        let mut inline_yield_sent: Option<JsValue> = initial_inline_yield_sent;

        loop {
            let (statements, terminator) = {
                let gen_state = &state_machine.states[current_id];
                (gen_state.statements.clone(), gen_state.terminator.clone())
            };

            let is_inline_replay = inline_yield_target.is_some();
            if let Some(target) = inline_yield_target.take() {
                let sv = inline_yield_sent.take().unwrap_or(JsValue::Undefined);
                self.generator_context = Some(GeneratorContext {
                    target_yield: target,
                    current_yield: 0,
                    sent_value: sv,
                    is_async: true,
                });
            }

            let stmt_result = self.exec_statements(&statements, &func_env);
            let ctx_after = if is_inline_replay {
                self.generator_context.take()
            } else {
                None
            };

            if let Completion::Throw(e) = stmt_result {
                if let Some(try_info) = current_try_stack.pop() {
                    if let Some(catch_state) = try_info.catch_state {
                        pending_exception = Some(e);
                        current_id = catch_state;
                        continue;
                    } else if let Some(finally_state) = try_info.finally_state {
                        current_id = finally_state;
                        continue;
                    }
                }
                obj_rc.borrow_mut().iterator_state =
                    Some(IteratorState::StateMachineAsyncGenerator {
                        state_machine,
                        func_env,
                        is_strict,
                        execution_state: StateMachineExecutionState::Completed,
                        _sent_value: JsValue::Undefined,
                        try_stack: vec![],
                        pending_binding: None,
                        delegated_iterator: None,
                        pending_exception: None,
                        pending_return: None,
                    });
                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                self.drain_microtasks();
                return Completion::Normal(promise);
            }
            if let Completion::Return(v) = stmt_result {
                let awaited = match self.await_value(&v) {
                    Completion::Normal(av) => av,
                    Completion::Throw(e) => {
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::Undefined,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            });
                        let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }
                    _ => JsValue::Undefined,
                };
                obj_rc.borrow_mut().iterator_state =
                    Some(IteratorState::StateMachineAsyncGenerator {
                        state_machine,
                        func_env,
                        is_strict,
                        execution_state: StateMachineExecutionState::Completed,
                        _sent_value: JsValue::Undefined,
                        try_stack: vec![],
                        pending_binding: None,
                        delegated_iterator: None,
                        pending_exception: None,
                        pending_return: None,
                    });
                let iter_result = self.create_iter_result_object(awaited, true);
                let _ = self.call_function(&resolve_fn, &JsValue::Undefined, &[iter_result]);
                self.drain_microtasks();
                return Completion::Normal(promise);
            }
            if let Completion::Yield(yield_val) = stmt_result {
                let is_destructuring = self.destructuring_yield;
                self.destructuring_yield = false;
                let awaited_val = match self.await_value(&yield_val) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => {
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::Undefined,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            });
                        let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }
                    _ => yield_val,
                };
                // Any Completion::Yield from exec_statements is an inline yield:
                // it came from a loop body or complex control flow that isn't
                // decomposed by the state machine transformer. Use InlineYield
                // to re-enter the same state and fast-forward past previous yields.
                {
                    let yield_count = ctx_after.map(|c| c.current_yield).unwrap_or(1);
                    obj_rc.borrow_mut().iterator_state =
                        Some(IteratorState::StateMachineAsyncGenerator {
                            state_machine: state_machine.clone(),
                            func_env: func_env.clone(),
                            is_strict,
                            execution_state: StateMachineExecutionState::SuspendedAtState {
                                state_id: current_id,
                            },
                            _sent_value: JsValue::Undefined,
                            try_stack: current_try_stack.clone(),
                            pending_binding: Some(
                                crate::interpreter::generator_transform::SentValueBinding {
                                    kind: SentValueBindingKind::InlineYield {
                                        yield_target: yield_count,
                                    },
                                },
                            ),
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        });
                }
                let iter_result = self.create_iter_result_object(awaited_val, false);
                let _ = self.call_function(&resolve_fn, &JsValue::Undefined, &[iter_result]);
                self.drain_microtasks();
                return Completion::Normal(promise);
            }

            match &terminator {
                StateTerminator::Yield {
                    value,
                    is_delegate,
                    resume_state,
                    sent_value_binding,
                } => {
                    let yield_val = if let Some(expr) = value {
                        match self.eval_expr(expr, &func_env) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                obj_rc.borrow_mut().iterator_state =
                                    Some(IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    });
                                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                            other => {
                                if let Completion::Yield(yv) = other {
                                    yv
                                } else {
                                    JsValue::Undefined
                                }
                            }
                        }
                    } else {
                        JsValue::Undefined
                    };

                    if *is_delegate {
                        let iterator = match self.get_async_iterator(&yield_val) {
                            Ok(it) => it,
                            Err(e) => match self.get_iterator(&yield_val) {
                                Ok(it) => it,
                                Err(_) => {
                                    obj_rc.borrow_mut().iterator_state =
                                        Some(IteratorState::StateMachineAsyncGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::Undefined,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        });
                                    let _ =
                                        self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                    self.drain_microtasks();
                                    return Completion::Normal(promise);
                                }
                            },
                        };

                        let next_method = if let JsValue::Object(io) = &iterator {
                            match self.get_object_property(io.id, "next", &iterator) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => {
                                    obj_rc.borrow_mut().iterator_state =
                                        Some(IteratorState::StateMachineAsyncGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::Undefined,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        });
                                    let _ =
                                        self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                    self.drain_microtasks();
                                    return Completion::Normal(promise);
                                }
                                _ => JsValue::Undefined,
                            }
                        } else {
                            JsValue::Undefined
                        };

                        let iter_result = match self.call_function(
                            &next_method,
                            &iterator,
                            &[JsValue::Undefined],
                        ) {
                            Completion::Normal(v) if matches!(v, JsValue::Object(_)) => Ok(v),
                            Completion::Normal(_) => {
                                Err(self.create_type_error("Iterator result is not an object"))
                            }
                            Completion::Throw(e) => Err(e),
                            _ => Err(self.create_type_error("Iterator next failed")),
                        };
                        let iter_result = match iter_result {
                            Ok(r) => r,
                            Err(e) => {
                                obj_rc.borrow_mut().iterator_state =
                                    Some(IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    });
                                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                        };

                        let awaited_result = match self.await_value(&iter_result) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                obj_rc.borrow_mut().iterator_state =
                                    Some(IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    });
                                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                            _ => iter_result,
                        };

                        let done = match self.iterator_complete(&awaited_result) {
                            Ok(d) => d,
                            Err(e) => {
                                obj_rc.borrow_mut().iterator_state =
                                    Some(IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    });
                                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                        };
                        let value = match self.iterator_value(&awaited_result) {
                            Ok(v) => v,
                            Err(e) => {
                                obj_rc.borrow_mut().iterator_state =
                                    Some(IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    });
                                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise.clone());
                            }
                        };

                        if done {
                            if let Some(binding) = sent_value_binding {
                                match &binding.kind {
                                    SentValueBindingKind::Variable(name) => {
                                        func_env.borrow_mut().set(name, value.clone()).ok();
                                    }
                                    SentValueBindingKind::Pattern(pattern) => {
                                        let _ = self.bind_pattern(
                                            pattern,
                                            value.clone(),
                                            BindingKind::Var,
                                            &func_env,
                                        );
                                    }
                                    SentValueBindingKind::Discard
                                    | SentValueBindingKind::InlineYield { .. } => {}
                                }
                            }
                            current_id = *resume_state;
                            continue;
                        } else {
                            // For async generator yield*, await the value before yielding
                            let awaited_value = match self.await_value(&value) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => {
                                    obj_rc.borrow_mut().iterator_state =
                                        Some(IteratorState::StateMachineAsyncGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::Undefined,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        });
                                    let _ =
                                        self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                    self.drain_microtasks();
                                    return Completion::Normal(promise);
                                }
                                _ => value,
                            };
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineAsyncGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::SuspendedAtState {
                                        state_id: *resume_state,
                                    },
                                    _sent_value: JsValue::Undefined,
                                    try_stack: current_try_stack,
                                    pending_binding: None,
                                    delegated_iterator: Some(
                                        crate::interpreter::types::DelegatedIteratorInfo {
                                            iterator,
                                            next_method: next_method.clone(),
                                            resume_state: *resume_state,
                                            sent_value_binding: sent_value_binding.clone(),
                                        },
                                    ),
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            let iter_result = self.create_iter_result_object(awaited_value, false);
                            let _ = self.call_function(
                                &resolve_fn,
                                &JsValue::Undefined,
                                &[iter_result],
                            );
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                    }

                    let awaited_val = match self.await_value(&yield_val) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineAsyncGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::Completed,
                                    _sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                        _ => yield_val,
                    };

                    obj_rc.borrow_mut().iterator_state =
                        Some(IteratorState::StateMachineAsyncGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::SuspendedAtState {
                                state_id: *resume_state,
                            },
                            _sent_value: JsValue::Undefined,
                            try_stack: current_try_stack,
                            pending_binding: sent_value_binding.clone(),
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        });
                    let iter_result = self.create_iter_result_object(awaited_val, false);
                    let _ = self.call_function(&resolve_fn, &JsValue::Undefined, &[iter_result]);
                    self.drain_microtasks();
                    return Completion::Normal(promise);
                }

                StateTerminator::Return(expr) => {
                    let ret_val = if let Some(e) = expr {
                        match self.eval_expr(e, &func_env) {
                            Completion::Normal(v) => v,
                            Completion::Throw(err) => {
                                obj_rc.borrow_mut().iterator_state =
                                    Some(IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    });
                                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[err]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                            other => {
                                if let Completion::Yield(yv) = other {
                                    yv
                                } else {
                                    JsValue::Undefined
                                }
                            }
                        }
                    } else {
                        JsValue::Undefined
                    };

                    let awaited = match self.await_value(&ret_val) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineAsyncGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::Completed,
                                    _sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                        _ => ret_val,
                    };

                    obj_rc.borrow_mut().iterator_state =
                        Some(IteratorState::StateMachineAsyncGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            _sent_value: JsValue::Undefined,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        });
                    let iter_result = self.create_iter_result_object(awaited, true);
                    let _ = self.call_function(&resolve_fn, &JsValue::Undefined, &[iter_result]);
                    self.drain_microtasks();
                    return Completion::Normal(promise);
                }

                StateTerminator::Throw(expr) => {
                    let throw_val = match self.eval_expr(expr, &func_env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => e,
                        other => {
                            if let Completion::Yield(yv) = other {
                                yv
                            } else {
                                JsValue::Undefined
                            }
                        }
                    };

                    if let Some(try_info) = current_try_stack.pop()
                        && let Some(catch_state) = try_info.catch_state
                    {
                        pending_exception = Some(throw_val);
                        current_id = catch_state;
                        continue;
                    }

                    obj_rc.borrow_mut().iterator_state =
                        Some(IteratorState::StateMachineAsyncGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            _sent_value: JsValue::Undefined,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        });
                    let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[throw_val]);
                    self.drain_microtasks();
                    return Completion::Normal(promise);
                }

                StateTerminator::Goto(next_state) => {
                    current_id = *next_state;
                }

                StateTerminator::ConditionalGoto {
                    condition,
                    true_state,
                    false_state,
                } => {
                    let cond_val = match self.eval_expr(condition, &func_env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineAsyncGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::Completed,
                                    _sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                        other => {
                            if let Completion::Yield(yv) = other {
                                yv
                            } else {
                                JsValue::Undefined
                            }
                        }
                    };
                    current_id = if self.to_boolean_val(&cond_val) {
                        *true_state
                    } else {
                        *false_state
                    };
                }

                StateTerminator::TryEnter {
                    try_state,
                    catch_state,
                    finally_state,
                    after_state,
                } => {
                    current_try_stack.push(TryContextInfo {
                        catch_state: catch_state.as_ref().map(|c| c.state),
                        finally_state: *finally_state,
                        _after_state: *after_state,
                        entered_catch: false,
                        entered_finally: false,
                    });
                    current_id = *try_state;
                }

                StateTerminator::TryExit { after_state } => {
                    current_try_stack.pop();
                    if let Some(ret_val) = pending_return.take() {
                        if current_try_stack.is_empty() {
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineAsyncGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::Completed,
                                    _sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            let iter_result = self.create_iter_result_object(ret_val, true);
                            let _ = self.call_function(
                                &resolve_fn,
                                &JsValue::Undefined,
                                &[iter_result],
                            );
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                        pending_return = Some(ret_val);
                    }
                    current_id = *after_state;
                }

                StateTerminator::EnterCatch { body_state, param } => {
                    if let Some(ctx) = current_try_stack.last_mut() {
                        ctx.entered_catch = true;
                    }
                    if let Some(pattern) = param {
                        let exception_val = pending_exception.take().unwrap_or(JsValue::Undefined);
                        let _ =
                            self.bind_pattern(pattern, exception_val, BindingKind::Let, &func_env);
                    }
                    current_id = *body_state;
                }

                StateTerminator::EnterFinally { body_state } => {
                    if let Some(ctx) = current_try_stack.last_mut() {
                        ctx.entered_finally = true;
                    }
                    current_id = *body_state;
                }

                StateTerminator::SwitchDispatch {
                    discriminant,
                    cases,
                    default_state,
                    after_state,
                } => {
                    let disc_val = match self.eval_expr(discriminant, &func_env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineAsyncGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::Completed,
                                    _sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                        other => {
                            if let Completion::Yield(yv) = other {
                                yv
                            } else {
                                JsValue::Undefined
                            }
                        }
                    };

                    let mut matched = false;
                    for case in cases {
                        let case_val = match self.eval_expr(&case.test, &func_env) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                obj_rc.borrow_mut().iterator_state =
                                    Some(IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    });
                                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                            other => {
                                if let Completion::Yield(yv) = other {
                                    yv
                                } else {
                                    JsValue::Undefined
                                }
                            }
                        };
                        if strict_equality(&disc_val, &case_val) {
                            current_id = case.state;
                            matched = true;
                            break;
                        }
                    }
                    if !matched {
                        current_id = default_state.unwrap_or(*after_state);
                    }
                }

                StateTerminator::Completed => {
                    obj_rc.borrow_mut().iterator_state =
                        Some(IteratorState::StateMachineAsyncGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            _sent_value: JsValue::Undefined,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                            pending_return: None,
                        });
                    let iter_result = self.create_iter_result_object(JsValue::Undefined, true);
                    let _ = self.call_function(&resolve_fn, &JsValue::Undefined, &[iter_result]);
                    self.drain_microtasks();
                    return Completion::Normal(promise);
                }
            }
        }
    }

    pub(crate) fn async_generator_next(
        &mut self,
        this: &JsValue,
        sent_value: JsValue,
    ) -> Completion {
        let JsValue::Object(o) = this else {
            return self
                .reject_with_type_error("AsyncGenerator.prototype.next called on non-object");
        };
        let Some(obj_rc) = self.get_object(o.id) else {
            return self
                .reject_with_type_error("AsyncGenerator.prototype.next called on non-object");
        };

        let state = obj_rc.borrow().iterator_state.clone();
        if let Some(IteratorState::StateMachineAsyncGenerator { .. }) = &state {
            return self.async_generator_next_state_machine(this, sent_value);
        }
        let Some(IteratorState::AsyncGenerator {
            body,
            func_env,
            is_strict,
            execution_state,
        }) = state
        else {
            return self.reject_with_type_error("not an async generator object");
        };

        let promise = self.create_promise_object();
        let promise_id = if let JsValue::Object(ref po) = promise {
            po.id
        } else {
            0
        };
        let (resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);

        // Determine target_yield based on execution state
        let target_yield = match &execution_state {
            GeneratorExecutionState::Completed => {
                let result = self.create_iter_result_object(JsValue::Undefined, true);
                let _ = self.call_function(&resolve_fn, &JsValue::Undefined, &[result]);
                self.drain_microtasks();
                return Completion::Normal(promise);
            }
            GeneratorExecutionState::Executing => {
                let err = self.create_type_error("AsyncGenerator is already executing");
                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[err]);
                self.drain_microtasks();
                return Completion::Normal(promise);
            }
            GeneratorExecutionState::SuspendedStart => 0,
            GeneratorExecutionState::SuspendedYield { target_yield } => *target_yield,
        };

        // Mark as executing
        obj_rc.borrow_mut().iterator_state = Some(IteratorState::AsyncGenerator {
            body: body.clone(),
            func_env: func_env.clone(),
            is_strict,
            execution_state: GeneratorExecutionState::Executing,
        });

        self.generator_context = Some(GeneratorContext {
            target_yield,
            current_yield: 0,
            sent_value,
            is_async: true,
        });

        func_env.borrow_mut().strict = is_strict;
        self.call_stack_envs.push(func_env.clone());
        let result = self.exec_statements(&body, &func_env);
        self.call_stack_envs.pop();
        let _ctx = self.generator_context.take();

        match result {
            Completion::Yield(v) => {
                let awaited = match self.await_value(&v) {
                    Completion::Normal(av) => av,
                    Completion::Throw(e) => {
                        obj_rc.borrow_mut().iterator_state = Some(IteratorState::AsyncGenerator {
                            body,
                            func_env,
                            is_strict,
                            execution_state: GeneratorExecutionState::Completed,
                        });
                        let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }
                    other => {
                        if let Completion::Yield(yv) = other {
                            yv
                        } else {
                            JsValue::Undefined
                        }
                    }
                };
                obj_rc.borrow_mut().iterator_state = Some(IteratorState::AsyncGenerator {
                    body,
                    func_env,
                    is_strict,
                    execution_state: GeneratorExecutionState::SuspendedYield {
                        target_yield: target_yield + 1,
                    },
                });
                let iter_result = self.create_iter_result_object(awaited, false);
                let _ = self.call_function(&resolve_fn, &JsValue::Undefined, &[iter_result]);
            }
            Completion::Return(v) => {
                let awaited = match self.await_value(&v) {
                    Completion::Normal(av) => av,
                    Completion::Throw(e) => {
                        obj_rc.borrow_mut().iterator_state = Some(IteratorState::AsyncGenerator {
                            body,
                            func_env,
                            is_strict,
                            execution_state: GeneratorExecutionState::Completed,
                        });
                        let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }
                    other => {
                        if let Completion::Yield(yv) = other {
                            yv
                        } else {
                            JsValue::Undefined
                        }
                    }
                };
                obj_rc.borrow_mut().iterator_state = Some(IteratorState::AsyncGenerator {
                    body,
                    func_env,
                    is_strict,
                    execution_state: GeneratorExecutionState::Completed,
                });
                let iter_result = self.create_iter_result_object(awaited, true);
                let _ = self.call_function(&resolve_fn, &JsValue::Undefined, &[iter_result]);
            }
            Completion::Normal(_) => {
                obj_rc.borrow_mut().iterator_state = Some(IteratorState::AsyncGenerator {
                    body,
                    func_env,
                    is_strict,
                    execution_state: GeneratorExecutionState::Completed,
                });
                let iter_result = self.create_iter_result_object(JsValue::Undefined, true);
                let _ = self.call_function(&resolve_fn, &JsValue::Undefined, &[iter_result]);
            }
            Completion::Throw(e) => {
                obj_rc.borrow_mut().iterator_state = Some(IteratorState::AsyncGenerator {
                    body,
                    func_env,
                    is_strict,
                    execution_state: GeneratorExecutionState::Completed,
                });
                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
            }
            _ => {}
        }
        self.drain_microtasks();
        Completion::Normal(promise)
    }

    pub(crate) fn async_generator_return(&mut self, this: &JsValue, value: JsValue) -> Completion {
        let JsValue::Object(o) = this else {
            return self
                .reject_with_type_error("AsyncGenerator.prototype.return called on non-object");
        };

        let Some(obj_rc) = self.get_object(o.id) else {
            return self
                .reject_with_type_error("AsyncGenerator.prototype.return called on non-object");
        };
        let state = obj_rc.borrow().iterator_state.clone();

        if let Some(IteratorState::StateMachineAsyncGenerator {
            state_machine,
            func_env,
            is_strict,
            try_stack,
            delegated_iterator,
            ..
        }) = &state
        {
            let state_machine = state_machine.clone();
            let func_env = func_env.clone();
            let is_strict = *is_strict;
            let try_stack = try_stack.clone();
            let delegated_iterator = delegated_iterator.clone();

            let promise = self.create_promise_object();
            let promise_id = if let JsValue::Object(ref po) = promise {
                po.id
            } else {
                0
            };
            let (resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);

            if let Some(ref deleg_info) = delegated_iterator {
                let iterator = deleg_info.iterator.clone();
                let next_method = deleg_info.next_method.clone();
                let resume_state = deleg_info.resume_state;
                let binding = deleg_info.sent_value_binding.clone();

                match self.iterator_return(&iterator, &value) {
                    Ok(Some(iter_result)) => {
                        let awaited_result = match self.await_value(&iter_result) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                obj_rc.borrow_mut().iterator_state =
                                    Some(IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    });
                                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                            _ => iter_result,
                        };
                        let done = match self.iterator_complete(&awaited_result) {
                            Ok(d) => d,
                            Err(e) => {
                                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                        };
                        let result_value = match self.iterator_value(&awaited_result) {
                            Ok(v) => v,
                            Err(e) => {
                                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                        };
                        if done {
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineAsyncGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::Completed,
                                    _sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            let iter_result = self.create_iter_result_object(result_value, true);
                            let _ = self.call_function(
                                &resolve_fn,
                                &JsValue::Undefined,
                                &[iter_result],
                            );
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        } else {
                            let awaited_value = match self.await_value(&result_value) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => {
                                    obj_rc.borrow_mut().iterator_state =
                                        Some(IteratorState::StateMachineAsyncGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::Undefined,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        });
                                    let _ =
                                        self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                    self.drain_microtasks();
                                    return Completion::Normal(promise);
                                }
                                _ => result_value,
                            };
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineAsyncGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::SuspendedAtState {
                                        state_id: resume_state,
                                    },
                                    _sent_value: JsValue::Undefined,
                                    try_stack: try_stack.clone(),
                                    pending_binding: None,
                                    delegated_iterator: Some(
                                        crate::interpreter::types::DelegatedIteratorInfo {
                                            iterator,
                                            next_method: next_method.clone(),
                                            resume_state,
                                            sent_value_binding: binding,
                                        },
                                    ),
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            let iter_result = self.create_iter_result_object(awaited_value, false);
                            let _ = self.call_function(
                                &resolve_fn,
                                &JsValue::Undefined,
                                &[iter_result],
                            );
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                    }
                    Ok(None) => {
                        // No .return() method — complete the generator
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::Undefined,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            });
                        let iter_result = self.create_iter_result_object(value, true);
                        let _ =
                            self.call_function(&resolve_fn, &JsValue::Undefined, &[iter_result]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }
                    Err(e) => {
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::Undefined,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            });
                        let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }
                }
            }

            if let Some(try_info) = try_stack.last()
                && !try_info.entered_finally
                && let Some(finally_state) = try_info.finally_state
            {
                obj_rc.borrow_mut().iterator_state =
                    Some(IteratorState::StateMachineAsyncGenerator {
                        state_machine,
                        func_env,
                        is_strict,
                        execution_state: StateMachineExecutionState::SuspendedAtState {
                            state_id: finally_state,
                        },
                        _sent_value: JsValue::Undefined,
                        try_stack: try_stack[..try_stack.len() - 1].to_vec(),
                        pending_binding: None,
                        delegated_iterator: None,
                        pending_exception: None,
                        pending_return: Some(value.clone()),
                    });
                let result = self.async_generator_next_state_machine(this, JsValue::Undefined);
                if let Completion::Normal(inner_promise) = result {
                    return Completion::Normal(inner_promise);
                }
                return result;
            }

            obj_rc.borrow_mut().iterator_state = Some(IteratorState::StateMachineAsyncGenerator {
                state_machine,
                func_env,
                is_strict,
                execution_state: StateMachineExecutionState::Completed,
                _sent_value: JsValue::Undefined,
                try_stack: vec![],
                pending_binding: None,
                delegated_iterator: None,
                pending_exception: None,
                pending_return: None,
            });
            let iter_result = self.create_iter_result_object(value, true);
            let _ = self.call_function(&resolve_fn, &JsValue::Undefined, &[iter_result]);
            self.drain_microtasks();
            return Completion::Normal(promise);
        }

        let Some(IteratorState::AsyncGenerator {
            body,
            func_env,
            is_strict,
            execution_state,
        }) = state
        else {
            return self.reject_with_type_error("not an async generator object");
        };

        let promise = self.create_promise_object();
        let promise_id = if let JsValue::Object(ref po) = promise {
            po.id
        } else {
            0
        };
        let (resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);

        match &execution_state {
            GeneratorExecutionState::SuspendedStart | GeneratorExecutionState::Completed => {
                obj_rc.borrow_mut().iterator_state = Some(IteratorState::AsyncGenerator {
                    body,
                    func_env,
                    is_strict,
                    execution_state: GeneratorExecutionState::Completed,
                });
                let iter_result = self.create_iter_result_object(value, true);
                let _ = self.call_function(&resolve_fn, &JsValue::Undefined, &[iter_result]);
                self.drain_microtasks();
                Completion::Normal(promise)
            }
            GeneratorExecutionState::Executing => {
                let err = self.create_type_error("AsyncGenerator is already executing");
                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[err]);
                self.drain_microtasks();
                Completion::Normal(promise)
            }
            GeneratorExecutionState::SuspendedYield { .. } => {
                obj_rc.borrow_mut().iterator_state = Some(IteratorState::AsyncGenerator {
                    body,
                    func_env,
                    is_strict,
                    execution_state: GeneratorExecutionState::Completed,
                });
                let iter_result = self.create_iter_result_object(value, true);
                let _ = self.call_function(&resolve_fn, &JsValue::Undefined, &[iter_result]);
                self.drain_microtasks();
                Completion::Normal(promise)
            }
        }
    }

    pub(crate) fn async_generator_throw(
        &mut self,
        this: &JsValue,
        exception: JsValue,
    ) -> Completion {
        let JsValue::Object(o) = this else {
            return self
                .reject_with_type_error("AsyncGenerator.prototype.throw called on non-object");
        };

        let Some(obj_rc) = self.get_object(o.id) else {
            return self
                .reject_with_type_error("AsyncGenerator.prototype.throw called on non-object");
        };
        let state = obj_rc.borrow().iterator_state.clone();

        if let Some(IteratorState::StateMachineAsyncGenerator {
            state_machine,
            func_env,
            is_strict,
            try_stack,
            delegated_iterator,
            ..
        }) = &state
        {
            let state_machine = state_machine.clone();
            let func_env = func_env.clone();
            let is_strict = *is_strict;
            let try_stack = try_stack.clone();
            let delegated_iterator = delegated_iterator.clone();

            let promise = self.create_promise_object();
            let promise_id = if let JsValue::Object(ref po) = promise {
                po.id
            } else {
                0
            };
            let (resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);

            if let Some(ref deleg_info) = delegated_iterator {
                let iterator = deleg_info.iterator.clone();
                let next_method = deleg_info.next_method.clone();
                let resume_state = deleg_info.resume_state;
                let binding = deleg_info.sent_value_binding.clone();

                match self.iterator_throw(&iterator, &exception) {
                    Ok(Some(iter_result)) => {
                        let awaited_result = match self.await_value(&iter_result) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                obj_rc.borrow_mut().iterator_state =
                                    Some(IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    });
                                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                            _ => iter_result,
                        };
                        let done = match self.iterator_complete(&awaited_result) {
                            Ok(d) => d,
                            Err(e) => {
                                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                        };
                        let result_value = match self.iterator_value(&awaited_result) {
                            Ok(v) => v,
                            Err(e) => {
                                // IteratorValue threw — propagate into the generator's
                                // execution context so try/catch can handle it.
                                // Pop the try_stack and jump directly to the catch state.
                                let mut ts = try_stack.clone();
                                if let Some(try_info) = ts.pop()
                                    && let Some(catch_state) = try_info.catch_state
                                {
                                    obj_rc.borrow_mut().iterator_state =
                                        Some(IteratorState::StateMachineAsyncGenerator {
                                            state_machine: state_machine.clone(),
                                            func_env: func_env.clone(),
                                            is_strict,
                                            execution_state:
                                                StateMachineExecutionState::SuspendedAtState {
                                                    state_id: catch_state,
                                                },
                                            _sent_value: JsValue::Undefined,
                                            try_stack: ts,
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: Some(e),
                                            pending_return: None,
                                        });
                                    return self.async_generator_next_state_machine(
                                        this,
                                        JsValue::Undefined,
                                    );
                                }
                                // No catch handler — reject the promise
                                obj_rc.borrow_mut().iterator_state =
                                    Some(IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        _sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    });
                                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                        };
                        if done {
                            use crate::interpreter::generator_transform::SentValueBindingKind;
                            if let Some(ref bind) = binding {
                                match &bind.kind {
                                    SentValueBindingKind::Variable(name) => {
                                        func_env.borrow_mut().set(name, result_value.clone()).ok();
                                    }
                                    SentValueBindingKind::Pattern(pattern) => {
                                        let _ = self.bind_pattern(
                                            pattern,
                                            result_value.clone(),
                                            BindingKind::Var,
                                            &func_env,
                                        );
                                    }
                                    SentValueBindingKind::Discard
                                    | SentValueBindingKind::InlineYield { .. } => {}
                                }
                            }
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineAsyncGenerator {
                                    state_machine: state_machine.clone(),
                                    func_env: func_env.clone(),
                                    is_strict,
                                    execution_state: StateMachineExecutionState::SuspendedAtState {
                                        state_id: resume_state,
                                    },
                                    _sent_value: JsValue::Undefined,
                                    try_stack: try_stack.clone(),
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            return self
                                .async_generator_next_state_machine(this, JsValue::Undefined);
                        } else {
                            let awaited_value = match self.await_value(&result_value) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => {
                                    obj_rc.borrow_mut().iterator_state =
                                        Some(IteratorState::StateMachineAsyncGenerator {
                                            state_machine,
                                            func_env,
                                            is_strict,
                                            execution_state: StateMachineExecutionState::Completed,
                                            _sent_value: JsValue::Undefined,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                            pending_return: None,
                                        });
                                    let _ =
                                        self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                    self.drain_microtasks();
                                    return Completion::Normal(promise);
                                }
                                _ => result_value,
                            };
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineAsyncGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::SuspendedAtState {
                                        state_id: resume_state,
                                    },
                                    _sent_value: JsValue::Undefined,
                                    try_stack: try_stack.clone(),
                                    pending_binding: None,
                                    delegated_iterator: Some(
                                        crate::interpreter::types::DelegatedIteratorInfo {
                                            iterator,
                                            next_method: next_method.clone(),
                                            resume_state,
                                            sent_value_binding: binding,
                                        },
                                    ),
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            let iter_result = self.create_iter_result_object(awaited_value, false);
                            let _ = self.call_function(
                                &resolve_fn,
                                &JsValue::Undefined,
                                &[iter_result],
                            );
                            self.drain_microtasks();
                            return Completion::Normal(promise);
                        }
                    }
                    Ok(None) => {
                        // No .throw() method — close iterator and reject with TypeError
                        let _ = self.iterator_close(&iterator, exception.clone());
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::Undefined,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            });
                        let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[exception]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }
                    Err(e) => {
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                _sent_value: JsValue::Undefined,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                                pending_return: None,
                            });
                        let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }
                }
            }

            obj_rc.borrow_mut().iterator_state = Some(IteratorState::StateMachineAsyncGenerator {
                state_machine,
                func_env,
                is_strict,
                execution_state: StateMachineExecutionState::Completed,
                _sent_value: JsValue::Undefined,
                try_stack: vec![],
                pending_binding: None,
                delegated_iterator: None,
                pending_exception: None,
                pending_return: None,
            });
            let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[exception]);
            self.drain_microtasks();
            return Completion::Normal(promise);
        }

        let Some(IteratorState::AsyncGenerator {
            body,
            func_env,
            is_strict,
            ..
        }) = state
        else {
            return self.reject_with_type_error("not an async generator object");
        };

        let promise = self.create_promise_object();
        let promise_id = if let JsValue::Object(ref po) = promise {
            po.id
        } else {
            0
        };
        let (_, reject_fn) = self.create_resolving_functions(promise_id);

        obj_rc.borrow_mut().iterator_state = Some(IteratorState::AsyncGenerator {
            body,
            func_env,
            is_strict,
            execution_state: GeneratorExecutionState::Completed,
        });
        let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[exception]);
        self.drain_microtasks();
        Completion::Normal(promise)
    }

    pub(crate) fn call_function(
        &mut self,
        func_val: &JsValue,
        _this_val: &JsValue,
        args: &[JsValue],
    ) -> Completion {
        if let JsValue::Object(o) = func_val
            && let Some(obj) = self.get_object(o.id)
        {
            // Proxy apply trap (also check revoked proxy)
            if obj.borrow().is_proxy() || obj.borrow().proxy_revoked {
                let target_val = self.get_proxy_target_val(o.id);
                let args_array = self.create_array(args.to_vec());
                match self.invoke_proxy_trap(
                    o.id,
                    "apply",
                    vec![target_val.clone(), _this_val.clone(), args_array],
                ) {
                    Ok(Some(v)) => return Completion::Normal(v),
                    Ok(None) => {
                        // No trap, call target directly
                        return self.call_function(&target_val, _this_val, args);
                    }
                    Err(e) => return Completion::Throw(e),
                }
            }
            let is_class_ctor = obj.borrow().is_class_constructor;
            if is_class_ctor && self.new_target.is_none() {
                return Completion::Throw(
                    self.create_type_error("Class constructor cannot be invoked without 'new'"),
                );
            }
            let callable = obj.borrow().callable.clone();
            if let Some(func) = callable {
                return match func {
                    JsFunction::Native(_, _, f, _) => {
                        // Root args and this_val so GC doesn't collect them
                        // during native function execution (e.g. Array.prototype.map callback)
                        self.gc_root_value(_this_val);
                        for a in args.iter() {
                            self.gc_root_value(a);
                        }
                        let saved_this = self.last_call_this_value.take();
                        let result = f(self, _this_val, args);
                        self.last_call_this_value = saved_this;
                        self.last_call_had_explicit_return = true;
                        for a in args.iter().rev() {
                            self.gc_unroot_value(a);
                        }
                        self.gc_unroot_value(_this_val);
                        result
                    }
                    JsFunction::User {
                        params,
                        body,
                        closure,
                        is_arrow,
                        is_strict,
                        is_generator,
                        is_async,
                        ..
                    } => {
                        if is_async && !is_generator {
                            return self.call_async_function(
                                &params,
                                &body,
                                closure.clone(),
                                is_arrow,
                                is_strict,
                                _this_val,
                                args,
                                func_val,
                            );
                        }
                        if is_async && is_generator {
                            let gen_obj = self.create_object();
                            // Set prototype from function's .prototype property, fall back to intrinsic
                            let mut proto_set = false;
                            if let Some(func_obj_rc) = self.get_object(o.id) {
                                let proto_val =
                                    func_obj_rc.borrow().get_property_value("prototype");
                                if let Some(JsValue::Object(ref p)) = proto_val
                                    && let Some(proto_rc) = self.get_object(p.id)
                                {
                                    gen_obj.borrow_mut().prototype = Some(proto_rc);
                                    proto_set = true;
                                }
                            }
                            if !proto_set {
                                gen_obj.borrow_mut().prototype =
                                    self.realm().async_generator_prototype.clone();
                            }
                            gen_obj.borrow_mut().class_name = "AsyncGenerator".to_string();
                            // Create persistent function environment
                            let func_env = Environment::new_function_scope(Some(closure.clone()));
                            func_env.borrow_mut().strict = is_strict;
                            func_env.borrow_mut().bindings.insert(
                                "this".to_string(),
                                Binding {
                                    value: _this_val.clone(),
                                    kind: BindingKind::Const,
                                    initialized: true,
                                    deletable: false,
                                },
                            );
                            let arguments_obj = self.create_arguments_object(
                                args,
                                JsValue::Undefined,
                                is_strict,
                                None,
                                &[],
                            );
                            func_env.borrow_mut().declare("arguments", BindingKind::Var);
                            let _ = func_env.borrow_mut().set("arguments", arguments_obj);
                            func_env.borrow_mut().arguments_immutable = true;
                            for (i, param) in params.iter().enumerate() {
                                if let Pattern::Rest(inner) = param {
                                    let rest: Vec<JsValue> = args.get(i..).unwrap_or(&[]).to_vec();
                                    let rest_arr = self.create_array(rest);
                                    if let Err(e) = self.bind_pattern(
                                        inner,
                                        rest_arr,
                                        BindingKind::Var,
                                        &func_env,
                                    ) {
                                        return Completion::Throw(e);
                                    }
                                    break;
                                }
                                let val = args.get(i).cloned().unwrap_or(JsValue::Undefined);
                                if let Err(e) =
                                    self.bind_pattern(param, val, BindingKind::Var, &func_env)
                                {
                                    return Completion::Throw(e);
                                }
                            }
                            let is_simple =
                                params.iter().all(|p| matches!(p, Pattern::Identifier(_)));
                            let exec_env = if !is_simple {
                                let body_env =
                                    Environment::new_function_scope(Some(func_env.clone()));
                                body_env.borrow_mut().strict = func_env.borrow().strict;
                                body_env.borrow_mut().has_simple_params = false;
                                let mut var_names = std::collections::HashSet::new();
                                Self::collect_var_names_from_stmts(&body, &mut var_names);
                                let mut param_names_set = std::collections::HashSet::new();
                                for p in &params {
                                    Self::collect_var_names_from_pattern(p, &mut param_names_set);
                                }
                                for name in &var_names {
                                    body_env.borrow_mut().declare(name, BindingKind::Var);
                                    if param_names_set.contains(name) {
                                        let val = func_env
                                            .borrow()
                                            .get(name)
                                            .unwrap_or(JsValue::Undefined);
                                        let _ = body_env.borrow_mut().set(name, val);
                                    }
                                }
                                body_env
                            } else {
                                func_env.clone()
                            };

                            use crate::interpreter::generator_transform::transform_generator;
                            let state_machine = Rc::new(transform_generator(&body, &params));
                            for temp_var in &state_machine.temp_vars {
                                exec_env.borrow_mut().declare(temp_var, BindingKind::Var);
                            }
                            gen_obj.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineAsyncGenerator {
                                    state_machine,
                                    func_env: exec_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::SuspendedStart,
                                    _sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            let gen_id = gen_obj.borrow().id.unwrap();
                            return Completion::Normal(JsValue::Object(crate::types::JsObject {
                                id: gen_id,
                            }));
                        }
                        if is_generator {
                            // Create a generator object instead of executing
                            let gen_obj = self.create_object();
                            // Set prototype from the function's .prototype property
                            if let Some(func_obj_rc) = self.get_object(o.id) {
                                let proto_val =
                                    func_obj_rc.borrow().get_property_value("prototype");
                                if let Some(JsValue::Object(ref p)) = proto_val
                                    && let Some(proto_rc) = self.get_object(p.id)
                                {
                                    gen_obj.borrow_mut().prototype = Some(proto_rc);
                                }
                            }
                            gen_obj.borrow_mut().class_name = "Generator".to_string();
                            // Create persistent function environment
                            let func_env = Environment::new_function_scope(Some(closure.clone()));
                            func_env.borrow_mut().strict = is_strict;
                            func_env.borrow_mut().bindings.insert(
                                "this".to_string(),
                                Binding {
                                    value: _this_val.clone(),
                                    kind: BindingKind::Const,
                                    initialized: true,
                                    deletable: false,
                                },
                            );
                            let arguments_obj = self.create_arguments_object(
                                args,
                                JsValue::Undefined,
                                is_strict,
                                None,
                                &[],
                            );
                            func_env.borrow_mut().declare("arguments", BindingKind::Var);
                            let _ = func_env.borrow_mut().set("arguments", arguments_obj);
                            func_env.borrow_mut().arguments_immutable = true;
                            for (i, param) in params.iter().enumerate() {
                                if let Pattern::Rest(inner) = param {
                                    let rest: Vec<JsValue> = args.get(i..).unwrap_or(&[]).to_vec();
                                    let rest_arr = self.create_array(rest);
                                    if let Err(e) = self.bind_pattern(
                                        inner,
                                        rest_arr,
                                        BindingKind::Var,
                                        &func_env,
                                    ) {
                                        return Completion::Throw(e);
                                    }
                                    break;
                                }
                                let val = args.get(i).cloned().unwrap_or(JsValue::Undefined);
                                if let Err(e) =
                                    self.bind_pattern(param, val, BindingKind::Var, &func_env)
                                {
                                    return Completion::Throw(e);
                                }
                            }
                            let is_simple =
                                params.iter().all(|p| matches!(p, Pattern::Identifier(_)));
                            let exec_env = if !is_simple {
                                let body_env =
                                    Environment::new_function_scope(Some(func_env.clone()));
                                body_env.borrow_mut().strict = func_env.borrow().strict;
                                body_env.borrow_mut().has_simple_params = false;
                                let mut var_names = std::collections::HashSet::new();
                                Self::collect_var_names_from_stmts(&body, &mut var_names);
                                let mut param_names_set = std::collections::HashSet::new();
                                for p in &params {
                                    Self::collect_var_names_from_pattern(p, &mut param_names_set);
                                }
                                for name in &var_names {
                                    body_env.borrow_mut().declare(name, BindingKind::Var);
                                    if param_names_set.contains(name) {
                                        let val = func_env
                                            .borrow()
                                            .get(name)
                                            .unwrap_or(JsValue::Undefined);
                                        let _ = body_env.borrow_mut().set(name, val);
                                    }
                                }
                                body_env
                            } else {
                                func_env.clone()
                            };

                            use crate::interpreter::generator_transform::transform_generator;
                            let state_machine = Rc::new(transform_generator(&body, &params));
                            for temp_var in &state_machine.temp_vars {
                                exec_env.borrow_mut().declare(temp_var, BindingKind::Var);
                            }
                            gen_obj.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineGenerator {
                                    state_machine,
                                    func_env: exec_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::SuspendedStart,
                                    _sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                    pending_return: None,
                                });
                            let gen_id = gen_obj.borrow().id.unwrap();
                            return Completion::Normal(JsValue::Object(crate::types::JsObject {
                                id: gen_id,
                            }));
                        }
                        let closure_strict = closure.borrow().strict;
                        let func_env = Environment::new_function_scope(Some(closure));
                        if is_arrow {
                            func_env.borrow_mut().is_arrow_scope = true;
                        }
                        let is_simple = params.iter().all(|p| matches!(p, Pattern::Identifier(_)));
                        if !is_arrow {
                            if self.constructing_derived {
                                // Derived constructor: this is in TDZ until super() is called
                                func_env.borrow_mut().bindings.insert(
                                    "this".to_string(),
                                    Binding {
                                        value: JsValue::Undefined,
                                        kind: BindingKind::Const,
                                        initialized: false,
                                        deletable: false,
                                    },
                                );
                                self.constructing_derived = false;
                            } else {
                                let effective_this = if !is_strict && !closure_strict {
                                    if matches!(_this_val, JsValue::Undefined | JsValue::Null) {
                                        self.realm().global_env
                                            .borrow()
                                            .get("this")
                                            .unwrap_or(_this_val.clone())
                                    } else if !matches!(_this_val, JsValue::Object(_)) {
                                        match self.to_object(_this_val) {
                                            Completion::Normal(v) => v,
                                            _ => _this_val.clone(),
                                        }
                                    } else {
                                        _this_val.clone()
                                    }
                                } else {
                                    _this_val.clone()
                                };
                                func_env.borrow_mut().bindings.insert(
                                    "this".to_string(),
                                    Binding {
                                        value: effective_this,
                                        kind: BindingKind::Const,
                                        initialized: true,
                                        deletable: false,
                                    },
                                );
                            }
                            let env_strict = func_env.borrow().strict;
                            let use_mapped = is_simple && !is_strict && !env_strict;
                            let param_names: Vec<String> = if use_mapped {
                                params
                                    .iter()
                                    .filter_map(|p| {
                                        if let Pattern::Identifier(name) = p {
                                            Some(name.clone())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect()
                            } else {
                                Vec::new()
                            };
                            let mapped_env = if use_mapped { Some(&func_env) } else { None };
                            let arguments_obj = self.create_arguments_object(
                                args,
                                func_val.clone(),
                                is_strict,
                                mapped_env,
                                &param_names,
                            );
                            func_env.borrow_mut().declare("arguments", BindingKind::Var);
                            let _ = func_env.borrow_mut().set("arguments", arguments_obj);
                            if is_strict || !is_simple {
                                func_env.borrow_mut().arguments_immutable = true;
                            }
                        }
                        // For arrows with non-simple params and "arguments" parameter,
                        // mark arguments as immutable for eval redeclaration checks
                        if is_arrow && !is_simple {
                            let has_arguments_param = params.iter().any(
                                |p| matches!(p, Pattern::Identifier(name) if name == "arguments"),
                            );
                            if has_arguments_param {
                                func_env.borrow_mut().arguments_immutable = true;
                            }
                        }
                        // Bind parameters (after this so default exprs can access this)
                        for (i, param) in params.iter().enumerate() {
                            if let Pattern::Rest(inner) = param {
                                let rest: Vec<JsValue> = args.get(i..).unwrap_or(&[]).to_vec();
                                let rest_arr = self.create_array(rest);
                                if let Err(e) =
                                    self.bind_pattern(inner, rest_arr, BindingKind::Var, &func_env)
                                {
                                    return Completion::Throw(e);
                                }
                                break;
                            }
                            let val = args.get(i).cloned().unwrap_or(JsValue::Undefined);
                            if let Err(e) =
                                self.bind_pattern(param, val, BindingKind::Var, &func_env)
                            {
                                return Completion::Throw(e);
                            }
                        }
                        let exec_env = if !is_simple {
                            let body_env = Environment::new_function_scope(Some(func_env.clone()));
                            body_env.borrow_mut().strict = func_env.borrow().strict;
                            body_env.borrow_mut().has_simple_params = false;
                            let mut var_names = std::collections::HashSet::new();
                            Self::collect_var_names_from_stmts(&body, &mut var_names);
                            let mut param_names = std::collections::HashSet::new();
                            for p in &params {
                                Self::collect_var_names_from_pattern(p, &mut param_names);
                            }
                            for name in &var_names {
                                body_env.borrow_mut().declare(name, BindingKind::Var);
                                if param_names.contains(name) {
                                    let val =
                                        func_env.borrow().get(name).unwrap_or(JsValue::Undefined);
                                    let _ = body_env.borrow_mut().set(name, val);
                                }
                            }
                            body_env
                        } else {
                            func_env.clone()
                        };
                        exec_env.borrow_mut().strict = is_strict;
                        self.call_stack_envs.push(exec_env.clone());
                        let result = self.exec_statements(&body, &exec_env);
                        self.call_stack_envs.pop();
                        let result = self.dispose_resources(&exec_env, result);
                        self.last_call_this_value = func_env.borrow().get("this");
                        match result {
                            Completion::Return(v) => {
                                self.last_call_had_explicit_return = true;
                                Completion::Normal(v)
                            }
                            Completion::Normal(_) | Completion::Empty => {
                                self.last_call_had_explicit_return = false;
                                Completion::Normal(JsValue::Undefined)
                            }
                            Completion::Yield(_) => Completion::Normal(JsValue::Undefined),
                            other => other,
                        }
                    }
                };
            }
        }
        let desc = match func_val {
            JsValue::Undefined => "undefined is not a function".to_string(),
            JsValue::Null => "null is not a function".to_string(),
            JsValue::Boolean(b) => format!("{} is not a function", b),
            JsValue::Number(n) => format!("{} is not a function", n),
            JsValue::String(s) => {
                let preview: String = s.to_rust_string().chars().take(30).collect();
                format!("\"{}\" is not a function", preview)
            }
            JsValue::Object(o) => {
                if let Some(obj) = self.get_object(o.id) {
                    let class = obj.borrow().class_name.clone();
                    let has_callable = obj.borrow().callable.is_some();
                    let keys: Vec<String> = obj
                        .borrow()
                        .property_order
                        .iter()
                        .take(10)
                        .cloned()
                        .collect();
                    format!(
                        "object (class={}, callable={}, id={}, keys={:?}) is not a function",
                        class, has_callable, o.id, keys
                    )
                } else {
                    format!("object (id={}, GC'd?) is not a function", o.id)
                }
            }
            _ => "is not a function".to_string(),
        };
        let err = self.create_type_error(&desc);
        Completion::Throw(err)
    }

    fn eval_spread_args(
        &mut self,
        args: &[Expression],
        env: &EnvRef,
    ) -> Result<Vec<JsValue>, JsValue> {
        let mut evaluated = Vec::new();
        for arg in args {
            if let Expression::Spread(inner) = arg {
                let val = match self.eval_expr(inner, env) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => {
                        for v in &evaluated {
                            self.gc_unroot_value(v);
                        }
                        return Err(e);
                    }
                    _ => JsValue::Undefined,
                };
                let items = match self.iterate_to_vec(&val) {
                    Ok(items) => items,
                    Err(e) => {
                        for v in &evaluated {
                            self.gc_unroot_value(v);
                        }
                        return Err(e);
                    }
                };
                for item in &items {
                    self.gc_root_value(item);
                }
                evaluated.extend(items);
            } else {
                let val = match self.eval_expr(arg, env) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => {
                        for v in &evaluated {
                            self.gc_unroot_value(v);
                        }
                        return Err(e);
                    }
                    _ => JsValue::Undefined,
                };
                self.gc_root_value(&val);
                evaluated.push(val);
            }
        }
        for v in &evaluated {
            self.gc_unroot_value(v);
        }
        Ok(evaluated)
    }

    fn is_builtin_eval(&self, val: &JsValue) -> bool {
        if let JsValue::Object(o) = val {
            // Direct eval must be the CURRENT realm's eval
            if let Some(eval_id) = self.realm().builtin_eval_id {
                return o.id == eval_id;
            }
        }
        false
    }

    fn stmts_contain_arguments(stmts: &[Statement]) -> bool {
        stmts.iter().any(Self::stmt_contains_arguments)
    }

    fn stmt_contains_arguments(stmt: &Statement) -> bool {
        use crate::ast::*;
        match stmt {
            Statement::Expression(e) => Self::expr_contains_arguments(e),
            Statement::Variable(d) => d.declarations.iter().any(|decl| {
                decl.init
                    .as_ref()
                    .is_some_and(Self::expr_contains_arguments)
            }),
            Statement::Block(stmts) => Self::stmts_contain_arguments(stmts),
            Statement::If(if_stmt) => {
                Self::expr_contains_arguments(&if_stmt.test)
                    || Self::stmt_contains_arguments(&if_stmt.consequent)
                    || if_stmt
                        .alternate
                        .as_ref()
                        .is_some_and(|a| Self::stmt_contains_arguments(a))
            }
            Statement::Return(e) => e.as_ref().is_some_and(Self::expr_contains_arguments),
            Statement::Throw(e) => Self::expr_contains_arguments(e),
            Statement::Try(t) => {
                Self::stmts_contain_arguments(&t.block)
                    || t.handler
                        .as_ref()
                        .is_some_and(|h| Self::stmts_contain_arguments(&h.body))
                    || t.finalizer
                        .as_ref()
                        .is_some_and(|f| Self::stmts_contain_arguments(f))
            }
            Statement::While(w) => {
                Self::expr_contains_arguments(&w.test) || Self::stmt_contains_arguments(&w.body)
            }
            Statement::For(f) => {
                f.init.as_ref().is_some_and(|i| match i {
                    ForInit::Expression(e) => Self::expr_contains_arguments(e),
                    ForInit::Variable(d) => d.declarations.iter().any(|decl| {
                        decl.init
                            .as_ref()
                            .is_some_and(Self::expr_contains_arguments)
                    }),
                }) || f.test.as_ref().is_some_and(Self::expr_contains_arguments)
                    || f.update.as_ref().is_some_and(Self::expr_contains_arguments)
                    || Self::stmt_contains_arguments(&f.body)
            }
            Statement::ForIn(f) => {
                Self::expr_contains_arguments(&f.right) || Self::stmt_contains_arguments(&f.body)
            }
            Statement::ForOf(f) => {
                Self::expr_contains_arguments(&f.right) || Self::stmt_contains_arguments(&f.body)
            }
            Statement::Switch(s) => {
                Self::expr_contains_arguments(&s.discriminant)
                    || s.cases
                        .iter()
                        .any(|c| Self::stmts_contain_arguments(&c.consequent))
            }
            Statement::DoWhile(d) => {
                Self::stmt_contains_arguments(&d.body) || Self::expr_contains_arguments(&d.test)
            }
            Statement::Labeled(_, s) => Self::stmt_contains_arguments(s),
            Statement::With(e, s) => {
                Self::expr_contains_arguments(e) || Self::stmt_contains_arguments(s)
            }
            // Function/class declarations create their own scope — don't recurse
            Statement::FunctionDeclaration(_) | Statement::ClassDeclaration(_) => false,
            _ => false,
        }
    }

    fn expr_contains_arguments(expr: &Expression) -> bool {
        use crate::ast::*;
        match expr {
            Expression::Identifier(name) => name == "arguments",
            Expression::Array(elems) => elems
                .iter()
                .any(|e| e.as_ref().is_some_and(Self::expr_contains_arguments)),
            Expression::Object(props) => props.iter().any(|p| {
                Self::expr_contains_arguments(&p.value)
                    || matches!(&p.key, PropertyKey::Computed(e) if Self::expr_contains_arguments(e))
            }),
            Expression::Member(obj, prop) => {
                Self::expr_contains_arguments(obj)
                    || matches!(prop, MemberProperty::Computed(e) if Self::expr_contains_arguments(e))
            }
            Expression::Call(callee, args) | Expression::New(callee, args) => {
                Self::expr_contains_arguments(callee)
                    || args.iter().any(Self::expr_contains_arguments)
            }
            Expression::Binary(_, l, r)
            | Expression::Logical(_, l, r)
            | Expression::Assign(_, l, r) => {
                Self::expr_contains_arguments(l) || Self::expr_contains_arguments(r)
            }
            Expression::Unary(_, e)
            | Expression::Update(_, _, e)
            | Expression::Spread(e)
            | Expression::Await(e)
            | Expression::Yield(Some(e), _) => Self::expr_contains_arguments(e),
            Expression::Conditional(t, c, a) => {
                Self::expr_contains_arguments(t)
                    || Self::expr_contains_arguments(c)
                    || Self::expr_contains_arguments(a)
            }
            Expression::Sequence(exprs) | Expression::Comma(exprs) => {
                exprs.iter().any(Self::expr_contains_arguments)
            }
            Expression::Template(tl) => tl.expressions.iter().any(Self::expr_contains_arguments),
            Expression::TaggedTemplate(tag, tl) => {
                Self::expr_contains_arguments(tag)
                    || tl.expressions.iter().any(Self::expr_contains_arguments)
            }
            Expression::ArrowFunction(af) => match &af.body {
                ArrowBody::Expression(e) => Self::expr_contains_arguments(e),
                ArrowBody::Block(stmts) => Self::stmts_contain_arguments(stmts),
            },
            Expression::Function(_) | Expression::Class(_) => false,
            _ => false,
        }
    }

    fn stmts_contain_super_call(stmts: &[Statement]) -> bool {
        stmts.iter().any(Self::stmt_contains_super_call)
    }

    fn stmt_contains_super_call(stmt: &Statement) -> bool {
        use crate::ast::*;
        match stmt {
            Statement::Expression(e) => Self::expr_contains_super_call(e),
            Statement::Variable(d) => d.declarations.iter().any(|decl| {
                decl.init
                    .as_ref()
                    .is_some_and(Self::expr_contains_super_call)
            }),
            Statement::Block(stmts) => Self::stmts_contain_super_call(stmts),
            Statement::If(if_stmt) => {
                Self::expr_contains_super_call(&if_stmt.test)
                    || Self::stmt_contains_super_call(&if_stmt.consequent)
                    || if_stmt
                        .alternate
                        .as_ref()
                        .is_some_and(|a| Self::stmt_contains_super_call(a))
            }
            Statement::Return(e) => e.as_ref().is_some_and(Self::expr_contains_super_call),
            Statement::Throw(e) => Self::expr_contains_super_call(e),
            Statement::FunctionDeclaration(_) | Statement::ClassDeclaration(_) => false,
            _ => false,
        }
    }

    fn expr_contains_super_call(expr: &Expression) -> bool {
        use crate::ast::*;
        match expr {
            Expression::Call(callee, args) => {
                matches!(**callee, Expression::Super)
                    || Self::expr_contains_super_call(callee)
                    || args.iter().any(Self::expr_contains_super_call)
            }
            Expression::Array(elems) => elems
                .iter()
                .any(|e| e.as_ref().is_some_and(Self::expr_contains_super_call)),
            Expression::Binary(_, l, r)
            | Expression::Logical(_, l, r)
            | Expression::Assign(_, l, r) => {
                Self::expr_contains_super_call(l) || Self::expr_contains_super_call(r)
            }
            Expression::Unary(_, e) | Expression::Update(_, _, e) | Expression::Spread(e) => {
                Self::expr_contains_super_call(e)
            }
            Expression::Conditional(t, c, a) => {
                Self::expr_contains_super_call(t)
                    || Self::expr_contains_super_call(c)
                    || Self::expr_contains_super_call(a)
            }
            Expression::ArrowFunction(af) => match &af.body {
                ArrowBody::Expression(e) => Self::expr_contains_super_call(e),
                ArrowBody::Block(stmts) => Self::stmts_contain_super_call(stmts),
            },
            Expression::New(callee, args) => {
                Self::expr_contains_super_call(callee)
                    || args.iter().any(Self::expr_contains_super_call)
            }
            Expression::Member(obj, prop) => {
                Self::expr_contains_super_call(obj)
                    || matches!(prop, MemberProperty::Computed(e) if Self::expr_contains_super_call(e))
            }
            Expression::Sequence(exprs) | Expression::Comma(exprs) => {
                exprs.iter().any(Self::expr_contains_super_call)
            }
            Expression::Function(_) | Expression::Class(_) => false,
            _ => false,
        }
    }

    pub(crate) fn perform_eval(
        &mut self,
        args: &[JsValue],
        caller_strict: bool,
        direct: bool,
        caller_env: &EnvRef,
    ) -> Completion {
        let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
        if !matches!(&arg, JsValue::String(_)) {
            return Completion::Normal(arg);
        }
        // Use PUA mapping to preserve lone surrogates through the UTF-8 parser
        let code = if let JsValue::String(ref s) = arg {
            crate::interpreter::builtins::regexp::js_string_to_regex_input(&s.code_units)
        } else {
            to_js_string(&arg)
        };
        let mut p = match parser::Parser::new(&code) {
            Ok(p) => p,
            Err(_) => {
                return Completion::Throw(self.create_error("SyntaxError", "Invalid eval source"));
            }
        };
        if caller_strict && direct {
            p.set_strict(true);
        }
        let mut in_field_initializer = false;
        if direct {
            let mut found_function = false;
            let mut found_home_object = false;
            let mut env_walk = Some(caller_env.clone());
            loop {
                let e = match env_walk {
                    Some(ref e) => e.clone(),
                    None => break,
                };
                let borrowed = e.borrow();
                if borrowed.is_field_initializer {
                    in_field_initializer = true;
                }
                if borrowed.is_function_scope && !borrowed.is_arrow_scope && !found_function {
                    found_function = true;
                }
                if borrowed.bindings.contains_key("__home_object__") && !found_home_object {
                    found_home_object = true;
                }
                if let Some(ref names) = borrowed.class_private_names {
                    let name_set: std::collections::HashSet<String> =
                        names.keys().cloned().collect();
                    p.set_eval_in_class_with_names(name_set);
                    break;
                }
                env_walk = borrowed.parent.clone();
            }
            if found_function {
                p.set_eval_new_target_allowed();
            }
            if found_home_object {
                p.set_eval_allow_super_property();
            }
        }
        if in_field_initializer {
            p.set_eval_in_field_initializer();
        }
        let program = match p.parse_program() {
            Ok(prog) => prog,
            Err(e) => {
                return Completion::Throw(self.create_error("SyntaxError", &format!("{}", e)));
            }
        };
        if in_field_initializer {
            if Self::stmts_contain_arguments(&program.body) {
                return Completion::Throw(self.create_error(
                    "SyntaxError",
                    "'arguments' is not allowed in class field initializer or static block",
                ));
            }
            if Self::stmts_contain_super_call(&program.body) {
                return Completion::Throw(self.create_error(
                    "SyntaxError",
                    "'super()' is not allowed in class field initializer",
                ));
            }
        }
        let is_strict = (caller_strict && direct) || program.body_is_strict;

        // Determine varEnv and lexEnv per spec PerformEval / EvalDeclarationInstantiation
        let (var_env, lex_env) = if is_strict {
            // Strict eval: both var and lex are a new function scope
            // For indirect eval, caller_env is already the eval's realm's global env
            let base = caller_env.clone();
            let new_env = Environment::new_function_scope(Some(base));
            new_env.borrow_mut().strict = true;
            (new_env.clone(), new_env)
        } else if direct {
            // Non-strict direct eval: var goes to caller's var scope,
            // lex is a new declarative environment for let/const/class
            let var_env = Environment::find_var_scope(caller_env);
            let lex_env = Environment::new(Some(caller_env.clone()));
            (var_env, lex_env)
        } else {
            // Non-strict indirect eval: var is global, lex is new child of global
            // For cross-realm eval, caller_env is already the eval function's realm's global env
            let lex_env = Environment::new(Some(caller_env.clone()));
            lex_env.borrow_mut().strict = false;
            (caller_env.clone(), lex_env)
        };

        // EvalDeclarationInstantiation
        if let Err(e) = self.eval_declaration_instantiation(
            &program.body,
            &var_env,
            &lex_env,
            is_strict,
            direct,
            caller_env,
        ) {
            return Completion::Throw(e);
        }

        // Execute statements in lex_env
        self.call_stack_envs.push(lex_env.clone());
        let mut last = Completion::Empty;
        for stmt in &program.body {
            self.maybe_gc();
            match self.exec_statement(stmt, &lex_env) {
                Completion::Normal(v) => last = Completion::Normal(v),
                Completion::Empty => {}
                other => {
                    self.call_stack_envs.pop();
                    return other;
                }
            }
        }
        self.call_stack_envs.pop();
        last.update_empty(JsValue::Undefined)
    }

    /// Collect top-level var-declared names from eval body (recursively into blocks, etc.)
    fn collect_eval_var_names(stmts: &[Statement], names: &mut Vec<String>) {
        for stmt in stmts {
            Self::collect_eval_var_names_from_stmt(stmt, names);
        }
    }

    fn collect_eval_var_names_from_stmt(stmt: &Statement, names: &mut Vec<String>) {
        match stmt {
            Statement::Variable(decl) if decl.kind == VarKind::Var => {
                for d in &decl.declarations {
                    Self::collect_pattern_names(&d.pattern, names);
                }
            }
            Statement::Block(stmts) => {
                for s in stmts {
                    Self::collect_eval_var_names_from_stmt(s, names);
                }
            }
            Statement::If(i) => {
                Self::collect_eval_var_names_from_stmt(&i.consequent, names);
                if let Some(alt) = &i.alternate {
                    Self::collect_eval_var_names_from_stmt(alt, names);
                }
            }
            Statement::While(w) => Self::collect_eval_var_names_from_stmt(&w.body, names),
            Statement::DoWhile(d) => Self::collect_eval_var_names_from_stmt(&d.body, names),
            Statement::For(f) => {
                if let Some(ForInit::Variable(decl)) = &f.init
                    && decl.kind == VarKind::Var
                {
                    for d in &decl.declarations {
                        Self::collect_pattern_names(&d.pattern, names);
                    }
                }
                Self::collect_eval_var_names_from_stmt(&f.body, names);
            }
            Statement::ForIn(fi) => {
                if let ForInOfLeft::Variable(decl) = &fi.left
                    && decl.kind == VarKind::Var
                {
                    for d in &decl.declarations {
                        Self::collect_pattern_names(&d.pattern, names);
                    }
                }
                Self::collect_eval_var_names_from_stmt(&fi.body, names);
            }
            Statement::ForOf(fo) => {
                if let ForInOfLeft::Variable(decl) = &fo.left
                    && decl.kind == VarKind::Var
                {
                    for d in &decl.declarations {
                        Self::collect_pattern_names(&d.pattern, names);
                    }
                }
                Self::collect_eval_var_names_from_stmt(&fo.body, names);
            }
            Statement::Switch(sw) => {
                for case in &sw.cases {
                    for s in &case.consequent {
                        Self::collect_eval_var_names_from_stmt(s, names);
                    }
                }
            }
            Statement::Try(t) => {
                for s in &t.block {
                    Self::collect_eval_var_names_from_stmt(s, names);
                }
                if let Some(handler) = &t.handler {
                    for s in &handler.body {
                        Self::collect_eval_var_names_from_stmt(s, names);
                    }
                }
                if let Some(finalizer) = &t.finalizer {
                    for s in finalizer {
                        Self::collect_eval_var_names_from_stmt(s, names);
                    }
                }
            }
            Statement::Labeled(_, inner) => {
                Self::collect_eval_var_names_from_stmt(inner, names);
            }
            Statement::With(_, inner) => {
                Self::collect_eval_var_names_from_stmt(inner, names);
            }
            _ => {}
        }
    }

    /// Collect top-level function declarations from eval body (only top-level, not inside blocks)
    fn collect_eval_function_decls(stmts: &[Statement]) -> Vec<FunctionDecl> {
        let mut funcs = Vec::new();
        for stmt in stmts {
            if let Some(f) = Self::unwrap_labeled_function(stmt) {
                funcs.push(f.clone());
            }
        }
        // Per spec: reverse order, keep last occurrence of each name
        funcs.reverse();
        let mut seen = std::collections::HashSet::new();
        funcs.retain(|f| seen.insert(f.name.clone()));
        funcs
    }

    /// EvalDeclarationInstantiation per spec 19.2.1.4
    fn eval_declaration_instantiation(
        &mut self,
        body: &[Statement],
        var_env: &EnvRef,
        lex_env: &EnvRef,
        strict: bool,
        direct: bool,
        caller_env: &EnvRef,
    ) -> Result<(), JsValue> {
        let is_global = var_env.borrow().global_object.is_some();

        // Collect function declarations to initialize
        let functions_to_init = Self::collect_eval_function_decls(body);
        let declared_func_names: Vec<String> =
            functions_to_init.iter().map(|f| f.name.clone()).collect();

        // Collect var-declared names (excluding those that are also function names)
        let mut all_var_names = Vec::new();
        Self::collect_eval_var_names(body, &mut all_var_names);
        let declared_var_names: Vec<String> = {
            let mut seen = std::collections::HashSet::new();
            all_var_names
                .into_iter()
                .filter(|n| !declared_func_names.contains(n) && seen.insert(n.clone()))
                .collect()
        };

        // §19.2.1.3 step 5.a.ii.1: check arguments immutability
        if direct && !is_global {
            let has_arguments_decl = declared_func_names.iter().any(|n| n == "arguments")
                || declared_var_names.iter().any(|n| n == "arguments");
            if has_arguments_decl && var_env.borrow().arguments_immutable {
                return Err(self.create_error(
                    "SyntaxError",
                    "Cannot declare 'arguments' in eval inside a function with non-simple parameters",
                ));
            }
        }

        if !strict {
            // §19.2.1.4 step 5.a: if varEnv is global, check for lexical conflicts
            // Only check for true lexical declarations (let/const/class), not built-in
            // value properties like NaN/Infinity/undefined which are stored as Const
            // but are part of the object environment record, not the declarative record.
            if is_global {
                let all_names: Vec<String> = declared_func_names
                    .iter()
                    .chain(declared_var_names.iter())
                    .cloned()
                    .collect();
                let env_b = var_env.borrow();
                let global_obj = env_b.global_object.clone();
                for name in &all_names {
                    if let Some(binding) = env_b.bindings.get(name)
                        && matches!(binding.kind, BindingKind::Let | BindingKind::Const)
                    {
                        let on_global_obj = global_obj
                            .as_ref()
                            .is_some_and(|g| g.borrow().properties.contains_key(name));
                        if !on_global_obj {
                            return Err(self.create_error(
                                "SyntaxError",
                                &format!("Identifier '{}' has already been declared", name),
                            ));
                        }
                    }
                }
            }
            // Check for conflicts with lexical declarations in intermediate scopes
            // (between lex_env/caller_env and var_env)
            if !is_global {
                let all_names: Vec<String> = declared_func_names
                    .iter()
                    .chain(declared_var_names.iter())
                    .cloned()
                    .collect();
                // Walk from caller_env up to (but not including) var_env
                let mut check_env: Option<EnvRef> = if direct {
                    Some(caller_env.clone())
                } else {
                    None
                };
                while let Some(env) = check_env {
                    if Rc::ptr_eq(&env, var_env) {
                        break;
                    }
                    for name in &all_names {
                        if env.borrow().bindings.contains_key(name) {
                            return Err(self.create_error(
                                "SyntaxError",
                                &format!("Identifier '{}' has already been declared", name),
                            ));
                        }
                    }
                    let next = env.borrow().parent.clone();
                    check_env = next;
                }
            }
        }

        // Check CanDeclareGlobalFunction / CanDeclareGlobalVar for global context
        if is_global {
            let global_obj = var_env.borrow().global_object.clone();
            if let Some(ref gobj) = global_obj {
                let gb = gobj.borrow();
                let extensible = gb.extensible;
                for fname in &declared_func_names {
                    if let Some(desc) = gb.properties.get(fname) {
                        if desc.configurable != Some(true) {
                            let is_valid_data = desc.value.is_some()
                                && desc.writable == Some(true)
                                && desc.enumerable == Some(true);
                            if !is_valid_data {
                                return Err(self.create_type_error(&format!(
                                    "Cannot declare global function '{}'",
                                    fname
                                )));
                            }
                        }
                    } else if !extensible {
                        return Err(self.create_type_error(&format!(
                            "Cannot define global function '{}'",
                            fname
                        )));
                    }
                }
                for vname in &declared_var_names {
                    if !gb.properties.contains_key(vname) && !extensible {
                        return Err(self.create_type_error(&format!(
                            "Cannot define global variable '{}'",
                            vname
                        )));
                    }
                }
            }
        }

        // Hoist function declarations to var_env
        for f in &functions_to_init {
            let enclosing_strict = lex_env.borrow().strict;
            let func = JsFunction::User {
                name: Some(f.name.clone()),
                params: f.params.clone(),
                body: f.body.clone(),
                closure: lex_env.clone(),
                is_arrow: false,
                is_strict: f.body_is_strict || enclosing_strict,
                is_generator: f.is_generator,
                is_async: f.is_async,
                is_method: false,
                source_text: f.source_text.clone(),
            };
            let val = self.create_function(func);
            if is_global {
                var_env
                    .borrow_mut()
                    .declare_global_function_binding(&f.name, val, true);
            } else {
                if !var_env.borrow().bindings.contains_key(&f.name) {
                    var_env
                        .borrow_mut()
                        .declare_deletable(&f.name, BindingKind::Var);
                }
                let _ = var_env.borrow_mut().set(&f.name, val);
            }
        }

        // Pre-instantiate lexical declarations (let/const/class) in lex_env — uninitialized (TDZ)
        // Per spec §19.2.1.4 step 14
        for stmt in body {
            match stmt {
                Statement::Variable(decl) if matches!(decl.kind, VarKind::Let | VarKind::Const) => {
                    let kind = if decl.kind == VarKind::Const {
                        BindingKind::Const
                    } else {
                        BindingKind::Let
                    };
                    for d in &decl.declarations {
                        let mut names = Vec::new();
                        Self::collect_pattern_names(&d.pattern, &mut names);
                        for name in names {
                            lex_env.borrow_mut().bindings.insert(
                                name,
                                Binding {
                                    value: JsValue::Undefined,
                                    kind,
                                    initialized: false,
                                    deletable: false,
                                },
                            );
                        }
                    }
                }
                Statement::ClassDeclaration(cls) => {
                    lex_env.borrow_mut().bindings.insert(
                        cls.name.clone(),
                        Binding {
                            value: JsValue::Undefined,
                            kind: BindingKind::Let,
                            initialized: false,
                            deletable: false,
                        },
                    );
                }
                _ => {}
            }
        }

        // Hoist var declarations to var_env
        for name in &declared_var_names {
            if !var_env.borrow().bindings.contains_key(name) {
                if is_global {
                    var_env.borrow_mut().declare_global_var_configurable(name);
                } else {
                    var_env
                        .borrow_mut()
                        .declare_deletable(name, BindingKind::Var);
                }
            }
        }

        // B.3.3.3: Annex B block-level function hoisting for eval
        if !strict {
            let mut annexb_names = Vec::new();
            let mut annexb_blocked = Vec::new();
            Self::collect_annexb_function_names(body, &mut annexb_names, &mut annexb_blocked);

            if !annexb_names.is_empty() {
                let mut eval_lexical_names = Vec::new();
                for stmt in body {
                    match stmt {
                        Statement::Variable(decl)
                            if matches!(decl.kind, VarKind::Let | VarKind::Const) =>
                        {
                            for d in &decl.declarations {
                                Self::collect_pattern_names(&d.pattern, &mut eval_lexical_names);
                            }
                        }
                        Statement::ClassDeclaration(cls) => {
                            eval_lexical_names.push(cls.name.clone());
                        }
                        _ => {}
                    }
                }

                let declared_func_or_var: Vec<String> = declared_func_names
                    .iter()
                    .chain(declared_var_names.iter())
                    .cloned()
                    .collect();

                let mut registered = Vec::new();
                for name in annexb_names {
                    if eval_lexical_names.contains(&name) {
                        continue;
                    }

                    if !declared_func_or_var.contains(&name) {
                        if direct && !is_global {
                            let mut conflict = false;
                            let mut check_env: Option<EnvRef> = Some(caller_env.clone());
                            while let Some(env) = check_env {
                                if Rc::ptr_eq(&env, var_env) {
                                    break;
                                }
                                if env.borrow().bindings.contains_key(&name) {
                                    conflict = true;
                                    break;
                                }
                                let next = env.borrow().parent.clone();
                                check_env = next;
                            }
                            if conflict {
                                continue;
                            }
                        }

                        if is_global {
                            if !var_env.borrow().bindings.contains_key(&name) {
                                var_env.borrow_mut().declare_global_var_configurable(&name);
                            }
                        } else if !var_env.borrow().bindings.contains_key(&name) {
                            var_env
                                .borrow_mut()
                                .declare_deletable(&name, BindingKind::Var);
                        }
                    }

                    if !registered.contains(&name) {
                        registered.push(name);
                    }
                }

                if !registered.is_empty() {
                    let mut existing = var_env
                        .borrow_mut()
                        .annexb_function_names
                        .take()
                        .unwrap_or_default();
                    for name in registered {
                        if !existing.contains(&name) {
                            existing.push(name);
                        }
                    }
                    var_env.borrow_mut().annexb_function_names = Some(existing);
                }
            }
        }

        Ok(())
    }

    fn eval_new(&mut self, callee: &Expression, args: &[Expression], env: &EnvRef) -> Completion {
        let callee_val = match self.eval_expr(callee, env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        let evaluated_args = match self.eval_spread_args(args, env) {
            Ok(args) => args,
            Err(e) => return Completion::Throw(e),
        };
        // Check if callee is a constructor
        if let JsValue::Object(ref co) = callee_val {
            let is_proxy = self.get_proxy_info(co.id).is_some();
            if !is_proxy && let Some(func_obj) = self.get_object(co.id) {
                let b = func_obj.borrow();
                let is_ctor = match &b.callable {
                    Some(JsFunction::User {
                        is_arrow,
                        is_generator,
                        is_async,
                        is_method,
                        ..
                    }) => !is_arrow && !is_method && !is_generator && !is_async,
                    Some(JsFunction::Native(_, _, _, is_ctor)) => *is_ctor,
                    None => false,
                };
                if !is_ctor {
                    let name = match &b.callable {
                        Some(JsFunction::Native(n, _, _, _)) => n.clone(),
                        Some(JsFunction::User { name, .. }) => name.clone().unwrap_or_default(),
                        None => String::new(),
                    };
                    drop(b);
                    return Completion::Throw(
                        self.create_type_error(&format!("{} is not a constructor", name)),
                    );
                }
            }
        } else {
            return Completion::Throw(
                self.create_type_error(&format!("{:?} is not a constructor", callee_val)),
            );
        }
        // Proxy construct trap
        if let JsValue::Object(ref co) = callee_val
            && self.get_proxy_info(co.id).is_some()
        {
            let target_val = self.get_proxy_target_val(co.id);
            let args_array = self.create_array(evaluated_args.clone());
            let new_target = callee_val.clone();
            match self.invoke_proxy_trap(
                co.id,
                "construct",
                vec![target_val.clone(), args_array, new_target.clone()],
            ) {
                Ok(Some(v)) => {
                    if matches!(v, JsValue::Object(_)) {
                        return Completion::Normal(v);
                    }
                    return Completion::Throw(
                        self.create_type_error("'construct' on proxy: trap returned non-Object"),
                    );
                }
                Ok(None) => {
                    // No trap, forward to target with original newTarget
                    return self.construct_with_new_target(
                        &target_val,
                        &evaluated_args,
                        new_target,
                    );
                }
                Err(e) => return Completion::Throw(e),
            }
        }
        // Check if this is a derived class constructor
        let is_derived = if let JsValue::Object(o) = &callee_val
            && let Some(func_obj) = self.get_object(o.id)
        {
            func_obj.borrow().is_derived_class_constructor
        } else {
            false
        };

        if is_derived {
            // Derived constructor: don't create this, let super() handle it
            let prev_new_target = self.new_target.take();
            self.new_target = Some(callee_val.clone());
            self.last_call_had_explicit_return = false;
            self.last_call_this_value = None;
            let prev_constructing_derived = self.constructing_derived;
            self.constructing_derived = true;
            let result = self.call_function(&callee_val, &JsValue::Undefined, &evaluated_args);
            self.constructing_derived = prev_constructing_derived;
            let had_explicit_return = self.last_call_had_explicit_return;
            let final_this = self.last_call_this_value.take();
            self.new_target = prev_new_target;
            match result {
                Completion::Normal(v) if had_explicit_return && matches!(v, JsValue::Object(_)) => {
                    Completion::Normal(v)
                }
                Completion::Normal(ref v) if had_explicit_return && !matches!(v, JsValue::Undefined) => {
                    Completion::Throw(self.create_type_error(
                        "Derived constructors may only return object or undefined",
                    ))
                }
                Completion::Normal(_) | Completion::Empty => {
                    match final_this {
                        Some(v) if matches!(v, JsValue::Object(_)) => Completion::Normal(v),
                        Some(v) if !matches!(v, JsValue::Undefined) => Completion::Normal(v),
                        _ => {
                            Completion::Throw(self.create_reference_error(
                                "Must call super constructor in derived class before accessing 'this' or returning from derived constructor",
                            ))
                        }
                    }
                }
                other => other,
            }
        } else {
            // Base constructor: create this object as before
            let new_obj = self.create_object();
            if let JsValue::Object(o) = &callee_val
                && let Some(func_obj) = self.get_object(o.id)
            {
                let proto = func_obj.borrow().get_property_value("prototype");
                if let Some(JsValue::Object(proto_obj)) = proto
                    && let Some(proto_rc) = self.get_object(proto_obj.id)
                {
                    new_obj.borrow_mut().prototype = Some(proto_rc);
                }
            }
            let instance_field_defs = if let JsValue::Object(o) = &callee_val
                && let Some(func_obj) = self.get_object(o.id)
            {
                func_obj.borrow().class_instance_field_defs.clone()
            } else {
                Vec::new()
            };
            let new_obj_id = new_obj.borrow().id.unwrap();
            let this_val = JsValue::Object(crate::types::JsObject { id: new_obj_id });
            let init_env = Environment::new(Some(env.clone()));
            init_env.borrow_mut().declare("this", BindingKind::Const);
            let _ = init_env.borrow_mut().set("this", this_val.clone());
            init_env.borrow_mut().is_field_initializer = true;
            if let JsValue::Object(o) = &callee_val
                && let Some(func_obj) = self.get_object(o.id)
            {
                if let Some(JsFunction::User { ref closure, .. }) = func_obj.borrow().callable {
                    let cls_env = closure.borrow();
                    if let Some(ref names) = cls_env.class_private_names {
                        init_env.borrow_mut().class_private_names = Some(names.clone());
                    }
                }
                // Set __home_object__ for super property access in field initializers.
                let proto_val = func_obj.borrow().get_property("prototype");
                if matches!(&proto_val, JsValue::Object(_)) {
                    init_env.borrow_mut().bindings.insert(
                        "__home_object__".to_string(),
                        Binding {
                            value: proto_val,
                            kind: BindingKind::Const,
                            initialized: true,
                            deletable: false,
                        },
                    );
                }
            }
            // Pass 1: Install private methods and accessors first.
            for idef in &instance_field_defs {
                match idef {
                    InstanceFieldDef::Private(PrivateFieldDef::Method { name, value }) => {
                        if let Some(obj) = self.get_object(new_obj_id) {
                            obj.borrow_mut()
                                .private_fields
                                .insert(name.clone(), PrivateElement::Method(value.clone()));
                        }
                    }
                    InstanceFieldDef::Private(PrivateFieldDef::Accessor { name, get, set }) => {
                        if let Some(obj) = self.get_object(new_obj_id) {
                            obj.borrow_mut().private_fields.insert(
                                name.clone(),
                                PrivateElement::Accessor {
                                    get: get.clone(),
                                    set: set.clone(),
                                },
                            );
                        }
                    }
                    _ => {}
                }
            }
            // Pass 2: Run field initializers in source order.
            for idef in &instance_field_defs {
                match idef {
                    InstanceFieldDef::Private(PrivateFieldDef::Field { name, initializer }) => {
                        let val = if let Some(init) = initializer {
                            match self.eval_expr(init, &init_env) {
                                Completion::Normal(v) => v,
                                other => return other,
                            }
                        } else {
                            JsValue::Undefined
                        };
                        if let Some(obj) = self.get_object(new_obj_id) {
                            obj.borrow_mut()
                                .private_fields
                                .insert(name.clone(), PrivateElement::Field(val));
                        }
                    }
                    InstanceFieldDef::Public(key, initializer) => {
                        let val = if let Some(init) = initializer {
                            match self.eval_expr(init, &init_env) {
                                Completion::Normal(v) => v,
                                other => return other,
                            }
                        } else {
                            JsValue::Undefined
                        };
                        if let Some(obj) = self.get_object(new_obj_id) {
                            obj.borrow_mut().insert_value(key.clone(), val);
                        }
                    }
                    _ => {} // Methods/accessors handled in pass 1
                }
            }
            let prev_new_target = self.new_target.take();
            self.new_target = Some(callee_val.clone());
            self.last_call_had_explicit_return = false;
            self.last_call_this_value = None;
            let result = self.call_function(&callee_val, &this_val, &evaluated_args);
            let had_explicit_return = self.last_call_had_explicit_return;
            let final_this = self.last_call_this_value.take().unwrap_or(this_val.clone());
            self.new_target = prev_new_target;
            match result {
                Completion::Normal(v) if had_explicit_return && matches!(v, JsValue::Object(_)) => {
                    Completion::Normal(v)
                }
                Completion::Normal(_) | Completion::Empty => Completion::Normal(final_this),
                other => other,
            }
        }
    }

    pub(crate) fn construct(&mut self, constructor: &JsValue, args: &[JsValue]) -> Completion {
        self.construct_with_new_target(constructor, args, constructor.clone())
    }

    /// Construct with a specific new.target (needed for super() calls where new.target
    /// must be the derived class, not the parent constructor).
    pub(crate) fn construct_with_new_target(
        &mut self,
        constructor: &JsValue,
        args: &[JsValue],
        new_target: JsValue,
    ) -> Completion {
        let co = if let JsValue::Object(co) = constructor {
            co.clone()
        } else {
            return Completion::Throw(self.create_type_error("not a constructor"));
        };

        // Proxy construct trap
        if self.get_proxy_info(co.id).is_some() {
            let target_val = self.get_proxy_target_val(co.id);
            let args_array = self.create_array(args.to_vec());
            let nt = new_target.clone();
            match self.invoke_proxy_trap(
                co.id,
                "construct",
                vec![target_val.clone(), args_array, nt],
            ) {
                Ok(Some(v)) => {
                    if matches!(v, JsValue::Object(_)) {
                        return Completion::Normal(v);
                    }
                    return Completion::Throw(
                        self.create_type_error("'construct' on proxy: trap returned non-Object"),
                    );
                }
                Ok(None) => {
                    // No trap, forward to target with original newTarget
                    return self.construct_with_new_target(&target_val, args, new_target);
                }
                Err(e) => return Completion::Throw(e),
            }
        }

        // Bound function [[Construct]]: resolve newTarget through bound chain
        if let Some(func_obj) = self.get_object(co.id) {
            let b = func_obj.borrow();
            if let Some(target) = b.bound_target_function.clone() {
                let ba = b.bound_args.clone().unwrap_or_default();
                drop(b);
                let mut all_args = ba;
                all_args.extend_from_slice(args);
                let resolved_nt = if same_value(constructor, &new_target) {
                    target.clone()
                } else {
                    new_target
                };
                return self.construct_with_new_target(&target, &all_args, resolved_nt);
            }
        }

        // Check is_constructor
        if let Some(func_obj) = self.get_object(co.id) {
            let b = func_obj.borrow();
            let is_ctor = match &b.callable {
                Some(JsFunction::User {
                    is_arrow,
                    is_generator,
                    is_async,
                    ..
                }) => !is_arrow && !is_generator && !is_async,
                Some(JsFunction::Native(_, _, _, is_ctor)) => *is_ctor,
                None => false,
            };
            if !is_ctor {
                drop(b);
                return Completion::Throw(self.create_type_error("not a constructor"));
            }
        }

        let is_derived = if let Some(func_obj) = self.get_object(co.id) {
            func_obj.borrow().is_derived_class_constructor
        } else {
            false
        };

        if is_derived {
            let prev_new_target = self.new_target.take();
            self.new_target = Some(new_target.clone());
            self.last_call_had_explicit_return = false;
            self.last_call_this_value = None;
            let prev_constructing_derived = self.constructing_derived;
            self.constructing_derived = true;
            let result = self.call_function(constructor, &JsValue::Undefined, args);
            self.constructing_derived = prev_constructing_derived;
            let had_explicit_return = self.last_call_had_explicit_return;
            let final_this = self.last_call_this_value.take();
            self.new_target = prev_new_target;
            match result {
                Completion::Normal(v) if had_explicit_return && matches!(v, JsValue::Object(_)) => {
                    Completion::Normal(v)
                }
                Completion::Normal(ref v) if had_explicit_return && !matches!(v, JsValue::Undefined) => {
                    Completion::Throw(self.create_type_error(
                        "Derived constructors may only return object or undefined",
                    ))
                }
                Completion::Normal(_) | Completion::Empty => {
                    match final_this {
                        Some(v) if matches!(v, JsValue::Object(_)) => Completion::Normal(v),
                        Some(v) if !matches!(v, JsValue::Undefined) => Completion::Normal(v),
                        _ => {
                            Completion::Throw(self.create_reference_error(
                                "Must call super constructor in derived class before accessing 'this' or returning from derived constructor",
                            ))
                        }
                    }
                }
                other => other,
            }
        } else {
            let new_obj = self.create_object();
            // Use new_target's .prototype for the new object's [[Prototype]]
            // Must use get_object_property to invoke proxy get traps
            if let JsValue::Object(nt_o) = &new_target {
                let nt_val = new_target.clone();
                let proto = match self.get_object_property(nt_o.id, "prototype", &nt_val) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Completion::Throw(e),
                    _ => JsValue::Undefined,
                };
                if let JsValue::Object(proto_obj) = proto
                    && let Some(proto_rc) = self.get_object(proto_obj.id)
                {
                    new_obj.borrow_mut().prototype = Some(proto_rc);
                }
            }
            let new_obj_id = new_obj.borrow().id.unwrap();
            let this_val = JsValue::Object(crate::types::JsObject { id: new_obj_id });

            // Initialize instance fields from the constructor's class_instance_field_defs.
            let instance_field_defs = if let JsValue::Object(co) = constructor
                && let Some(func_obj) = self.get_object(co.id)
            {
                func_obj.borrow().class_instance_field_defs.clone()
            } else {
                Vec::new()
            };
            if !instance_field_defs.is_empty() {
                let (class_pn, proto_val, outer_env) =
                    if let JsValue::Object(co) = constructor
                        && let Some(func_obj) = self.get_object(co.id)
                    {
                        let (pn, oe) =
                            if let Some(JsFunction::User { ref closure, .. }) =
                                func_obj.borrow().callable
                            {
                                let cls_env = closure.borrow();
                                (cls_env.class_private_names.clone(), cls_env.parent.clone())
                            } else {
                                (None, None)
                            };
                        let pv = func_obj.borrow().get_property("prototype");
                        (pn, pv, oe)
                    } else {
                        (None, JsValue::Undefined, None)
                    };
                let init_parent =
                    outer_env.unwrap_or_else(|| Environment::new_function_scope(None));
                let init_env = Environment::new(Some(init_parent));
                init_env.borrow_mut().declare("this", BindingKind::Const);
                let _ = init_env.borrow_mut().set("this", this_val.clone());
                init_env.borrow_mut().is_field_initializer = true;
                init_env.borrow_mut().class_private_names = class_pn;
                if matches!(&proto_val, JsValue::Object(_)) {
                    init_env.borrow_mut().bindings.insert(
                        "__home_object__".to_string(),
                        Binding {
                            value: proto_val,
                            kind: BindingKind::Const,
                            initialized: true,
                            deletable: false,
                        },
                    );
                }
                // Pass 1: Install private methods and accessors first.
                for idef in &instance_field_defs {
                    match idef {
                        InstanceFieldDef::Private(PrivateFieldDef::Method { name, value }) => {
                            if let Some(obj) = self.get_object(new_obj_id) {
                                obj.borrow_mut().private_fields.insert(
                                    name.clone(),
                                    PrivateElement::Method(value.clone()),
                                );
                            }
                        }
                        InstanceFieldDef::Private(PrivateFieldDef::Accessor {
                            name,
                            get,
                            set,
                        }) => {
                            if let Some(obj) = self.get_object(new_obj_id) {
                                obj.borrow_mut().private_fields.insert(
                                    name.clone(),
                                    PrivateElement::Accessor {
                                        get: get.clone(),
                                        set: set.clone(),
                                    },
                                );
                            }
                        }
                        _ => {}
                    }
                }
                // Pass 2: Run field initializers in source order.
                for idef in &instance_field_defs {
                    match idef {
                        InstanceFieldDef::Private(PrivateFieldDef::Field {
                            name,
                            initializer,
                        }) => {
                            let val = if let Some(init) = initializer {
                                match self.eval_expr(init, &init_env) {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                }
                            } else {
                                JsValue::Undefined
                            };
                            if let Some(obj) = self.get_object(new_obj_id) {
                                obj.borrow_mut()
                                    .private_fields
                                    .insert(name.clone(), PrivateElement::Field(val));
                            }
                        }
                        InstanceFieldDef::Public(key, initializer) => {
                            let val = if let Some(init) = initializer {
                                match self.eval_expr(init, &init_env) {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                }
                            } else {
                                JsValue::Undefined
                            };
                            if let Some(obj) = self.get_object(new_obj_id) {
                                obj.borrow_mut().insert_value(key.clone(), val);
                            }
                        }
                        _ => {} // Methods/accessors handled in pass 1
                    }
                }
            }

            let prev_new_target = self.new_target.take();
            self.new_target = Some(new_target.clone());
            self.last_call_had_explicit_return = false;
            self.last_call_this_value = None;
            let result = self.call_function(constructor, &this_val, args);
            let had_explicit_return = self.last_call_had_explicit_return;
            let final_this = self.last_call_this_value.take().unwrap_or(this_val.clone());
            self.new_target = prev_new_target;
            match result {
                Completion::Normal(v) if had_explicit_return && matches!(v, JsValue::Object(_)) => {
                    Completion::Normal(v)
                }
                Completion::Normal(_) | Completion::Empty => Completion::Normal(final_this),
                other => other,
            }
        }
    }

    // GetPrototypeFromConstructor: if new_target differs from intrinsic default,
    // set obj's prototype to new_target.prototype (using getter-aware property access).
    pub(crate) fn apply_new_target_prototype(
        &mut self,
        obj_id: u64,
        default_proto_id: Option<u64>,
    ) {
        if let Some(ref nt) = self.new_target.clone()
            && let JsValue::Object(nt_o) = nt
        {
            let nt_proto_id = if let Some(nt_obj) = self.get_object(nt_o.id) {
                nt_obj.borrow().id
            } else {
                None
            };
            let same = if let Some(dp_id) = default_proto_id {
                nt_proto_id == Some(dp_id)
            } else {
                false
            };
            if !same {
                let nt_val = nt.clone();
                let proto_val = match self.get_object_property(nt_o.id, "prototype", &nt_val) {
                    Completion::Normal(v) => v,
                    _ => return,
                };
                if let JsValue::Object(po) = proto_val
                    && let Some(proto_rc) = self.get_object(po.id)
                    && let Some(obj_rc) = self.get_object(obj_id)
                {
                    obj_rc.borrow_mut().prototype = Some(proto_rc);
                }
            }
        }
    }

    fn get_proxy_info(&self, obj_id: u64) -> Option<(bool, Option<u64>, Option<u64>)> {
        if let Some(obj) = self.get_object(obj_id) {
            let b = obj.borrow();
            if b.is_proxy() || b.proxy_revoked {
                let target_id = b.proxy_target.as_ref().and_then(|t| t.borrow().id);
                let handler_id = b.proxy_handler.as_ref().and_then(|h| h.borrow().id);
                return Some((b.proxy_revoked, target_id, handler_id));
            }
        }
        None
    }

    pub(crate) fn invoke_proxy_trap(
        &mut self,
        proxy_id: u64,
        trap_name: &str,
        args: Vec<JsValue>,
    ) -> Result<Option<JsValue>, JsValue> {
        let info = self.get_proxy_info(proxy_id);
        match info {
            Some((true, _, _)) => Err(self.create_type_error(&format!(
                "Cannot perform '{}' on a proxy that has been revoked",
                trap_name
            ))),
            Some((false, Some(_target_id), Some(handler_id))) => {
                let handler_val = JsValue::Object(crate::types::JsObject { id: handler_id });
                let trap_val = match self.get_object_property(handler_id, trap_name, &handler_val) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::Undefined,
                };
                if matches!(trap_val, JsValue::Undefined | JsValue::Null) {
                    return Ok(None); // No trap, fall through to target
                }
                if !self.is_callable(&trap_val) {
                    return Err(self.create_type_error(&format!(
                        "proxy handler's {} trap is not a function",
                        trap_name
                    )));
                }
                match self.call_function(&trap_val, &handler_val, &args) {
                    Completion::Normal(v) => Ok(Some(v)),
                    Completion::Throw(e) => Err(e),
                    _ => Ok(Some(JsValue::Undefined)),
                }
            }
            Some((false, _, _)) => Err(self.create_type_error(&format!(
                "Cannot perform '{}' on a proxy that has been revoked",
                trap_name
            ))),
            None => Ok(None),
        }
    }

    pub(crate) fn get_proxy_target_val(&self, proxy_id: u64) -> JsValue {
        if let Some(obj) = self.get_object(proxy_id) {
            let b = obj.borrow();
            if let Some(ref target) = b.proxy_target
                && let Some(tid) = target.borrow().id
            {
                return JsValue::Object(crate::types::JsObject { id: tid });
            }
        }
        JsValue::Undefined
    }

    pub(crate) fn validate_ownkeys_invariant(
        &mut self,
        trap_result: &JsValue,
        target_val: &JsValue,
    ) -> Result<(), JsValue> {
        let trap_keys: Vec<String> = if let JsValue::Object(arr) = trap_result
            && let Some(arr_obj) = self.get_object(arr.id)
        {
            let len = match arr_obj.borrow().get_property("length") {
                JsValue::Number(n) => n as usize,
                _ => 0,
            };
            (0..len)
                .map(|i| {
                    let v = arr_obj.borrow().get_property(&i.to_string());
                    to_js_string(&v)
                })
                .collect()
        } else {
            return Ok(());
        };

        if let JsValue::Object(t) = target_val
            && let Some(tobj) = self.get_object(t.id)
        {
            let target_extensible = tobj.borrow().extensible;
            let (target_nonconfig, target_config): (Vec<String>, Vec<String>) = {
                let b = tobj.borrow();
                let nc: Vec<String> = b
                    .property_order
                    .iter()
                    .filter(|k| {
                        b.properties
                            .get(*k)
                            .is_some_and(|d| d.configurable == Some(false))
                    })
                    .cloned()
                    .collect();
                let c: Vec<String> = b
                    .property_order
                    .iter()
                    .filter(|k| {
                        b.properties
                            .get(*k)
                            .is_some_and(|d| d.configurable != Some(false))
                    })
                    .cloned()
                    .collect();
                (nc, c)
            };
            let trap_set: std::collections::HashSet<&str> =
                trap_keys.iter().map(|s| s.as_str()).collect();

            for key in &target_nonconfig {
                if !trap_set.contains(key.as_str()) {
                    return Err(self.create_type_error(
                        "'ownKeys' on proxy: trap result did not include all non-configurable own keys of the proxy target",
                    ));
                }
            }

            if !target_extensible {
                let target_keys: std::collections::HashSet<&str> = target_nonconfig
                    .iter()
                    .chain(target_config.iter())
                    .map(|s| s.as_str())
                    .collect();
                for key in &trap_keys {
                    if !target_keys.contains(key.as_str()) {
                        return Err(self.create_type_error(
                            "'ownKeys' on proxy: trap returned extra keys for non-extensible proxy target",
                        ));
                    }
                }
                for key in &target_keys {
                    if !trap_set.contains(key) {
                        return Err(self.create_type_error(
                            "'ownKeys' on proxy: trap result did not include all own keys of non-extensible proxy target",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn eval_instanceof(&mut self, left: &JsValue, right: &JsValue) -> Completion {
        if !matches!(right, JsValue::Object(_)) {
            return Completion::Throw(JsValue::String(JsString::from_str(
                "Right-hand side of instanceof is not an object",
            )));
        }
        let rhs_obj = match right {
            JsValue::Object(o) => o.clone(),
            _ => unreachable!(),
        };
        let sym_key = self
            .cached_has_instance_key
            .clone()
            .or_else(|| self.get_symbol_key("hasInstance"));
        if let Some(sym_key) = sym_key {
            let method = match self.get_object_property(rhs_obj.id, &sym_key, right) {
                Completion::Normal(v) => v,
                other => return other,
            };
            if !matches!(method, JsValue::Undefined | JsValue::Null) {
                if !self.is_callable(&method) {
                    return Completion::Throw(JsValue::String(JsString::from_str(
                        "@@hasInstance is not callable",
                    )));
                }
                let result = self.call_function(&method, right, std::slice::from_ref(left));
                return match result {
                    Completion::Normal(v) => Completion::Normal(JsValue::Boolean(self.to_boolean_val(&v))),
                    other => other,
                };
            }
        }
        if !self.is_callable(right) {
            return Completion::Throw(JsValue::String(JsString::from_str(
                "Right-hand side of instanceof is not callable",
            )));
        }
        self.ordinary_has_instance(right, left)
    }

    pub(crate) fn ordinary_has_instance(&mut self, ctor: &JsValue, obj: &JsValue) -> Completion {
        // Step 2: bound function → recurse with target
        if let JsValue::Object(co) = ctor
            && let Some(obj_data) = self.get_object(co.id)
            && let Some(target) = obj_data.borrow().bound_target_function.clone()
        {
            return self.eval_instanceof(obj, &target);
        }
        if !self.is_callable(ctor) {
            return Completion::Normal(JsValue::Boolean(false));
        }
        // Step 3: If Type(O) is not Object, return false
        let JsValue::Object(lhs) = obj else {
            return Completion::Normal(JsValue::Boolean(false));
        };
        let Some(inst_obj) = self.get_object(lhs.id) else {
            return Completion::Normal(JsValue::Boolean(false));
        };
        let ctor_obj_ref = match ctor {
            JsValue::Object(o) => o.clone(),
            _ => return Completion::Normal(JsValue::Boolean(false)),
        };
        // Step 4: Let P be Get(C, "prototype")
        let proto_val = match self.get_object_property(ctor_obj_ref.id, "prototype", ctor) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Completion::Throw(e),
            _ => JsValue::Undefined,
        };
        // Step 5: If P is not Object, throw TypeError
        let JsValue::Object(proto_ref) = &proto_val else {
            return Completion::Throw(
                self.create_type_error("Function has non-object prototype in instanceof check"),
            );
        };
        let Some(proto_data) = self.get_object(proto_ref.id) else {
            return Completion::Throw(
                self.create_type_error("Function has non-object prototype in instanceof check"),
            );
        };
        let mut current = inst_obj.borrow().prototype.clone();
        while let Some(p) = current {
            if Rc::ptr_eq(&p, &proto_data) {
                return Completion::Normal(JsValue::Boolean(true));
            }
            current = p.borrow().prototype.clone();
        }
        Completion::Normal(JsValue::Boolean(false))
    }

    pub(crate) fn get_object_property(
        &mut self,
        obj_id: u64,
        key: &str,
        this_val: &JsValue,
    ) -> Completion {
        // Check if object is a proxy
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            let key_val = self.symbol_key_to_jsvalue(key);
            let receiver = this_val.clone();
            match self.invoke_proxy_trap(obj_id, "get", vec![target_val.clone(), key_val, receiver])
            {
                Ok(Some(v)) => {
                    // Invariant checks
                    if let JsValue::Object(ref t) = target_val
                        && let Some(tobj) = self.get_object(t.id)
                    {
                        let target_desc = tobj.borrow().get_own_property(key);
                        if let Some(ref desc) = target_desc
                            && desc.configurable == Some(false)
                        {
                            if desc.is_data_descriptor()
                                && desc.writable == Some(false)
                                && !same_value(
                                    &v,
                                    desc.value.as_ref().unwrap_or(&JsValue::Undefined),
                                )
                            {
                                return Completion::Throw(self.create_type_error(
                                        "'get' on proxy: property is a read-only and non-configurable data property on the proxy target but the proxy did not return its actual value",
                                    ));
                            }
                            if desc.is_accessor_descriptor()
                                && matches!(
                                    desc.get.as_ref().unwrap_or(&JsValue::Undefined),
                                    JsValue::Undefined
                                )
                                && !matches!(v, JsValue::Undefined)
                            {
                                return Completion::Throw(self.create_type_error(
                                        "'get' on proxy: property is a non-configurable accessor property on the proxy target and does not have a getter function, but the trap did not return 'undefined'",
                                    ));
                            }
                        }
                    }
                    return Completion::Normal(v);
                }
                Ok(None) => {
                    // No trap, fall through to target
                    if let JsValue::Object(ref t) = target_val {
                        return self.get_object_property(t.id, key, this_val);
                    }
                    return Completion::Normal(JsValue::Undefined);
                }
                Err(e) => return Completion::Throw(e),
            }
        }

        // Module namespace: look up live binding from environment
        if let Some(obj) = self.get_object(obj_id)
            && let Some(ref ns_data) = obj.borrow().module_namespace.clone()
        {
            // First try local binding lookup
            if let Some(binding_name) = ns_data.export_to_binding.get(key) {
                // Handle re-export binding format: *reexport:source:name
                if let Some(rest) = binding_name.strip_prefix("*reexport:") {
                    // Parse source:name format
                    if let Some(colon_idx) = rest.rfind(':') {
                        let source = &rest[..colon_idx];
                        let export_name = &rest[colon_idx + 1..];
                        // Resolve the source module
                        if let Some(ref module_path) = ns_data.module_path
                            && let Ok(resolved) =
                                self.resolve_module_specifier(source, Some(module_path))
                            && let Ok(source_mod) = self.load_module(&resolved)
                        {
                            // Get the source module's export binding to find the env variable
                            let source_ref = source_mod.borrow();
                            // Try environment lookup first for live bindings
                            if let Some(binding) = source_ref.export_bindings.get(export_name)
                                && let Some(val) = source_ref.env.borrow().get(binding)
                            {
                                return Completion::Normal(val);
                            }
                            // Fallback: direct environment lookup
                            if let Some(val) = source_ref.env.borrow().get(export_name) {
                                return Completion::Normal(val);
                            }
                            // Fallback: check exports map
                            if let Some(val) = source_ref.exports.get(export_name) {
                                return Completion::Normal(val.clone());
                            }
                        }
                    }
                } else if binding_name.starts_with("*ns:") {
                    // export * as ns from './mod' — binding stored in module.exports
                    if let Some(ref module_path) = ns_data.module_path
                        && let Some(module) = self.module_registry.get(module_path)
                        && let Some(val) = module.borrow().exports.get(key)
                    {
                        return Completion::Normal(val.clone());
                    }
                } else {
                    let val = ns_data
                        .env
                        .borrow()
                        .get(binding_name)
                        .unwrap_or(JsValue::Undefined);
                    return Completion::Normal(val);
                }
            }
            // Fallback: check module's exports directly (for re-exports)
            if let Some(ref module_path) = ns_data.module_path
                && let Some(module) = self.module_registry.get(module_path)
                && let Some(val) = module.borrow().exports.get(key)
            {
                return Completion::Normal(val.clone());
            }
        }

        // TypedArray [[Get]]: canonical numeric index strings MUST NOT walk prototype
        let is_typed_array_numeric = if let Some(obj) = self.get_object(obj_id) {
            let b = obj.borrow();
            b.typed_array_info.is_some()
                && crate::interpreter::types::canonical_numeric_index_string(key).is_some()
        } else {
            false
        };

        // Check own property first, then walk prototype chain proxy-aware
        let own_desc = if let Some(obj) = self.get_object(obj_id) {
            obj.borrow().get_own_property_full(key)
        } else {
            None
        };
        match own_desc {
            Some(ref d) if d.get.is_some() && !matches!(d.get, Some(JsValue::Undefined)) => {
                let getter = d.get.clone().unwrap();
                self.call_function(&getter, this_val, &[])
            }
            Some(ref d) if d.get.is_some() => Completion::Normal(JsValue::Undefined),
            Some(ref d) => Completion::Normal(d.value.clone().unwrap_or(JsValue::Undefined)),
            None => {
                // TypedArray: numeric index strings must not walk prototype chain
                if is_typed_array_numeric {
                    return Completion::Normal(JsValue::Undefined);
                }
                // Walk prototype chain with proxy awareness
                let proto = if let Some(obj) = self.get_object(obj_id) {
                    obj.borrow().prototype.clone()
                } else {
                    None
                };
                if let Some(proto_rc) = proto {
                    let proto_id = proto_rc.borrow().id.unwrap();
                    self.get_object_property(proto_id, key, this_val)
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
        }
    }

    /// Proxy-aware [[HasProperty]] - checks proxy `has` trap, recurses on target if no trap.
    pub(crate) fn proxy_has_property(&mut self, obj_id: u64, key: &str) -> Result<bool, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            let key_val = self.symbol_key_to_jsvalue(key);
            match self.invoke_proxy_trap(obj_id, "has", vec![target_val.clone(), key_val]) {
                Ok(Some(v)) => {
                    let trap_result = self.to_boolean_val(&v);
                    if !trap_result
                        && let JsValue::Object(ref t) = target_val
                        && let Some(tobj) = self.get_object(t.id)
                    {
                        let target_desc = tobj.borrow().get_own_property(key);
                        if let Some(ref desc) = target_desc {
                            if desc.configurable == Some(false) {
                                return Err(self.create_type_error(
                                        "'has' on proxy: trap returned falsish for property which exists in the proxy target as non-configurable",
                                    ));
                            }
                            if !tobj.borrow().extensible {
                                return Err(self.create_type_error(
                                        "'has' on proxy: trap returned falsish for property but the proxy target is not extensible",
                                    ));
                            }
                        }
                    }
                    Ok(trap_result)
                }
                Ok(None) => {
                    if let JsValue::Object(ref t) = target_val {
                        return self.proxy_has_property(t.id, key);
                    }
                    Ok(false)
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object(obj_id) {
            // TypedArray §10.4.5.3 [[HasProperty]]: numeric indices handled by IsValidIntegerIndex only
            {
                let b = obj.borrow();
                if b.typed_array_info.is_some()
                    && let Some(index) = crate::interpreter::types::canonical_numeric_index_string(key)
                {
                    return Ok(is_valid_integer_index(b.typed_array_info.as_ref().unwrap(), index));
                }
            }
            if obj.borrow().has_own_property(key) {
                return Ok(true);
            }
            // Walk prototype chain, checking for proxies
            let proto = obj.borrow().prototype.clone();
            if let Some(proto_rc) = proto {
                let proto_id = proto_rc.borrow().id.unwrap();
                return self.proxy_has_property(proto_id, key);
            }
            Ok(false)
        } else {
            Ok(false)
        }
    }

    // === New with-scope reference semantics (spec-compliant) ===

    /// Dynamically fetch @@unscopables from `obj_id` and check if `name` is blocked.
    fn check_unscopables_dynamic(&mut self, obj_id: u64, name: &str) -> Result<bool, JsValue> {
        let unscopables_val = {
            let this_val = JsValue::Object(crate::types::JsObject { id: obj_id });
            let key = "Symbol(Symbol.unscopables)";
            match self.get_object_property(obj_id, key, &this_val) {
                Completion::Normal(v) => {
                    if matches!(v, JsValue::Undefined) {
                        let key2 = "[Symbol.unscopables]";
                        match self.get_object_property(obj_id, key2, &this_val) {
                            Completion::Normal(v2) => v2,
                            Completion::Throw(e) => return Err(e),
                            _ => JsValue::Undefined,
                        }
                    } else {
                        v
                    }
                }
                Completion::Throw(e) => return Err(e),
                _ => JsValue::Undefined,
            }
        };
        if let JsValue::Object(u_ref) = &unscopables_val {
            let u_this = unscopables_val.clone();
            match self.get_object_property(u_ref.id, name, &u_this) {
                Completion::Normal(v) => Ok(self.to_boolean_val(&v)),
                Completion::Throw(e) => Err(e),
                _ => Ok(false),
            }
        } else {
            Ok(false)
        }
    }

    /// HasBinding for with-scopes: walks env chain, for each with-scope checks
    /// proxy_has_property + check_unscopables_dynamic. Returns Ok(Some(obj_id)) if
    /// the name resolves to a with-object, Ok(None) if found in a regular binding
    /// or not found at all, Err on trap error.
    pub(crate) fn resolve_with_has_binding(
        &mut self,
        name: &str,
        env: &EnvRef,
    ) -> Result<Option<u64>, JsValue> {
        let mut current = Some(env.clone());
        while let Some(env_ref) = current {
            let env_borrow = env_ref.borrow();
            if let Some(ref with) = env_borrow.with_object {
                let obj_id = with.obj_id;
                drop(env_borrow);
                match self.proxy_has_property(obj_id, name) {
                    Ok(true) => {
                        if !self.check_unscopables_dynamic(obj_id, name)? {
                            return Ok(Some(obj_id));
                        }
                    }
                    Ok(false) => {}
                    Err(e) => return Err(e),
                }
                let env_borrow = env_ref.borrow();
                current = env_borrow.parent.clone();
                continue;
            }
            if env_borrow.bindings.contains_key(name) {
                return Ok(None);
            }
            if env_borrow.global_object.is_some() {
                return Ok(None);
            }
            current = env_borrow.parent.clone();
        }
        Ok(None)
    }

    /// GetBindingValue for a known with-object: checks HasProperty(stillExists) + Get.
    /// No unscopables check (already done in HasBinding).
    fn with_get_binding_value(&mut self, obj_id: u64, name: &str, strict: bool) -> Completion {
        match self.proxy_has_property(obj_id, name) {
            Ok(true) => {
                let this_val = JsValue::Object(crate::types::JsObject { id: obj_id });
                self.get_object_property(obj_id, name, &this_val)
            }
            Ok(false) => {
                if strict {
                    Completion::Throw(
                        self.create_reference_error(&format!("{name} is not defined")),
                    )
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            Err(e) => Completion::Throw(e),
        }
    }

    /// SetMutableBinding for a known with-object: checks HasProperty(stillExists) + Set.
    /// No unscopables check (already done in HasBinding).
    pub(crate) fn with_set_mutable_binding(
        &mut self,
        obj_id: u64,
        name: &str,
        value: JsValue,
        strict: bool,
    ) -> Result<(), JsValue> {
        match self.proxy_has_property(obj_id, name) {
            Ok(true) => {
                let receiver = JsValue::Object(crate::types::JsObject { id: obj_id });
                self.proxy_set(obj_id, name, value, &receiver)?;
                Ok(())
            }
            Ok(false) => {
                if strict {
                    Err(self.create_reference_error(&format!("{name} is not defined")))
                } else {
                    let receiver = JsValue::Object(crate::types::JsObject { id: obj_id });
                    self.proxy_set(obj_id, name, value, &receiver)?;
                    Ok(())
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Resolve an identifier to a reference (for capturing before RHS evaluation).
    fn resolve_identifier_ref(
        &mut self,
        name: &str,
        env: &EnvRef,
    ) -> Result<IdentifierRef, JsValue> {
        match self.resolve_with_has_binding(name, env)? {
            Some(obj_id) => Ok(IdentifierRef::WithObject(obj_id)),
            None => Ok(IdentifierRef::Binding),
        }
    }

    /// Write a value through a captured identifier reference.
    fn put_value_by_ref(
        &mut self,
        name: &str,
        value: JsValue,
        id_ref: &IdentifierRef,
        env: &EnvRef,
    ) -> Completion {
        match id_ref {
            IdentifierRef::WithObject(obj_id) => {
                let strict = env.borrow().strict;
                match self.with_set_mutable_binding(*obj_id, name, value.clone(), strict) {
                    Ok(()) => Completion::Normal(value),
                    Err(e) => Completion::Throw(e),
                }
            }
            IdentifierRef::Binding => match Environment::check_set_binding(env, name) {
                SetBindingCheck::TdzError => Completion::Throw(self.create_reference_error(
                    &format!("Cannot access '{}' before initialization", name),
                )),
                SetBindingCheck::ConstAssign => {
                    Completion::Throw(self.create_type_error("Assignment to constant variable."))
                }
                SetBindingCheck::FunctionNameAssign => {
                    if env.borrow().strict {
                        Completion::Throw(
                            self.create_type_error("Assignment to constant variable."),
                        )
                    } else {
                        Completion::Normal(value)
                    }
                }
                SetBindingCheck::Unresolvable => {
                    if env.borrow().strict {
                        Completion::Throw(
                            self.create_reference_error(&format!("{name} is not defined")),
                        )
                    } else {
                        let var_scope = Environment::find_var_scope(env);
                        if !var_scope.borrow().bindings.contains_key(name) {
                            var_scope.borrow_mut().declare(name, BindingKind::Var);
                        }
                        match var_scope.borrow_mut().set(name, value.clone()) {
                            Ok(()) => Completion::Normal(value),
                            Err(_) => Completion::Throw(
                                self.create_type_error("Assignment to constant variable."),
                            ),
                        }
                    }
                }
                SetBindingCheck::Ok => match env.borrow_mut().set(name, value.clone()) {
                    Ok(()) => Completion::Normal(value),
                    Err(_) => Completion::Throw(
                        self.create_type_error("Assignment to constant variable."),
                    ),
                },
            },
        }
    }

    /// Check if a global object property has a getter and needs special handling.
    /// Returns Some(Completion) if the name resolves to a global getter property.
    /// Returns None if no getter or not a global property — caller should use env.get().
    fn resolve_global_getter(&mut self, name: &str, env: &EnvRef) -> Option<Completion> {
        let mut current = Some(env.clone());
        while let Some(env_ref) = current {
            let env_borrow = env_ref.borrow();
            if env_borrow.with_object.is_some() {
                drop(env_borrow);
                current = env_ref.borrow().parent.clone();
                continue;
            }
            if env_borrow.bindings.contains_key(name) {
                return None;
            }
            if let Some(ref global_obj) = env_borrow.global_object {
                let global_obj_clone = global_obj.clone();
                let has_getter = global_obj_clone
                    .borrow()
                    .properties
                    .get(name)
                    .is_some_and(|d| d.get.is_some());
                if has_getter && let Some(global_id) = global_obj_clone.borrow().id {
                    drop(env_borrow);
                    let this_val = JsValue::Object(crate::types::JsObject { id: global_id });
                    return Some(self.get_object_property(global_id, name, &this_val));
                }
                return None;
            }
            current = env_borrow.parent.clone();
        }
        None
    }

    /// Proxy-aware [[Set]] - checks proxy `set` trap, recurses on target if no trap.
    pub(crate) fn proxy_set(
        &mut self,
        obj_id: u64,
        key: &str,
        value: JsValue,
        receiver: &JsValue,
    ) -> Result<bool, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            let key_val = self.symbol_key_to_jsvalue(key);
            match self.invoke_proxy_trap(
                obj_id,
                "set",
                vec![target_val.clone(), key_val, value.clone(), receiver.clone()],
            ) {
                Ok(Some(v)) => {
                    if self.to_boolean_val(&v) {
                        if let JsValue::Object(ref t) = target_val
                            && let Some(tobj) = self.get_object(t.id)
                        {
                            let target_desc = tobj.borrow().get_own_property(key);
                            if let Some(ref desc) = target_desc
                                && desc.configurable == Some(false)
                            {
                                if desc.is_data_descriptor()
                                    && desc.writable == Some(false)
                                    && !same_value(
                                        &value,
                                        desc.value.as_ref().unwrap_or(&JsValue::Undefined),
                                    )
                                {
                                    return Err(self.create_type_error(
                                            "'set' on proxy: trap returned truish for property which exists in the proxy target as a non-configurable and non-writable data property with a different value",
                                        ));
                                }
                                if desc.is_accessor_descriptor()
                                    && matches!(
                                        desc.set.as_ref().unwrap_or(&JsValue::Undefined),
                                        JsValue::Undefined
                                    )
                                {
                                    return Err(self.create_type_error(
                                            "'set' on proxy: trap returned truish for property which exists in the proxy target as a non-configurable and non-writable accessor property without a setter",
                                        ));
                                }
                            }
                        }
                        Ok(true)
                    } else {
                        Ok(false)
                    }
                }
                Ok(None) => {
                    if let JsValue::Object(ref t) = target_val {
                        return self.proxy_set(t.id, key, value, receiver);
                    }
                    Ok(false)
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object(obj_id) {
            // TypedArray [[Set]]
            let is_ta = obj.borrow().typed_array_info.is_some();
            if is_ta && let Some(index) = canonical_numeric_index_string(key) {
                let is_bigint = obj
                    .borrow()
                    .typed_array_info
                    .as_ref()
                    .map(|ta| ta.kind.is_bigint())
                    .unwrap_or(false);
                let num_val = if is_bigint {
                    self.to_bigint_value(&value)?
                } else {
                    JsValue::Number(self.to_number_value(&value)?)
                };
                let obj_ref = obj.borrow();
                let ta = obj_ref.typed_array_info.as_ref().unwrap();
                if is_valid_integer_index(ta, index) {
                    let ta_clone = ta.clone();
                    drop(obj_ref);
                    typed_array_set_index(&ta_clone, index as usize, &num_val);
                }
                return Ok(true);
            }
            // OrdinarySetWithOwnDescriptor
            let own_desc = obj.borrow().get_own_property(key);
            if let Some(ref desc) = own_desc {
                if desc.is_accessor_descriptor() {
                    // Call setter with receiver as this
                    if let Some(ref setter) = desc.set
                        && !matches!(setter, JsValue::Undefined)
                    {
                        let setter = setter.clone();
                        match self.call_function(&setter, receiver, &[value]) {
                            Completion::Normal(_) => return Ok(true),
                            Completion::Throw(e) => return Err(e),
                            _ => return Ok(true),
                        }
                    }
                    return Ok(false);
                }
                // Data descriptor
                if desc.writable == Some(false) {
                    return Ok(false);
                }
                return Ok(obj.borrow_mut().set_property_value(key, value));
            }
            // No own property, walk prototype chain
            let proto = obj.borrow().prototype.clone();
            if let Some(proto_rc) = proto {
                let proto_id = proto_rc.borrow().id.unwrap();
                return self.proxy_set(proto_id, key, value, receiver);
            }
            // No prototype, create data property on receiver via [[DefineOwnProperty]]
            if let JsValue::Object(recv_o) = receiver {
                if let Some(recv_obj) = self.get_object(recv_o.id)
                    && (recv_obj.borrow().is_proxy() || recv_obj.borrow().proxy_revoked)
                {
                    // Receiver is a proxy: use proxy_define_own_property
                    let desc = crate::interpreter::types::PropertyDescriptor {
                        value: Some(value),
                        writable: Some(true),
                        enumerable: Some(true),
                        configurable: Some(true),
                        get: None,
                        set: None,
                    };
                    let desc_val = self.from_property_descriptor(&desc);
                    return self.proxy_define_own_property(recv_o.id, key.to_string(), &desc_val);
                }
                if let Some(recv_obj) = self.get_object(recv_o.id) {
                    return Ok(recv_obj.borrow_mut().set_property_value(key, value));
                }
            }
            Ok(obj.borrow_mut().set_property_value(key, value))
        } else {
            Ok(false)
        }
    }

    fn has_proxy_in_prototype_chain(&self, obj_id: u64) -> bool {
        if self.get_proxy_info(obj_id).is_some() {
            return true;
        }
        if let Some(obj) = self.get_object(obj_id)
            && let Some(ref proto) = obj.borrow().prototype
            && let Some(pid) = proto.borrow().id
        {
            return self.has_proxy_in_prototype_chain(pid);
        }
        false
    }

    /// Proxy-aware [[Delete]] - checks proxy `deleteProperty` trap, recurses on target if no trap.
    pub(crate) fn proxy_delete_property(
        &mut self,
        obj_id: u64,
        key: &str,
    ) -> Result<bool, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            let key_val = self.symbol_key_to_jsvalue(key);
            match self.invoke_proxy_trap(
                obj_id,
                "deleteProperty",
                vec![target_val.clone(), key_val],
            ) {
                Ok(Some(v)) => {
                    let trap_result = self.to_boolean_val(&v);
                    if trap_result
                        && let JsValue::Object(ref t) = target_val
                        && let Some(tobj) = self.get_object(t.id)
                    {
                        let target_desc = tobj.borrow().get_own_property(key);
                        if let Some(ref desc) = target_desc {
                            if desc.configurable == Some(false) {
                                return Err(self.create_type_error(
                                        "'deleteProperty' on proxy: trap returned truish for property which is non-configurable in the proxy target",
                                    ));
                            }
                            if !tobj.borrow().extensible {
                                return Err(self.create_type_error(
                                        "'deleteProperty' on proxy: trap returned truish for property but the proxy target is not extensible",
                                    ));
                            }
                        }
                    }
                    Ok(trap_result)
                }
                Ok(None) => {
                    if let JsValue::Object(ref t) = target_val {
                        return self.proxy_delete_property(t.id, key);
                    }
                    Ok(true)
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object(obj_id) {
            let mut m = obj.borrow_mut();
            if let Some(desc) = m.properties.get(key)
                && desc.configurable == Some(false)
            {
                return Ok(false);
            }
            m.properties.remove(key);
            m.property_order.retain(|k| k != key);
            Ok(true)
        } else {
            Ok(true)
        }
    }

    /// Proxy-aware [[DefineOwnProperty]] - checks proxy `defineProperty` trap, recurses on target if no trap.
    /// IsCompatiblePropertyDescriptor (§10.1.6.3)
    fn is_compatible_property_desc(
        _extensible: bool,
        desc: &PropertyDescriptor,
        current: &PropertyDescriptor,
    ) -> bool {
        // Step 3: If current.[[Configurable]] is false:
        if current.configurable == Some(false) {
            // 3a: If Desc.[[Configurable]] is true, return false
            if desc.configurable == Some(true) {
                return false;
            }
            // 3b: If Desc has [[Enumerable]] and it differs from current
            if let Some(desc_enum) = desc.enumerable
                && current.enumerable != Some(desc_enum)
            {
                return false;
            }
        }
        // Step 4: If IsGenericDescriptor(Desc) is true, return true
        let is_generic = !desc.is_data_descriptor() && !desc.is_accessor_descriptor();
        if is_generic {
            return true;
        }
        // Step 5: If IsDataDescriptor(current) != IsDataDescriptor(Desc)
        if current.is_data_descriptor() != desc.is_data_descriptor() {
            // 5a: If current.[[Configurable]] is false, return false
            if current.configurable == Some(false) {
                return false;
            }
            return true;
        }
        // Step 6: Both are data descriptors
        if current.is_data_descriptor() && desc.is_data_descriptor() {
            if current.configurable == Some(false) && current.writable == Some(false) {
                // 6a.i: If Desc.[[Writable]] is true, return false
                if desc.writable == Some(true) {
                    return false;
                }
                // 6a.ii: If Desc has [[Value]] and SameValue(Desc.[[Value]], current.[[Value]]) is false
                if let Some(ref desc_val) = desc.value {
                    let current_val = current.value.as_ref().unwrap_or(&JsValue::Undefined);
                    if !same_value(desc_val, current_val) {
                        return false;
                    }
                }
            }
            return true;
        }
        // Step 7: Both are accessor descriptors
        if current.configurable == Some(false) {
            // 7a.i: If Desc has [[Set]] and SameValue(Desc.[[Set]], current.[[Set]]) is false
            if let Some(ref desc_set) = desc.set {
                let current_set = current.set.as_ref().unwrap_or(&JsValue::Undefined);
                if !same_value(desc_set, current_set) {
                    return false;
                }
            }
            // 7a.ii: If Desc has [[Get]] and SameValue(Desc.[[Get]], current.[[Get]]) is false
            if let Some(ref desc_get) = desc.get {
                let current_get = current.get.as_ref().unwrap_or(&JsValue::Undefined);
                if !same_value(desc_get, current_get) {
                    return false;
                }
            }
        }
        true
    }

    pub(crate) fn proxy_define_own_property(
        &mut self,
        obj_id: u64,
        key: String,
        desc_val: &JsValue,
    ) -> Result<bool, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            let key_val = self.symbol_key_to_jsvalue(&key);
            match self.invoke_proxy_trap(
                obj_id,
                "defineProperty",
                vec![target_val.clone(), key_val, desc_val.clone()],
            ) {
                Ok(Some(v)) => {
                    let trap_result = self.to_boolean_val(&v);
                    if !trap_result {
                        return Ok(false);
                    }
                    if let JsValue::Object(ref t) = target_val
                        && let Some(tobj) = self.get_object(t.id)
                    {
                        let target_desc = tobj.borrow().get_own_property(&key);
                        let target_extensible = tobj.borrow().extensible;
                        let desc = self.to_property_descriptor(desc_val).ok();
                        let setting_config_false =
                            desc.as_ref().is_some_and(|d| d.configurable == Some(false));

                        if let Some(ref desc) = desc {
                            // Step 19: targetDesc is undefined
                            if target_desc.is_none() {
                                if !target_extensible {
                                    return Err(self.create_type_error(
                                        "'defineProperty' on proxy: trap returned truish for adding property to the non-extensible proxy target",
                                    ));
                                }
                                if setting_config_false {
                                    return Err(self.create_type_error(
                                        "'defineProperty' on proxy: trap returned truish for defining non-configurable property which does not exist on the proxy target",
                                    ));
                                }
                            }
                            // Step 20: targetDesc is not undefined
                            if let Some(ref td) = target_desc {
                                // 20a: IsCompatiblePropertyDescriptor check
                                if !Self::is_compatible_property_desc(target_extensible, desc, td) {
                                    return Err(self.create_type_error(
                                        "'defineProperty' on proxy: trap returned truish for property descriptor not compatible with the existing property in the proxy target",
                                    ));
                                }
                                // 20b: settingConfigFalse + target configurable
                                if setting_config_false && td.configurable == Some(true) {
                                    return Err(self.create_type_error(
                                        "'defineProperty' on proxy: trap returned truish for defining non-configurable property which is configurable in the proxy target",
                                    ));
                                }
                                // 20c: target non-configurable+writable, desc says non-writable
                                if td.is_data_descriptor()
                                    && td.configurable == Some(false)
                                    && td.writable == Some(true)
                                    && desc.writable == Some(false)
                                {
                                    return Err(self.create_type_error(
                                        "'defineProperty' on proxy: trap returned truish for setting non-writable on a non-configurable writable property in the proxy target",
                                    ));
                                }
                            }
                        }
                    }
                    Ok(true)
                }
                Ok(None) => {
                    if let JsValue::Object(ref t) = target_val {
                        return self.proxy_define_own_property(t.id, key, desc_val);
                    }
                    Ok(false)
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object(obj_id) {
            match self.to_property_descriptor(desc_val) {
                Ok(desc) => Ok(obj.borrow_mut().define_own_property(key, desc)),
                Err(Some(e)) => Err(e),
                Err(None) => Ok(false),
            }
        } else {
            Ok(false)
        }
    }

    /// Proxy-aware [[GetOwnProperty]] - checks proxy `getOwnPropertyDescriptor` trap, recurses on target if no trap.
    pub(crate) fn proxy_get_own_property_descriptor(
        &mut self,
        obj_id: u64,
        key: &str,
    ) -> Result<JsValue, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            let key_val = self.symbol_key_to_jsvalue(key);
            match self.invoke_proxy_trap(
                obj_id,
                "getOwnPropertyDescriptor",
                vec![target_val.clone(), key_val],
            ) {
                Ok(Some(v)) => {
                    // Step 11: If Type(trapResultObj) is neither Object nor Undefined, throw TypeError
                    if !matches!(v, JsValue::Object(_) | JsValue::Undefined) {
                        return Err(self.create_type_error(
                            "'getOwnPropertyDescriptor' on proxy: trap returned neither Object nor undefined",
                        ));
                    }
                    if let JsValue::Object(ref t) = target_val
                        && let Some(tobj) = self.get_object(t.id)
                    {
                        let target_desc = tobj.borrow().get_own_property(key);
                        let target_extensible = tobj.borrow().extensible;
                        if matches!(v, JsValue::Undefined) {
                            if let Some(ref td) = target_desc {
                                if td.configurable == Some(false) {
                                    return Err(self.create_type_error(
                                        "'getOwnPropertyDescriptor' on proxy: trap returned undefined for property which is non-configurable in the proxy target",
                                    ));
                                }
                                if !target_extensible {
                                    return Err(self.create_type_error(
                                        "'getOwnPropertyDescriptor' on proxy: trap returned undefined for property which exists in the non-extensible proxy target",
                                    ));
                                }
                            }
                        } else if matches!(v, JsValue::Object(_)) {
                            let trap_desc = self.to_property_descriptor(&v);
                            if let Ok(ref result_desc) = trap_desc {
                                // Step 22: If resultDesc.[[Configurable]] is false
                                if result_desc.configurable == Some(false) {
                                    // 22a: If targetDesc is undefined or targetDesc.[[Configurable]] is true
                                    if target_desc.is_none()
                                        || target_desc
                                            .as_ref()
                                            .is_some_and(|td| td.configurable == Some(true))
                                    {
                                        return Err(self.create_type_error(
                                            "'getOwnPropertyDescriptor' on proxy: trap reported non-configurable for a property that is either non-existent or configurable in the proxy target",
                                        ));
                                    }
                                }

                                if let Some(ref td) = target_desc {
                                    if td.configurable == Some(false) {
                                        // Step 21a: resultDesc configurable:true for non-configurable target
                                        if result_desc.configurable == Some(true) {
                                            return Err(self.create_type_error(
                                                "'getOwnPropertyDescriptor' on proxy: trap returned descriptor with configurable: true for non-configurable property in the proxy target",
                                            ));
                                        }
                                        // Step 21b: writable:true for non-configurable non-writable target
                                        if td.is_data_descriptor()
                                            && td.writable == Some(false)
                                            && result_desc.writable == Some(true)
                                        {
                                            return Err(self.create_type_error(
                                                "'getOwnPropertyDescriptor' on proxy: trap returned descriptor with writable: true for non-configurable non-writable property in the proxy target",
                                            ));
                                        }
                                    }
                                } else if !target_extensible {
                                    return Err(self.create_type_error(
                                        "'getOwnPropertyDescriptor' on proxy: trap returned descriptor for property which does not exist in the non-extensible proxy target",
                                    ));
                                }
                            }
                        }
                    }
                    Ok(v)
                }
                Ok(None) => {
                    if let JsValue::Object(ref t) = target_val {
                        return self.proxy_get_own_property_descriptor(t.id, key);
                    }
                    Ok(JsValue::Undefined)
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object(obj_id) {
            let desc = obj.borrow().get_own_property(key);
            match desc {
                Some(d) => Ok(self.from_property_descriptor(&d)),
                None => Ok(JsValue::Undefined),
            }
        } else {
            Ok(JsValue::Undefined)
        }
    }

    /// Proxy-aware [[OwnPropertyKeys]] - checks proxy `ownKeys` trap, recurses on target if no trap.
    /// Returns all own property keys (for getOwnPropertyNames).
    pub(crate) fn proxy_own_keys(&mut self, obj_id: u64) -> Result<Vec<JsValue>, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            match self.invoke_proxy_trap(obj_id, "ownKeys", vec![target_val.clone()]) {
                Ok(Some(v)) => {
                    if !matches!(v, JsValue::Object(_)) {
                        return Err(
                            self.create_type_error("CreateListFromArrayLike called on non-object")
                        );
                    }
                    if let JsValue::Object(arr) = &v {
                        let arr_id = arr.id;
                        // Use [[Get]] for length (spec: CreateListFromArrayLike)
                        let len_val = match self.get_object_property(arr_id, "length", &v) {
                            Completion::Normal(lv) => lv,
                            Completion::Throw(e) => return Err(e),
                            _ => JsValue::Undefined,
                        };
                        let len = match len_val {
                            JsValue::Number(n) => n as usize,
                            _ => return Err(self.create_type_error("ownKeys trap result length is not a number")),
                        };
                        // Use [[Get]] for each element
                        let mut keys: Vec<JsValue> = Vec::with_capacity(len);
                        for i in 0..len {
                            let elem = match self.get_object_property(arr_id, &i.to_string(), &v) {
                                Completion::Normal(ev) => ev,
                                Completion::Throw(e) => return Err(e),
                                _ => JsValue::Undefined,
                            };
                            keys.push(elem);
                        }
                        for key in &keys {
                            if !matches!(key, JsValue::String(_) | JsValue::Symbol(_)) {
                                return Err(self.create_type_error(
                                    "'ownKeys' on proxy: trap returned non-string/symbol key",
                                ));
                            }
                        }
                        let mut seen = std::collections::HashSet::new();
                        for key in &keys {
                            let key_str = to_property_key_string(key);
                            if !seen.insert(key_str) {
                                return Err(self.create_type_error(
                                    "'ownKeys' on proxy: trap returned duplicate entries",
                                ));
                            }
                        }
                        self.validate_ownkeys_invariant(&v, &target_val)?;
                        Ok(keys)
                    } else {
                        Ok(vec![])
                    }
                }
                Ok(None) => {
                    if let JsValue::Object(ref t) = target_val {
                        return self.proxy_own_keys(t.id);
                    }
                    Ok(vec![])
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object(obj_id) {
            // OrdinaryOwnPropertyKeys: integer indices (sorted), then string keys (in creation order), then symbol keys
            let b = obj.borrow();

            // String exotic objects (§10.4.3.3): virtual char indices included
            let is_string_wrapper = b.class_name == "String"
                && matches!(b.primitive_value, Some(JsValue::String(_)));
            let string_len = if is_string_wrapper {
                if let Some(JsValue::String(ref s)) = b.primitive_value {
                    s.code_units.len()
                } else {
                    0
                }
            } else {
                0
            };

            let mut int_keys_set: std::collections::BTreeMap<u64, String> = std::collections::BTreeMap::new();
            let mut str_keys: Vec<String> = Vec::new();
            let mut sym_keys: Vec<String> = Vec::new();

            // String exotic: char indices 0..len are virtual integer indices
            if is_string_wrapper {
                for i in 0..string_len {
                    int_keys_set.insert(i as u64, i.to_string());
                }
            }

            for k in &b.property_order {
                if k.starts_with("Symbol(") {
                    sym_keys.push(k.clone());
                } else if let Ok(n) = k.parse::<u64>() {
                    if n.to_string() == *k {
                        // This is an integer index - add/overwrite (string char indices take precedence, but we let btreemap handle uniqueness)
                        int_keys_set.insert(n, k.clone());
                    } else {
                        str_keys.push(k.clone());
                    }
                } else {
                    // Skip "length" for string wrappers - it's virtual, added separately
                    if is_string_wrapper && k == "length" {
                        continue;
                    }
                    str_keys.push(k.clone());
                }
            }

            let mut result: Vec<JsValue> = Vec::new();
            for (_, k) in int_keys_set {
                result.push(JsValue::String(JsString::from_str(&k)));
            }
            for k in str_keys {
                result.push(JsValue::String(JsString::from_str(&k)));
            }
            // String exotic: "length" is a virtual non-enumerable string key (after other str keys, before symbols)
            if is_string_wrapper {
                result.push(JsValue::String(JsString::from_str("length")));
            }
            for k in sym_keys {
                result.push(self.symbol_key_to_jsvalue(&k));
            }
            Ok(result)
        } else {
            Ok(vec![])
        }
    }

    /// Proxy-aware enumerable keys with prototype chain walk for for-in loops.
    pub(crate) fn proxy_enumerable_keys_with_proto(
        &mut self,
        obj_id: u64,
    ) -> Result<Vec<String>, JsValue> {
        let mut seen = std::collections::HashSet::new();
        let mut keys = Vec::new();
        let mut current_id = Some(obj_id);

        while let Some(cid) = current_id {
            // Get own keys for current object (proxy-aware)
            let own_keys = self.proxy_own_keys(cid)?;
            for key in &own_keys {
                if let JsValue::String(s) = key {
                    let key_str = s.to_rust_string();
                    if key_str.starts_with("Symbol(") {
                        continue;
                    }
                    if seen.contains(&key_str) {
                        continue;
                    }
                    // Check enumerability via proxy-aware [[GetOwnProperty]]
                    match self.proxy_get_own_property_descriptor(cid, &key_str) {
                        Ok(desc_val) => {
                            seen.insert(key_str.clone());
                            if !matches!(desc_val, JsValue::Undefined)
                                && let Ok(desc) = self.to_property_descriptor(&desc_val)
                                && desc.enumerable != Some(false)
                            {
                                keys.push(key_str);
                            }
                        }
                        Err(e) => return Err(e),
                    }
                }
            }

            // Walk prototype chain (proxy-aware)
            match self.proxy_get_prototype_of(cid) {
                Ok(JsValue::Object(proto_ref)) => {
                    current_id = Some(proto_ref.id);
                }
                Ok(_) => current_id = None,
                Err(e) => return Err(e),
            }
        }
        Ok(keys)
    }

    /// Proxy-aware [[GetPrototypeOf]] - checks proxy `getPrototypeOf` trap, recurses on target if no trap.
    pub(crate) fn proxy_get_prototype_of(&mut self, obj_id: u64) -> Result<JsValue, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            match self.invoke_proxy_trap(obj_id, "getPrototypeOf", vec![target_val.clone()]) {
                Ok(Some(v)) => {
                    if !matches!(v, JsValue::Object(_) | JsValue::Null) {
                        return Err(self.create_type_error(
                            "'getPrototypeOf' on proxy: trap returned neither object nor null",
                        ));
                    }
                    if let JsValue::Object(ref t) = target_val
                        && let Some(tobj) = self.get_object(t.id)
                        && !tobj.borrow().extensible
                    {
                        let actual_proto = {
                            let b = tobj.borrow();
                            if let Some(ref p) = b.prototype {
                                if let Some(pid) = p.borrow().id {
                                    JsValue::Object(crate::types::JsObject { id: pid })
                                } else {
                                    JsValue::Null
                                }
                            } else {
                                JsValue::Null
                            }
                        };
                        let same = match (&v, &actual_proto) {
                            (JsValue::Object(a), JsValue::Object(b)) => a.id == b.id,
                            (JsValue::Null, JsValue::Null) => true,
                            _ => false,
                        };
                        if !same {
                            return Err(self.create_type_error(
                                "'getPrototypeOf' on proxy: proxy target is non-extensible but the trap did not return its actual prototype",
                            ));
                        }
                    }
                    Ok(v)
                }
                Ok(None) => {
                    if let JsValue::Object(ref t) = target_val {
                        return self.proxy_get_prototype_of(t.id);
                    }
                    Ok(JsValue::Null)
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object(obj_id) {
            if let Some(proto) = &obj.borrow().prototype
                && let Some(id) = proto.borrow().id
            {
                Ok(JsValue::Object(crate::types::JsObject { id }))
            } else {
                Ok(JsValue::Null)
            }
        } else {
            Ok(JsValue::Null)
        }
    }

    /// Proxy-aware [[SetPrototypeOf]] - checks proxy `setPrototypeOf` trap, recurses on target if no trap.
    pub(crate) fn proxy_set_prototype_of(
        &mut self,
        obj_id: u64,
        proto: &JsValue,
    ) -> Result<bool, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            match self.invoke_proxy_trap(
                obj_id,
                "setPrototypeOf",
                vec![target_val.clone(), proto.clone()],
            ) {
                Ok(Some(v)) => {
                    if !self.to_boolean_val(&v) {
                        return Ok(false);
                    }
                    if let JsValue::Object(ref t) = target_val
                        && let Some(tobj) = self.get_object(t.id)
                        && !tobj.borrow().extensible
                    {
                        let actual_proto = {
                            let b = tobj.borrow();
                            if let Some(ref p) = b.prototype {
                                if let Some(pid) = p.borrow().id {
                                    JsValue::Object(crate::types::JsObject { id: pid })
                                } else {
                                    JsValue::Null
                                }
                            } else {
                                JsValue::Null
                            }
                        };
                        let same = match (proto, &actual_proto) {
                            (JsValue::Object(a), JsValue::Object(b)) => a.id == b.id,
                            (JsValue::Null, JsValue::Null) => true,
                            _ => false,
                        };
                        if !same {
                            return Err(self.create_type_error(
                                "'setPrototypeOf' on proxy: trap returned truish for setting a new prototype on the non-extensible proxy target",
                            ));
                        }
                    }
                    Ok(true)
                }
                Ok(None) => {
                    if let JsValue::Object(ref t) = target_val {
                        return self.proxy_set_prototype_of(t.id, proto);
                    }
                    Ok(true)
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object(obj_id) {
            match proto {
                JsValue::Null => {
                    obj.borrow_mut().prototype = None;
                }
                JsValue::Object(p) => {
                    if let Some(po) = self.get_object(p.id) {
                        obj.borrow_mut().prototype = Some(po);
                    }
                }
                _ => {}
            }
            Ok(true)
        } else {
            Ok(true)
        }
    }

    /// Proxy-aware [[IsExtensible]] - checks proxy `isExtensible` trap, recurses on target if no trap.
    pub(crate) fn proxy_is_extensible(&mut self, obj_id: u64) -> Result<bool, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            match self.invoke_proxy_trap(obj_id, "isExtensible", vec![target_val.clone()]) {
                Ok(Some(v)) => {
                    let trap_result = self.to_boolean_val(&v);
                    if let JsValue::Object(ref t) = target_val
                        && let Some(tobj) = self.get_object(t.id)
                    {
                        let target_extensible = tobj.borrow().extensible;
                        if trap_result != target_extensible {
                            return Err(self.create_type_error(
                                "'isExtensible' on proxy: trap result does not reflect extensibility of proxy target",
                            ));
                        }
                    }
                    Ok(trap_result)
                }
                Ok(None) => {
                    if let JsValue::Object(ref t) = target_val {
                        return self.proxy_is_extensible(t.id);
                    }
                    Ok(false)
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object(obj_id) {
            Ok(obj.borrow().extensible)
        } else {
            Ok(false)
        }
    }

    /// Proxy-aware [[PreventExtensions]] - checks proxy `preventExtensions` trap, recurses on target if no trap.
    pub(crate) fn proxy_prevent_extensions(&mut self, obj_id: u64) -> Result<bool, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            match self.invoke_proxy_trap(obj_id, "preventExtensions", vec![target_val.clone()]) {
                Ok(Some(v)) => {
                    let trap_result = self.to_boolean_val(&v);
                    if trap_result
                        && let JsValue::Object(ref t) = target_val
                        && let Some(tobj) = self.get_object(t.id)
                        && tobj.borrow().extensible
                    {
                        return Err(self.create_type_error(
                                "'preventExtensions' on proxy: trap returned truish but the proxy target is extensible",
                            ));
                    }
                    Ok(trap_result)
                }
                Ok(None) => {
                    if let JsValue::Object(ref t) = target_val {
                        return self.proxy_prevent_extensions(t.id);
                    }
                    Ok(false)
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object(obj_id) {
            obj.borrow_mut().extensible = false;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn eval_member(&mut self, obj: &Expression, prop: &MemberProperty, env: &EnvRef) -> Completion {
        let obj_val = match self.eval_expr(obj, env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        if let MemberProperty::Private(name) = prop {
            let branded = self.resolve_private_name(name, env);
            return match &obj_val {
                JsValue::Object(o) => {
                    if let Some(obj) = self.get_object(o.id) {
                        let elem = obj.borrow().private_fields.get(&branded).cloned();
                        match elem {
                            Some(PrivateElement::Field(v)) | Some(PrivateElement::Method(v)) => {
                                Completion::Normal(v)
                            }
                            Some(PrivateElement::Accessor { get, .. }) => {
                                if let Some(getter) = get {
                                    self.call_function(&getter, &obj_val, &[])
                                } else {
                                    Completion::Throw(self.create_type_error(&format!(
                                        "Cannot read private member #{name} which has no getter"
                                    )))
                                }
                            }
                            None => Completion::Throw(self.create_type_error(&format!(
                                "Cannot read private member #{name} from an object whose class did not declare it"
                            ))),
                        }
                    } else {
                        Completion::Normal(JsValue::Undefined)
                    }
                }
                _ => Completion::Throw(self.create_type_error(&format!(
                    "Cannot read private member #{name} from a non-object"
                ))),
            };
        }
        // For computed properties, evaluate the expression but defer ToPropertyKey
        // until after we check that the base is not null/undefined (spec: ToObject
        // precedes ToPropertyKey per §6.2.5.5 GetValue step 3.a vs 3.c.i).
        let (key, computed_raw) = match prop {
            MemberProperty::Dot(name) => (name.clone(), None),
            MemberProperty::Computed(expr) => {
                let v = match self.eval_expr(expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                // Check for null/undefined base before ToPropertyKey (skip for super)
                if !matches!(obj, Expression::Super)
                    && matches!(&obj_val, JsValue::Null | JsValue::Undefined)
                {
                    let err = self.create_type_error(&format!(
                        "Cannot read properties of {obj_val} (reading property)"
                    ));
                    return Completion::Throw(err);
                }
                let key = match self.to_property_key(&v) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                (key, Some(()))
            }
            MemberProperty::Private(_) => unreachable!(),
        };
        let _ = computed_raw;
        // super.x - look up on [[Prototype]] of HomeObject
        if matches!(obj, Expression::Super) {
            let this_val = env.borrow().get("this").unwrap_or(JsValue::Undefined);
            let home = env.borrow().get("__home_object__");
            if let Some(JsValue::Object(ref ho)) = home
                && let Some(home_obj) = self.get_object(ho.id)
            {
                if let Some(ref proto_rc) = home_obj.borrow().prototype.clone() {
                    let proto_id = proto_rc.borrow().id.unwrap();
                    return self.get_object_property(proto_id, &key, &this_val);
                }
                return Completion::Throw(self.create_type_error(&format!(
                    "Cannot read properties of null (reading '{key}')"
                )));
            }
            // Fallback: __super__.prototype for class super
            if let JsValue::Object(ref o) = obj_val
                && let Some(sup_obj) = self.get_object(o.id)
            {
                let proto_val = sup_obj.borrow().get_property("prototype");
                if let JsValue::Object(ref p) = proto_val {
                    return self.get_object_property(p.id, &key, &this_val);
                }
            }
            return Completion::Normal(JsValue::Undefined);
        }
        match &obj_val {
            JsValue::Object(o) => self.get_object_property(o.id, &key, &obj_val.clone()),
            JsValue::String(s) => {
                if key == "length" {
                    Completion::Normal(JsValue::Number(s.len() as f64))
                } else if let Ok(idx) = key.parse::<usize>() {
                    let ch = s.to_rust_string().chars().nth(idx);
                    match ch {
                        Some(c) => {
                            Completion::Normal(JsValue::String(JsString::from_str(&c.to_string())))
                        }
                        None => Completion::Normal(JsValue::Undefined),
                    }
                } else if let Some(ref sp) = self.realm().string_prototype {
                    Completion::Normal(sp.borrow().get_property(&key))
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            JsValue::Symbol(_) => {
                if let Some(ref sp) = self.realm().symbol_prototype {
                    let desc = sp.borrow().get_property_descriptor(&key);
                    match desc {
                        Some(ref d) if d.get.is_some() => {
                            let getter = d.get.clone().unwrap();
                            self.call_function(&getter, &obj_val, &[])
                        }
                        Some(ref d) => {
                            Completion::Normal(d.value.clone().unwrap_or(JsValue::Undefined))
                        }
                        None => Completion::Normal(JsValue::Undefined),
                    }
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            JsValue::Number(_) => {
                if let Some(ref np) = self.realm().number_prototype {
                    Completion::Normal(np.borrow().get_property(&key))
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            JsValue::Boolean(_) => {
                if let Some(ref bp) = self.realm().boolean_prototype {
                    Completion::Normal(bp.borrow().get_property(&key))
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            JsValue::BigInt(_) => {
                if let Some(ref bp) = self.realm().bigint_prototype {
                    Completion::Normal(bp.borrow().get_property(&key))
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            JsValue::Undefined | JsValue::Null => {
                let err = self.create_type_error(&format!(
                    "Cannot read properties of {obj_val} (reading '{key}')"
                ));
                Completion::Throw(err)
            }
        }
    }

    fn eval_array_literal(&mut self, elements: &[Option<Expression>], env: &EnvRef) -> Completion {
        let mut items: Vec<Option<JsValue>> = Vec::new();
        for elem in elements {
            match elem {
                Some(Expression::Spread(inner)) => {
                    let val = match self.eval_expr(inner, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    match self.iterate_to_vec(&val) {
                        Ok(spread_items) => {
                            for item in spread_items {
                                items.push(Some(item));
                            }
                        }
                        Err(e) => return Completion::Throw(e),
                    }
                }
                Some(expr) => {
                    let val = match self.eval_expr(expr, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    items.push(Some(val));
                }
                None => items.push(None), // elision — no own property
            }
        }
        Completion::Normal(self.create_array_with_holes(items))
    }

    pub(crate) fn eval_class(
        &mut self,
        name: &str,
        super_class: &Option<Box<Expression>>,
        body: &[ClassElement],
        env: &EnvRef,
        class_source_text: Option<String>,
    ) -> Completion {
        let brand_id = self.next_class_brand_id;
        self.next_class_brand_id += 1;
        let mut pn_set = std::collections::HashMap::new();
        for elem in body {
            match elem {
                ClassElement::Method(m) => {
                    if let PropertyKey::Private(n) = &m.key {
                        pn_set
                            .entry(n.clone())
                            .or_insert_with(|| format!("{n}#{brand_id}"));
                    }
                }
                ClassElement::Property(p) => {
                    if let PropertyKey::Private(n) = &p.key {
                        pn_set
                            .entry(n.clone())
                            .or_insert_with(|| format!("{n}#{brand_id}"));
                    }
                }
                ClassElement::StaticBlock(_) => {}
            }
        }
        self.class_private_names.push(pn_set);
        let result = self.eval_class_inner(name, super_class, body, env, class_source_text);
        self.class_private_names.pop();
        result
    }

    fn eval_class_inner(
        &mut self,
        name: &str,
        super_class: &Option<Box<Expression>>,
        body: &[ClassElement],
        env: &EnvRef,
        class_source_text: Option<String>,
    ) -> Completion {
        // Find constructor method
        let ctor_method = body.iter().find_map(|elem| {
            if let ClassElement::Method(m) = elem
                && m.kind == ClassMethodKind::Constructor
            {
                return Some(m);
            }
            None
        });

        // Evaluate super class if present
        let super_val = if let Some(sc) = super_class {
            match self.eval_expr(sc, env) {
                Completion::Normal(v) => Some(v),
                other => return other,
            }
        } else {
            None
        };

        // Validate super class: must be null or a constructor
        if let Some(ref sv) = super_val
            && !matches!(sv, JsValue::Null)
            && !self.is_constructor(sv)
        {
            return Completion::Throw(
                self.create_type_error("Class extends value is not a constructor or null"),
            );
        }

        // Create class environment with __super__ binding and private names
        let class_env = Environment::new(Some(env.clone()));
        class_env.borrow_mut().class_private_names = self.class_private_names.last().cloned();
        if let Some(ref sv) = super_val {
            class_env
                .borrow_mut()
                .declare("__super__", BindingKind::Const);
            let _ = class_env.borrow_mut().set("__super__", sv.clone());
        }

        // Create constructor function (classes are always strict mode)
        let ctor_func = if let Some(cm) = ctor_method {
            JsFunction::User {
                name: Some(name.to_string()),
                params: cm.value.params.clone(),
                body: cm.value.body.clone(),
                closure: class_env.clone(),
                is_arrow: false,
                is_strict: true,
                is_generator: false,
                is_async: false,
                is_method: false,
                source_text: class_source_text.clone(),
            }
        } else if super_val.is_some() {
            JsFunction::User {
                name: Some(name.to_string()),
                params: vec![Pattern::Rest(Box::new(Pattern::Identifier("args".into())))],
                body: vec![Statement::Expression(Expression::Call(
                    Box::new(Expression::Super),
                    vec![Expression::Spread(Box::new(Expression::Identifier(
                        "args".into(),
                    )))],
                ))],
                closure: class_env.clone(),
                is_arrow: false,
                is_strict: true,
                is_generator: false,
                is_async: false,
                is_method: false,
                source_text: class_source_text.clone(),
            }
        } else {
            JsFunction::User {
                name: Some(name.to_string()),
                params: vec![],
                body: vec![],
                closure: class_env.clone(),
                is_arrow: false,
                is_strict: true,
                is_generator: false,
                is_async: false,
                is_method: false,
                source_text: class_source_text.clone(),
            }
        };

        let ctor_val = self.create_function(ctor_func);

        // Mark derived class constructors and make .prototype writable:false
        if let JsValue::Object(ref o) = ctor_val
            && let Some(func_obj) = self.get_object(o.id)
        {
            func_obj.borrow_mut().is_class_constructor = true;
            if super_val.is_some() {
                func_obj.borrow_mut().is_derived_class_constructor = true;
            }
            // Per spec §14.6.13: class .prototype is {writable: false, enumerable: false, configurable: false}
            let proto_val_for_desc = func_obj.borrow().get_property("prototype");
            func_obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val_for_desc, false, false, false),
            );
        }

        // Bind class name as immutable binding in class_env (spec §15.7.14 step 2.e)
        if !name.is_empty() {
            class_env.borrow_mut().declare(name, BindingKind::Const);
            let _ = class_env.borrow_mut().set(name, ctor_val.clone());
        }

        // Get the prototype object that was auto-created by create_function
        let proto_obj = if let JsValue::Object(ref o) = ctor_val {
            if let Some(func_obj) = self.get_object(o.id) {
                let proto_val = func_obj.borrow().get_property("prototype");
                if let JsValue::Object(ref p) = proto_val {
                    self.get_object(p.id)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // Set up inheritance
        if let Some(ref sv) = super_val
            && let JsValue::Object(super_o) = sv
            && let Some(super_obj) = self.get_object(super_o.id)
        {
            // Step 5.e: proto.[[Prototype]] = superclass.prototype
            let super_proto_val = super_obj.borrow().get_property("prototype");
            if let JsValue::Object(ref sp) = super_proto_val
                && let Some(super_proto) = self.get_object(sp.id)
                && let Some(ref proto) = proto_obj
            {
                proto.borrow_mut().prototype = Some(super_proto);
            }
            // Step 7.a: F.[[Prototype]] = superclass (for static method inheritance)
            if let JsValue::Object(ref o) = ctor_val
                && let Some(ctor_obj) = self.get_object(o.id)
            {
                ctor_obj.borrow_mut().prototype = Some(super_obj);
            }
        }

        // Handle `extends null` — set prototype's [[Prototype]] to null
        if let Some(JsValue::Null) = super_val
            && let Some(ref proto) = proto_obj
        {
            proto.borrow_mut().prototype = None;
        }

        // Create environment for static field initializers with `this` = constructor
        let static_field_env = Environment::new_function_scope(Some(class_env.clone()));
        {
            let mut sfe = static_field_env.borrow_mut();
            sfe.bindings.insert(
                "this".to_string(),
                Binding {
                    value: ctor_val.clone(),
                    kind: BindingKind::Const,
                    initialized: true,
                    deletable: false,
                },
            );
            sfe.is_field_initializer = true;
            sfe.class_private_names = self.class_private_names.last().cloned();
            // Set __home_object__ for super property access in static field initializers.
            // Static field HomeObject = constructor.
            sfe.bindings.insert(
                "__home_object__".to_string(),
                Binding {
                    value: ctor_val.clone(),
                    kind: BindingKind::Const,
                    initialized: true,
                    deletable: false,
                },
            );
        }

        // Per spec §15.7.14 step 28-34: Process all elements in two phases.
        // Phase 1: Evaluate ALL computed keys in declaration order, install methods,
        //          collect instance field defs, and defer static fields/blocks.
        // Phase 2: Execute static field initializers and static blocks in order.
        enum DeferredStatic {
            PublicField(String, Option<Expression>),
            PrivateField(String, Option<Expression>),
            Block(Vec<Statement>),
        }
        let mut deferred_static: Vec<DeferredStatic> = Vec::new();

        for elem in body {
            match elem {
                ClassElement::Method(m) => {
                    if m.kind == ClassMethodKind::Constructor {
                        continue;
                    }
                    let (key, fn_name_for_key) = match &m.key {
                        PropertyKey::Identifier(s) | PropertyKey::String(s) => {
                            (s.clone(), s.clone())
                        }
                        PropertyKey::Number(n) => {
                            let s = to_js_string(&JsValue::Number(*n));
                            (s.clone(), s)
                        }
                        PropertyKey::Computed(expr) => match self.eval_expr(expr, env) {
                            Completion::Normal(v) => {
                                let is_symbol = matches!(&v, JsValue::Symbol(_));
                                let fn_name = if let JsValue::Symbol(ref sym) = v {
                                    match &sym.description {
                                        Some(desc) => format!("[{}]", desc),
                                        None => String::new(),
                                    }
                                } else {
                                    String::new()
                                };
                                match self.to_property_key(&v) {
                                    Ok(s) => {
                                        let name = if is_symbol { fn_name } else { s.clone() };
                                        (s, name)
                                    }
                                    Err(e) => return Completion::Throw(e),
                                }
                            }
                            other => return other,
                        },
                        PropertyKey::Private(name) => {
                            let branded = self.resolve_private_name(name, &class_env);
                            let method_func = JsFunction::User {
                                name: Some(format!("#{name}")),
                                params: m.value.params.clone(),
                                body: m.value.body.clone(),
                                closure: class_env.clone(),
                                is_arrow: false,
                                is_strict: true,
                                is_generator: m.value.is_generator,
                                is_async: m.value.is_async,
                                is_method: true,
                                source_text: m.value.source_text.clone(),
                            };
                            let method_val = self.create_function(method_func);

                            if m.is_static {
                                if let JsValue::Object(ref o) = ctor_val
                                    && let Some(func_obj) = self.get_object(o.id)
                                {
                                    match m.kind {
                                        ClassMethodKind::Get => {
                                            let existing = func_obj
                                                .borrow()
                                                .private_fields
                                                .get(&branded)
                                                .cloned();
                                            let elem = if let Some(PrivateElement::Accessor {
                                                get: _,
                                                set,
                                            }) = existing
                                            {
                                                PrivateElement::Accessor {
                                                    get: Some(method_val),
                                                    set,
                                                }
                                            } else {
                                                PrivateElement::Accessor {
                                                    get: Some(method_val),
                                                    set: None,
                                                }
                                            };
                                            func_obj
                                                .borrow_mut()
                                                .private_fields
                                                .insert(branded.clone(), elem);
                                        }
                                        ClassMethodKind::Set => {
                                            let existing = func_obj
                                                .borrow()
                                                .private_fields
                                                .get(&branded)
                                                .cloned();
                                            let elem = if let Some(PrivateElement::Accessor {
                                                get,
                                                set: _,
                                            }) = existing
                                            {
                                                PrivateElement::Accessor {
                                                    get,
                                                    set: Some(method_val),
                                                }
                                            } else {
                                                PrivateElement::Accessor {
                                                    get: None,
                                                    set: Some(method_val),
                                                }
                                            };
                                            func_obj
                                                .borrow_mut()
                                                .private_fields
                                                .insert(branded.clone(), elem);
                                        }
                                        _ => {
                                            func_obj.borrow_mut().private_fields.insert(
                                                branded.clone(),
                                                PrivateElement::Method(method_val),
                                            );
                                        }
                                    }
                                }
                            } else if let JsValue::Object(ref o) = ctor_val
                                && let Some(func_obj) = self.get_object(o.id)
                            {
                                match m.kind {
                                    ClassMethodKind::Get => {
                                        let mut b = func_obj.borrow_mut();
                                        let mut found = false;
                                        for idef in b.class_instance_field_defs.iter_mut() {
                                            if let InstanceFieldDef::Private(
                                                PrivateFieldDef::Accessor {
                                                    name: n, get: g, ..
                                                },
                                            ) = idef
                                                && *n == branded
                                            {
                                                *g = Some(method_val.clone());
                                                found = true;
                                                break;
                                            }
                                        }
                                        if !found {
                                            b.class_instance_field_defs.push(
                                                InstanceFieldDef::Private(
                                                    PrivateFieldDef::Accessor {
                                                        name: branded.clone(),
                                                        get: Some(method_val),
                                                        set: None,
                                                    },
                                                ),
                                            );
                                        }
                                    }
                                    ClassMethodKind::Set => {
                                        let mut b = func_obj.borrow_mut();
                                        let mut found = false;
                                        for idef in b.class_instance_field_defs.iter_mut() {
                                            if let InstanceFieldDef::Private(
                                                PrivateFieldDef::Accessor {
                                                    name: n, set: s, ..
                                                },
                                            ) = idef
                                                && *n == branded
                                            {
                                                *s = Some(method_val.clone());
                                                found = true;
                                                break;
                                            }
                                        }
                                        if !found {
                                            b.class_instance_field_defs.push(
                                                InstanceFieldDef::Private(
                                                    PrivateFieldDef::Accessor {
                                                        name: branded.clone(),
                                                        get: None,
                                                        set: Some(method_val),
                                                    },
                                                ),
                                            );
                                        }
                                    }
                                    _ => {
                                        func_obj.borrow_mut().class_instance_field_defs.push(
                                            InstanceFieldDef::Private(PrivateFieldDef::Method {
                                                name: branded.clone(),
                                                value: method_val,
                                            }),
                                        );
                                    }
                                }
                            }
                            continue;
                        }
                    };
                    let method_display_name = match m.kind {
                        ClassMethodKind::Get => format!("get {fn_name_for_key}"),
                        ClassMethodKind::Set => format!("set {fn_name_for_key}"),
                        _ => fn_name_for_key.clone(),
                    };
                    let method_func = JsFunction::User {
                        name: Some(method_display_name),
                        params: m.value.params.clone(),
                        body: m.value.body.clone(),
                        closure: class_env.clone(),
                        is_arrow: false,
                        is_strict: true,
                        is_generator: m.value.is_generator,
                        is_async: m.value.is_async,
                        is_method: true,
                        source_text: m.value.source_text.clone(),
                    };
                    let method_val = self.create_function(method_func);

                    // Set __home_object__ for super property access
                    let home_target = if m.is_static {
                        ctor_val.clone()
                    } else if let Some(ref p) = proto_obj {
                        let pid = p.borrow().id.unwrap();
                        JsValue::Object(crate::types::JsObject { id: pid })
                    } else {
                        JsValue::Undefined
                    };
                    if let JsValue::Object(ref fo) = method_val
                        && let Some(func_obj) = self.get_object(fo.id)
                        && let Some(JsFunction::User { ref closure, .. }) =
                            func_obj.borrow().callable
                    {
                        closure
                            .borrow_mut()
                            .declare("__home_object__", BindingKind::Const);
                        let _ = closure.borrow_mut().set("__home_object__", home_target);
                    }

                    let target = if m.is_static {
                        if let JsValue::Object(ref o) = ctor_val {
                            self.get_object(o.id)
                        } else {
                            None
                        }
                    } else {
                        proto_obj.clone()
                    };
                    if let Some(ref t) = target {
                        match m.kind {
                            ClassMethodKind::Get => {
                                let mut desc = t.borrow().properties.get(&key).cloned().unwrap_or(
                                    PropertyDescriptor {
                                        value: None,
                                        writable: None,
                                        get: None,
                                        set: None,
                                        enumerable: Some(false),
                                        configurable: Some(true),
                                    },
                                );
                                desc.get = Some(method_val);
                                desc.value = None;
                                desc.writable = None;
                                t.borrow_mut().insert_property(key, desc);
                            }
                            ClassMethodKind::Set => {
                                let mut desc = t.borrow().properties.get(&key).cloned().unwrap_or(
                                    PropertyDescriptor {
                                        value: None,
                                        writable: None,
                                        get: None,
                                        set: None,
                                        enumerable: Some(false),
                                        configurable: Some(true),
                                    },
                                );
                                desc.set = Some(method_val);
                                desc.value = None;
                                desc.writable = None;
                                t.borrow_mut().insert_property(key, desc);
                            }
                            _ => {
                                t.borrow_mut().insert_builtin(key, method_val);
                            }
                        }
                    }
                }
                ClassElement::Property(p) => {
                    // Check if this is a private field
                    if let PropertyKey::Private(name) = &p.key {
                        let branded = self.resolve_private_name(name, &class_env);
                        if !p.is_static {
                            // Store instance private field definition
                            if let JsValue::Object(ref o) = ctor_val
                                && let Some(func_obj) = self.get_object(o.id)
                            {
                                func_obj.borrow_mut().class_instance_field_defs.push(
                                    InstanceFieldDef::Private(PrivateFieldDef::Field {
                                        name: branded,
                                        initializer: p.value.clone(),
                                    }),
                                );
                            }
                        } else {
                            // Defer static private field initializer to phase 2
                            deferred_static
                                .push(DeferredStatic::PrivateField(branded, p.value.clone()));
                        }
                        continue;
                    }
                    if p.is_static {
                        // Evaluate computed key NOW in phase 1, defer initializer to phase 2
                        let key = match &p.key {
                            PropertyKey::Identifier(s) | PropertyKey::String(s) => s.clone(),
                            PropertyKey::Number(n) => to_js_string(&JsValue::Number(*n)),
                            PropertyKey::Computed(expr) => match self.eval_expr(expr, env) {
                                Completion::Normal(v) => match self.to_property_key(&v) {
                                    Ok(s) => s,
                                    Err(e) => return Completion::Throw(e),
                                },
                                other => return other,
                            },
                            PropertyKey::Private(_) => unreachable!(),
                        };
                        if key == "prototype" {
                            return Completion::Throw(self.create_type_error(
                                "Classes may not have a static property named 'prototype'",
                            ));
                        }
                        deferred_static.push(DeferredStatic::PublicField(key, p.value.clone()));
                    } else {
                        // Instance field: evaluate computed key, store field def
                        let key = match &p.key {
                            PropertyKey::Identifier(s) | PropertyKey::String(s) => s.clone(),
                            PropertyKey::Number(n) => to_js_string(&JsValue::Number(*n)),
                            PropertyKey::Computed(expr) => match self.eval_expr(expr, env) {
                                Completion::Normal(v) => match self.to_property_key(&v) {
                                    Ok(s) => s,
                                    Err(e) => return Completion::Throw(e),
                                },
                                other => return other,
                            },
                            PropertyKey::Private(_) => unreachable!(),
                        };
                        if let JsValue::Object(ref o) = ctor_val
                            && let Some(func_obj) = self.get_object(o.id)
                        {
                            func_obj
                                .borrow_mut()
                                .class_instance_field_defs
                                .push(InstanceFieldDef::Public(key, p.value.clone()));
                        }
                    }
                }
                ClassElement::StaticBlock(stmts) => {
                    // Defer static block execution to phase 2
                    deferred_static.push(DeferredStatic::Block(stmts.clone()));
                }
            }
        }

        // Phase 2: Execute deferred static field initializers and static blocks
        for deferred in deferred_static {
            match deferred {
                DeferredStatic::PublicField(key, initializer) => {
                    let val = if let Some(ref expr) = initializer {
                        match self.eval_expr(expr, &static_field_env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        }
                    } else {
                        JsValue::Undefined
                    };
                    if let JsValue::Object(ref o) = ctor_val
                        && let Some(func_obj) = self.get_object(o.id)
                    {
                        func_obj.borrow_mut().insert_value(key, val);
                    }
                }
                DeferredStatic::PrivateField(branded, initializer) => {
                    let val = if let Some(ref expr) = initializer {
                        match self.eval_expr(expr, &static_field_env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        }
                    } else {
                        JsValue::Undefined
                    };
                    if let JsValue::Object(ref o) = ctor_val
                        && let Some(func_obj) = self.get_object(o.id)
                    {
                        func_obj
                            .borrow_mut()
                            .private_fields
                            .insert(branded, PrivateElement::Field(val));
                    }
                }
                DeferredStatic::Block(stmts) => {
                    let block_env = Environment::new_function_scope(Some(class_env.clone()));
                    {
                        let mut be = block_env.borrow_mut();
                        be.strict = true; // class bodies are always strict
                        be.bindings.insert(
                            "this".to_string(),
                            Binding {
                                value: ctor_val.clone(),
                                kind: BindingKind::Const,
                                initialized: true,
                                deletable: false,
                            },
                        );
                        be.bindings.insert(
                            "__home_object__".to_string(),
                            Binding {
                                value: ctor_val.clone(),
                                kind: BindingKind::Const,
                                initialized: true,
                                deletable: false,
                            },
                        );
                        be.class_private_names = self.class_private_names.last().cloned();
                    }
                    match self.exec_statements(&stmts, &block_env) {
                        Completion::Normal(_) => {}
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => {}
                    }
                }
            }
        }

        Completion::Normal(ctor_val)
    }

    /// CopyDataProperties (§7.3.26) — copies own enumerable properties from source
    /// to target obj_data. Properly handles Proxy traps and Symbol keys.
    pub(crate) fn copy_data_properties(
        &mut self,
        src_id: u64,
        src_val: &JsValue,
        excluded: &[String],
    ) -> Result<Vec<(String, JsValue)>, JsValue> {
        let mut result = Vec::new();
        let keys = self.proxy_own_keys(src_id)?;
        for key_val in keys {
            let key_str = self.to_property_key(&key_val)?;
            if excluded.contains(&key_str) {
                continue;
            }
            let is_enumerable = if self.get_proxy_info(src_id).is_some() {
                let target_proxy_val = self.get_proxy_target_val(src_id);
                match self.invoke_proxy_trap(
                    src_id,
                    "getOwnPropertyDescriptor",
                    vec![target_proxy_val, key_val.clone()],
                ) {
                    Ok(Some(v)) => {
                        if v.is_undefined() {
                            continue;
                        }
                        if let JsValue::Object(ref dobj) = v
                            && let Some(desc_rc) = self.get_object(dobj.id)
                        {
                            match desc_rc.borrow().get_property_value("enumerable") {
                                Some(ev) => self.to_boolean_val(&ev),
                                None => false,
                            }
                        } else {
                            continue;
                        }
                    }
                    Ok(None) => {
                        if let Some(obj) = self.get_object(src_id) {
                            let desc = obj.borrow().get_own_property(&key_str);
                            match desc {
                                Some(d) => d.enumerable != Some(false),
                                None => continue,
                            }
                        } else {
                            continue;
                        }
                    }
                    Err(e) => return Err(e),
                }
            } else if let Some(obj) = self.get_object(src_id) {
                let desc = obj.borrow().get_own_property(&key_str);
                match desc {
                    Some(d) => d.enumerable != Some(false),
                    None => continue,
                }
            } else {
                continue;
            };
            if !is_enumerable {
                continue;
            }
            let val = match self.get_object_property(src_id, &key_str, src_val) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::Undefined,
            };
            result.push((key_str, val));
        }
        Ok(result)
    }

    fn eval_object_literal(&mut self, props: &[Property], env: &EnvRef) -> Completion {
        let mut obj_data = JsObjectData::new();
        obj_data.prototype = self.realm().object_prototype.clone();
        for prop in props {
            let (key, fn_name_for_key) = match &prop.key {
                PropertyKey::Identifier(n) => (n.clone(), n.clone()),
                PropertyKey::String(s) => (s.clone(), s.clone()),
                PropertyKey::Number(n) => {
                    let s = number_ops::to_string(*n);
                    (s.clone(), s)
                }
                PropertyKey::Computed(expr) => {
                    let v = match self.eval_expr(expr, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let is_symbol = matches!(&v, JsValue::Symbol(_));
                    let fn_name = if let JsValue::Symbol(ref sym) = v {
                        match &sym.description {
                            Some(desc) => format!("[{}]", desc),
                            None => String::new(),
                        }
                    } else {
                        String::new()
                    };
                    let pk = match self.to_property_key(&v) {
                        Ok(s) => s,
                        Err(e) => return Completion::Throw(e),
                    };
                    let name = if is_symbol { fn_name } else { pk.clone() };
                    (pk, name)
                }
                PropertyKey::Private(_) => {
                    return Completion::Throw(
                        self.create_type_error("Private names are not valid in object literals"),
                    );
                }
            };
            let value = match self.eval_expr(&prop.value, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            // Handle spread — CopyDataProperties (§7.3.26)
            if let Expression::Spread(inner) = &prop.value {
                let spread_val = match self.eval_expr(inner, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if let JsValue::Object(ref o) = spread_val {
                    let src_id = o.id;
                    match self.copy_data_properties(src_id, &spread_val, &[]) {
                        Ok(pairs) => {
                            for (k, v) in pairs {
                                obj_data.insert_value(k, v);
                            }
                        }
                        Err(e) => return Completion::Throw(e),
                    }
                }
                continue;
            }
            match prop.kind {
                PropertyKind::Get => {
                    self.set_function_name(&value, &format!("get {fn_name_for_key}"));
                    let mut desc =
                        obj_data
                            .properties
                            .get(&key)
                            .cloned()
                            .unwrap_or(PropertyDescriptor {
                                value: None,
                                writable: None,
                                get: None,
                                set: None,
                                enumerable: Some(true),
                                configurable: Some(true),
                            });
                    desc.get = Some(value);
                    desc.value = None;
                    desc.writable = None;
                    obj_data.insert_property(key, desc);
                }
                PropertyKind::Set => {
                    self.set_function_name(&value, &format!("set {fn_name_for_key}"));
                    let mut desc =
                        obj_data
                            .properties
                            .get(&key)
                            .cloned()
                            .unwrap_or(PropertyDescriptor {
                                value: None,
                                writable: None,
                                get: None,
                                set: None,
                                enumerable: Some(true),
                                configurable: Some(true),
                            });
                    desc.set = Some(value);
                    desc.value = None;
                    desc.writable = None;
                    obj_data.insert_property(key, desc);
                }
                _ => {
                    // __proto__: value sets [[Prototype]] per spec §13.2.5.5
                    if key == "__proto__" && !prop.computed && !prop.shorthand {
                        match &value {
                            JsValue::Object(o) => {
                                obj_data.prototype = self.get_object(o.id);
                            }
                            JsValue::Null => {
                                obj_data.prototype = None;
                            }
                            _ => {
                                // Non-object, non-null values are ignored per spec
                            }
                        }
                    } else {
                        if prop.value.is_anonymous_function_definition() {
                            self.set_function_name(&value, &fn_name_for_key);
                        }
                        obj_data.insert_value(key, value);
                    }
                }
            }
        }
        let obj = Rc::new(RefCell::new(obj_data));
        let id = self.allocate_object_slot(obj);
        // Set __home_object__ for concise methods, getters, and setters
        let obj_val = JsValue::Object(crate::types::JsObject { id });
        if let Some(obj_rc) = self.get_object(id) {
            let prop_values: Vec<JsValue> = {
                let b = obj_rc.borrow();
                b.properties
                    .values()
                    .flat_map(|desc| {
                        let mut vals = vec![];
                        if let Some(ref v) = desc.value {
                            vals.push(v.clone());
                        }
                        if let Some(ref g) = desc.get {
                            vals.push(g.clone());
                        }
                        if let Some(ref s) = desc.set {
                            vals.push(s.clone());
                        }
                        vals
                    })
                    .collect()
            };
            for val in &prop_values {
                if let JsValue::Object(fo) = val
                    && let Some(func_obj) = self.get_object(fo.id)
                {
                    if let Some(JsFunction::User { ref closure, .. }) = func_obj.borrow().callable {
                        closure
                            .borrow_mut()
                            .declare("__home_object__", BindingKind::Const);
                        let _ = closure.borrow_mut().set("__home_object__", obj_val.clone());
                    }
                    // Methods must not have own caller/arguments (spec §15.4)
                    func_obj.borrow_mut().properties.remove("caller");
                    func_obj.borrow_mut().properties.remove("arguments");
                }
            }
        }
        Completion::Normal(obj_val)
    }

    fn call_async_function(
        &mut self,
        params: &[Pattern],
        body: &[Statement],
        closure: EnvRef,
        is_arrow: bool,
        is_strict: bool,
        this_val: &JsValue,
        args: &[JsValue],
        func_val: &JsValue,
    ) -> Completion {
        let promise = self.create_promise_object();
        let promise_id = if let JsValue::Object(ref o) = promise {
            o.id
        } else {
            0
        };
        self.gc_root_value(&promise);
        let (resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);
        self.gc_root_value(&resolve_fn);
        self.gc_root_value(&reject_fn);

        let closure_strict = closure.borrow().strict;
        let func_env = Environment::new_function_scope(Some(closure));
        if is_arrow {
            func_env.borrow_mut().is_arrow_scope = true;
        }
        for (i, param) in params.iter().enumerate() {
            if let Pattern::Rest(inner) = param {
                let rest: Vec<JsValue> = args.get(i..).unwrap_or(&[]).to_vec();
                let rest_arr = self.create_array(rest);
                if let Err(e) = self.bind_pattern(inner, rest_arr, BindingKind::Var, &func_env) {
                    let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                    self.drain_microtasks();
                    self.gc_unroot_value(&reject_fn);
                    self.gc_unroot_value(&resolve_fn);
                    self.gc_unroot_value(&promise);
                    return Completion::Normal(promise);
                }
                break;
            }
            let val = args.get(i).cloned().unwrap_or(JsValue::Undefined);
            if let Err(e) = self.bind_pattern(param, val, BindingKind::Var, &func_env) {
                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                self.drain_microtasks();
                self.gc_unroot_value(&reject_fn);
                self.gc_unroot_value(&resolve_fn);
                self.gc_unroot_value(&promise);
                return Completion::Normal(promise);
            }
        }
        if !is_arrow {
            let effective_this = if !is_strict
                && !closure_strict
                && matches!(this_val, JsValue::Undefined | JsValue::Null)
            {
                self.realm().global_env
                    .borrow()
                    .get("this")
                    .unwrap_or(this_val.clone())
            } else {
                this_val.clone()
            };
            func_env.borrow_mut().bindings.insert(
                "this".to_string(),
                Binding {
                    value: effective_this,
                    kind: BindingKind::Const,
                    initialized: true,
                    deletable: false,
                },
            );
            let is_simple = params.iter().all(|p| matches!(p, Pattern::Identifier(_)));
            let env_strict = func_env.borrow().strict;
            let use_mapped = is_simple && !is_strict && !env_strict;
            let param_names: Vec<String> = if use_mapped {
                params
                    .iter()
                    .filter_map(|p| {
                        if let Pattern::Identifier(name) = p {
                            Some(name.clone())
                        } else {
                            None
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let mapped_env = if use_mapped { Some(&func_env) } else { None };
            let arguments_obj = self.create_arguments_object(
                args,
                func_val.clone(),
                is_strict,
                mapped_env,
                &param_names,
            );
            func_env.borrow_mut().declare("arguments", BindingKind::Var);
            let _ = func_env.borrow_mut().set("arguments", arguments_obj);
            if is_strict || !is_simple {
                func_env.borrow_mut().arguments_immutable = true;
            }
        }

        func_env.borrow_mut().strict = is_strict;
        let result = self.exec_statements(body, &func_env);
        let result = self.dispose_resources(&func_env, result);
        match result {
            Completion::Return(v) | Completion::Normal(v) => {
                let _ = self.call_function(&resolve_fn, &JsValue::Undefined, &[v]);
            }
            Completion::Empty => {
                let _ = self.call_function(&resolve_fn, &JsValue::Undefined, &[JsValue::Undefined]);
            }
            Completion::Throw(e) => {
                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
            }
            _ => {}
        }
        self.drain_microtasks();
        self.gc_unroot_value(&reject_fn);
        self.gc_unroot_value(&resolve_fn);
        self.gc_unroot_value(&promise);
        Completion::Normal(promise)
    }

    /// Spec [[Get]] — reads a property from an object, invoking getters.
    pub(crate) fn obj_get(&mut self, obj_val: &JsValue, key: &str) -> Result<JsValue, JsValue> {
        if let JsValue::Object(o) = obj_val {
            let mut current_id = Some(o.id);
            while let Some(id) = current_id {
                if let Some(obj) = self.get_object(id) {
                    let b = obj.borrow();
                    if let Some(desc) = b.properties.get(key) {
                        if let Some(ref getter) = desc.get {
                            if self.is_callable(getter) {
                                let getter = getter.clone();
                                let obj_val = obj_val.clone();
                                drop(b);
                                return match self.call_function(&getter, &obj_val, &[]) {
                                    Completion::Normal(v) => Ok(v),
                                    Completion::Throw(e) => Err(e),
                                    _ => Ok(JsValue::Undefined),
                                };
                            }
                            return Ok(JsValue::Undefined);
                        }
                        if let Some(ref val) = desc.value {
                            return Ok(val.clone());
                        }
                        return Ok(JsValue::Undefined);
                    }
                    current_id = b.prototype.as_ref().map(|p| p.borrow().id.unwrap());
                } else {
                    break;
                }
            }
        }
        Ok(JsValue::Undefined)
    }

    pub(crate) fn await_value(&mut self, val: &JsValue) -> Completion {
        if self.is_promise(val) {
            let promise_id = if let JsValue::Object(o) = val {
                o.id
            } else {
                0
            };
            self.drain_microtasks();
            match self.get_promise_state(promise_id) {
                Some(PromiseState::Fulfilled(v)) => Completion::Normal(v),
                Some(PromiseState::Rejected(r)) => Completion::Throw(r),
                Some(PromiseState::Pending) => {
                    for _ in 0..1000 {
                        if self.microtask_queue.is_empty() {
                            break;
                        }
                        self.drain_microtasks();
                        match self.get_promise_state(promise_id) {
                            Some(PromiseState::Fulfilled(v)) => return Completion::Normal(v),
                            Some(PromiseState::Rejected(r)) => return Completion::Throw(r),
                            _ => {}
                        }
                    }
                    Completion::Normal(JsValue::Undefined)
                }
                None => Completion::Normal(val.clone()),
            }
        } else if matches!(val, JsValue::Object(_)) {
            // Check for thenable using [[Get]] which triggers getters
            let then_val = match self.obj_get(val, "then") {
                Ok(v) => v,
                Err(e) => return Completion::Throw(e),
            };
            if self.is_callable(&then_val) {
                // Resolve thenable: create promise, call then(resolve, reject), await
                let promise = self.create_promise_object();
                let promise_id = if let JsValue::Object(ref o) = promise {
                    o.id
                } else {
                    0
                };
                self.promise_resolve_thenable(promise_id, val.clone(), then_val);
                return self.await_value(&promise);
            }
            Completion::Normal(val.clone())
        } else {
            Completion::Normal(val.clone())
        }
    }

    fn dynamic_import(&mut self, specifier: &str) -> Completion {
        // Resolve the module specifier
        let resolved =
            match self.resolve_module_specifier(specifier, self.current_module_path.as_deref()) {
                Ok(p) => p,
                Err(e) => {
                    // Return rejected promise
                    return self.create_rejected_promise(e);
                }
            };

        // Load the module
        let module = match self.load_module(&resolved) {
            Ok(m) => m,
            Err(e) => {
                // Return rejected promise
                return self.create_rejected_promise(e);
            }
        };

        // Create namespace object and return fulfilled promise
        let ns = self.create_module_namespace(&module);
        self.create_resolved_promise(ns)
    }
}
