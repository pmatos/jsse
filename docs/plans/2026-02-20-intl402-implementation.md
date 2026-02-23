# ECMA-402 (Intl) Full Compliance Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement full ECMA-402 internationalization support targeting all 3,220 test262 intl402 tests.

**Architecture:** ICU4X (`icu` crate v2.0) with compiled CLDR data. New `src/interpreter/builtins/intl/` module tree with one file per constructor. Internal slots stored via `IntlData` enum on `JsObjectData`, following the `TemporalData` pattern.

**Tech Stack:** Rust nightly, `icu` crate 2.0 (umbrella: locale, collator, decimal, datetime, plurals, list, segmenter, displaynames, relativetime, casemap)

---

## Task 1: Add ICU4X Dependency and Create Module Skeleton

**Files:**
- Modify: `Cargo.toml:15` (add icu dependency)
- Create: `src/interpreter/builtins/intl/mod.rs`
- Modify: `src/interpreter/builtins/mod.rs:1-12` (add `mod intl;`)

**Step 1: Add the icu dependency to Cargo.toml**

In `Cargo.toml`, after the last dependency line (line 15), add:

```toml
icu = "2.0"
```

**Step 2: Create the intl module directory and mod.rs**

Create `src/interpreter/builtins/intl/mod.rs` with:

```rust
use super::super::*;

impl Interpreter {
    pub(crate) fn setup_intl(&mut self) {
        let intl_obj = self.create_object();
        intl_obj.borrow_mut().class_name = "Intl".to_string();

        // Intl[@@toStringTag] = "Intl"
        let to_string_tag_key = self.get_well_known_symbol("toStringTag");
        if let Some(key) = to_string_tag_key {
            intl_obj.borrow_mut().insert_property(
                key,
                PropertyDescriptor::data(
                    JsValue::String(JsString::from_str("Intl")),
                    false,
                    false,
                    true,
                ),
            );
        }

        let intl_val = JsValue::Object(crate::types::JsObject {
            id: intl_obj.borrow().id.unwrap(),
        });
        self.global_env
            .borrow_mut()
            .declare("Intl", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("Intl", intl_val);
    }
}
```

**Step 3: Register the intl module in builtins/mod.rs**

In `src/interpreter/builtins/mod.rs`, after the line `mod typedarray;` (line 12), add:

```rust
mod intl;
```

**Step 4: Wire setup_intl into setup_globals**

In `src/interpreter/builtins/mod.rs`, after `self.setup_temporal();` (line 3122), add:

```rust
        // Intl built-in
        self.setup_intl();
```

**Step 5: Build to verify compilation**

Run: `cargo build --release 2>&1 | tail -5`
Expected: Successful build. The `icu` crate will download and compile (may take several minutes on first build due to CLDR data compilation).

**Step 6: Quick smoke test**

Run: `cargo run --release -- -e "typeof Intl"`
Expected: Outputs `object`

**Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/interpreter/builtins/mod.rs src/interpreter/builtins/intl/mod.rs
git commit -m "Add ICU4X dependency and create Intl namespace object"
```

---

## Task 2: Add IntlData Enum to JsObjectData

**Files:**
- Modify: `src/interpreter/types.rs:674` (add IntlData enum before TemporalData)
- Modify: `src/interpreter/types.rs:769` (add intl_data field to JsObjectData)
- Modify: `src/interpreter/types.rs:820` (add intl_data: None to JsObjectData::new())

**Step 1: Add the IntlData enum**

In `src/interpreter/types.rs`, before the `TemporalData` enum definition (line 674), add:

```rust
#[derive(Clone)]
pub(crate) enum IntlData {
    Locale {
        language: String,
        script: Option<String>,
        region: Option<String>,
        calendar: Option<String>,
        case_first: Option<String>,
        collation: Option<String>,
        hour_cycle: Option<String>,
        numbering_system: Option<String>,
        numeric: Option<bool>,
    },
    Collator {
        locale: String,
        usage: String,
        sensitivity: String,
        ignore_punctuation: bool,
        numeric: bool,
        case_first: String,
        collation: String,
    },
    NumberFormat {
        locale: String,
        style: String,
        currency: Option<String>,
        currency_display: Option<String>,
        currency_sign: Option<String>,
        unit: Option<String>,
        unit_display: Option<String>,
        minimum_integer_digits: u32,
        minimum_fraction_digits: Option<u32>,
        maximum_fraction_digits: Option<u32>,
        minimum_significant_digits: Option<u32>,
        maximum_significant_digits: Option<u32>,
        notation: String,
        compact_display: Option<String>,
        sign_display: String,
        rounding_mode: String,
        rounding_increment: u32,
        rounding_priority: String,
        trailing_zero_display: String,
        use_grouping: String,
        numbering_system: String,
    },
    DateTimeFormat {
        locale: String,
        calendar: String,
        numbering_system: String,
        time_zone: String,
        hour_cycle: Option<String>,
        date_style: Option<String>,
        time_style: Option<String>,
        weekday: Option<String>,
        era: Option<String>,
        year: Option<String>,
        month: Option<String>,
        day: Option<String>,
        day_period: Option<String>,
        hour: Option<String>,
        minute: Option<String>,
        second: Option<String>,
        fractional_second_digits: Option<u32>,
        time_zone_name: Option<String>,
    },
    PluralRules {
        locale: String,
        plural_type: String,
        minimum_integer_digits: u32,
        minimum_fraction_digits: u32,
        maximum_fraction_digits: u32,
        minimum_significant_digits: Option<u32>,
        maximum_significant_digits: Option<u32>,
    },
    RelativeTimeFormat {
        locale: String,
        style: String,
        numeric: String,
        numbering_system: String,
    },
    ListFormat {
        locale: String,
        list_type: String,
        style: String,
    },
    Segmenter {
        locale: String,
        granularity: String,
    },
    DisplayNames {
        locale: String,
        display_type: String,
        style: String,
        fallback: String,
        language_display: Option<String>,
    },
    DurationFormat {
        locale: String,
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
        numbering_system: String,
    },
}
```

**Step 2: Add intl_data field to JsObjectData struct**

In `src/interpreter/types.rs`, after `temporal_data: Option<TemporalData>,` (line 768), add:

```rust
    pub(crate) intl_data: Option<IntlData>,
```

**Step 3: Initialize intl_data in JsObjectData::new()**

In `src/interpreter/types.rs`, after `temporal_data: None,` (line 820), add:

```rust
            intl_data: None,
