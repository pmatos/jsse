use super::super::super::*;
use fixed_decimal::{Decimal, FloatPrecision};
use icu::experimental::relativetime::{
    RelativeTimeFormatter, RelativeTimeFormatterOptions,
    options::Numeric,
};
use icu::locale::Locale as IcuLocale;

fn is_valid_numbering_system(ns: &str) -> bool {
    // UTS 35 type sequence: (3*8alphanum) *("-" (3*8alphanum))
    if ns.is_empty() {
        return false;
    }
    for part in ns.split('-') {
        let len = part.len();
        if len < 3 || len > 8 {
            return false;
        }
        if !part.chars().all(|c| c.is_ascii_alphanumeric()) {
            return false;
        }
    }
    true
}

fn validate_rtf_unit(unit: &str) -> Option<&'static str> {
    match unit {
        "year" | "years" => Some("year"),
        "quarter" | "quarters" => Some("quarter"),
        "month" | "months" => Some("month"),
        "week" | "weeks" => Some("week"),
        "day" | "days" => Some("day"),
        "hour" | "hours" => Some("hour"),
        "minute" | "minutes" => Some("minute"),
        "second" | "seconds" => Some("second"),
        _ => None,
    }
}

fn create_rtf(
    locale_str: &str,
    style: &str,
    unit: &str,
    numeric: &str,
) -> Option<RelativeTimeFormatter> {
    let locale: IcuLocale = locale_str.parse().unwrap_or_else(|_| "en".parse().unwrap());
    let prefs = (&locale).into();
    let opts = RelativeTimeFormatterOptions {
        numeric: match numeric {
            "auto" => Numeric::Auto,
            _ => Numeric::Always,
        },
    };

    let result = match (style, unit) {
        ("long", "second") => RelativeTimeFormatter::try_new_long_second(prefs, opts),
        ("long", "minute") => RelativeTimeFormatter::try_new_long_minute(prefs, opts),
        ("long", "hour") => RelativeTimeFormatter::try_new_long_hour(prefs, opts),
        ("long", "day") => RelativeTimeFormatter::try_new_long_day(prefs, opts),
        ("long", "week") => RelativeTimeFormatter::try_new_long_week(prefs, opts),
        ("long", "month") => RelativeTimeFormatter::try_new_long_month(prefs, opts),
        ("long", "quarter") => RelativeTimeFormatter::try_new_long_quarter(prefs, opts),
        ("long", "year") => RelativeTimeFormatter::try_new_long_year(prefs, opts),
        ("short", "second") => RelativeTimeFormatter::try_new_short_second(prefs, opts),
        ("short", "minute") => RelativeTimeFormatter::try_new_short_minute(prefs, opts),
        ("short", "hour") => RelativeTimeFormatter::try_new_short_hour(prefs, opts),
        ("short", "day") => RelativeTimeFormatter::try_new_short_day(prefs, opts),
        ("short", "week") => RelativeTimeFormatter::try_new_short_week(prefs, opts),
        ("short", "month") => RelativeTimeFormatter::try_new_short_month(prefs, opts),
        ("short", "quarter") => RelativeTimeFormatter::try_new_short_quarter(prefs, opts),
        ("short", "year") => RelativeTimeFormatter::try_new_short_year(prefs, opts),
        ("narrow", "second") => RelativeTimeFormatter::try_new_narrow_second(prefs, opts),
        ("narrow", "minute") => RelativeTimeFormatter::try_new_narrow_minute(prefs, opts),
        ("narrow", "hour") => RelativeTimeFormatter::try_new_narrow_hour(prefs, opts),
        ("narrow", "day") => RelativeTimeFormatter::try_new_narrow_day(prefs, opts),
        ("narrow", "week") => RelativeTimeFormatter::try_new_narrow_week(prefs, opts),
        ("narrow", "month") => RelativeTimeFormatter::try_new_narrow_month(prefs, opts),
        ("narrow", "quarter") => RelativeTimeFormatter::try_new_narrow_quarter(prefs, opts),
        ("narrow", "year") => RelativeTimeFormatter::try_new_narrow_year(prefs, opts),
        _ => return None,
    };

    result.ok().or_else(|| {
        let fallback: IcuLocale = "en".parse().unwrap();
        let fallback_prefs = (&fallback).into();
        let r2 = match (style, unit) {
            ("long", "second") => RelativeTimeFormatter::try_new_long_second(fallback_prefs, opts),
            ("long", "minute") => RelativeTimeFormatter::try_new_long_minute(fallback_prefs, opts),
            ("long", "hour") => RelativeTimeFormatter::try_new_long_hour(fallback_prefs, opts),
            ("long", "day") => RelativeTimeFormatter::try_new_long_day(fallback_prefs, opts),
            ("long", "week") => RelativeTimeFormatter::try_new_long_week(fallback_prefs, opts),
            ("long", "month") => RelativeTimeFormatter::try_new_long_month(fallback_prefs, opts),
            ("long", "quarter") => {
                RelativeTimeFormatter::try_new_long_quarter(fallback_prefs, opts)
            }
            ("long", "year") => RelativeTimeFormatter::try_new_long_year(fallback_prefs, opts),
            ("short", "second") => {
                RelativeTimeFormatter::try_new_short_second(fallback_prefs, opts)
            }
            ("short", "minute") => {
                RelativeTimeFormatter::try_new_short_minute(fallback_prefs, opts)
            }
            ("short", "hour") => RelativeTimeFormatter::try_new_short_hour(fallback_prefs, opts),
            ("short", "day") => RelativeTimeFormatter::try_new_short_day(fallback_prefs, opts),
            ("short", "week") => RelativeTimeFormatter::try_new_short_week(fallback_prefs, opts),
            ("short", "month") => RelativeTimeFormatter::try_new_short_month(fallback_prefs, opts),
            ("short", "quarter") => {
                RelativeTimeFormatter::try_new_short_quarter(fallback_prefs, opts)
            }
            ("short", "year") => RelativeTimeFormatter::try_new_short_year(fallback_prefs, opts),
            ("narrow", "second") => {
                RelativeTimeFormatter::try_new_narrow_second(fallback_prefs, opts)
            }
            ("narrow", "minute") => {
                RelativeTimeFormatter::try_new_narrow_minute(fallback_prefs, opts)
            }
            ("narrow", "hour") => RelativeTimeFormatter::try_new_narrow_hour(fallback_prefs, opts),
            ("narrow", "day") => RelativeTimeFormatter::try_new_narrow_day(fallback_prefs, opts),
            ("narrow", "week") => RelativeTimeFormatter::try_new_narrow_week(fallback_prefs, opts),
            ("narrow", "month") => {
                RelativeTimeFormatter::try_new_narrow_month(fallback_prefs, opts)
            }
            ("narrow", "quarter") => {
                RelativeTimeFormatter::try_new_narrow_quarter(fallback_prefs, opts)
            }
            ("narrow", "year") => RelativeTimeFormatter::try_new_narrow_year(fallback_prefs, opts),
            _ => return None,
        };
        r2.ok()
    })
}

