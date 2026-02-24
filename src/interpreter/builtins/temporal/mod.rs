pub(crate) mod duration;
pub(crate) mod instant;
pub(crate) mod now;
pub(crate) mod plain_date;
pub(crate) mod plain_date_time;
pub(crate) mod plain_month_day;
pub(crate) mod plain_time;
pub(crate) mod plain_year_month;
pub(crate) mod zoned_date_time;

use super::*;

/// Which default date/time components to use when constructing DTF from toLocaleString.
pub(crate) enum TemporalDefaults {
    /// date + time (Instant, PlainDateTime, ZonedDateTime)
    All,
    /// date only (PlainDate)
    Date,
    /// time only (PlainTime)
    Time,
    /// year + month only (PlainYearMonth)
    YearMonth,
    /// month + day only (PlainMonthDay)
    MonthDay,
}

/// Check if options object has any explicit date/time formatting properties.
fn has_datetime_options(interp: &mut Interpreter, options: &JsValue) -> bool {
    if matches!(options, JsValue::Undefined | JsValue::Null) {
        return false;
    }
    if let JsValue::Object(o) = options {
        for key in &[
            "dateStyle",
            "timeStyle",
            "year",
            "month",
            "day",
            "weekday",
            "hour",
            "minute",
            "second",
            "fractionalSecondDigits",
            "dayPeriod",
            "timeZoneName",
            "era",
        ] {
            if let Completion::Normal(v) = interp.get_object_property(o.id, key, options) {
                if !matches!(v, JsValue::Undefined) {
                    return true;
                }
            }
        }
    }
    false
}

/// Construct a DateTimeFormat with Temporal-type-appropriate defaults.
/// If user didn't specify any date/time options, add defaults based on the Temporal type.
pub(crate) fn temporal_construct_dtf(
    interp: &mut Interpreter,
    dtf_ctor: &JsValue,
    locales: &JsValue,
    options: &JsValue,
    defaults: TemporalDefaults,
) -> Completion {
    if has_datetime_options(interp, options) {
        return interp.construct(dtf_ctor, &[locales.clone(), options.clone()]);
    }

    // Create options object with appropriate defaults
    let opts_obj = interp.create_object();
    if let Some(ref op) = interp.realm().object_prototype {
        opts_obj.borrow_mut().prototype = Some(op.clone());
    }

    // Copy any existing properties from user options (e.g. timeZone)
    if let JsValue::Object(o) = options {
        if let Some(obj) = interp.get_object(o.id) {
            let keys: Vec<String> = obj.borrow().property_order.clone();
            for key in keys {
                if let Completion::Normal(v) = interp.get_object_property(o.id, &key, options) {
                    if !matches!(v, JsValue::Undefined) {
                        opts_obj
                            .borrow_mut()
                            .insert_property(key, PropertyDescriptor::data(v, true, true, true));
                    }
                }
            }
        }
    }

    // Add type-specific defaults
    let numeric = JsValue::String(JsString::from_str("numeric"));
    let two_digit = JsValue::String(JsString::from_str("2-digit"));
    match defaults {
        TemporalDefaults::All => {
            opts_obj.borrow_mut().insert_property(
                "year".to_string(),
                PropertyDescriptor::data(numeric.clone(), true, true, true),
            );
            opts_obj.borrow_mut().insert_property(
                "month".to_string(),
                PropertyDescriptor::data(numeric.clone(), true, true, true),
            );
            opts_obj.borrow_mut().insert_property(
                "day".to_string(),
                PropertyDescriptor::data(numeric.clone(), true, true, true),
            );
            opts_obj.borrow_mut().insert_property(
                "hour".to_string(),
                PropertyDescriptor::data(two_digit.clone(), true, true, true),
            );
            opts_obj.borrow_mut().insert_property(
                "minute".to_string(),
                PropertyDescriptor::data(two_digit.clone(), true, true, true),
            );
            opts_obj.borrow_mut().insert_property(
                "second".to_string(),
                PropertyDescriptor::data(two_digit.clone(), true, true, true),
            );
        }
        TemporalDefaults::Date => {
            opts_obj.borrow_mut().insert_property(
                "year".to_string(),
                PropertyDescriptor::data(numeric.clone(), true, true, true),
            );
            opts_obj.borrow_mut().insert_property(
                "month".to_string(),
                PropertyDescriptor::data(numeric.clone(), true, true, true),
            );
            opts_obj.borrow_mut().insert_property(
                "day".to_string(),
                PropertyDescriptor::data(numeric.clone(), true, true, true),
            );
        }
        TemporalDefaults::Time => {
            opts_obj.borrow_mut().insert_property(
                "hour".to_string(),
                PropertyDescriptor::data(two_digit.clone(), true, true, true),
            );
            opts_obj.borrow_mut().insert_property(
                "minute".to_string(),
                PropertyDescriptor::data(two_digit.clone(), true, true, true),
            );
            opts_obj.borrow_mut().insert_property(
                "second".to_string(),
                PropertyDescriptor::data(two_digit.clone(), true, true, true),
            );
        }
        TemporalDefaults::YearMonth => {
            opts_obj.borrow_mut().insert_property(
                "year".to_string(),
                PropertyDescriptor::data(numeric.clone(), true, true, true),
            );
            opts_obj.borrow_mut().insert_property(
                "month".to_string(),
                PropertyDescriptor::data(numeric.clone(), true, true, true),
            );
        }
        TemporalDefaults::MonthDay => {
            opts_obj.borrow_mut().insert_property(
                "month".to_string(),
                PropertyDescriptor::data(numeric.clone(), true, true, true),
            );
            opts_obj.borrow_mut().insert_property(
                "day".to_string(),
                PropertyDescriptor::data(numeric.clone(), true, true, true),
            );
        }
    }

    let opts_id = opts_obj.borrow().id.unwrap();
    let opts_val = JsValue::Object(crate::types::JsObject { id: opts_id });
    interp.construct(dtf_ctor, &[locales.clone(), opts_val])
}

/// Check if options object has a style that conflicts with the temporal type.
/// In toLocaleString, each style must overlap with the type's data model:
/// - dateStyle requires date data (PlainDate, PlainDateTime, PlainYearMonth, PlainMonthDay)
/// - timeStyle requires time data (PlainTime, PlainDateTime)
/// Returns Err with TypeError if conflict detected.
pub(crate) fn check_locale_string_style_conflict(
    interp: &mut Interpreter,
    options: &JsValue,
    has_date: bool,
    has_time: bool,
) -> Result<(), JsValue> {
    if let JsValue::Object(o) = options {
        let ds = match interp.get_object_property(o.id, "dateStyle", options) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => JsValue::Undefined,
        };
        let ts = match interp.get_object_property(o.id, "timeStyle", options) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => JsValue::Undefined,
        };
        let has_ds = !matches!(ds, JsValue::Undefined);
        let has_ts = !matches!(ts, JsValue::Undefined);
        if has_ds && !has_date {
            return Err(
                interp.create_type_error("dateStyle does not overlap with this Temporal type")
            );
        }
        if has_ts && !has_time {
            return Err(
                interp.create_type_error("timeStyle does not overlap with this Temporal type")
            );
        }
    }
    Ok(())
}

