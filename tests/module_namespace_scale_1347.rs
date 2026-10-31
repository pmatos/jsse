//! Module namespaces with many exports must stay linear (jsse#1347).
//!
//! Every namespace property access used to clone the whole export table and
//! scan the export list, so `Object.keys(ns)` over N exports cost O(N^2): about
//! 55 s for 30,000 exports. The linear implementation takes well under a
//! second; the bound below sits two orders of magnitude from either side, so
//! host load cannot turn it flaky while the quadratic code still fails it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const EXPORTS: usize = 30_000;
const BOUND: Duration = Duration::from_secs(20);

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("jsse-ns-scale-1347-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("failed to create scratch dir");
    dir
}

fn run(dir: &Path, args: &[&str]) -> (String, Duration) {
    let start = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("failed to run jsse");
    let elapsed = start.elapsed();
    assert!(
        output.status.success(),
        "jsse failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    (
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
        elapsed,
    )
}

fn assert_within_bound(what: &str, elapsed: Duration) {
    assert!(
        elapsed < BOUND,
        "{what} took {elapsed:?} for {EXPORTS} exports (bound {BOUND:?}): quadratic namespace cost"
    );
}

const WALK: &str = "
const keys = Object.keys(ns);
const sorted = keys.slice().sort();
let ok = keys.length === COUNT + EXTRA && keys.every((k, i) => k === sorted[i]);
for (let i = 0; i < COUNT; i += 1) {
  const name = PREFIX + i;
  ok = ok && (name in ns) && Object.hasOwn(ns, name) && ns[name] === i
    && Object.getOwnPropertyDescriptor(ns, name).value === i
    && Reflect.deleteProperty(ns, name) === false;
}
ok = ok && !('missing' in ns) && Object.entries(ns).length === COUNT + EXTRA;
console.log(ok ? 'ok' : 'wrong');
";

fn walker(import: &str, prefix: &str, extra: usize) -> String {
    format!(
        "{import}\nconst COUNT = {EXPORTS};\nconst EXTRA = {extra};\nconst PREFIX = '{prefix}';\n{WALK}"
    )
}

#[test]
fn namespace_of_many_exports_is_linear() {
    let dir = scratch("esm");
    let mut source = String::new();
    for i in 0..EXPORTS {
        source.push_str(&format!("export const n{i} = {i};\n"));
    }
    fs::write(dir.join("e.mjs"), source).unwrap();
    fs::write(
        dir.join("main.mjs"),
        walker("import * as ns from './e.mjs';", "n", 0),
    )
    .unwrap();
    let (out, elapsed) = run(&dir, &["main.mjs"]);
    assert_eq!(out, "ok");
    assert_within_bound("namespace walk", elapsed);
}

#[test]
fn star_reexported_namespace_of_many_exports_is_linear() {
    let dir = scratch("star");
    let mut source = String::new();
    for i in 0..EXPORTS {
        source.push_str(&format!("export const n{i} = {i};\n"));
    }
    fs::write(dir.join("e.mjs"), source).unwrap();
    fs::write(dir.join("r.mjs"), "export * from './e.mjs';\n").unwrap();
    fs::write(
        dir.join("main.mjs"),
        walker("import * as ns from './r.mjs';", "n", 0),
    )
    .unwrap();
    let (out, elapsed) = run(&dir, &["main.mjs"]);
    assert_eq!(out, "ok");
    assert_within_bound("export * namespace walk", elapsed);
}

#[test]
fn commonjs_named_exports_under_node_host_are_linear() {
    let dir = scratch("cjs");
    let mut source = String::from("exports.__esModule = true;\n");
    for i in 0..EXPORTS {
        source.push_str(&format!("exports.k{i} = {i};\n"));
    }
    fs::write(dir.join("c.cjs"), source).unwrap();
    // __esModule, default and module.exports join the k<N> names
    let main = walker("import * as ns from './c.cjs';", "k", 3);
    fs::write(dir.join("main.mjs"), main).unwrap();
    let (out, elapsed) = run(&dir, &["--node", "main.mjs"]);
    assert_eq!(out, "ok");
    assert_within_bound("CommonJS namespace walk", elapsed);
}
