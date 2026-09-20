# Plan: issue #652 — JetStream `cdjs` fails with "... is not a function"

## 1. Problem restated

`scripts/run-jetstream.py` concatenates the cdjs sources and a *sync harness* into **one Script**.
cdjs's `benchmark.js` declares a top-level `function benchmark()`; the sync harness then emits a
top-level `const benchmark = new Benchmark();` (`build_sync_harness`, `scripts/run-jetstream.py:518`).
Per ECMAScript that concatenation is not a valid Script — it is an early SyntaxError
(function declarations are var-scoped at script top level and may not collide with a lexical
declaration). Node agrees, verbatim:

```
$ node cd_run.js          # exact concatenation the runner builds, plus a print/performance polyfill
cd_run.js:1208
const benchmark = new Benchmark();
      ^
SyntaxError: Identifier 'benchmark' has already been declared
```

jsse **accepts** the program, so two independent defects stack:

1. **Engine (spec bug):** `Parser::parse_program` (`src/parser/mod.rs:928-969`) implements the
   §16.1.1 "LexicallyDeclaredNames ∩ VarDeclaredNames" check with `collect_var_declared_names`
   (`src/parser/statements.rs:331`), which is the *block-level* VarDeclaredNames and never
   collects `FunctionDeclaration`. At script top level function declarations *are* var-declared
   names (TopLevelVarDeclaredNames), so `function f(){} const f = 1;` slips through. The `const`
   wins the global binding, `benchmark()` inside `Benchmark.prototype.runIteration` then resolves
   to the `Benchmark` instance, and the call throws.
2. **Harness (root cause of the cdjs failure):** the sync harness leaks bare top-level names
   (`benchmark`, `__iterations`, `__results`; `start`/`end`/`i` are block-scoped and harmless).
   The async harness is already wrapped in `(async () => {...})()`; only the sync one leaks.

The error text `object (class=Object, callable=false, id=1204, keys=[])` is the `Benchmark`
instance (a class instance with no own keys) reaching the not-callable tail of
`call_function_inner_impl` (`src/interpreter/eval.rs:5919-5952`), which formats an internal
debug dump. That dump is a third, cosmetic defect the issue asks to fix "while here".

Why #52 "fixed" it and it came back: #52 blamed `Symbol.iterator`/iterator caching and PR #63
changed only `src/interpreter/gc.rs`. The pinned cdjs revision (`c603c04`) contains no
`Symbol.iterator`, no `for…of`, no spread — the iterator theory was wrong; the identical
message was produced by this harness collision all along. #63 never touched it.

Reproduced on this branch (release build): `run-jetstream.py --test cdjs --no-idle-gate` →
`TypeError: object (class=Object, callable=false, id=1203, keys=[]) is not a function`; the
same script with the harness wrapped in an IIFE and `benchmark` renamed runs cdjs to
completion and passes its own `validate` (1336 collisions) in ~5 s.

**Both slices 1 and 2 are required to close #652, and sequencing matters:** the engine fix alone
turns the failure into a (correct, but still failing) SyntaxError; the harness fix alone makes
cdjs pass but leaves jsse silently accepting an invalid script.

## 2. Spec basis

- **§16.1.1 Static Semantics: Early Errors — Script** (`sec-scripts-static-semantics-early-errors`):
  "It is a Syntax Error if the LexicallyDeclaredNames of |ScriptBody| contains any duplicate
  entries." / "It is a Syntax Error if any element of the LexicallyDeclaredNames of |ScriptBody|
  also occurs in the VarDeclaredNames of |ScriptBody|."
- **`ScriptBody : StatementList` for LexicallyDeclaredNames / VarDeclaredNames** (`sec-static-semantics-lexicallydeclarednames`,
  `sec-static-semantics-vardeclarednames`, spec.html:7774, 8134): they are defined as the
  **TopLevelLexicallyDeclaredNames** / **TopLevelVarDeclaredNames** of the `StatementList`.
