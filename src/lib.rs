#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

pub mod ast;
mod cli;
mod cli_node;
pub(crate) mod emoji_strings;
mod fuzz_reentrancy;
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod thp;
pub mod types;
pub(crate) mod unicode_tables;

/// Runs the `jsse` CLI (argument parsing, file/eval/REPL/prelude dispatch) on
/// the engine-sized stack (see [`run_on_engine_stack`]). The sole entry point
/// `src/main.rs` calls — everything else the CLI needs stays `pub(crate)`
/// rather than swelling the library's public API, since only this function
/// and the fuzz-target helpers below need to cross the bin/lib boundary.
pub fn run_cli() -> std::process::ExitCode {
    run_on_engine_stack(cli::run_main)
}

/// Stack reserved for parsing/interpreting: deep (but bounded, see
/// `MAX_PARSE_DEPTH`/`CALL_DEPTH_HARD_LIMIT`) recursion needs room to reach
/// those catchable limits and throw before the native stack overflows into a
/// SIGABRT. Sized to fit inside the 512 MiB RLIMIT_AS the test harness (and
/// fuzz targets, see `fuzz/`) impose.
pub const ENGINE_STACK_SIZE: usize = 128 * 1024 * 1024;

/// Runs `f` on a thread with [`ENGINE_STACK_SIZE`] of stack, falling back to
/// the current thread if that large a stack can't be reserved (e.g. a tight
/// `RLIMIT_AS`). Any panic in `f` is propagated to the caller via
/// `resume_unwind` rather than swallowed at the join, so callers that rely on
/// process-level panic handling (e.g. libFuzzer's panic hook) still see it.
///
/// `F: Copy` (rather than plain `FnOnce`) so that if the OS thread spawn
/// itself fails, the same `f` can still be run as the fallback: a `Copy`
/// closure capturing only `Copy` data (e.g. a `&str` a fuzz target derived
/// from its input bytes) remains usable after being handed to `spawn_scoped`,
/// with no `'static` bound needed.
pub fn run_on_engine_stack<F, R>(f: F) -> R
where
    F: Fn() -> R + Send + Copy,
    R: Send,
{
    let spawned = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("jsse-engine".to_string())
            .stack_size(ENGINE_STACK_SIZE)
            .spawn_scoped(scope, f)
            .map(|handle| handle.join())
    });
    match spawned {
        Ok(Ok(result)) => result,
        Ok(Err(payload)) => std::panic::resume_unwind(payload),
        Err(_) => f(),
    }
}

/// Parses `data` and discards the result, panicking only if the parser
/// itself panics/aborts. Backs the `parse` fuzz target
/// (`fuzz/fuzz_targets/parse.rs`): a parse *error* is a normal
/// outcome for arbitrary bytes, not a finding, so the `Result` is discarded
/// entirely inside `run_on_engine_stack`'s closure — `ast::Program` isn't
/// `Send` (it's `Rc`-based), so it could not cross the thread boundary
/// anyway.
pub fn fuzz_parse_bytes(data: &[u8]) {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    run_on_engine_stack(move || {
        let _ = parse_script(src);
    });
}

/// Parses `src` on the current thread (already on an engine-sized stack) and
/// reports whether it parsed cleanly. Backs `benches/engine.rs`.
#[doc(hidden)]
pub fn bench_parse(src: &str) -> bool {
    parse_script(src).is_some()
}

fn parse_script(src: &str) -> Option<ast::Program> {
    parser::Parser::new(src)
        .and_then(|mut p| p.parse_program())
        .ok()
}

/// Runs `src` in a fresh interpreter, returning `false` on a parse error or a
/// synchronous uncaught exception. Backs `benches/engine.rs`.
#[doc(hidden)]
pub fn bench_eval(src: &str) -> bool {
    bench_eval_with_check(src, None)
}

/// Runs `src`, drains its jobs, then runs the validation script `check` in the
/// same interpreter. An assertion inside a promise callback only rejects its
/// promise, so async workloads store a result and `check` must reject a wrong
/// or missing one.
#[doc(hidden)]
pub fn bench_eval_with_check(src: &str, check: Option<&str>) -> bool {
    let completed = |interp: &mut interpreter::Interpreter, program: &ast::Program| {
        matches!(
            interp.run(program),
            interpreter::Completion::Normal(_) | interpreter::Completion::Empty
        )
    };
    let Some(program) = parse_script(src) else {
        return false;
    };
    let mut interp = interpreter::Interpreter::new();
    if !completed(&mut interp, &program) {
        return false;
    }
    match check {
        None => true,
        Some(check) => parse_script(check).is_some_and(|v| completed(&mut interp, &v)),
    }
}

