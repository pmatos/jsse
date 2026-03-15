use super::*;

impl Interpreter {
    pub(super) fn get_template_object(&mut self, tmpl: &TemplateLiteral) -> JsValue {
        let cache_key = tmpl.id;
        if let Some(&obj_id) = self.realm().template_cache.get(&cache_key)
            && self.get_object(obj_id).is_some()
        {
            return JsValue::Object(crate::types::JsObject { id: obj_id });
        }

        let cooked_vals: Vec<JsValue> = tmpl
            .quasis
            .iter()
            .map(|q| match q {
                Some(cu) => JsValue::String(JsString {
                    code_units: cu.clone(),
                }),
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

    pub(super) fn create_frozen_template_array(&mut self, values: Vec<JsValue>) -> JsValue {
        let len = values.len();
        let mut obj_data = JsObjectData::new();
        obj_data.prototype = self
            .realm()
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

    pub(super) fn eval_literal(&mut self, lit: &Literal) -> JsValue {
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
                obj.prototype = self
                    .realm()
                    .regexp_prototype
                    .clone()
                    .or(self.realm().object_prototype.clone());
                obj.class_name = "RegExp".to_string();
                let source_js = if pattern.is_empty() {
                    JsString::from_str("(?:)")
                } else {
                    crate::interpreter::builtins::regexp::regex_output_to_js_string(pattern)
                };
                obj.regexp_original_source = Some(source_js);
                obj.regexp_original_flags = Some(JsString::from_str(flags));
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
        obj.prototype = self
            .realm()
            .regexp_prototype
            .clone()
            .or(self.realm().object_prototype.clone());
        obj.class_name = "RegExp".to_string();
        let source_str = if pattern.is_empty() { "(?:)" } else { pattern };
        obj.regexp_original_source = Some(JsString::from_str(source_str));
        obj.regexp_original_flags = Some(JsString::from_str(flags));
        obj.insert_property(
            "lastIndex".to_string(),
            PropertyDescriptor::data(JsValue::Number(0.0), true, false, false),
        );
        let rc = Rc::new(RefCell::new(obj));
        let id = self.allocate_object_slot(rc);
        JsValue::Object(crate::types::JsObject { id })
    }

    pub(super) fn eval_array_literal(
        &mut self,
        elements: &[Option<Expression>],
        env: &EnvRef,
    ) -> Completion {
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
        class_binding_name: &str,
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
                ClassElement::AutoAccessor(p) => {
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
        let result = self.eval_class_inner(
            name,
            class_binding_name,
            super_class,
            body,
            env,
            class_source_text,
        );
        self.class_private_names.pop();
        result
    }

    pub(super) fn eval_class_inner(
        &mut self,
        name: &str,
        class_binding_name: &str,
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

        // Evaluate super class in class_env context (spec step 6a-6b)
        let super_val = if let Some(sc) = super_class {
            match self.eval_expr(sc, &class_env) {
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

        if let Some(ref sv) = super_val {
            class_env
                .borrow_mut()
                .declare("__super__", BindingKind::Const);
            class_env
                .borrow_mut()
                .initialize_binding("__super__", sv.clone());
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
                captured_new_target: None,
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
                captured_new_target: None,
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
                captured_new_target: None,
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
                if ctor_method.is_none() {
                    func_obj.borrow_mut().is_default_derived_constructor = true;
                }
            }
            // Per spec §14.6.13: class .prototype is {writable: false, enumerable: false, configurable: false}
            let proto_val_for_desc = func_obj.borrow().get_property("prototype");
            func_obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val_for_desc, false, false, false),
            );
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
        {
            let super_o_id = super_o.id;
            let sv_clone = sv.clone();
            // Step 5.e: proto.[[Prototype]] = superclass.prototype
            // Must use Get() to invoke accessor properties (e.g. getter-defined prototype)
            let super_proto_val = match self.get_object_property(super_o_id, "prototype", &sv_clone)
            {
                Completion::Normal(v) => v,
                other => return other,
            };
            // Validate: must be Object or null
            match &super_proto_val {
                JsValue::Object(_) | JsValue::Null => {}
                _ => {
                    return Completion::Throw(self.create_type_error(
                        "Class extends value does not have valid prototype property",
                    ));
                }
            }
            if let JsValue::Object(ref sp) = super_proto_val
                && let Some(super_proto) = self.get_object(sp.id)
                && let Some(ref proto) = proto_obj
            {
                proto.borrow_mut().prototype = Some(super_proto);
            }
            // Step 7.a: F.[[Prototype]] = superclass (for static method inheritance)
            if let JsValue::Object(ref o) = ctor_val
                && let Some(ctor_obj) = self.get_object(o.id)
                && let Some(super_obj) = self.get_object(super_o_id)
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

        // Set __home_object__ in class_env for the constructor (which uses class_env as
        // its closure directly). Non-constructor methods get per-method closures that
        // shadow this with their own __home_object__ binding.
        let ctor_home = if let Some(ref p) = proto_obj {
            let pid = p.borrow().id.unwrap();
            JsValue::Object(crate::types::JsObject { id: pid })
        } else {
            JsValue::Undefined
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
            // (source_name, branded_name, initializer)
            PrivateField(String, String, Option<Expression>),
            Block(Vec<Statement>),
            AutoAccessor(String, String, Option<Expression>),
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
                        PropertyKey::Computed(expr) => match self.eval_expr(expr, &class_env) {
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
                            let priv_home_target = if m.is_static {
                                ctor_val.clone()
                            } else if let Some(ref p) = proto_obj {
                                let pid = p.borrow().id.unwrap();
                                JsValue::Object(crate::types::JsObject { id: pid })
                            } else {
                                JsValue::Undefined
                            };
                            let method_closure = Environment::new(Some(class_env.clone()));
                            method_closure
                                .borrow_mut()
                                .declare("__home_object__", BindingKind::Const);
                            method_closure
                                .borrow_mut()
                                .initialize_binding("__home_object__", priv_home_target);
                            let method_func = JsFunction::User {
                                name: Some(format!("#{name}")),
                                params: m.value.params.clone(),
                                body: m.value.body.clone(),
                                closure: method_closure,
                                is_arrow: false,
                                is_strict: true,
                                is_generator: m.value.is_generator,
                                is_async: m.value.is_async,
                                is_method: true,
                                source_text: m.value.source_text.clone(),
                                captured_new_target: None,
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
                    let home_target = if m.is_static {
                        ctor_val.clone()
                    } else if let Some(ref p) = proto_obj {
                        let pid = p.borrow().id.unwrap();
                        JsValue::Object(crate::types::JsObject { id: pid })
                    } else {
                        JsValue::Undefined
                    };
                    let method_closure = Environment::new(Some(class_env.clone()));
                    method_closure
                        .borrow_mut()
                        .declare("__home_object__", BindingKind::Const);
                    method_closure
                        .borrow_mut()
                        .initialize_binding("__home_object__", home_target);
                    let method_func = JsFunction::User {
                        name: Some(method_display_name),
                        params: m.value.params.clone(),
                        body: m.value.body.clone(),
                        closure: method_closure,
                        is_arrow: false,
                        is_strict: true,
                        is_generator: m.value.is_generator,
                        is_async: m.value.is_async,
                        is_method: true,
                        source_text: m.value.source_text.clone(),
                        captured_new_target: None,
                    };
                    let method_val = self.create_function(method_func);

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
                        let ok = match m.kind {
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
                                t.borrow_mut().define_own_property(key, desc)
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
                            PropertyKey::Identifier(s) | PropertyKey::String(s) => s.clone(),
                            PropertyKey::Number(n) => to_js_string(&JsValue::Number(*n)),
                            PropertyKey::Computed(expr) => match self.eval_expr(expr, &class_env) {
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
                            PropertyKey::Computed(expr) => match self.eval_expr(expr, &class_env) {
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
                ClassElement::AutoAccessor(p) => {
                    // Private auto accessors: treat as private field (backing storage = the field itself)
                    if let PropertyKey::Private(name) = &p.key {
                        let branded = self.resolve_private_name(name, &class_env);
                        if !p.is_static {
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
                            deferred_static.push(DeferredStatic::PrivateField(
                                name.clone(),
                                branded,
                                p.value.clone(),
                            ));
                        }
                        continue;
                    }
                    let key = match &p.key {
                        PropertyKey::Identifier(s) | PropertyKey::String(s) => s.clone(),
                        PropertyKey::Number(n) => to_js_string(&JsValue::Number(*n)),
                        PropertyKey::Computed(expr) => match self.eval_expr(expr, &class_env) {
                            Completion::Normal(v) => match self.to_property_key(&v) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            },
                            other => return other,
                        },
                        PropertyKey::Private(_) => unreachable!(),
                    };
                    let slot_id = self.next_auto_accessor_id;
                    self.next_auto_accessor_id += 1;
                    let storage_slot = format!("__auto_accessor_{slot_id}");

                    let getter_slot = storage_slot.clone();
                    let getter_func = JsFunction::native(
                        format!("get {key}"),
                        0,
                        move |interp, this, _args| {
                            let obj_id = match this {
                                JsValue::Object(o) => o.id,
                                _ => {
                                    return Completion::Throw(interp.create_type_error(
                                        "Cannot read private member from an object whose class did not declare it",
                                    ));
                                }
                            };
                            if let Some(obj) = interp.get_object(obj_id)
                                && let Some(PrivateElement::Field(v)) =
                                    obj.borrow().private_fields.get(&getter_slot)
                            {
                                return Completion::Normal(v.clone());
                            }
                            Completion::Throw(interp.create_type_error(
                                "Cannot read private member from an object whose class did not declare it",
                            ))
                        },
                    );
                    let getter_val = self.create_function(getter_func);

                    let setter_slot = storage_slot.clone();
                    let setter_func = JsFunction::native(
                        format!("set {key}"),
                        1,
                        move |interp, this, args| {
                            let obj_id = match this {
                                JsValue::Object(o) => o.id,
                                _ => {
                                    return Completion::Throw(interp.create_type_error(
                                        "Cannot write private member to an object whose class did not declare it",
                                    ));
                                }
                            };
                            let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                            if let Some(obj) = interp.get_object(obj_id)
                                && obj.borrow().private_fields.contains_key(&setter_slot)
                            {
                                obj.borrow_mut()
                                    .private_fields
                                    .insert(setter_slot.clone(), PrivateElement::Field(val));
                                return Completion::Normal(JsValue::Undefined);
                            }
                            Completion::Throw(interp.create_type_error(
                                "Cannot write private member to an object whose class did not declare it",
                            ))
                        },
                    );
                    let setter_val = self.create_function(setter_func);

                    let target = if p.is_static {
                        if let JsValue::Object(ref o) = ctor_val {
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
                    } else if let JsValue::Object(ref o) = ctor_val
                        && let Some(func_obj) = self.get_object(o.id)
                    {
                        func_obj.borrow_mut().class_instance_field_defs.push(
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

        // Phase 2: Execute deferred static field initializers and static blocks
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
                        JsValue::Undefined
                    };
                    if let JsValue::Object(ref o) = ctor_val
                        && let Some(func_obj) = self.get_object(o.id)
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
                        JsValue::Undefined
                    };
                    if let JsValue::Object(ref o) = ctor_val
                        && let Some(func_obj) = self.get_object(o.id)
                    {
                        if !func_obj.borrow().extensible {
                            return Completion::Throw(self.create_type_error(
                                "Cannot add private field to non-extensible object",
                            ));
                        }
                        if func_obj.borrow().private_fields.contains_key(&branded) {
                            return Completion::Throw(self.create_type_error(
                                "Cannot initialize private field twice on the same object",
                            ));
                        }
                        func_obj
                            .borrow_mut()
                            .private_fields
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
                        JsValue::Undefined
                    };
                    if let JsValue::Object(ref o) = ctor_val
                        && let Some(func_obj) = self.get_object(o.id)
                    {
                        func_obj
                            .borrow_mut()
                            .private_fields
                            .insert(storage_slot, PrivateElement::Field(val));
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

    pub(super) fn eval_object_literal(&mut self, props: &[Property], env: &EnvRef) -> Completion {
        let mut obj_data = JsObjectData::new();
        obj_data.prototype = self.realm().object_prototype.clone();
        let mut method_values: Vec<JsValue> = Vec::new();
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
            self.next_function_is_method = false;
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
                    method_values.push(value.clone());
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
                    method_values.push(value.clone());
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
                    // Only plain property init, not methods, computed, or shorthand
                    if key == "__proto__" && !prop.computed && !prop.shorthand && !prop.method {
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
                        if prop.method {
                            method_values.push(value.clone());
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
        {
            for val in &method_values {
                if let JsValue::Object(fo) = val
                    && let Some(func_obj) = self.get_object(fo.id)
                {
                    let old_closure = if let Some(JsFunction::User { ref closure, .. }) =
                        func_obj.borrow().callable
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
                            .initialize_binding("__home_object__", obj_val.clone());
                        if let Some(JsFunction::User {
                            ref mut closure, ..
                        }) = func_obj.borrow_mut().callable
                        {
                            *closure = wrapper;
                        }
                    }
                    // Methods must not have own caller/arguments (spec §15.4)
                    func_obj.borrow_mut().properties.remove("caller");
                    func_obj.borrow_mut().properties.remove("arguments");
                }
            }
        }
        Completion::Normal(obj_val)
    }
}
