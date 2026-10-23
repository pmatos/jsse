# Plan: `execute_ast` fuzz target — arbitrary-driven JS AST generator (#1175)

## Problem restated
`execute` feeds raw libFuzzer bytes to the engine; most mutations die in the lexer/parser, so the interpreter, bytecode VM and builtins see few distinct programs. #1175 adds a structure-aware sibling: a compact JS AST in the `fuzz` crate deriving `arbitrary::Arbitrary`, a printer that renders it to source that nearly always parses, and a fuzz target that runs the printed source through the same in-process bytecode-vs-tree-walker check as `execute` (ADR 2026-10-23-1400). Acceptance: (1) record the parse-success rate; (2) target runs in ClusterFuzzLite and shares the divergence check; (3) record that its engine coverage beats raw-bytes `execute` on the same budget.

## Spec basis
N/A: no JavaScript behavior change. This adds fuzz tooling under `fuzz/`, `.clusterfuzzlite/` and docs; the printer only emits syntax the spec's grammar (ECMA-262 §13–15 Expressions/Statements/Functions) already defines, and the engine is not modified beyond a refactor of a fuzz helper.

## Assumptions (no operator to ask)
- Target name `execute_ast` (`gen` is a reserved keyword in edition 2024, so no `gen` module/ident).
- Scripts only (no modules), same as `execute`.
- The printer enforces context rules by construction (break/continue only inside loops/switch, return only in functions, yield/await only in generator/async bodies, distinct params, `var` for declarations) rather than relying on the parser to accept; that is how the "large majority parse" rate is reached. Target threshold: >= 90% parse in the unit test; the real measured number is recorded in the ADR.
- Reuse the existing `jsse::fuzz_execute_*` path so the divergence check is literally the same code, not a copy.
- `arbitrary` is already in `fuzz/Cargo.lock` (1.4.2, via libfuzzer-sys); add it directly with `derive` feature (pulls `derive_arbitrary`, lock file changes; `*.lock` is token-limit-ignored).
- `CONTEXT.md` is over the 5,000-token limit (31 KB; files already over may not grow), so new terms go in the ADR, as ADR 2026-10-23-1400 did.
- Per-file cap is 20,000 bytes (`scripts/check_staged_file_tokens.py`); `src/lib.rs` is 15 KB, so the engine edit must stay small and AST/printer code is split across several `fuzz/src/` modules.

## Files to touch
| File | Role | Lines of interest |
|------|------|-------------------|
| `src/lib.rs` | `fuzz_execute_bytes` (L~289) holds the run+compare+panic body; extract `fuzz_execute_source(&str)` | `fuzz_execute_signatures`, `execution_divergence`, `fuzz_execute_bytes` |
| `fuzz/Cargo.toml` | add `arbitrary` dep + `[[bin]] execute_ast` | existing `[[bin]]` blocks |
| `fuzz/src/lib.rs` | add `pub mod js_ast; pub mod js_print;` | top of file; existing `#[cfg(test)] mod tests` at L232 |
| `fuzz/src/js_ast.rs` [new] | AST types deriving `Arbitrary` | – |
| `fuzz/src/js_print.rs` [new] | printer: context, statements | – |
| `fuzz/src/js_print_expr.rs` [new] | printer: expressions (split to stay under the 20 KB per-file cap) | – |
| `fuzz/tests/ast_target.rs` [new] | integration tests (parse rate, pool-marker check, worst-case recursion) kept out of the src files for the file cap | – |
| `fuzz/fuzz_targets/execute_ast.rs` [new] | target: `Program` -> source -> `jsse::fuzz_execute_source` | mirror `fuzz/fuzz_targets/execute.rs` |
| `fuzz/corpus/execute_ast/*` [new] | a handful of small binary seeds (pre-commit excludes `fuzz/corpus/`) | – |
| `.clusterfuzzlite/build.sh` | add to `TARGETS`; zip seed corpus | `TARGETS=(parse execute)` |
| `docs/adr/2026-10-23-HHMM-execute-ast-fuzz-target.md` [new] | decision + recorded measurements (parse rate, coverage comparison) | follow `2026-10-23-1400-execute-fuzz-target.md` |
| `README.md` ("Fuzzing" section L173–212) | target list, run command, CI table rows | – |
| `.claude/skills/fuzzing/SKILL.md` | add target to run/replay/triage text and the coverage-comparison recipe | – |
| `docs/adr/2026-10-23-1400-execute-fuzz-target.md` | cross-link its "grammar-aware mutator" follow-up as done | Consequences |

