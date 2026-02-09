use super::*;

impl Interpreter {
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
        let (private_field_defs, public_field_defs) = if let JsValue::Object(ref o) = new_target_val
            && let Some(func_obj) = self.get_object(o.id)
        {
            let borrowed = func_obj.borrow();
            (
                borrowed.class_private_field_defs.clone(),
                borrowed.class_public_field_defs.clone(),
            )
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
        let outer_env = if let JsValue::Object(ref o) = new_target_val
            && let Some(func_obj) = self.get_object(o.id)
        {
            if let Some(JsFunction::User { ref closure, .. }) = func_obj.borrow().callable {
                let cls_env = closure.borrow();
                cls_env.parent.clone()
            } else {
                None
            }
        } else {
            None
        };
        let init_parent = outer_env.unwrap_or_else(|| env.clone());
        let init_env = Environment::new(Some(init_parent));
        init_env.borrow_mut().bindings.insert(
            "this".to_string(),
            crate::interpreter::types::Binding {
                value: this_val.clone(),
                kind: crate::interpreter::types::BindingKind::Const,
                initialized: true,
            },
        );
        for def in &private_field_defs {
            match def {
                PrivateFieldDef::Field { name, initializer } => {
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
                        obj.borrow_mut()
                            .private_fields
                            .insert(name.clone(), PrivateElement::Field(val));
                    }
                }
                PrivateFieldDef::Method { name, value } => {
                    if let Some(obj) = self.get_object(this_obj_id) {
                        obj.borrow_mut()
                            .private_fields
                            .insert(name.clone(), PrivateElement::Method(value.clone()));
                    }
                }
                PrivateFieldDef::Accessor { name, get, set } => {
                    if let Some(obj) = self.get_object(this_obj_id) {
                        obj.borrow_mut().private_fields.insert(
                            name.clone(),
                            PrivateElement::Accessor {
                                get: get.clone(),
                                set: set.clone(),
                            },
                        );
                    }
                }
            }
        }
        for (key, initializer) in &public_field_defs {
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
                obj.borrow_mut().insert_value(key.clone(), val);
            }
        }
        Ok(())
    }

    pub(crate) fn eval_expr(&mut self, expr: &Expression, env: &EnvRef) -> Completion {
        match expr {
            Expression::Literal(lit) => Completion::Normal(self.eval_literal(lit)),
            Expression::Identifier(name) => match env.borrow().get(name) {
                Some(val) => Completion::Normal(val),
                None => {
                    let err = self.create_reference_error(&format!("{name} is not defined"));
                    Completion::Throw(err)
                }
            },
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
                    let rval = match self.eval_expr(right, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    return match &rval {
                            JsValue::Object(o) => {
                                if let Some(obj) = self.get_object(o.id) {
                                    Completion::Normal(JsValue::Boolean(
                                        obj.borrow().private_fields.contains_key(name),
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
                if to_boolean(&test_val) {
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
                let func = JsFunction::User {
                    name: f.name.clone(),
                    params: f.params.clone(),
                    body: f.body.clone(),
                    closure: env.clone(),
                    is_arrow: false,
                    is_strict: Self::is_strict_mode_body(&f.body) || env.borrow().strict,
                    is_generator: f.is_generator,
                    is_async: f.is_async,
                    source_text: f.source_text.clone(),
                };
                Completion::Normal(self.create_function(func))
            }
            Expression::ArrowFunction(af) => {
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
                    is_strict: Self::is_strict_mode_body(&body_stmts) || env.borrow().strict,
                    is_generator: false,
                    is_async: af.is_async,
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
                // typeof on unresolvable reference returns "undefined"
                // typeof on TDZ reference throws ReferenceError
                if let Expression::Identifier(name) = operand.as_ref() {
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
                    if let JsValue::Object(o) = &obj_val
                        && let Some(obj) = self.get_object(o.id)
                    {
                        // Proxy deleteProperty trap
                        if obj.borrow().is_proxy() || obj.borrow().proxy_revoked {
                            match self.proxy_delete_property(o.id, &key) {
                                Ok(result) => return Completion::Normal(JsValue::Boolean(result)),
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        // TypedArray: §10.4.5.4 [[Delete]]
                        {
                            let obj_ref = obj.borrow();
                            if let Some(ref ta) = obj_ref.typed_array_info {
                                if let Some(index) = canonical_numeric_index_string(&key) {
                                    if is_valid_integer_index(ta, index) {
                                        return Completion::Normal(JsValue::Boolean(false));
                                    }
                                    return Completion::Normal(JsValue::Boolean(true));
                                }
                            }
                        }
                        let mut obj_mut = obj.borrow_mut();
                        if let Some(desc) = obj_mut.properties.get(&key)
                            && desc.configurable == Some(false)
                        {
                            return Completion::Normal(JsValue::Boolean(false));
                        }
                        obj_mut.properties.remove(&key);
                        obj_mut.property_order.retain(|k| k != &key);
                        if let Some(ref mut map) = obj_mut.parameter_map {
                            map.remove(&key);
                        }
                    }
                    Completion::Normal(JsValue::Boolean(true))
                }
                Expression::Identifier(name) => {
                    // Per §13.5.1.2: delete on a resolved binding reference
                    // In strict mode, this is a SyntaxError (handled at parse time)
                    // In sloppy mode:
                    //   - var/let/const bindings are non-configurable → return false
                    //   - global object configurable properties → delete and return true
                    //   - unresolvable → return true

                    // Check non-global environments first — these are always non-configurable
                    let mut current = Some(env.clone());
                    let global_env = self.global_env.clone();
                    while let Some(ref e) = current {
                        if std::rc::Rc::ptr_eq(e, &global_env) {
                            break;
                        }
                        if e.borrow().bindings.contains_key(name) {
                            return Completion::Normal(JsValue::Boolean(false));
                        }
                        let next = e.borrow().parent.clone();
                        current = next;
                    }

                    // At global level — check global object property descriptor
                    let global_obj = self.global_env.borrow().global_object.clone();
                    if let Some(ref global) = global_obj {
                        let gb = global.borrow();
                        if let Some(desc) = gb.properties.get(name) {
                            if desc.configurable == Some(false) {
                                return Completion::Normal(JsValue::Boolean(false));
                            }
                            drop(gb);
                            global.borrow_mut().properties.remove(name);
                            global.borrow_mut().property_order.retain(|k| k != name);
                            self.global_env.borrow_mut().bindings.remove(name);
                            return Completion::Normal(JsValue::Boolean(true));
                        }
                    }
                    // Check if it's a binding in the global env (var declaration not on global object)
                    if self.global_env.borrow().bindings.contains_key(name) {
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
                        let (done_val, value) = if let JsValue::Object(ref ro) = next_result {
                            if let Some(robj) = self.get_object(ro.id) {
                                let d = robj.borrow().get_property("done");
                                let v = robj.borrow().get_property("value");
                                (d, v)
                            } else {
                                (JsValue::Undefined, JsValue::Undefined)
                            }
                        } else {
                            (JsValue::Undefined, JsValue::Undefined)
                        };
                        if to_boolean(&done_val) {
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
                    return Completion::Yield(value);
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
                // Create import.meta object
                let meta = self.create_object();
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
            Expression::Import(source_expr) => {
                // Dynamic import() - returns a Promise
                let source_val = match self.eval_expr(source_expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let source = to_js_string(&source_val);
                self.dynamic_import(&source)
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
                        s.push_str(&format!("{val}"));
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
                                if let JsValue::Object(ref o) = obj_val
                                    && let Some(obj) = self.get_object(o.id)
                                {
                                    let elem = obj.borrow().private_fields.get(name).cloned();
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
            _ => Completion::Normal(JsValue::Undefined),
        }
    }

    fn access_property_on_value(&mut self, base_val: &JsValue, name: &str) -> Completion {
        match base_val {
            JsValue::Object(o) => self.get_object_property(o.id, name, base_val),
            JsValue::String(s) => {
                if name == "length" {
                    Completion::Normal(JsValue::Number(s.len() as f64))
                } else if let Some(ref sp) = self.string_prototype {
                    Completion::Normal(sp.borrow().get_property(name))
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            JsValue::Number(_) => {
                if let Some(ref np) = self.number_prototype {
                    Completion::Normal(np.borrow().get_property(name))
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            JsValue::Boolean(_) => {
                if let Some(ref bp) = self.boolean_prototype {
                    Completion::Normal(bp.borrow().get_property(name))
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            _ => Completion::Normal(JsValue::Undefined),
        }
    }

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
                        if let JsValue::Object(o) = &inner_val
                            && let Some(obj) = self.get_object(o.id)
                        {
                            let elem = obj.borrow().private_fields.get(name).cloned();
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
        if let Some(&obj_id) = self.template_cache.get(&cache_key) {
            if self.get_object(obj_id).is_some() {
                return JsValue::Object(crate::types::JsObject { id: obj_id });
            }
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

        if let JsValue::Object(o) = &template_arr {
            if let Some(obj) = self.get_object(o.id) {
                obj.borrow_mut().insert_property(
                    "raw".to_string(),
                    PropertyDescriptor::data(raw_arr, false, false, false),
                );
            }
        }

        if let JsValue::Object(o) = &template_arr {
            self.template_cache.insert(cache_key, o.id);
        }

        template_arr
    }

    fn create_frozen_template_array(&mut self, values: Vec<JsValue>) -> JsValue {
        let len = values.len();
        let mut obj_data = JsObjectData::new();
        obj_data.prototype = self
            .array_prototype
            .clone()
            .or(self.object_prototype.clone());
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
            Literal::String(s) => JsValue::String(JsString::from_str(s)),
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
                obj.prototype = self
                    .regexp_prototype
                    .clone()
                    .or(self.object_prototype.clone());
                obj.class_name = "RegExp".to_string();
                let source_str = if pattern.is_empty() {
                    "(?:)".to_string()
                } else {
                    pattern.clone()
                };
                obj.insert_property(
                    "__original_source__".to_string(),
                    PropertyDescriptor::data(
                        JsValue::String(JsString::from_str(&source_str)),
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
        }
    }

    // §7.1.14 ToPropertyKey
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
        obj.prototype = self
            .regexp_prototype
            .clone()
            .or(self.object_prototype.clone());
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
                JsValue::Object(_) => Completion::Normal(JsValue::Number(number_ops::unary_minus(
                    self.to_number_coerce(val),
                ))),
                _ => Completion::Normal(JsValue::Number(number_ops::unary_minus(to_number(val)))),
            },
            UnaryOp::Plus => match val {
                JsValue::BigInt(_) => Completion::Throw(
                    self.create_type_error("Cannot convert a BigInt value to a number"),
                ),
                JsValue::Object(_) => {
                    Completion::Normal(JsValue::Number(self.to_number_coerce(val)))
                }
                _ => Completion::Normal(JsValue::Number(to_number(val))),
            },
            UnaryOp::Not => Completion::Normal(JsValue::Boolean(!to_boolean(val))),
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

    fn is_regexp(&self, val: &JsValue) -> bool {
        if let JsValue::Object(o) = val
            && let Some(obj) = self.get_object(o.id)
        {
            return obj.borrow().class_name == "RegExp";
        }
        false
    }

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

    fn to_length(val: &JsValue) -> f64 {
        let len = to_number(val);
        if len.is_nan() || len <= 0.0 {
            return 0.0;
        }
        len.min(9007199254740991.0).floor() // 2^53 - 1
    }

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
                        if let Some(ref sp) = self.string_prototype {
                            obj_data.prototype = Some(sp.clone());
                        }
                    }
                    JsValue::Number(_) => {
                        obj_data.class_name = "Number".to_string();
                        if let Some(ref np) = self.number_prototype {
                            obj_data.prototype = Some(np.clone());
                        }
                    }
                    JsValue::Boolean(_) => {
                        obj_data.class_name = "Boolean".to_string();
                        if let Some(ref bp) = self.boolean_prototype {
                            obj_data.prototype = Some(bp.clone());
                        }
                    }
                    JsValue::Symbol(_) => {
                        obj_data.class_name = "Symbol".to_string();
                        if let Some(ref sp) = self.symbol_prototype {
                            obj_data.prototype = Some(sp.clone());
                        }
                    }
                    JsValue::BigInt(_) => {
                        obj_data.class_name = "BigInt".to_string();
                        if let Some(ref bp) = self.bigint_prototype {
                            obj_data.prototype = Some(bp.clone());
                        }
                    }
                    _ => unreachable!(),
                }
                if obj_data.prototype.is_none() {
                    obj_data.prototype = self.object_prototype.clone();
                }
                let obj = Rc::new(RefCell::new(obj_data));
                let id = self.allocate_object_slot(obj);
                Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
            }
            JsValue::Object(_) => Completion::Normal(val.clone()),
        }
    }

    pub(crate) fn to_primitive(
        &mut self,
        val: &JsValue,
        preferred_type: &str,
    ) -> Result<JsValue, JsValue> {
        match val {
            JsValue::Object(o) => {
                // §7.1.1 Step 2-3: Check @@toPrimitive
                let exotic_to_prim = if let Some(obj) = self.get_object(o.id) {
                    let key = "Symbol(Symbol.toPrimitive)";
                    obj.borrow().get_property(key)
                } else {
                    JsValue::Undefined
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

    pub(crate) fn to_number_coerce(&mut self, val: &JsValue) -> f64 {
        match self.to_primitive(val, "number") {
            Ok(prim) => to_number(&prim),
            Err(_) => f64::NAN,
        }
    }

    // §7.1.17 ToString — calls ToPrimitive for objects
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

    // §7.1.4 ToNumber — calls ToPrimitive for objects
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
                    return Err(self.create_error(
                        "SyntaxError",
                        &format!("Cannot convert \"{}\" to a BigInt", text),
                    ));
                }
                match trimmed.parse::<num_bigint::BigInt>() {
                    Ok(n) => Ok(JsValue::BigInt(crate::types::JsBigInt { value: n })),
                    Err(_) => Err(self.create_error(
                        "SyntaxError",
                        &format!("Cannot convert \"{}\" to a BigInt", text),
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

    fn abstract_equality(&mut self, left: &JsValue, right: &JsValue) -> bool {
        if std::mem::discriminant(left) == std::mem::discriminant(right) {
            return strict_equality(left, right);
        }
        if (left.is_null() && right.is_undefined()) || (left.is_undefined() && right.is_null()) {
            return true;
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
                return false;
            }
            if *n != n.trunc() {
                return false;
            }
            use num_bigint::BigInt;
            let n_as_bigint = BigInt::from(*n as i64);
            return bigint_ops::equal(&b.value, &n_as_bigint);
        }
        // BigInt == String
        if let (JsValue::BigInt(b), JsValue::String(s)) = (left, right) {
            if let Ok(parsed) = s.to_rust_string().parse::<num_bigint::BigInt>() {
                return bigint_ops::equal(&b.value, &parsed);
            }
            return false;
        }
        if let (JsValue::String(s), JsValue::BigInt(b)) = (left, right) {
            if let Ok(parsed) = s.to_rust_string().parse::<num_bigint::BigInt>() {
                return bigint_ops::equal(&parsed, &b.value);
            }
            return false;
        }
        // Object vs primitive (including BigInt)
        if matches!(left, JsValue::Object(_))
            && (right.is_string() || right.is_number() || right.is_symbol() || right.is_bigint())
        {
            let lprim = match self.to_primitive(left, "default") {
                Ok(v) => v,
                Err(_) => return false,
            };
            return self.abstract_equality(&lprim, right);
        }
        if matches!(right, JsValue::Object(_))
            && (left.is_string() || left.is_number() || left.is_symbol() || left.is_bigint())
        {
            let rprim = match self.to_primitive(right, "default") {
                Ok(v) => v,
                Err(_) => return false,
            };
            return self.abstract_equality(left, &rprim);
        }
        false
    }

    fn abstract_relational(&mut self, left: &JsValue, right: &JsValue) -> Option<bool> {
        let lprim = self
            .to_primitive(left, "number")
            .unwrap_or(JsValue::Undefined);
        let rprim = self
            .to_primitive(right, "number")
            .unwrap_or(JsValue::Undefined);
        if is_string(&lprim) && is_string(&rprim) {
            let ls = to_js_string(&lprim);
            let rs = to_js_string(&rprim);
            return Some(ls < rs);
        }
        // BigInt comparisons
        if let (JsValue::BigInt(a), JsValue::BigInt(b)) = (&lprim, &rprim) {
            return bigint_ops::less_than(&a.value, &b.value);
        }
        if let (JsValue::BigInt(b), JsValue::Number(n)) = (&lprim, &rprim) {
            if n.is_nan() {
                return None;
            }
            if *n == f64::INFINITY {
                return Some(true);
            }
            if *n == f64::NEG_INFINITY {
                return Some(false);
            }
            use num_bigint::BigInt;
            let n_floor = BigInt::from(*n as i64);
            if b.value < n_floor {
                return Some(true);
            }
            if b.value > n_floor {
                return Some(false);
            }
            // b.value == n_floor, check fractional part
            return Some((*n as i64 as f64) < *n);
        }
        if let (JsValue::Number(n), JsValue::BigInt(b)) = (&lprim, &rprim) {
            if n.is_nan() {
                return None;
            }
            if *n == f64::NEG_INFINITY {
                return Some(true);
            }
            if *n == f64::INFINITY {
                return Some(false);
            }
            use num_bigint::BigInt;
            let n_floor = BigInt::from(*n as i64);
            if n_floor < b.value {
                return Some(true);
            }
            if n_floor > b.value {
                return Some(false);
            }
            return Some(*n < (*n as i64 as f64));
        }
        // BigInt vs String: try parsing
        if let (JsValue::BigInt(_), JsValue::String(s)) = (&lprim, &rprim) {
            if let Ok(parsed) = s.to_rust_string().parse::<num_bigint::BigInt>() {
                return self
                    .abstract_relational(&lprim, &JsValue::BigInt(JsBigInt { value: parsed }));
            }
            return None;
        }
        if let (JsValue::String(s), JsValue::BigInt(_)) = (&lprim, &rprim) {
            if let Ok(parsed) = s.to_rust_string().parse::<num_bigint::BigInt>() {
                return self
                    .abstract_relational(&JsValue::BigInt(JsBigInt { value: parsed }), &rprim);
            }
            return None;
        }
        let ln = to_number(&lprim);
        let rn = to_number(&rprim);
        number_ops::less_than(ln, rn)
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
                if is_string(&lprim) || is_string(&rprim) {
                    let ls = to_js_string(&lprim);
                    let rs = to_js_string(&rprim);
                    Completion::Normal(JsValue::String(JsString::from_str(&format!("{ls}{rs}"))))
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
            BinaryOp::Eq => {
                Completion::Normal(JsValue::Boolean(self.abstract_equality(left, right)))
            }
            BinaryOp::NotEq => {
                Completion::Normal(JsValue::Boolean(!self.abstract_equality(left, right)))
            }
            BinaryOp::StrictEq => {
                Completion::Normal(JsValue::Boolean(strict_equality(left, right)))
            }
            BinaryOp::StrictNotEq => {
                Completion::Normal(JsValue::Boolean(!strict_equality(left, right)))
            }
            BinaryOp::Lt => Completion::Normal(JsValue::Boolean(
                self.abstract_relational(left, right) == Some(true),
            )),
            BinaryOp::Gt => Completion::Normal(JsValue::Boolean(
                self.abstract_relational(right, left) == Some(true),
            )),
            BinaryOp::LtEq => Completion::Normal(JsValue::Boolean(
                self.abstract_relational(right, left) == Some(false),
            )),
            BinaryOp::GtEq => Completion::Normal(JsValue::Boolean(
                self.abstract_relational(left, right) == Some(false),
            )),
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
                if !to_boolean(&lval) {
                    Completion::Normal(lval)
                } else {
                    self.eval_expr(right, env)
                }
            }
            LogicalOp::Or => {
                if to_boolean(&lval) {
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
            let raw_val = match env.borrow().get(name) {
                Some(v) => v,
                None => {
                    let err = self.create_reference_error(&format!("{name} is not defined"));
                    return Completion::Throw(err);
                }
            };
            let (old_val, new_val) = match self.apply_update_numeric(&raw_val, op) {
                Ok(pair) => pair,
                Err(e) => return Completion::Throw(e),
            };
            if let Err(_e) = env.borrow_mut().set(name, new_val.clone()) {
                return Completion::Throw(
                    self.create_type_error("Assignment to constant variable."),
                );
            }
            Completion::Normal(if prefix { new_val } else { old_val })
        } else if let Expression::Member(obj_expr, prop) = arg {
            let obj_val = match self.eval_expr(obj_expr, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            if let MemberProperty::Private(name) = prop {
                return match &obj_val {
                    JsValue::Object(o) => {
                        if let Some(obj) = self.get_object(o.id) {
                            let elem = obj.borrow().private_fields.get(name).cloned();
                            match elem {
                                Some(PrivateElement::Field(cur)) => {
                                    let (old_val, new_val) =
                                        match self.apply_update_numeric(&cur, op) {
                                            Ok(pair) => pair,
                                            Err(e) => return Completion::Throw(e),
                                        };
                                    obj.borrow_mut().private_fields.insert(
                                        name.clone(),
                                        PrivateElement::Field(new_val.clone()),
                                    );
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
            if let JsValue::Object(ref o) = obj_val {
                if let Some(obj) = self.get_object(o.id) {
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
            }
            Completion::Normal(if prefix { new_val } else { old_val })
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
                        match self.to_property_key(&v) {
                            Ok(s) => s,
                            Err(e) => return Err(e),
                        }
                    }
                    MemberProperty::Private(_) => return Ok(()),
                };
                if let JsValue::Object(ref o) = obj_val {
                    if let Some(obj) = self.get_object(o.id) {
                        // TypedArray [[Set]]
                        let is_ta = obj.borrow().typed_array_info.is_some();
                        if is_ta {
                            if let Some(index) = canonical_numeric_index_string(&key) {
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
                        }
                        obj.borrow_mut().set_property_value(&key, value);
                    }
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
            let lval = match self.eval_expr(left, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            let should_assign = match op {
                AssignOp::LogicalAndAssign => to_boolean(&lval),
                AssignOp::LogicalOrAssign => !to_boolean(&lval),
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
            if let Expression::Identifier(name) = left {
                if let Err(_e) = env.borrow_mut().set(name, rval.clone()) {
                    return Completion::Throw(
                        self.create_type_error("Assignment to constant variable."),
                    );
                }
            }
            return Completion::Normal(rval);
        }

        let rval = match self.eval_expr(right, env) {
            Completion::Normal(v) => v,
            other => return other,
        };

        match left {
            Expression::Identifier(name) => {
                let final_val = if op == AssignOp::Assign {
                    if right.is_anonymous_function_definition() {
                        self.set_function_name(&rval, name);
                    }
                    rval
                } else {
                    let lval = match env.borrow().get(name) {
                        Some(v) => v,
                        None => {
                            return Completion::Throw(
                                self.create_reference_error(&format!("{name} is not defined")),
                            );
                        }
                    };
                    match self.apply_compound_assign(op, &lval, &rval) {
                        Completion::Normal(v) => v,
                        other => return other,
                    }
                };
                if !env.borrow().has(name) {
                    if env.borrow().strict {
                        return Completion::Throw(
                            self.create_reference_error(&format!("{name} is not defined")),
                        );
                    }
                    env.borrow_mut().declare(name, BindingKind::Var);
                }
                if let Err(_e) = env.borrow_mut().set(name, final_val.clone()) {
                    return Completion::Throw(
                        self.create_type_error("Assignment to constant variable."),
                    );
                }
                Completion::Normal(final_val)
            }
            Expression::Member(obj_expr, prop) => {
                let obj_val = match self.eval_expr(obj_expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if let MemberProperty::Private(name) = prop {
                    return match &obj_val {
                        JsValue::Object(o) => {
                            if let Some(obj) = self.get_object(o.id) {
                                let elem = obj.borrow().private_fields.get(name).cloned();
                                match elem {
                                    Some(PrivateElement::Field(_)) => {
                                        let final_val = if op == AssignOp::Assign {
                                            rval
                                        } else {
                                            let lval = if let Some(PrivateElement::Field(v)) = obj.borrow().private_fields.get(name) {
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
                                            .insert(name.clone(), PrivateElement::Field(final_val.clone()));
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
                                            self.call_function(&setter, &obj_val, &[final_val.clone()]);
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
                    MemberProperty::Private(_) => unreachable!(),
                };
                if let JsValue::Object(ref o) = obj_val
                    && let Some(obj) = self.get_object(o.id)
                {
                    let final_val = if op == AssignOp::Assign {
                        rval
                    } else {
                        let lval = obj.borrow().get_property(&key);
                        match self.apply_compound_assign(op, &lval, &rval) {
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
                    // Check for setter
                    let desc = obj.borrow().get_property_descriptor(&key);
                    if let Some(ref d) = desc
                        && let Some(ref setter) = d.set
                        && !matches!(setter, JsValue::Undefined)
                    {
                        let setter = setter.clone();
                        let this = obj_val.clone();
                        return match self.call_function(&setter, &this, &[final_val.clone()]) {
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
                        if is_ta {
                            if let Some(index) = canonical_numeric_index_string(&key) {
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
                    }
                    // OrdinarySet: if no own property, check for proxy in prototype chain
                    if !obj.borrow().has_own_property(&key) {
                        let proto = obj.borrow().prototype.clone();
                        if let Some(proto_rc) = proto {
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
                        }
                    }
                    let success = obj.borrow_mut().set_property_value(&key, final_val.clone());
                    if !success && env.borrow().strict {
                        return Completion::Throw(self.create_type_error(&format!(
                            "Cannot assign to read only property '{key}'"
                        )));
                    }
                    return Completion::Normal(final_val);
                }
                Completion::Normal(rval)
            }
            Expression::Array(elements) if op == AssignOp::Assign => {
                match self.destructure_array_assignment(elements, &rval, env) {
                    Ok(()) => Completion::Normal(rval),
                    Err(e) => Completion::Throw(e),
                }
            }
            Expression::Object(props) if op == AssignOp::Assign => {
                match self.destructure_object_assignment(props, &rval, env) {
                    Ok(()) => Completion::Normal(rval),
                    Err(e) => Completion::Throw(e),
                }
            }
            _ => Completion::Normal(rval),
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
            return match &obj_val {
                JsValue::Object(o) => {
                    if let Some(obj) = self.get_object(o.id) {
                        let elem = obj.borrow().private_fields.get(name).cloned();
                        match elem {
                            Some(PrivateElement::Field(_)) => {
                                obj.borrow_mut()
                                    .private_fields
                                    .insert(name.clone(), PrivateElement::Field(val));
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
                match self.proxy_set(o.id, &key, val, &receiver) {
                    Ok(success) => {
                        if !success && env.borrow().strict {
                            return Err(self.create_type_error(&format!(
                                "Cannot assign to read only property '{key}'"
                            )));
                        }
                        return Ok(());
                    }
                    Err(e) => return Err(e),
                }
            }
            // Check for setter
            let desc = obj.borrow().get_property_descriptor(&key);
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
                if env.borrow().strict {
                    return Err(self.create_type_error(&format!(
                        "Cannot set property '{key}' which has only a getter"
                    )));
                }
                return Ok(());
            }
            // TypedArray [[Set]]
            let is_ta = obj.borrow().typed_array_info.is_some();
            if is_ta {
                if let Some(index) = canonical_numeric_index_string(&key) {
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
            }
            // OrdinarySet: if no own property, check for proxy in prototype chain
            if !obj.borrow().has_own_property(&key) {
                let proto = obj.borrow().prototype.clone();
                if let Some(proto_rc) = proto {
                    let proto_id = proto_rc.borrow().id.unwrap();
                    if self.has_proxy_in_prototype_chain(proto_id) {
                        let receiver = obj_val.clone();
                        match self.proxy_set(proto_id, &key, val, &receiver) {
                            Ok(success) => {
                                if !success && env.borrow().strict {
                                    return Err(self.create_type_error(&format!(
                                        "Cannot assign to read only property '{key}'"
                                    )));
                                }
                                return Ok(());
                            }
                            Err(e) => return Err(e),
                        }
                    }
                }
            }
            let success = obj.borrow_mut().set_property_value(&key, val);
            if !success && env.borrow().strict {
                return Err(
                    self.create_type_error(&format!("Cannot assign to read only property '{key}'"))
                );
            }
        }
        Ok(())
    }

    fn put_value_to_target(
        &mut self,
        target: &Expression,
        val: JsValue,
        env: &EnvRef,
    ) -> Result<(), JsValue> {
        match target {
            Expression::Identifier(name) => {
                if !env.borrow().has(name) {
                    if env.borrow().strict {
                        return Err(self.create_reference_error(&format!("{name} is not defined")));
                    }
                    env.borrow_mut().declare(name, BindingKind::Var);
                }
                env.borrow_mut().set(name, val)
            }
            Expression::Member(obj_expr, prop) => {
                self.set_member_property(obj_expr, prop, val, env)
            }
            Expression::Array(elements) => self.destructure_array_assignment(elements, &val, env),
            Expression::Object(props) => self.destructure_object_assignment(props, &val, env),
            Expression::Assign(AssignOp::Assign, inner_target, default) => {
                let v = if val.is_undefined() {
                    match self.eval_expr(default, env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Err(e),
                        _ => JsValue::Undefined,
                    }
                } else {
                    val
                };
                self.put_value_to_target(inner_target, v, env)
            }
            _ => Ok(()),
        }
    }

    fn destructure_array_assignment(
        &mut self,
        elements: &[Option<Expression>],
        rval: &JsValue,
        env: &EnvRef,
    ) -> Result<(), JsValue> {
        let iterator = self.get_iterator(rval)?;
        if let JsValue::Object(o) = &iterator {
            self.gc_temp_roots.push(o.id);
        }
        let mut done = false;
        let mut error: Option<JsValue> = None;

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
                    // Rest element: collect remaining into array
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
                        // The inner of Spread can itself have a default
                        if let Err(e) = self.put_value_to_target(inner, arr, env) {
                            error = Some(e);
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
                                    if let Expression::Identifier(name) = target {
                                        if default.is_anonymous_function_definition() {
                                            self.set_function_name(&v, name);
                                        }
                                    }
                                    v
                                }
                                Completion::Throw(e) => {
                                    error = Some(e);
                                    break;
                                }
                                _ => JsValue::Undefined,
                            }
                        } else {
                            item
                        }
                    } else {
                        item
                    };

                    if let Err(e) = self.put_value_to_target(target, val, env) {
                        error = Some(e);
                        break;
                    }
                }
            }
        }

        let unroot = |s: &mut Self| {
            if let JsValue::Object(o) = &iterator {
                if let Some(pos) = s.gc_temp_roots.iter().rposition(|&id| id == o.id) {
                    s.gc_temp_roots.remove(pos);
                }
            }
        };

        // IteratorClose: if iterator is not done, call return()
        if !done {
            if let Some(err) = error {
                let _ = self.iterator_close_result(&iterator);
                unroot(self);
                return Err(err);
            }
            let r = self.iterator_close_result(&iterator);
            unroot(self);
            return r;
        }

        unroot(self);
        if let Some(err) = error {
            return Err(err);
        }
        Ok(())
    }

    fn destructure_object_assignment(
        &mut self,
        props: &[Property],
        rval: &JsValue,
        env: &EnvRef,
    ) -> Result<(), JsValue> {
        // RequireObjectCoercible
        match self.require_object_coercible(rval) {
            Completion::Throw(e) => return Err(e),
            _ => {}
        }

        // ToObject to wrap primitives
        let obj_val = match self.to_object(rval) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => unreachable!(),
        };

        let mut excluded_keys: Vec<String> = Vec::new();

        for prop in props {
            // Handle rest: {...rest} = obj
            if let Expression::Spread(inner) = &prop.value {
                let rest_obj = self.create_object();
                if let JsValue::Object(o) = &obj_val {
                    if let Some(src) = self.get_object(o.id) {
                        let keys: Vec<String> = src.borrow().property_order.clone();
                        for key in &keys {
                            if !excluded_keys.contains(key) {
                                let desc = src.borrow().get_own_property(key);
                                if let Some(ref d) = desc
                                    && d.enumerable.unwrap_or(true)
                                {
                                    let v = match self.get_object_property(o.id, key, &obj_val) {
                                        Completion::Normal(v) => v,
                                        Completion::Throw(e) => return Err(e),
                                        _ => JsValue::Undefined,
                                    };
                                    rest_obj.borrow_mut().insert_value(key.clone(), v);
                                }
                            }
                        }
                    }
                }
                let rest_id = rest_obj.borrow().id.unwrap();
                let rest_val = JsValue::Object(crate::types::JsObject { id: rest_id });
                self.put_value_to_target(inner, rest_val, env)?;
                continue;
            }

            match &prop.kind {
                PropertyKind::Init => {
                    let key = match &prop.key {
                        PropertyKey::Identifier(s) | PropertyKey::String(s) => s.clone(),
                        PropertyKey::Number(n) => to_js_string(&JsValue::Number(*n)),
                        PropertyKey::Computed(expr) => match self.eval_expr(expr, env) {
                            Completion::Normal(v) => self.to_property_key(&v)?,
                            Completion::Throw(e) => return Err(e),
                            _ => String::new(),
                        },
                        PropertyKey::Private(_) => {
                            return Err(self.create_type_error(
                                "Private names are not valid in object patterns",
                            ));
                        }
                    };
                    excluded_keys.push(key.clone());

                    // Get property via get_object_property (invokes getters/Proxy)
                    let val = if let JsValue::Object(o) = &obj_val {
                        match self.get_object_property(o.id, &key, &obj_val) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Err(e),
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
                                    if let Expression::Identifier(name) = target {
                                        if default.is_anonymous_function_definition() {
                                            self.set_function_name(&v, name);
                                        }
                                    }
                                    v
                                }
                                Completion::Throw(e) => return Err(e),
                                _ => JsValue::Undefined,
                            }
                        } else {
                            val
                        }
                    } else {
                        val
                    };

                    self.put_value_to_target(target, val, env)?;
                }
                _ => continue,
            }
        }
        Ok(())
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
                    if let JsValue::Object(this_obj) = v {
                        if let Some(nt) = &self.new_target {
                            if let JsValue::Object(nt_o) = nt {
                                if let Some(nt_func) = self.get_object(nt_o.id) {
                                    let proto_val =
                                        nt_func.borrow().get_property_value("prototype");
                                    if let Some(JsValue::Object(proto_obj)) = proto_val {
                                        if let Some(proto_rc) = self.get_object(proto_obj.id) {
                                            if let Some(obj) = self.get_object(this_obj.id) {
                                                obj.borrow_mut().prototype = Some(proto_rc);
                                            }
                                        }
                                    }
                                }
                            }
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
                // Base constructor super() call (shouldn't normally happen, but handle gracefully)
                let this_val = env.borrow().get("this").unwrap_or(JsValue::Undefined);
                let result = self.call_function(&super_ctor, &this_val, &arg_vals);
                if let Completion::Normal(ref v) = result {
                    if matches!(v, JsValue::Object(_)) {
                        env.borrow_mut().bindings.insert(
                            "this".to_string(),
                            crate::interpreter::types::Binding {
                                value: v.clone(),
                                kind: crate::interpreter::types::BindingKind::Const,
                                initialized: true,
                            },
                        );
                    }
                }
                return result;
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
                        if let JsValue::Object(ref o) = obj_val
                            && let Some(obj) = self.get_object(o.id)
                        {
                            let elem = obj.borrow().private_fields.get(name).cloned();
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
                    if let Some(ref sp) = self.string_prototype {
                        let method = sp.borrow().get_property(&key);
                        (method, obj_val)
                    } else {
                        (JsValue::Undefined, obj_val)
                    }
                } else if matches!(&obj_val, JsValue::Number(_)) {
                    let proto = self
                        .number_prototype
                        .clone()
                        .or(self.object_prototype.clone());
                    if let Some(ref p) = proto {
                        let method = p.borrow().get_property(&key);
                        (method, obj_val)
                    } else {
                        (JsValue::Undefined, obj_val)
                    }
                } else if matches!(&obj_val, JsValue::Boolean(_)) {
                    let proto = self
                        .boolean_prototype
                        .clone()
                        .or(self.object_prototype.clone());
                    if let Some(ref p) = proto {
                        let method = p.borrow().get_property(&key);
                        (method, obj_val)
                    } else {
                        (JsValue::Undefined, obj_val)
                    }
                } else if matches!(&obj_val, JsValue::Symbol(_)) {
                    if let Some(ref p) = self.symbol_prototype {
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
                    let proto = self
                        .bigint_prototype
                        .clone()
                        .or(self.object_prototype.clone());
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
        if matches!(callee, Expression::Identifier(n) if n == "eval") {
            if self.is_builtin_eval(&func_val) {
                let evaluated_args = match self.eval_spread_args(args, env) {
                    Ok(args) => args,
                    Err(e) => return Completion::Throw(e),
                };
                let caller_strict = env.borrow().strict;
                return self.perform_eval(&evaluated_args, caller_strict, true, env);
            }
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
            ..
        }) = state
        else {
            return Completion::Throw(self.create_type_error("not a state machine generator"));
        };

        if let Some(ref deleg_info) = delegated_iterator {
            let iterator = deleg_info.iterator.clone();
            let resume_state = deleg_info.resume_state;
            let binding = deleg_info.sent_value_binding.clone();

            let result = self.iterator_next_with_value(&iterator, &sent_value);
            match result {
                Ok(iter_result) => {
                    let done = match self.iterator_complete(&iter_result) {
                        Ok(d) => d,
                        Err(e) => return Completion::Throw(e),
                    };
                    let value = match self.iterator_value(&iter_result) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
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
                                SentValueBindingKind::Discard => {}
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
                                sent_value: JsValue::Undefined,
                                try_stack: try_stack.clone(),
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
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
                                sent_value: JsValue::Undefined,
                                try_stack,
                                pending_binding: None,
                                delegated_iterator: Some(
                                    crate::interpreter::types::DelegatedIteratorInfo {
                                        iterator,
                                        resume_state,
                                        sent_value_binding: binding,
                                    },
                                ),
                                pending_exception: None,
                            });
                        return Completion::Normal(self.create_iter_result_object(value, false));
                    }
                }
                Err(e) => {
                    obj_rc.borrow_mut().iterator_state =
                        Some(IteratorState::StateMachineGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            sent_value: JsValue::Undefined,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                        });
                    return Completion::Throw(e);
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
            sent_value: sent_value.clone(),
            try_stack: try_stack.clone(),
            pending_binding: None,
            delegated_iterator: None,
            pending_exception: None,
        });

        use crate::interpreter::generator_transform::SentValueBindingKind;
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
            }
        }

        let mut current_id = current_state_id;
        let mut current_try_stack = try_stack;
        let mut pending_exception: Option<JsValue> = stored_pending_exception;

        loop {
            let (statements, terminator) = {
                let gen_state = &state_machine.states[current_id];
                (gen_state.statements.clone(), gen_state.terminator.clone())
            };

            let stmt_result = self.exec_statements(&statements, &func_env);
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
                    sent_value: JsValue::Undefined,
                    try_stack: vec![],
                    pending_binding: None,
                    delegated_iterator: None,
                    pending_exception: None,
                });
                return Completion::Throw(e);
            }
            if let Completion::Return(v) = stmt_result {
                obj_rc.borrow_mut().iterator_state = Some(IteratorState::StateMachineGenerator {
                    state_machine,
                    func_env,
                    is_strict,
                    execution_state: StateMachineExecutionState::Completed,
                    sent_value: JsValue::Undefined,
                    try_stack: vec![],
                    pending_binding: None,
                    delegated_iterator: None,
                    pending_exception: None,
                });
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
                                        sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
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
                                obj_rc.borrow_mut().iterator_state =
                                    Some(IteratorState::StateMachineGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                    });
                                return Completion::Throw(e);
                            }
                        };

                        let iter_result = match self.iterator_next(&iterator) {
                            Ok(r) => r,
                            Err(e) => {
                                obj_rc.borrow_mut().iterator_state =
                                    Some(IteratorState::StateMachineGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                    });
                                return Completion::Throw(e);
                            }
                        };

                        let done = match self.iterator_complete(&iter_result) {
                            Ok(d) => d,
                            Err(e) => return Completion::Throw(e),
                        };
                        let value = match self.iterator_value(&iter_result) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };

                        if done {
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
                                    SentValueBindingKind::Discard => {}
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
                                    sent_value: JsValue::Undefined,
                                    try_stack: current_try_stack,
                                    pending_binding: None,
                                    delegated_iterator: Some(
                                        crate::interpreter::types::DelegatedIteratorInfo {
                                            iterator,
                                            resume_state: *resume_state,
                                            sent_value_binding: sent_value_binding.clone(),
                                        },
                                    ),
                                    pending_exception: None,
                                });
                            return Completion::Normal(
                                self.create_iter_result_object(value, false),
                            );
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
                            sent_value: JsValue::Undefined,
                            try_stack: current_try_stack,
                            pending_binding: sent_value_binding.clone(),
                            delegated_iterator: None,
                            pending_exception: None,
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
                                        sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                    });
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
                            sent_value: JsValue::Undefined,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                        });
                    return Completion::Normal(self.create_iter_result_object(ret_val, true));
                }

                StateTerminator::Throw(expr) => {
                    let throw_val = match self.eval_expr(expr, &func_env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => e,
                        other => return other,
                    };

                    if let Some(try_info) = current_try_stack.pop() {
                        if let Some(catch_state) = try_info.catch_state {
                            current_id = catch_state;
                            continue;
                        }
                    }

                    obj_rc.borrow_mut().iterator_state =
                        Some(IteratorState::StateMachineGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            sent_value: JsValue::Undefined,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
                        });
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
                                    sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                });
                            return Completion::Throw(e);
                        }
                        other => return other,
                    };
                    current_id = if to_boolean(&cond_val) {
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
                        after_state: *after_state,
                        entered_catch: false,
                        entered_finally: false,
                    });
                    current_id = *try_state;
                }

                StateTerminator::TryExit { after_state } => {
                    current_try_stack.pop();
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
                                    sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
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
                                        sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
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
                            sent_value: JsValue::Undefined,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
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
                let resume_state = deleg_info.resume_state;
                let binding = deleg_info.sent_value_binding.clone();

                match self.iterator_return(&iterator, &value) {
                    Ok(Some(iter_result)) => {
                        let done = match self.iterator_complete(&iter_result) {
                            Ok(d) => d,
                            Err(e) => return Completion::Throw(e),
                        };
                        let result_value = match self.iterator_value(&iter_result) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };
                        if done {
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::Completed,
                                    sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                });
                            return Completion::Normal(
                                self.create_iter_result_object(result_value, true),
                            );
                        } else {
                            obj_rc.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::SuspendedAtState {
                                        state_id: resume_state,
                                    },
                                    sent_value: JsValue::Undefined,
                                    try_stack: try_stack.clone(),
                                    pending_binding: None,
                                    delegated_iterator: Some(
                                        crate::interpreter::types::DelegatedIteratorInfo {
                                            iterator,
                                            resume_state,
                                            sent_value_binding: binding,
                                        },
                                    ),
                                    pending_exception: None,
                                });
                            return Completion::Normal(
                                self.create_iter_result_object(result_value, false),
                            );
                        }
                    }
                    Ok(None) => {
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                sent_value: JsValue::Undefined,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                            });
                        return Completion::Normal(self.create_iter_result_object(value, true));
                    }
                    Err(e) => {
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                sent_value: JsValue::Undefined,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                            });
                        return Completion::Throw(e);
                    }
                }
            }

            if let Some(try_info) = try_stack.last() {
                if !try_info.entered_finally {
                    if let Some(finally_state) = try_info.finally_state {
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: finally_state,
                                },
                                sent_value: value.clone(),
                                try_stack: try_stack[..try_stack.len() - 1].to_vec(),
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                            });
                        return self.generator_next_state_machine(this, JsValue::Undefined);
                    }
                }
            }

            obj_rc.borrow_mut().iterator_state = Some(IteratorState::StateMachineGenerator {
                state_machine,
                func_env,
                is_strict,
                execution_state: StateMachineExecutionState::Completed,
                sent_value: JsValue::Undefined,
                try_stack: vec![],
                pending_binding: None,
                delegated_iterator: None,
                pending_exception: None,
            });
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
                let resume_state = deleg_info.resume_state;
                let binding = deleg_info.sent_value_binding.clone();

                match self.iterator_throw(&iterator, &exception) {
                    Ok(Some(iter_result)) => {
                        let done = match self.iterator_complete(&iter_result) {
                            Ok(d) => d,
                            Err(e) => return Completion::Throw(e),
                        };
                        let result_value = match self.iterator_value(&iter_result) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
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
                                    SentValueBindingKind::Discard => {}
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
                                    sent_value: JsValue::Undefined,
                                    try_stack: try_stack.clone(),
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
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
                                    sent_value: JsValue::Undefined,
                                    try_stack: try_stack.clone(),
                                    pending_binding: None,
                                    delegated_iterator: Some(
                                        crate::interpreter::types::DelegatedIteratorInfo {
                                            iterator,
                                            resume_state,
                                            sent_value_binding: binding,
                                        },
                                    ),
                                    pending_exception: None,
                                });
                            return Completion::Normal(
                                self.create_iter_result_object(result_value, false),
                            );
                        }
                    }
                    Ok(None) => {
                        let _ = self.iterator_close(&iterator, exception.clone());
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                sent_value: JsValue::Undefined,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                            });
                        return Completion::Throw(exception);
                    }
                    Err(e) => {
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                sent_value: JsValue::Undefined,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                            });
                        return Completion::Throw(e);
                    }
                }
            }

            if let Some(try_info) = try_stack.last() {
                if !try_info.entered_catch && !try_info.entered_finally {
                    if let Some(catch_state) = try_info.catch_state {
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: catch_state,
                                },
                                sent_value: JsValue::Undefined,
                                try_stack: try_stack[..try_stack.len() - 1].to_vec(),
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: Some(exception.clone()),
                            });
                        return self.generator_next_state_machine(this, JsValue::Undefined);
                    }
                }
                if !try_info.entered_finally {
                    if let Some(finally_state) = try_info.finally_state {
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: finally_state,
                                },
                                sent_value: JsValue::Undefined,
                                try_stack: try_stack[..try_stack.len() - 1].to_vec(),
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: Some(exception.clone()),
                            });
                        return self.generator_next_state_machine(this, JsValue::Undefined);
                    }
                }
            }

            obj_rc.borrow_mut().iterator_state = Some(IteratorState::StateMachineGenerator {
                state_machine,
                func_env,
                is_strict,
                execution_state: StateMachineExecutionState::Completed,
                sent_value: JsValue::Undefined,
                try_stack: vec![],
                pending_binding: None,
                delegated_iterator: None,
                pending_exception: None,
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
            let resume_state = deleg_info.resume_state;
            let binding = deleg_info.sent_value_binding.clone();

            let result = self.iterator_next_with_value(&iterator, &sent_value);
            match result {
                Ok(iter_result) => {
                    let done = match self.iterator_complete(&iter_result) {
                        Ok(d) => d,
                        Err(e) => return Completion::Throw(e),
                    };
                    let value = match self.iterator_value(&iter_result) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
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
                                SentValueBindingKind::Discard => {}
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
                                sent_value: JsValue::Undefined,
                                try_stack: try_stack.clone(),
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
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
                                        sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
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
                                execution_state: StateMachineExecutionState::SuspendedAtState {
                                    state_id: resume_state,
                                },
                                sent_value: JsValue::Undefined,
                                try_stack,
                                pending_binding: None,
                                delegated_iterator: Some(
                                    crate::interpreter::types::DelegatedIteratorInfo {
                                        iterator,
                                        resume_state,
                                        sent_value_binding: binding,
                                    },
                                ),
                                pending_exception: None,
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
                            sent_value: JsValue::Undefined,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
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
            sent_value: sent_value.clone(),
            try_stack: try_stack.clone(),
            pending_binding: None,
            delegated_iterator: None,
            pending_exception: None,
        });

        use crate::interpreter::generator_transform::SentValueBindingKind;
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
            }
        }

        let mut current_id = current_state_id;
        let mut current_try_stack = try_stack;
        let mut pending_exception: Option<JsValue> = stored_pending_exception;

        loop {
            let (statements, terminator) = {
                let gen_state = &state_machine.states[current_id];
                (gen_state.statements.clone(), gen_state.terminator.clone())
            };

            let stmt_result = self.exec_statements(&statements, &func_env);
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
                        sent_value: JsValue::Undefined,
                        try_stack: vec![],
                        pending_binding: None,
                        delegated_iterator: None,
                        pending_exception: None,
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
                                sent_value: JsValue::Undefined,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
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
                        sent_value: JsValue::Undefined,
                        try_stack: vec![],
                        pending_binding: None,
                        delegated_iterator: None,
                        pending_exception: None,
                    });
                let iter_result = self.create_iter_result_object(awaited, true);
                let _ = self.call_function(&resolve_fn, &JsValue::Undefined, &[iter_result]);
                self.drain_microtasks();
                return Completion::Normal(promise);
            }
            // Handle Yield completions from exec_statements (e.g., from yields inside for-of loops)
            if let Completion::Yield(yield_val) = stmt_result {
                let awaited_val = match self.await_value(&yield_val) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => {
                        obj_rc.borrow_mut().iterator_state =
                            Some(IteratorState::StateMachineAsyncGenerator {
                                state_machine,
                                func_env,
                                is_strict,
                                execution_state: StateMachineExecutionState::Completed,
                                sent_value: JsValue::Undefined,
                                try_stack: vec![],
                                pending_binding: None,
                                delegated_iterator: None,
                                pending_exception: None,
                            });
                        let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                        self.drain_microtasks();
                        return Completion::Normal(promise);
                    }
                    _ => yield_val,
                };
                // For yields inside for-of/for-await-of, we can't properly resume
                // the loop, so we mark as completed after yielding
                obj_rc.borrow_mut().iterator_state =
                    Some(IteratorState::StateMachineAsyncGenerator {
                        state_machine,
                        func_env,
                        is_strict,
                        execution_state: StateMachineExecutionState::Completed,
                        sent_value: JsValue::Undefined,
                        try_stack: current_try_stack,
                        pending_binding: None,
                        delegated_iterator: None,
                        pending_exception: None,
                    });
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
                                        sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
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
                                            sent_value: JsValue::Undefined,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
                                        });
                                    let _ =
                                        self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                    self.drain_microtasks();
                                    return Completion::Normal(promise);
                                }
                            },
                        };

                        let iter_result = match self.iterator_next(&iterator) {
                            Ok(r) => r,
                            Err(e) => {
                                obj_rc.borrow_mut().iterator_state =
                                    Some(IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::Completed,
                                        sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
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
                                        sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                    });
                                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
                                self.drain_microtasks();
                                return Completion::Normal(promise);
                            }
                            _ => iter_result,
                        };

                        let done = match self.iterator_complete(&awaited_result) {
                            Ok(d) => d,
                            Err(e) => return Completion::Throw(e),
                        };
                        let value = match self.iterator_value(&awaited_result) {
                            Ok(v) => v,
                            Err(e) => {
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
                                    SentValueBindingKind::Discard => {}
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
                                            sent_value: JsValue::Undefined,
                                            try_stack: vec![],
                                            pending_binding: None,
                                            delegated_iterator: None,
                                            pending_exception: None,
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
                                    sent_value: JsValue::Undefined,
                                    try_stack: current_try_stack,
                                    pending_binding: None,
                                    delegated_iterator: Some(
                                        crate::interpreter::types::DelegatedIteratorInfo {
                                            iterator,
                                            resume_state: *resume_state,
                                            sent_value_binding: sent_value_binding.clone(),
                                        },
                                    ),
                                    pending_exception: None,
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
                                    sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
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
                            sent_value: JsValue::Undefined,
                            try_stack: current_try_stack,
                            pending_binding: sent_value_binding.clone(),
                            delegated_iterator: None,
                            pending_exception: None,
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
                                        sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
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
                                    sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
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
                            sent_value: JsValue::Undefined,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
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

                    if let Some(try_info) = current_try_stack.pop() {
                        if let Some(catch_state) = try_info.catch_state {
                            pending_exception = Some(throw_val);
                            current_id = catch_state;
                            continue;
                        }
                    }

                    obj_rc.borrow_mut().iterator_state =
                        Some(IteratorState::StateMachineAsyncGenerator {
                            state_machine,
                            func_env,
                            is_strict,
                            execution_state: StateMachineExecutionState::Completed,
                            sent_value: JsValue::Undefined,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
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
                                    sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
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
                    current_id = if to_boolean(&cond_val) {
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
                        after_state: *after_state,
                        entered_catch: false,
                        entered_finally: false,
                    });
                    current_id = *try_state;
                }

                StateTerminator::TryExit { after_state } => {
                    current_try_stack.pop();
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
                                    sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
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
                                        sent_value: JsValue::Undefined,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
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
                            sent_value: JsValue::Undefined,
                            try_stack: vec![],
                            pending_binding: None,
                            delegated_iterator: None,
                            pending_exception: None,
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
            ..
        }) = &state
        {
            let promise = self.create_promise_object();
            let promise_id = if let JsValue::Object(ref po) = promise {
                po.id
            } else {
                0
            };
            let (resolve_fn, _reject_fn) = self.create_resolving_functions(promise_id);

            obj_rc.borrow_mut().iterator_state = Some(IteratorState::StateMachineAsyncGenerator {
                state_machine: state_machine.clone(),
                func_env: func_env.clone(),
                is_strict: *is_strict,
                execution_state: StateMachineExecutionState::Completed,
                sent_value: JsValue::Undefined,
                try_stack: vec![],
                pending_binding: None,
                delegated_iterator: None,
                pending_exception: None,
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
                return Completion::Normal(promise);
            }
            GeneratorExecutionState::Executing => {
                let err = self.create_type_error("AsyncGenerator is already executing");
                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[err]);
                self.drain_microtasks();
                return Completion::Normal(promise);
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
            ..
        }) = &state
        {
            let promise = self.create_promise_object();
            let promise_id = if let JsValue::Object(ref po) = promise {
                po.id
            } else {
                0
            };
            let (_, reject_fn) = self.create_resolving_functions(promise_id);

            obj_rc.borrow_mut().iterator_state = Some(IteratorState::StateMachineAsyncGenerator {
                state_machine: state_machine.clone(),
                func_env: func_env.clone(),
                is_strict: *is_strict,
                execution_state: StateMachineExecutionState::Completed,
                sent_value: JsValue::Undefined,
                try_stack: vec![],
                pending_binding: None,
                delegated_iterator: None,
                pending_exception: None,
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
                                    self.async_generator_prototype.clone();
                            }
                            gen_obj.borrow_mut().class_name = "AsyncGenerator".to_string();
                            // Create persistent function environment
                            let func_env = Environment::new_function_scope(Some(closure.clone()));
                            func_env.borrow_mut().strict = is_strict;
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
                            func_env.borrow_mut().bindings.insert(
                                "this".to_string(),
                                Binding {
                                    value: _this_val.clone(),
                                    kind: BindingKind::Const,
                                    initialized: true,
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

                            use crate::interpreter::generator_transform::transform_generator;
                            let state_machine = Rc::new(transform_generator(&body, &params));
                            for temp_var in &state_machine.temp_vars {
                                func_env.borrow_mut().declare(temp_var, BindingKind::Var);
                            }
                            gen_obj.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineAsyncGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::SuspendedStart,
                                    sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
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
                            func_env.borrow_mut().bindings.insert(
                                "this".to_string(),
                                Binding {
                                    value: _this_val.clone(),
                                    kind: BindingKind::Const,
                                    initialized: true,
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

                            use crate::interpreter::generator_transform::transform_generator;
                            let state_machine = Rc::new(transform_generator(&body, &params));
                            // Declare temp variables used by the state machine
                            for temp_var in &state_machine.temp_vars {
                                func_env.borrow_mut().declare(temp_var, BindingKind::Var);
                            }
                            gen_obj.borrow_mut().iterator_state =
                                Some(IteratorState::StateMachineGenerator {
                                    state_machine,
                                    func_env,
                                    is_strict,
                                    execution_state: StateMachineExecutionState::SuspendedStart,
                                    sent_value: JsValue::Undefined,
                                    try_stack: vec![],
                                    pending_binding: None,
                                    delegated_iterator: None,
                                    pending_exception: None,
                                });
                            let gen_id = gen_obj.borrow().id.unwrap();
                            return Completion::Normal(JsValue::Object(crate::types::JsObject {
                                id: gen_id,
                            }));
                        }
                        let closure_strict = closure.borrow().strict;
                        let func_env = Environment::new_function_scope(Some(closure));
                        // Bind `this` before parameters so default param exprs can access it
                        if !is_arrow {
                            if self.constructing_derived {
                                // Derived constructor: this is in TDZ until super() is called
                                func_env.borrow_mut().bindings.insert(
                                    "this".to_string(),
                                    Binding {
                                        value: JsValue::Undefined,
                                        kind: BindingKind::Const,
                                        initialized: false,
                                    },
                                );
                                self.constructing_derived = false;
                            } else {
                                let effective_this = if !is_strict && !closure_strict {
                                    if matches!(_this_val, JsValue::Undefined | JsValue::Null) {
                                        self.global_env
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
                                    },
                                );
                            }
                            let is_simple =
                                params.iter().all(|p| matches!(p, Pattern::Identifier(_)));
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
                        self.call_stack_envs.push(func_env.clone());
                        let result = self.exec_statements(&body, &func_env);
                        self.call_stack_envs.pop();
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
        if let JsValue::Object(o) = val
            && let Some(obj) = self.get_object(o.id)
        {
            if let Some(ref func) = obj.borrow().callable {
                return matches!(func, JsFunction::Native(name, _, _, _) if name == "eval");
            }
        }
        false
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
        let code = to_js_string(&arg);
        let mut p = match parser::Parser::new(&code) {
            Ok(p) => p,
            Err(_) => {
                return Completion::Throw(self.create_error("SyntaxError", "Invalid eval source"));
            }
        };
        if caller_strict {
            p.set_strict(true);
        }
        let program = match p.parse_program() {
            Ok(prog) => prog,
            Err(e) => {
                return Completion::Throw(self.create_error("SyntaxError", &format!("{}", e)));
            }
        };
        let eval_code_strict = program.body.first().is_some_and(|s| {
            matches!(s, Statement::Expression(Expression::Literal(Literal::String(s))) if s == "use strict")
        });
        let is_strict = caller_strict || eval_code_strict;
        let env = if is_strict {
            let new_env = Environment::new_function_scope(if direct {
                Some(caller_env.clone())
            } else {
                Some(self.global_env.clone())
            });
            new_env.borrow_mut().strict = true;
            new_env
        } else if direct {
            caller_env.clone()
        } else {
            self.global_env.clone()
        };
        let mut last = Completion::Empty;
        for stmt in &program.body {
            match self.exec_statement(stmt, &env) {
                Completion::Normal(v) => last = Completion::Normal(v),
                Completion::Empty => {}
                other => return other,
            }
        }
        last.update_empty(JsValue::Undefined)
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
                        ..
                    }) => !is_arrow && !is_generator && !is_async,
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
                Completion::Normal(_) | Completion::Empty => {
                    // If super() was never called, this is still uninitialized
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
            let (private_field_defs, public_field_defs) = if let JsValue::Object(o) = &callee_val
                && let Some(func_obj) = self.get_object(o.id)
            {
                let borrowed = func_obj.borrow();
                (
                    borrowed.class_private_field_defs.clone(),
                    borrowed.class_public_field_defs.clone(),
                )
            } else {
                (Vec::new(), Vec::new())
            };
            let new_obj_id = new_obj.borrow().id.unwrap();
            let this_val = JsValue::Object(crate::types::JsObject { id: new_obj_id });
            let init_env = Environment::new(Some(env.clone()));
            init_env.borrow_mut().declare("this", BindingKind::Const);
            let _ = init_env.borrow_mut().set("this", this_val.clone());
            for def in &private_field_defs {
                match def {
                    PrivateFieldDef::Field { name, initializer } => {
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
                    PrivateFieldDef::Method { name, value } => {
                        if let Some(obj) = self.get_object(new_obj_id) {
                            obj.borrow_mut()
                                .private_fields
                                .insert(name.clone(), PrivateElement::Method(value.clone()));
                        }
                    }
                    PrivateFieldDef::Accessor { name, get, set } => {
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
                }
            }
            for (key, initializer) in &public_field_defs {
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
                if let JsValue::Object(proto_obj) = proto {
                    if let Some(proto_rc) = self.get_object(proto_obj.id) {
                        new_obj.borrow_mut().prototype = Some(proto_rc);
                    }
                }
            }
            let new_obj_id = new_obj.borrow().id.unwrap();
            let this_val = JsValue::Object(crate::types::JsObject { id: new_obj_id });

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
        if let Some(ref nt) = self.new_target.clone() {
            if let JsValue::Object(nt_o) = nt {
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
                    if let JsValue::Object(po) = proto_val {
                        if let Some(proto_rc) = self.get_object(po.id) {
                            if let Some(obj_rc) = self.get_object(obj_id) {
                                obj_rc.borrow_mut().prototype = Some(proto_rc);
                            }
                        }
                    }
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
                let result = self.call_function(&method, right, &[left.clone()]);
                return match result {
                    Completion::Normal(v) => Completion::Normal(JsValue::Boolean(to_boolean(&v))),
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
        if !self.is_callable(ctor) {
            return Completion::Normal(JsValue::Boolean(false));
        }
        let ctor_obj_ref = match ctor {
            JsValue::Object(o) => o.clone(),
            _ => return Completion::Normal(JsValue::Boolean(false)),
        };
        let Some(ctor_data) = self.get_object(ctor_obj_ref.id) else {
            return Completion::Normal(JsValue::Boolean(false));
        };
        let proto_val = ctor_data.borrow().get_property("prototype");
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
        let JsValue::Object(lhs) = obj else {
            return Completion::Normal(JsValue::Boolean(false));
        };
        let Some(inst_obj) = self.get_object(lhs.id) else {
            return Completion::Normal(JsValue::Boolean(false));
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
            let key_val = JsValue::String(JsString::from_str(key));
            let receiver = this_val.clone();
            match self.invoke_proxy_trap(obj_id, "get", vec![target_val.clone(), key_val, receiver])
            {
                Ok(Some(v)) => {
                    // Invariant checks
                    if let JsValue::Object(ref t) = target_val
                        && let Some(tobj) = self.get_object(t.id)
                    {
                        let target_desc = tobj.borrow().get_own_property(key);
                        if let Some(ref desc) = target_desc {
                            if desc.configurable == Some(false) {
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
        if let Some(obj) = self.get_object(obj_id) {
            if let Some(ref ns_data) = obj.borrow().module_namespace.clone() {
                // First try local binding lookup
                if let Some(binding_name) = ns_data.export_to_binding.get(key) {
                    // Handle re-export binding format: *reexport:source:name
                    if let Some(rest) = binding_name.strip_prefix("*reexport:") {
                        // Parse source:name format
                        if let Some(colon_idx) = rest.rfind(':') {
                            let source = &rest[..colon_idx];
                            let export_name = &rest[colon_idx + 1..];
                            // Resolve the source module
                            if let Some(ref module_path) = ns_data.module_path {
                                if let Ok(resolved) =
                                    self.resolve_module_specifier(source, Some(module_path))
                                {
                                    if let Ok(source_mod) = self.load_module(&resolved) {
                                        // Get the source module's export binding to find the env variable
                                        let source_ref = source_mod.borrow();
                                        // Try environment lookup first for live bindings
                                        if let Some(binding) =
                                            source_ref.export_bindings.get(export_name)
                                        {
                                            if let Some(val) = source_ref.env.borrow().get(binding)
                                            {
                                                return Completion::Normal(val);
                                            }
                                        }
                                        // Fallback: direct environment lookup
                                        if let Some(val) = source_ref.env.borrow().get(export_name)
                                        {
                                            return Completion::Normal(val);
                                        }
                                        // Fallback: check exports map
                                        if let Some(val) = source_ref.exports.get(export_name) {
                                            return Completion::Normal(val.clone());
                                        }
                                    }
                                }
                            }
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
                if let Some(ref module_path) = ns_data.module_path {
                    if let Some(module) = self.module_registry.get(module_path) {
                        if let Some(val) = module.borrow().exports.get(key) {
                            return Completion::Normal(val.clone());
                        }
                    }
                }
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
            let key_val = JsValue::String(JsString::from_str(key));
            match self.invoke_proxy_trap(obj_id, "has", vec![target_val.clone(), key_val]) {
                Ok(Some(v)) => {
                    let trap_result = to_boolean(&v);
                    if !trap_result {
                        if let JsValue::Object(ref t) = target_val
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
            let key_val = JsValue::String(JsString::from_str(key));
            match self.invoke_proxy_trap(
                obj_id,
                "set",
                vec![target_val.clone(), key_val, value.clone(), receiver.clone()],
            ) {
                Ok(Some(v)) => {
                    if to_boolean(&v) {
                        if let JsValue::Object(ref t) = target_val
                            && let Some(tobj) = self.get_object(t.id)
                        {
                            let target_desc = tobj.borrow().get_own_property(key);
                            if let Some(ref desc) = target_desc {
                                if desc.configurable == Some(false) {
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
            if is_ta {
                if let Some(index) = canonical_numeric_index_string(key) {
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
            }
            // OrdinarySetWithOwnDescriptor
            let own_desc = obj.borrow().get_own_property(key);
            if let Some(ref desc) = own_desc {
                if desc.is_accessor_descriptor() {
                    // Call setter with receiver as this
                    if let Some(ref setter) = desc.set {
                        if !matches!(setter, JsValue::Undefined) {
                            let setter = setter.clone();
                            match self.call_function(&setter, receiver, &[value]) {
                                Completion::Normal(_) => return Ok(true),
                                Completion::Throw(e) => return Err(e),
                                _ => return Ok(true),
                            }
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
            // No prototype, create data property on receiver
            if let JsValue::Object(recv_o) = receiver {
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
        if let Some(obj) = self.get_object(obj_id) {
            if let Some(ref proto) = obj.borrow().prototype {
                if let Some(pid) = proto.borrow().id {
                    return self.has_proxy_in_prototype_chain(pid);
                }
            }
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
            let key_val = JsValue::String(JsString::from_str(key));
            match self.invoke_proxy_trap(
                obj_id,
                "deleteProperty",
                vec![target_val.clone(), key_val],
            ) {
                Ok(Some(v)) => {
                    let trap_result = to_boolean(&v);
                    if trap_result {
                        if let JsValue::Object(ref t) = target_val
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
    pub(crate) fn proxy_define_own_property(
        &mut self,
        obj_id: u64,
        key: String,
        desc_val: &JsValue,
    ) -> Result<bool, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            let key_val = JsValue::String(JsString::from_str(&key));
            match self.invoke_proxy_trap(
                obj_id,
                "defineProperty",
                vec![target_val.clone(), key_val, desc_val.clone()],
            ) {
                Ok(Some(v)) => {
                    let trap_result = to_boolean(&v);
                    if !trap_result {
                        return Ok(false);
                    }
                    if let JsValue::Object(ref t) = target_val
                        && let Some(tobj) = self.get_object(t.id)
                    {
                        let target_desc = tobj.borrow().get_own_property(&key);
                        let target_extensible = tobj.borrow().extensible;
                        let desc = self.to_property_descriptor(desc_val).ok();
                        if let Some(ref desc) = desc {
                            if !target_extensible && target_desc.is_none() {
                                return Err(self.create_type_error(
                                    "'defineProperty' on proxy: trap returned truish for adding property to the non-extensible proxy target",
                                ));
                            }
                            if let Some(ref td) = target_desc {
                                if td.configurable == Some(false) {
                                    if desc.configurable == Some(true) {
                                        return Err(self.create_type_error(
                                            "'defineProperty' on proxy: trap returned truish for defining non-configurable property which is already non-configurable in the proxy target as configurable",
                                        ));
                                    }
                                    if desc.is_data_descriptor()
                                        && td.is_data_descriptor()
                                        && td.writable == Some(false)
                                        && desc.writable == Some(true)
                                    {
                                        return Err(self.create_type_error(
                                            "'defineProperty' on proxy: trap returned truish for defining non-configurable property which cannot be made writable",
                                        ));
                                    }
                                }
                            }
                            if desc.configurable == Some(false) && target_desc.is_none() {
                                return Err(self.create_type_error(
                                    "'defineProperty' on proxy: trap returned truish for defining non-configurable property which does not exist on the proxy target",
                                ));
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
            let key_val = JsValue::String(JsString::from_str(key));
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
                            if let Some(ref td) = target_desc {
                                if td.configurable == Some(false) {
                                    let trap_desc = self.to_property_descriptor(&v);
                                    if let Ok(ref trap_d) = trap_desc {
                                        if trap_d.configurable == Some(true) {
                                            return Err(self.create_type_error(
                                                "'getOwnPropertyDescriptor' on proxy: trap returned descriptor with configurable: true for non-configurable property in the proxy target",
                                            ));
                                        }
                                        if td.is_data_descriptor()
                                            && td.writable == Some(false)
                                            && trap_d.writable == Some(true)
                                        {
                                            return Err(self.create_type_error(
                                                "'getOwnPropertyDescriptor' on proxy: trap returned descriptor with writable: true for non-configurable non-writable property in the proxy target",
                                            ));
                                        }
                                    }
                                }
                            } else if !target_extensible {
                                return Err(self.create_type_error(
                                    "'getOwnPropertyDescriptor' on proxy: trap returned descriptor for property which does not exist in the non-extensible proxy target",
                                ));
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
                    if let JsValue::Object(arr) = &v
                        && let Some(arr_obj) = self.get_object(arr.id)
                    {
                        let len = match arr_obj.borrow().get_property("length") {
                            JsValue::Number(n) => n as usize,
                            _ => 0,
                        };
                        let keys: Vec<JsValue> = (0..len)
                            .map(|i| arr_obj.borrow().get_property(&i.to_string()))
                            .collect();
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
            let b = obj.borrow();
            Ok(b.property_order
                .iter()
                .map(|k| JsValue::String(JsString::from_str(k)))
                .collect())
        } else {
            Ok(vec![])
        }
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
                    if !to_boolean(&v) {
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
                    let trap_result = to_boolean(&v);
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
                    let trap_result = to_boolean(&v);
                    if trap_result {
                        if let JsValue::Object(ref t) = target_val
                            && let Some(tobj) = self.get_object(t.id)
                            && tobj.borrow().extensible
                        {
                            return Err(self.create_type_error(
                                "'preventExtensions' on proxy: trap returned truish but the proxy target is extensible",
                            ));
                        }
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
            return match &obj_val {
                JsValue::Object(o) => {
                    if let Some(obj) = self.get_object(o.id) {
                        let elem = obj.borrow().private_fields.get(name).cloned();
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
            if let JsValue::Object(ref o) = obj_val {
                if let Some(sup_obj) = self.get_object(o.id) {
                    let proto_val = sup_obj.borrow().get_property("prototype");
                    if let JsValue::Object(ref p) = proto_val {
                        return self.get_object_property(p.id, &key, &this_val);
                    }
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
                } else if let Some(ref sp) = self.string_prototype {
                    Completion::Normal(sp.borrow().get_property(&key))
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            JsValue::Symbol(_) => {
                if let Some(ref sp) = self.symbol_prototype {
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
                if let Some(ref np) = self.number_prototype {
                    Completion::Normal(np.borrow().get_property(&key))
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            JsValue::Boolean(_) => {
                if let Some(ref bp) = self.boolean_prototype {
                    Completion::Normal(bp.borrow().get_property(&key))
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            JsValue::BigInt(_) => {
                if let Some(ref bp) = self.bigint_prototype {
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
        if let Some(ref sv) = super_val {
            if !matches!(sv, JsValue::Null) && !self.is_constructor(sv) {
                return Completion::Throw(
                    self.create_type_error("Class extends value is not a constructor or null"),
                );
            }
        }

        // Create class environment with __super__ binding
        let class_env = Environment::new(Some(env.clone()));
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
                source_text: class_source_text.clone(),
            }
        };

        let ctor_val = self.create_function(ctor_func);

        // Mark derived class constructors and make .prototype writable:false
        if let JsValue::Object(ref o) = ctor_val {
            if let Some(func_obj) = self.get_object(o.id) {
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
            if let JsValue::Object(ref o) = ctor_val {
                if let Some(ctor_obj) = self.get_object(o.id) {
                    ctor_obj.borrow_mut().prototype = Some(super_obj);
                }
            }
        }

        // Handle `extends null` — set prototype's [[Prototype]] to null
        if let Some(JsValue::Null) = super_val {
            if let Some(ref proto) = proto_obj {
                proto.borrow_mut().prototype = None;
            }
        }

        // Add methods and properties to prototype/constructor
        for elem in body {
            match elem {
                ClassElement::Method(m) => {
                    if m.kind == ClassMethodKind::Constructor {
                        continue;
                    }
                    let key = match &m.key {
                        PropertyKey::Identifier(s) | PropertyKey::String(s) => s.clone(),
                        PropertyKey::Number(n) => to_js_string(&JsValue::Number(*n)),
                        PropertyKey::Computed(expr) => match self.eval_expr(expr, env) {
                            Completion::Normal(v) => match self.to_property_key(&v) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            },
                            other => return other,
                        },
                        PropertyKey::Private(name) => {
                            let method_func = JsFunction::User {
                                name: Some(format!("#{name}")),
                                params: m.value.params.clone(),
                                body: m.value.body.clone(),
                                closure: class_env.clone(),
                                is_arrow: false,
                                is_strict: true,
                                is_generator: m.value.is_generator,
                                is_async: m.value.is_async,
                                source_text: m.value.source_text.clone(),
                            };
                            let method_val = self.create_function(method_func);

                            if m.is_static {
                                if let JsValue::Object(ref o) = ctor_val
                                    && let Some(func_obj) = self.get_object(o.id)
                                {
                                    match m.kind {
                                        ClassMethodKind::Get => {
                                            let existing =
                                                func_obj.borrow().private_fields.get(name).cloned();
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
                                                .insert(name.clone(), elem);
                                        }
                                        ClassMethodKind::Set => {
                                            let existing =
                                                func_obj.borrow().private_fields.get(name).cloned();
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
                                                .insert(name.clone(), elem);
                                        }
                                        _ => {
                                            func_obj.borrow_mut().private_fields.insert(
                                                name.clone(),
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
                                        for def in b.class_private_field_defs.iter_mut() {
                                            if let PrivateFieldDef::Accessor {
                                                name: n, get: g, ..
                                            } = def
                                                && n == name
                                            {
                                                *g = Some(method_val.clone());
                                                found = true;
                                                break;
                                            }
                                        }
                                        if !found {
                                            b.class_private_field_defs.push(
                                                PrivateFieldDef::Accessor {
                                                    name: name.clone(),
                                                    get: Some(method_val),
                                                    set: None,
                                                },
                                            );
                                        }
                                    }
                                    ClassMethodKind::Set => {
                                        let mut b = func_obj.borrow_mut();
                                        let mut found = false;
                                        for def in b.class_private_field_defs.iter_mut() {
                                            if let PrivateFieldDef::Accessor {
                                                name: n, set: s, ..
                                            } = def
                                                && n == name
                                            {
                                                *s = Some(method_val.clone());
                                                found = true;
                                                break;
                                            }
                                        }
                                        if !found {
                                            b.class_private_field_defs.push(
                                                PrivateFieldDef::Accessor {
                                                    name: name.clone(),
                                                    get: None,
                                                    set: Some(method_val),
                                                },
                                            );
                                        }
                                    }
                                    _ => {
                                        func_obj.borrow_mut().class_private_field_defs.push(
                                            PrivateFieldDef::Method {
                                                name: name.clone(),
                                                value: method_val,
                                            },
                                        );
                                    }
                                }
                            }
                            continue;
                        }
                    };
                    let method_func = JsFunction::User {
                        name: Some(key.clone()),
                        params: m.value.params.clone(),
                        body: m.value.body.clone(),
                        closure: class_env.clone(),
                        is_arrow: false,
                        is_strict: true,
                        is_generator: m.value.is_generator,
                        is_async: m.value.is_async,
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
                    {
                        if let Some(JsFunction::User { ref closure, .. }) =
                            func_obj.borrow().callable
                        {
                            closure
                                .borrow_mut()
                                .declare("__home_object__", BindingKind::Const);
                            let _ = closure.borrow_mut().set("__home_object__", home_target);
                        }
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
                        if !p.is_static {
                            // Store instance private field definition
                            if let JsValue::Object(ref o) = ctor_val
                                && let Some(func_obj) = self.get_object(o.id)
                            {
                                func_obj.borrow_mut().class_private_field_defs.push(
                                    PrivateFieldDef::Field {
                                        name: name.clone(),
                                        initializer: p.value.clone(),
                                    },
                                );
                            }
                        } else {
                            // Static private field - evaluate now and store on constructor
                            let val = if let Some(ref expr) = p.value {
                                match self.eval_expr(expr, env) {
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
                                    .insert(name.clone(), PrivateElement::Field(val));
                            }
                        }
                        continue;
                    }
                    // Static properties are set on the constructor
                    if p.is_static {
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
                        let val = if let Some(ref expr) = p.value {
                            match self.eval_expr(expr, env) {
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
                    } else {
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
                                .class_public_field_defs
                                .push((key, p.value.clone()));
                        }
                    }
                }
                ClassElement::StaticBlock(body) => {
                    let block_env = Environment::new_function_scope(Some(env.clone()));
                    block_env.borrow_mut().bindings.insert(
                        "this".to_string(),
                        Binding {
                            value: ctor_val.clone(),
                            kind: BindingKind::Const,
                            initialized: true,
                        },
                    );
                    match self.exec_statements(body, &block_env) {
                        Completion::Normal(_) => {}
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => {}
                    }
                }
            }
        }

        Completion::Normal(ctor_val)
    }

    fn eval_object_literal(&mut self, props: &[Property], env: &EnvRef) -> Completion {
        let mut obj_data = JsObjectData::new();
        obj_data.prototype = self.object_prototype.clone();
        for prop in props {
            let key = match &prop.key {
                PropertyKey::Identifier(n) => n.clone(),
                PropertyKey::String(s) => s.clone(),
                PropertyKey::Number(n) => number_ops::to_string(*n),
                PropertyKey::Computed(expr) => {
                    let v = match self.eval_expr(expr, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    match self.to_property_key(&v) {
                        Ok(s) => s,
                        Err(e) => return Completion::Throw(e),
                    }
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
            // Handle spread
            if let Expression::Spread(inner) = &prop.value {
                let spread_val = match self.eval_expr(inner, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if let JsValue::Object(ref o) = spread_val
                    && let Some(src) = self.get_object(o.id)
                {
                    let src_ref = src.borrow();
                    for k in &src_ref.property_order {
                        if let Some(v) = src_ref.properties.get(k) {
                            obj_data.insert_property(k.clone(), v.clone());
                        }
                    }
                }
                continue;
            }
            match prop.kind {
                PropertyKind::Get => {
                    self.set_function_name(&value, &format!("get {key}"));
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
                    self.set_function_name(&value, &format!("set {key}"));
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
                            self.set_function_name(&value, &key);
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
                self.global_env
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
        }

        let result = self.exec_statements(body, &func_env);
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
        } else if let JsValue::Object(o) = val {
            // Check for thenable
            if let Some(obj) = self.get_object(o.id) {
                let then_val = obj.borrow().get_property("then");
                if self.is_callable(&then_val) {
                    let p = self.promise_resolve_value(val);
                    return self.await_value(&p);
                }
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
