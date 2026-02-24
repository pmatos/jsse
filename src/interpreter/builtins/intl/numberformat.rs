use super::super::super::*;
use fixed_decimal::{
    Decimal, FloatPrecision, RoundingIncrement, SignDisplay, SignedRoundingMode,
    UnsignedRoundingMode,
};
use icu::decimal::options::{DecimalFormatterOptions, GroupingStrategy};
use icu::decimal::{DecimalFormatter, DecimalFormatterPreferences};
use icu::locale::Locale as IcuLocale;

fn locale_nan_string(locale: &str) -> &'static str {
    let lang = locale.split('-').next().unwrap_or(locale).split('_').next().unwrap_or(locale);
    match lang {
        "zh" => {
            if locale.contains("TW") || locale.contains("Hant") || locale.contains("HK") || locale.contains("MO") {
                "\u{975E}\u{6578}\u{503C}" // 非數值 (Traditional Chinese)
            } else {
                "\u{975E}\u{6570}\u{5B57}" // 非数字 (Simplified Chinese)
            }
        }
        "ar" => "\u{0644}\u{064A}\u{0633}\u{0020}\u{0631}\u{0642}\u{0645}\u{064B}\u{0627}", // ليس رقمًا
        _ => "NaN",
    }
}

fn locale_infinity_string(_locale: &str) -> &'static str {
    "\u{221E}" // ∞
}

fn currency_digits(currency: &str) -> u32 {
    match currency.to_ascii_uppercase().as_str() {
        "BHD" | "IQD" | "JOD" | "KWD" | "LYD" | "OMR" | "TND" => 3,
        "BIF" | "CLP" | "DJF" | "GNF" | "ISK" | "JPY" | "KMF" | "KRW" | "PYG" | "RWF"
        | "UGX" | "UYI" | "VND" | "VUV" | "XAF" | "XOF" | "XPF" => 0,
        _ => 2,
    }
}

pub(crate) fn is_known_numbering_system(ns: &str) -> bool {
    matches!(
        ns,
        "adlm" | "ahom" | "arab" | "arabext" | "bali" | "beng" | "bhks" | "brah"
            | "cakm" | "cham" | "deva" | "diak" | "fullwide" | "gong" | "gonm"
            | "gujr" | "guru" | "hanidec" | "hmng" | "hmnp" | "java" | "kali"
            | "kawi" | "khmr" | "knda" | "lana" | "lanatham" | "laoo" | "latn" | "lepc"
            | "limb" | "mathbold" | "mathdbl" | "mathmono" | "mathsanb" | "mathsans"
            | "mlym" | "modi" | "mong" | "mroo" | "mtei" | "mymr" | "mymrshan"
            | "mymrtlng" | "nagm" | "newa" | "nkoo" | "olck" | "orya" | "osma"
            | "rohg" | "saur" | "segment" | "shrd" | "sind" | "sinh" | "sora"
            | "sund" | "takr" | "talu" | "tamldec" | "telu" | "thai" | "tibt"
            | "tirh" | "tnsa" | "vaii" | "wara" | "wcho"
    )
}

fn is_well_formed_currency_code(code: &str) -> bool {
    code.len() == 3 && code.chars().all(|c| c.is_ascii_alphabetic())
}

fn is_well_formed_unit_identifier(unit: &str) -> bool {
    if unit.contains("-per-") {
        let parts: Vec<&str> = unit.splitn(2, "-per-").collect();
        if parts.len() == 2 {
            return is_sanctioned_single_unit(parts[0]) && is_sanctioned_single_unit(parts[1]);
        }
        return false;
    }
    is_sanctioned_single_unit(unit)
}

fn is_sanctioned_single_unit(unit: &str) -> bool {
    matches!(
        unit,
        "acre"
            | "bit"
            | "byte"
            | "celsius"
            | "centimeter"
            | "day"
            | "degree"
            | "fahrenheit"
            | "fluid-ounce"
            | "foot"
            | "gallon"
            | "gigabit"
            | "gigabyte"
            | "gram"
            | "hectare"
            | "hour"
            | "inch"
            | "kilobit"
            | "kilobyte"
            | "kilogram"
            | "kilometer"
            | "liter"
            | "megabit"
            | "megabyte"
            | "meter"
            | "microsecond"
            | "mile"
            | "mile-scandinavian"
            | "milliliter"
            | "millimeter"
            | "millisecond"
            | "minute"
            | "month"
            | "nanosecond"
            | "ounce"
            | "percent"
            | "petabyte"
            | "pound"
            | "second"
            | "stone"
            | "terabit"
            | "terabyte"
            | "week"
            | "yard"
            | "year"
    )
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
    let parsed: Result<IcuLocale, _> = strip_unicode_extensions(locale_str).parse();
    match parsed {
        Ok(loc) => loc.to_string(),
        Err(_) => strip_unicode_extensions(locale_str),
    }
}

fn currency_symbol(currency: &str, display: &str) -> String {
    currency_symbol_locale(currency, display, "en")
}

fn currency_symbol_locale(currency: &str, display: &str, locale: &str) -> String {
    if display == "code" {
        return currency.to_ascii_uppercase();
    }
    if display == "name" {
        return currency_name(currency);
    }
    let lang = locale.split('-').next().unwrap_or(locale).split('_').next().unwrap_or(locale);
    // symbol or narrowSymbol
    match currency.to_ascii_uppercase().as_str() {
        "USD" => {
            if display == "narrowSymbol" || matches!(lang, "en" | "ja" | "de" | "fr") {
                "$".to_string()
            } else {
                "US$".to_string()
            }
        }
        "EUR" => "\u{20AC}".to_string(),
        "GBP" => "\u{00A3}".to_string(),
        "JPY" | "CNY" => "\u{00A5}".to_string(),
        "KRW" => "\u{20A9}".to_string(),
        "INR" => "\u{20B9}".to_string(),
        "RUB" => "\u{20BD}".to_string(),
        "BRL" => "R$".to_string(),
        "CAD" | "AUD" | "NZD" | "HKD" | "SGD" | "MXN" | "ARS" | "CLP" | "COP" => {
            if display == "narrowSymbol" {
                "$".to_string()
            } else {
                format!("{}$", &currency[..2])
            }
        }
        "CHF" => "CHF".to_string(),
        "SEK" | "NOK" | "DKK" | "ISK" | "CZK" => "kr".to_string(),
        "PLN" => "z\u{0142}".to_string(),
        "THB" => "\u{0E3F}".to_string(),
        "TRY" => "\u{20BA}".to_string(),
        "ILS" => "\u{20AA}".to_string(),
        "ZAR" => "R".to_string(),
        "TWD" => {
            if display == "narrowSymbol" {
                "$".to_string()
            } else {
                "NT$".to_string()
            }
        }
        other => other.to_string(),
    }
}

fn currency_name(currency: &str) -> String {
    match currency.to_ascii_uppercase().as_str() {
        "USD" => "US dollars".to_string(),
        "EUR" => "euros".to_string(),
        "GBP" => "British pounds".to_string(),
        "JPY" => "Japanese yen".to_string(),
        "CNY" => "Chinese yuan".to_string(),
        "KRW" => "South Korean won".to_string(),
        "INR" => "Indian rupees".to_string(),
        "CAD" => "Canadian dollars".to_string(),
        "AUD" => "Australian dollars".to_string(),
        "CHF" => "Swiss francs".to_string(),
        "BRL" => "Brazilian reais".to_string(),
        other => other.to_string(),
    }
}

fn singular_unit_name(unit: &str) -> &str {
    match unit {
        "celsius" => "degree Celsius",
        "fahrenheit" => "degree Fahrenheit",
        "kilometer" => "kilometer",
        "meter" => "meter",
        "centimeter" => "centimeter",
        "millimeter" => "millimeter",
        "mile" => "mile",
        "foot" => "foot",
        "inch" => "inch",
        "yard" => "yard",
        "kilogram" => "kilogram",
        "gram" => "gram",
        "pound" => "pound",
        "ounce" => "ounce",
        "liter" => "liter",
        "milliliter" => "milliliter",
        "gallon" => "gallon",
        "hour" => "hour",
        "minute" => "minute",
        "second" => "second",
        "millisecond" => "millisecond",
        "microsecond" => "microsecond",
        "nanosecond" => "nanosecond",
        "day" => "day",
        "week" => "week",
        "month" => "month",
        "year" => "year",
        "byte" => "byte",
        "kilobyte" => "kilobyte",
        "megabyte" => "megabyte",
        "gigabyte" => "gigabyte",
        "terabyte" => "terabyte",
        "petabyte" => "petabyte",
        "bit" => "bit",
        "kilobit" => "kilobit",
        "megabit" => "megabit",
        "gigabit" => "gigabit",
        "terabit" => "terabit",
        "acre" => "acre",
        "hectare" => "hectare",
        "percent" => "percent",
        "degree" => "degree",
        "stone" => "stone",
        "fluid-ounce" => "fluid ounce",
        "mile-scandinavian" => "Scandinavian mile",
        other => other,
    }
}

fn unit_symbol(unit: &str, display: &str) -> String {
    let (prefix, suffix) = locale_unit_pattern(unit, display, "en", 2.0);
    if prefix.is_empty() {
        suffix
    } else {
        format!("{}{{NUM}}{}", prefix, suffix)
    }
}

// Returns (prefix, suffix) for locale-aware unit formatting.
// prefix is empty for suffix-only patterns (most cases).
// For circumfix patterns (ja long, ko long, zh-TW long), prefix is non-empty.
fn locale_unit_pattern(unit: &str, display: &str, locale: &str, value: f64) -> (String, String) {
    let lang = locale.split('-').next().unwrap_or(locale).split('_').next().unwrap_or(locale);

    if unit.contains("-per-") {
        let parts: Vec<&str> = unit.splitn(2, "-per-").collect();
        let numerator = parts[0];
        let denominator = parts[1];

        if numerator == "kilometer" && denominator == "hour" {
            return locale_kph_pattern(lang, display, locale);
        }

        if display == "long" {
            let num = single_unit_symbol(numerator, display, value);
            let den_singular = singular_unit_name(denominator);
            return ("".to_string(), format!("{} per {}", num, den_singular));
        }
        let num = single_unit_symbol(numerator, "narrow", value);
        let den = single_unit_symbol(denominator, "narrow", value);
        return ("".to_string(), format!("{}/{}", num, den.trim_start()));
    }

    let sym = locale_single_unit_symbol(unit, display, lang, value);
    ("".to_string(), sym)
}

fn locale_single_unit_symbol(unit: &str, display: &str, lang: &str, value: f64) -> String {
    match lang {
        "de" => match display {
            "long" => match unit {
                "kilometer" => " Kilometer".to_string(),
                "meter" => " Meter".to_string(),
                "centimeter" => " Zentimeter".to_string(),
                "hour" => if is_plural_en(value) { " Stunden".to_string() } else { " Stunde".to_string() },
                _ => single_unit_symbol(unit, display, value),
            },
            _ => single_unit_symbol(unit, display, value),
        },
        "ja" => match display {
            "long" => match unit {
                "kilometer" => " \u{30AD}\u{30ED}\u{30E1}\u{30FC}\u{30C8}\u{30EB}".to_string(),
                _ => single_unit_symbol(unit, display, value),
            },
            _ => single_unit_symbol(unit, display, value),
        },
        "ko" => match display {
            "long" => match unit {
                "kilometer" => "\u{D0AC}\u{B85C}\u{BBF8}\u{D130}".to_string(),
                _ => single_unit_symbol(unit, display, value),
            },
            _ => single_unit_symbol(unit, display, value),
        },
        "zh" => match display {
            "long" => match unit {
                "kilometer" => " \u{516C}\u{91CC}".to_string(),
                _ => single_unit_symbol(unit, display, value),
            },
            "short" => match unit {
                "kilometer" => " \u{516C}\u{91CC}".to_string(),
                _ => single_unit_symbol(unit, display, value),
            },
            _ => single_unit_symbol(unit, display, value),
        },
        _ => single_unit_symbol(unit, display, value),
    }
}

fn locale_kph_pattern(lang: &str, display: &str, locale: &str) -> (String, String) {
    match lang {
        "de" => match display {
            "short" | "narrow" => ("".to_string(), " km/h".to_string()),
            "long" => ("".to_string(), " Kilometer pro Stunde".to_string()),
            _ => ("".to_string(), " km/h".to_string()),
        },
        "ja" => match display {
            "short" => ("".to_string(), " km/h".to_string()),
            "narrow" => ("".to_string(), "km/h".to_string()),
            "long" => (
                "\u{6642}\u{901F} ".to_string(),  // 時速 (with trailing space)
                " \u{30AD}\u{30ED}\u{30E1}\u{30FC}\u{30C8}\u{30EB}".to_string(), // キロメートル (with leading space)
            ),
            _ => ("".to_string(), " km/h".to_string()),
        },
        "ko" => match display {
            "short" | "narrow" => ("".to_string(), "km/h".to_string()),
            "long" => (
                "\u{C2DC}\u{C18D} ".to_string(),  // 시속 (with trailing space)
                "\u{D0AC}\u{B85C}\u{BBF8}\u{D130}".to_string(), // 킬로미터 (no leading space)
            ),
            _ => ("".to_string(), "km/h".to_string()),
        },
        "zh" => {
            let is_traditional = locale.contains("TW") || locale.contains("Hant")
                || locale.contains("HK") || locale.contains("MO");
            if !is_traditional {
                // Simplified Chinese - use default English-like pattern
                match display {
                    "short" => ("".to_string(), " km/h".to_string()),
                    "narrow" => ("".to_string(), "km/h".to_string()),
                    "long" => ("".to_string(), " kilometers per hour".to_string()),
                    _ => ("".to_string(), " km/h".to_string()),
                }
            } else {
                match display {
                    "short" => ("".to_string(), " \u{516C}\u{91CC}/\u{5C0F}\u{6642}".to_string()), // 公里/小時
                    "narrow" => ("".to_string(), "\u{516C}\u{91CC}/\u{5C0F}\u{6642}".to_string()),  // 公里/小時
                    "long" => (
                        "\u{6BCF}\u{5C0F}\u{6642} ".to_string(),  // 每小時 (with trailing space)
                        " \u{516C}\u{91CC}".to_string(),           // 公里 (with leading space)
                    ),
                    _ => ("".to_string(), " \u{516C}\u{91CC}/\u{5C0F}\u{6642}".to_string()),
                }
            }
        },
        _ => {
            // English default
            match display {
                "short" => ("".to_string(), " km/h".to_string()),
                "narrow" => ("".to_string(), "km/h".to_string()),
                "long" => ("".to_string(), " kilometers per hour".to_string()),
                _ => ("".to_string(), " km/h".to_string()),
            }
        }
    }
}

fn is_plural_en(value: f64) -> bool {
    !(value.abs() == 1.0 && value.fract() == 0.0)
}

