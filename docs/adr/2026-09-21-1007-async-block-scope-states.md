# Async-function state machine: block scopes as `EnterScope`/`ExitScope` states

Issue #683: the async-function state machine flattened block scope, so a
block that directly declares `await using` (`block_has_await_using`) was
special-cased by keeping it intact and tree-walked, with its `DisposeResources`
parked on a single `suspendable_dispose_block: Option<usize>` slot for the
executor to finish suspendably. That one mechanism had to serve cases it
wasn't designed for: an unrelated `await` inside the intact block resumed by
re-entering the *same* state from the top, replaying (and re-declaring) the
block forever (issue's bug 1); `await using` declared directly in a
try/catch/finally clause's own statement list never reached the intact-block
path at all, so it landed on the *function-level* `dispose_stack` and
disposed only at function exit, after `finally` ran (bug 2); and the
single-slot mechanism couldn't track more than one open scope's disposal at
once, so a nested isolated block disposed via the blocking driver instead of
suspending (bug 3).

## Decision

Give the async-function driver (`eval.rs`'s `async_function_resume`) a real
runtime concept of "a block scope is currently open": a `scope_stack:
Vec<ScopeFrame>` on `AsyncFunctionState`, pushed/popped by two new
`StateTerminator` variants, `EnterScope { body_state }` and `ExitScope {
after_state }`, mirroring how `for_of_stack` already tracks active for-of
iteration environments.

- **Transform side** (`generator_transform.rs`): `transform_scope_block`
  lowers a block's interior through `EnterScope` + the *ordinary*
  per-statement pipeline (`transform_statements`) + `ExitScope`, instead of
  emitting the block as one intact, tree-walked statement. Because the
  interior now decomposes into as many states as it needs, an `await` inside
  it — whether or not it's part of the block's own disposal — gets its own
  state and its own resume point, fixing bug 1 by construction: there is no
  single state left to replay from the top.
- The same helper (`transform_clause_body`) applies to a `try`/`catch`/
  `finally` clause's own statement list when it directly declares `await
  using` (no extra `{ }`), giving it the same real scope. Its resource then
  disposes at the clause's own exit — before `Catch`/`Finally` ever runs —
  fixing bug 2. `generator_analysis.rs`'s `scan_await_using` gained a
  `scan_clause_body` helper recognizing this shape as isolatable, alongside
  the existing "a further nested block" case.
- **Runtime side** (`eval.rs`): `EnterScope` creates the block's own
  `Environment` (a child of whatever env is active) and pushes a
  `ScopeFrame { env, try_depth, for_of_depth }`; every subsequent state body
  in that scope executes against it via `term_env`, which now picks between
  the innermost open scope frame and the innermost open for-of loop by
  comparing `for_of_depth` against the live `for_of_stack` length — whichever
  was opened more recently wins. `ExitScope` pops the frame and disposes its
  `dispose_stack` suspendably (`DisposeCursor`/`PendingDispose`, the same
  primitive `for_of_stack` and the function-level disposal already use), not
  via the blocking `run_dispose_cursor_blocking` driver. Because `scope_stack`
  is a `Vec`, nested scopes need no new mechanism — fixing bug 3 as a
  consequence of the same design, not a separate patch.
- `route_return!`, `route_loop_control!`, and the throw-routing block now
  dispose any scope frames a `return`/`break`/`continue`/throw *crosses*
  before continuing to route it (`unwind_scopes_to!`), computing the
  crossing boundary the same way the existing `for_of_stack` unwind boundary
  is computed (from the intercepting `finally`'s try-stack depth, or the
  loop-control target's own recorded `scope_depth` when there is none).
  Because a scope frame's own disposal can suspend, a crossing that needs an
  `Await` bails out via `return` with a new `DisposeThen` variant
  (`ScopeExit`/`ScopeCrossReturn`/`ScopeCrossLoopControl`/`ScopeCrossThrow`)
  parked; on resume, `route_return!`/`route_loop_control!` are simply
  re-invoked with the value/target the disposal was seeded with (extracted
  back out of the completion for `Return`, carried as a `Copy` payload for
  loop-control), which safely redoes the now-cheaper crossing check. Throw
  routing needs no such payload: `pending_exception.is_some()` is already
  rechecked at the top of the driver loop every iteration, so a scope
  disposal that needs to suspend just leaves it set and lets the next
  iteration re-enter.

## Scoped to plain async functions

`StateTerminator` is shared by the one lowering pass across sync generators,
async generators, and async functions (`transform_async_function` rewrites
`await`→`yield` before the pass and `Yield`→`Await` after). `EnterScope`/
`ExitScope` are emitted only when lowering a plain async function body —
gated on `ctx.detect_for_await`, which is `true` only for
`transform_async_function`'s own call into the shared transform (`is_async:
true, detect_for_await: true`), never for `transform_generator`
(`is_async: false`) or `transform_async_generator` (`is_async: true,
detect_for_await: false`). The two generator drivers
(`generator_runtime.rs`'s `generator_next_state_machine_impl` and
`async_generator_next_state_machine_impl`) get `unreachable!()` stub arms for
the two new variants, exactly the shape already used for `StateTerminator::
Yield` in the async-function driver (a variant only the other side emits).

An async generator containing the same block shape (`{ await using a; ... }`)
keeps taking the pre-existing intact-block path unchanged: the
`suspendable_dispose_block`/`parked_block_dispose` single-slot mechanism and
the `block_exits`/`BlockExits` table this ADR's design replaces *for plain
async functions* remain live code, not dead code to delete, because async
generators still depend on them. Porting `scope_stack` to the two generator
drivers (or designing something native to their different per-terminator
idiom — they route abrupt completions via `route_generator_exception` and
friends, not `route_return!`/`route_loop_control!`/`unwind_for_of!`) is
follow-up work, tracked generally by #665's "blocking driver instead of
suspend" class of bugs for loop/switch heads in those drivers.

## Known boundary: deep scope/for-of alternation

`term_env` and the crossing unwind in `route_return!`/`route_loop_control!`/
throw-routing both order a scope frame against for-of frames by comparing the
frame's `for_of_depth` against the *current* `for_of_stack` length — correct
for one level of nesting either direction (a scope inside a loop, or a loop
inside a scope), which covers every shape in this issue and in the existing
`await-using-block-abrupt-exit-routing.js` regression suite. It is not a
fully general interleaved unwind: a completion crossing `scope → for-of →
scope` (three or more alternating levels) can close the outer for-of loop
before the inner scope's own disposal has had a chance to run, whereas spec
order is strictly innermost-first regardless of construct kind. No test in
this repo or test262 exercises that shape (for-of loops always appear either
entirely inside or entirely outside an isolated block in every case found).
Building the fully general step-by-step interleaved unwind was scoped out
rather than attempted blind — see the PR discussion for #683.

## Consequences

- `test262-extra/await-using-block-inner-await-suspends.js`,
  `await-using-block-inner-await-abrupt-variants.js`, and
  `await-using-try-list-dispose-before-finally.js` are new regression
  coverage for bugs 1 and 2. Bug 3 is covered by a new case in the existing
  `await-using-block-dispose-tick-alignment.js` (tick-alignment proves
  suspension, not just correct ordering) and by
  `await-using-dispose-suspended-gc-rooting.js` gaining a nested-scope GC
  case, since a `ScopeFrame`'s `Environment` must stay rooted across its own
  suspended disposal exactly like `for_of_stack`'s does.
- `has_suspendable_await_using_block`'s gate is *not* broadened by this
  change beyond what the three named bugs need — the `AwaitUsingScan::
  Blocked` cases (`for (let ..)`, `for-in`, `with`, a lexical declaration
  beside an isolatable block in the same list) still fall back to the
  tree-walker, unchanged. Widening that gate now that `scope_stack` exists is
  a real follow-on but its own PR, per the issue's own scoping note.