- **Static Semantics: TopLevelVarDeclaredNames** (`sec-static-semantics-toplevelvardeclarednames`):
  `StatementListItem : Declaration` where the Declaration is a `HoistableDeclaration` (function,
  generator, async function, async generator) returns its BoundNames; `LabelledItem : FunctionDeclaration`
  returns its BoundNames (so `l: function f(){}` counts); everything else is VarDeclaredNames of the statement.
  Note there: "At the top level of a function or script, inner function declarations are treated like var declarations."
- **Static Semantics: TopLevelLexicallyDeclaredNames** (`sec-static-semantics-toplevellexicallydeclarednames`):
  HoistableDeclarations contribute nothing; let/const/class contribute BoundNames.
- Script-goal parsing is shared by direct/indirect `eval` (`PerformEval`, `sec-performeval`
  — eval code is parsed as `Script`), so the same early errors apply there.
- **Message slice:** `EvaluateCall` (`sec-evaluatecall`) steps "If _func_ is not an Object, throw a
  TypeError" / "If IsCallable(_func_) is false, throw a TypeError" fix *which error* is thrown and *when*
  (after ArgumentListEvaluation). The message text is implementation-defined; no clause constrains it.
  The message change must not alter ordering or the error type.
- Slice 1 (harness) is `N/A: no JavaScript behavior change` — it is benchmark-runner tooling under `scripts/`;
  the language rule it must respect is the §16.1.1 rule cited above.

## 3. Files to touch

Slice 1 (harness):
- `scripts/run-jetstream.py` — `build_sync_harness` (wrap in an IIFE, prefix locals with `__`).
- `scripts/test_benchmark_protocol.py` — new harness-hygiene tests (it already loads the runner via
  `load_runner_module()`; CI runs `python -m unittest discover -s scripts -p 'test_*.py'`).

Slice 2 (engine, script-level early error):
- `src/parser/statements.rs` — new `collect_top_level_var_declared_names` next to
  `collect_var_declared_names` (do **not** modify the shared recursive collector — see §6).
- `src/parser/mod.rs` — `parse_program` (lines ~956-969) uses the new collector; add unit tests in the
  existing `mod tests` (line ~1432).
- `test262-extra/` — new files listed in §5.

Slice 3 (diagnostic):
- `src/interpreter/eval.rs` — replace the `class=/callable=/id=/keys=` arm (5936-5948) with a
  side-effect-free value description helper (new small `fn`, same file or `helpers.rs`).
- `tests/call-non-callable-object-message.js` — new custom test (message text is not spec-defined, so
  per CLAUDE.md it lives in `tests/`, not `test262-extra/`).
- `src/interpreter/bytecode/tests.rs` — one `assert_message_parity` case for a non-callable object callee.

No `docs/adr/` entry and no `CONTEXT.md` change (no new architecture or vocabulary). `CHANGELOG.md`
only if the repo's release process expects per-fix entries (check the top of the file; otherwise skip).

## 4. TDD slices

Each slice is a separate commit (Conventional Commits; PR is squash-merged, suggested title
`fix(parser): reject top-level function/lexical redeclaration and fix cdjs harness (#652)`).

### Slice 1 — sync harness must not leak top-level names (RED → GREEN)
- **Test (RED)** in `scripts/test_benchmark_protocol.py`, new `class SyncHarnessNameHygiene(unittest.TestCase)`:
  a. *Structural, engine-free (always runs):* `harness = runner.build_sync_harness(1, False, 3)`;
     assert no line begins at column 0 with `const|let|var|class|function` (`re.M`), i.e. the harness
     introduces no top-level declaration. Add the same assertion for `build_async_harness` as a pin.
  b. *Behavioural (skips if no engine):* build the exact cdjs collision shape — polyfill preamble +
     `function benchmark(){ … }` + `class Benchmark { runIteration(){ benchmark(); } }` + `var start, end, i;
     function __results(){} var __iterations = 'workload';` + `build_sync_harness(1, False, 3)` — write it to a temp
     file under `tempfile.gettempdir()`/`$TMPDIR`, run with `target/release/jsse` if it exists, else `node` if on PATH,
     else `skipTest`. Assert exit 0 and that stdout parses as JSON with `results` of length 1.
     (Before Slice 2 jsse fails this with the "not a function" TypeError; on Node it fails with the
     SyntaxError. Both are RED for the right reason.)