/// Check calendar mismatch: if the Temporal object has a non-ISO calendar,
/// the DTF's calendar must match. Call after constructing the DTF.
pub(crate) fn check_calendar_mismatch(
    interp: &mut Interpreter,
    dtf_instance: &JsValue,
    temporal_calendar: &str,
    allow_iso: bool,
) -> Result<(), JsValue> {
    if allow_iso && temporal_calendar == "iso8601" {
        return Ok(());
    }
    if let JsValue::Object(dtf_obj) = dtf_instance {
        let resolved_fn =
            match interp.get_object_property(dtf_obj.id, "resolvedOptions", dtf_instance) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => return Ok(()),
            };
        let resolved = match interp.call_function(&resolved_fn, dtf_instance, &[]) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => return Ok(()),
        };
        if let JsValue::Object(ro) = &resolved {
            let dtf_cal = match interp.get_object_property(ro.id, "calendar", &resolved) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::Undefined,
            };
            if let JsValue::String(ref s) = dtf_cal {
                let dtf_cal_str = s.to_string();
                if dtf_cal_str != temporal_calendar {
                    return Err(interp.create_range_error(&format!(
                        "calendar mismatch: Temporal object uses '{}' but locale uses '{}'",
                        temporal_calendar, dtf_cal_str
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Call DTF.format(temporal_value) on a constructed DTF instance
pub(crate) fn temporal_format_with_dtf(
    interp: &mut Interpreter,
    dtf_instance: &JsValue,
    temporal_this: &JsValue,
) -> Completion {
    if let JsValue::Object(dtf_obj) = dtf_instance {
        let format_val = match interp.get_object_property(dtf_obj.id, "format", dtf_instance) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Completion::Throw(e),
            _ => JsValue::Undefined,
        };
        match interp.call_function(&format_val, dtf_instance, &[temporal_this.clone()]) {
            Completion::Normal(v) => Completion::Normal(v),
            Completion::Throw(e) => Completion::Throw(e),
            _ => Completion::Normal(JsValue::Undefined),
        }
    } else {
        Completion::Normal(JsValue::Undefined)
    }
}

/// ASCII-only lowercase for calendar IDs per spec (ToTemporalCalendarSlotValue).
/// Only lowercase ASCII A-Z; non-ASCII characters are NOT lowercased.
pub(crate) fn ascii_lowercase(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_uppercase() {
                (b + 32) as char
            } else {
                b as char
            }
        })
        .collect()
}

/// Validate and normalize a calendar ID per spec's ToTemporalCalendarSlotValue.
/// Accepts:
/// - "iso8601" (case-insensitive, ASCII only)
/// - An ISO 8601 date string (extracts calendar annotation, defaults to "iso8601")
///
/// Returns the normalized calendar ID, or None if invalid.
const SUPPORTED_CALENDARS: &[&str] = &[
    "iso8601",
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
    "japanese",
    "persian",
    "roc",
];

fn canonicalize_temporal_calendar(cal: &str) -> String {
    let lower = ascii_lowercase(cal);
    match lower.as_str() {
        "islamicc" => "islamic-civil".to_string(),
        "ethiopic-amete-alem" => "ethioaa".to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn is_supported_temporal_calendar(cal: &str) -> bool {
    SUPPORTED_CALENDARS.contains(&cal)
}

pub(crate) fn calendar_has_eras(cal: &str) -> bool {
    matches!(
        cal,
        "buddhist"
            | "coptic"
            | "ethioaa"
            | "ethiopic"
            | "gregory"
            | "hebrew"
            | "indian"
            | "islamic-civil"
            | "islamic-tbla"
            | "islamic-umalqura"
            | "japanese"
            | "persian"
            | "roc"
    )
}

fn extract_calendar_annotation(s: &str) -> Option<String> {
    let mut rest = s;
    while let Some(bracket_pos) = rest.find('[') {
        let after = &rest[bracket_pos + 1..];
        let skip_bang = if after.starts_with('!') { 1 } else { 0 };
        let content = &after[skip_bang..];
        if let Some(close) = content.find(']') {
            let annotation = &content[..close];
            if let Some(eq) = annotation.find('=') {
                let key = &annotation[..eq];
                if key == "u-ca" {
                    return Some(annotation[eq + 1..].to_string());
                }
            }
            rest = &content[close + 1..];
        } else {
            break;
        }
    }
    None
}

pub(crate) fn validate_calendar(cal: &str) -> Option<String> {
    if !cal.bytes().all(|b| b.is_ascii()) {
        return None;
    }
    let normalized = canonicalize_temporal_calendar(cal);
    if is_supported_temporal_calendar(&normalized) {
        return Some(normalized);
    }
    if let Some(parsed) = parse_temporal_date_time_string(cal) {
        let c = parsed.calendar.unwrap_or_else(|| "iso8601".to_string());
        let cn = canonicalize_temporal_calendar(&c);
        if is_supported_temporal_calendar(&cn) {
            return Some(cn);
        }
    }
    if parse_temporal_time_string(cal).is_some() {
        // Extract calendar from [u-ca=...] annotation if present
        if let Some(ca_cal) = extract_calendar_annotation(cal) {
            let cn = canonicalize_temporal_calendar(&ca_cal);
            if is_supported_temporal_calendar(&cn) {
                return Some(cn);
            }
        }
        return Some("iso8601".to_string());
    }
    if let Some(parsed) = parse_temporal_month_day_string(cal) {
        let c = parsed.3.unwrap_or_else(|| "iso8601".to_string());
        let cn = canonicalize_temporal_calendar(&c);
        if is_supported_temporal_calendar(&cn) {
            return Some(cn);
        }
    }
    if let Some(parsed) = parse_temporal_year_month_string(cal) {
        let c = parsed.3.unwrap_or_else(|| "iso8601".to_string());
        let cn = canonicalize_temporal_calendar(&c);
        if is_supported_temporal_calendar(&cn) {
            return Some(cn);
        }
    }
    None
}

use icu::calendar::types::MonthCode as IcuMonthCode;
use icu::calendar::{AnyCalendar, AnyCalendarKind, Date as IcuDate};
use icu_calendar::types::DateFields as IcuDateFields;
use tinystr::TinyAsciiStr;

fn calendar_id_to_icu_kind(cal: &str) -> Option<AnyCalendarKind> {
    match cal {
        "buddhist" => Some(AnyCalendarKind::Buddhist),
        "chinese" => Some(AnyCalendarKind::Chinese),
        "coptic" => Some(AnyCalendarKind::Coptic),
        "dangi" => Some(AnyCalendarKind::Dangi),
        "ethioaa" => Some(AnyCalendarKind::EthiopianAmeteAlem),
        "ethiopic" => Some(AnyCalendarKind::Ethiopian),
        "gregory" => Some(AnyCalendarKind::Gregorian),
        "hebrew" => Some(AnyCalendarKind::Hebrew),
        "indian" => Some(AnyCalendarKind::Indian),
        "islamic" | "islamic-civil" => Some(AnyCalendarKind::HijriTabularTypeIIFriday),
        "islamic-rgsa" => Some(AnyCalendarKind::HijriSimulatedMecca),
        "islamic-tbla" => Some(AnyCalendarKind::HijriTabularTypeIIThursday),
        "islamic-umalqura" => Some(AnyCalendarKind::HijriUmmAlQura),
        "japanese" => Some(AnyCalendarKind::Japanese),
        "persian" => Some(AnyCalendarKind::Persian),
        "roc" => Some(AnyCalendarKind::Roc),
        _ => None,
    }
}

pub(crate) struct CalendarFields {
    pub year: i32,
    pub era: Option<String>,
    pub era_year: Option<i32>,
    pub month_ordinal: u8,
    pub month_code: String,
    pub day: u8,
    pub day_of_year: u16,
    pub days_in_month: u8,
    pub days_in_year: u16,
    pub months_in_year: u8,
    pub in_leap_year: bool,
}

pub(crate) fn iso_to_calendar_fields(
    iso_year: i32,
    iso_month: u8,
    iso_day: u8,
    calendar_id: &str,
) -> Option<CalendarFields> {
    let kind = calendar_id_to_icu_kind(calendar_id)?;
    let iso_date = IcuDate::try_new_iso(iso_year, iso_month, iso_day).ok()?;
    let cal = AnyCalendar::new(kind);
    let d = iso_date.to_any().to_calendar(cal);

    let yi = d.year();
    let (year, era, era_year) = if let Some(e) = yi.era() {
        let ext = yi.extended_year();
        // Japanese calendar: dates before 1873-01-01 use ce/bce instead of meiji etc.
        if calendar_id == "japanese"
            && (iso_year < 1873 || (iso_year == 1873 && iso_month == 1 && iso_day == 1 && false))
            && !matches!(e.era.to_string().as_str(), "ce" | "bce")
        {
            if ext >= 1 {
                (ext, Some("ce".to_string()), Some(ext))
            } else {
                (ext, Some("bce".to_string()), Some(1 - ext))
            }
        } else {
            (ext, Some(e.era.to_string()), Some(e.year))
        }
    } else {
        (yi.era_year_or_related_iso(), None, None)
    };

    Some(CalendarFields {
        year,
        era,
        era_year,
        month_ordinal: d.month().ordinal,
        month_code: d.month().standard_code.0.to_string(),
        day: d.day_of_month().0,
        day_of_year: d.day_of_year().0,
        days_in_month: d.days_in_month(),
        days_in_year: d.days_in_year(),
        months_in_year: d.months_in_year(),
        in_leap_year: d.is_in_leap_year(),
    })
}

pub(crate) fn calendar_fields_to_iso(
    era: Option<&str>,
    year: i32,
    month_code: Option<&str>,
    month_ordinal: Option<u8>,
    day: u8,
    calendar_id: &str,
) -> Option<(i32, u8, u8)> {
    let kind = calendar_id_to_icu_kind(calendar_id)?;
    let cal = AnyCalendar::new(kind);

    let mut fields = IcuDateFields::default();
    if let Some(e) = era {
        fields.era = Some(e.as_bytes());
        fields.era_year = Some(year);
    } else {
        fields.extended_year = Some(year);
    }
    if let Some(mc) = month_code {
        fields.month_code = Some(mc.as_bytes());
    }
    if let Some(mo) = month_ordinal {
        fields.ordinal_month = Some(mo);
    }
    fields.day = Some(day);

    let d = IcuDate::try_from_fields(fields, Default::default(), cal).ok()?;
    let iso = d.to_iso();
    Some((
        iso.year().extended_year(),
        iso.month().ordinal,
        iso.day_of_month().0,
    ))
}

fn validate_calendar_date_round_trip(
    iso_y: i32,
    iso_m: u8,
    iso_d: u8,
    calendar_id: &str,
    requested_day: u8,
    requested_month_code: Option<&str>,
) -> bool {
    if let Some(cf) = iso_to_calendar_fields(iso_y, iso_m, iso_d, calendar_id) {
        if cf.day != requested_day {
            return false;
        }
        if let Some(mc) = requested_month_code {
            if cf.month_code != mc {
                return false;
            }
        }
        true
    } else {
        false
    }
}

pub(crate) fn iso_to_calendar_fields_from_year(
    year: i32,
    calendar_id: &str,
) -> Option<CalendarFields> {
    let kind = calendar_id_to_icu_kind(calendar_id)?;
    let cal = AnyCalendar::new(kind);
    let mut f = IcuDateFields::default();
    f.extended_year = Some(year);
    f.ordinal_month = Some(1);
    f.day = Some(1);
    let d = IcuDate::try_from_fields(f, Default::default(), cal).ok()?;
    Some(CalendarFields {
        year,
        era: None,
        era_year: None,
        month_ordinal: 1,
        month_code: d.month().standard_code.0.to_string(),
        day: 1,
        day_of_year: 1,
        days_in_month: d.days_in_month(),
        days_in_year: d.days_in_year(),
        months_in_year: d.months_in_year(),
        in_leap_year: d.is_in_leap_year(),
    })
}

pub(crate) fn ordinal_month_to_month_code(
    year: i32,
    ordinal_month: u8,
    calendar_id: &str,
) -> Option<String> {
    let kind = calendar_id_to_icu_kind(calendar_id)?;
    let cal = AnyCalendar::new(kind);
    let mut f = IcuDateFields::default();
    f.extended_year = Some(year);
    f.ordinal_month = Some(ordinal_month);
    f.day = Some(1);
    let d = IcuDate::try_from_fields(f, Default::default(), cal).ok()?;
    Some(d.month().standard_code.0.to_string())
}

/// Like `calendar_fields_to_iso`, but with overflow handling.
fn is_valid_month_code_for_calendar(month_code: &str, calendar_id: &str) -> bool {
    if month_code.len() == 3 && month_code.starts_with('M') {
        if let Ok(num) = month_code[1..].parse::<u8>() {
            let max_month = if matches!(calendar_id, "coptic" | "ethiopic" | "ethioaa") {
                13
            } else {
                12
            };
            return num >= 1 && num <= max_month;
        }
        return false;
    }
    if month_code.len() == 4 && month_code.starts_with('M') && month_code.ends_with('L') {
        if let Ok(num) = month_code[1..3].parse::<u8>() {
            if num < 1 || num > 12 {
                return false;
            }
            if calendar_id == "hebrew" {
                return num == 5;
            }
            return matches!(calendar_id, "chinese" | "dangi");
        }
        return false;
    }
    false
}

/// When overflow is "constrain", clamps the day to the calendar month's maximum.
/// When overflow is "reject", returns None for invalid dates.
pub(crate) fn calendar_fields_to_iso_overflow(
    era: Option<&str>,
    year: i32,
    month_code: Option<&str>,
    month_ordinal: Option<u8>,
    day: u8,
    calendar_id: &str,
    overflow: &str,
) -> Option<(i32, u8, u8)> {
    if let Some(mc) = month_code {
        if !is_valid_month_code_for_calendar(mc, calendar_id) {
            return None;
        }
    }
    if let Some((iy, im, id)) =
        calendar_fields_to_iso(era, year, month_code, month_ordinal, day, calendar_id)
    {
        if overflow == "reject" {
            if !validate_calendar_date_round_trip(iy, im, id, calendar_id, day, month_code) {
                return None;
            }
        }
        return Some((iy, im, id));
    }
    if overflow == "reject" {
        return None;
    }
    // Constrain: find the max valid day for this month
    let kind = calendar_id_to_icu_kind(calendar_id)?;
    let cal = AnyCalendar::new(kind);
    let mut fields = IcuDateFields::default();
    if let Some(e) = era {
        fields.era = Some(e.as_bytes());
        fields.era_year = Some(year);
    } else {
        fields.extended_year = Some(year);
    }
    if let Some(mc) = month_code {
        fields.month_code = Some(mc.as_bytes());
    }
    if let Some(mo) = month_ordinal {
        fields.ordinal_month = Some(mo);
    }
    fields.day = Some(1);
    match IcuDate::try_from_fields(fields, Default::default(), cal) {
        Ok(d) => {
            let max_day = d.days_in_month();
            let clamped = day.min(max_day).max(1);
            calendar_fields_to_iso(era, year, month_code, month_ordinal, clamped, calendar_id)
        }
        Err(_) => {
            // Month code doesn't exist (e.g. M06L in non-leap year) — fall back
            if let Some(mc) = month_code {
                if mc.ends_with('L') {
                    let fallback_mc = if calendar_id == "hebrew" && mc == "M05L" {
                        "M06".to_string()
                    } else {
                        mc[..mc.len() - 1].to_string()
                    };
                    let kind2 = calendar_id_to_icu_kind(calendar_id)?;
                    let cal2 = AnyCalendar::new(kind2);
                    let mut f2 = IcuDateFields::default();
                    if let Some(e) = era {
                        f2.era = Some(e.as_bytes());
                        f2.era_year = Some(year);
                    } else {
                        f2.extended_year = Some(year);
                    }
                    f2.month_code = Some(fallback_mc.as_bytes());
                    f2.day = Some(1);
                    if let Ok(temp) = IcuDate::try_from_fields(f2, Default::default(), cal2) {
                        let max_day = temp.days_in_month();
                        let clamped = day.min(max_day).max(1);
                        return calendar_fields_to_iso(
                            era,
                            year,
                            Some(&fallback_mc),
                            month_ordinal,
                            clamped,
                            calendar_id,
                        );
                    }
                }
            }
            // Ordinal month out of range: clamp to last month
            if let Some(mo) = month_ordinal {
                if month_code.is_none() {
                    let kind3 = calendar_id_to_icu_kind(calendar_id)?;
                    let cal3 = AnyCalendar::new(kind3);
                    let mut f3 = IcuDateFields::default();
                    if let Some(e) = era {
                        f3.era = Some(e.as_bytes());
                        f3.era_year = Some(year);
                    } else {
                        f3.extended_year = Some(year);
                    }
                    f3.ordinal_month = Some(1);
                    f3.day = Some(1);
                    if let Ok(temp) = IcuDate::try_from_fields(f3, Default::default(), cal3) {
                        let miy = temp.months_in_year();
                        let clamped_mo = mo.min(miy);
                        let kind4 = calendar_id_to_icu_kind(calendar_id)?;
                        let cal4 = AnyCalendar::new(kind4);
                        let mut f4 = IcuDateFields::default();
                        if let Some(e) = era {
                            f4.era = Some(e.as_bytes());
                            f4.era_year = Some(year);
                        } else {
                            f4.extended_year = Some(year);
                        }
                        f4.ordinal_month = Some(clamped_mo);
                        f4.day = Some(1);
                        if let Ok(temp2) = IcuDate::try_from_fields(f4, Default::default(), cal4) {
                            let max_day = temp2.days_in_month();
                            let clamped_d = day.min(max_day).max(1);
                            return calendar_fields_to_iso(
                                era,
                                year,
                                None,
                                Some(clamped_mo),
                                clamped_d,
                                calendar_id,
                            );
                        }
                    }
                }
            }
            None
        }
    }
}

/// Convert a calendar month code + day into an ISO date for PlainMonthDay.
///
/// The reference ISO year is chosen as the latest ISO year ≤ 1972 where the
/// calendar date (month_code, day) exists. This matches the spec behavior for
/// Temporal.PlainMonthDay.
///
/// When `year_hint` is provided (from the property bag's `year` field), it's used
/// for validation: in reject mode, the day must be valid in that specific year;
/// in constrain mode, the day is clamped to valid range for that year. Either way,
/// the stored reference year uses the 1972-or-earlier rule.
///
/// Returns (iso_year, iso_month, iso_day) or None if invalid.
pub(crate) fn calendar_month_day_to_iso(
    month_code: &str,
    day: u8,
    year_hint: Option<i32>,
    calendar_id: &str,
    overflow: &str,
) -> Option<(i32, u8, u8)> {
    if !is_valid_month_code_for_calendar(month_code, calendar_id) {
        return None;
    }
    let kind = calendar_id_to_icu_kind(calendar_id)?;

    // If a year_hint is provided, validate/constrain against it first
    let actual_day = if let Some(hint_year) = year_hint {
        let cal = AnyCalendar::new(kind);
        let mut f = IcuDateFields::default();
        f.extended_year = Some(hint_year);
        f.month_code = Some(month_code.as_bytes());
        f.day = Some(day);
        if IcuDate::try_from_fields(f, Default::default(), cal).is_ok() {
            // Day is valid in hint year
            day
        } else {
            // Day doesn't fit in hint year
            if overflow == "reject" {
                return None;
            }
            // Constrain: find max days in this month for the hint year
            let cal = AnyCalendar::new(kind);
            let mut f = IcuDateFields::default();
            f.extended_year = Some(hint_year);
            f.month_code = Some(month_code.as_bytes());
            f.day = Some(1);
            if let Ok(temp) = IcuDate::try_from_fields(f, Default::default(), cal) {
                day.min(temp.days_in_month()).max(1)
            } else {
                // Month code doesn't exist in hint year (e.g., M05L in non-leap)
                if overflow == "reject" {
                    return None;
                }
                day // will be further constrained below
            }
        }
    } else {
        day
    };

    // Now find the reference ISO year: latest ISO year ≤ 1972 where month_code+actual_day exists.
    // Convert ISO 1972-07-01 to the calendar, then try multiple calendar years that could
    // map to ISO years ≤ 1972.
    let cal_1972 = AnyCalendar::new(kind);
    let iso_ref = IcuDate::try_new_iso(1972, 7, 1).ok()?;
    let cal_ref = iso_ref.to_any().to_calendar(cal_1972);
    let base_cal_year = cal_ref.year().extended_year();

    let search_range = if matches!(calendar_id, "chinese" | "dangi") {
        60
    } else {
        30
    };
    let mut best: Option<(i32, u8, u8)> = None;
    for cy in (base_cal_year - search_range..=base_cal_year + 1).rev() {
        let cal = AnyCalendar::new(kind);
        let mut f = IcuDateFields::default();
        f.extended_year = Some(cy);
        f.month_code = Some(month_code.as_bytes());
        f.day = Some(actual_day);
        if let Ok(d) = IcuDate::try_from_fields(f, Default::default(), cal) {
            let actual_mc = d.month().standard_code.0.to_string();
            if actual_mc != month_code {
                continue;
            }
            if d.day_of_month().0 != actual_day {
                continue;
            }
            let iso = d.to_iso();
            let iso_result_year = iso.year().extended_year();
            if iso_result_year <= 1972 {
                let candidate = (iso_result_year, iso.month().ordinal, iso.day_of_month().0);
                if best.is_none()
                    || candidate.0 > best.unwrap().0
                    || (candidate.0 == best.unwrap().0 && candidate > best.unwrap())
                {
                    best = Some(candidate);
                }
            }
        }
    }
    if best.is_some() {
        return best;
    }

    // Broader search for unusual month codes in lunisolar calendars
    if overflow == "constrain" {
        // First find the max days for this month_code across years ≤ 1972
        let mut best_max = 0u8;
        for cy in (base_cal_year - search_range..=base_cal_year + 1).rev() {
            let cal = AnyCalendar::new(kind);
            let mut f = IcuDateFields::default();
            f.extended_year = Some(cy);
            f.month_code = Some(month_code.as_bytes());
            f.day = Some(1);
            if let Ok(temp) = IcuDate::try_from_fields(f, Default::default(), cal) {
                let actual_mc = temp.month().standard_code.0.to_string();
                if actual_mc != month_code {
                    continue;
                }
                let iso = temp.to_iso();
                if iso.year().extended_year() > 1972 {
                    continue;
                }
                let md = temp.days_in_month();
                if md > best_max {
                    best_max = md;
                }
            }
        }
        if best_max > 0 {
            let clamped = actual_day.min(best_max).max(1);
            // Now find the latest year ≤ 1972 where this month_code + clamped day exists
            let mut best2: Option<(i32, u8, u8)> = None;
            for cy in (base_cal_year - search_range..=base_cal_year + 1).rev() {
                let cal = AnyCalendar::new(kind);
                let mut f = IcuDateFields::default();
                f.extended_year = Some(cy);
                f.month_code = Some(month_code.as_bytes());
                f.day = Some(clamped);
                if let Ok(d) = IcuDate::try_from_fields(f, Default::default(), cal) {
                    let amc = d.month().standard_code.0.to_string();
                    if amc != month_code {
                        continue;
                    }
                    if d.day_of_month().0 != clamped {
                        continue;
                    }
                    let iso = d.to_iso();
                    let iy = iso.year().extended_year();
                    if iy <= 1972 {
                        let c = (iy, iso.month().ordinal, iso.day_of_month().0);
                        if best2.is_none() || c.0 > best2.unwrap().0 {
                            best2 = Some(c);
                        }
                    }
                }
            }
            if best2.is_some() {
                return best2;
            }
        }
    }

    None
}

/// Add years/months/weeks/days to an ISO date using a non-ISO calendar for year/month arithmetic.
/// Returns the resulting ISO date (y, m, d).
pub(crate) fn add_calendar_date(
    iso_year: i32,
    iso_month: u8,
    iso_day: u8,
    years: i32,
    months: i32,
    weeks: i32,
    days: i32,
    calendar_id: &str,
    overflow: &str,
) -> Option<(i32, u8, u8)> {
    let kind = calendar_id_to_icu_kind(calendar_id)?;
    let cal = AnyCalendar::new(kind);

    // Convert ISO → calendar date
    let iso = IcuDate::try_new_iso(iso_year, iso_month, iso_day).ok()?;
    let mut cal_date = iso.to_any().to_calendar(cal);
    let original_day = cal_date.day_of_month().0;

    // Step 1: add years
    if years != 0 {
        let yi = cal_date.year().extended_year();
        let new_y = yi + years;
        let mc = cal_date.month().standard_code;
        let mc_str = mc.0.to_string();
        let d = cal_date.day_of_month().0;
        let new_cal = AnyCalendar::new(kind);
        let mut fields = IcuDateFields::default();
        fields.extended_year = Some(new_y);
        fields.month_code = Some(mc_str.as_bytes());
        fields.day = Some(d);
        cal_date = match IcuDate::try_from_fields(fields, Default::default(), new_cal) {
            Ok(result) => {
                if overflow == "reject" {
                    if result.day_of_month().0 != d
                        || result.month().standard_code.0.to_string() != mc_str
                    {
                        return None;
                    }
                }
                result
            }
            Err(_) => {
                if overflow == "reject" {
                    return None;
                }
                // Try keeping month code, just clamp day
                let new_cal2 = AnyCalendar::new(kind);
                let mut fields2 = IcuDateFields::default();
                fields2.extended_year = Some(new_y);
                fields2.month_code = Some(mc_str.as_bytes());
                fields2.day = Some(1);
                if let Ok(temp) = IcuDate::try_from_fields(fields2, Default::default(), new_cal2) {
                    let max_d = temp.days_in_month();
                    let clamped_d = d.min(max_d);
                    let new_cal3 = AnyCalendar::new(kind);
                    let mut fields3 = IcuDateFields::default();
                    fields3.extended_year = Some(new_y);
                    fields3.month_code = Some(mc_str.as_bytes());
                    fields3.day = Some(clamped_d);
                    IcuDate::try_from_fields(fields3, Default::default(), new_cal3).ok()?
                } else if mc_str.ends_with('L') {
                    // Month code doesn't exist (e.g. M06L in non-leap year)
                    // Hebrew M05L (Adar I) constrains to M06 (Adar);
                    // all other calendars strip the 'L' suffix.
                    let fallback_mc = if calendar_id == "hebrew" && mc_str == "M05L" {
                        "M06".to_string()
                    } else {
                        mc_str[..mc_str.len() - 1].to_string()
                    };
                    let new_cal4 = AnyCalendar::new(kind);
                    let mut fields4 = IcuDateFields::default();
                    fields4.extended_year = Some(new_y);
                    fields4.month_code = Some(fallback_mc.as_bytes());
                    fields4.day = Some(d);
                    match IcuDate::try_from_fields(fields4, Default::default(), new_cal4) {
                        Ok(d4) => d4,
                        Err(_) => {
                            let new_cal5 = AnyCalendar::new(kind);
                            let mut fields5 = IcuDateFields::default();
                            fields5.extended_year = Some(new_y);
                            fields5.month_code = Some(fallback_mc.as_bytes());
                            fields5.day = Some(1);
                            let temp2 =
                                IcuDate::try_from_fields(fields5, Default::default(), new_cal5)
                                    .ok()?;
                            let max_d2 = temp2.days_in_month();
                            let clamped_d2 = d.min(max_d2);
                            let new_cal6 = AnyCalendar::new(kind);
                            let mut fields6 = IcuDateFields::default();
                            fields6.extended_year = Some(new_y);
                            fields6.month_code = Some(fallback_mc.as_bytes());
                            fields6.day = Some(clamped_d2);
                            IcuDate::try_from_fields(fields6, Default::default(), new_cal6).ok()?
                        }
                    }
                } else {
                    return None;
                }
            }
        };
    }

    // Step 2: add months
    if months != 0 {
        let yi = cal_date.year().extended_year();
        let mo = cal_date.month().ordinal;
        let d = original_day; // preserve day across year-then-month adds
        let miy = cal_date.months_in_year();

        // Compute new month ordinal and year, wrapping around
        let mut total_months = (mo as i32 - 1) + months;
        let mut new_y = yi;
        while total_months < 0 {
            new_y -= 1;
            // Get months in the new year
            let new_cal = AnyCalendar::new(kind);
            let mut fields = IcuDateFields::default();
            fields.extended_year = Some(new_y);
            fields.ordinal_month = Some(1);
            fields.day = Some(1);
            if let Ok(temp) = IcuDate::try_from_fields(fields, Default::default(), new_cal) {
                total_months += temp.months_in_year() as i32;
            } else {
                total_months += miy as i32;
            }
        }
        loop {
            let new_cal = AnyCalendar::new(kind);
            let mut fields = IcuDateFields::default();
            fields.extended_year = Some(new_y);
            fields.ordinal_month = Some(1);
            fields.day = Some(1);
            let cur_miy =
                if let Ok(temp) = IcuDate::try_from_fields(fields, Default::default(), new_cal) {
                    temp.months_in_year() as i32
                } else {
                    miy as i32
                };
            if total_months < cur_miy {
                break;
            }
            total_months -= cur_miy;
            new_y += 1;
        }
        let new_mo = (total_months as u8) + 1;

        let new_cal = AnyCalendar::new(kind);
        let mut fields = IcuDateFields::default();
        fields.extended_year = Some(new_y);
        fields.ordinal_month = Some(new_mo);
        fields.day = Some(d);
        cal_date = match IcuDate::try_from_fields(fields, Default::default(), new_cal) {
            Ok(result) => {
                if overflow == "reject" && result.day_of_month().0 != d {
                    return None;
                }
                result
            }
            Err(_) => {
                if overflow == "reject" {
                    return None;
                }
                // Constrain day
                let new_cal2 = AnyCalendar::new(kind);
                let mut fields2 = IcuDateFields::default();
                fields2.extended_year = Some(new_y);
                fields2.ordinal_month = Some(new_mo);
                fields2.day = Some(1);
                let temp = IcuDate::try_from_fields(fields2, Default::default(), new_cal2).ok()?;
                let max_d = temp.days_in_month();
                let clamped_d = d.min(max_d);
                let new_cal3 = AnyCalendar::new(kind);
                let mut fields3 = IcuDateFields::default();
                fields3.extended_year = Some(new_y);
                fields3.ordinal_month = Some(new_mo);
                fields3.day = Some(clamped_d);
                IcuDate::try_from_fields(fields3, Default::default(), new_cal3).ok()?
            }
        };
    }

    // Step 3: Convert back to ISO and add weeks/days
    let iso_result = cal_date.to_iso();
    let mut ry = iso_result.year().extended_year();
    let mut rm = iso_result.month().ordinal;
    let mut rd = iso_result.day_of_month().0;

    let total_days = days + weeks * 7;
    if total_days != 0 {
        let epoch = iso_date_to_epoch_days(ry, rm, rd) + total_days as i64;
        let (ny, nm, nd) = epoch_days_to_iso_date(epoch);
        ry = ny;
        rm = nm;
        rd = nd;
    }

    Some((ry, rm, rd))
}

/// Compute the difference between two ISO dates in calendar-relative units.
/// Returns (years, months, weeks, days).
pub(crate) fn difference_calendar_date(
    iso_y1: i32,
    iso_m1: u8,
    iso_d1: u8,
    iso_y2: i32,
    iso_m2: u8,
    iso_d2: u8,
    largest_unit: &str,
    calendar_id: &str,
) -> Option<(i32, i32, i32, i32)> {
    let kind = calendar_id_to_icu_kind(calendar_id)?;

    let cal1 = AnyCalendar::new(kind);
    let cal2 = AnyCalendar::new(kind);
    let date1_iso = IcuDate::try_new_iso(iso_y1, iso_m1, iso_d1).ok()?;
    let date2_iso = IcuDate::try_new_iso(iso_y2, iso_m2, iso_d2).ok()?;
    let d1 = date1_iso.to_any().to_calendar(cal1);
    let d2 = date2_iso.to_any().to_calendar(cal2);

    let y1 = d1.year().extended_year();
    let mc1 = d1.month().standard_code.0.to_string();
    let day1 = d1.day_of_month().0;
    let y2 = d2.year().extended_year();
    let mc2 = d2.month().standard_code.0.to_string();
    let m2_ord = d2.month().ordinal;
    let day2 = d2.day_of_month().0;

    // Helper: check if (year, month_ordinal, day) surpasses target in direction sign
    // Uses lexicographic comparison on calendar coordinates
    let compare_surpasses =
        |sign: i32, year: i32, month: u8, day: u8, ty: i32, tm: u8, td: u8| -> bool {
            if year != ty {
                return sign * (year - ty) > 0;
            }
            if month != tm {
                return sign * (month as i32 - tm as i32) > 0;
            }
            if day != td {
                return sign * (day as i32 - td as i32) > 0;
            }
            false
        };

    // Helper: get calendar coordinates (year, ordinal_month, day) from ISO date
    let iso_to_cal = |iy: i32, im: u8, id: u8| -> Option<(i32, u8, u8)> {
        let iso_d = IcuDate::try_new_iso(iy, im, id).ok()?;
        let cal = AnyCalendar::new(kind);
        let cd = iso_d.to_any().to_calendar(cal);
        Some((
            cd.year().extended_year(),
            cd.month().ordinal,
            cd.day_of_month().0,
        ))
    };

    let epoch1 = iso_date_to_epoch_days(iso_y1, iso_m1, iso_d1);
    let epoch2 = iso_date_to_epoch_days(iso_y2, iso_m2, iso_d2);
    let sign: i32 = if epoch2 > epoch1 {
        1
    } else if epoch2 < epoch1 {
        -1
    } else {
        return Some((0, 0, 0, 0));
    };

    match largest_unit {
        "year" => {
            // Compute year difference using month code comparison
            let diff_years = y2 - y1;
            let mut years = if diff_years != 0 {
                let diff_in_year_sign = if mc2 > mc1 {
                    1
                } else if mc2 < mc1 {
                    -1
                } else {
                    (day2 as i32 - day1 as i32).signum()
                };
                let is_one_further = diff_in_year_sign * sign < 0;
                if is_one_further {
                    diff_years - sign
                } else {
                    diff_years
                }
            } else {
                0
            };

            // Verify year estimate with CompareSurpasses using original day
            if years != 0 {
                let inter = add_calendar_date(
                    iso_y1,
                    iso_m1,
                    iso_d1,
                    years,
                    0,
                    0,
                    0,
                    calendar_id,
                    "constrain",
                )?;
                let (iy, im, _id) = iso_to_cal(inter.0, inter.1, inter.2)?;
                if compare_surpasses(sign, iy, im, day1, y2, m2_ord, day2) {
                    years -= sign;
                }
            }

            // Count months using CompareSurpasses with original day
            let mut months = 0i32;
            loop {
                let next = add_calendar_date(
                    iso_y1,
                    iso_m1,
                    iso_d1,
                    years,
                    months + sign,
                    0,
                    0,
                    calendar_id,
                    "constrain",
                )?;
                let (ny, nm, _nd) = iso_to_cal(next.0, next.1, next.2)?;
                if compare_surpasses(sign, ny, nm, day1, y2, m2_ord, day2) {
                    break;
                }
                months += sign;
            }

            // Remaining days via epoch
            let final_date = add_calendar_date(
                iso_y1,
                iso_m1,
                iso_d1,
                years,
                months,
                0,
                0,
                calendar_id,
                "constrain",
            )?;
            let epoch_final = iso_date_to_epoch_days(final_date.0, final_date.1, final_date.2);
            let days = (epoch2 - epoch_final) as i32;

            Some((years, months, 0, days))
        }
        "month" => {
            // Count months using CompareSurpasses with original day
            let mut months = 0i32;
            loop {
                let next = add_calendar_date(
                    iso_y1,
                    iso_m1,
                    iso_d1,
                    0,
                    months + sign,
                    0,
                    0,
                    calendar_id,
                    "constrain",
                )?;
                let (ny, nm, _nd) = iso_to_cal(next.0, next.1, next.2)?;
                if compare_surpasses(sign, ny, nm, day1, y2, m2_ord, day2) {
                    break;
                }
                months += sign;
            }

            let final_date = add_calendar_date(
                iso_y1,
                iso_m1,
                iso_d1,
                0,
                months,
                0,
                0,
                calendar_id,
                "constrain",
            )?;
            let epoch_final = iso_date_to_epoch_days(final_date.0, final_date.1, final_date.2);
            let days = (epoch2 - epoch_final) as i32;

            Some((0, months, 0, days))
        }
        _ => {
            // For week/day largest unit, just use ISO difference
            let total_days = (epoch2 - epoch1) as i32;
            if largest_unit == "week" {
                let weeks = total_days / 7;
                let days = total_days % 7;
                Some((0, 0, weeks, days))
            } else {
                Some((0, 0, 0, total_days))
            }
        }
    }
}

/// Per spec ToTemporalCalendarSlotValue:
/// - If value is a Temporal object with a calendar slot, extract it
/// - If value is a string, validate as calendar identifier
/// - Otherwise throw TypeError
pub(crate) fn to_temporal_calendar_slot_value(
    interp: &mut Interpreter,
    val: &JsValue,
) -> Result<String, Completion> {
    match val {
        JsValue::Undefined => Ok("iso8601".to_string()),
        JsValue::Object(o) => {
            if let Some(obj) = interp.get_object(o.id) {
                let data = obj.borrow();
                match &data.temporal_data {
                    Some(TemporalData::PlainDate { calendar, .. })
                    | Some(TemporalData::PlainDateTime { calendar, .. })
                    | Some(TemporalData::PlainYearMonth { calendar, .. })
                    | Some(TemporalData::PlainMonthDay { calendar, .. })
                    | Some(TemporalData::ZonedDateTime { calendar, .. }) => {
                        return Ok(calendar.clone());
                    }
                    _ => {}
                }
            }
            Err(Completion::Throw(interp.create_type_error(
                "Invalid calendar value: expected a string or Temporal object",
            )))
        }
        JsValue::String(s) => {
            let raw = s.to_rust_string();
            match validate_calendar(&raw) {
                Some(c) => Ok(c),
                None => Err(Completion::Throw(
                    interp.create_range_error(&format!("Invalid calendar: {raw}")),
                )),
            }
        }
        _ => Err(Completion::Throw(interp.create_type_error(
            "Invalid calendar value: expected a string or Temporal object",
        ))),
    }
}

// Helper: get a property from a JsValue::Object, returning Completion
pub(crate) fn get_prop(interp: &mut Interpreter, obj: &JsValue, key: &str) -> Completion {
    match obj {
        JsValue::Object(o) => interp.get_object_property(o.id, key, obj),
        _ => Completion::Normal(JsValue::Undefined),
    }
}

// Helper: check if JsValue is undefined
pub(crate) fn is_undefined(v: &JsValue) -> bool {
    matches!(v, JsValue::Undefined)
}
/// Check if monthCode string has valid syntax: /^M\d{2}L?$/
/// Returns true if syntax is valid (even if the value is not valid for ISO 8601).
pub(crate) fn is_month_code_syntax_valid(mc: &str) -> bool {
    let b = mc.as_bytes();
    if b.len() < 3 || b.len() > 4 {
        return false;
    }
    if b[0] != b'M' {
        return false;
    }
    if !b[1].is_ascii_digit() || !b[2].is_ascii_digit() {
        return false;
    }
    if b.len() == 4 && b[3] != b'L' {
        return false;
    }
    true
}

/// Spec: IsPartialTemporalObject(value)
/// Returns Ok(()) if valid partial temporal object, Err(Completion) otherwise.
/// Rejects: non-objects, Temporal objects, objects with calendar/timeZone properties.
pub(crate) fn is_partial_temporal_object(
    interp: &mut Interpreter,
    value: &JsValue,
) -> Result<(), Completion> {
    let obj_ref = match value {
        JsValue::Object(o) => o,
        _ => {
            return Err(Completion::Throw(
                interp.create_type_error("with requires an object argument"),
            ));
        }
    };

    if let Some(obj) = interp.get_object(obj_ref.id) {
        let td = obj.borrow().temporal_data.clone();
        if let Some(
            TemporalData::PlainDate { .. }
            | TemporalData::PlainDateTime { .. }
            | TemporalData::PlainTime { .. }
            | TemporalData::PlainMonthDay { .. }
            | TemporalData::PlainYearMonth { .. }
            | TemporalData::ZonedDateTime { .. },
        ) = td
        {
            return Err(Completion::Throw(interp.create_type_error(
                "a Temporal object is not allowed as argument to with()",
            )));
        }
    }

    let cal_val = match get_prop(interp, value, "calendar") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    if !is_undefined(&cal_val) {
        return Err(Completion::Throw(interp.create_type_error(
            "calendar property not allowed in with() argument",
        )));
    }

    let tz_val = match get_prop(interp, value, "timeZone") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    if !is_undefined(&tz_val) {
        return Err(Completion::Throw(interp.create_type_error(
            "timeZone property not allowed in with() argument",
        )));
    }

    Ok(())
}

/// Read a date-like field as i32, returning (value, was_present).
/// Uses ToIntegerWithTruncation.
pub(crate) fn read_field_i32(
    interp: &mut Interpreter,
    obj: &JsValue,
    key: &str,
    default: i32,
) -> Result<(i32, bool), Completion> {
    let val = match get_prop(interp, obj, key) {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    if is_undefined(&val) {
        Ok((default, false))
    } else {
        let n = to_integer_with_truncation(interp, &val)?;
        Ok((n as i32, true))
    }
}

/// Read a date-like field as positive integer (month, day).
/// Uses ToPositiveIntegerWithTruncation: RangeError if <= 0.
/// Returns (value, was_present).
pub(crate) fn read_field_positive_int(
    interp: &mut Interpreter,
    obj: &JsValue,
    key: &str,
    default: u8,
) -> Result<(u8, bool), Completion> {
    let val = match get_prop(interp, obj, key) {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    if is_undefined(&val) {
        Ok((default, false))
    } else {
        let n = to_integer_with_truncation(interp, &val)?;
        if n < 1.0 {
            return Err(Completion::Throw(
                interp.create_range_error(&format!("{key} must be a positive integer")),
            ));
        }
        Ok((n as u8, true))
    }
}

/// Read a time-like field (hour, minute, second, etc) for with().
/// Returns (value, was_present). Uses ToIntegerWithTruncation.
pub(crate) fn read_time_field_new(
    interp: &mut Interpreter,
    obj: &JsValue,
    key: &str,
    default: f64,
) -> Result<(f64, bool), Completion> {
    let val = match get_prop(interp, obj, key) {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    if is_undefined(&val) {
        Ok((default, false))
    } else {
        let n = to_integer_with_truncation(interp, &val)?;
        Ok((n, true))
    }
}

/// Read monthCode field: returns (Option<String>, was_present).
pub(crate) fn read_month_code_field(
    interp: &mut Interpreter,
    obj: &JsValue,
) -> Result<(Option<String>, bool), Completion> {
    let mc_val = match get_prop(interp, obj, "monthCode") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    if is_undefined(&mc_val) {
        Ok((None, false))
    } else {
        let s = to_primitive_and_require_string(interp, &mc_val, "monthCode")?;
        Ok((Some(s), true))
    }
}

/// Per spec GetOptionsObject: if undefined return empty object, if object return it, else TypeError.
/// Returns Ok(true) if real options object, Ok(false) if undefined (use defaults).
pub(crate) fn get_options_object(
    interp: &mut Interpreter,
    options: &JsValue,
) -> Result<bool, Completion> {
    if matches!(options, JsValue::Undefined) {
        return Ok(false);
    }
    if matches!(options, JsValue::Object(_)) {
        return Ok(true);
    }
    Err(Completion::Throw(interp.create_type_error(
        "Options must be an object or undefined",
    )))
}

/// Maximum rounding increment for a given unit (for since/until / round)
pub(crate) fn max_rounding_increment(unit: &str) -> Option<u64> {
    match unit {
        "hour" | "hours" => Some(24),
        "minute" | "minutes" | "second" | "seconds" => Some(60),
        "millisecond" | "milliseconds" | "microsecond" | "microseconds" | "nanosecond"
        | "nanoseconds" => Some(1000),
        "day" | "days" => None, // no maximum for days in since/until
        _ => None,
    }
}

/// Validate rounding increment: truncate to integer, check range, check divisibility.
/// Coerce and validate a rounding increment value.
/// For since/until and PlainTime/PlainDateTime.round: uses max_rounding_increment (exclusive).
/// `is_difference`: true for since/until, false for round.
#[allow(dead_code)]
pub(crate) fn validate_rounding_increment(
    interp: &mut Interpreter,
    inc_val: &JsValue,
    unit: &str,
    is_difference: bool,
) -> Result<f64, Completion> {
    let int_inc = coerce_rounding_increment(interp, inc_val)?;
    // Check max_rounding_increment: increment < max AND max % increment == 0
    if let Some(max) = max_rounding_increment(unit) {
        let i = int_inc as u64;
        if i >= max {
            return Err(Completion::Throw(interp.create_range_error(&format!(
                "roundingIncrement {int_inc} is out of range for {unit}"
            ))));
        }
        if max % i != 0 {
            return Err(Completion::Throw(interp.create_range_error(&format!(
                "roundingIncrement {int_inc} does not divide evenly into {max}"
            ))));
        }
    } else if !is_difference {
        check_day_divisibility(interp, int_inc, unit)?;
    }
    Ok(int_inc)
}

/// For Instant.round: only requires increment to divide evenly into a solar day.
#[allow(dead_code)]
pub(crate) fn validate_rounding_increment_day_divisible(
    interp: &mut Interpreter,
    inc_val: &JsValue,
    unit: &str,
) -> Result<f64, Completion> {
    let int_inc = coerce_rounding_increment(interp, inc_val)?;
    check_day_divisibility(interp, int_inc, unit)?;
    Ok(int_inc)
}

pub(crate) fn coerce_rounding_increment(
    interp: &mut Interpreter,
    inc_val: &JsValue,
) -> Result<f64, Completion> {
    if is_undefined(inc_val) {
        return Ok(1.0);
    }
    let n = match interp.to_number_value(inc_val) {
        Ok(v) => v,
        Err(e) => return Err(Completion::Throw(e)),
    };
    if !n.is_finite() {
        return Err(Completion::Throw(
            interp.create_range_error("roundingIncrement must be finite"),
        ));
    }
    let int_inc = n.trunc();
    if !(1.0..=1e9).contains(&int_inc) {
        return Err(Completion::Throw(
            interp.create_range_error("roundingIncrement is out of range"),
        ));
    }
    Ok(int_inc)
}

#[allow(dead_code)]
fn check_day_divisibility(
    interp: &mut Interpreter,
    int_inc: f64,
    unit: &str,
) -> Result<(), Completion> {
    let unit_ns = temporal_unit_length_ns(unit) as u64;
    if unit_ns > 0 {
        let total_ns = int_inc as u64 * unit_ns;
        let day_ns: u64 = 86_400_000_000_000;
        if !day_ns.is_multiple_of(total_ns) {
            return Err(Completion::Throw(interp.create_range_error(&format!(
                "roundingIncrement {int_inc} for {unit} does not divide evenly into a day"
            ))));
        }
    }
    Ok(())
}

/// Validate a pre-coerced rounding increment value against unit constraints.
/// Returns Ok(inc) or Err(error_message).
pub(crate) fn validate_rounding_increment_raw(
    int_inc: f64,
    unit: &str,
    is_difference: bool,
) -> Result<f64, String> {
    if let Some(max) = max_rounding_increment(unit) {
        let i = int_inc as u64;
        if i >= max {
            return Err(format!(
                "roundingIncrement {int_inc} is out of range for {unit}"
            ));
        }
        if max % i != 0 {
            return Err(format!(
                "roundingIncrement {int_inc} does not divide evenly into {max}"
            ));
        }
    } else if !is_difference {
        let unit_ns = temporal_unit_length_ns(unit) as u64;
        if unit_ns > 0 {
            let total_ns = int_inc as u64 * unit_ns;
            let day_ns: u64 = 86_400_000_000_000;
            if !day_ns.is_multiple_of(total_ns) {
                return Err(format!(
                    "roundingIncrement {int_inc} for {unit} does not divide evenly into a day"
                ));
            }
        }
    }
    Ok(int_inc)
}

/// Parse the overflow option from an options bag. Returns "constrain" or "reject".
pub(crate) fn parse_overflow_option(
    interp: &mut Interpreter,
    options: &JsValue,
) -> Result<String, Completion> {
    let has_options = get_options_object(interp, options)?;
    if !has_options {
        return Ok("constrain".to_string());
    }
    let val = match get_prop(interp, options, "overflow") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    if is_undefined(&val) {
        return Ok("constrain".to_string());
    }
    let s = match interp.to_string_value(&val) {
        Ok(v) => v,
        Err(e) => return Err(Completion::Throw(e)),
    };
    match s.as_str() {
        "constrain" | "reject" => Ok(s),
        _ => Err(Completion::Throw(interp.create_range_error(&format!(
            "{s} is not a valid value for overflow"
        )))),
    }
}

impl Interpreter {
    pub(crate) fn setup_temporal(&mut self) {
        let temporal_obj = self.create_object();
        let temporal_id = temporal_obj.borrow().id.unwrap();

        // @@toStringTag = "Temporal"
        {
            let key = "Symbol(Symbol.toStringTag)".to_string();
            let desc = PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("Temporal"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            };
            temporal_obj.borrow_mut().property_order.push(key.clone());
            temporal_obj.borrow_mut().properties.insert(key, desc);
        }

        self.setup_temporal_duration(&temporal_obj);
        self.setup_temporal_instant(&temporal_obj);
        self.setup_temporal_plain_time(&temporal_obj);
        self.setup_temporal_plain_date(&temporal_obj);
        self.setup_temporal_plain_date_time(&temporal_obj);
        self.setup_temporal_plain_year_month(&temporal_obj);
        self.setup_temporal_plain_month_day(&temporal_obj);
        self.setup_temporal_zoned_date_time(&temporal_obj);
        self.setup_temporal_now(&temporal_obj);

        // Register Temporal as global (writable, not enumerable, configurable)
        let temporal_val = JsValue::Object(crate::types::JsObject { id: temporal_id });
        self.realm().global_env
            .borrow_mut()
            .declare("Temporal", BindingKind::Var);
        let _ = self.realm().global_env.borrow_mut().set("Temporal", temporal_val);
    }
}

// --- ISO 8601 calendar utilities ---

pub(crate) fn iso_is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub(crate) fn iso_days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if iso_is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 31,
    }
}

pub(crate) fn iso_days_in_year(year: i32) -> u16 {
    if iso_is_leap_year(year) { 366 } else { 365 }
}

pub(crate) fn iso_date_valid(year: i32, month: u8, day: u8) -> bool {
    if !(1..=12).contains(&month) {
        return false;
    }
    if day < 1 || day > iso_days_in_month(year, month) {
        return false;
    }
    // ISO year range: -271821 to 275760 (per spec)
    if !(-271821..=275760).contains(&year) {
        return false;
    }
    true
}

pub(crate) fn iso_time_valid(
    hour: u8,
    minute: u8,
    second: u8,
    millisecond: u16,
    microsecond: u16,
    nanosecond: u16,
) -> bool {
    hour < 24
        && minute < 60
        && second < 60
        && millisecond < 1000
        && microsecond < 1000
        && nanosecond < 1000
}

/// Validate time fields as f64 (before truncation to u8/u16).
/// Needed for reject mode where negative values must be caught.
pub(crate) fn iso_time_valid_f64(
    hour: f64,
    minute: f64,
    second: f64,
    millisecond: f64,
    microsecond: f64,
    nanosecond: f64,
) -> bool {
    (0.0..=23.0).contains(&hour)
        && (0.0..=59.0).contains(&minute)
        && (0.0..=59.0).contains(&second)
        && (0.0..=999.0).contains(&millisecond)
        && (0.0..=999.0).contains(&microsecond)
        && (0.0..=999.0).contains(&nanosecond)
}

pub(crate) fn iso_day_of_year(year: i32, month: u8, day: u8) -> u16 {
    let month_days: [u16; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let mut doy = month_days[(month - 1) as usize] + day as u16;
    if month > 2 && iso_is_leap_year(year) {
        doy += 1;
    }
    doy
}

// ISO day of week: 1=Monday, 7=Sunday (per spec)
pub(crate) fn iso_day_of_week(year: i32, month: u8, day: u8) -> u8 {
    // Use Julian Day Number for correct results across all years including negative
    let y = year as i64;
    let m = month as i64;
    let d = day as i64;
    let a = (14 - m) / 12;
    let y2 = y + 4800 - a;
    let m2 = m + 12 * a - 3;
    let jdn = d + (153 * m2 + 2) / 5 + 365 * y2 + y2 / 4 - y2 / 100 + y2 / 400 - 32045;
    // JDN % 7: 0=Monday, 1=Tuesday, ..., 6=Sunday
    let dow = jdn.rem_euclid(7) + 1; // 1=Monday, ..., 7=Sunday
    dow as u8
}

// ISO week of year (ISO 8601 week date): returns (week, yearOfWeek)
pub(crate) fn iso_week_of_year(year: i32, month: u8, day: u8) -> (u8, i32) {
    let doy = iso_day_of_year(year, month, day) as i32;
    let dow = iso_day_of_week(year, month, day) as i32;
    // ISO week number: week starts on Monday
    let woy = (doy - dow + 10) / 7;
    if woy < 1 {
        // Belongs to last week of previous year
        let prev_dec31_dow = iso_day_of_week(year - 1, 12, 31) as i32;
        let prev_weeks =
            if prev_dec31_dow == 4 || (iso_is_leap_year(year - 1) && prev_dec31_dow == 5) {
                53
            } else {
                52
            };
        (prev_weeks as u8, year - 1)
    } else if woy > 52 {
        let dec31_dow = iso_day_of_week(year, 12, 31) as i32;
        let max_weeks = if dec31_dow == 4 || (iso_is_leap_year(year) && dec31_dow == 5) {
            53
        } else {
            52
        };
        if woy > max_weeks {
            (1, year + 1)
        } else {
            (woy as u8, year)
        }
    } else {
        (woy as u8, year)
    }
}

#[allow(dead_code)]
pub(crate) fn balance_time(
    hour: i64,
    minute: i64,
    second: i64,
    millisecond: i64,
    microsecond: i64,
    nanosecond: i64,
) -> (i64, u8, u8, u8, u16, u16, u16) {
    let us = microsecond + nanosecond.div_euclid(1000);
    let ns = nanosecond.rem_euclid(1000) as u16;
    let ms = millisecond + us.div_euclid(1000);
    let us_out = us.rem_euclid(1000) as u16;
    let s = second + ms.div_euclid(1000);
    let ms_out = ms.rem_euclid(1000) as u16;
    let m = minute + s.div_euclid(60);
    let s_out = s.rem_euclid(60) as u8;
    let h = hour + m.div_euclid(60);
    let m_out = m.rem_euclid(60) as u8;
    let days = h.div_euclid(24);
    let h_out = h.rem_euclid(24) as u8;
    (days, h_out, m_out, s_out, ms_out, us_out, ns)
}

pub(crate) fn balance_iso_date(year: i32, month: i32, day: i32) -> (i32, u8, u8) {
    // First balance month into [1..12]
    let mut y = year + (month - 1).div_euclid(12);
    let mut m = ((month - 1).rem_euclid(12) + 1) as u8;
    let mut d = day;

    loop {
        let dim = iso_days_in_month(y, m) as i32;
        if d <= dim && d >= 1 {
            break;
        }
        if d > dim {
            d -= dim;
            m += 1;
            if m > 12 {
                m = 1;
                y += 1;
            }
        } else {
            // d < 1
            m -= 1;
            if m < 1 {
                m = 12;
                y -= 1;
            }
            d += iso_days_in_month(y, m) as i32;
        }
    }
    (y, m, d as u8)
}

pub(crate) fn add_iso_date(
    year: i32,
    month: u8,
    day: u8,
    years: i32,
    months: i32,
    weeks: i32,
    days: i32,
) -> (i32, u8, u8) {
    add_iso_date_with_overflow(year, month, day, years, months, weeks, days, "constrain").unwrap()
}

/// AddISODate with overflow handling. Returns Err(()) for reject when day > days-in-month.
pub(crate) fn add_iso_date_with_overflow(
    year: i32,
    month: u8,
    day: u8,
    years: i32,
    months: i32,
    weeks: i32,
    days: i32,
    overflow: &str,
) -> Result<(i32, u8, u8), ()> {
    let mut y = year + years;
    let mut m = month as i32 + months;
    y += (m - 1).div_euclid(12);
    m = (m - 1).rem_euclid(12) + 1;
    let mu = m as u8;
    let dim = iso_days_in_month(y, mu);
    if overflow == "reject" && day > dim {
        return Err(());
    }
    let d = day.min(dim) as i32;
    let total_days = d + weeks * 7 + days;
    Ok(balance_iso_date(y, mu as i32, total_days))
}

/// ISODateSurpasses: checks if adding years+months to baseDate surpasses target.
/// Uses the ORIGINAL unclamped day from baseDate for comparison (spec key insight).
fn iso_date_surpasses(
    sign: i32,
    base_y: i32,
    base_m: u8,
    base_d: u8,
    target_y: i32,
    target_m: u8,
    target_d: u8,
    years: i32,
    months: i32,
) -> bool {
    let y0 = base_y + years;
    if compare_surpasses(
        sign,
        y0,
        base_m as i32,
        base_d as i32,
        target_y,
        target_m as i32,
        target_d as i32,
    ) {
        return true;
    }
    if months == 0 {
        return false;
    }
    let m0 = base_m as i32 + months;
    let bal_y = y0 + (m0 - 1).div_euclid(12);
    let bal_m = (m0 - 1).rem_euclid(12) + 1;
    compare_surpasses(
        sign,
        bal_y,
        bal_m,
        base_d as i32,
        target_y,
        target_m as i32,
        target_d as i32,
    )
}

fn compare_surpasses(
    sign: i32,
    year: i32,
    month: i32,
    day: i32,
    ty: i32,
    tm: i32,
    td: i32,
) -> bool {
    // ConstrainISODate: clamp day to max days in month (e.g., Feb 29 → Feb 28 in non-leap year)
    let clamped_day = day.min(iso_days_in_month(year, month as u8) as i32);
    if year != ty {
        return sign * (year - ty) > 0;
    }
    if month != tm {
        return sign * (month - tm) > 0;
    }
    if clamped_day != td {
        return sign * (clamped_day - td) > 0;
    }
    false
}

/// Asymmetric date difference per spec: computes from date1's perspective.
/// date1 is the reference (receiver). Result is signed.
pub(crate) fn difference_iso_date(
    y1: i32,
    m1: u8,
    d1: u8,
    y2: i32,
    m2: u8,
    d2: u8,
    largest_unit: &str,
) -> (i32, i32, i32, i32) {
    let sign = if (y1, m1, d1) < (y2, m2, d2) {
        1
    } else if (y1, m1, d1) > (y2, m2, d2) {
        -1
    } else {
        return (0, 0, 0, 0);
    };

    match largest_unit {
        "year" | "years" | "month" | "months" => {
            let mut years = 0i32;
            let mut months = 0i32;

            if matches!(largest_unit, "year" | "years" | "month" | "months") {
                // Find years
                let mut candidate_years = y2 - y1;
                if candidate_years != 0 {
                    candidate_years -= sign;
                }
                while !iso_date_surpasses(sign, y1, m1, d1, y2, m2, d2, candidate_years, 0) {
                    years = candidate_years;
                    candidate_years += sign;
                }

                // Find months
                let mut candidate_months = sign;
                while !iso_date_surpasses(sign, y1, m1, d1, y2, m2, d2, years, candidate_months) {
                    months = candidate_months;
                    candidate_months += sign;
                }

                if matches!(largest_unit, "month" | "months") {
                    months += years * 12;
                    years = 0;
                }
            }

            // Compute intermediate: constrain day to fit result month
            let int_y = y1 + years;
            let int_m0 = m1 as i32 + months;
            let bal_y = int_y + (int_m0 - 1).div_euclid(12);
            let bal_m = ((int_m0 - 1).rem_euclid(12) + 1) as u8;
            let int_d = d1.min(iso_days_in_month(bal_y, bal_m));
            let days = (iso_date_to_epoch_days(y2, m2, d2)
                - iso_date_to_epoch_days(bal_y, bal_m, int_d)) as i32;
            (years, months, 0, days)
        }
        "week" | "weeks" => {
            let total = iso_date_to_epoch_days(y2, m2, d2) - iso_date_to_epoch_days(y1, m1, d1);
            let weeks = if total >= 0 { total / 7 } else { -(-total / 7) } as i32;
            let days = (total - weeks as i64 * 7) as i32;
            (0, 0, weeks, days)
        }
        _ => {
            let total = iso_date_to_epoch_days(y2, m2, d2) - iso_date_to_epoch_days(y1, m1, d1);
            (0, 0, 0, total as i32)
        }
    }
}

/// nsMaxInstant = 8.64e21 ns (= 1e8 days)
const NS_MAX_INSTANT: i128 = 8_640_000_000_000_000_000_000;
/// nsPerDay = 8.64e13 ns
const NS_PER_DAY: i128 = 86_400_000_000_000;

/// ISODateWithinLimits: uses noon for the range check.
/// Per spec: combine with NoonTimeRecord, then check ISODateTimeWithinLimits.
pub(crate) fn iso_date_within_limits(year: i32, month: u8, day: u8) -> bool {
    iso_date_time_within_limits(year, month, day, 12, 0, 0, 0, 0, 0)
}

/// ISODateTimeWithinLimits per spec:
/// 1. Quick day check: abs(epoch_days) > 10^8 + 1 → false
/// 2. NS check: nsMinInstant - nsPerDay < ns < nsMaxInstant + nsPerDay (strict)
pub(crate) fn iso_date_time_within_limits(
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    ms: u16,
    us: u16,
    ns: u16,
) -> bool {
    let epoch_days = iso_date_to_epoch_days(year, month, day);
    if epoch_days.abs() > 100_000_001 {
        return false;
    }
    let day_ns: i128 = epoch_days as i128 * NS_PER_DAY;
    let time_ns: i128 = hour as i128 * 3_600_000_000_000
        + minute as i128 * 60_000_000_000
        + second as i128 * 1_000_000_000
        + ms as i128 * 1_000_000
        + us as i128 * 1_000
        + ns as i128;
    let total_ns = day_ns + time_ns;
    total_ns > -NS_MAX_INSTANT - NS_PER_DAY && total_ns < NS_MAX_INSTANT + NS_PER_DAY
}

/// ISOYearMonthWithinLimits per spec:
/// Simple hardcoded year/month boundary check.
pub(crate) fn iso_year_month_within_limits(year: i32, month: u8) -> bool {
    if !(-271821..=275760).contains(&year) {
        return false;
    }
    if year == -271821 && month < 4 {
        return false;
    }
    if year == 275760 && month > 9 {
        return false;
    }
    true
}

pub(crate) fn iso_date_to_epoch_days(year: i32, month: u8, day: u8) -> i64 {
    // Howard Hinnant's civil calendar algorithm (inverse of epoch_days_to_iso_date)
    let y = year as i64 - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let m = month as i64;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + day as i64 - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

pub(crate) fn epoch_days_to_iso_date(epoch_days: i64) -> (i32, u8, u8) {
    // Convert epoch days (since 1970-01-01) to (year, month, day)
    // Shift to epoch starting from 0000-03-01 for simpler leap year handling
    let z = epoch_days + 719468; // days since 0000-03-01
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year as i32, m as u8, d as u8)
}

// --- ISO 8601 Duration string parser ---
// Format: PnYnMnWnDTnHnMnS (each component optional)

pub(crate) struct ParsedDuration {
    pub sign: f64,
    pub years: f64,
    pub months: f64,
    pub weeks: f64,
    pub days: f64,
    pub hours: f64,
    pub minutes: f64,
    pub seconds: f64,
    pub milliseconds: f64,
    pub microseconds: f64,
    pub nanoseconds: f64,
}

pub(crate) fn parse_temporal_duration_string(s: &str) -> Option<ParsedDuration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let bytes = s.as_bytes();
    let mut pos = 0;
    let sign = if bytes.get(pos) == Some(&b'-') || bytes.get(pos) == Some(&0xe2) {
        if bytes.get(pos) == Some(&b'-') {
            pos += 1;
            -1.0
        } else if bytes.len() >= pos + 3
            && bytes[pos] == 0xe2
            && bytes[pos + 1] == 0x88
            && bytes[pos + 2] == 0x92
        {
            // Unicode minus sign U+2212
            pos += 3;
            -1.0
        } else {
            1.0
        }
    } else if bytes.get(pos) == Some(&b'+') {
        pos += 1;
        1.0
    } else {
        1.0
    };

    if bytes.get(pos) != Some(&b'P') && bytes.get(pos) != Some(&b'p') {
        return None;
    }
    pos += 1;

    let mut years = 0.0;
    let mut months = 0.0;
    let mut weeks = 0.0;
    let mut days = 0.0;
    let mut hours = 0.0;
    let mut minutes = 0.0;
    let mut seconds = 0.0;
    let mut frac_seconds = 0.0;
    let mut has_t = false;
    let mut any_component = false;
    let mut last_time_unit = 0u8; // track ordering: H=1, M=2, S=3

    // Parse date components
    while pos < bytes.len() {
        if bytes[pos] == b'T' || bytes[pos] == b't' {
            has_t = true;
            pos += 1;
            break;
        }
        let (num, frac, new_pos) = parse_duration_number(bytes, pos)?;
        pos = new_pos;
        if pos >= bytes.len() {
            return None;
        }
        match bytes[pos] {
            b'Y' | b'y' => {
                if frac.is_some() {
                    return None;
                }
                years = num;
                any_component = true;
            }
            b'M' | b'm' => {
                if frac.is_some() {
                    return None;
                }
                months = num;
                any_component = true;
            }
            b'W' | b'w' => {
                if frac.is_some() {
                    return None;
                }
                weeks = num;
                any_component = true;
            }
            b'D' | b'd' => {
                if frac.is_some() {
                    return None;
                }
                days = num;
                any_component = true;
            }
            _ => return None,
        }
        pos += 1;
    }

    // Parse time components (after T)
    if has_t {
        if pos >= bytes.len() {
            return None; // T without any time component
        }
        let mut time_any = false;
        while pos < bytes.len() {
            let (num, frac, new_pos) = parse_duration_number(bytes, pos)?;
            pos = new_pos;
            if pos >= bytes.len() {
                return None;
            }
            match bytes[pos] {
                b'H' | b'h' => {
                    if last_time_unit >= 1 {
                        return None;
                    }
                    hours = num;
                    if let Some(f) = frac {
                        // Fractional hours -> minutes, seconds
                        let total_ns = f * 3_600_000_000_000.0;
                        let rem_minutes = total_ns / 60_000_000_000.0;
                        minutes = rem_minutes.trunc();
                        let rem_s = (total_ns - minutes * 60_000_000_000.0) / 1_000_000_000.0;
                        seconds = rem_s.trunc();
                        frac_seconds = rem_s - seconds;
                    }
                    last_time_unit = 1;
                    time_any = true;
                }
                b'M' | b'm' => {
                    if last_time_unit >= 2 {
                        return None;
                    }
                    minutes = num;
                    if let Some(f) = frac {
                        let total_ns = f * 60_000_000_000.0;
                        let rem_s = total_ns / 1_000_000_000.0;
                        seconds = rem_s.trunc();
                        frac_seconds = rem_s - seconds;
                    }
                    last_time_unit = 2;
                    time_any = true;
                }
                b'S' | b's' => {
                    if last_time_unit >= 3 {
                        return None;
                    }
                    seconds = num;
                    if let Some(f) = frac {
                        frac_seconds = f;
                    }
                    last_time_unit = 3;
                    time_any = true;
                }
                _ => return None,
            }
            pos += 1;
            if frac.is_some() {
                break; // Fractional component must be last
            }
        }
        if !time_any {
            return None;
        }
        any_component = true;
    }

    if pos != bytes.len() || !any_component {
        return None;
    }

    // Convert fractional seconds to ms/us/ns
    let mut ms = 0.0;
    let mut us = 0.0;
    let mut ns = 0.0;
    if frac_seconds > 0.0 {
        let total_ns = (frac_seconds * 1_000_000_000.0).round();
        ms = (total_ns / 1_000_000.0).trunc();
        let rem = total_ns - ms * 1_000_000.0;
        us = (rem / 1_000.0).trunc();
        ns = rem - us * 1_000.0;
    }

    Some(ParsedDuration {
        sign,
        years,
        months,
        weeks,
        days,
        hours,
        minutes,
        seconds,
        milliseconds: ms,
        microseconds: us,
        nanoseconds: ns,
    })
}

fn parse_duration_number(bytes: &[u8], start: usize) -> Option<(f64, Option<f64>, usize)> {
    let mut pos = start;
    if pos >= bytes.len() || !bytes[pos].is_ascii_digit() {
        return None;
    }
    let int_start = pos;
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        pos += 1;
    }
    let int_part: f64 = std::str::from_utf8(&bytes[int_start..pos])
        .ok()?
        .parse()
        .ok()?;

    if pos < bytes.len() && (bytes[pos] == b'.' || bytes[pos] == b',') {
        pos += 1;
        let frac_start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
        if pos == frac_start {
            return None; // decimal point with no digits
        }
        let frac_len = pos - frac_start;
        if frac_len > 9 {
            return None; // max 9 fractional digits per spec
        }
        let frac_str = std::str::from_utf8(&bytes[frac_start..pos]).ok()?;
        let frac_val: f64 = format!("0.{frac_str}").parse().ok()?;
        Some((int_part, Some(frac_val), pos))
    } else {
        Some((int_part, None, pos))
    }
}

// --- Timezone identifier validation ---

/// Parse a string as a UTC offset timezone identifier: ±HH:MM (no seconds).
/// Returns the normalized offset string if valid.
pub(crate) fn parse_utc_offset_timezone(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    if bytes.len() < 3 {
        return None;
    }
    let sign = match bytes[0] {
        b'+' => '+',
        b'-' => '-',
        _ => return None,
    };
    let start = 1;
    let rest = &bytes[start..];

    if rest.len() < 2 {
        return None;
    }
    let h0 = (rest[0] as char).to_digit(10)? as u8;
    let h1 = (rest[1] as char).to_digit(10)? as u8;
    let hours = h0 * 10 + h1;
    if hours > 23 {
        return None;
    }

    if rest.len() == 2 {
        return Some(format!("{}{:02}:00", sign, hours));
    }

    let has_sep = rest.len() > 2 && rest[2] == b':';
    let min_start = if has_sep { 3 } else { 2 };
    if rest.len() < min_start + 2 {
        if has_sep {
            return None;
        }
        return Some(format!("{}{:02}:00", sign, hours));
    }
    let m0 = (rest[min_start] as char).to_digit(10)? as u8;
    let m1 = (rest[min_start + 1] as char).to_digit(10)? as u8;
    let minutes = m0 * 10 + m1;
    if minutes > 59 {
        return None;
    }

    let after_min = min_start + 2;
    // Sub-minute precision → reject
    if rest.len() > after_min
        && (rest[after_min] == b':'
            || rest[after_min] == b'.'
            || rest[after_min] == b','
            || rest[after_min].is_ascii_digit())
    {
        return None;
    }

    Some(format!("{}{:02}:{:02}", sign, hours, minutes))
}

pub(super) fn parse_offset_string(s: &str) -> Option<ParsedOffset> {
    let bytes = s.as_bytes();
    let (offset, consumed) = parse_iso_offset(bytes, 0)?;
    if consumed != bytes.len() {
        return None;
    }
    Some(offset)
}

/// Convert a canonical offset string like "+01:00" or "-05:30" to nanoseconds.
pub(crate) fn offset_string_to_ns(s: &str) -> i128 {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return 0;
    }
    let sign: i128 = if bytes[0] == b'-' { -1 } else { 1 };
    let rest = &bytes[1..];
    let hours = if rest.len() >= 2 {
        (rest[0] - b'0') as i128 * 10 + (rest[1] - b'0') as i128
    } else {
        0
    };
    let minutes = if rest.len() >= 5 && rest[2] == b':' {
        (rest[3] - b'0') as i128 * 10 + (rest[4] - b'0') as i128
    } else {
        0
    };
    sign * (hours * 3_600_000_000_000 + minutes * 60_000_000_000)
}

/// Return the primary/canonical IANA timezone identifier for a given timezone name.
/// Link names are resolved to their primary zone (e.g., US/Pacific → America/Los_Angeles).
pub(crate) fn canonicalize_iana_tz(s: &str) -> String {
    static LINK_MAP: &[(&str, &str)] = &[
        // UTC aliases
        ("Etc/GMT", "UTC"),
        ("Etc/UTC", "UTC"),
        ("Etc/GMT+0", "UTC"),
        ("Etc/GMT-0", "UTC"),
        ("Etc/GMT0", "UTC"),
        ("Etc/Greenwich", "UTC"),
        ("Etc/UCT", "UTC"),
        ("Etc/Universal", "UTC"),
        ("Etc/Zulu", "UTC"),
        ("GMT", "UTC"),
        ("GMT+0", "UTC"),
        ("GMT-0", "UTC"),
        ("GMT0", "UTC"),
        ("Greenwich", "UTC"),
        ("UCT", "UTC"),
        ("Universal", "UTC"),
        ("Zulu", "UTC"),
        // Africa
        ("Africa/Asmera", "Africa/Asmara"),
        ("Africa/Timbuktu", "Africa/Bamako"),
        ("Africa/Bamako", "Africa/Abidjan"),
        ("Africa/Banjul", "Africa/Abidjan"),
        ("Africa/Conakry", "Africa/Abidjan"),
        ("Africa/Dakar", "Africa/Abidjan"),
        ("Africa/Freetown", "Africa/Abidjan"),
        ("Africa/Lome", "Africa/Abidjan"),
        ("Africa/Nouakchott", "Africa/Abidjan"),
        ("Africa/Ouagadougou", "Africa/Abidjan"),
        ("Atlantic/St_Helena", "Africa/Abidjan"),
        ("Africa/Asmara", "Africa/Nairobi"),
        ("Africa/Dar_es_Salaam", "Africa/Nairobi"),
        ("Africa/Djibouti", "Africa/Nairobi"),
        ("Africa/Kampala", "Africa/Nairobi"),
        ("Africa/Mogadishu", "Africa/Nairobi"),
        ("Indian/Antananarivo", "Africa/Nairobi"),
        ("Indian/Comoro", "Africa/Nairobi"),
        ("Indian/Mayotte", "Africa/Nairobi"),
        ("Africa/Blantyre", "Africa/Maputo"),
        ("Africa/Bujumbura", "Africa/Maputo"),
        ("Africa/Gaborone", "Africa/Maputo"),
        ("Africa/Harare", "Africa/Maputo"),
        ("Africa/Kigali", "Africa/Maputo"),
        ("Africa/Lubumbashi", "Africa/Maputo"),
        ("Africa/Lusaka", "Africa/Maputo"),
        ("Africa/Bangui", "Africa/Lagos"),
        ("Africa/Brazzaville", "Africa/Lagos"),
        ("Africa/Douala", "Africa/Lagos"),
        ("Africa/Kinshasa", "Africa/Lagos"),
        ("Africa/Libreville", "Africa/Lagos"),
        ("Africa/Luanda", "Africa/Lagos"),
        ("Africa/Malabo", "Africa/Lagos"),
        ("Africa/Niamey", "Africa/Lagos"),
        ("Africa/Porto-Novo", "Africa/Lagos"),
        ("Africa/Maseru", "Africa/Johannesburg"),
        ("Africa/Mbabane", "Africa/Johannesburg"),
        // Americas
        (
            "America/Argentina/ComodRivadavia",
            "America/Argentina/Catamarca",
        ),
        ("America/Atka", "America/Adak"),
        ("America/Buenos_Aires", "America/Argentina/Buenos_Aires"),
        ("America/Catamarca", "America/Argentina/Catamarca"),
        ("America/Coral_Harbour", "America/Atikokan"),
        ("America/Cordoba", "America/Argentina/Cordoba"),
        ("America/Ensenada", "America/Tijuana"),
        ("America/Fort_Wayne", "America/Indiana/Indianapolis"),
        ("America/Godthab", "America/Nuuk"),
        ("America/Indianapolis", "America/Indiana/Indianapolis"),
        ("America/Jujuy", "America/Argentina/Jujuy"),
        ("America/Knox_IN", "America/Indiana/Knox"),
        ("America/Louisville", "America/Kentucky/Louisville"),
        ("America/Mendoza", "America/Argentina/Mendoza"),
        ("America/Montreal", "America/Toronto"),
        ("America/Nipigon", "America/Toronto"),
        ("America/Pangnirtung", "America/Iqaluit"),
        ("America/Porto_Acre", "America/Rio_Branco"),
        ("America/Rainy_River", "America/Winnipeg"),
        ("America/Rosario", "America/Argentina/Cordoba"),
        ("America/Santa_Isabel", "America/Tijuana"),
        ("America/Shiprock", "America/Denver"),
        ("America/Thunder_Bay", "America/Toronto"),
        ("America/Virgin", "America/St_Thomas"),
        ("America/Yellowknife", "America/Edmonton"),
        ("America/Creston", "America/Phoenix"),
        ("America/Nassau", "America/Toronto"),
        ("America/Atikokan", "America/Panama"),
        ("America/Cayman", "America/Panama"),
        ("America/Anguilla", "America/Puerto_Rico"),
        ("America/Antigua", "America/Puerto_Rico"),
        ("America/Aruba", "America/Puerto_Rico"),
        ("America/Curacao", "America/Puerto_Rico"),
        ("America/Blanc-Sablon", "America/Puerto_Rico"),
        ("America/Dominica", "America/Puerto_Rico"),
        ("America/Grenada", "America/Puerto_Rico"),
        ("America/Guadeloupe", "America/Puerto_Rico"),
        ("America/Kralendijk", "America/Puerto_Rico"),
        ("America/Lower_Princes", "America/Puerto_Rico"),
        ("America/Marigot", "America/Puerto_Rico"),
        ("America/Montserrat", "America/Puerto_Rico"),
        ("America/Port_of_Spain", "America/Puerto_Rico"),
        ("America/St_Barthelemy", "America/Puerto_Rico"),
        ("America/St_Kitts", "America/Puerto_Rico"),
        ("America/St_Lucia", "America/Puerto_Rico"),
        ("America/St_Thomas", "America/Puerto_Rico"),
        ("America/St_Vincent", "America/Puerto_Rico"),
        ("America/Tortola", "America/Puerto_Rico"),
        // US
        ("US/Alaska", "America/Anchorage"),
        ("US/Aleutian", "America/Adak"),
        ("US/Arizona", "America/Phoenix"),
        ("US/Central", "America/Chicago"),
        ("US/East-Indiana", "America/Indiana/Indianapolis"),
        ("US/Eastern", "America/New_York"),
        ("US/Hawaii", "Pacific/Honolulu"),
        ("US/Indiana-Starke", "America/Indiana/Knox"),
        ("US/Michigan", "America/Detroit"),
        ("US/Mountain", "America/Denver"),
        ("US/Pacific", "America/Los_Angeles"),
        ("US/Samoa", "Pacific/Pago_Pago"),
        // Asia
        ("Asia/Ashkhabad", "Asia/Ashgabat"),
        ("Asia/Calcutta", "Asia/Kolkata"),
        ("Asia/Choibalsan", "Asia/Ulaanbaatar"),
        ("Asia/Chongqing", "Asia/Shanghai"),
        ("Asia/Chungking", "Asia/Shanghai"),
        ("Asia/Dacca", "Asia/Dhaka"),
        ("Asia/Harbin", "Asia/Shanghai"),
        ("Asia/Istanbul", "Europe/Istanbul"),
        ("Asia/Kashgar", "Asia/Urumqi"),
        ("Asia/Katmandu", "Asia/Kathmandu"),
        ("Asia/Macao", "Asia/Macau"),
        ("Asia/Rangoon", "Asia/Yangon"),
        ("Asia/Saigon", "Asia/Ho_Chi_Minh"),
        ("Asia/Tel_Aviv", "Asia/Jerusalem"),
        ("Asia/Thimbu", "Asia/Thimphu"),
        ("Asia/Ujung_Pandang", "Asia/Makassar"),
        ("Asia/Ulan_Bator", "Asia/Ulaanbaatar"),
        ("Antarctica/Syowa", "Asia/Riyadh"),
        ("Asia/Aden", "Asia/Riyadh"),
        ("Asia/Bahrain", "Asia/Qatar"),
        ("Asia/Kuwait", "Asia/Riyadh"),
        ("Asia/Phnom_Penh", "Asia/Bangkok"),
        ("Asia/Vientiane", "Asia/Bangkok"),
        ("Asia/Muscat", "Asia/Dubai"),
        ("Asia/Brunei", "Asia/Kuching"),
        // Europe
        ("Europe/Belfast", "Europe/London"),
        ("Europe/Kiev", "Europe/Kyiv"),
        ("Europe/Nicosia", "Asia/Nicosia"),
        ("Europe/Tiraspol", "Europe/Chisinau"),
        ("Europe/Uzhgorod", "Europe/Kyiv"),
        ("Europe/Zaporozhye", "Europe/Kyiv"),
        ("Europe/Jersey", "Europe/London"),
        ("Europe/Guernsey", "Europe/London"),
        ("Europe/Isle_of_Man", "Europe/London"),
        ("Europe/Mariehamn", "Europe/Helsinki"),
        ("Europe/Busingen", "Europe/Zurich"),
        ("Europe/Vatican", "Europe/Rome"),
        ("Europe/San_Marino", "Europe/Rome"),
        ("Europe/Vaduz", "Europe/Zurich"),
        ("Arctic/Longyearbyen", "Europe/Oslo"),
        ("Europe/Ljubljana", "Europe/Belgrade"),
        ("Europe/Podgorica", "Europe/Belgrade"),
        ("Europe/Sarajevo", "Europe/Belgrade"),
        ("Europe/Skopje", "Europe/Belgrade"),
        ("Europe/Zagreb", "Europe/Belgrade"),
        ("Europe/Bratislava", "Europe/Prague"),
        ("Europe/Copenhagen", "Europe/Berlin"),
        ("Europe/Luxembourg", "Europe/Brussels"),
        ("Europe/Monaco", "Europe/Paris"),
        // Australia
        ("Antarctica/South_Pole", "Antarctica/McMurdo"),
        ("Australia/ACT", "Australia/Sydney"),
        ("Australia/Canberra", "Australia/Sydney"),
        ("Australia/Currie", "Australia/Hobart"),
        ("Australia/LHI", "Australia/Lord_Howe"),
        ("Australia/NSW", "Australia/Sydney"),
        ("Australia/North", "Australia/Darwin"),
        ("Australia/Queensland", "Australia/Brisbane"),
        ("Australia/South", "Australia/Adelaide"),
        ("Australia/Tasmania", "Australia/Hobart"),
        ("Australia/Victoria", "Australia/Melbourne"),
        ("Australia/West", "Australia/Perth"),
        ("Australia/Yancowinna", "Australia/Broken_Hill"),
        ("Antarctica/McMurdo", "Pacific/Auckland"),
        ("Antarctica/DumontDUrville", "Pacific/Port_Moresby"),
        // Pacific
        ("Pacific/Enderbury", "Pacific/Kanton"),
        ("Pacific/Johnston", "Pacific/Honolulu"),
        ("Pacific/Ponape", "Pacific/Pohnpei"),
        ("Pacific/Samoa", "Pacific/Pago_Pago"),
        ("Pacific/Truk", "Pacific/Chuuk"),
        ("Pacific/Yap", "Pacific/Chuuk"),
        ("Pacific/Saipan", "Pacific/Guam"),
        ("Pacific/Midway", "Pacific/Pago_Pago"),
        ("Pacific/Funafuti", "Pacific/Tarawa"),
        ("Pacific/Majuro", "Pacific/Tarawa"),
        ("Pacific/Wake", "Pacific/Tarawa"),
        ("Pacific/Wallis", "Pacific/Tarawa"),
        // Brazil / Canada / Chile / Mexico
        ("Brazil/Acre", "America/Rio_Branco"),
        ("Brazil/DeNoronha", "America/Noronha"),
        ("Brazil/East", "America/Sao_Paulo"),
        ("Brazil/West", "America/Manaus"),
        ("Canada/Atlantic", "America/Halifax"),
        ("Canada/Central", "America/Winnipeg"),
        ("Canada/Eastern", "America/Toronto"),
        ("Canada/Mountain", "America/Edmonton"),
        ("Canada/Newfoundland", "America/St_Johns"),
        ("Canada/Pacific", "America/Vancouver"),
        ("Canada/Saskatchewan", "America/Regina"),
        ("Canada/Yukon", "America/Whitehorse"),
        ("Chile/Continental", "America/Santiago"),
        ("Chile/EasterIsland", "Pacific/Easter"),
        ("Mexico/BajaNorte", "America/Tijuana"),
        ("Mexico/BajaSur", "America/Mazatlan"),
        ("Mexico/General", "America/Mexico_City"),
        // Country names
        ("Cuba", "America/Havana"),
        ("Egypt", "Africa/Cairo"),
        ("Eire", "Europe/Dublin"),
        ("Hongkong", "Asia/Hong_Kong"),
        ("Iceland", "Atlantic/Reykjavik"),
        ("Iran", "Asia/Tehran"),
        ("Israel", "Asia/Jerusalem"),
        ("Jamaica", "America/Jamaica"),
        ("Japan", "Asia/Tokyo"),
        ("Kwajalein", "Pacific/Kwajalein"),
        ("Libya", "Africa/Tripoli"),
        ("NZ", "Pacific/Auckland"),
        ("NZ-CHAT", "Pacific/Chatham"),
        ("Navajo", "America/Denver"),
        ("PRC", "Asia/Shanghai"),
        ("Poland", "Europe/Warsaw"),
        ("Portugal", "Europe/Lisbon"),
        ("ROC", "Asia/Taipei"),
        ("ROK", "Asia/Seoul"),
        ("Singapore", "Asia/Singapore"),
        ("Turkey", "Europe/Istanbul"),
        ("W-SU", "Europe/Moscow"),
        ("GB", "Europe/London"),
        ("GB-Eire", "Europe/London"),
        // POSIX-style
        ("CET", "Europe/Brussels"),
        ("CST6CDT", "America/Chicago"),
        ("EET", "Europe/Athens"),
        ("EST", "America/Panama"),
        ("EST5EDT", "America/New_York"),
        ("HST", "Pacific/Honolulu"),
        ("MET", "Europe/Brussels"),
        ("MST", "America/Phoenix"),
        ("MST7MDT", "America/Denver"),
        ("PST8PDT", "America/Los_Angeles"),
        ("WET", "Europe/Lisbon"),
        // Indian
        ("Indian/Christmas", "Asia/Bangkok"),
        ("Indian/Cocos", "Asia/Yangon"),
        ("Indian/Kerguelen", "Indian/Maldives"),
        ("Indian/Mahe", "Asia/Dubai"),
        ("Indian/Reunion", "Asia/Dubai"),
        // Antarctica
        ("Antarctica/Davis", "Asia/Bangkok"),
        ("Antarctica/Mawson", "Indian/Maldives"),
        ("Antarctica/Rothera", "America/Noronha"),
        ("Antarctica/Vostok", "Asia/Urumqi"),
        ("Atlantic/Jan_Mayen", "Arctic/Longyearbyen"),
        ("Atlantic/Faeroe", "Atlantic/Faroe"),
    ];
    let mut current = s.to_string();
    for _ in 0..5 {
        let lower = current.to_ascii_lowercase();
        let mut found = false;
        for &(link, primary) in LINK_MAP {
            if link.eq_ignore_ascii_case(&lower) {
                current = primary.to_string();
                found = true;
                break;
            }
        }
        if !found {
            break;
        }
    }
    if current != s {
        return current;
    }
    // Not a link name — return the properly-cased version
    resolve_iana_timezone(s).unwrap_or_else(|| s.to_string())
}

/// Resolve an IANA timezone name, returning the properly-cased name if valid.
/// Uses chrono-tz's database for case-insensitive matching.
fn resolve_iana_timezone(s: &str) -> Option<String> {
    if s.is_empty() {
        return None;
    }
    // Fast path: try exact parse (case-sensitive hash lookup)
    use chrono_tz::Tz;
    if let Ok(tz) = s.parse::<Tz>() {
        return Some(tz.name().to_string());
    }
    // Slow path: case-insensitive scan
    use chrono_tz::TZ_VARIANTS;
    for tz in &TZ_VARIANTS {
        if tz.name().eq_ignore_ascii_case(s) {
            return Some(tz.name().to_string());
        }
    }
    None
}

fn is_iana_timezone(s: &str) -> bool {
    resolve_iana_timezone(s).is_some()
}

fn normalize_iana_timezone(s: &str) -> String {
    resolve_iana_timezone(s).unwrap_or_else(|| s.to_string())
}

/// Normalize a timezone ID for comparison: canonical offset form or case-insensitive IANA
pub(crate) fn normalize_tz_id(s: &str) -> String {
    // Try parsing as offset to get canonical form
    if let Some(canonical) = parse_utc_offset_timezone(s) {
        return canonical;
    }
    s.to_ascii_lowercase()
}

/// ParseTemporalTimeZoneString per spec: extract timezone identifier from a string
pub(super) fn parse_temporal_time_zone_string(s: &str) -> Option<String> {
    // 1. Try as UTC offset
    if let Some(offset) = parse_utc_offset_timezone(s) {
        return Some(offset);
    }

    // 2. Try as IANA timezone name
    if is_iana_timezone(s) {
        return Some(normalize_iana_timezone(s));
    }

    // 3. Try parsing as ISO datetime string and extract timezone info
    if let Some(parsed) = parse_temporal_date_time_string(s) {
        // Must have time component
        if !parsed.has_time {
            return None;
        }
        // If there's an explicit timezone annotation [Asia/Tokyo], use it
        if let Some(ref tz) = parsed.time_zone {
            if let Some(offset) = parse_utc_offset_timezone(tz) {
                return Some(offset);
            }
            if is_iana_timezone(tz) {
                return Some(normalize_iana_timezone(tz));
            }
            return None;
        }
        // If there's a UTC offset (Z or ±HH:MM), return it
        if let Some(ref offset) = parsed.offset {
            if offset.has_sub_minute {
                return None; // sub-minute offset
            }
            if parsed.has_utc_designator {
                return Some("UTC".to_string());
            }
            if offset.sign == 1 && offset.hours == 0 && offset.minutes == 0 {
                return Some("+00:00".to_string());
            }
            let sign = if offset.sign < 0 { '-' } else { '+' };
            return Some(format!("{}{:02}:{:02}", sign, offset.hours, offset.minutes));
        }
        // Has time but no timezone info → not valid
        return None;
    }

    None
}

/// Strict timezone validation — only bare offsets and IANA names, no ISO strings.
/// Used for constructor parameters where ISO string fallback is not allowed.
pub(super) fn validate_timezone_identifier_strict(
    interp: &mut Interpreter,
    arg: &JsValue,
) -> Result<String, Completion> {
    match arg {
        JsValue::String(s) => {
            let s_str = s.to_string();
            if let Some(offset) = parse_utc_offset_timezone(&s_str) {
                Ok(offset)
            } else if is_iana_timezone(&s_str) {
                Ok(normalize_iana_timezone(&s_str))
            } else {
                Err(Completion::Throw(interp.create_range_error(&format!(
                    "Invalid time zone: {}",
                    s_str
                ))))
            }
        }
        _ => to_temporal_time_zone_identifier(interp, arg),
    }
}

/// Strict calendar validation — only bare calendar names, no ISO strings.
pub(super) fn validate_calendar_strict(
    interp: &mut Interpreter,
    val: &JsValue,
) -> Result<String, Completion> {
    match val {
        JsValue::Undefined => Ok("iso8601".to_string()),
        JsValue::String(s) => {
            let raw = s.to_rust_string();
            if raw.is_empty() {
                return Err(Completion::Throw(
                    interp.create_range_error("Invalid calendar: empty string"),
                ));
            }
            if !raw.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                return Err(Completion::Throw(
                    interp.create_range_error(&format!("Invalid calendar: {raw}")),
                ));
            }
            let normalized = canonicalize_temporal_calendar(&raw);
            if is_supported_temporal_calendar(&normalized) {
                Ok(normalized)
            } else {
                Err(Completion::Throw(
                    interp.create_range_error(&format!("Invalid calendar: {raw}")),
                ))
            }
        }
        _ => to_temporal_calendar_slot_value(interp, val),
    }
}

/// ToTemporalTimeZoneIdentifier — validates and returns a timezone string, or throws
pub(super) fn to_temporal_time_zone_identifier(
    interp: &mut Interpreter,
    arg: &JsValue,
) -> Result<String, Completion> {
    match arg {
        JsValue::Undefined => {
            Ok(iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_string()))
        }
        JsValue::String(s) => {
            let s_str = s.to_string();
            match parse_temporal_time_zone_string(&s_str) {
                Some(tz) => Ok(tz),
                None => Err(Completion::Throw(
                    interp.create_range_error(&format!("Invalid time zone: {}", s_str)),
                )),
            }
        }
        JsValue::Object(o) => {
            // If it's a Temporal.ZonedDateTime, extract timeZoneId
            if let Some(obj) = interp.get_object(o.id) {
                let td = obj.borrow().temporal_data.clone();
                if let Some(TemporalData::ZonedDateTime { time_zone, .. }) = td {
                    return Ok(time_zone);
                }
            }
            Err(Completion::Throw(
                interp.create_type_error("Expected a string for time zone"),
            ))
        }
        JsValue::Null | JsValue::Boolean(_) | JsValue::Number(_) => Err(Completion::Throw(
            interp.create_type_error("Expected a string for time zone"),
        )),
        JsValue::Symbol(_) => Err(Completion::Throw(
            interp.create_type_error("Cannot convert a Symbol value to a string"),
        )),
        JsValue::BigInt(_) => Err(Completion::Throw(
            interp.create_type_error("Cannot convert a BigInt value to a string"),
        )),
    }
}

// --- ISO 8601 date/time string parser ---

pub(crate) struct ParsedIsoDateTime {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub millisecond: u16,
    pub microsecond: u16,
    pub nanosecond: u16,
    pub offset: Option<ParsedOffset>,
    pub calendar: Option<String>,
    pub time_zone: Option<String>,
    pub has_time: bool,
    pub has_utc_designator: bool,
}

pub(crate) struct ParsedOffset {
    pub sign: i32,
    pub hours: u8,
    pub minutes: u8,
    pub seconds: u8,
    pub nanoseconds: u32,
    pub has_sub_minute: bool,
}

pub(crate) fn parse_temporal_instant_string(s: &str) -> Option<ParsedIsoDateTime> {
    let parsed = parse_temporal_date_time_string(s)?;
    // Instant requires a UTC offset (Z or ±HH:MM) and a time component
    parsed.offset.as_ref()?;
    if !parsed.has_time {
        return None;
    }
    Some(parsed)
}

pub(crate) fn parse_temporal_date_time_string(s: &str) -> Option<ParsedIsoDateTime> {
    let s = s.trim();
    let bytes = s.as_bytes();
    let mut pos = 0;

    // Parse date: YYYY-MM-DD or YYYYMMDD
    let (year, month, day, new_pos) = parse_iso_date(bytes, pos)?;
    pos = new_pos;

    // Optional time part (separated by T or t or space)
    let mut hour = 0u8;
    let mut minute = 0u8;
    let mut second = 0u8;
    let mut millisecond = 0u16;
    let mut microsecond = 0u16;
    let mut nanosecond = 0u16;
    let mut has_time = false;

    if pos < bytes.len() && (bytes[pos] == b'T' || bytes[pos] == b't' || bytes[pos] == b' ') {
        pos += 1;
        let time_result = parse_iso_time(bytes, pos)?;
        hour = time_result.0;
        minute = time_result.1;
        second = time_result.2;
        millisecond = time_result.3;
        microsecond = time_result.4;
        nanosecond = time_result.5;
        pos = time_result.6;
        has_time = true;
    }

    // Optional offset
    let mut has_utc_designator = false;
    let offset = if pos < bytes.len()
        && (bytes[pos] == b'Z' || bytes[pos] == b'z' || bytes[pos] == b'+' || bytes[pos] == b'-')
    {
        if bytes[pos] == b'Z' || bytes[pos] == b'z' {
            has_utc_designator = true;
        }
        let (off, new_pos) = parse_iso_offset(bytes, pos)?;
        pos = new_pos;
        Some(off)
    } else {
        None
    };

    // Optional timezone annotation [...]
    let mut time_zone = None;
    let mut calendar = None;
    let mut calendar_critical = false;
    let mut calendar_count = 0u32;
    while pos < bytes.len() && bytes[pos] == b'[' {
        pos += 1;
        let is_critical = pos < bytes.len() && bytes[pos] == b'!';
        if is_critical {
            pos += 1;
        }
        let start = pos;
        while pos < bytes.len() && bytes[pos] != b']' {
            pos += 1;
        }
        if pos >= bytes.len() {
            return None;
        }
        let annotation = std::str::from_utf8(&bytes[start..pos]).ok()?;
        pos += 1; // skip ]
        if let Some(eq_pos) = annotation.find('=') {
            let key = &annotation[..eq_pos];
            if key.bytes().any(|b| b.is_ascii_uppercase()) {
                return None;
            }
            if key == "u-ca" {
                calendar_count += 1;
                if is_critical {
                    calendar_critical = true;
                }
                if calendar.is_none() {
                    calendar = Some(annotation[eq_pos + 1..].to_string());
                }
            } else if is_critical {
                return None;
            }
        } else {
            if time_zone.is_some() {
                return None;
            }
            // If it looks like a UTC offset, validate no sub-minute precision
            if annotation.starts_with('+') || annotation.starts_with('-') {
                if let Some(parsed_off) = parse_utc_offset_timezone(annotation) {
                    time_zone = Some(parsed_off);
                } else {
                    return None;
                }
            } else {
                // IANA name or custom timezone — store as-is
                time_zone = Some(annotation.to_string());
            }
        }
    }
    // Multiple calendar annotations with any critical flag → error
    if calendar_count > 1 && calendar_critical {
        return None;
    }

    if pos != bytes.len() {
        return None;
    }

    Some(ParsedIsoDateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        millisecond,
        microsecond,
        nanosecond,
        offset,
        calendar,
        time_zone,
        has_time,
        has_utc_designator,
    })
}

