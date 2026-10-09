use super::super::*;

impl Interpreter {
    pub(super) fn setup_function_constructors(&mut self) {
        self.setup_eval();
        self.setup_function_constructor();
        self.retrofit_function_prototype();
        self.install_function_prototype();
        self.link_native_error_constructors();
        self.setup_async_function_prototype();
        self.setup_async_function_constructor();
        self.setup_generator_function_constructor();
        self.setup_async_generator_function_constructor();
    }

    fn setup_eval(&mut self) {
        // eval
        {
            let eval_realm_id = self.current_realm_id;
            let eval_fn = self.create_function(JsFunction::native(
                "eval".to_string(),
                1,
                move |interp, _this, args| {
                    // Indirect eval uses the eval function's realm
                    let old_realm = interp.current_realm_id;
                    interp.current_realm_id = eval_realm_id;
                    let global = interp.realm().global_env.clone();
                    let result = interp.perform_eval(args, false, false, &global);
                    interp.current_realm_id = old_realm;
                    result
                },
            ));
            if let Some(eval_id) = eval_fn.as_object_id() {
                self.realm_mut().builtin_eval_id = Some(eval_id);
            }
            let global_env = self.realm().global_env.clone();
            global_env.borrow_mut().declare("eval", BindingKind::Var);
            let _ = self.env_set(&global_env, "eval", eval_fn);
        }

        self.register_global_fn(
            "$DONOTEVALUATE",
            BindingKind::Var,
            JsFunction::native("$DONOTEVALUATE".to_string(), 0, |_interp, _this, _args| {
                Completion::Throw(JsValue::string(JsString::from_str(
                    "Test262: $DONOTEVALUATE was called",
                )))
            }),
        );
    }

