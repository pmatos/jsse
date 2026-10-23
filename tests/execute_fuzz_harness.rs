//! The `execute` fuzz target's library plumbing: captured host output,
//! deterministic `Math.random`, virtual timers, and bytecode-vs-tree-walker
//! signature comparison.

use jsse::{
    RunSignature, SigOutcome, execution_divergence, fuzz_execute_signatures, run_on_engine_stack,
    run_script_observed,
};
use std::time::{Duration, Instant};

const BUDGET: u64 = 20_000;

fn observe(src: &str, bytecode: bool) -> RunSignature {
    run_on_engine_stack(move || run_script_observed(src, BUDGET, bytecode))
}

fn both(src: &str) -> (RunSignature, RunSignature) {
    (observe(src, true), observe(src, false))
}

fn signatures(src: &str) -> Option<(RunSignature, RunSignature)> {
    run_on_engine_stack(move || fuzz_execute_signatures(src))
}

#[test]
fn host_output_is_captured_per_stream() {
    let sig = observe(
        r#"print("a"); console.log(1, "b"); console.error("e"); console.assert(false, "m");"#,
        true,
    );
    assert_eq!(
        sig.output,
        "out: a\nout: 1 b\nerr: e\nerr: Assertion failed: m\n"
    );
}

#[test]
fn default_print_still_reaches_stdout() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(["-e", "print(1)"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "1\n");
}

#[test]
fn outcomes_match_in_both_modes() {
    let cases = [
        ("1+1", SigOutcome::Completed("2".into())),
        (
            "throw new TypeError('x')",
            SigOutcome::Threw("TypeError".into()),
        ),
        ("throw 7", SigOutcome::Threw("7".into())),
        ("let =", SigOutcome::ParseError),
        ("for(;;);", SigOutcome::BudgetExhausted),
    ];
    for (src, expected) in cases {
        let (bytecode, tree_walker) = both(src);
        assert_eq!(bytecode.outcome, expected, "bytecode: {src}");
        assert_eq!(tree_walker.outcome, expected, "tree-walker: {src}");
    }
}

#[test]
fn math_random_is_deterministic_across_runs_and_realms() {
    let src = "print(Math.random()); print(new ShadowRealm().evaluate('Math.random()'));";
    let (bytecode, tree_walker) = both(src);
    assert_eq!(bytecode, tree_walker);
    assert_eq!(observe(src, true), bytecode);
    let lines: Vec<&str> = bytecode.output.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_ne!(lines[0], lines[1], "realms must not share a stream");
}

#[test]
fn timers_fire_in_delay_order_without_sleeping() {
    let start = Instant::now();
    let sig = observe(
        "setTimeout(() => print('slow'), 20); setTimeout(() => print('fast'), 10); \
         setTimeout(() => print('never-waited'), 1e9); print('sync');",
        true,
    );
    assert!(start.elapsed() < Duration::from_secs(5));
    assert_eq!(
        sig.output,
        "out: sync\nout: fast\nout: slow\nout: never-waited\n"
    );
}

#[test]
fn uncleared_interval_ends_by_budget_not_wall_clock() {
    let start = Instant::now();
    let (bytecode, tree_walker) = both("setInterval(() => {}, 0);");
    assert!(start.elapsed() < Duration::from_secs(20));
    assert_eq!(bytecode.outcome, SigOutcome::BudgetExhausted);
    assert_eq!(tree_walker.outcome, SigOutcome::BudgetExhausted);
}

#[test]
fn hang_hazards_are_not_executed() {
    let start = Instant::now();
    for src in [
        "$262.agent.start('1')",
        "Atomics.waitAsync(new Int32Array(new SharedArrayBuffer(8)), 0, 0).value.then(() => {})",
        "$262.agent.sleep(100000)",
    ] {
        assert!(signatures(src).is_none(), "ran: {src}");
    }
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn nondeterministic_identifiers_run_but_are_not_compared() {
    assert!(signatures("print(Date.now())").is_none());
    assert!(signatures("$262.gc()").is_none());
}

#[test]
fn identifier_matching_respects_word_boundaries() {
    assert!(
        signatures("function gcd(a, b) { return b ? gcd(b, a % b) : a; } gcd(12, 18)").is_some()
    );
    assert!(signatures("var DateLike = 1; DateLike").is_some());
}

#[test]
fn comparison_runs_for_harness_shaped_seeds() {
    let src = "function Test262Error(m) { this.message = m; }\n\
               var assert = { sameValue(a, b) { if (a !== b) throw new Test262Error('x'); } };\n\
               assert.sameValue(1, 1);";
    let (bytecode, tree_walker) = signatures(src).expect("comparison must run");
    assert_eq!(bytecode, tree_walker);
}

#[test]
fn divergence_is_reported_and_exhaustion_is_not() {
    let sig = |outcome| RunSignature {
        outcome,
        output: String::new(),
    };
    let a = sig(SigOutcome::Completed("1".into()));
    let b = sig(SigOutcome::Completed("2".into()));
    assert!(execution_divergence(&a, &a).is_none());
    assert!(execution_divergence(&a, &b).is_some());
    let exhausted = sig(SigOutcome::BudgetExhausted);
    assert!(execution_divergence(&a, &exhausted).is_none());
    assert!(execution_divergence(&exhausted, &b).is_none());
    let noisy = RunSignature {
        outcome: a.outcome.clone(),
        output: "out: x\n".into(),
    };
    assert!(execution_divergence(&a, &noisy).is_some());
}

#[test]
fn wall_clock_reading_intl_formatter_is_not_compared() {
    assert!(signatures("new Intl.DateTimeFormat('en').format()").is_none());
}