/// Check if a time-like string (without T prefix) is ambiguous with date formats.
/// E.g. "1214" could be MMDD or HHMM, "202112" could be YYYYMM or HHMMSS.
fn is_ambiguous_time_string(bytes: &[u8]) -> bool {
    let core_end = bytes.iter().position(|&b| b == b'[').unwrap_or(bytes.len());
    let core = &bytes[..core_end];

    // 4 pure digits: could be MMDD
    if core.len() == 4 && core.iter().all(|b| b.is_ascii_digit()) {
        let mm = (core[0] - b'0') as u32 * 10 + (core[1] - b'0') as u32;
        let dd = (core[2] - b'0') as u32 * 10 + (core[3] - b'0') as u32;
        let max_dd = match mm {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => 29,
            _ => return false,
        };
        if (1..=12).contains(&mm) && dd >= 1 && dd <= max_dd {
            return true;
        }
    }

    // MM-DD (5 chars with dash at position 2)
    if core.len() == 5
        && core[2] == b'-'
        && core[0].is_ascii_digit()
        && core[1].is_ascii_digit()
        && core[3].is_ascii_digit()
        && core[4].is_ascii_digit()
    {
        let mm = (core[0] - b'0') as u32 * 10 + (core[1] - b'0') as u32;
        let dd = (core[3] - b'0') as u32 * 10 + (core[4] - b'0') as u32;
        let max_dd = match mm {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => 29,
            _ => return false,
        };
        if (1..=12).contains(&mm) && dd >= 1 && dd <= max_dd {
            return true;
        }
    }

    // 6 pure digits: could be YYYYMM
    if core.len() == 6 && core.iter().all(|b| b.is_ascii_digit()) {
        let mm = (core[4] - b'0') as u32 * 10 + (core[5] - b'0') as u32;
        if (1..=12).contains(&mm) {
            return true;
        }
    }

    // YYYY-MM (7 chars with dash at position 4)
    if core.len() == 7
        && core[4] == b'-'
        && core[..4].iter().all(|b| b.is_ascii_digit())
        && core[5].is_ascii_digit()
        && core[6].is_ascii_digit()
    {
        let mm = (core[5] - b'0') as u32 * 10 + (core[6] - b'0') as u32;
        if (1..=12).contains(&mm) {
            return true;
        }
    }

    false
}

