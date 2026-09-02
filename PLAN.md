# Plan: issue #556 — flatMap inner-iterator state not GC-rooted after reassignment

## 1. Problem restated

`Iterator.prototype.flatMap`'s native implementation (`src/interpreter/builtins/iterators.rs`,
`setup_iterator_helper_methods`, `flatMap` closure) keeps its per-helper state — outer
iterator/next, mapper, counter, current inner iterator/next, `alive`/`running` flags — in an
`Rc<RefCell<(...)>>` shared by the `next` and `return` native closures. `JsValue`s captured
inside a `Rc<dyn Fn>` native closure are invisible to `trace_object_fields`
(`src/interpreter/gc.rs`); the only way to make them visible is `pin_native_root(anchor, value)`,
which appends the value to `anchor`'s traced `gc_native_roots` list. At construction, `next`/
`return` are rooted on the helper via `set_helper_gc_roots(&helper, vec![outer_iter, outer_next,
mapper, /* inner_iter, inner_next if present */])` (iterators.rs:2843-2850) — but at that point
the state tuple's inner-iterator slots (`.4`, `.5`) are still `None`, since no inner iterable has
been opened yet, so the `if let Some(...)` guards never fire and nothing about the inner iterator
is ever rooted. Later, when the outer iterator yields a value and the mapper's result is turned
into an inner iterator (iterators.rs:2757-2762), the new `JsValue`s are written into the `RefCell`
but never pinned. If a GC runs while that inner iterator is mid-stream and its underlying object
has no other live reference (e.g. an ephemeral generator returned by the mapper), the object can
be swept while the closure still holds its id. This is a stale-id / use-after-collect correctness
bug, not a dangling-pointer memory-safety bug: the arena returns `None` from `get_object_cell`/
`get_object_cell_expect` for a freed id rather than aliasing live memory, so the failure mode is a
thrown/panicking lookup (or, if the id slot has since been reused, a silent read of an unrelated
object) — either way, wrong behavior, not undefined behavior.

This is a pure GC/memory-management bug in the native-closure implementation. It does not change
any spec-observable behavior of `Iterator.prototype.flatMap`: correctly implemented, the fix makes
already-specified behavior hold under GC pressure that previously could corrupt it.

## 2. Spec basis

N/A: no JavaScript behavior change. `Iterator.prototype.flatMap` is specified in
`spec/spec.html` at `<emu-clause id="sec-iterator.prototype.flatmap">` ("Iterator.prototype.flatMap
( _mapper_ )"); this plan does not alter the observable semantics that clause defines. The bug and
its fix are confined to how our engine roots GC references inside a native-closure capture
(`src/interpreter/gc.rs` / `pin_native_root`), which is implementation machinery the spec does not
reach — the spec has no notion of a garbage collector or of "native closures."

## 3. Files to touch

- `src/interpreter/builtins/iterators.rs` — `flatMap`'s `next_fn` closure (currently around
  iterators.rs:2672-2803) and the post-construction rooting block (iterators.rs:2843-2850).
- `src/interpreter/tests.rs` — new regression test(s) exercising a major GC between two `.next()`
  calls on a `flatMap()` helper while an inner iterator is mid-stream.

No `docs/adr/` entry: this is a bug fix within the existing "closures pin values with
`pin_native_root`; kind-specific fields are matched exhaustively in `trace_object_fields`" GC
design already documented in `CLAUDE.md`'s Architecture Notes and in `pin_native_root`'s own doc
comment (gc.rs:174-193). The fix uses `pin_native_root` exactly as designed, anchored on the
closure's own `this` (see Slice 2) — no new mechanism, so no new architectural decision to record.

## 4. TDD slices

### Slice 1 — red: regression test proving the bug

