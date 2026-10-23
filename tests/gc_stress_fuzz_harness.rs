//! The `gc_stress` fuzz target's library plumbing: a per-interpreter GC stress
//! period applied in-process, and the input predicates shared with `execute`.

use jsse::{
    RunSignature, fuzz_input_is_gc_observable, fuzz_input_is_runnable, run_on_engine_stack,
    run_script_gc_stressed,
};

const BUDGET: u64 = 50_000;

const ALLOCATING: &str = "var keep = []; for (var i = 0; i < 40; i++) { keep.push({ a: [i, i + 1], s: 'x' + i }); } keep.length";

fn stressed(src: &str, bytecode: bool, period: u32) -> (RunSignature, u64) {
    run_on_engine_stack(move || run_script_gc_stressed(src, BUDGET, bytecode, period))
}

#[test]
fn stress_is_applied_in_process() {
    for bytecode in [true, false] {
        let (_, off) = stressed(ALLOCATING, bytecode, 0);
        assert_eq!(off, 0, "period 0 must not count stress safepoints");
        let (_, on) = stressed(ALLOCATING, bytecode, 1);
        assert!(on > 0, "period 1 must count stress safepoints (bytecode={bytecode})");
    }
}

#[test]
fn stressed_signature_equals_unstressed_for_plain_script() {
    for bytecode in [true, false] {
        for period in [1, 2, 7] {
            let (base, _) = stressed(ALLOCATING, bytecode, 0);
            let (sig, _) = stressed(ALLOCATING, bytecode, period);
            assert_eq!(base, sig, "period {period}, bytecode={bytecode}");
        }
    }
}

#[test]
fn gc_unobservable_inputs_are_not_compared() {
    assert!(!fuzz_input_is_runnable("$262.agent.start('1')"));
    assert!(fuzz_input_is_runnable("var a = [1];"));
    assert!(fuzz_input_is_gc_observable("new WeakRef({})"));
    assert!(fuzz_input_is_gc_observable("FinalizationRegistry"));
    assert!(!fuzz_input_is_gc_observable("gcSomething"));
    assert!(!fuzz_input_is_gc_observable("var a = {};"));
}