/// Parse a Temporal time string. Returns (h, m, s, ms, us, ns, has_utc_designator).
pub(crate) fn parse_temporal_time_string(s: &str) -> Option<(u8, u8, u8, u16, u16, u16, bool)> {
    let s = s.trim();
    let bytes = s.as_bytes();
    let has_t_prefix = !bytes.is_empty() && (bytes[0] == b'T' || bytes[0] == b't');
    let pos = if has_t_prefix { 1 } else { 0 };

    // Try parsing as time directly (HH:MM:SS or HHMMSS), optionally after T prefix
    if let Some(result) = parse_iso_time(bytes, pos) {
        let mut p = result.6;
        let mut has_z = false;
        if p < bytes.len()
            && (bytes[p] == b'Z' || bytes[p] == b'z' || bytes[p] == b'+' || bytes[p] == b'-')
        {
            if bytes[p] == b'Z' || bytes[p] == b'z' {
                has_z = true;
            }
            if let Some((_, np)) = parse_iso_offset(bytes, p) {
                p = np;
            }
        }
        if let Some(np) = skip_annotations_validated(bytes, p)
            && np == bytes.len()
        {
            // Without T prefix, check for ambiguity with date formats
            if !has_t_prefix {
                let could_be_date = parse_temporal_date_time_string(s).is_some()
                    || parse_temporal_year_month_string(s).is_some()
                    || parse_temporal_month_day_string(s).is_some();
                if could_be_date {
                    return None; // ambiguous → require T prefix
                }
                // Check for ambiguous forms: MMDD (4-digit), YYYYMM (6-digit),
                // HH-UU (MM-DD), YYYY-MM (with annotations/offsets).
                // Find the "numeric prefix" before any annotation or offset.
                if is_ambiguous_time_string(bytes) {
                    return None;
                }
            }
            return Some((
                result.0, result.1, result.2, result.3, result.4, result.5, has_z,
            ));
        }
    }

    // Try parsing as date-time and extract time part — must have time component
    let parsed = parse_temporal_date_time_string(s)?;
    if !parsed.has_time {
        return None;
    }
    Some((
        parsed.hour,
        parsed.minute,
        parsed.second,
        parsed.millisecond,
        parsed.microsecond,
        parsed.nanosecond,
        parsed.has_utc_designator,
    ))
}

