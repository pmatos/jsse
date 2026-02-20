mod locale;

use super::super::*;
use icu::locale::Locale as IcuLocale;

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
                        "islamic",
                        "islamic-civil",
                        "islamic-rgsa",
                        "islamic-tbla",
                        "islamic-umalqura",
                        "iso8601",
                        "japanese",
                        "persian",
                        "roc",
                    ],
                    "collation" => vec![
                        "big5han", "compat", "dict", "emoji", "eor", "phonebk", "phonetic",
                        "pinyin", "search", "searchjl", "standard", "stroke", "trad", "unihan",
                        "zhuyin",
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
                        "adlm", "ahom", "arab", "arabext", "bali", "beng", "bhks", "cakm",
                        "cham", "deva", "diak", "fullwide", "gong", "gonm", "gujr", "guru",
                        "hanidec", "hmng", "hmnp", "java", "kali", "khmr", "knda", "lana",
                        "lanatham", "laoo", "latn", "lepc", "limb", "mathbold", "mathdbl",
                        "mathmono", "mathsanb", "mathsans", "mlym", "modi", "mong", "mroo",
                        "mtei", "mymr", "mymrshan", "mymrtlng", "newa", "nkoo", "olck", "orya",
                        "osma", "rohg", "saur", "segment", "shrd", "sind", "sinh", "sora",
                        "sund", "takr", "talu", "tamldec", "telu", "thai", "tibt", "tirh",
                        "tnsa", "vaii", "wara", "wcho",
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
                        "Asia/Calcutta",
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

        let intl_val = JsValue::Object(crate::types::JsObject { id: intl_id });
        self.global_env
            .borrow_mut()
            .declare("Intl", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("Intl", intl_val);
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

        if let JsValue::String(s) = locales {
            let tag = s.to_rust_string();
            let locale: IcuLocale = tag.parse().map_err(|_| {
                self.create_range_error(&format!("Invalid language tag: {}", tag))
            })?;
            seen.push(locale.to_string());
            return Ok(seen);
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

        let len = self.to_number_value(&len_val).unwrap_or(0.0) as u32;

        for i in 0..len {
            let key = i.to_string();
            let k_value = if let JsValue::Object(o) = &obj {
                match self.get_object_property(o.id, &key, &obj) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::Undefined,
                }
            } else {
                JsValue::Undefined
            };

            if matches!(k_value, JsValue::Undefined) {
                continue;
            }

            let tag = match &k_value {
                JsValue::String(s) => s.to_rust_string(),
                JsValue::Object(_) => self.to_string_value(&k_value)?,
                _ => {
                    return Err(self.create_type_error("Language tag must be a string or object"));
                }
            };

            let locale: IcuLocale = tag.parse().map_err(|_| {
                self.create_range_error(&format!("Invalid language tag: {}", tag))
            })?;
            let canonical = locale.to_string();

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
        _options: &JsValue,
    ) -> Result<JsValue, JsValue> {
        let supported: Vec<JsValue> = requested
            .iter()
            .filter_map(|tag| {
                tag.parse::<IcuLocale>().ok().map(|locale| {
                    JsValue::String(JsString::from_str(&locale.to_string()))
                })
            })
            .collect();
        Ok(self.create_array(supported))
    }

    // §9.2.8 ResolveLocale simplified
    pub(crate) fn intl_resolve_locale(&mut self, requested: &[String]) -> String {
        for tag in requested {
            if tag.parse::<IcuLocale>().is_ok() {
                return tag.clone();
            }
        }
        "en".to_string()
    }
}
