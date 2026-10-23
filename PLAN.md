# Plan: `reentrancy` fuzz target (Proxy traps and generators re-entering the engine)

Classification: Medium (new module + fuzz crate + CI/docs wiring; no engine semantics change).

## Problem restated

`execute` (#1183) feeds arbitrary bytes to the engine as source text; random bytes almost never form a script in which a Proxy trap or generator `next()` mutates, closes, or re-invokes the very object an in-flight builtin is operating on. Those paths (user code running mid-`[[Get]]`, `Array.prototype.sort`, Map iteration, `yield*`, ...) are where invalid state held across user code shows up (stale lengths, stale indices into `Vec`s, unrooted ids across a GC, `RefCell`-style double borrows, missing invariant re-checks). #1177 adds a structure-aware target: the fuzzer's bytes decode into a small *Re-entrancy Plan* (which operation is re-entered, by which trap or generator step, what the callback does), the plan is rendered to a deterministic JS program, and the program runs through the existing `execute` oracle (panic/abort/ASan, plus bytecode-vs-tree-walker Execution Divergence). It runs in ClusterFuzzLite with a seed corpus, and engine bugs it finds are filed as separate issues, not fixed here.

## Spec basis

N/A: no JavaScript behavior change. This is fuzz tooling (a harness, a corpus, CI wiring); the engine is untouched apart from a test-only hook being made available to the harness. The generated programs exercise these clauses (informational, cited by name only: `spec/` is empty in this workspace so clause numbers could not be verified): Proxy Object Internal Methods and invariants, GeneratorValidate / GeneratorResume ("already running" TypeError), IteratorStepValue / IteratorClose, CreateListFromArrayLike, EnumerateObjectProperties (for-in), `Array.prototype.{sort,map,filter,splice,concat,flat}`, `Map.prototype.forEach` / `%MapIteratorPrototype%.next` and Set equivalents, `JSON.parse` reviver / `JSON.stringify` replacer, resizable-ArrayBuffer detach/shrink during TypedArray ops.

## Assumptions

- Input encoding: hand-written byte cursor, no `arbitrary` derive (best guess: avoids depending on libfuzzer-sys's re-export features; seeds stay tiny raw byte files; reading past the end yields 0 so every input decodes).
- Oracle reused as-is: `fuzz_execute_signatures` (src/lib.rs:~232) with `FUZZ_EXECUTE_STEP_BUDGET` 20,000. No new oracle in v1; GC-stress is an optional last slice.
- No `$262`/`gc` in generated programs (both mark the input non-comparable, `FUZZ_EXECUTE_NONDETERMINISTIC_IDENTS`), so every generated program is divergence-compared. GC-under-re-entrancy coverage comes from the stress hook in slice 6 (collection is not observable, so comparison stays on).
- PR code-change fuzzing is blocking and there is no `main` build of the new target to filter latent crashes against (same situation ADR 2026-10-23-1400 documents for `execute`). Mitigation: the implementer runs a local session at CI flags before opening the PR; any crash found is filed as its own issue (acceptance criterion 3) and the PR description links it. If a crash blocks the PR check, state so in the PR rather than fixing the engine in this PR.
- `fuzz-seconds` stays 120 s (ADR 2026-10-23-0854 decision 2); the budget is now split three ways. Not bumped here (a CI-policy change).
- `CONTEXT.md` is at its size limit (31 KB); the **Re-entrancy Plan** term is defined in the new ADR, as the `execute` ADR did for Execution Divergence.
- Repo file token limit is 5,000 tokens (~20 KB) per authored file (`scripts/check_staged_file_tokens.py`): the new code is split across files and `src/lib.rs` (15 KB) only gets a few lines.

## Files to touch (Key Files)

| File | Role | Lines of Interest |
|------|------|-------------------|
| `src/lib.rs` | `fuzz_execute_signatures`, `fuzz_execute_bytes`, `run_script_observed`, `contains_ident`, constants; add `mod fuzz_reentrancy;` + `pub fn fuzz_reentrancy_bytes` | 148-300 |
| `src/fuzz_reentrancy/mod.rs` [new] | `Plan` decoder (byte cursor), `Plan::render() -> String`, `fuzz_reentrancy_bytes`, unit tests | n/a |
| `src/fuzz_reentrancy/ops.rs` [new] | JS fragment tables: one `fn` per `Op` and `Host`; `Op::ALL`, `Host::ALL` | n/a |
| `src/interpreter/fuzz_host.rs` | host hooks (capture, virtual timers, seed); add `set_gc_stress_period` (slice 6) | 1-60 |
| `src/interpreter/gc.rs` | `GcPacer::set_stress_period` is `#[cfg(test)]` (line ~120): widen to `pub(crate)` for slice 6 | 119-124 |
| `fuzz/fuzz_targets/reentrancy.rs` [new] | `fuzz_target!(|data: &[u8]| jsse::fuzz_reentrancy_bytes(data))`, mirrors `execute.rs` | n/a |
| `fuzz/Cargo.toml` | add `[[bin]] name = "reentrancy"` (copy `execute` block) | 31-36 |
| `fuzz/corpus/reentrancy/*.bin` [new] | seed corpus, one seed per Op x Host pairing (min), plus a few multi-scenario seeds | n/a |
| `scripts/gen-fuzz-reentrancy-corpus.py` [new] | deterministic seed writer from a readable table (pattern: `scripts/gen-fuzz-execute-corpus.py`) | n/a |
| `.clusterfuzzlite/build.sh` | `TARGETS=(parse execute reentrancy)`; write `reentrancy.options` (`max_len = 64`) | 4, 15-19 |
| `.github/workflows/clusterfuzzlite-pr.yml`, `clusterfuzzlite-batch.yml` | header comments only ("parse and execute" -> three targets); no logic change (build.sh drives target list) | 1-2 / 1 |
| `.github/workflows/fuzz.yml` | header comment mentions parse/execute | 2 |
| `AGENTS.md` | "Source Layout": add one line for `src/fuzz_reentrancy/` (and `fuzz_host.rs` if slice 6 lands) | Source Layout |
| `README.md` | "Fuzzing" section (~173-212): intro + CI table rows | 173-212 |
| `.claude/skills/fuzzing/SKILL.md` | add `reentrancy` to target list, replay/mutation commands, CI section | 1-60 |
| `docs/adr/2026-10-23-1600-reentrancy-fuzz-target.md` [new] | decision record (format of the 1400 ADR) | n/a |

## Design

**Plan encoding.** `Plan { scenarios: Vec<Scenario> }`, 1..=4 scenarios (`1 + b % 4`), then per scenario 6 bytes, each reduced `% N`:
`op` (what is re-entered), `host` (who calls back), `hook` (which trap / generator step), `action` (what the callback does), `step` (fire on the Nth invocation of the hook, 0..=3, so "from which trap or generator step"), `shape` (target shape: plain array, sparse array, array with holes, Map, Set, object with accessors, TypedArray over resizable buffer).

**Ops (`Op`)** — the re-entered operation, each a short JS statement over a target `T`: `get`/`set`/`has`/`delete`/`in`, `for-in`, `Object.keys/entries/assign/defineProperty/freeze/getOwnPropertyDescriptors`, spread `[...T]`, array destructuring, `Array.from`, `Array.prototype.{sort,map,filter,splice,concat,flat,reverse,includes,join,push}`, `Map/Set forEach` and `for-of` iteration, `new Map(iterable)`/`new Set(iterable)`, Set methods with a set-like argument, `yield*` delegation, `Promise.all([...])`/`await` with a generator-backed iterable, `JSON.stringify` replacer / `JSON.parse` reviver, `Reflect.ownKeys`, `Object.setPrototypeOf` through proxies, `Function.prototype.apply` with a proxied array-like, `String.prototype.replace` with callback, TypedArray `fill/set/slice` with a `valueOf` that shrinks/detaches the buffer.

**Hosts (`Host`)** — `ProxyTrap` (hook picks one of get/set/has/deleteProperty/ownKeys/getOwnPropertyDescriptor/defineProperty/getPrototypeOf/setPrototypeOf/isExtensible/preventExtensions/apply/construct), `Generator` (hook picks the Nth `next()`/`return()`/`throw()` step, or a `yield*` inner iterator), `Accessor` (getter/setter on the target or prototype), `Callback` (comparator/mapper/replacer/reviver/`valueOf`/`toString`/`Symbol.iterator`).

**Actions (`Action`)** — what the callback does when it fires: nothing (trace only), mutate target (push/pop/length=0/`splice`), delete a key, redefine a property (non-configurable, accessor<->data), `Object.freeze`/`preventExtensions`, `clear()`/`delete` on the collection being iterated, call the *same* op again on the same target (nested, depth-capped), call `gen.next()` on the generator currently running ("already running"), `gen.return()`/`throw()`, throw a fresh error, return an invariant-violating trap result, run `Array.prototype.sort`/`Object.keys` on the target, shrink/detach the buffer.

**Rendering.** `Plan::render()` emits one self-contained script: a helper prelude (a `calls` counter capping re-entry at 8 and a `depth` cap of 3 so nesting stays under the 20,000-step budget; `trace(tag)` pushing to a log array), then per scenario a `try { ...op... } catch (e) { trace(e.name) }` block, then `print(log.join())` and a final snapshot print of each target (`JSON.stringify(Object.keys(T))`, length, size) so state corruption surfaces as a bytecode/tree-walker mismatch, not only as a crash. Property names and identifiers are fixed strings that avoid every `FUZZ_EXECUTE_SKIP_IDENTS` / `FUZZ_EXECUTE_NONDETERMINISTIC_IDENTS` marker (no `Date`, `gc`, `agent`, `$262`, ...). Loop bounds are literals; no unbounded generators.

**Rendering rules (each also asserted by a test).**
- Every numeric literal the generator emits (lengths, indices, `resize`/`length=` values, trap-returned sizes, key counts) comes from a small fixed table (0..8). Unbounded native loops are not charged by the step budget, so a huge length would become a timeout/OOM artifact the harness caused itself, and a blocking PR check would fail on it.
- Detach buffers only with `ArrayBuffer.prototype.transfer()`; never `$262.detachArrayBuffer`, `gc`, growable `SharedArrayBuffer`, or any other marker identifier (the input would stop being compared).
- The `await` / `Promise.all` op is rendered inside an `async function` (top-level `await` is a SyntaxError in a script); confirm `interp.run` drains microtasks before the final `print`, otherwise async scenarios show nothing to compare (if it does not, drain via the same path `bench_eval_with_check` observes).
- The final per-target snapshot is wrapped in `try`/`catch`, so a throwing proxy trap does not make every scenario end in the same uncaught throw.

**Entry point.** `pub fn fuzz_reentrancy_bytes(data: &[u8])` decodes, renders, then does exactly what `fuzz_execute_bytes` does on the rendered `&str` (shared helper so the divergence panic text and `run_on_engine_stack` use are not duplicated: extract the body of `fuzz_execute_bytes` after the UTF-8 check into `fn execute_and_compare(src: &str)` and call it from both).

## Steps / TDD slices (ordered, red -> green -> refactor)

1. **Shared compare helper + plan decoder skeleton.**
   - Test first (`src/fuzz_reentrancy/mod.rs` `#[cfg(test)]`): `decode_is_total` (empty input, 1 byte, 1 KiB of 0xFF, random-looking bytes all decode without panic and yield 1..=4 scenarios); `decode_is_deterministic`.
   - Code: byte cursor (`next_mod(n)`), `Plan`, `Scenario`, enums with `ALL` consts in `ops.rs`; refactor `src/lib.rs`: extract `execute_and_compare(src)`, add `mod fuzz_reentrancy;` and `pub fn fuzz_reentrancy_bytes`. Existing `fuzz_execute_bytes` tests stay green.
2. **Render one op x one host end to end (vertical slice: `Array.prototype.sort` x comparator `Callback` x `mutate target`).**
   - Test: `render_parses_and_runs`: output parses (`parse_script` is private to lib; use `run_script_observed(src, budget, true)` and assert outcome is `Completed`, not `ParseError`/`BudgetExhausted`) and `fuzz_execute_signatures(src)` is `Some` (not skipped, comparable) and `execution_divergence` is `None`.
   - Code: prelude + scenario renderer + the one fragment.
3. **Fill in all Ops, Hosts, Actions.** Tests: `every_action_and_shape_renders_runnable_program` runs every `Action::ALL` and `Shape::ALL` variant at least once against one compatible op/host pair with the same assertions (a variant that always exhausts the budget or trips a marker would otherwise silently make a slice of inputs non-comparable); and `every_op_host_pair_renders_runnable_program` iterates `Op::ALL x Host::ALL` (skipping incompatible pairs through an explicit `Op::supports(Host)` table, asserted non-empty per op) with a fixed action/step/shape grid; each rendered program must parse, complete without budget exhaustion, be comparable, and show no divergence. Any pair that exposes a real engine bug during this slice: file an issue, narrow *that fragment* (not the engine) and cite the issue in a code comment-free way (PR body only, per repo comment-sparsity rule).
4. **Seeds.** `scripts/gen-fuzz-reentrancy-corpus.py` writes `fuzz/corpus/reentrancy/*.bin` from a table mirroring the Rust enum order; names encode the pairing (`sort-callback-mutate.bin`). Test (`src/fuzz_reentrancy/mod.rs`): `seeds_cover_every_op_and_host` reads `fuzz/corpus/reentrancy` via `env!("CARGO_MANIFEST_DIR")`, decodes each seed, and asserts every `Op::ALL` and `Host::ALL` variant appears in at least one seed (catches enum-order drift between the Python table and Rust) and that each seed's rendered program is comparable and divergence-free. Run with `uv run python scripts/gen-fuzz-reentrancy-corpus.py`. The generator puts a `0x00` byte in every seed (a valid value for every field: header reads as scenario count 1 via a trailing pad, or a field whose reduced value 0 is valid), so git/identify classify the file as binary and the file-mutating pre-commit hooks (`end-of-file-fixer`, `trailing-whitespace`) cannot rewrite it; the seed-coverage test would catch a rewritten seed.
5. **Fuzz crate + ClusterFuzzLite wiring.** `fuzz/fuzz_targets/reentrancy.rs`, `fuzz/Cargo.toml` bin, `.clusterfuzzlite/build.sh` (`TARGETS`, `reentrancy.options` with `max_len = 64`; the zip line already picks up `fuzz/corpus/reentrancy/*`). Verify: `cargo +nightly fuzz build --debug-assertions reentrancy` and `cargo +nightly fuzz run reentrancy -- -runs=0 -max_len=64 fuzz/corpus/reentrancy` (replay must be clean). Update comments in the three workflows.
6. **GC-stress hook (optional, only if slices 1-5 are green and the diff stays small).** Test: `gc_stress_run_matches_unstressed` in `src/fuzz_reentrancy/mod.rs`: a seed rendered and run with stress period 3 yields the same `RunSignature` as unstressed. Code: drop `#[cfg(test)]` on `GcPacer::set_stress_period` (`src/interpreter/gc.rs:~120`), add `Interpreter::set_gc_stress_period(u32)` in `src/interpreter/fuzz_host.rs`, thread a `gc_stress: u32` through `run_script_observed`'s call chain (a private `RunOptions`/extra argument; keep the public `run_script_observed(src, budget, bytecode)` signature as a wrapper), and let the plan's first byte pick period 0, 2, 5 or 11. If threading it perturbs `execute`'s behavior or exceeds the file token budget, drop this slice and record it as a follow-up in the ADR.
7. **Docs.** New ADR (decision: structure-aware plan -> JS template, reuse `execute` oracle, no `arbitrary`; seed policy and the generator script; ClusterFuzzLite wiring; defines **Re-entrancy Plan**; consequences: shared 120 s, latent-bug PR failure risk; follow-ups: grammar/dictionary, module inputs, per-action coverage report, `$262.gc` and weak-ref ops under a non-compare mode). `README.md` Fuzzing section (target list + two CI table rows), `.claude/skills/fuzzing/SKILL.md` (target list, replay and mutation commands with `-max_len=64`), cross-link from the `execute` ADR's follow-ups line if appropriate.
8. **Hunt and file.** Local session at CI flags: `cargo +nightly fuzz run --debug-assertions reentrancy -- -max_total_time=300 -max_len=64`. For each distinct crash/divergence: minimize with `cargo +nightly fuzz tmin`, file with `gh issue create` (title `bug(<area>): <symptom> (found by reentrancy fuzz target)`, link #1177/#1168, include the minimized testcase and the rendered JS), and list the issue numbers in the PR body. Do not fix engine bugs in this PR. If nothing is found in 5 minutes, say so in the PR body.

## Test surface

- `cargo test` (lib unit tests: decoder totality/determinism, per-pair render runnable, seed coverage, divergence-free; optional GC-stress equivalence).
- `./scripts/lint.sh` and the repo hooks (`./scripts/ensure-hooks.sh` first); token budget check on new files.
- Fuzz gates: replay `cargo +nightly fuzz run reentrancy -- -runs=0 -max_len=64 fuzz/corpus/reentrancy`; also replay `execute` and `parse` (`-runs=0`) to confirm the lib.rs refactor changed nothing; short mutation run (slice 8).
- test262: not exercised (no semantics change). Run `uv run python scripts/run-test262.py test262/test/built-ins/Proxy/` and `.../built-ins/Array/prototype/sort/` only as a sanity check if slice 6 touches `gc.rs`. No `test262-extra/` tests: nothing spec-visible changes. No `test262-pass.txt` change.

## Regression risk

- `src/lib.rs` refactor (`execute_and_compare` extraction) touches `fuzz_execute_bytes`, which `execute` and ClusterFuzzLite use; guarded by the `execute` replay and existing `lib_tests`.
- Slice 6 touches `GcPacer` (`cfg(test)` widening) and the `run_script_observed` path; GC stress is off unless the fuzz plan enables it, so the engine, `release` binary and test262 baseline are unaffected. Dead-code lint risk if the hook is unused: only land it together with its caller.
- A newly found latent engine bug can fail the blocking `ClusterFuzzLite PR` check on this PR (no `main` build of the target to compare against).
- The shared 120 s PR budget now splits across three targets, reducing `parse`/`execute` mutation time per PR; batch run (30 min) is unchanged in total.
- No change to the property MOP, tree-walker hot paths, bytecode fast path, `ObjectKind` matches or Node-compat harnesses.

## Out of scope

- Fixing any engine bug the target finds (each gets its own issue/PR).
- Raising `fuzz-seconds`, making the ClusterFuzzLite check required, corpus pruning/coverage modes, a `storage-repo`.
- A grammar-aware mutator/dictionary for `execute`, module inputs, counting native loops in the step budget.
- A node-based oracle for re-entrancy (stays in `differential`).
- Refactoring `src/lib.rs` beyond the one helper extraction; no CONTEXT.md edit (at size limit).

## Risks

- Generated fragments could be too uniform to reach interesting states: mitigated by `step`/`shape`/`action` axes and multi-scenario plans (up to 4 scenarios share state).
- Budget exhaustion makes a pair non-comparable: unit tests assert comparability for every pair, and the nesting/call caps keep programs well under 20,000 steps; native loops are unbudgeted (see step-budget ADR "Not covered") so a timeout artifact is still a legitimate finding.
- Timeouts are findings only when the program is bounded by construction (small-literal table rule); otherwise they are harness artifacts.
- Python seed table can drift from Rust enum order: `seeds_cover_every_op_and_host` fails loudly.