/// Returns (year, month, calendar, has_utc_designator, date_only_offset).
/// `date_only_offset` is true when offset present but no time component.
pub(crate) fn parse_temporal_year_month_string(
    s: &str,
) -> Option<(i32, u8, Option<u8>, Option<String>, bool, bool)> {
    let s = s.trim();
    let bytes = s.as_bytes();
    // Try YYYY-MM or YYYYMM first
    if let Some((year, new_pos)) = parse_iso_year(bytes, 0) {
        let has_sep = new_pos < bytes.len() && bytes[new_pos] == b'-';
        let month_start = if has_sep { new_pos + 1 } else { new_pos };
        if let Some((month, np)) = parse_two_digit(bytes, month_start) {
            if (1..=12).contains(&month) {
                // Check it's not followed by more digits (which would make it a date)
                let next_is_digit = np < bytes.len() && bytes[np].is_ascii_digit();
                let next_is_dash = np < bytes.len() && bytes[np] == b'-';
                let next_is_t = np < bytes.len()
                    && (bytes[np] == b'T' || bytes[np] == b't' || bytes[np] == b' ');
                if !next_is_digit && !(has_sep && next_is_dash) && !next_is_t {
                    let mut pos = np;
                    let mut calendar = None;
                    pos = parse_annotations_extract_calendar(bytes, pos, &mut calendar)?;
                    if pos == bytes.len() {
                        return Some((year, month, None, calendar, false, false));
                    }
                }
            }
        }
    }
    // Fall back to full date-time
    let parsed = parse_temporal_date_time_string(s)?;
    let date_only_offset = !parsed.has_time && parsed.offset.is_some();
    Some((
        parsed.year,
        parsed.month,
        Some(parsed.day),
        parsed.calendar,
        parsed.has_utc_designator,
        date_only_offset,
    ))
}

