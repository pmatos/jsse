# Plan: issue #687 — blocking `await_value` callers drain the microtask queue inline

## 0. Critical precondition: this branch is stale and must be synced first

This branch (`sym/jsse/687-...`) still carries 5 old commits (`d448dba6` plan,
`9caed474`/`d2b51722`/`61799f65` fixes, `f79b3879` plan-drop) from a prior
planning→implementation cycle. That cycle's PR **#714 is already merged to
`main`** (squash commit `f84dc6fc`) — verified by diffing: every line those 3
commits removed is already absent at the equivalent spot in `origin/main`
(commit `b887b68b`, fetched during this planning session). Since #714 merged,
`main` gained **~15 more merged PRs** directly in this same bug class (#707,
#708, #709, #721, #727/#753 — declaration-pattern yield desugars — and
#712/#713/#716/#733), all closed as "Follow-up of #687" or of each other.
#725 (array-pattern `await` defaults) is still open, not merged. None of
this is on this branch.

**Before any other work in the implementation stage:**

```
git fetch origin main
git revert --no-edit 61799f65 d2b51722 9caed474
git merge origin/main
```

Do **not** `git reset --hard` and do not revert the two `docs(plan)`/`chore`
PLAN.md commits (`d448dba6`, `f79b3879`): only the 3 fix commits need
reverting (their changes are a verified content-subset of `main`, see §0
below's diff check); the two PLAN.md commits already cancel each other out
(one adds `PLAN.md`, the other `git rm`s it) and reverting either just
re-introduces spurious PLAN.md churn into the merge. A plain
`git merge origin/main` *without* the reverts first was tried and produces a
real textual conflict in `generator_runtime.rs` (the intervening ~4,000-line
rewrite of that file makes a 3-way merge apply the 3 fix commits' small diffs
to the wrong context, even though their net effect already exists upstream).
With the 3 reverts first, this was verified to merge clean: in a disposable,
detached `git worktree add --detach "$TMPDIR/sync" HEAD`, running the 3
reverts followed by `git merge-tree --write-tree HEAD origin/main` produced a
clean tree (no `CONFLICT` output) whose only diff from `origin/main` was this
very `PLAN.md` file — i.e. the merge-base-to-`ours` diff really is nothing
but the plan now that the 3 fix commits are undone. (This planning run's own
sandbox permission classifier blocked a plain `git reset --hard`/`git merge`
attempt as "Irreversible Local Destruction" with no human available to
approve in this headless run, so the *actual* revert+merge was only verified
in the disposable worktree above, never applied to this branch — that is
deliberate: this stage commits `PLAN.md` only. If the implementation stage
hits the same classifier block on the revert+merge: do not attempt to work
around it with a different destructive git incantation, and do not proceed
to make code changes on the stale, unsynced tree. Instead, post
`gh issue comment 687` naming the exact command that was blocked and why
(quoting the classifier's denial), and stop without making any source
changes — building on the stale branch would silently resurrect the 3
already-upstream drain-removal fixes as a *second*, redundant copy, and
would evaluate the rest of this plan's file/line references against code
that no longer matches `main`.)

All line numbers, code quotes, and behavioral claims in the rest of this plan
were produced against `origin/main` at `b887b68b` — via `git show`, and via a
disposable `git worktree add "$TMPDIR/..." FETCH_HEAD` that was built
(`cargo build --release`) and exercised directly (hand-written repros, cross-
checked against Node 26.9.0, and `uv run python scripts/run-test262.py`) —
**not** from this branch's stale working tree, which currently disagrees with
`origin/main` on all three of these files.

## 1. Problem restated

`Interpreter::await_value` (`src/interpreter/eval.rs:9878` on `b887b68b`) is
jsse's blocking fallback for `Await`: it registers fulfill/reject reactions on
the awaited promise and then loops, popping and running jobs from the
interpreter's own microtask queue synchronously, until its own reaction
fires. This is correct only when nothing else is left running on the Rust
call stack above it. The architecturally-correct mechanism —
`StateTerminator::Await` in the generator/async state machine, and its
`SentValueBindingKind::InlineYield`-tagged variant for constructs the
transform can't decompose (ADR-2026-09-21-2157) — instead registers a
reaction and returns control immediately, letting the real job queue redrive
the continuation later. A few call sites still reach `await_value` directly
from code that has *not* gone through that treatment. Issue #687 is the
standing audit of this bug class; this cycle covers what's left after
#707–#713, #721, #726, #727/#753, and the `await using`/disposal family
(#665, #683–#686, #715/#716/#733) already picked off every previously-found
instance (#725 is still open/in-flight, not yet landed). The audit below finds that of 3 remaining raw call sites, one is
dead code and one has no confirmed live reacher, but the third — a `for
await` loop whose left-hand binding pattern itself contains a `yield`, inside
an async generator — is a **new, confirmed-live instance**: `exec_for_of_loop`
(`exec.rs`) runs that loop's per-iteration `Await(nextResult)` through
`await_value` with no awareness of the generator-state-machine replay it's
embedded in, so the loop's own iteration side effects and any concurrently-
queued jobs run inside a `.next()` call that should have returned promptly.

## 2. Audit of the 3 remaining `await_value` call sites on `origin/main`

On `b887b68b`, exactly 3 calls to `self.await_value(...)` remain outside its
own definition (confirmed: zero remain in `generator_runtime.rs` — the last
ones were removed by #711/#712/#720/#732/#734):

1. **`eval.rs:915-919`** (the `is_async_gen` check) / **`eval.rs:939`** (the
   `await_value` call) — inside `eval_expr`'s `Expression::Yield(expr,
   delegate)` arm, the `is_async_gen` branch of `yield*`.
2. **`eval.rs:1026-1031`** — the raw `Expression::Await(expr)` handler in
   `eval_expr`.
3. **`exec.rs:2333-2339`** — `exec_for_of_loop`'s `fo.is_await` branch (the
   tree-walker's generic, non-decomposed `for await` iteration step).

Disposition of each, established by static tracing, an empirical probe
(hand-written repros cross-checked against Node 26.9.0), and — critically,
after an initial pass got this wrong — **direct instrumentation**: a
temporary `eprintln!` was added at each of the 3 sites, the probe worktree
rebuilt, and run against every `.js` file (harness-concatenated) under
`test262/test/language/statements/generators`,
`language/expressions/yield`, `built-ins/GeneratorPrototype`,
`language/statements/async-generator`, `built-ins/AsyncGeneratorPrototype`,
`language/statements/async-function`, and `language/statements/for-await-of`
(2,431 scenarios in the last directory alone). This is the only way to be
sure a given branch fires or doesn't: an earlier draft of this plan reasoned
sites 1 and 3 were unreachable/already-owned from code-reading and two
hand-picked repros alone, and the instrumentation run below directly
contradicted that for site 3.

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
`eval.rs:904`, added by #710/ADR-2026-09-21-2157) always fires first.
**Instrumentation confirms this empirically**: zero hits across all 7 probed
test262 directories, including every `async-generator`/`AsyncGeneratorPrototype`
scenario. This matches ADR-2026-09-21-2157's own note ("kept only for the
legacy `IteratorState::Generator`/`AsyncGenerator` paths... dead code left for
#711") — but #711's own scope (confirmed via its issue body) only deleted the
legacy driver *functions* in `generator_runtime.rs`, not this branch in
`eval.rs`. It was never actually removed.

A residual concern raised during review — does a *sync* generator's `yield*`,
stepped from inside a *different*, currently-replaying async generator's
inline fallback, observe a **leaked** `generator_context.is_async == true`
(since the sync driver only assigns `generator_context` on its own replay,
and doesn't clear a pre-existing one on a normal, non-replay step, while its
own `exec_state_machine_body(.., false)` call still forces
`in_async_generator_body` back to `false` for that inner step, which would
reopen exactly this branch with a stale `true`)? This was probed twice,
instrumented the same way. The first attempt used an *object*-pattern
default (`var {a = yield 1} = {}`), which #728 already lowers via
`pattern_needs_lowering` — so it never actually entered the InlineYield
replay/never set `generator_context` at all, and proved nothing. Redone with
an *array*-pattern default (`var [a = yield 1] = []`), which
`pattern_lowering_supported` still declines (confirmed still on the replay
path: the resumed value logs correctly, proving the InlineYield fallback did
engage), nesting a plain `function*` with `yield* [10,20,30]` inside that
replay: still **no hit**, output matching Node exactly. The theoretical leak
window is closed by some other invariant not fully traced here (plausibly: a
generator driven by `.next()` always routes through its own
`exec_state_machine_body` call, whose lifetime brackets the entire nested
execution, and nothing in between constructs a *second* `generator_context`
without first consuming/clearing the first). Treat this as empirically dead
rather than formally proven dead — slice 2's full-suite proof-of-
unreachability step (§5) is the actual gate, not this writeup, and if the
branch does somehow fire there, deleting it is still the correct fix
(§3's `GetGeneratorKind` grounding holds regardless: the *running* generator
in that scenario is the sync one, so branching on an async flag is wrong by
construction either way).

### 2b. `eval.rs:1026` (raw `Expression::Await`) — no currently-live reacher found; not this issue's scope

Instrumented the same way: **zero hits** across all 7 probed test262
directories. Two hand-written repros targeting the theories that motivated
checking this line at all — bare `await` co-located with `await using` in the
same intact ("Isolated Block") block, and bare `await` nested in a plain `if`
with no `await using` at all — both matched Node exactly (confirmed with
correct, working repros this time; an earlier draft's Node comparison for the
first repro was accidentally checked against a `print`-crashed Node process
and is corrected here). The one *documented, still-live* reacher of this line
is explicit in `generator_analysis.rs`'s own comment on `contains_suspension`
(`Statement::Variable` arm): a pattern shape the transform can't yet lower
(array-pattern defaults, an object rest beside a suspending sibling) "needs...
the pre-existing blocking-tree-walker path" *by design*, pending proper
lowering. That is exactly **issue #725** ("await in array-pattern defaults...
with a blocking await_value", open, `sym:claimed`/`sym:running` — a peer
session is on it right now) and its closed sibling #726/#709. **Do not touch
pattern-default lowering here** — it is owned, in flight, and out of scope.

No new, currently-unowned reacher of `eval.rs:1026` was found. This line is
therefore left as-is: still a correct (if degraded) blocking fallback for the
one documented live case, whose fix belongs to #725.

### 2c. `exec.rs:2333` (`exec_for_of_loop`'s `is_await` step) — confirmed live; in scope

An earlier draft of this plan assumed this line was already fully owned by
open issue #685 (whose own body flags a "nested container" `for await` gap)
and closed issue #721/PR #737 ("recurse into containers when detecting
for-await suspension"). Both **hand-written repros reproducing #721's exact
shape and the advisor's suggested nested-`if` shape matched Node exactly** —
#721/#737 is indeed fully fixed. But the instrumentation sweep found **37
hits, all with `in_async_generator_body == true`**, concentrated in exactly
13 test262 files, all under `for-await-of/async-gen-decl-dstr-*-yield-expr.js`
and `async-gen-decl-dstr-array-elem-iter-rtrn-close-null.js` — every one of
them a `for await ([pattern = yield ...] of iterable)` (or object-pattern
equivalent) **inside an async generator**, i.e. a `for await` whose *own
left-hand binding pattern* contains a bare `yield`.

Root cause has two layers — an earlier draft of this plan found only the
second and had the causality backwards (assumed detection was already
correct and the gap was purely in the transform):

**Layer 1 — per-statement detection never sees a `ForInOfLeft::Pattern`
head's `yield` at all, so the statement never reaches the transform
function where any desugar could live.** `generator_analysis.rs`'s
`for_in_of_variable_head_contains_yield` (line 850-860; its name says
"variable" for a reason) is the single helper `contains_yield`'s and
`contains_suspension`'s `Statement::ForIn`/`Statement::ForOf` arms both call
to decide whether *this specific loop* needs lowering — and it is hard-coded
`ForInOfLeft::Pattern(_) | ForInOfLeft::Expression(_) => false`, with the
comment "Assignment heads still need their own lowering" (i.e. a
self-documented, not-yet-done TODO). Separately, `analyze_statement`'s own
`ForInOfLeft::Pattern(_) => { /* Pattern LHS is an assignment target, not a
declaration */ }` arms (both `ForIn` and `ForOf`, `generator_analysis.rs`
~line 270 and ~310) are literal no-ops — unlike the `Variable` arm right
above each one, which calls `analyze_pattern_expressions` to register the
pattern's embedded `yield` as a counted yield point. Net effect: a for-of
statement whose *only* suspension is a `yield` inside its assignment-form
head pattern is invisible to every one of these checks, so the per-statement
dispatch never routes it through `transform_for_in_of_loop` at all — it
stays a raw `Statement::ForOf`, tree-walked whenever the containing state
executes it, which is exactly why `exec_for_of_loop` fires (confirmed by the
`in_async_generator_body == true` instrumentation: the containing *function*
still got a compiled state machine — via *other* content in those test262
templates registering real yield points — but *this specific statement*
never got compiled, and runs raw inside the InlineYield backstop's replay).

**Layer 2 — even once detection is fixed, the transform still needs a
desugar**, because `ForOfHead`/`ForOfInit` bind their `left` via a single,
non-suspending runtime call (the same constraint documented on
`hoist_yield_pattern`, `generator_transform.rs:1795-1798`) and cannot run a
yield-containing pattern through it directly — per ADR-2026-09-21-1752, doing
so would silently drop the `Completion::Yield` and bind the wrong value.
**Both layers must land together**: shipping layer 1 alone would make
`ForOfHead` bind a yield-containing pattern directly and silently corrupt the
value; shipping layer 2 alone is simply never reached.

This exact two-layer shape was already solved once, for the sibling
*declaration* head (`for (var {a = yield 1} of x)`/`ForInOfLeft::Variable`):
- **Layer 2 (desugar) shipped first**, under #727/ADR-2026-09-21-1752:
  `transform_for_in_of_loop` (`generator_transform.rs:2682`) calls
  `hoist_yield_pattern` when the `Variable` head's pattern contains a
  `yield`, rewriting the loop to
  `for (var $tmp of x) { let {a = yield 1} = $tmp; body }` — the trivial
  `$tmp` binding flows through the unchanged, already-correct
  `ForOfInit`/`ForOfHead` states (#707/#720/#732/#734), and the real pattern
  becomes an ordinary `Statement::Variable` in the body.
- **Layer 1 (detection) shipped later**, under #753/PR #760 (`git show
  1227c8c8`, commit message "preserve iteration across head-pattern yield"):
  before this landed, a `Variable` head whose *only* suspension was its
  pattern's `yield` had the identical "never reaches
  `transform_for_in_of_loop`, stays tree-walked, replays and re-fetches the
  iterator on every resume" bug #753's own repro describes — PR #760's
  *entire* diff is 4 additions to `generator_analysis.rs`: the two
  `analyze_pattern_expressions` calls in `analyze_statement`'s `Variable`
  arms (already shown above, present today), and widening `contains_yield`/
  `contains_suspension`'s `ForIn`/`ForOf` arms to call the new
  `for_in_of_variable_head_contains_yield` helper. It touched *no* file
  under `generator_transform.rs` — layer 2 was already in place from #727.

**This plan's fix is the mirror image of #753/PR #760, for `ForInOfLeft::Pattern`
instead of `ForInOfLeft::Variable`, landing both layers in one slice since
neither is safe alone**:
- Layer 1: widen `for_in_of_variable_head_contains_yield`'s
  `ForInOfLeft::Pattern(pattern)` arm from `false` to
  `pattern_contains_yield(pattern)` (likely also renaming the function, since
  it's no longer variable-only) — `contains_yield`/`contains_suspension`'s
  4 call sites need no further change, they already delegate to this one
  helper. And fill in `analyze_statement`'s two no-op
  `ForInOfLeft::Pattern(_) => {}` arms with
  `analyze_pattern_expressions(pattern, analysis, ctx)` — confirmed reusable
  as-is: it takes a bare `&Pattern`, with no declaration-specific parameter
  (`generator_analysis.rs:675-679`).
- Layer 2: extend `transform_for_in_of_loop`'s existing desugar (see §4) to
  also cover `ForInOfLeft::Pattern` — synthesizing an assignment statement
  (`<pattern> = $tmp;`) prepended to the loop body instead of
  `hoist_yield_pattern`'s `let`-declaration form, routing through the
  *existing*, already-correct destructuring-*assignment* yield handling
  ADR-2157 already documents as working (`x[yield 1] = yield 2`).

**No open issue tracks this specific residual**: searched for
`ForInOfLeft::Pattern`, `lower_pattern_assignment`, and
`dstr-assignment-for-await` across all issues (open and closed) and found
none; #724 (closed) is the equivalent *await*-in-assignment-pattern gap
(unrelated construct), #725 (open) is *await* in *array*-pattern
*declaration* defaults, #753 (closed) is *yield* in *declaration* heads —
none is *yield* in an *assignment* head. ADR-2026-09-22-1752's "What this
still does not cover" section explicitly names
`ForInOfLeft::Pattern`/"uses a different lowering pipeline (`lower_pattern_assignment`)
this follow-up didn't touch" as left open.

This is a **transform/analysis-time fix, not a runtime one**: no change to
`exec.rs`/`exec_for_of_loop`/`await_value` is needed at all, since once both
layers land, the loop head becomes suspension-free (from the compiled
state machine's point of view) and is handled entirely by the already-correct
`ForOfInit`/`ForOfHead` states. An earlier draft of this plan proposed a
runtime fast-forward mechanism inside `exec_for_of_loop` itself, modeled on
`eval_inline_async_yield_star`; that was reviewed and rejected — it fights
the codebase's own established two-layer pattern (detect at analysis time,
desugar at transform time, never teach the tree-walker's own runtime driver
to suspend) and rested on an untested premise about what "fast-forward"
would even mean for a loop *iteration count* rather than a single expression
value.

**Confirmed observable divergence** (new repro, not in any existing test):

```js
var log=[]; var L=x=>log.push(x);
Promise.resolve().then(()=>L('w1')).then(()=>L('w2')).then(()=>L('w3'));
function mkIter(log){
  var n=0;
  return { [Symbol.asyncIterator](){ return { next(){
    log.push('iter-next'+(n+1));
    return Promise.resolve(n>=1?{done:true}:{done:false,value:[++n]});
  }}}}
}
async function* g(){
  for await ([x = yield] of mkIter(log)) { L('body'); }
}
var it = g();
it.next().then(r=>L('first-next-done'+r.done));
L('sync-end');
setTimeout(()=>{ it.next(4).then(r=>L('second-next-done'+r.done)); }, 10);
```

- Node: `iter-next1,sync-end,w1,body,iter-next2,w2,w3,first-next-donetrue,second-next-donetrue`
- jsse: `iter-next1,w1,body,iter-next2,w2,sync-end,w3,first-next-donetrue,second-next-donetrue`

`sync-end` — which must log immediately after the *first* `it.next()` call
returns — instead logs after `w1`, `body`, `iter-next2`, and `w2`: the entire
first loop iteration (including a second iterator-next call) runs *inside*
the synchronous `it.next()` call, exactly the "job runs while the caller's
stack hasn't unwound" violation this issue tracks. All 13 test262 files that
exercise this shape happen to assert only final *values*
(`assert.sameValue`/`compareArray`), not concurrent ordering, so
`uv run python scripts/run-test262.py test262/test/language/statements/for-await-of/`
is 100% green today despite the bug (2,431/2,431 passing) — this is why a
new `test262-extra` witness-chain test is needed (§6), following the same
pattern already used for #707's/#709's sibling fixes.

**Caveat on this specific repro, per review**: its per-iteration value is
`[++n]` (never `undefined`), so `[x = yield]`'s default is never actually
evaluated and the `yield` never fires — this repro demonstrates the ordering
violation (`await_value` blocking) but *not* #753's sibling "replay restarts
the loop and re-fetches the iterator" concern, since no replay is triggered
here at all. A second scenario is needed to exercise that: a value of
`undefined` at least once (so the default's `yield` does fire and a real
`.next(sentValue)` resume/replay happens), with call counters on
`[Symbol.asyncIterator]` and `next` checked across the resume — mirroring
#753's own counting-iterator test (`async-generator-for-in-of-head-yield-keeps-iteration.js`).
Today, this should show the iterator being re-acquired and/or re-stepped on
resume (the same bug class #753 fixed for `Variable` heads, now confirmed
reachable via a `Pattern` head too); after the fix, exactly one
`GetIterator` and one `next()` call per element. Slice 3's red step should
add this second scenario alongside the ordering repro above, rather than
relying on the ordering repro alone.

## 3. Spec basis

- **Await** (`sec-await`, spec.html:51047-51081): `Await(value)` resolves
  `value`, registers `fulfilledClosure`/`rejectedClosure` (each of which
  *resumes* the asyncContext when run), calls `PerformPromiseThen`, pops
  `asyncContext`, and resumes the *caller's* context. It never loops on the
  job queue itself — whoever calls `Await` gets control back immediately.
- **ForIn/OfBodyEvaluation** (spec.html:22401-22420): within the per-iteration
  `Repeat` loop, "Let nextResult be ? Call(...). If iteratorKind is ~async~,
  set nextResult to ? Await(nextResult)... Let nextValue be ?
  IteratorValue(nextResult)" happens *before*, and as a separate step from,
  "Let status be Completion(DestructuringAssignmentEvaluation of
  assignmentPattern with argument nextValue)" (for an assignment-form head,
  `lhsKind` ~assignment~). The spec itself already treats "obtain the
  (possibly-awaited) next value" and "destructure it into the head pattern"
  as two sequential steps against an implicit intermediate value — this is
  the exact grounding for `for await ($t of it) { <pattern> = $t; body }`
  being a meaning-preserving rewrite of `for await (<pattern> of it)`: it
  just makes the spec's own implicit two-step sequence into two explicit
  statements, in the same order. `exec_for_of_loop` implements this same
  `Await(nextResult)` step for the tree-walked path; this `Await` is subject
  to the same suspend/resume contract as any other, and nothing about being
  a loop head exempts it.
- **Jobs** (`sec-jobs`, spec.html:11957-11997): "Once evaluation of a Job
  starts, it must run to completion before evaluation of any other Job
  starts in an agent." `await_value`'s inline drain launches new Jobs while
  the current one (the `.next()` call that led to it) has not finished — the
  failure mode this whole issue tracks, and what the confirmed repro in §2c
  demonstrates directly.
- **PerformPromiseThen / NewPromiseReactionJob** (`sec-performpromisethen`,
  spec.html:49825-49871; `sec-newpromisereactionjob`, spec.html:49257-49297):
  settling a promise only *enqueues* reaction jobs; nothing runs
  synchronously.
- **`GetGeneratorKind`** (`sec-getgeneratorkind`, spec.html:50397-50407) and
  **`YieldExpression : yield * AssignmentExpression`**
  (`sec-generator-function-definitions-runtime-semantics-evaluation`,
  spec.html:24279-24282): `generatorKind` for a `yield*` is read from the
  Generator component of the *running execution context* — a property of the
  actual generator currently executing, never a separate mutable flag. This
  grounds why keying §2a's dead branch's async/sync choice off
  `self.generator_context.is_async` (an interpreter-wide `Option` slot,
  decoupled from which generator is actually running) was the wrong shape
  even before it became unreachable; the surviving code path
  (`eval_inline_async_yield_star`, gated on `self.in_async_generator_body`,
  scoped to the actual replay call) is the one that matches this spec
  reading.

No new JavaScript syntax: §2a deletes a provably/empirically unreachable
branch (no behavior change); §2c's fix makes an already-specified `Await`
step (ForIn/OfBodyEvaluation) suspend correctly instead of draining inline —
it changes *timing*, which the spec already mandates, not what value is
produced. §2b is deliberately left untouched (owned elsewhere).

## 4. Files to touch

- `src/interpreter/eval.rs` — delete the dead `is_async_gen` branch inside
  `Expression::Yield`'s delegate arm (`eval.rs:915-943` on `b887b68b`; exact
  span to re-locate after the branch sync in §0). Collapse
  `let iterator = if is_async_gen { get_async_iterator } else { get_iterator }`
  to always call `get_iterator`, and drop the
  `let next_result = if is_async_gen { await_value(...) } else { next_result }`
  indirection entirely (sync yield* never awaits).
- `src/interpreter/generator_analysis.rs` — **layer 1 (detection), must land
  together with the layer-2 desugar below, not separately**:
  `for_in_of_variable_head_contains_yield` (line 850-860): widen the
  `ForInOfLeft::Pattern(_) => false` arm to
  `ForInOfLeft::Pattern(pattern) => pattern_contains_yield(pattern)` (the
  function likely wants renaming once it covers both head kinds —
  `contains_yield`/`contains_suspension`'s `ForIn`/`ForOf` arms already
  delegate to it uniformly, so no other call site needs touching). Also fill
  in `analyze_statement`'s two no-op `ForInOfLeft::Pattern(_) => {}` arms
  (`ForIn` around line 270, `ForOf` around line 310) with
  `analyze_pattern_expressions(pattern, analysis, ctx)` — same call the
  adjacent `Variable` arm already makes, confirmed reusable since the
  function takes a bare `&Pattern` with no declaration-specific parameter.
- `src/interpreter/generator_transform.rs` — **layer 2 (desugar)**,
  `transform_for_in_of_loop`
  (line 2682 on `b887b68b`), specifically the `rewritten_left` block at
  lines 2707-2719: extend the `if let ForInOfLeft::Variable(decl) = left`
  condition that calls `hoist_yield_pattern` to also cover
  `ForInOfLeft::Pattern(pattern)` when `pattern_contains_yield(pattern)`.
  For the `Variable` case the hoist synthesizes a `let`-declaration
  (`synth_pattern_let_decl`) prepended to the body; for the `Pattern`
  (assignment) case it needs a sibling helper that synthesizes a plain
  assignment statement (`<pattern> = $tmp;`, an
  `Expression::Assign`/`Statement::Expression`, not a declaration) instead —
  `hoist_yield_pattern` itself takes a `VarKind` specifically to build the
  declaration form, so this is naturally a new function (e.g.
  `hoist_yield_pattern_assignment`) sharing the `pattern_contains_yield`
  gate and temp-var plumbing, not a parameter added to the existing one.
  The rewritten loop head (`$tmp`) flows through the *unchanged*
  `ForOfInit`/`ForOfHead` terminator construction immediately below (lines
  2732-2755ish) exactly as the `Variable` case already does — no change
  needed there. **No change to `exec.rs`/`exec_for_of_loop`/`await_value`
  is needed**: once layer 1 (above) recognizes the pattern's `yield` and
  routes the statement into `transform_for_in_of_loop`, and this layer-2
  desugar moves the `yield` out of the head, `exec_for_of_loop` is never
  reached for this statement at all — the loop lowers normally through the
  already-correct `ForOfInit`/`ForOfHead` states.
- `docs/adr/2026-09-21-2157-inline-yield-suspension.md` — add a short dated
  addendum: the `is_async_gen` branch it flagged as "dead code left for
  #711" was not actually covered by #711's scope and was deleted under #687
  instead (small doc correction, not a new architectural decision).
- `docs/adr/2026-09-22-1752-yield-in-declaration-pattern-default.md` — add a
  short dated addendum: the "Left as residual... `ForInOfLeft::Pattern`"
  note is resolved by this change; point to the new ADR below for the actual
  design.
- A new dated ADR (e.g.
  `docs/adr/<date>-yield-in-assignment-pattern-for-of-head.md`) documenting
  the assignment-form desugar, following the existing ADR-1752/ADR for #753
  house style (problem, audit result, decision, known boundaries) — this
  extends an established family (declaration-head desugar) to the sibling
  assignment-head case, worth its own record the same way #753 got one for
  extending #727.
- `test262-extra/async-generator-for-await-assignment-head-yield-does-not-drain-microtasks.js`
  (or similar; see §6) — new regression test for §2c, following the existing
  witness-chain pattern (e.g.
  `test262-extra/async-generator-throw-at-start-does-not-drain-microtasks.js`).
- No `CONTEXT.md` change needed: "InlineYield" and the existing
  head-pattern-desugar vocabulary already cover this.

## 5. TDD slices

1. **Sync this branch onto `origin/main`** (§0). Not a code slice, but a hard
   prerequisite — re-verify `cargo build --release` and
   `uv run python scripts/run-custom-tests.py` are green immediately after,
   before touching anything else, so a bad sync is caught before slice 2.
2. **Prove `eval.rs`'s `is_async_gen` branch dead, then delete it** (§2a/§4).
   - "Red" here is a proof, not a failing test: add a temporary
     `unreachable!("is_async_gen yield* branch: reachable — report on issue
     #687")` in place of the branch body, run the *full* test262 suite
     (`uv run python scripts/run-test262.py`) plus
     `uv run python scripts/run-custom-tests.py` plus `cargo test --release`.
     Zero new failures (in particular no `unreachable!` panics surfacing as
     new test262 crashes/timeouts) is the proof of unreachability — this
     plan's own instrumentation run already did a scoped version of this
     (§2a) and found nothing, but the full suite is the actual gate.
   - "Green": replace the temporary `unreachable!` with the actual deletion —
     simplify the block to the sync-only path described in §4. Re-run the
     same full suite; it must be byte-for-byte identical in pass/fail counts
     to the pre-change baseline.
   - No new `test262-extra`/`tests/` file: there is no new spec-correct
     behavior to pin — the change is removal of unreachable code, and the
     existing sync-generator `yield*` test262 coverage (§6) already pins the
     surviving path's behavior.
3. **Recognize and desugar a `yield`-containing assignment-form head pattern
   in `for await`'s head** (§2c/§4) — the one real bug in this cycle's scope.
   Both layers land in this one slice; neither is independently safe (§2c).
   - **Red**: add the new `test262-extra` witness-chain test(s). Cover both
     the ordering repro from §2c (array-pattern head, value that never
     triggers the default, catching the `await_value` inline-drain) and the
     replay/double-iteration scenario §2c's caveat describes (a value of
     `undefined` at least once, so the pattern's `yield` actually fires, with
     call counters on `[Symbol.asyncIterator]`/`next` checked across
     `.next(sentValue)` — mirroring #753's own
     `async-generator-for-in-of-head-yield-keeps-iteration.js`). Also cover
     an object-pattern assignment head (`for await ({a = yield} of
     iterable)`), since both array- and object-pattern heads are
     `ForInOfLeft::Pattern`. Confirm all fail against current `origin/main`.
   - **Green**: implement both layers from §4 — widen
     `for_in_of_variable_head_contains_yield` and fill in
     `analyze_statement`'s two `ForInOfLeft::Pattern` no-ops
     (`generator_analysis.rs`), then add the `transform_for_in_of_loop`
     assignment-form desugar (`generator_transform.rs`, new
     `hoist_yield_pattern_assignment`-style helper gated on
     `ForInOfLeft::Pattern` + `pattern_contains_yield`). Re-run the new
     tests; all must match Node/expected behavior.
   - **Refactor** (same slice, small): once both the `Variable` and
     `Pattern` desugars exist side by side in `transform_for_in_of_loop`,
     check whether they share enough (temp-var naming, the synthesized
     statement's placement in the body) to warrant one small shared helper
     parameterized on "declare vs assign" — but only if the two call sites
     end up near-duplicates; don't force a premature abstraction if the
     declaration/assignment statement construction differs enough to make a
     shared helper awkward.
4. **Close out the audit.** Post a `gh issue comment 687` mapping all 3
   remaining call sites to their disposition (2a deleted, 2b left as-is/owned
   by #725, 2c fixed here), and recommend `Refs #687` rather than `Closes
   #687` in the PR body — §2b's one documented live reacher is still open
   (via #725), so the audit is not fully closed out even though this PR
   resolves the only two items that were actually this issue's own to fix.

## 6. Test surface

- **Targeted test262, before and after slice 2 (dead-code deletion)**:
  - `test262/test/language/statements/generators/` and
    `test262/test/language/expressions/yield/` (sync generator `yield*`
    surface the surviving branch implements)
  - `test262/test/built-ins/GeneratorPrototype/`
  - `test262/test/language/statements/async-generator/` and
    `test262/test/built-ins/AsyncGeneratorPrototype/` (confirm the
    already-fixed `eval_inline_async_yield_star` path, untouched by slice 2,
    still passes unchanged)
- **Targeted test262 for slice 3**:
  - `test262/test/language/statements/for-await-of/` — specifically the 13
    files identified by instrumentation:
    `async-gen-decl-dstr-array-elem-init-yield-expr.js`,
    `async-gen-decl-dstr-array-elem-iter-rtrn-close-null.js`,
    `async-gen-decl-dstr-array-elem-nested-array-yield-expr.js`,
    `async-gen-decl-dstr-array-elem-nested-obj-yield-expr.js`,
    `async-gen-decl-dstr-array-elem-target-yield-expr.js`,
    `async-gen-decl-dstr-array-rest-nested-array-yield-expr.js`,
    `async-gen-decl-dstr-array-rest-nested-obj-yield-expr.js`,
    `async-gen-decl-dstr-array-rest-yield-expr.js`,
    `async-gen-decl-dstr-obj-id-init-yield-expr.js`,
    `async-gen-decl-dstr-obj-prop-elem-init-yield-expr.js`,
    `async-gen-decl-dstr-obj-prop-elem-target-yield-expr.js`,
    `async-gen-decl-dstr-obj-prop-nested-array-yield-expr.js`,
    `async-gen-decl-dstr-obj-prop-nested-obj-yield-expr.js`.
    These already pass today (2,431/2,431 in the directory) and must keep
    passing — they pin *values*, not ordering, so they do not catch this bug
    by themselves, but a bug in the desugar (e.g. binding the temp to the
    wrong value, or losing TDZ/scoping across the rewrite) would very
    plausibly break their values (they exercise exactly the destructuring +
    `for await` + inline-yield interaction the fix touches, just via the
    `ForInOfLeft::Variable` desugar already fixed by #727/#753 rather than
    the `ForInOfLeft::Pattern` one this slice adds). Re-run the full
    `for-await-of/` directory after the fix; it must stay at 100%.
  - `test262/test/language/statements/for-of/` and
    `test262/test/language/statements/for-in/` — the widened
    `for_in_of_variable_head_contains_yield`/`analyze_statement` changes
    (§4) are shared by *sync* `for-of`/`for-in` too (not just `for await`),
    since `contains_yield`'s `ForIn`/`ForOf` arms call the same helper; a
    sync generator with a `yield`-containing assignment-form head
    (`for ([a = yield] of x)`) is a plausible second live reacher of the
    exact same gap and should be checked, even though it wasn't part of the
    original instrumentation sweep (which only covered `for-await-of`). Both
    directories must stay at whatever their current baseline is (no
    regression), and either may pick up new passes the fix incidentally
    produces.
- **New `test262-extra` test** (§2c, §4): the witness-chain repro is not
  test262-coverable (test262 doesn't have a "generic microtask witness
  chain" convention beyond what individual tests hand-roll for their own
  narrow assertions) — follow the project's existing pattern of a dedicated
  `test262-extra` file for exactly this kind of ordering claim.
- `cargo test --release` for the Rust-level regression gate on both slices.
- Full `uv run python scripts/run-test262.py` before declaring either slice
  done — per CLAUDE.md, this is required after any implementation work
  regardless of how targeted the change looks.

## 7. Regression risk

- **Hot path**: `transform_for_in_of_loop` runs once per `for`/`for-in`/
  `for-of` loop at transform time (parse-adjacent, not per-iteration), so
  the new `ForInOfLeft::Pattern` branch adds no runtime cost at all — the
  generated state machine for the common case (no head-pattern `yield`) is
  byte-for-byte unaffected, since the new branch is only taken when
  `pattern_contains_yield` is true.
- **Shared machinery leaned on**: the new desugar sits right next to the
  `ForInOfLeft::Variable` one #727/#753 already ship, and both feed the same
  `ForOfInit`/`ForOfHead` terminator construction a few lines below — a
  mistake here risks regressing *that* existing, already-fixed path too
  (declaration-form head-pattern yields), not just the new assignment-form
  case, so the full test262 suite (not just `for-await-of/`) is the real
  gate, and the 13 files in §6 are the first thing to check on any failure
  since they exercise the sibling `Variable` desugar's own machinery.
- **`test262-pass.txt` baseline**: neither slice may move it (per project
  convention, rewriting the baseline is a `main`-branch operation). Slice 2
  must show *zero* diff in the pass/fail set; slice 3 must show the 13 files
  in §6 staying green, plus whatever *new* passes the fix produces being left
  for `main` to pick up later rather than force-added to the baseline here.
- **Branch-sync risk (§0)**: the biggest procedural risk in this cycle is not
  the code change but mis-syncing the branch — see §0 for what to do if the
  same permission block recurs.
- **Iterator-protocol double-invocation**: §2c's confirmed repro shows the
  loop's own iterator being stepped again mid-replay today; the desugar in
  §4 removes the root cause (the loop head is no longer where the `yield`
  lives, so the loop itself no longer needs to stay on the replayed
  tree-walker path at all — same reasoning #753's counting-iterator test
  already validates for the declaration-form sibling). Verify this slice's
  fix against `async-gen-decl-dstr-array-elem-iter-rtrn-close-null.js` (§6),
  which counts `next`/`return` calls directly and would catch a regression.

## 8. Out of scope

- **`eval.rs:1026` (bare `Expression::Await`) and its one known live reacher**
  (array-pattern defaults) — owned by open, actively-running issue #725 and
  closed sibling #726/#709. Do not touch pattern-initializer lowering here.
- **Further `has_suspendable_await_using_block`/`contains_suspension`
  coverage audit** beyond what's needed to resolve 2a/2b/2c — not attempted;
  no new gap was found beyond the ones already tracked or fixed in this
  cycle.
- **The legacy `IteratorState::Generator`/`AsyncGenerator` driver functions**
  — already deleted by #711; nothing left to clean up there.
- **Iterator-close / `.return()` mid-loop for the desugared `for-await`
  case beyond what §6's existing test262 coverage already exercises** — if
  the desugar surfaces a *new* gap here specifically (distinct from what
  `async-gen-decl-dstr-array-elem-iter-rtrn-close-null.js` already covers),
  file it as its own follow-up rather than growing this slice; do not
  preemptively build unwind machinery this cycle doesn't need.
- **A generalized/shared declare-vs-assign hoist helper** beyond what slice
  3's own refactor step finds warranted — see §5's note not to force one.
- **Formatting/unrelated cleanup** in `eval.rs`/`generator_analysis.rs`/
  `generator_transform.rs` beyond the changes in §4 — no drive-by refactors.