Not touched: `.github/workflows/clusterfuzzlite-*.yml` (they build/run every target `build.sh` emits; `fuzz-seconds` is split between targets automatically), `fuzz.yml`, `CONTEXT.md`, `test262-pass.txt`.

## TDD slices (ordered, each red -> green)

### 1. Extract the shared run+compare entry point
- **File**: `src/lib.rs`
- **Test (red first)**: in `lib_tests`, `fuzz_execute_source_accepts_equal_modes` runs `"var a = [1,2,3].map(x => x * 2); a.join();"`, and `fuzz_execute_source_skips_unsafe_idents` runs `"$262.agent.start('')"`-style source containing `agent` and returns without hanging. Fails to compile until the function exists.
- **Change**: add `pub fn fuzz_execute_source(src: &str)` containing the `run_on_engine_stack` + `fuzz_execute_signatures` + `execution_divergence` + `panic!("execution divergence ...")` body currently in `fuzz_execute_bytes`; `fuzz_execute_bytes` becomes `from_utf8` then `fuzz_execute_source`. Behavior identical (same panic text prefix `execution divergence`).
- **Reuses**: `fuzz_execute_signatures`, `execution_divergence`, `run_on_engine_stack`.
- **Check**: existing `execute` seed replay still passes (Step 7 replay).

### 2. Add dependency and target skeleton
- **Files**: `fuzz/Cargo.toml` (`arbitrary = { version = "1", features = ["derive"] }`; `[[bin]] name = "execute_ast", path = "fuzz_targets/execute_ast.rs", test/doc/bench = false`), `fuzz/Cargo.lock` regenerated by cargo.
- **Verify**: `cd fuzz && cargo check` (fuzz is its own workspace; use `-j2`).

