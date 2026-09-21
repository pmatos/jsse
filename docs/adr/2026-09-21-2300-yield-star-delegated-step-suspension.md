# Async generators: every `yield*` step suspends at its `Await`

Issue #708 (follow-up of #687). Only the first delegated step of `yield*` in an
async generator parked on `Await(innerResult)`; the second and later `next`,
`throw` and `return` steps, and a `return` request queued behind a parked
`yield*`, called the blocking `await_value`. That ran a nested microtask loop
inside the running job (a reaction that issued `it.next()` ran already-queued
reactions before returning) and, once the queue was empty, read a still-pending
inner result as `undefined`.

## Decisions

- **One continuation for every step.** `DelegateStep { Next, Throw, Return }`
  and `yield_star_suspend_on_inner_result` park the generator on
  `Interpreter::await_then` and resume in
  `yield_star_await_inner_result_resume`, which takes the step. The four old
  inline tails are deleted; the driver only makes the inner call, checks its
  type, and suspends.
- **The generator is already parked while the driver runs a delegated step.**
  It stays `SuspendedAtState { resume_state }` with `delegated_iterator: Some`
  from the previous step, so the suspend helper only clears
  `pending_exception`/`pending_return` and awaits. A request that arrives during
  the await is queued (`queue_len > 1`), never started.
- **Two queue invariants carry the whole change.** (1) A driver-context caller
  sets `async_gen_yield_pending` and returns, so `async_gen_process_queue`
  neither pops nor recurses. (2) The continuation settles and pops the front
  request exactly once on every exit, then calls `async_gen_process_queue`.
  A continuation reached from a job (not the driver) does not set the flag:
  nobody consumes it there.
- **The result binding comes from `DelegatedIteratorInfo::sent_value_binding`**
  (not `pending_binding`) and is written by one `bind_yield_star_result`
  (initialise-if-uninitialised, else `env_set`), so every step binds alike.
- **A throwing `IteratorValue` at any step routes through the generator's
  try/catch**, as the first step already did; the old `next` tail truncated the
  try stack and jumped to the catch state instead, which left the request
  rejected.

## Known boundaries (unchanged, not fixed here)

- A rejected inner result still rejects the request and completes the
  generator instead of throwing into the generator's own `try`/`catch`.
- `finally` blocks are not run when `.return()` unwinds a delegated `yield*`.
- A direct delegated `.return(v)` does not first `Await(v)`
  (`AsyncGeneratorUnwrapYieldResumption`).
- `async_generator_await_return` still drains microtasks inline, as do the
  `yield` operand, `for await` step and legacy `IteratorState::AsyncGenerator`
  paths.
- `var`-less pattern targets: `const {x} = yield* it` (and `let`) fails with
  `Cannot access 'x' before initialization` at every step, because the pattern
  is bound with `BindingKind::Var`.
