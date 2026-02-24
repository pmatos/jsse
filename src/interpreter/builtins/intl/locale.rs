use super::super::super::*;
use icu::locale::Locale as IcuLocale;
use icu::locale::extensions::unicode::{Key, Value};
use icu::locale::{LocaleCanonicalizer, LocaleDirectionality, LocaleExpander};

fn extract_unicode_keyword(locale: &IcuLocale, key_str: &str) -> Option<String> {
    let key: Key = key_str.parse().ok()?;
    locale
        .extensions
        .unicode
        .keywords
        .get(&key)
        .map(|v| v.to_string())
}

fn set_unicode_keyword(locale: &mut IcuLocale, key_str: &str, value_str: &str) {
    if let Ok(key) = key_str.parse::<Key>() {
        if let Ok(val) = value_str.parse::<Value>() {
            locale.extensions.unicode.keywords.set(key, val);
        }
    }
}

fn canonicalize_unicode_keyword_values(locale: &mut IcuLocale) {
    let ca_key: Key = "ca".parse().unwrap();
    if let Some(val) = locale.extensions.unicode.keywords.get(&ca_key) {
        let val_str = val.to_string();
        let canonical = match val_str.as_str() {
            "islamicc" => Some("islamic-civil"),
            "ethiopic-amete-alem" => Some("ethioaa"),
            _ => None,
        };
        if let Some(new_val) = canonical {
            if let Ok(v) = new_val.parse::<Value>() {
                locale.extensions.unicode.keywords.set(ca_key, v);
            }
        }
    }
}

fn get_variants_string(locale: &IcuLocale) -> Option<String> {
    if locale.id.variants.is_empty() {
        None
    } else {
        let parts: Vec<String> = locale.id.variants.iter().map(|v| v.to_string()).collect();
        Some(parts.join("-"))
    }
}

// Validate unicode extension keyword value per spec: (3*8alphanum) *("-" (3*8alphanum))
fn is_valid_unicode_type_value(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    for part in s.split('-') {
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

// Validate a variants string: one or more unicode_variant_subtag separated by "-"
// unicode_variant_subtag = (alphanum{5,8} | digit alphanum{3})
fn is_valid_variants_value(s: &str) -> bool {
    if s.is_empty() || s.starts_with('-') || s.ends_with('-') || s.contains("--") {
        return false;
    }
    let parts: Vec<&str> = s.split('-').collect();
    let mut seen = std::collections::HashSet::new();
    for part in &parts {
        let lower = part.to_ascii_lowercase();
        if !seen.insert(lower) {
            return false;
        }
        let len = part.len();
        let all_alphanum = part.chars().all(|c| c.is_ascii_alphanumeric());
        if !all_alphanum {
            return false;
        }
        let is_digit_variant =
            len == 4 && part.chars().next().map_or(false, |c| c.is_ascii_digit());
        let is_alpha_variant = len >= 5 && len <= 8;
        if !is_digit_variant && !is_alpha_variant {
            return false;
        }
    }
    true
}

fn number_to_weekday_string(n: f64) -> Option<&'static str> {
    match n as i64 {
        0 => Some("sun"),
        1 => Some("mon"),
        2 => Some("tue"),
        3 => Some("wed"),
        4 => Some("thu"),
        5 => Some("fri"),
        6 => Some("sat"),
        7 => Some("sun"),
        _ => None,
    }
}

fn string_digit_to_weekday(s: &str) -> Option<&'static str> {
    match s {
        "0" => Some("sun"),
        "1" => Some("mon"),
        "2" => Some("tue"),
        "3" => Some("wed"),
        "4" => Some("thu"),
        "5" => Some("fri"),
        "6" => Some("sat"),
        "7" => Some("sun"),
        _ => None,
    }
}

fn fw_keyword_to_day_number(fw: &str) -> Option<i32> {
    match fw {
        "mon" => Some(1),
        "tue" => Some(2),
        "wed" => Some(3),
        "thu" => Some(4),
        "fri" => Some(5),
        "sat" => Some(6),
        "sun" => Some(7),
        _ => None,
    }
}

fn weekday_to_number(wd: icu::calendar::types::Weekday) -> i32 {
    match wd {
        icu::calendar::types::Weekday::Monday => 1,
        icu::calendar::types::Weekday::Tuesday => 2,
        icu::calendar::types::Weekday::Wednesday => 3,
        icu::calendar::types::Weekday::Thursday => 4,
        icu::calendar::types::Weekday::Friday => 5,
        icu::calendar::types::Weekday::Saturday => 6,
        icu::calendar::types::Weekday::Sunday => 7,
    }
}

fn build_intl_data_from_locale(locale: &IcuLocale) -> IntlData {
    let numeric_str = extract_unicode_keyword(locale, "kn");
    let numeric = numeric_str.map(|s| s == "true" || s.is_empty());
    let first_day_of_week = extract_unicode_keyword(locale, "fw");

    IntlData::Locale {
        tag: locale.to_string(),
        language: locale.id.language.to_string(),
        script: locale.id.script.map(|s| s.to_string()),
        region: locale.id.region.map(|r| r.to_string()),
        variants: get_variants_string(locale),
        calendar: extract_unicode_keyword(locale, "ca"),
        case_first: extract_unicode_keyword(locale, "kf"),
        collation: extract_unicode_keyword(locale, "co"),
        hour_cycle: extract_unicode_keyword(locale, "hc"),
        numbering_system: extract_unicode_keyword(locale, "nu"),
        numeric,
        first_day_of_week,
    }
}

fn create_locale_object_from_icu(interp: &mut Interpreter, locale: &IcuLocale) -> JsValue {
    let obj = interp.create_object();
    if let Some(ref proto) = interp.intl_locale_prototype {
        obj.borrow_mut().prototype = Some(proto.clone());
    }
    obj.borrow_mut().class_name = "Intl.Locale".to_string();
    obj.borrow_mut().intl_data = Some(build_intl_data_from_locale(locale));
    let obj_id = obj.borrow().id.unwrap();
    JsValue::Object(crate::types::JsObject { id: obj_id })
}

