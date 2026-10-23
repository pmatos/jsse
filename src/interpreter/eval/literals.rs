use super::*;
use crate::interpreter::literal_frame::LiteralFrame;

thread_local! {
    /// The default `constructor(...args) { super(...args); }` that
    /// ClassDefinitionEvaluation gives a derived class without one. Every such
    /// class shares this Body, so the per-Body side tables (hoist analysis,
    /// bytecode compile verdict, IC store) hold one entry for all of them and
    /// the body is compiled once instead of once per class evaluation.
    static DEFAULT_DERIVED_CONSTRUCTOR: (Rc<Vec<Pattern>>, Body, Rc<LiteralFrame>) = {
        let statements = vec![Statement::Expression(Expression::Call(
            ExprBox::new(Expression::Super),
            vec![Expression::Spread(ExprBox::new(Expression::Identifier(
                "args".into(),
            )))],
            CallSiteId::UNASSIGNED,
        ))];
        debug_assert!(!stmts_use_arguments(&statements));
        let params = vec![Pattern::Rest(Box::new(Pattern::Identifier("args".into())))];
        (Rc::new(params), Body::new(statements), Rc::default())
    };
    /// The synthesized `constructor() {}` of a base class without one, shared
    /// the same way.
    static DEFAULT_BASE_CONSTRUCTOR: (Rc<Vec<Pattern>>, Body, Rc<LiteralFrame>) =
        (Rc::new(Vec::new()), Body::new(Vec::new()), Rc::default());
}

pub(crate) fn literal_string_property_key(code_units: &[u16]) -> JsPropertyKey {
    JsPropertyKey::from_js_string(&JsString::from_units(code_units))
}

pub(crate) fn regexp_literal_source(pattern: &[u16]) -> JsString {
    if pattern.is_empty() {
        JsString::from_str("(?:)")
    } else {
        JsString::from_units(pattern)
    }
}

/// The property key of a statically named literal property, `None` for a
/// computed or private one.
/// The key of a statically named property, in the interner's canonical
/// storage. An identifier key already in the interner builds no key at all.
pub(crate) fn static_property_key(key: &PropertyKey) -> Option<JsPropertyKey> {
    use crate::interpreter::key_intern::{intern_js_key, intern_key};
    match key {
        PropertyKey::Identifier(name) => Some(intern_key(name)),
        PropertyKey::String(units) => Some(intern_js_key(literal_string_property_key(units))),
        PropertyKey::Number(n) => Some(intern_js_key(JsPropertyKey::from(number_ops::to_string(
            *n,
        )))),
        PropertyKey::Computed(_) | PropertyKey::Private(_) => None,
    }
}

pub(crate) fn prefixed_function_name(prefix: &str, key: &JsPropertyKey) -> JsPropertyKey {
    let mut code_units: Vec<u16> = prefix.encode_utf16().collect();
    let name = key
        .symbol_function_name()
        .unwrap_or_else(|| key.to_js_string());
    code_units.extend_from_slice(name.as_units());
    JsPropertyKey::from_js_string(&JsString::from_vec(code_units))
}

impl Interpreter {
    pub(super) fn get_template_object(&mut self, tmpl: &TemplateLiteral) -> JsValue {
        let cache_key = tmpl.id;
        if let Some(&obj_id) = self.realm().template_cache.get(&cache_key)
            && self.get_object_cell(obj_id).is_some()
        {
            return JsValue::object(obj_id);
        }

        let cooked_vals: Vec<JsValue> = tmpl
            .quasis
            .iter()
            .map(|q| match q {
                Some(cu) => JsValue::string(JsString::from_vec(cu.clone())),
                None => JsValue::UNDEFINED,
            })
            .collect();
        let raw_vals: Vec<JsValue> = tmpl
            .raw_quasis
            .iter()
            .map(|s| {
                JsValue::string(if tmpl.raw_stand_ins {
                    crate::interpreter::builtins::regexp::decode_source_text(s)
                } else {
                    JsString::from_str(s)
                })
            })
            .collect();

        let raw_arr = self.create_frozen_template_array(raw_vals);
        let template_arr = self.create_frozen_template_array(cooked_vals);

        if let Some(o) = template_arr
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
            && let Some(obj) = self.get_object_cell(o.id)
        {
            obj.borrow_mut().insert_property(
                "raw".to_string(),
                PropertyDescriptor::data(raw_arr, false, false, false),
            );
        }

        if let Some(o) = template_arr
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            self.realm_mut().template_cache.insert(cache_key, o.id);
        }

