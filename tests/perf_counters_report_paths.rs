//! Every instrumented run must write the counter report to stderr, not only
//! the runs that happen to go through `execute_code` (#537 review finding).
//!
//! Before the fix, `--prelude` and REPL invocations bypassed `execute_code`
//! entirely and exited silently, contradicting the guarantee documented in
//! AGENTS.md. These are child-process checks because the condition is which
//! `main.rs` exit path the process takes.
#![cfg(feature = "perf-counters")]

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("jsse-perf-report-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Counts complete counter reports by their first line, so a path emitting the
/// report twice is as much a failure as one emitting it not at all.
fn report_count(stderr: &str) -> usize {
    stderr
        .lines()
        .filter(|l| l.starts_with("PERF\tvm_ops\t"))
        .count()
}

#[test]
fn prelude_path_emits_exactly_one_report() {
    let dir = scratch("prelude");
    fs::write(dir.join("pre.js"), "var a = 1;\n").expect("write prelude");
    fs::write(dir.join("main.js"), "var b = a + 1;\n").expect("write main");
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .current_dir(&dir)
        .args(["--prelude", "pre.js", "main.js"])
        .output()
        .expect("run jsse");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(report_count(&stderr), 1, "stderr was:\n{stderr}");
}

#[test]
fn file_path_emits_exactly_one_report() {
    let dir = scratch("file");
    fs::write(dir.join("main.js"), "var b = 2;\n").expect("write main");
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .current_dir(&dir)
        .args(["main.js"])
        .output()
        .expect("run jsse");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(report_count(&stderr), 1, "stderr was:\n{stderr}");
}

#[test]
fn eval_path_emits_exactly_one_report() {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(["-e", "1 + 1"])
        .output()
        .expect("run jsse");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(report_count(&stderr), 1, "stderr was:\n{stderr}");
}

#[test]
fn repl_path_emits_exactly_one_report() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn jsse");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"1 + 1\n")
        .expect("write to repl");
    drop(child.stdin.take());
    let out = child.wait_with_output().expect("wait for jsse");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(report_count(&stderr), 1, "stderr was:\n{stderr}");
}

/// A prelude that fails still executed JavaScript, so its counters are still
/// worth reporting — but those paths return straight out of `run_main` without
/// reaching `exit_code_from_result`. Caught by the cross-harness reviewer on
/// the second pass of the #537 review; all three emitted nothing beforehand.
#[test]
fn failing_prelude_paths_still_emit_exactly_one_report() {
    let dir = scratch("prelude-failures");
    fs::write(dir.join("ok.js"), "var ok = 1;\n").expect("write ok");
    fs::write(dir.join("throws.js"), "var a = 1;\nnull.x;\n").expect("write throws");
    fs::write(dir.join("syntax.js"), "var =;\n").expect("write syntax");

    // (prelude file, description) — the third does not exist on disk at all.
    for (prelude, what) in [
        ("throws.js", "a prelude that throws"),
        ("syntax.js", "a prelude with a syntax error"),
        ("missing.js", "an unreadable prelude"),
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
            .current_dir(&dir)
            .args(["--prelude", prelude, "ok.js"])
            .output()
            .expect("run jsse");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(
            report_count(&stderr),
            1,
            "{what} must still report exactly once; stderr was:\n{stderr}"
        );
    }
}

