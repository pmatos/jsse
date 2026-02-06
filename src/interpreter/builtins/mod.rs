mod array;
mod bigint;
mod collections;
mod date;
mod disposable;
mod iterators;
mod number;
mod promise;
mod regexp;
mod string;
mod typedarray;

use super::*;

impl Interpreter {
    pub(crate) fn setup_globals(&mut self) {
        let console = self.create_object();
        let console_id = console.borrow().id.unwrap();
        {
            let log_fn = self.create_function(JsFunction::native(
                "log".to_string(),
                0,
                |_interp, _this, args| {
                    let parts: Vec<String> = args.iter().map(|v| format!("{v}")).collect();
                    println!("{}", parts.join(" "));
                    Completion::Normal(JsValue::Undefined)
                },
            ));
            console.borrow_mut().insert_builtin("log".to_string(), log_fn);
        }
        let console_val = JsValue::Object(crate::types::JsObject { id: console_id });
        self.global_env
            .borrow_mut()
            .declare("console", BindingKind::Const);
        let _ = self.global_env.borrow_mut().set("console", console_val);

        // print global (needed by test262 async harness doneprintHandle.js)
        {
            let print_fn = self.create_function(JsFunction::native(
                "print".to_string(),
                1,
                |_interp, _this, args| {
                    let parts: Vec<String> = args.iter().map(|v| format!("{v}")).collect();
                    println!("{}", parts.join(" "));
                    Completion::Normal(JsValue::Undefined)
                },
            ));
            self.global_env
                .borrow_mut()
                .declare("print", BindingKind::Var);
            let _ = self.global_env.borrow_mut().set("print", print_fn);
        }

        // $262 test harness object
        {
            let dollar_262 = self.create_object();
            let detach_fn = self.create_function(JsFunction::native(
                "detachArrayBuffer".to_string(),
                1,
                |interp, _this, args| {
                    let buf = args.first().cloned().unwrap_or(JsValue::Undefined);
                    interp.detach_arraybuffer(&buf)
                },
            ));
            dollar_262
                .borrow_mut()
                .insert_builtin("detachArrayBuffer".to_string(), detach_fn);
            let gc_fn = self.create_function(JsFunction::native(
                "gc".to_string(),
                0,
                |interp, _this, _args| {
                    interp.maybe_gc();
                    Completion::Normal(JsValue::Undefined)
                },
            ));
            dollar_262
                .borrow_mut()
                .insert_builtin("gc".to_string(), gc_fn);
            let dollar_262_val =
                JsValue::Object(crate::types::JsObject { id: dollar_262.borrow().id.unwrap() });
            self.global_env
                .borrow_mut()
                .declare("$262", BindingKind::Var);
            let _ = self.global_env.borrow_mut().set("$262", dollar_262_val);
        }

        // Error constructor
        {
            let error_name = "Error".to_string();
            self.register_global_fn(
                "Error",
                BindingKind::Var,
                JsFunction::constructor(error_name.clone(), 1, move |interp, this, args| {
                    let msg_raw = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);

                    macro_rules! init_error {
                        ($o:expr) => {
                            $o.class_name = "Error".to_string();
                            if !matches!(msg_raw, JsValue::Undefined) {
                                let msg_str = match interp.to_string_value(&msg_raw) {
                                    Ok(s) => JsValue::String(JsString::from_str(&s)),
                                    Err(e) => return Completion::Throw(e),
                                };
                                $o.insert_builtin("message".to_string(), msg_str);
                            }
                            if let JsValue::Object(opts) = &options {
                                if let Some(opts_obj) = interp.get_object(opts.id) {
                                    if opts_obj.borrow().has_property("cause") {
                                        let cause =
                                            interp.get_object_property(opts.id, "cause", &options);
                                        match cause {
                                            Completion::Normal(v) => {
                                                $o.insert_builtin("cause".to_string(), v);
                                            }
                                            c => return c,
                                        }
                                    }
                                }
                            }
                        };
                    }

                    if let JsValue::Object(o) = this {
                        if let Some(obj) = interp.get_object(o.id) {
                            let mut o = obj.borrow_mut();
                            init_error!(o);
                        }
                        return Completion::Normal(this.clone());
                    }
                    let obj = interp.create_object();
                    {
                        let mut o = obj.borrow_mut();
                        init_error!(o);
                    }
                    let id = obj.borrow().id.unwrap();
                    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
                }),
            );
        }

        // Get Error.prototype for inheritance
        let error_prototype = {
            let env = self.global_env.borrow();
            if let Some(error_val) = env.get("Error") {
                if let JsValue::Object(o) = &error_val {
                    if let Some(ctor) = self.get_object(o.id) {
                        let proto_val = ctor.borrow().get_property("prototype");
                        if let JsValue::Object(p) = &proto_val {
                            self.get_object(p.id)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        };

        // Add toString to Error.prototype
        if let Some(ref ep) = error_prototype {
            let tostring_fn = self.create_function(JsFunction::native(
                "toString".to_string(),
                0,
                |interp, this_val, _args| {
                    if let JsValue::Object(o) = this_val
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        let obj_ref = obj.borrow();
                        let name = match obj_ref.get_property("name") {
                            JsValue::Undefined => "Error".to_string(),
                            v => to_js_string(&v),
                        };
                        let msg = match obj_ref.get_property("message") {
                            JsValue::Undefined => String::new(),
                            v => to_js_string(&v),
                        };
                        return if msg.is_empty() {
                            Completion::Normal(JsValue::String(JsString::from_str(&name)))
                        } else {
                            Completion::Normal(JsValue::String(JsString::from_str(&format!(
                                "{name}: {msg}"
                            ))))
                        };
                    }
                    Completion::Normal(JsValue::String(JsString::from_str("Error")))
                },
            ));
            ep.borrow_mut()
                .insert_builtin("toString".to_string(), tostring_fn);
            ep.borrow_mut().insert_builtin(
                "name".to_string(),
                JsValue::String(JsString::from_str("Error")),
            );
            ep.borrow_mut().insert_builtin(
                "message".to_string(),
                JsValue::String(JsString::from_str("")),
            );
            // Set constructor on Error.prototype
            {
                let env = self.global_env.borrow();
                if let Some(error_ctor) = env.get("Error") {
                    ep.borrow_mut()
                        .insert_builtin("constructor".to_string(), error_ctor);
                }
            }
        }

        // Error.isError() static method
        {
            let is_error_fn = self.create_function(JsFunction::native(
                "isError".to_string(),
                1,
                |interp, _this, args| {
                    let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(o) = &arg
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        let cn = &obj.borrow().class_name;
                        if cn.contains("Error") {
                            return Completion::Normal(JsValue::Boolean(true));
                        }
                    }
                    Completion::Normal(JsValue::Boolean(false))
                },
            ));
            let env = self.global_env.borrow();
            if let Some(error_ctor) = env.get("Error")
                && let JsValue::Object(o) = &error_ctor
                && let Some(obj) = self.get_object(o.id)
            {
                obj.borrow_mut()
                    .insert_builtin("isError".to_string(), is_error_fn);
            }
        }

        // Test262Error
        {
            let error_proto_clone = error_prototype.clone();
            self.register_global_fn(
                "Test262Error",
                BindingKind::Var,
                JsFunction::constructor(
                    "Test262Error".to_string(),
                    1,
                    move |interp, this, args| {
                        let msg = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if let JsValue::Object(o) = this {
                            if let Some(obj) = interp.get_object(o.id) {
                                let mut o = obj.borrow_mut();
                                o.class_name = "Test262Error".to_string();
                                if let Some(ref ep) = error_proto_clone {
                                    o.prototype = Some(ep.clone());
                                }
                                if !matches!(msg, JsValue::Undefined) {
                                    o.insert_builtin("message".to_string(), msg);
                                }
                                o.insert_builtin(
                                    "name".to_string(),
                                    JsValue::String(JsString::from_str("Test262Error")),
                                );
                            }
                            return Completion::Normal(this.clone());
                        }
                        let obj = interp.create_object();
                        {
                            let mut o = obj.borrow_mut();
                            o.class_name = "Test262Error".to_string();
                            if let Some(ref ep) = error_proto_clone {
                                o.prototype = Some(ep.clone());
                            }
                            if !matches!(msg, JsValue::Undefined) {
                                o.insert_builtin("message".to_string(), msg);
                            }
                            o.insert_builtin(
                                "name".to_string(),
                                JsValue::String(JsString::from_str("Test262Error")),
                            );
                        }
                        let id = obj.borrow().id.unwrap();
                        Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
                    },
                ),
            );
        }

        // Error subtype constructors
        for name in [
            "SyntaxError",
            "TypeError",
            "ReferenceError",
            "RangeError",
            "URIError",
            "EvalError",
        ] {
            let error_name = name.to_string();

            // Create per-type prototype inheriting from Error.prototype
            let native_proto = self.create_object();
            if let Some(ref ep) = error_prototype {
                native_proto.borrow_mut().prototype = Some(ep.clone());
            }
            native_proto.borrow_mut().insert_builtin(
                "name".to_string(),
                JsValue::String(JsString::from_str(name)),
            );
            native_proto.borrow_mut().insert_builtin(
                "message".to_string(),
                JsValue::String(JsString::from_str("")),
            );

            let native_proto_clone = native_proto.clone();
            let error_name_clone = error_name.clone();
            self.register_global_fn(
                name,
                BindingKind::Var,
                JsFunction::constructor(error_name.clone(), 1, move |interp, this, args| {
                    let msg_raw = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);

                    macro_rules! init_native_error {
                        ($o:expr) => {
                            $o.class_name = error_name_clone.clone();
                            $o.prototype = Some(native_proto_clone.clone());
                            if !matches!(msg_raw, JsValue::Undefined) {
                                let msg_str = match interp.to_string_value(&msg_raw) {
                                    Ok(s) => JsValue::String(JsString::from_str(&s)),
                                    Err(e) => return Completion::Throw(e),
                                };
                                $o.insert_builtin("message".to_string(), msg_str);
                            }
                            if let JsValue::Object(opts) = &options {
                                if let Some(opts_obj) = interp.get_object(opts.id) {
                                    if opts_obj.borrow().has_property("cause") {
                                        let cause =
                                            interp.get_object_property(opts.id, "cause", &options);
                                        match cause {
                                            Completion::Normal(v) => {
                                                $o.insert_builtin("cause".to_string(), v);
                                            }
                                            c => return c,
                                        }
                                    }
                                }
                            }
                        };
                    }

                    if let JsValue::Object(o) = this {
                        if let Some(obj) = interp.get_object(o.id) {
                            let mut o = obj.borrow_mut();
                            init_native_error!(o);
                        }
                        return Completion::Normal(this.clone());
                    }
                    let obj = interp.create_object();
                    {
                        let mut o = obj.borrow_mut();
                        init_native_error!(o);
                    }
                    let id = obj.borrow().id.unwrap();
                    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
                }),
            );

            // Set constructor on the per-type prototype
            {
                let env = self.global_env.borrow();
                if let Some(ctor_val) = env.get(name) {
                    native_proto
                        .borrow_mut()
                        .insert_builtin("constructor".to_string(), ctor_val);
                }
            }
            // Set constructor's .prototype to the per-type prototype
            {
                let env = self.global_env.borrow();
                if let Some(ctor_val) = env.get(name)
                    && let JsValue::Object(o) = &ctor_val
                    && let Some(ctor_obj) = self.get_object(o.id)
                {
                    let proto_id = native_proto.borrow().id.unwrap();
                    ctor_obj.borrow_mut().insert_property(
                        "prototype".to_string(),
                        PropertyDescriptor::data(JsValue::Object(crate::types::JsObject { id: proto_id }), false, false, false),
                    );
                }
            }
        }

        // SuppressedError constructor
        {
            let suppressed_proto = self.create_object();
            if let Some(ref ep) = error_prototype {
                suppressed_proto.borrow_mut().prototype = Some(ep.clone());
            }
            suppressed_proto.borrow_mut().insert_builtin(
                "name".to_string(),
                JsValue::String(JsString::from_str("SuppressedError")),
            );
            suppressed_proto.borrow_mut().insert_builtin(
                "message".to_string(),
                JsValue::String(JsString::from_str("")),
            );
            let suppressed_proto_clone = suppressed_proto.clone();
            self.register_global_fn(
                "SuppressedError",
                BindingKind::Var,
                JsFunction::constructor(
                    "SuppressedError".to_string(),
                    3,
                    move |interp, this, args| {
                        let error_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let suppressed_val = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                        let msg_raw = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                        let options = args.get(3).cloned().unwrap_or(JsValue::Undefined);

                        macro_rules! init_suppressed_error {
                            ($o:expr) => {
                                $o.class_name = "SuppressedError".to_string();
                                $o.prototype = Some(suppressed_proto_clone.clone());
                                $o.insert_builtin("error".to_string(), error_val.clone());
                                $o.insert_builtin("suppressed".to_string(), suppressed_val.clone());
                                if !matches!(msg_raw, JsValue::Undefined) {
                                    let msg_str = match interp.to_string_value(&msg_raw) {
                                        Ok(s) => JsValue::String(JsString::from_str(&s)),
                                        Err(e) => return Completion::Throw(e),
                                    };
                                    $o.insert_builtin("message".to_string(), msg_str);
                                }
                                if let JsValue::Object(opts) = &options {
                                    if let Some(opts_obj) = interp.get_object(opts.id) {
                                        if opts_obj.borrow().has_property("cause") {
                                            let cause = interp
                                                .get_object_property(opts.id, "cause", &options);
                                            match cause {
                                                Completion::Normal(v) => {
                                                    $o.insert_builtin("cause".to_string(), v);
                                                }
                                                c => return c,
                                            }
                                        }
                                    }
                                }
                            };
                        }

                        if let JsValue::Object(o) = this {
                            if let Some(obj) = interp.get_object(o.id) {
                                let mut o = obj.borrow_mut();
                                init_suppressed_error!(o);
                            }
                            return Completion::Normal(this.clone());
                        }
                        let obj = interp.create_object();
                        {
                            let mut o = obj.borrow_mut();
                            init_suppressed_error!(o);
                        }
                        let id = obj.borrow().id.unwrap();
                        Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
                    },
                ),
            );
            {
                let env = self.global_env.borrow();
                if let Some(ctor_val) = env.get("SuppressedError") {
                    suppressed_proto
                        .borrow_mut()
                        .insert_builtin("constructor".to_string(), ctor_val);
                }
            }
            {
                let env = self.global_env.borrow();
                if let Some(ctor_val) = env.get("SuppressedError")
                    && let JsValue::Object(o) = &ctor_val
                    && let Some(ctor_obj) = self.get_object(o.id)
                {
                    let proto_id = suppressed_proto.borrow().id.unwrap();
                    ctor_obj.borrow_mut().insert_property(
                        "prototype".to_string(),
                        PropertyDescriptor::data(JsValue::Object(crate::types::JsObject { id: proto_id }), false, false, false),
                    );
                }
            }
        }

        // AggregateError constructor
        {
            let agg_proto = self.create_object();
            if let Some(ref ep) = error_prototype {
                agg_proto.borrow_mut().prototype = Some(ep.clone());
            }
            agg_proto.borrow_mut().insert_builtin(
                "name".to_string(),
                JsValue::String(JsString::from_str("AggregateError")),
            );
            agg_proto.borrow_mut().insert_builtin(
                "message".to_string(),
                JsValue::String(JsString::from_str("")),
            );
            let agg_proto_clone = agg_proto.clone();
            self.aggregate_error_prototype = Some(agg_proto.clone());
            self.register_global_fn(
                "AggregateError",
                BindingKind::Var,
                JsFunction::constructor(
                    "AggregateError".to_string(),
                    2,
                    move |interp, this, args| {
                        let errors_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let msg_raw = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                        let options = args.get(2).cloned().unwrap_or(JsValue::Undefined);

                        let errors_vec = match interp.iterate_to_vec(&errors_arg) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };
                        let errors_arr = interp.create_array(errors_vec);

                        macro_rules! init_agg_error {
                            ($o:expr) => {
                                $o.class_name = "AggregateError".to_string();
                                $o.prototype = Some(agg_proto_clone.clone());
                                $o.insert_builtin("errors".to_string(), errors_arr.clone());
                                if !matches!(msg_raw, JsValue::Undefined) {
                                    let msg_str = match interp.to_string_value(&msg_raw) {
                                        Ok(s) => JsValue::String(JsString::from_str(&s)),
                                        Err(e) => return Completion::Throw(e),
                                    };
                                    $o.insert_builtin("message".to_string(), msg_str);
                                }
                                if let JsValue::Object(opts) = &options {
                                    if let Some(opts_obj) = interp.get_object(opts.id) {
                                        if opts_obj.borrow().has_property("cause") {
                                            let cause = interp
                                                .get_object_property(opts.id, "cause", &options);
                                            match cause {
                                                Completion::Normal(v) => {
                                                    $o.insert_builtin("cause".to_string(), v);
                                                }
                                                c => return c,
                                            }
                                        }
                                    }
                                }
                            };
                        }

                        if let JsValue::Object(o) = this {
                            if let Some(obj) = interp.get_object(o.id) {
                                let mut o = obj.borrow_mut();
                                init_agg_error!(o);
                            }
                            return Completion::Normal(this.clone());
                        }
                        let obj = interp.create_object();
                        {
                            let mut o = obj.borrow_mut();
                            init_agg_error!(o);
                        }
                        let id = obj.borrow().id.unwrap();
                        Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
                    },
                ),
            );
            {
                let env = self.global_env.borrow();
                if let Some(ctor_val) = env.get("AggregateError") {
                    agg_proto
                        .borrow_mut()
                        .insert_builtin("constructor".to_string(), ctor_val);
                }
            }
            {
                let env = self.global_env.borrow();
                if let Some(ctor_val) = env.get("AggregateError")
                    && let JsValue::Object(o) = &ctor_val
                    && let Some(ctor_obj) = self.get_object(o.id)
                {
                    let proto_id = agg_proto.borrow().id.unwrap();
                    ctor_obj.borrow_mut().insert_property(
                        "prototype".to_string(),
                        PropertyDescriptor::data(JsValue::Object(crate::types::JsObject { id: proto_id }), false, false, false),
                    );
                }
            }
        }

        // Object constructor (minimal)
        self.register_global_fn(
            "Object",
            BindingKind::Var,
            JsFunction::constructor("Object".to_string(), 1, |interp, _this, args| {
                match args.first() {
                    Some(val) if matches!(val, JsValue::Object(_)) => {
                        Completion::Normal(val.clone())
                    }
                    Some(val) if !matches!(val, JsValue::Undefined | JsValue::Null) => {
                        interp.to_object(val)
                    }
                    _ => {
                        let obj = interp.create_object();
                        let id = obj.borrow().id.unwrap();
                        Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
                    }
                }
            }),
        );

        self.setup_object_statics();

        // Array constructor (must be before setup_array_prototype so statics can be added)
        self.register_global_fn(
            "Array",
            BindingKind::Var,
            JsFunction::constructor("Array".to_string(), 1, |interp, _this, args| {
                if args.len() == 1
                    && let JsValue::Number(n) = &args[0]
                {
                    let arr = interp.create_array(vec![JsValue::Undefined; *n as usize]);
                    return Completion::Normal(arr);
                }
                let arr = interp.create_array(args.to_vec());
                Completion::Normal(arr)
            }),
        );

        // Symbol — must be before iterator prototypes so @@iterator key is available
        {
            let symbol_fn = self.create_function(JsFunction::constructor(
                "Symbol".to_string(),
                0,
                |interp, _this, args| {
                    if interp.new_target.is_some() {
                        let err = interp.create_type_error("Symbol is not a constructor");
                        return Completion::Throw(err);
                    }
                    let desc = args.first().and_then(|v| {
                        if matches!(v, JsValue::Undefined) {
                            None
                        } else {
                            Some(JsString::from_str(&to_js_string(v)))
                        }
                    });
                    let id = interp.next_symbol_id;
                    interp.next_symbol_id += 1;
                    Completion::Normal(JsValue::Symbol(crate::types::JsSymbol {
                        id,
                        description: desc,
                    }))
                },
            ));
            if let JsValue::Object(ref o) = symbol_fn
                && let Some(obj) = self.get_object(o.id)
            {
                let well_known = [
                    ("iterator", "Symbol.iterator"),
                    ("hasInstance", "Symbol.hasInstance"),
                    ("toPrimitive", "Symbol.toPrimitive"),
                    ("toStringTag", "Symbol.toStringTag"),
                    ("isConcatSpreadable", "Symbol.isConcatSpreadable"),
                    ("species", "Symbol.species"),
                    ("match", "Symbol.match"),
                    ("replace", "Symbol.replace"),
                    ("search", "Symbol.search"),
                    ("split", "Symbol.split"),
                    ("matchAll", "Symbol.matchAll"),
                    ("unscopables", "Symbol.unscopables"),
                    ("asyncIterator", "Symbol.asyncIterator"),
                    ("dispose", "Symbol.dispose"),
                    ("asyncDispose", "Symbol.asyncDispose"),
                ];
                for (name, desc) in well_known {
                    let id = self.next_symbol_id;
                    self.next_symbol_id += 1;
                    let sym = JsValue::Symbol(crate::types::JsSymbol {
                        id,
                        description: Some(JsString::from_str(desc)),
                    });
                    obj.borrow_mut().insert_property(
                        name.to_string(),
                        PropertyDescriptor::data(sym, false, false, false),
                    );
                }

                // Symbol.for
                let for_fn = self.create_function(JsFunction::Native(
                    "for".to_string(),
                    1,
                    Rc::new(|interp, _this, args| {
                        let key = args.first().map(to_js_string).unwrap_or_default();
                        if let Some(existing) = interp.global_symbol_registry.get(&key) {
                            return Completion::Normal(JsValue::Symbol(existing.clone()));
                        }
                        let id = interp.next_symbol_id;
                        interp.next_symbol_id += 1;
                        let sym = crate::types::JsSymbol {
                            id,
                            description: Some(JsString::from_str(&key)),
                        };
                        interp.global_symbol_registry.insert(key, sym.clone());
                        Completion::Normal(JsValue::Symbol(sym))
                    }),
                    false,
                ));
                obj.borrow_mut().insert_builtin("for".to_string(), for_fn);

                // Symbol.keyFor
                let key_for_fn = self.create_function(JsFunction::Native(
                    "keyFor".to_string(),
                    1,
                    Rc::new(|interp, _this, args| {
                        let Some(JsValue::Symbol(sym)) = args.first() else {
                            let err = interp
                                .create_type_error("Symbol.keyFor requires a symbol argument");
                            return Completion::Throw(err);
                        };
                        for (key, reg_sym) in &interp.global_symbol_registry {
                            if reg_sym.id == sym.id {
                                return Completion::Normal(JsValue::String(JsString::from_str(
                                    key,
                                )));
                            }
                        }
                        Completion::Normal(JsValue::Undefined)
                    }),
                    false,
                ));
                obj.borrow_mut()
                    .insert_builtin("keyFor".to_string(), key_for_fn);
            }
            self.global_env
                .borrow_mut()
                .declare("Symbol", BindingKind::Var);
            let _ = self.global_env.borrow_mut().set("Symbol", symbol_fn);
        }