fn single_unit_symbol(unit: &str, display: &str, value: f64) -> String {
    match display {
        "long" => {
            if is_plural_en(value) {
                match unit {
                    "celsius" => " degrees Celsius".to_string(),
                    "fahrenheit" => " degrees Fahrenheit".to_string(),
                    "kilometer" => " kilometers".to_string(),
                    "meter" => " meters".to_string(),
                    "centimeter" => " centimeters".to_string(),
                    "millimeter" => " millimeters".to_string(),
                    "mile" => " miles".to_string(),
                    "foot" => " feet".to_string(),
                    "inch" => " inches".to_string(),
                    "yard" => " yards".to_string(),
                    "kilogram" => " kilograms".to_string(),
                    "gram" => " grams".to_string(),
                    "pound" => " pounds".to_string(),
                    "ounce" => " ounces".to_string(),
                    "liter" => " liters".to_string(),
                    "milliliter" => " milliliters".to_string(),
                    "gallon" => " gallons".to_string(),
                    "hour" => " hours".to_string(),
                    "minute" => " minutes".to_string(),
                    "second" => " seconds".to_string(),
                    "millisecond" => " milliseconds".to_string(),
                    "microsecond" => " microseconds".to_string(),
                    "nanosecond" => " nanoseconds".to_string(),
                    "day" => " days".to_string(),
                    "week" => " weeks".to_string(),
                    "month" => " months".to_string(),
                    "year" => " years".to_string(),
                    "byte" => " bytes".to_string(),
                    "kilobyte" => " kilobytes".to_string(),
                    "megabyte" => " megabytes".to_string(),
                    "gigabyte" => " gigabytes".to_string(),
                    "terabyte" => " terabytes".to_string(),
                    "petabyte" => " petabytes".to_string(),
                    "bit" => " bits".to_string(),
                    "kilobit" => " kilobits".to_string(),
                    "megabit" => " megabits".to_string(),
                    "gigabit" => " gigabits".to_string(),
                    "terabit" => " terabits".to_string(),
                    "acre" => " acres".to_string(),
                    "hectare" => " hectares".to_string(),
                    "percent" => " percent".to_string(),
                    "degree" => " degrees".to_string(),
                    "stone" => " stone".to_string(),
                    "fluid-ounce" => " fluid ounces".to_string(),
                    "mile-scandinavian" => " Scandinavian miles".to_string(),
                    other => format!(" {}", other),
                }
            } else {
                match unit {
                    "celsius" => " degree Celsius".to_string(),
                    "fahrenheit" => " degree Fahrenheit".to_string(),
                    "foot" => " foot".to_string(),
                    "inch" => " inch".to_string(),
                    "fluid-ounce" => " fluid ounce".to_string(),
                    "mile-scandinavian" => " Scandinavian mile".to_string(),
                    other => format!(" {}", singular_unit_name(other)),
                }
            }
        }
        "narrow" => match unit {
            "celsius" => "\u{00B0}C".to_string(),
            "fahrenheit" => "\u{00B0}F".to_string(),
            "kilometer" => "km".to_string(),
            "meter" => "m".to_string(),
            "centimeter" => "cm".to_string(),
            "millimeter" => "mm".to_string(),
            "mile" => "mi".to_string(),
            "foot" => "ft".to_string(),
            "inch" => "in".to_string(),
            "yard" => "yd".to_string(),
            "kilogram" => "kg".to_string(),
            "gram" => "g".to_string(),
            "pound" => "lb".to_string(),
            "ounce" => "oz".to_string(),
            "liter" => "L".to_string(),
            "milliliter" => "mL".to_string(),
            "gallon" => "gal".to_string(),
            "hour" => "h".to_string(),
            "minute" => "min".to_string(),
            "second" => "s".to_string(),
            "millisecond" => "ms".to_string(),
            "microsecond" => "\u{03BC}s".to_string(),
            "nanosecond" => "ns".to_string(),
            "day" => "d".to_string(),
            "week" => "w".to_string(),
            "month" => "mo".to_string(),
            "year" => "y".to_string(),
            "byte" => "B".to_string(),
            "kilobyte" => "kB".to_string(),
            "megabyte" => "MB".to_string(),
            "gigabyte" => "GB".to_string(),
            "terabyte" => "TB".to_string(),
            "petabyte" => "PB".to_string(),
            "bit" => "bit".to_string(),
            "kilobit" => "kbit".to_string(),
            "megabit" => "Mbit".to_string(),
            "gigabit" => "Gbit".to_string(),
            "terabit" => "Tbit".to_string(),
            "acre" => "ac".to_string(),
            "hectare" => "ha".to_string(),
            "percent" => "%".to_string(),
            "degree" => "\u{00B0}".to_string(),
            "stone" => "st".to_string(),
            "fluid-ounce" => "fl oz".to_string(),
            "mile-scandinavian" => "smi".to_string(),
            other => other.to_string(),
        },
        _ => {
            // "short" (default) - some units need singular/plural
            if is_plural_en(value) {
                match unit {
                    "celsius" => " \u{00B0}C".to_string(),
                    "fahrenheit" => " \u{00B0}F".to_string(),
                    "kilometer" => " km".to_string(),
                    "meter" => " m".to_string(),
                    "centimeter" => " cm".to_string(),
                    "millimeter" => " mm".to_string(),
                    "mile" => " mi".to_string(),
                    "foot" => " ft".to_string(),
                    "inch" => " in".to_string(),
                    "yard" => " yd".to_string(),
                    "kilogram" => " kg".to_string(),
                    "gram" => " g".to_string(),
                    "pound" => " lb".to_string(),
                    "ounce" => " oz".to_string(),
                    "liter" => " L".to_string(),
                    "milliliter" => " mL".to_string(),
                    "gallon" => " gal".to_string(),
                    "hour" => " hr".to_string(),
                    "minute" => " min".to_string(),
                    "second" => " sec".to_string(),
                    "millisecond" => " ms".to_string(),
                    "microsecond" => " \u{03BC}s".to_string(),
                    "nanosecond" => " ns".to_string(),
                    "day" => " days".to_string(),
                    "week" => " wks".to_string(),
                    "month" => " mths".to_string(),
                    "year" => " yrs".to_string(),
                    "byte" => " byte".to_string(),
                    "kilobyte" => " kB".to_string(),
                    "megabyte" => " MB".to_string(),
                    "gigabyte" => " GB".to_string(),
                    "terabyte" => " TB".to_string(),
                    "petabyte" => " PB".to_string(),
                    "bit" => " bit".to_string(),
                    "kilobit" => " kbit".to_string(),
                    "megabit" => " Mbit".to_string(),
                    "gigabit" => " Gbit".to_string(),
                    "terabit" => " Tbit".to_string(),
                    "acre" => " ac".to_string(),
                    "hectare" => " ha".to_string(),
                    "percent" => "%".to_string(),
                    "degree" => "\u{00B0}".to_string(),
                    "stone" => " st".to_string(),
                    "fluid-ounce" => " fl oz".to_string(),
                    "mile-scandinavian" => " smi".to_string(),
                    other => format!(" {}", other),
                }
            } else {
                match unit {
                    "celsius" => " \u{00B0}C".to_string(),
                    "fahrenheit" => " \u{00B0}F".to_string(),
                    "kilometer" => " km".to_string(),
                    "meter" => " m".to_string(),
                    "centimeter" => " cm".to_string(),
                    "millimeter" => " mm".to_string(),
                    "mile" => " mi".to_string(),
                    "foot" => " ft".to_string(),
                    "inch" => " in".to_string(),
                    "yard" => " yd".to_string(),
                    "kilogram" => " kg".to_string(),
                    "gram" => " g".to_string(),
                    "pound" => " lb".to_string(),
                    "ounce" => " oz".to_string(),
                    "liter" => " L".to_string(),
                    "milliliter" => " mL".to_string(),
                    "gallon" => " gal".to_string(),
                    "hour" => " hr".to_string(),
                    "minute" => " min".to_string(),
                    "second" => " sec".to_string(),
                    "millisecond" => " ms".to_string(),
                    "microsecond" => " \u{03BC}s".to_string(),
                    "nanosecond" => " ns".to_string(),
                    "day" => " day".to_string(),
                    "week" => " wk".to_string(),
                    "month" => " mth".to_string(),
                    "year" => " yr".to_string(),
                    "byte" => " byte".to_string(),
                    "kilobyte" => " kB".to_string(),
                    "megabyte" => " MB".to_string(),
                    "gigabyte" => " GB".to_string(),
                    "terabyte" => " TB".to_string(),
                    "petabyte" => " PB".to_string(),
                    "bit" => " bit".to_string(),
                    "kilobit" => " kbit".to_string(),
                    "megabit" => " Mbit".to_string(),
                    "gigabit" => " Gbit".to_string(),
                    "terabit" => " Tbit".to_string(),
                    "acre" => " ac".to_string(),
                    "hectare" => " ha".to_string(),
                    "percent" => "%".to_string(),
                    "degree" => "\u{00B0}".to_string(),
                    "stone" => " st".to_string(),
                    "fluid-ounce" => " fl oz".to_string(),
                    "mile-scandinavian" => " smi".to_string(),
                    other => format!(" {}", other),
                }
            }
        }
    }
}

fn js_rounding_mode_to_fd(mode: &str) -> SignedRoundingMode {
    match mode {
        "ceil" => SignedRoundingMode::Ceil,
        "floor" => SignedRoundingMode::Floor,
        "expand" => SignedRoundingMode::Unsigned(UnsignedRoundingMode::Expand),
        "trunc" => SignedRoundingMode::Unsigned(UnsignedRoundingMode::Trunc),
        "halfCeil" => SignedRoundingMode::HalfCeil,
        "halfFloor" => SignedRoundingMode::HalfFloor,
        "halfTrunc" => SignedRoundingMode::Unsigned(UnsignedRoundingMode::HalfTrunc),
        "halfEven" => SignedRoundingMode::Unsigned(UnsignedRoundingMode::HalfEven),
        _ => SignedRoundingMode::Unsigned(UnsignedRoundingMode::HalfExpand),
    }
}

pub(crate) fn numbering_system_zero(ns: &str) -> Option<char> {
    match ns {
        "arab" => Some('\u{0660}'),
        "arabext" => Some('\u{06F0}'),
        "beng" => Some('\u{09E6}'),
        "deva" => Some('\u{0966}'),
        "fullwide" => Some('\u{FF10}'),
        "gujr" => Some('\u{0AE6}'),
        "guru" => Some('\u{0A66}'),
        "hanidec" => Some('\u{3007}'),
        "khmr" => Some('\u{17E0}'),
        "knda" => Some('\u{0CE6}'),
        "laoo" => Some('\u{0ED0}'),
        "limb" => Some('\u{1946}'),
        "mlym" => Some('\u{0D66}'),
        "mong" => Some('\u{1810}'),
        "mymr" => Some('\u{1040}'),
        "orya" => Some('\u{0B66}'),
        "tamldec" => Some('\u{0BE6}'),
        "telu" => Some('\u{0C66}'),
        "thai" => Some('\u{0E50}'),
        "tibt" => Some('\u{0F20}'),
        "bali" => Some('\u{1B50}'),
        "brah" => Some('\u{11066}'),
        "cakm" => Some('\u{11136}'),
        "cham" => Some('\u{AA50}'),
        "java" => Some('\u{A9D0}'),
        "kawi" => Some('\u{11F50}'),
        "lana" => Some('\u{1A80}'),
        "lepc" => Some('\u{1C40}'),
        "mathbold" => Some('\u{1D7CE}'),
        "mathdbl" => Some('\u{1D7D8}'),
        "mathmono" => Some('\u{1D7F6}'),
        "mathsanb" => Some('\u{1D7EC}'),
        "mathsans" => Some('\u{1D7E2}'),
        "mroo" => Some('\u{16A60}'),
        "mtei" => Some('\u{ABF0}'),
        "nagm" => Some('\u{1E4F0}'),
        "nkoo" => Some('\u{07C0}'),
        "olck" => Some('\u{1C50}'),
        "osma" => Some('\u{104A0}'),
        "rohg" => Some('\u{10D30}'),
        "saur" => Some('\u{A8D0}'),
        "segment" => Some('\u{1FBF0}'),
        "shrd" => Some('\u{111D0}'),
        "sora" => Some('\u{110F0}'),
        "sund" => Some('\u{1BB0}'),
        "takr" => Some('\u{116C0}'),
        "talu" => Some('\u{19D0}'),
        "tnsa" => Some('\u{16AC0}'),
        "vaii" => Some('\u{A620}'),
        "wara" => Some('\u{118E0}'),
        "adlm" => Some('\u{1E950}'),
        "ahom" => Some('\u{11730}'),
        "bhks" => Some('\u{11C50}'),
        "diak" => Some('\u{11950}'),
        "gong" => Some('\u{11DA0}'),
        "gonm" => Some('\u{11D50}'),
        "hmng" => Some('\u{16B50}'),
        "hmnp" => Some('\u{1E140}'),
        "kali" => Some('\u{A900}'),
        "lanatham" => Some('\u{1A90}'),
        "modi" => Some('\u{11650}'),
        "mymrshan" => Some('\u{1090}'),
        "mymrtlng" => Some('\u{A9F0}'),
        "newa" => Some('\u{11450}'),
        "sind" => Some('\u{112F0}'),
        "sinh" => Some('\u{0DE6}'),
        "tirh" => Some('\u{114D0}'),
        "wcho" => Some('\u{1E2F0}'),
        "latn" | "" => None,
        _ => None,
    }
}

pub(crate) fn transliterate_digits(s: &str, ns: &str) -> String {
    if ns == "hanidec" {
        let hanidec_digits: [char; 10] = [
            '\u{3007}', '\u{4E00}', '\u{4E8C}', '\u{4E09}', '\u{56DB}',
            '\u{4E94}', '\u{516D}', '\u{4E03}', '\u{516B}', '\u{4E5D}',
        ];
        return s.chars().map(|c| {
            if let Some(d) = c.to_digit(10) {
                hanidec_digits[d as usize]
            } else {
                c
            }
        }).collect();
    }

    let use_arabic_separators = ns == "arab" || ns == "arabext";

    match numbering_system_zero(ns) {
        None => {
            if use_arabic_separators {
                apply_arabic_separators(s, ns)
            } else {
                s.to_string()
            }
        }
        Some(zero) => {
            let zero_val = zero as u32;
            let result: String = s.chars().map(|c| {
                if let Some(d) = c.to_digit(10) {
                    char::from_u32(zero_val + d).unwrap_or(c)
                } else {
                    c
                }
            }).collect();
            if use_arabic_separators {
                apply_arabic_separators(&result, ns)
            } else {
                result
            }
        }
    }
}

pub(crate) fn apply_arabic_separators(s: &str, ns: &str) -> String {
    let mut result = String::with_capacity(s.len() * 2);
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut i = 0;
    while i < len {
        match chars[i] {
            '.' => result.push('\u{066B}'),
            ',' => result.push('\u{066C}'),
            '\u{200E}' if ns == "arab" => {
                // Replace LRM with ALM for arab numbering system
                result.push('\u{061C}');
            }
            c => result.push(c),
        }
        i += 1;
    }
    result
}

