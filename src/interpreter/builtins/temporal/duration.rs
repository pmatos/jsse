use super::*;
use crate::interpreter::builtins::temporal::{
    add_iso_date, default_largest_unit_for_duration, duration_sign, get_prop, is_undefined,
    is_valid_duration, iso_date_to_epoch_days, parse_temporal_duration_string,
    round_number_to_increment, temporal_unit_length_ns, temporal_unit_order,
    temporal_unit_singular, to_integer_if_integral, validate_rounding_increment,
};

macro_rules! try_completion {
    ($expr:expr) => {
        match $expr {
            Completion::Normal(v) => v,
            other => return other,
        }
    };
}

macro_rules! try_result {
    ($interp:expr, $expr:expr) => {
        match $expr {
            Ok(v) => v,
            Err(e) => return Completion::Throw(e),
        }
    };
}

/// Extract a PlainDate (year, month, day) from a relativeTo value.
/// Accepts PlainDate, PlainDateTime, ZonedDateTime objects, property bags, or strings.
/// For ZonedDateTime-like inputs, extracts just the date portion.
fn to_relative_to_date(
    interp: &mut Interpreter,
    val: &JsValue,
) -> Result<Option<(i32, u8, u8)>, Completion> {
    if is_undefined(val) {
        return Ok(None);
    }
    // For strings, handle ZonedDateTime-like strings specially
    if let JsValue::String(s) = val {
        let raw = s.to_rust_string();
        // Reject year zero (-000000)
        if raw.starts_with("-000000") {
            return Err(Completion::Throw(
                interp.create_range_error("negative zero year is not allowed"),
            ));
        }
        // For ZonedDateTime strings (have timezone bracket and offset),
        // validate offset-timezone consistency, then extract date
        if let Some(bracket_pos) = raw.find('[') {
            let after = &raw[bracket_pos + 1..];
            if !after.starts_with("u-ca=") {
                // This is a ZonedDateTime string
                if let Some(parsed) = super::parse_temporal_date_time_string(&raw) {
                    if let Some(ref offset) = parsed.offset {
                        let tz_end = after.find(']').unwrap_or(after.len());
                        let tz_name = &after[..tz_end];
                        // For known fixed-offset timezones, validate offset matches
                        if tz_name == "UTC" || tz_name == "Etc/UTC" || tz_name == "Etc/GMT" {
                            let is_zero = offset.hours == 0
                                && offset.minutes == 0
                                && offset.seconds == 0
                                && offset.nanoseconds == 0;
                            if !is_zero {
                                return Err(Completion::Throw(interp.create_range_error(
                                    "UTC offset mismatch in ZonedDateTime string",
                                )));
                            }
                        }
                    }
                    return Ok(Some((parsed.year, parsed.month, parsed.day)));
                }
                return Err(Completion::Throw(
                    interp.create_range_error(&format!("Invalid relativeTo string: {raw}")),
                ));
            }
        }
    }
    // For property bags with timeZone, validate timezone string for year-zero
    if let JsValue::Object(_) = val {
        let tz_val = match get_prop(interp, val, "timeZone") {
            Completion::Normal(v) => v,
            other => return Err(other),
        };
        if let JsValue::String(ref tz_str) = tz_val {
            let tz_raw = tz_str.to_rust_string();
            if tz_raw.starts_with("-000000") {
                return Err(Completion::Throw(
                    interp.create_range_error("negative zero year is not allowed in timeZone"),
                ));
            }
        }
    }
    // Extract date portion
    let (y, m, d, _) = super::plain_date::to_temporal_plain_date(interp, val.clone())?;
    Ok(Some((y, m, d)))
}

/// Compute total nanoseconds for a duration relative to a PlainDate.
/// Calendar units (years, months, weeks) are resolved by adding them to the date
/// and computing the epoch-day difference, then multiplied by ns/day.
/// Returns Err(()) if the result is out of range.
fn duration_total_ns_relative(
    y: f64,
    mo: f64,
    w: f64,
    d: f64,
    h: f64,
    mi: f64,
    s: f64,
    ms: f64,
    us: f64,
    ns: f64,
    base_year: i32,
    base_month: u8,
    base_day: u8,
) -> Result<i128, ()> {
    let (ry, rm, rd) = add_iso_date(
        base_year,
        base_month,
        base_day,
        y as i32,
        mo as i32,
        w as i32,
        d as i32,
    );
    if ry < -275760 || ry > 275760 {
        return Err(());
    }
    let base_epoch = iso_date_to_epoch_days(base_year, base_month, base_day);
    let result_epoch = iso_date_to_epoch_days(ry, rm, rd);
    let total_days = (result_epoch - base_epoch) as i128;
    let time_ns = h as i128 * 3_600_000_000_000
        + mi as i128 * 60_000_000_000
        + s as i128 * 1_000_000_000
        + ms as i128 * 1_000_000
        + us as i128 * 1_000
        + ns as i128;
    let total = total_days * 86_400_000_000_000 + time_ns;
    // Add24HourDaysToNormalizedTimeDuration range check
    let limit = (1i128 << 53) * 1_000_000_000;
    if total.abs() > limit {
        return Err(());
    }
    Ok(total)
}

impl Interpreter {
    pub(crate) fn setup_temporal_duration(&mut self, temporal_obj: &Rc<RefCell<JsObjectData>>) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Temporal.Duration".to_string();

        // @@toStringTag
        {
            let key = "Symbol(Symbol.toStringTag)".to_string();
            let desc = PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("Temporal.Duration"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            };
            proto.borrow_mut().property_order.push(key.clone());
            proto.borrow_mut().properties.insert(key, desc);
        }