- **Production (GREEN):** in `build_sync_harness` emit `(() => { const __iterations…; const __results…;
  const __benchmark = new Benchmark(); … })();` — mirror the async harness structure, keep the
  `print(JSON.stringify({...}))` output byte-identical so the result parser is unaffected.
  Grep `scripts/` and `benchmarks/` for other generated harnesses with top-level bindings
  (`gen-mandreel-phases.py`) and apply the same rule only if they emit top-level `const/let`.
- **Manual gate:** `uv run python scripts/run-jetstream.py --test cdjs --iterations 1 --timeout 120
  --no-idle-gate --engine target/release/jsse --jetstream /tmp/JetStream` → PASS (the idle gate
  refuses to run on a busy host; `--no-idle-gate` is diagnostic-only, note that in the PR).

### Slice 2 — script-level Lexical∩TopLevelVar early error (RED → GREEN)
- **Tests (RED)**
  - Rust parser unit tests in `src/parser/mod.rs` `mod tests` asserting `Parser::new(src)?.parse_program()`:
    - **Err** (`Identifier 'f' has already been declared`): `function f(){} const f=1;`, `const f=1; function f(){}`,
      `function f(){} let f;`, `function f(){} class f{}`, `function* f(){} let f;`, `async function f(){} let f;`,
      `async function* f(){} let f;`, `l: function f(){} let f;`, `a: b: function f(){} let f;`,
      and the issue shape `function benchmark(){} class B{} const benchmark = new B();`.
    - **Ok** (must stay legal): `function f(){} function f(){}`, `"use strict"; function f(){} function f(){}`,
      `var f; function f(){}`, `let f; { function f(){} }`, `let f; if (1) { function f(){} }`,
      `let f; switch (1) { case 1: function f(){} }`, `function f(){ let f; }` (inner scope),
      `let f; function g(){ var f; }`.
  - `test262-extra/` (see §5 for names): a real negative script file, an indirect-eval matrix, and a
    positive "still legal" file.
- **Production (GREEN):** `collect_top_level_var_declared_names(stmts: &[Statement], out)` implementing
  TopLevelVarDeclaredNames exactly, at statement-list level only:
  `FunctionDeclaration(f)` → push `f.name` (covers generator/async variants — they share the variant);
  `Labeled(_, inner)` → unwrap nested `Labeled` chain; if the innermost item is `FunctionDeclaration`
  push its name, else fall through to `collect_var_declared_names(item)`; every other statement →
  existing `collect_var_declared_names`. In `parse_program` replace the loop at mod.rs:958-961 with this
  collector; leave the lexical-duplicate check (928-951) as is (it already covers let/const/class).
- Confirm eval: `(0,eval)("function f(){} let f;")` and `eval("function f(){} let f;")` now throw
  SyntaxError (both route through `parse_program`).

### Slice 3 — no internal debug dump in "not a function" (RED → GREEN)
- **Test (RED):** `tests/call-non-callable-object-message.js` — for `var x = {}; x()`, `var a = []; a()`,
  `class B {}; new B()()` (instance callee), `Function.prototype.call.call({})`: catch the TypeError and assert
  `e instanceof TypeError`, `e.message` matches `/is not a function$/`, and does **not** contain
  `class=`, `callable=`, `id=`, `keys=` or `JsPropertyKey`. Also pin the primitive spellings that already work
  (`undefined`, `null`, `1`, `"str"`) and add Symbol/BigInt (today they print a bare `is not a function`
  with no subject). Assert the ordering guarantee from `EvaluateCall`: arguments are evaluated before the
  TypeError (`var n=0; try { ({})(n++) } catch(e){}; if (n!==1) throw …`).
  Add the `assert_message_parity` case in `bytecode/tests.rs` (VM and tree-walker must print the same text —
  both reach the same shared tail, this pins it).
