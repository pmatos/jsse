use super::super::super::*;
use super::listformat::create_list_formatter;
use super::numberformat::format_number_internal;
use super::numberformat::format_to_parts_internal;
use super::numberformat::is_known_numbering_system;

struct DurationFormatData {
    locale: String,
    numbering_system: String,
    style: String,
    years: String,
    years_display: String,
    months: String,
    months_display: String,
    weeks: String,
    weeks_display: String,
    days: String,
    days_display: String,
    hours: String,
    hours_display: String,
    minutes: String,
    minutes_display: String,
    seconds: String,
    seconds_display: String,
    milliseconds: String,
    milliseconds_display: String,
    microseconds: String,
    microseconds_display: String,
    nanoseconds: String,
    nanoseconds_display: String,
    fractional_digits: Option<u32>,
}

struct DurationRecord {
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
}

impl DurationRecord {
    fn is_zero(&self) -> bool {
        self.years == 0.0
            && self.months == 0.0
            && self.weeks == 0.0
            && self.days == 0.0
            && self.hours == 0.0
            && self.minutes == 0.0
            && self.seconds == 0.0
            && self.milliseconds == 0.0
            && self.microseconds == 0.0
            && self.nanoseconds == 0.0
    }

    fn has_negative(&self) -> bool {
        self.years < 0.0
            || self.months < 0.0
            || self.weeks < 0.0
            || self.days < 0.0
            || self.hours < 0.0
            || self.minutes < 0.0
            || self.seconds < 0.0
            || self.milliseconds < 0.0
            || self.microseconds < 0.0
            || self.nanoseconds < 0.0
    }

    fn get_value(&self, unit: &str) -> f64 {
        match unit {
            "years" => self.years,
            "months" => self.months,
            "weeks" => self.weeks,
            "days" => self.days,
            "hours" => self.hours,
            "minutes" => self.minutes,
            "seconds" => self.seconds,
            "milliseconds" => self.milliseconds,
            "microseconds" => self.microseconds,
            "nanoseconds" => self.nanoseconds,
            _ => 0.0,
        }
    }
}

impl DurationFormatData {
    fn get_style(&self, unit: &str) -> &str {
        match unit {
            "years" => &self.years,
            "months" => &self.months,
            "weeks" => &self.weeks,
            "days" => &self.days,
            "hours" => &self.hours,
            "minutes" => &self.minutes,
            "seconds" => &self.seconds,
            "milliseconds" => &self.milliseconds,
            "microseconds" => &self.microseconds,
            "nanoseconds" => &self.nanoseconds,
            _ => "short",
        }
    }

    fn get_display(&self, unit: &str) -> &str {
        match unit {
            "years" => &self.years_display,
            "months" => &self.months_display,
            "weeks" => &self.weeks_display,
            "days" => &self.days_display,
            "hours" => &self.hours_display,
            "minutes" => &self.minutes_display,
            "seconds" => &self.seconds_display,
            "milliseconds" => &self.milliseconds_display,
            "microseconds" => &self.microseconds_display,
            "nanoseconds" => &self.nanoseconds_display,
            _ => "auto",
        }
    }
}

const UNITS: &[&str] = &[
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
];

fn duration_to_fractional(dur: &DurationRecord, exponent: u32) -> String {
    let seconds = dur.seconds;
    let milliseconds = dur.milliseconds;
    let microseconds = dur.microseconds;
    let nanoseconds = dur.nanoseconds;

    match exponent {
        9 => {
            if milliseconds == 0.0 && microseconds == 0.0 && nanoseconds == 0.0 {
                return format_f64_no_trailing(seconds);
            }
        }
        6 => {
            if microseconds == 0.0 && nanoseconds == 0.0 {
                return format_f64_no_trailing(milliseconds);
            }
        }
        3 => {
            if nanoseconds == 0.0 {
                return format_f64_no_trailing(microseconds);
            }
        }
        _ => {}
    }

    // Use i128 arithmetic for precision
    let ns_total: i128 = nanoseconds as i128;
    let ns = match exponent {
        9 => {
            let mut ns = ns_total;
            ns += (seconds as i128) * 1_000_000_000;
            ns += (milliseconds as i128) * 1_000_000;
            ns += (microseconds as i128) * 1_000;
            ns
        }
        6 => {
            let mut ns = ns_total;
            ns += (milliseconds as i128) * 1_000_000;
            ns += (microseconds as i128) * 1_000;
            ns
        }
        3 => {
            let mut ns = ns_total;
            ns += (microseconds as i128) * 1_000;
            ns
        }
        _ => ns_total,
    };

    let e = 10_i128.pow(exponent);
    let q = ns / e;
    let mut r = ns % e;
    if r < 0 {
        r = -r;
    }
    let r_str = format!("{:0>width$}", r, width = exponent as usize);
    format!("{}.{}", q, r_str)
}

fn is_valid_numbering_system(ns: &str) -> bool {
    if ns.is_empty() {
        return false;
    }
    for part in ns.split('-') {
        if part.len() < 3 || part.len() > 8 {
            return false;
        }
        if !part.chars().all(|c| c.is_ascii_alphanumeric()) {
            return false;
        }
    }
    true
}

fn extract_unicode_extension(locale_str: &str, key: &str) -> Option<String> {
    let lower = locale_str.to_lowercase();
    let search_str = if let Some(x_idx) = lower.find("-x-") {
        &lower[..x_idx]
    } else {
        &lower[..]
    };
    let u_idx = search_str.find("-u-")?;
    let ext_part = &search_str[u_idx + 3..];
    let tokens: Vec<&str> = ext_part.split('-').collect();
    for i in 0..tokens.len() {
        if tokens[i] == key {
            if i + 1 < tokens.len() && tokens[i + 1].len() > 2 {
                return Some(tokens[i + 1].to_string());
            }
            if i + 1 < tokens.len() && tokens[i + 1].len() == 2 {
                return Some("true".to_string());
            }
            if i + 1 < tokens.len() {
                return Some(tokens[i + 1].to_string());
            }
            return Some("true".to_string());
        }
    }
    None
}

