use super::*;
use crate::ast::{CallSiteId, PropSiteId};
use crate::interpreter::property::SetOutcome;

mod access;
mod generator_runtime;
mod literals;
mod modules;

/// RAII guard that decrements the interpreter's expression-evaluation depth
/// counter on every exit path of `eval_expr` — the tail return, each of its
/// ~100 early `return`s, and any unwind. It holds an `Rc<Cell<usize>>` rather
/// than a borrow of `Interpreter` so `eval_expr` can keep using `&mut self`
/// across the whole match without a borrow conflict; because the counter lives
/// in its own allocation, nested `&mut self` reborrows never invalidate the
/// handle (a raw pointer into `self` would be unsound here).
struct EvalDepthGuard(std::rc::Rc<std::cell::Cell<usize>>);

impl Drop for EvalDepthGuard {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

pub(super) enum IdentifierRef {
    WithObject(u64),
    Unresolvable,
    SpecificEnv(EnvRef),
}

/// The state a rejected-[[Set]] diagnostic is chosen from, gathered once by
/// `Interpreter::set_rejection_facts`. `[[Set]]` has already decided and
/// discarded its reason by the time a message is needed, so the assignment
/// paths re-derive it here rather than each walking the prototype chain again.
struct SetRejection {
    is_module_namespace: bool,
    has_own: bool,
    /// `None` when the lookup was skipped because a proxy is involved.
    desc: Option<PropertyDescriptor>,
}

/// Pre-evaluated lref for destructuring assignment targets.
/// ToPropertyKey is deferred to PutValue time per spec §13.15.5.
enum DestructLRef {
    /// Regular member: base object + raw key (before ToPropertyKey)
    Member(JsValue, JsValue),
    /// Private field: base object + private name
    Private(JsValue, String),
    /// Super property: super_base_id + property key + this_val + strict
    Super(u64, JsPropertyKey, JsValue, bool),
}

/// Result of pre-evaluating a possible member target for destructuring.
/// Suspension is distinct from the optional-reference channel so callers
/// cannot mistake a yielded evaluation for a non-member target.
enum MemberLhsRef {
    Ref(Option<DestructLRef>),
    Suspended(JsValue),
}

impl Interpreter {
    /// §2.1.1.1 EvaluateImportCall: evaluate an import call's options expression
    /// without inspecting the resulting value. The raw value must survive the
    /// specifier's observable ToString; only after that succeeds may the caller
    /// read `with` and validate its attributes.
    ///
    /// `Err` is an abrupt completion from evaluating the options expression.
    fn eval_import_call_options(
        &mut self,
        options_expr: Option<&Expression>,
        env: &EnvRef,
    ) -> Result<JsValue, Completion> {
        let Some(options_expr) = options_expr else {
            return Ok(JsValue::UNDEFINED);
        };
        match self.eval_expr(options_expr, env) {
            Completion::Normal(v) => Ok(v),
            other => Err(other),
        }
    }

    /// Inspect an already-evaluated import options value after the specifier's
    /// ToString has succeeded. Import attributes are the *enumerable own*
    /// properties of `with`, so inherited or non-enumerable properties are not
    /// attributes. All three dynamic import forms share this path.
    fn import_call_options_type(
        &mut self,
        opts_val: &JsValue,
        callee: &str,
    ) -> Result<Option<super::ImportModuleType>, Completion> {
        self.import_call_module_type(opts_val, callee)
            .map_err(|e| self.create_rejected_promise(e))
    }

    fn import_call_module_type(
        &mut self,
        opts_val: &JsValue,
        callee: &str,
    ) -> Result<Option<super::ImportModuleType>, JsValue> {
        if opts_val.is_undefined() {
            return Ok(None);
        }
        let Some(opts_id) = opts_val.as_object_id() else {
            return Err(self.create_type_error(&format!(
                "The second argument to {callee} must be an object"
            )));
        };
        let wv = match self.get_object_property(opts_id, "with", opts_val) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => return Err(self.create_type_error("Invalid import options")),
        };
        if wv.is_undefined() {
            return Ok(None);
        }
        let Some(with_id) = wv.as_object_id() else {
            return Err(self.create_type_error("The 'with' option must be an object"));
        };