fn get_locale_intl_data_field<F>(
    interp: &mut Interpreter,
    this: &JsValue,
    method_name: &str,
    extractor: F,
) -> Completion
where
    F: FnOnce(&IntlData) -> Completion,
{
    if let JsValue::Object(o) = this
        && let Some(obj) = interp.get_object(o.id)
    {
        let b = obj.borrow();
        if let Some(ref data @ IntlData::Locale { .. }) = b.intl_data {
            return extractor(data);
        }
    }
    Completion::Throw(interp.create_type_error(&format!(
        "Intl.Locale.prototype.{} requires an Intl.Locale object",
        method_name
    )))
}

impl Interpreter {
    pub(crate) fn setup_intl_locale(&mut self, intl_obj: &Rc<RefCell<JsObjectData>>) {
        let proto = self.create_object();
        if let Some(ref op) = self.object_prototype {
            proto.borrow_mut().prototype = Some(op.clone());
        }
        proto.borrow_mut().class_name = "Intl.Locale".to_string();

        // @@toStringTag
        proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("Intl.Locale"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            },
        );

        // --- Getter accessors ---

        // baseName
        let getter = self.create_function(JsFunction::native(
            "get baseName".to_string(),
            0,
            |interp, this, _args| {
                get_locale_intl_data_field(interp, this, "baseName", |data| {
                    if let IntlData::Locale {
                        language,
                        script,
                        region,
                        variants,
                        ..
                    } = data
                    {
                        let mut result = language.clone();
                        if let Some(s) = script {
                            result.push('-');
                            result.push_str(s);
                        }
                        if let Some(r) = region {
                            result.push('-');
                            result.push_str(r);
                        }
                        if let Some(v) = variants {
                            result.push('-');
                            result.push_str(v);
                        }
                        Completion::Normal(JsValue::String(JsString::from_str(&result)))
                    } else {
                        unreachable!()
                    }
                })
            },
        ));
        proto.borrow_mut().insert_property(
            "baseName".to_string(),
            PropertyDescriptor::accessor(Some(getter), None, false, true),
        );

        // calendar
        let getter = self.create_function(JsFunction::native(
            "get calendar".to_string(),
            0,
            |interp, this, _args| {
                get_locale_intl_data_field(interp, this, "calendar", |data| {
                    if let IntlData::Locale { calendar, .. } = data {
                        Completion::Normal(match calendar {
                            Some(s) => JsValue::String(JsString::from_str(s)),
                            None => JsValue::Undefined,
                        })
                    } else {
                        unreachable!()
                    }
                })
            },
        ));
        proto.borrow_mut().insert_property(
            "calendar".to_string(),
            PropertyDescriptor::accessor(Some(getter), None, false, true),
        );

        // caseFirst
        let getter = self.create_function(JsFunction::native(
            "get caseFirst".to_string(),
            0,
            |interp, this, _args| {
                get_locale_intl_data_field(interp, this, "caseFirst", |data| {
                    if let IntlData::Locale { case_first, .. } = data {
                        Completion::Normal(match case_first {
                            Some(s) => JsValue::String(JsString::from_str(s)),
                            None => JsValue::Undefined,
                        })
                    } else {
                        unreachable!()
                    }
                })
            },
        ));
        proto.borrow_mut().insert_property(
            "caseFirst".to_string(),
            PropertyDescriptor::accessor(Some(getter), None, false, true),
        );

        // collation
        let getter = self.create_function(JsFunction::native(
            "get collation".to_string(),
            0,
            |interp, this, _args| {
                get_locale_intl_data_field(interp, this, "collation", |data| {
                    if let IntlData::Locale { collation, .. } = data {
                        Completion::Normal(match collation {
                            Some(s) => JsValue::String(JsString::from_str(s)),
                            None => JsValue::Undefined,
                        })
                    } else {
                        unreachable!()
                    }
                })
            },
        ));
        proto.borrow_mut().insert_property(
            "collation".to_string(),
            PropertyDescriptor::accessor(Some(getter), None, false, true),
        );

        // hourCycle
        let getter = self.create_function(JsFunction::native(
            "get hourCycle".to_string(),
            0,
            |interp, this, _args| {
                get_locale_intl_data_field(interp, this, "hourCycle", |data| {
                    if let IntlData::Locale { hour_cycle, .. } = data {
                        Completion::Normal(match hour_cycle {
                            Some(s) => JsValue::String(JsString::from_str(s)),
                            None => JsValue::Undefined,
                        })
                    } else {
                        unreachable!()
                    }
                })
            },
        ));
        proto.borrow_mut().insert_property(
            "hourCycle".to_string(),
            PropertyDescriptor::accessor(Some(getter), None, false, true),
        );

        // language
        let getter = self.create_function(JsFunction::native(
            "get language".to_string(),
            0,
            |interp, this, _args| {
                get_locale_intl_data_field(interp, this, "language", |data| {
                    if let IntlData::Locale { language, .. } = data {
                        Completion::Normal(JsValue::String(JsString::from_str(language)))
                    } else {
                        unreachable!()
                    }
                })
            },
        ));
        proto.borrow_mut().insert_property(
            "language".to_string(),
            PropertyDescriptor::accessor(Some(getter), None, false, true),
        );

        // numberingSystem
        let getter = self.create_function(JsFunction::native(
            "get numberingSystem".to_string(),
            0,
            |interp, this, _args| {
                get_locale_intl_data_field(interp, this, "numberingSystem", |data| {
                    if let IntlData::Locale {
                        numbering_system, ..
                    } = data
                    {
                        Completion::Normal(match numbering_system {
                            Some(s) => JsValue::String(JsString::from_str(s)),
                            None => JsValue::Undefined,
                        })
                    } else {
                        unreachable!()
                    }
                })
            },
        ));
        proto.borrow_mut().insert_property(
            "numberingSystem".to_string(),
            PropertyDescriptor::accessor(Some(getter), None, false, true),
        );