fn js_rounding_increment_to_fd(inc: u32) -> RoundingIncrement {
    match inc {
        2 => RoundingIncrement::MultiplesOf2,
        5 => RoundingIncrement::MultiplesOf5,
        10 => RoundingIncrement::MultiplesOf2, // approx: 10 = 2*5
        20 => RoundingIncrement::MultiplesOf2,
        25 => RoundingIncrement::MultiplesOf25,
        50 => RoundingIncrement::MultiplesOf5,
        100 => RoundingIncrement::MultiplesOf2,
        200 => RoundingIncrement::MultiplesOf2,
        500 => RoundingIncrement::MultiplesOf5,
        1000 => RoundingIncrement::MultiplesOf2,
        2000 => RoundingIncrement::MultiplesOf2,
        5000 => RoundingIncrement::MultiplesOf5,
        _ => RoundingIncrement::MultiplesOf1,
    }
}

fn js_sign_display_to_fd(sign_display: &str) -> SignDisplay {
    match sign_display {
        "always" => SignDisplay::Always,
        "exceptZero" => SignDisplay::ExceptZero,
        "negative" => SignDisplay::Negative,
        "never" => SignDisplay::Never,
        _ => SignDisplay::Auto,
    }
}

fn grouping_strategy_from_str(s: &str) -> GroupingStrategy {
    match s {
        "always" => GroupingStrategy::Always,
        "min2" => GroupingStrategy::Min2,
        "false" => GroupingStrategy::Never,
        "true" => GroupingStrategy::Always,
        _ => GroupingStrategy::Auto,
    }
}

fn currency_position_after(locale: &str) -> bool {
    let lang = locale.split('-').next().unwrap_or(locale).split('_').next().unwrap_or(locale);
    matches!(lang, "de" | "fr" | "es" | "pt" | "nl" | "it" | "ca" | "da" | "fi" | "nb" | "nn" | "no" | "sv" | "pl" | "cs" | "sk" | "hu" | "ro" | "bg" | "hr" | "sl" | "sr" | "tr" | "el" | "uk" | "ru" | "be" | "et" | "lv" | "lt" | "vi" | "id" | "ms")
}

fn locale_uses_narrow_currency(locale: &str, cur_code: &str) -> bool {
    if cur_code == "USD" {
        let lang = locale.split('-').next().unwrap_or(locale).split('_').next().unwrap_or(locale);
        match lang {
            "en" => false,
            "de" | "fr" | "es" | "pt" | "nl" | "it" => false,
            _ => true,
        }
    } else {
        false
    }
}

fn locale_percent_has_space(locale: &str) -> bool {
    let lang = locale.split('-').next().unwrap_or(locale).split('_').next().unwrap_or(locale);
    matches!(lang, "de" | "fr" | "es" | "pt" | "nl" | "it" | "ca" | "da" | "fi" | "nb" | "nn"
        | "no" | "sv" | "pl" | "cs" | "sk" | "hu" | "ro" | "bg" | "hr" | "sl" | "sr" | "tr"
        | "el" | "uk" | "ru" | "be" | "et" | "lv" | "lt" | "ar" | "he" | "fa" | "hi" | "bn"
        | "ta" | "te" | "mr" | "gu" | "kn" | "ml" | "si" | "th" | "ka" | "hy" | "az" | "kk"
        | "uz" | "ky" | "mn" | "sq" | "mk" | "bs" | "mt" | "is" | "ga" | "cy" | "eu" | "gl"
        | "af" | "zu" | "xh" | "sw" | "rw" | "gv")
}

fn wrap_style(
    num_str: &str,
    style: &str,
    currency: &Option<String>,
    currency_display: &Option<String>,
    currency_sign: &Option<String>,
    unit: &Option<String>,
    unit_display: &Option<String>,
    value: f64,
    locale: &str,
) -> String {
    match style {
        "currency" => {
            let cur = currency.as_deref().unwrap_or("USD");
            let cur_disp = currency_display.as_deref().unwrap_or("symbol");
            let cur_sign = currency_sign.as_deref().unwrap_or("standard");
            let sym = currency_symbol_locale(cur, cur_disp, locale);
            let after = currency_position_after(locale) && cur_disp != "name";

            let is_neg = num_str.starts_with('-') || num_str.starts_with('\u{2212}');
            let has_plus = num_str.starts_with('+');
            if cur_disp == "name" {
                format!("{} {}", num_str, sym)
            } else if after {
                let sep = "\u{00A0}";
                if cur_sign == "accounting" && is_neg {
                    let abs_str = num_str.trim_start_matches('-').trim_start_matches('\u{2212}');
                    format!("-{}{}{}", abs_str, sep, sym)
                } else {
                    format!("{}{}{}", num_str, sep, sym)
                }
            } else if cur_sign == "accounting" && is_neg {
                let abs_str = num_str.trim_start_matches('-').trim_start_matches('\u{2212}');
                format!("({}{})", sym, abs_str)
            } else {
                if is_neg {
                    let c = num_str.chars().next().unwrap();
                    let rest = &num_str[c.len_utf8()..];
                    format!("-{}{}", sym, rest)
                } else if has_plus {
                    format!("+{}{}", sym, &num_str[1..])
                } else {
                    format!("{}{}", sym, num_str)
                }
            }
        }
        "percent" => {
            if locale_percent_has_space(locale) {
                format!("{}\u{00A0}%", num_str)
            } else {
                format!("{}%", num_str)
            }
        }
        "unit" => {
            let u = unit.as_deref().unwrap_or("degree");
            let u_disp = unit_display.as_deref().unwrap_or("short");
            let (prefix, suffix) = locale_unit_pattern(u, u_disp, locale, value);
            if prefix.is_empty() {
                format!("{}{}", num_str, suffix)
            } else {
                format!("{}{}{}", prefix, num_str, suffix)
            }
        }
        _ => num_str.to_string(),
    }
}

fn intl_to_number(interp: &mut Interpreter, val: &JsValue) -> Result<f64, JsValue> {
    match val {
        JsValue::BigInt(bi) => {
            let s = bi.value.to_string();
            Ok(s.parse::<f64>().unwrap_or(if s.starts_with('-') { f64::NEG_INFINITY } else { f64::INFINITY }))
        }
        _ => interp.to_number_value(val),
    }
}

pub(crate) fn format_number_internal(
    value: f64,
    locale_str: &str,
    style: &str,
    currency: &Option<String>,
    currency_display: &Option<String>,
    currency_sign: &Option<String>,
    unit: &Option<String>,
    unit_display: &Option<String>,
    notation: &str,
    compact_display: &Option<String>,
    sign_display: &str,
    use_grouping: &str,
    minimum_integer_digits: u32,
    minimum_fraction_digits: u32,
    maximum_fraction_digits: u32,
    minimum_significant_digits: &Option<u32>,
    maximum_significant_digits: &Option<u32>,
    rounding_mode: &str,
    rounding_increment: u32,
    rounding_priority: &str,
    trailing_zero_display: &str,
    numbering_system: &str,
) -> String {
    if value.is_nan() {
        let nan_str = locale_nan_string(locale_str);
        let sign_prefix = match sign_display {
            "always" => "+",
            _ => "",
        };
        let num_str = format!("{}{}", sign_prefix, nan_str);
        return transliterate_digits(&wrap_style(&num_str, style, currency, currency_display, currency_sign, unit, unit_display, 0.0, locale_str), numbering_system);
    }
    if value.is_infinite() {
        let inf_str = locale_infinity_string(locale_str);
        let sign_prefix = match sign_display {
            "always" | "exceptZero" => {
                if value > 0.0 { "+" } else { "-" }
            }
            "never" => "",
            "negative" => {
                if value < 0.0 { "-" } else { "" }
            }
            _ => { // "auto"
                if value < 0.0 { "-" } else { "" }
            }
        };
        let num_str = format!("{}{}", sign_prefix, inf_str);
        return transliterate_digits(&wrap_style(&num_str, style, currency, currency_display, currency_sign, unit, unit_display, value, locale_str), numbering_system);
    }

    let work_value = match style {
        "percent" => value * 100.0,
        _ => value,
    };

    // Scientific/engineering notation
    if notation == "scientific" || notation == "engineering" {
        return transliterate_digits(&format_scientific(
            work_value,
            notation,
            sign_display,
            minimum_integer_digits,
            minimum_fraction_digits,
            maximum_fraction_digits,
            minimum_significant_digits,
            maximum_significant_digits,
            style,
            currency,
            currency_display,
            unit,
            unit_display,
            locale_str,
        ), numbering_system);
    }

    // Compact notation
    if notation == "compact" {
        return transliterate_digits(&format_compact(
            work_value,
            compact_display.as_deref().unwrap_or("short"),
            sign_display,
            locale_str,
            use_grouping,
            minimum_integer_digits,
            minimum_fraction_digits,
            maximum_fraction_digits,
            minimum_significant_digits,
            maximum_significant_digits,
            style,
            currency,
            currency_display,
            unit,
            unit_display,
        ), numbering_system);
    }

    let base = base_locale(locale_str);
    let locale: IcuLocale = base.parse().unwrap_or_else(|_| "en".parse().unwrap());
    let prefs = DecimalFormatterPreferences::from(&locale);
    let mut opts = DecimalFormatterOptions::default();
    opts.grouping_strategy = Some(grouping_strategy_from_str(use_grouping));
    let formatter = DecimalFormatter::try_new(prefs, opts)
        .unwrap_or_else(|_| DecimalFormatter::try_new(Default::default(), opts).unwrap());

    let work_value = if rounding_increment > 1 && minimum_significant_digits.is_none() {
        let scale = 10f64.powi(maximum_fraction_digits as i32);
        let raw_scaled = work_value * scale;
        // Snap to nearest integer to avoid floating-point imprecision
        // (e.g. 1.15 * 100 = 114.99999999999999 instead of 115)
        let scaled = if (raw_scaled - raw_scaled.round()).abs() < 1e-8 {
            raw_scaled.round()
        } else {
            raw_scaled
        };
        let ri = rounding_increment as f64;
        let rounded = match rounding_mode {
            "ceil" => (scaled / ri).ceil() * ri,
            "floor" => (scaled / ri).floor() * ri,
            "trunc" => {
                if scaled >= 0.0 { (scaled / ri).floor() * ri } else { (scaled / ri).ceil() * ri }
            }
            "expand" => {
                if scaled >= 0.0 { (scaled / ri).ceil() * ri } else { (scaled / ri).floor() * ri }
            }
            "halfFloor" => {
                let q = scaled / ri;
                let lo = q.floor();
                let hi = q.ceil();
                if (q - lo).abs() < (hi - q).abs() {
                    lo * ri
                } else if (hi - q).abs() < (q - lo).abs() {
                    hi * ri
                } else {
                    lo * ri // tie goes to floor (negative infinity)
                }
            }
            "halfCeil" => {
                let q = scaled / ri;
                let lo = q.floor();
                let hi = q.ceil();
                if (q - lo).abs() < (hi - q).abs() {
                    lo * ri
                } else if (hi - q).abs() < (q - lo).abs() {
                    hi * ri
                } else {
                    hi * ri // tie goes to ceil (positive infinity)
                }
            }
            "halfTrunc" => {
                let q = scaled / ri;
                let lo = q.floor();
                let hi = q.ceil();
                let dl = (q - lo).abs();
                let dh = (hi - q).abs();
                if dl < dh { lo * ri } else if dh < dl { hi * ri }
                else if scaled >= 0.0 { lo * ri } else { hi * ri }
            }
            "halfEven" => {
                let q = scaled / ri;
                let lo = q.floor();
                let hi = q.ceil();
                let dl = (q - lo).abs();
                let dh = (hi - q).abs();
                if dl < dh {
                    lo * ri
                } else if dh < dl {
                    hi * ri
                } else {
                    let lo_v = (lo * ri).round() as i64;
                    let hi_v = (hi * ri).round() as i64;
                    if lo_v % 2 == 0 { lo * ri } else { hi * ri }
                }
            }
            _ => {
                // halfExpand (default)
                let q = scaled / ri;
                let lo = q.floor();
                let hi = q.ceil();
                let dl = (q - lo).abs();
                let dh = (hi - q).abs();
                if dl < dh { lo * ri } else if dh < dl { hi * ri }
                else if scaled >= 0.0 { hi * ri } else { lo * ri }
            }
        };
        rounded / scale
    } else {
        work_value
    };

    let (mut dec, used_sd) = if rounding_priority != "auto" && minimum_significant_digits.is_some() {
        // roundingPriority: "lessPrecision" or "morePrecision"
        // Format with sig digits
        let min_sd = minimum_significant_digits.unwrap();
        let max_sd = maximum_significant_digits.unwrap_or(min_sd);
        let mut dec_sd = format_with_significant_digits(work_value, min_sd, max_sd, rounding_mode);

        // Format with fraction digits
        let mut dec_fd = match Decimal::try_from_f64(work_value, FloatPrecision::RoundTrip) {
            Ok(d) => d,
            Err(_) => match Decimal::try_from_str(&format!("{}", work_value)) {
                Ok(d) => d,
                Err(_) => Decimal::from(0),
            },
        };
        let mode = js_rounding_mode_to_fd(rounding_mode);
        dec_fd.round_with_mode(-(maximum_fraction_digits as i16), mode);
        dec_fd.absolute.trim_end();
        if minimum_fraction_digits > 0 {
            dec_fd.absolute.pad_end(-(minimum_fraction_digits as i16));
        }

        // Compare by rounding magnitude per spec:
        // SD rounding magnitude = floor(log10(|x|)) - maxSD + 1
        // FD rounding magnitude = -maxFD
        // morePrecision: pick lower magnitude (more precise rounding point)
        // lessPrecision: pick higher magnitude (less precise rounding point)
        let sd_mag: i32 = if work_value.abs() == 0.0 {
            1 - max_sd as i32
        } else {
            work_value.abs().log10().floor() as i32 - max_sd as i32 + 1
        };
        let fd_mag: i32 = -(maximum_fraction_digits as i32);

        let use_sd = if rounding_priority == "lessPrecision" {
            sd_mag >= fd_mag
        } else {
            // morePrecision (or auto with both specified)
            sd_mag <= fd_mag
        };

        let rp_used_sd = use_sd;
        (if use_sd { dec_sd } else { dec_fd }, rp_used_sd)
    } else if let Some(min_sd) = minimum_significant_digits {
        let max_sd = maximum_significant_digits.unwrap_or(*min_sd);
        (format_with_significant_digits(work_value, *min_sd, max_sd, rounding_mode), true)
    } else {
        (match Decimal::try_from_f64(work_value, FloatPrecision::RoundTrip) {
            Ok(d) => d,
            Err(_) => match Decimal::try_from_str(&format!("{}", work_value)) {
                Ok(d) => d,
                Err(_) => Decimal::from(0),
            },
        }, false)
    };

    // Apply rounding to max fraction digits (only when not using sig digits directly)
    if !used_sd {
        let mode = js_rounding_mode_to_fd(rounding_mode);
        dec.round_with_mode(-(maximum_fraction_digits as i16), mode);
        dec.absolute.trim_end();
    }

    // Pad fractional digits to minimum (only when not using sig digits result from rounding priority)
    if !used_sd && minimum_fraction_digits > 0 {
        dec.absolute.pad_end(-(minimum_fraction_digits as i16));
    }

    // Pad integer digits
    if minimum_integer_digits > 1 {
        dec.absolute.pad_start((minimum_integer_digits as i16) - 1);
    }

    // trailing zero display
    if trailing_zero_display == "stripIfInteger" {
        dec.absolute.trim_end_if_integer();
    }

    // sign display
    dec.apply_sign_display(js_sign_display_to_fd(sign_display));

    let formatted = formatter.format(&dec);
    let mut num_str = formatted.to_string();

    // Post-process to ensure minimum integer digits
    // pad_start doesn't always work correctly for zero values, so handle manually
    if minimum_integer_digits > 1 {
        // Find the position of the first digit character (any script)
        let first_digit_pos = num_str.char_indices()
            .find(|(_, c)| c.is_numeric())
            .map(|(i, _)| i);

        if let Some(fdp) = first_digit_pos {
            let sign_prefix = num_str[..fdp].to_string();
            let digits_part = &num_str[fdp..];

            // Find decimal separator in the digits-only part
            let dec_sep = locale_decimal_separator(locale_str);
            // Also handle Arabic decimal separator U+066B
            let decimal_pos = digits_part.find(dec_sep)
                .or_else(|| digits_part.find('\u{066b}'))
                .or_else(|| if dec_sep != "." { digits_part.find('.') } else { None });

            let int_part = match decimal_pos {
                Some(pos) => &digits_part[..pos],
                None => digits_part,
            };
            // Count only digit characters
            let digit_count = int_part.chars().filter(|c| c.is_numeric()).count();
            if digit_count < minimum_integer_digits as usize {
                let zeros_needed = minimum_integer_digits as usize - digit_count;
                // Insert zeros in the correct position (after sign prefix, before digits)
                num_str = format!("{}{}{}", sign_prefix, "0".repeat(zeros_needed), digits_part);
            }
        }
    }

    transliterate_digits(&wrap_style(&num_str, style, currency, currency_display, currency_sign, unit, unit_display, work_value, locale_str), numbering_system)
}

