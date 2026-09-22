# Async generators: inline `yield` and `yield*` suspend like lowered ones

Issue #710 (follow-up of #687, audit). Two spots on the degraded
`SentValueBindingKind::InlineYield` fallback called the blocking
`Interpreter::await_value`, which runs a nested microtask loop inside the
running job: (a) the async driver's handling of an inline `Completion::Yield`
and (b) the `yield*` loop in `eval.rs`.

## Audit result: both were reachable

The fallback is entered when a state body returns `Completion::Yield`, i.e.
the transform left a `yield` inside an expression it does not decompose. Known
reachers: destructuring-assignment patterns with a suspension in a default or
target (`({a = yield 1} = {})`, `[a = yield 1] = []`, `for ({a = yield} of …)`),
`x[yield 1] = yield 2`, `for (a[yield 1] of …)`, `super[yield 1]`. test262
drives the inline `yield` path from ~26 async-generator scenarios
(`for-await-of/async-gen-*-dstr-*-yield-expr.js`) and ~96 sync ones.
Neither site could be deleted or made `unreachable!`, so both became
suspension points.

- Inline `yield v` awaited `v` blocking and resolved the request under a
  nested `drain_microtasks()`: jobs queued before the request ran inside it,
  and a still-pending operand promise was read as `undefined`.
- Inline `yield*` was worse. On its *first* execution `generator_context` is
  `None`, so the evaluator took the synchronous `get_iterator` path and threw
  `TypeError: … is not iterable` for an async iterable; only a *replay* reached
  the blocking `await_value`.

## Decisions

- **The inline yield is a `StateTerminator::Yield` in disguise.** The driver
  builds a synthetic `Yield { resume_state: current_state, sent_value_binding:
  Some(InlineYield { .. }) }` from the `Completion::Yield` and lets it fall into
  the existing terminator tail, with the operand supplied by the completion
  (`inline_yield_operand`) instead of `eval_operand`. That reuses the tail's
  pending / rejected / fulfilled paths, the `async_gen_yield_pending` flag and
  the once-per-request pop in the continuation, so no new suspension code
  exists and the nested drain is gone. Two behaviours are inherited from the
  tail on purpose: `pending_exception`/`pending_return` are preserved across the
  suspension (the old inline site dropped them), and a promise operand that
  rejects *while pending* rejects the request without completing the generator,
  exactly like a lowered `yield`.
- **`yield*` in the fallback is handed to the delegate machinery.** Under an
  async generator body (`in_async_generator_body`, set by
  `exec_state_machine_body` and restored on the way out, so a nested sync
  activation resets it) the evaluator does not iterate. It returns the iterable
  as the `Completion::Yield` value, flagged by `inline_yield_delegates`, and the
  driver builds the same synthetic terminator with `is_delegate: true`. The
  existing delegate arm does `get_async_iterator`, the first `next`, and parks on
  `await_then`; `yield_star_await_inner_result_resume` does the rest of the
  steps.
- **Replay carries the delegation's result.** The `InlineYield` binding rides in
  `DelegatedIteratorInfo::sent_value_binding`. When the delegation reports
  `done`, `yield_star_await_inner_result_resume` stores it as `pending_binding`
  (instead of `None`) and passes the delegate's return value as the sent value,
  so the driver re-enters the state with the `yield*` slot fast-forwarded to that
  value. The non-done steps keep `delegated_iterator: Some(..)`, so the next
  request re-enters the delegate path and never replays the state body.
  `bind_yield_star_result` stays a no-op for `InlineYield`.
- **A replay does not re-evaluate the `yield*` operand** — unless the operand
  contains a yield of its own, whose slot must be numbered first — so the
  iterable is evaluated, and the delegate started, once.
- The `yield*` loop in `eval.rs` keeps its `is_async_gen` branch only for the
  legacy `IteratorState::Generator`/`AsyncGenerator` paths. Those variants are
  never constructed any more (they are only matched and re-stored), so the branch
  is dead code left for #711 rather than a live blocking await.

## Known boundaries (unchanged, not fixed here)

- The fallback still replays: it re-executes the state's *preceding statements*
  and the yield operand on every resume, so `function* g(){ n++; var a;
  [a = yield f()] = []; }` runs `n++` and `f()` again per resume. Only the
  `yield*` operand is exempt. Retiring the fallback means lowering
  destructuring patterns with suspensions into state-machine steps (#625).
- `generator_context` is a single interpreter-wide slot: an inline yield inside
  a generator that is itself advanced from another generator's replay clobbers
  the outer context.
- `var/let/const {a = yield 1} = {}` now reaches the fallback correctly
  (#727: `bind_pattern` propagates `Completion::Yield` instead of swallowing
  it — see ADR-2026-09-22-1752). `catch ({a = yield 1})` still swallows the
  yield — that call site keeps discarding non-`Throw` completions
  deliberately, since its own suspension detection is issue #726's
  territory, not touched by #727.
- `x[yield 1] = yield 2` evaluates the right-hand suspension first.
- The boundaries listed in ADR-2026-09-21-2300 (a rejected inner result completes
  the generator instead of throwing into its own `try`/`catch`; `finally` blocks
  on a delegated `.return()`; `async_generator_await_return` drains) apply to the
  inline forms too.
