# Plan: `typed_array` fuzz target (ArrayBuffer, typed arrays, DataView, Atomics)

Issue #1176, parent #1168. Classification: Medium (fuzz tooling + CI + docs, no engine change).

## Problem restated
`parse` and `execute` fuzz the engine with raw bytes. Raw bytes rarely build a valid chain of "make a resizable buffer, view it, shrink it, call a method whose argument coercion detaches it". These are exactly the sequences that reach the engine's `unsafe` buffer code (`SharedBufferInner::with_atomic_ptr` in `src/interpreter/types.rs:140`, `src/interpreter/builtins/atomics.rs`) and the length-tracking/out-of-bounds logic in `src/interpreter/builtins/typedarray.rs`. This issue adds a structure-aware third in-process target. It decodes the fuzz bytes into a bounded sequence of buffer/view/Atomics operations, renders them as a JS script, and runs that script on the bytecode VM and the tree-walker under the step budget, with ASan in ClusterFuzzLite and a committed seed corpus. Findings are filed as separate issues, never fixed in this PR.

## Spec basis
N/A: no JavaScript behavior change. This adds fuzz tooling (`fuzz/`, `.clusterfuzzlite/`, docs). The operations it exercises are governed by ECMA-262 §25.1 ArrayBuffer Objects (incl. resizable `maxByteLength`, `resize`, `transfer`, `transferToFixedLength`, detach), §25.2 SharedArrayBuffer Objects (growable, `grow`), §23.2 TypedArray Objects (length-tracking, `IsTypedArrayOutOfBounds`, `TypedArrayLength`, `ValidateTypedArray`), §25.3 DataView Objects, §25.4 The Atomics Object. Used only to decide what is a bug when triaging a finding.

## Assumptions
- Input format: hand-rolled byte cursor, no `arbitrary` crate (best guess). Seeds stay hand-assemblable, and no new dependency means `fuzz/Cargo.lock` does not move.
- The generator lives in the `fuzz` crate (`jsse_fuzz`), not in `jsse`: it is harness-only (same placement rationale as `differential` support in `fuzz/src/lib.rs`) and `jsse` already exports everything needed (`run_on_engine_stack` `src/lib.rs:41`, `run_script_observed` `:209`, `execution_divergence` `:250`).
- Oracle: same as `execute` (ADR `2026-10-23-1400`): panic/abort/ASan report, or bytecode-vs-tree-walker divergence in outcome kind, completion value, thrown `name` and captured `print` output. Unlike `execute`, `Atomics`/`SharedArrayBuffer` runs ARE compared: the generator never emits `agent`, `waitAsync`, `sleep` or `$262.agent`, so all shared-memory ops are single-threaded and deterministic. `execute` skips comparison for those identifiers only because it cannot rule agents out.
- `Atomics.wait` is emitted only with the literal timeout `0`, which never goes through the `Plain`/`Hostile` arg encoding (a hostile `valueOf` returning NaN/undefined would make the timeout +Infinity, per Atomics.wait DoWait). `run_script_observed` builds `Interpreter::new()` and `can_block` is only set by `src/cli.rs:147` (and tests), so in the fuzz target it is the default (`false`; implementer verifies): `Atomics.wait` takes the `!interp.can_block` TypeError branch (`atomics.rs:367`) after shared-array validation. Either way no wall-clock sleep. `Atomics.waitAsync` is never emitted; the render unit test asserts neither appears.
- Buffer sizes come from a small table (0, 1, 3, 8, 16, 24, 64); no huge allocations (they surface as OOM/allocation-limit noise, not typed-array bugs). Oversized lengths/RangeError paths are a follow-up.
- Detach uses `$262.detachArrayBuffer` (present in `src/interpreter/mod.rs`) and `transfer()`.
- Three targets now share the PR job's fixed 120 s. Left unchanged, so each target gets about 40 s. Raising it is a follow-up for the maintainer; documented in the ADR.
- Seeds are binary files written from a single encoding source: a Rust `encode(&[Op]) -> Vec<u8>` (inverse of `decode`) in the fuzz crate, round-trip tested. An `#[ignore]`d test `regenerate_seed_corpus` writes `fuzz/corpus/typed_array/*.bin` from named `Vec<Op>` seed definitions; a normal test asserts the committed files equal the encoded definitions, decode, cover every op, run cleanly and agree on both modes. No Python script (fuzz-crate tests do not run in `ci.yml`, so the guard is local/`cargo test --manifest-path fuzz/Cargo.toml --lib` only; noted as a risk).
- Bytecode-vs-tree-walker divergence has limited bite here: `src/interpreter/bytecode/` has no typed-array-specific fast path (grep found only `bytecode/tests.rs`), so element access and methods share `property.rs`/`builtins/typedarray.rs` in both modes. The target's main value is panic/ASan/debug-assertion coverage; the comparison is kept because it is free and catches VM-side element-access or inline-cache staleness (hence `Access`/`Store` loops below).

