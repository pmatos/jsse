# Plan: bytecode — compile `new` expressions and compound member assignment (#603)

## 1. Problem restated

The bytecode compiler (`src/interpreter/bytecode/compiler.rs`) rejects two
constructs that are the bread and butter of typed-array numeric code:
`Expression::New` has no `compile_expr` arm at all (falls through to the
catch-all `Unsupported("expression:New")`), and `Expression::Member` as an
assignment target is only accepted under `AssignOp::Assign` — every compound
operator (`+=`, `-=`, `^=`, etc.) falls through to
`Unsupported("assign target")` (`compiler.rs:295,317`). Because bytecode
eligibility is whole-`Body`, one occurrence of either shape anywhere in a
function bails that function's *entire* body to the tree-walker permanently
(`BytecodeCacheState::Ineligible`, cached forever per function object). Issue
#361's tweetnacl-js benchmark showed this concretely: three functions holding
96% of the tree-walker's work in `scalarMult.base` (`M`, `car25519`,
`sel25519`) bail on exactly these two constructs — `var t = new
Float64Array(31)` and `o[i] += 65536`-shaped loops — so `--bytecode` never
reaches the hot numeric path at all (3.3% of work displaced despite 29% of
invocations compiled). Closing both gaps is a precondition for measuring
whether the VM helps typed-array field arithmetic, not a demonstration that it
does (see `docs/perf/2026-09-05/tweetnacl-bytecode-null-result.md`).

## 2. Spec basis

- **`new`**: `sec-new-operator` ("The `new` Operator", `NewExpression : new
  NewExpression` / `MemberExpression : new MemberExpression Arguments`) →
  `sec-evaluatenew` (`EvaluateNew`): evaluate the callee, `GetValue`, evaluate
  arguments left-to-right (`ArgumentListEvaluation`), then `IsConstructor`
  check (throw `TypeError` if false), then `sec-construct` (`Construct`).
  Order matters: arguments are evaluated *before* the `IsConstructor` check.
- **Compound member assignment**: `sec-assignment-operators-runtime-semantics-evaluation`,
  the `AssignmentExpression : LeftHandSideExpression AssignmentOperator
  AssignmentExpression` production: `lRef ← Evaluation(LHS)`; `lVal ←
  GetValue(lRef)`; `rRef ← Evaluation(RHS)`; `rVal ← GetValue(rRef)`; `r ←
  ApplyStringOrNumericBinaryOperator(lVal, opText, rVal)`; `PutValue(lRef,
  r)`. The base and (for computed access) the key are evaluated exactly once,
  via `sec-property-accessors-runtime-semantics-evaluation`'s
  `EvaluatePropertyAccessWithExpressionKey`, which returns a Reference Record
  holding the *raw* key value. Per `sec-getvalue`: step
  `step-getvalue-toobject` (`ToObject(V.[[Base]])`, which throws for a
  null/undefined base) runs **before** the later step "if
  `[[ReferencedName]]` is not a property key, set it to `ToPropertyKey(...)`"
  — base-nullish check precedes key coercion, and `GetValue` mutates the
  Reference Record's `[[ReferencedName]]` in place; `sec-putvalue` only calls
  `ToPropertyKey` "if not already a property key", so on the same Reference
  Record it is a no-op. Net effect for `a[k] += v`: the base-nullish check
  runs first; only then does `k`'s coercion (which can run arbitrary user
  code via `toString`/`Symbol.toPrimitive` — specifically the `ToPrimitive`
  step of `ToPropertyKey`, since the subsequent `ToString` is a pure,
  side-effect-free operation on an already-primitive value) run, **exactly
  once**; both the read and the write then use the same base and the same
  coerced key. The tree-walker's own compound-member arm (`eval.rs:3090-3103`)
  encodes this exact ordering and throws `"Cannot read properties of null"` /
  `"Cannot read properties of undefined"` (no `"(reading ...)"` suffix — a
  different, compound-assignment-specific message from the plain-read arm's
  `member_get`/`member_get_computed` text) before touching the key at all.
  `&&=`/`||=`/`??=` are separate grammar productions
  (`sec-assignment-operators-runtime-semantics-evaluation`, the three
  short-circuit productions) with their own evaluation order and are out of
  scope for this issue (see §7).
