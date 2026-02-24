use super::super::super::*;
use icu::collator::options::{AlternateHandling, CaseLevel, CollatorOptions, Strength};
use icu::collator::preferences::{CollationCaseFirst, CollationNumericOrdering, CollationType};
use icu::collator::{Collator, CollatorPreferences};
use icu::locale::Locale as IcuLocale;
use icu::normalizer::ComposingNormalizer;

fn extract_unicode_extension(locale_str: &str, key: &str) -> Option<String> {
    let lower = locale_str.to_lowercase();
    // Strip private-use tags before searching for unicode extensions
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
            // Check if next token is a value (not another key)
            if i + 1 < tokens.len() && tokens[i + 1].len() > 2 {
                return Some(tokens[i + 1].to_string());
            }
            if i + 1 < tokens.len() && tokens[i + 1].len() == 2 {
                // Next token is another key, this key has no value (boolean true)
                return Some("true".to_string());
            }
            if i + 1 < tokens.len() {
                let next = tokens[i + 1];
                if next == "true" || next == "false" {
                    return Some(next.to_string());
                }
                return Some(next.to_string());
            }
            return Some("true".to_string());
        }
    }
    None
}

fn strip_unicode_extensions(locale_str: &str) -> String {
    // Don't look for -u- inside private use tags (-x-)
    let search_end = locale_str.find("-x-").unwrap_or(locale_str.len());
    let search_part = &locale_str[..search_end];
    if let Some(idx) = search_part.find("-u-") {
        let before = &locale_str[..idx];
        let after_u = &locale_str[idx + 3..];
        let tokens: Vec<&str> = after_u.split('-').collect();
        let mut end_of_u = tokens.len();
        for i in 0..tokens.len() {
            // A single-letter token that's not 'u' starts a new extension section
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
    let parsed: Result<IcuLocale, _> = strip_unicode_extensions(locale_str).parse();
    match parsed {
        Ok(loc) => loc.to_string(),
        Err(_) => strip_unicode_extensions(locale_str),
    }
}

pub(crate) fn do_compare(
    locale_str: &str,
    usage: &str,
    collation: &str,
    sensitivity: &str,
    numeric: bool,
    case_first: &str,
    ignore_punctuation: bool,
    x: &str,
    y: &str,
) -> f64 {
    let base = base_locale(locale_str);
    let locale: IcuLocale = base.parse().unwrap_or_else(|_| "en".parse().unwrap());

    let normalizer = ComposingNormalizer::new_nfc();
    let x_nfc = normalizer.normalize(x);
    let y_nfc = normalizer.normalize(y);

    let mut opts = CollatorOptions::default();
    match sensitivity {
        "base" => {
            opts.strength = Some(Strength::Primary);
        }
        "accent" => {
            opts.strength = Some(Strength::Secondary);
        }
        "case" => {
            opts.strength = Some(Strength::Primary);
            opts.case_level = Some(CaseLevel::On);
        }
        _ => {
            opts.strength = Some(Strength::Tertiary);
        }
    }
    if ignore_punctuation {
        opts.alternate_handling = Some(AlternateHandling::Shifted);
    } else {
        opts.alternate_handling = Some(AlternateHandling::NonIgnorable);
    }

    let mut prefs = CollatorPreferences::from(&locale);

    if usage == "search" {
        prefs.collation_type = Some(CollationType::Search);
    } else if collation != "default" {
        match collation {
            "phonebk" => prefs.collation_type = Some(CollationType::Phonebk),
            "dict" => prefs.collation_type = Some(CollationType::Dict),
            "compat" => prefs.collation_type = Some(CollationType::Compat),
            "emoji" => prefs.collation_type = Some(CollationType::Emoji),
            "eor" => prefs.collation_type = Some(CollationType::Eor),
            "phonetic" => prefs.collation_type = Some(CollationType::Phonetic),
            "pinyin" => prefs.collation_type = Some(CollationType::Pinyin),
            "searchjl" => prefs.collation_type = Some(CollationType::Searchjl),
            "stroke" => prefs.collation_type = Some(CollationType::Stroke),
            "trad" => prefs.collation_type = Some(CollationType::Trad),
            "unihan" => prefs.collation_type = Some(CollationType::Unihan),
            "zhuyin" => prefs.collation_type = Some(CollationType::Zhuyin),
            _ => {}
        }
    }

    if numeric {
        prefs.numeric_ordering = Some(CollationNumericOrdering::True);
    } else {
        prefs.numeric_ordering = Some(CollationNumericOrdering::False);
    }
    match case_first {
        "upper" => {
            prefs.case_first = Some(CollationCaseFirst::Upper);
        }
        "lower" => {
            prefs.case_first = Some(CollationCaseFirst::Lower);
        }
        _ => {
            prefs.case_first = Some(CollationCaseFirst::False);
        }
    }

    let collator = Collator::try_new(prefs, opts).unwrap_or_else(|_| {
        let fallback_prefs: CollatorPreferences = Default::default();
        Collator::try_new(fallback_prefs, opts).unwrap()
    });

    match collator.compare(&x_nfc, &y_nfc) {
        std::cmp::Ordering::Less => -1.0,
        std::cmp::Ordering::Equal => 0.0,
        std::cmp::Ordering::Greater => 1.0,
    }
}

fn collation_type_for_name(name: &str) -> Option<CollationType> {
    match name {
        "phonebk" => Some(CollationType::Phonebk),
        "dict" => Some(CollationType::Dict),
        "compat" => Some(CollationType::Compat),
        "emoji" => Some(CollationType::Emoji),
        "eor" => Some(CollationType::Eor),
        "phonetic" => Some(CollationType::Phonetic),
        "pinyin" => Some(CollationType::Pinyin),
        "searchjl" => Some(CollationType::Searchjl),
        "stroke" => Some(CollationType::Stroke),
        "trad" => Some(CollationType::Trad),
        "unihan" => Some(CollationType::Unihan),
        "zhuyin" => Some(CollationType::Zhuyin),
        _ => None,
    }
}

fn is_collation_supported_for_locale(locale_str: &str, collation_name: &str) -> bool {
    // "eor" and "emoji" are universally supported (root collation data)
    if collation_name == "eor" || collation_name == "emoji" {
        return true;
    }

    let base = base_locale(locale_str);
    let lang = base.split('-').next().unwrap_or("");

    // Locale-specific collation support based on CLDR data
    match lang {
        "ar" => matches!(collation_name, "compat"),
        "bg" | "mk" | "ru" | "uk" | "be" => false,
        "da" | "nb" | "nn" | "no" | "sv" => matches!(collation_name, "trad"),
        "de" => matches!(collation_name, "phonebk"),
        "es" => matches!(collation_name, "trad"),
        "fi" => matches!(collation_name, "trad"),
        "hi" => false,
        "ja" => matches!(collation_name, "unihan"),
        "ko" => matches!(collation_name, "searchjl" | "unihan"),
        "ln" => matches!(collation_name, "phonetic"),
        "si" => matches!(collation_name, "dict"),
        "th" => false,
        "tr" => false,
        "zh" => matches!(
            collation_name,
            "big5han" | "pinyin" | "stroke" | "unihan" | "zhuyin"
        ),
        _ => false,
    }
}

fn is_thai_locale(locale_str: &str) -> bool {
    let lower = locale_str.to_lowercase();
    lower == "th" || lower.starts_with("th-")
}

impl Interpreter {
    pub(crate) fn setup_intl_collator(&mut self, intl_obj: &Rc<RefCell<JsObjectData>>) {
        let proto = self.create_object();
        if let Some(ref op) = self.object_prototype {
            proto.borrow_mut().prototype = Some(op.clone());
        }
        proto.borrow_mut().class_name = "Intl.Collator".to_string();

        // @@toStringTag
        proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("Intl.Collator"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            },
        );

        // compare getter
        let compare_getter = self.create_function(JsFunction::native(
            "get compare".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this {
                    if let Some(obj) = interp.get_object(o.id) {
                        let cached = {
                            let b = obj.borrow();
                            if !matches!(b.intl_data, Some(IntlData::Collator { .. })) {
                                return Completion::Throw(interp.create_type_error(
                                    "Intl.Collator.prototype.compare called on non-Collator object",
                                ));
                            }
                            b.properties
                                .get("[[BoundCompare]]")
                                .and_then(|pd| pd.value.clone())
                        };

                        if let Some(func) = cached {
                            return Completion::Normal(func);
                        }
                    }

                    let (
                        locale,
                        usage,
                        collation_val,
                        sensitivity,
                        numeric,
                        case_first,
                        ignore_punctuation,
                    ) = {
                        if let Some(obj) = interp.get_object(o.id) {
                            let b = obj.borrow();
                            if let Some(IntlData::Collator {
                                ref locale,
                                ref usage,
                                ref collation,
                                ref sensitivity,
                                ref ignore_punctuation,
                                ref numeric,
                                ref case_first,
                                ..
                            }) = b.intl_data
                            {
                                (
                                    locale.clone(),
                                    usage.clone(),
                                    collation.clone(),
                                    sensitivity.clone(),
                                    *numeric,
                                    case_first.clone(),
                                    *ignore_punctuation,
                                )
                            } else {
                                return Completion::Throw(interp.create_type_error(
                                    "Intl.Collator.prototype.compare called on non-Collator object",
                                ));
                            }
                        } else {
                            return Completion::Throw(interp.create_type_error(
                                "Intl.Collator.prototype.compare called on non-Collator object",
                            ));
                        }
                    };

                    let compare_fn = interp.create_function(JsFunction::native(
                        "".to_string(),
                        2,
                        move |interp2, _this2, args2| {
                            let x_val = args2.first().cloned().unwrap_or(JsValue::Undefined);
                            let y_val = args2.get(1).cloned().unwrap_or(JsValue::Undefined);
                            let x_str = match interp2.to_string_value(&x_val) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };
                            let y_str = match interp2.to_string_value(&y_val) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };

                            let result = do_compare(
                                &locale,
                                &usage,
                                &collation_val,
                                &sensitivity,
                                numeric,
                                &case_first,
                                ignore_punctuation,
                                &x_str,
                                &y_str,
                            );
                            Completion::Normal(JsValue::Number(result))
                        },
                    ));

                    if let Some(obj) = interp.get_object(o.id) {
                        obj.borrow_mut().properties.insert(
                            "[[BoundCompare]]".to_string(),
                            PropertyDescriptor::data(compare_fn.clone(), false, false, false),
                        );
                    }

                    return Completion::Normal(compare_fn);
                }
                Completion::Throw(interp.create_type_error(
                    "Intl.Collator.prototype.compare called on non-Collator object",
                ))
            },
        ));
        proto.borrow_mut().insert_property(
            "compare".to_string(),
            PropertyDescriptor::accessor(Some(compare_getter), None, false, true),
        );

        // resolvedOptions()
        let resolved_fn = self.create_function(JsFunction::native(
            "resolvedOptions".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this {
                    if let Some(obj) = interp.get_object(o.id) {
                        let data = {
                            let b = obj.borrow();
                            b.intl_data.clone()
                        };
                        if let Some(IntlData::Collator {
                            locale,
                            usage,
                            sensitivity,
                            ignore_punctuation,
                            collation,
                            numeric,
                            case_first,
                        }) = data
                        {
                            let result = interp.create_object();
                            if let Some(ref op) = interp.object_prototype {
                                result.borrow_mut().prototype = Some(op.clone());
                            }

                            let props = vec![
                                ("locale", JsValue::String(JsString::from_str(&locale))),
                                ("usage", JsValue::String(JsString::from_str(&usage))),
                                (
                                    "sensitivity",
                                    JsValue::String(JsString::from_str(&sensitivity)),
                                ),
                                ("ignorePunctuation", JsValue::Boolean(ignore_punctuation)),
                                ("collation", JsValue::String(JsString::from_str(&collation))),
                                ("numeric", JsValue::Boolean(numeric)),
                                (
                                    "caseFirst",
                                    JsValue::String(JsString::from_str(&case_first)),
                                ),
                            ];
                            for (key, val) in props {
                                result.borrow_mut().insert_property(
                                    key.to_string(),
                                    PropertyDescriptor::data(val, true, true, true),
                                );
                            }

                            let result_id = result.borrow().id.unwrap();
                            return Completion::Normal(JsValue::Object(crate::types::JsObject {
                                id: result_id,
                            }));
                        }
                    }
                }
                Completion::Throw(interp.create_type_error(
                    "Intl.Collator.prototype.resolvedOptions called on non-Collator object",
                ))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("resolvedOptions".to_string(), resolved_fn);

        self.intl_collator_prototype = Some(proto.clone());

        // --- Constructor ---
        let proto_id = proto.borrow().id.unwrap();
        let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });
        let proto_clone = proto.clone();

        let collator_ctor = self.create_function(JsFunction::constructor(
            "Collator".to_string(),
            0,
            move |interp, _this, args| {
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

                let usage = match interp.intl_get_option(
                    &options,
                    "usage",
                    &["sort", "search"],
                    Some("sort"),
                ) {
                    Ok(Some(v)) => v,
                    Ok(None) => "sort".to_string(),
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

                // Read options for collation, numeric, caseFirst
                let opt_collation = match interp.intl_get_option(&options, "collation", &[], None) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let opt_numeric = {
                    let num_val = if let JsValue::Object(o) = &options {
                        match interp.get_object_property(o.id, "numeric", &options) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => JsValue::Undefined,
                        }
                    } else {
                        JsValue::Undefined
                    };
                    if matches!(num_val, JsValue::Undefined) {
                        None
                    } else {
                        Some(interp.to_boolean_val(&num_val))
                    }
                };

                let opt_case_first = match interp.intl_get_option(
                    &options,
                    "caseFirst",
                    &["upper", "lower", "false"],
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let raw_locale = interp.intl_resolve_locale(&requested);

                // Extract unicode extension keys from the resolved locale
                let ext_kn = extract_unicode_extension(&raw_locale, "kn");
                let ext_kf = extract_unicode_extension(&raw_locale, "kf");
                let ext_co = extract_unicode_extension(&raw_locale, "co");

                // Resolve numeric: options > unicode extension > default
                let numeric = if let Some(n) = opt_numeric {
                    n
                } else if let Some(ref kn) = ext_kn {
                    kn != "false"
                } else {
                    false
                };

                // Resolve caseFirst: options > unicode extension > default
                let case_first = if let Some(ref cf) = opt_case_first {
                    cf.clone()
                } else if let Some(ref kf) = ext_kf {
                    match kf.as_str() {
                        "upper" | "lower" | "false" => kf.clone(),
                        _ => "false".to_string(),
                    }
                } else {
                    "false".to_string()
                };

                let valid_collations = [
                    "big5han", "compat", "dict", "emoji", "eor", "phonebk", "phonetic", "pinyin",
                    "searchjl", "stroke", "trad", "unihan", "zhuyin",
                ];

                // Resolve collation: options > unicode extension > default
                // Invalid values and "search"/"standard" map to "default"
                // Also check if the collation is actually supported for the locale
                let collation = {
                    let base_loc = base_locale(&raw_locale);
                    let mut resolved_co = "default".to_string();

                    if let Some(ref co) = opt_collation {
                        if valid_collations.contains(&co.as_str())
                            && is_collation_supported_for_locale(&base_loc, co)
                        {
                            resolved_co = co.clone();
                        }
                    }

                    if resolved_co == "default" {
                        if let Some(ref co) = ext_co {
                            if co != "search"
                                && co != "standard"
                                && valid_collations.contains(&co.as_str())
                                && is_collation_supported_for_locale(&base_loc, co)
                            {
                                resolved_co = co.clone();
                            }
                        }
                    }

                    resolved_co
                };

                // Build the resolved locale string:
                // Start with the base locale (no unicode extensions)
                let base = base_locale(&raw_locale);

                // Build unicode extension keys that should be reflected
                let mut ext_parts: Vec<String> = Vec::new();

                // Reflect kn if it came from extension and options didn't override
                if opt_numeric.is_none() {
                    if let Some(ref _kn) = ext_kn {
                        if numeric {
                            ext_parts.push("kn".to_string());
                        } else {
                            ext_parts.push("kn-false".to_string());
                        }
                    }
                } else if opt_numeric.is_some() && ext_kn.is_some() {
                    // Options override: only reflect if option matches extension
                    let ext_numeric = ext_kn.as_ref().map(|v| v != "false").unwrap_or(false);
                    if numeric == ext_numeric {
                        if numeric {
                            ext_parts.push("kn".to_string());
                        } else {
                            ext_parts.push("kn-false".to_string());
                        }
                    }
                }

                // Reflect kf if it came from extension and options didn't override
                if opt_case_first.is_none() {
                    if let Some(ref kf) = ext_kf {
                        match kf.as_str() {
                            "upper" | "lower" | "false" => {
                                ext_parts.push(format!("kf-{}", kf));
                            }
                            _ => {}
                        }
                    }
                } else if opt_case_first.is_some() && ext_kf.is_some() {
                    let ext_cf = ext_kf.as_ref().unwrap();
                    if &case_first == ext_cf {
                        ext_parts.push(format!("kf-{}", case_first));
                    }
                }

                // Reflect co in locale if the extension value is the one being used
                if collation != "default" {
                    if let Some(ref co) = ext_co {
                        if &collation == co {
                            ext_parts.push(format!("co-{}", collation));
                        }
                    }
                }

                ext_parts.sort();

                let locale = if ext_parts.is_empty() {
                    base
                } else {
                    format!("{}-u-{}", base, ext_parts.join("-"))
                };

                let sensitivity = match interp.intl_get_option(
                    &options,
                    "sensitivity",
                    &["base", "accent", "case", "variant"],
                    None,
                ) {
                    Ok(Some(v)) => v,
                    Ok(None) => "variant".to_string(),
                    Err(e) => return Completion::Throw(e),
                };

                // ignorePunctuation: boolean option with locale-dependent default
                let ignore_punctuation = {
                    let ip_val = if let JsValue::Object(o) = &options {
                        match interp.get_object_property(o.id, "ignorePunctuation", &options) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => JsValue::Undefined,
                        }
                    } else {
                        JsValue::Undefined
                    };
                    if matches!(ip_val, JsValue::Undefined) {
                        is_thai_locale(&locale)
                    } else {
                        interp.to_boolean_val(&ip_val)
                    }
                };

                let obj = interp.create_object();
                obj.borrow_mut().prototype = Some(proto_clone.clone());
                obj.borrow_mut().class_name = "Intl.Collator".to_string();
                obj.borrow_mut().intl_data = Some(IntlData::Collator {
                    locale,
                    usage,
                    sensitivity,
                    ignore_punctuation,
                    collation,
                    numeric,
                    case_first,
                });

                let obj_id = obj.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id: obj_id }))
            },
        ));

        // Set Collator.prototype on constructor
        if let JsValue::Object(ctor_ref) = &collator_ctor {
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
            PropertyDescriptor::data(collator_ctor.clone(), true, false, true),
        );

        // Register Intl.Collator on the Intl namespace
        intl_obj.borrow_mut().insert_property(
            "Collator".to_string(),
            PropertyDescriptor::data(collator_ctor, true, false, true),
        );
    }

    pub(crate) fn intl_locale_compare(
        &mut self,
        x: &str,
        y: &str,
        locales: &JsValue,
        options: &JsValue,
    ) -> Result<f64, JsValue> {
        let requested = self.intl_canonicalize_locale_list(locales)?;

        let opts = self.intl_coerce_options_to_object(options)?;

        let usage = self
            .intl_get_option(&opts, "usage", &["sort", "search"], Some("sort"))?
            .unwrap_or_else(|| "sort".to_string());

        let _locale_matcher = self.intl_get_option(
            &opts,
            "localeMatcher",
            &["lookup", "best fit"],
            Some("best fit"),
        )?;

        let opt_collation = self.intl_get_option(&opts, "collation", &[], None)?;

        let opt_numeric = {
            let num_val = if let JsValue::Object(o) = &opts {
                match self.get_object_property(o.id, "numeric", &opts) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::Undefined,
                }
            } else {
                JsValue::Undefined
            };
            if matches!(num_val, JsValue::Undefined) {
                None
            } else {
                Some(self.to_boolean_val(&num_val))
            }
        };

        let opt_case_first =
            self.intl_get_option(&opts, "caseFirst", &["upper", "lower", "false"], None)?;

        let raw_locale = self.intl_resolve_locale(&requested);

        let ext_kn = extract_unicode_extension(&raw_locale, "kn");
        let ext_co = extract_unicode_extension(&raw_locale, "co");

        let numeric = if let Some(n) = opt_numeric {
            n
        } else if let Some(ref kn) = ext_kn {
            kn != "false"
        } else {
            false
        };

        let case_first = if let Some(ref cf) = opt_case_first {
            cf.clone()
        } else {
            let ext_kf = extract_unicode_extension(&raw_locale, "kf");
            if let Some(ref kf) = ext_kf {
                match kf.as_str() {
                    "upper" | "lower" | "false" => kf.clone(),
                    _ => "false".to_string(),
                }
            } else {
                "false".to_string()
            }
        };

        let valid_collations = [
            "big5han", "compat", "dict", "emoji", "eor", "phonebk", "phonetic", "pinyin",
            "searchjl", "stroke", "trad", "unihan", "zhuyin",
        ];

        let collation = {
            let base_loc = base_locale(&raw_locale);
            let mut resolved_co = "default".to_string();

            if let Some(ref co) = opt_collation {
                if valid_collations.contains(&co.as_str())
                    && is_collation_supported_for_locale(&base_loc, co)
                {
                    resolved_co = co.clone();
                }
            }

            if resolved_co == "default" {
                if let Some(ref co) = ext_co {
                    if co != "search"
                        && co != "standard"
                        && valid_collations.contains(&co.as_str())
                        && is_collation_supported_for_locale(&base_loc, co)
                    {
                        resolved_co = co.clone();
                    }
                }
            }

            resolved_co
        };

        let sensitivity = self
            .intl_get_option(
                &opts,
                "sensitivity",
                &["base", "accent", "case", "variant"],
                None,
            )?
            .unwrap_or_else(|| "variant".to_string());

        let locale = base_locale(&raw_locale);

        let ignore_punctuation = {
            let ip_val = if let JsValue::Object(o) = &opts {
                match self.get_object_property(o.id, "ignorePunctuation", &opts) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::Undefined,
                }
            } else {
                JsValue::Undefined
            };
            if matches!(ip_val, JsValue::Undefined) {
                is_thai_locale(&locale)
            } else {
                self.to_boolean_val(&ip_val)
            }
        };

        Ok(do_compare(
            &locale,
            &usage,
            &collation,
            &sensitivity,
            numeric,
            &case_first,
            ignore_punctuation,
            x,
            y,
        ))
    }
}