        template_arr
    }

    pub(super) fn create_frozen_template_array(&mut self, values: Vec<JsValue>) -> JsValue {
        let len = values.len();
        let mut obj_data = JsObjectData::new();
        obj_data.prototype_id = self
            .realm()
            .array_prototype
            .or(self.realm().object_prototype);
        obj_data.class_name = "Array";
        obj_data.kind = crate::interpreter::types::ObjectKind::Array(
            crate::interpreter::types::ArrayData::default(),
        );
        for (i, v) in values.iter().enumerate() {
            obj_data.insert_property(
                i.to_string(),
                PropertyDescriptor::data(v.clone(), false, true, false),
            );
        }
        obj_data.insert_property(
            "length".to_string(),
            PropertyDescriptor::data(JsValue::number(len as f64), false, false, false),
        );
        obj_data.set_array_elements(values);
        obj_data.extensible = false;
        let id = self.alloc_object(obj_data);
        JsValue::object(id)
    }

    pub(super) fn eval_literal(&mut self, lit: &Literal) -> JsValue {
        match lit {
            Literal::Null => JsValue::NULL,
            Literal::Boolean(b) => JsValue::boolean(*b),
            Literal::Number(n) => JsValue::number(*n),
            Literal::String(s) => JsValue::string(JsString::from_vec(s.clone())),
            Literal::BigInt(b) => JsValue::bigint(b.clone()),
            Literal::RegExp(pattern, flags) => {
                let source_js = regexp_literal_source(pattern);
                self.alloc_regexp_literal(source_js, JsString::from_str(flags))
            }
        }
    }

    /// A fresh RegExp object for a literal (each evaluation of a regular
    /// expression literal creates a new object, sec-regular-expression-
    /// literals-runtime-semantics-evaluation).
    pub(crate) fn alloc_regexp_literal(&mut self, source: JsString, flags: JsString) -> JsValue {
        let mut obj = JsObjectData::new();
        obj.prototype_id = self
            .realm()
            .regexp_prototype
            .or(self.realm().object_prototype);
        obj.class_name = "RegExp";
        obj.kind = crate::interpreter::types::ObjectKind::RegExp(
            crate::interpreter::types::RegExpData::new(source, flags),
        );
        obj.insert_property(
            "lastIndex".to_string(),
            PropertyDescriptor::data(JsValue::number(0.0), true, false, false),
        );
        let id = self.alloc_object(obj);
        JsValue::object(id)
    }

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_property_key(&mut self, val: &JsValue) -> Result<JsPropertyKey, JsValue> {
        // Fast path: non-negative integer Number → integer string (avoids ryu_js)
        if let Some(n) = (val).as_number() {
            let trunc = n.trunc();
            if n == trunc && trunc >= 0.0 && trunc <= u32::MAX as f64 {
                return Ok(JsPropertyKey::from_index(trunc as usize));
            }
        }
        if let Some(s) = val.as_string() {
            Ok(crate::interpreter::key_intern::key_from_js_string(&s))
        } else if let Some(s) = val.as_symbol() {
            Ok(s.to_property_key())
        } else if val.is_object() {
            let prim = self.to_primitive(val, "string")?;
            if let Some(s) = prim.as_symbol() {
                return Ok(s.to_property_key());
            }
            if let Some(s) = prim.as_string() {
                Ok(crate::interpreter::key_intern::key_from_js_string(&s))
            } else {
                self.to_string_lossy(&prim).map(JsPropertyKey::from)
            }
        } else {
            self.to_string_lossy(val).map(JsPropertyKey::from)
        }
    }

    /// `ToPropertyKey`, but returns the converted primitive as a `JsValue`
    /// (string or symbol) instead of the engine's internal `JsPropertyKey`
    /// representation — for the `ToPropertyKey` state-machine terminator,
    /// which stores the converted key back into a temp var for later reuse
    /// (see `docs/adr/` for issue #771: converting once and reusing the
    /// primitive avoids a second, user-observable `ToPropertyKey` call on a
    /// computed key ahead of an object-pattern rest).
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_property_key_value(&mut self, val: &JsValue) -> Result<JsValue, JsValue> {
        let key = self.to_property_key(val)?;
        Ok(self.symbol_key_to_jsvalue(&key))
    }

    pub(crate) fn create_regexp(&mut self, pattern: &JsString, flags: &str) -> JsValue {
        let mut obj = JsObjectData::new();
        obj.prototype_id = self
            .realm()
            .regexp_prototype
            .or(self.realm().object_prototype);
        obj.class_name = "RegExp";
        let source = if pattern.is_empty() {
            JsString::from_str("(?:)")
        } else {
            pattern.clone()
        };
        obj.kind = crate::interpreter::types::ObjectKind::RegExp(
            crate::interpreter::types::RegExpData::new(source, JsString::from_str(flags)),
        );
        obj.insert_property(
            "lastIndex".to_string(),
            PropertyDescriptor::data(JsValue::number(0.0), true, false, false),
        );
        let id = self.alloc_object(obj);
        JsValue::object(id)
    }

    pub(super) fn eval_array_literal(
        &mut self,
        elements: &[Option<Expression>],
        env: &EnvRef,
    ) -> Completion {
        let gc_frame = self.gc_root_frame();
        let result = (|| {
            let mut items: Vec<Option<JsValue>> = Vec::with_capacity(elements.len());
            for elem in elements {
                match elem {
                    Some(Expression::Spread(inner)) => {
                        let val = match self.eval_expr(inner, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        self.gc_root_value(&val);
                        match self.spread_values(inner, &val) {
                            Ok(spread_items) => {
                                for item in spread_items {
                                    self.gc_root_value(&item);
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
                        self.gc_root_value(&val);
                        items.push(Some(val));
                    }
                    None => items.push(None), // elision — no own property
                }
            }
            Completion::Normal(self.create_array_with_holes(items.into_iter()))
        })();
        self.gc_unroot_frame(gc_frame);
        result
    }

    pub(crate) fn eval_class(
        &mut self,
        name: &str,
        class_binding_name: &str,
        super_class: &Option<Box<Expression>>,
        body: &[ClassElement],
        env: &EnvRef,
        class_source_text: Option<SourceText>,
    ) -> Completion {
        let mut pn_set = PrivateNameScope::default();
        for elem in body {
            let key = match elem {
                ClassElement::Method(m) => &m.key,
                ClassElement::Property(p) | ClassElement::AutoAccessor(p) => &p.key,
                ClassElement::StaticBlock(_) => continue,
            };
            if let PropertyKey::Private(n) = key {
                pn_set.insert_if_absent(n, || {
                    let id = PrivateId(self.next_private_id);
                    self.next_private_id += 1;
                    id
                });
            }
        }
        self.class_private_names.push(Rc::new(pn_set));
        let gc_frame = self.gc_root_frame();
        let result = self.eval_class_inner(
            name,
            class_binding_name,
            super_class,
            body,
            env,
            class_source_text,
        );
        self.gc_unroot_frame(gc_frame);
        self.class_private_names.pop();
        result
    }

    /// Get(superclass, "prototype") of ClassDefinitionEvaluation step 8, which
    /// must be an Object or null.
    fn superclass_prototype(&mut self, super_id: u64, superclass: &JsValue) -> Completion {
        let proto = match self.get_object_property(super_id, "prototype", superclass) {
            Completion::Normal(v) => v,
            other => return other,
        };
        if !proto.is_object() && !proto.is_null() {
            return Completion::Throw(
                self.create_type_error(
                    "Class extends value does not have valid prototype property",
                ),
            );
        }
        Completion::Normal(proto)
    }

    /// `protoParent` for an evaluated ClassHeritage (§15.7.14 step 8), with
    /// the checks that precede any ClassElement.
    pub(crate) fn class_proto_parent(&mut self, superclass: &JsValue) -> Completion {
        if superclass.is_null() {
            return Completion::Normal(JsValue::NULL);
        }
        let Some(super_id) = superclass
            .as_object_id()
            .filter(|_| self.is_constructor(superclass))
        else {
            return Completion::Throw(
                self.create_type_error("Class extends value is not a constructor or null"),
            );
        };
        self.superclass_prototype(super_id, superclass)
    }

    pub(super) fn eval_class_inner(
        &mut self,
        name: &str,
        class_binding_name: &str,
        super_class: &Option<Box<Expression>>,
        body: &[ClassElement],
        env: &EnvRef,
        class_source_text: Option<SourceText>,
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

        // Per spec §15.7.14: Create class environment FIRST so heritage expression
        // is evaluated in it, and closures in heritage capture the class name binding.
        let class_env = Environment::new(Some(env.clone()));
        class_env.borrow_mut().class_private_names = self.class_private_names.last().cloned();
        class_env.borrow_mut().strict = true;
        // Pre-declare class name as uninitialized immutable binding (spec step 4a)
        if !class_binding_name.is_empty() {
            class_env
                .borrow_mut()
                .declare(class_binding_name, BindingKind::Const);
        }

        // Evaluate super class in class_env context (spec step 6a-6b). A
        // heritage the generator lowering already checked carries its
        // `protoParent`.
        let (super_val, resolved_proto) = match super_class.as_deref() {
            Some(Expression::ResolvedHeritage(sup, proto)) => {
                let sv = match self.eval_expr(sup, &class_env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                match self.eval_expr(proto, &class_env) {
                    Completion::Normal(pv) => (Some(sv), Some(pv)),
                    other => return other,
                }
            }
            Some(sc) => match self.eval_expr(sc, &class_env) {
                Completion::Normal(v) => (Some(v), None),
                other => return other,
            },
            None => (None, None),
        };

        // Validate super class: must be null or a constructor
        if resolved_proto.is_none()
            && let Some(ref sv) = super_val
            && !(sv).is_null()
            && !self.is_constructor(sv)
        {
            return Completion::Throw(
                self.create_type_error("Class extends value is not a constructor or null"),
            );
        }

        if let Some(ref sv) = super_val {
            class_env
                .borrow_mut()
                .declare("__super__", BindingKind::Const);
            class_env
                .borrow_mut()
                .initialize_binding("__super__", sv.clone());
        }

        // Create constructor function (classes are always strict mode)
        let (ctor_params, ctor_body, ctor_frame, uses_arguments, has_simple_params) =
            if let Some(cm) = ctor_method {
                let info = cm.value.info();
                (
                    info.params.clone(),
                    cm.value.body.clone(),
                    info.frame.clone(),
                    info.uses_arguments,
                    info.has_simple_params,
                )
            } else if super_val.is_some() {
                // default derived constructor takes `...args` (rest) — not simple
                let (params, body, frame) = DEFAULT_DERIVED_CONSTRUCTOR.with(Clone::clone);
                (params, body, frame, false, false)
            } else {
                // default base constructor has an empty (simple) parameter list
                let (params, body, frame) = DEFAULT_BASE_CONSTRUCTOR.with(Clone::clone);
                (params, body, frame, false, true)
            };
        let ctor_func = JsFunction::User {
            bytecode_cache: Default::default(),
            name: Some(name.to_string()),
            params: ctor_params,
            frame: ctor_frame,
            body: ctor_body,
            closure: class_env.clone(),
            is_arrow: false,
            is_strict: true,
            is_generator: false,
            is_async: false,
            is_method: false,
            source_text: class_source_text.clone(),
            uses_arguments,
            has_simple_params,
        };

        let ctor_val = self.create_function_with(ctor_func, FunctionInit::ClassConstructor);
        // Computed keys and field initializers run JS (and so may collect)
        // before the constructor is reachable from any binding.
        self.gc_root_value(&ctor_val);

        // Mark derived class constructors
        if let Some(o) = (ctor_val)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
            && let Some(func_obj) = self.get_object_cell(o.id)
        {
            func_obj.borrow_mut().constructor_kind = if super_val.is_some() {
                if ctor_method.is_none() {
                    crate::interpreter::types::ConstructorKind::DefaultDerivedClass
                } else {
                    crate::interpreter::types::ConstructorKind::DerivedClass
                }
            } else {
                crate::interpreter::types::ConstructorKind::Class
            };
        }

        // Store constructor func for dynamic GetSuperConstructor (§13.3.7.2)
        if super_val.is_some() {
            class_env
                .borrow_mut()
                .declare("__constructor_func__", BindingKind::Const);
            class_env
                .borrow_mut()
                .initialize_binding("__constructor_func__", ctor_val.clone());
        }

        // Get the prototype object that was auto-created by create_function
        let proto_obj = if let Some(o) = (ctor_val)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            let proto_val = self.get_property_on_id(o.id, "prototype");
            if let Some(p) = (proto_val)
                .as_object_id()
                .map(|id| crate::types::JsObject { id })
            {
                self.get_object(p.id)
            } else {
                None
            }
        } else {
            None
        };

        // Set up inheritance
        if let Some(ref sv) = super_val
            && let Some(super_o) = (sv).as_object_id().map(|id| crate::types::JsObject { id })
        {
            let super_o_id = super_o.id;
            // Step 5.e: proto.[[Prototype]] = superclass.prototype_id
            let super_proto_val = match resolved_proto {
                Some(pv) => pv,
                None => match self.superclass_prototype(super_o_id, sv) {
                    Completion::Normal(v) => v,
                    other => return other,
                },
            };
            if let Some(sp) = (super_proto_val)
                .as_object_id()
                .map(|id| crate::types::JsObject { id })
                && let Some(super_proto) = self.get_object_cell(sp.id)
                && let Some(ref proto) = proto_obj
            {
                proto.borrow_mut().prototype_id = Some(super_proto.borrow().id.unwrap());
            } else if super_proto_val.is_null()
                && let Some(ref proto) = proto_obj
            {
                proto.borrow_mut().prototype_id = None;
            }
            // Step 7.a: F.[[Prototype]] = superclass (for static method inheritance)
            if let Some(o) = (ctor_val)
                .as_object_id()
                .map(|id| crate::types::JsObject { id })
                && let Some(ctor_obj) = self.get_object_cell(o.id)
                && let Some(super_obj) = self.get_object_cell(super_o_id)
            {
                ctor_obj.borrow_mut().prototype_id = Some(super_obj.borrow().id.unwrap());
            }
        }

        // Handle `extends null` — set prototype's [[Prototype]] to null
        if super_val.as_ref().is_some_and(JsValue::is_null)
            && let Some(ref proto) = proto_obj
        {
            proto.borrow_mut().prototype_id = None;
        }

        // Set __home_object__ in class_env for the constructor (which uses class_env as
        // its closure directly). Non-constructor methods get per-method closures that
        // shadow this with their own __home_object__ binding.
        let ctor_home = if let Some(ref p) = proto_obj {
            let pid = p.borrow().id.unwrap();
            JsValue::object(pid)
        } else {
            JsValue::UNDEFINED
        };
        class_env
            .borrow_mut()
            .declare("__home_object__", BindingKind::Const);
        class_env
            .borrow_mut()
            .initialize_binding("__home_object__", ctor_home);

        // Create environment for static field initializers with `this` = constructor
        let static_field_env = Environment::new_function_scope(Some(class_env.clone()));
        {
            let mut sfe = static_field_env.borrow_mut();
            sfe.set_this(ctor_val.clone());
            sfe.is_field_initializer = true;
            sfe.class_private_names = self.class_private_names.last().cloned();
            // Set __home_object__ for super property access in static field initializers.
            // Static field HomeObject = constructor.
            sfe.bindings.insert(
                "__home_object__",
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
            PublicField(JsPropertyKey, Option<Expression>),
            // (source_name, private name, initializer)
            PrivateField(String, PrivateId, Option<Expression>),
            Block(Vec<Statement>),
            AutoAccessor(JsPropertyKey, PrivateId, Option<Expression>),
        }
        let mut deferred_static: Vec<DeferredStatic> = Vec::new();

        for elem in body {
            match elem {
                ClassElement::Method(m) => {
                    if m.kind == ClassMethodKind::Constructor {
                        continue;
                    }
                    let (key, fn_name_for_key) = match &m.key {
                        PropertyKey::Identifier(s) => {
                            let key = JsPropertyKey::from(s.clone());
                            (key.clone(), key)
                        }
                        PropertyKey::String(s) => {
                            let key = literal_string_property_key(s);
                            (key.clone(), key)
                        }
                        PropertyKey::Number(n) => {
                            let key = JsPropertyKey::from(to_js_string(&JsValue::number(*n)));
                            (key.clone(), key)
                        }
                        PropertyKey::Computed(expr) => match self.eval_expr(expr, &class_env) {
                            Completion::Normal(v) => match self.to_property_key(&v) {
                                Ok(s) => {
                                    let name = s
                                        .symbol_function_name()
                                        .map_or_else(|| s.clone(), JsPropertyKey::from);
                                    (s, name)
                                }
                                Err(e) => return Completion::Throw(e),
                            },
                            other => return other,
                        },
                        PropertyKey::Private(name) => {
                            let branded = self.resolve_private_name(name, &class_env);
                            let priv_home_target = if m.is_static {
                                ctor_val.clone()
                            } else if let Some(ref p) = proto_obj {
                                let pid = p.borrow().id.unwrap();
                                JsValue::object(pid)
                            } else {
                                JsValue::UNDEFINED
                            };
                            let method_closure = Environment::new(Some(class_env.clone()));
                            method_closure
                                .borrow_mut()
                                .declare("__home_object__", BindingKind::Const);
                            method_closure
                                .borrow_mut()
                                .initialize_binding("__home_object__", priv_home_target);
                            let info = m.value.info();
                            let method_func = JsFunction::User {
                                bytecode_cache: Default::default(),
                                frame: info.frame.clone(),
                                name: Some(format!("#{name}")),
                                params: info.params.clone(),
                                body: m.value.body.clone(),
                                closure: method_closure,
                                is_arrow: false,
                                is_strict: true,
                                is_generator: m.value.is_generator,
                                is_async: m.value.is_async,
                                is_method: true,
                                source_text: m.value.source_text.clone(),
                                uses_arguments: info.uses_arguments,
                                has_simple_params: info.has_simple_params,
                            };
                            let method_val = self.create_function(method_func);

                            if m.is_static {
                                if let Some(o) = (ctor_val)
                                    .as_object_id()
                                    .map(|id| crate::types::JsObject { id })
                                    && let Some(func_obj) = self.get_object_cell(o.id)
                                {
                                    match m.kind {
                                        ClassMethodKind::Get => {
                                            let existing =
                                                func_obj.borrow().private_field(branded).cloned();
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
                                                .private_fields_mut()
                                                .insert(branded, elem);
                                        }
                                        ClassMethodKind::Set => {
                                            let existing =
                                                func_obj.borrow().private_field(branded).cloned();
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
                                                .private_fields_mut()
                                                .insert(branded, elem);
                                        }
                                        _ => {
                                            func_obj.borrow_mut().private_fields_mut().insert(
                                                branded,
                                                PrivateElement::Method(method_val),
                                            );
                                        }
                                    }
                                }
                            } else if let Some(o) = (ctor_val)
                                .as_object_id()
                                .map(|id| crate::types::JsObject { id })
                                && let Some(func_obj) = self.get_object_cell(o.id)
                            {
                                match m.kind {
                                    ClassMethodKind::Get => {
                                        let mut b = func_obj.borrow_mut();
                                        let mut found = false;
                                        for idef in b.instance_field_defs_mut().iter_mut() {
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
                                            b.instance_field_defs_mut().push(
                                                InstanceFieldDef::Private(
                                                    PrivateFieldDef::Accessor {
                                                        name: branded,
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
                                        for idef in b.instance_field_defs_mut().iter_mut() {
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
                                            b.instance_field_defs_mut().push(
                                                InstanceFieldDef::Private(
                                                    PrivateFieldDef::Accessor {
                                                        name: branded,
                                                        get: None,
                                                        set: Some(method_val),
                                                    },
                                                ),
                                            );
                                        }
                                    }
                                    _ => {
                                        func_obj.borrow_mut().instance_field_defs_mut().push(
                                            InstanceFieldDef::Private(PrivateFieldDef::Method {
                                                name: branded,
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
                        ClassMethodKind::Get => prefixed_function_name("get ", &fn_name_for_key),
                        ClassMethodKind::Set => prefixed_function_name("set ", &fn_name_for_key),
                        _ => fn_name_for_key.clone(),
                    };
                    let home_target = if m.is_static {
                        ctor_val.clone()
                    } else if let Some(ref p) = proto_obj {
                        let pid = p.borrow().id.unwrap();
                        JsValue::object(pid)
                    } else {
                        JsValue::UNDEFINED
                    };
                    let method_closure = Environment::new(Some(class_env.clone()));
                    method_closure
                        .borrow_mut()
                        .declare("__home_object__", BindingKind::Const);
                    method_closure
                        .borrow_mut()
                        .initialize_binding("__home_object__", home_target);
                    let info = m.value.info();
                    let method_func = JsFunction::User {
                        bytecode_cache: Default::default(),
                        frame: info.frame.clone(),
                        // create_function installs an empty configurable name, then
                        // SetFunctionName below replaces it without a lossy Rust String seam.
                        name: Some(String::new()),
                        params: info.params.clone(),
                        body: m.value.body.clone(),
                        closure: method_closure,
                        is_arrow: false,
                        is_strict: true,
                        is_generator: m.value.is_generator,
                        is_async: m.value.is_async,
                        is_method: true,
                        source_text: m.value.source_text.clone(),
                        uses_arguments: info.uses_arguments,
                        has_simple_params: info.has_simple_params,
                    };
                    let method_val = self.create_function(method_func);
                    self.set_function_name(&method_val, &method_display_name);

                    let target = if m.is_static {
                        if let Some(o) = (ctor_val)
                            .as_object_id()
                            .map(|id| crate::types::JsObject { id })
                        {
                            self.get_object(o.id)
                        } else {
                            None
                        }
                    } else {
                        proto_obj.clone()
                    };
                    if let Some(ref t) = target {
                        let ok = match m.kind {
                            ClassMethodKind::Get => {
                                let mut desc = t
                                    .borrow()
                                    .properties
                                    .get(&key)
                                    .map(PropertyRecord::to_descriptor)
                                    .unwrap_or(PropertyDescriptor {
                                        value: None,
                                        writable: None,
                                        get: None,
                                        set: None,
                                        enumerable: Some(false),
                                        configurable: Some(true),
                                    });
                                desc.get = Some(method_val);
                                desc.value = None;
                                desc.writable = None;
                                t.borrow_mut().define_own_property(key, desc)
                            }
                            ClassMethodKind::Set => {
                                let mut desc = t
                                    .borrow()
                                    .properties
                                    .get(&key)
                                    .map(PropertyRecord::to_descriptor)
                                    .unwrap_or(PropertyDescriptor {
                                        value: None,
                                        writable: None,
                                        get: None,
                                        set: None,
                                        enumerable: Some(false),
                                        configurable: Some(true),
                                    });
                                desc.set = Some(method_val);
                                desc.value = None;
                                desc.writable = None;
                                t.borrow_mut().define_own_property(key, desc)
                            }
                            _ => {
                                let desc = PropertyDescriptor::data(method_val, true, false, true);
                                t.borrow_mut().define_own_property(key, desc)
                            }
                        };
                        if !ok {
                            return Completion::Throw(
                                self.create_type_error("Cannot redefine non-configurable property"),
                            );
                        }
                    }
                }
                ClassElement::Property(p) => {
                    // Check if this is a private field
                    if let PropertyKey::Private(name) = &p.key {
                        let branded = self.resolve_private_name(name, &class_env);
                        if !p.is_static {
                            // Store instance private field definition
                            if let Some(o) = (ctor_val)
                                .as_object_id()
                                .map(|id| crate::types::JsObject { id })
                                && let Some(func_obj) = self.get_object(o.id)
                            {
                                func_obj.borrow_mut().instance_field_defs_mut().push(
                                    InstanceFieldDef::Private(PrivateFieldDef::Field {
                                        name: branded,
                                        source_name: name.clone(),
                                        initializer: p.value.clone(),
                                    }),
                                );
                            }
                        } else {
                            // Defer static private field initializer to phase 2
                            deferred_static.push(DeferredStatic::PrivateField(
                                name.clone(),
                                branded,
                                p.value.clone(),
                            ));
                        }
                        continue;
                    }
                    if p.is_static {
                        // Evaluate computed key NOW in phase 1, defer initializer to phase 2
                        let key = match &p.key {
                            PropertyKey::Identifier(s) => JsPropertyKey::from(s.clone()),
                            PropertyKey::String(s) => literal_string_property_key(s),
                            PropertyKey::Number(n) => {
                                JsPropertyKey::from(to_js_string(&JsValue::number(*n)))
                            }
                            PropertyKey::Computed(expr) => match self.eval_expr(expr, &class_env) {
                                Completion::Normal(v) => match self.to_property_key(&v) {
                                    Ok(s) => s,
                                    Err(e) => return Completion::Throw(e),
                                },
                                other => return other,
                            },
                            PropertyKey::Private(_) => unreachable!(),
                        };
                        if key.eq_str("prototype") {
                            return Completion::Throw(self.create_type_error(
                                "Classes may not have a static property named 'prototype'",
                            ));
                        }
                        deferred_static.push(DeferredStatic::PublicField(key, p.value.clone()));
                    } else {
                        // Instance field: evaluate computed key, store field def
                        let key = match &p.key {
                            PropertyKey::Identifier(s) => JsPropertyKey::from(s.clone()),
                            PropertyKey::String(s) => literal_string_property_key(s),
                            PropertyKey::Number(n) => {
                                JsPropertyKey::from(to_js_string(&JsValue::number(*n)))
                            }
                            PropertyKey::Computed(expr) => match self.eval_expr(expr, &class_env) {
                                Completion::Normal(v) => match self.to_property_key(&v) {
                                    Ok(s) => s,
                                    Err(e) => return Completion::Throw(e),
                                },
                                other => return other,
                            },
                            PropertyKey::Private(_) => unreachable!(),
                        };
                        if let Some(o) = (ctor_val)
                            .as_object_id()
                            .map(|id| crate::types::JsObject { id })
                            && let Some(func_obj) = self.get_object(o.id)
                        {
                            func_obj
                                .borrow_mut()
                                .instance_field_defs_mut()
                                .push(InstanceFieldDef::Public(key, p.value.clone()));
                        }
                    }
                }
                ClassElement::AutoAccessor(p) => {
                    // Private auto accessors: treat as private field (backing storage = the field itself)
                    if let PropertyKey::Private(name) = &p.key {
                        let branded = self.resolve_private_name(name, &class_env);
                        if !p.is_static {
                            if let Some(o) = (ctor_val)
                                .as_object_id()
                                .map(|id| crate::types::JsObject { id })
                                && let Some(func_obj) = self.get_object(o.id)
                            {
                                func_obj.borrow_mut().instance_field_defs_mut().push(
                                    InstanceFieldDef::Private(PrivateFieldDef::Field {
                                        name: branded,
                                        source_name: name.clone(),
                                        initializer: p.value.clone(),
                                    }),
                                );
                            }
                        } else {
                            deferred_static.push(DeferredStatic::PrivateField(
                                name.clone(),
                                branded,
                                p.value.clone(),
                            ));
                        }
                        continue;
                    }
                    let key = match &p.key {
                        PropertyKey::Identifier(s) => JsPropertyKey::from(s.clone()),
                        PropertyKey::String(s) => literal_string_property_key(s),
                        PropertyKey::Number(n) => {
                            JsPropertyKey::from(to_js_string(&JsValue::number(*n)))
                        }
                        PropertyKey::Computed(expr) => match self.eval_expr(expr, &class_env) {
                            Completion::Normal(v) => match self.to_property_key(&v) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            },
                            other => return other,
                        },
                        PropertyKey::Private(_) => unreachable!(),
                    };
                    let storage_slot = PrivateId(self.next_private_id);
                    self.next_private_id += 1;

                    let getter_slot = storage_slot;
                    let getter_func = JsFunction::native(
                        String::new(),
                        0,
                        move |interp, this, _args| {
                            let Some(obj_id) = this.as_object_id() else {
                                return Completion::Throw(interp.create_type_error(
                                    "Cannot read private member from an object whose class did not declare it",
                                ));
                            };
                            if let Some(obj) = interp.get_object_cell(obj_id)
                                && let Some(PrivateElement::Field(v)) =
                                    obj.borrow().private_field(getter_slot)
                            {
                                return Completion::Normal(v.clone());
                            }
                            Completion::Throw(interp.create_type_error(
                                "Cannot read private member from an object whose class did not declare it",
                            ))
                        },
                    );
                    let getter_val = self.create_function(getter_func);
                    self.set_function_name(&getter_val, &prefixed_function_name("get ", &key));

                    let setter_slot = storage_slot;
                    let setter_func = JsFunction::native(
                        String::new(),
                        1,
                        move |interp, this, args| {
                            let Some(obj_id) = this.as_object_id() else {
                                return Completion::Throw(interp.create_type_error(
                                    "Cannot write private member to an object whose class did not declare it",
                                ));
                            };
                            let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                            if let Some(obj) = interp.get_object_cell(obj_id)
                                && obj.borrow().has_private_field(setter_slot)
                            {
                                obj.borrow_mut()
                                    .private_fields_mut()
                                    .insert(setter_slot, PrivateElement::Field(val));
                                return Completion::Normal(JsValue::UNDEFINED);
                            }
                            Completion::Throw(interp.create_type_error(
                                "Cannot write private member to an object whose class did not declare it",
                            ))
                        },
                    );
                    let setter_val = self.create_function(setter_func);
                    self.set_function_name(&setter_val, &prefixed_function_name("set ", &key));

                    let target = if p.is_static {
                        if let Some(o) = (ctor_val)
                            .as_object_id()
                            .map(|id| crate::types::JsObject { id })
                        {
                            self.get_object(o.id)
                        } else {
                            None
                        }
                    } else {
                        proto_obj.clone()
                    };
                    if let Some(ref t) = target {
                        let desc = PropertyDescriptor {
                            value: None,
                            writable: None,
                            get: Some(getter_val),
                            set: Some(setter_val),
                            enumerable: Some(false),
                            configurable: Some(true),
                        };
                        t.borrow_mut().define_own_property(key.clone(), desc);
                    }

                    if p.is_static {
                        deferred_static.push(DeferredStatic::AutoAccessor(
                            key,
                            storage_slot,
                            p.value.clone(),
                        ));
                    } else if let Some(o) = (ctor_val)
                        .as_object_id()
                        .map(|id| crate::types::JsObject { id })
                        && let Some(func_obj) = self.get_object_cell(o.id)
                    {
                        func_obj.borrow_mut().instance_field_defs_mut().push(
                            InstanceFieldDef::AutoAccessorStorage(storage_slot, p.value.clone()),
                        );
                    }
                }
                ClassElement::StaticBlock(stmts) => {
                    // Defer static block execution to phase 2
                    deferred_static.push(DeferredStatic::Block(stmts.clone()));
                }
            }
        }

        // Initialize class name binding AFTER element evaluation (spec §15.7.14 step 27)
        if !class_binding_name.is_empty() {
            class_env
                .borrow_mut()
                .initialize_binding(class_binding_name, ctor_val.clone());
        }

        // Phase 2: Execute deferred static field initializers and static blocks.
        // Both are methods called with no NewTarget; their environments carry none.
        for deferred in deferred_static {
            match deferred {
                DeferredStatic::PublicField(key, initializer) => {
                    let val = if let Some(ref expr) = initializer {
                        match self.eval_expr(expr, &static_field_env) {
                            Completion::Normal(v) => {
                                if expr.is_anonymous_function_definition() {
                                    self.set_function_name(&v, &key);
                                }
                                v
                            }
                            other => return other,
                        }
                    } else {
                        JsValue::UNDEFINED
                    };
                    if let Some(o) = (ctor_val)
                        .as_object_id()
                        .map(|id| crate::types::JsObject { id })
                        && let Some(func_obj) = self.get_object_cell(o.id)
                    {
                        func_obj.borrow_mut().insert_value(key, val);
                    }
                }
                DeferredStatic::PrivateField(source_name, branded, initializer) => {
                    let display_name = format!("#{source_name}");
                    let val = if let Some(ref expr) = initializer {
                        match self.eval_expr(expr, &static_field_env) {
                            Completion::Normal(v) => {
                                if expr.is_anonymous_function_definition() {
                                    self.set_function_name(&v, &display_name);
                                }
                                v
                            }
                            other => return other,
                        }
                    } else {
                        JsValue::UNDEFINED
                    };
                    if let Some(o) = (ctor_val)
                        .as_object_id()
                        .map(|id| crate::types::JsObject { id })
                        && let Some(func_obj) = self.get_object_cell(o.id)
                    {
                        if !func_obj.borrow().extensible {
                            return Completion::Throw(self.create_type_error(
                                "Cannot add private field to non-extensible object",
                            ));
                        }
                        if func_obj.borrow().has_private_field(branded) {
                            return Completion::Throw(self.create_type_error(
                                "Cannot initialize private field twice on the same object",
                            ));
                        }
                        func_obj
                            .borrow_mut()
                            .private_fields_mut()
                            .insert(branded, PrivateElement::Field(val));
                    }
                }
                DeferredStatic::AutoAccessor(_key, storage_slot, initializer) => {
                    let val = if let Some(ref expr) = initializer {
                        match self.eval_expr(expr, &static_field_env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        }
                    } else {
                        JsValue::UNDEFINED
                    };
                    if let Some(o) = (ctor_val)
                        .as_object_id()
                        .map(|id| crate::types::JsObject { id })
                        && let Some(func_obj) = self.get_object_cell(o.id)
                    {
                        func_obj
                            .borrow_mut()
                            .private_fields_mut()
                            .insert(storage_slot, PrivateElement::Field(val));
                    }
                }
                DeferredStatic::Block(stmts) => {
                    let block_env = Environment::new_function_scope(Some(class_env.clone()));
                    {
                        let mut be = block_env.borrow_mut();
                        be.strict = true; // class bodies are always strict
                        be.set_this(ctor_val.clone());
                        be.bindings.insert(
                            "__home_object__",
                            Binding {
                                value: ctor_val.clone(),
                                kind: BindingKind::Const,
                                initialized: true,
                                deletable: false,
                            },
                        );
                        be.class_private_names = self.class_private_names.last().cloned();
                    }
                    let result = self.exec_statements(&stmts, &block_env);
                    let result = self.dispose_resources(&block_env, result);
                    match result {
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
    ///
    /// Each already-fetched `val` is rooted for the rest of the loop: a later
    /// key's getter (or proxy trap) can run arbitrary code that reaches a GC
    /// safepoint, and `result` is a plain Rust `Vec` the GC does not walk.
    pub(crate) fn copy_data_properties(
        &mut self,
        src_id: u64,
        src_val: &JsValue,
        excluded: &[JsPropertyKey],
    ) -> Result<Vec<(JsPropertyKey, JsValue)>, JsValue> {
        self.with_gc_root_scope(|interp| {
            // Ordinary sources: walk the stored keys directly. The
            // enumerability of each key is re-read right before its [[Get]]
            // because an earlier getter may have deleted or hidden it.
            if let Some(keys) = interp.get_object_cell(src_id).and_then(|c| {
                c.borrow()
                    .ordinary_own_property_keys(crate::interpreter::types::OwnKeyFilter::AllKeys)
            }) {
                let mut result = Vec::with_capacity(keys.len());
                for key in keys {
                    if !excluded.is_empty() && excluded.contains(&key) {
                        continue;
                    }
                    let Some(val) = interp.get_own_enumerable_value(src_id, &key, src_val)? else {
                        continue;
                    };
                    interp.gc_root_value(&val);
                    result.push((key, val));
                }
                return Ok(result);
            }
            let mut result = Vec::new();
            let keys = interp.proxy_own_keys(src_id)?;
            for key_val in keys {
                let key_str = interp.to_property_key(&key_val)?;
                if excluded.contains(&key_str) {
                    continue;
                }
                let is_enumerable = if interp.get_proxy_info(src_id).is_some() {
                    let target_proxy_val = interp.get_proxy_target_val(src_id);
                    match interp.invoke_proxy_trap(
                        src_id,
                        "getOwnPropertyDescriptor",
                        vec![target_proxy_val, key_val.clone()],
                    ) {
                        Ok(Some(v)) => {
                            if v.is_undefined() {
                                continue;
                            }
                            if let Some(dobj) =
                                (v).as_object_id().map(|id| crate::types::JsObject { id })
                                && let Some(desc_rc) = interp.get_object_cell(dobj.id)
                            {
                                match desc_rc.borrow().get_property_value("enumerable") {
                                    Some(ev) => interp.to_boolean_val(&ev),
                                    None => false,
                                }
                            } else {
                                continue;
                            }
                        }
                        Ok(None) => {
                            if let Some(obj) = interp.get_object_cell(src_id) {
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
                } else if let Some(obj) = interp.get_object_cell(src_id) {
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
                let val = match interp.get_object_property(src_id, &key_str, src_val) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::UNDEFINED,
                };
                interp.gc_root_value(&val);
                result.push((key_str, val));
            }
            Ok(result)
        })
    }

    pub(super) fn eval_object_literal(&mut self, props: &[Property], env: &EnvRef) -> Completion {
        let gc_frame = self.gc_root_frame();
        let result = (|| {
            let mut obj_data = JsObjectData::new();
            obj_data.prototype_id = self.realm().object_prototype;
            let mut method_values: Vec<JsValue> = Vec::new();
            for prop in props {
                let (key, fn_name_for_key) = match static_property_key(&prop.key) {
                    Some(key) => (key.clone(), key),
                    None => match &prop.key {
                        PropertyKey::Computed(expr) => {
                            let v = match self.eval_expr(expr, env) {
                                Completion::Normal(v) => v,
                                other => return other,
                            };
                            self.gc_root_value(&v);
                            let pk = match self.to_property_key(&v) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };
                            let name = pk
                                .symbol_function_name()
                                .map_or_else(|| pk.clone(), JsPropertyKey::from);
                            (pk, name)
                        }
                        _ => {
                            return Completion::Throw(self.create_type_error(
                                "Private names are not valid in object literals",
                            ));
                        }
                    },
                };
                if prop.method {
                    self.next_function_is_method = true;
                }
                let value = match self.eval_expr(&prop.value, env) {
                    Completion::Normal(v) => v,
                    other => {
                        self.next_function_is_method = false;
                        return other;
                    }
                };
                self.gc_root_value(&value);
                self.next_function_is_method = false;
                // Handle spread — CopyDataProperties (§7.3.26)
                if let Expression::Spread(inner) = &prop.value {
                    let spread_val = match self.eval_expr(inner, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    self.gc_root_value(&spread_val);
                    if let Some(o) = (spread_val)
                        .as_object_id()
                        .map(|id| crate::types::JsObject { id })
                    {
                        let src_id = o.id;
                        match self.copy_data_properties(src_id, &spread_val, &[]) {
                            Ok(pairs) => {
                                obj_data.properties.reserve(pairs.len());
                                for (k, v) in pairs {
                                    self.gc_root_value(&v);
                                    obj_data.insert_value_prekeyed(k, v);
                                }
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    continue;
                }
                let spec = LiteralPropertySpec {
                    kind: prop.kind,
                    plain_init: !prop.computed && !prop.shorthand && !prop.method,
                    is_method: prop.method,
                    name_value: match prop.kind {
                        PropertyKind::Init => prop.value.is_anonymous_function_definition(),
                        PropertyKind::Get | PropertyKind::Set => true,
                    },
                };
                self.define_literal_property(
                    &mut obj_data,
                    crate::interpreter::key_intern::intern_js_key(key),
                    &fn_name_for_key,
                    spec,
                    value,
                    &mut method_values,
                );
            }
            let obj_val = self.finish_object_literal(obj_data, &method_values);
            self.gc_root_value(&obj_val);
            Completion::Normal(obj_val)
        })();
        self.gc_unroot_frame(gc_frame);
        result
    }

    /// Defines one property of an object literal on the object under
    /// construction (PropertyDefinitionEvaluation, sec-runtime-semantics-
    /// propertydefinitionevaluation): data properties, accessors (merged with
    /// an earlier half of the same pair), `__proto__: v` and methods. `key`
    /// must already be interned. Shared by the tree-walker and the VM's
    /// `NewObject`, so both keep one set of rules.
    pub(crate) fn define_literal_property(
        &self,
        obj_data: &mut JsObjectData,
        key: JsPropertyKey,
        fn_name_for_key: &JsPropertyKey,
        spec: LiteralPropertySpec,
        value: JsValue,
        method_values: &mut Vec<JsValue>,
    ) {
        match spec.kind {
            PropertyKind::Get | PropertyKind::Set => {
                let is_get = spec.kind == PropertyKind::Get;
                if spec.name_value {
                    self.set_function_name(
                        &value,
                        &prefixed_function_name(
                            if is_get { "get " } else { "set " },
                            fn_name_for_key,
                        ),
                    );
                }
                method_values.push(value.clone());
                let mut desc = obj_data
                    .properties
                    .get(&key)
                    .map(PropertyRecord::to_descriptor)
                    .unwrap_or(PropertyDescriptor {
                        value: None,
                        writable: None,
                        get: None,
                        set: None,
                        enumerable: Some(true),
                        configurable: Some(true),
                    });
                if is_get {
                    desc.get = Some(value);
                } else {
                    desc.set = Some(value);
                }
                desc.value = None;
                desc.writable = None;
                obj_data.insert_property(key, desc);
            }
            PropertyKind::Init => {
                // __proto__: value sets [[Prototype]] per spec §13.2.5.5
                // Only plain property init, not methods, computed, or shorthand
                if spec.sets_prototype(&key) {
                    if let Some(id) = value.as_object_id() {
                        obj_data.prototype_id = Some(id);
                    } else if value.is_null() {
                        obj_data.prototype_id = None;
                    }
                } else {
                    if spec.name_value {
                        self.set_function_name(&value, fn_name_for_key);
                    }
                    if spec.is_method {
                        method_values.push(value.clone());
                    }
                    obj_data.insert_value_prekeyed(key, value);
                }
            }
        }
    }

    /// Allocates the finished literal and gives each concise method, getter
    /// and setter its [[HomeObject]].
    pub(crate) fn finish_object_literal(
        &mut self,
        obj_data: JsObjectData,
        method_values: &[JsValue],
    ) -> JsValue {
        let id = self.alloc_object(obj_data);
        let obj_val = JsValue::object(id);
        self.install_home_object(&obj_val, method_values);
        obj_val
    }

    /// Wraps each method's closure environment in one binding
    /// `__home_object__` to `home`, which is how `super` property accesses
    /// find the object the method was defined on.
    fn install_home_object(&self, home: &JsValue, method_values: &[JsValue]) {
        for val in method_values {
            let Some(fo) = val.as_object_id() else {
                continue;
            };
            let Some(func_obj) = self.get_object_cell(fo) else {
                continue;
            };
            let old_closure = if let Some(JsFunction::User { closure, .. }) =
                func_obj.borrow().callable.as_deref()
            {
                Some(closure.clone())
            } else {
                None
            };
            if let Some(old_closure) = old_closure {
                let wrapper = Environment::new(Some(old_closure));
                wrapper
                    .borrow_mut()
                    .declare("__home_object__", BindingKind::Const);
                wrapper
                    .borrow_mut()
                    .initialize_binding("__home_object__", home.clone());
                if let Some(JsFunction::User { closure, .. }) =
                    func_obj.borrow_mut().callable.as_deref_mut()
                {
                    *closure = wrapper;
                }
            }
        }
    }

    /// `ArrayAppend`: appends literal elements to an array under
    /// construction.
    pub(crate) fn array_literal_append(&mut self, array: &JsValue, values: Vec<JsValue>) {
        let id = array
            .as_object_id()
            .expect("ArrayAppend target is an array literal");
        let cell = self.get_object_cell_expect(id);
        let mut obj = cell.borrow_mut();
        let start = obj.array_len();
        for (offset, value) in values.iter().enumerate() {
            if value.is_undefined() {
                obj.insert_value((start + offset).to_string(), value.clone());
            }
        }
        let elements = obj
            .array_elements_mut()
            .expect("ArrayAppend target is an array literal");
        elements.extend(values);
        let len = elements.len();
        obj.set_dense_array_length(len);
    }

    /// `ObjectAppend`: defines the next chunk of a large object literal's
    /// properties on the object `NewObject` allocated.
    pub(crate) fn extend_object_literal(
        &self,
        object: &JsValue,
        shape: &crate::interpreter::bytecode::chunk::ObjectShape,
        values: &[JsValue],
    ) {
        debug_assert_eq!(shape.props.len(), values.len());
        let id = object
            .as_object_id()
            .expect("ObjectAppend target is an object literal");
        let mut method_values: Vec<JsValue> = Vec::new();
        {
            let mut obj_data = self.get_object_cell_expect(id).borrow_mut();
            obj_data.properties.reserve(values.len());
            for (prop, value) in shape.props.iter().zip(values) {
                self.define_literal_property(
                    &mut obj_data,
                    prop.key.clone(),
                    &prop.key,
                    prop.spec,
                    value.clone(),
                    &mut method_values,
                );
            }
            // A later `__proto__: v` changes the prototype of an object that
            // already has a shape id.
            obj_data.shape_id = crate::interpreter::types::fresh_shape_id();
        }
        self.install_home_object(object, &method_values);
    }

    /// `NewObject`: builds a literal whose properties are all statically
    /// keyed from the operand values, in source order.
    pub(crate) fn build_object_literal(
        &mut self,
        shape: &crate::interpreter::bytecode::chunk::ObjectShape,
        values: &[JsValue],
    ) -> JsValue {
        debug_assert_eq!(shape.props.len(), values.len());
        let mut obj_data = JsObjectData::new();
        obj_data.prototype_id = self.realm().object_prototype;
        obj_data.properties.reserve(values.len());
        let mut method_values: Vec<JsValue> = Vec::new();
        for (prop, value) in shape.props.iter().zip(values) {
            if shape.distinct_data && !prop.spec.name_value {
                obj_data.push_fresh_value(prop.key.clone(), value.clone());
                continue;
            }
            self.define_literal_property(
                &mut obj_data,
                prop.key.clone(),
                &prop.key,
                prop.spec,
                value.clone(),
                &mut method_values,
            );
        }
        self.finish_object_literal(obj_data, &method_values)
    }
}

/// How one statically keyed property of an object literal is defined.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LiteralPropertySpec {
    pub(crate) kind: PropertyKind,
    /// Not computed, shorthand or a method: the only form in which
    /// `__proto__: v` sets the prototype.
    pub(crate) plain_init: bool,
    pub(crate) is_method: bool,
    /// The value is an anonymous function definition the literal must still
    /// name after the key (the VM names its closures when it creates them).
    pub(crate) name_value: bool,
}

impl LiteralPropertySpec {
    /// `__proto__: v` in its plain form, which sets the prototype instead of
    /// defining a property (and is not a NamedEvaluation position).
    pub(crate) fn sets_prototype(&self, key: &JsPropertyKey) -> bool {
        self.kind == PropertyKind::Init && self.plain_init && key.eq_str("__proto__")
    }
}
