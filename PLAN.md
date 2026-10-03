# Plan: issue #798 — DisposableStack resources are not traced by the GC

## 1. Problem restated

`ObjectKind::DisposableStack(DisposableStackData)` backs both `DisposableStack` and
`AsyncDisposableStack` instances (`src/interpreter/types.rs:2095`). Its payload
(`src/interpreter/types.rs:1983-1986`) is `stack: Vec<DisposableResource>`, where each
`DisposableResource` (`src/interpreter/types.rs:871-875`) holds a `value: JsValue` and a
`dispose_method: JsValue` registered via `.use()`, `.adopt()`, or `.defer()`. Code reading turned
up **three** independent reachability gaps feeding the single symptom the issue cites
(`DisposableStack-patterns` failing under `JSSE_GC_STRESS`), not the one match arm the issue
names. All three are needed for that symptom to actually go away; this is noted as a scope
judgment call on the issue (see note at the end of this section).

1. **Before disposal starts (the issue's literal ask).** `Interpreter::trace_object_fields`
   (`src/interpreter/gc.rs`) dispatches on `ObjectKind` in an exhaustive `match` (`gc.rs:1005`),
   but `ObjectKind::DisposableStack(_)` sits in the no-op arm (`gc.rs:1006-1013`). A collection
   between registration (`use`/`adopt`/`defer`) and the `dispose()`/`disposeAsync()` call can free
   a resource whose only remaining reference is the stack itself.
2. **During synchronous disposal itself.** `DisposableStack.prototype.dispose`
   (`disposable_stack_dispose`, `src/interpreter/builtins/disposable.rs:417-480`) takes the
   resources out of the object with `std::mem::take(&mut ds.stack)` into a bare Rust local
   *before* running the dispose loop, then calls each `dispose_method` via `call_function`, which
   executes arbitrary user code with its own statement-boundary GC safepoints. From the moment of
   the `take`, nothing roots the remaining (not-yet-called) entries — fix (1) only protects the
   window *before* `dispose()` runs, and a bare Rust `Vec<DisposableResource>` is invisible to the
   tracer (objects are addressed by arena id, not kept alive by holding a `JsValue`). A resource
   registered early (processed last, since the loop disposes in reverse) and reachable only
   through that stack can be collected while an earlier-processed disposer's body is still
   running — exactly the shape of `DisposableStack-patterns.js`'s two chained `defer()` calls. The
   loop's own error-accumulation local (`current_error: Option<JsValue>`, built via
   `wrap_suppressed_error`) has the identical problem: once produced it sits in a Rust local with
   no root until the loop returns it.
   `AsyncDisposableStack.prototype.disposeAsync` (`async_disposable_stack_dispose`,
   `disposable.rs:909-933`) does **not** share this gap for `use()`/`defer()` resources: it hands
   the taken stack to a `DisposeCursor` registered via `self.scheduler.insert_async_disposal`, and
   `collect_gc_roots` already walks `self.scheduler.iter_async_disposals()` (`gc.rs:507-509`), on
   top of `DisposeCursor::step`/`record_error` re-rooting `remaining` and any newly-wrapped error
   via `gc_root_value` on every step (`dispose.rs:87-93`, `202-224`). The synchronous path never
   adopted that machinery and rolled its own unrooted loop instead.
