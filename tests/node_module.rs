//! The embedded `module` builtin under `--node` (#1220): `Module`, the hooks
//! tools patch, `createRequire`, `findPackageJSON` and the source map API.
//! `scripts/run-node-module-oracle.sh` diffs the fixture drivers in
//! scripts/module-fixtures/proj against Node and pins Node's output in
//! expected-{main,sm}.txt; these tests check jsse against the pinned output
//! (so they need no Node) and cover what the oracle cannot (the compile cache
//! and `stripTypeScriptTypes`, which differ from Node on purpose).
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/module-fixtures")
}

fn tree() -> PathBuf {
    fixtures().join("proj")
}

fn jsse(args: &[&str], cwd: &Path, gc_stress: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
    if let Some(period) = gc_stress {
        cmd.env("JSSE_GC_STRESS", period);
    }
    cmd.env_remove("NODE_OPTIONS")
        .env("NODE_DISABLE_COMPILE_CACHE", "1")
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

fn eval(source: &str) -> String {
    let out = jsse(&["--node", "-e", source], &tree(), None);
    assert!(out.status.success(), "{}", stderr(&out));
    stdout(&out)
}

fn assert_matches_pinned(driver: &str, flags: &[&str], gc_stress: Option<&str>) {
    let mut args = vec!["--node"];
    args.extend_from_slice(flags);
    args.push(driver);
    let out = jsse(&args, &tree(), gc_stress);
    assert!(out.status.success(), "{driver} {flags:?}: {}", stderr(&out));
    let pinned = format!("expected-{}.txt", driver.trim_end_matches(".js"));
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
fn module_api_matches_nodes_output() {
    assert_matches_pinned("main.js", &[], None);
}

#[test]
fn source_map_api_matches_nodes_output() {
    assert_matches_pinned("sm.js", &[], None);
}

#[test]
fn fixtures_match_under_the_tree_walker() {
    assert_matches_pinned("main.js", &["--no-bytecode"], None);
    assert_matches_pinned("sm.js", &["--no-bytecode"], None);
}

#[test]
fn fixtures_match_under_gc_stress() {
    assert_matches_pinned("main.js", &[], Some("50"));
    assert_matches_pinned("sm.js", &[], Some("50"));
}

#[test]
fn compile_cache_is_a_reported_no_op() {
    let out = eval(
        r#"
        const m = require("module");
        const r = m.enableCompileCache();
        const s = m.enableCompileCache({ directory: "/tmp/never-created-by-jsse" });
        print(JSON.stringify([r.status, s.status, typeof r.message, m.getCompileCacheDir(),
          m.flushCompileCache(), m.constants.compileCacheStatus.DISABLED]));
        "#,
    );
    assert_eq!(out, r#"[3,3,"string",null,null,3]"#);
}

#[test]
fn strip_typescript_types_is_unsupported() {
    let out = eval(
        r#"
        try { require("module").stripTypeScriptTypes("let a: number = 1;"); }
        catch (e) { print(e.code, e.name); }
        "#,
    );
    assert_eq!(out, "ERR_METHOD_NOT_IMPLEMENTED Error");
}

#[test]
fn tsc_startup_probes_work() {
    let out = eval(
        r#"
        process.setSourceMapsEnabled(true);
        const m = require("node:module");
        let sms = "";
        try { require("jsse-absent-package-1220"); sms = "found"; } catch (e) { sms = e.code; }
        const req = m.createRequire(require("url").pathToFileURL(process.cwd() + "/lib/x.js"));
        print(process.sourceMapsEnabled, typeof m.enableCompileCache, sms,
          req("./a"), req.resolve("pkg").endsWith("node_modules/pkg/index.js"),
          m.isBuiltin("node:fs"), m.builtinModules.includes("fs"));
        "#,
    );
    assert_eq!(out, "true function MODULE_NOT_FOUND a true true true");
}

#[test]
fn source_map_is_registered_when_a_file_is_recompiled_while_enabled() {
    let out = eval(
        r#"
        const m = require("module");
        const f = require("path").join(process.cwd(), "sm/gen.js");
        require(f);
        m.setSourceMapsSupport(false);
        const before = m.findSourceMap(f);
        delete require.cache[f];
        m.setSourceMapsSupport(true);
        require(f);
        print(before === undefined, m.findSourceMap(f) instanceof m.SourceMap);
        "#,
    );
    assert_eq!(out, "true true");
}

#[test]
fn source_map_payload_is_isolated_from_its_input() {
    let out = eval(
        r#"
        const { SourceMap } = require("module");
        const p = { version: 3, sources: ["a"], names: ["foo"], mappings: "AAAA" };
        const sm = new SourceMap(p);
        p.names[0] = "z";
        sm.payload.names.push("x");
        print(sm.payload.names.join());
        "#,
    );
    assert_eq!(out, "foo");
}

#[test]
fn create_require_is_usable_from_an_es_module() {
    let dir = std::env::temp_dir().join(format!("jsse-node-module-esm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    std::fs::write(dir.join("dep.cjs"), "module.exports = { answer: 42 };").unwrap();
    let main = dir.join("main.mjs");
    std::fs::write(
        &main,
        r#"
        import { createRequire } from "node:module";
        import { pathToFileURL } from "node:url";
        const require = createRequire(pathToFileURL(process.argv[1]));
        console.log(require("./dep.cjs").answer, require.resolve("./dep.cjs").endsWith("dep.cjs"));
        "#,
    )
    .unwrap();
    let out = jsse(&["--node", main.to_str().unwrap()], &dir, None);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "42 true");
}

#[test]
fn nothing_is_visible_without_node() {
    let out = jsse(
        &["-e", "print(typeof require, typeof process, typeof module)"],
        Path::new(env!("CARGO_MANIFEST_DIR")),
        None,
    );
    assert_eq!(stdout(&out), "undefined undefined undefined");
}
