use super::super::super::*;
use crate::interpreter::helpers::{
    date_from_time, hour_from_time, min_from_time, month_from_time, ms_from_time, now_ms,
    sec_from_time, week_day, year_from_time,
};

fn extract_unicode_extension(locale: &str, key: &str) -> Option<String> {
    let u_idx = locale.find("-u-")?;
    let ext = &locale[u_idx + 3..];
    let parts: Vec<&str> = ext.split('-').collect();
    for i in 0..parts.len() {
        if parts[i] == key && i + 1 < parts.len() {
            let next = parts[i + 1];
            // A type value is 3-8 alphanum chars. If the next part is a singleton (1 char)
            // or another key (2 chars), this key has no value.
            if next.len() <= 2 {
                return None;
            }
            let mut value = next.to_string();
            let mut j = i + 2;
            while j < parts.len() && parts[j].len() > 2 {
                value.push('-');
                value.push_str(parts[j]);
                j += 1;
            }
            return Some(value);
        }
    }
    None
}

fn strip_unicode_extension_key(locale: &str, key: &str) -> String {
    let u_idx = match locale.find("-u-") {
        Some(idx) => idx,
        None => return locale.to_string(),
    };
    let before = &locale[..u_idx];
    let ext = &locale[u_idx + 3..];
    let parts: Vec<&str> = ext.split('-').collect();

    let mut result_parts: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < parts.len() {
        if parts[i].len() == 2 && parts[i] == key {
            // Skip this key and its value(s)
            i += 1;
            while i < parts.len() && parts[i].len() > 2 {
                i += 1;
            }
        } else {
            result_parts.push(parts[i]);
            i += 1;
        }
    }

    if result_parts.is_empty() {
        before.to_string()
    } else {
        format!("{}-u-{}", before, result_parts.join("-"))
    }
}

fn strip_unrecognized_unicode_keys(locale: &str) -> String {
    let u_idx = match locale.find("-u-") {
        Some(idx) => idx,
        None => return locale.to_string(),
    };
    let before = &locale[..u_idx];
    let ext = &locale[u_idx + 3..];
    let parts: Vec<&str> = ext.split('-').collect();

    let mut result_parts: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < parts.len() {
        if parts[i].len() == 2 {
            let key = parts[i];
            // Only keep recognized DTF extension keys
            if key == "ca" || key == "hc" || key == "nu" {
                result_parts.push(parts[i]);
                i += 1;
                // Include value parts
                while i < parts.len() && parts[i].len() > 2 {
                    result_parts.push(parts[i]);
                    i += 1;
                }
            } else {
                // Skip this key and its value(s)
                i += 1;
                while i < parts.len() && parts[i].len() > 2 {
                    i += 1;
                }
            }
        } else {
            result_parts.push(parts[i]);
            i += 1;
        }
    }

    if result_parts.is_empty() {
        before.to_string()
    } else {
        format!("{}-u-{}", before, result_parts.join("-"))
    }
}

fn is_valid_unicode_type(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    for part in s.split('-') {
        let len = part.len();
        if len < 3 || len > 8 {
            return false;
        }
        if !part.chars().all(|c| c.is_ascii_alphanumeric()) {
            return false;
        }
    }
    true
}

fn is_supported_calendar(cal: &str) -> bool {
    matches!(
        cal,
        "buddhist"
            | "chinese"
            | "coptic"
            | "dangi"
            | "ethioaa"
            | "ethiopic"
            | "gregory"
            | "hebrew"
            | "indian"
            | "islamic-civil"
            | "islamic-tbla"
            | "islamic-umalqura"
            | "iso8601"
            | "japanese"
            | "persian"
            | "roc"
    )
}

fn canonicalize_calendar(cal: &str) -> String {
    let lower = cal.to_ascii_lowercase();
    match lower.as_str() {
        "islamicc" | "islamic" | "islamic-rgsa" => "islamic-civil".to_string(),
        "ethiopic-amete-alem" => "ethioaa".to_string(),
        _ => lower,
    }
}

/// Parse an offset timezone string like "+03", "+03:00", "+0300", "-07:30"
/// Returns Some((hours, minutes)) if valid, None otherwise.
/// Hours: -23..=23, Minutes: 0..=59
fn parse_offset_timezone(tz: &str) -> Option<(i32, i32)> {
    if tz.is_empty() {
        return None;
    }
    let (sign, rest) = match tz.as_bytes()[0] {
        b'+' => (1i32, &tz[1..]),
        b'-' => (-1i32, &tz[1..]),
        _ => return None,
    };
    // Formats: HH, HHMM, HH:MM
    let (h, m) = if rest.len() == 2 {
        // +HH
        let h: i32 = rest.parse().ok()?;
        (h, 0)
    } else if rest.len() == 4 && !rest.contains(':') {
        // +HHMM
        let h: i32 = rest[..2].parse().ok()?;
        let m: i32 = rest[2..].parse().ok()?;
        (h, m)
    } else if rest.len() == 5 && rest.as_bytes()[2] == b':' {
        // +HH:MM
        let h: i32 = rest[..2].parse().ok()?;
        let m: i32 = rest[3..].parse().ok()?;
        (h, m)
    } else {
        return None;
    };
    if h > 23 || m > 59 {
        return None;
    }
    Some((sign * h, sign.abs() * m))
}

/// Normalize an offset timezone string to "+HH:MM" or "-HH:MM" format.
/// -00 and -00:00 normalize to +00:00 (no negative zero).
fn normalize_offset_timezone(tz: &str) -> Option<String> {
    let (h, m) = parse_offset_timezone(tz)?;
    let sign = if h < 0 { "-" } else { "+" };
    Some(format!("{}{:02}:{:02}", sign, h.abs(), m))
}

/// Get the UTC offset in milliseconds for a timezone at a specific UTC epoch ms.
/// Uses chrono_tz for DST-aware offset computation for IANA timezones.
fn tz_offset_ms(tz: &str, epoch_ms: f64) -> f64 {
    if let Some((h, m)) = parse_offset_timezone(tz) {
        let total_min = h * 60 + if h < 0 { -(m as i32) } else { m as i32 };
        return total_min as f64 * 60_000.0;
    }

    use chrono::{Offset, TimeZone, Utc};
    use chrono_tz::Tz;

    let canonical = canonicalize_timezone(tz);
    let tz_str = if canonical.eq_ignore_ascii_case(tz) && canonical != tz.to_string() {
        canonical
    } else {
        tz.to_string()
    };

    if let Ok(tz_parsed) = tz_str.parse::<Tz>() {
        let epoch_secs = (epoch_ms / 1000.0).floor() as i64;
        let nanos = ((epoch_ms % 1000.0) * 1_000_000.0).abs() as u32;
        if let Some(dt) = Utc.timestamp_opt(epoch_secs, nanos).single() {
            let offset = dt.with_timezone(&tz_parsed).offset().fix();
            return offset.local_minus_utc() as f64 * 1000.0;
        }
    }

    // Fallback to static lookup
    if let Some(info) = tz_lookup(tz) {
        let total_min = info.offset_hours * 60 + info.offset_minutes;
        return total_min as f64 * 60_000.0;
    }
    0.0
}

fn is_valid_timezone(tz: &str) -> bool {
    if tz.is_empty() {
        return false;
    }
    // Check offset timezone format (+HH, +HH:MM, +HHMM, etc.)
    if parse_offset_timezone(tz).is_some() {
        return true;
    }
    // Use canonicalize_timezone to check if it matches a known TZ
    let canonical = canonicalize_timezone(tz);
    if canonical.eq_ignore_ascii_case(tz) && canonical != tz.to_string() {
        return true;
    }
    if canonical == tz {
        let lower_canonical = canonicalize_timezone(&tz.to_ascii_lowercase());
        if lower_canonical != tz.to_ascii_lowercase() {
            return true;
        }
        let upper_canonical = canonicalize_timezone(&tz.to_ascii_uppercase());
        if upper_canonical != tz.to_ascii_uppercase() {
            return true;
        }
    }
    // Accept valid IANA-style patterns we might not have in our list
    if tz.chars().any(|c| !c.is_ascii()) {
        return false;
    }
    let valid_chars = tz.chars().all(|c| c.is_ascii_alphanumeric() || c == '/' || c == '_' || c == '-' || c == '+');
    if !valid_chars {
        return false;
    }
    if tz.contains('/') {
        let parts: Vec<&str> = tz.split('/').collect();
        if parts.len() >= 2 && parts.len() <= 4 {
            let region_upper = parts[0].to_uppercase();
            return matches!(
                region_upper.as_str(),
                "AFRICA" | "AMERICA" | "ANTARCTICA" | "ARCTIC" | "ASIA" | "ATLANTIC"
                | "AUSTRALIA" | "BRAZIL" | "CANADA" | "CHILE" | "ETC" | "EUROPE"
                | "INDIAN" | "MEXICO" | "PACIFIC" | "US"
            );
        }
    }
    false
}

fn canonicalize_timezone(tz: &str) -> String {
    static KNOWN_TZ: &[&str] = &[
        "Africa/Abidjan","Africa/Accra","Africa/Addis_Ababa","Africa/Algiers","Africa/Asmara",
        "Africa/Asmera","Africa/Bamako","Africa/Bangui","Africa/Banjul","Africa/Bissau",
        "Africa/Blantyre","Africa/Brazzaville","Africa/Bujumbura","Africa/Cairo",
        "Africa/Casablanca","Africa/Ceuta","Africa/Conakry","Africa/Dakar",
        "Africa/Dar_es_Salaam","Africa/Djibouti","Africa/Douala","Africa/El_Aaiun",
        "Africa/Freetown","Africa/Gaborone","Africa/Harare","Africa/Johannesburg",
        "Africa/Juba","Africa/Kampala","Africa/Khartoum","Africa/Kigali","Africa/Kinshasa",
        "Africa/Lagos","Africa/Libreville","Africa/Lome","Africa/Luanda",
        "Africa/Lubumbashi","Africa/Lusaka","Africa/Malabo","Africa/Maputo",
        "Africa/Maseru","Africa/Mbabane","Africa/Mogadishu","Africa/Monrovia",
        "Africa/Nairobi","Africa/Ndjamena","Africa/Niamey","Africa/Nouakchott",
        "Africa/Ouagadougou","Africa/Porto-Novo","Africa/Sao_Tome","Africa/Timbuktu",
        "Africa/Tripoli","Africa/Tunis","Africa/Windhoek",
        "America/Adak","America/Anchorage","America/Anguilla","America/Antigua",
        "America/Araguaina","America/Argentina/Buenos_Aires","America/Argentina/Catamarca",
        "America/Argentina/ComodRivadavia","America/Argentina/Cordoba",
        "America/Argentina/Jujuy","America/Argentina/La_Rioja",
        "America/Argentina/Mendoza","America/Argentina/Rio_Gallegos",
        "America/Argentina/Salta","America/Argentina/San_Juan",
        "America/Argentina/San_Luis","America/Argentina/Tucuman",
        "America/Argentina/Ushuaia","America/Aruba","America/Asuncion",
        "America/Atikokan","America/Atka","America/Bahia","America/Bahia_Banderas",
        "America/Barbados","America/Belem","America/Belize","America/Blanc-Sablon",
        "America/Boa_Vista","America/Bogota","America/Boise","America/Buenos_Aires",
        "America/Cambridge_Bay","America/Campo_Grande","America/Cancun",
        "America/Caracas","America/Catamarca","America/Cayenne","America/Cayman",
        "America/Chicago","America/Chihuahua","America/Ciudad_Juarez",
        "America/Coral_Harbour","America/Cordoba","America/Costa_Rica",
        "America/Creston","America/Cuiaba","America/Curacao","America/Danmarkshavn",
        "America/Dawson","America/Dawson_Creek","America/Denver","America/Detroit",
        "America/Dominica","America/Edmonton","America/Eirunepe",
        "America/El_Salvador","America/Ensenada","America/Fort_Nelson",
        "America/Fort_Wayne","America/Fortaleza","America/Glace_Bay",
        "America/Godthab","America/Goose_Bay","America/Grand_Turk",
        "America/Grenada","America/Guadeloupe","America/Guatemala",
        "America/Guayaquil","America/Guyana","America/Halifax","America/Havana",
        "America/Hermosillo","America/Indiana/Indianapolis","America/Indiana/Knox",
        "America/Indiana/Marengo","America/Indiana/Petersburg",
        "America/Indiana/Tell_City","America/Indiana/Vevay",
        "America/Indiana/Vincennes","America/Indiana/Winamac",
        "America/Indianapolis","America/Inuvik","America/Iqaluit",
        "America/Jamaica","America/Jujuy","America/Juneau",
        "America/Kentucky/Louisville","America/Kentucky/Monticello",
        "America/Knox_IN","America/Kralendijk","America/La_Paz","America/Lima",
        "America/Los_Angeles","America/Louisville","America/Lower_Princes",
        "America/Maceio","America/Managua","America/Manaus","America/Marigot",
        "America/Martinique","America/Matamoros","America/Mazatlan",
        "America/Mendoza","America/Menominee","America/Merida",
        "America/Metlakatla","America/Mexico_City","America/Miquelon",
        "America/Moncton","America/Monterrey","America/Montevideo",
        "America/Montreal","America/Montserrat","America/Nassau",
        "America/New_York","America/Nipigon","America/Nome","America/Noronha",
        "America/North_Dakota/Beulah","America/North_Dakota/Center",
        "America/North_Dakota/New_Salem","America/Nuuk","America/Ojinaga",
        "America/Panama","America/Pangnirtung","America/Paramaribo",
        "America/Phoenix","America/Port-au-Prince","America/Port_of_Spain",
        "America/Porto_Acre","America/Porto_Velho","America/Puerto_Rico",
        "America/Punta_Arenas","America/Rainy_River","America/Rankin_Inlet",
        "America/Recife","America/Regina","America/Resolute",
        "America/Rio_Branco","America/Rosario","America/Santa_Isabel",
        "America/Santarem","America/Santiago","America/Santo_Domingo",
        "America/Sao_Paulo","America/Scoresbysund","America/Shiprock",
        "America/Sitka","America/St_Barthelemy","America/St_Johns",
        "America/St_Kitts","America/St_Lucia","America/St_Thomas",
        "America/St_Vincent","America/Swift_Current","America/Tegucigalpa",
        "America/Thule","America/Thunder_Bay","America/Tijuana",
        "America/Toronto","America/Tortola","America/Vancouver",
        "America/Virgin","America/Whitehorse","America/Winnipeg",
        "America/Yakutat","America/Yellowknife",
        "Antarctica/Casey","Antarctica/Davis","Antarctica/DumontDUrville",
        "Antarctica/Macquarie","Antarctica/Mawson","Antarctica/McMurdo",
        "Antarctica/Palmer","Antarctica/Rothera","Antarctica/South_Pole",
        "Antarctica/Syowa","Antarctica/Troll","Antarctica/Vostok",
        "Arctic/Longyearbyen",
        "Asia/Aden","Asia/Almaty","Asia/Amman","Asia/Anadyr","Asia/Aqtau",
        "Asia/Aqtobe","Asia/Ashgabat","Asia/Ashkhabad","Asia/Atyrau",
        "Asia/Baghdad","Asia/Bahrain","Asia/Baku","Asia/Bangkok","Asia/Barnaul",
        "Asia/Beirut","Asia/Bishkek","Asia/Brunei","Asia/Calcutta","Asia/Chita",
        "Asia/Choibalsan","Asia/Chongqing","Asia/Chungking","Asia/Colombo",
        "Asia/Dacca","Asia/Damascus","Asia/Dhaka","Asia/Dili","Asia/Dubai",
        "Asia/Dushanbe","Asia/Famagusta","Asia/Gaza","Asia/Harbin",
        "Asia/Hebron","Asia/Ho_Chi_Minh","Asia/Hong_Kong","Asia/Hovd",
        "Asia/Irkutsk","Asia/Istanbul","Asia/Jakarta","Asia/Jayapura",
        "Asia/Jerusalem","Asia/Kabul","Asia/Kamchatka","Asia/Karachi",
        "Asia/Kashgar","Asia/Kathmandu","Asia/Katmandu","Asia/Khandyga",
        "Asia/Kolkata","Asia/Krasnoyarsk","Asia/Kuala_Lumpur","Asia/Kuching",
        "Asia/Kuwait","Asia/Macao","Asia/Macau","Asia/Magadan","Asia/Makassar",
        "Asia/Manila","Asia/Muscat","Asia/Nicosia","Asia/Novokuznetsk",
        "Asia/Novosibirsk","Asia/Omsk","Asia/Oral","Asia/Phnom_Penh",
        "Asia/Pontianak","Asia/Pyongyang","Asia/Qatar","Asia/Qostanay",
        "Asia/Qyzylorda","Asia/Rangoon","Asia/Riyadh","Asia/Saigon",
        "Asia/Sakhalin","Asia/Samarkand","Asia/Seoul","Asia/Shanghai",
        "Asia/Singapore","Asia/Srednekolymsk","Asia/Taipei","Asia/Tashkent",
        "Asia/Tbilisi","Asia/Tehran","Asia/Tel_Aviv","Asia/Thimbu",
        "Asia/Thimphu","Asia/Tokyo","Asia/Tomsk","Asia/Ujung_Pandang",
        "Asia/Ulaanbaatar","Asia/Ulan_Bator","Asia/Urumqi","Asia/Ust-Nera",
        "Asia/Vientiane","Asia/Vladivostok","Asia/Yakutsk","Asia/Yangon",
        "Asia/Yekaterinburg","Asia/Yerevan",
        "Atlantic/Azores","Atlantic/Bermuda","Atlantic/Canary",
        "Atlantic/Cape_Verde","Atlantic/Faeroe","Atlantic/Faroe",
        "Atlantic/Jan_Mayen","Atlantic/Madeira","Atlantic/Reykjavik",
        "Atlantic/South_Georgia","Atlantic/St_Helena","Atlantic/Stanley",
        "Australia/ACT","Australia/Adelaide","Australia/Brisbane",
        "Australia/Broken_Hill","Australia/Canberra","Australia/Currie",
        "Australia/Darwin","Australia/Eucla","Australia/Hobart","Australia/LHI",
        "Australia/Lindeman","Australia/Lord_Howe","Australia/Melbourne",
        "Australia/NSW","Australia/North","Australia/Perth",
        "Australia/Queensland","Australia/South","Australia/Sydney",
        "Australia/Tasmania","Australia/Victoria","Australia/West",
        "Australia/Yancowinna",
        "Brazil/Acre","Brazil/DeNoronha","Brazil/East","Brazil/West",
        "CET","CST6CDT",
        "Canada/Atlantic","Canada/Central","Canada/Eastern","Canada/Mountain",
        "Canada/Newfoundland","Canada/Pacific","Canada/Saskatchewan","Canada/Yukon",
        "Chile/Continental","Chile/EasterIsland","Cuba",
        "EET","EST","EST5EDT",
        "Egypt","Eire",
        "Etc/GMT","Etc/GMT+0","Etc/GMT+1","Etc/GMT+10","Etc/GMT+11","Etc/GMT+12",
        "Etc/GMT+2","Etc/GMT+3","Etc/GMT+4","Etc/GMT+5","Etc/GMT+6","Etc/GMT+7",
        "Etc/GMT+8","Etc/GMT+9","Etc/GMT-0","Etc/GMT-1","Etc/GMT-10","Etc/GMT-11",
        "Etc/GMT-12","Etc/GMT-13","Etc/GMT-14","Etc/GMT-2","Etc/GMT-3","Etc/GMT-4",
        "Etc/GMT-5","Etc/GMT-6","Etc/GMT-7","Etc/GMT-8","Etc/GMT-9","Etc/GMT0",
        "Etc/Greenwich","Etc/UCT","Etc/UTC","Etc/Universal","Etc/Zulu",
        "Europe/Amsterdam","Europe/Andorra","Europe/Astrakhan","Europe/Athens",
        "Europe/Belfast","Europe/Belgrade","Europe/Berlin","Europe/Bratislava",
        "Europe/Brussels","Europe/Bucharest","Europe/Budapest","Europe/Busingen",
        "Europe/Chisinau","Europe/Copenhagen","Europe/Dublin","Europe/Gibraltar",
        "Europe/Guernsey","Europe/Helsinki","Europe/Isle_of_Man","Europe/Istanbul",
        "Europe/Jersey","Europe/Kaliningrad","Europe/Kiev","Europe/Kirov",
        "Europe/Kyiv","Europe/Lisbon","Europe/Ljubljana","Europe/London",
        "Europe/Luxembourg","Europe/Madrid","Europe/Malta","Europe/Mariehamn",
        "Europe/Minsk","Europe/Monaco","Europe/Moscow","Europe/Nicosia",
        "Europe/Oslo","Europe/Paris","Europe/Podgorica","Europe/Prague",
        "Europe/Riga","Europe/Rome","Europe/Samara","Europe/San_Marino",
        "Europe/Sarajevo","Europe/Saratov","Europe/Simferopol","Europe/Skopje",
        "Europe/Sofia","Europe/Stockholm","Europe/Tallinn","Europe/Tirane",
        "Europe/Tiraspol","Europe/Ulyanovsk","Europe/Uzhgorod","Europe/Vaduz",
        "Europe/Vatican","Europe/Vienna","Europe/Vilnius","Europe/Volgograd",
        "Europe/Warsaw","Europe/Zagreb","Europe/Zaporozhye","Europe/Zurich",
        "GB","GB-Eire","GMT","GMT+0","GMT-0","GMT0","Greenwich",
        "HST","Hongkong",
        "Iceland",
        "Indian/Antananarivo","Indian/Chagos","Indian/Christmas","Indian/Cocos",
        "Indian/Comoro","Indian/Kerguelen","Indian/Mahe","Indian/Maldives",
        "Indian/Mauritius","Indian/Mayotte","Indian/Reunion",
        "Iran","Israel","Jamaica","Japan","Kwajalein","Libya",
        "MET","MST","MST7MDT",
        "Mexico/BajaNorte","Mexico/BajaSur","Mexico/General",
        "NZ","NZ-CHAT","Navajo",
        "PRC","PST8PDT",
        "Pacific/Apia","Pacific/Auckland","Pacific/Bougainville","Pacific/Chatham",
        "Pacific/Chuuk","Pacific/Easter","Pacific/Efate","Pacific/Enderbury",
        "Pacific/Fakaofo","Pacific/Fiji","Pacific/Funafuti","Pacific/Galapagos",
        "Pacific/Gambier","Pacific/Guadalcanal","Pacific/Guam","Pacific/Honolulu",
        "Pacific/Johnston","Pacific/Kanton","Pacific/Kiritimati","Pacific/Kosrae",
        "Pacific/Kwajalein","Pacific/Majuro","Pacific/Marquesas","Pacific/Midway",
        "Pacific/Nauru","Pacific/Niue","Pacific/Norfolk","Pacific/Noumea",
        "Pacific/Pago_Pago","Pacific/Palau","Pacific/Pitcairn",
        "Pacific/Pohnpei","Pacific/Ponape","Pacific/Port_Moresby",
        "Pacific/Rarotonga","Pacific/Saipan","Pacific/Samoa","Pacific/Tahiti",
        "Pacific/Tarawa","Pacific/Tongatapu","Pacific/Truk","Pacific/Wake",
        "Pacific/Wallis","Pacific/Yap",
        "Poland","Portugal",
        "ROC","ROK",
        "Singapore","Turkey",
        "UCT","US/Alaska","US/Aleutian","US/Arizona","US/Central",
        "US/East-Indiana","US/Eastern","US/Hawaii","US/Indiana-Starke",
        "US/Michigan","US/Mountain","US/Pacific","US/Samoa",
        "UTC","Universal","W-SU","WET","Zulu",
    ];

    for &known in KNOWN_TZ {
        if tz.eq_ignore_ascii_case(known) {
            return known.to_string();
        }
    }

    tz.to_string()
}

