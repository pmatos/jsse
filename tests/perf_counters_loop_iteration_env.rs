//! A `for`/`for-in`/`for-of` loop whose head, test, update and body cannot
//! capture or `eval` its scope runs every iteration in one environment, so it
//! must not intern or copy binding names per iteration (#919). A loop that can
//! capture keeps its fresh per-iteration environment, so the same counters
//! still scale with it.
//!
//! The atom interner is the only place script-controlled names are hashed, so
//! `atom_intern_calls` growing with the iteration count is the observable cost
//! of per-iteration environment construction. These are child-process checks
//! because the counters are per-process.
#![cfg(feature = "perf-counters")]

use std::process::Command;

fn intern_calls(source: &str) -> u64 {
    intern_calls_with(&[], source)
}

fn intern_calls_with(flags: &[&str], source: &str) -> u64 {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(flags)
        .args(["-e", source])
        .output()
        .expect("run jsse");
    assert!(
        out.status.success(),
        "jsse failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stderr)
        .lines()
        .find_map(|line| {
            let mut cols = line.split('\t');
            match (cols.next(), cols.next(), cols.next()) {
                (Some("PERF"), Some("atom_intern_calls"), Some(v)) => v.parse().ok(),
                _ => None,
            }
        })
        .expect("atom_intern_calls row")
}

fn for_let(iterations: u32, body: &str) -> String {
    format!(
        "function f() {{ var s = 0; for (let i = 0; i < {iterations}; i++) {{ {body} }} return s; }} f();"
    )
}

fn for_of(iterations: u32, body: &str) -> String {
    format!(
        "var xs = []; for (var n = 0; n < {iterations}; n++) xs.push(n);\n\
         function f() {{ var s = 0; for (const v of xs) {{ {body} }} return s; }} f();"
    )
}

fn for_in(iterations: u32, body: &str) -> String {
    format!(
        "var o = {{}}; for (var n = 0; n < {iterations}; n++) o['k' + n] = n;\n\
         function f() {{ var s = 0; for (const k in o) {{ {body} }} return s; }} f();"
    )
}

#[test]
fn non_capturing_loops_intern_nothing_per_iteration() {
    for build in [for_let, for_of, for_in] {
        let small = intern_calls(&build(1_000, "s = (s + 1) | 0;"));
        let large = intern_calls(&build(3_000, "s = (s + 1) | 0;"));
        assert!(
            large <= small,
            "atom_intern_calls grew with the iteration count: {small} -> {large}"
        );
    }
}

#[test]
fn capturing_tree_walked_loops_keep_a_fresh_environment_per_iteration() {
    for build in [for_of, for_in] {
        let body = "s = (s + 1) | 0; if (s < 0) (() => s);";
        let small = intern_calls_with(&["--no-bytecode"], &build(1_000, body));
        let large = intern_calls_with(&["--no-bytecode"], &build(3_000, body));
        assert!(
            large >= small + 1_000,
            "a capturing loop must still build per-iteration environments: {small} -> {large}"
        );
    }
}

/// The VM's for-in/for-of build the fresh per-iteration environment from the
/// pre-interned names of a block scope (`PushEnv`), so a capturing loop
/// hashes no name per iteration either (#960).
#[test]
fn capturing_vm_iteration_loops_intern_nothing_per_iteration() {
    for build in [for_of, for_in] {
        let body = "s = (s + 1) | 0; if (s < 0) (() => s);";
        let small = intern_calls(&build(1_000, body));
        let large = intern_calls(&build(3_000, body));
        assert!(
            large <= small,
            "atom_intern_calls grew with the iteration count: {small} -> {large}"
        );
    }
}

/// The VM's `for (let ...)` copies its head environment per iteration through
/// atoms (`CopyIterEnv`): the fresh environment is still built, but no name
/// is hashed for it. That the copies are distinct is a semantics check, in
/// `test262-extra/bytecode-block-environments.js`.
#[test]
fn capturing_for_let_loops_copy_their_environment_without_interning() {
    let small = intern_calls(&for_let(1_000, "s = (s + 1) | 0; if (s < 0) (() => s);"));
    let large = intern_calls(&for_let(3_000, "s = (s + 1) | 0; if (s < 0) (() => s);"));
    assert!(
        large <= small,
        "atom_intern_calls grew with the iteration count: {small} -> {large}"
    );
}