fn f64_to_decimal(value: f64) -> Decimal {
    if value.is_sign_negative() && value == 0.0 {
        // Negative zero: create 0 with negative sign
        let mut d = Decimal::from(0i32);
        d.sign = fixed_decimal::Sign::Negative;
        return d;
    }
    if value == value.trunc() && value.abs() < 1e15 {
        let int_val = value as i64;
        Decimal::from(int_val)
    } else {
        Decimal::try_from_f64(value, FloatPrecision::RoundTrip)
            .unwrap_or_else(|_| Decimal::from(value as i64))
    }
}

fn format_number_with_decimal_formatter(locale_str: &str, value: f64) -> String {
    let locale: IcuLocale = locale_str.parse().unwrap_or_else(|_| "en".parse().unwrap());
    let prefs: icu::decimal::DecimalFormatterPreferences = (&locale).into();
    let dec_formatter = icu::decimal::DecimalFormatter::try_new(
        prefs,
        icu::decimal::options::DecimalFormatterOptions::default(),
    )
    .unwrap_or_else(|_| {
        let fallback: IcuLocale = "en".parse().unwrap();
        let fb_prefs = (&fallback).into();
        icu::decimal::DecimalFormatter::try_new(
            fb_prefs,
            icu::decimal::options::DecimalFormatterOptions::default(),
        )
        .unwrap()
    });
    let decimal = f64_to_decimal(value);
    dec_formatter.format(&decimal).to_string()
}

