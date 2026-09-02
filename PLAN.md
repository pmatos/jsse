# Plan: issue #478 — root the promise combinator capability during synchronous setup

## 1. Problem restated

Every Promise combinator (`Promise.all`, `Promise.allSettled`, `Promise.any`, `Promise.race`, and
the in-repo `Promise.allKeyed` / `Promise.allSettledKeyed`) builds a `PromiseCapability` (`cap.promise`,
`cap.resolve`, `cap.reject`) and then runs a stretch of spec-mandated, user-observable operations —
`Get(C, "resolve")`, `GetIterator`/`[[OwnPropertyKeys]]`, `IteratorStep`/`IteratorValue` (or
`[[GetOwnProperty]]`/`Get`), and `C.resolve(nextValue)` — before the capability's functions are ever
handed to something the GC tracer walks (a promise reaction list, via `pin_native_root`, via
`.then()`). During that stretch `cap.promise` / `cap.resolve` / `cap.reject` (and, where the
combinator maintains one, the `value_anchor` object used to pin settled accumulator entries) live
only in Rust locals inside `src/interpreter/builtins/promise.rs`. A `gc_safepoint()` triggered by
any of that user-controllable code — the `Sub.resolve` accessor in the issue's repro, a
`Symbol.iterator` override, a `next()` override, or a `value` getter — can reclaim the capability
before it is ever pinned, silently breaking the combinator (the observable symptom in the repro is
a `TypeError: undefined is not a function` where a spec-conforming engine resolves normally). A GC
must never be observable, and right now it is. This is the synchronous counterpart to #309/#473,
which rooted the same capability only for the *asynchronous* window (once each per-element
resolve/reject closure exists and is pinned) — everything before that first pin is still exposed,
every iteration, not just the first.

## 2. Spec basis

All six combinators funnel through the same two abstract operations, which is why the same defect
shows up in all of them:

- **`NewPromiseCapability(C)`** — spec.html `sec-newpromisecapability`, ECMAScript 2024 §27.2.1.5.
  Constructs `{[[Promise]], [[Resolve]], [[Reject]]}`; already cited under this number in
  `src/interpreter/builtins/promise.rs:18` (`new_promise_capability`'s own doc comment).
- **`GetPromiseResolve(C)`** — spec.html `sec-getpromiseresolve`, nested under `Promise.all`,
  ECMAScript 2024 §27.2.4.1.1. Step 1 is `Let promiseResolve be ? Get(C, "resolve")`, i.e. an
  ordinary `[[Get]]` that can run an accessor — this is exactly the issue's repro (`Sub.resolve` as
  a getter that calls `$262.gc()`).
- **`Promise.all ( iterable )`** — spec.html `sec-promise.all`, §27.2.4.1. Steps: `NewPromiseCapability(C)`
  → `GetPromiseResolve(C)` + `IfAbruptRejectPromise` → `GetIterator(iterable, sync)` +
  `IfAbruptRejectPromise` → `PerformPromiseAll` (§27.2.4.1.2), whose per-element loop does
  `IteratorStepValue` then `? Call(promiseResolve, C, « nextValue »)` before registering the
  per-element resolve/reject functions via `PerformPromiseThen` (§27.2.5.4.1 /
  `sec-performpromisethen` — see the `Promise.race` bullet below for how this number was verified).
- **`Promise.allSettled ( iterable )`** — spec.html `sec-promise.allsettled`, §27.2.4.2. Identical
  shape via `PerformPromiseAllSettled` (§27.2.4.2.1).
- **`Promise.any ( iterable )`** — spec.html `sec-promise.any`, §27.2.4.3, already cited in
  `test262-extra/Promise-any-combinator-gc-rooting.js`. Identical shape via `PerformPromiseAny`
  (§27.2.4.3.1).
- **`Promise.race ( iterable )`** — spec.html `sec-promise.race`, §27.2.4.5 (confirmed by counting
  `emu-clause` siblings under `sec-properties-of-the-promise-constructor`: all §27.2.4.1,
  allSettled §27.2.4.2, any §27.2.4.3, prototype §27.2.4.4, **race §27.2.4.5**, reject §27.2.4.6,
  resolve §27.2.4.7). Identical shape via `PerformPromiseRace` (§27.2.4.5.1); the issue notes race
  has no accumulator, so only `cap.promise` / `cap.resolve` / `cap.reject` are at risk here, up
  until the first `.then()` call. `PerformPromiseThen` (`sec-performpromisethen`, nested under
  `Promise.prototype.then`) is likewise §27.2.5.4.1, not §27.2.5.5.1 — counting siblings under
  `sec-properties-of-the-promise-prototype-object` gives catch §27.2.5.1, constructor §27.2.5.2,
  finally §27.2.5.3 (matching the existing citation in
  `test262-extra/Promise-finally-gc-rooting.js`), **then §27.2.5.4**.
