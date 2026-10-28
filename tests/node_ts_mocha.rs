//! What mocha and the TypeScript test runner (#1255) needed from the `--node`
//! host: stack traces that keep the frame of a function whose last act is a
//! call (V8 has no proper tail calls, so a failing `it(() => assert...)` must
//! show the test's own frame), while the default environment keeps ES2015
//! tail calls. The mocha CLI and `built/local/run.js` against Node is
//! `scripts/run-ts-mocha-oracle.py` (the runner's command line, sharding and
//! `.failed-tests` flow are its `cli-*` cases); the case below only drives it
//! when TS_SRC_DIR points at a built TypeScript 6.0.3 checkout.

use std::path::Path;
use std::process::{Command, Output};

fn jsse(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(args)
        .output()
        .expect("run jsse")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

const TAIL_CALLS: &str = r#""use strict";
function frames() { return new Error("x").stack.split("\n").length; }
function viaReturn() { return frames(); }
const viaArrow = () => frames();
function viaTernary(c) { return c ? frames() : 0; }
function viaLogical() { return 1 && frames(); }
function viaSequence() { return (0, frames()); }
function viaSpread(...a) { return frames(...a); }
function viaMethod() { return { m: frames }.m(); }
const direct = frames();
console.log([viaReturn(), viaArrow(), viaTernary(true), viaLogical(), viaSequence(), viaSpread(), viaMethod()]
  .map(n => n - direct).join(" "));
function depth(n) { return n === 0 ? 0 : depth(n - 1); }
const arrow = n => n === 0 ? 0 : arrow(n - 1);
for (const f of [depth, arrow]) {
  try { f(1e6); console.log("returned"); } catch (e) { console.log(e instanceof RangeError); }
}
"#;

#[test]
fn strict_tail_calls_keep_their_frames_under_node() {
    let out = jsse(&["--node", "-e", TAIL_CALLS]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(stdout(&out), "1 1 1 1 1 1 1\ntrue\ntrue");
}

#[test]
fn strict_tail_calls_still_run_in_constant_space_by_default() {
    let out = jsse(&[
        "-e",
        r#""use strict";
function depth(n) { return n === 0 ? "done" : depth(n - 1); }
const arrow = n => n === 0 ? "done" : arrow(n - 1);
print(depth(1e6), arrow(1e6));"#,
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(stdout(&out), "done done");
}

#[test]
fn mocha_matches_node_when_a_typescript_checkout_is_given() {
    if std::env::var_os("TS_SRC_DIR").is_none() {
        eprintln!(
            "SKIP: set TS_SRC_DIR to a built TypeScript 6.0.3 checkout to run the mocha oracle"
        );
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts/run-ts-mocha-oracle.py"))
        .args(["--ci", "--require", "--jsse", env!("CARGO_BIN_EXE_jsse")])
        .args([
            "pass-*",
            "fail-*",
            "async-*",
            "late-*",
            "exit-*",
            "ts-semver-and-failures",
            "cli-list-fourslash-server",
            "cli-tests-*",
            "cli-shards-project-*",
            "cli-failed-*",
            "cli-timeout-*",
            "cli-baseline-accept",
        ])
        .output()
        .expect("run python3");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("passed"), "{text}");
}