fn locale_without_nu(locale_str: &str) -> String {
    if let Some(nu_pos) = locale_str.find("-nu-") {
        let before = &locale_str[..nu_pos];
        let after_nu = &locale_str[nu_pos + 4..];
        let rest = after_nu.find('-').map(|p| &after_nu[p..]).unwrap_or("");
        let result = format!("{}{}", before, rest);
        result.trim_end_matches("-u").to_string()
    } else {
        locale_str.to_string()
    }
}

fn replace_number_in_rtf_output(
    locale_str: &str,
    numbering_system: &str,
    style: &str,
    unit: &str,
    numeric: &str,
    value: f64,
) -> String {
    // Format using RTF with base locale (no nu extension) to get latn output
    let base_locale = locale_without_nu(locale_str);
    let formatter = match create_rtf(&base_locale, style, unit, numeric) {
        Some(f) => f,
        None => {
            let f = create_rtf(locale_str, style, unit, numeric).unwrap();
            let decimal = f64_to_decimal(value);
            return f.format(decimal).to_string();
        }
    };
    let decimal = f64_to_decimal(value);
    let base_result = formatter.format(decimal).to_string();

    if numbering_system == "latn" || numbering_system.is_empty() {
        return base_result;
    }

    // Format the absolute value as latn using DecimalFormatter
    let base_number = format_number_with_decimal_formatter(&base_locale, value.abs());

    // Find the latn number in the RTF output and replace with transliterated version
    if let Some(pos) = base_result.find(&base_number) {
        let transliterated = super::numberformat::transliterate_digits(&base_number, numbering_system);
        let mut result = String::new();
        result.push_str(&base_result[..pos]);
        result.push_str(&transliterated);
        result.push_str(&base_result[pos + base_number.len()..]);
        result
    } else {
        base_result
    }
}

fn detect_locale_separators(locale_str: &str) -> (String, String) {
    let locale: IcuLocale = locale_str.parse().unwrap_or_else(|_| "en".parse().unwrap());
    let prefs: icu::decimal::DecimalFormatterPreferences = (&locale).into();
    let dec_formatter = icu::decimal::DecimalFormatter::try_new(
        prefs,
        icu::decimal::options::DecimalFormatterOptions::default(),
    )
    .unwrap_or_else(|_| {
        let fallback: IcuLocale = "en".parse().unwrap();
        let fb_prefs = (&fallback).into();
        icu::decimal::DecimalFormatter::try_new(
            fb_prefs,
            icu::decimal::options::DecimalFormatterOptions::default(),
        )
        .unwrap()
    });

    // Format a number with both group and decimal separators to detect them
    let test_val = Decimal::try_from_f64(12345.6, FloatPrecision::RoundTrip)
        .unwrap_or_else(|_| Decimal::from(12345i64));
    let test_formatted = dec_formatter.format(&test_val).to_string();

    // Parse the formatted number to find the group and decimal separators
    // Expected format: "12{group}345{decimal}6"
    let mut group_sep = ",".to_string();
    let mut decimal_sep = ".".to_string();

    // Find what's between the "2" and "3" (group separator)
    // Find what's between the "5" and "6" (decimal separator)
    let chars: Vec<char> = test_formatted.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c == '2' {
            // Collect chars until we hit '3'
            let mut sep = String::new();
            for j in (i + 1)..chars.len() {
                if chars[j] == '3' {
                    break;
                }
                sep.push(chars[j]);
            }
            if !sep.is_empty() {
                group_sep = sep;
            }
        }
        if c == '5' {
            // Collect chars until we hit '6'
            let mut sep = String::new();
            for j in (i + 1)..chars.len() {
                if chars[j] == '6' {
                    break;
                }
                sep.push(chars[j]);
            }
            if !sep.is_empty() {
                decimal_sep = sep;
            }
        }
    }

    (group_sep, decimal_sep)
}

