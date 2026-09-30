# Plan: issue #687 — blocking `await_value` callers drain the microtask queue inline

## 0. Critical precondition: this branch is stale and must be synced first

This branch (`sym/jsse/687-...`) still carries 5 old commits (`d448dba6` plan,
`9caed474`/`d2b51722`/`61799f65` fixes, `f79b3879` plan-drop) from a prior
planning→implementation cycle. That cycle's PR **#714 is already merged to
`main`** (squash commit `f84dc6fc`) — verified by diffing: every line those 3
commits removed is already absent at the equivalent spot in `origin/main`
(commit `b887b68b`, fetched during this planning session). Since #714 merged,
`main` gained **~15 more merged PRs** directly in this same bug class (#707,
#708, #709 landing as #720/#723/#728/#729/#730/#731/#732/#734/#735, plus
#712/#713/#716/#733), all closed as "Follow-up of #687". None of that is on
this branch.

**Before any other work in the implementation stage:**

```
git fetch origin main
git reset --hard origin/main
```

A plain `git merge origin/main` was tried and tested (`git merge-tree
--write-tree HEAD FETCH_HEAD`, read-only) and **produces a real textual
conflict** in `generator_runtime.rs` even though the branch's actual changes
are a verified content-subset of `main` — the intervening ~4,000-line rewrite
of that file makes a 3-way merge apply the old small diff to the wrong
context. A hard reset is the correct sync, not a merge: nothing unique to
this branch would be lost (confirmed above), so this is a safe reset despite
being irreversible-looking. (This planning run's own sandbox permission
classifier blocked both `reset --hard` and `merge` as "Irreversible Local
Destruction" with no human available to approve in this headless run, so it
could not be performed here — it is recorded as the mandatory first step for
whoever/whatever runs the implementation stage instead.)

All line numbers, code quotes, and file contents in the rest of this plan are
taken from `origin/main` at `b887b68b` (via `git show`/a disposable
`git worktree add` probe build), **not** from this branch's stale working
tree — the working tree currently disagrees with both of them.

## 1. Problem restated

`Interpreter::await_value` (`src/interpreter/eval.rs:9878`) is jsse's blocking
fallback for `Await`: it registers fulfill/reject reactions on the awaited
promise and then loops, popping and running jobs from the interpreter's own
microtask queue synchronously, until its own reaction fires. This is correct
only when nothing else is left running on the Rust call stack above it. The
architecturally-correct mechanism — `StateTerminator::Await` in the
generator/async state machine — instead registers a reaction and returns
control immediately, letting the real job queue redrive the continuation
later; this is what `async_function_resume` and the async-generator driver do
for every `await`/`yield` the transform manages to decompose. A handful of
call sites still reach `await_value` directly from code that has *not* gone
through that decomposition (the tree-walker's raw `eval_expr`/`exec_statement`
evaluation of `Await`/`yield*`, used as a fallback when a statement isn't
lowered into the state machine). When one of those sites is reached from a
context that itself has more synchronous work queued above it, `await_value`'s
inline drain runs other jobs *nested inside* a job that hasn't returned yet,
reordering microtasks relative to spec/Node. Issue #687 is the standing audit
of this bug class; this cycle covers what's left after #707–#713, #725/#726
and the `await using`/disposal family (#665, #683–#686, #715/#716/#733)
already picked off every previously-found instance.

## 2. Audit of the 3 remaining `await_value` call sites on `origin/main`

On `b887b68b`, exactly 3 calls to `self.await_value(...)` remain outside its
own definition (confirmed: zero remain in `generator_runtime.rs` — the last
ones were removed by #711/#712/#720/#732/#734):

1. **`eval.rs:939`** — inside `eval_expr`'s `Expression::Yield(expr, delegate)`
   arm, the `is_async_gen` branch of `yield*` (`eval.rs:915-943`), gated on
   `self.generator_context.as_ref().map(|c| c.is_async)`.
2. **`eval.rs:1025`** — the raw `Expression::Await(expr)` handler in `eval_expr`.
3. **`exec.rs:2315`** — `exec_for_of_loop`'s `fo.is_await` branch (the tree-walker's
   generic, non-decomposed `for await` iteration step).

Disposition of each, established by static tracing **and** an empirical probe
(built `b887b68b` in a disposable worktree, ran witness-chain repros against
Node 26.9.0):

### 2a. `eval.rs:939` (`is_async_gen` yield\* branch) — dead code, delete it

`self.generator_context` is constructed in exactly 5 places, all in
`generator_runtime.rs` (verified: no other file constructs
`GeneratorContext { .. }` or assigns `self.generator_context = `). Only one of
the 5 sets `is_async: true` (the async-generator inline-replay driver, just
before its call to `exec_state_machine_body(.., .., .., true)`); the other 4
(sync-generator driver and its own inline-replay re-entry) all set
`is_async: false`. `exec_state_machine_body`'s 4th argument
(`async_generator: bool`) is threaded straight into
`self.in_async_generator_body` for the duration of the call
(`exec.rs:29-51`). So whenever `generator_context.is_async` can be `true`,
`self.in_async_generator_body` is *also* `true` for that entire call —
meaning the guard immediately above this branch
(`if self.in_async_generator_body { return self.eval_inline_async_yield_star(...) }`,
`eval.rs:904`, added by #710/ADR-2026-09-21-2157) always fires first. The
`is_async_gen`/`await_value` branch below it is therefore unreachable. This
matches ADR-2026-09-21-2157's own note ("kept only for the legacy
`IteratorState::Generator`/`AsyncGenerator` paths... dead code left for
#711") — but #711's own scope (confirmed via its issue body) only deleted the
legacy driver *functions* in `generator_runtime.rs`, not this branch in
`eval.rs`. It was never actually removed.

### 2b. `eval.rs:1025` (raw `Expression::Await`) — no currently-live reacher found; not this issue's scope

Two independent theories for reaching this from an async generator were
probed against Node 26.9.0 on the built `b887b68b` binary:

- Bare `await` co-located with `await using` in the same intact ("Isolated
  Block") block: `async function* g(){ { await using r = ...; await p; } }`.
  **Matches Node exactly** (`sync-end` logs before any queued `.then`, i.e.
  `.next()` returns promptly — no inline drain).
- Bare `await` nested in a container (`if`) with no `await using` at all,
  mirroring #685's own repro shape for `for await`: `async function* g(){ if
  (true) { await p; } }`. **Matches Node exactly.**

Both appear to already be covered by the disposal/scope-state suspension work
landed after the stale branch point (#686, #715, #716, #733) and by
`contains_suspension`'s general (non-container-specific) handling of
`Expression::Await`. The one *documented, still-live* reacher of this line is
explicit in `generator_analysis.rs`'s own comment on `contains_suspension`
(`Statement::Variable` arm): a pattern shape the transform can't yet lower
(array-pattern defaults, an object rest beside a suspending sibling) "needs...
the pre-existing blocking-tree-walker path" *by design*, pending proper
lowering. That is exactly **issue #725** ("await in array-pattern defaults...
with a blocking await_value", open, `sym:claimed`/`sym:running` — a peer
session is on it right now) and its closed sibling #726/#709. **Do not touch
pattern-default lowering here** — it is owned, in flight, and out of scope.

No new, currently-unowned reacher of `eval.rs:1025` was found. This line is
therefore left as-is: still a correct (if degraded) blocking fallback for the
one documented live case, whose fix belongs to #725.

### 2c. `exec.rs:2315` (`exec_for_of_loop`'s `is_await` step) — already owned by open issue #685

Issue #685's own text ("Newly discovered while implementing #699... not
new — reproduces today for a plain `for await` with no `await using` at all")
describes precisely this site and its root cause: `generator_analysis.rs`'s
`contains_suspension` recurses into `Statement::ForOf`'s own arm for a `for
await` nested inside `if`/`try`/a loop body/`switch`, but that arm never
consults `awaits_at_head()`/`is_await` (only the top-level `stmt_has_suspension`
fast path does), so the loop is never marked suspension-needing and the whole
statement falls back to the tree-walker, hitting this exact blocking
`await_value` call. #685 is `agent-ready`, open, not currently claimed. **Do
not duplicate #685's scope here** — it already owns this line, with a wider
remit (C-style `for` heads, iterator-close unwind paths) that a narrower fix
here would conflict with.

### Conclusion

Of the 3 remaining call sites, 2 are already fully owned by open, in-progress
sibling issues (#725 for 2b, #685 for 2c). The only item left in #687's own
scope is the dead branch at 2a.

## 3. Spec basis

- **Await** (`sec-await`, spec.html:51047-51081): `Await(value)` resolves
  `value`, registers `fulfilledClosure`/`rejectedClosure` (each of which
  *resumes* the asyncContext when run), calls `PerformPromiseThen`, pops
  `asyncContext`, and resumes the *caller's* context. It never loops on the
  job queue itself — whoever calls `Await` gets control back immediately.
- **Jobs** (`sec-jobs`, spec.html:11957-11997): "Once evaluation of a Job
  starts, it must run to completion before evaluation of any other Job
  starts in an agent." `await_value`'s inline drain launches new Jobs while
  the current one (the `.next()`/function call that led to it) has not
  finished — the failure mode this whole issue tracks.
- **PerformPromiseThen / NewPromiseReactionJob** (`sec-performpromisethen`,
  spec.html:49825-49871; `sec-newpromisereactionjob`, spec.html:49257-49297):
  settling a promise only *enqueues* reaction jobs; nothing runs synchronously.
- **`GetGeneratorKind`** (`sec-getgeneratorkind`, spec.html:50397-50407) and
  **`YieldExpression : yield * AssignmentExpression`**
  (`sec-generator-function-definitions-runtime-semantics-evaluation`,
  spec.html:24279-24282): `generatorKind` for a `yield*` is read from the
  Generator component of the *running execution context* — a property of the
  actual generator currently executing, never a separate mutable flag. This
  grounds why keying the dead branch's async/sync choice off
  `self.generator_context.is_async` (an interpreter-wide `Option` slot,
  decoupled from which generator is actually running) was the wrong shape
  even before it became provably unreachable; the surviving code path
  (`eval_inline_async_yield_star`, gated on `self.in_async_generator_body`,
  which is scoped to the actual replay call) is the one that matches this
  spec reading.

No new JavaScript syntax or semantics change: the only code change in this
slice deletes a provably-unreachable branch, and 2b/2c are deliberately left
untouched (owned elsewhere).

## 4. Files to touch

- `src/interpreter/eval.rs` — delete the dead `is_async_gen` branch inside
  `Expression::Yield`'s delegate arm (`eval.rs:915-943` on `b887b68b`; exact
  span to re-locate after the branch sync in §0, since line numbers will have
  shifted again by the time this slice starts). Collapse
  `let iterator = if is_async_gen { get_async_iterator } else { get_iterator }`
  to always call `get_iterator`, and drop the
  `let next_result = if is_async_gen { await_value(...) } else { next_result }`
  indirection entirely (sync yield* never awaits).
- `docs/adr/2026-09-21-2157-inline-yield-suspension.md` — the ADR already
  says this branch was "dead code left for #711"; add a one-line follow-up
  note (or a dated addendum) recording that it was actually deleted under
  #687, since #711's own scope turned out not to cover it. (Small doc
  correction, not a new architectural decision — no new ADR needed.)
- No `CONTEXT.md` change: no new vocabulary is introduced; "InlineYield" and
  "Isolated Block" already cover the surviving mechanism.

## 5. TDD slices

1. **Sync this branch onto `origin/main`** (§0). Not a code slice, but a hard
   prerequisite — re-verify `cargo build --release` and
   `uv run python scripts/run-custom-tests.py` are green immediately after,
   before touching anything else, so a bad sync is caught before slice 2.
2. **Prove `eval.rs`'s `is_async_gen` branch dead, then delete it.**
   - "Red" here is a proof, not a failing test: add a temporary
     `unreachable!("is_async_gen yield* branch: reachable — report on issue
     #687")` in place of the branch body, run the *full* test262 suite
     (`uv run python scripts/run-test262.py`) plus
     `uv run python scripts/run-custom-tests.py` plus `cargo test --release`.
     Zero new failures (in particular no `unreachable!` panics surfacing as
     new test262 crashes/timeouts) is the proof of unreachability.
   - "Green": replace the temporary `unreachable!` with the actual deletion —
     simplify the block to the sync-only path described in §4. Re-run the
     same full suite; it must be byte-for-byte identical in pass/fail counts
     to the pre-change baseline (this is a refactor with zero intended
     observable change, so "no diff in the pass list" is the correctness
     bar, not a new passing test).
   - No new `test262-extra`/`tests/` file: there is no new spec-correct
     behavior to pin — the change is the removal of unreachable code, and the
     existing sync-generator `yield*` test262 coverage (see §6) already pins
     the surviving path's behavior.
3. **Close out the audit.** Post a `gh issue comment 687` mapping all 3
   remaining call sites to their disposition (2a deleted here, 2b owned by
   #725, 2c owned by #685), and recommend `Closes #687` in the PR body (not
   the title — the squash-merge subject comes from the PR title verbatim, and
   `Closes` belongs in the body for GitHub's auto-close to trigger), following
   the same "superseded, tracked by dedicated follow-ups" reasoning the
   `/pm-triage` session used to close the parent issue #665.

## 6. Test surface

- **Targeted test262, before and after slice 2**, to prove no regression:
  - `test262/test/language/statements/generator/` (sync generator `yield*`
    surface the surviving branch implements)
  - `test262/test/language/expressions/yield/`
  - `test262/test/built-ins/GeneratorPrototype/`
  - `test262/test/language/statements/async-generator/` and
    `test262/test/built-ins/AsyncGeneratorPrototype/` (to confirm the
    already-fixed `eval_inline_async_yield_star` path, which this slice does
    not touch, still passes unchanged)
- **Full suite** (`uv run python scripts/run-test262.py`) is required for
  slice 2's "red" proof-of-unreachability step, not just the targeted dirs —
  unreachability is a whole-program claim, not a local one.
- **No new `test262-extra` test**: nothing spec-correct is newly pinned; see
  slice 2's rationale.
- `cargo test --release` for the Rust-level regression gate (unit tests
  touching `generator_context`/`GeneratorContext`, if any, will catch a
  mistaken deletion at compile or test time).

## 7. Regression risk

- **Hot path risk is low**: `Expression::Yield` with `delegate: true` is the
  tree-walker's generic `eval_expr` dispatch for every `yield*`, including the
  common sync-generator case — this slice simplifies, but does not change the
  control flow reachable for sync generators (only removes an already-dead
  conditional branch), so no new branch cost is added to the hot path.
- **Shared machinery leaned on**: `self.generator_context` and
  `self.in_async_generator_body` are both read (not written) by this change;
  the write sites (`generator_runtime.rs`) are untouched, so no other
  generator/async-function driver is at risk.
- **`test262-pass.txt` baseline**: this slice must not move it (per project
  convention, rewriting the baseline is a `main`-branch operation). The
  full-suite run in slice 2 is specifically to confirm the pass/fail set is
  identical before and after — any diff at all is a sign the branch was not
  actually dead and the slice must be reverted/re-scoped, not accepted.
- **Branch-sync risk (§0)**: the biggest real risk in this cycle is not the
  code change but mis-syncing the branch. If `git reset --hard origin/main`
  cannot be run (same permission restriction hit during planning), the
  implementation stage must escalate/request it explicitly rather than
  attempting the change on the stale tree — building on the stale branch
  would silently resurrect the 3 already-upstream drain-removal fixes as a
  *second*, redundant copy, and would evaluate "dead code" claims (§2a)
  against code that no longer matches `main`.

## 8. Out of scope

- **`eval.rs:1025` (bare `Expression::Await`) and its one known live reacher**
  (array-pattern defaults) — owned by open, actively-running issue #725 and
  closed sibling #726/#709. Do not touch pattern-initializer lowering here.
- **`exec.rs:2315` (`for await` nested in a container)** — owned by open
  issue #685, which has a wider remit (C-style `for` heads, iterator-close
  unwind paths) than a point fix here would address; a narrow fix in this PR
  would conflict with #685's eventual change.
- **Any further `has_suspendable_await_using_block`/`contains_suspension`
  coverage audit** beyond what's needed to resolve 2a/2b/2c — not attempted;
  no new gap was found beyond the ones already tracked.
- **The legacy `IteratorState::Generator`/`AsyncGenerator` driver functions**
  — already deleted by #711; nothing left to clean up there.
- **Formatting/unrelated cleanup** in `eval.rs`/`generator_runtime.rs` beyond
  the single dead branch — no drive-by refactors.
