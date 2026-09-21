# Plan: issue #713 — `await`/`yield` inside operand of `delete` / `++` / assignment / `??` yields wrong values or hangs

## 1. Problem restated

The async/generator lowering pass (`src/interpreter/generator_transform.rs`, `transform_yielding_expression`) rewrites an expression that contains `await`/`yield` into a sequence of states, hoisting each suspending sub-expression into a temp var. Four arms of that function hoist or emit the wrong thing, so the resulting state body either computes a wrong value or leaves an un-lowered `Expression::Await` in an emitted statement (which the tree-walker then evaluates with the blocking `await_value` path — the hang):

| # | Arm | Root cause (verified in source and by repro) |
|---|-----|----------------------------------------------|
| 1 | `Expression::Delete` | Hoists the **whole operand** into a temp (`$del = o[k]`) and emits `delete $del`. `delete` takes a *Reference*, not a value; deleting a var binding returns `false`, and the property is never deleted. Also, a non-Reference operand (`delete (await x)`) must return `true`. |
| 2 | `Expression::Update` | Same shape bug: `o[await k]++` becomes `$upd = o[k]; $upd++`; the temp is incremented, the property is not. |
| 3 | `Expression::Assign` | `if right suspends {…} else if left suspends {…}` — the both-sides case takes the first branch and emits `Assign(op, left.clone(), $tmp)` with `left` still containing an un-lowered `await` → blocking tree-walker await → **hang**. `extract_lhs_suspensions` also only understands `Member`. |
| 4 | `Expression::Logical` | Two stacked bugs. (a) `??` uses `StrictNotEq(left, null)` to select the "evaluate right" branch — inverted *and* strict: `null ?? await 3` skips the RHS, `1 ?? await 3` evaluates it. Same inverted test in the left-suspends branch. (b) In the RHS-only branch `emit_expression_with_binding(left, …)` runs **after** `finalize_current_state`, so the statement lands in `current_statements` and is attached to the *next* finalized state (`eval_right_state`); the short-circuit path never binds the result (`1 \|\| await 3` → `undefined`, `f() && await x` → `undefined`). The left-suspends branch never binds the short-circuit result either. |

Sync generators share the same arms (`expr_has_suspension` picks `yield` vs `await`), so `delete o[yield]`, `o[yield]++`, `o[yield] = yield`, `null ?? (yield)` are wrong too (repro s1–s5 below).

## 2. Spec basis

Clause ids are from `spec/spec.html` (tc39/ecma262 submodule; cited by id + title because the HTML carries no section numbers).

- `sec-delete-operator-runtime-semantics-evaluation` — *`delete` UnaryExpression*: "1. Let _ref_ be ? Evaluation of |UnaryExpression|. 2. **If _ref_ is not a Reference Record, return *true*.** … 4.a IsSuperReference → ReferenceError; ToObject(base); ToPropertyKey(ReferencedName); `[[Delete]]`; strict + false → TypeError." **This is the load-bearing line: `delete` operates on the Reference, so hoisting the operand's *value* into a temp is a spec violation, not a style choice; and a non-Reference operand yields `true`.**
- `sec-postfix-increment-operator-runtime-semantics-evaluation`, `sec-postfix-decrement-…`, `sec-prefix-increment-…`, `sec-prefix-decrement-…` — "Let _lhs_ be ? Evaluation of |LeftHandSideExpression|. Let _oldValue_ be ? ToNumeric(? GetValue(_lhs_)). … Perform ? PutValue(_lhs_, _newValue_)": read and write go through the *same Reference*.
- `sec-property-accessors-runtime-semantics-evaluation` + `sec-evaluate-property-access-with-expression-key` — base expression is evaluated and `GetValue`d **before** the key expression; the key *value* is captured raw (ToPropertyKey is deferred: "in the case of `a[b] = c`, it will not be performed until after evaluation of `c`"). So the lowering must capture base value and raw key value in temps, in that order, and rebuild `base_tmp[key_tmp]`.
- `sec-assignment-operators-runtime-semantics-evaluation` — `LeftHandSideExpression = AssignmentExpression`: "Let _lRef_ be ? Evaluation of |LeftHandSideExpression|" precedes evaluation of the RHS and `PutValue(lRef, rVal)`.
- `sec-binary-logical-operators-runtime-semantics-evaluation` — `&&`/`||`: return _lVal_ itself when short-circuiting; `??`: "If _lVal_ is either *undefined* or *null*, then evaluate the right operand; else return _lVal_" — RHS is evaluated **iff** _lVal_ is nullish.
- `sec-optional-chaining-evaluation` — `a?.b` with nullish base returns `undefined` (a non-Reference), so `delete a?.b` returns `true`; otherwise the chain yields a Reference and `delete` proceeds normally.
- `await` (Await ( value )) and `sec-yield` — the suspension itself; resumption result becomes the value of the operand expression.