- **`||=`/`&&=`/`??=` on a member target**: excluded from this slice — see §7.

## 3. Files to touch

- `src/interpreter/bytecode/op.rs` — two new opcodes for slice A/B
  (`DupN`/`ToPrimitiveKey`), one new opcode for slice C (`Construct`).
- `src/interpreter/bytecode/compiler.rs` — widen the `Expression::Member`
  assign-target arm to accept any `AssignOp` accepted by
  `compound_binary_op` (already rejects `&&=`/`||=`/`??=` via its existing
  catch-all `Err`); add an `Expression::New` arm to `compile_expr`.
- `src/interpreter/bytecode/vm.rs` — dispatch handlers for the new opcodes;
  reuse `member_get`/`member_get_computed`/`member_set`/`member_set_computed`
  and the existing `push_value`/`pop_value` rooting helpers unchanged.
- `src/interpreter/eval.rs` — refactor `eval_new` (lines ~6734-7139): extract
  everything from the `IsConstructor` check onward (proxy trap, bound-function
  delegation, derived-class fast/general paths, base-class path) into a new
  `JsValue`-level helper (e.g. `Interpreter::construct_from_evaluated`,
  signature roughly `(&mut self, callee_val: &JsValue, args: &[JsValue]) ->
  Completion`) that both `eval_new` and the new `Op::Construct` VM handler
  call. This is a pure extraction — no behavior change to `eval_new` itself —
  matching the precedent set by `Op::LoadThis`/`resolve_this_binding`
  (`docs/specs/2026-09-02-bytecode-this-expression-slice.md`): one shared
  implementation instead of two independently-maintained copies.
- `src/interpreter/bytecode/tests.rs` — new parity/GC/bail tests (§4); flip the
  two tests documented in §6 whose expectations change.
- `src/interpreter/perf_counters.rs` — the `OP` per-opcode histogram indexes
  an exhaustive per-opcode name table (per the this-expression slice's own
  "opcode reporting" note); the three new opcodes must be added there or the
  build fails to compile against an exhaustive match, same as the GC walker's
  exhaustive `ObjectKind` match.
- `docs/specs/2026-09-06-bytecode-new-and-compound-member-assignment-slice.md`
  — new design doc following the existing slice-doc convention (Context /
  Specification basis / Approaches considered / Lowering and invariants / GC
  rooting / Validation / Success criteria), covering both constructs as this
  is one PR.

## 4. TDD slices

Order: compound member assignment first (self-contained, no call-bridge
dependency, richest test262 coverage), then `new` (depends on the `eval_new`
extraction). Each slice is red (test written, fails to compile via bytecode
today) → green (compiler/VM change lands) → refactor (fold into the shared
helper where applicable).

**Slice A — compound member assignment, dot form**
1. Red: `bytecode/tests.rs` — `o.x += 1` (plain object) round-trips through
   `--bytecode` (`bytecode_chunks_executed >= 1`) and matches the tree-walker
   result. Currently bails.