- **`Promise.allKeyed` / `Promise.allSettledKeyed`** — the in-repo implementation of TC39's
  "await-dictionary" Stage-3 proposal (`src/interpreter/builtins/promise.rs:663`,
  `promise.rs:1510`, `promise.rs:1658`). Not in `spec/` (it is a proposal, not yet ECMA-262), but
  `test262/test/built-ins/Promise/allKeyed/` and `.../allSettledKeyed/` already stage its
  conformance tests and fix the `esid` values to use: `sec-promise.allkeyed` /
  `sec-promise.allsettledkeyed` (the two entry points), `sec-performpromiseallkeyed` (the per-key
  loop, the equivalent of `PerformPromiseAll`), and `sec-createkeyedpromisecombinatorresultobject`
  (`build_keyed_result`). The repo's own implementation follows the identical
  `NewPromiseCapability` → `GetPromiseResolve` → `[[OwnPropertyKeys]]`/`[[GetOwnProperty]]`/`Get`
  shape, so it inherits the same rooting gap and the same fix. No new JS syntax or semantics are
  introduced by this fix in either the ECMA-262 combinators or the proposal ones — this is purely
  an engine-internal reachability bug in an existing, already-shipped feature set.

The fix changes no observable JS grammar or semantics: a spec-conforming engine's behavior is
whatever `node` shows in the issue's repro (`resolved:1,2`), and closing this window is what makes
jsse match it. `gc_safepoint()`'s existing call sites (`src/interpreter/exec.rs:37,202`, and the
weak-collection/finalization helpers in `src/interpreter/gc.rs`) already establish that a GC can
run during ordinary statement execution — i.e. during any of the user-code entry points listed
above — which is the mechanism (not the spec clause) that makes the bug reachable.

## 3. Files to touch

- `src/interpreter/builtins/promise.rs` — the only production file. Six private methods need the
  same mechanical change: `promise_all` (~line 1190), `promise_all_settled` (~1329),
  `promise_all_keyed` (~1511), `promise_all_settled_keyed` (~1658), `promise_race` (~1872),
  `promise_any` (~1939).
- `test262-extra/` — new regression tests (see §5). No changes to `test262/` or `spec/` (read-only).
- No `docs/adr/` entry: this is a bug fix applying an existing, already-adopted rooting idiom
  (`gc_root_frame`/`gc_root_value`/`gc_unroot_frame`, already used in
  `src/interpreter/eval/literals.rs:190` and `:1300`) to a file that hadn't adopted it yet. It is
  not a new architectural decision.

## 4. TDD slices

Each slice is red (test written, fails against current `promise.rs`) → green (mechanical rooting
change) → done; no refactor step needed since the fix is a narrow, uniform insertion, not a
redesign.

1. **`Promise.all`, GC inside `Get(C, "resolve")`.**
   Test: extend/confirm the already-drafted `test262-extra/Promise-combinator-sync-setup-gc-rooting.js`
   (present in the workspace from a prior attempt — inspect it first; it already reproduces the
   issue's exact repro via a `resolve` accessor getter on a `Promise` subclass and asserts the
   combined promise still resolves to `[1, 2]`). Pre-fix, `Promise.all.call(Sub, [1, 2])` throws
   synchronously, so `combined` is never assigned; the current draft's `catch` block calls
   `$DONE(error)` but does not `return`, so execution falls through to `combined.then(...)` on
   `undefined` and throws a second, unrelated `TypeError` on top of the real one. Fix the control
   flow (`return $DONE(error);` in the `catch`, or move the `.then()` chaining inside a success path)
   before treating this as the slice's red test, so the pre-fix failure is legibly the assertion
   under test rather than a cascade. Confirm it fails against the interpreter before the fix, then
   make it pass.
   Production code: in `promise_all` (`src/interpreter/builtins/promise.rs:1190`), immediately after
   `let cap = match self.new_promise_capability(constructor) { ... };` succeeds, open a GC frame and
   root the three capability fields:
   ```rust
   let gc_frame = self.gc_root_frame();
   self.gc_root_value(&cap.promise);
   self.gc_root_value(&cap.resolve);
   self.gc_root_value(&cap.reject);
   let result = (|| {
       // existing body: GetPromiseResolve, GetIterator, the per-element loop, all
       // existing `return self.if_abrupt_reject_promise(e, &cap)` early exits included
       // verbatim — `return` inside this closure exits only the closure, mirroring the
       // idiom already used in eval_array_literal (src/interpreter/eval/literals.rs:185-226)
   })();
   self.gc_unroot_frame(gc_frame);
   result
   ```
   Also root `value_anchor` immediately after it is created (`let value_anchor = ...`) inside the
   closure, since it is exposed to the exact same window (created before the loop's first
   `IteratorStep`, only pinned onto `on_fulfilled` at the end of that same iteration).

