//! V8 message text of calls and array assignments in iterable positions,
//! parameter patterns, proxy traps and assorted builtins under `--node`
//! (#1557, #1558). `scripts/run-node-v8-messages-1557-oracle.sh` generates
//! the expectations in `tests/node-v8-messages-1557` from Node itself; this
//! file replays them without Node through the VM, the tree-walker and GC
//! stress, and checks that default mode keeps the engine's own wording.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/node-v8-messages-1557")
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
    assert!(checked >= 6, "only {checked} fixtures");
}

#[test]
fn v8_iteration_and_builtin_messages_match_node() {
    check_all(&[], false);
}

#[test]
fn v8_iteration_and_builtin_messages_match_node_on_the_tree_walker() {
    check_all(&["--no-bytecode"], false);
}

#[test]
fn v8_iteration_and_builtin_messages_match_node_under_gc_stress() {
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
    let probe = r#"var r = [];
function t(f) { try { f(); r.push('no error'); } catch (e) { r.push(e.message); } }
t(function () { var o = { m: 1 }; for (var x of o.m()); });
t(function () { var o = { m: 1 }; [...o.m()]; });
t(function () { var nul = null; var [a] = [] = nul; });
t(function () { for (var x of 1 + 1); });
t(function () { (function ({ a } = null) {})(); });
t(function () { (function ({ a }, [b]) {})({}); });
t(function () { var a = { ']': 1 }; a[']'](); });
t(function () { var nul = null; var [a] = (nul); });
t(function () { Promise.prototype.finally.call({ then: 1 }, function () {}); });
t(function () { new SharedArrayBuffer(1).grow(1); });
t(function () { Object.setPrototypeOf(new Proxy({}, { setPrototypeOf() { return false; } }), null); });
t(function () { new Proxy(function () {}, { apply: 1 })(); });
t(function () {
  class A { constructor(o) { return o; } }
  class B extends A { #y = 1; constructor(o) { super(o); } }
  var o = {}; new B(o); new B(o);
});
t(function () {
  class A { constructor(o) { return o; } }
  class B extends A { get #g() { return 1; } constructor(o) { super(o); } }
  var o = {}; new B(o); new B(o);
});
t(function () { Object.prototype.__proto__ = {}; });
t(function () { [][Symbol.iterator]().next.call(1); });
t(function () { Iterator.from(5); });
t(function () { new Intl.NumberFormat('en', { style: 'x' }); });
t(function () { new FinalizationRegistry(function () {}).register(1); });
t(function () { structuredClone(); });
t(function () { new RegExp('(', 'g'); });
print(r.join('|'));"#;
    let expected = [
        "1 is not a function",
        "1 is not a function",
        "Cannot convert undefined or null to object",
        "is not iterable",
        "Cannot convert undefined or null to object",
        "Cannot convert undefined or null to object",
        "1 is not a function",
        "Cannot convert undefined or null to object",
        "1 is not a function",
        "SharedArrayBuffer is not growable",
        "'setPrototypeOf' on proxy: trap returned falsish",
        "proxy handler's apply trap is not a function",
        "Cannot initialize private field twice on the same object",
        "Cannot add private accessor to object twice",
        "Cyclic __proto__ value",
        "next called on non-object",
        "value is not an object",
        "Value x is not allowed for option style",
        "FinalizationRegistry.register: target must be an object or non-registered symbol",
        "structuredClone: 1 argument required, but only 0 present.",
        "Invalid regular expression: /(/ : Unterminated group",
    ]
    .join("|");
    for args in [&[][..], &["--no-bytecode"][..]] {
        let got = default_mode(probe, args);
        assert_eq!(got, expected, "{args:?}");
    }
}
