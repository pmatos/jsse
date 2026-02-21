use super::super::super::*;
use icu::experimental::displaynames::{
    DisplayNamesOptions as IcuDisplayNamesOptions, LanguageDisplay as IcuLanguageDisplay,
    LocaleDisplayNamesFormatter, RegionDisplayNames, ScriptDisplayNames, Style as IcuStyle,
};
use icu::locale::Locale as IcuLocale;

fn is_valid_language_code(code: &str) -> bool {
    if code.is_empty() || code.starts_with('-') || code.ends_with('-') || code.contains("--") {
        return false;
    }

    // Must not contain underscores
    if code.contains('_') {
        return false;
    }

    let parts: Vec<&str> = code.split('-').collect();
    if parts.is_empty() {
        return false;
    }

    let mut idx = 0;

    // Language subtag: 2-3 alpha or 5-8 alpha
    let lang = parts[idx];
    let lang_len = lang.len();
    if !lang.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    if !((2..=3).contains(&lang_len) || (5..=8).contains(&lang_len)) {
        return false;
    }
    idx += 1;

    if idx >= parts.len() {
        return true;
    }

    // Optional script subtag: exactly 4 alpha
    let mut has_script = false;
    if parts[idx].len() == 4 && parts[idx].chars().all(|c| c.is_ascii_alphabetic()) {
        has_script = true;
        idx += 1;
        if idx >= parts.len() {
            return true;
        }
    }

    // Optional region subtag: 2 alpha or 3 digits
    let mut has_region = false;
    if idx < parts.len() {
        let region = parts[idx];
        if (region.len() == 2 && region.chars().all(|c| c.is_ascii_alphabetic()))
            || (region.len() == 3 && region.chars().all(|c| c.is_ascii_digit()))
        {
            has_region = true;
            idx += 1;
        }
    }

    // Optional variant subtags: 5-8 alphanum, or digit + 3 alphanum
    // No duplicate variants allowed, no singleton subtags
    let mut seen_variants: Vec<String> = Vec::new();
    while idx < parts.len() {
        let subtag = parts[idx];
        let slen = subtag.len();

        // Reject singleton subtags (single character like 'u', 'x', etc.)
        if slen == 1 {
            return false;
        }

        // Reject what would be a second script subtag
        if slen == 4 && subtag.chars().all(|c| c.is_ascii_alphabetic()) {
            if has_script {
                return false;
            }
            return false;
        }

        // Reject what would be a second region subtag
        if (slen == 2 && subtag.chars().all(|c| c.is_ascii_alphabetic()))
            || (slen == 3 && subtag.chars().all(|c| c.is_ascii_digit()))
        {
            if has_region {
                return false;
            }
            return false;
        }

        let all_alnum = subtag.chars().all(|c| c.is_ascii_alphanumeric());
        if (5..=8).contains(&slen) && all_alnum {
            let lower = subtag.to_ascii_lowercase();
            if seen_variants.contains(&lower) {
                return false;
            }
            seen_variants.push(lower);
            idx += 1;
        } else if slen == 4
            && subtag.chars().next().map_or(false, |c| c.is_ascii_digit())
            && subtag[1..].chars().all(|c| c.is_ascii_alphanumeric())
        {
            let lower = subtag.to_ascii_lowercase();
            if seen_variants.contains(&lower) {
                return false;
            }
            seen_variants.push(lower);
            idx += 1;
        } else {
            return false;
        }
    }

    true
}

fn is_valid_region_code(code: &str) -> bool {
    let len = code.len();
    (len == 2 && code.chars().all(|c| c.is_ascii_alphabetic()))
        || (len == 3 && code.chars().all(|c| c.is_ascii_digit()))
}

fn is_valid_script_code(code: &str) -> bool {
    code.len() == 4 && code.chars().all(|c| c.is_ascii_alphabetic())
}

fn is_valid_currency_code(code: &str) -> bool {
    code.len() == 3 && code.chars().all(|c| c.is_ascii_alphabetic())
}

