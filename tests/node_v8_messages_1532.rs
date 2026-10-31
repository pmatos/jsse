//! V8 message text of builtin receiver and range errors, proxy traps, call
//! spread and tagged templates under `--node` (#1532, #1512).
//! `scripts/run-node-v8-messages-1532-oracle.sh`
//! generates the expectations in `tests/node-v8-messages-1532` from Node
//! itself; this file replays them without Node through the VM, the
//! tree-walker and GC stress, and checks that default mode keeps the engine's
//! own wording.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/node-v8-messages-1532")
        .canonicalize()
        .unwrap()
}

fn normalise(text: &str, dir: &Path) -> String {
    let text = text.replace(dir.to_str().unwrap(), "<dir>");
    let mut out = String::new();
    for line in text.lines() {
        let frame = line.trim_start().starts_with("at ");
        if frame && line.contains("node:") {
            continue;
        }
        if line.starts_with("Node.js v") {
            out.push_str("Node.js <version>\n");
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn run(args: &[&str], stress: bool, dir: &Path, eval: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
    cmd.arg("--node").args(args).current_dir(dir);
    if let Some(text) = eval {
        cmd.arg("-e").arg(text);
    }
    if stress {
        cmd.env("JSSE_GC_STRESS", "7");
    }
    for name in ["FORCE_COLOR", "NO_COLOR", "NODE_DISABLE_COLORS"] {
        cmd.env_remove(name);
    }
    cmd.output().unwrap()
}

fn check_all(args: &[&str], stress: bool) {
    let dir = fixture_dir();
    let mut checked = 0;
    let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".js"))
        else {
            continue;
        };
        let source = std::fs::read_to_string(&path).unwrap();
        let expected = std::fs::read_to_string(dir.join(format!("{name}.out"))).unwrap();
        let out = if name.starts_with("eval-") {
            run(args, stress, &dir, Some(source.trim_end_matches('\n')))
        } else {
            let mut full = args.to_vec();
            full.push(path.to_str().unwrap());
            run(&full, stress, &dir, None)
        };
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        assert_eq!(normalise(&text, &dir), expected, "{name} {args:?}");
        checked += 1;
    }
    assert!(checked >= 3, "only {checked} fixtures");
}

#[test]
fn v8_receiver_messages_match_node() {
    check_all(&[], false);
}

#[test]
fn v8_receiver_messages_match_node_on_the_tree_walker() {
    check_all(&["--no-bytecode"], false);
}

#[test]
fn v8_receiver_messages_match_node_under_gc_stress() {
    check_all(&[], true);
}

fn default_mode(code: &str, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(args)
        .args(["-e", code])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn default_mode_keeps_the_engine_wording() {
    let probe = "var r = [];\n\
        function t(f) { try { f(); } catch (e) { r.push(e.message); } }\n\
        t(function () { String.prototype.trim.call(null); });\n\
        t(function () { Array.prototype.map.call(null, function () {}); });\n\
        t(function () { RegExp.prototype.exec.call(1); });\n\
        t(function () { new Uint8Array(new ArrayBuffer(3), 5); });\n\
        t(function () { new DataView(new ArrayBuffer(1), 5); });\n\
        t(function () { new Proxy({}, { get: 1 }).x; });\n\
        t(function () { var a = {}; a.a = a; JSON.stringify(a); });\n\
        t(function () { 'use strict'; undefined = 1; });\n\
        t(function () { (function () {})(...5); });\n\
        t(function () { var five = 5; var [a] = five; });\n\
        t(function () { Function.prototype.call.call(1); });\n\
        print(r.join('|'));";
    let expected = [
        "String.prototype method called on null or undefined",
        "Cannot convert undefined or null to object",
        "RegExp.prototype.exec requires that 'this' be an Object",
        "start offset is outside the bounds of the buffer",
        "offset is outside the bounds of the buffer",
        "proxy handler's get trap is not a function",
        "Converting circular structure to JSON",
        "Assignment to constant variable.",
        "is not iterable",
        "is not iterable",
        "1 is not a function",
    ]
    .join("|");
    for args in [&[][..], &["--no-bytecode"][..]] {
        let got = default_mode(probe, args);
        assert_eq!(got, expected, "{args:?}");
    }
}