fn is_supported_numbering_system(ns: &str) -> bool {
    matches!(
        ns,
        "adlm"
            | "ahom"
            | "arab"
            | "arabext"
            | "bali"
            | "beng"
            | "bhks"
            | "brah"
            | "cakm"
            | "cham"
            | "deva"
            | "diak"
            | "fullwide"
            | "gong"
            | "gonm"
            | "gujr"
            | "guru"
            | "hanidec"
            | "hmng"
            | "hmnp"
            | "java"
            | "kali"
            | "kawi"
            | "khmr"
            | "knda"
            | "lana"
            | "lanatham"
            | "laoo"
            | "latn"
            | "lepc"
            | "limb"
            | "mathbold"
            | "mathdbl"
            | "mathmono"
            | "mathsanb"
            | "mathsans"
            | "mlym"
            | "modi"
            | "mong"
            | "mroo"
            | "mtei"
            | "mymr"
            | "mymrshan"
            | "mymrtlng"
            | "nagm"
            | "newa"
            | "nkoo"
            | "olck"
            | "orya"
            | "osma"
            | "rohg"
            | "saur"
            | "segment"
            | "shrd"
            | "sind"
            | "sinh"
            | "sora"
            | "sund"
            | "takr"
            | "talu"
            | "tamldec"
            | "telu"
            | "thai"
            | "tibt"
            | "tirh"
            | "tnsa"
            | "vaii"
            | "wara"
            | "wcho"
    )
}

struct DateComponents {
    year: i32,
    month: u32, // 1-12
    day: u32,   // 1-31
    weekday: u32, // 0=Sunday, 1=Monday, ... 6=Saturday
    hour: u32,
    minute: u32,
    second: u32,
    millisecond: u32,
}

fn timestamp_to_components(ms: f64) -> DateComponents {
    let year = year_from_time(ms) as i32;
    let month = month_from_time(ms) as u32 + 1; // 0-based to 1-based
    let day = date_from_time(ms) as u32;
    let weekday = week_day(ms) as u32;
    let hour = hour_from_time(ms) as u32;
    let minute = min_from_time(ms) as u32;
    let second = sec_from_time(ms) as u32;
    let millisecond = ms_from_time(ms) as u32;
    DateComponents {
        year,
        month,
        day,
        weekday,
        hour,
        minute,
        second,
        millisecond,
    }
}

fn month_name_long(m: u32) -> &'static str {
    match m {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "January",
    }
}

fn month_name_short(m: u32) -> &'static str {
    match m {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "Jan",
    }
}

fn month_name_narrow(m: u32) -> &'static str {
    match m {
        1 => "J",
        2 => "F",
        3 => "M",
        4 => "A",
        5 => "M",
        6 => "J",
        7 => "J",
        8 => "A",
        9 => "S",
        10 => "O",
        11 => "N",
        12 => "D",
        _ => "J",
    }
}

fn weekday_name_long(d: u32) -> &'static str {
    match d {
        0 => "Sunday",
        1 => "Monday",
        2 => "Tuesday",
        3 => "Wednesday",
        4 => "Thursday",
        5 => "Friday",
        6 => "Saturday",
        _ => "Sunday",
    }
}

fn weekday_name_short(d: u32) -> &'static str {
    match d {
        0 => "Sun",
        1 => "Mon",
        2 => "Tue",
        3 => "Wed",
        4 => "Thu",
        5 => "Fri",
        6 => "Sat",
        _ => "Sun",
    }
}

fn weekday_name_narrow(d: u32) -> &'static str {
    match d {
        0 => "S",
        1 => "M",
        2 => "T",
        3 => "W",
        4 => "T",
        5 => "F",
        6 => "S",
        _ => "S",
    }
}

fn era_long(year: i32) -> &'static str {
    if year > 0 {
        "Anno Domini"
    } else {
        "Before Christ"
    }
}

fn era_short(year: i32) -> &'static str {
    if year > 0 {
        "AD"
    } else {
        "BC"
    }
}

fn era_narrow(year: i32) -> &'static str {
    if year > 0 {
        "A"
    } else {
        "B"
    }
}

fn day_period_text(hour: u32, style: &str) -> &'static str {
    match style {
        "narrow" => match hour {
            6..=11 => "in the morning",
            12 => "n",
            13..=17 => "in the afternoon",
            18..=20 => "in the evening",
            _ => "at night",
        },
        _ => match hour {
            6..=11 => "in the morning",
            12 => "noon",
            13..=17 => "in the afternoon",
            18..=20 => "in the evening",
            _ => "at night",
        },
    }
}

#[derive(Clone)]
struct DtfOptions {
    locale: String,
    calendar: String,
    numbering_system: String,
    time_zone: String,
    hour_cycle: Option<String>,
    hour12: Option<bool>,
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
    date_style: Option<String>,
    time_style: Option<String>,
    has_explicit_components: bool,
    // Set when formatting a Temporal object to indicate reduced output
    temporal_type: Option<TemporalType>,
}

fn locale_default_hour12(locale: &str) -> &'static str {
    let lang = locale.split('-').next().unwrap_or("en");
    match lang {
        "ja" => "h11",
        _ => "h12",
    }
}

fn locale_default_hour_cycle(locale: &str) -> &'static str {
    let lang = locale.split('-').next().unwrap_or("en");
    match lang {
        "en" | "ar" | "ko" | "hi" | "bn" => "h12",
        "ja" => "h23",
        "zh" | "de" | "fr" | "it" | "es" | "pt" | "ru" | "nl" | "sv" | "da" | "nb"
        | "fi" | "pl" | "cs" | "hu" | "ro" | "tr" | "uk" | "hr" | "sk" | "sl" | "bg"
        | "el" | "he" | "th" | "vi" | "id" | "ms" => "h23",
        _ => "h12",
    }
}

fn resolve_hour_cycle(opts: &DtfOptions) -> &str {
    if let Some(ref hc) = opts.hour_cycle {
        return hc.as_str();
    }
    if let Some(h12) = opts.hour12 {
        if h12 {
            return locale_default_hour12(&opts.locale);
        } else {
            return "h23";
        }
    }
    locale_default_hour_cycle(&opts.locale)
}

fn has_time_component(opts: &DtfOptions) -> bool {
    opts.hour.is_some()
        || opts.minute.is_some()
        || opts.second.is_some()
        || opts.day_period.is_some()
        || opts.fractional_second_digits.is_some()
}

fn format_hour(hour24: u32, hc: &str) -> (String, &'static str) {
    match hc {
        "h12" => {
            let period = if hour24 < 12 { "AM" } else { "PM" };
            let h = if hour24 == 0 {
                12
            } else if hour24 > 12 {
                hour24 - 12
            } else {
                hour24
            };
            (h.to_string(), period)
        }
        "h11" => {
            let period = if hour24 < 12 { "AM" } else { "PM" };
            let h = hour24 % 12;
            (h.to_string(), period)
        }
        "h23" => (hour24.to_string(), ""),
        "h24" => {
            let h = if hour24 == 0 { 24 } else { hour24 };
            (h.to_string(), "")
        }
        _ => {
            let period = if hour24 < 12 { "AM" } else { "PM" };
            let h = if hour24 == 0 {
                12
            } else if hour24 > 12 {
                hour24 - 12
            } else {
                hour24
            };
            (h.to_string(), period)
        }
    }
}

fn format_2digit(n: u32) -> String {
    if n < 10 {
        format!("0{}", n)
    } else {
        format!("{}", n % 100)
    }
}

fn format_date_style(c: &DateComponents, style: &str, tz: &str) -> String {
    match style {
        "full" => format!(
            "{}, {} {}, {}",
            weekday_name_long(c.weekday),
            month_name_long(c.month),
            c.day,
            c.year
        ),
        "long" => format!("{} {}, {}", month_name_long(c.month), c.day, c.year),
        "medium" => format!("{} {}, {}", month_name_short(c.month), c.day, c.year),
        "short" => format!("{}/{}/{}", c.month, c.day, c.year % 100),
        _ => format!("{}/{}/{}", c.month, c.day, c.year),
    }
}

fn format_reduced_date_style(c: &DateComponents, style: &str, has_year: bool, has_month: bool, has_day: bool) -> String {
    if has_year && has_month && !has_day {
        // PlainYearMonth: year + month, no day
        match style {
            "full" | "long" => format!("{} {}", month_name_long(c.month), c.year),
            "medium" => format!("{} {}", month_name_short(c.month), c.year),
            "short" => format!("{}/{}", c.month, c.year % 100),
            _ => format!("{}/{}", c.month, c.year),
        }
    } else if !has_year && has_month && has_day {
        // PlainMonthDay: month + day, no year
        match style {
            "full" | "long" => format!("{} {}", month_name_long(c.month), c.day),
            "medium" => format!("{} {}", month_name_short(c.month), c.day),
            "short" => format!("{}/{}", c.month, c.day),
            _ => format!("{}/{}", c.month, c.day),
        }
    } else {
        format_date_style(c, style, "")
    }
}

fn format_time_style(c: &DateComponents, style: &str, hc: &str, tz: &str, epoch_ms: f64) -> String {
    let (hour_str, period) = format_hour(c.hour, hc);
    let uses_period = hc == "h12" || hc == "h11";

    match style {
        "full" => {
            let tz_name = format_tz_name(tz, "long", epoch_ms);
            if uses_period {
                format!(
                    "{}:{:02}:{:02} {} {}",
                    hour_str, c.minute, c.second, period, tz_name
                )
            } else {
                format!("{}:{:02}:{:02} {}", hour_str, c.minute, c.second, tz_name)
            }
        }
        "long" => {
            let short_tz = format_tz_name(tz, "short", epoch_ms);
            if uses_period {
                format!(
                    "{}:{:02}:{:02} {} {}",
                    hour_str, c.minute, c.second, period, short_tz
                )
            } else {
                format!(
                    "{}:{:02}:{:02} {}",
                    hour_str, c.minute, c.second, short_tz
                )
            }
        }
        "medium" => {
            if uses_period {
                format!(
                    "{}:{:02}:{:02} {}",
                    hour_str, c.minute, c.second, period
                )
            } else {
                format!("{}:{:02}:{:02}", hour_str, c.minute, c.second)
            }
        }
        "short" => {
            if uses_period {
                format!("{}:{:02} {}", hour_str, c.minute, period)
            } else {
                format!("{}:{:02}", hour_str, c.minute)
            }
        }
        _ => {
            if uses_period {
                format!(
                    "{}:{:02}:{:02} {}",
                    hour_str, c.minute, c.second, period
                )
            } else {
                format!("{}:{:02}:{:02}", hour_str, c.minute, c.second)
            }
        }
    }
}

/// Transliterate ASCII digits 0-9 to the specified numbering system.
fn transliterate_digits(s: &str, ns: &str) -> String {
    if ns == "latn" {
        return s.to_string();
    }
    let zero_char: char = match ns {
        "arab" => '\u{0660}',      // ٠
        "arabext" => '\u{06F0}',   // ۰
        "beng" => '\u{09E6}',     // ০
        "deva" => '\u{0966}',     // ०
        "fullwide" => '\u{FF10}', // ０
        "gujr" => '\u{0AE6}',    // ૦
        "guru" => '\u{0A66}',    // ੦
        "khmr" => '\u{17E0}',    // ០
        "knda" => '\u{0CE6}',    // ೦
        "laoo" => '\u{0ED0}',    // ໐
        "mlym" => '\u{0D66}',    // ൦
        "mong" => '\u{1810}',    // ᠐
        "mymr" => '\u{1040}',    // ၀
        "orya" => '\u{0B66}',    // ୦
        "tamldec" => '\u{0BE6}', // ௦
        "telu" => '\u{0C66}',   // ౦
        "thai" => '\u{0E50}',   // ๐
        "tibt" => '\u{0F20}',   // ༠
        "hanidec" => {
            let han_digits = ['〇', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
            let mut result = String::with_capacity(s.len() * 3);
            for ch in s.chars() {
                if ch.is_ascii_digit() {
                    result.push(han_digits[(ch as u8 - b'0') as usize]);
                } else {
                    result.push(ch);
                }
            }
            return result;
        }
        _ => return s.to_string(),
    };
    // Also determine the decimal separator for this numbering system
    let decimal_sep = match ns {
        "arab" | "arabext" => '\u{066B}', // Arabic decimal separator ٫
        _ => '.',
    };
    let zero_val = zero_char as u32;
    let mut result = String::with_capacity(s.len() * 4);
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            let digit = (ch as u8 - b'0') as u32;
            if let Some(c) = char::from_u32(zero_val + digit) {
                result.push(c);
            } else {
                result.push(ch);
            }
        } else if ch == '.' && decimal_sep != '.' {
            result.push(decimal_sep);
        } else {
            result.push(ch);
        }
    }
    result
}

fn format_with_options(ms: f64, opts: &DtfOptions) -> String {
    let raw = format_with_options_raw(ms, opts);
    transliterate_digits(&raw, &opts.numbering_system)
}

