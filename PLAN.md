# Plan: issue #795 — AsyncFromSyncIterator wrapper captures its sync iterator in untraced native closures

## 1. Problem restated

`create_async_from_sync_iterator` (`src/interpreter/builtins/iterators.rs:4592`) builds the
Async-from-Sync Iterator wrapper object's `next`/`return`/`throw` methods as native Rust
closures (`JsFunction::native`) that capture `sync_iter` (and, for `next`, `cached_next`) by
`move`. Those captures live inside an `Rc<dyn Fn>`, which `trace_object_fields` cannot walk —
the only way the GC learns about a value a native closure holds is an explicit
`pin_native_root` (or equivalent `gc_native_roots` entry) on some object the tracer does visit.
Nothing pins `sync_iter`/`cached_next` on the wrapper, so once the wrapper itself is the *only*
reachable reference to the sync iterator (the normal case — nothing else in user code holds a
second reference to `[1,2,3][Symbol.iterator]()`), a collection at a safepoint between method
calls sweeps the sync iterator and/or its cached `next`, and the next wrapper method call reads
a dead id. The same capture-without-root bug recurs in `async_from_sync_continuation`'s
`onFulfilled`/`onRejected` reaction closures (`iterators.rs:4734`) and in
`async_from_sync_dispose_method`'s dispose-wrapper closure (`src/interpreter/dispose.rs:396`).
`JSSE_GC_STRESS` forces collections at safepoints that would not otherwise collect, which is why
normal runs rarely see this but stress mode reliably does.

## 2. Spec basis

- **§27.1.2 Async-from-Sync Iterator Objects**, `sec-createasyncfromsynciterator`
  (`CreateAsyncFromSyncIterator`) — defines the `[[SyncIteratorRecord]]` internal slot the
  wrapper carries; this is exactly the state `sync_iter`/`cached_next` represent in the engine's
  native-closure encoding (there is no internal-slot storage for native-only objects here, so
  the engine encodes the slot as closure captures instead — a representation choice, not a
  spec deviation, but one that makes the values invisible to the GC unless explicitly pinned).
- **§27.1.2.1 `%AsyncFromSyncIteratorPrototype%.next`**, **§27.1.2.2 `.return`**,
  **§27.1.2.3 `.throw`** — each reads `[[SyncIteratorRecord]]` to drive `IteratorNext`/
  `GetMethod(syncIterator, "return"/"throw")`; this is the `sync_iter`/`cached_next` the three
  wrapper closures capture. The fix does not change any of these steps — it only makes the
  engine's existing encoding of `[[SyncIteratorRecord]]` survive a collection.
- **§27.1.2.4 `AsyncFromSyncIteratorContinuation`** — step 6 (`IteratorClose` on abrupt
  `valueWrapper` when `closeOnRejection`) and the `onRejected` closure built from `closeIterator`
  (captures `syncIteratorRecord`) are exactly the `on_rejected`/`sync_for_close` capture in
  `async_from_sync_continuation`; `onFulfilled`'s closure captures `promiseCapability` (here,
  `outer_promise`) to resolve. Both closures' captures need the same pinning treatment.
- **`sec-getdisposemethod`** (`GetDisposeMethod`, Explicit Resource Management proposal; already
  the `esid` the codebase uses for this code — see `test262-extra/await-using-sync-dispose-fallback-ignores-returned-promise.js`).
  Note: this clause is not present in the current `spec/` submodule pin (ecma262 commit
  `270a490b`); Explicit Resource Management has not landed there yet even though test262 already
  ships `language/statements/using/`, `language/statements/await-using/`, and
  `staging/explicit-resource-management/`, and this engine already implements it end to end
  (`dispose.rs`, `disposable.rs`). The closure built by step b.ii.1 (captures `method`) is
  exactly `async_from_sync_dispose_method`'s capture.