fn string_needs_decimal_precision(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed == "Infinity" || trimmed == "-Infinity"
        || trimmed == "+Infinity" || trimmed == "NaN"
    {
        return false;
    }
    if let Ok(f) = trimmed.parse::<f64>() {
        if f.is_nan() || f.is_infinite() {
            return false;
        }
        let roundtrip = format!("{}", f);
        if let (Ok(orig), Ok(rt)) = (Decimal::try_from_str(trimmed), Decimal::try_from_str(&roundtrip)) {
            let mut orig_trimmed = orig;
            orig_trimmed.absolute.trim_end();
            let mut rt_trimmed = rt;
            rt_trimmed.absolute.trim_end();
            orig_trimmed.absolute.to_string() != rt_trimmed.absolute.to_string()
        } else {
            false
        }
    } else {
        false
    }
}

fn format_number_from_string_decimal(
    value_str: &str,
    locale_str: &str,
    style: &str,
    currency: &Option<String>,
    currency_display: &Option<String>,
    currency_sign: &Option<String>,
    unit: &Option<String>,
    unit_display: &Option<String>,
    sign_display: &str,
    use_grouping: &str,
    minimum_integer_digits: u32,
    minimum_fraction_digits: u32,
    maximum_fraction_digits: u32,
    numbering_system: &str,
) -> String {
    let trimmed = value_str.trim();
    if trimmed == "Infinity" || trimmed == "+Infinity" {
        return format_number_internal(
            f64::INFINITY, locale_str, style, currency, currency_display,
            currency_sign, unit, unit_display, "standard",
            &None, sign_display, use_grouping, minimum_integer_digits,
            minimum_fraction_digits, maximum_fraction_digits,
            &None, &None, "halfExpand", 1, "auto", "auto", numbering_system,
        );
    }
    if trimmed == "-Infinity" {
        return format_number_internal(
            f64::NEG_INFINITY, locale_str, style, currency, currency_display,
            currency_sign, unit, unit_display, "standard",
            &None, sign_display, use_grouping, minimum_integer_digits,
            minimum_fraction_digits, maximum_fraction_digits,
            &None, &None, "halfExpand", 1, "auto", "auto", numbering_system,
        );
    }
    if trimmed == "NaN" {
        return format_number_internal(
            f64::NAN, locale_str, style, currency, currency_display,
            currency_sign, unit, unit_display, "standard",
            &None, sign_display, use_grouping, minimum_integer_digits,
            minimum_fraction_digits, maximum_fraction_digits,
            &None, &None, "halfExpand", 1, "auto", "auto", numbering_system,
        );
    }

    let dec_result = Decimal::try_from_str(trimmed);
    if dec_result.is_err() {
        if let Ok(num) = trimmed.parse::<f64>() {
            return format_number_internal(
                num, locale_str, style, currency, currency_display,
                currency_sign, unit, unit_display, "standard",
                &None, sign_display, use_grouping, minimum_integer_digits,
                minimum_fraction_digits, maximum_fraction_digits,
                &None, &None, "halfExpand", 1, "auto", "auto", numbering_system,
            );
        }
        return format_number_internal(
            f64::NAN, locale_str, style, currency, currency_display,
            currency_sign, unit, unit_display, "standard",
            &None, sign_display, use_grouping, minimum_integer_digits,
            minimum_fraction_digits, maximum_fraction_digits,
            &None, &None, "halfExpand", 1, "auto", "auto", numbering_system,
        );
    }

    let mut dec = dec_result.unwrap();

    let base = base_locale(locale_str);
    let locale: IcuLocale = base.parse().unwrap_or_else(|_| "en".parse().unwrap());
    let prefs = DecimalFormatterPreferences::from(&locale);
    let mut opts = DecimalFormatterOptions::default();
    opts.grouping_strategy = Some(grouping_strategy_from_str(use_grouping));
    let formatter = DecimalFormatter::try_new(prefs, opts)
        .unwrap_or_else(|_| DecimalFormatter::try_new(Default::default(), opts).unwrap());

    dec.round_with_mode(-(maximum_fraction_digits as i16), SignedRoundingMode::Unsigned(UnsignedRoundingMode::HalfExpand));
    dec.absolute.trim_end();

    if minimum_fraction_digits > 0 {
        dec.absolute.pad_end(-(minimum_fraction_digits as i16));
    }
    if minimum_integer_digits > 1 {
        dec.absolute.pad_start((minimum_integer_digits as i16) - 1);
    }

    let work_value = if dec.sign == fixed_decimal::Sign::Negative { -1.0 } else { 0.0 };
    dec.apply_sign_display(js_sign_display_to_fd(sign_display));

    let formatted = formatter.format(&dec);
    let num_str = formatted.to_string();

    transliterate_digits(&wrap_style(&num_str, style, currency, currency_display, currency_sign, unit, unit_display, work_value, locale_str), numbering_system)
}

fn format_with_significant_digits(
    value: f64,
    min_sd: u32,
    max_sd: u32,
    rounding_mode: &str,
) -> Decimal {
    let mut dec = match Decimal::try_from_f64(value, FloatPrecision::RoundTrip) {
        Ok(d) => d,
        Err(_) => match Decimal::try_from_str(&format!("{}", value)) {
            Ok(d) => d,
            Err(_) => Decimal::from(0),
        },
    };

    // Count significant digits
    let mag_start = dec.absolute.nonzero_magnitude_start();
    let mag_end = dec.absolute.nonzero_magnitude_end();
    let current_sig = if dec.absolute.is_zero() {
        1i16
    } else {
        (mag_start - mag_end + 1).max(1)
    };

    // Round to max significant digits
    if current_sig > max_sd as i16 {
        let round_pos = mag_start - max_sd as i16 + 1;
        let mode = js_rounding_mode_to_fd(rounding_mode);
        dec.round_with_mode(round_pos, mode);
    }

    // Pad to min significant digits
    let mag_start_after = dec.absolute.nonzero_magnitude_start();
    let sig_after = if dec.absolute.is_zero() {
        1i16
    } else {
        let me = dec.absolute.nonzero_magnitude_end();
        (mag_start_after - me + 1).max(1)
    };
    if sig_after < min_sd as i16 {
        let pad_to = mag_start_after - min_sd as i16 + 1;
        dec.absolute.pad_end(pad_to);
    }

    dec
}

fn locale_decimal_separator(locale: &str) -> &'static str {
    let lang = locale.split('-').next().unwrap_or(locale).split('_').next().unwrap_or(locale);
    match lang {
        "de" | "fr" | "es" | "pt" | "it" | "nl" | "da" | "fi" | "nb" | "nn" | "no" | "sv"
        | "pl" | "cs" | "sk" | "hu" | "ro" | "bg" | "hr" | "sl" | "sr" | "tr" | "el"
        | "uk" | "ru" | "be" | "et" | "lv" | "lt" | "vi" | "id" | "ca" | "gl" | "eu" => ",",
        _ => ".",
    }
}

fn format_scientific(
    value: f64,
    notation: &str,
    sign_display: &str,
    _min_int_digits: u32,
    min_frac_digits: u32,
    max_frac_digits: u32,
    min_sig_digits: &Option<u32>,
    max_sig_digits: &Option<u32>,
    style: &str,
    currency: &Option<String>,
    currency_display: &Option<String>,
    unit: &Option<String>,
    unit_display: &Option<String>,
    locale_str: &str,
) -> String {
    if value == 0.0 {
        let sign_prefix = match sign_display {
            "always" | "exceptZero" => "",
            "never" => "",
            "negative" => "",
            _ => {
                if value.is_sign_negative() {
                    "-"
                } else {
                    ""
                }
            }
        };
        let mantissa = if let Some(min_sd) = min_sig_digits {
            let total_digits = *min_sd as usize;
            if total_digits <= 1 {
                "0".to_string()
            } else {
                format!("0.{}", "0".repeat(total_digits - 1))
            }
        } else if min_frac_digits > 0 {
            format!("0.{}", "0".repeat(min_frac_digits as usize))
        } else {
            "0".to_string()
        };
        let dec_sep = locale_decimal_separator(locale_str);
        let localized_mantissa = mantissa.replace('.', dec_sep);
        let result = format!("{}{}E0", sign_prefix, localized_mantissa);
        return wrap_with_style(&result, style, currency, currency_display, unit, unit_display, locale_str);
    }

    let abs_val = value.abs();
    let exp = abs_val.log10().floor() as i32;

    let adjusted_exp = if notation == "engineering" {
        (exp as f64 / 3.0).floor() as i32 * 3
    } else {
        exp
    };

    let mantissa_val = abs_val / 10f64.powi(adjusted_exp);

    let mantissa_str = if let Some(min_sd) = min_sig_digits {
        let max_sd = max_sig_digits.unwrap_or(*min_sd);
        let eng_digits = if notation == "engineering" {
            (exp - adjusted_exp) as u32
        } else {
            0
        };
        let effective_max = max_sd.max(eng_digits + 1);
        let effective_min = (*min_sd).max(eng_digits + 1);
        format_mantissa_sig_digits(mantissa_val, effective_min, effective_max)
    } else {
        format_mantissa_frac_digits(mantissa_val, min_frac_digits, max_frac_digits)
    };

    let sign_prefix = if value < 0.0 {
        match sign_display {
            "never" => "",
            _ => "-",
        }
    } else {
        match sign_display {
            "always" => "+",
            "exceptZero" => "+",
            _ => "",
        }
    };

    let dec_sep = locale_decimal_separator(locale_str);
    let localized_mantissa = mantissa_str.replace('.', dec_sep);
    let result = format!("{}{}E{}", sign_prefix, localized_mantissa, adjusted_exp);
    wrap_with_style(&result, style, currency, currency_display, unit, unit_display, locale_str)
}

fn format_mantissa_sig_digits(value: f64, min_sd: u32, max_sd: u32) -> String {
    let s = format!("{:.prec$}", value, prec = (max_sd as usize).saturating_sub(1));
    // Trim trailing zeros but keep at least min_sd significant digits
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() == 1 {
        return s;
    }
    let integer_part = parts[0];
    let frac_part = parts[1];
    let int_sig = if integer_part == "0" {
        0
    } else {
        integer_part.len()
    };
    let min_frac = if min_sd as usize > int_sig {
        min_sd as usize - int_sig
    } else {
        0
    };
    let trimmed = frac_part.trim_end_matches('0');
    let frac_len = trimmed.len().max(min_frac);
    if frac_len == 0 {
        integer_part.to_string()
    } else {
        format!("{}.{}", integer_part, &frac_part[..frac_len])
    }
}

fn format_mantissa_frac_digits(value: f64, min_frac: u32, max_frac: u32) -> String {
    let s = format!("{:.prec$}", value, prec = max_frac as usize);
    if min_frac == max_frac {
        return s;
    }
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() == 1 {
        if min_frac > 0 {
            return format!("{}.{}", s, "0".repeat(min_frac as usize));
        }
        return s;
    }
    let frac = parts[1].trim_end_matches('0');
    let frac_len = frac.len().max(min_frac as usize);
    if frac_len == 0 {
        parts[0].to_string()
    } else {
        format!("{}.{}", parts[0], &parts[1][..frac_len])
    }
}

fn compact_suffix_and_divisor(abs_val: f64, locale_str: &str, compact_display: &str) -> (f64, String) {
    let lang = locale_str.split('-').next().unwrap_or(locale_str).split('_').next().unwrap_or(locale_str);

    // Indian English uses lakh/crore system
    if locale_str.contains("IN") && lang == "en" {
        if abs_val >= 1e9 {
            let s = if compact_display == "long" { " billion" } else { "B" };
            return (1e9, s.to_string());
        } else if abs_val >= 1e7 {
            let s = if compact_display == "long" { " crore" } else { "Cr" };
            return (1e7, s.to_string());
        } else if abs_val >= 1e5 {
            let s = if compact_display == "long" { " lakh" } else { "L" };
            return (1e5, s.to_string());
        } else if abs_val >= 1e3 {
            let s = if compact_display == "long" { " thousand" } else { "K" };
            return (1e3, s.to_string());
        } else {
            return (1.0, String::new());
        }
    }

    match lang {
        "ja" | "zh" => {
            if abs_val >= 1e8 {
                let suffix = if lang == "ja" { "\u{5104}" } else { "\u{5104}" }; // 億
                (1e8, suffix.to_string())
            } else if abs_val >= 1e4 {
                let suffix = if lang == "ja" { "\u{4E07}" } else { "\u{842C}" }; // 万 / 萬
                (1e4, suffix.to_string())
            } else {
                (1.0, String::new())
            }
        }
        "ko" => {
            if abs_val >= 1e8 {
                (1e8, "\u{C5B5}".to_string()) // 억
            } else if abs_val >= 1e4 {
                (1e4, "\u{B9CC}".to_string()) // 만
            } else if abs_val >= 1e3 {
                (1e3, "\u{CC9C}".to_string()) // 천
            } else {
                (1.0, String::new())
            }
        }
        "de" => {
            if abs_val >= 1e12 {
                let s = if compact_display == "long" { " Billionen" } else { "\u{00A0}Bio." };
                (1e12, s.to_string())
            } else if abs_val >= 1e9 {
                let s = if compact_display == "long" { " Milliarden" } else { "\u{00A0}Mrd." };
                (1e9, s.to_string())
            } else if abs_val >= 1e6 {
                let s = if compact_display == "long" { " Millionen" } else { "\u{00A0}Mio." };
                (1e6, s.to_string())
            } else if abs_val >= 1e3 && compact_display == "long" {
                (1e3, " Tausend".to_string())
            } else {
                (1.0, String::new())
            }
        }
        _ => {
            // en-US and other Latin locales
            if abs_val >= 1e15 {
                let s = if compact_display == "long" { " quadrillion" } else { "Q" };
                (1e15, s.to_string())
            } else if abs_val >= 1e12 {
                let s = if compact_display == "long" { " trillion" } else { "T" };
                (1e12, s.to_string())
            } else if abs_val >= 1e9 {
                let s = if compact_display == "long" { " billion" } else { "B" };
                (1e9, s.to_string())
            } else if abs_val >= 1e6 {
                let s = if compact_display == "long" { " million" } else { "M" };
                (1e6, s.to_string())
            } else if abs_val >= 1e3 {
                let s = if compact_display == "long" { " thousand" } else { "K" };
                (1e3, s.to_string())
            } else {
                (1.0, String::new())
            }
        }
    }
}

