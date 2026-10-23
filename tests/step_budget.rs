//! Opt-in deterministic step budget (#1173): `--step-budget N` ends a run
//! uncatchably with exit status 124 once N counted safepoints are spent, in
//! both execution modes. Host resource limit, not ECMAScript-observable, so it
//! lives here rather than in test262-extra.
use std::process::{Command, Output};

const EXHAUSTED: i32 = 124;

fn run(program: &str, bytecode: bool, budget: Option<u64>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
    if !bytecode {
        cmd.arg("--no-bytecode");
    }
    if let Some(n) = budget {
        cmd.arg("--step-budget").arg(n.to_string());
    }
    cmd.args(["-e", program]);
    cmd.env_remove("JSSE_GC_STRESS");
    cmd.output().expect("failed to spawn jsse")
}

fn assert_exhausts(program: &str) {
    for bytecode in [true, false] {
        let out = run(program, bytecode, Some(2000));
        assert_eq!(
            out.status.code(),
            Some(EXHAUSTED),
            "bytecode={bytecode}: {program}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("step budget exhausted"),
            "bytecode={bytecode}: {program}"
        );
    }
}

#[test]
fn infinite_loops_exhaust_in_both_modes() {
    for program in [
        "while (1) {}",
        "do {} while (1)",
        "for (;;) {}",
        "L: for (;;) { continue L; }",
        "for (var k in { a: 1 }) { for (;;); }",
        "function* g() { for (;;) yield 1; } for (var x of g()) {}",
        "function spin() { for (;;); } spin()",
        "while (1) try { continue } finally {}",
        "L: for (;;) try { continue L } finally {}",
        "eval('for(;;);')",
        "new Function('for(;;);')()",
    ] {
        assert_exhausts(program);
    }
}

#[test]
fn exhaustion_is_uncatchable() {
    for program in [
        "try { for (;;) {} } catch (e) { print('caught') } finally { print('fin') }",
        "for (;;) { try { for (;;); } catch (e) {} }",
        "for (;;) { try { for (;;); } finally { print('fin') } }",
    ] {
        assert_exhausts(program);
        for bytecode in [true, false] {
            let out = run(program, bytecode, Some(2000));
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(
                !stdout.contains("caught") && !stdout.contains("fin"),
                "bytecode={bytecode}: {program}\n{stdout}"
            );
        }
    }
}

#[test]
fn exhaustion_crosses_native_and_job_boundaries() {
    for program in [
        "[1].forEach(function () { for (;;); })",
        "[3, 2, 1].sort(function () { for (;;); })",
        "({ get x() { for (;;); } }).x",
        "new Proxy({}, { get() { for (;;); } }).x",
        "String({ toString() { for (;;); } })",
        "new Promise(function () { for (;;); })",
        "Promise.resolve().then(function () { for (;;); })",
        "function f() { Promise.resolve().then(f) } f()",
        "const f = () => Promise.resolve().then(f); f()",
        "function t() { setTimeout(t, 0) } t()",
        "setInterval(() => 0, 0)",
        "async function f() { await 0; for (;;); } f()",
        "var it = { [Symbol.iterator]() { return { next() { return { done: false } }, return() { for (;;); } } } };\
         for (var x of it) { break; }",
    ] {
        assert_exhausts(program);
    }
}

#[test]
fn unreached_budget_changes_nothing() {
    let program = "var s = 0; for (var i = 0; i < 100; i++) { s += i; } console.log(s);";
    for bytecode in [true, false] {
        let plain = run(program, bytecode, None);
        let budgeted = run(program, bytecode, Some(1_000_000));
        assert_eq!(budgeted.status.code(), Some(0));
        assert_eq!(plain.stdout, budgeted.stdout);
        assert_eq!(String::from_utf8_lossy(&budgeted.stdout).trim(), "4950");
    }
}

#[test]
fn zero_budget_exhausts_immediately() {
    for bytecode in [true, false] {
        let out = run("console.log('ran')", bytecode, Some(0));
        assert_eq!(out.status.code(), Some(EXHAUSTED));
        assert!(out.stdout.is_empty());
    }
}

#[test]
fn exhaustion_is_deterministic() {
    let program = "var i = 0; for (;;) { console.log(i++); }";
    for bytecode in [true, false] {
        let a = run(program, bytecode, Some(500));
        let b = run(program, bytecode, Some(500));
        assert_eq!(a.status.code(), Some(EXHAUSTED));
        assert!(!a.stdout.is_empty());
        assert_eq!(a.stdout, b.stdout);
        assert_eq!(a.stderr, b.stderr);
    }
}

#[test]
fn exhaustion_holds_under_gc_stress() {
    for bytecode in [true, false] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
        if !bytecode {
            cmd.arg("--no-bytecode");
        }
        let out = cmd
            .args(["--step-budget", "500", "-e"])
            .arg("var o = []; for (;;) { o.push({ a: o.length }); }")
            .env("JSSE_GC_STRESS", "1")
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(EXHAUSTED), "bytecode={bytecode}");
    }
}
