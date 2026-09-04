# Deepening backlog

Persistent candidate memory for the `pm-deepen` architecture routine. Statuses:
`proposed` (eligible, not started), `in-flight` (branch+PR exist), `landed` (merged),
`dropped` (hard filter — reversible), `rejected` (human declined / recurring bail — human-only reopen).
Never delete rows; they are the memory that stops re-surfacing the same work.

## gc-root-scope-guard

- **Status**: in-flight
- **PR**: #595
- **Score**: 22/25 (leverage 5, locality 4, blast radius 3, heat 5)
- **Files (full candidate)**: ~9–12 — `src/interpreter/eval.rs` (primary), `src/interpreter/builtins/array.rs`, `src/interpreter/mod.rs` (seam home), + `iterators.rs`, `promise.rs`, `exec.rs`, `atomics.rs`, `typedarray.rs`, `property.rs`, `eval/literals.rs`, `bytecode/vm.rs`
- **Files (this firing's scope)**: ~2 estimated — `src/interpreter/mod.rs` (new `with_gc_root_scope` seam) + `src/interpreter/builtins/array.rs`
- **Modules**: `src/interpreter/mod.rs`, `src/interpreter/builtins/array.rs`
- **Summary**: Collapse the manual GC-root frame teardown epilogue behind a scope-guard combinator `with_gc_root_scope(|i| …)`, mirroring the in-file precedents `with_tail_position_suppressed` (`eval.rs:410`) and the `iterate_to_vec` IIFE (`iterators.rs:5185`). Codebase-wide: ~156 `gc_unroot_frame` teardowns against ~55 `gc_root_frame` setups — the gap is per-early-return epilogue copies. **This firing scopes to `array.rs`**, the single worst concentration (10 `Completion`-returning functions, 71 teardowns / 10 setups = ~61 redundant epilogue copies; `concat` alone repeats the teardown 10×). The remaining sites — `eval.rs` foremost — are deferred to `gc-root-scope-guard-eval` because `eval.rs` carries the `#[inline(always)]` `eval_expr` hot path, two seam bypasses that poke `gc_temp_roots.push` directly (`eval.rs:1066`, `:4324`), and 5 sites that already work around the epilogue with an IIFE — correctness-sensitive, a deliberately-scheduled firing. Mirrors the `arraybuffer-receiver-guard` → `dataview-receiver-guard` split.
- **First seen**: 2026-09-03
- **Picked**: 2026-09-04 firing (was recorded 2026-09-03 as runner-up to `complete-state-machine-generator-ctor`; #592 landing made it the natural next pick).
- **Delivered (PR #595)**: `with_gc_root_scope` combinator added in `mod.rs`; 8 whole-body `array.rs` natives migrated (concat, slice, map, filter, splice, flat, flatMap, Array.from array-like path). array.rs teardowns 71→9 (the 9 are Array.from's nested iterator frames, kept on the raw primitive). Net −17 lines (array.rs shows ~867 changed, mostly rustfmt reindent from the closure wrap). Also fixed 3 latent over-rooting exits in concat. Gate green: 622 unit / test262 built-ins/Array 6117/6117 (0 regressions) / 13 custom / clippy+fmt clean. Chose the closure combinator over an RAII guard (see PR's Proposed ADR); `eval.rs` + remaining files deferred to `gc-root-scope-guard-eval`.

## gc-root-scope-guard-eval

- **Status**: proposed
- **Score**: 22/25 (leverage 5, locality 4, blast radius 3, heat 5)
- **Files**: ~10 estimated — `src/interpreter/eval.rs` (primary; 22 setups / 50 teardowns, 5 IIFE sites, 2 `gc_temp_roots.push` bypasses at :1066 & :4324, 1 manual remove at :4586), + `iterators.rs`, `promise.rs`, `exec.rs`, `atomics.rs`, `typedarray.rs`, `property.rs`, `eval/literals.rs`, `mod.rs` (5), `bytecode/vm.rs`
- **Modules**: `src/interpreter/eval.rs`
- **Summary**: Follow-up to `gc-root-scope-guard` covering `eval.rs` and the remaining ~9 files once the `with_gc_root_scope` seam exists. Harder than the `array.rs` slice: the `eval_expr` `#[inline(always)]` hot path must not gain a call frame (the `EvalDepthGuard` doc at `eval.rs:10` warns why); the two `gc_temp_roots.push` bypasses must be routed through the seam or left as documented exceptions; the 5 existing IIFE workarounds adopt the combinator trivially. Correctness-sensitive control-flow rewrite of the hottest file — a deliberately-scheduled firing.
- **First seen**: 2026-09-04

## complete-state-machine-generator-ctor

- **Status**: landed
- **Score**: 22/25 (leverage 5, locality 4, blast radius 1, heat 3)
- **Files**: ~2 estimated — `src/interpreter/eval/generator_runtime.rs`, `src/interpreter/types.rs`
- **Modules**: `src/interpreter/eval/generator_runtime.rs`
- **Summary**: Collapse **97** byte-identical inlined "completed state-machine generator" 10-field struct literals (**31 sync + 66 async**) into `completed_state_machine_generator` / `…_async_generator` constructors. Landed net −576 lines, gate green (621 unit / 3168 test262 generator scenarios, 0 regressions / 13 custom).
- **First seen**: 2026-09-01
- **PR**: #592 (merged 2026-09-03)

## arraybuffer-receiver-guard

- **Status**: landed
- **Score**: 24/25 (leverage 5, locality 4, blast radius 1, heat 5)
- **Files**: ~1 estimated — `src/interpreter/builtins/typedarray.rs`
- **Modules**: `src/interpreter/builtins/typedarray.rs`
- **Summary**: Collapse the 5 ArrayBuffer getters' inline `enum Probe` borrow-escape prologues + 3 SharedArrayBuffer getters behind snapshot-returning receiver guards (`require_array_buffer` / `require_shared_array_buffer`), mirroring the landed `validate_typed_array` (#543). Guard returns `is_detached` in the snapshot rather than throwing (getters return 0 on detached). DataView getters + borrow-holding methods deferred to `dataview-receiver-guard`.
- **First seen**: 2026-09-02
- **PR**: #570 (merged 2026-09-02)

## validate-typed-array

- **Status**: landed
- **Score**: 24/25 (leverage 5, locality 4, blast radius 1, heat 5)
- **Files**: ~1 estimated — `src/interpreter/builtins/typedarray.rs`
- **Modules**: `src/interpreter/builtins/typedarray.rs`
- **Summary**: Collapse open-coded TypedArray receiver-validation prologues (brand check + detached/out-of-bounds check + clone + doubled `not a TypedArray` throw) behind one `validate_typed_array` seam, mirroring the existing kind-gated `validate_uint8array`. Landed: 14 read-mode sites migrated; 3 (`slice`, `sort`, `toSorted`) kept — they hold the object borrow across their body.
- **First seen**: 2026-09-01
- **PR**: #543 (merged 2026-09-01)

## completion-into-result

- **Status**: proposed
- **Score**: 21/25 (leverage 4, locality 3, blast radius 1, heat 5)
- **Files**: ~2 estimated — `src/interpreter/builtins/iterators.rs`, `src/interpreter/types.rs`
- **Modules**: `src/interpreter/builtins/iterators.rs`
- **Summary**: Add `Completion::into_result(self) -> Result<JsValue, JsValue>` and collapse the ~37 hand-rolled `match Completion { Normal(v)=>v, Throw(e)=>return Err(e), _=>… }` adapter heads in the Result-returning iterator abstract-operation helpers to `.into_result()?`; the fabricated `_ =>` error arms become removable dead code. First seen 2026-09-02. (2026-09-04 re-check: friction present — 196 `Completion::Normal` occurrences in `iterators.rs`; **runner-up candidate** to this firing's pick, within 1 point.)

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
- **Summary**: A `require_this_object(this) -> Result<u64, Completion>` ToObject prologue collapsing ~10 open-coded `match to_object(this_val) { Normal(v)=>v, other=>return other }` + object-id-unwrap dances in Object.prototype methods. A coercion prologue (ToObject can run user code), distinct from the `object-id-of` round-trip. First seen 2026-09-02. (2026-09-04 re-check: 10 `to_object(this` sites present.)

## iterator-close-return-dance

- **Status**: proposed
- **Score**: 20/25 (leverage 3, locality 4, blast radius 1, heat 5)
- **Files**: ~2 estimated — `src/interpreter/builtins/iterators.rs`
- **Modules**: `src/interpreter/builtins/iterators.rs`
- **Summary**: Four parallel reimplementations of spec IteratorClose (GetMethod(iterator,"return") → Call → handle result) that have drifted: `iterator_close_getter` (`iterators.rs:319`) and `iterator_close_with_completion` (`:541`) omit the `is_callable` pre-check that `iterator_close` (`:5103`) and `iterator_close_result` (`:5134`) perform; only the latter two handle `Completion::Exit` (the `__host_exit` floor). Extract one core `iterator_close(iterator, completion) -> Completion` all four delegate to, differences (Result vs JsValue wrapper, completion-priority, Exit) as thin adapters. Leverage 3 by backlog calibration (4 implementation sites; the 121 downstream callers do not change). Distinct from `unify-generator-async-drivers` (execution driver, not IteratorClose). First seen 2026-09-04.

## generator-entry-guard

- **Status**: proposed
- **Score**: 19/25 (leverage 4, locality 3, blast radius 1, heat 3)
- **Files**: ~1 estimated — `src/interpreter/eval/generator_runtime.rs`
- **Modules**: `src/interpreter/eval/generator_runtime.rs`
- **Summary**: Fold ~11 duplicated generator-entry "called on non-object" TypeError pairs into a `require_generator_object` guard. Ties into object-id-of.

## ordinary-create-from-constructor

- **Status**: proposed
- **Score**: 19/25 (leverage 5, locality 4, blast radius 4, heat 3)
- **Files**: ~15–20 estimated — `src/interpreter/builtins/collections.rs`, `disposable.rs`, `typedarray.rs`, `proxy.rs`, `iterators.rs`, `date.rs`, `promise.rs`, all `intl/*`, all `temporal/*`
- **Modules**: `src/interpreter/mod.rs` (seam home), `src/interpreter/builtins/collections.rs`
- **Summary**: Every `[[Construct]]` builtin hand-inlines OrdinaryCreateFromConstructor in two separable pieces: a new-target guard (`if new_target.is_none() { Throw }`, 29 sites) and prototype-resolution + object materialization (`match get_prototype_from_new_target_realm(...)` + `create_object_id` + field-set, 38 sites). `get_prototype_from_new_target_realm` (`mod.rs:1410`) already exists but the ~15 lines around it are copy-pasted per constructor, and drift is live (`collections.rs:479-488` does three separate `borrow_mut` + `.unwrap_or`, `disposable.rs:377-389` batches one borrow + `if let Some`). Ship as two composable helpers (`require_new_target(name)` + `ordinary_create_from_constructor(...)`) since Promise validates its executor between the two steps. Blast radius 4 (15–20 files, crosses many builtin families) drags the total below the pick; best done in waves by a human-scheduled firing. First seen 2026-09-04.

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

## this-primitive-value

- **Status**: proposed
- **Score**: 18/25 (leverage 3, locality 4, blast radius 1, heat 3)
- **Files**: ~5 estimated — `src/interpreter/builtins/number.rs`, `bigint.rs`, `string.rs` (+ helper home)
- **Modules**: `src/interpreter/builtins/number.rs`
- **Summary**: Five near-identical private helpers implement "return primitive X, else unwrap a wrapper object whose `class_name == "X"` reading `primitive_value`, else None/throw": `this_number_value` (`number.rs:397`), `this_boolean_value` (`:667`), `this_symbol_value` (`:258`), `this_bigint_value` (`bigint.rs:40`), `this_string_value` (`string.rs:6`), differing only by the class-name literal and the primitive extractor. A generic `this_primitive_value(this, class_name)` (or small trait) collapses the five parallel brand-and-unwrap bodies. Wrapper-object analogue of `object-this-coercion` (ToObject), so net-new. First seen 2026-09-04.

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
- **Reason**: Leverage 2 — `/simplify`-class round-trip cleanup, fails the deepening bar (complexity renamed, not concentrated behind a seam). (2026-09-04 re-check: filter still applies.)

## proxy-blind-callable-check

- **Status**: dropped
- **Score**: n/a (behaviour change, not a deepening)
- **Files**: ~4–6 estimated — `src/interpreter/builtins/typedarray.rs`, `collections.rs`, `iterators.rs`
- **Modules**: `src/interpreter/builtins/typedarray.rs`
- **Summary**: 37 `builtins/` sites open-code the callability test as a bare `obj.borrow().callable.is_some()`, bypassing the Proxy branch that the canonical `is_callable` (`promise.rs:2132`) handles — so `new Proxy(fn, {})` passed as a TypedArray `sort` comparator / `from` mapfn / Map `adder` throws "not a function" though spec IsCallable is true. Routing all 37 through `self.is_callable` would fix the bug and unify the checks.
- **First seen**: 2026-09-04
- **Reason**: This is a spec-conformance **behaviour change** (a latent bug fix), not a behaviour-preserving deepening — an unattended deepening run must pin existing behaviour before moving it, and here existing behaviour is wrong. File as a jsse bug report instead; a deepening that routes the checks through `is_callable` can follow once the semantics are agreed.

## define-accessor-adoption

- **Status**: dropped
- **Score**: n/a (leverage 2 — finishing an existing migration)
- **Files**: ~10–12 estimated — `src/interpreter/builtins/temporal/*`, `regexp.rs`, `iterators.rs`, `mod.rs`
- **Modules**: `src/interpreter/mod.rs`
- **Summary**: A `define_getter` seam already exists (`mod.rs:1812`) and is adopted at 22 sites, but 42 sites still open-code the getter as `create_function(...)` + raw six-field `PropertyDescriptor` + `insert_property`. The genuinely net-new piece is a `define_accessor(name, get, set)` for the 4 getter+setter sites lacking a helper.
- **First seen**: 2026-09-04
- **Reason**: Leverage 2 — the deep seam already exists, so migrating the 42 raw getters is `/simplify`-class finishing work, not a new deep module. The `define_accessor` (getter+setter) piece is genuinely net-new but only 4 sites, too low-leverage to pick.

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