fn format_rtf_to_parts_data(
    formatted: &str,
    value: f64,
    unit: &str,
    numeric_opt: &str,
    locale_str: &str,
) -> Vec<(String, String, Option<String>)> {
    if numeric_opt == "auto" {
        let abs_val = value.abs();
        if abs_val <= 1.0 && value == value.trunc() {
            let has_digit = formatted.chars().any(|c| c.is_ascii_digit());
            if !has_digit {
                return vec![("literal".to_string(), formatted.to_string(), None)];
            }
        }
    }

    let (group_sep, decimal_sep) = detect_locale_separators(locale_str);

    // Find the number portion within the formatted string
    let first_digit = formatted.find(|c: char| c.is_ascii_digit());
    if first_digit.is_none() {
        return vec![("literal".to_string(), formatted.to_string(), None)];
    }
    let start = first_digit.unwrap();

    // Find end of the number (digits, group separators, and decimal separators)
    let mut end = start;
    let remaining = &formatted[start..];
    let mut pos = 0;
    let remaining_chars: Vec<char> = remaining.chars().collect();
    while pos < remaining_chars.len() {
        let c = remaining_chars[pos];
        if c.is_ascii_digit() {
            end = start + remaining[..].char_indices()
                .nth(pos).map(|(i, c)| i + c.len_utf8()).unwrap_or(end);
            pos += 1;
        } else {
            // Check if this is a group or decimal separator
            let rest_str: String = remaining_chars[pos..].iter().collect();
            if rest_str.starts_with(&group_sep) {
                // Verify it's followed by a digit (group separator)
                let sep_chars = group_sep.chars().count();
                if pos + sep_chars < remaining_chars.len()
                    && remaining_chars[pos + sep_chars].is_ascii_digit()
                {
                    let byte_end = remaining[..].char_indices()
                        .nth(pos + sep_chars).map(|(i, c)| i + c.len_utf8())
                        .unwrap_or(remaining.len());
                    end = start + byte_end;
                    pos += sep_chars;
                    continue;
                }
            }
            if rest_str.starts_with(&decimal_sep) {
                let sep_chars = decimal_sep.chars().count();
                if pos + sep_chars < remaining_chars.len()
                    && remaining_chars[pos + sep_chars].is_ascii_digit()
                {
                    // Decimal separator followed by digit
                    let byte_end = remaining[..].char_indices()
                        .nth(pos + sep_chars).map(|(i, c)| i + c.len_utf8())
                        .unwrap_or(remaining.len());
                    end = start + byte_end;
                    pos += sep_chars;
                    continue;
                }
            }
            break;
        }
    }

    let mut parts: Vec<(String, String, Option<String>)> = Vec::new();

    if start > 0 {
        parts.push(("literal".to_string(), formatted[..start].to_string(), None));
    }

    let num_str = &formatted[start..end];
    let number_parts = parse_number_to_parts_locale(num_str, unit, &group_sep, &decimal_sep);
    parts.extend(number_parts);

    if end < formatted.len() {
        parts.push(("literal".to_string(), formatted[end..].to_string(), None));
    }

    parts
}

fn parse_number_to_parts_locale(
    number_str: &str,
    unit: &str,
    group_sep: &str,
    decimal_sep: &str,
) -> Vec<(String, String, Option<String>)> {
    let mut parts: Vec<(String, String, Option<String>)> = Vec::new();
    let unit_val = Some(unit.to_string());

    // Split on decimal separator first
    if let Some(dec_pos) = number_str.find(decimal_sep) {
        let integer_part = &number_str[..dec_pos];
        parse_integer_groups_locale(integer_part, unit, group_sep, &mut parts);

        parts.push((
            "decimal".to_string(),
            decimal_sep.to_string(),
            unit_val.clone(),
        ));

        let fraction_part = &number_str[dec_pos + decimal_sep.len()..];
        if !fraction_part.is_empty() {
            parts.push((
                "fraction".to_string(),
                fraction_part.to_string(),
                unit_val,
            ));
        }
    } else {
        parse_integer_groups_locale(number_str, unit, group_sep, &mut parts);
    }

    parts
}