```

**Step 4: Build to verify**

Run: `cargo build --release 2>&1 | tail -3`
Expected: Successful build

**Step 5: Commit**

```bash
git add src/interpreter/types.rs
git commit -m "Add IntlData enum for Intl object internal slots"
```

---

## Task 3: Implement Shared Abstract Operations (§9)

**Files:**
- Modify: `src/interpreter/builtins/intl/mod.rs` (add abstract operations)

**Step 1: Implement option extraction helpers and CanonicalizeLocaleList**

These are the core abstract operations from ECMA-402 §9 that every Intl constructor uses. Add to `src/interpreter/builtins/intl/mod.rs`:

```rust
use super::super::*;
use icu::locale::Locale as IcuLocale;

impl Interpreter {
    // §9.2.12 CoerceOptionsToObject
    pub(crate) fn intl_coerce_options_to_object(
        &mut self,
        options: &JsValue,
    ) -> Result<JsValue, JsValue> {
        if options.is_undefined() {
            let obj = self.create_object();
            return Ok(JsValue::Object(crate::types::JsObject {
                id: obj.borrow().id.unwrap(),
            }));
        }
        self.to_object(options)
    }

    // §9.2.13 GetOption
    pub(crate) fn intl_get_option(
        &mut self,
        options: &JsValue,
        property: &str,
        valid_values: &[&str],
        fallback: Option<&str>,
    ) -> Result<Option<String>, JsValue> {
        let value = self.get_property(options, property)?;
        if value.is_undefined() {
            return Ok(fallback.map(|s| s.to_string()));
        }
        let str_val = self.to_string_value(&value)?;
        if !valid_values.is_empty() && !valid_values.contains(&str_val.as_str()) {
            let err = self.create_range_error(&format!(
                "{str_val} is not a valid value for option {property}"
            ));
            return Err(err);
        }
        Ok(Some(str_val))
    }

    // §9.2.15 GetNumberOption
    pub(crate) fn intl_get_number_option(
        &mut self,
        options: &JsValue,
        property: &str,
        minimum: f64,
        maximum: f64,
        fallback: Option<f64>,
    ) -> Result<Option<f64>, JsValue> {
        let value = self.get_property(options, property)?;
        if value.is_undefined() {
            return Ok(fallback);
        }
        let num = self.to_number_value(&value)?;
        if num.is_nan() || num < minimum || num > maximum {
            let err = self.create_range_error(&format!(
                "{property} value {num} is out of range [{minimum}, {maximum}]"
            ));
            return Err(err);
        }
        Ok(Some(num.floor()))
    }

    // §9.2.1 CanonicalizeLocaleList
    pub(crate) fn intl_canonicalize_locale_list(
        &mut self,
        locales: &JsValue,
    ) -> Result<Vec<String>, JsValue> {
        if locales.is_undefined() {
            return Ok(vec![]);
        }

        let mut seen = Vec::new();

        if let JsValue::String(s) = locales {
            let tag = s.to_string();
            let locale = IcuLocale::try_from_str(&tag).map_err(|_| {
                self.create_range_error(&format!("Invalid language tag: {tag}"))
            })?;
            seen.push(locale.to_string());
            return Ok(seen);
        }

        let obj = self.to_object(locales)?;
        let len_val = self.get_property(&obj, "length")?;
        let len = self.to_length(&len_val)?;

        for k in 0..len {
            let pk = k.to_string();
            let k_present = self.has_property(&obj, &pk)?;
            if k_present {
                let k_value = self.get_property(&obj, &pk)?;
                if !k_value.is_string() && !k_value.is_object() {
                    let err = self.create_type_error(
                        "Language tag must be a string or object"
                    );
                    return Err(err);
                }
                let tag = self.to_string_value(&k_value)?;
                let locale = IcuLocale::try_from_str(&tag).map_err(|_| {
                    self.create_range_error(&format!("Invalid language tag: {tag}"))
                })?;
                let canonical = locale.to_string();
                if !seen.contains(&canonical) {
                    seen.push(canonical);
                }
            }
        }

        Ok(seen)
    }

    // §9.2.9 SupportedLocales (simplified — all ICU4X compiled locales are available)
    pub(crate) fn intl_supported_locales(
        &mut self,
        requested: &[String],
        options: &JsValue,
    ) -> Result<JsValue, JsValue> {
        // For compiled ICU4X data, all CLDR locales are available.
        // BestFitSupportedLocales returns the subset that the engine supports.
        let supported: Vec<JsValue> = requested
            .iter()
            .filter(|tag| IcuLocale::try_from_str(tag).is_ok())
            .map(|tag| JsValue::String(JsString::from_str(tag)))
            .collect();

        let arr = self.create_array_from_values(&supported);
        Ok(arr)
    }

    // Helper: resolve a single locale from a locale list, falling back to default
    pub(crate) fn intl_resolve_locale(
        &mut self,
        requested: &[String],
    ) -> String {
        // Try requested locales in order, fall back to "en"
        for tag in requested {
            if IcuLocale::try_from_str(tag).is_ok() {
                return tag.clone();
            }
        }
        "en".to_string()
    }
}
```

Note: The above implementations are deliberately simplified stubs. Each operation will be refined as we implement individual constructors and discover edge cases from test262 failures. The spec-compliant versions involve more nuance around Unicode extension keys, locale data filtering, and best-fit matching — these will be iteratively hardened.

**Step 2: Build to verify**

Run: `cargo build --release 2>&1 | tail -3`
Expected: Successful build. If there are compilation errors from missing helper methods (`to_object`, `get_property`, `to_length`, `has_property`, `create_range_error`, `create_array_from_values`), check the Interpreter's existing helpers in `src/interpreter/helpers.rs` and `src/interpreter/eval.rs` for the exact signatures and adapt accordingly.

**Step 3: Commit**

```bash
git add src/interpreter/builtins/intl/mod.rs
git commit -m "Implement shared Intl abstract operations (§9)"
```

---

## Task 4: Implement Intl.getCanonicalLocales and Intl.supportedValuesOf

**Files:**
- Modify: `src/interpreter/builtins/intl/mod.rs` (add to setup_intl)

**Step 1: Add getCanonicalLocales to setup_intl**

In `setup_intl()`, before the `let intl_val = ...` line, add:

```rust
        // Intl.getCanonicalLocales(locales) — §8.3.1
        let get_canonical = self.create_function(JsFunction::native(
            "getCanonicalLocales".to_string(),
            1,
            |interp, _this, args| {
                let locales = args.first().cloned().unwrap_or(JsValue::Undefined);
                match interp.intl_canonicalize_locale_list(&locales) {
                    Ok(list) => {
                        let values: Vec<JsValue> = list
                            .into_iter()
                            .map(|s| JsValue::String(JsString::from_str(&s)))
                            .collect();
                        let arr = interp.create_array_from_values(&values);
                        Completion::Normal(arr)
                    }
                    Err(e) => Completion::Throw(e),
                }
            },
        ));
        intl_obj.borrow_mut().insert_builtin(
            "getCanonicalLocales".to_string(),
            get_canonical,
        );