        let mut attrs = Vec::new();
        for k in crate::interpreter::helpers::enumerable_own_keys(self, with_id)? {
            let v = match self.get_object_property(with_id, &k, &wv) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => return Err(self.create_type_error("Invalid import attribute")),
            };
            let Some(sv) = (v).as_string() else {
                return Err(self.create_type_error("Import attribute values must be strings"));
            };
            // Every value must be read and string-checked before
            // AllImportAttributesSupported examines the collected keys.
            attrs.push((k.to_string(), sv.to_rust_string()));
        }
        self.dynamic_import_module_type(&attrs)
    }

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
        let instance_field_defs = if let Some(o) = (new_target_val)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
            && let Some(func_obj) = self.get_object_cell(o.id)
        {
            func_obj.borrow().class_instance_field_defs.clone()
        } else {
            return Ok(());
        };
        let this_obj_id = if let Some(o) = (this_val)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            o.id
        } else {
            return Ok(());
        };
        // Create env for evaluating field initializers.
        // Use the class constructor's closure (class_env) so the class name binding
        // is accessible in field initializers (spec §15.7.14 step 28.e.i).
        let (ctor_closure, class_pn) = if let Some(o) = (new_target_val)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
            && let Some(func_obj) = self.get_object_cell(o.id)
        {
            if let Some(JsFunction::User { ref closure, .. }) = func_obj.borrow().callable {
                let cls_env = closure.borrow();
                (Some(closure.clone()), cls_env.class_private_names.clone())
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };
        let init_parent = ctor_closure.unwrap_or_else(|| env.clone());
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
        if let Some(o) = (new_target_val)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            let proto_val = self.get_property_on_id(o.id, "prototype");
            if proto_val.is_object() {
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
                    if let Some(obj) = self.get_object_cell(this_obj_id) {
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
                    if let Some(obj) = self.get_object_cell(this_obj_id) {
                        if !obj.borrow().extensible {
                            return Err(self.create_type_error(
                                "Cannot define private accessor on non-extensible object",
                            ));
                        }
                        if obj.borrow().private_fields.contains_key(name) {
                            return Err(self
                                .create_type_error("Cannot add private accessor to object twice"));
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
                    let source_name = name.split('#').next().unwrap_or(name);
                    let display_name = format!("#{source_name}");
                    let val = if let Some(init) = initializer {
                        match self.eval_expr(init, &init_env) {
                            Completion::Normal(v) => {
                                if init.is_anonymous_function_definition() {
                                    self.set_function_name(&v, &display_name);
                                }
                                v
                            }
                            Completion::Throw(e) => return Err(e),
                            _ => JsValue::UNDEFINED,
                        }
                    } else {
                        JsValue::UNDEFINED
                    };
                    if let Some(obj) = self.get_object_cell(this_obj_id) {
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
                            Completion::Normal(v) => {
                                if init.is_anonymous_function_definition() {
                                    self.set_function_name(&v, key);
                                }
                                v
                            }
                            Completion::Throw(e) => return Err(e),
                            _ => JsValue::UNDEFINED,
                        }
                    } else {
                        JsValue::UNDEFINED
                    };
                    crate::interpreter::builtins::array::create_data_property_or_throw(
                        self, &this_val, key, val,
                    )?;
                }
                InstanceFieldDef::AutoAccessorStorage(slot_name, initializer) => {
                    let val = if let Some(init) = initializer {
                        match self.eval_expr(init, &init_env) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Err(e),
                            _ => JsValue::UNDEFINED,
                        }
                    } else {
                        JsValue::UNDEFINED
                    };
                    if let Some(obj) = self.get_object_cell(this_obj_id) {
                        obj.borrow_mut()
                            .private_fields
                            .insert(slot_name.clone(), PrivateElement::Field(val));
                    }
                }
                _ => {} // Methods/accessors handled in pass 1
            }
        }
        Ok(())
    }

    fn with_tail_position_suppressed<T>(&mut self, evaluate: impl FnOnce(&mut Self) -> T) -> T {
        let saved_tail = self.in_tail_position;
        self.in_tail_position = false;
        let result = evaluate(self);
        self.in_tail_position = saved_tail;
        result
    }

    pub(crate) fn eval_expr(&mut self, expr: &Expression, env: &EnvRef) -> Completion {
        // Catchable stack-depth guard for expression evaluation. Every
        // expression node — and each recursive descent into an operand —
        // funnels through here, so bounding this depth bounds the native
        // recursion a deeply left-nested AST (`1+1+1+…`, `1&&1&&1…`, parsed in
        // a loop so it never trips the parser's `MAX_PARSE_DEPTH`) would
        // otherwise use to overflow the stack (SIGABRT).
        //
        // Kept a single function on purpose. Splitting the depth accounting
        // into a thin wrapper around an inner `_impl` added a call frame per
        // operand, which roughly *tripled* the native stack each level consumes
        // — lowering the overflow ceiling and eroding the headroom the
        // `call_depth` guard depends on. Instead, `EvalDepthGuard` decrements on
        // every one of this match's exit paths (its many early `return`s and
        // any unwind included), so the counter reflects real native depth
        // without splitting the hot function.
        use crate::interpreter::EVAL_DEPTH_LIMIT;
        let depth = self.eval_depth.get();
        if depth >= EVAL_DEPTH_LIMIT {
            // Check before incrementing: the throw path never touches the counter.
            return Completion::Throw(
                self.create_error("RangeError", "Maximum call stack size exceeded"),
            );
        }
        self.eval_depth.set(depth + 1);
        let _depth_guard = EvalDepthGuard(std::rc::Rc::clone(&self.eval_depth));

        // A call is only ever in tail position if it is (recursively) the
        // return statement's own expression: `return`, through a Conditional's
        // taken branch, a Logical's short-circuited right operand, or a
        // Sequence's last element (mirrors expr_may_contain_tail_call below).
        // Capture the ambient eligibility once and clear it by default so
        // *every* other sub-expression (operands, elements, computed keys,
        // arguments, ...) evaluates as non-tail unless one of those few arms
        // explicitly restores it right before its own recursive dispatch —
        // this makes "not a tail position" the default instead of something
        // each arm has to remember to establish.
        let tail = self.in_tail_position;
        self.in_tail_position = false;
        match expr {
            Expression::Literal(lit) => Completion::Normal(self.eval_literal(lit)),
            Expression::Identifier(name) => {
                let strict = env.borrow().strict;
                self.last_identifier_with_base = None;
                self.resolve_identifier(name, env, strict)
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
                            Completion::Normal(JsValue::UNDEFINED)
                        }
                    }
                }
            }
            Expression::Super => {
                Completion::Normal(env.borrow().get("__super__").unwrap_or(JsValue::UNDEFINED))
            }
            Expression::NewTarget => {
                Completion::Normal(self.new_target.clone().unwrap_or(JsValue::UNDEFINED))
            }
            Expression::PrivateIdentifier(_) => Completion::Throw(
                self.create_type_error("Private identifier can only be used with 'in' operator"),
            ),
            Expression::Unary(op, operand) => {
                // §15.10.2 HasCallInTailPosition: a call nested in a unary
                // expression is not in tail position. This matters when the
                // unary expression is reached through a conditional/logical
                // branch that is otherwise evaluated in tail position: the
                // call result still has to be transformed by the unary
                // operator before the surrounding function can return.
                let val =
                    match self.with_tail_position_suppressed(|this| this.eval_expr(operand, env)) {
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
                    let Some(id) = rval.as_object_id() else {
                        return Completion::Throw(self.create_type_error(
                            "Cannot use 'in' operator to search for a private field without an object",
                        ));
                    };
                    return if let Some(obj) = self.get_object_cell(id) {
                        Completion::Normal(JsValue::boolean(
                            obj.borrow().private_fields.contains_key(&branded),
                        ))
                    } else {
                        Completion::Normal(JsValue::boolean(false))
                    };
                }
                let lval = match self.eval_expr(left, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                // EvaluateStringOrNumericBinaryExpression retains lVal while
                // evaluating rVal, then retains both values while applying the
                // operator. Either phase can run user code and reach a GC
                // safepoint, so keep object operands rooted until the operation
                // has completed.
                self.gc_root_value(&lval);
                let rval = match self.eval_expr(right, env) {
                    Completion::Normal(v) => v,
                    other => {
                        self.gc_unroot_value(&lval);
                        return other;
                    }
                };
                self.gc_root_value(&rval);
                // The fast string-concat arm below moves lval/rval, so snapshot
                // the object operands now for a targeted unroot afterwards. Only
                // objects are ever rooted, and that arm runs only for primitives,
                // so these are cheap handle copies or None.
                let lroot = lval.is_object().then(|| lval.clone());
                let rroot = rval.is_object().then(|| rval.clone());
                let result = if *op == BinaryOp::Instanceof {
                    self.eval_instanceof(&lval, &rval)
                // Fast path for string + on owned primitive values:
                // skip eval_binary → to_primitive → js_value_to_code_units clone chain.
                } else if *op == BinaryOp::Add
                    && !lval.is_object()
                    && !rval.is_object()
                    && (lval.is_string() || rval.is_string())
                {
                    if lval.is_symbol() || rval.is_symbol() {
                        Completion::Throw(
                            self.create_type_error("Cannot convert a Symbol value to a string"),
                        )
                    } else {
                        let mut code_units = if lval.is_string() {
                            lval.into_string().expect("kind checked").into_vec()
                        } else {
                            js_value_to_code_units(&lval)
                        };
                        if rval
                            .with_string(|s| code_units.extend_from_slice(s))
                            .is_none()
                        {
                            code_units.extend(js_value_to_code_units(&rval));
                        }
                        Completion::Normal(JsValue::string(JsString::from_vec(code_units)))
                    }
                } else {
                    self.eval_binary(*op, &lval, &rval)
                };
                // Unroot only the operands rooted for this expression.
                if let Some(ref r) = rroot {
                    self.gc_unroot_value(r);
                }
                if let Some(ref l) = lroot {
                    self.gc_unroot_value(l);
                }
                result
            }
            Expression::Logical(op, left, right) => {
                self.in_tail_position = tail;
                self.eval_logical(*op, left, right, env)
            }
            Expression::Update(op, prefix, arg) => self.eval_update(*op, *prefix, arg, env),
            Expression::Assign(op, left, right) => self.eval_assign(*op, left, right, env),
            Expression::Conditional(test, cons, alt) => {
                let test_val = match self.eval_expr(test, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                self.in_tail_position = tail;
                if self.to_boolean_val(&test_val) {
                    self.eval_expr(cons, env)
                } else {
                    self.eval_expr(alt, env)
                }
            }
            Expression::Call(callee, args, site_id) => {
                self.in_tail_position = tail;
                self.eval_call(callee, args, env, *site_id)
            }
            Expression::New(callee, args, site_id) => self.eval_new(callee, args, env, *site_id),
            Expression::Member(obj, prop, site_id) => self.eval_member(obj, prop, env, *site_id),
            Expression::Array(elements, _) => self.eval_array_literal(elements, env),
            Expression::Object(props) => self.eval_object_literal(props, env),
            Expression::Function(f) => {
                let closure_env = if let Some(ref name) = f.name {
                    let func_env = Rc::new(RefCell::new(Environment {
                        bindings: Default::default(),
                        parent: Some(env.clone()),
                        strict: env.borrow().strict || f.body_is_strict,
                        is_function_scope: false,
                        is_arrow_scope: false,
                        with_object: None,
                        dispose_stack: None,
                        global_object_id: None,
                        annexb_function_names: None,
                        class_private_names: None,
                        is_field_initializer: false,
                        arguments_immutable: false,
                        has_parameter_expressions: false,
                        has_simple_params: true,
                        is_simple_catch_scope: false,
                        is_derived_constructor_scope: false,
                        indirect_bindings: None,
                        module_path: None,
                    }));
                    func_env
                        .borrow_mut()
                        .declare(name, BindingKind::FunctionName);
                    func_env
                } else {
                    env.clone()
                };
                let enclosing_strict = env.borrow().strict;
                let force_method = self.next_function_is_method;
                let func = JsFunction::User {
                    name: f.name.clone(),
                    params: Rc::new(f.params.clone()),
                    body: f.body.clone(),
                    closure: closure_env.clone(),
                    is_arrow: false,
                    is_strict: f.body_is_strict || enclosing_strict,
                    is_generator: f.is_generator,
                    is_async: f.is_async,
                    is_method: force_method,
                    source_text: f.source_text.clone(),
                    captured_new_target: None,
                    uses_arguments: func_uses_arguments(&f.params, &f.body),
                    has_simple_params: crate::ast::params_are_simple(&f.params),
                };
                let func_val = self.create_function(func);
                if let Some(name) = &f.name {
                    let _ = self.env_set(&closure_env, name, func_val.clone());
                }
                Completion::Normal(func_val)
            }
            Expression::ArrowFunction(af) => {
                let enclosing_strict = env.borrow().strict;
                let func = JsFunction::User {
                    name: None,
                    params: Rc::new(af.params.clone()),
                    body: af.body.body().clone(),
                    closure: env.clone(),
                    is_arrow: true,
                    is_strict: af.body_is_strict || enclosing_strict,
                    is_generator: false,
                    is_async: af.is_async,
                    is_method: false,
                    source_text: af.source_text.clone(),
                    captured_new_target: self.new_target.clone(),
                    uses_arguments: false, // arrows never have own arguments
                    has_simple_params: crate::ast::params_are_simple(&af.params),
                };
                Completion::Normal(self.create_function(func))
            }
            Expression::Class(ce) => {
                let name = ce.name.clone().unwrap_or_default();
                self.eval_class(
                    &name,
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
                    if self.with_scope_depth > 0 || self.has_ever_entered_with {
                        match self.resolve_with_has_binding(name, env) {
                            Ok(Some(obj_id)) => {
                                return match self.with_get_binding_value(obj_id, name, strict) {
                                    Completion::Normal(val) => Completion::Normal(JsValue::string(
                                        JsString::from_str(typeof_val(&val, &self.objects)),
                                    )),
                                    other => other,
                                };
                            }
                            Ok(None) => {}
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    if let Some(result) = self.resolve_global_getter(name, env) {
                        return match result {
                            Completion::Normal(val) => Completion::Normal(JsValue::string(
                                JsString::from_str(typeof_val(&val, &self.objects)),
                            )),
                            other => other,
                        };
                    }
                    match self.env_get(env, name) {
                        Some(val) => {
                            return Completion::Normal(JsValue::string(JsString::from_str(
                                typeof_val(&val, &self.objects),
                            )));
                        }
                        None => {
                            if self.env_has(env, name) {
                                return Completion::Throw(self.create_reference_error(&format!(
                                    "Cannot access '{name}' before initialization"
                                )));
                            }
                            return Completion::Normal(JsValue::string(JsString::from_str(
                                "undefined",
                            )));
                        }
                    }
                }
                let val =
                    match self.with_tail_position_suppressed(|this| this.eval_expr(operand, env)) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                Completion::Normal(JsValue::string(JsString::from_str(typeof_val(
                    &val,
                    &self.objects,
                ))))
            }
            Expression::Void(operand) => {
                match self.with_tail_position_suppressed(|this| this.eval_expr(operand, env)) {
                    Completion::Normal(_) => {}
                    other => return other,
                }
                Completion::Normal(JsValue::UNDEFINED)
            }
            Expression::Delete(expr) => match expr.as_ref() {
                Expression::Member(obj_expr, prop, _) => {
                    // §13.5.1.2 step 5a: delete on SuperReference is always ReferenceError.
                    // §13.3.7.1: SuperProperty evaluation calls GetThisBinding() (step 2)
                    // before evaluating the key expression (step 3).
                    if matches!(obj_expr.as_ref(), Expression::Super) {
                        if Self::this_is_in_tdz(env) {
                            return Completion::Throw(self.create_reference_error(
                                "Must call super constructor in derived class before accessing 'this' or returning from derived constructor",
                            ));
                        }
                        if let MemberProperty::Computed(expr) = prop {
                            match self
                                .with_tail_position_suppressed(|this| this.eval_expr(expr, env))
                            {
                                Completion::Normal(_) => {}
                                other => return other,
                            }
                        }
                        return Completion::Throw(
                            self.create_reference_error("Unsupported reference to 'super'"),
                        );
                    }
                    let obj_val = match self
                        .with_tail_position_suppressed(|this| this.eval_expr(obj_expr, env))
                    {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let key = match prop {
                        MemberProperty::Dot(name) => JsPropertyKey::from(name.clone()),
                        MemberProperty::Computed(expr) => {
                            match self
                                .with_tail_position_suppressed(|this| this.eval_expr(expr, env))
                            {
                                Completion::Normal(v) => match self.to_property_key(&v) {
                                    Ok(s) => s,
                                    Err(e) => return Completion::Throw(e),
                                },
                                other => return other,
                            }
                        }
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
                    let obj_ref = if let Some(o) = obj_val
                        .as_object_id()
                        .map(|id| crate::types::JsObject { id })
                    {
                        o.clone()
                    } else {
                        match self.to_object(&obj_val) {
                            Completion::Normal(v) => {
                                let Some(id) = v.as_object_id() else {
                                    return Completion::Normal(JsValue::boolean(true));
                                };
                                crate::types::JsObject { id }
                            }
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => return Completion::Normal(JsValue::boolean(true)),
                        }
                    };
                    if let Some(obj) = self.get_object_cell(obj_ref.id) {
                        // Proxy deleteProperty trap
                        if obj.borrow().is_proxy() || obj.borrow().is_proxy_revoked() {
                            match self.proxy_delete_property(obj_ref.id, &key) {
                                Ok(false) => {
                                    if env.borrow().strict {
                                        return Completion::Throw(self.create_type_error(
                                            &format!("Cannot delete property '{key}' of object"),
                                        ));
                                    }
                                    return Completion::Normal(JsValue::boolean(false));
                                }
                                Ok(result) => return Completion::Normal(JsValue::boolean(result)),
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        // Module namespace exotic: [[Delete]] — only for string keys (not symbols)
                        if !key.is_symbol() {
                            let ns_info = obj
                                .borrow()
                                .module_namespace()
                                .as_ref()
                                .map(|ns| (ns.deferred, ns.export_names.clone()));
                            let ns_obj_id = obj.borrow().id.unwrap();
                            if let Some((deferred, export_names)) = ns_info {
                                if deferred
                                    && !Self::is_symbol_like_namespace_key(&key, true)
                                    && let Err(e) =
                                        self.ensure_deferred_namespace_evaluation(ns_obj_id)
                                {
                                    return Completion::Throw(e);
                                }
                                if key
                                    .as_str()
                                    .is_some_and(|key| export_names.iter().any(|name| name == key))
                                {
                                    if env.borrow().strict {
                                        return Completion::Throw(self.create_type_error(
                                            &format!(
                                                "Cannot delete property '{key}' of module namespace"
                                            ),
                                        ));
                                    }
                                    return Completion::Normal(JsValue::boolean(false));
                                }
                                return Completion::Normal(JsValue::boolean(true));
                            }
                        }
                        // TypedArray: §10.4.5.4 [[Delete]]
                        {
                            let obj_borrow = obj.borrow();
                            if let Some(ta) = obj_borrow.typed_array_info()
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
                                    return Completion::Normal(JsValue::boolean(false));
                                }
                                return Completion::Normal(JsValue::boolean(true));
                            }
                        }
                        let is_strict = env.borrow().strict;
                        // String exotic: length and index properties are non-configurable (§10.4.3.4)
                        {
                            let obj_b = obj.borrow();
                            if let Some(s) =
                                obj_b.primitive_value.as_ref().and_then(JsValue::as_string)
                                && obj_b.class_name == "String"
                            {
                                let is_exotic = key.eq_str("length")
                                    || crate::interpreter::types::string_exotic_index(
                                        &key,
                                        s.code_units.len(),
                                    )
                                    .is_some();
                                if is_exotic {
                                    drop(obj_b);
                                    if is_strict {
                                        return Completion::Throw(self.create_type_error(
                                            &format!("Cannot delete property '{key}' of object"),
                                        ));
                                    }
                                    return Completion::Normal(JsValue::boolean(false));
                                }
                            }
                        }
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
                            return Completion::Normal(JsValue::boolean(false));
                        }
                        obj_mut.remove_property(&key);
                        if let Some(map) = obj_mut.parameter_map_mut()
                            && let Some(key) = key.as_str()
                        {
                            map.remove(key);
                        }
                        if let Ok(idx) = key.parse::<usize>()
                            && let Some(elems) = obj_mut.array_elements_mut()
                            && idx < elems.len()
                        {
                            elems[idx] = JsValue::UNDEFINED;
                        }
                    }
                    Completion::Normal(JsValue::boolean(true))
                }
                Expression::Identifier(name) => {
                    // Check with-scopes first (Bug C fix)
                    if self.with_scope_depth > 0 || self.has_ever_entered_with {
                        match self.resolve_with_has_binding(name, env) {
                            Ok(Some(obj_id)) => {
                                return match self.proxy_delete_property(obj_id, name) {
                                    Ok(b) => Completion::Normal(JsValue::boolean(b)),
                                    Err(e) => Completion::Throw(e),
                                };
                            }
                            Ok(None) => {}
                            Err(e) => return Completion::Throw(e),
                        }
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
                                return Completion::Normal(JsValue::boolean(true));
                            }
                            return Completion::Normal(JsValue::boolean(false));
                        }
                        let next = eb.parent.clone();
                        drop(eb);
                        current = next;
                    }

                    // At global level — check global object property descriptor
                    let global_id = self.realm().global_env.borrow().global_object_id;
                    if let Some(gid) = global_id
                        && let Some(global) = self.get_object_cell(gid)
                    {
                        let gb = global.borrow();
                        if let Some(desc) = gb.properties.get(name) {
                            if desc.configurable == Some(false) {
                                return Completion::Normal(JsValue::boolean(false));
                            }
                            drop(gb);
                            global.borrow_mut().remove_property(name);
                            self.realm().global_env.borrow_mut().bindings.remove(name);
                            return Completion::Normal(JsValue::boolean(true));
                        }
                    }
                    // Check if it's a binding in the global env (var declaration not on global object)
                    if self.realm().global_env.borrow().bindings.contains_key(name) {
                        return Completion::Normal(JsValue::boolean(false));
                    }
                    // Unresolvable reference — return true per spec
                    Completion::Normal(JsValue::boolean(true))
                }
                Expression::OptionalChain(base, chain) => {
                    self.with_tail_position_suppressed(|this| {
                        this.eval_delete_optional_chain(base, chain, env)
                    })
                }
                _ => {
                    // Evaluate the expression for side effects, then return true
                    match self.with_tail_position_suppressed(|this| this.eval_expr(expr, env)) {
                        Completion::Normal(_) => Completion::Normal(JsValue::boolean(true)),
                        other => other,
                    }
                }
            },
            Expression::Sequence(exprs) | Expression::Comma(exprs) => {
                let last_idx = exprs.len().saturating_sub(1);
                let mut result = JsValue::UNDEFINED;
                for (i, e) in exprs.iter().enumerate() {
                    self.in_tail_position = if i == last_idx { tail } else { false };
                    match self.eval_expr(e, env) {
                        Completion::Normal(v) => result = v,
                        other => return other,
                    }
                }
                Completion::Normal(result)
            }
            Expression::Spread(_) => Completion::Normal(JsValue::UNDEFINED), // handled by caller
            Expression::Yield(expr, delegate) => {
                if *delegate {
                    let iterable = if let Some(e) = expr {
                        match self.eval_expr(e, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        }
                    } else {
                        JsValue::UNDEFINED
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
                    let gc_frame = self.gc_root_frame();
                    if let Some(o) = iterator
                        .as_object_id()
                        .map(|id| crate::types::JsObject { id })
                    {
                        self.gc_temp_roots.push(o.id);
                    }
                    let result = loop {
                        let next_result = match self.iterator_next(&iterator) {
                            Ok(v) => v,
                            Err(e) => {
                                self.gc_unroot_frame(gc_frame);
                                return Completion::Throw(e);
                            }
                        };
                        let next_result = if is_async_gen {
                            match self.await_value(&next_result) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => {
                                    self.gc_unroot_frame(gc_frame);
                                    return Completion::Throw(e);
                                }
                                other => {
                                    self.gc_unroot_frame(gc_frame);
                                    return other;
                                }
                            }
                        } else {
                            next_result
                        };
                        let done = match self.iterator_complete(&next_result) {
                            Ok(d) => d,
                            Err(e) => {
                                self.gc_unroot_frame(gc_frame);
                                return Completion::Throw(e);
                            }
                        };
                        let value = match self.iterator_value(&next_result) {
                            Ok(v) => v,
                            Err(e) => {
                                self.gc_unroot_frame(gc_frame);
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
                                match &ctx.resume_kind {
                                    GeneratorResumeKind::Next => {
                                        self.gc_unroot_frame(gc_frame);
                                        return Completion::Yield(value);
                                    }
                                    GeneratorResumeKind::Return(v) => {
                                        let v = v.clone();
                                        self.gc_unroot_frame(gc_frame);
                                        return Completion::Return(v);
                                    }
                                    GeneratorResumeKind::Throw(e) => {
                                        let e = e.clone();
                                        self.gc_unroot_frame(gc_frame);
                                        return Completion::Throw(e);
                                    }
                                }
                            }
                        }
                        self.gc_unroot_frame(gc_frame);
                        return Completion::Yield(value);
                    };
                    self.gc_unroot_frame(gc_frame);
                    result
                } else {
                    let value = if let Some(e) = expr {
                        match self.eval_expr(e, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        }
                    } else {
                        JsValue::UNDEFINED
                    };
                    if let Some(ctx) = self.generator_context.as_mut() {
                        let current = ctx.current_yield;
                        ctx.current_yield += 1;
                        if current < ctx.target_yield {
                            // Fast-forwarding past this yield — use the historically sent value
                            let ff_val = ctx
                                .prev_sent_values
                                .get(current)
                                .cloned()
                                .unwrap_or(JsValue::UNDEFINED);
                            return Completion::Normal(ff_val);
                        }
                        if current == ctx.target_yield {
                            match &ctx.resume_kind {
                                GeneratorResumeKind::Next => {}
                                GeneratorResumeKind::Return(v) => {
                                    return Completion::Return(v.clone());
                                }
                                GeneratorResumeKind::Throw(e) => {
                                    return Completion::Throw(e.clone());
                                }
                            }
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
                // §16.2.1.5.2 GetActiveScriptOrModule: walk env chain to find module path
                let module_path =
                    Environment::find_module_path(env).or_else(|| self.current_module_path.clone());
                if let Some(ref path) = module_path {
                    let canon = path.canonicalize();
                    if let Some(module) = self.module_registry_get(&canon)
                        && let Some(ref cached) = module.borrow().cached_import_meta
                    {
                        return Completion::Normal(cached.clone());
                    }
                }
                let meta_id = self.create_object_id();
                self.get_object_cell_expect(meta_id)
                    .borrow_mut()
                    .prototype_id = None;
                if let Some(path) = module_path.as_ref().and_then(ModuleKey::file_path) {
                    let url = format!("file://{}", path.display());
                    self.get_object_cell_expect(meta_id)
                        .borrow_mut()
                        .insert_property(
                            "url".to_string(),
                            PropertyDescriptor::data(
                                JsValue::string(JsString::from_str(&url)),
                                true,
                                true,
                                true,
                            ),
                        );
                }
                let id = meta_id;
                let meta_val = JsValue::object(id);
                if let Some(ref path) = module_path {
                    let canon = path.canonicalize();
                    if let Some(module) = self.module_registry_get(&canon) {
                        module.borrow_mut().cached_import_meta = Some(meta_val.clone());
                    }
                }
                Completion::Normal(meta_val)
            }
            Expression::Import(source_expr, options_expr) => {
                // Dynamic import() - returns a Promise
                // §2.1.1.1 EvaluateImportCall: evaluate specifier and options synchronously
                let source_val = match self.eval_expr(source_expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let options_val = match self.eval_import_call_options(options_expr.as_deref(), env)
                {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                // Per spec: ToString(specifier) errors produce a rejected promise
                let source = match self.to_string_value(&source_val) {
                    Ok(s) => s,
                    Err(e) => return self.create_rejected_promise(e),
                };
                let dynamic_import_type =
                    match self.import_call_options_type(&options_val, "import()") {
                        Ok(t) => t,
                        Err(c) => return c,
                    };
                self.dynamic_import(&source, dynamic_import_type)
            }
            Expression::ImportDefer(source_expr, options_expr) => {
                let source_val = match self.eval_expr(source_expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let options_val = match self.eval_import_call_options(options_expr.as_deref(), env)
                {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let source = match self.to_string_value(&source_val) {
                    Ok(s) => s,
                    Err(e) => return self.create_rejected_promise(e),
                };
                let defer_import_type =
                    match self.import_call_options_type(&options_val, "import.defer()") {
                        Ok(t) => t,
                        Err(c) => return c,
                    };
                // import.defer() loads module without evaluation, returns deferred namespace
                // but eagerly evaluates async transitive deps (spec ContinueDynamicImport step 25)
                let module_path = self.current_module_path.clone();
                let resolved = match self.resolve_module_specifier(
                    &source,
                    module_path.as_ref().and_then(ModuleKey::file_path),
                ) {
                    Ok(r) => r,
                    Err(e) => return self.create_rejected_promise(e),
                };
                match self.load_module_for_type(
                    &resolved,
                    defer_import_type,
                    super::ModuleLoadMode::Defer,
                ) {
                    Ok(module) => {
                        let resolved_canon = resolved.canonicalize();
                        self.evaluate_async_transitive_deps(&resolved_canon);
                        self.drain_microtasks();
                        let ns = self.create_deferred_module_namespace(&module);
                        self.create_resolved_promise(ns)
                    }
                    Err(e) => self.create_rejected_promise(e),
                }
            }
            Expression::ImportSource(source_expr, options_expr) => {
                let source_val = match self.eval_expr(source_expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let options_val = match self.eval_import_call_options(options_expr.as_deref(), env)
                {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let source = match self.to_string_value(&source_val) {
                    Ok(s) => s,
                    Err(e) => return self.create_rejected_promise(e),
                };
                let source_import_type =
                    match self.import_call_options_type(&options_val, "import.source()") {
                        Ok(t) => t,
                        Err(c) => return c,
                    };
                // ContinueDynamicImport with source phase: resolve to the
                // target module's [[ModuleSource]]. A Source Text Module has an
                // empty [[ModuleSource]] (GetModuleSource throws SyntaxError).
                let referrer = self.current_module_path.clone();
                match self.resolve_source_phase_target(
                    &source,
                    referrer.as_ref().and_then(ModuleKey::file_path),
                    source_import_type,
                ) {
                    Ok((_, Some(ms))) => self.create_resolved_promise(ms),
                    Ok((_, None)) => {
                        let err = self.create_error(
                            "SyntaxError",
                            "Source phase imports are not available for this module",
                        );
                        self.create_rejected_promise(err)
                    }
                    Err(e) => self.create_rejected_promise(e),
                }
            }
            Expression::Template(tmpl) => {
                let mut code_units: Vec<u16> = Vec::new();
                for (i, quasi) in tmpl.quasis.iter().enumerate() {
                    if let Some(q) = quasi {
                        code_units.extend_from_slice(q);
                    }
                    if i < tmpl.expressions.len() {
                        let val = match self.eval_expr(&tmpl.expressions[i], env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let str_val = match self.to_string_value(&val) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };
                        code_units.extend(str_val.encode_utf16());
                    }
                }
                Completion::Normal(JsValue::string(JsString::from_vec(code_units)))
            }
            Expression::OptionalChain(base, prop) => {
                let (base_val, base_this) = match self.eval_oc_base(base, prop, env) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if (base_val).is_nullish() {
                    return Completion::Normal(JsValue::UNDEFINED);
                }
                self.eval_optional_chain_tail_with_base_this(&base_val, &base_this, prop, env)
            }
            Expression::TaggedTemplate(tag_expr, tmpl) => {
                let (func_val, this_val) = match tag_expr.as_ref() {
                    Expression::Member(obj_expr, prop, _) => {
                        let obj_val = match self.eval_expr(obj_expr, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let key = match prop {
                            MemberProperty::Dot(name) => JsPropertyKey::from(name.clone()),
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
                        let func = obj_val.as_object_id().map_or_else(
                            || Completion::Normal(JsValue::UNDEFINED),
                            |id| self.get_object_property(id, &key, &obj_val),
                        );
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
                        (func, JsValue::UNDEFINED)
                    }
                };

                // The tag, its receiver, and each substitution value are held
                // in Rust locals until EvaluateCall invokes the tag. Later
                // substitutions can run arbitrary JavaScript (and therefore a
                // GC safepoint), so keep all previously evaluated values live.
                let gc_frame = self.gc_root_frame();
                self.gc_root_value(&func_val);
                self.gc_root_value(&this_val);
                let template_obj = self.get_template_object(tmpl);
                self.gc_root_value(&template_obj);

                let mut call_args = vec![template_obj];
                for sub_expr in &tmpl.expressions {
                    match self.eval_expr(sub_expr, env) {
                        Completion::Normal(v) => {
                            self.gc_root_value(&v);
                            call_args.push(v);
                        }
                        other => {
                            self.gc_unroot_frame(gc_frame);
                            return other;
                        }
                    }
                }

                if tail {
                    self.gc_unroot_frame(gc_frame);
                    return Completion::TailCall {
                        func: func_val,
                        this: this_val,
                        args: call_args,
                    };
                }
                let result = self.call_function(&func_val, &this_val, &call_args);
                self.gc_unroot_frame(gc_frame);
                result
            }
        }
    }

    fn access_property_on_value<K: PropertyKeyLike + ?Sized>(
        &mut self,
        base_val: &JsValue,
        name: &K,
    ) -> Completion {
        // §6.2.5.5 GetValue permits eliding the transient primitive wrapper,
        // but its prototype [[Get]] must still receive the primitive itself.
        let name = name.to_js_property_key();
        if let Some(id) = base_val.as_object_id() {
            self.get_object_property(id, &name, base_val)
        } else if let Some(s) = base_val.as_string() {
            if name.eq_str("length") {
                Completion::Normal(JsValue::number(s.len() as f64))
            } else if let Some(idx) =
                crate::interpreter::types::string_exotic_index(&name, s.code_units.len())
            {
                Completion::Normal(JsValue::string(JsString::from_vec(vec![s.code_units[idx]])))
            } else if let Some(sp_id) = self.realm().string_prototype {
                self.get_object_property(sp_id, &name, base_val)
            } else {
                Completion::Normal(JsValue::UNDEFINED)
            }
        } else if base_val.is_number() {
            if let Some(np_id) = self.realm().number_prototype {
                self.get_object_property(np_id, &name, base_val)
            } else {
                Completion::Normal(JsValue::UNDEFINED)
            }
        } else if base_val.is_boolean() {
            if let Some(bp_id) = self.realm().boolean_prototype {
                self.get_object_property(bp_id, &name, base_val)
            } else {
                Completion::Normal(JsValue::UNDEFINED)
            }
        } else if base_val.is_symbol() {
            if let Some(sp_id) = self.realm().symbol_prototype {
                self.get_object_property(sp_id, &name, base_val)
            } else {
                Completion::Normal(JsValue::UNDEFINED)
            }
        } else if base_val.is_bigint() {
            if let Some(bp_id) = self.realm().bigint_prototype {
                self.get_object_property(bp_id, &name, base_val)
            } else {
                Completion::Normal(JsValue::UNDEFINED)
            }
        } else {
            Completion::Normal(JsValue::UNDEFINED)
        }
    }

    // §7.1.14 ToPropertyKey
    pub(super) fn eval_unary(&mut self, op: UnaryOp, val: &JsValue) -> Completion {
        match op {
            UnaryOp::Minus => {
                let numeric = match self.to_numeric(val) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                if let Some(b) = numeric.as_bigint() {
                    Completion::Normal(JsValue::bigint(JsBigInt::new(bigint_ops::unary_minus(
                        &b.value,
                    ))))
                } else {
                    Completion::Normal(JsValue::number(number_ops::unary_minus(
                        numeric.as_number().expect("ToNumeric result"),
                    )))
                }
            }
            UnaryOp::Plus => match self.to_number_value(val) {
                Ok(n) => Completion::Normal(JsValue::number(n)),
                Err(e) => Completion::Throw(e),
            },
            UnaryOp::Not => Completion::Normal(JsValue::boolean(!self.to_boolean_val(val))),
            UnaryOp::BitNot => {
                let numeric = match self.to_numeric(val) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                if let Some(b) = numeric.as_bigint() {
                    Completion::Normal(JsValue::bigint(JsBigInt::new(bigint_ops::bitwise_not(
                        &b.value,
                    ))))
                } else {
                    Completion::Normal(JsValue::number(number_ops::bitwise_not(
                        numeric.as_number().expect("ToNumeric result"),
                    )))
                }
            }
        }
    }

    fn require_object_coercible(&mut self, val: &JsValue) -> Completion {
        if val.is_nullish() {
            let err = self.create_type_error("Cannot convert undefined or null to object");
            Completion::Throw(err)
        } else {
            Completion::Normal(val.clone())
        }
    }

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_index(&mut self, val: &JsValue) -> Completion {
        if val.is_undefined() {
            return Completion::Normal(JsValue::number(0.0));
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
        Completion::Normal(JsValue::number(integer_index))
    }

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_object(&mut self, val: &JsValue) -> Completion {
        if val.is_nullish() {
            let err = self.create_type_error("Cannot convert undefined or null to object");
            return Completion::Throw(err);
        }
        if val.is_object() {
            return Completion::Normal(val.clone());
        }
        let mut obj_data = JsObjectData::new();
        obj_data.primitive_value = Some(val.clone());
        if val.is_string() {
            obj_data.class_name = "String".to_string();
            obj_data.prototype_id = self.realm().string_prototype;
        } else if val.is_number() {
            obj_data.class_name = "Number".to_string();
            obj_data.prototype_id = self.realm().number_prototype;
        } else if val.is_boolean() {
            obj_data.class_name = "Boolean".to_string();
            obj_data.prototype_id = self.realm().boolean_prototype;
        } else if val.is_symbol() {
            obj_data.class_name = "Symbol".to_string();
            obj_data.prototype_id = self.realm().symbol_prototype;
        } else if val.is_bigint() {
            obj_data.class_name = "BigInt".to_string();
            obj_data.prototype_id = self.realm().bigint_prototype;
        } else {
            unreachable!();
        }
        if obj_data.prototype_id.is_none() {
            obj_data.prototype_id = self.realm().object_prototype;
        }
        let id = self.alloc_object(obj_data);
        Completion::Normal(JsValue::object(id))
    }

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_primitive(
        &mut self,
        val: &JsValue,
        preferred_type: &str,
    ) -> Result<JsValue, JsValue> {
        if let Some(id) = val.as_object_id() {
            // §7.1.1 Step 2-3: Check @@toPrimitive
            let exotic_to_prim = {
                let key = JsPropertyKey::well_known_symbol("toPrimitive");
                match self.get_object_property(id, &key, val) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::UNDEFINED,
                }
            };
            if !(exotic_to_prim).is_nullish() {
                if let Some(fo) = exotic_to_prim
                    .as_object_id()
                    .map(|id| crate::types::JsObject { id })
                    && self
                        .get_object_cell(fo.id)
                        .map(|o| o.borrow().callable.is_some())
                        .unwrap_or(false)
                {
                    let hint = JsValue::string(JsString::from_str(preferred_type));
                    let result = self.call_function(&exotic_to_prim, val, &[hint]);
                    match result {
                        Completion::Normal(v) if !(v).is_object() => {
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
                let method_val = match self.get_object_property(id, method_name, val) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::UNDEFINED,
                };
                if let Some(fo) = method_val
                    .as_object_id()
                    .map(|id| crate::types::JsObject { id })
                    && self
                        .get_object_cell(fo.id)
                        .map(|o| o.borrow().callable.is_some())
                        .unwrap_or(false)
                {
                    let result = self.call_function(&method_val, val, &[]);
                    match result {
                        Completion::Normal(v) if !(v).is_object() => {
                            return Ok(v);
                        }
                        Completion::Throw(e) => return Err(e),
                        _ => {}
                    }
                }
            }
            Err(self.create_type_error("Cannot convert object to primitive value"))
        } else {
            Ok(val.clone())
        }
    }

    // §7.1.3 ToNumeric: ToPrimitive(number), then BigInt stays BigInt, else ToNumber
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_numeric(&mut self, val: &JsValue) -> Result<JsValue, JsValue> {
        let prim = self.to_primitive(val, "number")?;
        if prim.is_bigint() {
            Ok(prim)
        } else if (prim).is_symbol() {
            Err(self.create_type_error("Cannot convert a Symbol value to a number"))
        } else {
            Ok(JsValue::number(to_number(&prim)))
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
        if val.is_undefined() {
            Ok("undefined".to_string())
        } else if val.is_null() {
            Ok("null".to_string())
        } else if let Some(b) = val.as_boolean() {
            Ok(if b { "true" } else { "false" }.to_string())
        } else if let Some(n) = val.as_number() {
            Ok(number_ops::to_string(n))
        } else if let Some(s) = val.as_string() {
            Ok(s.to_rust_string())
        } else if val.is_symbol() {
            Err(self.create_type_error("Cannot convert a Symbol value to a string"))
        } else if let Some(n) = val.as_bigint() {
            Ok(n.value.to_string())
        } else {
            let prim = self.to_primitive(val, "string")?;
            self.to_string_value(&prim)
        }
    }

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_js_string(&mut self, val: &JsValue) -> Result<JsString, JsValue> {
        if let Some(s) = val.as_string() {
            Ok(s)
        } else {
            let s = self.to_string_value(val)?;
            Ok(JsString::from_str(&s))
        }
    }

    // §7.1.4 ToNumber — calls ToPrimitive for objects
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_number_value(&mut self, val: &JsValue) -> Result<f64, JsValue> {
        if val.is_object() {
            let prim = self.to_primitive(val, "number")?;
            self.to_number_value(&prim)
        } else if val.is_symbol() {
            Err(self.create_type_error("Cannot convert a Symbol value to a number"))
        } else if val.is_bigint() {
            Err(self.create_type_error("Cannot convert a BigInt value to a number"))
        } else {
            Ok(to_number(val))
        }
    }

    // §7.1.5 ToIntegerOrInfinity — `? ToNumber(argument)` then truncate toward
    // zero (NaN → +0, ±∞ pass through). The combined coercion that callers used
    // to open-code as a `to_number_value` match feeding `to_integer_or_infinity`;
    // this is the spec abstract operation as a single named step, alongside
    // to_number_value / to_string_value / to_index.
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_integer_or_infinity_value(&mut self, val: &JsValue) -> Result<f64, JsValue> {
        let n = self.to_number_value(val)?;
        Ok(to_integer_or_infinity(n))
    }

    // §7.1.13 ToBigInt
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_bigint_value(&mut self, val: &JsValue) -> Result<JsValue, JsValue> {
        let prim = if val.is_object() {
            self.to_primitive(val, "number")?
        } else {
            val.clone()
        };
        if prim.is_bigint() {
            Ok(prim)
        } else if let Some(b) = prim.as_boolean() {
            Ok(JsValue::bigint(crate::types::JsBigInt::new(if b {
                num_bigint::BigInt::from(1)
            } else {
                num_bigint::BigInt::from(0)
            })))
        } else if let Some(s) = prim.as_string() {
            let text = s.to_rust_string();
            match crate::interpreter::helpers::string_to_bigint(&text) {
                Some(n) => Ok(JsValue::bigint(crate::types::JsBigInt::new(n))),
                None => Err(self.create_error(
                    "SyntaxError",
                    &format!("Cannot convert {} to a BigInt", text),
                )),
            }
        } else if prim.is_undefined() {
            Err(self.create_type_error("Cannot convert undefined to a BigInt"))
        } else if prim.is_null() {
            Err(self.create_type_error("Cannot convert null to a BigInt"))
        } else if prim.is_number() {
            Err(self.create_type_error("Cannot convert a Number to a BigInt"))
        } else if prim.is_symbol() {
            Err(self.create_type_error("Cannot convert a Symbol to a BigInt"))
        } else {
            Err(self.create_type_error("Cannot convert to BigInt"))
        }
    }

    fn abstract_equality(&mut self, left: &JsValue, right: &JsValue) -> Result<bool, JsValue> {
        if left.kind() == right.kind() {
            return Ok(strict_equality(left, right));
        }
        // B.3.6.2: IsHTMLDDA == null/undefined
        if let Some(o) = (left)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
            && let Some(obj) = self.objects.get(o.id)
            && obj.borrow().is_htmldda
            && (right.is_null() || right.is_undefined())
        {
            return Ok(true);
        }
        if let Some(o) = (right)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
            && let Some(obj) = self.objects.get(o.id)
            && obj.borrow().is_htmldda
            && (left.is_null() || left.is_undefined())
        {
            return Ok(true);
        }
        if (left.is_null() && right.is_undefined()) || (left.is_undefined() && right.is_null()) {
            return Ok(true);
        }
        if left.is_number() && right.is_string() {
            return self.abstract_equality(left, &JsValue::number(to_number(right)));
        }
        if left.is_string() && right.is_number() {
            return self.abstract_equality(&JsValue::number(to_number(left)), right);
        }
        if left.is_boolean() {
            return self.abstract_equality(&JsValue::number(to_number(left)), right);
        }
        if right.is_boolean() {
            return self.abstract_equality(left, &JsValue::number(to_number(right)));
        }
        // BigInt == Number
        if let Some((b, n)) = left
            .as_bigint()
            .zip(right.as_number())
            .or_else(|| right.as_bigint().zip(left.as_number()))
        {
            if n.is_nan() || n.is_infinite() {
                return Ok(false);
            }
            if n != n.trunc() {
                return Ok(false);
            }
            let n_as_bigint = crate::interpreter::builtins::bigint::f64_to_bigint(n);
            return Ok(bigint_ops::equal(&b.value, &n_as_bigint));
        }
        // BigInt == String (via StringToBigInt)
        if let (Some(b), Some(s)) = (left.as_bigint(), right.as_string()) {
            if let Some(parsed) = crate::interpreter::helpers::string_to_bigint(&s.to_rust_string())
            {
                return Ok(bigint_ops::equal(&b.value, &parsed));
            }
            return Ok(false);
        }
        if let (Some(s), Some(b)) = (left.as_string(), right.as_bigint()) {
            if let Some(parsed) = crate::interpreter::helpers::string_to_bigint(&s.to_rust_string())
            {
                return Ok(bigint_ops::equal(&parsed, &b.value));
            }
            return Ok(false);
        }
        // Object vs primitive (including BigInt)
        if (left).is_object()
            && (right.is_string() || right.is_number() || right.is_symbol() || right.is_bigint())
        {
            let lprim = self.to_primitive(left, "default")?;
            return self.abstract_equality(&lprim, right);
        }
        if (right).is_object()
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
        self.abstract_relational_inner(left, right, true)
    }

    /// §7.2.13 IsLessThan(x, y, LeftFirst)
    fn abstract_relational_inner(
        &mut self,
        left: &JsValue,
        right: &JsValue,
        left_first: bool,
    ) -> Result<Option<bool>, JsValue> {
        let (lprim, rprim) = if left_first {
            let l = self.to_primitive(left, "number")?;
            let r = self.to_primitive(right, "number")?;
            (l, r)
        } else {
            let r = self.to_primitive(right, "number")?;
            let l = self.to_primitive(left, "number")?;
            (l, r)
        };
        if is_string(&lprim) && is_string(&rprim) {
            // §7.2.13 step 3: Compare by UTF-16 code units, not UTF-8 bytes
            return Ok(Some(
                lprim.as_string().expect("kind checked").code_units
                    < rprim.as_string().expect("kind checked").code_units,
            ));
        }
        // BigInt comparisons
        if let (Some(a), Some(b)) = (lprim.as_bigint(), rprim.as_bigint()) {
            return Ok(bigint_ops::less_than(&a.value, &b.value));
        }
        if let (Some(b), Some(n)) = (lprim.as_bigint(), rprim.as_number()) {
            if n.is_nan() {
                return Ok(None);
            }
            if n == f64::INFINITY {
                return Ok(Some(true));
            }
            if n == f64::NEG_INFINITY {
                return Ok(Some(false));
            }
            let n_trunc = n.trunc();
            let n_floor = crate::interpreter::builtins::bigint::f64_to_bigint(n_trunc);
            if *b.value < n_floor {
                return Ok(Some(true));
            }
            if *b.value > n_floor {
                return Ok(Some(false));
            }
            // n_floor == b.value, so result depends on fractional part
            return Ok(Some(n_trunc < n));
        }
        if let (Some(n), Some(b)) = (lprim.as_number(), rprim.as_bigint()) {
            if n.is_nan() {
                return Ok(None);
            }
            if n == f64::NEG_INFINITY {
                return Ok(Some(true));
            }
            if n == f64::INFINITY {
                return Ok(Some(false));
            }
            let n_trunc = n.trunc();
            let n_floor = crate::interpreter::builtins::bigint::f64_to_bigint(n_trunc);
            if n_floor < *b.value {
                return Ok(Some(true));
            }
            if n_floor > *b.value {
                return Ok(Some(false));
            }
            // n_floor == b.value, so result depends on fractional part
            return Ok(Some(n < n_trunc));
        }
        // BigInt vs String: try parsing via StringToBigInt
        if lprim.is_bigint()
            && let Some(s) = rprim.as_string()
        {
            if let Some(parsed) = crate::interpreter::helpers::string_to_bigint(&s.to_rust_string())
            {
                return self.abstract_relational(&lprim, &JsValue::bigint(JsBigInt::new(parsed)));
            }
            return Ok(None);
        }
        if let Some(s) = lprim.as_string()
            && rprim.is_bigint()
        {
            if let Some(parsed) = crate::interpreter::helpers::string_to_bigint(&s.to_rust_string())
            {
                return self.abstract_relational(&JsValue::bigint(JsBigInt::new(parsed)), &rprim);
            }
            return Ok(None);
        }
        // ToNumeric: convert non-BigInt primitives to Number, keep BigInt as BigInt
        if (lprim).is_symbol() || (rprim).is_symbol() {
            return Err(self.create_type_error("Cannot convert a Symbol value to a number"));
        }
        let lnum = if (lprim).is_bigint() {
            lprim
        } else {
            JsValue::number(to_number(&lprim))
        };
        let rnum = if (rprim).is_bigint() {
            rprim
        } else {
            JsValue::number(to_number(&rprim))
        };
        // After ToNumeric, re-check BigInt vs Number cases
        if let (Some(a), Some(b)) = (lnum.as_bigint(), rnum.as_bigint()) {
            return Ok(bigint_ops::less_than(&a.value, &b.value));
        }
        if let (Some(b), Some(n)) = (lnum.as_bigint(), rnum.as_number()) {
            if n.is_nan() {
                return Ok(None);
            }
            if n == f64::INFINITY {
                return Ok(Some(true));
            }
            if n == f64::NEG_INFINITY {
                return Ok(Some(false));
            }
            let n_trunc = n.trunc();
            let n_floor = crate::interpreter::builtins::bigint::f64_to_bigint(n_trunc);
            if *b.value < n_floor {
                return Ok(Some(true));
            }
            if *b.value > n_floor {
                return Ok(Some(false));
            }
            return Ok(Some(n_trunc < n));
        }
        if let (Some(n), Some(b)) = (lnum.as_number(), rnum.as_bigint()) {
            if n.is_nan() {
                return Ok(None);
            }
            if n == f64::NEG_INFINITY {
                return Ok(Some(true));
            }
            if n == f64::INFINITY {
                return Ok(Some(false));
            }
            let n_trunc = n.trunc();
            let n_floor = crate::interpreter::builtins::bigint::f64_to_bigint(n_trunc);
            if n_floor < *b.value {
                return Ok(Some(true));
            }
            if n_floor > *b.value {
                return Ok(Some(false));
            }
            return Ok(Some(n < n_trunc));
        }
        if let (Some(ln), Some(rn)) = (lnum.as_number(), rnum.as_number()) {
            return Ok(number_ops::less_than(ln, rn));
        }
        Ok(None)
    }

    pub(super) fn dispatch_body(
        &mut self,
        func_obj_id: u64,
        body: &Body,
        exec_env: &EnvRef,
        this_val: &JsValue,
    ) -> Completion {
        if self.bytecode_enabled {
            use crate::interpreter::bytecode::{BytecodeCacheState, compiler, vm};
            let cache_state = self
                .get_object(func_obj_id)
                .map(|o| o.borrow().bytecode_cache.clone())
                .unwrap_or(BytecodeCacheState::Untried);
            let chunk = match cache_state {
                BytecodeCacheState::Compiled(c) => Some(c),
                BytecodeCacheState::Ineligible => None,
                BytecodeCacheState::Untried => match compiler::compile_body(body.as_slice()) {
                    Ok(c) => {
                        let rc = Rc::new(c);
                        if let Some(o) = self.get_object(func_obj_id) {
                            o.borrow_mut().bytecode_cache =
                                BytecodeCacheState::Compiled(rc.clone());
                        }
                        Some(rc)
                    }
                    Err(_) => {
                        if let Some(o) = self.get_object(func_obj_id) {
                            o.borrow_mut().bytecode_cache = BytecodeCacheState::Ineligible;
                        }
                        None
                    }
                },
            };
            if let Some(chunk) = chunk {
                let prev = self.enter_ic_body(body);
                let result = vm::run_chunk(self, &chunk, exec_env, this_val.clone());
                self.leave_ic_body(prev);
                return result;
            }
        }
        // #72: the declared-name collection for this Body is memoised, bounded
        // per #165.
        let analysis = self.hoist_cache.analysis_for(body);
        let prev = self.enter_ic_body(body);
        let result = self.exec_statements_cached(body.as_slice(), exec_env, Some(&analysis));
        self.leave_ic_body(prev);
        result
    }

    pub(super) fn eval_binary(
        &mut self,
        op: BinaryOp,
        left: &JsValue,
        right: &JsValue,
    ) -> Completion {
        // Fast path: both operands are Number — skip ToPrimitive/ToNumeric/BigInt checks
        if let (Some(ln), Some(rn)) = (left.as_number(), right.as_number()) {
            return match op {
                BinaryOp::Add => Completion::Normal(JsValue::number(number_ops::add(ln, rn))),
                BinaryOp::Sub => Completion::Normal(JsValue::number(number_ops::subtract(ln, rn))),
                BinaryOp::Mul => Completion::Normal(JsValue::number(number_ops::multiply(ln, rn))),
                BinaryOp::Div => Completion::Normal(JsValue::number(number_ops::divide(ln, rn))),
                BinaryOp::Mod => Completion::Normal(JsValue::number(number_ops::remainder(ln, rn))),
                BinaryOp::Exp => {
                    Completion::Normal(JsValue::number(number_ops::exponentiate(ln, rn)))
                }
                BinaryOp::LShift => {
                    Completion::Normal(JsValue::number(number_ops::left_shift(ln, rn)))
                }
                BinaryOp::RShift => {
                    Completion::Normal(JsValue::number(number_ops::signed_right_shift(ln, rn)))
                }
                BinaryOp::URShift => {
                    Completion::Normal(JsValue::number(number_ops::unsigned_right_shift(ln, rn)))
                }
                BinaryOp::BitAnd => {
                    Completion::Normal(JsValue::number(number_ops::bitwise_and(ln, rn)))
                }
                BinaryOp::BitOr => {
                    Completion::Normal(JsValue::number(number_ops::bitwise_or(ln, rn)))
                }
                BinaryOp::BitXor => {
                    Completion::Normal(JsValue::number(number_ops::bitwise_xor(ln, rn)))
                }
                BinaryOp::Lt => Completion::Normal(JsValue::boolean(
                    number_ops::less_than(ln, rn) == Some(true),
                )),
                BinaryOp::Gt => Completion::Normal(JsValue::boolean(
                    number_ops::less_than(rn, ln) == Some(true),
                )),
                BinaryOp::LtEq => Completion::Normal(JsValue::boolean(
                    number_ops::less_than(rn, ln) == Some(false),
                )),
                BinaryOp::GtEq => Completion::Normal(JsValue::boolean(
                    number_ops::less_than(ln, rn) == Some(false),
                )),
                BinaryOp::Eq | BinaryOp::StrictEq => {
                    Completion::Normal(JsValue::boolean(number_ops::equal(ln, rn)))
                }
                BinaryOp::NotEq | BinaryOp::StrictNotEq => {
                    Completion::Normal(JsValue::boolean(!number_ops::equal(ln, rn)))
                }
                // In, Instanceof — fall through to general path
                _ => self.eval_binary_slow(op, left, right),
            };
        }
        self.eval_binary_slow(op, left, right)
    }

    fn eval_binary_slow(&mut self, op: BinaryOp, left: &JsValue, right: &JsValue) -> Completion {
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
                if (lprim).is_symbol() || (rprim).is_symbol() {
                    return Completion::Throw(
                        self.create_type_error("Cannot convert a Symbol value to a string"),
                    );
                }
                if is_string(&lprim) || is_string(&rprim) {
                    let mut code_units = if lprim.is_string() {
                        lprim.into_string().expect("kind checked").into_vec()
                    } else {
                        js_value_to_code_units(&lprim)
                    };
                    if rprim
                        .with_string(|s| code_units.extend_from_slice(s))
                        .is_none()
                    {
                        code_units.extend(js_value_to_code_units(&rprim));
                    }
                    Completion::Normal(JsValue::string(JsString::from_vec(code_units)))
                } else if let (Some(a), Some(b)) = (lprim.as_bigint(), rprim.as_bigint()) {
                    Completion::Normal(JsValue::bigint(JsBigInt::new(bigint_ops::add(
                        &a.value, &b.value,
                    ))))
                } else if lprim.is_bigint() || rprim.is_bigint() {
                    Completion::Throw(self.create_type_error(
                        "Cannot mix BigInt and other types, use explicit conversions",
                    ))
                } else {
                    Completion::Normal(JsValue::number(number_ops::add(
                        to_number(&lprim),
                        to_number(&rprim),
                    )))
                }
            }
            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod | BinaryOp::Exp => {
                // §13.7/13.8: ToNumeric(lval) before ToNumeric(rval)
                let lnum = match self.to_numeric(left) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let rnum = match self.to_numeric(right) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                if let (Some(a), Some(b)) = (lnum.as_bigint(), rnum.as_bigint()) {
                    match op {
                        BinaryOp::Sub => Completion::Normal(JsValue::bigint(JsBigInt::new(
                            bigint_ops::subtract(&a.value, &b.value),
                        ))),
                        BinaryOp::Mul => Completion::Normal(JsValue::bigint(JsBigInt::new(
                            bigint_ops::multiply(&a.value, &b.value),
                        ))),
                        BinaryOp::Div => match bigint_ops::divide(&a.value, &b.value) {
                            Ok(v) => Completion::Normal(JsValue::bigint(JsBigInt::new(v))),
                            Err(_) => Completion::Throw(
                                self.create_error("RangeError", "Division by zero"),
                            ),
                        },
                        BinaryOp::Mod => match bigint_ops::remainder(&a.value, &b.value) {
                            Ok(v) => Completion::Normal(JsValue::bigint(JsBigInt::new(v))),
                            Err(_) => Completion::Throw(
                                self.create_error("RangeError", "Division by zero"),
                            ),
                        },
                        BinaryOp::Exp => match bigint_ops::exponentiate(&a.value, &b.value) {
                            Ok(v) => Completion::Normal(JsValue::bigint(JsBigInt::new(v))),
                            Err(_) => Completion::Throw(
                                self.create_error("RangeError", "Exponent must be positive"),
                            ),
                        },
                        _ => unreachable!(),
                    }
                } else if lnum.is_bigint() || rnum.is_bigint() {
                    Completion::Throw(self.create_type_error(
                        "Cannot mix BigInt and other types, use explicit conversions",
                    ))
                } else {
                    let ln = if let Some(n) = lnum.as_number() {
                        n
                    } else {
                        to_number(&lnum)
                    };
                    let rn = if let Some(n) = rnum.as_number() {
                        n
                    } else {
                        to_number(&rnum)
                    };
                    Completion::Normal(JsValue::number(match op {
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
                Ok(b) => Completion::Normal(JsValue::boolean(b)),
                Err(e) => Completion::Throw(e),
            },
            BinaryOp::NotEq => match self.abstract_equality(left, right) {
                Ok(b) => Completion::Normal(JsValue::boolean(!b)),
                Err(e) => Completion::Throw(e),
            },
            BinaryOp::StrictEq => {
                Completion::Normal(JsValue::boolean(strict_equality(left, right)))
            }
            BinaryOp::StrictNotEq => {
                Completion::Normal(JsValue::boolean(!strict_equality(left, right)))
            }
            BinaryOp::Lt => match self.abstract_relational(left, right) {
                Ok(r) => Completion::Normal(JsValue::boolean(r == Some(true))),
                Err(e) => Completion::Throw(e),
            },
            BinaryOp::Gt => match self.abstract_relational_inner(right, left, false) {
                Ok(r) => Completion::Normal(JsValue::boolean(r == Some(true))),
                Err(e) => Completion::Throw(e),
            },
            BinaryOp::LtEq => match self.abstract_relational_inner(right, left, false) {
                Ok(r) => Completion::Normal(JsValue::boolean(r == Some(false))),
                Err(e) => Completion::Throw(e),
            },
            BinaryOp::GtEq => match self.abstract_relational(left, right) {
                Ok(r) => Completion::Normal(JsValue::boolean(r == Some(false))),
                Err(e) => Completion::Throw(e),
            },
            BinaryOp::LShift | BinaryOp::RShift | BinaryOp::URShift => {
                let lnum = match self.to_numeric(left) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let rnum = match self.to_numeric(right) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                if lnum.is_bigint() || rnum.is_bigint() {
                    if op == BinaryOp::URShift {
                        return Completion::Throw(self.create_type_error(
                            "Cannot mix BigInt and other types, use explicit conversions",
                        ));
                    }
                    if let (Some(a), Some(b)) = (lnum.as_bigint(), rnum.as_bigint()) {
                        Completion::Normal(JsValue::bigint(JsBigInt::new(match op {
                            BinaryOp::LShift => bigint_ops::left_shift(&a.value, &b.value),
                            BinaryOp::RShift => bigint_ops::signed_right_shift(&a.value, &b.value),
                            _ => unreachable!(),
                        })))
                    } else {
                        Completion::Throw(self.create_type_error(
                            "Cannot mix BigInt and other types, use explicit conversions",
                        ))
                    }
                } else {
                    let ln = if let Some(n) = lnum.as_number() {
                        n
                    } else {
                        to_number(&lnum)
                    };
                    let rn = if let Some(n) = rnum.as_number() {
                        n
                    } else {
                        to_number(&rnum)
                    };
                    Completion::Normal(JsValue::number(match op {
                        BinaryOp::LShift => number_ops::left_shift(ln, rn),
                        BinaryOp::RShift => number_ops::signed_right_shift(ln, rn),
                        BinaryOp::URShift => number_ops::unsigned_right_shift(ln, rn),
                        _ => unreachable!(),
                    }))
                }
            }
            BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => {
                let lnum = match self.to_numeric(left) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let rnum = match self.to_numeric(right) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                if let (Some(a), Some(b)) = (lnum.as_bigint(), rnum.as_bigint()) {
                    Completion::Normal(JsValue::bigint(JsBigInt::new(match op {
                        BinaryOp::BitAnd => bigint_ops::bitwise_and(&a.value, &b.value),
                        BinaryOp::BitOr => bigint_ops::bitwise_or(&a.value, &b.value),
                        BinaryOp::BitXor => bigint_ops::bitwise_xor(&a.value, &b.value),
                        _ => unreachable!(),
                    })))
                } else if lnum.is_bigint() || rnum.is_bigint() {
                    Completion::Throw(self.create_type_error(
                        "Cannot mix BigInt and other types, use explicit conversions",
                    ))
                } else {
                    let ln = if let Some(n) = lnum.as_number() {
                        n
                    } else {
                        to_number(&lnum)
                    };
                    let rn = if let Some(n) = rnum.as_number() {
                        n
                    } else {
                        to_number(&rnum)
                    };
                    Completion::Normal(JsValue::number(match op {
                        BinaryOp::BitAnd => number_ops::bitwise_and(ln, rn),
                        BinaryOp::BitOr => number_ops::bitwise_or(ln, rn),
                        BinaryOp::BitXor => number_ops::bitwise_xor(ln, rn),
                        _ => unreachable!(),
                    }))
                }
            }
            BinaryOp::In => {
                if let Some(o) = right.as_object_id().map(|id| crate::types::JsObject { id }) {
                    let key = match self.to_property_key(left) {
                        Ok(k) => k,
                        Err(e) => return Completion::Throw(e),
                    };
                    match self.proxy_has_property(o.id, &key) {
                        Ok(result) => Completion::Normal(JsValue::boolean(result)),
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
        let saved_tail = self.in_tail_position;
        self.in_tail_position = false;
        let lval = match self.eval_expr(left, env) {
            Completion::Normal(v) => v,
            other => {
                self.in_tail_position = saved_tail;
                return other;
            }
        };
        self.in_tail_position = saved_tail;
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
        let numeric = if (raw_val).is_object() {
            self.to_primitive(raw_val, "number")?
        } else {
            raw_val.clone()
        };
        if let Some(b) = (numeric).as_bigint() {
            use num_bigint::BigInt;
            let one = BigInt::from(1);
            let new_bigint = match op {
                UpdateOp::Increment => &*b.value + &one,
                UpdateOp::Decrement => &*b.value - &one,
            };
            let old_val = JsValue::bigint(b.clone());
            let new_val = JsValue::bigint(JsBigInt::new(new_bigint));
            Ok((old_val, new_val))
        } else if (numeric).as_symbol().is_some() {
            Err(self.create_type_error("Cannot convert a Symbol value to a number"))
        } else {
            let old_num = to_number(&numeric);
            let new_num = match op {
                UpdateOp::Increment => old_num + 1.0,
                UpdateOp::Decrement => old_num - 1.0,
            };
            Ok((JsValue::number(old_num), JsValue::number(new_num)))
        }
    }

    pub(super) fn get_identifier_value_by_ref(
        &mut self,
        name: &str,
        id_ref: &IdentifierRef,
        env: &EnvRef,
    ) -> Completion {
        let strict = env.borrow().strict;
        match id_ref {
            IdentifierRef::WithObject(obj_id) => self.with_get_binding_value(*obj_id, name, strict),
            IdentifierRef::Unresolvable => {
                Completion::Throw(self.create_reference_error(&format!("{name} is not defined")))
            }
            IdentifierRef::SpecificEnv(specific_env) => {
                let (indirect, has_binding) = {
                    let specific = specific_env.borrow();
                    (
                        specific.resolve_indirect_binding(name),
                        specific.bindings.contains_key(name),
                    )
                };
                if let Some(resolved) = indirect {
                    match resolved {
                        Some(value) => Completion::Normal(value),
                        None => Completion::Throw(self.create_reference_error(&format!(
                            "Cannot access '{name}' before initialization"
                        ))),
                    }
                } else if has_binding {
                    match self.env_get(specific_env, name) {
                        Some(value) => Completion::Normal(value),
                        None => Completion::Throw(self.create_reference_error(&format!(
                            "Cannot access '{name}' before initialization"
                        ))),
                    }
                } else if let Some(result) = self.resolve_global_getter(name, specific_env) {
                    result
                } else {
                    Completion::Throw(
                        self.create_reference_error(&format!("{name} is not defined")),
                    )
                }
            }
        }
    }

    pub(super) fn eval_identifier_update(
        &mut self,
        op: UpdateOp,
        prefix: bool,
        name: &str,
        env: &EnvRef,
    ) -> Completion {
        // Fast path: identifier is a Number in the local scope chain (no with/module)
        {
            let env_borrow = env.borrow();
            if env_borrow.with_object.is_none()
                && let Some(binding) = env_borrow.bindings.get(name)
                && binding.initialized
                && binding.kind != BindingKind::Const
                && binding.kind != BindingKind::FunctionName
                && binding.kind != BindingKind::ImmutableValue
                && let Some(n) = binding.value.as_number()
            {
                let old_num = n;
                let new_num = match op {
                    UpdateOp::Increment => old_num + 1.0,
                    UpdateOp::Decrement => old_num - 1.0,
                };
                let new_val = JsValue::number(new_num);
                drop(env_borrow);
                if let Err(e) = self.env_set(env, name, new_val) {
                    return Completion::Throw(e);
                }
                return Completion::Normal(JsValue::number(if prefix { new_num } else { old_num }));
            }
        }

        let id_ref = match self.resolve_identifier_ref(name, env) {
            Ok(r) => r,
            Err(e) => return Completion::Throw(e),
        };
        let raw_val = match self.get_identifier_value_by_ref(name, &id_ref, env) {
            Completion::Normal(v) => v,
            other => return other,
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
    }

    fn eval_update(
        &mut self,
        op: UpdateOp,
        prefix: bool,
        arg: &Expression,
        env: &EnvRef,
    ) -> Completion {
        if let Expression::Identifier(name) = arg {
            self.eval_identifier_update(op, prefix, name, env)
        } else if let Expression::Member(obj_expr, prop, _) = arg {
            // §13.3.7.1: super[expr]++ — special evaluation order
            if matches!(obj_expr.as_ref(), Expression::Super)
                && !matches!(prop, MemberProperty::Private(_))
            {
                // Step 2: GetThisBinding — throw if uninitialized
                if Self::this_is_in_tdz(env) {
                    return Completion::Throw(self.create_reference_error(
                        "Must call super constructor in derived class before accessing 'this' or returning from derived constructor",
                    ));
                }
                let this_val = env.borrow().get("this").unwrap_or(JsValue::UNDEFINED);

                // Evaluate key expression (raw)
                let raw_key = match prop {
                    MemberProperty::Dot(name) => {
                        JsValue::string(crate::types::JsString::from_str(name))
                    }
                    MemberProperty::Computed(expr) => match self.eval_expr(expr, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    },
                    MemberProperty::Private(_) => unreachable!(),
                };

                // GetSuperBase — capture BEFORE ToPropertyKey
                let super_base_id = match self.get_super_base_id(env) {
                    Some(id) => id,
                    None => {
                        return Completion::Throw(
                            self.create_type_error("Cannot read properties of null"),
                        );
                    }
                };

                // ToPropertyKey
                let key = match self.to_property_key(&raw_key) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };

                // GetValue from super base
                let cur_val = match self.get_object_property(super_base_id, &key, &this_val) {
                    Completion::Normal(v) => v,
                    other => return other,
                };

                let (old_val, new_val) = match self.apply_update_numeric(&cur_val, op) {
                    Ok(pair) => pair,
                    Err(e) => return Completion::Throw(e),
                };

                // PutValue on super base
                let strict = env.borrow().strict;
                match self.super_set_property(
                    super_base_id,
                    &key,
                    new_val.clone(),
                    &this_val,
                    strict,
                ) {
                    Completion::Normal(_) => {}
                    other => return other,
                }
                return Completion::Normal(if prefix { new_val } else { old_val });
            }

            let obj_val = match self.eval_expr(obj_expr, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            if let MemberProperty::Private(name) = prop {
                // §13.4 UpdateExpression on a private reference desugars to
                // PrivateGet -> ToNumeric -> PrivateSet, so accessor-backed
                // privates read through their getter and write through their
                // setter just like a data field does. The receiver has to stay
                // rooted across all three steps: ToNumeric runs a user
                // `valueOf` that can reach a GC safepoint while `obj_val`
                // exists only as a Rust local, invisible to the collector.
                let gc_frame = self.gc_root_frame();
                self.gc_root_value(&obj_val);
                let result = (|| {
                    let old = match self.private_get(&obj_val, name, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let (old_val, new_val) = match self.apply_update_numeric(&old, op) {
                        Ok(pair) => pair,
                        Err(e) => return Completion::Throw(e),
                    };
                    if let Err(e) = self.set_private_field(&obj_val, name, new_val.clone(), env) {
                        return Completion::Throw(e);
                    }
                    Completion::Normal(if prefix { new_val } else { old_val })
                })();
                self.gc_unroot_frame(gc_frame);
                return result;
            }
            let key = match prop {
                MemberProperty::Dot(name) => JsPropertyKey::from(name.clone()),
                MemberProperty::Computed(expr) => {
                    let v = match self.eval_expr(expr, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    // ToObject(base) must precede ToPropertyKey(prop) per spec
                    if obj_val.is_nullish() {
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
            let cur_val = match obj_val.as_object_id() {
                Some(id) => match self.get_object_property(id, &key, &obj_val) {
                    Completion::Normal(v) => v,
                    other => return other,
                },
                None => {
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
            // Set value back (uses set_object_with_key to handle accessor setters, proxies, etc.)
            let strict = env.borrow().strict;
            if let Err(e) = self.set_object_with_key(obj_val, &key, new_val.clone(), strict) {
                return Completion::Throw(e);
            }
            Completion::Normal(if prefix { new_val } else { old_val })
        } else if let Expression::Call(_, _, _) = arg {
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
            Completion::Normal(JsValue::number(f64::NAN))
        }
    }

    pub(crate) fn assign_to_expr(
        &mut self,
        expr: &Expression,
        value: JsValue,
        env: &EnvRef,
    ) -> Result<(), JsValue> {
        match expr {
            Expression::Member(obj_expr, prop, _) => {
                // Handle super.prop / super[expr] assignment
                if matches!(obj_expr.as_ref(), Expression::Super) {
                    let key = match prop {
                        MemberProperty::Dot(name) => JsPropertyKey::from(name.clone()),
                        MemberProperty::Computed(cexpr) => {
                            let v = match self.eval_expr(cexpr, env) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Err(e),
                                _ => return Ok(()),
                            };
                            self.to_property_key(&v)?
                        }
                        MemberProperty::Private(_) => {
                            return Err(
                                self.create_type_error("Cannot use super with private names")
                            );
                        }
                    };
                    let super_base_id = self.get_super_base_id(env);
                    let this_val = env.borrow().get("this").unwrap_or(JsValue::UNDEFINED);
                    let strict = env.borrow().strict;
                    return match super_base_id {
                        Some(base_id) => {
                            match self.super_set_property(base_id, &key, value, &this_val, strict) {
                                Completion::Normal(_) => Ok(()),
                                Completion::Throw(e) => Err(e),
                                _ => Ok(()),
                            }
                        }
                        None => Err(self
                            .create_type_error("Cannot assign to super property: no super class")),
                    };
                }
                let obj_val = match self.eval_expr(obj_expr, env) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => return Ok(()),
                };
                if let MemberProperty::Private(name) = prop {
                    return self.set_private_field(&obj_val, name, value, env);
                }
                // The base Reference and the value being written must survive
                // the computed-key evaluation (arbitrary user code, plus
                // ToPropertyKey) and PutValue. Both can reach a GC safepoint
                // while these exist only as Rust locals, invisible to the
                // tracing collector.
                let gc_frame = self.gc_root_frame();
                self.gc_root_value(&obj_val);
                self.gc_root_value(&value);
                let result = (|| {
                    let key = match prop {
                        MemberProperty::Dot(name) => JsPropertyKey::from(name.clone()),
                        MemberProperty::Computed(cexpr) => {
                            let v = match self.eval_expr(cexpr, env) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Err(e),
                                _ => return Ok(()),
                            };
                            self.to_property_key(&v)?
                        }
                        MemberProperty::Private(_) => unreachable!(),
                    };
                    if obj_val.is_null() || obj_val.is_undefined() {
                        return Err(self.create_type_error(&format!(
                            "Cannot set properties of {} (setting '{key}')",
                            if obj_val.is_null() {
                                "null"
                            } else {
                                "undefined"
                            }
                        )));
                    }
                    let strict = env.borrow().strict;
                    self.set_object_with_key(obj_val, &key, value, strict)
                })();
                self.gc_unroot_frame(gc_frame);
                result
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
                    // Fast path: simple assignment to a local binding (no with, no class RHS)
                    if !matches!(right, Expression::Class(_))
                        && !right.is_anonymous_function_definition()
                    {
                        let has_mutable_local = {
                            let eb = env.borrow();
                            eb.with_object.is_none()
                                && eb.bindings.get(name).is_some_and(|b| {
                                    b.kind == BindingKind::Var || b.kind == BindingKind::Let
                                })
                        };
                        if has_mutable_local {
                            let rval = match self.eval_expr(right, env) {
                                Completion::Normal(v) => v,
                                other => return other,
                            };
                            match self.env_set(env, name, rval.clone()) {
                                Ok(()) => return Completion::Normal(rval),
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                    }

                    // Capture reference BEFORE evaluating RHS (Bug B fix)
                    let id_ref = match self.resolve_identifier_ref(name, env) {
                        Ok(r) => r,
                        Err(e) => return Completion::Throw(e),
                    };
                    let rval = if let Expression::Class(ce) = right
                        && ce.name.is_none()
                    {
                        match self.eval_class(
                            name,
                            "",
                            &ce.super_class,
                            &ce.body,
                            env,
                            ce.source_text.clone(),
                        ) {
                            Completion::Normal(v) => v,
                            other => return other,
                        }
                    } else {
                        let v = match self.eval_expr(right, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        if right.is_anonymous_function_definition() {
                            self.set_function_name(&v, name);
                        }
                        v
                    };
                    return self.put_value_by_ref(name, rval, &id_ref, env);
                }
                // Fast path: compound assignment on a local binding (no with)
                {
                    let has_mutable_local = {
                        let eb = env.borrow();
                        eb.with_object.is_none()
                            && eb.bindings.get(name).is_some_and(|b| {
                                b.kind == BindingKind::Var || b.kind == BindingKind::Let
                            })
                    };
                    if has_mutable_local {
                        let lval = match self.env_get(env, name) {
                            Some(v) => v,
                            None => {
                                return Completion::Throw(
                                    self.create_reference_error(&format!("{name} is not defined")),
                                );
                            }
                        };
                        let rval = match self.eval_expr(right, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let final_val = match self.apply_compound_assign(op, lval, rval) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        match self.env_set(env, name, final_val.clone()) {
                            Ok(()) => return Completion::Normal(final_val),
                            Err(e) => return Completion::Throw(e),
                        }
                    }
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
                    IdentifierRef::Unresolvable => {
                        return Completion::Throw(
                            self.create_reference_error(&format!("{name} is not defined")),
                        );
                    }
                    IdentifierRef::SpecificEnv(_) => {
                        if let Some(result) = self.resolve_global_getter(name, env) {
                            match result {
                                Completion::Normal(v) => v,
                                other => return other,
                            }
                        } else {
                            match self.env_get(env, name) {
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
                let final_val = match self.apply_compound_assign(op, lval, rval) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                self.put_value_by_ref(name, final_val, &id_ref, env)
            }
            Expression::Member(obj_expr, prop, _) => {
                // §13.3.7.1: super[expr] = val — special evaluation order
                if matches!(obj_expr.as_ref(), Expression::Super)
                    && !matches!(prop, MemberProperty::Private(_))
                {
                    // Step 2: GetThisBinding — throw if uninitialized (before key eval)
                    if Self::this_is_in_tdz(env) {
                        return Completion::Throw(self.create_reference_error(
                            "Must call super constructor in derived class before accessing 'this' or returning from derived constructor",
                        ));
                    }
                    let this_val = env.borrow().get("this").unwrap_or(JsValue::UNDEFINED);

                    // Evaluate key expression (raw, no ToPropertyKey yet)
                    let raw_key = match prop {
                        MemberProperty::Dot(name) => {
                            JsValue::string(crate::types::JsString::from_str(name))
                        }
                        MemberProperty::Computed(expr) => match self.eval_expr(expr, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        },
                        MemberProperty::Private(_) => unreachable!(),
                    };

                    // §13.3.7.3 step 3: GetSuperBase — capture BEFORE ToPropertyKey
                    let super_base_id = self.get_super_base_id(env);
                    let strict = env.borrow().strict;

                    if op == AssignOp::Assign {
                        // Simple: eval RHS first, then ToPropertyKey in PutValue
                        let rval = match self.eval_expr(right, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let key = match self.to_property_key(&raw_key) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        };
                        return match super_base_id {
                            Some(base_id) => {
                                self.super_set_property(base_id, &key, rval, &this_val, strict)
                            }
                            None => Completion::Throw(self.create_type_error(&format!(
                                "Cannot set properties of null (setting '{key}')"
                            ))),
                        };
                    } else {
                        // Compound: ToPropertyKey + GetValue first, then RHS, apply, PutValue
                        let key = match self.to_property_key(&raw_key) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        };
                        let base_id = match super_base_id {
                            Some(id) => id,
                            None => {
                                return Completion::Throw(self.create_type_error(&format!(
                                    "Cannot read properties of null (reading '{key}')"
                                )));
                            }
                        };
                        let lval = match self.get_object_property(base_id, &key, &this_val) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let rval = match self.eval_expr(right, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let final_val = match self.apply_compound_assign(op, lval, rval) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        return self
                            .super_set_property(base_id, &key, final_val, &this_val, strict);
                    }
                }

                let obj_val = match self.eval_expr(obj_expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                // The base Reference must survive computed-key/RHS evaluation
                // and PutValue. Those operations can hit GC safepoints while
                // `obj_val` otherwise exists only as a Rust local, invisible to
                // the tracing collector.
                let gc_frame = self.gc_root_frame();
                self.gc_root_value(&obj_val);
                let result = (|| {
                    if let MemberProperty::Private(name) = prop {
                        // The RHS is evaluated first (preserving jsse's existing
                        // evaluation order). A plain `= ` performs PrivateSet with
                        // no preceding PrivateGet; every compound operator desugars
                        // to PrivateGet -> op -> PrivateSet, so accessor-backed
                        // privates read through the getter and write through the
                        // setter exactly as a data field does.
                        let rval = match self.eval_expr(right, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        if op == AssignOp::Assign {
                            if let Err(e) =
                                self.set_private_field(&obj_val, name, rval.clone(), env)
                            {
                                return Completion::Throw(e);
                            }
                            return Completion::Normal(rval);
                        }
                        let lval = match self.private_get(&obj_val, name, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let final_val = match self.apply_compound_assign(op, lval, rval) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        if let Err(e) =
                            self.set_private_field(&obj_val, name, final_val.clone(), env)
                        {
                            return Completion::Throw(e);
                        }
                        return Completion::Normal(final_val);
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
                    let (key, lval_for_compound) = if op != AssignOp::Assign {
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
                            MemberProperty::Dot(name) => JsPropertyKey::from(name.clone()),
                            MemberProperty::Computed(_) => {
                                match self.to_property_key(key_val.as_ref().unwrap()) {
                                    Ok(s) => s,
                                    Err(e) => return Completion::Throw(e),
                                }
                            }
                            MemberProperty::Private(_) => unreachable!(),
                        };
                        let lval = if let Some(o) = (obj_val)
                            .as_object_id()
                            .map(|id| crate::types::JsObject { id })
                        {
                            match self.get_object_property(o.id, &key, &obj_val) {
                                Completion::Normal(v) => v,
                                other => return other,
                            }
                        } else {
                            match self.to_object(&obj_val) {
                                Completion::Normal(wrapped) => {
                                    if let Some(o) = (wrapped)
                                        .as_object_id()
                                        .map(|id| crate::types::JsObject { id })
                                    {
                                        match self.get_object_property(o.id, &key, &obj_val) {
                                            Completion::Normal(v) => v,
                                            other => return other,
                                        }
                                    } else {
                                        JsValue::UNDEFINED
                                    }
                                }
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => JsValue::UNDEFINED,
                            }
                        };
                        (key, Some(lval))
                    } else {
                        (JsPropertyKey::from_str(""), None) // key computed after RHS for simple assign
                    };
                    // Now evaluate RHS
                    let rval = match self.eval_expr(right, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    // For simple assign, compute key now
                    let key = if op == AssignOp::Assign {
                        match prop {
                            MemberProperty::Dot(name) => JsPropertyKey::from(name.clone()),
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
                    // Note: super[key] = val is handled by the early return above
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
                    let final_val = if op == AssignOp::Assign {
                        rval
                    } else {
                        match self.apply_compound_assign(op, lval_for_compound.unwrap(), rval) {
                            Completion::Normal(v) => v,
                            other => return other,
                        }
                    };
                    // Fast path for dense ordinary Array indexed writes: in-bounds
                    // overwrite or append-at-end directly into the backing Vec,
                    // bypassing the setter/prototype/Set machinery. Bails to the
                    // slow path on holes, length mismatch, frozen/sealed/non-
                    // -writable-length, non-extensible, proxies, or shadowed
                    // indices so strict-throw and exotic behaviour stay correct.
                    //
                    // NB: in this engine `JsValue::UNDEFINED` in `array_elements`
                    // (with no `properties` entry) is the HOLE sentinel (see
                    // `has_own_property`/`get_own_property_full`). The fast path
                    // therefore must not (a) treat an `Undefined` slot as a live
                    // element to overwrite, nor (b) store `undefined` as a present
                    // element without the `properties` presence marker the slow
                    // path adds — doing either materialises/leaves a hole with the
                    // wrong own-property state. Both cases bail to the slow path.
                    //
                    // The key and value tests gate the arena lookup: both are
                    // pure and allocation-free, so a non-index write never pays
                    // for an object handle it would immediately drop.
                    if !(final_val).is_undefined()
                        && let Some(idx_u32) = parse_array_index(&key)
                        && let Some(obj_id) = obj_val.as_object_id()
                        && let Some(obj) = self.get_object(obj_id)
                    {
                        let fast = {
                            let b = obj.borrow();
                            if b.class_name == "Array"
                                && !b.is_proxy()
                                && b.array_elements().is_some()
                                && !b.properties.contains_key(&key)
                            {
                                let elems = b.array_elements().unwrap();
                                let elems_len = elems.len();
                                let idx = idx_u32 as usize;
                                // Overwrite is only sound on a genuinely live slot
                                // (not the hole sentinel).
                                let slot_is_hole = idx < elems_len && (elems[idx]).is_undefined();
                                let len_desc = b.properties.get("length");
                                let cur_len = len_desc
                                    .and_then(|d| d.value.as_ref())
                                    .and_then(|v| (v).as_number().map(|n| n as u32))
                                    .unwrap_or(0);
                                let length_writable =
                                    len_desc.map(|d| d.writable != Some(false)).unwrap_or(false);
                                Some((
                                    elems_len,
                                    cur_len,
                                    length_writable,
                                    b.extensible,
                                    slot_is_hole,
                                    b.prototype_id,
                                ))
                            } else {
                                None
                            }
                        };
                        if let Some((
                            elems_len,
                            cur_len,
                            length_writable,
                            extensible,
                            slot_is_hole,
                            proto_id,
                        )) = fast
                        {
                            let idx = idx_u32 as usize;
                            if idx < elems_len && !slot_is_hole {
                                // In-bounds overwrite of a live slot: no length /
                                // extensible / shape / prototype involvement.
                                obj.borrow_mut().array_elements_mut().unwrap()[idx] =
                                    final_val.clone();
                                return Completion::Normal(final_val);
                            } else if idx == elems_len
                            && idx_u32 >= cur_len
                            && cur_len as usize == elems_len
                            && extensible
                            && length_writable
                            && idx_u32 < 0xFFFF_FFFF
                            // OrdinarySet walks the prototype chain when the
                            // receiver has no own property for this index.
                            // If the index ToString is inherited (setter or
                            // data), or a Proxy sits anywhere in the chain,
                            // we must honour the proto via the slow path's
                            // OrdinarySet/proxy_set — a bare Proxy exposes no
                            // own descriptor here, so check it explicitly.
                            && !proto_id.is_some_and(|pid| {
                                self.has_proxy_in_prototype_chain(pid)
                                    || self.get_property_descriptor_on_id(pid, &key).is_some()
                            }) {
                                // Append at end: push and bump length + shape.
                                let mut b = obj.borrow_mut();
                                b.array_elements_mut().unwrap().push(final_val.clone());
                                if let Some(len_desc) = b.properties.get_mut("length") {
                                    len_desc.value = Some(JsValue::number((idx_u32 + 1) as f64));
                                }
                                b.shape_id = crate::interpreter::types::fresh_shape_id();
                                return Completion::Normal(final_val);
                            }
                            // Otherwise (hole creation/overwrite, length mismatch,
                            // frozen/sealed/non-writable-length, non-extensible,
                            // inherited index): fall through to the slow path.
                        }
                    }
                    let strict = env.borrow().strict;
                    let set_outcome = match self.set_object_with_key_result(
                        obj_val.clone(),
                        &key,
                        final_val.clone(),
                        false,
                    ) {
                        Ok(succeeded) => succeeded,
                        Err(e) => return Completion::Throw(e),
                    };
                    if !set_outcome.succeeded() && strict {
                        return Completion::Throw(self.member_assignment_error(&obj_val, &key));
                    }
                    // The realm test comes before `as_str`, which validates the
                    // whole key as UTF-8: it is both cheaper and false for
                    // essentially every write in a real program.
                    if let Some(obj_id) = obj_val.as_object_id()
                        && set_outcome.wrote_own_data_property_on(obj_id)
                        && self.is_realm_global_object(obj_id)
                        && let Some(key_str) = key.as_str()
                    {
                        self.sync_global_object_binding(obj_id, key_str, &final_val);
                    }
                    Completion::Normal(final_val)
                })();
                self.gc_unroot_frame(gc_frame);
                result
            }
            Expression::Array(elements, _) if op == AssignOp::Assign => {
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
            Expression::Call(_, _, _) => {
                match self.eval_expr(left, env) {
                    Completion::Normal(_) => {}
                    other => return other,
                }
                Completion::Throw(
                    self.create_reference_error("Invalid left-hand side in assignment"),
                )
            }
            // Parenthesized identifier: (x) = expr
            // IsIdentifierRef is false, so no named evaluation
            Expression::Sequence(exprs) if exprs.len() == 1 => {
                if let Expression::Identifier(name) = &exprs[0] {
                    let id_ref = match self.resolve_identifier_ref(name, env) {
                        Ok(r) => r,
                        Err(e) => return Completion::Throw(e),
                    };
                    let rval = match self.eval_expr(right, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    self.put_value_by_ref(name, rval, &id_ref, env)
                } else {
                    // Parenthesized member expression or other — delegate
                    self.eval_assign(op, &exprs[0], right, env)
                }
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
                    IdentifierRef::Unresolvable => {
                        return Completion::Throw(
                            self.create_reference_error(&format!("{name} is not defined")),
                        );
                    }
                    IdentifierRef::SpecificEnv(_) => {
                        if let Some(result) = self.resolve_global_getter(name, env) {
                            match result {
                                Completion::Normal(v) => v,
                                other => return other,
                            }
                        } else {
                            match self.env_get(env, name) {
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
            Expression::Member(obj_expr, MemberProperty::Private(name), _) => {
                let obj_val = match self.eval_expr(obj_expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                // The receiver has to survive PrivateGet (which may call a
                // user getter) and the right-hand side evaluation before
                // PrivateSet writes through the setter. Both steps run user
                // code that can reach a GC safepoint while `obj_val` exists
                // only as a Rust local, invisible to the collector.
                let gc_frame = self.gc_root_frame();
                self.gc_root_value(&obj_val);
                let result = (|| {
                    let lval = match self.private_get(&obj_val, name, env) {
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
                    let rval = match self.eval_expr(right, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if let Err(e) = self.set_private_field(&obj_val, name, rval.clone(), env) {
                        return Completion::Throw(e);
                    }
                    Completion::Normal(rval)
                })();
                self.gc_unroot_frame(gc_frame);
                result
            }
            Expression::Member(obj_expr, prop, _) => {
                // Super property logical assignment: super.p &&= / ||= / ??=
                if matches!(obj_expr.as_ref(), Expression::Super)
                    && !matches!(prop, MemberProperty::Private(_))
                {
                    // §13.3.7.3 step 2: GetThisEnvironment().GetThisBinding()
                    // runs before the key expression and throws for an
                    // uninitialized `this` in a derived constructor.
                    if Self::this_is_in_tdz(env) {
                        return Completion::Throw(self.create_reference_error(
                            "Must call super constructor in derived class before accessing 'this' or returning from derived constructor",
                        ));
                    }
                    let this_val = env.borrow().get("this").unwrap_or(JsValue::UNDEFINED);
                    let raw_key = match prop {
                        MemberProperty::Dot(name) => {
                            JsValue::string(crate::types::JsString::from_str(name))
                        }
                        MemberProperty::Computed(expr) => match self.eval_expr(expr, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        },
                        MemberProperty::Private(_) => unreachable!(),
                    };
                    let super_base_id = self.get_super_base_id(env);
                    let strict = env.borrow().strict;
                    let key = match self.to_property_key(&raw_key) {
                        Ok(s) => s,
                        Err(e) => return Completion::Throw(e),
                    };
                    let base_id = match super_base_id {
                        Some(id) => id,
                        None => {
                            return Completion::Throw(self.create_type_error(&format!(
                                "Cannot read properties of null (reading '{key}')"
                            )));
                        }
                    };
                    let lval = match self.get_object_property(base_id, &key, &this_val) {
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
                    let rval = match self.eval_expr(right, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    return self.super_set_property(base_id, &key, rval, &this_val, strict);
                }

                let obj_val = match self.eval_expr(obj_expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                // The base Reference — and, for a primitive base, the ToObject
                // wrapper GetValue and PutValue share — must survive the
                // computed-key evaluation, the getter, the right-hand side, and
                // the write-back. Each of those can hit a GC safepoint while
                // these values exist only as Rust locals, invisible to the
                // tracing collector.
                let gc_frame = self.gc_root_frame();
                self.gc_root_value(&obj_val);
                let result = (|| {
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
                    let (boxed_obj, key) = if obj_val.is_object() {
                        let key = match prop {
                            MemberProperty::Dot(name) => JsPropertyKey::from(name.clone()),
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
                            _ => return Completion::Normal(JsValue::UNDEFINED),
                        };
                        self.gc_root_value(&boxed);
                        let key = match prop {
                            MemberProperty::Dot(name) => JsPropertyKey::from(name.clone()),
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
                    let base_id = boxed_obj.as_object_id();
                    let lval = if let Some(base_id) = base_id {
                        match self.get_object_property(base_id, &key, &obj_val) {
                            Completion::Normal(v) => v,
                            other => return other,
                        }
                    } else {
                        JsValue::UNDEFINED
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
                    // §6.2.5.6 PutValue: [[Set]] runs on ToObject(base) with the
                    // original base as receiver. `boxed_obj` already is that
                    // ToObject result, so reuse it instead of boxing again.
                    let strict = env.borrow().strict;
                    if let Some(base_id) = base_id
                        && let Err(e) = self.put_value_to_property(
                            base_id,
                            &key,
                            rval.clone(),
                            &obj_val,
                            strict,
                        )
                    {
                        return Completion::Throw(e);
                    }
                    Completion::Normal(rval)
                })();
                self.gc_unroot_frame(gc_frame);
                result
            }
            Expression::Sequence(exprs)
                if exprs.len() == 1 && matches!(&exprs[0], Expression::Identifier(_)) =>
            {
                if let Expression::Identifier(name) = &exprs[0] {
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
                        IdentifierRef::Unresolvable => {
                            return Completion::Throw(
                                self.create_reference_error(&format!("{name} is not defined")),
                            );
                        }
                        IdentifierRef::SpecificEnv(_) => {
                            if let Some(result) = self.resolve_global_getter(name, env) {
                                match result {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                }
                            } else {
                                match self.env_get(env, name) {
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
                    // No function naming for parenthesized LHS
                    self.put_value_by_ref(name, rval, &id_ref, env)
                } else {
                    unreachable!()
                }
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

    fn apply_compound_assign(&mut self, op: AssignOp, lval: JsValue, rval: JsValue) -> Completion {
        match op {
            AssignOp::AddAssign => {
                // Fast path: both primitives and at least one is a string — avoid
                // to_primitive/js_value_to_code_units clones in eval_binary.
                if !lval.is_object() && !rval.is_object() && (lval.is_string() || rval.is_string())
                {
                    if lval.is_symbol() || rval.is_symbol() {
                        return Completion::Throw(
                            self.create_type_error("Cannot convert a Symbol value to a string"),
                        );
                    }
                    let mut code_units = if lval.is_string() {
                        lval.into_string().expect("kind checked").into_vec()
                    } else {
                        js_value_to_code_units(&lval)
                    };
                    if rval
                        .with_string(|s| code_units.extend_from_slice(s))
                        .is_none()
                    {
                        code_units.extend(js_value_to_code_units(&rval));
                    }
                    return Completion::Normal(JsValue::string(JsString::from_vec(code_units)));
                }
                self.eval_binary(BinaryOp::Add, &lval, &rval)
            }
            AssignOp::SubAssign => self.eval_binary(BinaryOp::Sub, &lval, &rval),
            AssignOp::MulAssign => self.eval_binary(BinaryOp::Mul, &lval, &rval),
            AssignOp::DivAssign => self.eval_binary(BinaryOp::Div, &lval, &rval),
            AssignOp::ModAssign => self.eval_binary(BinaryOp::Mod, &lval, &rval),
            AssignOp::ExpAssign => self.eval_binary(BinaryOp::Exp, &lval, &rval),
            AssignOp::LShiftAssign => self.eval_binary(BinaryOp::LShift, &lval, &rval),
            AssignOp::RShiftAssign => self.eval_binary(BinaryOp::RShift, &lval, &rval),
            AssignOp::URShiftAssign => self.eval_binary(BinaryOp::URShift, &lval, &rval),
            AssignOp::BitAndAssign => self.eval_binary(BinaryOp::BitAnd, &lval, &rval),
            AssignOp::BitOrAssign => self.eval_binary(BinaryOp::BitOr, &lval, &rval),
            AssignOp::BitXorAssign => self.eval_binary(BinaryOp::BitXor, &lval, &rval),
            _ => Completion::Normal(rval),
        }
    }

    /// Set a property on an already-evaluated object+key pair (strict controls TypeError on failure).
    pub(super) fn set_object_with_key<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_val: JsValue,
        key: &K,
        val: JsValue,
        strict: bool,
    ) -> Result<(), JsValue> {
        self.set_object_with_key_result(obj_val, key, val, strict)
            .map(|_| ())
    }

    fn set_object_with_key_result<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_val: JsValue,
        key: &K,
        val: JsValue,
        strict: bool,
    ) -> Result<SetOutcome, JsValue> {
        // §6.2.5.6 PutValue: [[Set]] is invoked on ToObject(base), but the
        // receiver argument stays the original base value. For a primitive
        // base that means the receiver is never an object, so OrdinarySet's
        // "Receiver is not an Object" rejection (§10.1.9.2 step 3) applies —
        // capture it before boxing, not after.
        let receiver = obj_val.clone();
        // Auto-box primitives for property access. The two bail-outs below
        // yield `true` in the sense of "no rejection to report" — [[Set]] never
        // ran, so there is nothing for a strict caller to throw about. Neither
        // is reachable today: `to_object` on a non-object always completes
        // normally with an object.
        let obj_val = if !(obj_val).is_object() {
            match self.to_object(&obj_val) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => return Ok(SetOutcome::Succeeded),
            }
        } else {
            obj_val
        };

        let Some(base_id) = obj_val.as_object_id() else {
            return Ok(SetOutcome::Succeeded);
        };
        self.put_value_to_property(base_id, key, val, &receiver, strict)
    }

    /// Property-Reference branch of §6.2.5.6 PutValue after ToObject and
    /// ToPropertyKey have identified the [[Set]] holder and property key.
    ///
    /// `receiver` is GetThisValue(reference): the original base for an
    /// ordinary member Reference and the actual `this` for a Super Reference.
    fn put_value_to_property<K: PropertyKeyLike + ?Sized>(
        &mut self,
        base_id: u64,
        key: &K,
        val: JsValue,
        receiver: &JsValue,
        strict: bool,
    ) -> Result<SetOutcome, JsValue> {
        // Delegate to the canonical [[Set]] entry point: proxy `set` trap,
        // module-namespace reject, TypedArray integer-index set, accessor
        // setters, and the OrdinarySet prototype-chain walk all live in
        // `property.rs`. It is generic over the key, so only the error path
        // below needs an owned `JsPropertyKey` — converting here would allocate
        // on every write reached with a `&str` key.
        let outcome = self.set_object_property_with_outcome(base_id, key, val, receiver)?;
        if !outcome.succeeded() && strict {
            let key = key.to_js_property_key();
            // A non-object receiver never rejects because of a read-only
            // property: OrdinarySet bails at "Receiver is not an Object", and
            // `base_id` is only the throwaway ToObject wrapper, so describing
            // its descriptors would be misleading.
            if !receiver.is_object() {
                return Err(self.non_object_receiver_error(receiver, &key));
            }
            return Err(self.read_only_assignment_error(base_id, &key));
        }
        Ok(outcome)
    }

    /// The strict-mode TypeError for a [[Set]] rejected at OrdinarySet's
    /// "Receiver is not an Object" step (§10.1.9.2 step 3): no own property can
    /// be created on a primitive, so the write simply cannot land.
    fn non_object_receiver_error(&mut self, receiver: &JsValue, key: &JsPropertyKey) -> JsValue {
        self.create_type_error(&format!("Cannot create property '{key}' on {receiver}"))
    }

    /// Builds the strict-mode TypeError for a rejected [[Set]] on `obj_id`,
    /// preserving the diagnostic distinctions this assignment path historically
    /// produced: a module-namespace target, a getter-only accessor (own or
    /// inherited), or a plain read-only data property. Descriptor re-inspection
    /// is skipped when a proxy sits on the object or its prototype chain, so no
    /// trap is re-invoked on the error path.
    fn read_only_assignment_error(&mut self, obj_id: u64, key: &JsPropertyKey) -> JsValue {
        let facts = self.set_rejection_facts(obj_id, key);
        self.read_only_assignment_error_from(facts.as_ref(), key)
    }

    /// Reads, exactly once, every fact the two rejection formatters below
    /// select a message from. Returns `None` when `obj_id` names no live
    /// object, which both formatters report as the undecorated message.
    ///
    /// `desc` is deliberately left `None` when a proxy sits on the object or
    /// its prototype chain, so no trap is re-invoked on the error path.
    fn set_rejection_facts(&mut self, obj_id: u64, key: &JsPropertyKey) -> Option<SetRejection> {
        let cell = self.get_object_cell(obj_id)?;
        let (is_module_namespace, is_proxy, has_own) = {
            let b = cell.borrow();
            (
                b.module_namespace().is_some(),
                b.is_proxy() || b.is_proxy_revoked(),
                b.has_own_property(key),
            )
        };
        let desc = if is_proxy || self.has_proxy_in_prototype_chain(obj_id) {
            None
        } else {
            self.get_property_descriptor_on_id(obj_id, key)
        };
        Some(SetRejection {
            is_module_namespace,
            has_own,
            desc,
        })
    }

    fn read_only_assignment_error_from(
        &mut self,
        facts: Option<&SetRejection>,
        key: &JsPropertyKey,
    ) -> JsValue {
        match facts {
            Some(f) if f.is_module_namespace => self.create_type_error(&format!(
                "Cannot assign to read only property '{key}' of module namespace"
            )),
            Some(f) if f.desc.as_ref().is_some_and(|d| d.is_accessor_descriptor()) => self
                .create_type_error(&format!(
                    "Cannot set property '{key}' which has only a getter"
                )),
            _ => self.create_type_error(&format!("Cannot assign to read only property '{key}'")),
        }
    }

    /// Preserve the host-compatible diagnostics historically produced by the
    /// plain member-assignment dispatcher after canonical [[Set]] rejects.
    /// The rejection itself and all observable descriptor/proxy work have
    /// already happened inside `property.rs`; this only formats the TypeError.
    fn member_assignment_error(&mut self, base: &JsValue, key: &JsPropertyKey) -> JsValue {
        let Some(obj_id) = base.as_object_id() else {
            return self.non_object_receiver_error(base, key);
        };
        let facts = self.set_rejection_facts(obj_id, key);
        if let Some(f) = &facts {
            if f.is_module_namespace {
                return self.create_type_error(&format!(
                    "Cannot assign to read only property '{key}' of object '[object Module]'"
                ));
            }
            if !f.has_own
                && f.desc
                    .as_ref()
                    .is_some_and(|d| d.is_data_descriptor() && d.writable == Some(false))
            {
                return self.create_type_error(&format!(
                    "Cannot assign to read only property '{key}' of object '#<Object>'"
                ));
            }
        }
        self.read_only_assignment_error_from(facts.as_ref(), key)
    }

    fn set_member_property(
        &mut self,
        obj_expr: &Expression,
        prop: &MemberProperty,
        val: JsValue,
        env: &EnvRef,
    ) -> Result<(), JsValue> {
        // Handle super.prop / super[expr]
        if matches!(obj_expr, Expression::Super) {
            let key = match prop {
                MemberProperty::Dot(name) => JsPropertyKey::from(name.clone()),
                MemberProperty::Computed(cexpr) => {
                    let v = match self.eval_expr(cexpr, env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Err(e),
                        _ => return Ok(()),
                    };
                    self.to_property_key(&v)?
                }
                MemberProperty::Private(_) => {
                    return Err(self.create_type_error("Cannot use super with private names"));
                }
            };
            let super_base_id = self.get_super_base_id(env);
            let this_val = env.borrow().get("this").unwrap_or(JsValue::UNDEFINED);
            let strict = env.borrow().strict;
            return match super_base_id {
                Some(base_id) => {
                    match self.super_set_property(base_id, &key, val, &this_val, strict) {
                        Completion::Normal(_) => Ok(()),
                        Completion::Throw(e) => Err(e),
                        _ => Ok(()),
                    }
                }
                None => Err(self.create_type_error("Cannot assign to super: no super class")),
            };
        }
        let obj_val = match self.eval_expr(obj_expr, env) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => return Ok(()),
        };
        self.set_member_property_with_base(obj_val, prop, val, env)
    }

    /// PrivateGet ( O, P ) — §7.3.28. Resolves the private name `name` in `env`
    /// and performs the private-element MOP: a data field or method returns its
    /// stored value; an accessor invokes its getter, or throws a TypeError when
    /// it has none; a private name the object's class never declared, and a
    /// non-object receiver, both throw a TypeError. This is the single read
    /// operation that every `o.#x` reference form routes through.
    fn private_get(&mut self, obj_val: &JsValue, name: &str, env: &EnvRef) -> Completion {
        let branded = self.resolve_private_name(name, env);
        let Some(obj_id) = obj_val.as_object_id() else {
            return Completion::Throw(self.create_type_error(&format!(
                "Cannot read private member #{name} from a non-object"
            )));
        };
        let Some(obj) = self.get_object_cell(obj_id) else {
            return Completion::Normal(JsValue::UNDEFINED);
        };
        let elem = obj.borrow().private_fields.get(&branded).cloned();
        match elem {
            Some(PrivateElement::Field(v)) | Some(PrivateElement::Method(v)) => {
                Completion::Normal(v)
            }
            Some(PrivateElement::Accessor { get, .. }) => {
                if let Some(getter) = get {
                    self.call_function(&getter, obj_val, &[])
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
    }

    fn set_private_field(
        &mut self,
        obj_val: &JsValue,
        name: &str,
        val: JsValue,
        env: &EnvRef,
    ) -> Result<(), JsValue> {
        let branded = self.resolve_private_name(name, env);
        match obj_val.as_object_id() {
            Some(id) => {
                if let Some(obj) = self.get_object_cell(id) {
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
                                let obj_val = obj_val.clone();
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
            None => Err(self.create_type_error(&format!(
                "Cannot write private member #{name} to a non-object"
            ))),
        }
    }

    fn set_member_property_with_base(
        &mut self,
        obj_val: JsValue,
        prop: &MemberProperty,
        val: JsValue,
        env: &EnvRef,
    ) -> Result<(), JsValue> {
        if let MemberProperty::Private(name) = prop {
            return self.set_private_field(&obj_val, name, val, env);
        }

        // The base Reference and the value being written must survive the
        // computed-key evaluation (arbitrary user code, plus ToPropertyKey) and
        // PutValue. Both can reach a GC safepoint while these exist only as
        // Rust locals, invisible to the tracing collector.
        let gc_frame = self.gc_root_frame();
        self.gc_root_value(&obj_val);
        self.gc_root_value(&val);
        let result = (|| {
            let key = match prop {
                MemberProperty::Dot(name) => JsPropertyKey::from(name.clone()),
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
        })();
        self.gc_unroot_frame(gc_frame);
        result
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
                Expression::Array(exprs, false)
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
                            method: false,
                        },
                        ObjectPatternProperty::Shorthand(name) => Property {
                            key: PropertyKey::Identifier(name.clone()),
                            value: Expression::Identifier(name.clone()),
                            kind: PropertyKind::Init,
                            computed: false,
                            shorthand: true,
                            method: false,
                        },
                        ObjectPatternProperty::Rest(p) => Property {
                            key: PropertyKey::Identifier("__rest__".to_string()),
                            value: Expression::Spread(Box::new(Self::pattern_to_assignment_expr(
                                p,
                            ))),
                            kind: PropertyKind::Init,
                            computed: false,
                            shorthand: false,
                            method: false,
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
                    Completion::Normal(_) => Completion::Normal(JsValue::UNDEFINED),
                    other => other,
                }
            }
            Expression::Member(obj_expr, prop, _) => {
                match self.set_member_property(obj_expr, prop, val, env) {
                    Ok(()) => Completion::Normal(JsValue::UNDEFINED),
                    Err(e) => Completion::Throw(e),
                }
            }
            Expression::Array(elements, _) => {
                self.destructure_array_assignment(elements, &val, env)
            }
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
            _ => Completion::Normal(JsValue::UNDEFINED),
        };
        if matches!(result, Completion::Yield(_)) {
            self.destructuring_yield = true;
        }
        result
    }

    /// Root every object a destructuring lRef depends on until PutValue.
    /// The base, the raw key awaiting ToPropertyKey, and a super reference's
    /// receiver are all held only by Rust locals across arbitrary user code.
    fn gc_root_destruct_lref(&mut self, lref: &DestructLRef) {
        match lref {
            DestructLRef::Member(base, raw_key) => {
                self.gc_root_value(base);
                self.gc_root_value(raw_key);
            }
            DestructLRef::Private(base, _) => self.gc_root_value(base),
            DestructLRef::Super(_, _, this_val, _) => self.gc_root_value(this_val),
        }
    }

    /// Evaluate a member expression as an lref (Reference) for destructuring.
    /// Returns base + key info or suspension explicitly; ToPropertyKey is
    /// deferred to PutValue time per spec.
    fn eval_member_lhs_ref(
        &mut self,
        target: &Expression,
        env: &EnvRef,
    ) -> Result<MemberLhsRef, JsValue> {
        let Expression::Member(obj_expr, prop, _) = target else {
            return Ok(MemberLhsRef::Ref(None));
        };

        // Handle super.prop / super[expr]
        if matches!(obj_expr.as_ref(), Expression::Super) {
            let key = match prop {
                MemberProperty::Dot(name) => JsPropertyKey::from(name.clone()),
                MemberProperty::Computed(key_expr) => {
                    let raw_key = match self.eval_expr(key_expr, env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Err(e),
                        Completion::Yield(v) => return Ok(MemberLhsRef::Suspended(v)),
                        _ => return Ok(MemberLhsRef::Ref(None)),
                    };
                    self.to_property_key(&raw_key)?
                }
                MemberProperty::Private(_) => {
                    return Err(self.create_type_error("Cannot use super with private names"));
                }
            };
            let super_base_id = self
                .get_super_base_id(env)
                .ok_or_else(|| self.create_type_error("No super class"))?;
            let this_val = env.borrow().get("this").unwrap_or(JsValue::UNDEFINED);
            let strict = env.borrow().strict;
            return Ok(MemberLhsRef::Ref(Some(DestructLRef::Super(
                super_base_id,
                key,
                this_val,
                strict,
            ))));
        }

        let base = match self.eval_expr(obj_expr, env) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            Completion::Yield(v) => return Ok(MemberLhsRef::Suspended(v)),
            _ => return Ok(MemberLhsRef::Ref(None)),
        };

        match prop {
            MemberProperty::Dot(name) => Ok(MemberLhsRef::Ref(Some(DestructLRef::Member(
                base,
                JsValue::string(JsString::from_str(name)),
            )))),
            MemberProperty::Computed(key_expr) => {
                let raw_key = match self.eval_expr(key_expr, env) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    Completion::Yield(v) => return Ok(MemberLhsRef::Suspended(v)),
                    _ => return Ok(MemberLhsRef::Ref(None)),
                };
                Ok(MemberLhsRef::Ref(Some(DestructLRef::Member(base, raw_key))))
            }
            MemberProperty::Private(name) => Ok(MemberLhsRef::Ref(Some(DestructLRef::Private(
                base,
                name.clone(),
            )))),
        }
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
        if let Some(o) = iterator
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
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
                    let precomp = match self.eval_member_lhs_ref(inner, env) {
                        Ok(MemberLhsRef::Ref(r)) => r,
                        Ok(MemberLhsRef::Suspended(v)) => {
                            yield_val = Some(v);
                            break;
                        }
                        Err(e) => {
                            error = Some(e);
                            break;
                        }
                    };

                    // Keep the lRef's base, pending key, and receiver alive
                    // through iterator collection and the eventual PutValue.
                    let gc_frame = self.gc_root_frame();
                    if let Some(lref) = &precomp {
                        self.gc_root_destruct_lref(lref);
                    }

                    let result = (|| {
                        // Collect remaining iterator values into rest array
                        let mut rest = Vec::new();
                        if !done {
                            loop {
                                match self.iterator_step(&iterator) {
                                    Ok(Some(result)) => match self.iterator_value(&result) {
                                        Ok(v) => {
                                            // Later steps run user code; the
                                            // Vec is invisible to the GC.
                                            self.gc_root_value(&v);
                                            rest.push(v);
                                        }
                                        Err(e) => {
                                            done = true;
                                            return Completion::Throw(e);
                                        }
                                    },
                                    Ok(None) => {
                                        done = true;
                                        break;
                                    }
                                    Err(e) => {
                                        done = true;
                                        return Completion::Throw(e);
                                    }
                                }
                            }
                        }

                        let arr = self.create_array(rest);
                        // ToPropertyKey and the write itself can run user code.
                        self.gc_root_value(&arr);
                        match precomp {
                            Some(DestructLRef::Member(base, raw_key)) => {
                                match self.to_property_key(&raw_key) {
                                    Ok(key) => {
                                        let strict = env.borrow().strict;
                                        if let Err(e) =
                                            self.set_object_with_key(base, &key, arr, strict)
                                        {
                                            return Completion::Throw(e);
                                        }
                                    }
                                    Err(e) => {
                                        return Completion::Throw(e);
                                    }
                                }
                            }
                            Some(DestructLRef::Private(base, ref name)) => {
                                if let Err(e) = self.set_private_field(&base, name, arr, env) {
                                    return Completion::Throw(e);
                                }
                            }
                            Some(DestructLRef::Super(base_id, ref key, ref this_val, strict)) => {
                                if let Completion::Throw(e) =
                                    self.super_set_property(base_id, key, arr, this_val, strict)
                                {
                                    return Completion::Throw(e);
                                }
                            }
                            None => match self.put_value_to_target(inner, arr, env) {
                                Completion::Normal(_) | Completion::Empty => {}
                                Completion::Throw(e) => {
                                    return Completion::Throw(e);
                                }
                                Completion::Yield(v) => {
                                    return Completion::Yield(v);
                                }
                                _ => {}
                            },
                        }
                        Completion::Empty
                    })();
                    self.gc_unroot_frame(gc_frame);

                    match result {
                        Completion::Normal(_) | Completion::Empty => {}
                        Completion::Throw(e) => error = Some(e),
                        Completion::Yield(v) => yield_val = Some(v),
                        other => return other,
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
                    let precomp = match self.eval_member_lhs_ref(target, env) {
                        Ok(MemberLhsRef::Ref(r)) => r,
                        Ok(MemberLhsRef::Suspended(v)) => {
                            yield_val = Some(v);
                            break;
                        }
                        Err(e) => {
                            error = Some(e);
                            break;
                        }
                    };

                    // The target was evaluated before iterator/default user
                    // code; its lRef must survive until PutValue.
                    let gc_frame = self.gc_root_frame();
                    if let Some(lref) = &precomp {
                        self.gc_root_destruct_lref(lref);
                    }

                    let result = (|| {
                        let item = if done {
                            JsValue::UNDEFINED
                        } else {
                            match self.iterator_step(&iterator) {
                                Ok(Some(result)) => match self.iterator_value(&result) {
                                    Ok(v) => v,
                                    Err(e) => {
                                        done = true;
                                        return Completion::Throw(e);
                                    }
                                },
                                Ok(None) => {
                                    done = true;
                                    JsValue::UNDEFINED
                                }
                                Err(e) => {
                                    done = true;
                                    return Completion::Throw(e);
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
                                    Completion::Throw(e) => return Completion::Throw(e),
                                    Completion::Yield(v) => return Completion::Yield(v),
                                    other => return other,
                                }
                            } else {
                                item
                            }
                        } else {
                            item
                        };

                        // ToPropertyKey and the write itself can run user code.
                        self.gc_root_value(&val);
                        match precomp {
                            Some(DestructLRef::Member(base, raw_key)) => {
                                match self.to_property_key(&raw_key) {
                                    Ok(key) => {
                                        let strict = env.borrow().strict;
                                        if let Err(e) =
                                            self.set_object_with_key(base, &key, val, strict)
                                        {
                                            return Completion::Throw(e);
                                        }
                                    }
                                    Err(e) => {
                                        return Completion::Throw(e);
                                    }
                                }
                            }
                            Some(DestructLRef::Private(base, ref name)) => {
                                if let Err(e) = self.set_private_field(&base, name, val, env) {
                                    return Completion::Throw(e);
                                }
                            }
                            Some(DestructLRef::Super(base_id, ref key, ref this_val, strict)) => {
                                if let Completion::Throw(e) =
                                    self.super_set_property(base_id, key, val, this_val, strict)
                                {
                                    return Completion::Throw(e);
                                }
                            }
                            None => match self.put_value_to_target(target, val, env) {
                                Completion::Normal(_) | Completion::Empty => {}
                                Completion::Throw(e) => return Completion::Throw(e),
                                Completion::Yield(v) => return Completion::Yield(v),
                                _ => {}
                            },
                        }
                        Completion::Empty
                    })();
                    self.gc_unroot_frame(gc_frame);

                    match result {
                        Completion::Normal(_) | Completion::Empty => {}
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
                }
            }
        }

        let unroot = |s: &mut Self| {
            if let Some(o) = iterator
                .as_object_id()
                .map(|id| crate::types::JsObject { id })
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
                Ok(()) => Completion::Normal(JsValue::UNDEFINED),
                Err(e) => Completion::Throw(e),
            };
        }

        unroot(self);
        if let Some(err) = error {
            return Completion::Throw(err);
        }
        Completion::Normal(JsValue::UNDEFINED)
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

        let mut excluded_keys: Vec<JsPropertyKey> = Vec::new();

        for prop in props {
            // Handle rest: {...rest} = obj
            if let Expression::Spread(inner) = &prop.value {
                let rest_obj_id = self.create_object_id();
                if let Some(o) = obj_val
                    .as_object_id()
                    .map(|id| crate::types::JsObject { id })
                {
                    let pairs = match self.copy_data_properties(o.id, &obj_val, &excluded_keys) {
                        Ok(p) => p,
                        Err(e) => return Completion::Throw(e),
                    };
                    for (k, v) in pairs {
                        self.get_object_cell_expect(rest_obj_id)
                            .borrow_mut()
                            .insert_value(k, v);
                    }
                }
                let rest_id = rest_obj_id;
                let rest_val = JsValue::object(rest_id);
                match self.put_value_to_target(inner, rest_val, env) {
                    Completion::Normal(_) | Completion::Empty => {}
                    other => return other,
                }
                continue;
            }

            match &prop.kind {
                PropertyKind::Init => {
                    let key = match &prop.key {
                        PropertyKey::Identifier(s) => JsPropertyKey::from(s.clone()),
                        PropertyKey::String(s) => {
                            JsPropertyKey::from_js_string(&JsString::from_vec(s.clone()))
                        }
                        PropertyKey::Number(n) => {
                            JsPropertyKey::from(to_js_string(&JsValue::number(*n)))
                        }
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

                    // Per spec §13.15.5.6: extract target BEFORE GetV and evaluate lref first.
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

                    // §13.15.5.6 step 1: evaluate lref (base + key expression)
                    // before GetV. ToPropertyKey is deferred to PutValue time.
                    let pre_ref = match self.eval_member_lhs_ref(target, env) {
                        Ok(MemberLhsRef::Ref(r)) => r,
                        Ok(MemberLhsRef::Suspended(v)) => return Completion::Yield(v),
                        Err(e) => return Completion::Throw(e),
                    };

                    // GetV and an initializer can run user code after the
                    // target is evaluated, so retain the whole lRef.
                    let gc_frame = self.gc_root_frame();
                    if let Some(lref) = &pre_ref {
                        self.gc_root_destruct_lref(lref);
                    }

                    let result = (|| {
                        // Get property via get_object_property (invokes getters/Proxy)
                        let val = if let Some(o) = obj_val
                            .as_object_id()
                            .map(|id| crate::types::JsObject { id })
                        {
                            match self.get_object_property(o.id, &key, &obj_val) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                Completion::Yield(v) => return Completion::Yield(v),
                                _ => JsValue::UNDEFINED,
                            }
                        } else {
                            JsValue::UNDEFINED
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

                        // ToPropertyKey and the write itself can run user code.
                        self.gc_root_value(&val);
                        if let Some(lref) = pre_ref {
                            match lref {
                                DestructLRef::Member(base_val, raw_key) => {
                                    match self.to_property_key(&raw_key) {
                                        Ok(key) => {
                                            let strict = env.borrow().strict;
                                            if let Err(e) = self
                                                .set_object_with_key(base_val, &key, val, strict)
                                            {
                                                return Completion::Throw(e);
                                            }
                                        }
                                        Err(e) => return Completion::Throw(e),
                                    }
                                }
                                DestructLRef::Private(base_val, ref name) => {
                                    if let Err(e) =
                                        self.set_private_field(&base_val, name, val, env)
                                    {
                                        return Completion::Throw(e);
                                    }
                                }
                                DestructLRef::Super(base_id, ref key, ref this_val, strict) => {
                                    if let Completion::Throw(e) =
                                        self.super_set_property(base_id, key, val, this_val, strict)
                                    {
                                        return Completion::Throw(e);
                                    }
                                }
                            }
                        } else {
                            match self.put_value_to_target(target, val, env) {
                                Completion::Normal(_) | Completion::Empty => {}
                                other => return other,
                            }
                        }
                        Completion::Empty
                    })();
                    self.gc_unroot_frame(gc_frame);

                    match result {
                        Completion::Normal(_) | Completion::Empty => {}
                        other => return other,
                    }
                }
                _ => continue,
            }
        }
        Completion::Normal(JsValue::UNDEFINED)
    }

    fn eval_call(
        &mut self,
        callee: &Expression,
        args: &[Expression],
        env: &EnvRef,
        site_id: CallSiteId,
    ) -> Completion {
        let saved_tail = self.in_tail_position;
        self.in_tail_position = false;

        // Handle super() calls - call parent constructor with current this
        if matches!(callee, Expression::Super) {
            // §13.3.7.2 GetSuperConstructor: dynamically resolve via activeFunction.__proto__
            let super_ctor = if let Some(ctor_func) = env.borrow().get("__constructor_func__") {
                if let Some(o) = ctor_func
                    .as_object_id()
                    .map(|id| crate::types::JsObject { id })
                {
                    if let Some(obj_rc) = self.get_object_cell(o.id) {
                        if let Some(proto) = &obj_rc.borrow().prototype_id {
                            if let Some(id) = Some(*proto) {
                                JsValue::object(id)
                            } else {
                                JsValue::UNDEFINED
                            }
                        } else {
                            JsValue::NULL
                        }
                    } else {
                        JsValue::UNDEFINED
                    }
                } else {
                    JsValue::UNDEFINED
                }
            } else {
                env.borrow().get("__super__").unwrap_or(JsValue::UNDEFINED)
            };
            let gc_frame = self.gc_root_frame();
            let arg_vals = match self.eval_spread_args(args, env) {
                Ok(v) => v,
                Err(e) => {
                    self.gc_unroot_frame(gc_frame);
                    return Completion::Throw(e);
                }
            };
            let this_in_tdz = Self::this_is_in_tdz(env);
            if this_in_tdz {
                let current_new_target = self.new_target.clone().unwrap_or(super_ctor.clone());
                let saved_new_target = self.new_target.clone();
                let result =
                    self.construct_with_new_target(&super_ctor, &arg_vals, current_new_target);
                self.new_target = saved_new_target;
                self.gc_unroot_frame(gc_frame);
                if let Completion::Normal(ref v) = result {
                    Self::initialize_this_binding(env, v.clone());
                    if let Err(e) = self.initialize_instance_elements(v.clone(), env) {
                        return Completion::Throw(e);
                    }
                }
                return result;
            } else {
                let current_new_target = self.new_target.clone().unwrap_or(super_ctor.clone());
                let saved_new_target = self.new_target.clone();
                let result =
                    self.construct_with_new_target(&super_ctor, &arg_vals, current_new_target);
                self.new_target = saved_new_target;
                self.gc_unroot_frame(gc_frame);
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
            Expression::Member(obj_expr, prop, _) => {
                let is_super_call = matches!(obj_expr.as_ref(), Expression::Super);
                let obj_val = match self.eval_expr(obj_expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let key = match prop {
                    MemberProperty::Dot(name) => JsPropertyKey::from(name.clone()),
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
                        if let Some(o) = (obj_val)
                            .as_object_id()
                            .map(|id| crate::types::JsObject { id })
                            && let Some(obj) = self.get_object_cell(o.id)
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
                            // Evaluate the call arguments through the shared spread
                            // seam so a non-iterable spread throws (rather than being
                            // silently dropped) and every argument is GC-rooted, exactly
                            // as the ordinary member-call path does.
                            let gc_frame = self.gc_root_frame();
                            self.gc_root_value(&func_val);
                            self.gc_root_value(&obj_val);
                            let evaluated_args = match self.eval_spread_args(args, env) {
                                Ok(v) => v,
                                Err(e) => {
                                    self.gc_unroot_frame(gc_frame);
                                    return Completion::Throw(e);
                                }
                            };
                            if saved_tail {
                                self.gc_unroot_frame(gc_frame);
                                return Completion::TailCall {
                                    func: func_val,
                                    this: obj_val,
                                    args: evaluated_args,
                                };
                            }
                            let result = self.call_function(&func_val, &obj_val, &evaluated_args);
                            self.gc_unroot_frame(gc_frame);
                            return result;
                        }
                        return Completion::Throw(self.create_type_error(&format!(
                            "Cannot read private member #{name} from a non-object"
                        )));
                    }
                };
                // super.method() - look up on [[Prototype]] of HomeObject, bind this
                if is_super_call {
                    // Per spec §13.5.6: GetThisBinding() throws ReferenceError if this is in TDZ
                    if Self::this_is_in_tdz(env) {
                        return Completion::Throw(self.create_reference_error(
                            "Must call super constructor in derived class before accessing 'this' or returning from derived constructor",
                        ));
                    }
                    let this_val = env.borrow().get("this").unwrap_or(JsValue::UNDEFINED);
                    let home = env.borrow().get("__home_object__");
                    if let Some(home_id) = home.as_ref().and_then(JsValue::as_object_id) {
                        let proto_id = self
                            .get_object_cell(home_id)
                            .and_then(|ho_obj| ho_obj.borrow().prototype_id.as_ref().copied());
                        if let Some(pid) = proto_id {
                            let method = self.get_property_on_id(pid, &key);
                            (method, this_val)
                        } else {
                            return Completion::Throw(self.create_type_error(&format!(
                                "Cannot read properties of null (reading '{key}')"
                            )));
                        }
                    } else if let Some(o) = (obj_val)
                        .as_object_id()
                        .map(|id| crate::types::JsObject { id })
                    {
                        // Fallback: __super__.prototype for class super
                        let proto_val = self.get_property_on_id(o.id, "prototype");
                        if let Some(p) = (proto_val)
                            .as_object_id()
                            .map(|id| crate::types::JsObject { id })
                        {
                            let method = self.get_property_on_id(p.id, &key);
                            (method, this_val)
                        } else {
                            (JsValue::UNDEFINED, JsValue::UNDEFINED)
                        }
                    } else {
                        (JsValue::UNDEFINED, JsValue::UNDEFINED)
                    }
                } else if let Some(o) = (obj_val)
                    .as_object_id()
                    .map(|id| crate::types::JsObject { id })
                {
                    let oid = o.id;
                    let ov = obj_val.clone();
                    match self.get_object_property(oid, &key, &ov) {
                        Completion::Normal(method) => (method, obj_val),
                        other => return other,
                    }
                } else if obj_val.as_string().is_some() {
                    if let Some(sp_id) = self.realm().string_prototype {
                        let method = self.get_property_on_id(sp_id, &key);
                        (method, obj_val)
                    } else {
                        (JsValue::UNDEFINED, obj_val)
                    }
                } else if obj_val.is_number() {
                    let pid_opt = self
                        .realm()
                        .number_prototype
                        .or(self.realm().object_prototype);
                    if let Some(pid) = pid_opt {
                        let method = self.get_property_on_id(pid, &key);
                        (method, obj_val)
                    } else {
                        (JsValue::UNDEFINED, obj_val)
                    }
                } else if obj_val.is_boolean() {
                    let pid_opt = self
                        .realm()
                        .boolean_prototype
                        .or(self.realm().object_prototype);
                    if let Some(pid) = pid_opt {
                        let method = self.get_property_on_id(pid, &key);
                        (method, obj_val)
                    } else {
                        (JsValue::UNDEFINED, obj_val)
                    }
                } else if obj_val.is_symbol() {
                    if let Some(pid) = self.realm().symbol_prototype {
                        let desc = self.get_property_descriptor_on_id(pid, &key);
                        let method = match desc {
                            Some(ref d) if d.get.is_some() => {
                                let getter = d.get.clone().unwrap();
                                match self.call_function(&getter, &obj_val, &[]) {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                }
                            }
                            Some(ref d) => d.value.clone().unwrap_or(JsValue::UNDEFINED),
                            None => JsValue::UNDEFINED,
                        };
                        (method, obj_val)
                    } else {
                        (JsValue::UNDEFINED, obj_val)
                    }
                } else if obj_val.is_bigint() {
                    let pid_opt = self
                        .realm()
                        .bigint_prototype
                        .or(self.realm().object_prototype);
                    if let Some(pid) = pid_opt {
                        let method = self.get_property_on_id(pid, &key);
                        (method, obj_val)
                    } else {
                        (JsValue::UNDEFINED, obj_val)
                    }
                } else if obj_val.is_nullish() {
                    let err = self.create_type_error(&format!(
                        "Cannot read properties of {obj_val} (reading '{key}')"
                    ));
                    return Completion::Throw(err);
                } else {
                    (JsValue::UNDEFINED, obj_val)
                }
            }
            Expression::OptionalChain(oc_base, oc_chain) => {
                // (a?.b)() or similar: preserve this from optional chain
                match self.eval_optional_chain_with_ref(oc_base, oc_chain, env) {
                    Ok((v, t)) => (v, t),
                    Err(c) => return c,
                }
            }
            Expression::Identifier(_name) => {
                // §12.3.4.1: EvaluateCall — resolve identifier. If the reference comes
                // from a with-environment, thisValue = WithBaseObject.
                // eval_expr sets last_identifier_with_base during resolution.
                let val = match self.eval_expr(callee, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let this_val = match self.last_identifier_with_base.take() {
                    Some(obj_id) => JsValue::object(obj_id),
                    None => JsValue::UNDEFINED,
                };
                (val, this_val)
            }
            _ => {
                let val = match self.eval_expr(callee, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                (val, JsValue::UNDEFINED)
            }
        };

        // Direct eval: callee is bare `eval` identifier and resolves to built-in eval
        if matches!(callee, Expression::Identifier(n) if n == "eval")
            && self.is_builtin_eval(&func_val)
        {
            let gc_frame = self.gc_root_frame();
            let evaluated_args = match self.eval_spread_args(args, env) {
                Ok(args) => args,
                Err(e) => {
                    self.gc_unroot_frame(gc_frame);
                    return Completion::Throw(e);
                }
            };
            let caller_strict = env.borrow().strict;
            let result = self.perform_eval(&evaluated_args, caller_strict, true, env);
            self.gc_unroot_frame(gc_frame);
            return result;
        }

        // Root func_val and this_val before evaluating args (which may trigger GC)
        let gc_frame = self.gc_root_frame();
        self.gc_root_value(&func_val);
        self.gc_root_value(&this_val);
        let evaluated_args = match self.eval_spread_args(args, env) {
            Ok(args) => args,
            Err(e) => {
                self.gc_unroot_frame(gc_frame);
                return Completion::Throw(e);
            }
        };

        if saved_tail && !self.is_builtin_eval(&func_val) {
            self.gc_unroot_frame(gc_frame);
            return Completion::TailCall {
                func: func_val,
                this: this_val,
                args: evaluated_args,
            };
        }
        // Phase 3 call-IC probe + record. Issue #71.
        //  - Probe HIT increments call_ic_hit_count and dispatches through
        //    call_function_ic_validated, which skips the proxy/wrapped/
        //    class-ctor entry checks (9a6246f, #71 Phase-3 follow-up).
        //  - Probe MISS classifies the callable; if it's a plain native or
        //    user function (no proxy, no wrapped, not a class ctor without
        //    `new`, not bound), records Mono. Otherwise transitions to
        //    Megamorphic. State machine identical to PropIcSlot.
        if site_id != CallSiteId::UNASSIGNED
            && self.with_scope_depth == 0
            && let Some(o) = func_val
                .as_object_id()
                .map(|id| crate::types::JsObject { id })
        {
            use crate::interpreter::ic::CallIcSlot;
            let slot = *self.call_slot(site_id);
            let mut probe_hit = false;
            if let CallIcSlot::Mono {
                callee_obj_id,
                callee_shape_id,
                ..
            } = slot
                && o.id == callee_obj_id
                && let Some(obj_rc) = self.get_object(o.id)
                && obj_rc.borrow().shape_id == callee_shape_id
            {
                self.call_ic_hit_count.set(self.call_ic_hit_count.get() + 1);
                probe_hit = true;
            }
            if !probe_hit {
                self.call_ic_slow_path_count
                    .set(self.call_ic_slow_path_count.get() + 1);
            }
            // Phase-3 follow-up: route IC hits through the fast dispatch
            // entry that skips proxy/wrapped/class-ctor checks. Misses use
            // the slow path so the entry checks correctly classify novel
            // callables (proxies, bound, class ctors) before IC recording.
            let result = if probe_hit {
                self.call_function_ic_validated(&func_val, &this_val, &evaluated_args)
            } else {
                self.call_function(&func_val, &this_val, &evaluated_args)
            };
            // Record only on success to avoid caching error-paths.
            if !probe_hit && matches!(result, Completion::Normal(_)) {
                let new_slot = self.classify_for_call_ic(o.id);
                let next = match (slot, new_slot) {
                    (CallIcSlot::Megamorphic, _) => CallIcSlot::Megamorphic,
                    (_, None) => CallIcSlot::Empty,
                    (CallIcSlot::Empty, Some(s)) => s,
                    (
                        CallIcSlot::Mono {
                            callee_obj_id: prev,
                            ..
                        },
                        Some(
                            s @ CallIcSlot::Mono {
                                callee_obj_id: new, ..
                            },
                        ),
                    ) if prev == new => s,
                    (CallIcSlot::Mono { .. }, Some(_)) => CallIcSlot::Megamorphic,
                };
                *self.call_slot(site_id) = next;
            }
            self.gc_unroot_frame(gc_frame);
            return result;
        }
        let result = self.call_function(&func_val, &this_val, &evaluated_args);
        self.gc_unroot_frame(gc_frame);
        result
    }

    /// Classifies the post-call state of `callee_obj_id` into a
    /// `CallIcSlot::Mono { kind: ... }` ready for caching, or `None` if the
    /// site is not IC-able under the v1 narrow scope (proxy, wrapped, bound,
    /// or class-ctor-without-new). Phase 3, plan Step 14.
    ///
    /// `pub(super)` so the bytecode VM's `Call`/`ReturnCall` handling
    /// (`bytecode::vm`) can share this classification with the tree-walker's
    /// probe in `eval_call` (issue #432).
    pub(super) fn classify_for_call_ic(
        &self,
        callee_obj_id: u64,
    ) -> Option<crate::interpreter::ic::CallIcSlot> {
        use crate::interpreter::ic::{CallIcKind, CallIcSlot};
        let obj_rc = self.get_object(callee_obj_id)?;
        let obj = obj_rc.borrow();
        if obj.proxy().is_some()
            || obj.wrapped().is_some()
            || obj.bound().is_some()
            || obj.is_class_constructor()
        {
            return None;
        }
        let kind = match obj.callable.as_ref()? {
            JsFunction::Native(..) => CallIcKind::NativeFn,
            JsFunction::User { .. } => CallIcKind::UserFn,
        };
        Some(CallIcSlot::Mono {
            callee_obj_id,
            callee_shape_id: obj.shape_id,
            kind,
        })
    }

    fn bind_function_parameters(
        &mut self,
        params: &[Pattern],
        args: &[JsValue],
        func_env: &EnvRef,
        has_simple_params: bool,
    ) -> Result<(), JsValue> {
        if has_simple_params {
            let mut env = func_env.borrow_mut();
            for (index, param) in params.iter().enumerate() {
                let Pattern::Identifier(name) = param else {
                    unreachable!("simple parameter metadata must match the parameter list");
                };
                env.bindings.insert(
                    name.clone(),
                    Binding {
                        value: args.get(index).cloned().unwrap_or(JsValue::UNDEFINED),
                        kind: BindingKind::Var,
                        initialized: true,
                        deletable: false,
                    },
                );
            }
            return Ok(());
        }

        for (index, param) in params.iter().enumerate() {
            if let Pattern::Rest(inner) = param {
                let rest = args.get(index..).unwrap_or(&[]).to_vec();
                let rest_array = self.create_array(rest);
                self.bind_pattern(inner, rest_array, BindingKind::Var, func_env)?;
                break;
            }
            let value = args.get(index).cloned().unwrap_or(JsValue::UNDEFINED);
            self.bind_pattern(param, value, BindingKind::Var, func_env)?;
        }
        Ok(())
    }

    pub(crate) fn call_function(
        &mut self,
        func_val: &JsValue,
        this_val: &JsValue,
        args: &[JsValue],
    ) -> Completion {
        let mut result = self.call_function_inner(func_val, this_val, args, false);
        loop {
            match result {
                Completion::TailCall {
                    func,
                    this,
                    args: tc_args,
                } => {
                    result = self.call_function_inner(&func, &this, &tc_args, false);
                }
                other => return other,
            }
        }
    }

    /// Issue #71 Phase-3 follow-up: call_function variant for IC-validated
    /// hits. The first dispatch skips the proxy/wrapped/class-ctor entry
    /// checks (the IC's `Mono` state guarantees the callable is a plain
    /// native or user function). Tail-call recursion falls back to the
    /// slow path because the tail-called function is a different callable
    /// not validated by the originating IC slot.
    pub(crate) fn call_function_ic_validated(
        &mut self,
        func_val: &JsValue,
        this_val: &JsValue,
        args: &[JsValue],
    ) -> Completion {
        let mut result = self.call_function_inner(func_val, this_val, args, true);
        loop {
            match result {
                Completion::TailCall {
                    func,
                    this,
                    args: tc_args,
                } => {
                    result = self.call_function_inner(&func, &this, &tc_args, false);
                }
                other => return other,
            }
        }
    }

    /// Invoke a constructor body, then drive any proper-tail-call chain to
    /// completion — but capture the constructor frame's
    /// `last_call_had_explicit_return` / `last_call_this_value` side channels
    /// *before* driving the chain.
    ///
    /// A strict-mode `return <call>` in a constructor body produces a
    /// `Completion::TailCall`. The public `call_function` drives that chain
    /// internally, leaving the side channels reflecting the tail-callee rather
    /// than the constructor — so `[[Construct]]`'s "return `this` when the
    /// result is not an Object" substitution read the wrong `this` and
    /// explicit-return flag (issue #238). Behaviourally this is `call_function`
    /// plus a capture between the first `call_function_inner` and the drive
    /// loop: identical for non-tail-call bodies.
    fn call_constructor_body(
        &mut self,
        func: &JsValue,
        this: &JsValue,
        args: &[JsValue],
    ) -> (Completion, bool, Option<JsValue>) {
        let mut result = self.call_function_inner(func, this, args, false);
        let had_explicit_return = self.last_call_had_explicit_return;
        let final_this = self.last_call_this_value.take();
        // The constructor's frame is popped from the call stack the moment its
        // body returns a `TailCall`, and `last_call_this_value` is not a GC root
        // — so while the tail-call chain runs (its callees can allocate or call
        // `$262.gc()`) the freshly constructed object is reachable only through
        // these Rust locals. Temp-root it across the drive so it cannot be swept
        // before `[[Construct]]` returns it. Rooting `final_this` covers the base
        // path's `this_val` (same object); rooting `this` also guards the
        // `unwrap_or(this_val)` fallback and is a no-op for the derived path's
        // `undefined`.
        let gc_frame = self.gc_root_frame();
        self.gc_root_value(this);
        if let Some(v) = &final_this {
            self.gc_root_value(v);
        }
        while let Completion::TailCall {
            func,
            this,
            args: tc_args,
        } = result
        {
            result = self.call_function_inner(&func, &this, &tc_args, false);
        }
        self.gc_unroot_frame(gc_frame);
        (result, had_explicit_return, final_this)
    }

    fn call_function_inner(
        &mut self,
        func_val: &JsValue,
        this_val: &JsValue,
        args: &[JsValue],
        skip_entry_checks: bool,
    ) -> Completion {
        // Catchable stack-depth guard: every JS invocation funnels through here,
        // so bounding this depth bounds native recursion. Throwing a RangeError
        // before the native stack is exhausted keeps deep recursion catchable in
        // JS instead of aborting the process (SIGABRT).
        //
        // The soft limit fires once, then disarms so a JS catch handler running
        // deep in the stack (e.g. acorn's stack-overflow recovery) has room to
        // execute in the [soft, hard) band. The hard ceiling still fires even
        // while disarmed, and sits far below native capacity.
        use crate::interpreter::{
            CALL_DEPTH_HARD_LIMIT, CALL_DEPTH_REARM_LIMIT, CALL_DEPTH_SOFT_LIMIT,
        };
        if self.call_depth >= CALL_DEPTH_HARD_LIMIT
            || (self.overflow_armed && self.call_depth >= CALL_DEPTH_SOFT_LIMIT)
        {
            self.overflow_armed = false;
            return Completion::Throw(
                self.create_error("RangeError", "Maximum call stack size exceeded"),
            );
        }
        self.call_depth += 1;
        let result = self.call_function_inner_impl(func_val, this_val, args, skip_entry_checks);
        self.call_depth -= 1;
        if self.call_depth <= CALL_DEPTH_REARM_LIMIT {
            self.overflow_armed = true;
        }
        result
    }

    fn call_function_inner_impl(
        &mut self,
        func_val: &JsValue,
        _this_val: &JsValue,
        args: &[JsValue],
        skip_entry_checks: bool,
    ) -> Completion {
        if let Some(o) = (func_val)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
            && let Some(obj) = self.get_object_cell(o.id)
        {
            // Single borrow to check proxy/wrapped/class-ctor status. Skipped
            // when an IC hit already validated the callable category — see
            // `call_function_ic_validated`.
            let (is_proxy_or_revoked, has_wrapped_target, is_class_ctor) = if skip_entry_checks {
                self.call_ic_fast_dispatch_count
                    .set(self.call_ic_fast_dispatch_count.get() + 1);
                (false, false, false)
            } else {
                let b = obj.borrow();
                (
                    b.is_proxy() || b.is_proxy_revoked(),
                    b.wrapped().is_some(),
                    b.is_class_constructor(),
                )
            };
            if is_proxy_or_revoked {
                let target_val = self.get_proxy_target_val(o.id);
                let args_array = self.create_array(args.to_vec());
                match self.invoke_proxy_trap(
                    o.id,
                    "apply",
                    vec![target_val.clone(), _this_val.clone(), args_array],
                ) {
                    Ok(Some(v)) => return Completion::Normal(v),
                    Ok(None) => {
                        return self.call_function(&target_val, _this_val, args);
                    }
                    Err(e) => return Completion::Throw(e),
                }
            }
            if has_wrapped_target {
                return self.call_wrapped_function(o.id, _this_val, args);
            }
            if is_class_ctor && self.new_target.is_none() {
                let caller_realm = self.current_realm_id;
                if let Some(&fn_realm) = self.function_realm_map.get(&o.id) {
                    self.current_realm_id = fn_realm;
                }
                let err =
                    self.create_type_error("Class constructor cannot be invoked without 'new'");
                self.current_realm_id = caller_realm;
                return Completion::Throw(err);
            }
            let callable = obj.borrow().callable.clone();
            if let Some(func) = callable {
                // [[Call]] vs [[Construct]] new.target semantics:
                // - [[Call]]: new.target = undefined (clear for non-arrow functions)
                // - [[Construct]]: new.target = newTarget (preserve, set by construct_with_new_target)
                // - Arrow functions: inherit new.target from enclosing scope (don't clear)
                let is_arrow_func = matches!(func, JsFunction::User { is_arrow, .. } if is_arrow);
                let was_construct = std::mem::replace(&mut self.calling_as_construct, false);
                let outer_new_target = if !is_arrow_func {
                    let nt = self.new_target.take();
                    if was_construct {
                        self.new_target = nt.clone();
                    }
                    Some(nt)
                } else {
                    // Arrow functions inherit new.target from creation context
                    let saved = self.new_target.take();
                    if let JsFunction::User {
                        captured_new_target: Some(ref cnt),
                        ..
                    } = func
                    {
                        self.new_target = Some(cnt.clone());
                    }
                    Some(saved)
                };
                let result = match func {
                    JsFunction::Native(_, _, f, _) => {
                        let caller_realm = self.current_realm_id;
                        if let Some(&fn_realm) = self.function_realm_map.get(&o.id) {
                            self.current_realm_id = fn_realm;
                        }
                        self.gc_root_value(_this_val);
                        for a in args.iter() {
                            self.gc_root_value(a);
                        }
                        let saved_this = self.last_call_this_value.take();
                        let result = f(self, _this_val, args);
                        self.last_call_this_value = saved_this;
                        self.last_call_had_explicit_return = true;
                        // Unroot the native call operands after the call returns.
                        for a in args.iter().rev() {
                            self.gc_unroot_value(a);
                        }
                        self.gc_unroot_value(_this_val);
                        self.current_realm_id = caller_realm;
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
                        uses_arguments,
                        has_simple_params,
                        ..
                    } => {
                        // §10.2.1.1 PrepareForOrdinaryCall: switch to function's realm
                        let caller_realm = self.current_realm_id;
                        if let Some(&fn_realm) = self.function_realm_map.get(&o.id) {
                            self.current_realm_id = fn_realm;
                        }

                        if is_async && !is_generator {
                            let result = self.call_async_function(
                                &params,
                                &body,
                                closure.clone(),
                                is_arrow,
                                is_strict,
                                _this_val,
                                args,
                                func_val,
                                uses_arguments,
                                has_simple_params,
                            );
                            self.current_realm_id = caller_realm;
                            return result;
                        }
                        if is_async && is_generator {
                            // Create persistent function environment
                            let func_env = Environment::new_function_scope_with_capacity(
                                Some(closure.clone()),
                                params.len().saturating_add(2),
                            );
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
                            let is_simple_ag = has_simple_params;
                            if uses_arguments {
                                let env_strict_ag = func_env.borrow().strict;
                                let use_mapped_ag = is_simple_ag && !is_strict && !env_strict_ag;
                                let param_names_ag: Vec<String> = if use_mapped_ag {
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
                                let mapped_env_ag =
                                    if use_mapped_ag { Some(&func_env) } else { None };
                                let arguments_obj = self.create_arguments_object(
                                    args,
                                    func_val.clone(),
                                    is_strict,
                                    mapped_env_ag,
                                    &param_names_ag,
                                );
                                func_env.borrow_mut().declare("arguments", BindingKind::Var);
                                let _ = self.env_set(&func_env, "arguments", arguments_obj);
                                if is_strict || !is_simple_ag {
                                    func_env.borrow_mut().arguments_immutable = true;
                                }
                            } else {
                                func_env.borrow_mut().declare("arguments", BindingKind::Var);
                            }
                            if !is_simple_ag {
                                func_env.borrow_mut().has_parameter_expressions = true;
                            }
                            // §14.5.10 step 1: FunctionDeclarationInstantiation (bind params)
                            if let Err(error) = self.bind_function_parameters(
                                &params,
                                args,
                                &func_env,
                                is_simple_ag,
                            ) {
                                self.current_realm_id = caller_realm;
                                return Completion::Throw(error);
                            }
                            // §14.5.10 step 2: OrdinaryCreateFromConstructor AFTER decl inst
                            let gen_obj_id = self.create_object_id();
                            let mut proto_set = false;
                            if let Some(func_obj_rc) = self.get_object_cell(o.id) {
                                let proto_val =
                                    func_obj_rc.borrow().get_property_value("prototype");
                                if let Some(proto_id) =
                                    proto_val.as_ref().and_then(JsValue::as_object_id)
                                {
                                    self.get_object_cell_expect(gen_obj_id)
                                        .borrow_mut()
                                        .prototype_id = Some(proto_id);
                                    proto_set = true;
                                }
                            }
                            if !proto_set {
                                let fn_realm_id = match self.get_function_realm(func_val) {
                                    Ok(r) => r,
                                    Err(e) => return Completion::Throw(e),
                                };
                                let agp_id = self.realms[fn_realm_id].async_generator_prototype;
                                self.get_object_cell_expect(gen_obj_id)
                                    .borrow_mut()
                                    .prototype_id = agp_id;
                            }
                            self.get_object_cell_expect(gen_obj_id)
                                .borrow_mut()
                                .class_name = "AsyncGenerator".to_string();
                            let is_simple = has_simple_params;
                            let exec_env = if !is_simple {
                                let body_env =
                                    Environment::new_function_scope(Some(func_env.clone()));
                                body_env.borrow_mut().strict = func_env.borrow().strict;
                                body_env.borrow_mut().has_simple_params = false;
                                let mut var_names = HashSet::new();
                                Self::collect_var_names_from_stmts(body.as_slice(), &mut var_names);
                                let mut param_names_set = HashSet::new();
                                for p in params.iter() {
                                    Self::collect_var_names_from_pattern(p, &mut param_names_set);
                                }
                                for name in &var_names {
                                    body_env.borrow_mut().declare(name, BindingKind::Var);
                                    if param_names_set.contains(name) || name == "arguments" {
                                        let val = func_env
                                            .borrow()
                                            .get(name)
                                            .unwrap_or(JsValue::UNDEFINED);
                                        let _ = self.env_set(&body_env, name, val);
                                    }
                                }
                                body_env
                            } else {
                                func_env.clone()
                            };

                            use crate::interpreter::generator_transform::transform_async_generator;
                            let state_machine =
                                Rc::new(transform_async_generator(body.as_slice(), &params));
                            for temp_var in &state_machine.temp_vars {
                                exec_env.borrow_mut().declare(temp_var, BindingKind::Var);
                            }
                            for lv in &state_machine.local_vars {
                                let bk = match lv.kind {
                                    crate::ast::VarKind::Let
                                    | crate::ast::VarKind::Const
                                    | crate::ast::VarKind::Using
                                    | crate::ast::VarKind::AwaitUsing => {
                                        // Block-scoped bindings must not be hoisted to function level
                                        if lv.scope_depth > 0 {
                                            continue;
                                        }
                                        BindingKind::Let
                                    }
                                    _ => BindingKind::Var,
                                };
                                if !exec_env.borrow().bindings.contains_key(&lv.name)
                                    && !func_env.borrow().bindings.contains_key(&lv.name)
                                {
                                    exec_env.borrow_mut().declare(&lv.name, bk);
                                }
                            }
                            self.get_object_cell_expect(gen_obj_id).borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineAsyncGenerator {
                                        state_machine,
                                        func_env: exec_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::SuspendedStart,
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            let gen_id = gen_obj_id;
                            if let Some(obj_rc) = self.get_object_cell(gen_id) {
                                obj_rc.borrow_mut().generator_realm_id =
                                    Some(self.current_realm_id);
                            }
                            self.current_realm_id = caller_realm;
                            return Completion::Normal(JsValue::object(gen_id));
                        }
                        if is_generator {
                            // Create persistent function environment
                            let func_env = Environment::new_function_scope_with_capacity(
                                Some(closure.clone()),
                                params.len().saturating_add(2),
                            );
                            let closure_strict = closure.borrow().strict;
                            func_env.borrow_mut().strict = is_strict;
                            // §10.2.1.2 OrdinaryCallBindThis: sloppy mode this coercion
                            let effective_this = if !is_strict && !closure_strict {
                                if (_this_val).is_nullish() {
                                    self.realm()
                                        .global_env
                                        .borrow()
                                        .get("this")
                                        .unwrap_or(_this_val.clone())
                                } else if !(_this_val).is_object() {
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
                            let is_simple_g = has_simple_params;
                            if uses_arguments {
                                let env_strict_g = func_env.borrow().strict;
                                let use_mapped_g = is_simple_g && !is_strict && !env_strict_g;
                                let param_names_g: Vec<String> = if use_mapped_g {
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
                                let mapped_env_g =
                                    if use_mapped_g { Some(&func_env) } else { None };
                                let arguments_obj = self.create_arguments_object(
                                    args,
                                    func_val.clone(),
                                    is_strict,
                                    mapped_env_g,
                                    &param_names_g,
                                );
                                func_env.borrow_mut().declare("arguments", BindingKind::Var);
                                let _ = self.env_set(&func_env, "arguments", arguments_obj);
                                if is_strict || !is_simple_g {
                                    func_env.borrow_mut().arguments_immutable = true;
                                }
                            } else {
                                func_env.borrow_mut().declare("arguments", BindingKind::Var);
                            }
                            if !is_simple_g {
                                func_env.borrow_mut().has_parameter_expressions = true;
                            }
                            // §14.4.10 step 1: FunctionDeclarationInstantiation (bind params)
                            if let Err(error) =
                                self.bind_function_parameters(&params, args, &func_env, is_simple_g)
                            {
                                self.current_realm_id = caller_realm;
                                return Completion::Throw(error);
                            }
                            // §14.4.10 step 2: OrdinaryCreateFromConstructor AFTER decl inst
                            let gen_obj_id = self.create_object_id();
                            let mut proto_set = false;
                            if let Some(func_obj_rc) = self.get_object_cell(o.id) {
                                let proto_val =
                                    func_obj_rc.borrow().get_property_value("prototype");
                                if let Some(proto_id) =
                                    proto_val.as_ref().and_then(JsValue::as_object_id)
                                {
                                    self.get_object_cell_expect(gen_obj_id)
                                        .borrow_mut()
                                        .prototype_id = Some(proto_id);
                                    proto_set = true;
                                }
                            }
                            if !proto_set {
                                let fn_realm_id = match self.get_function_realm(func_val) {
                                    Ok(r) => r,
                                    Err(e) => return Completion::Throw(e),
                                };
                                let gp_id = self.realms[fn_realm_id].generator_prototype;
                                self.get_object_cell_expect(gen_obj_id)
                                    .borrow_mut()
                                    .prototype_id = gp_id;
                            }
                            self.get_object_cell_expect(gen_obj_id)
                                .borrow_mut()
                                .class_name = "Generator".to_string();
                            let is_simple = has_simple_params;
                            let exec_env = if !is_simple {
                                let body_env =
                                    Environment::new_function_scope(Some(func_env.clone()));
                                body_env.borrow_mut().strict = func_env.borrow().strict;
                                body_env.borrow_mut().has_simple_params = false;
                                let mut var_names = HashSet::new();
                                Self::collect_var_names_from_stmts(body.as_slice(), &mut var_names);
                                let mut param_names_set = HashSet::new();
                                for p in params.iter() {
                                    Self::collect_var_names_from_pattern(p, &mut param_names_set);
                                }
                                for name in &var_names {
                                    body_env.borrow_mut().declare(name, BindingKind::Var);
                                    if param_names_set.contains(name) || name == "arguments" {
                                        let val = func_env
                                            .borrow()
                                            .get(name)
                                            .unwrap_or(JsValue::UNDEFINED);
                                        let _ = self.env_set(&body_env, name, val);
                                    }
                                }
                                body_env
                            } else {
                                func_env.clone()
                            };

                            use crate::interpreter::generator_transform::transform_generator;
                            let state_machine =
                                Rc::new(transform_generator(body.as_slice(), &params));
                            for temp_var in &state_machine.temp_vars {
                                exec_env.borrow_mut().declare(temp_var, BindingKind::Var);
                            }
                            for lv in &state_machine.local_vars {
                                let bk = match lv.kind {
                                    crate::ast::VarKind::Let
                                    | crate::ast::VarKind::Const
                                    | crate::ast::VarKind::Using
                                    | crate::ast::VarKind::AwaitUsing => {
                                        // Block-scoped bindings must not be hoisted to function level
                                        if lv.scope_depth > 0 {
                                            continue;
                                        }
                                        BindingKind::Let
                                    }
                                    _ => BindingKind::Var,
                                };
                                if !exec_env.borrow().bindings.contains_key(&lv.name)
                                    && !func_env.borrow().bindings.contains_key(&lv.name)
                                {
                                    exec_env.borrow_mut().declare(&lv.name, bk);
                                }
                            }
                            self.get_object_cell_expect(gen_obj_id).borrow_mut().kind =
                                crate::interpreter::types::ObjectKind::Iterator(
                                    IteratorState::StateMachineGenerator {
                                        state_machine,
                                        func_env: exec_env,
                                        is_strict,
                                        execution_state: StateMachineExecutionState::SuspendedStart,
                                        _sent_value: JsValue::UNDEFINED,
                                        try_stack: vec![],
                                        pending_binding: None,
                                        delegated_iterator: None,
                                        pending_exception: None,
                                        pending_return: None,
                                    },
                                );
                            let gen_id = gen_obj_id;
                            if let Some(obj_rc) = self.get_object_cell(gen_id) {
                                obj_rc.borrow_mut().generator_realm_id =
                                    Some(self.current_realm_id);
                            }
                            self.current_realm_id = caller_realm;
                            return Completion::Normal(JsValue::object(gen_id));
                        }
                        let closure_strict = closure.borrow().strict;
                        let func_env = self
                            .acquire_function_environment(closure, params.len().saturating_add(2));
                        if is_arrow {
                            func_env.borrow_mut().is_arrow_scope = true;
                        }
                        let is_simple = has_simple_params;
                        let mut call_frame_arguments = CallFrameArguments::None;
                        if !is_arrow {
                            if self.constructing_derived {
                                // Derived constructor: this is in TDZ until super() is called
                                func_env.borrow_mut().bindings.insert(
                                    "this".to_string(),
                                    Binding {
                                        value: JsValue::UNDEFINED,
                                        kind: BindingKind::Const,
                                        initialized: false,
                                        deletable: false,
                                    },
                                );
                                func_env.borrow_mut().is_derived_constructor_scope = true;
                                self.constructing_derived = false;
                            } else {
                                let effective_this = if !is_strict && !closure_strict {
                                    if (_this_val).is_nullish() {
                                        self.realm()
                                            .global_env
                                            .borrow()
                                            .get("this")
                                            .unwrap_or(_this_val.clone())
                                    } else if !(_this_val).is_object() {
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
                            if uses_arguments {
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
                                call_frame_arguments =
                                    CallFrameArguments::Materialized(arguments_obj.clone());
                                func_env.borrow_mut().declare("arguments", BindingKind::Var);
                                let _ = self.env_set(&func_env, "arguments", arguments_obj);
                                if is_strict || !is_simple {
                                    func_env.borrow_mut().arguments_immutable = true;
                                }
                            } else {
                                func_env.borrow_mut().declare("arguments", BindingKind::Var);
                                if !is_strict && !env_strict {
                                    call_frame_arguments = CallFrameArguments::Deferred {
                                        args: DeferredCallArguments::new(args),
                                        func_env: func_env.clone(),
                                        mapped: is_simple,
                                    };
                                }
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
                        if !is_simple {
                            func_env.borrow_mut().has_parameter_expressions = true;
                        }
                        // Bind parameters (after this so default exprs can access this)
                        if let Err(error) =
                            self.bind_function_parameters(&params, args, &func_env, is_simple)
                        {
                            self.current_realm_id = caller_realm;
                            self.recycle_function_environment(func_env);
                            return Completion::Throw(error);
                        }
                        let exec_env = if !is_simple {
                            let body_env = Environment::new_function_scope(Some(func_env.clone()));
                            body_env.borrow_mut().strict = func_env.borrow().strict;
                            body_env.borrow_mut().has_simple_params = false;
                            let mut var_names = HashSet::new();
                            Self::collect_var_names_from_stmts(body.as_slice(), &mut var_names);
                            let mut param_names = HashSet::new();
                            for p in params.iter() {
                                Self::collect_var_names_from_pattern(p, &mut param_names);
                            }
                            for name in &var_names {
                                body_env.borrow_mut().declare(name, BindingKind::Var);
                                if param_names.contains(name) || name == "arguments" {
                                    let val =
                                        self.env_get(&func_env, name).unwrap_or(JsValue::UNDEFINED);
                                    let _ = self.env_set(&body_env, name, val);
                                }
                            }
                            body_env
                        } else {
                            func_env.clone()
                        };
                        exec_env.borrow_mut().strict = is_strict;
                        self.call_stack_frames.push(CallFrame {
                            func_obj_id: o.id,
                            arguments: call_frame_arguments,
                            is_eval: false,
                        });
                        self.call_stack_envs.push(exec_env.clone());
                        self.in_tail_position = false;
                        // A fresh function body: its tail positions are
                        // independent of any try/finally region in the caller.
                        let saved_tco_suppress = self.tco_suppress_depth;
                        self.tco_suppress_depth = 0;
                        let result = self.dispatch_body(o.id, &body, &exec_env, _this_val);
                        self.tco_suppress_depth = saved_tco_suppress;
                        self.call_stack_envs.pop();
                        self.call_stack_frames.pop();
                        let result = self.dispose_resources(&exec_env, result);
                        self.last_call_this_value = func_env.borrow().get("this");
                        self.current_realm_id = caller_realm;
                        drop(exec_env);
                        self.recycle_function_environment(func_env);
                        match result {
                            Completion::Return(v) => {
                                self.last_call_had_explicit_return = true;
                                Completion::Normal(v)
                            }
                            Completion::TailCall { .. } => {
                                self.last_call_had_explicit_return = true;
                                result
                            }
                            Completion::Normal(_) | Completion::Empty => {
                                self.last_call_had_explicit_return = false;
                                Completion::Normal(JsValue::UNDEFINED)
                            }
                            Completion::Yield(_) => Completion::Normal(JsValue::UNDEFINED),
                            other => other,
                        }
                    }
                };
                if let Some(nt) = outer_new_target {
                    self.new_target = nt;
                }
                return result;
            }
        }
        let desc = if func_val.is_undefined() {
            "undefined is not a function".to_string()
        } else if func_val.is_null() {
            "null is not a function".to_string()
        } else if let Some(b) = func_val.as_boolean() {
            format!("{} is not a function", b)
        } else if let Some(n) = func_val.as_number() {
            format!("{} is not a function", n)
        } else if let Some(s) = func_val.as_string() {
            let preview: String = s.to_rust_string().chars().take(30).collect();
            format!("\"{}\" is not a function", preview)
        } else if let Some(id) = func_val.as_object_id() {
            if let Some(obj) = self.get_object_cell(id) {
                let class = obj.borrow().class_name.clone();
                let has_callable = obj.borrow().callable.is_some();
                let keys: Vec<JsPropertyKey> = obj
                    .borrow()
                    .property_order
                    .iter()
                    .take(10)
                    .cloned()
                    .collect();
                format!(
                    "object (class={}, callable={}, id={}, keys={:?}) is not a function",
                    class, has_callable, id, keys
                )
            } else {
                format!("object (id={}, GC'd?) is not a function", id)
            }
        } else {
            "is not a function".to_string()
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
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::UNDEFINED,
                };
                let items = self.iterate_to_vec(&val)?;
                for item in &items {
                    self.gc_root_value(item);
                }
                evaluated.extend(items);
            } else {
                let val = match self.eval_expr(arg, env) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::UNDEFINED,
                };
                self.gc_root_value(&val);
                evaluated.push(val);
            }
        }
        Ok(evaluated)
    }

    fn is_builtin_eval(&self, val: &JsValue) -> bool {
        if let Some(o) = (val).as_object_id().map(|id| crate::types::JsObject { id }) {
            // Direct eval must be the CURRENT realm's eval
            if let Some(eval_id) = self.realm().builtin_eval_id {
                return o.id == eval_id;
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
        let arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
        if !arg.is_string() {
            return Completion::Normal(arg);
        }
        // Use PUA mapping to preserve lone surrogates through the UTF-8 parser
        let code = if let Some(s) = (arg).as_string() {
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
            let mut found_derived_constructor = false;
            let mut function_boundary_count: u32 = 0;
            let mut env_walk = Some(caller_env.clone());
            while let Some(ref e) = env_walk {
                let e = e.clone();
                let borrowed = e.borrow();
                if borrowed.is_field_initializer {
                    in_field_initializer = true;
                }
                // __home_object__ lives in the method's closure env (one scope
                // above its function scope). Allow finding it through the first
                // non-arrow function boundary (the method itself) but not
                // through a second one (a nested non-method function).
                if function_boundary_count <= 1
                    && borrowed.bindings.contains_key("__home_object__")
                    && !found_home_object
                {
                    found_home_object = true;
                }
                if function_boundary_count <= 1
                    && borrowed.is_derived_constructor_scope
                    && !found_derived_constructor
                {
                    found_derived_constructor = true;
                }
                if borrowed.is_function_scope && !borrowed.is_arrow_scope {
                    if !found_function {
                        found_function = true;
                    }
                    function_boundary_count += 1;
                }
                if let Some(ref names) = borrowed.class_private_names {
                    let name_set: HashSet<String> = names.keys().cloned().collect();
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
            if found_derived_constructor {
                p.set_eval_allow_super_call();
            }
        }
        if in_field_initializer {
            p.set_eval_in_field_initializer();
        }
        let mut program = match p.parse_program() {
            Ok(prog) => prog,
            Err(e) => {
                return Completion::Throw(self.create_error("SyntaxError", &format!("{}", e)));
            }
        };
        crate::ast::assign_ic_sites(&mut program.body);
        // Validate private name usage in eval-in-class context
        if let Err(e) = p.validate_eval_private_names() {
            return Completion::Throw(self.create_error("SyntaxError", &format!("{}", e)));
        }
        if in_field_initializer {
            if crate::ast::stmts_contain_matching(
                program.body.as_slice(),
                &crate::ast::is_arguments_reference,
            ) {
                return Completion::Throw(self.create_error(
                    "SyntaxError",
                    "'arguments' is not allowed in class field initializer or static block",
                ));
            }
            if crate::ast::stmts_contain_matching(
                program.body.as_slice(),
                &crate::ast::is_super_call,
            ) {
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
            program.body.as_slice(),
            &var_env,
            &lex_env,
            is_strict,
            direct,
            caller_env,
        ) {
            return Completion::Throw(e);
        }

        // Execute statements in lex_env
        self.call_stack_frames.push(CallFrame {
            func_obj_id: 0,
            arguments: CallFrameArguments::None,
            is_eval: true,
        });
        self.call_stack_envs.push(lex_env.clone());
        let result = self.exec_eval_body(&program.body, &lex_env);
        self.call_stack_envs.pop();
        self.call_stack_frames.pop();
        self.dispose_resources(&lex_env, result.update_empty(JsValue::UNDEFINED))
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
        let is_global = var_env.borrow().global_object_id.is_some();

        // Collect function declarations to initialize
        let functions_to_init = super::hoisting::collect_function_decls(body);
        let declared_func_names: Vec<String> =
            functions_to_init.iter().map(|f| f.name.clone()).collect();

        // Collect var-declared names (excluding those that are also function names)
        let all_var_names = super::hoisting::collect_var_names(body);
        let declared_var_names: Vec<String> = {
            let mut seen = HashSet::new();
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

        // §19.2.1.3 / §10.2.11: eval in parameter initializers of non-simple-param functions.
        // When eval runs in parameter defaults, var declarations must not conflict with params.
        if direct
            && !is_global
            && !strict
            && Rc::ptr_eq(caller_env, var_env)
            && var_env.borrow().has_parameter_expressions
        {
            let all_names: Vec<String> = declared_func_names
                .iter()
                .chain(declared_var_names.iter())
                .cloned()
                .collect();
            for name in &all_names {
                if var_env.borrow().bindings.contains_key(name) {
                    return Err(self.create_error(
                        "SyntaxError",
                        &format!("Identifier '{}' has already been declared", name),
                    ));
                }
            }
        }

        if !strict {
            // §19.2.1.4 step 5.a: if varEnv is global, check for lexical conflicts
            // Only check for true lexical declarations (let/const/class), not built-in
            // value properties like NaN/Infinity/undefined which are stored as ImmutableValue
            // but are part of the object environment record, not the declarative record.
            if is_global {
                let all_names: Vec<String> = declared_func_names
                    .iter()
                    .chain(declared_var_names.iter())
                    .cloned()
                    .collect();
                let global_id = var_env.borrow().global_object_id;
                let global_obj = global_id.and_then(|id| self.get_object_cell(id));
                let env_b = var_env.borrow();
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
                // §10.2.11 step 29: In sloppy mode, the spec creates a separate
                // lexical environment for let/const/class (child of var env). Our
                // engine merges them, so when caller_env === var_env, also check
                // for let/const conflicts within the same function scope.
                if direct && Rc::ptr_eq(caller_env, var_env) {
                    let env_b = var_env.borrow();
                    for name in &all_names {
                        if let Some(binding) = env_b.bindings.get(name)
                            && matches!(binding.kind, BindingKind::Let | BindingKind::Const)
                        {
                            drop(env_b);
                            return Err(self.create_error(
                                "SyntaxError",
                                &format!("Identifier '{}' has already been declared", name),
                            ));
                        }
                    }
                }
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
                    // B.3.5: simple catch scopes allow var redeclaration
                    if !env.borrow().is_simple_catch_scope {
                        for name in &all_names {
                            if env.borrow().bindings.contains_key(name) {
                                return Err(self.create_error(
                                    "SyntaxError",
                                    &format!("Identifier '{}' has already been declared", name),
                                ));
                            }
                        }
                    }
                    let next = env.borrow().parent.clone();
                    check_env = next;
                }
            }
        }

        // Check CanDeclareGlobalFunction / CanDeclareGlobalVar for global context
        if is_global {
            let global_id = var_env.borrow().global_object_id;
            let global_obj = global_id.and_then(|id| self.get_object(id));
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
                params: Rc::new(f.params.clone()),
                body: f.body.clone(),
                closure: lex_env.clone(),
                is_arrow: false,
                is_strict: f.body_is_strict || enclosing_strict,
                is_generator: f.is_generator,
                is_async: f.is_async,
                is_method: false,
                source_text: f.source_text.clone(),
                captured_new_target: None,
                uses_arguments: func_uses_arguments(&f.params, &f.body),
                has_simple_params: crate::ast::params_are_simple(&f.params),
            };
            let val = self.create_function(func);
            if is_global {
                self.env_declare_global_function_binding(var_env, &f.name, val, true);
            } else {
                if !var_env.borrow().bindings.contains_key(&f.name) {
                    var_env
                        .borrow_mut()
                        .declare_deletable(&f.name, BindingKind::Var);
                }
                let _ = self.env_set(var_env, &f.name, val);
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
                        d.pattern.bound_names(&mut names);
                        for name in names {
                            lex_env.borrow_mut().bindings.insert(
                                name,
                                Binding {
                                    value: JsValue::UNDEFINED,
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
                            value: JsValue::UNDEFINED,
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
                    self.env_declare_global_var_configurable(var_env, name);
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
                                d.pattern.bound_names(&mut eval_lexical_names);
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
                                self.env_declare_global_var_configurable(var_env, &name);
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

    fn eval_new(
        &mut self,
        callee: &Expression,
        args: &[Expression],
        env: &EnvRef,
        _site_id: CallSiteId,
    ) -> Completion {
        let gc_frame = self.gc_root_frame();
        let callee_val = match self.eval_expr(callee, env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        self.gc_root_value(&callee_val);
        let evaluated_args = match self.eval_spread_args(args, env) {
            Ok(args) => args,
            Err(e) => {
                self.gc_unroot_frame(gc_frame);
                return Completion::Throw(e);
            }
        };
        // Check if callee is a constructor
        if let Some(co) = (callee_val)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            let is_proxy = self.get_proxy_info(co.id).is_some();
            if !is_proxy && let Some(func_obj) = self.get_object_cell(co.id) {
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
                    self.gc_unroot_frame(gc_frame);
                    return Completion::Throw(
                        self.create_type_error(&format!("{} is not a constructor", name)),
                    );
                }
            }
        } else {
            self.gc_unroot_frame(gc_frame);
            return Completion::Throw(
                self.create_type_error(&format!("{:?} is not a constructor", callee_val)),
            );
        }
        // Proxy construct trap
        if let Some(co) = (callee_val)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
            && self.get_proxy_info(co.id).is_some()
        {
            let target_val = self.get_proxy_target_val(co.id);
            let args_array = self.create_array(evaluated_args.clone());
            let new_target = callee_val.clone();
            self.gc_unroot_frame(gc_frame);
            match self.invoke_proxy_trap(
                co.id,
                "construct",
                vec![target_val.clone(), args_array, new_target.clone()],
            ) {
                Ok(Some(v)) => {
                    if (v).is_object() {
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
        // Bound functions: delegate to construct_with_new_target which handles new_target resolution
        if let Some(co) = (callee_val)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
            && let Some(func_obj) = self.get_object_cell(co.id)
            && func_obj.borrow().bound().is_some()
        {
            self.gc_unroot_frame(gc_frame);
            return self.construct_with_new_target(
                &callee_val,
                &evaluated_args,
                callee_val.clone(),
            );
        }
        // Check if this is a derived class constructor
        let is_derived = if let Some(o) = callee_val
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
            && let Some(func_obj) = self.get_object_cell(o.id)
        {
            func_obj.borrow().is_derived_class_constructor()
        } else {
            false
        };

        // Fast path for default derived constructor: bypass the synthetic body to avoid
        // invoking Symbol.iterator on the rest parameter (spec §15.7.14).
        if is_derived {
            let is_default_derived = if let Some(o) = callee_val
                .as_object_id()
                .map(|id| crate::types::JsObject { id })
                && let Some(func_obj) = self.get_object_cell(o.id)
            {
                func_obj.borrow().is_default_derived_constructor()
            } else {
                false
            };
            if is_default_derived {
                // Use dynamic [[Prototype]] lookup so setPrototypeOf takes effect
                let super_ctor = if let Some(o) = callee_val
                    .as_object_id()
                    .map(|id| crate::types::JsObject { id })
                    && let Some(func_obj) = self.get_object_cell(o.id)
                {
                    if let Some(id) = func_obj.borrow().prototype_id {
                        JsValue::object(id)
                    } else {
                        JsValue::UNDEFINED
                    }
                } else {
                    JsValue::UNDEFINED
                };
                let prev_new_target = self.new_target.take();
                self.new_target = Some(callee_val.clone());
                self.gc_unroot_frame(gc_frame);
                let result = self.construct_with_new_target(
                    &super_ctor,
                    &evaluated_args,
                    callee_val.clone(),
                );
                if let Completion::Normal(ref new_obj) = result {
                    // initialize_instance_elements reads self.new_target to find class fields,
                    // so keep new_target set to callee_val until after it returns.
                    if let Err(e) = self.initialize_instance_elements(new_obj.clone(), env) {
                        self.new_target = prev_new_target;
                        return Completion::Throw(e);
                    }
                }
                self.new_target = prev_new_target;
                return result;
            }
        }

        if is_derived {
            // Derived constructor: don't create this, let super() handle it
            let prev_new_target = self.new_target.take();
            self.new_target = Some(callee_val.clone());
            self.last_call_had_explicit_return = false;
            self.last_call_this_value = None;
            let prev_constructing_derived = self.constructing_derived;
            self.constructing_derived = true;
            self.calling_as_construct = true;
            let (result, had_explicit_return, final_this) =
                self.call_constructor_body(&callee_val, &JsValue::UNDEFINED, &evaluated_args);
            self.gc_unroot_frame(gc_frame);
            self.constructing_derived = prev_constructing_derived;
            self.new_target = prev_new_target;
            match result {
                Completion::Normal(v) if had_explicit_return && (v).is_object() => {
                    Completion::Normal(v)
                }
                Completion::Normal(ref v) if had_explicit_return && !(v).is_undefined() => {
                    Completion::Throw(self.create_type_error(
                        "Derived constructors may only return object or undefined",
                    ))
                }
                Completion::Normal(_) | Completion::Empty => {
                    match final_this {
                        Some(v) if (v).is_object() => Completion::Normal(v),
                        Some(v) if !(v).is_undefined() => Completion::Normal(v),
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
            let new_obj_id = self.create_object_id();
            if let Some(o) = callee_val
                .as_object_id()
                .map(|id| crate::types::JsObject { id })
                && let Some(func_obj) = self.get_object_cell(o.id)
            {
                let proto = func_obj.borrow().get_property_value("prototype");
                if let Some(proto_id) = proto.as_ref().and_then(JsValue::as_object_id) {
                    self.get_object_cell_expect(new_obj_id)
                        .borrow_mut()
                        .prototype_id = Some(proto_id);
                }
            }
            let instance_field_defs = if let Some(o) = callee_val
                .as_object_id()
                .map(|id| crate::types::JsObject { id })
                && let Some(func_obj) = self.get_object_cell(o.id)
            {
                func_obj.borrow().class_instance_field_defs.clone()
            } else {
                Vec::new()
            };
            let this_val = JsValue::object(new_obj_id);
            // Use constructor's closure (class_env) so the class name binding
            // is accessible in field initializers (spec §15.7.14 step 28.e.i).
            let init_parent = if let Some(o) = callee_val
                .as_object_id()
                .map(|id| crate::types::JsObject { id })
                && let Some(func_obj) = self.get_object_cell(o.id)
                && let Some(JsFunction::User { ref closure, .. }) = func_obj.borrow().callable
            {
                closure.clone()
            } else {
                env.clone()
            };
            let init_env = Environment::new(Some(init_parent));
            init_env.borrow_mut().declare("this", BindingKind::Const);
            init_env
                .borrow_mut()
                .initialize_binding("this", this_val.clone());
            init_env.borrow_mut().is_field_initializer = true;
            if let Some(o) = callee_val
                .as_object_id()
                .map(|id| crate::types::JsObject { id })
                && let Some(func_obj) = self.get_object_cell(o.id)
            {
                if let Some(JsFunction::User { ref closure, .. }) = func_obj.borrow().callable {
                    let cls_env = closure.borrow();
                    if let Some(ref names) = cls_env.class_private_names {
                        init_env.borrow_mut().class_private_names = Some(names.clone());
                    }
                }
                // Set __home_object__ for super property access in field initializers.
                let func_obj_id = func_obj.borrow().id.unwrap();
                let proto_val = self.get_property_on_id(func_obj_id, "prototype");
                if proto_val.is_object() {
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
                        if let Some(obj) = self.get_object_cell(new_obj_id) {
                            if !obj.borrow().extensible {
                                return Completion::Throw(self.create_type_error(
                                    "Cannot define private method on non-extensible object",
                                ));
                            }
                            if obj.borrow().private_fields.contains_key(name) {
                                return Completion::Throw(self.create_type_error(
                                    "Cannot add private method to object twice",
                                ));
                            }
                            obj.borrow_mut()
                                .private_fields
                                .insert(name.clone(), PrivateElement::Method(value.clone()));
                        }
                    }
                    InstanceFieldDef::Private(PrivateFieldDef::Accessor { name, get, set }) => {
                        if let Some(obj) = self.get_object_cell(new_obj_id) {
                            if !obj.borrow().extensible {
                                return Completion::Throw(self.create_type_error(
                                    "Cannot define private accessor on non-extensible object",
                                ));
                            }
                            if obj.borrow().private_fields.contains_key(name) {
                                return Completion::Throw(self.create_type_error(
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
                        let source_name = name.split('#').next().unwrap_or(name);
                        let display_name = format!("#{source_name}");
                        let val = if let Some(init) = initializer {
                            match self.eval_expr(init, &init_env) {
                                Completion::Normal(v) => {
                                    if init.is_anonymous_function_definition() {
                                        self.set_function_name(&v, &display_name);
                                    }
                                    v
                                }
                                other => return other,
                            }
                        } else {
                            JsValue::UNDEFINED
                        };
                        if let Some(obj) = self.get_object_cell(new_obj_id) {
                            if !obj.borrow().extensible {
                                return Completion::Throw(self.create_type_error(
                                    "Cannot define private field on non-extensible object",
                                ));
                            }
                            if obj.borrow().private_fields.contains_key(name) {
                                return Completion::Throw(self.create_type_error(
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
                                Completion::Normal(v) => {
                                    if init.is_anonymous_function_definition() {
                                        self.set_function_name(&v, key);
                                    }
                                    v
                                }
                                other => return other,
                            }
                        } else {
                            JsValue::UNDEFINED
                        };
                        match crate::interpreter::builtins::array::create_data_property_or_throw(
                            self, &this_val, key, val,
                        ) {
                            Ok(()) => {}
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    InstanceFieldDef::AutoAccessorStorage(slot_name, initializer) => {
                        let val = if let Some(init) = initializer {
                            match self.eval_expr(init, &init_env) {
                                Completion::Normal(v) => v,
                                other => return other,
                            }
                        } else {
                            JsValue::UNDEFINED
                        };
                        if let Some(obj) = self.get_object_cell(new_obj_id) {
                            obj.borrow_mut()
                                .private_fields
                                .insert(slot_name.clone(), PrivateElement::Field(val));
                        }
                    }
                    _ => {} // Methods/accessors handled in pass 1
                }
            }
            let prev_new_target = self.new_target.take();
            self.new_target = Some(callee_val.clone());
            self.last_call_had_explicit_return = false;
            self.last_call_this_value = None;
            self.calling_as_construct = true;
            let (result, had_explicit_return, captured_this) =
                self.call_constructor_body(&callee_val, &this_val, &evaluated_args);
            self.gc_unroot_frame(gc_frame);
            let final_this = captured_this.unwrap_or(this_val.clone());
            self.new_target = prev_new_target;
            match result {
                Completion::Normal(v) if had_explicit_return && (v).is_object() => {
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
        let co = if let Some(co) = (constructor)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
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
                    if (v).is_object() {
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
        if let Some(func_obj) = self.get_object_cell(co.id) {
            let b = func_obj.borrow();
            if let Some(bd) = b.bound() {
                let target = bd.target.clone();
                let ba = bd.args.clone();
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
        if let Some(func_obj) = self.get_object_cell(co.id) {
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

        let is_derived = if let Some(func_obj) = self.get_object_cell(co.id) {
            func_obj.borrow().is_derived_class_constructor()
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
            self.calling_as_construct = true;
            let (result, had_explicit_return, final_this) =
                self.call_constructor_body(constructor, &JsValue::UNDEFINED, args);
            self.constructing_derived = prev_constructing_derived;
            self.new_target = prev_new_target;
            match result {
                Completion::Normal(v) if had_explicit_return && (v).is_object() => {
                    Completion::Normal(v)
                }
                Completion::Normal(ref v) if had_explicit_return && !(v).is_undefined() => {
                    Completion::Throw(self.create_type_error(
                        "Derived constructors may only return object or undefined",
                    ))
                }
                Completion::Normal(_) | Completion::Empty => {
                    match final_this {
                        Some(v) if (v).is_object() => Completion::Normal(v),
                        Some(v) if !(v).is_undefined() => Completion::Normal(v),
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
            // Constructors with deferred_construct skip the early prototype access
            // to let their body run pre-construction checks first (e.g., Promise checks
            // executor callable before OrdinaryCreateFromConstructor).
            let deferred = if let Some(func_obj) = self.get_object_cell(co.id) {
                func_obj.borrow().deferred_construct
            } else {
                false
            };

            let (this_val, new_obj_id) = if deferred {
                (JsValue::UNDEFINED, 0)
            } else {
                let new_obj_id = self.create_object_id();
                // Use new_target's .prototype for the new object's [[Prototype]]
                // Must use get_object_property to invoke proxy get traps
                if let Some(nt_o) = new_target
                    .as_object_id()
                    .map(|id| crate::types::JsObject { id })
                {
                    let nt_val = new_target.clone();
                    let proto = match self.get_object_property(nt_o.id, "prototype", &nt_val) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => JsValue::UNDEFINED,
                    };
                    if let Some(proto_obj) = (proto)
                        .as_object_id()
                        .map(|id| crate::types::JsObject { id })
                    {
                        self.get_object_cell_expect(new_obj_id)
                            .borrow_mut()
                            .prototype_id = Some(proto_obj.id);
                    } else {
                        // proto is not an Object: GetFunctionRealm(newTarget) → realm's %ObjectPrototype%
                        let nt_realm_id = match self.get_function_realm(&JsValue::object(nt_o.id)) {
                            Ok(r) => r,
                            Err(e) => return Completion::Throw(e),
                        };
                        let op_id = self.realms[nt_realm_id].object_prototype;
                        if let Some(proto_rc) = op_id {
                            self.get_object_cell_expect(new_obj_id)
                                .borrow_mut()
                                .prototype_id = Some(proto_rc);
                        }
                    }
                }
                let id = new_obj_id;
                (JsValue::object(id), id)
            };

            // Initialize instance fields from the constructor's class_instance_field_defs.
            let instance_field_defs = if let Some(co) = (constructor)
                .as_object_id()
                .map(|id| crate::types::JsObject { id })
                && let Some(func_obj) = self.get_object_cell(co.id)
            {
                func_obj.borrow().class_instance_field_defs.clone()
            } else {
                Vec::new()
            };
            if !instance_field_defs.is_empty() {
                let (class_pn, proto_val, outer_env) = if let Some(co) = (constructor)
                    .as_object_id()
                    .map(|id| crate::types::JsObject { id })
                    && let Some(func_obj) = self.get_object_cell(co.id)
                {
                    let (pn, oe) = if let Some(JsFunction::User { ref closure, .. }) =
                        func_obj.borrow().callable
                    {
                        let cls_env = closure.borrow();
                        (cls_env.class_private_names.clone(), cls_env.parent.clone())
                    } else {
                        (None, None)
                    };
                    let func_obj_id = func_obj.borrow().id.unwrap();
                    let pv = self.get_property_on_id(func_obj_id, "prototype");
                    (pn, pv, oe)
                } else {
                    (None, JsValue::UNDEFINED, None)
                };
                let init_parent =
                    outer_env.unwrap_or_else(|| Environment::new_function_scope(None));
                let init_env = Environment::new(Some(init_parent));
                init_env.borrow_mut().declare("this", BindingKind::Const);
                init_env
                    .borrow_mut()
                    .initialize_binding("this", this_val.clone());
                init_env.borrow_mut().is_field_initializer = true;
                init_env.borrow_mut().class_private_names = class_pn;
                if proto_val.is_object() {
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
                            if let Some(obj) = self.get_object_cell(new_obj_id) {
                                obj.borrow_mut()
                                    .private_fields
                                    .insert(name.clone(), PrivateElement::Method(value.clone()));
                            }
                        }
                        InstanceFieldDef::Private(PrivateFieldDef::Accessor { name, get, set }) => {
                            if let Some(obj) = self.get_object_cell(new_obj_id) {
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
                            let source_name = name.split('#').next().unwrap_or(name);
                            let display_name = format!("#{source_name}");
                            let val = if let Some(init) = initializer {
                                match self.eval_expr(init, &init_env) {
                                    Completion::Normal(v) => {
                                        if init.is_anonymous_function_definition() {
                                            self.set_function_name(&v, &display_name);
                                        }
                                        v
                                    }
                                    other => return other,
                                }
                            } else {
                                JsValue::UNDEFINED
                            };
                            if let Some(obj) = self.get_object_cell(new_obj_id) {
                                obj.borrow_mut()
                                    .private_fields
                                    .insert(name.clone(), PrivateElement::Field(val));
                            }
                        }
                        InstanceFieldDef::Public(key, initializer) => {
                            let val = if let Some(init) = initializer {
                                match self.eval_expr(init, &init_env) {
                                    Completion::Normal(v) => {
                                        if init.is_anonymous_function_definition() {
                                            self.set_function_name(&v, key);
                                        }
                                        v
                                    }
                                    other => return other,
                                }
                            } else {
                                JsValue::UNDEFINED
                            };
                            if let Some(obj) = self.get_object_cell(new_obj_id) {
                                obj.borrow_mut().insert_value(key.clone(), val);
                            }
                        }
                        InstanceFieldDef::AutoAccessorStorage(slot_name, initializer) => {
                            let val = if let Some(init) = initializer {
                                match self.eval_expr(init, &init_env) {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                }
                            } else {
                                JsValue::UNDEFINED
                            };
                            if let Some(obj) = self.get_object_cell(new_obj_id) {
                                obj.borrow_mut()
                                    .private_fields
                                    .insert(slot_name.clone(), PrivateElement::Field(val));
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
            self.calling_as_construct = true;
            let (result, had_explicit_return, captured_this) =
                self.call_constructor_body(constructor, &this_val, args);
            let final_this = captured_this.unwrap_or(this_val.clone());
            self.new_target = prev_new_target;
            match result {
                Completion::Normal(v) if had_explicit_return && (v).is_object() => {
                    Completion::Normal(v)
                }
                Completion::Normal(_) | Completion::Empty => Completion::Normal(final_this),
                other => other,
            }
        }
    }

    // GetPrototypeFromConstructor: if new_target differs from intrinsic default,
    // set obj's prototype to new_target.prototype (using getter-aware property access).
    // When new_target.prototype is not an Object, falls back to GetFunctionRealm(newTarget)'s
    // intrinsic determined by `realm_fallback`.
    pub(crate) fn apply_new_target_prototype<F>(
        &mut self,
        obj_id: u64,
        default_proto_id: Option<u64>,
        realm_fallback: F,
    ) where
        F: Fn(&crate::interpreter::types::Realm) -> Option<u64>,
    {
        if let Some(ref nt) = self.new_target.clone()
            && let Some(nt_o) = (nt).as_object_id().map(|id| crate::types::JsObject { id })
        {
            let nt_proto_id = if let Some(nt_obj) = self.get_object_cell(nt_o.id) {
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
                if let Some(po) = (proto_val)
                    .as_object_id()
                    .map(|id| crate::types::JsObject { id })
                    && let Some(obj_rc) = self.get_object_cell(obj_id)
                {
                    obj_rc.borrow_mut().prototype_id = Some(po.id);
                } else {
                    // proto is not an Object: GetFunctionRealm(newTarget) → realm's intrinsic
                    let nt_realm_id = match self.get_function_realm(&JsValue::object(nt_o.id)) {
                        Ok(r) => r,
                        Err(_) => return,
                    };
                    let fallback_id = realm_fallback(&self.realms[nt_realm_id]);
                    if let Some(proto_rc) = fallback_id
                        && let Some(obj_rc) = self.get_object_cell(obj_id)
                    {
                        obj_rc.borrow_mut().prototype_id = Some(proto_rc);
                    }
                }
            }
        }
    }

    pub(crate) fn get_proxy_info(&self, obj_id: u64) -> Option<(bool, Option<u64>, Option<u64>)> {
        let obj = self.get_object_cell(obj_id)?;
        let b = obj.borrow();
        let p = b.proxy()?;
        Some((p.revoked, p.target_id, p.handler_id))
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
                let handler_val = JsValue::object(handler_id);
                let trap_val = match self.get_object_property(handler_id, trap_name, &handler_val) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::UNDEFINED,
                };
                if (trap_val).is_nullish() {
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
                    _ => Ok(Some(JsValue::UNDEFINED)),
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
        if let Some(obj) = self.get_object_cell(proxy_id)
            && let Some(tid) = obj.borrow().proxy_target_id()
        {
            return JsValue::object(tid);
        }
        JsValue::UNDEFINED
    }

    pub(crate) fn validate_ownkeys_invariant(
        &mut self,
        trap_result: &JsValue,
        target_val: &JsValue,
    ) -> Result<(), JsValue> {
        const MAX_PROXY_OWNKEYS_RESULT_LEN: usize = 1_000_000;

        let trap_keys: Vec<JsPropertyKey> = if let Some(arr) = (trap_result)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            let len_value = self.get_property_on_id(arr.id, "length");
            let len = match len_value.as_number() {
                Some(n) if n.is_finite() && n > 0.0 => {
                    let len = n.floor() as usize;
                    if len > MAX_PROXY_OWNKEYS_RESULT_LEN {
                        return Err(self.create_type_error(
                            "'ownKeys' on proxy: trap result length exceeds supported limit",
                        ));
                    }
                    len
                }
                _ => 0,
            };
            let arr_id = arr.id;
            (0..len)
                .map(|i| {
                    let v = self.get_property_on_id(arr_id, &i.to_string());
                    to_property_key_string(&v)
                })
                .collect()
        } else {
            return Ok(());
        };

        if let Some(t) = (target_val)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
            && let Some(tobj) = self.get_object_cell(t.id)
        {
            let target_extensible = tobj.borrow().extensible;
            let (target_nonconfig, target_config): (Vec<JsPropertyKey>, Vec<JsPropertyKey>) = {
                let b = tobj.borrow();
                let nc: Vec<JsPropertyKey> = b
                    .property_order
                    .iter()
                    .filter(|k| {
                        b.properties
                            .get(k)
                            .is_some_and(|d| d.configurable == Some(false))
                    })
                    .cloned()
                    .collect();
                let c: Vec<JsPropertyKey> = b
                    .property_order
                    .iter()
                    .filter(|k| {
                        b.properties
                            .get(k)
                            .is_some_and(|d| d.configurable != Some(false))
                    })
                    .cloned()
                    .collect();
                (nc, c)
            };
            let trap_set: HashSet<&[u8]> = trap_keys.iter().map(|s| s.as_bytes()).collect();

            for key in &target_nonconfig {
                if !trap_set.contains(key.as_bytes()) {
                    return Err(self.create_type_error(
                        "'ownKeys' on proxy: trap result did not include all non-configurable own keys of the proxy target",
                    ));
                }
            }

            if !target_extensible {
                let target_keys: HashSet<&[u8]> = target_nonconfig
                    .iter()
                    .chain(target_config.iter())
                    .map(|s| s.as_bytes())
                    .collect();
                for key in &trap_keys {
                    if !target_keys.contains(key.as_bytes()) {
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
        if !(right).is_object() {
            return Completion::Throw(
                self.create_type_error("Right-hand side of instanceof is not an object"),
            );
        }
        let rhs_obj = crate::types::JsObject {
            id: right.as_object_id().expect("object checked"),
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
            if !(method).is_nullish() {
                if !self.is_callable(&method) {
                    return Completion::Throw(
                        self.create_type_error("@@hasInstance is not callable"),
                    );
                }
                let result = self.call_function(&method, right, std::slice::from_ref(left));
                return match result {
                    Completion::Normal(v) => {
                        Completion::Normal(JsValue::boolean(self.to_boolean_val(&v)))
                    }
                    other => other,
                };
            }
        }
        if !self.is_callable(right) {
            return Completion::Throw(
                self.create_type_error("Right-hand side of instanceof is not callable"),
            );
        }
        self.ordinary_has_instance(right, left)
    }

    pub(crate) fn ordinary_has_instance(&mut self, ctor: &JsValue, obj: &JsValue) -> Completion {
        // Step 2: bound function → recurse with target
        if let Some(co) = (ctor)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
            && let Some(obj_data) = self.get_object(co.id)
            && let Some(target) = obj_data.borrow().bound().map(|b| b.target.clone())
        {
            return self.eval_instanceof(obj, &target);
        }
        if !self.is_callable(ctor) {
            return Completion::Normal(JsValue::boolean(false));
        }
        // Step 3: If Type(O) is not Object, return false
        let Some(lhs) = (obj).as_object_id().map(|id| crate::types::JsObject { id }) else {
            return Completion::Normal(JsValue::boolean(false));
        };
        let Some(_inst_obj) = self.get_object_cell(lhs.id) else {
            return Completion::Normal(JsValue::boolean(false));
        };
        let ctor_obj_ref = match ctor.as_object_id() {
            Some(id) => crate::types::JsObject { id },
            None => return Completion::Normal(JsValue::boolean(false)),
        };
        // Step 4: Let P be Get(C, "prototype")
        let proto_val = match self.get_object_property(ctor_obj_ref.id, "prototype", ctor) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Completion::Throw(e),
            _ => JsValue::UNDEFINED,
        };
        // Step 5: If P is not Object, throw TypeError
        let Some(_proto_ref) = proto_val
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        else {
            return Completion::Throw(
                self.create_type_error("Function has non-object prototype in instanceof check"),
            );
        };
        // Step 6: Walk O.[[GetPrototypeOf]]() chain (proxy-aware)
        let mut current_val = obj.clone();
        while let Some(current_obj) = current_val
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            let current_id = current_obj.id;
            let next = match self.proxy_get_prototype_of(current_id) {
                Ok(v) => v,
                Err(e) => return Completion::Throw(e),
            };
            if (next).is_null() {
                return Completion::Normal(JsValue::boolean(false));
            }
            if same_value(&next, &proto_val) {
                return Completion::Normal(JsValue::boolean(true));
            }
            current_val = next;
        }
        Completion::Normal(JsValue::boolean(false))
    }

    /// Resolve an identifier to a reference (for capturing before RHS evaluation).
    pub(super) fn resolve_identifier_ref(
        &mut self,
        name: &str,
        env: &EnvRef,
    ) -> Result<IdentifierRef, JsValue> {
        if (self.with_scope_depth > 0 || self.has_ever_entered_with)
            && let Some(obj_id) = self.resolve_with_has_binding(name, env)?
        {
            return Ok(IdentifierRef::WithObject(obj_id));
        }
        if let Some(specific_env) = Environment::find_binding_env(env, name) {
            let (has_binding, global_obj_id) = {
                let e = specific_env.borrow();
                let in_bindings = e.bindings.contains_key(name) || e.is_indirect_binding(name);
                if in_bindings {
                    (true, None)
                } else if let Some(gid) = e.global_object_id {
                    let own = self
                        .get_object_cell(gid)
                        .is_some_and(|g| g.borrow().properties.contains_key(name));
                    (own, if !own { Some(gid) } else { None })
                } else {
                    (false, None)
                }
            };
            if has_binding {
                Ok(IdentifierRef::SpecificEnv(specific_env))
            } else if let Some(gid) = global_obj_id {
                // Check prototype chain (handles Proxy has traps)
                if self.proxy_has_property(gid, name)? {
                    Ok(IdentifierRef::SpecificEnv(specific_env))
                } else {
                    Ok(IdentifierRef::Unresolvable)
                }
            } else {
                Ok(IdentifierRef::Unresolvable)
            }
        } else {
            Ok(IdentifierRef::Unresolvable)
        }
    }

    /// PutValue for unresolvable reference in sloppy mode (§6.2.5.6):
    /// Set property on the global object.
    fn set_global_implicit(&mut self, name: &str, value: JsValue) -> Completion {
        let global_env = self.realm().global_env.clone();
        if !global_env.borrow().bindings.contains_key(name) {
            global_env
                .borrow_mut()
                .declare_deletable(name, BindingKind::Var);
        }
        match self.env_set(&global_env, name, value.clone()) {
            Ok(()) => Completion::Normal(value),
            Err(_) => Completion::Throw(self.create_type_error("Assignment to constant variable.")),
        }
    }

    /// Whether `obj_id` is some realm's global object — the only case in which
    /// a property write has a global environment binding to mirror.
    fn is_realm_global_object(&self, obj_id: u64) -> bool {
        self.realms
            .iter()
            .any(|realm| realm.global_object == Some(obj_id))
    }

    /// Sync a property set on an object to the corresponding global env binding,
    /// if the object is a realm's global object.
    fn sync_global_object_binding(&mut self, obj_id: u64, key: &str, value: &JsValue) {
        let env = self.realms.iter().find_map(|realm| {
            (realm.global_object == Some(obj_id)).then(|| realm.global_env.clone())
        });
        if let Some(env) = env
            && env.borrow().bindings.contains_key(key)
        {
            let _ = self.env_set(&env, key, value.clone());
        }
    }

    /// Write a value through a captured identifier reference.
    pub(super) fn put_value_by_ref(
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
            IdentifierRef::SpecificEnv(specific_env) => {
                match self.env_check_set_binding(specific_env, name) {
                    SetBindingCheck::TdzError => Completion::Throw(self.create_reference_error(
                        &format!("Cannot access '{}' before initialization", name),
                    )),
                    SetBindingCheck::ConstAssign => Completion::Throw(
                        self.create_type_error("Assignment to constant variable."),
                    ),
                    SetBindingCheck::FunctionNameAssign => {
                        if specific_env.borrow().strict || env.borrow().strict {
                            Completion::Throw(
                                self.create_type_error("Assignment to constant variable."),
                            )
                        } else {
                            Completion::Normal(value)
                        }
                    }
                    SetBindingCheck::Unresolvable => {
                        let strict = env.borrow().strict || specific_env.borrow().strict;
                        let global_obj_id = specific_env.borrow().global_object_id;
                        if let Some(gid) = global_obj_id {
                            // §9.1.1.2.5 SetMutableBinding: check HasProperty
                            let still_exists = self.proxy_has_property(gid, name);
                            match still_exists {
                                Ok(false) if strict => {
                                    return Completion::Throw(self.create_reference_error(
                                        &format!("{name} is not defined"),
                                    ));
                                }
                                Err(e) => return Completion::Throw(e),
                                _ => {}
                            }
                            let receiver = JsValue::object(gid);
                            match self.proxy_set(gid, name, value.clone(), &receiver) {
                                Ok(_) => Completion::Normal(value),
                                Err(e) => Completion::Throw(e),
                            }
                        } else if strict {
                            Completion::Throw(
                                self.create_reference_error(&format!("{name} is not defined")),
                            )
                        } else {
                            self.set_global_implicit(name, value)
                        }
                    }
                    SetBindingCheck::Ok => {
                        // If binding is not in env.bindings but found via global object's
                        // has_property (prototype chain), use [[Set]] to respect setters/proxies
                        let in_bindings = specific_env.borrow().bindings.contains_key(name);
                        if !in_bindings {
                            let global_obj_id = specific_env.borrow().global_object_id;
                            if let Some(gid) = global_obj_id {
                                // §9.1.1.2.5 SetMutableBinding: check HasProperty again
                                let still_exists = self.proxy_has_property(gid, name);
                                match still_exists {
                                    Ok(false) => {
                                        let strict =
                                            env.borrow().strict || specific_env.borrow().strict;
                                        if strict {
                                            return Completion::Throw(self.create_reference_error(
                                                &format!("{name} is not defined"),
                                            ));
                                        }
                                    }
                                    Err(e) => return Completion::Throw(e),
                                    Ok(true) => {}
                                }
                                let receiver = JsValue::object(gid);
                                match self.proxy_set(gid, name, value.clone(), &receiver) {
                                    Ok(_) => {
                                        self.sync_global_object_binding(gid, name, &value);
                                        return Completion::Normal(value);
                                    }
                                    Err(e) => return Completion::Throw(e),
                                }
                            }
                        }
                        match self.env_set(specific_env, name, value.clone()) {
                            Ok(()) => Completion::Normal(value),
                            Err(_) => Completion::Throw(
                                self.create_type_error("Assignment to constant variable."),
                            ),
                        }
                    }
                }
            }
            IdentifierRef::Unresolvable => {
                // Reference was unresolvable at resolve time — per §6.2.5.6 PutValue step 3
                if env.borrow().strict {
                    Completion::Throw(
                        self.create_reference_error(&format!("{name} is not defined")),
                    )
                } else {
                    self.set_global_implicit(name, value)
                }
            }
        }
    }

    /// Single-pass identifier resolution: combines with-scope check, binding lookup,
    /// and global getter resolution into one scope chain walk.
    pub(super) fn resolve_identifier(
        &mut self,
        name: &str,
        env: &EnvRef,
        strict: bool,
    ) -> Completion {
        let mut current = Some(env.clone());
        while let Some(env_ref) = current {
            let env_borrow = env_ref.borrow();

            // 1. Check with-object at this scope level
            if let Some(ref with) = env_borrow.with_object {
                let obj_id = with.obj_id;
                drop(env_borrow);
                match self.proxy_has_property(obj_id, name) {
                    Ok(true) => {
                        match self.check_unscopables_dynamic(obj_id, name) {
                            Ok(true) => {
                                // Unscopable — skip this with scope, continue walking
                            }
                            Ok(false) => {
                                self.last_identifier_with_base = Some(obj_id);
                                return self.with_get_binding_value(obj_id, name, strict);
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    Ok(false) => {}
                    Err(e) => return Completion::Throw(e),
                }
                current = env_ref.borrow().parent.clone();
                continue;
            }

            // 2. Check indirect bindings (module imports)
            if let Some(resolved) = env_borrow.resolve_indirect_binding(name) {
                return match resolved {
                    Some(val) => Completion::Normal(val),
                    None => {
                        // TDZ for indirect binding
                        drop(env_borrow);
                        Completion::Throw(self.create_reference_error(&format!(
                            "Cannot access '{name}' before initialization"
                        )))
                    }
                };
            }

            // 3. Check local bindings
            if let Some(binding) = env_borrow.bindings.get(name) {
                if !binding.initialized {
                    drop(env_borrow);
                    return Completion::Throw(self.create_reference_error(&format!(
                        "Cannot access '{name}' before initialization"
                    )));
                }
                return Completion::Normal(binding.value.clone());
            }

            // 4. Check global object (at the bottom of the scope chain)
            if let Some(gid) = env_borrow.global_object_id {
                drop(env_borrow);
                {
                    // Check own property first
                    let own_prop = self
                        .get_object_cell(gid)
                        .and_then(|o| o.borrow().get_own_property(name));
                    if let Some(ref desc) = own_prop {
                        if desc.get.is_some() {
                            let this_val = JsValue::object(gid);
                            return self.get_object_property(gid, name, &this_val);
                        }
                        return Completion::Normal(
                            desc.value.clone().unwrap_or(JsValue::UNDEFINED),
                        );
                    }
                    // Check prototype chain
                    match self.proxy_has_property(gid, name) {
                        Ok(true) => {
                            let this_val = JsValue::object(gid);
                            return self.get_object_property(gid, name, &this_val);
                        }
                        Ok(false) => {}
                        Err(e) => return Completion::Throw(e),
                    }
                }
                // Not found on global object either
                let err = self.create_reference_error(&format!("{name} is not defined"));
                return Completion::Throw(err);
            }

            current = env_borrow.parent.clone();
        }

        Completion::Throw(self.create_reference_error(&format!("{name} is not defined")))
    }

    /// Resolve a global object property for reading, walking the prototype chain.
    /// Returns Some(Completion) if the name resolves to a property on the global object
    /// or its prototype chain (including through Proxy has/get traps).
    /// Returns None if no property found — caller should use env.get().
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
            if let Some(gid) = env_borrow.global_object_id {
                drop(env_borrow);
                // Check own property first (fast path)
                let own_prop = self
                    .get_object_cell(gid)
                    .and_then(|o| o.borrow().get_own_property(name));
                if let Some(ref desc) = own_prop {
                    if desc.get.is_some() {
                        let this_val = JsValue::object(gid);
                        return Some(self.get_object_property(gid, name, &this_val));
                    }
                    // Data property — slim Environment::get no longer falls
                    // through to the global object, so return the value here.
                    return Some(Completion::Normal(
                        desc.value.clone().unwrap_or(JsValue::UNDEFINED),
                    ));
                }
                // Not own — check prototype chain (handles Proxy has/get traps)
                match self.proxy_has_property(gid, name) {
                    Ok(true) => {
                        let this_val = JsValue::object(gid);
                        return Some(self.get_object_property(gid, name, &this_val));
                    }
                    Ok(false) => return None,
                    Err(e) => return Some(Completion::Throw(e)),
                }
            }
            current = env_borrow.parent.clone();
        }
        None
    }

    /// Get the super base object ID from __home_object__.__proto__ in the given env.
    /// Returns Ok(Some(id)) for a valid super base, Ok(None) for null prototype, or
    /// falls back to __super__.prototype.
    fn get_super_base_id(&self, env: &EnvRef) -> Option<u64> {
        let home = env.borrow().get("__home_object__");
        if let Some(home_id) = home.as_ref().and_then(JsValue::as_object_id)
            && let Some(home_obj) = self.get_object_cell(home_id)
        {
            return home_obj.borrow().prototype_id;
        }
        // Fallback: __super__.prototype_id
        let obj_val = env.borrow().get("__super__").unwrap_or(JsValue::UNDEFINED);
        if let Some(o) = (obj_val)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            let proto_val = self.get_property_on_id(o.id, "prototype");
            if let Some(p) = (proto_val)
                .as_object_id()
                .map(|id| crate::types::JsObject { id })
            {
                return Some(p.id);
            }
        }
        None
    }

    /// PutValue for a Super Reference, whose [[Set]] holder and receiver differ.
    fn super_set_property<K: PropertyKeyLike + ?Sized>(
        &mut self,
        base_id: u64,
        key: &K,
        val: JsValue,
        receiver: &JsValue,
        strict: bool,
    ) -> Completion {
        match self.put_value_to_property(base_id, key, val.clone(), receiver, strict) {
            Ok(_) => Completion::Normal(val),
            Err(e) => Completion::Throw(e),
        }
    }

    fn call_async_function(
        &mut self,
        params: &[Pattern],
        body: &Body,
        closure: EnvRef,
        is_arrow: bool,
        is_strict: bool,
        this_val: &JsValue,
        args: &[JsValue],
        func_val: &JsValue,
        uses_arguments: bool,
        has_simple_params: bool,
    ) -> Completion {
        let gc_frame = self.gc_root_frame();
        let promise = self.create_promise_object();
        let promise_id = if let Some(o) = (promise)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            o.id
        } else {
            0
        };
        self.gc_root_value(&promise);
        let (resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);
        self.gc_root_value(&resolve_fn);
        self.gc_root_value(&reject_fn);

        let closure_strict = closure.borrow().strict;
        let func_env = Environment::new_function_scope_with_capacity(
            Some(closure),
            params.len().saturating_add(2),
        );
        if is_arrow {
            func_env.borrow_mut().is_arrow_scope = true;
        }
        // Set up `this` and `arguments` before binding parameters so that
        // default parameter expressions can reference `arguments`.
        if !is_arrow {
            let effective_this = if !is_strict && !closure_strict {
                if (this_val).is_nullish() {
                    self.realm()
                        .global_env
                        .borrow()
                        .get("this")
                        .unwrap_or(this_val.clone())
                } else if !(this_val).is_object() {
                    match self.to_object(this_val) {
                        Completion::Normal(v) => v,
                        _ => this_val.clone(),
                    }
                } else {
                    this_val.clone()
                }
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
            if uses_arguments {
                let is_simple = has_simple_params;
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
                let _ = self.env_set(&func_env, "arguments", arguments_obj);
                if is_strict || !is_simple {
                    func_env.borrow_mut().arguments_immutable = true;
                }
            } else {
                func_env.borrow_mut().declare("arguments", BindingKind::Var);
            }
        }
        {
            let is_simple_p = has_simple_params;
            if !is_simple_p {
                func_env.borrow_mut().has_parameter_expressions = true;
            }
        }
        if let Err(error) =
            self.bind_function_parameters(params, args, &func_env, has_simple_params)
        {
            let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[error]);
            self.drain_microtasks();
            self.gc_unroot_frame(gc_frame);
            // A default-param expression may have called `__host_exit`
            // (issue #229): return abrupt so the caller unwinds.
            if self.pending_exit.is_some() {
                return Completion::Throw(JsValue::UNDEFINED);
            }
            return Completion::Normal(promise);
        }

        func_env.borrow_mut().strict = is_strict;
        self.in_tail_position = false;

        let sm = Rc::new(
            crate::interpreter::generator_transform::transform_async_function(
                body.as_slice(),
                params,
            ),
        );

        for tv in &sm.temp_vars {
            func_env.borrow_mut().declare(tv, BindingKind::Var);
        }
        for lv in &sm.local_vars {
            if matches!(
                lv.kind,
                VarKind::Let | VarKind::Const | VarKind::Using | VarKind::AwaitUsing
            ) && lv.scope_depth > 0
            {
                // Nested lexical bindings are created by their transformed
                // runtime scopes and must not leak into the function scope.
                continue;
            }
            if !func_env.borrow().bindings.contains_key(&lv.name) {
                func_env.borrow_mut().declare(&lv.name, BindingKind::Var);
            }
        }

        let async_id = self.scheduler.alloc_async_function_id();

        self.scheduler.insert_async_function_state(
            async_id,
            AsyncFunctionState {
                state_machine: sm,
                func_env,
                is_strict,
                current_state: 0,
                try_stack: vec![],
                pending_binding: None,
                pending_return: None,
                pending_loop_control: None,
                saved_finally_exception: None,
                pending_for_of_unwind: None,
                resolve_fn,
                reject_fn,
                for_of_stack: vec![],
                module_path: None,
            },
        );

        let resume = self.async_function_resume(async_id, JsValue::UNDEFINED, false);

        self.gc_unroot_frame(gc_frame);
        // If the body ran `__host_exit` synchronously (before any await, issue
        // #242), propagate the exit as this call's completion — in expression
        // position too (`f(), g()`; `h(f())`), which a statement-level check
        // cannot reach — instead of returning the never-to-settle promise.
        if let Completion::Exit(code) = resume {
            return Completion::Exit(code);
        }
        Completion::Normal(promise)
    }

    /// Drives an async function's state machine. Returns `Completion::Exit`
    /// (issue #242) if the body called `__host_exit` — the caller must
    /// propagate it rather than settling the result promise — and
    /// `Completion::Normal(undefined)` otherwise (the promise was settled or
    /// the machine suspended at an await).
    pub(crate) fn async_function_resume(
        &mut self,
        async_id: u64,
        sent_value: JsValue,
        is_error: bool,
    ) -> Completion {
        use crate::interpreter::generator_transform::{SentValueBindingKind, StateTerminator};

        let Some(state) = self.scheduler.remove_async_function_state(async_id) else {
            return Completion::Normal(JsValue::UNDEFINED);
        };

        let AsyncFunctionState {
            state_machine,
            func_env,
            is_strict,
            current_state,
            mut try_stack,
            pending_binding,
            pending_return: saved_pending_return,
            pending_loop_control: restored_pending_loop_control,
            saved_finally_exception: restored_saved_finally_exception,
            pending_for_of_unwind: restored_pending_for_of_unwind,
            resolve_fn,
            reject_fn,
            for_of_stack: saved_for_of_stack,
            module_path: async_module_path,
        } = state;

        if let Some(ref mp) = async_module_path {
            self.current_module_path = Some(mp.clone());
        }

        // §14.7.5.6 step 6.b: `Await(nextResult)` rejecting sets the iterator
        // record's [[Done]] and returns without performing IteratorClose. The
        // `<iter>__await` temp is only ever bound by a `for await` head, so a
        // rejection resumed into it identifies that loop's protocol failure.
        let mut for_of_protocol_failure: Option<String> = None;
        if is_error
            && let Some(ref binding) = pending_binding
            && let SentValueBindingKind::Variable(name) = &binding.kind
            && let Some(iter_var) = name.strip_suffix("__await")
            && saved_for_of_stack
                .iter()
                .any(|loop_state| loop_state.iter_var == iter_var)
        {
            for_of_protocol_failure = Some(iter_var.to_string());
        }

        if let Some(binding) = pending_binding {
            match &binding.kind {
                SentValueBindingKind::Variable(name) => {
                    let mut env = func_env.borrow_mut();
                    let needs_init = env
                        .bindings
                        .get(name.as_str())
                        .is_some_and(|b| !b.initialized);
                    if needs_init {
                        env.initialize_binding(name, sent_value.clone());
                    } else {
                        env.set(name, sent_value.clone()).ok();
                    }
                }
                SentValueBindingKind::Pattern(pattern) => {
                    let _ =
                        self.bind_pattern(pattern, sent_value.clone(), BindingKind::Var, &func_env);
                }
                SentValueBindingKind::Discard | SentValueBindingKind::InlineYield { .. } => {}
            }
        }

        // If the sent_value is an error (from a rejected promise), route through try stack
        let mut pending_exception: Option<JsValue> = if is_error { Some(sent_value) } else { None };

        // Re-insert state so GC can trace it during execution
        self.scheduler.insert_async_function_state(
            async_id,
            AsyncFunctionState {
                state_machine: state_machine.clone(),
                func_env: func_env.clone(),
                is_strict,
                current_state,
                try_stack: try_stack.clone(),
                pending_binding: None,
                pending_return: None,
                pending_loop_control: restored_pending_loop_control,
                saved_finally_exception: None,
                pending_for_of_unwind: restored_pending_for_of_unwind.clone(),
                resolve_fn: resolve_fn.clone(),
                reject_fn: reject_fn.clone(),
                for_of_stack: saved_for_of_stack.clone(),
                module_path: async_module_path.clone(),
            },
        );

        func_env.borrow_mut().strict = is_strict;
        let saved_in_state_machine = self.in_state_machine;
        self.in_state_machine = true;
        let mut current_id = current_state;
        let mut pending_return: Option<JsValue> = saved_pending_return;
        let mut pending_loop_control = restored_pending_loop_control;
        let mut saved_finally_exception: Option<JsValue> = restored_saved_finally_exception;
        // Stack tracking active for-of loops for break/continue/return iterator close
        let mut for_of_stack: Vec<ForOfLoopState> = saved_for_of_stack;
        // An abrupt completion may need to visit a catch/finally inside an
        // enclosing loop before that loop itself can be closed. Keep that
        // obligation across suspension until the handler completes normally.
        let mut pending_for_of_unwind = restored_pending_for_of_unwind;

        // Helper: close the for-of loops from `$from` inward, surfacing an
        // abrupt completion from a disposer or an iterator `return` method.
        macro_rules! unwind_for_of {
            ($from:expr) => {
                let unwind_from = $from;
                let mut unwind_completion = Completion::Empty;
                while for_of_stack.len() > unwind_from {
                    let Some(loop_state) = for_of_stack.pop() else {
                        break;
                    };
                    // Handlers entered inside this loop have already completed
                    // before its IteratorClose runs. Handlers surrounding the
                    // loop remain available for a close failure.
                    try_stack.truncate(loop_state.try_depth);
                    unwind_completion =
                        self.close_async_for_of_loop(loop_state, &func_env, unwind_completion);
                    match &unwind_completion {
                        Completion::Exit(code) => {
                            self.scheduler.remove_async_function_state(async_id);
                            return Completion::Exit(*code);
                        }
                        Completion::Throw(_) => {
                            let handler_depth =
                                try_stack
                                    .iter()
                                    .enumerate()
                                    .rev()
                                    .find_map(|(depth, handler)| {
                                        if !handler.entered_catch
                                            && !handler.entered_finally
                                            && handler.catch_state.is_some()
                                        {
                                            Some(depth)
                                        } else if !handler.entered_finally
                                            && handler.finally_state.is_some()
                                        {
                                            Some(depth)
                                        } else {
                                            None
                                        }
                                    });
                            let reached_unwind_boundary = for_of_stack.len() == unwind_from;
                            let handler_precedes_next_loop = !reached_unwind_boundary
                                && for_of_stack.last().is_some_and(|next_loop| {
                                    handler_depth.is_some_and(|depth| depth >= next_loop.try_depth)
                                });
                            if reached_unwind_boundary || handler_precedes_next_loop {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if let Completion::Throw(error) = unwind_completion {
                    pending_for_of_unwind = Some(PendingForOfUnwind {
                        clear_at_state: None,
                    });
                    pending_exception = Some(error);
                    continue;
                }
            };
        }

        // Helper: route a return through finally blocks in try_stack
        macro_rules! route_return {
            ($val:expr) => {{
                let ret_val: JsValue = $val;
                // A return produced by a finalizer replaces any loop-control
                // completion that originally entered it.
                pending_loop_control = None;
                let mut routed_to = None;
                for i in (0..try_stack.len()).rev() {
                    if !try_stack[i].entered_finally
                        && let Some(finally_state) = try_stack[i].finally_state
                    {
                        routed_to = Some((i, finally_state));
                        break;
                    }
                }
                // §14.7.5.6: a return completion leaving a for-of closes its
                // iterator and disposes its iteration environment. Only the
                // loops nested inside the intercepting `finally` unwind now —
                // a `finally` lexically inside a loop runs first, and that
                // loop closes once the return resumes past it.
                let unwind_from = match routed_to {
                    Some((depth, _)) => for_of_stack
                        .iter()
                        .position(|loop_state| loop_state.try_depth > depth)
                        .unwrap_or(for_of_stack.len()),
                    None => 0,
                };
                unwind_for_of!(unwind_from);
                if let Some((_, finally_state)) = routed_to {
                    pending_return = Some(ret_val);
                    current_id = finally_state;
                } else {
                    let disp = self.dispose_resources(&func_env, Completion::Return(ret_val));
                    match disp {
                        Completion::Return(v) => {
                            self.scheduler.remove_async_function_state(async_id);
                            let _ = self.call_function(&resolve_fn, &JsValue::UNDEFINED, &[v]);
                            return Completion::Normal(JsValue::UNDEFINED);
                        }
                        Completion::Throw(e) => {
                            pending_exception = Some(e);
                        }
                        // A disposer that called `__host_exit` (issue #242)
                        // propagates out uncatchably rather than settling.
                        Completion::Exit(code) => {
                            self.scheduler.remove_async_function_state(async_id);
                            return Completion::Exit(code);
                        }
                        _ => {}
                    }
                }
            }};
        }

        // Helper: route an abrupt break/continue edge through every finally
        // between its source and target. The target records the lexical stacks
        // that remain active there, so iterator closing never depends on state
        // id equality.
        macro_rules! route_loop_control {
            ($target:expr) => {{
                let target = $target;

                // A loop-control completion produced by a finalizer replaces
                // the return, throw, or earlier loop-control completion that
                // originally entered it.
                pending_return = None;
                saved_finally_exception = None;
                pending_for_of_unwind = None;
                pending_loop_control = Some(target);

                let mut routed_to = None;
                for i in (target.try_depth..try_stack.len()).rev() {
                    if !try_stack[i].entered_finally
                        && let Some(finally_state) = try_stack[i].finally_state
                    {
                        routed_to = Some((i, finally_state));
                        break;
                    }
                }

                // Loops nested inside the selected finally close before it;
                // loops containing that finally remain until routing resumes.
                // Never retain more loops than the target itself retains.
                let handler_boundary = routed_to.map_or(target.for_of_depth, |(depth, _)| {
                    for_of_stack
                        .iter()
                        .position(|loop_state| loop_state.try_depth > depth)
                        .unwrap_or(for_of_stack.len())
                        .max(target.for_of_depth)
                });
                debug_assert!(handler_boundary <= for_of_stack.len());
                unwind_for_of!(handler_boundary.min(for_of_stack.len()));

                if let Some((_, finally_state)) = routed_to {
                    current_id = finally_state;
                } else {
                    pending_loop_control = None;
                    current_id = target.target_state;
                }
            }};
        }

        loop {
            // §10.4.4.3 Dispose step 3: async-dispose resources need Await(result),
            // which must truly suspend the async function. dispose_resources already
            // resolved the promise synchronously via await_value; this flag triggers
            // an additional suspension so the continuation runs in a new microtask.
            if self.pending_async_dispose_await {
                self.pending_async_dispose_await = false;
                self.async_fn_suspend_at_await(
                    async_id,
                    &state_machine,
                    &func_env,
                    is_strict,
                    current_id,
                    &try_stack,
                    None,
                    pending_return.take(),
                    pending_loop_control.take(),
                    saved_finally_exception.take(),
                    pending_for_of_unwind.take(),
                    &resolve_fn,
                    &reject_fn,
                    &JsValue::UNDEFINED,
                    &for_of_stack,
                );
                return Completion::Normal(JsValue::UNDEFINED);
            }

            if current_id >= state_machine.states.len() {
                return self.async_fn_complete(async_id, &func_env, &resolve_fn, &reject_fn);
            }
            let terminator = state_machine.states[current_id].terminator.clone();

            // Route pending exception through try stack
            // Skip routing if we're at EnterCatch/EnterFinally (already routed to handler)
            if pending_exception.is_some()
                && !matches!(
                    terminator,
                    StateTerminator::EnterCatch { .. } | StateTerminator::EnterFinally { .. }
                )
                && let Some(mut exc) = pending_exception.take()
            {
                // §14.7.5.6 steps 6.b–6.g: an abrupt completion raised by the
                // iterator protocol itself (IteratorStep, `Await(nextResult)`,
                // IteratorValue) sets [[Done]] and skips IteratorClose, so drop
                // that loop's entry without calling its `return` method.
                if let Some(failed_iter_var) = for_of_protocol_failure.take()
                    && let Some(pos) = for_of_stack
                        .iter()
                        .rposition(|loop_state| loop_state.iter_var == failed_iter_var)
                {
                    let loop_state = for_of_stack.remove(pos);
                    let iterator = func_env.borrow().get(&loop_state.iter_var);
                    if let Some(iterator) = iterator {
                        self.unroot_async_for_of_iterator(&iterator);
                    }
                }

                // A throw produced while an intervening finally was handling
                // another abrupt completion replaces that completion.
                let pending_return_was_replaced = pending_return.take().is_some();
                let pending_loop_control_was_replaced = pending_loop_control.take().is_some();
                let pending_completion_was_replaced =
                    pending_return_was_replaced || pending_loop_control_was_replaced;
                // §14.7.5.6: any abrupt body completion leaving a for-of closes
                // its iterator, so every still-active loop crossed on the way to
                // the handler unwinds — not just the ones a previous unwind
                // retained.
                let needs_for_of_unwind = !for_of_stack.is_empty();
                // Genuine throws route through the async body's catch/finally
                // handlers here. A `Completion::Exit` (issue #242) never becomes
                // a `pending_exception`, so it is not routed and cannot be
                // caught — it is handled at the body-execution site below.
                let mut handler = None;
                for i in (0..try_stack.len()).rev() {
                    if !try_stack[i].entered_catch
                        && !try_stack[i].entered_finally
                        && let Some(catch_state) = try_stack[i].catch_state
                    {
                        handler = Some((i, catch_state, true, try_stack[i]._after_state));
                        break;
                    }
                    if !try_stack[i].entered_finally
                        && let Some(finally_state) = try_stack[i].finally_state
                    {
                        handler = Some((i, finally_state, false, try_stack[i]._after_state));
                        break;
                    }
                }

                if needs_for_of_unwind {
                    // A return-replacing throw or an IteratorClose failure can
                    // retain enclosing loops until an intervening handler has
                    // run. Close only the loops crossed before the next handler.
                    let unwind_from = handler.map_or(0, |(depth, _, _, _)| {
                        for_of_stack
                            .iter()
                            .position(|loop_state| loop_state.try_depth > depth)
                            .unwrap_or(for_of_stack.len())
                    });
                    match self.unwind_async_for_of_loops(
                        &mut for_of_stack,
                        unwind_from,
                        &func_env,
                        Completion::Throw(exc),
                    ) {
                        Completion::Throw(error) => exc = error,
                        Completion::Exit(code) => {
                            self.scheduler.remove_async_function_state(async_id);
                            return Completion::Exit(code);
                        }
                        _ => unreachable!("unwinding a throw must stay abrupt"),
                    }
                }

                if needs_for_of_unwind {
                    pending_for_of_unwind = if for_of_stack.is_empty() {
                        None
                    } else {
                        handler.map(|(_, _, _, after_state)| PendingForOfUnwind {
                            clear_at_state: Some(after_state),
                        })
                    };
                }

                if let Some((depth, state, is_catch, _)) = handler {
                    if is_catch {
                        // A catch-only context is finished once its handler is
                        // selected, but try-catch-finally must retain this
                        // context so abrupt control from the catch still routes
                        // through its attached finalizer. EnterCatch marks it
                        // entered, preventing the catch from handling itself.
                        let retained_depth = if try_stack[depth].finally_state.is_some() {
                            depth + 1
                        } else {
                            depth
                        };
                        try_stack.truncate(retained_depth);
                    } else if pending_completion_was_replaced {
                        // Drop the completed inner finally contexts so
                        // EnterFinally marks the handler selected above.
                        try_stack.truncate(depth + 1);
                    }
                    pending_exception = Some(exc);
                    current_id = state;
                    continue;
                }

                let disp = self.dispose_resources(&func_env, Completion::Throw(exc));
                // A disposer that called `__host_exit` (issue #242) propagates
                // out uncatchably instead of rejecting the promise.
                if let Completion::Exit(code) = disp {
                    self.scheduler.remove_async_function_state(async_id);
                    return Completion::Exit(code);
                }
                let exc = match disp {
                    Completion::Throw(e) => e,
                    _ => JsValue::UNDEFINED,
                };
                self.scheduler.remove_async_function_state(async_id);
                let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[exc]);
                return Completion::Normal(JsValue::UNDEFINED);
            }

            self.in_state_machine = true;
            let exec_env = for_of_stack
                .last()
                .map_or(&func_env, ForOfLoopState::effective_env)
                .clone();
            let mut stmt_result = self.exec_body(&state_machine.states[current_id].body, &exec_env);
            self.in_state_machine = saved_in_state_machine;
            // `__host_exit` in the async body (issue #242) propagates out as
            // `Completion::Exit` instead of settling the result promise; the
            // caller re-raises it uncatchably.
            if let Completion::Exit(code) = stmt_result {
                self.scheduler.remove_async_function_state(async_id);
                return Completion::Exit(code);
            }

            // Execute tail calls inline — async functions don't use PTC, but
            // strict mode return statements produce TailCall completions.
            // The resolved result is a return value (TailCall always originates
            // from a `return` statement).
            if matches!(stmt_result, Completion::TailCall { .. }) {
                while let Completion::TailCall { func, this, args } = stmt_result {
                    stmt_result = self.call_function(&func, &this, &args);
                }
                match stmt_result {
                    Completion::Normal(v) | Completion::Return(v) => {
                        route_return!(v);
                        continue;
                    }
                    Completion::Throw(e) => {
                        pending_exception = Some(e);
                        continue;
                    }
                    _ => {}
                }
            }

            match &stmt_result {
                Completion::Throw(e) => {
                    let e = e.clone();
                    pending_exception = Some(e);
                    continue;
                }
                Completion::Return(v) => {
                    // route_return! closes the active for-of iterators first.
                    route_return!(v.clone());
                    continue;
                }
                Completion::Break(label, _) => {
                    // Close iterator for the innermost matching for-of loop
                    if let Some(pos) = for_of_stack.iter().rposition(|_| label.is_none()) {
                        let after_state = for_of_stack[pos].after_state;
                        unwind_for_of!(pos);
                        current_id = after_state;
                        continue;
                    }
                }
                Completion::Continue(label, _) => {
                    // Jump to head_state for the innermost matching for-of loop
                    if let Some(pos) = for_of_stack.iter().rposition(|_| label.is_none()) {
                        current_id = for_of_stack[pos].head_state;
                        continue;
                    }
                }
                _ => {}
            }

            // Handle Completion::Yield from inline awaits (await expressions not
            // decomposed by the state machine transform)
            if let Completion::Yield(yield_val) = stmt_result {
                let yield_val = yield_val.clone();
                self.async_fn_suspend_at_await(
                    async_id,
                    &state_machine,
                    &func_env,
                    is_strict,
                    current_id,
                    &try_stack,
                    None,
                    pending_return.take(),
                    pending_loop_control.take(),
                    saved_finally_exception.take(),
                    pending_for_of_unwind.take(),
                    &resolve_fn,
                    &reject_fn,
                    &yield_val,
                    &for_of_stack,
                );
                return Completion::Normal(JsValue::UNDEFINED);
            }

            let term_env = exec_env;
            match terminator {
                StateTerminator::Await {
                    value,
                    resume_state,
                    sent_value_binding,
                } => {
                    let await_val = match self.eval_expr(&value, &term_env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            pending_exception = Some(e);
                            continue;
                        }
                        Completion::Yield(v) => {
                            self.async_fn_suspend_at_await(
                                async_id,
                                &state_machine,
                                &func_env,
                                is_strict,
                                current_id,
                                &try_stack,
                                sent_value_binding.clone(),
                                pending_return.take(),
                                pending_loop_control.take(),
                                saved_finally_exception.take(),
                                pending_for_of_unwind.take(),
                                &resolve_fn,
                                &reject_fn,
                                &v,
                                &for_of_stack,
                            );
                            return Completion::Normal(JsValue::UNDEFINED);
                        }
                        _ => JsValue::UNDEFINED,
                    };

                    self.async_fn_suspend_at_await(
                        async_id,
                        &state_machine,
                        &func_env,
                        is_strict,
                        resume_state,
                        &try_stack,
                        sent_value_binding.clone(),
                        pending_return.take(),
                        pending_loop_control.take(),
                        saved_finally_exception.take(),
                        pending_for_of_unwind.take(),
                        &resolve_fn,
                        &reject_fn,
                        &await_val,
                        &for_of_stack,
                    );
                    return Completion::Normal(JsValue::UNDEFINED);
                }

                StateTerminator::Return(ref expr) => {
                    let ret_val = if let Some(e) = expr {
                        let mut result = self.eval_expr(e, &term_env);
                        while let Completion::TailCall { func, this, args } = result {
                            result = self.call_function(&func, &this, &args);
                        }
                        match result {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                pending_exception = Some(e);
                                continue;
                            }
                            _ => JsValue::UNDEFINED,
                        }
                    } else {
                        JsValue::UNDEFINED
                    };

                    route_return!(ret_val);
                    continue;
                }

                StateTerminator::Throw(ref expr) => {
                    let throw_val = match self.eval_expr(expr, &term_env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => e,
                        _ => JsValue::UNDEFINED,
                    };
                    pending_exception = Some(throw_val);
                    continue;
                }

                StateTerminator::Goto(next) => {
                    if pending_for_of_unwind
                        .as_ref()
                        .is_some_and(|pending| pending.clear_at_state == Some(next))
                    {
                        pending_for_of_unwind = None;
                    }
                    current_id = next;
                }

                StateTerminator::LoopControl(target) => {
                    route_loop_control!(target);
                }

                StateTerminator::ConditionalGoto {
                    ref condition,
                    true_state,
                    false_state,
                } => {
                    let cond_val = match self.eval_expr(condition, &term_env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            pending_exception = Some(e);
                            continue;
                        }
                        _ => JsValue::UNDEFINED,
                    };
                    current_id = if self.to_boolean_val(&cond_val) {
                        true_state
                    } else {
                        false_state
                    };
                }

                StateTerminator::TryEnter {
                    try_state,
                    ref catch_state,
                    finally_state,
                    after_state,
                } => {
                    try_stack.push(TryContextInfo {
                        catch_state: catch_state.as_ref().map(|c| c.state),
                        finally_state,
                        _after_state: after_state,
                        entered_catch: false,
                        entered_finally: false,
                    });
                    current_id = try_state;
                }

                StateTerminator::TryExit { after_state } => {
                    try_stack.pop();
                    if let Some(exc) = pending_exception.take() {
                        pending_exception = Some(exc);
                        continue;
                    }
                    if let Some(ret_val) = pending_return.take() {
                        route_return!(ret_val);
                        continue;
                    }
                    // Restore any exception saved from before the finally block
                    if let Some(exc) = saved_finally_exception.take() {
                        pending_exception = Some(exc);
                        continue;
                    }
                    if let Some(target) = pending_loop_control.take() {
                        route_loop_control!(target);
                        continue;
                    }
                    if pending_for_of_unwind
                        .as_ref()
                        .is_some_and(|pending| pending.clear_at_state == Some(after_state))
                    {
                        pending_for_of_unwind = None;
                    }
                    current_id = after_state;
                }

                StateTerminator::EnterCatch {
                    body_state,
                    ref param,
                } => {
                    if let Some(ctx) = try_stack.last_mut() {
                        ctx.entered_catch = true;
                    }
                    let exc_val = pending_exception.take().unwrap_or(JsValue::UNDEFINED);
                    if let Some(pattern) = param {
                        let _ = self.bind_pattern(pattern, exc_val, BindingKind::Let, &term_env);
                    }
                    current_id = body_state;
                }

                StateTerminator::EnterFinally { body_state } => {
                    if let Some(ctx) = try_stack.last_mut() {
                        ctx.entered_finally = true;
                    }
                    // Park any pending exception so the finally body runs normally
                    saved_finally_exception = pending_exception.take();
                    current_id = body_state;
                }

                StateTerminator::SwitchDispatch {
                    ref discriminant,
                    ref cases,
                    default_state,
                    after_state,
                } => {
                    let disc_val = match self.eval_expr(discriminant, &term_env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            pending_exception = Some(e);
                            continue;
                        }
                        _ => JsValue::UNDEFINED,
                    };
                    let mut matched = false;
                    for case in cases {
                        let case_val = match self.eval_expr(&case.test, &term_env) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                pending_exception = Some(e);
                                matched = true;
                                break;
                            }
                            _ => JsValue::UNDEFINED,
                        };
                        if strict_equality(&disc_val, &case_val) {
                            current_id = case.state;
                            matched = true;
                            break;
                        }
                    }
                    if pending_exception.is_some() {
                        continue;
                    }
                    if !matched {
                        current_id = default_state.unwrap_or(after_state);
                    }
                }

                StateTerminator::ForOfInit {
                    ref iterable,
                    ref iter_var,
                    ref left,
                    head_state,
                    after_state: forinit_after,
                    is_await,
                    ..
                } => {
                    // §14.7.5.12 ForIn/OfHeadEvaluation: create TDZ bindings
                    // before evaluating the iterable expression
                    let iterable_env = if let ForInOfLeft::Variable(decl) = left
                        && !matches!(decl.kind, VarKind::Var)
                    {
                        let head_env = Environment::new(Some(term_env.clone()));
                        let mut tdz_names = Vec::new();
                        if let Some(d) = decl.declarations.first() {
                            d.pattern.bound_names(&mut tdz_names);
                        }
                        let binding_kind = match decl.kind {
                            VarKind::Let => BindingKind::Let,
                            VarKind::Const | VarKind::Using | VarKind::AwaitUsing => {
                                BindingKind::Const
                            }
                            VarKind::Var => unreachable!(),
                        };
                        for name in &tdz_names {
                            head_env.borrow_mut().declare(name, binding_kind);
                        }
                        head_env
                    } else {
                        term_env.clone()
                    };

                    let iterable_result = self.eval_expr(iterable, &iterable_env);

                    let iterable_val = match iterable_result {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            pending_exception = Some(e);
                            continue;
                        }
                        _ => JsValue::UNDEFINED,
                    };
                    let iterator = if is_await {
                        match self.get_async_iterator(&iterable_val) {
                            Ok(it) => it,
                            Err(e) => {
                                pending_exception = Some(e);
                                continue;
                            }
                        }
                    } else {
                        match self.get_iterator(&iterable_val) {
                            Ok(it) => it,
                            Err(e) => {
                                pending_exception = Some(e);
                                continue;
                            }
                        }
                    };
                    self.gc_root_value(&iterator);
                    self.pending_iter_close.push(iterator.clone());
                    self.env_set(&func_env, iter_var, iterator).ok();
                    for_of_stack.push(ForOfLoopState {
                        iter_var: iter_var.clone(),
                        head_state,
                        after_state: forinit_after,
                        try_depth: try_stack.len(),
                        outer_env: term_env,
                        iteration_env: None,
                    });
                    current_id = head_state;
                }

                StateTerminator::ForOfHead {
                    ref iter_var,
                    ref left,
                    body_state,
                    after_state,
                    is_await,
                    ..
                } => {
                    // A head reached without its entry would mean the driver's
                    // unwinding dropped it; rebuild one rather than abort.
                    let loop_pos = match for_of_stack
                        .iter()
                        .rposition(|loop_state| loop_state.iter_var == *iter_var)
                    {
                        Some(pos) => pos,
                        None => {
                            debug_assert!(false, "for-of head without an active loop state");
                            for_of_stack.push(ForOfLoopState {
                                iter_var: iter_var.clone(),
                                head_state: current_id,
                                after_state,
                                try_depth: try_stack.len(),
                                outer_env: term_env.clone(),
                                iteration_env: None,
                            });
                            for_of_stack.len() - 1
                        }
                    };

                    // Dispose resources from previous iteration (for using/await using)
                    if let Some(disp_env) = for_of_stack[loop_pos].iteration_env.take() {
                        let disp = self.dispose_resources(&disp_env, Completion::Empty);
                        if let Completion::Exit(code) = disp {
                            // A disposer that called `__host_exit` (issue #242)
                            // propagates out uncatchably.
                            self.scheduler.remove_async_function_state(async_id);
                            return Completion::Exit(code);
                        }
                        if let Completion::Throw(e) = disp {
                            pending_exception = Some(e);
                            continue;
                        }
                    }

                    // For `for await`, use a temp var to distinguish first
                    // entry (call iterator_next + await) from resume (result ready)
                    let await_tmp = format!("{}__await", iter_var);
                    let step_result = if is_await {
                        let cached = func_env.borrow().get(&await_tmp);
                        if let Some(v) = cached
                            && !(v).is_undefined()
                        {
                            // Resume after await — clear the temp and use the value
                            func_env
                                .borrow_mut()
                                .set(&await_tmp, JsValue::UNDEFINED)
                                .ok();
                            v
                        } else {
                            // First entry — call iterator_next, then suspend for await
                            let iterator = func_env
                                .borrow()
                                .get(iter_var)
                                .unwrap_or(JsValue::UNDEFINED);
                            let raw_result = match self.iterator_next(&iterator) {
                                Ok(v) => v,
                                Err(e) => {
                                    for_of_protocol_failure = Some(iter_var.clone());
                                    pending_exception = Some(e);
                                    continue;
                                }
                            };
                            // Ensure the temp var exists
                            if func_env.borrow().get(&await_tmp).is_none() {
                                func_env.borrow_mut().declare(&await_tmp, BindingKind::Var);
                            }
                            use crate::interpreter::generator_transform::{
                                SentValueBinding, SentValueBindingKind,
                            };
                            let binding = Some(SentValueBinding {
                                kind: SentValueBindingKind::Variable(await_tmp),
                            });
                            self.async_fn_suspend_at_await(
                                async_id,
                                &state_machine,
                                &func_env,
                                is_strict,
                                current_id, // resume to same ForOfHead state
                                &try_stack,
                                binding,
                                pending_return.take(),
                                pending_loop_control.take(),
                                saved_finally_exception.take(),
                                pending_for_of_unwind.take(),
                                &resolve_fn,
                                &reject_fn,
                                &raw_result,
                                &for_of_stack,
                            );
                            self.in_state_machine = saved_in_state_machine;
                            return Completion::Normal(JsValue::UNDEFINED);
                        }
                    } else {
                        let iterator = func_env
                            .borrow()
                            .get(iter_var)
                            .unwrap_or(JsValue::UNDEFINED);
                        match self.iterator_next(&iterator) {
                            Ok(v) => v,
                            Err(e) => {
                                for_of_protocol_failure = Some(iter_var.clone());
                                pending_exception = Some(e);
                                continue;
                            }
                        }
                    };
                    let done = match self.iterator_complete(&step_result) {
                        Ok(d) => d,
                        Err(e) => {
                            for_of_protocol_failure = Some(iter_var.clone());
                            pending_exception = Some(e);
                            continue;
                        }
                    };
                    if done {
                        let iterator = func_env
                            .borrow()
                            .get(iter_var)
                            .unwrap_or(JsValue::UNDEFINED);
                        self.unroot_async_for_of_iterator(&iterator);
                        for_of_stack.remove(loop_pos);
                        current_id = after_state;
                    } else {
                        let value = match self.iterator_value(&step_result) {
                            Ok(v) => v,
                            Err(e) => {
                                for_of_protocol_failure = Some(iter_var.clone());
                                pending_exception = Some(e);
                                continue;
                            }
                        };
                        let needs_iter_env = matches!(left, ForInOfLeft::Variable(decl) if !matches!(decl.kind, VarKind::Var));
                        let outer_env = for_of_stack[loop_pos].outer_env.clone();
                        let bind_env = if needs_iter_env {
                            let ie = Environment::new(Some(outer_env));
                            for_of_stack[loop_pos].iteration_env = Some(ie.clone());
                            ie
                        } else {
                            outer_env
                        };
                        let bind_result = match left {
                            ForInOfLeft::Variable(decl) => {
                                let is_using =
                                    matches!(decl.kind, VarKind::Using | VarKind::AwaitUsing);
                                if is_using {
                                    let hint = if decl.kind == VarKind::AwaitUsing {
                                        crate::interpreter::types::DisposeHint::Async
                                    } else {
                                        crate::interpreter::types::DisposeHint::Sync
                                    };
                                    if let Err(e) =
                                        self.add_disposable_resource(&bind_env, &value, hint)
                                    {
                                        pending_exception = Some(e);
                                        continue;
                                    }
                                }
                                if let Some(d) = decl.declarations.first() {
                                    self.bind_pattern(
                                        &d.pattern,
                                        value,
                                        match decl.kind {
                                            VarKind::Var => BindingKind::Var,
                                            VarKind::Let => BindingKind::Let,
                                            VarKind::Const
                                            | VarKind::Using
                                            | VarKind::AwaitUsing => BindingKind::Const,
                                        },
                                        &bind_env,
                                    )
                                } else {
                                    Ok(())
                                }
                            }
                            ForInOfLeft::Pattern(p) => {
                                match self.assign_to_for_pattern(p, value, &bind_env) {
                                    Completion::Normal(_) | Completion::Empty => Ok(()),
                                    Completion::Throw(e) => Err(e),
                                    _ => Ok(()),
                                }
                            }
                            ForInOfLeft::Expression(e) => self.assign_to_expr(e, value, &bind_env),
                        };
                        if let Err(e) = bind_result {
                            pending_exception = Some(e);
                            continue;
                        }
                        current_id = body_state;
                    }
                }

                StateTerminator::Completed => {
                    return self.async_fn_complete(async_id, &func_env, &resolve_fn, &reject_fn);
                }

                StateTerminator::Yield { .. } => {
                    unreachable!("Yield terminator in async function")
                }
            }
        }
    }

    fn unroot_async_for_of_iterator(&mut self, iterator: &JsValue) {
        self.gc_unroot_value(iterator);
        if let Some(iterator_id) = iterator.as_object_id() {
            self.pending_iter_close
                .retain(|value| value.as_object_id() != Some(iterator_id));
        }
    }

    fn close_async_for_of_loop(
        &mut self,
        loop_state: ForOfLoopState,
        func_env: &EnvRef,
        completion: Completion,
    ) -> Completion {
        let mut completion = match loop_state.iteration_env {
            Some(env) => self.dispose_resources(&env, completion),
            None => completion,
        };

        // The borrow must end before `iterator_close_result` runs the user's
        // `return` method, which may write bindings in this same environment.
        let iterator = func_env.borrow().get(&loop_state.iter_var);
        if matches!(completion, Completion::Exit(_)) {
            if let Some(iterator) = iterator {
                self.unroot_async_for_of_iterator(&iterator);
            }
            return completion;
        }
        if let Some(iterator) = iterator {
            let close_result = self.iterator_close_result(&iterator);
            self.unroot_async_for_of_iterator(&iterator);
            if let Some(code) = self.pending_exit {
                return Completion::Exit(code);
            }
            if !completion.is_abrupt()
                && let Err(error) = close_result
            {
                completion = Completion::Throw(error);
            }
        }

        completion
    }

    /// Closes every active for-of loop from `from` to the innermost, inner to
    /// outer, carrying each loop's resulting completion into the next outer
    /// iteration disposal.
    fn unwind_async_for_of_loops(
        &mut self,
        for_of_stack: &mut Vec<ForOfLoopState>,
        from: usize,
        func_env: &EnvRef,
        mut completion: Completion,
    ) -> Completion {
        for loop_state in for_of_stack.drain(from..).rev() {
            completion = self.close_async_for_of_loop(loop_state, func_env, completion);
            if matches!(completion, Completion::Exit(_)) {
                break;
            }
        }
        completion
    }

    fn async_fn_complete(
        &mut self,
        async_id: u64,
        func_env: &EnvRef,
        resolve_fn: &JsValue,
        reject_fn: &JsValue,
    ) -> Completion {
        let disp = self.dispose_resources(func_env, Completion::Normal(JsValue::UNDEFINED));
        self.scheduler.remove_async_function_state(async_id);
        match disp {
            // A disposer that called `__host_exit` (issue #242) propagates out
            // uncatchably instead of settling the result promise.
            Completion::Exit(code) => Completion::Exit(code),
            Completion::Throw(e) => {
                let _ = self.call_function(reject_fn, &JsValue::UNDEFINED, &[e]);
                Completion::Normal(JsValue::UNDEFINED)
            }
            _ => {
                let _ = self.call_function(resolve_fn, &JsValue::UNDEFINED, &[JsValue::UNDEFINED]);
                Completion::Normal(JsValue::UNDEFINED)
            }
        }
    }

    fn async_fn_suspend_at_await(
        &mut self,
        async_id: u64,
        state_machine: &Rc<crate::interpreter::generator_transform::GeneratorStateMachine>,
        func_env: &EnvRef,
        is_strict: bool,
        resume_state: usize,
        try_stack: &[TryContextInfo],
        sent_value_binding: Option<crate::interpreter::generator_transform::SentValueBinding>,
        pending_return: Option<JsValue>,
        pending_loop_control: Option<crate::interpreter::generator_transform::LoopControlTarget>,
        saved_finally_exception: Option<JsValue>,
        pending_for_of_unwind: Option<PendingForOfUnwind>,
        resolve_fn: &JsValue,
        reject_fn: &JsValue,
        await_val: &JsValue,
        for_of_stack: &[ForOfLoopState],
    ) {
        let promise = self.promise_resolve_value(await_val);
        let promise_id = if let Some(o) = (promise)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            o.id
        } else {
            0
        };

        // Save state for resumption
        self.scheduler.insert_async_function_state(
            async_id,
            AsyncFunctionState {
                state_machine: state_machine.clone(),
                func_env: func_env.clone(),
                is_strict,
                current_state: resume_state,
                try_stack: try_stack.to_vec(),
                pending_binding: sent_value_binding,
                pending_return,
                pending_loop_control,
                saved_finally_exception,
                pending_for_of_unwind,
                resolve_fn: resolve_fn.clone(),
                reject_fn: reject_fn.clone(),
                for_of_stack: for_of_stack.to_vec(),
                module_path: self.module_async_info.get(&async_id).cloned(),
            },
        );

        // Schedule continuation based on promise state
        let pstate = self.get_promise_state(promise_id);
        match pstate {
            Some(PromiseState::Fulfilled(v)) => {
                let value = v.clone();
                self.scheduler.enqueue_microtask((
                    vec![resolve_fn.clone(), reject_fn.clone(), value.clone()],
                    // Return the resume completion so a `__host_exit` in the
                    // resumed body (issue #242) reaches the drain loop as
                    // `Completion::Exit` instead of being discarded.
                    Box::new(move |interp| interp.async_function_resume(async_id, value, false)),
                ));
            }
            Some(PromiseState::Rejected(e)) => {
                let err = e.clone();
                self.scheduler.enqueue_microtask((
                    vec![resolve_fn.clone(), reject_fn.clone(), err.clone()],
                    Box::new(move |interp| interp.async_function_resume(async_id, err, true)),
                ));
            }
            _ => {
                // Pending or not a promise — attach handlers
                let resolve_c = resolve_fn.clone();
                let reject_c = reject_fn.clone();
                let fulfill_handler = self.create_function(JsFunction::native(
                    "asyncFnFulfill".to_string(),
                    1,
                    // Return the resume completion so a `__host_exit` in the
                    // resumed body (issue #242) propagates as `Completion::Exit`
                    // through the Promise reaction to the drain loop.
                    move |interp, _this, args| {
                        let v = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        interp.async_function_resume(async_id, v, false)
                    },
                ));

                let reject_handler = self.create_function(JsFunction::native(
                    "asyncFnReject".to_string(),
                    1,
                    move |interp, _this, args| {
                        let e = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        interp.async_function_resume(async_id, e, true)
                    },
                ));

                if let Some(obj) = self.get_object_cell(promise_id) {
                    let mut ob = obj.borrow_mut();
                    if let Some(pd) = ob.promise_data_mut() {
                        pd.is_handled = true;
                        pd.fulfill_reactions.push(PromiseReaction {
                            handler: Some(fulfill_handler),
                            promise_id: None,
                            resolve: resolve_c,
                            reject: reject_c,
                            reaction_type: PromiseReactionType::Fulfill,
                        });
                        pd.reject_reactions.push(PromiseReaction {
                            handler: Some(reject_handler),
                            promise_id: None,
                            resolve: JsValue::UNDEFINED,
                            reject: JsValue::UNDEFINED,
                            reaction_type: PromiseReactionType::Reject,
                        });
                    }
                }
            }
        }
    }

    /// Spec [[Get]] — reads a property from an object, invoking getters.
    pub(crate) fn obj_get(&mut self, obj_val: &JsValue, key: &str) -> Result<JsValue, JsValue> {
        if let Some(o) = (obj_val)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            let mut current_id = Some(o.id);
            while let Some(id) = current_id {
                if let Some(obj) = self.get_object_cell(id) {
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
                                    _ => Ok(JsValue::UNDEFINED),
                                };
                            }
                            return Ok(JsValue::UNDEFINED);
                        }
                        if let Some(ref val) = desc.value {
                            return Ok(val.clone());
                        }
                        return Ok(JsValue::UNDEFINED);
                    }
                    current_id = b.prototype_id;
                } else {
                    break;
                }
            }
        }
        Ok(JsValue::UNDEFINED)
    }

    pub(crate) fn await_value(&mut self, val: &JsValue) -> Completion {
        use std::cell::Cell;

        // §27.7.5.3 Await — every await goes through PromiseResolve and schedules
        // its continuation as a microtask, ensuring proper interleaving.
        let promise = self.promise_resolve_value(val);
        let promise_id = if let Some(o) = (promise)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            o.id
        } else {
            0
        };

        let gc_frame = self.gc_root_frame();
        self.gc_root_value(&promise);

        let done = Rc::new(Cell::new(false));
        let result: Rc<RefCell<Option<Result<JsValue, JsValue>>>> = Rc::new(RefCell::new(None));

        let state = self.get_promise_state(promise_id);
        match state {
            Some(PromiseState::Fulfilled(v)) => {
                let done_c = done.clone();
                let result_c = result.clone();
                let value = v.clone();
                self.scheduler.enqueue_microtask((
                    vec![value.clone()],
                    Box::new(move |_interp| {
                        done_c.set(true);
                        *result_c.borrow_mut() = Some(Ok(value));
                        Completion::Normal(JsValue::UNDEFINED)
                    }),
                ));
            }
            Some(PromiseState::Rejected(r)) => {
                let done_c = done.clone();
                let result_c = result.clone();
                let reason = r.clone();
                self.scheduler.enqueue_microtask((
                    vec![reason.clone()],
                    Box::new(move |_interp| {
                        done_c.set(true);
                        *result_c.borrow_mut() = Some(Err(reason));
                        Completion::Normal(JsValue::UNDEFINED)
                    }),
                ));
            }
            Some(PromiseState::Pending) => {
                let done_f = done.clone();
                let result_f = result.clone();
                let done_r = done.clone();
                let result_r = result.clone();

                let fulfill_handler = self.create_function(JsFunction::native(
                    "awaitFulfill".to_string(),
                    1,
                    move |_interp, _this, args| {
                        let v = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        done_f.set(true);
                        *result_f.borrow_mut() = Some(Ok(v));
                        Completion::Normal(JsValue::UNDEFINED)
                    },
                ));
                let reject_handler = self.create_function(JsFunction::native(
                    "awaitReject".to_string(),
                    1,
                    move |_interp, _this, args| {
                        let v = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        done_r.set(true);
                        *result_r.borrow_mut() = Some(Err(v));
                        Completion::Normal(JsValue::UNDEFINED)
                    },
                ));

                if let Some(obj) = self.get_object_cell(promise_id) {
                    let mut o = obj.borrow_mut();
                    if let Some(pd) = o.promise_data_mut() {
                        pd.is_handled = true;
                        pd.fulfill_reactions.push(PromiseReaction {
                            handler: Some(fulfill_handler),
                            promise_id: None,
                            resolve: JsValue::UNDEFINED,
                            reject: JsValue::UNDEFINED,
                            reaction_type: PromiseReactionType::Fulfill,
                        });
                        pd.reject_reactions.push(PromiseReaction {
                            handler: Some(reject_handler),
                            promise_id: None,
                            resolve: JsValue::UNDEFINED,
                            reject: JsValue::UNDEFINED,
                            reaction_type: PromiseReactionType::Reject,
                        });
                    }
                }
            }
            None => {
                self.gc_unroot_frame(gc_frame);
                return Completion::Normal(val.clone());
            }
        }

        let await_deadline = if self.is_agent_thread {
            Some(std::time::Instant::now() + std::time::Duration::from_secs(120))
        } else {
            None
        };
        loop {
            if done.get() {
                break;
            }
            // Terminal-sink read for `__host_exit` (issue #242): stop draining
            // immediately so no further queued user job runs after the exit.
            // Inert unless the node host floor is on.
            if self.pending_exit.is_some() {
                break;
            }
            if let Some((roots, job)) = self.scheduler.pop_microtask() {
                let mt_frame = self.gc_root_frame();
                for val in &roots {
                    self.gc_root_value(val);
                }
                let job_result = job(self);
                self.gc_unroot_frame(mt_frame);
                // A `__host_exit` inside the job (issue #242) latches the
                // terminal sink; the post-loop check turns it into a
                // `Completion::Exit` returned from this `await`.
                if let Completion::Exit(code) = job_result {
                    self.pending_exit = Some(code);
                    break;
                }
                continue;
            }
            // Check agent async completions
            let completions: Vec<_> = {
                let mut lock = self.agent_async_completions.0.lock().unwrap();
                lock.drain(..).collect()
            };
            if !completions.is_empty() {
                for f in completions {
                    f(self);
                    // Stop before the rest of the batch if a callback exited.
                    if self.pending_exit.is_some() {
                        break;
                    }
                }
                continue;
            }
            // For agent threads, block-wait for async completions
            if let Some(deadline) = await_deadline {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                if remaining.is_zero() {
                    break;
                }
                // Timers are in-process (#254), so this loop has to fire them
                // itself rather than wait for a completion that never comes.
                if self.run_due_timers() {
                    continue;
                }
                let wait = self.completion_wait(remaining);
                let (ref mtx, ref cvar) = *self.agent_async_completions;
                let lock = mtx.lock().unwrap();
                if !lock.is_empty() {
                    drop(lock);
                    continue;
                }
                let _ = cvar.wait_timeout(lock, wait).unwrap();
                continue;
            }
            break;
        }

        self.gc_unroot_frame(gc_frame);

        // If a drained job requested `__host_exit` (issue #242), propagate the
        // exit out of this `await` uncatchably instead of yielding the awaited
        // value; the terminal sink stays set for `main`.
        if let Some(code) = self.pending_exit {
            return Completion::Exit(code);
        }
        match result.borrow_mut().take() {
            Some(Ok(v)) => Completion::Normal(v),
            Some(Err(e)) => Completion::Throw(e),
            None => Completion::Normal(JsValue::UNDEFINED),
        }
    }
}