fn format_compact(
    value: f64,
    compact_display: &str,
    sign_display: &str,
    locale_str: &str,
    use_grouping: &str,
    min_int_digits: u32,
    min_frac_digits: u32,
    _max_frac_digits: u32,
    min_sig_digits: &Option<u32>,
    max_sig_digits: &Option<u32>,
    style: &str,
    currency: &Option<String>,
    currency_display: &Option<String>,
    unit: &Option<String>,
    unit_display: &Option<String>,
) -> String {
    let abs_val = value.abs();
    let (divisor, suffix) = compact_suffix_and_divisor(abs_val, locale_str, compact_display);

    let scaled = value / divisor;
    let abs_scaled = scaled.abs();

    let base = base_locale(locale_str);
    let locale: IcuLocale = base.parse().unwrap_or_else(|_| "en".parse().unwrap());
    let prefs = DecimalFormatterPreferences::from(&locale);
    let mut opts = DecimalFormatterOptions::default();
    let grp = if divisor > 1.0 { "false" } else { "min2" };
    opts.grouping_strategy = Some(grouping_strategy_from_str(grp));
    let formatter = DecimalFormatter::try_new(prefs, opts)
        .unwrap_or_else(|_| DecimalFormatter::try_new(Default::default(), opts).unwrap());

    let mut dec = if let Some(min_sd) = min_sig_digits {
        let max_sd = max_sig_digits.unwrap_or(*min_sd);
        format_with_significant_digits(scaled, *min_sd, max_sd, "halfExpand")
    } else if divisor > 1.0 {
        if abs_scaled >= 10.0 {
            let mut d = match Decimal::try_from_f64(scaled, FloatPrecision::RoundTrip) {
                Ok(d) => d,
                Err(_) => match Decimal::try_from_str(&format!("{}", scaled)) {
                    Ok(d) => d,
                    Err(_) => Decimal::from(scaled as i64),
                },
            };
            d.round_with_mode(0, SignedRoundingMode::Unsigned(UnsignedRoundingMode::HalfExpand));
            d.absolute.trim_end();
            d
        } else {
            format_with_significant_digits(scaled, 1, 2, "halfExpand")
        }
    } else {
        if abs_scaled >= 10.0 || abs_scaled == 0.0 {
            let mut d = match Decimal::try_from_f64(scaled, FloatPrecision::RoundTrip) {
                Ok(d) => d,
                Err(_) => match Decimal::try_from_str(&format!("{}", scaled)) {
                    Ok(d) => d,
                    Err(_) => Decimal::from(scaled as i64),
                },
            };
            d.round_with_mode(0, SignedRoundingMode::Unsigned(UnsignedRoundingMode::HalfExpand));
            d.absolute.trim_end();
            d
        } else if abs_scaled >= 1.0 {
            format_with_significant_digits(scaled, 1, 2, "halfExpand")
        } else {
            format_with_significant_digits(scaled, 1, 2, "halfExpand")
        }
    };

    if min_int_digits > 1 {
        dec.absolute.pad_start((min_int_digits as i16) - 1);
    }

    if min_frac_digits > 0 {
        dec.absolute.pad_end(-(min_frac_digits as i16));
    }

    dec.apply_sign_display(js_sign_display_to_fd(sign_display));

    let num_str = formatter.format(&dec).to_string();
    let result = format!("{}{}", num_str, suffix);

    wrap_with_style(&result, style, currency, currency_display, unit, unit_display, locale_str)
}

fn wrap_with_style(
    num_str: &str,
    style: &str,
    currency: &Option<String>,
    currency_display: &Option<String>,
    unit: &Option<String>,
    unit_display: &Option<String>,
    locale: &str,
) -> String {
    match style {
        "currency" => {
            let cur = currency.as_deref().unwrap_or("USD");
            let cur_disp = currency_display.as_deref().unwrap_or("symbol");
            let sym = currency_symbol(cur, cur_disp);
            if cur_disp == "name" {
                format!("{} {}", num_str, sym)
            } else {
                format!("{}{}", sym, num_str)
            }
        }
        "percent" => {
            if locale_percent_has_space(locale) {
                format!("{}\u{00A0}%", num_str)
            } else {
                format!("{}%", num_str)
            }
        }
        "unit" => {
            let u = unit.as_deref().unwrap_or("degree");
            let u_disp = unit_display.as_deref().unwrap_or("short");
            let sym = unit_symbol(u, u_disp);
            format!("{}{}", num_str, sym)
        }
        _ => num_str.to_string(),
    }
}

fn locale_range_separator(locale: &str, is_currency: bool) -> &'static str {
    let base = base_locale(locale);
    let lang = base.split('-').next().unwrap_or(&base);
    match lang {
        "pt" | "es" | "it" | "fr" | "ca" | "gl" | "ro" | "oc" => " - ",
        _ => {
            if is_currency { " \u{2013} " } else { "\u{2013}" }
        }
    }
}

fn format_range_string(
    fmt_start: &str,
    fmt_end: &str,
    locale: &str,
    style: &str,
    currency: &Option<String>,
    currency_display: &Option<String>,
    sign_display: &str,
) -> String {
    if fmt_start == fmt_end {
        return format!("~{}", fmt_start);
    }

    let is_currency = style == "currency";
    let range_sep = locale_range_separator(locale, is_currency);

    if is_currency {
        let cur = currency.as_deref().unwrap_or("USD");
        let cur_disp = currency_display.as_deref().unwrap_or("symbol");
        let sym = currency_symbol_locale(cur, cur_disp, locale);
        let base = base_locale(locale);
        let is_suffix_locale = currency_position_after(&base);

        if is_suffix_locale {
            // Currency-after-number locale (e.g., pt-PT: "3 €")
            // Always share the currency suffix
            if fmt_start.ends_with(&sym) && fmt_end.ends_with(&sym) {
                let start_num = fmt_start[..fmt_start.len() - sym.len()].trim_end();
                let end_num = fmt_end[..fmt_end.len() - sym.len()].trim_end();
                // Also strip shared sign prefix if signDisplay is explicit
                if sign_display == "always" || sign_display == "exceptZero" {
                    let signs = ["+", "-", "\u{2212}"];
                    for sign in &signs {
                        if start_num.starts_with(sign) && end_num.starts_with(sign) {
                            let s_rest = &start_num[sign.len()..];
                            let e_rest = &end_num[sign.len()..];
                            // Determine separator between number and currency
                            let cur_sep = if fmt_start.contains('\u{A0}') { "\u{A0}" } else { " " };
                            return format!("{}{}{}{}{}{}", sign, s_rest, range_sep, e_rest, cur_sep, sym);
                        }
                    }
                }
                let cur_sep = if fmt_start.contains('\u{A0}') { "\u{A0}" } else { " " };
                return format!("{}{}{}{}{}", start_num, range_sep, end_num, cur_sep, sym);
            }
        } else {
            // Currency-prefix locale (e.g., en-US: "$3")
            // Only share prefix when sign is explicitly displayed
            if sign_display == "always" || sign_display == "exceptZero" {
                let signs = ["+", "-", "\u{2212}"];
                for sign in &signs {
                    let prefix = format!("{}{}", sign, sym);
                    if fmt_start.starts_with(&prefix) && fmt_end.starts_with(&prefix) {
                        let start_num = &fmt_start[prefix.len()..];
                        let end_num = &fmt_end[prefix.len()..];
                        let compact_sep = range_sep.trim();
                        return format!("{}{}{}{}", prefix, start_num, compact_sep, end_num);
                    }
                }
            }
        }
    }

    format!("{}{}{}", fmt_start, range_sep, fmt_end)
}

