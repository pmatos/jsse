# Deepening backlog

Persistent candidate memory for the `pm-deepen` architecture routine. Statuses:
`proposed` (eligible, not started), `in-flight` (branch+PR exist), `landed` (merged),
`dropped` (hard filter — reversible), `rejected` (human declined / recurring bail — human-only reopen).
Never delete rows; they are the memory that stops re-surfacing the same work.

## arraybuffer-receiver-guard

- **Status**: proposed
- **Score**: 24/25 (leverage 5, locality 4, blast radius 1, heat 5)
- **Files**: ~1 estimated — `src/interpreter/builtins/typedarray.rs`
- **Modules**: `src/interpreter/builtins/typedarray.rs`
- **Summary**: Collapse the 5 ArrayBuffer getters' inline `enum Probe` borrow-escape prologues + 3 SharedArrayBuffer getters behind snapshot-returning receiver guards (`require_array_buffer` / `require_shared_array_buffer`), mirroring the landed `validate_typed_array` (#543). Guard returns `is_detached` in the snapshot rather than throwing (getters return 0 on detached). DataView getters + borrow-holding methods deferred to `dataview-receiver-guard`.
- **First seen**: 2026-09-02
- **PR**: (this run)

## validate-typed-array

- **Status**: landed
- **Score**: 24/25 (leverage 5, locality 4, blast radius 1, heat 5)
- **Files**: ~1 estimated — `src/interpreter/builtins/typedarray.rs`
- **Modules**: `src/interpreter/builtins/typedarray.rs`
- **Summary**: Collapse open-coded TypedArray receiver-validation prologues (brand check + detached/out-of-bounds check + clone + doubled `not a TypedArray` throw) behind one `validate_typed_array` seam, mirroring the existing kind-gated `validate_uint8array`. Landed: 14 read-mode sites migrated; 3 (`slice`, `sort`, `toSorted`) kept — they hold the object borrow across their body.
- **First seen**: 2026-09-01
- **PR**: #543 (merged 2026-09-01)

## complete-state-machine-generator-ctor

- **Status**: proposed
- **Score**: 22/25 (leverage 5, locality 4, blast radius 1, heat 3)
- **Files**: ~2 estimated — `src/interpreter/eval/generator_runtime.rs`, `src/interpreter/types.rs`
- **Modules**: `src/interpreter/eval/generator_runtime.rs`
- **Summary**: Collapse ~87 byte-identical inlined "completed state-machine generator" 10-field struct literals into `completed_state_machine_generator` / `…_async_generator` constructors. Runner-up candidate on 2026-09-02; natural next firing. Friction re-verified present.

## completion-into-result

- **Status**: proposed
- **Score**: 21/25 (leverage 4, locality 3, blast radius 1, heat 5)
- **Files**: ~2 estimated — `src/interpreter/builtins/iterators.rs`, `src/interpreter/types.rs`
- **Modules**: `src/interpreter/builtins/iterators.rs`
- **Summary**: Add `Completion::into_result(self) -> Result<JsValue, JsValue>` and collapse the ~37 hand-rolled `match Completion { Normal(v)=>v, Throw(e)=>return Err(e), _=>… }` adapter heads in the Result-returning iterator abstract-operation helpers to `.into_result()?`; the fabricated `_ =>` error arms become removable dead code. First seen 2026-09-02.

## completion-unwrap-macro

- **Status**: proposed
- **Score**: 21/25 (leverage 4, locality 3, blast radius 1, heat 5)
- **Files**: ~2 estimated — `src/interpreter/types.rs`, `src/interpreter/builtins/typedarray.rs`
- **Modules**: `src/interpreter/types.rs`
- **Summary**: A `try_completion!(expr)` macro that binds a `Completion::Normal` value and propagates any abrupt Completion, for the Completion-returning natives (typedarray 27, builtins/mod 22, eval 11, string 11, …). Distinct from `completion-into-result` (Result vs Completion return context). Scope the first step to one adopter to stay blast-radius 1. First seen 2026-09-02.

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
- **Summary**: Add `this_weak_map` / `this_weak_set` sibling helpers to collapse 9 inconsistently hand-rolled WeakMap/WeakSet receiver-unwrap dances, mirroring the existing `this_map` / `this_set`. (2026-09-02 re-check: `this_map`/`this_set` still present; the WeakMap/WeakSet brand strings differ from `not a WeakMap` — confirm the exact error wording per site before migrating.)

## object-this-coercion