/// How a script run under [`run_script_with_budget`] ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptOutcome {
    Completed,
    Threw,
    ParseError,
    /// The step budget ran out. This is fatal and uncatchable: no `catch` or
    /// `finally` block, iterator `return()` or promise reaction observes it.
    StepBudgetExhausted,
}

/// Runs `src` in a fresh interpreter under an optional deterministic step
/// budget (see `Interpreter::set_step_budget`), on the tree-walker or the
/// bytecode VM. Returns the outcome and the number of steps counted. Call it on
/// an engine-sized stack (`run_on_engine_stack`). Backs in-process fuzz targets.
pub fn run_script_with_budget(
    src: &str,
    budget: Option<u64>,
    bytecode: bool,
) -> (ScriptOutcome, u64) {
    let Some(program) = parse_script(src) else {
        return (ScriptOutcome::ParseError, 0);
    };
    let mut interp = interpreter::Interpreter::new();
    interp.bytecode_enabled = bytecode;
    interp.set_step_budget(budget);
    let completion = interp.run(&program);
    let outcome = if interp.step_budget_exhausted() {
        ScriptOutcome::StepBudgetExhausted
    } else if !completion.is_abrupt() {
        ScriptOutcome::Completed
    } else {
        ScriptOutcome::Threw
    };
    (outcome, interp.steps_used())
}

/// Step budget for each run of the `execute` fuzz target.
const FUZZ_EXECUTE_STEP_BUDGET: u64 = 20_000;

/// Fixed `Math.random` seed shared by both runs of the `execute` fuzz target.
const FUZZ_EXECUTE_RANDOM_SEED: u64 = 0x5EED_F0F0_1174_CAFE;

/// Identifiers whose presence means the input must not run at all:
/// `$262.agent.start` spawns an unbudgeted thread, `sleep` is a real
/// wall-clock sleep and `waitAsync` parks the event loop on a condvar.
const FUZZ_EXECUTE_SKIP_IDENTS: &[&str] = &["agent", "waitAsync", "sleep"];

/// Identifiers whose presence means the observable outcome legitimately varies
/// between two runs (clock, GC timing, shared memory), so the signatures are
/// not compared. Both modes still run for panic and sanitizer coverage.
const FUZZ_EXECUTE_NONDETERMINISTIC_IDENTS: &[&str] = &[
    "Date",
    "DateTimeFormat",
    "Temporal",
    "WeakRef",
    "FinalizationRegistry",
    "$262",
    "Atomics",
    "SharedArrayBuffer",
    "performance",
    "gc",
];

/// What a script run observably did, ignoring step counts and the wording of
/// error messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SigOutcome {
    /// Normal completion, with the rendered completion value.
    Completed(String),
    /// Uncaught exception: the error's `name` for objects, else the rendered value.
    Threw(String),
    ParseError,
    BudgetExhausted,
}

/// The observable result of one run: how it ended plus the captured
/// `print` / `console.*` output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSignature {
    pub outcome: SigOutcome,
    pub output: String,
}

/// Runs `src` in a fresh, deterministic interpreter (fixed `Math.random` seed,
/// virtual timers, captured host output) under `budget` steps, on the bytecode
/// VM or the tree-walker, and returns its [`RunSignature`]. Call it on an
/// engine-sized stack (`run_on_engine_stack`).
pub fn run_script_observed(src: &str, budget: u64, bytecode: bool) -> RunSignature {
    observe(src, budget, bytecode, None).0
}

/// [`run_script_observed`] with the GC stress period forced to `period` (0 turns
/// stress off, whatever `JSSE_GC_STRESS` says), plus the number of safepoints
/// that counted towards stress. Backs the `gc_stress` fuzz target.
pub fn run_script_gc_stressed(
    src: &str,
    budget: u64,
    bytecode: bool,
    period: u32,
) -> (RunSignature, u64) {
    observe(src, budget, bytecode, Some(period))
}