3. **`adopt()`'s wrapper closure, on both stacks, independent of (1) and (2).**
   `DisposableStack.prototype.adopt` (`disposable.rs:195-213`) and
   `AsyncDisposableStack.prototype.adopt` (`disposable.rs:693-710`) each build a
   `wrapper_fn = interp.create_function(JsFunction::native(..., move |interp2, _, _| {
   interp2.call_function(&wrapper_dispose, &UNDEFINED, &[wrapper_val]) }))` and store only
   `dispose_method: wrapper_fn` in the resource — `wrapper_val` (the adopted handle) and
   `wrapper_dispose` (the `onDispose` callback) live only inside the native closure's `Rc<dyn Fn>`
   capture. `trace_object_fields`'s doc comment is explicit that this capture shape is invisible to
   the tracer unless pinned (`gc.rs:284-297`) — exactly the bug #812 (commit `7246d7f6`) already
   fixed once for a different synthetic wrapper
   (`async_from_sync_dispose_method`, `dispose.rs:396-406`, fixed via one `self.pin_native_root(&wrapper,
   &pinned_method)` call). Neither `adopt()` site pins its captures, so fix (1) rooting the wrapper
   function object itself is not enough — what it captures stays unreachable. This is live in the
   cited repro today: `DisposableStack-patterns.js`'s `stack3.adopt(handle, function(h) {...})`
   passes its callback inline with no outer binding, so only the (missing) pin protects it.
   (Note for the implementer: `wrapper_val`/`wrapper_dispose` are themselves moved into the `move`
   closure and so are gone from the outer scope once `create_function` is called — the pin calls
   must use the still-in-scope originals, `&value` and `&on_dispose`, which are clones of the same
   object ids. See §3's exact call for the two-line fix.)

None of these are spec ambiguity — see §2 — they are purely engine-internal reachability defects.

**Scope note for the issue:** the issue's title and body name only gap (1). Gaps (2) and (3) were
found by reading the disposal call paths while verifying the fix actually clears the cited
stress failure; both are necessary for `DisposableStack-patterns.js` to pass under
`JSSE_GC_STRESS`, so cutting them would leave the issue's own acceptance evidence red. The
implementation stage should post a `gh issue comment 798` noting the scope grew from one
`gc.rs` match arm to three fixes (two in `disposable.rs`), with a one-line pointer to this
plan's §1, so a reviewer isn't surprised by the diff's shape.

This is the second half of a pair of gaps in the same feature area: #812 fixed the analogous
*environment-level* `dispose_stack` populated by `using`/`await using` declarations
(`Environment.dispose_stack`, traced in `collect_env_roots`, `gc.rs:1249-1254`). That
environment-level disposal loop already runs through `DisposeCursor`
(`exec.rs:2816`, `self.run_dispose_cursor_blocking(...)`), so #812 only needed gap (1)'s
counterpart — it never had gap (2), because it never had a bespoke unrooted loop, and `using`
declarations have no `adopt()` equivalent, so never had gap (3) either.
`DisposableStack.prototype.dispose`/`.adopt` are where all three gaps actually live.

## 2. Spec basis

Explicit Resource Management (`DisposableStack`, `AsyncDisposableStack`, `AddDisposableResource`,
`DisposeResources`, `GetDisposeMethod`) is **not yet in the pinned `spec/` submodule** (confirmed:
no `DisposableStack`/`AddDisposableResource`/`DisposeResources` hits anywhere under `spec/`). This
is the same situation already documented in-repo by the sibling fix
(`test262-extra/await-using-sync-dispose-fallback-gc-rooting.js`,
`test262-extra/await-using-dispose-suspended-gc-rooting.js`): the proposal is test262-covered
(`test262/test/built-ins/DisposableStack/`, `test262/test/built-ins/AsyncDisposableStack/`,
confirmed present in the submodule) and already implemented end to end by this engine, so those
test262 directories and the proposal's own clause names are the normative reference, not a
stand-in for it.

Governing clauses (by `esid`, Explicit Resource Management proposal):
- `sec-disposablestack.prototype.use`, `sec-disposablestack.prototype.adopt`,
  `sec-disposablestack.prototype.defer` (and the `AsyncDisposableStack` equivalents) — populate
  `disposeCapability.[[DisposableResourceStack]]` (this engine's `DisposableStackData.stack`) with
  `{ [[ResourceValue]], [[Hint]], [[DisposeMethod]] }` records. For `adopt`, the spec's own
  `[[DisposeMethod]]` *is* a closure over the adopted value and the user callback — quoting the
  `info:` block of `test262/test/built-ins/DisposableStack/prototype/adopt/adds-value-onDispose.js`
  (step 5, verified directly since the proposal text itself isn't in the pinned `spec/` submodule):
  "Let closure be a new Abstract Closure with no parameters that captures value and onDispose and
  performs the following steps when called" — i.e. the spec already models `adopt` as producing
  exactly the kind of closure this engine must keep fully reachable, which is what gap (3) violates
  at the engine level.
- `sec-disposablestack.prototype.dispose` / `sec-asyncdisposablestack.prototype.disposeasync` —
  invoke `DisposeResources`, which walks the list in reverse and calls each `[[DisposeMethod]]`
  against each `[[ResourceValue]]` — the abstract operation requires every not-yet-visited record
  to stay intact partway through the walk, which gap (2) violates at the engine level.
- `sec-disposablestack.prototype.move` — transfers `[[DisposableResourceStack]]` wholesale to a
  freshly created target stack (`disposable.rs:296-346`); the same records keep needing GC roots,
  just under a different object id (covered by fix (1) regardless of which object holds the data).

No clause needs reinterpretation and no JavaScript-observable semantics change: this is a pure
engine-internal reachability fix.

## 3. Files to touch

- `src/interpreter/gc.rs` — fix (1): move `ObjectKind::DisposableStack(_)` out of the no-op arm
  (`gc.rs:1006-1013`) into its own arm that roots `d.stack[*].value` and `d.stack[*].dispose_method`
  via the existing `Self::collect_value_roots`. Add a unit test to the existing `mod tests` block
  (alongside `trace_object_fields_roots_array_elements_and_native_roots`, `gc.rs:1332-1341`).
- `src/interpreter/builtins/disposable.rs`:
  - fix (2), in `disposable_stack_dispose` (`disposable.rs:417-480`): wrap the dispose loop in
    `self.with_gc_root_scope(...)`. Root every taken resource's `value` and `dispose_method` via
    `gc_root_value` *before* the loop runs (mirroring `DisposeCursor::for_each_value`,
    `dispose.rs:202-209`). Inside the loop, on `Completion::Throw(e)`: root `e` itself via
    `gc_root_value` *before* calling `wrap_suppressed_error` (`wrap_suppressed_error` calls the
    user-replaceable global `SuppressedError` constructor via `call_global_constructor`, which can
    run arbitrary user code, so `e` needs its own root for that call, not just the result), then
    root the wrapped result the same way `DisposeCursor::record_error` does
    (`dispose.rs:220-224`). This goes one call further than `record_error` itself — `record_error`
    roots only its own return value, not its `error`/`current_error.take()` inputs going into
    `wrap_suppressed_error`. That's a narrower, pre-existing gap shared by every
    `SuppressedError`-wrapping call site in `dispose.rs`, not something introduced by or specific to
    `DisposableStack`; this plan does not audit or change `record_error`/`wrap_suppressed_error`
    themselves (out of scope, see §7), it just has the new code avoid the same narrow gap at no
    extra cost. The loop's control flow otherwise — reverse order, `SuppressedError` accumulation,
    the `Completion::Exit` short-circuit for issue #242 — is unchanged; only root coverage is
    added. `with_gc_root_scope`'s truncate-on-every-exit contract (`mod.rs:1475-1495`) means an
    early return for `Completion::Exit` from inside the scope still unroots correctly.
  - fix (3), in both `DisposableStack.prototype.adopt` (`disposable.rs:195-213`) and
    `AsyncDisposableStack.prototype.adopt` (`disposable.rs:693-710`): add
    `interp.pin_native_root(&wrapper_fn, &value); interp.pin_native_root(&wrapper_fn,
    &on_dispose);` immediately after `create_function`, before the wrapper is stored as
    `dispose_method` — the exact pattern #812 used for `async_from_sync_dispose_method`
    (`dispose.rs:396-406`). Pin `&value`/`&on_dispose` (the method's own parameters, still in
    scope), not `&wrapper_val`/`&wrapper_dispose` — those two are moved into the closure by
    `create_function`'s `move |...|` and are gone from the outer scope by the time
    `create_function` returns; `value`/`on_dispose` are clones of the same object ids and are what
    pin correctly.
- `test262-extra/DisposableStack-use-adopt-defer-gc-rooting.js` (new) — `$262.gc()` regressions for
  all three gaps, covering `DisposableStack.dispose()` and `AsyncDisposableStack.disposeAsync()`,
  modeled on `test262-extra/await-using-dispose-suspended-gc-rooting.js`.
- No `docs/adr/` entry: this conforms to already-documented GC architecture (`CLAUDE.md`'s
  "Kind-specific roots are derived from `ObjectKind` via an exhaustive match in
  `gc::trace_object_fields`", and the existing `with_gc_root_scope`/`gc_root_value`/
  `pin_native_root` idioms used by `DisposeCursor` and by #812); it doesn't change the
  architecture, it applies it where it was missing.
- No `CONTEXT.md` change: no new vocabulary.

## 4. TDD slices

General rule for every slice below: write the test section first and **run it against the
binary before applying that slice's production fix**, and look at the failure (not just trust
that it would fail) — a GC regression that passes whether or not the fix is present proves
nothing. Only once the red is actually observed does the fix go in.

1. **Red (observed): Rust unit test proving gap (1).** In `src/interpreter/gc.rs`'s `mod tests`,
   add `trace_object_fields_roots_disposable_stack_resources`: build a `JsObjectData` whose `kind`
   is `ObjectKind::DisposableStack(DisposableStackData { stack: vec![DisposableResource { value:
   obj(30), hint: DisposeHint::Sync, dispose_method: obj(31) }], disposed: false })`, call
   `Interpreter::trace_object_fields`, assert `as_set(worklist) == vec![30, 31]`. Run it: confirm
   it fails (empty worklist) before touching `gc.rs`'s match arm.
2. **Red (observed): test262-extra, pre-disposal window, `use()`/`defer()` only.** Before applying
   fix (1), add this section to the new file
   (`features: [explicit-resource-management, host-gc-required]`): register one resource via
   `.use()` (an inline object with `[Symbol.dispose]`, no outer binding) and one via `.defer()`
   (an inline callback, no outer binding) on a `DisposableStack`; call `$262.gc()` once, *after*
   registering and *before* `.dispose()`; assert both side effects fire in LIFO order. Repeat for
   `AsyncDisposableStack` + `await stack.disposeAsync()`. Run it against the still-unfixed binary
   (same binary slice 1's red came from): confirm red for both stacks.
3. **Green: fix (1).** Add the `ObjectKind::DisposableStack(d)` arm described in §3, next to the
   existing `ObjectKind::BoundFunction(b)` / `ObjectKind::Proxy(p)` arms that already destructure
   `&obj.kind` directly in this function. Slice 1 and slice 2 both go green; no other arm changes.
   (Slice 2 has no fix of its own beyond this — it exists to prove fix (1) end-to-end for the two
   registration paths it fully covers.)
4. **Red (observed): test262-extra, pre-disposal window, `adopt()`.** Same file, new section:
   `.adopt()` an inline handle with an inline callback (both with no outer binding) on a
   `DisposableStack`; `$262.gc()` before `.dispose()`; assert the callback fires with the right
   handle identity. Repeat for `AsyncDisposableStack` + `disposeAsync()`. Run against the slice-3
   binary (fix (1) only): confirm still red for both stacks — fix (1) roots the wrapper function
   object, not what it captures, so this isolates gap (3) from gap (1).
5. **Green: fix (3).** Add the two `pin_native_root` calls described in §3 (using `&value`/
   `&on_dispose`), in both the sync and async `adopt()`. Slice 4 goes green for both stacks.
6. **Red (observed): test262-extra, mid-disposal window, resource survival (gap 2a).** Same file,
   new section: on a fresh `DisposableStack`, `.defer()` a first callback that is the sole
   reference to itself and only records a marker, then `.defer()` a second callback that calls
   `$262.gc()` as its first statement and then records its own marker. Call `.dispose()`. Disposal
   runs in reverse, so the second (GC-triggering) callback runs first; the first callback —
   reachable at that point only through the already-taken, unrooted Rust `Vec` — runs after the
   GC. Assert both markers recorded, in order, with no `TypeError` from a recycled arena id. Run
   against the slice-5 binary: confirm red.
7. **Red (observed): test262-extra, mid-disposal window, error survival (gap 2b) — exact scenario
   matters.** A GC that fires before any error exists can't test error-rooting at all, and a GC
   that fires while only the *about-to-be-wrapped* error is live tests gap 2a again, not 2b. Use
   three `.defer()` calls, in this registration order, on a fresh `DisposableStack`:
   - `A`: `throw new RangeError('first')` — registered first, so disposed *last*.
   - `B`: calls `$262.gc()` only, no throw — registered second, disposed second-to-last.
   - `C`: `throw new RangeError('second')` — registered last, so disposed *first*.

   Disposal order is `C`, `B`, `A`. `C` throws, so its `RangeError('second')` is sitting in the
   loop's `current_error` local when `B` runs and collects; `A` then throws `RangeError('first')`,
   which `wrap_suppressed_error` chains onto whatever survived in `current_error`. Call
   `.dispose()` inside a `try`; assert the caught error is a `SuppressedError` with
   `e.error.message === 'first'` and `e.suppressed.message === 'second'` (same assertion style as
   `await-using-dispose-suspended-gc-rooting.js`'s error-identity checks). Run against the slice-5
   binary: confirm red.
8. **Green, in two steps, to keep the error-rooting line provably load-bearing:**
   - **8a — resource rooting only:** wrap the loop in `with_gc_root_scope` and root every taken
     resource's `value`/`dispose_method` up front (the first half of fix (2) in §3), *without* yet
     adding the `gc_root_value` calls around `wrap_suppressed_error`. Re-run slice 6: confirm
     green. Re-run slice 7: it should **still be red** — if it's unexpectedly green here, the
     error-rooting half of fix (2) isn't actually tested by slice 7 and the scenario needs
     revisiting before claiming gap (2b) is fixed, rather than assuming slice 7 was right.
   - **8b — error rooting:** add the `gc_root_value(&e)`-before-wrapping and
     `gc_root_value(&wrapped)`-after-wrapping calls (the second half of fix (2) in §3). Re-run
     slice 7: confirm green. Re-run slices 2, 4, and 6 alongside it to confirm nothing regressed.
9. **Confirm, don't fix: `AsyncDisposableStack.disposeAsync()` under the same mid-disposal shapes.**
   Add `disposeAsync()` variants of slices 6 and 7 to the same file. Per the §1 analysis these
   should already be green with no further engine change (the `DisposeCursor`/scheduler path
   already roots `remaining` and `current_error` on every step) — run them to verify that
   prediction empirically rather than assume it; if either is unexpectedly red, that is new
   information requiring a fourth fix, not a pre-existing finding this plan already covers.
10. **Verify, don't re-fix: existing baseline file.** Confirm
    `JSSE_GC_STRESS=1 uv run python scripts/run-test262.py test262-extra/DisposableStack-patterns.js`
    passes after slices 3, 5, and 8 (it was the issue's cited repro). Do not edit that file — the
    new file above is the targeted regression; `DisposableStack-patterns.js` stays a plain
    functional test per existing convention (`#812`/`#816` added dedicated `*-gc-rooting.js` files
    rather than editing the file that first surfaced the bug under stress).

## 5. Test surface

- Targeted test262: `test262/test/built-ins/DisposableStack/` and
  `test262/test/built-ins/AsyncDisposableStack/` (run both directories; no observable-semantics
  change is expected — these are reachability bugs — but they must stay green).
- New test262-extra regression: `test262-extra/DisposableStack-use-adopt-defer-gc-rooting.js`
  (slices 3/4/6/7/9), run via
  `uv run python scripts/run-test262.py test262-extra/DisposableStack-use-adopt-defer-gc-rooting.js`
  and again under `JSSE_GC_STRESS=1` with the same invocation.
- Existing test262-extra GC-stress coverage to re-check: `test262-extra/DisposableStack-patterns.js`
  (slice 10), `test262-extra/async-disposable-stack-dispose-async-tick-alignment.js`,
  `test262-extra/await-using-dispose-suspended-gc-rooting.js`,
  `test262-extra/await-using-sync-dispose-fallback-gc-rooting.js` — all under
  `JSSE_GC_STRESS=1 uv run python scripts/run-test262.py test262-extra/ --timeout 300`.
- Rust unit tests: `cargo test --release` (slice 1's new test plus the full `gc.rs` test module,
  and `src/interpreter/tests.rs`'s `with_gc_root_scope_truncates_on_every_exit`
  (`tests.rs:5410`), which already exercises the scope-truncation contract fix (2) relies on).
- Root-stack discipline: fix (2) adds `gc_root_value`/`with_gc_root_scope` calls and fix (3) adds
  `pin_native_root` calls — exactly the mechanisms `gc_assert_root_depth` and the
  `release-checked` profile exist to police — so run `cargo build --profile release-checked` and a
  targeted `run-test262.py --binary target/release-checked/jsse` pass over `test262-extra/` (the
  new file plus the existing dispose-adjacent files above) to catch any imbalance the debug
  assertions would flag.
- Not covered by test262 (needs the new test262-extra file): the GC-reachability guarantees
  themselves — test262 has no concept of forcing a GC, so all three gaps can only be expressed via
  `$262.gc()` + `host-gc-required`, i.e. in `test262-extra/`.

## 6. Regression risk

- `trace_object_fields` runs for every live object on every major/minor collection — the hottest
  GC path in the engine. Fix (1) is strictly additive and scoped to one `ObjectKind` variant:
  non-`DisposableStack` objects take the exact same path as before. `collect_value_roots` is
  already a no-op on non-object `JsValue`s (exercised today via `collect_env_roots` on the same
  `DisposableResource` shape), so primitive-valued resources (e.g. `stack.use(null)`, covered by
  `DisposableStack-patterns.js`) cost one cheap check and push nothing.
- Fix (2) adds `gc_root_value`/`gc_unroot_frame` calls around an existing loop; `with_gc_root_scope`
  already guarantees balanced push/pop on every exit path, so this does not risk the
  `gc_assert_root_depth` debug-assertions tripping elsewhere — it specifically *fixes* an
  under-rooting bug, it doesn't add a new unbalanced path.
- Fix (3)'s `pin_native_root` calls "only ever accumulate" on the wrapper function object
  (`gc.rs:301`), and each `adopt()` call allocates a fresh wrapper, so there is no risk of
  unbounded accumulation on a long-lived anchor — each anchor gets exactly two pins, once, for its
  own lifetime.
- Checked and ruled out as a separate concern: the generational write barrier. `ObjectHandle::
  borrow_mut` (the path every `use`/`adopt`/`defer`/`dispose` mutation in `disposable.rs` goes
  through — confirmed no call site uses `borrow_mut_untracked`) already runs the conservative
  object-level write barrier unconditionally (`gc.rs:299-300`, `341-346`), so a young resource
  value or dispose method pushed onto an old `DisposableStackData` is already remembered for the
  next minor collection. No barrier change is needed alongside fixes (1)/(2)/(3).
- `ObjectKind` is matched exhaustively in `trace_object_fields`; moving one variant between arms
  doesn't add or remove a variant, so compile-time exhaustiveness is unaffected.
- Expected to **not move `test262-pass.txt`**: under normal (non-stress) execution without an
  explicit `$262.gc()`, landing in any of the three vulnerable windows is rare, so no currently
  passing test262 test is likely to depend on the old (buggy) behavior. All three fixes are
  primarily proven by the new deterministic `$262.gc()` regressions and by `JSSE_GC_STRESS`, which
  `CLAUDE.md` already treats as separate from the baseline ("Never `--update-baseline` under
  stress").
- No interaction expected with the bytecode fast path (GC rooting is interpreter-wide, not
  bytecode-specific — `DisposableStack` methods are native built-ins, not compiled), the property
  MOP (`property.rs` doesn't touch `DisposableStackData`), or the Node-compat library harnesses
  (none of the pinned libraries use explicit resource management).
- `DisposableStack.prototype.move` (`disposable.rs:296-346`) builds its target stack's prototype
  via a user-replaceable global lookup (`get_global_var("DisposableStack")` +
  `get_property_on_id(..., "prototype")`), which can run user getter code while the moved
  `Vec<DisposableResource>` is a Rust local — a fourth window of the same general shape, narrower
  than gap (2) only in that `move()` never *calls* a dispose method (it just needs the moved
  resources to survive until the new object's `kind` is set, not to stay callable through a
  multi-step loop). A later `dispose()` on the target stack would still need whatever survived.
  Flagged as a follow-up (§7), not fixed here, since it is a distinct code path from both gap (2)'s
  loop and gap (3)'s adopt wrappers.

## 7. Out of scope

- **Consolidating `DisposableStack.prototype.dispose` onto `DisposeCursor`/
  `run_dispose_cursor_blocking`** (the same machinery `exec.rs:2816` already uses for synchronous
  `using` block disposal). That would fix gap (2) by reuse instead of by adding rooting calls to
  the bespoke loop, and would remove duplicated `SuppressedError`/`Completion::Exit` handling — but
  verifying its completion semantics are a byte-for-byte match for the sync, no-`Await` case is
  more surface area than this bug fix needs, and "many small changes beat one large change" argues
  for doing it as a separate, deliberate refactor PR, not folded into a GC correctness fix. Noted
  here so a future pass (or the architecture-deepening backlog) can pick it up.
- No dedicated regression for `DisposableStack.prototype.move()`'s narrower window (§6's last
  bullet) — if it needs one, that's a follow-up issue.
- **No audit or fix of `DisposeCursor::record_error`/`wrap_suppressed_error` themselves** (§3's fix
  (2) note): `record_error` (`dispose.rs:220-224`) roots only its own return value, not the
  `error`/`current_error.take()` inputs it passes into `wrap_suppressed_error`, which calls the
  user-replaceable `SuppressedError` constructor and so can run arbitrary code. If that's a live
  gap, it's shared by every `using`/`await using` disposal path already on `main`, not introduced
  by or specific to `DisposableStack` — a separate issue, not this one. This plan's fix (2) simply
  avoids repeating the same narrow gap in its own new code, at no extra cost.
- No change to `DisposableStack-patterns.js` (existing functional test) — left as-is per slice 10.
- No change to `Environment.dispose_stack` rooting (`collect_env_roots`) — already fixed by #812.
- No baseline update (`test262-pass.txt --update-baseline`) — not a `main`-branch operation this
  plan performs.