fn format_with_options_raw(ms: f64, opts: &DtfOptions) -> String {
    let adjusted_ms = ms + tz_offset_ms(&opts.time_zone, ms);
    let c = timestamp_to_components(adjusted_ms);
    let hc = resolve_hour_cycle(opts);

    // dateStyle/timeStyle shorthand
    if opts.date_style.is_some() || opts.time_style.is_some() {
        // Check if Temporal type requires reduced date formatting
        let need_reduced_date = opts.date_style.is_some()
            && matches!(opts.temporal_type, Some(TemporalType::PlainYearMonth) | Some(TemporalType::PlainMonthDay));

        let date_part = if need_reduced_date {
            None
        } else {
            opts.date_style
                .as_ref()
                .map(|ds| format_date_style(&c, ds, &opts.time_zone))
        };

        let effective_time_style = opts.time_style.as_ref().map(|ts| {
            let is_plain_temporal = matches!(
                opts.temporal_type,
                Some(TemporalType::PlainTime) | Some(TemporalType::PlainDateTime)
                    | Some(TemporalType::PlainDate) | Some(TemporalType::PlainYearMonth)
                    | Some(TemporalType::PlainMonthDay)
            );
            if is_plain_temporal && opts.time_zone_name.is_none() && (ts == "long" || ts == "full") {
                "medium".to_string()
            } else {
                ts.clone()
            }
        });
        let time_part = effective_time_style
            .as_ref()
            .map(|ts| format_time_style(&c, ts, hc, &opts.time_zone, ms));

        if need_reduced_date {
            let ds = opts.date_style.as_deref().unwrap_or("short");
            let is_ym = matches!(opts.temporal_type, Some(TemporalType::PlainYearMonth));
            let reduced = format_reduced_date_style(&c, ds, is_ym || opts.year.is_some(), true, !is_ym);
            return match time_part {
                Some(t) => format!("{}, {}", reduced, t),
                None => reduced,
            };
        }

        return match (date_part, time_part) {
            (Some(d), Some(t)) => format!("{}, {}", d, t),
            (Some(d), None) => d,
            (None, Some(t)) => t,
            (None, None) => String::new(),
        };
    }

    // Build formatted string from individual components
    let mut date_parts: Vec<String> = Vec::new();
    let mut time_parts: Vec<String> = Vec::new();

    // Weekday
    if let Some(ref wd) = opts.weekday {
        let s = match wd.as_str() {
            "long" => weekday_name_long(c.weekday).to_string(),
            "short" => weekday_name_short(c.weekday).to_string(),
            "narrow" => weekday_name_narrow(c.weekday).to_string(),
            _ => weekday_name_long(c.weekday).to_string(),
        };
        date_parts.push(s);
    }

    // Year - for proleptic Gregorian, display absolute year when era is present
    let display_year = if opts.era.is_some() && c.year <= 0 {
        1 - c.year // year 0 -> 1 BC, year -1 -> 2 BC
    } else {
        c.year
    };
    let year_str = opts.year.as_ref().map(|y| match y.as_str() {
        "2-digit" => format_2digit((display_year.unsigned_abs() % 100) as u32),
        _ => display_year.to_string(),
    });

    // Month
    let month_str = opts.month.as_ref().map(|m| match m.as_str() {
        "2-digit" => format_2digit(c.month),
        "long" => month_name_long(c.month).to_string(),
        "short" => month_name_short(c.month).to_string(),
        "narrow" => month_name_narrow(c.month).to_string(),
        _ => c.month.to_string(), // "numeric"
    });

    // Day
    let day_str = opts.day.as_ref().map(|d| match d.as_str() {
        "2-digit" => format_2digit(c.day),
        _ => c.day.to_string(), // "numeric"
    });

    // Build date portion based on available components
    let has_date =
        opts.year.is_some() || opts.month.is_some() || opts.day.is_some();

    if has_date {
        let month_is_text = opts.month.as_ref().is_some_and(|m| {
            matches!(m.as_str(), "long" | "short" | "narrow")
        });

        if month_is_text {
            // Use text-style formatting: "January 15, 2024" or "Jan 15, 2024"
            if let Some(ref m) = month_str {
                date_parts.push(m.clone());
            }
            if let Some(ref d) = day_str {
                if year_str.is_some() {
                    date_parts.push(format!("{},", d));
                } else {
                    date_parts.push(d.clone());
                }
            }
            if let Some(ref y) = year_str {
                date_parts.push(y.clone());
            }
            // Era after year
            if let Some(ref e) = opts.era {
                let s = match e.as_str() {
                    "long" => era_long(c.year).to_string(),
                    "short" => era_short(c.year).to_string(),
                    "narrow" => era_narrow(c.year).to_string(),
                    _ => era_short(c.year).to_string(),
                };
                date_parts.push(s);
            }
        } else {
            // Use numeric-style formatting: "1/15/2024"
            let mut numeric_parts: Vec<String> = Vec::new();
            if let Some(ref m) = month_str {
                numeric_parts.push(m.clone());
            }
            if let Some(ref d) = day_str {
                numeric_parts.push(d.clone());
            }
            if let Some(ref y) = year_str {
                numeric_parts.push(y.clone());
            }
            if !numeric_parts.is_empty() {
                date_parts.push(numeric_parts.join("/"));
            }
            // Era after year for numeric
            if let Some(ref e) = opts.era {
                let s = match e.as_str() {
                    "long" => era_long(c.year).to_string(),
                    "short" => era_short(c.year).to_string(),
                    "narrow" => era_narrow(c.year).to_string(),
                    _ => era_short(c.year).to_string(),
                };
                date_parts.push(s);
            }
        }
    }

    // Time portion
    let has_time = has_time_component(opts);
    if has_time {
        let (hour_str, period) = format_hour(c.hour, hc);
        let uses_period = hc == "h12" || hc == "h11";

        // dayPeriod alone (no hour)
        if opts.day_period.is_some() && opts.hour.is_none() {
            let dp_text = day_period_text(c.hour, opts.day_period.as_ref().unwrap());
            let date_str = if !date_parts.is_empty() {
                date_parts.join(" ")
            } else {
                String::new()
            };
            if !date_str.is_empty() {
                return format!("{}, {}", date_str, dp_text);
            }
            return dp_text.to_string();
        }

        if let Some(ref h) = opts.hour {
            let h_str = match h.as_str() {
                "2-digit" => format_2digit(
                    match hc {
                        "h12" => {
                            let v = if c.hour == 0 { 12 } else if c.hour > 12 { c.hour - 12 } else { c.hour };
                            v
                        }
                        "h11" => c.hour % 12,
                        "h23" => c.hour,
                        "h24" => if c.hour == 0 { 24 } else { c.hour },
                        _ => c.hour,
                    }
                ),
                _ => {
                    // h23/h24: always use 2-digit padding per ICU convention
                    if hc == "h23" || hc == "h24" {
                        format_2digit(match hc {
                            "h24" => if c.hour == 0 { 24 } else { c.hour },
                            _ => c.hour,
                        })
                    } else {
                        hour_str.clone()
                    }
                }
            };
            time_parts.push(h_str);
        }

        if let Some(ref m) = opts.minute {
            let m_str = match m.as_str() {
                "2-digit" => format_2digit(c.minute),
                _ => {
                    if opts.hour.is_some() || opts.second.is_some() {
                        format_2digit(c.minute)
                    } else {
                        c.minute.to_string()
                    }
                }
            };
            time_parts.push(m_str);
        }

        if let Some(ref s) = opts.second {
            let s_str = match s.as_str() {
                "2-digit" => format_2digit(c.second),
                _ => {
                    if opts.hour.is_some() || opts.minute.is_some() {
                        format_2digit(c.second)
                    } else {
                        c.second.to_string()
                    }
                }
            };
            time_parts.push(s_str);
        }

        // Fractional seconds
        if let Some(digits) = opts.fractional_second_digits {
            let frac = match digits {
                1 => format!(".{}", c.millisecond / 100),
                2 => format!(".{:02}", c.millisecond / 10),
                3 => format!(".{:03}", c.millisecond),
                _ => String::new(),
            };
            if !frac.is_empty() {
                if let Some(last) = time_parts.last_mut() {
                    last.push_str(&frac);
                }
            }
        }

        // dayPeriod with hour: use day period text instead of AM/PM
        if opts.day_period.is_some() && opts.hour.is_some() {
            let dp_text = day_period_text(c.hour, opts.day_period.as_ref().unwrap());
            let time_str = time_parts.join(":");
            let mut result_parts = date_parts.clone();
            let with_dp = format!("{} {}", time_str, dp_text);
            if !result_parts.is_empty() {
                let date_str = result_parts.join(" ");
                let mut final_str = format!("{}, {}", date_str, with_dp);
                if let Some(ref tzn) = opts.time_zone_name {
                    final_str.push(' ');
                    final_str.push_str(&format_tz_name(&opts.time_zone, tzn, ms));
                }
                return final_str;
            }
            let mut final_str = with_dp;
            if let Some(ref tzn) = opts.time_zone_name {
                final_str.push(' ');
                final_str.push_str(&format_tz_name(&opts.time_zone, tzn, ms));
            }
            return final_str;
        }

        // Append AM/PM for 12-hour formats
        if uses_period && opts.hour.is_some() {
            let time_str = time_parts.join(":");
            let mut result_parts = date_parts.clone();
            if !time_str.is_empty() {
                let with_period = format!("{} {}", time_str, period);
                if !result_parts.is_empty() {
                    let date_str = result_parts.join(" ");
                    let mut final_str = format!("{}, {}", date_str, with_period);

                    // TimeZone name
                    if let Some(ref tzn) = opts.time_zone_name {
                        final_str.push(' ');
                        final_str.push_str(&format_tz_name(&opts.time_zone, tzn, ms));
                    }
                    return final_str;
                }
                let mut final_str = with_period;
                if let Some(ref tzn) = opts.time_zone_name {
                    final_str.push(' ');
                    final_str.push_str(&format_tz_name(&opts.time_zone, tzn, ms));
                }
                return final_str;
            }
        }
    }

    // Combine date and time
    let date_str = if !date_parts.is_empty() {
        date_parts.join(" ")
    } else {
        String::new()
    };

    let time_str = if !time_parts.is_empty() {
        time_parts.join(":")
    } else {
        String::new()
    };

    let mut result = if !date_str.is_empty() && !time_str.is_empty() {
        format!("{}, {}", date_str, time_str)
    } else if !date_str.is_empty() {
        date_str
    } else {
        time_str
    };

    // TimeZone name
    if let Some(ref tzn) = opts.time_zone_name {
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(&format_tz_name(&opts.time_zone, tzn, ms));
    }

    result
}

const RANGE_SEP: &str = "\u{2009}\u{2013}\u{2009}";

fn format_range_with_options(
    start_ms: f64,
    end_ms: f64,
    opts: &DtfOptions,
) -> String {
    let start_str = format_with_options(start_ms, opts);
    let end_str = format_with_options(end_ms, opts);

    if start_str == end_str {
        return start_str;
    }

    // Smart range collapsing only for text-month, non-style formats
    if opts.date_style.is_none()
        && opts.time_style.is_none()
        && opts.hour.is_none()
        && opts.month.as_ref().is_some_and(|m| matches!(m.as_str(), "long" | "short" | "narrow"))
        && opts.year.is_some()
        && opts.day.is_some()
    {
        let sc = timestamp_to_components(start_ms);
        let ec = timestamp_to_components(end_ms);

        let month_fn = match opts.month.as_ref().unwrap().as_str() {
            "long" => month_name_long as fn(u32) -> &'static str,
            "short" => month_name_short as fn(u32) -> &'static str,
            "narrow" => month_name_narrow as fn(u32) -> &'static str,
            _ => month_name_long as fn(u32) -> &'static str,
        };

        let day_fn = |d: u32, opt: &str| -> String {
            if opt == "2-digit" { format_2digit(d) } else { d.to_string() }
        };
        let year_fn = |y: i32, opt: &str| -> String {
            if opt == "2-digit" { format_2digit((y.unsigned_abs() % 100) as u32) } else { y.to_string() }
        };

        let d_opt = opts.day.as_ref().unwrap().as_str();
        let y_opt = opts.year.as_ref().unwrap().as_str();

        if sc.year == ec.year && sc.month == ec.month {
            // Same year + same month: "Jan 3 – 5, 2019"
            return format!(
                "{} {}{}{}{}",
                month_fn(sc.month),
                day_fn(sc.day, d_opt),
                RANGE_SEP,
                day_fn(ec.day, d_opt),
                if opts.year.is_some() { format!(", {}", year_fn(sc.year, y_opt)) } else { String::new() }
            );
        } else if sc.year == ec.year {
            // Same year, different month: "Jan 3 – Mar 4, 2019"
            return format!(
                "{} {}{}{}{}",
                month_fn(sc.month),
                day_fn(sc.day, d_opt),
                RANGE_SEP,
                format!("{} {}", month_fn(ec.month), day_fn(ec.day, d_opt)),
                if opts.year.is_some() { format!(", {}", year_fn(sc.year, y_opt)) } else { String::new() }
            );
        }
        // Different year: full "Jan 3, 2019 – Mar 4, 2020"
    }

    // Same-day collapsing: if date parts are the same, show "date, time1 – time2"
    let start_parts = format_to_parts_with_options(start_ms, opts);
    let end_parts = format_to_parts_with_options(end_ms, opts);
    let date_types = ["year", "month", "day", "weekday", "era", "relatedYear"];
    let time_types = ["hour", "minute", "second", "fractionalSecond", "dayPeriod"];
    let start_date: Vec<_> = start_parts.iter()
        .filter(|(t, _)| date_types.contains(&t.as_str()))
        .collect();
    let end_date: Vec<_> = end_parts.iter()
        .filter(|(t, _)| date_types.contains(&t.as_str()))
        .collect();
    let has_time = start_parts.iter().any(|(t, _)| time_types.contains(&t.as_str()));

    if has_time && start_date == end_date && start_parts != end_parts {
        if let Some(time_start) = start_parts.iter().position(|(t, _)| time_types.contains(&t.as_str())) {
            let shared_end = if time_start > 0 && start_parts[time_start - 1].0 == "literal" {
                time_start - 1
            } else {
                time_start
            };
            let mut result = String::new();
            for (_, v) in &start_parts[..shared_end] {
                result.push_str(v);
            }
            if shared_end < time_start {
                result.push_str(&start_parts[shared_end].1);
            }
            for (_, v) in &start_parts[time_start..] {
                result.push_str(v);
            }
            result.push_str(RANGE_SEP);
            let end_time_start = end_parts.iter().position(|(t, _)| time_types.contains(&t.as_str()))
                .unwrap_or(0);
            for (_, v) in &end_parts[end_time_start..] {
                result.push_str(v);
            }
            return result;
        }
    }

    format!("{}{}{}", start_str, RANGE_SEP, end_str)
}

fn format_range_to_parts_with_options(
    start_ms: f64,
    end_ms: f64,
    opts: &DtfOptions,
) -> Vec<(String, String, String)> {
    let start_parts = format_to_parts_with_options(start_ms, opts);
    let end_parts = format_to_parts_with_options(end_ms, opts);

    if start_parts == end_parts {
        return start_parts
            .into_iter()
            .map(|(t, v)| (t, v, "shared".to_string()))
            .collect();
    }

    // Smart range collapsing for text-month, non-style formats
    if opts.date_style.is_none()
        && opts.time_style.is_none()
        && opts.hour.is_none()
        && opts.month.as_ref().is_some_and(|m| matches!(m.as_str(), "long" | "short" | "narrow"))
        && opts.year.is_some()
        && opts.day.is_some()
    {
        let sc = timestamp_to_components(start_ms);
        let ec = timestamp_to_components(end_ms);

        let month_fn = match opts.month.as_ref().unwrap().as_str() {
            "long" => month_name_long as fn(u32) -> &'static str,
            "short" => month_name_short as fn(u32) -> &'static str,
            "narrow" => month_name_narrow as fn(u32) -> &'static str,
            _ => month_name_long as fn(u32) -> &'static str,
        };

        let day_fn = |d: u32, opt: &str| -> String {
            if opt == "2-digit" { format_2digit(d) } else { d.to_string() }
        };
        let year_fn = |y: i32, opt: &str| -> String {
            if opt == "2-digit" { format_2digit((y.unsigned_abs() % 100) as u32) } else { y.to_string() }
        };

        let d_opt = opts.day.as_ref().unwrap().as_str();
        let y_opt = opts.year.as_ref().unwrap().as_str();

        let mut all: Vec<(String, String, String)> = Vec::new();

        if sc.year == ec.year && sc.month == ec.month {
            // "Jan 3 – 5, 2019" -- month shared, days differ, year shared
            all.push(("month".to_string(), month_fn(sc.month).to_string(), "shared".to_string()));
            all.push(("literal".to_string(), " ".to_string(), "shared".to_string()));
            all.push(("day".to_string(), day_fn(sc.day, d_opt), "startRange".to_string()));
            all.push(("literal".to_string(), RANGE_SEP.to_string(), "shared".to_string()));
            all.push(("day".to_string(), day_fn(ec.day, d_opt), "endRange".to_string()));
            all.push(("literal".to_string(), ", ".to_string(), "shared".to_string()));
            all.push(("year".to_string(), year_fn(sc.year, y_opt), "shared".to_string()));
            return all;
        } else if sc.year == ec.year {
            // "Jan 3 – Mar 4, 2019" -- months/days differ, year shared
            all.push(("month".to_string(), month_fn(sc.month).to_string(), "startRange".to_string()));
            all.push(("literal".to_string(), " ".to_string(), "startRange".to_string()));
            all.push(("day".to_string(), day_fn(sc.day, d_opt), "startRange".to_string()));
            all.push(("literal".to_string(), RANGE_SEP.to_string(), "shared".to_string()));
            all.push(("month".to_string(), month_fn(ec.month).to_string(), "endRange".to_string()));
            all.push(("literal".to_string(), " ".to_string(), "endRange".to_string()));
            all.push(("day".to_string(), day_fn(ec.day, d_opt), "endRange".to_string()));
            all.push(("literal".to_string(), ", ".to_string(), "shared".to_string()));
            all.push(("year".to_string(), year_fn(sc.year, y_opt), "shared".to_string()));
            return all;
        }
        // Different years: fall through to default (no collapsing)
    }

    // Check if date fields are the same but time fields differ (same-day range collapsing)
    let date_types = ["year", "month", "day", "weekday", "era", "relatedYear"];
    let time_types = ["hour", "minute", "second", "fractionalSecond", "dayPeriod"];

    let start_date: Vec<_> = start_parts.iter()
        .filter(|(t, _)| date_types.contains(&t.as_str()))
        .collect();
    let end_date: Vec<_> = end_parts.iter()
        .filter(|(t, _)| date_types.contains(&t.as_str()))
        .collect();
    let has_time = start_parts.iter().any(|(t, _)| time_types.contains(&t.as_str()));

    if has_time && start_date == end_date && start_parts != end_parts {
        // Find where time parts begin in the start_parts list
        let first_time_idx = start_parts.iter().position(|(t, _)| time_types.contains(&t.as_str()));
        if let Some(time_start) = first_time_idx {
            // Walk back to include the preceding literal separator (e.g. ", ")
            let shared_end = if time_start > 0 && start_parts[time_start - 1].0 == "literal" {
                time_start - 1
            } else {
                time_start
            };

            let mut all: Vec<(String, String, String)> = Vec::new();

            // Emit shared date prefix
            for (t, v) in &start_parts[..shared_end] {
                all.push((t.clone(), v.clone(), "shared".to_string()));
            }
            // Emit the literal separator before time as shared
            if shared_end < time_start {
                let (t, v) = &start_parts[shared_end];
                all.push((t.clone(), v.clone(), "shared".to_string()));
            }

            // Emit startRange time parts
            for (t, v) in &start_parts[time_start..] {
                all.push((t.clone(), v.clone(), "startRange".to_string()));
            }

            // Range separator
            all.push(("literal".to_string(), RANGE_SEP.to_string(), "shared".to_string()));

            // Find where time parts begin in end_parts
            let end_time_start = end_parts.iter().position(|(t, _)| time_types.contains(&t.as_str()))
                .unwrap_or(0);
            for (t, v) in &end_parts[end_time_start..] {
                all.push((t.clone(), v.clone(), "endRange".to_string()));
            }

            return all;
        }
    }

    // Default: startRange parts, separator, endRange parts
    let mut all: Vec<(String, String, String)> = Vec::new();
    for (t, v) in &start_parts {
        all.push((t.clone(), v.clone(), "startRange".to_string()));
    }
    all.push(("literal".to_string(), RANGE_SEP.to_string(), "shared".to_string()));
    for (t, v) in &end_parts {
        all.push((t.clone(), v.clone(), "endRange".to_string()));
    }
    all
}

fn is_utc_equivalent(tz: &str) -> bool {
    matches!(
        tz,
        "UTC" | "Etc/UTC" | "Etc/GMT" | "GMT" | "Etc/GMT+0" | "Etc/GMT-0" | "Etc/GMT0"
        | "Etc/Greenwich" | "Etc/UCT" | "Etc/Universal" | "Etc/Zulu"
        | "GMT+0" | "GMT-0" | "GMT0" | "Greenwich" | "UCT" | "Universal" | "Zulu"
    )
}

struct TzInfo {
    long_name: &'static str,
    short_name: &'static str,
    offset_hours: i32,
    offset_minutes: i32,
}