/// Returns (month, day, ref_year, calendar, has_utc_designator).
pub(crate) fn parse_temporal_month_day_string(
    s: &str,
) -> Option<(u8, u8, Option<i32>, Option<String>, bool)> {
    let s = s.trim();
    let bytes = s.as_bytes();
    // Try MM-DD first
    if bytes.len() >= 5
        && bytes[2] == b'-'
        && let Some((month, p1)) = parse_two_digit(bytes, 0)
        && bytes.get(p1) == Some(&b'-')
        && let Some((day, p2)) = parse_two_digit(bytes, p1 + 1)
    {
        let mut p = p2;
        let mut calendar = None;
        p = parse_annotations_extract_calendar(bytes, p, &mut calendar)?;
        if p == bytes.len()
            && (1..=12).contains(&month)
            && day >= 1
            && day <= iso_days_in_month(1972, month)
        {
            return Some((month, day, None, calendar, false));
        }
    }
    // Try MMDD (4 digits, no dash)
    if bytes.len() >= 4
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_digit()
        && let Some((month, p1)) = parse_two_digit(bytes, 0)
        && let Some((day, p2)) = parse_two_digit(bytes, p1)
    {
        // Must not be followed by more digits (that would be YYYYMM or YYYYMMDD)
        if p2 == bytes.len() || !bytes[p2].is_ascii_digit() {
            let mut p = p2;
            let mut calendar = None;
            p = parse_annotations_extract_calendar(bytes, p, &mut calendar)?;
            if p == bytes.len()
                && (1..=12).contains(&month)
                && day >= 1
                && day <= iso_days_in_month(1972, month)
            {
                return Some((month, day, None, calendar, false));
            }
        }
    }
    // Try --MM-DD or --MMDD (ISO 8601 extended)
    if bytes.len() >= 6
        && bytes[0] == b'-'
        && bytes[1] == b'-'
        && let Some((month, p1)) = parse_two_digit(bytes, 2)
    {
        let sep = if bytes.get(p1) == Some(&b'-') {
            p1 + 1
        } else {
            p1
        };
        if let Some((day, p2)) = parse_two_digit(bytes, sep) {
            let mut p = p2;
            let mut calendar = None;
            p = parse_annotations_extract_calendar(bytes, p, &mut calendar)?;
            if p == bytes.len()
                && (1..=12).contains(&month)
                && day >= 1
                && day <= iso_days_in_month(1972, month)
            {
                return Some((month, day, None, calendar, false));
            }
        }
    }
    // Fall back to full date-time
    let parsed = parse_temporal_date_time_string(s)?;
    // Reject date-only strings with UTC offset (no time component)
    let date_only_offset =
        !parsed.has_time && (parsed.offset.is_some() || parsed.has_utc_designator);
    Some((
        parsed.month,
        parsed.day,
        Some(parsed.year),
        parsed.calendar,
        parsed.has_utc_designator || date_only_offset,
    ))
}

fn parse_iso_date(bytes: &[u8], start: usize) -> Option<(i32, u8, u8, usize)> {
    let mut pos = start;
    let (year, new_pos) = parse_iso_year(bytes, pos)?;
    pos = new_pos;
    let has_sep = pos < bytes.len() && bytes[pos] == b'-';
    if has_sep {
        pos += 1;
    }
    let (month, new_pos) = parse_two_digit(bytes, pos)?;
    pos = new_pos;
    if !(1..=12).contains(&month) {
        return None;
    }
    if has_sep {
        if pos < bytes.len() && bytes[pos] == b'-' {
            pos += 1;
        } else {
            return None;
        }
    }
    let (day, new_pos) = parse_two_digit(bytes, pos)?;
    pos = new_pos;
    if day < 1 || day > iso_days_in_month(year, month) {
        return None;
    }
    Some((year, month, day, pos))
}

