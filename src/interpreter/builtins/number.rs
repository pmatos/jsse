use super::super::*;

impl Interpreter {
    pub(crate) fn setup_symbol_prototype(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Symbol".to_string();

        fn this_symbol_value(interp: &Interpreter, this: &JsValue) -> Option<crate::types::JsSymbol> {
            match this {
                JsValue::Symbol(s) => Some(s.clone()),
                JsValue::Object(o) => interp.get_object(o.id).and_then(|obj| {
                    let b = obj.borrow();
                    if b.class_name == "Symbol" {
                        if let Some(JsValue::Symbol(s)) = &b.primitive_value {
                            return Some(s.clone());
                        }
                    }
                    None
                }),
                _ => None,
            }
        }

        let methods: Vec<(
            &str,
            usize,
            Rc<dyn Fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion>,
        )> = vec![
            (
                "toString",
                0,
                Rc::new(|interp, this, _args| {
                    let Some(sym) = this_symbol_value(interp, this) else {
                        let err = interp.create_type_error("Symbol.prototype.toString requires a Symbol");
                        return Completion::Throw(err);
                    };
                    let desc = sym.description.as_ref().map(|d| d.to_rust_string()).unwrap_or_default();
                    Completion::Normal(JsValue::String(JsString::from_str(&format!("Symbol({desc})"))))
                }),
            ),
            (
                "valueOf",
                0,
                Rc::new(|interp, this, _args| {
                    let Some(sym) = this_symbol_value(interp, this) else {
                        let err = interp.create_type_error("Symbol.prototype.valueOf requires a Symbol");
                        return Completion::Throw(err);
                    };
                    Completion::Normal(JsValue::Symbol(sym))
                }),
            ),
        ];

        for (name, arity, func) in methods {
            let fn_val = self.create_function(JsFunction::Native(name.to_string(), arity, func));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
        }

        // description getter
        let desc_getter = self.create_function(JsFunction::Native(
            "get description".to_string(),
            0,
            Rc::new(|interp, this, _args| {
                let Some(sym) = this_symbol_value(interp, this) else {
                    let err = interp.create_type_error("Symbol.prototype.description requires a Symbol");
                    return Completion::Throw(err);
                };
                match sym.description {
                    Some(d) => Completion::Normal(JsValue::String(d)),
                    None => Completion::Normal(JsValue::Undefined),
                }
            }),
        ));
        proto.borrow_mut().insert_property(
            "description".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(desc_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );

        // [Symbol.toPrimitive]
        let to_prim_fn = self.create_function(JsFunction::Native(
            "[Symbol.toPrimitive]".to_string(),
            1,
            Rc::new(|interp, this, _args| {
                let Some(sym) = this_symbol_value(interp, this) else {
                    let err = interp.create_type_error("Symbol[Symbol.toPrimitive] requires a Symbol");
                    return Completion::Throw(err);
                };
                Completion::Normal(JsValue::Symbol(sym))
            }),
        ));
        // Get the @@toPrimitive well-known symbol key
        if let Some(sym_val) = self.global_env.borrow().get("Symbol")
            && let JsValue::Object(sym_obj) = &sym_val
            && let Some(sym_data) = self.get_object(sym_obj.id)
        {
            let to_prim_sym = sym_data.borrow().get_property("toPrimitive");
            if let JsValue::Symbol(s) = &to_prim_sym {
                let key = format!("Symbol({})", s.description.as_ref().map(|d| d.to_rust_string()).unwrap_or_default());
                proto.borrow_mut().insert_builtin(key, to_prim_fn);
            }
        }

        // [Symbol.toStringTag] = "Symbol"
        if let Some(sym_val) = self.global_env.borrow().get("Symbol")
            && let JsValue::Object(sym_obj) = &sym_val
            && let Some(sym_data) = self.get_object(sym_obj.id)
        {
            let tag_sym = sym_data.borrow().get_property("toStringTag");
            if let JsValue::Symbol(s) = &tag_sym {
                let key = format!("Symbol({})", s.description.as_ref().map(|d| d.to_rust_string()).unwrap_or_default());
                proto.borrow_mut().insert_property(
                    key,
                    PropertyDescriptor::data(
                        JsValue::String(JsString::from_str("Symbol")),
                        false,
                        false,
                        true,
                    ),
                );
            }
        }

        // Set Symbol.prototype on the Symbol constructor
        if let Some(sym_val) = self.global_env.borrow().get("Symbol")
            && let JsValue::Object(o) = &sym_val
            && let Some(sym_obj) = self.get_object(o.id)
        {
            let proto_val = JsValue::Object(crate::types::JsObject {
                id: proto.borrow().id.unwrap(),
            });
            sym_obj.borrow_mut().insert_value("prototype".to_string(), proto_val);
            // Set constructor on prototype
            let ctor_val = sym_val.clone();
            proto.borrow_mut().insert_builtin("constructor".to_string(), ctor_val);
        }

        self.symbol_prototype = Some(proto);
    }


    pub(crate) fn setup_number_prototype(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Number".to_string();
        proto.borrow_mut().primitive_value = Some(JsValue::Number(0.0));

        fn this_number_value(interp: &Interpreter, this: &JsValue) -> Option<f64> {
            match this {
                JsValue::Number(n) => Some(*n),
                JsValue::Object(o) => interp.get_object(o.id).and_then(|obj| {
                    let b = obj.borrow();
                    if b.class_name == "Number"
                        && let Some(JsValue::Number(n)) = &b.primitive_value
                    {
                        return Some(*n);
                    }
                    None
                }),
                _ => None,
            }
        }

        let methods: Vec<(
            &str,
            usize,
            Rc<dyn Fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion>,
        )> = vec![
            (
                "toString",
                1,
                Rc::new(|interp, this, args| {
                    let Some(n) = this_number_value(interp, this) else {
                        let err =
                            interp.create_type_error("Number.prototype.toString requires a Number");
                        return Completion::Throw(err);
                    };
                    let radix = args
                        .first()
                        .map(|v| {
                            if v.is_undefined() {
                                10
                            } else {
                                to_number(v) as u32
                            }
                        })
                        .unwrap_or(10);
                    if !(2..=36).contains(&radix) {
                        let err =
                            interp.create_error("RangeError", "radix must be between 2 and 36");
                        return Completion::Throw(err);
                    }
                    if radix == 10 {
                        Completion::Normal(JsValue::String(JsString::from_str(&to_js_string(
                            &JsValue::Number(n),
                        ))))
                    } else {
                        let s = format_radix(n as i64, radix);
                        Completion::Normal(JsValue::String(JsString::from_str(&s)))
                    }
                }),
            ),
            (
                "valueOf",
                0,
                Rc::new(|interp, this, _args| {
                    let Some(n) = this_number_value(interp, this) else {
                        let err =
                            interp.create_type_error("Number.prototype.valueOf requires a Number");
                        return Completion::Throw(err);
                    };
                    Completion::Normal(JsValue::Number(n))
                }),
            ),
            (
                "toFixed",
                1,
                Rc::new(|interp, this, args| {
                    let Some(n) = this_number_value(interp, this) else {
                        let err =
                            interp.create_type_error("Number.prototype.toFixed requires a Number");
                        return Completion::Throw(err);
                    };
                    let digits = args.first().map(|v| to_number(v) as usize).unwrap_or(0);
                    Completion::Normal(JsValue::String(JsString::from_str(&format!(
                        "{n:.digits$}"
                    ))))
                }),
            ),
            (
                "toExponential",
                1,
                Rc::new(|interp, this, args| {
                    let Some(n) = this_number_value(interp, this) else {
                        let err = interp
                            .create_type_error("Number.prototype.toExponential requires a Number");
                        return Completion::Throw(err);
                    };
                    let has_arg = args.first().is_some_and(|v| !v.is_undefined());
                    if has_arg {
                        let digits = to_number(args.first().unwrap()) as usize;
                        Completion::Normal(JsValue::String(JsString::from_str(&format!(
                            "{n:.digits$e}"
                        ))))
                    } else {
                        Completion::Normal(JsValue::String(JsString::from_str(&format!("{n:e}"))))
                    }
                }),
            ),
            (
                "toPrecision",
                1,
                Rc::new(|interp, this, args| {
                    let Some(n) = this_number_value(interp, this) else {
                        let err = interp
                            .create_type_error("Number.prototype.toPrecision requires a Number");
                        return Completion::Throw(err);
                    };
                    let has_arg = args.first().is_some_and(|v| !v.is_undefined());
                    if has_arg {
                        let precision = to_number(args.first().unwrap()) as usize;
                        Completion::Normal(JsValue::String(JsString::from_str(&format!(
                            "{n:.prec$}",
                            prec = precision.saturating_sub(1)
                        ))))
                    } else {
                        Completion::Normal(JsValue::String(JsString::from_str(&to_js_string(
                            &JsValue::Number(n),
                        ))))
                    }
                }),
            ),
            (
                "toLocaleString",
                0,
                Rc::new(|interp, this, _args| {
                    let Some(n) = this_number_value(interp, this) else {
                        let err = interp
                            .create_type_error("Number.prototype.toLocaleString requires a Number");
                        return Completion::Throw(err);
                    };
                    Completion::Normal(JsValue::String(JsString::from_str(&to_js_string(
                        &JsValue::Number(n),
                    ))))
                }),
            ),
        ];

        for (name, arity, func) in methods {
            let fn_val = self.create_function(JsFunction::Native(name.to_string(), arity, func));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
        }

        // Set Number.prototype on the Number constructor
        if let Some(num_val) = self.global_env.borrow().get("Number")
            && let JsValue::Object(o) = &num_val
            && let Some(num_obj) = self.get_object(o.id)
        {
            let proto_val = JsValue::Object(crate::types::JsObject {
                id: proto.borrow().id.unwrap(),
            });
            num_obj
                .borrow_mut()
                .insert_value("prototype".to_string(), proto_val);
        }

        self.number_prototype = Some(proto);
    }


    pub(crate) fn setup_boolean_prototype(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Boolean".to_string();
        proto.borrow_mut().primitive_value = Some(JsValue::Boolean(false));

        fn this_boolean_value(interp: &Interpreter, this: &JsValue) -> Option<bool> {
            match this {
                JsValue::Boolean(b) => Some(*b),
                JsValue::Object(o) => interp.get_object(o.id).and_then(|obj| {
                    let b = obj.borrow();
                    if b.class_name == "Boolean"
                        && let Some(JsValue::Boolean(v)) = &b.primitive_value
                    {
                        return Some(*v);
                    }
                    None
                }),
                _ => None,
            }
        }

        let methods: Vec<(
            &str,
            usize,
            Rc<dyn Fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion>,
        )> = vec![
            (
                "toString",
                0,
                Rc::new(|interp, this, _args| {
                    let Some(b) = this_boolean_value(interp, this) else {
                        let err = interp
                            .create_type_error("Boolean.prototype.toString requires a Boolean");
                        return Completion::Throw(err);
                    };
                    Completion::Normal(JsValue::String(JsString::from_str(if b {
                        "true"
                    } else {
                        "false"
                    })))
                }),
            ),
            (
                "valueOf",
                0,
                Rc::new(|interp, this, _args| {
                    let Some(b) = this_boolean_value(interp, this) else {
                        let err = interp
                            .create_type_error("Boolean.prototype.valueOf requires a Boolean");
                        return Completion::Throw(err);
                    };
                    Completion::Normal(JsValue::Boolean(b))
                }),
            ),
        ];

        for (name, arity, func) in methods {
            let fn_val = self.create_function(JsFunction::Native(name.to_string(), arity, func));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
        }

        // Set Boolean.prototype on the Boolean constructor
        if let Some(bool_val) = self.global_env.borrow().get("Boolean")
            && let JsValue::Object(o) = &bool_val
            && let Some(bool_obj) = self.get_object(o.id)
        {
            let proto_val = JsValue::Object(crate::types::JsObject {
                id: proto.borrow().id.unwrap(),
            });
            bool_obj
                .borrow_mut()
                .insert_value("prototype".to_string(), proto_val);
        }

        self.boolean_prototype = Some(proto);
    }

}