## 3. Files to touch

- `src/interpreter/generator_transform.rs` — all production changes:
  - new helper `lower_reference_operand(expr, ctx) -> Expression` (generalises/replaces `extract_lhs_suspensions`): for `Member(obj, prop)`, if `obj` or a computed `prop` suspends → evaluate `obj` into a temp (**always**, even if `obj` itself is non-suspending, whenever the key suspends — base before key; skip only when `obj` is `Expression::Super`), then the computed key into a temp (raw value, no ToPropertyKey), return the suspension-free `Member`. Nested suspending `obj` recurses through `transform_yielding_expression` (GetValue of the inner member).
  - new helper for the "is nullish" test: `tmp === null || tmp === void 0` (strict, `void 0` so a shadowed `undefined` cannot interfere).
  - rewrite the `Delete`, `Update`, `Assign` (both-sides), `Logical` arms.
  - `#[cfg(test)] mod tests` (line ~3078): optional structural unit tests, see slices.
- `test262-extra/` — new JS regression tests (list in §5).
- No `docs/adr/` or `CONTEXT.md` change: no new architectural decision or vocabulary.
- `spec/`, `test262/`, `test262-pass.txt` are **not** touched.

## 4. TDD slices

Each slice = red test(s) first (each test file covers async function **and** sync generator, or a paired file), then the arm fix, then run the targeted test262 dirs from §5. Slices 1↔2 and 3↔4 are independent; land as separate commits in this order. Repro matrix below is the red-test specification (jsse-before vs Node/spec-expected; jsse output captured with a pre-fix release binary).

### Repro matrix (async function unless noted)

| id | snippet | jsse today | expected |
|----|---------|-----------|----------|
| a | `var o={a:1}; var r=delete o[await 'a']` | `r=false`, `'a' in o` true | `true`, false |
| g13 | `delete (await o).a` | false / still present | true / deleted |
| g14 | `delete (await 5)` | false | true |
| g15 | `delete o?.[await 'a']` | false / present | true / deleted |
| g16 | `delete o.a[await 'x']` (`o.a===1`) | false | true |
| g22 | `delete g()[await 'a']` | false | true, `g` called once, before the await |
| b | `o[await 'k']++` (`o.k=1`) | `o.k===1` | `2` |
| g1 | `++o[await 'k']` | result 2, `o.k===1` | 2, 2 |
| g2 | `o[await 'k']--` | result 1, `o.k===1` | 1, 0 |
| g3 | `o[await 'a'].k++` | unchanged | incremented |
| g17 | `(await o).k++` | unchanged | incremented |
| c | `o[await 'k'] = await 5` | **hangs** | `o.k===5` |
| g21 | `g()[await 'k'] = await 5` | **hangs** | `g` called before the await, `o.k===5` |
| g5 | `o[await 'k'] += await 5` | **hangs** | `o.k===6` (`o.k` starts 1) |
| e | `null ?? await 3` | `undefined` | 3 |
| e2 | `1 ?? await 3` | 3 | 1 |
| e4 | `undefined ?? await 3` | 3 (ok) | 3 |
| e5 | `(await null) ?? await 3` | `undefined` | 3 |
| e6 | `(await 1) ?? await 3` | 3 | 1 |
| l1/l2 | `(await 0) && await 3` / `(await 1) \|\| await 3` | `undefined` | 0 / 1 |
| l3 | `1 \|\| await 3` | `undefined` | 1 |
| e3 | `f() && await 3` (`f` returns 0, counts calls) | `undefined` | 0, `f` called once |
| l6 | `((await null) ?? 5) ?? await 3` | 3 | 5 |
| s1–s5 | sync generator: `delete o[yield]`, `o[yield]++`, `o[yield] = yield`, `null ?? (yield 3)`, `1 ?? (yield 3)` | all wrong | same values as the async column |

Each expected column above is spec-derived (clauses in §2); Node agrees and was used only as a cross-check.

### Slice 1 — `delete` keeps its Reference
- **Red**: `test262-extra/async-function-delete-operand-await-is-a-reference.js` and `generator-delete-operand-yield-is-a-reference.js` (matrix a, g13–g16, g22; also: non-configurable property in strict function → TypeError thrown *after* the await; `delete (await 5)` → `true`).
- **Green**: rewrite the `Expression::Delete(inner)` arm: `Member` → `lower_reference_operand`, emit `delete <rebuilt member>` with the incoming binding; `OptionalChain(base, chain)` → hoist base into a temp, `ConditionalGoto` on the nullish test, skip-state binds `true`, eval-state recurses into the `Delete` arm on `oc_chain_to_regular_expr(chain, tmp)`, join at an `after` state; any other operand (non-Reference) → lower it for effect (`Discard`-style binding), then bind literal `true`.
- **Refactor**: none beyond the helper.

