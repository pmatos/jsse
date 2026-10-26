//! The ES module loader under `--node` (#1240): `.mjs` and `"type": "module"`
//! files, bare specifiers through package `exports`/`imports`, builtins as
//! modules, JSON import attributes, dynamic `import()` from ES modules and
//! CommonJS, top-level await and the Node error codes. The fixture tree under
//! scripts/esm-fixtures is diffed against Node by
//! `scripts/run-node-esm-oracle.sh`, which also pins Node's output in
//! expected-main.txt; these tests check jsse against the pinned output (so they
//! need no Node) and cover the launcher contract.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/esm-fixtures")
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
    let dir = std::env::temp_dir().join(format!("jsse-node-esm-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Scratch(dir.canonicalize().unwrap())
}

fn write(dir: &Scratch, name: &str, source: &str) -> PathBuf {
    let path = dir.0.join(name);
    std::fs::write(&path, source).unwrap();
    path
}

fn run_main(dir: &Scratch, name: &str, extra: &[&str]) -> Output {
    let main = dir.0.join(name);
    let mut args = vec!["--node"];
    args.extend_from_slice(extra);
    args.push(main.to_str().unwrap());
    jsse(&args, &dir.0, None)
}

fn assert_matches_pinned(flags: &[&str]) {
    let main = fixtures().join("proj/main.mjs");
    let mut args = vec!["--node"];
    args.extend_from_slice(flags);
    args.push(main.to_str().unwrap());
    let out = jsse(&args, Path::new(env!("CARGO_MANIFEST_DIR")), None);
    assert!(out.status.success(), "{flags:?}: {}", stderr(&out));
    let want = std::fs::read_to_string(fixtures().join("expected-main.txt")).unwrap();
    let got = stdout(&out);
    for (n, (g, w)) in got.lines().zip(want.trim_end().lines()).enumerate() {
        assert_eq!(g, w, "{flags:?}: line {}", n + 1);
    }
    assert_eq!(
        got.lines().count(),
        want.trim_end().lines().count(),
        "{flags:?}"
    );
}

#[test]
fn fixture_tree_matches_nodes_output() {
    assert_matches_pinned(&[]);
}

#[test]
fn fixture_tree_matches_nodes_output_under_the_tree_walker() {
    assert_matches_pinned(&["--no-bytecode"]);
}

#[test]
fn a_graph_of_every_format_survives_gc_stress() {
    let dir = scratch("stress");
    write(&dir, "package.json", r#"{"type":"module"}"#);
    write(&dir, "data.json", r#"{"k":[1,2,3]}"#);
    write(&dir, "cjs.cjs", "module.exports = { v: 'cjs' };");
    write(&dir, "dyn.mjs", "export default 'dyn';");
    write(
        &dir,
        "main.mjs",
        r#"
        import { join } from 'node:path';
        import data from './data.json' with { type: 'json' };
        import cjs from './cjs.cjs';
        const dyn = await import('./dyn.mjs');
        console.log(join('a', 'b'), JSON.stringify(data), cjs.v, dyn.default);
        "#,
    );
    let main = dir.0.join("main.mjs");
    let out = jsse(&["--node", main.to_str().unwrap()], &dir.0, Some("5"));
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "a/b {\"k\":[1,2,3]} cjs dyn");
}

#[test]
fn the_loader_is_invisible_without_node() {
    let dir = scratch("gate");
    write(&dir, "dep.mjs", "export const x = 1;");
    let main = write(&dir, "main.mjs", "import 'node:fs';");
    let out = jsse(&[main.to_str().unwrap()], &dir.0, None);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("Cannot resolve bare module specifier"),
        "{}",
        stderr(&out)
    );
    let main = write(
        &dir,
        "probe.mjs",
        "import { x } from './dep.mjs'; console.log(x, typeof __host_esm_install, typeof __host_tag_function);",
    );
    let out = jsse(&[main.to_str().unwrap()], &dir.0, None);
    assert_eq!(stdout(&out), "1 undefined undefined");
}

#[test]
fn the_host_entry_points_are_private() {
    let dir = scratch("private");
    write(
        &dir,
        "main.mjs",
        "console.log(typeof __host_esm_install, typeof __host_tag_function);",
    );
    let out = run_main(&dir, "main.mjs", &[]);
    assert_eq!(stdout(&out), "undefined undefined");
}

#[test]
fn an_unresolvable_static_import_is_fatal_with_its_code() {
    let dir = scratch("missing");
    write(&dir, "main.mjs", "import './nope.mjs';");
    let out = run_main(&dir, "main.mjs", &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("Cannot find module"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn unsettled_top_level_await_exits_13() {
    let dir = scratch("unsettled");
    write(
        &dir,
        "main.mjs",
        "console.log('a'); await new Promise(() => {}); console.log('never');",
    );
    let out = run_main(&dir, "main.mjs", &[]);
    assert_eq!(out.status.code(), Some(13));
    assert_eq!(stdout(&out), "a");
}

#[test]
fn a_late_top_level_await_rejection_is_an_uncaught_exception() {
    let dir = scratch("late");
    write(
        &dir,
        "main.mjs",
        "await new Promise((r) => setTimeout(r, 5)); throw new Error('late');",
    );
    let out = run_main(&dir, "main.mjs", &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("late"), "{}", stderr(&out));

    write(
        &dir,
        "handled.mjs",
        "process.on('uncaughtException', (e) => console.log('caught', e.message));\
         await new Promise((r) => setTimeout(r, 5)); throw new Error('late');",
    );
    let out = run_main(&dir, "handled.mjs", &[]);
    assert_eq!(stdout(&out), "caught late");
}

#[test]
fn a_timer_keeps_pending_top_level_await_alive() {
    let dir = scratch("timer");
    write(
        &dir,
        "main.mjs",
        "const v = await new Promise((r) => setTimeout(() => r('done'), 20)); console.log(v);",
    );
    let out = run_main(&dir, "main.mjs", &[]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "done");
}

#[test]
fn commonjs_main_and_eval_can_import() {
    let dir = scratch("cjs");
    write(&dir, "dep.mjs", "export const x = 'dep';");
    write(
        &dir,
        "main.cjs",
        "import('./dep.mjs').then((m) => console.log(m.x));\
         setTimeout(() => import('./dep.mjs').then((m) => console.log('later', m.x)), 1);",
    );
    let out = run_main(&dir, "main.cjs", &[]);
    assert_eq!(stdout(&out), "dep\nlater dep");
    let out = jsse(
        &[
            "--node",
            "-e",
            "import('./dep.mjs').then((m) => console.log(m.x))",
        ],
        &dir.0,
        None,
    );
    assert_eq!(stdout(&out), "dep");
}

#[test]
fn dynamic_import_loads_after_the_calling_code_finishes() {
    let dir = scratch("order");
    write(&dir, "dep.mjs", "console.log('dep');");
    write(
        &dir,
        "main.mjs",
        "const p = import('./dep.mjs'); console.log('after'); await p;",
    );
    let out = run_main(&dir, "main.mjs", &[]);
    assert_eq!(stdout(&out), "after\ndep");
}

#[test]
fn builtins_import_as_default_and_named() {
    let dir = scratch("builtin");
    write(
        &dir,
        "main.mjs",
        "import * as os from 'node:os'; import os2, { EOL } from 'os'; \
         console.log(os.default === os2, os.EOL === EOL);",
    );
    let out = run_main(&dir, "main.mjs", &[]);
    assert_eq!(stdout(&out), "true true");
}
