use super::super::*;

impl Interpreter {
    pub(crate) fn setup_bigint_prototype(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "BigInt".to_string();
        proto.borrow_mut().primitive_value = Some(JsValue::BigInt(JsBigInt {
            value: num_bigint::BigInt::from(0),
        }));

        fn this_bigint_value(interp: &Interpreter, this: &JsValue) -> Option<num_bigint::BigInt> {
            match this {
                JsValue::BigInt(b) => Some(b.value.clone()),
                JsValue::Object(o) => interp.get_object(o.id).and_then(|obj| {
                    let b = obj.borrow();
                    if b.class_name == "BigInt"
                        && let Some(JsValue::BigInt(bi)) = &b.primitive_value
                    {
                        return Some(bi.value.clone());
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
                    let Some(n) = this_bigint_value(interp, this) else {
                        return Completion::Throw(
                            interp.create_type_error("BigInt.prototype.toString requires a BigInt"),
                        );
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
                        return Completion::Throw(
                            interp.create_error("RangeError", "radix must be between 2 and 36"),
                        );
                    }
                    let s = if radix == 10 {
                        n.to_string()
                    } else {
                        bigint_to_string_radix(&n, radix)
                    };
                    Completion::Normal(JsValue::String(JsString::from_str(&s)))
                }),
            ),
            (
                "valueOf",
                0,
                Rc::new(|interp, this, _args| {
                    let Some(n) = this_bigint_value(interp, this) else {
                        return Completion::Throw(
                            interp.create_type_error("BigInt.prototype.valueOf requires a BigInt"),
                        );
                    };
                    Completion::Normal(JsValue::BigInt(JsBigInt { value: n }))
                }),
            ),
            (
                "toLocaleString",
                0,
                Rc::new(|interp, this, _args| {
                    let Some(n) = this_bigint_value(interp, this) else {
                        return Completion::Throw(interp.create_type_error(
                            "BigInt.prototype.toLocaleString requires a BigInt",
                        ));
                    };
                    Completion::Normal(JsValue::String(JsString::from_str(&n.to_string())))
                }),
            ),
        ];

        for (name, arity, func) in methods {
            let fn_val =
                self.create_function(JsFunction::Native(name.to_string(), arity, func, false));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
        }

        // @@toStringTag
        let tag_key = self
            .get_symbol_key("toStringTag")
            .unwrap_or_else(|| "Symbol(Symbol.toStringTag)".to_string());
        proto.borrow_mut().insert_property(
            tag_key,
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str("BigInt")),
                false,
                false,
                true,
            ),
        );