fn strip_unicode_extensions(locale_str: &str) -> String {
    let search_end = locale_str.find("-x-").unwrap_or(locale_str.len());
    let search_part = &locale_str[..search_end];
    if let Some(idx) = search_part.find("-u-") {
        let before = &locale_str[..idx];
        let after_u = &locale_str[idx + 3..];
        let tokens: Vec<&str> = after_u.split('-').collect();
        let mut end_of_u = tokens.len();
        for i in 0..tokens.len() {
            if tokens[i].len() == 1 && tokens[i] != "u" {
                end_of_u = i;
                break;
            }
        }
        if end_of_u < tokens.len() {
            format!("{}-{}", before, tokens[end_of_u..].join("-"))
        } else {
            before.to_string()
        }
    } else {
        locale_str.to_string()
    }
}

fn base_locale(locale_str: &str) -> String {
    strip_unicode_extensions(locale_str)
}

fn normalize_zero(v: f64) -> f64 {
    if v == 0.0 { 0.0 } else { v }
}

fn format_f64_no_trailing(v: f64) -> String {
    if v == 0.0 && v.is_sign_negative() {
        return "0".to_string();
    }
    let s = format!("{}", v);
    if s.ends_with(".0") {
        s[..s.len() - 2].to_string()
    } else {
        s
    }
}

fn format_one_unit(
    value: f64,
    value_str: Option<&str>,
    style: &str,
    unit: &str,
    locale: &str,
    numbering_system: &str,
    sign_display: &str,
    extra_min_frac: Option<u32>,
    extra_max_frac: Option<u32>,
    rounding_mode: &str,
    min_integer_digits: u32,
) -> String {
    let nf_unit_singular = &unit[..unit.len() - 1]; // "years" -> "year"

    if style != "numeric" && style != "2-digit" {
        // Unit formatting: style=unit, unit=singular, unitDisplay=style
        let unit_opt = Some(nf_unit_singular.to_string());
        let unit_display_opt = Some(style.to_string());

        let actual_value = if let Some(vs) = value_str {
            vs.parse::<f64>().unwrap_or(value)
        } else {
            value
        };

        format_number_internal(
            actual_value,
            locale,
            "unit",
            &None,
            &None,
            &None,
            &unit_opt,
            &unit_display_opt,
            "standard",
            &None,
            sign_display,
            "auto",
            1,
            extra_min_frac.unwrap_or(0),
            extra_max_frac.unwrap_or(0),
            &None,
            &None,
            rounding_mode,
            1,
            "auto",
            "auto",
            numbering_system,
        )
    } else {
        // Numeric or 2-digit: plain number formatting
        let mid = if style == "2-digit" {
            2
        } else {
            min_integer_digits
        };

        let actual_value = if let Some(vs) = value_str {
            vs.parse::<f64>().unwrap_or(value)
        } else {
            value
        };

        format_number_internal(
            actual_value,
            locale,
            "decimal",
            &None,
            &None,
            &None,
            &None,
            &None,
            "standard",
            &None,
            sign_display,
            "false",
            mid,
            extra_min_frac.unwrap_or(0),
            extra_max_frac.unwrap_or(0),
            &None,
            &None,
            rounding_mode,
            1,
            "auto",
            "auto",
            numbering_system,
        )
    }
}

// Returns Vec of (type, value) pairs from NumberFormat.formatToParts for a single unit
fn format_one_unit_parts(
    value: f64,
    value_str: Option<&str>,
    style: &str,
    unit: &str,
    locale: &str,
    numbering_system: &str,
    sign_display: &str,
    extra_min_frac: Option<u32>,
    extra_max_frac: Option<u32>,
    rounding_mode: &str,
    min_integer_digits: u32,
) -> Vec<(String, String)> {
    let nf_unit_singular = &unit[..unit.len() - 1];

    let actual_value = if let Some(vs) = value_str {
        vs.parse::<f64>().unwrap_or(value)
    } else {
        value
    };

    if style != "numeric" && style != "2-digit" {
        let unit_opt = Some(nf_unit_singular.to_string());
        let unit_display_opt = Some(style.to_string());

        format_to_parts_internal(
            actual_value,
            locale,
            "unit",
            &None,
            &None,
            &None,
            &unit_opt,
            &unit_display_opt,
            "standard",
            &None,
            sign_display,
            "auto",
            1,
            extra_min_frac.unwrap_or(0),
            extra_max_frac.unwrap_or(0),
            &None,
            &None,
            rounding_mode,
            1,
            "auto",
            "auto",
            numbering_system,
        )
    } else {
        let mid = if style == "2-digit" {
            2
        } else {
            min_integer_digits
        };

        format_to_parts_internal(
            actual_value,
            locale,
            "decimal",
            &None,
            &None,
            &None,
            &None,
            &None,
            "standard",
            &None,
            sign_display,
            "false",
            mid,
            extra_min_frac.unwrap_or(0),
            extra_max_frac.unwrap_or(0),
            &None,
            &None,
            rounding_mode,
            1,
            "auto",
            "auto",
            numbering_system,
        )
    }
}

