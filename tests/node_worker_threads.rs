//! The single-thread `worker_threads` module under `--node` (#1294): the main thread's identity, the
//! environment-data store and the untransferable marks work; `new Worker` fails as on a platform without
//! the feature. ESLint loads the module for `isMainThread`/`threadId`; the end-to-end check is the
//! `eslint-*` cases of `scripts/run-ts-mocha-oracle.py`.

use std::path::Path;
use std::process::Command;

fn run(source: &str, node: bool) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(node.then_some("--node"))
        .args(["-e", source])
        .output()
        .expect("run jsse");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

#[test]
fn it_is_a_builtin_reached_with_and_without_the_node_prefix() {
    assert_eq!(
        run(
            "const w = require('worker_threads');\
             console.log(require('node:worker_threads') === w,\
             require('module').builtinModules.includes('worker_threads'),\
             require('module').isBuiltin('node:worker_threads'))",
            true
        ),
        "true true true"
    );
}

#[test]
fn the_main_thread_has_no_parent_and_no_data() {
    assert_eq!(
        run(
            "const w = require('worker_threads');\
             console.log(w.isMainThread, w.isInternalThread, w.threadId, w.threadName,\
             w.parentPort, w.workerData, JSON.stringify(w.resourceLimits),\
             typeof w.SHARE_ENV, w.SHARE_ENV === Symbol.for('nodejs.worker_threads.SHARE_ENV'))",
            true
        ),
        "true false 0 main null null {} symbol true"
    );
}

#[test]
fn environment_data_is_a_keyed_store_and_undefined_deletes() {
    assert_eq!(
        run(
            "const w = require('worker_threads');\
             const log = [];\
             log.push(w.getEnvironmentData('k'));\
             w.setEnvironmentData('k', { v: 1 });\
             log.push(JSON.stringify(w.getEnvironmentData('k')));\
             w.setEnvironmentData('k', undefined);\
             log.push(w.getEnvironmentData('k'));\
             console.log(log.map(String).join(' '))",
            true
        ),
        "undefined {\"v\":1} undefined"
    );
}

#[test]
fn untransferable_marks_apply_to_objects_only() {
    assert_eq!(
        run(
            "const w = require('worker_threads');\
             const o = {}; const f = () => {};\
             const before = [w.isMarkedAsUntransferable(o), w.isMarkedAsUntransferable(1)];\
             w.markAsUntransferable(o); w.markAsUntransferable(f); w.markAsUntransferable(1);\
             console.log(before, w.isMarkedAsUntransferable(o), w.isMarkedAsUntransferable(f),\
             w.isMarkedAsUntransferable(1))",
            true
        ),
        "[ false, false ] true true false"
    );
}

#[test]
fn constructing_a_worker_fails_as_an_unavailable_feature() {
    assert_eq!(
        run(
            "const { Worker } = require('worker_threads');\
             try { new Worker('./x.js'); } catch (e) { console.log(e.name, e.code, e.message); }",
            true
        ),
        "TypeError ERR_FEATURE_UNAVAILABLE_ON_PLATFORM The feature Worker is unavailable on the \
         current platform, which is being used to run Node.js"
    );
}

#[test]
fn the_module_does_not_exist_outside_node_mode() {
    assert_eq!(
        run("console.log(typeof require, typeof process)", false),
        "undefined undefined"
    );
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, text).expect("write");
}

#[test]
fn esbuild_keeps_its_one_shot_child_fallback_for_the_sync_api() {
    let dir = std::env::temp_dir().join(format!("jsse-worker-threads-{}", std::process::id()));
    let probe = "let r; try { r = typeof require('worker_threads').Worker; } catch (e) { r = e.code; }\n\
                 module.exports = r;\n";
    write(&dir.join("node_modules/esbuild/lib/main.js"), probe);
    write(
        &dir.join("node_modules/esbuild/package.json"),
        r#"{"main": "lib/main.js"}"#,
    );
    write(&dir.join("node_modules/other/index.js"), probe);
    write(
        &dir.join("main.js"),
        "console.log(require('esbuild'), require('other'));\n",
    );
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(["--node"])
        .arg(dir.join("main.js"))
        .output()
        .expect("run jsse");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        "MODULE_NOT_FOUND function"
    );
}
