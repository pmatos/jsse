# Plan: issue #651 — JetStream `js-tokens` still fails with "this.tokenCount of 115475 is invalid"

## 1. Problem restated

`js-tokens` gets 250 × 6 = 1500 extra tokens (115475 vs the expected 113975) because
the generator-based tokenizer mis-tokenizes the JSX sample: it emits 132 tokens where Node
emits 126. The cause is **not** generator replay or JSX-mode state (the hypothesis in #53),
and #76 (var hoisting) never touched it. It is a control-flow bug in the generator/async
state-machine transform, `src/interpreter/generator_transform.rs`, `transform_switch_statement`:

- A `switch` that contains a suspension (`yield`/`await`) is split into one state per case
  clause, chained by `Goto(next_case_state)`.
- A case body **without** a suspension is emitted verbatim (`ctx.emit_statement(stmt.clone())`,
  around lines 2237-2247). Its `break;` / `continue;` then runs natively inside the state body
  and yields a raw `Completion::Break`. The state driver
  (`src/interpreter/eval/generator_runtime.rs`) has no jump target for it, ignores it, and
  proceeds to the state's `Goto(next_case_state)`. **The case falls through into the next
  clause.**
- Cases that do contain a suspension are lowered via `transform_statements`, which turns
  `break` into `Goto(after_switch)`. That is why the bug is masked in most generator tests.

In js-tokens the punctuator `switch` has `case "(":` / `case ")":` / `case "{":` (yield-free,
each ending in `break`) ahead of `case "}"` and `case "<"` (which yield). So `(` falls through
into `case "}"` and runs `postfixIncDec = braces.pop(); nextLastSignificantToken = "}" | "?ExpressionBraceEnd"`.
`lastSignificantToken` is corrupted, the `<` after `return (` is no longer recognised as JSX
entry (`TokensPrecedingExpression.test(lastSignificantToken)` is false), and the rest of the
sample tokenizes in JS mode.

Evidence gathered while planning (release build of `181dbe9`, JetStream `c603c04` at `/tmp/JetStream`):

- `jsse` on the JSX sample: 132 tokens; Node: 126. First divergence at token 18 (the `<` of the
  first `<div className="Comment">`, after `return (`). Reproduces in 0.4 s (no JetStream driver needed).
- 25 × 3299 + 250 × 126 = 113975 (the expected value); jsse has 250 × 132 for the JSX half.
- Trace instrumentation showed `P "(" nextLastSignificantToken="?ExpressionBraceEnd"` — the `}` case tail
  running for `(`.
- Minimal repro (identical output shape on `--bytecode`, because the transform is shared):

  ```js
  function* g(x) {
    var log = [];
    switch (x) {
      case 1: log.push("one"); break;
      case 2: log.push("two"); break;
      case 3: log.push("three"); yield 0; break;
      default: log.push("def");
    }
    yield log.join(",");
  }
  // g(1) → jsse [0,"one,two,three"], node ["one"]
  ```

- **Prototype validation (scratch worktree, reverted, nothing left in this tree):** changing the
  routing condition in `transform_switch_statement` to
  `suspension || stmt_has_break_or_continue(stmt)` makes the switch matrix match Node (sync
  generator, async function, async generator; `break`, block-wrapped `break`, `if (c) break;`,
  mid-list `default`, labeled `break`, `continue` inside for/while/do-while/for-of, nested switch)
  and `uv run python scripts/run-jetstream.py --test js-tokens --iterations 1 --timeout 120 ... --repeats 3`
  reports **PASS** (~4.5 s per measurement).

## 2. Spec basis

The state machine is an implementation device; the observable contract is ordinary statement
completion semantics, which the transform must preserve across suspension points.

- `sec-switch-statement` / `sec-runtime-semantics-caseblockevaluation` (14.12, Runtime Semantics:
  CaseBlockEvaluation): once a clause is selected, subsequent clauses are evaluated in order **only
  until an abrupt completion**: "If _R_ is an abrupt completion, return ? UpdateEmpty(_R_, _V_)".
  A `break` completion in a clause therefore stops fall-through.
- `sec-break-statement-runtime-semantics-evaluation` (14.9): `break;` returns a `break` completion
  with empty target; `break L;` targets label _L_.
- `sec-continue-statement-runtime-semantics-evaluation` (14.8): the analogous `continue` completions.
- `sec-runtime-semantics-labelledevaluation` (14.13, `BreakableStatement : SwitchStatement` and
  `LabelledStatement`): a break completion with empty target ends the `switch` normally; a break
  whose target is the statement's label is converted to normal completion; anything else propagates.
- `sec-generatorresume`, `sec-yield` and the async-function/async-generator
  counterparts (`sec-asyncgeneratorstart` family): resumption continues the body from the suspension point with the same completion
  semantics; splitting a `switch` into states must not change which statements run.

Grep the ids in `spec/spec.html` (`spec/` is a submodule: `git submodule update --init --depth 1 spec`).

## 3. Files to touch

- `src/interpreter/generator_transform.rs` — `transform_switch_statement` only (the per-case
  routing at ~lines 2237-2247). Optionally a one-line comment there explaining why a yield-free case
  must still be lowered when it contains a jump (a raw break/continue completion is dropped by the
  driver). No other production file.
- New tests under `test262-extra/` (see §4/§5).
- No changes to `spec/`, `test262/`, `test262-pass.txt`, `CHANGELOG.md` (release-managed),
  `docs/perf/*` (historical reports), or `scripts/run-jetstream.py`.
- No `docs/adr/` entry and no `CONTEXT.md` change: this is a bug fix inside an existing mechanism.

## 4. TDD slices

Environment: `test262/` is an empty submodule in a fresh workspace — run
`git submodule update --init --depth 1 test262` first. `target/release/jsse` here was rebuilt from
the unmodified tree at the end of planning and reproduces the bug (132 JSX tokens; the drv check
is `node`-vs-`jsse` on the JSX sample). If in any doubt (planning-stage prototypes were built into this
`target/`), `cargo clean -p jsse --release` and rebuild before the first red run: a stale prototype binary
makes slice 1 look green before any production code exists.

Write each test first, run it against the current release binary to see it **red**, then apply the
single production change (slice 1) and confirm green. Run tests with
`uv run python scripts/run-test262.py test262-extra/<file>` (there is no dedicated runner; the files
need the test262 harness). Build with `CARGO_PROFILE_RELEASE_DEBUG=0 cargo build --release -j8`.

1. **Sync generator: yield-free `break` case does not fall through** (the core slice).
   - Test: `test262-extra/generator-switch-yield-free-case-break-does-not-fall-through.js`
     (`includes: [compareArray.js]`, `features: [generators]`,
     `esid: sec-runtime-semantics-caseblockevaluation`).
   - Behavior: `switch` where an early case body has no `yield` and ends in `break` (also: block-wrapped
     `{ ...; break; }`, `if (c) break;` followed by more statements, a `default:` in the middle ending in `break`,
     the last case) and a later case yields. Assert the exact yielded sequence per discriminant.
     Include a **positive fall-through guard** (`case 1: push(1); case 2: push(2); break; case 3: yield 0;`
     must still run both 1 and 2) so the fix cannot over-correct.
   - Production: in `transform_switch_statement`, route a case body through
     `transform_statements(&case.consequent, ctx, next_state)` when
     `stmt_has_suspension(s, ..) || stmt_has_break_or_continue(s)` for any statement `s` of the body;
     keep the verbatim-emit branch for bodies with neither.
     Do **not** make it unconditional: `transform_statements` special-cases `return` in async
     generators (Return terminators / tick alignment) and `await using` blocks, which would change
     behavior of yield-free, jump-free case bodies that work today.
2. **`continue` and labeled `break` from a yield-free case**.
   - Test: `test262-extra/generator-switch-yield-free-case-continue-and-labeled-break.js`
     (`esid: sec-continue-statement-runtime-semantics-evaluation`, plus
     `sec-runtime-semantics-labelledevaluation`).
   - Behavior: `continue` (and `continue outer`) from a yield-free case inside `for`, `while`,
     `do-while`, `for-of`; `break outer` (loop label and labeled block); an inner `switch` whose
     `break` must not consume the outer switch's clause; `return`/`throw` from a yield-free case still work.
     Do **not** put `for-in` here: `for (var k in o) { yield k }` in a generator is broken independently at
     this commit (see §7). Do **not** put a `try`/`finally` (or `with`) wrapping the `break`/`continue` in a
     case body here either: `case 1: try { break; } finally { ... }` is still wrong after slice 1 — that is the
     separate §7 item 1; adding it would leave a red test with no in-scope fix.
   - Production: none beyond slice 1 (expected green immediately after slice 1; if any variant stays
     red, that is a separate `Continue`/`LoopControl` target bug — report it, don't widen this PR).
3. **Async function and async generator** (`await` / `yield` in a later case).
   - Test: `test262-extra/async-switch-yield-free-case-break-does-not-fall-through.js`
     (`flags: [async]`, `includes: [asyncHelpers.js, compareArray.js]`, `features: [async-functions, async-iteration]`).
   - Behavior: `async function` with `await` in a later case; `async function*` with `yield` in a later
     case, consumed with `for await`; yield-free early cases end in `break`. The `ctx.is_async`/`detect_for_await`
     lowering (`StateTerminator::LoopControl`) is the path under test.
   - Production: none beyond slice 1.
4. **JetStream-shaped regression**.
   - Test: `test262-extra/generator-switch-punctuator-dispatch-yield-free-cases-before-yielding-case.js`.
   - Behavior: a reduced copy of the `js-tokens` punctuator `switch` (yield-free `case "(":` / `")"` / `"{"` with
     `break`, a `case "}"` containing a nested `switch` and yields, `case "<"` with `yield ...; continue;`,
     `default:`) driven over `return (<a/>)` and `x = <a/>` with `jsx: true`; assert the token types/values equal the
     hand-derived (spec-level) expected list, with `(` **followed by a `JSXPunctuator`** for `<`. This is the
     guard for the reported symptom without depending on the external JetStream checkout.
   - Production: none beyond slice 1.
5. **End-to-end verification and cleanup** (no new production code).
   - Run the JetStream gate (below), the full test262 suite, `cargo test --release`, `./scripts/lint.sh`.
   - Optional simplification of slice 1's predicate into a tiny local helper only if it reads better
     (no refactor beyond that).

JetStream gate (needs the pinned checkout from the issue at `/tmp/JetStream`; node >= 22):

```
uv run python scripts/run-jetstream.py --test js-tokens --iterations 1 --timeout 120 \
  --engine target/release/jsse --jetstream /tmp/JetStream --repeats 3
```

Expected: `PASS  js-tokens`. Also run `--test js-tokens,lazy-collections,sync-file-system,async-file-system`
(the four `generators/` workloads) to catch generator regressions. Add `--no-idle-gate` if the host is busy;
this gate checks correctness only, not timing.

## 5. Test surface

Targeted test262 directories (run each with `uv run python scripts/run-test262.py <dir>`):

- `test262/test/language/statements/switch/`
- `test262/test/language/statements/generators/`, `language/expressions/generators/`
- `test262/test/language/statements/async-generator/`, `language/expressions/async-generator/`
- `test262/test/language/statements/async-function/`, `language/expressions/async-function/`
- `test262/test/language/statements/break/`, `continue/`, `labeled/`
- `test262/test/language/statements/for-await-of/`, `for-of/`
- `test262/test/language/expressions/yield/`, `await/`
- `test262/test/built-ins/GeneratorPrototype/`, `built-ins/AsyncGeneratorPrototype/`
- `test262-extra/` (whole directory: pre-flight gate before the full run)

Not covered by test262 (hence the new `test262-extra/` files above): a yield-free case ending in `break`/`continue`
inside a `switch` that has a suspension in another case, in sync generators, async functions and async generators.
The bug is present while the test262 baseline is green, so no existing test262 file exercises this shape.
Then the full run: `uv run python scripts/run-test262.py` (baseline is read from `origin/main`; never
`--update-baseline`), and `cargo test --release`, `./scripts/lint.sh`.

## 6. Regression risk

- **Measured during planning (prototype of exactly the slice-1 change, scratch worktree, discarded):** identical
  pass counts to the unmodified binary, 100% on both, for `language/statements/{switch,generators,async-generator,
  async-function,break,continue,labeled}`, `language/expressions/{generators,async-generator,yield}`,
  `language/statements/for-await-of` (2431 scenarios), `built-ins/{GeneratorPrototype,AsyncGeneratorPrototype}`
  and `test262-extra/` (381 scenarios). The full-suite run remains the implementation stage's gate.
- **Baseline (`test262-pass.txt`)**: the change only alters control flow for generator/async bodies whose `switch`
  contains a suspension *and* a yield-free case containing `break`/`continue`. It moves behavior from
  "falls through" to spec. Expect no regressions; any test that passed *because of* fall-through would be
  a real spec violation and must be investigated, not special-cased.
- **Shared machinery**: `transform_statements` / `transform_yielding_statement` `Break`/`Continue` lowering
  (already used by yielding cases), `StateTerminator::LoopControl` in async for-await contexts, the
  `SwitchDispatch` driver in `eval/generator_runtime.rs`. Each lowered `break` leaves one empty dead state
  (`ctx.new_state()`), harmless but adds states; perf effect negligible.
- **Untouched**: tree-walker hot paths (`eval_expr`/`exec_statement`), property MOP, GC rooting / `gc_safepoint()`,
  exhaustive `ObjectKind` matches, bytecode fast path (generators are not compiled; the issue notes identical
  results with `--bytecode`), Node-compat library harnesses (none is expected to move; run one that uses
  generators/async, e.g. `./scripts/run-library-tests.sh zod`, only if time allows).
- **Merge conflicts**: open issue #663 targets `if`/`while`/`for` lowering in the same file. Keep the diff to the
  one routing condition so it rebases trivially.

## 7. Out of scope (file follow-up issues, do not bundle)

Observed while planning, all reproducible with the differential scripts (jsse vs node) and each a **separate**
defect from #651:

1. **Yield-free `try` / `with` / nested constructs containing a `break`/`continue` that targets a lowered target.**
   `stmt_has_break_or_continue` (generator_transform.rs ~line 528) does not recurse into `Try`/`With` (or into nested
   loops/switches for labeled/outer jumps), so e.g. `case 1: try { break; } finally { ... }` and
   `for (...) { try { if (c) break; } finally {} yield i; }` still drop the jump. A prototype that merely recursed into
   `Try`/`With` was **still wrong** (finalizer skipped, `with` scope), so this needs its own design:
   the `try`-depth-aware `LoopControl` targets and `with_scopes`.
2. **`for (var k in o) { yield k; }` in a generator yields nothing** at `181dbe9` (`{done:true}` on first `next()`;
   Node yields `"a"`, `"b"`). Independent of `switch`; likely a recent regression in the for-in lowering — check
   `git log`/bisect and whether it is already tracked before filing.
3. **`yield` inside a `case` test expression** (`case (yield 5, 2):`) throws `TypeError: Iterator next failed`;
   `SwitchDispatch` evaluates case tests with a plain `eval_expr`.
4. Audit for other `emit_statement(stmt.clone())` sites that emit a body verbatim in a lowered context without a
   break/continue check (e.g. `transform_labeled_statement`).
5. Nothing else: no refactors of the state-machine transform, no formatting changes, no unrelated cleanups, and
   no change to `scripts/run-jetstream.py`.

PR hygiene: title as Conventional Commits, e.g. `fix(generators): lower yield-free break/continue in switch cases of state-machine bodies`.
The PR body should state that #53's "generator replay / JSX mode" diagnosis was wrong and #76 was unrelated, link the
follow-up issues created for §7 (via `gh issue create`, without handoff labels), and record the JetStream `js-tokens`
PASS. Delete `PLAN.md` (`git rm PLAN.md`) before opening the PR.