fn format_duration(data: &DurationFormatData, dur: &DurationRecord) -> String {
    let time_separator = ":";
    let mut result: Vec<Vec<String>> = Vec::new();
    let mut need_separator = false;
    let mut display_negative_sign = true;

    let has_negative = dur.has_negative();

    let mut i = 0;
    while i < UNITS.len() {
        let unit = UNITS[i];
        let mut value = dur.get_value(unit);
        let style = data.get_style(unit);
        let display = data.get_display(unit);

        let mut done = false;
        let mut value_str: Option<String> = None;
        let mut extra_min_frac: Option<u32> = None;
        let mut extra_max_frac: Option<u32> = None;
        let mut rounding_mode = "halfExpand";

        // Numeric seconds and sub-seconds are combined into a single value.
        if unit == "seconds" || unit == "milliseconds" || unit == "microseconds" {
            if let Some(next_unit) = UNITS.get(i + 1) {
                let next_style = data.get_style(next_unit);
                if next_style == "numeric" {
                    let exponent = match unit {
                        "seconds" => 9u32,
                        "milliseconds" => 6,
                        _ => 3,
                    };
                    let frac_str = duration_to_fractional(dur, exponent);
                    value_str = Some(frac_str);

                    let fd = data.fractional_digits;
                    extra_max_frac = Some(fd.unwrap_or(9));
                    extra_min_frac = Some(fd.unwrap_or(0));
                    rounding_mode = "trunc";
                    done = true;
                }
            }
        }

        // Display zero numeric minutes when seconds will be displayed
        // (spec: hoursFormatted && secondsFormatted => minutesFormatted)
        let display_required = if unit == "minutes" && need_separator {
            let sec_display = data.get_display("seconds");
            sec_display == "always"
                || dur.seconds != 0.0
                || dur.milliseconds != 0.0
                || dur.microseconds != 0.0
                || dur.nanoseconds != 0.0
        } else {
            false
        };

        let effective_value = if let Some(ref vs) = value_str {
            vs.parse::<f64>().unwrap_or(value)
        } else {
            value
        };

        if effective_value != 0.0 || display != "auto" || display_required {
            let sign_display_str;
            if display_negative_sign {
                display_negative_sign = false;
                if value == 0.0 && value_str.is_none() && has_negative {
                    value = -0.0_f64;
                }
                sign_display_str = "auto";
            } else {
                sign_display_str = "never";
            }

            let formatted = format_one_unit(
                value,
                value_str.as_deref(),
                style,
                unit,
                &data.locale,
                &data.numbering_system,
                sign_display_str,
                extra_min_frac,
                extra_max_frac,
                rounding_mode,
                1,
            );

            if need_separator {
                // Append to current group with ":"
                if let Some(last) = result.last_mut() {
                    last.push(time_separator.to_string());
                    last.push(formatted);
                }
            } else {
                let mut group = Vec::new();
                group.push(formatted);

                if style == "2-digit" || style == "numeric" {
                    need_separator = true;
                }

                result.push(group);
            }
        }

        if done {
            break;
        }

        i += 1;
    }

    // Join all groups into strings
    let strings: Vec<String> = result.iter().map(|parts| parts.join("")).collect();

    if strings.is_empty() {
        return String::new();
    }

    if strings.len() == 1 {
        return strings[0].clone();
    }

    // Use ListFormat to join
    let list_style = if data.style == "digital" {
        "short"
    } else {
        &data.style
    };

    let formatter = create_list_formatter(&data.locale, "unit", list_style);
    formatter.format_to_string(strings.iter().map(|s| s.as_str()))
}

fn to_duration_record(
    interp: &mut Interpreter,
    input: &JsValue,
) -> Result<DurationRecord, JsValue> {
    // Step 1-2: If input is a string, parse as ISO 8601 duration
    if let JsValue::String(s) = input {
        let dur_str = s.to_rust_string();
        return parse_duration_string(interp, &dur_str);
    }

    // Not an object? TypeError
    if matches!(input, JsValue::Undefined | JsValue::Null) {
        return Err(interp.create_type_error("Duration must be an object or string"));
    }
    if !matches!(input, JsValue::Object(_)) {
        return Err(interp.create_type_error("Duration must be an object or string"));
    }

    let obj_id = if let JsValue::Object(o) = input {
        o.id
    } else {
        unreachable!()
    };

    // Check for Temporal.Duration
    if let Some(obj_data) = interp.get_object(obj_id) {
        let b = obj_data.borrow();
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
        }) = &b.temporal_data
        {
            let rec = DurationRecord {
                years: *years,
                months: *months,
                weeks: *weeks,
                days: *days,
                hours: *hours,
                minutes: *minutes,
                seconds: *seconds,
                milliseconds: *milliseconds,
                microseconds: *microseconds,
                nanoseconds: *nanoseconds,
            };
            drop(b);
            validate_duration_record(interp, &rec)?;
            return Ok(rec);
        }
    }

    let mut has_relevant_field = false;

    let get_field =
        |interp: &mut Interpreter, name: &str, has: &mut bool| -> Result<f64, JsValue> {
            let val = match interp.get_object_property(obj_id, name, input) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::Undefined,
            };

            if matches!(val, JsValue::Undefined) {
                return Ok(0.0);
            }

            *has = true;

            let num = interp.to_number_value(&val)?;
            if num.is_nan() || num.is_infinite() {
                return Err(interp
                    .create_range_error(&format!("Invalid duration value for {}: {}", name, num)));
            }

            // All duration values must be integers
            if num != num.trunc() {
                return Err(interp.create_range_error(&format!(
                    "Duration {} must be an integer, got {}",
                    name, num
                )));
            }

            Ok(num)
        };

    let years = normalize_zero(get_field(interp, "years", &mut has_relevant_field)?);
    let months = normalize_zero(get_field(interp, "months", &mut has_relevant_field)?);
    let weeks = normalize_zero(get_field(interp, "weeks", &mut has_relevant_field)?);
    let days = normalize_zero(get_field(interp, "days", &mut has_relevant_field)?);
    let hours = normalize_zero(get_field(interp, "hours", &mut has_relevant_field)?);
    let minutes = normalize_zero(get_field(interp, "minutes", &mut has_relevant_field)?);
    let seconds = normalize_zero(get_field(interp, "seconds", &mut has_relevant_field)?);
    let milliseconds = normalize_zero(get_field(interp, "milliseconds", &mut has_relevant_field)?);
    let microseconds = normalize_zero(get_field(interp, "microseconds", &mut has_relevant_field)?);
    let nanoseconds = normalize_zero(get_field(interp, "nanoseconds", &mut has_relevant_field)?);

    if !has_relevant_field {
        return Err(
            interp.create_type_error("Duration object must have at least one duration property")
        );
    }

    let rec = DurationRecord {
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
    };

    validate_duration_record(interp, &rec)?;
    Ok(rec)
}