```

**Step 2: Add supportedValuesOf to setup_intl**

```rust
        // Intl.supportedValuesOf(key) — §8.3.2
        let supported_values_of = self.create_function(JsFunction::native(
            "supportedValuesOf".to_string(),
            1,
            |interp, _this, args| {
                let key = args.first().cloned().unwrap_or(JsValue::Undefined);
                let key_str = match interp.to_string_value(&key) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                let values: Vec<&str> = match key_str.as_str() {
                    "calendar" => vec![
                        "buddhist", "chinese", "coptic", "dangi", "ethioaa",
                        "ethiopic", "gregory", "hebrew", "indian", "islamic",
                        "islamic-civil", "islamic-rgsa", "islamic-tbla",
                        "islamic-umalqura", "iso8601", "japanese", "persian", "roc",
                    ],
                    "collation" => vec![
                        "big5han", "compat", "dict", "direct", "ducet",
                        "emoji", "eor", "gb2312", "phonebk", "phonetic",
                        "pinyin", "reformed", "search", "searchjl", "standard",
                        "stroke", "trad", "unihan", "zhuyin",
                    ],
                    "currency" => {
                        // Return a large set of ISO 4217 currency codes
                        // Simplified — full list populated from ICU4X data
                        vec!["AED", "AFN", "ALL", "AMD", "ANG", "AOA", "ARS",
                             "AUD", "AWG", "AZN", "BAM", "BBD", "BDT", "BGN",
                             "BHD", "BIF", "BMD", "BND", "BOB", "BRL", "BSD",
                             "BTN", "BWP", "BYN", "BZD", "CAD", "CDF", "CHF",
                             "CLP", "CNY", "COP", "CRC", "CUP", "CVE", "CZK",
                             "DJF", "DKK", "DOP", "DZD", "EGP", "ERN", "ETB",
                             "EUR", "FJD", "FKP", "GBP", "GEL", "GHS", "GIP",
                             "GMD", "GNF", "GTQ", "GYD", "HKD", "HNL", "HRK",
                             "HTG", "HUF", "IDR", "ILS", "INR", "IQD", "IRR",
                             "ISK", "JMD", "JOD", "JPY", "KES", "KGS", "KHR",
                             "KMF", "KPW", "KRW", "KWD", "KYD", "KZT", "LAK",
                             "LBP", "LKR", "LRD", "LSL", "LYD", "MAD", "MDL",
                             "MGA", "MKD", "MMK", "MNT", "MOP", "MRU", "MUR",
                             "MVR", "MWK", "MXN", "MYR", "MZN", "NAD", "NGN",
                             "NIO", "NOK", "NPR", "NZD", "OMR", "PAB", "PEN",
                             "PGK", "PHP", "PKR", "PLN", "PYG", "QAR", "RON",
                             "RSD", "RUB", "RWF", "SAR", "SBD", "SCR", "SDG",
                             "SEK", "SGD", "SHP", "SLE", "SOS", "SRD", "SSP",
                             "STN", "SVC", "SYP", "SZL", "THB", "TJS", "TMT",
                             "TND", "TOP", "TRY", "TTD", "TWD", "TZS", "UAH",
                             "UGX", "USD", "UYU", "UZS", "VES", "VND", "VUV",
                             "WST", "XAF", "XCD", "XOF", "XPF", "YER", "ZAR",
                             "ZMW", "ZWL"]
                    }
                    "numberingSystem" => vec![
                        "adlm", "ahom", "arab", "arabext", "bali", "beng",
                        "bhks", "brah", "cakm", "cham", "deva", "diak",
                        "fullwide", "gong", "gonm", "gujr", "guru", "hanidec",
                        "hmng", "hmnp", "java", "kali", "khmr", "knda",
                        "lana", "lanatham", "laoo", "latn", "lepc", "limb",
                        "mathbold", "mathdbl", "mathmono", "mathsanb",
                        "mathsans", "mlym", "modi", "mong", "mroo", "mtei",
                        "mymr", "mymrshan", "mymrtlng", "newa", "nkoo",
                        "olck", "orya", "osma", "rohg", "saur", "segment",
                        "shrd", "sind", "sinh", "sora", "sund", "takr",
                        "talu", "tamldec", "telu", "thai", "tibt", "tirh",
                        "vaii", "wara", "wcho",
                    ],
                    "timeZone" => {
                        // Return IANA time zone identifiers
                        // This will be populated from ICU4X data
                        vec!["Africa/Abidjan", "Africa/Accra", "Africa/Addis_Ababa",
                             "Africa/Cairo", "Africa/Casablanca", "Africa/Lagos",
                             "Africa/Nairobi", "America/Anchorage", "America/Buenos_Aires",
                             "America/Chicago", "America/Denver", "America/Los_Angeles",
                             "America/New_York", "America/Sao_Paulo", "America/Toronto",
                             "Asia/Bangkok", "Asia/Calcutta", "Asia/Dubai",
                             "Asia/Hong_Kong", "Asia/Kolkata", "Asia/Shanghai",
                             "Asia/Singapore", "Asia/Tokyo", "Atlantic/Reykjavik",
                             "Australia/Melbourne", "Australia/Sydney",
                             "Europe/Berlin", "Europe/London", "Europe/Moscow",
                             "Europe/Paris", "Pacific/Auckland", "Pacific/Honolulu",
                             "UTC"]
                    }
                    "unit" => vec![
                        "acre", "bit", "byte", "celsius", "centimeter",
                        "day", "degree", "fahrenheit", "fluid-ounce", "foot",
                        "gallon", "gigabit", "gigabyte", "gram", "hectare",
                        "hour", "inch", "kilobit", "kilobyte", "kilogram",
                        "kilometer", "liter", "megabit", "megabyte", "meter",
                        "microsecond", "mile", "mile-scandinavian", "milliliter",
                        "millimeter", "millisecond", "minute", "month",
                        "nanosecond", "ounce", "percent", "petabyte", "pound",
                        "second", "stone", "terabit", "terabyte", "week",
                        "yard", "year",
                    ],
                    _ => {
                        let err = interp.create_range_error(&format!(
                            "Invalid key: {key_str}"
                        ));
                        return Completion::Throw(err);
                    }
                };
                let js_values: Vec<JsValue> = values
                    .into_iter()
                    .map(|s| JsValue::String(JsString::from_str(s)))
                    .collect();
                let arr = interp.create_array_from_values(&js_values);
                Completion::Normal(arr)
            },
        ));
        intl_obj.borrow_mut().insert_builtin(
            "supportedValuesOf".to_string(),
            supported_values_of,
        );