fn is_valid_calendar_code(code: &str) -> bool {
    if code.is_empty() {
        return false;
    }
    for part in code.split('-') {
        let len = part.len();
        if !(3..=8).contains(&len) || !part.chars().all(|c| c.is_ascii_alphanumeric()) {
            return false;
        }
    }
    true
}

fn is_valid_date_time_field(code: &str) -> bool {
    matches!(
        code,
        "era"
            | "year"
            | "quarter"
            | "month"
            | "weekOfYear"
            | "weekday"
            | "day"
            | "dayPeriod"
            | "hour"
            | "minute"
            | "second"
            | "timeZoneName"
    )
}

fn get_currency_display_name(code: &str, style: &str) -> Option<&'static str> {
    let upper = code.to_ascii_uppercase();
    let name = match upper.as_str() {
        "USD" => match style {
            "short" | "narrow" => "USD",
            _ => "US Dollar",
        },
        "EUR" => match style {
            "short" | "narrow" => "EUR",
            _ => "Euro",
        },
        "GBP" => match style {
            "short" | "narrow" => "GBP",
            _ => "British Pound",
        },
        "JPY" => match style {
            "short" | "narrow" => "JPY",
            _ => "Japanese Yen",
        },
        "CNY" => match style {
            "short" | "narrow" => "CNY",
            _ => "Chinese Yuan",
        },
        "KRW" => match style {
            "short" | "narrow" => "KRW",
            _ => "South Korean Won",
        },
        "INR" => match style {
            "short" | "narrow" => "INR",
            _ => "Indian Rupee",
        },
        "CAD" => match style {
            "short" | "narrow" => "CAD",
            _ => "Canadian Dollar",
        },
        "AUD" => match style {
            "short" | "narrow" => "AUD",
            _ => "Australian Dollar",
        },
        "CHF" => match style {
            "short" | "narrow" => "CHF",
            _ => "Swiss Franc",
        },
        "MXN" => match style {
            "short" | "narrow" => "MXN",
            _ => "Mexican Peso",
        },
        "BRL" => match style {
            "short" | "narrow" => "BRL",
            _ => "Brazilian Real",
        },
        "RUB" => match style {
            "short" | "narrow" => "RUB",
            _ => "Russian Ruble",
        },
        "HKD" => match style {
            "short" | "narrow" => "HKD",
            _ => "Hong Kong Dollar",
        },
        "NZD" => match style {
            "short" | "narrow" => "NZD",
            _ => "New Zealand Dollar",
        },
        "SEK" => match style {
            "short" | "narrow" => "SEK",
            _ => "Swedish Krona",
        },
        "NOK" => match style {
            "short" | "narrow" => "NOK",
            _ => "Norwegian Krone",
        },
        "DKK" => match style {
            "short" | "narrow" => "DKK",
            _ => "Danish Krone",
        },
        "SGD" => match style {
            "short" | "narrow" => "SGD",
            _ => "Singapore Dollar",
        },
        "THB" => match style {
            "short" | "narrow" => "THB",
            _ => "Thai Baht",
        },
        "TWD" => match style {
            "short" | "narrow" => "TWD",
            _ => "New Taiwan Dollar",
        },
        "PLN" => match style {
            "short" | "narrow" => "PLN",
            _ => "Polish Zloty",
        },
        "TRY" => match style {
            "short" | "narrow" => "TRY",
            _ => "Turkish Lira",
        },
        "ZAR" => match style {
            "short" | "narrow" => "ZAR",
            _ => "South African Rand",
        },
        "PHP" => match style {
            "short" | "narrow" => "PHP",
            _ => "Philippine Peso",
        },
        "IDR" => match style {
            "short" | "narrow" => "IDR",
            _ => "Indonesian Rupiah",
        },
        "CZK" => match style {
            "short" | "narrow" => "CZK",
            _ => "Czech Koruna",
        },
        "ILS" => match style {
            "short" | "narrow" => "ILS",
            _ => "Israeli New Shekel",
        },
        "CLP" => match style {
            "short" | "narrow" => "CLP",
            _ => "Chilean Peso",
        },
        "ARS" => match style {
            "short" | "narrow" => "ARS",
            _ => "Argentine Peso",
        },
        "COP" => match style {
            "short" | "narrow" => "COP",
            _ => "Colombian Peso",
        },
        "SAR" => match style {
            "short" | "narrow" => "SAR",
            _ => "Saudi Riyal",
        },
        "AED" => match style {
            "short" | "narrow" => "AED",
            _ => "United Arab Emirates Dirham",
        },
        "EGP" => match style {
            "short" | "narrow" => "EGP",
            _ => "Egyptian Pound",
        },
        "MYR" => match style {
            "short" | "narrow" => "MYR",
            _ => "Malaysian Ringgit",
        },
        "VND" => match style {
            "short" | "narrow" => "VND",
            _ => "Vietnamese Dong",
        },
        _ => return None,
    };
    Some(name)
}

