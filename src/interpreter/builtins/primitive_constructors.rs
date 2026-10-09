use super::super::*;

impl Interpreter {
    pub(super) fn setup_symbol_constructor(&mut self) {
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
                    let desc = if let Some(v) = args.first() {
                        if (v).is_undefined() {
                            None
                        } else {
                            match interp.to_string_value(v) {
                                Ok(s) => Some(JsString::from_str(&s)),
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                    } else {
                        None
                    };
                    let id = interp.next_symbol_id;
                    interp.next_symbol_id += 1;
                    Completion::Normal(JsValue::symbol(crate::types::JsSymbol::new(id, desc)))
                },
            ));
            if let Some(symbol_id) = symbol_fn.as_object_id()
                && let Some(obj) = self.get_object(symbol_id)
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
                    let sym_val = if let Some(existing) = self.well_known_symbols.get(name) {
                        JsValue::symbol(existing.clone())
                    } else {
                        let id = self.next_symbol_id;
                        self.next_symbol_id += 1;
                        let sym = crate::types::JsSymbol::new(id, Some(JsString::from_str(desc)));
                        self.well_known_symbols
                            .insert(name.to_string(), sym.clone());
                        JsValue::symbol(sym)
                    };
                    obj.borrow_mut().insert_property(
                        name.to_string(),
                        PropertyDescriptor::data(sym_val, false, false, false),
                    );
                }

                // Symbol.for
                let for_fn = self.create_function(JsFunction::Native(
                    "for".to_string(),
                    1,
                    Rc::new(|interp, _this, args| {
                        let key = if let Some(v) = args.first() {
                            match interp.to_string_value(v) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            }
                        } else {
                            "undefined".to_string()
                        };
                        if let Some(existing) = interp.global_symbol_registry.get(&key) {
                            return Completion::Normal(JsValue::symbol(existing.clone()));
                        }
                        let id = interp.next_symbol_id;
                        interp.next_symbol_id += 1;
                        let sym = crate::types::JsSymbol::new(id, Some(JsString::from_str(&key)));
                        interp.global_symbol_registry.insert(key, sym.clone());
                        Completion::Normal(JsValue::symbol(sym))
                    }),
                    false,
                ));
                obj.borrow_mut().insert_builtin("for".to_string(), for_fn);

                // Symbol.keyFor
                let key_for_fn = self.create_function(JsFunction::Native(
                    "keyFor".to_string(),
                    1,
                    Rc::new(|interp, _this, args| {
                        let Some(sym) = args.first().and_then(JsValue::as_symbol) else {
                            let err = interp
                                .create_type_error("Symbol.keyFor requires a symbol argument");
                            return Completion::Throw(err);
                        };
                        for (key, reg_sym) in &interp.global_symbol_registry {
                            if reg_sym.id() == sym.id() {
                                return Completion::Normal(JsValue::string(JsString::from_str(
                                    key,
                                )));
                            }
                        }
                        Completion::Normal(JsValue::UNDEFINED)
                    }),
                    false,
                ));
                obj.borrow_mut()
                    .insert_builtin("keyFor".to_string(), key_for_fn);
            }
            self.realm()
                .global_env
                .borrow_mut()
                .declare("Symbol", BindingKind::Var);
            let env = self.realm().global_env.clone();
            let _ = self.env_set(&env, "Symbol", symbol_fn);
        }
    }

    pub(super) fn setup_string_constructor(&mut self) {
        // String constructor/converter — must be before setup_string_prototype
        self.register_global_fn(
            "String",
            BindingKind::Var,
            JsFunction::constructor("String".to_string(), 1, |interp, this, args| {
                let js_str = if args.is_empty() {
                    JsString::from_str("")
                } else {
                    let val = &args[0];
                    // §22.1.1.1 step 2a: only when NewTarget is undefined AND value is Symbol
                    if interp.new_target.is_none()
                        && let Some(sym) = val.as_symbol()
                    {
                        let desc = if let Some(desc) = sym.description() {
                            format!("Symbol({desc})")
                        } else {
                            "Symbol()".to_string()
                        };
                        JsString::from_str(&desc)
                    } else if let Some(s) = val.as_string() {
                        s
                    } else {
                        // §22.1.1.1 step 2b: ToString(value) — throws TypeError for Symbol
                        match interp.to_string_value(val) {
                            Ok(s) => JsString::from_str(&s),
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                };
                // §22.1.1.1 step 3: only box into `this` when invoked via [[Construct]].
                // A plain [[Call]] (e.g. `String.call(obj, x)`) must not mutate `this`.
                if interp.new_target.is_some()
                    && let Some(this_id) = this.as_object_id()
                    && let Some(obj) = interp.get_object(this_id)
                {
                    let proto = match interp
                        .get_prototype_from_new_target_realm(|realm| realm.string_prototype)
                    {
                        Ok(p) => p,
                        Err(e) => return Completion::Throw(e),
                    };
                    if let Some(proto_rc) = proto {
                        obj.borrow_mut().prototype_id = Some(proto_rc);
                    }
                    obj.borrow_mut().primitive_value = Some(JsValue::string(js_str.clone()));
                    obj.borrow_mut().class_name = "String".to_string();
                }
                Completion::Normal(JsValue::string(js_str))
            }),
        );
    }

    pub(super) fn setup_string_raw(&mut self) {
        // String.raw
        {
            let raw_fn = self.create_function(JsFunction::native(
                "raw".to_string(),
                1,
                |interp, _this, args| {
                    let template = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let template_obj = match interp.to_object(&template) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let raw_val = if let Some(template_id) = template_obj.as_object_id() {
                        match interp.get_object_property(template_id, "raw", &template_obj) {
                            Completion::Normal(v) => v,
                            other => return other,
                        }
                    } else {
                        JsValue::UNDEFINED
                    };
                    let raw_obj = match interp.to_object(&raw_val) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let len = if let Some(raw_id) = raw_obj.as_object_id() {
                        let length_val =
                            match interp.get_object_property(raw_id, "length", &raw_obj) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                other => return other,
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
                        return Completion::Normal(JsValue::string(JsString::from_str("")));
                    }
                    let subs = &args[1..];
                    let mut result = String::new();
                    for i in 0..len {
                        let next_seg = if let Some(raw_id) = raw_obj.as_object_id() {
                            match interp.get_object_property(raw_id, &i.to_string(), &raw_obj) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                other => return other,
                            }
                        } else {
                            JsValue::UNDEFINED
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
                    Completion::Normal(JsValue::string(JsString::from_str(&result)))
                },
            ));
            if let Some(string_ctor) = self.get_global_var("String")
                && let Some(string_ctor_id) = string_ctor.as_object_id()
                && let Some(obj) = self.get_object(string_ctor_id)
            {
                obj.borrow_mut().insert_builtin("raw".to_string(), raw_fn);
            }
        }
    }

    pub(super) fn setup_number_constructor(&mut self) {
        // Number constructor/converter
        self.register_global_fn(
            "Number",
            BindingKind::Var,
            JsFunction::constructor("Number".to_string(), 1, |interp, this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::number(0.0));
                let n = if let Some(b) = val.as_bigint() {
                    let s = b.value.to_string();
                    s.parse::<f64>().unwrap_or(f64::INFINITY)
                } else {
                    match interp.to_number_value(&val) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    }
                };
                // §21.1.1.1 step 3: only box into `this` when invoked via [[Construct]].
                // A plain [[Call]] (e.g. `Number.call(obj, x)`) must not mutate `this`.
                if interp.new_target.is_some()
                    && let Some(this_id) = this.as_object_id()
                    && let Some(obj) = interp.get_object(this_id)
                {
                    // OrdinaryCreateFromConstructor — realm-aware prototype
                    let proto = match interp
                        .get_prototype_from_new_target_realm(|realm| realm.number_prototype)
                    {
                        Ok(p) => p,
                        Err(e) => return Completion::Throw(e),
                    };
                    let mut b = obj.borrow_mut();
                    b.primitive_value = Some(JsValue::number(n));
                    b.class_name = "Number".to_string();
                    if let Some(p) = proto {
                        b.prototype_id = Some(p);
                    }
                }
                Completion::Normal(JsValue::number(n))
            }),
        );
    }

    pub(super) fn setup_number_statics(&mut self) {
        // Number static properties
        {
            let is_finite_fn = self.create_function(JsFunction::native(
                "isFinite".to_string(),
                1,
                |_interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let result = val.as_number().is_some_and(f64::is_finite);
                    Completion::Normal(JsValue::boolean(result))
                },
            ));
            let is_nan_fn = self.create_function(JsFunction::native(
                "isNaN".to_string(),
                1,
                |_interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let result = val.as_number().is_some_and(f64::is_nan);
                    Completion::Normal(JsValue::boolean(result))
                },
            ));
            let is_integer_fn = self.create_function(JsFunction::native(
                "isInteger".to_string(),
                1,
                |_interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let result = if let Some(n) = val.as_number() {
                        n.is_finite() && n == n.trunc()
                    } else {
                        false
                    };
                    Completion::Normal(JsValue::boolean(result))
                },
            ));
            let is_safe_fn = self.create_function(JsFunction::native(
                "isSafeInteger".to_string(),
                1,
                |_interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let result = if let Some(n) = val.as_number() {
                        n.is_finite() && n == n.trunc() && n.abs() <= 9007199254740991.0
                    } else {
                        false
                    };
                    Completion::Normal(JsValue::boolean(result))
                },
            ));
            if let Some(num_val) = self.get_global_var("Number")
                && let Some(number_id) = num_val.as_object_id()
                && let Some(num_obj) = self.get_object(number_id)
            {
                let mut n = num_obj.borrow_mut();
                n.insert_property(
                    "POSITIVE_INFINITY".to_string(),
                    PropertyDescriptor::data(JsValue::number(f64::INFINITY), false, false, false),
                );
                n.insert_property(
                    "NEGATIVE_INFINITY".to_string(),
                    PropertyDescriptor::data(
                        JsValue::number(f64::NEG_INFINITY),
                        false,
                        false,
                        false,
                    ),
                );
                n.insert_property(
                    "MAX_VALUE".to_string(),
                    PropertyDescriptor::data(JsValue::number(f64::MAX), false, false, false),
                );
                n.insert_property(
                    "MIN_VALUE".to_string(),
                    PropertyDescriptor::data(JsValue::number(5e-324_f64), false, false, false),
                );
                n.insert_property(
                    "NaN".to_string(),
                    PropertyDescriptor::data(JsValue::number(f64::NAN), false, false, false),
                );
                n.insert_property(
                    "EPSILON".to_string(),
                    PropertyDescriptor::data(JsValue::number(f64::EPSILON), false, false, false),
                );
                n.insert_property(
                    "MAX_SAFE_INTEGER".to_string(),
                    PropertyDescriptor::data(
                        JsValue::number(9007199254740991.0),
                        false,
                        false,
                        false,
                    ),
                );
                n.insert_property(
                    "MIN_SAFE_INTEGER".to_string(),
                    PropertyDescriptor::data(
                        JsValue::number(-9007199254740991.0),
                        false,
                        false,
                        false,
                    ),
                );
                n.insert_builtin("isFinite".to_string(), is_finite_fn);
                n.insert_builtin("isNaN".to_string(), is_nan_fn);
                n.insert_builtin("isInteger".to_string(), is_integer_fn);
                n.insert_builtin("isSafeInteger".to_string(), is_safe_fn);
            }
        }
    }

    pub(super) fn setup_boolean_constructor(&mut self) {
        // Boolean constructor/converter
        self.register_global_fn(
            "Boolean",
            BindingKind::Var,
            JsFunction::constructor("Boolean".to_string(), 1, |interp, this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let b = interp.to_boolean_val(&val);
                // §21.3.1.1 step 2: only box into `this` when invoked via [[Construct]].
                // A plain [[Call]] (e.g. `Boolean.call(obj, x)`) must not mutate `this`.
                if interp.new_target.is_some()
                    && let Some(this_id) = this.as_object_id()
                    && let Some(obj) = interp.get_object(this_id)
                {
                    // OrdinaryCreateFromConstructor — realm-aware prototype
                    let proto = match interp
                        .get_prototype_from_new_target_realm(|realm| realm.boolean_prototype)
                    {
                        Ok(p) => p,
                        Err(e) => return Completion::Throw(e),
                    };
                    let mut bo = obj.borrow_mut();
                    bo.primitive_value = Some(JsValue::boolean(b));
                    bo.class_name = "Boolean".to_string();
                    if let Some(p) = proto {
                        bo.prototype_id = Some(p);
                    }
                }
                Completion::Normal(JsValue::boolean(b))
            }),
        );
    }

    pub(super) fn setup_string_from_char_code(&mut self) {
        // String.fromCharCode
        {
            let string_ctor = self.get_global_var("String");
            if let Some(string_ctor_id) = string_ctor.and_then(|v| v.as_object_id()) {
                let from_char_code = self.create_function(JsFunction::native(
                    "fromCharCode".to_string(),
                    1,
                    |interp, _this, args: &[JsValue]| {
                        let mut code_units: Vec<u16> = Vec::with_capacity(args.len());
                        for a in args {
                            // ToUint16 calls ToNumber which throws for Symbol/BigInt
                            let n = match interp.to_number_value(a) {
                                Ok(n) => n,
                                Err(e) => return Completion::Throw(e),
                            };
                            // ToUint16 (§7.1.7): NaN/±0/±Infinity → 0; otherwise modulo 2^16
                            let cu = if n.is_nan() || n == 0.0 || n.is_infinite() {
                                0u16
                            } else {
                                // floor(abs(n)) mod 2^16, with sign handling
                                let int = n.abs().floor();
                                let int16bit = (int % 65536.0) as u32;
                                if n < 0.0 && int16bit != 0 {
                                    (65536 - int16bit) as u16
                                } else {
                                    int16bit as u16
                                }
                            };
                            code_units.push(cu);
                        }
                        Completion::Normal(JsValue::string(JsString::from_vec(code_units)))
                    },
                ));
                let from_code_point = self.create_function(JsFunction::native(
                    "fromCodePoint".to_string(),
                    1,
                    |interp, _this, args: &[JsValue]| {
                        let mut code_units: Vec<u16> = Vec::new();
                        for a in args {
                            // ToNumber throws for Symbol/BigInt
                            let next_cp = match interp.to_number_value(a) {
                                Ok(n) => n,
                                Err(e) => return Completion::Throw(e),
                            };
                            // Not an integral Number → RangeError
                            if next_cp != next_cp.trunc() || next_cp.is_infinite() {
                                return Completion::Throw(
                                    interp.create_range_error(&format!(
                                        "Invalid code point {next_cp}"
                                    )),
                                );
                            }
                            let cp = next_cp as i64;
                            if !(0..=0x10FFFF).contains(&cp) {
                                return Completion::Throw(
                                    interp.create_range_error(&format!("Invalid code point {cp}")),
                                );
                            }
                            let cp = cp as u32;
                            if let Some(c) = char::from_u32(cp) {
                                let mut buf = [0u16; 2];
                                let encoded = c.encode_utf16(&mut buf);
                                code_units.extend_from_slice(encoded);
                            } else {
                                // Lone surrogate: push directly as a code unit
                                code_units.push(cp as u16);
                            }
                        }
                        Completion::Normal(JsValue::string(JsString::from_vec(code_units)))
                    },
                ));
                if let Some(obj) = self.get_object_cell(string_ctor_id) {
                    obj.borrow_mut()
                        .insert_builtin("fromCharCode".to_string(), from_char_code);
                    obj.borrow_mut()
                        .insert_builtin("fromCodePoint".to_string(), from_code_point);
                }
            }
        }
    }
}