        // Accessor properties for the 10 components + sign + blank
        let component_names = [
            "years",
            "months",
            "weeks",
            "days",
            "hours",
            "minutes",
            "seconds",
            "milliseconds",
            "microseconds",
            "nanoseconds",
            "sign",
            "blank",
        ];
        for &name in &component_names {
            let getter = self.create_function(JsFunction::native(
                format!("get {name}"),
                0,
                move |interp, this, _args| {
                    let obj = match &this {
                        JsValue::Object(o) => match interp.get_object(o.id) {
                            Some(obj) => obj,
                            None => {
                                return Completion::Throw(
                                    interp.create_type_error("invalid object"),
                                );
                            }
                        },
                        _ => return Completion::Throw(interp.create_type_error(&format!(
                            "get Temporal.Duration.prototype.{name} requires a Temporal.Duration"
                        ))),
                    };
                    let data = obj.borrow();
                    match &data.temporal_data {
                        Some(TemporalData::Duration {
                            years,
                            months,
                            weeks,
                            days,
                            hours,
                            minutes,
                            seconds,
                            milliseconds,
                            microseconds,
                            nanoseconds,
                        }) => {
                            let val = match name {
                                "years" => JsValue::Number(*years),
                                "months" => JsValue::Number(*months),
                                "weeks" => JsValue::Number(*weeks),
                                "days" => JsValue::Number(*days),
                                "hours" => JsValue::Number(*hours),
                                "minutes" => JsValue::Number(*minutes),
                                "seconds" => JsValue::Number(*seconds),
                                "milliseconds" => JsValue::Number(*milliseconds),
                                "microseconds" => JsValue::Number(*microseconds),
                                "nanoseconds" => JsValue::Number(*nanoseconds),
                                "sign" => JsValue::Number(duration_sign(
                                    *years,
                                    *months,
                                    *weeks,
                                    *days,
                                    *hours,
                                    *minutes,
                                    *seconds,
                                    *milliseconds,
                                    *microseconds,
                                    *nanoseconds,
                                ) as f64),
                                "blank" => JsValue::Boolean(
                                    duration_sign(
                                        *years,
                                        *months,
                                        *weeks,
                                        *days,
                                        *hours,
                                        *minutes,
                                        *seconds,
                                        *milliseconds,
                                        *microseconds,
                                        *nanoseconds,
                                    ) == 0,
                                ),
                                _ => JsValue::Undefined,
                            };
                            Completion::Normal(val)
                        }
                        _ => Completion::Throw(interp.create_type_error(&format!(
                            "get Temporal.Duration.prototype.{name} requires a Temporal.Duration"
                        ))),
                    }
                },
            ));
            let desc = PropertyDescriptor {
                value: None,
                writable: None,
                enumerable: Some(false),
                configurable: Some(true),
                get: Some(getter),
                set: None,
            };
            proto.borrow_mut().insert_property(name.to_string(), desc);
        }

