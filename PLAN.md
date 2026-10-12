# Plan: #902 — remove the bytecode eligibility cliff (AST fallback ops + native throw / typeof / member ++ / instanceof / in / BigInt)

## Problem (restated)

Bytecode eligibility is all-or-nothing per function body: any construct `Compiler`
(`src/interpreter/bytecode/compiler.rs`) rejects with `CompileError::Unsupported` — `switch`, `throw`,
`try`, `for-in/of`, `let/const`, `typeof`, array/object/function/class expressions, member `++/--`,
`instanceof`/`in`, BigInt/RegExp literals, labeled non-loops, function/class declarations — sends the
*entire* body to the tree-walker, ~1.8x slower than the VM and 5.6–6.6x boa on the same loop. The
`bench_numeric_bail_*` repros (`docs/perf/2026-10-10/repros/numeric/`) add one never-executed unsupported
statement outside a hot loop and lose ~1.8x. Goal: one unsupported *node* must cost only that node, and
the constructs with the best measured payoff that are cheap to lower must compile natively.

## Approach and PR boundary (decided)

1. **Node-granular AST fallback first** (the issue's "evaluate first" item):
   - `Op::EvalAst(u16)` evaluates an `Expression` via `interp.eval_expr(expr, env)` and pushes the result
     (operand stack may be non-empty; pending operands are already rooted in `gc_bytecode_roots`, as for
     `Call`/`GetProp`, which also re-enter user code).
   - `Op::ExecAst(u16, u16)` runs a `Statement` via `interp.exec_statement(stmt, env)` at a statement
     boundary (operand stack empty; `compile_statement_list` already emits `Safepoint` first).
   Spec semantics are inherited from the conformance-tested tree-walker, and every later slice is
   incremental.
2. **Native lowering, required in this PR:** `throw`, `typeof`, member `++/--`, `instanceof`/`in`,
   BigInt literal constant. These are cheap, local, and are exactly the hot constructs behind AES (`throw`),
   richards/crypto/gbemu (member `++`), ML/sha256 (`typeof`), noble/paillier (`instanceof`, BigInt).
3. **PR scope = Slices 0–4d + 7.** Function Goal only. The PR must say plainly that after it, a `switch`
   or a `let/const`-owning block that executes hot still runs on the tree-walker at statement granularity
   (no worse than today, and it no longer drags the rest of the function down).
4. **Follow-up issues to file at PR time (`gh issue create`, copy the designs below):**
   (F1) native `switch` + labeled blocks, (F2) native function-top-level `let/const`, (F3) VM block
   environments / per-iteration `for(let)` / block-scoped `let/const`, (F4) native array/object literals and
   closures, (F5) native `try`/`for-in`/`for-of`, (F6) Script-goal `ExecAst`/`EvalAst`. Do not start them.

## Poison constructs — whole-body ineligible, enforced by a pre-scan

`CompileError::Ineligible(&'static str)` is new and means "this body stays on the tree-walker, exactly as
today": direct `eval(...)`, `with`, `super`, `new.target`, `import.meta`/`import()`, `yield`/`await`,
private-`in` (`#x in o`), `return` in Script goal, and the overflow guards (`constant pool`, `name pool`,
`jump offset`, `operand stack`, `statement:Break`/`Continue` with no resolvable frame).
**Enforcement:** the node-level wrappers only see nodes the compiler visits, and once a parent falls back
its subtree is never visited — so poison is found by a one-time deep pre-scan `body_has_poison(stmts)`
run at the top of `compile_function_body`/`compile_body`. It walks statements and expressions but does
**not** descend into nested function/arrow/method bodies (separate Bodies with their own verdict, not
evaluated in this frame); it does descend into class heritage, computed keys, static blocks and field
initialisers. A hit returns `Ineligible` before any code is emitted. The compile-time `Ineligible`
returns at the original sites stay as a second line of defence.

## Spec basis

Fallback ops delegate to the tree-walker, so semantics = the tree-walker's; the native lowerings must
preserve these clauses (ids and numbers verified against tc39/ecma262 `spec.html` at the pinned `spec/`
commit `270a490b3f8b…`):

- Completion routing across the fallback boundary: `sec-completion-record-specification-type` (6.2.4),
  `sec-updateempty` (6.2.4.4), `sec-block-runtime-semantics-evaluation` (14.2.2).
- `break`/`continue`/labels: `sec-break-statement-runtime-semantics-evaluation` (14.9.2),
  `sec-continue-statement-runtime-semantics-evaluation` (14.8.2), `sec-runtime-semantics-labelledevaluation`
  (14.13.4).
- `throw`: `sec-throw-statement-runtime-semantics-evaluation` (14.14.1).
- `typeof`: `sec-typeof-operator-runtime-semantics-evaluation` (13.5.3.1) — unresolvable reference →
  `"undefined"`, uninitialised (TDZ) binding → ReferenceError via `sec-getvalue` (6.2.5.5) /
  `sec-declarative-environment-records` (9.1.1.1).
- `++`/`--` on member references: `sec-postfix-increment-operator-runtime-semantics-evaluation` (13.4.2.1),
  `sec-postfix-decrement-operator-runtime-semantics-evaluation`,
  `sec-prefix-increment-operator-runtime-semantics-evaluation` (13.4.4.1),
  `sec-prefix-decrement-operator-runtime-semantics-evaluation`, `sec-getvalue` (6.2.5.5),
  `sec-putvalue` (6.2.5.6), `sec-tonumeric` (7.1.3) — base evaluated once; GetValue → ToNumeric → ±1 →
  PutValue; postfix yields the *numeric* old value.
- `instanceof` / `in`: `sec-relational-operators-runtime-semantics-evaluation` (13.10.1),
  `sec-instanceofoperator` (13.10.2), `sec-ordinaryhasinstance` (7.3.21), `sec-hasproperty` (7.3.11).
- BigInt literals: `sec-literals-runtime-semantics-evaluation` and `sec-literals-numeric-literals`
  (12.9.3, NumericValue). RegExp literals allocate a fresh object per evaluation
  (`sec-regular-expression-literals-runtime-semantics-evaluation`, 13.2.7.3) and are NOT constants.
- Fallback-boundary hazards: name inference — `sec-isanonymousfunctiondefinition` (8.4.3),
  `sec-runtime-semantics-namedevaluation` (8.4.5); block scope/TDZ — `sec-blockdeclarationinstantiation`
  (14.2.3), `sec-let-and-const-declarations-runtime-semantics-evaluation` (14.3.1.2),
  `sec-functiondeclarationinstantiation` (10.2.11), `sec-forbodyevaluation` +
  `sec-createperiterationenvironment` (14.7.4.4); proper tail calls — `sec-tail-position-calls` (15.10),
  `sec-preparefortailcall`.
- Deferred designs: switch (F1) `sec-switch-statement-runtime-semantics-evaluation` (14.12.4),
  `sec-runtime-semantics-caseblockevaluation` (14.12.2), `sec-runtime-semantics-caseclauseisselected`
  (14.12.3), `sec-isstrictlyequal` (7.2.14); try (F5) `sec-try-statement-runtime-semantics-evaluation`
  (14.15.3).

## Facts established during exploration

- `dispatch_body` (`src/interpreter/eval.rs:1929`) runs `vm::run_chunk` directly and never calls
  `instantiate_body_declarations` (`src/interpreter/exec.rs:221`); the VM only declares `chunk.var_names`
  (`vm.rs:308-317`). Safe today because the compiler bails on function/class/let/const and any `var`
  it does not itself see. Precedent: `exec_script_body` (`exec.rs:97-133`) instantiates then runs the chunk.
- Only the plain `CallTarget::User` path (`eval.rs:5751-5880`) reaches `dispatch_body`; generator/async
  bodies go through `generator_transform.rs` state machines (tree-walker only). With non-simple params
  `exec_env` is a separate `body_env` into which the call path has already declared `analysis.var_names`
  (`eval.rs:5828-5846`); with simple params one environment holds params, `arguments`, vars and top-level
  lexicals.
- Compiled `Statement::Block` creates **no** child env (`compiler.rs:697-702`); the tree-walker creates
  `Environment::new(Some(env))` per block (`exec.rs:1061`) and hoists TDZ per block (`exec.rs:618`).
  `bind_pattern` for `let/const` does `env.declare` + `initialize_binding` in the env it is given
  (`exec.rs:1441-1444`), so a lone `let` run via `ExecAst` inside a *compiled* block would leak into the
  function env.
- `Statement` is an owned `Clone` tree; nested function `Body`s share `Rc<Vec<Statement>>` (identity
  `Body::key()` preserved by clone). IC sites are dense ids stored in nodes and valid inside the
  `with_ic_body(body, …)` that `dispatch_body` already establishes. Pointer-keyed identities in `src/`
  are only `Body::key`/`Rc::as_ptr(&self._body)` (Body-level) and `suspendable_dispose_block`/`eval.rs:8919`
  (state machines only); the template cache is keyed by `tmpl.id`.
- Visibility (VM is `interpreter::bytecode::vm`): `pub(super)` in `eval.rs` is VM-visible
  (`eval_binary`, `eval_unary`, `eval_identifier_update`, `resolve_identifier`, `set_object_with_key`,
  `put_value_by_ref`); `pub(super)` in `eval/literals.rs`/`access.rs`/`modules.rs` is not. Private and
  needing a bump: `eval_instanceof` (`eval.rs:7451`; `eval_binary(Instanceof)` is `unreachable!`),
  `apply_update_numeric` (`eval.rs:2344`, pure), `with_get_binding_value`/`resolve_global_getter` (reached
  only through the new `eval_typeof_identifier` helper in `eval.rs`).
- `eval_update` on a **primitive** member base re-runs `eval_expr(arg, env)` (`eval.rs:2601-2607`),
  evaluating base and key twice — a latent spec violation. Native `UpdateProp`/`UpdateElement` evaluate once.
- `eval_literal` re-parses BigInt text on every evaluation (`src/interpreter/eval/literals.rs:117-130`).
- No exception table in the VM: every non-Normal completion is `return abrupt`; the chunk-exit balance
  assert already exempts `Throw`/`Exit`.
- Perf counters: `dispatch_body` calls `enter_ast_body` only on the AST branch (`eval.rs:1957-1962`), so
  tree-walker units from fallback nodes inside a compiled body would otherwise be credited to whatever
  frame is on the attribution stack (the #537 misattribution).
- `hoist_cache` builds `HoistAnalysis.var_names` from a `HashSet` (arbitrary but stable-per-Body order).

## Design details

### Chunk-side AST storage
`Chunk` gains `ast: Vec<AstNode>` (`enum AstNode { Expr(Expression), Stmt(Statement) }`) of **deep
clones**: safe, no `unsafe`, nested function bodies stay Rc-shared so `bytecode_memo`/`hoist_cache`/IC
identity is unchanged. `Chunk` is `Default`, so hand-built test chunks still compile. Rejected for this PR:
pinning `Rc<Vec<Statement>>` + index paths/raw pointers. Skip the clones in the slot-discovery pass of
`compile_function_body` (it only needs `var_names`).

### Error taxonomy, pre-checks and transactional wrappers
- `Unsupported(&'static str)` = "this node may fall back". `Ineligible(&'static str)` = whole body.
- `compile_expr` becomes a wrapper around `compile_expr_native`. Snapshot = `code.len()`, `current_stack`,
  `current_refs`, `loop_frames.len()`, every frame's `break_sites.len()`/`continue_sites.len()`,
  `ast.len()`. On `Err(Unsupported)` for a **standalone-evaluable** expression: roll back, push
  `AstNode::Expr(expr.clone())`, emit `EvalAst idx`, `push_n(1)`, record the reason. `Super`,
  `PrivateIdentifier`, `Spread` are not standalone: their error propagates to the parent.
- `compile_statement` gets the same wrapper emitting `ExecAst idx` for `Unsupported` statements.
  `Ineligible` always propagates.
- **Parent pre-checks run *before* compiling children**, because an inner wrapper *succeeds* by emitting
  `EvalAst` and the parent would never see an error:
  1. *Name inference:* `Assign`/`Variable`/default-value arms test `value.is_anonymous_function_definition()`
     before compiling the RHS and, if true, raise `Unsupported` at the declaration/assignment node itself
     so the whole node falls back (the tree-walker's `set_function_name`/`eval_class` binding name is
     applied by the parent, never by the function expression).
  2. *Scope rule:* a `Block`, `Switch`, loop, `Try` or `Labeled` body is compiled inline (no child env)
     only if its statement list declares nothing block-scoped — no `let/const/using/class/function`
     declaration, and for loops no `let/const/using` head. The predicate `declares_lexical(&[Statement])`
     runs first; otherwise raise `Unsupported` at that statement so it runs as one `ExecAst` unit and the
     tree-walker creates the block env, hoists TDZ and does per-iteration copies. Function *top level* is
     exempt (its lexicals live in `exec_env`).
  3. *Tail calls:* `Return(Some(expr))` with `contains_tail_call(expr)` (non-direct) raises `Unsupported`
     at the `return` statement (tree-walker computes `in_tail_position`, returns `TailCall`); the direct
     `Return(Some(Call))` arm keeps `ReturnCall` and a failing `compile_call` falls back at the *statement*.

### Hoisting without a per-call instantiation tax
`instantiate_body_declarations` does string-keyed `contains_key`/`declare` work per call — exactly what
#898 removed (`tests/perf_counters_env_hashing.rs`). Fallback nodes like a never-executed `throw` need none
of it. So:
- `Chunk.var_names` (declared by the existing atom prologue) additionally receives every `var` name found
  inside fallback *statements* (`Interpreter::collect_var_names_from_stmts` on the cloned node; it does
  not enter nested functions), so a read before the declaration sees `undefined`, as today.
- New `Chunk.lexical_decls: Vec<(u16 /*name idx*/, bool /*const*/)>`: top-level `let/const/using` names
  (and top-level `class` names as const), declared uninitialised by the VM prologue with `declare_atom`
  into `env` if absent (TDZ). A top-level `let/const/class` statement itself is an `ExecAst` node in this PR.
- New `Chunk.needs_instantiation: bool`: set only when the body (not descending into nested functions)
  contains any `FunctionDeclaration` statement, at top level or nested in a block (Annex B).
  `dispatch_body` then calls `instantiate_body_declarations(stmts, exec_env, Some(&analysis))` (make it
  `pub(crate)`) before `run_chunk`. Top-level `FunctionDeclaration` compiles to a no-op *only* in that case.
- Slot hints (`locals`) are unchanged in this scheme because prologue order = first-use order as today;
  if full instantiation runs, pass `hoist_cache.analysis_for(body).var_names` order via
  `FrameShape.hoisted_vars` from `shared_bytecode_verdict` (do not derive order from the `HashSet`).

### Completion routing for `ExecAst`
| completion from `exec_statement` | VM action |
|---|---|
| `Normal`/`Empty` | continue (Function goal) |
| `Return(v)` | `return Completion::Return(v)` (as `Op::Return`) |
| `TailCall{..}` | return unchanged (as `Op::ReturnCall` strict path) |
| `Throw`/`Exit` | return unchanged |
| `Break(label,_)`/`Continue(label,_)` | look up `chunk.escape_tables[t]` → jump target; no entry ⇒ return the completion unchanged |

`ExecAst` carries `(ast_idx, escape_table)` (`0xFFFF` = none). The table is built at compile time from
**all enclosing compiled loop frames** (unlabeled break → innermost frame; labeled → each label; unlabeled
continue → innermost loop; labeled continue → that loop). Break targets are forward-patched when the frame
pops (generalise the patch lists to cover both 2-byte jump operands and table entries); `while` continue
targets are known, `for`/`do-while` are patched like `continue_sites`. A statement with no enclosing
frame gets no table.

### Entry-cost guard
A chunk whose only content is `Safepoint`/`ExecAst`/`EvalAst` pays chunk-entry cost for nothing
(`docs/perf/2026-09-02/bytecode-entry-cost.md`, #539). Count `native_nodes`; if zero return
`Ineligible("fallback-only")` so the verdict is unchanged from today.

### Counters and attribution (so "BAIL → 0" is not vacuous and AST units stay honest)
Under `perf-counters`:
- New `FALLBACK\t<reason>\t<static sites>` rows, recorded once per literal at the verdict site; and
  `PERF\tast_fallback_execs` (dynamic `ExecAst`/`EvalAst` executions).
- `dispatch_body`'s compiled branch wraps `run_chunk` in `enter_ast_body(name, seq, false)` /
  `leave_ast_body()` **only when the chunk has fallback nodes**, so fallback units are exclusive to that
  function's `BODY` row. `is_function_invocation = false` keeps `body_dispatch_ast`,
  `ast_units_in_functions` and `ast_units_per_ast_body` unpolluted; add `PERF ast_units_in_fallbacks`.
  Do not increment `body_ast`/`body_non_function`.

### Acceptance criteria — scoped honestly
The Script goal stays all-or-nothing, so each repro's `<script body>` still records
`BAIL statement:FunctionDeclaration` (it did before). The acceptance checks are therefore:
(a) no `BAIL` row carrying any of the issue's construct labels for **function** bodies, and the `kernel`
`BODY` row names no bail reason, (b) `body_dispatch_ast == 0` for the repro, (c) each
`bench_numeric_bail_*` ≤ 1.1x `bench_numeric_bail_none` (plain release build, interleaved median of 5,
`--no-bytecode` control), (d) JetStream AES AST units < 1% of total, with `ast_fallback_execs` and
`ast_units_in_fallbacks` reported for richards, crypto and gbemu so "BAIL→0" is not read as "done" where a
construct still executes hot on the tree-walker.

## Files to touch

| File | Change |
|---|---|
| `src/interpreter/bytecode/op.rs` | add `EvalAst`, `ExecAst`, `Throw`, `Typeof`, `TypeofName`, `Instanceof`, `In`, `UpdateProp`, `UpdateElement` (+ `from_u8`) |
| `src/interpreter/bytecode/chunk.rs` | `AstNode`, `Chunk.{ast, escape_tables, lexical_decls, needs_instantiation, has_fallback}`, `Constant::BigInt(JsBigInt)` |
| `src/interpreter/bytecode/compiler.rs` | `Ineligible`; `body_has_poison` pre-scan; `compile_expr`/`compile_statement` wrappers + snapshot/rollback; parent pre-checks; `declares_lexical`; escape tables; throw/typeof/update/instanceof/in/BigInt lowering; prologue data; `native_nodes` guard |
| `src/interpreter/bytecode/vm.rs` | handlers for new ops (rooting per `push_value`/`pop_value`); lexical-TDZ prologue; `ExecAst` completion routing |
| `src/interpreter/eval.rs` | `dispatch_body`: instantiation when `needs_instantiation` + attribution frame under counters; `shared_bytecode_verdict`: match `Ineligible`, `FALLBACK` record; `pub(super) eval_typeof_identifier` factored from `Expression::Typeof` (`665-716`); `eval_instanceof`/`apply_update_numeric` → `pub(super)`; fix `eval_update` primitive-base double evaluation (`2601-2607`) |
| `src/interpreter/eval/literals.rs` | `pub(crate) fn parse_bigint_literal(&str) -> num_bigint::BigInt` shared with the compiler |
| `src/interpreter/exec.rs` | `instantiate_body_declarations` → `pub(crate)`; `exec_script_body` error match for `Ineligible`/`Unsupported` (both bail for Script goal) |
| `src/interpreter/perf_counters.rs` | `FALLBACK` table, `ast_fallback_execs`, `ast_units_in_fallbacks`, report rows |
| `src/interpreter/bytecode/tests.rs` | rewrite the "asserts it bails" tests (list in Test surface); add per-slice parity tests |
| `tests/perf_counters_report_paths.rs` | `bail.js` case (l.278-327) and the `expression:Function` closure test (l.412-455) need a still-ineligible construct (`with`/direct `eval`); add `FALLBACK` row coverage |
| `tests/perf_counters_bytecode_fallback.rs` [new] | gate for the six construct micros (acceptance (a),(b)) + atom-counter assertion for a function with a never-executed `throw` called in a loop; add to the `--test` list in `.github/workflows/ci.yml` (l.60-63) |
| `test262-extra/bytecode-*.js` [new] | parity files listed below |
| `docs/adr/2026-10-12-HHMM-bytecode-ast-fallback-ops.md` [new] | ADR: node-granular fallback, poison pre-scan, scope rule, prologue hoisting vs instantiation, completion routing, attribution |
| `CONTEXT.md` | add **Bail Reason**, **Eligibility Membrane**, **AST Fallback** (format `**Term**:` … `_Avoid_:`) |
| `AGENTS.md` | Architecture Notes + GC-stress paragraph: fallback is per node now; list `Ineligible` constructs |
| `docs/perf/<date>/902-bytecode-ast-fallback.md` [new] | measurements; template `docs/perf/2026-10-12/898-atom-environments.md` |

## TDD slices (red → green → refactor; `cargo test --release` green between slices)

**Slice 0 — taxonomy + poison pre-scan (refactor under existing tests).**
Red: `poison_constructs_stay_ineligible` — direct `eval`, `with`, `super.x`, `new.target`, `#x in o`,
**each placed inside** a `try`, a block with `let`, and an array literal (the subtree a fallback would have
hidden) — assert `compile_body`/`compile_function_body` return `Err(Ineligible(_))` and runtime
`bytecode_count == 0`. Code: `Ineligible`, `body_has_poison`, reclassify sites, update the two `match`
sites (`eval.rs:1910`, `exec.rs:130`).

**Slice 1 — `EvalAst`.**
Red (`assert_parity_number`-style: both modes, `bc_count >= 1`, `ast_count == 0`): array literal in a hot
loop; object literal; regex literal gives a fresh object per call; function expression closing over a
local; `var f = function(){}; f.name === 'f'` and `f = () => {}` on a local (parent pre-check 1);
template literal; `typeof undeclared`; optional chain; `delete o.x`; class expression; spread call;
`(0, f)(x)` callee; `this`/`arguments` inside a fallback expression; a fallback inside a call argument
and inside a binary operand with a pending object operand under `JSSE_GC_STRESS=1`.
Code: `Chunk.ast`, `Op::EvalAst`, snapshot/rollback (incl. `loop_frames.len()`), wrapper, parent
pre-check 1, `has_fallback`, `var_names` from fallback subtrees.

**Slice 2 — `ExecAst`, scope rule, escape tables, hoisting prologue.**
Red: `try/catch/finally` ordering (finally overrides return; return inside try-with-finally); `for-in`;
`for-of` with early `break` closing the iterator; labeled non-loop `a: { … break a; }`; `continue`,
`break` and labeled `break outer`/`continue outer` escaping an `ExecAst`'d `try` inside compiled
`while`/`for`/`do-while`; `return` and a strict tail call out of a fallback (`TailCall` preserved); an
uncaught `throw` from a fallback reaches the caller's `catch`; `let`/`const` in a block inside a compiled
loop with a closure capturing the per-iteration value (scope rule); lone `let` inside an `if` block is not
leaked into the function scope (parent pre-check 2); TDZ read before `let` throws ReferenceError;
function declaration in a block (Annex B, sloppy) visible after the block; `var` declared inside a
fallback `try` readable (`undefined`) before it; top-level function declaration callable before its
definition; top-level `let` read before its declaration throws; `using` disposed at function exit.
Code: `Op::ExecAst`, `escape_tables`, `declares_lexical`, `lexical_decls` prologue, `needs_instantiation`
+ `dispatch_body` call, `native_nodes` guard.

**Slice 3 — counters, attribution, gate.**
Red: `tests/perf_counters_bytecode_fallback.rs` runs each construct micro via `-e` with the counters
binary and asserts acceptance (a),(b), `FALLBACK` row present, `ast_fallback_execs == 0`, and the
atom counters (`atom_intern_calls`/`atom_lookup_calls`) stay flat as the call count grows for a function
with a never-executed fallback node. Update `perf_counters_report_paths.rs`. A second test executes a
fallback node in a loop and asserts `ast_units_in_fallbacks > 0` while `body_dispatch_ast == 0` and
`ast_units_per_ast_body` is unaffected. Code: `perf_counters.rs`, record at the verdict sites,
attribution frame in `dispatch_body`.

**Slice 4a — native `throw`.** Red: thrown value/Error caught by caller; uncaught; GC stress with a live
outer operand (extend `throw_with_live_outer_operand_does_not_trip_chunk_exit_assert`); `decode_ops`
shows no `ExecAst` for it. Code: `Op::Throw`, `Statement::Throw` arm. Rewrite
`if_with_unsupported_branch_bails_to_unsupported` / `…_alternate_…` (use `with`/direct `eval` or parity).

**Slice 4b — native `typeof`.** Red: unresolvable ident → `"undefined"`; TDZ `let` → ReferenceError;
value operands (`typeof a.b`, `typeof (x, y)`); each typeof result string; a closure invoked inside
`with` reading `typeof x` (the *function* containing `with` stays ineligible). Code: factor
`eval_typeof_identifier`, `Op::TypeofName`/`Op::Typeof` (`typeof_val`).

**Slice 4c-pre — tree-walker fix.** Red (`test262-extra/bytecode-member-update.js`, runs in both
modes): `f().x++`, `f()[k()]++` with `f` counting calls and a primitive (`"abc"`, `5`) base evaluates base
and key exactly once; postfix result is the numeric old value (`"5"` → `5`). Code: `eval.rs:2601-2607`.

**Slice 4c — member `++`/`--`.** Red: `o.x++`, `--o.x`, `a[i++]++`; non-number operands (`"5"`, `null`,
`valueOf` object, BigInt `1n`, Symbol TypeError); getter/setter call counts; frozen object strict
(TypeError) vs sloppy (silent); nullish-base error text parity (`assert_message_parity`). Code:
`Op::UpdateProp name mode`, `Op::UpdateElement mode` using `member_get*`, `apply_update_numeric`,
`member_set*`; computed form does `ToPrimitiveKey` once as compound assignment does. Existing compound
member assignment (`o.x += v`) is already native; logical assignment (`&&=`, `||=`, `??=`) and
`super`/private targets fall back. Rewrite `update_on_member_bails_to_unsupported` (l.1290-1298).

**Slice 4d — `instanceof` / `in` / BigInt literal.** Red: `Symbol.hasInstance`, bound functions,
non-callable RHS TypeError; `k in o` with proxy trap order, non-object RHS TypeError, symbol keys; `10n ** 5n`,
`0x1Fn`, `0o7n`, `0b1n`, mixed `1n + 1` TypeError, constant identity across iterations; RegExp literal still
a fresh object (`EvalAst`). Code: `Op::Instanceof` (root operands as `eval.rs:534-553`), `Op::In` via
`eval_binary(BinaryOp::In, …)`, `Constant::BigInt` built at compile time with `parse_bigint_literal`.

**Slice 7 — docs and measurement.** ADR, CONTEXT.md, AGENTS.md; perf doc with interleaved
median-of-5 numbers; remove stale comments (`tests.rs` l.1135-1136 and l.2544-2547, `tests/gc_stress.rs`
l.18-26/78-86); `gh issue create` for F1–F6 copying the designs below.

## Follow-up designs (not implemented here)

- **F1 native `switch`:** frame kinds `Loop`/`Switch`/`LabeledBlock` (unlabeled `continue` skips switch
  frames); lower as `DupN 1; <test>; StrictEq; JumpIfTrue match_i` per case, `Pop` the discriminant before
  jumping to the matched body or default (stack empty at statement boundaries, so `Safepoint` asserts hold);
  a switch owning any declaration stays `ExecAst` (tree-walker hoists case-body function declarations into
  `switch_env` and does no TDZ for case-body `let/const`). Update
  `labeled_non_loop_statement_still_bails_to_tree_walker` (l.1840-1850).
- **F2 function-top-level `let/const`:** `Op::InitLexical name` setting the TDZ binding declared by the
  prologue (`initialized = true`, keep hoisted kind); anonymous-function init stays `ExecAst`;
  destructuring/`using`/`class` stay `ExecAst`; rewrite `lexical_for_loop_falls_back_to_tree_walker`
  (l.1731-1738). Measure call-heavy benches first.
- **F3 block environments:** mutable current env in `run_chunk_inner`, `PushEnv`/`PopEnv` with static
  depth counted into escape tables, `call_stack_envs` pushes, slot hints only for names not shadowed by an
  enclosing block, per-iteration copies only when the body can capture.
- **F4/F5/F6:** literals (`NewArray`/`ArrayPush`/`NewObject`/`InitProp`, home-object post-pass),
  `MakeClosure` via precomputed function templates, a handler table for `try` (no `ReturnCall` inside
  handler ranges with a finalizer; iterator-close table from `exec_for_of_loop`), Script goal
  (`Normal(v)` → `SetCompletion`, `Empty` → nothing, valued `Break`/`Continue` update the completion
  before the jump; guard with `assert_script_completion_*` and `language/**/cptn-*`).

## Test surface

Initialise submodules first (`git submodule update --init --depth 1 test262 spec`; they are empty in
fresh workspaces). Targeted test262 before the full suite
(`uv run python scripts/run-test262.py <dir>`): `test262/test/language/statements/{try,throw,labeled,
break,continue,for,for-in,for-of,let,const,function,class,block,with,switch}/`,
`test262/test/language/expressions/{typeof,instanceof,in,postfix-increment,postfix-decrement,
prefix-increment,prefix-decrement,compound-assignment,assignment,function,arrow-function,array,object,
class,call,template-literal,delete,tagged-template}/`,
`test262/test/language/{function-code,eval-code,global-code,block-scope,literals,identifier-resolution,
arguments-object,types}/`, `test262/test/annexB/language/{function-code,global-code,statements}/`,
`test262/test/built-ins/{BigInt,Function,Reflect}/`, and the tail-call tests — individual `tco-*.js`
files tagged `features: [tail-call-optimization]` under `language/{expressions,statements}/` (they are
files, not directories; select by name/feature). Then the full `uv run python scripts/run-test262.py`,
and `test262-extra/` in default, `--no-bytecode`, and `JSSE_GC_STRESS=7` modes (`=1` on the new files).

New `test262-extra/` files (front-matter `description`, `info` naming the clause, `esid`,
`flags: [noStrict]` where sloppy behaviour is exercised; they run in both bytecode modes in CI so they
are parity tests by construction — engine-path assertions live in `bytecode/tests.rs` and `tests/`):
- `bytecode-ast-fallback-completion-routing.js` (14.9.2, 14.8.2, 14.15.3, 15.10),
- `bytecode-ast-fallback-lexical-scope.js` (14.2.3, TDZ, `for (let …)` per-iteration capture),
- `bytecode-ast-fallback-function-name-inference.js` (8.4.3, 8.4.5),
- `bytecode-ast-fallback-hoisting.js` (var/function/Annex B visibility),
- `bytecode-member-update.js` (13.4.2.1, 13.4.4.1; includes the primitive-base once-only case),
- `bytecode-typeof-instanceof-in-bigint.js` (13.5.3.1, 13.10.2, 12.9.3),
- `bytecode-throw-in-compiled-body.js` (14.14.1).

Rust tests to rewrite in `src/interpreter/bytecode/tests.rs`: l.1189-1216 (object-literal `if`),
1243-1276 (throw branches), 1290-1298 (member update), 1301-1309 (optional chain → parity),
1312-1327 (`this.#x` member access → falls back via `EvalAst`, parity; private-`in` is poison),
1660-1681 and 2237-2308 (direct eval / super / `new.target` → assert `Err(Ineligible)`; spread and nested
tail call → parity via fallback), 1731-1738 (lexical `for` → parity, runs as `ExecAst`),
1840-1850 (labeled non-loop → parity), 1866-1882 (`bytecode_count` 1 → 2). Re-run
`src/interpreter/tests.rs` (l.6160-6172 `bytecode_memo.len() < 20`; IC/hoist-cache tests using
`run_script_tree_walker`) and `tests/gc_stress.rs`.

Gates: `./scripts/lint.sh`; `cargo test --release`; `cargo build --profile release-checked` + test262-extra
with `--binary target/release-checked/jsse` (debug asserts: root-stack LIFO, chunk-exit balance);
`tests/recursion-limit-*.js` on the plain release build only.

Performance (not CI): counters build `cargo build --release --features perf-counters --target-dir
target/perf-counters`; per micro check acceptance (a),(b); never time that build. Timings from the plain
release build via `docs/perf/2026-10-10/data/microbench.py` (interleaved, median of 5, with a
`jsse-nobc` entry). JetStream `aes`/`gbemu`/`Box2D`/`crypto`/`richards`/`raytrace` via
`scripts/run-jetstream.py --engine target/perf-counters/release/jsse --test <name> --iterations 1
--no-idle-gate`. State which criteria were not measurable on the host (boa/valgrind may be absent;
precedent `docs/perf/2026-10-12/898-atom-environments.md`). Node-compat harnesses:
`scripts/run-library-tests.sh decimal.js`, `acorn`, `zod`.

## Regression risk

- **`test262-pass.txt` baseline** can move wherever previously tree-walker-only bodies now run partly in
  the VM: Annex B function-in-block (`annexB/language/*`), TDZ and `let` closures in loops, tail-call
  tests, mapped `arguments`, `using`/dispose-at-exit. Diff the targeted dirs against
  `origin/main:test262-pass.txt`; no baseline rewrite.
- **Shared machinery leaned on:** `exec_statement`/`eval_expr` called from the VM (state they assume —
  `in_tail_position`, `next_function_is_method`, `tco_suppress_depth`, `call_stack_envs` — is at its
  call-entry value because the fallback runs where the VM would otherwise call `call_function`);
  `instantiate_body_declarations`/`hoist_cache`; `bytecode_memo` (verdicts change for many literals);
  `with_ic_body`; GC rooting (`gc_bytecode_roots` LIFO + chunk-exit balance, `gc_safepoint()` inside the
  tree-walker while operands pending); `Completion::TailCall`; `local_slot_*` hints.
- **Semantic traps:** name inference, scope rule, tail position, poison pre-scan completeness — each has a
  dedicated red test above.
- **Perf risks:** entry cost for fallback-heavy bodies (guarded); atom-counter regressions from
  prologue/instantiation (gated by the Slice 3 test); AST clone memory per compiled literal; the
  attribution frame adds work only under `perf-counters`.

## Assumptions (decided without the user)

- Deep-clone AST storage over pinned pointers (safety first; revisit if memory shows up).
- Poison list is deliberately identical to today's behaviour; loosening it is separate.
- Script goal stays all-or-nothing (F6); acceptance is scoped to function bodies and per-label BAIL rows.
- Switch and top-level `let/const` are follow-ups despite being highest in the issue's payoff list: the
  fallback already removes the cliff for them and the PR stays reviewable; the PR description must state
  that hot `switch` bodies still run on the tree-walker until F1 lands.
- Clause numbers are from the `spec.html` at the pinned submodule commit; re-verify against `spec/`
  once checked out.

## Out of scope

- F1–F6 above; compiling generator/async bodies in the VM; widening `u16` pools / `i16` jumps; any
  refactor, formatting or cleanup unrelated to the above; rewriting `test262-pass.txt`.
