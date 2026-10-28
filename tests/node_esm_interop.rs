//! `import.meta`, ES module/CommonJS interop and `require()` of ES modules
//! under `--node` (#1241). `scripts/run-node-esm-interop-oracle.sh` diffs the
//! drivers in scripts/esm-interop-fixtures/proj against Node and pins Node's
//! output in expected-*.txt; these tests check jsse against the pinned output
//! (so they need no Node, also under the tree-walker and GC stress) and cover
//! the launcher paths and the default-mode gate.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/esm-interop-fixtures")
}

fn jsse(args: &[&str], cwd: &Path, gc_stress: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
    if let Some(period) = gc_stress {
        cmd.env("JSSE_GC_STRESS", period);
    }
    cmd.env_remove("NODE_OPTIONS")
        .current_dir(cwd)
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

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(tag: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!("jsse-node-interop-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Scratch(dir.canonicalize().unwrap())
}

fn write(dir: &Scratch, name: &str, source: &str) -> PathBuf {
    let path = dir.0.join(name);
    std::fs::write(&path, source).unwrap();
    path
}

fn assert_matches_pinned(driver: &str, flags: &[&str], gc_stress: Option<&str>) {
    let main = fixtures().join("proj").join(driver);
    let mut args = vec!["--node"];
    args.extend_from_slice(flags);
    args.push(main.to_str().unwrap());
    let out = jsse(&args, Path::new(env!("CARGO_MANIFEST_DIR")), gc_stress);
    assert!(out.status.success(), "{driver} {flags:?}: {}", stderr(&out));
    let pinned = format!("expected-{}.txt", driver.replace('.', "-"));
    let want = std::fs::read_to_string(fixtures().join(pinned)).unwrap();
    let got = stdout(&out);
    for (n, (g, w)) in got.lines().zip(want.trim_end().lines()).enumerate() {
        assert_eq!(g, w, "{driver} {flags:?}: line {}", n + 1);
    }
    assert_eq!(
        got.lines().count(),
        want.trim_end().lines().count(),
        "{driver} {flags:?}"
    );
}

#[test]
fn es_module_drivers_match_nodes_output() {
    assert_matches_pinned("main.mjs", &[], None);
    assert_matches_pinned("lex.mjs", &[], None);
}

#[test]
fn require_esm_driver_matches_nodes_output() {
    assert_matches_pinned("main.cjs", &[], None);
}

#[test]
fn drivers_match_under_the_tree_walker() {
    for driver in ["main.mjs", "main.cjs", "lex.mjs"] {
        assert_matches_pinned(driver, &["--no-bytecode"], None);
    }
}

#[test]
fn drivers_match_under_gc_stress() {
    for driver in ["main.mjs", "main.cjs", "lex.mjs"] {
        assert_matches_pinned(driver, &[], Some("7"));
    }
}

#[test]
fn a_typeless_main_with_es_syntax_runs_as_an_es_module() {
    let dir = scratch("main-detect");
    write(&dir, "package.json", r#"{"name":"detect"}"#);
    let main = write(
        &dir,
        "main.js",
        "import fs from 'node:fs';\nconsole.log(typeof fs.readFileSync, import.meta.main);\nawait 1;\nconsole.log('after await');",
    );
    let out = jsse(&["--node", main.to_str().unwrap()], &dir.0, None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "function true\nafter await");

    let failing = write(
        &dir,
        "fail.js",
        "import 'node:fs';\nthrow new Error('esm main failed');",
    );
    let out = jsse(&["--node", failing.to_str().unwrap()], &dir.0, None);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("esm main failed"), "{}", stderr(&out));
}

#[test]
fn create_require_resolves_relative_to_import_meta_url() {
    let dir = scratch("create-require");
    write(&dir, "package.json", r#"{"type":"module"}"#);
    std::fs::create_dir_all(dir.0.join("lib")).unwrap();
    write(&dir, "lib/dep.cjs", "module.exports = { answer: 42 };");
    let main = write(
        &dir,
        "lib/main.js",
        r#"
        import { createRequire } from 'node:module';
        const require = createRequire(import.meta.url);
        console.log(require('./dep.cjs').answer, require.resolve('./dep.cjs').endsWith('lib/dep.cjs'));
        "#,
    );
    let out = jsse(&["--node", main.to_str().unwrap()], &dir.0, None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "42 true");
}

#[test]
fn a_required_es_module_survives_gc_stress_with_its_facade() {
    let dir = scratch("gc");
    write(
        &dir,
        "dep.mjs",
        "export let n = 0; export const inc = () => { n++; }; export default { tag: 'd' };",
    );
    write(
        &dir,
        "named.cjs",
        "exports.a = 1; exports.b = { deep: [1, 2, 3] };",
    );
    let main = write(
        &dir,
        "main.cjs",
        r#"
        const dep = require('./dep.mjs');
        dep.inc();
        import('./named.cjs').then((ns) => {
          console.log(dep.n, dep.__esModule, dep.default.tag, ns.a, ns.b.deep.length);
        });
        "#,
    );
    let out = jsse(&["--node", main.to_str().unwrap()], &dir.0, Some("3"));
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "1 true d 1 3");
}

#[test]
fn nothing_changes_without_node() {
    let dir = scratch("gate");
    let main = write(
        &dir,
        "main.mjs",
        "console.log(Object.keys(import.meta).join(), typeof __host_esm_require, typeof __host_cjs_lex, typeof process);",
    );
    let out = jsse(&[main.to_str().unwrap()], &dir.0, None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "url undefined undefined undefined");
}
