# Plan: share per-literal function metadata across closures (#900, second half)

## Goal
Close #900. The compile-verdict half landed in #943 (`bytecode_memo`, `src/interpreter/body_memo.rs`; `shared_bytecode_verdict`, `src/interpreter/eval.rs:1850`). What remains is per-closure *metadata* work done on every closure creation: `Rc::new(params.clone())` (deep `Pattern` clone), `func_uses_arguments` (full body walk) and `params_are_simple`. Compute once per function literal and share by `Rc`.

## Problem restated
Every evaluation of a function expression/declaration/arrow/method/class constructor builds a `JsFunction::User` by deep-cloning the parameter patterns into a fresh `Rc<Vec<Pattern>>`, walking the whole body for `arguments`, and rescanning params for simplicity. These are pure functions of the AST literal but cost O(literal size) per closure (`func_uses_arguments` was 13.6% of instructions in `bench_calls_closure_bigbody`). #943 shares the bytecode verdict, state machine and hoist analysis per `Body`; this metadata is the last per-closure O(size) work. Class field initializers / static blocks do not compile per evaluation (they are tree-walked via `eval_expr`/`exec_statements`, `src/interpreter/eval/literals.rs:1117`, `:1194`), so they cannot trigger repeated compiles; their per-class-evaluation AST clones are a separate follow-up.

## Spec basis
No JavaScript behavior change: pure memoisation of values the spec defines as functions of the function literal alone. Governing clauses (ids are ECMA-262 `sec-…` ids; the `spec/` submodule is empty in this workspace — `git submodule update --init --depth 1 spec` and confirm numbers once):
- `sec-static-semantics-issimpleparameterlist` (IsSimpleParameterList, §15.1.3 in ES2024) → `has_simple_params`.
- `sec-functiondeclarationinstantiation` (FunctionDeclarationInstantiation, §10.2.11; `argumentsObjectNeeded`: arrows never get their own `arguments`; a parameter/lexical/function named `arguments` suppresses it) → `uses_arguments` (conservative over-approximation, unchanged).
- `sec-runtime-semantics-instantiateordinaryfunctionobject`, `sec-runtime-semantics-instantiateordinaryfunctionexpression`, `sec-runtime-semantics-instantiatearrowfunctionexpression`, `sec-definemethod`, `sec-runtime-semantics-classdefinitionevaluation` — each evaluation must still produce a fresh function object with its own [[Environment]], `prototype` object and own properties; only immutable literal-derived data is shared.

## Assumptions
- Design: a lazily initialised cell on the three AST function nodes (`FunctionDecl`, `FunctionExpr`, `ArrowFunction`), not a `BodyMemo` side table and not parse-time eager computation. Rationale: no hashing/eviction/pinning, lifetime follows the AST, matches the issue's "per AST node". Lazy because post-parse passes mutate params/body (`assign_ic_sites`, generator transform) and the parser has ~25 construction sites; first use is after the last mutation. (best guess)
- The cell's `Clone` **resets to empty** (hand-written impl). A node cloned and then mutated (e.g. `Rc::make_mut` in `assign_ic_sites`, generator-transform renumbering of IC sites inside default initialisers) therefore can never carry a stale `Rc<Vec<Pattern>>` — no per-pass audit needed. Cost: any path that clones the node per evaluation loses sharing; the slice-2 `Rc::ptr_eq` tests per creation path detect that, and such a path must be changed to hold a reference/`Rc` instead. (advisor review)
- Type is named `FunctionLiteralInfo` / `LiteralInfoCell` to avoid colliding with object "shape" vocabulary. (advisor review)
- `name: Option<String>` / `source_text` clones are left alone: `SourceText` clones an `Rc<str>`; changing `JsFunction::User.name` touches 46 sites. Re-profile after the change; file a follow-up if `name.clone()` still shows. (best guess)
- `new Function` sites (`builtins/function_constructors.rs`, `uses_arguments: true` by design, fresh AST per call) are untouched: nothing to share.
- The ≤ 2.5 s wall-clock criterion cannot be measured here; the verifiable proxies are deterministic perf-counters and callgrind instruction/call counts. (best guess)