### Slice 2 — `++`/`--` keep their Reference
- **Red**: `async-function-update-expression-await-in-member-operand.js`, `generator-update-expression-yield-in-member-operand.js` (b, g1, g2, g3, g17; BigInt operand `o[await k]++` with `o.k = 1n`; string coercion `o.k = "5"` → returns `5`, stores `6`).
- **Green**: `Expression::Update(op, prefix, inner)` arm → `lower_reference_operand(inner)`, emit `Update(op, prefix, rebuilt)` with the binding. Identifier operands never reach it (no suspension inside).
- **Refactor**: reuse the helper from slice 1.

### Slice 3 — assignment with suspension in the target (and both sides)
- **Red**: `async-function-assignment-await-in-target-and-value.js`, `generator-assignment-yield-in-target-and-value.js` (c, g21, g5; keep existing single-side cases `o[await k] = 5`, `o.k = await 5` as guards; `o[await a][await b] = await c`). Timeout in the runner is the failure signal for the current hang.
- **Green**: `Expression::Assign` arm: if `left` suspends, replace `left` via `lower_reference_operand` **first** (obj, key, in source order), then handle `right` (existing hoist to `$assign` temp) and emit `Assign(op, rebuilt_left, rhs)`. Restructure to remove the unreachable-both-sides `else if`. `lower_reference_operand` supersedes `extract_lhs_suspensions`; for a non-`Member` left (destructuring pattern) keep today's behavior unchanged (out of scope, see §7) — do not make it worse.
- **Note**: this also changes the existing `c[await 9] = 1` path to capture `c` before the await when `c` is non-suspending; that matches `sec-property-accessors-…` and only touches assignments already routed through the state machine.

### Slice 4 — `&&` / `||` / `??` with a suspending right operand
- **Red**: `async-function-logical-operators-await-in-right-operand.js`, `generator-logical-operators-yield-in-right-operand.js` (e, e2, e3, e4, e5, e6, l1–l3, l6; short-circuit must **not** evaluate the RHS *nor* suspend: assert the tick count / a side-effect counter for `1 ?? await f()`; result bound through `var`, `let` and `const` declarations and as a call argument, to cover `Pattern` and `Variable` bindings; `document.all`-style `$262.IsHTMLDDA` is **not** nullish for `??` — `annexB` already covers the unlowered path, add a lowered-path case).
- **Green**: replace the two suspending-RHS branches (left suspends / left does not) with one shape:
  1. `$l` ← left (`transform_yielding_expression` with a `Variable($l)` binding if it suspends, else `emit_expression_with_binding(left, Variable($l))` in the current state — **before** finalizing);
  2. `ConditionalGoto { cond(op, $l), true: eval_right, false: skip }` with `And`: `$l`, `Or`: `!$l`, `??`: nullish test (fixes the inverted/strict condition);
  3. `skip`: `$res = $l`, `Goto after`; `eval_right`: lower `right` with `Variable($res)`, `Goto after`;
  4. `after`: `emit_expression_with_binding(Identifier($res), &binding)` **once**.
  Do not double-evaluate `left` (once in the condition, once in a binding) and do not emit a `Pattern` binding (a `let` declaration) on both branch paths — bind into `$res` on both and apply the caller's binding once at the join. The left-suspends / right-plain branch (`Logical(op, $l, right)`) is already correct and stays as is.
- **Refactor**: share the nullish-test builder with the new `Delete` optional-chain path.

Optional structural unit tests (only if `mod tests` gains/has a cheap way to parse source → state machine): assert no emitted state *body* statement `stmt_has_suspension` for each of the four shapes — catches the "un-lowered await left in a body" class without needing the event loop.

## 5. Test surface

**Targeted test262 runs** (run after each slice; commands from `CLAUDE.md`, e.g. `uv run python scripts/run-test262.py test262/test/language/expressions/delete/`):
- `test262/test/language/expressions/{delete,coalesce,logical-or,logical-and,logical-assignment,assignment,compound-assignment,postfix-increment,prefix-increment,postfix-decrement,prefix-decrement,await,yield,async-function,async-generator,generators,optional-chaining}/`
- `test262/test/language/statements/{async-function,async-generator,generators,for-await-of,class}/` (class computed keys use the hoisting helpers next to the changed arms)
- `test262/test/annexB/language/expressions/{coalesce,logical-and,logical-or,logical-assignment,yield}/`, `test262/test/annexB/language/expressions/assignmenttargettype/` (IsHTMLDDA + AssignmentTargetType)
- The `language/expressions/assignment/dstr/*yield*` and `array-elem-target-yield-expr.js` tests exercise the `Assign` arm through generators and must stay green (slice 3 leaves non-`Member` targets alone).
- Then the **full** suite (`uv run python scripts/run-test262.py`): every async function and generator body passes through this pass, so the blast radius is the whole language tree.