fn tz_lookup(tz: &str) -> Option<TzInfo> {
    let lower = tz.to_ascii_lowercase();
    // Map IANA timezone IDs (and aliases) to display info.
    // Offsets are standard time (non-DST). Display names are en-US.
    let (long, short, oh, om) = match lower.as_str() {
        // UTC equivalents
        "utc" | "etc/utc" | "etc/gmt" | "gmt" | "etc/gmt+0" | "etc/gmt-0" | "etc/gmt0"
        | "etc/greenwich" | "etc/uct" | "etc/universal" | "etc/zulu"
        | "gmt+0" | "gmt-0" | "gmt0" | "greenwich" | "uct" | "universal" | "zulu" =>
            ("Coordinated Universal Time", "UTC", 0, 0),

        // Etc/GMT offsets (note: Etc/GMT+N means UTC-N)
        "etc/gmt+1" => ("GMT-01:00", "GMT-1", -1, 0),
        "etc/gmt+2" => ("GMT-02:00", "GMT-2", -2, 0),
        "etc/gmt+3" => ("GMT-03:00", "GMT-3", -3, 0),
        "etc/gmt+4" => ("GMT-04:00", "GMT-4", -4, 0),
        "etc/gmt+5" => ("GMT-05:00", "GMT-5", -5, 0),
        "etc/gmt+6" => ("GMT-06:00", "GMT-6", -6, 0),
        "etc/gmt+7" => ("GMT-07:00", "GMT-7", -7, 0),
        "etc/gmt+8" => ("GMT-08:00", "GMT-8", -8, 0),
        "etc/gmt+9" => ("GMT-09:00", "GMT-9", -9, 0),
        "etc/gmt+10" => ("GMT-10:00", "GMT-10", -10, 0),
        "etc/gmt+11" => ("GMT-11:00", "GMT-11", -11, 0),
        "etc/gmt+12" => ("GMT-12:00", "GMT-12", -12, 0),
        "etc/gmt-1" => ("GMT+01:00", "GMT+1", 1, 0),
        "etc/gmt-2" => ("GMT+02:00", "GMT+2", 2, 0),
        "etc/gmt-3" => ("GMT+03:00", "GMT+3", 3, 0),
        "etc/gmt-4" => ("GMT+04:00", "GMT+4", 4, 0),
        "etc/gmt-5" => ("GMT+05:00", "GMT+5", 5, 0),
        "etc/gmt-6" => ("GMT+06:00", "GMT+6", 6, 0),
        "etc/gmt-7" => ("GMT+07:00", "GMT+7", 7, 0),
        "etc/gmt-8" => ("GMT+08:00", "GMT+8", 8, 0),
        "etc/gmt-9" => ("GMT+09:00", "GMT+9", 9, 0),
        "etc/gmt-10" => ("GMT+10:00", "GMT+10", 10, 0),
        "etc/gmt-11" => ("GMT+11:00", "GMT+11", 11, 0),
        "etc/gmt-12" => ("GMT+12:00", "GMT+12", 12, 0),
        "etc/gmt-13" => ("GMT+13:00", "GMT+13", 13, 0),
        "etc/gmt-14" => ("GMT+14:00", "GMT+14", 14, 0),

        // US zones
        "america/new_york" | "america/detroit" | "america/indiana/indianapolis"
        | "america/indiana/vevay" | "america/indiana/vincennes" | "america/indiana/winamac"
        | "america/indiana/marengo" | "america/indiana/petersburg"
        | "america/kentucky/louisville" | "america/kentucky/monticello"
        | "america/indianapolis" | "america/fort_wayne" | "america/louisville"
        | "us/eastern" | "us/east-indiana" | "us/michigan" =>
            ("Eastern Standard Time", "EST", -5, 0),

        "america/chicago" | "america/indiana/knox" | "america/indiana/tell_city"
        | "america/menominee" | "america/north_dakota/beulah"
        | "america/north_dakota/center" | "america/north_dakota/new_salem"
        | "america/knox_in" | "us/central" | "us/indiana-starke"
        | "america/mexico_city" | "mexico/general" | "america/matamoros"
        | "america/monterrey" | "america/merida" =>
            ("Central Standard Time", "CST", -6, 0),

        "america/denver" | "america/boise" | "america/shiprock" | "navajo"
        | "us/mountain" | "america/ojinaga" | "america/ciudad_juarez" =>
            ("Mountain Standard Time", "MST", -7, 0),

        "america/phoenix" | "america/creston" | "us/arizona" | "mst" | "mst7mdt" =>
            ("Mountain Standard Time", "MST", -7, 0),

        "america/los_angeles" | "us/pacific" | "america/tijuana"
        | "america/vancouver" | "america/santa_isabel" | "mexico/bajanorte"
        | "pst8pdt" =>
            ("Pacific Standard Time", "PST", -8, 0),

        "america/anchorage" | "america/juneau" | "america/sitka" | "america/yakutat"
        | "america/nome" | "america/metlakatla" | "us/alaska" =>
            ("Alaska Standard Time", "AKST", -9, 0),

        "america/adak" | "america/atka" | "us/aleutian" =>
            ("Hawaii-Aleutian Standard Time", "HST", -10, 0),

        "pacific/honolulu" | "us/hawaii" | "hst" =>
            ("Hawaii-Aleutian Standard Time", "HST", -10, 0),

        // Canada
        "america/toronto" | "america/nipigon" | "america/thunder_bay"
        | "america/iqaluit" | "america/pangnirtung" | "america/montreal"
        | "canada/eastern" =>
            ("Eastern Standard Time", "EST", -5, 0),

        "america/winnipeg" | "america/rainy_river" | "america/rankin_inlet"
        | "america/resolute" | "canada/central" =>
            ("Central Standard Time", "CST", -6, 0),

        "america/edmonton" | "america/cambridge_bay" | "america/inuvik"
        | "america/yellowknife" | "canada/mountain" =>
            ("Mountain Standard Time", "MST", -7, 0),

        "america/whitehorse" | "america/dawson" | "canada/yukon" =>
            ("Mountain Standard Time", "MST", -7, 0),

        "america/halifax" | "america/glace_bay" | "america/moncton"
        | "canada/atlantic" =>
            ("Atlantic Standard Time", "AST", -4, 0),

        "america/st_johns" | "canada/newfoundland" =>
            ("Newfoundland Standard Time", "NST", -3, -30),

        "america/regina" | "america/swift_current" | "canada/saskatchewan" =>
            ("Central Standard Time", "CST", -6, 0),

        "canada/pacific" => ("Pacific Standard Time", "PST", -8, 0),

        // Europe
        "europe/london" | "europe/belfast" | "europe/guernsey" | "europe/isle_of_man"
        | "europe/jersey" | "gb" | "gb-eire" =>
            ("Greenwich Mean Time", "GMT", 0, 0),

        "europe/dublin" | "eire" =>
            ("Greenwich Mean Time", "GMT", 0, 0),

        "atlantic/reykjavik" | "iceland" =>
            ("Greenwich Mean Time", "GMT", 0, 0),

        "europe/lisbon" | "portugal" | "atlantic/madeira" =>
            ("Western European Standard Time", "WET", 0, 0),

        "europe/paris" | "europe/brussels" | "europe/amsterdam" | "europe/luxembourg"
        | "europe/monaco" | "europe/zurich" | "europe/vaduz" | "europe/berlin"
        | "europe/copenhagen" | "europe/oslo" | "europe/stockholm"
        | "europe/vienna" | "europe/prague" | "europe/bratislava"
        | "europe/budapest" | "europe/warsaw" | "europe/belgrade"
        | "europe/ljubljana" | "europe/sarajevo" | "europe/zagreb"
        | "europe/skopje" | "europe/podgorica" | "europe/rome"
        | "europe/vatican" | "europe/san_marino" | "europe/malta"
        | "europe/andorra" | "europe/gibraltar" | "europe/tirane"
        | "europe/madrid" | "europe/busingen" | "cet" | "met"
        | "poland" | "arctic/longyearbyen" | "atlantic/jan_mayen" =>
            ("Central European Standard Time", "GMT+1", 1, 0),

        "europe/athens" | "europe/bucharest" | "europe/sofia" | "europe/helsinki"
        | "europe/tallinn" | "europe/riga" | "europe/vilnius" | "europe/mariehamn"
        | "europe/kiev" | "europe/kyiv" | "europe/uzhgorod" | "europe/zaporozhye"
        | "europe/chisinau" | "europe/tiraspol" | "eet" =>
            ("Eastern European Standard Time", "GMT+2", 2, 0),

        "europe/istanbul" | "asia/istanbul" | "turkey" =>
            ("Turkey Time", "GMT+3", 3, 0),

        "europe/moscow" | "europe/kirov" | "europe/simferopol" | "europe/volgograd"
        | "w-su" =>
            ("Moscow Standard Time", "GMT+3", 3, 0),

        "europe/samara" | "europe/astrakhan" | "europe/saratov" | "europe/ulyanovsk" =>
            ("Samara Standard Time", "GMT+4", 4, 0),

        "europe/kaliningrad" =>
            ("Eastern European Standard Time", "GMT+2", 2, 0),

        "europe/minsk" =>
            ("Moscow Standard Time", "GMT+3", 3, 0),

        // Asia
        "asia/kolkata" | "asia/calcutta" =>
            ("India Standard Time", "GMT+5:30", 5, 30),

        "asia/tokyo" | "japan" =>
            ("Japan Standard Time", "GMT+9", 9, 0),

        "asia/shanghai" | "asia/chongqing" | "asia/chungking" | "asia/harbin"
        | "prc" =>
            ("China Standard Time", "GMT+8", 8, 0),

        "asia/hong_kong" | "hongkong" =>
            ("Hong Kong Standard Time", "GMT+8", 8, 0),

        "asia/taipei" | "roc" =>
            ("Taipei Standard Time", "GMT+8", 8, 0),

        "asia/seoul" | "rok" =>
            ("Korean Standard Time", "GMT+9", 9, 0),

        "asia/singapore" | "singapore" =>
            ("Singapore Standard Time", "GMT+8", 8, 0),

        "asia/kuala_lumpur" | "asia/kuching" =>
            ("Malaysia Time", "GMT+8", 8, 0),

        "asia/bangkok" | "asia/phnom_penh" | "asia/vientiane" =>
            ("Indochina Time", "GMT+7", 7, 0),

        "asia/ho_chi_minh" | "asia/saigon" =>
            ("Indochina Time", "GMT+7", 7, 0),

        "asia/jakarta" | "asia/pontianak" =>
            ("Western Indonesia Time", "GMT+7", 7, 0),

        "asia/makassar" | "asia/ujung_pandang" =>
            ("Central Indonesia Time", "GMT+8", 8, 0),

        "asia/jayapura" =>
            ("Eastern Indonesia Time", "GMT+9", 9, 0),

        "asia/dubai" | "asia/muscat" =>
            ("Gulf Standard Time", "GMT+4", 4, 0),

        "asia/riyadh" | "asia/aden" | "asia/bahrain" | "asia/kuwait" | "asia/qatar" =>
            ("Arabian Standard Time", "GMT+3", 3, 0),

        "asia/tehran" | "iran" =>
            ("Iran Standard Time", "GMT+3:30", 3, 30),

        "asia/karachi" =>
            ("Pakistan Standard Time", "GMT+5", 5, 0),

        "asia/dhaka" | "asia/dacca" =>
            ("Bangladesh Standard Time", "GMT+6", 6, 0),

        "asia/yangon" | "asia/rangoon" =>
            ("Myanmar Time", "GMT+6:30", 6, 30),

        "asia/kathmandu" | "asia/katmandu" =>
            ("Nepal Time", "GMT+5:45", 5, 45),

        "asia/colombo" =>
            ("India Standard Time", "GMT+5:30", 5, 30),

        "asia/tashkent" | "asia/samarkand" =>
            ("Uzbekistan Standard Time", "GMT+5", 5, 0),

        "asia/almaty" | "asia/qostanay" =>
            ("East Kazakhstan Time", "GMT+6", 6, 0),

        "asia/aqtobe" | "asia/aqtau" | "asia/atyrau" | "asia/oral" | "asia/qyzylorda" =>
            ("West Kazakhstan Time", "GMT+5", 5, 0),

        "asia/yekaterinburg" =>
            ("Yekaterinburg Standard Time", "GMT+5", 5, 0),

        "asia/omsk" =>
            ("Omsk Standard Time", "GMT+6", 6, 0),

        "asia/novosibirsk" | "asia/novokuznetsk" | "asia/barnaul" | "asia/tomsk"
        | "asia/krasnoyarsk" =>
            ("Krasnoyarsk Standard Time", "GMT+7", 7, 0),

        "asia/irkutsk" =>
            ("Irkutsk Standard Time", "GMT+8", 8, 0),

        "asia/yakutsk" | "asia/chita" | "asia/khandyga" =>
            ("Yakutsk Standard Time", "GMT+9", 9, 0),

        "asia/vladivostok" | "asia/ust-nera" =>
            ("Vladivostok Standard Time", "GMT+10", 10, 0),

        "asia/magadan" | "asia/sakhalin" | "asia/srednekolymsk" =>
            ("Magadan Standard Time", "GMT+11", 11, 0),

        "asia/kamchatka" | "asia/anadyr" =>
            ("Kamchatka Standard Time", "GMT+12", 12, 0),

        "asia/jerusalem" | "asia/tel_aviv" | "israel" =>
            ("Israel Standard Time", "GMT+2", 2, 0),

        "asia/beirut" =>
            ("Eastern European Standard Time", "GMT+2", 2, 0),

        "asia/damascus" =>
            ("Eastern European Standard Time", "GMT+2", 2, 0),

        "asia/amman" =>
            ("Eastern European Standard Time", "GMT+2", 2, 0),

        "asia/baghdad" =>
            ("Arabian Standard Time", "GMT+3", 3, 0),

        "asia/kabul" =>
            ("Afghanistan Time", "GMT+4:30", 4, 30),

        "asia/baku" =>
            ("Azerbaijan Standard Time", "GMT+4", 4, 0),

        "asia/tbilisi" =>
            ("Georgia Standard Time", "GMT+4", 4, 0),

        "asia/yerevan" =>
            ("Armenia Standard Time", "GMT+4", 4, 0),

        "asia/dushanbe" =>
            ("Tajikistan Time", "GMT+5", 5, 0),

        "asia/ashgabat" | "asia/ashkhabad" =>
            ("Turkmenistan Standard Time", "GMT+5", 5, 0),

        "asia/bishkek" =>
            ("Kyrgyzstan Time", "GMT+6", 6, 0),

        "asia/brunei" =>
            ("Brunei Darussalam Time", "GMT+8", 8, 0),

        "asia/thimphu" | "asia/thimbu" =>
            ("Bhutan Time", "GMT+6", 6, 0),

        "asia/dili" =>
            ("East Timor Time", "GMT+9", 9, 0),

        "asia/manila" =>
            ("Philippine Standard Time", "GMT+8", 8, 0),

        "asia/ulaanbaatar" | "asia/ulan_bator" | "asia/choibalsan" | "asia/hovd" =>
            ("Ulaanbaatar Standard Time", "GMT+8", 8, 0),

        "asia/pyongyang" =>
            ("Korean Standard Time", "GMT+9", 9, 0),

        "asia/urumqi" | "asia/kashgar" =>
            ("China Standard Time", "GMT+6", 6, 0),

        "asia/gaza" | "asia/hebron" =>
            ("Eastern European Standard Time", "GMT+2", 2, 0),

        "asia/famagusta" | "asia/nicosia" | "europe/nicosia" =>
            ("Eastern European Standard Time", "GMT+2", 2, 0),

        "asia/macao" | "asia/macau" =>
            ("China Standard Time", "GMT+8", 8, 0),

        // Africa
        "africa/cairo" | "egypt" =>
            ("Eastern European Standard Time", "GMT+2", 2, 0),

        "africa/johannesburg" | "africa/harare" | "africa/lusaka" | "africa/maputo"
        | "africa/blantyre" | "africa/bujumbura" | "africa/gaborone"
        | "africa/kigali" | "africa/lubumbashi" | "africa/maseru" | "africa/mbabane"
        | "africa/windhoek" =>
            ("South Africa Standard Time", "GMT+2", 2, 0),

        "africa/lagos" | "africa/bangui" | "africa/brazzaville" | "africa/douala"
        | "africa/kinshasa" | "africa/libreville" | "africa/luanda"
        | "africa/malabo" | "africa/ndjamena" | "africa/niamey"
        | "africa/porto-novo" =>
            ("West Africa Standard Time", "GMT+1", 1, 0),

        "africa/nairobi" | "africa/addis_ababa" | "africa/asmara" | "africa/asmera"
        | "africa/dar_es_salaam" | "africa/djibouti" | "africa/kampala"
        | "africa/mogadishu" | "indian/antananarivo" | "indian/comoro"
        | "indian/mayotte" =>
            ("East Africa Time", "GMT+3", 3, 0),

        "africa/casablanca" =>
            ("Western European Standard Time", "WET", 0, 0),

        "africa/algiers" =>
            ("Central European Standard Time", "GMT+1", 1, 0),

        "africa/tunis" =>
            ("Central European Standard Time", "GMT+1", 1, 0),

        "africa/tripoli" | "libya" =>
            ("Eastern European Standard Time", "GMT+2", 2, 0),

        "africa/khartoum" | "africa/juba" =>
            ("Central Africa Time", "GMT+2", 2, 0),

        "africa/abidjan" | "africa/accra" | "africa/bamako" | "africa/banjul"
        | "africa/bissau" | "africa/conakry" | "africa/dakar" | "africa/freetown"
        | "africa/lome" | "africa/monrovia" | "africa/nouakchott"
        | "africa/ouagadougou" | "africa/sao_tome" | "africa/timbuktu"
        | "africa/el_aaiun" =>
            ("Greenwich Mean Time", "GMT", 0, 0),

        // Australia
        "australia/sydney" | "australia/melbourne" | "australia/act"
        | "australia/canberra" | "australia/nsw" | "australia/victoria"
        | "australia/currie" | "australia/hobart" | "australia/tasmania" =>
            ("Australian Eastern Standard Time", "GMT+10", 10, 0),

        "australia/brisbane" | "australia/queensland" | "australia/lindeman" =>
            ("Australian Eastern Standard Time", "GMT+10", 10, 0),

        "australia/adelaide" | "australia/south" | "australia/broken_hill"
        | "australia/yancowinna" =>
            ("Australian Central Standard Time", "GMT+9:30", 9, 30),

        "australia/darwin" | "australia/north" =>
            ("Australian Central Standard Time", "GMT+9:30", 9, 30),

        "australia/perth" | "australia/west" =>
            ("Australian Western Standard Time", "GMT+8", 8, 0),

        "australia/eucla" =>
            ("Australian Central Western Standard Time", "GMT+8:45", 8, 45),

        "australia/lord_howe" | "australia/lhi" =>
            ("Lord Howe Standard Time", "GMT+10:30", 10, 30),

        // Pacific
        "pacific/auckland" | "nz" | "antarctica/mcmurdo" | "antarctica/south_pole" =>
            ("New Zealand Standard Time", "GMT+12", 12, 0),

        "pacific/chatham" | "nz-chat" =>
            ("Chatham Standard Time", "GMT+12:45", 12, 45),

        "pacific/fiji" =>
            ("Fiji Standard Time", "GMT+12", 12, 0),

        "pacific/tongatapu" =>
            ("Tonga Standard Time", "GMT+13", 13, 0),

        "pacific/guam" | "pacific/saipan" =>
            ("Chamorro Standard Time", "GMT+10", 10, 0),

        "pacific/noumea" =>
            ("New Caledonia Standard Time", "GMT+11", 11, 0),

        "pacific/port_moresby" =>
            ("Papua New Guinea Time", "GMT+10", 10, 0),

        "pacific/guadalcanal" =>
            ("Solomon Islands Time", "GMT+11", 11, 0),

        "pacific/efate" =>
            ("Vanuatu Standard Time", "GMT+11", 11, 0),

        "pacific/tarawa" | "pacific/majuro" | "pacific/kwajalein" | "kwajalein" =>
            ("Marshall Islands Time", "GMT+12", 12, 0),

        "pacific/nauru" =>
            ("Nauru Time", "GMT+12", 12, 0),

        "pacific/funafuti" | "pacific/wallis" | "pacific/wake" =>
            ("Tuvalu Time", "GMT+12", 12, 0),

        "pacific/kiritimati" =>
            ("Line Islands Time", "GMT+14", 14, 0),

        "pacific/kanton" | "pacific/enderbury" =>
            ("Phoenix Islands Time", "GMT+13", 13, 0),

        "pacific/pago_pago" | "pacific/midway" | "pacific/samoa" | "us/samoa" =>
            ("Samoa Standard Time", "GMT-11", -11, 0),

        "pacific/niue" =>
            ("Niue Time", "GMT-11", -11, 0),

        "pacific/rarotonga" =>
            ("Cook Islands Standard Time", "GMT-10", -10, 0),

        "pacific/tahiti" | "pacific/gambier" | "pacific/marquesas" =>
            ("Tahiti Time", "GMT-10", -10, 0),

        "pacific/pitcairn" =>
            ("Pitcairn Time", "GMT-8", -8, 0),

        "pacific/galapagos" =>
            ("Galapagos Time", "GMT-6", -6, 0),

        "pacific/easter" | "chile/easterisland" =>
            ("Easter Island Standard Time", "GMT-6", -6, 0),

        "pacific/norfolk" =>
            ("Norfolk Island Standard Time", "GMT+11", 11, 0),

        "pacific/palau" =>
            ("Palau Time", "GMT+9", 9, 0),

        "pacific/chuuk" | "pacific/truk" | "pacific/yap" =>
            ("Chuuk Time", "GMT+10", 10, 0),

        "pacific/pohnpei" | "pacific/ponape" =>
            ("Pohnpei Standard Time", "GMT+11", 11, 0),

        "pacific/kosrae" =>
            ("Kosrae Time", "GMT+11", 11, 0),

        "pacific/bougainville" =>
            ("Bougainville Standard Time", "GMT+11", 11, 0),

        "pacific/apia" =>
            ("Apia Standard Time", "GMT+13", 13, 0),

        "pacific/fakaofo" =>
            ("Tokelau Time", "GMT+13", 13, 0),

        // Americas (non-US/Canada)
        "america/sao_paulo" | "america/fortaleza" | "america/recife" | "america/belem"
        | "america/maceio" | "america/bahia" | "america/santarem"
        | "america/araguaina" | "brazil/east" =>
            ("Brasilia Standard Time", "GMT-3", -3, 0),

        "america/manaus" | "america/porto_velho" | "america/boa_vista"
        | "america/campo_grande" | "america/cuiaba" | "brazil/west" =>
            ("Amazon Standard Time", "GMT-4", -4, 0),

        "america/noronha" | "brazil/denoronha" =>
            ("Fernando de Noronha Standard Time", "GMT-2", -2, 0),

        "america/rio_branco" | "america/eirunepe" | "america/porto_acre"
        | "brazil/acre" =>
            ("Acre Standard Time", "GMT-5", -5, 0),

        "america/argentina/buenos_aires" | "america/argentina/cordoba"
        | "america/argentina/salta" | "america/argentina/jujuy"
        | "america/argentina/tucuman" | "america/argentina/catamarca"
        | "america/argentina/la_rioja" | "america/argentina/san_juan"
        | "america/argentina/san_luis" | "america/argentina/mendoza"
        | "america/argentina/rio_gallegos" | "america/argentina/ushuaia"
        | "america/argentina/comodrivadavia"
        | "america/buenos_aires" | "america/catamarca" | "america/cordoba"
        | "america/jujuy" | "america/mendoza" | "america/rosario" =>
            ("Argentina Standard Time", "GMT-3", -3, 0),

        "america/santiago" | "chile/continental" | "america/punta_arenas" =>
            ("Chile Standard Time", "GMT-4", -4, 0),

        "america/bogota" =>
            ("Colombia Standard Time", "GMT-5", -5, 0),

        "america/lima" =>
            ("Peru Standard Time", "GMT-5", -5, 0),

        "america/caracas" =>
            ("Venezuela Time", "GMT-4", -4, 0),

        "america/guayaquil" =>
            ("Ecuador Time", "GMT-5", -5, 0),

        "america/asuncion" =>
            ("Paraguay Standard Time", "GMT-4", -4, 0),

        "america/montevideo" =>
            ("Uruguay Standard Time", "GMT-3", -3, 0),

        "america/la_paz" =>
            ("Bolivia Time", "GMT-4", -4, 0),

        "america/havana" | "cuba" =>
            ("Cuba Standard Time", "CST", -5, 0),

        "america/jamaica" | "jamaica" =>
            ("Eastern Standard Time", "EST", -5, 0),

        "america/panama" | "america/cayman" | "america/atikokan"
        | "america/coral_harbour" =>
            ("Eastern Standard Time", "EST", -5, 0),

        "america/guatemala" | "america/belize" | "america/el_salvador"
        | "america/costa_rica" | "america/tegucigalpa" | "america/managua" =>
            ("Central Standard Time", "CST", -6, 0),

        "america/port-au-prince" =>
            ("Eastern Standard Time", "EST", -5, 0),

        "america/santo_domingo" | "america/puerto_rico" | "america/virgin"
        | "america/st_thomas" | "america/tortola" | "america/anguilla"
        | "america/antigua" | "america/aruba" | "america/barbados"
        | "america/curacao" | "america/dominica" | "america/grenada"
        | "america/guadeloupe" | "america/kralendijk" | "america/lower_princes"
        | "america/marigot" | "america/martinique" | "america/montserrat"
        | "america/port_of_spain" | "america/st_barthelemy"
        | "america/st_kitts" | "america/st_lucia" | "america/st_vincent" =>
            ("Atlantic Standard Time", "AST", -4, 0),

        "america/cancun" =>
            ("Eastern Standard Time", "EST", -5, 0),

        "america/bahia_banderas" | "america/chihuahua" | "america/mazatlan"
        | "mexico/bajasur" =>
            ("Mountain Standard Time", "MST", -7, 0),

        "america/hermosillo" =>
            ("Mountain Standard Time", "MST", -7, 0),

        "america/dawson_creek" | "america/fort_nelson" =>
            ("Mountain Standard Time", "MST", -7, 0),

        "america/paramaribo" =>
            ("Suriname Time", "GMT-3", -3, 0),

        "america/cayenne" =>
            ("French Guiana Time", "GMT-3", -3, 0),

        "america/guyana" =>
            ("Guyana Time", "GMT-4", -4, 0),

        "america/miquelon" =>
            ("St. Pierre & Miquelon Standard Time", "GMT-3", -3, 0),

        "america/godthab" | "america/nuuk" =>
            ("West Greenland Standard Time", "GMT-3", -3, 0),

        "america/scoresbysund" =>
            ("East Greenland Standard Time", "GMT-1", -1, 0),

        "america/danmarkshavn" =>
            ("Greenwich Mean Time", "GMT", 0, 0),

        "america/thule" =>
            ("Atlantic Standard Time", "AST", -4, 0),

        "america/blanc-sablon" =>
            ("Atlantic Standard Time", "AST", -4, 0),

        "america/nassau" =>
            ("Eastern Standard Time", "EST", -5, 0),

        "america/grand_turk" =>
            ("Eastern Standard Time", "EST", -5, 0),

        // Indian Ocean
        "indian/mauritius" =>
            ("Mauritius Standard Time", "GMT+4", 4, 0),

        "indian/reunion" =>
            ("Reunion Time", "GMT+4", 4, 0),

        "indian/mahe" =>
            ("Seychelles Time", "GMT+4", 4, 0),

        "indian/maldives" =>
            ("Maldives Time", "GMT+5", 5, 0),

        "indian/chagos" =>
            ("Indian Ocean Time", "GMT+6", 6, 0),

        "indian/christmas" =>
            ("Christmas Island Time", "GMT+7", 7, 0),

        "indian/cocos" =>
            ("Cocos Islands Time", "GMT+6:30", 6, 30),

        "indian/kerguelen" =>
            ("French Southern & Antarctic Time", "GMT+5", 5, 0),

        // Antarctica
        "antarctica/casey" =>
            ("Australian Western Standard Time", "GMT+8", 8, 0),

        "antarctica/davis" =>
            ("Davis Time", "GMT+7", 7, 0),

        "antarctica/dumontdurville" =>
            ("Dumont-d'Urville Time", "GMT+10", 10, 0),

        "antarctica/mawson" =>
            ("Mawson Time", "GMT+5", 5, 0),

        "antarctica/palmer" =>
            ("Chile Standard Time", "GMT-3", -3, 0),

        "antarctica/rothera" =>
            ("Rothera Time", "GMT-3", -3, 0),

        "antarctica/syowa" =>
            ("Syowa Time", "GMT+3", 3, 0),

        "antarctica/troll" =>
            ("Greenwich Mean Time", "GMT", 0, 0),

        "antarctica/vostok" =>
            ("Vostok Time", "GMT+6", 6, 0),

        "antarctica/macquarie" =>
            ("Australian Eastern Standard Time", "GMT+10", 10, 0),

        // Atlantic
        "atlantic/azores" =>
            ("Azores Standard Time", "GMT-1", -1, 0),

        "atlantic/bermuda" =>
            ("Atlantic Standard Time", "AST", -4, 0),

        "atlantic/canary" =>
            ("Western European Standard Time", "WET", 0, 0),

        "atlantic/cape_verde" =>
            ("Cape Verde Standard Time", "GMT-1", -1, 0),

        "atlantic/faroe" | "atlantic/faeroe" =>
            ("Western European Standard Time", "WET", 0, 0),

        "atlantic/south_georgia" =>
            ("South Georgia Time", "GMT-2", -2, 0),

        "atlantic/st_helena" =>
            ("Greenwich Mean Time", "GMT", 0, 0),

        "atlantic/stanley" =>
            ("Falkland Islands Standard Time", "GMT-3", -3, 0),

        // Abbreviation-style zones
        "cst6cdt" =>
            ("Central Standard Time", "CST", -6, 0),

        "est5edt" | "est" =>
            ("Eastern Standard Time", "EST", -5, 0),

        "wet" =>
            ("Western European Standard Time", "WET", 0, 0),

        _ => return None,
    };

    Some(TzInfo {
        long_name: long,
        short_name: short,
        offset_hours: oh,
        offset_minutes: om,
    })
}