No syntax or semantics change: every fix is "pin a value the engine already computes correctly
so the GC does not collect it out from under a live native closure." Observable behavior is
identical in the no-collection case (today's passing tests) and becomes spec-correct in the
collection case (today's `JSSE_GC_STRESS` failures), where it currently throws a spurious
`TypeError: Iterator result is not an object` or similar that the spec does not call for.

## 3. Files to touch

Engine:
- `src/interpreter/builtins/iterators.rs`
  - `create_async_from_sync_iterator` (~4592–4731): pin `sync_iter` and `cached_next` on the
    wrapper object, right after `cached_next` is computed and before/after the three
    `define_method` calls (order doesn't matter — pinning only needs the wrapper's id, which
    exists from the top of the function).
  - `async_from_sync_continuation` (~4734–4827): pin `outer_clone1` on `on_fulfilled`; pin
    `outer_clone2` (both branches) and `sync_for_close` (the `!done_bool && close_on_rejection`
    branch only) on `on_rejected`.
- `src/interpreter/dispose.rs`
  - `async_from_sync_dispose_method` (~396–406): clone `method` before it is moved into the
    closure, create the wrapper function, then `pin_native_root(&wrapper, &method)` before
    returning.

No non-engine files change. No `docs/adr/` entry — `CONTEXT.md` already defines **Pinned Native
Root** (the exact mechanism this fix applies) and **Rooted Slot** (the mechanism this fix
deliberately does *not* need, since none of these captures are reassigned after construction —
each closure's capture is fixed at creation, matching the "pin once, fixed set" rule
`pin_native_root`'s own doc comment states). This is applying an existing, documented pattern to
a site that was missed, not a new architectural decision.

## 4. TDD slices

1. **Repro script, not yet a committed test.** Run the issue's literal repro
   (`for await (const x of [1,2,3]) { $262.gc(); }` via `jsse -e` with a `--prelude` or inline
   `$262` shim, or directly through `scripts/run-test262.py` on a throwaway file) against the
   release binary to confirm today's behavior: a `TypeError` (or silently wrong iteration) once
   a collection runs between `next()` calls with no other reference to the array iterator held.
   This step is diagnostic only — no file is added to the repo yet.
2. **Wrapper `next`/`return` pinning (closes the issue's literal repro).**
   Red: add `test262-extra/AsyncFromSyncIterator-wrapper-gc-rooting.js` (see §5) with a case
   that iterates a plain array literal via `for await` and calls `$262.gc()` in the loop body
   after each value, plus a `break` case to exercise `.return()`. Confirm it fails under
   `JSSE_GC_STRESS=1` (and, ideally, deterministically — `$262.gc()` forces a collection
   immediately, so it should fail even at stress period effectively infinite, i.e. without the
   env var at all, as long as nothing else roots the sync iterator).
   Green: in `create_async_from_sync_iterator`, pin `sync_iter` and `cached_next` on the wrapper
   value via `pin_native_root`.
3. **Wrapper `throw` pinning.** Extend the same test file (or a case within it) with a sync
   iterable whose `throw` method is invoked (via `for await` + a generator's `.throw()` reaching
   the sync-wrapped delegate, or directly constructing the wrapper's `throw` through
   `Symbol.asyncIterator`-less delegation) with `$262.gc()` immediately before the call. This is
   covered by the same `sync_iter` pin from slice 2 — no separate production change — but add
   the case to lock in that `throw()` doesn't regress.
4. **`AsyncFromSyncIteratorContinuation` reaction closures.** Red: extend the same test file
   with a sync iterator whose `next()` returns `{ done: false, value: <thenable that rejects> }`
   and `$262.gc()` called between the `.next()` call returning and the microtask queue draining
   (`await null; $262.gc();`), checking the iterator's `return()` still gets invoked for cleanup
   (i.e. `IteratorClose` still runs — observable via a side-effecting `return` method) and the
   outer promise still settles with the right rejection. Confirm this fails independently of
   slice 2's fix (it exercises `async_from_sync_continuation`, not the wrapper methods directly).
   Green: pin `outer_clone1`/`outer_clone2`/`sync_for_close` as described in §3.
5. **Dispose-fallback wrapper pinning.** Red: add
   `test262-extra/await-using-sync-dispose-fallback-gc-rooting.js` (see §5): an `await using`
   resource with only a synchronous `[Symbol.dispose]`, with `$262.gc()` forced between the
   `using` declaration's evaluation (where `GetDisposeMethod` wraps the sync method) and the
   block exit that actually calls it (e.g. an `await` in the block body before falling off the
   end). Confirm the disposer still runs and its side effect is observed, under
   `JSSE_GC_STRESS=1`.
   Green: pin `method` on the wrapper in `async_from_sync_dispose_method`.
6. **Full regression sweep.** Run the issue's listed failing-under-stress test262 names plus the
   directories in §5 once more, this time also under plain (non-stress) `cargo test --release`
   and `run-test262.py` to confirm no behavior changed outside the collection case.

## 5. Test surface

Targeted test262 (run via `uv run python scripts/run-test262.py <dir>`, both with and without
`JSSE_GC_STRESS=1`):
- `test262/test/built-ins/AsyncFromSyncIteratorPrototype/` (next/return/throw correctness —
  guards against a pinning change accidentally altering observable semantics)
- `test262/test/language/statements/for-await-of/`
- `test262/test/language/statements/for-of/` (the `head-await-using-*` cases route through
  dispose)
- `test262/test/language/statements/await-using/`, `test262/test/language/statements/using/`
- `test262/test/built-ins/AsyncDisposableStack/`, `test262/test/built-ins/DisposableStack/`
- `test262/test/staging/explicit-resource-management/`
- `test262/test/language/statements/async-generator/` (yield* delegating to a sync iterable)

None of this is rewriting `test262-pass.txt` — just confirming the targeted directories still
pass at (at least) today's baseline count, read from `origin/main:test262-pass.txt`.

Spec-correct behavior not covered by test262 (GC survival is engine-internal, not something
test262 can assert without a `$262.gc()`-driven, deterministic trigger):
- `test262-extra/AsyncFromSyncIterator-wrapper-gc-rooting.js` (new) — `esid:
  sec-createasyncfromsynciterator`, `features: [async-iteration, host-gc-required]`. Covers
  slices 2–4: `next()`/`return()`/`throw()` wrapper survival and
  `AsyncFromSyncIteratorContinuation`'s reaction-closure survival, across an explicit `$262.gc()`
  with nothing but the wrapper (or its in-flight promise) holding the sync iterator.
- `test262-extra/await-using-sync-dispose-fallback-gc-rooting.js` (new) — `esid:
  sec-getdisposemethod`, `features: [explicit-resource-management, host-gc-required]`. Covers
  slice 5: the dispose-fallback wrapper's captured `method` surviving a collection between
  `using` evaluation and the block's exit.

Both follow the existing `<Feature>-gc-rooting.js` naming and structure already used throughout
`test262-extra/` (e.g. `async-generator-await-using-dispose-suspended-gc-rooting.js`,
`async-function-for-of-iterator-gc-rooting.js`): a `host-gc-required`-gated, deterministic
`$262.gc()` call rather than relying on `JSSE_GC_STRESS` sampling, so the regression is caught by
a plain `cargo test --release` / `run-test262.py test262-extra/` run with no env var.

The existing `test262-extra/await-using-sync-dispose-fallback-ignores-returned-promise.js` and
the async-generator/await-using `*-gc-rooting.js` files already listed in the issue as
stress-failing are expected to go green again as a side effect — they are regression evidence,
not new coverage, so they don't need edits.

## 6. Regression risk

- `create_async_from_sync_iterator` and `async_from_sync_continuation` are on the hot path for
  every `for await...of` over a non-async-iterable (arrays, strings, sync generators, custom
  sync iterables), `Array.fromAsync` (`array.rs:3285`), and `yield*` delegating to a sync
  iterable from an async generator — a mistake in the pinning (e.g. pinning the wrong clone, or
  pinning on an anchor that doesn't outlive the closure) would not show up as a compile error or
  even a non-stress test failure, only under `JSSE_GC_STRESS`, so the targeted directories in §5
  must be run under stress, not just normally.
- `pin_native_root` only appends (never un-pins); since each of these closures is created fresh
  per wrapper/per `continuation` call and never replaces an already-pinned value, this matches
  the "pin once, fixed set" discipline the helper's own doc comment requires — no `RootedSlots`
  needed. Double-check no code path re-enters `create_async_from_sync_iterator` or
  `async_from_sync_continuation` for an already-pinned wrapper (it doesn't — each call allocates
  a fresh `wrapper_id`/`outer_promise`).
- GC rooting changes are invisible to the tree-walker's normal control flow and to the bytecode
  fast path (this is all native-builtin code, not compiled AST), so no bytecode-specific testing
  is needed beyond the existing `--bytecode` test262 lane picking up the same test262-extra files.
- `test262-pass.txt` could move if a targeted directory's current pass count depends on
  non-deterministic stress-mode luck in CI — it should not, since the baseline is built from a
  non-stress run, but worth confirming the targeted counts match `origin/main:test262-pass.txt`
  exactly post-fix (not just "no new failures").
- Library-harness regression surface is minimal: `luxon`/`zod`/`moment`/etc. don't lean on
  `for await` over sync iterables in their hot paths, so `./scripts/run-library-tests.sh` is not
  expected to move, but a quick run of any library using async iteration (if any) is cheap
  insurance.

## 7. Out of scope

- Auditing every other native closure in `iterators.rs`/`promise.rs`/`dispose.rs` for the same
  missing-pin pattern. This issue is scoped to the four sites `create_async_from_sync_iterator`,
  `async_from_sync_continuation`, and `async_from_sync_dispose_method` name; a broader GC-rooting
  audit is exactly the kind of work `pm-deepen`'s backlog already tracks incrementally (see
  memory: `gc-root-scope-guard`/`gc-root-scope-guard-eval`/`iterator-helper-argument-prologue`
  lineage) and should stay a separate issue, not get bundled into this bug fix.
- Refactoring `create_async_from_sync_iterator`'s three `define_method` closures to share a
  single `RootedSlots`-based state object instead of three independent clones. The current
  captures are fixed at construction and never reassigned, so plain `pin_native_root` is the
  correct tool per `CONTEXT.md`'s own guidance — introducing `RootedSlots` here would be
  unneeded complexity for a value that's never mutated in place.
- Any change to `async_from_sync_dispose_method`'s or `async_from_sync_continuation`'s control
  flow, error messages, or promise-timing semantics. Only the rooting changes.
- Rewriting `test262-pass.txt` (a `main`-branch operation).
