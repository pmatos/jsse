//! The Node prelude embedded in `--node` (#1216): `process`, `console`, `util`,
//! `Buffer`, `TextEncoder`/`TextDecoder` and `crypto` with no prepended files,
//! and its absence everywhere else. Child-process checks; the Node-oracle diff
//! lives in `scripts/run-node-prelude-oracle.sh`.

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

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim_end().to_string()
}

const PRELUDE_GLOBALS: &str = r#"["process","Buffer","TextEncoder","TextDecoder","crypto","util"].map(function (n) { return n + ":" + typeof globalThis[n]; }).join(" ")"#;

#[test]
fn one_liner_works_with_no_prepended_files() {
    let out = jsse(&[
        "--node",
        "-e",
        r#"console.log(process.version, Buffer.from("a"))"#,
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    let (version, rest) = text.split_once(' ').expect("version then buffer");
    assert!(
        version.starts_with('v') && version[1..].split('.').count() == 3,
        "unexpected process.version {version:?}"
    );
    assert!(rest.contains("Buffer") || rest.contains("61"), "{rest:?}");
    assert_eq!(stderr(&out), "");
    let out = jsse(&[
        "--node",
        "-e",
        r#"console.log(Buffer.from("a").toString("hex"), Buffer.isBuffer(Buffer.from("a")))"#,
    ]);
    assert_eq!(stdout(&out), "61 true");
}

#[test]
fn prelude_surfaces_are_installed_under_node() {
    let source = format!(
        r#"print({PRELUDE_GLOBALS}); print([typeof process.stdout.write, typeof console.group, typeof Buffer.isBuffer, typeof crypto.getRandomValues, typeof crypto.randomUUID, typeof util.format, typeof util.inspect].join())"#
    );
    let out = jsse(&["--node", "-e", &source]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "process:object Buffer:function TextEncoder:function TextDecoder:function crypto:object util:object\n\
         function,function,function,function,function,function,function"
    );
}

#[test]
fn prelude_works_in_modules_and_script_files() {
    let dir = std::env::temp_dir().join(format!("jsse-node-prelude-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("probe.js");
    std::fs::write(&script, "console.log(Buffer.from('hi').toString('hex'))").unwrap();
    let out = jsse(&["--node", script.to_str().unwrap()]);
    assert_eq!(stdout(&out), "6869");
    let module = dir.join("probe.mjs");
    std::fs::write(
        &module,
        "console.log(typeof process.argv[1], Buffer.byteLength('€'))",
    )
    .unwrap();
    let out = jsse(&["--node", module.to_str().unwrap()]);
    assert_eq!(stdout(&out), "string 3");
}

#[test]
fn prelude_is_not_evaluated_without_node() {
    let source = format!(
        r#"print({PRELUDE_GLOBALS}); print(typeof console.group, typeof __host_write, Function.prototype.toString.call(console.log).indexOf("[native code]") >= 0)"#
    );
    let out = jsse(&["-e", &source]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "process:undefined Buffer:undefined TextEncoder:undefined TextDecoder:undefined crypto:undefined util:undefined\n\
         undefined undefined true"
    );
}

#[test]
fn prelude_flag_does_not_enable_the_prelude() {
    let dir = std::env::temp_dir().join(format!("jsse-node-prelude-flag-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let harness = dir.join("harness.js");
    std::fs::write(&harness, "var harnessLoaded = true;").unwrap();
    let source = format!(r#"print(harnessLoaded, {PRELUDE_GLOBALS})"#);
    let out = jsse(&["--prelude", harness.to_str().unwrap(), "-e", &source]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "true process:undefined Buffer:undefined TextEncoder:undefined TextDecoder:undefined crypto:undefined util:undefined"
    );
}

#[test]
fn prelude_does_not_use_the_step_budget() {
    let out = jsse(&["--node", "--step-budget", "50", "-e", "print('ran')"]);
    assert_eq!(stdout(&out), "ran");
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn prelude_runs_before_user_preludes_and_main() {
    let dir = std::env::temp_dir().join(format!("jsse-node-prelude-order-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pre = dir.join("pre.js");
    std::fs::write(&pre, "var sawBuffer = typeof Buffer;").unwrap();
    let out = jsse(&[
        "--node",
        "--prelude",
        pre.to_str().unwrap(),
        "-e",
        "print(sawBuffer)",
    ]);
    assert_eq!(stdout(&out), "function");
}
