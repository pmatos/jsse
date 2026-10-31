use super::*;
use icu::datetime::NoCalendarFormatter;
use icu::datetime::fieldsets::zone::SpecificLong;
use icu::datetime::input::{Date, DateTime, Time};
use icu::time::zone::{IanaParser, UtcOffset};
use std::cell::OnceCell;
use writeable::Writeable;

type LongNameFormatter = Option<NoCalendarFormatter<SpecificLong>>;

thread_local! {
    static SPECIFIC_LONG: OnceCell<LongNameFormatter> = const { OnceCell::new() };
}

/// CLDR specific non-location long name of `tz_id` at the UTC instant
/// `epoch_ms` (for example "Central European Summer Time"), falling back to
/// the localized offset form ("GMT+09:00") where CLDR has no name. Same text
/// as V8's `Intl.DateTimeFormat('en', {timeZoneName: 'long'})`.
pub(crate) fn long_time_zone_name(tz_id: &str, epoch_ms: f64, offset_secs: i32) -> Option<String> {
    let local_ms = epoch_ms + f64::from(offset_secs) * 1000.0;
    let datetime = DateTime {
        date: Date::try_new_iso(
            year_from_time(local_ms) as i32,
            month_from_time(local_ms) as u8 + 1,
            date_from_time(local_ms) as u8,
        )
        .ok()?,
        time: Time::try_new(
            hour_from_time(local_ms) as u8,
            min_from_time(local_ms) as u8,
            sec_from_time(local_ms) as u8,
            0,
        )
        .ok()?,
    };
    let zone = IanaParser::new()
        .parse(tz_id)
        .with_offset(UtcOffset::try_from_seconds(offset_secs).ok())
        .at_date_time_iso(datetime);
    SPECIFIC_LONG.with(|cell| {
        let formatter = cell.get_or_init(|| {
            NoCalendarFormatter::try_new(icu::locale::locale!("en").into(), SpecificLong).ok()
        });
        let formatted = formatter.as_ref()?.format(&zone);
        Some(formatted.write_to_string().into_owned())
    })
}

fn local_time_zone_abbreviation(t: f64) -> String {
    let Some(tz) = system_time_zone_jiff() else {
        return system_time_zone_identifier();
    };
    if !t.is_finite() {
        return system_time_zone_identifier();
    }
    let epoch_secs = (t.floor() as i64).div_euclid(1000);
    match named_time_zone_timestamp(epoch_secs) {
        Some(ts) => tz.to_offset_info(ts).abbreviation().to_string(),
        None => system_time_zone_identifier(),
    }
}

/// The parenthesised text of `Date.prototype.toString` and `toTimeString`
/// (TimeZoneString leaves it implementation-defined): the zone abbreviation,
/// or with `long` the CLDR long name V8 prints.
pub(crate) fn local_time_zone_name(t: f64, long: bool) -> String {
    if long && t.is_finite() {
        let offset_secs = (local_tza(t) / 1000.0) as i32;
        if let Some(name) = long_time_zone_name(&system_time_zone_identifier(), t, offset_secs) {
            return name;
        }
    }
    local_time_zone_abbreviation(t)
}
