use super::super::super::*;
use fixed_decimal::{CompactDecimal, Decimal};
use icu::locale::Locale as IcuLocale;
use icu::plurals::{
    PluralCategory, PluralOperands, PluralRuleType, PluralRules as IcuPluralRules,
    PluralRulesOptions as IcuPluralRulesOptions, PluralRulesPreferences, PluralRulesWithRanges,
};

fn plural_category_to_str(cat: PluralCategory) -> &'static str {
    match cat {
        PluralCategory::Zero => "zero",
        PluralCategory::One => "one",
        PluralCategory::Two => "two",
        PluralCategory::Few => "few",
        PluralCategory::Many => "many",
        PluralCategory::Other => "other",
    }
}

fn number_to_plural_operands(n: f64) -> PluralOperands {
    if n.is_nan() || n.is_infinite() {
        return PluralOperands::from(0u64);
    }
    let abs = n.abs();
    if abs == abs.floor() && abs < u64::MAX as f64 {
        return PluralOperands::from(abs as u64);
    }
    let s = format!("{}", abs);
    let decimal: Decimal = s.parse().unwrap_or_else(|_| Decimal::from(0i32));
    PluralOperands::from(&decimal)
}

fn number_to_plural_operands_with_notation(n: f64, notation: &str) -> PluralOperands {
    if n.is_nan() || n.is_infinite() {
        return PluralOperands::from(0u64);
    }
    let abs = n.abs();

    match notation {
        "compact" => {
            if abs == 0.0 {
                return PluralOperands::from(0u64);
            }
            let exponent = if abs >= 1.0 {
                (abs.log10().floor() as u8 / 3) * 3
            } else {
                0
            };
            let significand = abs / 10f64.powi(exponent as i32);
            let sig_str = format!("{}", significand);
            let mut dec: Decimal = sig_str.parse().unwrap_or_else(|_| Decimal::from(0i32));
            dec.multiply_pow10(0);
            let compact = CompactDecimal::from_significand_and_exponent(dec, exponent);
            PluralOperands::from(&compact)
        }
        "scientific" | "engineering" => {
            if abs == 0.0 {
                return PluralOperands::from(0u64);
            }
            let log10 = abs.log10().floor() as i32;
            let exponent = match notation {
                "engineering" => (log10 / 3) * 3,
                _ => log10,
            };
            let significand = abs / 10f64.powi(exponent);
            let abs_exponent = exponent.unsigned_abs() as u8;
            let sig_str = format!("{}", significand);
            let mut dec: Decimal = sig_str.parse().unwrap_or_else(|_| Decimal::from(0i32));
            dec.multiply_pow10(0);
            let compact = CompactDecimal::from_significand_and_exponent(dec, abs_exponent);
            PluralOperands::from(&compact)
        }
        _ => number_to_plural_operands(n),
    }
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
    let parsed: Result<IcuLocale, _> = strip_unicode_extensions(locale_str).parse();
    match parsed {
        Ok(loc) => loc.to_string(),
        Err(_) => strip_unicode_extensions(locale_str),
    }
}

fn get_plural_categories_sorted(locale_str: &str, plural_type: &str) -> Vec<&'static str> {
    let base = base_locale(locale_str);
    let icu_locale: IcuLocale = base.parse().unwrap_or_else(|_| "en".parse().unwrap());
    let prefs = PluralRulesPreferences::from(&icu_locale);
    let mut opts = IcuPluralRulesOptions::default();
    opts.rule_type = Some(if plural_type == "ordinal" {
        PluralRuleType::Ordinal
    } else {
        PluralRuleType::Cardinal
    });

    let rules = match IcuPluralRules::try_new(prefs, opts) {
        Ok(r) => r,
        Err(_) => return vec!["other"],
    };

    // Spec order: zero, one, two, few, many, other
    let spec_order = [
        PluralCategory::Zero,
        PluralCategory::One,
        PluralCategory::Two,
        PluralCategory::Few,
        PluralCategory::Many,
        PluralCategory::Other,
    ];

    let available: Vec<PluralCategory> = rules.categories().collect();
    spec_order
        .iter()
        .filter(|c| available.contains(c))
        .map(|c| plural_category_to_str(*c))
        .collect()
}

