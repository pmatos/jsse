pub(crate) mod array;
mod atomics;
pub(crate) mod bigint;
mod collections;
mod date;
mod disposable;
mod errors;
mod function;
mod function_constructors;
mod global_functions;
mod host;
mod intl;
mod iterators;
mod json;
mod math;
pub(crate) mod node_host;
mod number;
mod object;
mod promise;
mod proxy;
mod reflect;
pub(crate) mod regexp;
mod regexp_lookbehind;
pub(crate) mod string;
mod temporal;
pub(crate) mod typedarray;

use super::*;

fn is_identifier_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        None => return false,
        Some(c) => {
            if !unicode_ident::is_xid_start(c) && c != '$' && c != '_' {
                return false;
            }
        }
    }
    for c in chars {
        if !unicode_ident::is_xid_continue(c) && c != '$' && c != '\u{200C}' && c != '\u{200D}' {
            return false;
        }
    }
    true
}

fn sanitize_native_fn_name(name: &str) -> String {
    if name.is_empty() {
        return String::new();
    }
    // Handle "get X" / "set X" accessor prefix
    if let Some(rest) = name.strip_prefix("get ") {
        if is_identifier_name(rest) {
            return format!("get {rest}");
        }
        return "get ".to_string();
    }
    if let Some(rest) = name.strip_prefix("set ") {
        if is_identifier_name(rest) {
            return format!("set {rest}");
        }
        return "set ".to_string();
    }
    // Handle bracket notation like "[Symbol.iterator]"
    if name.starts_with('[') && name.ends_with(']') {
        return name.to_string();
    }
    if name.starts_with('[') {
        return String::new();
    }
    if is_identifier_name(name) {
        return name.to_string();
    }
    String::new()
}