fn observe(
    src: &str,
    budget: u64,
    bytecode: bool,
    stress_period: Option<u32>,
) -> (RunSignature, u64) {
    let Some(program) = parse_script(src) else {
        let signature = RunSignature {
            outcome: SigOutcome::ParseError,
            output: String::new(),
        };
        return (signature, 0);
    };
    let mut interp = interpreter::Interpreter::new();
    interp.bytecode_enabled = bytecode;
    interp.set_step_budget(Some(budget));
    if let Some(period) = stress_period {
        interp.set_gc_stress_period(period);
    }
    interp.use_virtual_timers();
    interp.set_math_random_seed(FUZZ_EXECUTE_RANDOM_SEED);
    interp.capture_host_output();
    let completion = interp.run(&program);
    let outcome = if interp.step_budget_exhausted() {
        SigOutcome::BudgetExhausted
    } else {
        match completion {
            interpreter::Completion::Throw(val) => SigOutcome::Threw(interp.thrown_class(&val)),
            interpreter::Completion::Normal(val) => SigOutcome::Completed(format!("{val}")),
            _ => SigOutcome::Completed(String::new()),
        }
    };
    let signature = RunSignature {
        outcome,
        output: interp.take_captured_host_output(),
    };
    (signature, interp.gc_stress_safepoints())
}

/// Whether `src` is safe to run in-process: it names none of the identifiers
/// that would hang or escape the step budget (`$262.agent`, `sleep`,
/// `waitAsync`).
pub fn fuzz_input_is_runnable(src: &str) -> bool {
    !FUZZ_EXECUTE_SKIP_IDENTS
        .iter()
        .any(|ident| contains_ident(src, ident))
}

/// Whether `src` names an identifier through which GC timing, the clock or
/// shared memory can leak into the observable outcome, so two runs of it need
/// not agree.
pub fn fuzz_input_is_gc_observable(src: &str) -> bool {
    FUZZ_EXECUTE_NONDETERMINISTIC_IDENTS
        .iter()
        .any(|ident| contains_ident(src, ident))
}

fn contains_ident(src: &str, ident: &str) -> bool {
    let is_ident_char = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    src.match_indices(ident).any(|(at, _)| {
        let before = src[..at].chars().next_back();
        let after = src[at + ident.len()..].chars().next();
        !before.is_some_and(is_ident_char) && !after.is_some_and(is_ident_char)
    })
}

/// Describes how two runs of the same source differ, or `None` when they agree
/// or the pair is not comparable (either run exhausted its budget: the modes
/// count steps differently, so they stop at different points).
pub fn execution_divergence(bytecode: &RunSignature, tree_walker: &RunSignature) -> Option<String> {
    if bytecode.outcome == SigOutcome::BudgetExhausted
        || tree_walker.outcome == SigOutcome::BudgetExhausted
        || bytecode == tree_walker
    {
        return None;
    }
    Some(format!(
        "bytecode:    {bytecode:?}\ntree-walker: {tree_walker:?}"
    ))
}

/// Runs `src` on the bytecode VM and then the tree-walker and returns both
/// signatures, or `None` when the input is skipped (it contains an identifier
/// that would hang or escape the budget) or its outcome is not comparable.
/// Skipped-for-nondeterminism inputs still run in both modes.
pub fn fuzz_execute_signatures(src: &str) -> Option<(RunSignature, RunSignature)> {
    if !fuzz_input_is_runnable(src) {
        return None;
    }
    let bytecode = run_script_observed(src, FUZZ_EXECUTE_STEP_BUDGET, true);
    let tree_walker = run_script_observed(src, FUZZ_EXECUTE_STEP_BUDGET, false);
    if fuzz_input_is_gc_observable(src) {
        return None;
    }
    Some((bytecode, tree_walker))
}

/// Backs the `execute` fuzz target (`fuzz/fuzz_targets/execute.rs`): runs the
/// input on both execution engines under a step budget. A panic, abort or
/// sanitizer report is a finding, and so is an observable difference between
/// the bytecode VM and the tree-walker (reported as a panic starting with
/// `execution divergence`). Inputs that exhaust the budget are skipped.
pub fn fuzz_execute_bytes(data: &[u8]) {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    fuzz_execute_source(src);
}

/// The shared body of the execute-style fuzz targets (`execute`,
/// `execute_ast`): runs `src` on both engines and panics on a divergence.
pub fn fuzz_execute_source(src: &str) {
    run_on_engine_stack(move || {
        if let Some((bytecode, tree_walker)) = fuzz_execute_signatures(src)
            && let Some(report) = execution_divergence(&bytecode, &tree_walker)
        {
            panic!(
                "execution divergence ({} bytes of source)\n{report}",
                src.len()
            );
        }
    });
}

