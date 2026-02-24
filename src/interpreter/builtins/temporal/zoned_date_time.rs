use super::instant::{bigint_to_f64, floor_div_bigint, is_valid_epoch_ns, to_bigint_arg};
use super::*;
use crate::interpreter::builtins::temporal::{
    get_prop, is_undefined, parse_overflow_option, read_month_fields, resolve_month_fields,
    temporal_unit_length_ns, temporal_unit_singular, to_temporal_time_zone_identifier,
    validate_calendar_strict, validate_timezone_identifier_strict,
};
use num_bigint::BigInt;

const NS_PER_MS: i128 = 1_000_000;
const NS_PER_SEC: i128 = 1_000_000_000;
const NS_PER_MIN: i128 = 60 * NS_PER_SEC;
const NS_PER_HOUR: i128 = 60 * NS_PER_MIN;
const NS_PER_DAY: i128 = 24 * NS_PER_HOUR;

/// Check if a monthCode string has valid syntax: M\d\d or M\d\dL
fn is_valid_month_code_syntax(mc: &str) -> bool {
    let b = mc.as_bytes();
    if b.len() < 3 || b[0] != b'M' {
        return false;
    }
    if !b[1].is_ascii_digit() || !b[2].is_ascii_digit() {
        return false;
    }
    if b.len() == 3 {
        return true;
    }
    if b.len() == 4 && b[3] == b'L' {
        return true;
    }
    false
}

/// Round an i128 nanosecond total to a given increment using the specified rounding mode.
fn round_ns_i128(total: i128, increment: i128, mode: &str) -> i128 {
    if increment == 0 || increment == 1 {
        return total;
    }
    let quotient = total / increment;
    let remainder = total % increment;
    if remainder == 0 {
        return total;
    }
    let rounded = match mode {
        "trunc" => quotient,
        "ceil" => {
            if remainder > 0 {
                quotient + 1
            } else {
                quotient
            }
        }
        "floor" => {
            if remainder < 0 {
                quotient - 1
            } else {
                quotient
            }
        }
        "expand" => {
            if total >= 0 {
                quotient + 1
            } else {
                quotient - 1
            }
        }
        "halfExpand" => {
            let abs_rem = remainder.unsigned_abs();
            let half = increment.unsigned_abs() / 2;
            let exact_half = increment.unsigned_abs().is_multiple_of(2) && abs_rem == half;
            if total >= 0 {
                if abs_rem > half || (exact_half && abs_rem >= half) {
                    quotient + 1
                } else {
                    quotient
                }
            } else if abs_rem > half || (exact_half && abs_rem >= half) {
                quotient - 1
            } else {
                quotient
            }
        }
        "halfTrunc" => {
            let abs_rem = remainder.unsigned_abs();
            let half = increment.unsigned_abs() / 2;
            if abs_rem > half {
                if total >= 0 {
                    quotient + 1
                } else {
                    quotient - 1
                }
            } else {
                quotient
            }
        }
        "halfCeil" => {
            let abs_rem = remainder.unsigned_abs();
            let half = increment.unsigned_abs() / 2;
            if abs_rem > half || (abs_rem == half && total >= 0) {
                if total >= 0 {
                    quotient + 1
                } else {
                    quotient - 1
                }
            } else {
                quotient
            }
        }
        "halfFloor" => {
            let abs_rem = remainder.unsigned_abs();
            let half = increment.unsigned_abs() / 2;
            if abs_rem > half || (abs_rem == half && total < 0) {
                if total >= 0 {
                    quotient + 1
                } else {
                    quotient - 1
                }
            } else {
                quotient
            }
        }
        "halfEven" => {
            let abs_rem = remainder.unsigned_abs();
            let half = increment.unsigned_abs() / 2;
            if abs_rem > half {
                if total >= 0 {
                    quotient + 1
                } else {
                    quotient - 1
                }
            } else if abs_rem == half {
                // Round to even quotient
                if quotient % 2 != 0 {
                    if total >= 0 {
                        quotient + 1
                    } else {
                        quotient - 1
                    }
                } else {
                    quotient
                }
            } else {
                quotient
            }
        }
        _ => quotient,
    };
    rounded * increment
}

fn get_zdt_fields(
    interp: &mut Interpreter,
    this: &JsValue,
) -> Result<(BigInt, String, String), Completion> {
    match this {
        JsValue::Object(o) => {
            let obj = interp.get_object(o.id).ok_or_else(|| {
                Completion::Throw(interp.create_type_error("invalid ZonedDateTime"))
            })?;
            let data = obj.borrow().temporal_data.clone();
            match data {
                Some(TemporalData::ZonedDateTime {
                    epoch_nanoseconds,
                    time_zone,
                    calendar,
                }) => Ok((epoch_nanoseconds, time_zone, calendar)),
                _ => Err(Completion::Throw(
                    interp.create_type_error("this is not a Temporal.ZonedDateTime"),
                )),
            }
        }
        _ => Err(Completion::Throw(
            interp.create_type_error("this is not a Temporal.ZonedDateTime"),
        )),
    }
}

/// Get UTC offset in nanoseconds for a timezone at a given epoch nanoseconds
fn get_tz_offset_ns(tz: &str, epoch_ns: &BigInt) -> i64 {
    // UTC and fixed offsets
    if tz == "UTC" || tz == "Etc/UTC" || tz == "Etc/GMT" {
        return 0;
    }
    if tz.starts_with('+') || tz.starts_with('-') {
        return parse_offset_to_ns(tz);
    }

    // IANA timezone — use chrono-tz
    use chrono::{Offset, TimeZone, Utc};
    use chrono_tz::Tz;
    // Use floor division to correctly handle negative epoch nanoseconds.
    // Truncation toward zero would place e.g. -59004000000000001ns in second
    // -59004000 instead of -59004001, giving the wrong offset near transitions.
    let ns_per_sec_bi = BigInt::from(NS_PER_SEC);
    let zero = BigInt::from(0i64);
    let q = epoch_ns / &ns_per_sec_bi;
    let r = epoch_ns % &ns_per_sec_bi;
    let (epoch_secs_bi, rem_bi) = if r < zero {
        (q - BigInt::from(1i64), r + &ns_per_sec_bi)
    } else {
        (q, r)
    };
    let epoch_secs: i64 = epoch_secs_bi.try_into().unwrap_or(0);
    let nanos: u32 = {
        let r: i64 = rem_bi.try_into().unwrap_or(0);
        r as u32
    };

    if let Ok(tz_parsed) = tz.parse::<Tz>() {
        let utc_dt = Utc.timestamp_opt(epoch_secs, nanos).single();
        if let Some(dt) = utc_dt {
            let offset = dt.with_timezone(&tz_parsed).offset().fix();
            return offset.local_minus_utc() as i64 * NS_PER_SEC as i64;
        }
    }
    0
}

/// Return all possible epoch nanoseconds for a given wall-clock time in a timezone.
/// For normal times there is one result; for DST overlaps there are two (sorted chronologically).
/// For DST gaps the result is empty.
pub(crate) fn get_possible_epoch_ns(tz: &str, local_ns: i128) -> Vec<i128> {
    if tz == "UTC" || tz == "Etc/UTC" || tz == "Etc/GMT" {
        return vec![local_ns];
    }
    if tz.starts_with('+') || tz.starts_with('-') {
        let off = parse_offset_to_ns(tz) as i128;
        return vec![local_ns - off];
    }

    use chrono::{NaiveDateTime, TimeZone, Offset, MappedLocalTime};
    use chrono_tz::Tz;

    let tz_parsed = match tz.parse::<Tz>() {
        Ok(t) => t,
        Err(_) => {
            let off = get_tz_offset_ns(tz, &BigInt::from(local_ns)) as i128;
            return vec![local_ns - off];
        }
    };

    let epoch_secs = local_ns.div_euclid(NS_PER_SEC) as i64;
    let sub_sec_ns = local_ns.rem_euclid(NS_PER_SEC);
    let nanos = sub_sec_ns as u32;

    let naive = match NaiveDateTime::from_timestamp_opt(epoch_secs, nanos) {
        Some(dt) => dt,
        None => {
            let off = get_tz_offset_ns(tz, &BigInt::from(local_ns)) as i128;
            return vec![local_ns - off];
        }
    };

    match tz_parsed.from_local_datetime(&naive) {
        MappedLocalTime::Single(dt) => {
            let off = dt.offset().fix().local_minus_utc() as i128 * NS_PER_SEC as i128;
            vec![local_ns - off]
        }
        MappedLocalTime::Ambiguous(dt1, dt2) => {
            let off1 = dt1.offset().fix().local_minus_utc() as i128 * NS_PER_SEC as i128;
            let off2 = dt2.offset().fix().local_minus_utc() as i128 * NS_PER_SEC as i128;
            let e1 = local_ns - off1;
            let e2 = local_ns - off2;
            let mut results = vec![e1, e2];
            results.sort();
            results.dedup();
            results
        }
        MappedLocalTime::None => {
            vec![]
        }
    }
}

/// Check if a parsed offset matches an actual offset, with optional minute-rounding.
fn is_offset_match(parsed_ns: i128, actual_ns: i128, sub_minute: bool) -> bool {
    if sub_minute {
        parsed_ns == actual_ns
    } else {
        let round_min = |ns: i128| -> i128 {
            let sign = if ns < 0 { -1 } else { 1 };
            let abs = ns.unsigned_abs();
            sign * ((abs + NS_PER_MIN as u128 / 2) / NS_PER_MIN as u128) as i128
        };
        round_min(parsed_ns) == round_min(actual_ns)
    }
}

/// Find the first possible epoch ns whose tz offset fuzzy-matches the parsed offset.
/// Returns None if no candidate matches.
fn offset_match_candidates(tz: &str, local_ns: i128, off_ns: i128, sub_minute: bool) -> Option<BigInt> {
    let candidates = get_possible_epoch_ns(tz, local_ns);
    for candidate in &candidates {
        let actual_off = get_tz_offset_ns(tz, &BigInt::from(*candidate)) as i128;
        if is_offset_match(off_ns, actual_off, sub_minute) {
            return Some(BigInt::from(*candidate));
        }
    }
    None
}

/// Find the first matching candidate or throw RangeError.
fn offset_match_or_reject(
    interp: &mut Interpreter,
    tz: &str,
    local_ns: i128,
    off_ns: i128,
    sub_minute: bool,
) -> Result<BigInt, Completion> {
    match offset_match_candidates(tz, local_ns, off_ns, sub_minute) {
        Some(ns) => Ok(ns),
        None => Err(Completion::Throw(
            interp.create_range_error("UTC offset mismatch with time zone"),
        )),
    }
}

/// Disambiguate a local wall-clock time to a single UTC epoch nanosecond.
/// `mode`: "compatible" | "earlier" | "later"
pub(crate) fn disambiguate_instant(tz: &str, local_ns: i128, mode: &str) -> i128 {
    let candidates = get_possible_epoch_ns(tz, local_ns);
    match candidates.len() {
        0 => {
            // Gap: per spec DisambiguatePossibleEpochNanoseconds.
            // Find offsetBefore and offsetAfter by probing on each side of the gap.
            // Use local_ns as an approximate UTC to find the two offsets around transition.
            let off_a = get_tz_offset_ns(tz, &BigInt::from(local_ns)) as i128;
            let epoch_a = local_ns - off_a;
            let off_b = get_tz_offset_ns(tz, &BigInt::from(epoch_a)) as i128;

            // Determine which offset is before/after the transition
            let (offset_before, offset_after) = if off_a < off_b {
                (off_a, off_b)
            } else {
                (off_b, off_a)
            };
            let nanoseconds = offset_after - offset_before;

            if mode == "earlier" {
                // Shift requested time backward by nanoseconds, then resolve
                let earlier_local = local_ns - nanoseconds;
                let earlier_candidates = get_possible_epoch_ns(tz, earlier_local);
                if !earlier_candidates.is_empty() {
                    earlier_candidates[0]
                } else {
                    // Fallback: use pre-transition offset
                    local_ns - offset_before
                }
            } else {
                // "compatible" or "later": shift forward by nanoseconds, then resolve
                let later_local = local_ns + nanoseconds;
                let later_candidates = get_possible_epoch_ns(tz, later_local);
                if !later_candidates.is_empty() {
                    *later_candidates.last().unwrap()
                } else {
                    // Fallback: use post-transition offset
                    local_ns - offset_after
                }
            }
        }
        1 => candidates[0],
        _ => {
            // Overlap: candidates are sorted chronologically
            if mode == "later" {
                *candidates.last().unwrap()
            } else {
                // "compatible" or "earlier": use the first (earlier) instant
                candidates[0]
            }
        }
    }
}

/// Find the start of day per spec GetStartOfDay:
/// Try midnight; if it exists, return earliest instant. If gap, find first valid local time.
pub(crate) fn get_start_of_day(tz: &str, epoch_days: i128) -> i128 {
    let local_midnight = epoch_days * NS_PER_DAY;
    let candidates = get_possible_epoch_ns(tz, local_midnight);
    if !candidates.is_empty() {
        return candidates[0];
    }
    // Midnight is in a gap. Binary search for the first local time that resolves.
    // The transition is within this day. Search forward from midnight.
    let off_a = get_tz_offset_ns(tz, &BigInt::from(local_midnight)) as i128;
    let epoch_a = local_midnight - off_a;
    let off_b = get_tz_offset_ns(tz, &BigInt::from(epoch_a)) as i128;

    // The two candidate instants bracket the transition
    let (mut lo, mut hi) = if epoch_a < local_midnight - off_b {
        (epoch_a, local_midnight - off_b)
    } else {
        (local_midnight - off_b, epoch_a)
    };

    // Binary search for exact transition nanosecond
    let lo_off = get_tz_offset_ns(tz, &BigInt::from(lo)) as i128;
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        let mid_off = get_tz_offset_ns(tz, &BigInt::from(mid)) as i128;
        if mid_off == lo_off {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi
}

/// Get total UTC offset in seconds at a given UTC epoch second.
/// Uses OffsetComponents to get base_utc_offset + dst_offset.
fn get_total_offset_secs(tz_parsed: &chrono_tz::Tz, epoch_secs: i64) -> i32 {
    use chrono::{NaiveDateTime, TimeZone};
    use chrono_tz::OffsetComponents;
    let dt = NaiveDateTime::from_timestamp_opt(epoch_secs, 0)
        .unwrap_or_else(|| NaiveDateTime::from_timestamp_opt(0, 0).unwrap());
    let utc_dt = dt.and_utc();
    let tz_dt = utc_dt.with_timezone(tz_parsed);
    let offset = tz_dt.offset();
    (offset.base_utc_offset().num_seconds() + offset.dst_offset().num_seconds()) as i32
}

/// Find the exact transition point (in nanoseconds) between lo_ns and hi_ns,
/// where offsets are known to differ. Binary search at second granularity.
fn find_exact_transition(tz_parsed: &chrono_tz::Tz, lo_ns: i128, hi_ns: i128) -> i128 {
    let mut lo = lo_ns;
    let mut hi = hi_ns;
    let lo_off = get_total_offset_secs(tz_parsed, (lo / NS_PER_SEC) as i64);
    while hi - lo > NS_PER_SEC {
        let mid = lo + (hi - lo) / 2;
        let mid_sec = (mid / NS_PER_SEC) * NS_PER_SEC;
        let mid_off = get_total_offset_secs(tz_parsed, (mid_sec / NS_PER_SEC) as i64);
        if mid_off == lo_off {
            lo = mid_sec;
        } else {
            hi = mid_sec;
        }
    }
    hi
}

/// Scan forward day-by-day from start_ns to find the first offset transition.
/// Returns None if no transition found before limit_ns.
/// Find the next UTC offset transition strictly after epoch_ns.
/// First window uses 1-day steps (catches close-together transitions near start).
/// Subsequent windows use 90-day coarse steps for fast far-distance scanning.
fn get_next_transition(tz: &str, epoch_ns: i128) -> Option<i128> {
    use chrono_tz::Tz;

    let ns_max: i128 = 8_640_000_000_000_000_000_000;
    let tz_parsed: Tz = tz.parse().ok()?;

    let ref_sec = epoch_ns.div_euclid(NS_PER_SEC);
    let start_ns = (ref_sec + 1) * NS_PER_SEC;
    if start_ns > ns_max {
        return None;
    }

    // Check if there's a transition at the boundary between the current second and the next
    let ref_off = get_total_offset_secs(&tz_parsed, ref_sec as i64);
    let start_off = get_total_offset_secs(&tz_parsed, (start_ns / NS_PER_SEC) as i64);
    if ref_off != start_off {
        return Some(start_ns);
    }

    let coarse = NS_PER_DAY * 90;
    let mut prev_ns = start_ns;
    let mut prev_off = start_off;
    let mut step = coarse;
    let mut first_window = true;

    loop {
        let window_end = (prev_ns + step).min(ns_max);
        let inner_step = if first_window { NS_PER_DAY } else { coarse };
        let mut check = prev_ns;
        let mut check_off = prev_off;

        while check < window_end {
            let next_check = (check + inner_step).min(window_end);
            let next_check_off =
                get_total_offset_secs(&tz_parsed, (next_check / NS_PER_SEC) as i64);
            if next_check_off != check_off {
                if inner_step <= NS_PER_DAY {
                    return Some(find_exact_transition(&tz_parsed, check, next_check));
                }
                // Coarse bracket — day-scan within it
                let mut scan = check;
                let mut scan_off = check_off;
                while scan < next_check {
                    let ns = (scan + NS_PER_DAY).min(next_check);
                    let ns_off =
                        get_total_offset_secs(&tz_parsed, (ns / NS_PER_SEC) as i64);
                    if ns_off != scan_off {
                        return Some(find_exact_transition(&tz_parsed, scan, ns));
                    }
                    scan = ns;
                    scan_off = ns_off;
                }
            }
            check = next_check;
            check_off = next_check_off;
        }

        if window_end >= ns_max {
            return None;
        }
        prev_ns = window_end;
        prev_off = check_off;
        first_window = false;
        step = (step * 2).min(NS_PER_DAY * 36500);
    }
}

/// Find the previous UTC offset transition strictly before epoch_ns.
/// First window uses 1-day steps (catches close-together transitions near start).
/// Subsequent windows use 90-day coarse steps for fast far-distance scanning.
fn get_previous_transition(tz: &str, epoch_ns: i128) -> Option<i128> {
    use chrono_tz::Tz;

    let ns_min: i128 = -8_640_000_000_000_000_000_000;
    let tz_parsed: Tz = tz.parse().ok()?;

    let ref_sec = (epoch_ns - 1).div_euclid(NS_PER_SEC);
    let ref_ns = ref_sec * NS_PER_SEC;
    if ref_ns < ns_min {
        return None;
    }

    let coarse = NS_PER_DAY * 90;
    let mut next_ns = ref_ns;
    let mut next_off = get_total_offset_secs(&tz_parsed, ref_sec as i64);
    let mut step = coarse;
    let mut first_window = true;

    loop {
        let window_start = (next_ns - step).max(ns_min);
        let inner_step = if first_window { NS_PER_DAY } else { coarse };
        let mut check = next_ns;
        let mut check_off = next_off;

        while check > window_start {
            let prev_check = (check - inner_step).max(window_start);
            let prev_check_off =
                get_total_offset_secs(&tz_parsed, (prev_check / NS_PER_SEC) as i64);
            if prev_check_off != check_off {
                if inner_step <= NS_PER_DAY {
                    return Some(find_exact_transition(&tz_parsed, prev_check, check));
                }
                // Coarse bracket — day-scan backward within it
                let mut scan = check;
                let mut scan_off = check_off;
                while scan > prev_check {
                    let ns = (scan - NS_PER_DAY).max(prev_check);
                    let ns_off =
                        get_total_offset_secs(&tz_parsed, (ns / NS_PER_SEC) as i64);
                    if ns_off != scan_off {
                        return Some(find_exact_transition(&tz_parsed, ns, scan));
                    }
                    scan = ns;
                    scan_off = ns_off;
                }
            }
            check = prev_check;
            check_off = prev_check_off;
        }

        if window_start <= ns_min {
            return None;
        }
        next_ns = window_start;
        next_off = check_off;
        first_window = false;
        step = (step * 2).min(NS_PER_DAY * 36500);
    }
}

fn parse_offset_to_ns(s: &str) -> i64 {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return 0;
    }
    let sign: i64 = if bytes[0] == b'-' { -1 } else { 1 };
    let rest = &s[1..];
    let parts: Vec<&str> = rest.split(':').collect();
    let h: i64 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let m: i64 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    sign * (h * NS_PER_HOUR as i64 + m * NS_PER_MIN as i64)
}

