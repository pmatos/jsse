# Plan: issue #813 — Map/Set/WeakMap/WeakSet constructor's under-construction object is unrooted during iteration

## 1. Problem restated

`Map`, `Set`, `WeakMap`, and `WeakSet`'s constructors (`src/interpreter/builtins/collections.rs`) each allocate the new collection object (`obj_id` / `this_val`) before the optional `iterable` argument is consumed, and then run a sequence of operations — `Get(target, "set"/"add")`, `GetIterator`, repeated `IteratorStep`/`IteratorStepValue`, `Get(next, "0"/"1")`, and `Call(adder, target, …)` — every one of which can invoke arbitrary user code (a getter, a `[Symbol.iterator]` method, the adder itself) and therefore run a garbage collection. `this_val`/`obj_id` is a plain Rust local in the native closure; it is never pushed onto `gc_temp_roots`, `gc_bytecode_roots`, or any other root set collected by `collect_gc_roots`. If a collection runs while the half-built collection object is otherwise unreachable, it gets swept, and its arena slot can be recycled by a later allocation (e.g. the churn the user's getter/`return()` performs) — so a subsequent use of `this_val`/`obj_id` (the next `adder` call, a later `Get`) silently operates on an unrelated recycled object, producing an incorrect result (e.g. the observed spurious `TypeError("Map.prototype.set requires a Map")`) instead of the spec-mandated outcome. The bug needs `JSSE_GC_STRESS` (or an explicit `$262.gc()`) to manifest deterministically; ordinary allocator-paced GC rarely lands a collection in this exact window.

While grounding the fix shape against `call_function_inner_impl` (`src/interpreter/eval.rs`) and `get_object_property` (`src/interpreter/property.rs`), the same local-Rust-variable hazard was found on a second value in the same four constructors: the `iterator` record returned by `GetIterator`/`[Symbol.iterator]()`. `call_function_inner_impl`'s native-call path (`src/interpreter/eval.rs:~5205-5219`) only roots whatever is passed as *that specific call's* `this_val`/`args` for the duration of *that* call — so `iterator` is protected while its own `next()`/`return()` is running (it is always passed as the receiver there), but sits exposed, like `this_val`, during `iterator_value`'s `Get(next, "value")`, the `Get(next, "0")`/`Get(next, "1")` pair, and the `adder` call — each of which can run arbitrary code. `adder` itself needs no extra rooting: it is always a property value reachable from either `new_target`'s prototype chain or the realm's permanently-rooted `*_prototype` field, both of which `collect_gc_roots` already roots unconditionally, so a temp-root on `adder` would be redundant. This plan fixes both `this_val` and `iterator` in the same change, at the same four call sites, since the issue title's "unrooted during iteration" covers both and leaving one half-fixed would leave an equally real, just-as-stress-discoverable variant of the same bug.

## 2. Spec basis

The algorithms below all pass the under-construction collection object as a stable receiver/target across multiple steps that call `Get`/`GetIterator`/`Call`, each of which can execute arbitrary ECMAScript code. The spec's abstract-step model assumes that object stays the *same* object throughout — a prerequisite the engine must provide, not a semantics choice this fix makes:

- `spec/spec.html#sec-map-iterable` — **Map ( [ iterable ] )**: creates `map` (step 2), then `Get(map, "set")` (step 5) and `? AddEntriesFromIterable(map, iterable, adder)` (step 7).
- `spec/spec.html#sec-add-entries-from-iterable` — **AddEntriesFromIterable ( target, iterable, adder )**, used by both Map and WeakMap: `GetIterator` → loop of `IteratorStepValue` → `Get(next, "0")`/`Get(next, "1")` (each `IfAbruptCloseIterator`) → `Call(adder, target, « k, v »)`. `target` must remain the same object across every iteration.
- `spec/spec.html#sec-weakmap-iterable` — **WeakMap ( [ iterable ] )**: same shape as Map, also delegating to `AddEntriesFromIterable`.
- `spec/spec.html#sec-set-iterable` — **Set ( [ iterable ] )**: creates `set`, `Get(set, "add")`, then an inlined loop (`GetIterator` → `IteratorStepValue` → `Call(adder, set, « next »)` → `IfAbruptCloseIterator`).
- `spec/spec.html#sec-weakset-iterable` — **WeakSet ( [ iterable ] )**: same inlined shape as Set.