2. **`Promise.all`, GC inside a `Symbol.iterator` override (`GetIterator` step).**
   Test: new `test262-extra/Promise-all-sync-setup-iterator-gc-rooting.js`. A plain (non-subclassed)
   `Promise.all` call where the iterable's `[Symbol.iterator]` getter or the returned iterator
   object's construction calls `$262.gc()` before yielding any element — reproduces the same window
   without going through the `resolve` accessor path, isolating step 4 of §27.2.4.1 (`GetIterator`).
   Production code: covered by slice 1's frame (no additional code — this slice is here to prove the
   frame protects the whole setup, not just the `resolve` accessor call).

3. **`Promise.all`, GC inside `next()` / a `value` getter (`IteratorStep`/`IteratorValue`) on a later
   iteration.**
   Test: new `test262-extra/Promise-all-sync-setup-nextvalue-gc-rooting.js`. Iterable with 3+
   elements whose custom iterator's `next()` (or the returned result's `value` getter) calls
   `$262.gc()` starting on the *second* call — this specifically exercises the issue's "same
   exposure on later iterations" note (the capability must stay rooted for the whole loop, not just
   until iteration 0's `pin_native_root`).
   Production code: covered by slice 1's frame (function-scoped, not iteration-scoped) — this slice
   is a regression guard against a narrower fix that only rooted before the first pin.

4. **`Promise.allSettled`.**
   Test: new `test262-extra/Promise-allSettled-sync-setup-gc-rooting.js`, mirroring slice 1's
   `resolve`-accessor repro adapted to `Promise.allSettled`.
   Production code: identical frame insertion in `promise_all_settled`
   (`src/interpreter/builtins/promise.rs:1329`), including its `value_anchor`.

5. **`Promise.any`.**
   Test: new `test262-extra/Promise-any-sync-setup-gc-rooting.js`.
   Production code: identical frame insertion in `promise_any` (`src/interpreter/builtins/promise.rs:1939`),
   including its `value_anchor`. (`promise_any` roots `cap.resolve` and `cap.reject`, matching what
   the existing async-window fix in `#473` already pins per element — both must be alive
   synchronously too, since `cap.resolve` is passed straight into `.then()` at the end of each
   iteration.)

6. **`Promise.race`.**
   Test: new `test262-extra/Promise-race-sync-setup-gc-rooting.js`. Per the issue, `race` has no
   accumulator; the only exposure is `cap.promise`/`cap.resolve`/`cap.reject` between capability
   creation and the first `.then(cap.resolve, cap.reject)` call.
   Production code: identical frame insertion in `promise_race` (`src/interpreter/builtins/promise.rs:1872`),
   no `value_anchor` needed.

**Establishing red before slice 1**: build once against the unmodified `promise.rs` and keep that
binary aside (e.g. `cargo build --release && cp target/release/jsse /tmp/jsse-prefix-478`), then run
every one of the seven new tests against it to confirm each fails before writing any production
code. Do this once, up front, rather than per-slice — slices 2, 3, and 7's second file add tests
whose production support is already delivered by an earlier slice in this same list (slice 1's frame
in `promise_all` already covers slices 2–3; slice 7 is one production change covering two tests), so
per-slice "red" would otherwise require repeatedly stashing and restoring the fix, which this
repo's git-safety rules discourage doing casually. Confirming red once against a saved pre-fix
binary, then green incrementally as each slice's production change lands, gets the same TDD
guarantee without the churn.

7. **`Promise.allKeyed` / `Promise.allSettledKeyed`.**
   Test: new `test262-extra/Promise-allKeyed-sync-setup-gc-rooting.js` and
   `Promise-allSettledKeyed-sync-setup-gc-rooting.js`. Drive the GC via `[[OwnPropertyKeys]]`
   (a Proxy `ownKeys` trap on the `promises` argument), `[[GetOwnProperty]]` (a Proxy
   `getOwnPropertyDescriptor` trap), and a `Get` accessor for one of the enumerable keys — these are
   this pair's equivalents of `GetIterator`/`IteratorStep`/`IteratorValue`.
   Production code: identical frame insertion in `promise_all_keyed`
   (`src/interpreter/builtins/promise.rs:1511`) and `promise_all_settled_keyed` (`:1658`), including
   their `value_anchor`s.