```

Note: The `supportedValuesOf` lists above are deliberately simplified. They should be populated dynamically from ICU4X data at runtime for full compliance. This will be refined when test262 failures reveal missing values.

**Step 2: Build and smoke test**

Run: `cargo build --release && cargo run --release -- -e "Intl.getCanonicalLocales('en-US')"`
Expected: Outputs `en-US` (or the canonical form)

Run: `cargo run --release -- -e "typeof Intl.supportedValuesOf"`
Expected: Outputs `function`

**Step 3: Run intl402 core tests**

Run: `uv run python scripts/run-test262.py test262/test/intl402/Intl/`
Note: Many tests will still fail since they depend on full Locale/Constructor support. Track how many pass.

**Step 4: Commit**

```bash
git add src/interpreter/builtins/intl/mod.rs
git commit -m "Implement Intl.getCanonicalLocales and Intl.supportedValuesOf"
```

---

## Task 5: Add Intl Prototype Fields to Interpreter Struct

**Files:**
- Modify: `src/interpreter/mod.rs:26-77` (add prototype fields)
- Modify: `src/interpreter/mod.rs:129-204` (initialize to None in new())

**Step 1: Add prototype fields for all Intl constructors**

In `src/interpreter/mod.rs`, after `temporal_zoned_date_time_prototype` (line 77), add:

```rust
    intl_collator_prototype: Option<Rc<RefCell<JsObjectData>>>,
    intl_number_format_prototype: Option<Rc<RefCell<JsObjectData>>>,
    intl_date_time_format_prototype: Option<Rc<RefCell<JsObjectData>>>,
    intl_plural_rules_prototype: Option<Rc<RefCell<JsObjectData>>>,
    intl_relative_time_format_prototype: Option<Rc<RefCell<JsObjectData>>>,
    intl_list_format_prototype: Option<Rc<RefCell<JsObjectData>>>,
    intl_segmenter_prototype: Option<Rc<RefCell<JsObjectData>>>,
    intl_segments_prototype: Option<Rc<RefCell<JsObjectData>>>,
    intl_segment_iterator_prototype: Option<Rc<RefCell<JsObjectData>>>,
    intl_display_names_prototype: Option<Rc<RefCell<JsObjectData>>>,
    intl_locale_prototype: Option<Rc<RefCell<JsObjectData>>>,
    intl_duration_format_prototype: Option<Rc<RefCell<JsObjectData>>>,
```

**Step 2: Initialize all to None in Interpreter::new()**

In `src/interpreter/mod.rs`, inside the `Self { ... }` block (after `temporal_zoned_date_time_prototype: None,`), add:

```rust
            intl_collator_prototype: None,
            intl_number_format_prototype: None,
            intl_date_time_format_prototype: None,
            intl_plural_rules_prototype: None,
            intl_relative_time_format_prototype: None,
            intl_list_format_prototype: None,
            intl_segmenter_prototype: None,
            intl_segments_prototype: None,
            intl_segment_iterator_prototype: None,
            intl_display_names_prototype: None,
            intl_locale_prototype: None,
            intl_duration_format_prototype: None,
```

**Step 3: Build to verify**

Run: `cargo build --release 2>&1 | tail -3`
Expected: Successful build

**Step 4: Commit**

```bash
git add src/interpreter/mod.rs
git commit -m "Add Intl prototype fields to Interpreter struct"
```

---

## Task 6: Implement Intl.Locale (Phase 1)

**Files:**
- Create: `src/interpreter/builtins/intl/locale.rs`
- Modify: `src/interpreter/builtins/intl/mod.rs` (add `mod locale;`, call `setup_intl_locale`)

**Step 1: Create locale.rs with constructor and prototype**

Create `src/interpreter/builtins/intl/locale.rs`. This is a large file implementing:

- `Intl.Locale` constructor (§14.1.1): Takes a `tag` string and optional `options` object. Parses via `icu::locale::Locale::try_from_str()`. Options can override `language`, `script`, `region`, `calendar`, `caseFirst`, `collation`, `hourCycle`, `numberingSystem`, `numeric`.
- Prototype getters: `baseName`, `calendar`, `caseFirst`, `collation`, `hourCycle`, `language`, `numberingSystem`, `numeric`, `region`, `script`
- `Locale.prototype.maximize()`: Uses `icu::locale::LocaleExpander::maximize()`
- `Locale.prototype.minimize()`: Uses `icu::locale::LocaleExpander::minimize()`
- `Locale.prototype.toString()`: Returns the canonicalized locale tag

The constructor stores an `IntlData::Locale` on the object.

```rust
use super::super::super::*;
use icu::locale::Locale as IcuLocale;
use icu::locale::LocaleExpander;

impl Interpreter {
    pub(crate) fn setup_intl_locale(&mut self, intl_obj: &Rc<RefCell<JsObjectData>>) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Intl.Locale".to_string();

        // Constructor
        let proto_clone = proto.clone();
        let locale_ctor = self.create_function(JsFunction::constructor(
            "Locale".to_string(),
            1,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    let err = interp.create_type_error(
                        "Intl.Locale must be called with 'new'"
                    );
                    return Completion::Throw(err);
                }

                let tag_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                if tag_arg.is_undefined() {
                    let err = interp.create_type_error(
                        "First argument to Intl.Locale must be a string or Intl.Locale object"
                    );
                    return Completion::Throw(err);
                }

                // Get the tag string
                let tag = match interp.to_string_value(&tag_arg) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };

                // Parse the locale tag
                let mut locale = match IcuLocale::try_from_str(&tag) {
                    Ok(l) => l,
                    Err(_) => {
                        let err = interp.create_range_error(
                            &format!("Invalid language tag: {tag}")
                        );
                        return Completion::Throw(err);
                    }
                };