fn validate_duration_record(interp: &mut Interpreter, rec: &DurationRecord) -> Result<(), JsValue> {
    // Check mixed signs
    let mut has_positive = false;
    let mut has_negative = false;
    let fields = [
        rec.years,
        rec.months,
        rec.weeks,
        rec.days,
        rec.hours,
        rec.minutes,
        rec.seconds,
        rec.milliseconds,
        rec.microseconds,
        rec.nanoseconds,
    ];
    for &v in &fields {
        if v > 0.0 {
            has_positive = true;
        }
        if v < 0.0 {
            has_negative = true;
        }
    }
    if has_positive && has_negative {
        return Err(
            interp.create_range_error("Duration cannot have mixed positive and negative values")
        );
    }

    // Check abs(years|months|weeks) < 2^32
    let limit = 4294967296.0_f64; // 2^32
    if rec.years.abs() >= limit {
        return Err(interp.create_range_error("Duration years value out of range"));
    }
    if rec.months.abs() >= limit {
        return Err(interp.create_range_error("Duration months value out of range"));
    }
    if rec.weeks.abs() >= limit {
        return Err(interp.create_range_error("Duration weeks value out of range"));
    }

    // Check normalizedSeconds: days * 86400 + hours * 3600 + minutes * 60 + seconds +
    // milliseconds * 1e-3 + microseconds * 1e-6 + nanoseconds * 1e-9
    // Use i128 nanosecond arithmetic for precision at the 2^53 boundary
    let days_ns = (rec.days as i128) * 86_400_000_000_000i128;
    let hours_ns = (rec.hours as i128) * 3_600_000_000_000i128;
    let minutes_ns = (rec.minutes as i128) * 60_000_000_000i128;
    let seconds_ns = (rec.seconds as i128) * 1_000_000_000i128;
    let millis_ns = (rec.milliseconds as i128) * 1_000_000i128;
    let micros_ns = (rec.microseconds as i128) * 1_000i128;
    let nanos_ns = rec.nanoseconds as i128;
    let total_ns = days_ns + hours_ns + minutes_ns + seconds_ns + millis_ns + micros_ns + nanos_ns;
    // limit: 2^53 seconds = 2^53 * 10^9 nanoseconds
    let limit_ns: i128 = 9_007_199_254_740_992_000_000_000;
    if total_ns.abs() >= limit_ns {
        return Err(interp
            .create_range_error("Duration value out of range: normalizedSeconds exceeds 2^53"));
    }

    Ok(())
}

fn parse_duration_string(interp: &mut Interpreter, s: &str) -> Result<DurationRecord, JsValue> {
    // Parse ISO 8601 duration string like "P1Y2M3W4DT5H6M7.008009010S"
    let s = s.trim();
    if s.is_empty() {
        return Err(interp.create_range_error("Invalid duration string"));
    }

    let mut neg = false;
    let mut chars = s.chars().peekable();

    // Optional sign
    if chars.peek() == Some(&'-') || chars.peek() == Some(&'\u{2212}') {
        neg = true;
        chars.next();
    } else if chars.peek() == Some(&'+') {
        chars.next();
    }

    if chars.next() != Some('P') {
        return Err(interp.create_range_error("Invalid duration string: must start with P"));
    }

    let mut years = 0.0_f64;
    let mut months = 0.0_f64;
    let mut weeks = 0.0_f64;
    let mut days = 0.0_f64;
    let mut hours = 0.0_f64;
    let mut minutes = 0.0_f64;
    let mut seconds = 0.0_f64;
    let mut milliseconds = 0.0_f64;
    let mut microseconds = 0.0_f64;
    let mut nanoseconds = 0.0_f64;

    let mut in_time = false;
    let remaining: String = chars.collect();
    let mut rest = remaining.as_str();

    while !rest.is_empty() {
        if rest.starts_with('T') {
            in_time = true;
            rest = &rest[1..];
            continue;
        }

        // Parse number
        let num_end = rest
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(rest.len());
        if num_end == 0 {
            return Err(interp.create_range_error("Invalid duration string"));
        }
        let num_str = &rest[..num_end];
        rest = &rest[num_end..];

        if rest.is_empty() {
            return Err(
                interp.create_range_error("Invalid duration string: missing unit designator")
            );
        }

        let designator = rest.chars().next().unwrap();
        rest = &rest[designator.len_utf8()..];

        if num_str.contains('.') && designator == 'S' && in_time {
            // Fractional seconds - parse carefully
            let parts: Vec<&str> = num_str.splitn(2, '.').collect();
            let whole: f64 = parts[0].parse().unwrap_or(0.0);
            let frac_str = parts.get(1).unwrap_or(&"");

            seconds = whole;

            // Parse fractional part into ms/us/ns
            let padded = format!("{:0<9}", frac_str);
            let ms_str = &padded[0..3];
            let us_str = &padded[3..6];
            let ns_str = &padded[6..9];

            milliseconds = ms_str.parse::<f64>().unwrap_or(0.0);
            microseconds = us_str.parse::<f64>().unwrap_or(0.0);
            nanoseconds = ns_str.parse::<f64>().unwrap_or(0.0);
        } else {
            let val: f64 = num_str
                .parse()
                .map_err(|_| interp.create_range_error("Invalid number in duration string"))?;

            match (in_time, designator) {
                (false, 'Y') => years = val,
                (false, 'M') => months = val,
                (false, 'W') => weeks = val,
                (false, 'D') => days = val,
                (true, 'H') => hours = val,
                (true, 'M') => minutes = val,
                (true, 'S') => seconds = val,
                _ => {
                    return Err(interp.create_range_error(&format!(
                        "Invalid duration designator: {}",
                        designator
                    )));
                }
            }
        }
    }

    if neg {
        years = -years;
        months = -months;
        weeks = -weeks;
        days = -days;
        hours = -hours;
        minutes = -minutes;
        seconds = -seconds;
        milliseconds = -milliseconds;
        microseconds = -microseconds;
        nanoseconds = -nanoseconds;
    }

    let rec = DurationRecord {
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
    };
    validate_duration_record(interp, &rec)?;
    Ok(rec)
}

fn extract_duration_format_data(
    interp: &mut Interpreter,
    this: &JsValue,
) -> Result<DurationFormatData, JsValue> {
    if let JsValue::Object(o) = this {
        if let Some(obj) = interp.get_object(o.id) {
            let b = obj.borrow();
            if let Some(IntlData::DurationFormat {
                ref locale,
                ref numbering_system,
                ref style,
                ref years,
                ref years_display,
                ref months,
                ref months_display,
                ref weeks,
                ref weeks_display,
                ref days,
                ref days_display,
                ref hours,
                ref hours_display,
                ref minutes,
                ref minutes_display,
                ref seconds,
                ref seconds_display,
                ref milliseconds,
                ref milliseconds_display,
                ref microseconds,
                ref microseconds_display,
                ref nanoseconds,
                ref nanoseconds_display,
                ref fractional_digits,
            }) = b.intl_data
            {
                return Ok(DurationFormatData {
                    locale: locale.clone(),
                    numbering_system: numbering_system.clone(),
                    style: style.clone(),
                    years: years.clone(),
                    years_display: years_display.clone(),
                    months: months.clone(),
                    months_display: months_display.clone(),
                    weeks: weeks.clone(),
                    weeks_display: weeks_display.clone(),
                    days: days.clone(),
                    days_display: days_display.clone(),
                    hours: hours.clone(),
                    hours_display: hours_display.clone(),
                    minutes: minutes.clone(),
                    minutes_display: minutes_display.clone(),
                    seconds: seconds.clone(),
                    seconds_display: seconds_display.clone(),
                    milliseconds: milliseconds.clone(),
                    milliseconds_display: milliseconds_display.clone(),
                    microseconds: microseconds.clone(),
                    microseconds_display: microseconds_display.clone(),
                    nanoseconds: nanoseconds.clone(),
                    nanoseconds_display: nanoseconds_display.clone(),
                    fractional_digits: *fractional_digits,
                });
            }
        }
    }
    Err(interp.create_type_error("Intl.DurationFormat method called on incompatible receiver"))
}