fn get_calendar_display_name(code: &str, _style: &str) -> Option<&'static str> {
    match code {
        "gregory" => Some("Gregorian Calendar"),
        "buddhist" => Some("Buddhist Calendar"),
        "chinese" => Some("Chinese Calendar"),
        "coptic" => Some("Coptic Calendar"),
        "dangi" => Some("Dangi Calendar"),
        "ethioaa" => Some("Ethiopic Amete Alem Calendar"),
        "ethiopic" => Some("Ethiopic Calendar"),
        "hebrew" => Some("Hebrew Calendar"),
        "indian" => Some("Indian National Calendar"),
        "islamic" => Some("Islamic Calendar"),
        "islamic-civil" => Some("Islamic Calendar (Tabular, Civil Epoch)"),
        "islamic-rgsa" => Some("Islamic Calendar (Saudi Arabia, Sighting)"),
        "islamic-tbla" => Some("Islamic Calendar (Tabular, Astronomical Epoch)"),
        "islamic-umalqura" => Some("Islamic Calendar (Umm al-Qura)"),
        "iso8601" => Some("ISO-8601 Calendar"),
        "japanese" => Some("Japanese Calendar"),
        "persian" => Some("Persian Calendar"),
        "roc" => Some("Minguo Calendar"),
        _ => None,
    }
}

fn get_date_time_field_display_name(code: &str, style: &str) -> Option<&'static str> {
    match code {
        "era" => match style {
            "short" | "narrow" => Some("era"),
            _ => Some("era"),
        },
        "year" => match style {
            "short" | "narrow" => Some("yr."),
            _ => Some("year"),
        },
        "quarter" => match style {
            "short" | "narrow" => Some("qtr."),
            _ => Some("quarter"),
        },
        "month" => match style {
            "short" | "narrow" => Some("mo."),
            _ => Some("month"),
        },
        "weekOfYear" => match style {
            "short" | "narrow" => Some("wk."),
            _ => Some("week"),
        },
        "weekday" => match style {
            "short" | "narrow" => Some("day of wk."),
            _ => Some("day of the week"),
        },
        "day" => match style {
            "short" | "narrow" => Some("day"),
            _ => Some("day"),
        },
        "dayPeriod" => match style {
            "short" | "narrow" => Some("AM/PM"),
            _ => Some("AM/PM"),
        },
        "hour" => match style {
            "short" | "narrow" => Some("hr."),
            _ => Some("hour"),
        },
        "minute" => match style {
            "short" | "narrow" => Some("min."),
            _ => Some("minute"),
        },
        "second" => match style {
            "short" | "narrow" => Some("sec."),
            _ => Some("second"),
        },
        "timeZoneName" => match style {
            "short" | "narrow" => Some("zone"),
            _ => Some("time zone"),
        },
        _ => None,
    }
}

struct DisplayNamesData {
    locale: String,
    style: String,
    display_type: String,
    fallback: String,
    language_display: Option<String>,
}