Test262 has almost no coverage of `await`/`yield` inside these operands (the four `delete`/`update`/`assign`/logical directories only carry parse-time `yield` target tests), so the fixes add no expected baseline passes; the new files below carry the coverage.

**New `test262-extra/` files** (test262 frontmatter, `flags: [async]` where they await, `esid` naming the clause; run with `uv run python scripts/run-test262.py test262-extra/<file>.js`, or the directory — there is no dedicated runner):
1. `async-function-delete-operand-await-is-a-reference.js` / `generator-delete-operand-yield-is-a-reference.js` — esid `sec-delete-operator-runtime-semantics-evaluation`.
2. `async-function-update-expression-await-in-member-operand.js` / `generator-update-expression-yield-in-member-operand.js` — esid `sec-postfix-increment-operator-runtime-semantics-evaluation` (+ prefix/decrement in the same file, cited in `info`).
3. `async-function-assignment-await-in-target-and-value.js` / `generator-assignment-yield-in-target-and-value.js` — esid `sec-assignment-operators-runtime-semantics-evaluation`, `info` quoting `sec-evaluate-property-access-with-expression-key` (base then key then RHS).
4. `async-function-logical-operators-await-in-right-operand.js` / `generator-logical-operators-yield-in-right-operand.js` — esid `sec-binary-logical-operators-runtime-semantics-evaluation`.

Naming follows the existing `async-function-*` / `generator-*` files in `test262-extra/`. Also run `uv run python scripts/run-test262.py test262-extra/` in full (the neighbouring `generator-*` / `async-generator-*` files exercise the same pass) and `cargo test --release` (unit tests in `generator_transform.rs`, `tests/`).

## 6. Regression risk

- **Baseline (`test262-pass.txt`)**: this pass compiles *every* async function/generator body; a bad state graph shows up as timeouts or wrong resumption anywhere in `language/`, `built-ins/Async*`, `built-ins/Promise`, `intl402` async helpers. Compare against `origin/main:test262-pass.txt` via the runner (do not `--update-baseline`).
- **Slice 3** changes an existing passing path: `c[await 9] = 1` now hoists a non-suspending `c` to a temp before the await. Semantically correct, but watch `language/expressions/assignment/**` and class/`super` tests — `super[await k]` must keep `super` un-hoisted (`Expression::Super` guard).
- **Slice 4** replaces a condition and adds states; watch `logical-assignment`, `coalesce`, optional-chaining tests, and the `Pattern`-binding path (`let`/`const` declarations bound through the state machine) for redeclaration errors.
- **Not touched**: `eval_expr`/`exec_statement` hot paths, `property.rs` MOP, GC rooting (new temps are plain function-scope `var`s like the existing `$del_N`/`$upd_N`/`$mem_obj_N`), `ObjectKind` matches, the bytecode compiler (it bails on `Expression::Await`, so lowered bodies never take that path), `generator_runtime.rs`. Temps holding base/key values stay reachable via the environment across suspension (same as existing lowering).
- **Library harnesses**: async-heavy libraries are the realistic canary — run `./scripts/run-library-tests.sh zod` (normal + jitless) as a sanity check if time permits; not a required gate.

## 7. Out of scope (follow-up issues to file, not bundled)

- **Logical assignment short-circuit** (`o.k ||= (n++, await 5)` runs the RHS and awaits even when `o.k` is truthy; `&&=`, `??=` likewise) — needs a `ConditionalGoto` in the `Assign` arm.
- **Compound-assignment read order** (`x += await p` re-reads `x` *after* the await; spec reads `lVal` first) — needs an `$old` hoist and applying the operator (`AssignOp`→`BinaryOp`) by hand.
- **Destructuring-assignment targets containing `await`** (`[o[await k]] = [7]`, `({p: o[await k]} = …)` hang because `extract_lhs_suspensions` only handles `Member`) — overlaps the destructuring-default family (#709); needs interleaving with iteration steps.
- **LHS reference evaluated after a suspending RHS** (`o[k] = (k = 'b', await 5)` writes `o.b`; `g()[k] = await x` calls `g` after the await) — fixing means hoisting LHS ref parts to temps even when the LHS has no suspension; spec-correct but moves every lowered assignment, so it is its own slice/issue.
- **Left operand evaluated after a suspending right operand** in other arms (`Binary`, `Call` args etc., e.g. `f() + await x` calls `f` after the await).
- The optional-chain arm's `!= null` (loose) test is a latent `IsHTMLDDA` divergence (`emulates-undefined`); left as is.
- Formatting/naming cleanups in `generator_transform.rs`; deleting `extract_lhs_suspensions` beyond replacing its call site.

PR title (squash subject): `fix(generators): keep references and short-circuits intact when lowering await/yield in delete, update, assignment and logical operands`
