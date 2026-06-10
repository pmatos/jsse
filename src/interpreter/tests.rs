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
    interp.gc_requested = true;
    interp.gc_safepoint();
    assert!(
        interp.get_object_cell(id).is_some(),
        "microtask root should keep object alive"
    );

    interp.scheduler.clear_microtasks();
    interp.gc_requested = true;
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

    interp.gc_requested = true;
    interp.gc_safepoint();
    assert!(interp.get_object_cell(obj_ref.id).is_some());

    interp.module_registry.remove(&key);
    interp.gc_requested = true;
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
fn ic_megamorphic_after_distinct_objects_at_same_site() {
    // Same call site sees two different objects; the second access shape
    // mismatches and transitions to Megamorphic. Behavioural correctness
    // must still hold.
    let interp = run_script(
        r#"
        var a = {x: 1};
        var b = {x: 2};
        function read(o) { return o.x; }
        var v1 = read(a);
        var v2 = read(b);
        var v3 = read(a);
        var result = v1 + "|" + v2 + "|" + v3;
        "#,
    );
    assert_eq!(global_string(&interp, "result"), "1|2|1");
}

#[test]
fn ic_megamorphic_stays_terminal_after_non_cacheable_miss() {
    // A site that has already gone Megamorphic must not be demoted back to
    // Empty when it later sees a non-cacheable proxy lookup. If it were
    // demoted, the final hot reads of `a.x` would re-enter Mono and hit.
    let interp = run_script(
        r#"
        var a = {x: 1};
        var b = {x: 2};
        var p = new Proxy({x: 3}, {});
        function read(o) { return o.x; }
        var sum = 0;
        sum += read(a);  // Empty -> Mono(a)
        sum += read(b);  // Mono(a) -> Megamorphic
        sum += read(p);  // non-cacheable; must stay Megamorphic
        for (var i = 0; i < 10; i++) sum += read(a);
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
