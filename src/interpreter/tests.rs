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

fn run_step(interp: &mut Interpreter, source: &str) {
    let program = parse_program(source);
    let result = interp.run(&program);
    assert!(
        matches!(result, Completion::Normal(_) | Completion::Empty),
        "unexpected completion: {result:?}"
    );
}

fn run_script(source: &str) -> Interpreter {
    let mut interp = Interpreter::new();
    run_step(&mut interp, source);
    interp
}

fn run_script_as_blocking_agent(source: &str) -> Interpreter {
    let program = parse_program(source);
    let mut interp = Interpreter::new();
    interp.can_block = true;
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
    let value = interp
        .get_global_var_ref(name)
        .unwrap_or(JsValue::UNDEFINED);
    value
        .as_string()
        .unwrap_or_else(|| panic!("expected global string for {name}, got {value:?}"))
        .to_string()
}

fn global_number(interp: &Interpreter, name: &str) -> f64 {
    let value = interp
        .get_global_var_ref(name)
        .unwrap_or(JsValue::UNDEFINED);
    value
        .as_number()
        .unwrap_or_else(|| panic!("expected global number for {name}, got {value:?}"))
}

fn global_object_id(interp: &Interpreter, name: &str) -> u64 {
    let value = interp
        .get_global_var_ref(name)
        .unwrap_or(JsValue::UNDEFINED);
    value
        .as_object_id()
        .unwrap_or_else(|| panic!("expected global object for {name}, got {value:?}"))
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
fn function_environment_pool_reuses_and_resets_unescaped_storage() {
    let mut interp = Interpreter::new();
    let first_parent = Environment::new(None);
    let env = interp.acquire_function_environment(first_parent, 3);
    let allocation = Rc::as_ptr(&env);
    {
        let mut env = env.borrow_mut();
        env.declare("stale", BindingKind::Var);
        env.is_arrow_scope = true;
        env.arguments_immutable = true;
    }

    interp.recycle_function_environment(env);
    assert_eq!(interp.function_env_pool.len(), 1);

    let second_parent = Environment::new(None);
    let reused = interp.acquire_function_environment(second_parent.clone(), 2);
    assert_eq!(Rc::as_ptr(&reused), allocation);
    let reused_env = reused.borrow();
    assert!(reused_env.bindings.is_empty());
    assert!(!reused_env.is_arrow_scope);
    assert!(!reused_env.arguments_immutable);
    assert!(Rc::ptr_eq(
        reused_env.parent.as_ref().expect("new parent"),
        &second_parent
    ));
}

#[test]
fn function_environment_pool_rejects_escaped_storage() {
    let mut interp = Interpreter::new();
    let env = interp.acquire_function_environment(Environment::new(None), 1);
    env.borrow_mut().declare("captured", BindingKind::Var);
    let escaped = env.clone();

    interp.recycle_function_environment(env);

    assert!(interp.function_env_pool.is_empty());
    assert!(escaped.borrow().bindings.contains_key("captured"));
}

#[test]
fn ordinary_calls_return_non_escaping_activations_to_the_pool() {
    let interp = run_script(
        r#"
        function addOne(value) { return value + 1; }
        var result = addOne(addOne(0));
        if (result !== 2) throw new Error("unexpected result");
        "#,
    );

    assert!(!interp.function_env_pool.is_empty());
}

#[test]
fn define_method_installs_a_correctly_shaped_builtin() {
    let mut interp = Interpreter::new();
    let target_id = interp.create_object_id();

    interp.define_method(target_id, "greet", 1, |_interp, _this, args| {
        let name = args
            .first()
            .and_then(JsValue::as_string)
            .map(|s| s.to_rust_string())
            .unwrap_or_else(|| "world".to_string());
        Completion::Normal(JsValue::from_str(&format!("hello {name}")))
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
    let fn_id = greet_fn
        .as_object_id()
        .expect("expected greet to be a function object");
    let fn_cell = interp.get_object_cell_expect(fn_id);
    let name = fn_cell
        .borrow()
        .get_own_property("name")
        .unwrap()
        .value
        .and_then(|value| value.as_string())
        .expect("expected name string");
    assert_eq!(name.to_rust_string(), "greet");
    let length = fn_cell
        .borrow()
        .get_own_property("length")
        .unwrap()
        .value
        .and_then(|value| value.as_number())
        .expect("expected length number");
    assert_eq!(length, 1.0);

    let target_val = JsValue::object(target_id);
    let result = interp.call_function(&greet_fn, &target_val, &[JsValue::from_str("jsse")]);
    match result {
        Completion::Normal(value) => assert_eq!(
            value
                .as_string()
                .expect("expected string completion")
                .to_rust_string(),
            "hello jsse"
        ),
        other => panic!("unexpected completion: {other:?}"),
    }
}

#[test]
fn define_getter_installs_a_correctly_shaped_accessor() {
    let mut interp = Interpreter::new();
    let target_id = interp.create_object_id();

    let getter = interp.define_getter(target_id, "answer", |_interp, _this, _args| {
        Completion::Normal(JsValue::number(42.0))
    });

    // Raw stored descriptor (get_own_property would complete an absent setter
    // to `undefined`); this pins exactly what define_getter installs.
    let desc = interp
        .get_object_cell_expect(target_id)
        .borrow()
        .get_own_property_full("answer")
        .expect("answer property installed");

    // Read-only accessor: getter present, no setter, no data slots.
    assert!(desc.get.is_some(), "accessor must carry a getter");
    assert!(desc.set.is_none(), "read-only accessor has no setter");
    assert!(desc.value.is_none(), "accessor has no data value");
    assert_eq!(desc.writable, None, "accessor has no writable attribute");
    assert_eq!(
        desc.enumerable,
        Some(false),
        "builtin accessors must not be enumerable"
    );
    assert_eq!(
        desc.configurable,
        Some(true),
        "builtin accessors must stay configurable"
    );

    // define_getter must return the same function it installed as the getter.
    let installed_getter = desc.get.expect("getter value present");
    let getter_id = getter
        .as_object_id()
        .expect("expected getter to be a function object");
    assert_eq!(
        installed_getter.as_object_id(),
        Some(getter_id),
        "returned getter must be the one installed on the accessor"
    );

    // Getter bookkeeping: name is prefixed with "get ", length is 0 (per spec).
    let fn_cell = interp.get_object_cell_expect(getter_id);
    let name = fn_cell
        .borrow()
        .get_own_property("name")
        .unwrap()
        .value
        .and_then(|value| value.as_string())
        .expect("expected name string");
    assert_eq!(name.to_rust_string(), "get answer");
    let length = fn_cell
        .borrow()
        .get_own_property("length")
        .unwrap()
        .value
        .and_then(|value| value.as_number())
        .expect("expected length number");
    assert_eq!(length, 0.0);

    // The getter is callable and returns its computed value.
    let target_val = JsValue::object(target_id);
    match interp.call_function(&getter, &target_val, &[]) {
        Completion::Normal(value) => assert_eq!(value.as_number(), Some(42.0)),
        other => panic!("unexpected completion: {other:?}"),
    }
}

#[test]
fn define_to_string_tag_installs_a_correctly_shaped_data_property() {
    let mut interp = Interpreter::new();
    let target_id = interp.create_object_id();

    interp.define_to_string_tag(target_id, "CorrectlyShaped");

    // The tag lives under the @@toStringTag well-known symbol key.
    let tag_key = crate::types::JsPropertyKey::well_known_symbol("toStringTag");

    // Raw stored descriptor pins exactly what define_to_string_tag installs.
    let desc = interp
        .get_object_cell_expect(target_id)
        .borrow()
        .get_own_property_full(&tag_key)
        .expect("@@toStringTag property installed");

    // Per spec, @@toStringTag on builtin prototypes is a data property:
    // { [[Value]]: tag, [[Writable]]: false, [[Enumerable]]: false, [[Configurable]]: true }.
    let tag = desc
        .value
        .as_ref()
        .and_then(JsValue::as_string)
        .expect("expected string tag value");
    assert_eq!(tag.to_rust_string(), "CorrectlyShaped");
    assert_eq!(desc.writable, Some(false), "@@toStringTag is non-writable");
    assert_eq!(
        desc.enumerable,
        Some(false),
        "@@toStringTag is non-enumerable"
    );
    assert_eq!(
        desc.configurable,
        Some(true),
        "@@toStringTag stays configurable"
    );
    assert!(desc.get.is_none(), "data property has no getter");
    assert!(desc.set.is_none(), "data property has no setter");

    // Routed through insert_property, so property_order and properties stay in
    // sync: the symbol key must appear exactly once in the enumeration order.
    let cell = interp.get_object_cell_expect(target_id);
    let order_hits = cell
        .borrow()
        .property_order
        .iter()
        .filter(|k| k.as_bytes() == tag_key.as_bytes())
        .count();
    assert_eq!(
        order_hits, 1,
        "@@toStringTag appears once in property_order"
    );

    // Observable: Object.prototype.toString sees the installed tag.
    let target_val = JsValue::object(target_id);
    let to_string = match interp.run(&parse_program("Object.prototype.toString;")) {
        Completion::Normal(v) => v,
        other => panic!("expected Object.prototype.toString value, got {other:?}"),
    };
    match interp.call_function(&to_string, &target_val, &[]) {
        Completion::Normal(value) => assert_eq!(
            value
                .as_string()
                .expect("expected string completion")
                .to_rust_string(),
            "[object CorrectlyShaped]"
        ),
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
fn timers_fire_before_run_returns() {
    let interp = run_script(
        r#"
        var result = "pending";
        setTimeout(() => { result = "done"; }, 0);
        "#,
    );
    assert_eq!(global_string(&interp, "result"), "done");
}

#[test]
fn timer_ids_are_distinct_and_never_zero() {
    // Issue #254: setTimeout used to return 0 for every call, so nothing could
    // be cancelled even once clearTimeout existed.
    let interp = run_script(
        r#"
        var a = setTimeout(() => {}, 1000);
        var b = setTimeout(() => {}, 1000);
        var distinct = a !== b && a !== 0 && b !== 0;
        clearTimeout(a);
        clearTimeout(b);
        "#,
    );
    assert_eq!(
        interp
            .get_global_var_ref("distinct")
            .and_then(|v| v.as_boolean()),
        Some(true)
    );
}

#[test]
fn clear_timeout_cancels_a_pending_timer() {
    let interp = run_script(
        r#"
        var log = "";
        var cancelled = setTimeout(() => { log += "cancelled"; }, 0);
        setTimeout(() => { log += "kept"; }, 0);
        clearTimeout(cancelled);
        "#,
    );
    assert_eq!(global_string(&interp, "log"), "kept");
}

#[test]
fn clear_timeout_of_an_unknown_id_is_a_no_op() {
    let interp = run_script(
        r#"
        var log = "";
        clearTimeout(0);
        clearTimeout(9999);
        clearTimeout(undefined);
        clearTimeout("not an id");
        setTimeout(() => { log += "ran"; }, 0);
        "#,
    );
    assert_eq!(global_string(&interp, "log"), "ran");
}

#[test]
fn clear_timeout_does_not_coerce_a_non_primitive_id() {
    // Node resolves an id only from a number or a string and ignores anything
    // else outright, so clearing with an object must not run its valueOf.
    let interp = run_script(
        r#"
        var coerced = "no";
        clearTimeout({ valueOf: function () { coerced = "yes"; return 1; } });
        "#,
    );
    assert_eq!(global_string(&interp, "coerced"), "no");
}

#[test]
fn clear_timeout_accepts_a_string_id() {
    let interp = run_script(
        r#"
        var log = "";
        var id = setTimeout(function () { log += "ran"; }, 0);
        clearTimeout(String(id));
        "#,
    );
    assert_eq!(global_string(&interp, "log"), "");
}

#[test]
fn interval_repeats_until_cleared() {
    let interp = run_script(
        r#"
        var ticks = 0;
        var id = setInterval(() => {
          ticks++;
          if (ticks === 3) clearInterval(id);
        }, 1);
        "#,
    );
    assert_eq!(global_number(&interp, "ticks"), 3.0);
}

#[test]
fn microtasks_drain_between_timer_callbacks() {
    // Each timer callback is a task, so its microtasks run before the next
    // callback — matching the HTML/Node event loop.
    let interp = run_script(
        r#"
        var order = "";
        setTimeout(() => {
          order += "t1";
          Promise.resolve().then(() => { order += "m1"; });
        }, 0);
        setTimeout(() => { order += "t2"; }, 0);
        "#,
    );
    assert_eq!(global_string(&interp, "order"), "t1m1t2");
}

#[test]
fn same_delay_timers_fire_in_arming_order() {
    let interp = run_script(
        r#"
        var order = "";
        setTimeout(() => { order += "c"; }, 5);
        setTimeout(() => { order += "a"; }, 0);
        setTimeout(() => { order += "b"; }, 0);
        "#,
    );
    assert_eq!(global_string(&interp, "order"), "abc");
}

#[test]
fn timer_extra_arguments_are_forwarded_to_the_callback() {
    let interp = run_script(
        r#"
        var seen = "";
        setTimeout((x, y) => { seen = x + "," + y; }, 0, "first", "second");
        "#,
    );
    assert_eq!(global_string(&interp, "seen"), "first,second");
}

#[test]
fn non_callable_timer_callback_throws_a_type_error() {
    for source in [
        "try { setTimeout(42, 0); } catch (e) { var name = e.constructor.name; }",
        "try { setInterval(42, 0); } catch (e) { var name = e.constructor.name; }",
    ] {
        let interp = run_script(source);
        assert_eq!(global_string(&interp, "name"), "TypeError");
    }
}

#[test]
fn a_gc_inside_one_timer_callback_does_not_collect_the_rest_of_the_batch() {
    // The due batch is handed out of the scheduler before any of it runs, so
    // nothing in the queue roots it any more. A callback that allocates enough
    // to trigger a collection must not take its unrun siblings with it.
    let interp = run_script(
        r#"
        var fired = 0;
        for (var i = 0; i < 200; i++) {
          (function (n) {
            setTimeout(function () {
              fired++;
              if (n === 0) {
                var junk = [];
                for (var k = 0; k < 400000; k++) junk.push({ a: k, b: [k, k] });
                junk = null;
              }
            }, 0);
          })(i);
        }
        "#,
    );
    assert_eq!(global_number(&interp, "fired"), 200.0);
}

#[test]
fn a_nested_interpreter_run_does_not_re_enter_timer_dispatch() {
    // A zero-delay interval re-arms for the next turn before its callback runs.
    // If a nested run (`$262.evalScript`) serviced timers, it would find that
    // re-armed interval due and recurse until the drain deadline — the callback
    // never reached its own clearInterval, and the process hung.
    let interp = run_script(
        r#"
        var ticks = 0;
        var id = setInterval(function () {
          ticks++;
          $262.evalScript("void 0;");
          clearInterval(id);
        }, 0);
        "#,
    );
    assert_eq!(global_number(&interp, "ticks"), 1.0);
}

#[test]
fn a_timer_scheduled_inside_a_callback_waits_for_the_next_turn() {
    // The child timeout must not be fired by the nested run started inside its
    // parent's callback: tasks do not re-enter one another.
    let interp = run_script(
        r#"
        var order = "";
        setTimeout(function () {
          order += "a";
          setTimeout(function () { order += "b"; }, 0);
          order += "c";
          $262.evalScript("void 0;");
          order += "d";
        }, 0);
        "#,
    );
    assert_eq!(global_string(&interp, "order"), "acdb");
}

#[test]
fn many_concurrent_timers_run_without_a_thread_per_timer() {
    // The failure in issue #254: a thread per setTimeout exhausted the OS
    // thread limit once enough timers were armed at once.
    let interp = run_script(
        r#"
        var fired = 0;
        var ids = [];
        for (var i = 0; i < 20000; i++) ids.push(setTimeout(() => { fired++; }, 20));
        for (var i = 0; i < 20000; i += 2) clearTimeout(ids[i]);
        "#,
    );
    assert_eq!(global_number(&interp, "fired"), 10000.0);
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
fn module_key_canonicalization_distinguishes_host_identity_from_a_real_file() {
    let dir = temp_case_dir("module-key");
    let file_path = write_case_file(&dir, MODULE_SOURCE_SPECIFIER, "export const value = 1;");

    let host_key = ModuleKey::module_source();
    assert!(host_key.is_module_source());
    assert!(host_key.file_path().is_none());
    assert_eq!(host_key.canonicalize(), host_key);

    let file_key = ModuleKey::for_file(file_path.clone());
    assert!(!file_key.is_module_source());
    assert_eq!(file_key.file_path(), Some(file_path.as_path()));
    assert_eq!(file_key.canonicalize(), file_key);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn module_loader_dispatch_owns_host_type_and_mode_handling() {
    let mut interp = Interpreter::new();
    let key = interp
        .resolve_module_specifier(MODULE_SOURCE_SPECIFIER, None)
        .expect("host module should resolve");

    let eager = interp
        .load_module_for_type(&key, None, ModuleLoadMode::Evaluate)
        .expect("untyped eager host module should load");
    let deferred = interp
        .load_module_for_type(&key, None, ModuleLoadMode::Defer)
        .expect("untyped deferred host module should load");
    assert!(Rc::ptr_eq(&eager, &deferred));

    for import_type in [ImportModuleType::Text, ImportModuleType::Bytes] {
        let mut messages = Vec::new();
        for mode in [ModuleLoadMode::Evaluate, ModuleLoadMode::Defer] {
            let error = match interp.load_module_for_type(&key, Some(import_type), mode) {
                Ok(_) => panic!("typed host module request unexpectedly loaded"),
                Err(error) => error,
            };
            let message = interp.format_value(&error);
            assert!(
                message.starts_with("TypeError:"),
                "unexpected error: {message}"
            );
            messages.push(message);
        }
        assert_eq!(messages[0], messages[1]);
    }
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

    let broken_key = ModuleKey::for_file(broken_path.clone());
    let realm_id = interp.current_realm_id;
    let cached = interp
        .module_registry
        .get(&(realm_id, broken_key))
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
fn clear_timeout_from_js_disarms_the_timer_and_releases_its_callback() {
    let mut interp = Interpreter::new();
    interp.run(&parse_program(
        "var cb = () => {}; clearTimeout(setTimeout(cb, 3600000));",
    ));
    assert!(
        !interp.scheduler.has_timers(),
        "clearTimeout must reach the timer queue, not just return undefined"
    );

    let callback_id = global_object_id(&interp, "cb");
    interp.run(&parse_program("cb = null;"));
    interp.gc.request();
    interp.gc_safepoint();
    assert!(
        interp.get_object_cell(callback_id).is_none(),
        "a timer cleared from JS must stop rooting its callback"
    );
}

#[test]
fn gc_keeps_timer_roots_alive_until_the_timer_is_cleared() {
    let mut interp = Interpreter::new();
    let callback_id = interp.create_object_id();
    let arg_id = interp.create_object_id();

    let timer_id = interp.scheduler.add_timer(
        JsValue::object(callback_id),
        vec![JsValue::object(arg_id)],
        std::time::Duration::from_secs(3600),
        false,
    );
    interp.gc.request();
    interp.gc_safepoint();
    assert!(
        interp.get_object_cell(callback_id).is_some(),
        "an armed timer must keep its callback alive"
    );
    assert!(
        interp.get_object_cell(arg_id).is_some(),
        "an armed timer must keep its bound arguments alive"
    );

    // Cancellation drops the roots; the thread-per-timer model leaked them
    // instead, because only a fired timer ever unrooted.
    interp.scheduler.clear_timer(timer_id);
    interp.gc.request();
    interp.gc_safepoint();
    assert!(
        interp.get_object_cell(callback_id).is_none(),
        "a cleared timer must release its callback"
    );
    assert!(
        interp.get_object_cell(arg_id).is_none(),
        "a cleared timer must release its bound arguments"
    );
}

#[test]
fn gc_keeps_microtask_roots_alive_until_queue_is_cleared() {
    let mut interp = Interpreter::new();
    let id = interp.create_object_id();
    let obj_val = JsValue::object(id);

    interp.scheduler.enqueue_microtask((
        vec![obj_val.clone()],
        Box::new(|_| Completion::Normal(JsValue::UNDEFINED)),
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

/// Run each source in turn on one interpreter, forcing a major collection
/// between steps. Combinator machinery that is only reachable through a native
/// closure capture is reclaimed at those points unless it is pinned.
fn run_steps_with_major_gc_between(steps: &[&str]) -> Interpreter {
    let mut interp = Interpreter::new();
    for step in steps {
        run_step(&mut interp, step);
        interp.gc.request();
        interp.gc_safepoint();
    }
    interp
}

// The promise combinators build their per-element resolve/reject functions as
// native closures that capture the capability's resolving function and the
// shared result accumulator by value. Those captures are invisible to the GC
// tracer, so a major collection while the combinator is in flight used to
// reclaim them: the element function then called a dead id, the combinator
// promise never settled, and every continuation awaiting it was lost (#309).
// In each case below the input promises are scoped to an IIFE, so nothing but
// the combinator's own machinery keeps them — or their settled values — alive.

#[test]
fn promise_all_settles_across_major_gc_between_element_settlements() {
    let interp = run_steps_with_major_gc_between(&[
        r#"
        globalThis.outcome = "pending";
        (function () {
            const first = new Promise((resolve) => { globalThis.releaseFirst = resolve; });
            const second = new Promise((resolve) => { globalThis.releaseSecond = resolve; });
            Promise.all([first, second]).then((values) => {
                globalThis.outcome = "all:" + values[0].marker + "," + values[1].marker;
            });
        })();
        "#,
        r#"
        (function () {
            var settle = globalThis.releaseFirst;
            delete globalThis.releaseFirst;
            settle({ marker: "first" });
        })();
        "#,
        r#"
        (function () {
            var settle = globalThis.releaseSecond;
            delete globalThis.releaseSecond;
            settle({ marker: "second" });
        })();
        "#,
    ]);
    assert_eq!(global_string(&interp, "outcome"), "all:first,second");
}

#[test]
fn promise_all_settled_settles_across_major_gc_between_element_settlements() {
    let interp = run_steps_with_major_gc_between(&[
        r#"
        globalThis.outcome = "pending";
        (function () {
            const first = new Promise((resolve) => { globalThis.releaseFirst = resolve; });
            const second = new Promise((_, reject) => { globalThis.rejectSecond = reject; });
            Promise.allSettled([first, second]).then((results) => {
                globalThis.outcome = "settled:" + results[0].status + ":" +
                    results[0].value.marker + "," + results[1].status + ":" +
                    results[1].reason.marker;
            });
        })();
        "#,
        r#"
        (function () {
            var settle = globalThis.releaseFirst;
            delete globalThis.releaseFirst;
            settle({ marker: "first" });
        })();
        "#,
        r#"
        (function () {
            var settle = globalThis.rejectSecond;
            delete globalThis.rejectSecond;
            settle({ marker: "second" });
        })();
        "#,
    ]);
    assert_eq!(
        global_string(&interp, "outcome"),
        "settled:fulfilled:first,rejected:second"
    );
}

#[test]
fn promise_any_rejects_across_major_gc_between_element_settlements() {
    let interp = run_steps_with_major_gc_between(&[
        r#"
        globalThis.outcome = "pending";
        (function () {
            const first = new Promise((_, reject) => { globalThis.rejectFirst = reject; });
            const second = new Promise((_, reject) => { globalThis.rejectSecond = reject; });
            Promise.any([first, second]).catch((error) => {
                globalThis.outcome = "any:" + error.errors[0].marker + "," +
                    error.errors[1].marker;
            });
        })();
        "#,
        r#"
        (function () {
            var settle = globalThis.rejectFirst;
            delete globalThis.rejectFirst;
            settle({ marker: "first" });
        })();
        "#,
        r#"
        (function () {
            var settle = globalThis.rejectSecond;
            delete globalThis.rejectSecond;
            settle({ marker: "second" });
        })();
        "#,
    ]);
    assert_eq!(global_string(&interp, "outcome"), "any:first,second");
}

#[test]
fn promise_finally_runs_callback_across_major_gc() {
    let interp = run_steps_with_major_gc_between(&[
        r#"
        globalThis.outcome = "pending";
        (function () {
            const inner = new Promise((resolve) => { globalThis.release = resolve; });
            inner.finally(() => { globalThis.outcome = "ran"; });
        })();
        "#,
        r#"
        (function () {
            var settle = globalThis.release;
            delete globalThis.release;
            settle("v");
        })();
        "#,
    ]);
    assert_eq!(global_string(&interp, "outcome"), "ran");
}

#[test]
fn promise_finally_forwards_value_across_major_gc() {
    let interp = run_steps_with_major_gc_between(&[
        r#"
        globalThis.outcome = "pending";
        (function () {
            const inner = new Promise((resolve) => { globalThis.release = resolve; });
            inner.finally(() => {}).then((v) => { globalThis.outcome = "value:" + v.marker; });
        })();
        "#,
        r#"
        (function () {
            var settle = globalThis.release;
            delete globalThis.release;
            settle({ marker: "kept" });
        })();
        "#,
    ]);
    assert_eq!(global_string(&interp, "outcome"), "value:kept");
}

/// Number of GC pins recorded on the object held by global `name`.
fn global_pin_count(interp: &Interpreter, name: &str) -> usize {
    let value = interp
        .get_global_var_ref(name)
        .unwrap_or_else(|| panic!("expected global {name}"));
    let id = value
        .as_object_id()
        .unwrap_or_else(|| panic!("global {name} must be an object"));
    interp
        .get_object_cell(id)
        .expect("global object must be live")
        .borrow()
        .gc_native_roots
        .as_ref()
        .map_or(0, Vec::len)
}

#[test]
fn combinator_pins_do_not_accumulate_on_a_reused_capability_function() {
    // A constructor may hand the same resolving function to every capability it
    // builds. Pins are never removed, so anchoring the accumulated values on the
    // capability function would leave one settled value pinned per call — growth
    // without bound. The anchor must be per-invocation instead.
    let interp = run_steps_with_major_gc_between(&[r#"
        globalThis.sharedResolve = function () {};
        globalThis.sharedReject = function () {};
        globalThis.Ctor = function (executor) {
            executor(globalThis.sharedResolve, globalThis.sharedReject);
        };
        globalThis.Ctor.resolve = function (value) { return Promise.resolve(value); };

        for (var i = 0; i < 50; i++) {
            Promise.all.call(globalThis.Ctor, [{ marker: i }]);
            Promise.allSettled.call(globalThis.Ctor, [{ marker: i }]);
        }
        "#]);

    assert_eq!(
        global_pin_count(&interp, "sharedResolve"),
        0,
        "a reused capability resolve function must not accumulate combinator pins"
    );
    assert_eq!(
        global_pin_count(&interp, "sharedReject"),
        0,
        "a reused capability reject function must not accumulate combinator pins"
    );
}

#[test]
fn gc_keeps_module_exports_alive_until_registry_entry_is_removed() {
    let dir = temp_case_dir("module-gc");
    let main_path = write_case_file(&dir, "main.js", r#"export const obj = { marker: 1 };"#);

    let mut interp = run_module_with_path(&fs::read_to_string(&main_path).unwrap(), &main_path);
    let module_key = ModuleKey::for_file(main_path.clone());
    let realm_id = interp.current_realm_id;
    let key = (realm_id, module_key);
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
    let object_id = export_val.as_object_id().expect("expected exported object");

    interp.gc.request();
    interp.gc_safepoint();
    assert!(interp.get_object_cell(object_id).is_some());

    interp.module_registry.remove(&key);
    interp.gc.request();
    interp.gc_safepoint();
    assert!(interp.get_object_cell(object_id).is_none());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn array_literal_releases_temp_roots_after_abrupt_completion() {
    let interp = run_script(
        r#"
        try {
            [{ marker: "ordinary" }, ...[{ marker: "spread" }], ...null];
        } catch (e) {}
        "#,
    );

    assert!(
        interp.gc_temp_roots.is_empty(),
        "array literal temporary roots must be released after a throw"
    );
}

#[test]
fn private_update_releases_temp_roots_after_abrupt_completion() {
    // The private update branch roots its receiver across
    // PrivateGet -> ToNumeric -> PrivateSet. Every abrupt exit inside that
    // window must still release the temp-root frame.
    let interp = run_script(
        r#"
        class C {
            get #x() { return { valueOf() { throw new Error("coercion"); } }; }
            set #x(v) {}
            static bump() { return (new C()).#x++; }
        }
        try { C.bump(); } catch (e) {}
        "#,
    );

    assert!(
        interp.gc_temp_roots.is_empty(),
        "private update temporary roots must be released after a throw"
    );
}

#[test]
fn private_logical_assign_releases_temp_roots_after_abrupt_completion() {
    // Same contract for the logical-assignment branch, whose rooted window
    // spans PrivateGet -> right-hand side evaluation -> PrivateSet.
    let interp = run_script(
        r#"
        class C {
            get #x() { return undefined; }
            set #x(v) {}
            static assign() {
                return (new C()).#x ??= (function () { throw new Error("rhs"); })();
            }
        }
        try { C.assign(); } catch (e) {}
        "#,
    );

    assert!(
        interp.gc_temp_roots.is_empty(),
        "private logical-assignment temporary roots must be released after a throw"
    );
}

#[test]
fn private_destructuring_releases_temp_roots_after_abrupt_completion() {
    // Destructuring evaluates each private target before iterator/property
    // access. Throws from those intervening operations must release both the
    // private receiver's scoped root and the array iterator's existing root.
    let interp = run_script(
        r#"
        var throwingIterable = {
            [Symbol.iterator]: function () {
                return {
                    next: function () { throw new Error("iterator step"); }
                };
            }
        };
        var throwingSource = {
            get item() { throw new Error("source getter"); }
        };
        class C {
            set #x(v) {}
            static element() { [(new C()).#x] = throwingIterable; }
            static rest() { [...(new C()).#x] = throwingIterable; }
            static property() { ({ item: (new C()).#x } = throwingSource); }
        }
        try { C.element(); } catch (e) {}
        try { C.rest(); } catch (e) {}
        try { C.property(); } catch (e) {}
        "#,
    );

    assert!(
        interp.gc_temp_roots.is_empty(),
        "private destructuring temporary roots must be released after a throw"
    );
}

#[test]
fn computed_member_destructuring_releases_temp_roots_after_abrupt_key_evaluation() {
    // The member base is rooted while its computed key is evaluated. An abrupt
    // key completion must release that scoped root before propagating the throw.
    let interp = run_script(
        r#"
        function fail() { throw new Error("computed key"); }
        try { [Object.create({})[fail()]] = [1]; } catch (e) {}
        try { ({ item: Object.create({})[fail()] } = { item: 1 }); } catch (e) {}
        "#,
    );

    assert!(
        interp.gc_temp_roots.is_empty(),
        "computed member target roots must be released after key evaluation throws"
    );
}

#[test]
fn private_method_call_with_non_iterable_spread_throws() {
    // A spread argument that is not iterable must throw a TypeError, even when the
    // callee is a private method reached through `this.#m(...)`. The private-call
    // path used to hand-roll argument evaluation and silently drop the iteration
    // error, invoking the method with zero arguments instead of throwing.
    let interp = run_script(
        r#"
        var result = "";
        class C {
            #m() { return "called"; }
            run() { return this.#m(...5); }
        }
        try {
            new C().run();
            result = "no-throw";
        } catch (e) {
            result = e.constructor.name;
        }
        "#,
    );
    assert_eq!(global_string(&interp, "result"), "TypeError");
}

#[test]
fn private_method_call_forwards_iterable_spread_arguments() {
    // The private-method call path must forward spread arguments from an iterable,
    // matching the ordinary member-call path.
    let interp = run_script(
        r#"
        class D {
            #m(a, b, c) { return a + b + c; }
            run() { return this.#m(1, ...[2, 3]); }
        }
        var result = new D().run();
        "#,
    );
    assert_eq!(global_number(&interp, "result"), 6.0);
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

// -- Atomics / SharedArrayBuffer edge cases (issue #25) ----------------------------
// Deterministic, single-threaded coverage of Atomics plumbing that test262 mostly
// exercises only through the real multi-agent $262.agent harness. Cross-thread
// wake-up itself is deliberately NOT tested here (timing-dependent, and already
// exercised by test262's own agent tests) — these instead lock down the branch
// logic directly, setting `can_block` on the interpreter rather than spawning a
// real OS thread.

#[test]
fn atomics_wait_on_matching_value_throws_on_non_blocking_main_agent() {
    let interp = run_script(
        r#"
        var result = "";
        var sab = new SharedArrayBuffer(4);
        var view = new Int32Array(sab);
        try {
            Atomics.wait(view, 0, 0);
            result = "no-throw";
        } catch (e) {
            result = e.constructor.name;
        }
        "#,
    );
    assert_eq!(global_string(&interp, "result"), "TypeError");
}

#[test]
fn atomics_wait_value_mismatch_returns_not_equal_without_blocking() {
    let start = std::time::Instant::now();
    let interp = run_script(
        r#"
        var sab = new SharedArrayBuffer(4);
        var view = new Int32Array(sab);
        var result = Atomics.wait(view, 0, 1, Infinity);
        "#,
    );
    assert_eq!(global_string(&interp, "result"), "not-equal");
    assert!(
        start.elapsed() < std::time::Duration::from_secs(2),
        "a value mismatch must return immediately, never touching the wait/notify blocking path"
    );
}

#[test]
fn atomics_wait_zero_timeout_on_blocking_agent_returns_timed_out_immediately() {
    let start = std::time::Instant::now();
    let interp = run_script_as_blocking_agent(
        r#"
        var sab = new SharedArrayBuffer(4);
        var view = new Int32Array(sab);
        var result = Atomics.wait(view, 0, 0, 0);
        "#,
    );
    assert_eq!(global_string(&interp, "result"), "timed-out");
    assert!(
        start.elapsed() < std::time::Duration::from_secs(2),
        "a zero timeout must never register a waiter or actually block"
    );
}

#[test]
fn atomics_notify_on_non_shared_buffer_returns_zero_without_throwing() {
    let interp = run_script(
        r#"
        var view = new Int32Array(new ArrayBuffer(4));
        var result = Atomics.notify(view, 0);
        "#,
    );
    assert_eq!(global_number(&interp, "result"), 0.0);
}

#[test]
fn atomics_notify_on_shared_buffer_with_no_waiters_returns_zero() {
    let interp = run_script(
        r#"
        var sab = new SharedArrayBuffer(4);
        var view = new Int32Array(sab);
        var result = Atomics.notify(view, 0);
        "#,
    );
    assert_eq!(global_number(&interp, "result"), 0.0);
}

#[test]
fn atomics_add_on_non_integer_typed_array_throws_type_error() {
    let interp = run_script(
        r#"
        var result = "";
        var sab = new SharedArrayBuffer(8);
        var view = new Float64Array(sab);
        try {
            Atomics.add(view, 0, 1);
            result = "no-throw";
        } catch (e) {
            result = e.constructor.name;
        }
        "#,
    );
    assert_eq!(global_string(&interp, "result"), "TypeError");
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

#[test]
fn typed_array_define_own_property_coerces_string_value_via_tonumber() {
    // A typed array's [[DefineOwnProperty]] runs IntegerIndexedElementSet, which
    // coerces a String [[Value]] with the canonical ToNumber (§7.1.4). Reached via
    // Object.defineProperties with a raw descriptor value, this must honour the
    // 0x/0o/0b prefixes, trim exactly the ECMAScript whitespace set, and treat
    // "inf" as NaN — not defer to a naive float parse.
    let interp = run_script(
        r#"
        function define(TA, v) { var a = new TA(1); Object.defineProperties(a, { 0: { value: v } }); return a[0]; }
        var hexInt8 = define(Int8Array, "0x10");
        var octInt8 = define(Int8Array, "0o17");
        var binInt8 = define(Int8Array, "0b101");
        var wsInt8  = define(Int8Array, "\t\n\r 5 ");
        var hexF64  = define(Float64Array, "0x10");
        var wsF64   = define(Float64Array, " 3.5 ");
        var infF64  = String(define(Float64Array, "inf"));
        var num300  = define(Int8Array, 300);
        var boolT   = define(Int8Array, true);
        "#,
    );
    assert_eq!(global_number(&interp, "hexInt8"), 16.0);
    assert_eq!(global_number(&interp, "octInt8"), 15.0);
    assert_eq!(global_number(&interp, "binInt8"), 5.0);
    assert_eq!(global_number(&interp, "wsInt8"), 5.0);
    assert_eq!(global_number(&interp, "hexF64"), 16.0);
    assert_eq!(global_number(&interp, "wsF64"), 3.5);
    assert_eq!(global_string(&interp, "infF64"), "NaN");
    // Non-String branches of ToNumber are unchanged.
    assert_eq!(global_number(&interp, "num300"), 44.0);
    assert_eq!(global_number(&interp, "boolT"), 1.0);
}

// -- Resizable ArrayBuffer / growable SharedArrayBuffer invariants (issue #25) -----
// Whitebox checks on the length-tracking bookkeeping in types.rs. A bug here is a
// Rust panic or an out-of-bounds slice access, not just a wrong JS-visible value, so
// these need direct Rust coverage rather than relying on test262 alone.

#[test]
fn length_tracking_view_recomputes_length_after_resizable_buffer_grows() {
    let interp = run_script(
        r#"
        var buf = new ArrayBuffer(4, { maxByteLength: 16 });
        var view = new Int32Array(buf);
        buf.resize(16);
        "#,
    );
    let ta_obj = interp
        .get_object(global_object_id(&interp, "view"))
        .unwrap();
    let ta_ref = ta_obj.borrow();
    let ta = ta_ref.typed_array_info().unwrap();
    assert!(
        ta.is_length_tracking,
        "omitted length over a resizable buffer must be length-tracking"
    );
    assert_eq!(
        typed_array_length(ta),
        4,
        "length must recompute to the grown buffer's element count"
    );
    assert!(!is_typed_array_out_of_bounds(ta));
}

#[test]
fn length_tracking_view_with_offset_goes_out_of_bounds_after_shrink_below_offset() {
    let interp = run_script(
        r#"
        var buf = new ArrayBuffer(16, { maxByteLength: 16 });
        var view = new Int32Array(buf, 8);
        buf.resize(4);
        "#,
    );
    let ta_obj = interp
        .get_object(global_object_id(&interp, "view"))
        .unwrap();
    let ta_ref = ta_obj.borrow();
    let ta = ta_ref.typed_array_info().unwrap();
    assert!(
        is_typed_array_out_of_bounds(ta),
        "byte_offset (8) now exceeds the shrunk buffer length (4)"
    );
    assert_eq!(
        typed_array_length(ta),
        0,
        "an out-of-bounds length-tracking view must saturate to zero, not underflow"
    );
}

#[test]
fn length_tracking_view_recomputes_length_after_growable_shared_buffer_grows() {
    let interp = run_script(
        r#"
        var sab = new SharedArrayBuffer(4, { maxByteLength: 16 });
        var view = new Int32Array(sab);
        sab.grow(16);
        "#,
    );
    let ta_obj = interp
        .get_object(global_object_id(&interp, "view"))
        .unwrap();
    let ta_ref = ta_obj.borrow();
    let ta = ta_ref.typed_array_info().unwrap();
    assert!(ta.is_length_tracking);
    assert_eq!(typed_array_length(ta), 4);
    assert!(!is_typed_array_out_of_bounds(ta));
}

#[test]
fn explicit_length_view_over_resizable_buffer_is_not_fixed_length() {
    let interp = run_script(
        r#"
        var buf = new ArrayBuffer(16, { maxByteLength: 16 });
        var view = new Int32Array(buf, 0, 2);
        "#,
    );
    let ta_obj = interp
        .get_object(global_object_id(&interp, "view"))
        .unwrap();
    let ta_ref = ta_obj.borrow();
    let ta = ta_ref.typed_array_info().unwrap();
    let buf_obj = interp.get_object(global_object_id(&interp, "buf")).unwrap();
    let buf_ref = buf_obj.borrow();
    assert!(
        !is_typed_array_fixed_length(ta, &buf_ref),
        "an explicit-length view over a resizable (non-shared) buffer can still go \
         out-of-bounds if the buffer shrinks, so it is not IsTypedArrayFixedLength"
    );
}

#[test]
fn explicit_length_view_over_growable_shared_buffer_is_fixed_length() {
    let interp = run_script(
        r#"
        var sab = new SharedArrayBuffer(16, { maxByteLength: 16 });
        var view = new Int32Array(sab, 0, 2);
        "#,
    );
    let ta_obj = interp
        .get_object(global_object_id(&interp, "view"))
        .unwrap();
    let ta_ref = ta_obj.borrow();
    let ta = ta_ref.typed_array_info().unwrap();
    let buf_obj = interp.get_object(global_object_id(&interp, "sab")).unwrap();
    let buf_ref = buf_obj.borrow();
    assert!(
        is_typed_array_fixed_length(ta, &buf_ref),
        "a growable SharedArrayBuffer can only grow, never shrink, so an explicit-length \
         view over it stays IsTypedArrayFixedLength"
    );
}

#[test]
fn is_valid_integer_index_rejects_stale_and_malformed_indices() {
    let interp = run_script(
        r#"
        var buf = new ArrayBuffer(16, { maxByteLength: 16 });
        var view = new Int32Array(buf);
        buf.resize(8);
        "#,
    );
    let ta_obj = interp
        .get_object(global_object_id(&interp, "view"))
        .unwrap();
    let ta_ref = ta_obj.borrow();
    let ta = ta_ref.typed_array_info().unwrap();
    assert!(
        is_valid_integer_index(ta, 1.0),
        "index 1 is still within the shrunk length (2)"
    );
    assert!(
        !is_valid_integer_index(ta, 3.0),
        "index 3 was valid before the shrink but must be rejected now"
    );
    assert!(!is_valid_integer_index(ta, f64::NAN));
    assert!(!is_valid_integer_index(ta, f64::INFINITY));
    assert!(
        !is_valid_integer_index(ta, -0.0),
        "-0 is not a valid integer index"
    );
    assert!(!is_valid_integer_index(ta, -1.0));
    assert!(
        !is_valid_integer_index(ta, 1.5),
        "non-integer index must be rejected"
    );
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
        .set_property_value("x", JsValue::number(1.0));
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
        .set_property_value("x", JsValue::number(1.0));
    let before = interp.get_object(id).unwrap().borrow().shape_id;
    interp
        .get_object(id)
        .unwrap()
        .borrow_mut()
        .set_property_value("x", JsValue::number(2.0));
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
        .set_property_value("x", JsValue::number(1.0));
    let before = interp.get_object(id).unwrap().borrow().shape_id;
    let ok = interp
        .get_object(id)
        .unwrap()
        .borrow_mut()
        .define_own_property(
            "x".to_string(),
            PropertyDescriptor {
                value: Some(JsValue::number(1.0)),
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
        .set_property_value("x", JsValue::number(1.0));
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
                get: Some(JsValue::UNDEFINED), // sentinel — real getter not needed for this test
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
        let fn_id = desc
            .value
            .expect("method has a function value")
            .as_object_id()
            .unwrap_or_else(|| panic!("Array.prototype.{name} is not a function object"));
        let fn_cell = interp.get_object_cell_expect(fn_id);
        let actual_name = fn_cell
            .borrow()
            .get_own_property("name")
            .unwrap()
            .value
            .and_then(|value| value.as_string())
            .unwrap_or_else(|| panic!("Array.prototype.{name}: expected name string"));
        assert_eq!(actual_name.to_rust_string(), *name);
        let actual_length = fn_cell
            .borrow()
            .get_own_property("length")
            .unwrap()
            .value
            .and_then(|value| value.as_number())
            .unwrap_or_else(|| panic!("Array.prototype.{name}: expected length number"));
        assert_eq!(actual_length, *len, "Array.prototype.{name}.length");
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
    let values_id = values_desc
        .value
        .expect("values has a function value")
        .as_object_id()
        .expect("values must be a function object");
    let iterator_id = iterator_desc
        .value
        .expect("@@iterator has a function value")
        .as_object_id()
        .expect("@@iterator must be a function object");
    assert_eq!(
        values_id, iterator_id,
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

// #165 / #468: both per-Body caches pin each Body they memoise, so either
// unbounded table would retain every `new Function` / `eval` Body the program
// ever ran. This pins both independent bounds through the normal call path.
#[test]
fn per_body_caches_stay_bounded_across_many_distinct_dynamic_function_bodies() {
    let bodies = super::hoist_cache::DEFAULT_CAPACITY + 2000;
    let interp = run_script(&format!(
        r#"
        var sum = 0;
        for (var i = 0; i < {bodies}; i++) {{
          // A distinct source per iteration, so each call dispatches a body the
          // cache has never seen, with both var and Annex-B names to collect.
          var f = new Function("var x" + i + " = 1; {{ function g" + i + "(){{}} }} return x" + i + ";");
          sum += f();
        }}
        globalThis.__sum = sum;
        "#
    ));
    assert_eq!(global_number(&interp, "__sum"), bodies as f64);
    assert!(
        interp.hoist_cache.len() <= super::hoist_cache::DEFAULT_CAPACITY,
        "hoist cache grew to {} entries for {bodies} dynamic bodies",
        interp.hoist_cache.len()
    );
    // Guards against the bound passing vacuously: these bodies must really be
    // reaching the cache, which means it fills and then evicts.
    assert!(
        interp.hoist_cache.len() > super::hoist_cache::DEFAULT_CAPACITY / 2,
        "expected the dynamic bodies to fill the cache, got {} entries",
        interp.hoist_cache.len()
    );
    assert!(
        interp.ic_store.len() <= super::ic_store::DEFAULT_CAPACITY,
        "IC store grew to {} entries for {bodies} dynamic bodies",
        interp.ic_store.len()
    );
    assert!(
        interp.ic_store.slot_count() <= super::ic_store::DEFAULT_CAPACITY,
        "IC store allocated {} slots for {bodies} dynamic bodies",
        interp.ic_store.slot_count()
    );
    assert!(
        interp.ic_store.len() > super::ic_store::DEFAULT_CAPACITY / 2,
        "expected the dynamic bodies to fill the IC store, got {} entries",
        interp.ic_store.len()
    );
}

#[test]
fn hoist_cache_still_reuses_one_analysis_across_repeated_calls() {
    let interp = run_script(
        r#"
        function f(n) { var acc = 0; for (var i = 0; i < n; i++) { acc += i; } return acc; }
        var total = 0;
        for (var k = 0; k < 50; k++) { total += f(3); }
        globalThis.__total = total;
        "#,
    );
    assert_eq!(global_number(&interp, "__total"), 150.0);
    assert!(
        interp.hoist_cache.hits() >= 49,
        "expected the repeatedly-called body to be memoised, got {} hits",
        interp.hoist_cache.hits()
    );
}

// Loop and labelled-statement completion values are the observable surface of
// the `v = val` folding consolidated into `handle_loop_body_completion`
// (src/interpreter/exec.rs). These pin the behaviour across the six loop heads
// migrated to the shared helper, plus the two deliberately-excluded forms
// (for-of interleaves IteratorClose; switch has no `continue` arm). Expected
// values are the ECMAScript completion-value semantics, cross-checked against
// Node — not recomputed from jsse's code.
mod loop_completion_value_tests {
    use super::*;

    fn completion_value(source: &str) -> JsValue {
        let program = parse_program(source);
        let mut interp = Interpreter::new();
        match interp.run(&program) {
            Completion::Normal(v) => v,
            other => panic!("unexpected completion for `{source}`: {other:?}"),
        }
    }

    fn assert_number(source: &str, expected: f64) {
        let value = completion_value(source);
        assert_eq!(
            value.as_number(),
            Some(expected),
            "completion value of `{source}`"
        );
    }

    #[test]
    fn while_loop_yields_last_body_value() {
        assert_number("var i=0; while(i<3){ i++; 42; }", 42.0);
    }

    #[test]
    fn do_while_yields_last_body_value() {
        assert_number("do { 5; } while(false)", 5.0);
    }

    #[test]
    fn for_loop_yields_last_body_value() {
        assert_number("for(var i=0;i<1;i++){ 7; }", 7.0);
    }

    #[test]
    fn for_in_yields_last_body_value() {
        assert_number("for(var k in {a:1,b:2}){ 33; }", 33.0);
    }

    #[test]
    fn while_break_with_value_wins() {
        assert_number(
            "var n=0; while(n<3){ n++; if(n===2){ 99; break; } 44; }",
            99.0,
        );
    }

    #[test]
    fn labelled_for_continue_carries_value() {
        assert_number(
            "outer: for(var i=0;i<2;i++){ for(var j=0;j<2;j++){ 9; continue outer; } }",
            9.0,
        );
    }

    #[test]
    fn labelled_while_body_value_after_inner_break() {
        assert_number(
            "var o=0; outer: while(o<2){ o++; inner: while(true){ 11; break inner; } 22; }",
            22.0,
        );
    }

    #[test]
    fn labelled_do_while_continue_then_body_value() {
        assert_number(
            "var m=0; outer: do { m++; if(m<3){ 55; continue outer; } 66; } while(m<3)",
            66.0,
        );
    }

    #[test]
    fn labelled_for_in_continue_carries_value() {
        assert_number(
            "outer: for(var k in {a:1,b:2}){ for(var j=0;j<2;j++){ 77; continue outer; } }",
            77.0,
        );
    }

    #[test]
    fn labelled_break_out_of_for_carries_value() {
        assert_number(
            "L: for (var i=0;i<3;i++){ if(i===1){ 71; break L; } }",
            71.0,
        );
    }

    #[test]
    fn labelled_block_break_yields_value() {
        assert_number("l: { 1; break l; }", 1.0);
    }

    // Guard: for-of and switch are deliberately NOT routed through the shared
    // helper. These pin that the refactor left them unaffected.
    #[test]
    fn for_of_completion_value_unaffected() {
        assert_number("for(var x of [1,2,3]){ x*10; }", 30.0);
    }

    #[test]
    fn for_of_break_with_value_unaffected() {
        assert_number("for(var x of [1,2,3]){ if(x===2){ 88; break; } }", 88.0);
    }

    #[test]
    fn switch_break_with_value_unaffected() {
        assert_number("switch(1){ case 1: 111; break; }", 111.0);
    }
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
        assert_eq!(conv(JsValue::number(3.7)), 3.0);
        assert_eq!(conv(JsValue::number(-3.7)), -3.0);
        assert_eq!(conv(JsValue::number(5.0)), 5.0);
        assert_eq!(conv(JsValue::number(-0.9)), 0.0);
    }

    #[test]
    fn nan_becomes_positive_zero() {
        let r = conv(JsValue::number(f64::NAN));
        assert_eq!(r, 0.0);
        assert!(r.is_sign_positive(), "ToIntegerOrInfinity(NaN) is +0");
    }

    #[test]
    fn infinities_pass_through() {
        assert_eq!(conv(JsValue::number(f64::INFINITY)), f64::INFINITY);
        assert_eq!(conv(JsValue::number(f64::NEG_INFINITY)), f64::NEG_INFINITY);
    }

    #[test]
    fn coerces_booleans_null_and_undefined() {
        assert_eq!(conv(JsValue::TRUE), 1.0); // ToNumber(true) = 1
        assert_eq!(conv(JsValue::FALSE), 0.0);
        assert_eq!(conv(JsValue::NULL), 0.0); // ToNumber(null) = 0
        assert_eq!(conv(JsValue::UNDEFINED), 0.0); // ToNumber(undefined) = NaN -> 0
    }

    #[test]
    fn coerces_strings() {
        assert_eq!(conv(JsValue::from_str("42")), 42.0);
        assert_eq!(conv(JsValue::from_str("42.9")), 42.0);
        assert_eq!(conv(JsValue::from_str("  -7.5 ")), -7.0);
        assert_eq!(conv(JsValue::from_str("abc")), 0.0); // NaN -> 0
        assert_eq!(conv(JsValue::from_str("Infinity")), f64::INFINITY);
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

/// §7.1.4.1 StringToNumber through the public `Number(...)` seam. Expected
/// values are the ECMA-262 truth table (cross-checked against node): these lock
/// the three divergences the deepened conversion fixes — Rust's whitespace set,
/// hex overflow, and leaked `inf`/`nan` spellings. Whitespace chars are built
/// with `String.fromCharCode` so the source stays ASCII-clean.
mod string_to_number_seam_tests {
    use super::*;

    #[test]
    fn ecmascript_whitespace_is_trimmed_but_rust_only_whitespace_is_not() {
        let interp = run_script(
            r#"
            var nel    = Number(String.fromCharCode(0x85) + "1");   // U+0085 NEL: not WhiteSpace
            var zwnbsp = Number(String.fromCharCode(0xFEFF) + "1"); // U+FEFF ZWNBSP: WhiteSpace
            var mixed  = Number("\t\n\r 5 \t");
            "#,
        );
        assert!(global_number(&interp, "nel").is_nan());
        assert_eq!(global_number(&interp, "zwnbsp"), 1.0);
        assert_eq!(global_number(&interp, "mixed"), 5.0);
    }

    #[test]
    fn only_the_capitalized_infinity_word_is_infinite() {
        let interp = run_script(
            r#"
            var inf   = Number("inf");
            var pInf  = Number("+Infinity");
            var nInf  = Number("-Infinity");
            var lc    = Number("infinity");
            var nan   = +"nan";
            "#,
        );
        assert!(global_number(&interp, "inf").is_nan());
        assert_eq!(global_number(&interp, "pInf"), f64::INFINITY);
        assert_eq!(global_number(&interp, "nInf"), f64::NEG_INFINITY);
        assert!(global_number(&interp, "lc").is_nan());
        assert!(global_number(&interp, "nan").is_nan());
    }

    #[test]
    fn large_non_decimal_literals_round_instead_of_overflowing() {
        let interp = run_script(
            r#"
            var big   = Number("0x10000000000000000"); // 2**64
            var exact = Number("0x6269e107215582e");
            var carry = Number("0x200000000000011");
            var small = Number("0o17");
            var empty = Number("0x");
            "#,
        );
        assert_eq!(global_number(&interp, "big"), 2f64.powi(64));
        assert_eq!(global_number(&interp, "exact"), 443_215_406_813_239_360.0);
        assert_eq!(global_number(&interp, "carry"), 2f64.powi(57) + 32.0);
        assert_eq!(global_number(&interp, "small"), 15.0);
        assert!(global_number(&interp, "empty").is_nan());
    }
}

/// §10.4.3 String-exotic objects expose an own indexed character property only
/// when the key is the CanonicalNumericIndexString of an in-range integer.
/// These exercise the public seams (member read, `in`, GetOwnPropertyDescriptor,
/// DefineProperty, delete, Reflect) that all route through `string_exotic_index`.
/// Expected values are node's (spec-conformant) results as known-good literals.
mod string_exotic_index_seam_tests {
    use super::*;

    #[test]
    fn non_canonical_index_keys_are_not_own_string_properties() {
        // "01"/"+1"/"1.0" are not CanonicalNumericIndexStrings, so they name no
        // own character property of the string; only "1" does.
        let interp = run_script(
            r#"
            var s = "abc";
            var o = Object("abc");
            var report = [
                String(s["01"]),
                String(s["+1"]),
                String(s["1.0"]),
                String(s["1"]),
                String(o["01"]),
                ("01" in o),
                ("1" in o),
                (Object.getOwnPropertyDescriptor(o, "01") === undefined),
                Object.keys(o).join(",")
            ].join("|");
            "#,
        );
        assert_eq!(
            global_string(&interp, "report"),
            "undefined|undefined|undefined|b|undefined|false|true|true|0,1,2"
        );
    }

    #[test]
    fn define_property_on_non_canonical_index_is_allowed() {
        // "01" is an ordinary property key, so defineProperty must succeed and
        // the value must read back (the string-index redefinition guard must
        // not fire for a look-alike key).
        let interp = run_script(
            r#"
            var o = Object("abc");
            Object.defineProperty(o, "01", {
                value: "z", writable: true, configurable: true, enumerable: true
            });
            // [[Set]] of a look-alike key must also store an ordinary property
            // (the non-writable string-index guard must not fire for "+2").
            var p = Object("abc");
            p["+2"] = "w";
            var report = String(o["01"]) + "|" + String(p["+2"]);
            "#,
        );
        assert_eq!(global_string(&interp, "report"), "z|w");
    }

    #[test]
    fn delete_distinguishes_canonical_indices_from_look_alikes() {
        // Own indices are non-configurable (delete -> false); look-alikes are
        // ordinary absent properties (delete -> true). Indexing is by UTF-16
        // code unit, so both halves of a non-BMP code point are in range.
        let interp = run_script(
            r#"
            var report = [
                delete Object("abc")["01"],
                delete Object("abc")[1],
                Reflect.deleteProperty(Object("abc"), "1"),
                Reflect.deleteProperty(Object("abc"), "01"),
                Reflect.deleteProperty(Object("\u{1F4A9}"), "1"),
                Reflect.deleteProperty(Object("\u{1F4A9}"), "0")
            ].join(",");
            "#,
        );
        assert_eq!(
            global_string(&interp, "report"),
            "true,false,false,true,false,false"
        );
    }

    #[test]
    fn strict_delete_of_own_string_index_throws_but_look_alike_succeeds() {
        // Index 1 is a non-configurable own property, so strict-mode delete
        // throws a TypeError; "01" is deletable and returns true.
        let interp = run_script(
            r#"
            "use strict";
            var deleted01 = delete Object("abc")["01"];
            var threw = false;
            try { delete Object("abc")[1]; } catch (e) { threw = (e instanceof TypeError); }
            var report = String(deleted01) + "|" + String(threw);
            "#,
        );
        assert_eq!(global_string(&interp, "report"), "true|true");
    }
}

/// §13.3.7 Optional Chains and §6.2.5.5 GetValue require primitive property
/// references to perform wrapper [[Get]] with the primitive as the receiver.
mod optional_chain_primitive_get_tests {
    use super::*;

    #[test]
    fn prototype_accessors_are_invoked_with_the_primitive_receiver() {
        run_script(
            r#"
            function install(proto, key, expectedThis, result) {
                Object.defineProperty(proto, key, {
                    configurable: true,
                    get: function () {
                        "use strict";
                        if (this !== expectedThis) {
                            throw new Error("getter received the wrong primitive");
                        }
                        return result;
                    }
                });
            }

            var symbol = Symbol("receiver");
            install(String.prototype, "01", "abc", 41);
            install(String.prototype, "5", "abc", 42);
            install(Number.prototype, "optionalAccessor", 5, 43);
            install(Boolean.prototype, "optionalAccessor", true, 44);
            install(Symbol.prototype, "optionalAccessor", symbol, 45);
            install(BigInt.prototype, "optionalAccessor", 7n, 46);

            if ("abc"?.["01"] !== 41) throw new Error("String look-alike getter skipped");
            if ("abc"?.["5"] !== 42) throw new Error("String out-of-range getter skipped");
            if ((5)?.optionalAccessor !== 43) throw new Error("Number getter skipped");
            if ((true)?.["optionalAccessor"] !== 44) throw new Error("Boolean getter skipped");
            if (symbol?.optionalAccessor !== 45) throw new Error("Symbol getter skipped");
            if ((7n)?.["optionalAccessor"] !== 46) throw new Error("BigInt getter skipped");

            var marker = {};
            Object.defineProperty(Boolean.prototype, "throwingOptionalAccessor", {
                configurable: true,
                get: function () { throw marker; }
            });
            var propagated = false;
            try {
                (false)?.throwingOptionalAccessor;
            } catch (error) {
                propagated = error === marker;
            }
            if (!propagated) throw new Error("getter exception was not propagated");
            "#,
        );
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
              typeof __host_proxy_target,
              typeof __host_array_extra_keys,
            ].join(",");
            "#,
        );
        assert_eq!(
            global_string(&interp, "r"),
            "undefined,undefined,undefined,undefined,undefined,undefined"
        );
    }

    #[test]
    fn host_globals_are_non_enumerable_functions() {
        assert_node_ok(
            r#"
            for (const name of ["__host_write","__host_exit","__host_hrtime","__host_random_bytes","__host_proxy_target","__host_array_extra_keys"]) {
              const d = Object.getOwnPropertyDescriptor(globalThis, name);
              if (!d) throw new Error(name + " missing");
              if (d.enumerable) throw new Error(name + " is enumerable");
              // node-shim.js deletes its metadata hooks under "use strict" to
              // keep the seams private; a non-configurable binding would make
              // that delete throw and kill the prelude.
              if (!d.configurable) throw new Error(name + " is not configurable");
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
    fn host_proxy_target_bypasses_handler_traps() {
        assert_node_ok(
            r#"
            let calls = 0;
            const target = { value: 1 };
            const proxy = new Proxy(target, {
              get() { calls++; throw new Error("get trap"); },
              getPrototypeOf() { calls++; throw new Error("getPrototypeOf trap"); },
              ownKeys() { calls++; throw new Error("ownKeys trap"); },
              getOwnPropertyDescriptor() {
                calls++;
                throw new Error("getOwnPropertyDescriptor trap");
              },
            });
            if (__host_proxy_target(proxy) !== target) throw new Error("wrong target");
            if (__host_proxy_target(target) !== undefined) throw new Error("ordinary object");
            if (__host_proxy_target(1) !== undefined) throw new Error("primitive");
            if (calls !== 0) throw new Error("handler trap invoked");

            const revocable = Proxy.revocable({}, {});
            revocable.revoke();
            if (__host_proxy_target(revocable.proxy) !== null) {
              throw new Error("revoked Proxy marker");
            }
            "#,
        );
    }

    #[test]
    fn host_array_extra_keys_uses_named_metadata_without_getting_values() {
        assert_node_ok(
            r#"
            let getterCalls = 0;
            // The Array storage shapes must agree even though their dense-index
            // descriptor layouts differ. The host hook reads the dedicated
            // non-index String-key order, never either dense representation.
            const builders = {
              literal: () => [1, 2, 3, 4, 5],
              fill: () => new Array(5).fill(1),
              arrayFrom: () => Array.from({ length: 5 }, () => 1),
            };
            for (const [shape, build] of Object.entries(builders)) {
              const a = build();
              a.z = 2;
              Object.defineProperty(a, "getter", {
                get() { getterCalls++; throw new Error("named getter ran"); },
                enumerable: true,
                configurable: true,
              });
              Object.defineProperty(a, "hidden", {
                value: 3,
                enumerable: false,
                configurable: true,
              });
              Object.defineProperty(a, "2", {
                get() { getterCalls++; throw new Error("index getter ran"); },
                enumerable: true,
                configurable: true,
              });
              a["4294967295"] = "max";
              a["-0"] = "minus";
              Object.defineProperty(a, "+1", {
                value: "plus",
                enumerable: true,
                configurable: true,
              });
              a["01"] = "leading";
              a["1e0"] = "exponential";
              a[Symbol("marker")] = 4;

              const keys = __host_array_extra_keys(a);
              if (keys.join(",") !== "z,getter,4294967295,-0,+1,01,1e0") {
                throw new Error(shape + " wrong keys: " + keys.join(","));
              }
            }
            if (getterCalls !== 0) throw new Error("getter invoked");
            if (__host_array_extra_keys({ z: 1 }).length !== 0) {
              throw new Error("ordinary object accepted");
            }
            if (__host_array_extra_keys(1).length !== 0) {
              throw new Error("primitive accepted");
            }
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
        // The exit propagates structurally as `Completion::Exit` (issue #242) —
        // not a `Throw` — so no `catch`/`finally` can consume it.
        assert!(matches!(c, Completion::Exit(42)));
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
    fn host_exit_from_async_loop_disposer_skips_iterator_close() {
        // Async functions with a suspension point use the transformed for-of
        // path. If iteration disposal exits, IteratorClose must not invoke the
        // iterator's user-defined return() afterward.
        let (interp, _c) = run_node_script(
            r#"
            globalThis.cleanup = "no";
            const iter = {
              done: false,
              [Symbol.iterator]() { return this; },
              next() {
                if (this.done) return { done: true };
                this.done = true;
                return {
                  value: { [Symbol.dispose]() { __host_exit(3); } },
                  done: false,
                };
              },
              return() { globalThis.cleanup = "ran"; return { done: true }; },
            };
            async function f() {
              for (using resource of iter) {
                await null;
                return 1;
              }
            }
            f();
            "#,
        );
        assert_eq!(interp.pending_exit, Some(3));
        assert_eq!(global_string(&interp, "cleanup"), "no");
    }

    #[test]
    fn host_exit_from_generator_loop_disposer_stops_iteration() {
        // A resumed generator disposes the previous `for (using ...)`
        // iteration before requesting the next value. An exit from that
        // disposer must complete the generator without calling next() or
        // IteratorClose afterward.
        let (interp, c) = run_node_script(
            r#"
            globalThis.nextCalls = 0;
            globalThis.cleanup = "no";
            globalThis.after = "no";
            globalThis.iter = {
              done: false,
              [Symbol.iterator]() { return this; },
              next() {
                globalThis.nextCalls++;
                if (this.done) return { done: true };
                this.done = true;
                return {
                  value: { [Symbol.dispose]() { __host_exit(3); } },
                  done: false,
                };
              },
              return() { globalThis.cleanup = "ran"; return { done: true }; },
            };
            function* g() {
              for (using resource of iter) {
                yield resource;
              }
            }
            globalThis.gen = g();
            gen.next();
            gen.next();
            globalThis.after = "yes";
            "#,
        );
        assert!(matches!(c, Completion::Exit(3)));
        assert_eq!(interp.pending_exit, Some(3));
        assert_eq!(global_number(&interp, "nextCalls"), 1.0);
        assert_eq!(global_string(&interp, "cleanup"), "no");
        assert_eq!(global_string(&interp, "after"), "no");

        let generator_id = global_object_id(&interp, "gen");
        let iterator_id = global_object_id(&interp, "iter");
        let generator = interp.get_object(generator_id).unwrap();
        assert!(matches!(
            generator.borrow().iterator_state(),
            Some(IteratorState::StateMachineGenerator {
                execution_state: StateMachineExecutionState::Completed,
                ..
            })
        ));
        assert!(!interp.generator_for_of_stacks.contains_key(&generator_id));
        assert!(!interp.generator_inline_iters.contains_key(&generator_id));
        assert!(!interp.gc_temp_roots.contains(&iterator_id));
    }

    #[test]
    fn host_exit_from_async_generator_loop_disposer_stops_iteration() {
        // The async-generator queue is a completion-less boundary. It must
        // latch an Exit returned by the resumed state machine, leave the
        // request promise unsettled, and stop the current reaction immediately.
        let (interp, _c) = run_node_script(
            r#"
            globalThis.nextCalls = 0;
            globalThis.cleanup = "no";
            globalThis.after = "no";
            globalThis.iter = {
              done: false,
              [Symbol.iterator]() { return this; },
              next() {
                globalThis.nextCalls++;
                if (this.done) return { done: true };
                this.done = true;
                return {
                  value: { [Symbol.dispose]() { __host_exit(4); } },
                  done: false,
                };
              },
              return() { globalThis.cleanup = "ran"; return { done: true }; },
            };
            async function* g() {
              for (using resource of iter) {
                yield resource;
              }
            }
            globalThis.gen = g();
            gen.next().then(function () {
              gen.next();
              globalThis.after = "yes";
            });
            "#,
        );
        assert_eq!(interp.pending_exit, Some(4));
        assert_eq!(global_number(&interp, "nextCalls"), 1.0);
        assert_eq!(global_string(&interp, "cleanup"), "no");
        assert_eq!(global_string(&interp, "after"), "no");

        let generator_id = global_object_id(&interp, "gen");
        let iterator_id = global_object_id(&interp, "iter");
        let generator = interp.get_object(generator_id).unwrap();
        assert!(matches!(
            generator.borrow().iterator_state(),
            Some(IteratorState::StateMachineAsyncGenerator {
                execution_state: StateMachineExecutionState::Completed,
                ..
            })
        ));
        assert!(!interp.generator_for_of_stacks.contains_key(&generator_id));
        assert!(!interp.generator_inline_iters.contains_key(&generator_id));
        assert!(!interp.gc_temp_roots.contains(&iterator_id));
    }

    #[test]
    fn host_exit_from_inner_async_iterator_close_skips_outer_cleanup() {
        // An exit requested by an inner iterator's return() must stop the
        // transformed unwind before it invokes an outer iterator's return().
        let (interp, _c) = run_node_script(
            r#"
            globalThis.innerCleanup = "no";
            globalThis.outerCleanup = "no";
            const outer = {
              done: false,
              [Symbol.iterator]() { return this; },
              next() {
                if (this.done) return { done: true };
                this.done = true;
                return { value: 1, done: false };
              },
              return() { globalThis.outerCleanup = "ran"; return { done: true }; },
            };
            const inner = {
              done: false,
              [Symbol.iterator]() { return this; },
              next() {
                if (this.done) return { done: true };
                this.done = true;
                return { value: 2, done: false };
              },
              return() {
                globalThis.innerCleanup = "ran";
                __host_exit(4);
                return { done: true };
              },
            };
            async function f() {
              for (const outerValue of outer) {
                for (const innerValue of inner) {
                  await null;
                  return outerValue + innerValue;
                }
              }
            }
            f();
            "#,
        );
        assert_eq!(interp.pending_exit, Some(4));
        assert_eq!(global_string(&interp, "innerCleanup"), "ran");
        assert_eq!(global_string(&interp, "outerCleanup"), "no");
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
        let (interp, c) = run_node_script(
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
        // The generator state machine surfaces the exit as `Completion::Exit`
        // rather than routing it into the body's catch state (issue #242).
        assert!(matches!(c, Completion::Exit(7)));
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
        let (interp, c) = run_node_script(
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
        // dispose_resources returns `Completion::Exit` structurally (issue #242).
        assert!(matches!(c, Completion::Exit(4)));
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

    #[test]
    fn host_exit_from_disposer_propagates_abrupt() {
        // A disposer calling __host_exit while the block completes NORMALLY must
        // make dispose_resources return an abrupt completion, so the statement
        // after the block does not run. (PR #237 review round 4, Codex P2.)
        let (interp, _c) = run_node_script(
            r#"
            globalThis.after = "no";
            {
              using r = { [Symbol.dispose]() { __host_exit(4); } };
            }
            globalThis.after = "yes"; // must NOT run
            "#,
        );
        assert_eq!(interp.pending_exit, Some(4));
        assert_eq!(global_string(&interp, "after"), "no");
    }

    #[test]
    fn host_exit_from_async_disposer_propagates_abrupt() {
        // Async disposal awaits via await_value's own drain loop; an exit there
        // must stop draining and propagate abruptly. (PR #237 review round 4.)
        let (interp, _c) = run_node_script(
            r#"
            globalThis.afterAsync = "no";
            async function f() {
              {
                await using r = { async [Symbol.asyncDispose]() { __host_exit(6); } };
              }
              globalThis.afterAsync = "yes"; // must NOT run
            }
            f();
            "#,
        );
        assert_eq!(interp.pending_exit, Some(6));
        assert_eq!(global_string(&interp, "afterAsync"), "no");
    }

    #[test]
    fn host_exit_from_sync_async_body_propagates_abrupt() {
        // An async function that calls __host_exit in its synchronous prologue
        // rejects its promise and returns Normal(promise) to the caller; the
        // exec_statements chokepoint must stop the trailing statement.
        // (PR #237 review round 5, Codex P2.)
        let (interp, _c) = run_node_script(
            r#"
            globalThis.after = "no";
            async function f() { __host_exit(5); }
            f();
            globalThis.after = "yes"; // must NOT run
            "#,
        );
        assert_eq!(interp.pending_exit, Some(5));
        assert_eq!(global_string(&interp, "after"), "no");
    }

    #[test]
    fn host_exit_from_disposable_stack_stops_remaining() {
        // DisposableStack.prototype.dispose has its own LIFO disposal loop.
        // The last-deferred disposer (exit) runs first; the earlier one must
        // not run, and the statement after dispose() must not run.
        // (PR #237 review round 5, Codex P2.)
        let (interp, c) = run_node_script(
            r#"
            globalThis.side = "no";
            globalThis.afterStack = "no";
            const s = new DisposableStack();
            s.defer(() => { globalThis.side = "ran"; });
            s.defer(() => { __host_exit(4); });
            s.dispose();
            globalThis.afterStack = "yes"; // must NOT run
            "#,
        );
        assert_eq!(interp.pending_exit, Some(4));
        assert_eq!(global_string(&interp, "side"), "no");
        assert_eq!(global_string(&interp, "afterStack"), "no");
        // DisposableStack.prototype.dispose returns `Completion::Exit` (issue #242).
        assert!(matches!(c, Completion::Exit(4)));
    }

    #[test]
    fn host_exit_from_chained_reaction_stops_chain() {
        // A `__host_exit` in a Promise reaction handler (issue #242) must
        // propagate out of the reaction job as `Completion::Exit` — not be
        // swallowed into a fulfillment — so the drain loop stops and the
        // chained `.then` never runs. This is load-bearing: if the reaction
        // job's completion match dropped `Exit`, `chained` would become "yes".
        let (interp, _c) = run_node_script(
            r#"
            globalThis.chained = "no";
            Promise.resolve()
              .then(() => { __host_exit(3); })
              .then(() => { globalThis.chained = "yes"; });
            "#,
        );
        assert_eq!(interp.pending_exit, Some(3));
        assert_eq!(global_string(&interp, "chained"), "no");
    }

    #[test]
    fn host_exit_from_reject_reaction_stops_drain() {
        // The reject-reaction path must also propagate `Completion::Exit`
        // (issue #242): a `.catch` handler that calls `__host_exit` stops the
        // drain, and a following queued microtask does not run.
        let (interp, _c) = run_node_script(
            r#"
            globalThis.after = "no";
            Promise.reject(0).catch(() => { __host_exit(1); });
            Promise.resolve().then(() => { globalThis.after = "yes"; });
            "#,
        );
        assert_eq!(interp.pending_exit, Some(1));
        assert_eq!(global_string(&interp, "after"), "no");
    }

    #[test]
    fn host_exit_from_thenable_resolve() {
        // Resolving a promise with a thenable runs a job that calls the user
        // `then`; a `__host_exit` there (issue #242) must propagate out of that
        // job rather than being turned into a rejection.
        let (interp, _c) = run_node_script(
            r#"
            globalThis.after = "no";
            Promise.resolve({ then(_res, _rej) { __host_exit(2); } });
            Promise.resolve().then(() => { globalThis.after = "yes"; });
            "#,
        );
        assert_eq!(interp.pending_exit, Some(2));
        assert_eq!(global_string(&interp, "after"), "no");
    }

    #[test]
    fn host_exit_from_iterator_return_during_throw_is_uncatchable() {
        // When a for-of unwinds a *genuine* throw, the iterator's `return()`
        // runs — and if it calls `__host_exit` (issue #242), the exit must stay
        // uncatchable even though `return()` returns a `JsValue` and so cannot
        // carry a `Completion::Exit`. The sink is latched in `iterator_close`
        // and honored at the `try` boundary, so the surrounding `catch` and
        // `finally` do not run.
        let (interp, c) = run_node_script(
            r#"
            globalThis.caught = "no";
            globalThis.cleanup = "no";
            globalThis.fin = "no";
            const iter = {
              [Symbol.iterator]() { return this; },
              next() { return { value: 1, done: false }; },
              return() { globalThis.cleanup = "ran"; __host_exit(5); return { done: true }; },
            };
            try {
              for (const x of iter) { throw new Error("boom"); }
            } catch (e) { globalThis.caught = "yes"; }
            finally { globalThis.fin = "ran"; }
            "#,
        );
        assert_eq!(interp.pending_exit, Some(5));
        // `return()` ran (genuine throw unwind) and requested the exit...
        assert_eq!(global_string(&interp, "cleanup"), "ran");
        // ...but the exit is uncatchable: neither catch nor finally ran.
        assert_eq!(global_string(&interp, "caught"), "no");
        assert_eq!(global_string(&interp, "fin"), "no");
        assert!(matches!(c, Completion::Exit(5)));
    }

    #[test]
    fn host_exit_from_sync_async_body_stops_expression_position() {
        // The statement-level chokepoint can't stop a sibling expression that
        // evaluates before control returns to the statement loop; the producer
        // guard in call_async_function must return abrupt so the comma RHS does
        // not run. (PR #237 review round 6, Codex P2.)
        let (interp, _c) = run_node_script(
            r#"
            globalThis.after = "no";
            async function f() { __host_exit(5); }
            f(), (globalThis.after = "yes"); // RHS must NOT run
            "#,
        );
        assert_eq!(interp.pending_exit, Some(5));
        assert_eq!(global_string(&interp, "after"), "no");
    }

    #[test]
    fn host_exit_during_dispose_async_await_propagates_abrupt() {
        // A microtask that calls __host_exit while AsyncDisposableStack's
        // disposeAsync is awaiting must make the disposeAsync() call return
        // abruptly, so a sibling in expression position does not run.
        // (PR #237 review round 7, Codex P2.)
        let (interp, _c) = run_node_script(
            r#"
            globalThis.after = "no";
            Promise.resolve().then(() => { __host_exit(23); });
            const s = new AsyncDisposableStack();
            s.use(null);
            s.disposeAsync(), (globalThis.after = "yes"); // RHS must NOT run
            "#,
        );
        assert_eq!(interp.pending_exit, Some(23));
        assert_eq!(global_string(&interp, "after"), "no");
    }

    #[test]
    fn host_exit_from_promise_try_callback_skips_custom_constructor() {
        // Promise.try's catch-all match arm used to run new_promise_capability
        // (which can invoke a custom receiver constructor) before checking
        // whether the callback's completion was a `__host_exit` (issue #242).
        // The exit must propagate immediately, so the constructor never runs
        // and execution after the Promise.try call site never resumes.
        let (interp, c) = run_node_script(
            r#"
            globalThis.log = "";
            class C extends Promise {
              constructor(executor) {
                super(executor);
                globalThis.log += "ctor;";
              }
            }
            Promise.try.call(C, () => { globalThis.log += "cb;"; __host_exit(7); });
            globalThis.log += "after;";
            "#,
        );
        assert_eq!(interp.pending_exit, Some(7));
        assert_eq!(global_string(&interp, "log"), "cb;");
        assert!(matches!(c, Completion::Exit(7)));
    }

    #[test]
    fn host_exit_from_promise_try_custom_reject_propagates() {
        // Promise.try's Throw arm calls `cap.reject`, which a custom receiver
        // constructor can bind to arbitrary user code. A `__host_exit` there
        // (issue #242) must propagate rather than being discarded once the
        // reject call returns.
        let (interp, c) = run_node_script(
            r#"
            globalThis.log = "";
            function C(executor) {
              executor(() => {}, () => { globalThis.log += "reject;"; __host_exit(3); });
            }
            Promise.try.call(C, () => { throw new Error("boom"); });
            globalThis.log += "after;";
            "#,
        );
        assert_eq!(interp.pending_exit, Some(3));
        assert_eq!(global_string(&interp, "log"), "reject;");
        assert!(matches!(c, Completion::Exit(3)));
    }
}

// Spec-derived property-descriptor invariants for the builtin methods installed
// by iterators.rs, promise.rs and disposable.rs. Every built-in method must be
// a `{ writable: true, enumerable: false, configurable: true }` own property
// whose function object carries `name`/`length` own props that are
// `{ writable: false, enumerable: false, configurable: true }` (ECMAScript
// §10.2.9 SetFunctionName / §10.2.10 SetFunctionLength, §20 built-in intro).
// Expected `name`/`length` values are the reference values reported by Node
// (v26), an independent source of truth — not read off jsse's implementation.
// These pin the observable shape so the define_method adoption refactor is
// verifiably behavior-preserving.
mod builtin_method_descriptors {
    use super::run_script;

    // JS helpers shared by every descriptor check. `checkMethod` returns the
    // function object so aliased-symbol checks can assert value identity.
    const PRELUDE: &str = r#"
        function _attrs(d) { return [d.writable, d.enumerable, d.configurable]; }
        function checkMethod(obj, objName, key, expName, expLen) {
            var d = Object.getOwnPropertyDescriptor(obj, key);
            if (!d) throw new Error(objName + "." + String(key) + " missing");
            var a = _attrs(d);
            if (!(a[0] === true && a[1] === false && a[2] === true))
                throw new Error(objName + "." + String(key) + " method attrs " + a);
            var f = d.value;
            if (typeof f !== "function")
                throw new Error(objName + "." + String(key) + " value not a function");
            var nd = Object.getOwnPropertyDescriptor(f, "name");
            var na = _attrs(nd);
            if (!(na[0] === false && na[1] === false && na[2] === true))
                throw new Error(objName + "." + String(key) + " name attrs " + na);
            if (nd.value !== expName)
                throw new Error(objName + "." + String(key) + " name " + nd.value + " != " + expName);
            var ld = Object.getOwnPropertyDescriptor(f, "length");
            var la = _attrs(ld);
            if (!(la[0] === false && la[1] === false && la[2] === true))
                throw new Error(objName + "." + String(key) + " length attrs " + la);
            if (ld.value !== expLen)
                throw new Error(objName + "." + String(key) + " length " + ld.value + " != " + expLen);
            return f;
        }
        // A symbol-keyed method that must be the *same* function object as a
        // named data-key method (e.g. @@dispose === dispose), same attrs.
        function checkAlias(obj, objName, symKey, dataKey) {
            var d = Object.getOwnPropertyDescriptor(obj, symKey);
            if (!d) throw new Error(objName + " @@" + " alias missing");
            var a = _attrs(d);
            if (!(a[0] === true && a[1] === false && a[2] === true))
                throw new Error(objName + " alias attrs " + a);
            if (d.value !== obj[dataKey])
                throw new Error(objName + " alias not same object as " + dataKey);
        }
    "#;

    fn run_checks(checks: &str) {
        run_script(&format!("{PRELUDE}\n{checks}"));
    }

    #[test]
    fn promise_builtin_method_descriptors() {
        run_checks(
            r#"
            var P = Promise.prototype;
            checkMethod(P, "Promise.prototype", "then", "then", 2);
            checkMethod(P, "Promise.prototype", "catch", "catch", 1);
            checkMethod(P, "Promise.prototype", "finally", "finally", 1);
            checkMethod(Promise, "Promise", "resolve", "resolve", 1);
            checkMethod(Promise, "Promise", "reject", "reject", 1);
            checkMethod(Promise, "Promise", "all", "all", 1);
            checkMethod(Promise, "Promise", "allSettled", "allSettled", 1);
            checkMethod(Promise, "Promise", "race", "race", 1);
            checkMethod(Promise, "Promise", "any", "any", 1);
            checkMethod(Promise, "Promise", "withResolvers", "withResolvers", 0);
        "#,
        );
    }

    #[test]
    fn iterator_helper_builtin_method_descriptors() {
        run_checks(
            r#"
            var I = Iterator.prototype;
            checkMethod(I, "Iterator.prototype", "map", "map", 1);
            checkMethod(I, "Iterator.prototype", "filter", "filter", 1);
            checkMethod(I, "Iterator.prototype", "take", "take", 1);
            checkMethod(I, "Iterator.prototype", "drop", "drop", 1);
            checkMethod(I, "Iterator.prototype", "flatMap", "flatMap", 1);
            checkMethod(I, "Iterator.prototype", "toArray", "toArray", 0);
            checkMethod(I, "Iterator.prototype", "forEach", "forEach", 1);
            checkMethod(I, "Iterator.prototype", "some", "some", 1);
            checkMethod(I, "Iterator.prototype", "every", "every", 1);
            checkMethod(I, "Iterator.prototype", "find", "find", 1);
            checkMethod(I, "Iterator.prototype", "reduce", "reduce", 1);
            checkMethod(Iterator, "Iterator", "from", "from", 1);
            checkMethod(Iterator, "Iterator", "concat", "concat", 0);
        "#,
        );
    }

    #[test]
    fn disposable_stack_builtin_method_descriptors() {
        run_checks(
            r#"
            var D = DisposableStack.prototype;
            checkMethod(D, "DisposableStack.prototype", "dispose", "dispose", 0);
            checkMethod(D, "DisposableStack.prototype", "use", "use", 1);
            checkMethod(D, "DisposableStack.prototype", "adopt", "adopt", 2);
            checkMethod(D, "DisposableStack.prototype", "defer", "defer", 1);
            checkMethod(D, "DisposableStack.prototype", "move", "move", 0);
            checkAlias(D, "DisposableStack.prototype", Symbol.dispose, "dispose");

            var A = AsyncDisposableStack.prototype;
            checkMethod(A, "AsyncDisposableStack.prototype", "disposeAsync", "disposeAsync", 0);
            checkMethod(A, "AsyncDisposableStack.prototype", "use", "use", 1);
            checkMethod(A, "AsyncDisposableStack.prototype", "adopt", "adopt", 2);
            checkMethod(A, "AsyncDisposableStack.prototype", "defer", "defer", 1);
            checkMethod(A, "AsyncDisposableStack.prototype", "move", "move", 0);
            checkAlias(A, "AsyncDisposableStack.prototype", Symbol.asyncDispose, "disposeAsync");
        "#,
        );
    }
}

/// Pins the `with_gc_root_scope` seam: whatever the body pushes onto the
/// temp-root stack is bulk-unrooted on *every* exit path — normal return and
/// early return alike — while roots taken before the scope are left untouched.
/// This is the interface the `array.rs` epilogue migration relies on, exercised
/// through the seam rather than by reaching into `gc_temp_roots` directly.
#[test]
fn with_gc_root_scope_truncates_on_every_exit() {
    let mut interp = Interpreter::new();

    // A value rooted before the scope must survive it.
    interp.gc_root_value(&JsValue::object(9_001));
    let baseline = interp.gc_root_frame();

    // Normal completion: temps rooted inside are released; the body value returns.
    let returned = interp.with_gc_root_scope(|i| {
        i.gc_root_value(&JsValue::object(9_002));
        i.gc_root_value(&JsValue::object(9_003));
        assert_eq!(
            i.gc_root_frame(),
            baseline + 2,
            "temps rooted inside the scope"
        );
        42u32
    });
    assert_eq!(returned, 42);
    assert_eq!(
        interp.gc_root_frame(),
        baseline,
        "scope truncated back to baseline on normal return",
    );

    // Early return from the body: the scope must still truncate.
    let outcome: Result<(), ()> = interp.with_gc_root_scope(|i| {
        i.gc_root_value(&JsValue::object(9_004));
        if i.gc_root_frame() == baseline + 1 {
            return Err(());
        }
        Ok(())
    });
    assert_eq!(outcome, Err(()));
    assert_eq!(
        interp.gc_root_frame(),
        baseline,
        "scope truncated back to baseline on early return",
    );

    // The pre-scope root is untouched.
    assert!(interp.gc_temp_roots.contains(&9_001));
}