impl Interpreter {
    pub(crate) fn setup_intl_plural_rules(&mut self, intl_obj: &Rc<RefCell<JsObjectData>>) {
        let proto = self.create_object();
        if let Some(ref op) = self.realm().object_prototype {
            proto.borrow_mut().prototype = Some(op.clone());
        }
        proto.borrow_mut().class_name = "Intl.PluralRules".to_string();

        // @@toStringTag
        proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("Intl.PluralRules"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            },
        );

        // select(value)
        let select_fn = self.create_function(JsFunction::native(
            "select".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this {
                    if let Some(obj) = interp.get_object(o.id) {
                        let data = {
                            let b = obj.borrow();
                            b.intl_data.clone()
                        };
                        if let Some(IntlData::PluralRules {
                            ref locale,
                            ref plural_type,
                            ref notation,
                            ..
                        }) = data
                        {
                            let n_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                            let n = match interp.to_number_value(&n_val) {
                                Ok(v) => v,
                                Err(e) => return Completion::Throw(e),
                            };

                            if n.is_nan() {
                                return Completion::Normal(JsValue::String(JsString::from_str(
                                    "other",
                                )));
                            }

                            let base = base_locale(locale);
                            let icu_locale: IcuLocale =
                                base.parse().unwrap_or_else(|_| "en".parse().unwrap());
                            let prefs = PluralRulesPreferences::from(&icu_locale);
                            let mut opts = IcuPluralRulesOptions::default();
                            opts.rule_type = Some(if plural_type == "ordinal" {
                                PluralRuleType::Ordinal
                            } else {
                                PluralRuleType::Cardinal
                            });

                            let rules = match IcuPluralRules::try_new(prefs, opts) {
                                Ok(r) => r,
                                Err(_) => {
                                    return Completion::Normal(JsValue::String(
                                        JsString::from_str("other"),
                                    ));
                                }
                            };

                            let operands = number_to_plural_operands_with_notation(n, notation);
                            let cat = rules.category_for(operands);
                            return Completion::Normal(JsValue::String(JsString::from_str(
                                plural_category_to_str(cat),
                            )));
                        }
                    }
                }
                Completion::Throw(interp.create_type_error(
                    "Intl.PluralRules.prototype.select called on non-PluralRules object",
                ))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("select".to_string(), select_fn);

        // selectRange(start, end)
        let select_range_fn = self.create_function(JsFunction::native(
            "selectRange".to_string(),
            2,
            |interp, this, args| {
                if let JsValue::Object(o) = this {
                    if let Some(obj) = interp.get_object(o.id) {
                        let data = {
                            let b = obj.borrow();
                            b.intl_data.clone()
                        };
                        if let Some(IntlData::PluralRules {
                            ref locale,
                            ref plural_type,
                            ..
                        }) = data
                        {
                            let start_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                            let end_val = args.get(1).cloned().unwrap_or(JsValue::Undefined);

                            if matches!(start_val, JsValue::Undefined) {
                                return Completion::Throw(
                                    interp.create_type_error("start is undefined"),
                                );
                            }
                            if matches!(end_val, JsValue::Undefined) {
                                return Completion::Throw(
                                    interp.create_type_error("end is undefined"),
                                );
                            }

                            let start_n = match interp.to_number_value(&start_val) {
                                Ok(v) => v,
                                Err(e) => return Completion::Throw(e),
                            };
                            let end_n = match interp.to_number_value(&end_val) {
                                Ok(v) => v,
                                Err(e) => return Completion::Throw(e),
                            };

                            if start_n.is_nan() || end_n.is_nan() {
                                return Completion::Throw(
                                    interp.create_range_error("Invalid number NaN for selectRange"),
                                );
                            }

                            let base = base_locale(locale);
                            let icu_locale: IcuLocale =
                                base.parse().unwrap_or_else(|_| "en".parse().unwrap());
                            let prefs = PluralRulesPreferences::from(&icu_locale);
                            let mut opts = IcuPluralRulesOptions::default();
                            opts.rule_type = Some(if plural_type == "ordinal" {
                                PluralRuleType::Ordinal
                            } else {
                                PluralRuleType::Cardinal
                            });

                            let range_rules = match PluralRulesWithRanges::try_new(prefs, opts) {
                                Ok(r) => r,
                                Err(_) => {
                                    return Completion::Normal(JsValue::String(
                                        JsString::from_str("other"),
                                    ));
                                }
                            };

                            let start_ops = number_to_plural_operands(start_n);
                            let end_ops = number_to_plural_operands(end_n);
                            let cat = range_rules.category_for_range(start_ops, end_ops);
                            return Completion::Normal(JsValue::String(JsString::from_str(
                                plural_category_to_str(cat),
                            )));
                        }
                    }
                }
                Completion::Throw(interp.create_type_error(
                    "Intl.PluralRules.prototype.selectRange called on non-PluralRules object",
                ))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("selectRange".to_string(), select_range_fn);

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
                        if let Some(IntlData::PluralRules {
                            locale,
                            plural_type,
                            notation,
                            minimum_integer_digits,
                            minimum_fraction_digits,
                            maximum_fraction_digits,
                            minimum_significant_digits,
                            maximum_significant_digits,
                            rounding_mode,
                            rounding_increment,
                            rounding_priority,
                            trailing_zero_display,
                        }) = data
                        {
                            let result = interp.create_object();
                            if let Some(ref op) = interp.realm().object_prototype {
                                result.borrow_mut().prototype = Some(op.clone());
                            }

                            // Spec order: locale, type, notation, minimumIntegerDigits,
                            // then fraction/significant digits depending on what's set,
                            // pluralCategories, roundingIncrement, roundingMode,
                            // roundingPriority, trailingZeroDisplay

                            result.borrow_mut().insert_property(
                                "locale".to_string(),
                                PropertyDescriptor::data(
                                    JsValue::String(JsString::from_str(&locale)),
                                    true,
                                    true,
                                    true,
                                ),
                            );
                            result.borrow_mut().insert_property(
                                "type".to_string(),
                                PropertyDescriptor::data(
                                    JsValue::String(JsString::from_str(&plural_type)),
                                    true,
                                    true,
                                    true,
                                ),
                            );
                            result.borrow_mut().insert_property(
                                "notation".to_string(),
                                PropertyDescriptor::data(
                                    JsValue::String(JsString::from_str(&notation)),
                                    true,
                                    true,
                                    true,
                                ),
                            );
                            result.borrow_mut().insert_property(
                                "minimumIntegerDigits".to_string(),
                                PropertyDescriptor::data(
                                    JsValue::Number(minimum_integer_digits as f64),
                                    true,
                                    true,
                                    true,
                                ),
                            );

                            if minimum_significant_digits.is_none() {
                                // Only show fraction digits when no significant digits
                                result.borrow_mut().insert_property(
                                    "minimumFractionDigits".to_string(),
                                    PropertyDescriptor::data(
                                        JsValue::Number(minimum_fraction_digits as f64),
                                        true,
                                        true,
                                        true,
                                    ),
                                );
                                result.borrow_mut().insert_property(
                                    "maximumFractionDigits".to_string(),
                                    PropertyDescriptor::data(
                                        JsValue::Number(maximum_fraction_digits as f64),
                                        true,
                                        true,
                                        true,
                                    ),
                                );
                            } else {
                                // Both specified (morePrecision/lessPrecision) - show both
                                if rounding_priority != "auto" {
                                    result.borrow_mut().insert_property(
                                        "minimumFractionDigits".to_string(),
                                        PropertyDescriptor::data(
                                            JsValue::Number(minimum_fraction_digits as f64),
                                            true,
                                            true,
                                            true,
                                        ),
                                    );
                                    result.borrow_mut().insert_property(
                                        "maximumFractionDigits".to_string(),
                                        PropertyDescriptor::data(
                                            JsValue::Number(maximum_fraction_digits as f64),
                                            true,
                                            true,
                                            true,
                                        ),
                                    );
                                }
                            }

                            if let Some(min_sd) = minimum_significant_digits {
                                result.borrow_mut().insert_property(
                                    "minimumSignificantDigits".to_string(),
                                    PropertyDescriptor::data(
                                        JsValue::Number(min_sd as f64),
                                        true,
                                        true,
                                        true,
                                    ),
                                );
                            }
                            if let Some(max_sd) = maximum_significant_digits {
                                result.borrow_mut().insert_property(
                                    "maximumSignificantDigits".to_string(),
                                    PropertyDescriptor::data(
                                        JsValue::Number(max_sd as f64),
                                        true,
                                        true,
                                        true,
                                    ),
                                );
                            }

                            // pluralCategories
                            let cats = get_plural_categories_sorted(&locale, &plural_type);
                            let cat_values: Vec<JsValue> = cats
                                .iter()
                                .map(|c| JsValue::String(JsString::from_str(c)))
                                .collect();
                            let cat_array = interp.create_array(cat_values);
                            result.borrow_mut().insert_property(
                                "pluralCategories".to_string(),
                                PropertyDescriptor::data(cat_array, true, true, true),
                            );

                            result.borrow_mut().insert_property(
                                "roundingIncrement".to_string(),
                                PropertyDescriptor::data(
                                    JsValue::Number(rounding_increment as f64),
                                    true,
                                    true,
                                    true,
                                ),
                            );
                            result.borrow_mut().insert_property(
                                "roundingMode".to_string(),
                                PropertyDescriptor::data(
                                    JsValue::String(JsString::from_str(&rounding_mode)),
                                    true,
                                    true,
                                    true,
                                ),
                            );
                            result.borrow_mut().insert_property(
                                "roundingPriority".to_string(),
                                PropertyDescriptor::data(
                                    JsValue::String(JsString::from_str(&rounding_priority)),
                                    true,
                                    true,
                                    true,
                                ),
                            );
                            result.borrow_mut().insert_property(
                                "trailingZeroDisplay".to_string(),
                                PropertyDescriptor::data(
                                    JsValue::String(JsString::from_str(&trailing_zero_display)),
                                    true,
                                    true,
                                    true,
                                ),
                            );

                            let result_id = result.borrow().id.unwrap();
                            return Completion::Normal(JsValue::Object(crate::types::JsObject {
                                id: result_id,
                            }));
                        }
                    }
                }
                Completion::Throw(interp.create_type_error(
                    "Intl.PluralRules.prototype.resolvedOptions called on non-PluralRules object",
                ))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("resolvedOptions".to_string(), resolved_fn);

        self.realm_mut().intl_plural_rules_prototype = Some(proto.clone());

        // --- Constructor ---
        let proto_id = proto.borrow().id.unwrap();
        let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });
        let proto_clone = proto.clone();

        let ctor = self.create_function(JsFunction::constructor(
            "PluralRules".to_string(),
            0,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(interp.create_type_error(
                        "Intl.PluralRules must be called with 'new'",
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

                let _locale_matcher = match interp.intl_get_option(
                    &options,
                    "localeMatcher",
                    &["lookup", "best fit"],
                    Some("best fit"),
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let plural_type = match interp.intl_get_option(
                    &options,
                    "type",
                    &["cardinal", "ordinal"],
                    Some("cardinal"),
                ) {
                    Ok(Some(v)) => v,
                    Ok(None) => "cardinal".to_string(),
                    Err(e) => return Completion::Throw(e),
                };

                // notation - must be read between type and digit options per spec
                let notation = match interp.intl_get_option(
                    &options,
                    "notation",
                    &["standard", "compact", "scientific", "engineering"],
                    Some("standard"),
                ) {
                    Ok(Some(v)) => v,
                    Ok(None) => "standard".to_string(),
                    Err(e) => return Completion::Throw(e),
                };

                let minimum_integer_digits = match interp.intl_get_number_option(
                    &options,
                    "minimumIntegerDigits",
                    1.0,
                    21.0,
                    Some(1.0),
                ) {
                    Ok(Some(v)) => v as u32,
                    Ok(None) => 1,
                    Err(e) => return Completion::Throw(e),
                };

                // Read raw digit options to detect presence
                let raw_min_fd = if let JsValue::Object(o) = &options {
                    match interp.get_object_property(o.id, "minimumFractionDigits", &options) {
                        Completion::Normal(v) => {
                            if matches!(v, JsValue::Undefined) {
                                None
                            } else {
                                Some(v)
                            }
                        }
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => None,
                    }
                } else {
                    None
                };

                let raw_max_fd = if let JsValue::Object(o) = &options {
                    match interp.get_object_property(o.id, "maximumFractionDigits", &options) {
                        Completion::Normal(v) => {
                            if matches!(v, JsValue::Undefined) {
                                None
                            } else {
                                Some(v)
                            }
                        }
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => None,
                    }
                } else {
                    None
                };

                let raw_min_sd = if let JsValue::Object(o) = &options {
                    match interp
                        .get_object_property(o.id, "minimumSignificantDigits", &options)
                    {
                        Completion::Normal(v) => {
                            if matches!(v, JsValue::Undefined) {
                                None
                            } else {
                                Some(v)
                            }
                        }
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => None,
                    }
                } else {
                    None
                };

                let raw_max_sd = if let JsValue::Object(o) = &options {
                    match interp
                        .get_object_property(o.id, "maximumSignificantDigits", &options)
                    {
                        Completion::Normal(v) => {
                            if matches!(v, JsValue::Undefined) {
                                None
                            } else {
                                Some(v)
                            }
                        }
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => None,
                    }
                } else {
                    None
                };

                let has_sd = raw_min_sd.is_some() || raw_max_sd.is_some();
                let has_fd = raw_min_fd.is_some() || raw_max_fd.is_some();

                // roundingIncrement
                let raw_rounding_increment = if let JsValue::Object(o) = &options {
                    match interp.get_object_property(o.id, "roundingIncrement", &options) {
                        Completion::Normal(v) => {
                            if matches!(v, JsValue::Undefined) {
                                None
                            } else {
                                Some(v)
                            }
                        }
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => None,
                    }
                } else {
                    None
                };

                // roundingMode
                let rounding_mode = match interp.intl_get_option(
                    &options,
                    "roundingMode",
                    &[
                        "ceil",
                        "floor",
                        "expand",
                        "trunc",
                        "halfCeil",
                        "halfFloor",
                        "halfExpand",
                        "halfTrunc",
                        "halfEven",
                    ],
                    Some("halfExpand"),
                ) {
                    Ok(Some(v)) => v,
                    Ok(None) => "halfExpand".to_string(),
                    Err(e) => return Completion::Throw(e),
                };

                // roundingPriority
                let rounding_priority = match interp.intl_get_option(
                    &options,
                    "roundingPriority",
                    &["auto", "morePrecision", "lessPrecision"],
                    Some("auto"),
                ) {
                    Ok(Some(v)) => v,
                    Ok(None) => "auto".to_string(),
                    Err(e) => return Completion::Throw(e),
                };

                let default_min_fd: u32 = 0;
                let default_max_fd: u32 = 3;

                let (minimum_fraction_digits, maximum_fraction_digits, minimum_significant_digits, maximum_significant_digits) =
                    if has_sd && !has_fd && rounding_priority == "auto" {
                        let min_sd = if let Some(ref v) = raw_min_sd {
                            match interp.to_number_value(v) {
                                Ok(n) => {
                                    if n.is_nan() || n < 1.0 || n > 21.0 {
                                        return Completion::Throw(interp.create_range_error(
                                            "minimumSignificantDigits is out of range",
                                        ));
                                    }
                                    n.floor() as u32
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        } else {
                            1
                        };
                        let max_sd = if let Some(ref v) = raw_max_sd {
                            match interp.to_number_value(v) {
                                Ok(n) => {
                                    if n.is_nan() || n < 1.0 || n > 21.0 {
                                        return Completion::Throw(interp.create_range_error(
                                            "maximumSignificantDigits is out of range",
                                        ));
                                    }
                                    n.floor() as u32
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        } else {
                            21
                        };
                        if min_sd > max_sd {
                            return Completion::Throw(interp.create_range_error(
                                "minimumSignificantDigits is greater than maximumSignificantDigits",
                            ));
                        }
                        (default_min_fd, default_max_fd, Some(min_sd), Some(max_sd))
                    } else if has_fd && !has_sd && rounding_priority == "auto" {
                        let explicit_min_fd = if let Some(ref v) = raw_min_fd {
                            match interp.to_number_value(v) {
                                Ok(n) => {
                                    if n.is_nan() || n < 0.0 || n > 100.0 {
                                        return Completion::Throw(interp.create_range_error(
                                            "minimumFractionDigits is out of range",
                                        ));
                                    }
                                    Some(n.floor() as u32)
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        } else {
                            None
                        };
                        let explicit_max_fd = if let Some(ref v) = raw_max_fd {
                            match interp.to_number_value(v) {
                                Ok(n) => {
                                    if n.is_nan() || n < 0.0 || n > 100.0 {
                                        return Completion::Throw(interp.create_range_error(
                                            "maximumFractionDigits is out of range",
                                        ));
                                    }
                                    Some(n.floor() as u32)
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        } else {
                            None
                        };
                        let (min_fd, max_fd) = match (explicit_min_fd, explicit_max_fd) {
                            (Some(mn), Some(mx)) => (mn, mx),
                            (Some(mn), None) => (mn, default_max_fd.max(mn)),
                            (None, Some(mx)) => (default_min_fd.min(mx), mx),
                            (None, None) => (default_min_fd, default_max_fd),
                        };
                        if min_fd > max_fd {
                            return Completion::Throw(interp.create_range_error(
                                "minimumFractionDigits is greater than maximumFractionDigits",
                            ));
                        }
                        (min_fd, max_fd, None, None)
                    } else if has_sd && has_fd {
                        let min_sd = if let Some(ref v) = raw_min_sd {
                            match interp.to_number_value(v) {
                                Ok(n) => {
                                    if n.is_nan() || n < 1.0 || n > 21.0 {
                                        return Completion::Throw(interp.create_range_error(
                                            "minimumSignificantDigits is out of range",
                                        ));
                                    }
                                    n.floor() as u32
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        } else {
                            1
                        };
                        let max_sd = if let Some(ref v) = raw_max_sd {
                            match interp.to_number_value(v) {
                                Ok(n) => {
                                    if n.is_nan() || n < 1.0 || n > 21.0 {
                                        return Completion::Throw(interp.create_range_error(
                                            "maximumSignificantDigits is out of range",
                                        ));
                                    }
                                    n.floor() as u32
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        } else {
                            21
                        };
                        let min_fd = if let Some(ref v) = raw_min_fd {
                            match interp.to_number_value(v) {
                                Ok(n) => {
                                    if n.is_nan() || n < 0.0 || n > 100.0 {
                                        return Completion::Throw(interp.create_range_error(
                                            "minimumFractionDigits is out of range",
                                        ));
                                    }
                                    n.floor() as u32
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        } else {
                            default_min_fd
                        };
                        let max_fd = if let Some(ref v) = raw_max_fd {
                            match interp.to_number_value(v) {
                                Ok(n) => {
                                    if n.is_nan() || n < 0.0 || n > 100.0 {
                                        return Completion::Throw(interp.create_range_error(
                                            "maximumFractionDigits is out of range",
                                        ));
                                    }
                                    n.floor() as u32
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        } else {
                            default_max_fd.max(min_fd)
                        };
                        if min_sd > max_sd {
                            return Completion::Throw(interp.create_range_error(
                                "minimumSignificantDigits is greater than maximumSignificantDigits",
                            ));
                        }
                        if min_fd > max_fd {
                            return Completion::Throw(interp.create_range_error(
                                "minimumFractionDigits is greater than maximumFractionDigits",
                            ));
                        }
                        (min_fd, max_fd, Some(min_sd), Some(max_sd))
                    } else {
                        (default_min_fd, default_max_fd, None, None)
                    };

                // Validate roundingIncrement
                let rounding_increment = if let Some(ref ri_val) = raw_rounding_increment {
                    let num = match interp.to_number_value(ri_val) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    if num.is_nan() || num < 1.0 || num > 5000.0 {
                        return Completion::Throw(
                            interp.create_range_error("roundingIncrement value is out of range"),
                        );
                    }
                    let vi = num.floor() as u32;
                    let valid = [
                        1, 2, 5, 10, 20, 25, 50, 100, 200, 250, 500, 1000, 2000, 2500, 5000,
                    ];
                    if !valid.contains(&vi) {
                        return Completion::Throw(interp.create_range_error(&format!(
                            "Invalid roundingIncrement value: {}",
                            vi
                        )));
                    }
                    vi
                } else {
                    1u32
                };

                if rounding_increment != 1 {
                    if minimum_significant_digits.is_some()
                        || maximum_significant_digits.is_some()
                    {
                        return Completion::Throw(interp.create_type_error(
                            "roundingIncrement is not compatible with significantDigits rounding",
                        ));
                    }
                    if rounding_priority != "auto" {
                        return Completion::Throw(interp.create_type_error(
                            "roundingIncrement is not compatible with non-auto roundingPriority",
                        ));
                    }
                    if minimum_fraction_digits != maximum_fraction_digits {
                        return Completion::Throw(interp.create_range_error(
                            "If roundingIncrement is not 1, maximumFractionDigits must equal minimumFractionDigits",
                        ));
                    }
                }

                // trailingZeroDisplay
                let trailing_zero_display = match interp.intl_get_option(
                    &options,
                    "trailingZeroDisplay",
                    &["auto", "stripIfInteger"],
                    Some("auto"),
                ) {
                    Ok(Some(v)) => v,
                    Ok(None) => "auto".to_string(),
                    Err(e) => return Completion::Throw(e),
                };

                let raw_locale = interp.intl_resolve_locale(&requested);
                let locale = base_locale(&raw_locale);

                let obj = interp.create_object();
                obj.borrow_mut().prototype = Some(proto_clone.clone());
                obj.borrow_mut().class_name = "Intl.PluralRules".to_string();
                obj.borrow_mut().intl_data = Some(IntlData::PluralRules {
                    locale,
                    plural_type,
                    notation,
                    minimum_integer_digits,
                    minimum_fraction_digits,
                    maximum_fraction_digits,
                    minimum_significant_digits,
                    maximum_significant_digits,
                    rounding_mode,
                    rounding_increment,
                    rounding_priority,
                    trailing_zero_display,
                });

                let obj_id = obj.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id: obj_id }))
            },
        ));

        // Set PluralRules.prototype on constructor
        if let JsValue::Object(ctor_ref) = &ctor {
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
            PropertyDescriptor::data(ctor.clone(), true, false, true),
        );

        // Register Intl.PluralRules on the Intl namespace
        intl_obj.borrow_mut().insert_property(
            "PluralRules".to_string(),
            PropertyDescriptor::data(ctor, true, false, true),
        );
    }
}