This change does not alter any observable JavaScript semantics (no new throws, no changed property access order, no changed return values for a correctly-collected engine). It is a pure memory-safety fix so the engine actually implements the steps above as written — today, under GC pressure, it silently does not. `src/interpreter/mod.rs:with_gc_root_scope`/`gc_root_value` is existing, already-used machinery (see `src/interpreter/builtins/array.rs` `concat`/`slice` for the identical idiom); this plan applies it, it does not design anything new.

## 3. Files to touch

- `src/interpreter/builtins/collections.rs` — the only production file. Four sites, each the constructor closure passed to `JsFunction::constructor`:
  - `Map` constructor (currently ~line 463–573)
  - `Set` constructor (currently ~line 1519–1638)
  - `WeakMap` constructor (currently ~line 1907–2063)
  - `WeakSet` constructor (currently ~line 2188–2317)
- `test262-extra/Map-constructor-under-construction-gc-rooting.js` — new.
- `test262-extra/Set-constructor-under-construction-gc-rooting.js` — new.
- `test262-extra/WeakMap-constructor-under-construction-gc-rooting.js` — new.
- `test262-extra/WeakSet-constructor-under-construction-gc-rooting.js` — new.
- No `docs/adr/` entry: this applies an already-documented pattern (`with_gc_root_scope`/`gc_root_value`, already described in `CONTEXT.md`'s GC rooting vocabulary) to four more call sites; it is not a new architectural decision. No `CONTEXT.md` change: no new vocabulary is introduced.

## 4. TDD slices

Each constructor slice: add one failing test262-extra file demonstrating the hazard without needing `JSSE_GC_STRESS` (a direct, placed `$262.gc()` + allocation churn inside the user-code callback that the constructor must call before the object is safe), confirm it fails against the current binary, then apply the one-shape fix to that constructor and confirm it passes.

The fix shape (applied identically at each of the four sites): immediately after `let this_val = JsValue::object(obj_id);`, wrap everything from the `iterable` check through the final `Completion::Normal(this_val)` in `interp.with_gc_root_scope(|interp| { interp.gc_root_value(&this_val); <unchanged body up to and including obtaining `iterator`>; interp.gc_root_value(&iterator); <unchanged rest of body>; Completion::Normal(this_val) })`, used as the closure's tail expression (no `return` keyword needed — mirrors the existing `Array.prototype.concat`/`slice` idiom in `array.rs`). No change to control flow, error messages, or any step logic inside the wrapped body — only the two `gc_root_value` calls are added.

0. **Confirm the repro, with a capped build.** Before touching any code: build the current (unfixed) binary with explicit parallelism and timeout (`cargo build --release -j<N> ` sized to the host's actual available share, not its full core count, per the run's memory-budget constraint — never an unbounded `-j`; pass an explicit Bash timeout well over the default 2 minutes for a release build), then run the issue's exact repro script under `JSSE_GC_STRESS=1`. This is the dynamic confirmation the issue itself asks for ("worth a quick confirmation against `origin/main` directly before fixing") and is independent of this plan's own static analysis in §1. Record the actual observed output (the exact error thrown, or lack of one) for the PR body. If the repro does *not* reproduce on this branch's current `collections.rs` (unlikely, since the issue's own diff-based reasoning already rules out #794 as the cause — but confirm), stop and re-open the triage question in a PR comment rather than forcing a fix onto a non-reproducing bug.

1. **Map constructor.**
   - Test: `test262-extra/Map-constructor-under-construction-gc-rooting.js`, two scenarios in one file:
     (a) an iterable whose `[Symbol.iterator]()` method calls `$262.gc()` plus a small allocation churn (same shape as `iterator-close-completion-payload-gc-rooting.js`'s `collect()` helper) before returning the iterator — exercises the `this_val` window between `Get(map, "set")` and the first `adder` call. Assert `new Map(iterable_with_one_entry)` actually contains that entry (`m.get(1) === "a"`) instead of throwing the engine-internal `TypeError("Map.prototype.set requires a Map")`.
     (b) a two-entry iterable whose second entry's `get 0()` accessor calls `$262.gc()` plus churn, then throws a distinctive `Error` — exercises the `iterator` window (`Get(next, "0")` runs between the two `adder`-reachable `Get` calls, with `iterator` unrooted). Assert the thrown error is exactly that `Error` (message intact, not replaced by a corrupted/substituted engine error) and that `m.size === 1` (the first entry committed, the second never did) after the `catch`.
   - Expect red on both (a) and (b) against the current binary (per slice 0's confirmation) — (a) throwing the spurious TypeError or similar, (b) possibly throwing the wrong error or producing a corrupted map.
   - Fix: wrap the `Map` constructor body as described above (`this_val`, then `iterator`, both rooted).
   - Expect green: both assertions pass; no other `Map` behavior changes.

2. **Set constructor.**
   - Test: `test262-extra/Set-constructor-under-construction-gc-rooting.js`, same two-scenario shape: (a) GC inside `[Symbol.iterator]()`, asserting `new Set(iterable_with_one_entry).has(1)`; (b) GC inside a throwing getter encountered while stepping the second value, asserting the thrown error survives intact and `s.size === 1`.
   - Red against the unfixed `Set` constructor (even after slice 1's `Map` fix — the two constructors are independent closures).
   - Fix: same wrapper (`this_val` then `iterator`) applied to the `Set` constructor body.
   - Green.

3. **WeakMap constructor.**
   - Test: `test262-extra/WeakMap-constructor-under-construction-gc-rooting.js`. Keys must be objects (`WeakMap` requirement) — use plain object literals as keys. Same two scenarios: (a) asserts `wm.get(key) === "a"`; (b) asserts the thrown error survives intact and the first entry is retained (probe via a `has(key1)` call, since `WeakMap` has no `.size`).
   - Red, then fix, then green, same pattern.

4. **WeakSet constructor.**
   - Test: `test262-extra/WeakSet-constructor-under-construction-gc-rooting.js`. Values must be objects. (a) asserts `ws.has(obj) === true`; (b) asserts the thrown error survives intact and `ws.has(obj1) === true` (no `.size` on `WeakSet` either).
   - Red, then fix, then green, same pattern.

5. **Full-suite confirmation (no new code).** Run the regression and stress gates in "Test surface" below and confirm all four new tests stay green together, the targeted test262 directories are unaffected, and a `JSSE_GC_STRESS` sweep of `test262-extra/` (which is how the issue was originally found) no longer reproduces the issue's own repro script from slice 0.

## 5. Test surface

- New regression coverage (this is implementation-detail GC-safety, not a test262-covered spec requirement — test262 has no portable way to force a GC at a precise point, hence `test262-extra/` with `features: [host-gc-required]`, following the exact pattern of the existing `test262-extra/iterator-close-completion-payload-gc-rooting.js`). Each file carries two scenarios per §4, covering both the `this_val` window (GC inside `[Symbol.iterator]()`) and the `iterator` window (GC inside a later entry's throwing key getter):
  - `test262-extra/Map-constructor-under-construction-gc-rooting.js`
  - `test262-extra/Set-constructor-under-construction-gc-rooting.js`
  - `test262-extra/WeakMap-constructor-under-construction-gc-rooting.js`
  - `test262-extra/WeakSet-constructor-under-construction-gc-rooting.js`
- Targeted test262 directories to run unchanged (confirm no regression, no baseline movement expected since these are native-closure constructors and the fix changes no control flow):
  - `uv run python scripts/run-test262.py test262/test/built-ins/Map/`
  - `uv run python scripts/run-test262.py test262/test/built-ins/Set/`
  - `uv run python scripts/run-test262.py test262/test/built-ins/WeakMap/`
  - `uv run python scripts/run-test262.py test262/test/built-ins/WeakSet/`
  - `uv run python scripts/run-test262.py test262/test/built-ins/Map/ --bytecode` (and same for Set/WeakMap/WeakSet) — the constructors are native Rust closures either way, but the calling test scripts route through the bytecode VM under this flag, so it is cheap extra coverage that the rooting fix behaves identically from both callers.
- `uv run python scripts/run-test262.py test262-extra/` (plain) to confirm the four new files plus all existing `test262-extra/` tests pass.
- `JSSE_GC_STRESS=1 uv run python scripts/run-test262.py test262-extra/ --timeout 300` — mirrors how the issue was found; confirms the fix holds under the harshest per-safepoint stress setting on the small `test262-extra/` corpus.
- `JSSE_GC_STRESS=7 uv run python scripts/run-test262.py test262-extra/ --timeout 300` — matches the blocking CI gate (`ci.yml`) so the PR exercises the same check CI will run.
- `cargo build --profile release-checked` then `uv run python scripts/run-test262.py --binary target/release-checked/jsse test262-extra/` (normal and `--bytecode`) — exercises the `gc_assert_root_depth`/root-stack LIFO debug-asserts described in `CLAUDE.md`'s "GC Root-Stack Discipline" against the new rooting calls.
- `cargo test --release` — full Rust unit/integration suite (includes the pre-existing `with_gc_root_scope`-seam tests in `src/interpreter/tests.rs`; no new Rust unit test is planned since the hazard is only observable end-to-end through actual GC behavior, which is exactly what the new `test262-extra/` scripts exercise, consistent with how the sibling #794 `iterator_close` rooting fix was tested).
- `./scripts/lint.sh` before considering any slice done.
- Full `uv run python scripts/run-test262.py` run (the whole suite) at the end, per `CLAUDE.md`'s "After any implementation work, run the full test262 suite" — not expected to move `test262-pass.txt` in either direction.

## 6. Regression risk

- **Low risk, narrow blast radius.** The change only adds `with_gc_root_scope`/`gc_root_value` calls around four native-closure bodies; it does not touch `eval_expr`/`exec_statement`, `property.rs`'s MOP, any `ObjectKind` match, or the bytecode compiler/VM. The constructors are native Rust closures invoked identically from the tree-walker and the bytecode VM, so there is no `--bytecode`-specific risk.
- **GC rooting / `gc_safepoint()`:** this is squarely the machinery being exercised. The risk is getting the wrapper boundaries wrong (e.g. rooting after a GC-risking call has already run, or unrooting before the object is truly done) — mitigated by rooting immediately after `this_val` is constructed and keeping the wrapped scope open through the literal tail return, matching the issue's own suggested fix shape and the existing `array.rs` idiom precisely.
- **Root-stack LIFO discipline (`gc_temp_roots`):** `with_gc_root_scope` already guarantees balanced push/pop on every exit path (tail, early `return`, `?`), so no manual `gc_unroot_id` bookkeeping is introduced that could violate the LIFO-release debug-assert. Rooting two values (`this_val` then `iterator`) in the same scope is still a single bulk-truncate on exit — `with_gc_root_scope` doesn't pop them individually, so push order doesn't matter. Running under `cargo build --profile release-checked` (slice 5) is the direct check for this.
- **`adder` deliberately left untouched:** confirmed via `collect_gc_roots` (`src/interpreter/gc.rs`) that `new_target` is unconditionally rooted for the whole native call, and every realm's `*_prototype` field is rooted every safepoint (per `CLAUDE.md`'s Architecture Notes) — `adder` is always reachable through one of those two paths, so adding a temp-root for it would be dead code, not a fix.
- **No baseline movement expected.** `test262-pass.txt` (read from `origin/main`) should be unaffected: the fix changes no control flow, no thrown error messages, no property-access order under normal (non-GC-forced) execution — it only changes what the GC tracer can see. The bug this fixes is not currently causing any `test262/` failures (per the issue, it needs `JSSE_GC_STRESS` or a precisely-timed allocator GC to manifest), so there is nothing to "newly pass" in the standard suite either; the targeted directories above are run to confirm that, not to move the baseline.
- **Interaction with `#794`'s `iterator_close` rooting:** the two fixes are independent and additive — `#794` roots `iterator_close`'s own completion payload across its `return()` call; this fix roots the constructor's `this_val` across the whole constructor body (which includes calls into `iterator_close`). No overlap, no conflict.

## 7. Out of scope

- **`Map.groupBy`** (`src/interpreter/builtins/collections.rs`, the static method starting ~line 619): creates `result_map_id`/`result_val` and then calls the user `callback` in a loop before the result is returned, structurally the same hazard as the four constructors. The issue (#813) only names "Map/Set constructor" and "WeakMap, and WeakSet's constructors" — `groupBy` is a different, unmentioned call site. Fixing it here would be scope creep on a bug fix; flagging it for a follow-up issue instead rather than silently bundling it into this PR.
- **Any refactor of the four constructors' shared boilerplate** (the `OrdinaryCreateFromConstructor`-equivalent object-creation lines, or the near-duplicated iterator-driving loops across Map/Set/WeakMap/WeakSet) into a shared helper. The four bodies are already near-duplicates; de-duplicating them is a legitimate follow-up but is an unrelated horizontal refactor that would make this bug-fix diff harder to review.
- **No formatting-only changes** anywhere else in `collections.rs`.
- **Not touching `iterator_close`'s own rooting** (already fixed by #794) or any other collection-builtin method (`Map.prototype.set`, `Set.prototype.add`, etc.) — those are not under construction and are not in scope here.
