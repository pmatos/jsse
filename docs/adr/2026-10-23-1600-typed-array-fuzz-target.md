# `typed_array` fuzz target: structured buffer, view and Atomics sequences

Status: accepted (#1176); builds on the `execute` target (ADR 2026-10-23-1400).

## Context

Raw bytes rarely build "make a resizable buffer, view it, shrink it, then call
a method whose argument coercion detaches it". Those sequences reach the
engine's `unsafe` buffer code (`SharedBufferInner::with_atomic_ptr` in
`src/interpreter/types.rs`, `src/interpreter/builtins/atomics.rs`) and the
length-tracking and out-of-bounds logic of `builtins/typedarray.rs`
(ECMA-262 25.1 ArrayBuffer, 25.2 SharedArrayBuffer, 23.2 TypedArray,
25.3 DataView, 25.4 Atomics). `execute` only gets there by luck.

## Decision

- `fuzz/fuzz_targets/typed_array.rs` calls `jsse_fuzz::typed_array::fuzz_typed_array_bytes`.
  The generator lives in the `fuzz` crate, not in `jsse`: it is harness-only
  and `jsse` already exports `run_on_engine_stack`, `run_script_observed` and
  `execution_divergence`.
- Input format: a hand-rolled byte cursor (`ops.rs`) that yields `0` past the
  end, so decoding never fails and empty input is an empty program. Every
  field is reduced into a small table. At most 64 ops. No `arbitrary` crate,
  so seeds can be written by hand and `fuzz/Cargo.lock` does not move.
- Ops: create a (resizable, shared) buffer, create a view of any of the 12
  typed array kinds or a `DataView` (length-tracking or fixed), `resize` /
  `grow`, `transfer` / `transferToFixedLength`, detach (`$262.detachArrayBuffer`),
  indexed read and write (repeated, so inline caches warm before a later
  resize), 30 typed array / `DataView` methods and 12 `Atomics` operations.
- Hostile arguments: any numeric argument is either plain or an object whose
  `valueOf` resizes, detaches or transfers a buffer slot before returning the
  value. Comparator and mapper callbacks do the same when called. This is the
  shape that shrinks or detaches a buffer in the middle of a method.
- Oracle: as in `execute`, a panic, abort or sanitizer report, or an
  Execution Divergence between the bytecode VM and the tree-walker (a panic
  starting `execution divergence`). The compared signature is the one
  `execute` uses (outcome kind, completion value, thrown `name`, captured
  `print` output). The epilogue prints every buffer's and view's length, max
  length, detached flag and a byte checksum, so state differences are visible.
- `Atomics` and `SharedArrayBuffer` ARE compared here, unlike in `execute`
  (which skips comparison only because it cannot rule out agents). The
  generator never emits `agent`, `waitAsync` or `sleep`, so every
  shared-memory operation is single-threaded and deterministic.
  `Atomics.wait` is emitted only with the literal timeout `0`, never through
  the hostile-argument encoding (a hostile `valueOf` returning NaN would make
  the timeout infinite). A unit test asserts none of those identifiers appears.
- Buffer sizes come from a table (0 to 64 bytes). Huge lengths surface as
  allocation-limit noise rather than typed array bugs.
- Step budget: 20,000, as for `execute`. Budget exhaustion skips the comparison.
- Seeds: `seeds.rs` defines 14 named op sequences; the ignored test
  `regenerate_seed_corpus` writes them to `fuzz/corpus/typed_array/NN-<name>.bin`.
  `committed_seeds_match_definitions_and_run_clean` fails if the committed files
  are stale, a seed does not round-trip, the two modes disagree, a run does not
  complete, or any op kind, method, `Atomics` op, view kind or hostile action
  is uncovered. Run it with `cargo test --manifest-path fuzz/Cargo.toml --lib`
  (the fuzz crate's tests do not run in `ci.yml`).
- ClusterFuzzLite: `.clusterfuzzlite/build.sh` builds `typed_array` with
  `max_len = 768` (64 ops of at most 12 bytes). No workflow change.
- Triage: `JSSE_FUZZ_DUMP_SCRIPT=1` prints the generated script to stderr.

## Consequences

- A latent typed array or `Atomics` bug can fail the `ClusterFuzzLite PR` check
  on an unrelated PR (same consequence as `execute`). Seeds replay clean and a
  5-minute local run at the CI flags found nothing before this landed.
- Throughput is about 7-10 executions/sec under ASan with debug assertions,
  dominated by creating two interpreters per input. The PR job's fixed 120 s is
  now split across three targets, about 40 s each. Raising `fuzz-seconds` is a
  follow-up for the maintainer; the daily batch job is the main depth source.
- The bytecode compiler has no typed-array-specific fast path, so the
  bytecode-vs-tree-walker comparison has limited bite here. The main value is
  panic, ASan and debug-assertion coverage; the comparison is free and catches
  VM-side element-access or inline-cache staleness.
- Rejected: the `arbitrary` crate (harder to hand-assemble seeds, new
  dependency); a Python seed generator (the Rust encoder is the single source
  of truth and the round-trip is tested).
- Follow-ups: huge lengths and `RangeError` paths, `$262.agent` multi-thread
  targets, non-zero `Atomics.wait` timeouts, `Atomics.waitAsync`, a dictionary.
