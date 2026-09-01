# Plan: issue #553 — `Promise.try` wraps a returned promise instead of returning it

## 1. Problem restated

`Promise.try(callback, ...args)` currently always routes the callback's
completion value through `promiseCapability.[[Resolve]]`/`[[Reject]]` and
returns the freshly created `promiseCapability.[[Promise]]`. When `callback`
returns a promise produced by the same constructor as the receiver (e.g.
`Promise.try(() => somePromise)` where `somePromise.constructor === Promise`),
the spec now requires that `somePromise` itself be returned, unwrapped —
mirroring the "avoid wrapping a same-constructor promise" behavior already
used by `Promise.resolve`. jsse instead always wraps, so
`Promise.try(() => sentinel) !== sentinel`, failing
`built-ins/Promise/try/avoids-wrap.js` and
`avoids-wrap-for-subclass.js` (4 scenarios: strict + sloppy × 2 files).

## 2. Spec basis

- `spec/spec.html:49712-49729` (`sec-promise.try`, *Promise.try ( callback,
  ...args )`) — the clause under test. **Important caveat**: the pinned spec
  submodule commit (`270a490b`) predates the "avoids wrap" editorial change
  that test262 (`771005236`, pulled in by #547) already encodes. The
  algorithm text at this pinned commit still unconditionally routes the
  result through `promiseCapability.[[Resolve]]`, which is the exact bug
  behavior — the spec text itself hasn't caught up to the committee decision
  yet. This is a genuine spec/test262 version skew, not a test262 error: the
  issue notes that Node fails these same two tests as well, i.e. this is a
  conformance lag shared by both engines, not a jsse-specific defect. Per
  the project's authority order (spec > test262 > node), and since test262
  is demonstrably ahead here, the fix is grounded not by editing the stale
  algorithm text
  but by composing it with a clause the pinned spec **does** already contain
  in full:
- `spec/spec.html:49690-49709` (`sec-promise-resolve`, abstract operation
  `PromiseResolve ( C, x )`, used by `Promise.resolve` at
  `spec/spec.html:49678-49685`) — already implements exactly the "return `x`
  unwrapped when `x` is a promise and `SameValue(x.[[constructor]], C)`"
  check that `avoids-wrap.js`/`avoids-wrap-for-subclass.js` require. jsse
  already implements this operation faithfully as
  `Interpreter::promise_resolve_with_constructor` (`src/interpreter/builtins/promise.rs:158-187`,
  doc-commented `// PromiseResolve(C, x) - spec 27.2.4.7`).

  The reconstructed algorithm — replacing `sec-promise.try` step 6 (`Perform
  ? Call(promiseCapability.[[Resolve]], undefined, «status.[[Value]]»)` /
  step 7 `Return promiseCapability.[[Promise]]`) with `Return ?
  PromiseResolve(C, status.[[Value]])` — was verified by hand against every
  test in `test262/test/built-ins/Promise/try/` (not just the two failing
  ones), including the ordering-sensitive ones:
  - `ctx-ctor.js` requires the receiver's constructor be invoked **exactly
    once** even when the callback returns a non-promise value. This rules
    out an eager `NewPromiseCapability(C)` followed by a *second*
    capability creation inside `PromiseResolve` on the fallback path — the
    capability must be created lazily, only once, per branch actually
    taken.
  - `ctx-ctor-throws.js` / `ctx-non-ctor.js` require that a receiver whose
    `[[Construct]]` throws (or isn't a constructor at all) still surfaces
    as a **synchronous** throw out of `Promise.try` itself, not a rejected
    promise — satisfied because `NewPromiseCapability(C)` is only ever
    reached through a `?` (ReturnIfAbrupt), never swallowed.
  - `ctx-ctor-for-error.js` requires the receiver's constructor be invoked
    once, and the produced instance be an `instanceof` the receiver, when
    the callback throws — satisfied by the abrupt branch calling
    `NewPromiseCapability(C)` once and returning its promise.
  - `args.js`, `return-value.js`, `throws.js`, `promise.js`, `name.js`,
    `length.js`, `prop-desc.js`, `not-a-constructor.js`,
    `ctx-non-object.js` are all unaffected by this restructuring (they don't
    depend on the wrap/no-wrap distinction) and must continue to pass.

  Reconstructed algorithm (for the implementation stage to transcribe into
  code, not spec prose — the actual spec text is not being edited, since
  `spec/` is read-only):
  ```
  1. Let C be the this value.
  2. If C is not an Object, throw a TypeError exception.
  3. Let status be Completion(Call(callback, undefined, args)).
  4. If status is an abrupt completion, then
     a. Let promiseCapability be ? NewPromiseCapability(C).
     b. Perform ? Call(promiseCapability.[[Reject]], undefined, « status.[[Value]] »).
     c. Return promiseCapability.[[Promise]].
  5. Return ? PromiseResolve(C, status.[[Value]]).
  ```
  Steps 1-2 are copied **verbatim** from the current, undisputed pinned text
  at `spec/spec.html:49716-49717` — they are not part of the wrap/no-wrap
  skew and must run *before* the callback, as their own explicit check, not
  be left to happen implicitly as a side effect of some later
  `NewPromiseCapability` call. This matters because `PromiseResolve`'s
  early-return branch (`sec-promise-resolve` step 1, `spec/spec.html:49702-49704`)
  never calls `NewPromiseCapability` at all — so if the "C is an Object"
  check were only enforced lazily inside capability creation, a
  non-object `C` that (via a crafted `x.constructor` accessor) happens to
  `SameValue`-match could skip the TypeError entirely. Keeping steps 1-2 as
  an explicit, unconditional guard at the top of `try_fn` avoids relying on
  that argument at all.

  Step 3 folds the "callback not callable" case in for free: `Call` on a
  non-callable value is itself a TypeError, which `Completion(...)`
  captures as an ordinary abrupt completion feeding into step 4 — no
  separate `is_callable` pre-check is needed (see Files to touch). Concretely
  this means `Promise.try(undefined)` (good receiver, non-callable
  callback) must still return a **rejected promise**, not throw
  synchronously — the abrupt completion is caught and routed through step
  4's `NewPromiseCapability` + `[[Reject]]`, same as any other callback
  throw. No file in `test262/test/built-ins/Promise/try/` exercises this
  exact combination (`ctx-non-object.js`/`ctx-non-ctor.js` only cover a bad
  *receiver*, not a bad callback on a good receiver), so this is a
  behavior to protect by code review / manual check during implementation,
  not something the existing suite will catch if it regresses.

  On the choice to make capability creation lazy (once, only on the branch
  actually taken) rather than eager (once, always, up front, as the pinned
  spec's own text still has it at step 3 of `sec-promise.try`): **both
  orderings pass all 15 files** in `test262/test/built-ins/Promise/try/`.
  `ctx-ctor.js`'s `callCount === 1` assertion does not by itself force the
  lazy reading — an eager `NewPromiseCapability(C)` followed by
  `PromiseResolve`'s early-return branch (which never constructs again)
  also yields exactly one call. The lazy reading is chosen because it lets
  step 5 delegate to the existing, already-correct
  `promise_resolve_with_constructor` (`PromiseResolve`) verbatim instead of
  duplicating its capability-creation logic inline before the early-return
  check — an implementation simplicity argument, not a spec-mandated one.
  Flagging this explicitly so a reviewer doesn't need to re-derive it from
  `ctx-ctor.js` and conclude (incorrectly) that the ordering was forced.

## 3. Files to touch

- `src/interpreter/builtins/promise.rs` (the `try_fn` native closure,
  currently lines 610-661) — sole production change.
- `README.md` — pass-count/percentage update after the full test262 run,
  per repo convention (implementation-stage step, not a design change).
- No changes to `spec/`, `test262/`, `src/lexer.rs`, `src/parser/**`, or any
  other builtin. `promise_resolve_with_constructor` and
  `new_promise_capability` (both already correct, already spec-cited) are
  reused as-is, not modified.

## 4. TDD slices

1. **Red baseline.** Run
   `uv run python scripts/run-test262.py test262/test/built-ins/Promise/try/`
   before touching code. Confirms the 4 known-red scenarios
   (`avoids-wrap.js`, `avoids-wrap-for-subclass.js` × strict/sloppy) and
   captures the full pass list for the other 13 files in that directory as
   the "must stay green" set.

2. **Green: rewrite `try_fn`.** In `src/interpreter/builtins/promise.rs`,
   replace the current body (eager `new_promise_capability` +
   `is_callable` pre-check + resolve/reject-then-always-return-`cap.promise`)
   with the reconstructed algorithm from §2:
   - Add an explicit, unconditional "if `this` is not an Object, throw
     TypeError" guard at the top, before calling the callback — copied from
     the undisputed `spec/spec.html:49716-49717` steps 1-2, not derived from
     `new_promise_capability`'s internal `is_constructor` check (see §2 for
     why that check alone isn't equivalent).
   - Drop the manual `interp.is_callable(&callback)` check and its bespoke
     `"Promise.try requires a callable"` TypeError — `call_function`'s own
     "is not a function" TypeError (verified at `src/interpreter/eval.rs:6114-6143`)
     already produces a `Completion::Throw` that step 4 handles generically.
     Note for manual verification (no test262 file covers this exact
     combination, see §2): a good receiver with a non-callable callback,
     e.g. `Promise.try(undefined)`, must come out as a **rejected promise**,
     not a synchronous throw.
   - Call `interp.call_function(&callback, &JsValue::UNDEFINED, &call_args)`
     next, with no capability created yet.
   - On `Completion::Throw(e)`: create `cap = interp.new_promise_capability(this)?`,
     call `cap.reject` with `e` (propagating any throw from that call, per
     the `?` in step 4b), return `Completion::Normal(cap.promise)`.
   - On `Completion::Normal(v)`: return
     `interp.promise_resolve_with_constructor(this, &v)` mapped to
     `Completion::Normal`/`Completion::Throw`.
   - Any other `Completion` variant (`Return`/`Break`/`Continue`/`Yield`/
     `TailCall`/`Exit`/`Empty`) is not reachable from a completed
     `call_function` invocation in today's engine except `Exit` (host
     `__host_exit`, `--node` only). The *current* code silently drops that
     case into `_ => {}` and still returns an unresolved `cap.promise`; the
     rewrite must preserve that exact (pre-existing, out-of-scope) behavior
     bit-for-bit rather than newly deciding how `Exit` should propagate
     through `Promise.try` — that is a separate, unfiled concern, not part
     of this issue.
   - Build (`cargo build --release`) and re-run the targeted test262
     directory; confirm `avoids-wrap.js` and `avoids-wrap-for-subclass.js`
     now pass and none of the other 13 files in the directory regress.

3. **Widen the regression check.** Run the full suite:
   `uv run python scripts/run-test262.py` (no `--update-baseline` — that is
   a `main`-branch operation and is explicitly not part of this plan). Also
   run `cargo test --bin jsse` (crate is bin-only, matching the
   fmt/clippy-hook memory note) and `./scripts/lint.sh`. Any newly-red test
   outside `built-ins/Promise/try/` means the restructuring touched shared
   Promise machinery in an unintended way and must be root-caused before
   proceeding — `promise_resolve_with_constructor` and
   `new_promise_capability` are shared with `Promise.resolve`, `.all`,
   `.allSettled`, `.race`, `.any`, `.withResolvers`, `.allKeyed`,
   `.allSettledKeyed`, so a full run (not just the `try/` subdirectory) is
   the real gate, not an optional extra.

4. **Spec-correct behavior test262 doesn't check.** `avoids-wrap.js` and
   `avoids-wrap-for-subclass.js` assert only *identity* (`returnValue ===
   sentinel`) for the matching-constructor case, and no file in the
   directory checks what `PromiseResolve` does when the constructor
   *doesn't* match, nor how many times it reads `x`'s `"constructor"`
   property. Both are grounded directly in the undisputed
   `sec-promise-resolve` text (`spec/spec.html:49701-49708`, not part of the
   wrap/no-wrap skew): step 1.a is a single `? Get(x, "constructor")`, and
   the fallback (step 1's condition false, or `x` not a promise at all)
   must produce a **new** promise that is `instanceof C` and distinct from
   `x`. Add `test262-extra/Promise-try-promise-resolve-constructor-check.js`,
   following the existing flat-file, full-frontmatter convention seen in
   `test262-extra/Promise-all-combinator-gc-rooting.js` (`description`,
   `esid: sec-promise.try`, an `info:` block citing `sec-promise-resolve`
   and issue #553, `flags`/`features` as needed), asserting:
   - A same-realm `Promise` instance with an own accessor property
     `"constructor"` that counts reads and returns the receiver: after
     `Receiver.try(() => thatPromise)`, the getter was invoked **exactly
     once** and the returned value is the same object.
   - `SubPromise.try(() => Promise.resolve(1))` (a genuine promise whose
     `constructor` is the intrinsic `Promise`, not `SubPromise`) returns a
     value that is `instanceof SubPromise` and is **not** the original
     `Promise.resolve(1)` object — the wrap path is still exercised when
     the constructors differ, which no existing `try/` file covers.

## 5. Test surface

- Targeted: `uv run python scripts/run-test262.py test262/test/built-ins/Promise/try/`
  — the direct regression surface for this issue (15 files, all of
  `sec-promise.try`).
- Full run (required, not optional, per slice 3):
  `uv run python scripts/run-test262.py` — because the two helpers this
  change leans on (`promise_resolve_with_constructor`,
  `new_promise_capability`) are shared across most of the `Promise` static
  surface.
- New coverage beyond test262: `test262-extra/Promise-try-promise-resolve-constructor-check.js`
  (slice 4), run via
  `uv run python scripts/run-test262.py test262-extra/Promise-try-promise-resolve-constructor-check.js`
  per the existing no-dedicated-runner convention for `test262-extra/`.
- `cargo test --bin jsse` and `./scripts/lint.sh` as the standard non-test262
  gates.

## 6. Regression risk

- **Shared machinery.** `promise_resolve_with_constructor` and
  `new_promise_capability` are used by essentially every other `Promise`
  static method (`resolve`, `all`, `allSettled`, `race`, `any`,
  `withResolvers`, `allKeyed`, `allSettledKeyed`). This plan does not modify
  either function — only how `try_fn` sequences calls to them — but because
  they're shared, the full test262 run (slice 3) is the real safety net, not
  the targeted `Promise/try/` directory alone.
- **Completion-ordering change.** Moving `NewPromiseCapability(C)` from
  "always, up front" to "lazily, once, per taken branch" changes *when* the
  receiver's constructor executor runs relative to the callback. This is
  deliberate (required by `ctx-ctor.js`'s call-count assertion) but is the
  highest-risk part of the change — any code path that relied on the
  capability (and thus the executor) running before the callback would
  break. Nothing in the current `try_fn` or its tests currently depends on
  that ordering the *old* way, but this is the detail most likely to bite a
  reviewer skimming the diff.
- **Silent `Completion::Exit` handling.** As noted in slice 2, the existing
  swallow-and-return-pending-promise behavior for non-Normal/non-Throw
  completions is preserved verbatim, not fixed. Flagging this explicitly so
  it isn't "fixed" as a drive-by and isn't mistaken for new behavior this
  plan introduces.
- **Not a tree-walker/GC/bytecode-path change.** No `eval_expr` /
  `exec_statement` hot-path, `property.rs` MOP, `gc_safepoint()`, or
  `ObjectKind` match is touched, so the usual GC-rooting and bytecode
  fast-path regression classes don't apply here.

## 7. Out of scope

- Fixing `Completion::Exit` propagation through `Promise.try` (see §4/§6) —
  pre-existing, unrelated to this issue, not filed.
- Updating the `spec/` submodule text to reflect the "avoids wrap" editorial
  change — `spec/` is read-only and not ours to change; the skew is simply
  noted in §2.
- Touching any other `Promise` static or instance method, even though they
  share helpers with `Promise.try`.
- Rolling `test262-pass.txt` forward via `--update-baseline` — that's a
  `main`-branch operation, not part of this branch's PR.
- Any refactor of `new_promise_capability` / `promise_resolve_with_constructor`
  themselves — both are already spec-correct and untouched by this fix.