        // BigInt() function (NOT a constructor)
        self.register_global_fn(
            "BigInt",
            BindingKind::Var,
            JsFunction::native("BigInt".to_string(), 1, |interp, _this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                match &val {
                    JsValue::BigInt(_) => Completion::Normal(val),
                    JsValue::Boolean(b) => Completion::Normal(JsValue::BigInt(JsBigInt {
                        value: num_bigint::BigInt::from(if *b { 1 } else { 0 }),
                    })),
                    JsValue::Number(n) => {
                        if n.is_nan() || n.is_infinite() || *n != n.trunc() {
                            return Completion::Throw(interp.create_error(
                                "RangeError",
                                &format!("The number {n} cannot be converted to a BigInt because it is not an integer"),
                            ));
                        }
                        Completion::Normal(JsValue::BigInt(JsBigInt {
                            value: num_bigint::BigInt::from(*n as i64),
                        }))
                    }
                    JsValue::String(s) => {
                        let text = s.to_rust_string().trim().to_string();
                        if text.is_empty() {
                            return Completion::Throw(interp.create_error(
                                "SyntaxError",
                                "Cannot convert  to a BigInt",
                            ));
                        }
                        let parsed = if let Some(hex) =
                            text.strip_prefix("0x").or_else(|| text.strip_prefix("0X"))
                        {
                            num_bigint::BigInt::parse_bytes(hex.as_bytes(), 16)
                        } else if let Some(oct) =
                            text.strip_prefix("0o").or_else(|| text.strip_prefix("0O"))
                        {
                            num_bigint::BigInt::parse_bytes(oct.as_bytes(), 8)
                        } else if let Some(bin) =
                            text.strip_prefix("0b").or_else(|| text.strip_prefix("0B"))
                        {
                            num_bigint::BigInt::parse_bytes(bin.as_bytes(), 2)
                        } else {
                            text.parse::<num_bigint::BigInt>().ok()
                        };
                        match parsed {
                            Some(v) => Completion::Normal(JsValue::BigInt(JsBigInt { value: v })),
                            None => Completion::Throw(interp.create_error(
                                "SyntaxError",
                                &format!("Cannot convert {text} to a BigInt"),
                            )),
                        }
                    }
                    JsValue::Object(_) => {
                        let prim = match interp.to_primitive(&val, "number") {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };
                        let args = [prim];
                        let bigint_fn = interp.global_env.borrow().get("BigInt");
                        if let Some(bigint_fn) = bigint_fn {
                            return interp.call_function(&bigint_fn, &JsValue::Undefined, &args);
                        }
                        Completion::Throw(
                            interp.create_type_error("Cannot convert value to a BigInt"),
                        )
                    }
                    _ => Completion::Throw(
                        interp.create_type_error("Cannot convert value to a BigInt"),
                    ),
                }
            }),
        );

        // Create static methods first to avoid borrow conflicts
        let as_int_n = self.create_function(JsFunction::native(
            "asIntN".to_string(),
            2,
            |interp, _this, args| {
                let bits = to_number(args.first().unwrap_or(&JsValue::Undefined)) as u64;
                let bigint_val = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let JsValue::BigInt(ref b) = bigint_val else {
                    return Completion::Throw(
                        interp.create_type_error("Cannot convert value to a BigInt"),
                    );
                };
                if bits == 0 {
                    return Completion::Normal(JsValue::BigInt(JsBigInt {
                        value: num_bigint::BigInt::from(0),
                    }));
                }
                let modulus = num_bigint::BigInt::from(1) << bits;
                let mut result = &b.value % &modulus;
                if result < num_bigint::BigInt::from(0) {
                    result += &modulus;
                }
                let half = &modulus >> 1;
                if result >= half {
                    result -= modulus;
                }
                Completion::Normal(JsValue::BigInt(JsBigInt { value: result }))
            },
        ));
        let as_uint_n = self.create_function(JsFunction::native(
            "asUintN".to_string(),
            2,
            |interp, _this, args| {
                let bits = to_number(args.first().unwrap_or(&JsValue::Undefined)) as u64;
                let bigint_val = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let JsValue::BigInt(ref b) = bigint_val else {
                    return Completion::Throw(
                        interp.create_type_error("Cannot convert value to a BigInt"),
                    );
                };
                if bits == 0 {
                    return Completion::Normal(JsValue::BigInt(JsBigInt {
                        value: num_bigint::BigInt::from(0),
                    }));
                }
                let modulus = num_bigint::BigInt::from(1) << bits;
                let mut result = &b.value % &modulus;
                if result < num_bigint::BigInt::from(0) {
                    result += &modulus;
                }
                Completion::Normal(JsValue::BigInt(JsBigInt { value: result }))
            },
        ));

        if let Some(bigint_val) = self.global_env.borrow().get("BigInt")
            && let JsValue::Object(o) = &bigint_val
            && let Some(bigint_obj) = self.get_object(o.id)
        {
            let proto_val = JsValue::Object(crate::types::JsObject {
                id: proto.borrow().id.unwrap(),
            });
            bigint_obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val, false, false, false),
            );
            bigint_obj
                .borrow_mut()
                .insert_builtin("asIntN".to_string(), as_int_n);
            bigint_obj
                .borrow_mut()
                .insert_builtin("asUintN".to_string(), as_uint_n);
        }

        // Set constructor property on prototype
        if let Some(bigint_val) = self.global_env.borrow().get("BigInt") {
            proto.borrow_mut().insert_property(
                "constructor".to_string(),
                PropertyDescriptor::data(bigint_val, true, false, true),
            );
        }

        self.bigint_prototype = Some(proto);
    }
}

fn bigint_to_string_radix(n: &num_bigint::BigInt, radix: u32) -> String {
    use num_bigint::Sign;
    let (sign, digits) = n.to_radix_be(radix);
    if digits.is_empty() || (digits.len() == 1 && digits[0] == 0) {
        return "0".to_string();
    }
    let mut s = String::new();
    if sign == Sign::Minus {
        s.push('-');
    }
    for d in digits {
        s.push(char::from_digit(d as u32, radix).unwrap_or('?'));
    }
    s
}