- **Production (GREEN):** extract the `desc` computation at eval.rs:5919-5951 into a helper
  `describe_non_callable(&JsValue) -> String` that never invokes user code (no `ToString`, no getters):
  `undefined`/`null`/booleans/numbers/strings as today; Symbol → `Symbol(desc)`; BigInt → `<n>n`;
  object → `#<ClassName>` using the object's `class_name` (V8-style, keys/ids never exposed);
  drop the `GC'd?` text. Chosen over full `<expr> is not a function` deliberately — see §7.

### Verification pass (after all slices)
- `cargo build --release` (`-j 8`, explicit long timeout), `cargo test --release`,
  `./scripts/lint.sh`, `uv run python -m unittest discover -s scripts -p 'test_*.py'`.
- Custom tests: `uv run python scripts/run-custom-tests.py`.

## 5. Test surface

Targeted test262 (run each with `uv run python scripts/run-test262.py test262/test/<dir>/`; the
`test262/` and `spec/` submodules are empty in a fresh workspace — `git submodule update --init --depth 1 test262 spec`
first):
`language/global-code/`, `language/block-scope/`, `language/eval-code/` (direct+indirect), `language/function-code/`,
`language/statements/{function,generators,async-function,async-generator,class,let,const,switch,labeled,variable}/`,
`language/directive-prologue/`, `annexB/language/{global-code,eval-code,function-code,statements}/`
(Annex B.3.2/B.3.3 function-in-block hoisting interacts with these names), `built-ins/Function/`,
`built-ins/Error/`. Then the full suite once (`uv run python scripts/run-test262.py`); no baseline update.

Not covered by test262 → new first-party tests. `test262-extra/` files (test262 frontmatter, `esid`
naming the clause; note the run command: `uv run python scripts/run-test262.py test262-extra/`):
- `script-toplevel-function-lexical-redeclaration-early-error.js` — `negative: {phase: parse, type: SyntaxError}`,
  `esid: sec-scripts-static-semantics-early-errors`, body `$DONOTEVALUATE(); function f(){} const f = 1;`
  (real Script path).
- `script-toplevel-lexical-then-function-redeclaration-early-error.js` — same, reversed order.
- `script-toplevel-function-class-redeclaration-early-error.js` — `function f(){} class f {}`.
- `script-toplevel-function-lexical-redeclaration-eval-matrix.js` — indirect and direct eval matrix of the
  negative forms (generator, async, async generator, labelled function, let/const/class both orders) using
  `assert.throws(SyntaxError, …)`; `esid: sec-scripts-static-semantics-early-errors`.
- `script-toplevel-function-declaration-still-legal.js` — the positive list from Slice 2 (duplicate
  function declarations sloppy and strict, `var`+function, block-nested function with outer `let`,
  inner-scope shadowing), asserting the program runs; `esid: sec-static-semantics-toplevelvardeclarednames`.
- `tests/call-non-callable-object-message.js` (Slice 3; diagnostics, not spec-mandated).

Non-engine gate for Slice 1: `uv run python -m unittest discover -s scripts -p 'test_*.py'`
plus the manual `run-jetstream.py --test cdjs` run above. The Node-compat shim gates
(`run-node-shim-selftest.sh`, `run-shim-fixtures.sh`) are only needed if a shim is touched (none planned).

## 6. Regression risk

- **Do not add `FunctionDeclaration` to the shared `collect_var_declared_names`.** It recurses through
  Block / If / Try / Switch / For bodies, and in those positions function declarations are *lexical*
  (`sec-block-static-semantics-early-errors`). Adding it there would make `let f; { function f(){} }`
  — legal today — a SyntaxError and regress annexB tests. The separate top-level collector avoids this;
  the Rust tests in Slice 2 pin the legal cases.