fn format_offset_short(hours: i32, minutes: i32) -> String {
    if hours == 0 && minutes == 0 {
        return "GMT".to_string();
    }
    let sign = if hours < 0 || (hours == 0 && minutes < 0) { "-" } else { "+" };
    let ah = hours.abs();
    let am = minutes.abs();
    if am == 0 {
        format!("GMT{}{}", sign, ah)
    } else {
        format!("GMT{}{}:{:02}", sign, ah, am)
    }
}

fn format_offset_long(hours: i32, minutes: i32) -> String {
    if hours == 0 && minutes == 0 {
        return "GMT".to_string();
    }
    let sign = if hours < 0 || (hours == 0 && minutes < 0) { "-" } else { "+" };
    let ah = hours.abs();
    let am = minutes.abs();
    format!("GMT{}{:02}:{:02}", sign, ah, am)
}

fn format_tz_name(tz: &str, style: &str, epoch_ms: f64) -> String {
    // Handle offset timezones (+03:00, -07:00, etc.)
    if let Some((h, m)) = parse_offset_timezone(tz) {
        return match style {
            "longOffset" => format_offset_long(h, m as i32),
            _ => format_offset_short(h, m as i32),
        };
    }

    // For offset-based styles, use DST-aware offset via chrono_tz
    if matches!(style, "shortOffset" | "longOffset") {
        let offset_ms = tz_offset_ms(tz, epoch_ms);
        let total_secs = (offset_ms / 1000.0) as i32;
        let hours = total_secs / 3600;
        let minutes = (total_secs.abs() % 3600) / 60;
        let signed_min = if hours < 0 { -(minutes) } else { minutes };
        return match style {
            "longOffset" => format_offset_long(hours, signed_min),
            _ => format_offset_short(hours, signed_min),
        };
    }

    // For named styles ("short", "long"), use chrono_tz for DST-aware names
    {
        use chrono::{Offset, TimeZone, Utc};
        use chrono_tz::Tz;

        let canonical = canonicalize_timezone(tz);
        let tz_str = if canonical.eq_ignore_ascii_case(tz) && canonical != tz.to_string() {
            canonical
        } else {
            tz.to_string()
        };

        if let Ok(tz_parsed) = tz_str.parse::<Tz>() {
            let epoch_secs = (epoch_ms / 1000.0).floor() as i64;
            let nanos = ((epoch_ms % 1000.0) * 1_000_000.0).abs() as u32;
            if let Some(dt) = Utc.timestamp_opt(epoch_secs, nanos).single() {
                let local = dt.with_timezone(&tz_parsed);
                let abbr = local.format("%Z").to_string();
                let offset = local.offset().fix();
                let offset_secs = offset.local_minus_utc();

                match style {
                    "short" | "shortGeneric" => return abbr,
                    "long" | "longGeneric" => {
                        // Map common abbreviations to full names
                        return tz_abbr_to_long_name(&abbr, offset_secs);
                    }
                    _ => return abbr,
                }
            }
        }
    }

    // Fallback to static lookup
    if let Some(info) = tz_lookup(tz) {
        match style {
            "long" => info.long_name.to_string(),
            "short" => info.short_name.to_string(),
            "shortGeneric" => info.short_name.to_string(),
            "longGeneric" => info.long_name.to_string(),
            _ => info.long_name.to_string(),
        }
    } else if is_utc_equivalent(tz) {
        match style {
            "long" | "longGeneric" => "Coordinated Universal Time".to_string(),
            "short" | "shortGeneric" => "UTC".to_string(),
            "shortOffset" | "longOffset" => "GMT".to_string(),
            _ => "UTC".to_string(),
        }
    } else {
        match style {
            "shortOffset" | "longOffset" => "GMT".to_string(),
            _ => tz.to_string(),
        }
    }
}

fn tz_abbr_to_long_name(abbr: &str, offset_secs: i32) -> String {
    match abbr {
        "EST" => "Eastern Standard Time".to_string(),
        "EDT" => "Eastern Daylight Time".to_string(),
        "CST" => "Central Standard Time".to_string(),
        "CDT" => "Central Daylight Time".to_string(),
        "MST" => "Mountain Standard Time".to_string(),
        "MDT" => "Mountain Daylight Time".to_string(),
        "PST" => "Pacific Standard Time".to_string(),
        "PDT" => "Pacific Daylight Time".to_string(),
        "AKST" => "Alaska Standard Time".to_string(),
        "AKDT" => "Alaska Daylight Time".to_string(),
        "HST" => "Hawaii-Aleutian Standard Time".to_string(),
        "HDT" => "Hawaii-Aleutian Daylight Time".to_string(),
        "GMT" | "UTC" => "Coordinated Universal Time".to_string(),
        "BST" => "British Summer Time".to_string(),
        "IST" => "Irish Standard Time".to_string(),
        "CET" => "Central European Standard Time".to_string(),
        "CEST" => "Central European Summer Time".to_string(),
        "EET" => "Eastern European Standard Time".to_string(),
        "EEST" => "Eastern European Summer Time".to_string(),
        "WET" => "Western European Standard Time".to_string(),
        "WEST" => "Western European Summer Time".to_string(),
        "JST" => "Japan Standard Time".to_string(),
        "KST" => "Korean Standard Time".to_string(),
        "CST" if offset_secs == 8 * 3600 => "China Standard Time".to_string(),
        "IST" if offset_secs == 19800 => "India Standard Time".to_string(),
        "AEST" => "Australian Eastern Standard Time".to_string(),
        "AEDT" => "Australian Eastern Daylight Time".to_string(),
        "ACST" => "Australian Central Standard Time".to_string(),
        "ACDT" => "Australian Central Daylight Time".to_string(),
        "AWST" => "Australian Western Standard Time".to_string(),
        "NZST" => "New Zealand Standard Time".to_string(),
        "NZDT" => "New Zealand Daylight Time".to_string(),
        _ => {
            // Fallback: use static lookup or abbreviation
            let hours = offset_secs / 3600;
            let minutes = (offset_secs.abs() % 3600) / 60;
            format_offset_long(hours, if hours < 0 { -(minutes) } else { minutes })
        }
    }
}

fn format_date_style_to_parts(c: &DateComponents, style: &str) -> Vec<(String, String)> {
    let mut parts: Vec<(String, String)> = Vec::new();
    match style {
        "full" => {
            parts.push(("weekday".to_string(), weekday_name_long(c.weekday).to_string()));
            parts.push(("literal".to_string(), ", ".to_string()));
            parts.push(("month".to_string(), month_name_long(c.month).to_string()));
            parts.push(("literal".to_string(), " ".to_string()));
            parts.push(("day".to_string(), c.day.to_string()));
            parts.push(("literal".to_string(), ", ".to_string()));
            parts.push(("year".to_string(), c.year.to_string()));
        }
        "long" => {
            parts.push(("month".to_string(), month_name_long(c.month).to_string()));
            parts.push(("literal".to_string(), " ".to_string()));
            parts.push(("day".to_string(), c.day.to_string()));
            parts.push(("literal".to_string(), ", ".to_string()));
            parts.push(("year".to_string(), c.year.to_string()));
        }
        "medium" => {
            parts.push(("month".to_string(), month_name_long(c.month).to_string()));
            parts.push(("literal".to_string(), " ".to_string()));
            parts.push(("day".to_string(), c.day.to_string()));
            parts.push(("literal".to_string(), ", ".to_string()));
            parts.push(("year".to_string(), c.year.to_string()));
        }
        "short" => {
            parts.push(("month".to_string(), c.month.to_string()));
            parts.push(("literal".to_string(), "/".to_string()));
            parts.push(("day".to_string(), c.day.to_string()));
            parts.push(("literal".to_string(), "/".to_string()));
            parts.push(("year".to_string(), format!("{}", c.year % 100)));
        }
        _ => {
            parts.push(("month".to_string(), c.month.to_string()));
            parts.push(("literal".to_string(), "/".to_string()));
            parts.push(("day".to_string(), c.day.to_string()));
            parts.push(("literal".to_string(), "/".to_string()));
            parts.push(("year".to_string(), c.year.to_string()));
        }
    }
    parts
}

