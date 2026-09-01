# Deepening backlog

Persistent candidate memory for the `pm-deepen` architecture routine. Statuses:
`proposed` (eligible, not started), `in-flight` (branch+PR exist), `landed` (merged),
`dropped` (hard filter — reversible), `rejected` (human declined / recurring bail — human-only reopen).
Never delete rows; they are the memory that stops re-surfacing the same work.

## validate-typed-array

- **Status**: in-flight
- **Score**: 24/25 (leverage 5, locality 4, blast radius 1, heat 5)
- **Files**: ~1 estimated — `src/interpreter/builtins/typedarray.rs`
- **Modules**: `src/interpreter/builtins/typedarray.rs`
- **Summary**: Collapse open-coded TypedArray receiver-validation prologues (brand check + detached/out-of-bounds check + clone + doubled `not a TypedArray` throw) behind one `validate_typed_array` seam, mirroring the existing kind-gated `validate_uint8array`. Landed: 14 read-mode sites migrated; 3 (`slice`, `sort`, `toSorted`) kept — they hold the object borrow across their body.
- **First seen**: 2026-09-01
- **PR**: #543

## complete-state-machine-generator-ctor

- **Status**: proposed
- **Score**: 22/25 (leverage 5, locality 4, blast radius 1, heat 3)
- **Files**: ~2 estimated — `src/interpreter/eval/generator_runtime.rs`, `src/interpreter/types.rs`
- **Modules**: `src/interpreter/eval/generator_runtime.rs`
- **Summary**: Collapse ~87 byte-identical inlined "completed state-machine generator" 10-field struct literals into `completed_state_machine_generator` / `…_async_generator` constructors. Runner-up candidate this run; natural next firing.

## settle-and-return-tail

- **Status**: proposed
- **Score**: 20/25 (leverage 4, locality 4, blast radius 1, heat 3)
- **Files**: ~1 estimated — `src/interpreter/eval/generator_runtime.rs`
- **Modules**: `src/interpreter/eval/generator_runtime.rs`
- **Summary**: Extract `settle_and_return(settle_fn, arg, promise)` for the ~47 "call settle fn + drain microtasks + return promise" async-generator exit tails. Sequence after complete-state-machine-generator-ctor, which shrinks the same driver.

## this-weak-map-set

- **Status**: proposed
- **Score**: 20/25 (leverage 4, locality 4, blast radius 1, heat 3)
- **Files**: ~1 estimated — `src/interpreter/builtins/collections.rs`
- **Modules**: `src/interpreter/builtins/collections.rs`
- **Summary**: Add `this_weak_map` / `this_weak_set` sibling helpers to collapse 9 inconsistently hand-rolled WeakMap/WeakSet receiver-unwrap dances, mirroring the existing `this_map` / `this_set`.

## generator-entry-guard

- **Status**: proposed
- **Score**: 19/25 (leverage 4, locality 3, blast radius 1, heat 3)
- **Files**: ~1 estimated — `src/interpreter/eval/generator_runtime.rs`
- **Modules**: `src/interpreter/eval/generator_runtime.rs`
- **Summary**: Fold ~11 duplicated generator-entry "called on non-object" TypeError pairs into a `require_generator_object` guard. Ties into object-id-of.

## pattern-bound-names-walker

- **Status**: proposed
- **Score**: 19/25 (leverage 3, locality 3, blast radius 1, heat 5)
- **Files**: ~1 estimated — `src/interpreter/exec.rs`
- **Modules**: `src/interpreter/exec.rs`
- **Summary**: Delete `collect_pattern_bound_names` (a near-byte-identical copy of `ast::Pattern::bound_names`) from the for-of TDZ path and reuse the existing method. Low count (1 straggler) but hot file.

## object-id-of

- **Status**: proposed
- **Score**: 13/25 (leverage 2, locality 2, blast radius 3, heat 4)
- **Files**: ~3+ estimated — `src/interpreter/eval.rs`, `src/interpreter/eval/generator_runtime.rs`, `src/interpreter/exec.rs`
- **Modules**: `src/interpreter/eval.rs`
- **Summary**: ~130 circular `.as_object_id().map(|id| JsObject { id })` round-trips that rebuild a `JsObject` only to read `.id` back out. A `/simplify`-class cleanup, not a deepening — recorded so future runs don't re-derive it as a deep-module candidate.

## unify-generator-async-drivers

- **Status**: dropped
- **Score**: n/a (blast radius 5 — too large for one unattended PR)
- **Files**: 40+ estimated — `src/interpreter/eval/generator_runtime.rs`
- **Modules**: `src/interpreter/eval/generator_runtime.rs`
- **Summary**: `generator_next_state_machine_impl` and `async_generator_next_state_machine_impl` are largely parallel ~1580/~3050-line state-machine interpreters. Unifying them is a deep structural refactor for a human to schedule.
- **First seen**: 2026-09-01
- **Reason**: Blast radius 5 — human-scheduled. Land the generator-constructor and settle-tail candidates first to shrink both drivers.
