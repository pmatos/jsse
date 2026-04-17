use super::*;
use crate::parser::Parser;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn parse_program(source: &str) -> Program {
    let mut parser = Parser::new(source).expect("parser init");
    parser.parse_program().expect("parse program")
}

fn parse_module_program(source: &str) -> Program {
    let mut parser = Parser::new(source).expect("parser init");
    parser
        .parse_program_as_module()
        .expect("parse module program")
}

fn run_script(source: &str) -> Interpreter {
    let program = parse_program(source);
    let mut interp = Interpreter::new();
    let result = interp.run(&program);
    assert!(
        matches!(result, Completion::Normal(_) | Completion::Empty),
        "unexpected completion: {result:?}"
    );
    interp
}

fn run_with_path(source: &str, path: &Path) -> Interpreter {
    let program = parse_program(source);
    let mut interp = Interpreter::new();
    let result = interp.run_with_path(&program, path);
    assert!(
        matches!(result, Completion::Normal(_) | Completion::Empty),
        "unexpected completion: {result:?}"
    );
    interp
}

fn run_module_with_path(source: &str, path: &Path) -> Interpreter {
    let program = parse_module_program(source);
    let mut interp = Interpreter::new();
    let result = interp.run_with_path(&program, path);
    assert!(
        matches!(result, Completion::Normal(_) | Completion::Empty),
        "unexpected completion: {result:?}"
    );
    interp
}

fn global_string(interp: &Interpreter, name: &str) -> String {
    let env = interp.realm().global_env.borrow();
    match env.get(name).unwrap_or(JsValue::Undefined) {
        JsValue::String(s) => s.to_string(),
        other => panic!("expected global string for {name}, got {other:?}"),
    }
}

fn temp_case_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "jsse-runtime-tests-{label}-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_case_file(dir: &Path, name: &str, source: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, source).expect("write module file");
    path
}

#[test]
fn microtask_queue_drains_before_run_returns() {
    let interp = run_script(
        r#"
        var result = "pending";
        Promise.resolve().then(() => { result = "done"; });
        "#,
    );
    assert_eq!(global_string(&interp, "result"), "done");
    assert!(interp.microtask_queue.is_empty());
}

#[test]
fn nested_microtasks_run_to_quiescence_in_order() {
    let interp = run_script(
        r#"
        var order = "";
        Promise.resolve().then(() => {
          order += "a";
          Promise.resolve().then(() => { order += "c"; });
        });
        Promise.resolve().then(() => { order += "b"; });
        "#,
    );
    assert_eq!(global_string(&interp, "order"), "abc");
    assert!(interp.microtask_queue.is_empty());
}