/// Decompose epoch nanoseconds + tz offset → date/time components
pub(super) fn epoch_ns_to_components(
    epoch_ns: &BigInt,
    tz: &str,
) -> (i32, u8, u8, u8, u8, u8, u16, u16, u16) {
    let offset_ns = get_tz_offset_ns(tz, epoch_ns);
    let total_ns: i128 = epoch_ns.try_into().unwrap_or(0);
    let local_ns = total_ns + offset_ns as i128;

    let epoch_days = local_ns.div_euclid(NS_PER_DAY);
    let day_ns = local_ns.rem_euclid(NS_PER_DAY);

    let (year, month, day) = super::epoch_days_to_iso_date(epoch_days as i64);
    let nanosecond = (day_ns % 1_000) as u16;
    let microsecond = ((day_ns / 1_000) % 1_000) as u16;
    let millisecond = ((day_ns / 1_000_000) % 1_000) as u16;
    let second = ((day_ns / NS_PER_SEC) % 60) as u8;
    let minute = ((day_ns / NS_PER_MIN) % 60) as u8;
    let hour = ((day_ns / NS_PER_HOUR) % 24) as u8;

    (
        year,
        month,
        day,
        hour,
        minute,
        second,
        millisecond,
        microsecond,
        nanosecond,
    )
}

fn format_offset_string(offset_ns: i64) -> String {
    let sign = if offset_ns >= 0 { '+' } else { '-' };
    let abs = offset_ns.unsigned_abs() as i64;
    let h = abs / NS_PER_HOUR as i64;
    let m = (abs / NS_PER_MIN as i64) % 60;
    let s = (abs / NS_PER_SEC as i64) % 60;
    let ns_rem = abs % NS_PER_SEC as i64;
    if ns_rem != 0 {
        let frac = format!("{ns_rem:09}");
        let trimmed = frac.trim_end_matches('0');
        format!("{sign}{h:02}:{m:02}:{s:02}.{trimmed}")
    } else if s != 0 {
        format!("{sign}{h:02}:{m:02}:{s:02}")
    } else {
        format!("{sign}{h:02}:{m:02}")
    }
}

// FormatDateTimeUTCOffsetRounded: rounds offset to nearest minute
fn format_offset_string_rounded(offset_ns: i64) -> String {
    let ns_per_min = NS_PER_MIN as i64;
    // RoundNumberToIncrement(offset_ns, 60e9, half-expand)
    let rounded = if offset_ns >= 0 {
        (offset_ns + ns_per_min / 2) / ns_per_min * ns_per_min
    } else {
        -(((-offset_ns) + ns_per_min / 2) / ns_per_min * ns_per_min)
    };
    let sign = if rounded >= 0 { '+' } else { '-' };
    let abs_min = (rounded.unsigned_abs() as i64) / ns_per_min;
    let h = abs_min / 60;
    let m = abs_min % 60;
    format!("{sign}{h:02}:{m:02}")
}

fn create_zdt(interp: &mut Interpreter, ns: BigInt, tz: String, cal: String) -> Completion {
    if !is_valid_epoch_ns(&ns) {
        return Completion::Throw(interp.create_range_error("epochNanoseconds out of range"));
    }
    let obj = interp.create_object();
    obj.borrow_mut().class_name = "Temporal.ZonedDateTime".to_string();
    if let Some(ref proto) = interp.realm().temporal_zoned_date_time_prototype {
        obj.borrow_mut().prototype = Some(proto.clone());
    }
    obj.borrow_mut().temporal_data = Some(TemporalData::ZonedDateTime {
        epoch_nanoseconds: ns,
        time_zone: tz,
        calendar: cal,
    });
    let id = obj.borrow().id.unwrap();
    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
}

pub(super) fn get_tz_offset_ns_pub(tz: &str, epoch_ns: &BigInt) -> i64 {
    get_tz_offset_ns(tz, epoch_ns)
}

pub(super) fn create_zdt_pub(
    interp: &mut Interpreter,
    ns: BigInt,
    tz: String,
    cal: String,
) -> Completion {
    create_zdt(interp, ns, tz, cal)
}

pub(super) fn to_temporal_zoned_date_time(interp: &mut Interpreter, item: &JsValue) -> Completion {
    to_temporal_zoned_date_time_with_options(
        interp,
        item,
        "constrain",
        "compatible",
        "reject",
        None,
    )
}