// Returns (type, value, unit) triples. unit is empty string for literal separators.
fn format_to_parts_duration(
    data: &DurationFormatData,
    dur: &DurationRecord,
) -> Vec<(String, String, String)> {
    let time_separator = ":";
    // Each group is a list of (type, value, unit) parts
    let mut result_groups: Vec<Vec<(String, String, String)>> = Vec::new();
    let mut need_separator = false;
    let mut display_negative_sign = true;

    let has_negative = dur.has_negative();

    let mut i = 0;
    while i < UNITS.len() {
        let unit = UNITS[i];
        let mut value = dur.get_value(unit);
        let style = data.get_style(unit);
        let display = data.get_display(unit);
        let nf_unit = &unit[..unit.len() - 1]; // "hours" -> "hour"

        let mut done = false;
        let mut value_str: Option<String> = None;
        let mut extra_min_frac: Option<u32> = None;
        let mut extra_max_frac: Option<u32> = None;
        let mut rounding_mode = "halfExpand";

        if unit == "seconds" || unit == "milliseconds" || unit == "microseconds" {
            if let Some(next_unit) = UNITS.get(i + 1) {
                let next_style = data.get_style(next_unit);
                if next_style == "numeric" {
                    let exponent = match unit {
                        "seconds" => 9u32,
                        "milliseconds" => 6,
                        _ => 3,
                    };
                    let frac_str = duration_to_fractional(dur, exponent);
                    value_str = Some(frac_str);

                    let fd = data.fractional_digits;
                    extra_max_frac = Some(fd.unwrap_or(9));
                    extra_min_frac = Some(fd.unwrap_or(0));
                    rounding_mode = "trunc";
                    done = true;
                }
            }
        }

        let display_required = if unit == "minutes" && need_separator {
            let sec_display = data.get_display("seconds");
            sec_display == "always"
                || dur.seconds != 0.0
                || dur.milliseconds != 0.0
                || dur.microseconds != 0.0
                || dur.nanoseconds != 0.0
        } else {
            false
        };

        let effective_value = if let Some(ref vs) = value_str {
            vs.parse::<f64>().unwrap_or(value)
        } else {
            value
        };

        if effective_value != 0.0 || display != "auto" || display_required {
            let sign_display_str;
            if display_negative_sign {
                display_negative_sign = false;
                if value == 0.0 && value_str.is_none() && has_negative {
                    value = -0.0_f64;
                }
                sign_display_str = "auto";
            } else {
                sign_display_str = "never";
            }

            // Get individual parts from NumberFormat
            let nf_parts = format_one_unit_parts(
                value,
                value_str.as_deref(),
                style,
                unit,
                &data.locale,
                &data.numbering_system,
                sign_display_str,
                extra_min_frac,
                extra_max_frac,
                rounding_mode,
                1,
            );

            // Tag each part with the unit name
            let tagged_parts: Vec<(String, String, String)> = nf_parts
                .into_iter()
                .map(|(ptype, pval)| (ptype, pval, nf_unit.to_string()))
                .collect();

            if need_separator {
                if let Some(last) = result_groups.last_mut() {
                    last.push((
                        "literal".to_string(),
                        time_separator.to_string(),
                        String::new(),
                    ));
                    last.extend(tagged_parts);
                }
            } else {
                if style == "2-digit" || style == "numeric" {
                    need_separator = true;
                }
                result_groups.push(tagged_parts);
            }
        }

        if done {
            break;
        }

        i += 1;
    }

    if result_groups.is_empty() {
        return Vec::new();
    }

    let list_style = if data.style == "digital" {
        "short"
    } else {
        &data.style
    };

    if result_groups.len() == 1 {
        return result_groups.into_iter().next().unwrap();
    }

    // Build string values for list format
    let strings: Vec<String> = result_groups
        .iter()
        .map(|parts| parts.iter().map(|(_, v, _)| v.as_str()).collect::<String>())
        .collect();

    let formatter = create_list_formatter(&data.locale, "unit", list_style);

    // Use placeholder approach to find separator positions
    let placeholder_prefix = "\x01\x02";
    let placeholder_suffix = "\x03\x04";
    let placeholders: Vec<String> = (0..strings.len())
        .map(|i| format!("{}{}{}", placeholder_prefix, i, placeholder_suffix))
        .collect();

    let formatted_with_ph = formatter.format_to_string(placeholders.iter().map(|s| s.as_str()));

    let mut parts: Vec<(String, String, String)> = Vec::new();
    let mut remaining = formatted_with_ph.as_str();

    for (idx, group) in result_groups.iter().enumerate() {
        let ph = &placeholders[idx];
        if let Some(pos) = remaining.find(ph.as_str()) {
            if pos > 0 {
                parts.push((
                    "literal".to_string(),
                    remaining[..pos].to_string(),
                    String::new(),
                ));
            }
            for part in group {
                parts.push(part.clone());
            }
            remaining = &remaining[pos + ph.len()..];
        }
    }
    if !remaining.is_empty() {
        parts.push(("literal".to_string(), remaining.to_string(), String::new()));
    }

    parts
}

impl Interpreter {
    pub(crate) fn setup_intl_duration_format(&mut self, intl_obj: &Rc<RefCell<JsObjectData>>) {
        let proto = self.create_object();
        if let Some(ref op) = self.object_prototype {
            proto.borrow_mut().prototype = Some(op.clone());
        }
        proto.borrow_mut().class_name = "Intl.DurationFormat".to_string();

        // @@toStringTag
        proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("Intl.DurationFormat"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            },
        );

