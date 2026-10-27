//! What `hereby local` (#1253) needed from the `--node` host: a program that
//! spawns `process.execPath` (the TypeScript Herebyfile does, for its
//! generator scripts and for `tsc -b`) gets a Node-compatible child, as with
//! `fork`. The build itself runs under `scripts/run-hereby-local-oracle.py`;
//! the last case drives it when TS_SRC_DIR points at a TypeScript 6.0.3
//! checkout and JSSE_HEREBY_LOCAL_ORACLE is set (the build takes a long time).

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

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim_end().to_string()
}

fn run_node(code: &str) -> String {
    let out = jsse(&["--node", "-e", code]);
    assert!(out.status.success(), "{}", stderr(&out));
    stdout(&out)
}

#[test]
fn spawning_exec_path_runs_a_node_program() {
    let got = run_node(
        r#"const cp = require("child_process");
const probe = "console.log(typeof require, typeof process.versions.node, JSON.stringify(process.argv.slice(1)))";
const sync = cp.spawnSync(process.execPath, ["-e", probe, "a", "b"], { encoding: "utf8" });
console.log(sync.status, sync.stdout.trim());
console.log(cp.execFileSync(process.execPath, ["-e", probe], { encoding: "utf8" }).trim());
const child = cp.spawn(process.execPath, ["-e", probe, "x"]);
let out = "";
child.stdout.on("data", (d) => (out += d));
child.on("close", (code) => console.log(code, out.trim()));"#,
    );
    assert_eq!(
        got,
        "0 function string [\"a\",\"b\"]\nfunction string []\n0 function string [\"x\"]"
    );
}

#[test]
fn an_explicit_node_flag_is_not_repeated() {
    let got = run_node(
        r#"const cp = require("child_process");
const r = cp.spawnSync(process.execPath, ["--node", "-e", "console.log(process.argv.length)"], { encoding: "utf8" });
console.log(r.status, r.stdout.trim());"#,
    );
    assert_eq!(got, "0 1");
}

#[test]
fn spawn_through_a_shell_and_other_programs_are_left_alone() {
    let got = run_node(
        r#"const cp = require("child_process");
const r = cp.spawnSync("echo", ["--node", "kept"], { encoding: "utf8" });
console.log(r.stdout.trim());
const s = cp.spawnSync("echo " + JSON.stringify(process.execPath) + " ok", { shell: true, encoding: "utf8" });
console.log(s.stdout.trim() === process.execPath + " ok");"#,
    );
    assert_eq!(got, "--node kept\ntrue");
}

#[test]
fn a_bare_exec_path_child_without_node_has_no_prelude_in_default_mode() {
    let out = jsse(&["-e", "print(typeof require, typeof process)"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "undefined undefined");
}

#[test]
fn hereby_local_matches_node_when_a_checkout_is_given() {
    if std::env::var_os("JSSE_HEREBY_LOCAL_ORACLE").is_none() {
        return;
    }
    let Some(ts) = std::env::var_os("TS_SRC_DIR") else {
        return;
    };
    if !Path::new(&ts).join("node_modules").is_dir() {
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("uv")
        .args(["run", "python", "scripts/run-hereby-local-oracle.py"])
        .arg("--jsse")
        .arg(env!("CARGO_BIN_EXE_jsse"))
        .arg("--skip-harness")
        .current_dir(root)
        .output()
        .expect("run the hereby local oracle");
    assert!(out.status.success(), "{}\n{}", stdout(&out), stderr(&out));
}