                // Process options if provided
                let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                if !options.is_undefined() {
                    let opts = match interp.to_object(&options) {
                        Ok(o) => o,
                        Err(e) => return Completion::Throw(e),
                    };

                    // Override language, script, region from options
                    // Each option overrides the corresponding subtag
                    // (Implementation detail: apply unicode extension keywords)
                    // This will be refined as tests reveal requirements
                }

                let language = locale.id.language.to_string();
                let script = if locale.id.script.is_some() {
                    Some(locale.id.script.unwrap().to_string())
                } else {
                    None
                };
                let region = if locale.id.region.is_some() {
                    Some(locale.id.region.unwrap().to_string())
                } else {
                    None
                };

                // Extract unicode extension keywords
                let calendar = None; // TODO: extract from locale extensions
                let case_first = None;
                let collation = None;
                let hour_cycle = None;
                let numbering_system = None;
                let numeric = None;

                // Create the Locale object
                let obj = interp.create_object();
                obj.borrow_mut().prototype = Some(proto_clone.clone());
                obj.borrow_mut().class_name = "Intl.Locale".to_string();
                obj.borrow_mut().intl_data = Some(IntlData::Locale {
                    language,
                    script,
                    region,
                    calendar,
                    case_first,
                    collation,
                    hour_cycle,
                    numbering_system,
                    numeric,
                });

