//! The V8-compatible Error API under `--node` (#1228): `stack` frames,
//! `Error.captureStackTrace`, `Error.stackTraceLimit`, `Error.prepareStackTrace`
//! and CallSite objects, and their absence everywhere else. The Node-oracle
//! diff lives in `scripts/run-node-v8-error-oracle.sh`; this file replays the
//! same fixture against Node's pinned output through every execution mode.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn jsse(args: &[&str], stress: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_jsse"));
    command.args(args);
    if let Some(period) = stress {
        command.env("JSSE_GC_STRESS", period);
    }
    command.output().expect("run jsse")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim_end().to_string()
}

/// The oracle script's normalisation: the fixture directory becomes `<dir>`
/// and stack frames outside the fixture (plus its top-level frame) go.
fn normalise(text: &str, dir: &Path) -> String {
    let dir = dir.to_string_lossy();
    let text = text.replace(dir.as_ref(), "<dir>");
    let mut kept = Vec::new();
    for line in text.lines() {
        let frame = line.trim_start().starts_with("at ");
        if frame && (!line.contains("<dir>/cases.js") || line.contains("Object.<anonymous>")) {
            continue;
        }
        kept.push(line);
    }
    kept.join("\n")
}

fn replay_fixture(extra: &[&str], stress: Option<&str>) {
    let dir = root().join("scripts/v8-error-fixtures");
    let cases = dir.join("cases.js");
    let mut args = vec!["--node"];
    args.extend_from_slice(extra);
    args.push(cases.to_str().expect("utf-8 path"));
    let out = jsse(&args, stress);
    assert!(out.status.success(), "{}", stderr(&out));
    let expected = std::fs::read_to_string(dir.join("expected-cases.txt")).expect("snapshot");
    let mut actual = normalise(&stdout(&out), &dir);
    let mut expected = expected.trim_end().to_string();
    if extra.contains(&"--no-bytecode") {
        // The state machine behind async functions has no frame of its own.
        for text in [&mut actual, &mut expected] {
            *text = text
                .split("## async before await")
                .next()
                .unwrap()
                .trim_end()
                .to_string();
        }
    }
    assert_eq!(actual, expected, "output differs from Node's pinned output");
}

#[test]
fn fixture_matches_node_under_the_bytecode_vm() {
    replay_fixture(&[], None);
}

#[test]
fn fixture_matches_node_under_the_tree_walker() {
    replay_fixture(&["--no-bytecode"], None);
}

#[test]
fn fixture_matches_node_under_gc_stress() {
    replay_fixture(&[], Some("7"));
    replay_fixture(&["--no-bytecode"], Some("7"));
}

#[test]
fn default_mode_has_no_v8_error_api() {
    let out = jsse(
        &[
            "-e",
            r#"function f() { return new Error("x"); }
print([typeof Error.captureStackTrace, typeof Error.stackTraceLimit, typeof Error.prepareStackTrace,
  JSON.stringify(f().stack), JSON.stringify(Object.getOwnPropertyNames(f())),
  typeof __host_stack_install].join(" "))"#,
        ],
        None,
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        r#"undefined undefined undefined "" ["message"] undefined"#
    );
}

#[test]
fn floor_globals_are_taken_private_by_the_prelude() {
    let out = jsse(
        &[
            "--node",
            "-e",
            r#"print(["install", "capture", "frames", "locate"].map((n) => typeof globalThis["__host_stack_" + n]).join())"#,
        ],
        None,
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "undefined,undefined,undefined,undefined");
}

#[test]
fn engine_thrown_errors_carry_the_throwing_function() {
    let out = jsse(
        &[
            "--node",
            "-e",
            r#"function thrower() { null.x; }
function outer() { try { thrower(); } catch (e) { return e; } }
const lines = outer().stack.split("\n");
print(lines[0]); print(lines[1].startsWith("    at thrower (")); print(lines[2].startsWith("    at outer ("));"#,
        ],
        None,
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "TypeError: Cannot read properties of null (reading 'x')\ntrue\ntrue"
    );
}

#[test]
fn spread_and_construct_calls_record_their_position() {
    let source = "function make(...a) { return new Error('s'); }\n\
function spreadCall(list) { return make(...list); }\n\
function construct() { return new (class K { constructor(...a) { this.e = new Error('k'); } })(...[1]).e; }\n\
print(spreadCall([1]).stack.split('\\n')[2].replace(/^.*\\(/, '('));\n\
print(construct().stack.split('\\n').length);";
    for mode in [None, Some("--no-bytecode")] {
        let mut args = vec!["--node"];
        args.extend(mode);
        args.extend(["-e", source]);
        let out = jsse(&args, None);
        assert!(out.status.success(), "{}", stderr(&out));
        assert_eq!(stdout(&out), "(<anonymous>:2:36)\n3", "mode {mode:?}");
    }
}

#[test]
fn named_sources_supply_file_and_line() {
    let out = jsse(
        &[
            "--node",
            "-e",
            r#"const tagged = new Function("return new Error('t')\n//# sourceURL=/virtual/tagged.js");
const named = new Function("return new Error('n')");
__host_stack_name_source(named, "/virtual/named.js", -2);
const mention = new Function("return new Error('m') + '\\n//# sourceURL=/not/a/comment.js'");
print(tagged().stack.split("\n")[1]); print(named().stack.split("\n")[1]); print(new Function("return new Error('q')")().stack.split("\n")[1]);"#,
        ],
        None,
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "    at anonymous (/virtual/tagged.js:3:9)\n    at anonymous (/virtual/named.js:1:9)\n    at anonymous (<anonymous>:3:9)"
    );
}

#[test]
fn prepare_stack_trace_that_allocates_and_collects_survives_gc_stress() {
    let source = r#"function churn() {
  Error.prepareStackTrace = function (err, sites) {
    const junk = [];
    for (let i = 0; i < 100; i++) junk.push({ i, text: "x".repeat(20) });
    $262.gc();
    return "prepared:" + sites.length + ":" + junk.length + ":" + err.message;
  };
  const errors = [];
  for (let i = 0; i < 20; i++) errors.push(new Error("e" + i));
  return errors.map((e) => e.stack).join(",");
}
print(churn().split(",").length, churn().slice(0, 16));"#;
    for stress in [None, Some("3")] {
        let out = jsse(&["--node", "-e", source], stress);
        assert!(out.status.success(), "{}", stderr(&out));
        assert_eq!(stdout(&out), "20 prepared:1:100:e", "stress {stress:?}");
    }
}

#[test]
fn uncaught_error_prints_its_stack() {
    let out = jsse(
        &[
            "--node",
            "-e",
            "function boom() { throw new Error('fatal'); }\nboom();",
        ],
        None,
    );
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(
        err.contains("Error: fatal\n    at boom (<anonymous>:1:"),
        "{err}"
    );
}