fn format_reduced_date_style_to_parts(c: &DateComponents, style: &str, has_year: bool, has_month: bool, has_day: bool) -> Vec<(String, String)> {
    let mut parts: Vec<(String, String)> = Vec::new();
    if has_year && has_month && !has_day {
        // PlainYearMonth: month + year
        match style {
            "full" | "long" => {
                parts.push(("month".to_string(), month_name_long(c.month).to_string()));
                parts.push(("literal".to_string(), " ".to_string()));
                parts.push(("year".to_string(), c.year.to_string()));
            }
            "medium" => {
                parts.push(("month".to_string(), month_name_short(c.month).to_string()));
                parts.push(("literal".to_string(), " ".to_string()));
                parts.push(("year".to_string(), c.year.to_string()));
            }
            _ => {
                parts.push(("month".to_string(), c.month.to_string()));
                parts.push(("literal".to_string(), "/".to_string()));
                parts.push(("year".to_string(), format!("{}", c.year % 100)));
            }
        }
    } else if !has_year && has_month && has_day {
        // PlainMonthDay: month + day
        match style {
            "full" | "long" => {
                parts.push(("month".to_string(), month_name_long(c.month).to_string()));
                parts.push(("literal".to_string(), " ".to_string()));
                parts.push(("day".to_string(), c.day.to_string()));
            }
            "medium" => {
                parts.push(("month".to_string(), month_name_short(c.month).to_string()));
                parts.push(("literal".to_string(), " ".to_string()));
                parts.push(("day".to_string(), c.day.to_string()));
            }
            _ => {
                parts.push(("month".to_string(), c.month.to_string()));
                parts.push(("literal".to_string(), "/".to_string()));
                parts.push(("day".to_string(), c.day.to_string()));
            }
        }
    }
    parts
}

fn format_time_style_to_parts(c: &DateComponents, style: &str, hc: &str, tz: &str, epoch_ms: f64) -> Vec<(String, String)> {
    let mut parts: Vec<(String, String)> = Vec::new();
    let (hour_str, period) = format_hour(c.hour, hc);
    let uses_period = hc == "h12" || hc == "h11";

    match style {
        "full" => {
            parts.push(("hour".to_string(), hour_str));
            parts.push(("literal".to_string(), ":".to_string()));
            parts.push(("minute".to_string(), format!("{:02}", c.minute)));
            parts.push(("literal".to_string(), ":".to_string()));
            parts.push(("second".to_string(), format!("{:02}", c.second)));
            if uses_period {
                parts.push(("literal".to_string(), " ".to_string()));
                parts.push(("dayPeriod".to_string(), period.to_string()));
            }
            let tz_name = format_tz_name(tz, "long", epoch_ms);
            parts.push(("literal".to_string(), " ".to_string()));
            parts.push(("timeZoneName".to_string(), tz_name));
        }
        "long" => {
            parts.push(("hour".to_string(), hour_str));
            parts.push(("literal".to_string(), ":".to_string()));
            parts.push(("minute".to_string(), format!("{:02}", c.minute)));
            parts.push(("literal".to_string(), ":".to_string()));
            parts.push(("second".to_string(), format!("{:02}", c.second)));
            if uses_period {
                parts.push(("literal".to_string(), " ".to_string()));
                parts.push(("dayPeriod".to_string(), period.to_string()));
            }
            let short_tz = format_tz_name(tz, "short", epoch_ms);
            parts.push(("literal".to_string(), " ".to_string()));
            parts.push(("timeZoneName".to_string(), short_tz));
        }
        "medium" => {
            parts.push(("hour".to_string(), hour_str));
            parts.push(("literal".to_string(), ":".to_string()));
            parts.push(("minute".to_string(), format!("{:02}", c.minute)));
            parts.push(("literal".to_string(), ":".to_string()));
            parts.push(("second".to_string(), format!("{:02}", c.second)));
            if uses_period {
                parts.push(("literal".to_string(), " ".to_string()));
                parts.push(("dayPeriod".to_string(), period.to_string()));
            }
        }
        "short" => {
            parts.push(("hour".to_string(), hour_str));
            parts.push(("literal".to_string(), ":".to_string()));
            parts.push(("minute".to_string(), format!("{:02}", c.minute)));
            if uses_period {
                parts.push(("literal".to_string(), " ".to_string()));
                parts.push(("dayPeriod".to_string(), period.to_string()));
            }
        }
        _ => {
            parts.push(("hour".to_string(), hour_str));
            parts.push(("literal".to_string(), ":".to_string()));
            parts.push(("minute".to_string(), format!("{:02}", c.minute)));
            parts.push(("literal".to_string(), ":".to_string()));
            parts.push(("second".to_string(), format!("{:02}", c.second)));
            if uses_period {
                parts.push(("literal".to_string(), " ".to_string()));
                parts.push(("dayPeriod".to_string(), period.to_string()));
            }
        }
    }
    parts
}

fn format_to_parts_with_options(
    ms: f64,
    opts: &DtfOptions,
) -> Vec<(String, String)> {
    let raw = format_to_parts_with_options_raw(ms, opts);
    if opts.numbering_system == "latn" {
        return raw;
    }
    raw.into_iter()
        .map(|(typ, val)| {
            if typ == "literal" || typ == "timeZoneName" || typ == "era" || typ == "dayPeriod" {
                (typ, val)
            } else {
                (typ, transliterate_digits(&val, &opts.numbering_system))
            }
        })
        .collect()
}

fn format_to_parts_with_options_raw(
    ms: f64,
    opts: &DtfOptions,
) -> Vec<(String, String)> {
    let adjusted_ms = ms + tz_offset_ms(&opts.time_zone, ms);
    let c = timestamp_to_components(adjusted_ms);
    let hc = resolve_hour_cycle(opts);
    let mut parts: Vec<(String, String)> = Vec::new();

    if opts.date_style.is_some() || opts.time_style.is_some() {
        let need_reduced = opts.date_style.is_some()
            && matches!(opts.temporal_type, Some(TemporalType::PlainYearMonth) | Some(TemporalType::PlainMonthDay));
        if need_reduced {
            let ds = opts.date_style.as_deref().unwrap_or("short");
            let is_ym = matches!(opts.temporal_type, Some(TemporalType::PlainYearMonth));
            let rp = format_reduced_date_style_to_parts(&c, ds, is_ym || opts.year.is_some(), true, !is_ym);
            parts.extend(rp);
        } else if let Some(ref ds) = opts.date_style {
            let date_parts = format_date_style_to_parts(&c, ds);
            parts.extend(date_parts);
        }
        let effective_ts = opts.time_style.as_ref().map(|ts| {
            let is_plain_temporal = matches!(
                opts.temporal_type,
                Some(TemporalType::PlainTime) | Some(TemporalType::PlainDateTime)
                    | Some(TemporalType::PlainDate) | Some(TemporalType::PlainYearMonth)
                    | Some(TemporalType::PlainMonthDay)
            );
            if is_plain_temporal && opts.time_zone_name.is_none() && (ts == "long" || ts == "full") {
                "medium".to_string()
            } else {
                ts.clone()
            }
        });
        if (opts.date_style.is_some() || need_reduced) && effective_ts.is_some() {
            parts.push(("literal".to_string(), ", ".to_string()));
        }
        if let Some(ref ts) = effective_ts {
            let time_parts = format_time_style_to_parts(&c, ts, hc, &opts.time_zone, ms);
            parts.extend(time_parts);
        }
        return parts;
    }

    let has_date = opts.year.is_some() || opts.month.is_some() || opts.day.is_some();
    let has_time = has_time_component(opts);

    // Weekday
    if let Some(ref wd) = opts.weekday {
        let s = match wd.as_str() {
            "long" => weekday_name_long(c.weekday).to_string(),
            "short" => weekday_name_short(c.weekday).to_string(),
            "narrow" => weekday_name_narrow(c.weekday).to_string(),
            _ => weekday_name_long(c.weekday).to_string(),
        };
        parts.push(("weekday".to_string(), s));
        if has_date || has_time {
            parts.push(("literal".to_string(), ", ".to_string()));
        }
    }

    // Date components
    if has_date {
        let month_is_text = opts.month.as_ref().is_some_and(|m| {
            matches!(m.as_str(), "long" | "short" | "narrow")
        });

        let display_year = if opts.era.is_some() && c.year <= 0 {
            1 - c.year
        } else {
            c.year
        };

        if month_is_text {
            if let Some(ref m) = opts.month {
                let s = match m.as_str() {
                    "long" => month_name_long(c.month).to_string(),
                    "short" => month_name_short(c.month).to_string(),
                    "narrow" => month_name_narrow(c.month).to_string(),
                    _ => c.month.to_string(),
                };
                parts.push(("month".to_string(), s));
            }
            if opts.day.is_some() {
                parts.push(("literal".to_string(), " ".to_string()));
                let d = match opts.day.as_ref().unwrap().as_str() {
                    "2-digit" => format_2digit(c.day),
                    _ => c.day.to_string(),
                };
                parts.push(("day".to_string(), d));
            }
            if opts.year.is_some() {
                parts.push(("literal".to_string(), ", ".to_string()));
                let y = match opts.year.as_ref().unwrap().as_str() {
                    "2-digit" => format_2digit((display_year.unsigned_abs() % 100) as u32),
                    _ => display_year.to_string(),
                };
                parts.push(("year".to_string(), y));
            }
            if let Some(ref e) = opts.era {
                parts.push(("literal".to_string(), " ".to_string()));
                let s = match e.as_str() {
                    "long" => era_long(c.year).to_string(),
                    "short" => era_short(c.year).to_string(),
                    "narrow" => era_narrow(c.year).to_string(),
                    _ => era_short(c.year).to_string(),
                };
                parts.push(("era".to_string(), s));
            }
        } else {
            // Numeric date: M/D/YYYY
            if let Some(ref m) = opts.month {
                let s = match m.as_str() {
                    "2-digit" => format_2digit(c.month),
                    _ => c.month.to_string(),
                };
                parts.push(("month".to_string(), s));
            }
            if opts.month.is_some() && opts.day.is_some() {
                parts.push(("literal".to_string(), "/".to_string()));
            }
            if let Some(ref d) = opts.day {
                let s = match d.as_str() {
                    "2-digit" => format_2digit(c.day),
                    _ => c.day.to_string(),
                };
                parts.push(("day".to_string(), s));
            }
            if (opts.month.is_some() || opts.day.is_some()) && opts.year.is_some() {
                parts.push(("literal".to_string(), "/".to_string()));
            }
            if let Some(ref y) = opts.year {
                let s = match y.as_str() {
                    "2-digit" => format_2digit((display_year.unsigned_abs() % 100) as u32),
                    _ => display_year.to_string(),
                };
                parts.push(("year".to_string(), s));
            }
            if let Some(ref e) = opts.era {
                parts.push(("literal".to_string(), " ".to_string()));
                let s = match e.as_str() {
                    "long" => era_long(c.year).to_string(),
                    "short" => era_short(c.year).to_string(),
                    "narrow" => era_narrow(c.year).to_string(),
                    _ => era_short(c.year).to_string(),
                };
                parts.push(("era".to_string(), s));
            }
        }
    }

    // Separator between date and time
    if has_date && has_time {
        parts.push(("literal".to_string(), ", ".to_string()));
    }

    // Time components
    if has_time {
        let uses_period = hc == "h12" || hc == "h11";
        let (hour_val, period) = format_hour(c.hour, hc);

        // dayPeriod alone (no hour)
        if opts.day_period.is_some() && opts.hour.is_none() {
            let dp_text = day_period_text(c.hour, opts.day_period.as_ref().unwrap());
            parts.push(("dayPeriod".to_string(), dp_text.to_string()));
            // Skip the rest of time components
        } else {
            if let Some(ref h) = opts.hour {
                let s = match h.as_str() {
                    "2-digit" => {
                        let v = match hc {
                            "h12" => {
                                if c.hour == 0 { 12 } else if c.hour > 12 { c.hour - 12 } else { c.hour }
                            }
                            "h11" => c.hour % 12,
                            "h23" => c.hour,
                            "h24" => if c.hour == 0 { 24 } else { c.hour },
                            _ => c.hour,
                        };
                        format_2digit(v)
                    }
                    _ => hour_val.clone(),
                };
                parts.push(("hour".to_string(), s));
            }

            if opts.hour.is_some() && opts.minute.is_some() {
                parts.push(("literal".to_string(), ":".to_string()));
            }

            if let Some(ref m) = opts.minute {
                let s = match m.as_str() {
                    "2-digit" => format_2digit(c.minute),
                    _ => {
                        if opts.hour.is_some() || opts.second.is_some() {
                            format_2digit(c.minute)
                        } else {
                            c.minute.to_string()
                        }
                    }
                };
                parts.push(("minute".to_string(), s));
            }

            if (opts.hour.is_some() || opts.minute.is_some()) && opts.second.is_some() {
                parts.push(("literal".to_string(), ":".to_string()));
            }

            if let Some(ref s) = opts.second {
                let sv = match s.as_str() {
                    "2-digit" => format_2digit(c.second),
                    _ => {
                        if opts.hour.is_some() || opts.minute.is_some() {
                            format_2digit(c.second)
                        } else {
                            c.second.to_string()
                        }
                    }
                };
                parts.push(("second".to_string(), sv));
            }

            if let Some(digits) = opts.fractional_second_digits {
                let frac = match digits {
                    1 => format!("{}", c.millisecond / 100),
                    2 => format!("{:02}", c.millisecond / 10),
                    3 => format!("{:03}", c.millisecond),
                    _ => String::new(),
                };
                if !frac.is_empty() {
                    parts.push(("literal".to_string(), ".".to_string()));
                    parts.push(("fractionalSecond".to_string(), frac));
                }
            }

            if opts.day_period.is_some() && opts.hour.is_some() {
                let dp_text = day_period_text(c.hour, opts.day_period.as_ref().unwrap());
                parts.push(("literal".to_string(), " ".to_string()));
                parts.push(("dayPeriod".to_string(), dp_text.to_string()));
            } else if uses_period && opts.hour.is_some() {
                parts.push(("literal".to_string(), " ".to_string()));
                parts.push(("dayPeriod".to_string(), period.to_string()));
            }
        }
    }

    // TimeZone name
    if let Some(ref tzn) = opts.time_zone_name {
        parts.push(("literal".to_string(), " ".to_string()));
        parts.push((
            "timeZoneName".to_string(),
            format_tz_name(&opts.time_zone, tzn, ms),
        ));
    }

    parts
}

fn extract_dtf_data(
    interp: &mut Interpreter,
    this: &JsValue,
) -> Result<DtfOptions, JsValue> {
    if let JsValue::Object(o) = this {
        if let Some(obj) = interp.get_object(o.id) {
            let b = obj.borrow();
            if let Some(IntlData::DateTimeFormat {
                ref locale,
                ref calendar,
                ref numbering_system,
                ref time_zone,
                ref hour_cycle,
                ref hour12,
                ref weekday,
                ref era,
                ref year,
                ref month,
                ref day,
                ref day_period,
                ref hour,
                ref minute,
                ref second,
                ref fractional_second_digits,
                ref time_zone_name,
                ref date_style,
                ref time_style,
                has_explicit_components,
            }) = b.intl_data
            {
                return Ok(DtfOptions {
                    locale: locale.clone(),
                    calendar: calendar.clone(),
                    numbering_system: numbering_system.clone(),
                    time_zone: time_zone.clone(),
                    hour_cycle: hour_cycle.clone(),
                    hour12: *hour12,
                    weekday: weekday.clone(),
                    era: era.clone(),
                    year: year.clone(),
                    month: month.clone(),
                    day: day.clone(),
                    day_period: day_period.clone(),
                    hour: hour.clone(),
                    minute: minute.clone(),
                    second: second.clone(),
                    fractional_second_digits: *fractional_second_digits,
                    time_zone_name: time_zone_name.clone(),
                    date_style: date_style.clone(),
                    time_style: time_style.clone(),
                    has_explicit_components,
                    temporal_type: None,
                });
            }
        }
    }
    Err(interp.create_type_error(
        "Intl.DateTimeFormat method called on incompatible receiver",
    ))
}

fn time_clip(t: f64) -> f64 {
    if !t.is_finite() {
        return f64::NAN;
    }
    if t.abs() > 8.64e15 {
        return f64::NAN;
    }
    let clipped = t.trunc();
    if clipped == 0.0 {
        0.0_f64 // convert -0 to +0
    } else {
        clipped
    }
}

fn resolve_date_value(interp: &mut Interpreter, date_arg: &JsValue) -> Result<f64, JsValue> {
    if matches!(date_arg, JsValue::Undefined) {
        return Ok(now_ms().floor());
    }
    // Check if it's a Temporal object
    if let JsValue::Object(o) = date_arg {
        if let Some(obj) = interp.get_object(o.id) {
            let temporal = obj.borrow().temporal_data.clone();
            if let Some(td) = temporal {
                return temporal_to_epoch_ms(&td);
            }
        }
    }
    let num = interp.to_number_value(date_arg)?;
    let clipped = time_clip(num);
    if clipped.is_nan() {
        return Err(interp.create_range_error("Invalid time value"));
    }
    Ok(clipped)
}

#[derive(Clone, Copy)]
enum TemporalType {
    Instant,
    ZonedDateTime,
    PlainDate,
    PlainTime,
    PlainDateTime,
    PlainYearMonth,
    PlainMonthDay,
}

fn detect_temporal_type(interp: &Interpreter, val: &JsValue) -> Option<TemporalType> {
    if let JsValue::Object(o) = val {
        if let Some(obj) = interp.get_object(o.id) {
            let td = obj.borrow().temporal_data.clone();
            return match td {
                Some(TemporalData::Instant { .. }) => Some(TemporalType::Instant),
                Some(TemporalData::ZonedDateTime { .. }) => Some(TemporalType::ZonedDateTime),
                Some(TemporalData::PlainDate { .. }) => Some(TemporalType::PlainDate),
                Some(TemporalData::PlainTime { .. }) => Some(TemporalType::PlainTime),
                Some(TemporalData::PlainDateTime { .. }) => Some(TemporalType::PlainDateTime),
                Some(TemporalData::PlainYearMonth { .. }) => Some(TemporalType::PlainYearMonth),
                Some(TemporalData::PlainMonthDay { .. }) => Some(TemporalType::PlainMonthDay),
                _ => None,
            };
        }
    }
    None
}

fn has_explicit_date_time_opts(opts: &DtfOptions) -> bool {
    opts.has_explicit_components
}