/// If `deferred_options` is Some((raw_options, default_offset)), options are read from
/// raw_options AFTER property bag fields (for observable order in ZDT.from).
/// When None, the pre-parsed overflow/disambiguation/offset_option are used.
fn to_temporal_zoned_date_time_with_options(
    interp: &mut Interpreter,
    item: &JsValue,
    overflow: &str,
    disambiguation: &str,
    offset_option: &str,
    deferred_options: Option<(&JsValue, &str)>,
) -> Completion {
    match item {
        JsValue::Object(o) => {
            let obj = match interp.get_object(o.id) {
                Some(o) => o,
                None => return Completion::Throw(interp.create_type_error("invalid object")),
            };
            let data = obj.borrow().temporal_data.clone();
            if let Some(TemporalData::ZonedDateTime {
                epoch_nanoseconds,
                time_zone,
                calendar,
            }) = data
            {
                return create_zdt(interp, epoch_nanoseconds, time_zone, calendar);
            }
            // Property bag — read all fields in alphabetical order per spec
            // 1. calendar
            let cal_val = match get_prop(interp, item, "calendar") {
                Completion::Normal(v) => v,
                c => return c,
            };
            let calendar = match super::to_temporal_calendar_slot_value(interp, &cal_val) {
                Ok(c) => c,
                Err(c) => return c,
            };

            // 2. day (required)
            let d_val = match get_prop(interp, item, "day") {
                Completion::Normal(v) => v,
                c => return c,
            };
            let has_day = !is_undefined(&d_val);
            let day_i = if has_day {
                match to_integer_with_truncation(interp, &d_val) {
                    Ok(n) => n,
                    Err(c) => return c,
                }
            } else {
                0.0
            };

            // 3. hour (default 0)
            let hour_raw = match get_time_field(interp, item, "hour") {
                Ok(v) => v as i32,
                Err(c) => return c,
            };

            // 4. microsecond (default 0)
            let microsecond_raw = match get_time_field(interp, item, "microsecond") {
                Ok(v) => v as i32,
                Err(c) => return c,
            };

            // 5. millisecond (default 0)
            let millisecond_raw = match get_time_field(interp, item, "millisecond") {
                Ok(v) => v as i32,
                Err(c) => return c,
            };

            // 6. minute (default 0)
            let minute_raw = match get_time_field(interp, item, "minute") {
                Ok(v) => v as i32,
                Err(c) => return c,
            };

            // 7. month (optional, coerce if defined)
            let m_val = match get_prop(interp, item, "month") {
                Completion::Normal(v) => v,
                c => return c,
            };
            let has_month = !is_undefined(&m_val);
            let month_coerced: Option<i32> = if has_month {
                Some(match to_integer_with_truncation(interp, &m_val) {
                    Ok(n) => n as i32,
                    Err(c) => return c,
                })
            } else {
                None
            };

            // 8. monthCode (optional, coerce + SYNTAX validate immediately)
            let mc_val = match get_prop(interp, item, "monthCode") {
                Completion::Normal(v) => v,
                c => return c,
            };
            let has_month_code = !is_undefined(&mc_val);
            let month_code_str: Option<String> = if has_month_code {
                let mc = match super::to_primitive_and_require_string(interp, &mc_val, "monthCode")
                {
                    Ok(s) => s,
                    Err(c) => return c,
                };
                if !is_valid_month_code_syntax(&mc) {
                    return Completion::Throw(
                        interp.create_range_error(&format!("Invalid monthCode: {mc}")),
                    );
                }
                Some(mc)
            } else {
                None
            };

            // 9. nanosecond (default 0)
            let nanosecond_raw = match get_time_field(interp, item, "nanosecond") {
                Ok(v) => v as i32,
                Err(c) => return c,
            };

            // 10. offset (optional, ToPrimitiveAndRequireString + validate syntax immediately)
            let offset_val = match get_prop(interp, item, "offset") {
                Completion::Normal(v) => v,
                c => return c,
            };
            let bag_offset_ns: Option<i64> = if is_undefined(&offset_val) {
                None
            } else {
                let os = match super::to_primitive_and_require_string(interp, &offset_val, "offset")
                {
                    Ok(s) => s,
                    Err(c) => return c,
                };
                match super::parse_utc_offset_timezone(&os) {
                    Some(normalized) => Some(parse_offset_to_ns(&normalized)),
                    None => {
                        return Completion::Throw(
                            interp.create_range_error(&format!("invalid offset string: {os}")),
                        );
                    }
                }
            };

            // 11. second (default 0)
            let second_raw = match get_time_field(interp, item, "second") {
                Ok(v) => v as i32,
                Err(c) => return c,
            };

            // 12. timeZone (required)
            let tz_val = match get_prop(interp, item, "timeZone") {
                Completion::Normal(v) => v,
                c => return c,
            };
            if is_undefined(&tz_val) {
                return Completion::Throw(
                    interp.create_type_error("timeZone is required for ZonedDateTime property bag"),
                );
            }
            let tz = match to_temporal_time_zone_identifier(interp, &tz_val) {
                Ok(t) => t,
                Err(c) => return c,
            };

            // 13. year (required, unless era+eraYear provided for era calendars)
            let y_val = match get_prop(interp, item, "year") {
                Completion::Normal(v) => v,
                c => return c,
            };
            let year = if !is_undefined(&y_val) {
                match to_integer_with_truncation(interp, &y_val) {
                    Ok(n) => n as i32,
                    Err(c) => return c,
                }
            } else if calendar != "iso8601" && super::calendar_has_eras(&calendar) {
                // Check for era+eraYear
                let era_check = match get_prop(interp, item, "era") {
                    Completion::Normal(v) => v,
                    c => return c,
                };
                let era_year_check = match get_prop(interp, item, "eraYear") {
                    Completion::Normal(v) => v,
                    c => return c,
                };
                if !is_undefined(&era_check) && !is_undefined(&era_year_check) {
                    0 // placeholder — will be overridden by era+eraYear later
                } else {
                    return Completion::Throw(interp.create_type_error("year is required"));
                }
            } else {
                return Completion::Throw(interp.create_type_error("year is required"));
            };

            // If deferred options, read them now (after all bag field reads)
            let (eff_overflow, eff_disambiguation, eff_offset_option) =
                if let Some((opts, default_off)) = deferred_options {
                    match parse_zdt_options(interp, opts, default_off) {
                        Ok((d, o, ovf)) => (ovf, d, o),
                        Err(c) => return c,
                    }
                } else {
                    (
                        overflow.to_string(),
                        disambiguation.to_string(),
                        offset_option.to_string(),
                    )
                };
            let overflow = &eff_overflow;
            let _disambiguation = &eff_disambiguation;
            let offset_option = &eff_offset_option;

            // --- Validation (after all reads) ---
            if !has_day {
                return Completion::Throw(interp.create_type_error("day is required"));
            }

            // Non-ISO calendar: convert calendar fields to ISO via ICU4X
            // This must happen before ISO month resolution since non-ISO calendars
            // can have M13 (Coptic/Ethiopian), M01L (Chinese/Hebrew leap), etc.
            if calendar != "iso8601" {
                if day_i < 1.0 {
                    return Completion::Throw(interp.create_range_error("day out of range"));
                }

                let era_val = match get_prop(interp, item, "era") {
                    Completion::Normal(v) => v,
                    c => return c,
                };
                let era_year_val = match get_prop(interp, item, "eraYear") {
                    Completion::Normal(v) => v,
                    c => return c,
                };
                let (icu_era, icu_year) = if !is_undefined(&era_val) && !is_undefined(&era_year_val)
                    && super::calendar_has_eras(&calendar)
                {
                    let era_str = match super::to_primitive_and_require_string(interp, &era_val, "era") {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let ey = match to_integer_with_truncation(interp, &era_year_val) {
                        Ok(v) => v as i32,
                        Err(c) => return c,
                    };
                    (Some(era_str), ey)
                } else {
                    (None, year)
                };
                let mc_opt = month_code_str.as_deref();
                let mo_opt = if has_month { month_coerced.map(|m| m as u8) } else { None };

                // Require month or monthCode
                if mc_opt.is_none() && mo_opt.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("month or monthCode is required"),
                    );
                }

                match super::calendar_fields_to_iso_overflow(
                    icu_era.as_deref(), icu_year, mc_opt, mo_opt, day_i as u8, &calendar, &overflow,
                ) {
                    Some((iy, im, id)) => {
                        // Jump to the final ZDT construction with ISO values
                        let year = iy;
                        let month_raw = im as i32;
                        let day_i = id as f64;

                        // Validate time fields
                        let (hour_i, minute_i, second_i, ms_i, us_i, ns_i) = if overflow == "reject" {
                            if hour_raw < 0 || hour_raw > 23 {
                                return Completion::Throw(interp.create_range_error("hour out of range"));
                            }
                            if minute_raw < 0 || minute_raw > 59 {
                                return Completion::Throw(interp.create_range_error("minute out of range"));
                            }
                            if second_raw < 0 || second_raw > 59 {
                                return Completion::Throw(interp.create_range_error("second out of range"));
                            }
                            if millisecond_raw < 0 || millisecond_raw > 999 {
                                return Completion::Throw(interp.create_range_error("millisecond out of range"));
                            }
                            if microsecond_raw < 0 || microsecond_raw > 999 {
                                return Completion::Throw(interp.create_range_error("microsecond out of range"));
                            }
                            if nanosecond_raw < 0 || nanosecond_raw > 999 {
                                return Completion::Throw(interp.create_range_error("nanosecond out of range"));
                            }
                            (hour_raw, minute_raw, second_raw, millisecond_raw, microsecond_raw, nanosecond_raw)
                        } else {
                            (
                                hour_raw.clamp(0, 23), minute_raw.clamp(0, 59),
                                second_raw.clamp(0, 59), millisecond_raw.clamp(0, 999),
                                microsecond_raw.clamp(0, 999), nanosecond_raw.clamp(0, 999),
                            )
                        };

                        let epoch_days = super::iso_date_to_epoch_days(year, month_raw as u8, day_i as u8) as i128;
                        let day_ns = hour_i as i128 * NS_PER_HOUR
                            + minute_i as i128 * NS_PER_MIN
                            + second_i as i128 * NS_PER_SEC
                            + ms_i as i128 * NS_PER_MS
                            + us_i as i128 * 1_000
                            + ns_i as i128;
                        let local_ns = epoch_days * NS_PER_DAY + day_ns;

                        let offset_ns = if offset_option == "use" {
                            if let Some(off_ns) = bag_offset_ns {
                                off_ns as i128
                            } else {
                                get_tz_offset_ns(&tz, &BigInt::from(local_ns)) as i128
                            }
                        } else {
                            get_tz_offset_ns(&tz, &BigInt::from(local_ns)) as i128
                        };
                        let epoch_ns = BigInt::from(local_ns - offset_ns);
                        return create_zdt(interp, epoch_ns, tz, calendar);
                    }
                    None => {
                        return Completion::Throw(
                            interp.create_range_error("Invalid calendar date for ZonedDateTime"),
                        );
                    }
                }
            }

            // ISO path: resolve month/monthCode
            let month_raw: i32 = if let Some(ref mc) = month_code_str {
                match super::plain_date::month_code_to_number_pub(mc) {
                    Some(n) => {
                        if let Some(explicit_m) = month_coerced
                            && explicit_m != n as i32
                        {
                            return Completion::Throw(
                                interp.create_range_error("month and monthCode conflict"),
                            );
                        }
                        n as i32
                    }
                    None => {
                        return Completion::Throw(
                            interp.create_range_error(&format!("Invalid monthCode: {mc}")),
                        );
                    }
                }
            } else if let Some(m) = month_coerced {
                m
            } else {
                return Completion::Throw(
                    interp.create_type_error("month or monthCode is required"),
                );
            };

            if month_raw < 1 {
                return Completion::Throw(interp.create_range_error("month out of range"));
            }
            if day_i < 1.0 {
                return Completion::Throw(interp.create_range_error("day out of range"));
            }

            // Apply overflow to all fields
            let (month, day, hour, minute, second, millisecond, microsecond, nanosecond) =
                if overflow == "reject" {
                    if month_raw > 12 {
                        return Completion::Throw(interp.create_range_error("month out of range"));
                    }
                    if day_i > super::iso_days_in_month(year, month_raw as u8) as f64 {
                        return Completion::Throw(interp.create_range_error("day out of range"));
                    }
                    if !(0..=23).contains(&hour_raw) {
                        return Completion::Throw(interp.create_range_error("hour out of range"));
                    }
                    if !(0..=59).contains(&minute_raw) {
                        return Completion::Throw(interp.create_range_error("minute out of range"));
                    }
                    if !(0..=59).contains(&second_raw) {
                        return Completion::Throw(interp.create_range_error("second out of range"));
                    }
                    if !(0..=999).contains(&millisecond_raw) {
                        return Completion::Throw(
                            interp.create_range_error("millisecond out of range"),
                        );
                    }
                    if !(0..=999).contains(&microsecond_raw) {
                        return Completion::Throw(
                            interp.create_range_error("microsecond out of range"),
                        );
                    }
                    if !(0..=999).contains(&nanosecond_raw) {
                        return Completion::Throw(
                            interp.create_range_error("nanosecond out of range"),
                        );
                    }
                    (
                        month_raw as u8,
                        day_i as u8,
                        hour_raw as u8,
                        minute_raw as u8,
                        second_raw as u8,
                        millisecond_raw as u16,
                        microsecond_raw as u16,
                        nanosecond_raw as u16,
                    )
                } else {
                    // constrain: clamp all fields to valid ranges
                    let month = month_raw.clamp(1, 12) as u8;
                    let max_day = super::iso_days_in_month(year, month);
                    let day = (day_i as i32).clamp(1, max_day as i32) as u8;
                    let hour = hour_raw.clamp(0, 23) as u8;
                    let minute = minute_raw.clamp(0, 59) as u8;
                    let second = second_raw.clamp(0, 59) as u8;
                    let millisecond = millisecond_raw.clamp(0, 999) as u16;
                    let microsecond = microsecond_raw.clamp(0, 999) as u16;
                    let nanosecond = nanosecond_raw.clamp(0, 999) as u16;
                    (
                        month,
                        day,
                        hour,
                        minute,
                        second,
                        millisecond,
                        microsecond,
                        nanosecond,
                    )
                };

            // Convert local datetime to epoch nanoseconds
            let epoch_days = super::iso_date_to_epoch_days(year, month, day) as i128;
            let day_ns = hour as i128 * NS_PER_HOUR
                + minute as i128 * NS_PER_MIN
                + second as i128 * NS_PER_SEC
                + millisecond as i128 * NS_PER_MS
                + microsecond as i128 * 1_000
                + nanosecond as i128;
            let local_ns = epoch_days * NS_PER_DAY + day_ns;

            // Compute epoch ns considering offset option
            let approx_ns = BigInt::from(local_ns);
            let tz_offset = get_tz_offset_ns(&tz, &approx_ns) as i128;

            let effective_offset = match offset_option.as_str() {
                "use" => bag_offset_ns.map(|o| o as i128).unwrap_or(tz_offset),
                "ignore" => tz_offset,
                "reject" => {
                    if let Some(bag_ns) = bag_offset_ns
                        && bag_ns as i128 != tz_offset
                    {
                        return Completion::Throw(
                            interp.create_range_error("offset does not agree with time zone"),
                        );
                    }
                    tz_offset
                }
                "prefer" => {
                    if let Some(bag_ns) = bag_offset_ns {
                        let candidate = BigInt::from(local_ns - bag_ns as i128);
                        let actual_offset = get_tz_offset_ns(&tz, &candidate) as i128;
                        if actual_offset == bag_ns as i128 {
                            bag_ns as i128
                        } else {
                            tz_offset
                        }
                    } else {
                        tz_offset
                    }
                }
                _ => tz_offset,
            };

            let epoch_ns = BigInt::from(local_ns - effective_offset);
            create_zdt(interp, epoch_ns, tz, calendar)
        }
        JsValue::String(s) => {
            let s_str = s.to_string();
            match super::parse_temporal_date_time_string(&s_str) {
                Some(parsed) => {
                    // If there's an offset (Z, ±HH:MM), time is required
                    if parsed.offset.is_some() && !parsed.has_time {
                        return Completion::Throw(interp.create_range_error(
                            "UTC offset without time is not valid for ZonedDateTime",
                        ));
                    }
                    // Must have timezone annotation (e.g. [UTC], [+01:00])
                    let tz = if let Some(ref tz_ann) = parsed.time_zone {
                        // Validate the annotation is a valid timezone
                        match super::parse_temporal_time_zone_string(tz_ann) {
                            Some(tz) => tz,
                            None => {
                                return Completion::Throw(interp.create_range_error(&format!(
                                    "Invalid timezone annotation: {tz_ann}"
                                )));
                            }
                        }
                    } else {
                        // Bare offset or no timezone — not a valid ZDT string
                        return Completion::Throw(interp.create_range_error(
                            "ZonedDateTime string requires a timezone annotation (e.g. [UTC])",
                        ));
                    };

                    let cal_raw = parsed.calendar.unwrap_or_else(|| "iso8601".to_string());
                    let cal = match super::validate_calendar(&cal_raw) {
                        Some(c) => c,
                        None => {
                            return Completion::Throw(
                                interp.create_range_error(&format!("Invalid calendar: {cal_raw}")),
                            );
                        }
                    };
                    // Compute epoch ns from date/time + offset
                    let epoch_days =
                        super::iso_date_to_epoch_days(parsed.year, parsed.month, parsed.day)
                            as i128;
                    let day_ns = parsed.hour as i128 * NS_PER_HOUR
                        + parsed.minute as i128 * NS_PER_MIN
                        + parsed.second as i128 * NS_PER_SEC
                        + parsed.millisecond as i128 * NS_PER_MS
                        + parsed.microsecond as i128 * 1_000
                        + parsed.nanosecond as i128;
                    let local_ns = epoch_days * NS_PER_DAY + day_ns;

                    // If offset is provided, use it to compute exact time
                    let epoch_ns = if let Some(ref off) = parsed.offset {
                        let off_ns = off.sign as i128
                            * (off.hours as i128 * NS_PER_HOUR
                                + off.minutes as i128 * NS_PER_MIN
                                + off.seconds as i128 * NS_PER_SEC
                                + off.nanoseconds as i128);
                        let exact_ns = local_ns - off_ns;
                        let tz_off = get_tz_offset_ns(&tz, &BigInt::from(exact_ns)) as i128;

                        match offset_option {
                            "reject" => {
                                if !parsed.has_utc_designator && epoch_days.abs() > 100_000_000 {
                                    return Completion::Throw(interp.create_range_error(
                                        "ZonedDateTime is outside the representable range",
                                    ));
                                }
                                if parsed.has_utc_designator {
                                    BigInt::from(exact_ns)
                                } else {
                                    match offset_match_or_reject(interp, &tz, local_ns, off_ns, off.has_sub_minute) {
                                        Ok(ns) => ns,
                                        Err(c) => return c,
                                    }
                                }
                            }
                            "use" => {
                                // Use the offset from the string
                                BigInt::from(exact_ns)
                            }
                            "ignore" => {
                                // Ignore the string offset, use wall time with tz
                                BigInt::from(local_ns - tz_off)
                            }
                            "prefer" => {
                                if !parsed.has_utc_designator && epoch_days.abs() > 100_000_000 {
                                    return Completion::Throw(interp.create_range_error(
                                        "ZonedDateTime is outside the representable range",
                                    ));
                                }
                                if parsed.has_utc_designator {
                                    BigInt::from(exact_ns)
                                } else {
                                    // Try fuzzy match first; fall back to compatible disambiguation
                                    match offset_match_candidates(&tz, local_ns, off_ns, off.has_sub_minute) {
                                        Some(ns) => ns,
                                        None => {
                                            // No match — use "compatible" disambiguation
                                            BigInt::from(local_ns - tz_off)
                                        }
                                    }
                                }
                            }
                            _ => BigInt::from(exact_ns),
                        }
                    } else {
                        // Use timezone to compute offset
                        let approx = BigInt::from(local_ns);
                        let off = get_tz_offset_ns(&tz, &approx) as i128;
                        BigInt::from(local_ns - off)
                    };

                    // Validate epoch ns is within range
                    if !is_valid_epoch_ns(&epoch_ns) {
                        return Completion::Throw(interp.create_range_error(
                            "ZonedDateTime is outside the representable range",
                        ));
                    }

                    create_zdt(interp, epoch_ns, tz, cal)
                }
                None => Completion::Throw(
                    interp
                        .create_range_error(&format!("Cannot parse '{}' as ZonedDateTime", s_str)),
                ),
            }
        }
        _ => Completion::Throw(
            interp.create_type_error("Expected an object or string for ZonedDateTime"),
        ),
    }
}

fn get_time_field(interp: &mut Interpreter, obj: &JsValue, name: &str) -> Result<i64, Completion> {
    let val = match get_prop(interp, obj, name) {
        Completion::Normal(v) => v,
        c => return Err(c),
    };
    if is_undefined(&val) {
        return Ok(0);
    }
    Ok(to_integer_with_truncation(interp, &val)? as i64)
}

/// from() with string: parse string first, then read options, then apply offset behavior
fn from_string_with_options(
    interp: &mut Interpreter,
    item: &JsValue,
    options: &JsValue,
) -> Completion {
    let s_str = match item {
        JsValue::String(s) => s.to_string(),
        _ => unreachable!(),
    };
    let parsed = match super::parse_temporal_date_time_string(&s_str) {
        Some(p) => p,
        None => {
            return Completion::Throw(
                interp.create_range_error(&format!("Invalid ZonedDateTime string: {s_str}")),
            );
        }
    };

    // If there's an offset (Z, ±HH:MM), time is required
    if parsed.offset.is_some() && !parsed.has_time {
        return Completion::Throw(
            interp.create_range_error("UTC offset without time is not valid for ZonedDateTime"),
        );
    }

    // Must have timezone annotation
    let tz = if let Some(ref tz_ann) = parsed.time_zone {
        match super::parse_temporal_time_zone_string(tz_ann) {
            Some(tz) => tz,
            None => {
                return Completion::Throw(
                    interp.create_range_error(&format!("Invalid timezone annotation: {tz_ann}")),
                );
            }
        }
    } else {
        return Completion::Throw(interp.create_range_error(
            "ZonedDateTime string requires a timezone annotation (e.g. [UTC])",
        ));
    };

    let cal_raw = parsed.calendar.unwrap_or_else(|| "iso8601".to_string());
    let cal = match super::validate_calendar(&cal_raw) {
        Some(c) => c,
        None => {
            return Completion::Throw(
                interp.create_range_error(&format!("Invalid calendar: {cal_raw}")),
            );
        }
    };

    // String parsed successfully — NOW read options
    let (_disambiguation, offset_opt, _overflow) =
        match parse_zdt_options(interp, options, "reject") {
            Ok(v) => v,
            Err(c) => return c,
        };

    // Compute epoch ns from date/time + offset
    let epoch_days = super::iso_date_to_epoch_days(parsed.year, parsed.month, parsed.day) as i128;
    let day_ns = parsed.hour as i128 * NS_PER_HOUR
        + parsed.minute as i128 * NS_PER_MIN
        + parsed.second as i128 * NS_PER_SEC
        + parsed.millisecond as i128 * NS_PER_MS
        + parsed.microsecond as i128 * 1_000
        + parsed.nanosecond as i128;
    let local_ns = epoch_days * NS_PER_DAY + day_ns;

    let epoch_ns =
        if let Some(ref off) = parsed.offset {
            let off_ns = off.sign as i128
                * (off.hours as i128 * NS_PER_HOUR
                    + off.minutes as i128 * NS_PER_MIN
                    + off.seconds as i128 * NS_PER_SEC
                    + off.nanoseconds as i128);
            let exact_ns = local_ns - off_ns;
            let tz_off = get_tz_offset_ns(&tz, &BigInt::from(exact_ns)) as i128;

            match offset_opt.as_str() {
                "reject" => {
                    if !parsed.has_utc_designator && epoch_days.abs() > 100_000_000 {
                        return Completion::Throw(interp.create_range_error(
                            "ZonedDateTime is outside the representable range",
                        ));
                    }
                    if parsed.has_utc_designator {
                        BigInt::from(exact_ns)
                    } else {
                        match offset_match_or_reject(interp, &tz, local_ns, off_ns, off.has_sub_minute) {
                                        Ok(ns) => ns,
                                        Err(c) => return c,
                                    }
                    }
                }
                "use" => BigInt::from(exact_ns),
                "ignore" => BigInt::from(local_ns - tz_off),
                "prefer" => {
                    if !parsed.has_utc_designator && epoch_days.abs() > 100_000_000 {
                        return Completion::Throw(interp.create_range_error(
                            "ZonedDateTime is outside the representable range",
                        ));
                    }
                    if parsed.has_utc_designator {
                        BigInt::from(exact_ns)
                    } else {
                        match offset_match_candidates(&tz, local_ns, off_ns, off.has_sub_minute) {
                            Some(ns) => ns,
                            None => BigInt::from(local_ns - tz_off),
                        }
                    }
                }
                _ => BigInt::from(exact_ns),
            }
        } else if !parsed.has_time {
            // No time in string → start-of-day per spec (time is ~start-of-day~)
            BigInt::from(get_start_of_day(&tz, epoch_days))
        } else {
            // Time specified but no offset → wall clock disambiguation
            BigInt::from(disambiguate_instant(&tz, local_ns, &_disambiguation))
        };

    if !is_valid_epoch_ns(&epoch_ns) {
        return Completion::Throw(
            interp.create_range_error("ZonedDateTime is outside the representable range"),
        );
    }

    create_zdt(interp, epoch_ns, tz, cal)
}

