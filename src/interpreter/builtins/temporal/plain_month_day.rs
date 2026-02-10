use super::*;
use crate::interpreter::builtins::temporal::{
    get_prop, is_undefined, iso_date_valid, iso_days_in_month, iso_month_code,
    parse_overflow_option, parse_temporal_month_day_string, read_month_fields, resolve_month_fields,
    to_temporal_calendar_slot_value, validate_calendar,
};

pub(super) fn create_plain_month_day_result(
    interp: &mut Interpreter,
    m: u8,
    d: u8,
    ref_year: i32,
    cal: &str,
) -> Completion {
    let obj = interp.create_object();
    obj.borrow_mut().class_name = "Temporal.PlainMonthDay".to_string();
    if let Some(ref proto) = interp.temporal_plain_month_day_prototype {
        obj.borrow_mut().prototype = Some(proto.clone());
    }
    obj.borrow_mut().temporal_data = Some(TemporalData::PlainMonthDay {
        iso_month: m,
        iso_day: d,
        reference_iso_year: ref_year,
        calendar: cal.to_string(),
    });
    let id = obj.borrow().id.unwrap();
    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
}

fn get_md_fields(
    interp: &mut Interpreter,
    this: &JsValue,
) -> Result<(u8, u8, i32, String), Completion> {
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
                interp.create_type_error("not a Temporal.PlainMonthDay"),
            ));
        }
    };
    let data = obj.borrow();
    match &data.temporal_data {
        Some(TemporalData::PlainMonthDay {
            iso_month,
            iso_day,
            reference_iso_year,
            calendar,
        }) => Ok((*iso_month, *iso_day, *reference_iso_year, calendar.clone())),
        _ => Err(Completion::Throw(
            interp.create_type_error("not a Temporal.PlainMonthDay"),
        )),
    }
}

fn read_pmd_property_bag_raw(
    interp: &mut Interpreter,
    item: &JsValue,
) -> Result<(Option<f64>, Option<String>, f64, String, Option<i32>), Completion> {
    // Alphabetical: calendar, day, month, monthCode, year
    let cal_val = match get_prop(interp, item, "calendar") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let cal = to_temporal_calendar_slot_value(interp, &cal_val)?;
    let d_val = match get_prop(interp, item, "day") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let d_f = if is_undefined(&d_val) {
        return Err(Completion::Throw(interp.create_type_error("day is required")));
    } else {
        to_integer_with_truncation(interp, &d_val)?
    };
    let m_val = match get_prop(interp, item, "month") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let month_num = if !is_undefined(&m_val) {
        Some(to_integer_with_truncation(interp, &m_val)?)
    } else {
        None
    };
    let mc_val = match get_prop(interp, item, "monthCode") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let mc_str = if !is_undefined(&mc_val) {
        Some(super::to_primitive_and_require_string(interp, &mc_val, "monthCode")?)
    } else {
        None
    };
    let y_val = match get_prop(interp, item, "year") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let year_opt = if !is_undefined(&y_val) {
        Some(to_integer_with_truncation(interp, &y_val)? as i32)
    } else {
        None
    };
    if d_f < 1.0 {
        return Err(Completion::Throw(
            interp.create_range_error("day must be a positive integer"),
        ));
    }
    Ok((month_num, mc_str, d_f, cal, year_opt))
}

fn resolve_month_from_raw(
    interp: &mut Interpreter,
    month_num: Option<f64>,
    mc_str: Option<String>,
) -> Result<u8, Completion> {
    if let Some(ref mc) = mc_str {
        match super::plain_date::month_code_to_number_pub(mc) {
            Some(n) => {
                if let Some(mn) = month_num {
                    if mn as u8 != n {
                        return Err(Completion::Throw(
                            interp.create_range_error("month and monthCode conflict"),
                        ));
                    }
                }
                Ok(n)
            }
            None => Err(Completion::Throw(
                interp.create_range_error(&format!("Invalid monthCode: {mc}")),
            )),
        }
    } else if let Some(mn) = month_num {
        Ok(mn as u8)
    } else {
        Err(Completion::Throw(
            interp.create_type_error("month or monthCode is required"),
        ))
    }
}

