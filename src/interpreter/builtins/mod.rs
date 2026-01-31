mod array;
mod collections;
mod date;
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
            console.borrow_mut().insert_value("log".to_string(), log_fn);
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

        // Error constructor
        {
            let error_name = "Error".to_string();
            self.register_global_fn(
                "Error",
                BindingKind::Var,
                JsFunction::native(error_name.clone(), 0, move |interp, this, args| {
                    let msg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(o) = this {
                        if let Some(obj) = interp.get_object(o.id) {
                            let mut o = obj.borrow_mut();
                            o.class_name = "Error".to_string();
                            if !matches!(msg, JsValue::Undefined) {
                                o.insert_value("message".to_string(), msg);
                            }
                        }
                        return Completion::Normal(this.clone());
                    }
                    let obj = interp.create_object();
                    {
                        let mut o = obj.borrow_mut();
                        o.class_name = "Error".to_string();
                        if !matches!(msg, JsValue::Undefined) {
                            o.insert_value("message".to_string(), msg);
                        }
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
            ep.borrow_mut().insert_value(
                "name".to_string(),
                JsValue::String(JsString::from_str("Error")),
            );
            ep.borrow_mut().insert_value(
                "message".to_string(),
                JsValue::String(JsString::from_str("")),
            );
        }

        // Test262Error
        {
            let error_proto_clone = error_prototype.clone();
            self.register_global_fn(
                "Test262Error",
                BindingKind::Var,
                JsFunction::native("Test262Error".to_string(), 1, move |interp, this, args| {
                    let msg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(o) = this {
                        if let Some(obj) = interp.get_object(o.id) {
                            let mut o = obj.borrow_mut();
                            o.class_name = "Test262Error".to_string();
                            if let Some(ref ep) = error_proto_clone {
                                o.prototype = Some(ep.clone());
                            }
                            if !matches!(msg, JsValue::Undefined) {
                                o.insert_value("message".to_string(), msg);
                            }
                            o.insert_value(
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
                            o.insert_value("message".to_string(), msg);
                        }
                        o.insert_value(
                            "name".to_string(),
                            JsValue::String(JsString::from_str("Test262Error")),
                        );
                    }
                    let id = obj.borrow().id.unwrap();
                    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
                }),
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
            let error_proto_clone = error_prototype.clone();
            self.register_global_fn(
                name,
                BindingKind::Var,
                JsFunction::native(error_name.clone(), 0, move |interp, this, args| {
                    let msg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(o) = this {
                        if let Some(obj) = interp.get_object(o.id) {
                            let mut o = obj.borrow_mut();
                            o.class_name = error_name.clone();
                            if let Some(ref ep) = error_proto_clone {
                                o.prototype = Some(ep.clone());
                            }
                            if !matches!(msg, JsValue::Undefined) {
                                o.insert_value("message".to_string(), msg.clone());
                            }
                            o.insert_value(
                                "name".to_string(),
                                JsValue::String(JsString::from_str(&error_name)),
                            );
                        }
                        return Completion::Normal(this.clone());
                    }
                    let obj = interp.create_object();
                    {
                        let mut o = obj.borrow_mut();
                        o.class_name = error_name.clone();
                        if let Some(ref ep) = error_proto_clone {
                            o.prototype = Some(ep.clone());
                        }
                        if !matches!(msg, JsValue::Undefined) {
                            o.insert_value("message".to_string(), msg);
                        }
                        o.insert_value(
                            "name".to_string(),
                            JsValue::String(JsString::from_str(&error_name)),
                        );
                    }
                    let id = obj.borrow().id.unwrap();
                    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
                }),
            );
        }

        // Object constructor (minimal)
        self.register_global_fn(
            "Object",
            BindingKind::Var,
            JsFunction::native("Object".to_string(), 1, |interp, _this, args| {
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
            JsFunction::native("Array".to_string(), 1, |interp, _this, args| {
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
            let symbol_fn = self.create_function(JsFunction::native(
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
        self.setup_array_prototype();
        self.setup_string_prototype();

        // String constructor/converter
        self.register_global_fn(
            "String",
            BindingKind::Var,
            JsFunction::native("String".to_string(), 1, |interp, this, args| {
                let val = args
                    .first()
                    .cloned()
                    .unwrap_or(JsValue::String(JsString::from_str("")));
                let s = to_js_string(&val);
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

        // Number constructor/converter
        self.register_global_fn(
            "Number",
            BindingKind::Var,
            JsFunction::native("Number".to_string(), 1, |interp, this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::Number(0.0));
                let n = to_number(&val);
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
                n.insert_value(
                    "POSITIVE_INFINITY".to_string(),
                    JsValue::Number(f64::INFINITY),
                );
                n.insert_value(
                    "NEGATIVE_INFINITY".to_string(),
                    JsValue::Number(f64::NEG_INFINITY),
                );
                n.insert_value("MAX_VALUE".to_string(), JsValue::Number(f64::MAX));
                n.insert_value("MIN_VALUE".to_string(), JsValue::Number(f64::MIN_POSITIVE));
                n.insert_value("NaN".to_string(), JsValue::Number(f64::NAN));
                n.insert_value("EPSILON".to_string(), JsValue::Number(f64::EPSILON));
                n.insert_value(
                    "MAX_SAFE_INTEGER".to_string(),
                    JsValue::Number(9007199254740991.0),
                );
                n.insert_value(
                    "MIN_SAFE_INTEGER".to_string(),
                    JsValue::Number(-9007199254740991.0),
                );
                n.insert_value("isFinite".to_string(), is_finite_fn);
                n.insert_value("isNaN".to_string(), is_nan_fn);
                n.insert_value("isInteger".to_string(), is_integer_fn);
                n.insert_value("isSafeInteger".to_string(), is_safe_fn);
            }
        }

        // Boolean constructor/converter
        self.register_global_fn(
            "Boolean",
            BindingKind::Var,
            JsFunction::native("Boolean".to_string(), 1, |interp, this, args| {
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

        self.setup_symbol_prototype();
        self.setup_number_prototype();
        self.setup_boolean_prototype();
        self.setup_map_prototype();
        self.setup_set_prototype();
        self.setup_weakmap_prototype();
        self.setup_weakset_prototype();
        self.setup_date_builtin();

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
                    n.insert_value("parseInt".to_string(), pi);
                }
                if let Some(pf) = parse_float {
                    n.insert_value("parseFloat".to_string(), pf);
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

        // Math object
        let math_obj = self.create_object();
        let math_id = math_obj.borrow().id.unwrap();
        {
            let mut m = math_obj.borrow_mut();
            m.class_name = "Math".to_string();
            m.insert_value("PI".to_string(), JsValue::Number(std::f64::consts::PI));
            m.insert_value("E".to_string(), JsValue::Number(std::f64::consts::E));
            m.insert_value("LN2".to_string(), JsValue::Number(std::f64::consts::LN_2));
            m.insert_value("LN10".to_string(), JsValue::Number(std::f64::consts::LN_10));
            m.insert_value(
                "LOG2E".to_string(),
                JsValue::Number(std::f64::consts::LOG2_E),
            );
            m.insert_value(
                "LOG10E".to_string(),
                JsValue::Number(std::f64::consts::LOG10_E),
            );
            m.insert_value(
                "SQRT2".to_string(),
                JsValue::Number(std::f64::consts::SQRT_2),
            );
            m.insert_value(
                "SQRT1_2".to_string(),
                JsValue::Number(std::f64::consts::FRAC_1_SQRT_2),
            );
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
            math_obj.borrow_mut().insert_value(name.to_string(), fn_val);
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
            math_obj.borrow_mut().insert_value(name.to_string(), fn_val);
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
                let env = interp.global_env.clone();
                let mut last = JsValue::Undefined;
                for stmt in &program.body {
                    match interp.exec_statement(stmt, &env) {
                        Completion::Normal(v) => {
                            if !matches!(v, JsValue::Undefined) {
                                last = v;
                            }
                        }
                        other => return other,
                    }
                }
                Completion::Normal(last)
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
            JsFunction::native("Function".to_string(), 1, |interp, _this, args| {
                let (params_str, body_str) = if args.is_empty() {
                    (String::new(), String::new())
                } else if args.len() == 1 {
                    (String::new(), to_js_string(&args[0]))
                } else {
                    let params: Vec<String> =
                        args[..args.len() - 1].iter().map(to_js_string).collect();
                    (params.join(","), to_js_string(args.last().unwrap()))
                };

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
                    };
                    Completion::Normal(interp.create_function(js_func))
                } else {
                    Completion::Throw(
                        interp.create_error("SyntaxError", "Failed to parse function"),
                    )
                }
            }),
        );

        // JSON object
        let json_obj = self.create_object();
        let json_stringify = self.create_function(JsFunction::native(
            "stringify".to_string(),
            3,
            |interp, _this, args: &[JsValue]| {
                let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let result = json_stringify_value(interp, &val);
                match result {
                    Some(s) => Completion::Normal(JsValue::String(JsString::from_str(&s))),
                    None => Completion::Normal(JsValue::Undefined),
                }
            },
        ));
        let json_parse = self.create_function(JsFunction::native(
            "parse".to_string(),
            2,
            |interp, _this, args: &[JsValue]| {
                let s = args.first().map(to_js_string).unwrap_or_default();
                json_parse_value(interp, &s)
            },
        ));
        json_obj
            .borrow_mut()
            .insert_builtin("stringify".to_string(), json_stringify);
        json_obj
            .borrow_mut()
            .insert_builtin("parse".to_string(), json_parse);
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
                        .insert_value("fromCharCode".to_string(), from_char_code);
                    obj.borrow_mut()
                        .insert_value("fromCodePoint".to_string(), from_code_point);
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

                // Add hasOwnProperty to Object.prototype
                let has_own_fn = self.create_function(JsFunction::native(
                    "hasOwnProperty".to_string(),
                    1,
                    |interp, this_val, args| {
                        let key = args.first().map(to_js_string).unwrap_or_default();
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

                // Object.prototype.propertyIsEnumerable
                let pie_fn = self.create_function(JsFunction::native(
                    "propertyIsEnumerable".to_string(),
                    1,
                    |interp, this_val, args| {
                        let key = args.first().map(to_js_string).unwrap_or_default();
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
                    let key = args.get(1).map(to_js_string).unwrap_or_default();
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
                                if !obj.borrow_mut().define_own_property(key, desc) {
                                    return Completion::Throw(interp.create_type_error(
                                        "Cannot define property, object is not extensible or property is non-configurable",
                                    ));
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
                .insert_value("defineProperty".to_string(), define_property_fn);

            // Add Object.getOwnPropertyDescriptor
            let get_own_prop_desc_fn = self.create_function(JsFunction::native(
                "getOwnPropertyDescriptor".to_string(),
                2,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let key = args.get(1).map(to_js_string).unwrap_or_default();
                    if let JsValue::Object(ref o) = target {
                        // Proxy getOwnPropertyDescriptor trap
                        if let Some(obj) = interp.get_object(o.id)
                            && obj.borrow().is_proxy() {
                                let target_inner = interp.get_proxy_target_val(o.id);
                                let key_val = JsValue::String(JsString::from_str(&key));
                                match interp.invoke_proxy_trap(
                                    o.id,
                                    "getOwnPropertyDescriptor",
                                    vec![target_inner.clone(), key_val],
                                ) {
                                    Ok(Some(v)) => return Completion::Normal(v),
                                    Ok(None) => {
                                        // No trap, fall through to target
                                        if let JsValue::Object(ref t) = target_inner
                                            && let Some(tobj) = interp.get_object(t.id)
                                            && let Some(desc) =
                                                tobj.borrow().get_own_property(&key).cloned()
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
                        if let Some(obj) = interp.get_object(o.id)
                            && let Some(desc) = obj.borrow().get_own_property(&key).cloned()
                        {
                            return Completion::Normal(interp.from_property_descriptor(&desc));
                        }
                    }
                    Completion::Normal(JsValue::Undefined)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_value("getOwnPropertyDescriptor".to_string(), get_own_prop_desc_fn);

            // Add Object.keys
            let keys_fn = self.create_function(JsFunction::native(
                "keys".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(ref o) = target
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
                                    // Filter to enumerable keys from trap result
                                    // For simplicity, return all keys from trap
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
                .insert_value("keys".to_string(), keys_fn);

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
                .insert_value("freeze".to_string(), freeze_fn);

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
                                Ok(Some(v)) => return Completion::Normal(v),
                                Ok(None) => {
                                    // No trap, fall through to target
                                    if let JsValue::Object(ref t) = target_inner
                                        && let Some(tobj) = interp.get_object(t.id)
                                        && let Some(proto) = &tobj.borrow().prototype
                                        && let Some(id) = proto.borrow().id {
                                            return Completion::Normal(JsValue::Object(
                                                crate::types::JsObject { id },
                                            ));
                                        }
                                    return Completion::Normal(JsValue::Null);
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        if let Some(proto) = &obj.borrow().prototype
                            && let Some(id) = proto.borrow().id {
                                return Completion::Normal(JsValue::Object(
                                    crate::types::JsObject { id },
                                ));
                            }
                    }
                    Completion::Normal(JsValue::Null)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_value("getPrototypeOf".to_string(), get_proto_fn);

            // Add Object.create
            let create_fn = self.create_function(JsFunction::native(
                "create".to_string(),
                2,
                |interp, _this, args| {
                    let proto_arg = args.first().cloned().unwrap_or(JsValue::Null);
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
                        _ => {}
                    }
                    let id = new_obj.borrow().id.unwrap();
                    let target = JsValue::Object(crate::types::JsObject { id });

                    let props_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    if !matches!(props_arg, JsValue::Undefined)
                        && let JsValue::Object(ref d) = props_arg
                            && let Some(desc_obj) = interp.get_object(d.id)
                        {
                            let keys: Vec<String> =
                                desc_obj.borrow().properties.keys().cloned().collect();
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
                                let prop_desc_val = desc_obj.borrow().get_property(&key);
                                if let JsValue::Object(ref pd) = prop_desc_val
                                    && let Some(pd_obj) = interp.get_object(pd.id)
                                {
                                    let b = pd_obj.borrow();
                                    let mut desc = PropertyDescriptor {
                                        value: None,
                                        writable: None,
                                        get: None,
                                        set: None,
                                        enumerable: None,
                                        configurable: None,
                                    };
                                    let v = b.get_property("value");
                                    if !matches!(v, JsValue::Undefined)
                                        || b.has_own_property("value")
                                    {
                                        desc.value = Some(v);
                                    }
                                    let w = b.get_property("writable");
                                    if !matches!(w, JsValue::Undefined)
                                        || b.has_own_property("writable")
                                    {
                                        desc.writable = Some(to_boolean(&w));
                                    }
                                    let e = b.get_property("enumerable");
                                    if !matches!(e, JsValue::Undefined)
                                        || b.has_own_property("enumerable")
                                    {
                                        desc.enumerable = Some(to_boolean(&e));
                                    }
                                    let c = b.get_property("configurable");
                                    if !matches!(c, JsValue::Undefined)
                                        || b.has_own_property("configurable")
                                    {
                                        desc.configurable = Some(to_boolean(&c));
                                    }
                                    let g = b.get_property("get");
                                    if !matches!(g, JsValue::Undefined) || b.has_own_property("get")
                                    {
                                        desc.get = Some(g);
                                    }
                                    let s = b.get_property("set");
                                    if !matches!(s, JsValue::Undefined) || b.has_own_property("set")
                                    {
                                        desc.set = Some(s);
                                    }
                                    drop(b);
                                    if desc.enumerable.is_none() {
                                        desc.enumerable = Some(false);
                                    }
                                    if desc.configurable.is_none() {
                                        desc.configurable = Some(false);
                                    }
                                    if desc.writable.is_none()
                                        && desc.get.is_none()
                                        && desc.set.is_none()
                                    {
                                        desc.writable = Some(false);
                                    }
                                    if let Some(target_obj) = interp.get_object(id) {
                                        target_obj.borrow_mut().insert_property(key, desc);
                                    }
                                }
                            }
                        }

                    Completion::Normal(target)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_value("create".to_string(), create_fn);

            // Object.entries
            let entries_fn = self.create_function(JsFunction::native(
                "entries".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(o) = &target
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        let borrowed = obj.borrow();
                        let pairs: Vec<_> = borrowed
                            .property_order
                            .iter()
                            .filter_map(|k| {
                                let desc = borrowed.properties.get(k)?;
                                if desc.enumerable == Some(false) {
                                    return None;
                                }
                                let key = JsValue::String(JsString::from_str(k));
                                let val = desc.value.clone().unwrap_or(JsValue::Undefined);
                                Some((key, val))
                            })
                            .collect();
                        drop(borrowed);
                        let entries: Vec<JsValue> = pairs
                            .into_iter()
                            .map(|(key, val)| interp.create_array(vec![key, val]))
                            .collect();
                        let arr = interp.create_array(entries);
                        return Completion::Normal(arr);
                    }
                    Completion::Normal(interp.create_array(Vec::new()))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_value("entries".to_string(), entries_fn);

            // Object.values
            let values_fn = self.create_function(JsFunction::native(
                "values".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(o) = &target
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        let borrowed = obj.borrow();
                        let values: Vec<JsValue> = borrowed
                            .property_order
                            .iter()
                            .filter_map(|k| {
                                let desc = borrowed.properties.get(k)?;
                                if desc.enumerable == Some(false) {
                                    return None;
                                }
                                Some(desc.value.clone().unwrap_or(JsValue::Undefined))
                            })
                            .collect();
                        let arr = interp.create_array(values);
                        return Completion::Normal(arr);
                    }
                    Completion::Normal(interp.create_array(Vec::new()))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_value("values".to_string(), values_fn);

            // Object.assign
            let assign_fn = self.create_function(JsFunction::native(
                "assign".to_string(),
                2,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(t) = &target {
                        for source in args.iter().skip(1) {
                            if let JsValue::Object(s) = source
                                && let Some(src_obj) = interp.get_object(s.id)
                            {
                                let borrowed = src_obj.borrow();
                                let props: Vec<(String, JsValue)> = borrowed
                                    .property_order
                                    .iter()
                                    .filter_map(|k| {
                                        let desc = borrowed.properties.get(k)?;
                                        if desc.enumerable == Some(false) {
                                            return None;
                                        }
                                        Some((
                                            k.clone(),
                                            desc.value.clone().unwrap_or(JsValue::Undefined),
                                        ))
                                    })
                                    .collect();
                                drop(borrowed);
                                if let Some(tgt_obj) = interp.get_object(t.id) {
                                    let mut tgt = tgt_obj.borrow_mut();
                                    for (k, v) in props {
                                        tgt.insert_value(k, v);
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
                .insert_value("assign".to_string(), assign_fn);

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
            obj_func.borrow_mut().insert_value("is".to_string(), is_fn);

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
                                Ok(Some(v)) => return Completion::Normal(v),
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
                .insert_value("getOwnPropertyNames".to_string(), gopn_fn);

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
                                    if !to_boolean(&v) {
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
                .insert_value("preventExtensions".to_string(), pe_fn);

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
                                    return Completion::Normal(JsValue::Boolean(to_boolean(&v)));
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
                .insert_value("isExtensible".to_string(), ie_fn);

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
                .insert_value("isFrozen".to_string(), frozen_fn);

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
                .insert_value("isSealed".to_string(), sealed_fn);

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
                .insert_value("seal".to_string(), seal_fn);

            // Object.hasOwn
            let has_own_fn = self.create_function(JsFunction::native(
                "hasOwn".to_string(),
                2,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let key = args.get(1).map(to_js_string).unwrap_or_default();
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
                .insert_value("hasOwn".to_string(), has_own_fn);

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
                                    return Completion::Normal(target);
                                }
                                Ok(None) => {
                                    // No trap, fall through to target
                                    if let JsValue::Object(ref t) = target_inner
                                        && let Some(tobj) = interp.get_object(t.id)
                                    {
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
                .insert_value("setPrototypeOf".to_string(), set_proto_fn);

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
                        // Collect enumerable own property keys
                        let keys: Vec<String> = {
                            let b = desc_obj.borrow();
                            b.properties.iter().filter(|(_, prop)| {
                                prop.enumerable != Some(false)
                            }).map(|(k, _)| k.clone()).collect()
                        };
                        // Collect all descriptors first
                        let mut descriptors: Vec<(String, PropertyDescriptor)> = Vec::new();
                        for key in keys {
                            let prop_desc_val = desc_obj.borrow().get_property(&key);
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
                .insert_value("defineProperties".to_string(), def_props_fn);

            // Object.getOwnPropertyDescriptors
            let get_descs_fn = self.create_function(JsFunction::native(
                "getOwnPropertyDescriptors".to_string(),
                1,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(ref t) = target
                        && let Some(obj) = interp.get_object(t.id)
                    {
                        let result = interp.create_object();
                        let keys: Vec<String> = obj.borrow().properties.keys().cloned().collect();
                        for key in keys {
                            let desc = obj.borrow().properties.get(&key).cloned();
                            if let Some(d) = desc {
                                let desc_result = interp.create_object();
                                if let Some(ref v) = d.value {
                                    desc_result
                                        .borrow_mut()
                                        .insert_value("value".to_string(), v.clone());
                                }
                                if let Some(w) = d.writable {
                                    desc_result
                                        .borrow_mut()
                                        .insert_value("writable".to_string(), JsValue::Boolean(w));
                                }
                                if let Some(e) = d.enumerable {
                                    desc_result.borrow_mut().insert_value(
                                        "enumerable".to_string(),
                                        JsValue::Boolean(e),
                                    );
                                }
                                if let Some(c) = d.configurable {
                                    desc_result.borrow_mut().insert_value(
                                        "configurable".to_string(),
                                        JsValue::Boolean(c),
                                    );
                                }
                                if let Some(ref g) = d.get {
                                    desc_result
                                        .borrow_mut()
                                        .insert_value("get".to_string(), g.clone());
                                }
                                if let Some(ref s) = d.set {
                                    desc_result
                                        .borrow_mut()
                                        .insert_value("set".to_string(), s.clone());
                                }
                                let did = desc_result.borrow().id.unwrap();
                                let dval = JsValue::Object(crate::types::JsObject { id: did });
                                result.borrow_mut().insert_value(key, dval);
                            }
                        }
                        let id = result.borrow().id.unwrap();
                        return Completion::Normal(JsValue::Object(crate::types::JsObject { id }));
                    }
                    Completion::Normal(JsValue::Undefined)
                },
            ));
            obj_func
                .borrow_mut()
                .insert_value("getOwnPropertyDescriptors".to_string(), get_descs_fn);

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
                                obj.borrow_mut().insert_value(k, v);
                            }
                        }
                    }
                    let id = obj.borrow().id.unwrap();
                    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
                },
            ));
            obj_func
                .borrow_mut()
                .insert_value("fromEntries".to_string(), from_entries_fn);
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

    fn iterator_complete(&self, result: &JsValue) -> bool {
        if let JsValue::Object(o) = result
            && let Some(obj) = self.get_object(o.id) {
                let done = obj.borrow().get_property("done");
                return to_boolean(&done);
            }
        true
    }

    pub(crate) fn iterator_value(&self, result: &JsValue) -> JsValue {
        if let JsValue::Object(o) = result
            && let Some(obj) = self.get_object(o.id) {
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
                        && tobj.borrow().callable.is_none() {
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
                            Some(JsFunction::Native(..)) => b.has_own_property("prototype"),
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
                let key = args.get(1).map(to_js_string).unwrap_or_default();
                let desc_val = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(ref o) = target
                    && let Some(obj) = interp.get_object(o.id)
                {
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
                let key = args.get(1).map(to_js_string).unwrap_or_default();
                if let JsValue::Object(ref o) = target
                    && let Some(obj) = interp.get_object(o.id)
                {
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
                let key = args.get(1).map(to_js_string).unwrap_or_default();
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
                    let key = args.get(1).map(to_js_string).unwrap_or_default();
                    if let JsValue::Object(ref o) = target
                        && let Some(obj) = interp.get_object(o.id)
                        && let Some(desc) = obj.borrow().get_own_property(&key).cloned()
                    {
                        return Completion::Normal(interp.from_property_descriptor(&desc));
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
                    && let Some(proto) = &obj.borrow().prototype
                    && let Some(id) = proto.borrow().id {
                        return Completion::Normal(JsValue::Object(crate::types::JsObject { id }));
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
                let key = args.get(1).map(to_js_string).unwrap_or_default();
                if let JsValue::Object(ref o) = target
                    && let Some(obj) = interp.get_object(o.id)
                {
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
                let key = args.get(1).map(to_js_string).unwrap_or_default();
                let value = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                let receiver = args.get(3).cloned().unwrap_or(target.clone());
                if let JsValue::Object(ref o) = receiver
                    && let Some(obj) = interp.get_object(o.id)
                {
                    // Check for setter
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
                    // Check writable
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
        let proxy_fn = self.create_function(JsFunction::native(
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
                        .insert_value("proxy".to_string(), proxy_val);
                    result
                        .borrow_mut()
                        .insert_value("revoke".to_string(), revoke_fn);
                    let result_id = result.borrow().id.unwrap();
                    Completion::Normal(JsValue::Object(crate::types::JsObject { id: result_id }))
                },
            ));
            proxy_func_obj
                .borrow_mut()
                .insert_value("revocable".to_string(), revocable_fn);
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
                let bind_this = args.first().cloned().unwrap_or(JsValue::Undefined);
                let bound_args: Vec<JsValue> = args.iter().skip(1).cloned().collect();
                let func = this_val.clone();
                let bound = JsFunction::native(
                    "bound".to_string(),
                    0,
                    move |interp2, _this, call_args: &[JsValue]| {
                        let mut all_args = bound_args.clone();
                        all_args.extend_from_slice(call_args);
                        interp2.call_function(&func, &bind_this, &all_args)
                    },
                );
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
                    if obj.borrow().callable.is_some() {
                        return Completion::Normal(JsValue::String(JsString::from_str(
                            "function() { [native code] }",
                        )));
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