fn extract_display_names_data(
    interp: &mut Interpreter,
    this: &JsValue,
) -> Result<DisplayNamesData, JsValue> {
    if let JsValue::Object(o) = this {
        if let Some(obj) = interp.get_object(o.id) {
            let b = obj.borrow();
            if let Some(IntlData::DisplayNames {
                ref locale,
                ref style,
                ref display_type,
                ref fallback,
                ref language_display,
            }) = b.intl_data
            {
                return Ok(DisplayNamesData {
                    locale: locale.clone(),
                    style: style.clone(),
                    display_type: display_type.clone(),
                    fallback: fallback.clone(),
                    language_display: language_display.clone(),
                });
            }
        }
    }
    Err(interp.create_type_error("Intl.DisplayNames method called on incompatible receiver"))
}

fn get_display_name_for_code(
    locale_str: &str,
    display_type: &str,
    style: &str,
    fallback: &str,
    language_display: &Option<String>,
    code: &str,
) -> Result<Option<String>, &'static str> {
    match display_type {
        "language" => {
            if !is_valid_language_code(code) {
                return Err("invalid language code");
            }
            let locale: IcuLocale = locale_str.parse().unwrap_or_else(|_| "en".parse().unwrap());
            let icu_style = match style {
                "short" => Some(IcuStyle::Short),
                "narrow" => Some(IcuStyle::Narrow),
                _ => None,
            };
            let lang_display = match language_display.as_deref() {
                Some("standard") => IcuLanguageDisplay::Standard,
                _ => IcuLanguageDisplay::Dialect,
            };
            let mut opts = IcuDisplayNamesOptions::default();
            opts.style = icu_style;
            opts.language_display = lang_display;
            let prefs = (&locale).into();
            if let Ok(formatter) = LocaleDisplayNamesFormatter::try_new(prefs, opts) {
                let code_locale: IcuLocale =
                    code.parse().unwrap_or_else(|_| "und".parse().unwrap());
                let result = formatter.of(&code_locale).into_owned();
                if result == code || result == "und" {
                    if fallback == "code" {
                        return Ok(Some(code.to_string()));
                    }
                    return Ok(None);
                }
                return Ok(Some(result));
            }
            if fallback == "code" {
                Ok(Some(code.to_string()))
            } else {
                Ok(None)
            }
        }
        "region" => {
            if !is_valid_region_code(code) {
                return Err("invalid region code");
            }
            let locale: IcuLocale = locale_str.parse().unwrap_or_else(|_| "en".parse().unwrap());
            let icu_style = match style {
                "short" => Some(IcuStyle::Short),
                "narrow" => Some(IcuStyle::Narrow),
                _ => None,
            };
            let mut opts = IcuDisplayNamesOptions::default();
            opts.style = icu_style;
            let prefs = (&locale).into();
            if let Ok(formatter) = RegionDisplayNames::try_new(prefs, opts) {
                let upper = code.to_ascii_uppercase();
                if let Ok(region) = upper.parse() {
                    if let Some(name) = formatter.of(region) {
                        return Ok(Some(name.to_string()));
                    }
                }
            }
            if fallback == "code" {
                Ok(Some(code.to_ascii_uppercase()))
            } else {
                Ok(None)
            }
        }
        "script" => {
            if !is_valid_script_code(code) {
                return Err("invalid script code");
            }
            let locale: IcuLocale = locale_str.parse().unwrap_or_else(|_| "en".parse().unwrap());
            let icu_style = match style {
                "short" => Some(IcuStyle::Short),
                "narrow" => Some(IcuStyle::Narrow),
                _ => None,
            };
            let mut opts = IcuDisplayNamesOptions::default();
            opts.style = icu_style;
            let prefs = (&locale).into();
            if let Ok(formatter) = ScriptDisplayNames::try_new(prefs, opts) {
                // Capitalize first letter for parsing: "latn" -> "Latn"
                let mut capitalized = String::new();
                for (i, c) in code.chars().enumerate() {
                    if i == 0 {
                        capitalized.push(c.to_ascii_uppercase());
                    } else {
                        capitalized.push(c.to_ascii_lowercase());
                    }
                }
                if let Ok(script) = capitalized.parse() {
                    if let Some(name) = formatter.of(script) {
                        return Ok(Some(name.to_string()));
                    }
                }
            }
            if fallback == "code" {
                // Title case for script codes
                let mut capitalized = String::new();
                for (i, c) in code.chars().enumerate() {
                    if i == 0 {
                        capitalized.push(c.to_ascii_uppercase());
                    } else {
                        capitalized.push(c.to_ascii_lowercase());
                    }
                }
                Ok(Some(capitalized))
            } else {
                Ok(None)
            }
        }
        "currency" => {
            if !is_valid_currency_code(code) {
                return Err("invalid currency code");
            }
            if let Some(name) = get_currency_display_name(code, style) {
                Ok(Some(name.to_string()))
            } else if fallback == "code" {
                Ok(Some(code.to_ascii_uppercase()))
            } else {
                Ok(None)
            }
        }
        "calendar" => {
            if !is_valid_calendar_code(code) {
                return Err("invalid calendar code");
            }
            if let Some(name) = get_calendar_display_name(code, style) {
                Ok(Some(name.to_string()))
            } else if fallback == "code" {
                Ok(Some(code.to_string()))
            } else {
                Ok(None)
            }
        }
        "dateTimeField" => {
            if !is_valid_date_time_field(code) {
                return Err("invalid dateTimeField code");
            }
            if let Some(name) = get_date_time_field_display_name(code, style) {
                Ok(Some(name.to_string()))
            } else if fallback == "code" {
                Ok(Some(code.to_string()))
            } else {
                Ok(None)
            }
        }
        _ => Err("invalid type"),
    }
}