fn adjust_opts_for_temporal(opts: &DtfOptions, tt: TemporalType) -> DtfOptions {
    let mut adjusted = opts.clone();
    adjusted.temporal_type = Some(tt);

    // When explicit components are set, filter out non-overlapping ones
    if has_explicit_date_time_opts(opts) || opts.date_style.is_some() || opts.time_style.is_some() {
        match tt {
            TemporalType::PlainDate => {
                // Remove time components
                adjusted.hour = None;
                adjusted.minute = None;
                adjusted.second = None;
                adjusted.fractional_second_digits = None;
                adjusted.day_period = None;
                adjusted.time_zone_name = None;
                adjusted.time_style = None;
                // If no date components remain, add defaults
                if adjusted.year.is_none() && adjusted.month.is_none()
                    && adjusted.day.is_none() && adjusted.weekday.is_none()
                    && adjusted.date_style.is_none()
                {
                    adjusted.year = Some("numeric".to_string());
                    adjusted.month = Some("numeric".to_string());
                    adjusted.day = Some("numeric".to_string());
                }
            }
            TemporalType::PlainTime => {
                // Remove date components
                adjusted.year = None;
                adjusted.month = None;
                adjusted.day = None;
                adjusted.weekday = None;
                adjusted.era = None;
                adjusted.time_zone_name = None;
                adjusted.date_style = None;
                // If no time components remain, add defaults
                if adjusted.hour.is_none() && adjusted.minute.is_none()
                    && adjusted.second.is_none() && adjusted.fractional_second_digits.is_none()
                    && adjusted.day_period.is_none() && adjusted.time_style.is_none()
                {
                    adjusted.hour = Some("numeric".to_string());
                    adjusted.minute = Some("2-digit".to_string());
                    adjusted.second = Some("2-digit".to_string());
                }
            }
            TemporalType::PlainYearMonth => {
                // Keep only year, month, era
                adjusted.day = None;
                adjusted.weekday = None;
                adjusted.hour = None;
                adjusted.minute = None;
                adjusted.second = None;
                adjusted.fractional_second_digits = None;
                adjusted.day_period = None;
                adjusted.time_zone_name = None;
                adjusted.time_style = None;
                // If no year/month remain, add defaults
                if adjusted.year.is_none() && adjusted.month.is_none()
                    && adjusted.date_style.is_none()
                {
                    adjusted.year = Some("numeric".to_string());
                    adjusted.month = Some("numeric".to_string());
                }
            }
            TemporalType::PlainMonthDay => {
                // Keep only month, day
                adjusted.year = None;
                adjusted.era = None;
                adjusted.weekday = None;
                adjusted.hour = None;
                adjusted.minute = None;
                adjusted.second = None;
                adjusted.fractional_second_digits = None;
                adjusted.day_period = None;
                adjusted.time_zone_name = None;
                adjusted.time_style = None;
                // If no month/day remain, add defaults
                if adjusted.month.is_none() && adjusted.day.is_none()
                    && adjusted.date_style.is_none()
                {
                    adjusted.month = Some("numeric".to_string());
                    adjusted.day = Some("numeric".to_string());
                }
            }
            TemporalType::PlainDateTime => {
                adjusted.time_zone_name = None;
                // If no date/time components remain, add defaults
                if !has_explicit_date_time_opts(&adjusted) && adjusted.date_style.is_none()
                    && adjusted.time_style.is_none()
                {
                    adjusted.year = Some("numeric".to_string());
                    adjusted.month = Some("numeric".to_string());
                    adjusted.day = Some("numeric".to_string());
                    adjusted.hour = Some("2-digit".to_string());
                    adjusted.minute = Some("2-digit".to_string());
                    adjusted.second = Some("2-digit".to_string());
                }
            }
            TemporalType::Instant => {
                // Instant formats like a full date/time, keep timeZoneName
            }
            TemporalType::ZonedDateTime => {}
        }
        return adjusted;
    }

    // No explicit components: set defaults based on Temporal type
    // Also strip timeZoneName for plain types (they have no timezone)
    match tt {
        TemporalType::Instant | TemporalType::ZonedDateTime => {
            adjusted.year = Some("numeric".to_string());
            adjusted.month = Some("numeric".to_string());
            adjusted.day = Some("numeric".to_string());
            adjusted.hour = Some("2-digit".to_string());
            adjusted.minute = Some("2-digit".to_string());
            adjusted.second = Some("2-digit".to_string());
        }
        TemporalType::PlainDateTime => {
            adjusted.year = Some("numeric".to_string());
            adjusted.month = Some("numeric".to_string());
            adjusted.day = Some("numeric".to_string());
            adjusted.hour = Some("2-digit".to_string());
            adjusted.minute = Some("2-digit".to_string());
            adjusted.second = Some("2-digit".to_string());
            adjusted.time_zone_name = None;
        }
        TemporalType::PlainDate => {
            adjusted.year = Some("numeric".to_string());
            adjusted.month = Some("numeric".to_string());
            adjusted.day = Some("numeric".to_string());
            adjusted.hour = None;
            adjusted.minute = None;
            adjusted.second = None;
            adjusted.fractional_second_digits = None;
            adjusted.day_period = None;
            adjusted.time_zone_name = None;
        }
        TemporalType::PlainTime => {
            adjusted.year = None;
            adjusted.month = None;
            adjusted.day = None;
            adjusted.weekday = None;
            adjusted.era = None;
            adjusted.hour = Some("numeric".to_string());
            adjusted.minute = Some("2-digit".to_string());
            adjusted.second = Some("2-digit".to_string());
            adjusted.time_zone_name = None;
        }
        TemporalType::PlainYearMonth => {
            adjusted.year = Some("numeric".to_string());
            adjusted.month = Some("numeric".to_string());
            adjusted.day = None;
            adjusted.weekday = None;
            adjusted.hour = None;
            adjusted.minute = None;
            adjusted.second = None;
            adjusted.fractional_second_digits = None;
            adjusted.day_period = None;
            adjusted.time_zone_name = None;
        }
        TemporalType::PlainMonthDay => {
            adjusted.year = None;
            adjusted.era = None;
            adjusted.weekday = None;
            adjusted.month = Some("numeric".to_string());
            adjusted.day = Some("numeric".to_string());
            adjusted.hour = None;
            adjusted.minute = None;
            adjusted.second = None;
            adjusted.fractional_second_digits = None;
            adjusted.day_period = None;
            adjusted.time_zone_name = None;
        }
    }
    adjusted
}

fn check_temporal_overlap(opts: &DtfOptions, tt: TemporalType) -> bool {
    // Instant and PlainDateTime overlap with everything.
    if matches!(tt, TemporalType::Instant | TemporalType::PlainDateTime | TemporalType::ZonedDateTime) {
        return true;
    }

    // If no explicit date/time components (only defaults/timeZoneName), always overlap
    if !opts.has_explicit_components {
        return true;
    }

    let type_has_date = matches!(tt, TemporalType::PlainDate | TemporalType::PlainYearMonth | TemporalType::PlainMonthDay);
    let type_has_time = matches!(tt, TemporalType::PlainTime);

    // Style-based overlap: at least one style must overlap with the type
    if opts.date_style.is_some() || opts.time_style.is_some() {
        let date_overlaps = opts.date_style.is_some() && type_has_date;
        let time_overlaps = opts.time_style.is_some() && type_has_time;
        return date_overlaps || time_overlaps;
    }

    // Check explicit component options
    let has_any_date_opt = opts.year.is_some() || opts.month.is_some() || opts.day.is_some()
        || opts.weekday.is_some();
    let has_any_time_opt = opts.hour.is_some() || opts.minute.is_some() || opts.second.is_some()
        || opts.day_period.is_some() || opts.fractional_second_digits.is_some();

    match tt {
        TemporalType::PlainDate => {
            if has_any_date_opt { return true; }
            if has_any_time_opt && !has_any_date_opt { return false; }
            true
        }
        TemporalType::PlainTime => {
            if has_any_time_opt { return true; }
            if has_any_date_opt && !has_any_time_opt { return false; }
            true
        }
        TemporalType::PlainYearMonth => {
            if opts.year.is_some() || opts.month.is_some() || opts.era.is_some() { return true; }
            if opts.day.is_some() || opts.weekday.is_some() { return false; }
            if has_any_time_opt { return false; }
            true
        }
        TemporalType::PlainMonthDay => {
            if opts.month.is_some() || opts.day.is_some() { return true; }
            if opts.year.is_some() || opts.era.is_some() || opts.weekday.is_some() { return false; }
            if has_any_time_opt { return false; }
            true
        }
        _ => true,
    }
}

fn adjust_plain_temporal_ms(ms: f64, tz: &str, tt: TemporalType) -> f64 {
    // Plain temporal types have no timezone — their ISO fields represent local date/time.
    // The formatter applies tz_offset_ms(tz) to convert from UTC to local.
    // We counteract that by subtracting the offset so the result is the original fields.
    match tt {
        TemporalType::PlainDate | TemporalType::PlainTime | TemporalType::PlainDateTime
        | TemporalType::PlainYearMonth | TemporalType::PlainMonthDay => {
            ms - tz_offset_ms(tz, ms)
        }
        // Instant and ZonedDateTime already encode a real UTC instant
        TemporalType::Instant | TemporalType::ZonedDateTime => ms,
    }
}

fn bigint_to_epoch_ms(epoch_nanoseconds: &num_bigint::BigInt) -> f64 {
    let ms_bigint = epoch_nanoseconds / num_bigint::BigInt::from(1_000_000i64);
    ms_bigint.to_string().parse::<f64>().unwrap_or(f64::NAN)
}

fn temporal_to_epoch_ms(td: &TemporalData) -> Result<f64, JsValue> {
    match td {
        TemporalData::Instant { epoch_nanoseconds } => {
            Ok(bigint_to_epoch_ms(epoch_nanoseconds))
        }
        TemporalData::ZonedDateTime { epoch_nanoseconds, .. } => {
            Ok(bigint_to_epoch_ms(epoch_nanoseconds))
        }
        TemporalData::PlainDate { iso_year, iso_month, iso_day, .. } => {
            let ms = date_fields_to_epoch_ms(*iso_year, *iso_month, *iso_day, 0, 0, 0, 0);
            Ok(ms)
        }
        TemporalData::PlainTime { hour, minute, second, millisecond, .. } => {
            // Use epoch date 1970-01-01
            let ms = date_fields_to_epoch_ms(1970, 1, 1, *hour as u8, *minute as u8, *second as u8, *millisecond as u16);
            Ok(ms)
        }
        TemporalData::PlainDateTime { iso_year, iso_month, iso_day, hour, minute, second, millisecond, .. } => {
            let ms = date_fields_to_epoch_ms(*iso_year, *iso_month, *iso_day, *hour, *minute, *second, *millisecond);
            Ok(ms)
        }
        TemporalData::PlainYearMonth { iso_year, iso_month, reference_iso_day, .. } => {
            let ms = date_fields_to_epoch_ms(*iso_year, *iso_month, *reference_iso_day, 0, 0, 0, 0);
            Ok(ms)
        }
        TemporalData::PlainMonthDay { iso_month, iso_day, reference_iso_year, .. } => {
            let ms = date_fields_to_epoch_ms(*reference_iso_year, *iso_month, *iso_day, 0, 0, 0, 0);
            Ok(ms)
        }
        TemporalData::Duration { .. } => {
            Err(JsValue::Undefined) // Duration is not a date type
        }
    }
}

fn date_fields_to_epoch_ms(year: i32, month: u8, day: u8, hour: u8, minute: u8, second: u8, millisecond: u16) -> f64 {
    // Compute UTC epoch milliseconds from ISO date/time fields
    let y = year as f64;
    let m = month as f64;
    let d = day as f64;
    // Using the same algorithm as Date.UTC
    let ym = y + (m - 1.0).div_euclid(12.0).floor();
    let mn = ((m - 1.0) % 12.0 + 12.0) % 12.0;
    // Days from epoch to year
    let yd = 365.0 * (ym - 1970.0) + ((ym - 1969.0) / 4.0).floor()
        - ((ym - 1901.0) / 100.0).floor()
        + ((ym - 1601.0) / 400.0).floor();
    // Days in months for the target month (cumulative)
    let month_days: [f64; 12] = [0.0, 31.0, 59.0, 90.0, 120.0, 151.0, 181.0, 212.0, 243.0, 273.0, 304.0, 334.0];
    let md = month_days[mn as usize];
    // Leap day adjustment
    let leap = if mn >= 2.0 && (ym % 4.0 == 0.0 && (ym % 100.0 != 0.0 || ym % 400.0 == 0.0)) {
        1.0
    } else {
        0.0
    };
    let total_days = yd + md + leap + (d - 1.0);
    total_days * 86_400_000.0
        + hour as f64 * 3_600_000.0
        + minute as f64 * 60_000.0
        + second as f64 * 1_000.0
        + millisecond as f64
}

impl Interpreter {
    pub(crate) fn setup_intl_date_time_format(
        &mut self,
        intl_obj: &Rc<RefCell<JsObjectData>>,
    ) {
        let proto = self.create_object();
        if let Some(ref op) = self.object_prototype {
            proto.borrow_mut().prototype = Some(op.clone());
        }
        proto.borrow_mut().class_name = "Intl.DateTimeFormat".to_string();

        // @@toStringTag
        proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("Intl.DateTimeFormat"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            },
        );