        // numeric
        let getter = self.create_function(JsFunction::native(
            "get numeric".to_string(),
            0,
            |interp, this, _args| {
                get_locale_intl_data_field(interp, this, "numeric", |data| {
                    if let IntlData::Locale { numeric, .. } = data {
                        Completion::Normal(JsValue::Boolean(numeric.unwrap_or(false)))
                    } else {
                        unreachable!()
                    }
                })
            },
        ));
        proto.borrow_mut().insert_property(
            "numeric".to_string(),
            PropertyDescriptor::accessor(Some(getter), None, false, true),
        );

        // region
        let getter = self.create_function(JsFunction::native(
            "get region".to_string(),
            0,
            |interp, this, _args| {
                get_locale_intl_data_field(interp, this, "region", |data| {
                    if let IntlData::Locale { region, .. } = data {
                        Completion::Normal(match region {
                            Some(s) => JsValue::String(JsString::from_str(s)),
                            None => JsValue::Undefined,
                        })
                    } else {
                        unreachable!()
                    }
                })
            },
        ));
        proto.borrow_mut().insert_property(
            "region".to_string(),
            PropertyDescriptor::accessor(Some(getter), None, false, true),
        );

        // script
        let getter = self.create_function(JsFunction::native(
            "get script".to_string(),
            0,
            |interp, this, _args| {
                get_locale_intl_data_field(interp, this, "script", |data| {
                    if let IntlData::Locale { script, .. } = data {
                        Completion::Normal(match script {
                            Some(s) => JsValue::String(JsString::from_str(s)),
                            None => JsValue::Undefined,
                        })
                    } else {
                        unreachable!()
                    }
                })
            },
        ));
        proto.borrow_mut().insert_property(
            "script".to_string(),
            PropertyDescriptor::accessor(Some(getter), None, false, true),
        );

        // variants
        let getter = self.create_function(JsFunction::native(
            "get variants".to_string(),
            0,
            |interp, this, _args| {
                get_locale_intl_data_field(interp, this, "variants", |data| {
                    if let IntlData::Locale { variants, .. } = data {
                        Completion::Normal(match variants {
                            Some(s) => JsValue::String(JsString::from_str(s)),
                            None => JsValue::Undefined,
                        })
                    } else {
                        unreachable!()
                    }
                })
            },
        ));
        proto.borrow_mut().insert_property(
            "variants".to_string(),
            PropertyDescriptor::accessor(Some(getter), None, false, true),
        );

        // firstDayOfWeek
        let getter = self.create_function(JsFunction::native(
            "get firstDayOfWeek".to_string(),
            0,
            |interp, this, _args| {
                get_locale_intl_data_field(interp, this, "firstDayOfWeek", |data| {
                    if let IntlData::Locale {
                        first_day_of_week, ..
                    } = data
                    {
                        Completion::Normal(match first_day_of_week {
                            Some(s) => JsValue::String(JsString::from_str(s)),
                            None => JsValue::Undefined,
                        })
                    } else {
                        unreachable!()
                    }
                })
            },
        ));
        proto.borrow_mut().insert_property(
            "firstDayOfWeek".to_string(),
            PropertyDescriptor::accessor(Some(getter), None, false, true),
        );

        // --- Prototype methods ---