impl Interpreter {
    pub(crate) fn setup_globals(&mut self) {
        // Create a placeholder Function.prototype early so all functions created during
        // setup get the correct [[Prototype]]. It will be adopted by Function.prototype later.
        if self.realm().function_prototype.is_none() {
            let fp_id = self.create_object_id();
            self.get_object_cell_expect(fp_id).borrow_mut().prototype_id =
                self.realm().object_prototype;
            self.get_object_cell_expect(fp_id).borrow_mut().callable =
                Some(JsFunction::native(String::new(), 0, |_, _, _| {
                    Completion::Normal(JsValue::UNDEFINED)
                }));
            self.get_object_cell_expect(fp_id).borrow_mut().class_name = "Function".to_string();
            self.realm_mut().function_prototype = Some(fp_id);
        }

        // Create %ThrowTypeError% intrinsic (§10.2.4) — must exist before anything uses it
        {
            let thrower = self.create_thrower_function();
            if let Some(thrower_id) = thrower.as_object_id()
                && let Some(obj) = self.get_object_cell(thrower_id)
            {
                let mut b = obj.borrow_mut();
                b.extensible = false;
                b.insert_property(
                    "length".to_string(),
                    PropertyDescriptor::data(JsValue::number(0.0), false, false, false),
                );
                b.insert_property(
                    "name".to_string(),
                    PropertyDescriptor::data(
                        JsValue::string(JsString::from_str("")),
                        false,
                        false,
                        false,
                    ),
                );
            }
            self.realm_mut().throw_type_error = Some(thrower);
        }

        self.setup_host_globals();

        self.setup_error_builtins();

        // Object constructor (minimal)
        self.register_global_fn(
            "Object",
            BindingKind::Var,
            JsFunction::constructor("Object".to_string(), 1, |interp, _this, args| {
                // §20.1.1 Object(value): Step 1 — if NewTarget is not undefined and is not
                // the active function (Object), return OrdinaryCreateFromConstructor(NewTarget, "%Object.prototype%")
                if let Some(ref nt) = interp.new_target.clone() {
                    // Check if new_target is different from the Object constructor itself
                    let object_fn = interp
                        .get_global_var("Object")
                        .unwrap_or(JsValue::UNDEFINED);
                    let nt_is_object = object_fn
                        .as_object_id()
                        .zip(nt.as_object_id())
                        .is_some_and(|(object_id, new_target_id)| object_id == new_target_id);
                    if !nt_is_object {
                        // OrdinaryCreateFromConstructor(NewTarget, "%Object.prototype%")
                        let default_proto = interp.realm().object_prototype;
                        let new_obj_rc_id = interp.create_object_id();
                        let no_id = new_obj_rc_id;
                        interp.apply_new_target_prototype(no_id, default_proto, |realm| {
                            realm.object_prototype
                        });
                        let new_obj_val = JsValue::object(no_id);
                        return Completion::Normal(new_obj_val);
                    }
                }
                match args.first() {
                    Some(val) if (val).is_object() => Completion::Normal(val.clone()),
                    Some(val) if !(val).is_nullish() => interp.to_object(val),
                    _ => {
                        let obj_id = interp.create_object_id();
                        let id = obj_id;
                        Completion::Normal(JsValue::object(id))
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
                let arr = if args.len() == 1
                    && let Some(n) = args[0].as_number()
                {
                    let len = n;
                    let uint32_len = len as u32;
                    if (uint32_len as f64) != len {
                        let err = interp.create_range_error("Invalid array length");
                        return Completion::Throw(err);
                    }
                    interp.create_array_with_length(uint32_len as usize)
                } else {
                    interp.create_array(args.to_vec())
                };
                if let Some(array_id) = arr.as_object_id() {
                    let default_proto_id = interp.realm().array_prototype;
                    interp.apply_new_target_prototype(array_id, default_proto_id, |realm| {
                        realm.array_prototype
                    });
                }
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

        self.setup_iterator_prototypes();
        self.setup_generator_prototype();
        self.setup_async_generator_prototype();
        self.setup_array_prototype();
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
        self.setup_string_prototype();

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

        self.setup_global_functions();

        self.setup_math();

        self.setup_function_constructors();

        // JSON object
        self.setup_json();

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

        // RegExp constructor and prototype
        self.setup_regexp();

        // Reflect and Proxy built-ins
        self.setup_reflect();
        self.setup_proxy();

        // TypedArray, ArrayBuffer, DataView built-ins
        self.setup_typedarray_builtins();

        // Atomics built-in
        self.setup_atomics();

        // Promise built-in
        self.setup_promise();

        // Temporal built-in
        self.setup_temporal();

        // Intl built-in
        self.setup_intl();

        // ShadowRealm built-in
        self.setup_shadow_realm();

        // globalThis - create a global object
        let global_obj_id = self.create_object_id();
        let global_val = JsValue::object(global_obj_id);
        self.realm()
            .global_env
            .borrow_mut()
            .declare("globalThis", BindingKind::Var);
        let env = self.realm().global_env.clone();
        let _ = self.env_set(&env, "globalThis", global_val.clone());
        self.realm().global_env.borrow_mut().bindings.insert(
            "this".to_string(),
            Binding {
                value: global_val,
                kind: BindingKind::Const,
                initialized: true,
                deletable: false,
            },
        );

        // Populate globalThis with built-in constructors and functions as
        // non-enumerable, writable, configurable properties (per spec §19.1)
        let global_names = [
            "Object",
            "Function",
            "Array",
            "String",
            "Number",
            "Boolean",
            "Symbol",
            "Error",
            "SyntaxError",
            "TypeError",
            "ReferenceError",
            "RangeError",
            "URIError",
            "EvalError",
            "Date",
            "RegExp",
            "Map",
            "Set",
            "WeakMap",
            "WeakSet",
            "WeakRef",
            "FinalizationRegistry",
            "Promise",
            "ArrayBuffer",
            "DataView",
            "JSON",
            "Math",
            "Reflect",
            "Proxy",
            "eval",
            "parseInt",
            "parseFloat",
            "isNaN",
            "isFinite",
            "encodeURI",
            "decodeURI",
            "encodeURIComponent",
            "decodeURIComponent",
            "NaN",
            "Infinity",
            "undefined",
            "Int8Array",
            "Uint8Array",
            "Uint8ClampedArray",
            "Int16Array",
            "Uint16Array",
            "Int32Array",
            "Uint32Array",
            "Float16Array",
            "Float32Array",
            "Float64Array",
            "BigInt64Array",
            "BigUint64Array",
            "BigInt",
            "AggregateError",
            "SharedArrayBuffer",
            "Atomics",
            "Temporal",
            "Intl",
            "setTimeout",
            "setInterval",
            "clearTimeout",
            "clearInterval",
            "escape",
            "unescape",
            "DisposableStack",
            "AsyncDisposableStack",
            "SuppressedError",
            "ShadowRealm",
            "Iterator",
        ];
        let vals: Vec<(String, JsValue)> = global_names
            .iter()
            .filter_map(|name| self.get_global_var(name).map(|v| (name.to_string(), v)))
            .collect();
        for (name, val) in vals {
            let (writable, configurable) = match name.as_str() {
                "NaN" | "Infinity" | "undefined" => (false, false),
                _ => (true, true),
            };
            self.get_object_cell_expect(global_obj_id)
                .borrow_mut()
                .insert_property(
                    name,
                    PropertyDescriptor::data(val, writable, false, configurable),
                );
        }
        // Also set globalThis on itself
        let gt_val = JsValue::object(global_obj_id);
        self.get_object_cell_expect(global_obj_id)
            .borrow_mut()
            .insert_property(
                "globalThis".to_string(),
                PropertyDescriptor::data(gt_val, true, false, true),
            );

        // Fix .prototype descriptors on built-in constructors.
        // create_function sets writable=true (correct for user-defined constructors per §10.2.5),
        // but built-in constructors need writable=false per their respective spec sections.
        let builtin_ctors = [
            "Object",
            "Function",
            "Array",
            "RegExp",
            "Promise",
            "Error",
            "TypeError",
            "RangeError",
            "SyntaxError",
            "ReferenceError",
            "URIError",
            "EvalError",
            "DataView",
            "ArrayBuffer",
            "SharedArrayBuffer",
            "WeakRef",
            "FinalizationRegistry",
            "ShadowRealm",
        ];
        let ctor_vals: Vec<JsValue> = builtin_ctors
            .iter()
            .filter_map(|name| self.get_global_var(name))
            .collect();
        for ctor_val in &ctor_vals {
            if let Some(ctor_id) = ctor_val.as_object_id()
                && let Some(ctor_obj) = self.get_object_cell(ctor_id)
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
        let gid = global_obj_id;
        self.realm().global_env.borrow_mut().global_object_id = Some(gid);
        self.realm_mut().global_object = Some(gid);

        // Per §9.1.1.4, built-in global names live on the global object (Object
        // Environment Record), not as declarative bindings. Remove the bootstrapping
        // bindings so that identifier resolution falls through to the global object.
        // Keep NaN/Infinity/undefined — they are non-writable/non-configurable
        // (ImmutableValue bindings that must shadow global object writes).
        {
            let mut env = self.realm().global_env.borrow_mut();
            for name in &global_names {
                match *name {
                    "NaN" | "Infinity" | "undefined" => continue,
                    _ => {
                        env.bindings.remove(*name);
                    }
                }
            }
        }

        // $262 test harness object (must be after global object is created)
        {
            let realm_id = self.current_realm_id;
            let dollar_262_val = self.create_dollar_262(realm_id);
            let global_env = self.realm().global_env.clone();
            global_env.borrow_mut().declare("$262", BindingKind::Var);
            let _ = self.env_set(&global_env, "$262", dollar_262_val);
        }
    }

    pub(crate) fn setup_shadow_realm(&mut self) {
        let my_realm_id = self.current_realm_id;
        let proto_id = self.create_object_id();
        let op = self.realm().object_prototype;
        {
            let mut p = self.get_object_cell_expect(proto_id).borrow_mut();
            p.class_name = "ShadowRealm".to_string();
            p.prototype_id = op;
        }

        // ShadowRealm.prototype[Symbol.toStringTag] = "ShadowRealm"
        self.define_to_string_tag(proto_id, "ShadowRealm");

        // ShadowRealm.prototype.evaluate
        let evaluate_fn = self.create_function(JsFunction::native(
            "evaluate".to_string(),
            1,
            move |interp, this, args| {
                let eval_realm_id = if let Some(shadow_realm_id) = this.as_object_id()
                    && let Some(obj) = interp.get_object_cell(shadow_realm_id)
                    && let Some(realm_id) = obj.borrow().shadow_realm_id()
                {
                    realm_id
                } else {
                    return Completion::Throw(interp.create_error_in_realm(
                        my_realm_id,
                        "TypeError",
                        "ShadowRealm.prototype.evaluate called on non-ShadowRealm",
                    ));
                };

                let source_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let source_text = if let Some(source) = source_val.as_string() {
                    source.to_string()
                } else {
                    return Completion::Throw(interp.create_error_in_realm(
                        my_realm_id,
                        "TypeError",
                        "ShadowRealm.prototype.evaluate: sourceText must be a string",
                    ));
                };

                interp.perform_realm_eval(&source_text, my_realm_id, eval_realm_id)
            },
        ));
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_builtin("evaluate".to_string(), evaluate_fn);

        // ShadowRealm.prototype.importValue
        let import_value_fn = self.create_function(JsFunction::native(
            "importValue".to_string(),
            2,
            move |interp, this, args| {
                let eval_realm_id = if let Some(shadow_realm_id) = this.as_object_id()
                    && let Some(obj) = interp.get_object_cell(shadow_realm_id)
                    && let Some(realm_id) = obj.borrow().shadow_realm_id()
                {
                    realm_id
                } else {
                    return Completion::Throw(interp.create_error_in_realm(
                        my_realm_id,
                        "TypeError",
                        "ShadowRealm.prototype.importValue called on non-ShadowRealm",
                    ));
                };

                let specifier_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let export_name_val = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);

                let specifier = match interp.to_string_value(&specifier_val) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };

                let export_name = if let Some(export_name) = export_name_val.as_string() {
                    export_name.to_string()
                } else {
                    return Completion::Throw(interp.create_error_in_realm(
                        my_realm_id,
                        "TypeError",
                        "ShadowRealm.prototype.importValue: exportName must be a string",
                    ));
                };

                let caller_realm_id = my_realm_id;
                let referrer = interp.current_module_path.clone();

                let old_realm = interp.current_realm_id;
                interp.current_realm_id = eval_realm_id;

                let module_path = match interp.resolve_module_specifier(
                    &specifier,
                    referrer.as_ref().and_then(ModuleKey::file_path),
                ) {
                    Ok(p) => p,
                    Err(_) => {
                        interp.current_realm_id = old_realm;
                        let err = interp.create_error_in_realm(
                            caller_realm_id,
                            "TypeError",
                            "ShadowRealm importValue: cannot resolve module",
                        );
                        return interp.create_rejected_promise(err);
                    }
                };

                let module = match interp.load_module_for_type(
                    &module_path,
                    None,
                    super::ModuleLoadMode::Evaluate,
                ) {
                    Ok(m) => m,
                    Err(_) => {
                        interp.current_realm_id = old_realm;
                        let err = interp.create_error_in_realm(
                            caller_realm_id,
                            "TypeError",
                            "ShadowRealm importValue: module load error",
                        );
                        return interp.create_rejected_promise(err);
                    }
                };
                let resolved_canon = module_path.canonicalize();
                let mut stack = vec![];
                if interp
                    .inner_module_evaluation(&resolved_canon, &mut stack, 0)
                    .is_err()
                {
                    interp.current_realm_id = old_realm;
                    let err = interp.create_error_in_realm(
                        caller_realm_id,
                        "TypeError",
                        "ShadowRealm importValue: module evaluation error",
                    );
                    return interp.create_rejected_promise(err);
                }
                interp.drain_microtasks();
                interp.current_realm_id = old_realm;

                // Get the named export
                let export_val = {
                    let m = module.borrow();
                    if let Some(v) = m.exports.get(&export_name) {
                        Some(v.clone())
                    } else {
                        // Also check export_bindings -> env
                        if let Some(binding_name) = m.export_bindings.get(&export_name) {
                            m.env.borrow().get(binding_name)
                        } else {
                            None
                        }
                    }
                };

                let export_val = match export_val {
                    Some(v) => v,
                    None => {
                        let err = interp.create_error_in_realm(
                            caller_realm_id,
                            "TypeError",
                            &format!(
                                "ShadowRealm importValue: export '{}' not found",
                                export_name
                            ),
                        );
                        return interp.create_rejected_promise(err);
                    }
                };

                match interp.get_wrapped_value(caller_realm_id, &export_val) {
                    Ok(wrapped) => interp.create_resolved_promise(wrapped),
                    Err(e) => interp.create_rejected_promise(e),
                }
            },
        ));
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_builtin("importValue".to_string(), import_value_fn);

        let proto_val = JsValue::object(proto_id);

        // ShadowRealm constructor
        let proto_val_for_ctor = proto_val.clone();
        let shadow_realm_ctor = self.create_function(JsFunction::constructor(
            "ShadowRealm".to_string(),
            0,
            move |interp, _this, _args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("ShadowRealm must be called with 'new'"),
                    );
                }

                let new_realm_id = interp.create_new_realm();

                let obj_id = interp.create_object_id();
                {
                    let mut o = interp.get_object_cell_expect(obj_id).borrow_mut();
                    o.class_name = "ShadowRealm".to_string();
                    o.kind = crate::interpreter::types::ObjectKind::ShadowRealm(new_realm_id);
                    if let Some(proto_id) = proto_val_for_ctor.as_object_id() {
                        o.prototype_id = Some(proto_id);
                    }
                }
                Completion::Normal(JsValue::object(obj_id))
            },
        ));

        // Set ShadowRealm.prototype on constructor
        if let Some(constructor_id) = shadow_realm_ctor.as_object_id()
            && let Some(ctor_obj) = self.get_object_cell(constructor_id)
        {
            ctor_obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val.clone(), false, false, false),
            );
        }

        // Set ShadowRealm.prototype.constructor = ShadowRealm
        if let Some(proto_id) = proto_val.as_object_id()
            && let Some(proto_obj) = self.get_object_cell(proto_id)
        {
            proto_obj
                .borrow_mut()
                .insert_builtin("constructor".to_string(), shadow_realm_ctor.clone());
        }

        // Store prototype in realm
        self.realm_mut().shadow_realm_prototype = Some(proto_id);

        // Register ShadowRealm as global
        let global_env = self.realm().global_env.clone();
        global_env
            .borrow_mut()
            .declare("ShadowRealm", BindingKind::Var);
        let _ = global_env
            .borrow_mut()
            .set("ShadowRealm", shadow_realm_ctor);
    }
}
