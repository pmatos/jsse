use super::super::*;

fn format_number_radix(n: f64, radix: u32) -> String {
    let negative = n < 0.0;
    let x = n.abs();
    let int_part = x.trunc() as i64;
    let frac_part = x - (int_part as f64);

    let mut result = format_radix(int_part, radix);

    if frac_part != 0.0 {
        result.push('.');
        let mut frac = frac_part;
        // Limit to ~20 digits to avoid infinite loops
        for _ in 0..20 {
            frac *= radix as f64;
            let digit = frac.trunc() as u32;
            result.push(char::from_digit(digit, radix).unwrap_or('0'));
            frac -= digit as f64;
            if frac < 1e-10 {
                break;
            }
        }
    }

    if negative {
        format!("-{result}")
    } else {
        result
    }
}

fn format_exponential(n: f64, fraction_digits: Option<usize>) -> String {
    let negative = n < 0.0;
    let x = n.abs();
    if x == 0.0 {
        let sign = if negative { "-" } else { "" };
        return match fraction_digits {
            Some(f) if f > 0 => format!("{sign}0.{}e+0", "0".repeat(f)),
            _ => format!("{sign}0e+0"),
        };
    }

    let e = x.log10().floor() as i32;
    let result = match fraction_digits {
        Some(f) => {
            let scaled = x / 10f64.powi(e);
            let formatted = format!("{scaled:.f$}");
            // If rounding pushed us to 10.xxx, adjust
            let parsed: f64 = formatted.parse().unwrap_or(scaled);
            if parsed >= 10.0 {
                let scaled2 = x / 10f64.powi(e + 1);
                let formatted2 = format!("{scaled2:.f$}");
                let exp = e + 1;
                let exp_sign = if exp >= 0 { "+" } else { "" };
                format!("{formatted2}e{exp_sign}{exp}")
            } else {
                let exp_sign = if e >= 0 { "+" } else { "" };
                format!("{formatted}e{exp_sign}{e}")
            }
        }
        None => {
            let scaled = x / 10f64.powi(e);
            // Use enough precision then strip trailing zeros
            let formatted = format!("{scaled:.20}");
            let trimmed = formatted.trim_end_matches('0');
            let trimmed = trimmed.trim_end_matches('.');
            let exp_sign = if e >= 0 { "+" } else { "" };
            format!("{trimmed}e{exp_sign}{e}")
        }
    };

    if negative {
        format!("-{result}")
    } else {
        result
    }
}

fn format_precision(n: f64, precision: usize) -> String {
    let negative = n < 0.0;
    let x = n.abs();

    if x == 0.0 {
        let sign = if negative { "-" } else { "" };
        if precision == 1 {
            return format!("{sign}0");
        }
        return format!("{sign}0.{}", "0".repeat(precision - 1));
    }

    let e = x.log10().floor() as i32;

    if e < -6 || e >= precision as i32 {
        // Exponential notation
        let frac_digits = precision - 1;
        let scaled = x / 10f64.powi(e);
        let formatted = format!("{scaled:.frac_digits$}");
        let parsed: f64 = formatted.parse().unwrap_or(scaled);
        let (formatted, exp) = if parsed >= 10.0 {
            let scaled2 = x / 10f64.powi(e + 1);
            (format!("{scaled2:.frac_digits$}"), e + 1)
        } else {
            (formatted, e)
        };
        // Strip trailing zeros after decimal if needed - actually spec says keep them
        let exp_sign = if exp >= 0 { "+" } else { "" };
        let result = format!("{formatted}e{exp_sign}{exp}");
        if negative {
            format!("-{result}")
        } else {
            result
        }
    } else {
        // Fixed notation
        let frac_digits = if precision as i32 > e + 1 {
            (precision as i32 - e - 1) as usize
        } else {
            0
        };
        let formatted = format!("{x:.frac_digits$}");
        if negative {
            format!("-{formatted}")
        } else {
            formatted
        }
    }
}