/// Generator and `eval` bodies bypass `dispatch_body`, so without frames of
/// their own their work units are credited to the calling function — the
/// `BODY` ranking's whole purpose is defeated (#537 review). Measured before
/// the fix, `caller` reported 16,020 exclusive units; its real total is 13.
#[test]
fn generator_and_eval_work_is_not_credited_to_the_caller() {
    let dir = scratch("attribution");
    fs::write(
        dir.join("main.js"),
        "function* gen(n) { var s = 0; for (var i = 0; i < n; i++) { s += i; } yield s; }\n\
         function caller() { var t = 0; for (var g of gen(2000)) { t += g; } return t; }\n\
         function evalCaller() { var r = 0; for (var i = 0; i < 200; i++) { r += eval(\"i + 1\"); } return r; }\n\
         caller(); evalCaller();\n",
    )
    .expect("write main");
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .current_dir(&dir)
        .args(["main.js"])
        .output()
        .expect("run jsse");
    let stderr = String::from_utf8_lossy(&out.stderr);

    let row = |name: &str| -> Option<u64> {
        stderr
            .lines()
            .find(|l| l.starts_with(&format!("BODY\t{name}\t")))
            .and_then(|l| l.split('\t').nth(2))
            .and_then(|n| n.parse().ok())
    };

    let perf = |key: &str| -> u64 {
        stderr
            .lines()
            .find(|l| l.starts_with(&format!("PERF\t{key}\t")))
            .and_then(|l| l.split('\t').nth(2))
            .and_then(|n| n.parse().ok())
            .unwrap_or_else(|| panic!("no PERF {key} row; stderr:\n{stderr}"))
    };

    let generator =
        row("gen").unwrap_or_else(|| panic!("no named generator BODY row; stderr:\n{stderr}"));
    let eval_body =
        row("<eval>").unwrap_or_else(|| panic!("no <eval> BODY row; stderr:\n{stderr}"));
    let caller = row("caller").unwrap_or_else(|| panic!("no caller BODY row; stderr:\n{stderr}"));
    let eval_caller =
        row("evalCaller").unwrap_or_else(|| panic!("no evalCaller BODY row; stderr:\n{stderr}"));

    assert!(
        generator > 1000,
        "the generator's own loop must be credited to it, got {generator}"
    );
    assert!(eval_body > 0, "eval'd code must be credited to <eval>");
    assert!(
        caller < 100,
        "caller must not absorb the generator's work, got {caller}"
    );
    assert_eq!(
        perf("ast_units_in_functions"),
        caller + eval_caller,
        "generator state-machine steps must stay out of the function-invocation metric"
    );
}

#[test]
fn named_async_function_gets_its_own_body_row() {
    let dir = scratch("async-attribution");
    fs::write(
        dir.join("main.js"),
        "async function asyncWork(n) { var s = 0; for (var i = 0; i < n; i++) { s += i; } return s; }\n\
         async function asyncCaller() { return await asyncWork(2000); }\n\
         asyncCaller();\n",
    )
    .expect("write main");
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .current_dir(&dir)
        .args(["main.js"])
        .output()
        .expect("run jsse");
    let stderr = String::from_utf8_lossy(&out.stderr);

    let row = |name: &str| -> Option<u64> {
        stderr
            .lines()
            .find(|l| l.starts_with(&format!("BODY\t{name}\t")))
            .and_then(|l| l.split('\t').nth(2))
            .and_then(|n| n.parse().ok())
    };

    let async_work = row("asyncWork")
        .unwrap_or_else(|| panic!("no named async function BODY row; stderr:\n{stderr}"));
    let async_caller = row("asyncCaller")
        .unwrap_or_else(|| panic!("no named async caller BODY row; stderr:\n{stderr}"));

    assert!(
        async_work > 1000,
        "the async function's loop must be credited to it, got {async_work}"
    );
    assert!(
        async_caller < 100,
        "the async caller must not absorb the callee's work, got {async_caller}"
    );
}

/// `body_ast` is the published denominator of the compiled/AST *invocation*
/// split — the metric #526's report is built on. `exec_body` fires once per
/// generator state-machine step (~4x per yield), so counting those there
/// understated the compiled share 5x on generator-heavy code: a workload with
/// 2,000 compiled calls and one generator reported 20.0% compiled instead of
/// ~99.9%. Those executions belong in `body_non_function_execs` instead.
/// Caught by the local evaluator on the second pass of the #537 review.
#[test]
fn generator_replay_does_not_inflate_the_ast_invocation_count() {
    let dir = scratch("invocation-split");
    fs::write(
        dir.join("main.js"),
        "function hot(n) { var s = 0; for (var i = 0; i < n; i++) { s += i; } return s; }\n\
         function* g(n) { for (var i = 0; i < n; i++) { yield i; } }\n\
         var t = 0;\n\
         for (var k = 0; k < 2000; k++) { t += hot(3); }\n\
         for (var v of g(2000)) { t += v; }\n",
    )
    .expect("write main");
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .current_dir(&dir)
        .args(["--bytecode", "main.js"])
        .output()
        .expect("run jsse");
    let stderr = String::from_utf8_lossy(&out.stderr);

    let perf = |key: &str| -> u64 {
        stderr
            .lines()
            .find(|l| l.starts_with(&format!("PERF\t{key}\t")))
            .and_then(|l| l.split('\t').nth(2))
            .and_then(|n| n.parse().ok())
            .unwrap_or_else(|| panic!("no PERF {key} row; stderr:\n{stderr}"))
    };

    let compiled = perf("body_dispatch_compiled");
    let ast = perf("body_dispatch_ast");
    let non_function = perf("body_non_function_execs");

    assert!(compiled > 1000, "hot() must compile, got {compiled}");
    // The generator's thousands of state-machine steps must NOT land here.
    assert!(
        ast < 100,
        "generator replay leaked into the AST invocation count: \
         body_dispatch_ast={ast} against body_dispatch_compiled={compiled}"
    );
    assert!(
        non_function > 1000,
        "state-machine steps must be counted separately, got {non_function}"
    );
}

