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
/// and stack frames outside the fixture directory (plus top-level frames) go.
fn normalise(text: &str, dir: &Path) -> String {
    let dir = dir.to_string_lossy();
    let text = text.replace(dir.as_ref(), "<dir>");
    let mut kept = Vec::new();
    for line in text.lines() {
        let frame = line.starts_with(char::is_whitespace) && line.trim_start().starts_with("at ");
        if frame && (!line.contains("<dir>/") || line.contains("Object.<anonymous>")) {
            continue;
        }
        kept.push(line);
    }
    kept.join("\n")
}

fn replay_fixture(extra: &[&str], stress: Option<&str>) {
    replay_named("cases", extra, stress);
}

fn replay_named(name: &str, extra: &[&str], stress: Option<&str>) {
    let dir = root().join("scripts/v8-error-fixtures");
    let cases = dir.join(format!("{name}.js"));
    let mut args = vec!["--node"];
    args.extend_from_slice(extra);
    args.push(cases.to_str().expect("utf-8 path"));
    let out = jsse(&args, stress);
    assert!(out.status.success(), "{}", stderr(&out));
    let expected =
        std::fs::read_to_string(dir.join(format!("expected-{name}.txt"))).expect("snapshot");
    let mut actual = normalise(&stdout(&out), &dir);
    let mut expected = expected.trim_end().to_string();
    if extra.contains(&"--no-bytecode") {
        // The state machine behind async functions has no frame of its own.
        for text in [&mut actual, &mut expected] {
            *text = text
                .split("## async before await")
                .next()
                .and_then(|kept| kept.split("## async arrow body").next())
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
fn required_modules_report_their_file_line_and_column() {
    for (extra, stress) in [
        (&[][..], None),
        (&["--no-bytecode"][..], None),
        (&[][..], Some("7")),
    ] {
        replay_named("modules", extra, stress);
    }
}

#[test]
fn frame_names_and_error_keys_match_node() {
    for (extra, stress) in [
        (&[][..], None),
        (&["--no-bytecode"][..], None),
        (&[][..], Some("7")),
    ] {
        replay_named("names", extra, stress);
    }
}

#[test]
fn engine_errors_keep_a_name_their_prototype_does_not_supply() {
    let out = jsse(
        &[
            "--node",
            "-e",
            r#"globalThis.TypeError = function TypeError() {}; TypeError.prototype = {};
try { null.x } catch (e) { print(e.name, JSON.stringify(Object.getOwnPropertyNames(e))) }"#,
        ],
        None,
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), r#"TypeError ["stack","message","name"]"#);
}

#[test]
fn default_mode_keeps_engine_error_keys() {
    let out = jsse(
        &[
            "-e",
            r#"var e; try { null.x } catch (x) { e = x }
var a = {}; a.f = function () { return new Error("x") };
print(JSON.stringify(Object.getOwnPropertyNames(e)), JSON.stringify(Object.getOwnPropertyNames(new AggregateError([], "m", { cause: 1 }))), a.f().stack === "")"#,
        ],
        None,
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        r#"["message","name"] ["message","cause","errors"] true"#
    );
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
            r#"print(["install", "capture", "frames", "locate", "name_source"].map((n) => typeof globalThis["__host_stack_" + n]).join())"#,
        ],
        None,
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "undefined,undefined,undefined,undefined,undefined"
    );
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
        assert_eq!(stdout(&out), "([eval]:2:36)\n11", "mode {mode:?}");
    }
}

#[test]
fn source_url_comments_supply_file_and_line() {
    let out = jsse(
        &[
            "--node",
            "-e",
            r#"const tagged = new Function("return new Error('t')\n//# sourceURL=/virtual/tagged.js");
const mention = new Function("return new Error('m') + '\\n//# sourceURL=/not/a/comment.js'");
print(tagged().stack.split("\n")[1]); print(new Function("return new Error('q')")().stack.split("\n")[1]);"#,
        ],
        None,
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "    at anonymous (/virtual/tagged.js:3:8)\n    at anonymous (<anonymous>:3:8)"
    );
}

#[test]
fn the_commonjs_loader_names_module_sources() {
    let dir = std::env::temp_dir().join(format!("jsse-v8-error-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    std::fs::write(
        dir.join("mod.js"),
        "#!/usr/bin/env node\nexports.make = function make() { return new Error('m'); };\n",
    )
    .expect("module");
    std::fs::write(
        dir.join("main.js"),
        "const m = require('./mod');\nconst f = m.make().stack.split('\\n');\nconsole.log(f[1].trim());\nconsole.log(new Function('return new Error(\"q\")')().stack.split('\\n')[1].trim());\n",
    )
    .expect("main");
    let main = dir.join("main.js");
    let out = jsse(&["--node", main.to_str().expect("utf-8 path")], None);
    let dir_text = dir.canonicalize().expect("canonical");
    let text = stdout(&out).replace(dir_text.to_str().expect("utf-8 path"), "<dir>");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        text,
        "at Object.make (<dir>/mod.js:2:41)\nat anonymous (<anonymous>:3:8)"
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
        assert_eq!(stdout(&out), "20 prepared:9:100:e", "stress {stress:?}");
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
        err.contains("Error: fatal\n    at boom ([eval]:1:"),
        "{err}"
    );
}