impl Interpreter {
    pub(crate) fn setup_symbol_prototype(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Symbol".to_string();

        fn this_symbol_value(
            interp: &Interpreter,
            this: &JsValue,
        ) -> Option<crate::types::JsSymbol> {
            match this {
                JsValue::Symbol(s) => Some(s.clone()),
                JsValue::Object(o) => interp.get_object(o.id).and_then(|obj| {
                    let b = obj.borrow();
                    if b.class_name == "Symbol"
                        && let Some(JsValue::Symbol(s)) = &b.primitive_value
                    {
                        return Some(s.clone());
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
                        let err =
                            interp.create_type_error("Symbol.prototype.toString requires a Symbol");
                        return Completion::Throw(err);
                    };
                    let desc = sym
                        .description
                        .as_ref()
                        .map(|d| d.to_rust_string())
                        .unwrap_or_default();
                    Completion::Normal(JsValue::String(JsString::from_str(&format!(
                        "Symbol({desc})"
                    ))))
                }),
            ),
            (
                "valueOf",
                0,
                Rc::new(|interp, this, _args| {
                    let Some(sym) = this_symbol_value(interp, this) else {
                        let err =
                            interp.create_type_error("Symbol.prototype.valueOf requires a Symbol");
                        return Completion::Throw(err);
                    };
                    Completion::Normal(JsValue::Symbol(sym))
                }),
            ),
        ];

        for (name, arity, func) in methods {
            let fn_val = self.create_function(JsFunction::Native(name.to_string(), arity, func, false));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
        }

        // description getter
        let desc_getter = self.create_function(JsFunction::Native(
            "get description".to_string(),
            0,
            Rc::new(|interp, this, _args| {
                let Some(sym) = this_symbol_value(interp, this) else {
                    let err =
                        interp.create_type_error("Symbol.prototype.description requires a Symbol");
                    return Completion::Throw(err);
                };
                match sym.description {
                    Some(d) => Completion::Normal(JsValue::String(d)),
                    None => Completion::Normal(JsValue::Undefined),
                }
            }),
            false,
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
                    let err =
                        interp.create_type_error("Symbol[Symbol.toPrimitive] requires a Symbol");
                    return Completion::Throw(err);
                };
                Completion::Normal(JsValue::Symbol(sym))
            }),
            false,
        ));
        // Get the @@toPrimitive well-known symbol key
        if let Some(sym_val) = self.global_env.borrow().get("Symbol")
            && let JsValue::Object(sym_obj) = &sym_val
            && let Some(sym_data) = self.get_object(sym_obj.id)
        {
            let to_prim_sym = sym_data.borrow().get_property("toPrimitive");
            if let JsValue::Symbol(s) = &to_prim_sym {
                let key = format!(
                    "Symbol({})",
                    s.description
                        .as_ref()
                        .map(|d| d.to_rust_string())
                        .unwrap_or_default()
                );
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
                let key = format!(
                    "Symbol({})",
                    s.description
                        .as_ref()
                        .map(|d| d.to_rust_string())
                        .unwrap_or_default()
                );
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
            sym_obj
                .borrow_mut()
                .insert_value("prototype".to_string(), proto_val);
            // Set constructor on prototype
            let ctor_val = sym_val.clone();
            proto
                .borrow_mut()
                .insert_builtin("constructor".to_string(), ctor_val);
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
                    } else if n.is_nan() {
                        Completion::Normal(JsValue::String(JsString::from_str("NaN")))
                    } else if n.is_infinite() {
                        Completion::Normal(JsValue::String(JsString::from_str(if n > 0.0 {
                            "Infinity"
                        } else {
                            "-Infinity"
                        })))
                    } else if n == 0.0 {
                        Completion::Normal(JsValue::String(JsString::from_str("0")))
                    } else {
                        let s = format_number_radix(n, radix);
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
                    let f_raw = args.first().map(to_number).unwrap_or(0.0);
                    let f = to_integer_or_infinity(f_raw);
                    if !(0.0..=100.0).contains(&f) {
                        let err = interp.create_error(
                            "RangeError",
                            "toFixed() digits argument must be between 0 and 100",
                        );
                        return Completion::Throw(err);
                    }
                    let digits = f as usize;
                    if n.is_nan() {
                        return Completion::Normal(JsValue::String(JsString::from_str("NaN")));
                    }
                    if n.is_infinite() {
                        return Completion::Normal(JsValue::String(JsString::from_str(
                            if n > 0.0 { "Infinity" } else { "-Infinity" },
                        )));
                    }
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
                        let f_raw = to_number(args.first().unwrap());
                        let f = to_integer_or_infinity(f_raw);
                        if n.is_nan() {
                            return Completion::Normal(JsValue::String(JsString::from_str("NaN")));
                        }
                        if n.is_infinite() {
                            return Completion::Normal(JsValue::String(JsString::from_str(
                                if n > 0.0 { "Infinity" } else { "-Infinity" },
                            )));
                        }
                        if !(0.0..=100.0).contains(&f) {
                            let err = interp.create_error(
                                "RangeError",
                                "toExponential() argument must be between 0 and 100",
                            );
                            return Completion::Throw(err);
                        }
                        let digits = f as usize;
                        let result = format_exponential(n, Some(digits));
                        Completion::Normal(JsValue::String(JsString::from_str(&result)))
                    } else {
                        if n.is_nan() {
                            return Completion::Normal(JsValue::String(JsString::from_str("NaN")));
                        }
                        if n.is_infinite() {
                            return Completion::Normal(JsValue::String(JsString::from_str(
                                if n > 0.0 { "Infinity" } else { "-Infinity" },
                            )));
                        }
                        let result = format_exponential(n, None);
                        Completion::Normal(JsValue::String(JsString::from_str(&result)))
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
                    if !has_arg {
                        return Completion::Normal(JsValue::String(JsString::from_str(
                            &to_js_string(&JsValue::Number(n)),
                        )));
                    }
                    let p_raw = to_number(args.first().unwrap());
                    let p = to_integer_or_infinity(p_raw);
                    if n.is_nan() {
                        return Completion::Normal(JsValue::String(JsString::from_str("NaN")));
                    }
                    if n.is_infinite() {
                        return Completion::Normal(JsValue::String(JsString::from_str(
                            if n > 0.0 { "Infinity" } else { "-Infinity" },
                        )));
                    }
                    if !(1.0..=100.0).contains(&p) {
                        let err = interp.create_error(
                            "RangeError",
                            "toPrecision() argument must be between 1 and 100",
                        );
                        return Completion::Throw(err);
                    }
                    let precision = p as usize;
                    let result = format_precision(n, precision);
                    Completion::Normal(JsValue::String(JsString::from_str(&result)))
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
            let fn_val = self.create_function(JsFunction::Native(name.to_string(), arity, func, false));
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
            let fn_val = self.create_function(JsFunction::Native(name.to_string(), arity, func, false));
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
