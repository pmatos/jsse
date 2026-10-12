# Plan: fuse effect-context assignment/update ops and inline number-number binary ops in the bytecode VM (#903)

## Problem restated

The bytecode VM dispatches more opcodes than the work needs. A statement-context assignment `x = e;` compiles to `<ref/load> e Store* Pop`; a statement-context `i++` to `UpdateLocal Pop`; `a < b` in a branch to `Lt JumpIfFalse`; a constant operand to `LoadConst` + op; and every binary op is mapped `Op -> BinaryOp` in `vm.rs` and then re-matched inside `eval_binary` (`eval.rs:2012`). `push_value`/`pop_value` are not forced inline. HEAD already resolves function-frame names through `LoadLocal`/`StoreLocal`/`UpdateLocal` (#898), so the issue's "49 ops/iteration" is the pre-#898 sequence: at HEAD the `bench_opmix.js` `arith` loop body is **43 counted ops (+6 `Safepoint` dispatches that `record_op` excludes)**. Goal: <= 25 counted ops per `arith` iteration (-42%), `bench_opmix` arith <= 2.5x boa, no observable change (evaluation order, errors, `with`/global-object bindings, GC rooting).

## Assumptions

- The acceptance line "arith ops/iteration <= 25 on `bench_numeric_intarith`" is a naming slip: 25 is only reachable for the opmix `arith` loop (6 statements; 43 ops at HEAD, 24 after this plan). `bench_numeric_intarith`'s loop body is ~132 counted ops at HEAD and ~88 after (-33%, inside the "25-40% fewer dispatches" criterion). The PR reports both; the gated number is the `arith` body (best guess). Slice 0 re-measures the HEAD baseline (expected 43, not 49) before anything is trusted.
- The issue's `StoreNamePop` maps to two opcodes: `StoreLocalPop` (frame-local names) and `StoreResolvedNamePop` (by-name, after `ResolveName`). The issue's `IncName` is a *discard bit* on the existing `UpdateName`/`UpdateLocal` mode byte, not new opcodes (best guess).
- "Effect context" = the value is discarded: `Statement::Expression` in `CompileGoal::Function` (Script goal needs the value for `SetCompletion`, so Script expression statements are **not** fused — follow-up), `for` init expression, `for` update, non-final comma elements, and `var x = e` initialisers (these three are fused under both goals).
- "Small ints as immediates" is delivered by `BinConst` (constant operand folded into the op, no `LoadConst` dispatch). A standalone `LoadSmallInt` removes no dispatch, so it is dropped (follow-up).
- The <= 2.5x boa wall-clock target is a measured outcome, not a guarantee: dispatch count falls ~44% but the per-local `RefCell` borrow + `BindingMap::entry_at` cost remains. If missed, the PR reports measured numbers and files the frame-register follow-up rather than widening scope. `boa` may be absent from the build host; record that if so (#899 did).
- Spec clause numbers below are from memory; the ecmarkup anchors were verified present in `spec/spec.html` (spec submodule initialised in this workspace). Re-run `git submodule update --init --depth 1 spec test262` in a fresh workspace.

## Spec basis

Pure optimisation; the ECMAScript semantics must stay exactly these (anchors verified in `spec/spec.html`):

- 13.15.2 Runtime Semantics: Evaluation of AssignmentExpression (`sec-assignment-operators-runtime-semantics-evaluation`): `lref` is evaluated **before** the RHS; `GetValue(lref)` for compound ops happens before the RHS; `PutValue(lref, rval)` happens after, and is where strict-mode unresolvable references throw.
- 13.4.2 / 13.4.4 Postfix/Prefix Increment/Decrement Evaluation (`sec-postfix-increment-operator-runtime-semantics-evaluation`, `sec-prefix-increment-operator-runtime-semantics-evaluation`): `ToNumeric(GetValue(lhs))`, then `PutValue`; the discarded result value is unobservable, so effect context may drop it.
- 14.5.1 Expression Statement Evaluation (`sec-expression-statement-runtime-semantics-evaluation`): the value matters only for completion values (Script goal).
- 13.16.1 Comma Operator Evaluation (`sec-comma-operator-runtime-semantics-evaluation`): non-final operands are evaluated for effect.
- 6.2.5.5 GetValue / 6.2.5.6 PutValue (`sec-getvalue`, `sec-putvalue`), 9.4.2 ResolveBinding (`sec-resolvebinding`): reference creation vs. value access split.
- 9.1.1.2.x Object Environment Record and 9.1.1.4.x Global Environment Record `SetMutableBinding` (`sec-object-environment-records-setmutablebinding-n-v-s`, `sec-global-environment-records-setmutablebinding-n-v-s`): a property deleted between ResolveBinding and PutValue is re-created in sloppy code and throws ReferenceError in strict code (test262 `S11.13.1_A5/A6/A7`).
- 13.15.3 ApplyStringOrNumericBinaryOperator (`sec-applystringornumericbinaryoperator`); 6.1.6.1.x Number::add/subtract/multiply/divide/remainder/exponentiate/leftShift/signedRightShift/unsignedRightShift/bitwiseAND/OR/XOR/equal/lessThan (`sec-numeric-types-number-*`); 7.1.6 ToInt32 / 7.1.7 ToUint32 (`sec-toint32`, `sec-touint32`): the inline fast path computes exactly what the `number_ops::*` functions already compute for `eval_binary`'s existing number fast path.
- 13.10 Relational Operators (`sec-relational-operators`) + 7.2.13 IsLessThan (`sec-islessthan`, LeftFirst=false for `>`/`<=`), 7.2.14 IsLooselyEqual (`sec-islooselyequal`), 7.2.15 IsStrictlyEqual (`sec-isstrictlyequal`): unchanged; the fused compare+branch must produce the same boolean and the same ToPrimitive order on the slow path.
- 14.6.2 `if` (`sec-if-statement-runtime-semantics-evaluation`), do-while/while loop evaluation (`sec-runtime-semantics-dowhileloopevaluation`, `sec-runtime-semantics-whileloopevaluation`), ForBodyEvaluation (`sec-forbodyevaluation`), conditional operator (`sec-conditional-operator-runtime-semantics-evaluation`), ToBoolean (`sec-toboolean`): a comparison result is always a Boolean, so jump-if-false on it is identical to `JumpIfFalse` (no `[[IsHTMLDDA]]` case).

## Files to touch

| File | Role | Lines |
|---|---|---|
| `src/interpreter/bytecode/op.rs` | new opcodes + `from_u8` | `Op` ends at `UpdateLocal = 61` (`:79`); `from_u8` `:83-149` |
| `src/interpreter/bytecode/compiler.rs` | effect-context compilation, fusion emitters, branch fusion | Assign `:318-419`, Update `:420-442`, Sequence `:304`, Binary `:484`, `compile_var_declaration` `:602`, `Statement::Expression` `:687`, If `:704`, loops `:839-980` |
| `src/interpreter/bytecode/vm.rs` | fused handlers, number fast path, `#[inline(always)]` | helpers `:10-127`, `StoreLocal` `:381`, `UpdateLocal` `:426`, `StoreResolvedName` `:530`, `UpdateName` `:561`, binary arm `:862-922`, `JumpIfFalse` `:956` |
| `src/interpreter/bytecode/chunk.rs` | `Constant::Number` read by `BinConst` | `:5-19` |
| `src/interpreter/perf_counters.rs` | `OP_SLOTS = 64` (`:19`) and `assert!(Op::UpdateLocal < OP_SLOTS)` (`:33`) must cover the new ops | `:19`, `:33`, `:284` |
| `src/interpreter/bytecode/tests.rs` | op-sequence + parity tests; `decode_ops` (`:2680`) generalised | `:2680-2785` |
| `tests/perf_counters_env_hashing.rs` | `frame_locals_resolve_through_slots` asserts `StoreLocal` is present (`:100`); after fusion that op is `StoreLocalPop` | `:97-110` |
| `tests/perf_counters_op_budget.rs` [new] | gate: counted ops per `arith` iteration <= 25 (CI already runs `--features perf-counters` tests, `.github/workflows/ci.yml:60`) | |
| `test262-extra/*.js` [new] | spec-visible regressions (list below) | |
| `docs/perf/2026-10-12/903-fused-effect-context-ops.md` [new] | measurements, follows `899-call-fixed-work.md` | |
| `docs/adr/2026-10-12-1200-bytecode-effect-context-fusion.md` [new] | decision record (`docs/adr/README.md` has naming rules only, no index to update) | |
| `CONTEXT.md` | vocabulary: *Effect context*, *Fused op*; next to the *Slot hint* entry (`:56`) | |
| reference only | `eval.rs`: `eval_binary` `:2012`, `eval_identifier_update` `:2421`, `is_primitive_string_concat` `:10399`; `types.rs`: `number_ops` `:1028`; `benchmarks/scripts/bench_opmix.js` | |

## Design (decisions)

New opcodes (additive; existing ops keep their meaning so the compiler still emits them where fusion does not apply):

| Op | Operands | Semantics |
|---|---|---|
| `StoreLocalPop` | name u16, slot u16 | `StoreLocal`, then pop the value. The value stays on the operand stack (rooted) *through* the store and is popped after it: the slow path (global accessor setter, proxy trap) can GC. |
| `StoreResolvedNamePop` | name u16 | `StoreResolvedName` (pops the ref), then pop the value, same ordering. `ResolveName` is still emitted before the RHS. |
| `BinConst` | binop u8, const u16 | `[.., l]` -> `[.., l op K]`, `K` a `Constant::Number`; emitted only when the RHS is a number literal and `op` is one of the 20 ops `binary_op` supports. |
| `CmpJumpIfFalse` | cmp u8, off i16 | pops r, l; evaluates `l cmp r` (`Lt Gt LtEq GtEq Eq NotEq StrictEq StrictNotEq`); jumps when false. The patch site is the i16 (the cmp byte precedes it) so `patch_jump` works unchanged. |
| `LocalBinConst` | name u16, slot u16, binop u8, const u16 | exactly `LoadLocal` + `BinConst`. Slice 5 (required). |

Update discard: bit `0b100` of the `UpdateName`/`UpdateLocal` mode byte means "do not push the result". `update_mode` returns `(UpdateOp, prefix, discard)`; effect-context pre/post are identical (result unobservable).

Number fast path: per-op arms generated by a macro. Each arm peeks the top two stack values, takes the `as_number` pair, pops both (numbers are never rooted, so no unroot), computes via a shared `#[inline(always)] fn number_binop(op, l, r) -> JsValue` built on the same `number_ops::*` calls as `eval_binary`, and pushes. Everything else calls a non-inlined slow helper `binary_slow(interp, op: BinaryOp, l: JsValue, r: JsValue) -> Completion` that takes **both operands already popped** and keeps today's discipline verbatim: `is_primitive_string_concat` check for `Add`, then `eval_binary`, with `l` and `r` still rooted in `gc_bytecode_roots` across the call and unrooted afterwards (r, then l: LIFO). `BinConst` pops only `l` (the constant is never on the stack or rooted) and calls the same helper with `r = K`; it unroots only `l`. Add `#[inline(always)]` to `push_value`, `pop_value`, `root_stack_value`, `unroot_stack_value`, `decode_u16`, `decode_i16`.

`LocalBinConst` slow/miss path must behave exactly like `LoadLocal` then `BinConst`: load via the shared `load_local_value` (hint hit, else by-name `load_name_value` fallback), and when the value is not a number, `push_value` it (rooting it), pop it, and run the slow helper. The loaded local can be an object whose `valueOf` reassigns the captured local through a closure and calls `$262.gc()`, so the loaded value must stay rooted for the whole `eval_binary`.

`compile_effect(expr)`: new `Compiler` method, "compile for side effects, leave stack unchanged". `Assign` to an identifier (plain and arithmetic compound) -> the existing lowering with `StoreLocalPop`/`StoreResolvedNamePop`; `Update` of an identifier -> discard bit; `Sequence`/`Comma` -> `compile_effect` on every element; anything else -> `compile_expr` + `Pop`. Callers: `Statement::Expression` (Function goal only), `for` init expression, `for` update, `compile_var_declaration`, non-final comma elements. `compile_cond_jump(test) -> patch site`: if `test` is `Expression::Binary(cmp, l, r)` with a supported compare -> `l`, `r`, `CmpJumpIfFalse`; else `compile_expr` + `JumpIfFalse`. Used by `if`, `while`, `for`, `do-while`, `Conditional`.

## Steps (TDD slices)

Each slice: tests first, run red, implement, run green, `cargo test --release bytecode` before the next slice. Bound build parallelism: `cargo build --release -j4`, explicit long Bash timeout.

### Slice 0 — guards and baseline evidence (no production code)
- Write all `test262-extra/` files listed under Test surface now. They pass on HEAD (they pin current behaviour) and exist to catch regressions in slices 1–5. Run them with `uv run python scripts/run-test262.py test262-extra/` (default and `--no-bytecode`) and under `JSSE_GC_STRESS=7`.
- Build a `perf-counters` binary (`cargo build --release --features perf-counters -j4 --target-dir "$TMPDIR/pc"`, never the timing binary). Record `vm_ops`/iteration and the `OP` histogram for `bench_opmix.js` arith (expect 43), `bench_numeric_intarith.js` (~132), `bench_numeric_bail_none.js` into the perf note. Snapshot the plain release binary to `$TMPDIR/jsse-base` for before/after timings.

### Slice 1 — generalise `decode_ops`, then effect-context stores: `StoreLocalPop`, `StoreResolvedNamePop`
- First: make `decode_ops` (`tests.rs:2680`) decode **every** opcode through an operand-width table (it currently panics on any op outside its list; the free-name test below already needs `LoadCalleeName`/`Call`, and later slices need `Jump`, `JumpIfFalse`, `BitOr`, `BitAnd`, `Shr`, `Lt`). Keep existing tests green.
- Red: `function_body_statement_assignment_to_local_fuses_store_and_pop` — compile `var c = a + 1; c = c + 2;` via `compile_function_body`; assert `StoreLocalPop` twice and **no** `Pop`. `..._to_free_name_keeps_resolve_before_rhs`: `g = f();` -> `[ResolveName g, LoadCalleeName f, Call, StoreResolvedNamePop g]`. `..._value_context_keeps_unfused_store`: `var y = (a = 1);` still has `StoreLocal`. `script_goal_expression_statement_is_not_fused`: `x = 1;` via `compile_script_body` keeps `SetCompletion`. Update `tests/perf_counters_env_hashing.rs` (`StoreLocal` -> `StoreLocalPop`; also assert no `StoreResolvedNamePop`).
- Green: opcodes in `op.rs` (`from_u8` too); `perf_counters.rs`: `OP_SLOTS` -> 128 and the const assert to the highest op (the enum passes 64 here); `compile_effect` + callers; VM handlers sharing store helpers with `StoreLocal`/`StoreResolvedName` (pop *after* the store).
- Parity tests (tree-walker vs bytecode, existing `assert_parity_number`): compound assignment statements `a += b; a -= b; a <<= 1`, `var` initialisers, `for` init/update `i = i + 2`, comma statement `a = 1, b = 2, a + b`.

### Slice 2 — effect-context update: discard bit
- Red: `statement_update_of_local_emits_no_pop` (`i++;`, `--i;`, `for(...;...;i++)`: no `Pop` after `UpdateLocal`); `value_context_update_still_pushes` (`var j = i++;`). Parity: string/BigInt/object operands (`var s="5"; s++`, `var b=1n; b++; b--`, `valueOf` called once), const-binding `TypeError`, strict unresolvable `ReferenceError`.
- Green: `update_mode` third element; handlers skip `push_value` when set (fast path and `eval_identifier_update` fallback); compiler sets it from `compile_effect`.

### Slice 3 — number-number fast path in the dispatch loop + inlining
- Guard: parity table test driving every one of the 20 binary ops over `{0, -0, 1, -1, 2^31, 2^32, 1e21, NaN, Infinity, 0.5}` pairs, bytecode == tree-walker (`Object.is`). It passes before and after. The dispatch win is not unit-observable here; it is verified by the Slice 0 vs final `perf-counters` counts and callgrind (`eval_binary` entries from the VM on the arith loop ~ 0).
- Green: macro-generated arms + `binary_slow(interp, op, l, r)`; `#[inline(always)]` helpers. `Add`'s `is_primitive_string_concat` stays on the slow path only (a number pair is never a concat).

### Slice 4 — `BinConst` and fused compare+branch
- Red: `binary_with_number_literal_rhs_compiles_to_binconst` (`x | 0`, `x & 65535`, `x >>> 3`); `x + "s"`, `x + y`, `x - -1` stay generic; `comparison_in_branch_compiles_to_cmpjump` for `if`, `while`, `for`, `do-while`, `?:`; `var t = a < b` (value context) stays `Lt`. Parity: NaN branches (`if (NaN < 1)`, `if (!(NaN >= 1))`), mixed string/number, BigInt/number, object `valueOf` order for `<`,`>`,`<=`,`>=`, loose/strict equality branches, jump offsets near the i16 boundary.
- Green: opcodes + decode; `Binary` emission; `compile_cond_jump`; `BinConst` handler reusing `number_binop`/`binary_slow` with `l` only rooted.

### Slice 5 — `LocalBinConst` (required: slices 1–4 reach 28, the gate is 25)
- Red (a): `opmix_arith_loop_body_is_at_most_25_ops` in `bytecode/tests.rs` — parse the whole `function arith(n){...}` declaration (a body with a top-level `return` is a SyntaxError as a Script), take the `FunctionDeclaration` body from the AST, `compile_function_body` with params `[n]`, count non-`Safepoint` ops from loop start to the back `Jump` with the generalised `decode_ops`; assert `<= 25` (HEAD 43; slices 1–4: 28; with this slice: 24).
- Red (b): `tests/perf_counters_op_budget.rs` [new, `#![cfg(feature = "perf-counters")]`, modelled on `perf_counters_env_hashing.rs`]: run the arith function for N and 2N iterations as child processes, assert `(vm_ops(2N) - vm_ops(N)) / N <= 25` (the real runtime metric; `Safepoint` is excluded by `record_op`).
- Red (c): `GC-local-bin-const-captured-local-reassigned-by-valueof.js` (test262-extra): `function f(){ var o = { valueOf() { o = 0; $262.gc(); return 1 } }; function g(){ return o }; return o | 0 }`-shaped test where the loaded local is an object, its `valueOf` reassigns the captured local and forces a collection; result must be correct under `JSSE_GC_STRESS=7`.
- Green: compiler peephole at emission of `Binary(op, Identifier(local), NumberLiteral)` -> `LocalBinConst`; VM handler shares `load_local_value` with `LoadLocal` and the slow path described in Design.

### Slice 6 — ADR, vocabulary, measurements
- `docs/adr/2026-10-12-1200-bytecode-effect-context-fusion.md` (effect context, fusion set, discard bit instead of new ops, Script goal excluded, `OP_SLOTS`), `CONTEXT.md` entries, `docs/perf/2026-10-12/903-fused-effect-context-ops.md` with #899's protocol: median of 7 interleaved `user+sys` runs vs `$TMPDIR/jsse-base`; callgrind on scaled-down copies if `valgrind` exists; `--features perf-counters` only for counts, never for timing.

## Test surface

Targeted test262 (must stay green, no pass-list regressions), then the full run:
- `test262/test/language/expressions/assignment/` (incl. `S11.13.1_A5*`, `A6*`, `A7*`), `.../compound-assignment/`, `.../postfix-increment/`, `.../prefix-increment/`, `.../postfix-decrement/`, `.../prefix-decrement/`
- `test262/test/language/expressions/{addition,subtraction,multiplication,division,modulus,exponentiation,left-shift,right-shift,unsigned-right-shift,bitwise-and,bitwise-or,bitwise-xor,less-than,greater-than,less-than-or-equal,greater-than-or-equal,equals,does-not-equals,strict-equals,strict-does-not-equals,conditional,comma}/`
- `test262/test/language/statements/{for,while,do-while,if,variable,expression}/`, `language/identifier-resolution/`, `language/global-code/`, `annexB/language/`
- Full gate: `uv run python scripts/run-test262.py` (baseline read from `origin/main:test262-pass.txt`; never `--update-baseline`).

New `test262-extra/` files (test262 frontmatter, `esid:` naming the clause, `flags: [noStrict]`/`[onlyStrict]` as needed; all written in Slice 0):
- `bytecode-assignment-statement-reference-before-rhs.js` — `esid: sec-assignment-operators-runtime-semantics-evaluation`. In compiled functions (no `with`/`eval`): `g = h();` where `h` deletes `globalThis.g`: sloppy re-creates the property with the RHS value; strict throws `ReferenceError` *after* `h` ran (side effect observed); undeclared strict target throws only after the RHS runs; same through `var x = ...` and `for(;;g = h())`.
- `bytecode-compound-assignment-statement-reads-before-rhs.js` — `g += h()` / `g -= h()` where `h` rewrites `g`: result uses the value read before `h`; global accessor getter/setter order logged.
- `bytecode-update-statement-discard-semantics.js` — `esid: sec-postfix-increment-operator-runtime-semantics-evaluation`: string/BigInt/`valueOf`/`Symbol` (TypeError) operands, const binding TypeError, strict undeclared ReferenceError, `NaN`/`-0`, value-context `x++` unaffected.
- `bytecode-number-binary-ops-edge-values.js` — `esid: sec-applystringornumericbinaryoperator`: the 20 ops over the edge-value table (`-0`, `NaN`, `±Infinity`, 2^31/2^32, `1e21`, fractional/negative shift counts, `%` signs), `assert.sameValue` (Object.is).
- `bytecode-compare-branch-fusion.js` — `esid: sec-islessthan`: `if/while/for/do-while/?:` with all 8 comparisons over NaN, mixed string/number, BigInt/number, objects with logging `valueOf` (operand order incl. LeftFirst=false for `>`/`<=`), result also used as a value.
- `GC-assignment-statement-pop-keeps-value-rooted.js` — object assigned in effect context to a global accessor whose setter calls `$262.gc()`; guards pop-after-store ordering.
- `GC-local-bin-const-captured-local-reassigned-by-valueof.js` — added in Slice 5 (red c); it must also pass on HEAD.
- Rust unit tests in `bytecode/tests.rs` pin op-sequence shape (not observable ECMAScript behaviour, so they live in crate tests per AGENTS.md); `tests/perf_counters_op_budget.rs` pins the dispatch budget.

Gates before the PR: `cargo test --release` and plain `cargo test` (debug asserts), `cargo test --release --features perf-counters --target-dir target/perf-counters`, `./scripts/lint.sh`, `uv run python scripts/run-test262.py test262-extra/` (default and `--no-bytecode`), `JSSE_GC_STRESS=7` over `test262-extra/` (both modes, `--timeout 300`), `cargo build --profile release-checked -j4` + `--binary target/release-checked/jsse` over `test262-extra/` and a 10% `--sample`, then the full `uv run python scripts/run-test262.py`. Library smoke: `./scripts/run-library-tests.sh acorn` and `decimal.js`.

## Regression risk

- **Evaluation order** (headline): `ResolveName` must stay before the RHS and `StoreResolvedNamePop` must consume that ref; the local-slot path skips early resolution only because a frame local cannot be rebound mid-RHS (#898 argument, unchanged). `compile_effect` never rewrites the by-name path, so `with`/global-object names keep `ResolveName`.
- **GC rooting**: pop-after-store for both `*Pop` stores; `binary_slow`/`BinConst`/`LocalBinConst` keep operands rooted across `eval_binary` and unroot LIFO afterwards; a local value loaded by `LocalBinConst` is `push_value`d before the slow path (closure-reassigned local + `valueOf` + `$262.gc()`). `gc_bytecode_roots` chunk-exit balance assert and `JSSE_GC_STRESS` guard this. `Safepoint` placement is untouched (fusion removes `Pop`s inside a statement, never statement boundaries).
- **Stack accounting**: `current_stack`/`max_stack` bookkeeping in each emitter (`pop_n(1)` after `*Pop` stores; compare-jump pops 2, pushes none); `debug_assert_eq!(current_stack, 0)` at statement boundaries catches mistakes.
- **Jump encoding**: `CmpJumpIfFalse` has an extra operand byte before the i16; `patch_jump`'s `from = patch + 2` stays right only if the patch site is the offset itself. `max_jump_target`/`ends_with_return` (trailing `ReturnUndefined`) unchanged.
- **Opcode table**: `OP_SLOTS` overflow indexes `vm_op_hist` out of range under `perf-counters` — bump and tighten the const assert; `Op::from_u8` and test `decode_ops` are the only decoders.
- **Number fast path divergence**: must use the same `number_ops` functions as `eval_binary`'s fast path and `JsValue::number` for NaN canonicalisation; `Eq`/`StrictEq` on a number pair both mean `number_ops::equal`.
- `test262-pass.txt` baseline: not expected to move (no semantic change); any movement is a bug. Shared machinery leaned on: the bytecode VM fast path, `eval_identifier_update`, `put_value_by_ref`, `resolve_identifier_ref`; the tree-walker `eval_expr`/`exec_statement`, the property MOP and `ObjectKind` matches are untouched. The acorn/decimal.js harnesses stress arithmetic and branches hard; run the two quick ones.

## Out of scope / follow-ups

- Standalone `LoadSmallInt` immediate (no dispatch saved), constant-pool de-duplication (`add_constant` appends per literal).
- Loop inversion (test at the bottom, saving the back `Jump`; needs a conditional back-edge safepoint).
- Script-goal top-level expression statements (`SetCompletion` fusion/elision).
- Frame-register/`Vec<JsValue>` locals replacing the `RefCell<Environment>` + `BindingMap` access, a `local-local` binary/compare form, int32-specialised ops, merging `Safepoint` into the following op.
- An unsafe `Op` transmute in `from_u8` (measure with callgrind first).
- Any change to `eval_binary`/`eval_binary_slow`, `ResolveName` semantics, generator/async transforms, or the tree-walker.
- Rolling `test262-pass.txt` forward (`--update-baseline` is a `main` operation).
