use super::*;
use crate::interpreter::builtins::temporal::{
    add_iso_date, difference_iso_date, get_prop, is_undefined, iso_date_valid, iso_day_of_week,
    iso_day_of_year, iso_days_in_month, iso_days_in_year, iso_is_leap_year, iso_month_code,
    iso_time_valid, iso_week_of_year, nanoseconds_to_time, parse_difference_options,
    parse_overflow_option, parse_temporal_date_time_string, resolve_month_fields,
    round_number_to_increment, temporal_unit_singular, time_to_nanoseconds,
    to_temporal_calendar_slot_value, validate_calendar,
};

pub(super) fn create_plain_date_time_result(
    interp: &mut Interpreter,
    y: i32,
    m: u8,
    d: u8,
    hour: u8,
    minute: u8,
    second: u8,
    ms: u16,
    us: u16,
    ns: u16,
    cal: &str,
) -> Completion {
    let obj = interp.create_object();
    obj.borrow_mut().class_name = "Temporal.PlainDateTime".to_string();
    if let Some(ref proto) = interp.temporal_plain_date_time_prototype {
        obj.borrow_mut().prototype = Some(proto.clone());
    }
    obj.borrow_mut().temporal_data = Some(TemporalData::PlainDateTime {
        iso_year: y,
        iso_month: m,
        iso_day: d,
        hour,
        minute,
        second,
        millisecond: ms,
        microsecond: us,
        nanosecond: ns,
        calendar: cal.to_string(),
    });
    let id = obj.borrow().id.unwrap();
    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
}

pub(super) fn to_temporal_plain_date_time(
    interp: &mut Interpreter,
    item: JsValue,
) -> Result<(i32, u8, u8, u8, u8, u8, u16, u16, u16, String), Completion> {
    to_temporal_plain_date_time_with_overflow(interp, item, "constrain")
}

pub(super) fn to_temporal_plain_date_time_with_overflow(
    interp: &mut Interpreter,
    item: JsValue,
    overflow: &str,
) -> Result<(i32, u8, u8, u8, u8, u8, u16, u16, u16, String), Completion> {
    match &item {
        JsValue::Object(o) => {
            if let Some(obj) = interp.get_object(o.id) {
                let data = obj.borrow();
                if let Some(TemporalData::PlainDateTime {
                    iso_year,
                    iso_month,
                    iso_day,
                    hour,
                    minute,
                    second,
                    millisecond,
                    microsecond,
                    nanosecond,
                    calendar,
                }) = &data.temporal_data
                {
                    return Ok((
                        *iso_year,
                        *iso_month,
                        *iso_day,
                        *hour,
                        *minute,
                        *second,
                        *millisecond,
                        *microsecond,
                        *nanosecond,
                        calendar.clone(),
                    ));
                }
                if let Some(TemporalData::PlainDate {
                    iso_year,
                    iso_month,
                    iso_day,
                    calendar,
                }) = &data.temporal_data
                {
                    return Ok((
                        *iso_year,
                        *iso_month,
                        *iso_day,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        calendar.clone(),
                    ));
                }
                if let Some(TemporalData::ZonedDateTime {
                    epoch_nanoseconds,
                    time_zone,
                    calendar,
                }) = &data.temporal_data
                {
                    let (y, mo, d, h, mi, s, ms, us, ns) =
                        super::zoned_date_time::epoch_ns_to_components(
                            epoch_nanoseconds,
                            time_zone,
                        );
                    return Ok((y, mo, d, h, mi, s, ms, us, ns, calendar.clone()));
                }
            }
            let (y_f, month_num, mc_str, d_f, h, mi, s, ms, us, ns, cal) =
                read_pdt_property_bag(interp, &item)?;
            apply_pdt_overflow(
                interp, y_f, month_num, mc_str, d_f, h, mi, s, ms, us, ns, cal, overflow,
            )
        }
        JsValue::String(s) => parse_date_time_string(interp, &s.to_rust_string()),
        _ => Err(Completion::Throw(
            interp.create_type_error("Cannot convert to Temporal.PlainDateTime"),
        )),
    }
}

