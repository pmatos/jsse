//! V8 message text of builtin errors and engine-thrown error positions under
//! `--node` (#1510, #1511, #1512). `scripts/run-node-v8-messages-1510-oracle.sh`
//! generates the expectations in `tests/node-v8-messages-1510` from Node
//! itself; this file replays them without Node through the VM, the
//! tree-walker and GC stress, and checks that default mode keeps the engine's
//! own wording.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/node-v8-messages-1510")
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
fn v8_builtin_messages_match_node() {
    check_all(&[], false);
}

#[test]
fn v8_builtin_messages_match_node_on_the_tree_walker() {
    check_all(&["--no-bytecode"], false);
}

#[test]
fn v8_builtin_messages_match_node_under_gc_stress() {
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
        var frozen = Object.freeze({ a: 1 });\n\
        t(function () { 'abc'.repeat(-1); });\n\
        t(function () { [1].map(5); });\n\
        t(function () { new Promise(1); });\n\
        t(function () { Reflect.ownKeys(1); });\n\
        t(function () { JSON.parse('{'); });\n\
        t(function () { Object.setPrototypeOf({}, 1); });\n\
        t(function () { 'use strict'; frozen.a = 2; });\n\
        t(function () { 'use strict'; frozen.b = 2; });\n\
        t(function () { 'use strict'; delete frozen.a; });\n\
        t(function () { new Set(1); });\n\
        t(function () { Map.prototype.get.call({}, 1); });\n\
        t(function () { Number.prototype.toString.call('a'); });\n\
        t(function () { Object.defineProperty(frozen, 'z', { value: 1 }); });\n\
        print(r.join('|'));";
    let expected = [
        "Invalid count value",
        "map callback is not a function",
        "Promise resolver is not a function",
        "Reflect.ownKeys requires an object",
        "Unexpected token '{', \"{\" is not valid JSON",
        "Object prototype may only be an Object or null",
        "Cannot assign to read only property 'a'",
        "Cannot assign to read only property 'b'",
        "Cannot delete property 'a' of object",
        "is not iterable",
        "Map.prototype.get requires a Map",
        "Number.prototype.toString requires a Number",
        "Cannot define property, object is not extensible or property is non-configurable",
    ]
    .join("|");
    for args in [&[][..], &["--no-bytecode"][..]] {
        assert_eq!(default_mode(probe, args), expected, "{args:?}");
    }
}
