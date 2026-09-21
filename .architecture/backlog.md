# Deepening backlog

Persistent candidate memory for the `pm-deepen` architecture routine. Statuses:
`proposed` (eligible, not started), `in-flight` (branch+PR exist), `landed` (merged),
`dropped` (hard filter — reversible), `rejected` (human declined / recurring bail — human-only reopen).
Never delete rows; they are the memory that stops re-surfacing the same work.

## iterator-helper-argument-prologue

- **Status**: landed
- **PR**: #640 (merged 2026-09-18T06:54:53Z; reconciled in-flight→landed by the 2026-09-21 firing — 2026-09-18 firing — the 8 callable-argument sites; remainder in `iterator-helper-close-policy`)
- **Score**: 24/25 (leverage 5, locality 4, blast radius 1, heat 5)
- **Files**: ~1 estimated — `src/interpreter/builtins/iterators.rs`
- **Modules**: `src/interpreter/builtins/iterators.rs`
- **Summary**: The **14 `%IteratorPrototype%` helper methods** — `forEach` :1553, `some` :1602, `every` :1662, `find` :1721, `includes` :1775, `reduce` :1875, `join` :1950, `map` :2025, `filter` :2156, `take` :2295, `drop` :2456, `chunks` :2626, `windows` :2739, `flatMap` :2875 — each open-code the same ~16-line prologue: brand-check `this` is an Object → read and validate argument 0 → **close the underlying iterator before propagating a validation failure** → `GetIteratorDirect`. The close step is spelled **three incompatible ways across 25 sites**: `close_iterator_for_error` (:390, roots the error across the close) at 14 sites — :1554, 1603, 1663, 1722, 1876, 2036, 2167, 2315, 2324, 2336, 2475, 2484, 2496, 2886; raw `iterator_close_getter` *with* hand-written `gc_root_value`/`gc_unroot_value` at :1794, 1807, 1817 and **without** it at :2307, 2467; and `iterator_close_with_completion(…, Err(err.clone()))` at :1961, 2636, 2642, 2749, 2755, 2769. `take` (:2295–2340) and `drop` (:2456–2500) are ~45-line near-verbatim clones differing only in a method-name string and a state tuple — and **each contains both policies**: the `ToNumber` failure path closes with a bare *unrooted* `iterator_close_getter`, while the three RangeError paths in the same function close through the *rooting* `close_iterator_for_error`. Same spec step, same function, two answers. Only `includes` (:1786-1789) carries a comment explaining why the error must stay rooted across a close that runs user `return()`; the other 13 re-derive it, which is how the three spellings arose. Extract one guard that brand-checks the receiver, runs a caller-supplied validation closure, closes-and-rethrows on failure under a **single explicit rooting policy**, and returns the `(validated, iterator, next)` record. Deletion test **concentrates**: the seam hides the ordering contract between creating the error, keeping it alive across user code, and running `return()`. **Constrained by the `dropped` `proxy-blind-callable-check` entry**: 11 of these sites open-code callability as `obj.borrow().callable.is_some()` instead of calling `is_callable` (`promise.rs:2195`), a known latent spec bug. This refactor **preserves that open-coded check byte-for-byte and fixes nothing** — but moving it behind the seam concentrates the bug from 11 sites to 1, making the filed fix a one-line change once its semantics are agreed. Distinct from `iterator-close-return-dance`, which unifies the four IteratorClose *implementations* (:319, :587, :4999, :5030); this is the **caller-side** prologue and would consume whichever core that candidate lands. Pinned exactly by test262 `argument-validation-failure-closes-underlying.js`, `argument-effect-order.js`, `limit-rangeerror.js`, `limit-tonumber-throws.js`, `this-non-object.js`, `callable.js` under `take/`, `drop/`, `map/`, `filter/`, `flatMap/`.
- **First seen**: 2026-09-18
- **Delivered (2026-09-18 firing)**: `require_callable_arg(interp, iterator, args, not_a_function) -> Result<(JsValue, JsValue, JsValue), JsValue>` in `iterators.rs`, adopted at the **8 callable-argument sites** (`forEach`, `some`, `every`, `find`, `reduce`, `map`, `filter`, `flatMap`) via `propagate!`; 15 lines → 1 statement each. Winning design **C&prime;** — the plain-function form that Design C (a `macro_rules!`) surfaced in its own "strongest argument against" section; it won on adjudication criterion 4 (test surface), because a macro that `return`s out of its caller cannot be unit-tested and this run must implement test-first. Designs A (`throw_closing` + fn-pointer close policy) and B (7 policy types) lost on criteria 1 and 4 respectively. **Two corrections the design pass forced**: the brand check is **out of scope** (only 9 of 14 helpers have one; `forEach`/`some`/`every`/`find`/`reduce` do not, so an unconditional check is a behaviour change at 5 sites), and the callable-check population is **8, not 11** (:474, :755, :3245 are outside the 14). Pinned by a new 4-test `require_callable_arg_tests` module. Gate: 680 lib unit tests, lint + perf-counters clippy clean, test262 `built-ins/Iterator/` **1308/1308, 0 regressions**, 11 custom tests. Remaining scope carved out as `iterator-helper-close-policy`.
- **Picked**: 2026-09-18 firing. **Tied at 24/25 with `temporal-rounding-options-reader`** (leverage 5, locality 4, blast 1, heat 5 vs leverage 5, locality 5, blast 2, heat 5); won on the deterministic lower-blast-radius tie-break (1 file vs 6). Branch adopted (`sym/jsse/routine/refactor-audit/01M2RSMHC2`), not renamed. Outscored the prior firing's flagged next pick `gc-root-scope-guard-remainder` (22/25).

## state-machine-terminator-dispatch

