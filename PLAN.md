# Plan: issue #857 — Blocked await-using disposal still drains inline instead of suspending

## 1. Problem restated

When an `await using` disposal lives inside a container that the generator
state-machine transform classifies `AwaitUsingScan::Blocked` (e.g. a `using`
sync-for-of head wrapping the disposal, or a sibling sloppy-mode `function`
declaration next to it) and that disposal is the *only* suspension point
anywhere in the enclosing async function, the function never gets any real
state split at all: `stmt_has_suspension` (and the top-level
`create_simple_machine` eligibility check) only recognize an `Isolatable`
reach via `has_suspendable_await_using_block`, so a `Blocked` reach is
invisible to both. The whole function body is then emitted into a single
state and executed by the ordinary tree-walker, where the disposal's genuine
`Await` has no state to suspend to and instead drains the job queue inline.
The result: the async function runs to completion synchronously, and code
after its call site that should run first (per the Await contract) runs
last.

## 2. Spec basis

- **Await** (`spec/spec.html#await`, `oldids="await-fulfilled,await-rejected"`):
  every `Await` suspends the running execution context and resumes it only
  from a promise-reaction job; it never blocks or drains microtasks inline.
  This is the invariant the bug violates — the implicit `Await` performed by
  `await using` disposal (DisposeResources) is still a real `Await` and must
  go through this same suspend/resume contract.
- **AsyncFunctionStart** / **AsyncBlockStart**
  (`spec/spec.html#sec-async-functions-abstract-operations-async-function-start`,
  `#sec-asyncblockstart`): an async function's body runs synchronously in the
  caller's turn only up to its first suspension (an `Await`) or its
  completion, then returns control to the caller — "the possible sources of
  this value are `Await` or... the async function doesn't await anything".
  The observable bug (the caller's own subsequent sync code running *after*
  the callee's disposal output, instead of before it) is exactly a violation
  of this synchronous-prefix contract.
- **`await using` / DisposeResources** (TC39 proposal-explicit-resource-management):
  **not yet merged into the pinned `spec/` submodule** — grepping
  `spec/spec.html` at the pinned commit for `DisposeResources`,
  `AddDisposableResource`, or `await using` returns zero matches. Per
  AGENTS.md's authority order (spec, then test262, then node), test262 is the
  grounding source for this feature's behavior here:
  `test262/test/language/statements/await-using/` and
  `test262/test/language/statements/using/` (178 files / 342 scenarios,
  already 100% passing and unaffected by this change — §3). The fix changes
  no disposal order, dispose-method selection, or disposal-observable value —
  only *when* the function performing an already-correct disposal suspends.

This is not a syntax or semantics change to `await using` itself. It is a
scheduling-correctness fix to the async-function suspend/resume contract
(Await, AsyncBlockStart) for containers the existing lowering silently
skipped.

## 3. Root cause and validation

`scan_await_using` (`generator_analysis.rs`) already computes the right
three-way classification (`None` / `Isolatable` / `Blocked`) for every
container the transform can reach. The bug is that the one boolean
projection of it that routing actually consults —
`has_suspendable_await_using_block` (`scan_await_using(_) == Isolatable`) —
is used by `stmt_has_suspension` (`generator_transform.rs:767`) and the
top-level fast-path eligibility check (`:609-624`) to decide whether a
statement needs real state-splitting. `Blocked` is treated identically to
`None` there: a `Blocked`-only function collapses to one `emit_statement`d
state exactly like `create_simple_machine` would have produced — matching
the issue title's "both the no-suspension-points fast path and... the full
multi-state transform fail".

Ordinary explicit `await` inside the same `Blocked` containers (a
`using`-headed for-of body, a `with` body) is already lowered correctly
today — the general state-splitting machinery
(`transform_scope_block`/`EnterScope`/`ExitScope`, `#787`'s `disposes_at_head`
handling) already knows how to give a disposal genuine suspend capability
once it is actually *reached* by `transform_yielding_statement`. The fix is
in the **routing predicate**: stop filtering out `Blocked` reaches, with one
deliberate, verified exception (below).

### Probes (all cross-checked against `node` v26.9.0; each row states what was
actually measured, not inferred)