        // toString()
        let to_string_fn = self.create_function(JsFunction::native(
            "toString".to_string(),
            0,
            |interp, this, _args| {
                get_locale_intl_data_field(interp, this, "toString", |data| {
                    if let IntlData::Locale { tag, .. } = data {
                        Completion::Normal(JsValue::String(JsString::from_str(tag)))
                    } else {
                        unreachable!()
                    }
                })
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
                get_locale_intl_data_field(interp, this, "toJSON", |data| {
                    if let IntlData::Locale { tag, .. } = data {
                        Completion::Normal(JsValue::String(JsString::from_str(tag)))
                    } else {
                        unreachable!()
                    }
                })
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toJSON".to_string(), to_json_fn);

        // maximize()
        let maximize_fn = self.create_function(JsFunction::native(
            "maximize".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let tag = {
                        let b = obj.borrow();
                        if let Some(IntlData::Locale { ref tag, .. }) = b.intl_data {
                            tag.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error(
                                "Intl.Locale.prototype.maximize requires an Intl.Locale object",
                            ));
                        }
                    };

                    match tag.parse::<IcuLocale>() {
                        Ok(mut locale) => {
                            let expander = LocaleExpander::new_extended();
                            expander.maximize(&mut locale.id);
                            return Completion::Normal(create_locale_object_from_icu(
                                interp, &locale,
                            ));
                        }
                        Err(_) => {
                            // For tags ICU4X can't parse (like "posix"),
                            // maximize returns the same tag
                            return Completion::Normal(JsValue::Object(crate::types::JsObject {
                                id: o.id,
                            }));
                        }
                    }
                }
                Completion::Throw(interp.create_type_error(
                    "Intl.Locale.prototype.maximize requires an Intl.Locale object",
                ))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("maximize".to_string(), maximize_fn);

        // minimize()
        let minimize_fn = self.create_function(JsFunction::native(
            "minimize".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let tag = {
                        let b = obj.borrow();
                        if let Some(IntlData::Locale { ref tag, .. }) = b.intl_data {
                            tag.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error(
                                "Intl.Locale.prototype.minimize requires an Intl.Locale object",
                            ));
                        }
                    };

                    match tag.parse::<IcuLocale>() {
                        Ok(mut locale) => {
                            let expander = LocaleExpander::new_extended();
                            expander.minimize(&mut locale.id);
                            return Completion::Normal(create_locale_object_from_icu(
                                interp, &locale,
                            ));
                        }
                        Err(_) => {
                            return Completion::Normal(JsValue::Object(crate::types::JsObject {
                                id: o.id,
                            }));
                        }
                    }
                }
                Completion::Throw(interp.create_type_error(
                    "Intl.Locale.prototype.minimize requires an Intl.Locale object",
                ))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("minimize".to_string(), minimize_fn);

        // --- Locale info methods ---

        // getCalendars()
        let get_calendars_fn = self.create_function(JsFunction::native(
            "getCalendars".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let cal = {
                        let b = obj.borrow();
                        if let Some(IntlData::Locale { ref calendar, .. }) = b.intl_data {
                            calendar.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error(
                                "Intl.Locale.prototype.getCalendars requires an Intl.Locale object",
                            ));
                        }
                    };
                    let calendars = if let Some(ca) = cal {
                        vec![JsValue::String(JsString::from_str(&ca))]
                    } else {
                        vec![JsValue::String(JsString::from_str("gregory"))]
                    };
                    return Completion::Normal(interp.create_array(calendars));
                }
                Completion::Throw(interp.create_type_error(
                    "Intl.Locale.prototype.getCalendars requires an Intl.Locale object",
                ))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("getCalendars".to_string(), get_calendars_fn);

        // getCollations()
        let get_collations_fn = self.create_function(JsFunction::native(
            "getCollations".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let col = {
                        let b = obj.borrow();
                        if let Some(IntlData::Locale { ref collation, .. }) = b.intl_data {
                            collation.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error(
                                "Intl.Locale.prototype.getCollations requires an Intl.Locale object",
                            ));
                        }
                    };
                    let collations = if let Some(co) = col {
                        if co == "standard" || co == "search" {
                            vec![JsValue::String(JsString::from_str("emoji"))]
                        } else {
                            vec![JsValue::String(JsString::from_str(&co))]
                        }
                    } else {
                        vec![JsValue::String(JsString::from_str("emoji"))]
                    };
                    return Completion::Normal(interp.create_array(collations));
                }
                Completion::Throw(
                    interp.create_type_error(
                        "Intl.Locale.prototype.getCollations requires an Intl.Locale object",
                    ),
                )
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("getCollations".to_string(), get_collations_fn);

        // getHourCycles()
        let get_hour_cycles_fn = self.create_function(JsFunction::native(
            "getHourCycles".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let (hc, region) = {
                        let b = obj.borrow();
                        if let Some(IntlData::Locale {
                            ref hour_cycle,
                            ref region,
                            ..
                        }) = b.intl_data
                        {
                            (hour_cycle.clone(), region.clone())
                        } else {
                            return Completion::Throw(interp.create_type_error(
                                "Intl.Locale.prototype.getHourCycles requires an Intl.Locale object",
                            ));
                        }
                    };
                    let cycles = if let Some(h) = hc {
                        vec![JsValue::String(JsString::from_str(&h))]
                    } else {
                        let h12_regions = ["US", "CA", "AU", "NZ", "PH", "IN", "EG", "SA", "CO", "PK", "MY"];
                        let default = if let Some(ref r) = region {
                            if h12_regions.contains(&r.as_str()) { "h12" } else { "h23" }
                        } else {
                            "h23"
                        };
                        vec![JsValue::String(JsString::from_str(default))]
                    };
                    return Completion::Normal(interp.create_array(cycles));
                }
                Completion::Throw(
                    interp.create_type_error(
                        "Intl.Locale.prototype.getHourCycles requires an Intl.Locale object",
                    ),
                )
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("getHourCycles".to_string(), get_hour_cycles_fn);

        // getNumberingSystems()
        let get_numbering_systems_fn = self.create_function(JsFunction::native(
            "getNumberingSystems".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let nu = {
                        let b = obj.borrow();
                        if let Some(IntlData::Locale {
                            ref numbering_system,
                            ..
                        }) = b.intl_data
                        {
                            numbering_system.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error(
                                "Intl.Locale.prototype.getNumberingSystems requires an Intl.Locale object",
                            ));
                        }
                    };
                    let systems = if let Some(n) = nu {
                        vec![JsValue::String(JsString::from_str(&n))]
                    } else {
                        vec![JsValue::String(JsString::from_str("latn"))]
                    };
                    return Completion::Normal(interp.create_array(systems));
                }
                Completion::Throw(
                    interp.create_type_error(
                        "Intl.Locale.prototype.getNumberingSystems requires an Intl.Locale object",
                    ),
                )
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("getNumberingSystems".to_string(), get_numbering_systems_fn);

        // getTextInfo()
        let get_text_info_fn = self.create_function(JsFunction::native(
            "getTextInfo".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let tag = {
                        let b = obj.borrow();
                        if let Some(IntlData::Locale { ref tag, .. }) = b.intl_data {
                            tag.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error(
                                "Intl.Locale.prototype.getTextInfo requires an Intl.Locale object",
                            ));
                        }
                    };

                    let direction = if let Ok(locale) = tag.parse::<IcuLocale>() {
                        let ld = LocaleDirectionality::new_extended();
                        if ld.is_right_to_left(&locale.id) {
                            "rtl"
                        } else {
                            "ltr"
                        }
                    } else {
                        "ltr"
                    };

                    let info_obj = interp.create_object();
                    if let Some(ref op) = interp.object_prototype {
                        info_obj.borrow_mut().prototype = Some(op.clone());
                    }
                    info_obj.borrow_mut().insert_property(
                        "direction".to_string(),
                        PropertyDescriptor::data(
                            JsValue::String(JsString::from_str(direction)),
                            true,
                            true,
                            true,
                        ),
                    );
                    let info_id = info_obj.borrow().id.unwrap();
                    return Completion::Normal(JsValue::Object(crate::types::JsObject {
                        id: info_id,
                    }));
                }
                Completion::Throw(interp.create_type_error(
                    "Intl.Locale.prototype.getTextInfo requires an Intl.Locale object",
                ))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("getTextInfo".to_string(), get_text_info_fn);

        // getTimeZones()
        let get_time_zones_fn = self.create_function(JsFunction::native(
            "getTimeZones".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let region = {
                        let b = obj.borrow();
                        if let Some(IntlData::Locale { ref region, .. }) = b.intl_data {
                            region.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error(
                                "Intl.Locale.prototype.getTimeZones requires an Intl.Locale object",
                            ));
                        }
                    };

                    if region.is_none() {
                        return Completion::Normal(JsValue::Undefined);
                    }

                    let region_str = region.unwrap();
                    let tzs = get_timezones_for_region(&region_str);
                    let values: Vec<JsValue> = tzs
                        .iter()
                        .map(|tz| JsValue::String(JsString::from_str(tz)))
                        .collect();
                    return Completion::Normal(interp.create_array(values));
                }
                Completion::Throw(interp.create_type_error(
                    "Intl.Locale.prototype.getTimeZones requires an Intl.Locale object",
                ))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("getTimeZones".to_string(), get_time_zones_fn);

        // getWeekInfo()
        let get_week_info_fn = self.create_function(JsFunction::native(
            "getWeekInfo".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let (tag, fw_value) = {
                        let b = obj.borrow();
                        if let Some(IntlData::Locale {
                            ref tag,
                            ref first_day_of_week,
                            ..
                        }) = b.intl_data
                        {
                            (tag.clone(), first_day_of_week.clone())
                        } else {
                            return Completion::Throw(interp.create_type_error(
                                "Intl.Locale.prototype.getWeekInfo requires an Intl.Locale object",
                            ));
                        }
                    };

                    let locale: IcuLocale = match tag.parse() {
                        Ok(l) => l,
                        Err(_) => {
                            return Completion::Throw(
                                interp
                                    .create_range_error(&format!("Invalid language tag: {}", tag)),
                            );
                        }
                    };

                    let first_day = if let Some(ref fw) = fw_value {
                        fw_keyword_to_day_number(fw).unwrap_or(7)
                    } else {
                        let wi = icu::calendar::week::WeekInformation::try_new((&locale).into());
                        if let Ok(week_info) = wi {
                            weekday_to_number(week_info.first_weekday)
                        } else {
                            7 // default Sunday
                        }
                    };

                    let mut weekend_days: Vec<i32> = Vec::new();
                    let wi = icu::calendar::week::WeekInformation::try_new((&locale).into());
                    if let Ok(week_info) = wi {
                        use icu::calendar::types::Weekday;
                        for wd in [
                            Weekday::Monday,
                            Weekday::Tuesday,
                            Weekday::Wednesday,
                            Weekday::Thursday,
                            Weekday::Friday,
                            Weekday::Saturday,
                            Weekday::Sunday,
                        ] {
                            if week_info.weekend.contains(wd) {
                                weekend_days.push(weekday_to_number(wd));
                            }
                        }
                    } else {
                        weekend_days = vec![6, 7];
                    }
                    weekend_days.sort();

                    let info_obj = interp.create_object();
                    if let Some(ref op) = interp.object_prototype {
                        info_obj.borrow_mut().prototype = Some(op.clone());
                    }
                    info_obj.borrow_mut().insert_property(
                        "firstDay".to_string(),
                        PropertyDescriptor::data(
                            JsValue::Number(first_day as f64),
                            true,
                            true,
                            true,
                        ),
                    );
                    let weekend_values: Vec<JsValue> = weekend_days
                        .iter()
                        .map(|&d| JsValue::Number(d as f64))
                        .collect();
                    let weekend_arr = interp.create_array(weekend_values);
                    info_obj.borrow_mut().insert_property(
                        "weekend".to_string(),
                        PropertyDescriptor::data(weekend_arr, true, true, true),
                    );
                    let info_id = info_obj.borrow().id.unwrap();
                    return Completion::Normal(JsValue::Object(crate::types::JsObject {
                        id: info_id,
                    }));
                }
                Completion::Throw(interp.create_type_error(
                    "Intl.Locale.prototype.getWeekInfo requires an Intl.Locale object",
                ))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("getWeekInfo".to_string(), get_week_info_fn);

        // Store the prototype
        self.intl_locale_prototype = Some(proto.clone());

        // --- Constructor ---
        let proto_id = proto.borrow().id.unwrap();
        let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });
        let proto_clone = proto.clone();

        let locale_ctor = self.create_function(JsFunction::constructor(
            "Locale".to_string(),
            1,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Constructor Intl.Locale requires 'new'"),
                    );
                }

                let tag_arg = args.first().cloned().unwrap_or(JsValue::Undefined);

                // Step 7: If Type(tag) is not String or Object, throw a TypeError
                match &tag_arg {
                    JsValue::String(_) | JsValue::Object(_) => {}
                    _ => {
                        return Completion::Throw(interp.create_type_error(
                            "First argument to Intl.Locale must be a string or object",
                        ));
                    }
                }

                // If tag_arg is an Intl.Locale, get its tag string
                let tag_string = if let JsValue::Object(o) = &tag_arg {
                    if let Some(obj) = interp.get_object(o.id) {
                        let b = obj.borrow();
                        if let Some(IntlData::Locale { ref tag, .. }) = b.intl_data {
                            tag.clone()
                        } else {
                            drop(b);
                            match interp.to_string_value(&tag_arg) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                    } else {
                        match interp.to_string_value(&tag_arg) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                } else {
                    match interp.to_string_value(&tag_arg) {
                        Ok(s) => s,
                        Err(e) => return Completion::Throw(e),
                    }
                };

                let (mut locale, is_fallback_tag) = match tag_string.parse::<IcuLocale>() {
                    Ok(l) => (l, false),
                    Err(_) => {
                        if Interpreter::is_structurally_valid_language_tag(&tag_string) {
                            // Tags like "posix" are structurally valid BCP47 but not in ICU4X data.
                            // Create a minimal locale and store the original tag.
                            let fallback: IcuLocale = "und".parse().unwrap();
                            (fallback, true)
                        } else {
                            return Completion::Throw(interp.create_range_error(&format!(
                                "Invalid language tag: {}",
                                tag_string
                            )));
                        }
                    }
                };

                // Canonicalize the parsed tag first (before applying options).
                // This handles aliases like "mo" -> "ro", "aar" -> "aa", etc.
                if !is_fallback_tag {
                    let canonicalizer = LocaleCanonicalizer::new_extended();
                    canonicalizer.canonicalize(&mut locale);
                    canonicalize_unicode_keyword_values(&mut locale);
                }

                // Apply options if provided
                let options_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                if !options_arg.is_undefined() {
                    let options = match interp.intl_coerce_options_to_object(&options_arg) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };

                    if let JsValue::Object(o) = &options {
                        // language override
                        let lang_val = match interp.get_object_property(o.id, "language", &options)
                        {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => JsValue::Undefined,
                        };
                        if !lang_val.is_undefined() {
                            let lang_str = match interp.to_string_value(&lang_val) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };
                            if let Ok(lang) = lang_str.parse::<icu::locale::subtags::Language>() {
                                locale.id.language = lang;
                            } else {
                                return Completion::Throw(interp.create_range_error(&format!(
                                    "Invalid language subtag: {}",
                                    lang_str
                                )));
                            }
                        }

                        // script override
                        let script_val = match interp.get_object_property(o.id, "script", &options)
                        {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => JsValue::Undefined,
                        };
                        if !script_val.is_undefined() {
                            let script_str = match interp.to_string_value(&script_val) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };
                            if let Ok(scr) = script_str.parse::<icu::locale::subtags::Script>() {
                                locale.id.script = Some(scr);
                            } else {
                                return Completion::Throw(interp.create_range_error(&format!(
                                    "Invalid script subtag: {}",
                                    script_str
                                )));
                            }
                        }

                        // region override
                        let region_val = match interp.get_object_property(o.id, "region", &options)
                        {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => JsValue::Undefined,
                        };
                        if !region_val.is_undefined() {
                            let region_str = match interp.to_string_value(&region_val) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };
                            if let Ok(reg) = region_str.parse::<icu::locale::subtags::Region>() {
                                locale.id.region = Some(reg);
                            } else {
                                return Completion::Throw(interp.create_range_error(&format!(
                                    "Invalid region subtag: {}",
                                    region_str
                                )));
                            }
                        }

                        // variants override
                        let variants_val =
                            match interp.get_object_property(o.id, "variants", &options) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => JsValue::Undefined,
                            };
                        if !variants_val.is_undefined() {
                            let variants_str = match interp.to_string_value(&variants_val) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };
                            if !is_valid_variants_value(&variants_str) {
                                return Completion::Throw(interp.create_range_error(&format!(
                                    "Invalid variants subtag: {}",
                                    variants_str
                                )));
                            }
                            let mut new_base = locale.id.language.to_string();
                            if let Some(s) = locale.id.script {
                                new_base.push('-');
                                new_base.push_str(&s.to_string());
                            }
                            if let Some(r) = locale.id.region {
                                new_base.push('-');
                                new_base.push_str(&r.to_string());
                            }
                            new_base.push('-');
                            new_base.push_str(&variants_str);
                            let ext_str = {
                                let full = locale.to_string();
                                let base_end = full
                                    .find("-u-")
                                    .or_else(|| full.find("-x-"))
                                    .or_else(|| full.find("-t-"));
                                base_end.map(|i| full[i..].to_string())
                            };
                            if let Some(ext) = ext_str {
                                new_base.push_str(&ext);
                            }
                            match new_base.parse::<IcuLocale>() {
                                Ok(new_loc) => locale = new_loc,
                                Err(_) => {
                                    return Completion::Throw(interp.create_range_error(&format!(
                                        "Invalid variants subtag: {}",
                                        variants_str
                                    )));
                                }
                            }
                        }

                        // calendar (ca)
                        let cal_val = match interp.get_object_property(o.id, "calendar", &options) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => JsValue::Undefined,
                        };
                        if !cal_val.is_undefined() {
                            let s = match interp.to_string_value(&cal_val) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };
                            if !is_valid_unicode_type_value(&s) {
                                return Completion::Throw(interp.create_range_error(&format!(
                                    "Invalid calendar value: {}",
                                    s
                                )));
                            }
                            set_unicode_keyword(&mut locale, "ca", &s);
                        }

                        // collation (co)
                        let col_val = match interp.get_object_property(o.id, "collation", &options)
                        {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => JsValue::Undefined,
                        };
                        if !col_val.is_undefined() {
                            let s = match interp.to_string_value(&col_val) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };
                            if !is_valid_unicode_type_value(&s) {
                                return Completion::Throw(interp.create_range_error(&format!(
                                    "Invalid collation value: {}",
                                    s
                                )));
                            }
                            set_unicode_keyword(&mut locale, "co", &s);
                        }

                        // firstDayOfWeek (fw)
                        let fw_val =
                            match interp.get_object_property(o.id, "firstDayOfWeek", &options) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => JsValue::Undefined,
                            };
                        if !fw_val.is_undefined() {
                            let fw_string = match interp.to_string_value(&fw_val) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };

                            // Convert numeric strings/values to day names
                            let resolved = if let Some(day) = string_digit_to_weekday(&fw_string) {
                                day.to_string()
                            } else {
                                fw_string.to_ascii_lowercase()
                            };

                            // Validate: must be a valid unicode type value (3-8 alphanum parts)
                            if !is_valid_unicode_type_value(&resolved) {
                                return Completion::Throw(interp.create_range_error(&format!(
                                    "Invalid firstDayOfWeek value: {}",
                                    resolved
                                )));
                            }
                            set_unicode_keyword(&mut locale, "fw", &resolved);
                        }

                        // hourCycle (hc)
                        let hc_val = match interp.get_object_property(o.id, "hourCycle", &options) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => JsValue::Undefined,
                        };
                        if !hc_val.is_undefined() {
                            let s = match interp.to_string_value(&hc_val) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };
                            if !["h11", "h12", "h23", "h24"].contains(&s.as_str()) {
                                return Completion::Throw(interp.create_range_error(&format!(
                                    "Invalid hourCycle value: {}",
                                    s
                                )));
                            }
                            set_unicode_keyword(&mut locale, "hc", &s);
                        }

                        // caseFirst (kf)
                        let kf_val = match interp.get_object_property(o.id, "caseFirst", &options) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => JsValue::Undefined,
                        };
                        if !kf_val.is_undefined() {
                            let s = match interp.to_string_value(&kf_val) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };
                            if !["upper", "lower", "false"].contains(&s.as_str()) {
                                return Completion::Throw(interp.create_range_error(&format!(
                                    "Invalid caseFirst value: {}",
                                    s
                                )));
                            }
                            set_unicode_keyword(&mut locale, "kf", &s);
                        }

                        // numeric (kn)
                        let numeric_val =
                            match interp.get_object_property(o.id, "numeric", &options) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => JsValue::Undefined,
                            };
                        if !numeric_val.is_undefined() {
                            let b = interp.to_boolean_val(&numeric_val);
                            let kn_val = if b { "true" } else { "false" };
                            set_unicode_keyword(&mut locale, "kn", kn_val);
                        }

                        // numberingSystem (nu)
                        let nu_val =
                            match interp.get_object_property(o.id, "numberingSystem", &options) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => JsValue::Undefined,
                            };
                        if !nu_val.is_undefined() {
                            let s = match interp.to_string_value(&nu_val) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };
                            if !is_valid_unicode_type_value(&s) {
                                return Completion::Throw(interp.create_range_error(&format!(
                                    "Invalid numberingSystem value: {}",
                                    s
                                )));
                            }
                            set_unicode_keyword(&mut locale, "nu", &s);
                        }
                    }
                }

                if is_fallback_tag {
                    let obj = interp.create_object();
                    obj.borrow_mut().prototype = Some(proto_clone.clone());
                    obj.borrow_mut().class_name = "Intl.Locale".to_string();
                    let lower_tag = tag_string.to_ascii_lowercase();
                    obj.borrow_mut().intl_data = Some(IntlData::Locale {
                        tag: lower_tag.clone(),
                        language: lower_tag,
                        script: None,
                        region: None,
                        variants: None,
                        calendar: None,
                        collation: None,
                        hour_cycle: None,
                        case_first: None,
                        numeric: None,
                        numbering_system: None,
                        first_day_of_week: None,
                    });
                    let obj_id = obj.borrow().id.unwrap();
                    Completion::Normal(JsValue::Object(crate::types::JsObject { id: obj_id }))
                } else {
                    // Canonicalize again after applying options
                    let canonicalizer = LocaleCanonicalizer::new_extended();
                    canonicalizer.canonicalize(&mut locale);
                    canonicalize_unicode_keyword_values(&mut locale);

                    let obj = interp.create_object();
                    obj.borrow_mut().prototype = Some(proto_clone.clone());
                    obj.borrow_mut().class_name = "Intl.Locale".to_string();
                    obj.borrow_mut().intl_data = Some(build_intl_data_from_locale(&locale));
                    let obj_id = obj.borrow().id.unwrap();
                    Completion::Normal(JsValue::Object(crate::types::JsObject { id: obj_id }))
                }
            },
        ));

        // Set Locale.prototype on constructor
        if let JsValue::Object(ctor_ref) = &locale_ctor
            && let Some(obj) = self.get_object(ctor_ref.id)
        {
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

        // Set constructor on prototype
        proto.borrow_mut().insert_property(
            "constructor".to_string(),
            PropertyDescriptor::data(locale_ctor.clone(), true, false, true),
        );

        // Register Intl.Locale on the Intl namespace
        intl_obj.borrow_mut().insert_property(
            "Locale".to_string(),
            PropertyDescriptor::data(locale_ctor, true, false, true),
        );
    }
}