        // format(duration)
        let format_fn = self.create_function(JsFunction::native(
            "format".to_string(),
            1,
            |interp, this, args| {
                let data = match extract_duration_format_data(interp, this) {
                    Ok(d) => d,
                    Err(e) => return Completion::Throw(e),
                };

                let dur_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let dur = match to_duration_record(interp, &dur_arg) {
                    Ok(d) => d,
                    Err(e) => return Completion::Throw(e),
                };

                let result = format_duration(&data, &dur);
                Completion::Normal(JsValue::String(JsString::from_str(&result)))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("format".to_string(), format_fn);

        // formatToParts(duration)
        let format_to_parts_fn = self.create_function(JsFunction::native(
            "formatToParts".to_string(),
            1,
            |interp, this, args| {
                let data = match extract_duration_format_data(interp, this) {
                    Ok(d) => d,
                    Err(e) => return Completion::Throw(e),
                };

                let dur_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let dur = match to_duration_record(interp, &dur_arg) {
                    Ok(d) => d,
                    Err(e) => return Completion::Throw(e),
                };

                let parts = format_to_parts_duration(&data, &dur);

                let js_parts: Vec<JsValue> = parts
                    .into_iter()
                    .map(|(ptype, value, unit)| {
                        let part_obj = interp.create_object();
                        if let Some(ref op) = interp.object_prototype {
                            part_obj.borrow_mut().prototype = Some(op.clone());
                        }
                        part_obj.borrow_mut().insert_property(
                            "type".to_string(),
                            PropertyDescriptor::data(
                                JsValue::String(JsString::from_str(&ptype)),
                                true,
                                true,
                                true,
                            ),
                        );
                        part_obj.borrow_mut().insert_property(
                            "value".to_string(),
                            PropertyDescriptor::data(
                                JsValue::String(JsString::from_str(&value)),
                                true,
                                true,
                                true,
                            ),
                        );
                        if !unit.is_empty() {
                            part_obj.borrow_mut().insert_property(
                                "unit".to_string(),
                                PropertyDescriptor::data(
                                    JsValue::String(JsString::from_str(&unit)),
                                    true,
                                    true,
                                    true,
                                ),
                            );
                        }
                        let id = part_obj.borrow().id.unwrap();
                        JsValue::Object(crate::types::JsObject { id })
                    })
                    .collect();

                Completion::Normal(interp.create_array(js_parts))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("formatToParts".to_string(), format_to_parts_fn);

        // resolvedOptions()
        let resolved_fn = self.create_function(JsFunction::native(
            "resolvedOptions".to_string(),
            0,
            |interp, this, _args| {
                let data = match extract_duration_format_data(interp, this) {
                    Ok(d) => d,
                    Err(e) => return Completion::Throw(e),
                };

                let result = interp.create_object();
                if let Some(ref op) = interp.object_prototype {
                    result.borrow_mut().prototype = Some(op.clone());
                }

                let mut props: Vec<(&str, JsValue)> = vec![
                    ("locale", JsValue::String(JsString::from_str(&data.locale))),
                    (
                        "numberingSystem",
                        JsValue::String(JsString::from_str(&data.numbering_system)),
                    ),
                    ("style", JsValue::String(JsString::from_str(&data.style))),
                    ("years", JsValue::String(JsString::from_str(&data.years))),
                    (
                        "yearsDisplay",
                        JsValue::String(JsString::from_str(&data.years_display)),
                    ),
                    ("months", JsValue::String(JsString::from_str(&data.months))),
                    (
                        "monthsDisplay",
                        JsValue::String(JsString::from_str(&data.months_display)),
                    ),
                    ("weeks", JsValue::String(JsString::from_str(&data.weeks))),
                    (
                        "weeksDisplay",
                        JsValue::String(JsString::from_str(&data.weeks_display)),
                    ),
                    ("days", JsValue::String(JsString::from_str(&data.days))),
                    (
                        "daysDisplay",
                        JsValue::String(JsString::from_str(&data.days_display)),
                    ),
                    ("hours", JsValue::String(JsString::from_str(&data.hours))),
                    (
                        "hoursDisplay",
                        JsValue::String(JsString::from_str(&data.hours_display)),
                    ),
                    (
                        "minutes",
                        JsValue::String(JsString::from_str(&data.minutes)),
                    ),
                    (
                        "minutesDisplay",
                        JsValue::String(JsString::from_str(&data.minutes_display)),
                    ),
                    (
                        "seconds",
                        JsValue::String(JsString::from_str(&data.seconds)),
                    ),
                    (
                        "secondsDisplay",
                        JsValue::String(JsString::from_str(&data.seconds_display)),
                    ),
                    (
                        "milliseconds",
                        JsValue::String(JsString::from_str(&data.milliseconds)),
                    ),
                    (
                        "millisecondsDisplay",
                        JsValue::String(JsString::from_str(&data.milliseconds_display)),
                    ),
                    (
                        "microseconds",
                        JsValue::String(JsString::from_str(&data.microseconds)),
                    ),
                    (
                        "microsecondsDisplay",
                        JsValue::String(JsString::from_str(&data.microseconds_display)),
                    ),
                    (
                        "nanoseconds",
                        JsValue::String(JsString::from_str(&data.nanoseconds)),
                    ),
                    (
                        "nanosecondsDisplay",
                        JsValue::String(JsString::from_str(&data.nanoseconds_display)),
                    ),
                ];

                if let Some(fd) = data.fractional_digits {
                    props.push(("fractionalDigits", JsValue::Number(fd as f64)));
                }

                for (key, val) in props {
                    result.borrow_mut().insert_property(
                        key.to_string(),
                        PropertyDescriptor::data(val, true, true, true),
                    );
                }

                let result_id = result.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id: result_id }))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("resolvedOptions".to_string(), resolved_fn);

        self.intl_duration_format_prototype = Some(proto.clone());

        // --- Constructor ---
        let proto_id = proto.borrow().id.unwrap();
        let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });
        let proto_clone = proto.clone();

        let duration_format_ctor = self.create_function(JsFunction::constructor(
            "DurationFormat".to_string(),
            0,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Constructor Intl.DurationFormat requires 'new'"),
                    );
                }

                let locales_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let options_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);

                let requested = match interp.intl_canonicalize_locale_list(&locales_arg) {
                    Ok(list) => list,
                    Err(e) => return Completion::Throw(e),
                };

                let options = match interp.intl_get_options_object(&options_arg) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let _locale_matcher = match interp.intl_get_option(
                    &options,
                    "localeMatcher",
                    &["lookup", "best fit"],
                    Some("best fit"),
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // Read numberingSystem
                let numbering_system_opt =
                    match interp.intl_get_option(&options, "numberingSystem", &[], None) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };

                // Validate numberingSystem if provided
                if let Some(ref ns) = numbering_system_opt {
                    if !is_valid_numbering_system(ns) {
                        return Completion::Throw(
                            interp.create_range_error(&format!("Invalid numberingSystem: {}", ns)),
                        );
                    }
                }

                // Read style
                let style = match interp.intl_get_option(
                    &options,
                    "style",
                    &["long", "short", "narrow", "digital"],
                    Some("short"),
                ) {
                    Ok(Some(v)) => v,
                    Ok(None) => "short".to_string(),
                    Err(e) => return Completion::Throw(e),
                };

                // Determine per-unit styles following the GetDurationUnitOptions algorithm
                let mut prev_style = String::new();

                // Date-like units: years, months, weeks, days
                let date_units = ["years", "months", "weeks", "days"];
                let mut unit_styles: Vec<(String, String)> = Vec::new();

                for unit in &date_units {
                    let digital_base = "short";
                    let valid = &["long", "short", "narrow"];

                    // GetDurationUnitOptions: compute style and displayDefault
                    let explicit_style = match interp.intl_get_option(&options, unit, valid, None) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };

                    let (unit_style, display_default);
                    if let Some(s) = explicit_style {
                        // User provided explicit style; displayDefault stays "always"
                        display_default = "always";
                        unit_style = s;
                        // Validate: if prevStyle is numeric/2-digit, unit style must also be numeric/2-digit
                        if (prev_style == "numeric"
                            || prev_style == "2-digit"
                            || prev_style == "fractional")
                            && unit_style != "numeric"
                            && unit_style != "2-digit"
                        {
                            return Completion::Throw(interp.create_range_error(&format!(
                                "{} style must be numeric or 2-digit when following a numeric unit",
                                unit
                            )));
                        }
                    } else if style == "digital" {
                        unit_style = digital_base.to_string();
                        // Date-like units are not hours/minutes/seconds
                        display_default = "auto";
                    } else if prev_style == "numeric"
                        || prev_style == "2-digit"
                        || prev_style == "fractional"
                    {
                        return Completion::Throw(interp.create_range_error(&format!(
                            "Cannot use non-numeric style for {} after numeric unit",
                            unit
                        )));
                    } else {
                        unit_style = style.clone();
                        display_default = "auto";
                    };

                    let display_name = format!("{}Display", unit);
                    let unit_display = match interp.intl_get_option(
                        &options,
                        &display_name,
                        &["auto", "always"],
                        Some(display_default),
                    ) {
                        Ok(Some(v)) => v,
                        Ok(None) => display_default.to_string(),
                        Err(e) => return Completion::Throw(e),
                    };

                    prev_style = unit_style.clone();
                    unit_styles.push((unit_style, unit_display));
                }

                // Time-like units: hours, minutes, seconds
                let time_units = ["hours", "minutes", "seconds"];
                for unit in &time_units {
                    let digital_base = "numeric";
                    let valid = &["long", "short", "narrow", "numeric", "2-digit"];

                    let explicit_style = match interp.intl_get_option(&options, unit, valid, None) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };

                    let (mut unit_style, mut display_default);
                    if let Some(s) = explicit_style {
                        // User provided explicit style; displayDefault stays "always"
                        display_default = "always";
                        unit_style = s;
                        // Validate: if prevStyle is numeric/2-digit, unit style must be numeric/2-digit
                        if (prev_style == "numeric"
                            || prev_style == "2-digit"
                            || prev_style == "fractional")
                            && unit_style != "numeric"
                            && unit_style != "2-digit"
                        {
                            return Completion::Throw(interp.create_range_error(&format!(
                                "{} style must be numeric or 2-digit when following a numeric unit",
                                unit
                            )));
                        }
                    } else if style == "digital" {
                        unit_style = digital_base.to_string();
                        // hours/minutes/seconds in digital: displayDefault stays "always"
                        display_default = "always";
                    } else if prev_style == "numeric"
                        || prev_style == "2-digit"
                        || prev_style == "fractional"
                    {
                        unit_style = "numeric".to_string();
                        // minutes/seconds keep "always", hours gets "auto"
                        if *unit == "minutes" || *unit == "seconds" {
                            display_default = "always";
                        } else {
                            display_default = "auto";
                        }
                    } else {
                        unit_style = style.clone();
                        display_default = "auto";
                    };

                    // If style is "numeric" and unit is a fractional second unit, set to "fractional"
                    // (not applicable for hours/minutes/seconds, but kept for completeness)

                    // Force 2-digit for minutes/seconds when following numeric/2-digit
                    if (prev_style == "numeric"
                        || prev_style == "2-digit"
                        || prev_style == "fractional")
                        && (*unit == "minutes" || *unit == "seconds")
                        && unit_style == "numeric"
                    {
                        unit_style = "2-digit".to_string();
                    }

                    let display_name = format!("{}Display", unit);
                    let unit_display = match interp.intl_get_option(
                        &options,
                        &display_name,
                        &["auto", "always"],
                        Some(display_default),
                    ) {
                        Ok(Some(v)) => v,
                        Ok(None) => display_default.to_string(),
                        Err(e) => return Completion::Throw(e),
                    };

                    prev_style = unit_style.clone();
                    unit_styles.push((unit_style, unit_display));
                }

                // Sub-second units: milliseconds, microseconds, nanoseconds
                let subsec_units = ["milliseconds", "microseconds", "nanoseconds"];
                for unit in &subsec_units {
                    let digital_base = "numeric";
                    let valid = &["long", "short", "narrow", "numeric"];

                    let explicit_style = match interp.intl_get_option(&options, unit, valid, None) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };

                    let (mut unit_style, mut display_default);
                    if let Some(s) = explicit_style {
                        display_default = "always";
                        unit_style = s;
                        // Validate
                        if (prev_style == "numeric"
                            || prev_style == "2-digit"
                            || prev_style == "fractional")
                            && unit_style != "numeric"
                            && unit_style != "2-digit"
                        {
                            return Completion::Throw(interp.create_range_error(&format!(
                                "{} style must be numeric when following a numeric unit",
                                unit
                            )));
                        }
                    } else if style == "digital" {
                        unit_style = digital_base.to_string();
                        // Sub-second units are not hours/minutes/seconds
                        display_default = "auto";
                    } else if prev_style == "numeric"
                        || prev_style == "2-digit"
                        || prev_style == "fractional"
                    {
                        unit_style = "numeric".to_string();
                        // Sub-second units are not minutes/seconds
                        display_default = "auto";
                    } else {
                        unit_style = style.clone();
                        display_default = "auto";
                    };

                    // Per spec: if style is "numeric" and this is a fractional second unit,
                    // conceptually style becomes "fractional" and displayDefault = "auto".
                    // We keep "numeric" in storage for the format algorithm but track
                    // prevStyle as "fractional" for cascade purposes.
                    let mut is_fractional = false;
                    if unit_style == "numeric" {
                        is_fractional = true;
                        display_default = "auto";
                    }

                    let display_name = format!("{}Display", unit);
                    let unit_display = match interp.intl_get_option(
                        &options,
                        &display_name,
                        &["auto", "always"],
                        Some(display_default),
                    ) {
                        Ok(Some(v)) => v,
                        Ok(None) => display_default.to_string(),
                        Err(e) => return Completion::Throw(e),
                    };

                    // Track prevStyle as "fractional" for cascade, store "numeric" for format
                    prev_style = if is_fractional {
                        "fractional".to_string()
                    } else {
                        unit_style.clone()
                    };
                    unit_styles.push((unit_style, unit_display));
                }