fn to_temporal_plain_month_day(
    interp: &mut Interpreter,
    item: JsValue,
) -> Result<(u8, u8, i32, String), Completion> {
    match &item {
        JsValue::Object(o) => {
            if let Some(obj) = interp.get_object(o.id) {
                let data = obj.borrow();
                if let Some(TemporalData::PlainMonthDay {
                    iso_month,
                    iso_day,
                    reference_iso_year,
                    calendar,
                }) = &data.temporal_data
                {
                    return Ok((*iso_month, *iso_day, *reference_iso_year, calendar.clone()));
                }
            }
            // Alphabetical order: calendar, day, month, monthCode, year
            let cal_val = match get_prop(interp, &item, "calendar") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let cal = to_temporal_calendar_slot_value(interp, &cal_val)?;
            let d_val = match get_prop(interp, &item, "day") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let d_f = if is_undefined(&d_val) {
                return Err(Completion::Throw(
                    interp.create_type_error("day is required"),
                ));
            } else {
                to_integer_with_truncation(interp, &d_val)?
            };
            let m_val = match get_prop(interp, &item, "month") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let month_num = if !is_undefined(&m_val) {
                Some(to_integer_with_truncation(interp, &m_val)?)
            } else {
                None
            };
            let mc_val = match get_prop(interp, &item, "monthCode") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let mc_str = if !is_undefined(&mc_val) {
                Some(super::to_primitive_and_require_string(interp, &mc_val, "monthCode")?)
            } else {
                None
            };
            let y_val = match get_prop(interp, &item, "year") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let _y = if !is_undefined(&y_val) {
                Some(to_integer_with_truncation(interp, &y_val)? as i32)
            } else {
                None
            };
            // Resolve month from month/monthCode
            let m = if let Some(ref mc) = mc_str {
                match super::plain_date::month_code_to_number_pub(mc) {
                    Some(n) => {
                        if let Some(mn) = month_num {
                            if mn as u8 != n {
                                return Err(Completion::Throw(
                                    interp.create_range_error("month and monthCode conflict"),
                                ));
                            }
                        }
                        n
                    }
                    None => {
                        return Err(Completion::Throw(
                            interp.create_range_error(&format!("Invalid monthCode: {mc}")),
                        ));
                    }
                }
            } else if let Some(mn) = month_num {
                mn as u8
            } else {
                return Err(Completion::Throw(
                    interp.create_type_error("month or monthCode is required"),
                ));
            };
            let d = d_f as u8;
            if d_f < 1.0 {
                return Err(Completion::Throw(
                    interp.create_range_error("day must be a positive integer"),
                ));
            }
            Ok((m, d, 1972, cal)) // 1972 is a leap year reference
        }
        JsValue::String(s) => {
            let parsed = match parse_temporal_month_day_string(&s.to_rust_string()) {
                Some(v) => v,
                None => {
                    return Err(Completion::Throw(interp.create_range_error(&format!(
                        "Invalid month-day string: {}",
                        s.to_rust_string()
                    ))));
                }
            };
            // PlainMonthDay does not accept UTC designator
            if parsed.4 {
                return Err(Completion::Throw(interp.create_range_error(
                    "UTC designator Z is not allowed in a PlainMonthDay string",
                )));
            }
            let cal = parsed.3.unwrap_or_else(|| "iso8601".to_string());
            let cal = match validate_calendar(&cal) {
                Some(c) => c,
                None => {
                    return Err(Completion::Throw(
                        interp.create_range_error(&format!("Invalid calendar: {cal}")),
                    ));
                }
            };
            // For ISO calendar, reference year is always 1972 (a leap year)
            let ref_year = if cal == "iso8601" { 1972 } else { parsed.2.unwrap_or(1972) };
            Ok((parsed.0, parsed.1, ref_year, cal))
        }
        _ => Err(Completion::Throw(
            interp.create_type_error("Cannot convert to Temporal.PlainMonthDay"),
        )),
    }
}