impl Interpreter {
    pub(crate) fn setup_intl_display_names(&mut self, intl_obj: &Rc<RefCell<JsObjectData>>) {
        let proto = self.create_object();
        if let Some(ref op) = self.object_prototype {
            proto.borrow_mut().prototype = Some(op.clone());
        }
        proto.borrow_mut().class_name = "Intl.DisplayNames".to_string();

        // @@toStringTag
        proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("Intl.DisplayNames"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            },
        );

        // of(code)
        let of_fn = self.create_function(JsFunction::native(
            "of".to_string(),
            1,
            |interp, this, args| {
                let data = match extract_display_names_data(interp, this) {
                    Ok(d) => d,
                    Err(e) => return Completion::Throw(e),
                };

                let code_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let code = match interp.to_string_value(&code_arg) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };

                match get_display_name_for_code(
                    &data.locale,
                    &data.display_type,
                    &data.style,
                    &data.fallback,
                    &data.language_display,
                    &code,
                ) {
                    Ok(Some(name)) => {
                        Completion::Normal(JsValue::String(JsString::from_str(&name)))
                    }
                    Ok(None) => Completion::Normal(JsValue::Undefined),
                    Err(_msg) => {
                        let err = interp.create_range_error(&format!("Invalid code: {}", code));
                        Completion::Throw(err)
                    }
                }
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("of".to_string(), of_fn);

        // resolvedOptions()
        let resolved_fn = self.create_function(JsFunction::native(
            "resolvedOptions".to_string(),
            0,
            |interp, this, _args| {
                let data = match extract_display_names_data(interp, this) {
                    Ok(d) => d,
                    Err(e) => return Completion::Throw(e),
                };

                let result = interp.create_object();
                if let Some(ref op) = interp.object_prototype {
                    result.borrow_mut().prototype = Some(op.clone());
                }

                // Properties in spec order: locale, style, type, fallback, languageDisplay
                result.borrow_mut().insert_property(
                    "locale".to_string(),
                    PropertyDescriptor::data(
                        JsValue::String(JsString::from_str(&data.locale)),
                        true,
                        true,
                        true,
                    ),
                );
                result.borrow_mut().insert_property(
                    "style".to_string(),
                    PropertyDescriptor::data(
                        JsValue::String(JsString::from_str(&data.style)),
                        true,
                        true,
                        true,
                    ),
                );
                result.borrow_mut().insert_property(
                    "type".to_string(),
                    PropertyDescriptor::data(
                        JsValue::String(JsString::from_str(&data.display_type)),
                        true,
                        true,
                        true,
                    ),
                );
                result.borrow_mut().insert_property(
                    "fallback".to_string(),
                    PropertyDescriptor::data(
                        JsValue::String(JsString::from_str(&data.fallback)),
                        true,
                        true,
                        true,
                    ),
                );
                if data.display_type == "language" {
                    let ld = data.language_display.as_deref().unwrap_or("dialect");
                    result.borrow_mut().insert_property(
                        "languageDisplay".to_string(),
                        PropertyDescriptor::data(
                            JsValue::String(JsString::from_str(ld)),
                            true,
                            true,
                            true,
                        ),
                    );
                }

                let result_id = result.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id: result_id }))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("resolvedOptions".to_string(), resolved_fn);

        self.intl_display_names_prototype = Some(proto.clone());

        // --- Constructor ---
        let proto_id = proto.borrow().id.unwrap();
        let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });
        let proto_clone = proto.clone();