pub(crate) fn format_to_parts_internal(
    value: f64,
    locale_str: &str,
    style: &str,
    currency: &Option<String>,
    currency_display: &Option<String>,
    _currency_sign: &Option<String>,
    unit: &Option<String>,
    unit_display: &Option<String>,
    notation: &str,
    compact_display: &Option<String>,
    sign_display: &str,
    use_grouping: &str,
    minimum_integer_digits: u32,
    minimum_fraction_digits: u32,
    maximum_fraction_digits: u32,
    minimum_significant_digits: &Option<u32>,
    maximum_significant_digits: &Option<u32>,
    rounding_mode: &str,
    rounding_increment: u32,
    rounding_priority: &str,
    trailing_zero_display: &str,
    numbering_system: &str,
) -> Vec<(String, String)> {
    let formatted = format_number_internal(
        value,
        locale_str,
        style,
        currency,
        currency_display,
        _currency_sign,
        unit,
        unit_display,
        notation,
        compact_display,
        sign_display,
        use_grouping,
        minimum_integer_digits,
        minimum_fraction_digits,
        maximum_fraction_digits,
        minimum_significant_digits,
        maximum_significant_digits,
        rounding_mode,
        rounding_increment,
        rounding_priority,
        trailing_zero_display,
        numbering_system,
    );

    if value.is_nan() {
        let nan_str = locale_nan_string(locale_str);
        let mut parts = Vec::new();
        if sign_display == "always" {
            parts.push(("plusSign".to_string(), "+".to_string()));
        }
        if style == "currency" {
            let cur = currency.as_deref().unwrap_or("USD");
            let cur_disp = currency_display.as_deref().unwrap_or("symbol");
            if cur_disp != "name" {
                let sym = currency_symbol(cur, cur_disp);
                parts.push(("currency".to_string(), sym));
            }
        }
        parts.push(("nan".to_string(), nan_str.to_string()));
        if style == "currency" {
            let cur_disp = currency_display.as_deref().unwrap_or("symbol");
            if cur_disp == "name" {
                let cur = currency.as_deref().unwrap_or("USD");
                parts.push(("literal".to_string(), " ".to_string()));
                parts.push(("currency".to_string(), currency_name(cur)));
            }
        }
        if style == "unit" {
            let u = unit.as_deref().unwrap_or("degree");
            let u_disp = unit_display.as_deref().unwrap_or("short");
            let sym = unit_symbol(u, u_disp);
            if !sym.is_empty() {
                if u_disp == "long" || u_disp == "short" {
                    parts.push(("literal".to_string(), " ".to_string()));
                }
                parts.push(("unit".to_string(), sym.trim().to_string()));
            }
        }
        if style == "percent" {
            if locale_percent_has_space(locale_str) {
                parts.push(("literal".to_string(), "\u{00A0}".to_string()));
            }
            parts.push(("percentSign".to_string(), "%".to_string()));
        }
        return parts;
    }
    if value.is_infinite() {
        let inf_str = locale_infinity_string(locale_str);
        let mut parts = Vec::new();
        match sign_display {
            "always" | "exceptZero" => {
                if value > 0.0 {
                    parts.push(("plusSign".to_string(), "+".to_string()));
                } else {
                    parts.push(("minusSign".to_string(), "-".to_string()));
                }
            }
            "never" => {}
            "negative" => {
                if value < 0.0 {
                    parts.push(("minusSign".to_string(), "-".to_string()));
                }
            }
            _ => { // "auto"
                if value < 0.0 {
                    parts.push(("minusSign".to_string(), "-".to_string()));
                }
            }
        }
        if style == "currency" {
            let cur = currency.as_deref().unwrap_or("USD");
            let cur_disp = currency_display.as_deref().unwrap_or("symbol");
            if cur_disp != "name" {
                let sym = currency_symbol(cur, cur_disp);
                parts.push(("currency".to_string(), sym));
            }
        }
        parts.push(("infinity".to_string(), inf_str.to_string()));
        if style == "currency" {
            let cur_disp = currency_display.as_deref().unwrap_or("symbol");
            if cur_disp == "name" {
                let cur = currency.as_deref().unwrap_or("USD");
                parts.push(("literal".to_string(), " ".to_string()));
                parts.push(("currency".to_string(), currency_name(cur)));
            }
        }
        if style == "unit" {
            let u = unit.as_deref().unwrap_or("degree");
            let u_disp = unit_display.as_deref().unwrap_or("short");
            let sym = unit_symbol(u, u_disp);
            if !sym.is_empty() {
                if u_disp == "long" || u_disp == "short" {
                    parts.push(("literal".to_string(), " ".to_string()));
                }
                parts.push(("unit".to_string(), sym.trim().to_string()));
            }
        }
        if style == "percent" {
            if locale_percent_has_space(locale_str) {
                parts.push(("literal".to_string(), "\u{00A0}".to_string()));
            }
            parts.push(("percentSign".to_string(), "%".to_string()));
        }
        return parts;
    }

    // Simple decomposition of the formatted string
    let mut parts = Vec::new();
    let mut past_decimal = false;
    let mut currency_suffix: Option<(String, String)> = None;

    let cur_after = style == "currency" && currency_position_after(locale_str);
    let mut work_str = formatted.clone();

    // For unit style with circumfix patterns, strip prefix and suffix
    let mut unit_prefix_parts: Vec<(String, String)> = Vec::new();
    let mut unit_suffix_parts: Vec<(String, String)> = Vec::new();
    if style == "unit" {
        let u = unit.as_deref().unwrap_or("degree");
        let u_disp = unit_display.as_deref().unwrap_or("short");
        let (prefix, suffix) = locale_unit_pattern(u, u_disp, locale_str, value);
        if !prefix.is_empty() {
            // Circumfix pattern: strip prefix from formatted string
            if work_str.starts_with(&prefix) {
                work_str = work_str[prefix.len()..].to_string();
                // Parse prefix into unit parts: "時速 " -> [unit:"時速", literal:" "]
                let prefix_trimmed = prefix.trim_end();
                let trailing_space = &prefix[prefix_trimmed.len()..];
                if !prefix_trimmed.is_empty() {
                    unit_prefix_parts.push(("unit".to_string(), prefix_trimmed.to_string()));
                }
                if !trailing_space.is_empty() {
                    unit_prefix_parts.push(("literal".to_string(), trailing_space.to_string()));
                }
            }
            // Strip suffix from formatted string
            let suffix_trimmed = suffix.trim_start();
            let leading_space = &suffix[..suffix.len() - suffix_trimmed.len()];
            if !suffix_trimmed.is_empty() && work_str.ends_with(suffix_trimmed) {
                work_str = work_str[..work_str.len() - suffix_trimmed.len()].to_string();
                // Also strip leading space if present
                if !leading_space.is_empty() && work_str.ends_with(leading_space) {
                    work_str = work_str[..work_str.len() - leading_space.len()].to_string();
                    unit_suffix_parts.push(("literal".to_string(), leading_space.to_string()));
                }
                unit_suffix_parts.push(("unit".to_string(), suffix_trimmed.to_string()));
            }
        } else if !suffix.is_empty() {
            // Suffix-only pattern: strip suffix for clean number parsing
            let suffix_trimmed = suffix.trim_start();
            let leading_space = &suffix[..suffix.len() - suffix_trimmed.len()];
            if !suffix_trimmed.is_empty() && work_str.ends_with(suffix_trimmed) {
                work_str = work_str[..work_str.len() - suffix_trimmed.len()].to_string();
                if !leading_space.is_empty() && work_str.ends_with(leading_space) {
                    work_str = work_str[..work_str.len() - leading_space.len()].to_string();
                    unit_suffix_parts.push(("literal".to_string(), leading_space.to_string()));
                }
                unit_suffix_parts.push(("unit".to_string(), suffix_trimmed.to_string()));
            }
        }
    }

    // For currency-after locales, strip the trailing currency and literal
    if cur_after && style == "currency" {
        let cur = currency.as_deref().unwrap_or("USD");
        let cur_disp = currency_display.as_deref().unwrap_or("symbol");
        if cur_disp != "name" {
            let sym = currency_symbol_locale(cur, cur_disp, locale_str);
            if work_str.ends_with(&sym) {
                let without_sym = &work_str[..work_str.len() - sym.len()];
                if without_sym.ends_with('\u{00A0}') {
                    work_str = without_sym[..without_sym.len() - '\u{00A0}'.len_utf8()].to_string();
                    currency_suffix = Some(("\u{00A0}".to_string(), sym));
                } else if without_sym.ends_with(' ') {
                    work_str = without_sym[..without_sym.len() - 1].to_string();
                    currency_suffix = Some((" ".to_string(), sym));
                } else {
                    work_str = without_sym.to_string();
                    currency_suffix = Some(("".to_string(), sym));
                }
            }
        }
    }

    // For compact notation, strip the trailing compact suffix
    let mut compact_suffix: Option<(String, String)> = None; // (literal, compact_value)
    if notation == "compact" {
        let abs_val = value.abs();
        let cd = compact_display.as_deref().unwrap_or("short");
        let (divisor, raw_suffix) = compact_suffix_and_divisor(abs_val, locale_str, cd);
        if divisor > 1.0 && !raw_suffix.is_empty() {
            // raw_suffix may start with a space/NBSP (literal separator)
            let suffix_trimmed = raw_suffix.trim_start();
            let literal_part = &raw_suffix[..raw_suffix.len() - suffix_trimmed.len()];
            if work_str.ends_with(suffix_trimmed) {
                work_str = work_str[..work_str.len() - suffix_trimmed.len()].to_string();
                // Also strip the literal separator if present
                if !literal_part.is_empty() && work_str.ends_with(literal_part) {
                    work_str = work_str[..work_str.len() - literal_part.len()].to_string();
                    compact_suffix = Some((literal_part.to_string(), suffix_trimmed.to_string()));
                } else {
                    compact_suffix = Some((String::new(), suffix_trimmed.to_string()));
                }
            }
        }
    }

    // Insert unit prefix parts (for circumfix patterns like ja-JP long)
    for p in unit_prefix_parts {
        parts.push(p);
    }

    let mut chars = work_str.chars().peekable();
    let mut current = String::new();

    // Handle currency prefix (with possible sign before it)
    if style == "currency" && !cur_after {
        let cur = currency.as_deref().unwrap_or("USD");
        let cur_disp = currency_display.as_deref().unwrap_or("symbol");
        let cur_sign = _currency_sign.as_deref().unwrap_or("standard");
        if cur_disp != "name" {
            let sym = currency_symbol_locale(cur, cur_disp, locale_str);
            if cur_sign == "accounting" && work_str.starts_with('(') {
                chars.next();
                parts.push(("literal".to_string(), "(".to_string()));
                let rest: String = chars.clone().collect();
                if rest.starts_with(&sym) {
                    parts.push(("currency".to_string(), sym.clone()));
                    for _ in 0..sym.chars().count() { chars.next(); }
                }
            } else if work_str.starts_with(&sym) {
                parts.push(("currency".to_string(), sym.clone()));
                for _ in 0..sym.chars().count() { chars.next(); }
            } else {
                let first = work_str.chars().next();
                let is_bidi_prefix = first == Some('\u{061C}') || first == Some('\u{200E}') || first == Some('\u{200F}');
                let sign_start = if is_bidi_prefix { work_str.chars().nth(1) } else { first };
                if sign_start == Some('-') || sign_start == Some('+') || sign_start == Some('\u{2212}') {
                    let mut sign_str = String::new();
                    if is_bidi_prefix {
                        sign_str.push(chars.next().unwrap());
                    }
                    let sign_char = chars.next().unwrap();
                    sign_str.push(sign_char);
                    // Consume trailing bidi mark
                    if let Some(&trail) = chars.peek() {
                        if trail == '\u{061C}' || trail == '\u{200E}' || trail == '\u{200F}' {
                            sign_str.push(chars.next().unwrap());
                        }
                    }
                    if sign_char == '-' || sign_char == '\u{2212}' {
                        parts.push(("minusSign".to_string(), sign_str));
                    } else {
                        parts.push(("plusSign".to_string(), sign_str));
                    }
                    let rest: String = chars.clone().collect();
                    if rest.starts_with(&sym) {
                        parts.push(("currency".to_string(), sym.clone()));
                        for _ in 0..sym.chars().count() { chars.next(); }
                    }
                }
            }
        }
    }

    // Parse number portion
    while let Some(&c) = chars.peek() {
        if c == '\u{061C}' || c == '\u{200E}' || c == '\u{200F}' {
            // Bidi marks: combine with following sign character
            let bidi = chars.next().unwrap();
            if let Some(&next_c) = chars.peek() {
                if next_c == '-' || next_c == '+' || next_c == '\u{2212}' {
                    if !current.is_empty() {
                        parts.push((if past_decimal { "fraction" } else { "integer" }.to_string(), current.clone()));
                        current.clear();
                    }
                    let sign_char = chars.next().unwrap();
                    let mut sign_str = String::new();
                    sign_str.push(bidi);
                    sign_str.push(sign_char);
                    // Consume trailing bidi mark if present
                    if let Some(&trail) = chars.peek() {
                        if trail == '\u{061C}' || trail == '\u{200E}' || trail == '\u{200F}' {
                            sign_str.push(chars.next().unwrap());
                        }
                    }
                    if sign_char == '-' || sign_char == '\u{2212}' {
                        parts.push(("minusSign".to_string(), sign_str));
                    } else {
                        parts.push(("plusSign".to_string(), sign_str));
                    }
                } else {
                    current.push(bidi);
                }
            }
        } else if c == '-' || c == '+' || c == '\u{2212}' {
            if !current.is_empty() {
                parts.push((if past_decimal { "fraction" } else { "integer" }.to_string(), current.clone()));
                current.clear();
            }
            let sign_char = chars.next().unwrap();
            if sign_char == '-' || sign_char == '\u{2212}' {
                parts.push(("minusSign".to_string(), sign_char.to_string()));
            } else {
                parts.push(("plusSign".to_string(), sign_char.to_string()));
            }
        } else if c == ',' || c == '.' || c == '\u{066B}' || c == '\u{066C}' {
            if !current.is_empty() {
                let kind = if past_decimal { "fraction" } else { "integer" };
                parts.push((kind.to_string(), current.clone()));
                current.clear();
            }
            let sep = chars.next().unwrap();
            // Determine if this is a decimal separator or group separator
            // Use locale knowledge: de-DE uses comma as decimal, period as group
            let base_loc = base_locale(locale_str);
            let is_decimal = if base_loc == "de" || base_loc.starts_with("de-") || base_loc == "pt" || base_loc.starts_with("pt-") {
                sep == ','
            } else {
                sep == '.' || sep == '\u{066B}'
            };
            if is_decimal {
                parts.push(("decimal".to_string(), sep.to_string()));
                past_decimal = true;
            } else {
                parts.push(("group".to_string(), sep.to_string()));
            }
        } else if c == '%' {
            if !current.is_empty() {
                parts.push((if past_decimal { "fraction" } else { "integer" }.to_string(), current.clone()));
                current.clear();
            }
            chars.next();
            if style == "unit" {
                parts.push(("unit".to_string(), "%".to_string()));
            } else {
                parts.push(("percentSign".to_string(), "%".to_string()));
            }
        } else if c == ')' {
            if !current.is_empty() {
                parts.push((if past_decimal { "fraction" } else { "integer" }.to_string(), current.clone()));
                current.clear();
            }
            chars.next();
            parts.push(("literal".to_string(), ")".to_string()));
        } else if c == 'E' && (notation == "scientific" || notation == "engineering") {
            if !current.is_empty() {
                parts.push((if past_decimal { "fraction" } else { "integer" }.to_string(), current.clone()));
                current.clear();
            }
            chars.next();
            parts.push(("exponentSeparator".to_string(), "E".to_string()));
            // Parse exponent integer
            let mut exp_str = String::new();
            while let Some(&ec) = chars.peek() {
                if ec.is_ascii_digit() || ec == '-' || ec == '+' {
                    exp_str.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            if !exp_str.is_empty() {
                // Check for sign in exponent
                if exp_str.starts_with('-') {
                    parts.push(("exponentMinusSign".to_string(), "-".to_string()));
                    parts.push(("exponentInteger".to_string(), exp_str[1..].to_string()));
                } else if exp_str.starts_with('+') {
                    parts.push(("exponentInteger".to_string(), exp_str[1..].to_string()));
                } else {
                    parts.push(("exponentInteger".to_string(), exp_str));
                }
            }
        } else if c.is_ascii_digit()
            || c.is_numeric()
            || (c >= '\u{0660}' && c <= '\u{0669}')
            || (c >= '\u{06F0}' && c <= '\u{06F9}')
        {
            current.push(chars.next().unwrap());
        } else {
            // Non-numeric, non-separator char => likely unit/currency suffix or literal
            if !current.is_empty() {
                parts.push((if past_decimal { "fraction" } else { "integer" }.to_string(), current.clone()));
                current.clear();
            }
            // Collect remaining as literal or unit
            let mut rest = String::new();
            while let Some(&rc) = chars.peek() {
                rest.push(chars.next().unwrap());
                let _ = rc;
            }
            if !rest.is_empty() {
                if style == "unit" {
                    let trimmed = rest.trim_start();
                    if trimmed.len() < rest.len() {
                        let space = &rest[..rest.len() - trimmed.len()];
                        parts.push(("literal".to_string(), space.to_string()));
                    }
                    let unit_val = trimmed.trim_end();
                    if !unit_val.is_empty() {
                        parts.push(("unit".to_string(), unit_val.to_string()));
                    }
                } else if style == "currency" {
                    parts.push(("currency".to_string(), rest.trim().to_string()));
                } else {
                    parts.push(("literal".to_string(), rest));
                }
            }
            break;
        }
    }

    if !current.is_empty() {
        parts.push((if past_decimal { "fraction" } else { "integer" }.to_string(), current.clone()));
    }

    // Append compact suffix
    if let Some((lit, compact_val)) = compact_suffix {
        if !lit.is_empty() {
            parts.push(("literal".to_string(), lit));
        }
        parts.push(("compact".to_string(), compact_val));
    }

    // Append unit suffix parts (for pre-stripped unit patterns)
    for p in unit_suffix_parts {
        parts.push(p);
    }

    // Append currency suffix for currency-after-number locales
    if let Some((lit, sym)) = currency_suffix {
        if !lit.is_empty() {
            parts.push(("literal".to_string(), lit));
        }
        parts.push(("currency".to_string(), sym));
    }

    // Handle currency name suffix
    if style == "currency" {
        let cur = currency.as_deref().unwrap_or("USD");
        let cur_disp = currency_display.as_deref().unwrap_or("symbol");
        if cur_disp == "name" {
            let name = currency_name(cur);
            let _ = name;
        }
    }

    parts
}

fn classify_number_chunk(s: &str) -> (String, String) {
    if s.contains('.') {
        ("fraction".to_string(), s.to_string())
    } else {
        ("integer".to_string(), s.to_string())
    }
}

impl Interpreter {
    pub(crate) fn setup_intl_number_format(&mut self, intl_obj: &Rc<RefCell<JsObjectData>>) {
        let proto = self.create_object();
        if let Some(ref op) = self.realm().object_prototype {
            proto.borrow_mut().prototype = Some(op.clone());
        }
        proto.borrow_mut().class_name = "Intl.NumberFormat".to_string();

        // @@toStringTag
        proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("Intl.NumberFormat"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            },
        );

        // format getter
        let format_getter = self.create_function(JsFunction::native(
            "get format".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this {
                    if let Some(obj) = interp.get_object(o.id) {
                        let cached = {
                            let b = obj.borrow();
                            if !matches!(b.intl_data, Some(IntlData::NumberFormat { .. })) {
                                return Completion::Throw(interp.create_type_error(
                                    "Intl.NumberFormat.prototype.format called on incompatible receiver",
                                ));
                            }
                            b.properties
                                .get("[[BoundFormat]]")
                                .and_then(|pd| pd.value.clone())
                        };

                        if let Some(func) = cached {
                            return Completion::Normal(func);
                        }
                    }

                    let nf_data = {
                        if let Some(obj) = interp.get_object(o.id) {
                            let b = obj.borrow();
                            b.intl_data.clone()
                        } else {
                            None
                        }
                    };

                    if let Some(IntlData::NumberFormat {
                        locale,
                        numbering_system,
                        style,
                        currency,
                        currency_display,
                        currency_sign,
                        unit,
                        unit_display,
                        notation,
                        compact_display,
                        sign_display,
                        use_grouping,
                        minimum_integer_digits,
                        minimum_fraction_digits,
                        maximum_fraction_digits,
                        minimum_significant_digits,
                        maximum_significant_digits,
                        rounding_mode,
                        rounding_increment,
                        rounding_priority,
                        trailing_zero_display,
                    }) = nf_data
                    {
                        let format_fn = interp.create_function(JsFunction::native(
                            "".to_string(),
                            1,
                            move |interp2, _this2, args2| {
                                let val = args2.first().cloned().unwrap_or(JsValue::Undefined);

                                let use_string_decimal = if let JsValue::BigInt(bi) = &val {
                                    let abs_str = bi.value.to_string().trim_start_matches('-').to_string();
                                    abs_str.len() > 15
                                } else if let JsValue::String(s) = &val {
                                    string_needs_decimal_precision(&s.to_string())
                                } else {
                                    false
                                };

                                let result = if use_string_decimal {
                                    let s = match &val {
                                        JsValue::BigInt(bi) => bi.value.to_string(),
                                        JsValue::String(s) => s.to_string(),
                                        _ => unreachable!(),
                                    };
                                    format_number_from_string_decimal(
                                        &s, &locale, &style, &currency, &currency_display,
                                        &currency_sign, &unit, &unit_display, &sign_display,
                                        &use_grouping, minimum_integer_digits, minimum_fraction_digits,
                                        maximum_fraction_digits, &numbering_system,
                                    )
                                } else {
                                    let num = match intl_to_number(interp2, &val) {
                                        Ok(n) => n,
                                        Err(e) => return Completion::Throw(e),
                                    };
                                    format_number_internal(
                                        num, &locale, &style, &currency, &currency_display,
                                        &currency_sign, &unit, &unit_display, &notation,
                                        &compact_display, &sign_display, &use_grouping,
                                        minimum_integer_digits, minimum_fraction_digits,
                                        maximum_fraction_digits, &minimum_significant_digits,
                                        &maximum_significant_digits, &rounding_mode, rounding_increment,
                                        &rounding_priority, &trailing_zero_display, &numbering_system,
                                    )
                                };

                                Completion::Normal(JsValue::String(JsString::from_str(&result)))
                            },
                        ));

                        if let Some(obj) = interp.get_object(o.id) {
                            obj.borrow_mut().properties.insert(
                                "[[BoundFormat]]".to_string(),
                                PropertyDescriptor::data(format_fn.clone(), false, false, false),
                            );
                        }

                        return Completion::Normal(format_fn);
                    }
                }
                Completion::Throw(interp.create_type_error(
                    "Intl.NumberFormat.prototype.format called on incompatible receiver",
                ))
            },
        ));
        proto.borrow_mut().insert_property(
            "format".to_string(),
            PropertyDescriptor::accessor(Some(format_getter), None, false, true),
        );

        // formatToParts(number)
        let format_to_parts_fn = self.create_function(JsFunction::native(
            "formatToParts".to_string(),
            1,
            |interp, this, args| {
                if let JsValue::Object(o) = this {
                    let nf_data = {
                        if let Some(obj) = interp.get_object(o.id) {
                            let b = obj.borrow();
                            b.intl_data.clone()
                        } else {
                            None
                        }
                    };

                    if let Some(IntlData::NumberFormat {
                        locale,
                        numbering_system,
                        style,
                        currency,
                        currency_display,
                        currency_sign,
                        unit,
                        unit_display,
                        notation,
                        compact_display,
                        sign_display,
                        use_grouping,
                        minimum_integer_digits,
                        minimum_fraction_digits,
                        maximum_fraction_digits,
                        minimum_significant_digits,
                        maximum_significant_digits,
                        rounding_mode,
                        rounding_increment,
                        rounding_priority,
                        trailing_zero_display,
                    }) = nf_data
                    {
                        let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let num = match intl_to_number(interp, &val) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        };

                        let parts = format_to_parts_internal(
                            num,
                            &locale,
                            &style,
                            &currency,
                            &currency_display,
                            &currency_sign,
                            &unit,
                            &unit_display,
                            &notation,
                            &compact_display,
                            &sign_display,
                            &use_grouping,
                            minimum_integer_digits,
                            minimum_fraction_digits,
                            maximum_fraction_digits,
                            &minimum_significant_digits,
                            &maximum_significant_digits,
                            &rounding_mode,
                            rounding_increment,
                            &rounding_priority,
                            &trailing_zero_display,
                            &numbering_system,
                        );

                        let result_parts: Vec<JsValue> = parts
                            .into_iter()
                            .map(|(typ, val)| {
                                let part_obj = interp.create_object();
                                if let Some(ref op) = interp.realm().object_prototype {
                                    part_obj.borrow_mut().prototype = Some(op.clone());
                                }
                                part_obj.borrow_mut().insert_property(
                                    "type".to_string(),
                                    PropertyDescriptor::data(
                                        JsValue::String(JsString::from_str(&typ)),
                                        true,
                                        true,
                                        true,
                                    ),
                                );
                                part_obj.borrow_mut().insert_property(
                                    "value".to_string(),
                                    PropertyDescriptor::data(
                                        JsValue::String(JsString::from_str(&val)),
                                        true,
                                        true,
                                        true,
                                    ),
                                );
                                let id = part_obj.borrow().id.unwrap();
                                JsValue::Object(crate::types::JsObject { id })
                            })
                            .collect();

                        return Completion::Normal(interp.create_array(result_parts));
                    }
                }
                Completion::Throw(interp.create_type_error(
                    "Intl.NumberFormat.prototype.formatToParts called on incompatible receiver",
                ))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("formatToParts".to_string(), format_to_parts_fn);

        // formatRange(start, end)
        let format_range_fn = self.create_function(JsFunction::native(
            "formatRange".to_string(),
            2,
            |interp, this, args| {
                if let JsValue::Object(o) = this {
                    if let Some(obj) = interp.get_object(o.id) {
                        let has_nf = {
                            let b = obj.borrow();
                            matches!(b.intl_data, Some(IntlData::NumberFormat { .. }))
                        };
                        if !has_nf {
                            return Completion::Throw(interp.create_type_error(
                                "Intl.NumberFormat.prototype.formatRange called on incompatible receiver",
                            ));
                        }

                        let start = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let end = args.get(1).cloned().unwrap_or(JsValue::Undefined);

                        if matches!(start, JsValue::Undefined) || matches!(end, JsValue::Undefined) {
                            return Completion::Throw(interp.create_type_error(
                                "start and end must not be undefined",
                            ));
                        }

                        let start_str = if let JsValue::String(s) = &start {
                            let sv = s.to_string();
                            if string_needs_decimal_precision(&sv) { Some(sv) } else { None }
                        } else {
                            None
                        };
                        let end_str = if let JsValue::String(s) = &end {
                            let sv = s.to_string();
                            if string_needs_decimal_precision(&sv) { Some(sv) } else { None }
                        } else {
                            None
                        };

                        let x = match intl_to_number(interp, &start) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        };
                        let y = match intl_to_number(interp, &end) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        };

                        if x.is_nan() || y.is_nan() {
                            return Completion::Throw(
                                interp.create_range_error("Invalid number for range formatting"),
                            );
                        }

                        let nf_data = {
                            let b = obj.borrow();
                            b.intl_data.clone()
                        };

                        if let Some(IntlData::NumberFormat {
                            locale,
                            numbering_system,
                            style,
                            currency,
                            currency_display,
                            currency_sign,
                            unit,
                            unit_display,
                            notation,
                            compact_display,
                            sign_display,
                            use_grouping,
                            minimum_integer_digits,
                            minimum_fraction_digits,
                            maximum_fraction_digits,
                            minimum_significant_digits,
                            maximum_significant_digits,
                            rounding_mode,
                            rounding_increment,
                            rounding_priority,
                            trailing_zero_display,
                        }) = nf_data
                        {
                            let fmt_start = if let Some(ref s) = start_str {
                                format_number_from_string_decimal(
                                    s, &locale, &style, &currency, &currency_display,
                                    &currency_sign, &unit, &unit_display, &sign_display,
                                    &use_grouping, minimum_integer_digits, minimum_fraction_digits,
                                    maximum_fraction_digits, &numbering_system,
                                )
                            } else {
                                format_number_internal(
                                    x, &locale, &style, &currency, &currency_display,
                                    &currency_sign, &unit, &unit_display, &notation,
                                    &compact_display, &sign_display, &use_grouping,
                                    minimum_integer_digits, minimum_fraction_digits,
                                    maximum_fraction_digits, &minimum_significant_digits,
                                    &maximum_significant_digits, &rounding_mode, rounding_increment,
                                    &rounding_priority, &trailing_zero_display, &numbering_system,
                                )
                            };
                            let fmt_end = if let Some(ref s) = end_str {
                                format_number_from_string_decimal(
                                    s, &locale, &style, &currency, &currency_display,
                                    &currency_sign, &unit, &unit_display, &sign_display,
                                    &use_grouping, minimum_integer_digits, minimum_fraction_digits,
                                    maximum_fraction_digits, &numbering_system,
                                )
                            } else {
                                format_number_internal(
                                    y, &locale, &style, &currency, &currency_display,
                                    &currency_sign, &unit, &unit_display, &notation,
                                    &compact_display, &sign_display, &use_grouping,
                                    minimum_integer_digits, minimum_fraction_digits,
                                    maximum_fraction_digits, &minimum_significant_digits,
                                    &maximum_significant_digits, &rounding_mode, rounding_increment,
                                    &rounding_priority, &trailing_zero_display, &numbering_system,
                                )
                            };

                            let result = format_range_string(
                                &fmt_start, &fmt_end, &locale, &style,
                                &currency, &currency_display, &sign_display,
                            );
                            return Completion::Normal(JsValue::String(JsString::from_str(
                                &result,
                            )));
                        }
                    }
                }
                Completion::Throw(interp.create_type_error(
                    "Intl.NumberFormat.prototype.formatRange called on incompatible receiver",
                ))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("formatRange".to_string(), format_range_fn);

        // formatRangeToParts(start, end)
        let format_range_to_parts_fn = self.create_function(JsFunction::native(
            "formatRangeToParts".to_string(),
            2,
            |interp, this, args| {
                if let JsValue::Object(o) = this {
                    if let Some(obj) = interp.get_object(o.id) {
                        let has_nf = {
                            let b = obj.borrow();
                            matches!(b.intl_data, Some(IntlData::NumberFormat { .. }))
                        };
                        if !has_nf {
                            return Completion::Throw(interp.create_type_error(
                                "Intl.NumberFormat.prototype.formatRangeToParts called on incompatible receiver",
                            ));
                        }

                        let start = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let end = args.get(1).cloned().unwrap_or(JsValue::Undefined);

                        if matches!(start, JsValue::Undefined) || matches!(end, JsValue::Undefined) {
                            return Completion::Throw(interp.create_type_error(
                                "start and end must not be undefined",
                            ));
                        }

                        let x = match intl_to_number(interp, &start) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        };
                        let y = match intl_to_number(interp, &end) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        };

                        if x.is_nan() || y.is_nan() {
                            return Completion::Throw(
                                interp.create_range_error("Invalid number for range formatting"),
                            );
                        }

                        let nf_data = {
                            let b = obj.borrow();
                            b.intl_data.clone()
                        };

                        if let Some(IntlData::NumberFormat {
                            locale,
                            numbering_system,
                            style,
                            currency,
                            currency_display,
                            currency_sign,
                            unit,
                            unit_display,
                            notation,
                            compact_display,
                            sign_display,
                            use_grouping,
                            minimum_integer_digits,
                            minimum_fraction_digits,
                            maximum_fraction_digits,
                            minimum_significant_digits,
                            maximum_significant_digits,
                            rounding_mode,
                            rounding_increment,
                            rounding_priority,
                            trailing_zero_display,
                        }) = nf_data
                        {
                            let fmt_start = format_number_internal(
                                x, &locale, &style, &currency, &currency_display,
                                &currency_sign, &unit, &unit_display, &notation,
                                &compact_display, &sign_display, &use_grouping,
                                minimum_integer_digits, minimum_fraction_digits,
                                maximum_fraction_digits, &minimum_significant_digits,
                                &maximum_significant_digits, &rounding_mode, rounding_increment,
                                &rounding_priority, &trailing_zero_display, &numbering_system,
                            );
                            let fmt_end = format_number_internal(
                                y, &locale, &style, &currency, &currency_display,
                                &currency_sign, &unit, &unit_display, &notation,
                                &compact_display, &sign_display, &use_grouping,
                                minimum_integer_digits, minimum_fraction_digits,
                                maximum_fraction_digits, &minimum_significant_digits,
                                &maximum_significant_digits, &rounding_mode, rounding_increment,
                                &rounding_priority, &trailing_zero_display, &numbering_system,
                            );

                            let approximately_equal = fmt_start == fmt_end;

                            let make_part = |interp: &mut Interpreter, typ: &str, val: &str, source: &str| -> JsValue {
                                let part = interp.create_object();
                                if let Some(ref op) = interp.realm().object_prototype {
                                    part.borrow_mut().prototype = Some(op.clone());
                                }
                                part.borrow_mut().insert_property(
                                    "type".to_string(),
                                    PropertyDescriptor::data(JsValue::String(JsString::from_str(typ)), true, true, true),
                                );
                                part.borrow_mut().insert_property(
                                    "value".to_string(),
                                    PropertyDescriptor::data(JsValue::String(JsString::from_str(val)), true, true, true),
                                );
                                part.borrow_mut().insert_property(
                                    "source".to_string(),
                                    PropertyDescriptor::data(JsValue::String(JsString::from_str(source)), true, true, true),
                                );
                                let id = part.borrow().id.unwrap();
                                JsValue::Object(crate::types::JsObject { id })
                            };

                            let mut result_parts = Vec::new();

                            if approximately_equal {
                                result_parts.push(make_part(interp, "approximatelySign", "~", "shared"));
                                let parts = format_to_parts_internal(
                                    x, &locale, &style, &currency, &currency_display,
                                    &currency_sign, &unit, &unit_display, &notation,
                                    &compact_display, &sign_display, &use_grouping,
                                    minimum_integer_digits, minimum_fraction_digits,
                                    maximum_fraction_digits, &minimum_significant_digits,
                                    &maximum_significant_digits, &rounding_mode, rounding_increment,
                                    &rounding_priority, &trailing_zero_display, &numbering_system,
                                );
                                for (typ, val) in &parts {
                                    result_parts.push(make_part(interp, typ, val, "shared"));
                                }
                            } else {
                                let parts_start = format_to_parts_internal(
                                    x, &locale, &style, &currency, &currency_display,
                                    &currency_sign, &unit, &unit_display, &notation,
                                    &compact_display, &sign_display, &use_grouping,
                                    minimum_integer_digits, minimum_fraction_digits,
                                    maximum_fraction_digits, &minimum_significant_digits,
                                    &maximum_significant_digits, &rounding_mode, rounding_increment,
                                    &rounding_priority, &trailing_zero_display, &numbering_system,
                                );
                                for (typ, val) in &parts_start {
                                    result_parts.push(make_part(interp, typ, val, "startRange"));
                                }

                                let range_sep = locale_range_separator(&locale, style == "currency");
                                result_parts.push(make_part(interp, "literal", range_sep, "shared"));

                                let parts_end = format_to_parts_internal(
                                    y, &locale, &style, &currency, &currency_display,
                                    &currency_sign, &unit, &unit_display, &notation,
                                    &compact_display, &sign_display, &use_grouping,
                                    minimum_integer_digits, minimum_fraction_digits,
                                    maximum_fraction_digits, &minimum_significant_digits,
                                    &maximum_significant_digits, &rounding_mode, rounding_increment,
                                    &rounding_priority, &trailing_zero_display, &numbering_system,
                                );
                                for (typ, val) in &parts_end {
                                    result_parts.push(make_part(interp, typ, val, "endRange"));
                                }
                            }

                            return Completion::Normal(interp.create_array(result_parts));
                        }
                    }
                }
                Completion::Throw(interp.create_type_error(
                    "Intl.NumberFormat.prototype.formatRangeToParts called on incompatible receiver",
                ))
            },
        ));
        proto.borrow_mut().insert_builtin(
            "formatRangeToParts".to_string(),
            format_range_to_parts_fn,
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
                        if let Some(IntlData::NumberFormat {
                            locale,
                            numbering_system,
                            style,
                            currency,
                            currency_display,
                            currency_sign,
                            unit,
                            unit_display,
                            notation,
                            compact_display,
                            sign_display,
                            use_grouping,
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

                            let mut props: Vec<(&str, JsValue)> = vec![
                                (
                                    "locale",
                                    JsValue::String(JsString::from_str(&locale)),
                                ),
                                (
                                    "numberingSystem",
                                    JsValue::String(JsString::from_str(&numbering_system)),
                                ),
                                (
                                    "style",
                                    JsValue::String(JsString::from_str(&style)),
                                ),
                            ];

                            if style == "currency" {
                                if let Some(ref c) = currency {
                                    props.push((
                                        "currency",
                                        JsValue::String(JsString::from_str(c)),
                                    ));
                                }
                                if let Some(ref cd) = currency_display {
                                    props.push((
                                        "currencyDisplay",
                                        JsValue::String(JsString::from_str(cd)),
                                    ));
                                }
                                if let Some(ref cs) = currency_sign {
                                    props.push((
                                        "currencySign",
                                        JsValue::String(JsString::from_str(cs)),
                                    ));
                                }
                            }

                            if style == "unit" {
                                if let Some(ref u) = unit {
                                    props.push((
                                        "unit",
                                        JsValue::String(JsString::from_str(u)),
                                    ));
                                }
                                if let Some(ref ud) = unit_display {
                                    props.push((
                                        "unitDisplay",
                                        JsValue::String(JsString::from_str(ud)),
                                    ));
                                }
                            }

                            props.push((
                                "minimumIntegerDigits",
                                JsValue::Number(minimum_integer_digits as f64),
                            ));
                            props.push((
                                "minimumFractionDigits",
                                JsValue::Number(minimum_fraction_digits as f64),
                            ));
                            props.push((
                                "maximumFractionDigits",
                                JsValue::Number(maximum_fraction_digits as f64),
                            ));

                            if let Some(min_sd) = minimum_significant_digits {
                                props.push((
                                    "minimumSignificantDigits",
                                    JsValue::Number(min_sd as f64),
                                ));
                            }
                            if let Some(max_sd) = maximum_significant_digits {
                                props.push((
                                    "maximumSignificantDigits",
                                    JsValue::Number(max_sd as f64),
                                ));
                            }

                            // useGrouping: spec says return a string or boolean
                            let ug_val = match use_grouping.as_str() {
                                "true" => JsValue::String(JsString::from_str("auto")),
                                "false" => JsValue::Boolean(false),
                                "auto" | "always" | "min2" => {
                                    JsValue::String(JsString::from_str(&use_grouping))
                                }
                                _ => JsValue::String(JsString::from_str(&use_grouping)),
                            };
                            props.push(("useGrouping", ug_val));

                            props.push((
                                "notation",
                                JsValue::String(JsString::from_str(&notation)),
                            ));

                            if notation == "compact" {
                                if let Some(ref cd) = compact_display {
                                    props.push((
                                        "compactDisplay",
                                        JsValue::String(JsString::from_str(cd)),
                                    ));
                                }
                            }

                            props.push((
                                "signDisplay",
                                JsValue::String(JsString::from_str(&sign_display)),
                            ));

                            props.push((
                                "roundingIncrement",
                                JsValue::Number(rounding_increment as f64),
                            ));
                            props.push((
                                "roundingMode",
                                JsValue::String(JsString::from_str(&rounding_mode)),
                            ));
                            props.push((
                                "roundingPriority",
                                JsValue::String(JsString::from_str(&rounding_priority)),
                            ));
                            props.push((
                                "trailingZeroDisplay",
                                JsValue::String(JsString::from_str(&trailing_zero_display)),
                            ));

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
                    "Intl.NumberFormat.prototype.resolvedOptions called on incompatible receiver",
                ))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("resolvedOptions".to_string(), resolved_fn);

        self.realm_mut().intl_number_format_prototype = Some(proto.clone());

        // --- Constructor ---
        let proto_id = proto.borrow().id.unwrap();
        let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });
        let proto_clone = proto.clone();

        let nf_ctor = self.create_function(JsFunction::constructor(
            "NumberFormat".to_string(),
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

                // localeMatcher
                let _locale_matcher = match interp.intl_get_option(
                    &options,
                    "localeMatcher",
                    &["lookup", "best fit"],
                    Some("best fit"),
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let raw_locale = interp.intl_resolve_locale(&requested);

                // numberingSystem: from option or unicode extension
                let opt_nu = match interp.intl_get_option(
                    &options,
                    "numberingSystem",
                    &[],
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // Validate numbering system: must match (3*8alphanum) *("-" (3*8alphanum))
                if let Some(ref nu) = opt_nu {
                    let valid = if nu.is_empty() {
                        false
                    } else {
                        nu.split('-').all(|part| {
                            part.len() >= 3
                                && part.len() <= 8
                                && part.chars().all(|c| c.is_ascii_alphanumeric())
                        })
                    };
                    if !valid {
                        return Completion::Throw(interp.create_range_error(&format!(
                            "Invalid numberingSystem value: {}",
                            nu
                        )));
                    }
                }

                let valid_opt_nu = opt_nu.filter(|nu| is_known_numbering_system(nu));

                let ext_nu = extract_unicode_extension(&raw_locale, "nu");
                let valid_ext_nu = ext_nu.filter(|nu| is_known_numbering_system(nu));
                let nu_from_option = valid_opt_nu.is_some();
                let numbering_system = valid_opt_nu
                    .or(valid_ext_nu.clone())
                    .unwrap_or_else(|| "latn".to_string());

                // style
                let style = match interp.intl_get_option(
                    &options,
                    "style",
                    &["decimal", "currency", "percent", "unit"],
                    Some("decimal"),
                ) {
                    Ok(Some(v)) => v,
                    Ok(None) => "decimal".to_string(),
                    Err(e) => return Completion::Throw(e),
                };

                // currency
                let currency_opt = match interp.intl_get_option(
                    &options,
                    "currency",
                    &[],
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // Validate currency code
                if let Some(ref c) = currency_opt {
                    if !is_well_formed_currency_code(c) {
                        return Completion::Throw(interp.create_range_error(&format!(
                            "Invalid currency code: {}",
                            c
                        )));
                    }
                }

                if style == "currency" && currency_opt.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Currency code is required with currency style"),
                    );
                }

                let currency = currency_opt.map(|c| c.to_ascii_uppercase());

                // currencyDisplay
                let currency_display = if style == "currency" {
                    match interp.intl_get_option(
                        &options,
                        "currencyDisplay",
                        &["symbol", "narrowSymbol", "code", "name"],
                        Some("symbol"),
                    ) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    }
                } else {
                    // Still read the option so we throw if it's an invalid value
                    match interp.intl_get_option(
                        &options,
                        "currencyDisplay",
                        &["symbol", "narrowSymbol", "code", "name"],
                        None,
                    ) {
                        Ok(_) => None,
                        Err(e) => return Completion::Throw(e),
                    }
                };

                // currencySign
                let currency_sign = if style == "currency" {
                    match interp.intl_get_option(
                        &options,
                        "currencySign",
                        &["standard", "accounting"],
                        Some("standard"),
                    ) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    }
                } else {
                    match interp.intl_get_option(
                        &options,
                        "currencySign",
                        &["standard", "accounting"],
                        None,
                    ) {
                        Ok(_) => None,
                        Err(e) => return Completion::Throw(e),
                    }
                };

                // unit
                let unit_opt = match interp.intl_get_option(
                    &options,
                    "unit",
                    &[],
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                if let Some(ref u) = unit_opt {
                    if !is_well_formed_unit_identifier(u) {
                        return Completion::Throw(interp.create_range_error(&format!(
                            "Invalid unit identifier: {}",
                            u
                        )));
                    }
                }

                if style == "unit" && unit_opt.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Unit is required with unit style"),
                    );
                }

                let unit = unit_opt;

                // unitDisplay
                let unit_display = if style == "unit" {
                    match interp.intl_get_option(
                        &options,
                        "unitDisplay",
                        &["short", "narrow", "long"],
                        Some("short"),
                    ) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    }
                } else {
                    match interp.intl_get_option(
                        &options,
                        "unitDisplay",
                        &["short", "narrow", "long"],
                        None,
                    ) {
                        Ok(_) => None,
                        Err(e) => return Completion::Throw(e),
                    }
                };

                // notation
                let notation = match interp.intl_get_option(
                    &options,
                    "notation",
                    &["standard", "scientific", "engineering", "compact"],
                    Some("standard"),
                ) {
                    Ok(Some(v)) => v,
                    Ok(None) => "standard".to_string(),
                    Err(e) => return Completion::Throw(e),
                };

                // Digit options
                let notation_is_compact = notation == "compact";

                // minimumIntegerDigits
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

                // Read raw options to detect presence
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
                    match interp.get_object_property(o.id, "minimumSignificantDigits", &options) {
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
                    match interp.get_object_property(o.id, "maximumSignificantDigits", &options) {
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

                // Read roundingIncrement raw value (read in spec order)
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

                // Default fraction digits based on style
                let (default_min_fd, default_max_fd) = if style == "currency" && notation == "standard" {
                    let cd = currency_digits(currency.as_deref().unwrap_or("USD"));
                    (cd, cd)
                } else if style == "percent" {
                    (0, 0)
                } else {
                    (0, if notation_is_compact { 0 } else { 3 })
                };

                let (minimum_fraction_digits, maximum_fraction_digits, minimum_significant_digits, maximum_significant_digits) =
                    if notation_is_compact && !has_sd && !has_fd {
                        // Compact notation with no explicit digit options
                        (0u32, 0u32, None::<u32>, None::<u32>)
                    } else if has_sd && !has_fd && rounding_priority == "auto" {
                        // Only significant digits specified
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
                        // Only fraction digits specified
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
                        // Per spec: if mnfd undefined, set to min(mnfdDefault, mxfd)
                        // If mxfd undefined, set to max(mxfdDefault, mnfd)
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
                        // Both specified (rounding priority matters)
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
                        // Neither specified (default)
                        if notation_is_compact {
                            (0, 0, Some(1), Some(2))
                        } else {
                            (default_min_fd, default_max_fd, None, None)
                        }
                    };

                // Validate roundingIncrement from raw value read earlier
                let rounding_increment = if let Some(ref ri_val) = raw_rounding_increment {
                    let num = match interp.to_number_value(ri_val) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    if num.is_nan() || num < 1.0 || num > 5000.0 {
                        return Completion::Throw(interp.create_range_error(&format!(
                            "roundingIncrement value is out of range"
                        )));
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

                // roundingIncrement constraints per spec
                if rounding_increment != 1 {
                    if minimum_significant_digits.is_some() || maximum_significant_digits.is_some()
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

                // compactDisplay (read after digit options per spec)
                let compact_display = if notation == "compact" {
                    match interp.intl_get_option(
                        &options,
                        "compactDisplay",
                        &["short", "long"],
                        Some("short"),
                    ) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    }
                } else {
                    match interp.intl_get_option(
                        &options,
                        "compactDisplay",
                        &["short", "long"],
                        None,
                    ) {
                        Ok(_) => None,
                        Err(e) => return Completion::Throw(e),
                    }
                };

                // useGrouping: GetStringOrBooleanOption per spec
                let use_grouping_default = if notation == "compact" { "min2" } else { "auto" };
                let use_grouping = {
                    let ug_val = if let JsValue::Object(o) = &options {
                        match interp.get_object_property(o.id, "useGrouping", &options) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => JsValue::Undefined,
                        }
                    } else {
                        JsValue::Undefined
                    };

                    match &ug_val {
                        JsValue::Undefined => use_grouping_default.to_string(),
                        JsValue::Boolean(true) => "always".to_string(),
                        _ => {
                            // Step 4-5: ToBoolean(value) - if false, return false
                            let val_bool = match &ug_val {
                                JsValue::Boolean(false) | JsValue::Null => false,
                                JsValue::Number(n) => *n != 0.0 && !n.is_nan(),
                                JsValue::String(s) => !s.to_rust_string().is_empty(),
                                _ => true,
                            };
                            if !val_bool {
                                "false".to_string()
                            } else {
                                // Step 6: ToString(value)
                                let sv = match interp.to_string_value(&ug_val) {
                                    Ok(s) => s,
                                    Err(e) => return Completion::Throw(e),
                                };
                                // Step 7: "true"/"false" strings -> fallback
                                if sv == "true" || sv == "false" {
                                    use_grouping_default.to_string()
                                } else if sv == "auto" || sv == "always" || sv == "min2" {
                                    sv
                                } else {
                                    return Completion::Throw(interp.create_range_error(
                                        "Invalid useGrouping value",
                                    ));
                                }
                            }
                        }
                    }
                };

                // signDisplay
                let sign_display = match interp.intl_get_option(
                    &options,
                    "signDisplay",
                    &["auto", "always", "exceptZero", "negative", "never"],
                    Some("auto"),
                ) {
                    Ok(Some(v)) => v,
                    Ok(None) => "auto".to_string(),
                    Err(e) => return Completion::Throw(e),
                };

                // Build the locale string
                let base = base_locale(&raw_locale);
                let ext_nu_raw = extract_unicode_extension(&raw_locale, "nu");
                let locale = if nu_from_option {
                    // Option takes precedence over extension.
                    // If extension had the same value, keep it in the locale.
                    // Otherwise strip the extension (option value is in resolvedOptions only).
                    if ext_nu_raw.as_deref() == Some(&*numbering_system) {
                        format!("{}-u-nu-{}", base, numbering_system)
                    } else {
                        base.clone()
                    }
                } else if valid_ext_nu.is_some() {
                    // Numbering system came from the unicode extension
                    format!("{}-u-nu-{}", base, numbering_system)
                } else {
                    base.clone()
                };

                let obj = interp.create_object();
                obj.borrow_mut().prototype = Some(proto_clone.clone());
                obj.borrow_mut().class_name = "Intl.NumberFormat".to_string();
                obj.borrow_mut().intl_data = Some(IntlData::NumberFormat {
                    locale,
                    numbering_system,
                    style,
                    currency,
                    currency_display,
                    currency_sign,
                    unit,
                    unit_display,
                    notation,
                    compact_display,
                    sign_display,
                    use_grouping,
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

        // Set NumberFormat.prototype on constructor
        if let JsValue::Object(ctor_ref) = &nf_ctor {
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
            PropertyDescriptor::data(nf_ctor.clone(), true, false, true),
        );

        // Save built-in constructor for internal use (e.g. toLocaleString)
        self.realm_mut().intl_number_format_ctor = Some(nf_ctor.clone());

        // Register Intl.NumberFormat on the Intl namespace
        intl_obj.borrow_mut().insert_property(
            "NumberFormat".to_string(),
            PropertyDescriptor::data(nf_ctor, true, false, true),
        );
    }
}