/// A compiled top-level script recorded neither its compile nor its execution,
/// while the fallback recorded both — an eligible script reported VM ops with
/// `compile_ok` and every dispatch counter at zero (#537 review, third pass).
/// Both outcomes are now recorded, and neither goes to `body_compiled`, which
/// counts function invocations only.
#[test]
fn script_body_records_its_compile_outcome_but_not_a_function_invocation() {
    let dir = scratch("script-compile");
    fs::write(
        dir.join("ok.js"),
        "var s = 0; for (var i = 0; i < 5; i++) { s += i; } s;\n",
    )
    .expect("write eligible");
    fs::write(
        dir.join("bail.js"),
        "function f(){ try { return 1; } catch(e) { return 0; } } f();\n",
    )
    .expect("write ineligible");

    let perf = |file: &str, key: &str| -> u64 {
        let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
            .current_dir(&dir)
            .args(["--bytecode", file])
            .output()
            .expect("run jsse");
        let stderr = String::from_utf8_lossy(&out.stderr);
        stderr
            .lines()
            .find(|l| l.starts_with(&format!("PERF\t{key}\t")))
            .and_then(|l| l.split('\t').nth(2))
            .and_then(|n| n.parse().ok())
            .unwrap_or_else(|| panic!("no PERF {key}; stderr:\n{stderr}"))
    };

    assert!(
        perf("ok.js", "vm_ops") > 0,
        "eligible script must run compiled"
    );
    assert_eq!(
        perf("ok.js", "compile_ok"),
        1,
        "its compile must be recorded"
    );
    assert_eq!(perf("ok.js", "body_non_function_execs"), 1);
    // A script is not a function invocation — these must stay untouched.
    assert_eq!(perf("ok.js", "body_dispatch_compiled"), 0);
    assert_eq!(perf("ok.js", "body_dispatch_ast"), 0);
    // An ineligible script must surface its bail instead of reporting nothing.
    assert!(
        perf("bail.js", "compile_bail") > 0,
        "script bail must be recorded"
    );
}

/// Module items run through `exec_statement` without `dispatch_body`, so their
/// work landed in `ast_work_units` under no `BODY` row at all (#537 review,
/// third pass).
#[test]
fn module_body_gets_its_own_attribution_row() {
    let dir = scratch("module-attribution");
    fs::write(
        dir.join("m.mjs"),
        "var t = 0;\nfor (var i = 0; i < 3; i++) { t += i; }\nexport const v = t;\n",
    )
    .expect("write module");
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .current_dir(&dir)
        .args(["m.mjs"])
        .output()
        .expect("run jsse");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr
            .lines()
            .any(|l| l.starts_with("BODY\t<module body>\t")),
        "module work must be attributed, not orphaned; stderr:\n{stderr}"
    );
}

/// Two distinct functions sharing a name merged into one `BODY` row with
/// combined work, so the ranking could identify neither (#537 review, third
/// pass). The id disambiguates them; unique names must stay bare.
#[test]
fn same_named_functions_get_separate_body_rows() {
    let dir = scratch("same-name");
    fs::write(
        dir.join("main.js"),
        "function outer1() { function same() { var s = 0; for (var i = 0; i < 900; i++) { s += i; } return s; } return same(); }\n\
         function outer2() { function same() { var s = 0; for (var i = 0; i < 30; i++) { s += i; } return s; } return same(); }\n\
         outer1(); outer2();\n",
    )
    .expect("write main");
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .current_dir(&dir)
        .args(["main.js"])
        .output()
        .expect("run jsse");
    let stderr = String::from_utf8_lossy(&out.stderr);

    let same_rows: Vec<&str> = stderr
        .lines()
        .filter(|l| l.starts_with("BODY\tsame#"))
        .collect();
    assert_eq!(
        same_rows.len(),
        2,
        "both `same` functions must appear separately; stderr:\n{stderr}"
    );
    assert!(
        !stderr.lines().any(|l| l.starts_with("BODY\tsame\t")),
        "the merged bare-name row must be gone; stderr:\n{stderr}"
    );
    // outer1/outer2 are unique, so they must NOT be suffixed.
    assert!(
        stderr.lines().any(|l| l.starts_with("BODY\touter1\t")),
        "unique names must stay bare; stderr:\n{stderr}"
    );
}