        let display_names_ctor = self.create_function(JsFunction::constructor(
            "DisplayNames".to_string(),
            2,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp
                            .create_type_error("Constructor Intl.DisplayNames requires 'new'"),
                    );
                }

                let locales_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let options_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);

                let requested = match interp.intl_canonicalize_locale_list(&locales_arg) {
                    Ok(list) => list,
                    Err(e) => return Completion::Throw(e),
                };

                // DisplayNames requires options to be an object, not undefined
                if matches!(options_arg, JsValue::Undefined) {
                    return Completion::Throw(
                        interp.create_type_error("Options argument is required for Intl.DisplayNames"),
                    );
                }

                let options = match interp.intl_get_options_object(&options_arg) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // Read options in spec order: localeMatcher, style, type, fallback, languageDisplay
                let _locale_matcher = match interp.intl_get_option(
                    &options,
                    "localeMatcher",
                    &["lookup", "best fit"],
                    Some("best fit"),
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

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

                // type is REQUIRED - must throw TypeError if undefined
                let display_type = match interp.intl_get_option(
                    &options,
                    "type",
                    &[
                        "language",
                        "region",
                        "script",
                        "currency",
                        "calendar",
                        "dateTimeField",
                    ],
                    None,
                ) {
                    Ok(Some(v)) => v,
                    Ok(None) => {
                        return Completion::Throw(
                            interp.create_type_error("Required option \"type\" not specified"),
                        );
                    }
                    Err(e) => return Completion::Throw(e),
                };

                let fallback = match interp.intl_get_option(
                    &options,
                    "fallback",
                    &["code", "none"],
                    Some("code"),
                ) {
                    Ok(Some(v)) => v,
                    Ok(None) => "code".to_string(),
                    Err(e) => return Completion::Throw(e),
                };

                let language_display = if display_type == "language" {
                    match interp.intl_get_option(
                        &options,
                        "languageDisplay",
                        &["dialect", "standard"],
                        Some("dialect"),
                    ) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    }
                } else {
                    None
                };

                let locale = interp.intl_resolve_locale(&requested);

                let obj = interp.create_object();
                obj.borrow_mut().prototype = Some(proto_clone.clone());
                obj.borrow_mut().class_name = "Intl.DisplayNames".to_string();
                obj.borrow_mut().intl_data = Some(IntlData::DisplayNames {
                    locale,
                    style,
                    display_type,
                    fallback,
                    language_display,
                });

                let obj_id = obj.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id: obj_id }))
            },
        ));

        // Set DisplayNames.prototype on constructor
        if let JsValue::Object(ctor_ref) = &display_names_ctor {
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
            PropertyDescriptor::data(display_names_ctor.clone(), true, false, true),
        );

        // Register Intl.DisplayNames on the Intl namespace
        intl_obj.borrow_mut().insert_property(
            "DisplayNames".to_string(),
            PropertyDescriptor::data(display_names_ctor, true, false, true),
        );
    }
}