#[test]
fn dynamic_import_uses_run_path_during_microtask_drain() {
    let dir = temp_case_dir("dynamic-import");
    let main_path = write_case_file(
        &dir,
        "main.js",
        r#"
        globalThis.imported = "pending";
        import("./dep.js").then(ns => { globalThis.imported = ns.value; });
        "#,
    );
    write_case_file(&dir, "dep.js", r#"export const value = "loaded";"#);

    let interp = run_with_path(&fs::read_to_string(&main_path).unwrap(), &main_path);
    assert_eq!(global_string(&interp, "imported"), "loaded");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn dynamic_import_keeps_resolvers_alive_across_gc_during_module_evaluation() {
    let dir = temp_case_dir("dynamic-import-gc");
    let main_path = write_case_file(
        &dir,
        "main.js",
        r#"
        globalThis.imported = "pending";
        import("./dep.js").then(ns => { globalThis.imported = ns.value; });
        $262.gc();
        "#,
    );
    write_case_file(&dir, "dep.js", r#"export const value = "loaded";"#);

    let interp = run_module_with_path(&fs::read_to_string(&main_path).unwrap(), &main_path);
    assert_eq!(global_string(&interp, "imported"), "loaded");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn module_cycle_preserves_live_bindings_and_reuses_registry_entries() {
    let dir = temp_case_dir("module-cycle");
    let main_path = write_case_file(
        &dir,
        "main.js",
        r#"
        import { valueA, bumpA } from "./a.js";
        import { valueB, bumpB, readA } from "./b.js";
        bumpA();
        bumpB();
        globalThis.summary = String(valueA) + "," + String(valueB) + "," + String(readA());
        "#,
    );
    write_case_file(
        &dir,
        "a.js",
        r#"
        import { valueB } from "./b.js";
        export let valueA = 1;
        export function bumpA() { valueA += 1; }
        export function readB() { return valueB; }
        "#,
    );
    write_case_file(
        &dir,
        "b.js",
        r#"
        import { valueA } from "./a.js";
        export let valueB = 10;
        export function bumpB() { valueB += valueA; }
        export function readA() { return valueA; }
        "#,
    );

    let interp = run_module_with_path(&fs::read_to_string(&main_path).unwrap(), &main_path);
    assert_eq!(global_string(&interp, "summary"), "2,12,2");
    assert_eq!(interp.module_registry.len(), 3);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn dynamic_import_waits_for_async_module_fulfillment_in_leaf_to_root_order() {
    let dir = temp_case_dir("issue-79-fulfillment");
    let main_path = write_case_file(
        &dir,
        "main.js",
        r#"
        import { p1, pA_start, pB_start } from "./setup.js";

        globalThis.result = "pending";
        let logs = [];
        const importsP = Promise.all([
          pB_start.promise
            .then(() => import("./a.js").finally(() => logs.push("A")))
            .catch(() => {}),
          import("./b.js").finally(() => logs.push("B")).catch(() => {}),
        ]);

        Promise.all([pA_start.promise, pB_start.promise]).then(p1.resolve);
        importsP.then(() => { globalThis.result = logs.join(","); });
        "#,
    );
    write_case_file(
        &dir,
        "setup.js",
        r#"
        export const p1 = Promise.withResolvers();
        export const pA_start = Promise.withResolvers();
        export const pB_start = Promise.withResolvers();
        "#,
    );
    write_case_file(
        &dir,
        "a.js",
        r#"
        import "./a-sentinel.js";
        import "./b.js";
        "#,
    );
    write_case_file(
        &dir,
        "a-sentinel.js",
        r#"
        import { pA_start } from "./setup.js";
        pA_start.resolve();
        "#,
    );
    write_case_file(
        &dir,
        "b.js",
        r#"
        import "./b-sentinel.js";
        import { p1 } from "./setup.js";
        await p1.promise;
        "#,
    );
    write_case_file(
        &dir,
        "b-sentinel.js",
        r#"
        import { pB_start } from "./setup.js";
        pB_start.resolve();
        "#,
    );

    let interp = run_module_with_path(&fs::read_to_string(&main_path).unwrap(), &main_path);
    assert_eq!(global_string(&interp, "result"), "B,A");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn dynamic_import_waits_for_async_module_rejection_in_leaf_to_root_order() {
    let dir = temp_case_dir("issue-79-rejection");
    let main_path = write_case_file(
        &dir,
        "main.js",
        r#"
        import { p1, pA_start, pB_start } from "./setup.js";

        globalThis.result = "pending";
        let logs = [];
        const importsP = Promise.all([
          pB_start.promise
            .then(() => import("./a.js").finally(() => logs.push("A")))
            .catch(() => {}),
          import("./b.js").finally(() => logs.push("B")).catch(() => {}),
        ]);

        Promise.all([pA_start.promise, pB_start.promise]).then(p1.reject);
        importsP.then(() => { globalThis.result = logs.join(","); });
        "#,
    );
    write_case_file(
        &dir,
        "setup.js",
        r#"
        export const p1 = Promise.withResolvers();
        export const pA_start = Promise.withResolvers();
        export const pB_start = Promise.withResolvers();
        "#,
    );
    write_case_file(
        &dir,
        "a.js",
        r#"
        import "./a-sentinel.js";
        import "./b.js";
        "#,
    );
    write_case_file(
        &dir,
        "a-sentinel.js",
        r#"
        import { pA_start } from "./setup.js";
        pA_start.resolve();
        "#,
    );
    write_case_file(
        &dir,
        "b.js",
        r#"
        import "./b-sentinel.js";
        import { p1 } from "./setup.js";
        await p1.promise;
        "#,
    );
    write_case_file(
        &dir,
        "b-sentinel.js",
        r#"
        import { pB_start } from "./setup.js";
        pB_start.resolve();
        "#,
    );

    let interp = run_module_with_path(&fs::read_to_string(&main_path).unwrap(), &main_path);
    assert_eq!(global_string(&interp, "result"), "B,A");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn gc_keeps_microtask_roots_alive_until_queue_is_cleared() {
    let mut interp = Interpreter::new();
    let obj = interp.create_object();
    let id = obj.borrow().id.expect("object id");
    let obj_val = JsValue::Object(crate::types::JsObject { id });

    interp.microtask_queue.push((
        vec![obj_val.clone()],
        Box::new(|_| Completion::Normal(JsValue::Undefined)),
    ));
    interp.gc_requested = true;
    interp.gc_safepoint();
    assert!(
        interp.get_object(id).is_some(),
        "microtask root should keep object alive"
    );

    interp.microtask_queue.clear();
    interp.gc_requested = true;
    interp.gc_safepoint();
    assert!(
        interp.get_object(id).is_none(),
        "object should be collectable after queue clears"
    );
}

#[test]
fn gc_keeps_module_exports_alive_until_registry_entry_is_removed() {
    let dir = temp_case_dir("module-gc");
    let main_path = write_case_file(&dir, "main.js", r#"export const obj = { marker: 1 };"#);

    let mut interp = run_module_with_path(&fs::read_to_string(&main_path).unwrap(), &main_path);
    let canon = main_path.canonicalize().unwrap_or(main_path.clone());
    let module = interp
        .module_registry
        .get(&canon)
        .expect("module registry entry")
        .clone();
    let export_val = module
        .borrow()
        .exports
        .get("obj")
        .expect("module export")
        .clone();
    let JsValue::Object(obj_ref) = export_val else {
        panic!("expected exported object");
    };

    interp.gc_requested = true;
    interp.gc_safepoint();
    assert!(interp.get_object(obj_ref.id).is_some());

    interp.module_registry.remove(&canon);
    interp.gc_requested = true;
    interp.gc_safepoint();
    assert!(interp.get_object(obj_ref.id).is_none());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn shared_array_buffer_atomics_smoke() {
    let interp = run_script(
        r#"
        var result = "";
        let sab = new SharedArrayBuffer(16);
        let view = new Int32Array(sab);
        Atomics.store(view, 0, 3);
        Atomics.add(view, 0, 4);
        result = String(Atomics.load(view, 0));
        "#,
    );
    assert_eq!(global_string(&interp, "result"), "7");
}

#[test]
fn typed_array_clamps_assigned_values() {
    let interp = run_script(
        r#"
        var result = "";
        let view = new Uint8ClampedArray(1);
        view[0] = 300;
        result = String(view[0]);
        "#,
    );
    assert_eq!(global_string(&interp, "result"), "255");
}
