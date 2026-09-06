# Bytecode `new` expression and compound member assignment slice

## Context

Issue #361 measured the bytecode VM against tweetnacl-js's curve25519/Ed25519
field arithmetic and found no difference (1.00-1.07x, inside run-to-run noise).
A `perf-counters` build explained why: the VM never runs the hot path at all.
Three functions carry 96% of the tree-walker's work in a `scalarMult.base`
run — `M` (the GF(2^255-19) multiply), `car25519` (carry propagation), and
`sel25519` (constant-time swap) — and all three bail on exactly two
constructs: `var t = new Float64Array(31)` (`expression:New`, no
`compile_expr` arm existed at all) and `o[i] += 65536`-shaped loops (`assign
target`, `Expression::Member` as an assignment target was only accepted under
`AssignOp::Assign`). Because bytecode eligibility is whole-`Body`, one
occurrence of either shape anywhere in a function permanently bails that
function's entire body to the tree-walker
(`BytecodeCacheState::Ineligible`, cached forever per function object). This
issue (#603) closes both gaps. It is a precondition for measuring whether the
VM helps typed-array field arithmetic, not a demonstration that it does — see
`docs/perf/2026-09-05/tweetnacl-bytecode-null-result.md` and the issue's own
"Scope note".

## Specification basis

**`new`** (`sec-new-operator`, `sec-evaluatenew`): evaluate the callee,
`GetValue`, evaluate arguments left-to-right, then check `IsConstructor`
(`TypeError` if false — after argument evaluation, not before), then
`sec-construct`. JSSE's tree-walker already implements this in `eval_new`.

**Compound member assignment**
(`sec-assignment-operators-runtime-semantics-evaluation`, the
`AssignmentExpression : LeftHandSideExpression AssignmentOperator
AssignmentExpression` production): `lRef ← Evaluation(LHS)`; `lVal ←
GetValue(lRef)`; `rRef ← Evaluation(RHS)`; `rVal ← GetValue(rRef)`; `r ←
ApplyStringOrNumericBinaryOperator(lVal, opText, rVal)`; `PutValue(lRef, r)`.
The base and (for computed access) the key are each evaluated exactly once.
Per `sec-getvalue`, the base-nullish check (`ToObject` on the base, which
throws for null/undefined) runs *before* `ToPropertyKey`'s coercion — so for
`a[k] += v`, the base-nullish check runs first, and only then does `k`'s
coercion (which can run arbitrary user code via `toString`/
`Symbol.toPrimitive`) run, exactly once. The tree-walker's own compound-member
arm (`eval.rs`, the `op != AssignOp::Assign` branch of `eval_assign`) encodes
this exact ordering and throws `"Cannot read properties of null"` /
`"...of undefined"` — no `"(reading ...)"` suffix, a different message from
the plain-read arm's text — before touching the key at all.
`&&=`/`||=`/`??=` are separate grammar productions with their own evaluation
order and are out of scope (see "Out of scope" below).

## Approaches considered

1. **Add a dedicated `DupN`/`ToPrimitiveKey` opcode pair for compound member
   assignment, and a dedicated `Construct` opcode reusing a shared
   `construct_from_evaluated` helper for `new`.** Selected. Matches the
   project's established pattern of adding narrow, composable opcodes that
   reuse existing MOP entry points (`property.rs`, `eval.rs`) rather than
   re-deriving semantics in the VM — the same shape as the member-access slice
   (`docs/specs/2026-07-25-bytecode-member-access-slice.md`) and the `this`
   expression slice (`docs/specs/2026-09-02-bytecode-this-expression-slice.md`).
2. **A parallel `PropRef`-style ref stack for compound member assignment**
   (analogous to the existing `IdentifierRef`/`ResolveName`/`LoadResolvedName`
   machinery), explicitly rejected by the member-access slice as unwarranted
   mechanical cost. Rejected again here for the same reason: `DupN` reuses the
   existing object-id-keyed rooting model with zero new GC design, at the cost
   of one cheap `JsValue::clone` per duplicated stack slot.
3. **Coerce the computed key all the way to a property-key string upfront**,
   before duplicating base/key. Rejected: `GetElement`'s numeric-index fast
   path (`numeric_index_fast_get`) specifically wants a `JsValue::Number` key,
   not a stringified one. `ToPrimitiveKey` only performs `ToPropertyKey`'s
   side-effecting `ToPrimitive` step — the subsequent `ToString`/Symbol
   passthrough is pure and side-effect-free on an already-primitive value, so
   `GetElement`/`SetElement`'s own internal `to_property_key` calls re-running
   on the primitive result are idempotent, no-op-observable, and the fast path
   stays intact.
4. **Duplicate `eval_new`'s post-argument-evaluation logic into the VM
   directly, rather than extracting a shared helper.** Rejected: `eval_new` is
   a ~400-line function serving 100% test262 pass rate (proxy `construct`
   trap, bound-function delegation, derived/base construction paths, instance
   field initialization). Two independently-maintained copies would
   immediately drift, exactly the risk the `this`-expression slice's
   `resolve_this_binding` extraction was designed to avoid.
5. **`o[i]++`/`o.x++` (`Expression::Update` on a member target) and
   `||=`/`&&=`/`??=` on a member target in the same change**, since the same
   `DupN`-style machinery would enable the former. Rejected — neither is in
   the issue's bail table or the motivating tweetnacl-js workload; left as
   possible small follow-ups.

## Lowering and invariants

### Compound member assignment

`compiler.rs`'s `Expression::Assign` match widened its `Expression::Member`
arm to accept any `AssignOp`, branching internally by property kind and
op:

- **Dot, `op != Assign`**: `compile_expr(obj)` → `DupN(1)` → `GetProp(idx)` →
  `compile_expr(value)` → `compound_binary_op(op)` → `SetProp(idx)`. The base
  is evaluated once and duplicated; `GetProp`'s pop and `SetProp`'s pop each
  consume their own copy.
- **Computed, `op != Assign`**: `compile_expr(obj)` → `compile_expr(key)` →
  `ToPrimitiveKey` → `DupN(2)` → `GetElement` → `compile_expr(value)` →
  `compound_binary_op(op)` → `SetElement`. `ToPrimitiveKey` peeks the base
  (one slot below the key, without popping it) and throws the tree-walker's
  exact nullish-base message *before* touching the key if the base is
  null/undefined; otherwise it pops the raw key, runs `to_primitive(&key,
  "string")` (the side-effecting half of `ToPropertyKey`), and pushes the
  result back — this performs the coercion exactly once, in the spec's
  required order.
- **`op == Assign`** (both kinds): unchanged from the existing member-access
  slice.
- **Private field target, or `&&=`/`||=`/`??=` on any member target**:
  unchanged — still `Unsupported`, bailing the whole Body.

`Op::DupN(count: u8)` duplicates the top `count` operand-stack values in
place (via `JsValue::clone` + the existing `push_value` path) — no new GC
design, since rooting is keyed by object id and a duplicate id is
rooted/unrooted independently by `push_value`/`unroot_stack_value`'s existing
`rposition`-based removal.

### `new` expressions

`eval_new`'s post-argument-evaluation logic (the `IsConstructor` check, proxy
`construct` trap, bound-function delegation, derived/base construction paths)
was extracted, unmodified, into `Interpreter::construct_from_evaluated(&mut
self, callee_val: &JsValue, args: &[JsValue], env: &EnvRef) -> Completion`.
`eval_new` now evaluates the callee and arguments, then delegates to this
helper; the extraction was verified byte-for-byte behavior-preserving with
the full `cargo test --release` suite and a full default-mode test262 run
*before* any bytecode-side code was written (100% pass, 0 regressions).