fn parse_integer_groups_locale(
    integer_str: &str,
    unit: &str,
    group_sep: &str,
    parts: &mut Vec<(String, String, Option<String>)>,
) {
    let unit_val = Some(unit.to_string());

    if group_sep.is_empty() {
        if !integer_str.is_empty() {
            parts.push(("integer".to_string(), integer_str.to_string(), unit_val));
        }
        return;
    }

    let segments: Vec<&str> = integer_str.split(group_sep).collect();
    for (i, segment) in segments.iter().enumerate() {
        if !segment.is_empty() {
            parts.push(("integer".to_string(), segment.to_string(), unit_val.clone()));
        }
        if i < segments.len() - 1 {
            parts.push(("group".to_string(), group_sep.to_string(), unit_val.clone()));
        }
    }
}

fn extract_rtf_data(
    interp: &mut Interpreter,
    this: &JsValue,
) -> Result<(String, String, String, String), JsValue> {
    if let JsValue::Object(o) = this {
        if let Some(obj) = interp.get_object(o.id) {
            let b = obj.borrow();
            if let Some(IntlData::RelativeTimeFormat {
                ref locale,
                ref style,
                ref numeric,
                ref numbering_system,
            }) = b.intl_data
            {
                return Ok((
                    locale.clone(),
                    style.clone(),
                    numeric.clone(),
                    numbering_system.clone(),
                ));
            }
        }
    }
    Err(interp.create_type_error(
        "Intl.RelativeTimeFormat method called on incompatible receiver",
    ))
}

impl Interpreter {
    pub(crate) fn setup_intl_relative_time_format(
        &mut self,
        intl_obj: &Rc<RefCell<JsObjectData>>,
    ) {
        let proto = self.create_object();
        if let Some(ref op) = self.realm().object_prototype {
            proto.borrow_mut().prototype = Some(op.clone());
        }
        proto.borrow_mut().class_name = "Intl.RelativeTimeFormat".to_string();

        // @@toStringTag
        proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("Intl.RelativeTimeFormat"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            },
        );