## Key Files
| File | Role | Lines of Interest |
|------|------|-------------------|
| `fuzz/fuzz_targets/execute.rs` | Template for the new target (3-line `fuzz_target!`) | whole file |
| `fuzz/Cargo.toml` | Add `[[bin]] typed_array` | `[[bin]]` blocks |
| `fuzz/src/lib.rs` | Crate root; add `pub mod typed_array;` (currently differential helpers) | top |
| `src/lib.rs` | `run_on_engine_stack`, `run_script_observed`, `execution_divergence` (all `pub`; reuse as is) | 41, 209, 250 |
| `.clusterfuzzlite/build.sh` | `TARGETS=(parse execute)`; seed zip loop; `execute.options` heredoc | 5, 10-18 |
| `docs/adr/2026-10-23-1400-execute-fuzz-target.md` | Oracle, determinism, skip-list rationale to cite | whole file |
| `README.md` | "Fuzzing" section and CI table | 173-212 |
| `.claude/skills/fuzzing/SKILL.md` | Target list, replay/run/triage commands | description, intro, steps 2-3, CI list |
| `scripts/gen-fuzz-execute-corpus.py` | Pattern for the seed generator | whole file |
| `.pre-commit-config.yaml` | `fuzz/corpus/` excluded from hooks; every staged file must be <= 5,000 tokens (bytes/4 = 20 KB) | exclude, token hook |

## Steps

### 1. Op model and byte decoder [new] `fuzz/src/typed_array/ops.rs`
- Cursor over `&[u8]` that returns `0` past the end (never fails; empty input = empty program). Cap at `MAX_OPS = 64`; `max_len` in step 6 is sized from it (about 8 bytes per op).
- Also `encode(&[Op]) -> Vec<u8>` (inverse of decode, used by seeds).
- `enum Op` (one opcode byte `% N`, operands are the following bytes, reduced into tables):
  - `NewBuffer{slot, len, max: Option, shared}`: `new ArrayBuffer(len, {maxByteLength})`, and `new SharedArrayBuffer(len, {maxByteLength})` when `shared`.
  - `NewView{slot, kind, buf, offset, len: Auto|N}`: typed array of one of the 12 kinds in `TypedArrayKind` (`typedarray.rs:2784`; Int8..Float64, BigInt64/BigUint64, Float16, Uint8Clamped), or `DataView`. `Auto` = length-tracking.
  - `Resize{buf, len}` (`resize`/`grow`), `Transfer{buf, len: Same|Fixed(n)|ToFixed}`, `Detach{buf}`.
  - `Access{view, idx, reps}`/`Store{view, idx, val, reps}` including out-of-range and negative indices; `reps` > 1 renders a short loop so inline caches/fast paths warm before a later `Resize`/`Detach` of the same view.
  - `Method{view, name, args}` where `name` is one of: `fill`, `set` (array and typed-array source), `copyWithin`, `slice`, `subarray`, `sort` (comparator arg), `reverse`, `indexOf`, `includes`, `join`, `at`, `with`, `toReversed`, `toSorted`, `values`/`entries` drained, `map`/`forEach`/`every` (callback arg), `DataView` get/set pairs.
  - `Atomic{view, op, idx, a, b}` with `op` in add/sub/and/or/xor/exchange/compareExchange/load/store/isLockFree/notify/wait(timeout 0).
- Each numeric arg is `Plain(v)` or `Hostile(action, buf)`: an object whose `valueOf` runs `Resize`/`Detach`/`Transfer` on a buffer slot and then returns `v`. A callback arg does the same inside the callback. This is the high-value shape: coercion, comparator or callback side effects that shrink or detach the buffer mid-method.
- Keep this file under 20 KB (token hook); split `Op` tables into `fuzz/src/typed_array/tables.rs` [new] if needed.

### 2. Script renderer [new] `fuzz/src/typed_array/script.rs`
- `render(&[Op]) -> String`: preamble declares `B` (buffer slots) and `V` (view slots) arrays and a `log(i, f)` helper that runs `f` in `try/catch` and `print`s `i`, the result summary (numbers, `typeof`, array contents via `Array.from`) or `e.name`.
- Epilogue prints a state dump per slot: buffer `byteLength`, `maxByteLength`, `resizable`/`growable`, `detached`, a byte checksum; view `length`, `byteLength`, `byteOffset`, so tracking/OOB state differences between modes are visible in compared output.
- Never emits identifiers `agent`, `waitAsync`, `sleep`; asserted in a unit test with a local identifier-boundary matcher (reimplemented: `contains_ident` in `src/lib.rs` is private).
- Unit tests (TDD, red first): empty input renders a parseable script; every op variant renders text for which `jsse::run_script_observed(...).outcome != SigOutcome::ParseError` and runs without panic; hostile arg actually shrinks the buffer before the method continues.