`compiler.rs` gained an `Expression::New(callee, args, site_id)` arm: bail
with `Unsupported("spread constructor argument")` if any argument is
`Expression::Spread` (matching `compile_call`'s existing spread restriction);
otherwise `compile_expr(callee)`, then each argument left-to-right, then
`Construct argc site_id`. Unlike `compile_call`, the callee is *any*
expression, not restricted to `Identifier` — `new (getCtor())()` is valid
syntax, and `Construct` never needs to capture a `this` receiver from the
callee reference the way `Call` does (no `LoadCalleeName`-style step).

The `Op::Construct` VM handler mirrors `Op::Call`'s rooting discipline
exactly: it pops `argc` arguments and the callee off the stack *without*
unrooting (`take_construct_operands`, analogous to `take_call_operands`,
leaving their ids in `gc_bytecode_roots`), calls
`interp.construct_from_evaluated(&callee, &args, env)`, then unroots all of
them (`release_construct_operands`) before pushing the result. Using
`pop_value` instead would unroot the callee/args before
`construct_from_evaluated` runs, reopening exactly the GC hazard the
direct-call slice's rooting design closed for `Call`.

Three opcodes were added (`op.rs`, values 53-55): `DupN`, `ToPrimitiveKey`,
`Construct`. `CallSiteId` is threaded through `Construct` but unread, matching
the tree-walker's own pre-existing forward-compatible allocation (`eval_new`'s
`_site_id`, issue #71) — wiring up an inline cache for construct sites is out
of scope (see below).

## GC rooting

Both new opcodes reuse the existing object-id-keyed rooting model
(`push_value`/`pop_value`/`root_stack_value`/`unroot_stack_value`) rather than
introducing a second rooting mechanism — deliberately, to avoid reintroducing
the two-tier rooting-design hazard the member-access slice's own GC-rooting
section documents. `DupN` pushes its clones through the ordinary
`push_value` path, so a duplicated pending id is rooted/unrooted exactly like
any other operand-stack value. `ToPrimitiveKey`'s side-effecting
`to_primitive` call runs inside a `root_operand_stack` scope (roots the
*entire* operand stack, not just this opcode's own operands — the same
mechanism the member-access slice's GC-rooting section motivates for chained
member expressions), so a duplicated base/key pending from an earlier `DupN`
survives a nested collection triggered by the key's own `toString`/
`Symbol.toPrimitive`.

`construct_from_evaluated` opens its *own* `gc_root_frame` and roots
`callee_val` and each argument at entry (on `gc_temp_roots`, the tree-walker's
pre-existing root list — a mechanism orthogonal to the VM's
`gc_bytecode_roots`). `eval_new` keeps its own, separate root/unroot pair
around callee/argument *evaluation* only, then calls the helper;
double-rooting the same object ids briefly is harmless, since rooting is by
id, not by frame ownership. The VM's `Op::Construct` handler additionally
keeps the callee/args rooted in `gc_bytecode_roots` for the complete nested
construction (mirroring `Op::Call`), since a constructor body or field
initializer can run arbitrary JS and hit any number of nested safepoints
before returning.

## Validation

`bytecode/tests.rs` gained parity/GC-hazard/bail-flip tests for both
constructs, following the existing `eval_with_mode`/`bytecode_chunks_executed`
pattern that proves the bytecode path was actually taken, not silently
bypassed: dot and computed compound assignment on plain objects/arrays and a
`Float64Array` (the direct motivating shape); null/undefined-base message
parity against the tree-walker's exact text; a custom-`toString`-key test
pinning single key coercion; a base-capture-before-RHS-evaluation test; `new`
with no/some arguments, a derived class (default and explicit-`super()`
constructors), and non-constructor `TypeError` message parity; and GC-hazard
tests adapting the existing call-argument-survives-GC patterns to a `new`
callee/argument. Several of these tests use a poison-pill lexical
declaration (`let unused = 0;`) in an auxiliary helper function specifically
to prevent that helper from independently compiling to bytecode on its own
already-supported merits — without it, `bytecode_chunks_executed >= 1` could
be satisfied by the helper alone even if the construct genuinely under test
still bailed, masking a false green.

`compound_assign_on_member_bails_to_unsupported` flipped from asserting a
bail to asserting a compiled, value-correct result, per the eligibility
change; the plain-`Assign` dot/computed tests from the member-access slice
are unchanged and still pass, confirming the widened match arm didn't
regress simple assignment.

Targeted test262 coverage (both default and `--bytecode` modes):
`language/expressions/compound-assignment/` (including the
`S11.13.2_A5`/`A6`/`A7` single-evaluation/operand-order series),
`language/expressions/assignment/` (regression check for the edited match
arm), `language/expressions/new/`, `built-ins/Reflect/construct/`, and
`built-ins/TypedArrayConstructors/ctors/*` (the direct motivating shape). The
full default-mode test262 suite is the regression gate; a full `--bytecode`
run is not, since `--bytecode` isn't the mode `test262-pass.txt` tracks and
would surface unrelated pre-existing bail-driven differences with no
established baseline.

## Success criteria

Rebuilding with `--features perf-counters` and re-running the `tweetnacl-js`
`scalarMult.base` repro from the issue should no longer attribute
`expression:New` or `assign target` to `M`/`car25519`/`sel25519` in the
`BAIL` table. This closes the two gaps identified by #361; it does not claim
or measure a wall-clock speedup, which the issue explicitly scopes as
unmeasured and deferred to a follow-up checkpoint.

## Out of scope

- `||=`/`&&=`/`??=` on a member target (separate grammar productions).
- `o[i]++`/`o.x++` (`Expression::Update` on a member target) — same
  `DupN`-style machinery would enable it, but not in the issue's bail table.
- Private-field member targets for compound assignment — already rejected,
  untouched by this slice.
- `new` with a spread argument — bails, matching `compile_call`.
- `super(...)` calls and `new.target` — separate constructs with their own
  compiler arms to add.
- Wiring `CallSiteId`/inline-cache state for `Construct` sites — deferred to
  the same future IC-integration work already deferred for `Op::Call`.
- Measuring or claiming a #361 wall-clock speedup.
