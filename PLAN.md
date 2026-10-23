# Plan: in-process `execute` fuzz target with bytecode-vs-tree-walker comparison (#1174)

## Problem restated

Parent #1168. The only in-process fuzz target is `parse`; the interpreter, GC, bytecode VM, inline caches, typed arrays and the other built-ins have no fuzz coverage, and `differential` runs jsse as a subprocess against `node` (tens of exec/s, no sanitizer). Both blockers are merged: ClusterFuzzLite with ASan for `parse` (#1180) and the opt-in step budget with `run_script_with_budget` (#1182). Add an `execute` cargo-fuzz target that parses and runs each input in a fresh `Interpreter` under a step budget, once on the bytecode VM and once on the tree-walker, and treats (a) any panic/abort/ASan report and (b) a divergence in observable outcome between the two modes as a finding. Budget-exhausted inputs are skipped, not reported. Seed the corpus from `tests/*.js` and test262, build it in ClusterFuzzLite so it runs in the PR (code-change) and batch jobs, and update the triage docs.

## Spec basis

N/A: no JavaScript behavior change. This adds fuzz tooling, a host-only output-capture/determinism hook and CI wiring; nothing observable to a script changes when the hooks are unset. (Bytecode and tree-walker are both required to implement the same ECMAScript semantics, which is why a mode divergence is a bug; a divergence is a lead and is resolved against `spec/`, never against `node`.)

## Assumptions (decided unattended)

- **What "observable outcome" means**: the tuple `(outcome kind, completion value or thrown class, captured print/console output)`. Thrown *messages* are NOT compared (diagnostic text is not spec-defined and the two modes may word it differently); only the error class (`TypeError` etc.) for Error objects, and the rendered value for thrown primitives. Messages still reach the output comparison only if a script prints them; the calibration slice (Slice 5) decides whether that needs normalising. Step counts are never compared (ADR 2026-10-23-1200: they legitimately differ between modes).
- **Budget exhaustion in either run => skip the whole input** (no comparison): the two modes exhaust at different points, so a completed-vs-exhausted pair is not a divergence. Initial budget `100_000` steps, tuned in Slice 5 against exec/s.
- **Nondeterminism handling**, two separate filters, both matched on identifier boundaries (not bare substrings, so `gc` does not hit `gcd`):
  - *Don't-execute* markers (`agent`, `waitAsync`, `sleep`): `$262.agent.start` spawns a thread with no step budget that outlives the interpreter, `$262.agent.sleep` is a real wall-clock sleep (`src/interpreter/mod.rs:1221`, `:5804`), and `Atomics.waitAsync` with a reaction keeps `has_awaited_pending_async` true so the drain waits on a condvar for up to 120 s regardless of virtual timers. Inputs containing them return before running either mode (libFuzzer still learns the coverage up to the check; the filter lives in `fuzz_execute_bytes`).
  - *Don't-compare* markers (`Date`, `Temporal`, `WeakRef`, `FinalizationRegistry`, `$262`, `Atomics`, `SharedArrayBuffer`, `performance`, `gc`): both modes still run (panic/ASan coverage is the main value) but the signatures are not compared.
  - `Math.random` is made deterministic, not filtered: the fuzz seed is per-interpreter and applied to *every* realm the interpreter creates (fixed seed XOR a per-interpreter realm index), because `new_math_random_state` (`types.rs:49`) mixes a process-global realm counter that would otherwise differ between run 1 and run 2 for `ShadowRealm` / `$262.createRealm` realms. Timers use a virtual clock in the fuzz path (no real sleeping).
- **Lives in the library, not the fuzz crate**: the runner and signature type go in `src/lib.rs` next to `run_script_with_budget` because `Interpreter` internals are `pub(crate)` and ADR 0004 keeps the fuzz crate to public API.
- **Seeds from test262** are generated (not hand-copied) with the harness (`sta.js` + `assert.js`) inlined so each seed is self-contained; ~40 small, flag-free, deterministic tests. `test262/` is read-only and is only read by the generator; it is empty in fresh workspaces, so the implementer runs `git submodule update --init --depth 1 test262`.
- **Batch/PR durations unchanged** (PR 120 s, batch 1800 s). ClusterFuzzLite splits `fuzz-seconds` between targets, so `parse` and `execute` each get half; documented, not "fixed" here. Raising them is a settings follow-up.
- **No new required status check**; same non-required, blocking-in-practice posture as `parse` (ADR 2026-10-23-0854).

## Files to touch