/// Backs the `reentrancy` fuzz target (`fuzz/fuzz_targets/reentrancy.rs`):
/// decodes the bytes into a re-entrancy plan, renders it to a JS program in
/// which Proxy traps, generators, accessors and callbacks call back into the
/// engine mid-operation, and runs it exactly like `fuzz_execute_bytes`.
pub fn fuzz_reentrancy_bytes(data: &[u8]) {
    fuzz_execute_source(&fuzz_reentrancy::Plan::decode(data).render());
}

#[cfg(test)]
mod lib_tests {
    use super::*;

    #[test]
    fn benchmark_validation_observes_drained_promise_jobs() {
        run_on_engine_stack(|| {
            let check = "if (result !== 42) throw new Error('wrong or missing result');";
            assert!(bench_eval_with_check(
                "var result; Promise.resolve(41).then(v => { result = v + 1; });",
                Some(check),
            ));
            for src in [
                "var result; Promise.resolve(0).then(v => { result = v; });",
                "var result; Promise.resolve().then(() => { throw new Error('bad'); });",
                "var result; Promise.reject(new Error('bad')).then(v => { result = v; });",
                "var result; new Promise(() => {}).then(v => { result = v; });",
            ] {
                assert!(!bench_eval_with_check(src, Some(check)), "accepted: {src}");
            }
        });
    }

    #[test]
    fn benchmark_validation_rejects_script_failures() {
        run_on_engine_stack(|| {
            assert!(bench_eval(""));
            assert!(!bench_eval("throw new Error('bad');"));
            assert!(!bench_eval("let ="));
            assert!(!bench_eval_with_check(
                "",
                Some("throw new Error('bad check');")
            ));
            assert!(!bench_eval_with_check("", Some("let =")));
        });
    }

    #[test]
    fn fuzz_execute_source_accepts_equal_modes() {
        fuzz_execute_source("var a = [1,2,3].map(x => x * 2); a.join();");
        fuzz_execute_source("throw new TypeError('both modes agree');");
    }

    #[test]
    fn fuzz_execute_source_skips_unsafe_idents() {
        fuzz_execute_source("while (true) { agent; }");
    }

    #[test]
    fn run_on_engine_stack_returns_result() {
        assert_eq!(run_on_engine_stack(|| 1 + 1), 2);
    }

    #[test]
    fn run_on_engine_stack_borrows_non_static_data() {
        // `ast::Program` holds `Rc`s and isn't `Send`, so (matching how a real
        // fuzz target must behave) the parse result is consumed entirely
        // inside the closure; only the `bool` verdict crosses the thread.
        let owned = "[".repeat(parser::MAX_PARSE_DEPTH as usize * 2);
        let borrowed: &str = &owned;
        let is_err = run_on_engine_stack(move || {
            parser::Parser::new(borrowed)
                .and_then(|mut p| p.parse_program())
                .is_err()
        });
        assert!(
            is_err,
            "deeply nested input should hit MAX_PARSE_DEPTH, not abort"
        );
    }

    #[test]
    fn fuzz_parse_bytes_empty_input() {
        fuzz_parse_bytes(b"");
    }

    #[test]
    fn fuzz_parse_bytes_invalid_utf8() {
        fuzz_parse_bytes(&[0xff, 0xfe, 0x00, 0x80]);
    }

    #[test]
    fn fuzz_parse_bytes_valid_program() {
        fuzz_parse_bytes(b"function f(x) { return x + 1; } f(41);");
    }

    #[test]
    fn fuzz_parse_bytes_deep_nesting() {
        fuzz_parse_bytes("[".repeat(parser::MAX_PARSE_DEPTH as usize * 2).as_bytes());
    }

    #[test]
    fn run_script_with_budget_classifies_outcomes() {
        run_on_engine_stack(|| {
            for bytecode in [true, false] {
                let run = |src, budget| run_script_with_budget(src, budget, bytecode);
                assert_eq!(
                    run("while (1) {}", Some(10_000)),
                    (ScriptOutcome::StepBudgetExhausted, 10_000)
                );
                assert_eq!(run("throw 1", None).0, ScriptOutcome::Threw);
                assert_eq!(run("let =", None).0, ScriptOutcome::ParseError);
                assert_eq!(run("1 + 1", None).0, ScriptOutcome::Completed);
                assert_eq!(
                    run("try { for (;;); } catch (e) {}", Some(100)).0,
                    ScriptOutcome::StepBudgetExhausted
                );
                for _ in 0..20 {
                    assert_eq!(
                        run("for (;;);", Some(50)).0,
                        ScriptOutcome::StepBudgetExhausted
                    );
                }
            }
        });
    }
}