    fn setup_function_constructor(&mut self) {
        // Function constructor
        // Capture realm_id so that functions created by `new other.Function()` are
        // registered in the realm where the Function constructor was defined (§10.3).
        {
            let fn_ctor_realm_id = self.current_realm_id;
            let fn_ctor_fn = self.create_function(JsFunction::constructor(
                "Function".to_string(),
                1,
                move |interp, _this, args| {
                    let (params_str, body_str) = if args.is_empty() {
                        (String::new(), String::new())
                    } else if args.len() == 1 {
                        match interp.to_string_value(&args[0]) {
                            Ok(s) => (String::new(), s),
                            Err(e) => return Completion::Throw(e),
                        }
                    } else {
                        let mut params = Vec::new();
                        for arg in &args[..args.len() - 1] {
                            match interp.to_string_value(arg) {
                                Ok(s) => params.push(s),
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        let body = match interp.to_string_value(args.last().unwrap()) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        };
                        (params.join(","), body)
                    };

                    // Per spec §20.2.1.1: parse parameters and body independently
                    // to reject cases like Function("/*", "*/) {") where a comment
                    // spans the parameter/body boundary.
                    if !params_str.is_empty() {
                        let params_test = format!("(function({}\n) {{}})", params_str);
                        match parser::Parser::new(&params_test).and_then(|mut p| p.parse_program())
                        {
                            Ok(_) => {}
                            Err(e) => {
                                return Completion::Throw(
                                    interp.create_error("SyntaxError", &format!("{}", e)),
                                );
                            }
                        }
                    }
                    {
                        let body_test = format!("(function() {{\n{}\n}})", body_str);
                        match parser::Parser::new(&body_test).and_then(|mut p| p.parse_program()) {
                            Ok(_) => {}
                            Err(e) => {
                                return Completion::Throw(
                                    interp.create_error("SyntaxError", &format!("{}", e)),
                                );
                            }
                        }
                    }

                    let fn_source_text =
                        format!("function anonymous({}\n) {{\n{}\n}}", params_str, body_str);
                    let source = format!(
                        "(function anonymous({}\n) {{\n{}\n}})",
                        params_str, body_str
                    );
                    let mut p = match parser::Parser::new(&source) {
                        Ok(p) => p,
                        Err(e) => {
                            return Completion::Throw(
                                interp.create_error("SyntaxError", &format!("{}", e)),
                            );
                        }
                    };
                    let program = match p.parse_program() {
                        Ok(prog) => prog,
                        Err(e) => {
                            return Completion::Throw(
                                interp.create_error("SyntaxError", &format!("{}", e)),
                            );
                        }
                    };

                    if let Some(Statement::Expression(Expression::Function(fe))) =
                        program.body.as_slice().first()
                    {
                        let is_strict = fe.body_is_strict;
                        // Use the realm of the Function constructor, not the caller's realm
                        let old_realm = interp.current_realm_id;
                        interp.current_realm_id = fn_ctor_realm_id;
                        let global_env = interp.realm().global_env.clone();
                        interp.current_realm_id = old_realm;
                        let dynamic_fn_env = Environment::new(Some(global_env));
                        dynamic_fn_env.borrow_mut().strict = false;
                        let js_func = JsFunction::User {
                            name: Some("anonymous".to_string()),
                            params: Rc::new(fe.params.clone()),
                            body: fe.body.clone(),
                            closure: dynamic_fn_env,
                            is_arrow: false,
                            is_strict,
                            is_generator: false,
                            is_async: false,
                            is_method: false,
                            source_text: Some(fn_source_text.into()),
                            captured_new_target: None,
                            uses_arguments: true, // conservative for dynamic Function()
                            has_simple_params: crate::ast::params_are_simple(&fe.params),
                        };
                        // Create function in the Function constructor's realm
                        let old_realm = interp.current_realm_id;
                        interp.current_realm_id = fn_ctor_realm_id;
                        let result = interp.create_function(js_func);
                        interp.current_realm_id = old_realm;
                        // OrdinaryCreateFromConstructor — realm-aware prototype
                        let proto = match interp
                            .get_prototype_from_new_target_realm(|realm| realm.function_prototype)
                        {
                            Ok(p) => p,
                            Err(e) => return Completion::Throw(e),
                        };
                        if let Some(p) = proto
                            && let Some(result_id) = result.as_object_id()
                            && let Some(fobj) = interp.get_object_cell(result_id)
                        {
                            fobj.borrow_mut().prototype_id = Some(p);
                        }
                        Completion::Normal(result)
                    } else {
                        Completion::Throw(
                            interp.create_error("SyntaxError", "Failed to parse function"),
                        )
                    }
                },
            ));
            // Parse-before-getprototype: body is parsed before prototype lookup
            if let Some(function_id) = fn_ctor_fn.as_object_id()
                && let Some(func_obj) = self.get_object_cell(function_id)
            {
                func_obj.borrow_mut().deferred_construct = true;
            }
            let global_env = self.realm().global_env.clone();
            global_env
                .borrow_mut()
                .declare("Function", BindingKind::Var);
            let _ = self.env_set(&global_env, "Function", fn_ctor_fn);
        }
    }

    fn retrofit_function_prototype(&mut self) {
        // Per spec §20.2.3, Function.prototype is itself a function object.
        // If we already have an early-created function_prototype, update it;
        // otherwise update the auto-created one from the Function constructor.
        {
            let fp = self.realm().function_prototype;
            if let Some(fp_id) = fp {
                // Already has callable from early init, but ensure length/name
                let mut b = self.get_object_cell_expect(fp_id).borrow_mut();
                if !b.properties.contains_key("length") {
                    b.insert_property(
                        "length".to_string(),
                        PropertyDescriptor::data(JsValue::number(0.0), false, false, true),
                    );
                }
                if !b.properties.contains_key("name") {
                    b.insert_property(
                        "name".to_string(),
                        PropertyDescriptor::data(
                            JsValue::string(JsString::from_str("")),
                            false,
                            false,
                            true,
                        ),
                    );
                }
            } else {
                let func_val = self.get_global_var("Function");
                if let Some(function_id) = func_val.and_then(|v| v.as_object_id()) {
                    let pv = self.get_property_on_id(function_id, "prototype");
                    if let Some(proto_id) = pv.as_object_id()
                        && let Some(proto_obj) = self.get_object_cell(proto_id)
                    {
                        proto_obj.borrow_mut().callable = Some(JsFunction::native(
                            "".to_string(),
                            0,
                            |_interp, _this, _args| Completion::Normal(JsValue::UNDEFINED),
                        ));
                        proto_obj.borrow_mut().insert_property(
                            "length".to_string(),
                            PropertyDescriptor::data(JsValue::number(0.0), false, false, true),
                        );
                        proto_obj.borrow_mut().insert_property(
                            "name".to_string(),
                            PropertyDescriptor::data(
                                JsValue::string(JsString::from_str("")),
                                false,
                                false,
                                true,
                            ),
                        );
                    }
                }
            }
        }
    }

    fn install_function_prototype(&mut self) {
        // Store Function.prototype for use as [[Prototype]] of all function objects
        {
            let func_val = self.get_global_var("Function");
            if let Some(function_id) = func_val.and_then(|v| v.as_object_id())
                && let Some(func_data) = self.get_object(function_id)
            {
                // Use the early-created function_prototype if available, otherwise
                // fall back to the auto-created one from create_function
                let fp = if let Some(existing_fp_id) = self.realm().function_prototype
                    && let Some(existing_fp) = self.get_object(existing_fp_id)
                {
                    // Replace the Function constructor's .prototype with our early object
                    let fp_val = JsValue::object(existing_fp_id);
                    func_data.borrow_mut().insert_property(
                        "prototype".to_string(),
                        PropertyDescriptor::data(fp_val, true, false, false),
                    );
                    // Set constructor back-link
                    let func_obj_val = JsValue::object(function_id);
                    existing_fp.borrow_mut().insert_property(
                        "constructor".to_string(),
                        PropertyDescriptor::data(func_obj_val, true, false, true),
                    );
                    existing_fp.clone()
                } else {
                    let pv = self.get_property_on_id(function_id, "prototype");
                    if let Some(proto_id) = pv.as_object_id() {
                        self.get_object(proto_id).unwrap()
                    } else {
                        unreachable!()
                    }
                };
                {
                    // Set Function.prototype's [[Prototype]] to Object.prototype_id
                    if fp.borrow().prototype_id.is_none() {
                        fp.borrow_mut().prototype_id = self.realm().object_prototype;
                    }
                    // Ensure function_prototype is set
                    self.realm_mut().function_prototype = Some(fp.borrow().id.unwrap());

                    // Install call/apply/bind/toString on Function.prototype_id
                    let fp_id = fp.borrow().id.unwrap();
                    self.setup_function_prototype(fp_id);

                    // Add Function.prototype[@@hasInstance]
                    if let Some(sym_key) = self.get_symbol_key("hasInstance") {
                        let has_instance_fn = self.create_function(JsFunction::native(
                            "[Symbol.hasInstance]".to_string(),
                            1,
                            |interp, this_val, args| {
                                let arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                                interp.ordinary_has_instance(this_val, &arg)
                            },
                        ));
                        fp.borrow_mut().insert_property(
                            sym_key,
                            PropertyDescriptor::data(has_instance_fn, false, false, false),
                        );
                    }

                    // Function.prototype.caller and .arguments (§20.2.3.1, §20.2.3.2)
                    if let Some(ref thrower) = self.realm().throw_type_error {
                        fp.borrow_mut().insert_property(
                            "caller".to_string(),
                            PropertyDescriptor::accessor(
                                Some(thrower.clone()),
                                Some(thrower.clone()),
                                false,
                                true,
                            ),
                        );
                        fp.borrow_mut().insert_property(
                            "arguments".to_string(),
                            PropertyDescriptor::accessor(
                                Some(thrower.clone()),
                                Some(thrower.clone()),
                                false,
                                true,
                            ),
                        );
                    }

                    // Sloppy function .caller getter (Annex B)
                    {
                        let caller_getter = self.create_function(JsFunction::native(
                            "get caller".to_string(),
                            0,
                            |interp: &mut Interpreter, this_val: &JsValue, _args: &[JsValue]| {
                                let Some(this_id) = this_val.as_object_id() else {
                                    return Completion::Normal(JsValue::NULL);
                                };
                                let this_realm_id = interp
                                    .function_realm_map
                                    .get(&this_id)
                                    .copied()
                                    .unwrap_or(interp.current_realm_id);
                                // Walk call_stack_frames from top to find this function's activation
                                let mut found_idx = None;
                                for i in (0..interp.call_stack_frames.len()).rev() {
                                    if interp.call_stack_frames[i].func_obj_id == this_id {
                                        found_idx = Some(i);
                                        break;
                                    }
                                }
                                let idx = match found_idx {
                                    Some(i) => i,
                                    None => return Completion::Normal(JsValue::NULL),
                                };
                                // Scan backwards from idx, skipping eval frames
                                if idx == 0 {
                                    return Completion::Normal(JsValue::NULL);
                                }
                                for i in (0..idx).rev() {
                                    let frame = &interp.call_stack_frames[i];
                                    if frame.is_eval {
                                        continue;
                                    }
                                    if frame.func_obj_id == 0 {
                                        continue;
                                    }
                                    let caller_realm_id = interp
                                        .function_realm_map
                                        .get(&frame.func_obj_id)
                                        .copied()
                                        .unwrap_or(interp.current_realm_id);
                                    // Do not leak cross-realm caller function objects.
                                    if caller_realm_id != this_realm_id {
                                        return Completion::Normal(JsValue::NULL);
                                    }
                                    // Check if the caller is strict — return null for strict callers
                                    if let Some(obj) = interp.get_object(frame.func_obj_id)
                                        && let Some(JsFunction::User { is_strict, .. }) =
                                            &obj.borrow().callable
                                        && *is_strict
                                    {
                                        return Completion::Normal(JsValue::NULL);
                                    }
                                    return Completion::Normal(JsValue::object(frame.func_obj_id));
                                }
                                Completion::Normal(JsValue::NULL)
                            },
                        ));
                        self.realm_mut().sloppy_caller_getter = Some(caller_getter);
                    }

                    // Sloppy function .arguments getter (Annex B)
                    {
                        let args_getter = self.create_function(JsFunction::native(
                            "get arguments".to_string(),
                            0,
                            |interp: &mut Interpreter, this_val: &JsValue, _args: &[JsValue]| {
                                let Some(this_id) = this_val.as_object_id() else {
                                    return Completion::Normal(JsValue::NULL);
                                };
                                // Walk call_stack_frames from top to find this function's activation
                                for i in (0..interp.call_stack_frames.len()).rev() {
                                    if interp.call_stack_frames[i].func_obj_id == this_id {
                                        return Completion::Normal(
                                            interp.materialize_call_frame_arguments(i),
                                        );
                                    }
                                }
                                Completion::Normal(JsValue::NULL)
                            },
                        ));
                        self.realm_mut().sloppy_arguments_getter = Some(args_getter);
                    }

                    // Retroactively fix [[Prototype]] of all functions created before
                    // Function was registered. Walk global bindings 3 levels deep:
                    // global → Constructor → prototype → method
                    // Also fix accessor get/set functions.
                    let fp_id = fp.borrow().id;

                    // Helper: fix a single object's prototype if it's callable
                    let fix_callable = |obj: &ObjectHandle, fp: &ObjectHandle| {
                        if obj.borrow().callable.is_some() {
                            obj.borrow_mut().prototype_id = Some(fp.borrow().id.unwrap());
                        }
                    };

                    // Collect all JsValue objects from a property descriptor (value + accessors)
                    fn collect_pd_objects(pd: &PropertyDescriptor) -> Vec<JsValue> {
                        let mut out = Vec::new();
                        if let Some(ref v) = pd.value {
                            out.push(v.clone());
                        }
                        if let Some(ref g) = pd.get {
                            out.push(g.clone());
                        }
                        if let Some(ref s) = pd.set {
                            out.push(s.clone());
                        }
                        out
                    }

                    let bindings: Vec<JsValue> = self
                        .realm()
                        .global_env
                        .borrow()
                        .bindings
                        .values()
                        .map(|b| b.value.clone())
                        .collect();
                    for val in &bindings {
                        if let Some(obj_id) = val.as_object_id()
                            && let Some(obj) = self.get_object_cell(obj_id)
                        {
                            fix_callable(obj, &fp);
                            // Level 2: properties of global bindings (static methods, .prototype)
                            let level2_vals: Vec<JsValue> = obj
                                .borrow()
                                .properties
                                .values()
                                .flat_map(collect_pd_objects)
                                .collect();
                            for pv in &level2_vals {
                                if let Some(property_id) = pv.as_object_id()
                                    && let Some(pobj) = self.get_object_cell(property_id)
                                {
                                    // Don't set fp's own [[Prototype]] to itself
                                    if Some(property_id) != fp_id {
                                        fix_callable(pobj, &fp);
                                    }
                                    // Level 3: always walk into properties (including fp's own)
                                    let level3_vals: Vec<JsValue> = pobj
                                        .borrow()
                                        .properties
                                        .values()
                                        .flat_map(collect_pd_objects)
                                        .collect();
                                    for pv3 in &level3_vals {
                                        if let Some(property_id) = pv3.as_object_id() {
                                            if Some(property_id) == fp_id {
                                                continue;
                                            }
                                            if let Some(pobj3) = self.get_object_cell(property_id) {
                                                fix_callable(pobj3, &fp);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Fix generator/async/async-generator function prototypes:
                    // Their [[Prototype]] should be Function.prototype per spec
                    // (§27.3.3, §27.7.3, §27.4.3) regardless of callability
                    let special_proto_ids: Vec<u64> = [
                        self.realm().generator_function_prototype,
                        self.realm().async_function_prototype,
                        self.realm().async_generator_function_prototype,
                    ]
                    .into_iter()
                    .flatten()
                    .collect();
                    for sp_id in special_proto_ids {
                        if let Some(special_proto) = self.get_object_cell(sp_id) {
                            special_proto.borrow_mut().prototype_id = Some(fp.borrow().id.unwrap());
                        }
                    }

                    // Fix internal prototype fields (iterator protos, collection protos, etc.)
                    // that aren't reachable through the global bindings walk above.
                    let internal_proto_ids: Vec<u64> = [
                        self.realm().iterator_prototype,
                        self.realm().array_iterator_prototype,
                        self.realm().string_iterator_prototype,
                        self.realm().map_iterator_prototype,
                        self.realm().set_iterator_prototype,
                        self.realm().generator_prototype,
                        self.realm().async_iterator_prototype,
                        self.realm().async_generator_prototype,
                        self.realm().regexp_prototype,
                        self.realm().promise_prototype,
                        self.realm().arraybuffer_prototype,
                        self.realm().typed_array_prototype,
                        self.realm().dataview_prototype,
                        self.realm().weakref_prototype,
                        self.realm().finalization_registry_prototype,
                        self.realm().aggregate_error_prototype,
                    ]
                    .into_iter()
                    .flatten()
                    .collect();
                    let internal_protos: Vec<&ObjectHandle> = internal_proto_ids
                        .into_iter()
                        .filter_map(|id| self.get_object_cell(id))
                        .collect();
                    for proto in &internal_protos {
                        let prop_vals: Vec<JsValue> = proto
                            .borrow()
                            .properties
                            .values()
                            .flat_map(collect_pd_objects)
                            .collect();
                        for pv in &prop_vals {
                            if let Some(property_id) = pv.as_object_id() {
                                if Some(property_id) == fp_id {
                                    continue;
                                }
                                if let Some(pobj) = self.get_object_cell(property_id) {
                                    fix_callable(pobj, &fp);
                                }
                            }
                        }
                    }

                    // Fix %ThrowTypeError% prototype (§10.2.4 step 11)
                    if let Some(thrower_id) = self
                        .realm()
                        .throw_type_error
                        .as_ref()
                        .and_then(JsValue::as_object_id)
                        && let Some(te_obj) = self.get_object_cell(thrower_id)
                    {
                        te_obj.borrow_mut().prototype_id = Some(fp.borrow().id.unwrap());
                    }
                }
            }
        }
    }

    fn link_native_error_constructors(&mut self) {
        // Set NativeError constructors' [[Prototype]] to %Error% (spec §20.5.6.1)
        // Must happen after the Function.prototype retroactive fix above
        {
            let error_ctor_obj = self.get_global_var("Error").and_then(|v| {
                v.as_object_id()
                    .and_then(|error_id| self.get_object(error_id))
            });
            if let Some(err_data) = error_ctor_obj {
                for name in [
                    "SyntaxError",
                    "TypeError",
                    "ReferenceError",
                    "RangeError",
                    "URIError",
                    "EvalError",
                    "SuppressedError",
                    "AggregateError",
                ] {
                    let ctor_val = self.get_global_var(name);
                    if let Some(ctor_id) = ctor_val.and_then(|v| v.as_object_id())
                        && let Some(ctor_obj) = self.get_object_cell(ctor_id)
                    {
                        ctor_obj.borrow_mut().prototype_id = Some(err_data.borrow().id.unwrap());
                    }
                }
            }
        }
    }

    fn setup_async_function_prototype(&mut self) {
        // %AsyncFunction.prototype%
        // Per spec, this should inherit from Function.prototype_id
        {
            let af_proto_id = self.create_object_id();
            self.get_object_cell_expect(af_proto_id)
                .borrow_mut()
                .class_name = "AsyncFunction".to_string();

            // [[Prototype]] = Function.prototype_id
            if let Some(func_val) = self.get_global_var("Function")
                && let Some(function_id) = func_val.as_object_id()
                && let Some(function_proto_id) = self
                    .get_property_on_id(function_id, "prototype")
                    .as_object_id()
                && let Some(func_proto) = self.get_object_cell(function_proto_id)
            {
                self.get_object_cell_expect(af_proto_id)
                    .borrow_mut()
                    .prototype_id = Some(func_proto.borrow().id.unwrap());
            }

            // Symbol.toStringTag = "AsyncFunction"
            self.define_to_string_tag(af_proto_id, "AsyncFunction");

            self.realm_mut().async_function_prototype = Some(af_proto_id);
        }
    }

    fn setup_async_function_constructor(&mut self) {
        // AsyncFunction constructor (not a global per spec)
        // Create the constructor and wire it up with AsyncFunction.prototype_id
        if let Some(af_proto_id) = self.realm().async_function_prototype
            && let Some(af_proto) = self.get_object(af_proto_id)
        {
            let af_ctor = self.create_function(JsFunction::constructor(
                "AsyncFunction".to_string(),
                1,
                |interp, _this, args| {
                    let (params_str, body_str) = if args.is_empty() {
                        (String::new(), String::new())
                    } else if args.len() == 1 {
                        match interp.to_string_value(&args[0]) {
                            Ok(s) => (String::new(), s),
                            Err(e) => return Completion::Throw(e),
                        }
                    } else {
                        let mut params = Vec::new();
                        for arg in &args[..args.len() - 1] {
                            match interp.to_string_value(arg) {
                                Ok(s) => params.push(s),
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        let body = match interp.to_string_value(args.last().unwrap()) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        };
                        (params.join(","), body)
                    };

                    if !params_str.is_empty() {
                        let params_test = format!("(async function({}\n) {{}})", params_str);
                        match parser::Parser::new(&params_test).and_then(|mut p| p.parse_program())
                        {
                            Ok(_) => {}
                            Err(e) => {
                                return Completion::Throw(
                                    interp.create_error("SyntaxError", &format!("{}", e)),
                                );
                            }
                        }
                    }
                    {
                        let body_test = format!("(async function() {{\n{}\n}})", body_str);
                        match parser::Parser::new(&body_test).and_then(|mut p| p.parse_program()) {
                            Ok(_) => {}
                            Err(e) => {
                                return Completion::Throw(
                                    interp.create_error("SyntaxError", &format!("{}", e)),
                                );
                            }
                        }
                    }

                    let fn_source_text = format!(
                        "async function anonymous({}\n) {{\n{}\n}}",
                        params_str, body_str
                    );
                    let source = format!(
                        "(async function anonymous({}\n) {{\n{}\n}})",
                        params_str, body_str
                    );
                    let mut p = match parser::Parser::new(&source) {
                        Ok(p) => p,
                        Err(e) => {
                            return Completion::Throw(
                                interp.create_error("SyntaxError", &format!("{}", e)),
                            );
                        }
                    };
                    let program = match p.parse_program() {
                        Ok(prog) => prog,
                        Err(e) => {
                            return Completion::Throw(
                                interp.create_error("SyntaxError", &format!("{}", e)),
                            );
                        }
                    };

                    if let Some(Statement::Expression(Expression::Function(fe))) =
                        program.body.as_slice().first()
                    {
                        let is_strict = fe.body_is_strict;
                        let dynamic_fn_env =
                            Environment::new(Some(interp.realm().global_env.clone()));
                        dynamic_fn_env.borrow_mut().strict = false;
                        let js_func = JsFunction::User {
                            name: Some("anonymous".to_string()),
                            params: Rc::new(fe.params.clone()),
                            body: fe.body.clone(),
                            closure: dynamic_fn_env,
                            is_arrow: false,
                            is_strict,
                            is_generator: false,
                            is_async: true,
                            is_method: false,
                            source_text: Some(fn_source_text.into()),
                            captured_new_target: None,
                            uses_arguments: true, // conservative for dynamic AsyncFunction()
                            has_simple_params: crate::ast::params_are_simple(&fe.params),
                        };
                        let fn_val = interp.create_function(js_func);
                        // Apply GetPrototypeFromConstructor(newTarget, "%AsyncFunction.prototype%")
                        if let Some(function_id) = fn_val.as_object_id() {
                            let proto = interp.get_prototype_from_new_target_realm(|realm| {
                                realm.async_function_prototype
                            });
                            match proto {
                                Ok(Some(proto_rc)) => {
                                    if let Some(fo_obj) = interp.get_object_cell(function_id) {
                                        fo_obj.borrow_mut().prototype_id = Some(proto_rc);
                                    }
                                }
                                Ok(None) => {}
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        Completion::Normal(fn_val)
                    } else {
                        Completion::Throw(
                            interp.create_error("SyntaxError", "Failed to parse async function"),
                        )
                    }
                },
            ));
            // Wire up AsyncFunction.prototype and constructor property
            if let Some(async_function_id) = af_ctor.as_object_id()
                && let Some(af) = self.get_object(async_function_id)
            {
                // Parse-before-getprototype: body is parsed before prototype lookup
                af.borrow_mut().deferred_construct = true;
                // §27.7.1 %AsyncFunction% inherits from %Function%
                let function_ctor = self
                    .get_global_var("Function")
                    .unwrap_or(JsValue::UNDEFINED);
                if let Some(function_id) = function_ctor.as_object_id()
                    && let Some(fc_obj) = self.get_object_cell(function_id)
                {
                    af.borrow_mut().prototype_id = Some(fc_obj.borrow().id.unwrap());
                }
                let proto_id = af_proto.borrow().id.unwrap();
                // Set AsyncFunction.prototype_id
                af.borrow_mut().insert_property(
                    "prototype".to_string(),
                    PropertyDescriptor::data(JsValue::object(proto_id), false, false, false),
                );
                // Set constructor back-reference on AsyncFunction.prototype_id
                af_proto.borrow_mut().insert_property(
                    "constructor".to_string(),
                    PropertyDescriptor::data(af_ctor.clone(), false, false, true),
                );
            }
        }
    }

    fn setup_generator_function_constructor(&mut self) {
        // GeneratorFunction constructor (not a global per spec)
        // Create the constructor and wire it up with GeneratorFunction.prototype_id
        if let Some(gf_proto_id) = self.realm().generator_function_prototype
            && let Some(gf_proto) = self.get_object(gf_proto_id)
        {
            let gf_ctor = self.create_function(JsFunction::constructor(
                "GeneratorFunction".to_string(),
                1,
                |interp, _this, args| {
                    let (params_str, body_str) = if args.is_empty() {
                        (String::new(), String::new())
                    } else if args.len() == 1 {
                        match interp.to_string_value(&args[0]) {
                            Ok(s) => (String::new(), s),
                            Err(e) => return Completion::Throw(e),
                        }
                    } else {
                        let mut params = Vec::new();
                        for arg in &args[..args.len() - 1] {
                            match interp.to_string_value(arg) {
                                Ok(s) => params.push(s),
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        let body = match interp.to_string_value(args.last().unwrap()) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        };
                        (params.join(","), body)
                    };

                    if !params_str.is_empty() {
                        let params_test = format!("(function*({}\n) {{}})", params_str);
                        match parser::Parser::new(&params_test).and_then(|mut p| p.parse_program())
                        {
                            Ok(_) => {}
                            Err(e) => {
                                return Completion::Throw(
                                    interp.create_error("SyntaxError", &format!("{}", e)),
                                );
                            }
                        }
                    }
                    {
                        let body_test = format!("(function*() {{\n{}\n}})", body_str);
                        match parser::Parser::new(&body_test).and_then(|mut p| p.parse_program()) {
                            Ok(_) => {}
                            Err(e) => {
                                return Completion::Throw(
                                    interp.create_error("SyntaxError", &format!("{}", e)),
                                );
                            }
                        }
                    }

                    let fn_source_text =
                        format!("function* anonymous({}\n) {{\n{}\n}}", params_str, body_str);
                    let source = format!(
                        "(function* anonymous({}\n) {{\n{}\n}})",
                        params_str, body_str
                    );
                    let mut p = match parser::Parser::new(&source) {
                        Ok(p) => p,
                        Err(e) => {
                            return Completion::Throw(
                                interp.create_error("SyntaxError", &format!("{}", e)),
                            );
                        }
                    };
                    let program = match p.parse_program() {
                        Ok(prog) => prog,
                        Err(e) => {
                            return Completion::Throw(
                                interp.create_error("SyntaxError", &format!("{}", e)),
                            );
                        }
                    };

                    if let Some(Statement::Expression(Expression::Function(fe))) =
                        program.body.as_slice().first()
                    {
                        let is_strict = fe.body_is_strict;
                        let dynamic_fn_env =
                            Environment::new(Some(interp.realm().global_env.clone()));
                        dynamic_fn_env.borrow_mut().strict = false;
                        let js_func = JsFunction::User {
                            name: Some("anonymous".to_string()),
                            params: Rc::new(fe.params.clone()),
                            body: fe.body.clone(),
                            closure: dynamic_fn_env,
                            is_arrow: false,
                            is_strict,
                            is_generator: true,
                            is_async: false,
                            is_method: false,
                            source_text: Some(fn_source_text.into()),
                            captured_new_target: None,
                            uses_arguments: true, // conservative for dynamic GeneratorFunction()
                            has_simple_params: crate::ast::params_are_simple(&fe.params),
                        };
                        let fn_val = interp.create_function(js_func);
                        // Apply GetPrototypeFromConstructor(newTarget, "%GeneratorFunction.prototype%")
                        if let Some(function_id) = fn_val.as_object_id() {
                            let proto = interp.get_prototype_from_new_target_realm(|realm| {
                                realm.generator_function_prototype
                            });
                            match proto {
                                Ok(Some(proto_rc)) => {
                                    if let Some(fo_obj) = interp.get_object_cell(function_id) {
                                        fo_obj.borrow_mut().prototype_id = Some(proto_rc);
                                    }
                                }
                                Ok(None) => {}
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        Completion::Normal(fn_val)
                    } else {
                        Completion::Throw(
                            interp
                                .create_error("SyntaxError", "Failed to parse generator function"),
                        )
                    }
                },
            ));
            // Wire up GeneratorFunction.prototype and constructor property
            if let Some(generator_function_id) = gf_ctor.as_object_id()
                && let Some(gf) = self.get_object(generator_function_id)
            {
                gf.borrow_mut().deferred_construct = true;
                // §27.3.1 %GeneratorFunction% inherits from %Function%
                let function_ctor = self
                    .get_global_var("Function")
                    .unwrap_or(JsValue::UNDEFINED);
                if let Some(function_id) = function_ctor.as_object_id()
                    && let Some(fc_obj) = self.get_object_cell(function_id)
                {
                    gf.borrow_mut().prototype_id = Some(fc_obj.borrow().id.unwrap());
                }
                let proto_id = gf_proto.borrow().id.unwrap();
                // Set GeneratorFunction.prototype_id
                gf.borrow_mut().insert_property(
                    "prototype".to_string(),
                    PropertyDescriptor::data(JsValue::object(proto_id), false, false, false),
                );
                // Set constructor back-reference on GeneratorFunction.prototype_id
                gf_proto.borrow_mut().insert_property(
                    "constructor".to_string(),
                    PropertyDescriptor::data(gf_ctor.clone(), false, false, true),
                );
            }
        }
    }

    fn setup_async_generator_function_constructor(&mut self) {
        // AsyncGeneratorFunction constructor (not a global per spec)
        // Create the constructor and wire it up with AsyncGeneratorFunction.prototype_id
        if let Some(agf_proto_id) = self.realm().async_generator_function_prototype
            && let Some(agf_proto) = self.get_object(agf_proto_id)
        {
            let agf_ctor = self.create_function(JsFunction::constructor(
                "AsyncGeneratorFunction".to_string(),
                1,
                |interp, _this, args| {
                    let (params_str, body_str) = if args.is_empty() {
                        (String::new(), String::new())
                    } else if args.len() == 1 {
                        match interp.to_string_value(&args[0]) {
                            Ok(s) => (String::new(), s),
                            Err(e) => return Completion::Throw(e),
                        }
                    } else {
                        let mut params = Vec::new();
                        for arg in &args[..args.len() - 1] {
                            match interp.to_string_value(arg) {
                                Ok(s) => params.push(s),
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        let body = match interp.to_string_value(args.last().unwrap()) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        };
                        (params.join(","), body)
                    };

                    if !params_str.is_empty() {
                        let params_test = format!("(async function*({}\n) {{}})", params_str);
                        match parser::Parser::new(&params_test).and_then(|mut p| p.parse_program())
                        {
                            Ok(_) => {}
                            Err(e) => {
                                return Completion::Throw(
                                    interp.create_error("SyntaxError", &format!("{}", e)),
                                );
                            }
                        }
                    }
                    {
                        let body_test = format!("(async function*() {{\n{}\n}})", body_str);
                        match parser::Parser::new(&body_test).and_then(|mut p| p.parse_program()) {
                            Ok(_) => {}
                            Err(e) => {
                                return Completion::Throw(
                                    interp.create_error("SyntaxError", &format!("{}", e)),
                                );
                            }
                        }
                    }

                    let fn_source_text = format!(
                        "async function* anonymous({}\n) {{\n{}\n}}",
                        params_str, body_str
                    );
                    let source = format!(
                        "(async function* anonymous({}\n) {{\n{}\n}})",
                        params_str, body_str
                    );
                    let mut p = match parser::Parser::new(&source) {
                        Ok(p) => p,
                        Err(e) => {
                            return Completion::Throw(
                                interp.create_error("SyntaxError", &format!("{}", e)),
                            );
                        }
                    };
                    let program = match p.parse_program() {
                        Ok(prog) => prog,
                        Err(e) => {
                            return Completion::Throw(
                                interp.create_error("SyntaxError", &format!("{}", e)),
                            );
                        }
                    };

                    if let Some(Statement::Expression(Expression::Function(fe))) =
                        program.body.as_slice().first()
                    {
                        let is_strict = fe.body_is_strict;
                        let dynamic_fn_env =
                            Environment::new(Some(interp.realm().global_env.clone()));
                        dynamic_fn_env.borrow_mut().strict = false;
                        let js_func = JsFunction::User {
                            name: Some("anonymous".to_string()),
                            params: Rc::new(fe.params.clone()),
                            body: fe.body.clone(),
                            closure: dynamic_fn_env,
                            is_arrow: false,
                            is_strict,
                            is_generator: true,
                            is_async: true,
                            is_method: false,
                            source_text: Some(fn_source_text.into()),
                            captured_new_target: None,
                            uses_arguments: true, // conservative for dynamic AsyncGeneratorFunction()
                            has_simple_params: crate::ast::params_are_simple(&fe.params),
                        };
                        let fn_val = interp.create_function(js_func);
                        // Apply GetPrototypeFromConstructor(newTarget, "%AsyncGeneratorFunction.prototype%")
                        if let Some(function_id) = fn_val.as_object_id() {
                            let proto = interp.get_prototype_from_new_target_realm(|realm| {
                                realm.async_generator_function_prototype
                            });
                            match proto {
                                Ok(Some(proto_rc)) => {
                                    if let Some(fo_obj) = interp.get_object_cell(function_id) {
                                        fo_obj.borrow_mut().prototype_id = Some(proto_rc);
                                    }
                                }
                                Ok(None) => {}
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        Completion::Normal(fn_val)
                    } else {
                        Completion::Throw(interp.create_error(
                            "SyntaxError",
                            "Failed to parse async generator function",
                        ))
                    }
                },
            ));
            // Wire up AsyncGeneratorFunction.prototype and constructor property
            if let Some(async_generator_function_id) = agf_ctor.as_object_id()
                && let Some(agf) = self.get_object(async_generator_function_id)
            {
                agf.borrow_mut().deferred_construct = true;
                // §27.4.1 %AsyncGeneratorFunction% inherits from %Function%
                let function_ctor = self
                    .get_global_var("Function")
                    .unwrap_or(JsValue::UNDEFINED);
                if let Some(function_id) = function_ctor.as_object_id()
                    && let Some(fc_obj) = self.get_object_cell(function_id)
                {
                    agf.borrow_mut().prototype_id = Some(fc_obj.borrow().id.unwrap());
                }
                let proto_id = agf_proto.borrow().id.unwrap();
                // Set AsyncGeneratorFunction.prototype_id
                agf.borrow_mut().insert_property(
                    "prototype".to_string(),
                    PropertyDescriptor::data(JsValue::object(proto_id), false, false, false),
                );
                // Set constructor back-reference on AsyncGeneratorFunction.prototype_id
                agf_proto.borrow_mut().insert_property(
                    "constructor".to_string(),
                    PropertyDescriptor::data(agf_ctor.clone(), false, false, true),
                );
            }
        }
    }
}