fn get_timezones_for_region(region: &str) -> Vec<&'static str> {
    match region {
        "US" => vec![
            "America/Adak",
            "America/Anchorage",
            "America/Boise",
            "America/Chicago",
            "America/Denver",
            "America/Detroit",
            "America/Indiana/Knox",
            "America/Indiana/Marengo",
            "America/Indiana/Petersburg",
            "America/Indiana/Tell_City",
            "America/Indiana/Vevay",
            "America/Indiana/Vincennes",
            "America/Indiana/Winamac",
            "America/Indianapolis",
            "America/Juneau",
            "America/Kentucky/Monticello",
            "America/Los_Angeles",
            "America/Louisville",
            "America/Menominee",
            "America/Metlakatla",
            "America/New_York",
            "America/Nome",
            "America/North_Dakota/Beulah",
            "America/North_Dakota/Center",
            "America/North_Dakota/New_Salem",
            "America/Phoenix",
            "America/Sitka",
            "America/Yakutat",
            "Pacific/Honolulu",
        ],
        "GB" => vec!["Europe/London"],
        "FR" => vec!["Europe/Paris"],
        "DE" => vec!["Europe/Berlin", "Europe/Busingen"],
        "JP" => vec!["Asia/Tokyo"],
        "CN" => vec!["Asia/Shanghai", "Asia/Urumqi"],
        "IN" => vec!["Asia/Kolkata"],
        "RU" => vec![
            "Asia/Anadyr",
            "Asia/Barnaul",
            "Asia/Chita",
            "Asia/Irkutsk",
            "Asia/Kamchatka",
            "Asia/Khandyga",
            "Asia/Krasnoyarsk",
            "Asia/Magadan",
            "Asia/Novokuznetsk",
            "Asia/Novosibirsk",
            "Asia/Omsk",
            "Asia/Sakhalin",
            "Asia/Srednekolymsk",
            "Asia/Tomsk",
            "Asia/Ust-Nera",
            "Asia/Vladivostok",
            "Asia/Yakutsk",
            "Asia/Yekaterinburg",
            "Europe/Astrakhan",
            "Europe/Kaliningrad",
            "Europe/Kirov",
            "Europe/Moscow",
            "Europe/Samara",
            "Europe/Saratov",
            "Europe/Ulyanovsk",
            "Europe/Volgograd",
        ],
        "AU" => vec![
            "Antarctica/Macquarie",
            "Australia/Adelaide",
            "Australia/Brisbane",
            "Australia/Broken_Hill",
            "Australia/Darwin",
            "Australia/Eucla",
            "Australia/Hobart",
            "Australia/Lindeman",
            "Australia/Lord_Howe",
            "Australia/Melbourne",
            "Australia/Perth",
            "Australia/Sydney",
        ],
        "BR" => vec![
            "America/Araguaina",
            "America/Bahia",
            "America/Belem",
            "America/Boa_Vista",
            "America/Campo_Grande",
            "America/Cuiaba",
            "America/Eirunepe",
            "America/Fortaleza",
            "America/Maceio",
            "America/Manaus",
            "America/Noronha",
            "America/Porto_Velho",
            "America/Recife",
            "America/Rio_Branco",
            "America/Santarem",
            "America/Sao_Paulo",
        ],
        "CA" => vec![
            "America/Atikokan",
            "America/Blanc-Sablon",
            "America/Cambridge_Bay",
            "America/Creston",
            "America/Dawson",
            "America/Dawson_Creek",
            "America/Edmonton",
            "America/Fort_Nelson",
            "America/Glace_Bay",
            "America/Goose_Bay",
            "America/Halifax",
            "America/Inuvik",
            "America/Iqaluit",
            "America/Moncton",
            "America/Rankin_Inlet",
            "America/Regina",
            "America/Resolute",
            "America/St_Johns",
            "America/Swift_Current",
            "America/Toronto",
            "America/Vancouver",
            "America/Whitehorse",
            "America/Winnipeg",
        ],
        "MX" => vec![
            "America/Bahia_Banderas",
            "America/Cancun",
            "America/Chihuahua",
            "America/Ciudad_Juarez",
            "America/Hermosillo",
            "America/Matamoros",
            "America/Mazatlan",
            "America/Merida",
            "America/Mexico_City",
            "America/Monterrey",
            "America/Ojinaga",
            "America/Tijuana",
        ],
        "NZ" => vec!["Pacific/Auckland", "Pacific/Chatham"],
        "ES" => vec!["Africa/Ceuta", "Atlantic/Canary", "Europe/Madrid"],
        "PT" => vec!["Atlantic/Azores", "Atlantic/Madeira", "Europe/Lisbon"],
        "IT" => vec!["Europe/Rome"],
        "KR" => vec!["Asia/Seoul"],
        "SG" => vec!["Asia/Singapore"],
        "HK" => vec!["Asia/Hong_Kong"],
        "TW" => vec!["Asia/Taipei"],
        "TH" => vec!["Asia/Bangkok"],
        "PH" => vec!["Asia/Manila"],
        "MY" => vec!["Asia/Kuala_Lumpur", "Asia/Kuching"],
        "ID" => vec![
            "Asia/Jakarta",
            "Asia/Jayapura",
            "Asia/Makassar",
            "Asia/Pontianak",
        ],
        "SA" => vec!["Asia/Riyadh"],
        "AE" => vec!["Asia/Dubai"],
        "EG" => vec!["Africa/Cairo"],
        "ZA" => vec!["Africa/Johannesburg"],
        "NG" => vec!["Africa/Lagos"],
        "KE" => vec!["Africa/Nairobi"],
        "IL" => vec!["Asia/Jerusalem"],
        "TR" => vec!["Europe/Istanbul"],
        "PL" => vec!["Europe/Warsaw"],
        "SE" => vec!["Europe/Stockholm"],
        "NO" => vec!["Europe/Oslo"],
        "FI" => vec!["Europe/Helsinki"],
        "DK" => vec!["Europe/Copenhagen"],
        "NL" => vec!["Europe/Amsterdam"],
        "BE" => vec!["Europe/Brussels"],
        "CH" => vec!["Europe/Zurich"],
        "AT" => vec!["Europe/Vienna"],
        "IE" => vec!["Europe/Dublin"],
        "GR" => vec!["Europe/Athens"],
        "CZ" => vec!["Europe/Prague"],
        "HU" => vec!["Europe/Budapest"],
        "RO" => vec!["Europe/Bucharest"],
        "UA" => vec!["Europe/Kyiv", "Europe/Simferopol"],
        "AR" => vec![
            "America/Argentina/Buenos_Aires",
            "America/Argentina/Catamarca",
            "America/Argentina/Cordoba",
            "America/Argentina/Jujuy",
            "America/Argentina/La_Rioja",
            "America/Argentina/Mendoza",
            "America/Argentina/Rio_Gallegos",
            "America/Argentina/Salta",
            "America/Argentina/San_Juan",
            "America/Argentina/San_Luis",
            "America/Argentina/Tucuman",
            "America/Argentina/Ushuaia",
        ],
        "CL" => vec!["America/Punta_Arenas", "America/Santiago", "Pacific/Easter"],
        "CO" => vec!["America/Bogota"],
        "PE" => vec!["America/Lima"],
        "VE" => vec!["America/Caracas"],
        "EC" => vec!["America/Guayaquil", "Pacific/Galapagos"],
        "PK" => vec!["Asia/Karachi"],
        "BD" => vec!["Asia/Dhaka"],
        "IR" => vec!["Asia/Tehran"],
        "IQ" => vec!["Asia/Baghdad"],
        "AM" => vec!["Asia/Yerevan"],
        "GE" => vec!["Asia/Tbilisi"],
        "AZ" => vec!["Asia/Baku"],
        "IS" => vec!["Atlantic/Reykjavik"],
        "CU" => vec!["America/Havana"],
        "ZZ" => vec!["Etc/Unknown"],
        _ => vec!["Etc/Unknown"],
    }
}
