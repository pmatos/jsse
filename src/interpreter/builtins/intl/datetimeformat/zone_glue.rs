use super::*;
use crate::interpreter::helpers::long_time_zone_name;
use std::cell::RefCell;

type GlueKey = (String, String, String, &'static str, bool);

thread_local! {
    static GLUE_CACHE: RefCell<Vec<(GlueKey, Option<String>)>> = const { RefCell::new(Vec::new()) };
}

const GLUE_CACHE_CAP: usize = 32;

fn needs_date_zone_glue(opts: &DtfOptions) -> bool {
    opts.time_zone_name.is_some()
        && opts.date_style.is_none()
        && opts.time_style.is_none()
        && !has_time_component(opts)
        && (opts.weekday.is_some()
            || opts.era.is_some()
            || opts.year.is_some()
            || opts.month.is_some()
            || opts.day.is_some())
}

/// Separator before the zone name of the English-only fallback formatter.
pub(super) fn raw_zone_separator(opts: &DtfOptions) -> &'static str {
    if !needs_date_zone_glue(opts) || opts.locale.split('-').next() != Some("en") {
        return " ";
    }
    if opts.month.as_deref() == Some("long") {
        " at "
    } else {
        ", "
    }
}

/// The literal CLDR puts between the date and the time for these date fields.
/// ICU4C picks the date-time glue from the month width alone (MMMM: long, or
/// full with a weekday; MMM: medium; anything else: short), so read it off a
/// probe whose date has that month width followed by an hour.
fn probe_date_time_glue(ms: f64, opts: &DtfOptions, month: &str, weekday: bool) -> Option<String> {
    let mut probe = opts.clone();
    probe.weekday = weekday.then(|| "long".to_string());
    probe.era = None;
    probe.year = Some("numeric".to_string());
    probe.month = Some(month.to_string());
    probe.day = Some("numeric".to_string());
    probe.time_zone_name = None;
    probe.hour = Some("numeric".to_string());
    let parts = format_with_icu(ms, &probe)?;
    let hour_at = parts.iter().position(|(t, _)| t == "hour")?;
    let date_end = parts[..hour_at].iter().rposition(|(t, _)| t != "literal")? + 1;
    let glue: String = parts[date_end..hour_at]
        .iter()
        .map(|(_, v)| v.as_str())
        .collect();
    (!glue.is_empty()).then_some(glue)
}

fn date_time_glue(ms: f64, opts: &DtfOptions) -> Option<String> {
    let month = match opts.month.as_deref() {
        Some("long") => "long",
        Some("short") => "short",
        _ => "numeric",
    };
    let weekday = month == "long" && opts.weekday.is_some();
    let key: GlueKey = (
        opts.locale.clone(),
        opts.calendar.clone(),
        opts.numbering_system.clone(),
        month,
        weekday,
    );
    let cached = GLUE_CACHE.with(|cache| {
        let cache = cache.borrow();
        cache
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, glue)| glue.clone())
    });
    if let Some(glue) = cached {
        return glue;
    }
    let glue = probe_date_time_glue(ms, opts, month, weekday);
    GLUE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= GLUE_CACHE_CAP {
            cache.remove(0);
        }
        cache.push((key, glue.clone()));
    });
    glue
}

/// ICU4X joins a date and a zone name with a plain space; ICU4C (V8) treats
/// the zone as the time part of the date-time glue, so `en` prints
/// "1/1/1970, Central European Standard Time" and "September 9 at ...".
pub(super) fn apply_date_zone_glue(
    ms: f64,
    opts: &DtfOptions,
    mut parts: Vec<(String, String)>,
) -> Vec<(String, String)> {
    if !needs_date_zone_glue(opts) {
        return parts;
    }
    let Some(zone_at) = parts.iter().position(|(t, _)| t == "timeZoneName") else {
        return parts;
    };
    let Some(date_end) = parts[..zone_at]
        .iter()
        .rposition(|(t, _)| t != "literal")
        .map(|i| i + 1)
    else {
        return parts;
    };
    let Some(glue) = date_time_glue(ms, opts) else {
        return parts;
    };
    parts.splice(
        date_end..zone_at,
        std::iter::once(("literal".to_string(), glue)),
    );
    parts
}

/// The `timeZoneName: "long"` text of the fallback formatter: the CLDR name, or
/// the localized `GMT+hh:mm` form when CLDR has none.
pub(super) fn long_name_or_offset(tz_id: &str, epoch_ms: f64, offset_secs: i32) -> String {
    long_time_zone_name(tz_id, epoch_ms, offset_secs).unwrap_or_else(|| {
        let hours = offset_secs / 3600;
        let minutes = (offset_secs.abs() % 3600) / 60;
        format_offset_long(hours, if hours < 0 { -minutes } else { minutes })
    })
}