                // Read fractionalDigits
                let fractional_digits = match interp.intl_get_number_option(
                    &options,
                    "fractionalDigits",
                    0.0,
                    9.0,
                    None,
                ) {
                    Ok(v) => v.map(|f| f as u32),
                    Err(e) => return Completion::Throw(e),
                };

                let raw_locale = interp.intl_resolve_locale(&requested);

                let valid_opt_nu = numbering_system_opt.filter(|nu| is_known_numbering_system(nu));

                let ext_nu = extract_unicode_extension(&raw_locale, "nu");
                let valid_ext_nu = ext_nu.filter(|nu| is_known_numbering_system(nu));
                let nu_from_option = valid_opt_nu.is_some();
                let numbering_system = valid_opt_nu
                    .or(valid_ext_nu.clone())
                    .unwrap_or_else(|| "latn".to_string());

                let base = base_locale(&raw_locale);
                let ext_nu_raw = extract_unicode_extension(&raw_locale, "nu");
                let locale = if nu_from_option {
                    if ext_nu_raw.as_deref() == Some(&*numbering_system) {
                        format!("{}-u-nu-{}", base, numbering_system)
                    } else {
                        base.clone()
                    }
                } else if valid_ext_nu.is_some() {
                    format!("{}-u-nu-{}", base, numbering_system)
                } else {
                    base.clone()
                };