fn parse_iso_year(bytes: &[u8], start: usize) -> Option<(i32, usize)> {
    let mut pos = start;
    let sign: i32;

    if pos < bytes.len() && (bytes[pos] == b'+' || bytes[pos] == b'-') {
        sign = if bytes[pos] == b'-' { -1 } else { 1 };
        pos += 1;
        // Extended year: 6 digits
        if pos + 6 > bytes.len() {
            return None;
        }
        let year_str = std::str::from_utf8(&bytes[pos..pos + 6]).ok()?;
        let year: i32 = year_str.parse().ok()?;
        // Reject -000000 (negative zero)
        if sign == -1 && year == 0 {
            return None;
        }
        Some((year * sign, pos + 6))
    } else {
        // 4-digit year
        if pos + 4 > bytes.len() {
            return None;
        }
        let year_str = std::str::from_utf8(&bytes[pos..pos + 4]).ok()?;
        let year: i32 = year_str.parse().ok()?;
        Some((year, pos + 4))
    }
}

fn parse_iso_time(bytes: &[u8], start: usize) -> Option<(u8, u8, u8, u16, u16, u16, usize)> {
    let mut pos = start;
    let (hour, new_pos) = parse_two_digit(bytes, pos)?;
    pos = new_pos;
    if hour > 23 {
        return None;
    }

    let has_sep = pos < bytes.len() && bytes[pos] == b':';
    if has_sep {
        pos += 1;
    }

    // Minute: optional if followed by offset/annotation/end
    let mut minute = 0u8;
    let mut second = 0u8;
    let mut ms = 0u16;
    let mut us = 0u16;
    let mut ns = 0u16;

    let has_minute = pos < bytes.len()
        && bytes[pos].is_ascii_digit()
        && (has_sep || (pos + 1 < bytes.len() && bytes[pos + 1].is_ascii_digit()));
    if has_minute {
        let (m, new_pos) = parse_two_digit(bytes, pos)?;
        pos = new_pos;
        if m > 59 {
            return None;
        }
        minute = m;

        let need_sep = has_sep;
        if pos < bytes.len()
            && ((need_sep && bytes[pos] == b':') || (!need_sep && bytes[pos].is_ascii_digit()))
        {
            if need_sep {
                pos += 1;
            }
            let (s, new_pos) = parse_two_digit(bytes, pos)?;
            pos = new_pos;
            // Allow 60 for leap second (treated as 59)
            if s > 60 {
                return None;
            }
            second = if s == 60 { 59 } else { s };

            // Fractional seconds
            if pos < bytes.len() && (bytes[pos] == b'.' || bytes[pos] == b',') {
                pos += 1;
                let frac_start = pos;
                while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                    pos += 1;
                }
                let frac_len = pos - frac_start;
                if frac_len == 0 || frac_len > 9 {
                    return None;
                }
                let mut frac_digits = [b'0'; 9];
                let copy_len = 9.min(frac_len);
                frac_digits[..copy_len].copy_from_slice(&bytes[frac_start..frac_start + copy_len]);
                let ms_str = std::str::from_utf8(&frac_digits[0..3]).ok()?;
                let us_str = std::str::from_utf8(&frac_digits[3..6]).ok()?;
                let ns_str = std::str::from_utf8(&frac_digits[6..9]).ok()?;
                ms = ms_str.parse().ok()?;
                us = us_str.parse().ok()?;
                ns = ns_str.parse().ok()?;
            }
        }
    }

    Some((hour, minute, second, ms, us, ns, pos))
}

/// Skip annotations `[...]`, extracting calendar and rejecting critical unknown annotations.
fn parse_annotations_extract_calendar(
    bytes: &[u8],
    start: usize,
    calendar: &mut Option<String>,
) -> Option<usize> {
    let mut p = start;
    while p < bytes.len() && bytes[p] == b'[' {
        p += 1;
        let is_critical = p < bytes.len() && bytes[p] == b'!';
        if is_critical {
            p += 1;
        }
        let ann_start = p;
        while p < bytes.len() && bytes[p] != b']' {
            p += 1;
        }
        if p >= bytes.len() {
            return None;
        }
        let annotation = std::str::from_utf8(&bytes[ann_start..p]).ok()?;
        p += 1;
        if let Some(eq_pos) = annotation.find('=') {
            let key = &annotation[..eq_pos];
            if key.bytes().any(|b| b.is_ascii_uppercase()) {
                return None;
            }
            if key == "u-ca" {
                if calendar.is_none() {
                    *calendar = Some(annotation[eq_pos + 1..].to_string());
                }
            } else if is_critical {
                return None;
            }
        }
    }
    Some(p)
}

/// Skip annotations `[...]`, rejecting critical unknown annotations (`[!key=value]`).
/// Returns None if a critical unknown annotation is found.
fn skip_annotations_validated(bytes: &[u8], start: usize) -> Option<usize> {
    let mut p = start;
    let mut calendar_count = 0u32;
    let mut calendar_critical = false;
    let mut tz_count = 0u32;
    while p < bytes.len() && bytes[p] == b'[' {
        p += 1;
        let is_critical = p < bytes.len() && bytes[p] == b'!';
        if is_critical {
            p += 1;
        }
        let ann_start = p;
        while p < bytes.len() && bytes[p] != b']' {
            p += 1;
        }
        if p >= bytes.len() {
            return None;
        }
        let annotation = std::str::from_utf8(&bytes[ann_start..p]).ok()?;
        p += 1; // skip ]
        if let Some(eq_pos) = annotation.find('=') {
            let key = &annotation[..eq_pos];
            if key.bytes().any(|b| b.is_ascii_uppercase()) {
                return None;
            }
            if key == "u-ca" {
                calendar_count += 1;
                if is_critical {
                    calendar_critical = true;
                }
            } else if is_critical {
                return None;
            }
        } else {
            tz_count += 1;
            if tz_count > 1 {
                return None;
            }
        }
    }
    if calendar_count > 1 && calendar_critical {
        return None;
    }
    Some(p)
}

fn parse_iso_offset(bytes: &[u8], start: usize) -> Option<(ParsedOffset, usize)> {
    let mut pos = start;
    if pos >= bytes.len() {
        return None;
    }

    if bytes[pos] == b'Z' || bytes[pos] == b'z' {
        return Some((
            ParsedOffset {
                sign: 1,
                hours: 0,
                minutes: 0,
                seconds: 0,
                nanoseconds: 0,
                has_sub_minute: false,
            },
            pos + 1,
        ));
    }

    let sign = match bytes[pos] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    pos += 1;

    let (hours, new_pos) = parse_two_digit(bytes, pos)?;
    pos = new_pos;
    if hours > 23 {
        return None;
    }

    let has_sep = pos < bytes.len() && bytes[pos] == b':';
    if has_sep {
        pos += 1;
    }

    let mut minutes = 0u8;
    let mut seconds = 0u8;
    let mut nanoseconds = 0u32;
    let mut has_sub_minute = false;

    if pos < bytes.len() && bytes[pos].is_ascii_digit() {
        let (m, new_pos) = parse_two_digit(bytes, pos)?;
        pos = new_pos;
        if m > 59 {
            return None;
        }
        minutes = m;

        if has_sep && pos < bytes.len() && bytes[pos] == b':' {
            pos += 1;
            has_sub_minute = true;
            let (s, new_pos) = parse_two_digit(bytes, pos)?;
            pos = new_pos;
            seconds = s;

            if pos < bytes.len() && (bytes[pos] == b'.' || bytes[pos] == b',') {
                pos += 1;
                let frac_start = pos;
                while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                    pos += 1;
                }
                let frac_len = pos - frac_start;
                if frac_len > 0 {
                    let mut frac_digits = [b'0'; 9];
                    let copy_len = 9.min(frac_len);
                    frac_digits[..copy_len]
                        .copy_from_slice(&bytes[frac_start..frac_start + copy_len]);
                    let ns_str = std::str::from_utf8(&frac_digits[..9]).ok()?;
                    nanoseconds = ns_str.parse().ok()?;
                }
            }
        }
    }

    Some((
        ParsedOffset {
            sign,
            hours,
            minutes,
            seconds,
            nanoseconds,
            has_sub_minute,
        },
        pos,
    ))
}

fn parse_two_digit(bytes: &[u8], pos: usize) -> Option<(u8, usize)> {
    if pos + 2 > bytes.len() {
        return None;
    }
    let d1 = bytes[pos].wrapping_sub(b'0');
    let d2 = bytes[pos + 1].wrapping_sub(b'0');
    if d1 > 9 || d2 > 9 {
        return None;
    }
    Some((d1 * 10 + d2, pos + 2))
}

// --- Helper: total nanoseconds for a time ---
pub(crate) fn time_to_nanoseconds(h: u8, m: u8, s: u8, ms: u16, us: u16, ns: u16) -> i128 {
    h as i128 * 3_600_000_000_000
        + m as i128 * 60_000_000_000
        + s as i128 * 1_000_000_000
        + ms as i128 * 1_000_000
        + us as i128 * 1_000
        + ns as i128
}

pub(crate) fn nanoseconds_to_time(mut ns: i128) -> (u8, u8, u8, u16, u16, u16) {
    if ns < 0 {
        ns += 86_400_000_000_000;
    }
    let nanosecond = (ns % 1000) as u16;
    ns /= 1000;
    let microsecond = (ns % 1000) as u16;
    ns /= 1000;
    let millisecond = (ns % 1000) as u16;
    ns /= 1000;
    let second = (ns % 60) as u8;
    ns /= 60;
    let minute = (ns % 60) as u8;
    ns /= 60;
    let hour = (ns % 24) as u8;
    (hour, minute, second, millisecond, microsecond, nanosecond)
}

// Duration sign helper: returns -1, 0, or 1
pub(crate) fn duration_sign(
    years: f64,
    months: f64,
    weeks: f64,
    days: f64,
    hours: f64,
    minutes: f64,
    seconds: f64,
    milliseconds: f64,
    microseconds: f64,
    nanoseconds: f64,
) -> i32 {
    for &v in &[
        years,
        months,
        weeks,
        days,
        hours,
        minutes,
        seconds,
        milliseconds,
        microseconds,
        nanoseconds,
    ] {
        if v < 0.0 {
            return -1;
        }
        if v > 0.0 {
            return 1;
        }
    }
    0
}

// Check that all duration fields have the same sign (or are zero)
pub(crate) fn is_valid_duration(
    years: f64,
    months: f64,
    weeks: f64,
    days: f64,
    hours: f64,
    minutes: f64,
    seconds: f64,
    milliseconds: f64,
    microseconds: f64,
    nanoseconds: f64,
) -> bool {
    let sign = duration_sign(
        years,
        months,
        weeks,
        days,
        hours,
        minutes,
        seconds,
        milliseconds,
        microseconds,
        nanoseconds,
    );
    for &v in &[
        years,
        months,
        weeks,
        days,
        hours,
        minutes,
        seconds,
        milliseconds,
        microseconds,
        nanoseconds,
    ] {
        if !v.is_finite() {
            return false;
        }
        if v < 0.0 && sign > 0 {
            return false;
        }
        if v > 0.0 && sign < 0 {
            return false;
        }
    }
    // Calendar units: abs(v) must be < 2^32
    for &v in &[years, months, weeks] {
        if v.abs() >= 4_294_967_296.0 {
            return false;
        }
    }
    // Per spec IsValidDuration step 6-7: compute total nanoseconds using exact
    // integer arithmetic (i128) to avoid f64 precision loss, then check against
    // maxTimeDuration = 2^53 × 10^9 - 1.
    // Guard against f64 values too large for i128 (e.g. Number.MAX_VALUE).
    for &v in &[
        days,
        hours,
        minutes,
        seconds,
        milliseconds,
        microseconds,
        nanoseconds,
    ] {
        if v.abs() > 1e35 {
            return false;
        }
    }
    let d = days.abs() as i128;
    let h = hours.abs() as i128;
    let mi = minutes.abs() as i128;
    let s = seconds.abs() as i128;
    let ms = milliseconds.abs() as i128;
    let us = microseconds.abs() as i128;
    let ns = nanoseconds.abs() as i128;
    let total_ns = d * 86_400_000_000_000
        + h * 3_600_000_000_000
        + mi * 60_000_000_000
        + s * 1_000_000_000
        + ms * 1_000_000
        + us * 1_000
        + ns;
    const MAX_TIME_DURATION: i128 = (1i128 << 53) * 1_000_000_000 - 1;
    if total_ns > MAX_TIME_DURATION {
        return false;
    }
    true
}

// Rounding utilities
pub(crate) fn round_number_to_increment(x: f64, increment: f64, rounding_mode: &str) -> f64 {
    let quotient = x / increment;
    let rounded = match rounding_mode {
        "ceil" => quotient.ceil(),
        "floor" => quotient.floor(),
        "trunc" => quotient.trunc(),
        "expand" => {
            if quotient >= 0.0 {
                quotient.ceil()
            } else {
                quotient.floor()
            }
        }
        "halfExpand" => {
            if quotient >= 0.0 {
                (quotient + 0.5).floor()
            } else {
                (quotient - 0.5).ceil()
            }
        }
        "halfTrunc" => {
            // Round to nearest, breaking ties toward zero
            let f = quotient.fract();
            if f == 0.0 {
                quotient
            } else if quotient > 0.0 {
                if f > 0.5 {
                    quotient.ceil()
                } else {
                    quotient.floor()
                }
            } else if f < -0.5 {
                quotient.floor()
            } else {
                quotient.ceil()
            }
        }
        "halfCeil" => (quotient + 0.5).floor(),
        "halfFloor" => (quotient - 0.5).ceil(),
        "halfEven" => {
            let down = quotient.floor();
            let up = quotient.ceil();
            let diff_down = (quotient - down).abs();
            let diff_up = (up - quotient).abs();
            if diff_down < diff_up {
                down
            } else if diff_up < diff_down {
                up
            } else {
                // Exactly halfway — pick even
                if down as i64 % 2 == 0 { down } else { up }
            }
        }
        _ => quotient.trunc(), // default: trunc
    };
    rounded * increment
}

/// Integer-precision rounding for i128 values (avoids f64 precision loss for large ns values)
pub(crate) fn round_i128_to_increment(x: i128, increment: i128, rounding_mode: &str) -> i128 {
    let remainder = x % increment;
    if remainder == 0 {
        return x;
    }
    let truncated = x - remainder;
    let sign = if x >= 0 { 1i128 } else { -1i128 };
    let abs_rem = remainder.abs();
    match rounding_mode {
        "trunc" => truncated,
        "ceil" => {
            if remainder > 0 {
                truncated + increment
            } else {
                truncated
            }
        }
        "floor" => {
            if remainder < 0 {
                truncated - increment
            } else {
                truncated
            }
        }
        "expand" => truncated + sign * increment,
        "halfExpand" => {
            if 2 * abs_rem >= increment {
                truncated + sign * increment
            } else {
                truncated
            }
        }
        "halfTrunc" => {
            if 2 * abs_rem > increment {
                truncated + sign * increment
            } else {
                truncated
            }
        }
        "halfCeil" => {
            if remainder > 0 {
                if 2 * remainder >= increment {
                    truncated + increment
                } else {
                    truncated
                }
            } else if 2 * abs_rem > increment {
                truncated - increment
            } else {
                truncated
            }
        }
        "halfFloor" => {
            if remainder > 0 {
                if 2 * remainder > increment {
                    truncated + increment
                } else {
                    truncated
                }
            } else if 2 * abs_rem >= increment {
                truncated - increment
            } else {
                truncated
            }
        }
        #[allow(clippy::if_same_then_else)]
        "halfEven" => {
            let q_trunc = x / increment;
            if 2 * abs_rem > increment {
                truncated + sign * increment
            } else if 2 * abs_rem < increment {
                truncated
            } else if q_trunc % 2 == 0 {
                truncated
            } else {
                truncated + sign * increment
            }
        }
        _ => truncated,
    }
}

// ToIntegerIfIntegral
// Spec: ToIntegerWithTruncation — ToNumber then reject NaN/Infinity, truncate
pub(crate) fn to_integer_with_truncation(
    interp: &mut Interpreter,
    val: &JsValue,
) -> Result<f64, Completion> {
    let n = match interp.to_number_value(val) {
        Ok(n) => n,
        Err(e) => return Err(Completion::Throw(e)),
    };
    if n.is_nan() || n.is_infinite() {
        return Err(Completion::Throw(interp.create_range_error(
            "Infinity is not allowed as a Temporal field value",
        )));
    }
    Ok(n.trunc())
}

pub(crate) fn to_integer_if_integral(v: f64) -> Option<f64> {
    if !v.is_finite() {
        return None;
    }
    if v != v.trunc() {
        return None;
    }
    Some(v)
}

// Format fractional seconds for toString
#[allow(dead_code)]
pub(crate) fn format_fractional_seconds(
    seconds: u8,
    millisecond: u16,
    microsecond: u16,
    nanosecond: u16,
    precision: Option<u8>,
) -> String {
    let s = format!("{seconds:02}");
    let total_ns = millisecond as u32 * 1_000_000 + microsecond as u32 * 1_000 + nanosecond as u32;
    match precision {
        Some(0) => s,
        Some(p) => {
            let frac = format!("{total_ns:09}");
            format!("{s}.{}", &frac[..p as usize])
        }
        None => {
            // Auto: trim trailing zeros
            if total_ns == 0 {
                s
            } else {
                let frac = format!("{total_ns:09}");
                let trimmed = frac.trim_end_matches('0');
                format!("{s}.{trimmed}")
            }
        }
    }
}

// ISO month code
pub(crate) fn iso_month_code(month: u8) -> String {
    format!("M{month:02}")
}

/// Read raw month/monthCode values from a property bag (coerce each immediately).
/// Returns (Option<u8>, Option<String>) for (month, monthCode).
/// Per spec: get+coerce month BEFORE get+coerce monthCode (alphabetical with immediate coercion).
pub(crate) fn read_month_fields(
    interp: &mut Interpreter,
    obj: &JsValue,
) -> Result<(Option<u8>, Option<String>), Completion> {
    // Read and coerce month immediately
    let m_val = match get_prop(interp, obj, "month") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let month = if is_undefined(&m_val) {
        None
    } else {
        Some(to_integer_with_truncation(interp, &m_val)? as u8)
    };
    // Then read and coerce monthCode
    let mc_val = match get_prop(interp, obj, "monthCode") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let month_code = if is_undefined(&mc_val) {
        None
    } else {
        Some(to_primitive_and_require_string(
            interp,
            &mc_val,
            "monthCode",
        )?)
    };
    Ok((month, month_code))
}

pub(crate) fn to_primitive_and_require_string(
    interp: &mut Interpreter,
    val: &JsValue,
    field_name: &str,
) -> Result<String, Completion> {
    let primitive = interp
        .to_primitive(val, "string")
        .map_err(Completion::Throw)?;
    match primitive {
        JsValue::String(s) => Ok(s.to_rust_string()),
        _ => Err(Completion::Throw(
            interp.create_type_error(&format!("{field_name} must be a string")),
        )),
    }
}