| File | Role | Lines of interest |
|---|---|---|
| `src/lib.rs` | `run_script_with_budget` (~l.110-140), `ScriptOutcome`, `parse_script`, `run_on_engine_stack`; add the observed-run API here | 33-48, 103-140, 142+ (`lib_tests`) |
| `src/interpreter/step_budget.rs` | `set_step_budget`, `step_budget_exhausted`, `steps_used`; reuse | 56-80 |
| `src/interpreter/builtins/host.rs` | `console_stdout_write` (l.8-14), `console_stderr_write` (l.16-25), `print` (l.177-195), `console.assert` writer (~l.108-125): all write straight to fd 1/2 | 8-25, 108-125, 177-195 |
| `src/interpreter/mod.rs` | `Interpreter` struct (`bytecode_enabled` l.433, `pending_exit` l.456, ctor l.883); `run_program` (l.2386); `drain_microtasks_until_idle` (l.5569-5625) sleeps on real timers up to 120 s | 433, 456, 883, 2386, 5569-5625 |
| `src/interpreter/scheduler.rs` | `TimerQueue::add` uses `Instant::now()`; `take_due`/`has_timers` | 100-135 |
| `src/interpreter/types.rs` | `new_math_random_state` (l.49, randomly seeded per realm), `Realm::math_random` (l.820) | 49-54, 820 |
| `src/interpreter/builtins/node_host.rs` | stdout/stderr in `--node` mode only; out of scope (fuzz path never enables node host) | 71-75 |
| `fuzz/Cargo.toml` | add `[[bin]] execute` | 17-30 |
| `fuzz/fuzz_targets/execute.rs` [new] | the target | model on `parse.rs`, `differential.rs` |
| `fuzz/corpus/execute/` [new] | seed corpus (pre-commit hooks exclude `fuzz/corpus/`) | |
| `scripts/gen-fuzz-execute-corpus.py` [new] | deterministic test262 seed picker | model on other `scripts/*.py` |
| `.clusterfuzzlite/build.sh` | `TARGETS=(parse)` -> `(parse execute)`; stale comment l.2-3 stays true for `differential` | 1-14 |
| `.github/workflows/clusterfuzzlite-{pr,batch,build}.yml` | already build all targets from `build.sh`; verify only, expect no change besides comments | |
| `.github/workflows/fuzz.yml` | header comment mentions "Parser fuzzing runs in clusterfuzzlite"; update to cover `execute` | 1-5 |
| `README.md` | "Fuzzing" section and CI table | 173-210 |
| `.claude/skills/fuzzing/SKILL.md` | run/triage workflow; add `execute` | whole |
| `CONTEXT.md` | add **Execution Divergence** term next to **Divergence Tier** (l.47-49) | 47-49 |
| `docs/adr/2026-10-23-HHMM-execute-fuzz-target.md` [new] | decision record (name per existing `docs/adr/2026-10-23-*` pattern) | |
| `docs/adr/0004-fuzz-lib-target-and-subprocess-differential.md` | append `Update:` note to the "in-process differential target with an interpreter step budget" out-of-scope bullet (same style as the existing updates) | "Deliberately out of scope" |
| `docs/adr/2026-10-23-0854-clusterfuzzlite-fuzzing.md` | tick the "further targets from #1168" follow-up | Consequences |
| `tests/execute_fuzz_harness.rs` [new] | integration tests of the new lib API (pattern: `tests/step_budget.rs`) | |

## Steps / TDD slices

Order matters: each slice is red-then-green; run `./scripts/ensure-hooks.sh` first. Build release for anything touching the binary; plain `cargo test` for lib/integration tests (`CARGO_PROFILE_*` overrides and `-j` caps per AGENTS.md; keep parallelism low).

### Slice 1: captured host output (the sink)
- **Test** [new] `tests/execute_fuzz_harness.rs` (or `lib_tests` in `src/lib.rs`): run `print("a"); console.log(1, "b"); console.error("e"); console.assert(false, "m")` through the new observed-run API and assert the captured string contains each line, tagged by stream, and that nothing reached the process stdout/stderr (test spawns no subprocess; assertion is on the returned value; add one subprocess test that `jsse -e 'print(1)'` still prints `1` to stdout so default behaviour is unchanged; `tests/console_stderr_routing.rs` must stay green).
- **Code**: add `Interpreter.host_capture: Option<String>` (+ `pub(crate) fn host_write(&mut self, stream, &str)`), initialised `None`. Route `console_stdout_write`, `console_stderr_write`, `print`, and the `console.assert` writer through it: `None` keeps today's `println!`/`write_all` exactly; `Some(buf)` appends `"out: "`/`"err: "`-prefixed lines. The natives already receive `interp: &mut Interpreter` (the first param is currently `_interp`). Reuses `format_host_args` (`host.rs:3`).

