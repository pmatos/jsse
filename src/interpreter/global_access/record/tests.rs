//! A Proxy global object (jsse#1346): declarations, implicit globals, reads
//! and `delete` reach its traps (sec-global-environment-records,
//! sec-object-environment-records).

use super::super::super::*;
use crate::parser::Parser;

fn eval_str(interp: &mut Interpreter, source: &str) -> String {
    let mut parser = Parser::new(source).expect("parser init");
    let program = parser.parse_program().expect("parse program");
    match interp.run(&program) {
        Completion::Normal(v) => interp.to_string_lossy(&v).expect("string"),
        other => panic!("unexpected completion: {other:?}"),
    }
}

/// A second realm whose global object is a Proxy over its real global object,
/// logging every trap into the main realm's `log` array.
fn proxy_realm(interp: &mut Interpreter) -> usize {
    eval_str(
        interp,
        r#"
        var log = [];
        var realm = $262.createRealm();
        var handler = {};
        ["getOwnPropertyDescriptor", "defineProperty", "has", "get", "set",
         "deleteProperty", "isExtensible"].forEach(function (trap) {
          handler[trap] = function (target, key) {
            if (typeof key === "string") log.push(trap + ":" + key);
            return Reflect[trap].apply(null, arguments);
          };
        });
        var proxyGlobal = new Proxy(realm.global, handler);
        "ok";
        "#,
    );
    let realm_id = interp.realms.len() - 1;
    let proxy_id = {
        let global = interp.realms[0].global_object.expect("main global");
        interp
            .get_property_on_id(global, "proxyGlobal")
            .as_object_id()
            .expect("proxy")
    };
    interp.realms[realm_id].global_object = Some(proxy_id);
    interp.realms[realm_id]
        .global_env
        .borrow_mut()
        .global_object_id = Some(proxy_id);
    realm_id
}

fn run_in(interp: &mut Interpreter, realm_id: usize, source: &str) {
    let mut parser = Parser::new(source).expect("parser init");
    let program = parser.parse_program().expect("parse program");
    let result = interp.with_realm(Some(realm_id), |interp| interp.run(&program));
    assert!(
        matches!(result, Completion::Normal(_) | Completion::Empty),
        "unexpected completion: {result:?}"
    );
}

fn log_of(interp: &mut Interpreter) -> Vec<String> {
    let joined = eval_str(interp, "log.join('\\n')");
    joined.lines().map(str::to_string).collect()
}

fn position(log: &[String], entry: &str) -> usize {
    log.iter()
        .position(|e| e == entry)
        .unwrap_or_else(|| panic!("{entry} not in {log:?}"))
}

fn check_declarations_reach_the_proxy(bytecode: bool) {
    let mut interp = Interpreter::new();
    interp.bytecode_enabled = bytecode;
    let realm = proxy_realm(&mut interp);
    run_in(&mut interp, realm, "var q = 1; function f() {} b = 5;");
    let log = log_of(&mut interp);

    for entry in [
        "getOwnPropertyDescriptor:f",
        "defineProperty:f",
        "set:f",
        "getOwnPropertyDescriptor:q",
        "defineProperty:q",
        "set:q",
        "has:b",
        "set:b",
    ] {
        position(&log, entry);
    }
    assert!(
        position(&log, "defineProperty:f") < position(&log, "defineProperty:q"),
        "functions are bound before vars: {log:?}"
    );

    let seen = eval_str(
        &mut interp,
        "[realm.global.q, realm.global.b, typeof realm.global.f].join()",
    );
    assert_eq!(seen, "1,5,function");
    let q = eval_str(
        &mut interp,
        "var d = Object.getOwnPropertyDescriptor(realm.global, 'q'); \
         [d.writable, d.enumerable, d.configurable].join()",
    );
    assert_eq!(q, "true,true,false");
}

#[test]
fn proxy_global_sees_declarations_and_implicit_globals() {
    check_declarations_reach_the_proxy(true);
}

