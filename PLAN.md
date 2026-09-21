# Plan: issue #684 — lowered async loops/try flatten lexical scope

## 0. Verification method

Before planning a fix, every symptom in the issue body was re-run against a
release build of `main` at `6b519ee3` (this branch's parent), via `jsse -e`
plus a Node reference where useful. Three of the issue's four bulleted
repros reproduce exactly as described; the fourth does not. Catch-param
shadowing, named only in the issue's prose (not one of its four bullets),
was checked separately and also reproduces. The plan below is scoped to what
actually reproduces.

## 1. Problem restated

`generator_transform.rs` lowers generator/async-generator/async-function
bodies into a flat `GeneratorStateMachine` (a linear list of `GeneratorState`s
executed by jumping between them), and the three drivers that execute it
(`async_function_resume` in `src/interpreter/eval.rs`,
`generator_next_state_machine_impl` and
`async_generator_next_state_machine_impl` in
`src/interpreter/eval/generator_runtime.rs`) run every state's statement list
against a single persistent `func_env`, with the sole exception of `for-of`
/`for-in` loop-head bindings, which already get a dedicated per-iteration
environment via `ForOfLoopState`. Because nested `Statement::Block`s get
spliced directly into the flat state list (losing their own
`NewDeclarativeEnvironment`), and because `while`/`do-while`/C-style `for`
never materialize a fresh environment per body entry or per iteration, any
`let`/`const` declared inside a loop body or a nested block inside a
generator/async function collapses onto one shared binding: closures created
in different iterations end up aliasing the same slot, and a shadowing inner
block's declaration overwrites the outer one instead of being discarded when
the block exits.

Confirmed reproducing on `6b519ee3`, identically across sync generators,
async functions, and async generators (verified by collecting closures across
all iterations and calling them only after the loop/generator finishes, to
rule out same-iteration read artifacts):

- `while(i<3){ let j=i; fs.push(()=>j); await 0; i++ }` → closures read
  `2,2,2` instead of `0,1,2`.
- `for(let i=0;i<3;i++){ fs.push(()=>i); await 0 }` → closures read `3,3,3`
  instead of `0,1,2`.
- `let x=1; { let x=2; await 0; log(x) } log(x)` → prints `2,2` instead of
  `2,1`.
- catch-param shadowing (named in the issue body's prose, not one of its four
  bulleted repros, and checked separately during planning):
  `let e="outer"; try{throw 1}catch(e){ await 0; } log(e)` → leaves the outer
  `e` as `1` instead of restoring `"outer"`. Same root cause: a `catch`
  clause introduces a block-like scope for its parameter, and the lowering
  has no mechanism for that scope either.

Checked and **not** reproducing on `6b519ee3` (see §7 — out of scope, and the
`gh issue comment` posted alongside this plan):

- `transform_for_in_statement` does not emit `Statement::Empty`. It delegates
  to the shared `transform_for_in_of_loop` (`generator_transform.rs:2192`),
  which builds `StateTerminator::ForOfInit`/`ForOfHead` the same way `for-of`
  does. A `for-in` loop with `await` in the body runs its body every
  iteration; verified directly with `jsse -e`.
- `var i; for (let i=0;i<2;i++) { await 0 }` does not throw, in any of the
  `for`/`for-in`/`for-of` head forms tested, with or without a trailing
  `.catch()` to surface an async rejection. This also matches the spec: the
  `sec-for-statement` early error that forbids a lexical `for`-head name from
  colliding with a `var` fires when the *loop body*'s `VarDeclaredNames`
  contains the head's bound name, not when an unrelated `var` exists outside
  the loop — `var i; for (let i ...) {}` is legal in real JS, confirmed
  against `node`.

`for-of`/`for-in` themselves are already correct, including for a `let`
declared *inside* their bodies (`for (const x of arr) { let y = x*2; ...;
await 0 }` correctly yields fresh `y` per iteration on `6b519ee3`) — because
their loop-head binding already gets a real per-iteration environment via
`ForOfLoopState`, and every statement in the body_state (including a
flattened block's declarations) happens to execute inside that same
per-iteration environment as a side effect. `while`/`do-while`/`for` have no
equivalent mechanism, and plain nested blocks have none either. The fix
generalizes what `ForOfLoopState` already does.

## 2. Spec basis

- **`sec-block-runtime-semantics-evaluation`** (`Block : { StatementList }`
  Evaluation, `spec/spec.html`): "Let _blockEnv_ be
  NewDeclarativeEnvironment(_oldEnv_)... Set the running execution context's
  LexicalEnvironment to _blockEnv_... Set the running execution context's
  LexicalEnvironment to _oldEnv_ [after evaluating StatementList]," with the
  explicit note "No matter how control leaves the Block the LexicalEnvironment
  is always restored to its former state." This is the clause the lowering
  violates for plain nested blocks (symptom: block shadowing) and, since a
  loop body is a `Block`, for `while`/`do-while` bodies re-entered each
  iteration (symptom: `while` per-iteration `let`).
- **`sec-for-statement`** (For Statement Evaluation): computes
  `perIterationLets` as the `ForDeclaration`'s `BoundNames` when the head uses
  `let` (empty for `const`, empty for `var`/expression heads), then calls
  `ForBodyEvaluation` with them.
- **`sec-forbodyevaluation`** (`ForBodyEvaluation`): calls
  `CreatePerIterationEnvironment(perIterationBindings)` once before the first
  test, and again after every iteration's body completes (before the
  increment runs). This is the clause the lowering skips entirely for `for
  (let ...)` (symptom: `for`-head per-iteration `let`).
- **`sec-createperiterationenvironment`** (abstract operation
  `CreatePerIterationEnvironment`): "Let _thisIterationEnv_ be
  NewDeclarativeEnvironment(_outer_)... For each element _bn_ of
  _perIterationBindings_ ... Let _lastValue_ be ...
  _lastIterationEnv_.GetBindingValue(_bn_) ... _thisIterationEnv_
  .InitializeBinding(_bn_, _lastValue_)." — copy-forward-into-a-fresh-env,
  the exact algorithm `exec_for` (`src/interpreter/exec.rs:1835-1937`)
  already implements correctly for the non-suspending tree-walker path.
- **`sec-while-statement`** (While Statement Evaluation): re-evaluates the
  `Statement` (the body) every pass with no per-iteration environment
  handling of its own — the loop-body freshness comes entirely from `Block`
  evaluation being invoked afresh each pass, per
  `sec-block-runtime-semantics-evaluation` above.
- **`sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`**
  (`ForIn/OfBodyEvaluation`): creates `iterationEnv` fresh each iteration for
  a lexical `LeftHandSideExpression`. Cited for contrast only — this clause
  is already correctly modeled by `ForOfLoopState`/`effective_env`
  (`src/interpreter/types.rs:396-424`) and is the mechanism this plan
  generalizes; it is not itself being changed.

The async/generator state-machine lowering is an internal implementation
strategy, not something the spec describes — but it is required to preserve
the observable semantics of the clauses above. The fix restores that.

## 3. Files to touch

Engine:

- `src/interpreter/generator_transform.rs` — `transform_yielding_statement`'s
  `Statement::Block` arm (~line 846: currently
  `transform_statements(stmts, ctx, after_state)` with no scope tracking),
  and `transform_while_statement` / `transform_do_while_statement` /
  `transform_for_statement` (~lines 1938–2190), to emit whatever
  scope-enter/scope-exit markup the runtime needs (see §4 for the shape).
  `transform_for_in_of_loop` and `transform_for_in_statement` /
  `transform_for_of_statement` are read-only reference material, not touched.
- `src/interpreter/generator_analysis.rs` — needed if the chosen marker
  representation requires knowing a block's own lexically-declared names
  (`let`/`const`/`class`/block-scoped function declarations) at transform
  time; `generator_analysis.rs` already computes comparable per-body name
  sets for hoisting.
- `src/interpreter/types.rs` — `ForOfLoopState` (~line 396) is the model to
  generalize. Two persisted-state locations need the analogous stack:
  `AsyncFunctionState.for_of_stack` (~line 389, for plain `async function`)
  and `Interpreter.generator_for_of_stacks: FxHashMap<u64, Vec<ForOfLoopState>>`
  (`src/interpreter/mod.rs:260`, shared by sync generators and async
  generators, keyed by generator object id).
- `src/interpreter/eval.rs` — `async_function_resume` (~line 8178), the
  `term_env` computation at ~line 8684-8687.
- `src/interpreter/eval/generator_runtime.rs` —
  `generator_next_state_machine_impl` (~line 482, `term_env` at ~771-774) and
  `async_generator_next_state_machine_impl` (~line 3267, `term_env` at
  ~4165-4168). Also audit `generator_return_state_machine` (~line 1838),
  `generator_throw_state_machine` (~line 2164),
  `async_generator_return_state_machine_with_promise` (~line 6303), and
  `async_generator_throw_state_machine_with_promise` (~line 6418): confirm
  during implementation whether these route through the two `_impl`
  functions above (no `term_env` duplication was found by grep, but an
  abrupt `return()`/`throw()` mid-scope is exactly the unwind case this fix
  must get right, so each needs an explicit check, not an assumption).
- `src/interpreter/gc.rs` — root the new stack(s) alongside the existing
  `for_of_stack` rooting (`collect_for_of_stack_roots`, ~lines 1135-1148, and
  its call sites at ~452, ~462, ~476).
- `src/interpreter/exec.rs` — `exec_state_machine_body` (~line 26) only if
  the env-selection responsibility moves into it; otherwise unchanged.

Do not touch: `exec.rs`'s `exec_for`/`exec_while`/block handling — those are
already spec-correct (confirmed by reading them; they are the reference
implementation for this fix) — or the `for-of`/`for-in` transform/runtime
paths, which are already correct and are only being generalized *from*, not
changed.

Docs:

- `docs/adr/` — add a short ADR once the design lands, in the style of
  `docs/adr/0001-inline-cache-ast-seam.md`: what `ForOfLoopState` covered,
  why it needed generalizing to cover plain blocks and `while`/`for`, and the
  chosen shape of the new stack(s). This documents a real internal
  architecture decision (a second production consumer of the
  per-iteration-environment pattern), matching the bar the existing ADRs
  set.
- `CONTEXT.md` — no new vocabulary needed; "lexical environment",
  "per-iteration environment" etc. are already spec terms, not
  project-specific jargon.

## 4. TDD slices

The mechanism is one fix (a lexical-scope stack, generalizing
`ForOfLoopState`) that must reach all three drivers, because all three share
the same flattening root cause and the issue reproduces identically on all
three. Slice by *capability*, not by driver, so each slice is independently
red/green and the mechanism accretes correctness incrementally; wire all
three drivers together in each slice rather than leaving two of three
drivers silently still-broken after a slice lands.

**Pop mechanism (decided up front, not left to implementation):** control
does not leave a lowered block by syntactic Rust-level unwinding — it jumps
to a target state id (`LoopControlTarget`, `inline_jump_terminator`,
`TryEnter`/`TryExit`), so "pop when a completion propagates past a scope
boundary" is not itself an implementable rule; the driver needs to know *how
many* scopes to pop for a given jump. Follow the idiom `ForOfLoopState`
already established for the analogous `try_stack` problem: its `try_depth`
field (`types.rs:409`, doc comment: "Depth of the driver's try stack when
the loop began, so an abrupt completion can tell a `finally` lexically
inside the loop ... from one outside it") records a stack depth at loop
entry so an abrupt completion can truncate back to it. Do the same for the
new scope stack: every jump target computed by the transform (loop
`break`/`continue` targets, `try`/`catch`/`finally` entry/exit states, the
function-exit state) records the scope-stack depth that should be active
once execution reaches it; the driver truncates the runtime scope stack to
that recorded depth whenever it jumps, and pushes a fresh environment for
whatever scope the target state itself opens. This is truncate, not pop-one,
so it's correct regardless of how many scopes a jump crosses in one step,
and it composes with `for_of_stack`/`try_stack`, which keep their own
independent depth bookkeeping.

1. **Block-scope push/pop, straight-line case.**
   Red: `test262-extra/async-function-block-scope-shadowing-across-await.js`
   — `let x=1; { let x=2; await 0; assert(x===2) } assert(x===1)`, run via
   the existing `test262-extra` harness pattern. The inner assertion already
   passes today (jsse's flattening bug means the inner block *reads* the
   value it just wrote just fine); make sure the fixture is discriminating
   by also capturing a closure over the outer `x` *before* the block runs
   and asserting it still reads `1` afterward — that's the assertion that
   actually fails on `6b519ee3` and is the one this slice is red on.
   Green: give `Statement::Block` (in `transform_yielding_statement`) a way
   to mark "this block's flattened statements need their own environment",
   and have `async_function_resume`'s `term_env` computation create a fresh
   child environment on entry and restore the outer one (via the
   depth-truncation rule above) on the block's normal-completion exit. Also
   add the equivalent `#[cfg(test)]` unit test in `generator_transform.rs`
   asserting the emitted state carries the new scope marker and recorded
   depth (following the existing `test_simple_transform` style at the bottom
   of that file).

2. **Abrupt completion unwinds the block stack.**
   Red: three more cases in the same `test262-extra` file (or siblings) —
   `break`/`continue` out of a scoped block inside a loop, `return` from
   inside a scoped block, and a `throw` from inside a scoped block caught
   outside it — each asserting the outer scope's bindings (via a
   pre-captured closure, same discriminating shape as slice 1) are restored
   after the block is left abruptly. Also a catch-param case:
   `let e="outer"; try{throw 1}catch(e){ await 0 } assert(e==="outer")` (the
   symptom confirmed in §1 and reported to the issue).
   Green: implement the depth-truncation rule above at every jump in the
   three drivers — loop `break`/`continue` targets, `TryEnter`/`TryExit`,
   `EnterCatch`, and function return/throw — truncating the runtime scope
   stack to each jump target's recorded depth before executing the target
   state.

3. **Loop-body re-entry (while/do-while).**
   Red: `test262-extra/async-function-while-loop-per-entry-let-binding.js`
   — the issue's exact `while` repro, plus a `do-while` sibling, both
   asserting distinct closures observe distinct values.
   Green: since a loop body is a `Block`, this should require no new
   mechanism beyond slice 1 firing on every entry into the body state rather
   than only once — the slice exists specifically to catch (and regression
   test) the case where an implementation mistakenly keys the fresh-env
   creation off state *id* instead of state *traversal* (a stale env would
   be reused on the second iteration). Extend `transform_while_statement`
   and `transform_do_while_statement` only if the marker from slice 1 isn't
   automatically picked up by the loop's existing `body_state`.

4. **`for`-head `CreatePerIterationEnvironment`.**
   Red: `test262-extra/async-function-for-loop-per-iteration-let-binding.js`
   — the issue's exact `for (let i ...)` repro, asserting `0,1,2` not
   `3,3,3`; include a `const`-in-head negative-ish case (no per-iteration
   copy needed, single binding, still correct) and a case exercising the
   "copy forward before the increment runs" ordering from
   `sec-forbodyevaluation`.
   Green: `transform_for_statement` gains the `perIterationBindings`
   computation (`ForDeclaration`'s bound names when `VarKind::Let`, mirroring
   `exec_for`'s `per_iteration_bindings` at `exec.rs:1838-1854`) and emits a
   copy-forward marker at the loop's `update_state` transition, consumed by
   the same runtime stack from slice 1.

5. **Sync-generator and async-generator parity.**
   Red: mirror slices 1–4's fixtures as `function*`/`async function*` custom
   tests under `test262-extra/` (or extend the existing files with
   generator variants) plus the `spec/spec.html`-cited targeted `test262`
   directories in §5.
   Green: wire `Interpreter.generator_for_of_stacks`
   (`src/interpreter/mod.rs:260`) with the same new stack type used by
   `AsyncFunctionState`, and apply it in
   `generator_next_state_machine_impl` / `async_generator_next_state_machine_impl`.
   This slice is expected to be small if slices 1–4 built the mechanism
   driver-agnostically; if it isn't small, that is a sign slice 1's design
   was accidentally async-function-specific and needs revisiting before
   continuing.

6. **GC rooting.**
   Red: a `cargo test --release` stress test (in
   `src/interpreter/tests.rs`, alongside the existing
   `generator_for_of_stacks` assertions at ~lines 4208/4264) that suspends an
   async function mid-block with a live closure over a block-scoped `let`,
   forces a GC pass (`gc_safepoint`/an explicit collect entry point already
   used elsewhere in that file), resumes, and asserts the closure still
   reads the correct value (i.e., its binding wasn't collected).
   Green: extend `collect_for_of_stack_roots`-equivalent tracing in `gc.rs`
   to also walk the new stack(s), at the same call sites the existing
   `for_of_stack` rooting uses (~lines 452, 462, 476, 1135-1148).

Follow-ups explicitly not in this PR (see §7).

## 5. Test surface

Cadence: run the relevant targeted directory (or two) after the slice that
touches it, not the full list after every slice — e.g. slice 1 only needs
`block/` and `async-function/`; slice 4 needs `for/`. Run the *full* list
once before opening the PR, and the full `uv run python
scripts/run-test262.py` (see below) exactly once, right before the PR, not
per slice.

Targeted `test262` directories, by relevance:

- `test262/test/language/statements/for/` (per-iteration `let` head
  bindings — `head-let-fresh-binding-per-iteration.js` and siblings, to
  confirm no regression on the synchronous path)
- `test262/test/language/statements/while/`, `.../do-while/`
- `test262/test/language/statements/block/`
- `test262/test/language/statements/for-of/`, `.../for-in/`,
  `.../for-await-of/` (regression coverage only — these already pass and
  must keep passing, since `ForOfLoopState` is the code being generalized)
- `test262/test/language/statements/async-function/`
- `test262/test/language/statements/generators/`
- `test262/test/language/statements/async-generator/`
- `test262/test/language/statements/try/` (the new abrupt-completion unwind
  logic shares machinery with `try_stack` unwinding; rerun to catch any
  interaction)

None of the existing `test262` tests above combine a per-iteration/block
binding with a closure *and* a suspension point in the same test — the
existing `for`-head per-iteration tests use value accumulation
(`s += x`), not closures, and don't involve `await`/`yield` at all (verified
by grepping `head-let-fresh-binding-per-iteration.js` for `await`/`function*`
— absent). That combination is exactly what test262-extra needs to cover,
one file per confirmed symptom (§4 slices 1–5), each with `esid`/spec-clause
frontmatter citing the clauses in §2, following the existing
`test262-extra/*.js` frontmatter pattern (see
`test262-extra/tail-call-in-try-block.js` for the format:
`description`/`esid`/`info` block comment, then plain `assert`-based body,
no test262 harness `includes` beyond what's already available).

Also run `cargo test --release` (the new GC-rooting regression lives there)
and the full `uv run python scripts/run-test262.py` before opening the PR,
per the project's standard gate.

## 6. Regression risk

- **Every closure/`await`/`yield` combination in the corpus.** This change
  touches the environment every statement in a generator/async-function
  state executes against, across all three drivers. The most likely
  regression shape is a *correctly-working* case that starts creating scopes
  it didn't before (extra allocation, fine) versus one that stops sharing an
  environment it needed to share (e.g. two sibling statements in the same
  original block that must still see each other's declarations — a wrong
  scope-boundary placement in the transform would silently break that
  without an obvious crash). The full `test262` run is the primary defense
  here; a partial/targeted run is not sufficient given the blast radius.
- **`for_of_stack`/`ForOfLoopState` interaction.** Loops can nest arbitrarily
  (a `for-of` containing a `while` containing a scoped block); the new stack
  and the existing `for_of_stack` must compose (both contribute to
  `term_env` selection), not replace each other. Get the "which stack wins /
  how they nest" rule right in slice 1, since every later slice builds on it.
- **`try_stack` unwind interaction.** A scoped block can be inside a `try`,
  and a `try`/`finally` can be inside a scoped block; abrupt completions
  (slice 2) must unwind both stacks in the correct relative order. Existing
  `try`/`finally`-inside-loop test262 coverage
  (`test262/test/language/statements/try/`) is the regression net.
  `catch`-parameter shadowing (mentioned in the issue body's prose) was
  confirmed broken during planning (§1) with the same root cause — a
  `catch` clause's own binding is exactly this kind of block-like scope in
  the lowering — and is covered by slice 2's `EnterCatch` handling.
- **The `await using` branch in `transform_yielding_statement`
  (`block_has_await_using`).** That arm deliberately keeps its block intact
  (not flattened) specifically to get a real `block_env` via the ordinary
  tree-walker `Statement::Block` path, as a workaround for a different bug
  (#665, blocking-drain disposal). Do not fold it into the new mechanism in
  this PR — verify by test that it still behaves as before, since both
  mechanisms now produce "a real per-block environment" by different means
  and could plausibly be merged later, but doing so now conflates two
  unrelated bugs.
- **The `InlineYield`/`self.generator_context` replay backstop**
  (`generator_runtime.rs`, documented in this repo's `CLAUDE.md` as "a
  degraded-behavior backstop for undiscovered gaps," issue #625). If any
  construct in the loops/blocks touched here currently falls back to replay,
  replay re-executes a state's statement list from scratch on each resume,
  which happens to look like "fresh environment each time" by accident for
  some shapes (this is why an early exploratory sync-generator repro looked
  correct until it was retested with a side-effect counter — see the
  verification log this plan is based on). Confirm which construct, if any,
  in the fixtures added here goes through replay rather than the compiled
  state machine, and don't let replay mask a bug the compiled path still
  has; prefer fixtures that exercise the compiled path directly.
- **GC rooting is silent-failure-shaped.** A missed root doesn't fail fast;
  it shows up as sporadic wrong values or crashes under memory pressure,
  invisible in small `-e` repros. Slice 6 exists specifically because this
  class of bug is easy to introduce and easy to miss without a targeted
  test.
- **Bytecode fast path.** `src/interpreter/bytecode/` is feature-flagged off
  by default (`bytecode_enabled`) and, per `CLAUDE.md`, is a separate
  experimental path; confirm it does not independently implement
  generator/async lowering that would need the same fix (a quick grep for
  `GeneratorStateMachine`/`ForOfLoopState` under `bytecode/` during
  implementation; not expected to be in scope, called out so it isn't
  silently missed).

## 7. Out of scope

- **The two issue-body symptoms that don't reproduce** (`for-in` body
  skipped via `Statement::Empty`; spurious `var`/`for-let` redeclaration
  error) — not fixed, because there is nothing to fix on `6b519ee3`. Posted
  as a `gh issue comment 684` alongside this plan so the discrepancy is a
  recorded, overridable decision rather than a silent scope cut. Regression
  tests locking in the current correct behavior for these two cases can ride
  along in slice 1's or slice 4's test file (cheap, and the for-loop head
  changes in slice 4 are exactly the code most likely to accidentally
  reintroduce the redeclaration false-positive), but no production code
  changes are planned for either.
- **Folding `for_of_stack`/`ForOfLoopState` into the new generalized stack.**
  The new mechanism generalizes the *pattern* `ForOfLoopState` established;
  it does not need to literally merge the two stacks into one type in this
  PR. That unification is a reasonable follow-up refactor once both have
  shipped and stabilized independently, not a prerequisite for fixing the
  bug.
- **`switch` statement scoping inside generators/async functions.** Not
  mentioned in the issue and not verified broken; if the same root cause
  turns out to affect `switch` block scoping (a `switch` case list is also a
  kind of block scope per spec), file a follow-up issue rather than
  expanding this PR's surface — flag it for a quick manual check during
  implementation but don't plan production changes around it speculatively.
- **`await using` disposal timing (#665) and its blocking-drain bug.**
  Explicitly a different, already-filed issue; not touched here beyond the
  "don't break it" verification in §6.
- **The `InlineYield` replay backstop's eventual removal (#625).** Out of
  scope; this plan's fix must be correct independent of whether replay is
  later removed, but removing replay itself is separate work.
- **Bytecode fast path changes**, unless the audit in §6 finds it actually
  implements this lowering independently (not expected).
- **Refactors, formatting, or unrelated cleanup** in any file this PR
  touches, beyond what's needed for the stack generalization itself.