/// Parse ZonedDateTime-specific options: disambiguation, offset, overflow.
/// Read in alphabetical order per spec.
fn parse_zdt_options(
    interp: &mut Interpreter,
    options: &JsValue,
    default_offset: &str,
) -> Result<(String, String, String), Completion> {
    let has_options = super::get_options_object(interp, options)?;
    if !has_options {
        return Ok((
            "compatible".to_string(),
            default_offset.to_string(),
            "constrain".to_string(),
        ));
    }
    // disambiguation
    let dis_val = match get_prop(interp, options, "disambiguation") {
        Completion::Normal(v) => v,
        c => return Err(c),
    };
    let disambiguation = if is_undefined(&dis_val) {
        "compatible".to_string()
    } else {
        let s = match interp.to_string_value(&dis_val) {
            Ok(v) => v,
            Err(e) => return Err(Completion::Throw(e)),
        };
        match s.as_str() {
            "compatible" | "earlier" | "later" | "reject" => s,
            _ => {
                return Err(Completion::Throw(interp.create_range_error(&format!(
                    "{s} is not a valid value for disambiguation"
                ))));
            }
        }
    };
    // offset
    let off_val = match get_prop(interp, options, "offset") {
        Completion::Normal(v) => v,
        c => return Err(c),
    };
    let offset = if is_undefined(&off_val) {
        default_offset.to_string()
    } else {
        let s = match interp.to_string_value(&off_val) {
            Ok(v) => v,
            Err(e) => return Err(Completion::Throw(e)),
        };
        match s.as_str() {
            "prefer" | "use" | "ignore" | "reject" => s,
            _ => {
                return Err(Completion::Throw(interp.create_range_error(&format!(
                    "{s} is not a valid value for offset"
                ))));
            }
        }
    };
    // overflow
    let ovf_val = match get_prop(interp, options, "overflow") {
        Completion::Normal(v) => v,
        c => return Err(c),
    };
    let overflow = if is_undefined(&ovf_val) {
        "constrain".to_string()
    } else {
        let s = match interp.to_string_value(&ovf_val) {
            Ok(v) => v,
            Err(e) => return Err(Completion::Throw(e)),
        };
        match s.as_str() {
            "constrain" | "reject" => s,
            _ => {
                return Err(Completion::Throw(interp.create_range_error(&format!(
                    "{s} is not a valid value for overflow"
                ))));
            }
        }
    };
    Ok((disambiguation, offset, overflow))
}

fn zdt_to_string(
    ns: &BigInt,
    tz: &str,
    cal: &str,
    offset_display: &str,
    tz_display: &str,
    cal_display: &str,
    precision: Option<i32>,
    rounding_mode: &str,
) -> String {
    let offset_ns = get_tz_offset_ns(tz, ns);
    let total_ns: i128 = ns.try_into().unwrap_or(0);
    let local_ns = total_ns + offset_ns as i128;

    // Decompose into epoch_days and day_ns first, then round the time-of-day
    // portion. This is spec-correct: RoundISODateTime rounds as if positive.
    let mut epoch_days = local_ns.div_euclid(NS_PER_DAY);
    let day_ns = local_ns.rem_euclid(NS_PER_DAY);

    let day_ns = if let Some(p) = precision {
        let increment = match p {
            -1 => NS_PER_MIN,
            0 => NS_PER_SEC,
            1..=3 => NS_PER_MS * 10i128.pow(3 - p as u32),
            4..=6 => 1_000 * 10i128.pow(6 - p as u32),
            7..=9 => 10i128.pow(9 - p as u32),
            _ => 1,
        };
        let rounded = round_ns_to_increment(day_ns, increment, rounding_mode);
        if rounded >= NS_PER_DAY {
            epoch_days += 1;
            rounded - NS_PER_DAY
        } else {
            rounded
        }
    } else {
        day_ns
    };

    let (year, month, day) = super::epoch_days_to_iso_date(epoch_days as i64);
    let nanosecond = (day_ns % 1_000) as u16;
    let microsecond = ((day_ns / 1_000) % 1_000) as u16;
    let millisecond = ((day_ns / 1_000_000) % 1_000) as u16;
    let second = ((day_ns / NS_PER_SEC) % 60) as u8;
    let minute = ((day_ns / NS_PER_MIN) % 60) as u8;
    let hour = ((day_ns / NS_PER_HOUR) % 24) as u8;

    let year_str = if (0..=9999).contains(&year) {
        format!("{year:04}")
    } else if year >= 0 {
        format!("+{year:06}")
    } else {
        format!("-{:06}", year.unsigned_abs())
    };

    let frac_ns = millisecond as u32 * 1_000_000 + microsecond as u32 * 1_000 + nanosecond as u32;
    let time_str = match precision {
        Some(-1) => format!("{hour:02}:{minute:02}"),
        Some(0) => format!("{hour:02}:{minute:02}:{second:02}"),
        Some(digits) if digits > 0 => {
            let frac = format!("{frac_ns:09}");
            let truncated = &frac[..digits as usize];
            format!("{hour:02}:{minute:02}:{second:02}.{truncated}")
        }
        None => {
            if frac_ns == 0 {
                format!("{hour:02}:{minute:02}:{second:02}")
            } else {
                let frac = format!("{frac_ns:09}");
                let trimmed = frac.trim_end_matches('0');
                format!("{hour:02}:{minute:02}:{second:02}.{trimmed}")
            }
        }
        _ => format!("{hour:02}:{minute:02}:{second:02}"),
    };

    let offset_str = if offset_display != "never" {
        format_offset_string_rounded(offset_ns)
    } else {
        String::new()
    };

    let tz_str = match tz_display {
        "never" => String::new(),
        "critical" => format!("[!{tz}]"),
        _ => format!("[{tz}]"),
    };

    let cal_str = match cal_display {
        "always" => format!("[u-ca={cal}]"),
        "never" => String::new(),
        "critical" => format!("[!u-ca={cal}]"),
        _ => {
            // "auto"
            if cal == "iso8601" {
                String::new()
            } else {
                format!("[u-ca={cal}]")
            }
        }
    };

    format!("{year_str}-{month:02}-{day:02}T{time_str}{offset_str}{tz_str}{cal_str}")
}

fn round_ns_to_increment(ns: i128, increment: i128, mode: &str) -> i128 {
    round_ns_i128(ns, increment, mode)
}

impl Interpreter {
    pub(crate) fn setup_temporal_zoned_date_time(
        &mut self,
        temporal_obj: &Rc<RefCell<JsObjectData>>,
    ) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Temporal.ZonedDateTime".to_string();

