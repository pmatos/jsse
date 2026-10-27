//! The vendored web-platform-tests URL data (`scripts/url-fixtures/wpt`, #1230)
//! replayed on `jsse --node`. The Node cross-check is
//! `scripts/run-node-url-wpt.sh`; this pins the output it expects.

use std::path::Path;
use std::process::Command;

fn run_wpt() -> (String, String, bool) {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/url-fixtures/wpt/run.js");
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .arg("--node")
        .arg(script)
        .output()
        .expect("run jsse");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

#[test]
fn wpt_url_data_matches_the_pinned_output() {
    let want = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/url-fixtures/wpt/expected.output"),
    )
    .unwrap();
    let (stdout, stderr, ok) = run_wpt();
    assert!(ok, "{stderr}");
    assert_eq!(stderr, "");
    assert_eq!(stdout, want);
}

#[test]
fn idna_follows_the_unicode_data_where_ada_lags() {
    let probe = r#"print(["aࡰb", "a؝b", "᫁b", "a᫁", "ࡰ"].map((s) => {
      try { return new URL("https://" + s + "/").hostname; } catch (e) { return e.code; }
    }).join())"#;
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(["--node", "-e", probe])
        .output()
        .expect("run jsse");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        "ERR_INVALID_URL,ERR_INVALID_URL,ERR_INVALID_URL,xn--a-d8k,xn--cxb"
    );
}
