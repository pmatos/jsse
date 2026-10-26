//! The CommonJS loader under `--node` (#1219): `require`, `module`, the main
//! module wrapper, node_modules resolution, package exports/imports, JSON and
//! cycles. The fixture tree under scripts/cjs-fixtures is diffed against Node
//! by `scripts/run-node-cjs-oracle.sh`, which also pins Node's output in
//! expected-{main,eval}.txt; these tests check jsse against the pinned output
//! (so they need no Node) and cover the launcher contract.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/cjs-fixtures")
}

fn tree() -> PathBuf {
    fixtures().join("proj")
}

fn jsse(args: &[&str], cwd: &Path, gc_stress: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
    if let Some(period) = gc_stress {
        cmd.env("JSSE_GC_STRESS", period);
    }
    cmd.env("HOME", tree().join("home"))
        .env("NODE_PATH", tree().join("np"))
        .env_remove("NODE_OPTIONS")
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
    let dir = std::env::temp_dir().join(format!("jsse-node-cjs-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Scratch(dir.canonicalize().unwrap())
}

fn assert_matches_pinned(flags: &[&str], gc_stress: Option<&str>) {
    let main = tree().join("main.js");
    let mut args = vec!["--node"];
    args.extend_from_slice(flags);
    args.push(main.to_str().unwrap());
    let out = jsse(&args, Path::new(env!("CARGO_MANIFEST_DIR")), gc_stress);
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
    assert_matches_pinned(&[], None);
}

#[test]
fn fixture_tree_matches_nodes_output_under_the_tree_walker() {
    assert_matches_pinned(&["--no-bytecode"], None);
}

#[test]
fn fixture_tree_matches_nodes_output_under_gc_stress() {
    assert_matches_pinned(&[], Some("50"));
}

#[test]
fn eval_gets_require_and_an_eval_module() {
    let source = std::fs::read_to_string(fixtures().join("eval.js")).unwrap();
    let out = jsse(&["--node", "-e", &source], &tree(), None);
    assert!(out.status.success(), "{}", stderr(&out));
    let want = std::fs::read_to_string(fixtures().join("expected-eval.txt")).unwrap();
    assert_eq!(stdout(&out), want.trim_end());
}

#[test]
fn nothing_is_visible_without_node() {
    let probe = "print(typeof require, typeof module, typeof exports, typeof __filename, typeof __dirname, typeof __node_run_main, typeof __host_main_kind)";
    let want = "undefined undefined undefined undefined undefined undefined undefined";
    let cwd = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(stdout(&jsse(&["-e", probe], cwd, None)), want);
    let dir = scratch("gate");
    let empty = dir.0.join("empty.js");
    std::fs::write(&empty, "void 0;").unwrap();
    let out = jsse(
        &["--prelude", empty.to_str().unwrap(), "-e", probe],
        cwd,
        None,
    );
    assert_eq!(stdout(&out), want);
    let script = dir.0.join("plain.js");
    std::fs::write(&script, format!("{probe}; print(this === globalThis)")).unwrap();
    let out = jsse(&[script.to_str().unwrap()], cwd, None);
    assert_eq!(stdout(&out), format!("{want}\ntrue"));
}

#[test]
fn registries_and_entry_points_are_private() {
    let dir = scratch("private");
    let main = dir.0.join("main.js");
    std::fs::write(
        &main,
        r#"
        const names = ["__node_run_main", "__node_builtin", "__node_builtins", "__host_main_kind", "__host_builtin_source"];
        print(names.map((n) => typeof globalThis[n]).join(), typeof globalThis[Symbol.for("jsse.node.builtins")]);
        "#,
    )
    .unwrap();
    let cwd = &dir.0;
    let out = jsse(&["--node", main.to_str().unwrap()], cwd, None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "undefined,undefined,undefined,undefined,undefined undefined"
    );
    let eval = jsse(
        &[
            "--node",
            "-e",
            r#"print(["__node_run_main", "__node_builtin", "__host_main_kind"].map((n) => typeof globalThis[n]).join())"#,
        ],
        cwd,
        None,
    );
    assert_eq!(stdout(&eval), "undefined,undefined,undefined");
}

#[test]
fn builtin_registries_unify_behind_require() {
    let source = r#"
      const fs = require("fs");
      print(typeof fs.readFileSync, require("node:fs") === fs, require("path") === require("node:path"));
      print(require.resolve("fs"), require.resolve("node:path"));
      const timers = require("timers");
      print(require("node:timers") === timers, require("timers/promises") === timers.promises, timers.setTimeout === setTimeout);
      for (const id of ["http", "node:fs/nope", "fs/"]) {
        try { require(id); print("no throw", id); } catch (e) { print(id, e.code); }
      }
    "#;
    let out = jsse(&["--node", "-e", source], &tree(), None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "function true true\nfs node:path\ntrue true true\nhttp MODULE_NOT_FOUND\nnode:fs/nope ERR_UNKNOWN_BUILTIN_MODULE\nfs/ MODULE_NOT_FOUND"
    );
}

#[test]
fn the_main_module_runs_as_commonjs() {
    let dir = scratch("main");
    let main = dir.0.join("app");
    std::fs::create_dir_all(&main).unwrap();
    std::fs::write(main.join("dep.js"), "exports.dep = __filename;").unwrap();
    std::fs::write(
        main.join("app.js"),
        r#"#!/usr/bin/env node
        var v = 1;
        print(this === module.exports, typeof globalThis.v, require.main === module, module.id, process.argv[1] === __filename);
        print(JSON.stringify(process.argv.slice(2)), require("./dep").dep === __dirname + "/dep.js");
        process.exitCode = 3;
        "#,
    )
    .unwrap();
    let script = main.join("app.js");
    let out = jsse(
        &["--node", script.to_str().unwrap(), "x", "--", "-y"],
        &dir.0,
        None,
    );
    assert_eq!(out.status.code(), Some(3), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "true undefined true . true\n[\"x\",\"--\",\"-y\"] true"
    );
}

#[test]
fn a_main_module_without_extension_and_a_missing_one() {
    let dir = scratch("noext");
    std::fs::write(dir.0.join("app.js"), "print('ran');").unwrap();
    let bare = dir.0.join("app");
    let out = jsse(&["--node", bare.to_str().unwrap()], &dir.0, None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "ran");

    let missing = dir.0.join("missing.js");
    let out = jsse(&["--node", missing.to_str().unwrap()], &dir.0, None);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("Cannot find module"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn errors_thrown_by_the_main_module_exit_1() {
    let dir = scratch("throw");
    let script = dir.0.join("t.js");
    std::fs::write(&script, "print('before'); throw new TypeError('boom');").unwrap();
    let out = jsse(&["--node", script.to_str().unwrap()], &dir.0, None);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "before");
    assert!(stderr(&out).contains("boom"), "{}", stderr(&out));
}

#[test]
fn es_modules_keep_running_as_modules_and_cannot_be_required() {
    let dir = scratch("esm");
    let script = dir.0.join("m.mjs");
    std::fs::write(
        &script,
        "print(typeof require, typeof module, typeof __filename, import.meta.url.startsWith('file:'));",
    )
    .unwrap();
    let out = jsse(&["--node", script.to_str().unwrap()], &dir.0, None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "undefined undefined undefined true");

    std::fs::write(dir.0.join("lib.mjs"), "export default 1;").unwrap();
    std::fs::create_dir_all(dir.0.join("pkg")).unwrap();
    std::fs::write(dir.0.join("pkg/package.json"), r#"{"type":"module"}"#).unwrap();
    std::fs::write(dir.0.join("pkg/esm.js"), "export default 1;").unwrap();
    let probe = r#"
      for (const id of ["./lib.mjs", "./pkg/esm.js"]) {
        try { require(id); } catch (e) { print(id, e.code); }
      }
    "#;
    let out = jsse(&["--node", "-e", probe], &dir.0, None);
    assert_eq!(
        stdout(&out),
        "./lib.mjs ERR_REQUIRE_ESM\n./pkg/esm.js ERR_REQUIRE_ESM"
    );
}

#[test]
fn symlinked_modules_are_cached_by_real_path() {
    let dir = scratch("symlink");
    std::fs::create_dir_all(dir.0.join("real")).unwrap();
    std::fs::write(
        dir.0.join("real/m.js"),
        "exports.n = (globalThis.n = (globalThis.n || 0) + 1);",
    )
    .unwrap();
    std::os::unix::fs::symlink(dir.0.join("real"), dir.0.join("link")).unwrap();
    let probe = r#"
      const a = require("./real/m"), b = require("./link/m");
      print(a === b, a.n, require.resolve("./link/m") === __dirname_real + "/real/m.js");
    "#;
    let source = format!(
        "const __dirname_real = {:?};\n{probe}",
        dir.0.to_str().unwrap()
    );
    let out = jsse(&["--node", "-e", &source], &dir.0, None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "true 1 true");
}

#[test]
fn the_loader_survives_intrinsic_tampering_in_user_code() {
    let dir = scratch("tamper");
    std::fs::write(dir.0.join("dep.js"), "module.exports = 'dep';").unwrap();
    let source = r#"
      String.prototype.startsWith = function () { return true; };
      Object.keys = function () { return []; };
      JSON.parse = function () { return "tampered"; };
      Function = function () { throw new Error("Function"); };
      print(require("./dep"), require("node:path").join("a", "b"), require("./dep.js") === require("./dep"));
    "#;
    let out = jsse(&["--node", "-e", source], &dir.0, None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "dep a/b true");
}

#[test]
fn a_main_script_in_a_type_module_package_runs_as_an_es_module() {
    let dir = scratch("typemodule");
    std::fs::write(dir.0.join("package.json"), "{\n  \"type\": \"module\"\n}").unwrap();
    std::fs::write(
        dir.0.join("main.js"),
        "print(typeof require, typeof module, import.meta.url.startsWith('file:'));",
    )
    .unwrap();
    std::fs::write(
        dir.0.join("legacy.cjs"),
        "print(typeof require, this === module.exports);",
    )
    .unwrap();
    std::fs::write(
        dir.0.join("app"),
        "print(typeof require, import.meta.url.startsWith('file:'));",
    )
    .unwrap();
    let out = jsse(
        &["--node", dir.0.join("main.js").to_str().unwrap()],
        &dir.0,
        None,
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "undefined undefined true");
    let out = jsse(
        &["--node", dir.0.join("app").to_str().unwrap()],
        &dir.0,
        None,
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "undefined true");
    let out = jsse(
        &["--node", dir.0.join("legacy.cjs").to_str().unwrap()],
        &dir.0,
        None,
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "function true");
}

#[test]
fn the_module_sync_condition_is_skipped_until_require_of_esm_exists() {
    let dir = scratch("modulesync");
    let pkg = dir.0.join("node_modules/ms");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(
        pkg.join("package.json"),
        r#"{"name":"ms","exports":{"module-sync":"./a.mjs","default":"./b.js"}}"#,
    )
    .unwrap();
    std::fs::write(pkg.join("a.mjs"), "export default 'esm';").unwrap();
    std::fs::write(pkg.join("b.js"), "module.exports = 'cjs';").unwrap();
    let out = jsse(&["--node", "-e", "print(require('ms'))"], &dir.0, None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "cjs");
}

#[test]
fn create_require_accepts_paths_and_file_urls() {
    let dir = scratch("createrequire");
    std::fs::write(dir.0.join("dep.js"), "module.exports = 'dep';").unwrap();
    let d = dir.0.to_str().unwrap();
    let probe = format!(
        r#"
        const create = module.constructor.createRequire;
        const viaPath = create({d:?} + "/x.js"), viaUrl = create("file://" + {d:?} + "/x.js");
        const viaObject = create(new URL("file://" + {d:?} + "/dir/"));
        print(viaPath("./dep"), viaUrl("./dep"), viaObject("../dep"), viaPath.resolve("./dep") === {d:?} + "/dep.js");
        try {{ create("relative.js"); }} catch (e) {{ print(e.code); }}
        "#
    );
    let out = jsse(&["--node", "-e", &probe], &dir.0, None);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "dep dep dep true\nERR_INVALID_ARG_VALUE");
}