- **test262-pass.txt baseline:** expected effect is *no regressions*; possibly a few new passes among
  `language/global-code/`, `annexB/language/global-code/`, eval-code redeclaration negatives. Any newly
  failing test means the collector is over-inclusive — fix the collector, do not touch the baseline.
  The baseline is read from `origin/main`; nothing in this plan rewrites it (`--update-baseline` must not be used).
- **Newly-rejected programs in our own harnesses:** any script we concatenate that relied on the
  now-illegal shadowing would start failing with SyntaxError (this is what the fix is for). Check:
  (a) every benchmark in `run-jetstream.py`'s table still parses — build each concatenated script and
  run `node --check` on it, and run it briefly on jsse looking for `SyntaxError` (one-off, record result in PR body);
  (b) Node-compat library harnesses that prepend shims to bundles (`scripts/run-library-tests.sh`): run
  the fast ones (`uuid`, `prismjs`, and `decimal.js` if its cache is warm) — they are already
  Node-cross-checked, so any new jsse-only SyntaxError is a real finding, not a shim excuse.
- **Shared machinery touched:** parser only for Slice 2 (`parse_program` is also used by
  `eval`, `new Function` wrapper parsing, `--prelude` — all Script goal, all
  spec-required to apply this rule). No interpreter hot path (`eval_expr`/`exec_statement`), no property MOP,
  no GC rooting, no `ObjectKind` matches, no bytecode compiler changes. Slice 3 touches only the cold
  error-construction tail of `call_function_inner_impl`; ensure the helper does not allocate GC objects
  before `create_type_error` runs (strings only), so no new rooting is needed. Bytecode VM shares that tail,
  so message parity holds by construction; the added parity test pins it.
- The `--prelude` path (test262 harness) loads each prelude as its own Script, so existing harness
  files (`assert.js`, `sta.js`, `propertyHelper.js`, …) are unaffected; verified by the full test262 run.

## 7. Out of scope (deliberately not bundled; file follow-up issues via `gh issue create`)

1. **Function-body top-level early errors are entirely missing.** `function g(){ let f; let f; }`,
   `function g(){ let f; var f; }`, `function g(){ function f(){} let f; }`, arrow/generator bodies and
   `new Function("function f(){} let f;")` all parse today. Governed by
   `sec-function-definitions-static-semantics-early-errors` (BoundNames of FormalParameters vs
   LexicallyDeclaredNames is the only rule implemented, `declarations.rs:1380-1408`, and only for
   let/const, not class) plus TopLevel{Lexically,Var}DeclaredNames. The new top-level collector from
   Slice 2 is the natural building block — reuse it there, in its own PR. `parse_program` fixes
   script and `eval`, **not** the `Function` constructor path.
2. **`<expr> is not a function` with callee source text** (`o.foo is not a function`). jsse has never
   printed callee text — the issue's "usual `<expr>` text" does not exist today (`o.foo()` yields
   `undefined is not a function`). Threading the callee expression from `eval_call` (`eval.rs:4653`)
   to the throw site needs an optional description parameter through the many `call_function*`
   callers (which have no AST) and equivalent text in the bytecode call opcodes
   (`assert_message_parity` in `bytecode/tests.rs:470` pins tree-walker/VM equality). Larger and
   cross-cutting; Slice 3 removes the leak and keeps the shared tail as the single message site so that
   follow-up is additive.
3. `is not a constructor` messages in `eval_new` (`eval.rs:6592-6599`): `new ({})` prints an empty subject
   (`" is not a constructor"`) and a non-object callee prints `{:?}` of the value — same class of
   diagnostic problem, different site; leave for the message follow-up.
4. The `Symbol.iterator` / "next called on non-array iterator" theory in #52 (validatorjs es6 bundle
   path) — unrelated to cdjs; do not chase it here.
5. No formatting/refactor churn in `run-jetstream.py`, no perf work on cdjs (5 s/iteration is what it is),
   no `docs/perf/` report edits (historical), no `test262-pass.txt` movement.