        // format getter (returns a bound function, like Collator.compare)
        let format_getter = self.create_function(JsFunction::native(
            "get format".to_string(),
            0,
            |interp, this, _args| {
                if let JsValue::Object(o) = this {
                    if let Some(obj) = interp.get_object(o.id) {
                        let cached = {
                            let b = obj.borrow();
                            if !matches!(b.intl_data, Some(IntlData::DateTimeFormat { .. })) {
                                return Completion::Throw(interp.create_type_error(
                                    "Intl.DateTimeFormat.prototype.format called on incompatible receiver",
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

                    let opts = {
                        if let Some(obj) = interp.get_object(o.id) {
                            let b = obj.borrow();
                            if let Some(IntlData::DateTimeFormat {
                                ref locale,
                                ref calendar,
                                ref numbering_system,
                                ref time_zone,
                                ref hour_cycle,
                                ref hour12,
                                ref weekday,
                                ref era,
                                ref year,
                                ref month,
                                ref day,
                                ref day_period,
                                ref hour,
                                ref minute,
                                ref second,
                                ref fractional_second_digits,
                                ref time_zone_name,
                                ref date_style,
                                ref time_style,
                                has_explicit_components,
                            }) = b.intl_data
                            {
                                DtfOptions {
                                    locale: locale.clone(),
                                    calendar: calendar.clone(),
                                    numbering_system: numbering_system.clone(),
                                    time_zone: time_zone.clone(),
                                    hour_cycle: hour_cycle.clone(),
                                    hour12: *hour12,
                                    weekday: weekday.clone(),
                                    era: era.clone(),
                                    year: year.clone(),
                                    month: month.clone(),
                                    day: day.clone(),
                                    day_period: day_period.clone(),
                                    hour: hour.clone(),
                                    minute: minute.clone(),
                                    second: second.clone(),
                                    fractional_second_digits: *fractional_second_digits,
                                    time_zone_name: time_zone_name.clone(),
                                    date_style: date_style.clone(),
                                    time_style: time_style.clone(),
                                    has_explicit_components,
                                    temporal_type: None,
                                }
                            } else {
                                return Completion::Throw(interp.create_type_error(
                                    "Intl.DateTimeFormat.prototype.format called on incompatible receiver",
                                ));
                            }
                        } else {
                            return Completion::Throw(interp.create_type_error(
                                "Intl.DateTimeFormat.prototype.format called on incompatible receiver",
                            ));
                        }
                    };

                    let format_fn = interp.create_function(JsFunction::native(
                        "".to_string(),
                        1,
                        move |interp2, _this2, args2| {
                            let date_arg =
                                args2.first().cloned().unwrap_or(JsValue::Undefined);
                            let temporal_type = detect_temporal_type(interp2, &date_arg);
                            if matches!(temporal_type, Some(TemporalType::ZonedDateTime)) {
                                return Completion::Throw(interp2.create_type_error(
                                    "Temporal.ZonedDateTime is not supported in DateTimeFormat format()",
                                ));
                            }
                            if let Some(tt) = temporal_type {
                                if !check_temporal_overlap(&opts, tt) {
                                    return Completion::Throw(interp2.create_type_error(
                                        "Temporal object does not overlap with DateTimeFormat options",
                                    ));
                                }
                            }
                            let ms = match resolve_date_value(interp2, &date_arg) {
                                Ok(v) => v,
                                Err(e) => return Completion::Throw(e),
                            };
                            let effective_opts = if let Some(tt) = temporal_type {
                                adjust_opts_for_temporal(&opts, tt)
                            } else {
                                opts.clone()
                            };
                            let ms = if let Some(tt) = temporal_type {
                                adjust_plain_temporal_ms(ms, &effective_opts.time_zone, tt)
                            } else {
                                ms
                            };
                            let result = format_with_options(ms, &effective_opts);
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
                Completion::Throw(interp.create_type_error(
                    "Intl.DateTimeFormat.prototype.format called on incompatible receiver",
                ))
            },
        ));
        proto.borrow_mut().insert_property(
            "format".to_string(),
            PropertyDescriptor::accessor(Some(format_getter), None, false, true),
        );

        // formatToParts(date)
        let format_to_parts_fn = self.create_function(JsFunction::native(
            "formatToParts".to_string(),
            1,
            |interp, this, args| {
                let opts = match extract_dtf_data(interp, this) {
                    Ok(data) => data,
                    Err(e) => return Completion::Throw(e),
                };

                let date_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let temporal_type = detect_temporal_type(interp, &date_arg);
                if matches!(temporal_type, Some(TemporalType::ZonedDateTime)) {
                    return Completion::Throw(interp.create_type_error(
                        "Temporal.ZonedDateTime is not supported in DateTimeFormat formatToParts()",
                    ));
                }
                if let Some(tt) = temporal_type {
                    if !check_temporal_overlap(&opts, tt) {
                        return Completion::Throw(interp.create_type_error(
                            "Temporal object does not overlap with DateTimeFormat options",
                        ));
                    }
                }
                let ms = match resolve_date_value(interp, &date_arg) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let effective_opts = if let Some(tt) = temporal_type {
                    adjust_opts_for_temporal(&opts, tt)
                } else {
                    opts
                };
                let ms = if let Some(tt) = temporal_type {
                    adjust_plain_temporal_ms(ms, &effective_opts.time_zone, tt)
                } else {
                    ms
                };

                let parts = format_to_parts_with_options(ms, &effective_opts);

                let js_parts: Vec<JsValue> = parts
                    .into_iter()
                    .map(|(ptype, value)| {
                        let part_obj = interp.create_object();
                        if let Some(ref op) = interp.object_prototype {
                            part_obj.borrow_mut().prototype = Some(op.clone());
                        }
                        part_obj.borrow_mut().insert_property(
                            "type".to_string(),
                            PropertyDescriptor::data(
                                JsValue::String(JsString::from_str(&ptype)),
                                true,
                                true,
                                true,
                            ),
                        );
                        part_obj.borrow_mut().insert_property(
                            "value".to_string(),
                            PropertyDescriptor::data(
                                JsValue::String(JsString::from_str(&value)),
                                true,
                                true,
                                true,
                            ),
                        );
                        let id = part_obj.borrow().id.unwrap();
                        JsValue::Object(crate::types::JsObject { id })
                    })
                    .collect();

                Completion::Normal(interp.create_array(js_parts))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("formatToParts".to_string(), format_to_parts_fn);

        // formatRange(startDate, endDate)
        let format_range_fn = self.create_function(JsFunction::native(
            "formatRange".to_string(),
            2,
            |interp, this, args| {
                let opts = match extract_dtf_data(interp, this) {
                    Ok(data) => data,
                    Err(e) => return Completion::Throw(e),
                };

                let start_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let end_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);

                if matches!(start_arg, JsValue::Undefined) || matches!(end_arg, JsValue::Undefined) {
                    return Completion::Throw(
                        interp.create_type_error("startDate and endDate are required"),
                    );
                }

                let start_tt = detect_temporal_type(interp, &start_arg);
                let end_tt = detect_temporal_type(interp, &end_arg);
                // Per spec: ToNumberToDateTimeFormattable calls ToNumber for
                // non-Temporal args before checking SameTemporalType.
                // Only call ToNumber here (not TimeClip) since the type
                // mismatch check must happen before HandleDateTimeOthers.
                if start_tt.is_none() {
                    if let Err(e) = interp.to_number_value(&start_arg) {
                        return Completion::Throw(e);
                    }
                }
                if end_tt.is_none() {
                    if let Err(e) = interp.to_number_value(&end_arg) {
                        return Completion::Throw(e);
                    }
                }
                if matches!(start_tt, Some(TemporalType::ZonedDateTime))
                    || matches!(end_tt, Some(TemporalType::ZonedDateTime))
                {
                    return Completion::Throw(interp.create_type_error(
                        "Temporal.ZonedDateTime is not supported in DateTimeFormat formatRange()",
                    ));
                }
                // Both args must be same type (both Temporal of same kind, or both non-Temporal)
                match (start_tt, end_tt) {
                    (Some(stt), Some(ett)) => {
                        if std::mem::discriminant(&stt) != std::mem::discriminant(&ett) {
                            return Completion::Throw(interp.create_type_error(
                                "formatRange requires both arguments to be the same type",
                            ));
                        }
                    }
                    (Some(_), None) | (None, Some(_)) => {
                        return Completion::Throw(interp.create_type_error(
                            "formatRange requires both arguments to be the same type",
                        ));
                    }
                    (None, None) => {}
                }
                if let Some(tt) = start_tt.or(end_tt) {
                    if !check_temporal_overlap(&opts, tt) {
                        return Completion::Throw(interp.create_type_error(
                            "Temporal object does not overlap with DateTimeFormat options",
                        ));
                    }
                }
                let start_ms = match resolve_date_value(interp, &start_arg) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let end_ms = match resolve_date_value(interp, &end_arg) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let effective_opts = if let Some(tt) = start_tt {
                    adjust_opts_for_temporal(&opts, tt)
                } else {
                    opts
                };
                let start_ms = if let Some(tt) = start_tt {
                    adjust_plain_temporal_ms(start_ms, &effective_opts.time_zone, tt)
                } else {
                    start_ms
                };
                let end_ms = if let Some(tt) = end_tt {
                    adjust_plain_temporal_ms(end_ms, &effective_opts.time_zone, tt)
                } else {
                    end_ms
                };
                let result = format_range_with_options(start_ms, end_ms, &effective_opts);
                Completion::Normal(JsValue::String(JsString::from_str(&result)))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("formatRange".to_string(), format_range_fn);

        // formatRangeToParts(startDate, endDate)
        let format_range_to_parts_fn = self.create_function(JsFunction::native(
            "formatRangeToParts".to_string(),
            2,
            |interp, this, args| {
                let opts = match extract_dtf_data(interp, this) {
                    Ok(data) => data,
                    Err(e) => return Completion::Throw(e),
                };

                let start_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let end_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);

                if matches!(start_arg, JsValue::Undefined) || matches!(end_arg, JsValue::Undefined) {
                    return Completion::Throw(
                        interp.create_type_error("startDate and endDate are required"),
                    );
                }

                let start_tt = detect_temporal_type(interp, &start_arg);
                let end_tt = detect_temporal_type(interp, &end_arg);
                if start_tt.is_none() {
                    if let Err(e) = interp.to_number_value(&start_arg) {
                        return Completion::Throw(e);
                    }
                }
                if end_tt.is_none() {
                    if let Err(e) = interp.to_number_value(&end_arg) {
                        return Completion::Throw(e);
                    }
                }
                if matches!(start_tt, Some(TemporalType::ZonedDateTime))
                    || matches!(end_tt, Some(TemporalType::ZonedDateTime))
                {
                    return Completion::Throw(interp.create_type_error(
                        "Temporal.ZonedDateTime is not supported in DateTimeFormat formatRangeToParts()",
                    ));
                }
                match (start_tt, end_tt) {
                    (Some(stt), Some(ett)) => {
                        if std::mem::discriminant(&stt) != std::mem::discriminant(&ett) {
                            return Completion::Throw(interp.create_type_error(
                                "formatRangeToParts requires both arguments to be the same type",
                            ));
                        }
                    }
                    (Some(_), None) | (None, Some(_)) => {
                        return Completion::Throw(interp.create_type_error(
                            "formatRangeToParts requires both arguments to be the same type",
                        ));
                    }
                    (None, None) => {}
                }
                if let Some(tt) = start_tt.or(end_tt) {
                    if !check_temporal_overlap(&opts, tt) {
                        return Completion::Throw(interp.create_type_error(
                            "Temporal object does not overlap with DateTimeFormat options",
                        ));
                    }
                }
                let start_ms = match resolve_date_value(interp, &start_arg) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let end_ms = match resolve_date_value(interp, &end_arg) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let effective_opts = if let Some(tt) = start_tt {
                    adjust_opts_for_temporal(&opts, tt)
                } else {
                    opts
                };
                let start_ms = if let Some(tt) = start_tt {
                    adjust_plain_temporal_ms(start_ms, &effective_opts.time_zone, tt)
                } else {
                    start_ms
                };
                let end_ms = if let Some(tt) = end_tt {
                    adjust_plain_temporal_ms(end_ms, &effective_opts.time_zone, tt)
                } else {
                    end_ms
                };
                let all_parts = format_range_to_parts_with_options(start_ms, end_ms, &effective_opts);

                let js_parts: Vec<JsValue> = all_parts
                    .into_iter()
                    .map(|(ptype, value, source)| {
                        let part_obj = interp.create_object();
                        if let Some(ref op) = interp.object_prototype {
                            part_obj.borrow_mut().prototype = Some(op.clone());
                        }
                        part_obj.borrow_mut().insert_property(
                            "type".to_string(),
                            PropertyDescriptor::data(
                                JsValue::String(JsString::from_str(&ptype)),
                                true,
                                true,
                                true,
                            ),
                        );
                        part_obj.borrow_mut().insert_property(
                            "value".to_string(),
                            PropertyDescriptor::data(
                                JsValue::String(JsString::from_str(&value)),
                                true,
                                true,
                                true,
                            ),
                        );
                        part_obj.borrow_mut().insert_property(
                            "source".to_string(),
                            PropertyDescriptor::data(
                                JsValue::String(JsString::from_str(&source)),
                                true,
                                true,
                                true,
                            ),
                        );
                        let id = part_obj.borrow().id.unwrap();
                        JsValue::Object(crate::types::JsObject { id })
                    })
                    .collect();

                Completion::Normal(interp.create_array(js_parts))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("formatRangeToParts".to_string(), format_range_to_parts_fn);

        // resolvedOptions()
        let resolved_fn = self.create_function(JsFunction::native(
            "resolvedOptions".to_string(),
            0,
            |interp, this, _args| {
                let opts = match extract_dtf_data(interp, this) {
                    Ok(data) => data,
                    Err(e) => return Completion::Throw(e),
                };

                let result = interp.create_object();
                if let Some(ref op) = interp.object_prototype {
                    result.borrow_mut().prototype = Some(op.clone());
                }

                // Properties in spec order
                let mut props: Vec<(&str, JsValue)> = Vec::new();
                props.push(("locale", JsValue::String(JsString::from_str(&opts.locale))));
                props.push(("calendar", JsValue::String(JsString::from_str(&opts.calendar))));
                props.push((
                    "numberingSystem",
                    JsValue::String(JsString::from_str(&opts.numbering_system)),
                ));
                props.push((
                    "timeZone",
                    JsValue::String(JsString::from_str(&opts.time_zone)),
                ));

                // hourCycle and hour12 only present if hour is used
                let has_hour = opts.hour.is_some()
                    || opts.time_style.is_some();
                if has_hour {
                    let hc = resolve_hour_cycle(&opts);
                    props.push((
                        "hourCycle",
                        JsValue::String(JsString::from_str(hc)),
                    ));
                    let h12 = hc == "h12" || hc == "h11";
                    props.push(("hour12", JsValue::Boolean(h12)));
                }

                if let Some(ref v) = opts.weekday {
                    props.push(("weekday", JsValue::String(JsString::from_str(v))));
                }
                if let Some(ref v) = opts.era {
                    props.push(("era", JsValue::String(JsString::from_str(v))));
                }
                if let Some(ref v) = opts.year {
                    props.push(("year", JsValue::String(JsString::from_str(v))));
                }
                if let Some(ref v) = opts.month {
                    props.push(("month", JsValue::String(JsString::from_str(v))));
                }
                if let Some(ref v) = opts.day {
                    props.push(("day", JsValue::String(JsString::from_str(v))));
                }
                if let Some(ref v) = opts.day_period {
                    props.push(("dayPeriod", JsValue::String(JsString::from_str(v))));
                }
                if let Some(ref v) = opts.hour {
                    props.push(("hour", JsValue::String(JsString::from_str(v))));
                }
                if let Some(ref v) = opts.minute {
                    props.push(("minute", JsValue::String(JsString::from_str(v))));
                }
                if let Some(ref v) = opts.second {
                    props.push(("second", JsValue::String(JsString::from_str(v))));
                }
                if let Some(v) = opts.fractional_second_digits {
                    props.push((
                        "fractionalSecondDigits",
                        JsValue::Number(v as f64),
                    ));
                }
                if let Some(ref v) = opts.time_zone_name {
                    props.push(("timeZoneName", JsValue::String(JsString::from_str(v))));
                }
                if let Some(ref v) = opts.date_style {
                    props.push(("dateStyle", JsValue::String(JsString::from_str(v))));
                }
                if let Some(ref v) = opts.time_style {
                    props.push(("timeStyle", JsValue::String(JsString::from_str(v))));
                }

                for (key, val) in props {
                    result.borrow_mut().insert_property(
                        key.to_string(),
                        PropertyDescriptor::data(val, true, true, true),
                    );
                }

                let result_id = result.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id: result_id }))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("resolvedOptions".to_string(), resolved_fn);

        self.intl_date_time_format_prototype = Some(proto.clone());

        // --- Constructor ---
        let proto_id = proto.borrow().id.unwrap();
        let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });
        let proto_clone = proto.clone();

        let dtf_ctor = self.create_function(JsFunction::constructor(
            "DateTimeFormat".to_string(),
            0,
            move |interp, _this, args| {
                let locales_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let options_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);

                let requested = match interp.intl_canonicalize_locale_list(&locales_arg) {
                    Ok(list) => list,
                    Err(e) => return Completion::Throw(e),
                };

                // Step 3: CoerceOptionsToObject (not GetOptionsObject)
                let options = match interp.intl_coerce_options_to_object(&options_arg) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // Step 4: localeMatcher
                let _locale_matcher = match interp.intl_get_option(
                    &options,
                    "localeMatcher",
                    &["lookup", "best fit"],
                    Some("best fit"),
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // Steps 5-7: calendar
                let calendar_opt = match interp.intl_get_option(&options, "calendar", &[], None) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let calendar_opt_provided = calendar_opt.is_some();
                if let Some(ref cal) = calendar_opt {
                    if !is_valid_unicode_type(cal) {
                        return Completion::Throw(interp.create_range_error(&format!(
                            "Invalid calendar value: {}",
                            cal
                        )));
                    }
                }

                // Steps 8-10: numberingSystem
                let ns_opt =
                    match interp.intl_get_option(&options, "numberingSystem", &[], None) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };
                let ns_opt_provided = ns_opt.is_some();
                if let Some(ref ns) = ns_opt {
                    if !is_valid_unicode_type(ns) {
                        return Completion::Throw(interp.create_range_error(&format!(
                            "Invalid numberingSystem value: {}",
                            ns
                        )));
                    }
                }

                // Step 12: hour12
                let hour12_raw = if let JsValue::Object(o) = &options {
                    match interp.get_object_property(o.id, "hour12", &options) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => JsValue::Undefined,
                    }
                } else {
                    JsValue::Undefined
                };
                let hour12 = if matches!(hour12_raw, JsValue::Undefined) {
                    None
                } else {
                    Some(crate::interpreter::helpers::to_boolean(&hour12_raw))
                };

                // Step 13: hourCycle
                let hour_cycle_opt = match interp.intl_get_option(
                    &options,
                    "hourCycle",
                    &["h11", "h12", "h23", "h24"],
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let hour_cycle_opt_provided = hour_cycle_opt.is_some();

                // Step 29: timeZone
                let tz_opt =
                    match interp.intl_get_option(&options, "timeZone", &[], None) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };
                let time_zone = if let Some(tz) = tz_opt {
                    if !is_valid_timezone(&tz) {
                        return Completion::Throw(interp.create_range_error(&format!(
                            "Invalid time zone specified: {}",
                            tz
                        )));
                    }
                    if let Some(normalized) = normalize_offset_timezone(&tz) {
                        normalized
                    } else {
                        canonicalize_timezone(&tz)
                    }
                } else {
                    "UTC".to_string()
                };

                // Step 36: Table 7 component options (in table order)
                let weekday = match interp.intl_get_option(
                    &options,
                    "weekday",
                    &["narrow", "short", "long"],
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let era = match interp.intl_get_option(
                    &options,
                    "era",
                    &["narrow", "short", "long"],
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let year_opt = match interp.intl_get_option(
                    &options,
                    "year",
                    &["numeric", "2-digit"],
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let month_opt = match interp.intl_get_option(
                    &options,
                    "month",
                    &["numeric", "2-digit", "narrow", "short", "long"],
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let day_opt = match interp.intl_get_option(
                    &options,
                    "day",
                    &["numeric", "2-digit"],
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let day_period = match interp.intl_get_option(
                    &options,
                    "dayPeriod",
                    &["narrow", "short", "long"],
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let hour_opt = match interp.intl_get_option(
                    &options,
                    "hour",
                    &["numeric", "2-digit"],
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let minute_opt = match interp.intl_get_option(
                    &options,
                    "minute",
                    &["numeric", "2-digit"],
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let second_opt = match interp.intl_get_option(
                    &options,
                    "second",
                    &["numeric", "2-digit"],
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let fsd_opt = match interp.intl_get_number_option(
                    &options,
                    "fractionalSecondDigits",
                    1.0,
                    3.0,
                    None,
                ) {
                    Ok(v) => v.map(|n| n as u32),
                    Err(e) => return Completion::Throw(e),
                };

                let tz_name = match interp.intl_get_option(
                    &options,
                    "timeZoneName",
                    &[
                        "short",
                        "long",
                        "shortOffset",
                        "longOffset",
                        "shortGeneric",
                        "longGeneric",
                    ],
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // Step 37: formatMatcher (after Table 7 options)
                let _format_matcher = match interp.intl_get_option(
                    &options,
                    "formatMatcher",
                    &["basic", "best fit"],
                    Some("best fit"),
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // Step 38: dateStyle (after formatMatcher)
                let date_style = match interp.intl_get_option(
                    &options,
                    "dateStyle",
                    &["full", "long", "medium", "short"],
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // Step 40: timeStyle (after dateStyle)
                let time_style = match interp.intl_get_option(
                    &options,
                    "timeStyle",
                    &["full", "long", "medium", "short"],
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // Step 41: dateStyle/timeStyle conflict with component options
                let has_style = date_style.is_some() || time_style.is_some();
                let has_date_time_component = weekday.is_some()
                    || year_opt.is_some()
                    || month_opt.is_some()
                    || day_opt.is_some()
                    || day_period.is_some()
                    || hour_opt.is_some()
                    || minute_opt.is_some()
                    || second_opt.is_some()
                    || fsd_opt.is_some();
                let has_component = has_date_time_component || era.is_some() || tz_name.is_some();

                if has_style && has_component {
                    return Completion::Throw(interp.create_type_error(
                        "Can't set option when dateStyle or timeStyle is used",
                    ));
                }

                // Default behavior: if no date/time components and no style, default to date
                // Per spec, timeZoneName alone does NOT prevent defaults
                let (year, month, day) = if !has_style
                    && !has_date_time_component
                {
                    (
                        Some("numeric".to_string()),
                        Some("numeric".to_string()),
                        Some("numeric".to_string()),
                    )
                } else {
                    (year_opt, month_opt, day_opt)
                };

                let resolved_locale = interp.intl_resolve_locale(&requested);

                // Extract unicode extension values from locale
                let locale_hc = extract_unicode_extension(&resolved_locale, "hc");
                let locale_ca = extract_unicode_extension(&resolved_locale, "ca")
                    .map(|c| canonicalize_calendar(&c));
                let locale_nu = extract_unicode_extension(&resolved_locale, "nu")
                    .map(|n| n.to_ascii_lowercase());

                // Calendar: option > locale extension > default
                // Both option and extension must be supported to be used
                let calendar_opt_canonical = calendar_opt.map(|c| canonicalize_calendar(&c));
                let calendar_raw = if let Some(ref co) = calendar_opt_canonical {
                    if is_supported_calendar(co) {
                        co.clone()
                    } else if let Some(ref lc) = locale_ca {
                        if is_supported_calendar(lc) { lc.clone() } else { "gregory".to_string() }
                    } else {
                        "gregory".to_string()
                    }
                } else if let Some(ref lc) = locale_ca {
                    if is_supported_calendar(lc) { lc.clone() } else { "gregory".to_string() }
                } else {
                    "gregory".to_string()
                };
                let calendar = calendar_raw;

                // NumberingSystem: option > locale extension > default
                let ns_opt_lower = ns_opt.map(|n| n.to_ascii_lowercase());
                let numbering_system = if let Some(ref no) = ns_opt_lower {
                    if is_supported_numbering_system(no) {
                        no.clone()
                    } else if let Some(ref ln) = locale_nu {
                        if is_supported_numbering_system(ln) { ln.clone() } else { "latn".to_string() }
                    } else {
                        "latn".to_string()
                    }
                } else if let Some(ref ln) = locale_nu {
                    if is_supported_numbering_system(ln) { ln.clone() } else { "latn".to_string() }
                } else {
                    "latn".to_string()
                };

                // hourCycle resolution: option > locale extension > locale default
                let hour_cycle = if hour12.is_some() {
                    None // hour12 takes precedence, resolved at format time
                } else if hour_cycle_opt.is_some() {
                    hour_cycle_opt
                } else {
                    locale_hc
                };

                // Build locale for display
                // First strip all extension keys that DTF doesn't use (cu, tz, etc.)
                let mut locale = strip_unrecognized_unicode_keys(&resolved_locale);

                // hc: strip if option/hour12 overrides
                if hour12.is_some() {
                    locale = strip_unicode_extension_key(&locale, "hc");
                } else if hour_cycle_opt_provided {
                    let ext_hc = extract_unicode_extension(&resolved_locale, "hc");
                    if ext_hc.as_ref() != hour_cycle.as_ref() {
                        locale = strip_unicode_extension_key(&locale, "hc");
                    }
                }

                // ca: keep only if the resolved calendar matches the extension value
                if let Some(ref ext_ca) = locale_ca {
                    if ext_ca != &calendar {
                        locale = strip_unicode_extension_key(&locale, "ca");
                    }
                }

                // nu: keep only if the resolved numbering system matches the extension value
                if let Some(ref ext_nu) = locale_nu {
                    if ext_nu != &numbering_system {
                        locale = strip_unicode_extension_key(&locale, "nu");
                    }
                }

                let has_era = era.is_some();
                let obj = interp.create_object();
                obj.borrow_mut().prototype = Some(proto_clone.clone());
                obj.borrow_mut().class_name = "Intl.DateTimeFormat".to_string();
                obj.borrow_mut().intl_data = Some(IntlData::DateTimeFormat {
                    locale,
                    calendar,
                    numbering_system,
                    time_zone,
                    hour_cycle,
                    hour12,
                    weekday,
                    era,
                    year,
                    month,
                    day,
                    day_period,
                    hour: hour_opt,
                    minute: minute_opt,
                    second: second_opt,
                    fractional_second_digits: fsd_opt,
                    time_zone_name: tz_name,
                    date_style,
                    time_style,
                    has_explicit_components: has_date_time_component || has_style,
                });

                let obj_id = obj.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id: obj_id }))
            },
        ));

        // Set DateTimeFormat.prototype on constructor
        if let JsValue::Object(ctor_ref) = &dtf_ctor {
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
            PropertyDescriptor::data(dtf_ctor.clone(), true, false, true),
        );

        // Save built-in constructor for internal use (e.g. Date.toLocaleString)
        self.intl_date_time_format_ctor = Some(dtf_ctor.clone());

        // Register Intl.DateTimeFormat on the Intl namespace
        intl_obj.borrow_mut().insert_property(
            "DateTimeFormat".to_string(),
            PropertyDescriptor::data(dtf_ctor, true, false, true),
        );
    }
}
