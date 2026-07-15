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
    match interp
        .get_global_var_ref(name)
        .unwrap_or(JsValue::Undefined)
    {
        JsValue::String(s) => s.to_string(),
        other => panic!("expected global string for {name}, got {other:?}"),
    }
}

fn global_number(interp: &Interpreter, name: &str) -> f64 {
    match interp
        .get_global_var_ref(name)
        .unwrap_or(JsValue::Undefined)
    {
        JsValue::Number(n) => n,
        other => panic!("expected global number for {name}, got {other:?}"),
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
fn define_method_installs_a_correctly_shaped_builtin() {
    let mut interp = Interpreter::new();
    let target_id = interp.create_object_id();

    interp.define_method(target_id, "greet", 1, |_interp, _this, args| {
        let name = match args.first() {
            Some(JsValue::String(s)) => s.to_rust_string(),
            _ => "world".to_string(),
        };
        Completion::Normal(JsValue::String(JsString::from_str(&format!(
            "hello {name}"
        ))))
    });

    let desc = interp
        .get_object_cell_expect(target_id)
        .borrow()
        .get_own_property("greet")
        .expect("greet property installed");
    assert_eq!(desc.writable, Some(true), "builtins must stay writable");
    assert_eq!(
        desc.enumerable,
        Some(false),
        "builtins must not be enumerable"
    );
    assert_eq!(
        desc.configurable,
        Some(true),
        "builtins must stay configurable"
    );
    let greet_fn = desc.value.expect("greet has a function value");

    // define_method must still route through create_function, so name/length bookkeeping
    // (used by Function.prototype.toString, .length, etc.) isn't lost.
    let JsValue::Object(fn_obj) = &greet_fn else {
        panic!("expected greet to be a function object")
    };
    let fn_cell = interp.get_object_cell_expect(fn_obj.id);
    match fn_cell.borrow().get_own_property("name").unwrap().value {
        Some(JsValue::String(ref s)) => assert_eq!(s.to_rust_string(), "greet"),
        ref other => panic!("expected name string, got {other:?}"),
    }
    match fn_cell.borrow().get_own_property("length").unwrap().value {
        Some(JsValue::Number(n)) => assert_eq!(n, 1.0),
        ref other => panic!("expected length number, got {other:?}"),
    }

    let target_val = JsValue::Object(crate::types::JsObject { id: target_id });
    let result = interp.call_function(
        &greet_fn,
        &target_val,
        &[JsValue::String(JsString::from_str("jsse"))],
    );
    match result {
        Completion::Normal(JsValue::String(s)) => assert_eq!(s.to_rust_string(), "hello jsse"),
        other => panic!("unexpected completion: {other:?}"),
    }
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
    assert!(interp.scheduler.microtask_queue_is_empty());
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
    assert!(interp.scheduler.microtask_queue_is_empty());
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
fn module_top_level_call_and_member_evaluate() {
    let dir = temp_case_dir("module-ic-fallback");
    let main_path = write_case_file(
        &dir,
        "main.mjs",
        r#"
        globalThis.f = function() { return 42; };
        globalThis.m = { n: 7 };
        globalThis.result = globalThis.f() + globalThis.m.n;
        export const dummy = 1;
        "#,
    );

    let interp = run_module_with_path(&fs::read_to_string(&main_path).unwrap(), &main_path);
    assert_eq!(global_number(&interp, "result"), 49.0);

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
fn transitive_module_import_link_error_aborts_parent_before_evaluation() {
    let dir = temp_case_dir("module-link-import-error");
    let main_path = write_case_file(
        &dir,
        "main.mjs",
        r#"
        import "./broken.mjs";
        globalThis.marker = "ran";
        "#,
    );
    let broken_path = write_case_file(
        &dir,
        "broken.mjs",
        r#"
        import { nonExistent } from "./broken.mjs";
        "#,
    );

    let program = parse_module_program(&fs::read_to_string(&main_path).unwrap());
    let mut interp = Interpreter::new();
    let result = interp.run_with_path(&program, &main_path);

    let err = match result {
        Completion::Throw(err) => interp.format_value(&err),
        other => panic!("expected module linking error, got {other:?}"),
    };
    assert!(err.contains("SyntaxError"), "unexpected error: {err}");
    assert!(err.contains("nonExistent"), "unexpected error: {err}");
    assert!(interp.get_global_var_ref("marker").is_none());

    let broken_canon = broken_path.canonicalize().unwrap_or(broken_path.clone());
    let realm_id = interp.current_realm_id;
    let cached = interp
        .module_registry
        .get(&(realm_id, broken_canon))
        .expect("broken module registry entry")
        .borrow()
        .error
        .clone()
        .expect("cached module error");
    let cached_text = interp.format_value(&cached);
    assert!(
        cached_text.contains("SyntaxError"),
        "unexpected cached error: {cached_text}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn transitive_reexport_link_error_aborts_parent_before_evaluation() {
    let dir = temp_case_dir("module-link-reexport-error");
    let main_path = write_case_file(
        &dir,
        "main.mjs",
        r#"
        export {} from "./a.mjs";
        globalThis.marker = "ran";
        "#,
    );
    write_case_file(
        &dir,
        "a.mjs",
        r#"
        export * from "./broken.mjs";
        "#,
    );
    write_case_file(
        &dir,
        "broken.mjs",
        r#"
        import { nonExistent } from "./broken.mjs";
        export const ok = 1;
        "#,
    );

    let program = parse_module_program(&fs::read_to_string(&main_path).unwrap());
    let mut interp = Interpreter::new();
    let result = interp.run_with_path(&program, &main_path);

    let err = match result {
        Completion::Throw(err) => interp.format_value(&err),
        other => panic!("expected module linking error, got {other:?}"),
    };
    assert!(err.contains("SyntaxError"), "unexpected error: {err}");
    assert!(err.contains("nonExistent"), "unexpected error: {err}");
    assert!(interp.get_global_var_ref("marker").is_none());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn default_cannot_be_reexported_through_star_resolution() {
    let dir = temp_case_dir("module-default-through-star");
    let main_path = write_case_file(
        &dir,
        "main.mjs",
        r#"
        export { default } from "./indirect.mjs";
        globalThis.marker = "ran";
        "#,
    );
    write_case_file(
        &dir,
        "indirect.mjs",
        r#"
        export * from "./defaulted.mjs";
        "#,
    );
    write_case_file(
        &dir,
        "defaulted.mjs",
        r#"
        const x = 1;
        export { x as default };
        "#,
    );

    let program = parse_module_program(&fs::read_to_string(&main_path).unwrap());
    let mut interp = Interpreter::new();
    let result = interp.run_with_path(&program, &main_path);

    let err = match result {
        Completion::Throw(err) => interp.format_value(&err),
        other => panic!("expected module linking error, got {other:?}"),
    };
    assert!(err.contains("SyntaxError"), "unexpected error: {err}");
    assert!(err.contains("default"), "unexpected error: {err}");
    assert!(interp.get_global_var_ref("marker").is_none());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn missing_named_module_import_throws_syntax_error() {
    let dir = temp_case_dir("module-missing-named-import");
    let main_path = write_case_file(
        &dir,
        "main.js",
        r#"
        import { missing } from "./dep.js";
        globalThis.value = missing;
        "#,
    );
    write_case_file(&dir, "dep.js", r#"export const present = 1;"#);

    let program = parse_module_program(&fs::read_to_string(&main_path).unwrap());
    let mut interp = Interpreter::new();
    let result = interp.run_with_path(&program, &main_path);
    let err = match result {
        Completion::Throw(err) => err,
        other => panic!("expected thrown completion, got {other:?}"),
    };
    let message = interp.format_value(&err);
    assert!(message.starts_with("SyntaxError: "));
    assert!(message.contains("has no export named 'missing'"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn gc_keeps_microtask_roots_alive_until_queue_is_cleared() {
    let mut interp = Interpreter::new();
    let id = interp.create_object_id();
    let obj_val = JsValue::Object(crate::types::JsObject { id });

    interp.scheduler.enqueue_microtask((
        vec![obj_val.clone()],
        Box::new(|_| Completion::Normal(JsValue::Undefined)),
    ));
    interp.gc.request();
    interp.gc_safepoint();
    assert!(
        interp.get_object_cell(id).is_some(),
        "microtask root should keep object alive"
    );

    interp.scheduler.clear_microtasks();
    interp.gc.request();
    interp.gc_safepoint();
    assert!(
        interp.get_object_cell(id).is_none(),
        "object should be collectable after queue clears"
    );
}

#[test]
fn gc_keeps_module_exports_alive_until_registry_entry_is_removed() {
    let dir = temp_case_dir("module-gc");
    let main_path = write_case_file(&dir, "main.js", r#"export const obj = { marker: 1 };"#);

    let mut interp = run_module_with_path(&fs::read_to_string(&main_path).unwrap(), &main_path);
    let canon = main_path.canonicalize().unwrap_or(main_path.clone());
    let realm_id = interp.current_realm_id;
    let key = (realm_id, canon.clone());
    let module = interp
        .module_registry
        .get(&key)
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

    interp.gc.request();
    interp.gc_safepoint();
    assert!(interp.get_object_cell(obj_ref.id).is_some());

    interp.module_registry.remove(&key);
    interp.gc.request();
    interp.gc_safepoint();
    assert!(interp.get_object_cell(obj_ref.id).is_none());

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

/// Regression test for PR 1b.2 (#105): prototype chain survives GC.
/// Builds chain a -> b -> c -> d, stashes a reference to d, allocates
/// many throwaway objects to force safepoints, then asserts the chain
/// still resolves and returns d's own property via walk.
#[test]
fn prototype_chain_survives_gc() {
    let interp = run_script(
        r#"
        var d = { marker: "deep" };
        var c = Object.create(d);
        var b = Object.create(c);
        var a = Object.create(b);
        // force many allocations to trigger gc_safepoint
        var sink = [];
        for (var i = 0; i < 20000; i++) {
            sink.push({ k: i });
        }
        sink = null;
        // prototype walk must still resolve
        var resolved = a.marker;
        // own-property access on d must still work
        var direct = d.marker;
        var result = resolved + "|" + direct;
        "#,
    );
    assert_eq!(global_string(&interp, "result"), "deep|deep");
}

// -- Phase 1: shape-id infrastructure (issue #71) -----------------------------
// Whitebox tests for the per-object shape_id counter that backs the inline
// caches added in Phases 2 & 3. Each structural mutation must advance the
// counter; pure value re-assignment must not.

#[test]
fn alloc_seeds_unique_shape_ids() {
    let mut interp = Interpreter::new();
    let id_a = interp.alloc_object(JsObjectData::new());
    let id_b = interp.alloc_object(JsObjectData::new());
    let shape_a = interp.get_object(id_a).unwrap().borrow().shape_id;
    let shape_b = interp.get_object(id_b).unwrap().borrow().shape_id;
    assert_ne!(shape_a, 0, "shape_id must be non-zero after alloc");
    assert_ne!(shape_b, 0, "shape_id must be non-zero after alloc");
    assert_ne!(shape_a, shape_b, "fresh allocations get distinct shape ids");
}

#[test]
fn mutate_object_shape_bumps_shape_id() {
    let mut interp = Interpreter::new();
    let id = interp.alloc_object(JsObjectData::new());
    let before = interp.get_object(id).unwrap().borrow().shape_id;
    let returned = interp.mutate_object_shape(id, |obj| {
        // helper must bump regardless of what closure does (or doesn't do).
        obj.extensible = false;
        42_u32
    });
    assert_eq!(returned, 42, "closure return value must be propagated");
    let after = interp.get_object(id).unwrap().borrow().shape_id;
    assert!(after > before, "mutate_object_shape must advance shape_id");
}

#[test]
fn mutate_object_shape_bumps_even_on_noop_closure() {
    let mut interp = Interpreter::new();
    let id = interp.alloc_object(JsObjectData::new());
    let before = interp.get_object(id).unwrap().borrow().shape_id;
    interp.mutate_object_shape(id, |_obj| {});
    let after = interp.get_object(id).unwrap().borrow().shape_id;
    assert!(
        after > before,
        "mutate_object_shape bumps unconditionally — \
         a no-op closure still produces a fresh shape id (per Step 4 of the plan)"
    );
}

#[test]
fn set_property_value_add_bumps_shape() {
    let mut interp = Interpreter::new();
    let id = interp.alloc_object(JsObjectData::new());
    let before = interp.get_object(id).unwrap().borrow().shape_id;
    let ok = interp
        .get_object(id)
        .unwrap()
        .borrow_mut()
        .set_property_value("x", JsValue::Number(1.0));
    assert!(
        ok,
        "set_property_value should succeed on extensible empty obj"
    );
    let after = interp.get_object(id).unwrap().borrow().shape_id;
    assert!(
        after > before,
        "adding a new property is a structural mutation; shape_id must advance \
         (before={before}, after={after})"
    );
}

#[test]
fn set_property_value_update_existing_does_not_bump_shape() {
    let mut interp = Interpreter::new();
    let id = interp.alloc_object(JsObjectData::new());
    interp
        .get_object(id)
        .unwrap()
        .borrow_mut()
        .set_property_value("x", JsValue::Number(1.0));
    let before = interp.get_object(id).unwrap().borrow().shape_id;
    interp
        .get_object(id)
        .unwrap()
        .borrow_mut()
        .set_property_value("x", JsValue::Number(2.0));
    let after = interp.get_object(id).unwrap().borrow().shape_id;
    assert_eq!(
        after, before,
        "reassigning an existing data property's value is NOT a structural \
         mutation; shape_id must remain stable so IC slots stay live"
    );
}

#[test]
fn define_own_property_bumps_shape_on_attribute_change() {
    let mut interp = Interpreter::new();
    let id = interp.alloc_object(JsObjectData::new());
    // Seed an existing property so we can flip its attributes.
    interp
        .get_object(id)
        .unwrap()
        .borrow_mut()
        .set_property_value("x", JsValue::Number(1.0));
    let before = interp.get_object(id).unwrap().borrow().shape_id;
    let ok = interp
        .get_object(id)
        .unwrap()
        .borrow_mut()
        .define_own_property(
            "x".to_string(),
            PropertyDescriptor {
                value: Some(JsValue::Number(1.0)),
                writable: Some(false),
                enumerable: Some(true),
                configurable: Some(true),
                get: None,
                set: None,
            },
        );
    assert!(ok, "defineProperty should succeed");
    let after = interp.get_object(id).unwrap().borrow().shape_id;
    assert!(
        after > before,
        "flipping writable from default to false IS a structural mutation \
         (an attribute changed); shape_id must advance"
    );
}

#[test]
fn set_prototype_via_chokepoint_bumps_shape() {
    let mut interp = Interpreter::new();
    let proto_id = interp.alloc_object(JsObjectData::new());
    let id = interp.alloc_object(JsObjectData::new());
    let before = interp.get_object(id).unwrap().borrow().shape_id;
    interp.mutate_object_shape(id, |obj| {
        obj.prototype_id = Some(proto_id);
    });
    let after = interp.get_object(id).unwrap().borrow().shape_id;
    assert!(
        after > before,
        "prototype mutation routed via mutate_object_shape must bump shape_id"
    );
}

#[test]
fn proxy_install_via_chokepoint_bumps_shape() {
    let mut interp = Interpreter::new();
    let target_id = interp.alloc_object(JsObjectData::new());
    let handler_id = interp.alloc_object(JsObjectData::new());
    let proxy_id = interp.alloc_object(JsObjectData::new());
    let before = interp.get_object(proxy_id).unwrap().borrow().shape_id;
    interp.mutate_object_shape(proxy_id, |obj| {
        obj.kind = crate::interpreter::types::ObjectKind::Proxy(
            crate::interpreter::types::ProxyData::active(target_id, handler_id),
        );
    });
    let after = interp.get_object(proxy_id).unwrap().borrow().shape_id;
    assert!(
        after > before,
        "proxy install routed via mutate_object_shape must bump shape_id"
    );
}

#[test]
fn ic_records_after_repeated_dot_access() {
    // Hot loop reads o.x 100 times. After warmup the IC slot must be hitting,
    // not falling to the slow path on every read. This is the Phase-2 tracer
    // bullet: it asserts both correctness (sum=4200) and that the IC counter
    // advanced (proves the probe fired on cache hits, not just misses).
    let interp = run_script(
        r#"
        var o = {x: 42};
        var sum = 0;
        for (var i = 0; i < 100; i++) {
            sum += o.x;
        }
        "#,
    );
    assert_eq!(
        global_number(&interp, "sum"),
        4200.0,
        "behavioral correctness"
    );
    assert!(
        interp.ic_hit_count() > 0,
        "expected IC hits after 100-iteration hot loop on o.x; got 0 \
         (the probe never recognised the cached shape)"
    );
}

#[test]
fn ic_invalidates_on_define_property_attribute_flip() {
    // Read o.x (populates IC as OwnData), then defineProperty flips it to a
    // getter; the next read must observe the getter. If the IC ignored the
    // shape change, this would return the stale data value.
    let interp = run_script(
        r#"
        var o = {x: 1};
        var pre = o.x;
        Object.defineProperty(o, 'x', { get: function() { return 99; }, configurable: true });
        var post = o.x;
        var result = pre + "|" + post;
        "#,
    );
    assert_eq!(global_string(&interp, "result"), "1|99");
}

#[test]
fn ic_invalidates_on_delete_property() {
    // Populate IC by reading o.x repeatedly, then delete it. Subsequent reads
    // must return undefined (and continue working without panicking on a
    // stale OwnData slot).
    let interp = run_script(
        r#"
        var o = {x: 7};
        var sum = 0;
        for (var i = 0; i < 10; i++) sum += o.x;       // populate IC
        delete o.x;
        var post = (typeof o.x === "undefined") ? "gone" : "still:" + o.x;
        var result = sum + "|" + post;
        "#,
    );
    assert_eq!(global_string(&interp, "result"), "70|gone");
}

#[test]
fn ic_records_proto_data_after_repeated_dot_access() {
    // Hot loop reads `o.x` where `x` lives on the immediate prototype as a
    // data property, not on `o` itself. After warmup the depth-1 ProtoData IC
    // must be hitting (proves the probe serves the value directly from the
    // prototype without re-walking the chain on every read).
    let interp = run_script(
        r#"
        var proto = {x: 42};
        var o = Object.create(proto);
        var sum = 0;
        for (var i = 0; i < 100; i++) {
            sum += o.x;
        }
        "#,
    );
    assert_eq!(
        global_number(&interp, "sum"),
        4200.0,
        "behavioral correctness"
    );
    assert!(
        interp.ic_hit_count() > 0,
        "expected depth-1 ProtoData IC hits after 100-iteration hot loop on \
         o.x (x on the immediate prototype); got 0"
    );
}

#[test]
fn ic_proto_data_invalidates_on_prototype_swap() {
    // CRITICAL: reassigning `[[Prototype]]` does NOT bump the receiver's
    // shape_id. If the ProtoData hit arm validated only the receiver shape, it
    // would serve the OLD prototype's value after the swap. The explicit
    // `prototype_id == proto_id` guard must force a miss and re-resolution.
    let interp = run_script(
        r#"
        var a = {x: 1};
        var b = {x: 2};
        var o = Object.create(a);
        var first = 0;
        for (var i = 0; i < 20; i++) first += o.x;   // populate ProtoData on `a`
        Object.setPrototypeOf(o, b);
        var post = o.x;                              // must now see b.x === 2
        var result = first + "|" + post;
        "#,
    );
    assert_eq!(
        global_string(&interp, "result"),
        "20|2",
        "after Object.setPrototypeOf the read must reflect the new prototype, \
         not the stale cached one"
    );
}

#[test]
fn ic_proto_data_invalidates_on_proto_property_change() {
    // Mutating the prototype's property structure (delete) bumps the
    // prototype's shape_id → the cached proto_shape_id no longer matches → the
    // probe must miss and observe the new (absent) state.
    let interp = run_script(
        r#"
        var proto = {x: 5};
        var o = Object.create(proto);
        var sum = 0;
        for (var i = 0; i < 20; i++) sum += o.x;     // populate ProtoData
        delete proto.x;
        var post = (typeof o.x === "undefined") ? "gone" : "still:" + o.x;
        var result = sum + "|" + post;
        "#,
    );
    assert_eq!(global_string(&interp, "result"), "100|gone");
}

#[test]
fn ic_proto_data_shadowed_by_own_property() {
    // Adding an own property to the receiver bumps the receiver's shape_id →
    // the cached (proto-resolved) slot misses → the closer own value wins.
    let interp = run_script(
        r#"
        var proto = {x: 1};
        var o = Object.create(proto);
        var first = 0;
        for (var i = 0; i < 20; i++) first += o.x;   // populate ProtoData (=1)
        o.x = 99;                                    // shadow on the receiver
        var post = o.x;
        var result = first + "|" + post;
        "#,
    );
    assert_eq!(
        global_string(&interp, "result"),
        "20|99",
        "an own property added to the receiver must shadow the cached \
         prototype data value"
    );
}

#[test]
fn ic_proto_accessor_not_served_as_data() {
    // The property resolves on the immediate prototype as an ACCESSOR, not a
    // data descriptor. classify_for_prop_ic must NOT record it as ProtoData,
    // so every read invokes the getter (observed via the side-effect counter).
    let interp = run_script(
        r#"
        var calls = 0;
        var proto = {};
        Object.defineProperty(proto, 'x', { get: function() { calls++; return 7; }, configurable: true });
        var o = Object.create(proto);
        var sum = 0;
        for (var i = 0; i < 10; i++) sum += o.x;
        var result = sum + "|" + calls;
        "#,
    );
    assert_eq!(
        global_string(&interp, "result"),
        "70|10",
        "a prototype accessor must be invoked on every read (10 getter calls), \
         never cached as ProtoData"
    );
}

#[test]
fn call_ic_records_after_repeated_call() {
    // Phase 3 tracer: the call site `f()` repeatedly invokes the same plain
    // user function. After the first call, the IC slot must be Mono and
    // subsequent iterations must register as hits.
    let interp = run_script(
        r#"
        function f() { return 42; }
        var sum = 0;
        for (var i = 0; i < 100; i++) sum += f();
        "#,
    );
    assert_eq!(
        global_number(&interp, "sum"),
        4200.0,
        "behavioral correctness"
    );
    assert!(
        interp.call_ic_hit_count() > 0,
        "expected call-IC hits after 100-iteration hot loop on f(); got 0"
    );
}

#[test]
fn call_ic_fast_dispatch_actually_skips_entry_checks() {
    // Phase-3 follow-up tracer: a hot loop should drive IC hits AND the
    // fast-dispatch counter — proves call_function_ic_validated is the
    // path being taken, not the slow call_function. Without the fast
    // path wired up, fast_dispatch_count would stay at 0 even though
    // hit_count advanced.
    let interp = run_script(
        r#"
        function f() { return 7; }
        var sum = 0;
        for (var i = 0; i < 100; i++) sum += f();
        "#,
    );
    assert_eq!(
        global_number(&interp, "sum"),
        700.0,
        "behavioral correctness"
    );
    assert!(
        interp.call_ic_fast_dispatch_count() > 0,
        "expected fast-dispatch path to fire on IC hits; got 0 \
         (IC hits = {})",
        interp.call_ic_hit_count()
    );
}

#[test]
fn call_ic_does_not_cache_proxy_callable() {
    // Proxy with apply trap MUST always invoke the trap. classify_for_call_ic
    // returns None for proxies, so the IC slot stays Empty / Megamorphic
    // forever — every call goes through the slow entry checks. The trap
    // counter proves this.
    let interp = run_script(
        r#"
        var apply_count = 0;
        var target = function() { return 1; };
        var p = new Proxy(target, {
            apply: function(t, thisArg, args) { apply_count++; return 99; }
        });
        var sum = 0;
        for (var i = 0; i < 5; i++) sum += p();
        var result = sum + "|" + apply_count;
        "#,
    );
    assert_eq!(
        global_string(&interp, "result"),
        "495|5",
        "proxy apply trap must fire on every call regardless of IC"
    );
}

#[test]
fn call_ic_does_not_cache_class_ctor_without_new() {
    // Calling a class constructor without `new` must throw TypeError on
    // every call, even after a hot loop. The classifier excludes class
    // ctors, so the slow path always runs the is_class_ctor check.
    let interp = run_script(
        r#"
        class C { constructor() { this.x = 1; } }
        var threw_count = 0;
        for (var i = 0; i < 5; i++) {
            try { C(); } catch (e) { if (e instanceof TypeError) threw_count++; }
        }
        var result = threw_count;
        "#,
    );
    assert_eq!(
        global_number(&interp, "result"),
        5.0,
        "expected 5 TypeErrors"
    );
}

#[test]
fn call_ic_invalidates_on_function_replacement() {
    // Reassign `f` to a different function — second hot loop must observe
    // the new behavior, not the cached resolution.
    let interp = run_script(
        r#"
        function a() { return 1; }
        function b() { return 100; }
        var f = a;
        var s1 = 0; for (var i = 0; i < 5; i++) s1 += f();
        f = b;
        var s2 = 0; for (var i = 0; i < 5; i++) s2 += f();
        var result = s1 + "|" + s2;
        "#,
    );
    assert_eq!(global_string(&interp, "result"), "5|500");
}

#[test]
fn ic_polymorphic_after_two_distinct_objects_at_same_site() {
    // Same call site sees two different objects. The first miss records Mono(a);
    // the second promotes the site to a two-entry Poly([a, b]); the third
    // access re-sees `a` and must HIT the poly entry (not fall to the slow
    // path). Correctness AND the hit counter are asserted.
    let interp = run_script(
        r#"
        var a = {x: 1};
        var b = {x: 2};
        function read(o) { return o.x; }
        var v1 = read(a);   // Empty -> Mono(a)      (miss)
        var v2 = read(b);   // Mono(a) -> Poly([a,b]) (miss)
        var v3 = read(a);   // Poly hit on `a`
        var result = v1 + "|" + v2 + "|" + v3;
        "#,
    );
    assert_eq!(global_string(&interp, "result"), "1|2|1");
    assert!(
        interp.ic_hit_count() > 0,
        "the third read of `a` must hit the polymorphic slot; got 0 hits \
         (the probe never recognised the second cached shape)"
    );
}

#[test]
fn ic_polymorphic_two_shapes_hit_in_hot_loop() {
    // A site alternating between two long-lived objects must cache both and hit
    // on every steady-state access after the two-iteration warmup.
    let interp = run_script(
        r#"
        var a = {x: 10};
        var b = {x: 20};
        function read(o) { return o.x; }
        var sum = 0;
        for (var i = 0; i < 50; i++) { sum += read(a); sum += read(b); }
        "#,
    );
    assert_eq!(
        global_number(&interp, "sum"),
        1500.0,
        "behavioral correctness"
    );
    assert!(
        interp.ic_hit_count() > 0,
        "expected polymorphic IC hits across two alternating shapes; got 0"
    );
}

#[test]
fn ic_polymorphic_four_shapes_hit() {
    // Four distinct objects fill the polymorphic slot to its arity; each is
    // re-hit in steady state. Correctness plus a positive hit count prove all
    // four entries are served from the cache.
    let interp = run_script(
        r#"
        var a = {x: 1}, b = {x: 2}, c = {x: 3}, d = {x: 4};
        function read(o) { return o.x; }
        var sum = 0;
        for (var i = 0; i < 20; i++) {
            sum += read(a) + read(b) + read(c) + read(d);
        }
        "#,
    );
    assert_eq!(
        global_number(&interp, "sum"),
        200.0,
        "behavioral correctness"
    );
    assert!(
        interp.ic_hit_count() > 0,
        "expected polymorphic IC hits across four cached shapes; got 0"
    );
}

#[test]
fn ic_megamorphic_after_fifth_distinct_shape() {
    // The polymorphic slot caps at four entries. A fifth distinct object
    // overflows it to Megamorphic, which is terminal: subsequent reads of a
    // previously-cached object must NOT re-enter the cache and hit. The warmup
    // itself produces no hits (each object is first-seen), so a zero total hit
    // count proves the site went — and stayed — megamorphic.
    let interp = run_script(
        r#"
        var a = {x: 1}, b = {x: 2}, c = {x: 3}, d = {x: 4}, e = {x: 5};
        function read(o) { return o.x; }
        // Five distinct shapes at one site: a,b,c,d fill the Poly; e overflows.
        var warm = read(a) + read(b) + read(c) + read(d) + read(e);
        var sum = 0;
        for (var i = 0; i < 10; i++) sum += read(a);  // Megamorphic: no hits
        var result = warm + "|" + sum;
        "#,
    );
    assert_eq!(
        global_string(&interp, "result"),
        "15|10",
        "behavioral correctness"
    );
    assert_eq!(
        interp.ic_hit_count(),
        0,
        "a site that overflowed to Megamorphic must not be demoted and start \
         hitting again"
    );
}

#[test]
fn ic_record_uses_pre_slow_path_slot_snapshot() {
    // The slow path can run user code (here an own-accessor getter) that
    // re-enters the SAME body + prop site and mutates the slot before recording
    // resumes. The record step must transition from the slot as it stood BEFORE
    // this access, not the reentrancy-mutated slot.
    //
    // `read(reenter)` misses as a non-cacheable own accessor (classify → None).
    // Its getter recursively runs `read(a)` and `read(b)` at the same site,
    // driving the slot Empty → Mono(a) → Poly([a,b]). If the outer record read
    // the slot AFTER the getter, it would see Poly and apply Poly+None →
    // Megamorphic, terminalizing the site so the following `read(a)` loop can
    // never hit. Snapshotting before the slow path yields Empty+None → Empty,
    // so the loop re-primes Mono(a) and hits. A positive hit count proves the
    // site was not wrongly terminalized.
    let interp = run_script(
        r#"
        var a = {x: 1};
        var b = {x: 2};
        function read(o) { return o.x; }
        var reenter = { get x() { read(a); read(b); return 0; } };
        read(reenter);                                // reentrant getter mutates the slot
        var sum = 0;
        for (var i = 0; i < 10; i++) sum += read(a);  // must be able to cache + hit
        "#,
    );
    assert_eq!(
        global_number(&interp, "sum"),
        10.0,
        "behavioral correctness"
    );
    assert!(
        interp.ic_hit_count() > 0,
        "the site must survive the reentrant slow path and re-cache `a`; got 0 \
         hits (a stale post-slow-path read terminalized it to Megamorphic)"
    );
}

#[test]
fn ic_megamorphic_stays_terminal_after_non_cacheable_miss() {
    // A polymorphic site that then meets a non-cacheable proxy lookup must go
    // Megamorphic and stay there — never demoted back to Empty. If it were
    // demoted, the final hot reads of `a.x` would re-enter the cache and hit.
    // This exercises the `Poly + None -> Megamorphic` transition specifically.
    let interp = run_script(
        r#"
        var a = {x: 1};
        var b = {x: 2};
        var p = new Proxy({x: 3}, {});
        function read(o) { return o.x; }
        var sum = 0;
        sum += read(a);  // Empty -> Mono(a)
        sum += read(b);  // Mono(a) -> Poly([a,b])
        sum += read(p);  // non-cacheable at a Poly site -> Megamorphic
        for (var i = 0; i < 10; i++) sum += read(a);  // must stay terminal
        var result = sum;
        "#,
    );
    assert_eq!(
        global_number(&interp, "result"),
        16.0,
        "behavioral correctness"
    );
    assert_eq!(
        interp.ic_hit_count(),
        0,
        "Megamorphic property IC slot was demoted and started hitting again"
    );
}

#[test]
fn call_ic_megamorphic_stays_terminal_after_non_cacheable_miss() {
    // Same state-machine guard for call ICs: proxy callables classify as None,
    // but a previously Megamorphic site must remain terminal afterwards.
    let interp = run_script(
        r#"
        function a() { return 1; }
        function b() { return 2; }
        var p = new Proxy(function() { return 3; }, {
            apply: function(target, thisArg, args) { return 3; }
        });
        var fns = [a, b, p, a, a, a, a, a, a, a, a, a, a];
        var sum = 0;
        for (var i = 0; i < fns.length; i++) sum += fns[i]();
        var result = sum;
        "#,
    );
    assert_eq!(
        global_number(&interp, "result"),
        16.0,
        "behavioral correctness"
    );
    assert_eq!(
        interp.call_ic_hit_count(),
        0,
        "Megamorphic call IC slot was demoted and started hitting again"
    );
    assert_eq!(
        interp.call_ic_fast_dispatch_count(),
        0,
        "Megamorphic call IC slot reached fast dispatch after demotion"
    );
}

#[test]
fn behavioral_engine_passes_property_value_round_trip() {
    // Behavioral cross-check: even with shape bumps in place, basic
    // assignment/read round-trips still work via the public engine.
    let interp = run_script(
        r#"
        var o = {};
        o.x = 42;
        o.x = 43;
        var a = [1,2,3];
        a[10] = 99;
        a.length = 1;
        var result = o.x + "|" + a.length + "|" + (a[0] || "?");
        "#,
    );
    assert_eq!(global_string(&interp, "result"), "43|1|1");
}

#[test]
fn define_own_property_bumps_shape_on_data_to_accessor_swap() {
    let mut interp = Interpreter::new();
    let id = interp.alloc_object(JsObjectData::new());
    interp
        .get_object(id)
        .unwrap()
        .borrow_mut()
        .set_property_value("x", JsValue::Number(1.0));
    let before = interp.get_object(id).unwrap().borrow().shape_id;
    // Swap data → accessor by defining a getter.
    let ok = interp
        .get_object(id)
        .unwrap()
        .borrow_mut()
        .define_own_property(
            "x".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(JsValue::Undefined), // sentinel — real getter not needed for this test
                set: None,
                enumerable: Some(true),
                configurable: Some(true),
            },
        );
    assert!(ok, "data→accessor swap should succeed");
    let after = interp.get_object(id).unwrap().borrow().shape_id;
    assert!(
        after > before,
        "data→accessor swap is the canonical IC-invalidating shape change"
    );
}

#[test]
fn array_prototype_methods_have_correctly_shaped_builtins() {
    // Characterization test for the setup_array_prototype → define_method refactor:
    // pins the (name, length) contract and the writable/non-enumerable/configurable
    // shape §10.2.4 requires of every own method, independent of how each method is
    // installed internally.
    const EXPECTED: &[(&str, f64)] = &[
        ("push", 1.0),
        ("pop", 0.0),
        ("shift", 0.0),
        ("unshift", 1.0),
        ("indexOf", 1.0),
        ("lastIndexOf", 1.0),
        ("includes", 1.0),
        ("join", 1.0),
        ("toString", 0.0),
        ("toLocaleString", 0.0),
        ("concat", 1.0),
        ("slice", 2.0),
        ("reverse", 0.0),
        ("toReversed", 0.0),
        ("forEach", 1.0),
        ("map", 1.0),
        ("filter", 1.0),
        ("reduce", 1.0),
        ("reduceRight", 1.0),
        ("some", 1.0),
        ("every", 1.0),
        ("find", 1.0),
        ("findIndex", 1.0),
        ("findLast", 1.0),
        ("findLastIndex", 1.0),
        ("splice", 2.0),
        ("toSpliced", 2.0),
        ("fill", 1.0),
        ("sort", 1.0),
        ("toSorted", 1.0),
        ("flat", 0.0),
        ("flatMap", 1.0),
        ("copyWithin", 2.0),
        ("at", 1.0),
        ("with", 2.0),
        ("entries", 0.0),
        ("keys", 0.0),
        ("values", 0.0),
    ];

    let interp = run_script("");
    let proto_id = interp
        .realm()
        .array_prototype
        .expect("array_prototype installed");
    let proto_cell = interp.get_object_cell_expect(proto_id);

    for (name, len) in EXPECTED {
        let desc = proto_cell
            .borrow()
            .get_own_property(name)
            .unwrap_or_else(|| panic!("Array.prototype.{name} installed"));
        assert_eq!(
            desc.writable,
            Some(true),
            "Array.prototype.{name} must stay writable"
        );
        assert_eq!(
            desc.enumerable,
            Some(false),
            "Array.prototype.{name} must not be enumerable"
        );
        assert_eq!(
            desc.configurable,
            Some(true),
            "Array.prototype.{name} must stay configurable"
        );
        let JsValue::Object(fn_obj) = desc.value.expect("method has a function value") else {
            panic!("Array.prototype.{name} is not a function object")
        };
        let fn_cell = interp.get_object_cell_expect(fn_obj.id);
        match fn_cell.borrow().get_own_property("name").unwrap().value {
            Some(JsValue::String(ref s)) => assert_eq!(s.to_rust_string(), *name),
            ref other => panic!("Array.prototype.{name}: expected name string, got {other:?}"),
        }
        match fn_cell.borrow().get_own_property("length").unwrap().value {
            Some(JsValue::Number(n)) => assert_eq!(n, *len, "Array.prototype.{name}.length"),
            ref other => panic!("Array.prototype.{name}: expected length number, got {other:?}"),
        }
    }

    // Array.prototype[@@iterator] must be the very same function object as .values (§23.1.3.35).
    let iterator_key = interp
        .get_symbol_iterator_key()
        .expect("well-known @@iterator key registered");
    let values_desc = proto_cell
        .borrow()
        .get_own_property("values")
        .expect("values installed");
    let iterator_desc = proto_cell
        .borrow()
        .get_own_property(&iterator_key)
        .expect("@@iterator installed");
    let (JsValue::Object(values_obj), JsValue::Object(iterator_obj)) = (
        values_desc.value.expect("values has a function value"),
        iterator_desc
            .value
            .expect("@@iterator has a function value"),
    ) else {
        panic!("expected both values and @@iterator to be function objects")
    };
    assert_eq!(
        values_obj.id, iterator_obj.id,
        "Array.prototype[@@iterator] must be identical to Array.prototype.values"
    );
}

// Characterization test for the collections.rs → define_method refactor. Pins,
// through the public JS surface, the (name, length) contract, the
// writable/non-enumerable/configurable §10.2.4 method shape, the well-known
// aliasing (Map[@@iterator]===entries, Set.keys===values===[@@iterator]), and a
// functional smoke of every Map/Set/WeakMap/WeakSet/WeakRef/FinalizationRegistry
// prototype method — all independent of how each method is installed internally.
// Written before switching those installs to define_method and kept as a guard.
#[test]
fn collection_prototype_methods_have_correctly_shaped_builtins() {
    let interp = run_script(
        r#"
        var E = [];
        function ck(cond, msg) { if (!cond) E.push(msg); }
        function shape(proto, pn, name, len) {
            var d = Object.getOwnPropertyDescriptor(proto, name);
            if (!d) { E.push(pn + "." + name + " missing"); return; }
            var f = d.value;
            ck(typeof f === "function", pn + "." + name + " not function");
            ck(f && f.name === name, pn + "." + name + ".name=" + (f && f.name));
            ck(f && f.length === len, pn + "." + name + ".length=" + (f && f.length));
            ck(d.writable === true, pn + "." + name + " not writable");
            ck(d.enumerable === false, pn + "." + name + " enumerable");
            ck(d.configurable === true, pn + "." + name + " not configurable");
        }

        [["entries",0],["keys",0],["values",0],["get",1],["set",2],["has",1],
         ["delete",1],["clear",0],["forEach",1],["getOrInsert",2],["getOrInsertComputed",2]]
            .forEach(function(m){ shape(Map.prototype, "Map.prototype", m[0], m[1]); });
        [["values",0],["entries",0],["add",1],["has",1],["delete",1],["clear",0],
         ["forEach",1],["union",1],["intersection",1],["difference",1],
         ["symmetricDifference",1],["isSubsetOf",1],["isSupersetOf",1],["isDisjointFrom",1]]
            .forEach(function(m){ shape(Set.prototype, "Set.prototype", m[0], m[1]); });
        [["get",1],["set",2],["has",1],["delete",1],["getOrInsert",2],["getOrInsertComputed",2]]
            .forEach(function(m){ shape(WeakMap.prototype, "WeakMap.prototype", m[0], m[1]); });
        [["add",1],["has",1],["delete",1]]
            .forEach(function(m){ shape(WeakSet.prototype, "WeakSet.prototype", m[0], m[1]); });
        shape(WeakRef.prototype, "WeakRef.prototype", "deref", 0);
        [["register",2],["unregister",1],["cleanupSome",0]]
            .forEach(function(m){ shape(FinalizationRegistry.prototype, "FinalizationRegistry.prototype", m[0], m[1]); });

        // Iterator-prototype next() methods, reached through a live iterator.
        shape(Object.getPrototypeOf(new Map([[1,2]]).entries()), "MapIterator", "next", 0);
        shape(Object.getPrototypeOf(new Set([1]).values()), "SetIterator", "next", 0);

        // Well-known aliasing that the install order must preserve.
        ck(Map.prototype[Symbol.iterator] === Map.prototype.entries, "Map[@@iterator]!==entries");
        ck(Set.prototype[Symbol.iterator] === Set.prototype.values, "Set[@@iterator]!==values");
        ck(Set.prototype.keys === Set.prototype.values, "Set.keys!==values");

        // Functional smoke — behavior, not just shape.
        var m = new Map(); m.set("a", 1); m.set("b", 2);
        ck(m.get("a") === 1, "map.get");
        ck(m.has("b") === true, "map.has");
        ck(m.size === 2, "map.size");
        ck(m.delete("a") === true, "map.delete");
        ck(m.size === 1, "map.size2");
        var acc = ""; m.forEach(function(v, k){ acc += k + ":" + v; });
        ck(acc === "b:2", "map.forEach=" + acc);
        var ent = [].concat.apply([], [...new Map([[1,10],[2,20]]).entries()])
            .join(",");
        ck(ent === "1,10,2,20", "map.entries=" + ent);

        var s = new Set(); s.add(1); s.add(2); s.add(2);
        ck(s.size === 2, "set.size");
        ck(s.has(1) === true, "set.has");
        ck(s.delete(1) === true, "set.delete");
        ck([...new Set([1,2]).union(new Set([2,3]))].join(",") === "1,2,3", "set.union");
        ck([...new Set([1,2,3]).intersection(new Set([2,3,4]))].join(",") === "2,3", "set.intersection");
        ck(new Set([1,2]).isSubsetOf(new Set([1,2,3])) === true, "set.isSubsetOf");

        var wmKey = {}; var wm = new WeakMap(); wm.set(wmKey, 42);
        ck(wm.get(wmKey) === 42, "weakmap.get");
        ck(wm.has(wmKey) === true, "weakmap.has");
        ck(wm.delete(wmKey) === true, "weakmap.delete");
        var wsKey = {}; var ws = new WeakSet(); ws.add(wsKey);
        ck(ws.has(wsKey) === true, "weakset.has");
        ck(ws.delete(wsKey) === true, "weakset.delete");

        var R = E.length ? E.join(" | ") : "OK";
        "#,
    );
    assert_eq!(
        global_string(&interp, "R"),
        "OK",
        "collection prototype methods must keep their observable shape and behavior"
    );
}

#[test]
fn date_set_hours_coerces_all_provided_args_left_to_right_regardless_of_nan() {
    let interp = run_script(
        r#"
        var order = [];
        function tap(name, val) {
            return { valueOf: function () { order.push(name); return val; } };
        }
        var d = new Date(NaN);
        var r = d.setHours(tap("h", 1), tap("m", 2), tap("s", 3), tap("ms", 4));
        globalThis.__order = order.join(",");
        globalThis.__result = r;
        "#,
    );
    assert_eq!(
        global_string(&interp, "__order"),
        "h,m,s,ms",
        "setHours must coerce every provided argument, in order, even when the result is NaN"
    );
    assert!(global_number(&interp, "__result").is_nan());
}

#[test]
fn date_set_minutes_defaults_missing_trailing_args_from_current_local_time() {
    let interp = run_script(
        r#"
        var d = new Date(2024, 0, 1, 10, 20, 30, 400);
        d.setMinutes(5);
        globalThis.__h = d.getHours();
        globalThis.__m = d.getMinutes();
        globalThis.__s = d.getSeconds();
        globalThis.__ms = d.getMilliseconds();
        "#,
    );
    assert_eq!(global_number(&interp, "__h"), 10.0);
    assert_eq!(global_number(&interp, "__m"), 5.0);
    assert_eq!(global_number(&interp, "__s"), 30.0);
    assert_eq!(global_number(&interp, "__ms"), 400.0);
}

#[test]
fn date_set_utc_hours_overrides_only_time_of_day_components() {
    let interp = run_script(
        r#"
        var d = new Date(Date.UTC(2024, 5, 15, 1, 2, 3, 4));
        d.setUTCHours(23, 59, 58, 999);
        globalThis.__y = d.getUTCFullYear();
        globalThis.__mo = d.getUTCMonth();
        globalThis.__d = d.getUTCDate();
        globalThis.__h = d.getUTCHours();
        globalThis.__mi = d.getUTCMinutes();
        globalThis.__s = d.getUTCSeconds();
        globalThis.__ms = d.getUTCMilliseconds();
        "#,
    );
    assert_eq!(global_number(&interp, "__y"), 2024.0);
    assert_eq!(global_number(&interp, "__mo"), 5.0);
    assert_eq!(global_number(&interp, "__d"), 15.0);
    assert_eq!(global_number(&interp, "__h"), 23.0);
    assert_eq!(global_number(&interp, "__mi"), 59.0);
    assert_eq!(global_number(&interp, "__s"), 58.0);
    assert_eq!(global_number(&interp, "__ms"), 999.0);
}

#[test]
fn date_set_utc_full_year_on_invalid_date_seeds_missing_args_from_epoch_not_nan() {
    let interp = run_script(
        r#"
        var d = new Date(NaN);
        d.setUTCFullYear(2024);
        globalThis.__y = d.getUTCFullYear();
        globalThis.__mo = d.getUTCMonth();
        globalThis.__d = d.getUTCDate();
        "#,
    );
    assert_eq!(global_number(&interp, "__y"), 2024.0);
    assert_eq!(global_number(&interp, "__mo"), 0.0);
    assert_eq!(global_number(&interp, "__d"), 1.0);
}

// §7.1.5 ToIntegerOrInfinity — the combined `? ToIntegerOrInfinity(argument)`
// coercion method that sits alongside to_number_value / to_string_value / to_index.
// Expected values are the spec's, not recomputed the way the code does it.
mod to_integer_or_infinity_value_tests {
    use super::*;

    fn conv(v: JsValue) -> f64 {
        let mut interp = Interpreter::new();
        interp
            .to_integer_or_infinity_value(&v)
            .expect("expected a normal (non-throwing) coercion")
    }

    #[test]
    fn truncates_toward_zero() {
        assert_eq!(conv(JsValue::Number(3.7)), 3.0);
        assert_eq!(conv(JsValue::Number(-3.7)), -3.0);
        assert_eq!(conv(JsValue::Number(5.0)), 5.0);
        assert_eq!(conv(JsValue::Number(-0.9)), 0.0);
    }

    #[test]
    fn nan_becomes_positive_zero() {
        let r = conv(JsValue::Number(f64::NAN));
        assert_eq!(r, 0.0);
        assert!(r.is_sign_positive(), "ToIntegerOrInfinity(NaN) is +0");
    }

    #[test]
    fn infinities_pass_through() {
        assert_eq!(conv(JsValue::Number(f64::INFINITY)), f64::INFINITY);
        assert_eq!(conv(JsValue::Number(f64::NEG_INFINITY)), f64::NEG_INFINITY);
    }

    #[test]
    fn coerces_booleans_null_and_undefined() {
        assert_eq!(conv(JsValue::Boolean(true)), 1.0); // ToNumber(true) = 1
        assert_eq!(conv(JsValue::Boolean(false)), 0.0);
        assert_eq!(conv(JsValue::Null), 0.0); // ToNumber(null) = 0
        assert_eq!(conv(JsValue::Undefined), 0.0); // ToNumber(undefined) = NaN -> 0
    }

    #[test]
    fn coerces_strings() {
        assert_eq!(conv(JsValue::String(JsString::from_str("42"))), 42.0);
        assert_eq!(conv(JsValue::String(JsString::from_str("42.9"))), 42.0);
        assert_eq!(conv(JsValue::String(JsString::from_str("  -7.5 "))), -7.0);
        assert_eq!(conv(JsValue::String(JsString::from_str("abc"))), 0.0); // NaN -> 0
        assert_eq!(
            conv(JsValue::String(JsString::from_str("Infinity"))),
            f64::INFINITY
        );
    }

    #[test]
    fn observable_truncation_through_array_prototype_at() {
        // The builtin routes its index argument through the coercion method,
        // so truncation is observable at the public JS seam.
        let interp = run_script(r#"var R = [10, 20, 30].at(1.9);"#);
        assert_eq!(global_number(&interp, "R"), 20.0);
    }

    #[test]
    fn throwing_valueof_propagates_through_the_seam() {
        // ? ToIntegerOrInfinity must forward a ToNumber abrupt completion.
        let interp = run_script(
            r#"
            var threw = false;
            try {
                [1, 2, 3].at({ valueOf() { throw new Error("boom"); } });
            } catch (e) {
                threw = (e instanceof Error) && e.message === "boom";
            }
            var R = threw ? "threw" : "did-not-throw";
            "#,
        );
        assert_eq!(global_string(&interp, "R"), "threw");
    }
}

/// Node host-compat "syscall floor" (issue #229). These exercise the ON-path
/// (`enable_node_host`); the OFF-path 0-regression guarantee is covered by the
/// full test262 run, which never enables the floor.
mod node_host_tests {
    use super::*;

    fn run_node_script(source: &str) -> (Interpreter, Completion) {
        let program = parse_program(source);
        let mut interp = Interpreter::new();
        interp.enable_node_host();
        let c = interp.run(&program);
        (interp, c)
    }

    /// Enable the floor, run `source`, and assert it finished without throwing.
    /// JS-level `throw new Error(...)` inside `source` is how each case reports
    /// a failed assertion.
    fn assert_node_ok(source: &str) {
        let (_interp, c) = run_node_script(source);
        assert!(
            matches!(c, Completion::Normal(_) | Completion::Empty),
            "unexpected completion: {c:?}"
        );
    }

    #[test]
    fn host_globals_absent_when_floor_off() {
        // `typeof` on an undeclared name is safe (no ReferenceError).
        let interp = run_script(
            r#"
            var r = [
              typeof __host_write,
              typeof globalThis.__host_exit,
              typeof __host_hrtime,
              typeof __host_random_bytes,
            ].join(",");
            "#,
        );
        assert_eq!(
            global_string(&interp, "r"),
            "undefined,undefined,undefined,undefined"
        );
    }

    #[test]
    fn host_globals_are_non_enumerable_functions() {
        assert_node_ok(
            r#"
            for (const name of ["__host_write","__host_exit","__host_hrtime","__host_random_bytes"]) {
              const d = Object.getOwnPropertyDescriptor(globalThis, name);
              if (!d) throw new Error(name + " missing");
              if (d.enumerable) throw new Error(name + " is enumerable");
              if (typeof globalThis[name] !== "function") throw new Error(name + " not a function");
              if (Object.keys(globalThis).includes(name)) throw new Error(name + " shows in keys");
            }
            "#,
        );
    }

    #[test]
    fn host_write_returns_utf8_byte_count() {
        // "€" encodes to 3 UTF-8 bytes; a lone surrogate becomes U+FFFD (also
        // 3 bytes), matching Node's lossy handling.
        assert_node_ok(
            r#"
            if (__host_write(1, "abc") !== 3) throw new Error("ascii");
            if (__host_write(1, "€") !== 3) throw new Error("euro");
            if (__host_write(2, String.fromCharCode(0xD800)) !== 3) throw new Error("surrogate");
            if (__host_write(1, "") !== 0) throw new Error("empty");
            "#,
        );
    }

    #[test]
    fn host_hrtime_is_monotonic_bigint() {
        assert_node_ok(
            r#"
            const a = __host_hrtime();
            const b = __host_hrtime();
            if (typeof a !== "bigint") throw new Error("not a bigint");
            if (a < 0n) throw new Error("negative");
            if (!(b >= a)) throw new Error("not monotonic");
            "#,
        );
    }

    #[test]
    fn host_random_bytes_length_and_entropy() {
        assert_node_ok(
            r#"
            const a = __host_random_bytes(16);
            if (!(a instanceof Uint8Array)) throw new Error("not Uint8Array");
            if (a.length !== 16) throw new Error("wrong length");
            if (__host_random_bytes(0).length !== 0) throw new Error("zero length");
            const b = __host_random_bytes(16);
            // Two independent 16-byte draws colliding is ~2^-128.
            let same = true;
            for (let i = 0; i < 16; i++) if (a[i] !== b[i]) { same = false; break; }
            if (same) throw new Error("no entropy");
            "#,
        );
    }

    #[test]
    fn host_random_bytes_rejects_out_of_range() {
        assert_node_ok(
            r#"
            let threw = false;
            try { __host_random_bytes(-1); } catch (e) { threw = e instanceof RangeError; }
            if (!threw) throw new Error("negative not rejected");
            threw = false;
            try { __host_random_bytes(2 ** 31); } catch (e) { threw = e instanceof RangeError; }
            if (!threw) throw new Error("oversize not rejected");
            "#,
        );
    }

    #[test]
    fn host_exit_is_uncatchable_and_records_code() {
        let (interp, c) = run_node_script(
            r#"
            globalThis.reached = "before";
            try { __host_exit(42); globalThis.reached = "after-exit"; }
            catch (e) { globalThis.reached = "caught"; }
            finally { globalThis.reached = "finally"; }
            globalThis.reached = "end";
            "#,
        );
        assert_eq!(interp.pending_exit, Some(42));
        // Execution stopped at __host_exit: catch, finally, and the trailing
        // statement never ran.
        assert_eq!(global_string(&interp, "reached"), "before");
        assert!(matches!(c, Completion::Throw(_)));
    }

    #[test]
    fn host_exit_from_async_reaction_stops_drain() {
        // The drain-loop backstop: a throw raised inside a Promise reaction is
        // swallowed into a rejection, so only the loop's `pending_exit` check
        // stops further microtasks from running.
        let (interp, _c) = run_node_script(
            r#"
            globalThis.log = "";
            Promise.resolve().then(() => { globalThis.log += "then;"; __host_exit(9); globalThis.log += "after;"; });
            Promise.resolve().then(() => { globalThis.log += "second;"; });
            "#,
        );
        assert_eq!(interp.pending_exit, Some(9));
        assert_eq!(global_string(&interp, "log"), "then;");
    }

    #[test]
    fn host_exit_skips_iterator_return_cleanup() {
        // A pending exit must not run the iterator's user-defined return()
        // during for-of unwinding — it could re-enter __host_exit and overwrite
        // the code, or run arbitrary side effects. (PR #237 review, Codex P2.)
        let (interp, _c) = run_node_script(
            r#"
            globalThis.cleanup = "no";
            const iter = {
              [Symbol.iterator]() { return this; },
              next() { return { value: 1, done: false }; },
              return() { globalThis.cleanup = "ran"; __host_exit(99); return { done: true }; },
            };
            for (const x of iter) { __host_exit(7); }
            "#,
        );
        assert_eq!(interp.pending_exit, Some(7)); // not overwritten by return()'s exit(99)
        assert_eq!(global_string(&interp, "cleanup"), "no");
    }

    #[test]
    fn host_exit_skips_using_disposal() {
        // A pending exit must not run Symbol.dispose from a `using` declaration.
        let (interp, _c) = run_node_script(
            r#"
            globalThis.disposed = "no";
            {
              using r = { [Symbol.dispose]() { globalThis.disposed = "ran"; } };
              __host_exit(3);
            }
            "#,
        );
        assert_eq!(interp.pending_exit, Some(3));
        assert_eq!(global_string(&interp, "disposed"), "no");
    }

    #[test]
    fn host_exit_uncatchable_in_generator_body() {
        // The generator/async state machine routes a Throw through its own
        // catch/finally states; a pending exit must bypass that. (PR #237
        // review round 2, Codex P2.)
        let (interp, _c) = run_node_script(
            r#"
            globalThis.ran = "no";
            function* g() { try { yield 0; __host_exit(7); } catch { globalThis.ran = "caught"; } }
            const it = g();
            it.next(); // yields 0
            it.next(); // resumes, calls __host_exit(7)
            "#,
        );
        assert_eq!(interp.pending_exit, Some(7));
        assert_eq!(global_string(&interp, "ran"), "no");
    }

    #[test]
    fn host_exit_uncatchable_in_async_body() {
        let (interp, _c) = run_node_script(
            r#"
            globalThis.aran = "no";
            async function f() { try { await 0; __host_exit(5); } catch { globalThis.aran = "caught"; } }
            f();
            "#,
        );
        assert_eq!(interp.pending_exit, Some(5));
        assert_eq!(global_string(&interp, "aran"), "no");
    }

    #[test]
    fn host_exit_from_disposer_stops_remaining_disposers() {
        // Disposal runs in reverse order: `b` disposes first and calls exit,
        // so `a`'s disposer must not run. (PR #237 review round 2, Codex P2.)
        let (interp, _c) = run_node_script(
            r#"
            globalThis.d = "";
            {
              using a = { [Symbol.dispose]() { globalThis.d += "a"; } };
              using b = { [Symbol.dispose]() { globalThis.d += "b"; __host_exit(4); } };
            }
            "#,
        );
        assert_eq!(interp.pending_exit, Some(4));
        assert_eq!(global_string(&interp, "d"), "b");
    }

    #[test]
    fn host_exit_from_disposer_skips_suppressed_error_wrapping() {
        // When disposal is already unwinding a throw, a disposer that calls
        // __host_exit must not fall through to wrap_suppressed_error, which
        // would invoke the (user-replaceable) SuppressedError constructor —
        // arbitrary JS after the exit. (PR #237 review round 3, Codex P2.)
        let (interp, _c) = run_node_script(
            r#"
            globalThis.suppressedCtorRan = "no";
            globalThis.aRan = "no";
            globalThis.caught = "no";
            globalThis.SuppressedError = function () { globalThis.suppressedCtorRan = "ran"; };
            try {
              {
                using a = { [Symbol.dispose]() { globalThis.aRan = "yes"; } };
                using b = { [Symbol.dispose]() { __host_exit(8); } };
                throw new Error("boom"); // disposal now unwinds an existing error
              }
            } catch (e) { globalThis.caught = "yes"; }
            "#,
        );
        assert_eq!(interp.pending_exit, Some(8));
        assert_eq!(global_string(&interp, "suppressedCtorRan"), "no");
        assert_eq!(global_string(&interp, "aRan"), "no"); // earlier resource not disposed
        assert_eq!(global_string(&interp, "caught"), "no"); // exit is uncatchable
    }
}