                let obj = interp.create_object();
                obj.borrow_mut().prototype = Some(proto_clone.clone());
                obj.borrow_mut().class_name = "Intl.DurationFormat".to_string();
                obj.borrow_mut().intl_data = Some(IntlData::DurationFormat {
                    locale,
                    numbering_system,
                    style,
                    years: unit_styles[0].0.clone(),
                    years_display: unit_styles[0].1.clone(),
                    months: unit_styles[1].0.clone(),
                    months_display: unit_styles[1].1.clone(),
                    weeks: unit_styles[2].0.clone(),
                    weeks_display: unit_styles[2].1.clone(),
                    days: unit_styles[3].0.clone(),
                    days_display: unit_styles[3].1.clone(),
                    hours: unit_styles[4].0.clone(),
                    hours_display: unit_styles[4].1.clone(),
                    minutes: unit_styles[5].0.clone(),
                    minutes_display: unit_styles[5].1.clone(),
                    seconds: unit_styles[6].0.clone(),
                    seconds_display: unit_styles[6].1.clone(),
                    milliseconds: unit_styles[7].0.clone(),
                    milliseconds_display: unit_styles[7].1.clone(),
                    microseconds: unit_styles[8].0.clone(),
                    microseconds_display: unit_styles[8].1.clone(),
                    nanoseconds: unit_styles[9].0.clone(),
                    nanoseconds_display: unit_styles[9].1.clone(),
                    fractional_digits,
                });

                let obj_id = obj.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id: obj_id }))
            },
        ));

        // Set DurationFormat.prototype on constructor
        if let JsValue::Object(ctor_ref) = &duration_format_ctor {
            if let Some(obj) = self.get_object(ctor_ref.id) {
                obj.borrow_mut().insert_property(
                    "prototype".to_string(),
                    PropertyDescriptor::data(proto_val.clone(), false, false, false),
                );

                // supportedLocalesOf static method
                let slof = self.create_function(JsFunction::native(
                    "supportedLocalesOf".to_string(),
                    1,
                    |interp, _this, args| {
                        let locales = args.first().unwrap_or(&JsValue::Undefined);
                        let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                        let requested = match interp.intl_canonicalize_locale_list(locales) {
                            Ok(list) => list,
                            Err(e) => return Completion::Throw(e),
                        };
                        match interp.intl_supported_locales(&requested, &options) {
                            Ok(v) => Completion::Normal(v),
                            Err(e) => Completion::Throw(e),
                        }
                    },
                ));
                obj.borrow_mut()
                    .insert_builtin("supportedLocalesOf".to_string(), slof);
            }
        }

        // Set constructor on prototype
        proto.borrow_mut().insert_property(
            "constructor".to_string(),
            PropertyDescriptor::data(duration_format_ctor.clone(), true, false, true),
        );

        // Save built-in constructor for internal use (e.g. Duration.toLocaleString)
        self.intl_duration_format_ctor = Some(duration_format_ctor.clone());

        // Register Intl.DurationFormat on the Intl namespace
        intl_obj.borrow_mut().insert_property(
            "DurationFormat".to_string(),
            PropertyDescriptor::data(duration_format_ctor, true, false, true),
        );
    }
}
