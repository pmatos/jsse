use std::process::Command;

#[test]
fn tz_environment_controls_the_system_time_zone_and_date_offsets() {
    let output = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .env("TZ", "America/New_York")
        .args([
            "-e",
            r#"
console.log(new Intl.DateTimeFormat().resolvedOptions().timeZone);
console.log(Temporal.Now.timeZoneId());
console.log(new Date("2024-01-15T12:00:00Z").getTimezoneOffset());
console.log(new Date("2024-07-15T12:00:00Z").getTimezoneOffset());
console.log(new Date(2024, 0, 15, 12).toISOString());
console.log(new Date(2024, 2, 10, 2, 30).toISOString());
console.log(new Date(2024, 10, 3, 1, 30).toISOString());
console.log((new Date(2023, 4, 6) - new Date(2023, 0, 1)) / 3600000);
"#,
        ])
        .output()
        .expect("failed to run jsse");

    assert!(
        output.status.success(),
        "jsse failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("jsse stdout was not UTF-8"),
        concat!(
            "America/New_York\n",
            "America/New_York\n",
            "300\n",
            "240\n",
            "2024-01-15T17:00:00.000Z\n",
            "2024-03-10T07:30:00.000Z\n",
            "2024-11-03T05:30:00.000Z\n",
            "2999\n",
        )
    );
}
