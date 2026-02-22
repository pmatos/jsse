use super::*;
use crate::interpreter::builtins::temporal::{
    add_iso_date, add_iso_date_with_overflow, difference_iso_date, get_prop, is_undefined,
    iso_date_valid, iso_day_of_week, iso_day_of_year, iso_days_in_month, iso_days_in_year,
    iso_is_leap_year, iso_month_code, iso_week_of_year, parse_difference_options,
    parse_overflow_option, parse_temporal_date_time_string, resolve_month_fields,
    round_date_duration, to_temporal_calendar_slot_value, validate_calendar,
};

impl Interpreter {
    pub(crate) fn setup_temporal_plain_date(&mut self, temporal_obj: &Rc<RefCell<JsObjectData>>) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Temporal.PlainDate".to_string();
        {
            let key = "Symbol(Symbol.toStringTag)".to_string();
            let desc = PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("Temporal.PlainDate"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            };
            proto.borrow_mut().property_order.push(key.clone());
            proto.borrow_mut().properties.insert(key, desc);
        }

        // Getter: calendarId
        {
            let getter = self.create_function(JsFunction::native(
                "get calendarId".to_string(),
                0,
                |interp, this, _args| {
                    let (_, _, _, cal) = match get_plain_date_fields(interp, &this) {
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

        // Getters: year, month, day
        for &(name, idx) in &[("year", 0u8), ("month", 1), ("day", 2)] {
            let getter = self.create_function(JsFunction::native(
                format!("get {name}"),
                0,
                move |interp, this, _args| {
                    let (y, m, d, cal) = match get_plain_date_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    if cal != "iso8601" {
                        if let Some(cf) = super::iso_to_calendar_fields(y, m, d, &cal) {
                            let val = match idx {
                                0 => cf.year as f64,
                                1 => cf.month_ordinal as f64,
                                _ => cf.day as f64,
                            };
                            return Completion::Normal(JsValue::Number(val));
                        }
                    }
                    let val = match idx {
                        0 => y as f64,
                        1 => m as f64,
                        _ => d as f64,
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

        // Getter: monthCode
        {
            let getter = self.create_function(JsFunction::native(
                "get monthCode".to_string(),
                0,
                |interp, this, _args| {
                    let (y, m, d, cal) = match get_plain_date_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    if cal != "iso8601" {
                        if let Some(cf) = super::iso_to_calendar_fields(y, m, d, &cal) {
                            return Completion::Normal(JsValue::String(JsString::from_str(&cf.month_code)));
                        }
                    }
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

        // Getter: dayOfWeek
        {
            let getter = self.create_function(JsFunction::native(
                "get dayOfWeek".to_string(),
                0,
                |interp, this, _args| {
                    let (y, m, d, _) = match get_plain_date_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    Completion::Normal(JsValue::Number(iso_day_of_week(y, m, d) as f64))
                },
            ));
            proto.borrow_mut().insert_property(
                "dayOfWeek".to_string(),
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

        // Getter: dayOfYear
        {
            let getter = self.create_function(JsFunction::native(
                "get dayOfYear".to_string(),
                0,
                |interp, this, _args| {
                    let (y, m, d, cal) = match get_plain_date_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    if cal != "iso8601" {
                        if let Some(cf) = super::iso_to_calendar_fields(y, m, d, &cal) {
                            return Completion::Normal(JsValue::Number(cf.day_of_year as f64));
                        }
                    }
                    Completion::Normal(JsValue::Number(iso_day_of_year(y, m, d) as f64))
                },
            ));
            proto.borrow_mut().insert_property(
                "dayOfYear".to_string(),
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

        // Getter: weekOfYear
        {
            let getter = self.create_function(JsFunction::native(
                "get weekOfYear".to_string(),
                0,
                |interp, this, _args| {
                    let (y, m, d, _) = match get_plain_date_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let (week, _) = iso_week_of_year(y, m, d);
                    Completion::Normal(JsValue::Number(week as f64))
                },
            ));
            proto.borrow_mut().insert_property(
                "weekOfYear".to_string(),
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

        // Getter: yearOfWeek
        {
            let getter = self.create_function(JsFunction::native(
                "get yearOfWeek".to_string(),
                0,
                |interp, this, _args| {
                    let (y, m, d, _) = match get_plain_date_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let (_, year_of_week) = iso_week_of_year(y, m, d);
                    Completion::Normal(JsValue::Number(year_of_week as f64))
                },
            ));
            proto.borrow_mut().insert_property(
                "yearOfWeek".to_string(),
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

        // Getter: daysInWeek (always 7 for iso8601)
        {
            let getter = self.create_function(JsFunction::native(
                "get daysInWeek".to_string(),
                0,
                |interp, this, _args| {
                    let _ = match get_plain_date_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    Completion::Normal(JsValue::Number(7.0))
                },
            ));
            proto.borrow_mut().insert_property(
                "daysInWeek".to_string(),
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

        // Getter: daysInMonth
        {
            let getter = self.create_function(JsFunction::native(
                "get daysInMonth".to_string(),
                0,
                |interp, this, _args| {
                    let (y, m, d, cal) = match get_plain_date_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    if cal != "iso8601" {
                        if let Some(cf) = super::iso_to_calendar_fields(y, m, d, &cal) {
                            return Completion::Normal(JsValue::Number(cf.days_in_month as f64));
                        }
                    }
                    Completion::Normal(JsValue::Number(iso_days_in_month(y, m) as f64))
                },
            ));
            proto.borrow_mut().insert_property(
                "daysInMonth".to_string(),
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

        // Getter: daysInYear
        {
            let getter = self.create_function(JsFunction::native(
                "get daysInYear".to_string(),
                0,
                |interp, this, _args| {
                    let (y, m, d, cal) = match get_plain_date_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    if cal != "iso8601" {
                        if let Some(cf) = super::iso_to_calendar_fields(y, m, d, &cal) {
                            return Completion::Normal(JsValue::Number(cf.days_in_year as f64));
                        }
                    }
                    Completion::Normal(JsValue::Number(iso_days_in_year(y) as f64))
                },
            ));
            proto.borrow_mut().insert_property(
                "daysInYear".to_string(),
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

        // Getter: monthsInYear
        {
            let getter = self.create_function(JsFunction::native(
                "get monthsInYear".to_string(),
                0,
                |interp, this, _args| {
                    let (y, m, d, cal) = match get_plain_date_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    if cal != "iso8601" {
                        if let Some(cf) = super::iso_to_calendar_fields(y, m, d, &cal) {
                            return Completion::Normal(JsValue::Number(cf.months_in_year as f64));
                        }
                    }
                    Completion::Normal(JsValue::Number(12.0))
                },
            ));
            proto.borrow_mut().insert_property(
                "monthsInYear".to_string(),
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

        // Getter: inLeapYear
        {
            let getter = self.create_function(JsFunction::native(
                "get inLeapYear".to_string(),
                0,
                |interp, this, _args| {
                    let (y, m, d, cal) = match get_plain_date_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    if cal != "iso8601" {
                        if let Some(cf) = super::iso_to_calendar_fields(y, m, d, &cal) {
                            return Completion::Normal(JsValue::Boolean(cf.in_leap_year));
                        }
                    }
                    Completion::Normal(JsValue::Boolean(iso_is_leap_year(y)))
                },
            ));
            proto.borrow_mut().insert_property(
                "inLeapYear".to_string(),
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

        // Getter: era
        {
            let getter = self.create_function(JsFunction::native(
                "get era".to_string(),
                0,
                |interp, this, _args| {
                    let (y, m, d, cal) = match get_plain_date_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    if cal != "iso8601" {
                        if let Some(cf) = super::iso_to_calendar_fields(y, m, d, &cal) {
                            if let Some(era) = cf.era {
                                return Completion::Normal(JsValue::String(JsString::from_str(&era)));
                            }
                        }
                    }
                    Completion::Normal(JsValue::Undefined)
                },
            ));
            proto.borrow_mut().insert_property(
                "era".to_string(),
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

        // Getter: eraYear
        {
            let getter = self.create_function(JsFunction::native(
                "get eraYear".to_string(),
                0,
                |interp, this, _args| {
                    let (y, m, d, cal) = match get_plain_date_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    if cal != "iso8601" {
                        if let Some(cf) = super::iso_to_calendar_fields(y, m, d, &cal) {
                            if let Some(ey) = cf.era_year {
                                return Completion::Normal(JsValue::Number(ey as f64));
                            }
                        }
                    }
                    Completion::Normal(JsValue::Undefined)
                },
            ));
            proto.borrow_mut().insert_property(
                "eraYear".to_string(),
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

        // with(temporalDateLike, options?)
        let with_fn = self.create_function(JsFunction::native(
            "with".to_string(),
            1,
            |interp, this, args| {
                let (y, m, d, cal) = match get_plain_date_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let item = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) = is_partial_temporal_object(interp, &item) {
                    return c;
                }
                let overflow = match parse_overflow_option(
                    interp,
                    &args.get(1).cloned().unwrap_or(JsValue::Undefined),
                ) {
                    Ok(v) => v,
                    Err(c) => return c,
                };

                // For non-ISO calendars, work in calendar-relative space
                if cal != "iso8601" {
                    if let Some(cf) = super::iso_to_calendar_fields(y, m, d, &cal) {
                        let mut has_any = false;
                        let (new_d, has_d) =
                            match read_field_positive_int(interp, &item, "day", cf.day) {
                                Ok(v) => v,
                                Err(c) => return c,
                            };
                        has_any |= has_d;
                        let (raw_month, has_m) = match read_field_positive_int(
                            interp,
                            &item,
                            "month",
                            cf.month_ordinal,
                        ) {
                            Ok(v) => (Some(v.0), v.1),
                            Err(c) => return c,
                        };
                        let raw_month = if has_m { raw_month } else { None };
                        has_any |= has_m;
                        let (raw_month_code, has_mc) =
                            match read_month_code_field(interp, &item) {
                                Ok(v) => v,
                                Err(c) => return c,
                            };
                        has_any |= has_mc;
                        let (new_y, has_y) =
                            match read_field_i32(interp, &item, "year", cf.year) {
                                Ok(v) => v,
                                Err(c) => return c,
                            };
                        has_any |= has_y;
                        // Also check era/eraYear for calendars with eras
                        let era_val = match super::get_prop(interp, &item, "era") {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let has_era = !super::is_undefined(&era_val);
                        has_any |= has_era;
                        let era_year_val = match super::get_prop(interp, &item, "eraYear") {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let has_era_year = !super::is_undefined(&era_year_val);
                        has_any |= has_era_year;

                        if !has_any {
                            return Completion::Throw(
                                interp.create_type_error(
                                    "with() requires at least one recognized property",
                                ),
                            );
                        }

                        // Determine month_code and month_ordinal for ICU
                        let mc_for_icu = if has_mc {
                            raw_month_code.clone()
                        } else if !has_m {
                            Some(cf.month_code.clone())
                        } else {
                            None
                        };
                        let mo_for_icu = if has_m { raw_month } else { None };

                        // Determine era for ICU
                        let (icu_era, icu_year) =
                            if super::calendar_has_eras(&cal) && has_era && has_era_year {
                                let era_str = match super::to_primitive_and_require_string(
                                    interp, &era_val, "era",
                                ) {
                                    Ok(v) => v,
                                    Err(c) => return c,
                                };
                                let ey = match super::to_integer_with_truncation(
                                    interp,
                                    &era_year_val,
                                ) {
                                    Ok(v) => v as i32,
                                    Err(c) => return c,
                                };
                                (Some(era_str), ey)
                            } else {
                                (None, new_y)
                            };

                        match super::calendar_fields_to_iso(
                            icu_era.as_deref(),
                            icu_year,
                            mc_for_icu.as_deref(),
                            mo_for_icu,
                            new_d,
                            &cal,
                        ) {
                            Some((iso_y, iso_m, iso_d)) => {
                                if !super::iso_date_within_limits(iso_y, iso_m, iso_d) {
                                    return Completion::Throw(
                                        interp.create_range_error(
                                            "Date outside valid ISO range",
                                        ),
                                    );
                                }
                                return create_plain_date_result(
                                    interp, iso_y, iso_m, iso_d, &cal,
                                );
                            }
                            None => {
                                return Completion::Throw(
                                    interp.create_range_error("Invalid calendar date"),
                                );
                            }
                        }
                    }
                }

                // ISO path
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
                let (new_y, has_y) = match read_field_i32(interp, &item, "year", y) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                has_any |= has_y;
                if !has_any {
                    return Completion::Throw(
                        interp
                            .create_type_error("with() requires at least one recognized property"),
                    );
                }
                let new_m = match resolve_month_fields(interp, raw_month, raw_month_code, m) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if new_y < -271821 || new_y > 275760 {
                    return Completion::Throw(interp.create_range_error("year out of range"));
                }
                if overflow == "reject" {
                    if !iso_date_valid(new_y, new_m, new_d) {
                        return Completion::Throw(interp.create_range_error("Invalid date fields"));
                    }
                    create_plain_date_result(interp, new_y, new_m, new_d, &cal)
                } else {
                    let cm = new_m.max(1).min(12);
                    let cd = new_d.max(1).min(iso_days_in_month(new_y, cm));
                    create_plain_date_result(interp, new_y, cm, cd, &cal)
                }
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("with".to_string(), with_fn);

        // add(temporalDuration, options?) / subtract(temporalDuration, options?)
        for &(name, sign) in &[("add", 1i32), ("subtract", -1i32)] {
            let fn_val = self.create_function(JsFunction::native(
                name.to_string(),
                1,
                move |interp, this, args| {
                    let (y, m, d, cal) = match get_plain_date_fields(interp, &this) {
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
                    let overflow = match parse_overflow_option(
                        interp,
                        &args.get(1).cloned().unwrap_or(JsValue::Undefined),
                    ) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    // Balance time components into extra days using i128 for precision
                    let time_ns: i128 = (dur.4 as i128) * 3_600_000_000_000
                        + (dur.5 as i128) * 60_000_000_000
                        + (dur.6 as i128) * 1_000_000_000
                        + (dur.7 as i128) * 1_000_000
                        + (dur.8 as i128) * 1_000
                        + (dur.9 as i128);
                    let extra_days = time_ns / 86_400_000_000_000;
                    let total_days = dur.3 as i64 + extra_days as i64;
                    let years = (dur.0 * sign as f64) as i32;
                    let months = (dur.1 * sign as f64) as i32;
                    let weeks = (dur.2 * sign as f64) as i32;
                    let days = (total_days as i32) * sign;

                    // Non-ISO calendar: use calendar-aware addition
                    if cal != "iso8601" && (years != 0 || months != 0) {
                        match super::add_calendar_date(
                            y, m, d, years, months, weeks, days, &cal, &overflow,
                        ) {
                            Some((ry, rm, rd)) => {
                                if !super::iso_date_within_limits(ry, rm, rd) {
                                    return Completion::Throw(
                                        interp.create_range_error(
                                            "Result date outside valid ISO range",
                                        ),
                                    );
                                }
                                return create_plain_date_result(interp, ry, rm, rd, &cal);
                            }
                            None => {
                                return Completion::Throw(
                                    interp.create_range_error("Date out of range"),
                                );
                            }
                        }
                    }

                    let (ry, rm, rd) = match add_iso_date_with_overflow(
                        y, m, d, years, months, weeks, days, &overflow,
                    ) {
                        Ok(v) => v,
                        Err(()) => {
                            return Completion::Throw(
                                interp.create_range_error("Date out of range"),
                            );
                        }
                    };
                    if !super::iso_date_within_limits(ry, rm, rd) {
                        return Completion::Throw(
                            interp.create_range_error("Result date outside valid ISO range"),
                        );
                    }
                    create_plain_date_result(interp, ry, rm, rd, &cal)
                },
            ));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
        }

        // until(other, options?) / since(other, options?)
        for &(name, sign) in &[("until", 1i32), ("since", -1i32)] {
            let fn_val = self.create_function(JsFunction::native(
                name.to_string(),
                1,
                move |interp, this, args| {
                    let (y1, m1, d1, cal) = match get_plain_date_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let other = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let (y2, mut m2, mut d2, _) = match to_temporal_plain_date(interp, other) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    m2 = m2.max(1).min(12);
                    d2 = d2.max(1).min(iso_days_in_month(y2, m2));
                    let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    let date_units: &[&str] = &["year", "month", "week", "day"];
                    let (largest_unit, smallest_unit, rounding_mode, rounding_increment) =
                        match parse_difference_options(interp, &options, "day", date_units) {
                            Ok(v) => v,
                            Err(c) => return c,
                        };

                    let (mut dy, mut dm, mut dw, mut dd) =
                        if cal != "iso8601"
                            && matches!(largest_unit.as_str(), "year" | "month")
                        {
                            match super::difference_calendar_date(
                                y1,
                                m1,
                                d1,
                                y2,
                                m2,
                                d2,
                                &largest_unit,
                                &cal,
                            ) {
                                Some(v) => v,
                                None => difference_iso_date(y1, m1, d1, y2, m2, d2, &largest_unit),
                            }
                        } else {
                            difference_iso_date(y1, m1, d1, y2, m2, d2, &largest_unit)
                        };

                    // Per spec: for since, negate rounding mode, round signed values, then negate result
                    let effective_mode = if sign == -1 {
                        negate_rounding_mode(&rounding_mode)
                    } else {
                        rounding_mode.clone()
                    };

                    // Apply rounding on signed values
                    if smallest_unit != "day"
                        || rounding_increment != 1.0
                        || rounding_mode != "trunc"
                    {
                        let (ry, rm, rd) = (y1, m1, d1);
                        // Pre-check: NudgeToCalendarUnit end boundary within limits
                        if matches!(smallest_unit.as_str(), "month" | "year") {
                            let dur_sign = if dy > 0 || dm > 0 || dw > 0 || dd > 0 {
                                1i64
                            } else if dy < 0 || dm < 0 || dw < 0 || dd < 0 {
                                -1i64
                            } else {
                                1
                            };
                            let inc = rounding_increment as i64;
                            let end_date = match smallest_unit.as_str() {
                                "month" => {
                                    let end_m = dm as i64 + dur_sign * inc;
                                    add_iso_date(ry, rm, rd, dy, end_m as i32, 0, 0)
                                }
                                _ => {
                                    let end_y = dy as i64 + dur_sign * inc;
                                    add_iso_date(ry, rm, rd, end_y as i32, 0, 0, 0)
                                }
                            };
                            if !iso_date_within_limits(end_date.0, end_date.1, end_date.2) {
                                return Completion::Throw(
                                    interp
                                        .create_range_error("Rounded date outside valid ISO range"),
                                );
                            }
                        }
                        let (ry2, rm2, rw2, rd2) = match round_date_duration(
                            dy,
                            dm,
                            dw,
                            dd,
                            &smallest_unit,
                            &largest_unit,
                            rounding_increment,
                            &effective_mode,
                            ry,
                            rm,
                            rd,
                        ) {
                            Ok(v) => v,
                            Err(msg) => return Completion::Throw(interp.create_range_error(&msg)),
                        };
                        dy = ry2;
                        dm = rm2;
                        dw = rw2;
                        dd = rd2;
                        // Check that rounded date is within valid ISO range (calendar units only)
                        if matches!(smallest_unit.as_str(), "month" | "year") {
                            let rounded_end = add_iso_date(ry, rm, rd, dy, dm, dw, dd);
                            if !iso_date_within_limits(rounded_end.0, rounded_end.1, rounded_end.2)
                            {
                                return Completion::Throw(
                                    interp
                                        .create_range_error("Rounded date outside valid ISO range"),
                                );
                            }
                        }
                        // Rebalance months overflow into years when largestUnit is year
                        if matches!(largest_unit.as_str(), "year") && dm.abs() >= 12 {
                            dy += dm / 12;
                            dm %= 12;
                        }
                    }

                    // For since: negate the result
                    if sign == -1 {
                        dy = -dy;
                        dm = -dm;
                        dw = -dw;
                        dd = -dd;
                    }

                    super::duration::create_duration_result(
                        interp, dy as f64, dm as f64, dw as f64, dd as f64, 0.0, 0.0, 0.0, 0.0,
                        0.0, 0.0,
                    )
                },
            ));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
        }

        // equals(other)
        let equals_fn = self.create_function(JsFunction::native(
            "equals".to_string(),
            1,
            |interp, this, args| {
                let (y1, m1, d1, c1) = match get_plain_date_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let other = args.first().cloned().unwrap_or(JsValue::Undefined);
                let (y2, mut m2, mut d2, c2) = match to_temporal_plain_date(interp, other) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                m2 = m2.max(1).min(12);
                d2 = d2.max(1).min(iso_days_in_month(y2, m2));
                let eq = y1 == y2 && m1 == m2 && d1 == d2 && c1 == c2;
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
                let (y, m, d, cal) = match get_plain_date_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let options = args.first().cloned().unwrap_or(JsValue::Undefined);
                let has_opts = match super::get_options_object(interp, &options) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let show_calendar = if has_opts {
                    let cv = match get_prop(interp, &options, "calendarName") {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if is_undefined(&cv) {
                        "auto"
                    } else {
                        let s = match interp.to_string_value(&cv) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };
                        match s.as_str() {
                            "auto" => "auto",
                            "always" => "always",
                            "never" => "never",
                            "critical" => "critical",
                            _ => {
                                return Completion::Throw(
                                    interp
                                        .create_range_error(&format!("Invalid calendarName: {s}")),
                                );
                            }
                        }
                    }
                } else {
                    "auto"
                };

                let result = format_plain_date(y, m, d, &cal, show_calendar);
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
                let (y, m, d, cal) = match get_plain_date_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let result = format_plain_date(y, m, d, &cal, "auto");
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
            |interp, this, args| {
                let (y, m, d, cal) = match get_plain_date_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let dtf_val = match interp.intl_date_time_format_ctor.clone() {
                    Some(v) => v,
                    None => {
                        let result = format_plain_date(y, m, d, &cal, "auto");
                        return Completion::Normal(JsValue::String(JsString::from_str(&result)));
                    }
                };
                let locales_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let options_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let dtf_instance = match interp.construct(&dtf_val, &[locales_arg, options_arg]) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Completion::Throw(e),
                    _ => return Completion::Normal(JsValue::Undefined),
                };
                super::temporal_format_with_dtf(interp, &dtf_instance, &this)
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
                        "use compare() or equals() to compare Temporal.PlainDate",
                    ))
                },
            ));
        proto
            .borrow_mut()
            .insert_builtin("valueOf".to_string(), value_of_fn);

        // toPlainDateTime(temporalTime?)
        let to_pdt_fn = self.create_function(JsFunction::native(
            "toPlainDateTime".to_string(),
            0,
            |interp, this, args| {
                let (y, m, d, cal) = match get_plain_date_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let time = args.first().cloned().unwrap_or(JsValue::Undefined);
                let (h, mi, s, ms, us, ns) = if is_undefined(&time) {
                    (0u8, 0u8, 0u8, 0u16, 0u16, 0u16)
                } else {
                    match super::plain_time::to_temporal_plain_time(interp, time) {
                        Ok(v) => v,
                        Err(c) => return c,
                    }
                };
                if !super::iso_date_time_within_limits(y, m, d, h, mi, s, ms, us, ns) {
                    return Completion::Throw(
                        interp.create_range_error("DateTime outside valid ISO range"),
                    );
                }
                super::plain_date_time::create_plain_date_time_result(
                    interp, y, m, d, h, mi, s, ms, us, ns, &cal,
                )
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toPlainDateTime".to_string(), to_pdt_fn);

        // toPlainYearMonth()
        let to_ym_fn = self.create_function(JsFunction::native(
            "toPlainYearMonth".to_string(),
            0,
            |interp, this, _args| {
                let (y, m, _d, cal) = match get_plain_date_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                super::plain_year_month::create_plain_year_month_result(interp, y, m, 1, &cal)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toPlainYearMonth".to_string(), to_ym_fn);

        // toPlainMonthDay()
        let to_md_fn = self.create_function(JsFunction::native(
            "toPlainMonthDay".to_string(),
            0,
            |interp, this, _args| {
                let (_y, m, d, cal) = match get_plain_date_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                // Per spec, ISO 8601 calendar uses reference year 1972
                super::plain_month_day::create_plain_month_day_result(interp, m, d, 1972, &cal)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toPlainMonthDay".to_string(), to_md_fn);

        // withCalendar(calendar)
        let with_cal_fn = self.create_function(JsFunction::native(
            "withCalendar".to_string(),
            1,
            |interp, this, args| {
                let (y, m, d, _) = match get_plain_date_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let cal_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                if matches!(cal_arg, JsValue::Undefined) {
                    return Completion::Throw(
                        interp.create_type_error("withCalendar requires a calendar argument"),
                    );
                }
                let cal = match to_temporal_calendar_slot_value(interp, &cal_arg) {
                    Ok(c) => c,
                    Err(c) => return c,
                };
                create_plain_date_result(interp, y, m, d, &cal)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("withCalendar".to_string(), with_cal_fn);

        // getISOFields()
        let get_iso_fn = self.create_function(JsFunction::native(
            "getISOFields".to_string(),
            0,
            |interp, this, _args| {
                let (y, m, d, cal) = match get_plain_date_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let obj = interp.create_object();
                obj.borrow_mut().insert_property(
                    "calendar".to_string(),
                    PropertyDescriptor::data(
                        JsValue::String(JsString::from_str(&cal)),
                        true,
                        true,
                        true,
                    ),
                );
                obj.borrow_mut().insert_property(
                    "isoDay".to_string(),
                    PropertyDescriptor::data(JsValue::Number(d as f64), true, true, true),
                );
                obj.borrow_mut().insert_property(
                    "isoMonth".to_string(),
                    PropertyDescriptor::data(JsValue::Number(m as f64), true, true, true),
                );
                obj.borrow_mut().insert_property(
                    "isoYear".to_string(),
                    PropertyDescriptor::data(JsValue::Number(y as f64), true, true, true),
                );
                let id = obj.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("getISOFields".to_string(), get_iso_fn);

        // toZonedDateTime(item)
        let to_zdt_fn = self.create_function(JsFunction::native(
            "toZonedDateTime".to_string(),
            1,
            |interp, this, args| {
                let (y, m, d, cal) = match get_plain_date_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let item = args.first().cloned().unwrap_or(JsValue::Undefined);

                let (tz, h, mi, s, ms, us, ns) = if let JsValue::String(_) = &item {
                    // String argument = timezone, time defaults to midnight
                    let tz = match super::to_temporal_time_zone_identifier(interp, &item) {
                        Ok(t) => t,
                        Err(c) => return c,
                    };
                    (tz, 0u8, 0u8, 0u8, 0u16, 0u16, 0u16)
                } else if let JsValue::Object(_) = &item {
                    // Per spec: get timeZone property
                    let tz_val = match super::get_prop(interp, &item, "timeZone") {
                        Completion::Normal(v) => v,
                        c => return c,
                    };
                    if super::is_undefined(&tz_val) {
                        // timeZone undefined → treat item itself as timezone
                        let tz = match super::to_temporal_time_zone_identifier(interp, &item) {
                            Ok(t) => t,
                            Err(c) => return c,
                        };
                        (tz, 0u8, 0u8, 0u8, 0u16, 0u16, 0u16)
                    } else {
                        // Object with timeZone and optional plainTime
                        let tz = match super::to_temporal_time_zone_identifier(interp, &tz_val) {
                            Ok(t) => t,
                            Err(c) => return c,
                        };
                        let pt_val = match super::get_prop(interp, &item, "plainTime") {
                            Completion::Normal(v) => v,
                            c => return c,
                        };
                        if super::is_undefined(&pt_val) {
                            (tz, 0, 0, 0, 0, 0, 0)
                        } else {
                            let (th, tm, ts, tms, tus, tns) =
                                match super::plain_time::to_temporal_plain_time(interp, pt_val) {
                                    Ok(v) => v,
                                    Err(c) => return c,
                                };
                            (tz, th, tm, ts, tms, tus, tns)
                        }
                    }
                } else {
                    return Completion::Throw(
                        interp.create_type_error("Expected a string or object for toZonedDateTime"),
                    );
                };

                let epoch_days = super::iso_date_to_epoch_days(y, m, d) as i128;
                let day_ns = h as i128 * 3_600_000_000_000
                    + mi as i128 * 60_000_000_000
                    + s as i128 * 1_000_000_000
                    + ms as i128 * 1_000_000
                    + us as i128 * 1_000
                    + ns as i128;
                let local_ns = epoch_days * 86_400_000_000_000 + day_ns;
                let approx = num_bigint::BigInt::from(local_ns);
                let offset = super::zoned_date_time::get_tz_offset_ns_pub(&tz, &approx) as i128;
                let epoch_ns = num_bigint::BigInt::from(local_ns - offset);

                super::zoned_date_time::create_zdt_pub(interp, epoch_ns, tz, cal)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toZonedDateTime".to_string(), to_zdt_fn);

        self.temporal_plain_date_prototype = Some(proto.clone());

        // Constructor
        let constructor = self.create_function(JsFunction::constructor(
            "PlainDate".to_string(),
            3,
            |interp, _this, args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Temporal.PlainDate must be called with new"),
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
                let d_val = args.get(2).cloned().unwrap_or(JsValue::Undefined);
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
                let cal_arg = args.get(3).cloned().unwrap_or(JsValue::Undefined);
                let cal = match super::validate_calendar_strict(interp, &cal_arg) {
                    Ok(c) => c,
                    Err(c) => return c,
                };
                if !iso_date_valid(y, m, d) {
                    return Completion::Throw(interp.create_range_error("Invalid date"));
                }
                if !super::iso_date_within_limits(y, m, d) {
                    return Completion::Throw(
                        interp.create_range_error("Date outside valid ISO range"),
                    );
                }
                create_plain_date_result(interp, y, m, d, &cal)
            },
        ));

        // Constructor.prototype
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

        // PlainDate.from(item, options?)
        let from_fn = self.create_function(JsFunction::native(
            "from".to_string(),
            1,
            |interp, _this, args| {
                let item = args.first().cloned().unwrap_or(JsValue::Undefined);
                let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                // Per spec: if item is a string, parse first, then validate overflow (but don't use it)
                if matches!(&item, JsValue::String(_)) {
                    let (y, m, d, cal) = match to_temporal_plain_date(interp, item) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    match parse_overflow_option(interp, &options) {
                        Ok(_) => {}
                        Err(c) => return c,
                    }
                    if !super::iso_date_within_limits(y, m, d) {
                        return Completion::Throw(
                            interp.create_range_error("Date outside valid ISO range"),
                        );
                    }
                    return create_plain_date_result(interp, y, m, d, &cal);
                }
                // Check if it's a Temporal object (read overflow first, return copy)
                let is_temporal = if let JsValue::Object(ref o) = item {
                    if let Some(obj) = interp.get_object(o.id) {
                        let data = obj.borrow();
                        matches!(
                            &data.temporal_data,
                            Some(TemporalData::PlainDate { .. })
                                | Some(TemporalData::PlainDateTime { .. })
                                | Some(TemporalData::ZonedDateTime { .. })
                        )
                    } else {
                        false
                    }
                } else {
                    false
                };
                if is_temporal {
                    let (y, m, d, cal) = match to_temporal_plain_date(interp, item) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    match parse_overflow_option(interp, &options) {
                        Ok(_) => {}
                        Err(c) => return c,
                    }
                    create_plain_date_result(interp, y, m, d, &cal)
                } else {
                    // Property bag: read fields raw, then overflow, then validate
                    let bag = match read_pd_property_bag_raw(interp, &item) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let overflow = match parse_overflow_option(interp, &options) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };

                    // Non-ISO calendar: convert via ICU
                    if bag.cal != "iso8601" {
                        if bag.mc_str.is_none() && bag.month_num.is_none() {
                            return Completion::Throw(
                                interp.create_type_error("month or monthCode is required"),
                            );
                        }

                        let (icu_era, icu_year) =
                            if super::calendar_has_eras(&bag.cal) {
                                if let (Some(e), Some(ey)) = (&bag.era, bag.era_year) {
                                    (Some(e.as_str()), ey)
                                } else {
                                    (None, bag.year.unwrap_or(0))
                                }
                            } else {
                                (None, bag.year.unwrap_or(0))
                            };

                        match super::calendar_fields_to_iso(
                            icu_era,
                            icu_year,
                            bag.mc_str.as_deref(),
                            bag.month_num,
                            bag.day,
                            &bag.cal,
                        ) {
                            Some((iso_y, iso_m, iso_d)) => {
                                if !super::iso_date_within_limits(iso_y, iso_m, iso_d) {
                                    return Completion::Throw(
                                        interp
                                            .create_range_error("Date outside valid ISO range"),
                                    );
                                }
                                return create_plain_date_result(
                                    interp, iso_y, iso_m, iso_d, &bag.cal,
                                );
                            }
                            None => {
                                return Completion::Throw(
                                    interp.create_range_error("Invalid calendar date"),
                                );
                            }
                        }
                    }

                    // ISO path: resolve monthCode
                    let y = bag.year.unwrap();
                    let d = bag.day;
                    let m = match resolve_pd_month(interp, bag.month_num, bag.mc_str) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    if overflow == "reject" && !iso_date_valid(y, m, d) {
                        return Completion::Throw(interp.create_range_error("Invalid date"));
                    }
                    let (y, m, d) = constrain_or_reject_date(y, m, d, &overflow);
                    if !super::iso_date_within_limits(y, m, d) {
                        return Completion::Throw(
                            interp.create_range_error("Date outside valid ISO range"),
                        );
                    }
                    create_plain_date_result(interp, y, m, d, &bag.cal)
                }
            },
        ));
        if let JsValue::Object(ref o) = constructor {
            if let Some(obj) = self.get_object(o.id) {
                obj.borrow_mut().insert_builtin("from".to_string(), from_fn);
            }
        }

        // PlainDate.compare(one, two)
        let compare_fn = self.create_function(JsFunction::native(
            "compare".to_string(),
            2,
            |interp, _this, args| {
                let (y1, mut m1, mut d1, _) = match to_temporal_plain_date(
                    interp,
                    args.first().cloned().unwrap_or(JsValue::Undefined),
                ) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                m1 = m1.max(1).min(12);
                d1 = d1.max(1).min(iso_days_in_month(y1, m1));
                let (y2, mut m2, mut d2, _) = match to_temporal_plain_date(
                    interp,
                    args.get(1).cloned().unwrap_or(JsValue::Undefined),
                ) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                m2 = m2.max(1).min(12);
                d2 = d2.max(1).min(iso_days_in_month(y2, m2));
                let result = if y1 != y2 {
                    if y1 < y2 { -1.0 } else { 1.0 }
                } else if m1 != m2 {
                    if m1 < m2 { -1.0 } else { 1.0 }
                } else if d1 != d2 {
                    if d1 < d2 { -1.0 } else { 1.0 }
                } else {
                    0.0
                };
                Completion::Normal(JsValue::Number(result))
            },
        ));
        if let JsValue::Object(ref o) = constructor {
            if let Some(obj) = self.get_object(o.id) {
                obj.borrow_mut()
                    .insert_builtin("compare".to_string(), compare_fn);
            }
        }

        temporal_obj.borrow_mut().insert_property(
            "PlainDate".to_string(),
            PropertyDescriptor::data(constructor, true, false, true),
        );
    }
}

fn get_plain_date_fields(
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
                interp.create_type_error("not a Temporal.PlainDate"),
            ));
        }
    };
    let data = obj.borrow();
    match &data.temporal_data {
        Some(TemporalData::PlainDate {
            iso_year,
            iso_month,
            iso_day,
            calendar,
        }) => Ok((*iso_year, *iso_month, *iso_day, calendar.clone())),
        _ => Err(Completion::Throw(
            interp.create_type_error("not a Temporal.PlainDate"),
        )),
    }
}

pub(super) fn create_plain_date_result(
    interp: &mut Interpreter,
    y: i32,
    m: u8,
    d: u8,
    cal: &str,
) -> Completion {
    let obj = interp.create_object();
    obj.borrow_mut().class_name = "Temporal.PlainDate".to_string();
    if let Some(ref proto) = interp.temporal_plain_date_prototype {
        obj.borrow_mut().prototype = Some(proto.clone());
    }
    obj.borrow_mut().temporal_data = Some(TemporalData::PlainDate {
        iso_year: y,
        iso_month: m,
        iso_day: d,
        calendar: cal.to_string(),
    });
    let id = obj.borrow().id.unwrap();
    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
}

/// Read PlainDate property bag fields without validating monthCode.
/// Returns (year, month_num_opt, monthCode_str_opt, day, calendar).
struct PdPropertyBag {
    year: Option<i32>,
    month_num: Option<u8>,
    mc_str: Option<String>,
    day: u8,
    cal: String,
    era: Option<String>,
    era_year: Option<i32>,
}

fn read_pd_property_bag_raw(
    interp: &mut Interpreter,
    item: &JsValue,
) -> Result<PdPropertyBag, Completion> {
    let cal_val = match get_prop(interp, item, "calendar") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let cal = to_temporal_calendar_slot_value(interp, &cal_val)?;
    let d_val = match get_prop(interp, item, "day") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let d = if is_undefined(&d_val) {
        return Err(Completion::Throw(
            interp.create_type_error("day is required"),
        ));
    } else {
        let d_f = to_integer_with_truncation(interp, &d_val)?;
        if d_f < 1.0 {
            return Err(Completion::Throw(
                interp.create_range_error("day must be a positive integer"),
            ));
        }
        d_f as u8
    };
    // era (for non-ISO calendars)
    let era_val = match get_prop(interp, item, "era") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let era = if !is_undefined(&era_val) {
        Some(super::to_primitive_and_require_string(interp, &era_val, "era")?)
    } else {
        None
    };
    let era_year_val = match get_prop(interp, item, "eraYear") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let era_year = if !is_undefined(&era_year_val) {
        Some(to_integer_with_truncation(interp, &era_year_val)? as i32)
    } else {
        None
    };
    let m_val = match get_prop(interp, item, "month") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let month_num = if !is_undefined(&m_val) {
        let m_f = to_integer_with_truncation(interp, &m_val)?;
        if m_f < 1.0 {
            return Err(Completion::Throw(
                interp.create_range_error("month must be a positive integer"),
            ));
        }
        Some(m_f as u8)
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
    let year = if is_undefined(&y_val) {
        None
    } else {
        Some(to_integer_with_truncation(interp, &y_val)? as i32)
    };
    // Era/eraYear validation
    if super::calendar_has_eras(&cal) {
        // If one of era/eraYear is present but not the other → TypeError
        if era.is_some() != era_year.is_some() {
            return Err(Completion::Throw(
                interp.create_type_error("era and eraYear must both be present or both absent"),
            ));
        }
    } else {
        // For calendars without eras (iso8601, chinese, dangi), ignore era/eraYear
        // but year is required
        if year.is_none() {
            return Err(Completion::Throw(
                interp.create_type_error("year is required"),
            ));
        }
    }
    // For calendars with eras, either year or era+eraYear is required
    if super::calendar_has_eras(&cal)
        && year.is_none()
        && (era.is_none() || era_year.is_none())
    {
        return Err(Completion::Throw(
            interp.create_type_error("year or era+eraYear is required"),
        ));
    }
    Ok(PdPropertyBag {
        year,
        month_num,
        mc_str,
        day: d,
        cal,
        era,
        era_year,
    })
}

fn resolve_pd_month(
    interp: &mut Interpreter,
    month_num: Option<u8>,
    mc_str: Option<String>,
) -> Result<u8, Completion> {
    if let Some(ref mc) = mc_str {
        match month_code_to_number(mc) {
            Some(n) => {
                if let Some(mn) = month_num {
                    if mn != n {
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
        Ok(mn)
    } else {
        Err(Completion::Throw(
            interp.create_type_error("month or monthCode is required"),
        ))
    }
}

pub(super) fn to_temporal_plain_date(
    interp: &mut Interpreter,
    item: JsValue,
) -> Result<(i32, u8, u8, String), Completion> {
    match &item {
        JsValue::Object(o) => {
            if let Some(obj) = interp.get_object(o.id) {
                let data = obj.borrow();
                if let Some(TemporalData::PlainDate {
                    iso_year,
                    iso_month,
                    iso_day,
                    calendar,
                }) = &data.temporal_data
                {
                    return Ok((*iso_year, *iso_month, *iso_day, calendar.clone()));
                }
                if let Some(TemporalData::PlainDateTime {
                    iso_year,
                    iso_month,
                    iso_day,
                    calendar,
                    ..
                }) = &data.temporal_data
                {
                    return Ok((*iso_year, *iso_month, *iso_day, calendar.clone()));
                }
                if let Some(TemporalData::ZonedDateTime {
                    epoch_nanoseconds,
                    time_zone,
                    calendar,
                }) = &data.temporal_data
                {
                    let (y, m, d, _, _, _, _, _, _) =
                        super::zoned_date_time::epoch_ns_to_components(
                            epoch_nanoseconds,
                            time_zone,
                        );
                    return Ok((y, m, d, calendar.clone()));
                }
            }
            // Property bag: read fields in alphabetical order with interleaved coercion.
            // Per spec PrepareCalendarFields: calendar, day, era, eraYear, month, monthCode, year

            // 1. calendar: get + coerce
            let cal_val = match get_prop(interp, &item, "calendar") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let cal = to_temporal_calendar_slot_value(interp, &cal_val)?;

            // 2. day: get + coerce (required, positive integer)
            let d_val = match get_prop(interp, &item, "day") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let d = if is_undefined(&d_val) {
                return Err(Completion::Throw(
                    interp.create_type_error("day is required"),
                ));
            } else {
                let d_f = to_integer_with_truncation(interp, &d_val)?;
                if d_f < 1.0 {
                    return Err(Completion::Throw(
                        interp.create_range_error("day must be a positive integer"),
                    ));
                }
                d_f as u8
            };

            // 2b. era: get + coerce (optional, for non-ISO calendars)
            let era_val = match get_prop(interp, &item, "era") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let era_str = if !is_undefined(&era_val) {
                Some(super::to_primitive_and_require_string(interp, &era_val, "era")?)
            } else {
                None
            };

            // 2c. eraYear: get + coerce (optional, for non-ISO calendars)
            let era_year_val = match get_prop(interp, &item, "eraYear") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let era_year = if !is_undefined(&era_year_val) {
                Some(to_integer_with_truncation(interp, &era_year_val)? as i32)
            } else {
                None
            };

            // 3. month: get + coerce
            let m_val = match get_prop(interp, &item, "month") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let has_month = !is_undefined(&m_val);
            let month_num = if has_month {
                let m_f = to_integer_with_truncation(interp, &m_val)?;
                if m_f < 1.0 {
                    return Err(Completion::Throw(
                        interp.create_range_error("month must be a positive integer"),
                    ));
                }
                Some(m_f as u8)
            } else {
                None
            };

            // 4. monthCode: get + coerce + SYNTAX check (before year)
            let mc_val = match get_prop(interp, &item, "monthCode") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let has_month_code = !is_undefined(&mc_val);
            let mc_str = if has_month_code {
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

            // 5. year: get + coerce
            let y_val = match get_prop(interp, &item, "year") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            let has_year = !is_undefined(&y_val);
            let year_num = if has_year {
                Some(to_integer_with_truncation(interp, &y_val)? as i32)
            } else {
                None
            };

            // Non-ISO calendar path: use ICU4X to convert calendar fields → ISO
            if cal != "iso8601" {
                // Determine the month code
                if mc_str.is_none() && month_num.is_none() {
                    return Err(Completion::Throw(
                        interp.create_type_error("month or monthCode is required"),
                    ));
                }

                // Determine era and year for ICU
                let (icu_era, icu_year) = if let (Some(e), Some(ey)) = (&era_str, era_year) {
                    (Some(e.as_str()), ey)
                } else if let Some(y) = year_num {
                    (None, y)
                } else {
                    return Err(Completion::Throw(
                        interp.create_type_error("year or era+eraYear is required"),
                    ));
                };

                match super::calendar_fields_to_iso(
                    icu_era,
                    icu_year,
                    mc_str.as_deref(),
                    month_num,
                    d,
                    &cal,
                ) {
                    Some((iso_y, iso_m, iso_d)) => return Ok((iso_y, iso_m, iso_d, cal)),
                    None => {
                        return Err(Completion::Throw(
                            interp.create_range_error("Invalid calendar date"),
                        ));
                    }
                }
            }

            // ISO path
            let y = match year_num {
                Some(y) => y,
                None => {
                    return Err(Completion::Throw(
                        interp.create_type_error("year is required"),
                    ));
                }
            };

            // Validate monthCode VALUE (after year coercion)
            let month_code_num = if let Some(ref mc) = mc_str {
                match month_code_to_number(mc) {
                    Some(n) => Some(n),
                    None => {
                        return Err(Completion::Throw(
                            interp.create_range_error(&format!("Invalid monthCode: {mc}")),
                        ));
                    }
                }
            } else {
                None
            };

            // Resolve month
            let m = if let Some(mc_n) = month_code_num {
                if let Some(explicit_m) = month_num {
                    if explicit_m != mc_n {
                        return Err(Completion::Throw(
                            interp.create_range_error("month and monthCode conflict"),
                        ));
                    }
                }
                mc_n
            } else if let Some(mn) = month_num {
                mn
            } else {
                return Err(Completion::Throw(
                    interp.create_type_error("month or monthCode is required"),
                ));
            };

            Ok((y, m, d, cal))
        }
        JsValue::String(s) => parse_date_string(interp, &s.to_rust_string()),
        _ => Err(Completion::Throw(
            interp.create_type_error("Cannot convert to Temporal.PlainDate"),
        )),
    }
}

fn parse_date_string(
    interp: &mut Interpreter,
    s: &str,
) -> Result<(i32, u8, u8, String), Completion> {
    let parsed = match parse_temporal_date_time_string(s) {
        Some(p) => p,
        None => {
            return Err(Completion::Throw(
                interp.create_range_error(&format!("Invalid date string: {s}")),
            ));
        }
    };
    // PlainDate does not accept UTC designator (Z)
    if parsed.has_utc_designator {
        return Err(Completion::Throw(interp.create_range_error(
            "UTC designator Z is not allowed in a PlainDate string",
        )));
    }
    // Date-only string with UTC offset is not valid for PlainDate
    if !parsed.has_time && parsed.offset.is_some() {
        return Err(Completion::Throw(interp.create_range_error(
            "UTC offset without time is not valid for PlainDate",
        )));
    }
    let cal = parsed.calendar.unwrap_or_else(|| "iso8601".to_string());
    let cal = match validate_calendar(&cal) {
        Some(c) => c,
        None => {
            return Err(Completion::Throw(
                interp.create_range_error(&format!("Invalid calendar: {cal}")),
            ));
        }
    };
    if !super::iso_date_within_limits(parsed.year, parsed.month, parsed.day) {
        return Err(Completion::Throw(
            interp.create_range_error("Date outside representable range"),
        ));
    }
    Ok((parsed.year, parsed.month, parsed.day, cal))
}

fn get_date_field_i32(
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

fn get_date_field_u8(
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

fn month_code_to_number(mc: &str) -> Option<u8> {
    month_code_to_number_pub(mc)
}

pub(super) fn month_code_to_number_pub(mc: &str) -> Option<u8> {
    match mc {
        "M01" => Some(1),
        "M02" => Some(2),
        "M03" => Some(3),
        "M04" => Some(4),
        "M05" => Some(5),
        "M06" => Some(6),
        "M07" => Some(7),
        "M08" => Some(8),
        "M09" => Some(9),
        "M10" => Some(10),
        "M11" => Some(11),
        "M12" => Some(12),
        _ => None,
    }
}

// parse_overflow_option is now shared from mod.rs

fn constrain_or_reject_date(y: i32, mut m: u8, mut d: u8, overflow: &str) -> (i32, u8, u8) {
    if overflow == "constrain" {
        m = m.max(1).min(12);
        let dim = iso_days_in_month(y, m);
        d = d.max(1).min(dim);
    }
    (y, m, d)
}

pub(super) fn format_plain_date(y: i32, m: u8, d: u8, cal: &str, show_calendar: &str) -> String {
    let year_str = if y >= 0 && y <= 9999 {
        format!("{y:04}")
    } else if y >= 0 {
        format!("+{y:06}")
    } else {
        format!("-{:06}", y.unsigned_abs())
    };
    let mut result = format!("{year_str}-{m:02}-{d:02}");
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
        _ => {} // "never"
    }
    result
}
