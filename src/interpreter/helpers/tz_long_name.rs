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

const REFERENCE_JANUARY_MS: f64 = 1_736_899_200_000.0;
const REFERENCE_JULY_MS: f64 = 1_752_537_600_000.0;

fn is_offset_form(name: &str) -> bool {
    name.starts_with("GMT")
}

fn is_daylight(t: f64) -> bool {
    let year_start = time_from_year(year_from_time(t));
    let standard = local_tza(year_start).min(local_tza(year_start + 182.0 * MS_PER_DAY));
    local_tza(t) > standard
}

/// V8 names a local time by the zone's daylight flag and its current metazone,
/// not by the instant: Europe/London in 1970 is "Greenwich Mean Time".
fn current_metazone_name(tz_id: &str, t: f64) -> Option<String> {
    let daylight = is_daylight(t);
    let standard = local_tza(REFERENCE_JANUARY_MS).min(local_tza(REFERENCE_JULY_MS));
    [REFERENCE_JANUARY_MS, REFERENCE_JULY_MS]
        .into_iter()
        .find(|&reference| (local_tza(reference) > standard) == daylight)
        .and_then(|reference| {
            long_time_zone_name(tz_id, reference, (local_tza(reference) / 1000.0) as i32)
        })
}

/// The parenthesised text of `Date.prototype.toString` and `toTimeString`
/// (TimeZoneString leaves it implementation-defined): the zone abbreviation,
/// or with `long` the CLDR long name V8 prints.
pub(crate) fn local_time_zone_name(t: f64, long: bool) -> String {
    if long && t.is_finite() {
        let tz_id = system_time_zone_identifier();
        let offset_secs = (local_tza(t) / 1000.0) as i32;
        let named = |name: Option<String>| name.filter(|n| !is_offset_form(n));
        let name = named(current_metazone_name(&tz_id, t))
            .or_else(|| named(long_time_zone_name(&tz_id, t, offset_secs)));
        if let Some(name) = name {
            return name;
        }
    }
    local_time_zone_abbreviation(t)
}

impl Interpreter {
    pub(crate) fn date_string(&self, t: f64) -> String {
        format_date_string(t, self.node_host_enabled)
    }

    pub(crate) fn time_string(&self, t: f64) -> String {
        format_time_only_string(t, self.node_host_enabled)
    }
}
