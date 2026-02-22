use super::*;
use crate::interpreter::builtins::temporal::{
    coerce_rounding_increment, get_prop, is_undefined, parse_temporal_instant_string,
    temporal_unit_length_ns, temporal_unit_singular,
};
use num_bigint::BigInt;

const NS_MAX: i128 = 8_640_000_000_000_000_000_000; // ±8.64×10²¹
const NS_PER_MS: i128 = 1_000_000;
#[allow(dead_code)]
const NS_PER_US: i128 = 1_000;

pub(super) fn is_valid_epoch_ns(ns: &BigInt) -> bool {
    let max = BigInt::from(NS_MAX);
    let min = BigInt::from(-NS_MAX);
    *ns >= min && *ns <= max
}

impl Interpreter {
    pub(crate) fn setup_temporal_instant(&mut self, temporal_obj: &Rc<RefCell<JsObjectData>>) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Temporal.Instant".to_string();

        // @@toStringTag
        {
            let key = "Symbol(Symbol.toStringTag)".to_string();
            let desc = PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("Temporal.Instant"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            };
            proto.borrow_mut().property_order.push(key.clone());
            proto.borrow_mut().properties.insert(key, desc);
        }

        // Getter: epochMilliseconds
        {
            let getter = self.create_function(JsFunction::native(
                "get epochMilliseconds".to_string(),
                0,
                |interp, this, _args| {
                    let ns = match get_instant_ns(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let ms = floor_div_bigint(&ns, NS_PER_MS);
                    Completion::Normal(JsValue::Number(bigint_to_f64(&ms)))
                },
            ));
            proto.borrow_mut().insert_property(
                "epochMilliseconds".to_string(),
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

        // Getter: epochNanoseconds
        {
            let getter = self.create_function(JsFunction::native(
                "get epochNanoseconds".to_string(),
                0,
                |interp, this, _args| {
                    let ns = match get_instant_ns(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    Completion::Normal(JsValue::BigInt(crate::types::JsBigInt { value: ns }))
                },
            ));
            proto.borrow_mut().insert_property(
                "epochNanoseconds".to_string(),
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

        // equals(other)
        let equals_fn = self.create_function(JsFunction::native(
            "equals".to_string(),
            1,
            |interp, this, args| {
                let ns = match get_instant_ns(interp, this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let other = match to_temporal_instant(
                    interp,
                    args.first().cloned().unwrap_or(JsValue::Undefined),
                ) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                Completion::Normal(JsValue::Boolean(ns == other))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("equals".to_string(), equals_fn);

        // add(temporalDuration)
        let add_fn = self.create_function(JsFunction::native(
            "add".to_string(),
            1,
            |interp, this, args| {
                let ns = match get_instant_ns(interp, this) {
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
                if dur.0 != 0.0 || dur.1 != 0.0 || dur.2 != 0.0 || dur.3 != 0.0 {
                    return Completion::Throw(interp.create_range_error(
                        "Instant.add does not support years, months, weeks, or days",
                    ));
                }
                let delta_ns = dur.4 as i128 * 3_600_000_000_000
                    + dur.5 as i128 * 60_000_000_000
                    + dur.6 as i128 * 1_000_000_000
                    + dur.7 as i128 * 1_000_000
                    + dur.8 as i128 * 1_000
                    + dur.9 as i128;
                let result = ns + BigInt::from(delta_ns);
                if !is_valid_epoch_ns(&result) {
                    return Completion::Throw(interp.create_range_error("Instant out of range"));
                }
                create_instant_result(interp, result)
            },
        ));
        proto.borrow_mut().insert_builtin("add".to_string(), add_fn);

        // subtract(temporalDuration)
        let subtract_fn = self.create_function(JsFunction::native(
            "subtract".to_string(),
            1,
            |interp, this, args| {
                let ns = match get_instant_ns(interp, this) {
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
                if dur.0 != 0.0 || dur.1 != 0.0 || dur.2 != 0.0 || dur.3 != 0.0 {
                    return Completion::Throw(interp.create_range_error(
                        "Instant.subtract does not support years, months, weeks, or days",
                    ));
                }
                let delta_ns = dur.4 as i128 * 3_600_000_000_000
                    + dur.5 as i128 * 60_000_000_000
                    + dur.6 as i128 * 1_000_000_000
                    + dur.7 as i128 * 1_000_000
                    + dur.8 as i128 * 1_000
                    + dur.9 as i128;
                let result = ns - BigInt::from(delta_ns);
                if !is_valid_epoch_ns(&result) {
                    return Completion::Throw(interp.create_range_error("Instant out of range"));
                }
                create_instant_result(interp, result)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("subtract".to_string(), subtract_fn);

        // until(other, options?) / since(other, options?)
        for &(name, sign) in &[("until", 1i128), ("since", -1i128)] {
            let fn_val = self.create_function(JsFunction::native(
                name.to_string(),
                1,
                move |interp, this, args| {
                    let ns1 = match get_instant_ns(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let ns2 = match to_temporal_instant(
                        interp,
                        args.first().cloned().unwrap_or(JsValue::Undefined),
                    ) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let diff_ns: i128 = if sign == 1 {
                        (&ns2 - &ns1).try_into().unwrap_or(0)
                    } else {
                        (&ns1 - &ns2).try_into().unwrap_or(0)
                    };

                    // Parse options
                    let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    let time_units: &[&str] = &[
                        "hour",
                        "minute",
                        "second",
                        "millisecond",
                        "microsecond",
                        "nanosecond",
                    ];
                    let (largest_unit, smallest_unit, rounding_mode, rounding_increment) =
                        match super::parse_difference_options(
                            interp, &options, "second", time_units,
                        ) {
                            Ok(v) => v,
                            Err(c) => return c,
                        };

                    // Round the difference using mathematical rounding
                    let diff_big = BigInt::from(diff_ns);
                    let rounded_big = if smallest_unit != "nanosecond" || rounding_increment != 1.0
                    {
                        let unit_ns = temporal_unit_length_ns(&smallest_unit) as i128;
                        let inc_ns = unit_ns * rounding_increment as i128;
                        round_bigint_to_increment(&diff_big, inc_ns, &rounding_mode)
                    } else {
                        diff_big
                    };

                    // Unbalance into components using largest unit, with BigInt precision
                    let effective_largest = if super::temporal_unit_order(&largest_unit)
                        > super::temporal_unit_order(&smallest_unit)
                    {
                        &largest_unit
                    } else {
                        &smallest_unit
                    };
                    let is_neg = rounded_big < BigInt::from(0);
                    let abs_big = if is_neg {
                        -&rounded_big
                    } else {
                        rounded_big.clone()
                    };
                    let abs_ns: i128 = abs_big.try_into().unwrap_or(0);
                    let sign_f = if is_neg {
                        -1.0
                    } else if abs_ns == 0 {
                        0.0
                    } else {
                        1.0
                    };
                    let (d, h, mi, sec, ms, us, ns) =
                        unbalance_time_ns_i128(abs_ns, effective_largest);
                    super::duration::create_duration_result(
                        interp,
                        0.0,
                        0.0,
                        0.0,
                        d as f64 * sign_f,
                        h as f64 * sign_f,
                        mi as f64 * sign_f,
                        sec as f64 * sign_f,
                        ms as f64 * sign_f,
                        us as f64 * sign_f,
                        ns as f64 * sign_f,
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
                let ns = match get_instant_ns(interp, this) {
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
                        Some(u) if is_valid_instant_round_unit(u) => (u, "halfExpand", 1.0),
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
                    // 1. roundingIncrement
                    let inc_val = match get_prop(interp, &round_to, "roundingIncrement") {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let inc_raw = match coerce_rounding_increment(interp, &inc_val) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    // 2. roundingMode
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
                    // 3. smallestUnit
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
                    // Validate
                    let unit = if let Some(ref sv) = su_str {
                        match temporal_unit_singular(sv) {
                            Some(u) if is_valid_instant_round_unit(u) => u,
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
                                    interp.create_range_error(&format!("Invalid roundingMode: {rs}")),
                                );
                            }
                        }
                    } else {
                        "halfExpand"
                    };
                    // Validate roundingIncrement for day divisibility
                    let unit_ns = temporal_unit_length_ns(unit) as u64;
                    if unit_ns > 0 {
                        let total_ns = inc_raw as u64 * unit_ns;
                        let day_ns: u64 = 86_400_000_000_000;
                        if !day_ns.is_multiple_of(total_ns) {
                            return Completion::Throw(interp.create_range_error(&format!(
                                "roundingIncrement {inc_raw} for {unit} does not divide evenly into a day"
                            )));
                        }
                    }
                    (unit, rm, inc_raw)
                } else {
                    return Completion::Throw(
                        interp.create_type_error("round requires a string or object"),
                    );
                };

                let unit_ns = temporal_unit_length_ns(unit) as i128;
                let inc_ns = unit_ns * increment as i128;
                let result = round_temporal_instant(&ns, inc_ns, rounding_mode);
                if !is_valid_epoch_ns(&result) {
                    return Completion::Throw(interp.create_range_error("Instant out of range"));
                }
                create_instant_result(interp, result)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("round".to_string(), round_fn);

        // toString(options?)
        let to_string_fn = self.create_function(JsFunction::native(
            "toString".to_string(),
            0,
            |interp, this, args| {
                let ns = match get_instant_ns(interp, this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let options = args.first().cloned().unwrap_or(JsValue::Undefined);
                let (tz_id, tz_offset_ns, tz_explicit, frac_digits, smallest_unit, rounding_mode) =
                    match parse_to_string_options(interp, &options) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };

                // Apply rounding if smallestUnit or fractionalSecondDigits specified
                let rounded_ns = if let Some(unit) = smallest_unit {
                    let unit_ns = temporal_unit_length_ns(unit) as i128;
                    round_temporal_instant(&ns, unit_ns, rounding_mode)
                } else if let Some(digits) = frac_digits {
                    let increment = 10i128.pow(9 - digits as u32);
                    round_temporal_instant(&ns, increment, rounding_mode)
                } else {
                    ns
                };

                let precision = if let Some(unit) = smallest_unit {
                    match unit {
                        "minute" => Some(-1i32), // special: omit seconds entirely
                        "second" => Some(0),
                        "millisecond" => Some(3),
                        "microsecond" => Some(6),
                        "nanosecond" => Some(9),
                        _ => None,
                    }
                } else {
                    frac_digits.map(|d| d as i32)
                };

                let result = instant_to_string_with_tz(
                    &rounded_ns,
                    &tz_id,
                    tz_offset_ns,
                    precision,
                    tz_explicit,
                );
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
                let ns = match get_instant_ns(interp, this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let result = instant_to_string_with_tz(&ns, "UTC", 0, None, false);
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
                let ns = match get_instant_ns(interp, this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let result = instant_to_string_with_tz(&ns, "UTC", 0, None, false);
                Completion::Normal(JsValue::String(JsString::from_str(&result)))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toLocaleString".to_string(), to_locale_fn);

        // valueOf() — throws
        let value_of_fn = self.create_function(JsFunction::native(
            "valueOf".to_string(),
            0,
            |interp, _this, _args| {
                Completion::Throw(
                    interp
                        .create_type_error("use compare() or equals() to compare Temporal.Instant"),
                )
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("valueOf".to_string(), value_of_fn);

        // toZonedDateTimeISO(timeZone)
        let to_zdt_fn = self.create_function(JsFunction::native(
            "toZonedDateTimeISO".to_string(),
            1,
            |interp, this, args| {
                let ns = match get_instant_ns(interp, this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let tz_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                if super::is_undefined(&tz_arg) {
                    return Completion::Throw(
                        interp.create_type_error("timeZone argument is required"),
                    );
                }
                let tz = match super::to_temporal_time_zone_identifier(interp, &tz_arg) {
                    Ok(t) => t,
                    Err(c) => return c,
                };
                super::zoned_date_time::create_zdt_pub(interp, ns, tz, "iso8601".to_string())
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toZonedDateTimeISO".to_string(), to_zdt_fn);

        self.temporal_instant_prototype = Some(proto.clone());

        // Constructor
        let constructor = self.create_function(JsFunction::constructor(
            "Instant".to_string(),
            1,
            |interp, _this, args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Temporal.Instant must be called with new"),
                    );
                }
                let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let epoch_ns = match to_bigint_arg(interp, &arg) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if !is_valid_epoch_ns(&epoch_ns) {
                    return Completion::Throw(interp.create_range_error("Instant out of range"));
                }
                create_instant_result(interp, epoch_ns)
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

        // Instant.from(item)
        let from_fn = self.create_function(JsFunction::native(
            "from".to_string(),
            1,
            |interp, _this, args| {
                let item = args.first().cloned().unwrap_or(JsValue::Undefined);
                let ns = match to_temporal_instant(interp, item) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                create_instant_result(interp, ns)
            },
        ));
        if let JsValue::Object(ref o) = constructor
            && let Some(obj) = self.get_object(o.id) {
                obj.borrow_mut().insert_builtin("from".to_string(), from_fn);
            }

        // Instant.fromEpochMilliseconds(ms)
        let from_ms_fn = self.create_function(JsFunction::native(
            "fromEpochMilliseconds".to_string(),
            1,
            |interp, _this, args| {
                let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let ms = match interp.to_number_value(&arg) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                if !ms.is_finite() || ms != ms.trunc() {
                    return Completion::Throw(
                        interp.create_range_error("epochMilliseconds must be an integer"),
                    );
                }
                let ns = BigInt::from(ms as i128) * BigInt::from(NS_PER_MS);
                if !is_valid_epoch_ns(&ns) {
                    return Completion::Throw(interp.create_range_error("Instant out of range"));
                }
                create_instant_result(interp, ns)
            },
        ));
        if let JsValue::Object(ref o) = constructor
            && let Some(obj) = self.get_object(o.id) {
                obj.borrow_mut()
                    .insert_builtin("fromEpochMilliseconds".to_string(), from_ms_fn);
            }

        // Instant.fromEpochNanoseconds(ns)
        let from_ns_fn = self.create_function(JsFunction::native(
            "fromEpochNanoseconds".to_string(),
            1,
            |interp, _this, args| {
                let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let ns = match to_bigint_arg(interp, &arg) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if !is_valid_epoch_ns(&ns) {
                    return Completion::Throw(interp.create_range_error("Instant out of range"));
                }
                create_instant_result(interp, ns)
            },
        ));
        if let JsValue::Object(ref o) = constructor
            && let Some(obj) = self.get_object(o.id) {
                obj.borrow_mut()
                    .insert_builtin("fromEpochNanoseconds".to_string(), from_ns_fn);
            }

        // Instant.compare(one, two)
        let compare_fn = self.create_function(JsFunction::native(
            "compare".to_string(),
            2,
            |interp, _this, args| {
                let one = match to_temporal_instant(
                    interp,
                    args.first().cloned().unwrap_or(JsValue::Undefined),
                ) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let two = match to_temporal_instant(
                    interp,
                    args.get(1).cloned().unwrap_or(JsValue::Undefined),
                ) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let result = if one < two {
                    -1.0
                } else if one > two {
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
            "Instant".to_string(),
            PropertyDescriptor::data(constructor, true, false, true),
        );
    }
}

pub(super) fn unbalance_time_ns_i128(
    total_ns: i128,
    largest_unit: &str,
) -> (i128, i128, i128, i128, i128, i128, i128) {
    match largest_unit {
        "day" | "days" => {
            let d = total_ns / 86_400_000_000_000;
            let rem = total_ns % 86_400_000_000_000;
            let h = rem / 3_600_000_000_000;
            let rem = rem % 3_600_000_000_000;
            let mi = rem / 60_000_000_000;
            let rem = rem % 60_000_000_000;
            let s = rem / 1_000_000_000;
            let rem = rem % 1_000_000_000;
            let ms = rem / 1_000_000;
            let rem = rem % 1_000_000;
            let us = rem / 1_000;
            let ns = rem % 1_000;
            (d, h, mi, s, ms, us, ns)
        }
        "hour" | "hours" => {
            let h = total_ns / 3_600_000_000_000;
            let rem = total_ns % 3_600_000_000_000;
            let mi = rem / 60_000_000_000;
            let rem = rem % 60_000_000_000;
            let s = rem / 1_000_000_000;
            let rem = rem % 1_000_000_000;
            let ms = rem / 1_000_000;
            let rem = rem % 1_000_000;
            let us = rem / 1_000;
            let ns = rem % 1_000;
            (0, h, mi, s, ms, us, ns)
        }
        "minute" | "minutes" => {
            let mi = total_ns / 60_000_000_000;
            let rem = total_ns % 60_000_000_000;
            let s = rem / 1_000_000_000;
            let rem = rem % 1_000_000_000;
            let ms = rem / 1_000_000;
            let rem = rem % 1_000_000;
            let us = rem / 1_000;
            let ns = rem % 1_000;
            (0, 0, mi, s, ms, us, ns)
        }
        "second" | "seconds" => {
            let s = total_ns / 1_000_000_000;
            let rem = total_ns % 1_000_000_000;
            let ms = rem / 1_000_000;
            let rem = rem % 1_000_000;
            let us = rem / 1_000;
            let ns = rem % 1_000;
            (0, 0, 0, s, ms, us, ns)
        }
        "millisecond" | "milliseconds" => {
            let ms = total_ns / 1_000_000;
            let rem = total_ns % 1_000_000;
            let us = rem / 1_000;
            let ns = rem % 1_000;
            (0, 0, 0, 0, ms, us, ns)
        }
        "microsecond" | "microseconds" => {
            let us = total_ns / 1_000;
            let ns = total_ns % 1_000;
            (0, 0, 0, 0, 0, us, ns)
        }
        _ => (0, 0, 0, 0, 0, 0, total_ns),
    }
}

fn is_valid_instant_round_unit(unit: &str) -> bool {
    matches!(
        unit,
        "hour" | "minute" | "second" | "millisecond" | "microsecond" | "nanosecond"
    )
}

fn get_instant_ns(interp: &mut Interpreter, this: &JsValue) -> Result<BigInt, Completion> {
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
                interp.create_type_error("not a Temporal.Instant"),
            ));
        }
    };
    let data = obj.borrow();
    match &data.temporal_data {
        Some(TemporalData::Instant { epoch_nanoseconds }) => Ok(epoch_nanoseconds.clone()),
        _ => Err(Completion::Throw(
            interp.create_type_error("not a Temporal.Instant"),
        )),
    }
}

fn create_instant_result(interp: &mut Interpreter, epoch_ns: BigInt) -> Completion {
    let obj = interp.create_object();
    obj.borrow_mut().class_name = "Temporal.Instant".to_string();
    if let Some(ref proto) = interp.temporal_instant_prototype {
        obj.borrow_mut().prototype = Some(proto.clone());
    }
    obj.borrow_mut().temporal_data = Some(TemporalData::Instant {
        epoch_nanoseconds: epoch_ns,
    });
    let id = obj.borrow().id.unwrap();
    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
}

pub(super) fn to_temporal_instant(
    interp: &mut Interpreter,
    item: JsValue,
) -> Result<BigInt, Completion> {
    match &item {
        JsValue::Object(o) => {
            if let Some(obj) = interp.get_object(o.id) {
                let data = obj.borrow();
                if let Some(TemporalData::Instant { epoch_nanoseconds }) = &data.temporal_data {
                    return Ok(epoch_nanoseconds.clone());
                }
                if let Some(TemporalData::ZonedDateTime {
                    epoch_nanoseconds, ..
                }) = &data.temporal_data
                {
                    return Ok(epoch_nanoseconds.clone());
                }
            }
            // Not a Temporal.Instant — call toString() and parse
            let s = match interp.to_string_value(&item) {
                Ok(v) => v,
                Err(e) => return Err(Completion::Throw(e)),
            };
            parse_instant_string(interp, &s)
        }
        JsValue::String(s) => parse_instant_string(interp, &s.to_rust_string()),
        JsValue::Undefined
        | JsValue::Null
        | JsValue::Boolean(_)
        | JsValue::Number(_)
        | JsValue::BigInt(_) => Err(Completion::Throw(
            interp.create_type_error("Cannot convert to Temporal.Instant"),
        )),
        JsValue::Symbol(_) => Err(Completion::Throw(
            interp.create_type_error("Cannot convert a Symbol to a string"),
        )),
    }
}

fn parse_instant_string(interp: &mut Interpreter, s: &str) -> Result<BigInt, Completion> {
    let parsed = match parse_temporal_instant_string(s) {
        Some(p) => p,
        None => {
            return Err(Completion::Throw(
                interp.create_range_error(&format!("Invalid instant string: {s}")),
            ));
        }
    };
    let offset = parsed.offset.unwrap();
    let epoch_days = super::iso_date_to_epoch_days(parsed.year, parsed.month, parsed.day);
    let day_ns = epoch_days as i128 * 86_400_000_000_000;
    let time_ns = parsed.hour as i128 * 3_600_000_000_000
        + parsed.minute as i128 * 60_000_000_000
        + parsed.second as i128 * 1_000_000_000
        + parsed.millisecond as i128 * 1_000_000
        + parsed.microsecond as i128 * 1_000
        + parsed.nanosecond as i128;
    let offset_ns = (offset.sign as i128)
        * (offset.hours as i128 * 3_600_000_000_000
            + offset.minutes as i128 * 60_000_000_000
            + offset.seconds as i128 * 1_000_000_000
            + offset.nanoseconds as i128);
    let total_ns = day_ns + time_ns - offset_ns;
    let result = BigInt::from(total_ns);
    if !is_valid_epoch_ns(&result) {
        return Err(Completion::Throw(
            interp.create_range_error("Instant out of range"),
        ));
    }
    Ok(result)
}

pub(super) fn to_bigint_arg(interp: &mut Interpreter, val: &JsValue) -> Result<BigInt, Completion> {
    match val {
        JsValue::BigInt(n) => Ok(n.value.clone()),
        JsValue::String(s) => {
            let s_str = s.to_rust_string();
            match s_str.parse::<BigInt>() {
                Ok(v) => Ok(v),
                Err(_) => Err(Completion::Throw(interp.create_error(
                    "SyntaxError",
                    &format!("Cannot convert {s_str} to a BigInt"),
                ))),
            }
        }
        JsValue::Boolean(b) => Ok(BigInt::from(if *b { 1 } else { 0 })),
        JsValue::Number(_) => Err(Completion::Throw(
            interp.create_type_error("Cannot convert a Number to a BigInt"),
        )),
        JsValue::Undefined | JsValue::Null => Err(Completion::Throw(
            interp.create_type_error("Cannot convert to BigInt"),
        )),
        JsValue::Symbol(_) => Err(Completion::Throw(
            interp.create_type_error("Cannot convert a Symbol value to a BigInt"),
        )),
        _ => {
            // ToBigInt via ToPrimitive
            let prim = match interp.to_primitive(val, "number") {
                Ok(v) => v,
                Err(e) => return Err(Completion::Throw(e)),
            };
            to_bigint_arg(interp, &prim)
        }
    }
}

// Mathematical rounding for durations (until/since): sign-aware trunc/expand/etc.
fn round_bigint_to_increment(n: &BigInt, increment: i128, mode: &str) -> BigInt {
    let inc = BigInt::from(increment);
    let zero = BigInt::from(0);
    let (quotient, remainder) = {
        let q = n / &inc;
        let r = n - &q * &inc;
        (q, r)
    };
    if remainder == zero {
        return n.clone();
    }
    let is_negative = *n < zero;
    let abs_rem = if remainder < zero {
        -&remainder
    } else {
        remainder.clone()
    };
    let abs_inc = BigInt::from(increment.abs());

    let round_up = match mode {
        "ceil" => !is_negative,
        "floor" => is_negative,
        "trunc" => false,
        "expand" => true,
        "halfExpand" => abs_rem.clone() * BigInt::from(2) >= abs_inc,
        "halfTrunc" => abs_rem.clone() * BigInt::from(2) > abs_inc,
        "halfCeil" => {
            let doubled = &abs_rem * BigInt::from(2);
            if !is_negative {
                doubled >= abs_inc
            } else {
                doubled > abs_inc
            }
        }
        "halfFloor" => {
            let doubled = &abs_rem * BigInt::from(2);
            if is_negative {
                doubled >= abs_inc
            } else {
                doubled > abs_inc
            }
        }
        "halfEven" => {
            let doubled = &abs_rem * BigInt::from(2);
            if doubled > abs_inc {
                true
            } else if doubled < abs_inc {
                false
            } else {
                let abs_q = if quotient < zero {
                    -&quotient
                } else {
                    quotient.clone()
                };
                abs_q % BigInt::from(2) != zero
            }
        }
        _ => false,
    };

    if round_up {
        if is_negative {
            (&quotient - BigInt::from(1)) * &inc
        } else {
            (&quotient + BigInt::from(1)) * &inc
        }
    } else {
        &quotient * &inc
    }
}

// Spec's RoundTemporalInstant rounding: r1=floor, r2=ceil, sign-independent modes.
// trunc/expand/halfTrunc/halfExpand behave as floor/ceil/halfFloor/halfCeil.
fn round_temporal_instant(n: &BigInt, increment: i128, mode: &str) -> BigInt {
    let inc = BigInt::from(increment);
    let zero = BigInt::from(0);

    let remainder = n % &inc;
    if remainder == zero {
        return n.clone();
    }

    let q_trunc = n / &inc;
    let (r1_q, r2_q) = if remainder > zero {
        (q_trunc.clone(), &q_trunc + BigInt::from(1))
    } else {
        (&q_trunc - BigInt::from(1), q_trunc.clone())
    };

    let r1 = &r1_q * &inc;
    let r2 = &r2_q * &inc;
    let d1 = n - &r1;
    let d2 = &r2 - n;

    match mode {
        "floor" | "trunc" => r1,
        "ceil" | "expand" => r2,
        "halfFloor" | "halfTrunc" => {
            if d1 < d2 {
                r1
            } else if d2 < d1 {
                r2
            } else {
                r1
            }
        }
        "halfCeil" | "halfExpand" => {
            if d1 < d2 {
                r1
            } else if d2 < d1 {
                r2
            } else {
                r2
            }
        }
        "halfEven" => {
            if d1 < d2 {
                r1
            } else if d2 < d1 {
                r2
            } else {
                let r1_abs = if r1_q < zero { -&r1_q } else { r1_q.clone() };
                if r1_abs % BigInt::from(2) == zero {
                    r1
                } else {
                    r2
                }
            }
        }
        _ => r1,
    }
}

pub(super) fn bigint_to_f64(n: &BigInt) -> f64 {
    // Convert BigInt to f64, handling large values
    let s = n.to_string();
    s.parse::<f64>().unwrap_or(f64::NAN)
}

pub(super) fn floor_div_bigint(n: &BigInt, d: i128) -> BigInt {
    let divisor = BigInt::from(d);
    if *n >= BigInt::from(0) {
        n / &divisor
    } else {
        // Floor division for negative: -((-n - 1) / d + 1)
        let neg_n = -n;
        let q = (&neg_n - BigInt::from(1)) / &divisor + BigInt::from(1);
        -q
    }
}

fn instant_to_string_with_tz(
    ns: &BigInt,
    tz_id: &str,
    tz_offset_ns: i64,
    precision: Option<i32>,
    tz_explicit: bool,
) -> String {
    let total_ns: i128 = ns.try_into().unwrap_or(0);
    // Apply timezone offset
    let local_ns = total_ns + tz_offset_ns as i128;
    let epoch_days = local_ns.div_euclid(86_400_000_000_000);
    let day_ns = local_ns.rem_euclid(86_400_000_000_000);

    let (year, month, day) = super::epoch_days_to_iso_date(epoch_days as i64);
    let nanosecond = (day_ns % 1_000) as u16;
    let microsecond = ((day_ns / 1_000) % 1_000) as u16;
    let millisecond = ((day_ns / 1_000_000) % 1_000) as u16;
    let second = ((day_ns / 1_000_000_000) % 60) as u8;
    let minute = ((day_ns / 60_000_000_000) % 60) as u8;
    let hour = ((day_ns / 3_600_000_000_000) % 24) as u8;

    let year_str = if (0..=9999).contains(&year) {
        format!("{year:04}")
    } else if year >= 0 {
        format!("+{year:06}")
    } else {
        format!("-{:06}", year.unsigned_abs())
    };

    let frac_ns = millisecond as u32 * 1_000_000 + microsecond as u32 * 1_000 + nanosecond as u32;
    let time_str = match precision {
        Some(-1) => format!("{hour:02}:{minute:02}"), // minute precision: omit seconds
        Some(0) => format!("{hour:02}:{minute:02}:{second:02}"),
        Some(digits) if digits > 0 => {
            let frac = format!("{frac_ns:09}");
            let truncated = &frac[..digits as usize];
            format!("{hour:02}:{minute:02}:{second:02}.{truncated}")
        }
        None => {
            if frac_ns == 0 {
                format!("{hour:02}:{minute:02}:{second:02}")
            } else {
                let frac = format!("{frac_ns:09}");
                let trimmed = frac.trim_end_matches('0');
                format!("{hour:02}:{minute:02}:{second:02}.{trimmed}")
            }
        }
        _ => format!("{hour:02}:{minute:02}:{second:02}"),
    };

    // Format offset: Z only when no explicit timeZone; otherwise numeric offset
    let offset_str = if !tz_explicit && tz_offset_ns == 0 {
        "Z".to_string()
    } else {
        let abs_offset = tz_offset_ns.unsigned_abs() as i64;
        let sign_ch = if tz_offset_ns >= 0 { '+' } else { '-' };
        let oh = abs_offset / 3_600_000_000_000;
        let om = (abs_offset / 60_000_000_000) % 60;
        let os = (abs_offset / 1_000_000_000) % 60;
        let ons = abs_offset % 1_000_000_000;
        if ons != 0 {
            let frac = format!("{ons:09}");
            let trimmed = frac.trim_end_matches('0');
            format!("{sign_ch}{oh:02}:{om:02}:{os:02}.{trimmed}")
        } else if os != 0 {
            format!("{sign_ch}{oh:02}:{om:02}:{os:02}")
        } else {
            format!("{sign_ch}{oh:02}:{om:02}")
        }
    };

    let mut result = format!("{year_str}-{month:02}-{day:02}T{time_str}{offset_str}");

    // Add timezone annotation if not UTC/offset
    if tz_id != "UTC" && !tz_id.starts_with('+') && !tz_id.starts_with('-') {
        result.push_str(&format!("[{tz_id}]"));
    }

    result
}

// Returns (tz_id, tz_offset_ns, tz_explicit, frac_digits, smallest_unit, rounding_mode)
fn parse_to_string_options(
    interp: &mut Interpreter,
    options: &JsValue,
) -> Result<
    (
        String,
        i64,
        bool,
        Option<u8>,
        Option<&'static str>,
        &'static str,
    ),
    Completion,
> {
    if is_undefined(options) {
        return Ok(("UTC".to_string(), 0, false, None, None, "trunc"));
    }
    if !matches!(options, JsValue::Object(_)) {
        return Err(Completion::Throw(
            interp.create_type_error("options must be an object"),
        ));
    }

    // Per spec, read options in alphabetical order:
    // fractionalSecondDigits, roundingMode, smallestUnit, timeZone

    // fractionalSecondDigits — GetStringOrNumberOption
    let fsd_val = match get_prop(interp, options, "fractionalSecondDigits") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let frac_digits = if is_undefined(&fsd_val) {
        None
    } else if matches!(fsd_val, JsValue::Number(_)) {
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
        Some(floored as u8)
    } else {
        // Not a number — convert to string and check for "auto"
        let s = match interp.to_string_value(&fsd_val) {
            Ok(v) => v,
            Err(e) => return Err(Completion::Throw(e)),
        };
        if s == "auto" {
            None
        } else {
            return Err(Completion::Throw(interp.create_range_error(&format!(
                "Invalid fractionalSecondDigits: {s}"
            ))));
        }
    };

    // roundingMode
    let rm_val = match get_prop(interp, options, "roundingMode") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let rounding_mode = if is_undefined(&rm_val) {
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

    // smallestUnit — read and coerce, defer validation
    let su_val = match get_prop(interp, options, "smallestUnit") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let su_str: Option<String> = if is_undefined(&su_val) {
        None
    } else {
        Some(match interp.to_string_value(&su_val) {
            Ok(v) => v,
            Err(e) => return Err(Completion::Throw(e)),
        })
    };

    // timeZone — ToTemporalTimeZoneIdentifier
    let tz_val = match get_prop(interp, options, "timeZone") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let (tz_id, tz_offset_ns, tz_explicit) = if is_undefined(&tz_val) {
        ("UTC".to_string(), 0i64, false)
    } else {
        // Per spec, must be a string (not coerced from other types)
        let tz_str = match &tz_val {
            JsValue::String(s) => s.to_string(),
            _ => {
                return Err(Completion::Throw(
                    interp.create_type_error("timeZone must be a string"),
                ));
            }
        };
        let (id, offset) = validate_timezone_string(interp, &tz_str)?;
        (id, offset, true)
    };

    // Now validate smallestUnit
    let smallest_unit = if let Some(ref ss) = su_str {
        match temporal_unit_singular(ss) {
            Some(u)
                if matches!(
                    u,
                    "minute" | "second" | "millisecond" | "microsecond" | "nanosecond"
                ) =>
            {
                Some(u)
            }
            _ => {
                return Err(Completion::Throw(interp.create_range_error(&format!(
                    "{ss} is not a valid value for smallest unit"
                ))));
            }
        }
    } else {
        None
    };

    Ok((
        tz_id,
        tz_offset_ns,
        tz_explicit,
        frac_digits,
        smallest_unit,
        rounding_mode,
    ))
}

/// Parse a plain numeric offset like "+01:00", "-05:00", "+0100", "+01".
/// Returns (formatted_id, offset_ns) or None if not a valid offset.
/// Only HH:MM precision allowed — sub-minute offsets rejected.
fn parse_plain_offset(s: &str) -> Option<(String, i64)> {
    let bytes = s.as_bytes();
    if bytes.is_empty() || (bytes[0] != b'+' && bytes[0] != b'-') {
        return None;
    }
    let sign: i64 = if bytes[0] == b'-' { -1 } else { 1 };
    let rest = &s[1..];
    // Only digits and colons allowed
    if rest.chars().any(|c| !c.is_ascii_digit() && c != ':') {
        return None;
    }
    let parts: Vec<&str> = rest.split(':').collect();
    match parts.len() {
        1 => {
            // ±HH or ±HHMM
            if parts[0].len() == 2 {
                let h: i64 = parts[0].parse().ok()?;
                if h > 23 {
                    return None;
                }
                let id = format!("{}{:02}:{:02}", if sign < 0 { "-" } else { "+" }, h, 0);
                Some((id, sign * h * 3_600_000_000_000))
            } else if parts[0].len() == 4 {
                let h: i64 = parts[0][..2].parse().ok()?;
                let m: i64 = parts[0][2..].parse().ok()?;
                if h > 23 || m > 59 {
                    return None;
                }
                let id = format!("{}{:02}:{:02}", if sign < 0 { "-" } else { "+" }, h, m);
                Some((id, sign * (h * 3_600_000_000_000 + m * 60_000_000_000)))
            } else {
                None
            }
        }
        2 => {
            // ±HH:MM — exactly 2 parts
            if parts[0].len() != 2 || parts[1].len() != 2 {
                return None;
            }
            let h: i64 = parts[0].parse().ok()?;
            let m: i64 = parts[1].parse().ok()?;
            if h > 23 || m > 59 {
                return None;
            }
            let id = format!("{}{:02}:{:02}", if sign < 0 { "-" } else { "+" }, h, m);
            Some((id, sign * (h * 3_600_000_000_000 + m * 60_000_000_000)))
        }
        _ => None, // ±HH:MM:SS or more — sub-minute, rejected
    }
}

/// ToTemporalTimeZoneIdentifier — validates a timezone string.
/// Returns (tz_id, offset_ns) or error.
fn validate_timezone_string(
    interp: &mut Interpreter,
    s: &str,
) -> Result<(String, i64), Completion> {
    if s.is_empty() {
        return Err(Completion::Throw(
            interp.create_range_error("Invalid time zone: empty string"),
        ));
    }
    // 1. Try as plain numeric offset: ±HH:MM, ±HHMM, ±HH
    if let Some(v) = parse_plain_offset(s) {
        return Ok(v);
    }
    // 2. Try as named timezone: "UTC", "America/New_York", "Etc/GMT+5", etc.
    //    Named TZ identifiers are alphanumeric with /, _, -, +
    //    They don't start with digits and don't look like ISO date-time strings
    if s.eq_ignore_ascii_case("utc") || s == "Etc/UTC" || s == "Etc/GMT" {
        return Ok(("UTC".to_string(), 0));
    }
    // A named timezone must have at least one letter and not look like a datetime
    // (datetimes have digit runs like YYYY-MM-DD or contain 'T' as date/time separator
    //  in positions after a date-like prefix)
    let looks_like_iana = s.contains('/')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '/' || c == '_' || c == '-' || c == '+');
    if looks_like_iana {
        return Ok((s.to_string(), 0));
    }
    // 3. Try as ISO date-time string with timezone info
    //    Must have a Z, offset, or [annotation] to be valid as timezone source
    if let Some(tz) = extract_timezone_from_iso_string(s) {
        return Ok(tz);
    }
    Err(Completion::Throw(
        interp.create_range_error(&format!("Invalid time zone: {s}")),
    ))
}

/// Extract timezone info from an ISO date-time string.
/// Returns Some((id, offset_ns)) if the string has timezone info, None otherwise.
fn extract_timezone_from_iso_string(s: &str) -> Option<(String, i64)> {
    // Reject negative zero year: -000000
    if s.starts_with("-000000") {
        return None;
    }
    // Look for [annotation] bracket — IANA name takes precedence
    if let Some(bracket_start) = s.find('[')
        && let Some(bracket_end) = s[bracket_start..].find(']') {
            let annotation = &s[bracket_start + 1..bracket_start + bracket_end];
            // Skip non-timezone annotations like u-ca=iso8601
            if !annotation.contains('=') {
                if annotation.eq_ignore_ascii_case("UTC") || annotation == "Etc/UTC" {
                    return Some(("UTC".to_string(), 0));
                }
                // Validate: annotation must be a valid TZ identifier
                // (either IANA name or ±HH:MM offset — no sub-minute)
                if annotation.starts_with('+') || annotation.starts_with('-') {
                    // Must be a valid offset with no sub-minute
                    return parse_plain_offset(annotation);
                }
                // IANA name: must contain '/' and be alphanumeric
                if annotation.contains('/') {
                    return Some((annotation.to_string(), 0));
                }
                // Other plain names (e.g. just "UTC" was handled above)
                return None; // Invalid annotation
            }
        }
    // Look for Z
    let _upper = s.to_uppercase();
    // Find time portion (after T)
    let t_pos = s.find(['T', 't'])?;
    let time_part = &s[t_pos + 1..];
    // Strip calendar annotation if present
    let time_part = if let Some(b) = time_part.find('[') {
        &time_part[..b]
    } else {
        time_part
    };
    // Check for Z at the end
    if time_part.ends_with('Z') || time_part.ends_with('z') {
        return Some(("UTC".to_string(), 0));
    }
    // Look for offset after the time digits
    // Find the last +/- that's part of an offset (not part of exponent, etc.)
    // Time format: HH:MM:SS.fff±HH:MM
    let offset_re_start = find_offset_in_time(time_part)?;
    let offset_str = &time_part[offset_re_start..];
    // Validate: must be exactly ±HH:MM (no sub-minute)
    parse_plain_offset(offset_str)
}

/// Find the start of an offset (+ or -) in a time string portion.
/// Skips time digits to find the trailing offset.
fn find_offset_in_time(time: &str) -> Option<usize> {
    // Walk past time digits: HH:MM:SS.fractional
    let bytes = time.as_bytes();
    let mut i = 0;
    // Skip digits and colons and dots (time portion)
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b':' || bytes[i] == b'.') {
        i += 1;
    }
    // Should now be at + or -
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        // Validate the offset portion has no sub-minute (seconds) parts
        let offset_part = &time[i..];
        let _offset_bytes = offset_part.as_bytes();
        // Check for sub-minute: if there are more than 2 colon-separated parts, reject
        let colon_count = offset_part.chars().filter(|&c| c == ':').count();
        if colon_count > 1 {
            return None; // Sub-minute offset
        }
        // Also check ±HHMMSS (6+ digits without colons)
        let digits_after_sign = &offset_part[1..];
        if digits_after_sign.len() > 4
            && !digits_after_sign.contains(':')
            && digits_after_sign.chars().all(|c| c.is_ascii_digit())
        {
            return None; // Sub-minute offset in compact form
        }
        Some(i)
    } else {
        None // No offset found — bare date-time string
    }
}

/// Legacy wrapper for non-validated timezone parsing (used by ZDT etc.)
#[allow(dead_code)]
fn parse_timezone_offset(s: &str) -> (String, i64) {
    if s == "UTC" {
        return ("UTC".to_string(), 0);
    }
    if let Some(v) = parse_plain_offset(s) {
        return v;
    }
    // Named timezone — for now just treat as UTC (full IANA support in Phase 9)
    (s.to_string(), 0)
}