impl Interpreter {
    pub(crate) fn setup_temporal_plain_month_day(
        &mut self,
        temporal_obj: &Rc<RefCell<JsObjectData>>,
    ) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Temporal.PlainMonthDay".to_string();
        {
            let key = "Symbol(Symbol.toStringTag)".to_string();
            let desc = PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str(
                    "Temporal.PlainMonthDay",
                ))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            };
            proto.borrow_mut().property_order.push(key.clone());
            proto.borrow_mut().properties.insert(key, desc);
        }

        // Getters: calendarId, monthCode, month, day
        {
            let getter = self.create_function(JsFunction::native(
                "get calendarId".to_string(),
                0,
                |interp, this, _| {
                    let (_, _, _, cal) = match get_md_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    Completion::Normal(JsValue::String(JsString::from_str(&cal)))
                },
            ));
            proto.borrow_mut().insert_property(
                "calendarId".to_string(),
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
        {
            let getter = self.create_function(JsFunction::native(
                "get monthCode".to_string(),
                0,
                |interp, this, _| {
                    let (m, _, _, _) = match get_md_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    Completion::Normal(JsValue::String(JsString::from_str(&iso_month_code(m))))
                },
            ));
            proto.borrow_mut().insert_property(
                "monthCode".to_string(),
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
        {
            let getter = self.create_function(JsFunction::native(
                "get day".to_string(),
                0,
                |interp, this, _| {
                    let (_, d, _, _) = match get_md_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    Completion::Normal(JsValue::Number(d as f64))
                },
            ));
            proto.borrow_mut().insert_property(
                "day".to_string(),
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

        // with(fields)
        let with_fn = self.create_function(JsFunction::native(
            "with".to_string(),
            1,
            |interp, this, args| {
                let (m, d, ry, cal) = match get_md_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let item = args.first().cloned().unwrap_or(JsValue::Undefined);
                // IsPartialTemporalObject
                if let Err(c) = is_partial_temporal_object(interp, &item) {
                    return c;
                }
                // PrepareCalendarFields in alphabetical order: day, month, monthCode, year
                let mut has_any = false;
                let (new_d, has_d) = match read_field_positive_int(interp, &item, "day", d) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                has_any |= has_d;
                let (raw_month, has_m) = match read_field_positive_int(interp, &item, "month", m) {
                    Ok(v) => (Some(v.0), v.1),
                    Err(c) => return c,
                };
                let raw_month = if has_m { raw_month } else { None };
                has_any |= has_m;
                let (raw_month_code, has_mc) = match read_month_code_field(interp, &item) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                has_any |= has_mc;
                let (new_ry, has_y) = match read_field_i32(interp, &item, "year", ry) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                has_any |= has_y;
                if !has_any {
                    return Completion::Throw(
                        interp.create_type_error("with() requires at least one recognized property"),
                    );
                }
                let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let overflow = match parse_overflow_option(interp, &options) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let new_m = match resolve_month_fields(interp, raw_month, raw_month_code, m) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                // Year is used for validation only; reference year stays unchanged
                if overflow == "reject" {
                    if !iso_date_valid(ry, new_m, new_d) {
                        return Completion::Throw(interp.create_range_error("Invalid month/day"));
                    }
                    create_plain_month_day_result(interp, new_m, new_d, ry, &cal)
                } else {
                    let cm = new_m.max(1).min(12);
                    let cd = new_d.max(1).min(iso_days_in_month(ry, cm));
                    create_plain_month_day_result(interp, cm, cd, ry, &cal)
                }
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("with".to_string(), with_fn);

        // equals
        let equals_fn = self.create_function(JsFunction::native(
            "equals".to_string(),
            1,
            |interp, this, args| {
                let (m1, d1, ry1, c1) = match get_md_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let other = args.first().cloned().unwrap_or(JsValue::Undefined);
                let (m2, d2, ry2, c2) = match to_temporal_plain_month_day(interp, other) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                Completion::Normal(JsValue::Boolean(
                    m1 == m2 && d1 == d2 && ry1 == ry2 && c1 == c2,
                ))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("equals".to_string(), equals_fn);

        // toString / toJSON / toLocaleString
        let to_string_fn = self.create_function(JsFunction::native(
            "toString".to_string(),
            0,
            |interp, this, args| {
                let (m, d, ref_year, cal) = match get_md_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let options = args.first().cloned().unwrap_or(JsValue::Undefined);
                let has_opts = match super::get_options_object(interp, &options) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let show_cal_owned: String = if has_opts {
                    let cv = match get_prop(interp, &options, "calendarName") {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if is_undefined(&cv) {
                        "auto".to_string()
                    } else {
                        let s = match interp.to_string_value(&cv) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };
                        match s.as_str() {
                            "auto" | "always" | "never" | "critical" => s,
                            _ => {
                                return Completion::Throw(
                                    interp.create_range_error("Invalid calendarName"),
                                );
                            }
                        }
                    }
                } else {
                    "auto".to_string()
                };
                Completion::Normal(JsValue::String(JsString::from_str(&format_month_day(
                    m, d, ref_year, &cal, &show_cal_owned,
                ))))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toString".to_string(), to_string_fn);

        let to_json_fn = self.create_function(JsFunction::native(
            "toJSON".to_string(),
            0,
            |interp, this, _| {
                let (m, d, ref_year, cal) = match get_md_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                Completion::Normal(JsValue::String(JsString::from_str(&format_month_day(
                    m, d, ref_year, &cal, "auto",
                ))))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toJSON".to_string(), to_json_fn);

        let to_locale_fn = self.create_function(JsFunction::native(
            "toLocaleString".to_string(),
            0,
            |interp, this, _| {
                let (m, d, ref_year, cal) = match get_md_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                Completion::Normal(JsValue::String(JsString::from_str(&format_month_day(
                    m, d, ref_year, &cal, "auto",
                ))))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toLocaleString".to_string(), to_locale_fn);

        // valueOf — throws
        let value_of_fn = self.create_function(JsFunction::native(
            "valueOf".to_string(),
            0,
            |interp, _, _| {
                Completion::Throw(
                    interp.create_type_error("use equals() to compare Temporal.PlainMonthDay"),
                )
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("valueOf".to_string(), value_of_fn);

        // toPlainDate({ year })
        let to_pd_fn = self.create_function(JsFunction::native(
            "toPlainDate".to_string(),
            1,
            |interp, this, args| {
                let (m, d, _, cal) = match get_md_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let item = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(item, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("argument must be an object with a year property"),
                    );
                }
                let y_val = match get_prop(interp, &item, "year") {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if is_undefined(&y_val) {
                    return Completion::Throw(interp.create_type_error("year is required"));
                }
                let y = match interp.to_number_value(&y_val) {
                    Ok(n) => n as i32,
                    Err(e) => return Completion::Throw(e),
                };
                if !iso_date_valid(y, m, d) {
                    return Completion::Throw(interp.create_range_error("Invalid date"));
                }
                super::plain_date::create_plain_date_result(interp, y, m, d, &cal)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toPlainDate".to_string(), to_pd_fn);

        self.temporal_plain_month_day_prototype = Some(proto.clone());

        // Constructor
        let constructor = self.create_function(JsFunction::constructor(
            "PlainMonthDay".to_string(),
            2,
            |interp, _this, args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Temporal.PlainMonthDay must be called with new"),
                    );
                }
                let m_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let m = match interp.to_number_value(&m_val) {
                    Ok(n) => {
                        if !n.is_finite() {
                            return Completion::Throw(interp.create_range_error("Invalid month"));
                        }
                        let t = n.trunc();
                        if t < 1.0 || t > 12.0 {
                            return Completion::Throw(interp.create_range_error("Invalid month"));
                        }
                        t as u8
                    }
                    Err(e) => return Completion::Throw(e),
                };
                let d_val = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let d = match interp.to_number_value(&d_val) {
                    Ok(n) => {
                        if !n.is_finite() {
                            return Completion::Throw(interp.create_range_error("Invalid day"));
                        }
                        let t = n.trunc();
                        if t < 1.0 || t > 31.0 {
                            return Completion::Throw(interp.create_range_error("Invalid day"));
                        }
                        t as u8
                    }
                    Err(e) => return Completion::Throw(e),
                };
                let cal_arg = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                let cal = match super::validate_calendar_strict(interp, &cal_arg) {
                    Ok(c) => c,
                    Err(c) => return c,
                };
                let ry = if let Some(v) = args.get(3) {
                    if is_undefined(v) {
                        1972i32
                    } else {
                        match interp.to_number_value(v) {
                            Ok(n) => {
                                if !n.is_finite() {
                                    return Completion::Throw(
                                        interp.create_range_error("Invalid referenceISOYear"),
                                    );
                                }
                                n.trunc() as i32
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                } else {
                    1972i32
                };
                if !iso_date_valid(ry, m, d) {
                    return Completion::Throw(interp.create_range_error("Invalid date"));
                }
                if !super::iso_date_within_limits(ry, m, d) {
                    return Completion::Throw(
                        interp.create_range_error("Date outside valid ISO range"),
                    );
                }
                create_plain_month_day_result(interp, m, d, ry, &cal)
            },
        ));

        if let JsValue::Object(ref o) = constructor {
            if let Some(obj) = self.get_object(o.id) {
                let proto_val = JsValue::Object(crate::types::JsObject {
                    id: proto.borrow().id.unwrap(),
                });
                obj.borrow_mut().insert_property(
                    "prototype".to_string(),
                    PropertyDescriptor::data(proto_val, false, false, false),
                );
            }
        }
        proto.borrow_mut().insert_property(
            "constructor".to_string(),
            PropertyDescriptor::data(constructor.clone(), true, false, true),
        );

        // from(item, options?)
        let from_fn = self.create_function(JsFunction::native(
            "from".to_string(),
            1,
            |interp, _, args| {
                let item = args.first().cloned().unwrap_or(JsValue::Undefined);
                let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                // Per spec: if item is a string, parse first, then validate overflow (but don't use it)
                if matches!(&item, JsValue::String(_)) {
                    let (m, d, ry, cal) = match to_temporal_plain_month_day(interp, item) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    match parse_overflow_option(interp, &options) {
                        Ok(_) => {}
                        Err(c) => return c,
                    }
                    return create_plain_month_day_result(interp, m, d, ry, &cal);
                }
                // Check if it's a Temporal PlainMonthDay (read overflow first, return copy)
                let is_temporal = if let JsValue::Object(ref o) = item {
                    if let Some(obj) = interp.get_object(o.id) {
                        let data = obj.borrow();
                        matches!(&data.temporal_data, Some(TemporalData::PlainMonthDay { .. }))
                    } else {
                        false
                    }
                } else {
                    false
                };
                if is_temporal {
                    let overflow = match parse_overflow_option(interp, &options) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let (m, d, ry, cal) = match to_temporal_plain_month_day(interp, item) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let _ = overflow;
                    create_plain_month_day_result(interp, m, d, ry, &cal)
                } else {
                    // Property bag: read fields raw, then overflow, then validate
                    let (month_num, mc_str, d_f, cal, year_opt) = match read_pmd_property_bag_raw(interp, &item) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let overflow = match parse_overflow_option(interp, &options) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    // Now resolve monthCode
                    let m = match resolve_month_from_raw(interp, month_num, mc_str) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    // For ISO 8601 calendar: year from bag is used for validation only,
                    // stored referenceISOYear is always 1972 (leap year)
                    let validation_year = year_opt.unwrap_or(1972);
                    let d = d_f as u8;
                    if m < 1 {
                        return Completion::Throw(
                            interp.create_range_error("Invalid month"),
                        );
                    }
                    if overflow == "constrain" {
                        let cm = m.min(12);
                        let max_day = iso_days_in_month(validation_year, cm);
                        let cd = d.max(1).min(max_day);
                        create_plain_month_day_result(interp, cm, cd, 1972, &cal)
                    } else {
                        // reject
                        if m > 12 {
                            return Completion::Throw(
                                interp.create_range_error("Invalid month"),
                            );
                        }
                        let max_day = iso_days_in_month(validation_year, m);
                        if d < 1 || d > max_day {
                            return Completion::Throw(
                                interp.create_range_error("Invalid day"),
                            );
                        }
                        create_plain_month_day_result(interp, m, d, 1972, &cal)
                    }
                }
            },
        ));
        if let JsValue::Object(ref o) = constructor {
            if let Some(obj) = self.get_object(o.id) {
                obj.borrow_mut().insert_builtin("from".to_string(), from_fn);
            }
        }

        temporal_obj.borrow_mut().insert_property(
            "PlainMonthDay".to_string(),
            PropertyDescriptor::data(constructor, true, false, true),
        );
    }
}

fn get_opt_u8(
    interp: &mut Interpreter,
    obj: &JsValue,
    key: &str,
    default: u8,
) -> Result<u8, Completion> {
    let val = match get_prop(interp, obj, key) {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    if is_undefined(&val) {
        Ok(default)
    } else {
        Ok(to_integer_with_truncation(interp, &val)? as u8)
    }
}

fn format_month_day(m: u8, d: u8, ref_year: i32, cal: &str, show_calendar: &str) -> String {
    let mut result = format!("{m:02}-{d:02}");
    let need_year = match show_calendar {
        "always" | "critical" => true,
        "auto" if cal != "iso8601" => true,
        _ => false,
    };
    if need_year {
        let year_str = if ref_year >= 0 && ref_year <= 9999 {
            format!("{ref_year:04}")
        } else if ref_year >= 0 {
            format!("+{ref_year:06}")
        } else {
            format!("-{:06}", ref_year.unsigned_abs())
        };
        result = format!("{year_str}-{result}");
    }
    match show_calendar {
        "always" => {
            result.push_str(&format!("[u-ca={cal}]"));
        }
        "critical" => {
            result.push_str(&format!("[!u-ca={cal}]"));
        }
        "auto" => {
            if cal != "iso8601" {
                result.push_str(&format!("[u-ca={cal}]"));
            }
        }
        _ => {}
    }
    result
}