#[test]
fn proxy_global_sees_declarations_and_implicit_globals_tree_walker() {
    check_declarations_reach_the_proxy(false);
}

fn check_reads_writes_and_delete(bytecode: bool) {
    let mut interp = Interpreter::new();
    interp.bytecode_enabled = bytecode;
    let realm = proxy_realm(&mut interp);
    run_in(
        &mut interp,
        realm,
        "var q = 1; b = 5; \
         Object.defineProperty(globalThis, 'acc', { \
           get: function () { return 'getter'; }, \
           set: function (v) { $262.seen = v; }, configurable: true });",
    );
    eval_str(&mut interp, "log.length = 0; 0");

    run_in(
        &mut interp,
        realm,
        "$262.out = [q, b, typeof nope, typeof q].join(); acc = 'written'; \
         $262.read = acc; $262.del = [delete b, delete q, delete nope].join(); \
         $262.hasB = 'b' in globalThis;",
    );
    let log = log_of(&mut interp);
    for entry in [
        "has:q",
        "get:q",
        "has:nope",
        "set:acc",
        "get:acc",
        "getOwnPropertyDescriptor:b",
        "deleteProperty:b",
        "getOwnPropertyDescriptor:nope",
    ] {
        position(&log, entry);
    }
    assert!(
        !log.iter().any(|e| e == "get:nope"),
        "typeof of a missing name: {log:?}"
    );

    let out = eval_str(
        &mut interp,
        "var g = realm.global; [g.$262.out, g.$262.seen, g.$262.read, g.$262.del, g.$262.hasB].join('|')",
    );
    assert_eq!(
        out,
        "1,5,undefined,number|written|getter|true,false,true|false"
    );
}

#[test]
fn proxy_global_reads_accessors_and_deletes() {
    check_reads_writes_and_delete(true);
}

#[test]
fn proxy_global_reads_accessors_and_deletes_tree_walker() {
    check_reads_writes_and_delete(false);
}

#[test]
fn proxy_global_sees_eval_declarations() {
    let mut interp = Interpreter::new();
    let realm = proxy_realm(&mut interp);
    run_in(
        &mut interp,
        realm,
        "(0, eval)('var ev = 1; function ef() {}');",
    );
    let log = log_of(&mut interp);
    for entry in ["defineProperty:ev", "defineProperty:ef", "set:ef"] {
        position(&log, entry);
    }
    let seen = eval_str(
        &mut interp,
        "var d = Object.getOwnPropertyDescriptor(realm.global, 'ev'); \
         [d.configurable, realm.global.ev, typeof realm.global.ef].join()",
    );
    assert_eq!(seen, "true,1,function");
}

#[test]
fn proxy_global_declaration_checks_use_the_traps() {
    let mut interp = Interpreter::new();
    let realm = proxy_realm(&mut interp);
    eval_str(
        &mut interp,
        "Object.defineProperty(realm.global, 'locked', { value: 1, configurable: false }); 0",
    );
    let mut parser = Parser::new("function locked() {}").expect("parser init");
    let program = parser.parse_program().expect("parse program");
    let result = interp.with_realm(Some(realm), |interp| interp.run(&program));
    assert!(matches!(result, Completion::Throw(_)), "{result:?}");
    let log = log_of(&mut interp);
    position(&log, "getOwnPropertyDescriptor:locked");
}

#[test]
fn global_environment_keeps_no_var_bindings() {
    let mut interp = Interpreter::new();
    eval_str(
        &mut interp,
        "var v = 1; function f() {} implicit = 2; (0, eval)('var ev = 3; function ef() {}'); 0",
    );
    let env = interp.realm().global_env.clone();
    let env = env.borrow();
    let stale: Vec<String> = (0..)
        .map_while(|i| env.bindings.entry_at(i))
        .filter(|(_, b)| b.kind == BindingKind::Var)
        .map(|(name, _)| name.as_str().to_string())
        .collect();
    assert!(
        stale.is_empty(),
        "Var bindings in the global env: {stale:?}"
    );
}
