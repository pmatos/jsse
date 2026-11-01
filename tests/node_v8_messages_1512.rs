//! V8 message text of destructuring, iteration, callees that are not a name
//! chain and builtin receivers under `--node` (#1512, #1532).
//! `scripts/run-node-v8-messages-1512-oracle.sh` generates the expectations in
//! `tests/node-v8-messages-1512` from Node itself; this file replays them
//! without Node through the VM, the tree-walker and GC stress, and checks that
//! default mode keeps the engine's own wording.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/node-v8-messages-1512")
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
fn v8_destructure_messages_match_node() {
    check_all(&[], false);
}

#[test]
fn v8_destructure_messages_match_node_on_the_tree_walker() {
    check_all(&["--no-bytecode"], false);
}

#[test]
fn v8_destructure_messages_match_node_under_gc_stress() {
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
        var five = 5, nul = null, fn = function () {}, o = {};\n\
        t(function () { class A extends null { constructor() { super(); } } new A(); });\n\
        t(function () { var { a = 1 } = nul; });\n\
        t(function () { var { 1: a } = nul; });\n\
        t(function () { var b; ({ a: [b] } = five); });\n\
        t(function () { var b; ({ a: o.x } = nul); });\n\
        t(function () { (five + 1)(); });\n\
        t(function () { (o.a = 1)(); });\n\
        t(function () { fn(...five, 1); });\n\
        t(function () { fn(1, ...five); });\n\
        t(function () { for (var x of new fn()); });\n\
        t(function () { ({}) instanceof { [Symbol.hasInstance]: 1 }; });\n\
        t(function () { BigInt(undefined); });\n\
        t(function () { BigInt.asIntN(1, 1); });\n\
        t(function () { Promise.withResolvers.call(1); });\n\
        t(function () { Object.getOwnPropertyDescriptor(RegExp.prototype, 'flags').get.call(1); });\n\
        t(function () { Object.getOwnPropertyDescriptor(Object.prototype, '__proto__').set.call(null, {}); });\n\
        t(function () { Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'byteLength').get.call(1); });\n\
        t(function () { Uint8Array.from.call(1); });\n\
        t(function () { new Map()[Symbol.iterator]().next.call(1); });\n\
        t(function () { try { throw null; } catch ({ a }) {} });\n\
        print(r.join('|'));";
    let expected = [
        "Super constructor is not a constructor",
        "Cannot convert undefined or null to object",
        "Cannot convert undefined or null to object",
        "Cannot convert undefined or null to object",
        "Cannot convert undefined or null to object",
        "6 is not a function",
        "1 is not a function",
        "is not iterable",
        "is not iterable",
        "is not iterable",
        "@@hasInstance is not callable",
        "Cannot convert value to a BigInt",
        "Cannot convert a Number to a BigInt",
        "Promise resolve or reject function is not callable",
        "RegExp.prototype.flags requires that 'this' be an Object",
        "Cannot convert undefined or null to object",
        "not an ArrayBuffer",
        "TypedArray.from requires a constructor",
        "Map Iterator.prototype.next requires a Map Iterator",
        "Cannot convert undefined or null to object",
    ]
    .join("|");
    for args in [&[][..], &["--no-bytecode"][..]] {
        let got = default_mode(probe, args);
        assert_eq!(got, expected, "{args:?}");
    }
}