2. Green: add `Op::DupN(count: u8)` (duplicates the top `count` stack values
   in place, via the existing `push_value`/`clone` — no new GC design needed,
   since rooting is keyed by object id and a duplicate id is rooted/unrooted
   independently by the existing `root_stack_value`/`unroot_stack_value`).
   Widen `compiler.rs`'s `Expression::Member(obj, MemberProperty::Dot(name),
   _)` assign-target arm: for `op != Assign`, emit `compile_expr(obj)` →
   `DupN(1)` → `GetProp(idx)` → `compile_expr(value)` →
   `compound_binary_op(op)` → `SetProp(idx)`.
3. Red: `o.x += 1` where `x` is an accessor whose getter/setter run
   user code that calls `$262.gc()` — base must survive
   (`end_to_end_member_chain_base_survives_gc_during_rhs_evaluation` is the
   template at `tests.rs:416`).
4. Green: confirm it passes with no additional code (rooting already handled
   by `DupN` reusing `push_value`); if not, fix the gap this test finds.

**Slice B — compound member assignment, computed form (the typed-array case)**
1. Red: `o[i] += 1` (plain array and a `Float64Array`) round-trips through
   `--bytecode` and matches the tree-walker.
2. Red: `null[i] += 1` / `undefined[i] += 1`, thrown message equal to the
   tree-walker's (parity assertion against the tree-walker's own text, not a
   hardcoded string — the tree-walker's compound-member text is
   `"Cannot read properties of null"` / `"...of undefined"`, with **no**
   `"(reading ...)"` suffix, a different string from the plain-read arm).
   This must be red *before* step 4's opcode exists (today it bails, so it
   trivially "passes" by falling back — assert `bytecode_chunks_executed >= 1`
   too, so a silent fallback can't masquerade as success).
3. Red: `o[{toString(){calls.push('k'); return 'i'}}] += 1` — `calls` must
   have exactly one entry (single key coercion) and the containing object
   must not be null (so this test is orthogonal to step 2's ordering check;
   together they pin both halves of the spec's "nullish-check-then-coerce"
   order). This is the discriminating test the earlier member-access slice's
   rejected "peek-without-pop" option was worried about; test262's
   `S11.13.2_A5.*` series covers the same invariant for identifier targets,
   but the member-target case needs its own custom-`toString`-key test since
   test262 doesn't parametrize base kind here.
4. Red: `x[k] += (someMutator(), 1)` where `someMutator` is a plain function
   call (not a `Comma`/`Sequence`-expression RHS, which may itself bail
   independently and produce a false-red/false-green result for the wrong
   reason) that reassigns the outer `x` binding — the base object used for
   the final write must be the one captured *before* the RHS ran.
5. Green: add `Op::ToPrimitiveKey`. Semantics: peek (do not pop) the operand
   one slot below the top of the operand stack (the base); if it is
   null/undefined, throw the tree-walker's exact compound-assignment message
   from step 2 *without touching the key*; otherwise pop the raw key and
   call `interp.to_primitive(&key, "string")` (`eval.rs:1606` — the
   `ToPrimitive` half of `ToPropertyKey`; the remaining `ToString`/Symbol-passthrough
   half is a pure, side-effect-free operation on an already-primitive value),
   pushing the result back. This performs the only side-effecting part of key
   coercion exactly once, in the spec's required order, and — because a
   `Number` key stays a `JsValue::Number` rather than becoming a stringified
   key — leaves `GetElement`'s existing numeric-index fast path
   (`numeric_index_fast_get`) intact for this shape; no fast-path regression
   to defer, unlike an earlier draft of this plan that coerced all the way to
   a property-key string upfront. Widen the `MemberProperty::Computed`
   assign-target arm: for `op != Assign`, emit `compile_expr(obj)` →
   `compile_expr(key)` → `ToPrimitiveKey` → `DupN(2)` → `GetElement` →
   `compile_expr(value)` → `compound_binary_op(op)` → `SetElement`.
   `GetElement`/`SetElement`'s own internal `to_property_key` calls run
   again on the already-primitive result — idempotent, no further
   observable effect. (Note for the design doc, not a fix required here:
   `member_set_computed`, `vm.rs:155-169`, currently coerces its key *before*
   checking the base for nullish — the reverse of `sec-putvalue`'s order —
   which is a pre-existing quirk of the already-shipped simple-assignment
   `SetElement` path, unrelated to and not exercised incorrectly by this
   slice since by the time our sequence reaches `SetElement` the key is
   already primitive and the base was already proven non-null; flag it, do
   not fix it here, and consider a follow-up issue if it proves observable
   for plain `a[b] = c`.)
6. Red: a `$262.gc()`-forcing getter in the RHS while `[base, key]` sit
   duplicated and pending (adapt
   `end_to_end_member_chain_base_survives_gc_during_rhs_evaluation`).
7. Green: confirm passes; the per-object-id rooting model should already
   cover a duplicated pending id with no new code, since `DupN` pushes via the
   ordinary `push_value` path.
8. Refactor: flip `compound_assign_on_member_bails_to_unsupported`
   (`tests.rs:1023-1032`) from asserting a bail to asserting a compiled,
   value-correct result.

**Slice C — `new` expression, no-arg / simple-arg case**
1. Red: `new Foo()` and `new Foo(1, 2)` for a plain user constructor,
   round-tripped through `--bytecode`, matching tree-walker output
   (constructed instance's own properties, prototype chain). Compile it
   inside a wrapping function (`function g(){ return new Foo(); }`, asserting
   on `g`'s own chunk) rather than at script top level, since
   `Statement::FunctionDeclaration` has no `compile_statement` arm — a script
   containing `function Foo(){}` bails on the declaration regardless of `new`
   support, which would produce a misleading red result for the wrong reason.
2a. Refactor-only (no new behavior): extract `eval_new`'s
   post-argument-evaluation logic (`IsConstructor` check, proxy trap,
   bound-function delegation, derived/base construction — everything from
   `eval.rs:6754` through the function's end) into
   `Interpreter::construct_from_evaluated(&mut self, callee_val: &JsValue,
   args: &[JsValue]) -> Completion`. The helper opens its **own**
   `gc_root_frame`/roots `callee_val` and each element of `args` at entry
   (the existing early `self.gc_unroot_frame(gc_frame)` calls throughout the
   extracted body stay correct unmodified, since they now unroot the
   helper's own frame); `eval_new` keeps its existing, separate
   root/unroot pair around callee/argument *evaluation* only, then calls the
   helper (double-rooting the same object ids briefly is harmless — rooting is
   by id, not by frame ownership). Run the full `cargo test --release` and
   the full default (non-`--bytecode`) `scripts/run-test262.py` now, before
   writing any opcode/compiler code, to confirm the extraction is
   byte-for-byte behavior-preserving — this touches the tree-walker's own
   100%-passing hot path and any divergence here is strictly this slice's
   fault, not pre-existing.
2b. Green (new behavior): add `Op::Construct(argc: u16, site_id: u32)`;
   `compiler.rs` gains an `Expression::New(callee, args, site_id)` arm: bail
   with `Unsupported("spread constructor argument")` if any arg is
   `Expression::Spread`; otherwise `compile_expr(callee)`, then each arg
   left-to-right, then `Construct argc site_id`. No `LoadCalleeName`-style
   receiver capture is needed — `Construct` never reads a `this` value from
   the callee reference, unlike `Call`. The VM handler must mirror `Op::Call`'s
   rooting discipline exactly, not `pop_value`: pop `argc` args and the callee
   off the stack *without* unrooting (mirroring `take_call_operands`, which
   leaves the popped values' ids in `gc_bytecode_roots` until the call
   completes), call `interp.construct_from_evaluated(&callee, &args)`, then
   unroot all of them (mirroring `release_call_operands`) before pushing the
   result. Using `pop_value` here instead would unroot the callee/args before
   `construct_from_evaluated` runs, reopening exactly the GC hazard the
   direct-call slice's rooting design closed for `Call`.
3. Red: `new (class extends Base {})()` (derived constructor, default
   constructor and explicit-`super()` forms) and the not-a-constructor
   `TypeError` case (`new (() => {})()`, `new (42)()`) — message text must
   match the tree-walker's exactly (parity assertion, not a hardcoded
   string).
4. Green: confirmed by construction via the shared helper (no new logic).
5. Red: `new Ctor($262.gc() ?? "arg1", ...)`-shaped GC-hazard test — callee
   pending while a later argument forces a collection (adapt
   `pending_argument_survives_gc_during_later_call_argument`, `tests.rs:1708`,
   and `pending_with_getter_callee_survives_gc_during_argument`, `tests.rs:1726`,
   to a `new` callee instead of a call callee).
6. Green: confirm the `take_call_operands`/`release_call_operands`-style
   rooting from step 2b already covers it (same mechanism `Op::Call` uses);
   fix if not.
7. Refactor: `end_to_end_constructor_with_empty_body_returns_this`
   (`tests.rs:126-139`) currently proves only that the *constructor's own
   body* compiles, while `new f()` itself and its containing script fall back
   to the tree-walker (`f`'s declaration is a top-level
   `Statement::FunctionDeclaration`, itself uncompiled, quite apart from
   `new`). Add a sibling test using the same wrapping-function shape as step 1
   (`function g(){ return new f(); }`) so the assertion is actually pinned on
   `new` compiling, and leave the original test's script-level assertion
   describing the pre-existing fallback as-is.

## 5. Test surface

- **`bytecode/tests.rs`** (engine-internal parity/GC/bail suite, in-module —
  this repo has no separate `tests/bytecode_*.rs`): all tests in §4, following
  the existing `eval_with_mode`/`bytecode_chunks_executed` pattern that proves
  the bytecode path was actually taken, not silently bypassed.
- **test262, run both in default and `--bytecode` mode**
  (`uv run python scripts/run-test262.py --bytecode <dir>` and without the
  flag):
  - `test262/test/language/expressions/compound-assignment/` (includes the
    `S11.13.2_A5.*` single-evaluation-of-base series, `S11.13.2_A6.*`, and
    `S11.13.2_A7.*` operand-order series — these exercise identifier targets
    directly but the same engine code paths `apply_compound_assign` and
    `compound_binary_op` are shared with the member-target case).
  - `test262/test/language/expressions/assignment/` (simple-assignment
    regression check — must stay green since the `Expression::Assign` arm's
    identifier/member split is being edited).
  - `test262/test/language/expressions/new/`.
  - `test262/test/built-ins/Reflect/construct/` and
    `test262/test/built-ins/TypedArrayConstructors/ctors/*` (both exercise
    `Construct`/`IsConstructor` and, for typed arrays, are the direct
    motivating shape from the issue, e.g. `new Float64Array(31)`).
- **No new test262-extra needed**: both constructs' full JS-observable
  semantics (single key coercion, evaluation order, `IsConstructor`,
  prototype wiring, derived-class `this`-TDZ) are already covered by the
  test262 directories above running through the tree-walker; this issue adds
  a second (`--bytecode`) execution path for already-spec-tested behavior, so
  the correctness gate is "does `--bytecode` produce byte-identical results to
  the tree-walker," which is exactly what `bytecode/tests.rs`'s parity
  pattern checks — not a new spec-compliance surface requiring its own
  test262-style file.
- **Full regression gate**: `cargo test --release`, `./scripts/lint.sh`, and
  the full default-mode `uv run python scripts/run-test262.py` (per
  `test262-pass.txt` baseline behavior — not rewritten by this branch). Prior
  bytecode slice docs (e.g. the this-expression slice) run `--bytecode`
  *targeted* at the directories in this section, not a full-suite
  `--bytecode` pass — a full `--bytecode` run would surface unrelated
  pre-existing bail-driven differences with no established baseline to
  compare against, since `--bytecode` isn't the default mode
  `test262-pass.txt` tracks. Keep this run targeted at the directories listed
  above.
- **Closure criterion for the issue's own motivating benchmark** (not a merge
  gate, but the concrete "did this land" check named in the issue): rebuild
  with `--features perf-counters`, re-run the `tweetnacl-js` `scalarMult.base`
  repro from the issue, and confirm the `BAIL` table no longer attributes
  `expression:New` / `assign target` to `M`/`car25519`/`sel25519`. Do **not**
  plan or claim a wall-clock speedup measurement here — the issue explicitly
  scopes that as unmeasured, deferred to a follow-up checkpoint on #361 after
  this lands. `scripts/libs/tweetnacl-js.sh` already pins tweetnacl-js 1.0.3
  and is runnable via `./scripts/run-library-tests.sh tweetnacl-js` for an
  end-to-end correctness check (not a timing benchmark) if desired.

## 6. Regression risk

- **`eval_new` refactor is the main risk.** It is a 400-line function
  currently serving 100% test262 pass rate; extracting its tail into a
  shared helper must be mechanical (move code, adjust signatures) with no
  logic change, verified by running the full default-mode test262 suite
  *before* writing any bytecode-side code for slice C (§4, Slice C step 2).
  A subtle divergence here (e.g. picking up `construct_with_new_target`'s
  proxy-aware `.prototype` read instead of `eval_new`'s current raw
  `get_property_value` read, as the research noted as an existing minor
  inconsistency between the two pre-existing tree-walker code paths) would
  move `test262-pass.txt` in either direction. Do not "fix" that
  inconsistency as a side effect of this issue — if it surfaces, note it and
  file a separate follow-up issue; this plan calls for byte-identical
  behavior to the *pre-existing* `eval_new`, not for making it agree with
  `construct_with_new_target`.
- **Widening the `Expression::Member` assign-target match arm** touches only
  the `op != Assign` branch; the existing `AssignOp::Assign` branch is
  untouched, so simple member assignment (already working, already tested)
  should be unaffected — but the arm's `match prop` structure is being
  edited, so a full re-run of the existing dot/computed/private member-access
  tests in `bytecode/tests.rs` is the direct regression check.
- **GC rooting**: both new opcodes (`DupN`, `Construct`) must go through the
  existing `push_value`/`pop_value` (object-id-keyed) rooting rather than a
  new mechanism — reusing this is deliberate specifically to avoid
  reintroducing the two-tier rooting-design hazard the member-access slice
  documented (`docs/specs/2026-07-25-bytecode-member-access-slice.md`,
  "GC rooting" section). Confirm via the GC-hazard tests in §4 that duplicated
  pending stack entries and pending `new` callee/args survive a nested
  collection.
- **`BytecodeCacheState` whole-body caching**: since eligibility is
  per-function and permanent (`Ineligible` is never retried), a bug that
  makes a *previously-ineligible* function now compile but produce a wrong
  result is strictly worse than today's status quo (silent wrong answers
  under `--bytecode` vs. a safe, slow fallback) — this is why every new-code
  test in §4 asserts both `bytecode_chunks_executed >= 1` (proving compilation
  happened) *and* value/observable-behavior equality with the tree-walker,
  never one without the other.
- **Library/harness exposure**: `--bytecode` is off by default, so
  `scripts/run-library-tests.sh` (decimal.js, big.js, acorn, etc., run without
  `--bytecode`) is not part of this change's regression surface; only a
  targeted `--bytecode` pass of `tweetnacl-js` (optional, per §5) exercises it
  end-to-end on real code.
- **Inline-cache (`CallSiteId`) plumbing**: `Expression::New`'s `CallSiteId` is
  currently unread even by the tree-walker (`_site_id`, `eval.rs:6734`); the
  new `Op::Construct` should carry it through unread too (matching the
  existing forward-compatibility allocation, issue #71) rather than wiring up
  IC now — scope creep into call/construct-site ICs is explicitly out of
  scope (§7).

## 7. Out of scope

- `||=`/`&&=`/`??=` on a member target (short-circuit assignment) — different
  grammar productions with their own evaluation order; not in the issue's
  bail table and not needed by the motivating tweetnacl-js workload.
- `o[i]++`/`o.x++` (`Expression::Update` on a member target) — same
  `DupN`-style machinery would enable it, but it isn't in the issue's bail
  table (`compiler.rs:319-322` restricts `Update` to `Identifier` targets
  independently); leave `update_on_member_bails_to_unsupported` as-is and
  file a small follow-up issue if wanted.
- Private-field member targets for compound assignment
  (`MemberProperty::Private`) — already explicitly rejected
  (`Unsupported("private field")`) and untouched by this plan; private-field
  compound assignment has different brand-check semantics and its own
  eligibility slice.
- `new` with a spread argument (`new Foo(...args)`) — bails, matching
  `compile_call`'s existing spread restriction; not present in the motivating
  workload.
- `super(...)` calls and `new.target` — separate constructs (`Expression::Super`,
  `Expression::NewTarget`) with their own compiler arms to add; not touched
  here even though `construct_from_evaluated` is adjacent code.
- Wiring `CallSiteId`/inline-cache state for `Construct` sites — deferred to
  the same future IC-integration work already deferred for `Op::Call`
  (`docs/specs/2026-07-26-bytecode-direct-call-slice.md`, approach 3).
- Measuring or claiming a `#361` wall-clock speedup — out of scope per the
  issue's own "Scope note"; only the compile-eligibility closure (bail-table
  disappearance) is this issue's deliverable.
- Rewriting `test262-pass.txt` — not performed on this branch per repository
  convention; baseline changes are a `main`-branch operation.
