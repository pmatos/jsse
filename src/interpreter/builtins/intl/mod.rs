mod collator;
mod datetimeformat;
mod displaynames;
mod durationformat;
mod listformat;
mod locale;
mod numberformat;
mod pluralrules;
mod relativetimeformat;
mod segmenter;

use super::super::*;
use icu::locale::Locale as IcuLocale;
use icu::locale::LocaleCanonicalizer;

impl Interpreter {
    pub(crate) fn setup_intl(&mut self) {
        let intl_obj = self.create_object();
        let intl_id = intl_obj.borrow().id.unwrap();

        // @@toStringTag = "Intl" (per spec 8.1.1)
        {
            let key = "Symbol(Symbol.toStringTag)".to_string();
            let desc = PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("Intl"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            };
            intl_obj.borrow_mut().property_order.push(key.clone());
            intl_obj.borrow_mut().properties.insert(key, desc);
        }

        // Intl.getCanonicalLocales(locales)
        let gcl_fn = self.create_function(JsFunction::native(
            "getCanonicalLocales".to_string(),
            1,
            |interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]| {
                let locales = args.first().unwrap_or(&JsValue::Undefined);
                match interp.intl_canonicalize_locale_list(locales) {
                    Ok(list) => {
                        let values: Vec<JsValue> = list
                            .into_iter()
                            .map(|s| JsValue::String(JsString::from_str(&s)))
                            .collect();
                        Completion::Normal(interp.create_array(values))
                    }
                    Err(e) => Completion::Throw(e),
                }
            },
        ));
        intl_obj
            .borrow_mut()
            .insert_builtin("getCanonicalLocales".to_string(), gcl_fn);