        // negated()
        let negated_fn = self.create_function(JsFunction::native(
            "negated".to_string(),
            0,
            |interp, this, _args| {
                let fields = match get_duration_fields(interp, &this) {
                    Ok(f) => f,
                    Err(c) => return c,
                };
                let (y, mo, w, d, h, mi, s, ms, us, ns) = fields;
                create_duration_result(interp, -y, -mo, -w, -d, -h, -mi, -s, -ms, -us, -ns)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("negated".to_string(), negated_fn);

        // abs()
        let abs_fn = self.create_function(JsFunction::native(
            "abs".to_string(),
            0,
            |interp, this, _args| {
                let fields = match get_duration_fields(interp, &this) {
                    Ok(f) => f,
                    Err(c) => return c,
                };
                let (y, mo, w, d, h, mi, s, ms, us, ns) = fields;
                create_duration_result(
                    interp,
                    y.abs(),
                    mo.abs(),
                    w.abs(),
                    d.abs(),
                    h.abs(),
                    mi.abs(),
                    s.abs(),
                    ms.abs(),
                    us.abs(),
                    ns.abs(),
                )
            },
        ));
        proto.borrow_mut().insert_builtin("abs".to_string(), abs_fn);

        // with(durationLike)
        let with_fn = self.create_function(JsFunction::native(
            "with".to_string(),
            1,
            |interp, this, args| {
                let fields = match get_duration_fields(interp, &this) {
                    Ok(f) => f,
                    Err(c) => return c,
                };
                let (y, mo, w, d, h, mi, s, ms, us, ns) = fields;
                let like = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(like, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("Duration.with requires an object argument"),
                    );
                }
                macro_rules! get_field {
                    ($name:expr, $default:expr) => {{
                        let v = try_completion!(get_prop(interp, &like, $name));
                        if is_undefined(&v) {
                            (false, $default)
                        } else {
                            let n = try_result!(interp, interp.to_number_value(&v));
                            match to_integer_if_integral(n) {
                                Some(i) => (true, i),
                                None => {
                                    return Completion::Throw(interp.create_range_error(&format!(
                                        "{} must be an integer",
                                        $name
                                    )));
                                }
                            }
                        }
                    }};
                }
                let (hy, ny) = get_field!("years", y);
                let (hmo, nmo) = get_field!("months", mo);
                let (hw, nw) = get_field!("weeks", w);
                let (hd, nd) = get_field!("days", d);
                let (hh, nh) = get_field!("hours", h);
                let (hmi, nmi) = get_field!("minutes", mi);
                let (hs, ns_val) = get_field!("seconds", s);
                let (hms, nms) = get_field!("milliseconds", ms);
                let (hus, nus) = get_field!("microseconds", us);
                let (hns, nns) = get_field!("nanoseconds", ns);
                if !hy && !hmo && !hw && !hd && !hh && !hmi && !hs && !hms && !hus && !hns {
                    return Completion::Throw(interp.create_type_error(
                        "Invalid duration-like object: at least one duration property must be present",
                    ));
                }
                create_duration_result(interp, ny, nmo, nw, nd, nh, nmi, ns_val, nms, nus, nns)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("with".to_string(), with_fn);

        // add(other)
        let add_fn = self.create_function(JsFunction::native(
            "add".to_string(),
            1,
            |interp, this, args| {
                let fields = match get_duration_fields(interp, &this) {
                    Ok(f) => f,
                    Err(c) => return c,
                };
                let (y, mo, w, d, h, mi, s, ms, us, ns) = fields;
                let other = match to_temporal_duration_record(
                    interp,
                    args.first().cloned().unwrap_or(JsValue::Undefined),
                ) {
                    Ok(f) => f,
                    Err(c) => return c,
                };
                if y != 0.0 || mo != 0.0 || w != 0.0 || other.0 != 0.0 || other.1 != 0.0 || other.2 != 0.0 {
                    return Completion::Throw(interp.create_range_error(
                        "Duration.add cannot use calendar units (years, months, weeks) without relativeTo",
                    ));
                }
                let (ry, rmo, rw, rd, rh, rmi, rs, rms, rus, rns) = balance_duration_relative(
                    y + other.0,
                    mo + other.1,
                    w + other.2,
                    d + other.3,
                    h + other.4,
                    mi + other.5,
                    s + other.6,
                    ms + other.7,
                    us + other.8,
                    ns + other.9,
                );
                create_duration_result(interp, ry, rmo, rw, rd, rh, rmi, rs, rms, rus, rns)
            },
        ));
        proto.borrow_mut().insert_builtin("add".to_string(), add_fn);

        // subtract(other)
        let subtract_fn = self.create_function(JsFunction::native(
            "subtract".to_string(),
            1,
            |interp, this, args| {
                let fields = match get_duration_fields(interp, &this) {
                    Ok(f) => f,
                    Err(c) => return c,
                };
                let (y, mo, w, d, h, mi, s, ms, us, ns) = fields;
                let other = match to_temporal_duration_record(
                    interp,
                    args.first().cloned().unwrap_or(JsValue::Undefined),
                ) {
                    Ok(f) => f,
                    Err(c) => return c,
                };
                if y != 0.0 || mo != 0.0 || w != 0.0 || other.0 != 0.0 || other.1 != 0.0 || other.2 != 0.0 {
                    return Completion::Throw(interp.create_range_error(
                        "Duration.subtract cannot use calendar units (years, months, weeks) without relativeTo",
                    ));
                }
                let (ry, rmo, rw, rd, rh, rmi, rs, rms, rus, rns) = balance_duration_relative(
                    y - other.0,
                    mo - other.1,
                    w - other.2,
                    d - other.3,
                    h - other.4,
                    mi - other.5,
                    s - other.6,
                    ms - other.7,
                    us - other.8,
                    ns - other.9,
                );
                create_duration_result(interp, ry, rmo, rw, rd, rh, rmi, rs, rms, rus, rns)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("subtract".to_string(), subtract_fn);

        // round(roundTo)
        let round_fn = self.create_function(JsFunction::native(
            "round".to_string(),
            1,
            |interp, this, args| {
                let fields = match get_duration_fields(interp, &this) {
                    Ok(f) => f,
                    Err(c) => return c,
                };
                let (y, mo, w, d, h, mi, s, ms, us, ns) = fields;
                let round_to = args.first().cloned().unwrap_or(JsValue::Undefined);
                if is_undefined(&round_to) {
                    return Completion::Throw(
                        interp.create_type_error("round requires options argument"),
                    );
                }

                let (smallest_unit, rounding_mode, increment, largest_unit) =
                    match parse_round_options(interp, &round_to, y, mo, w, d, h, mi, s, ms, us, ns)
                    {
                        Ok(opts) => opts,
                        Err(c) => return c,
                    };

                if temporal_unit_order(largest_unit) < temporal_unit_order(smallest_unit) {
                    return Completion::Throw(interp.create_range_error(
                        "largestUnit must be at least as large as smallestUnit",
                    ));
                }

                // Parse relativeTo from options
                let relative_to = if matches!(round_to, JsValue::Object(_)) {
                    let rt = match get_prop(interp, &round_to, "relativeTo") {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    match to_relative_to_date(interp, &rt) {
                        Ok(v) => v,
                        Err(c) => return c,
                    }
                } else {
                    None
                };

                // Calendar units require relativeTo
                let has_calendar = y != 0.0 || mo != 0.0 || w != 0.0;
                if has_calendar && relative_to.is_none() {
                    return Completion::Throw(interp.create_range_error(
                        "relativeTo is required for rounding durations with calendar units",
                    ));
                }

                // If we have relativeTo, use it to resolve calendar units
                if let Some((by, bm, bd)) = relative_to {
                    let su_order = temporal_unit_order(smallest_unit);

                    if su_order >= temporal_unit_order("day") {
                        // Calendar/day unit rounding: use NudgeCalendarUnit approach
                        let time_ns = h * 3_600_000_000_000.0
                            + mi * 60_000_000_000.0
                            + s * 1_000_000_000.0
                            + ms * 1_000_000.0
                            + us * 1_000.0
                            + ns;
                        let frac_days = w * 7.0 + d + time_ns / 86_400_000_000_000.0;
                        // Range check: verify end date is within ISO limits
                        let check_end = add_iso_date(by, bm, bd, y as i32, mo as i32, 0, frac_days.trunc() as i32);
                        if !super::iso_date_within_limits(check_end.0, check_end.1, check_end.2) {
                            return Completion::Throw(interp.create_range_error(
                                "duration out of range when applied to relativeTo",
                            ));
                        }
                        let (mut ry, mut rm, rw, rd) = super::round_date_duration_with_frac_days(
                            y as i32, mo as i32, 0, frac_days,
                            smallest_unit, increment, rounding_mode,
                            by, bm, bd,
                        );
                        // Re-balance based on largest_unit
                        if matches!(largest_unit, "month") {
                            rm += ry * 12;
                            ry = 0;
                        }
                        create_duration_result(
                            interp, ry as f64, rm as f64, rw as f64, rd as f64,
                            0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                        )
                    } else {
                        // Time unit rounding: flatten to ns
                        let total_ns = match duration_total_ns_relative(
                            y, mo, w, d, h, mi, s, ms, us, ns, by, bm, bd,
                        ) {
                            Ok(v) => v as f64,
                            Err(()) => return Completion::Throw(interp.create_range_error(
                                "duration out of range when applied to relativeTo",
                            )),
                        };
                        let unit_ns = temporal_unit_length_ns(smallest_unit);
                        let rounded_ns =
                            round_number_to_increment(total_ns, unit_ns * increment, rounding_mode);
                        if matches!(largest_unit, "year" | "month" | "week") {
                            let total_days = (rounded_ns / 86_400_000_000_000.0).trunc();
                            let remainder_ns = rounded_ns - total_days * 86_400_000_000_000.0;
                            let (ry, rm, rd_result) =
                                add_iso_date(by, bm, bd, 0, 0, 0, total_days as i32);
                            let (dy, dm, _dw, dd) = super::difference_iso_date(
                                by, bm, bd, ry, rm, rd_result, largest_unit,
                            );
                            let (_, rh, rmi, rs, rms, rus, rns) =
                                unbalance_time_ns(remainder_ns, "hour");
                            create_duration_result(
                                interp, dy as f64, dm as f64, 0.0, dd as f64, rh, rmi, rs, rms, rus, rns,
                            )
                        } else {
                            let (rd, rh, rmi, rs, rms, rus, rns) =
                                unbalance_time_ns(rounded_ns, largest_unit);
                            create_duration_result(interp, 0.0, 0.0, 0.0, rd, rh, rmi, rs, rms, rus, rns)
                        }
                    }
                } else {
                    let total_ns = d * 86_400_000_000_000.0
                        + h * 3_600_000_000_000.0
                        + mi * 60_000_000_000.0
                        + s * 1_000_000_000.0
                        + ms * 1_000_000.0
                        + us * 1_000.0
                        + ns;

                    let unit_ns = temporal_unit_length_ns(smallest_unit);
                    let rounded_ns =
                        round_number_to_increment(total_ns, unit_ns * increment, rounding_mode);
                    let (rd, rh, rmi, rs, rms, rus, rns) =
                        unbalance_time_ns(rounded_ns, largest_unit);
                    create_duration_result(interp, y, mo, w, rd, rh, rmi, rs, rms, rus, rns)
                }
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("round".to_string(), round_fn);

        // total(totalOf)
        let total_fn = self.create_function(JsFunction::native(
            "total".to_string(),
            1,
            |interp, this, args| {
                let fields = match get_duration_fields(interp, &this) {
                    Ok(f) => f,
                    Err(c) => return c,
                };
                let (y, mo, w, d, h, mi, s, ms, us, ns) = fields;
                let total_of = args.first().cloned().unwrap_or(JsValue::Undefined);
                if is_undefined(&total_of) {
                    return Completion::Throw(
                        interp.create_type_error("total requires options argument"),
                    );
                }

                let unit = if let JsValue::String(ref su) = total_of {
                    let su_str = su.to_rust_string();
                    match temporal_unit_singular(&su_str) {
                        Some(u) => u,
                        None => {
                            return Completion::Throw(
                                interp.create_range_error(&format!("Invalid unit: {su_str}")),
                            );
                        }
                    }
                } else if matches!(total_of, JsValue::Object(_)) {
                    let u = try_completion!(get_prop(interp, &total_of, "unit"));
                    if is_undefined(&u) {
                        return Completion::Throw(interp.create_range_error("unit is required"));
                    }
                    let us = try_result!(interp, interp.to_string_value(&u));
                    match temporal_unit_singular(&us) {
                        Some(u) => u,
                        None => {
                            return Completion::Throw(
                                interp.create_range_error(&format!("Invalid unit: {us}")),
                            );
                        }
                    }
                } else {
                    return Completion::Throw(
                        interp.create_type_error("total requires a string or options object"),
                    );
                };

                // Parse relativeTo from options
                let relative_to = if matches!(total_of, JsValue::Object(_)) {
                    let rt = match get_prop(interp, &total_of, "relativeTo") {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    match to_relative_to_date(interp, &rt) {
                        Ok(v) => v,
                        Err(c) => return c,
                    }
                } else {
                    None
                };

                // Calendar units require relativeTo
                let has_calendar = y != 0.0 || mo != 0.0 || w != 0.0;
                if has_calendar && relative_to.is_none() {
                    return Completion::Throw(
                        interp.create_range_error("relativeTo is required for calendar units"),
                    );
                }

                let total_ns: f64 = if let Some((by, bm, bd)) = relative_to {
                    match duration_total_ns_relative(
                        y, mo, w, d, h, mi, s, ms, us, ns, by, bm, bd,
                    ) {
                        Ok(v) => v as f64,
                        Err(()) => return Completion::Throw(interp.create_range_error(
                            "duration out of range when applied to relativeTo",
                        )),
                    }
                } else {
                    d * 86_400_000_000_000.0
                        + h * 3_600_000_000_000.0
                        + mi * 60_000_000_000.0
                        + s * 1_000_000_000.0
                        + ms * 1_000_000.0
                        + us * 1_000.0
                        + ns
                };

                let unit_ns = temporal_unit_length_ns(unit);
                Completion::Normal(JsValue::Number(total_ns / unit_ns))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("total".to_string(), total_fn);

        // toString(options?)
        let to_string_fn = self.create_function(JsFunction::native(
            "toString".to_string(),
            0,
            |interp, this, args| {
                let fields = match get_duration_fields(interp, &this) {
                    Ok(f) => f,
                    Err(c) => return c,
                };
                let (y, mo, w, d, h, mi, s, ms, us, ns) = fields;

                let (precision, rounding_mode) =
                    match parse_to_string_options(interp, args.first()) {
                        Ok(p) => p,
                        Err(c) => return c,
                    };

                let result = format_duration_iso(
                    y, mo, w, d, h, mi, s, ms, us, ns, precision, rounding_mode,
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
                let fields = match get_duration_fields(interp, &this) {
                    Ok(f) => f,
                    Err(c) => return c,
                };
                let (y, mo, w, d, h, mi, s, ms, us, ns) = fields;
                let result = format_duration_iso(y, mo, w, d, h, mi, s, ms, us, ns, None, "trunc");
                Completion::Normal(JsValue::String(JsString::from_str(&result)))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toJSON".to_string(), to_json_fn);

        // toLocaleString()
        let to_locale_string_fn = self.create_function(JsFunction::native(
            "toLocaleString".to_string(),
            0,
            |interp, this, _args| {
                let fields = match get_duration_fields(interp, &this) {
                    Ok(f) => f,
                    Err(c) => return c,
                };
                let (y, mo, w, d, h, mi, s, ms, us, ns) = fields;
                let result = format_duration_iso(y, mo, w, d, h, mi, s, ms, us, ns, None, "trunc");
                Completion::Normal(JsValue::String(JsString::from_str(&result)))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toLocaleString".to_string(), to_locale_string_fn);

        // valueOf() — throws TypeError
        let value_of_fn =
            self.create_function(JsFunction::native(
                "valueOf".to_string(),
                0,
                |interp, _this, _args| {
                    Completion::Throw(interp.create_type_error(
                        "use compare() or equals() to compare Temporal.Duration",
                    ))
                },
            ));
        proto
            .borrow_mut()
            .insert_builtin("valueOf".to_string(), value_of_fn);

        self.temporal_duration_prototype = Some(proto.clone());

        // Constructor
        let constructor = self.create_function(JsFunction::constructor(
            "Duration".to_string(),
            0,
            |interp, _this, args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Temporal.Duration must be called with new"),
                    );
                }

                let get_field = |interp: &mut Interpreter, idx: usize| -> Result<f64, Completion> {
                    let v = args.get(idx).cloned().unwrap_or(JsValue::Undefined);
                    if is_undefined(&v) {
                        return Ok(0.0);
                    }
                    let n = interp.to_number_value(&v).map_err(Completion::Throw)?;
                    to_integer_if_integral(n).ok_or_else(|| {
                        Completion::Throw(
                            interp.create_range_error("Duration field must be an integer"),
                        )
                    })
                };

                let years = match get_field(interp, 0) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let months = match get_field(interp, 1) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let weeks = match get_field(interp, 2) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let days = match get_field(interp, 3) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let hours = match get_field(interp, 4) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let minutes = match get_field(interp, 5) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let seconds = match get_field(interp, 6) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let milliseconds = match get_field(interp, 7) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let microseconds = match get_field(interp, 8) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let nanoseconds = match get_field(interp, 9) {
                    Ok(v) => v,
                    Err(c) => return c,
                };

                if !is_valid_duration(
                    years,
                    months,
                    weeks,
                    days,
                    hours,
                    minutes,
                    seconds,
                    milliseconds,
                    microseconds,
                    nanoseconds,
                ) {
                    return Completion::Throw(
                        interp.create_range_error("Invalid duration: mixed signs or non-finite"),
                    );
                }

                create_duration_result(
                    interp,
                    years,
                    months,
                    weeks,
                    days,
                    hours,
                    minutes,
                    seconds,
                    milliseconds,
                    microseconds,
                    nanoseconds,
                )
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

        // prototype.constructor
        proto.borrow_mut().insert_property(
            "constructor".to_string(),
            PropertyDescriptor::data(constructor.clone(), true, false, true),
        );

        // Duration.from(item)
        let from_fn = self.create_function(JsFunction::native(
            "from".to_string(),
            1,
            |interp, _this, args| {
                let item = args.first().cloned().unwrap_or(JsValue::Undefined);
                let record = match to_temporal_duration_record(interp, item) {
                    Ok(r) => r,
                    Err(c) => return c,
                };
                let (y, mo, w, d, h, mi, s, ms, us, ns) = record;
                create_duration_result(interp, y, mo, w, d, h, mi, s, ms, us, ns)
            },
        ));
        if let JsValue::Object(ref o) = constructor {
            if let Some(obj) = self.get_object(o.id) {
                obj.borrow_mut().insert_builtin("from".to_string(), from_fn);
            }
        }

        // Duration.compare(one, two)
        let compare_fn = self.create_function(JsFunction::native(
            "compare".to_string(),
            2,
            |interp, _this, args| {
                let one = match to_temporal_duration_record(
                    interp,
                    args.first().cloned().unwrap_or(JsValue::Undefined),
                ) {
                    Ok(r) => r,
                    Err(c) => return c,
                };
                let two = match to_temporal_duration_record(
                    interp,
                    args.get(1).cloned().unwrap_or(JsValue::Undefined),
                ) {
                    Ok(r) => r,
                    Err(c) => return c,
                };
                // Parse options (3rd argument)
                let options = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                let relative_to = if is_undefined(&options) {
                    None
                } else if matches!(options, JsValue::Object(_)) {
                    let rt = match get_prop(interp, &options, "relativeTo") {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    match to_relative_to_date(interp, &rt) {
                        Ok(v) => v,
                        Err(c) => return c,
                    }
                } else {
                    return Completion::Throw(
                        interp.create_type_error("options must be an object or undefined"),
                    );
                };

                // If both durations are identical, return 0 without needing relativeTo
                if one.0 == two.0 && one.1 == two.1 && one.2 == two.2 && one.3 == two.3
                    && one.4 == two.4 && one.5 == two.5 && one.6 == two.6 && one.7 == two.7
                    && one.8 == two.8 && one.9 == two.9
                {
                    return Completion::Normal(JsValue::Number(0.0));
                }

                let has_calendar_units = one.0 != 0.0
                    || one.1 != 0.0
                    || one.2 != 0.0
                    || two.0 != 0.0
                    || two.1 != 0.0
                    || two.2 != 0.0;

                if has_calendar_units && relative_to.is_none() {
                    return Completion::Throw(interp.create_range_error(
                        "relativeTo is required for comparing durations with calendar units",
                    ));
                }

                let (ns1, ns2) = if let Some((by, bm, bd)) = relative_to {
                    let n1 = match duration_total_ns_relative(
                        one.0, one.1, one.2, one.3, one.4, one.5, one.6, one.7, one.8, one.9,
                        by, bm, bd,
                    ) {
                        Ok(v) => v,
                        Err(()) => return Completion::Throw(interp.create_range_error(
                            "duration out of range when applied to relativeTo",
                        )),
                    };
                    let n2 = match duration_total_ns_relative(
                        two.0, two.1, two.2, two.3, two.4, two.5, two.6, two.7, two.8, two.9,
                        by, bm, bd,
                    ) {
                        Ok(v) => v,
                        Err(()) => return Completion::Throw(interp.create_range_error(
                            "duration out of range when applied to relativeTo",
                        )),
                    };
                    (n1, n2)
                } else {
                    let n1 = one.2 as i128 * 604_800_000_000_000
                        + one.3 as i128 * 86_400_000_000_000
                        + one.4 as i128 * 3_600_000_000_000
                        + one.5 as i128 * 60_000_000_000
                        + one.6 as i128 * 1_000_000_000
                        + one.7 as i128 * 1_000_000
                        + one.8 as i128 * 1_000
                        + one.9 as i128;
                    let n2 = two.2 as i128 * 604_800_000_000_000
                        + two.3 as i128 * 86_400_000_000_000
                        + two.4 as i128 * 3_600_000_000_000
                        + two.5 as i128 * 60_000_000_000
                        + two.6 as i128 * 1_000_000_000
                        + two.7 as i128 * 1_000_000
                        + two.8 as i128 * 1_000
                        + two.9 as i128;
                    (n1, n2)
                };
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
        if let JsValue::Object(ref o) = constructor {
            if let Some(obj) = self.get_object(o.id) {
                obj.borrow_mut()
                    .insert_builtin("compare".to_string(), compare_fn);
            }
        }

        // Register Duration on Temporal namespace
        temporal_obj.borrow_mut().insert_property(
            "Duration".to_string(),
            PropertyDescriptor::data(constructor, true, false, true),
        );
    }
}

fn get_duration_fields(
    interp: &mut Interpreter,
    this: &JsValue,
) -> Result<(f64, f64, f64, f64, f64, f64, f64, f64, f64, f64), Completion> {
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
                interp.create_type_error("not a Temporal.Duration"),
            ));
        }
    };
    let data = obj.borrow();
    match &data.temporal_data {
        Some(TemporalData::Duration {
            years,
            months,
            weeks,
            days,
            hours,
            minutes,
            seconds,
            milliseconds,
            microseconds,
            nanoseconds,
        }) => Ok((
            *years,
            *months,
            *weeks,
            *days,
            *hours,
            *minutes,
            *seconds,
            *milliseconds,
            *microseconds,
            *nanoseconds,
        )),
        _ => Err(Completion::Throw(
            interp.create_type_error("not a Temporal.Duration"),
        )),
    }
}

pub(crate) fn create_duration_result(
    interp: &mut Interpreter,
    years: f64,
    months: f64,
    weeks: f64,
    days: f64,
    hours: f64,
    minutes: f64,
    seconds: f64,
    milliseconds: f64,
    microseconds: f64,
    nanoseconds: f64,
) -> Completion {
    // Normalize -0.0 to 0.0 for all components (IEEE 754: -0.0 + 0.0 = 0.0)
    let years = years + 0.0;
    let months = months + 0.0;
    let weeks = weeks + 0.0;
    let days = days + 0.0;
    let hours = hours + 0.0;
    let minutes = minutes + 0.0;
    let seconds = seconds + 0.0;
    let milliseconds = milliseconds + 0.0;
    let microseconds = microseconds + 0.0;
    let nanoseconds = nanoseconds + 0.0;
    if !is_valid_duration(
        years,
        months,
        weeks,
        days,
        hours,
        minutes,
        seconds,
        milliseconds,
        microseconds,
        nanoseconds,
    ) {
        return Completion::Throw(
            interp.create_range_error("Invalid duration: mixed signs or non-finite"),
        );
    }
    let obj = interp.create_object();
    obj.borrow_mut().class_name = "Temporal.Duration".to_string();
    if let Some(ref proto) = interp.temporal_duration_prototype {
        obj.borrow_mut().prototype = Some(proto.clone());
    }
    obj.borrow_mut().temporal_data = Some(TemporalData::Duration {
        years,
        months,
        weeks,
        days,
        hours,
        minutes,
        seconds,
        milliseconds,
        microseconds,
        nanoseconds,
    });
    let id = obj.borrow().id.unwrap();
    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
}

pub(crate) fn to_temporal_duration_record(
    interp: &mut Interpreter,
    item: JsValue,
) -> Result<(f64, f64, f64, f64, f64, f64, f64, f64, f64, f64), Completion> {
    // String path first
    if let JsValue::String(s) = &item {
        let parsed = parse_temporal_duration_string(&s.to_rust_string()).ok_or_else(|| {
            Completion::Throw(
                interp.create_range_error(&format!("Invalid duration string: {s}")),
            )
        })?;
        let sign = parsed.sign;
        let (y, mo, w, d, h, mi, sec, ms, us, ns) = (
            parsed.years * sign,
            parsed.months * sign,
            parsed.weeks * sign,
            parsed.days * sign,
            parsed.hours * sign,
            parsed.minutes * sign,
            parsed.seconds * sign,
            parsed.milliseconds * sign,
            parsed.microseconds * sign,
            parsed.nanoseconds * sign,
        );
        if !is_valid_duration(y, mo, w, d, h, mi, sec, ms, us, ns) {
            return Err(Completion::Throw(
                interp.create_range_error("Invalid duration: out of range"),
            ));
        }
        return Ok((y, mo, w, d, h, mi, sec, ms, us, ns));
    }
    // Must be an object (reject null, booleans, numbers, etc.)
    if !matches!(&item, JsValue::Object(_)) {
        return Err(Completion::Throw(interp.create_type_error(
            "Invalid duration: expected string or object",
        )));
    }
    // Check for existing Duration instance
    if let JsValue::Object(o) = &item {
        if let Some(obj) = interp.get_object(o.id) {
            let data = obj.borrow();
            if let Some(TemporalData::Duration {
                years,
                months,
                weeks,
                days,
                hours,
                minutes,
                seconds,
                milliseconds,
                microseconds,
                nanoseconds,
            }) = &data.temporal_data
            {
                return Ok((
                    *years,
                    *months,
                    *weeks,
                    *days,
                    *hours,
                    *minutes,
                    *seconds,
                    *milliseconds,
                    *microseconds,
                    *nanoseconds,
                ));
            }
        }
    }
    // Property bag — read all plural fields
    let obj_val = item.clone();
    macro_rules! get_dur_field {
        ($name:expr) => {{
            let v = match get_prop(interp, &obj_val, $name) {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            if is_undefined(&v) {
                None
            } else {
                let n = interp.to_number_value(&v).map_err(Completion::Throw)?;
                Some(to_integer_if_integral(n).ok_or_else(|| {
                    Completion::Throw(
                        interp.create_range_error(&format!("{} must be an integer", $name)),
                    )
                })?)
            }
        }};
    }
    // Read properties in alphabetical order per spec ToTemporalDurationRecord
    let d = get_dur_field!("days");
    let h = get_dur_field!("hours");
    let us = get_dur_field!("microseconds");
    let ms = get_dur_field!("milliseconds");
    let mi = get_dur_field!("minutes");
    let mo = get_dur_field!("months");
    let ns = get_dur_field!("nanoseconds");
    let s = get_dur_field!("seconds");
    let w = get_dur_field!("weeks");
    let y = get_dur_field!("years");
    // If ALL recognized properties are undefined, throw TypeError
    if d.is_none()
        && h.is_none()
        && us.is_none()
        && ms.is_none()
        && mi.is_none()
        && mo.is_none()
        && ns.is_none()
        && s.is_none()
        && w.is_none()
        && y.is_none()
    {
        return Err(Completion::Throw(interp.create_type_error(
            "Invalid duration-like object: at least one duration property must be present",
        )));
    }
    let (y, mo, w, d, h, mi, s, ms, us, ns) = (
        y.unwrap_or(0.0),
        mo.unwrap_or(0.0),
        w.unwrap_or(0.0),
        d.unwrap_or(0.0),
        h.unwrap_or(0.0),
        mi.unwrap_or(0.0),
        s.unwrap_or(0.0),
        ms.unwrap_or(0.0),
        us.unwrap_or(0.0),
        ns.unwrap_or(0.0),
    );
    if !is_valid_duration(y, mo, w, d, h, mi, s, ms, us, ns) {
        return Err(Completion::Throw(
            interp.create_range_error("Invalid duration"),
        ));
    }
    Ok((y, mo, w, d, h, mi, s, ms, us, ns))
}

fn parse_round_options(
    interp: &mut Interpreter,
    round_to: &JsValue,
    y: f64,
    mo: f64,
    w: f64,
    d: f64,
    h: f64,
    mi: f64,
    s: f64,
    ms: f64,
    us: f64,
    ns: f64,
) -> Result<(&'static str, &'static str, f64, &'static str), Completion> {
    if let JsValue::String(su) = round_to {
        let su_str = su.to_rust_string();
        let unit = temporal_unit_singular(&su_str).ok_or_else(|| {
            Completion::Throw(interp.create_range_error(&format!("Invalid unit: {su_str}")))
        })?;
        let def = default_largest_unit_for_duration(y, mo, w, d, h, mi, s, ms, us, ns);
        let largest = if temporal_unit_order(def) < temporal_unit_order(unit) {
            unit
        } else {
            def
        };
        return Ok((unit, "halfExpand", 1.0, largest));
    }
    if !matches!(round_to, JsValue::Object(_)) {
        return Err(Completion::Throw(
            interp.create_type_error("round requires a string or options object"),
        ));
    }

    let small_unit_val = match get_prop(interp, round_to, "smallestUnit") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let large_unit_val = match get_prop(interp, round_to, "largestUnit") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };

    // Both smallestUnit and largestUnit undefined → error
    if is_undefined(&small_unit_val) && is_undefined(&large_unit_val) {
        return Err(Completion::Throw(
            interp.create_range_error("smallestUnit or largestUnit is required"),
        ));
    }

    let small_unit = if is_undefined(&small_unit_val) {
        "nanosecond"
    } else {
        let su = interp
            .to_string_value(&small_unit_val)
            .map_err(Completion::Throw)?;
        temporal_unit_singular(&su).ok_or_else(|| {
            Completion::Throw(interp.create_range_error(&format!("Invalid unit: {su}")))
        })?
    };

    let large_unit = if is_undefined(&large_unit_val) {
        let def = default_largest_unit_for_duration(y, mo, w, d, h, mi, s, ms, us, ns);
        if temporal_unit_order(def) < temporal_unit_order(small_unit) {
            small_unit
        } else {
            def
        }
    } else {
        let lu = interp
            .to_string_value(&large_unit_val)
            .map_err(Completion::Throw)?;
        temporal_unit_singular(&lu).ok_or_else(|| {
            Completion::Throw(interp.create_range_error(&format!("Invalid unit: {lu}")))
        })?
    };

    let rm_val = match get_prop(interp, round_to, "roundingMode") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let rounding_mode: &'static str = if is_undefined(&rm_val) {
        "halfExpand"
    } else {
        let rm = interp.to_string_value(&rm_val).map_err(Completion::Throw)?;
        match rm.as_str() {
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
                    interp.create_range_error(&format!("Invalid rounding mode: {rm}")),
                ));
            }
        }
    };

    let inc_val = match get_prop(interp, round_to, "roundingIncrement") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let increment = validate_rounding_increment(interp, &inc_val, &small_unit, true)?;

    Ok((small_unit, rounding_mode, increment, large_unit))
}

/// Returns (precision, rounding_mode).
/// precision: None = auto, Some(0..9) = fixed digits.
/// rounding_mode: "trunc" by default for toString.
fn parse_to_string_options(
    interp: &mut Interpreter,
    options: Option<&JsValue>,
) -> Result<(Option<u8>, &'static str), Completion> {
    let opt_val = options.cloned().unwrap_or(JsValue::Undefined);
    let has_opts = match super::get_options_object(interp, &opt_val) {
        Ok(v) => v,
        Err(c) => return Err(c),
    };
    if !has_opts {
        return Ok((None, "trunc"));
    }
    let options = opt_val;

    // Read roundingMode first
    let rm_val = match get_prop(interp, &options, "roundingMode") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let rounding_mode: &'static str = if is_undefined(&rm_val) {
        "trunc"
    } else {
        let rm = interp.to_string_value(&rm_val).map_err(Completion::Throw)?;
        match rm.as_str() {
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
                    interp.create_range_error(&format!("Invalid rounding mode: {rm}")),
                ));
            }
        }
    };

    // Per spec: smallestUnit overrides fractionalSecondDigits when present
    let sp = match get_prop(interp, &options, "smallestUnit") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    if !is_undefined(&sp) {
        let su = interp.to_string_value(&sp).map_err(Completion::Throw)?;
        return match su.as_str() {
            "second" | "seconds" => Ok((Some(0), rounding_mode)),
            "millisecond" | "milliseconds" => Ok((Some(3), rounding_mode)),
            "microsecond" | "microseconds" => Ok((Some(6), rounding_mode)),
            "nanosecond" | "nanoseconds" => Ok((Some(9), rounding_mode)),
            _ => Err(Completion::Throw(
                interp.create_range_error(&format!("Invalid smallestUnit: {su}")),
            )),
        };
    }

    let fp = match get_prop(interp, &options, "fractionalSecondDigits") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    if is_undefined(&fp) {
        return Ok((None, rounding_mode));
    }
    if matches!(fp, JsValue::Number(_)) {
        let n = interp.to_number_value(&fp).map_err(Completion::Throw)?;
        if n.is_nan() || !n.is_finite() {
            return Err(Completion::Throw(
                interp.create_range_error("fractionalSecondDigits must be 0-9 or 'auto'"),
            ));
        }
        let floored = n.floor();
        if floored < 0.0 || floored > 9.0 {
            return Err(Completion::Throw(
                interp.create_range_error("fractionalSecondDigits must be 0-9 or 'auto'"),
            ));
        }
        return Ok((Some(floored as u8), rounding_mode));
    }
    let s = interp.to_string_value(&fp).map_err(Completion::Throw)?;
    if s == "auto" {
        return Ok((None, rounding_mode));
    }
    Err(Completion::Throw(
        interp.create_range_error("fractionalSecondDigits must be 0-9 or 'auto'"),
    ))
}

fn format_duration_iso(
    years: f64,
    months: f64,
    weeks: f64,
    days: f64,
    hours: f64,
    minutes: f64,
    seconds: f64,
    milliseconds: f64,
    microseconds: f64,
    nanoseconds: f64,
    precision: Option<u8>,
    rounding_mode: &str,
) -> String {
    let sign = duration_sign(
        years,
        months,
        weeks,
        days,
        hours,
        minutes,
        seconds,
        milliseconds,
        microseconds,
        nanoseconds,
    );

    // Balance subsecond components into seconds for display
    let total_sub_ns = nanoseconds.abs()
        + microseconds.abs() * 1_000.0
        + milliseconds.abs() * 1_000_000.0
        + seconds.abs() * 1_000_000_000.0;

    // Apply rounding if precision is specified
    let total_sub_ns = if let Some(p) = precision {
        let increment = 10.0f64.powi(9 - p as i32);
        let effective_mode = if sign < 0 {
            match rounding_mode {
                "ceil" => "floor",
                "floor" => "ceil",
                "halfCeil" => "halfFloor",
                "halfFloor" => "halfCeil",
                other => other,
            }
        } else {
            rounding_mode
        };
        round_number_to_increment(total_sub_ns, increment, effective_mode)
    } else {
        total_sub_ns
    };

    let mut balanced_s = (total_sub_ns / 1_000_000_000.0).trunc();
    let frac_ns = (total_sub_ns - balanced_s * 1_000_000_000.0).round() as u64;

    // Handle carry across unit boundaries ONLY when rounding was applied
    let mut ami = minutes.abs();
    let mut ah = hours.abs();
    let mut extra_days = 0.0f64;
    if precision.is_some() {
        let has_higher_time = ami != 0.0 || ah != 0.0;
        if has_higher_time && balanced_s >= 60.0 {
            let carry_m = (balanced_s / 60.0).trunc();
            balanced_s -= carry_m * 60.0;
            ami += carry_m;
        }
        if ah != 0.0 && ami >= 60.0 {
            let carry_h = (ami / 60.0).trunc();
            ami -= carry_h * 60.0;
            ah += carry_h;
        }
        if ah >= 24.0 {
            let carry_d = (ah / 24.0).trunc();
            ah -= carry_d * 24.0;
            extra_days = carry_d;
        }
    }

    let mut result = String::new();
    if sign < 0 {
        result.push('-');
    }
    result.push('P');

    let (ay, amo, aw) = (years.abs(), months.abs(), weeks.abs());
    let ad = days.abs() + extra_days;

    if ay != 0.0 {
        result.push_str(&format_number(ay));
        result.push('Y');
    }
    if amo != 0.0 {
        result.push_str(&format_number(amo));
        result.push('M');
    }
    if aw != 0.0 {
        result.push_str(&format_number(aw));
        result.push('W');
    }
    if ad != 0.0 {
        result.push_str(&format_number(ad));
        result.push('D');
    }

    let has_time = ah != 0.0 || ami != 0.0 || balanced_s != 0.0 || frac_ns != 0 || precision.is_some();
    let has_date = ay != 0.0 || amo != 0.0 || aw != 0.0 || ad != 0.0;

    if has_time || !has_date {
        result.push('T');
        if ah != 0.0 {
            result.push_str(&format_number(ah));
            result.push('H');
        }
        if ami != 0.0 {
            result.push_str(&format_number(ami));
            result.push('M');
        }

        let need_seconds =
            balanced_s != 0.0 || frac_ns != 0 || (!has_time && !has_date) || precision.is_some();
        if need_seconds {
            let sec_part = format_number(balanced_s);
            if frac_ns != 0 {
                let frac = format!("{frac_ns:09}");
                match precision {
                    Some(0) => result.push_str(&sec_part),
                    Some(p) => {
                        result.push_str(&sec_part);
                        result.push('.');
                        result.push_str(&frac[..p as usize]);
                    }
                    None => {
                        let trimmed = frac.trim_end_matches('0');
                        result.push_str(&sec_part);
                        if !trimmed.is_empty() {
                            result.push('.');
                            result.push_str(trimmed);
                        }
                    }
                }
            } else {
                result.push_str(&sec_part);
                match precision {
                    Some(0) | None => {}
                    Some(p) => {
                        result.push('.');
                        for _ in 0..p {
                            result.push('0');
                        }
                    }
                }
            }
            result.push('S');
        }
    }

    if result == "P" || result == "-P" {
        return "PT0S".to_string();
    }
    result
}

fn format_number(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        crate::types::number_ops::to_string(v)
    }
}

pub(super) fn unbalance_time_ns(
    total_ns: f64,
    largest_unit: &str,
) -> (f64, f64, f64, f64, f64, f64, f64) {
    match largest_unit {
        "day" | "days" => {
            let d = (total_ns / 86_400_000_000_000.0).trunc();
            let rem = total_ns - d * 86_400_000_000_000.0;
            let h = (rem / 3_600_000_000_000.0).trunc();
            let rem = rem - h * 3_600_000_000_000.0;
            let mi = (rem / 60_000_000_000.0).trunc();
            let rem = rem - mi * 60_000_000_000.0;
            let s = (rem / 1_000_000_000.0).trunc();
            let rem = rem - s * 1_000_000_000.0;
            let ms = (rem / 1_000_000.0).trunc();
            let rem = rem - ms * 1_000_000.0;
            let us = (rem / 1_000.0).trunc();
            let ns = (rem - us * 1_000.0).round();
            (d, h, mi, s, ms, us, ns)
        }
        "hour" | "hours" => {
            let h = (total_ns / 3_600_000_000_000.0).trunc();
            let rem = total_ns - h * 3_600_000_000_000.0;
            let mi = (rem / 60_000_000_000.0).trunc();
            let rem = rem - mi * 60_000_000_000.0;
            let s = (rem / 1_000_000_000.0).trunc();
            let rem = rem - s * 1_000_000_000.0;
            let ms = (rem / 1_000_000.0).trunc();
            let rem = rem - ms * 1_000_000.0;
            let us = (rem / 1_000.0).trunc();
            let ns = (rem - us * 1_000.0).round();
            (0.0, h, mi, s, ms, us, ns)
        }
        "minute" | "minutes" => {
            let mi = (total_ns / 60_000_000_000.0).trunc();
            let rem = total_ns - mi * 60_000_000_000.0;
            let s = (rem / 1_000_000_000.0).trunc();
            let rem = rem - s * 1_000_000_000.0;
            let ms = (rem / 1_000_000.0).trunc();
            let rem = rem - ms * 1_000_000.0;
            let us = (rem / 1_000.0).trunc();
            let ns = (rem - us * 1_000.0).round();
            (0.0, 0.0, mi, s, ms, us, ns)
        }
        "second" | "seconds" => {
            let s = (total_ns / 1_000_000_000.0).trunc();
            let rem = total_ns - s * 1_000_000_000.0;
            let ms = (rem / 1_000_000.0).trunc();
            let rem = rem - ms * 1_000_000.0;
            let us = (rem / 1_000.0).trunc();
            let ns = (rem - us * 1_000.0).round();
            (0.0, 0.0, 0.0, s, ms, us, ns)
        }
        "millisecond" | "milliseconds" => {
            let ms = (total_ns / 1_000_000.0).trunc();
            let rem = total_ns - ms * 1_000_000.0;
            let us = (rem / 1_000.0).trunc();
            let ns = (rem - us * 1_000.0).round();
            (0.0, 0.0, 0.0, 0.0, ms, us, ns)
        }
        "microsecond" | "microseconds" => {
            let us = (total_ns / 1_000.0).trunc();
            let ns = (total_ns - us * 1_000.0).round();
            (0.0, 0.0, 0.0, 0.0, 0.0, us, ns)
        }
        _ => (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, total_ns.round()),
    }
}

// Balance time portion of a duration after add/subtract.
// Per spec: AddDurations converts day+time to total nanoseconds,
// then re-balances up to the largest unit present in either operand.
pub(crate) fn balance_duration_relative(
    years: f64,
    months: f64,
    weeks: f64,
    days: f64,
    hours: f64,
    minutes: f64,
    seconds: f64,
    milliseconds: f64,
    microseconds: f64,
    nanoseconds: f64,
) -> (f64, f64, f64, f64, f64, f64, f64, f64, f64, f64) {
    // Determine largest unit present
    let largest = default_largest_unit_for_duration(
        years,
        months,
        weeks,
        days,
        hours,
        minutes,
        seconds,
        milliseconds,
        microseconds,
        nanoseconds,
    );

    // For calendar units without relativeTo, just return as-is (validation elsewhere)
    if matches!(largest, "year" | "month" | "week") {
        return (
            years,
            months,
            weeks,
            days,
            hours,
            minutes,
            seconds,
            milliseconds,
            microseconds,
            nanoseconds,
        );
    }

    // Convert day+time to total nanoseconds and re-balance
    let total_ns = nanoseconds
        + microseconds * 1_000.0
        + milliseconds * 1_000_000.0
        + seconds * 1_000_000_000.0
        + minutes * 60_000_000_000.0
        + hours * 3_600_000_000_000.0
        + days * 86_400_000_000_000.0;

    let sign = if total_ns < 0.0 {
        -1.0
    } else if total_ns > 0.0 {
        1.0
    } else {
        0.0
    };
    let abs_ns = total_ns.abs();
    let (rd, rh, rmi, rs, rms, rus, rns) = unbalance_time_ns(abs_ns, largest);

    (
        years,
        months,
        weeks,
        rd * sign,
        rh * sign,
        rmi * sign,
        rs * sign,
        rms * sign,
        rus * sign,
        rns * sign,
    )
}