### 3. Target entry point [new] `fuzz/src/typed_array/mod.rs`
- `pub fn fuzz_typed_array_bytes(data: &[u8])`: decode, render, optionally print script to stderr when `JSSE_FUZZ_DUMP_SCRIPT` is set, then inside `jsse::run_on_engine_stack` run `run_script_observed(src, BUDGET, true)` and `(src, BUDGET, false)` and `panic!("execution divergence ...")` with the script text when `execution_divergence` returns `Some`. Budget skip is already handled by `execution_divergence`. `BUDGET = 20_000` (same as `execute`, local `const`).
- Test: a fixed byte vector runs through `fuzz_typed_array_bytes` to completion (no panic), and both-mode signatures from `run_script_observed` are equal.

### 4. Wire the fuzz crate and target
- `fuzz/src/lib.rs`: `pub mod typed_array;`.
- `fuzz/fuzz_targets/typed_array.rs` [new]: `#![no_main]`, `fuzz_target!(|data: &[u8]| jsse_fuzz::typed_array::fuzz_typed_array_bytes(data));`.
- `fuzz/Cargo.toml`: `[[bin]] name = "typed_array" path = "fuzz_targets/typed_array.rs" test/doc/bench = false`.

### 5. Seed corpus
- Seed definitions as named `Vec<Op>` in `fuzz/src/typed_array/seeds.rs` [new]: about 15, one per theme: resizable AB shrink under a length-tracking view; SAB grow; transfer then use of the old view; detach inside `fill`/`set`/`sort` comparator; `subarray` of an OOB view; hostile arg to `slice`; each Atomics op on Int32/BigInt64; `Atomics.wait` timeout 0; DataView on a resizable buffer; every typed array kind; warmed `Access` loop then `Resize`.
- Written to `fuzz/corpus/typed_array/NN-<name>.bin` by the `#[ignore]`d `regenerate_seed_corpus` test (`cargo test --manifest-path fuzz/Cargo.toml --lib -- --ignored regenerate_seed_corpus`).
- Test `committed_seeds_match_definitions_and_run_clean`: files equal `encode(def)`, every `Op` variant and every Atomics op appears, both modes agree, no run is `BudgetExhausted` (so the comparison really ran).