        // Intl.supportedValuesOf(key)
        let svo_fn = self.create_function(JsFunction::native(
            "supportedValuesOf".to_string(),
            1,
            |interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]| {
                let key = match args.first() {
                    Some(v) => match interp.to_string_value(v) {
                        Ok(s) => s,
                        Err(e) => return Completion::Throw(e),
                    },
                    None => {
                        let err =
                            interp.create_range_error("supportedValuesOf requires a key argument");
                        return Completion::Throw(err);
                    }
                };

                let values: Vec<&str> = match key.as_str() {
                    "calendar" => vec![
                        "buddhist",
                        "chinese",
                        "coptic",
                        "dangi",
                        "ethioaa",
                        "ethiopic",
                        "gregory",
                        "hebrew",
                        "indian",
                        "islamic-civil",
                        "islamic-tbla",
                        "islamic-umalqura",
                        "iso8601",
                        "japanese",
                        "persian",
                        "roc",
                    ],
                    "collation" => vec![
                        "big5han", "compat", "dict", "emoji", "eor", "phonebk", "phonetic",
                        "pinyin", "searchjl", "stroke", "trad", "unihan", "zhuyin",
                    ],
                    "currency" => vec![
                        "AED", "AFN", "ALL", "AMD", "ANG", "AOA", "ARS", "AUD", "AWG", "AZN",
                        "BAM", "BBD", "BDT", "BGN", "BHD", "BIF", "BMD", "BND", "BOB", "BRL",
                        "BSD", "BTN", "BWP", "BYN", "BZD", "CAD", "CDF", "CHF", "CLP", "CNY",
                        "COP", "CRC", "CUC", "CUP", "CVE", "CZK", "DJF", "DKK", "DOP", "DZD",
                        "EGP", "ERN", "ETB", "EUR", "FJD", "FKP", "GBP", "GEL", "GHS", "GIP",
                        "GMD", "GNF", "GTQ", "GYD", "HKD", "HNL", "HRK", "HTG", "HUF", "IDR",
                        "ILS", "INR", "IQD", "IRR", "ISK", "JMD", "JOD", "JPY", "KES", "KGS",
                        "KHR", "KMF", "KPW", "KRW", "KWD", "KYD", "KZT", "LAK", "LBP", "LKR",
                        "LRD", "LSL", "LYD", "MAD", "MDL", "MGA", "MKD", "MMK", "MNT", "MOP",
                        "MRU", "MUR", "MVR", "MWK", "MXN", "MYR", "MZN", "NAD", "NGN", "NIO",
                        "NOK", "NPR", "NZD", "OMR", "PAB", "PEN", "PGK", "PHP", "PKR", "PLN",
                        "PYG", "QAR", "RON", "RSD", "RUB", "RWF", "SAR", "SBD", "SCR", "SDG",
                        "SEK", "SGD", "SHP", "SLE", "SLL", "SOS", "SRD", "SSP", "STN", "SVC",
                        "SYP", "SZL", "THB", "TJS", "TMT", "TND", "TOP", "TRY", "TTD", "TWD",
                        "TZS", "UAH", "UGX", "USD", "UYU", "UZS", "VED", "VES", "VND", "VUV",
                        "WST", "XAF", "XCD", "XOF", "XPF", "YER", "ZAR", "ZMW", "ZWL",
                    ],
                    "numberingSystem" => vec![
                        "adlm", "ahom", "arab", "arabext", "bali", "beng", "bhks", "brah",
                        "cakm", "cham", "deva", "diak", "fullwide", "gong", "gonm", "gujr",
                        "guru", "hanidec", "hmng", "hmnp", "java", "kali", "kawi", "khmr",
                        "knda", "lana", "lanatham", "laoo", "latn", "lepc", "limb", "mathbold",
                        "mathdbl", "mathmono", "mathsanb", "mathsans", "mlym", "modi", "mong",
                        "mroo", "mtei", "mymr", "mymrshan", "mymrtlng", "nagm", "newa", "nkoo",
                        "olck", "orya", "osma", "rohg", "saur", "segment", "shrd", "sind",
                        "sinh", "sora", "sund", "takr", "talu", "tamldec", "telu", "thai",
                        "tibt", "tirh", "tnsa", "vaii", "wara", "wcho",
                    ],
                    "timeZone" => vec![
                        "Africa/Abidjan",
                        "Africa/Accra",
                        "Africa/Addis_Ababa",
                        "Africa/Algiers",
                        "Africa/Cairo",
                        "Africa/Casablanca",
                        "Africa/Johannesburg",
                        "Africa/Lagos",
                        "Africa/Nairobi",
                        "America/Anchorage",
                        "America/Argentina/Buenos_Aires",
                        "America/Bogota",
                        "America/Chicago",
                        "America/Denver",
                        "America/Los_Angeles",
                        "America/Mexico_City",
                        "America/New_York",
                        "America/Sao_Paulo",
                        "America/Toronto",
                        "America/Vancouver",
                        "Asia/Baghdad",
                        "Asia/Bangkok",
                        "Asia/Colombo",
                        "Asia/Dhaka",
                        "Asia/Dubai",
                        "Asia/Hong_Kong",
                        "Asia/Jakarta",
                        "Asia/Karachi",
                        "Asia/Kolkata",
                        "Asia/Kuala_Lumpur",
                        "Asia/Manila",
                        "Asia/Seoul",
                        "Asia/Shanghai",
                        "Asia/Singapore",
                        "Asia/Taipei",
                        "Asia/Tehran",
                        "Asia/Tokyo",
                        "Atlantic/Reykjavik",
                        "Australia/Melbourne",
                        "Australia/Sydney",
                        "Etc/GMT+1",
                        "Etc/GMT+10",
                        "Etc/GMT+11",
                        "Etc/GMT+12",
                        "Etc/GMT+2",
                        "Etc/GMT+3",
                        "Etc/GMT+4",
                        "Etc/GMT+5",
                        "Etc/GMT+6",
                        "Etc/GMT+7",
                        "Etc/GMT+8",
                        "Etc/GMT+9",
                        "Etc/GMT-1",
                        "Etc/GMT-10",
                        "Etc/GMT-11",
                        "Etc/GMT-12",
                        "Etc/GMT-13",
                        "Etc/GMT-14",
                        "Etc/GMT-2",
                        "Etc/GMT-3",
                        "Etc/GMT-4",
                        "Etc/GMT-5",
                        "Etc/GMT-6",
                        "Etc/GMT-7",
                        "Etc/GMT-8",
                        "Etc/GMT-9",
                        "Europe/Amsterdam",
                        "Europe/Athens",
                        "Europe/Berlin",
                        "Europe/Brussels",
                        "Europe/Budapest",
                        "Europe/Dublin",
                        "Europe/Helsinki",
                        "Europe/Istanbul",
                        "Europe/Lisbon",
                        "Europe/London",
                        "Europe/Madrid",
                        "Europe/Moscow",
                        "Europe/Oslo",
                        "Europe/Paris",
                        "Europe/Prague",
                        "Europe/Rome",
                        "Europe/Stockholm",
                        "Europe/Vienna",
                        "Europe/Warsaw",
                        "Europe/Zurich",
                        "Pacific/Auckland",
                        "Pacific/Honolulu",
                        "UTC",
                    ],
                    "unit" => vec![
                        "acre",
                        "bit",
                        "byte",
                        "celsius",
                        "centimeter",
                        "day",
                        "degree",
                        "fahrenheit",
                        "fluid-ounce",
                        "foot",
                        "gallon",
                        "gigabit",
                        "gigabyte",
                        "gram",
                        "hectare",
                        "hour",
                        "inch",
                        "kilobit",
                        "kilobyte",
                        "kilogram",
                        "kilometer",
                        "liter",
                        "megabit",
                        "megabyte",
                        "meter",
                        "microsecond",
                        "mile",
                        "mile-scandinavian",
                        "milliliter",
                        "millimeter",
                        "millisecond",
                        "minute",
                        "month",
                        "nanosecond",
                        "ounce",
                        "percent",
                        "petabyte",
                        "pound",
                        "second",
                        "stone",
                        "terabit",
                        "terabyte",
                        "week",
                        "yard",
                        "year",
                    ],
                    _ => {
                        let err = interp.create_range_error(&format!(
                            "Invalid key \"{}\" for supportedValuesOf",
                            key
                        ));
                        return Completion::Throw(err);
                    }
                };

                let js_values: Vec<JsValue> = values
                    .into_iter()
                    .map(|s| JsValue::String(JsString::from_str(s)))
                    .collect();
                Completion::Normal(interp.create_array(js_values))
            },
        ));
        intl_obj
            .borrow_mut()
            .insert_builtin("supportedValuesOf".to_string(), svo_fn);

        // Intl.Locale
        self.setup_intl_locale(&intl_obj);

        // Intl.Collator
        self.setup_intl_collator(&intl_obj);

        // Intl.NumberFormat
        self.setup_intl_number_format(&intl_obj);

        // Intl.PluralRules
        self.setup_intl_plural_rules(&intl_obj);

        // Intl.ListFormat
        self.setup_intl_list_format(&intl_obj);

        // Intl.RelativeTimeFormat
        self.setup_intl_relative_time_format(&intl_obj);

        // Intl.Segmenter
        self.setup_intl_segmenter(&intl_obj);

        // Intl.DisplayNames
        self.setup_intl_display_names(&intl_obj);

        // Intl.DateTimeFormat
        self.setup_intl_date_time_format(&intl_obj);

        // Intl.DurationFormat
        self.setup_intl_duration_format(&intl_obj);

        let intl_val = JsValue::Object(crate::types::JsObject { id: intl_id });
        self.global_env
            .borrow_mut()
            .declare("Intl", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("Intl", intl_val);
    }

    pub(crate) fn is_structurally_valid_language_tag(tag: &str) -> bool {
        // Only handle specific tags that ICU4X rejects but are valid per BCP47.
        // A language subtag of 5-8 alpha chars is grammatically valid.
        let lower = tag.to_ascii_lowercase();
        let parts: Vec<&str> = lower.split('-').collect();
        if parts.is_empty() {
            return false;
        }
        let lang = parts[0];
        if !lang.chars().all(|c| c.is_ascii_alphabetic()) {
            return false;
        }
        if !((2..=3).contains(&lang.len()) || (5..=8).contains(&lang.len())) {
            return false;
        }
        // For 5-8 char language subtags (like "posix"), only allow if there are
        // no further subtags or all subtags look sane (x-private, simple variants)
        if (5..=8).contains(&lang.len()) {
            for part in &parts[1..] {
                if part.is_empty() || part.len() > 8 {
                    return false;
                }
                if !part.chars().all(|c| c.is_ascii_alphanumeric()) {
                    return false;
                }
            }
            return true;
        }
        // For 2-3 char language subtags that ICU4X rejects, we don't second-guess ICU4X
        false
    }

    fn post_canonicalize_locale(tag: &str) -> String {
        let mut result = tag.to_string();

        // Unicode extension type aliases not handled by ICU4X
        // Calendar aliases
        result = result.replace("-ca-islamicc", "-ca-islamic-civil");
        result = result.replace("-ca-ethiopic-amete-alem", "-ca-ethioaa");

        // For keys kb, kc, kh, kk, kn: "yes" is alias for "true", and "true" values are removed
        let boolean_keys = ["kb", "kc", "kh", "kk", "kn"];
        for key in &boolean_keys {
            let yes_pattern = format!("-{}-yes", key);
            let true_pattern = format!("-{}-true", key);
            let key_only = format!("-{}", key);
            if result.contains(&yes_pattern) {
                let idx = result.find(&yes_pattern).unwrap();
                let after = &result[idx + yes_pattern.len()..];
                if after.is_empty() || after.starts_with('-') {
                    result = result.replace(&yes_pattern, &key_only);
                }
            } else if result.contains(&true_pattern) {
                let idx = result.find(&true_pattern).unwrap();
                let after = &result[idx + true_pattern.len()..];
                if after.is_empty() || after.starts_with('-') {
                    result = result.replace(&true_pattern, &key_only);
                }
            }
        }

        // Collation strength (ks) aliases
        result = result.replace("-ks-primary", "-ks-level1");
        result = result.replace("-ks-tertiary", "-ks-level3");

        // Collation type aliases
        result = result.replace("-co-dictionary", "-co-dict");
        result = result.replace("-co-phonebook", "-co-phonebk");
        result = result.replace("-co-traditional", "-co-trad");

        // Measurement system aliases
        result = result.replace("-ms-imperial", "-ms-uksystem");

        // Timezone aliases (deprecated/alias -> canonical)
        // Must match whole subtag values to avoid false matches
        let tz_aliases: &[(&str, &str)] = &[
            ("cnckg", "cnsha"), ("eire", "iedub"), ("est", "papty"),
            ("gmt0", "gmt"), ("uct", "utc"), ("zulu", "utc"),
        ];
        for (alias, canonical) in tz_aliases {
            let from = format!("-tz-{}", alias);
            if result.contains(&from) {
                let start = result.find(&from).unwrap();
                let end = start + from.len();
                if end == result.len() || result[end..].starts_with('-') {
                    let to = format!("-tz-{}", canonical);
                    result = format!("{}{}{}", &result[..start], to, &result[end..]);
                }
            }
        }

        // Transformed extension aliases
        result = result.replace("-m0-names", "-m0-prprname");

        // Numbering system aliases
        result = result.replace("-nu-traditional", "-nu-traditio");

        result
    }

    // §9.2.1 CanonicalizeLocaleList(locales)
    pub(crate) fn intl_canonicalize_locale_list(
        &mut self,
        locales: &JsValue,
    ) -> Result<Vec<String>, JsValue> {
        if matches!(locales, JsValue::Undefined) {
            return Ok(Vec::new());
        }

        let mut seen = Vec::new();

        // Step 3: If Type(locales) is String or locales has [[InitializedLocale]]
        if let JsValue::String(s) = locales {
            let tag = s.to_rust_string();
            match tag.parse::<IcuLocale>() {
                Ok(mut locale) => {
                    let canonicalizer = LocaleCanonicalizer::new_extended();
                    canonicalizer.canonicalize(&mut locale);
                    seen.push(Self::post_canonicalize_locale(&locale.to_string()));
                }
                Err(_) => {
                    if Self::is_structurally_valid_language_tag(&tag) {
                        seen.push(tag.to_ascii_lowercase());
                    } else {
                        return Err(self.create_range_error(&format!(
                            "Invalid language tag: {}",
                            tag
                        )));
                    }
                }
            }
            return Ok(seen);
        }

        // Check if locales is an Intl.Locale object itself (treat as single-element list)
        if let JsValue::Object(o) = locales {
            if let Some(obj) = self.get_object(o.id) {
                let b = obj.borrow();
                if let Some(IntlData::Locale { ref tag, .. }) = b.intl_data {
                    let tag_clone = tag.clone();
                    drop(b);
                    seen.push(tag_clone);
                    return Ok(seen);
                }
            }
        }

        let obj = match self.to_object(locales) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => return Ok(Vec::new()),
        };

        let len_val = if let JsValue::Object(o) = &obj {
            match self.get_object_property(o.id, "length", &obj) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::Undefined,
            }
        } else {
            JsValue::Undefined
        };

        // Step 5: Let len be ? ToLength(? Get(O, "length")).
        let len_num = match self.to_number_value(&len_val) {
            Ok(n) => n,
            Err(e) => return Err(e),
        };
        let len = if len_num.is_nan() || len_num <= 0.0 {
            0u64
        } else if len_num.is_infinite() {
            if len_num > 0.0 {
                (1u64 << 53) - 1
            } else {
                0
            }
        } else {
            let n = len_num.floor() as u64;
            n.min((1u64 << 53) - 1)
        };

        for i in 0..len {
            let key = i.to_string();

            // Step 7b: Let kPresent be ? HasProperty(O, Pk).
            let k_present = if let JsValue::Object(o) = &obj {
                self.proxy_has_property(o.id, &key)?
            } else {
                false
            };

            // Step 7c: If kPresent is true, then
            if !k_present {
                continue;
            }

            // Step 7c.i: Let kValue be ? Get(O, Pk).
            let k_value = if let JsValue::Object(o) = &obj {
                match self.get_object_property(o.id, &key, &obj) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::Undefined,
                }
            } else {
                JsValue::Undefined
            };

            // Step 7c.ii: If Type(kValue) is not String or Object, throw TypeError.
            match &k_value {
                JsValue::String(_) | JsValue::Object(_) => {}
                _ => {
                    return Err(
                        self.create_type_error("Language tag must be a string or object"),
                    );
                }
            }

            // Step 7c.iii-iv: If kValue has [[InitializedLocale]], use [[Locale]] directly
            let tag = if let JsValue::Object(o) = &k_value {
                if let Some(obj_data) = self.get_object(o.id) {
                    let b = obj_data.borrow();
                    if let Some(IntlData::Locale { ref tag, .. }) = b.intl_data {
                        tag.clone()
                    } else {
                        drop(b);
                        self.to_string_value(&k_value)?
                    }
                } else {
                    self.to_string_value(&k_value)?
                }
            } else if let JsValue::String(s) = &k_value {
                s.to_rust_string()
            } else {
                unreachable!()
            };

            let canonical = match tag.parse::<IcuLocale>() {
                Ok(mut locale) => {
                    let canonicalizer = LocaleCanonicalizer::new_extended();
                    canonicalizer.canonicalize(&mut locale);
                    Self::post_canonicalize_locale(&locale.to_string())
                }
                Err(_) => {
                    if Self::is_structurally_valid_language_tag(&tag) {
                        tag.to_ascii_lowercase()
                    } else {
                        return Err(self.create_range_error(&format!(
                            "Invalid language tag: {}",
                            tag
                        )));
                    }
                }
            };

            if !seen.contains(&canonical) {
                seen.push(canonical);
            }
        }

        Ok(seen)
    }

    // §9.2.2 CoerceOptionsToObject
    pub(crate) fn intl_coerce_options_to_object(
        &mut self,
        options: &JsValue,
    ) -> Result<JsValue, JsValue> {
        if matches!(options, JsValue::Undefined) {
            let obj = self.create_object();
            obj.borrow_mut().prototype = None; // ObjectCreate(null)
            let id = obj.borrow().id.unwrap();
            return Ok(JsValue::Object(crate::types::JsObject { id }));
        }
        match self.to_object(options) {
            Completion::Normal(v) => Ok(v),
            Completion::Throw(e) => Err(e),
            _ => {
                let obj = self.create_object();
                let id = obj.borrow().id.unwrap();
                Ok(JsValue::Object(crate::types::JsObject { id }))
            }
        }
    }

    // §9.2.12 GetOptionsObject
    pub(crate) fn intl_get_options_object(
        &mut self,
        options: &JsValue,
    ) -> Result<JsValue, JsValue> {
        if matches!(options, JsValue::Undefined) {
            let obj = self.create_object();
            obj.borrow_mut().prototype = None;
            let id = obj.borrow().id.unwrap();
            return Ok(JsValue::Object(crate::types::JsObject { id }));
        }
        if matches!(options, JsValue::Object(_)) {
            return Ok(options.clone());
        }
        Err(self.create_type_error("Options argument must be an object or undefined"))
    }

    // §9.2.12 GetOption
    pub(crate) fn intl_get_option(
        &mut self,
        options: &JsValue,
        property: &str,
        valid_values: &[&str],
        fallback: Option<&str>,
    ) -> Result<Option<String>, JsValue> {
        let value = if let JsValue::Object(o) = options {
            match self.get_object_property(o.id, property, options) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::Undefined,
            }
        } else {
            JsValue::Undefined
        };

        if matches!(value, JsValue::Undefined) {
            return Ok(fallback.map(|s| s.to_string()));
        }

        let str_val = self.to_string_value(&value)?;

        if !valid_values.is_empty() && !valid_values.contains(&str_val.as_str()) {
            return Err(self.create_range_error(&format!(
                "Value {} is not allowed for option {}",
                str_val, property
            )));
        }

        Ok(Some(str_val))
    }

    // §9.2.13 GetNumberOption
    pub(crate) fn intl_get_number_option(
        &mut self,
        options: &JsValue,
        property: &str,
        minimum: f64,
        maximum: f64,
        fallback: Option<f64>,
    ) -> Result<Option<f64>, JsValue> {
        let value = if let JsValue::Object(o) = options {
            match self.get_object_property(o.id, property, options) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::Undefined,
            }
        } else {
            JsValue::Undefined
        };

        if matches!(value, JsValue::Undefined) {
            return Ok(fallback);
        }

        let num = self.to_number_value(&value)?;
        if num.is_nan() || num < minimum || num > maximum {
            return Err(self.create_range_error(&format!(
                "Value {} is outside of range [{}, {}] for option {}",
                num, minimum, maximum, property
            )));
        }

        Ok(Some(num.floor()))
    }

    // §9.2.6 BestAvailableLocale / §9.2.7 LookupSupportedLocales simplified
    pub(crate) fn intl_supported_locales(
        &mut self,
        requested: &[String],
        options: &JsValue,
    ) -> Result<JsValue, JsValue> {
        if !matches!(options, JsValue::Undefined) {
            // Step 1: If options is not undefined, let options be ToObject(options)
            if matches!(options, JsValue::Null) {
                return Err(self.create_type_error("Cannot convert null to object"));
            }
            let opts = match self.to_object(options) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::Undefined,
            };
            // Validate localeMatcher option
            let _matcher = self.intl_get_option(
                &opts,
                "localeMatcher",
                &["lookup", "best fit"],
                Some("best fit"),
            )?;
        }

        let supported: Vec<JsValue> = requested
            .iter()
            .filter_map(|tag| {
                let locale: IcuLocale = tag.parse().ok()?;
                let canonical = locale.to_string();
                if Self::intl_best_available_locale(&canonical) {
                    Some(JsValue::String(JsString::from_str(&canonical)))
                } else {
                    None
                }
            })
            .collect();
        Ok(self.create_array(supported))
    }

    fn intl_best_available_locale(locale_str: &str) -> bool {
        let mut candidate = locale_str.to_string();
        // Strip unicode extensions for matching
        if let Some(idx) = candidate.find("-u-") {
            candidate = candidate[..idx].to_string();
        }
        loop {
            // Try to create a collator with this locale to see if ICU4X has data for it
            if let Ok(loc) = candidate.parse::<IcuLocale>() {
                let lang = loc.id.language.to_string();
                // Reject locales with no real language subtag (e.g., "zxx", "und")
                if !lang.is_empty() && lang != "und" {
                    let known_languages = [
                        "af", "am", "ar", "as", "az", "be", "bg", "bn", "bo", "br",
                        "bs", "ca", "cs", "cy", "da", "de", "el", "en", "eo", "es",
                        "et", "eu", "fa", "fi", "fil", "fo", "fr", "ga", "gl", "gu",
                        "ha", "he", "hi", "hr", "hu", "hy", "id", "ig", "is", "it",
                        "ja", "ka", "kk", "km", "kn", "ko", "kok", "ku", "ky", "lb",
                        "ln", "lo", "lt", "lv", "mk", "ml", "mn", "mr", "ms", "mt", "my",
                        "nb", "ne", "nl", "nn", "no", "or", "pa", "pl", "ps", "pt",
                        "ro", "ru", "si", "sk", "sl", "sq", "sr", "sv", "sw", "ta",
                        "te", "th", "tk", "tr", "uk", "ur", "uz", "vi", "wo", "yo",
                        "zh", "zu",
                    ];
                    if known_languages.contains(&lang.as_str()) {
                        return true;
                    }
                }
            }
            // Remove the last subtag and try again
            if let Some(idx) = candidate.rfind('-') {
                candidate = candidate[..idx].to_string();
            } else {
                break;
            }
        }
        false
    }

    // §9.2.8 ResolveLocale simplified
    pub(crate) fn intl_resolve_locale(&mut self, requested: &[String]) -> String {
        for tag in requested {
            if tag.parse::<IcuLocale>().is_ok() && Self::intl_best_available_locale(tag) {
                return tag.clone();
            }
        }
        "en".to_string()
    }

    pub(crate) fn intl_construct_number_format(
        &mut self,
        locales: &JsValue,
        options: &JsValue,
    ) -> Result<JsValue, JsValue> {
        let nf_ctor = if let Some(ref ctor) = self.intl_number_format_ctor {
            ctor.clone()
        } else {
            return Err(self.create_type_error("Intl.NumberFormat is not available"));
        };
        let old_new_target = self.new_target.take();
        self.new_target = Some(nf_ctor.clone());
        let result = self.call_function(
            &nf_ctor,
            &JsValue::Undefined,
            &[locales.clone(), options.clone()],
        );
        self.new_target = old_new_target;
        match result {
            Completion::Normal(v) => Ok(v),
            Completion::Throw(e) => Err(e),
            _ => Err(self.create_type_error("NumberFormat construction failed")),
        }
    }

    pub(crate) fn intl_number_format_format(
        &mut self,
        nf: &JsValue,
        value: &JsValue,
    ) -> Completion {
        if let JsValue::Object(nf_obj) = nf {
            let format_fn = match self.get_object_property(nf_obj.id, "format", nf) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Completion::Throw(e),
                _ => return Completion::Throw(self.create_type_error("format not found")),
            };
            self.call_function(&format_fn, &JsValue::Undefined, &[value.clone()])
        } else {
            Completion::Throw(self.create_type_error("NumberFormat is not an object"))
        }
    }
}