A throwaway patch (never committed; tree is clean) added
`reaches_await_using_block` and repointed the two `generator_transform.rs`
call sites (`:621`, `:767`) to it, twice: once as a blanket
`scan_await_using(stmt) != AwaitUsingScan::None`, and — after probe G below
forced a revision — once with the one exclusion described in §4. All rows
below are the **final (excluding)** version unless marked otherwise.

| Probe | Shape | Before (measured) | After (measured) | Matches node? |
|---|---|---|---|---|
| issue repro | `using` for-of wrapping `await using`, + sibling `function` | drains inline | suspends correctly | yes (after) |
| A | `using` for-of wrapping `await using`, no sibling | drains inline | suspends correctly | yes (after) |
| B (regression check) | `using` for-of with an *explicit* `await` (no disposal) | already correct | unchanged | yes (both) |
| C | bare block, `await using` + sibling `function`, no for-of | drains inline | suspends correctly | yes (after) |
| c2 | same as C, plus `typeof g` after the block | hoists `g` correctly, wrong tick | hoists `g` correctly, right tick | yes (after) |
| E | same as A, inside an async generator | drains inline | suspends correctly | yes (after) |
| I | `await using` for-of **head** wrapping nested `await using` block | **already correct** (measured on unpatched baseline) | unchanged | yes (both) |
| **G** | C-style `for (using r = …)` wrapping `await using` | `disp-async, loop-disp, after, sync-end` | **with the blanket (non-excluding) predicate**, changed to a *different* wrong order (`…, after, loop-disp`); **with the final (excluding) predicate, identical to Before** | no (both) — pre-existing #855, deliberately excluded (see §4) |
| D/D2 | `with (o) { await using a = …; }` | `disp-async, after, sync-end` | **identical** | no (both) — separate bug, see §8 |

Row I matters: it was measured wrong in an earlier draft of this plan (never
tested on the unpatched baseline). It turns out `stmt_contains_await_using_head`
— a pre-existing, separate gate in the top-level eligibility check
(`generator_transform.rs:618`) — already forces full lowering for *any*
`disposes_at_head()` for-of head regardless of `scan_await_using`, so this
shape was already correct before this issue. It is not part of what this fix
changes, and is not part of its test surface (§6).

