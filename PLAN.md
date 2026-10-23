# Plan: `gc_stress` fuzz target (#1178)

## Problem restated

`JSSE_GC_STRESS=N` (`src/interpreter/gc.rs:53-62`, read once in `GcPacer::new`) forces a collection at every Nth safepoint, alternating major (finds missing roots) and minor (finds missing write barriers). Today it only runs over fixed corpora (`test262-extra/` in `ci.yml:127-135`, a nightly test262 sample). The in-process `execute` target (#1183) and the `typed_array` target (#1194) never enable it, and the env var cannot vary per input. Build a fourth ClusterFuzzLite target, `gc_stress`, whose input carries the stress period (and engine mode) in a header, applied in-process, over allocation-heavy scripts, with a committed seed corpus. Oracle: panic/abort/ASan/debug-assertion (`gc_assert_root_depth`, LIFO root-stack asserts) **plus** a GC-transparency check: the same script run with stress off vs stress on, in the same execution mode, must have an identical observable signature (a freed-then-recycled object shows up as a wrong-typed value or spurious `TypeError`). Engine bugs it finds are filed as separate issues, not fixed in this PR.

## Spec basis

N/A: no JavaScript behavior change. This adds fuzz tooling and a debug/test hook for an engine-internal GC mode. The oracle rests on the observation that ECMA-262 does not make garbage collection observable except through `WeakRef` / `FinalizationRegistry` / `WeakMap` liveness (the "Weak References" and "Processing Model of WeakRef and FinalizationRegistry Targets" clauses, `KeepDuringJob`; clause numbers not cited because the `spec/` submodule is not checked out in this workspace), so inputs using `WeakRef`, `FinalizationRegistry`, `gc`, `$262` are not compared (reusing the `execute` skip list).

## Assumptions

- Header design (best guess): first input byte is the header (`period = PERIODS[b & 7]`, bit 3 = run on the tree-walker instead of the bytecode VM); the rest must be UTF-8 JS source, else the input is ignored (as in `execute`). Bytes mutate independently of the source, so libFuzzer keeps exploring periods. Alternative rejected: deriving the period from a hash of the source (a one-byte source edit reshuffles the period; poor locality).
- Two runs per input (stress off, stress on, same mode), not four: the mode bit picks which engine gets covered, and bytecode-vs-tree-walker is already `execute`'s job. Four runs would halve throughput that is already ~100x below plain libFuzzer.
- Harness lives in the `fuzz` crate (`fuzz/src/gc_stress/`), like `typed_array`, using only `pub` `jsse` API.
- Period table is a starting guess: `[1, 2, 3, 5, 7, 16, 61, 251]`; step budget starts at 5,000 (not 20,000) because period 1 with a major GC per statement is O(heap) per step. **Tune both by measurement in slice 5** (target >= ~20 exec/s under ASan with debug assertions); record the final numbers in the ADR.
- Baseline run sets the period explicitly to 0 so a developer's exported `JSSE_GC_STRESS` cannot turn the baseline into a stressed run.
- No new workflow files: ClusterFuzzLite builds and runs every target `build.sh` emits (precedent: `typed_array`, #1194 changed no workflow).
- Equality rule: compare signatures exactly, including `BudgetExhausted`. Stress does not add safepoint ticks (collection happens inside the tick, `step_budget.rs:80-95`), so a step-count difference between stressed and unstressed runs is itself a bug signal. If slice 5 shows this is noisy (e.g. collection-triggered jobs), fall back to `execution_divergence`'s skip-on-exhaustion rule and say so in the ADR.

## Key Files

| File | Role | Lines |
|------|------|-------|
| `src/interpreter/gc.rs` | `GcPacer::set_stress_period` is `#[cfg(test)]` — make it available outside tests; add `stress_safepoints()` accessor | 121-125 (setter), 37-39 (`stress_count`), 193-205 (`begin_collection`) |
| `src/interpreter/step_budget.rs` | `set_step_budget` (model for a sibling setter, `set_budget_active` also syncs `poll`) | 62-70 |
| `src/interpreter/fuzz_host.rs` | Existing fuzz-only `Interpreter` hooks (`capture_host_output`, `use_virtual_timers`, `set_math_random_seed`); new `set_gc_stress_period` / `gc_stress_safepoints` go here | 20-45 |
| `src/lib.rs` | `run_script_observed` (192-222) to refactor behind a shared inner fn; `FUZZ_EXECUTE_*` consts (130-160) and `contains_ident` (225) to expose a `pub` predicate | 117-290 |
| `fuzz/src/typed_array/{mod.rs,seeds.rs}` | Reference for fuzz-crate target layout, seed definition + regenerate/verify tests | all |
| `fuzz/src/lib.rs` | add `pub mod gc_stress;` next to `pub mod typed_array;` | 8 |
| `fuzz/Cargo.toml` | new `[[bin]] gc_stress` | 15-45 |
| `.clusterfuzzlite/build.sh` | `TARGETS=(parse execute typed_array)`; add `gc_stress` + `gc_stress.options` | 5, 18-27 |
| `tests/execute_fuzz_harness.rs`, `tests/gc_stress.rs` | Model for root-crate integration tests of the new lib API | 1-40 |
| `README.md` | Fuzzing section: "Four targets", run commands, CI table, corpus list | 173-211 |
| `.claude/skills/fuzzing/SKILL.md` | target list, replay/run commands, CI list, triage section | 8-80 |
| `docs/adr/2026-10-23-1800-gc-stress-fuzz-target.md` [new] | decision record (CONTEXT.md is at its size limit, 31 KB, so new terms go in the ADR) | — |

## Steps (TDD slices)

### 1. Engine hook: per-interpreter GC stress period (red → green)
- **Test first** — `tests/gc_stress_fuzz_harness.rs` [new]: `stress_is_applied_in_process` runs an allocating loop through the new `jsse::run_script_gc_stressed(src, budget, bytecode, period)` and asserts `stress_safepoints > 0` for period 1 and `== 0` for period 0, in both modes; `stressed_signature_equals_unstressed_for_plain_script` asserts equal `RunSignature`s. A unit test in `gc.rs` (next to `stress_period_zero_never_collects`, `gc.rs:1963`) asserts `set_stress_period` after construction overrides whatever the env-derived period was. The env-override case is not tested through `std::env::set_var` (unsafe in edition 2024, process-global).
- **Production** — `gc.rs`: drop `#[cfg(test)]` from `set_stress_period` (keeps `stress_count = 0; sync_poll()`), add `pub(crate) fn stress_safepoints(&self) -> u64 { self.stress_count }`. `fuzz_host.rs`: `Interpreter::set_gc_stress_period(u32)` and `gc_stress_safepoints()`. `src/lib.rs`: extract the body of `run_script_observed` into a private `observe(src, budget, bytecode, stress: Option<u32>) -> (RunSignature, u64)`; keep `run_script_observed` as `observe(.., None).0` (no behaviour change for `execute`/`typed_array`); add `pub fn run_script_gc_stressed(src, budget, bytecode, period: u32) -> (RunSignature, u64)` that always calls `set_gc_stress_period(period)` (so 0 = explicitly off). Verify the unused-`cfg(test)` clippy/dead-code gate (`-D warnings`, lib+bin) stays clean.
- **Reuses**: `GcPacer::sync_poll` (`gc.rs:94`) so the single-load poll stays correct (`safepoint_pending` debug-asserts `poll == wanted_poll`).

### 2. Expose input-eligibility predicates (red → green)
- **Test** (same new test file): `gc_unobservable_inputs_are_not_compared` — `jsse::fuzz_input_is_runnable("$262.agent.start('1')") == false`; `fuzz_input_is_gc_observable("new WeakRef({})") == true`, `"gcSomething"` false (identifier boundary).
- **Production** `src/lib.rs`: two small `pub fn`s wrapping the existing `FUZZ_EXECUTE_SKIP_IDENTS` / `FUZZ_EXECUTE_NONDETERMINISTIC_IDENTS` + `contains_ident`; make `fuzz_execute_signatures` use them (no behaviour change). Keeps one list of hazard identifiers instead of a copy in the fuzz crate.

### 3. Fuzz crate: decoder + oracle (red → green)
- **Test first** — `fuzz/src/gc_stress/mod.rs` [new] `#[cfg(test)]`: `decode` unit tests (empty input → ignored; header bit 3 → tree-walker; `PERIODS[b & 7]`; invalid UTF-8 body → ignored); `fixed_inputs_run_to_completion`; `transparent_script_has_no_divergence`.
- **Production**: `fuzz/src/gc_stress/mod.rs` with `pub fn fuzz_gc_stress_bytes(data: &[u8])`: decode header; skip if not runnable; `run_on_engine_stack` runs `run_script_gc_stressed(src, BUDGET, bytecode, 0)` then `(…, period)`; skip comparison when `fuzz_input_is_gc_observable`; if signatures differ, `panic!("gc stress divergence (period {period}, {mode})\nunstressed: {a:?}\nstressed:   {b:?}\n--- script ---\n{src}")`. Honour `JSSE_FUZZ_DUMP_SCRIPT` like `typed_array/mod.rs:14`. `fuzz/fuzz_targets/gc_stress.rs` [new] (4 lines, as `typed_array.rs`); `fuzz/src/lib.rs` `pub mod gc_stress;`; `fuzz/Cargo.toml` `[[bin]] name = "gc_stress"`.

### 4. Seed corpus (red → green)
- **Test first** — `fuzz/src/gc_stress/seeds.rs` [new, `#[cfg(test)]`], mirroring `typed_array/seeds.rs`: `committed_seeds_match_definitions_and_run_clean` (committed `.bin` files equal the definitions; every seed decodes, completes without budget exhaustion in both stressed and unstressed runs, signatures match, and `stress_safepoints > 0` so the seed really exercises stress; every period index and both modes are used by at least one seed; each seed asserts `stress_safepoints >= 2 * period`, which proves both a stressed major (first at `stress_count == period`) and a stressed minor (at `2*period`, the write-barrier half) fired — `> 0` alone would not; pick small periods for short seeds and large periods only for loop-heavy seeds) and ignored `regenerate_seed_corpus`.
- **Seeds** (~14, `fuzz/corpus/gc_stress/NN-<name>.bin`, each a few hundred bytes, no `Date`/`WeakRef`/`gc`/`$262`/`agent`): linked list + tree building in loops; closures capturing fresh objects in loops; array/object/string growth via `push`/spread/`concat`; `Map`/`Set`/`WeakMap`/`WeakSet` churn; **native → JS re-entry holding unrooted values** (the highest-yield class, since safepoints only occur in JS code): `Array.prototype.sort/map/reduce/from` callbacks that allocate, `String.prototype.replace` callback, `JSON.parse` reviver / `stringify` toJSON, getters/setters and `valueOf`/`toString` coercion allocating, `Proxy` traps, `Reflect.apply`; class instances + `super` + private fields; generators / async functions / promise chains / `await` in loops; destructuring + spread + tagged templates; iterator helpers; `Symbol` and `BigInt` churn; typed array + `DataView` allocation; `eval` / `new Function` in loops; each across several periods and both modes (tree-walker seeds get header bit 3). Seeds are authored once in `seeds.rs` and written by the regenerate test.

### 5. Measure and tune (no new tests; recorded in ADR)
- Bound work per input, not just steps: period 1 with 5,000 steps is ~2,500 full-heap majors (all realm intrinsics, under ASan), seconds per input against ~30 s per target in the PR job. Prefer a budget that scales with period, e.g. `min(20_000, period * K)`, so stressed collections per input stay roughly constant; record the chosen rule in the ADR.
- Run `cargo +nightly fuzz run --debug-assertions gc_stress fuzz/corpus/gc_stress -- -runs=0 -max_len=4096` and a 5-minute run at CI flags. Pick `BUDGET` and `PERIODS` so throughput is acceptable and no seed times out. Confirm the target can actually find something: temporarily (locally, never committed) comment out one `collect_gc_roots` root and check a seed trips the oracle or a stress-only assertion. Any real finding → `gh issue create` (title `bug(gc): …`, minimized input, stressed/unstressed signature), not a fix here.

### 6. ClusterFuzzLite wiring
- `.clusterfuzzlite/build.sh`: `TARGETS=(parse execute typed_array gc_stress)`; add `$OUT/gc_stress.options` with `max_len = 4096` (seeds are small; larger inputs only slow stressed runs). The existing zip line already ships `fuzz/corpus/gc_stress/*` as `gc_stress_seed_corpus.zip`. Update the stale `build.sh:2-3` comment only if it lists targets. No change to `clusterfuzzlite-{pr,batch,build}.yml` (they run all targets); update comment/README wording "shared between the three targets" → four.

### 7. Docs
- `docs/adr/2026-10-23-1800-gc-stress-fuzz-target.md` [new]: context (stress only ran on fixed corpora), decision (header format, two-run transparency oracle, skip list, period table, budget, final tuned numbers), what is *not* covered (collection never fires at allocation time, so allocation-time rooting gaps between safepoints are invisible to stress by design; module inputs; minor-GC barrier bugs surface only if the old→young store happens before a stressed minor), consequences (CI PR job's fixed 120 s is now split across four targets; latent-bug PR failures as for `execute`), known limitation (the skip list matches identifiers in source, so computed access like `globalThis["Weak"+"Ref"]` evades it; a divergence involving one is a harness false positive, not a GC bug; `execute` has the same hole), rejected alternatives (period from source hash; four-way matrix; setting `JSSE_GC_STRESS` env per input — read once per interpreter and `set_var` is unsafe/global).
- `README.md` Fuzzing section: "Five targets"/list, run commands (`cargo +nightly fuzz run gc_stress -- -max_total_time=60 -max_len=4096`), CI table rows, corpus-dir list. `.claude/skills/fuzzing/SKILL.md`: description/target list, replay + run commands, CI list, triage bullet ("`gc_stress` failed: panic starting `gc stress divergence` → reproduce with `JSSE_GC_STRESS=<period> jsse [--no-bytecode] f.js` vs unset; minimize; stress-only failure = live value unreachable from `collect_gc_roots`; see AGENTS.md "GC Stress Mode"; header byte reading: `PERIODS[b & 7]`, bit 3").
- `AGENTS.md` "GC Stress Mode": one bullet pointing at the new target and ADR.

## Testing

- `cargo test --profile release-nolto --test gc_stress_fuzz_harness` (new) and the existing `execute_fuzz_harness`, `gc_stress` integration tests (regression for the `run_script_observed` refactor).
- `cargo test --manifest-path fuzz/Cargo.toml --lib` (decoder, oracle, seed round-trip + coverage; not run by `ci.yml`, run locally and say so in the PR).
- `cargo +nightly fuzz run --debug-assertions gc_stress fuzz/corpus/gc_stress -- -runs=0 -max_len=4096` (replay) plus the 5-minute run from slice 5; also replay `execute` and `typed_array` corpora to confirm no change.
- `./scripts/lint.sh`; `./scripts/ensure-hooks.sh` before committing.
- test262: no engine semantics change, but the refactor touches `run_script_observed` and `GcPacer`; run `uv run python scripts/run-test262.py test262/test/built-ins/WeakMap/ test262/test/built-ins/WeakRef/ test262/test/built-ins/FinalizationRegistry/` and `JSSE_GC_STRESS=7 uv run python scripts/run-test262.py test262-extra/ --timeout 300` as smoke checks. No new `test262-extra/` test: nothing spec-observable changes (hence no spec-clause-keyed test); engine-resource and harness checks belong in `tests/` and `fuzz/`.

## Regression risk

- `test262-pass.txt` baseline: should not move (no interpreter semantics touched). Do not update the baseline.
- `GcPacer` change is only removing `#[cfg(test)]` from a setter and adding a getter; the hot path (`safepoint_pending` single load, `poll` sync) is untouched. Confirm `set_stress_period` calls `sync_poll()` so the `debug_assert_eq!(self.poll, self.wanted_poll())` holds, including interaction with `set_budget_active` (order of the two setters must not matter).
- `src/lib.rs` refactor of `run_script_observed` is shared by `execute` and `typed_array`; the integration tests `tests/execute_fuzz_harness.rs` and `typed_array` seed test guard it.
- New target can fail the blocking `ClusterFuzzLite PR` check on an unrelated PR if it quickly finds a latent GC bug (same consequence accepted for `execute`/`typed_array`). Mitigation: replay seeds clean locally before pushing; file any finding as its own issue; if a real finding makes seeds fail, drop that seed rather than weakening the oracle, and note it on the issue.
- Throughput: stressed runs at period 1–3 are slow; PR job's 120 s is now split four ways. Tuned budget/periods in slice 5 bound this; raising `fuzz-seconds` is a maintainer follow-up.

## Out of scope

- Fixing any engine bug the target finds (separate issues per root cause, per acceptance criteria).
- Four-way (bytecode × tree-walker × stress) matrix, a grammar-aware generator or dictionary, module inputs, allocation-time (non-safepoint) GC stress, `$262.agent` / multi-thread stress.
- Changing `JSSE_GC_STRESS` semantics, the nightly test262 stress job, or `ci.yml`.
- Raising `fuzz-seconds` in the ClusterFuzzLite workflows; editing `symphonika/`; `CONTEXT.md` (at size limit; terms live in the ADR).