### 6. ClusterFuzzLite
- `.clusterfuzzlite/build.sh`: `TARGETS=(parse execute typed_array)`; add `typed_array.options` heredoc with `max_len = 512` (64 ops at about 8 bytes).
- No workflow YAML change: `clusterfuzzlite-{pr,build,batch}.yml` build every target in `build.sh` and zip `fuzz/corpus/$target`.
- Local verification (foreground, explicit timeouts; cap build parallelism with `CARGO_BUILD_JOBS=4`; `cargo fuzz --version`, else `cargo install cargo-fuzz --locked`, no sudo): `cargo +nightly fuzz build typed_array`; `cargo +nightly fuzz run typed_array -- -runs=0 fuzz/corpus/typed_array`; then a 60-90 s mutation run with `--debug-assertions`, at the CI flags, to confirm it starts clean (ASan is cargo-fuzz's default).

### 7. Docs
- New ADR `docs/adr/2026-10-23-1600-typed-array-fuzz-target.md` [new] (check the date prefix against `ls docs/adr`): decisions above (byte-cursor format, hostile args, determinism rules incl. `Atomics.wait` timeout 0 and no agents, why `Atomics`/SAB are compared here but not in `execute`, seed generator + coverage test, shared 120 s PR budget, deferred follow-ups: huge sizes, `agent`-based multi-thread targets, dictionary, `Atomics.waitAsync`).
- `README.md` Fuzzing section: "Three" -> "Four" targets, add `typed_array` run command and the two CI table rows.
- `.claude/skills/fuzzing/SKILL.md`: target list, replay command (`-runs=0 -max_len=512 fuzz/corpus/typed_array`), run command, CI list, and the `JSSE_FUZZ_DUMP_SCRIPT=1` triage tip.
- `CONTEXT.md` is at its size limit (per the execute ADR), so no new term there.

### 8. Acceptance: file findings
- Run the target locally for a time-boxed session. For every distinct engine bug, `gh issue create` a separate issue (minimal JS repro from `JSSE_FUZZ_DUMP_SCRIPT`, spec clause, `fuzz` label if present). Do not fix them here and do not match `node`. If nothing is found, record that in the PR description (duration, executions).

## Files to touch
New: `fuzz/fuzz_targets/typed_array.rs`, `fuzz/src/typed_array/mod.rs`, `fuzz/src/typed_array/ops.rs`, `fuzz/src/typed_array/script.rs`, `fuzz/src/typed_array/seeds.rs`, `fuzz/corpus/typed_array/*.bin`, `docs/adr/2026-10-23-1600-typed-array-fuzz-target.md`. Optional `fuzz/src/typed_array/tables.rs` if a file nears 20 KB.
Modified: `fuzz/Cargo.toml`, `fuzz/src/lib.rs` (`pub mod typed_array;`), `.clusterfuzzlite/build.sh`, `README.md`, `.claude/skills/fuzzing/SKILL.md`.
Not touched: `src/`, `spec/`, `test262/`, `test262-pass.txt`, `CONTEXT.md`, `.github/workflows/*`.

## TDD slices (ordered; test location is `fuzz/src/typed_array/*` `#[cfg(test)]`, run with `cargo test --manifest-path fuzz/Cargo.toml --lib`)
0. Preflight: confirm the existing `classify` tests in `fuzz/src/lib.rs` build and run under that command (libfuzzer-sys links). If not, move the tests to an `[[test]]` or a workspace-visible home before continuing.
1. Red: `decode(&[])` is empty and `decode` of arbitrary bytes never panics and caps at `MAX_OPS`; `decode(encode(ops)) == ops` for every variant. Green: `ops.rs`.
2. Red: `render` of empty and of each variant is a script with outcome `!= ParseError` that `run_script_observed` completes on both modes; hostile arg shrinks the buffer before the method continues (observed via printed `byteLength`); forbidden identifiers absent. Green: `script.rs`.
3. Red: `fuzz_typed_array_bytes(fixed bytes)` returns without panic. Green: `mod.rs` entry point + `JSSE_FUZZ_DUMP_SCRIPT`.
4. Red: `committed_seeds_match_definitions_and_run_clean`. Green: `seeds.rs` + run `regenerate_seed_corpus` + commit `.bin` files.
5. Wiring (no unit test; verified by commands): `[[bin]]`, target file, `build.sh`, then `cargo +nightly fuzz build/run`, replay and a time-boxed mutation run.
6. Docs: ADR, README, SKILL.md; `./scripts/lint.sh`.

## Testing
- Fuzz crate unit tests: `cargo test --manifest-path fuzz/Cargo.toml --lib` (add `--profile release-nolto` if debug is too slow). Covers decoder, renderer, seed coverage/cleanliness.
- Replay: `cargo +nightly fuzz run typed_array -- -runs=0 -max_len=512 fuzz/corpus/typed_array`. Also replay `parse`/`execute` to confirm `build.sh`/Cargo edits did not break them.
- Script sanity: run a few rendered scripts through `target/release/jsse` and `node` (reference only) to confirm they are valid JS.
- Fuzz-crate tests do not run in `ci.yml` (no `fuzz` reference there); the real CI coverage is the ClusterFuzzLite build + replay of the seed zip. Adding a CI test step is out of scope (listed below).
- Gates: `./scripts/lint.sh`, `./scripts/ensure-hooks.sh` before committing. Token hook: every new file <= 20 KB.
- test262: no engine change, so no test262 run is needed beyond a sanity check that `src/` is untouched (`git diff --stat -- src`). No `test262-extra/` test is added here. If a finding is later fixed, the fix PR adds its own test262-extra test.

## Regression risk
- Engine: none (no `src/` change). `test262-pass.txt` cannot move; no baseline update is planned.
- CI: the blocking `ClusterFuzzLite PR` check gains a target; a latent typed-array/Atomics bug can fail unrelated PRs (same consequence as ADR 2026-10-23-1400). Mitigations: seeds replay clean locally first, `main` build upload filters known crashes, and any finding is filed rather than fixed here. Risk of a long-running or OOM input: all sizes are tiny and the step budget applies, but natives that loop over a large `length` (e.g. `fill` on a big view) are not budgeted; the small size table keeps that bounded.
- Shared 120 s now split three ways: less depth per target per PR. Batch (30 min daily) is the main depth source.
- Builder image builds `fuzz/` with latest nightly; no new dependency, so no lockfile or `cargo-deny` impact.

## Out of scope
- Fixing any engine bug the target finds; each is its own issue.
- Multi-threaded `$262.agent` scenarios, `Atomics.waitAsync`, non-zero `Atomics.wait` timeouts.
- Huge-allocation / RangeError length coverage, dictionaries, grammar-aware mutator.
- Raising `fuzz-seconds`, corpus pruning, `storage-repo`, making the PR check required.
- Changing `execute`'s skip list or `src/lib.rs` APIs.
- Adding a `ci.yml` step that runs the fuzz-crate unit tests.
