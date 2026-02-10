use super::*;
use crate::interpreter::builtins::temporal::{
    add_iso_date, difference_iso_date, get_options_object, get_prop, is_undefined, iso_date_valid,
    iso_days_in_month, iso_days_in_year, iso_is_leap_year, iso_month_code,
    parse_difference_options, parse_overflow_option, parse_temporal_year_month_string,
    read_month_fields, resolve_month_fields, round_date_duration,
    to_temporal_calendar_slot_value, validate_calendar,
};

pub(super) fn create_plain_year_month_result(
    interp: &mut Interpreter,
    y: i32,
    m: u8,
    ref_day: u8,
    cal: &str,
) -> Completion {
    if !super::iso_year_month_within_limits(y, m) {
        return Completion::Throw(
            interp.create_range_error("PlainYearMonth outside representable range"),
        );
    }
    let obj = interp.create_object();
    obj.borrow_mut().class_name = "Temporal.PlainYearMonth".to_string();
    if let Some(ref proto) = interp.temporal_plain_year_month_prototype {
        obj.borrow_mut().prototype = Some(proto.clone());
    }
    obj.borrow_mut().temporal_data = Some(TemporalData::PlainYearMonth {
        iso_year: y,
        iso_month: m,
        reference_iso_day: ref_day,
        calendar: cal.to_string(),
    });
    let id = obj.borrow().id.unwrap();
    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
}

fn get_ym_fields(
    interp: &mut Interpreter,
    this: &JsValue,
) -> Result<(i32, u8, u8, String), Completion> {
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
                interp.create_type_error("not a Temporal.PlainYearMonth"),
            ));
        }
    };
    let data = obj.borrow();
    match &data.temporal_data {
        Some(TemporalData::PlainYearMonth {
            iso_year,
            iso_month,
            reference_iso_day,
            calendar,
        }) => Ok((*iso_year, *iso_month, *reference_iso_day, calendar.clone())),
        _ => Err(Completion::Throw(
            interp.create_type_error("not a Temporal.PlainYearMonth"),
        )),
    }
}

fn read_pym_property_bag_raw(
    interp: &mut Interpreter,
    item: &JsValue,
) -> Result<(i32, Option<f64>, Option<String>, String), Completion> {
    // Alphabetical: calendar, month, monthCode, year
    let cal_val = match get_prop(interp, item, "calendar") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let cal = to_temporal_calendar_slot_value(interp, &cal_val)?;
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
        let mc = super::to_primitive_and_require_string(interp, &mc_val, "monthCode")?;
        if !super::is_month_code_syntax_valid(&mc) {
            return Err(Completion::Throw(
                interp.create_range_error(&format!("Invalid monthCode: {mc}")),
            ));
        }
        Some(mc)
    } else {
        None
    };
    let y_val = match get_prop(interp, item, "year") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let y = if is_undefined(&y_val) {
        return Err(Completion::Throw(interp.create_type_error("year is required")));
    } else {
        to_integer_with_truncation(interp, &y_val)? as i32
    };
    Ok((y, month_num, mc_str, cal))
}

