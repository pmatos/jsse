use super::*;
use crate::interpreter::builtins::temporal::{
    coerce_rounding_increment, get_options_object, get_prop, is_undefined, iso_time_valid,
    nanoseconds_to_time, parse_overflow_option, parse_temporal_time_string,
    temporal_unit_length_ns, temporal_unit_order, temporal_unit_singular, time_to_nanoseconds,
    validate_rounding_increment_raw,
};

impl Interpreter {
    pub(crate) fn setup_temporal_plain_time(&mut self, temporal_obj: &Rc<RefCell<JsObjectData>>) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Temporal.PlainTime".to_string();
        {
            let key = "Symbol(Symbol.toStringTag)".to_string();
            let desc = PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("Temporal.PlainTime"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            };
            proto.borrow_mut().property_order.push(key.clone());
            proto.borrow_mut().properties.insert(key, desc);
        }

        // Getters
        for &(name, field_idx) in &[
            ("hour", 0u8),
            ("minute", 1),
            ("second", 2),
            ("millisecond", 3),
            ("microsecond", 4),
            ("nanosecond", 5),
        ] {
            let getter = self.create_function(JsFunction::native(
                format!("get {name}"),
                0,
                move |interp, this, _args| {
                    let fields = match get_plain_time_fields(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let val = match field_idx {
                        0 => fields.0 as f64,
                        1 => fields.1 as f64,
                        2 => fields.2 as f64,
                        3 => fields.3 as f64,
                        4 => fields.4 as f64,
                        5 => fields.5 as f64,
                        _ => 0.0,
                    };
                    Completion::Normal(JsValue::Number(val))
                },
            ));
            proto.borrow_mut().insert_property(
                name.to_string(),
                PropertyDescriptor {
                    value: None,
                    writable: None,
                    enumerable: Some(false),
                    configurable: Some(true),
                    get: Some(getter),
                    set: None,
                },
            );
        }

        // with(temporalTimeLike)
        let with_fn = self.create_function(JsFunction::native(
            "with".to_string(),
            1,
            |interp, this, args| {
                let (h, m, s, ms, us, ns) = match get_plain_time_fields(interp, this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let item = args.first().cloned().unwrap_or(JsValue::Undefined);
                // IsPartialTemporalObject
                if let Err(c) = is_partial_temporal_object(interp, &item) {
                    return c;
                }
                // ToTemporalTimeRecord in alphabetical order:
                // hour, microsecond, millisecond, minute, nanosecond, second
                let mut has_any = false;
                let (new_h, has_h) = match read_time_field_new(interp, &item, "hour", h as f64) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                has_any |= has_h;
                let (new_us, has_us) =
                    match read_time_field_new(interp, &item, "microsecond", us as f64) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                has_any |= has_us;
                let (new_ms, has_ms) =
                    match read_time_field_new(interp, &item, "millisecond", ms as f64) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                has_any |= has_ms;
                let (new_m, has_mi) = match read_time_field_new(interp, &item, "minute", m as f64) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                has_any |= has_mi;
                let (new_ns, has_ns) =
                    match read_time_field_new(interp, &item, "nanosecond", ns as f64) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                has_any |= has_ns;
                let (new_s, has_s) = match read_time_field_new(interp, &item, "second", s as f64) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                has_any |= has_s;
                if !has_any {
                    return Completion::Throw(
                        interp
                            .create_type_error("with() requires at least one recognized property"),
                    );
                }
                let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let overflow = match parse_overflow_option(interp, &options) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if overflow == "reject" {
                    if !iso_time_valid_f64(new_h, new_m, new_s, new_ms, new_us, new_ns) {
                        return Completion::Throw(interp.create_range_error("Invalid time fields"));
                    }
                    create_plain_time_result(
                        interp,
                        new_h as u8,
                        new_m as u8,
                        new_s as u8,
                        new_ms as u16,
                        new_us as u16,
                        new_ns as u16,
                    )
                } else {
                    let ch = (new_h.clamp(0.0, 23.0)) as u8;
                    let cm = (new_m.clamp(0.0, 59.0)) as u8;
                    let cs = (new_s.clamp(0.0, 59.0)) as u8;
                    let cms = (new_ms.clamp(0.0, 999.0)) as u16;
                    let cus = (new_us.clamp(0.0, 999.0)) as u16;
                    let cns = (new_ns.clamp(0.0, 999.0)) as u16;
                    create_plain_time_result(interp, ch, cm, cs, cms, cus, cns)
                }
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("with".to_string(), with_fn);

        // add(temporalDuration) / subtract(temporalDuration)
        for &(name, sign) in &[("add", 1i128), ("subtract", -1i128)] {
            let fn_val = self.create_function(JsFunction::native(
                name.to_string(),
                1,
                move |interp, this, args| {
                    let (h, m, s, ms, us, ns) = match get_plain_time_fields(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let dur = match super::duration::to_temporal_duration_record(
                        interp,
                        args.first().cloned().unwrap_or(JsValue::Undefined),
                    ) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let time_ns = time_to_nanoseconds(h, m, s, ms, us, ns);
                    let delta = ((dur.4 as i128) * 3_600_000_000_000
                        + (dur.5 as i128) * 60_000_000_000
                        + (dur.6 as i128) * 1_000_000_000
                        + (dur.7 as i128) * 1_000_000
                        + (dur.8 as i128) * 1_000
                        + (dur.9 as i128))
                        * sign;
                    let result_ns = time_ns + delta;
                    let ns_per_day = 86_400_000_000_000i128;
                    let wrapped = ((result_ns % ns_per_day) + ns_per_day) % ns_per_day;
                    let (rh, rm, rs, rms, rus, rns) = nanoseconds_to_time(wrapped);
                    create_plain_time_result(interp, rh, rm, rs, rms, rus, rns)
                },
            ));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
        }

        // until(other, options?) / since(other, options?)
        for &(name, sign) in &[("until", 1i128), ("since", -1i128)] {
            let fn_val = self.create_function(JsFunction::native(
                name.to_string(),
                1,
                move |interp, this, args| {
                    let (h1, m1, s1, ms1, us1, ns1) = match get_plain_time_fields(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let other = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let (h2, m2, s2, ms2, us2, ns2) = match to_temporal_plain_time(interp, other) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let ns_this = time_to_nanoseconds(h1, m1, s1, ms1, us1, ns1);
                    let ns_other = time_to_nanoseconds(h2, m2, s2, ms2, us2, ns2);
                    let diff = if sign == 1 {
                        ns_other - ns_this
                    } else {
                        ns_this - ns_other
                    };

                    let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    let (largest_unit, smallest_unit, rounding_mode, rounding_increment) =
                        match parse_time_diff_options(interp, &options, "hour") {
                            Ok(v) => v,
                            Err(c) => return c,
                        };

                    let unit_ns = temporal_unit_length_ns(smallest_unit) as i128;
                    let inc_ns = unit_ns * rounding_increment as i128;
                    let rounded = if smallest_unit != "nanosecond" || rounding_increment != 1.0 {
                        round_i128_to_increment(diff, inc_ns, rounding_mode)
                    } else {
                        diff
                    };

                    let is_neg = rounded < 0;
                    let abs_ns = if is_neg { -rounded } else { rounded } as i128;
                    let sign_f = if is_neg {
                        -1.0
                    } else if abs_ns == 0 {
                        0.0
                    } else {
                        1.0
                    };
                    let (_, rh, rm, rs, rms, rus, rns) =
                        super::instant::unbalance_time_ns_i128(abs_ns, largest_unit);
                    super::duration::create_duration_result(
                        interp,
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                        rh as f64 * sign_f,
                        rm as f64 * sign_f,
                        rs as f64 * sign_f,
                        rms as f64 * sign_f,
                        rus as f64 * sign_f,
                        rns as f64 * sign_f,
                    )
                },
            ));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
        }

        // round(roundTo)
        let round_fn = self.create_function(JsFunction::native(
            "round".to_string(),
            1,
            |interp, this, args| {
                let (h, m, s, ms, us, ns) = match get_plain_time_fields(interp, this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let round_to = args.first().cloned().unwrap_or(JsValue::Undefined);
                if is_undefined(&round_to) {
                    return Completion::Throw(interp.create_type_error("round requires options"));
                }
                let (unit, rounding_mode, increment) = if let JsValue::String(ref su) = round_to {
                    let su_str = su.to_rust_string();
                    match temporal_unit_singular(&su_str) {
                        Some(u) if is_valid_time_round_unit(u) => (u, "halfExpand", 1.0),
                        Some(u) => {
                            return Completion::Throw(interp.create_range_error(&format!(
                                "{u} is not a valid value for smallest unit"
                            )));
                        }
                        None => {
                            return Completion::Throw(
                                interp.create_range_error(&format!("Invalid unit: {su_str}")),
                            );
                        }
                    }
                } else if matches!(round_to, JsValue::Object(_)) {
                    // Read all options in alphabetical order first, then validate
                    // 1. roundingIncrement: get + coerce
                    let inc_val = match get_prop(interp, &round_to, "roundingIncrement") {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let inc_raw = match coerce_rounding_increment(interp, &inc_val) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    // 2. roundingMode: get + coerce
                    let rm_val = match get_prop(interp, &round_to, "roundingMode") {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let rm_str: Option<String> = if is_undefined(&rm_val) {
                        None
                    } else {
                        Some(match interp.to_string_value(&rm_val) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        })
                    };
                    // 3. smallestUnit: get + coerce
                    let su_val = match get_prop(interp, &round_to, "smallestUnit") {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let su_str: Option<String> = if is_undefined(&su_val) {
                        None
                    } else {
                        Some(match interp.to_string_value(&su_val) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        })
                    };
                    // Validate smallestUnit
                    let unit = if let Some(ref sv) = su_str {
                        match temporal_unit_singular(sv) {
                            Some(u) if is_valid_time_round_unit(u) => u,
                            Some(u) => {
                                return Completion::Throw(interp.create_range_error(&format!(
                                    "{u} is not a valid value for smallest unit"
                                )));
                            }
                            None => {
                                return Completion::Throw(
                                    interp.create_range_error(&format!("Invalid unit: {sv}")),
                                );
                            }
                        }
                    } else {
                        return Completion::Throw(
                            interp.create_range_error("smallestUnit is required"),
                        );
                    };
                    // Validate roundingMode
                    let rm = if let Some(ref rs) = rm_str {
                        match rs.as_str() {
                            "ceil" => "ceil",
                            "floor" => "floor",
                            "trunc" => "trunc",
                            "expand" => "expand",
                            "halfExpand" => "halfExpand",
                            "halfTrunc" => "halfTrunc",
                            "halfCeil" => "halfCeil",
                            "halfFloor" => "halfFloor",
                            "halfEven" => "halfEven",
                            _ => {
                                return Completion::Throw(
                                    interp
                                        .create_range_error(&format!("Invalid roundingMode: {rs}")),
                                );
                            }
                        }
                    } else {
                        "halfExpand"
                    };
                    // Validate roundingIncrement
                    let inc = match validate_rounding_increment_raw(inc_raw, unit, false) {
                        Ok(v) => v,
                        Err(msg) => return Completion::Throw(interp.create_range_error(&msg)),
                    };
                    (unit, rm, inc)
                } else {
                    return Completion::Throw(
                        interp.create_type_error("round requires a string or object"),
                    );
                };

                let time_ns = time_to_nanoseconds(h, m, s, ms, us, ns);
                let unit_ns = temporal_unit_length_ns(unit) as i128;
                let rounded =
                    round_i128_to_increment(time_ns, unit_ns * increment as i128, rounding_mode);
                let ns_per_day = 86_400_000_000_000i128;
                let wrapped = ((rounded % ns_per_day) + ns_per_day) % ns_per_day;
                let (rh, rm, rs, rms, rus, rns) = nanoseconds_to_time(wrapped);
                create_plain_time_result(interp, rh, rm, rs, rms, rus, rns)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("round".to_string(), round_fn);

        // equals(other)
        let equals_fn = self.create_function(JsFunction::native(
            "equals".to_string(),
            1,
            |interp, this, args| {
                let (h1, m1, s1, ms1, us1, ns1) = match get_plain_time_fields(interp, this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let other = args.first().cloned().unwrap_or(JsValue::Undefined);
                let (h2, m2, s2, ms2, us2, ns2) = match to_temporal_plain_time(interp, other) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let eq = h1 == h2 && m1 == m2 && s1 == s2 && ms1 == ms2 && us1 == us2 && ns1 == ns2;
                Completion::Normal(JsValue::Boolean(eq))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("equals".to_string(), equals_fn);

        // toString(options?)
        let to_string_fn = self.create_function(JsFunction::native(
            "toString".to_string(),
            0,
            |interp, this, args| {
                let (h, m, s, ms, us, ns) = match get_plain_time_fields(interp, this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let options = args.first().cloned().unwrap_or(JsValue::Undefined);
                let (precision, rounding_mode) =
                    match parse_time_to_string_options(interp, &options) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };

                let (rh, rm, rs, rms, rus, rns) = if let Some(prec) = precision {
                    if prec <= 0 {
                        let time_ns = time_to_nanoseconds(h, m, s, ms, us, ns);
                        let unit_ns: i128 = if prec == -1 {
                            60_000_000_000
                        } else {
                            1_000_000_000
                        };
                        let rounded = round_i128_to_increment(time_ns, unit_ns, rounding_mode);
                        let ns_per_day: i128 = 86_400_000_000_000;
                        let wrapped = ((rounded % ns_per_day) + ns_per_day) % ns_per_day;
                        nanoseconds_to_time(wrapped)
                    } else {
                        let increment: i128 = 10i128.pow(9 - prec as u32);
                        let time_ns = time_to_nanoseconds(h, m, s, ms, us, ns);
                        let rounded = round_i128_to_increment(time_ns, increment, rounding_mode);
                        let ns_per_day: i128 = 86_400_000_000_000;
                        let wrapped = ((rounded % ns_per_day) + ns_per_day) % ns_per_day;
                        nanoseconds_to_time(wrapped)
                    }
                } else {
                    (h, m, s, ms, us, ns)
                };

                let result = format_plain_time(rh, rm, rs, rms, rus, rns, precision);
                Completion::Normal(JsValue::String(JsString::from_str(&result)))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toString".to_string(), to_string_fn);

        // toJSON()
        let to_json_fn = self.create_function(JsFunction::native(
            "toJSON".to_string(),
            0,
            |interp, this, _args| {
                let (h, m, s, ms, us, ns) = match get_plain_time_fields(interp, this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let result = format_plain_time(h, m, s, ms, us, ns, None);
                Completion::Normal(JsValue::String(JsString::from_str(&result)))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toJSON".to_string(), to_json_fn);

        // toLocaleString()
        let to_locale_fn = self.create_function(JsFunction::native(
            "toLocaleString".to_string(),
            0,
            |interp, this, _args| {
                let (h, m, s, ms, us, ns) = match get_plain_time_fields(interp, this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let result = format_plain_time(h, m, s, ms, us, ns, None);
                Completion::Normal(JsValue::String(JsString::from_str(&result)))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toLocaleString".to_string(), to_locale_fn);

        // valueOf() — throws
        let value_of_fn =
            self.create_function(JsFunction::native(
                "valueOf".to_string(),
                0,
                |interp, _this, _args| {
                    Completion::Throw(interp.create_type_error(
                        "use compare() or equals() to compare Temporal.PlainTime",
                    ))
                },
            ));
        proto
            .borrow_mut()
            .insert_builtin("valueOf".to_string(), value_of_fn);

        self.temporal_plain_time_prototype = Some(proto.clone());

        // Constructor
        let constructor = self.create_function(JsFunction::constructor(
            "PlainTime".to_string(),
            0,
            |interp, _this, args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Temporal.PlainTime must be called with new"),
                    );
                }
                let hour = if let Some(v) = args.first() {
                    if is_undefined(v) {
                        0u8
                    } else {
                        match interp.to_number_value(v) {
                            Ok(n) => {
                                if !n.is_finite() {
                                    return Completion::Throw(
                                        interp.create_range_error("Invalid hour"),
                                    );
                                }
                                let t = n.trunc();
                                if !(0.0..=23.0).contains(&t) {
                                    return Completion::Throw(
                                        interp.create_range_error("Invalid hour"),
                                    );
                                }
                                t as u8
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                } else {
                    0
                };
                let minute = if let Some(v) = args.get(1) {
                    if is_undefined(v) {
                        0u8
                    } else {
                        match interp.to_number_value(v) {
                            Ok(n) => {
                                if !n.is_finite() {
                                    return Completion::Throw(
                                        interp.create_range_error("Invalid minute"),
                                    );
                                }
                                let t = n.trunc();
                                if !(0.0..=59.0).contains(&t) {
                                    return Completion::Throw(
                                        interp.create_range_error("Invalid minute"),
                                    );
                                }
                                t as u8
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                } else {
                    0
                };
                let second = if let Some(v) = args.get(2) {
                    if is_undefined(v) {
                        0u8
                    } else {
                        match interp.to_number_value(v) {
                            Ok(n) => {
                                if !n.is_finite() {
                                    return Completion::Throw(
                                        interp.create_range_error("Invalid second"),
                                    );
                                }
                                let t = n.trunc();
                                if !(0.0..=59.0).contains(&t) {
                                    return Completion::Throw(
                                        interp.create_range_error("Invalid second"),
                                    );
                                }
                                t as u8
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                } else {
                    0
                };
                let millisecond = if let Some(v) = args.get(3) {
                    if is_undefined(v) {
                        0u16
                    } else {
                        match interp.to_number_value(v) {
                            Ok(n) => {
                                if !n.is_finite() {
                                    return Completion::Throw(
                                        interp.create_range_error("Invalid millisecond"),
                                    );
                                }
                                let t = n.trunc();
                                if !(0.0..=999.0).contains(&t) {
                                    return Completion::Throw(
                                        interp.create_range_error("Invalid millisecond"),
                                    );
                                }
                                t as u16
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                } else {
                    0
                };
                let microsecond = if let Some(v) = args.get(4) {
                    if is_undefined(v) {
                        0u16
                    } else {
                        match interp.to_number_value(v) {
                            Ok(n) => {
                                if !n.is_finite() {
                                    return Completion::Throw(
                                        interp.create_range_error("Invalid microsecond"),
                                    );
                                }
                                let t = n.trunc();
                                if !(0.0..=999.0).contains(&t) {
                                    return Completion::Throw(
                                        interp.create_range_error("Invalid microsecond"),
                                    );
                                }
                                t as u16
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                } else {
                    0
                };
                let nanosecond = if let Some(v) = args.get(5) {
                    if is_undefined(v) {
                        0u16
                    } else {
                        match interp.to_number_value(v) {
                            Ok(n) => {
                                if !n.is_finite() {
                                    return Completion::Throw(
                                        interp.create_range_error("Invalid nanosecond"),
                                    );
                                }
                                let t = n.trunc();
                                if !(0.0..=999.0).contains(&t) {
                                    return Completion::Throw(
                                        interp.create_range_error("Invalid nanosecond"),
                                    );
                                }
                                t as u16
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                } else {
                    0
                };
                create_plain_time_result(
                    interp,
                    hour,
                    minute,
                    second,
                    millisecond,
                    microsecond,
                    nanosecond,
                )
            },
        ));

        // Constructor.prototype
        if let JsValue::Object(ref o) = constructor
            && let Some(obj) = self.get_object(o.id) {
                let proto_val = JsValue::Object(crate::types::JsObject {
                    id: proto.borrow().id.unwrap(),
                });
                obj.borrow_mut().insert_property(
                    "prototype".to_string(),
                    PropertyDescriptor::data(proto_val, false, false, false),
                );
            }
        proto.borrow_mut().insert_property(
            "constructor".to_string(),
            PropertyDescriptor::data(constructor.clone(), true, false, true),
        );

        // PlainTime.from(item, options?)
        let from_fn = self.create_function(JsFunction::native(
            "from".to_string(),
            1,
            |interp, _this, args| {
                let item = args.first().cloned().unwrap_or(JsValue::Undefined);
                let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                // Per spec: if item is a string, parse first, then validate overflow (but don't use it)
                if matches!(&item, JsValue::String(_)) {
                    let (h, m, s, ms, us, ns) =
                        match to_temporal_plain_time_with_overflow(interp, item, "constrain") {
                            Ok(v) => v,
                            Err(c) => return c,
                        };
                    match parse_overflow_option(interp, &options) {
                        Ok(_) => {}
                        Err(c) => return c,
                    }
                    return create_plain_time_result(interp, h, m, s, ms, us, ns);
                }
                // Per spec: read fields first (ToTemporalTimeRecord), then overflow option
                let raw = match to_temporal_plain_time_raw(interp, item) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let overflow = match parse_overflow_option(interp, &options) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if overflow == "reject" {
                    if !iso_time_valid(raw.0, raw.1, raw.2, raw.3, raw.4, raw.5) {
                        return Completion::Throw(interp.create_range_error("Invalid time fields"));
                    }
                    create_plain_time_result(interp, raw.0, raw.1, raw.2, raw.3, raw.4, raw.5)
                } else {
                    create_plain_time_result(
                        interp,
                        raw.0.min(23),
                        raw.1.min(59),
                        raw.2.min(59),
                        raw.3.min(999),
                        raw.4.min(999),
                        raw.5.min(999),
                    )
                }
            },
        ));
        if let JsValue::Object(ref o) = constructor
            && let Some(obj) = self.get_object(o.id) {
                obj.borrow_mut().insert_builtin("from".to_string(), from_fn);
            }

        // PlainTime.compare(one, two)
        let compare_fn = self.create_function(JsFunction::native(
            "compare".to_string(),
            2,
            |interp, _this, args| {
                let one = match to_temporal_plain_time(
                    interp,
                    args.first().cloned().unwrap_or(JsValue::Undefined),
                ) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let two = match to_temporal_plain_time(
                    interp,
                    args.get(1).cloned().unwrap_or(JsValue::Undefined),
                ) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let ns1 = time_to_nanoseconds(one.0, one.1, one.2, one.3, one.4, one.5);
                let ns2 = time_to_nanoseconds(two.0, two.1, two.2, two.3, two.4, two.5);
                let result = if ns1 < ns2 {
                    -1.0
                } else if ns1 > ns2 {
                    1.0
                } else {
                    0.0
                };
                Completion::Normal(JsValue::Number(result))
            },
        ));
        if let JsValue::Object(ref o) = constructor
            && let Some(obj) = self.get_object(o.id) {
                obj.borrow_mut()
                    .insert_builtin("compare".to_string(), compare_fn);
            }

        temporal_obj.borrow_mut().insert_property(
            "PlainTime".to_string(),
            PropertyDescriptor::data(constructor, true, false, true),
        );
    }
}

fn get_plain_time_fields(
    interp: &mut Interpreter,
    this: &JsValue,
) -> Result<(u8, u8, u8, u16, u16, u16), Completion> {
    let obj = match this {
        JsValue::Object(o) => match interp.get_object(o.id) {
            Some(obj) => obj,
            None => {
                return Err(Completion::Throw(
                    interp.create_type_error("invalid object"),
                ));
            }
        },
        _ => {
            return Err(Completion::Throw(
                interp.create_type_error("not a Temporal.PlainTime"),
            ));
        }
    };
    let data = obj.borrow();
    match &data.temporal_data {
        Some(TemporalData::PlainTime {
            hour,
            minute,
            second,
            millisecond,
            microsecond,
            nanosecond,
        }) => Ok((
            *hour,
            *minute,
            *second,
            *millisecond,
            *microsecond,
            *nanosecond,
        )),
        _ => Err(Completion::Throw(
            interp.create_type_error("not a Temporal.PlainTime"),
        )),
    }
}

pub(super) fn create_plain_time_result(
    interp: &mut Interpreter,
    h: u8,
    m: u8,
    s: u8,
    ms: u16,
    us: u16,
    ns: u16,
) -> Completion {
    let obj = interp.create_object();
    obj.borrow_mut().class_name = "Temporal.PlainTime".to_string();
    if let Some(ref proto) = interp.temporal_plain_time_prototype {
        obj.borrow_mut().prototype = Some(proto.clone());
    }
    obj.borrow_mut().temporal_data = Some(TemporalData::PlainTime {
        hour: h,
        minute: m,
        second: s,
        millisecond: ms,
        microsecond: us,
        nanosecond: ns,
    });
    let id = obj.borrow().id.unwrap();
    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
}

pub(super) fn to_temporal_plain_time_with_overflow(
    interp: &mut Interpreter,
    item: JsValue,
    overflow: &str,
) -> Result<(u8, u8, u8, u16, u16, u16), Completion> {
    let result = to_temporal_plain_time_raw(interp, item)?;
    if overflow == "constrain" {
        Ok((
            result.0.min(23),
            result.1.min(59),
            result.2.min(59),
            result.3.min(999),
            result.4.min(999),
            result.5.min(999),
        ))
    } else {
        // reject
        if !iso_time_valid(result.0, result.1, result.2, result.3, result.4, result.5) {
            return Err(Completion::Throw(
                interp.create_range_error("Invalid time fields"),
            ));
        }
        Ok(result)
    }
}

/// ToTemporalTime per spec — always constrains (default overflow=constrain)
pub(super) fn to_temporal_plain_time(
    interp: &mut Interpreter,
    item: JsValue,
) -> Result<(u8, u8, u8, u16, u16, u16), Completion> {
    let result = to_temporal_plain_time_raw(interp, item)?;
    Ok((
        result.0.min(23),
        result.1.min(59),
        result.2.min(59),
        result.3.min(999),
        result.4.min(999),
        result.5.min(999),
    ))
}

/// Raw time extraction without clamping — used by to_temporal_plain_time_with_overflow
fn to_temporal_plain_time_raw(
    interp: &mut Interpreter,
    item: JsValue,
) -> Result<(u8, u8, u8, u16, u16, u16), Completion> {
    match &item {
        JsValue::Object(o) => {
            if let Some(obj) = interp.get_object(o.id) {
                let data = obj.borrow();
                if let Some(TemporalData::PlainTime {
                    hour,
                    minute,
                    second,
                    millisecond,
                    microsecond,
                    nanosecond,
                }) = &data.temporal_data
                {
                    return Ok((
                        *hour,
                        *minute,
                        *second,
                        *millisecond,
                        *microsecond,
                        *nanosecond,
                    ));
                }
                if let Some(TemporalData::PlainDateTime {
                    hour,
                    minute,
                    second,
                    millisecond,
                    microsecond,
                    nanosecond,
                    ..
                }) = &data.temporal_data
                {
                    return Ok((
                        *hour,
                        *minute,
                        *second,
                        *millisecond,
                        *microsecond,
                        *nanosecond,
                    ));
                }
                if let Some(TemporalData::ZonedDateTime {
                    epoch_nanoseconds,
                    time_zone,
                    ..
                }) = &data.temporal_data
                {
                    let (_, _, _, h, mi, s, ms, us, ns) =
                        super::zoned_date_time::epoch_ns_to_components(
                            epoch_nanoseconds,
                            time_zone,
                        );
                    return Ok((h, mi, s, ms, us, ns));
                }
            }
            // Property bag: read and coerce each field in alphabetical order per spec:
            // hour, microsecond, millisecond, minute, nanosecond, second
            let h_val = match get_prop(interp, &item, "hour") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let (h, h_present) = if is_undefined(&h_val) {
                (0u8, false)
            } else {
                (to_integer_with_truncation(interp, &h_val)? as u8, true)
            };
            let us_val = match get_prop(interp, &item, "microsecond") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let (us, us_present) = if is_undefined(&us_val) {
                (0u16, false)
            } else {
                (to_integer_with_truncation(interp, &us_val)? as u16, true)
            };
            let ms_val = match get_prop(interp, &item, "millisecond") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let (ms, ms_present) = if is_undefined(&ms_val) {
                (0u16, false)
            } else {
                (to_integer_with_truncation(interp, &ms_val)? as u16, true)
            };
            let m_val = match get_prop(interp, &item, "minute") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let (m, m_present) = if is_undefined(&m_val) {
                (0u8, false)
            } else {
                (to_integer_with_truncation(interp, &m_val)? as u8, true)
            };
            let ns_val = match get_prop(interp, &item, "nanosecond") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let (ns, ns_present) = if is_undefined(&ns_val) {
                (0u16, false)
            } else {
                (to_integer_with_truncation(interp, &ns_val)? as u16, true)
            };
            let s_val = match get_prop(interp, &item, "second") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let (s, s_present) = if is_undefined(&s_val) {
                (0u8, false)
            } else {
                (to_integer_with_truncation(interp, &s_val)? as u8, true)
            };
            // Per spec: if no time properties exist, throw TypeError
            if !h_present && !m_present && !s_present && !ms_present && !us_present && !ns_present {
                return Err(Completion::Throw(
                    interp.create_type_error("Property bag has no time fields"),
                ));
            }
            Ok((h, m, s, ms, us, ns))
        }
        JsValue::String(s) => parse_time_string(interp, &s.to_rust_string()),
        _ => Err(Completion::Throw(
            interp.create_type_error("Cannot convert to Temporal.PlainTime"),
        )),
    }
}

fn parse_time_string(
    interp: &mut Interpreter,
    s: &str,
) -> Result<(u8, u8, u8, u16, u16, u16), Completion> {
    match parse_temporal_time_string(s) {
        Some((h, m, sec, ms, us, ns, has_utc)) => {
            if has_utc {
                return Err(Completion::Throw(interp.create_range_error(
                    "UTC designator Z is not allowed in a PlainTime string",
                )));
            }
            Ok((h, m, sec, ms, us, ns))
        }
        None => Err(Completion::Throw(
            interp.create_range_error(&format!("Invalid time string: {s}")),
        )),
    }
}

#[allow(dead_code)]
fn get_time_field(
    interp: &mut Interpreter,
    obj: &JsValue,
    key: &str,
    default: f64,
) -> Result<f64, Completion> {
    let val = match get_prop(interp, obj, key) {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    if is_undefined(&val) {
        Ok(default)
    } else {
        to_integer_with_truncation(interp, &val)
    }
}

fn is_valid_time_round_unit(unit: &str) -> bool {
    matches!(
        unit,
        "hour" | "minute" | "second" | "millisecond" | "microsecond" | "nanosecond"
    )
}

pub(super) fn round_i128_to_increment(n: i128, increment: i128, mode: &str) -> i128 {
    if increment == 0 {
        return n;
    }
    let quotient = n / increment;
    let remainder = n % increment;
    if remainder == 0 {
        return n;
    }
    let is_negative = n < 0;
    let abs_rem = remainder.unsigned_abs();
    let abs_inc = increment.unsigned_abs();

    let round_up = match mode {
        "ceil" => !is_negative,
        "floor" => is_negative,
        "trunc" => false,
        "expand" => true,
        "halfExpand" => abs_rem * 2 >= abs_inc,
        "halfTrunc" => abs_rem * 2 > abs_inc,
        "halfCeil" => {
            if !is_negative {
                abs_rem * 2 >= abs_inc
            } else {
                abs_rem * 2 > abs_inc
            }
        }
        "halfFloor" => {
            if is_negative {
                abs_rem * 2 >= abs_inc
            } else {
                abs_rem * 2 > abs_inc
            }
        }
        "halfEven" => {
            let doubled = abs_rem * 2;
            if doubled > abs_inc {
                true
            } else if doubled < abs_inc {
                false
            } else {
                !quotient.unsigned_abs().is_multiple_of(2)
            }
        }
        _ => false,
    };

    if round_up {
        if is_negative {
            (quotient - 1) * increment
        } else {
            (quotient + 1) * increment
        }
    } else {
        quotient * increment
    }
}

fn parse_time_diff_options<'a>(
    interp: &mut Interpreter,
    options: &JsValue,
    default_largest: &'a str,
) -> Result<(&'a str, &'a str, &'a str, f64), Completion> {
    // GetOptionsObject per spec
    let has_options = get_options_object(interp, options)?;
    if !has_options {
        return Ok((default_largest, "nanosecond", "trunc", 1.0));
    }

    // Read ALL options first (get + coerce), then validate

    // 1. largestUnit: get + coerce to string
    let lu = match get_prop(interp, options, "largestUnit") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let largest_str: Option<String> = if is_undefined(&lu) {
        None // auto
    } else {
        Some(match interp.to_string_value(&lu) {
            Ok(v) => v,
            Err(e) => return Err(Completion::Throw(e)),
        })
    };

    // 2. roundingIncrement: get + coerce
    let inc_val = match get_prop(interp, options, "roundingIncrement") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let inc_raw = coerce_rounding_increment(interp, &inc_val)?;

    // 3. roundingMode: get + coerce to string
    let rm_val = match get_prop(interp, options, "roundingMode") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let rm_str: Option<String> = if is_undefined(&rm_val) {
        None
    } else {
        Some(match interp.to_string_value(&rm_val) {
            Ok(v) => v,
            Err(e) => return Err(Completion::Throw(e)),
        })
    };

    // 4. smallestUnit: get + coerce to string
    let su = match get_prop(interp, options, "smallestUnit") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let su_str: Option<String> = if is_undefined(&su) {
        None
    } else {
        Some(match interp.to_string_value(&su) {
            Ok(v) => v,
            Err(e) => return Err(Completion::Throw(e)),
        })
    };

    // Now validate all values
    let mut largest_auto = largest_str.is_none();
    let largest: &str = if let Some(ref ls) = largest_str {
        if ls == "auto" {
            largest_auto = true;
            default_largest
        } else {
            match temporal_unit_singular(ls) {
                Some(u) if is_valid_time_round_unit(u) => u,
                _ => {
                    return Err(Completion::Throw(
                        interp.create_range_error(&format!("Invalid unit: {ls}")),
                    ));
                }
            }
        }
    } else {
        default_largest
    };

    let rm: &str = if let Some(ref rs) = rm_str {
        match rs.as_str() {
            "ceil" => "ceil",
            "floor" => "floor",
            "trunc" => "trunc",
            "expand" => "expand",
            "halfExpand" => "halfExpand",
            "halfTrunc" => "halfTrunc",
            "halfCeil" => "halfCeil",
            "halfFloor" => "halfFloor",
            "halfEven" => "halfEven",
            _ => {
                return Err(Completion::Throw(
                    interp.create_range_error(&format!("Invalid roundingMode: {rs}")),
                ));
            }
        }
    } else {
        "trunc"
    };

    let smallest: &str = if let Some(ref ss) = su_str {
        match temporal_unit_singular(ss) {
            Some(u) if is_valid_time_round_unit(u) => u,
            _ => {
                return Err(Completion::Throw(
                    interp.create_range_error(&format!("Invalid unit: {ss}")),
                ));
            }
        }
    } else {
        "nanosecond"
    };

    // Auto-bump largestUnit if smallestUnit is larger
    let largest: &str =
        if largest_auto && temporal_unit_order(smallest) > temporal_unit_order(largest) {
            smallest
        } else {
            largest
        };

    // Validate: smallestUnit <= largestUnit
    if temporal_unit_order(smallest) > temporal_unit_order(largest) {
        return Err(Completion::Throw(interp.create_range_error(
            "smallestUnit must not be larger than largestUnit",
        )));
    }

    // Validate roundingIncrement against smallestUnit
    if let Some(max) = max_rounding_increment(smallest) {
        let i = inc_raw as u64;
        if i >= max {
            return Err(Completion::Throw(interp.create_range_error(&format!(
                "roundingIncrement {inc_raw} is out of range for {smallest}"
            ))));
        }
        if max % i != 0 {
            return Err(Completion::Throw(interp.create_range_error(&format!(
                "roundingIncrement {inc_raw} does not divide evenly into {max}"
            ))));
        }
    }

    Ok((largest, smallest, rm, inc_raw))
}

fn parse_time_to_string_options(
    interp: &mut Interpreter,
    options: &JsValue,
) -> Result<(Option<i32>, &'static str), Completion> {
    if is_undefined(options) {
        return Ok((None, "trunc"));
    }
    if !matches!(options, JsValue::Object(_)) {
        return Err(Completion::Throw(
            interp.create_type_error("options must be an object"),
        ));
    }
    // fractionalSecondDigits
    let fsd_val = match get_prop(interp, options, "fractionalSecondDigits") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let mut precision = None;
    if !is_undefined(&fsd_val) {
        if matches!(fsd_val, JsValue::Number(_)) {
            // GetStringOrNumberOption: value is Number → floor then range check
            let n = match interp.to_number_value(&fsd_val) {
                Ok(v) => v,
                Err(e) => return Err(Completion::Throw(e)),
            };
            if n.is_nan() || !n.is_finite() {
                return Err(Completion::Throw(interp.create_range_error(
                    "fractionalSecondDigits must be 0-9 or 'auto'",
                )));
            }
            let floored = n.floor();
            if !(0.0..=9.0).contains(&floored) {
                return Err(Completion::Throw(interp.create_range_error(
                    "fractionalSecondDigits must be 0-9 or 'auto'",
                )));
            }
            precision = Some(floored as i32);
        } else {
            // GetStringOrNumberOption: non-Number → ToString then check for "auto"
            let s = interp
                .to_string_value(&fsd_val)
                .map_err(Completion::Throw)?;
            if s != "auto" {
                return Err(Completion::Throw(interp.create_range_error(
                    "fractionalSecondDigits must be 0-9 or 'auto'",
                )));
            }
        }
    }
    // roundingMode: get + coerce (before smallestUnit per spec)
    let rm_val = match get_prop(interp, options, "roundingMode") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let rm = if is_undefined(&rm_val) {
        "trunc"
    } else {
        let s = match interp.to_string_value(&rm_val) {
            Ok(v) => v,
            Err(e) => return Err(Completion::Throw(e)),
        };
        match s.as_str() {
            "ceil" => "ceil",
            "floor" => "floor",
            "trunc" => "trunc",
            "expand" => "expand",
            "halfExpand" => "halfExpand",
            "halfTrunc" => "halfTrunc",
            "halfCeil" => "halfCeil",
            "halfFloor" => "halfFloor",
            "halfEven" => "halfEven",
            _ => {
                return Err(Completion::Throw(
                    interp.create_range_error(&format!("Invalid roundingMode: {s}")),
                ));
            }
        }
    };
    // smallestUnit: get + coerce (overrides fractionalSecondDigits)
    let su_val = match get_prop(interp, options, "smallestUnit") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    if !is_undefined(&su_val) {
        let s = match interp.to_string_value(&su_val) {
            Ok(v) => v,
            Err(e) => return Err(Completion::Throw(e)),
        };
        precision = match temporal_unit_singular(&s) {
            Some("minute") => Some(-1),
            Some("second") => Some(0),
            Some("millisecond") => Some(3),
            Some("microsecond") => Some(6),
            Some("nanosecond") => Some(9),
            _ => {
                return Err(Completion::Throw(
                    interp.create_range_error(&format!("Invalid unit: {s}")),
                ));
            }
        };
    }
    Ok((precision, rm))
}

pub(super) fn format_plain_time(
    h: u8,
    m: u8,
    s: u8,
    ms: u16,
    us: u16,
    ns: u16,
    precision: Option<i32>,
) -> String {
    let frac_ns = ms as u32 * 1_000_000 + us as u32 * 1_000 + ns as u32;
    match precision {
        Some(-1) => format!("{h:02}:{m:02}"),
        Some(0) => format!("{h:02}:{m:02}:{s:02}"),
        Some(digits) if digits > 0 => {
            let frac = format!("{frac_ns:09}");
            let truncated = &frac[..digits as usize];
            format!("{h:02}:{m:02}:{s:02}.{truncated}")
        }
        None => {
            if frac_ns == 0 {
                format!("{h:02}:{m:02}:{s:02}")
            } else {
                let frac = format!("{frac_ns:09}");
                let trimmed = frac.trim_end_matches('0');
                format!("{h:02}:{m:02}:{s:02}.{trimmed}")
            }
        }
        _ => format!("{h:02}:{m:02}:{s:02}"),
    }
}