fn parse_date_time_string(
    interp: &mut Interpreter,
    s: &str,
) -> Result<(i32, u8, u8, u8, u8, u8, u16, u16, u16, String), Completion> {
    let parsed = match parse_temporal_date_time_string(s) {
        Some(p) => p,
        None => {
            return Err(Completion::Throw(
                interp.create_range_error(&format!("Invalid date-time string: {s}")),
            ));
        }
    };
    // PlainDateTime does not accept UTC designator (Z)
    if parsed.has_utc_designator {
        return Err(Completion::Throw(interp.create_range_error(
            "UTC designator Z is not allowed in a PlainDateTime string",
        )));
    }
    // Date-only string with UTC offset is not valid for PlainDateTime
    if !parsed.has_time && parsed.offset.is_some() {
        return Err(Completion::Throw(interp.create_range_error(
            "UTC offset without time is not valid for PlainDateTime",
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
    if !super::iso_date_time_within_limits(
        parsed.year,
        parsed.month,
        parsed.day,
        parsed.hour,
        parsed.minute,
        parsed.second,
        parsed.millisecond,
        parsed.microsecond,
        parsed.nanosecond,
    ) {
        return Err(Completion::Throw(
            interp.create_range_error("Date outside representable range"),
        ));
    }
    Ok((
        parsed.year,
        parsed.month,
        parsed.day,
        parsed.hour,
        parsed.minute,
        parsed.second,
        parsed.millisecond,
        parsed.microsecond,
        parsed.nanosecond,
        cal,
    ))
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
        let n = to_integer_with_truncation(interp, &val)?;
        Ok(n as u8)
    }
}

fn get_opt_u16(
    interp: &mut Interpreter,
    obj: &JsValue,
    key: &str,
    default: u16,
) -> Result<u16, Completion> {
    let val = match get_prop(interp, obj, key) {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    if is_undefined(&val) {
        Ok(default)
    } else {
        let n = to_integer_with_truncation(interp, &val)?;
        Ok(n as u16)
    }
}

/// Read PlainDateTime fields from a property bag without applying overflow.
/// Returns (year_f64, month_num_opt, month_code_opt, day_f64, hour, minute, second, ms, us, ns, calendar).
/// month validation and monthCode resolution are deferred to apply_pdt_overflow.
fn read_pdt_property_bag(
    interp: &mut Interpreter,
    item: &JsValue,
) -> Result<
    (
        f64,
        Option<f64>,
        Option<String>,
        f64,
        u8,
        u8,
        u8,
        u16,
        u16,
        u16,
        String,
    ),
    Completion,
> {
    let cal_val = match get_prop(interp, item, "calendar") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let cal = to_temporal_calendar_slot_value(interp, &cal_val)?;
    let d_val = match get_prop(interp, item, "day") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let d_f: f64 = if is_undefined(&d_val) {
        return Err(Completion::Throw(
            interp.create_type_error("day is required"),
        ));
    } else {
        to_integer_with_truncation(interp, &d_val)?
    };
    let h = get_opt_u8(interp, item, "hour", 0)?;
    let us = get_opt_u16(interp, item, "microsecond", 0)?;
    let ms = get_opt_u16(interp, item, "millisecond", 0)?;
    let mi = get_opt_u8(interp, item, "minute", 0)?;
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
    let ns = get_opt_u16(interp, item, "nanosecond", 0)?;
    let s = get_opt_u8(interp, item, "second", 0)?;
    let y_val = match get_prop(interp, item, "year") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let y_f = if is_undefined(&y_val) {
        return Err(Completion::Throw(
            interp.create_type_error("year is required"),
        ));
    } else {
        to_integer_with_truncation(interp, &y_val)?
    };
    Ok((y_f, month_num, mc_str, d_f, h, mi, s, ms, us, ns, cal))
}

fn resolve_pdt_month(
    interp: &mut Interpreter,
    month_num: Option<f64>,
    mc_str: Option<String>,
    d_f: f64,
) -> Result<f64, Completion> {
    let month_f: f64 = if let Some(ref mc) = mc_str {
        match super::plain_date::month_code_to_number_pub(mc) {
            Some(n) => {
                let mc_n = n as f64;
                if let Some(mn) = month_num {
                    if mn as u8 != mc_n as u8 {
                        return Err(Completion::Throw(
                            interp.create_range_error("month and monthCode conflict"),
                        ));
                    }
                }
                mc_n
            }
            None => {
                return Err(Completion::Throw(
                    interp.create_range_error(&format!("Invalid monthCode: {mc}")),
                ));
            }
        }
    } else if let Some(mn) = month_num {
        mn
    } else {
        return Err(Completion::Throw(
            interp.create_type_error("month or monthCode is required"),
        ));
    };
    if month_f < 1.0 || d_f < 1.0 {
        return Err(Completion::Throw(
            interp.create_range_error("Month and day must be positive integers"),
        ));
    }
    Ok(month_f)
}

fn apply_pdt_overflow(
    interp: &mut Interpreter,
    y_f: f64,
    month_num: Option<f64>,
    mc_str: Option<String>,
    d_f: f64,
    h: u8,
    mi: u8,
    s: u8,
    ms: u16,
    us: u16,
    ns: u16,
    cal: String,
    overflow: &str,
) -> Result<(i32, u8, u8, u8, u8, u8, u16, u16, u16, String), Completion> {
    let month_f = resolve_pdt_month(interp, month_num, mc_str, d_f)?;
    let y = y_f as i32;
    let month = month_f as u8;
    let d = d_f as u8;
    if overflow == "reject" {
        if !iso_date_valid(y, month, d) {
            return Err(Completion::Throw(interp.create_range_error("Invalid date")));
        }
        if !iso_time_valid(h, mi, s, ms, us, ns) {
            return Err(Completion::Throw(interp.create_range_error("Invalid time")));
        }
        Ok((y, month, d, h, mi, s, ms, us, ns, cal))
    } else {
        let month = month.max(1).min(12);
        let dim = iso_days_in_month(y, month);
        let d = d.max(1).min(dim);
        let h = h.min(23);
        let mi = mi.min(59);
        let s = s.min(59);
        let ms = ms.min(999);
        let us = us.min(999);
        let ns = ns.min(999);
        Ok((y, month, d, h, mi, s, ms, us, ns, cal))
    }
}

fn get_pdt_fields(
    interp: &mut Interpreter,
    this: &JsValue,
) -> Result<(i32, u8, u8, u8, u8, u8, u16, u16, u16, String), Completion> {
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
                interp.create_type_error("not a Temporal.PlainDateTime"),
            ));
        }
    };
    let data = obj.borrow();
    match &data.temporal_data {
        Some(TemporalData::PlainDateTime {
            iso_year,
            iso_month,
            iso_day,
            hour,
            minute,
            second,
            millisecond,
            microsecond,
            nanosecond,
            calendar,
        }) => Ok((
            *iso_year,
            *iso_month,
            *iso_day,
            *hour,
            *minute,
            *second,
            *millisecond,
            *microsecond,
            *nanosecond,
            calendar.clone(),
        )),
        _ => Err(Completion::Throw(
            interp.create_type_error("not a Temporal.PlainDateTime"),
        )),
    }
}