- **Status**: in-flight
- **PR**: #694 (2026-09-21 firing)
- **Score**: 25/25 (leverage 5, locality 5, blast radius 1, heat 5)
- **Files**: ~4 estimated — `src/interpreter/eval/generator_runtime.rs`, `src/interpreter/eval.rs`, a seam home, `src/interpreter/tests.rs`
- **Modules**: `src/interpreter/eval/generator_runtime.rs`, `src/interpreter/eval.rs`
- **Summary**: **Three drivers interpret one `StateTerminator` enum independently** — sync generator (`generator_runtime.rs:482`, 1,989 lines), async generator (`generator_runtime.rs:3267`, 2,650 lines), async function (`eval.rs:8178 async_function_resume`, ~1,158 lines). Several terminators carry an **operand** the driver must evaluate and consume as a value (`ConditionalGoto` condition, `SwitchDispatch` discriminant + each case test, `ForOfInit` iterable, `Await`/`Return` operands — **15 sites**), and each site re-derives the completion taxonomy inline. Three spellings: sync uses the catch-all `other => return other` (`:1339`, `:1449`, `:1455`, `:1502`); async-generator uses an explicit `Completion::Exit` arm then `Yield(yv) => yv` then `_ => UNDEFINED` (`:5198`, `:5316`, `:5325`); async-function uses bare `_ => JsValue::UNDEFINED` with **no `Exit` arm at all** (`eval.rs:8845`, `:8896`, `:8945`, `:9033`, `:9043`, `:9081`). **8 defects reproduce today** against the release binary with `--node`, each verified against the sibling driver that gets the identical program right: `eval.rs:8945` if/while condition (exit 0, want 3/4), `:9033` switch discriminant (want 5), `:9043` switch case test (want 5), `:9081` for-of iterable (want 6), `:8845` await operand (want 8), `:8896` return operand (want 7), and `generator_runtime.rs:5395` async-generator for-of iterable (want 6); `:5820` (async-generator await operand) reproduces the same way. In cases 1–5 the async function also **keeps running past `__host_exit`** — `Exit` is coerced to `UNDEFINED` and read as a falsy condition — contradicting `builtins/node_host.rs:84-93`, which documents the completion as *structurally uncatchable*. The "fix applied in N places" signal is explicit: `a86201e9` (#664) rewrote `SwitchDispatch` twice in one commit (`:1442` + `:5309`) and `5e8ee75b` (#682) rewrote `ConditionalGoto` twice (`:1334` + `:5193`); **neither touched `eval.rs`**. Deletion test **concentrates**: the seam hides the completion taxonomy (spec text, not an enumeration of sites), and the win is that the abrupt arm becomes **unmissable** — no `_ =>` to fall into. Test-first footing is the best in the backlog: `tests.rs:3848 host_exit_in_generator_conditional_goto_is_not_swallowed` is already a parameterised table over `("function*", …)` and `("async function*", …)` with no `"async function"` rows, so the red test is a table addition. **Note: this is a deepening that also changes behaviour** — routing the drivers through one seam necessarily fixes the 8 defects.
- **First seen**: 2026-09-21
- **Delivered (PR #694)**: `Operand` enum + pure `classify_operand` + `Interpreter::eval_operand` in the new `src/interpreter/eval/operand.rs`; all 22 operand sites across the three drivers routed through it. Winning design **C** (pure classifier + one thin per-driver macro) over **B** (a `TerminatorOperandDriver` trait — same seam placement, but 18 method bodies for 22 sites and it still needs a macro for control flow) and **A** (`-> Result<JsValue, Completion>` — rejected because its `Err(other)` arm re-duplicates the 10–11 line generator teardown at all 14 generator sites). Adjudicated **without an advisor** (rate-limited). **10 defects fixed**, each **empirically reproduced** on the release binary before and after, and each verified against a sibling driver producing the right answer on the identical program. (The design pass's inspection-only count said 11; `generator_runtime.rs:5118`, the async-generator `throw` operand, was **measured correct at baseline** and struck — a top-level `throw` is not lowered to a `Throw` terminator, the same reason its async-function twin `eval.rs:8917` was excluded earlier. Its `Exit` arm still changed, so it is pinned as hardening rather than claimed as a fix.) **Deviation from the design's projection**: call-site lines fell far less than the projected 323→85 — `generator_runtime.rs` −32 net, `eval.rs` +14 net — because the macro's default throw tail only applies broadly in the async-function driver (uniform at 5 of 7 sites, and those did collapse to one-liners); the two generator drivers carry four distinct throw tails each, preserved verbatim with explicit `Operand::` arms to keep this an `Exit`-only behaviour change. Gate: 728 unit tests, lint + perf-counters clippy clean, **test262 99,911/99,911 with 0 regressions**, 14 custom tests. CONTEXT.md gained the **Terminator Operand** term. Three adjacent drift findings carved out as `generator-operand-throw-tail-drift`.
- **Picked**: 2026-09-21 firing. Clear top at 25/25 over a three-way 24/25 tie (`generator-completion-teardown`, `suspension-scope-expression-walker`, `temporal-rounding-options-reader`) — **within 1 point**, so the pick was close. Branch adopted (`sym/jsse/routine/refactor-audit/01M30GVV03`), not renamed.

## generator-operand-throw-tail-drift

- **Status**: proposed
- **Score**: 18/25 (leverage 3, locality 4, blast radius 1, heat 5)
- **Files**: ~1 estimated — `src/interpreter/eval/generator_runtime.rs`
- **Modules**: `src/interpreter/eval/generator_runtime.rs`
- **Summary**: Three drift findings surfaced by the PR #694 design pass that are **not** about `Completion::Exit`, and were therefore preserved byte-for-byte rather than absorbed by that PR's shared defaults. Each is a genuine behaviour change needing its own test. **(1)** `generator_runtime.rs:1196` (sync `Return` operand) omits `route_exception!` where its siblings call it, so a throw from a `return` expression inside a `try` does not reach the `catch` — the sharpest of the three, and a real spec bug. **(2)** `:885`, `:1502`, `:4315`, `:5395` omit `dispose_resources` on the operand-throw path where sibling sites call it (§27.5.3.3 / §27.6.3.3), so a `using` disposer is skipped when a for-of iterable throws but run when a while-condition does. **(3)** `:4778` omits `drain_microtasks()` after rejecting where `:5198` calls it. Leverage 3 because these are three separate fixes rather than one seam — the seam they would have shared already landed in #694; what remains is deciding the correct behaviour per family and pinning it. Related: the `drain_microtasks` inconsistency is a subset of `generator-completion-teardown`'s 43-vs-7 finding, and should land with it.
- **First seen**: 2026-09-21 (carved from `state-machine-terminator-dispatch` at design time)

## generator-completion-teardown

- **Status**: proposed
- **Score**: 24/25 (leverage 5, locality 4, blast radius 1, heat 5)
- **Files**: ~2 estimated — `src/interpreter/eval/generator_runtime.rs`, `src/interpreter/types.rs`
- **Modules**: `src/interpreter/eval/generator_runtime.rs`
- **Summary**: The "this generator is finished, tear it down" transition is hand-written at **98 sites** — 30 `IteratorState::completed_state_machine_generator(…)` assignments (all in the sync driver), 68 `…_async_generator(…)` (56 in the async driver, 12 in the `async_gen_*` helpers at `:2664, :2683, :2703, :2763, :3098, :3118, :3137, :3154, :3203, :3226, :6353, :6459`) — alongside 66 `generator_inline_iters.remove(&o.id)`, 11 `generator_for_of_stacks.remove(&o.id)`, 22 `dispose_resources(…)` and 50 `call_function(&reject_fn, …)`. **Measured drift**: `drain_microtasks()` follows the reject at **43 of 50** sites, absent at 7 (`:4659, :4714, :4802, :4916, :5095, :5151, :5799`) — and commit `5e8ee75b` added two new reject epilogues *in one diff* with different answers (`:4802` no drain, `:5218` drain). Second axis: `dispose_resources` (§27.5.3.3) runs on the `ConditionalGoto` (`:1344`) and `SwitchDispatch` (`:1471`) throw paths but **not** on the `ForOfInit` iterable-throw paths (`:1504`, `:1524`) in the same driver — so a `using` disposer is skipped when a for-of iterable throws but run when a while-condition does. Seam: `complete_generator(&mut self, gen: &GeneratorHandle, outcome: Completion) -> Completion` plus a thin `settle_completed_async(…)` wrapper owning `drain_microtasks` exactly once. Deletion test **concentrates**: what is hidden is an *ordering invariant* (dispose before latching `Completed`; clear both side tables before the object is unreachable or `gc.rs:380`/`:385` leak roots; turn an `Exit` out of dispose into a propagated exit rather than `unreachable!()`), not a call sequence. Locality is 4 not 5 because the promise-settling wrapper is the separately-tracked `settle-and-return-tail`. **Not a re-proposal of** `IteratorState::completed_state_machine_generator` (`types.rs:1501`), which is an already-extracted seam for the 10-field literal; this is the next ring outward. Test-first: `tests.rs:4141-4143` and `:4197-4199` already assert exactly the post-conditions the seam would enforce, for 2 of the paths.
- **First seen**: 2026-09-21
- **Note**: Runner-up candidate to the 2026-09-21 pick, and the natural next firing — it lands in `generator_runtime.rs`, which that pick warms.

## suspension-scope-expression-walker

- **Status**: proposed
- **Score**: 24/25 (leverage 5, locality 5, blast radius 1, heat 4)
- **Files**: ~2 estimated — `src/interpreter/generator_analysis.rs`, `src/interpreter/generator_transform.rs`
- **Modules**: `src/interpreter/generator_analysis.rs`
- **Summary**: **14 recursive walkers over the same statement grammar** — `generator_analysis.rs:115 analyze_statement`, `:431 analyze_expression`, `:666 contains_yield`, `:729 expr_contains_yield`, `:982 contains_suspension`, `:784 expr_contains_suspension`, `:845 has_block_with_await_using`, `:918 scan_await_using`, `:895 declares_lexical_binding`, `:725 class_contains_yield`, `:1043 class_contains_suspension`; `generator_transform.rs:548 stmt_contains_for_await`, `:581 stmt_contains_return`, `:630 stmt_has_break_or_continue`, `:672 collect_escaping_jumps`, `:2644 rewrite_stmt_await_to_yield`, `:2750 rewrite_expr`. **Two are verbatim twins**: after renaming the recursive callee, `contains_yield` vs `contains_suspension` differs by 11 lines, all whitespace; `expr_contains_yield` vs `expr_contains_suspension` differs by 2 semantic lines out of 30 (`Expression::Await` moves from a recursion arm to a `=> true` leaf). **Drift**: `stmt_contains_for_await` (`:548`) matches `Statement::ForOf(f) => f.is_await` and returns *without descending into `f.body`*, while the structurally identical sibling `stmt_contains_return` (`:595`) does recurse — gating `create_simple_machine` (`generator_transform.rs:486`). Seam must be **scope-aware**, not a generic child-enumerator: `any_in_suspension_scope(stmt, pred: &mut impl FnMut(&Expression) -> bool) -> bool`, carrying the four boundary rules — (1) function/arrow bodies terminate descent (restated at `:744` and `:799`), (2) class heritage and computed keys are in scope but method bodies are not, (3) for/for-in/for-of heads are in scope, (4) switch case **tests** are in scope. Deletion test **concentrates** *given that framing*: each of the four is a rule this codebase has already got wrong — rule 4 is exactly what `5ca7aa36` (#692) fixed, at 175 changed lines in `generator_transform.rs` alone. A bare `visit_children` would smear nothing and is explicitly **not** the proposal. Test-first: `generator_analysis.rs:1186-1413` is a dedicated unit block already pinning these boundary rules.
- **First seen**: 2026-09-21

## await-using-lowering-eligibility

- **Status**: proposed
- **Score**: 21/25 (leverage 4, locality 4, blast radius 2, heat 5)
- **Files**: ~4 estimated — `src/interpreter/generator_analysis.rs`, `generator_transform.rs`, `eval.rs`, `types.rs`
- **Modules**: `src/interpreter/generator_analysis.rs`, `src/interpreter/generator_transform.rs`
- **Summary**: "Can this **Isolated Block**'s disposal be lowered into its own state?" is answered by **four** functions in `generator_analysis.rs` (`:845 has_block_with_await_using` — if/labeled/block reach only; `:859 block_has_await_using` — leaf test; `:918 scan_await_using` + `:978 has_suspendable_await_using_block` — loops/try/switch reach, with an `AwaitUsingScan::{None,Isolatable,Blocked}` lattice), with five consumers in `generator_transform.rs` (`:488`, `:489`, `:624`, `:783`, `:848`) each picking a *different* one — plus a **fifth, open-coded copy in the driver** at `eval.rs:8696-8699`, which re-derives "this state's last statement is an isolated await-using block" **by raw pointer identity** (`last as *const Statement as usize`), duplicating a decision the transform already made and discarded. `AwaitUsing` spans 8 source files; `181dbe9f` (#666) touched 10, `da44e1e6` (#688) touched 4 and added the 204-line lattice. Seam: record the decision as `GeneratorState.suspendable_dispose_tail: bool` so `eval.rs:8691-8702` collapses to a field read, plus one `await_using_reach(stmt, ctx) -> AwaitUsingScan` entry point. Deletion test **concentrates, with a caveat**: the load-bearing part is the recorded flag (it deletes the pointer-identity trick); merging the four predicates is closer to *parameterisation* than deepening, since they ask genuinely different reach questions — score the flag, treat the predicate merge as optional. Fully pinned: `generator_analysis.rs:1117-1186` (3 tests, 33 table rows) plus 6 `test262-extra/await-using-*.js` files.
- **First seen**: 2026-09-21

## generator-for-of-stack-mirror

- **Status**: proposed
- **Score**: 19/25 (leverage 3, locality 4, blast radius 2, heat 5)
- **Files**: ~4 estimated — `src/interpreter/eval/generator_runtime.rs`, `eval.rs`, `gc.rs`, `mod.rs`
- **Modules**: `src/interpreter/eval/generator_runtime.rs`
- **Summary**: A suspended generator's live for-of iterator stack is kept in **three representations** callers synchronise by hand — a driver-local `Vec<ForOfLoopState>` (`generator_runtime.rs:718`, `:3982`, `eval.rs:8301`), the `generator_for_of_stacks` side table (`mod.rs:260`, 24 references across `gc.rs`/`generator_runtime.rs`/`mod.rs`/`tests.rs`), and `generator_inline_iters` (`mod.rs:259`, 66 `.remove(&o.id)` calls) — flushed manually at **20** `sync_generator_for_of_stack` call sites (`:795, 853, 1174, 1226, 1401, 1552, 1600, 1660, 1690, 4223, 4265, 4828, 5006, 5478, 5526, 5627, 5661, 6674, 6693, 6783`; definition `:6790`). `eval.rs` uses a **fourth** design, round-tripping the stack through saved async state (`:8155`, `:8205`, `:8288`) rather than the side table. Seam: a `ForOfStackGuard` owning the local `Vec` and flushing the mirror on `Drop`. Deletion test **concentrates, but thinly** — hence leverage 3: seven helpers (`:473 stash_pending_iter_close`, `:6624 align_generator_for_of_stack`, `:6664 discard_failed_generator_for_of_loop`, `:6680 discard_generator_for_of_loops_on_exit`, `:6700 route_generator_exception`, `:6765 unwind_generator_for_of_loops`, `:6790 sync_generator_for_of_stack`) already absorb the unwinding logic, so what remains is close to "call `sync_` after each mutation", which a guard *automates* rather than *explains*. Its real value is forcing `eval.rs`'s fourth design into the same shape. Test-first is **partial**: `tests.rs:4090`/`:4147` assert the GC-root and side-table invariants for two host-exit paths, but the ordinary suspend/resume flush is unpinned and needs a new test.
- **First seen**: 2026-09-21

## switch-scope-dispose-exits

- **Status**: dropped
- **Score**: not ranked — hard-filtered
- **Files**: ~1 — `src/interpreter/exec.rs`
- **Modules**: `src/interpreter/exec.rs`
- **Summary**: The 7 `self.dispose_resources(&switch_env, …)` calls at each control-flow exit of `exec_switch_statement` (`exec.rs:2488, 2494, 2497, 2517, 2520, 2526, 2531`) look like the same epilogue family as `generator-completion-teardown`.
- **Reason**: **Leverage 1 — fails the deletion test.** Each call already carries the completion it is disposing (`r`, or `Completion::Normal(v)`); there is no shared state to latch, no ordering invariant, and no side table to clear. A `dispose_and_return!` helper turns 7 lines into 7 shorter lines and a reader must still visit all 7 to know disposal happened — complexity **moves**, it does not concentrate. The genuine fix here is a scope guard on `switch_env`, which is the existing `gc-root-scope-guard-remainder` entry, not a new seam.
- **First seen**: 2026-09-21

## iterator-helper-close-policy

- **Status**: proposed
- **Score**: 20/25 (leverage 4, locality 4, blast radius 1, heat 5)
- **Files**: ~1 estimated — `src/interpreter/builtins/iterators.rs`
- **Modules**: `src/interpreter/builtins/iterators.rs`
- **Summary**: The tail of `iterator-helper-argument-prologue` after the 2026-09-18 firing took the 8 callable-argument sites. What remains is the **close-policy divergence** across the other close sites, and the 6 numeric/string-argument helpers that host them (`includes` :1782, `join` :1948, `take` :2267, `drop` :2428, `chunks` :2598, `windows` :2711). The real partition is **rooted vs unrooted**, not four spellings: `close_iterator_for_error` (:390) and the three `includes` inline triples (**:1804**, **:1817**, **:1827** — `gc_root_value` / `iterator_close_getter` / `gc_unroot_value`) root the error across the close; **8 sites do not** — the 2 bare `iterator_close_getter` calls at **:2282** (`take` ToNumber path) and **:2442** (`drop` ToNumber path), and the 6 `iterator_close_with_completion(…, Err(err.clone()))` calls at **:1962** (`join`), **:2611**/**:2617** (`chunks`), **:2724**/**:2730**/**:2744** (`windows`), where the `err.clone()` is a handle clone into a Rust local, **not** a GC root. All 8 violate the invariant the file states in exactly one place, the `includes` comment at **:1795-1799** (*"The error object is created before IteratorClose … It must be rooted across the close, which runs arbitrary JS and can trigger GC"*) — now also stated on the `require_callable_arg` seam doc at :406-414. Sharpest evidence that this is a real decision and not style: **`take` and `drop` each use both policies within one function** — unrooted on the ToNumber path, rooted on their three RangeError paths. **This is a latent GC-safety bug, so the fix is a behaviour change under GC pressure, not a pure deepening** — it must be landed test-first with a GC-stress pin per site, which is why the 2026-09-18 firing deliberately did not touch it. The deepening part (a policy-parameterised close seam, e.g. Design A's `throw_closing(interp, iterator, error, close)` from that firing's report) is behaviour-preserving and can land first, making the subsequent fix a one-token diff per site.
- **First seen**: 2026-09-18 (carved from `iterator-helper-argument-prologue` at implementation time)
- **Line numbers**: re-derived **after** PR #640's migration, against that branch's tree — valid as of commit on `sym/jsse/routine/refactor-audit/01M2RSMHC2`. Re-check if #640 is revised before merge.
- **Note**: Design A in `.architecture/reviews/2026-09-18-iterator-helper-argument-prologue.md` is a complete, borrow-checked design for exactly this scope — start there rather than redesigning. Also related: `iterator-close-return-dance` unifies the four IteratorClose *implementations*; this entry is about which of them each *caller* picks.

## temporal-rounding-options-reader

- **Status**: proposed
- **Score**: 24/25 (leverage 5, locality 5, blast radius 2, heat 5)
- **Files**: ~6 estimated — `src/interpreter/builtins/temporal/mod.rs` (seam home), `instant.rs`, `duration.rs`, `plain_time.rs`, `plain_date_time.rs`, `zoned_date_time.rs`
- **Modules**: `src/interpreter/builtins/temporal/`
- **Summary**: **12 option-reader sites** re-spell the Temporal rounding-option bag — 7 named functions (`mod.rs:4116 parse_difference_options`, `instant.rs:1166`, `duration.rs:3037 parse_round_options`, `duration.rs:3212`, `plain_time.rs:1136 parse_time_diff_options`, `plain_time.rs:1285`, `zoned_date_time.rs:4189`) plus 5 copies inlined inside `round`/`toString` native closures (`instant.rs:309-390`, `plain_time.rs:287-365`, `plain_date_time.rs:1587-1650`, `plain_date_time.rs:1814-1845`, `zoned_date_time.rs:3288-3340`). Each reads `largestUnit`/`roundingIncrement`/`roundingMode`/`smallestUnit` in alphabetical order, *then* validates — and **8 carry a verbatim 9-arm `roundingMode` match** (`plain_date_time.rs:1830`, `plain_time.rs:348`, `:1224`, `:1351`, `duration.rs:3125`, `:3272`, `instant.rs:369`, `:1238`). Already diverged: `instant.rs:376` throws `"Invalid roundingMode: {rs}"`, `mod.rs:4218` throws `"{rs} is not a valid value for roundingMode"` — same spec step, two messages. Seam: `read_rounding_options(interp, options, spec: RoundingOptionSpec) -> Result<RoundingOptions, Completion>` carrying allowed-unit subset, per-type defaults and increment policy. Deletion test **concentrates**: hides *which rounding policy a call site is on* plus the spec-mandated read-all-before-validating observation order. The defaults legitimately differ per type (`instant.rs:379` → `halfExpand`, `mod.rs:4224` → `trunc`), which is what makes this a deepening rather than a dedup. Sub-family worth folding in: `ToSecondsStringPrecisionRecord` at `instant.rs:1183`, `duration.rs:3227`, `plain_time.rs:1298`, `plain_date_time.rs:1814`, `zoned_date_time.rs:4290`. Pinned by the per-type `order-of-operations.js`, `options-read-before-algorithmic-validation.js`, `roundingincrement-*.js` and `rounding-direction.js` families.
- **First seen**: 2026-09-18
- **2026-09-21 re-check: friction INTACT, headline count exact (12), but the entry was wrong on four points and missed two findings.** `grep -n '"roundingMode"' src/interpreter/builtins/temporal/*.rs` returns **exactly 12**; every reader is one of them, none disappeared. **(a) "Each reads `largestUnit`/`roundingIncrement`/`roundingMode`/`smallestUnit`" is FALSE** — `largestUnit` is read at only **3** sites (`duration.rs:3083`, `plain_time.rs:1150`, `mod.rs:4138`) and `roundingIncrement` at only **7**; the 5 to-string readers read neither, reading `fractionalSecondDigits` + `roundingMode` + `smallestUnit` interleaved with type-specific keys. It is a **7-way rounding family and a 5-way precision family overlapping on one key**, not one 12-way clone. **(b) Three error-message spellings, not two** — `"Invalid roundingMode: {…}"` ×8 (`plain_time.rs:356`, `:1231`, `:1358`, `plain_date_time.rs:1643`, `:1837`, `instant.rs:376`, `:1245`, `zoned_date_time.rs:4258`), `"{…} is not a valid value for roundingMode"` ×2 (`mod.rs:4220`, `zoned_date_time.rs:3259`), and the **missed** `"Invalid rounding mode: {rm}"` ×2 (`duration.rs:3132`, `:3279`). No test262 test and nothing in `test262-extra/` asserts the text, so unifying all three is free. **(c) The 8 expanded 9-arm matches have 4 collapsed or-pattern siblings** (`mod.rs:4216`, `plain_date_time.rs:1638`, `zoned_date_time.rs:3255`, `:4246`) — 12 validators, 1:1 with readers. The 8 expanded anchors are all still exact (first arm is 4 lines above the entry's `"halfExpand"` anchor). **(d) Stale anchors**: the 7th named function is `zoned_date_time.rs:4135 parse_zdt_to_string_options`, not `:4189`; the zdt `round` reader starts at `:3234`, not `:3288` (that range is the validation tail); `plain_date_time.rs`'s toString reader starts at `:1753`, not `:1814`. Also the `ToSecondsStringPrecisionRecord` sub-family is a **relabelling of 5 of the 12, not 5 extra** — `plain_date_time.rs:1814` was double-counted; the total stays 12.
- **2026-09-21, two findings the entry did not carry.** **(i) A half-built seam is being bypassed**: `mod.rs:1554 validate_rounding_increment_raw(int_inc, unit, is_difference)` exists, but only 3 of the 7 increment sites call it. Three more (`mod.rs:4266-4279`, `plain_time.rs:1268-1281`, `duration.rs:3186-3199`) hold copies of its `if let Some(max)` half that are byte-identical **including both format strings**, so `validate_rounding_increment_raw(inc, unit, true)` is a provably output-identical substitution at all three — which also un-deads the `is_difference == true` branch, currently **dead** because every existing caller passes `false`. **(ii) `zoned_date_time.rs:3305-3319` is unreachable dead code** (15 lines): `validate_rounding_increment_raw(…, false)` at `:3290` already performs the identical `i >= max` / `max % i != 0` checks for non-`day` units, and for `day` the helper's divisibility branch rejects every `increment > 1` first — so both `"roundingIncrement for day must be 1"` (`:3308`) and the truncated `"{increment} does not divide evenly into {max}"` (`:3317`, missing the `roundingIncrement ` prefix) are unobservable.
- **2026-09-21, why it did not win (24/25 vs the pick's 25/25), and a WARNING for whoever implements it.** The re-check **weakened** the candidate. **Naive extraction is unsafe**: the spec-mandated alphabetical read order is *interleaved with type-specific keys* and some validation is deliberately deferred past later reads — `relativeTo` sits between `largestUnit` and `roundingIncrement` in Duration; `offset` sits between `fractionalSecondDigits` and `roundingMode` in ZDT; `timeZone`/`timeZoneName` are read *after* `smallestUnit` yet `smallestUnit` validation must wait for them (`instant.rs:1289+`, `zoned_date_time.rs:4303+`). Any fixed-shape "read the rounding bag" helper reorders observable gets and breaks the 5 `options-read-before-algorithmic-validation.js` tests. **The seam must be descriptor-driven** — the caller supplies an ordered key list and the helper walks it. Second trap: **`instant.rs:383` is legitimately different and must stay different** — `Instant.round` uses inclusive per-day maxima, so `roundingIncrement: 24` with `smallestUnit: "hour"` must be *accepted*; the shared helper's `i >= max` check would wrongly reject it, pinned by `built-ins/Temporal/Instant/prototype/round/allow-increments-that-divide-evenly-into-solar-days.js`. Replace the `is_difference` boolean with a **3-valued increment policy** (`Difference` / `RoundInclusive` / `RoundExclusiveWithDayOne`) rather than widening the boolean. Suggested sequencing: Tier 1 (unify the 12 roundingMode validators — pure duplication removal, ~110 lines) + Tier 2 (the increment policy enum + the dead-code deletion) as one PR; Tier 3 (the descriptor-driven bag reader) as a separate design task. **Test surface**: fully green today — 63 `order-of-operations.js`, 45 `options-read-before-algorithmic-validation.js`, 121 `roundingincrement-*.js`, 22 `roundingmode-invalid-string.js`, all passing — but the module has **zero Rust unit tests** (`grep -rc "cfg(test)"` is 0 for all 10 files), so the extracted helpers need new ones. Coverage hole: the richer `order-of-operations.js` exists under `round/` only for Duration and ZonedDateTime.
- **Note**: Runner-up candidate to the 2026-09-18 pick (tied at 24/25, lost the blast-radius tie-break). **2026-09-21: still a runner-up, now in a three-way 24/25 tie** with `generator-completion-teardown` and `suspension-scope-expression-walker`; lost the blast-radius tie-break again (6 files vs 2). The "natural next firing" note is superseded — `generator-completion-teardown` is the better next pick, being the same score at blast radius 1 in a file the 2026-09-21 pick warms.

## temporal-property-bag-reader

- **Status**: proposed
- **Score**: 22/25 (leverage 5, locality 5, blast radius 3, heat 4)
- **Files**: ~7 estimated — `src/interpreter/builtins/temporal/mod.rs` (seam home), `plain_date.rs`, `plain_year_month.rs`, `plain_month_day.rs`, `plain_date_time.rs`, `zoned_date_time.rs`, `duration.rs`
- **Modules**: `src/interpreter/builtins/temporal/`
- **Summary**: **6 whole-bag readers** independently re-implement spec `PrepareTemporalFields` — `plain_date.rs:1605 read_pd_property_bag_raw`, `plain_year_month.rs:66 read_pym_property_bag_raw`, `plain_month_day.rs:60 read_pmd_property_bag_raw`, `plain_date_time.rs:282 read_pdt_property_bag`, `zoned_date_time.rs:844` (inlined in `to_temporal_zoned_date_time_with_options:754`), `duration.rs:237-370` (inlined in `to_relative_to_date:47`, whose own comment reads "read ALL fields in alphabetical order per spec PrepareTemporalFields"). Each is 60–140 lines of `get_prop(item, "day"/"era"/"eraYear"/"month"/"monthCode"/"year") → if !undefined → to_integer_with_truncation | to_primitive_and_require_string → per-field range check`. Second-pass `with()`-style copies at `plain_date.rs:1872`, `plain_year_month.rs:212`, `plain_month_day.rs:211` and `:294`. `mod.rs` already carries two *partial* extractions (`read_month_code_field:1482`, `read_month_fields:3937`) that **none of the six route through** — evidence the seam is wanted but under-built. Seam: `read_temporal_fields(interp, item, spec: &[TemporalField]) -> Result<TemporalFieldBag, Completion>`, the table alphabetically ordered by construction. Deletion test **concentrates**: hides in what order a bag's fields are observed and how each is coerced. Deliberately **not** the required-combination check (PlainYearMonth has no `day`, PlainMonthDay no required `year`, `plain_date.rs:1695-1715`'s era/eraYear pairing) — that genuinely differs and stays at the call site. Pinned by `from/order-of-operations.js` and `calendarresolvefields-error-ordering.js` per type.
- **First seen**: 2026-09-18
- **Note**: Blast radius 3 partly because it touches `duration.rs` and `plain_date_time.rs`, which `.cargo/mutants.toml` excludes as combinatorially explosive — sequence it after `temporal-rounding-options-reader`, which warms the same files at lower risk.

## string-symbol-protocol-dispatch

- **Status**: proposed
- **Score**: 20/25 (leverage 4, locality 4, blast radius 1, heat 3)
- **Files**: ~2 estimated — `src/interpreter/builtins/string.rs` + a seam home (`interpreter/mod.rs` or `helpers.rs`)
- **Modules**: `src/interpreter/builtins/string.rs`
- **Summary**: There is **no `GetMethod` abstract-operation seam anywhere in the tree** — `fn get_method` has 0 hits, while 17 comments across 7 files say "GetMethod" and then open-code it. In `string.rs` alone **9 sites** spell `GetMethod(V, @@x)` → `if !method.is_nullish() { call }`: `split:690-702`, `replace:790-800`, `replaceAll:946-960`, `search:1084-1096` and again `:1112-1120`, `match:1139-1150` and again `:1166-1172`, `matchAll:1244-1256` and again `:1271-1284`. Seam: `get_method(&mut self, value, key) -> Result<Option<JsValue>, JsValue>` (`Ok(None)` for nullish, `Err` for a throwing getter). Second family in the same file: `is_regexp` exists at `string.rs:82` and is used by `includes:290`, `startsWith:338`, `endsWith:380`, but `replaceAll:912-930` and `matchAll:1213-1231` re-inline its 14-line body verbatim and then each re-spell the identical "flags must contain 'g'" check (`:938-944`, `:1236-1242`).
- **First seen**: 2026-09-18
- **Caveat (why not picked 2026-09-18)**: the sites carry a **script-observable drift**, not just duplication — `search:1115` and `match:1168` use `get_property_on_id`, which *swallows* a throwing `@@search`/`@@match` getter, while `matchAll:1272` uses `get_object_property`, which propagates it. Unifying them is therefore a **behaviour change** on at least one path, the same shape that got `proxy-blind-callable-check` dropped. test262 coverage is asymmetric: `String/prototype/matchAll/` pins the throwing-getter case exactly (`regexp-get-matchAll-throws.js`, `regexp-prototype-get-matchAll-throws.js`), but `match/` and `search/` carry only `invoke-builtin-*.js` — the throwing-getter case on the *internally created* RegExp is uncovered. A `test262-extra` pin must be written for `:1115`/`:1168` **before** either is touched, and the semantics decided; until then this is a behaviour-preserving extraction of the other 7 sites only.

## typed-array-kind-table

- **Status**: proposed
- **Score**: 19/25 (leverage 4, locality 4, blast radius 2, heat 3)
- **Files**: ~3 estimated — `src/interpreter/types.rs`, `src/interpreter/builtins/typedarray.rs`, `src/interpreter/builtins/atomics.rs`
- **Modules**: `src/interpreter/types.rs`
- **Summary**: "What a `TypedArrayKind` is" is re-derived across **~13 parallel 12-arm tables in 3 files**: `types.rs:1634 bytes_per_element`, `:1643 name`, `:3650 decode_ta_element`, `:3726 encode_ta_element`, `:474-485` (12 separate `*_prototype: Option<u64>` realm fields), `:578-589` (init), `:679-690` (GC root list); `typedarray.rs:3535` (NewTarget-realm prototype lookup), `:3827` (prototype store), `:4287 get_typed_array_prototype`, `:4323` (constructor-name → kind); `atomics.rs:1238 number_from_raw_bytes`, `:1319 number_raw_to_i64`, `:1337 write_i64_to_buffer`. Adding `Float16Array` — which this repo did — meant editing every one. Seam: extend `impl TypedArrayKind` (`types.rs:1632`) with `read(self, buf, order) -> JsValue` / `write(self, buf, order, v) -> bool` / `from_constructor_name(&str) -> Option<Self>`, and replace the 12 realm fields with `typed_array_prototypes: [Option<u64>; 12]` behind `Realm::typed_array_prototype(kind)`. Deletion test **concentrates**: hides per-kind element layout and coercion. Supporting symptom (not the load-bearing claim): `types.rs:3650/3726`, documented as "the single source of truth for the byte→JsValue mapping", uses `from_ne_bytes`/`to_ne_bytes` while `atomics.rs:1238/1319/1337` uses `*_le_bytes` for the same element layout — unobservable on x86-64/aarch64. DataView already got this right via a macro threading an explicit `le: bool` (`typedarray.rs:4768-4846`, `:5057-5138`).
- **First seen**: 2026-09-18
- **Caveat (why not picked 2026-09-18)**: **needs new tests first.** The existing `built-ins/TypedArray*` and `built-ins/Atomics/*` suites pin *behaviour* but nothing pins the 12-kind table as a unit, so test-first footing is weaker than the picked candidate's ready-made test262 pins. The honest move is a Rust unit test in `types.rs` looping all 12 kinds (the file already has that shape at `:4340-4600`) plus one `test262-extra` pin alongside the existing `DataView-typedarray-nan-bitexact-roundtrip.js`.

## typedarray-getter-receiver-guard

- **Status**: landed
- **PR**: #626 (merged 2026-09-11T11:34:50Z; reconciled in-flight→landed by the 2026-09-14 firing)
- **Score**: 22/25 (leverage 4, locality 4, blast radius 1, heat 5)
- **Files**: ~1 estimated — `src/interpreter/builtins/typedarray.rs`
- **Modules**: `src/interpreter/builtins/typedarray.rs`
- **Summary**: The 4 `%TypedArray%.prototype` getters (`byteOffset` :1313, `byteLength` :1328, `length` :1343, `buffer` :1359) each re-open-code the `as_object_id→get_object[_cell]→borrow→typed_array_info→(detach/OOB branch)→else create_type_error("not a TypedArray")` receiver dance, because `validate_typed_array` (:5655) hides exactly this but only under a **throw-on-detached** policy — the getters need **return-0-on-detached** (spec TypedArrayLength→0), so they reach past the seam. Alongside sits a 3-member validator sibling family (`validate_typed_array` :5655, `validate_uint8array` :5673, `validate_uint8array_no_detach_check` :5700) differing only in brand string + detach policy. Add one `require_typed_array_receiver(this, brand, detach_policy) -> Result<TypedArrayInfo, Completion>` guard (policy = Throw / NoCheck / ReturnZero) the 4 getters and 3 validators all route through. **Genuine deepening**, not straggler-adoption: the ReturnZero policy *extends what the seam hides* (the distinction that keeps `dataview-receiver-guard` proposed while `define-method-adoption` was dropped). Direct follow-up to landed `validate-typed-array` (#543); same family relation `arraybuffer-receiver-guard` (#570) bears to it. **Correction**: an earlier scan over-counted "~10 getters" — only 4 are genuine TypedArray-prototype getters; the rest were ArrayBuffer/SAB getters (#570) and DataView getters (`dataview-receiver-guard`).
- **First seen**: 2026-09-11
- **Picked**: 2026-09-11 firing. Top two tied at 22/25 with `gc-root-scope-guard-eval` (blast 3); won on the lower-blast-radius tie-break. Branch adopted (`sym/jsse/routine/refactor-audit/01M26RZFXE`), not renamed.

## gc-root-scope-guard

- **Status**: landed
- **PR**: #595 (merged 2026-09-04; reconciled in-flight→landed by the 2026-09-10 firing)
- **Score**: 22/25 (leverage 5, locality 4, blast radius 3, heat 5)
- **Files (full candidate)**: ~9–12 — `src/interpreter/eval.rs` (primary), `src/interpreter/builtins/array.rs`, `src/interpreter/mod.rs` (seam home), + `iterators.rs`, `promise.rs`, `exec.rs`, `atomics.rs`, `typedarray.rs`, `property.rs`, `eval/literals.rs`, `bytecode/vm.rs`
- **Files (this firing's scope)**: ~2 estimated — `src/interpreter/mod.rs` (new `with_gc_root_scope` seam) + `src/interpreter/builtins/array.rs`
- **Modules**: `src/interpreter/mod.rs`, `src/interpreter/builtins/array.rs`
- **Summary**: Collapse the manual GC-root frame teardown epilogue behind a scope-guard combinator `with_gc_root_scope(|i| …)`, mirroring the in-file precedents `with_tail_position_suppressed` (`eval.rs:410`) and the `iterate_to_vec` IIFE (`iterators.rs:5185`). Codebase-wide: ~156 `gc_unroot_frame` teardowns against ~55 `gc_root_frame` setups — the gap is per-early-return epilogue copies. **This firing scopes to `array.rs`**, the single worst concentration (10 `Completion`-returning functions, 71 teardowns / 10 setups = ~61 redundant epilogue copies; `concat` alone repeats the teardown 10×). The remaining sites — `eval.rs` foremost — are deferred to `gc-root-scope-guard-eval` because `eval.rs` carries the `#[inline(always)]` `eval_expr` hot path, two seam bypasses that poke `gc_temp_roots.push` directly (`eval.rs:1066`, `:4324`), and 5 sites that already work around the epilogue with an IIFE — correctness-sensitive, a deliberately-scheduled firing. Mirrors the `arraybuffer-receiver-guard` → `dataview-receiver-guard` split.
- **First seen**: 2026-09-03
- **Picked**: 2026-09-04 firing (was recorded 2026-09-03 as runner-up to `complete-state-machine-generator-ctor`; #592 landing made it the natural next pick).
- **Delivered (PR #595)**: `with_gc_root_scope` combinator added in `mod.rs`; 8 whole-body `array.rs` natives migrated (concat, slice, map, filter, splice, flat, flatMap, Array.from array-like path). array.rs teardowns 71→9 (the 9 are Array.from's nested iterator frames, kept on the raw primitive). Net −17 lines (array.rs shows ~867 changed, mostly rustfmt reindent from the closure wrap). Also fixed 3 latent over-rooting exits in concat. Gate green: 622 unit / test262 built-ins/Array 6117/6117 (0 regressions) / 13 custom / clippy+fmt clean. Chose the closure combinator over an RAII guard (see PR's Proposed ADR); `eval.rs` + remaining files deferred to `gc-root-scope-guard-eval`.

## gc-root-scope-guard-eval

- **Status**: landed
- **PR**: #628 (merged 2026-09-14T07:19:16Z; reconciled in-flight→landed by the 2026-09-18 firing)
- **Score**: 22/25 (leverage 5, locality 4, blast radius 3, heat 5)
- **Files**: ~10 estimated — `src/interpreter/eval.rs` (primary) + `iterators.rs`, `promise.rs`, `exec.rs`, `atomics.rs`, `typedarray.rs`, `property.rs`, `eval/literals.rs`, `mod.rs`, `bytecode/vm.rs`
- **Modules**: `src/interpreter/eval.rs`
- **Summary**: Follow-up to `gc-root-scope-guard` covering `eval.rs` and the remaining ~9 files once the `with_gc_root_scope` seam exists. **2026-09-14 re-scan: `eval.rs` now has 22 `gc_root_frame` setups / 41 teardowns (was 50; #624 removed ~9 via the `yield*` migration), 10 IIFE sites (was recorded as 5 — undercount) + 12 manual-epilogue sites (~19 redundant teardown copies), 1 remaining `gc_temp_roots.push` bypass at :4302 (the :1066 one became `with_gc_root_scope` in #624), 1 manual remove at :4566. Critically, the headline blocker is GONE: `eval_expr` is no longer `#[inline(always)]` (def at :418, no attribute; `EvalDepthGuard` doc no longer warns), so adopting the `#[inline]` combinator adds no hot-path frame concern.** ADR-2026-09-10-2014 sanctions this remaining scope, per-site against the criterion single frame / no cross-branch identity removal / no multi-tick continuation.
- **First seen**: 2026-09-04
- **Picked**: 2026-09-14 firing (22/25, top; runner-up `completion-into-result` 21/25 within 1 point). Branch adopted (`sym/jsse/routine/refactor-audit/01M2EG403H`), not renamed. This firing scoped to `eval.rs`, mirroring how #595 scoped the parent to `array.rs`.
- **Delivered (2026-09-14 firing)**: **5 sites** — manual-epilogue :1385 (tagged-template call, 3 hand-threaded teardowns → returns) + IIFE sites :2611 (private `#x++`), :2744 (computed member set), :3421 (private logical-assign), :4046 (member compound set). Winning design: **A — bare `with_gc_root_scope` adoption** (Design B value-rooting slice rejected on E0505 shared-borrow-vs-move; Design C rooting-handle borrow-infeasible). The two largest IIFE bodies (:3016 ~273 lines, :3517 ~85 lines) were **deferred to `gc-root-scope-guard-remainder`** at implementation time — 0 redundant teardowns, near-cosmetic full-body `self.`→`i.` rewrite, better as a human-reviewed slice. Gate green: 676 lib unit tests (+1 new pin) / lint / test262-extra 316·316 / 7,950 targeted test262 (0 regressions). CONTEXT.md already carries `Temp-Root Frame` + `GC Root Scope` (from #595) — no glossary change. On PR open, status flips to in-flight with the PR number; the remainder stays `proposed`.

## gc-root-scope-guard-remainder

- **Status**: proposed
- **Score**: 22/25 (leverage 5, locality 4, blast radius 3, heat 5)
- **Files**: ~10 estimated — the `eval.rs` sites the 2026-09-14 firing deferred + the non-`eval.rs` files
- **Modules**: `src/interpreter/eval.rs`, `builtins/mod.rs`, `iterators.rs`, `promise.rs`, `exec.rs`, `atomics.rs`, `typedarray.rs`, `eval/literals.rs`, `mod.rs`, `interpreter/bytecode/vm.rs`
- **Summary**: The tail of `gc-root-scope-guard-eval` after the 2026-09-14 firing took `eval.rs`'s 5 clean member/tagged-template sites. Remaining `eval.rs` sites: the two large IIFE bodies :3016 (~273 lines) / :3517 (~85 lines) deferred at implementation time (0 redundant teardowns — near-cosmetic full-body `self.`→`i.` rewrite, better human-reviewed), the array/object-destructuring IIFEs :4340/4453/4621/4702 (ADR-flagged sensitive region — roots `DestructLRef`, nests frames; migratable per ADR criterion but higher-risk), the hot call/spread manual-epilogue sites :4839/4931/5109/5124/6716/6749 (highest teardown redundancy incl. :6749's 7 → a deliberately-scheduled slice), the promise/multi-tick sites :8193/9735, the two cosmetic single-exit sites :4268/5366 (no redundant teardown — low value). Explicitly **out of scope forever**: the ADR-excluded `gc_temp_roots.push`/`remove` bypasses (:4302, :4566, destructuring identity-removal) and `gc_unroot_value` sites. Plus the 9 non-`eval.rs` files. **`promise.rs` overlaps `promise-combinator-setup-prologue`** — whichever lands second adapts. (Note: pre-migration `eval.rs` line numbers; re-derive after the 2026-09-14 PR lands.)
- **First seen**: 2026-09-14 (carved from `gc-root-scope-guard-eval`)
- **2026-09-18 re-scan (post-#628, line numbers now current)**: tree-wide **48 `gc_root_frame` setups / 83 `gc_unroot_frame` teardowns = 35 redundant epilogue copies** (was ~55/~156 pre-#595). Per file — `eval.rs` 17/34 (+6 adopted at :1061, :1389, :2610, :2740, :3414, :4036); `iterators.rs` 10/10 (1:1, no redundancy; :1513–:2008 are 8 sibling closures in `setup_iterator_helper_methods`); `promise.rs` 8/9; `builtins/mod.rs` **1/7**; `exec.rs` 1/1; `atomics.rs` 1/1; `typedarray.rs` 1/1; `eval/literals.rs` 2/2; `mod.rs` 4/4. **Three structural corrections.** (a) **`property.rs` has ZERO frame call sites** — struck from the module list; its only match is a doc comment at `:575` recording why `array_set_length` (:580) deliberately uses `gc_root_value`/`gc_unroot_value` identity removal instead (a `valueOf` call can leave a *persistent* root that a frame truncate would discard). It is documented-excluded, not a candidate. (b) **`src/interpreter/builtins/mod.rs` was missing from this entry entirely** and now holds the single best remaining site: `Object.fromEntries` at **:6463**, 1 setup / **7 teardowns** (:6470, 6477, 6483, 6492, 6502, 6512, 6521) over a ~69-line closure, passing every ADR criterion (single frame, no identity removal, no multi-tick continuation, whole-body). One benign delta to state in the PR: 4 of the 7 teardowns currently run *before* `iterator_close`, so wrapping widens the `iterator` root across that user-code call — strictly safer, but a real lifetime change. (c) `src/bytecode/vm.rs` does not exist; the real path is `src/interpreter/bytecode/vm.rs`, and it is already **5:5 balanced** (one `gc_root_frame` inside the helper `root_operand_stack` at :38, called from :343, :369, :386, :401, :720) — **no redundancy there, strike it**. **Risk correction on the highest-count `eval.rs` site**: `construct_from_evaluated` (**:6736**, 1 setup / 7 teardowns — the entry's ":6749's 7") is *higher-risk than the count implies*. Only :6769/:6776 are terminal throws; the other five (:6790, :6822, :6869, :6899, :7118) are **deliberate early unroots before delegating** to `invoke_proxy_trap` / `construct_with_new_target` / `call_constructor_body`, so a trailing bulk truncate would run *after* those callees and discard any persistent root they register (the `Atomics.waitAsync`-resolver hazard the ADR and `property.rs:571-578` both name). Needs per-exit analysis, not a mechanical wrap — split or defer it. Safest remaining slices, in order: `builtins/mod.rs:6463` (6 redundant), `eval.rs:4918` private-method call path (2 redundant, ~30 lines, all three exits terminal), `eval.rs:6703` `eval_new` (1 redundant, ~34 lines, whole-body), and the four identical 5-line microtask-root frames (`eval.rs:9826/9831` + `mod.rs:5497/5502`, `5619/5625`, `5741/5746`) which the ADR explicitly blesses as plain LIFO nesting. `eval.rs:8180` (`call_async_function`) and `:9722` (`await_value`) are **ADR-excluded** (multi-tick continuation) — strike them from the entry's scope.

## promise-combinator-setup-prologue

- **Status**: proposed
- **Score**: 20/25 (leverage 4, locality 4, blast radius 1, heat 3)
- **Files**: ~1 estimated — `src/interpreter/builtins/promise.rs`
- **Modules**: `src/interpreter/builtins/promise.rs`
- **Summary**: 4 full combinators (`promise_all` :1238, `promise_all_settled` :1386, `promise_race` :1964, `promise_any` :2039) + 2 partial keyed fast-path hooks (`promise_all_keyed` :1577, `promise_all_settled_keyed` :1740) re-spell the identical prologue — `NewPromiseCapability(C)` → root cap on a GC frame → `GetPromiseResolve(C)` + `is_callable` → `GetIterator(iterable)`, each abrupt through `if_abrupt_reject_promise` (:13). Extract `perform_promise_combinator_setup(constructor, iterable) -> Result<(PromiseCapability, JsValue, JsValue), Completion>`. Deletion test concentrates: hides the *abrupt-becomes-rejected-promise vs thrown-completion* decision. Composes with `gc-root-scope-guard-remainder`'s `promise.rs` frames. First seen 2026-09-14.

## regexp-object-receiver-guard

- **Status**: proposed
- **Score**: 18/25 (leverage 3, locality 3, blast radius 1, heat 4)
- **Files**: ~1 estimated — `src/interpreter/builtins/regexp.rs`
- **Modules**: `src/interpreter/builtins/regexp.rs`
- **Summary**: ~13 sites open-code `match this.as_object_id() { Some(id)=>id, None=>return Throw(TypeError "…requires that 'this' be an Object") }` (exec :8325, test :8363, toString :8392, compile :8435, `@@match` :8592, `@@search` :8801, `@@replace` :8882, `@@split` :9456, `@@matchAll` :9692, RegExpStringIterator.next :9857, `flags` :10137, flag getters :10195, `source` :10249). Two **deliberate** realm policies (not drift): methods use caller-realm `create_type_error`; the flag/`source` accessors use `create_error_in_realm(my_realm_id, …)` captured at getter-creation (:10181) — cross-realm accessor semantics. `flags` also does a second liveness check (:10144) the others skip. `require_regexp_object_receiver(this, name, realm_policy) -> Result<u64/cell, Completion>`. Genuine deepening only if it returns the cell + carries the realm policy (else it's `object-this-coercion`-class); RegExp sibling of `object-this-coercion` and the receiver-guard family, in a module with none. First seen 2026-09-14.

## complete-state-machine-generator-ctor

- **Status**: landed
- **Score**: 22/25 (leverage 5, locality 4, blast radius 1, heat 3)
- **Files**: ~2 estimated — `src/interpreter/eval/generator_runtime.rs`, `src/interpreter/types.rs`
- **Modules**: `src/interpreter/eval/generator_runtime.rs`
- **Summary**: Collapse **97** byte-identical inlined "completed state-machine generator" 10-field struct literals (**31 sync + 66 async**) into `completed_state_machine_generator` / `…_async_generator` constructors. Landed net −576 lines, gate green (621 unit / 3168 test262 generator scenarios, 0 regressions / 13 custom).
- **First seen**: 2026-09-01
- **PR**: #592 (merged 2026-09-03)

## arraybuffer-receiver-guard

- **Status**: landed
- **Score**: 24/25 (leverage 5, locality 4, blast radius 1, heat 5)
- **Files**: ~1 estimated — `src/interpreter/builtins/typedarray.rs`
- **Modules**: `src/interpreter/builtins/typedarray.rs`
- **Summary**: Collapse the 5 ArrayBuffer getters' inline `enum Probe` borrow-escape prologues + 3 SharedArrayBuffer getters behind snapshot-returning receiver guards (`require_array_buffer` / `require_shared_array_buffer`), mirroring the landed `validate_typed_array` (#543). Guard returns `is_detached` in the snapshot rather than throwing (getters return 0 on detached). DataView getters + borrow-holding methods deferred to `dataview-receiver-guard`.
- **First seen**: 2026-09-02
- **PR**: #570 (merged 2026-09-02)

## validate-typed-array

- **Status**: landed
- **Score**: 24/25 (leverage 5, locality 4, blast radius 1, heat 5)
- **Files**: ~1 estimated — `src/interpreter/builtins/typedarray.rs`
- **Modules**: `src/interpreter/builtins/typedarray.rs`
- **Summary**: Collapse open-coded TypedArray receiver-validation prologues (brand check + detached/out-of-bounds check + clone + doubled `not a TypedArray` throw) behind one `validate_typed_array` seam, mirroring the existing kind-gated `validate_uint8array`. Landed: 14 read-mode sites migrated; 3 (`slice`, `sort`, `toSorted`) kept — they hold the object borrow across their body.
- **First seen**: 2026-09-01
- **PR**: #543 (merged 2026-09-01)

## completion-into-result

- **Status**: proposed
- **Score**: 21/25 (leverage 4, locality 3, blast radius 1, heat 5)
- **Files**: ~2 estimated — `src/interpreter/builtins/iterators.rs`, `src/interpreter/types.rs`
- **Modules**: `src/interpreter/builtins/iterators.rs`
- **Summary**: Add `Completion::into_result(self) -> Result<JsValue, JsValue>` and collapse the ~37 hand-rolled `match Completion { Normal(v)=>v, Throw(e)=>return Err(e), _=>… }` adapter heads in the Result-returning iterator abstract-operation helpers to `.into_result()?`; the fabricated `_ =>` error arms become removable dead code. First seen 2026-09-02. (2026-09-14 re-check: 37 `Normal(v)=>v` unwrap heads, 25 `Throw(e)=>return Err(e)` arms, **18 canonical 3-arm heads** at `iterators.rs:308, 322, 418, 427, 439, 483, 506, 527, 545, 4513, 4530, 4547, 4553, 4593, 4903, 5038, 5114, 5128` — all 18 shed a fabricated `_ =>` dead arm. **Runner-up candidate** to the 2026-09-14 pick, within 1 point.) **2026-09-18 re-check: friction 100% intact** (37 and 25 arms, and all 18 heads, at the exact lines above; #622 deleted 90 lines from this file but touched only borrow preambles). Two corrections. (a) **"byte-identical" was wrong**: arms 1–2 are identical across all 18, but **arm 3 splits 10 / 2 / 6** — 10× `_ => JsValue::UNDEFINED` (treating `Empty` as a *success* value), 2× `_ => return Ok(())`, 6× `_ => return Err(create_type_error(…))` with 4 distinct messages. `into_result` must pick **one** disposition for `Empty`/`Break`/`Continue`/`Exit` where the sites currently pick three, so the deletion test has to *argue* the `_ =>` arms are dead, not assume it. (b) **Confirmed complementary to `propagate!`, not redundant and not blocked**: `propagate!` expands to `return c` where `c: Completion` and therefore cannot compile inside any of the 9 `-> Result<T, JsValue>` helpers that host all 18 heads, and `IntoAbrupt`'s `other => Err(other)` is actively incompatible with the 10 `_ => UNDEFINED` sites. Repo-wide the same `Throw(e) => return Err(e)` shape appears **96 times across 21 files** (iterators.rs 25, eval.rs 22, exec.rs 9, helpers.rs 6, intl/mod.rs 6, temporal/mod.rs 5, typedarray.rs 4, promise.rs 4, property.rs 3, collections.rs 3, rest ≤2). Recommend landing it as an `IntoThrow` trait + `propagate_err!` macro **in #623's `types.rs` home**, so it composes with the existing seam instead of being a parallel invention and generalises to the 71 non-`iterators.rs` sites. `propagate!` adoption today is only 21 sites (string.rs 13, tests.rs 5, temporal/duration.rs 4) — ~1.2% penetration.

## completion-unwrap-macro

- **Status**: landed
- **PR**: #623 (merged 2026-09-10T19:59Z; reconciled in-flight→landed by the 2026-09-11 firing). Delivered as the `propagate!` macro + `IntoAbrupt` trait in `src/interpreter/types.rs`, adopted in `string.rs` and `temporal/duration.rs`; CONTEXT.md gained the "Completion Propagation" term.
- **Score**: 23/25 (leverage 5, locality 3, blast radius 1, heat 5)
- **Files (this firing's scope)**: ~3 estimated — hoist macros out of `src/interpreter/builtins/temporal/duration.rs:9-25` into a crate-visible home in `src/interpreter/types.rs`, re-point `duration.rs`, adopt one representative file (`array.rs`/`string.rs`/`typedarray.rs` — chosen at step 5 from the shape-3 concentrations).
- **Modules**: `src/interpreter/types.rs`, `src/interpreter/builtins/temporal/duration.rs`
- **Summary**: Promote the private error-propagation macros to a crate-visible seam. **2026-09-10 re-score (leverage 4→5): fresh scan found `try_completion!` AND a sibling `try_result!` already exist but are `macro_rules!`-private to `temporal/duration.rs:9-25`, and the true footprint is ~1725 hand-rolled adapter sites** (911 `Result<_,JsValue>`→`Completion::Throw`, 598 `Result<_,Completion>`→`return c`, 216 `Completion`→bind-`Normal`) — not the ~200 last estimated. Hoist both macros to a shared home (drop `try_result!`'s vestigial unused `$interp` param), add the missing `Result<T,Completion>` arm, re-point `duration.rs`, adopt one file to prove the seam and pin behaviour. Distinct from `completion-into-result` (Result-return context, a method) and `throw-error-completion` (throw-site wrapper). Scope the first step to one adopter to stay blast-radius 1. First seen 2026-09-02.
- **Picked**: 2026-09-10 firing (re-scored 23/25, above runner-up `gc-root-scope-guard-eval` 22/25 by 1 point on the inverted blast-radius term). Branch adopted (`sym/jsse/routine/refactor-audit/01M25W97CF`), not renamed.

## settle-and-return-tail

- **Status**: proposed
- **Score**: 21/25 (leverage 4, locality 4, blast radius 1, heat 4)
- **Files**: ~1 estimated — `src/interpreter/eval/generator_runtime.rs`
- **Modules**: `src/interpreter/eval/generator_runtime.rs`
- **Summary**: Extract `settle_and_return(settle_fn, arg, promise)` for the "call settle fn + drain microtasks + return promise" async-generator exit tails. **2026-09-18 re-check: 54 canonical tails, not ~47** — 56 `drain_microtasks()` sites of which 54 are the canonical shape (48 `reject_fn`, 6 `resolve_fn`; 48 spelled `return Completion::Normal(promise);`, 6 as a bare tail expression). The prior "sequence after `complete-state-machine-generator-ctor`" note is **struck as stale**: #592 landed 2026-09-03 and the tail count went *up*, not down — the two do not interact. Heat raised 3→4 on that growth.

## this-weak-map-set

- **Status**: proposed
- **Score**: 20/25 (leverage 4, locality 4, blast radius 1, heat 3)
- **Files**: ~1 estimated — `src/interpreter/builtins/collections.rs`
- **Modules**: `src/interpreter/builtins/collections.rs`
- **Summary**: Add `this_weak_map` / `this_weak_set` sibling helpers to collapse 9 inconsistently hand-rolled WeakMap/WeakSet receiver-unwrap dances, mirroring the existing `this_map` / `this_set`. (2026-09-02 re-check: `this_map`/`this_set` still present; the WeakMap/WeakSet brand strings differ from `not a WeakMap` — confirm the exact error wording per site before migrating.)
- **2026-09-18 re-check: STILL-LIVE (9/9) and the 2026-09-02 blocker is RESOLVED.** All 9 hand-rolled sites remain — WeakMap `get` :1699, `set` :1723, `has` :1754, `delete` :1778, `getOrInsert` :1806, `getOrInsertComputed` :1841; WeakSet `add` :2101, `has` :2130, `delete` :2154 (in `collections.rs`). Drift confirmed across **four distinct spellings** of the same brand check: `get`/`has`/`delete` (:1703) read `class_name` then `map_data()`; `set` (:1728) reverses that order; `getOrInsert*` (:1810) combines both in one `let`; WeakSet `add` (:2105) uses `set_data()`. A third drift axis: all 9 use `get_object_cell(o)` where the existing `this_map` (:12) / `this_set` (:34) siblings use `get_object(o)`. **The blocker is gone**: the prior caveat ("the WeakMap/WeakSet brand strings differ from `not a WeakMap` — confirm the exact error wording per site") is resolved — all 9 messages are uniformly `"{Brand}.prototype.{method} requires a {Brand}"`, the same template `this_map`/`this_set` already format, so a parametrized `this_weak_map`/`this_weak_set` is a drop-in. Possible 10th site: `WeakRef.prototype.deref` (:2369) follows the same template, though on a different data kind.

## object-this-coercion

- **Status**: proposed
- **Score**: 19/25 (leverage 3, locality 3, blast radius 1, heat 4)
- **Files**: ~1 estimated — `src/interpreter/builtins/mod.rs`
- **Modules**: `src/interpreter/builtins/mod.rs`
- **Summary**: A `require_this_object(this) -> Result<u64, Completion>` ToObject prologue collapsing ~10 open-coded `match to_object(this_val) { Normal(v)=>v, other=>return other }` + object-id-unwrap dances in Object.prototype methods. A coercion prologue (ToObject can run user code), distinct from the `object-id-of` round-trip. First seen 2026-09-02. (2026-09-04 re-check: 10 `to_object(this` sites present.) **2026-09-18 re-check: leverage downgraded 4→3.** All 10 sites still present at `mod.rs:4221, 4257, 4332, 4384, 4422, 4471, 4540, 4607, 4673, 4737`, but #623's `propagate!` is now a ready-made behaviour-preserving rewrite for the 8 byte-identical heads (`IntoAbrupt for Completion` maps `other => Err(other)`, and its doc comment explicitly names "the `other => return other` sites this seam replaces"). That collapses the completion-unwrap half via an existing seam, leaving only the **id-returning** guard as residual deepening — 7 of 10 sites still unwrap `as_object_id()` within 6 lines. Two sites are non-conforming and must stay hand-written: `:4332` (`Object.prototype.valueOf`) is an identity pass-through with no `return`, and `:4737` (`__proto__` getter) maps non-Throw abrupt completions to a **Normal** result, so routing it through `propagate!` would change behaviour on `Empty`. Note `propagate!` is adopted **0 times** in `mod.rs` today.

## iterator-close-return-dance

- **Status**: proposed
- **Score**: 20/25 (leverage 3, locality 4, blast radius 1, heat 5)
- **Files**: ~2 estimated — `src/interpreter/builtins/iterators.rs`
- **Modules**: `src/interpreter/builtins/iterators.rs`
- **Summary**: Four parallel reimplementations of spec IteratorClose (GetMethod(iterator,"return") → Call → handle result) that have drifted: `iterator_close_getter` (`iterators.rs:319`) and `iterator_close_with_completion` (`:587`, 2026-09-14 re-check; was :541) omit the `is_callable` pre-check that `iterator_close` (`:4999`; was :5103, #622 deleted ~104 lines above) and `iterator_close_result` (`:5030`; was :5134) perform; only the latter two handle `Completion::Exit` (the `__host_exit` floor). Extract one core `iterator_close(iterator, completion) -> Completion` all four delegate to, differences (Result vs JsValue wrapper, completion-priority, Exit) as thin adapters. Leverage 3 by backlog calibration (4 implementation sites; the 121 downstream callers do not change). Distinct from `unify-generator-async-drivers` (execution driver, not IteratorClose). First seen 2026-09-04.

## generator-entry-guard

- **Status**: proposed
- **Score**: 19/25 (leverage 4, locality 3, blast radius 1, heat 3)
- **Files**: ~1 estimated — `src/interpreter/eval/generator_runtime.rs`
- **Modules**: `src/interpreter/eval/generator_runtime.rs`
- **Summary**: Fold ~11 duplicated generator-entry "called on non-object" TypeError pairs into a `require_generator_object` guard. Ties into object-id-of.

## ordinary-create-from-constructor

- **Status**: proposed
- **Score**: 19/25 (leverage 5, locality 4, blast radius 4, heat 3)
- **Files**: ~15–20 estimated — `src/interpreter/builtins/collections.rs`, `disposable.rs`, `typedarray.rs`, `proxy.rs`, `iterators.rs`, `date.rs`, `promise.rs`, all `intl/*`, all `temporal/*`
- **Modules**: `src/interpreter/mod.rs` (seam home), `src/interpreter/builtins/collections.rs`
- **Summary**: Every `[[Construct]]` builtin hand-inlines OrdinaryCreateFromConstructor in two separable pieces: a new-target guard (`if new_target.is_none() { Throw }`, 29 sites) and prototype-resolution + object materialization (`match get_prototype_from_new_target_realm(...)` + `create_object_id` + field-set, 38 sites). `get_prototype_from_new_target_realm` (`mod.rs:1410`) already exists but the ~15 lines around it are copy-pasted per constructor, and drift is live (`collections.rs:479-488` does three separate `borrow_mut` + `.unwrap_or`, `disposable.rs:377-389` batches one borrow + `if let Some`). Ship as two composable helpers (`require_new_target(name)` + `ordinary_create_from_constructor(...)`) since Promise validates its executor between the two steps. Blast radius 4 (15–20 files, crosses many builtin families) drags the total below the pick; best done in waves by a human-scheduled firing. First seen 2026-09-04.

## pattern-bound-names-walker

- **Status**: proposed
- **Score**: 19/25 (leverage 3, locality 3, blast radius 1, heat 5)
- **Files**: ~1 estimated — `src/interpreter/exec.rs`
- **Modules**: `src/interpreter/exec.rs`
- **Summary**: Delete `collect_pattern_bound_names` (a near-byte-identical copy of `ast::Pattern::bound_names`) from the for-of TDZ path and reuse the existing method. Low count (1 straggler) but hot file. (2026-09-02 re-check: 5 references still present.)

## dataview-receiver-guard

- **Status**: proposed
- **Score**: 18/25 (leverage 4, locality 3, blast radius 1, heat 5)
- **Files**: ~1 estimated — `src/interpreter/builtins/typedarray.rs`
- **Modules**: `src/interpreter/builtins/typedarray.rs`
- **Summary**: Follow-up to `arraybuffer-receiver-guard` covering the DataView getters (`buffer`/`byteOffset`/`byteLength`) and the borrow-holding ArrayBuffer methods. Harder than the getter family: DataView getters *throw* on IsViewOutOfBounds (subsumes detached), compute a per-getter OOB condition, and read through to the underlying buffer (cross-object), and the methods re-probe detached after the species constructor runs user code. First seen 2026-09-02.

## this-primitive-value

- **Status**: proposed
- **Score**: 18/25 (leverage 3, locality 4, blast radius 1, heat 3)
- **Files**: ~5 estimated — `src/interpreter/builtins/number.rs`, `bigint.rs`, `string.rs` (+ helper home)
- **Modules**: `src/interpreter/builtins/number.rs`
- **Summary**: Five near-identical private helpers implement "return primitive X, else unwrap a wrapper object whose `class_name == "X"` reading `primitive_value`, else None/throw": `this_number_value` (`number.rs:397`), `this_boolean_value` (`:667`), `this_symbol_value` (`:258`), `this_bigint_value` (`bigint.rs:40`), `this_string_value` (`string.rs:6`), differing only by the class-name literal and the primitive extractor. A generic `this_primitive_value(this, class_name)` (or small trait) collapses the parallel brand-and-unwrap bodies. Wrapper-object analogue of `object-this-coercion` (ToObject), so net-new. First seen 2026-09-04. (2026-09-11 re-check: the parallel family is **4, not 5** — `this_string_value` (`string.rs:6`) is *not* a sibling: it returns `Result<String, Completion>`, takes `&mut`, and does RequireObjectCoercible + ToString fallback rather than the Option-returning brand-unwrap the other four share. Re-scored leverage 3, total 18/25.)

## regexp-last-index-accessor

- **Status**: proposed
- **Score**: 18/25 (leverage 3, locality 3, blast radius 1, heat 4)
- **Files**: ~1 estimated — `src/interpreter/builtins/regexp.rs`
- **Modules**: `src/interpreter/builtins/regexp.rs`
- **Summary**: `get_last_index(interp, rx) -> Result<usize, Completion>` / `set_last_index(interp, rx, v)` for the 5 read+ToLength and 3 `spec_set(...,"lastIndex",...)` sites re-spelling the `Get(R,"lastIndex")`→`ToLength` / `Set(R,"lastIndex",v,true)` dance inline. First seen 2026-09-02.

## object-id-of

- **Status**: dropped
- **Score**: 13/25 (leverage 2, locality 2, blast radius 3, heat 4)
- **Files**: ~3+ estimated — `src/interpreter/eval.rs`, `src/interpreter/eval/generator_runtime.rs`, `src/interpreter/exec.rs`
- **Modules**: `src/interpreter/eval.rs`
- **Summary**: ~130 circular `.as_object_id().map(|id| JsObject { id })` round-trips that rebuild a `JsObject` only to read `.id` back out. A `/simplify`-class cleanup, not a deepening — recorded so future runs don't re-derive it as a deep-module candidate.
- **First seen**: 2026-09-01
- **Reason**: Leverage 2 — `/simplify`-class round-trip cleanup, fails the deepening bar (complexity renamed, not concentrated behind a seam). (2026-09-14 re-check: filter still applies; now 158 `.map(|id| JsObject { id })` sites, up from ~130.)

## proxy-blind-callable-check

- **Status**: dropped
- **Score**: n/a (behaviour change, not a deepening)
- **Files**: ~4–6 estimated — `src/interpreter/builtins/typedarray.rs`, `collections.rs`, `iterators.rs`
- **Modules**: `src/interpreter/builtins/typedarray.rs`
- **Summary**: 37 `builtins/` sites open-code the callability test as a bare `obj.borrow().callable.is_some()`, bypassing the Proxy branch that the canonical `is_callable` (`promise.rs:2132`) handles — so `new Proxy(fn, {})` passed as a TypedArray `sort` comparator / `from` mapfn / Map `adder` throws "not a function" though spec IsCallable is true. Routing all 37 through `self.is_callable` would fix the bug and unify the checks.
- **First seen**: 2026-09-04
- **Reason**: This is a spec-conformance **behaviour change** (a latent bug fix), not a behaviour-preserving deepening — an unattended deepening run must pin existing behaviour before moving it, and here existing behaviour is wrong. File as a jsse bug report instead; a deepening that routes the checks through `is_callable` can follow once the semantics are agreed.
- **2026-09-21 re-check: filter still applies — and the distinction that keeps it dropped is worth stating, because the 2026-09-21 pick is also a behaviour change.** What blocks *this* entry is the clause "once the semantics are agreed": nothing in-tree says what the proxy-blind sites *should* do, so an unattended run would be choosing the spec reading itself. `state-machine-terminator-dispatch` has no such gap — two of its three drivers already produce the correct answer on the identical program, and `builtins/node_host.rs:84-93` documents the intended semantics in prose, so the target behaviour is read off the tree rather than decided by the run. The operative filter is therefore *"can current behaviour be pinned and is the target unambiguous?"*, not *"is any behaviour changed?"*.
- **2026-09-18 correction — the stated example is WRONG, tested directly.** The entry claims `new Proxy(fn, {})` "throws 'not a function' though spec IsCallable is true". It does not: `Proxy` construction **copies a callable target's `callable` slot onto the proxy object** (`src/interpreter/builtins/proxy.rs:38-41`), so the bare `obj.borrow().callable.is_some()` probe sees it without unwrapping and **agrees with `is_callable`** for a Proxy directly wrapping a function. Verified by `require_callable_arg_tests::proxy_wrapped_function_is_accepted_matching_is_callable` (`iterators.rs`), which asserts both `interp.is_callable(&p)` and that the probe accepts the same value. Scope of this correction: the **8 iterator-helper sites** the 2026-09-18 firing touched, for a Proxy whose target is directly callable. The other ~29 sites in this entry were **not** retested, and a divergence may still exist for shapes not covered (e.g. a revoked Proxy, or a target that becomes callable after construction). **Status stays `dropped`** — a human should re-derive the real divergence before reopening, because the motivating example does not reproduce.

## define-accessor-adoption

- **Status**: dropped
- **Score**: n/a (leverage 2 — finishing an existing migration)
- **Files**: ~10–12 estimated — `src/interpreter/builtins/temporal/*`, `regexp.rs`, `iterators.rs`, `mod.rs`
- **Modules**: `src/interpreter/mod.rs`
- **Summary**: A `define_getter` seam already exists (`mod.rs:1812`) and is adopted at 22 sites, but 42 sites still open-code the getter as `create_function(...)` + raw six-field `PropertyDescriptor` + `insert_property`. The genuinely net-new piece is a `define_accessor(name, get, set)` for the 4 getter+setter sites lacking a helper.
- **First seen**: 2026-09-04
- **Reason**: Leverage 2 — the deep seam already exists, so migrating the raw getters is `/simplify`-class finishing work, not a new deep module. The `define_accessor` (getter+setter) piece is genuinely net-new but only 4 sites, too low-leverage to pick. (2026-09-14 re-check: `define_getter` at `mod.rs:1840` with 24 adopters; raw getter sites grew 42→54. Filter still applies.)

## typedarray-shared-equality

- **Status**: dropped
- **Score**: n/a (leverage 2 — `/simplify`-class, not a deepening)
- **Files**: ~1 estimated — `src/interpreter/builtins/typedarray.rs`
- **Modules**: `src/interpreter/builtins/typedarray.rs`
- **Summary**: `typedarray.rs` re-implements private `same_value_zero` / `strict_eq` that already exist in `helpers.rs`. Deduping moves code rather than concentrating behaviour behind a new seam.
- **First seen**: 2026-09-02
- **Reason**: Leverage 2 — missed-reuse dedup, not a deep-module candidate. Caveat: the private `strict_eq` compares strings via `to_rust_string()`; a genuine semantic divergence must be confirmed first, and if real is a bug report rather than a dedup.

## unify-generator-async-drivers

- **Status**: dropped
- **Score**: n/a (blast radius 5 — too large for one unattended PR)
- **Files**: 40+ estimated — `src/interpreter/eval/generator_runtime.rs`
- **Modules**: `src/interpreter/eval/generator_runtime.rs`
- **Summary**: `generator_next_state_machine_impl` and `async_generator_next_state_machine_impl` are largely parallel ~1580/~3050-line state-machine interpreters. Unifying them is a deep structural refactor for a human to schedule.
- **First seen**: 2026-09-01
- **Reason**: Blast radius 5 — human-scheduled. Land the generator-constructor and settle-tail candidates first to shrink both drivers.
- **2026-09-21 re-check: filter still applies (blast radius 5), and the recommended path is now being followed.** The 2026-09-21 pick `state-machine-terminator-dispatch` is a deliberately bounded slice of exactly this friction — it unifies the *terminator-operand completion taxonomy* across the three drivers without merging the drivers themselves. `generator-completion-teardown` and `settle-and-return-tail` are the other two slices. Each shrinks both drivers; re-score this entry once all three have landed.

## throw-error-completion

- **Status**: proposed
- **Score**: 18/25 (leverage 3, locality 2, blast radius 1, heat 5)
- **Files**: ~1 estimated (one adopter file first) — all `builtins/*`; representative `src/interpreter/builtins/typedarray.rs:1426`.
- **Modules**: `src/interpreter/mod.rs` (seam home), `src/interpreter/builtins/typedarray.rs`
- **Summary**: `interp.throw_type_error(msg) -> Completion` / `throw_range_error` helpers collapsing the 535 `return Completion::Throw(interp.create_type_error(...))` sites (287 type, 248 range, 5 reference) that each re-spell the constructor lookup + `Completion::Throw` wrapper. Thinner than `completion-unwrap-macro` (hides one wrapper token) but large. First seen 2026-09-10.

## array-typedarray-immutable-methods

- **Status**: proposed
- **Score**: 17/25 (leverage 3, locality 3, blast radius 2, heat 4)
- **Files**: ~2 estimated — `src/interpreter/builtins/array.rs:1652/2535/2908`, `src/interpreter/builtins/typedarray.rs:2258/2315`.
- **Modules**: `src/interpreter/builtins/array.rs`, `src/interpreter/builtins/typedarray.rs`
- **Summary**: Two parallel implementations of `toReversed`/`toSorted`/`with` (Array vs TypedArray) that risk drifting. A shared core with thin per-kind adapters. Small (~3 method pairs) but the "parallel implementations drift" pattern the scan watches for. First seen 2026-09-10.

## arg-or-undefined

- **Status**: dropped
- **Score**: n/a (leverage 2 — shallow DRY helper)
- **Files**: ubiquitous across `builtins/*`; representative `typedarray.rs:1394`.
- **Modules**: `src/interpreter/builtins/mod.rs`
- **Summary**: `fn arg(args, n) -> JsValue` collapsing 943 `args.get(n).cloned().unwrap_or(JsValue::UNDEFINED)` chains (627 `.first()`, 316 `.get(N)`).
- **First seen**: 2026-09-10
- **Reason**: Leverage 2 — the largest raw site count in the tree, but a helper whose interface equals its implementation (shallow by definition). Pure DRY + naming, not a deep module; `/simplify`-class, same bar as `object-id-of`.

## define-method-adoption

- **Status**: dropped
- **Score**: n/a (leverage 2 — finishing an existing migration)
- **Files**: ~200–260 sites — `typedarray.rs`, `iterators.rs`, `promise.rs`, `regexp.rs`, `mod.rs`.
- **Modules**: `src/interpreter/mod.rs`
- **Summary**: The `define_method` seam already exists (`mod.rs:1819`, 258 adopters) but ~200 sites still register the verbose `create_function(...)` + `insert_builtin` way.
- **First seen**: 2026-09-10
- **Reason**: Leverage 2 — the deep seam already exists; migrating stragglers is `/simplify`-class finishing work, same reasoning as `define-accessor-adoption`.

## proxy-trap-skeleton

- **Status**: dropped
- **Score**: n/a (leverage 2 — thin, deep part already factored)
- **Files**: ~13 sites — `src/interpreter/property.rs` (`proxy_is_extensible:2016`, `proxy_prevent_extensions:2050`, …).
- **Modules**: `src/interpreter/property.rs`
- **Summary**: The `if proxy { trap-or-recurse } else { ordinary }` skeleton repeats across ~13 trap wrappers.
- **First seen**: 2026-09-10
- **Reason**: Leverage 2 — the deep part (`invoke_proxy_trap`) is already factored, and the per-trap middle validation genuinely differs, so only a thin skeleton would collapse.