Add a test in `src/interpreter/tests.rs`, near the existing native-closure-GC regression tests
(`promise_all_settles_across_major_gc_between_element_settlements` and siblings, ~line 1200,
which cover the analogous bug #309 for promise combinators) using the existing
`run_steps_with_major_gc_between` helper. New test:
`flat_map_inner_iterator_survives_major_gc_between_next_calls`.

Steps:
1. Step 1 JS: create `globalThis.flat = (function* () { yield 1; yield 2; })().flatMap(function* (x) { yield x; yield x * 10; })`
   and call `flat.next()` once, storing the yielded value on `globalThis` (opens the inner
   generator; nothing else in the JS heap holds a reference to that inner generator object once
   the IIFE-less literal expression producing it goes out of scope).
2. `run_steps_with_major_gc_between` forces a major GC after step 1 (this is where the bug bites:
   the inner generator is reachable only from inside the `next_fn` closure's `Rc<RefCell>`
   capture, which today is not represented in any traced root).
3. Step 2 JS: call `flat.next()` again and store the result.

Assert both `.next()` calls returned `{ value: 1, done: false }` then `{ value: 10, done: false }`
(second value drawn from the still-alive inner generator, not a freed/stale object id). Before the
fix, this either panics (arena lookup on a freed id via `get_object_cell_expect`/`expect`) or —
depending on id-reuse timing — silently reads through to an unrelated object; either way the test
fails or aborts the test binary. This is the "red" step.

A second case in the same test (or a follow-up test) also exercises the *last* inner iterator
opened before the outer iterator is exhausted, to confirm the return-path (`Ok(None)` on the
outer, `state_next.borrow_mut().6 = false`) doesn't mask the same bug for the final fan-out.

### Slice 2 — green: root the inner iterator on reassignment

No new anchor-tracking machinery is needed. `%IteratorHelperPrototype%.next` (iterators.rs:1270,
specifically `interp.call_function(&next_closure, this, args)` at line 1310) invokes the stored
`next_fn` closure with `this` bound to the helper object itself — confirmed by reading that
dispatch site and the mirrored `return` dispatch at iterators.rs:1328-1375
(`interp.call_function(&return_closure, this, args)`). So the `next_fn` closure's own `_this`
parameter (currently unused, `move |interp, _this, _args|`) already *is* the correct
`pin_native_root` anchor — the helper `JsValue` — on every call, with no `Option`/fallback case to
handle: this closure is never reachable except through that dispatch path, so `this` is always the
helper.

- Rename `_this` to `this` in the `flatMap` `next_fn` closure signature (iterators.rs:2675).
- At the reassignment point (iterators.rs:2757-2762, the `Ok((new_inner, inner_next_method))` arm),
  immediately after writing `.4`/`.5`, add:
  `interp.pin_native_root(this, &new_inner); interp.pin_native_root(this, &inner_next_method);`
- Drop the now-fully-dead `if let Some(ref v) = b.4 { ... }` / `b.5` branches in the
  construction-time rooting block (iterators.rs:2846-2848) — they can never fire (state is built
  with `.4`/`.5` as `None`, and nothing reassigns them before `create_iterator_helper_object` runs),
  so keeping them would be dead code that misleadingly suggests inner-iterator rooting is already
  handled there.

Per `pin_native_root`'s doc comment, pins only accumulate (no unpin), so this leaks one pinned
`JsValue` pair per inner iterable the helper opens over its lifetime. That matches the issue's
accepted tradeoff for "typical flatMap fan-out counts" and mirrors the existing precedent at
iterators.rs:1422-1429 (`set_helper_gc_roots`) and the promise-combinator fix for #309 — no new
unpinning mechanism is in scope for this issue.

Do not rely on a green build alone to call this slice done: `pin_native_root` silently no-ops if
its anchor isn't an object (gc.rs:195-197), so a wrong anchor (e.g. accidentally passing something
other than `this`) would compile and pass every test *except* Slice 1's GC-timing regression test.
Slice 1's test is the actual verification; run it explicitly and confirm it fails before this
change and passes after, not just that the suite is green at the end.

Verify Slice 1's test goes green. Run the full `cargo test --release --bin jsse` (per
`fmt-hook-clippy-gate`, edits to `.rs` files trigger a PostToolUse rustfmt+clippy gate; the crate
is bin-only) to confirm no regressions in the surrounding iterator-helper tests.

### Slice 3 — refactor (only if warranted)

None planned. The fix is a two-line addition at one call site; there's nothing to extract.