        self.setup_iterator_prototypes();
        self.setup_generator_prototype();
        self.setup_async_generator_prototype();
        self.setup_array_prototype();
        // String constructor/converter — must be before setup_string_prototype
        self.register_global_fn(
            "String",
            BindingKind::Var,
            JsFunction::constructor("String".to_string(), 1, |interp, this, args| {
                let s = if args.is_empty() {
                    String::new()
                } else {
                    let val = &args[0];
                    // §22.1.1.1: If value is Symbol, return SymbolDescriptiveString
                    if let JsValue::Symbol(sym) = val {
                        if let Some(desc) = &sym.description {
                            format!("Symbol({desc})")
                        } else {
                            "Symbol()".to_string()
                        }
                    } else {
                        match interp.to_string_value(val) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                };
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    obj.borrow_mut().primitive_value =
                        Some(JsValue::String(JsString::from_str(&s)));
                    obj.borrow_mut().class_name = "String".to_string();
                }
                Completion::Normal(JsValue::String(JsString::from_str(&s)))
            }),
        );
        self.setup_string_prototype();

        // String.raw
        {
            let raw_fn = self.create_function(JsFunction::native(
                "raw".to_string(),
                1,
                |interp, _this, args| {
                    let template = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let template_obj = match interp.to_object(&template) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let raw_val = if let JsValue::Object(o) = &template_obj {
                        match interp.get_object_property(o.id, "raw", &template_obj) {
                            Completion::Normal(v) => v,
                            other => return other,
                        }
                    } else {
                        JsValue::Undefined
                    };
                    let raw_obj = match interp.to_object(&raw_val) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let len = if let JsValue::Object(o) = &raw_obj {
                        let length_val = match interp.get_object_property(o.id, "length", &raw_obj)
                        {
                            Completion::Normal(v) => v,
                            _ => JsValue::Number(0.0),
                        };
                        let n = match interp.to_number_value(&length_val) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        };
                        if n.is_nan() || n < 0.0 {
                            0usize
                        } else {
                            n as usize
                        }
                    } else {
                        0
                    };
                    if len == 0 {
                        return Completion::Normal(JsValue::String(JsString::from_str("")));
                    }
                    let subs = &args[1..];
                    let mut result = String::new();
                    for i in 0..len {
                        let next_seg = if let JsValue::Object(o) = &raw_obj {
                            match interp.get_object_property(o.id, &i.to_string(), &raw_obj) {
                                Completion::Normal(v) => v,
                                _ => JsValue::Undefined,
                            }
                        } else {
                            JsValue::Undefined
                        };
                        let seg_str = match interp.to_string_value(&next_seg) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        };
                        result.push_str(&seg_str);
                        if i + 1 < len && i < subs.len() {
                            let sub_str = match interp.to_string_value(&subs[i]) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };
                            result.push_str(&sub_str);
                        }
                    }
                    Completion::Normal(JsValue::String(JsString::from_str(&result)))
                },
            ));
            let env = self.global_env.borrow();
            if let Some(string_ctor) = env.get("String")
                && let JsValue::Object(o) = &string_ctor
                && let Some(obj) = self.get_object(o.id)
            {
                obj.borrow_mut().insert_builtin("raw".to_string(), raw_fn);
            }
        }

        // Number constructor/converter
        self.register_global_fn(
            "Number",
            BindingKind::Var,
            JsFunction::constructor("Number".to_string(), 1, |interp, this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::Number(0.0));
                let n = if let JsValue::BigInt(ref b) = val {
                    let s = b.value.to_string();
                    s.parse::<f64>().unwrap_or(f64::INFINITY)
                } else {
                    to_number(&val)
                };
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    obj.borrow_mut().primitive_value = Some(JsValue::Number(n));
                    obj.borrow_mut().class_name = "Number".to_string();
                }
                Completion::Normal(JsValue::Number(n))
            }),
        );

        // Number static properties
        {
            let is_finite_fn = self.create_function(JsFunction::native(
                "isFinite".to_string(),
                1,
                |_interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let result = matches!(&val, JsValue::Number(n) if n.is_finite());
                    Completion::Normal(JsValue::Boolean(result))
                },
            ));
            let is_nan_fn = self.create_function(JsFunction::native(
                "isNaN".to_string(),
                1,
                |_interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let result = matches!(&val, JsValue::Number(n) if n.is_nan());
                    Completion::Normal(JsValue::Boolean(result))
                },
            ));
            let is_integer_fn = self.create_function(JsFunction::native(
                "isInteger".to_string(),
                1,
                |_interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let result = if let JsValue::Number(n) = &val {
                        n.is_finite() && *n == n.trunc()
                    } else {
                        false
                    };
                    Completion::Normal(JsValue::Boolean(result))
                },
            ));
            let is_safe_fn = self.create_function(JsFunction::native(
                "isSafeInteger".to_string(),
                1,
                |_interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let result = if let JsValue::Number(n) = &val {
                        n.is_finite() && *n == n.trunc() && n.abs() <= 9007199254740991.0
                    } else {
                        false
                    };
                    Completion::Normal(JsValue::Boolean(result))
                },
            ));
            if let Some(num_val) = self.global_env.borrow().get("Number")
                && let JsValue::Object(o) = &num_val
                && let Some(num_obj) = self.get_object(o.id)
            {
                let mut n = num_obj.borrow_mut();
                n.insert_property(
                    "POSITIVE_INFINITY".to_string(),
                    PropertyDescriptor::data(JsValue::Number(f64::INFINITY), false, false, false),
                );
                n.insert_property(
                    "NEGATIVE_INFINITY".to_string(),
                    PropertyDescriptor::data(JsValue::Number(f64::NEG_INFINITY), false, false, false),
                );
                n.insert_property("MAX_VALUE".to_string(), PropertyDescriptor::data(JsValue::Number(f64::MAX), false, false, false));
                n.insert_property("MIN_VALUE".to_string(), PropertyDescriptor::data(JsValue::Number(f64::MIN_POSITIVE), false, false, false));
                n.insert_property("NaN".to_string(), PropertyDescriptor::data(JsValue::Number(f64::NAN), false, false, false));
                n.insert_property("EPSILON".to_string(), PropertyDescriptor::data(JsValue::Number(f64::EPSILON), false, false, false));
                n.insert_property(
                    "MAX_SAFE_INTEGER".to_string(),
                    PropertyDescriptor::data(JsValue::Number(9007199254740991.0), false, false, false),
                );
                n.insert_property(
                    "MIN_SAFE_INTEGER".to_string(),
                    PropertyDescriptor::data(JsValue::Number(-9007199254740991.0), false, false, false),
                );
                n.insert_builtin("isFinite".to_string(), is_finite_fn);
                n.insert_builtin("isNaN".to_string(), is_nan_fn);
                n.insert_builtin("isInteger".to_string(), is_integer_fn);
                n.insert_builtin("isSafeInteger".to_string(), is_safe_fn);
            }
        }

        // Boolean constructor/converter
        self.register_global_fn(
            "Boolean",
            BindingKind::Var,
            JsFunction::constructor("Boolean".to_string(), 1, |interp, this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let b = to_boolean(&val);
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    obj.borrow_mut().primitive_value = Some(JsValue::Boolean(b));
                    obj.borrow_mut().class_name = "Boolean".to_string();
                }
                Completion::Normal(JsValue::Boolean(b))
            }),
        );

        self.setup_bigint_prototype();
        self.setup_symbol_prototype();
        self.cached_has_instance_key = self.get_symbol_key("hasInstance");
        self.setup_number_prototype();
        self.setup_boolean_prototype();
        self.setup_map_prototype();
        self.setup_set_prototype();
        self.setup_weakmap_prototype();
        self.setup_weakset_prototype();
        self.setup_weakref();
        self.setup_finalization_registry();
        self.setup_date_builtin();
        self.setup_disposable_stack();
        self.setup_async_disposable_stack();

        // Global functions
        self.register_global_fn(
            "parseInt",
            BindingKind::Var,
            JsFunction::native("parseInt".to_string(), 2, |_interp, _this, args| {
                let s = args.first().map(to_js_string).unwrap_or_default();
                let radix = args.get(1).map(|v| to_number(v) as i32).unwrap_or(10);
                let s = s.trim();
                let (negative, s) = if let Some(rest) = s.strip_prefix('-') {
                    (true, rest)
                } else if let Some(rest) = s.strip_prefix('+') {
                    (false, rest)
                } else {
                    (false, s)
                };
                let radix = if radix == 0 {
                    if s.starts_with("0x") || s.starts_with("0X") {
                        16
                    } else {
                        10
                    }
                } else {
                    radix
                };
                let s = if radix == 16 {
                    s.strip_prefix("0x")
                        .or_else(|| s.strip_prefix("0X"))
                        .unwrap_or(s)
                } else {
                    s
                };
                match i64::from_str_radix(s, radix as u32) {
                    Ok(n) => {
                        let n = if negative { -n } else { n };
                        Completion::Normal(JsValue::Number(n as f64))
                    }
                    Err(_) => Completion::Normal(JsValue::Number(f64::NAN)),
                }
            }),
        );

        self.register_global_fn(
            "parseFloat",
            BindingKind::Var,
            JsFunction::native("parseFloat".to_string(), 1, |_interp, _this, args| {
                let s = args.first().map(to_js_string).unwrap_or_default();
                let s = s.trim();
                match s.parse::<f64>() {
                    Ok(n) => Completion::Normal(JsValue::Number(n)),
                    Err(_) => Completion::Normal(JsValue::Number(f64::NAN)),
                }
            }),
        );

        // Attach parseInt/parseFloat to Number constructor (must be after global registration)
        {
            let parse_int = self.global_env.borrow().get("parseInt");
            let parse_float = self.global_env.borrow().get("parseFloat");
            if let Some(num_val) = self.global_env.borrow().get("Number")
                && let JsValue::Object(o) = &num_val
                && let Some(num_obj) = self.get_object(o.id)
            {
                let mut n = num_obj.borrow_mut();
                if let Some(pi) = parse_int {
                    n.insert_builtin("parseInt".to_string(), pi);
                }
                if let Some(pf) = parse_float {
                    n.insert_builtin("parseFloat".to_string(), pf);
                }
            }
        }

        self.register_global_fn(
            "isNaN",
            BindingKind::Var,
            JsFunction::native("isNaN".to_string(), 1, |_interp, _this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let n = to_number(&val);
                Completion::Normal(JsValue::Boolean(n.is_nan()))
            }),
        );

        self.register_global_fn(
            "isFinite",
            BindingKind::Var,
            JsFunction::native("isFinite".to_string(), 1, |_interp, _this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let n = to_number(&val);
                Completion::Normal(JsValue::Boolean(n.is_finite()))
            }),
        );

        self.register_global_fn(
            "encodeURI",
            BindingKind::Var,
            JsFunction::native("encodeURI".to_string(), 1, |interp, _this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let code_units = match &val {
                    JsValue::String(s) => s.code_units.clone(),
                    other => JsString::from_str(&to_js_string(other)).code_units,
                };
                match encode_uri_string(&code_units, true) {
                    Ok(encoded) => {
                        Completion::Normal(JsValue::String(JsString::from_str(&encoded)))
                    }
                    Err(msg) => Completion::Throw(interp.create_error("URIError", &msg)),
                }
            }),
        );

        self.register_global_fn(
            "encodeURIComponent",
            BindingKind::Var,
            JsFunction::native(
                "encodeURIComponent".to_string(),
                1,
                |interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let code_units = match &val {
                        JsValue::String(s) => s.code_units.clone(),
                        other => JsString::from_str(&to_js_string(other)).code_units,
                    };
                    match encode_uri_string(&code_units, false) {
                        Ok(encoded) => {
                            Completion::Normal(JsValue::String(JsString::from_str(&encoded)))
                        }
                        Err(msg) => Completion::Throw(interp.create_error("URIError", &msg)),
                    }
                },
            ),
        );

        self.register_global_fn(
            "decodeURI",
            BindingKind::Var,
            JsFunction::native("decodeURI".to_string(), 1, |interp, _this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let code_units = match &val {
                    JsValue::String(s) => s.code_units.clone(),
                    other => JsString::from_str(&to_js_string(other)).code_units,
                };
                match decode_uri_string(&code_units, true) {
                    Ok(decoded) => Completion::Normal(JsValue::String(JsString {
                        code_units: decoded,
                    })),
                    Err(msg) => Completion::Throw(interp.create_error("URIError", &msg)),
                }
            }),
        );

        self.register_global_fn(
            "decodeURIComponent",
            BindingKind::Var,
            JsFunction::native(
                "decodeURIComponent".to_string(),
                1,
                |interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let code_units = match &val {
                        JsValue::String(s) => s.code_units.clone(),
                        other => JsString::from_str(&to_js_string(other)).code_units,
                    };
                    match decode_uri_string(&code_units, false) {
                        Ok(decoded) => Completion::Normal(JsValue::String(JsString {
                            code_units: decoded,
                        })),
                        Err(msg) => Completion::Throw(interp.create_error("URIError", &msg)),
                    }
                },
            ),
        );

        // Annex B: escape()
        self.register_global_fn(
            "escape",
            BindingKind::Var,
            JsFunction::native("escape".to_string(), 1, |_interp, _this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let s = to_js_string(&val);
                let units: Vec<u16> = s.encode_utf16().collect();
                let mut result = String::new();
                for &cu in &units {
                    match cu {
                        // A-Z a-z 0-9 @ * _ + - . /
                        b if (b'A' as u16..=b'Z' as u16).contains(&b)
                            || (b'a' as u16..=b'z' as u16).contains(&b)
                            || (b'0' as u16..=b'9' as u16).contains(&b)
                            || b == b'@' as u16
                            || b == b'*' as u16
                            || b == b'_' as u16
                            || b == b'+' as u16
                            || b == b'-' as u16
                            || b == b'.' as u16
                            || b == b'/' as u16 =>
                        {
                            result.push(cu as u8 as char);
                        }
                        b if b <= 0xFF => {
                            result.push_str(&format!("%{:02X}", b));
                        }
                        _ => {
                            result.push_str(&format!("%u{:04X}", cu));
                        }
                    }
                }
                Completion::Normal(JsValue::String(JsString::from_str(&result)))
            }),
        );

        // Annex B: unescape()
        self.register_global_fn(
            "unescape",
            BindingKind::Var,
            JsFunction::native("unescape".to_string(), 1, |_interp, _this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let s = to_js_string(&val);
                let chars: Vec<char> = s.chars().collect();
                let mut result: Vec<u16> = Vec::new();
                let mut i = 0;
                while i < chars.len() {
                    if chars[i] == '%' {
                        if i + 5 < chars.len()
                            && chars[i + 1] == 'u'
                            && chars[i + 2..i + 6].iter().all(|c| c.is_ascii_hexdigit())
                        {
                            let hex: String = chars[i + 2..i + 6].iter().collect();
                            if let Ok(code) = u16::from_str_radix(&hex, 16) {
                                result.push(code);
                                i += 6;
                                continue;
                            }
                        }
                        if i + 2 < chars.len()
                            && chars[i + 1..i + 3].iter().all(|c| c.is_ascii_hexdigit())
                        {
                            let hex: String = chars[i + 1..i + 3].iter().collect();
                            if let Ok(code) = u8::from_str_radix(&hex, 16) {
                                result.push(code as u16);
                                i += 3;
                                continue;
                            }
                        }
                    }
                    let ch = chars[i];
                    let mut buf = [0u16; 2];
                    for u in ch.encode_utf16(&mut buf) {
                        result.push(*u);
                    }
                    i += 1;
                }
                Completion::Normal(JsValue::String(JsString { code_units: result }))
            }),
        );

        // Math object
        let math_obj = self.create_object();
        let math_id = math_obj.borrow().id.unwrap();
        {
            let mut m = math_obj.borrow_mut();
            m.class_name = "Math".to_string();
            let math_consts: &[(&str, f64)] = &[
                ("PI", std::f64::consts::PI),
                ("E", std::f64::consts::E),
                ("LN2", std::f64::consts::LN_2),
                ("LN10", std::f64::consts::LN_10),
                ("LOG2E", std::f64::consts::LOG2_E),
                ("LOG10E", std::f64::consts::LOG10_E),
                ("SQRT2", std::f64::consts::SQRT_2),
                ("SQRT1_2", std::f64::consts::FRAC_1_SQRT_2),
            ];
            for (name, val) in math_consts {
                m.insert_property(
                    name.to_string(),
                    PropertyDescriptor::data(JsValue::Number(*val), false, false, false),
                );
            }
        }
        // Add Math methods
        let math_fns: Vec<(&str, fn(f64) -> f64)> = vec![
            ("abs", f64::abs),
            ("ceil", f64::ceil),
            ("floor", f64::floor),
            ("round", f64::round),
            ("sqrt", f64::sqrt),
            ("sin", f64::sin),
            ("cos", f64::cos),
            ("tan", f64::tan),
            ("log", f64::ln),
            ("exp", f64::exp),
            ("asin", f64::asin),
            ("acos", f64::acos),
            ("atan", f64::atan),
            ("trunc", f64::trunc),
            (
                "sign",
                (|x: f64| {
                    if x.is_nan() || x == 0.0 {
                        x
                    } else if x > 0.0 {
                        1.0
                    } else {
                        -1.0
                    }
                }) as fn(f64) -> f64,
            ),
            ("cbrt", f64::cbrt),
        ];
        for (name, op) in math_fns {
            let fn_val = self.create_function(JsFunction::native(
                name.to_string(),
                1,
                move |_interp, _this, args| {
                    let x = args.first().map(to_number).unwrap_or(f64::NAN);
                    Completion::Normal(JsValue::Number(op(x)))
                },
            ));
            math_obj
                .borrow_mut()
                .insert_builtin(name.to_string(), fn_val);
        }
        // Math.max, Math.min, Math.pow, Math.random, Math.atan2
        let max_fn = self.create_function(JsFunction::native(
            "max".to_string(),
            2,
            |_interp, _this, args| {
                if args.is_empty() {
                    return Completion::Normal(JsValue::Number(f64::NEG_INFINITY));
                }
                let mut result = f64::NEG_INFINITY;
                for a in args {
                    let n = to_number(a);
                    if n.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    if n > result {
                        result = n;
                    }
                }
                Completion::Normal(JsValue::Number(result))
            },
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("max".to_string(), max_fn);
        let min_fn = self.create_function(JsFunction::native(
            "min".to_string(),
            2,
            |_interp, _this, args| {
                if args.is_empty() {
                    return Completion::Normal(JsValue::Number(f64::INFINITY));
                }
                let mut result = f64::INFINITY;
                for a in args {
                    let n = to_number(a);
                    if n.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    if n < result {
                        result = n;
                    }
                }
                Completion::Normal(JsValue::Number(result))
            },
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("min".to_string(), min_fn);
        let pow_fn = self.create_function(JsFunction::native(
            "pow".to_string(),
            2,
            |_interp, _this, args| {
                let base = args.first().map(to_number).unwrap_or(f64::NAN);
                let exp = args.get(1).map(to_number).unwrap_or(f64::NAN);
                Completion::Normal(JsValue::Number(base.powf(exp)))
            },
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("pow".to_string(), pow_fn);
        let random_fn = self.create_function(JsFunction::native(
            "random".to_string(),
            0,
            |_interp, _this, _args| {
                Completion::Normal(JsValue::Number(0.5)) // deterministic for testing
            },
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("random".to_string(), random_fn);

        // Math.atan2
        let atan2_fn = self.create_function(JsFunction::native(
            "atan2".to_string(),
            2,
            |_interp, _this, args| {
                let y = args.first().map(to_number).unwrap_or(f64::NAN);
                let x = args.get(1).map(to_number).unwrap_or(f64::NAN);
                Completion::Normal(JsValue::Number(y.atan2(x)))
            },
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("atan2".to_string(), atan2_fn);

        // Math.hypot
        let hypot_fn = self.create_function(JsFunction::native(
            "hypot".to_string(),
            2,
            |_interp, _this, args| {
                if args.is_empty() {
                    return Completion::Normal(JsValue::Number(0.0));
                }
                let mut sum = 0.0f64;
                for a in args {
                    let n = to_number(a);
                    if n.is_infinite() {
                        return Completion::Normal(JsValue::Number(f64::INFINITY));
                    }
                    if n.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    sum += n * n;
                }
                Completion::Normal(JsValue::Number(sum.sqrt()))
            },
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("hypot".to_string(), hypot_fn);

        // Math.log2, Math.log10
        let log2_fn = self.create_function(JsFunction::native(
            "log2".to_string(),
            1,
            |_interp, _this, args| {
                let x = args.first().map(to_number).unwrap_or(f64::NAN);
                Completion::Normal(JsValue::Number(x.log2()))
            },
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("log2".to_string(), log2_fn);
        let log10_fn = self.create_function(JsFunction::native(
            "log10".to_string(),
            1,
            |_interp, _this, args| {
                let x = args.first().map(to_number).unwrap_or(f64::NAN);
                Completion::Normal(JsValue::Number(x.log10()))
            },
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("log10".to_string(), log10_fn);

        // Math.fround
        let fround_fn = self.create_function(JsFunction::native(
            "fround".to_string(),
            1,
            |_interp, _this, args| {
                let x = args.first().map(to_number).unwrap_or(f64::NAN);
                Completion::Normal(JsValue::Number((x as f32) as f64))
            },
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("fround".to_string(), fround_fn);

        // Math.clz32
        let clz32_fn = self.create_function(JsFunction::native(
            "clz32".to_string(),
            1,
            |_interp, _this, args| {
                let x = args.first().map(to_number).unwrap_or(0.0);
                let n = number_ops::to_uint32(x);
                Completion::Normal(JsValue::Number(n.leading_zeros() as f64))
            },
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("clz32".to_string(), clz32_fn);

        // Math.imul
        let imul_fn = self.create_function(JsFunction::native(
            "imul".to_string(),
            2,
            |_interp, _this, args| {
                let a = args.first().map(to_number).unwrap_or(0.0);
                let b = args.get(1).map(to_number).unwrap_or(0.0);
                let ia = number_ops::to_int32(a);
                let ib = number_ops::to_int32(b);
                Completion::Normal(JsValue::Number(ia.wrapping_mul(ib) as f64))
            },
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("imul".to_string(), imul_fn);

        // Math.expm1, Math.log1p, Math.cosh, Math.sinh, Math.tanh, Math.acosh, Math.asinh, Math.atanh
        let extra_math_fns: Vec<(&str, fn(f64) -> f64)> = vec![
            ("expm1", f64::exp_m1),
            ("log1p", f64::ln_1p),
            ("cosh", f64::cosh),
            ("sinh", f64::sinh),
            ("tanh", f64::tanh),
            ("acosh", f64::acosh),
            ("asinh", f64::asinh),
            ("atanh", f64::atanh),
        ];
        for (name, op) in extra_math_fns {
            let fn_val = self.create_function(JsFunction::native(
                name.to_string(),
                1,
                move |_interp, _this, args| {
                    let x = args.first().map(to_number).unwrap_or(f64::NAN);
                    Completion::Normal(JsValue::Number(op(x)))
                },
            ));
            math_obj
                .borrow_mut()
                .insert_builtin(name.to_string(), fn_val);
        }

        // @@toStringTag
        {
            let desc = PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("Math"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            };
            let key = "Symbol(Symbol.toStringTag)".to_string();
            math_obj.borrow_mut().property_order.push(key.clone());
            math_obj.borrow_mut().properties.insert(key, desc);
        }

        let math_val = JsValue::Object(crate::types::JsObject { id: math_id });
        self.global_env
            .borrow_mut()
            .declare("Math", BindingKind::Const);
        let _ = self.global_env.borrow_mut().set("Math", math_val);

        // eval
        self.register_global_fn(
            "eval",
            BindingKind::Var,
            JsFunction::native("eval".to_string(), 1, |interp, _this, args| {
                let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(&arg, JsValue::String(_)) {
                    return Completion::Normal(arg);
                }
                let code = to_js_string(&arg);
                let mut p = match parser::Parser::new(&code) {
                    Ok(p) => p,
                    Err(_) => {
                        return Completion::Throw(
                            interp.create_error("SyntaxError", "Invalid eval source"),
                        );
                    }
                };
                let program = match p.parse_program() {
                    Ok(prog) => prog,
                    Err(_) => {
                        return Completion::Throw(
                            interp.create_error("SyntaxError", "Invalid eval source"),
                        );
                    }
                };
                let is_strict = program.body.first().is_some_and(|s| {
                    matches!(s, Statement::Expression(Expression::Literal(Literal::String(s))) if s == "use strict")
                });
                let env = if is_strict {
                    Environment::new(Some(interp.global_env.clone()))
                } else {
                    interp.global_env.clone()
                };
                let mut last = Completion::Empty;
                for stmt in &program.body {
                    match interp.exec_statement(stmt, &env) {
                        Completion::Normal(v) => last = Completion::Normal(v),
                        Completion::Empty => {}
                        other => return other,
                    }
                }
                last.update_empty(JsValue::Undefined)
            }),
        );

        self.register_global_fn(
            "$DONOTEVALUATE",
            BindingKind::Var,
            JsFunction::native("$DONOTEVALUATE".to_string(), 0, |_interp, _this, _args| {
                Completion::Throw(JsValue::String(JsString::from_str(
                    "Test262: $DONOTEVALUATE was called",
                )))
            }),
        );

        // Function constructor
        self.register_global_fn(
            "Function",
            BindingKind::Var,
            JsFunction::constructor("Function".to_string(), 1, |interp, _this, args| {
                let (params_str, body_str) = if args.is_empty() {
                    (String::new(), String::new())
                } else if args.len() == 1 {
                    (String::new(), to_js_string(&args[0]))
                } else {
                    let params: Vec<String> =
                        args[..args.len() - 1].iter().map(to_js_string).collect();
                    (params.join(","), to_js_string(args.last().unwrap()))
                };

                let fn_source_text = format!("function anonymous({}\n) {{\n{}\n}}", params_str, body_str);
                let source = format!("(function anonymous({}) {{ {} }})", params_str, body_str);
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
                    program.body.first()
                {
                    let is_strict = fe.body.first().is_some_and(|s| {
                        matches!(s, Statement::Expression(Expression::Literal(Literal::String(s))) if s == "use strict")
                    });
                    let js_func = JsFunction::User {
                        name: Some("anonymous".to_string()),
                        params: fe.params.clone(),
                        body: fe.body.clone(),
                        closure: interp.global_env.clone(),
                        is_arrow: false,
                        is_strict,
                        is_generator: false,
                        is_async: false,
                        source_text: Some(fn_source_text),
                    };
                    Completion::Normal(interp.create_function(js_func))
                } else {
                    Completion::Throw(
                        interp.create_error("SyntaxError", "Failed to parse function"),
                    )
                }
            }),
        );

        // Per spec §20.2.3, Function.prototype is itself a function object
        {
            let func_val = self.global_env.borrow().get("Function");
            if let Some(JsValue::Object(fo)) = func_val {
                if let Some(func_data) = self.get_object(fo.id) {
                    let pv = func_data.borrow().get_property("prototype");
                    if let JsValue::Object(pr) = pv {
                        if let Some(proto_obj) = self.get_object(pr.id) {
                            proto_obj.borrow_mut().callable = Some(JsFunction::native(
                                "".to_string(),
                                0,
                                |_interp, _this, _args| Completion::Normal(JsValue::Undefined),
                            ));
                            proto_obj.borrow_mut().insert_property(
                                "length".to_string(),
                                PropertyDescriptor::data(JsValue::Number(0.0), false, false, true),
                            );
                            proto_obj.borrow_mut().insert_property(
                                "name".to_string(),
                                PropertyDescriptor::data(
                                    JsValue::String(JsString::from_str("")),
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

        // Add Function.prototype[@@hasInstance]
        if let Some(sym_key) = self.get_symbol_key("hasInstance") {
            let func_val = self.global_env.borrow().get("Function");
            let proto_data = func_val.and_then(|fv| {
                if let JsValue::Object(fo) = fv {
                    self.get_object(fo.id).and_then(|fd| {
                        let pv = fd.borrow().get_property("prototype");
                        if let JsValue::Object(pr) = pv {
                            self.get_object(pr.id)
                        } else {
                            None
                        }
                    })
                } else {
                    None
                }
            });
            if let Some(proto_data) = proto_data {
                let has_instance_fn = self.create_function(JsFunction::native(
                    "[Symbol.hasInstance]".to_string(),
                    1,
                    |interp, this_val, args| {
                        let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                        interp.ordinary_has_instance(this_val, &arg)
                    },
                ));
                proto_data.borrow_mut().insert_property(
                    sym_key,
                    PropertyDescriptor::data(has_instance_fn, false, false, false),
                );
            }
        }

        // Store Function.prototype for use as [[Prototype]] of all function objects
        {
            let func_val = self.global_env.borrow().get("Function");
            if let Some(JsValue::Object(fo)) = func_val {
                if let Some(func_data) = self.get_object(fo.id) {
                    let pv = func_data.borrow().get_property("prototype");
                    if let JsValue::Object(pr) = pv {
                        if let Some(fp) = self.get_object(pr.id) {
                            // Set Function.prototype's [[Prototype]] to Object.prototype
                            if fp.borrow().prototype.is_none() {
                                fp.borrow_mut().prototype = self.object_prototype.clone();
                            }
                            // Install call/apply/bind/toString on Function.prototype
                            self.setup_function_prototype(&fp);
                            self.function_prototype = Some(fp.clone());

                            // Retroactively fix [[Prototype]] of all functions created before
                            // Function was registered
                            let bindings: Vec<JsValue> = self
                                .global_env
                                .borrow()
                                .bindings
                                .values()
                                .map(|b| b.value.clone())
                                .collect();
                            for val in bindings {
                                if let JsValue::Object(o) = &val {
                                    if let Some(obj) = self.get_object(o.id) {
                                        let is_func = obj.borrow().callable.is_some();
                                        if is_func {
                                            obj.borrow_mut().prototype = Some(fp.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // %AsyncFunction.prototype%
        // Per spec, this should inherit from Function.prototype
        {
            let af_proto = self.create_object();
            af_proto.borrow_mut().class_name = "AsyncFunction".to_string();

            // [[Prototype]] = Function.prototype
            if let Some(func_val) = self.global_env.borrow().get("Function") {
                if let JsValue::Object(func_obj) = func_val {
                    if let Some(func_data) = self.get_object(func_obj.id) {
                        if let JsValue::Object(func_proto_obj) =
                            func_data.borrow().get_property("prototype")
                        {
                            if let Some(func_proto) = self.get_object(func_proto_obj.id) {
                                af_proto.borrow_mut().prototype = Some(func_proto);
                            }
                        }
                    }
                }
            }

            // Symbol.toStringTag = "AsyncFunction"
            af_proto.borrow_mut().insert_property(
                "Symbol(Symbol.toStringTag)".to_string(),
                PropertyDescriptor::data(
                    JsValue::String(JsString::from_str("AsyncFunction")),
                    false,
                    false,
                    true,
                ),
            );

            self.async_function_prototype = Some(af_proto);
        }

        // AsyncFunction constructor (not a global per spec)
        // Create the constructor and wire it up with AsyncFunction.prototype
        if let Some(af_proto) = self.async_function_prototype.clone() {
            let af_ctor = self.create_function(JsFunction::constructor(
                "AsyncFunction".to_string(),
                1,
                |interp, _this, args| {
                    let (params_str, body_str) = if args.is_empty() {
                        (String::new(), String::new())
                    } else if args.len() == 1 {
                        (String::new(), to_js_string(&args[0]))
                    } else {
                        let params: Vec<String> =
                            args[..args.len() - 1].iter().map(to_js_string).collect();
                        (params.join(","), to_js_string(args.last().unwrap()))
                    };

                    let fn_source_text =
                        format!("async function anonymous({}\n) {{\n{}\n}}", params_str, body_str);
                    let source =
                        format!("(async function anonymous({}) {{ {} }})", params_str, body_str);
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
                        program.body.first()
                    {
                        let is_strict = fe.body.first().is_some_and(|s| {
                            matches!(s, Statement::Expression(Expression::Literal(Literal::String(s))) if s == "use strict")
                        });
                        let js_func = JsFunction::User {
                            name: Some("anonymous".to_string()),
                            params: fe.params.clone(),
                            body: fe.body.clone(),
                            closure: interp.global_env.clone(),
                            is_arrow: false,
                            is_strict,
                            is_generator: false,
                            is_async: true,
                            source_text: Some(fn_source_text),
                        };
                        Completion::Normal(interp.create_function(js_func))
                    } else {
                        Completion::Throw(
                            interp.create_error("SyntaxError", "Failed to parse async function"),
                        )
                    }
                },
            ));
            // Wire up AsyncFunction.prototype and constructor property
            if let JsValue::Object(af_obj) = &af_ctor {
                if let Some(af) = self.get_object(af_obj.id) {
                    let proto_id = af_proto.borrow().id.unwrap();
                    // Set AsyncFunction.prototype
                    af.borrow_mut().insert_property(
                        "prototype".to_string(),
                        PropertyDescriptor::data(
                            JsValue::Object(crate::types::JsObject { id: proto_id }),
                            false,
                            false,
                            false,
                        ),
                    );
                    // Set constructor back-reference on AsyncFunction.prototype
                    af_proto.borrow_mut().insert_property(
                        "constructor".to_string(),
                        PropertyDescriptor::data(
                            af_ctor.clone(),
                            true,
                            false,
                            true,
                        ),
                    );
                }
            }
        }

        // GeneratorFunction constructor (not a global per spec)
        // Create the constructor and wire it up with GeneratorFunction.prototype
        if let Some(gf_proto) = self.generator_function_prototype.clone() {
            let gf_ctor = self.create_function(JsFunction::constructor(
                "GeneratorFunction".to_string(),
                1,
                |interp, _this, args| {
                    let (params_str, body_str) = if args.is_empty() {
                        (String::new(), String::new())
                    } else if args.len() == 1 {
                        (String::new(), to_js_string(&args[0]))
                    } else {
                        let params: Vec<String> =
                            args[..args.len() - 1].iter().map(to_js_string).collect();
                        (params.join(","), to_js_string(args.last().unwrap()))
                    };

                    let fn_source_text = format!(
                        "function* anonymous({}\n) {{\n{}\n}}",
                        params_str, body_str
                    );
                    let source =
                        format!("(function* anonymous({}) {{ {} }})", params_str, body_str);
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
                        program.body.first()
                    {
                        let is_strict = fe.body.first().is_some_and(|s| {
                            matches!(s, Statement::Expression(Expression::Literal(Literal::String(s))) if s == "use strict")
                        });
                        let js_func = JsFunction::User {
                            name: Some("anonymous".to_string()),
                            params: fe.params.clone(),
                            body: fe.body.clone(),
                            closure: interp.global_env.clone(),
                            is_arrow: false,
                            is_strict,
                            is_generator: true,
                            is_async: false,
                            source_text: Some(fn_source_text),
                        };
                        Completion::Normal(interp.create_function(js_func))
                    } else {
                        Completion::Throw(
                            interp.create_error("SyntaxError", "Failed to parse generator function"),
                        )
                    }
                },
            ));
            // Wire up GeneratorFunction.prototype and constructor property
            if let JsValue::Object(gf_obj) = &gf_ctor {
                if let Some(gf) = self.get_object(gf_obj.id) {
                    let proto_id = gf_proto.borrow().id.unwrap();
                    // Set GeneratorFunction.prototype
                    gf.borrow_mut().insert_property(
                        "prototype".to_string(),
                        PropertyDescriptor::data(
                            JsValue::Object(crate::types::JsObject { id: proto_id }),
                            false,
                            false,
                            false,
                        ),
                    );
                    // Set constructor back-reference on GeneratorFunction.prototype
                    gf_proto.borrow_mut().insert_property(
                        "constructor".to_string(),
                        PropertyDescriptor::data(
                            gf_ctor.clone(),
                            true,
                            false,
                            true,
                        ),
                    );
                }
            }
        }

        // AsyncGeneratorFunction constructor (not a global per spec)
        // Create the constructor and wire it up with AsyncGeneratorFunction.prototype
        if let Some(agf_proto) = self.async_generator_function_prototype.clone() {
            let agf_ctor = self.create_function(JsFunction::constructor(
                "AsyncGeneratorFunction".to_string(),
                1,
                |interp, _this, args| {
                    let (params_str, body_str) = if args.is_empty() {
                        (String::new(), String::new())
                    } else if args.len() == 1 {
                        (String::new(), to_js_string(&args[0]))
                    } else {
                        let params: Vec<String> =
                            args[..args.len() - 1].iter().map(to_js_string).collect();
                        (params.join(","), to_js_string(args.last().unwrap()))
                    };

                    let fn_source_text = format!(
                        "async function* anonymous({}\n) {{\n{}\n}}",
                        params_str, body_str
                    );
                    let source =
                        format!("(async function* anonymous({}) {{ {} }})", params_str, body_str);
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
                        program.body.first()
                    {
                        let is_strict = fe.body.first().is_some_and(|s| {
                            matches!(s, Statement::Expression(Expression::Literal(Literal::String(s))) if s == "use strict")
                        });
                        let js_func = JsFunction::User {
                            name: Some("anonymous".to_string()),
                            params: fe.params.clone(),
                            body: fe.body.clone(),
                            closure: interp.global_env.clone(),
                            is_arrow: false,
                            is_strict,
                            is_generator: true,
                            is_async: true,
                            source_text: Some(fn_source_text),
                        };
                        Completion::Normal(interp.create_function(js_func))
                    } else {
                        Completion::Throw(interp.create_error(
                            "SyntaxError",
                            "Failed to parse async generator function",
                        ))
                    }
                },
            ));
            // Wire up AsyncGeneratorFunction.prototype and constructor property
            if let JsValue::Object(agf_obj) = &agf_ctor {
                if let Some(agf) = self.get_object(agf_obj.id) {
                    let proto_id = agf_proto.borrow().id.unwrap();
                    // Set AsyncGeneratorFunction.prototype
                    agf.borrow_mut().insert_property(
                        "prototype".to_string(),
                        PropertyDescriptor::data(
                            JsValue::Object(crate::types::JsObject { id: proto_id }),
                            false,
                            false,
                            false,
                        ),
                    );
                    // Set constructor back-reference on AsyncGeneratorFunction.prototype
                    agf_proto.borrow_mut().insert_property(
                        "constructor".to_string(),
                        PropertyDescriptor::data(
                            agf_ctor.clone(),
                            true,
                            false,
                            true,
                        ),
                    );
                }
            }
        }

        // JSON object
        let json_obj = self.create_object();
        let json_stringify = self.create_function(JsFunction::native(
            "stringify".to_string(),
            3,
            |interp, _this, args: &[JsValue]| {
                let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let replacer_arg = args.get(1).cloned();
                let space_arg = args.get(2).cloned().unwrap_or(JsValue::Undefined);

                // Process space argument
                let mut space_val = space_arg;
                // Unwrap wrapper objects
                if let JsValue::Object(o) = &space_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let cn = obj.borrow().class_name.clone();
                    let pv = obj.borrow().primitive_value.clone();
                    if cn == "Number" {
                        if let Some(p) = pv {
                            space_val = JsValue::Number(to_number(&p));
                        }
                    } else if cn == "String"
                        && let Some(p) = pv
                    {
                        space_val = p;
                    }
                }
                let gap = match &space_val {
                    JsValue::Number(n) => {
                        let count = (*n as i64).clamp(0, 10) as usize;
                        " ".repeat(count)
                    }
                    JsValue::String(s) => {
                        let rs = s.to_rust_string();
                        if rs.len() > 10 {
                            rs[..10].to_string()
                        } else {
                            rs
                        }
                    }
                    _ => String::new(),
                };

                let replacer = if matches!(&replacer_arg, Some(JsValue::Undefined) | None) {
                    None
                } else {
                    replacer_arg
                };

                match json_stringify_full(interp, &val, &replacer, &gap) {
                    Ok(Some(s)) => Completion::Normal(JsValue::String(JsString::from_str(&s))),
                    Ok(None) => Completion::Normal(JsValue::Undefined),
                    Err(e) => Completion::Throw(e),
                }
            },
        ));
        let json_parse = self.create_function(JsFunction::native(
            "parse".to_string(),
            2,
            |interp, _this, args: &[JsValue]| {
                let s = args.first().map(to_js_string).unwrap_or_default();
                let reviver = args.get(1).cloned();
                let result = json_parse_value(interp, &s);
                match result {
                    Completion::Normal(parsed) => {
                        if let Some(JsValue::Object(rev_obj)) = &reviver
                            && let Some(obj) = interp.get_object(rev_obj.id)
                            && obj.borrow().callable.is_some()
                        {
                            let wrapper = interp.create_object();
                            wrapper.borrow_mut().insert_value("".to_string(), parsed);
                            let wrapper_val = JsValue::Object(crate::types::JsObject {
                                id: wrapper.borrow().id.unwrap(),
                            });
                            return json_internalize(
                                interp,
                                &wrapper_val,
                                "",
                                reviver.as_ref().unwrap(),
                            );
                        }
                        Completion::Normal(parsed)
                    }
                    other => other,
                }
            },
        ));
        let json_raw_json = self.create_function(JsFunction::native(
            "rawJSON".to_string(),
            1,
            |interp, _this, args: &[JsValue]| {
                let text = args.first().map(to_js_string).unwrap_or_default();
                // Reject empty, leading/trailing whitespace
                if text.is_empty() {
                    let err = interp.create_error(
                        "SyntaxError",
                        "JSON.rawJSON cannot be called with an empty string",
                    );
                    return Completion::Throw(err);
                }
                let first = text.as_bytes()[0];
                let last = text.as_bytes()[text.len() - 1];
                if matches!(first, b'\t' | b'\n' | b'\r' | b' ')
                    || matches!(last, b'\t' | b'\n' | b'\r' | b' ')
                {
                    let err = interp.create_error(
                        "SyntaxError",
                        "JSON.rawJSON text must not start or end with whitespace",
                    );
                    return Completion::Throw(err);
                }
                // Must be a valid JSON primitive (not object/array)
                if text.starts_with('{') || text.starts_with('[') {
                    let err = interp
                        .create_error("SyntaxError", "JSON.rawJSON only accepts JSON primitives");
                    return Completion::Throw(err);
                }
                // Validate it's valid JSON
                if let Completion::Throw(e) = json_parse_value(interp, &text) {
                    return Completion::Throw(e);
                }
                let obj = interp.create_object();
                obj.borrow_mut().prototype = None;
                obj.borrow_mut().insert_builtin(
                    "rawJSON".to_string(),
                    JsValue::String(JsString::from_str(&text)),
                );
                obj.borrow_mut().extensible = false;
                obj.borrow_mut().is_raw_json = true;
                // Freeze: make all properties non-writable, non-configurable
                let keys: Vec<String> = obj.borrow().property_order.clone();
                for k in keys {
                    if let Some(desc) = obj.borrow_mut().properties.get_mut(&k) {
                        desc.writable = Some(false);
                        desc.configurable = Some(false);
                    }
                }
                let id = obj.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
            },
        ));
        let json_is_raw_json = self.create_function(JsFunction::native(
            "isRawJSON".to_string(),
            1,
            |interp, _this, args: &[JsValue]| {
                let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = &val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    return Completion::Normal(JsValue::Boolean(obj.borrow().is_raw_json));
                }
                Completion::Normal(JsValue::Boolean(false))
            },
        ));
        json_obj
            .borrow_mut()
            .insert_builtin("stringify".to_string(), json_stringify);
        json_obj
            .borrow_mut()
            .insert_builtin("parse".to_string(), json_parse);
        json_obj
            .borrow_mut()
            .insert_builtin("rawJSON".to_string(), json_raw_json);
        json_obj
            .borrow_mut()
            .insert_builtin("isRawJSON".to_string(), json_is_raw_json);
        // @@toStringTag
        {
            let desc = PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("JSON"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            };
            let key = "Symbol(Symbol.toStringTag)".to_string();
            json_obj.borrow_mut().property_order.push(key.clone());
            json_obj.borrow_mut().properties.insert(key, desc);
        }
        let json_val = JsValue::Object(crate::types::JsObject {
            id: json_obj.borrow().id.unwrap(),
        });
        self.global_env
            .borrow_mut()
            .declare("JSON", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("JSON", json_val);

        // String.fromCharCode
        {
            let string_ctor = self.global_env.borrow().get("String");
            if let Some(JsValue::Object(ref o)) = string_ctor {
                let from_char_code = self.create_function(JsFunction::native(
                    "fromCharCode".to_string(),
                    1,
                    |_interp, _this, args: &[JsValue]| {
                        let code_units: Vec<u16> = args
                            .iter()
                            .map(|a| {
                                let n = to_number(a) as u32;
                                (n & 0xFFFF) as u16
                            })
                            .collect();
                        Completion::Normal(JsValue::String(JsString { code_units }))
                    },
                ));
                let from_code_point = self.create_function(JsFunction::native(
                    "fromCodePoint".to_string(),
                    1,
                    |_interp, _this, args: &[JsValue]| {
                        let mut s = String::new();
                        for a in args {
                            let n = to_number(a) as u32;
                            if let Some(c) = char::from_u32(n) {
                                s.push(c);
                            }
                        }
                        Completion::Normal(JsValue::String(JsString::from_str(&s)))
                    },
                ));
                if let Some(obj) = self.get_object(o.id) {
                    obj.borrow_mut()
                        .insert_builtin("fromCharCode".to_string(), from_char_code);
                    obj.borrow_mut()
                        .insert_builtin("fromCodePoint".to_string(), from_code_point);
                }
            }
        }

        // RegExp constructor and prototype
        self.setup_regexp();

        // Reflect and Proxy built-ins
        self.setup_reflect();
        self.setup_proxy();

        // TypedArray, ArrayBuffer, DataView built-ins
        self.setup_typedarray_builtins();

        // Promise built-in
        self.setup_promise();

        // globalThis - create a global object
        let global_obj = self.create_object();
        let global_val = JsValue::Object(crate::types::JsObject {
            id: global_obj.borrow().id.unwrap(),
        });
        self.global_env
            .borrow_mut()
            .declare("globalThis", BindingKind::Var);
        let _ = self
            .global_env
            .borrow_mut()
            .set("globalThis", global_val.clone());
        self.global_env.borrow_mut().bindings.insert(
            "this".to_string(),
            Binding {
                value: global_val,
                kind: BindingKind::Const,
                initialized: true,
            },
        );

        // Populate globalThis with built-in constructors and functions as
        // non-enumerable, writable, configurable properties (per spec §19.1)
        let global_names = [
            "Object", "Function", "Array", "String", "Number", "Boolean",
            "Symbol", "Error", "SyntaxError", "TypeError", "ReferenceError",
            "RangeError", "URIError", "EvalError", "Date", "RegExp", "Map",
            "Set", "WeakMap", "WeakSet", "WeakRef", "FinalizationRegistry",
            "Promise", "ArrayBuffer", "DataView", "JSON", "Math", "Reflect",
            "Proxy", "eval", "parseInt", "parseFloat", "isNaN", "isFinite",
            "encodeURI", "decodeURI", "encodeURIComponent",
            "decodeURIComponent", "NaN", "Infinity", "undefined",
            "Int8Array", "Uint8Array", "Uint8ClampedArray", "Int16Array",
            "Uint16Array", "Int32Array", "Uint32Array", "Float32Array",
            "Float64Array", "BigInt64Array", "BigUint64Array", "BigInt",
            "AggregateError",
        ];
        let vals: Vec<(String, JsValue)> = {
            let env = self.global_env.borrow();
            global_names
                .iter()
                .filter_map(|name| {
                    env.get(name).map(|v| (name.to_string(), v))
                })
                .collect()
        };
        for (name, val) in vals {
            let (writable, configurable) = match name.as_str() {
                "NaN" | "Infinity" | "undefined" => (false, false),
                _ => (true, true),
            };
            global_obj.borrow_mut().insert_property(
                name,
                PropertyDescriptor::data(val, writable, false, configurable),
            );
        }
        // Also set globalThis on itself
        let gt_val = JsValue::Object(crate::types::JsObject {
            id: global_obj.borrow().id.unwrap(),
        });
        global_obj.borrow_mut().insert_property(
            "globalThis".to_string(),
            PropertyDescriptor::data(gt_val, true, false, true),
        );

        // Fix .prototype descriptors on built-in constructors.
        // create_function sets writable=true (correct for user-defined constructors per §10.2.5),
        // but built-in constructors need writable=false per their respective spec sections.
        let builtin_ctors = [
            "Object", "Function", "Array", "RegExp", "Promise", "Error",
            "TypeError", "RangeError", "SyntaxError", "ReferenceError",
            "URIError", "EvalError", "DataView", "ArrayBuffer", "SharedArrayBuffer",
            "WeakRef", "FinalizationRegistry",
        ];
        let ctor_vals: Vec<JsValue> = {
            let env = self.global_env.borrow();
            builtin_ctors.iter().filter_map(|name| env.get(name)).collect()
        };
        for ctor_val in &ctor_vals {
            if let JsValue::Object(o) = ctor_val
                && let Some(ctor_obj) = self.get_object(o.id)
            {
                let proto_val = ctor_obj.borrow().get_property_value("prototype");
                if let Some(val) = proto_val {
                    ctor_obj.borrow_mut().insert_property(
                        "prototype".to_string(),
                        PropertyDescriptor::data(val, false, false, false),
                    );
                }
            }
        }

        // Wire up global object as backing for global environment lookups
        // Per spec §9.1.1.4, the Global Environment Record has an Object Environment
        // Record whose binding object is the global object. Variable lookups in global
        // scope should check global object properties.
        self.global_env.borrow_mut().global_object = Some(global_obj);
    }

    fn setup_object_statics(&mut self) {
        // Get the Object function from global env
        let obj_func_val = self
            .global_env
            .borrow()
            .get("Object")
            .unwrap_or(JsValue::Undefined);
        if let JsValue::Object(ref o) = obj_func_val
            && let Some(obj_func) = self.get_object(o.id)
        {
            // Get prototype property
            let proto_val = obj_func.borrow().get_property_value("prototype");
            if let Some(JsValue::Object(ref proto_ref)) = proto_val
                && let Some(proto_obj) = self.get_object(proto_ref.id)
            {
                self.object_prototype = Some(proto_obj.clone());

                // Fix Error.prototype chain - created before object_prototype was available
                {
                    let env = self.global_env.borrow();
                    for name in [
                        "Error",
                        "SyntaxError",
                        "TypeError",
                        "ReferenceError",
                        "RangeError",
                        "URIError",
                        "EvalError",
                        "Test262Error",
                    ] {
                        if let Some(error_val) = env.get(name) {
                            if let JsValue::Object(o) = &error_val {
                                if let Some(ctor) = self.get_object(o.id) {
                                    let pv = ctor.borrow().get_property("prototype");
                                    if let JsValue::Object(p) = &pv {
                                        if let Some(ep) = self.get_object(p.id) {
                                            if ep.borrow().prototype.is_none() {
                                                ep.borrow_mut().prototype =
                                                    Some(proto_obj.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Add hasOwnProperty to Object.prototype
                let has_own_fn = self.create_function(JsFunction::native(
                    "hasOwnProperty".to_string(),
                    1,
                    |interp, this_val, args| {
                        let key = args.first().map(to_property_key_string).unwrap_or_default();
                        if let JsValue::Object(o) = this_val
                            && let Some(obj) = interp.get_object(o.id)
                        {
                            return Completion::Normal(JsValue::Boolean(
                                obj.borrow().has_own_property(&key),
                            ));
                        }
                        Completion::Normal(JsValue::Boolean(false))
                    },
                ));
                proto_obj
                    .borrow_mut()
                    .insert_builtin("hasOwnProperty".to_string(), has_own_fn);

                // Object.prototype.toString
                let obj_tostring_fn = self.create_function(JsFunction::native(
                    "toString".to_string(),
                    0,
                    |interp, this_val, _args| {
                        let tag = match this_val {
                            JsValue::Object(o) => {
                                if let Some(obj) = interp.get_object(o.id) {
                                    let cn = obj.borrow().class_name.clone();
                                    if cn == "Object" && obj.borrow().callable.is_some() {
                                        "Function".to_string()
                                    } else {
                                        cn
                                    }
                                } else {
                                    "Object".to_string()
                                }
                            }
                            JsValue::Undefined => "Undefined".to_string(),
                            JsValue::Null => "Null".to_string(),
                            JsValue::Boolean(_) => "Boolean".to_string(),
                            JsValue::Number(_) => "Number".to_string(),
                            JsValue::String(_) => "String".to_string(),
                            JsValue::Symbol(_) => "Symbol".to_string(),
                            JsValue::BigInt(_) => "BigInt".to_string(),
                        };
                        Completion::Normal(JsValue::String(JsString::from_str(&format!(
                            "[object {tag}]"
                        ))))
                    },
                ));
                proto_obj
                    .borrow_mut()
                    .insert_builtin("toString".to_string(), obj_tostring_fn);

                // Object.prototype.valueOf
                let obj_valueof_fn = self.create_function(JsFunction::native(
                    "valueOf".to_string(),
                    0,
                    |interp, this_val, _args| {
                        if let JsValue::Object(o) = this_val
                            && let Some(obj) = interp.get_object(o.id)
                            && let Some(pv) = obj.borrow().primitive_value.clone()
                        {
                            return Completion::Normal(pv);
                        }
                        Completion::Normal(this_val.clone())
                    },
                ));
                proto_obj
                    .borrow_mut()
                    .insert_builtin("valueOf".to_string(), obj_valueof_fn);

                // Object.prototype.toLocaleString
                let obj_tolocalestring_fn = self.create_function(JsFunction::native(
                    "toLocaleString".to_string(),
                    0,
                    |interp, this_val, _args| {
                        // 1. Let O be ? ToObject(this value).
                        let o = match interp.to_object(this_val) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        // 2. Return ? Invoke(O, "toString").
                        if let JsValue::Object(ref obj_ref) = o {
                            let to_string_fn =
                                match interp.get_object_property(obj_ref.id, "toString", &o) {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                };
                            if interp.is_callable(&to_string_fn) {
                                return interp.call_function(&to_string_fn, this_val, &[]);
                            }
                        }
                        Completion::Throw(interp.create_type_error("toString is not a function"))
                    },
                ));
                proto_obj
                    .borrow_mut()
                    .insert_builtin("toLocaleString".to_string(), obj_tolocalestring_fn);

                // Object.prototype.propertyIsEnumerable
                let pie_fn = self.create_function(JsFunction::native(
                    "propertyIsEnumerable".to_string(),
                    1,
                    |interp, this_val, args| {
                        let key = args.first().map(to_property_key_string).unwrap_or_default();
                        if let JsValue::Object(o) = this_val
                            && let Some(obj) = interp.get_object(o.id)
                            && let Some(desc) = obj.borrow().get_own_property(&key)
                        {
                            return Completion::Normal(JsValue::Boolean(
                                desc.enumerable != Some(false),
                            ));
                        }
                        Completion::Normal(JsValue::Boolean(false))
                    },
                ));
                proto_obj
                    .borrow_mut()
                    .insert_builtin("propertyIsEnumerable".to_string(), pie_fn);

                // Object.prototype.isPrototypeOf
                let ipof_fn = self.create_function(JsFunction::native(
                    "isPrototypeOf".to_string(),
                    1,
                    |interp, this_val, args| {
                        let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if let (JsValue::Object(this_o), JsValue::Object(target_o)) =
                            (this_val, &target)
                            && let (Some(this_data), Some(target_data)) =
                                (interp.get_object(this_o.id), interp.get_object(target_o.id))
                        {
                            let mut current = target_data.borrow().prototype.clone();
                            while let Some(p) = current {
                                if Rc::ptr_eq(&p, &this_data) {
                                    return Completion::Normal(JsValue::Boolean(true));
                                }
                                current = p.borrow().prototype.clone();
                            }
                        }
                        Completion::Normal(JsValue::Boolean(false))
                    },
                ));
                proto_obj
                    .borrow_mut()
                    .insert_builtin("isPrototypeOf".to_string(), ipof_fn);

                // Object.prototype.__defineGetter__
                let define_getter_fn = self.create_function(JsFunction::native(
                    "__defineGetter__".to_string(),
                    2,
                    |interp, this_val, args| {
                        let key = args.first().map(to_property_key_string).unwrap_or_default();
                        let getter = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                        if !matches!(&getter, JsValue::Object(o) if interp.get_object(o.id).map(|obj| obj.borrow().callable.is_some()).unwrap_or(false))
                        {
                            return Completion::Throw(
                                interp.create_type_error("Getter must be a function"),
                            );
                        }
                        if let JsValue::Object(o) = this_val
                            && let Some(obj) = interp.get_object(o.id)
                        {
                            obj.borrow_mut().define_own_property(
                                key,
                                PropertyDescriptor {
                                    value: None,
                                    writable: None,
                                    get: Some(getter),
                                    set: None,
                                    enumerable: Some(true),
                                    configurable: Some(true),
                                },
                            );
                        }
                        Completion::Normal(JsValue::Undefined)
                    },
                ));
                proto_obj
                    .borrow_mut()
                    .insert_builtin("__defineGetter__".to_string(), define_getter_fn);

                // Object.prototype.__defineSetter__
                let define_setter_fn = self.create_function(JsFunction::native(
                    "__defineSetter__".to_string(),
                    2,
                    |interp, this_val, args| {
                        let key = args.first().map(to_property_key_string).unwrap_or_default();
                        let setter = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                        if !matches!(&setter, JsValue::Object(o) if interp.get_object(o.id).map(|obj| obj.borrow().callable.is_some()).unwrap_or(false))
                        {
                            return Completion::Throw(
                                interp.create_type_error("Setter must be a function"),
                            );
                        }
                        if let JsValue::Object(o) = this_val
                            && let Some(obj) = interp.get_object(o.id)
                        {
                            obj.borrow_mut().define_own_property(
                                key,
                                PropertyDescriptor {
                                    value: None,
                                    writable: None,
                                    get: None,
                                    set: Some(setter),
                                    enumerable: Some(true),
                                    configurable: Some(true),
                                },
                            );
                        }
                        Completion::Normal(JsValue::Undefined)
                    },
                ));
                proto_obj
                    .borrow_mut()
                    .insert_builtin("__defineSetter__".to_string(), define_setter_fn);

                // Object.prototype.__lookupGetter__
                let lookup_getter_fn = self.create_function(JsFunction::native(
                    "__lookupGetter__".to_string(),
                    1,
                    |interp, this_val, args| {
                        let key = args.first().map(to_property_key_string).unwrap_or_default();
                        let mut current = this_val.clone();
                        loop {
                            if let JsValue::Object(ref o) = current {
                                if let Some(obj) = interp.get_object(o.id) {
                                    if let Some(desc) = obj.borrow().get_own_property(&key) {
                                        if let Some(ref g) = desc.get {
                                            return Completion::Normal(g.clone());
                                        }
                                        return Completion::Normal(JsValue::Undefined);
                                    }
                                    let proto = obj.borrow().prototype.clone();
                                    if let Some(p) = proto {
                                        let pid = p.borrow().id.unwrap();
                                        current =
                                            JsValue::Object(crate::types::JsObject { id: pid });
                                        continue;
                                    }
                                }
                            }
                            return Completion::Normal(JsValue::Undefined);
                        }
                    },
                ));
                proto_obj
                    .borrow_mut()
                    .insert_builtin("__lookupGetter__".to_string(), lookup_getter_fn);

                // Object.prototype.__lookupSetter__
                let lookup_setter_fn = self.create_function(JsFunction::native(
                    "__lookupSetter__".to_string(),
                    1,
                    |interp, this_val, args| {
                        let key = args.first().map(to_property_key_string).unwrap_or_default();
                        let mut current = this_val.clone();
                        loop {
                            if let JsValue::Object(ref o) = current {
                                if let Some(obj) = interp.get_object(o.id) {
                                    if let Some(desc) = obj.borrow().get_own_property(&key) {
                                        if let Some(ref s) = desc.set {
                                            return Completion::Normal(s.clone());
                                        }
                                        return Completion::Normal(JsValue::Undefined);
                                    }
                                    let proto = obj.borrow().prototype.clone();
                                    if let Some(p) = proto {
                                        let pid = p.borrow().id.unwrap();
                                        current =
                                            JsValue::Object(crate::types::JsObject { id: pid });
                                        continue;
                                    }
                                }
                            }
                            return Completion::Normal(JsValue::Undefined);
                        }
                    },
                ));
                proto_obj
                    .borrow_mut()
                    .insert_builtin("__lookupSetter__".to_string(), lookup_setter_fn);

                // Object.prototype.__proto__ accessor (Annex B §B.2.2.1)
                let proto_getter = self.create_function(JsFunction::native(
                    "get __proto__".to_string(),
                    0,
                    |interp, this_val, _args| {
                        let obj_val = match interp.to_object(this_val) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => return Completion::Normal(JsValue::Undefined),
                        };
                        if let JsValue::Object(ref o) = obj_val {
                            if let Some(obj) = interp.get_object(o.id) {
                                return if let Some(ref proto) = obj.borrow().prototype {
                                    let pid = proto.borrow().id.unwrap();
                                    Completion::Normal(JsValue::Object(
                                        crate::types::JsObject { id: pid },
                                    ))
                                } else {
                                    Completion::Normal(JsValue::Null)
                                };
                            }
                        }
                        Completion::Normal(JsValue::Null)
                    },
                ));
                let proto_setter = self.create_function(JsFunction::native(
                    "set __proto__".to_string(),
                    1,
                    |interp, this_val, args| {
                        let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if !matches!(this_val, JsValue::Object(_)) {
                            return Completion::Normal(JsValue::Undefined);
                        }
                        if !matches!(val, JsValue::Object(_) | JsValue::Null) {
                            return Completion::Normal(JsValue::Undefined);
                        }
                        if let JsValue::Object(o) = this_val {
                            if let Some(obj) = interp.get_object(o.id) {
                                if !obj.borrow().extensible {
                                    return Completion::Throw(interp.create_type_error(
                                        "Object is not extensible",
                                    ));
                                }
                                match &val {
                                    JsValue::Null => {
                                        obj.borrow_mut().prototype = None;
                                    }
                                    JsValue::Object(p) => {
                                        if let Some(proto_rc) = interp.get_object(p.id) {
                                            // Cycle check
                                            let mut check = Some(proto_rc.clone());
                                            while let Some(ref c) = check {
                                                if c.borrow().id == obj.borrow().id {
                                                    return Completion::Throw(
                                                        interp.create_type_error(
                                                            "Cyclic __proto__ value",
                                                        ),
                                                    );
                                                }
                                                let next = c.borrow().prototype.clone();
                                                check = next;
                                            }
                                            obj.borrow_mut().prototype = Some(proto_rc);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Completion::Normal(JsValue::Undefined)
                    },
                ));
                proto_obj.borrow_mut().insert_property(
                    "__proto__".to_string(),
                    PropertyDescriptor::accessor(
                        Some(proto_getter),
                        Some(proto_setter),
                        false,
                        true,
                    ),
                );

                self.setup_function_prototype(&proto_obj);
            }

            // Add Object.defineProperty
            let define_property_fn = self.create_function(JsFunction::native(
                "defineProperty".to_string(),
                3,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if !matches!(target, JsValue::Object(_)) {
                        return Completion::Throw(interp.create_type_error(
                            "Object.defineProperty called on non-object",
                        ));
                    }
                    let key_raw = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    let key = if matches!(key_raw, JsValue::Symbol(_)) {
                        to_property_key_string(&key_raw)
                    } else {
                        match interp.to_string_value(&key_raw) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        }
                    };
                    let desc_val = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(ref o) = target
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        // Proxy defineProperty trap
                        if obj.borrow().is_proxy() {
                            let target_inner = interp.get_proxy_target_val(o.id);
                            let key_val = JsValue::String(JsString::from_str(&key));
                            match interp.invoke_proxy_trap(
                                o.id,
                                "defineProperty",
                                vec![target_inner.clone(), key_val, desc_val.clone()],
                            ) {
                                Ok(Some(v)) => {
                                    if !to_boolean(&v) {
                                        return Completion::Throw(interp.create_type_error(
                                            "'defineProperty' on proxy: trap returned falsish",
                                        ));
                                    }
                                    // Invariant checks
                                    if let JsValue::Object(ref t) = target_inner
                                        && let Some(tobj) = interp.get_object(t.id)
                                    {
                                        let target_desc = tobj.borrow().get_own_property(&key);
                                        let target_extensible = tobj.borrow().extensible;
                                        let desc = interp
                                            .to_property_descriptor(&desc_val)
                                            .ok();
                                        if let Some(ref desc) = desc {
                                            if !target_extensible && target_desc.is_none() {
                                                return Completion::Throw(interp.create_type_error(
                                                    "'defineProperty' on proxy: trap returned truish for adding property to the non-extensible proxy target",
                                                ));
                                            }
                                            if let Some(ref td) = target_desc {
                                                if td.configurable == Some(false) {
                                                    if desc.configurable == Some(true) {
                                                        return Completion::Throw(interp.create_type_error(
                                                            "'defineProperty' on proxy: trap returned truish for defining non-configurable property which is already non-configurable in the proxy target as configurable",
                                                        ));
                                                    }
                                                    if desc.is_data_descriptor() && td.is_data_descriptor()
                                                        && td.writable == Some(false)
                                                        && desc.writable == Some(true)
                                                    {
                                                        return Completion::Throw(interp.create_type_error(
                                                            "'defineProperty' on proxy: trap returned truish for defining non-configurable property which cannot be made writable",
                                                        ));
                                                    }
                                                }
                                            }
                                            if desc.configurable == Some(false) && target_desc.is_none() {
                                                return Completion::Throw(interp.create_type_error(
                                                    "'defineProperty' on proxy: trap returned truish for defining non-configurable property which does not exist on the proxy target",
                                                ));
                                            }
                                        }
                                    }
                                    return Completion::Normal(target);
                                }
                                Ok(None) => {
                                    // No trap, fall through to target
                                    if let JsValue::Object(ref t) = target_inner
                                        && let Some(tobj) = interp.get_object(t.id)
                                    {
                                        match interp.to_property_descriptor(&desc_val) {
                                            Ok(desc) => {
                                                if !tobj.borrow_mut().define_own_property(key, desc) {
                                                    return Completion::Throw(interp.create_type_error(
                                                        "Cannot define property, object is not extensible or property is non-configurable",
                                                    ));
                                                }
                                            }
                                            Err(Some(e)) => return Completion::Throw(e),
                                            Err(None) => {}
                                        }
                                    }
                                    return Completion::Normal(target);
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        match interp.to_property_descriptor(&desc_val) {
                            Ok(desc) => {
                                // ArraySetLength: §10.4.2.4
                                if key == "length"
                                    && obj.borrow().class_name == "Array"
                                {
                                    if let Some(ref new_len_val) = desc.value {
                                        let new_num = interp.to_number_coerce(new_len_val);
                                        let new_len = new_num as u32;
                                        if (new_len as f64) != new_num
                                            || new_num < 0.0
                                            || new_num.is_nan()
                                            || new_num.is_infinite()
                                        {
                                            return Completion::Throw(
                                                interp.create_error(
                                                    "RangeError",
                                                    "Invalid array length",
                                                ),
                                            );
                                        }
                                        let old_len = match obj.borrow().get_property("length") {
                                            JsValue::Number(n) => n as u32,
                                            _ => 0,
                                        };
                                        // §10.4.2.4 step 3.f: if old length is non-writable, reject
                                        let old_len_writable = obj.borrow().properties.get("length")
                                            .map(|d| d.writable != Some(false))
                                            .unwrap_or(true);
                                        if !old_len_writable && new_len != old_len {
                                            return Completion::Throw(interp.create_type_error(
                                                "Cannot assign to read only property 'length'",
                                            ));
                                        }
                                        let mut final_len = new_len;
                                        if new_len < old_len {
                                            // §10.4.2.4 step 3.l: delete from old_len-1 downward
                                            let mut b = obj.borrow_mut();
                                            let mut delete_failed = false;
                                            let mut i = old_len;
                                            while i > new_len {
                                                i -= 1;
                                                let k = i.to_string();
                                                // Check if property exists and is non-configurable
                                                let is_non_configurable = b.properties.get(&k)
                                                    .map(|d| d.configurable == Some(false))
                                                    .unwrap_or(false);
                                                if is_non_configurable {
                                                    final_len = i + 1;
                                                    delete_failed = true;
                                                    break;
                                                }
                                                b.properties.remove(&k);
                                                b.property_order.retain(|pk| pk != &k);
                                            }
                                            // Also delete remaining indices between final_len and the failed one
                                            if delete_failed {
                                                // Clean up indices we already passed
                                            } else {
                                                // Delete everything from new_len to where we stopped
                                                for j in new_len..i {
                                                    let k = j.to_string();
                                                    b.properties.remove(&k);
                                                    b.property_order.retain(|pk| pk != &k);
                                                }
                                            }
                                            if let Some(ref mut elems) = b.array_elements {
                                                elems.truncate(final_len as usize);
                                            }
                                            if delete_failed {
                                                // Set length to final_len, then throw
                                                b.properties.insert(
                                                    "length".to_string(),
                                                    PropertyDescriptor::data(JsValue::Number(final_len as f64), true, false, false),
                                                );
                                                drop(b);
                                                return Completion::Throw(interp.create_type_error(
                                                    "Cannot delete array element",
                                                ));
                                            }
                                        }
                                        let len_desc = PropertyDescriptor {
                                            value: Some(JsValue::Number(final_len as f64)),
                                            ..desc
                                        };
                                        if !obj
                                            .borrow_mut()
                                            .define_own_property(key, len_desc)
                                        {
                                            return Completion::Throw(interp.create_type_error(
                                                "Cannot define property, object is not extensible or property is non-configurable",
                                            ));
                                        }
                                        return Completion::Normal(target);
                                    }
                                }
                                let is_array = obj.borrow().class_name == "Array";
                                let key_for_len = if is_array { Some(key.clone()) } else { None };
                                if !obj.borrow_mut().define_own_property(key, desc) {
                                    return Completion::Throw(interp.create_type_error(
                                        "Cannot define property, object is not extensible or property is non-configurable",
                                    ));
                                }
                                // §10.4.2.1 step 3: if array and key is valid index >= length, update length
                                if let Some(ref k) = key_for_len {
                                    if let Ok(idx) = k.parse::<u32>() {
                                        let old_len = match obj.borrow().get_property("length") {
                                            JsValue::Number(n) => n as u32,
                                            _ => 0,
                                        };
                                        if idx >= old_len {
                                            let new_len = idx + 1;
                                            obj.borrow_mut().properties.insert(
                                                "length".to_string(),
                                                PropertyDescriptor::data(JsValue::Number(new_len as f64), true, false, false),
                                            );
                                        }
                                    }
                                }
                            }
                            Err(Some(e)) => return Completion::Throw(e),
                            Err(None) => {}
                        }
                    }
                    Completion::Normal(target)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("defineProperty".to_string(), define_property_fn);

            // Add Object.getOwnPropertyDescriptor
            let get_own_prop_desc_fn = self.create_function(JsFunction::native(
                "getOwnPropertyDescriptor".to_string(),
                2,
                |interp, _this, args| {
                    let target_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let target = match interp.to_object(&target_arg) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => return Completion::Normal(JsValue::Undefined),
                    };
                    let key_raw = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    let key = if matches!(key_raw, JsValue::Symbol(_)) {
                        to_property_key_string(&key_raw)
                    } else {
                        match interp.to_string_value(&key_raw) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        }
                    };
                    if let JsValue::Object(ref o) = target {
                        // Proxy getOwnPropertyDescriptor trap
                        if let Some(obj) = interp.get_object(o.id)
                            && obj.borrow().is_proxy()
                        {
                            let target_inner = interp.get_proxy_target_val(o.id);
                            let key_val = JsValue::String(JsString::from_str(&key));
                            match interp.invoke_proxy_trap(
                                o.id,
                                "getOwnPropertyDescriptor",
                                vec![target_inner.clone(), key_val],
                            ) {
                                Ok(Some(v)) => {
                                    // Invariant checks
                                    if let JsValue::Object(ref t) = target_inner
                                        && let Some(tobj) = interp.get_object(t.id)
                                    {
                                        let target_desc = tobj.borrow().get_own_property(&key);
                                        let target_extensible = tobj.borrow().extensible;
                                        if matches!(v, JsValue::Undefined) {
                                            if let Some(ref td) = target_desc {
                                                if td.configurable == Some(false) {
                                                    return Completion::Throw(interp.create_type_error(
                                                        "'getOwnPropertyDescriptor' on proxy: trap returned undefined for property which is non-configurable in the proxy target",
                                                    ));
                                                }
                                                if !target_extensible {
                                                    return Completion::Throw(interp.create_type_error(
                                                        "'getOwnPropertyDescriptor' on proxy: trap returned undefined for property which exists in the non-extensible proxy target",
                                                    ));
                                                }
                                            }
                                        } else if matches!(v, JsValue::Object(_)) {
                                            if let Some(ref td) = target_desc {
                                                if td.configurable == Some(false) {
                                                    let trap_desc = interp.to_property_descriptor(&v);
                                                    if let Ok(ref trap_d) = trap_desc {
                                                        if trap_d.configurable == Some(true) {
                                                            return Completion::Throw(interp.create_type_error(
                                                                "'getOwnPropertyDescriptor' on proxy: trap returned descriptor with configurable: true for non-configurable property in the proxy target",
                                                            ));
                                                        }
                                                        if td.is_data_descriptor() && td.writable == Some(false)
                                                            && trap_d.writable == Some(true)
                                                        {
                                                            return Completion::Throw(interp.create_type_error(
                                                                "'getOwnPropertyDescriptor' on proxy: trap returned descriptor with writable: true for non-configurable non-writable property in the proxy target",
                                                            ));
                                                        }
                                                    }
                                                }
                                            } else if !target_extensible {
                                                return Completion::Throw(interp.create_type_error(
                                                    "'getOwnPropertyDescriptor' on proxy: trap returned descriptor for property which does not exist in the non-extensible proxy target",
                                                ));
                                            }
                                        }
                                    }
                                    return Completion::Normal(v);
                                }
                                Ok(None) => {
                                    // No trap, fall through to target
                                    if let JsValue::Object(ref t) = target_inner
                                        && let Some(tobj) = interp.get_object(t.id)
                                    {
                                        // Proxy-of-proxy: if target is also a proxy, recurse
                                        if tobj.borrow().is_proxy() {
                                            let inner2 = interp.get_proxy_target_val(t.id);
                                            let key_val2 =
                                                JsValue::String(JsString::from_str(&key));
                                            match interp.invoke_proxy_trap(
                                                t.id,
                                                "getOwnPropertyDescriptor",
                                                vec![inner2.clone(), key_val2],
                                            ) {
                                                Ok(Some(v)) => return Completion::Normal(v),
                                                Ok(None) => {
                                                    if let JsValue::Object(ref t2) = inner2
                                                        && let Some(tobj2) =
                                                            interp.get_object(t2.id)
                                                        && let Some(desc) = tobj2
                                                            .borrow()
                                                            .get_own_property(&key)
                                                    {
                                                        return Completion::Normal(
                                                            interp.from_property_descriptor(&desc),
                                                        );
                                                    }
                                                    return Completion::Normal(JsValue::Undefined);
                                                }
                                                Err(e) => return Completion::Throw(e),
                                            }
                                        }
                                        if let Some(desc) =
                                            tobj.borrow().get_own_property(&key)
                                        {
                                            return Completion::Normal(
                                                interp.from_property_descriptor(&desc),
                                            );
                                        }
                                    }
                                    return Completion::Normal(JsValue::Undefined);
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        if let Some(obj) = interp.get_object(o.id)
                            && let Some(desc) = obj.borrow().get_own_property(&key)
                        {
                            return Completion::Normal(interp.from_property_descriptor(&desc));
                        }
                    }
                    Completion::Normal(JsValue::Undefined)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("getOwnPropertyDescriptor".to_string(), get_own_prop_desc_fn);

            // Add Object.keys
            let keys_fn = self.create_function(JsFunction::native(
                "keys".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let obj_val = match interp.to_object(&target) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if let JsValue::Object(ref o) = obj_val
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        // Proxy ownKeys trap
                        if obj.borrow().is_proxy() {
                            let target_inner = interp.get_proxy_target_val(o.id);
                            match interp.invoke_proxy_trap(
                                o.id,
                                "ownKeys",
                                vec![target_inner.clone()],
                            ) {
                                Ok(Some(v)) => {
                                    if let Err(e) =
                                        interp.validate_ownkeys_invariant(&v, &target_inner)
                                    {
                                        return Completion::Throw(e);
                                    }
                                    return Completion::Normal(v);
                                }
                                Ok(None) => {
                                    // No trap, fall through to target
                                    if let JsValue::Object(ref t) = target_inner
                                        && let Some(tobj) = interp.get_object(t.id)
                                    {
                                        let b = tobj.borrow();
                                        let keys: Vec<JsValue> = b
                                            .property_order
                                            .iter()
                                            .filter(|k| {
                                                b.properties
                                                    .get(*k)
                                                    .is_some_and(|d| d.enumerable != Some(false))
                                            })
                                            .map(|k| JsValue::String(JsString::from_str(k)))
                                            .collect();
                                        let arr = interp.create_array(keys);
                                        return Completion::Normal(arr);
                                    }
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        let borrowed = obj.borrow();
                        let keys: Vec<JsValue> = borrowed
                            .property_order
                            .iter()
                            .filter(|k| {
                                borrowed
                                    .properties
                                    .get(*k)
                                    .is_some_and(|d| d.enumerable != Some(false))
                            })
                            .map(|k| JsValue::String(JsString::from_str(k)))
                            .collect();
                        let arr = interp.create_array(keys);
                        return Completion::Normal(arr);
                    }
                    Completion::Normal(JsValue::Undefined)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("keys".to_string(), keys_fn);

            // Add Object.freeze
            let freeze_fn = self.create_function(JsFunction::native(
                "freeze".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(ref o) = target
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        let mut o = obj.borrow_mut();
                        o.extensible = false;
                        for desc in o.properties.values_mut() {
                            desc.configurable = Some(false);
                            if desc.is_data_descriptor() {
                                desc.writable = Some(false);
                            }
                        }
                    }
                    Completion::Normal(target)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("freeze".to_string(), freeze_fn);

            // Add Object.getPrototypeOf
            let get_proto_fn = self.create_function(JsFunction::native(
                "getPrototypeOf".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(ref o) = target
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        // Proxy getPrototypeOf trap
                        if obj.borrow().is_proxy() {
                            let target_inner = interp.get_proxy_target_val(o.id);
                            match interp.invoke_proxy_trap(
                                o.id,
                                "getPrototypeOf",
                                vec![target_inner.clone()],
                            ) {
                                Ok(Some(v)) => {
                                    // Invariant: if target is non-extensible, must return target's actual prototype
                                    if let JsValue::Object(ref t) = target_inner
                                        && let Some(tobj) = interp.get_object(t.id)
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
                                            return Completion::Throw(interp.create_type_error(
                                                "'getPrototypeOf' on proxy: proxy target is non-extensible but the trap did not return its actual prototype",
                                            ));
                                        }
                                    }
                                    return Completion::Normal(v);
                                }
                                Ok(None) => {
                                    // No trap, fall through to target
                                    if let JsValue::Object(ref t) = target_inner
                                        && let Some(tobj) = interp.get_object(t.id)
                                    {
                                        // Proxy-of-proxy
                                        if tobj.borrow().is_proxy() {
                                            let inner2 = interp.get_proxy_target_val(t.id);
                                            match interp.invoke_proxy_trap(
                                                t.id,
                                                "getPrototypeOf",
                                                vec![inner2.clone()],
                                            ) {
                                                Ok(Some(v)) => return Completion::Normal(v),
                                                Ok(None) => {
                                                    if let JsValue::Object(ref t2) = inner2
                                                        && let Some(tobj2) =
                                                            interp.get_object(t2.id)
                                                        && let Some(proto) =
                                                            &tobj2.borrow().prototype
                                                        && let Some(id) = proto.borrow().id
                                                    {
                                                        return Completion::Normal(
                                                            JsValue::Object(
                                                                crate::types::JsObject { id },
                                                            ),
                                                        );
                                                    }
                                                    return Completion::Normal(JsValue::Null);
                                                }
                                                Err(e) => return Completion::Throw(e),
                                            }
                                        }
                                        if let Some(proto) = &tobj.borrow().prototype
                                            && let Some(id) = proto.borrow().id
                                        {
                                            return Completion::Normal(JsValue::Object(
                                                crate::types::JsObject { id },
                                            ));
                                        }
                                    }
                                    return Completion::Normal(JsValue::Null);
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        if let Some(proto) = &obj.borrow().prototype
                            && let Some(id) = proto.borrow().id
                        {
                            return Completion::Normal(JsValue::Object(crate::types::JsObject {
                                id,
                            }));
                        }
                    }
                    Completion::Normal(JsValue::Null)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("getPrototypeOf".to_string(), get_proto_fn);

            // Add Object.create
            let create_fn = self.create_function(JsFunction::native(
                "create".to_string(),
                2,
                |interp, _this, args| {
                    let proto_arg = args.first().cloned().unwrap_or(JsValue::Null);
                    if !matches!(&proto_arg, JsValue::Object(_) | JsValue::Null) {
                        return Completion::Throw(interp.create_type_error(
                            "Object prototype may only be an Object or null",
                        ));
                    }
                    let new_obj = interp.create_object();
                    match &proto_arg {
                        JsValue::Object(o) => {
                            if let Some(proto_rc) = interp.get_object(o.id) {
                                new_obj.borrow_mut().prototype = Some(proto_rc);
                            }
                        }
                        JsValue::Null => {
                            new_obj.borrow_mut().prototype = None;
                        }
                        _ => unreachable!(),
                    }
                    let id = new_obj.borrow().id.unwrap();
                    let target = JsValue::Object(crate::types::JsObject { id });

                    let props_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    if !matches!(props_arg, JsValue::Undefined) {
                        // ObjectDefineProperties(target, props_arg)
                        let props_obj_val = match interp.to_object(&props_arg) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => return Completion::Normal(target),
                        };
                        if let JsValue::Object(ref d) = props_obj_val
                            && let Some(desc_obj) = interp.get_object(d.id)
                        {
                            let keys: Vec<String> =
                                desc_obj.borrow().property_order.clone();
                            for key in keys {
                                let b_desc = desc_obj.borrow();
                                let is_enum = b_desc
                                    .get_property_descriptor(&key)
                                    .map(|d| d.enumerable.unwrap_or(true))
                                    .unwrap_or(true);
                                drop(b_desc);
                                if !is_enum {
                                    continue;
                                }
                                let prop_desc_val =
                                    match interp.get_object_property(d.id, &key, &props_obj_val) {
                                        Completion::Normal(v) => v,
                                        Completion::Throw(e) => return Completion::Throw(e),
                                        _ => continue,
                                    };
                                match interp.to_property_descriptor(&prop_desc_val) {
                                    Ok(desc) => {
                                        if let Some(target_obj) = interp.get_object(id) {
                                            if !target_obj
                                                .borrow_mut()
                                                .define_own_property(key, desc)
                                            {
                                                return Completion::Throw(
                                                    interp.create_type_error(
                                                        "Cannot define property on non-extensible object",
                                                    ),
                                                );
                                            }
                                        }
                                    }
                                    Err(Some(e)) => return Completion::Throw(e),
                                    Err(None) => {
                                        return Completion::Throw(interp.create_type_error(
                                            "Property description must be an object",
                                        ));
                                    }
                                }
                            }
                        }
                    }

                    Completion::Normal(target)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("create".to_string(), create_fn);

            // Object.entries
            let entries_fn = self.create_function(JsFunction::native(
                "entries".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let obj_val = match interp.to_object(&target) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if let JsValue::Object(o) = &obj_val
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        let keys: Vec<String> = {
                            let borrowed = obj.borrow();
                            borrowed.property_order.iter()
                                .filter(|k| {
                                    if let Some(desc) = borrowed.properties.get(*k) {
                                        desc.enumerable != Some(false)
                                    } else {
                                        false
                                    }
                                })
                                .cloned()
                                .collect()
                        };
                        let mut pairs = Vec::new();
                        for k in keys {
                            let val = match interp.get_object_property(o.id, &k, &obj_val) {
                                Completion::Normal(v) => v,
                                other => return other,
                            };
                            let key = JsValue::String(JsString::from_str(&k));
                            pairs.push(interp.create_array(vec![key, val]));
                        }
                        let arr = interp.create_array(pairs);
                        return Completion::Normal(arr);
                    }
                    Completion::Normal(interp.create_array(Vec::new()))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("entries".to_string(), entries_fn);

            // Object.values
            let values_fn = self.create_function(JsFunction::native(
                "values".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let obj_val = match interp.to_object(&target) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if let JsValue::Object(o) = &obj_val
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        let keys: Vec<String> = {
                            let borrowed = obj.borrow();
                            borrowed.property_order.iter()
                                .filter(|k| {
                                    if let Some(desc) = borrowed.properties.get(*k) {
                                        desc.enumerable != Some(false)
                                    } else {
                                        false
                                    }
                                })
                                .cloned()
                                .collect()
                        };
                        let mut values = Vec::new();
                        for k in keys {
                            let val = match interp.get_object_property(o.id, &k, &obj_val) {
                                Completion::Normal(v) => v,
                                other => return other,
                            };
                            values.push(val);
                        }
                        let arr = interp.create_array(values);
                        return Completion::Normal(arr);
                    }
                    Completion::Normal(interp.create_array(Vec::new()))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("values".to_string(), values_fn);

            // Object.assign
            let assign_fn = self.create_function(JsFunction::native(
                "assign".to_string(),
                2,
                |interp, _this, args| {
                    let target_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let target = match interp.to_object(&target_arg) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => return Completion::Normal(JsValue::Undefined),
                    };
                    let t_id = if let JsValue::Object(ref o) = target {
                        o.id
                    } else {
                        return Completion::Normal(target);
                    };
                    for source in args.iter().skip(1) {
                        if matches!(source, JsValue::Undefined | JsValue::Null) {
                            continue;
                        }
                        let src_obj_val = match interp.to_object(source) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => continue,
                        };
                        let s_id = if let JsValue::Object(ref o) = src_obj_val {
                            o.id
                        } else {
                            continue;
                        };
                        let keys: Vec<String> = if let Some(src) = interp.get_object(s_id) {
                            let b = src.borrow();
                            b.property_order
                                .iter()
                                .chain(
                                    b.properties
                                        .keys()
                                        .filter(|k| k.starts_with("Symbol("))
                                        .filter(|k| !b.property_order.contains(k)),
                                )
                                .filter(|k| {
                                    b.properties
                                        .get(*k)
                                        .map(|d| d.enumerable != Some(false))
                                        .unwrap_or(false)
                                })
                                .cloned()
                                .collect()
                        } else {
                            continue;
                        };
                        for key in keys {
                            let val = match interp.get_object_property(s_id, &key, &src_obj_val) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => JsValue::Undefined,
                            };
                            // [[Set]] on target: check for setters
                            if let Some(tgt) = interp.get_object(t_id) {
                                let desc = tgt.borrow().get_property_descriptor(&key);
                                if let Some(ref d) = desc
                                    && let Some(ref setter) = d.set
                                {
                                    let setter = setter.clone();
                                    match interp.call_function(&setter, &target, &[val]) {
                                        Completion::Throw(e) => return Completion::Throw(e),
                                        _ => {}
                                    }
                                } else {
                                    tgt.borrow_mut().set_property_value(&key, val);
                                }
                            }
                        }
                    }
                    Completion::Normal(target)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("assign".to_string(), assign_fn);

            // Object.groupBy
            let group_by_fn = self.create_function(JsFunction::native(
                "groupBy".to_string(),
                2,
                |interp, _this, args| {
                    let items = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let callback = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    if !matches!(&callback, JsValue::Object(o) if interp.get_object(o.id).map(|obj| obj.borrow().callable.is_some()).unwrap_or(false))
                    {
                        return Completion::Throw(
                            interp.create_type_error("callbackfn is not a function"),
                        );
                    }
                    let iterator = match interp.get_iterator(&items) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };
                    let result_obj = interp.create_object();
                    result_obj.borrow_mut().prototype = None;
                    let result_id = result_obj.borrow().id.unwrap();
                    let result_val = JsValue::Object(crate::types::JsObject { id: result_id });
                    let mut k: u64 = 0;
                    loop {
                        let next = match interp.iterator_step(&iterator) {
                            Ok(Some(v)) => v,
                            Ok(None) => break,
                            Err(e) => return Completion::Throw(e),
                        };
                        let value = interp.iterator_value(&next);
                        let key_val = match interp.call_function(
                            &callback,
                            &JsValue::Undefined,
                            &[value.clone(), JsValue::Number(k as f64)],
                        ) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => JsValue::Undefined,
                        };
                        let key_str = to_property_key_string(&key_val);
                        if let Some(obj) = interp.get_object(result_id) {
                            let existing = obj.borrow().get_property(&key_str);
                            if let JsValue::Object(ref arr_o) = existing
                                && let Some(arr) = interp.get_object(arr_o.id)
                            {
                                let len_val = arr.borrow().get_property("length");
                                let len = to_number(&len_val) as usize;
                                arr.borrow_mut()
                                    .insert_builtin(len.to_string(), value);
                                arr.borrow_mut().insert_builtin(
                                    "length".to_string(),
                                    JsValue::Number((len + 1) as f64),
                                );
                            } else {
                                let new_arr = interp.create_array(vec![value]);
                                obj.borrow_mut().insert_builtin(key_str, new_arr);
                            }
                        }
                        k += 1;
                    }
                    Completion::Normal(result_val)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("groupBy".to_string(), group_by_fn);

            // Object.is
            let is_fn = self.create_function(JsFunction::native(
                "is".to_string(),
                2,
                |_interp, _this, args| {
                    let a = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let b = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    let result = match (&a, &b) {
                        (JsValue::Number(x), JsValue::Number(y)) => number_ops::same_value(*x, *y),
                        _ => strict_equality(&a, &b),
                    };
                    Completion::Normal(JsValue::Boolean(result))
                },
            ));
            obj_func.borrow_mut().insert_builtin("is".to_string(), is_fn);

            // Object.getOwnPropertyNames
            let gopn_fn = self.create_function(JsFunction::native(
                "getOwnPropertyNames".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(o) = &target
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        // Proxy ownKeys trap
                        if obj.borrow().is_proxy() {
                            let target_inner = interp.get_proxy_target_val(o.id);
                            match interp.invoke_proxy_trap(
                                o.id,
                                "ownKeys",
                                vec![target_inner.clone()],
                            ) {
                                Ok(Some(v)) => {
                                    if let Err(e) =
                                        interp.validate_ownkeys_invariant(&v, &target_inner)
                                    {
                                        return Completion::Throw(e);
                                    }
                                    return Completion::Normal(v);
                                }
                                Ok(None) => {
                                    if let JsValue::Object(ref t) = target_inner
                                        && let Some(tobj) = interp.get_object(t.id)
                                    {
                                        let names: Vec<JsValue> = tobj
                                            .borrow()
                                            .property_order
                                            .iter()
                                            .map(|k| JsValue::String(JsString::from_str(k)))
                                            .collect();
                                        let arr = interp.create_array(names);
                                        return Completion::Normal(arr);
                                    }
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        let names: Vec<JsValue> = obj
                            .borrow()
                            .property_order
                            .iter()
                            .map(|k| JsValue::String(JsString::from_str(k)))
                            .collect();
                        let arr = interp.create_array(names);
                        return Completion::Normal(arr);
                    }
                    Completion::Normal(interp.create_array(Vec::new()))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("getOwnPropertyNames".to_string(), gopn_fn);

            // Object.getOwnPropertySymbols
            let gops_fn = self.create_function(JsFunction::native(
                "getOwnPropertySymbols".to_string(),
                1,
                |interp, _this, args| {
                    let target_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let target = match interp.to_object(&target_arg) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => return Completion::Normal(interp.create_array(Vec::new())),
                    };
                    if let JsValue::Object(ref o) = target
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        let symbols: Vec<JsValue> = obj
                            .borrow()
                            .properties
                            .keys()
                            .filter(|k| k.starts_with("Symbol("))
                            .map(|k| JsValue::String(JsString::from_str(k)))
                            .collect();
                        return Completion::Normal(interp.create_array(symbols));
                    }
                    Completion::Normal(interp.create_array(Vec::new()))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("getOwnPropertySymbols".to_string(), gops_fn);

            // Object.preventExtensions
            let pe_fn = self.create_function(JsFunction::native(
                "preventExtensions".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(o) = &target
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        // Proxy preventExtensions trap
                        if obj.borrow().is_proxy() {
                            let target_inner = interp.get_proxy_target_val(o.id);
                            match interp.invoke_proxy_trap(
                                o.id,
                                "preventExtensions",
                                vec![target_inner.clone()],
                            ) {
                                Ok(Some(v)) => {
                                    let trap_result = to_boolean(&v);
                                    if trap_result {
                                        // Can only return true if target is actually non-extensible
                                        if let JsValue::Object(ref t) = target_inner
                                            && let Some(tobj) = interp.get_object(t.id)
                                            && tobj.borrow().extensible
                                        {
                                            return Completion::Throw(interp.create_type_error(
                                                "'preventExtensions' on proxy: trap returned truish but the proxy target is extensible",
                                            ));
                                        }
                                    }
                                    if !trap_result {
                                        return Completion::Throw(interp.create_type_error(
                                            "'preventExtensions' on proxy: trap returned falsish",
                                        ));
                                    }
                                    return Completion::Normal(target);
                                }
                                Ok(None) => {
                                    if let JsValue::Object(ref t) = target_inner
                                        && let Some(tobj) = interp.get_object(t.id)
                                    {
                                        tobj.borrow_mut().extensible = false;
                                    }
                                    return Completion::Normal(target);
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        obj.borrow_mut().extensible = false;
                    }
                    Completion::Normal(target)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("preventExtensions".to_string(), pe_fn);

            // Object.isExtensible
            let ie_fn = self.create_function(JsFunction::native(
                "isExtensible".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(o) = &target
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        // Proxy isExtensible trap
                        if obj.borrow().is_proxy() {
                            let target_inner = interp.get_proxy_target_val(o.id);
                            match interp.invoke_proxy_trap(
                                o.id,
                                "isExtensible",
                                vec![target_inner.clone()],
                            ) {
                                Ok(Some(v)) => {
                                    let trap_result = to_boolean(&v);
                                    // Must match target's actual extensibility
                                    if let JsValue::Object(ref t) = target_inner
                                        && let Some(tobj) = interp.get_object(t.id)
                                    {
                                        let target_extensible = tobj.borrow().extensible;
                                        if trap_result != target_extensible {
                                            return Completion::Throw(interp.create_type_error(
                                                "'isExtensible' on proxy: trap result does not reflect extensibility of proxy target",
                                            ));
                                        }
                                    }
                                    return Completion::Normal(JsValue::Boolean(trap_result));
                                }
                                Ok(None) => {
                                    if let JsValue::Object(ref t) = target_inner
                                        && let Some(tobj) = interp.get_object(t.id)
                                    {
                                        return Completion::Normal(JsValue::Boolean(
                                            tobj.borrow().extensible,
                                        ));
                                    }
                                    return Completion::Normal(JsValue::Boolean(false));
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        return Completion::Normal(JsValue::Boolean(obj.borrow().extensible));
                    }
                    Completion::Normal(JsValue::Boolean(false))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("isExtensible".to_string(), ie_fn);

            // Object.isFrozen
            let frozen_fn = self.create_function(JsFunction::native(
                "isFrozen".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(o) = &target
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        let obj_ref = obj.borrow();
                        if obj_ref.extensible {
                            return Completion::Normal(JsValue::Boolean(false));
                        }
                        let all_frozen = obj_ref.properties.values().all(|d| {
                            d.configurable == Some(false)
                                && (!d.is_data_descriptor() || d.writable == Some(false))
                        });
                        return Completion::Normal(JsValue::Boolean(all_frozen));
                    }
                    Completion::Normal(JsValue::Boolean(true))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("isFrozen".to_string(), frozen_fn);

            // Object.isSealed
            let sealed_fn = self.create_function(JsFunction::native(
                "isSealed".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(o) = &target
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        let obj_ref = obj.borrow();
                        if obj_ref.extensible {
                            return Completion::Normal(JsValue::Boolean(false));
                        }
                        let all_sealed = obj_ref
                            .properties
                            .values()
                            .all(|d| d.configurable == Some(false));
                        return Completion::Normal(JsValue::Boolean(all_sealed));
                    }
                    Completion::Normal(JsValue::Boolean(true))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("isSealed".to_string(), sealed_fn);

            // Object.seal
            let seal_fn = self.create_function(JsFunction::native(
                "seal".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(o) = &target
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        let mut obj_mut = obj.borrow_mut();
                        obj_mut.extensible = false;
                        for desc in obj_mut.properties.values_mut() {
                            desc.configurable = Some(false);
                        }
                    }
                    Completion::Normal(target)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("seal".to_string(), seal_fn);

            // Object.hasOwn
            let has_own_fn = self.create_function(JsFunction::native(
                "hasOwn".to_string(),
                2,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let key = args.get(1).map(to_property_key_string).unwrap_or_default();
                    if let JsValue::Object(o) = &target
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        return Completion::Normal(JsValue::Boolean(
                            obj.borrow().has_own_property(&key),
                        ));
                    }
                    Completion::Normal(JsValue::Boolean(false))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("hasOwn".to_string(), has_own_fn);

            // Object.setPrototypeOf
            let set_proto_fn = self.create_function(JsFunction::native(
                "setPrototypeOf".to_string(),
                2,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let proto = args.get(1).cloned().unwrap_or(JsValue::Null);
                    if let JsValue::Object(ref o) = target
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        // Proxy setPrototypeOf trap
                        if obj.borrow().is_proxy() {
                            let target_inner = interp.get_proxy_target_val(o.id);
                            match interp.invoke_proxy_trap(
                                o.id,
                                "setPrototypeOf",
                                vec![target_inner.clone(), proto.clone()],
                            ) {
                                Ok(Some(v)) => {
                                    if !to_boolean(&v) {
                                        return Completion::Throw(interp.create_type_error(
                                            "'setPrototypeOf' on proxy: trap returned falsish",
                                        ));
                                    }
                                    // Invariant: if target is non-extensible, proto must match target's prototype
                                    if let JsValue::Object(ref t) = target_inner
                                        && let Some(tobj) = interp.get_object(t.id)
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
                                        let same = match (&proto, &actual_proto) {
                                            (JsValue::Object(a), JsValue::Object(b)) => a.id == b.id,
                                            (JsValue::Null, JsValue::Null) => true,
                                            _ => false,
                                        };
                                        if !same {
                                            return Completion::Throw(interp.create_type_error(
                                                "'setPrototypeOf' on proxy: trap returned truish for setting a new prototype on the non-extensible proxy target",
                                            ));
                                        }
                                    }
                                    return Completion::Normal(target);
                                }
                                Ok(None) => {
                                    // No trap, fall through to target
                                    if let JsValue::Object(ref t) = target_inner
                                        && let Some(tobj) = interp.get_object(t.id)
                                    {
                                        // Proxy-of-proxy
                                        if tobj.borrow().is_proxy() {
                                            let inner2 = interp.get_proxy_target_val(t.id);
                                            match interp.invoke_proxy_trap(
                                                t.id,
                                                "setPrototypeOf",
                                                vec![inner2, proto.clone()],
                                            ) {
                                                Ok(Some(v)) => {
                                                    if !to_boolean(&v) {
                                                        return Completion::Throw(
                                                            interp.create_type_error(
                                                                "'setPrototypeOf' on proxy: trap returned falsish",
                                                            ),
                                                        );
                                                    }
                                                    return Completion::Normal(target);
                                                }
                                                Ok(None) => {}
                                                Err(e) => return Completion::Throw(e),
                                            }
                                        }
                                        match &proto {
                                            JsValue::Null => {
                                                tobj.borrow_mut().prototype = None;
                                            }
                                            JsValue::Object(p) => {
                                                if let Some(po) = interp.get_object(p.id) {
                                                    tobj.borrow_mut().prototype = Some(po);
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    return Completion::Normal(target);
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        match &proto {
                            JsValue::Null => {
                                obj.borrow_mut().prototype = None;
                            }
                            JsValue::Object(p) => {
                                if let Some(proto_obj) = interp.get_object(p.id) {
                                    obj.borrow_mut().prototype = Some(proto_obj);
                                }
                            }
                            _ => {}
                        }
                    }
                    Completion::Normal(target)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("setPrototypeOf".to_string(), set_proto_fn);

            // Object.defineProperties
            let def_props_fn = self.create_function(JsFunction::native(
                "defineProperties".to_string(),
                2,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if !matches!(target, JsValue::Object(_)) {
                        return Completion::Throw(interp.create_type_error(
                            "Object.defineProperties called on non-object",
                        ));
                    }
                    let descs = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(ref t) = target
                        && let JsValue::Object(ref d) = descs
                        && let Some(desc_obj) = interp.get_object(d.id)
                    {
                        // Collect enumerable own property keys in insertion order
                        let keys: Vec<String> = {
                            let b = desc_obj.borrow();
                            b.property_order
                                .iter()
                                .filter(|k| {
                                    b.properties
                                        .get(*k)
                                        .map(|p| p.enumerable != Some(false))
                                        .unwrap_or(false)
                                })
                                .cloned()
                                .collect()
                        };
                        // Collect all descriptors first
                        let mut descriptors: Vec<(String, PropertyDescriptor)> = Vec::new();
                        for key in keys {
                            let prop_desc_val =
                                match interp.get_object_property(d.id, &key, &descs) {
                                    Completion::Normal(v) => v,
                                    Completion::Throw(e) => return Completion::Throw(e),
                                    _ => continue,
                                };
                            match interp.to_property_descriptor(&prop_desc_val) {
                                Ok(desc) => descriptors.push((key, desc)),
                                Err(Some(e)) => return Completion::Throw(e),
                                Err(None) => {}
                            }
                        }
                        // Apply all descriptors
                        for (key, desc) in descriptors {
                            if let Some(target_obj) = interp.get_object(t.id)
                                && !target_obj.borrow_mut().define_own_property(key, desc) {
                                    return Completion::Throw(interp.create_type_error(
                                        "Cannot define property, object is not extensible or property is non-configurable",
                                    ));
                                }
                        }
                    }
                    Completion::Normal(target)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("defineProperties".to_string(), def_props_fn);

            // Object.getOwnPropertyDescriptors
            let get_descs_fn = self.create_function(JsFunction::native(
                "getOwnPropertyDescriptors".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    // §22.1.2.8 step 1: RequireObjectCoercible then ToObject
                    if matches!(target, JsValue::Undefined | JsValue::Null) {
                        return Completion::Throw(interp.create_type_error(
                            "Cannot convert undefined or null to object",
                        ));
                    }
                    let obj_val = match interp.to_object(&target) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if let JsValue::Object(ref t) = obj_val
                        && let Some(obj) = interp.get_object(t.id)
                    {
                        let result = interp.create_object();
                        // Collect all own keys including String exotic indices
                        let mut keys: Vec<String> = Vec::new();
                        let b = obj.borrow();
                        if let Some(JsValue::String(ref s)) = b.primitive_value {
                            if b.class_name == "String" {
                                for i in 0..s.code_units.len() {
                                    keys.push(i.to_string());
                                }
                                keys.push("length".to_string());
                            }
                        }
                        for k in &b.property_order {
                            if !keys.contains(k) {
                                keys.push(k.clone());
                            }
                        }
                        drop(b);
                        for key in keys {
                            if let Some(d) = obj.borrow().get_own_property(&key) {
                                let desc_val = interp.from_property_descriptor(&d);
                                result.borrow_mut().insert_value(key, desc_val);
                            }
                        }
                        let id = result.borrow().id.unwrap();
                        return Completion::Normal(JsValue::Object(crate::types::JsObject { id }));
                    }
                    // Primitive wrapped to object with no own properties
                    let result = interp.create_object();
                    let id = result.borrow().id.unwrap();
                    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("getOwnPropertyDescriptors".to_string(), get_descs_fn);

            // Object.fromEntries
            let from_entries_fn = self.create_function(JsFunction::native(
                "fromEntries".to_string(),
                1,
                |interp, _this, args| {
                    let iterable = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let obj = interp.create_object();
                    if let JsValue::Object(ref arr) = iterable
                        && let Some(arr_obj) = interp.get_object(arr.id)
                    {
                        let len = if let Some(JsValue::Number(n)) =
                            arr_obj.borrow().get_property_value("length")
                        {
                            n as usize
                        } else {
                            0
                        };
                        for i in 0..len {
                            let entry = arr_obj.borrow().get_property(&i.to_string());
                            if let JsValue::Object(ref e) = entry
                                && let Some(e_obj) = interp.get_object(e.id)
                            {
                                let k = to_js_string(&e_obj.borrow().get_property("0"));
                                let v = e_obj.borrow().get_property("1");
                                obj.borrow_mut().insert_builtin(k, v);
                            }
                        }
                    }
                    let id = obj.borrow().id.unwrap();
                    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_builtin("fromEntries".to_string(), from_entries_fn);
        }
    }

    pub(crate) fn get_symbol_iterator_key(&self) -> Option<String> {
        self.global_env.borrow().get("Symbol").and_then(|sv| {
            if let JsValue::Object(so) = sv {
                self.get_object(so.id).map(|sobj| {
                    let val = sobj.borrow().get_property("iterator");
                    to_js_string(&val)
                })
            } else {
                None
            }
        })
    }

    pub(crate) fn create_iter_result_object(&mut self, value: JsValue, done: bool) -> JsValue {
        let obj = self.create_object();
        obj.borrow_mut().insert_value("value".to_string(), value);
        obj.borrow_mut()
            .insert_value("done".to_string(), JsValue::Boolean(done));
        let id = obj.borrow().id.unwrap();
        JsValue::Object(crate::types::JsObject { id })
    }

    pub(crate) fn get_iterator(&mut self, obj: &JsValue) -> Result<JsValue, JsValue> {
        let sym_key = self.get_symbol_iterator_key();
        let iter_fn = match obj {
            JsValue::Object(o) => {
                if let Some(key) = &sym_key {
                    if let Some(obj_data) = self.get_object(o.id) {
                        let val = obj_data.borrow().get_property(key);
                        if matches!(val, JsValue::Undefined) {
                            return Err(self.create_type_error("is not iterable"));
                        }
                        val
                    } else {
                        return Err(self.create_type_error("is not iterable"));
                    }
                } else {
                    return Err(self.create_type_error("is not iterable"));
                }
            }
            JsValue::String(_) => {
                if let Some(key) = &sym_key {
                    let str_proto = self.string_prototype.clone();
                    if let Some(proto) = str_proto {
                        let val = proto.borrow().get_property(key);
                        if !matches!(val, JsValue::Undefined) {
                            val
                        } else {
                            return Err(self.create_type_error("is not iterable"));
                        }
                    } else {
                        return Err(self.create_type_error("is not iterable"));
                    }
                } else {
                    return Err(self.create_type_error("is not iterable"));
                }
            }
            _ => return Err(self.create_type_error("is not iterable")),
        };
        match self.call_function(&iter_fn, obj, &[]) {
            Completion::Normal(v) => {
                if matches!(v, JsValue::Object(_)) {
                    Ok(v)
                } else {
                    Err(self
                        .create_type_error("Result of the Symbol.iterator method is not an object"))
                }
            }
            Completion::Throw(e) => Err(e),
            _ => Err(self.create_type_error("is not iterable")),
        }
    }

    pub(crate) fn get_async_iterator(&mut self, obj: &JsValue) -> Result<JsValue, JsValue> {
        let async_sym_key = self.get_symbol_key("asyncIterator");
        if let Some(key) = &async_sym_key {
            let iter_fn = match obj {
                JsValue::Object(o) => {
                    if let Some(obj_data) = self.get_object(o.id) {
                        let val = obj_data.borrow().get_property(key);
                        if !matches!(val, JsValue::Undefined) {
                            Some(val)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(iter_fn) = iter_fn {
                return match self.call_function(&iter_fn, obj, &[]) {
                    Completion::Normal(v) => {
                        if matches!(v, JsValue::Object(_)) {
                            Ok(v)
                        } else {
                            Err(self.create_type_error(
                                "Result of the Symbol.asyncIterator method is not an object",
                            ))
                        }
                    }
                    Completion::Throw(e) => Err(e),
                    _ => Err(self.create_type_error("is not async iterable")),
                };
            }
        }
        // Fallback: wrap sync iterator
        let sync_iter = self.get_iterator(obj)?;
        Ok(self.create_async_from_sync_iterator(sync_iter))
    }

    fn create_async_from_sync_iterator(&mut self, sync_iter: JsValue) -> JsValue {
        let wrapper = self.create_object();
        let sync_for_next = sync_iter.clone();
        let next_fn = self.create_function(JsFunction::native(
            "next".to_string(),
            1,
            move |interp, _this, _args| {
                let result = match interp.iterator_next(&sync_for_next) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                Completion::Normal(interp.promise_resolve_value(&result))
            },
        ));
        wrapper
            .borrow_mut()
            .insert_builtin("next".to_string(), next_fn);

        let sync_for_return = sync_iter.clone();
        let return_fn = self.create_function(JsFunction::native(
            "return".to_string(),
            1,
            move |interp, _this, args| {
                if let JsValue::Object(io) = &sync_for_return {
                    let ret_fn = interp.get_object(io.id).and_then(|obj| {
                        let val = obj.borrow().get_property("return");
                        if matches!(val, JsValue::Object(_)) {
                            Some(val)
                        } else {
                            None
                        }
                    });
                    if let Some(ret_fn) = ret_fn {
                        match interp.call_function(&ret_fn, &sync_for_return, args) {
                            Completion::Normal(v) => {
                                Completion::Normal(interp.promise_resolve_value(&v))
                            }
                            Completion::Throw(e) => Completion::Throw(e),
                            _ => {
                                let result =
                                    interp.create_iter_result_object(JsValue::Undefined, true);
                                Completion::Normal(interp.promise_resolve_value(&result))
                            }
                        }
                    } else {
                        let result = interp.create_iter_result_object(
                            args.first().cloned().unwrap_or(JsValue::Undefined),
                            true,
                        );
                        Completion::Normal(interp.promise_resolve_value(&result))
                    }
                } else {
                    let result = interp.create_iter_result_object(JsValue::Undefined, true);
                    Completion::Normal(interp.promise_resolve_value(&result))
                }
            },
        ));
        wrapper
            .borrow_mut()
            .insert_builtin("return".to_string(), return_fn);

        let sync_for_throw = sync_iter;
        let throw_fn = self.create_function(JsFunction::native(
            "throw".to_string(),
            1,
            move |interp, _this, args| {
                if let JsValue::Object(io) = &sync_for_throw {
                    let throw_method = interp.get_object(io.id).and_then(|obj| {
                        let val = obj.borrow().get_property("throw");
                        if matches!(val, JsValue::Object(_)) {
                            Some(val)
                        } else {
                            None
                        }
                    });
                    if let Some(throw_method) = throw_method {
                        match interp.call_function(&throw_method, &sync_for_throw, args) {
                            Completion::Normal(v) => {
                                Completion::Normal(interp.promise_resolve_value(&v))
                            }
                            Completion::Throw(e) => Completion::Throw(e),
                            _ => {
                                Completion::Throw(interp.create_type_error("Iterator throw failed"))
                            }
                        }
                    } else {
                        Completion::Throw(args.first().cloned().unwrap_or(JsValue::Undefined))
                    }
                } else {
                    Completion::Throw(args.first().cloned().unwrap_or(JsValue::Undefined))
                }
            },
        ));
        wrapper
            .borrow_mut()
            .insert_builtin("throw".to_string(), throw_fn);

        let id = wrapper.borrow().id.unwrap();
        JsValue::Object(crate::types::JsObject { id })
    }

    pub(crate) fn iterator_next(&mut self, iterator: &JsValue) -> Result<JsValue, JsValue> {
        if let JsValue::Object(io) = iterator {
            let next_fn = self.get_object(io.id).and_then(|obj| {
                obj.borrow()
                    .get_property_descriptor("next")
                    .and_then(|d| d.value)
            });
            if let Some(next_fn) = next_fn {
                match self.call_function(&next_fn, iterator, &[]) {
                    Completion::Normal(v) => {
                        if matches!(v, JsValue::Object(_)) {
                            Ok(v)
                        } else {
                            Err(self.create_type_error("Iterator result is not an object"))
                        }
                    }
                    Completion::Throw(e) => Err(e),
                    _ => Err(self.create_type_error("Iterator next failed")),
                }
            } else {
                Err(self.create_type_error("Iterator does not have a next method"))
            }
        } else {
            Err(self.create_type_error("Iterator is not an object"))
        }
    }

    pub(crate) fn iterator_next_with_value(
        &mut self,
        iterator: &JsValue,
        value: &JsValue,
    ) -> Result<JsValue, JsValue> {
        if let JsValue::Object(io) = iterator {
            let next_fn = self.get_object(io.id).and_then(|obj| {
                obj.borrow()
                    .get_property_descriptor("next")
                    .and_then(|d| d.value)
            });
            if let Some(next_fn) = next_fn {
                match self.call_function(&next_fn, iterator, &[value.clone()]) {
                    Completion::Normal(v) => {
                        if matches!(v, JsValue::Object(_)) {
                            Ok(v)
                        } else {
                            Err(self.create_type_error("Iterator result is not an object"))
                        }
                    }
                    Completion::Throw(e) => Err(e),
                    _ => Err(self.create_type_error("Iterator next failed")),
                }
            } else {
                Err(self.create_type_error("Iterator does not have a next method"))
            }
        } else {
            Err(self.create_type_error("Iterator is not an object"))
        }
    }

    pub(crate) fn iterator_complete(&self, result: &JsValue) -> bool {
        if let JsValue::Object(o) = result
            && let Some(obj) = self.get_object(o.id)
        {
            let done = obj.borrow().get_property("done");
            return to_boolean(&done);
        }
        true
    }

    pub(crate) fn iterator_value(&self, result: &JsValue) -> JsValue {
        if let JsValue::Object(o) = result
            && let Some(obj) = self.get_object(o.id)
        {
            return obj.borrow().get_property("value");
        }
        JsValue::Undefined
    }

    pub(crate) fn iterator_step(&mut self, iterator: &JsValue) -> Result<Option<JsValue>, JsValue> {
        let result = self.iterator_next(iterator)?;
        if self.iterator_complete(&result) {
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }

    pub(crate) fn iterator_return(
        &mut self,
        iterator: &JsValue,
        value: &JsValue,
    ) -> Result<Option<JsValue>, JsValue> {
        if let JsValue::Object(io) = iterator {
            let return_fn = self.get_object(io.id).and_then(|obj| {
                let val = obj.borrow().get_property("return");
                if matches!(val, JsValue::Object(_)) {
                    Some(val)
                } else if matches!(val, JsValue::Undefined) {
                    None
                } else {
                    None
                }
            });
            if let Some(return_fn) = return_fn {
                match self.call_function(&return_fn, iterator, &[value.clone()]) {
                    Completion::Normal(v) => {
                        if matches!(v, JsValue::Object(_)) {
                            Ok(Some(v))
                        } else {
                            Err(self.create_type_error("Iterator return result is not an object"))
                        }
                    }
                    Completion::Throw(e) => Err(e),
                    _ => Err(self.create_type_error("Iterator return failed")),
                }
            } else {
                Ok(None)
            }
        } else {
            Err(self.create_type_error("Iterator is not an object"))
        }
    }

    pub(crate) fn iterator_throw(
        &mut self,
        iterator: &JsValue,
        exception: &JsValue,
    ) -> Result<Option<JsValue>, JsValue> {
        if let JsValue::Object(io) = iterator {
            let throw_fn = self.get_object(io.id).and_then(|obj| {
                let val = obj.borrow().get_property("throw");
                if matches!(val, JsValue::Object(_)) {
                    Some(val)
                } else if matches!(val, JsValue::Undefined) {
                    None
                } else {
                    None
                }
            });
            if let Some(throw_fn) = throw_fn {
                match self.call_function(&throw_fn, iterator, &[exception.clone()]) {
                    Completion::Normal(v) => {
                        if matches!(v, JsValue::Object(_)) {
                            Ok(Some(v))
                        } else {
                            Err(self.create_type_error("Iterator throw result is not an object"))
                        }
                    }
                    Completion::Throw(e) => Err(e),
                    _ => Err(self.create_type_error("Iterator throw failed")),
                }
            } else {
                Ok(None)
            }
        } else {
            Err(self.create_type_error("Iterator is not an object"))
        }
    }

    pub(crate) fn iterator_close(&mut self, iterator: &JsValue, _completion: JsValue) -> JsValue {
        if let JsValue::Object(io) = iterator {
            let return_fn = self.get_object(io.id).and_then(|obj| {
                let val = obj.borrow().get_property("return");
                if matches!(val, JsValue::Object(_)) {
                    Some(val)
                } else {
                    None
                }
            });
            if let Some(return_fn) = return_fn {
                let _ = self.call_function(&return_fn, iterator, &[]);
            }
        }
        _completion
    }

    pub(crate) fn iterator_close_result(&mut self, iterator: &JsValue) -> Result<(), JsValue> {
        if let JsValue::Object(io) = iterator {
            let return_fn = self.get_object(io.id).and_then(|obj| {
                let val = obj.borrow().get_property("return");
                if matches!(val, JsValue::Object(_)) {
                    Some(val)
                } else {
                    None
                }
            });
            if let Some(return_fn) = return_fn {
                match self.call_function(&return_fn, iterator, &[]) {
                    Completion::Normal(inner_result) => {
                        if !matches!(inner_result, JsValue::Object(_)) {
                            return Err(self.create_type_error("Iterator result is not an object"));
                        }
                    }
                    Completion::Throw(e) => return Err(e),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn get_iterator_direct(&mut self, obj: &JsValue) -> Result<(JsValue, JsValue), JsValue> {
        match obj {
            JsValue::Object(o) => {
                let next_method = self
                    .get_object(o.id)
                    .map(|od| od.borrow().get_property("next"))
                    .unwrap_or(JsValue::Undefined);
                if let JsValue::Object(no) = &next_method {
                    if self
                        .get_object(no.id)
                        .map(|od| od.borrow().callable.is_some())
                        .unwrap_or(false)
                    {
                        Ok((obj.clone(), next_method))
                    } else {
                        Err(self.create_type_error("Iterator next is not a function"))
                    }
                } else {
                    Err(self.create_type_error("Iterator next is not a function"))
                }
            }
            _ => Err(self.create_type_error("Iterator is not an object")),
        }
    }

    fn iterator_step_direct(
        &mut self,
        iterator: &JsValue,
        next_method: &JsValue,
    ) -> Result<Option<JsValue>, JsValue> {
        match self.call_function(next_method, iterator, &[]) {
            Completion::Normal(result) => {
                if !matches!(result, JsValue::Object(_)) {
                    return Err(self.create_type_error("Iterator result is not an object"));
                }
                if self.iterator_complete(&result) {
                    Ok(None)
                } else {
                    Ok(Some(result))
                }
            }
            Completion::Throw(e) => Err(e),
            _ => Err(self.create_type_error("Iterator next failed")),
        }
    }

    pub(crate) fn iterate_to_vec(&mut self, iterable: &JsValue) -> Result<Vec<JsValue>, JsValue> {
        let iterator = self.get_iterator(iterable)?;
        let mut values = Vec::new();
        loop {
            match self.iterator_step(&iterator)? {
                Some(result) => values.push(self.iterator_value(&result)),
                None => break,
            }
        }
        Ok(values)
    }

    fn setup_reflect(&mut self) {
        let reflect_obj = self.create_object();
        let reflect_id = reflect_obj.borrow().id.unwrap();

        // Reflect.apply(target, thisArg, argsList)
        let apply_fn = self.create_function(JsFunction::native(
            "apply".to_string(),
            3,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(target, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.apply requires a function target"),
                    );
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let args_list = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                let call_args = if matches!(args_list, JsValue::Object(_)) {
                    match interp.iterate_to_vec(&args_list) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    }
                } else {
                    Vec::new()
                };
                interp.call_function(&target, &this_arg, &call_args)
            },
        ));
        reflect_obj
            .borrow_mut()
            .insert_builtin("apply".to_string(), apply_fn);

        // Reflect.construct(target, argsList, newTarget?)
        let construct_fn = self.create_function(JsFunction::native(
            "construct".to_string(),
            2,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(target, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.construct requires a constructor"),
                    );
                }
                // Check target is callable
                if let JsValue::Object(ref to) = target
                    && let Some(tobj) = interp.get_object(to.id)
                    && tobj.borrow().callable.is_none()
                {
                    return Completion::Throw(
                        interp.create_type_error("target is not a constructor"),
                    );
                }
                let args_list = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let new_target = args.get(2).cloned().unwrap_or(target.clone());
                // Check newTarget is a constructor (has [[Construct]])
                if let JsValue::Object(ref nto) = new_target {
                    if let Some(ntobj) = interp.get_object(nto.id) {
                        let b = ntobj.borrow();
                        let is_ctor = match &b.callable {
                            Some(JsFunction::User { is_arrow, .. }) => !is_arrow,
                            Some(JsFunction::Native(_, _, _, is_ctor)) => *is_ctor,
                            None => false,
                        };
                        if !is_ctor {
                            return Completion::Throw(
                                interp.create_type_error("newTarget is not a constructor"),
                            );
                        }
                    }
                } else {
                    return Completion::Throw(
                        interp.create_type_error("newTarget is not a constructor"),
                    );
                }
                let call_args = if matches!(args_list, JsValue::Object(_)) {
                    match interp.iterate_to_vec(&args_list) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    }
                } else {
                    Vec::new()
                };
                // Create new object
                let new_obj = interp.create_object();
                // Set prototype from newTarget.prototype
                if let JsValue::Object(nt) = &new_target
                    && let Some(nt_obj) = interp.get_object(nt.id)
                {
                    let proto = nt_obj.borrow().get_property("prototype");
                    if let JsValue::Object(proto_obj) = &proto
                        && let Some(proto_rc) = interp.get_object(proto_obj.id)
                    {
                        new_obj.borrow_mut().prototype = Some(proto_rc);
                    }
                }
                let new_obj_id = new_obj.borrow().id.unwrap();
                let this_val = JsValue::Object(crate::types::JsObject { id: new_obj_id });
                let prev_new_target = interp.new_target.take();
                interp.new_target = Some(new_target);
                let result = interp.call_function(&target, &this_val, &call_args);
                interp.new_target = prev_new_target;
                match result {
                    Completion::Normal(v) if matches!(v, JsValue::Object(_)) => {
                        Completion::Normal(v)
                    }
                    Completion::Normal(_) => Completion::Normal(this_val),
                    other => other,
                }
            },
        ));
        reflect_obj
            .borrow_mut()
            .insert_builtin("construct".to_string(), construct_fn);

        // Reflect.defineProperty(target, key, desc)
        let def_prop_fn = self.create_function(JsFunction::native(
            "defineProperty".to_string(),
            3,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(target, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.defineProperty requires an object"),
                    );
                }
                let key_raw = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let key = if matches!(key_raw, JsValue::Symbol(_)) {
                    to_property_key_string(&key_raw)
                } else {
                    match interp.to_string_value(&key_raw) {
                        Ok(s) => s,
                        Err(e) => return Completion::Throw(e),
                    }
                };
                let desc_val = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(ref o) = target
                    && let Some(obj) = interp.get_object(o.id)
                {
                    if obj.borrow().is_proxy() {
                        let target_inner = interp.get_proxy_target_val(o.id);
                        let key_val = JsValue::String(JsString::from_str(&key));
                        match interp.invoke_proxy_trap(
                            o.id,
                            "defineProperty",
                            vec![target_inner.clone(), key_val, desc_val.clone()],
                        ) {
                            Ok(Some(v)) => {
                                return Completion::Normal(JsValue::Boolean(to_boolean(&v)));
                            }
                            Ok(None) => {
                                if let JsValue::Object(ref t) = target_inner
                                    && let Some(tobj) = interp.get_object(t.id)
                                {
                                    // Proxy-of-proxy: if target is also a proxy, recurse
                                    if tobj.borrow().is_proxy() {
                                        let inner2 = interp.get_proxy_target_val(t.id);
                                        let key_val2 =
                                            JsValue::String(JsString::from_str(&key));
                                        match interp.invoke_proxy_trap(
                                            t.id,
                                            "defineProperty",
                                            vec![inner2, key_val2, desc_val.clone()],
                                        ) {
                                            Ok(Some(v)) => {
                                                return Completion::Normal(JsValue::Boolean(
                                                    to_boolean(&v),
                                                ));
                                            }
                                            Ok(None) => {}
                                            Err(e) => return Completion::Throw(e),
                                        }
                                    }
                                    match interp.to_property_descriptor(&desc_val) {
                                        Ok(desc) => {
                                            let result =
                                                tobj.borrow_mut().define_own_property(key, desc);
                                            return Completion::Normal(JsValue::Boolean(result));
                                        }
                                        Err(Some(e)) => return Completion::Throw(e),
                                        Err(None) => {}
                                    }
                                }
                                return Completion::Normal(JsValue::Boolean(false));
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    match interp.to_property_descriptor(&desc_val) {
                        Ok(desc) => {
                            let result = obj.borrow_mut().define_own_property(key, desc);
                            return Completion::Normal(JsValue::Boolean(result));
                        }
                        Err(Some(e)) => return Completion::Throw(e),
                        Err(None) => {}
                    }
                }
                Completion::Normal(JsValue::Boolean(false))
            },
        ));
        reflect_obj
            .borrow_mut()
            .insert_builtin("defineProperty".to_string(), def_prop_fn);

        // Reflect.deleteProperty(target, key)
        let del_prop_fn = self.create_function(JsFunction::native(
            "deleteProperty".to_string(),
            2,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(target, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.deleteProperty requires an object"),
                    );
                }
                let key = args.get(1).map(to_property_key_string).unwrap_or_default();
                if let JsValue::Object(ref o) = target
                    && let Some(obj) = interp.get_object(o.id)
                {
                    if obj.borrow().is_proxy() {
                        let target_inner = interp.get_proxy_target_val(o.id);
                        let key_val = JsValue::String(JsString::from_str(&key));
                        match interp.invoke_proxy_trap(o.id, "deleteProperty", vec![target_inner.clone(), key_val]) {
                            Ok(Some(v)) => {
                                let trap_result = to_boolean(&v);
                                if trap_result {
                                    if let JsValue::Object(ref t) = target_inner
                                        && let Some(tobj) = interp.get_object(t.id)
                                    {
                                        let target_desc = tobj.borrow().get_own_property(&key);
                                        if let Some(ref desc) = target_desc {
                                            if desc.configurable == Some(false) {
                                                return Completion::Throw(interp.create_type_error(
                                                    "'deleteProperty' on proxy: trap returned truish for property which is non-configurable in the proxy target",
                                                ));
                                            }
                                            if !tobj.borrow().extensible {
                                                return Completion::Throw(interp.create_type_error(
                                                    "'deleteProperty' on proxy: trap returned truish for property but the proxy target is not extensible",
                                                ));
                                            }
                                        }
                                    }
                                }
                                return Completion::Normal(JsValue::Boolean(trap_result));
                            }
                            Ok(None) => {
                                if let JsValue::Object(ref t) = target_inner
                                    && let Some(tobj) = interp.get_object(t.id)
                                {
                                    // Proxy-of-proxy: if target is also a proxy, recurse
                                    if tobj.borrow().is_proxy() {
                                        let inner2 = interp.get_proxy_target_val(t.id);
                                        let key_val2 =
                                            JsValue::String(JsString::from_str(&key));
                                        match interp.invoke_proxy_trap(
                                            t.id,
                                            "deleteProperty",
                                            vec![inner2, key_val2],
                                        ) {
                                            Ok(Some(v)) => {
                                                return Completion::Normal(JsValue::Boolean(
                                                    to_boolean(&v),
                                                ));
                                            }
                                            Ok(None) => {}
                                            Err(e) => return Completion::Throw(e),
                                        }
                                    }
                                    let mut tm = tobj.borrow_mut();
                                    if let Some(desc) = tm.properties.get(&key)
                                        && desc.configurable == Some(false)
                                    {
                                        return Completion::Normal(JsValue::Boolean(false));
                                    }
                                    tm.properties.remove(&key);
                                    tm.property_order.retain(|k| k != &key);
                                    return Completion::Normal(JsValue::Boolean(true));
                                }
                                return Completion::Normal(JsValue::Boolean(false));
                            }
                            Err(e) => return Completion::Throw(e),
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
                    return Completion::Normal(JsValue::Boolean(true));
                }
                Completion::Normal(JsValue::Boolean(false))
            },
        ));
        reflect_obj
            .borrow_mut()
            .insert_builtin("deleteProperty".to_string(), del_prop_fn);

        // Reflect.get(target, key, receiver?)
        let get_fn = self.create_function(JsFunction::native(
            "get".to_string(),
            2,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(target, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.get requires an object"),
                    );
                }
                let key = args.get(1).map(to_property_key_string).unwrap_or_default();
                let receiver = args.get(2).cloned().unwrap_or(target.clone());
                if let JsValue::Object(ref o) = target {
                    interp.get_object_property(o.id, &key, &receiver)
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            },
        ));
        reflect_obj
            .borrow_mut()
            .insert_builtin("get".to_string(), get_fn);

        // Reflect.getOwnPropertyDescriptor(target, key)
        let gopd_fn =
            self.create_function(JsFunction::native(
                "getOwnPropertyDescriptor".to_string(),
                2,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if !matches!(target, JsValue::Object(_)) {
                        return Completion::Throw(interp.create_type_error(
                            "Reflect.getOwnPropertyDescriptor requires an object",
                        ));
                    }
                    let key = args.get(1).map(to_property_key_string).unwrap_or_default();
                    if let JsValue::Object(ref o) = target {
                        if let Some(obj) = interp.get_object(o.id)
                            && obj.borrow().is_proxy()
                        {
                            let target_inner = interp.get_proxy_target_val(o.id);
                            let key_val = JsValue::String(JsString::from_str(&key));
                            match interp.invoke_proxy_trap(
                                o.id,
                                "getOwnPropertyDescriptor",
                                vec![target_inner.clone(), key_val],
                            ) {
                                Ok(Some(v)) => return Completion::Normal(v),
                                Ok(None) => {
                                    if let JsValue::Object(ref t) = target_inner
                                        && let Some(tobj) = interp.get_object(t.id)
                                    {
                                        // Proxy-of-proxy: if target is also a proxy, recurse
                                        if tobj.borrow().is_proxy() {
                                            let inner2 = interp.get_proxy_target_val(t.id);
                                            let key_val2 =
                                                JsValue::String(JsString::from_str(&key));
                                            match interp.invoke_proxy_trap(
                                                t.id,
                                                "getOwnPropertyDescriptor",
                                                vec![inner2.clone(), key_val2],
                                            ) {
                                                Ok(Some(v)) => return Completion::Normal(v),
                                                Ok(None) => {
                                                    if let JsValue::Object(ref t2) = inner2
                                                        && let Some(tobj2) =
                                                            interp.get_object(t2.id)
                                                        && let Some(desc) = tobj2
                                                            .borrow()
                                                            .get_own_property(&key)
                                                    {
                                                        return Completion::Normal(
                                                            interp.from_property_descriptor(&desc),
                                                        );
                                                    }
                                                    return Completion::Normal(JsValue::Undefined);
                                                }
                                                Err(e) => return Completion::Throw(e),
                                            }
                                        }
                                        if let Some(desc) =
                                            tobj.borrow().get_own_property(&key)
                                        {
                                            return Completion::Normal(
                                                interp.from_property_descriptor(&desc),
                                            );
                                        }
                                    }
                                    return Completion::Normal(JsValue::Undefined);
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        if let Some(obj) = interp.get_object(o.id)
                            && let Some(desc) = obj.borrow().get_own_property(&key)
                        {
                            return Completion::Normal(interp.from_property_descriptor(&desc));
                        }
                    }
                    Completion::Normal(JsValue::Undefined)
                },
            ));
        reflect_obj
            .borrow_mut()
            .insert_builtin("getOwnPropertyDescriptor".to_string(), gopd_fn);

        // Reflect.getPrototypeOf(target)
        let gpo_fn = self.create_function(JsFunction::native(
            "getPrototypeOf".to_string(),
            1,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(target, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.getPrototypeOf requires an object"),
                    );
                }
                if let JsValue::Object(ref o) = target
                    && let Some(obj) = interp.get_object(o.id)
                {
                    if obj.borrow().is_proxy() {
                        let target_inner = interp.get_proxy_target_val(o.id);
                        match interp.invoke_proxy_trap(o.id, "getPrototypeOf", vec![target_inner.clone()]) {
                            Ok(Some(v)) => {
                                if let JsValue::Object(ref t) = target_inner
                                    && let Some(tobj) = interp.get_object(t.id)
                                    && !tobj.borrow().extensible
                                {
                                    let actual_proto = {
                                        let b = tobj.borrow();
                                        if let Some(ref p) = b.prototype {
                                            if let Some(pid) = p.borrow().id {
                                                JsValue::Object(crate::types::JsObject { id: pid })
                                            } else { JsValue::Null }
                                        } else { JsValue::Null }
                                    };
                                    let same = match (&v, &actual_proto) {
                                        (JsValue::Object(a), JsValue::Object(b)) => a.id == b.id,
                                        (JsValue::Null, JsValue::Null) => true,
                                        _ => false,
                                    };
                                    if !same {
                                        return Completion::Throw(interp.create_type_error(
                                            "'getPrototypeOf' on proxy: proxy target is non-extensible but the trap did not return its actual prototype",
                                        ));
                                    }
                                }
                                return Completion::Normal(v);
                            }
                            Ok(None) => {
                                if let JsValue::Object(ref t) = target_inner
                                    && let Some(tobj) = interp.get_object(t.id)
                                {
                                    // Proxy-of-proxy: if target is also a proxy, recurse
                                    if tobj.borrow().is_proxy() {
                                        let inner2 = interp.get_proxy_target_val(t.id);
                                        match interp.invoke_proxy_trap(
                                            t.id,
                                            "getPrototypeOf",
                                            vec![inner2.clone()],
                                        ) {
                                            Ok(Some(v)) => return Completion::Normal(v),
                                            Ok(None) => {
                                                if let JsValue::Object(ref t2) = inner2
                                                    && let Some(tobj2) = interp.get_object(t2.id)
                                                    && let Some(proto) =
                                                        &tobj2.borrow().prototype
                                                    && let Some(id) = proto.borrow().id
                                                {
                                                    return Completion::Normal(JsValue::Object(
                                                        crate::types::JsObject { id },
                                                    ));
                                                }
                                                return Completion::Normal(JsValue::Null);
                                            }
                                            Err(e) => return Completion::Throw(e),
                                        }
                                    }
                                    if let Some(proto) = &tobj.borrow().prototype
                                        && let Some(id) = proto.borrow().id
                                    {
                                        return Completion::Normal(JsValue::Object(
                                            crate::types::JsObject { id },
                                        ));
                                    }
                                }
                                return Completion::Normal(JsValue::Null);
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    if let Some(proto) = &obj.borrow().prototype
                        && let Some(id) = proto.borrow().id
                    {
                        return Completion::Normal(JsValue::Object(crate::types::JsObject { id }));
                    }
                }
                Completion::Normal(JsValue::Null)
            },
        ));
        reflect_obj
            .borrow_mut()
            .insert_builtin("getPrototypeOf".to_string(), gpo_fn);

        // Reflect.has(target, key)
        let has_fn = self.create_function(JsFunction::native(
            "has".to_string(),
            1,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(target, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.has requires an object"),
                    );
                }
                let key = args.get(1).map(to_property_key_string).unwrap_or_default();
                if let JsValue::Object(ref o) = target
                    && let Some(obj) = interp.get_object(o.id)
                {
                    if obj.borrow().is_proxy() {
                        let target_inner = interp.get_proxy_target_val(o.id);
                        let key_val = JsValue::String(JsString::from_str(&key));
                        match interp.invoke_proxy_trap(o.id, "has", vec![target_inner.clone(), key_val.clone()]) {
                            Ok(Some(v)) => {
                                let trap_result = to_boolean(&v);
                                if !trap_result {
                                    if let JsValue::Object(ref t) = target_inner
                                        && let Some(tobj) = interp.get_object(t.id)
                                    {
                                        let target_desc = tobj.borrow().get_own_property(&key);
                                        if let Some(ref desc) = target_desc {
                                            if desc.configurable == Some(false) {
                                                return Completion::Throw(interp.create_type_error(
                                                    "'has' on proxy: trap returned falsish for property which exists in the proxy target as non-configurable",
                                                ));
                                            }
                                            if !tobj.borrow().extensible {
                                                return Completion::Throw(interp.create_type_error(
                                                    "'has' on proxy: trap returned falsish for property but the proxy target is not extensible",
                                                ));
                                            }
                                        }
                                    }
                                }
                                return Completion::Normal(JsValue::Boolean(trap_result));
                            }
                            Ok(None) => {
                                if let JsValue::Object(ref t) = target_inner
                                    && let Some(tobj) = interp.get_object(t.id)
                                {
                                    // Proxy-of-proxy: if target is also a proxy, recurse
                                    if tobj.borrow().is_proxy() {
                                        let inner2 = interp.get_proxy_target_val(t.id);
                                        match interp.invoke_proxy_trap(
                                            t.id,
                                            "has",
                                            vec![inner2.clone(), key_val],
                                        ) {
                                            Ok(Some(v)) => {
                                                return Completion::Normal(JsValue::Boolean(
                                                    to_boolean(&v),
                                                ));
                                            }
                                            Ok(None) => {
                                                if let JsValue::Object(ref t2) = inner2
                                                    && let Some(tobj2) = interp.get_object(t2.id)
                                                {
                                                    return Completion::Normal(JsValue::Boolean(
                                                        tobj2.borrow().has_property(&key),
                                                    ));
                                                }
                                                return Completion::Normal(JsValue::Boolean(false));
                                            }
                                            Err(e) => return Completion::Throw(e),
                                        }
                                    }
                                    return Completion::Normal(JsValue::Boolean(tobj.borrow().has_property(&key)));
                                }
                                return Completion::Normal(JsValue::Boolean(false));
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    return Completion::Normal(JsValue::Boolean(obj.borrow().has_property(&key)));
                }
                Completion::Normal(JsValue::Boolean(false))
            },
        ));
        reflect_obj
            .borrow_mut()
            .insert_builtin("has".to_string(), has_fn);

        // Reflect.isExtensible(target)
        let is_ext_fn = self.create_function(JsFunction::native(
            "isExtensible".to_string(),
            1,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(target, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.isExtensible requires an object"),
                    );
                }
                if let JsValue::Object(ref o) = target
                    && let Some(obj) = interp.get_object(o.id)
                {
                    if obj.borrow().is_proxy() {
                        let target_inner = interp.get_proxy_target_val(o.id);
                        match interp.invoke_proxy_trap(o.id, "isExtensible", vec![target_inner.clone()]) {
                            Ok(Some(v)) => {
                                let trap_result = to_boolean(&v);
                                if let JsValue::Object(ref t) = target_inner
                                    && let Some(tobj) = interp.get_object(t.id)
                                {
                                    if trap_result != tobj.borrow().extensible {
                                        return Completion::Throw(interp.create_type_error(
                                            "'isExtensible' on proxy: trap result does not reflect extensibility of proxy target",
                                        ));
                                    }
                                }
                                return Completion::Normal(JsValue::Boolean(trap_result));
                            }
                            Ok(None) => {
                                if let JsValue::Object(ref t) = target_inner
                                    && let Some(tobj) = interp.get_object(t.id)
                                {
                                    // Proxy-of-proxy
                                    if tobj.borrow().is_proxy() {
                                        let inner2 = interp.get_proxy_target_val(t.id);
                                        match interp.invoke_proxy_trap(
                                            t.id,
                                            "isExtensible",
                                            vec![inner2.clone()],
                                        ) {
                                            Ok(Some(v)) => {
                                                return Completion::Normal(JsValue::Boolean(
                                                    to_boolean(&v),
                                                ));
                                            }
                                            Ok(None) => {
                                                if let JsValue::Object(ref t2) = inner2
                                                    && let Some(tobj2) = interp.get_object(t2.id)
                                                {
                                                    return Completion::Normal(JsValue::Boolean(
                                                        tobj2.borrow().extensible,
                                                    ));
                                                }
                                            }
                                            Err(e) => return Completion::Throw(e),
                                        }
                                    }
                                    return Completion::Normal(JsValue::Boolean(tobj.borrow().extensible));
                                }
                                return Completion::Normal(JsValue::Boolean(false));
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    return Completion::Normal(JsValue::Boolean(obj.borrow().extensible));
                }
                Completion::Normal(JsValue::Boolean(false))
            },
        ));
        reflect_obj
            .borrow_mut()
            .insert_builtin("isExtensible".to_string(), is_ext_fn);

        // Reflect.ownKeys(target)
        let own_keys_fn = self.create_function(JsFunction::native(
            "ownKeys".to_string(),
            1,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(target, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.ownKeys requires an object"),
                    );
                }
                if let JsValue::Object(ref o) = target
                    && let Some(obj) = interp.get_object(o.id)
                {
                    if obj.borrow().is_proxy() {
                        let target_inner = interp.get_proxy_target_val(o.id);
                        match interp.invoke_proxy_trap(o.id, "ownKeys", vec![target_inner.clone()])
                        {
                            Ok(Some(v)) => {
                                if let Err(e) = interp.validate_ownkeys_invariant(&v, &target_inner)
                                {
                                    return Completion::Throw(e);
                                }
                                return Completion::Normal(v);
                            }
                            Ok(None) => {
                                if let JsValue::Object(ref t) = target_inner
                                    && let Some(tobj) = interp.get_object(t.id)
                                {
                                    // Proxy-of-proxy: if target is also a proxy, recurse
                                    if tobj.borrow().is_proxy() {
                                        let inner2 = interp.get_proxy_target_val(t.id);
                                        match interp.invoke_proxy_trap(
                                            t.id,
                                            "ownKeys",
                                            vec![inner2.clone()],
                                        ) {
                                            Ok(Some(v)) => return Completion::Normal(v),
                                            Ok(None) => {
                                                if let JsValue::Object(ref t2) = inner2
                                                    && let Some(tobj2) = interp.get_object(t2.id)
                                                {
                                                    let keys: Vec<JsValue> = tobj2
                                                        .borrow()
                                                        .property_order
                                                        .iter()
                                                        .map(|k| {
                                                            JsValue::String(JsString::from_str(k))
                                                        })
                                                        .collect();
                                                    let arr = interp.create_array(keys);
                                                    return Completion::Normal(arr);
                                                }
                                            }
                                            Err(e) => return Completion::Throw(e),
                                        }
                                    } else {
                                        let keys: Vec<JsValue> = tobj
                                            .borrow()
                                            .property_order
                                            .iter()
                                            .map(|k| JsValue::String(JsString::from_str(k)))
                                            .collect();
                                        let arr = interp.create_array(keys);
                                        return Completion::Normal(arr);
                                    }
                                }
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    let keys: Vec<JsValue> = obj
                        .borrow()
                        .property_order
                        .iter()
                        .map(|k| JsValue::String(JsString::from_str(k)))
                        .collect();
                    let arr = interp.create_array(keys);
                    return Completion::Normal(arr);
                }
                Completion::Normal(interp.create_array(Vec::new()))
            },
        ));
        reflect_obj
            .borrow_mut()
            .insert_builtin("ownKeys".to_string(), own_keys_fn);

        // Reflect.preventExtensions(target)
        let pe_fn = self.create_function(JsFunction::native(
            "preventExtensions".to_string(),
            1,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(target, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.preventExtensions requires an object"),
                    );
                }
                if let JsValue::Object(ref o) = target
                    && let Some(obj) = interp.get_object(o.id)
                {
                    if obj.borrow().is_proxy() {
                        let target_inner = interp.get_proxy_target_val(o.id);
                        match interp.invoke_proxy_trap(o.id, "preventExtensions", vec![target_inner.clone()]) {
                            Ok(Some(v)) => {
                                let trap_result = to_boolean(&v);
                                if trap_result {
                                    if let JsValue::Object(ref t) = target_inner
                                        && let Some(tobj) = interp.get_object(t.id)
                                        && tobj.borrow().extensible
                                    {
                                        return Completion::Throw(interp.create_type_error(
                                            "'preventExtensions' on proxy: trap returned truish but the proxy target is extensible",
                                        ));
                                    }
                                }
                                return Completion::Normal(JsValue::Boolean(trap_result));
                            }
                            Ok(None) => {
                                if let JsValue::Object(ref t) = target_inner
                                    && let Some(tobj) = interp.get_object(t.id)
                                {
                                    // Proxy-of-proxy
                                    if tobj.borrow().is_proxy() {
                                        let inner2 = interp.get_proxy_target_val(t.id);
                                        match interp.invoke_proxy_trap(
                                            t.id,
                                            "preventExtensions",
                                            vec![inner2],
                                        ) {
                                            Ok(Some(v)) => {
                                                return Completion::Normal(JsValue::Boolean(
                                                    to_boolean(&v),
                                                ));
                                            }
                                            Ok(None) => {}
                                            Err(e) => return Completion::Throw(e),
                                        }
                                    }
                                    tobj.borrow_mut().extensible = false;
                                }
                                return Completion::Normal(JsValue::Boolean(true));
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    obj.borrow_mut().extensible = false;
                }
                Completion::Normal(JsValue::Boolean(true))
            },
        ));
        reflect_obj
            .borrow_mut()
            .insert_builtin("preventExtensions".to_string(), pe_fn);

        // Reflect.set(target, key, value, receiver?)
        let set_fn = self.create_function(JsFunction::native(
            "set".to_string(),
            3,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(target, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.set requires an object"),
                    );
                }
                let key = args.get(1).map(to_property_key_string).unwrap_or_default();
                let value = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                let receiver = args.get(3).cloned().unwrap_or(target.clone());
                // Check if target is a proxy
                if let JsValue::Object(ref o) = target
                    && let Some(obj) = interp.get_object(o.id)
                    && obj.borrow().is_proxy()
                {
                    let target_inner = interp.get_proxy_target_val(o.id);
                    let key_val = JsValue::String(JsString::from_str(&key));
                    match interp.invoke_proxy_trap(o.id, "set", vec![target_inner.clone(), key_val, value.clone(), receiver.clone()]) {
                        Ok(Some(v)) => {
                            let trap_result = to_boolean(&v);
                            if trap_result {
                                if let JsValue::Object(ref t) = target_inner
                                    && let Some(tobj) = interp.get_object(t.id)
                                {
                                    let target_desc = tobj.borrow().get_own_property(&key);
                                    if let Some(ref desc) = target_desc {
                                        if desc.configurable == Some(false) {
                                            if desc.is_data_descriptor()
                                                && desc.writable == Some(false)
                                                && !same_value(&value, desc.value.as_ref().unwrap_or(&JsValue::Undefined))
                                            {
                                                return Completion::Throw(interp.create_type_error(
                                                    "'set' on proxy: trap returned truish for property which exists in the proxy target as a non-configurable and non-writable data property with a different value",
                                                ));
                                            }
                                            if desc.is_accessor_descriptor()
                                                && matches!(desc.set.as_ref().unwrap_or(&JsValue::Undefined), JsValue::Undefined)
                                            {
                                                return Completion::Throw(interp.create_type_error(
                                                    "'set' on proxy: trap returned truish for property which exists in the proxy target as a non-configurable and non-writable accessor property without a setter",
                                                ));
                                            }
                                        }
                                    }
                                }
                            }
                            return Completion::Normal(JsValue::Boolean(trap_result));
                        }
                        Ok(None) => {
                            if let JsValue::Object(ref t) = target_inner
                                && let Some(tobj) = interp.get_object(t.id)
                            {
                                // Proxy-of-proxy: if target is also a proxy, recurse
                                if tobj.borrow().is_proxy() {
                                    let inner2 = interp.get_proxy_target_val(t.id);
                                    let key_val2 = JsValue::String(JsString::from_str(&key));
                                    match interp.invoke_proxy_trap(
                                        t.id,
                                        "set",
                                        vec![inner2, key_val2, value.clone(), receiver],
                                    ) {
                                        Ok(Some(v)) => {
                                            return Completion::Normal(JsValue::Boolean(
                                                to_boolean(&v),
                                            ));
                                        }
                                        Ok(None) => {
                                            // Fall through to set on inner target
                                        }
                                        Err(e) => return Completion::Throw(e),
                                    }
                                }
                                tobj.borrow_mut().set_property_value(&key, value);
                                return Completion::Normal(JsValue::Boolean(true));
                            }
                            return Completion::Normal(JsValue::Boolean(false));
                        }
                        Err(e) => return Completion::Throw(e),
                    }
                }
                if let JsValue::Object(ref o) = receiver
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let desc = obj.borrow().get_property_descriptor(&key);
                    if let Some(ref d) = desc
                        && let Some(ref setter) = d.set
                    {
                        let setter = setter.clone();
                        return match interp.call_function(&setter, &receiver, &[value]) {
                            Completion::Normal(_) => Completion::Normal(JsValue::Boolean(true)),
                            Completion::Throw(e) => Completion::Throw(e),
                            _ => Completion::Normal(JsValue::Boolean(true)),
                        };
                    }
                    if let Some(ref d) = desc
                        && d.writable == Some(false)
                    {
                        return Completion::Normal(JsValue::Boolean(false));
                    }
                    obj.borrow_mut().set_property_value(&key, value);
                    return Completion::Normal(JsValue::Boolean(true));
                }
                Completion::Normal(JsValue::Boolean(false))
            },
        ));
        reflect_obj
            .borrow_mut()
            .insert_builtin("set".to_string(), set_fn);

        // Reflect.setPrototypeOf(target, proto)
        let spo_fn = self.create_function(JsFunction::native(
            "setPrototypeOf".to_string(),
            2,
            |interp, _this, args| {
                let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(target, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("Reflect.setPrototypeOf requires an object"),
                    );
                }
                let proto = args.get(1).cloned().unwrap_or(JsValue::Null);
                if let JsValue::Object(ref o) = target
                    && let Some(obj) = interp.get_object(o.id)
                {
                    if obj.borrow().is_proxy() {
                        let target_inner = interp.get_proxy_target_val(o.id);
                        match interp.invoke_proxy_trap(o.id, "setPrototypeOf", vec![target_inner.clone(), proto.clone()]) {
                            Ok(Some(v)) => {
                                let trap_result = to_boolean(&v);
                                if trap_result {
                                    if let JsValue::Object(ref t) = target_inner
                                        && let Some(tobj) = interp.get_object(t.id)
                                        && !tobj.borrow().extensible
                                    {
                                        let actual_proto = {
                                            let b = tobj.borrow();
                                            if let Some(ref p) = b.prototype {
                                                if let Some(pid) = p.borrow().id {
                                                    JsValue::Object(crate::types::JsObject { id: pid })
                                                } else { JsValue::Null }
                                            } else { JsValue::Null }
                                        };
                                        let same = match (&proto, &actual_proto) {
                                            (JsValue::Object(a), JsValue::Object(b)) => a.id == b.id,
                                            (JsValue::Null, JsValue::Null) => true,
                                            _ => false,
                                        };
                                        if !same {
                                            return Completion::Throw(interp.create_type_error(
                                                "'setPrototypeOf' on proxy: trap returned truish for setting a new prototype on the non-extensible proxy target",
                                            ));
                                        }
                                    }
                                }
                                return Completion::Normal(JsValue::Boolean(trap_result));
                            }
                            Ok(None) => {
                                if let JsValue::Object(ref t) = target_inner
                                    && let Some(tobj) = interp.get_object(t.id)
                                {
                                    // Proxy-of-proxy
                                    if tobj.borrow().is_proxy() {
                                        let inner2 = interp.get_proxy_target_val(t.id);
                                        match interp.invoke_proxy_trap(
                                            t.id,
                                            "setPrototypeOf",
                                            vec![inner2, proto.clone()],
                                        ) {
                                            Ok(Some(v)) => {
                                                return Completion::Normal(JsValue::Boolean(
                                                    to_boolean(&v),
                                                ));
                                            }
                                            Ok(None) => {}
                                            Err(e) => return Completion::Throw(e),
                                        }
                                    }
                                    match &proto {
                                        JsValue::Null => { tobj.borrow_mut().prototype = None; }
                                        JsValue::Object(p) => {
                                            if let Some(po) = interp.get_object(p.id) {
                                                tobj.borrow_mut().prototype = Some(po);
                                            }
                                        }
                                        _ => return Completion::Normal(JsValue::Boolean(false)),
                                    }
                                    return Completion::Normal(JsValue::Boolean(true));
                                }
                                return Completion::Normal(JsValue::Boolean(false));
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    match &proto {
                        JsValue::Null => {
                            obj.borrow_mut().prototype = None;
                        }
                        JsValue::Object(p) => {
                            if let Some(proto_obj) = interp.get_object(p.id) {
                                obj.borrow_mut().prototype = Some(proto_obj);
                            }
                        }
                        _ => {
                            return Completion::Normal(JsValue::Boolean(false));
                        }
                    }
                    return Completion::Normal(JsValue::Boolean(true));
                }
                Completion::Normal(JsValue::Boolean(false))
            },
        ));
        reflect_obj
            .borrow_mut()
            .insert_builtin("setPrototypeOf".to_string(), spo_fn);

        // Register Reflect as global
        let reflect_val = JsValue::Object(crate::types::JsObject { id: reflect_id });
        self.global_env
            .borrow_mut()
            .declare("Reflect", BindingKind::Const);
        let _ = self.global_env.borrow_mut().set("Reflect", reflect_val);
    }

    fn setup_proxy(&mut self) {
        // Proxy constructor
        let proxy_fn = self.create_function(JsFunction::constructor(
            "Proxy".to_string(),
            2,
            |interp, _this, args| {
                // Must be called with new (we check new.target)
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Constructor Proxy requires 'new'"),
                    );
                }
                let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                let handler = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                if !matches!(target, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("Cannot create proxy with a non-object as target"),
                    );
                }
                if !matches!(handler, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp
                            .create_type_error("Cannot create proxy with a non-object as handler"),
                    );
                }
                let proxy_obj = interp.create_object();
                proxy_obj.borrow_mut().class_name = "Proxy".to_string();
                if let JsValue::Object(ref t) = target
                    && let Some(target_rc) = interp.get_object(t.id)
                {
                    // Copy callable if target is callable
                    let callable = target_rc.borrow().callable.clone();
                    if callable.is_some() {
                        proxy_obj.borrow_mut().callable = callable;
                    }
                    proxy_obj.borrow_mut().proxy_target = Some(target_rc);
                }
                if let JsValue::Object(ref h) = handler
                    && let Some(handler_rc) = interp.get_object(h.id)
                {
                    proxy_obj.borrow_mut().proxy_handler = Some(handler_rc);
                }
                let proxy_id = proxy_obj.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id: proxy_id }))
            },
        ));

        // Override eval_new behavior: Proxy constructor returns proxy_obj, not new_obj
        // The proxy constructor already returns an Object, so eval_new will use it.

        // Proxy.revocable(target, handler)
        if let JsValue::Object(ref pf) = proxy_fn
            && let Some(proxy_func_obj) = self.get_object(pf.id)
        {
            let revocable_fn = self.create_function(JsFunction::native(
                "revocable".to_string(),
                2,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let handler = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    if !matches!(target, JsValue::Object(_)) {
                        return Completion::Throw(
                            interp.create_type_error(
                                "Cannot create proxy with a non-object as target",
                            ),
                        );
                    }
                    if !matches!(handler, JsValue::Object(_)) {
                        return Completion::Throw(interp.create_type_error(
                            "Cannot create proxy with a non-object as handler",
                        ));
                    }
                    let proxy_obj = interp.create_object();
                    proxy_obj.borrow_mut().class_name = "Proxy".to_string();
                    if let JsValue::Object(ref t) = target
                        && let Some(target_rc) = interp.get_object(t.id)
                    {
                        let callable = target_rc.borrow().callable.clone();
                        if callable.is_some() {
                            proxy_obj.borrow_mut().callable = callable;
                        }
                        proxy_obj.borrow_mut().proxy_target = Some(target_rc);
                    }
                    if let JsValue::Object(ref h) = handler
                        && let Some(handler_rc) = interp.get_object(h.id)
                    {
                        proxy_obj.borrow_mut().proxy_handler = Some(handler_rc);
                    }
                    let proxy_id = proxy_obj.borrow().id.unwrap();
                    let proxy_val = JsValue::Object(crate::types::JsObject { id: proxy_id });

                    // Create revoke function that captures proxy_id
                    let revoke_fn = interp.create_function(JsFunction::native(
                        "".to_string(),
                        0,
                        move |interp2, _this2, _args2| {
                            if let Some(p) = interp2.get_object(proxy_id) {
                                let mut pm = p.borrow_mut();
                                pm.proxy_revoked = true;
                                pm.proxy_target = None;
                                pm.proxy_handler = None;
                            }
                            Completion::Normal(JsValue::Undefined)
                        },
                    ));

                    let result = interp.create_object();
                    result
                        .borrow_mut()
                        .insert_builtin("proxy".to_string(), proxy_val);
                    result
                        .borrow_mut()
                        .insert_builtin("revoke".to_string(), revoke_fn);
                    let result_id = result.borrow().id.unwrap();
                    Completion::Normal(JsValue::Object(crate::types::JsObject { id: result_id }))
                },
            ));
            proxy_func_obj
                .borrow_mut()
                .insert_builtin("revocable".to_string(), revocable_fn);
        }

        self.global_env
            .borrow_mut()
            .declare("Proxy", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("Proxy", proxy_fn);
    }

    fn setup_function_prototype(&mut self, obj_proto: &Rc<RefCell<JsObjectData>>) {
        // Add call to Object.prototype (simplified - applies to all functions via prototype chain)
        let call_fn = self.create_function(JsFunction::native(
            "call".to_string(),
            1,
            |interp, _this, args| {
                let this_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let call_args = if args.len() > 1 { &args[1..] } else { &[] };
                interp.call_function(_this, &this_arg, call_args)
            },
        ));
        obj_proto
            .borrow_mut()
            .insert_builtin("call".to_string(), call_fn);

        // Add apply
        let apply_fn = self.create_function(JsFunction::native(
            "apply".to_string(),
            3,
            |interp, _this, args| {
                let this_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let arr_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let mut call_args = Vec::new();
                if let JsValue::Object(ref o) = arr_arg
                    && let Some(arr_obj) = interp.get_object(o.id)
                {
                    let b = arr_obj.borrow();
                    if let Some(elems) = &b.array_elements {
                        call_args = elems.clone();
                    } else {
                        let len = to_number(&b.get_property("length")) as usize;
                        for i in 0..len {
                            call_args.push(b.get_property(&i.to_string()));
                        }
                    }
                }
                interp.call_function(_this, &this_arg, &call_args)
            },
        ));
        obj_proto
            .borrow_mut()
            .insert_builtin("apply".to_string(), apply_fn);

        // Function.prototype.bind
        let bind_fn = self.create_function(JsFunction::native(
            "bind".to_string(),
            1,
            |interp, this_val, args: &[JsValue]| {
                if !matches!(this_val, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("Bind must be called on a function"),
                    );
                }
                // Check if target is callable
                let is_callable = if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    obj.borrow().callable.is_some()
                } else {
                    false
                };
                if !is_callable {
                    return Completion::Throw(
                        interp.create_type_error("Bind must be called on a function"),
                    );
                }

                let bind_this = args.first().cloned().unwrap_or(JsValue::Undefined);
                let bound_args: Vec<JsValue> = args.iter().skip(1).cloned().collect();
                let func = this_val.clone();

                // Read target length and compute bound length
                let target_length = if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    obj.borrow()
                        .get_property_value("length")
                        .and_then(|v| match v {
                            JsValue::Number(n) => Some(n as usize),
                            _ => None,
                        })
                        .unwrap_or(0)
                } else {
                    0
                };
                let bound_length = target_length.saturating_sub(bound_args.len());

                // Read target name
                let target_name = if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    obj.borrow()
                        .get_property_value("name")
                        .and_then(|v| match v {
                            JsValue::String(s) => Some(s.to_string()),
                            _ => None,
                        })
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                let bound_name = format!("bound {}", target_name);

                // Check if target is a constructor
                let is_ctor = if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    match &obj.borrow().callable {
                        Some(JsFunction::User { is_arrow, .. }) => !is_arrow,
                        Some(JsFunction::Native(_, _, _, ctor)) => *ctor,
                        None => false,
                    }
                } else {
                    false
                };

                let _bound_args_len = bound_args.len();
                let bound = if is_ctor {
                    JsFunction::constructor(
                        bound_name,
                        bound_length,
                        move |interp2, this, call_args: &[JsValue]| {
                            let mut all_args = bound_args.clone();
                            all_args.extend_from_slice(call_args);
                            // When called as constructor, new_target is set and this is a fresh object
                            // Use that this (not bind_this) — the new machinery already created it
                            if interp2.new_target.is_some() {
                                interp2.call_function(&func, this, &all_args)
                            } else {
                                interp2.call_function(&func, &bind_this, &all_args)
                            }
                        },
                    )
                } else {
                    JsFunction::Native(
                        bound_name,
                        bound_length,
                        Rc::new(
                            move |interp2: &mut Interpreter,
                                  _this: &JsValue,
                                  call_args: &[JsValue]| {
                                let mut all_args = bound_args.clone();
                                all_args.extend_from_slice(call_args);
                                interp2.call_function(&func, &bind_this, &all_args)
                            },
                        ),
                        false,
                    )
                };
                Completion::Normal(interp.create_function(bound))
            },
        ));
        obj_proto
            .borrow_mut()
            .insert_builtin("bind".to_string(), bind_fn);

        // Merge Function.prototype.toString into Object.prototype.toString
        // The existing Object.prototype.toString already handles [object Type] for non-functions.
        // We override it with a combined version that handles both functions and objects.
        let combined_tostring = self.create_function(JsFunction::native(
            "toString".to_string(),
            0,
            |interp, this_val, _args: &[JsValue]| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    if let Some(ref func) = obj.borrow().callable {
                        let s = match func {
                            JsFunction::User {
                                source_text: Some(text),
                                ..
                            } => text.clone(),
                            JsFunction::User {
                                name,
                                is_arrow,
                                is_async,
                                is_generator,
                                ..
                            } => {
                                let n = name.clone().unwrap_or_default();
                                if *is_arrow {
                                    "() => { [native code] }".to_string()
                                } else {
                                    let mut prefix = String::new();
                                    if *is_async {
                                        prefix.push_str("async ");
                                    }
                                    if *is_generator {
                                        format!("{prefix}function* {n}() {{ [native code] }}")
                                    } else {
                                        format!("{prefix}function {n}() {{ [native code] }}")
                                    }
                                }
                            }
                            JsFunction::Native(name, _, _, _) => {
                                format!("function {}() {{ [native code] }}", name)
                            }
                        };
                        return Completion::Normal(JsValue::String(JsString::from_str(&s)));
                    }
                    let cn = obj.borrow().class_name.clone();
                    return Completion::Normal(JsValue::String(JsString::from_str(&format!(
                        "[object {cn}]"
                    ))));
                }
                let tag = match this_val {
                    JsValue::Undefined => "Undefined",
                    JsValue::Null => "Null",
                    JsValue::Boolean(_) => "Boolean",
                    JsValue::Number(_) => "Number",
                    JsValue::String(_) => "String",
                    JsValue::Symbol(_) => "Symbol",
                    JsValue::BigInt(_) => "BigInt",
                    JsValue::Object(_) => "Object",
                };
                Completion::Normal(JsValue::String(JsString::from_str(&format!(
                    "[object {tag}]"
                ))))
            },
        ));
        obj_proto
            .borrow_mut()
            .insert_builtin("toString".to_string(), combined_tostring);
    }
}