### 3. AST (`fuzz/src/js_ast.rs` [new])
- **Test (red)**: `fuzz/src/js_ast.rs` unit test `arbitrary_from_empty_and_short_input_yields_terminal_program` — `Program::arbitrary(&mut Unstructured::new(&[]))` returns `Ok` (every recursive enum's *first* variant is a leaf, because `Unstructured` returns the lowest choice when bytes run out; that is what terminates recursion).
- **Change**: compact types, all `#[derive(Debug, Clone, Arbitrary)]`:
  - `Program { stmts: Vec<Stmt> }`.
  - `Stmt`: `Expr(Expr)` (first/leaf), `Var(VarId, Option<Expr>)`, `If`, `For`, `ForIn`/`ForOf`, `While`, `DoWhile`, `Block`, `Try{block,catch,finally}`, `Throw`, `Return`, `Break`, `Continue`, `Switch`, `Labeled`, `FuncDecl`, `ClassDecl`.
  - `Expr`: `Num(i8-ish)` (first), `Str(StrId)`, `Bool`, `Null`, `Undef`, `Ident(VarId)`, `Global(GlobalId)` (pool of safe builtins: `Array, Object, Map, Set, WeakMap, Promise, Symbol, Reflect, Proxy, JSON, Math, Number, String, BigInt, RegExp, Error, TypeError, Int8Array/Uint8Array/Float64Array, ArrayBuffer, DataView, Function, parseInt …`), `Unary`, `Binary`, `Logical`, `Assign{op,target,value}`, `Update`, `Cond`, `Call`, `New`, `Member{obj,prop:PropId}`, `Index`, `Array`, `Object`, `Arrow`, `FuncExpr`, `ClassExpr`, `Template`, `Seq`, `Spread-in-call`, `Await`, `Yield`, `Typeof`, `OptChain`, `TaggedTemplate`, `RegExpLit(RegexId)`, `BigIntLit`.
  - Small index newtypes (`VarId`, `PropId`, `StrId`, `GlobalId`) are `u8` reduced modulo a fixed pool at print time, so they never produce an invalid identifier and the pools never contain the `execute` skip/non-compare idents (`agent`, `waitAsync`, `sleep`, `Date`, `Temporal`, `WeakRef`, `FinalizationRegistry`, `$262`, `Atomics`, `SharedArrayBuffer`, `performance`, `gc`) — asserted by a test in step 4. Property pool includes `length, prototype, constructor, __proto__, toString, valueOf, then, next, return, throw, get, set, 0, 1, Symbol.iterator`-style entries; string pool includes edge strings.
  - Function/class bodies hold `Vec<Stmt>`; params are a `u8` count (0..=3) printed as distinct pool names.
- Keep every file < 20,000 bytes (CI `build` enforces it): types in `js_ast.rs`, statement printing in `js_print.rs`, expression printing in `js_print_expr.rs`, tests in `fuzz/tests/ast_target.rs`. `fuzz/src/lib.rs` (12.7 KB) only gains `pub mod` lines.

### 4. Printer (`fuzz/src/js_print.rs`, `js_print_expr.rs` [new])
- **Tests (red first)**, in `fuzz/tests/ast_target.rs` (pub API of `jsse_fuzz`):
  1. `printed_programs_parse_in_the_engine`: N=2,000 deterministic xorshift buffers (fixed seed, sizes 16-2048) -> `Program::arbitrary` -> `print` -> `jsse::bench_parse` inside `jsse::run_on_engine_stack`; assert rate >= 0.90 and `eprintln!` the exact rate (recorded in the ADR); on failure print the first 3 failing sources.
  2. `pools_contain_no_runner_marker_identifiers`: every pool (identifiers, globals, properties, strings, regex sources, labels, class names) is checked, with the identifier-boundary match mirrored from `contains_ident` / `MARKERS` in `scripts/gen-fuzz-execute-corpus.py`, against `agent, waitAsync, sleep, Date, DateTimeFormat, Temporal, WeakRef, FinalizationRegistry, $262, Atomics, SharedArrayBuffer, performance, gc`. It does NOT run the engine (a divergence or panic must not turn a generator unit test red). Also keep `"use strict"` out of the string pool.
  3. `worst_case_input_does_not_overflow_the_stack`: a full 4096-byte buffer of `0xFF` / alternating selectors that keeps picking recursive variants; `Program::arbitrary` + `print` run on a thread with an 8 MiB stack (libFuzzer's thread) and must return. First confirm in `arbitrary` 1.4.2 source whether the derive has a recursion guard (it does not recurse unboundedly only because exhausted `Unstructured` yields lowest choices); if depth still grows large, bound it by hand-writing `Arbitrary` for `Expr`/`Stmt` with a depth parameter, not by trusting the derive. ASan inflates frames, so also run this test under `cargo +nightly fuzz` with the real target in Step 5.
  4. Targeted printer tests (each a minimal AST -> expected source property, parsed with `bench_parse`): see the rules below.
- **Change**: `pub fn print(program: &Program) -> String` with `Ctx { depth, in_loop, in_switch, in_fn, in_gen, in_async, labels, class_names }` threaded through. Rules (each a guaranteed early SyntaxError otherwise):
  - Hoisted prologue `var a, b, c, d, e, f;` covers the whole variable pool; `Var` statements print as `var`, never `let`/`const`, so no redeclaration.
  - Class and function declaration names come from pools **disjoint** from the `var` pool, and each is unique within its scope (counter-suffixed, e.g. `C0`, `C1`) - `var a; class a {}` and two `class C{}` in one block are errors.
  - Sub-statement bodies of `if`/loops/`with`-free labeled statements always print as `{ ... }`; a `FuncDecl`/`ClassDecl` is never a bare sub-statement.
  - `break` only inside loop/switch; `break L` / `continue L` only when `L` is in scope, and `continue L` only when `L` labels an *iteration* statement; no nested duplicate labels. Violations degrade to an expression statement.
  - Entering any function, arrow, method, getter/setter or class static block resets `in_loop`, `in_switch`, `labels`, `in_gen`, `in_async` (arrow and plain functions set `in_fn`, generators set `in_gen`, async sets `in_async`); `return` only when `in_fn`; `yield`/`await` degrade to their operand outside generator/async.
  - `Assign` and `Update` targets are a restricted `Target` enum (`Ident`, `Member`, `Index`) so `f() = 1` is unrepresentable; `delete` of a bare identifier is never emitted (class bodies are strict).
  - Template/string/regex content escapes `` ` ``, `${` and `\`; regex literals come from a fixed pool of valid patterns, flags `g,i,m,s,u,y` combos that are valid together (no `u`+`v`).
  - Every operand is parenthesized (no precedence/ASI hazards), unary before `**` is parenthesized, statements end with `;`, depth cap `MAX_DEPTH` (12) prints a literal, output soft-capped at ~16 KB of source.
  - No top-level `try` wrapper: a throw is a valid outcome and both modes must agree on it.
- **Reuses**: `jsse::bench_parse` (`src/lib.rs`, `#[doc(hidden)] pub`), `jsse::run_on_engine_stack`.
- **Run**: `cd fuzz && cargo +nightly test -j2` (bins have `test = false`; lib + `tests/` run). These tests are local-only: `ci.yml` never runs anything under `fuzz/`.

### 5. Fuzz target (`fuzz/fuzz_targets/execute_ast.rs` [new])
- Body: `fuzz_target!(|program: jsse_fuzz::js_ast::Program| { jsse::fuzz_execute_source(&jsse_fuzz::js_print::print(&program)); });` (libfuzzer-sys implements `Arbitrary` input directly). Add the module doc comment naming the shared check.
- **Check**: `cargo +nightly fuzz build execute_ast`, then `cargo +nightly fuzz run execute_ast -- -runs=0 fuzz/corpus/execute_ast` and a short mutation run (`-max_total_time=60 -timeout=30`) at the CI flags (`--debug-assertions`).

### 6. Seeds and ClusterFuzzLite wiring
- `fuzz/corpus/execute_ast/`: 6–10 small binary files (e.g. `head -c N /dev/urandom`-style but committed deterministically; pick ones whose printed form exercises functions/classes/loops/try/switch/generators — verify with a throwaway test that prints them, do not commit the test). Non-empty so `zip -jq ... fuzz/corpus/$target/*` in `build.sh` succeeds.
- `.clusterfuzzlite/build.sh`: `TARGETS=(parse execute execute_ast)`; update the leading comment; keep the `execute.options` heredoc and add `execute_ast.options` (`max_len = 4096`, explicit so it does not silently follow the 16384 of `execute` — structured inputs need no harness-sized seeds).
- Workflows unchanged. Validate with `bash -n .clusterfuzzlite/build.sh` and `./scripts/lint.sh` (workflow-lint covers the unchanged yml). Real CI proof is the PR's `ClusterFuzzLite PR` check building and running the new target; note this in the PR body.

### 7. Measure and record (acceptance 1 and 3)
- Parse rate: from Step 4's test output (also run once at 20,000 samples); record the number.
- Coverage comparison, same budget, same ASan-free local build, fixed seed: for each of `execute` (seeded from `fuzz/corpus/execute`) and `execute_ast` (seeded from its corpus) run `cargo +nightly fuzz run <t> -- -max_total_time=300 -seed=1 -timeout=30` into a scratch corpus under `$TMPDIR`; then `cargo +nightly fuzz coverage <t> <scratch-corpus>` and `cargo cov -- report`/`llvm-cov report` (needs `rustup component add llvm-tools-preview`; if unavailable, fall back to libFuzzer's final `cov:` edge counts from the `-max_total_time` runs and say so) restricted to `src/interpreter/` and `src/parser/`. Record line/region coverage of both side by side in the ADR, plus execs/sec. Also run `execute_ast` with an *empty-seed* corpus to show the generator, not the seeds, drives the gain. If `execute_ast` does **not** exceed `execute`, record the data honestly and still ship (state it in the PR, add a follow-up for dictionary/weighting), per the unattended contract; do not tune toward the number by special-casing.
- Put the recipe in `.claude/skills/fuzzing/SKILL.md` ("Comparing target coverage").

### 8. Docs
- New ADR `docs/adr/2026-10-23-<HHMM>-execute-ast-fuzz-target.md`: Status accepted (#1175, part of #1168); Context (raw bytes mostly fail to parse); Decision (types in `fuzz` crate not engine; printer context rules; first-variant-leaf recursion rule; pools exclude skip/nondeterministic idents; shares `fuzz_execute_source`; seeds; `max_len`); measured parse rate and coverage table; rejected alternatives (generator in the library crate — exposes no engine internals and would bloat `src/lib.rs` past the token cap; AST->bytes mutator via custom mutator — more complex than `Arbitrary`; running with `node` — wrong oracle); follow-ups (module goal, weighted/dictionary generation, async-ordering programs, a test262-derived seed expander).
- `README.md` Fuzzing section: four targets, `cargo +nightly fuzz run execute_ast -- -max_total_time=60` command, two table rows (`clusterfuzzlite-pr.yml` / `-batch.yml`, shared with `parse` and `execute`), corpus path mention.
- `.claude/skills/fuzzing/SKILL.md`: add `execute_ast` to the target list, replay/new-mutation commands, CI list, and the triage note (a divergence on `execute_ast` is triaged exactly like `execute`; minimize the *printed source*, not the Arbitrary bytes: print it, `tmin` the source via `execute`).
- Add a one-liner to the "Follow-ups" bullet of ADR 2026-10-23-1400 pointing at the new ADR.

## Test surface
- No `test262/test/...` directory exercises this change (no engine syntax/semantics changed) and no `test262-extra/` test applies. The gates are: `cd fuzz && cargo +nightly test -j2` (fuzz-crate unit + integration tests; local-only, not run by CI), corpus replay of all four targets, the Step 1 engine unit tests, and the blocking `ClusterFuzzLite PR` check that builds and runs the new target.
- `cargo test --profile release-nolto fuzz_execute -j2` for the Step 1 engine unit tests (or plain `cargo test fuzz_execute`; nextest not required).
- Replay all four corpora: `cargo +nightly fuzz run {parse,execute,execute_ast} -- -runs=0 ...` (execute with `-max_len=16384`).
- `./scripts/lint.sh`; `./scripts/ensure-hooks.sh` before committing; commit via normal hooks (no `--no-verify`).
- test262: the engine change is a pure extraction in fuzz-only code, not on any test262 path. Run `uv run python scripts/run-test262.py test262/test/language/expressions/addition/` as a smoke check only if the build is already available; no baseline movement is expected and `test262-pass.txt` is not touched. No new `test262-extra/` test: there is no ECMAScript behavior change.
- Build with `-j2`/`CARGO_BUILD_JOBS=2`, scratch under `$TMPDIR`, fuzz runs foreground with explicit time bounds.

## Regression risk
- `fuzz_execute_bytes` refactor: the only shared-machinery touch. It must keep the `execution divergence` panic prefix (the skill/triage docs and ClusterFuzzLite dedup depend on it) and the engine-stack wrapper; covered by the replay of `fuzz/corpus/execute`.
- No change to the interpreter, parser, GC, property MOP or bytecode paths; `test262-pass.txt` and library harnesses unaffected.
- CI: the `ClusterFuzzLite PR` check is blocking and shares a fixed 120 s between three targets now, so each gets less time; a latent bug the new generator finds can fail an unrelated PR (same caveat as the `execute` ADR). Mitigation: run 5+ minutes locally at CI flags before pushing and file any finding as its own issue instead of fixing it here.
- Generator-produced resource-limit hits (huge `repeat`, regex backtracking, exponential recursion) surface as libFuzzer timeout/oom artifacts: keep the property/string/number pools small and omit `repeat`/`padStart` with large literal arguments from the pools, so findings stay about engine bugs.
- Unbounded recursion in derived `Arbitrary`/printer: bounded by input `max_len` (4096) and the printer's depth cap; checked by a deep-input unit test.
- Token-limit rule: new files each < 20,000 bytes; `src/lib.rs` grows by a few lines only (15 KB now).

## Out of scope
- Module-goal programs, `eval`/`Function` constructors of generated strings, async-ordering/timer programs, Temporal/Intl generation.
- A custom libFuzzer mutator, dictionaries, grammar weighting, corpus pruning/coverage CI mode, a `storage-repo`.
- Moving `differential` into ClusterFuzzLite; changing `fuzz-seconds` or workflow YAML.
- Fixing any engine bug the new target finds (file separately).
- Edits to `CONTEXT.md` (over its size limit), `spec/`, `test262/`, `test262-pass.txt`.