- **Status**: proposed
- **Score**: 20/25 (leverage 4, locality 3, blast radius 1, heat 4)
- **Files**: ~1 estimated — `src/interpreter/builtins/mod.rs`
- **Modules**: `src/interpreter/builtins/mod.rs`
- **Summary**: A `require_this_object(this) -> Result<u64, Completion>` ToObject prologue collapsing ~10 open-coded `match to_object(this_val) { Normal(v)=>v, other=>return other }` + object-id-unwrap dances in Object.prototype methods. A coercion prologue (ToObject can run user code), distinct from the `object-id-of` round-trip. First seen 2026-09-02.

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
- **Summary**: Delete `collect_pattern_bound_names` (a near-byte-identical copy of `ast::Pattern::bound_names`) from the for-of TDZ path and reuse the existing method. Low count (1 straggler) but hot file. (2026-09-02 re-check: 5 references still present.)

## dataview-receiver-guard

- **Status**: proposed
- **Score**: 18/25 (leverage 4, locality 3, blast radius 1, heat 5)
- **Files**: ~1 estimated — `src/interpreter/builtins/typedarray.rs`
- **Modules**: `src/interpreter/builtins/typedarray.rs`
- **Summary**: Follow-up to `arraybuffer-receiver-guard` covering the DataView getters (`buffer`/`byteOffset`/`byteLength`) and the borrow-holding ArrayBuffer methods. Harder than the getter family: DataView getters *throw* on IsViewOutOfBounds (subsumes detached), compute a per-getter OOB condition, and read through to the underlying buffer (cross-object), and the methods re-probe detached after the species constructor runs user code. First seen 2026-09-02.

## regexp-last-index-accessor

- **Status**: proposed
- **Score**: 18/25 (leverage 3, locality 3, blast radius 1, heat 4)
- **Files**: ~1 estimated — `src/interpreter/builtins/regexp.rs`
- **Modules**: `src/interpreter/builtins/regexp.rs`
- **Summary**: `get_last_index(interp, rx) -> Result<usize, Completion>` / `set_last_index(interp, rx, v)` for the 5 read+ToLength and 3 `spec_set(...,"lastIndex",...)` sites re-spelling the `Get(R,"lastIndex")`→`ToLength` / `Set(R,"lastIndex",v,true)` dance inline. First seen 2026-09-02.

## object-id-of

- **Status**: dropped
- **Score**: 13/25 (leverage 2, locality 2, blast radius 3, heat 4)
- **Files**: ~3+ estimated — `src/interpreter/eval.rs`, `src/interpreter/eval/generator_runtime.rs`, `src/interpreter/exec.rs`
- **Modules**: `src/interpreter/eval.rs`
- **Summary**: ~130 circular `.as_object_id().map(|id| JsObject { id })` round-trips that rebuild a `JsObject` only to read `.id` back out. A `/simplify`-class cleanup, not a deepening — recorded so future runs don't re-derive it as a deep-module candidate.
- **First seen**: 2026-09-01
- **Reason**: Leverage 2 — `/simplify`-class round-trip cleanup, fails the deepening bar (complexity renamed, not concentrated behind a seam).

## typedarray-shared-equality

- **Status**: dropped
- **Score**: n/a (leverage 2 — `/simplify`-class, not a deepening)
- **Files**: ~1 estimated — `src/interpreter/builtins/typedarray.rs`
- **Modules**: `src/interpreter/builtins/typedarray.rs`
- **Summary**: `typedarray.rs` re-implements private `same_value_zero` / `strict_eq` that already exist in `helpers.rs`. Deduping moves code rather than concentrating behaviour behind a new seam.
- **First seen**: 2026-09-02
- **Reason**: Leverage 2 — missed-reuse dedup, not a deep-module candidate. Caveat: the private `strict_eq` compares strings via `to_rust_string()`; a genuine semantic divergence must be confirmed first, and if real is a bug report rather than a dedup.

## unify-generator-async-drivers

- **Status**: dropped
- **Score**: n/a (blast radius 5 — too large for one unattended PR)
- **Files**: 40+ estimated — `src/interpreter/eval/generator_runtime.rs`
- **Modules**: `src/interpreter/eval/generator_runtime.rs`
- **Summary**: `generator_next_state_machine_impl` and `async_generator_next_state_machine_impl` are largely parallel ~1580/~3050-line state-machine interpreters. Unifying them is a deep structural refactor for a human to schedule.
- **First seen**: 2026-09-01
- **Reason**: Blast radius 5 — human-scheduled. Land the generator-constructor and settle-tail candidates first to shrink both drivers.
