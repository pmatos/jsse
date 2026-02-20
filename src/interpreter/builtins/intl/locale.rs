use super::super::super::*;
use icu::locale::extensions::unicode::{Key, Value};
use icu::locale::Locale as IcuLocale;
use icu::locale::LocaleExpander;

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
        // digit alphanum{3} (len=4 starting with digit)
        let is_digit_variant =
            len == 4 && part.chars().next().map_or(false, |c| c.is_ascii_digit());
        // alphanum{5,8}
        let is_alpha_variant = len >= 5 && len <= 8;
        if !is_digit_variant && !is_alpha_variant {
            return false;
        }
    }
    true
}

fn build_intl_data_from_locale(locale: &IcuLocale) -> IntlData {
    let numeric_str = extract_unicode_keyword(locale, "kn");
    let numeric = numeric_str.map(|s| s == "true" || s.is_empty());

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

        // --- Getter accessors (all enumerable: false, configurable: true per spec) ---

        // baseName
        let getter = self.create_function(JsFunction::native(
            "get baseName".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let b = obj.borrow();
                    if let Some(IntlData::Locale {
                        ref language,
                        ref script,
                        ref region,
                        ref variants,
                        ..
                    }) = b.intl_data
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
                        return Completion::Normal(JsValue::String(JsString::from_str(&result)));
                    }
                }
                Completion::Throw(
                    interp.create_type_error(
                        "Intl.Locale.prototype.baseName requires an Intl.Locale object",
                    ),
                )
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
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let b = obj.borrow();
                    if let Some(IntlData::Locale { ref calendar, .. }) = b.intl_data {
                        return Completion::Normal(match calendar {
                            Some(s) => JsValue::String(JsString::from_str(s)),
                            None => JsValue::Undefined,
                        });
                    }
                }
                Completion::Throw(
                    interp.create_type_error(
                        "Intl.Locale.prototype.calendar requires an Intl.Locale object",
                    ),
                )
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
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let b = obj.borrow();
                    if let Some(IntlData::Locale { ref case_first, .. }) = b.intl_data {
                        return Completion::Normal(match case_first {
                            Some(s) => JsValue::String(JsString::from_str(s)),
                            None => JsValue::Undefined,
                        });
                    }
                }
                Completion::Throw(
                    interp.create_type_error(
                        "Intl.Locale.prototype.caseFirst requires an Intl.Locale object",
                    ),
                )
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
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let b = obj.borrow();
                    if let Some(IntlData::Locale { ref collation, .. }) = b.intl_data {
                        return Completion::Normal(match collation {
                            Some(s) => JsValue::String(JsString::from_str(s)),
                            None => JsValue::Undefined,
                        });
                    }
                }
                Completion::Throw(
                    interp.create_type_error(
                        "Intl.Locale.prototype.collation requires an Intl.Locale object",
                    ),
                )
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
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let b = obj.borrow();
                    if let Some(IntlData::Locale { ref hour_cycle, .. }) = b.intl_data {
                        return Completion::Normal(match hour_cycle {
                            Some(s) => JsValue::String(JsString::from_str(s)),
                            None => JsValue::Undefined,
                        });
                    }
                }
                Completion::Throw(
                    interp.create_type_error(
                        "Intl.Locale.prototype.hourCycle requires an Intl.Locale object",
                    ),
                )
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
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let b = obj.borrow();
                    if let Some(IntlData::Locale { ref language, .. }) = b.intl_data {
                        return Completion::Normal(JsValue::String(JsString::from_str(language)));
                    }
                }
                Completion::Throw(
                    interp.create_type_error(
                        "Intl.Locale.prototype.language requires an Intl.Locale object",
                    ),
                )
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
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let b = obj.borrow();
                    if let Some(IntlData::Locale {
                        ref numbering_system,
                        ..
                    }) = b.intl_data
                    {
                        return Completion::Normal(match numbering_system {
                            Some(s) => JsValue::String(JsString::from_str(s)),
                            None => JsValue::Undefined,
                        });
                    }
                }
                Completion::Throw(
                    interp.create_type_error(
                        "Intl.Locale.prototype.numberingSystem requires an Intl.Locale object",
                    ),
                )
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
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let b = obj.borrow();
                    if let Some(IntlData::Locale { ref numeric, .. }) = b.intl_data {
                        return Completion::Normal(JsValue::Boolean(numeric.unwrap_or(false)));
                    }
                }
                Completion::Throw(
                    interp.create_type_error(
                        "Intl.Locale.prototype.numeric requires an Intl.Locale object",
                    ),
                )
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
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let b = obj.borrow();
                    if let Some(IntlData::Locale { ref region, .. }) = b.intl_data {
                        return Completion::Normal(match region {
                            Some(s) => JsValue::String(JsString::from_str(s)),
                            None => JsValue::Undefined,
                        });
                    }
                }
                Completion::Throw(
                    interp.create_type_error(
                        "Intl.Locale.prototype.region requires an Intl.Locale object",
                    ),
                )
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
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let b = obj.borrow();
                    if let Some(IntlData::Locale { ref script, .. }) = b.intl_data {
                        return Completion::Normal(match script {
                            Some(s) => JsValue::String(JsString::from_str(s)),
                            None => JsValue::Undefined,
                        });
                    }
                }
                Completion::Throw(
                    interp.create_type_error(
                        "Intl.Locale.prototype.script requires an Intl.Locale object",
                    ),
                )
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
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let b = obj.borrow();
                    if let Some(IntlData::Locale { ref variants, .. }) = b.intl_data {
                        return Completion::Normal(match variants {
                            Some(s) => JsValue::String(JsString::from_str(s)),
                            None => JsValue::Undefined,
                        });
                    }
                }
                Completion::Throw(
                    interp.create_type_error(
                        "Intl.Locale.prototype.variants requires an Intl.Locale object",
                    ),
                )
            },
        ));
        proto.borrow_mut().insert_property(
            "variants".to_string(),
            PropertyDescriptor::accessor(Some(getter), None, false, true),
        );

        // --- Prototype methods ---

        // toString()
        let to_string_fn = self.create_function(JsFunction::native(
            "toString".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let b = obj.borrow();
                    if let Some(IntlData::Locale { ref tag, .. }) = b.intl_data {
                        return Completion::Normal(JsValue::String(JsString::from_str(tag)));
                    }
                }
                Completion::Throw(
                    interp.create_type_error(
                        "Intl.Locale.prototype.toString requires an Intl.Locale object",
                    ),
                )
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toString".to_string(), to_string_fn);

        // toJSON() -- same as toString()
        let to_json_fn = self.create_function(JsFunction::native(
            "toJSON".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let b = obj.borrow();
                    if let Some(IntlData::Locale { ref tag, .. }) = b.intl_data {
                        return Completion::Normal(JsValue::String(JsString::from_str(tag)));
                    }
                }
                Completion::Throw(
                    interp.create_type_error(
                        "Intl.Locale.prototype.toJSON requires an Intl.Locale object",
                    ),
                )
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

                    let mut locale: IcuLocale = match tag.parse() {
                        Ok(l) => l,
                        Err(_) => {
                            return Completion::Throw(
                                interp.create_range_error(&format!(
                                    "Invalid language tag: {}",
                                    tag
                                )),
                            );
                        }
                    };

                    let expander = LocaleExpander::new_extended();
                    expander.maximize(&mut locale.id);

                    return Completion::Normal(create_locale_object_from_icu(interp, &locale));
                }
                Completion::Throw(
                    interp.create_type_error(
                        "Intl.Locale.prototype.maximize requires an Intl.Locale object",
                    ),
                )
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

                    let mut locale: IcuLocale = match tag.parse() {
                        Ok(l) => l,
                        Err(_) => {
                            return Completion::Throw(
                                interp.create_range_error(&format!(
                                    "Invalid language tag: {}",
                                    tag
                                )),
                            );
                        }
                    };

                    let expander = LocaleExpander::new_extended();
                    expander.minimize(&mut locale.id);

                    return Completion::Normal(create_locale_object_from_icu(interp, &locale));
                }
                Completion::Throw(
                    interp.create_type_error(
                        "Intl.Locale.prototype.minimize requires an Intl.Locale object",
                    ),
                )
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("minimize".to_string(), minimize_fn);

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
                    JsValue::Undefined => {
                        return Completion::Throw(interp.create_type_error(
                            "First argument to Intl.Locale must be a string or object",
                        ));
                    }
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

                let mut locale: IcuLocale = match tag_string.parse() {
                    Ok(l) => l,
                    Err(_) => {
                        return Completion::Throw(interp.create_range_error(&format!(
                            "Invalid language tag: {}",
                            tag_string
                        )));
                    }
                };

                // Apply options if provided
                let options_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                if !options_arg.is_undefined() {
                    let options = match interp.intl_coerce_options_to_object(&options_arg) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };

                    if let JsValue::Object(o) = &options {
                        // Read options in spec order: language, script, region, variants,
                        // then calendar, collation, hourCycle, caseFirst, numeric, numberingSystem

                        // language override
                        let lang_val =
                            match interp.get_object_property(o.id, "language", &options) {
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
                        let script_val =
                            match interp.get_object_property(o.id, "script", &options) {
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
                        let region_val =
                            match interp.get_object_property(o.id, "region", &options) {
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
                            // Build a new tag string with the variants and re-parse
                            // to get canonical ordering
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
                            // Add extensions back
                            let ext_str = {
                                let full = locale.to_string();
                                let base_end = full.find("-u-")
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
                                    return Completion::Throw(
                                        interp.create_range_error(&format!(
                                            "Invalid variants subtag: {}",
                                            variants_str
                                        )),
                                    );
                                }
                            }
                        }

                        // Unicode extension keyword overrides with validation
                        // calendar (ca)
                        let cal_val =
                            match interp.get_object_property(o.id, "calendar", &options) {
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
                        let col_val =
                            match interp.get_object_property(o.id, "collation", &options) {
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

                        // hourCycle (hc) - restricted to h11, h12, h23, h24
                        let hc_val =
                            match interp.get_object_property(o.id, "hourCycle", &options) {
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

                        // caseFirst (kf) - restricted to upper, lower, false
                        let kf_val =
                            match interp.get_object_property(o.id, "caseFirst", &options) {
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

                        // numeric (boolean) -> kn extension
                        let numeric_val =
                            match interp.get_object_property(o.id, "numeric", &options) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => JsValue::Undefined,
                            };
                        if !numeric_val.is_undefined() {
                            let b = to_boolean(&numeric_val);
                            let kn_val = if b { "true" } else { "false" };
                            set_unicode_keyword(&mut locale, "kn", kn_val);
                        }

                        // numberingSystem (nu)
                        let nu_val =
                            match interp.get_object_property(o.id, "numberingSystem", &options)
                            {
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

                let obj = interp.create_object();
                obj.borrow_mut().prototype = Some(proto_clone.clone());
                obj.borrow_mut().class_name = "Intl.Locale".to_string();
                obj.borrow_mut().intl_data = Some(build_intl_data_from_locale(&locale));
                let obj_id = obj.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id: obj_id }))
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