Row G is why the predicate has an exclusion (§4): the first (blanket)
version of the fix left G *differently* wrong instead of unaffected — it
newly routed this shape into `transform_for_statement`'s known, separately
tracked lowering bug (#855), trading one scheduling bug for a disposal-order
bug. The final predicate special-cases exactly this shape to defer to #855
and leaves it provably bit-for-bit unchanged.

Regression sweep with the final (excluding) patch applied: `cargo test
--release` 844/844 lib tests pass; `uv run python scripts/run-test262.py
test262-extra/` 1014/1014 (100%, 0 regressions against the `origin/main`
baseline); same run under `JSSE_GC_STRESS=7` 1014/1014; targeted
`test262/test/language/statements/{await-using,using}` 342/342 (100%, 0
regressions). The patch was then reverted
(`git checkout -- src/interpreter/generator_analysis.rs
src/interpreter/generator_transform.rs`); the tree is clean except for this
file.

## 4. Files to touch

All changes are under `src/interpreter/`; no `scripts/`, `benchmarks/`, or
`.github/` changes, no new ADR (bug fix within the existing, already-documented
state-machine architecture, not a new design decision), no `CONTEXT.md`
update.

- `src/interpreter/generator_analysis.rs`
  - Add:
    ```rust
    pub(crate) fn reaches_await_using_block(stmt: &Statement) -> bool {
        if let Statement::For(f) = stmt
            && matches!(&f.init, Some(ForInit::Variable(decl)) if decl.kind == VarKind::Using)
        {
            // A C-style `for (using r = …)` head has no per-iteration
            // disposal support (`transform_for_statement`'s lowering
            // disposes after loop-exit code, not at the loop's own exit,
            // regardless of what's in the body — jsse#855). Routing it here
            // would trade this issue's scheduling bug for that one instead
            // of fixing either. Stay excluded until #855 lands.
            return false;
        }
        scan_await_using(stmt) != AwaitUsingScan::None
    }
    ```
    The exclusion only triggers when `stmt` *is itself* the C-style `for`
    statement — which is exactly how `stmt_has_suspension` is invoked at
    every nesting level (each container's own transform function passes its
    *own* child statement to it), so the exclusion applies correctly however
    deeply the loop is nested, without needing to thread it through
    `scan_await_using`'s own recursive combine logic. A parent container that
    contains the excluded loop as its only reach may still get routed into
    `transform_yielding_statement` unnecessarily (an extra, harmless
    `OpenBlock`/`Goto` state) — but the loop itself, re-evaluated
    independently at its own nesting level, still excludes and
    `emit_statement`s as today. Verified by probe G (§3): output is
    bit-for-bit identical before and after.
  - Remove `has_suspendable_await_using_block` (unused in production once
    both call sites are repointed; its lone remaining reference is the test
    helper, which is being repointed to `scan_await_using` directly — see
    TDD slice 1).
  - Remove `has_block_with_await_using` and its doc comment (unused once its
    two call sites are removed/subsumed below). Do **not** remove
    `block_has_await_using` (lowercase-b) — a different, still-used internal
    helper `scan_await_using` itself calls.
  - Do **not** touch `contains_annexb_function_declaration`,
    `AwaitUsingScan::{Isolatable,Blocked}`, or any `blocked_if`/
    `blocked_unless_none` call site: the three-way classification stays
    internally load-bearing for `scan_await_using`'s own combine logic (e.g.
    `scan_scoped_list`'s `reaches_via_unsafe_flatten` check genuinely needs
    to distinguish `Isolatable` from `Blocked`); only the external consumer
    that cared solely about `Isolatable` is retired. This also keeps this PR
    clear of the still-open Annex-B tracking issue, #848.
  - Update the three unit tests under `mod tests` (see TDD slice 1).