fn format_plain_date_time(
    y: i32,
    m: u8,
    d: u8,
    h: u8,
    mi: u8,
    s: u8,
    ms: u16,
    us: u16,
    ns: u16,
    cal: &str,
    show_calendar: &str,
    precision: Option<i32>,
) -> String {
    let date_str = super::plain_date::format_plain_date(y, m, d, cal, "never");
    let time_str = super::plain_time::format_plain_time(h, mi, s, ms, us, ns, precision);
    let mut result = format!("{date_str}T{time_str}");
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

impl Interpreter {
    pub(crate) fn setup_temporal_plain_date_time(
        &mut self,
        temporal_obj: &Rc<RefCell<JsObjectData>>,
    ) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Temporal.PlainDateTime".to_string();
        {
            let key = "Symbol(Symbol.toStringTag)".to_string();
            let desc = PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str(
                    "Temporal.PlainDateTime",
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

        // Getter: calendarId
        {
            let getter = self.create_function(JsFunction::native(
                "get calendarId".to_string(),
                0,
                |interp, this, _args| {
                    let (_, _, _, _, _, _, _, _, _, cal) = match get_pdt_fields(interp, &this) {
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

        // Date getters: year, month, day + calendar computed
        for &(name, idx) in &[("year", 0u8), ("month", 1), ("day", 2)] {
            let getter = self.create_function(JsFunction::native(
                format!("get {name}"),
                0,
                move |interp, this, _args| {
                    let (y, m, d, _, _, _, _, _, _, cal) = match get_pdt_fields(interp, &this) {
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

        // Time getters: hour, minute, second, millisecond, microsecond, nanosecond
        for &(name, idx) in &[
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
                    let (_, _, _, h, mi, s, ms, us, ns, _) = match get_pdt_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let val = match idx {
                        0 => h as f64,
                        1 => mi as f64,
                        2 => s as f64,
                        3 => ms as f64,
                        4 => us as f64,
                        _ => ns as f64,
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

        // monthCode
        {
            let getter = self.create_function(JsFunction::native(
                "get monthCode".to_string(),
                0,
                |interp, this, _args| {
                    let (y, m, d, _, _, _, _, _, _, cal) = match get_pdt_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    if cal != "iso8601" {
                        if let Some(cf) = super::iso_to_calendar_fields(y, m, d, &cal) {
                            return Completion::Normal(JsValue::String(JsString::from_str(
                                &cf.month_code,
                            )));
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

        // Computed date getters
        for &(name, which) in &[
            ("dayOfWeek", 0u8),
            ("dayOfYear", 1),
            ("weekOfYear", 2),
            ("yearOfWeek", 3),
            ("daysInWeek", 4),
            ("daysInMonth", 5),
            ("daysInYear", 6),
            ("monthsInYear", 7),
            ("inLeapYear", 8),
        ] {
            let getter = self.create_function(JsFunction::native(
                format!("get {name}"),
                0,
                move |interp, this, _args| {
                    let (y, m, d, _, _, _, _, _, _, cal) = match get_pdt_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    if cal != "iso8601" {
                        if let Some(cf) = super::iso_to_calendar_fields(y, m, d, &cal) {
                            let val = match which {
                                0 => JsValue::Number(iso_day_of_week(y, m, d) as f64),
                                1 => JsValue::Number(cf.day_of_year as f64),
                                2 => {
                                    let (w, _) = iso_week_of_year(y, m, d);
                                    JsValue::Number(w as f64)
                                }
                                3 => {
                                    let (_, yw) = iso_week_of_year(y, m, d);
                                    JsValue::Number(yw as f64)
                                }
                                4 => JsValue::Number(7.0),
                                5 => JsValue::Number(cf.days_in_month as f64),
                                6 => JsValue::Number(cf.days_in_year as f64),
                                7 => JsValue::Number(cf.months_in_year as f64),
                                _ => JsValue::Boolean(cf.in_leap_year),
                            };
                            return Completion::Normal(val);
                        }
                    }
                    let val = match which {
                        0 => JsValue::Number(iso_day_of_week(y, m, d) as f64),
                        1 => JsValue::Number(iso_day_of_year(y, m, d) as f64),
                        2 => {
                            let (w, _) = iso_week_of_year(y, m, d);
                            JsValue::Number(w as f64)
                        }
                        3 => {
                            let (_, yw) = iso_week_of_year(y, m, d);
                            JsValue::Number(yw as f64)
                        }
                        4 => JsValue::Number(7.0),
                        5 => JsValue::Number(iso_days_in_month(y, m) as f64),
                        6 => JsValue::Number(iso_days_in_year(y) as f64),
                        7 => JsValue::Number(12.0),
                        _ => JsValue::Boolean(iso_is_leap_year(y)),
                    };
                    Completion::Normal(val)
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

        // era / eraYear
        for &(name, is_era) in &[("era", true), ("eraYear", false)] {
            let getter = self.create_function(JsFunction::native(
                format!("get {name}"),
                0,
                move |interp, this, _args| {
                    let (y, m, d, _, _, _, _, _, _, cal) = match get_pdt_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    if cal != "iso8601" {
                        if let Some(cf) = super::iso_to_calendar_fields(y, m, d, &cal) {
                            if is_era {
                                return Completion::Normal(match cf.era {
                                    Some(e) => JsValue::String(JsString::from_str(&e)),
                                    None => JsValue::Undefined,
                                });
                            } else {
                                return Completion::Normal(match cf.era_year {
                                    Some(ey) => JsValue::Number(ey as f64),
                                    None => JsValue::Undefined,
                                });
                            }
                        }
                    }
                    Completion::Normal(JsValue::Undefined)
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

        // with(temporalDateTimeLike, options?)
        let with_fn = self.create_function(JsFunction::native(
            "with".to_string(),
            1,
            |interp, this, args| {
                let (y, m, d, h, mi, s, ms, us, ns, cal) = match get_pdt_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let item = args.first().cloned().unwrap_or(JsValue::Undefined);
                // IsPartialTemporalObject
                if let Err(c) = is_partial_temporal_object(interp, &item) {
                    return c;
                }
                // PrepareCalendarFields in alphabetical order:
                // day, hour, microsecond, millisecond, minute, month, monthCode, nanosecond, second, year
                let mut has_any = false;
                let (new_d, has_d) = match read_field_positive_int(interp, &item, "day", d) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                has_any |= has_d;
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
                let (new_mi, has_mi) = match read_time_field_new(interp, &item, "minute", mi as f64)
                {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                has_any |= has_mi;
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
                // GetTemporalOverflowOption
                let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let overflow = match parse_overflow_option(interp, &options) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                // Resolve month/monthCode
                let new_m = match resolve_month_fields(interp, raw_month, raw_month_code, m) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if overflow == "reject" {
                    if !iso_date_valid(new_y, new_m, new_d) {
                        return Completion::Throw(interp.create_range_error("Invalid date"));
                    }
                    if !iso_time_valid_f64(new_h, new_mi, new_s, new_ms, new_us, new_ns) {
                        return Completion::Throw(interp.create_range_error("Invalid time"));
                    }
                    let (ch, cmi, cs, cms, cus, cns) = (
                        new_h as u8,
                        new_mi as u8,
                        new_s as u8,
                        new_ms as u16,
                        new_us as u16,
                        new_ns as u16,
                    );
                    if !iso_date_time_within_limits(new_y, new_m, new_d, ch, cmi, cs, cms, cus, cns)
                    {
                        return Completion::Throw(
                            interp.create_range_error("DateTime outside valid range"),
                        );
                    }
                    create_plain_date_time_result(
                        interp, new_y, new_m, new_d, ch, cmi, cs, cms, cus, cns, &cal,
                    )
                } else {
                    let cm = new_m.max(1).min(12);
                    let cd = new_d.max(1).min(iso_days_in_month(new_y, cm));
                    let ch = (new_h.max(0.0).min(23.0)) as u8;
                    let cmi = (new_mi.max(0.0).min(59.0)) as u8;
                    let cs = (new_s.max(0.0).min(59.0)) as u8;
                    let cms = (new_ms.max(0.0).min(999.0)) as u16;
                    let cus = (new_us.max(0.0).min(999.0)) as u16;
                    let cns = (new_ns.max(0.0).min(999.0)) as u16;
                    if !iso_date_time_within_limits(new_y, cm, cd, ch, cmi, cs, cms, cus, cns) {
                        return Completion::Throw(
                            interp.create_range_error("DateTime outside valid range"),
                        );
                    }
                    create_plain_date_time_result(
                        interp, new_y, cm, cd, ch, cmi, cs, cms, cus, cns, &cal,
                    )
                }
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("with".to_string(), with_fn);

        // withPlainTime(plainTimeLike?)
        let with_time_fn = self.create_function(JsFunction::native(
            "withPlainTime".to_string(),
            0,
            |interp, this, args| {
                let (y, m, d, _, _, _, _, _, _, cal) = match get_pdt_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let time_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let (h, mi, s, ms, us, ns) = if is_undefined(&time_arg) {
                    (0, 0, 0, 0, 0, 0)
                } else {
                    match super::plain_time::to_temporal_plain_time(interp, time_arg) {
                        Ok(v) => v,
                        Err(c) => return c,
                    }
                };
                if !super::iso_date_time_within_limits(y, m, d, h, mi, s, ms, us, ns) {
                    return Completion::Throw(
                        interp.create_range_error("DateTime outside valid ISO range"),
                    );
                }
                create_plain_date_time_result(interp, y, m, d, h, mi, s, ms, us, ns, &cal)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("withPlainTime".to_string(), with_time_fn);

        // withCalendar(calendar)
        let with_cal_fn = self.create_function(JsFunction::native(
            "withCalendar".to_string(),
            1,
            |interp, this, args| {
                let (y, m, d, h, mi, s, ms, us, ns, _) = match get_pdt_fields(interp, &this) {
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
                create_plain_date_time_result(interp, y, m, d, h, mi, s, ms, us, ns, &cal)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("withCalendar".to_string(), with_cal_fn);

        // add(duration, options?) / subtract(duration, options?)
        for &(name, sign) in &[("add", 1i32), ("subtract", -1i32)] {
            let fn_val = self.create_function(JsFunction::native(
                name.to_string(),
                1,
                move |interp, this, args| {
                    let (y, m, d, h, mi, s, ms, us, ns, cal) = match get_pdt_fields(interp, &this) {
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
                    let overflow = match parse_overflow_option(interp, &options) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    // Add time components using i128 for precision
                    let s128 = sign as i128;
                    let total_ns = time_to_nanoseconds(h, mi, s, ms, us, ns)
                        + (dur.4 as i128) * s128 * 3_600_000_000_000
                        + (dur.5 as i128) * s128 * 60_000_000_000
                        + (dur.6 as i128) * s128 * 1_000_000_000
                        + (dur.7 as i128) * s128 * 1_000_000
                        + (dur.8 as i128) * s128 * 1_000
                        + (dur.9 as i128) * s128;
                    let ns_per_day: i128 = 86_400_000_000_000;
                    let extra_days = if total_ns >= 0 {
                        (total_ns / ns_per_day) as i32
                    } else {
                        -(((-total_ns + ns_per_day - 1) / ns_per_day) as i32)
                    };
                    let rem_ns = ((total_ns % ns_per_day) + ns_per_day) % ns_per_day;
                    let (nh, nmi, nse, nms, nus, nns) = nanoseconds_to_time(rem_ns);
                    let total_days = (dur.3 as i32) * sign + extra_days;
                    let (ry, rm, rd) = match super::add_iso_date_with_overflow(
                        y,
                        m,
                        d,
                        (dur.0 as i32) * sign,
                        (dur.1 as i32) * sign,
                        (dur.2 as i32) * sign,
                        total_days,
                        &overflow,
                    ) {
                        Ok(v) => v,
                        Err(()) => {
                            return Completion::Throw(interp.create_range_error(
                                "Ambiguous date in add/subtract with reject overflow",
                            ));
                        }
                    };
                    if !super::iso_date_time_within_limits(ry, rm, rd, nh, nmi, nse, nms, nus, nns)
                    {
                        return Completion::Throw(
                            interp.create_range_error("Result date-time outside valid ISO range"),
                        );
                    }
                    create_plain_date_time_result(
                        interp, ry, rm, rd, nh, nmi, nse, nms, nus, nns, &cal,
                    )
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
                    let (y1, m1, d1, h1, mi1, s1, ms1, us1, ns1, _) =
                        match get_pdt_fields(interp, &this) {
                            Ok(v) => v,
                            Err(c) => return c,
                        };
                    let other = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let (y2, m2, d2, h2, mi2, s2, ms2, us2, ns2, _) =
                        match to_temporal_plain_date_time(interp, other) {
                            Ok(v) => v,
                            Err(c) => return c,
                        };
                    let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    let all_units: &[&str] = &[
                        "year",
                        "month",
                        "week",
                        "day",
                        "hour",
                        "minute",
                        "second",
                        "millisecond",
                        "microsecond",
                        "nanosecond",
                    ];
                    let (largest_unit, smallest_unit, rounding_mode, rounding_increment) =
                        match parse_difference_options(interp, &options, "day", all_units) {
                            Ok(v) => v,
                            Err(c) => return c,
                        };

                    let ns1_total = time_to_nanoseconds(h1, mi1, s1, ms1, us1, ns1);
                    let ns2_total = time_to_nanoseconds(h2, mi2, s2, ms2, us2, ns2);

                    let (
                        mut dy,
                        mut dm,
                        mut dw,
                        mut dd,
                        mut dh,
                        mut dmi,
                        mut ds,
                        mut dms,
                        mut dus,
                        mut dns,
                    ) = diff_date_time(y1, m1, d1, ns1_total, y2, m2, d2, ns2_total, &largest_unit);

                    // Per spec: for since, negate rounding mode, round signed values, then negate result
                    let effective_mode = if sign == -1 {
                        super::negate_rounding_mode(&rounding_mode)
                    } else {
                        rounding_mode.clone()
                    };

                    // Apply rounding on signed values
                    if smallest_unit != "nanosecond" || rounding_increment != 1.0 {
                        let su_order = super::temporal_unit_order(&smallest_unit);
                        if su_order >= super::temporal_unit_order("day") {
                            let time_ns_i128: i128 = dh as i128 * 3_600_000_000_000
                                + dmi as i128 * 60_000_000_000
                                + ds as i128 * 1_000_000_000
                                + dms as i128 * 1_000_000
                                + dus as i128 * 1_000
                                + dns as i128;
                            let time_ns = time_ns_i128 as f64;
                            let fractional_days = dd as f64 + time_ns / 86_400_000_000_000.0;
                            let (ry, rm, rd) = (y1, m1, d1);
                            // Pre-check: verify the NudgeToCalendarUnit end boundary is within limits
                            if matches!(smallest_unit.as_str(), "month" | "year") {
                                let dur_sign =
                                    if dy > 0 || dm > 0 || dw > 0 || fractional_days > 0.0 {
                                        1i64
                                    } else if dy < 0 || dm < 0 || dw < 0 || fractional_days < 0.0 {
                                        -1i64
                                    } else {
                                        1
                                    };
                                let inc = rounding_increment as i64;
                                let end_date = match smallest_unit.as_str() {
                                    "month" => {
                                        let end_m = dm as i64 + dur_sign * inc;
                                        super::add_iso_date(ry, rm, rd, dy, end_m as i32, 0, 0)
                                    }
                                    _ => {
                                        // year
                                        let end_y = dy as i64 + dur_sign * inc;
                                        super::add_iso_date(ry, rm, rd, end_y as i32, 0, 0, 0)
                                    }
                                };
                                if !super::iso_date_within_limits(
                                    end_date.0, end_date.1, end_date.2,
                                ) {
                                    return Completion::Throw(interp.create_range_error(
                                        "Rounded date outside valid ISO range",
                                    ));
                                }
                            }
                            let (ry2, rm2, rw2, rd2) =
                                match super::round_date_duration_with_frac_days(
                                    dy,
                                    dm,
                                    dw,
                                    fractional_days,
                                    time_ns_i128,
                                    &smallest_unit,
                                    &largest_unit,
                                    rounding_increment,
                                    &effective_mode,
                                    ry,
                                    rm,
                                    rd,
                                    false,
                                ) {
                                    Ok(v) => v,
                                    Err(msg) => {
                                        return Completion::Throw(interp.create_range_error(&msg));
                                    }
                                };
                            dy = ry2;
                            dm = rm2;
                            dw = rw2;
                            dd = rd2;
                            // Check that rounded date is within valid ISO range (calendar units only)
                            if matches!(smallest_unit.as_str(), "month" | "year") {
                                let rounded_end = super::add_iso_date(ry, rm, rd, dy, dm, dw, dd);
                                if !super::iso_date_within_limits(
                                    rounded_end.0,
                                    rounded_end.1,
                                    rounded_end.2,
                                ) {
                                    return Completion::Throw(interp.create_range_error(
                                        "Rounded date outside valid ISO range",
                                    ));
                                }
                            }
                            // Rebalance months overflow into years when largestUnit is year
                            if matches!(largest_unit.as_str(), "year") && dm.abs() >= 12 {
                                dy += dm / 12;
                                dm %= 12;
                            }
                            dh = 0;
                            dmi = 0;
                            ds = 0;
                            dms = 0;
                            dus = 0;
                            dns = 0;
                        } else {
                            let time_ns = dh as f64 * 3_600_000_000_000.0
                                + dmi as f64 * 60_000_000_000.0
                                + ds as f64 * 1_000_000_000.0
                                + dms as f64 * 1_000_000.0
                                + dus as f64 * 1_000.0
                                + dns as f64;
                            let unit_ns = super::temporal_unit_length_ns(&smallest_unit);
                            let increment_ns = unit_ns * rounding_increment;
                            let rounded_ns =
                                round_number_to_increment(time_ns, increment_ns, &effective_mode);
                            let total = rounded_ns as i64;
                            let lu_order = super::temporal_unit_order(&largest_unit);
                            let day_order = super::temporal_unit_order("day");
                            if lu_order < day_order {
                                // largest_unit is a time unit: decompose according to largest_unit
                                match largest_unit.as_str() {
                                    "hour" => {
                                        dns = total % 1000;
                                        let rem = total / 1000;
                                        dus = rem % 1000;
                                        let rem = rem / 1000;
                                        dms = rem % 1000;
                                        let rem = rem / 1000;
                                        ds = rem % 60;
                                        let rem = rem / 60;
                                        dmi = rem % 60;
                                        dh = rem / 60;
                                    }
                                    "minute" => {
                                        dns = total % 1000;
                                        let rem = total / 1000;
                                        dus = rem % 1000;
                                        let rem = rem / 1000;
                                        dms = rem % 1000;
                                        let rem = rem / 1000;
                                        ds = rem % 60;
                                        dmi = rem / 60;
                                        dh = 0;
                                    }
                                    "second" => {
                                        dns = total % 1000;
                                        let rem = total / 1000;
                                        dus = rem % 1000;
                                        let rem = rem / 1000;
                                        dms = rem % 1000;
                                        ds = rem / 1000;
                                        dmi = 0;
                                        dh = 0;
                                    }
                                    "millisecond" => {
                                        dns = total % 1000;
                                        let rem = total / 1000;
                                        dus = rem % 1000;
                                        dms = rem / 1000;
                                        ds = 0;
                                        dmi = 0;
                                        dh = 0;
                                    }
                                    "microsecond" => {
                                        dns = total % 1000;
                                        dus = total / 1000;
                                        dms = 0;
                                        ds = 0;
                                        dmi = 0;
                                        dh = 0;
                                    }
                                    _ => {
                                        // nanosecond
                                        dns = total;
                                        dus = 0;
                                        dms = 0;
                                        ds = 0;
                                        dmi = 0;
                                        dh = 0;
                                    }
                                }
                            } else {
                                // largest_unit is day or higher: decompose fully, cascade overflow
                                dns = total % 1000;
                                let rem = total / 1000;
                                dus = rem % 1000;
                                let rem = rem / 1000;
                                dms = rem % 1000;
                                let rem = rem / 1000;
                                ds = rem % 60;
                                let rem = rem / 60;
                                dmi = rem % 60;
                                let rem = rem / 60;
                                dh = rem;
                                if dh.abs() >= 24 {
                                    let day_overflow =
                                        (if dh >= 0 { dh / 24 } else { -((-dh) / 24) }) as i32;
                                    dh -= day_overflow as i64 * 24;
                                    dd += day_overflow;
                                    if lu_order >= super::temporal_unit_order("month") {
                                        let intermediate =
                                            super::add_iso_date(y1, m1, d1, dy, dm, 0, 0);
                                        let target = super::add_iso_date(
                                            intermediate.0,
                                            intermediate.1,
                                            intermediate.2,
                                            0,
                                            0,
                                            0,
                                            dd,
                                        );
                                        let (ny, nm, _, nd) = super::difference_iso_date(
                                            y1,
                                            m1,
                                            d1,
                                            target.0,
                                            target.1,
                                            target.2,
                                            &largest_unit,
                                        );
                                        dy = ny;
                                        dm = nm;
                                        dd = nd;
                                    }
                                }
                            }
                        }
                    }

                    // For since: negate the result
                    if sign == -1 {
                        dy = -dy;
                        dm = -dm;
                        dw = -dw;
                        dd = -dd;
                        dh = -dh;
                        dmi = -dmi;
                        ds = -ds;
                        dms = -dms;
                        dus = -dus;
                        dns = -dns;
                    }

                    super::duration::create_duration_result(
                        interp, dy as f64, dm as f64, dw as f64, dd as f64, dh as f64, dmi as f64,
                        ds as f64, dms as f64, dus as f64, dns as f64,
                    )
                },
            ));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
        }

        // round(options)
        let round_fn = self.create_function(JsFunction::native(
            "round".to_string(),
            1,
            |interp, this, args| {
                let (y, m, d, h, mi, s, ms, us, ns, cal) = match get_pdt_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let options = args.first().cloned().unwrap_or(JsValue::Undefined);
                let (unit, mode_str, increment) = if let JsValue::String(ref s) = options {
                    let u = match temporal_unit_singular(&s.to_rust_string()) {
                        Some(u) => u,
                        None => {
                            return Completion::Throw(interp.create_range_error("Invalid unit"));
                        }
                    };
                    (u, "halfExpand".to_string(), 1i128)
                } else if matches!(options, JsValue::Object(_)) {
                    // Read all options in alphabetical order first, then validate
                    // 1. roundingIncrement
                    let ri = match get_prop(interp, &options, "roundingIncrement") {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let inc_raw = match super::coerce_rounding_increment(interp, &ri) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    // 2. roundingMode
                    let rm = match get_prop(interp, &options, "roundingMode") {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let rm_str: Option<String> = if is_undefined(&rm) {
                        None
                    } else {
                        Some(match interp.to_string_value(&rm) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        })
                    };
                    // 3. smallestUnit
                    let su = match get_prop(interp, &options, "smallestUnit") {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let su_str: Option<String> = if is_undefined(&su) {
                        None
                    } else {
                        Some(match interp.to_string_value(&su) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        })
                    };
                    // Validate
                    let unit = if let Some(ref sv) = su_str {
                        match temporal_unit_singular(sv) {
                            Some(u) => u,
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
                    let mode_str = if let Some(ref rs) = rm_str {
                        match rs.as_str() {
                            "ceil" | "floor" | "trunc" | "expand" | "halfExpand" | "halfTrunc"
                            | "halfCeil" | "halfFloor" | "halfEven" => rs.clone(),
                            _ => {
                                return Completion::Throw(
                                    interp
                                        .create_range_error(&format!("Invalid roundingMode: {rs}")),
                                );
                            }
                        }
                    } else {
                        "halfExpand".to_string()
                    };
                    let inc = match super::validate_rounding_increment_raw(inc_raw, unit, false) {
                        Ok(v) => v as i128,
                        Err(msg) => return Completion::Throw(interp.create_range_error(&msg)),
                    };
                    (unit, mode_str, inc)
                } else {
                    return Completion::Throw(
                        interp.create_type_error("options required for round"),
                    );
                };

                if matches!(unit, "year" | "month" | "week") {
                    return Completion::Throw(
                        interp.create_range_error("Cannot round PlainDateTime to year/month/week"),
                    );
                }

                let total_ns = time_to_nanoseconds(h, mi, s, ms, us, ns);
                let unit_ns: i128 = match unit {
                    "day" => 86_400_000_000_000,
                    "hour" => 3_600_000_000_000,
                    "minute" => 60_000_000_000,
                    "second" => 1_000_000_000,
                    "millisecond" => 1_000_000,
                    "microsecond" => 1_000,
                    "nanosecond" => 1,
                    _ => {
                        return Completion::Throw(
                            interp.create_range_error(&format!("Invalid unit: {unit}")),
                        );
                    }
                };
                let inc_ns = unit_ns * increment;
                let rounded =
                    super::plain_time::round_i128_to_increment(total_ns, inc_ns, &mode_str);
                let ns_per_day: i128 = 86_400_000_000_000;
                let extra_days = if rounded >= 0 {
                    (rounded / ns_per_day) as i32
                } else {
                    -(((-rounded + ns_per_day - 1) / ns_per_day) as i32)
                };
                let rem_ns = ((rounded % ns_per_day) + ns_per_day) % ns_per_day;
                let (nh, nmi, nse, nms, nus, nns) = nanoseconds_to_time(rem_ns);
                let (ry, rm, rd) = add_iso_date(y, m, d, 0, 0, 0, extra_days);
                if !super::iso_date_time_within_limits(ry, rm, rd, nh, nmi, nse, nms, nus, nns) {
                    return Completion::Throw(
                        interp.create_range_error("Rounded DateTime outside valid ISO range"),
                    );
                }
                create_plain_date_time_result(interp, ry, rm, rd, nh, nmi, nse, nms, nus, nns, &cal)
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
                let a = match get_pdt_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let other = args.first().cloned().unwrap_or(JsValue::Undefined);
                let b = match to_temporal_plain_date_time(interp, other) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let eq = a.0 == b.0
                    && a.1 == b.1
                    && a.2 == b.2
                    && a.3 == b.3
                    && a.4 == b.4
                    && a.5 == b.5
                    && a.6 == b.6
                    && a.7 == b.7
                    && a.8 == b.8
                    && a.9 == b.9;
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
                let (y, m, d, h, mi, s, ms, us, ns, cal) = match get_pdt_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let options = args.first().cloned().unwrap_or(JsValue::Undefined);
                let has_opts = match super::get_options_object(interp, &options) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let (show_calendar, precision, rounding_mode) =
                    if has_opts {
                        let cv = match get_prop(interp, &options, "calendarName") {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let sc = if is_undefined(&cv) {
                            "auto"
                        } else {
                            let sv = match interp.to_string_value(&cv) {
                                Ok(v) => v,
                                Err(e) => return Completion::Throw(e),
                            };
                            match sv.as_str() {
                                "auto" => "auto",
                                "always" => "always",
                                "never" => "never",
                                "critical" => "critical",
                                _ => {
                                    return Completion::Throw(interp.create_range_error(&format!(
                                        "Invalid calendarName: {sv}"
                                    )));
                                }
                            }
                        };
                        // fractionalSecondDigits
                        let fsd = match get_prop(interp, &options, "fractionalSecondDigits") {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let mut prec: Option<i32> = if is_undefined(&fsd) {
                            None
                        } else if matches!(fsd, JsValue::Number(_)) {
                            let n = match interp.to_number_value(&fsd) {
                                Ok(v) => v,
                                Err(e) => return Completion::Throw(e),
                            };
                            if n.is_nan() || !n.is_finite() {
                                return Completion::Throw(interp.create_range_error(
                                    "fractionalSecondDigits must be 0-9 or 'auto'",
                                ));
                            }
                            let floored = n.floor();
                            if floored < 0.0 || floored > 9.0 {
                                return Completion::Throw(interp.create_range_error(
                                    "fractionalSecondDigits must be 0-9 or 'auto'",
                                ));
                            }
                            Some(floored as i32)
                        } else {
                            let s = match interp.to_string_value(&fsd) {
                                Ok(v) => v,
                                Err(e) => return Completion::Throw(e),
                            };
                            if s == "auto" {
                                None
                            } else {
                                return Completion::Throw(interp.create_range_error(
                                    "fractionalSecondDigits must be 0-9 or 'auto'",
                                ));
                            }
                        };
                        // roundingMode
                        let rm_val = match get_prop(interp, &options, "roundingMode") {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let rm: &str = if is_undefined(&rm_val) {
                            "trunc"
                        } else {
                            let sv = match interp.to_string_value(&rm_val) {
                                Ok(v) => v,
                                Err(e) => return Completion::Throw(e),
                            };
                            match sv.as_str() {
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
                                    return Completion::Throw(interp.create_range_error(&format!(
                                        "Invalid roundingMode: {sv}"
                                    )));
                                }
                            }
                        };
                        // smallestUnit overrides fractionalSecondDigits
                        let su_val = match get_prop(interp, &options, "smallestUnit") {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        if !is_undefined(&su_val) {
                            let sv = match interp.to_string_value(&su_val) {
                                Ok(v) => v,
                                Err(e) => return Completion::Throw(e),
                            };
                            prec = match super::temporal_unit_singular(&sv) {
                                Some("minute") => Some(-1),
                                Some("second") => Some(0),
                                Some("millisecond") => Some(3),
                                Some("microsecond") => Some(6),
                                Some("nanosecond") => Some(9),
                                _ => {
                                    return Completion::Throw(
                                        interp.create_range_error(&format!("Invalid unit: {sv}")),
                                    );
                                }
                            };
                        }
                        (sc, prec, rm)
                    } else {
                        ("auto", None, "trunc")
                    };
                // Apply rounding to the time component
                let (ry, rm, rd, rh, rmi, rs, rms, rus, rns) = if let Some(prec) = precision {
                    let time_ns = super::time_to_nanoseconds(h, mi, s, ms, us, ns);
                    let unit_ns: i128 = if prec == -1 {
                        60_000_000_000
                    } else if prec <= 0 {
                        1_000_000_000
                    } else {
                        10i128.pow(9 - prec as u32)
                    };
                    let rounded =
                        super::plain_time::round_i128_to_increment(time_ns, unit_ns, rounding_mode);
                    let ns_per_day: i128 = 86_400_000_000_000;
                    let extra_days = if rounded >= ns_per_day {
                        (rounded / ns_per_day) as i32
                    } else if rounded < 0 {
                        -(((-rounded + ns_per_day - 1) / ns_per_day) as i32)
                    } else {
                        0
                    };
                    let wrapped = ((rounded % ns_per_day) + ns_per_day) % ns_per_day;
                    let (th, tmi, ts, tms, tus, tns) = super::nanoseconds_to_time(wrapped);
                    let (ny, nm, nd) = if extra_days != 0 {
                        super::balance_iso_date(y, m as i32, d as i32 + extra_days)
                    } else {
                        (y, m, d)
                    };
                    (ny, nm, nd, th, tmi, ts, tms, tus, tns)
                } else {
                    (y, m, d, h, mi, s, ms, us, ns)
                };
                if !super::iso_date_time_within_limits(ry, rm, rd, rh, rmi, rs, rms, rus, rns) {
                    return Completion::Throw(
                        interp.create_range_error("Rounded DateTime outside valid ISO range"),
                    );
                }
                let result = format_plain_date_time(
                    ry,
                    rm,
                    rd,
                    rh,
                    rmi,
                    rs,
                    rms,
                    rus,
                    rns,
                    &cal,
                    show_calendar,
                    precision,
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
                let (y, m, d, h, mi, s, ms, us, ns, cal) = match get_pdt_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let result =
                    format_plain_date_time(y, m, d, h, mi, s, ms, us, ns, &cal, "auto", None);
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
                let (_y, _m, _d, _h, _mi, _s, _ms, _us, _ns, _cal) = match get_pdt_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let dtf_val = match interp.intl_date_time_format_ctor.clone() {
                    Some(v) => v,
                    None => {
                        let result = format_plain_date_time(_y, _m, _d, _h, _mi, _s, _ms, _us, _ns, &_cal, "auto", None);
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
        let value_of_fn = self.create_function(JsFunction::native(
            "valueOf".to_string(),
            0,
            |interp, _this, _args| {
                Completion::Throw(interp.create_type_error(
                    "use compare() or equals() to compare Temporal.PlainDateTime",
                ))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("valueOf".to_string(), value_of_fn);

        // toPlainDate()
        let to_pd_fn = self.create_function(JsFunction::native(
            "toPlainDate".to_string(),
            0,
            |interp, this, _args| {
                let (y, m, d, _, _, _, _, _, _, cal) = match get_pdt_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                super::plain_date::create_plain_date_result(interp, y, m, d, &cal)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toPlainDate".to_string(), to_pd_fn);

        // toPlainTime()
        let to_pt_fn = self.create_function(JsFunction::native(
            "toPlainTime".to_string(),
            0,
            |interp, this, _args| {
                let (_, _, _, h, mi, s, ms, us, ns, _) = match get_pdt_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                super::plain_time::create_plain_time_result(interp, h, mi, s, ms, us, ns)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toPlainTime".to_string(), to_pt_fn);

        // toPlainYearMonth()
        let to_ym_fn = self.create_function(JsFunction::native(
            "toPlainYearMonth".to_string(),
            0,
            |interp, this, _args| {
                let (y, m, _d, _, _, _, _, _, _, cal) = match get_pdt_fields(interp, &this) {
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
                let (_y, m, d, _, _, _, _, _, _, cal) = match get_pdt_fields(interp, &this) {
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

        // getISOFields()
        let get_iso_fn = self.create_function(JsFunction::native(
            "getISOFields".to_string(),
            0,
            |interp, this, _args| {
                let (y, m, d, h, mi, s, ms, us, ns, cal) = match get_pdt_fields(interp, &this) {
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
                    "isoHour".to_string(),
                    PropertyDescriptor::data(JsValue::Number(h as f64), true, true, true),
                );
                obj.borrow_mut().insert_property(
                    "isoMicrosecond".to_string(),
                    PropertyDescriptor::data(JsValue::Number(us as f64), true, true, true),
                );
                obj.borrow_mut().insert_property(
                    "isoMillisecond".to_string(),
                    PropertyDescriptor::data(JsValue::Number(ms as f64), true, true, true),
                );
                obj.borrow_mut().insert_property(
                    "isoMinute".to_string(),
                    PropertyDescriptor::data(JsValue::Number(mi as f64), true, true, true),
                );
                obj.borrow_mut().insert_property(
                    "isoMonth".to_string(),
                    PropertyDescriptor::data(JsValue::Number(m as f64), true, true, true),
                );
                obj.borrow_mut().insert_property(
                    "isoNanosecond".to_string(),
                    PropertyDescriptor::data(JsValue::Number(ns as f64), true, true, true),
                );
                obj.borrow_mut().insert_property(
                    "isoSecond".to_string(),
                    PropertyDescriptor::data(JsValue::Number(s as f64), true, true, true),
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

        // toZonedDateTime(timeZone, options?)
        let to_zdt_fn = self.create_function(JsFunction::native(
            "toZonedDateTime".to_string(),
            1,
            |interp, this, args| {
                let (y, m, d, h, mi, s, ms, us, ns, cal) = match get_pdt_fields(interp, &this) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let tz_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let tz = match super::to_temporal_time_zone_identifier(interp, &tz_arg) {
                    Ok(t) => t,
                    Err(c) => return c,
                };
                // Validate options: read disambiguation per spec
                let opts = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                if !super::is_undefined(&opts) && !matches!(opts, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("options must be an object"),
                    );
                }
                if matches!(opts, JsValue::Object(_)) {
                    let dis_val = match super::get_prop(interp, &opts, "disambiguation") {
                        Completion::Normal(v) => v,
                        c => return c,
                    };
                    if !super::is_undefined(&dis_val) {
                        let s = match interp.to_string_value(&dis_val) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };
                        if !matches!(s.as_str(), "compatible" | "earlier" | "later" | "reject") {
                            return Completion::Throw(interp.create_range_error(&format!(
                                "{s} is not a valid value for disambiguation"
                            )));
                        }
                    }
                }
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

        self.temporal_plain_date_time_prototype = Some(proto.clone());

        // Constructor
        let constructor = self.create_function(JsFunction::constructor(
            "PlainDateTime".to_string(),
            3,
            |interp, _this, args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Temporal.PlainDateTime must be called with new"),
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
                let h = match get_constructor_field(interp, args.get(3), 0, 23) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let mi = match get_constructor_field(interp, args.get(4), 0, 59) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let s = match get_constructor_field(interp, args.get(5), 0, 59) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let ms = match get_constructor_field_u16(interp, args.get(6), 0, 999) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let us = match get_constructor_field_u16(interp, args.get(7), 0, 999) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let ns = match get_constructor_field_u16(interp, args.get(8), 0, 999) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let cal_arg = args.get(9).cloned().unwrap_or(JsValue::Undefined);
                let cal = match super::validate_calendar_strict(interp, &cal_arg) {
                    Ok(c) => c,
                    Err(c) => return c,
                };
                if !iso_date_valid(y, m, d) {
                    return Completion::Throw(interp.create_range_error("Invalid date"));
                }
                if !super::iso_date_time_within_limits(y, m, d, h, mi, s, ms, us, ns) {
                    return Completion::Throw(
                        interp.create_range_error("DateTime outside valid ISO range"),
                    );
                }
                create_plain_date_time_result(interp, y, m, d, h, mi, s, ms, us, ns, &cal)
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

        // PlainDateTime.from(item, options?)
        let from_fn = self.create_function(JsFunction::native(
            "from".to_string(),
            1,
            |interp, _this, args| {
                let item = args.first().cloned().unwrap_or(JsValue::Undefined);
                let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                // Per spec: if item is a string, parse first, then validate overflow (but don't use it)
                if matches!(&item, JsValue::String(_)) {
                    let (y, m, d, h, mi, s, ms, us, ns, cal) =
                        match to_temporal_plain_date_time_with_overflow(interp, item, "constrain") {
                            Ok(v) => v,
                            Err(c) => return c,
                        };
                    match parse_overflow_option(interp, &options) {
                        Ok(_) => {}
                        Err(c) => return c,
                    }
                    if !super::iso_date_time_within_limits(y, m, d, h, mi, s, ms, us, ns) {
                        return Completion::Throw(
                            interp.create_range_error("DateTime outside valid ISO range"),
                        );
                    }
                    return create_plain_date_time_result(
                        interp, y, m, d, h, mi, s, ms, us, ns, &cal,
                    );
                }
                // Check if it's a Temporal object (read overflow first, then return copy)
                let is_temporal = if let JsValue::Object(ref o) = item {
                    if let Some(obj) = interp.get_object(o.id) {
                        let data = obj.borrow();
                        matches!(
                            &data.temporal_data,
                            Some(TemporalData::PlainDateTime { .. })
                                | Some(TemporalData::PlainDate { .. })
                                | Some(TemporalData::ZonedDateTime { .. })
                        )
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
                    let (y, m, d, h, mi, s, ms, us, ns, cal) =
                        match to_temporal_plain_date_time_with_overflow(interp, item, &overflow) {
                            Ok(v) => v,
                            Err(c) => return c,
                        };
                    create_plain_date_time_result(interp, y, m, d, h, mi, s, ms, us, ns, &cal)
                } else {
                    // Property bag: read fields first, then overflow
                    let (y_f, month_num, mc_str, d_f, h, mi, s, ms, us, ns, cal) =
                        match read_pdt_property_bag(interp, &item) {
                            Ok(v) => v,
                            Err(c) => return c,
                        };
                    let overflow = match parse_overflow_option(interp, &options) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let (y, m, d, h, mi, s, ms, us, ns, cal) = match apply_pdt_overflow(
                        interp, y_f, month_num, mc_str, d_f, h, mi, s, ms, us, ns, cal, &overflow,
                    ) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    if !super::iso_date_time_within_limits(y, m, d, h, mi, s, ms, us, ns) {
                        return Completion::Throw(
                            interp.create_range_error("DateTime outside valid ISO range"),
                        );
                    }
                    create_plain_date_time_result(interp, y, m, d, h, mi, s, ms, us, ns, &cal)
                }
            },
        ));
        if let JsValue::Object(ref o) = constructor {
            if let Some(obj) = self.get_object(o.id) {
                obj.borrow_mut().insert_builtin("from".to_string(), from_fn);
            }
        }

        // PlainDateTime.compare(one, two)
        let compare_fn = self.create_function(JsFunction::native(
            "compare".to_string(),
            2,
            |interp, _this, args| {
                let a = match to_temporal_plain_date_time(
                    interp,
                    args.first().cloned().unwrap_or(JsValue::Undefined),
                ) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let b = match to_temporal_plain_date_time(
                    interp,
                    args.get(1).cloned().unwrap_or(JsValue::Undefined),
                ) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let cmp = (a.0, a.1, a.2, a.3, a.4, a.5, a.6, a.7, a.8)
                    .cmp(&(b.0, b.1, b.2, b.3, b.4, b.5, b.6, b.7, b.8));
                let result = match cmp {
                    std::cmp::Ordering::Less => -1.0,
                    std::cmp::Ordering::Equal => 0.0,
                    std::cmp::Ordering::Greater => 1.0,
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
            "PlainDateTime".to_string(),
            PropertyDescriptor::data(constructor, true, false, true),
        );
    }
}

fn get_constructor_field(
    interp: &mut Interpreter,
    arg: Option<&JsValue>,
    min: u8,
    max: u8,
) -> Result<u8, Completion> {
    let val = arg.cloned().unwrap_or(JsValue::Undefined);
    if is_undefined(&val) {
        return Ok(0);
    }
    match interp.to_number_value(&val) {
        Ok(n) => {
            if !n.is_finite() {
                return Err(Completion::Throw(
                    interp.create_range_error("Time field out of range"),
                ));
            }
            let t = n.trunc();
            if t < min as f64 || t > max as f64 {
                Err(Completion::Throw(
                    interp.create_range_error("Time field out of range"),
                ))
            } else {
                Ok(t as u8)
            }
        }
        Err(e) => Err(Completion::Throw(e)),
    }
}

fn get_constructor_field_u16(
    interp: &mut Interpreter,
    arg: Option<&JsValue>,
    min: u16,
    max: u16,
) -> Result<u16, Completion> {
    let val = arg.cloned().unwrap_or(JsValue::Undefined);
    if is_undefined(&val) {
        return Ok(0);
    }
    match interp.to_number_value(&val) {
        Ok(n) => {
            if !n.is_finite() {
                return Err(Completion::Throw(
                    interp.create_range_error("Time field out of range"),
                ));
            }
            let t = n.trunc();
            if t < min as f64 || t > max as f64 {
                Err(Completion::Throw(
                    interp.create_range_error("Time field out of range"),
                ))
            } else {
                Ok(t as u16)
            }
        }
        Err(e) => Err(Completion::Throw(e)),
    }
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

fn get_date_field_u16(
    interp: &mut Interpreter,
    obj: &JsValue,
    key: &str,
    default: u16,
) -> Result<u16, Completion> {
    let val = match get_prop(interp, obj, key) {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    if is_undefined(&val) {
        Ok(default)
    } else {
        Ok(to_integer_with_truncation(interp, &val)? as u16)
    }
}

fn diff_date_time(
    y1: i32,
    m1: u8,
    d1: u8,
    ns1: i128,
    y2: i32,
    m2: u8,
    d2: u8,
    ns2: i128,
    largest_unit: &str,
) -> (i32, i32, i32, i32, i64, i64, i64, i64, i64, i64) {
    let time_units = matches!(
        largest_unit,
        "hour" | "minute" | "second" | "millisecond" | "microsecond" | "nanosecond"
    );

    if time_units {
        // Flatten to total nanoseconds
        let epoch1 = crate::interpreter::builtins::temporal::iso_date_to_epoch_days(y1, m1, d1)
            as i128
            * 86_400_000_000_000i128
            + ns1;
        let epoch2 = crate::interpreter::builtins::temporal::iso_date_to_epoch_days(y2, m2, d2)
            as i128
            * 86_400_000_000_000i128
            + ns2;
        let diff = epoch2 - epoch1;
        let (hours, minutes, seconds, milliseconds, microseconds, nanoseconds) = match largest_unit
        {
            "hour" => {
                let h = diff / 3_600_000_000_000;
                let rem = diff % 3_600_000_000_000;
                let mi = rem / 60_000_000_000;
                let rem = rem % 60_000_000_000;
                let s = rem / 1_000_000_000;
                let rem = rem % 1_000_000_000;
                let ms = rem / 1_000_000;
                let rem = rem % 1_000_000;
                let us = rem / 1_000;
                let ns = rem % 1_000;
                (h, mi, s, ms, us, ns)
            }
            "minute" => {
                let mi = diff / 60_000_000_000;
                let rem = diff % 60_000_000_000;
                let s = rem / 1_000_000_000;
                let rem = rem % 1_000_000_000;
                let ms = rem / 1_000_000;
                let rem = rem % 1_000_000;
                let us = rem / 1_000;
                let ns = rem % 1_000;
                (0, mi, s, ms, us, ns)
            }
            "second" => {
                let s = diff / 1_000_000_000;
                let rem = diff % 1_000_000_000;
                let ms = rem / 1_000_000;
                let rem = rem % 1_000_000;
                let us = rem / 1_000;
                let ns = rem % 1_000;
                (0, 0, s, ms, us, ns)
            }
            "millisecond" => {
                let ms = diff / 1_000_000;
                let rem = diff % 1_000_000;
                let us = rem / 1_000;
                let ns = rem % 1_000;
                (0, 0, 0, ms, us, ns)
            }
            "microsecond" => {
                let us = diff / 1_000;
                let ns = diff % 1_000;
                (0, 0, 0, 0, us, ns)
            }
            _ => (0, 0, 0, 0, 0, diff), // nanosecond
        };
        (
            0,
            0,
            0,
            0,
            hours as i64,
            minutes as i64,
            seconds as i64,
            milliseconds as i64,
            microseconds as i64,
            nanoseconds as i64,
        )
    } else {
        // DifferenceISODateTime spec: compute natively in both directions
        let time_diff_ns = ns2 - ns1; // end_time - start_time
        let time_sign: i32 = if time_diff_ns > 0 {
            1
        } else if time_diff_ns < 0 {
            -1
        } else {
            0
        };
        let date1_epoch =
            crate::interpreter::builtins::temporal::iso_date_to_epoch_days(y1, m1, d1);
        let date2_epoch =
            crate::interpreter::builtins::temporal::iso_date_to_epoch_days(y2, m2, d2);
        let date_sign: i32 = if date1_epoch > date2_epoch {
            1
        } else if date1_epoch < date2_epoch {
            -1
        } else {
            0
        };

        // Per spec step 7: if timeSign = dateSign (same non-zero sign), adjust
        let (adjusted_end, time_ns) = if time_sign != 0 && time_sign == date_sign {
            let adj_epoch = date2_epoch + time_sign as i64;
            let adj_date =
                crate::interpreter::builtins::temporal::epoch_days_to_iso_date(adj_epoch);
            let adj_time = time_diff_ns - time_sign as i128 * 86_400_000_000_000;
            (adj_date, adj_time)
        } else {
            ((y2, m2, d2), time_diff_ns)
        };

        let (dy, dm, dw, dd) = difference_iso_date(
            y1,
            m1,
            d1,
            adjusted_end.0,
            adjusted_end.1,
            adjusted_end.2,
            largest_unit,
        );

        // Decompose time_ns (may be negative)
        let h = time_ns / 3_600_000_000_000;
        let rem = time_ns % 3_600_000_000_000;
        let mi = rem / 60_000_000_000;
        let rem = rem % 60_000_000_000;
        let s = rem / 1_000_000_000;
        let rem = rem % 1_000_000_000;
        let ms = rem / 1_000_000;
        let rem = rem % 1_000_000;
        let us = rem / 1_000;
        let ns = rem % 1_000;
        (
            dy, dm, dw, dd, h as i64, mi as i64, s as i64, ms as i64, us as i64, ns as i64,
        )
    }
}