## Files to touch
| File | Role | Lines of Interest |
|------|------|-------------------|
| `src/ast.rs` | add `FunctionLiteralInfo` + `LiteralInfoCell`; field on `FunctionDecl`/`FunctionExpr`/`ArrowFunction`; reuse `func_uses_arguments`/`params_are_simple` | structs 830-860, fns 992-1003, literal at 2217 |
| `src/parser/expressions.rs` | add field to every `FunctionExpr {..}`/`ArrowFunction {..}` literal | 986-1484, 1739-2291 |
| `src/parser/declarations.rs` | same: `FunctionDecl` 292, `FunctionExpr` 1162 | 292, 1162 |
| `src/parser/modules.rs` | same: `FunctionDecl` 409, 437 | 409, 437 |
| `src/interpreter/generator_transform.rs`, `src/interpreter/generator_analysis.rs` | test-only `FunctionExpr` literals | 4278, 1510 |
| `src/interpreter/eval.rs` | `Expression::Function`/`ArrowFunction` creation; object-literal method creation | 609-650, 6290-6301 |
| `src/interpreter/eval/literals.rs` | class ctor + method/accessor creation (default ctors keep #947 `DEFAULT_*_CONSTRUCTOR`) | 376-406, 628-640, 809-821 |
| `src/interpreter/exec.rs` | `hoist_function_decl`, switch-case function hoist | 593-604, 2709-2720 |
| `src/interpreter/mod.rs` | function-declaration instantiation, module default export | 5165-5176, 5214-5225, 5395-5406 |
| `src/interpreter/types.rs` | `JsFunction::User` unchanged in type; only fed from the shared info | 1300-1380 |
| `tests/perf_counters_report_paths.rs` | `compile_ok` locks (functions, class elements) | after 409 |
| `src/interpreter/tests.rs` | sharing + parity unit tests (`run_script` l.28, `global_number`) | near 6070 |
| `test262-extra/function-literal-closures-share-metadata-but-not-state.js` [new] | spec-visible regression | — |
| `AGENTS.md` | one-line Architecture Notes entry | — |

## TDD slices (each its own commit)

### 1. Lock acceptance criteria (passes already; guards later slices)
- **File**: `tests/perf_counters_report_paths.rs`, run with `cargo test --release --features perf-counters --test perf_counters_report_paths`.
- **Tests** (child-process, helpers `scratch()`/`perf()`):
  a. `bench_calls_closure_bigbody` shape scaled to ~50 closures plus `v => v + 1` created 100×: `PERF compile_ok` equals the number of distinct executed literals (list each in a comment; derive the exact number by running once), independent of closure count; an ineligible literal created many times yields one `BAIL` row.
  b. A class with an instance field, a static field and a static block, evaluated in a loop (50×) and instantiated: `compile_ok` is constant w.r.t. loop count (field initialisers/static blocks are tree-walked, so no per-evaluation compile).

### 2. RED: sharing + parity unit tests
- **File**: `src/interpreter/tests.rs`.
- **(a) sharing**: for each creation path, create the closure twice and assert `Rc::ptr_eq` on the two `JsFunction::User.params`: function expression; arrow; function declaration hoisted on two calls of the outer function; declaration in a switch case (exec.rs:2709 path); object-literal method, getter, setter; class constructor, method, getter, static method; closure created inside a generator body and inside an async body; module default-export function (mod.rs:5395 path, if reachable from a unit test, else cover via a `tests/` module run).
- **(b) parity**: `uses_arguments`/`has_simple_params` equal a direct `func_uses_arguments`/`params_are_simple` for: no params, defaults, rest, destructuring, `arguments` in a default initialiser, `arguments` only in a nested arrow (counts), in a nested non-arrow function (does not count), arrow (always false).
- Fails until slices 3-4.

### 3. GREEN: the cell on the AST nodes
- **File**: `src/ast.rs`; parser/test literal sites.
- **Change**: `pub(crate) struct FunctionLiteralInfo { pub params: Rc<Vec<Pattern>>, pub uses_arguments: bool, pub has_simple_params: bool }`; `pub(crate) struct LiteralInfoCell(std::cell::OnceCell<Rc<FunctionLiteralInfo>>)` with `Default`, `Debug`, a hand-written `Clone` returning an empty cell, and `get_or_init(&self, params: &[Pattern], body: Option<&Body>) -> &Rc<FunctionLiteralInfo>` (body `None` ⇒ arrow ⇒ `uses_arguments = false`, no walk). Add `pub info: LiteralInfoCell` to the three structs and `info: Default::default()` to every literal. Convenience `FunctionExpr::info()`, `FunctionDecl::info()`, `ArrowFunction::info()`.
- **Reuses**: `func_uses_arguments` (ast.rs:992), `params_are_simple` (ast.rs:1000).
- One comment: the cell is filled on first closure creation and reset on clone, so it can never outlive a mutation.

### 4. GREEN: route every creation site through the info
- **Files**: `eval.rs` (611/621/622, 634/645, 6290-6301), `eval/literals.rs` (379-382, 628-639, 809-820), `exec.rs` (593-604, 2709-2720), `mod.rs` (5165-5176, 5214-5225, 5395-5406).
- **Change**: `let info = x.info();` then `params: info.params.clone()`, `uses_arguments: info.uses_arguments`, `has_simple_params: info.has_simple_params`. `ClassMethod.value`/object-literal `m.value` are `FunctionExpr`s, so the cell lives there. If a `ptr_eq` test in slice 2 still fails because a path clones the node per evaluation (hoisting collects `functions_to_init`, labeled-function unwrap, class element cloning), change that path to borrow or hold an `Rc` rather than weakening the test.
- Slice 2 turns green.

### 5. Docs
- `AGENTS.md` Architecture Notes: "closures of one function literal share `Rc<FunctionLiteralInfo>` (params, uses_arguments, has_simple_params) via the AST node's `LiteralInfoCell`, alongside the per-Body `BodyMemo`s (#900)". No ADR (extends #943's decision); no `CONTEXT.md` term.

### 6. Measure and record (PR body, not a code change)
- `cargo build --release --features perf-counters`: run `docs/perf/2026-10-10/repros/calls/bench_calls_closure_bigbody.js` and `bench_calls_closure_create.js`; report `compile_ok`.
- `valgrind --tool=callgrind` on a scaled-down bigbody (e.g. 5,000 closures) on the default release build, before (`origin/main`) vs after: the call count of `ast::func_uses_arguments` and `Pattern::clone` must drop from ≈ closure count to ≈ distinct literals; record instruction totals. Wall times only from the default build, never the instrumented one. Do not edit `test262-pass.txt`.

## Test surface
- Targeted test262 (`uv run python scripts/run-test262.py <dir>`): `test262/test/language/arguments-object/`, `language/function-code/`, `language/expressions/{function,arrow-function,generators,async-function,async-generator,class,object}/`, `language/statements/{function,class,generators,async-function,async-generator}/`, `built-ins/Function/`, `built-ins/GeneratorFunction/`, `built-ins/AsyncFunction/`, `annexB/language/function-code/`, `annexB/language/global-code/`, `annexB/language/eval-code/`.
- Full `uv run python scripts/run-test262.py` against the `origin/main` baseline; `--no-bytecode` spot runs on `language/arguments-object` and `language/expressions/function`.
- `uv run python scripts/run-custom-tests.py`, `cargo test --release` (and with `--features perf-counters` for the report tests), `./scripts/lint.sh`.
- `JSSE_GC_STRESS=7 uv run python scripts/run-test262.py test262-extra/ --timeout 300`, normal and `--no-bytecode` (shared data is AST, not GC-managed; confirms no new roots).
- New `test262-extra/function-literal-closures-share-metadata-but-not-state.js` (pattern of `Array-push-undefined-presence.js`, header names `sec-functiondeclarationinstantiation`, `sec-runtime-semantics-instantiateordinaryfunctionexpression`, `sec-runtime-semantics-instantiatearrowfunctionexpression`, `sec-static-semantics-issimpleparameterlist`): closures from one literal have distinct identity, independent captured bindings, independent own `prototype` objects and `length`; `arguments` present exactly when referenced (including in a default initialiser; arrows see the outer `arguments`); mapped vs unmapped `arguments` for simple vs non-simple params; created in a loop and called interleaved; generator / async / async-generator closures from one literal run independent instances; a class evaluated twice yields distinct constructors/methods with shared behaviour.

## Regression risk
- Stale/over-shared info: eliminated by reset-on-clone; remaining risk is lost sharing (perf only), caught by slice-2(a).
- `Rc<Vec<Pattern>>` identity now shared across closures: nothing calls `Rc::make_mut`/`try_unwrap` on `JsFunction::User.params` (grepped); generator transform and bytecode compiler read params immutably.
- `eval` / `new Function` parse a fresh AST each time → own cells, no cross-contamination.
- Hot paths touched: closure creation only (`eval_expr` function/arrow arms, `hoist_function_decl`). Call path, property MOP, GC rooting, `ObjectKind` matches and the bytecode VM are untouched. `test262-pass.txt` should not move.
- Memory: one extra `Vec<Pattern>` copy per executed literal; negligible against the per-closure clones removed.

## Out of scope
- `JsFunction::User.name` → `Rc<str>`, and `create_function`'s per-closure `fn_name`/`length` computation (follow-up if profiling shows it).
- Per-class-evaluation clones of field initialisers/static blocks (`p.value.clone()`, `stmts.clone()` in `literals.rs`) — separate follow-up.
- `compile_body` speed (21% for the first compile), `BodyMemo` capacity/eviction, and the `then_chain` reaction-job overhead (#897).
- Storing `params` as `Rc<Vec<Pattern>>` in the AST itself.
- Any `test262-pass.txt` baseline update.
