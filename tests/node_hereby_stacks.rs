//! Error stacks of a failing Herebyfile load (#1251): the `Error [CODE]` header
//! of Node's own errors under `--node`, and the hereby stacks oracle
//! (`scripts/run-hereby-stacks-oracle.py`) when TS_SRC_DIR points at a
//! TypeScript 6.0.3 checkout.

use std::path::{Path, PathBuf};
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

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("jsse-hereby-stacks-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Scratch(dir.canonicalize().expect("canonicalize scratch dir"))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn node_errors_carry_their_code_in_the_stack_header() {
    let out = jsse(&[
        "--node",
        "-e",
        r#"try { require("path").join(1); } catch (e) {
  console.log(e.stack.split("\n")[0]);
  console.log(Object.keys(e).join(), e.toString());
  var s = Object.getOwnPropertySymbols(e).map((k) => k.description + ":" + Object.getOwnPropertyDescriptor(e, k).enumerable + ":" + Object.getOwnPropertyDescriptor(e, k).writable);
  console.log(s.join());
}"#,
    ]);
    assert!(out.status.success());
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[0],
        "TypeError [ERR_INVALID_ARG_TYPE]: The \"path\" argument must be of type string. Received type number (1)"
    );
    assert_eq!(lines[1], format!("code {}", lines[0]));
    assert_eq!(lines[2], "kIsNodeError:false:true");
}

#[test]
fn plain_errors_keep_the_plain_header() {
    let out = jsse(&[
        "--node",
        "-e",
        r#"var e = new RangeError("r"); e.code = "ERR_FAKE";
console.log(e.stack.split("\n")[0]);"#,
    ]);
    assert_eq!(stdout(&out), "RangeError: r");
}

#[test]
fn a_missing_module_names_its_code_in_the_header_and_the_user_frames() {
    let dir = Scratch::new("missing");
    let main = dir.0.join("main.mjs");
    std::fs::write(
        &main,
        r#"function load() {
  return import("./nope.mjs");
}
try { await load(); } catch (e) {
  console.log(e.stack.split("\n")[0].replace(/'[^']*'/g, "'P'").replace(/imported from .*/, "imported from P"));
  console.log(e.code, e.name, e instanceof Error);
}"#,
    )
    .unwrap();
    let out = jsse(&["--node", main.to_str().unwrap()]);
    assert!(out.status.success());
    assert_eq!(
        stdout(&out),
        "Error [ERR_MODULE_NOT_FOUND]: Cannot find module 'P' imported from P\nERR_MODULE_NOT_FOUND Error true"
    );
}

#[test]
fn hereby_stacks_match_node_when_a_typescript_checkout_is_given() {
    if std::env::var_os("TS_SRC_DIR").is_none() {
        eprintln!(
            "SKIP: set TS_SRC_DIR to a TypeScript 6.0.3 checkout (npm ci done) to run the hereby stacks oracle"
        );
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("uv")
        .args(["run", "python"])
        .arg(root.join("scripts/run-hereby-stacks-oracle.py"))
        .args(["--require", "--jsse", env!("CARGO_BIN_EXE_jsse")])
        .output()
        .expect("run uv");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("as expected"), "{text}");
}