        // format(value, unit)
        let format_fn = self.create_function(JsFunction::native(
            "format".to_string(),
            2,
            |interp, this, args| {
                let (locale, style, numeric, ns) = match extract_rtf_data(interp, this) {
                    Ok(data) => data,
                    Err(e) => return Completion::Throw(e),
                };

                let value_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let value = match interp.to_number_value(&value_arg) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                if value.is_nan() || value.is_infinite() {
                    return Completion::Throw(
                        interp.create_range_error("Value must be finite for RelativeTimeFormat.format"),
                    );
                }

                let unit_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let unit_str = match interp.to_string_value(&unit_arg) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };

                let unit = match validate_rtf_unit(&unit_str) {
                    Some(u) => u,
                    None => {
                        return Completion::Throw(interp.create_range_error(&format!(
                            "Invalid unit argument for RelativeTimeFormat: {}",
                            unit_str
                        )));
                    }
                };

                let result = replace_number_in_rtf_output(&locale, &ns, &style, unit, &numeric, value);
                Completion::Normal(JsValue::String(JsString::from_str(&result)))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("format".to_string(), format_fn);

        // formatToParts(value, unit)
        let format_to_parts_fn = self.create_function(JsFunction::native(
            "formatToParts".to_string(),
            2,
            |interp, this, args| {
                let (locale, style, numeric, ns) = match extract_rtf_data(interp, this) {
                    Ok(data) => data,
                    Err(e) => return Completion::Throw(e),
                };

                let value_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let value = match interp.to_number_value(&value_arg) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                if value.is_nan() || value.is_infinite() {
                    return Completion::Throw(
                        interp.create_range_error(
                            "Value must be finite for RelativeTimeFormat.formatToParts",
                        ),
                    );
                }

                let unit_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let unit_str = match interp.to_string_value(&unit_arg) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };

                let unit = match validate_rtf_unit(&unit_str) {
                    Some(u) => u,
                    None => {
                        return Completion::Throw(interp.create_range_error(&format!(
                            "Invalid unit argument for RelativeTimeFormat: {}",
                            unit_str
                        )));
                    }
                };

                let formatted = replace_number_in_rtf_output(&locale, &ns, &style, unit, &numeric, value);

                let parts_data =
                    format_rtf_to_parts_data(&formatted, value, unit, &numeric, &locale);

                let js_parts: Vec<JsValue> = parts_data
                    .into_iter()
                    .map(|(ptype, pvalue, punit)| {
                        let part_obj = interp.create_object();
                        if let Some(ref op) = interp.realm().object_prototype {
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
                                JsValue::String(JsString::from_str(&pvalue)),
                                true,
                                true,
                                true,
                            ),
                        );
                        if let Some(u) = punit {
                            part_obj.borrow_mut().insert_property(
                                "unit".to_string(),
                                PropertyDescriptor::data(
                                    JsValue::String(JsString::from_str(&u)),
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
                let (locale, style, numeric, numbering_system) =
                    match extract_rtf_data(interp, this) {
                        Ok(data) => data,
                        Err(e) => return Completion::Throw(e),
                    };

                let result = interp.create_object();
                if let Some(ref op) = interp.realm().object_prototype {
                    result.borrow_mut().prototype = Some(op.clone());
                }

                let props = vec![
                    ("locale", JsValue::String(JsString::from_str(&locale))),
                    ("style", JsValue::String(JsString::from_str(&style))),
                    ("numeric", JsValue::String(JsString::from_str(&numeric))),
                    (
                        "numberingSystem",
                        JsValue::String(JsString::from_str(&numbering_system)),
                    ),
                ];
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

        self.realm_mut().intl_relative_time_format_prototype = Some(proto.clone());

        // --- Constructor ---
        let proto_id = proto.borrow().id.unwrap();
        let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });
        let proto_clone = proto.clone();

        let rtf_ctor = self.create_function(JsFunction::constructor(
            "RelativeTimeFormat".to_string(),
            0,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(interp.create_type_error(
                        "Constructor Intl.RelativeTimeFormat requires 'new'",
                    ));
                }

                let locales_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let options_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);

                let requested = match interp.intl_canonicalize_locale_list(&locales_arg) {
                    Ok(list) => list,
                    Err(e) => return Completion::Throw(e),
                };

                let options = match interp.intl_coerce_options_to_object(&options_arg) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // Step 7: localeMatcher
                let _locale_matcher = match interp.intl_get_option(
                    &options,
                    "localeMatcher",
                    &["lookup", "best fit"],
                    Some("best fit"),
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // Step 8-9: numberingSystem (read before style and numeric per spec)
                let ns_opt = match interp.intl_get_option(
                    &options,
                    "numberingSystem",
                    &[],
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // Validate numberingSystem per UTS 35 type sequence: (3*8alphanum) *("-" (3*8alphanum))
                if let Some(ref ns) = ns_opt {
                    if !is_valid_numbering_system(ns) {
                        return Completion::Throw(interp.create_range_error(&format!(
                            "Invalid numberingSystem value: {}",
                            ns
                        )));
                    }
                }

                // Step 14: style
                let style = match interp.intl_get_option(
                    &options,
                    "style",
                    &["long", "short", "narrow"],
                    Some("long"),
                ) {
                    Ok(Some(v)) => v,
                    Ok(None) => "long".to_string(),
                    Err(e) => return Completion::Throw(e),
                };

                // Step 16: numeric
                let numeric = match interp.intl_get_option(
                    &options,
                    "numeric",
                    &["always", "auto"],
                    Some("always"),
                ) {
                    Ok(Some(v)) => v,
                    Ok(None) => "always".to_string(),
                    Err(e) => return Completion::Throw(e),
                };

                let mut locale = interp.intl_resolve_locale(&requested);

                let known_numbering_systems = [
                    "adlm", "ahom", "arab", "arabext", "bali", "beng", "bhks", "brah",
                    "cakm", "cham", "deva", "diak", "fullwide", "gong", "gonm", "gujr",
                    "guru", "hanidec", "hmng", "hmnp", "java", "kali", "kawi", "khmr",
                    "knda", "lana", "lanatham", "laoo", "latn", "lepc", "limb", "mathbold",
                    "mathdbl", "mathmono", "mathsanb", "mathsans", "mlym", "modi", "mong",
                    "mroo", "mtei", "mymr", "mymrshan", "mymrtlng", "nagm", "newa", "nkoo",
                    "olck", "orya", "osma", "rohg", "saur", "segment", "shrd", "sind",
                    "sinh", "sora", "sund", "takr", "talu", "tamldec", "telu", "thai",
                    "tibt", "tirh", "tnsa", "vaii", "wara", "wcho",
                ];

                // Extract nu extension from locale if present
                let locale_nu = {
                    let tag = locale.clone();
                    tag.find("-nu-").map(|pos| {
                        let rest = &tag[pos + 4..];
                        let end = rest.find('-').unwrap_or(rest.len());
                        rest[..end].to_string()
                    })
                };
                let locale_nu_supported = locale_nu
                    .as_ref()
                    .is_some_and(|nu| known_numbering_systems.contains(&nu.as_str()));

                fn strip_nu_extension(locale: &str) -> String {
                    if let Some(nu_pos) = locale.find("-nu-") {
                        let before = &locale[..nu_pos];
                        let after_nu = &locale[nu_pos + 4..];
                        let rest = after_nu.find('-')
                            .map(|p| &after_nu[p..])
                            .unwrap_or("");
                        let result = format!("{}{}", before, rest);
                        // Clean up trailing -u- if empty
                        result.trim_end_matches("-u").to_string()
                    } else {
                        locale.to_string()
                    }
                }

                let numbering_system;

                if let Some(ref opt_ns) = ns_opt {
                    let opt_supported = known_numbering_systems.contains(&opt_ns.as_str());
                    if opt_supported {
                        numbering_system = opt_ns.clone();
                        // Option overrides extension: strip nu from locale unless they match
                        if let Some(ref loc_nu) = locale_nu {
                            if loc_nu != opt_ns {
                                locale = strip_nu_extension(&locale);
                            }
                        }
                    } else {
                        // Option is unsupported, fall back to locale extension or default
                        if locale_nu_supported {
                            numbering_system = locale_nu.unwrap();
                        } else {
                            numbering_system = "latn".to_string();
                            if locale_nu.is_some() {
                                locale = strip_nu_extension(&locale);
                            }
                        }
                    }
                } else {
                    // No option provided
                    if locale_nu_supported {
                        numbering_system = locale_nu.unwrap();
                    } else {
                        numbering_system = "latn".to_string();
                        if locale_nu.is_some() {
                            locale = strip_nu_extension(&locale);
                        }
                    }
                };

                let obj = interp.create_object();
                obj.borrow_mut().prototype = Some(proto_clone.clone());
                obj.borrow_mut().class_name = "Intl.RelativeTimeFormat".to_string();
                obj.borrow_mut().intl_data = Some(IntlData::RelativeTimeFormat {
                    locale,
                    style,
                    numeric,
                    numbering_system,
                });

                let obj_id = obj.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id: obj_id }))
            },
        ));

        // Set RelativeTimeFormat.prototype on constructor
        if let JsValue::Object(ctor_ref) = &rtf_ctor {
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
            PropertyDescriptor::data(rtf_ctor.clone(), true, false, true),
        );

        // Register Intl.RelativeTimeFormat on the Intl namespace
        intl_obj.borrow_mut().insert_property(
            "RelativeTimeFormat".to_string(),
            PropertyDescriptor::data(rtf_ctor, true, false, true),
        );
    }
}