- `src/interpreter/generator_transform.rs`
  - `:620-621` (top-level `create_simple_machine` eligibility check inside
    `transform_generator_inner_opts`): replace
    `!body.iter().any(has_block_with_await_using) && !body.iter().any(has_suspendable_await_using_block)`
    with `!body.iter().any(reaches_await_using_block)`.
  - `:767` (`stmt_has_suspension`'s `is_async` branch): replace
    `contains_suspension(stmt) || has_suspendable_await_using_block(stmt)`
    with `contains_suspension(stmt) || reaches_await_using_block(stmt)`.
    This is the line that actually fixes the bug — consulted at every
    state-splitting decision point in the transform (if/while/do-while/for/
    for-of body/with/switch/try/labeled — about a dozen call sites), so
    widening it here propagates the fix through every nesting level without
    touching those call sites individually.
  - `:926-929` (`transform_statements`'s extra `else if`): remove the
    now-unreachable `(ctx.is_async && has_block_with_await_using(stmt)) ||`
    disjunct, leaving only the break/continue condition. (Provably dead:
    `has_block_with_await_using(stmt) == true` implies
    `reaches_await_using_block(stmt) == true` for every one of its four
    shapes — direct-declaring `Block`, `If`, `Labeled`, `For.disposes_at_head()`
    — none of which is the excluded C-style-`using`-`for` shape, so the
    branch above it at `:920` already catches every case this one did.)
  - `:245-250` (`StateTerminator::EnterScope` doc comment): update the
    `has_block_with_await_using` cross-reference (being removed) to instead
    say "a block/clause list for which `ctx.scopes_disposables` is true".

No other files change.

## 5. TDD slices

1. **Red: pin the 3-way classification precisely, and pin the one
   exclusion.** In `generator_analysis.rs`'s `mod tests`, change
   `scan_first_statement` to return `AwaitUsingScan` (call `scan_await_using`
   directly instead of `has_suspendable_await_using_block`), and update the
   three existing tests' assertions:
   - `suspendable_await_using_block_through_containers`: assert `==
     AwaitUsingScan::Isolatable` for each entry (unchanged shapes/behavior,
     just a precise equality instead of a boolean).
   - `no_await_using_block_is_not_suspendable`: assert `== AwaitUsingScan::None`.
   - `lowering_that_would_flatten_a_lexical_scope_is_blocked`: assert `==
     AwaitUsingScan::Blocked` (previously asserted the boolean was `false`,
     conflating `Blocked` with `None`; this precisely pins that these shapes
     stay `Blocked` — unaffected by this fix's routing change).
   This refactor of existing assertions will not compile until
   `scan_first_statement`'s signature changes — do it as its own step before
   touching production routing.

2. **Red: add the new predicate's own test, including the exclusion.** Add a
   test asserting `reaches_await_using_block` is `true` for every entry in
   the existing `isolatable` array *and* every entry in the existing
   `blocked` array **except** `"for (using r = y; ; ) { { await using a =
   null; } }"`, for which it must assert `false`; and `false` for every entry
   in the `none` array. This fails to compile until the function exists
   (slice 3) — acceptable Rust-flavored red.

3. **Green: add `reaches_await_using_block`, wire it into the two production
   call sites.** Add the function (with the exclusion) in
   `generator_analysis.rs`; update `generator_transform.rs:620-621` and
   `:767` as in §4. `has_suspendable_await_using_block` and
   `has_block_with_await_using` are now unused in production — delete both
   and their doc comments; update the `EnterScope` doc-comment
   cross-reference; remove the dead disjunct at `transform_statements`
   `:926`. `cargo build --release` and `./scripts/lint.sh` must be clean (the
   dead-code clippy gate catches anything missed).

4. **Green: end-to-end tick-alignment tests under `test262-extra/`,** one per
   distinct `Blocked` reason this issue actually fixes (not I — already
   correct; not G — deliberately excluded, see §8), each structured like the
   existing `*-dispose-tick-alignment.js` files (console-log witness chain,
   cross-checked against `node`, sloppy and strict where applicable):
   - `await-using-blocked-using-for-of-wraps-await-using-dispose-tick-alignment.js`
     — a `using` (sync) for-of head wrapping a nested `await using` block, no
     other suspension point in the function (probe A), plus a variant adding
     a sibling `function` declaration (the issue's literal repro, combining
     both reasons).
   - `await-using-blocked-function-decl-sibling-dispose-tick-alignment.js` —
     a bare block containing `await using` with a sibling sloppy-mode
     `function` declaration, no for-of (probe C), asserting both the
     disposal tick *and* that `typeof g` after the block still reads
     `"function"` (probe c2 — confirms this fix doesn't disturb the Annex-B
     hoisting value #854 already fixed).
   - `async-generator-await-using-blocked-using-for-of-wraps-await-using-dispose-tick-alignment.js`
     — the async-generator analogue of the first file (probe E), confirming
     the same routing fix applies through `transform_async_generator`.
   Each file is red on current `main` (reproduces the drain-inline symptom)
   and green after slice 3.

5. **Verify: full regression gate**, each run as its own command (never
   `&&`-chained, per AGENTS.md):
   - `./scripts/lint.sh`
   - `cargo test --release`
   - `uv run python scripts/run-test262.py` (full suite) — confirm no
     regressions against `origin/main:test262-pass.txt`; do **not** pass
     `--update-baseline`.
   - `uv run python scripts/run-test262.py test262-extra/`
   - `uv run python scripts/run-custom-tests.py`
   - `JSSE_GC_STRESS=7 uv run python scripts/run-test262.py test262-extra/`
     (plain and `--bytecode`, matching CI's own gate) — this change shifts
     more functions from a single emitted state into multiple
     `EnterScope`/`ExitScope`-bearing states, exactly what GC-stress mode
     exists to catch (new states mean new safepoints and root-stack shapes).
   - `cargo build --profile release-checked` then
     `uv run python scripts/run-test262.py --binary target/release-checked/jsse test262-extra/`
     (plain and `--bytecode`) — this is the build CI uses to assert the
     `gc_temp_roots`/`gc_bytecode_roots` LIFO-and-balance `debug_assert!`s;
     new `EnterScope`/`ExitScope` state shapes are exactly what those asserts
     police, and `JSSE_GC_STRESS=7` alone (a release build) doesn't exercise
     them.

## 6. Test surface

- **Targeted test262**: `test262/test/language/statements/await-using/`,
  `test262/test/language/statements/using/` — 178 files / 342 scenarios,
  already 100% passing; re-confirmed zero regressions during planning (§3).
- **New `test262-extra/` tests** (this fix's actual coverage — test262 itself
  has no tests combining `await using` disposal timing with a `Blocked`
  container; these are engine-internal scheduling-heuristic regressions per
  AGENTS.md's test262-extra criterion): the three files in TDD slice 4.
- **`tests/`**: none needed — purely observable ECMAScript behavior (tick
  ordering), not a host-compatibility or resource-limit concern.
- **Mutation testing / library harnesses**: not applicable — no library
  exercises `Blocked`-classified `await using` containers.

## 7. Regression risk

- **Baseline movement**: none expected — the change only reclassifies which
  statements get a real multi-state split (and deliberately leaves one
  C-style-for shape untouched, §3/§4); it does not change any already-
  `Isolatable` or already-`None` statement's routing. Confirmed by
  `test262-extra/` (1014/1014), targeted `test262` using/await-using
  (342/342), and `cargo test --release` (844/844) during planning. The full
  `test262/` suite run in TDD slice 5 is the final confirmation gate before
  opening the PR; `test262-pass.txt` is not touched (baseline rewrite is a
  `main`-branch operation, not planned here).
- **Shared machinery leaned on**: more async-function and async-generator
  bodies move from a single emitted state into genuine multi-state
  `GeneratorStateMachine`s via the *existing* `transform_scope_block`/
  `EnterScope`/`ExitScope` and `disposes_at_head` machinery — more
  `ScopeAction`/root-stack activity, more GC safepoints. `JSSE_GC_STRESS=7`
  and the `release-checked` LIFO/balance asserts (TDD slice 5) are the
  specific gates for this; both already run clean once during planning
  validation (the plain-release stress run; the `release-checked` run is
  queued for the implementation stage, not yet run during planning).
  `eval.rs`'s `EnterScope`/`ExitScope` terminator execution is itself
  unchanged by this PR — only which statements get compiled to reach it
  changes.
- **Real-work cost**: more functions now go through the full
  `transform_statements` multi-state transform instead of the cheaper
  single-state `create_simple_machine` fast path — the same trade-off the
  issue's own "What was tried" section flagged for the reverted first
  attempt, except this time paired with the actual fix at
  `stmt_has_suspension` rather than only the top-level eligibility check, so
  the extra cost now buys correct behavior. No perf regression test is
  proposed; this is a correctness fix for an already-broken, presumably rare
  shape (disposal + `Blocked` container + no other suspension point).
- **No bytecode-VM impact**: `generator_transform.rs`/`generator_analysis.rs`
  feed the tree-walker's generator/async-function machinery; the bytecode
  compiler has its own separate compile-bail path for constructs it doesn't
  handle (falls back to the tree-walker), so this change doesn't touch it.
- **Property MOP (`property.rs`)**: untouched, not exercised by this change.

## 8. Out of scope

Discovered during planning/validation and **deliberately not fixed here**,
per "many small changes beat one large change". Both items below are
implementation-stage actions, not just notes — `PLAN.md` itself is deleted
before the PR opens, so anything "recommended" here must actually be filed
before this plan's diagnosis is lost:

- **C-style `for (using r = …; ;)` wrapping `await using` (or any
  suspension)**: deliberately excluded from the new predicate (§4), proven
  by probe G (§3) to be bit-for-bit unchanged by this fix. This shape is
  already tracked as **#855** ("lowered `using`-headed C-style `for` disposes
  after loop-exit code on any exit"), confirmed during planning to reproduce
  identically with a plain explicit `await` in the loop body (no `await
  using` involved), i.e. it is already reachable via ordinary
  `contains_suspension`-triggered lowering on current `main` today,
  independent of this fix. No new issue needed — reference #855 in the PR
  body; do not add a `test262-extra` file pinning either wrong output for
  this shape.
- **`with (o) { await using a = …; }` still drains inline** (named in the
  issue body as a second example of a `Blocked` container). Diagnosis:
  `finalize_current_state`'s `with_scopes` handling
  (`generator_transform.rs:469-482`) re-wraps a state's emitted statement
  list in a literal `Statement::With(...)` AST node only when that list is
  non-empty at finalize time. The `EnterScope`/`ExitScope` boundaries
  `transform_scope_block` inserts for the disposal's own scope are
  terminator-only (empty list) at the point they're finalized, so they never
  get this re-wrap — instead the *following* state holding the actual
  `await using` declaration gets wrapped in a fresh `Statement::With(Block(...))`,
  which, tree-walked, opens a **new**, ordinary block environment for the
  declaration, distinct from the `scope_env` `EnterScope` created (and that
  `ExitScope` later calls `take_dispose_stack` on). The resource registers on
  the wrong environment; `ExitScope` finds nothing to dispose; disposal
  instead runs via the ordinary blocking tree-walker path when the wrapped
  block exits — reproducing the original symptom by a different route.
  Probes D/D2 (§3) confirm this is identical before and after this fix — not
  a regression, just unreached. Fixing it requires reworking how
  `with_scopes` interacts with scope-opening terminators (likely: track
  `with_scopes` as part of `EnterScope`/`ExitScope`'s own environment chain
  rather than a per-state AST rewrap) — a materially different, riskier
  change than this one. **Implementation-stage action**: before or when
  opening this PR, run `gh issue create` for this (title along the lines of
  "bug: `with` wrapping `await using` still drains disposal inline — distinct
  from #857's Blocked-routing fix", body = this diagnosis), `gh issue comment
  857` noting the scope decision and linking the new issue, and reference
  both in the PR body — mirroring the precedent PR #856 set for #855.
- **Other Annex-B function-hoisting gaps**: tracked as the still-open
  **#848** (direct same-statement-list `await using` + `function`
  co-declaration bypassing the Annex-B guard entirely; `disposes_at_head`
  for-loop siblings; plain-sync `try {} finally {}`). This plan's fix does
  not touch `contains_annexb_function_declaration` or any Annex-B guard —
  confirmed via probe c2 that the one Annex-B-sibling shape this issue *does*
  fix (nested-block sibling, matching the issue's own repro) already hoists
  `g` correctly both before and after this change, because #854 fixed
  hoisting at the function-entry level, independent of which containers get
  lowered. #848's remaining shapes are unaffected either way; no action
  needed here beyond not reopening that issue's scope.
- **PR #856** (open, unmerged as of this plan, not on this branch's base):
  touches the same `scan_await_using` `ForOf` arm (splitting the
  `Using`/`AwaitUsing` match to relax the sync-`using`-for-of-head case for
  issue #845). Functionally complementary, not competing — this PR's
  `reaches_await_using_block` routes correctly either way that arm ends up
  classifying that shape — but likely to merge-conflict on adjacent lines;
  whichever lands second should rebase. No `test262-extra` file name planned
  here collides with #856's stated file name
  (`await-using-for-of-head-sync-using-wraps-await-using-dispose-tick-alignment.js`)
  since this plan's file is named
  `await-using-blocked-using-for-of-wraps-await-using-dispose-tick-alignment.js`
  — double-check for a collision at implementation time regardless, since
  #856 may have changed its own file name since its description was written.
- **Performance tuning of the "more functions take the full transform" cost**
  (§7): no action proposed; revisit only if a profiling pass surfaces it as
  material.
- **Formatting/cleanup unrelated to the above**: none bundled.
