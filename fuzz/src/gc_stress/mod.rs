//! `gc_stress` fuzz target: the first input byte selects a GC stress period
//! and an execution mode, the rest is JS source. The script runs twice in the
//! same mode, once with stress off and once with a collection forced at every
//! Nth safepoint (alternating major and minor). A panic, abort, debug
//! assertion or sanitizer report is a finding, and so is any observable
//! difference between the two runs: collection must be unobservable, so a
//! difference means a live value was freed or a write barrier was missed. See
//! `docs/adr/2026-10-23-1800-gc-stress-fuzz-target.md`.

#[cfg(test)]
mod seeds;

/// Stress period selected by the low three header bits.
pub const PERIODS: [u32; 8] = [1, 2, 3, 5, 7, 16, 61, 251];

/// Header bit that selects the tree-walker instead of the bytecode VM.
pub const TREE_WALKER_BIT: u8 = 0b1000;

/// Hard cap on the steps of one run.
const MAX_STEP_BUDGET: u64 = 20_000;

/// Steps granted per unit of period, so the number of forced collections per
/// input stays roughly constant: period 1 collects the whole heap at every
/// safepoint.
const STEPS_PER_PERIOD: u64 = 400;

const MIN_STEP_BUDGET: u64 = 2_000;

/// Set to dump the script to stderr, for triaging a crash.
const DUMP_ENV: &str = "JSSE_FUZZ_DUMP_SCRIPT";

#[derive(Debug, PartialEq, Eq)]
pub struct Input<'a> {
    pub period: u32,
    pub bytecode: bool,
    pub src: &'a str,
}

/// Splits `data` into the header and the script. `None` for an empty input or
/// a body that is not UTF-8.
pub fn decode(data: &[u8]) -> Option<Input<'_>> {
    let (&header, body) = data.split_first()?;
    Some(Input {
        period: PERIODS[usize::from(header & 0b111)],
        bytecode: header & TREE_WALKER_BIT == 0,
        src: std::str::from_utf8(body).ok()?,
    })
}

/// Builds the input that [`decode`] maps to `period_index`, `bytecode` and `src`.
pub fn encode(period_index: u8, bytecode: bool, src: &str) -> Vec<u8> {
    let mode = if bytecode { 0 } else { TREE_WALKER_BIT };
    let mut bytes = vec![(period_index & 0b111) | mode];
    bytes.extend_from_slice(src.as_bytes());
    bytes
}

pub fn step_budget(period: u32) -> u64 {
    (u64::from(period) * STEPS_PER_PERIOD).clamp(MIN_STEP_BUDGET, MAX_STEP_BUDGET)
}

/// Runs `input` unstressed and stressed and describes how the observable
/// results differ, if they do.
pub fn stress_divergence(input: &Input<'_>) -> Option<String> {
    let Input {
        period,
        bytecode,
        src,
    } = *input;
    let budget = step_budget(period);
    let (plain, stressed) = jsse::run_on_engine_stack(move || {
        (
            jsse::run_script_gc_stressed(src, budget, bytecode, 0).0,
            jsse::run_script_gc_stressed(src, budget, bytecode, period).0,
        )
    });
    if jsse::fuzz_input_is_gc_observable(src) || plain == stressed {
        return None;
    }
    Some(format!("unstressed: {plain:?}\nstressed:   {stressed:?}"))
}

pub fn fuzz_gc_stress_bytes(data: &[u8]) {
    let Some(input) = decode(data) else {
        return;
    };
    if std::env::var_os(DUMP_ENV).is_some() {
        eprintln!("{}", input.src);
    }
    if !jsse::fuzz_input_is_runnable(input.src) {
        return;
    }
    if let Some(report) = stress_divergence(&input) {
        let mode = if input.bytecode {
            "bytecode"
        } else {
            "tree-walker"
        };
        panic!(
            "gc stress divergence (period {}, {mode})\n{report}\n--- script ---\n{}",
            input.period, input.src
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_reads_the_header() {
        assert_eq!(decode(&[]), None);
        assert_eq!(decode(&[0xFF, 0xFE]), None);
        assert_eq!(
            decode(&[0b1101, b'1']),
            Some(Input {
                period: PERIODS[5],
                bytecode: false,
                src: "1",
            })
        );
        assert_eq!(
            decode(&[0b0010]),
            Some(Input {
                period: PERIODS[2],
                bytecode: true,
                src: "",
            })
        );
    }

    #[test]
    fn encode_round_trips_through_decode() {
        for index in 0..8 {
            for bytecode in [true, false] {
                let bytes = encode(index, bytecode, "var a;");
                let input = decode(&bytes).unwrap();
                assert_eq!(input.period, PERIODS[usize::from(index)]);
                assert_eq!(input.bytecode, bytecode);
                assert_eq!(input.src, "var a;");
            }
        }
    }

    #[test]
    fn step_budget_scales_with_period_within_bounds() {
        assert_eq!(step_budget(1), MIN_STEP_BUDGET);
        assert_eq!(step_budget(251), MAX_STEP_BUDGET);
        assert!(step_budget(7) <= step_budget(16));
    }

    #[test]
    fn fixed_inputs_run_to_completion() {
        fuzz_gc_stress_bytes(&[]);
        fuzz_gc_stress_bytes(&[0xFF; 64]);
        fuzz_gc_stress_bytes(&encode(0, true, "throw 1"));
        fuzz_gc_stress_bytes(&encode(0, false, "let ="));
        fuzz_gc_stress_bytes(&encode(1, true, "for (;;);"));
        fuzz_gc_stress_bytes(&encode(1, true, "$262.agent.start('1')"));
    }

    #[test]
    fn transparent_script_has_no_divergence() {
        let src = "var l = null; for (var i = 0; i < 50; i++) l = { next: l, v: [i] }; l.v[0]";
        for bytecode in [true, false] {
            let input = Input {
                period: 1,
                bytecode,
                src,
            };
            assert_eq!(stress_divergence(&input), None);
        }
    }

    #[test]
    fn gc_observable_scripts_are_not_compared() {
        let input = Input {
            period: 1,
            bytecode: true,
            src: "var r = new WeakRef({}); typeof r.deref()",
        };
        assert_eq!(stress_divergence(&input), None);
    }
}
