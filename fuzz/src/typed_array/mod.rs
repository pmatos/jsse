//! `typed_array` fuzz target: decodes the input into a bounded sequence of
//! buffer, view and `Atomics` operations (see [`ops`]), renders it as a JS
//! program ([`script`]) and runs it on the bytecode VM and the tree-walker.
//! A panic, abort or sanitizer report is a finding, and so is an observable
//! difference between the two engines. See
//! `docs/adr/2026-10-23-1600-typed-array-fuzz-target.md`.

pub mod ops;
pub mod script;
#[cfg(test)]
mod seeds;

const STEP_BUDGET: u64 = 20_000;

/// Set to dump the generated script to stderr, for triaging a crash.
const DUMP_ENV: &str = "JSSE_FUZZ_DUMP_SCRIPT";

pub fn fuzz_typed_array_bytes(data: &[u8]) {
    let src = script::render(&ops::decode(data));
    if std::env::var_os(DUMP_ENV).is_some() {
        eprintln!("{src}");
    }
    let src = src.as_str();
    let divergence = jsse::run_on_engine_stack(move || {
        let bytecode = jsse::run_script_observed(src, STEP_BUDGET, true);
        let tree_walker = jsse::run_script_observed(src, STEP_BUDGET, false);
        jsse::execution_divergence(&bytecode, &tree_walker)
    });
    if let Some(divergence) = divergence {
        panic!("execution divergence\n{divergence}\n--- script ---\n{src}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_inputs_run_to_completion() {
        fuzz_typed_array_bytes(&[]);
        fuzz_typed_array_bytes(&[0xFF; 700]);
        let mut state = 7_u32;
        let data: Vec<u8> = (0..700)
            .map(|_| {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                (state >> 24) as u8
            })
            .collect();
        fuzz_typed_array_bytes(&data);
    }
}
