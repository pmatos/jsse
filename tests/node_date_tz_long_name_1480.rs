//! Node host mode prints the CLDR long time zone name in `Date.prototype.toString`
//! and `toTimeString` like V8 (#1480), while the default environment keeps the
//! zone abbreviation. TimeZoneString's text is implementation-defined, so only
//! the host fidelity differs. Expected strings come from Node 26 with the same `TZ`.
use std::process::Command;

fn run(tz: &str, node: bool, source: &str) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_jsse"));
    command.env("TZ", tz);
    if node {
        command.arg("--node");
    }
    let output = command.args(["-e", source]).output().expect("run jsse");
    assert!(
        output.status.success(),
        "jsse failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8 stdout")
}

const PROBE: &str = r#"
console.log(String(new Date(0)));
console.log(new Date(0).toTimeString());
console.log(new Date(1e12).toString());
console.log(Date().slice(-1) === ")");
console.log(new Date(NaN).toString());
console.log(new Date(0).toUTCString());
"#;

fn expect(tz: &str, first: &str, summer: &str) {
    let out = run(tz, true, PROBE);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], first, "{tz} toString");
    assert_eq!(lines[1], &first[16..], "{tz} toTimeString");
    assert_eq!(lines[2], summer, "{tz} summer toString");
    assert_eq!(lines[3], "true");
    assert_eq!(lines[4], "Invalid Date");
    assert_eq!(lines[5], "Thu, 01 Jan 1970 00:00:00 GMT");
}

#[test]
fn berlin_long_names() {
    expect(
        "Europe/Berlin",
        "Thu Jan 01 1970 01:00:00 GMT+0100 (Central European Standard Time)",
        "Sun Sep 09 2001 03:46:40 GMT+0200 (Central European Summer Time)",
    );
}

#[test]
fn new_york_long_names() {
    expect(
        "America/New_York",
        "Wed Dec 31 1969 19:00:00 GMT-0500 (Eastern Standard Time)",
        "Sat Sep 08 2001 21:46:40 GMT-0400 (Eastern Daylight Time)",
    );
}

#[test]
fn london_names_follow_the_daylight_flag_not_the_era() {
    expect(
        "Europe/London",
        "Thu Jan 01 1970 01:00:00 GMT+0100 (Greenwich Mean Time)",
        "Sun Sep 09 2001 02:46:40 GMT+0100 (British Summer Time)",
    );
}

#[test]
fn utc_and_half_hour_zones() {
    expect(
        "UTC",
        "Thu Jan 01 1970 00:00:00 GMT+0000 (Coordinated Universal Time)",
        "Sun Sep 09 2001 01:46:40 GMT+0000 (Coordinated Universal Time)",
    );
    expect(
        "Asia/Kolkata",
        "Thu Jan 01 1970 05:30:00 GMT+0530 (India Standard Time)",
        "Sun Sep 09 2001 07:16:40 GMT+0530 (India Standard Time)",
    );
}

#[test]
fn default_environment_keeps_the_abbreviation() {
    let out = run("Europe/Berlin", false, "console.log(String(new Date(0)))");
    assert_eq!(out.trim_end(), "Thu Jan 01 1970 01:00:00 GMT+0100 (CET)");
    let out = run(
        "America/New_York",
        false,
        "console.log(new Date(0).toTimeString())",
    );
    assert_eq!(out.trim_end(), "19:00:00 GMT-0500 (EST)");
}

#[test]
fn intl_zone_name_after_a_date_has_the_cldr_separator_in_both_modes() {
    let source = r#"console.log(new Intl.DateTimeFormat("en", {timeZoneName: "long"}).format(0))"#;
    for node in [false, true] {
        assert_eq!(
            run("Europe/Berlin", node, source).trim_end(),
            "1/1/1970, Central European Standard Time"
        );
    }
}