**Confirmed sibling occurrence, deliberately not fixed here:** `Iterator.concat` (iterators.rs,
`concat_fn`, currently ~lines 2996-3203) has the identical bug shape. Its `next_fn` closure keeps
`(iterables, current_index, current_iter, current_next, alive, running)` in the same
`Rc<RefCell<...>>` pattern; `current_iter`/`current_next` (`.2`/`.3`) start `None` and get
reassigned at iterators.rs:3138-3139 each time a new iterable in the `concat` list is opened, after
construction-time rooting (iterators.rs:3192-3200) has already run over `.2`/`.3` while they were
still `None` — same dead-`if-let` shape as flatMap's `.4`/`.5`. This was found by inspection while
checking every `get_iterator_flattenable`/lazy-helper call site in iterators.rs for the same
reassignment-after-construction shape (the other two `get_iterator_flattenable` callers, in
`Iterator.zip`/`Iterator.zipKeyed`, root synchronously via `interp.gc_temp_roots` inside a single
call rather than a long-lived closure capture, so they don't have this bug). It isn't a
hypothetical future occurrence — it is a live, currently-unfixed instance of the same bug. It is
out of scope for this issue (assigned scope is flatMap only; see
`docs/agents/issue-tracker.md`'s single-issue-per-branch model) but should not be silently dropped:
the implementation stage should file a follow-up GitHub issue for `Iterator.concat` (same title
pattern, same fix shape: rename `concat_fn`'s `next_fn` closure's `_this` to `this`, pin `.2`/`.3`
at iterators.rs:3138-3139, drop the dead `if let Some(ref v) = b.2 { ... }` / `b.3` guards) before
opening the PR for #556, and reference it from the PR description.

## 5. Test surface

- No `test262/` directory is a targeted match: test262 has no test that forces a GC cycle
  mid-iteration (test262 is engine-agnostic and doesn't assume a moving/collecting GC at all), so
  this bug is invisible to it by construction. Run the existing targeted directory as a
  non-regression check anyway: `uv run python scripts/run-test262.py test262/test/built-ins/Iterator/prototype/flatMap/`.
- The regression coverage lives in `src/interpreter/tests.rs` (Slice 1), run via
  `cargo test --release --bin jsse flat_map_inner_iterator_survives_major_gc_between_next_calls`.
  This is the right home rather than `test262-extra/`: `test262-extra/` follows test262's own
  file-per-behavior, engine-agnostic pattern for spec-correctness gaps, and this is instead an
  engine-internal GC-safety property with no spec vocabulary to phrase it in (there is no
  `$262.gc()`-based test262 convention for asserting an object survives a specific collection
  point relative to a specific reassignment). The precedent (`promise_all_settles_across_major_gc_between_element_settlements`
  and siblings for #309) lives in the same Rust test module for the same reason.
- Full gate before considering the slice done: `cargo test --release --bin jsse`,
  `./scripts/lint.sh`, and the full `uv run python scripts/run-test262.py` run (not baseline-updating —
  this is a feature branch; baseline comparison is against `origin/main:test262-pass.txt`
  automatically).

## 6. Regression risk

- **Blast radius is narrow**: the change touches only the `flatMap` closure's inner-iterator
  reassignment arm and its construction-time rooting block. It does not touch `eval_expr`/
  `exec_statement`, `property.rs`, the bytecode fast path, or any `ObjectKind` match arm.
- **GC rooting correctness**: the new pinning calls must fire on every reassignment of `.4`/`.5`
  (there is exactly one call site today — iterators.rs:2757-2762 — but double-check no other arm
  in the `next_fn` loop also writes those fields without going through this path before landing
  the change).
- **Leak-shaped, not crash-shaped, if under-pinned**: because `pin_native_root` only accumulates,
  a mistake here is far more likely to under-root (bug persists, possibly intermittently depending
  on GC timing/heap layout) than to over-root or double-free; there's no unpin path to get wrong.
  The regression test's major-GC-immediately-after-open timing is deliberately the worst case for
  under-rooting and should reliably catch a reintroduction.
- **test262-pass.txt baseline**: not expected to move — no test262 test is timing-sensitive to GC
  the way the regression test is, so no currently-passing or currently-failing test should flip.
  If the full test262 run does show movement, that's a signal the change had a wider effect than
  planned and needs re-examination before merging, not a baseline update (out of scope per the
  constraints — baseline updates are a `main`-branch operation).
- **Library-test harnesses**: none of the wired libraries (`decimal.js`, `big.js`, `acorn`,
  `prismjs`, `uglify-js`, `highlight.js`, `uuid`, `luxon`, `zod`, `moment`) are known to use
  `Iterator.prototype.flatMap` on ephemeral generators under GC pressure in a way this would
  perturb; not planned to re-run as part of this fix, but `cargo test --release` already covers
  the engine-level contract these harnesses depend on.

## 7. Out of scope

- **`Iterator.concat`'s identical bug** (see Slice 3) — same fix shape, different call site, but a
  different function outside the issue's assigned scope. File a follow-up issue before opening the
  PR for #556; do not bundle the fix into this PR.
- Generalizing `pin_native_root`/`set_helper_gc_roots` into a documented reusable pattern for
  *reassigned* (as opposed to construction-time) closure state, as the issue's third suggested
  direction proposes. Now that a second occurrence (`Iterator.concat`) is confirmed to exist, this
  is a reasonable candidate for the `.architecture/backlog.md` deepening list once both instances
  are fixed — but implementing the shared pattern is still not this issue's job; note it in the
  follow-up issue instead of designing it here.
- Any unpinning/root-shrinking mechanism for `pin_native_root` (e.g. to bound the leak for
  pathological fan-out counts). The issue explicitly accepts the leak as-is; out of scope here.
- Refactoring `chunks`/`windows` or any other iterator helper — they don't have this bug.
- Formatting or lint cleanups outside the touched lines.