        // @@toStringTag
        {
            let key = "Symbol(Symbol.toStringTag)".to_string();
            let desc = PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str(
                    "Temporal.ZonedDateTime",
                ))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            };
            proto.borrow_mut().property_order.push(key.clone());
            proto.borrow_mut().properties.insert(key, desc);
        }

        // --- Getters ---
        macro_rules! zdt_getter {
            ($name:expr, $body:expr) => {{
                let getter = self.create_function(JsFunction::native(
                    format!("get {}", $name),
                    0,
                    |interp, this, _args| {
                        let (ns, tz, cal) = match get_zdt_fields(interp, &this) {
                            Ok(v) => v,
                            Err(c) => return c,
                        };
                        let body_fn: fn(&BigInt, &str, &str) -> Completion = $body;
                        body_fn(&ns, &tz, &cal)
                    },
                ));
                proto.borrow_mut().insert_property(
                    $name.to_string(),
                    PropertyDescriptor {
                        value: None,
                        writable: None,
                        enumerable: Some(false),
                        configurable: Some(true),
                        get: Some(getter),
                        set: None,
                    },
                );
            }};
        }

        zdt_getter!("calendarId", |_ns, _tz, cal| {
            Completion::Normal(JsValue::String(JsString::from_str(cal)))
        });

        zdt_getter!("timeZoneId", |_ns, tz, _cal| {
            Completion::Normal(JsValue::String(JsString::from_str(tz)))
        });

        zdt_getter!("epochMilliseconds", |ns, _tz, _cal| {
            let ms = floor_div_bigint(ns, NS_PER_MS);
            Completion::Normal(JsValue::Number(bigint_to_f64(&ms)))
        });

        zdt_getter!("epochNanoseconds", |ns, _tz, _cal| {
            Completion::Normal(JsValue::BigInt(crate::types::JsBigInt {
                value: ns.clone(),
            }))
        });

        zdt_getter!("year", |ns, tz, cal| {
            let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(ns, tz);
            if cal != "iso8601" {
                if let Some(cf) = super::iso_to_calendar_fields(y, m, d, cal) {
                    return Completion::Normal(JsValue::Number(cf.year as f64));
                }
            }
            Completion::Normal(JsValue::Number(y as f64))
        });

        zdt_getter!("month", |ns, tz, cal| {
            let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(ns, tz);
            if cal != "iso8601" {
                if let Some(cf) = super::iso_to_calendar_fields(y, m, d, cal) {
                    return Completion::Normal(JsValue::Number(cf.month_ordinal as f64));
                }
            }
            Completion::Normal(JsValue::Number(m as f64))
        });

        zdt_getter!("monthCode", |ns, tz, cal| {
            let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(ns, tz);
            if cal != "iso8601" {
                if let Some(cf) = super::iso_to_calendar_fields(y, m, d, cal) {
                    return Completion::Normal(JsValue::String(JsString::from_str(&cf.month_code)));
                }
            }
            Completion::Normal(JsValue::String(JsString::from_str(&format!("M{m:02}"))))
        });

        zdt_getter!("day", |ns, tz, cal| {
            let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(ns, tz);
            if cal != "iso8601" {
                if let Some(cf) = super::iso_to_calendar_fields(y, m, d, cal) {
                    return Completion::Normal(JsValue::Number(cf.day as f64));
                }
            }
            Completion::Normal(JsValue::Number(d as f64))
        });

        zdt_getter!("hour", |ns, tz, _cal| {
            let (_, _, _, h, _, _, _, _, _) = epoch_ns_to_components(ns, tz);
            Completion::Normal(JsValue::Number(h as f64))
        });

        zdt_getter!("minute", |ns, tz, _cal| {
            let (_, _, _, _, mi, _, _, _, _) = epoch_ns_to_components(ns, tz);
            Completion::Normal(JsValue::Number(mi as f64))
        });

        zdt_getter!("second", |ns, tz, _cal| {
            let (_, _, _, _, _, s, _, _, _) = epoch_ns_to_components(ns, tz);
            Completion::Normal(JsValue::Number(s as f64))
        });

        zdt_getter!("millisecond", |ns, tz, _cal| {
            let (_, _, _, _, _, _, ms, _, _) = epoch_ns_to_components(ns, tz);
            Completion::Normal(JsValue::Number(ms as f64))
        });

        zdt_getter!("microsecond", |ns, tz, _cal| {
            let (_, _, _, _, _, _, _, us, _) = epoch_ns_to_components(ns, tz);
            Completion::Normal(JsValue::Number(us as f64))
        });

        zdt_getter!("nanosecond", |ns, tz, _cal| {
            let (_, _, _, _, _, _, _, _, nanos) = epoch_ns_to_components(ns, tz);
            Completion::Normal(JsValue::Number(nanos as f64))
        });

        zdt_getter!("dayOfWeek", |ns, tz, _cal| {
            let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(ns, tz);
            Completion::Normal(JsValue::Number(super::iso_day_of_week(y, m, d) as f64))
        });

        zdt_getter!("dayOfYear", |ns, tz, cal| {
            let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(ns, tz);
            if cal != "iso8601" {
                if let Some(cf) = super::iso_to_calendar_fields(y, m, d, cal) {
                    return Completion::Normal(JsValue::Number(cf.day_of_year as f64));
                }
            }
            Completion::Normal(JsValue::Number(super::iso_day_of_year(y, m, d) as f64))
        });

        zdt_getter!("weekOfYear", |ns, tz, cal| {
            if cal != "iso8601" {
                return Completion::Normal(JsValue::Undefined);
            }
            let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(ns, tz);
            let (woy, _) = super::iso_week_of_year(y, m, d);
            Completion::Normal(JsValue::Number(woy as f64))
        });

        zdt_getter!("yearOfWeek", |ns, tz, cal| {
            if cal != "iso8601" {
                return Completion::Normal(JsValue::Undefined);
            }
            let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(ns, tz);
            let (_, yow) = super::iso_week_of_year(y, m, d);
            Completion::Normal(JsValue::Number(yow as f64))
        });

        zdt_getter!("daysInWeek", |_ns, _tz, _cal| {
            Completion::Normal(JsValue::Number(7.0))
        });

        zdt_getter!("daysInMonth", |ns, tz, cal| {
            let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(ns, tz);
            if cal != "iso8601" {
                if let Some(cf) = super::iso_to_calendar_fields(y, m, d, cal) {
                    return Completion::Normal(JsValue::Number(cf.days_in_month as f64));
                }
            }
            Completion::Normal(JsValue::Number(super::iso_days_in_month(y, m) as f64))
        });

        zdt_getter!("daysInYear", |ns, tz, cal| {
            let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(ns, tz);
            if cal != "iso8601" {
                if let Some(cf) = super::iso_to_calendar_fields(y, m, d, cal) {
                    return Completion::Normal(JsValue::Number(cf.days_in_year as f64));
                }
            }
            let days = if super::iso_is_leap_year(y) { 366 } else { 365 };
            Completion::Normal(JsValue::Number(days as f64))
        });

        zdt_getter!("monthsInYear", |ns, tz, cal| {
            let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(ns, tz);
            if cal != "iso8601" {
                if let Some(cf) = super::iso_to_calendar_fields(y, m, d, cal) {
                    return Completion::Normal(JsValue::Number(cf.months_in_year as f64));
                }
            }
            Completion::Normal(JsValue::Number(12.0))
        });

        zdt_getter!("inLeapYear", |ns, tz, cal| {
            let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(ns, tz);
            if cal != "iso8601" {
                if let Some(cf) = super::iso_to_calendar_fields(y, m, d, cal) {
                    return Completion::Normal(JsValue::Boolean(cf.in_leap_year));
                }
            }
            Completion::Normal(JsValue::Boolean(super::iso_is_leap_year(y)))
        });

        zdt_getter!("offset", |ns, tz, _cal| {
            let offset_ns = get_tz_offset_ns(tz, ns);
            Completion::Normal(JsValue::String(JsString::from_str(&format_offset_string(
                offset_ns,
            ))))
        });

        zdt_getter!("offsetNanoseconds", |ns, tz, _cal| {
            let offset_ns = get_tz_offset_ns(tz, ns);
            Completion::Normal(JsValue::Number(offset_ns as f64))
        });

        {
            let getter = self.create_function(JsFunction::native(
                "get hoursInDay".to_string(),
                0,
                |interp, this, _args| {
                    let (ns, tz, _cal) = match get_zdt_fields(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(&ns, &tz);
                    let start_days = super::iso_date_to_epoch_days(y, m, d);
                    if start_days.abs() > 100_000_000 {
                        return Completion::Throw(
                            interp.create_range_error("date outside representable range"),
                        );
                    }
                    let (ny, nm, nd) = super::balance_iso_date(y, m as i32, d as i32 + 1);
                    let next_days = super::iso_date_to_epoch_days(ny, nm, nd);
                    if next_days.abs() > 100_000_000 {
                        return Completion::Throw(
                            interp.create_range_error("date outside representable range"),
                        );
                    }

                    let start_utc = get_start_of_day(&tz, start_days as i128);

                    let ns_max: i128 = 8_640_000_000_000_000_000_000;
                    if start_utc < -ns_max || start_utc > ns_max {
                        return Completion::Throw(
                            interp.create_range_error("date outside representable range"),
                        );
                    }

                    let next_utc = get_start_of_day(&tz, next_days as i128);

                    if next_utc < -ns_max || next_utc > ns_max {
                        return Completion::Throw(
                            interp.create_range_error("date outside representable range"),
                        );
                    }

                    let diff_ns = next_utc - start_utc;
                    let hours = diff_ns as f64 / NS_PER_HOUR as f64;
                    Completion::Normal(JsValue::Number(hours))
                },
            ));
            proto.borrow_mut().insert_property(
                "hoursInDay".to_string(),
                PropertyDescriptor {
                    value: None,
                    writable: None,
                    enumerable: Some(false),
                    configurable: Some(true),
                    get: Some(getter),
                    set: None,
                },
            );
        }

        zdt_getter!("era", |ns, tz, cal| {
            if cal != "iso8601" {
                let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(ns, tz);
                if let Some(cf) = super::iso_to_calendar_fields(y, m, d, cal) {
                    return Completion::Normal(match cf.era {
                        Some(e) => JsValue::String(JsString::from_str(&e)),
                        None => JsValue::Undefined,
                    });
                }
            }
            Completion::Normal(JsValue::Undefined)
        });

        zdt_getter!("eraYear", |ns, tz, cal| {
            if cal != "iso8601" {
                let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(ns, tz);
                if let Some(cf) = super::iso_to_calendar_fields(y, m, d, cal) {
                    return Completion::Normal(match cf.era_year {
                        Some(ey) => JsValue::Number(ey as f64),
                        None => JsValue::Undefined,
                    });
                }
            }
            Completion::Normal(JsValue::Undefined)
        });

        self.realm_mut().temporal_zoned_date_time_prototype = Some(proto.clone());

        // --- Methods ---

        // toString(options?)
        {
            let method = self.create_function(JsFunction::native(
                "toString".to_string(),
                0,
                |interp, this, args| {
                    let (ns, tz, cal) = match get_zdt_fields(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let options = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let (precision, rounding_mode, offset_display, tz_display, cal_display) =
                        match parse_zdt_to_string_options(interp, &options) {
                            Ok(v) => v,
                            Err(c) => return c,
                        };
                    let result = zdt_to_string(
                        &ns,
                        &tz,
                        &cal,
                        &offset_display,
                        &tz_display,
                        &cal_display,
                        precision,
                        &rounding_mode,
                    );
                    Completion::Normal(JsValue::String(JsString::from_str(&result)))
                },
            ));
            proto
                .borrow_mut()
                .insert_builtin("toString".to_string(), method);
        }

        // toJSON()
        {
            let method = self.create_function(JsFunction::native(
                "toJSON".to_string(),
                0,
                |interp, this, _args| {
                    let (ns, tz, cal) = match get_zdt_fields(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let result =
                        zdt_to_string(&ns, &tz, &cal, "auto", "auto", "auto", None, "trunc");
                    Completion::Normal(JsValue::String(JsString::from_str(&result)))
                },
            ));
            proto
                .borrow_mut()
                .insert_builtin("toJSON".to_string(), method);
        }

        // toLocaleString()
        {
            let method = self.create_function(JsFunction::native(
                "toLocaleString".to_string(),
                0,
                |interp, this, args| {
                    let (ns, tz, _cal) = match get_zdt_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let dtf_val = match interp.realm().intl_date_time_format_ctor.clone() {
                        Some(v) => v,
                        None => {
                            let result = zdt_to_string(&ns, &tz, &_cal, "auto", "auto", "auto", None, "trunc");
                            return Completion::Normal(JsValue::String(JsString::from_str(&result)));
                        }
                    };
                    let locales_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let options_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    // Reject user-provided timeZone option
                    if let JsValue::Object(ref o) = options_arg {
                        let tz_val = match interp.get_object_property(o.id, "timeZone", &options_arg) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => JsValue::Undefined,
                        };
                        if !matches!(tz_val, JsValue::Undefined) {
                            return Completion::Throw(interp.create_type_error(
                                "ZonedDateTime toLocaleString does not accept a timeZone option",
                            ));
                        }
                    }
                    // Inject timeZone from ZDT into options
                    let effective_opts = {
                        let opts_obj = interp.create_object();
                        if let Some(ref op) = interp.realm().object_prototype {
                            opts_obj.borrow_mut().prototype = Some(op.clone());
                        }
                        // Copy properties from user options if present
                        if let JsValue::Object(ref o) = options_arg {
                            let keys: Vec<String> = interp.get_object(o.id)
                                .map(|rc| rc.borrow().properties.keys().cloned().collect())
                                .unwrap_or_default();
                            for key in keys {
                                let val = match interp.get_object_property(o.id, &key, &options_arg) {
                                    Completion::Normal(v) => v,
                                    Completion::Throw(e) => return Completion::Throw(e),
                                    _ => JsValue::Undefined,
                                };
                                opts_obj.borrow_mut().insert_property(
                                    key,
                                    crate::interpreter::types::PropertyDescriptor::data(
                                        val, true, true, true,
                                    ),
                                );
                            }
                        }
                        // Set timeZone from ZDT
                        opts_obj.borrow_mut().insert_property(
                            "timeZone".to_string(),
                            crate::interpreter::types::PropertyDescriptor::data(
                                JsValue::String(JsString::from_str(&tz)), true, true, true,
                            ),
                        );
                        // If no explicit date/time components/styles, set ZDT defaults
                        // (timeZoneName alone doesn't count as explicit)
                        let has_explicit = {
                            let b = opts_obj.borrow();
                            ["year", "month", "day", "weekday", "hour", "minute",
                             "second", "dayPeriod",
                             "fractionalSecondDigits", "dateStyle", "timeStyle"]
                                .iter().any(|k| {
                                    b.properties.get(*k).is_some_and(|pd| {
                                        !matches!(pd.value, Some(JsValue::Undefined) | None)
                                    })
                                })
                        };
                        if !has_explicit {
                            let has_tz_name = {
                                let b = opts_obj.borrow();
                                b.properties.get("timeZoneName").is_some_and(|pd| {
                                    !matches!(pd.value, Some(JsValue::Undefined) | None)
                                })
                            };
                            for (k, v) in [
                                ("year", "numeric"), ("month", "numeric"), ("day", "numeric"),
                                ("hour", "numeric"), ("minute", "numeric"), ("second", "numeric"),
                            ] {
                                opts_obj.borrow_mut().insert_property(
                                    k.to_string(),
                                    crate::interpreter::types::PropertyDescriptor::data(
                                        JsValue::String(JsString::from_str(v)), true, true, true,
                                    ),
                                );
                            }
                            if !has_tz_name {
                                opts_obj.borrow_mut().insert_property(
                                    "timeZoneName".to_string(),
                                    crate::interpreter::types::PropertyDescriptor::data(
                                        JsValue::String(JsString::from_str("short")), true, true, true,
                                    ),
                                );
                            }
                        }
                        let oid = opts_obj.borrow().id.unwrap();
                        JsValue::Object(crate::types::JsObject { id: oid })
                    };
                    let dtf_instance = match interp.construct(&dtf_val, &[locales_arg, effective_opts]) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => return Completion::Normal(JsValue::Undefined),
                    };
                    if let Err(e) = super::check_calendar_mismatch(interp, &dtf_instance, &_cal, true) {
                        return Completion::Throw(e);
                    }
                    // Convert ZDT to epoch ms for DTF.format()
                    let epoch_ms = {
                        use num_bigint::BigInt;
                        let ms_bigint = &ns / BigInt::from(1_000_000i64);
                        ms_bigint.to_string().parse::<f64>().unwrap_or(f64::NAN)
                    };
                    let ms_val = JsValue::Number(epoch_ms);
                    super::temporal_format_with_dtf(interp, &dtf_instance, &ms_val)
                },
            ));
            proto
                .borrow_mut()
                .insert_builtin("toLocaleString".to_string(), method);
        }

        // valueOf()
        {
            let method = self.create_function(JsFunction::native(
                "valueOf".to_string(),
                0,
                |interp, _this, _args| {
                    Completion::Throw(
                        interp.create_type_error("use compare() to compare Temporal.ZonedDateTime"),
                    )
                },
            ));
            proto
                .borrow_mut()
                .insert_builtin("valueOf".to_string(), method);
        }

        // equals(other)
        {
            let method = self.create_function(JsFunction::native(
                "equals".to_string(),
                1,
                |interp, this, args| {
                    let (ns, tz, cal) = match get_zdt_fields(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let other_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let other_val = match to_temporal_zoned_date_time(interp, &other_arg) {
                        Completion::Normal(v) => v,
                        c => return c,
                    };
                    let (ons, otz, ocal) = match get_zdt_fields(interp, &other_val) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let tz_equal = tz.eq_ignore_ascii_case(&otz)
                        || super::canonicalize_iana_tz(&tz) == super::canonicalize_iana_tz(&otz);
                    Completion::Normal(JsValue::Boolean(ns == ons && tz_equal && cal == ocal))
                },
            ));
            proto
                .borrow_mut()
                .insert_builtin("equals".to_string(), method);
        }

        // toInstant()
        {
            let method = self.create_function(JsFunction::native(
                "toInstant".to_string(),
                0,
                |interp, this, _args| {
                    let (ns, _, _) = match get_zdt_fields(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let obj = interp.create_object();
                    obj.borrow_mut().class_name = "Temporal.Instant".to_string();
                    if let Some(ref proto) = interp.realm().temporal_instant_prototype {
                        obj.borrow_mut().prototype = Some(proto.clone());
                    }
                    obj.borrow_mut().temporal_data = Some(TemporalData::Instant {
                        epoch_nanoseconds: ns,
                    });
                    let id = obj.borrow().id.unwrap();
                    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
                },
            ));
            proto
                .borrow_mut()
                .insert_builtin("toInstant".to_string(), method);
        }

        // toPlainDate()
        {
            let method = self.create_function(JsFunction::native(
                "toPlainDate".to_string(),
                0,
                |interp, this, _args| {
                    let (ns, tz, cal) = match get_zdt_fields(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(&ns, &tz);
                    super::plain_date::create_plain_date_result(interp, y, m, d, &cal)
                },
            ));
            proto
                .borrow_mut()
                .insert_builtin("toPlainDate".to_string(), method);
        }

        // toPlainTime()
        {
            let method = self.create_function(JsFunction::native(
                "toPlainTime".to_string(),
                0,
                |interp, this, _args| {
                    let (ns, tz, _cal) = match get_zdt_fields(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let (_, _, _, h, mi, s, ms, us, nanos) = epoch_ns_to_components(&ns, &tz);
                    super::plain_time::create_plain_time_result(interp, h, mi, s, ms, us, nanos)
                },
            ));
            proto
                .borrow_mut()
                .insert_builtin("toPlainTime".to_string(), method);
        }

        // toPlainDateTime()
        {
            let method = self.create_function(JsFunction::native(
                "toPlainDateTime".to_string(),
                0,
                |interp, this, _args| {
                    let (ns, tz, cal) = match get_zdt_fields(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let (y, m, d, h, mi, s, ms, us, nanos) = epoch_ns_to_components(&ns, &tz);
                    super::plain_date_time::create_plain_date_time_result(
                        interp, y, m, d, h, mi, s, ms, us, nanos, &cal,
                    )
                },
            ));
            proto
                .borrow_mut()
                .insert_builtin("toPlainDateTime".to_string(), method);
        }

        // toPlainYearMonth()
        {
            let method = self.create_function(JsFunction::native(
                "toPlainYearMonth".to_string(),
                0,
                |interp, this, _args| {
                    let (ns, tz, cal) = match get_zdt_fields(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(&ns, &tz);
                    super::plain_year_month::create_plain_year_month_result(interp, y, m, d, &cal)
                },
            ));
            proto
                .borrow_mut()
                .insert_builtin("toPlainYearMonth".to_string(), method);
        }

        // toPlainMonthDay()
        {
            let method = self.create_function(JsFunction::native(
                "toPlainMonthDay".to_string(),
                0,
                |interp, this, _args| {
                    let (ns, tz, cal) = match get_zdt_fields(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let (_y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(&ns, &tz);
                    // Per spec, ISO 8601 calendar uses reference year 1972
                    super::plain_month_day::create_plain_month_day_result(interp, m, d, 1972, &cal)
                },
            ));
            proto
                .borrow_mut()
                .insert_builtin("toPlainMonthDay".to_string(), method);
        }

        // startOfDay()
        {
            let method = self.create_function(JsFunction::native(
                "startOfDay".to_string(),
                0,
                |interp, this, _args| {
                    let (ns, tz, cal) = match get_zdt_fields(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(&ns, &tz);
                    let start_days = super::iso_date_to_epoch_days(y, m, d) as i128;
                    let epoch_ns = get_start_of_day(&tz, start_days);
                    create_zdt(interp, BigInt::from(epoch_ns), tz, cal)
                },
            ));
            proto
                .borrow_mut()
                .insert_builtin("startOfDay".to_string(), method);
        }

        // withCalendar(calendar)
        {
            let method = self.create_function(JsFunction::native(
                "withCalendar".to_string(),
                1,
                |interp, this, args| {
                    let (ns, tz, _cal) = match get_zdt_fields(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let cal_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if matches!(&cal_arg, JsValue::Undefined) {
                        return Completion::Throw(
                            interp.create_type_error("withCalendar requires a calendar argument"),
                        );
                    }
                    let new_cal = match super::to_temporal_calendar_slot_value(interp, &cal_arg) {
                        Ok(c) => c,
                        Err(c) => return c,
                    };
                    create_zdt(interp, ns, tz, new_cal)
                },
            ));
            proto
                .borrow_mut()
                .insert_builtin("withCalendar".to_string(), method);
        }

        // withTimeZone(timeZone)
        {
            let method = self.create_function(JsFunction::native(
                "withTimeZone".to_string(),
                1,
                |interp, this, args| {
                    let (ns, _tz, cal) = match get_zdt_fields(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let tz_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let new_tz = match to_temporal_time_zone_identifier(interp, &tz_arg) {
                        Ok(t) => t,
                        Err(c) => return c,
                    };
                    create_zdt(interp, ns, new_tz, cal)
                },
            ));
            proto
                .borrow_mut()
                .insert_builtin("withTimeZone".to_string(), method);
        }

        // withPlainTime(plainTimeLike?)
        {
            let method = self.create_function(JsFunction::native(
                "withPlainTime".to_string(),
                0,
                |interp, this, args| {
                    let (ns, tz, cal) = match get_zdt_fields(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(&ns, &tz);
                    let time_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let epoch_days = super::iso_date_to_epoch_days(y, m, d) as i128;
                    if is_undefined(&time_arg) {
                        // Per spec: use GetStartOfDay
                        let epoch_ns = get_start_of_day(&tz, epoch_days);
                        create_zdt(interp, BigInt::from(epoch_ns), tz, cal)
                    } else {
                        let (h, mi, s, ms, us, nanos) = match super::plain_time::to_temporal_plain_time(interp, time_arg) {
                            Ok(f) => f,
                            Err(c) => return c,
                        };
                        let day_ns = h as i128 * NS_PER_HOUR
                            + mi as i128 * NS_PER_MIN
                            + s as i128 * NS_PER_SEC
                            + ms as i128 * NS_PER_MS
                            + us as i128 * 1_000
                            + nanos as i128;
                        let local_ns = epoch_days * NS_PER_DAY + day_ns;
                        // Per spec: use GetEpochNanosecondsFor with compatible
                        let epoch_ns = BigInt::from(disambiguate_instant(&tz, local_ns, "compatible"));
                        create_zdt(interp, epoch_ns, tz, cal)
                    }
                },
            ));
            proto
                .borrow_mut()
                .insert_builtin("withPlainTime".to_string(), method);
        }

        // with(temporalZonedDateTimeLike, options?)
        {
            let method = self.create_function(JsFunction::native(
                "with".to_string(),
                1,
                |interp, this, args| {
                    let (ns, tz, cal) = match get_zdt_fields(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let bag = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let Err(c) = is_partial_temporal_object(interp, &bag) {
                        return c;
                    }

                    let (y, m, d, h, mi, s, ms, us, nanos) = epoch_ns_to_components(&ns, &tz);

                    // Non-ISO calendar path: work in calendar-relative space
                    if cal != "iso8601" {
                        if let Some(cf) = super::iso_to_calendar_fields(y, m, d, &cal) {
                            let mut has_any = false;

                            // Read date fields in calendar space (alphabetical)
                            let (new_d, has_d) =
                                match super::read_field_positive_int(interp, &bag, "day", cf.day) {
                                    Ok(v) => v,
                                    Err(c) => return c,
                                };
                            has_any |= has_d;

                            // Time fields (alphabetical among time: hour, microsecond, millisecond, minute)
                            macro_rules! time_field {
                                ($name:expr, $default:expr) => {{
                                    let v = match get_prop(interp, &bag, $name) {
                                        Completion::Normal(v) => v,
                                        c => return c,
                                    };
                                    if is_undefined(&v) {
                                        ($default as i32, false)
                                    } else {
                                        has_any = true;
                                        let n = match interp.to_number_value(&v) {
                                            Ok(n) => n,
                                            Err(c) => return Completion::Throw(c),
                                        };
                                        if n.is_infinite() {
                                            return Completion::Throw(
                                                interp.create_range_error(&format!(
                                                    "{} must be finite", $name
                                                )),
                                            );
                                        }
                                        (if n.is_nan() { 0 } else { n as i32 }, true)
                                    }
                                }};
                            }

                            let (nh_raw, _) = time_field!("hour", h);
                            let (nus_raw, _) = time_field!("microsecond", us);
                            let (nms_raw, _) = time_field!("millisecond", ms);
                            let (nmi_raw, _) = time_field!("minute", mi);

                            // month, monthCode
                            let (raw_month, has_m) = match super::read_field_positive_int(
                                interp, &bag, "month", cf.month_ordinal,
                            ) {
                                Ok(v) => (Some(v.0), v.1),
                                Err(c) => return c,
                            };
                            let raw_month = if has_m { raw_month } else { None };
                            has_any |= has_m;
                            let (raw_month_code, has_mc) =
                                match super::read_month_code_field(interp, &bag) {
                                    Ok(v) => v,
                                    Err(c) => return c,
                                };
                            has_any |= has_mc;

                            let (nns_raw, _) = time_field!("nanosecond", nanos);

                            // offset
                            let offset_prop = match get_prop(interp, &bag, "offset") {
                                Completion::Normal(v) => v,
                                c => return c,
                            };
                            if !is_undefined(&offset_prop) {
                                has_any = true;
                                match &offset_prop {
                                    JsValue::String(sv) => {
                                        let s_str = sv.to_rust_string();
                                        if super::parse_utc_offset_timezone(&s_str).is_none() {
                                            return Completion::Throw(interp.create_range_error(
                                                &format!("invalid offset string: {s_str}"),
                                            ));
                                        }
                                    }
                                    JsValue::Object(_) => {
                                        let s_str = match interp.to_string_value(&offset_prop) {
                                            Ok(sv) => sv,
                                            Err(c) => return Completion::Throw(c),
                                        };
                                        if super::parse_utc_offset_timezone(&s_str).is_none() {
                                            return Completion::Throw(interp.create_range_error(
                                                &format!("invalid offset string: {s_str}"),
                                            ));
                                        }
                                    }
                                    _ => {
                                        return Completion::Throw(
                                            interp.create_type_error("offset must be a string"),
                                        );
                                    }
                                }
                            }

                            let (ns2_raw, _) = time_field!("second", s);

                            let (new_y, has_y) =
                                match super::read_field_i32(interp, &bag, "year", cf.year) {
                                    Ok(v) => v,
                                    Err(c) => return c,
                                };
                            has_any |= has_y;

                            // era/eraYear
                            let era_val = match super::get_prop(interp, &bag, "era") {
                                Completion::Normal(v) => v,
                                other => return other,
                            };
                            let has_era = !super::is_undefined(&era_val);
                            has_any |= has_era;
                            let era_year_val = match super::get_prop(interp, &bag, "eraYear") {
                                Completion::Normal(v) => v,
                                other => return other,
                            };
                            let has_era_year = !super::is_undefined(&era_year_val);
                            has_any |= has_era_year;

                            if !has_any {
                                return Completion::Throw(interp.create_type_error(
                                    "with requires at least one recognized temporal property",
                                ));
                            }

                            // Validate era/eraYear pairing
                            if super::calendar_has_eras(&cal) {
                                if has_era && !has_era_year {
                                    return Completion::Throw(interp.create_type_error(
                                        "era provided without eraYear",
                                    ));
                                }
                                if has_era_year && !has_era {
                                    return Completion::Throw(interp.create_type_error(
                                        "eraYear provided without era",
                                    ));
                                }
                            } else if has_era || has_era_year {
                                return Completion::Throw(interp.create_type_error(
                                    "era and eraYear are not valid for this calendar",
                                ));
                            }

                            // Determine month_code and month_ordinal for ICU
                            let mc_for_icu = if has_mc {
                                raw_month_code.clone()
                            } else if !has_m {
                                Some(cf.month_code.clone())
                            } else {
                                None
                            };
                            let mo_for_icu = if has_m { raw_month } else { None };

                            // Determine era for ICU
                            let (icu_era, icu_year) =
                                if super::calendar_has_eras(&cal) && has_era && has_era_year {
                                    let era_str =
                                        match super::to_primitive_and_require_string(
                                            interp, &era_val, "era",
                                        ) {
                                            Ok(v) => v,
                                            Err(c) => return c,
                                        };
                                    let ey = match super::to_integer_with_truncation(
                                        interp,
                                        &era_year_val,
                                    ) {
                                        Ok(v) => v as i32,
                                        Err(c) => return c,
                                    };
                                    (Some(era_str), ey)
                                } else {
                                    (None, new_y)
                                };

                            // Read options
                            let opts = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                            let (_disambiguation, offset_opt, overflow) =
                                match parse_zdt_options(interp, &opts, "prefer") {
                                    Ok(v) => v,
                                    Err(c) => return c,
                                };

                            // Convert calendar fields back to ISO
                            match super::calendar_fields_to_iso_overflow(
                                icu_era.as_deref(),
                                icu_year,
                                mc_for_icu.as_deref(),
                                mo_for_icu,
                                new_d,
                                &cal,
                                &overflow,
                            ) {
                                Some((iso_y, iso_m, iso_d)) => {
                                    // Clamp/reject time fields
                                    let (nh, nmi, ns2, nms, nus, nns) = if overflow == "reject" {
                                        if nh_raw < 0 || nh_raw > 23 {
                                            return Completion::Throw(
                                                interp.create_range_error("hour out of range"),
                                            );
                                        }
                                        if nmi_raw < 0 || nmi_raw > 59 {
                                            return Completion::Throw(
                                                interp.create_range_error("minute out of range"),
                                            );
                                        }
                                        if ns2_raw < 0 || ns2_raw > 59 {
                                            return Completion::Throw(
                                                interp.create_range_error("second out of range"),
                                            );
                                        }
                                        if nms_raw < 0 || nms_raw > 999 {
                                            return Completion::Throw(
                                                interp.create_range_error("millisecond out of range"),
                                            );
                                        }
                                        if nus_raw < 0 || nus_raw > 999 {
                                            return Completion::Throw(
                                                interp.create_range_error("microsecond out of range"),
                                            );
                                        }
                                        if nns_raw < 0 || nns_raw > 999 {
                                            return Completion::Throw(
                                                interp.create_range_error("nanosecond out of range"),
                                            );
                                        }
                                        (
                                            nh_raw as u8, nmi_raw as u8, ns2_raw as u8,
                                            nms_raw as u16, nus_raw as u16, nns_raw as u16,
                                        )
                                    } else {
                                        (
                                            nh_raw.clamp(0, 23) as u8,
                                            nmi_raw.clamp(0, 59) as u8,
                                            ns2_raw.clamp(0, 59) as u8,
                                            nms_raw.clamp(0, 999) as u16,
                                            nus_raw.clamp(0, 999) as u16,
                                            nns_raw.clamp(0, 999) as u16,
                                        )
                                    };

                                    let epoch_days =
                                        super::iso_date_to_epoch_days(iso_y, iso_m, iso_d) as i128;
                                    let day_ns = nh as i128 * NS_PER_HOUR
                                        + nmi as i128 * NS_PER_MIN
                                        + ns2 as i128 * NS_PER_SEC
                                        + nms as i128 * NS_PER_MS
                                        + nus as i128 * 1_000
                                        + nns as i128;
                                    let local_ns = epoch_days * NS_PER_DAY + day_ns;
                                    let offset_ns =
                                        if offset_opt == "use" && !is_undefined(&offset_prop) {
                                            let off_str =
                                                match interp.to_string_value(&offset_prop) {
                                                    Ok(sv) => sv,
                                                    Err(c) => return Completion::Throw(c),
                                                };
                                            match super::parse_utc_offset_timezone(&off_str) {
                                                Some(canonical) => {
                                                    super::offset_string_to_ns(&canonical)
                                                }
                                                None => get_tz_offset_ns(
                                                    &tz,
                                                    &BigInt::from(local_ns),
                                                )
                                                    as i128,
                                            }
                                        } else {
                                            get_tz_offset_ns(&tz, &BigInt::from(local_ns)) as i128
                                        };
                                    let new_epoch_ns = BigInt::from(local_ns - offset_ns);
                                    return create_zdt(interp, new_epoch_ns, tz, cal);
                                }
                                None => {
                                    return Completion::Throw(
                                        interp.create_range_error("Invalid calendar date"),
                                    );
                                }
                            }
                        }
                    }

                    // ISO path
                    let mut has_any = false;
                    macro_rules! field_or_default {
                        ($name:expr, $default:expr) => {{
                            let v = match get_prop(interp, &bag, $name) {
                                Completion::Normal(v) => v,
                                c => return c,
                            };
                            if is_undefined(&v) {
                                $default as f64
                            } else {
                                has_any = true;
                                let n = match interp.to_number_value(&v) {
                                    Ok(n) => n,
                                    Err(c) => return Completion::Throw(c),
                                };
                                if n.is_infinite() {
                                    return Completion::Throw(
                                        interp.create_range_error(&format!(
                                            "{} must be finite",
                                            $name
                                        )),
                                    );
                                }
                                if n.is_nan() { 0.0 } else { n }
                            }
                        }};
                    }

                    // Alphabetical: day, hour, microsecond, millisecond, minute
                    let nd_raw = field_or_default!("day", d) as i32;
                    let nh_raw = field_or_default!("hour", h) as i32;
                    let nus_raw = field_or_default!("microsecond", us) as i32;
                    let nms_raw = field_or_default!("millisecond", ms) as i32;
                    let nmi_raw = field_or_default!("minute", mi) as i32;
                    // month, monthCode
                    let (raw_month, raw_month_code) = match read_month_fields(interp, &bag) {
                        Ok(v) => {
                            if v.0.is_some() || v.1.is_some() {
                                has_any = true;
                            }
                            v
                        }
                        Err(c) => return c,
                    };
                    // nanosecond
                    let nns_raw = field_or_default!("nanosecond", nanos) as i32;
                    // offset
                    let offset_prop = match get_prop(interp, &bag, "offset") {
                        Completion::Normal(v) => v,
                        c => return c,
                    };
                    if !is_undefined(&offset_prop) {
                        has_any = true;
                        match &offset_prop {
                            JsValue::String(s) => {
                                let s_str = s.to_rust_string();
                                if super::parse_utc_offset_timezone(&s_str).is_none() {
                                    return Completion::Throw(interp.create_range_error(&format!(
                                        "invalid offset string: {s_str}"
                                    )));
                                }
                            }
                            JsValue::Object(_) => {
                                let s_str = match interp.to_string_value(&offset_prop) {
                                    Ok(s) => s,
                                    Err(c) => return Completion::Throw(c),
                                };
                                if super::parse_utc_offset_timezone(&s_str).is_none() {
                                    return Completion::Throw(interp.create_range_error(&format!(
                                        "invalid offset string: {s_str}"
                                    )));
                                }
                            }
                            _ => {
                                return Completion::Throw(
                                    interp.create_type_error("offset must be a string"),
                                );
                            }
                        }
                    }
                    // second, year
                    let ns2_raw = field_or_default!("second", s) as i32;
                    let ny = field_or_default!("year", y) as i32;

                    if !has_any {
                        return Completion::Throw(interp.create_type_error(
                            "with requires at least one recognized temporal property",
                        ));
                    }

                    if nd_raw < 1 {
                        return Completion::Throw(interp.create_range_error("day out of range"));
                    }

                    // Read options: disambiguation (default "compatible"), offset (default "prefer"), overflow (default "constrain")
                    let opts = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    let (_disambiguation, offset_opt, overflow) =
                        match parse_zdt_options(interp, &opts, "prefer") {
                            Ok(v) => v,
                            Err(c) => return c,
                        };

                    // Resolve month/monthCode after options are read
                    let nm_raw = match resolve_month_fields(interp, raw_month, raw_month_code, m) {
                        Ok(v) => v as i32,
                        Err(c) => return c,
                    };

                    // Apply overflow behavior to ALL fields
                    let (nm, nd, nh, nmi, ns2, nms, nus, nns) = if overflow == "reject" {
                        if !(1..=12).contains(&nm_raw) {
                            return Completion::Throw(
                                interp.create_range_error("month out of range"),
                            );
                        }
                        let max_day = super::iso_days_in_month(ny, nm_raw as u8) as i32;
                        if nd_raw < 1 || nd_raw > max_day {
                            return Completion::Throw(
                                interp.create_range_error("day out of range"),
                            );
                        }
                        if !(0..=23).contains(&nh_raw) {
                            return Completion::Throw(
                                interp.create_range_error("hour out of range"),
                            );
                        }
                        if !(0..=59).contains(&nmi_raw) {
                            return Completion::Throw(
                                interp.create_range_error("minute out of range"),
                            );
                        }
                        if !(0..=59).contains(&ns2_raw) {
                            return Completion::Throw(
                                interp.create_range_error("second out of range"),
                            );
                        }
                        if !(0..=999).contains(&nms_raw) {
                            return Completion::Throw(
                                interp.create_range_error("millisecond out of range"),
                            );
                        }
                        if !(0..=999).contains(&nus_raw) {
                            return Completion::Throw(
                                interp.create_range_error("microsecond out of range"),
                            );
                        }
                        if !(0..=999).contains(&nns_raw) {
                            return Completion::Throw(
                                interp.create_range_error("nanosecond out of range"),
                            );
                        }
                        (
                            nm_raw as u8,
                            nd_raw as u8,
                            nh_raw as u8,
                            nmi_raw as u8,
                            ns2_raw as u8,
                            nms_raw as u16,
                            nus_raw as u16,
                            nns_raw as u16,
                        )
                    } else {
                        let nm = nm_raw.clamp(1, 12) as u8;
                        let max_day = super::iso_days_in_month(ny, nm);
                        let nd = nd_raw.clamp(1, max_day as i32) as u8;
                        let nh = nh_raw.clamp(0, 23) as u8;
                        let nmi = nmi_raw.clamp(0, 59) as u8;
                        let ns2 = ns2_raw.clamp(0, 59) as u8;
                        let nms = nms_raw.clamp(0, 999) as u16;
                        let nus = nus_raw.clamp(0, 999) as u16;
                        let nns = nns_raw.clamp(0, 999) as u16;
                        (nm, nd, nh, nmi, ns2, nms, nus, nns)
                    };

                    let epoch_days = super::iso_date_to_epoch_days(ny, nm, nd) as i128;
                    let day_ns = nh as i128 * NS_PER_HOUR
                        + nmi as i128 * NS_PER_MIN
                        + ns2 as i128 * NS_PER_SEC
                        + nms as i128 * NS_PER_MS
                        + nus as i128 * 1_000
                        + nns as i128;
                    let local_ns = epoch_days * NS_PER_DAY + day_ns;

                    // Determine offset based on offset option
                    let tz_offset_ns = get_tz_offset_ns(&tz, &BigInt::from(local_ns)) as i128;
                    let offset_ns = if !is_undefined(&offset_prop) {
                        let off_str = match interp.to_string_value(&offset_prop) {
                            Ok(s) => s,
                            Err(c) => return Completion::Throw(c),
                        };
                        let user_offset_ns = match super::parse_utc_offset_timezone(&off_str) {
                            Some(canonical) => super::offset_string_to_ns(&canonical),
                            None => tz_offset_ns,
                        };
                        if offset_opt == "reject" && user_offset_ns != tz_offset_ns {
                            return Completion::Throw(interp.create_range_error(
                                "offset does not match the time zone offset",
                            ));
                        } else if offset_opt == "use" {
                            user_offset_ns
                        } else {
                            // "prefer" or "ignore": use timezone offset
                            tz_offset_ns
                        }
                    } else {
                        tz_offset_ns
                    };
                    let new_epoch_ns = BigInt::from(local_ns - offset_ns);
                    create_zdt(interp, new_epoch_ns, tz, cal)
                },
            ));
            proto
                .borrow_mut()
                .insert_builtin("with".to_string(), method);
        }

        // add(duration)
        {
            let method = self.create_function(JsFunction::native(
                "add".to_string(),
                1,
                |interp, this, args| zdt_add_subtract(interp, this, args, 1),
            ));
            proto.borrow_mut().insert_builtin("add".to_string(), method);
        }

        // subtract(duration)
        {
            let method = self.create_function(JsFunction::native(
                "subtract".to_string(),
                1,
                |interp, this, args| zdt_add_subtract(interp, this, args, -1),
            ));
            proto
                .borrow_mut()
                .insert_builtin("subtract".to_string(), method);
        }

        // until(other, options?)
        {
            let method = self.create_function(JsFunction::native(
                "until".to_string(),
                1,
                |interp, this, args| zdt_until_since(interp, this, args, 1),
            ));
            proto
                .borrow_mut()
                .insert_builtin("until".to_string(), method);
        }

        // since(other, options?)
        {
            let method = self.create_function(JsFunction::native(
                "since".to_string(),
                1,
                |interp, this, args| zdt_until_since(interp, this, args, -1),
            ));
            proto
                .borrow_mut()
                .insert_builtin("since".to_string(), method);
        }

        // round(options)
        {
            let method = self.create_function(JsFunction::native(
                "round".to_string(),
                1,
                |interp, this, args| {
                    let (ns, tz, cal) = match get_zdt_fields(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let options_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if is_undefined(&options_arg) {
                        return Completion::Throw(
                            interp.create_type_error("round requires options"),
                        );
                    }

                    let allowed_units: &[&str] = &[
                        "day",
                        "hour",
                        "minute",
                        "second",
                        "millisecond",
                        "microsecond",
                        "nanosecond",
                    ];
                    let (smallest_unit, rounding_mode, increment) = if let JsValue::String(s) =
                        &options_arg
                    {
                        let su = s.to_rust_string();
                        let unit = match temporal_unit_singular(&su) {
                            Some(u) if allowed_units.contains(&u) => u,
                            Some(u) => {
                                return Completion::Throw(interp.create_range_error(&format!(
                                    "\"{u}\" is not a valid value for smallest unit"
                                )));
                            }
                            None => {
                                return Completion::Throw(
                                    interp.create_range_error(&format!("Invalid unit: {su}")),
                                );
                            }
                        };
                        (unit, "halfExpand".to_string(), 1.0)
                    } else if matches!(options_arg, JsValue::Object(_)) {
                        // Per spec: read roundingIncrement first, then roundingMode, then smallestUnit
                        let ri_val = match get_prop(interp, &options_arg, "roundingIncrement") {
                            Completion::Normal(v) => v,
                            c => return c,
                        };
                        let increment_raw = match super::coerce_rounding_increment(interp, &ri_val)
                        {
                            Ok(v) => v,
                            Err(c) => return c,
                        };
                        let rm_val = match get_prop(interp, &options_arg, "roundingMode") {
                            Completion::Normal(v) => v,
                            c => return c,
                        };
                        let rounding_mode = if is_undefined(&rm_val) {
                            "halfExpand".to_string()
                        } else {
                            let v = match interp.to_string_value(&rm_val) {
                                Ok(s) => s,
                                Err(c) => return Completion::Throw(c),
                            };
                            match v.as_str() {
                                "ceil" | "floor" | "trunc" | "expand" | "halfExpand"
                                | "halfTrunc" | "halfCeil" | "halfFloor" | "halfEven" => v,
                                _ => {
                                    return Completion::Throw(interp.create_range_error(&format!(
                                        "{v} is not a valid value for roundingMode"
                                    )));
                                }
                            }
                        };
                        let su_val = match get_prop(interp, &options_arg, "smallestUnit") {
                            Completion::Normal(v) => v,
                            c => return c,
                        };
                        if is_undefined(&su_val) {
                            return Completion::Throw(
                                interp.create_range_error("smallestUnit is required for round"),
                            );
                        }
                        let su_str = match interp.to_string_value(&su_val) {
                            Ok(s) => s,
                            Err(c) => return Completion::Throw(c),
                        };
                        let unit = match temporal_unit_singular(&su_str) {
                            Some(u) if allowed_units.contains(&u) => u,
                            Some(u) => {
                                return Completion::Throw(interp.create_range_error(&format!(
                                    "\"{u}\" is not a valid value for smallest unit"
                                )));
                            }
                            None => {
                                return Completion::Throw(
                                    interp.create_range_error(&format!("Invalid unit: {su_str}")),
                                );
                            }
                        };
                        let increment = match super::validate_rounding_increment_raw(
                            increment_raw,
                            unit,
                            false,
                        ) {
                            Ok(v) => v,
                            Err(msg) => return Completion::Throw(interp.create_range_error(&msg)),
                        };
                        // ZDT.round per-unit max check (stricter than Instant):
                        // day: must be 1; others: < unit max and divides max
                        let inc_i = increment as u64;
                        if unit == "day" {
                            if inc_i > 1 {
                                return Completion::Throw(
                                    interp
                                        .create_range_error("roundingIncrement for day must be 1"),
                                );
                            }
                        } else if let Some(max) = super::max_rounding_increment(unit) {
                            if inc_i >= max {
                                return Completion::Throw(interp.create_range_error(&format!(
                                    "roundingIncrement {increment} is out of range for {unit}"
                                )));
                            }
                            if max % inc_i != 0 {
                                return Completion::Throw(interp.create_range_error(&format!(
                                    "{increment} does not divide evenly into {max}"
                                )));
                            }
                        }
                        (unit, rounding_mode, increment)
                    } else {
                        return Completion::Throw(
                            interp.create_type_error("round requires string or object options"),
                        );
                    };

                    let unit_ns = temporal_unit_length_ns(smallest_unit) as i128;
                    let offset_ns = get_tz_offset_ns(&tz, &ns) as i128;
                    let total_ns: i128 = (&ns).try_into().unwrap_or(0);

                    // For day rounding, we need to compute relative to start of day
                    let rounded_ns = if smallest_unit == "day" {
                        // Per spec: use GetStartOfDay for day boundaries
                        let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(&ns, &tz);
                        let today_days = super::iso_date_to_epoch_days(y, m, d);
                        if today_days.abs() > 100_000_000 {
                            return Completion::Throw(
                                interp.create_range_error("date outside representable range"),
                            );
                        }
                        let (ny, nm, nd) = super::balance_iso_date(y, m as i32, d as i32 + 1);
                        let tomorrow_days = super::iso_date_to_epoch_days(ny, nm, nd);
                        if tomorrow_days.abs() > 100_000_000 {
                            return Completion::Throw(
                                interp.create_range_error("next day outside representable range"),
                            );
                        }
                        let start_ns = get_start_of_day(&tz, today_days as i128);
                        let end_ns = get_start_of_day(&tz, tomorrow_days as i128);
                        let day_length_ns = end_ns - start_ns;
                        let day_progress_ns = total_ns - start_ns;
                        let rounded_day_ns =
                            round_ns_to_increment(day_progress_ns, day_length_ns, &rounding_mode);
                        start_ns + rounded_day_ns
                    } else {
                        let inc_ns = unit_ns * increment as i128;
                        let local_ns = total_ns + offset_ns;
                        let (y, m, d, _, _, _, _, _, _) = epoch_ns_to_components(&ns, &tz);
                        let start_days = super::iso_date_to_epoch_days(y, m, d) as i128;
                        let start_local = start_days * NS_PER_DAY;
                        let day_ns = local_ns - start_local;
                        let rounded_day_ns = round_ns_to_increment(day_ns, inc_ns, &rounding_mode);
                        let new_local = start_local + rounded_day_ns;
                        new_local - offset_ns
                    };

                    create_zdt(interp, BigInt::from(rounded_ns), tz, cal)
                },
            ));
            proto
                .borrow_mut()
                .insert_builtin("round".to_string(), method);
        }

        // getISOFields()
        {
            let method = self.create_function(JsFunction::native(
                "getISOFields".to_string(),
                0,
                |interp, this, _args| {
                    let (ns, tz, cal) = match get_zdt_fields(interp, this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let (y, m, d, h, mi, s, ms, us, nanos) = epoch_ns_to_components(&ns, &tz);
                    let offset_ns = get_tz_offset_ns(&tz, &ns);
                    let result = interp.create_object();
                    macro_rules! set_field {
                        ($name:expr, $val:expr) => {
                            result.borrow_mut().insert_property(
                                $name.to_string(),
                                PropertyDescriptor::data($val, true, true, true),
                            );
                        };
                    }
                    set_field!("calendar", JsValue::String(JsString::from_str(&cal)));
                    set_field!("isoDay", JsValue::Number(d as f64));
                    set_field!("isoHour", JsValue::Number(h as f64));
                    set_field!("isoMicrosecond", JsValue::Number(us as f64));
                    set_field!("isoMillisecond", JsValue::Number(ms as f64));
                    set_field!("isoMinute", JsValue::Number(mi as f64));
                    set_field!("isoMonth", JsValue::Number(m as f64));
                    set_field!("isoNanosecond", JsValue::Number(nanos as f64));
                    set_field!("isoSecond", JsValue::Number(s as f64));
                    set_field!("isoYear", JsValue::Number(y as f64));
                    set_field!(
                        "offset",
                        JsValue::String(JsString::from_str(&format_offset_string(offset_ns)))
                    );
                    set_field!("timeZone", JsValue::String(JsString::from_str(&tz)));

                    let id = result.borrow().id.unwrap();
                    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
                },
            ));
            proto
                .borrow_mut()
                .insert_builtin("getISOFields".to_string(), method);
        }

        // getTimeZoneTransition(direction)
        {
            let method = self.create_function(JsFunction::native(
                "getTimeZoneTransition".to_string(),
                1,
                |interp, this, args| {
                    let (ns, tz, cal) = match get_zdt_fields(interp, &this) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let options_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if is_undefined(&options_arg) {
                        return Completion::Throw(
                            interp.create_type_error("getTimeZoneTransition requires an argument"),
                        );
                    }
                    // Accept string shorthand or options object
                    let direction = if let JsValue::String(s) = &options_arg {
                        s.to_rust_string()
                    } else if matches!(options_arg, JsValue::Object(_)) {
                        let dir_val = match get_prop(interp, &options_arg, "direction") {
                            Completion::Normal(v) => v,
                            c => return c,
                        };
                        if is_undefined(&dir_val) {
                            return Completion::Throw(interp.create_range_error(
                                "direction is required for getTimeZoneTransition",
                            ));
                        }
                        match interp.to_string_value(&dir_val) {
                            Ok(s) => s,
                            Err(c) => return Completion::Throw(c),
                        }
                    } else {
                        return Completion::Throw(interp.create_type_error(
                            "getTimeZoneTransition requires a string or object argument",
                        ));
                    };
                    match direction.as_str() {
                        "next" | "previous" => {}
                        _ => {
                            return Completion::Throw(interp.create_range_error(&format!(
                                "{direction} is not a valid value for direction"
                            )));
                        }
                    }
                    let epoch_ns_i128: i128 = ns.try_into().unwrap_or(0);
                    // Offset time zones have no transitions
                    if tz.starts_with('+') || tz.starts_with('-') || tz == "UTC" {
                        return Completion::Normal(JsValue::Null);
                    }
                    let transition = if direction == "next" {
                        get_next_transition(&tz, epoch_ns_i128)
                    } else {
                        get_previous_transition(&tz, epoch_ns_i128)
                    };
                    match transition {
                        Some(t) => {
                            let t_bi = BigInt::from(t);
                            if !is_valid_epoch_ns(&t_bi) {
                                Completion::Normal(JsValue::Null)
                            } else {
                                create_zdt(interp, t_bi, tz, cal)
                            }
                        }
                        None => Completion::Normal(JsValue::Null),
                    }
                },
            ));
            proto
                .borrow_mut()
                .insert_builtin("getTimeZoneTransition".to_string(), method);
        }

        // --- Constructor ---
        let constructor = self.create_function(JsFunction::constructor(
            "ZonedDateTime".to_string(),
            2,
            |interp, _this, args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Temporal.ZonedDateTime must be called with new"),
                    );
                }
                let ns_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let ns_bigint = match to_bigint_arg(interp, &ns_arg) {
                    Ok(n) => n,
                    Err(c) => return c,
                };

                let tz_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let tz = match validate_timezone_identifier_strict(interp, &tz_arg) {
                    Ok(t) => t,
                    Err(c) => return c,
                };

                let cal_arg = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                let cal = match validate_calendar_strict(interp, &cal_arg) {
                    Ok(c) => c,
                    Err(c) => return c,
                };

                create_zdt(interp, ns_bigint, tz, cal)
            },
        ));

        if let JsValue::Object(ref o) = constructor
            && let Some(obj) = self.get_object(o.id)
        {
            let proto_val = JsValue::Object(crate::types::JsObject {
                id: proto.borrow().id.unwrap(),
            });
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val, false, false, false),
            );
        }
        proto.borrow_mut().insert_property(
            "constructor".to_string(),
            PropertyDescriptor::data(constructor.clone(), true, false, true),
        );

        // --- Static methods ---

        // ZonedDateTime.from(item, options?)
        {
            let from_fn = self.create_function(JsFunction::native(
                "from".to_string(),
                1,
                |interp, _this, args| {
                    let item = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    if matches!(&item, JsValue::String(_)) {
                        return from_string_with_options(interp, &item, &options);
                    }
                    if !matches!(&item, JsValue::Object(_)) {
                        return Completion::Throw(
                            interp.create_type_error("invalid type for ZonedDateTime.from"),
                        );
                    }
                    // Check if item is already a ZDT — read options then clone
                    let is_zdt = if let JsValue::Object(o) = &item {
                        interp
                            .get_object(o.id)
                            .map(|obj| {
                                matches!(
                                    obj.borrow().temporal_data,
                                    Some(TemporalData::ZonedDateTime { .. })
                                )
                            })
                            .unwrap_or(false)
                    } else {
                        false
                    };
                    if is_zdt {
                        let (disambiguation, offset_opt, overflow) =
                            match parse_zdt_options(interp, &options, "reject") {
                                Ok(v) => v,
                                Err(c) => return c,
                            };
                        return to_temporal_zoned_date_time_with_options(
                            interp,
                            &item,
                            &overflow,
                            &disambiguation,
                            &offset_opt,
                            None,
                        );
                    }
                    // Property bag: read bag fields first, then options (deferred)
                    to_temporal_zoned_date_time_with_options(
                        interp,
                        &item,
                        "",
                        "",
                        "", // unused when deferred_options is Some
                        Some((&options, "reject")),
                    )
                },
            ));
            if let JsValue::Object(ref o) = constructor
                && let Some(obj) = self.get_object(o.id)
            {
                obj.borrow_mut().insert_builtin("from".to_string(), from_fn);
            }
        }

        // ZonedDateTime.compare(one, two)
        {
            let compare_fn = self.create_function(JsFunction::native(
                "compare".to_string(),
                2,
                |interp, _this, args| {
                    let one_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let two_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    let one_val = match to_temporal_zoned_date_time(interp, &one_arg) {
                        Completion::Normal(v) => v,
                        c => return c,
                    };
                    let two_val = match to_temporal_zoned_date_time(interp, &two_arg) {
                        Completion::Normal(v) => v,
                        c => return c,
                    };
                    let (ns1, _, _) = match get_zdt_fields(interp, &one_val) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let (ns2, _, _) = match get_zdt_fields(interp, &two_val) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let result = if ns1 < ns2 {
                        -1.0
                    } else if ns1 > ns2 {
                        1.0
                    } else {
                        0.0
                    };
                    Completion::Normal(JsValue::Number(result))
                },
            ));
            if let JsValue::Object(ref o) = constructor
                && let Some(obj) = self.get_object(o.id)
            {
                obj.borrow_mut()
                    .insert_builtin("compare".to_string(), compare_fn);
            }
        }

        temporal_obj.borrow_mut().insert_property(
            "ZonedDateTime".to_string(),
            PropertyDescriptor::data(constructor, true, false, true),
        );
    }
}

fn zdt_add_subtract(
    interp: &mut Interpreter,
    this: &JsValue,
    args: &[JsValue],
    sign: i32,
) -> Completion {
    let (ns, tz, cal) = match get_zdt_fields(interp, this) {
        Ok(v) => v,
        Err(c) => return c,
    };
    let dur_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
    let dur = match super::duration::to_temporal_duration_record(interp, dur_arg) {
        Ok(d) => d,
        Err(c) => return c,
    };
    let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
    let overflow = match parse_overflow_option(interp, &options) {
        Ok(v) => v,
        Err(c) => return c,
    };

    let (
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
    ) = dur;

    // Date part: add years/months/weeks/days relative to local date
    let (y, m, d, h, mi, s, ms, us, nanos) = epoch_ns_to_components(&ns, &tz);

    let sy = (years * sign as f64) as i32;
    let sm = (months * sign as f64) as i32;
    let sw = (weeks * sign as f64) as i32;
    let sd = (days * sign as f64) as i32;
    let (ny, nm, nd) = if years != 0.0 || months != 0.0 || weeks != 0.0 || days != 0.0 {
        if cal != "iso8601" && (sy != 0 || sm != 0) {
            match super::add_calendar_date(y, m, d, sy, sm, sw, sd, &cal, &overflow) {
                Some(v) => v,
                None => {
                    return Completion::Throw(
                        interp
                            .create_range_error("day is out of range for the resulting month"),
                    );
                }
            }
        } else {
            match super::add_iso_date_with_overflow(y, m, d, sy, sm, sw, sd, &overflow) {
                Ok(v) => v,
                Err(()) => {
                    return Completion::Throw(
                        interp.create_range_error("day is out of range for the resulting month"),
                    );
                }
            }
        }
    } else {
        (y, m, d)
    };

    // Time part delta in nanoseconds
    let time_ns = (hours * sign as f64) as i128 * NS_PER_HOUR
        + (minutes * sign as f64) as i128 * NS_PER_MIN
        + (seconds * sign as f64) as i128 * NS_PER_SEC
        + (milliseconds * sign as f64) as i128 * NS_PER_MS
        + (microseconds * sign as f64) as i128 * 1_000
        + (nanoseconds * sign as f64) as i128;

    // If date part was added, rebuild local wall-clock then re-resolve
    if years != 0.0 || months != 0.0 || weeks != 0.0 || days != 0.0 {
        let epoch_days = super::iso_date_to_epoch_days(ny, nm, nd) as i128;
        let day_ns = h as i128 * NS_PER_HOUR
            + mi as i128 * NS_PER_MIN
            + s as i128 * NS_PER_SEC
            + ms as i128 * NS_PER_MS
            + us as i128 * 1_000
            + nanos as i128;
        let local_ns = epoch_days * NS_PER_DAY + day_ns;

        // Resolve local time in timezone using compatible disambiguation
        let intermediate_epoch = disambiguate_instant(&tz, local_ns, "compatible");
        // Add time part directly to epoch (no re-resolution)
        let new_epoch_ns = BigInt::from(intermediate_epoch + time_ns);
        create_zdt(interp, new_epoch_ns, tz, cal)
    } else {
        // Time-only duration: add directly to epoch nanoseconds
        let epoch: i128 = (&ns).try_into().unwrap_or(0);
        let new_epoch_ns = BigInt::from(epoch + time_ns);
        create_zdt(interp, new_epoch_ns, tz, cal)
    }
}

fn zdt_until_since(
    interp: &mut Interpreter,
    this: &JsValue,
    args: &[JsValue],
    sign: i32,
) -> Completion {
    let (ns1, tz, cal) = match get_zdt_fields(interp, this) {
        Ok(v) => v,
        Err(c) => return c,
    };
    let other_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
    let other_val = match to_temporal_zoned_date_time(interp, &other_arg) {
        Completion::Normal(v) => v,
        c => return c,
    };
    let (ns2, tz2, cal2) = match get_zdt_fields(interp, &other_val) {
        Ok(v) => v,
        Err(c) => return c,
    };
    if cal != cal2 {
        return Completion::Throw(interp.create_range_error(
            &format!("cannot compute difference between dates of different calendars: {} and {}", cal, cal2),
        ));
    }

    let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
    let all_units: &[&str] = &[
        "year",
        "month",
        "week",
        "day",
        "hour",
        "minute",
        "second",
        "millisecond",
        "microsecond",
        "nanosecond",
    ];
    let (largest_unit, smallest_unit, rounding_mode, rounding_increment) =
        match super::parse_difference_options(interp, &options, "hour", all_units) {
            Ok(v) => v,
            Err(c) => return c,
        };

    // Per spec: TimeZoneEquals check only when largestUnit is day or larger
    if matches!(largest_unit.as_str(), "year" | "month" | "week" | "day") {
        let tz_eq = tz.eq_ignore_ascii_case(&tz2)
            || super::canonicalize_iana_tz(&tz) == super::canonicalize_iana_tz(&tz2);
        if !tz_eq {
            return Completion::Throw(interp.create_range_error(&format!(
                "Time zones '{}' and '{}' are not equal",
                tz, tz2
            )));
        }
    }

    let diff_ns: i128 = {
        let n1: i128 = (&ns1).try_into().unwrap_or(0);
        let n2: i128 = (&ns2).try_into().unwrap_or(0);
        (n2 - n1) * sign as i128
    };

    // DifferenceZonedDateTime: compute from receiver (this = ns1) perspective
    if matches!(largest_unit.as_str(), "year" | "month" | "week" | "day") {
        let n1: i128 = (&ns1).try_into().unwrap_or(0);
        let n2: i128 = (&ns2).try_into().unwrap_or(0);
        let ns_diff = n2 - n1;

        if ns_diff == 0 {
            return super::duration::create_duration_result(
                interp, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            );
        }

        // Decompose this (ns1) and other (ns2) to wall-clock date/time
        let (ty, tm, td, th, tmi, ts, tms, tus, tns_c) = epoch_ns_to_components(&ns1, &tz);
        let (oy, om, od, oh, omi, os, oms, ous, ons_c) = epoch_ns_to_components(&ns2, &tz);

        let this_time_ns = th as i128 * NS_PER_HOUR
            + tmi as i128 * NS_PER_MIN
            + ts as i128 * NS_PER_SEC
            + tms as i128 * NS_PER_MS
            + tus as i128 * 1_000
            + tns_c as i128;

        // Day correction per DifferenceZonedDateTime spec:
        // poly_sign: 1 when ns2 < ns1 (backward), -1 when ns2 > ns1 (forward)
        let poly_sign: i32 = if ns_diff < 0 { 1 } else { -1 };
        let other_time_ns = oh as i128 * NS_PER_HOUR
            + omi as i128 * NS_PER_MIN
            + os as i128 * NS_PER_SEC
            + oms as i128 * NS_PER_MS
            + ous as i128 * 1_000
            + ons_c as i128;
        let time_diff_sign: i32 = {
            let td = other_time_ns - this_time_ns;
            if td > 0 {
                1
            } else if td < 0 {
                -1
            } else {
                0
            }
        };

        let mut time_remainder: i128;
        let mut adj_oy: i32;
        let mut adj_om: u8;
        let mut adj_od: u8;

        // When ISO dates are the same, skip day correction entirely.
        // The time remainder is just the raw epoch nanosecond difference.
        // This handles same-date-reverse-wallclock during DST transitions
        // (per tc39/proposal-temporal#3141).
        if ty == oy as i32 && tm == om && td == od {
            adj_oy = oy as i32;
            adj_om = om;
            adj_od = od;
            time_remainder = ns_diff;
        } else {
            let mut day_correction: i32 = if time_diff_sign == poly_sign { 1 } else { 0 };
            let max_correction = if poly_sign == -1 { 2 } else { 1 };
            adj_oy = oy as i32;
            adj_om = om;
            adj_od = od;

            loop {
                let (ay, am, ad) = if day_correction == 0 {
                    (oy as i32, om, od)
                } else {
                    super::balance_iso_date(
                        oy as i32,
                        om as i32,
                        od as i32 + day_correction * poly_sign,
                    )
                };
                let int_epoch = super::iso_date_to_epoch_days(ay, am, ad) as i128;
                let int_local = int_epoch * NS_PER_DAY + this_time_ns;
                let int_off = get_tz_offset_ns(&tz, &BigInt::from(int_local)) as i128;
                let int_ns = int_local - int_off;

                time_remainder = n2 - int_ns;
                let tr_sign = if time_remainder > 0 {
                    1
                } else if time_remainder < 0 {
                    -1
                } else {
                    0
                };
                if tr_sign != poly_sign {
                    adj_oy = ay;
                    adj_om = am;
                    adj_od = ad;
                    break;
                }
                day_correction += 1;
                if day_correction > max_correction {
                    adj_oy = ay;
                    adj_om = am;
                    adj_od = ad;
                    break;
                }
            }
        }

        // dateUntil from this's perspective (asymmetric)
        let (mut dy, mut dm, mut dw, dd) =
            if cal != "iso8601" && matches!(largest_unit.as_str(), "year" | "month") {
                match super::difference_calendar_date(
                    ty, tm, td, adj_oy, adj_om, adj_od, &largest_unit, &cal,
                ) {
                    Some(v) => v,
                    None => super::difference_iso_date(
                        ty, tm, td, adj_oy, adj_om, adj_od, &largest_unit,
                    ),
                }
            } else {
                super::difference_iso_date(ty, tm, td, adj_oy, adj_om, adj_od, &largest_unit)
            };
        let mut dd = dd as i64;

        // Decompose time remainder (already signed, same direction as date components)
        let mut dh = (time_remainder / NS_PER_HOUR) as i64;
        let mut dmi = ((time_remainder % NS_PER_HOUR) / NS_PER_MIN) as i64;
        let mut ds = ((time_remainder % NS_PER_MIN) / NS_PER_SEC) as i64;
        let mut dms = ((time_remainder % NS_PER_SEC) / NS_PER_MS) as i64;
        let mut dus = ((time_remainder % NS_PER_MS) / 1_000) as i64;
        let mut dns = (time_remainder % 1_000) as i64;

        // Per spec DifferenceTemporal: for "since", negate rounding mode BEFORE rounding,
        // round the signed result, THEN negate the final result.
        let effective_mode = if sign == -1 {
            super::negate_rounding_mode(&rounding_mode)
        } else {
            rounding_mode.clone()
        };

        // Rounding reference: always the receiver's date (this = ns1)
        let ref_y = ty;
        let ref_m = tm;
        let ref_d = td;

        // Apply rounding on signed values
        if smallest_unit != "nanosecond" || rounding_increment != 1.0 {
            let su_order = super::temporal_unit_order(&smallest_unit);
            if su_order >= super::temporal_unit_order("day") {
                let time_ns_i128: i128 = dh as i128 * 3_600_000_000_000
                    + dmi as i128 * 60_000_000_000
                    + ds as i128 * 1_000_000_000
                    + dms as i128 * 1_000_000
                    + dus as i128 * 1_000
                    + dns as i128;
                let time_ns = time_ns_i128 as f64;
                let fractional_days = dd as f64 + time_ns / 86_400_000_000_000.0;
                let (mut ry2, mut rm2, rw2, rd2) = match super::round_date_duration_with_frac_days(
                    dy,
                    dm,
                    dw,
                    fractional_days,
                    time_ns_i128,
                    &smallest_unit,
                    &largest_unit,
                    rounding_increment,
                    &effective_mode,
                    ref_y,
                    ref_m,
                    ref_d,
                    true,
                ) {
                    Ok(v) => v,
                    Err(msg) => return Completion::Throw(interp.create_range_error(&msg)),
                };
                // Rebalance months overflow into years when largestUnit is year
                if matches!(largest_unit.as_str(), "year") && rm2.abs() >= 12 {
                    ry2 += rm2 / 12;
                    rm2 %= 12;
                }
                // For since (sign=-1): negate the rounded result
                let nf = if sign == -1 { -1.0 } else { 1.0 };
                return super::duration::create_duration_result(
                    interp,
                    ry2 as f64 * nf,
                    rm2 as f64 * nf,
                    rw2 as f64 * nf,
                    rd2 as f64 * nf,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                );
            } else {
                // Time unit rounding on signed values
                let unit_ns = super::temporal_unit_length_ns(&smallest_unit) as i128;
                let increment_ns = unit_ns * rounding_increment as i128;
                let rounded = round_ns_i128(time_remainder, increment_ns, &effective_mode);
                dns = (rounded % 1000) as i64;
                let rem = rounded / 1000;
                dus = (rem % 1000) as i64;
                let rem = rem / 1000;
                dms = (rem % 1000) as i64;
                let rem = rem / 1000;
                ds = (rem % 60) as i64;
                let rem = rem / 60;
                dmi = (rem % 60) as i64;
                let rem = rem / 60;
                dh = rem as i64;
                // Cascade day overflow from time rounding into calendar units
                if dh.abs() >= 24 {
                    let day_overflow = if dh >= 0 { dh / 24 } else { -((-dh) / 24) };
                    dh -= day_overflow * 24;
                    dd += day_overflow;
                    let lu_order = super::temporal_unit_order(&largest_unit);
                    if lu_order >= super::temporal_unit_order("month") {
                        let intermediate = super::add_iso_date(ref_y, ref_m, ref_d, dy, dm, 0, 0);
                        let target = super::add_iso_date(
                            intermediate.0,
                            intermediate.1,
                            intermediate.2,
                            0,
                            0,
                            0,
                            dd as i32,
                        );
                        let (ny, nm, _, nd) = super::difference_iso_date(
                            ref_y,
                            ref_m,
                            ref_d,
                            target.0,
                            target.1,
                            target.2,
                            &largest_unit,
                        );
                        dy = ny;
                        dm = nm;
                        dd = nd as i64;
                    }
                }
            }
        }

        // For since (sign=-1): negate everything
        if sign == -1 {
            dy = -dy;
            dm = -dm;
            dw = -dw;
            dd = -dd;
            dh = -dh;
            dmi = -dmi;
            ds = -ds;
            dms = -dms;
            dus = -dus;
            dns = -dns;
        }

        return super::duration::create_duration_result(
            interp, dy as f64, dm as f64, dw as f64, dd as f64, dh as f64, dmi as f64, ds as f64,
            dms as f64, dus as f64, dns as f64,
        );
    }

    // Time units only
    let total_ns = diff_ns;
    let decompose_time = |total: i128, lu: &str| -> (i64, i64, i64, i64, i64, i64) {
        match lu {
            "hour" => {
                let h = total / NS_PER_HOUR;
                let rem = total % NS_PER_HOUR;
                let m = rem / NS_PER_MIN;
                let rem = rem % NS_PER_MIN;
                let s = rem / NS_PER_SEC;
                let rem = rem % NS_PER_SEC;
                let ms = rem / NS_PER_MS;
                let rem = rem % NS_PER_MS;
                let us = rem / 1_000;
                let ns = rem % 1_000;
                (
                    h as i64, m as i64, s as i64, ms as i64, us as i64, ns as i64,
                )
            }
            "minute" => {
                let m = total / NS_PER_MIN;
                let rem = total % NS_PER_MIN;
                let s = rem / NS_PER_SEC;
                let rem = rem % NS_PER_SEC;
                let ms = rem / NS_PER_MS;
                let rem = rem % NS_PER_MS;
                let us = rem / 1_000;
                let ns = rem % 1_000;
                (0, m as i64, s as i64, ms as i64, us as i64, ns as i64)
            }
            "second" => {
                let s = total / NS_PER_SEC;
                let rem = total % NS_PER_SEC;
                let ms = rem / NS_PER_MS;
                let rem = rem % NS_PER_MS;
                let us = rem / 1_000;
                let ns = rem % 1_000;
                (0, 0, s as i64, ms as i64, us as i64, ns as i64)
            }
            "millisecond" => {
                let ms = total / NS_PER_MS;
                let rem = total % NS_PER_MS;
                let us = rem / 1_000;
                let ns = rem % 1_000;
                (0, 0, 0, ms as i64, us as i64, ns as i64)
            }
            "microsecond" => {
                let us = total / 1_000;
                let ns = total % 1_000;
                (0, 0, 0, 0, us as i64, ns as i64)
            }
            _ => (0, 0, 0, 0, 0, total as i64),
        }
    };

    let (mut dh, mut dmi, mut ds, mut dms, mut dus, mut dns) =
        decompose_time(total_ns, &largest_unit);

    // Apply rounding for time units — use i128 for precision
    if smallest_unit != "nanosecond" || rounding_increment != 1.0 {
        let unit_ns = super::temporal_unit_length_ns(&smallest_unit) as i128;
        let increment_ns = unit_ns * rounding_increment as i128;
        let rounded = round_ns_i128(total_ns, increment_ns, &rounding_mode);
        let result = decompose_time(rounded, &largest_unit);
        dh = result.0;
        dmi = result.1;
        ds = result.2;
        dms = result.3;
        dus = result.4;
        dns = result.5;
    }

    super::duration::create_duration_result(
        interp, 0.0, 0.0, 0.0, 0.0, dh as f64, dmi as f64, ds as f64, dms as f64, dus as f64,
        dns as f64,
    )
}

fn parse_zdt_to_string_options(
    interp: &mut Interpreter,
    options: &JsValue,
) -> Result<(Option<i32>, String, String, String, String), Completion> {
    if is_undefined(options) {
        return Ok((
            None,
            "trunc".to_string(),
            "auto".to_string(),
            "auto".to_string(),
            "auto".to_string(),
        ));
    }
    if !matches!(options, JsValue::Object(_)) {
        return Err(Completion::Throw(
            interp.create_type_error("options must be an object"),
        ));
    }

    // Per spec: read options in alphabetical order.
    // calendarName
    let cn_val = match get_prop(interp, options, "calendarName") {
        Completion::Normal(v) => v,
        c => return Err(c),
    };
    let cal_display = if is_undefined(&cn_val) {
        "auto".to_string()
    } else {
        match interp.to_string_value(&cn_val) {
            Ok(s) => {
                if !matches!(s.as_str(), "auto" | "always" | "never" | "critical") {
                    return Err(Completion::Throw(
                        interp.create_range_error(&format!("Invalid calendarName option: {s}")),
                    ));
                }
                s
            }
            Err(c) => return Err(Completion::Throw(c)),
        }
    };

    // fractionalSecondDigits — ALWAYS read (even if smallestUnit overrides it)
    let fsd_val = match get_prop(interp, options, "fractionalSecondDigits") {
        Completion::Normal(v) => v,
        c => return Err(c),
    };
    let fsd_precision = if is_undefined(&fsd_val) {
        None
    } else if matches!(&fsd_val, JsValue::Number(_)) {
        let n = match interp.to_number_value(&fsd_val) {
            Ok(n) => n,
            Err(c) => return Err(Completion::Throw(c)),
        };
        if n.is_nan() || n.is_infinite() {
            return Err(Completion::Throw(interp.create_range_error(
                "fractionalSecondDigits must be a finite number",
            )));
        }
        let digits = n.floor() as i32;
        if !(0..=9).contains(&digits) {
            return Err(Completion::Throw(interp.create_range_error(&format!(
                "fractionalSecondDigits must be 0-9 or auto, got {n}"
            ))));
        }
        Some(digits)
    } else {
        let s = match interp.to_string_value(&fsd_val) {
            Ok(v) => v,
            Err(c) => return Err(Completion::Throw(c)),
        };
        if s == "auto" {
            None
        } else {
            return Err(Completion::Throw(interp.create_range_error(&format!(
                "Invalid fractionalSecondDigits: {s}"
            ))));
        }
    };

    // offset
    let off_val = match get_prop(interp, options, "offset") {
        Completion::Normal(v) => v,
        c => return Err(c),
    };
    let offset_display = if is_undefined(&off_val) {
        "auto".to_string()
    } else {
        match interp.to_string_value(&off_val) {
            Ok(s) => {
                if !matches!(s.as_str(), "auto" | "never") {
                    return Err(Completion::Throw(
                        interp.create_range_error(&format!("Invalid offset option: {s}")),
                    ));
                }
                s
            }
            Err(c) => return Err(Completion::Throw(c)),
        }
    };

    // roundingMode
    let rm_val = match get_prop(interp, options, "roundingMode") {
        Completion::Normal(v) => v,
        c => return Err(c),
    };
    let rounding_mode = if is_undefined(&rm_val) {
        "trunc".to_string()
    } else {
        match interp.to_string_value(&rm_val) {
            Ok(s) => {
                if !matches!(
                    s.as_str(),
                    "ceil"
                        | "floor"
                        | "expand"
                        | "trunc"
                        | "halfCeil"
                        | "halfFloor"
                        | "halfExpand"
                        | "halfTrunc"
                        | "halfEven"
                ) {
                    return Err(Completion::Throw(
                        interp.create_range_error(&format!("Invalid roundingMode: {s}")),
                    ));
                }
                s
            }
            Err(c) => return Err(Completion::Throw(c)),
        }
    };

    // smallestUnit — read and coerce but defer validation until after timeZoneName
    let su_val = match get_prop(interp, options, "smallestUnit") {
        Completion::Normal(v) => v,
        c => return Err(c),
    };
    let su_coerced = if !is_undefined(&su_val) {
        let su_str = match interp.to_string_value(&su_val) {
            Ok(s) => s,
            Err(c) => return Err(Completion::Throw(c)),
        };
        Some(su_str)
    } else {
        None
    };

    // timeZoneName — must be read before smallestUnit validation per spec
    let tzn_val = match get_prop(interp, options, "timeZoneName") {
        Completion::Normal(v) => v,
        c => return Err(c),
    };
    let tz_display = if is_undefined(&tzn_val) {
        "auto".to_string()
    } else {
        match interp.to_string_value(&tzn_val) {
            Ok(s) => {
                if !matches!(s.as_str(), "auto" | "never" | "critical") {
                    return Err(Completion::Throw(
                        interp.create_range_error(&format!("Invalid timeZoneName option: {s}")),
                    ));
                }
                s
            }
            Err(c) => return Err(Completion::Throw(c)),
        }
    };

    // Now validate smallestUnit
    let precision = if let Some(su_str) = su_coerced {
        match temporal_unit_singular(&su_str) {
            Some("minute") => Some(-1),
            Some("second") => Some(0),
            Some("millisecond") => Some(3),
            Some("microsecond") => Some(6),
            Some("nanosecond") => Some(9),
            Some(u) => {
                return Err(Completion::Throw(interp.create_range_error(&format!(
                    "{u} is not a valid value for smallestUnit"
                ))));
            }
            None => {
                return Err(Completion::Throw(
                    interp.create_range_error(&format!("Invalid unit: {su_str}")),
                ));
            }
        }
    } else {
        fsd_precision
    };

    Ok((
        precision,
        rounding_mode,
        offset_display,
        tz_display,
        cal_display,
    ))
}