/// Resolve previously-read month/monthCode values into a concrete month number.
/// Validates monthCode and checks consistency with month.
pub(crate) fn resolve_month_fields(
    interp: &mut Interpreter,
    month: Option<u8>,
    month_code: Option<String>,
    current: u8,
) -> Result<u8, Completion> {
    if let Some(mc) = month_code {
        match plain_date::month_code_to_number_pub(&mc) {
            Some(n) => {
                if let Some(explicit_m) = month
                    && explicit_m != n
                {
                    return Err(Completion::Throw(
                        interp.create_range_error("month and monthCode conflict"),
                    ));
                }
                Ok(n)
            }
            None => Err(Completion::Throw(
                interp.create_range_error(&format!("Invalid monthCode: {mc}")),
            )),
        }
    } else if let Some(m) = month {
        Ok(m)
    } else {
        Ok(current)
    }
}

/// Convenience: read + resolve in one step (for callers where ordering doesn't matter).
#[allow(dead_code)]
pub(crate) fn resolve_month_with_code(
    interp: &mut Interpreter,
    obj: &JsValue,
    current: u8,
) -> Result<u8, Completion> {
    let (month, month_code) = read_month_fields(interp, obj)?;
    resolve_month_fields(interp, month, month_code, current)
}

// Get temporal unit name mapping
pub(crate) fn temporal_unit_singular(unit: &str) -> Option<&'static str> {
    match unit {
        "years" | "year" => Some("year"),
        "months" | "month" => Some("month"),
        "weeks" | "week" => Some("week"),
        "days" | "day" => Some("day"),
        "hours" | "hour" => Some("hour"),
        "minutes" | "minute" => Some("minute"),
        "seconds" | "second" => Some("second"),
        "milliseconds" | "millisecond" => Some("millisecond"),
        "microseconds" | "microsecond" => Some("microsecond"),
        "nanoseconds" | "nanosecond" => Some("nanosecond"),
        _ => None,
    }
}

pub(crate) fn temporal_unit_length_ns(unit: &str) -> f64 {
    match unit {
        "day" | "days" => 86_400_000_000_000.0,
        "hour" | "hours" => 3_600_000_000_000.0,
        "minute" | "minutes" => 60_000_000_000.0,
        "second" | "seconds" => 1_000_000_000.0,
        "millisecond" | "milliseconds" => 1_000_000.0,
        "microsecond" | "microseconds" => 1_000.0,
        "nanosecond" | "nanoseconds" => 1.0,
        _ => 1.0,
    }
}

pub(crate) fn default_largest_unit_for_duration(
    years: f64,
    months: f64,
    weeks: f64,
    days: f64,
    hours: f64,
    minutes: f64,
    seconds: f64,
    milliseconds: f64,
    microseconds: f64,
    _nanoseconds: f64,
) -> &'static str {
    if years != 0.0 {
        return "year";
    }
    if months != 0.0 {
        return "month";
    }
    if weeks != 0.0 {
        return "week";
    }
    if days != 0.0 {
        return "day";
    }
    if hours != 0.0 {
        return "hour";
    }
    if minutes != 0.0 {
        return "minute";
    }
    if seconds != 0.0 {
        return "second";
    }
    if milliseconds != 0.0 {
        return "millisecond";
    }
    if microseconds != 0.0 {
        return "microsecond";
    }
    "nanosecond"
}

// Larger temporal unit ordering
/// DateDurationSign: returns the sign of a date duration's components.
#[allow(dead_code)]
pub(crate) fn duration_date_sign(years: i32, months: i32, weeks: i32, days: i32) -> i32 {
    for &v in &[years, months, weeks, days] {
        if v > 0 {
            return 1;
        }
        if v < 0 {
            return -1;
        }
    }
    0
}

pub(crate) fn negate_rounding_mode(mode: &str) -> String {
    match mode {
        "ceil" => "floor".to_string(),
        "floor" => "ceil".to_string(),
        "halfCeil" => "halfFloor".to_string(),
        "halfFloor" => "halfCeil".to_string(),
        _ => mode.to_string(), // expand, trunc, halfExpand, halfTrunc, halfEven are symmetric
    }
}

pub(crate) fn temporal_unit_order(unit: &str) -> u8 {
    match unit {
        "year" | "years" => 10,
        "month" | "months" => 9,
        "week" | "weeks" => 8,
        "day" | "days" => 7,
        "hour" | "hours" => 6,
        "minute" | "minutes" => 5,
        "second" | "seconds" => 4,
        "millisecond" | "milliseconds" => 3,
        "microsecond" | "microseconds" => 2,
        "nanosecond" | "nanoseconds" => 1,
        _ => 0,
    }
}

/// Parse rounding options (smallestUnit, roundingMode, roundingIncrement) from options bag.
/// Returns (smallest_unit, rounding_mode, rounding_increment).
/// If smallest_unit is None, no rounding needed.
pub(crate) fn parse_difference_options(
    interp: &mut Interpreter,
    options: &JsValue,
    default_largest: &str,
    allowed_units: &[&str],
) -> Result<(String, String, String, f64), Completion> {
    let default_smallest = *allowed_units.last().unwrap_or(&"nanosecond");

    // GetOptionsObject per spec
    let has_options = get_options_object(interp, options)?;
    if !has_options {
        return Ok((
            default_largest.to_string(),
            default_smallest.to_string(),
            "trunc".to_string(),
            1.0,
        ));
    }

    // Read ALL options first (get + coerce), then validate

    // 1. largestUnit: get + coerce to string
    let lu = match get_prop(interp, options, "largestUnit") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let lu_str: Option<String> = if is_undefined(&lu) {
        None
    } else {
        Some(match interp.to_string_value(&lu) {
            Ok(v) => v,
            Err(e) => return Err(Completion::Throw(e)),
        })
    };

    // 2. roundingIncrement: get + coerce
    let ri = match get_prop(interp, options, "roundingIncrement") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let ri_coerced = coerce_rounding_increment(interp, &ri)?;

    // 3. roundingMode: get + coerce to string
    let rm = match get_prop(interp, options, "roundingMode") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let rm_str: Option<String> = if is_undefined(&rm) {
        None
    } else {
        Some(match interp.to_string_value(&rm) {
            Ok(v) => v,
            Err(e) => return Err(Completion::Throw(e)),
        })
    };

    // 4. smallestUnit: get + coerce to string
    let su = match get_prop(interp, options, "smallestUnit") {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let su_str: Option<String> = if is_undefined(&su) {
        None
    } else {
        Some(match interp.to_string_value(&su) {
            Ok(v) => v,
            Err(e) => return Err(Completion::Throw(e)),
        })
    };

    // Now validate all values

    let mut largest_unit_auto = lu_str.is_none();
    let largest_unit = if let Some(ref ls) = lu_str {
        if ls == "auto" {
            largest_unit_auto = true;
            default_largest.to_string()
        } else {
            match temporal_unit_singular(ls) {
                Some(u) => {
                    if !allowed_units.contains(&u) {
                        return Err(Completion::Throw(interp.create_range_error(&format!(
                            "{ls} is not a valid value for largestUnit"
                        ))));
                    }
                    u.to_string()
                }
                None => {
                    return Err(Completion::Throw(
                        interp.create_range_error(&format!("Invalid unit: {ls}")),
                    ));
                }
            }
        }
    } else {
        default_largest.to_string()
    };

    let rounding_mode = if let Some(ref rs) = rm_str {
        match rs.as_str() {
            "ceil" | "floor" | "trunc" | "expand" | "halfExpand" | "halfTrunc" | "halfCeil"
            | "halfFloor" | "halfEven" => rs.clone(),
            _ => {
                return Err(Completion::Throw(interp.create_range_error(&format!(
                    "{rs} is not a valid value for roundingMode"
                ))));
            }
        }
    } else {
        "trunc".to_string()
    };

    let smallest_unit = if let Some(ref ss) = su_str {
        match temporal_unit_singular(ss) {
            Some(u) => {
                if !allowed_units.contains(&u) {
                    return Err(Completion::Throw(interp.create_range_error(&format!(
                        "{ss} is not a valid value for smallestUnit"
                    ))));
                }
                u.to_string()
            }
            None => {
                return Err(Completion::Throw(
                    interp.create_range_error(&format!("Invalid unit: {ss}")),
                ));
            }
        }
    } else {
        default_smallest.to_string()
    };

    // Per spec: if largestUnit was auto/default, bump it up to at least smallestUnit
    let largest_unit = if largest_unit_auto
        && temporal_unit_order(&smallest_unit) > temporal_unit_order(&largest_unit)
    {
        smallest_unit.clone()
    } else {
        largest_unit
    };

    // Validate: smallestUnit <= largestUnit
    if temporal_unit_order(&smallest_unit) > temporal_unit_order(&largest_unit) {
        return Err(Completion::Throw(interp.create_range_error(
            "smallestUnit must not be larger than largestUnit",
        )));
    }

    // Validate roundingIncrement against smallestUnit (using pre-coerced value)
    let rounding_increment = ri_coerced;
    if let Some(max) = max_rounding_increment(&smallest_unit) {
        let i = rounding_increment as u64;
        if i >= max {
            return Err(Completion::Throw(interp.create_range_error(&format!(
                "roundingIncrement {rounding_increment} is out of range for {smallest_unit}"
            ))));
        }
        if max % i != 0 {
            return Err(Completion::Throw(interp.create_range_error(&format!(
                "roundingIncrement {rounding_increment} does not divide evenly into {max}"
            ))));
        }
    }

    Ok((
        largest_unit,
        smallest_unit,
        rounding_mode,
        rounding_increment,
    ))
}

/// Round a date duration per RoundRelativeDuration (simplified for ISO 8601 calendar).
/// Takes the raw difference (years, months, weeks, days) and the start date for calendar
/// computations. Returns (years, months, weeks, days) after rounding.
pub(crate) fn round_date_duration(
    years: i32,
    months: i32,
    weeks: i32,
    days: i32,
    smallest_unit: &str,
    largest_unit: &str,
    rounding_increment: f64,
    rounding_mode: &str,
    ref_year: i32,
    ref_month: u8,
    ref_day: u8,
) -> Result<(i32, i32, i32, i32), String> {
    round_date_duration_with_frac_days(
        years,
        months,
        weeks,
        days as f64,
        0i128,
        smallest_unit,
        largest_unit,
        rounding_increment,
        rounding_mode,
        ref_year,
        ref_month,
        ref_day,
        false,
    )
}

pub(crate) fn round_date_duration_with_frac_days(
    years: i32,
    months: i32,
    weeks: i32,
    frac_days: f64,
    time_ns_i128: i128,
    smallest_unit: &str,
    largest_unit: &str,
    rounding_increment: f64,
    rounding_mode: &str,
    ref_year: i32,
    ref_month: u8,
    ref_day: u8,
    is_zdt: bool,
) -> Result<(i32, i32, i32, i32), String> {
    let days = frac_days.trunc() as i32;
    match smallest_unit {
        "year" => {
            // Use DifferenceDate to find the correct year boundary
            let end_date = add_iso_date(ref_year, ref_month, ref_day, years, months, weeks, days);
            let (diff_y, _, _, _) = difference_iso_date(
                ref_year, ref_month, ref_day, end_date.0, end_date.1, end_date.2, "year",
            );
            let year_start = add_iso_date(ref_year, ref_month, ref_day, diff_y, 0, 0, 0);
            let year_start_epoch = iso_date_to_epoch_days(year_start.0, year_start.1, year_start.2);
            let end_epoch = iso_date_to_epoch_days(end_date.0, end_date.1, end_date.2);
            let sign = if end_epoch > iso_date_to_epoch_days(ref_year, ref_month, ref_day) {
                1
            } else if end_epoch < iso_date_to_epoch_days(ref_year, ref_month, ref_day) {
                -1
            } else {
                return Ok((0, 0, 0, 0));
            };
            let year_end = add_iso_date(ref_year, ref_month, ref_day, diff_y + sign, 0, 0, 0);
            let year_end_epoch = iso_date_to_epoch_days(year_end.0, year_end.1, year_end.2);
            let days_in_year = (year_end_epoch - year_start_epoch).abs() as f64;
            let remaining_days = (end_epoch - year_start_epoch) as f64 + frac_days.fract();
            let fractional = diff_y as f64
                + if days_in_year > 0.0 {
                    remaining_days / days_in_year
                } else {
                    0.0
                };
            let rounded = round_number_to_increment(fractional, rounding_increment, rounding_mode);
            Ok((rounded as i32, 0, 0, 0))
        }
        "month" => {
            let end_date = add_iso_date(ref_year, ref_month, ref_day, years, months, weeks, days);
            let end_epoch = iso_date_to_epoch_days(end_date.0, end_date.1, end_date.2);
            let base_epoch = iso_date_to_epoch_days(ref_year, ref_month, ref_day);
            let sign = if end_epoch > base_epoch {
                1
            } else if end_epoch < base_epoch {
                -1
            } else {
                return Ok((0, 0, 0, 0));
            };
            if largest_unit == "year" {
                // Per spec NudgeToCalendarUnit: keep years fixed, round only months component
                let month_base = add_iso_date(ref_year, ref_month, ref_day, years, months, 0, 0);
                let month_base_epoch =
                    iso_date_to_epoch_days(month_base.0, month_base.1, month_base.2);
                let month_next =
                    add_iso_date(ref_year, ref_month, ref_day, years, months + sign, 0, 0);
                let month_next_epoch =
                    iso_date_to_epoch_days(month_next.0, month_next.1, month_next.2);
                let days_in_month = (month_next_epoch - month_base_epoch).abs() as f64;
                let remaining_days = (end_epoch - month_base_epoch) as f64 + frac_days.fract();
                let fractional = months as f64
                    + if days_in_month > 0.0 {
                        remaining_days / days_in_month
                    } else {
                        0.0
                    };
                let rounded =
                    round_number_to_increment(fractional, rounding_increment, rounding_mode);
                Ok((years, rounded as i32, 0, 0))
            } else {
                // Flatten to total months
                let (_, total_months, _, _) = difference_iso_date(
                    ref_year, ref_month, ref_day, end_date.0, end_date.1, end_date.2, "month",
                );
                let month_start = add_iso_date(ref_year, ref_month, ref_day, 0, total_months, 0, 0);
                let month_start_epoch =
                    iso_date_to_epoch_days(month_start.0, month_start.1, month_start.2);
                let month_end =
                    add_iso_date(ref_year, ref_month, ref_day, 0, total_months + sign, 0, 0);
                let month_end_epoch = iso_date_to_epoch_days(month_end.0, month_end.1, month_end.2);
                let days_in_month = (month_end_epoch - month_start_epoch).abs() as f64;
                let remaining_days = (end_epoch - month_start_epoch) as f64 + frac_days.fract();
                let fractional = total_months as f64
                    + if days_in_month > 0.0 {
                        remaining_days / days_in_month
                    } else {
                        0.0
                    };
                let rounded =
                    round_number_to_increment(fractional, rounding_increment, rounding_mode);
                Ok((0, rounded as i32, 0, 0))
            }
        }
        "week" => {
            let end_date = add_iso_date(ref_year, ref_month, ref_day, years, months, weeks, days);
            let end_epoch = iso_date_to_epoch_days(end_date.0, end_date.1, end_date.2);
            let preserve_months = matches!(largest_unit, "year" | "month");
            if preserve_months {
                // largestUnit preserves months: round remaining weeks within current month
                let (_, total_months, _, _) = difference_iso_date(
                    ref_year, ref_month, ref_day, end_date.0, end_date.1, end_date.2, "month",
                );
                let month_start = add_iso_date(ref_year, ref_month, ref_day, 0, total_months, 0, 0);
                let month_start_epoch =
                    iso_date_to_epoch_days(month_start.0, month_start.1, month_start.2);
                let remaining = (end_epoch - month_start_epoch) as f64 + frac_days.fract();
                let fractional_weeks = remaining / 7.0;
                let rounded =
                    round_number_to_increment(fractional_weeks, rounding_increment, rounding_mode);
                Ok((0, total_months, rounded as i32, 0))
            } else {
                // Flatten everything to total weeks
                let base_epoch = iso_date_to_epoch_days(ref_year, ref_month, ref_day);
                let total_days = (end_epoch - base_epoch) as f64 + frac_days.fract();
                let fractional_weeks = total_days / 7.0;
                let rounded =
                    round_number_to_increment(fractional_weeks, rounding_increment, rounding_mode);
                Ok((0, 0, rounded as i32, 0))
            }
        }
        "day" => {
            // NudgeToCalendarUnit pre-check for ZDT: GetStartOfDay calls CheckISODaysRange
            if is_zdt {
                let sign_f = if frac_days > 0.0 {
                    1.0
                } else if frac_days < 0.0 {
                    -1.0
                } else if time_ns_i128 > 0 {
                    1.0
                } else if time_ns_i128 < 0 {
                    -1.0
                } else {
                    0.0
                };
                if sign_f != 0.0 {
                    let end_days_i64 =
                        frac_days.trunc() as i64 + sign_f as i64 * rounding_increment as i64;
                    let nudge_base = if years != 0 || months != 0 || weeks != 0 {
                        add_iso_date(ref_year, ref_month, ref_day, years, months, weeks, 0)
                    } else {
                        (ref_year, ref_month, ref_day)
                    };
                    let nudge_end = add_iso_date(
                        nudge_base.0,
                        nudge_base.1,
                        nudge_base.2,
                        0,
                        0,
                        0,
                        end_days_i64 as i32,
                    );
                    if iso_date_to_epoch_days(nudge_end.0, nudge_end.1, nudge_end.2).abs()
                        > 100_000_000
                    {
                        return Err("Rounded date outside valid ISO range".to_string());
                    }
                }
            }
            if years != 0 || months != 0 || weeks != 0 {
                let days = frac_days.trunc() as i32;
                let start = add_iso_date(ref_year, ref_month, ref_day, years, months, weeks, 0);
                let end_date =
                    add_iso_date(ref_year, ref_month, ref_day, years, months, weeks, days);
                let start_epoch = iso_date_to_epoch_days(start.0, start.1, start.2);
                let end_epoch = iso_date_to_epoch_days(end_date.0, end_date.1, end_date.2);
                let calendar_days = (end_epoch - start_epoch) as i128;
                let sub_day_ns = time_ns_i128 % 86_400_000_000_000;
                let remaining_ns: i128 = calendar_days * 86_400_000_000_000 + sub_day_ns;
                let inc_ns: i128 = rounding_increment as i128 * 86_400_000_000_000;
                let rounded_ns = round_i128_to_increment(remaining_ns, inc_ns, rounding_mode);
                let rounded_days = (rounded_ns / 86_400_000_000_000) as i32;
                Ok((years, months, weeks, rounded_days))
            } else {
                let days_i = frac_days.trunc() as i128;
                let sub_day_ns = time_ns_i128 % 86_400_000_000_000;
                let total_day_ns: i128 = days_i * 86_400_000_000_000 + sub_day_ns;
                let inc_ns: i128 = rounding_increment as i128 * 86_400_000_000_000;
                let rounded_ns = round_i128_to_increment(total_day_ns, inc_ns, rounding_mode);
                let rounded_days = (rounded_ns / 86_400_000_000_000) as i32;
                Ok((0, 0, 0, rounded_days))
            }
        }
        _ => Ok((years, months, weeks, days)),
    }
}