Each slice's test must fail on the pre-fix binary and pass after that slice's production change
before moving to the next slice — build with `cargo build --release` between slices (`gc_safepoint`
frequency and mark/sweep timing are debug/release-sensitive, and this bug is specifically about GC
timing, so a debug build could hide or spuriously trigger it).

## 5. Test surface

- **Targeted test262 run** (regression check, not new coverage — test262 has no `host-gc-required`
  reproduction of this specific window):
  - `uv run python scripts/run-test262.py test262/test/built-ins/Promise/all/`
  - `uv run python scripts/run-test262.py test262/test/built-ins/Promise/allSettled/`
  - `uv run python scripts/run-test262.py test262/test/built-ins/Promise/any/`
  - `uv run python scripts/run-test262.py test262/test/built-ins/Promise/race/`
  - `uv run python scripts/run-test262.py test262/test/built-ins/Promise/allKeyed/`
  - `uv run python scripts/run-test262.py test262/test/built-ins/Promise/allSettledKeyed/`
- **New `test262-extra/` tests** (per TDD slices above) — test262 itself has no host-GC-triggered
  combinator tests of this shape (the existing `host-gc-required` feature tests in test262 proper,
  if any, don't target this exact synchronous-setup window), so this is exactly the kind of
  spec-correct-but-uncovered behavior `test262-extra/` exists for. Follow the frontmatter pattern
  already established by `Promise-all-combinator-gc-rooting.js` / `Promise-any-combinator-gc-rooting.js`
  (`flags: [async]`, `features: [host-gc-required]`, `esid` set to the combinator's own section,
  `info:` citing the exact spec steps under test). Run the whole batch with:
  `uv run python scripts/run-test262.py test262-extra/`
  (per `[[memory: run-test262-extra-tests]]`, there is no dedicated runner — pass the directory to
  the same script used for `test262/`).
- **Full regression gate before opening the PR**: `uv run python scripts/run-test262.py` (default
  `language/`, `built-ins/`, `annexB/`, `intl402/`) plus `cargo test --release` (per
  `[[memory: fmt-hook-clippy-gate]]`, this crate is bin-only — `cargo test --bin jsse`). Do **not**
  pass `--update-baseline`; the baseline stays whatever `origin/main:test262-pass.txt` says, per
  this repo's baseline policy.

## 6. Regression risk

- **GC rooting correctness, not tree-walker hot paths.** The change touches none of `eval_expr`,
  `exec_statement`, or the property MOP in `property.rs` — it only adds `gc_root_value`/
  `gc_root_frame`/`gc_unroot_frame` calls around existing control flow in six `promise.rs` methods.
  The main risk is a leaked or unbalanced frame: every early `return` inside the wrapped closure
  must stay inside the closure boundary (verified per-function while doing the mechanical wrap —
  `promise_all_keyed`/`promise_all_settled_keyed` have `continue` statements inside their `for`
  loops that must keep targeting that `for`, not get near a stray closure boundary that would change
  their meaning; confirmed during exploration that no `break`/`continue` in any of the six functions
  crosses the intended closure boundary).
- **`gc_temp_roots` growth.** `gc_root_frame`/`gc_unroot_frame` is O(1) truncation (already used this
  way in `eval/literals.rs`), and this change adds at most 4 entries (`cap.promise`, `cap.resolve`,
  `cap.reject`, `value_anchor`) per combinator call, unrooted in one bulk truncate at the end — no
  unbounded growth across loop iterations, unlike a naive per-iteration root.
  This is a genuine behavior change: **before** the fix, per-iteration re-roots would be needed and
  weren't there; **after**, a single root pinned once at function entry is live for the whole call,
  which is strictly more correct and no more expensive asymptotically (`gc_temp_roots` is a flat
  `Vec<u64>`, so 4 more entries per outstanding combinator call is negligible next to iteration
  count already driving `results`/`errors`/`values` Vec growth).
- **`test262-pass.txt` baseline.** Should not move at all in the failing direction — this closes a
  gap where jsse was non-conformant (it should never have thrown in the repro); if anything, it can
  only newly pass tests that happened to trigger a GC during combinator setup and previously failed
  spuriously (unlikely, since `test262/` proper doesn't run with forced-GC timing the way
  `test262-extra/`'s `host-gc-required` tests do — most likely zero movement in `test262/` pass
  count). Per this repo's rules, do not run `--update-baseline` regardless of the direction moved.
- **No interaction with the #465 persistent-root truncation hazard.** #465 (closed) documented that
  `gc_temp_roots` used to carry two incompatible lifetimes — frame-scoped temporaries reclaimed by
  `gc_unroot_frame`, and persistent async-completion roots (`$262.agent.getReportAsync`,
  `Atomics.waitAsync`) that needed to survive past the enclosing frame — and an enclosing
  `gc_unroot_frame` could truncate a persistent root out from under a still-pending async
  completion. Re-checked for this plan: neither `mod.rs` (`getReportAsync`) nor
  `builtins/atomics.rs` (`waitAsync`) pushes onto `gc_temp_roots` anymore, and grepping every
  production `gc_temp_roots.push`/`gc_root_value` call site (`mod.rs:1047`, `exec.rs:1286`,
  `eval.rs:1044,4310`, `builtins/array.rs:347`, `builtins/iterators.rs` (for-of/spread iterator
  protection), `builtins/regexp.rs:8980`) shows every one is a locally-scoped frame temporary,
  balanced by that same call's own `gc_unroot_frame` or natural truncation — none is a
  survive-past-this-frame persistent root. So the new `gc_root_frame`/`gc_unroot_frame` pair in
  each combinator does not reintroduce that hazard; it is the same idiom already load-bearing in
  `eval/literals.rs` and now several other production sites, all coexisting safely today.
- **Interaction with #473's async-window fix.** The existing `pin_native_root(&on_fulfilled, &cap.resolve)`
  / `pin_native_root(&on_fulfilled, &value_anchor)` calls inside each loop body are unchanged — this
  fix only adds protection for the time *before* those pins fire (and reinforces it across every
  loop iteration, not just the first), so the two fixes are additive, not overlapping. No existing
  pin call needs to move or be removed.
- **Only `promise.rs`'s six combinator entry points are affected** — `Promise.resolve`,
  `Promise.reject`, `Promise.prototype.then`/`catch`/`finally`, and the constructor itself are
  untouched, so no risk to the rest of the Promise surface.

## 7. Out of scope

- **The RAII frame guard from #331** (`gc-rooting-ergonomics-raii-frame-guard`). The issue suggests
  landing it first "to make this fix mechanical," but #331 hasn't merged and introducing a new
  rooting abstraction is a separate refactor with its own review surface. This plan uses the
  primitives #331 would wrap (`gc_root_frame`/`gc_root_value`/`gc_unroot_frame`), which already
  exist and are already the established idiom (`eval/literals.rs`) — landing #331 later can migrate
  this code onto the guard as a pure refactor with no behavior change, but is not a prerequisite.
- **Rooting `p` (the per-element resolved promise), `on_fulfilled`/`on_rejected`, and `then_fn`
  between their own creation and use.** A GC during `Get(p, "then")` (e.g. if `p`'s prototype chain
  has a user-defined `then` accessor) is arguably the same defect class, but the issue text scopes
  the report specifically to the capability (`cap.promise`/`cap.resolve`/`cap.reject`) and its
  accumulator entries, not to these per-iteration locals. Bundling that in would widen this PR well
  past "the minimal first slice that closes the issue." Flag as a candidate follow-up issue if it
  survives triage, but do not fix it here.
- **A GC inside the general-case constructor call in `new_promise_capability` itself.** For a
  non-built-in `C`, `new_promise_capability` (`src/interpreter/builtins/promise.rs:79`) calls
  `self.construct(constructor, &[executor])`, which runs the subclass's own constructor body —
  synchronous, user-authored code (e.g. `constructor(e) { super(e); $262.gc(); }`). At the point
  that code runs, the executor has already written `resolve`/`reject` into `resolve_slot`/
  `reject_slot` (`Rc<RefCell<JsValue>>` locals the GC tracer cannot see), one call frame earlier
  than the window this issue describes. Same defect class, same root cause (a value alive only in
  an untraced Rust closure capture), but it lives inside `new_promise_capability` rather than in any
  of the six combinator bodies this plan touches, so fixing it is a separate, narrower change (root
  `resolve_slot`/`reject_slot`'s contents for the duration of the `construct()` call) with its own
  review surface. Noting it here so it is visibly considered, not missed — recommend filing it as a
  sibling follow-up issue rather than folding it into this PR.
- **Rooting the combinator's own arguments (`constructor`, `iterable`, `promises`).** Whatever
  mechanism already keeps call arguments alive across a native function invocation is existing,
  accepted behavior untouched by this issue; not revisited here.
- **Any change to `test262-pass.txt`.** Read-only for this branch; rolling it forward is a
  `main`-branch operation per this repo's conventions.
- **Formatting/cleanup unrelated to the six touched functions** — e.g. no drive-by renames or
  comment rewrites elsewhere in `promise.rs`.