                let obj_val = JsValue::Object(crate::types::JsObject {
                    id: obj.borrow().id.unwrap(),
                });
                Completion::Normal(obj_val)
            },
        ));

        // Add getter methods to prototype
        // Each getter reads from IntlData::Locale

        // toString / toJSON
        let to_string_fn = self.create_function(JsFunction::native(
            "toString".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let b = obj.borrow();
                    if let Some(IntlData::Locale { ref language, ref script, ref region, .. }) = b.intl_data {
                        let mut tag = language.clone();
                        if let Some(s) = script {
                            tag.push('-');
                            tag.push_str(s);
                        }
                        if let Some(r) = region {
                            tag.push('-');
                            tag.push_str(r);
                        }
                        return Completion::Normal(
                            JsValue::String(JsString::from_str(&tag))
                        );
                    }
                }
                let err = interp.create_type_error("Not an Intl.Locale object");
                Completion::Throw(err)
            },
        ));
        proto.borrow_mut().insert_builtin("toString".to_string(), to_string_fn.clone());
        proto.borrow_mut().insert_builtin("toJSON".to_string(), to_string_fn);

        // Getter: language
        let language_getter = self.create_function(JsFunction::native(
            "get language".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let b = obj.borrow();
                    if let Some(IntlData::Locale { ref language, .. }) = b.intl_data {
                        return Completion::Normal(
                            JsValue::String(JsString::from_str(language))
                        );
                    }
                }
                let err = interp.create_type_error("Not an Intl.Locale object");
                Completion::Throw(err)
            },
        ));
        proto.borrow_mut().insert_property(
            "language".to_string(),
            PropertyDescriptor::accessor(Some(language_getter), None, true, true),
        );

        // Similarly add getters for: script, region, baseName, calendar, caseFirst,
        // collation, hourCycle, numberingSystem, numeric
        // (Each follows the same pattern as language_getter above)

        // maximize() and minimize()
        let maximize_fn = self.create_function(JsFunction::native(
            "maximize".to_string(),
            0,
            |interp, this, _args| {
                // Read the locale tag from this, call LocaleExpander::maximize,
                // return new Intl.Locale with maximized tag
                // TODO: implement
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto.borrow_mut().insert_builtin("maximize".to_string(), maximize_fn);

        let minimize_fn = self.create_function(JsFunction::native(
            "minimize".to_string(),
            0,
            |interp, this, _args| {
                // TODO: implement
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto.borrow_mut().insert_builtin("minimize".to_string(), minimize_fn);

        // Constructor ↔ Prototype linking
        let proto_val = JsValue::Object(crate::types::JsObject {
            id: proto.borrow().id.unwrap(),
        });
        if let JsValue::Object(ctor_obj) = &locale_ctor
            && let Some(obj) = self.get_object(ctor_obj.id)
        {
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val.clone(), false, false, false),
            );
        }
        proto.borrow_mut().insert_property(
            "constructor".to_string(),
            PropertyDescriptor::data(locale_ctor.clone(), true, false, true),
        );

        // supportedLocalesOf static method
        let supported_locales_of = self.create_function(JsFunction::native(
            "supportedLocalesOf".to_string(),
            1,
            |interp, _this, args| {
                let locales = args.first().cloned().unwrap_or(JsValue::Undefined);
                let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                match interp.intl_canonicalize_locale_list(&locales) {
                    Ok(list) => match interp.intl_supported_locales(&list, &options) {
                        Ok(arr) => Completion::Normal(arr),
                        Err(e) => Completion::Throw(e),
                    },
                    Err(e) => Completion::Throw(e),
                }
            },
        ));
        if let JsValue::Object(ctor_obj) = &locale_ctor
            && let Some(obj) = self.get_object(ctor_obj.id)
        {
            obj.borrow_mut().insert_builtin(
                "supportedLocalesOf".to_string(),
                supported_locales_of,
            );
        }

        // Register Locale on the Intl object
        intl_obj.borrow_mut().insert_builtin("Locale".to_string(), locale_ctor);
        self.intl_locale_prototype = Some(proto);
    }
}
```

Note: This is a skeleton. Many getters and the options handling in the constructor are marked TODO. They will be filled in iteratively as test262 tests are run. The structure is correct — tests will guide which parts need flesh.

**Step 2: Wire locale.rs into the intl module**

In `src/interpreter/builtins/intl/mod.rs`, add at the top:

```rust
mod locale;
```

And in `setup_intl()`, before the intl_val registration, add:

```rust
        self.setup_intl_locale(&intl_obj);
```

**Step 3: Build and test**

Run: `cargo build --release && cargo run --release -- -e "new Intl.Locale('en-US').language"`
Expected: Outputs `en`

Run: `uv run python scripts/run-test262.py test262/test/intl402/Locale/`
Track pass count.

**Step 4: Commit**

```bash
git add src/interpreter/builtins/intl/locale.rs src/interpreter/builtins/intl/mod.rs
git commit -m "Implement Intl.Locale constructor and prototype (Phase 1)"
```

---

## Task 7: Implement Intl.Collator (Phase 2)

**Files:**
- Create: `src/interpreter/builtins/intl/collator.rs`
- Modify: `src/interpreter/builtins/intl/mod.rs` (add `mod collator;`, call setup)

**Step 1: Create collator.rs**

The `Intl.Collator` constructor:
- Accepts `locales` and `options` arguments
- Resolves locale via `CanonicalizeLocaleList` + `ResolveLocale`
- Options: `usage` (sort/search), `sensitivity` (base/accent/case/variant), `ignorePunctuation`, `numeric`, `caseFirst`, `collation`
- Creates an `icu::collator::Collator` instance
- `compare()` returns a bound comparison function
- `resolvedOptions()` returns the resolved options object

Pattern: Same as Locale (constructor with `new` check, prototype methods, `supportedLocalesOf` static, `resolvedOptions`).

Store internal data as `IntlData::Collator`. The `compare` getter lazily creates a bound compare function that uses the ICU4X collator.

**Step 2: Wire into mod.rs and build**

**Step 3: Test**

Run: `uv run python scripts/run-test262.py test262/test/intl402/Collator/`
Track pass count.

**Step 4: Commit**

```bash
git add src/interpreter/builtins/intl/collator.rs src/interpreter/builtins/intl/mod.rs
git commit -m "Implement Intl.Collator constructor and prototype (Phase 2)"
```

---

## Task 8: Implement Intl.NumberFormat (Phase 3)

**Files:**
- Create: `src/interpreter/builtins/intl/numberformat.rs`
- Modify: `src/interpreter/builtins/intl/mod.rs`

**Step 1: Create numberformat.rs**

The `Intl.NumberFormat` constructor:
- Options: `style` (decimal/currency/percent/unit), `currency`, `currencyDisplay`, `currencySign`, `unit`, `unitDisplay`, `notation` (standard/scientific/engineering/compact), `compactDisplay`, `signDisplay`, `useGrouping`, `minimumIntegerDigits`, `minimumFractionDigits`, `maximumFractionDigits`, `minimumSignificantDigits`, `maximumSignificantDigits`, `roundingMode`, `roundingIncrement`, `roundingPriority`, `trailingZeroDisplay`, `numberingSystem`
- `format()` returns a bound function; `formatToParts()` returns array of objects
- `formatRange()` and `formatRangeToParts()` for range formatting
- ICU4X: `icu::decimal::DecimalFormatter` for basic decimal, plus potentially `icu::compactdecimal` for compact notation

Store as `IntlData::NumberFormat`.

**Step 2: Wire and build**

**Step 3: Test**

Run: `uv run python scripts/run-test262.py test262/test/intl402/NumberFormat/`
Track pass count.

**Step 4: Commit**

```bash
git add src/interpreter/builtins/intl/numberformat.rs src/interpreter/builtins/intl/mod.rs
git commit -m "Implement Intl.NumberFormat constructor and prototype (Phase 3)"
```

---

## Task 9: Implement Intl.DateTimeFormat (Phase 4)

**Files:**
- Create: `src/interpreter/builtins/intl/datetimeformat.rs`
- Modify: `src/interpreter/builtins/intl/mod.rs`

**Step 1: Create datetimeformat.rs**

The `Intl.DateTimeFormat` constructor:
- Options: `dateStyle`, `timeStyle`, `calendar`, `dayPeriod`, `numberingSystem`, `timeZone`, `hour12`, `hourCycle`, `weekday`, `era`, `year`, `month`, `day`, `hour`, `minute`, `second`, `fractionalSecondDigits`, `timeZoneName`
- `format()` returns a bound function; `formatToParts()` returns array of objects
- `formatRange()` and `formatRangeToParts()` for range formatting
- ICU4X: `icu::datetime::DateTimeFormatter` with various `fieldsets` for component selection

Store as `IntlData::DateTimeFormat`.

Note: This is one of the most complex constructors due to the large number of option combinations and the need to map between ECMA-402's component model and ICU4X's fieldset model.

**Step 2: Wire and build**

**Step 3: Test**

Run: `uv run python scripts/run-test262.py test262/test/intl402/DateTimeFormat/`
Track pass count.

**Step 4: Commit**

```bash
git add src/interpreter/builtins/intl/datetimeformat.rs src/interpreter/builtins/intl/mod.rs
git commit -m "Implement Intl.DateTimeFormat constructor and prototype (Phase 4)"
```

---

## Task 10: Implement Intl.PluralRules (Phase 5)

**Files:**
- Create: `src/interpreter/builtins/intl/pluralrules.rs`
- Modify: `src/interpreter/builtins/intl/mod.rs`

**Step 1: Create pluralrules.rs**

The `Intl.PluralRules` constructor:
- Options: `type` (cardinal/ordinal), `minimumIntegerDigits`, `minimumFractionDigits`, `maximumFractionDigits`, `minimumSignificantDigits`, `maximumSignificantDigits`
- `select(value)` returns one of: "zero", "one", "two", "few", "many", "other"
- `selectRange(start, end)` returns plural category for a range
- ICU4X: `icu::plurals::PluralRules` with `PluralRules::try_new_cardinal()` or `try_new_ordinal()`

Store as `IntlData::PluralRules`.

**Step 2: Wire, build, test**

Run: `uv run python scripts/run-test262.py test262/test/intl402/PluralRules/`

**Step 3: Commit**

```bash
git add src/interpreter/builtins/intl/pluralrules.rs src/interpreter/builtins/intl/mod.rs
git commit -m "Implement Intl.PluralRules constructor and prototype (Phase 5)"
```

---

## Task 11: Implement Intl.RelativeTimeFormat (Phase 6)

**Files:**
- Create: `src/interpreter/builtins/intl/relativetimeformat.rs`
- Modify: `src/interpreter/builtins/intl/mod.rs`

**Step 1: Create relativetimeformat.rs**

The `Intl.RelativeTimeFormat` constructor:
- Options: `localeMatcher`, `numeric` (always/auto), `style` (long/short/narrow), `numberingSystem`
- `format(value, unit)` — value is a number, unit is one of: year/quarter/month/week/day/hour/minute/second
- `formatToParts(value, unit)` — returns array of part objects
- ICU4X: `icu::relativetime::RelativeTimeFormatter`

Store as `IntlData::RelativeTimeFormat`.

**Step 2: Wire, build, test**

Run: `uv run python scripts/run-test262.py test262/test/intl402/RelativeTimeFormat/`

**Step 3: Commit**

```bash
git add src/interpreter/builtins/intl/relativetimeformat.rs src/interpreter/builtins/intl/mod.rs
git commit -m "Implement Intl.RelativeTimeFormat constructor and prototype (Phase 6)"
```

---

## Task 12: Implement Intl.ListFormat (Phase 7)

**Files:**
- Create: `src/interpreter/builtins/intl/listformat.rs`
- Modify: `src/interpreter/builtins/intl/mod.rs`

**Step 1: Create listformat.rs**

The `Intl.ListFormat` constructor:
- Options: `localeMatcher`, `type` (conjunction/disjunction/unit), `style` (long/short/narrow)
- `format(list)` — list is an iterable of strings
- `formatToParts(list)` — returns array of {type, value} objects
- ICU4X: `icu::list::ListFormatter` with `try_new_and()`, `try_new_or()`, `try_new_unit()`

Store as `IntlData::ListFormat`.

**Step 2: Wire, build, test**

Run: `uv run python scripts/run-test262.py test262/test/intl402/ListFormat/`

**Step 3: Commit**

```bash
git add src/interpreter/builtins/intl/listformat.rs src/interpreter/builtins/intl/mod.rs
git commit -m "Implement Intl.ListFormat constructor and prototype (Phase 7)"
```

---

## Task 13: Implement Intl.Segmenter (Phase 8)

**Files:**
- Create: `src/interpreter/builtins/intl/segmenter.rs`
- Modify: `src/interpreter/builtins/intl/mod.rs`

**Step 1: Create segmenter.rs**

The `Intl.Segmenter` constructor:
- Options: `localeMatcher`, `granularity` (grapheme/word/sentence)
- `segment(string)` — returns a `Segments` object
- `Segments` object: `[@@iterator]()` returns segment iterator, `containing(index)` returns segment at index
- Segment iterator yields `{segment, index, input, isWordLike?}` objects
- ICU4X: `icu::segmenter::{GraphemeClusterSegmenter, WordSegmenter, SentenceSegmenter}`

This task requires implementing three connected objects: Segmenter, Segments, and SegmentIterator. Use `intl_segments_prototype` and `intl_segment_iterator_prototype` from the Interpreter struct.

Store as `IntlData::Segmenter`.

**Step 2: Wire, build, test**

Run: `uv run python scripts/run-test262.py test262/test/intl402/Segmenter/`

**Step 3: Commit**

```bash
git add src/interpreter/builtins/intl/segmenter.rs src/interpreter/builtins/intl/mod.rs
git commit -m "Implement Intl.Segmenter constructor and prototype (Phase 8)"
```

---

## Task 14: Implement Intl.DisplayNames (Phase 9)

**Files:**
- Create: `src/interpreter/builtins/intl/displaynames.rs`
- Modify: `src/interpreter/builtins/intl/mod.rs`

**Step 1: Create displaynames.rs**

The `Intl.DisplayNames` constructor:
- Options (required): `type` (language/region/script/calendar/dateTimeField/currency)
- Options (optional): `localeMatcher`, `style` (long/short/narrow), `fallback` (code/none), `languageDisplay` (standard/dialect)
- `of(code)` — returns the display name string for the given code
- ICU4X: `icu::displaynames::{LocaleDisplayNamesFormatter, RegionDisplayNames, ScriptDisplayNames}`

Store as `IntlData::DisplayNames`.

**Step 2: Wire, build, test**

Run: `uv run python scripts/run-test262.py test262/test/intl402/DisplayNames/`

**Step 3: Commit**

```bash
git add src/interpreter/builtins/intl/displaynames.rs src/interpreter/builtins/intl/mod.rs
git commit -m "Implement Intl.DisplayNames constructor and prototype (Phase 9)"
```

---

## Task 15: Implement Intl.DurationFormat (Phase 10)

**Files:**
- Create: `src/interpreter/builtins/intl/durationformat.rs`
- Modify: `src/interpreter/builtins/intl/mod.rs`

**Step 1: Create durationformat.rs**

The `Intl.DurationFormat` constructor:
- Options: `style` (long/short/narrow/digital), per-unit styles for years/months/weeks/days/hours/minutes/seconds/milliseconds/microseconds/nanoseconds (long/short/narrow/numeric/2-digit), per-unit display (always/auto), `fractionalDigits`, `numberingSystem`
- `format(duration)` — duration is a Temporal.Duration-like object with {years, months, ...} fields
- `formatToParts(duration)` — returns array of part objects
- ICU4X: Duration formatting may need to be implemented manually using ListFormat + NumberFormat since ICU4X's duration formatting may be limited.

Store as `IntlData::DurationFormat`.

**Step 2: Wire, build, test**

Run: `uv run python scripts/run-test262.py test262/test/intl402/DurationFormat/`

**Step 3: Commit**

```bash
git add src/interpreter/builtins/intl/durationformat.rs src/interpreter/builtins/intl/mod.rs
git commit -m "Implement Intl.DurationFormat constructor and prototype (Phase 10)"
```

---

## Task 16: Implement Locale-aware Prototype Overrides (Phase 11)

**Files:**
- Modify: `src/interpreter/builtins/string.rs` (localeCompare, toLocaleLowerCase, toLocaleUpperCase)
- Modify: `src/interpreter/builtins/number.rs` (Number.prototype.toLocaleString)
- Modify: `src/interpreter/builtins/bigint.rs` (BigInt.prototype.toLocaleString)
- Modify: `src/interpreter/builtins/date.rs` (toLocaleString, toLocaleDateString, toLocaleTimeString)
- Modify: `src/interpreter/builtins/array.rs` (Array.prototype.toLocaleString)
- Modify: `src/interpreter/builtins/typedarray.rs` (TypedArray.prototype.toLocaleString)

**Step 1: String.prototype.localeCompare(that [, locales [, options]])**

In `string.rs`, find the existing `localeCompare` implementation (it likely uses a simple string comparison). Replace it with one that:
1. Creates a temporary `Intl.Collator` with the given locales/options
2. Uses its compare function
3. Returns -1, 0, or 1

If no `localeCompare` exists yet, add it.

**Step 2: String.prototype.toLocaleLowerCase/toLocaleUpperCase**

Use `icu::casemap::CaseMapper` with the resolved locale:

```rust
let case_mapper = icu::casemap::CaseMapper::new();
let result = case_mapper.lowercase_to_string(&input, &langid);
// or uppercase_to_string for toLocaleUpperCase
```

**Step 3: Number.prototype.toLocaleString**

Create a temporary `Intl.NumberFormat` with the given locales/options and use it to format the number.

**Step 4: BigInt.prototype.toLocaleString**

Same pattern as Number, but converting BigInt to a suitable input for NumberFormat.

**Step 5: Date.prototype.toLocaleString / toLocaleDateString / toLocaleTimeString**

Create a temporary `Intl.DateTimeFormat` with:
- `toLocaleString`: both date and time components
- `toLocaleDateString`: date components only
- `toLocaleTimeString`: time components only

**Step 6: Array.prototype.toLocaleString / TypedArray.prototype.toLocaleString**

Call `toLocaleString()` on each element and join with locale-appropriate list separator (from Intl-aware logic).

**Step 7: Test**

Run: `uv run python scripts/run-test262.py test262/test/intl402/String/ test262/test/intl402/Number/ test262/test/intl402/Date/ test262/test/intl402/BigInt/ test262/test/intl402/Array/ test262/test/intl402/TypedArray/`

**Step 8: Commit**

```bash
git add src/interpreter/builtins/string.rs src/interpreter/builtins/number.rs src/interpreter/builtins/bigint.rs src/interpreter/builtins/date.rs src/interpreter/builtins/array.rs src/interpreter/builtins/typedarray.rs
git commit -m "Implement locale-aware prototype overrides (Phase 11)"
```

---

## Task 17: Implement Temporal Intl Integration (Phase 12)

**Files:**
- Modify: `src/interpreter/builtins/temporal/` (various files for toLocaleString)

**Step 1: Add toLocaleString to Temporal types**

For each Temporal type that has a `toLocaleString` method:
- `Temporal.Instant.prototype.toLocaleString()`
- `Temporal.PlainDate.prototype.toLocaleString()`
- `Temporal.PlainTime.prototype.toLocaleString()`
- `Temporal.PlainDateTime.prototype.toLocaleString()`
- `Temporal.PlainYearMonth.prototype.toLocaleString()`
- `Temporal.PlainMonthDay.prototype.toLocaleString()`
- `Temporal.ZonedDateTime.prototype.toLocaleString()`

Each creates a temporary `Intl.DateTimeFormat` and formats the temporal value.

**Step 2: Test**

Run: `uv run python scripts/run-test262.py test262/test/intl402/Temporal/`
This is the largest phase (1,919 tests).

**Step 3: Commit**

```bash
git add src/interpreter/builtins/temporal/
git commit -m "Implement Temporal intl integration (Phase 12)"
```

---

## Task 18: Run Top-Level intl402 Tests

**Files:**
- No new files — test-driven fixes

**Step 1: Run the 22 top-level intl402 tests**

Run: `uv run python scripts/run-test262.py test262/test/intl402/*.js`

These test cross-cutting concerns like:
- `constructors-taint-Object-prototype.js` — constructor option handling with tainted Object.prototype
- `language-tags-valid.js` / `language-tags-invalid.js` — locale tag validation
- `supportedLocalesOf-*` — common supportedLocalesOf behavior
- `default-locale-is-canonicalized.js` — default locale handling

**Step 2: Fix failures iteratively**

Each failure will point to a specific abstract operation or constructor behavior that needs refinement.

**Step 3: Commit fixes**

```bash
git add -u
git commit -m "Fix top-level intl402 test failures"
```

---

## Task 19: Full intl402 Test Suite Run and Progress Update

**Files:**
- Modify: `README.md` (update test counts)
- Modify: `PLAN.md` (update Intl status)

**Step 1: Run the full intl402 test suite**

Run: `uv run python scripts/run-test262.py test262/test/intl402/`

Record the total pass/fail counts.

**Step 2: Run the full test262 suite to check for regressions**

Run: `uv run python scripts/run-test262.py`

Verify that existing core test pass rates haven't regressed.

**Step 3: Update README.md with new counts**

Add an intl402 section to the test262 progress table.

**Step 4: Update PLAN.md**

Add Intl status to the built-in status table.

**Step 5: Commit**

```bash
git add README.md PLAN.md test262-pass.txt
git commit -m "Update test262 progress with intl402 results"
```

---

## Task 20: Iterative Test-Driven Hardening

**Files:**
- All intl/ files as needed

This is an open-ended task. After the initial implementation, many tests will fail due to:
- Edge cases in locale negotiation
- Missing Unicode extension keyword handling
- Subtle option resolution differences
- formatToParts() output structure mismatches
- Missing error conditions (wrong argument types, out-of-range values)

**Process:**
1. Pick the constructor with the worst pass rate
2. Run its test262 tests: `uv run python scripts/run-test262.py test262/test/intl402/<Constructor>/`
3. Examine failing tests to understand what's missing
4. Implement the fix
5. Re-run tests to confirm improvement
6. Commit

Repeat until satisfied with pass rates.

**Commit pattern:**

```bash
git add -u
git commit -m "Fix Intl.<Constructor>: <description of what was fixed> (+N passes)"
```

---

## Implementation Order Summary

| Task | Phase | Component | Test Target | Dependencies |
|------|-------|-----------|-------------|--------------|
| 1 | 0 | ICU4X dependency + module skeleton | Build | None |
| 2 | 0 | IntlData enum | Build | Task 1 |
| 3 | 0 | Shared abstract operations | Build | Task 2 |
| 4 | 0 | getCanonicalLocales + supportedValuesOf | ~88 tests | Task 3 |
| 5 | 0 | Interpreter prototype fields | Build | Task 1 |
| 6 | 1 | Intl.Locale | 152 tests | Tasks 3-5 |
| 7 | 2 | Intl.Collator | 65 tests | Task 6 |
| 8 | 3 | Intl.NumberFormat | 271 tests | Task 6 |
| 9 | 4 | Intl.DateTimeFormat | 243 tests | Task 6 |
| 10 | 5 | Intl.PluralRules | 52 tests | Task 6 |
| 11 | 6 | Intl.RelativeTimeFormat | 80 tests | Task 6 |
| 12 | 7 | Intl.ListFormat | 81 tests | Task 6 |
| 13 | 8 | Intl.Segmenter | 79 tests | Task 6 |
| 14 | 9 | Intl.DisplayNames | 57 tests | Task 6 |
| 15 | 10 | Intl.DurationFormat | 111 tests | Task 6 |
| 16 | 11 | Prototype overrides | ~52 tests | Tasks 7-9 |
| 17 | 12 | Temporal intl | 1,919 tests | Task 9 |
| 18 | — | Top-level tests | 22 tests | Tasks 3-4 |
| 19 | — | Full suite + progress | All | All |
| 20 | — | Iterative hardening | All | All |