fn resolve_pym_month_from_raw(
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

fn to_temporal_plain_year_month_with_overflow(
    interp: &mut Interpreter,
    item: JsValue,
    overflow: &str,
) -> Result<(i32, u8, u8, String), Completion> {
    let (y, m, rd, cal) = to_temporal_plain_year_month(interp, item)?;
    if overflow == "constrain" {
        let cm = m.max(1).min(12);
        Ok((y, cm, rd, cal))
    } else {
        if m < 1 || m > 12 {
            return Err(Completion::Throw(
                interp.create_range_error("Invalid month"),
            ));
        }
        Ok((y, m, rd, cal))
    }
}

fn to_temporal_plain_year_month(
    interp: &mut Interpreter,
    item: JsValue,
) -> Result<(i32, u8, u8, String), Completion> {
    match &item {
        JsValue::Object(o) => {
            if let Some(obj) = interp.get_object(o.id) {
                let data = obj.borrow();
                if let Some(TemporalData::PlainYearMonth {
                    iso_year,
                    iso_month,
                    reference_iso_day,
                    calendar,
                }) = &data.temporal_data
                {
                    return Ok((*iso_year, *iso_month, *reference_iso_day, calendar.clone()));
                }
            }
            // Alphabetical order: calendar, month, monthCode, year
            let cal_val = match get_prop(interp, &item, "calendar") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let cal = to_temporal_calendar_slot_value(interp, &cal_val)?;
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
                let mc = super::to_primitive_and_require_string(interp, &mc_val, "monthCode")?;
                if !super::is_month_code_syntax_valid(&mc) {
                    return Err(Completion::Throw(
                        interp.create_range_error(&format!("Invalid monthCode: {mc}")),
                    ));
                }
                Some(mc)
            } else {
                None
            };
            let y_val = match get_prop(interp, &item, "year") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let y = if is_undefined(&y_val) {
                return Err(Completion::Throw(
                    interp.create_type_error("year is required"),
                ));
            } else {
                to_integer_with_truncation(interp, &y_val)? as i32
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
            Ok((y, m, 1, cal))
        }
        JsValue::String(s) => {
            let parsed = match parse_temporal_year_month_string(&s.to_rust_string()) {
                Some(v) => v,
                None => {
                    return Err(Completion::Throw(interp.create_range_error(&format!(
                        "Invalid year-month string: {}",
                        s.to_rust_string()
                    ))));
                }
            };
            // PlainYearMonth does not accept UTC designator
            if parsed.3 {
                return Err(Completion::Throw(interp.create_range_error(
                    "UTC designator Z is not allowed in a PlainYearMonth string",
                )));
            }
            // Date-only string with UTC offset is not valid
            if parsed.4 {
                return Err(Completion::Throw(interp.create_range_error(
                    "UTC offset without time is not valid for PlainYearMonth",
                )));
            }
            let cal = parsed.2.unwrap_or_else(|| "iso8601".to_string());
            let cal = match validate_calendar(&cal) {
                Some(c) => c,
                None => {
                    return Err(Completion::Throw(
                        interp.create_range_error(&format!("Invalid calendar: {cal}")),
                    ));
                }
            };
            if !super::iso_year_month_within_limits(parsed.0, parsed.1) {
                return Err(Completion::Throw(
                    interp.create_range_error("Date outside representable range"),
                ));
            }
            Ok((parsed.0, parsed.1, 1, cal))
        }
        _ => Err(Completion::Throw(
            interp.create_type_error("Cannot convert to Temporal.PlainYearMonth"),
        )),
    }
}

impl Interpreter {
    pub(crate) fn setup_temporal_plain_year_month(
        &mut self,
        temporal_obj: &Rc<RefCell<JsObjectData>>,
    ) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Temporal.PlainYearMonth".to_string();
        {
            let key = "Symbol(Symbol.toStringTag)".to_string();
            let desc = PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str(
                    "Temporal.PlainYearMonth",
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

        // Getters: calendarId, year, month, monthCode
        {
            let getter = self.create_function(JsFunction::native(
                "get calendarId".to_string(),
                0,
                |interp, this, _| {
                    let (_, _, _, cal) = match get_ym_fields(interp, &this) {
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
        for &(name, idx) in &[("year", 0u8), ("month", 1)] {
            let getter = self.create_function(JsFunction::native(
                format!("get {name}"),
                0,
                move |interp, this, _| {
                    let (y, m, _, _) = match get_ym_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    Completion::Normal(JsValue::Number(if idx == 0 { y as f64 } else { m as f64 }))
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
        {
            let getter = self.create_function(JsFunction::native(
                "get monthCode".to_string(),
                0,
                |interp, this, _| {
                    let (_, m, _, _) = match get_ym_fields(interp, &this) {
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

        // Computed getters
        for &(name, which) in &[
            ("daysInMonth", 0u8),
            ("daysInYear", 1),
            ("monthsInYear", 2),
            ("inLeapYear", 3),
            ("era", 4),
            ("eraYear", 5),
        ] {
            let getter = self.create_function(JsFunction::native(
                format!("get {name}"),
                0,
                move |interp, this, _| {
                    let (y, m, _, _) = match get_ym_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    match which {
                        0 => Completion::Normal(JsValue::Number(iso_days_in_month(y, m) as f64)),
                        1 => Completion::Normal(JsValue::Number(iso_days_in_year(y) as f64)),
                        2 => Completion::Normal(JsValue::Number(12.0)),
                        3 => Completion::Normal(JsValue::Boolean(iso_is_leap_year(y))),
                        4 => Completion::Normal(JsValue::Undefined),
                        _ => Completion::Normal(JsValue::Undefined),
                    }
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

        // with(fields, options?)
        let with_fn = self.create_function(JsFunction::native(
            "with".to_string(),
            1,
            |interp, this, args| {
                let (y, m, rd, cal) = match get_ym_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let item = args.first().cloned().unwrap_or(JsValue::Undefined);
                // IsPartialTemporalObject
                if let Err(c) = is_partial_temporal_object(interp, &item) {
                    return c;
                }
                // PrepareCalendarFields in alphabetical order: month, monthCode, year
                let mut has_any = false;
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
                let (new_y, has_y) = match read_field_i32(interp, &item, "year", y) {
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
                if overflow == "reject" {
                    if new_m < 1 || new_m > 12 {
                        return Completion::Throw(interp.create_range_error("Invalid month"));
                    }
                    create_plain_year_month_result(interp, new_y, new_m, rd, &cal)
                } else {
                    let cm = new_m.max(1).min(12);
                    create_plain_year_month_result(interp, new_y, cm, rd, &cal)
                }
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("with".to_string(), with_fn);

        // add / subtract
        for &(name, sign) in &[("add", 1i32), ("subtract", -1i32)] {
            let fn_val = self.create_function(JsFunction::native(
                name.to_string(),
                1,
                move |interp, this, args| {
                    let (y, m, rd, cal) = match get_ym_fields(interp, &this) {
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
                    let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    let _overflow = match parse_overflow_option(interp, &options) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    // Check ISODateWithinLimits on intermediate date (day=rd)
                    if !super::iso_date_within_limits(y, m, rd) {
                        return Completion::Throw(
                            interp.create_range_error("PlainYearMonth intermediate date outside valid ISO range"),
                        );
                    }
                    // Check no sub-month units per AddDurationToYearMonth
                    let time_ns: i128 = (dur.3 as i128) * 86_400_000_000_000
                        + (dur.4 as i128) * 3_600_000_000_000
                        + (dur.5 as i128) * 60_000_000_000
                        + (dur.6 as i128) * 1_000_000_000
                        + (dur.7 as i128) * 1_000_000
                        + (dur.8 as i128) * 1_000
                        + (dur.9 as i128);
                    if time_ns != 0 {
                        return Completion::Throw(
                            interp.create_range_error("Duration days/time must be zero for PlainYearMonth arithmetic"),
                        );
                    }
                    if dur.2 != 0.0 {
                        return Completion::Throw(
                            interp.create_range_error("Duration weeks must be zero for PlainYearMonth arithmetic"),
                        );
                    }
                    let (ry, rm, _) = add_iso_date(
                        y,
                        m,
                        rd,
                        (dur.0 as i32) * sign,
                        (dur.1 as i32) * sign,
                        0,
                        0,
                    );
                    let cm = rm.max(1).min(12);
                    let final_rd = rd.min(iso_days_in_month(ry, cm));
                    if !super::iso_date_within_limits(ry, cm, final_rd) {
                        return Completion::Throw(
                            interp.create_range_error("Result year-month outside valid ISO range"),
                        );
                    }
                    create_plain_year_month_result(interp, ry, cm, final_rd, &cal)
                },
            ));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
        }

        // until / since
        for &(name, sign) in &[("until", 1i32), ("since", -1i32)] {
            let fn_val = self.create_function(JsFunction::native(
                name.to_string(),
                1,
                move |interp, this, args| {
                    let (y1, m1, rd1, _) = match get_ym_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let other = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let (y2, m2, rd2, _) = match to_temporal_plain_year_month(interp, other) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    // Step 6: early return if both ISO dates are equal
                    if y1 == y2 && m1 == m2 && rd1 == rd2 {
                        return super::duration::create_duration_result(
                            interp, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                        );
                    }
                    // Steps 9,12: create PlainDates from day 1 — check limits
                    if !super::iso_date_within_limits(y1, m1, 1)
                        || !super::iso_date_within_limits(y2, m2, 1)
                    {
                        return Completion::Throw(
                            interp.create_range_error("PlainYearMonth outside representable range for since/until"),
                        );
                    }
                    let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    let ym_units: &[&str] = &["year", "month"];
                    let (largest_unit, smallest_unit, rounding_mode, rounding_increment) =
                        match parse_difference_options(interp, &options, "year", ym_units) {
                            Ok(v) => v,
                            Err(c) => return c,
                        };

                    let (mut dy, mut dm, _, _) =
                        difference_iso_date(y1, m1, rd1, y2, m2, rd1, &largest_unit);

                    let effective_mode = if sign == -1 {
                        negate_rounding_mode(&rounding_mode)
                    } else {
                        rounding_mode.clone()
                    };

                    if smallest_unit != "month" || rounding_increment != 1.0 || rounding_mode != "trunc" {
                        let (ry, rm, rd) = (y1, m1, rd1);
                        // Pre-check: verify NudgeToCalendarUnit boundary is within ISO range
                        if matches!(smallest_unit.as_str(), "month" | "year") {
                            let dur_sign = if dy > 0 || dm > 0 { 1i64 }
                                else if dy < 0 || dm < 0 { -1i64 }
                                else { 1 };
                            let inc = rounding_increment as i64;
                            let end_date = match smallest_unit.as_str() {
                                "month" => {
                                    let end_m = dm as i64 + dur_sign * inc;
                                    super::add_iso_date(ry, rm, rd, dy, end_m as i32, 0, 0)
                                }
                                _ => {
                                    let end_y = dy as i64 + dur_sign * inc;
                                    super::add_iso_date(ry, rm, rd, end_y as i32, 0, 0, 0)
                                }
                            };
                            if !super::iso_date_within_limits(end_date.0, end_date.1, end_date.2) {
                                return Completion::Throw(
                                    interp.create_range_error("Rounded date outside valid ISO range"),
                                );
                            }
                        }
                        let (ry2, rm2, _, _) = round_date_duration(
                            dy, dm, 0, 0,
                            &smallest_unit, &largest_unit, rounding_increment, &effective_mode,
                            ry, rm, rd,
                        );
                        dy = ry2;
                        dm = rm2;
                        // Check that rounded date is within valid ISO range
                        let rounded_end = add_iso_date(ry, rm, rd, dy, dm, 0, 0);
                        if !iso_date_within_limits(rounded_end.0, rounded_end.1, rounded_end.2) {
                            return Completion::Throw(
                                interp.create_range_error("Rounded date outside valid ISO range"),
                            );
                        }
                        // Rebalance months overflow into years when largestUnit is year
                        if matches!(largest_unit.as_str(), "year") && dm.abs() >= 12 {
                            dy += dm / 12;
                            dm %= 12;
                        }
                    }

                    if sign == -1 {
                        dy = -dy; dm = -dm;
                    }

                    super::duration::create_duration_result(
                        interp, dy as f64, dm as f64, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                    )
                },
            ));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
        }

        // equals
        let equals_fn = self.create_function(JsFunction::native(
            "equals".to_string(),
            1,
            |interp, this, args| {
                let (y1, m1, rd1, c1) = match get_ym_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let other = args.first().cloned().unwrap_or(JsValue::Undefined);
                let (y2, m2, rd2, c2) = match to_temporal_plain_year_month(interp, other) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                Completion::Normal(JsValue::Boolean(
                    y1 == y2 && m1 == m2 && rd1 == rd2 && c1 == c2,
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
                let (y, m, ref_day, cal) = match get_ym_fields(interp, &this) {
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
                let result = format_year_month(y, m, ref_day as u8, &cal, &show_cal_owned);
                Completion::Normal(JsValue::String(JsString::from_str(&result)))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toString".to_string(), to_string_fn);

        let to_json_fn = self.create_function(JsFunction::native(
            "toJSON".to_string(),
            0,
            |interp, this, _| {
                let (y, m, ref_day, cal) = match get_ym_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                Completion::Normal(JsValue::String(JsString::from_str(&format_year_month(
                    y, m, ref_day as u8, &cal, "auto",
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
                let (y, m, ref_day, cal) = match get_ym_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                Completion::Normal(JsValue::String(JsString::from_str(&format_year_month(
                    y, m, ref_day as u8, &cal, "auto",
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
                Completion::Throw(interp.create_type_error(
                    "use compare() or equals() to compare Temporal.PlainYearMonth",
                ))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("valueOf".to_string(), value_of_fn);

        // toPlainDate({ day })
        let to_pd_fn = self.create_function(JsFunction::native(
            "toPlainDate".to_string(),
            1,
            |interp, this, args| {
                let (y, m, _, cal) = match get_ym_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let item = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(item, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("argument must be an object with a day property"),
                    );
                }
                let d_val = match get_prop(interp, &item, "day") {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if is_undefined(&d_val) {
                    return Completion::Throw(interp.create_type_error("day is required"));
                }
                let d = match to_integer_with_truncation(interp, &d_val) {
                    Ok(n) => n as u8,
                    Err(c) => return c,
                };
                // Constrain day to valid range for the given year/month
                let max_day = iso_days_in_month(y, m);
                let cd = d.min(max_day);
                if !super::iso_date_within_limits(y, m, cd) {
                    return Completion::Throw(
                        interp.create_range_error("Date outside valid ISO range"),
                    );
                }
                super::plain_date::create_plain_date_result(interp, y, m, cd, &cal)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toPlainDate".to_string(), to_pd_fn);

        self.temporal_plain_year_month_prototype = Some(proto.clone());

        // Constructor
        let constructor = self.create_function(JsFunction::constructor(
            "PlainYearMonth".to_string(),
            2,
            |interp, _this, args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Temporal.PlainYearMonth must be called with new"),
                    );
                }
                let y_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let y = match interp.to_number_value(&y_val) {
                    Ok(n) => {
                        if !n.is_finite() {
                            return Completion::Throw(interp.create_range_error("Invalid year"));
                        }
                        n.trunc() as i32
                    }
                    Err(e) => return Completion::Throw(e),
                };
                let m_val = args.get(1).cloned().unwrap_or(JsValue::Undefined);
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
                let cal_arg = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                let cal = match super::validate_calendar_strict(interp, &cal_arg) {
                    Ok(c) => c,
                    Err(c) => return c,
                };
                let rd = if let Some(v) = args.get(3) {
                    if is_undefined(v) {
                        1u8
                    } else {
                        match interp.to_number_value(v) {
                            Ok(n) => {
                                if !n.is_finite() {
                                    return Completion::Throw(
                                        interp.create_range_error("Invalid referenceISODay"),
                                    );
                                }
                                let t = n.trunc();
                                if t < 1.0 || t > 31.0 {
                                    return Completion::Throw(
                                        interp.create_range_error("Invalid referenceISODay"),
                                    );
                                }
                                t as u8
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                } else {
                    1u8
                };
                if !super::iso_date_valid(y, m, rd) {
                    return Completion::Throw(
                        interp.create_range_error("Invalid ISO date for PlainYearMonth"),
                    );
                }
                if !super::iso_year_month_within_limits(y, m) {
                    return Completion::Throw(
                        interp.create_range_error("Date outside valid ISO range"),
                    );
                }
                create_plain_year_month_result(interp, y, m, rd, &cal)
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
                    let (y, m, rd, cal) = match to_temporal_plain_year_month_with_overflow(
                        interp,
                        item,
                        "constrain",
                    ) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    match parse_overflow_option(interp, &options) {
                        Ok(_) => {}
                        Err(c) => return c,
                    }
                    return create_plain_year_month_result(interp, y, m, rd, &cal);
                }
                // Check if it's a Temporal PlainYearMonth (read overflow first, return copy)
                let is_temporal = if let JsValue::Object(ref o) = item {
                    if let Some(obj) = interp.get_object(o.id) {
                        let data = obj.borrow();
                        matches!(&data.temporal_data, Some(TemporalData::PlainYearMonth { .. }))
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
                    let (y, m, rd, cal) =
                        match to_temporal_plain_year_month_with_overflow(interp, item, &overflow) {
                            Ok(v) => v,
                            Err(c) => return c,
                        };
                    create_plain_year_month_result(interp, y, m, rd, &cal)
                } else {
                    // Property bag: read fields raw, then overflow, then validate
                    let (y, month_num, mc_str, cal) = match read_pym_property_bag_raw(interp, &item) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let overflow = match parse_overflow_option(interp, &options) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let m = match resolve_pym_month_from_raw(interp, month_num, mc_str) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let (y, m, rd, cal) = if overflow == "constrain" {
                        if m < 1 {
                            return Completion::Throw(
                                interp.create_range_error("Invalid month"),
                            );
                        }
                        let cm = m.min(12);
                        (y, cm, 1, cal)
                    } else {
                        if m < 1 || m > 12 {
                            return Completion::Throw(
                                interp.create_range_error("Invalid month"),
                            );
                        }
                        (y, m, 1, cal)
                    };
                    create_plain_year_month_result(interp, y, m, rd, &cal)
                }
            },
        ));
        if let JsValue::Object(ref o) = constructor {
            if let Some(obj) = self.get_object(o.id) {
                obj.borrow_mut().insert_builtin("from".to_string(), from_fn);
            }
        }

        // compare
        let compare_fn = self.create_function(JsFunction::native(
            "compare".to_string(),
            2,
            |interp, _, args| {
                let (y1, m1, rd1, _) = match to_temporal_plain_year_month(
                    interp,
                    args.first().cloned().unwrap_or(JsValue::Undefined),
                ) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let (y2, m2, rd2, _) = match to_temporal_plain_year_month(
                    interp,
                    args.get(1).cloned().unwrap_or(JsValue::Undefined),
                ) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let r = if y1 != y2 {
                    if y1 < y2 { -1.0 } else { 1.0 }
                } else if m1 != m2 {
                    if m1 < m2 { -1.0 } else { 1.0 }
                } else if rd1 != rd2 {
                    if rd1 < rd2 { -1.0 } else { 1.0 }
                } else {
                    0.0
                };
                Completion::Normal(JsValue::Number(r))
            },
        ));
        if let JsValue::Object(ref o) = constructor {
            if let Some(obj) = self.get_object(o.id) {
                obj.borrow_mut()
                    .insert_builtin("compare".to_string(), compare_fn);
            }
        }

        temporal_obj.borrow_mut().insert_property(
            "PlainYearMonth".to_string(),
            PropertyDescriptor::data(constructor, true, false, true),
        );
    }
}

fn get_opt_i32(
    interp: &mut Interpreter,
    obj: &JsValue,
    key: &str,
    default: i32,
) -> Result<i32, Completion> {
    let val = match get_prop(interp, obj, key) {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    if is_undefined(&val) {
        Ok(default)
    } else {
        Ok(to_integer_with_truncation(interp, &val)? as i32)
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

fn format_year_month(y: i32, m: u8, ref_day: u8, cal: &str, show_calendar: &str) -> String {
    let year_str = if y >= 0 && y <= 9999 {
        format!("{y:04}")
    } else if y >= 0 {
        format!("+{y:06}")
    } else {
        format!("-{:06}", y.unsigned_abs())
    };
    let mut result = format!("{year_str}-{m:02}");
    let need_day = match show_calendar {
        "always" | "critical" => true,
        "auto" if cal != "iso8601" => true,
        _ => false,
    };
    if need_day {
        result.push_str(&format!("-{ref_day:02}"));
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