### Slice 2: deterministic Math.random and non-sleeping timers
- **Test**: the observed-run API on `Math.random() === Math.random()` style probes: `String(Math.random())` printed by two separate calls with the same seed in both modes yields identical captured output; `setTimeout(()=>print("t"), 1e9); print("s")` returns promptly (assert wall time < 5 s) with output `s` then `t`; two timers with delays 20 then 10 fire in delay order; `setInterval` that never clears terminates through the step budget (timer callbacks are ticked), not the 120 s deadline.
- **Code**: `Interpreter::set_virtual_timers(bool)`: when set, `drain_microtasks_until_idle` (`mod.rs:5569`) does not `Instant`-sleep: when nothing is due it advances the queue to the earliest timer (a `take_due(earliest)` / `advance_to_next` helper in `scheduler.rs`), preserving (fire time, id) ordering; the 120 s wall deadline stays as a backstop. Hot-path cost: one bool test in the already-cold drain loop; confirm nothing is added to safepoints.
- **Seeding must cover every realm**: replace the seed source in `new_math_random_state` with an interpreter-level override consulted at realm creation (e.g. `Interpreter.math_random_seed: Option<u64>` read by the realm constructor sites, mixed with the interpreter-local realm index). Extra test: `print(new ShadowRealm().evaluate("Math.random()"))` and `print(Math.random())` give identical captured output across the bytecode and tree-walker runs.
- **Hang-hazard tests** (wall time < 5 s each, in the new test file): a `$262.agent.start`-containing source and an `Atomics.waitAsync(...).value.then(...)` source are rejected by the don't-execute filter without spawning threads or sleeping.
- Check what `completion_wait` (mod.rs) does with host-async promises so Atomics-style waits cannot block the fuzz path (agent threads are covered by the marker filter; the target never sets `can_block`).

### Slice 3: observed-run API + comparison in the library
- **Test** (same new test file): table of sources -> expected signature in both modes: completion value (`1+1` -> `Completed("2")`), `throw new TypeError("x")` -> `Threw("TypeError")`, `throw 7` -> `Threw("7")`, `let =` -> `ParseError`, `for(;;);` -> `BudgetExhausted`, plus a synthetic-divergence unit test that feeds two hand-built differing signatures to the compare function and asserts a `Some(report)`; an exhausted pair returns `None`; a nondeterminism-marker source returns `None` while still executing both modes.
- **Code** in `src/lib.rs`: `pub struct RunSignature { outcome: SigOutcome, output: String }` (enum: `Completed(String)`, `Threw(String)`, `ParseError`, `BudgetExhausted`), `pub fn run_script_observed(src, budget, bytecode) -> RunSignature` (like `run_script_with_budget`: `parse_script`, fresh `Interpreter`, `bytecode_enabled`, `set_step_budget`, `set_virtual_timers(true)`, fixed Math.random seed, `host_capture = Some`, `interp.run`; thrown class via `interp.format_value` cut at the first `:` for objects), and `pub fn fuzz_execute_bytes(data: &[u8])`: UTF-8 gate (as `fuzz_parse_bytes`), `run_on_engine_stack(move || ...)` running bytecode then tree-walker, **dropping each interpreter** (do not `forget` as the CLI does; the process is long-lived), and `panic!("execution divergence: ...")` with both signatures and the source length when they differ. `Completion::Exit` other than budget (not reachable without node host) is treated as `Completed`. Mark the new items `#[doc(hidden)]` where they are fuzz plumbing, mirroring `bench_*`.
- **Reuses**: `parse_script` (`lib.rs:~100`), `ScriptOutcome` logic in `run_script_with_budget` (`lib.rs:~115`), `Interpreter::step_budget_exhausted`.

