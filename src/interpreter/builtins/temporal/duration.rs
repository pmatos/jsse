use super::*;
use crate::interpreter::builtins::temporal::{
    add_iso_date, coerce_rounding_increment, default_largest_unit_for_duration, duration_sign,
    get_prop, is_undefined, is_valid_duration, iso_date_to_epoch_days, max_rounding_increment,
    parse_temporal_duration_string, temporal_unit_length_ns, temporal_unit_order,
    temporal_unit_singular, to_integer_if_integral,
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

/// Correctly-rounded i128 / i128 → f64 division.
/// Uses string-based conversion for precise results: constructs a decimal
/// string with enough digits and lets f64 parsing handle the rounding.
fn divide_i128_to_f64(numerator: i128, divisor: i128) -> f64 {
    debug_assert!(divisor != 0);
    if numerator == 0 {
        return 0.0;
    }
    let sign = if (numerator < 0) != (divisor < 0) {
        -1.0f64
    } else {
        1.0f64
    };
    let abs_num = numerator.unsigned_abs();
    let abs_div = divisor.unsigned_abs();
    let whole = abs_num / abs_div;
    let remainder = abs_num % abs_div;
    if remainder == 0 {
        return sign * whole as f64;
    }
    // Compute 25 decimal digits of the fractional part
    // f64 has ~15.9 significant digits, but we need extra for leading zeros + rounding
    let mut frac_digits = String::with_capacity(25);
    let mut rem = remainder;
    for _ in 0..25 {
        rem *= 10;
        let digit = rem / abs_div;
        frac_digits.push((b'0' + digit as u8) as char);
        rem %= abs_div;
    }
    let s = format!("{whole}.{frac_digits}");
    sign * s.parse::<f64>().unwrap()
}

/// Extract a PlainDate (year, month, day) from a relativeTo value.
/// Accepts PlainDate, PlainDateTime, ZonedDateTime objects, property bags, or strings.
/// For ZonedDateTime-like inputs, extracts just the date portion.
/// Returns (year, month, day, zdt_info) where zdt_info is Some((epoch_ns, timezone)) for ZDT.
fn to_relative_to_date(
    interp: &mut Interpreter,
    val: &JsValue,
) -> Result<Option<(i32, u8, u8, Option<(i128, String)>)>, Completion> {
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
                        if !parsed.has_utc_designator {
                            let tz_end = after.find(']').unwrap_or(after.len());
                            let tz_name = &after[..tz_end];
                            if let Some(canonical_tz) = super::parse_utc_offset_timezone(tz_name) {
                                let offset_sign = if offset.sign < 0 { '-' } else { '+' };
                                let iso_truncated = format!(
                                    "{}{:02}:{:02}",
                                    offset_sign, offset.hours, offset.minutes
                                );
                                if iso_truncated != canonical_tz {
                                    return Err(Completion::Throw(interp.create_range_error(
                                        "UTC offset mismatch in ZonedDateTime string",
                                    )));
                                }
                            } else if tz_name == "UTC"
                                || tz_name == "Etc/UTC"
                                || tz_name == "Etc/GMT"
                            {
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
                    }
                    let tz_end = after.find(']').unwrap_or(after.len());
                    let tz_name_raw = &after[..tz_end];
                    let tz_name_str = super::canonicalize_iana_tz(tz_name_raw);

                    // Compute local_ns from wall-clock
                    let wall_epoch_days =
                        iso_date_to_epoch_days(parsed.year, parsed.month, parsed.day);
                    if wall_epoch_days.abs() > 100_000_000 {
                        return Err(Completion::Throw(interp.create_range_error(
                            "ZonedDateTime relativeTo is outside the representable range",
                        )));
                    }
                    let epoch_days_i = wall_epoch_days as i128;
                    let day_ns = parsed.hour as i128 * 3_600_000_000_000
                        + parsed.minute as i128 * 60_000_000_000
                        + parsed.second as i128 * 1_000_000_000
                        + parsed.millisecond as i128 * 1_000_000
                        + parsed.microsecond as i128 * 1_000
                        + parsed.nanosecond as i128;
                    let local_ns = epoch_days_i * 86_400_000_000_000 + day_ns;

                    let computed_epoch_ns = if parsed.has_utc_designator {
                        // Z → UTC
                        local_ns
                    } else if let Some(ref offset) = parsed.offset {
                        let off_ns = offset.sign as i128
                            * (offset.hours as i128 * 3_600_000_000_000
                                + offset.minutes as i128 * 60_000_000_000
                                + offset.seconds as i128 * 1_000_000_000
                                + offset.nanoseconds as i128);
                        let has_seconds_in_offset = offset.has_sub_minute;

                        // Validate offset against timezone
                        if super::parse_utc_offset_timezone(tz_name_raw).is_none()
                            && tz_name_raw != "UTC"
                            && tz_name_raw != "Etc/UTC"
                            && tz_name_raw != "Etc/GMT"
                        {
                            // IANA timezone: validate offset matches
                            let candidates = super::zoned_date_time::get_possible_epoch_ns(
                                &tz_name_str, local_ns,
                            );
                            let mut matched: Option<i128> = None;
                            for &cand in &candidates {
                                let actual_off = local_ns - cand;
                                if has_seconds_in_offset {
                                    // Exact match to the second
                                    let actual_secs = actual_off / 1_000_000_000;
                                    let string_secs = off_ns / 1_000_000_000;
                                    if actual_secs == string_secs {
                                        matched = Some(cand);
                                        break;
                                    }
                                } else {
                                    // HH:MM only: round actual offset to nearest minute
                                    let actual_secs = actual_off / 1_000_000_000;
                                    let rounded_mins = if actual_secs >= 0 {
                                        (actual_secs + 30) / 60
                                    } else {
                                        -((-actual_secs + 30) / 60)
                                    };
                                    let string_mins = off_ns / 60_000_000_000;
                                    if rounded_mins == string_mins {
                                        matched = Some(cand);
                                        break;
                                    }
                                }
                            }
                            match matched {
                                Some(ns) => ns,
                                None => {
                                    return Err(Completion::Throw(interp.create_range_error(
                                        "UTC offset mismatch in ZonedDateTime string",
                                    )));
                                }
                            }
                        } else {
                            local_ns - off_ns
                        }
                    } else {
                        // No offset → use "compatible" disambiguation
                        super::zoned_date_time::disambiguate_instant(
                            &tz_name_str, local_ns, "compatible",
                        )
                    };
                    let epoch_ns_bi = num_bigint::BigInt::from(computed_epoch_ns);
                    if !super::instant::is_valid_epoch_ns(&epoch_ns_bi) {
                        return Err(Completion::Throw(interp.create_range_error(
                            "ZonedDateTime relativeTo is outside the representable range",
                        )));
                    }
                    // Derive actual calendar date in the timezone from epoch_ns
                    let (zy, zm, zd, _, _, _, _, _, _) =
                        super::zoned_date_time::epoch_ns_to_components(
                            &epoch_ns_bi, &tz_name_str,
                        );
                    return Ok(Some((
                        zy, zm, zd,
                        Some((computed_epoch_ns, tz_name_str)),
                    )));
                }
                return Err(Completion::Throw(
                    interp.create_range_error(&format!("Invalid relativeTo string: {raw}")),
                ));
            }
        }
    }
    // If the value is a Temporal object, handle directly (no property bag reading)
    if let JsValue::Object(obj_ref) = val {
        if let Some(obj) = interp.get_object(obj_ref.id) {
            let td = obj.borrow().temporal_data.clone();
            if let Some(super::TemporalData::ZonedDateTime {
                epoch_nanoseconds,
                time_zone,
                ..
            }) = &td
            {
                let ns: i128 = epoch_nanoseconds.try_into().unwrap_or(0);
                let (y, m, d, _, _, _, _, _, _) =
                    super::zoned_date_time::epoch_ns_to_components(epoch_nanoseconds, time_zone);
                return Ok(Some((y, m, d, Some((ns, time_zone.clone())))));
            }
            if let Some(super::TemporalData::PlainDate {
                iso_year,
                iso_month,
                iso_day,
                ..
            }) = &td
            {
                return Ok(Some((*iso_year, *iso_month, *iso_day, None)));
            }
            if let Some(super::TemporalData::PlainDateTime {
                iso_year,
                iso_month,
                iso_day,
                ..
            }) = &td
            {
                return Ok(Some((*iso_year, *iso_month, *iso_day, None)));
            }
        }
    }

    // Property bag: read ALL fields in alphabetical order per spec PrepareTemporalFields.
    if let JsValue::Object(_) = val {
        // 1. calendar
        let cal_val = match get_prop(interp, val, "calendar") {
            Completion::Normal(v) => v,
            other => return Err(other),
        };
        let cal = super::to_temporal_calendar_slot_value(interp, &cal_val)?;
        let cal_has_era = matches!(cal.as_str(), "gregory" | "japanese" | "roc" | "coptic" | "ethiopic" | "ethioaa");

        // 2. day (required, coerce if defined)
        let d_val = match get_prop(interp, val, "day") {
            Completion::Normal(v) => v,
            other => return Err(other),
        };
        let has_day = !is_undefined(&d_val);
        let day_f = if has_day {
            super::to_integer_with_truncation(interp, &d_val)?
        } else {
            0.0
        };

        // 2b. era (coerce if defined, only for calendars with eras)
        let mut era_str: Option<String> = None;
        let mut era_year: Option<f64> = None;
        if cal_has_era {
            let era_val = match get_prop(interp, val, "era") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            if !is_undefined(&era_val) {
                era_str = Some(super::to_primitive_and_require_string(interp, &era_val, "era")?);
            }

            // 2c. eraYear (coerce if defined)
            let era_year_val = match get_prop(interp, val, "eraYear") {
                Completion::Normal(v) => v,
                other => return Err(other),
            };
            if !is_undefined(&era_year_val) {
                era_year = Some(super::to_integer_with_truncation(interp, &era_year_val)?);
            }
        }

        // 3. hour (coerce if defined)
        let hour_val = match get_prop(interp, val, "hour") {
            Completion::Normal(v) => v,
            other => return Err(other),
        };
        if !is_undefined(&hour_val) {
            super::to_integer_with_truncation(interp, &hour_val)?;
        }

        // 4. microsecond (coerce if defined)
        let us_val = match get_prop(interp, val, "microsecond") {
            Completion::Normal(v) => v,
            other => return Err(other),
        };
        if !is_undefined(&us_val) {
            super::to_integer_with_truncation(interp, &us_val)?;
        }

        // 5. millisecond (coerce if defined)
        let ms_val = match get_prop(interp, val, "millisecond") {
            Completion::Normal(v) => v,
            other => return Err(other),
        };
        if !is_undefined(&ms_val) {
            super::to_integer_with_truncation(interp, &ms_val)?;
        }

        // 6. minute (coerce if defined)
        let min_val = match get_prop(interp, val, "minute") {
            Completion::Normal(v) => v,
            other => return Err(other),
        };
        if !is_undefined(&min_val) {
            super::to_integer_with_truncation(interp, &min_val)?;
        }

        // 7. month (coerce if defined)
        let m_val = match get_prop(interp, val, "month") {
            Completion::Normal(v) => v,
            other => return Err(other),
        };
        let has_month = !is_undefined(&m_val);
        let month_coerced: Option<i32> = if has_month {
            Some(super::to_integer_with_truncation(interp, &m_val)? as i32)
        } else {
            None
        };

        // 8. monthCode (coerce + syntax validate immediately)
        let mc_val = match get_prop(interp, val, "monthCode") {
            Completion::Normal(v) => v,
            other => return Err(other),
        };
        let has_month_code = !is_undefined(&mc_val);
        let month_code_str: Option<String> = if has_month_code {
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

        // 9. nanosecond (coerce if defined)
        let ns_val = match get_prop(interp, val, "nanosecond") {
            Completion::Normal(v) => v,
            other => return Err(other),
        };
        if !is_undefined(&ns_val) {
            super::to_integer_with_truncation(interp, &ns_val)?;
        }

        // 10. offset (ToPrimitiveAndRequireString + validate syntax immediately)
        let offset_val = match get_prop(interp, val, "offset") {
            Completion::Normal(v) => v,
            other => return Err(other),
        };
        let _offset_str: Option<String> = if is_undefined(&offset_val) {
            None
        } else {
            let os = super::to_primitive_and_require_string(interp, &offset_val, "offset")?;
            if super::parse_offset_string(&os).is_none() {
                return Err(Completion::Throw(
                    interp.create_range_error(&format!("{os} is not a valid offset string")),
                ));
            }
            Some(os)
        };

        // 11. second (coerce if defined)
        let sec_val = match get_prop(interp, val, "second") {
            Completion::Normal(v) => v,
            other => return Err(other),
        };
        if !is_undefined(&sec_val) {
            super::to_integer_with_truncation(interp, &sec_val)?;
        }

        // 12. timeZone
        let tz_val = match get_prop(interp, val, "timeZone") {
            Completion::Normal(v) => v,
            other => return Err(other),
        };

        // 13. year (required, coerce — or computed from era+eraYear)
        let y_val = match get_prop(interp, val, "year") {
            Completion::Normal(v) => v,
            other => return Err(other),
        };
        let year: i32 = if !is_undefined(&y_val) {
            super::to_integer_with_truncation(interp, &y_val)? as i32
        } else if let (Some(_era), Some(ey)) = (&era_str, era_year) {
            ey as i32
        } else {
            return Err(Completion::Throw(
                interp.create_type_error("year is required"),
            ));
        };

        // --- After all reads, validate ---
        if !has_day {
            return Err(Completion::Throw(
                interp.create_type_error("day is required"),
            ));
        }
        let day = day_f as u8;
        if day_f < 1.0 {
            return Err(Completion::Throw(
                interp.create_range_error("day must be a positive integer"),
            ));
        }

        // Resolve month/monthCode (syntax already validated at step 8)
        let month = if let Some(ref mc) = month_code_str {
            match super::plain_date::month_code_to_number_pub(mc) {
                Some(n) => {
                    if let Some(explicit_m) = month_coerced {
                        if explicit_m != n as i32 {
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
        } else if let Some(m) = month_coerced {
            if m < 1 {
                return Err(Completion::Throw(
                    interp.create_range_error("month must be a positive integer"),
                ));
            }
            m as u8
        } else {
            return Err(Completion::Throw(
                interp.create_type_error("month or monthCode is required"),
            ));
        };

        // Determine PlainDate vs ZDT context
        if !is_undefined(&tz_val) {
            // ZDT context
            let tz = super::to_temporal_time_zone_identifier(interp, &tz_val)?;

            // For property bags, validate offset exactly against timezone (reject semantics)
            if let Some(ref os) = _offset_str {
                let epoch_days = iso_date_to_epoch_days(year, month, day) as i128;
                let local_ns = epoch_days * 86_400_000_000_000;
                let candidates = super::zoned_date_time::get_possible_epoch_ns(&tz, local_ns);
                if let Some(parsed_off) = super::parse_offset_string(os) {
                    let off_ns_i128 = parsed_off.sign as i128
                        * (parsed_off.hours as i128 * 3_600_000_000_000
                            + parsed_off.minutes as i128 * 60_000_000_000
                            + parsed_off.seconds as i128 * 1_000_000_000
                            + parsed_off.nanoseconds as i128);
                    let mut found = false;
                    for &cand in &candidates {
                        let actual_off = local_ns - cand;
                        if actual_off == off_ns_i128 {
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return Err(Completion::Throw(interp.create_range_error(
                            "UTC offset mismatch in ZonedDateTime relativeTo",
                        )));
                    }
                }
            }

            // Use "compatible" disambiguation
            let epoch_days = iso_date_to_epoch_days(year, month, day) as i128;
            let local_ns = epoch_days * 86_400_000_000_000;
            let epoch_ns = super::zoned_date_time::disambiguate_instant(&tz, local_ns, "compatible");

            return Ok(Some((year, month, day, Some((epoch_ns, tz)))));
        } else {
            // PlainDate context
            return Ok(Some((year, month, day, None)));
        }
    }

    // Non-object, non-string, non-undefined — fall through to to_temporal_plain_date
    let (y, m, d, _) = super::plain_date::to_temporal_plain_date(interp, val.clone())?;
    Ok(Some((y, m, d, None)))
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
        base_year, base_month, base_day, y as i32, mo as i32, w as i32, d as i32,
    );
    if !super::iso_date_within_limits(ry, rm, rd) {
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

/// TotalRelativeDuration per spec (simplified for ISO 8601 calendar).
/// Adds date fields (Y/M/W) to relativeTo to get targetDate, then computes
/// the fractional total in the given unit using calendar-aware boundaries.
fn total_relative_duration(
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
    unit: &str,
    base_year: i32,
    base_month: u8,
    base_day: u8,
) -> Result<f64, ()> {
    // Step 1: Add date fields (years, months, weeks) to get targetDate — NOT days
    let target = add_iso_date(
        base_year, base_month, base_day, y as i32, mo as i32, w as i32, 0,
    );
    if !super::iso_date_within_limits(target.0, target.1, target.2) {
        return Err(());
    }

    // Step 2: NormalizedTimeDuration = days * dayNs + time components
    let time_ns: i128 = h as i128 * 3_600_000_000_000
        + mi as i128 * 60_000_000_000
        + s as i128 * 1_000_000_000
        + ms as i128 * 1_000_000
        + us as i128 * 1_000
        + ns as i128;
    // Handle fractional days (can occur when d comes from NanosecondsToDays with ZDT)
    let d_whole = d.trunc() as i128;
    let d_frac_ns = ((d - d.trunc()) * 86_400_000_000_000.0) as i128;
    let norm_ns: i128 = d_whole * 86_400_000_000_000 + d_frac_ns + time_ns;

    let whole_days = if norm_ns >= 0 {
        norm_ns / 86_400_000_000_000
    } else {
        -(-norm_ns / 86_400_000_000_000)
            - if (-norm_ns) % 86_400_000_000_000 != 0 {
                1
            } else {
                0
            }
    };
    let frac_day_ns = norm_ns - whole_days * 86_400_000_000_000;
    let frac_day = frac_day_ns as f64 / 86_400_000_000_000.0;

    // Step 3: endDate = targetDate + wholeDays
    let end = add_iso_date(target.0, target.1, target.2, 0, 0, 0, whole_days as i32);
    if !super::iso_date_within_limits(end.0, end.1, end.2) {
        return Err(());
    }

    let base_epoch = iso_date_to_epoch_days(base_year, base_month, base_day);
    let end_epoch = iso_date_to_epoch_days(end.0, end.1, end.2);

    match unit {
        "year" => {
            let (diff_y, _, _, _) = super::difference_iso_date(
                base_year, base_month, base_day, end.0, end.1, end.2, "year",
            );
            let year_start = add_iso_date(base_year, base_month, base_day, diff_y, 0, 0, 0);
            let mut sign = if diff_y > 0 || (diff_y == 0 && end_epoch > base_epoch) {
                1
            } else if diff_y < 0 || (diff_y == 0 && end_epoch < base_epoch) {
                -1
            } else {
                0
            };
            if sign == 0 {
                if frac_day_ns > 0 {
                    sign = 1;
                } else if frac_day_ns < 0 {
                    sign = -1;
                } else {
                    return Ok(0.0);
                }
            }
            let year_end = add_iso_date(base_year, base_month, base_day, diff_y + sign, 0, 0, 0);
            if !super::iso_date_within_limits(year_end.0, year_end.1, year_end.2) {
                return Err(());
            }
            let year_start_epoch = iso_date_to_epoch_days(year_start.0, year_start.1, year_start.2);
            let year_end_epoch = iso_date_to_epoch_days(year_end.0, year_end.1, year_end.2);
            let year_length = (year_end_epoch - year_start_epoch).abs();
            if year_length == 0 {
                return Ok(diff_y as f64);
            }
            let days_into_year = end_epoch - year_start_epoch;
            let numerator_ns: i128 = diff_y as i128 * year_length as i128 * 86_400_000_000_000
                + days_into_year as i128 * 86_400_000_000_000
                + frac_day_ns;
            let denominator_ns: i128 = year_length as i128 * 86_400_000_000_000;
            Ok(divide_i128_to_f64(numerator_ns, denominator_ns))
        }
        "month" => {
            let (_, diff_m, _, _) = super::difference_iso_date(
                base_year, base_month, base_day, end.0, end.1, end.2, "month",
            );
            let month_start = add_iso_date(base_year, base_month, base_day, 0, diff_m, 0, 0);
            let mut sign = if diff_m > 0 || (diff_m == 0 && end_epoch > base_epoch) {
                1
            } else if diff_m < 0 || (diff_m == 0 && end_epoch < base_epoch) {
                -1
            } else {
                0
            };
            if sign == 0 {
                if frac_day_ns > 0 {
                    sign = 1;
                } else if frac_day_ns < 0 {
                    sign = -1;
                } else {
                    return Ok(0.0);
                }
            }
            let month_end = add_iso_date(base_year, base_month, base_day, 0, diff_m + sign, 0, 0);
            if !super::iso_date_within_limits(month_end.0, month_end.1, month_end.2) {
                return Err(());
            }
            let month_start_epoch =
                iso_date_to_epoch_days(month_start.0, month_start.1, month_start.2);
            let month_end_epoch = iso_date_to_epoch_days(month_end.0, month_end.1, month_end.2);
            let month_length = (month_end_epoch - month_start_epoch).abs();
            if month_length == 0 {
                return Ok(diff_m as f64);
            }
            let days_into_month = end_epoch - month_start_epoch;
            let numerator_ns: i128 = diff_m as i128 * month_length as i128 * 86_400_000_000_000
                + days_into_month as i128 * 86_400_000_000_000
                + frac_day_ns;
            let denominator_ns: i128 = month_length as i128 * 86_400_000_000_000;
            Ok(divide_i128_to_f64(numerator_ns, denominator_ns))
        }
        "week" => {
            // Decompose to preserve f64 precision: integer weeks + fractional remainder
            let total_days_int = end_epoch - base_epoch;
            let whole_weeks = total_days_int / 7;
            let remaining_days = total_days_int % 7;
            Ok(whole_weeks as f64 + (remaining_days as f64 + frac_day) / 7.0)
        }
        "day" => {
            let total_days = (end_epoch - base_epoch) as f64 + frac_day;
            Ok(total_days)
        }
        _ => {
            // Time units: flatten everything to total nanoseconds
            let target_epoch = iso_date_to_epoch_days(target.0, target.1, target.2);
            let calendar_days = (target_epoch - base_epoch) as i128;
            let total_ns = (calendar_days + d as i128) * 86_400_000_000_000 + time_ns;
            let unit_ns = temporal_unit_length_ns(unit) as i128;
            Ok(divide_i128_to_f64(total_ns, unit_ns))
        }
    }
}

/// Compute the endpoint epoch_ns after applying a duration to a ZDT.
/// This is the ZDT-aware AddZonedDateTime: adds Y/M/W to date with same wall time,
/// then adds D days with same wall time, then adds time components.
fn add_duration_to_zdt_epoch_ns(
    y: f64, mo: f64, w: f64, d: f64,
    h: f64, mi: f64, s: f64, ms: f64, us: f64, ns: f64,
    base_year: i32, base_month: u8, base_day: u8,
    base_epoch_ns: i128, tz: &str,
) -> Result<i128, ()> {
    let time_ns = h as i128 * 3_600_000_000_000
        + mi as i128 * 60_000_000_000
        + s as i128 * 1_000_000_000
        + ms as i128 * 1_000_000
        + us as i128 * 1_000
        + ns as i128;

    let has_date = y != 0.0 || mo != 0.0 || w != 0.0 || d != 0.0;
    if !has_date {
        return Ok(base_epoch_ns + time_ns);
    }

    let bi = num_bigint::BigInt::from(base_epoch_ns);
    let (_, _, _, bh, bmi, bs, bms, bus, bns) =
        super::zoned_date_time::epoch_ns_to_components(&bi, tz);
    let wall_time_ns = bh as i128 * 3_600_000_000_000
        + bmi as i128 * 60_000_000_000
        + bs as i128 * 1_000_000_000
        + bms as i128 * 1_000_000
        + bus as i128 * 1_000
        + bns as i128;

    // Add Y/M/W
    let inter = add_iso_date(base_year, base_month, base_day, y as i32, mo as i32, w as i32, 0);
    if !super::iso_date_within_limits(inter.0, inter.1, inter.2) {
        return Err(());
    }

    // Add D days
    let day_adv = add_iso_date(inter.0, inter.1, inter.2, 0, 0, 0, d as i32);
    if !super::iso_date_within_limits(day_adv.0, day_adv.1, day_adv.2) {
        return Err(());
    }
    let day_adv_epoch_days = iso_date_to_epoch_days(day_adv.0, day_adv.1, day_adv.2) as i128;
    let day_adv_local_ns = day_adv_epoch_days * 86_400_000_000_000 + wall_time_ns;
    let day_adv_epoch_ns = super::zoned_date_time::disambiguate_instant(
        tz, day_adv_local_ns, "compatible",
    );

    Ok(day_adv_epoch_ns + time_ns)
}

/// NanosecondsToDays: count actual timezone days between two instants.
/// Uses AddZonedDateTime-style day stepping (same wall-clock time, disambiguated).
fn nanoseconds_to_tz_days(
    dest_epoch_ns: i128,
    inter_epoch_ns: i128,
    inter_year: i32, inter_month: u8, inter_day: u8,
    wall_time_ns: i128,
    tz: &str,
) -> f64 {
    let total_ns = dest_epoch_ns - inter_epoch_ns;
    if total_ns == 0 {
        return 0.0;
    }
    let sign: i128 = if total_ns > 0 { 1 } else { -1 };
    let mut day_count: i64 = 0;
    let mut current_ns = inter_epoch_ns;
    let mut day_length_ns: i128 = 86_400_000_000_000;
    let mut current_epoch_days = iso_date_to_epoch_days(inter_year, inter_month, inter_day) as i128;

    loop {
        let next_epoch_days = current_epoch_days + sign;
        let next_local_ns = next_epoch_days * 86_400_000_000_000 + wall_time_ns;
        let next_ns = super::zoned_date_time::disambiguate_instant(
            tz, next_local_ns, "compatible",
        );
        day_length_ns = (next_ns - current_ns).abs();
        if day_length_ns == 0 { day_length_ns = 86_400_000_000_000; }
        let remaining = (dest_epoch_ns - next_ns) * sign;
        if remaining < 0 {
            break; // overshot
        }
        day_count += sign as i64;
        current_ns = next_ns;
        current_epoch_days = next_epoch_days;
        if remaining == 0 {
            return day_count as f64;
        }
        if day_count.abs() > 200_000_000 { break; }
    }

    let remaining_ns = dest_epoch_ns - current_ns;
    let fractional = remaining_ns as f64 / day_length_ns as f64;
    day_count as f64 + fractional
}

/// ZDT-aware TotalRelativeDuration: accounts for actual timezone day lengths.
fn total_relative_duration_zdt(
    y: f64, mo: f64, w: f64, d: f64,
    h: f64, mi: f64, s: f64, ms: f64, us: f64, ns: f64,
    unit: &str,
    base_year: i32, base_month: u8, base_day: u8,
    base_epoch_ns: i128, tz: &str,
) -> Result<f64, ()> {
    // Get wall-clock time at base
    let bi = num_bigint::BigInt::from(base_epoch_ns);
    let (_, _, _, bh, bmi, bs, bms, bus, bns) =
        super::zoned_date_time::epoch_ns_to_components(&bi, tz);

    // Add Y/M/W to base date
    let intermediate_iso = add_iso_date(
        base_year, base_month, base_day, y as i32, mo as i32, w as i32, 0,
    );
    if !super::iso_date_within_limits(intermediate_iso.0, intermediate_iso.1, intermediate_iso.2) {
        return Err(());
    }

    let wall_time_ns = bh as i128 * 3_600_000_000_000
        + bmi as i128 * 60_000_000_000
        + bs as i128 * 1_000_000_000
        + bms as i128 * 1_000_000
        + bus as i128 * 1_000
        + bns as i128;

    // Compute intermediate epoch_ns: when Y/M/W=0, preserve original epoch_ns
    let has_date_add = y != 0.0 || mo != 0.0 || w != 0.0;
    let intermediate_epoch_ns = if has_date_add {
        let inter_epoch_days = iso_date_to_epoch_days(
            intermediate_iso.0, intermediate_iso.1, intermediate_iso.2,
        ) as i128;
        let inter_local_ns = inter_epoch_days * 86_400_000_000_000 + wall_time_ns;
        super::zoned_date_time::disambiguate_instant(tz, inter_local_ns, "compatible")
    } else {
        base_epoch_ns
    };

    // Add D days: advance date by D, keep same wall-clock time, disambiguate
    let has_day_add = d != 0.0;
    let day_advanced_epoch_ns = if has_day_add || has_date_add {
        let day_advanced_iso = add_iso_date(
            intermediate_iso.0, intermediate_iso.1, intermediate_iso.2, 0, 0, 0, d as i32,
        );
        if !super::iso_date_within_limits(day_advanced_iso.0, day_advanced_iso.1, day_advanced_iso.2)
        {
            return Err(());
        }
        let day_adv_epoch_days = iso_date_to_epoch_days(
            day_advanced_iso.0, day_advanced_iso.1, day_advanced_iso.2,
        ) as i128;
        let day_adv_local_ns = day_adv_epoch_days * 86_400_000_000_000 + wall_time_ns;
        super::zoned_date_time::disambiguate_instant(tz, day_adv_local_ns, "compatible")
    } else {
        intermediate_epoch_ns
    };

    // Add time components
    let time_ns: i128 = h as i128 * 3_600_000_000_000
        + mi as i128 * 60_000_000_000
        + s as i128 * 1_000_000_000
        + ms as i128 * 1_000_000
        + us as i128 * 1_000
        + ns as i128;
    let dest_epoch_ns = day_advanced_epoch_ns + time_ns;

    match unit {
        "day" => {
            // Total days from base to dest (includes Y/M/W/D contributions)
            let tz_days = nanoseconds_to_tz_days(
                dest_epoch_ns, base_epoch_ns,
                base_year, base_month, base_day,
                wall_time_ns, tz,
            );
            Ok(tz_days)
        }
        "year" | "month" | "week" => {
            // DifferenceZonedDateTimeWithTotal: total from base to dest
            // Get end date in the timezone
            let end_bi = num_bigint::BigInt::from(dest_epoch_ns);
            let (ey, em, ed, eh, emi, es, ems, eus, ens) =
                super::zoned_date_time::epoch_ns_to_components(&end_bi, tz);

            // Date difference from base to end
            let diff_result = super::difference_iso_date(
                base_year, base_month, base_day, ey, em, ed, unit,
            );
            let diff_primary = match unit {
                "year" => diff_result.0,
                "month" => diff_result.1,
                "week" => diff_result.2,
                _ => unreachable!(),
            };

            // Compute sign
            let sign = if dest_epoch_ns > base_epoch_ns { 1i32 }
                else if dest_epoch_ns < base_epoch_ns { -1i32 } else { return Ok(0.0); };

            // Compute unit_boundary: base + diff_primary units, then + sign units
            let (ref_y, ref_mo, ref_w) = match unit {
                "year" => (diff_primary, 0, 0),
                "month" => (0, diff_primary, 0),
                "week" => (0, 0, diff_primary),
                _ => unreachable!(),
            };
            let unit_start_iso = add_iso_date(
                base_year, base_month, base_day, ref_y, ref_mo, ref_w, 0,
            );
            let (next_y, next_mo, next_w) = match unit {
                "year" => (diff_primary + sign, 0, 0),
                "month" => (0, diff_primary + sign, 0),
                "week" => (0, 0, diff_primary + sign),
                _ => unreachable!(),
            };
            let unit_end_iso = add_iso_date(
                base_year, base_month, base_day, next_y, next_mo, next_w, 0,
            );
            if !super::iso_date_within_limits(unit_end_iso.0, unit_end_iso.1, unit_end_iso.2) {
                return Err(());
            }

            // Convert these boundary dates to ZDT epoch_ns (same wall time)
            let start_epoch_days = iso_date_to_epoch_days(
                unit_start_iso.0, unit_start_iso.1, unit_start_iso.2,
            ) as i128;
            let start_local_ns = start_epoch_days * 86_400_000_000_000 + wall_time_ns;
            let unit_start_ns = super::zoned_date_time::disambiguate_instant(
                tz, start_local_ns, "compatible",
            );

            let end_epoch_days_val = iso_date_to_epoch_days(
                unit_end_iso.0, unit_end_iso.1, unit_end_iso.2,
            ) as i128;
            let end_local_ns = end_epoch_days_val * 86_400_000_000_000 + wall_time_ns;
            let unit_end_ns = super::zoned_date_time::disambiguate_instant(
                tz, end_local_ns, "compatible",
            );

            let unit_length_ns = (unit_end_ns - unit_start_ns).abs();
            let position_in_unit = (dest_epoch_ns - unit_start_ns) * sign as i128;
            let fractional = if unit_length_ns > 0 {
                position_in_unit as f64 / unit_length_ns as f64
            } else {
                0.0
            };
            Ok(diff_primary as f64 + fractional)
        }
        _ => {
            // Time units: total = (dest_epoch_ns - base_epoch_ns) / unit_ns
            let total_ns = dest_epoch_ns - base_epoch_ns;
            let unit_ns = temporal_unit_length_ns(unit) as i128;
            Ok(divide_i128_to_f64(total_ns, unit_ns))
        }
    }
}

/// Round a duration with calendar-aware relativeTo, then re-balance per spec.
/// Implements NudgeToCalendarUnit + BalanceDateDurationRelative.
/// Returns (years, months, weeks, days, hours, minutes, seconds, ms, µs, ns).
fn round_relative_duration(
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
    smallest_unit: &str,
    largest_unit: &str,
    increment: f64,
    rounding_mode: &str,
    base_year: i32,
    base_month: u8,
    base_day: u8,
    is_zdt: bool,
    zdt_info: Option<&(i128, String)>,
) -> Result<(f64, f64, f64, f64, f64, f64, f64, f64, f64, f64), String> {
    let su_order = temporal_unit_order(smallest_unit);
    let lu_order = temporal_unit_order(largest_unit);

    // Spec: cannot round to increment>1 of a calendar/day unit while balancing to a larger one
    let su_is_calendar_or_day = su_order >= temporal_unit_order("day");
    if increment > 1.0 && su_is_calendar_or_day && lu_order > su_order {
        return Err(format!(
            "Cannot round to an increment of {smallest_unit} while also balancing to {largest_unit}"
        ));
    }

    if su_order >= temporal_unit_order("day") {
        // Calendar/day unit rounding: NudgeToCalendarUnit
        let time_ns_i128: i128 = h as i128 * 3_600_000_000_000
            + mi as i128 * 60_000_000_000
            + s as i128 * 1_000_000_000
            + ms as i128 * 1_000_000
            + us as i128 * 1_000
            + ns as i128;

        if let Some((base_ens, tz)) = zdt_info {
            // ZDT NudgeToCalendarUnit: use actual epoch_ns for month/year boundaries
            // and actual day lengths for day rounding.
            let bi = num_bigint::BigInt::from(*base_ens);
            let (_, _, _, bh, bmi, bs, bms, bus, bns) =
                super::zoned_date_time::epoch_ns_to_components(&bi, tz);
            let wall_time_ns: i128 = bh as i128 * 3_600_000_000_000
                + bmi as i128 * 60_000_000_000
                + bs as i128 * 1_000_000_000
                + bms as i128 * 1_000_000
                + bus as i128 * 1_000
                + bns as i128;

            // Helper: compute AddZDT epoch_ns for a given (Y,M,W,D) added to base
            let add_zdt = |ay: i32, am: i32, aw: i32, ad: i32| -> i128 {
                let iso = add_iso_date(base_year, base_month, base_day, ay, am, aw, ad);
                let ed = iso_date_to_epoch_days(iso.0, iso.1, iso.2) as i128;
                let local = ed * 86_400_000_000_000 + wall_time_ns;
                super::zoned_date_time::disambiguate_instant(tz, local, "compatible")
            };

            // Compute dest_epoch_ns = AddZDT(base, fullDuration)
            let dest_epoch_ns = {
                let day_adv_ens = add_zdt(y as i32, mo as i32, w as i32, d as i32);
                day_adv_ens + time_ns_i128
            };

            // Compute end_date for the truncated duration (Y/M/W/D with NanosecondsToDays)
            let inter_ens = add_zdt(y as i32, mo as i32, w as i32, 0);
            let frac_days = nanoseconds_to_tz_days(
                dest_epoch_ns, inter_ens,
                {
                    let iso = add_iso_date(base_year, base_month, base_day, y as i32, mo as i32, w as i32, 0);
                    iso.0
                },
                {
                    let iso = add_iso_date(base_year, base_month, base_day, y as i32, mo as i32, w as i32, 0);
                    iso.1
                },
                {
                    let iso = add_iso_date(base_year, base_month, base_day, y as i32, mo as i32, w as i32, 0);
                    iso.2
                },
                wall_time_ns, tz,
            );
            let total_days_i = frac_days.trunc() as i32;
            let end_date = add_iso_date(
                base_year, base_month, base_day, y as i32, mo as i32, w as i32, total_days_i,
            );

            let sign_f = if dest_epoch_ns > *base_ens { 1i128 }
                else if dest_epoch_ns < *base_ens { -1i128 } else { 0 };
            if sign_f == 0 {
                return Ok((0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0));
            }

            let (ry, rm, rw, rd) = match smallest_unit {
                "year" => {
                    let end_epoch = iso_date_to_epoch_days(end_date.0, end_date.1, end_date.2);
                    let base_epoch = iso_date_to_epoch_days(base_year, base_month, base_day);
                    let (diff_y, _, _, _) = super::difference_iso_date(
                        base_year, base_month, base_day,
                        end_date.0, end_date.1, end_date.2, "year",
                    );
                    if end_epoch == base_epoch {
                        (0, 0, 0, 0)
                    } else {
                        // ZDT boundaries for the year
                        let start_ens = add_zdt(diff_y, 0, 0, 0);
                        let end_ens = add_zdt(diff_y + sign_f as i32, 0, 0, 0);
                        let span = (end_ens - start_ens).abs() as f64;
                        let remaining = (dest_epoch_ns - start_ens) as f64;
                        let fractional = diff_y as f64 + if span > 0.0 { remaining / span } else { 0.0 };
                        let rounded = super::round_number_to_increment(fractional, increment, rounding_mode);
                        (rounded as i32, 0, 0, 0)
                    }
                }
                "month" => {
                    let end_epoch = iso_date_to_epoch_days(end_date.0, end_date.1, end_date.2);
                    let base_epoch = iso_date_to_epoch_days(base_year, base_month, base_day);
                    if end_epoch == base_epoch {
                        (0, 0, 0, 0)
                    } else if largest_unit == "year" {
                        // Keep years fixed, round months component using ZDT boundaries
                        let start_ens = add_zdt(y as i32, mo as i32, 0, 0);
                        let end_ens = add_zdt(y as i32, mo as i32 + sign_f as i32, 0, 0);
                        let span = (end_ens - start_ens).abs() as f64;
                        let remaining = (dest_epoch_ns - start_ens) as f64;
                        let fractional = mo + if span > 0.0 { remaining / span } else { 0.0 };
                        let rounded = super::round_number_to_increment(fractional, increment, rounding_mode);
                        (y as i32, rounded as i32, 0, 0)
                    } else {
                        // Flatten to total months using ZDT boundaries
                        let (_, total_months, _, _) = super::difference_iso_date(
                            base_year, base_month, base_day,
                            end_date.0, end_date.1, end_date.2, "month",
                        );
                        let start_ens = add_zdt(0, total_months, 0, 0);
                        let end_ens = add_zdt(0, total_months + sign_f as i32, 0, 0);
                        let span = (end_ens - start_ens).abs() as f64;
                        let remaining = (dest_epoch_ns - start_ens) as f64;
                        let fractional = total_months as f64 + if span > 0.0 { remaining / span } else { 0.0 };
                        let rounded = super::round_number_to_increment(fractional, increment, rounding_mode);
                        (0, rounded as i32, 0, 0)
                    }
                }
                "week" => {
                    let end_epoch = iso_date_to_epoch_days(end_date.0, end_date.1, end_date.2);
                    let base_epoch = iso_date_to_epoch_days(base_year, base_month, base_day);
                    let preserve_months = matches!(largest_unit, "year" | "month");
                    if preserve_months {
                        let (_, total_months, _, _) = super::difference_iso_date(
                            base_year, base_month, base_day,
                            end_date.0, end_date.1, end_date.2, "month",
                        );
                        let month_start_ens = add_zdt(0, total_months, 0, 0);
                        let remaining_ns = (dest_epoch_ns - month_start_ens) as f64;
                        // 1 week = 7 actual days from month_start
                        let week_end_ens = add_zdt(0, total_months, 0, 7);
                        let week_ns = (week_end_ens - month_start_ens).abs() as f64;
                        let fractional_weeks = if week_ns > 0.0 { remaining_ns / week_ns } else { 0.0 };
                        let rounded = super::round_number_to_increment(fractional_weeks, increment, rounding_mode);
                        (0, total_months, rounded as i32, 0)
                    } else {
                        let total_days_from_base = (end_epoch - base_epoch) as f64 + frac_days.fract();
                        let fractional_weeks = total_days_from_base / 7.0;
                        let rounded = super::round_number_to_increment(fractional_weeks, increment, rounding_mode);
                        (0, 0, rounded as i32, 0)
                    }
                }
                "day" => {
                    // ZDT day rounding: use actual day length, not constant 86400s
                    // Range check
                    let end_days_i64 = total_days_i as i64 + sign_f as i64 * increment as i64;
                    let nudge_base = if y != 0.0 || mo != 0.0 || w != 0.0 {
                        add_iso_date(base_year, base_month, base_day, y as i32, mo as i32, w as i32, 0)
                    } else {
                        (base_year, base_month, base_day)
                    };
                    let nudge_end = add_iso_date(
                        nudge_base.0, nudge_base.1, nudge_base.2, 0, 0, 0, end_days_i64 as i32,
                    );
                    if iso_date_to_epoch_days(nudge_end.0, nudge_end.1, nudge_end.2).abs() > 100_000_000 {
                        return Err("Rounded date outside valid ISO range".to_string());
                    }

                    // Compute actual day length at current position using ZDT
                    let inter_iso = add_iso_date(
                        base_year, base_month, base_day, y as i32, mo as i32, w as i32, 0,
                    );
                    let day_pos_iso = add_iso_date(
                        inter_iso.0, inter_iso.1, inter_iso.2, 0, 0, 0, total_days_i,
                    );
                    let day_pos_ens = add_zdt(y as i32, mo as i32, w as i32, total_days_i);
                    let next_day_ens = add_zdt(
                        y as i32, mo as i32, w as i32, total_days_i + sign_f as i32,
                    );
                    let day_length_ns = (next_day_ens - day_pos_ens).abs();
                    let remaining_ns = dest_epoch_ns - day_pos_ens;

                    // Round using actual day length as the increment unit
                    let inc_ns = increment as i128 * day_length_ns;
                    let total_ns_in_day = total_days_i as i128 * day_length_ns + remaining_ns;
                    let rounded_ns = super::round_i128_to_increment(
                        remaining_ns, inc_ns, rounding_mode,
                    );
                    let rounded_days = total_days_i + (rounded_ns / day_length_ns) as i32;

                    if y != 0.0 || mo != 0.0 || w != 0.0 {
                        (y as i32, mo as i32, w as i32, rounded_days)
                    } else {
                        (0, 0, 0, rounded_days)
                    }
                }
                _ => (y as i32, mo as i32, w as i32, total_days_i),
            };

            // BalanceDateDurationRelative
            if matches!(largest_unit, "year" | "month" | "week")
                || (largest_unit == "day" && (ry != 0 || rm != 0 || rw != 0))
            {
                let result_date = add_iso_date(base_year, base_month, base_day, ry, rm, rw, rd);
                if !super::iso_date_within_limits(result_date.0, result_date.1, result_date.2) {
                    return Err("Rounded date outside valid ISO range".to_string());
                }
                let (dy, dm, mut dw, mut dd) = super::difference_iso_date(
                    base_year, base_month, base_day,
                    result_date.0, result_date.1, result_date.2,
                    largest_unit,
                );
                if smallest_unit == "week" && dd != 0 {
                    dw = dd / 7;
                    dd = dd % 7;
                }
                Ok((dy as f64, dm as f64, dw as f64, dd as f64, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0))
            } else {
                Ok((ry as f64, rm as f64, rw as f64, rd as f64, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0))
            }
        } else {
            // PlainDate path: use constant 24h days and ISO epoch days
            let frac_days = {
                let total_ns_i128 = d as i128 * 86_400_000_000_000 + time_ns_i128;
                let max_instant_ns: i128 = 86400 * 1_000_000_000 * 100_000_000;
                if total_ns_i128.abs() > max_instant_ns {
                    return Err("Total nanoseconds too large".to_string());
                }
                d + time_ns_i128 as f64 / 86_400_000_000_000.0
            };

            let (ry, rm, rw, rd) = super::round_date_duration_with_frac_days(
                y as i32, mo as i32, w as i32, frac_days, time_ns_i128,
                smallest_unit, largest_unit, increment, rounding_mode,
                base_year, base_month, base_day, is_zdt,
            )?;

            if matches!(largest_unit, "year" | "month" | "week")
                || (largest_unit == "day" && (ry != 0 || rm != 0 || rw != 0))
            {
                let result_date = add_iso_date(base_year, base_month, base_day, ry, rm, rw, rd);
                if !super::iso_date_within_limits(result_date.0, result_date.1, result_date.2) {
                    return Err("Rounded date outside valid ISO range".to_string());
                }
                let (dy, dm, mut dw, mut dd) = super::difference_iso_date(
                    base_year, base_month, base_day,
                    result_date.0, result_date.1, result_date.2,
                    largest_unit,
                );
                if smallest_unit == "week" && dd != 0 {
                    dw = dd / 7;
                    dd = dd % 7;
                }
                Ok((dy as f64, dm as f64, dw as f64, dd as f64, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0))
            } else {
                Ok((ry as f64, rm as f64, rw as f64, rd as f64, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0))
            }
        }
    } else {
        // Time unit rounding: flatten to ns with calendar-aware day resolution
        if let Some((base_ens, tz)) = zdt_info {
            // ZDT NudgeToZonedTime: normalize time within actual day boundaries,
            // then round, then adjust for overflow.
            let lu_order = temporal_unit_order(largest_unit);
            let lu_is_day_or_larger = lu_order >= temporal_unit_order("day");

            if lu_is_day_or_larger {
                // Get wall-clock time at base
                let bi = num_bigint::BigInt::from(*base_ens);
                let (_, _, _, bh, bmi, bs, bms, bus, bns) =
                    super::zoned_date_time::epoch_ns_to_components(&bi, tz);
                let wall_time_ns: i128 = bh as i128 * 3_600_000_000_000
                    + bmi as i128 * 60_000_000_000
                    + bs as i128 * 1_000_000_000
                    + bms as i128 * 1_000_000
                    + bus as i128 * 1_000
                    + bns as i128;

                // Add Y/M/W to base date
                let inter_iso = add_iso_date(
                    base_year, base_month, base_day, y as i32, mo as i32, w as i32, 0,
                );

                // Add D days
                let mut day_iso = add_iso_date(
                    inter_iso.0, inter_iso.1, inter_iso.2, 0, 0, 0, d as i32,
                );

                // Compute dest_epoch_ns by adding time to day_iso with wall time
                let day_epoch_days = iso_date_to_epoch_days(day_iso.0, day_iso.1, day_iso.2) as i128;
                let day_local_ns = day_epoch_days * 86_400_000_000_000 + wall_time_ns;
                let day_epoch_ns = super::zoned_date_time::disambiguate_instant(
                    tz, day_local_ns, "compatible",
                );
                let time_ns: i128 = h as i128 * 3_600_000_000_000
                    + mi as i128 * 60_000_000_000
                    + s as i128 * 1_000_000_000
                    + ms as i128 * 1_000_000
                    + us as i128 * 1_000
                    + ns as i128;
                let dest_epoch_ns = day_epoch_ns + time_ns;

                // Compute start-of-day for day_iso
                let mut day_start_ns = super::zoned_date_time::get_start_of_day(
                    tz, day_epoch_days,
                );

                // Normalize: carry full days using actual day lengths
                let sign: i128 = if dest_epoch_ns >= day_start_ns {
                    if dest_epoch_ns == day_start_ns && time_ns == 0 { 1 } else { 1 }
                } else { -1 };

                let mut time_within_day = dest_epoch_ns - day_start_ns;
                let mut extra_days: i32 = 0;

                if sign >= 0 {
                    // Forward: carry while time_within_day >= day_length
                    loop {
                        let next_ed = day_epoch_days + extra_days as i128 + 1;
                        let next_start = super::zoned_date_time::get_start_of_day(tz, next_ed);
                        let day_length = next_start - day_start_ns;
                        if day_length <= 0 { break; }
                        if time_within_day < day_length { break; }
                        time_within_day -= day_length;
                        extra_days += 1;
                        day_start_ns = next_start;
                        if extra_days > 200_000_000 { break; }
                    }
                } else {
                    // Backward: borrow while time_within_day < 0
                    while time_within_day < 0 {
                        let prev_ed = day_epoch_days + extra_days as i128 - 1;
                        let prev_start = super::zoned_date_time::get_start_of_day(tz, prev_ed);
                        let day_length = day_start_ns - prev_start;
                        if day_length <= 0 { break; }
                        time_within_day += day_length;
                        extra_days -= 1;
                        day_start_ns = prev_start;
                        if extra_days.abs() > 200_000_000 { break; }
                    }
                }

                if extra_days != 0 {
                    day_iso = add_iso_date(day_iso.0, day_iso.1, day_iso.2, 0, 0, 0, extra_days);
                }

                // Round time_within_day to increment
                let unit_ns = temporal_unit_length_ns(smallest_unit) as i128;
                let inc = increment as i128;
                let mut rounded_time = super::round_i128_to_increment(
                    time_within_day, unit_ns * inc, rounding_mode,
                );

                // AdjustRoundedDurationDays: if rounded time overflows the day, carry
                // and re-round the beyond amount per spec step 11b.
                let day_ed = iso_date_to_epoch_days(day_iso.0, day_iso.1, day_iso.2) as i128;
                let current_day_start = super::zoned_date_time::get_start_of_day(tz, day_ed);
                let next_day_start = super::zoned_date_time::get_start_of_day(tz, day_ed + 1);
                let current_day_length = next_day_start - current_day_start;
                if rounded_time >= current_day_length && current_day_length > 0 {
                    let beyond = rounded_time - current_day_length;
                    day_iso = add_iso_date(day_iso.0, day_iso.1, day_iso.2, 0, 0, 0, 1);
                    // Re-round the beyond amount to the same increment
                    rounded_time = super::round_i128_to_increment(
                        beyond, unit_ns * inc, rounding_mode,
                    );
                }

                // BalanceDateDurationRelative
                let (dy, dm, dw, dd) = super::difference_iso_date(
                    base_year, base_month, base_day,
                    day_iso.0, day_iso.1, day_iso.2,
                    largest_unit,
                );

                let r = unbalance_time_ns_i128(rounded_time, "hour");
                Ok((
                    dy as f64, dm as f64, dw as f64, dd as f64,
                    r.1 as f64, r.2 as f64, r.3 as f64,
                    r.4 as f64, r.5 as f64, r.6 as f64,
                ))
            } else {
                // largestUnit < "day": flatten to total ns, round, split by time units
                let dest_ns = add_duration_to_zdt_epoch_ns(
                    y, mo, w, d, h, mi, s, ms, us, ns,
                    base_year, base_month, base_day, *base_ens, tz,
                ).map_err(|_| "duration out of range".to_string())?;
                let total_ns = dest_ns - base_ens;

                let unit_ns = temporal_unit_length_ns(smallest_unit) as i128;
                let inc = increment as i128;
                let rounded_ns = super::round_i128_to_increment(total_ns, unit_ns * inc, rounding_mode);

                let limit = (1i128 << 53) * 1_000_000_000;
                if rounded_ns.abs() >= limit {
                    return Err("Rounded duration time is out of range".to_string());
                }

                let r = unbalance_time_ns_i128(rounded_ns, largest_unit);
                Ok((
                    0.0, 0.0, 0.0, r.0 as f64, r.1 as f64, r.2 as f64, r.3 as f64, r.4 as f64,
                    r.5 as f64, r.6 as f64,
                ))
            }
        } else {
            // PlainDate path: use constant 24h days
            let target = add_iso_date(
                base_year, base_month, base_day, y as i32, mo as i32, w as i32, 0,
            );
            let base_epoch = iso_date_to_epoch_days(base_year, base_month, base_day);
            let target_epoch = iso_date_to_epoch_days(target.0, target.1, target.2);
            let calendar_days = (target_epoch - base_epoch) as i128;

            let time_ns: i128 = h as i128 * 3_600_000_000_000
                + mi as i128 * 60_000_000_000
                + s as i128 * 1_000_000_000
                + ms as i128 * 1_000_000
                + us as i128 * 1_000
                + ns as i128;
            let total_ns = (calendar_days + d as i128) * 86_400_000_000_000 + time_ns;

            let unit_ns = temporal_unit_length_ns(smallest_unit) as i128;
            let inc = increment as i128;
            let rounded_ns = super::round_i128_to_increment(total_ns, unit_ns * inc, rounding_mode);

            let limit = (1i128 << 53) * 1_000_000_000;
            if rounded_ns.abs() >= limit {
                return Err("Rounded duration time is out of range".to_string());
            }

            if matches!(largest_unit, "year" | "month" | "week") {
                let total_days = rounded_ns / 86_400_000_000_000;
                let remainder_ns = rounded_ns - total_days * 86_400_000_000_000;
                let (ry, rm, rd_result) =
                    add_iso_date(base_year, base_month, base_day, 0, 0, 0, total_days as i32);
                let (dy, dm, dw, dd) = super::difference_iso_date(
                    base_year, base_month, base_day, ry, rm, rd_result, largest_unit,
                );
                let r = unbalance_time_ns_i128(remainder_ns, "hour");
                Ok((
                    dy as f64, dm as f64, dw as f64, dd as f64, r.1 as f64, r.2 as f64, r.3 as f64,
                    r.4 as f64, r.5 as f64, r.6 as f64,
                ))
            } else {
                let r = unbalance_time_ns_i128(rounded_ns, largest_unit);
                Ok((
                    0.0, 0.0, 0.0, r.0 as f64, r.1 as f64, r.2 as f64, r.3 as f64, r.4 as f64,
                    r.5 as f64, r.6 as f64,
                ))
            }
        }
    }
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
                // Alphabetical order per spec
                let (hd, nd) = get_field!("days", d);
                let (hh, nh) = get_field!("hours", h);
                let (hus, nus) = get_field!("microseconds", us);
                let (hms, nms) = get_field!("milliseconds", ms);
                let (hmi, nmi) = get_field!("minutes", mi);
                let (hmo, nmo) = get_field!("months", mo);
                let (hns, nns) = get_field!("nanoseconds", ns);
                let (hs, ns_val) = get_field!("seconds", s);
                let (hw, nw) = get_field!("weeks", w);
                let (hy, ny) = get_field!("years", y);
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
                // Use i128 arithmetic to avoid f64 precision loss
                let total_ns = ns as i128 + other.9 as i128
                    + (us as i128 + other.8 as i128) * 1_000
                    + (ms as i128 + other.7 as i128) * 1_000_000
                    + (s as i128 + other.6 as i128) * 1_000_000_000
                    + (mi as i128 + other.5 as i128) * 60_000_000_000
                    + (h as i128 + other.4 as i128) * 3_600_000_000_000
                    + (d as i128 + other.3 as i128) * 86_400_000_000_000;
                let lu1 = default_temporal_largest_unit(y, mo, w, d, h, mi, s, ms, us);
                let lu2 = default_temporal_largest_unit(
                    other.0, other.1, other.2, other.3, other.4,
                    other.5, other.6, other.7, other.8,
                );
                let lu = larger_of_two_temporal_units(&lu1, &lu2);
                let (ry, rmo, rw, rd, rh, rmi, rs, rms, rus, rns) =
                    balance_from_i128_ns(total_ns, &lu);
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
                // Use i128 arithmetic to avoid f64 precision loss
                let total_ns = ns as i128 - other.9 as i128
                    + (us as i128 - other.8 as i128) * 1_000
                    + (ms as i128 - other.7 as i128) * 1_000_000
                    + (s as i128 - other.6 as i128) * 1_000_000_000
                    + (mi as i128 - other.5 as i128) * 60_000_000_000
                    + (h as i128 - other.4 as i128) * 3_600_000_000_000
                    + (d as i128 - other.3 as i128) * 86_400_000_000_000;
                let lu1 = default_temporal_largest_unit(y, mo, w, d, h, mi, s, ms, us);
                let lu2 = default_temporal_largest_unit(
                    other.0, other.1, other.2, other.3, other.4,
                    other.5, other.6, other.7, other.8,
                );
                let lu = larger_of_two_temporal_units(&lu1, &lu2);
                let (ry, rmo, rw, rd, rh, rmi, rs, rms, rus, rns) =
                    balance_from_i128_ns(total_ns, &lu);
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

                let (smallest_unit, rounding_mode, increment, largest_unit, relative_to) =
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

                // Calendar units require relativeTo:
                // - Duration has calendar units (years/months/weeks), OR
                // - Target unit is a calendar unit (years/months/weeks)
                let has_calendar = y != 0.0 || mo != 0.0 || w != 0.0;
                let target_is_calendar = matches!(smallest_unit, "year" | "month" | "week")
                    || matches!(largest_unit, "year" | "month" | "week");
                if (has_calendar || target_is_calendar) && relative_to.is_none() {
                    return Completion::Throw(interp.create_range_error(
                        "relativeTo is required for rounding durations with calendar units",
                    ));
                }

                if let Some((by, bm, bd, zdt_info)) = relative_to {
                    let is_zdt = zdt_info.is_some();
                    // Spec early return: zero duration returns P0D before boundary checks
                    // For PlainDate: always (ISODateTimeWithinLimits at midnight is skipped)
                    // For ZDT: only when largestUnit < "day" (day+ units need boundary checks)
                    let is_zero = y == 0.0
                        && mo == 0.0
                        && w == 0.0
                        && d == 0.0
                        && h == 0.0
                        && mi == 0.0
                        && s == 0.0
                        && ms == 0.0
                        && us == 0.0
                        && ns == 0.0;
                    let lu_order = temporal_unit_order(largest_unit);
                    if is_zero && (!is_zdt || lu_order < temporal_unit_order("day"))
                    {
                        return create_duration_result(
                            interp, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                        );
                    }

                    // Range checks after early return
                    let time_ns_total: i128 = h as i128 * 3_600_000_000_000
                        + mi as i128 * 60_000_000_000
                        + s as i128 * 1_000_000_000
                        + ms as i128 * 1_000_000
                        + us as i128 * 1_000
                        + ns as i128;
                    if let Some((ens, _)) = &zdt_info {
                        let end_ns = ens + d as i128 * 86_400_000_000_000 + time_ns_total;
                        let ns_max: i128 = 8_640_000_000_000_000_000_000;
                        if end_ns < -ns_max || end_ns > ns_max {
                            return Completion::Throw(interp.create_range_error(
                                "duration out of range when applied to relativeTo",
                            ));
                        }
                    } else if !super::iso_date_time_within_limits(by, bm, bd, 0, 0, 0, 0, 0, 0) {
                        return Completion::Throw(interp.create_range_error(
                            "duration out of range when applied to relativeTo",
                        ));
                    }
                    // NudgeToZonedTime pre-check: next-day boundary must be in range
                    if is_zdt
                        && temporal_unit_order(smallest_unit) < temporal_unit_order("day")
                    {
                        let target = super::add_iso_date(
                            by, bm, bd, y as i32, mo as i32, w as i32, d as i32,
                        );
                        let (ny, nm, nd) =
                            super::balance_iso_date(target.0, target.1 as i32, target.2 as i32 + 1);
                        let next_days = super::iso_date_to_epoch_days(ny, nm, nd);
                        if next_days.abs() > 100_000_000 {
                            return Completion::Throw(
                                interp.create_range_error("next day boundary is out of range"),
                            );
                        }
                    }
                    // Spec step 24: If smallestUnit is "nanosecond" and increment is 1,
                    // skip rounding entirely — just AdjustRoundedDurationDays + Balance.
                    // But only when largestUnit >= "day", because time-unit targets need
                    // NanosecondsToDays to convert days to actual timezone-aware time.
                    if smallest_unit == "nanosecond" && increment == 1.0
                        && temporal_unit_order(largest_unit) >= temporal_unit_order("day")
                    {
                        let (mut rd, mut rh, mut rmi, mut rs, mut rms, mut rus, mut rns) =
                            (d, h, mi, s, ms, us, ns);

                        // AdjustRoundedDurationDays for ZDT
                        if let Some((base_ens, tz)) = &zdt_info {
                            let time_ns: i128 = rh as i128 * 3_600_000_000_000
                                + rmi as i128 * 60_000_000_000
                                + rs as i128 * 1_000_000_000
                                + rms as i128 * 1_000_000
                                + rus as i128 * 1_000
                                + rns as i128;
                            let direction: i128 = if time_ns > 0 { 1 }
                                else if time_ns < 0 { -1 } else { 0 };
                            if direction != 0 {
                                let day_start = add_duration_to_zdt_epoch_ns(
                                    y, mo, w, rd, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                                    by, bm, bd, *base_ens, tz,
                                ).unwrap_or(*base_ens);
                                let day_end = add_duration_to_zdt_epoch_ns(
                                    y, mo, w, rd + direction as f64, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                                    by, bm, bd, *base_ens, tz,
                                ).unwrap_or(*base_ens);
                                let day_length_ns = day_end - day_start;
                                if day_length_ns > 0 {
                                    let one_day_less = time_ns - day_length_ns;
                                    // Adjust if time >= one day (direction>0: time_ns >= day_length, direction<0: time_ns <= -day_length)
                                    let less_sign = if one_day_less > 0 { 1i128 }
                                        else if one_day_less < 0 { -1 } else { 0 };
                                    if less_sign != -direction {
                                        // Time exceeds one day: carry
                                        rd += direction as f64;
                                        let r = unbalance_time_ns_i128(one_day_less, "hour");
                                        rh = r.1 as f64;
                                        rmi = r.2 as f64;
                                        rs = r.3 as f64;
                                        rms = r.4 as f64;
                                        rus = r.5 as f64;
                                        rns = r.6 as f64;
                                    }
                                }
                            }
                        }

                        // BalanceTimeDuration: convert time to days for PlainDate
                        if !is_zdt && temporal_unit_order(largest_unit) >= temporal_unit_order("day") {
                            let time_ns: i128 = rh as i128 * 3_600_000_000_000
                                + rmi as i128 * 60_000_000_000
                                + rs as i128 * 1_000_000_000
                                + rms as i128 * 1_000_000
                                + rus as i128 * 1_000
                                + rns as i128;
                            let extra_days = time_ns / 86_400_000_000_000;
                            let rem_ns = time_ns % 86_400_000_000_000;
                            rd += extra_days as f64;
                            let r = unbalance_time_ns_i128(rem_ns, "hour");
                            rh = r.1 as f64;
                            rmi = r.2 as f64;
                            rs = r.3 as f64;
                            rms = r.4 as f64;
                            rus = r.5 as f64;
                            rns = r.6 as f64;
                        }

                        // BalanceDateDurationRelative
                        if matches!(largest_unit, "year" | "month" | "week")
                            || (largest_unit == "day"
                                && (y != 0.0 || mo != 0.0 || w != 0.0))
                        {
                            let result_date = super::add_iso_date(
                                by, bm, bd, y as i32, mo as i32, w as i32, rd as i32,
                            );
                            let (dy, dm, dw, dd) = super::difference_iso_date(
                                by, bm, bd,
                                result_date.0, result_date.1, result_date.2,
                                largest_unit,
                            );
                            return create_duration_result(
                                interp,
                                dy as f64, dm as f64, dw as f64, dd as f64,
                                rh, rmi, rs, rms, rus, rns,
                            );
                        }
                        return create_duration_result(
                            interp, y, mo, w, rd, rh, rmi, rs, rms, rus, rns,
                        );
                    }

                    match round_relative_duration(
                        y,
                        mo,
                        w,
                        d,
                        h,
                        mi,
                        s,
                        ms,
                        us,
                        ns,
                        smallest_unit,
                        largest_unit,
                        increment,
                        rounding_mode,
                        by,
                        bm,
                        bd,
                        is_zdt,
                        zdt_info.as_ref(),
                    ) {
                        Ok((ry, rm, rw, rd, rh, rmi, rs, rms, rus, rns)) => create_duration_result(
                            interp, ry, rm, rw, rd, rh, rmi, rs, rms, rus, rns,
                        ),
                        Err(msg) => Completion::Throw(interp.create_range_error(&msg)),
                    }
                } else {
                    // Use i128 for precision with large values
                    let total_ns: i128 = d as i128 * 86_400_000_000_000
                        + h as i128 * 3_600_000_000_000
                        + mi as i128 * 60_000_000_000
                        + s as i128 * 1_000_000_000
                        + ms as i128 * 1_000_000
                        + us as i128 * 1_000
                        + ns as i128;

                    let unit_ns = temporal_unit_length_ns(smallest_unit) as i128;
                    let inc = increment as i128;
                    let rounded_ns =
                        super::round_i128_to_increment(total_ns, unit_ns * inc, rounding_mode);
                    let r = unbalance_time_ns_i128(rounded_ns, largest_unit);
                    create_duration_result(
                        interp, y, mo, w, r.0 as f64, r.1 as f64, r.2 as f64, r.3 as f64,
                        r.4 as f64, r.5 as f64, r.6 as f64,
                    )
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

                let (unit, relative_to) = if let JsValue::String(ref su) = total_of {
                    let su_str = su.to_rust_string();
                    let u = match temporal_unit_singular(&su_str) {
                        Some(u) => u,
                        None => {
                            return Completion::Throw(
                                interp.create_range_error(&format!("Invalid unit: {su_str}")),
                            );
                        }
                    };
                    (u, None)
                } else if matches!(total_of, JsValue::Object(_)) {
                    // Per spec: get + process relativeTo first, then get + coerce unit
                    let rt = match get_prop(interp, &total_of, "relativeTo") {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let relative = match to_relative_to_date(interp, &rt) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let u = try_completion!(get_prop(interp, &total_of, "unit"));
                    if is_undefined(&u) {
                        return Completion::Throw(interp.create_range_error("unit is required"));
                    }
                    let us = try_result!(interp, interp.to_string_value(&u));
                    let unit = match temporal_unit_singular(&us) {
                        Some(u) => u,
                        None => {
                            return Completion::Throw(
                                interp.create_range_error(&format!("Invalid unit: {us}")),
                            );
                        }
                    };
                    (unit, relative)
                } else {
                    return Completion::Throw(
                        interp.create_type_error("total requires a string or options object"),
                    );
                };

                // Calendar units require relativeTo:
                // - Duration has calendar units, OR
                // - Target unit is a calendar unit (year/month/week)
                let has_calendar = y != 0.0 || mo != 0.0 || w != 0.0;
                let target_is_calendar = matches!(unit, "year" | "month" | "week");
                if (has_calendar || target_is_calendar) && relative_to.is_none() {
                    return Completion::Throw(
                        interp.create_range_error("relativeTo is required for calendar units"),
                    );
                }

                if let Some((by, bm, bd, Some((ens, ref tz)))) = relative_to {
                    // ZDT relativeTo path — use timezone-aware day lengths
                    match total_relative_duration_zdt(
                        y, mo, w, d, h, mi, s, ms, us, ns, unit,
                        by, bm, bd, ens, tz,
                    ) {
                        Ok(result) => Completion::Normal(JsValue::Number(result)),
                        Err(()) => Completion::Throw(interp.create_range_error(
                            "duration out of range when applied to relativeTo",
                        )),
                    }
                } else if let Some((by, bm, bd, None)) = relative_to {
                    // PlainDate relativeTo path
                    // Spec: DifferencePlainDateTimeWithTotal step 1: if isoDateTime1 = isoDateTime2, return 0
                    let is_zero = y == 0.0
                        && mo == 0.0
                        && w == 0.0
                        && d == 0.0
                        && h == 0.0
                        && mi == 0.0
                        && s == 0.0
                        && ms == 0.0
                        && us == 0.0
                        && ns == 0.0;
                    if is_zero {
                        return Completion::Normal(JsValue::Number(0.0));
                    }
                    // Spec step 2: ISODateTimeWithinLimits on base at midnight
                    if !super::iso_date_time_within_limits(by, bm, bd, 0, 0, 0, 0, 0, 0) {
                        return Completion::Throw(interp.create_range_error(
                            "duration out of range when applied to relativeTo",
                        ));
                    }
                    match total_relative_duration(
                        y, mo, w, d, h, mi, s, ms, us, ns, unit, by, bm, bd,
                    ) {
                        Ok(result) => Completion::Normal(JsValue::Number(result)),
                        Err(()) => Completion::Throw(interp.create_range_error(
                            "duration out of range when applied to relativeTo",
                        )),
                    }
                } else {
                    let total_ns: i128 = d as i128 * 86_400_000_000_000
                        + h as i128 * 3_600_000_000_000
                        + mi as i128 * 60_000_000_000
                        + s as i128 * 1_000_000_000
                        + ms as i128 * 1_000_000
                        + us as i128 * 1_000
                        + ns as i128;
                    let unit_ns = temporal_unit_length_ns(unit) as i128;
                    let result = divide_i128_to_f64(total_ns, unit_ns);
                    Completion::Normal(JsValue::Number(result))
                }
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

                let (precision, rounding_mode) = match parse_to_string_options(interp, args.first())
                {
                    Ok(p) => p,
                    Err(c) => return c,
                };

                let result = match format_duration_iso(
                    y,
                    mo,
                    w,
                    d,
                    h,
                    mi,
                    s,
                    ms,
                    us,
                    ns,
                    precision,
                    rounding_mode,
                ) {
                    Ok(s) => s,
                    Err(msg) => return Completion::Throw(interp.create_range_error(&msg)),
                };
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
                let result =
                    match format_duration_iso(y, mo, w, d, h, mi, s, ms, us, ns, None, "trunc") {
                        Ok(s) => s,
                        Err(msg) => return Completion::Throw(interp.create_range_error(&msg)),
                    };
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
            |interp, this, args| {
                let fields = match get_duration_fields(interp, &this) {
                    Ok(f) => f,
                    Err(c) => return c,
                };
                let df_val = match interp.intl_duration_format_ctor.clone() {
                    Some(v) => v,
                    None => {
                        let (y, mo, w, d, h, mi, s, ms, us, ns) = fields;
                        let result = match format_duration_iso(y, mo, w, d, h, mi, s, ms, us, ns, None, "trunc") {
                            Ok(s) => s,
                            Err(msg) => return Completion::Throw(interp.create_range_error(&msg)),
                        };
                        return Completion::Normal(JsValue::String(JsString::from_str(&result)));
                    }
                };
                let locales_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let options_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let df_instance = match interp.construct(&df_val, &[locales_arg, options_arg]) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Completion::Throw(e),
                    _ => return Completion::Normal(JsValue::Undefined),
                };
                if let JsValue::Object(df_obj) = &df_instance {
                    let format_val = match interp.get_object_property(df_obj.id, "format", &df_instance) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => JsValue::Undefined,
                    };
                    match interp.call_function(&format_val, &df_instance, &[this.clone()]) {
                        Completion::Normal(v) => Completion::Normal(v),
                        Completion::Throw(e) => Completion::Throw(e),
                        _ => Completion::Normal(JsValue::Undefined),
                    }
                } else {
                    let (y, mo, w, d, h, mi, s, ms, us, ns) = fields;
                    let result = match format_duration_iso(y, mo, w, d, h, mi, s, ms, us, ns, None, "trunc") {
                        Ok(s) => s,
                        Err(msg) => return Completion::Throw(interp.create_range_error(&msg)),
                    };
                    Completion::Normal(JsValue::String(JsString::from_str(&result)))
                }
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
                if one.0 == two.0
                    && one.1 == two.1
                    && one.2 == two.2
                    && one.3 == two.3
                    && one.4 == two.4
                    && one.5 == two.5
                    && one.6 == two.6
                    && one.7 == two.7
                    && one.8 == two.8
                    && one.9 == two.9
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

                let (ns1, ns2) = if let Some((by, bm, bd, Some((ens, ref tz)))) = relative_to {
                    // ZDT: compute actual endpoint epoch_ns for each duration
                    let n1 = match add_duration_to_zdt_epoch_ns(
                        one.0, one.1, one.2, one.3, one.4, one.5, one.6, one.7, one.8, one.9,
                        by, bm, bd, ens, tz,
                    ) {
                        Ok(v) => v,
                        Err(()) => {
                            return Completion::Throw(interp.create_range_error(
                                "duration out of range when applied to relativeTo",
                            ));
                        }
                    };
                    let n2 = match add_duration_to_zdt_epoch_ns(
                        two.0, two.1, two.2, two.3, two.4, two.5, two.6, two.7, two.8, two.9,
                        by, bm, bd, ens, tz,
                    ) {
                        Ok(v) => v,
                        Err(()) => {
                            return Completion::Throw(interp.create_range_error(
                                "duration out of range when applied to relativeTo",
                            ));
                        }
                    };
                    (n1, n2)
                } else if let Some((by, bm, bd, None)) = relative_to {
                    let n1 = match duration_total_ns_relative(
                        one.0, one.1, one.2, one.3, one.4, one.5, one.6, one.7, one.8, one.9, by,
                        bm, bd,
                    ) {
                        Ok(v) => v,
                        Err(()) => {
                            return Completion::Throw(interp.create_range_error(
                                "duration out of range when applied to relativeTo",
                            ));
                        }
                    };
                    let n2 = match duration_total_ns_relative(
                        two.0, two.1, two.2, two.3, two.4, two.5, two.6, two.7, two.8, two.9, by,
                        bm, bd,
                    ) {
                        Ok(v) => v,
                        Err(()) => {
                            return Completion::Throw(interp.create_range_error(
                                "duration out of range when applied to relativeTo",
                            ));
                        }
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
            Completion::Throw(interp.create_range_error(&format!("Invalid duration string: {s}")))
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
        return Err(Completion::Throw(
            interp.create_type_error("Invalid duration: expected string or object"),
        ));
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
) -> Result<
    (
        &'static str,
        &'static str,
        f64,
        &'static str,
        Option<(i32, u8, u8, Option<(i128, String)>)>,
    ),
    Completion,
> {
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
        return Ok((unit, "halfExpand", 1.0, largest, None));
    }
    if !matches!(round_to, JsValue::Object(_)) {
        return Err(Completion::Throw(
            interp.create_type_error("round requires a string or options object"),
        ));
    }

    // Read and coerce each option in alphabetical order per spec.
    // Each option is get+coerced before the next is read.

    // 1. largestUnit: get + coerce
    let large_unit_val = match get_prop(interp, round_to, "largestUnit") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let large_unit_str = if !is_undefined(&large_unit_val) {
        Some(
            interp
                .to_string_value(&large_unit_val)
                .map_err(Completion::Throw)?,
        )
    } else {
        None
    };

    // 2. relativeTo: get + process (bag fields read here for correct observable order)
    let relative_to_val = match get_prop(interp, round_to, "relativeTo") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let relative_to = to_relative_to_date(interp, &relative_to_val)?;

    // 3. roundingIncrement: get + coerce
    let inc_val = match get_prop(interp, round_to, "roundingIncrement") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let increment = coerce_rounding_increment(interp, &inc_val)?;

    // 4. roundingMode: get + coerce
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

    // 5. smallestUnit: get + coerce
    let small_unit_val = match get_prop(interp, round_to, "smallestUnit") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let small_unit = if is_undefined(&small_unit_val) {
        None
    } else {
        let su = interp
            .to_string_value(&small_unit_val)
            .map_err(Completion::Throw)?;
        Some(temporal_unit_singular(&su).ok_or_else(|| {
            Completion::Throw(interp.create_range_error(&format!("Invalid unit: {su}")))
        })?)
    };

    // Both smallestUnit and largestUnit undefined → error (auto counts as provided)
    if small_unit.is_none() && large_unit_str.is_none() {
        return Err(Completion::Throw(
            interp.create_range_error("smallestUnit or largestUnit is required"),
        ));
    }

    let small_unit = small_unit.unwrap_or("nanosecond");

    let large_unit = if let Some(ref lu_str) = large_unit_str {
        if lu_str == "auto" {
            let def = default_largest_unit_for_duration(y, mo, w, d, h, mi, s, ms, us, ns);
            if temporal_unit_order(def) < temporal_unit_order(small_unit) {
                small_unit
            } else {
                def
            }
        } else {
            temporal_unit_singular(lu_str).ok_or_else(|| {
                Completion::Throw(interp.create_range_error(&format!("Invalid unit: {lu_str}")))
            })?
        }
    } else {
        let def = default_largest_unit_for_duration(y, mo, w, d, h, mi, s, ms, us, ns);
        if temporal_unit_order(def) < temporal_unit_order(small_unit) {
            small_unit
        } else {
            def
        }
    };

    // Validate roundingIncrement against smallestUnit
    if let Some(max) = max_rounding_increment(small_unit) {
        let i = increment as u64;
        if i >= max {
            return Err(Completion::Throw(interp.create_range_error(&format!(
                "roundingIncrement {increment} is out of range for {small_unit}"
            ))));
        }
        if max % i != 0 {
            return Err(Completion::Throw(interp.create_range_error(&format!(
                "roundingIncrement {increment} does not divide evenly into {max}"
            ))));
        }
    }

    Ok((
        small_unit,
        rounding_mode,
        increment,
        large_unit,
        relative_to,
    ))
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

    // Read and coerce options in alphabetical order: fractionalSecondDigits, roundingMode, smallestUnit
    // Each option is read AND coerced before the next is read (spec observable ordering).

    // 1. fractionalSecondDigits: get + coerce
    let fp = match get_prop(interp, &options, "fractionalSecondDigits") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let fsd_result: Option<Option<u8>> = if is_undefined(&fp) {
        None // will use default
    } else if matches!(fp, JsValue::Number(_)) {
        let n = interp.to_number_value(&fp).map_err(Completion::Throw)?;
        if n.is_nan() || !n.is_finite() {
            return Err(Completion::Throw(interp.create_range_error(
                "fractionalSecondDigits must be 0-9 or 'auto'",
            )));
        }
        let floored = n.floor();
        if floored < 0.0 || floored > 9.0 {
            return Err(Completion::Throw(interp.create_range_error(
                "fractionalSecondDigits must be 0-9 or 'auto'",
            )));
        }
        Some(Some(floored as u8))
    } else {
        let s = interp.to_string_value(&fp).map_err(Completion::Throw)?;
        if s == "auto" {
            Some(None)
        } else {
            return Err(Completion::Throw(interp.create_range_error(
                "fractionalSecondDigits must be 0-9 or 'auto'",
            )));
        }
    };

    // 2. roundingMode: get + coerce
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

    // 3. smallestUnit: get + coerce (only if not undefined)
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

    // smallestUnit was undefined; use fractionalSecondDigits result
    match fsd_result {
        None => Ok((None, rounding_mode)),
        Some(digits) => Ok((digits, rounding_mode)),
    }
}

/// Balance total nanoseconds into a duration tuple using i128 precision.
fn balance_from_i128_ns(
    total_ns: i128,
    largest_unit: &str,
) -> (f64, f64, f64, f64, f64, f64, f64, f64, f64, f64) {
    let sign = if total_ns < 0 { -1i128 } else { 1 };
    let mut remaining = total_ns.abs();
    let lu_order = super::temporal_unit_order(largest_unit);

    let days = if lu_order >= super::temporal_unit_order("day") {
        let d = remaining / 86_400_000_000_000;
        remaining %= 86_400_000_000_000;
        d
    } else {
        0
    };
    let hours = if lu_order >= super::temporal_unit_order("hour") {
        let h = remaining / 3_600_000_000_000;
        remaining %= 3_600_000_000_000;
        h
    } else {
        0
    };
    let minutes = if lu_order >= super::temporal_unit_order("minute") {
        let m = remaining / 60_000_000_000;
        remaining %= 60_000_000_000;
        m
    } else {
        0
    };
    let seconds = if lu_order >= super::temporal_unit_order("second") {
        let s = remaining / 1_000_000_000;
        remaining %= 1_000_000_000;
        s
    } else {
        0
    };
    let milliseconds = if lu_order >= super::temporal_unit_order("millisecond") {
        let ms = remaining / 1_000_000;
        remaining %= 1_000_000;
        ms
    } else {
        0
    };
    let microseconds = if lu_order >= super::temporal_unit_order("microsecond") {
        let us = remaining / 1_000;
        remaining %= 1_000;
        us
    } else {
        0
    };
    let nanoseconds = remaining;

    (
        0.0,
        0.0,
        0.0,
        (sign * days) as f64,
        (sign * hours) as f64,
        (sign * minutes) as f64,
        (sign * seconds) as f64,
        (sign * milliseconds) as f64,
        (sign * microseconds) as f64,
        (sign * nanoseconds) as f64,
    )
}

fn default_temporal_largest_unit(
    years: f64,
    months: f64,
    weeks: f64,
    days: f64,
    hours: f64,
    minutes: f64,
    seconds: f64,
    milliseconds: f64,
    microseconds: f64,
) -> String {
    if years != 0.0 {
        "year"
    } else if months != 0.0 {
        "month"
    } else if weeks != 0.0 {
        "week"
    } else if days != 0.0 {
        "day"
    } else if hours != 0.0 {
        "hour"
    } else if minutes != 0.0 {
        "minute"
    } else if seconds != 0.0 {
        "second"
    } else if milliseconds != 0.0 {
        "millisecond"
    } else if microseconds != 0.0 {
        "microsecond"
    } else {
        "nanosecond"
    }
    .to_string()
}

fn larger_of_two_temporal_units(a: &str, b: &str) -> String {
    let order = |u: &str| match u {
        "year" => 9,
        "month" => 8,
        "week" => 7,
        "day" => 6,
        "hour" => 5,
        "minute" => 4,
        "second" => 3,
        "millisecond" => 2,
        "microsecond" => 1,
        _ => 0,
    };
    if order(a) >= order(b) { a } else { b }.to_string()
}

/// Balance total nanoseconds into (days, hours, minutes, seconds, frac_ns)
fn balance_time_ns(total_ns: i128, largest_unit: &str) -> (i128, i128, i128, i128, u64) {
    let mut remaining = total_ns;
    let days = if largest_unit == "day"
        || largest_unit == "week"
        || largest_unit == "month"
        || largest_unit == "year"
    {
        let d = remaining / 86_400_000_000_000;
        remaining -= d * 86_400_000_000_000;
        d
    } else {
        0
    };
    let hours = if largest_unit != "minute"
        && largest_unit != "second"
        && largest_unit != "millisecond"
        && largest_unit != "microsecond"
        && largest_unit != "nanosecond"
    {
        let h = remaining / 3_600_000_000_000;
        remaining -= h * 3_600_000_000_000;
        h
    } else {
        0
    };
    let minutes = if largest_unit != "second"
        && largest_unit != "millisecond"
        && largest_unit != "microsecond"
        && largest_unit != "nanosecond"
    {
        let m = remaining / 60_000_000_000;
        remaining -= m * 60_000_000_000;
        m
    } else {
        0
    };
    let seconds = remaining / 1_000_000_000;
    let frac_ns = (remaining - seconds * 1_000_000_000) as u64;
    (days, hours, minutes, seconds, frac_ns)
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
) -> Result<String, String> {
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

    // Per spec: combine all time components + days into total nanoseconds
    let time_ns_i128 = nanoseconds.abs() as i128
        + microseconds.abs() as i128 * 1_000
        + milliseconds.abs() as i128 * 1_000_000
        + seconds.abs() as i128 * 1_000_000_000
        + minutes.abs() as i128 * 60_000_000_000
        + hours.abs() as i128 * 3_600_000_000_000
        + days.abs() as i128 * 86_400_000_000_000;

    // Apply rounding if precision is specified (RoundTimeDuration + BalanceTimeDuration)
    let (balanced_s, frac_ns, ami, ah, extra_days) = if let Some(p) = precision {
        let increment = 10i128.pow(9 - p as u32);
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
        let rounded = round_i128_to_increment(time_ns_i128, increment, effective_mode);
        // IsNormalizedTimeDurationWithinRange: |rounded| < 2^53 * 10^9
        const MAX_TIME_NS: i128 = (1i128 << 53) * 1_000_000_000;
        if rounded >= MAX_TIME_NS {
            return Err("Rounded duration time is out of range".to_string());
        }
        // BalanceTimeDuration: extract days, hours, minutes, seconds from rounded total
        // Use LargerOfTwoTemporalUnits(DefaultTemporalLargestUnit, "seconds")
        let largest_orig = default_temporal_largest_unit(
            years,
            months,
            weeks,
            days,
            hours,
            minutes,
            seconds,
            milliseconds,
            microseconds,
        );
        let balance_unit = larger_of_two_temporal_units(&largest_orig, "seconds");

        let (rd, rh, rm, rs, rfrac) = balance_time_ns(rounded, &balance_unit);
        (rs, rfrac, rm, rh, rd)
    } else {
        // No rounding: use original components
        let total_sub = nanoseconds.abs() as i128
            + microseconds.abs() as i128 * 1_000
            + milliseconds.abs() as i128 * 1_000_000
            + seconds.abs() as i128 * 1_000_000_000;
        let bs = total_sub / 1_000_000_000;
        let fns = (total_sub - bs * 1_000_000_000) as u64;
        (bs, fns, minutes.abs() as i128, hours.abs() as i128, 0i128)
    };

    // After rounding, validate result (IsValidDuration)
    const MAX_SAFE: i128 = (1i128 << 53) - 1;
    // When rounding was applied, extra_days already includes original days
    let total_days = if precision.is_some() {
        extra_days
    } else {
        days.abs() as i128
    };
    if precision.is_some() {
        if balanced_s > MAX_SAFE || ami > MAX_SAFE || ah > MAX_SAFE || total_days > MAX_SAFE {
            return Err("Rounded duration is out of range".to_string());
        }
    }

    let mut result = String::new();
    if sign < 0 {
        result.push('-');
    }
    result.push('P');

    let (ay, amo, aw) = (years.abs(), months.abs(), weeks.abs());
    let ad = total_days;

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
    if ad != 0 {
        result.push_str(&format!("{ad}"));
        result.push('D');
    }

    let has_time = ah != 0 || ami != 0 || balanced_s != 0 || frac_ns != 0 || precision.is_some();
    let has_date = ay != 0.0 || amo != 0.0 || aw != 0.0 || ad != 0;

    if has_time || !has_date {
        result.push('T');
        if ah != 0 {
            result.push_str(&format!("{ah}"));
            result.push('H');
        }
        if ami != 0 {
            result.push_str(&format!("{ami}"));
            result.push('M');
        }

        let need_seconds =
            balanced_s != 0 || frac_ns != 0 || (!has_time && !has_date) || precision.is_some();
        if need_seconds {
            let sec_part = format!("{balanced_s}");
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
        return Ok("PT0S".to_string());
    }
    Ok(result)
}

/// Round a non-negative i128 value to the nearest multiple of increment.
fn round_i128_to_increment(value: i128, increment: i128, mode: &str) -> i128 {
    let remainder = value % increment;
    if remainder == 0 {
        return value;
    }
    let truncated = value - remainder;
    let expanded = truncated + increment;
    match mode {
        "trunc" | "floor" => truncated,
        "ceil" | "expand" => expanded,
        "halfExpand" | "halfCeil" => {
            if remainder * 2 >= increment {
                expanded
            } else {
                truncated
            }
        }
        "halfTrunc" | "halfFloor" => {
            if remainder * 2 > increment {
                expanded
            } else {
                truncated
            }
        }
        "halfEven" => {
            if remainder * 2 > increment {
                expanded
            } else if remainder * 2 < increment {
                truncated
            } else {
                if (truncated / increment) % 2 == 0 {
                    truncated
                } else {
                    expanded
                }
            }
        }
        _ => truncated,
    }
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
    let result = unbalance_time_ns_i128(total_ns as i128, largest_unit);
    (
        result.0 as f64,
        result.1 as f64,
        result.2 as f64,
        result.3 as f64,
        result.4 as f64,
        result.5 as f64,
        result.6 as f64,
    )
}

fn unbalance_time_ns_i128(
    total_ns: i128,
    largest_unit: &str,
) -> (i128, i128, i128, i128, i128, i128, i128) {
    match largest_unit {
        "day" | "days" => {
            let d = total_ns / 86_400_000_000_000;
            let rem = total_ns - d * 86_400_000_000_000;
            let h = rem / 3_600_000_000_000;
            let rem = rem - h * 3_600_000_000_000;
            let mi = rem / 60_000_000_000;
            let rem = rem - mi * 60_000_000_000;
            let s = rem / 1_000_000_000;
            let rem = rem - s * 1_000_000_000;
            let ms = rem / 1_000_000;
            let rem = rem - ms * 1_000_000;
            let us = rem / 1_000;
            let ns = rem - us * 1_000;
            (d, h, mi, s, ms, us, ns)
        }
        "hour" | "hours" => {
            let h = total_ns / 3_600_000_000_000;
            let rem = total_ns - h * 3_600_000_000_000;
            let mi = rem / 60_000_000_000;
            let rem = rem - mi * 60_000_000_000;
            let s = rem / 1_000_000_000;
            let rem = rem - s * 1_000_000_000;
            let ms = rem / 1_000_000;
            let rem = rem - ms * 1_000_000;
            let us = rem / 1_000;
            let ns = rem - us * 1_000;
            (0, h, mi, s, ms, us, ns)
        }
        "minute" | "minutes" => {
            let mi = total_ns / 60_000_000_000;
            let rem = total_ns - mi * 60_000_000_000;
            let s = rem / 1_000_000_000;
            let rem = rem - s * 1_000_000_000;
            let ms = rem / 1_000_000;
            let rem = rem - ms * 1_000_000;
            let us = rem / 1_000;
            let ns = rem - us * 1_000;
            (0, 0, mi, s, ms, us, ns)
        }
        "second" | "seconds" => {
            let s = total_ns / 1_000_000_000;
            let rem = total_ns - s * 1_000_000_000;
            let ms = rem / 1_000_000;
            let rem = rem - ms * 1_000_000;
            let us = rem / 1_000;
            let ns = rem - us * 1_000;
            (0, 0, 0, s, ms, us, ns)
        }
        "millisecond" | "milliseconds" => {
            let ms = total_ns / 1_000_000;
            let rem = total_ns - ms * 1_000_000;
            let us = rem / 1_000;
            let ns = rem - us * 1_000;
            (0, 0, 0, 0, ms, us, ns)
        }
        "microsecond" | "microseconds" => {
            let us = total_ns / 1_000;
            let ns = total_ns - us * 1_000;
            (0, 0, 0, 0, 0, us, ns)
        }
        _ => (0, 0, 0, 0, 0, 0, total_ns),
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

    // Convert day+time to total nanoseconds using i128 and re-balance
    let total_ns = nanoseconds as i128
        + microseconds as i128 * 1_000
        + milliseconds as i128 * 1_000_000
        + seconds as i128 * 1_000_000_000
        + minutes as i128 * 60_000_000_000
        + hours as i128 * 3_600_000_000_000
        + days as i128 * 86_400_000_000_000;

    let sign: i128 = if total_ns < 0 {
        -1
    } else if total_ns > 0 {
        1
    } else {
        0
    };
    let abs_ns = total_ns.abs();
    let (rd, rh, rmi, rs, rms, rus, rns) = unbalance_time_ns_i128(abs_ns, largest);

    (
        years,
        months,
        weeks,
        (rd * sign) as f64,
        (rh * sign) as f64,
        (rmi * sign) as f64,
        (rs * sign) as f64,
        (rms * sign) as f64,
        (rus * sign) as f64,
        (rns * sign) as f64,
    )
}