### Slice 4: the fuzz target, seeds, build wiring
- **Test**: `cd fuzz && cargo +nightly fuzz build execute` succeeds; `cargo +nightly fuzz run execute -- -runs=0 fuzz/corpus/execute` replays every seed with exit 0 (also under the default ASan sanitizer). A negative check: temporarily make `fuzz_execute_bytes` compare against a deliberately wrong signature (local, not committed) to see the panic path produce an artifact.
- **Code**:
  - `fuzz/fuzz_targets/execute.rs` [new]: `fuzz_target!(|data: &[u8]| { jsse::fuzz_execute_bytes(data); });` same shape as `parse.rs`.
  - `fuzz/Cargo.toml`: `[[bin]] name = "execute"` (same `test/doc/bench = false`).
  - `scripts/gen-fuzz-execute-corpus.py` [new]: walks `test262/test/{language,built-ins}` in sorted order, keeps tests with no `flags:`, `includes:` limited to none, `features:` free of the marker list, size <= ~2 KiB, spreads picks across top-level dirs, prepends `harness/sta.js` + `harness/assert.js`, writes `fuzz/corpus/execute/test262-<dir>-<name>.js`. Deterministic so reruns are reviewable.
  - `fuzz/corpus/execute/`: ~19 files copied from `tests/*.js` plus the existing fuzz seeds that are self-contained, plus ~40 generated test262 seeds. Exclude `tests/recursion-limit-*.js` (they need the plain release build's stack limits; ASan frames are larger) unless a local ASan replay proves them clean. The generator asserts the inlined `sta.js` + `assert.js` and every chosen test contain no don't-execute/don't-compare marker (otherwise every test262 seed would silently skip its comparison and "replay cleanly" would prove nothing); a unit-style check in the new test file runs one generated-shape seed (`assert.sameValue(1, 1)` with the harness prefix) and asserts the comparison actually ran (`fuzz_execute_signatures` returns `Some(..)` for both modes). Each file must stay under the 5,000-token staged-file limit (`scripts/check_staged_file_tokens.py`) ; generate then check `wc -c` <= 20000.
  - `.clusterfuzzlite/build.sh`: `TARGETS=(parse execute)`. Also write `$OUT/execute.options` (`[libfuzzer]` `max_len = 16384`) so seeds carrying the harness are not truncated, and the libFuzzer `-timeout` stays above the interpreter's worst case. Update the comment so it still states `differential` is excluded.
  - **ASan leak check** (verify, then decide): interpreter graphs use `Rc` and may leak cycles that LeakSanitizer reports as findings on every input. Run the replay under ASan locally. If leaks are reported and are not a real bug, add `detect_leaks = 0` to `execute.options` and record it in the ADR; if a leak is a real bug, file it, do not suppress silently.
- **Workflows**: `clusterfuzzlite-{pr,batch,build}.yml` build every target in `build.sh`, so no functional change is expected; update only header comments that say "the parse target". Keep action pins untouched.

### Slice 5: calibration (before declaring done)
- Reproduce CI exactly: `cargo +nightly fuzz build -O --debug-assertions execute` (ASan is the cargo-fuzz default) then run it for 5 minutes over `fuzz/corpus/execute`, and record exec/s. There is no `main` baseline build of `execute`, so ClusterFuzzLite cannot filter pre-existing crashes for this PR: any latent bug found by the PR's 120 s run counts against the PR, which is why every local finding must be triaged (filed as an issue and, where it blocks a clean replay, its trigger kept out of the committed seeds) before pushing. Run the generated seeds plus a ~200-file deterministic sample of `test262-extra/` through `fuzz_execute_bytes` (a throwaway `cargo test` loop or `-runs=0` over a temp dir under `$TMPDIR`) to find *existing* bytecode-vs-tree-walker divergences and noise sources (message wording, recursion-depth `RangeError` boundary, Function/class `toString`, object-key/Map order, `Symbol` descriptions).
- Decide per finding: (a) real engine bug -> file an issue, add the minimized input to nothing (the seed corpus must replay cleanly), and add a *narrow* normalisation/skip only if the behaviour is spec-unspecified; (b) noise -> normalise in `RunSignature` with a comment naming the clause that leaves it unspecified. Never normalise a spec-defined difference away.
- Tune the step budget so a typical input finishes in well under libFuzzer's timeout; record the final number in the ADR. Known non-budgeted hangs (step-budget ADR "Not covered": regex backtracking, `"x".repeat(2**28)`, expression-bodied-arrow iterator loops, exponential arrow recursion) surface as libFuzzer timeout/OOM artifacts; documented in triage, not suppressed.

### Slice 6: documentation
- ADR `docs/adr/2026-10-23-HHMM-execute-fuzz-target.md` [new]: decision (in-process, library-hosted runner, two-mode comparison, what is compared and what is deliberately not, skip rules, nondeterminism policy, virtual timers, seed generation, ClusterFuzzLite wiring, leak decision, rejected alternative of comparing step counts or messages).
- `CONTEXT.md`: **Execution Divergence** entry (bytecode vs tree-walker signature mismatch; distinct from **Divergence Tier**, which is jsse-vs-node); `_Avoid_: tier, mismatch`.
- `README.md` Fuzzing: three targets, CI table rows for `execute` (PR code-change, batch, ASan), command line, note that the two ClusterFuzzLite targets share the fuzz time.
- `.claude/skills/fuzzing/SKILL.md`: add `execute` to the description/front matter, replay/run commands, the "Where CI runs each target" list, and a triage section: an `execute` finding is either a panic/ASan report (same flow as `parse`) or an `execution divergence:` panic (reproduce with `jsse -e`/file in both modes: `jsse f.js` vs `jsse --no-bytecode f.js`, diff stdout/exit code; minimize with `tmin`; file as its own issue; fix against the spec); timeouts/OOMs from unbudgeted work; budget exhaustion is never a finding.
- ADR 0004 and ADR 2026-10-23-0854: short `Update:` notes (see Key files).

## Test surface

- No test262 directory exercises this (no engine semantics change). Do not touch `test262-pass.txt`; no `--update-baseline`.
- New Rust integration tests `tests/execute_fuzz_harness.rs` (host-only behaviour belongs in `tests/`, not `test262-extra/`, per AGENTS.md "exact host-compatibility diagnostics and engine resource-limit or stress checks remain in `tests/`").
- Existing gates to run after the engine-side slices: `cargo test` (lib + integration), `tests/console_stderr_routing.rs`, `tests/step_budget.rs`, `./scripts/lint.sh`, `cd fuzz && cargo fmt --check && cargo clippy --all-targets -- -D warnings` (manual, as ADR 0004 notes), `uv run python scripts/run-test262.py test262/test/built-ins/Math/ test262/test/built-ins/global/` and `uv run python scripts/run-custom-tests.py` to show print/timer/Math.random defaults are unchanged. Timer-heavy sanity: `uv run python scripts/run-test262.py test262/test/built-ins/Atomics/ test262-extra/` (async harness uses `print` + timers).
- Fuzz gates: `cargo +nightly fuzz build execute`, `-runs=0` replay (ASan on and `--debug-assertions`), a 5-minute mutation run (Slice 5).

## Regression risk

- **`print`/`console.*` routing** (Slice 1) is on the path every test262 async test relies on (`doneprintHandle.js` prints `Test262:AsyncTestComplete`). Default `None` branch must stay byte-identical to the current `println!`; covered by the subprocess test and the Atomics/async test262 directories. Perf: one `Option` test per print call, not a safepoint path.
- **Timer drain** (Slice 2) changes `drain_microtasks_until_idle`, shared with the CLI event loop and `drain_microtasks_blocking`. The virtual-timer flag must default off and be read only in the cold wait branch; do not alter safepoints, `GcPacer.poll`, `eval_expr`/`exec_statement` or the bytecode dispatch loop (the step-budget PR paid attention to exactly this: docs/perf/2026-10-23/1173-step-budget.md).
- **GC rooting**: no new roots; the captured string is a plain Rust `String`. In-process dropping of interpreters runs the debug arena assertions (`gc_assert_root_depth`); those are an upside (extra coverage) but may turn a latent teardown bug into a fuzz finding on day one: expected, file it, do not hide it.
- **Baseline**: `test262-pass.txt` should not move; any movement means the sink/timer change leaked into default behaviour: stop and fix.
- **No baseline for the new target**: the first PR run has no `main` ASan build of `execute` to diff against, so it can fail on latent bugs; mitigated only by Slice 5 local runs with the CI flags. If the first CI run still finds one, file it and re-run (documented in the triage section).
- **CI**: a latent panic or mode divergence can fail the blocking `ClusterFuzzLite PR` check on unrelated PRs. Mitigations already in place (main build upload filters pre-existing crashes); the seed corpus must replay cleanly before merge and Slice 5 triages the first-hour findings so the PR does not land a red check. The leak decision (Slice 4) is the other day-one hazard.
- **Throughput/timeouts**: two `Interpreter::new()` per input; if exec/s is far below `parse`, lower the budget rather than the input size.
- Node-compat harnesses and library tests do not use the new hooks (node host never enabled on the fuzz path); no impact expected.

## Out of scope

- Fixing any bug the new target finds (each is its own issue/PR, fixed against `spec/`/test262).
- Module (`.mjs`) inputs, `--node` host mode, Atomics agent threads, comparison of step counts or error messages.
- Making budget count function entries/native loops (follow-up listed in the step-budget ADR).
- Corpus pruning/coverage modes, a `storage-repo`, dictionaries (`.dict`), grammar-aware mutators; raising PR/batch fuzz durations; making the PR check a required status.
- Wiring `fuzz/` into `scripts/lint.sh`; any refactor of `differential`.
- A `--no-bytecode` vs bytecode check in CI over test262 (separate effort).
