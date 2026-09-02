# Plan: issue #524 — bytecode compiler eligibility covers <10% of called functions

## 1. Problem restated

`src/interpreter/bytecode/compiler.rs` compiles a function or script body
*atomically*: `compile_body`/`compile_script_body` walk the statement list
once, and the first AST node `compile_statement`/`compile_expr` doesn't
recognize propagates a `CompileError::Unsupported` that fails the **whole
body** (`dispatch_body`, `src/interpreter/eval.rs:1966-2037`, caches the
outcome per function object as `BytecodeCacheState::Compiled`/`Ineligible`
— compiled or tree-walked forever after, never partial). The supported
subset is narrow: statements `Empty/Expression/Block/Variable(var only)/
If/While/For/Return`; expressions `Literal/Identifier/Unary/Sequence/Comma/
Assign(identifier or member target)/Update(identifier target only)/Logical/
Conditional/Void/Binary/Member/Call(bare-identifier callee only)`. Everything
else — `let`/`const`, `throw`/`try`, `switch`, `do`/`while`, `for-in`/`of`,
`break`/`continue`, `this`, `new`, object/array literals, method-call callees
(`obj.m()`) — bails the containing body entirely. Real-world code is
saturated with these constructs, so the compiler reaches almost none of it:
the issue quotes 9.6% function coverage on typescript-octane and 0.8% on
OfflineAssembler.

The issue's own suggested expansion order (`throw`/`try` first, as "expected
biggest real-code blocker") was **explicitly a guess** — no bail-reason data
existed when the issue was filed. That data now exists (see below), and it
contradicts the guess. This plan's first slice follows the measured order,
not the guessed one.

## 2. Spec basis

This PR compiles existing tree-walker semantics into bytecode; it does not
change JavaScript behavior. Every clause below is cited from the pinned
`spec/spec.html` (tc39/ecma262 submodule) by `emu-clause` id, since the raw
source carries no rendered section numbers.

The first slice (§4) adds `this`-expression support to the compiler:

- **`sec-this-keyword`** — "The `this` Keyword": `PrimaryExpression : this` →
  `Return ? ResolveThisBinding()`.
- **`sec-resolvethisbinding`** / **`sec-getthisenvironment`** — abstract
  operations `ResolveThisBinding` / `GetThisEnvironment`, which walk the
  environment chain to the nearest Function/Global/Module Environment Record
  carrying a `this` binding.
- **`sec-function-environment-records`** — defines the `[[ThisValue]]` /
  `[[ThisBindingStatus]]` fields; a *derived* class constructor's Function
  Environment Record starts `[[ThisBindingStatus]]` as `lexical`/uninitialized
  until `super()` runs, which is the TDZ-for-`this` case exercised in §4's
  regression test.
- jsse models this via an ordinary named binding `"this"` declared with
  `env.declare("this", ...)` (`src/interpreter/mod.rs:2297,3020,3246`) and
  resolved through the same TDZ-checked path as `let`/`const`
  (`Interpreter::resolve_identifier`, `src/interpreter/eval.rs:7966`, throws
  `"Cannot access 'this' before initialization"` — a `ReferenceError`, which
  is all the spec requires; message text is implementation-defined) — this is
  jsse's existing implementation strategy already in force for the
  tree-walker's `Expression::This` arm (`src/interpreter/eval.rs:447-461`)
  and is not something this PR introduces or changes.

The follow-up list (§ below) touches `sec-break-statement`,
`sec-continue-statement`, `sec-labelled-statements` (break/continue slice),
and other clauses named inline per item — cited there rather than here since
they are not part of this PR.

## 3. Files to touch

Engine:
- `src/interpreter/bytecode/compiler.rs` — add a `compile_expr` arm for
  `Expression::This`. Primary approach (TDD-decided, see §4): reuse the
  existing `Op::LoadName` opcode with the literal name `"this"`, since `"this"`
  is a genuine environment binding and `Op::LoadName` already calls
  `resolve_identifier`, which already implements the generic TDZ check the
  derived-constructor case needs. Only add a new `Op::LoadThis` (touching
  `src/interpreter/bytecode/op.rs` and `src/interpreter/bytecode/vm.rs`) if a
  parity test in §4 slice 1 finds a real behavioral divergence from the
  tree-walker's dedicated `Expression::This` arm (`src/interpreter/eval.rs:
  447-461`) that reuse can't cover.
- `src/interpreter/bytecode/tests.rs` — new tests (§4).
- `src/interpreter/perf_counters.rs` — **only** if a new opcode is added: the
  compile-time bound `assert!((Op::ReturnCompletion as usize) < OP_SLOTS)`
  (line 33) is pinned to `Op::ReturnCompletion`'s discriminant specifically,
  not to "the highest variant" generically — it silently stops catching
  overflow once a later-added variant becomes the true max. Not exercised by
  this slice (52 of 64 slots used today; one more opcode is still 53 < 64),
  but flag it so the assert doesn't quietly go stale.

Docs (new, following the existing `docs/specs/2026-07-25-...`/
`2026-07-26-...` bytecode-slice precedent):
- `docs/specs/2026-09-02-bytecode-this-expression-slice.md` — context,
  approaches considered (LoadName reuse vs. dedicated opcode), and why
  `Super`/`NewTarget` (same env-binding shape) are left for their own slices.
- `docs/perf/2026-09-02/typescript-octane-bail-report.md` and
  `docs/perf/2026-09-02/offlineassembler-bail-report.md` — the bail-reason
  measurements gathered for this plan (§ Measurement below), completing the
  3-workload table the issue's "Prerequisite step" asked for. Include the raw
  `counters-*.txt` dumps alongside, matching
  `docs/perf/2026-08-26/counters-{default,bytecode}.txt`'s pattern.
- `README.md` — only if the test262 pass count changes after a full run
  (unlikely here since `bytecode_enabled` defaults to `false`; see §6).

Nothing under `spec/`, `test262/`, `.github/`, or `scripts/` needs to change
for this slice.

## 4. TDD slices

### Slice 0 — measurement (no source change, already executed for this plan)

The issue's "Prerequisite step" (bail-reason counters) is **already built**:
`src/interpreter/perf_counters.rs` has `record_bail`, a `compile_bail:
FxHashMap<&'static str, u64>`, and per-body `bail_by_name` attribution, wired
into `dispatch_body` (`eval.rs:1995-2003`) and `exec_script_body`
(`exec.rs:95-108`). It already produced a full report for mandreel
(`docs/perf/2026-08-26/mandreel-bytecode-work-share.md`). What was missing —
and is the actual remaining prerequisite — was the same report for the other
two workloads the issue names. Both were run this session:
`cargo build --release --features perf-counters`, then each workload's
JetStream harness bundle (built the same way `scripts/run-jetstream.py`
does, via `build_polyfill_preamble`/`build_sync_harness`) through
`target/release/jsse --bytecode`, 1 outer iteration (compile attempts are
cached per function object, so bail *reasons* don't scale with iteration
count — 1 outer iteration reaches every function invoked at least once).

**typescript-octane** (`compile_ok` 1188, `compile_bail` 11213 — matches the
issue's originally-quoted 1,187/11,211 almost exactly, confirming
reproducibility):

| bail reason | count |
| --- | --- |
| `call callee` (method-call target, `obj.m()`) | 10360 |
| `expression:This` | 307 |
| `nested tail call` | 184 |
| `statement:FunctionDeclaration` (nested) | 165 |
| `statement:Switch` | 86 |
| `expression:Array` | 39 |
| `expression:New` | 39 |
| `expression:Typeof` | 12 |
| `assign target` | 8 |
| `expression:Function` / `expression:Object` / `literal` / `statement:Continue` / `update target` | 2 each |
| `lexical declaration` / `statement:ForIn` / **`statement:Try`** | 1 each |

By exclusive tree-walker work share (the `BODY` rows, ranked — invocation
count alone misleads, per the mandreel doc), the top 20 bodies split roughly
34% `call callee`, 19% `expression:This`, 5% `update target`, 4%
`expression:New`, 4% `expression:Typeof` — `call callee` and `expression:This`
dominate by both metrics.

**OfflineAssembler** (`compile_ok` 3, `compile_bail` 4428 — differs sharply
from the issue's originally-quoted 1/123; plausibly because this parser
benchmark creates fresh closures per parse, so compile-attempt volume tracks
how much of the workload actually ran rather than being a fixed per-source
count. The *proportions* are what matter for prioritization, and they agree
directionally with the issue's own 123-bail sample):

| bail reason | count |
| --- | --- |
| `call callee` | 4313 |
| `expression:This` | 91 |
| `lexical declaration` | 15 |
| `binary op` (unsupported `in`/`instanceof`) | 3 |
| `nested tail call` | 3 |
| `expression:Object` / `expression:Typeof` / `statement:FunctionDeclaration` | 1 each |

Top exclusive-work body: `lex` (the tokenizer), 25% of all tree-walker work,
blocked on `statement:FunctionDeclaration` (a nested function declaration).
Next: an anonymous body at 14.2%, blocked on `binary op` (`in`/`instanceof`).

**mandreel** (from `docs/perf/2026-08-26/mandreel-bytecode-work-share.md`,
not re-measured): dominant reason is `statement:Labeled` (47/103 bails,
69.5% of exclusive work — labeled `break`/`continue` in Emscripten-style
`while(true)` loops), then `call callee` (39). `statement:Try` is **1**.

**Cross-workload conclusion**: `call callee` (method-call callees) is the
single largest bail reason on 2 of 3 measured workloads by both count and
work-share, and a strong second on the third. `expression:This` is the only
other reason present in *all three* workloads' bail tables (4 / 91 / 307).
`throw`/`try` — the issue's guessed "biggest blocker" — is **1 bail or fewer
on every workload measured**, out of a combined ~15,700 compile attempts.
The guessed order is wrong; this plan follows the measured order instead.

### Slice 1 — `Expression::This` compiles (this PR)

`this` is chosen over the higher-count `call callee` because full method-call
support needs the IC plumbing the issue itself flags as deferred from #398
(the bytecode VM's `Expression::Member` compile arm already ignores its
`_site_id`, `compiler.rs:374` — no bytecode-side IC consumer exists yet; see
ADR 0001). `this` has no such dependency, needs (at most) one new opcode with
12 free slots to spare (52/64 used), and unlocks a real subset of bodies on
its own (property reads via `this.x` that never call `this.method()`) while
being a direct prerequisite for the method-call slice later (OO method
bodies almost always read `this` before or alongside calling on it).

1. **Red**: `src/interpreter/bytecode/tests.rs` — add
   `this_expression_compiles_and_matches_tree_walker`, following the
   `assert_parity_number` pattern (`tests.rs:769-776`):
   ```rust
   let source = "function f(){ 'use strict'; return this === undefined ? 1 : 2; } \
                  var __r = f();";
   assert_parity_number(source, 1.0);
   ```
   `f` is declared strict via its own directive prologue, so its `this`
   binding is not substituted with the global object regardless of how it's
   called (`OrdinaryCallBindThis`) — deterministic across caller context.
   Currently fails: `Expression::This` hits `compile_expr`'s catch-all
   (`compiler.rs:396`), the body bails with `expression:This`, `bc_count`
   stays 0, and `assert!(bc_count >= 1, ...)` fails.
   **Green**: add the `compile_expr` arm compiling `Expression::This` to
   `Op::LoadName("this")` (reusing `add_name`/`emit_resolve`-style plumbing
   already used for `Expression::Identifier`, `compiler.rs:233-239`).

2. **Red**: add a second parity case in the same test — sloppy-mode `this`
   substitution (`f2` without `'use strict'`, called bare, `this` becomes the
   global object, not `undefined`) — to catch a case where the compiled path
   might diverge from the tree-walker's dedicated `Expression::This` handling
   (`eval.rs:447-461`) if `resolve_identifier`'s generic path treats a
   missing/never-substituted binding differently. **Green**: confirmed by the
   existing substitution logic (that logic runs before `dispatch_body`, in
   `OrdinaryCallBindThis`-equivalent setup, not in the compiled path itself,
   so this should already pass once slice 1's arm exists — the test exists to
   *prove* that, not to require new production code).

3. **Red**: `derived_constructor_this_before_super_still_throws_reference_error`
   — a derived class constructor reading `this` before calling `super()`
   must still throw `ReferenceError`, in **both** modes, matching today's
   tree-walker behavior exactly (this is the TDZ case from §2). Use the
   completion-based helper pattern (`eval_script_completion_with_mode`,
   `tests.rs:42-50`) rather than `assert_parity_number` (this is a `Throw`
   completion, not a return value):
   ```js
   class Base {}
   class Derived extends Base {
     constructor() { this.x = 1; super(); }
   }
   var __r = 0;
   try { new Derived(); } catch (e) { __r = (e instanceof ReferenceError) ? 1 : 2; }
   ```
   This body's `super()` call is `Expression::Call(Expression::Super, ...)`
   (`ast.rs:930`), which already bails via `compile_call`'s bare-identifier
   check (`compiler.rs:407-409`, reason `"call callee"`) regardless of this
   slice — so in practice this class body was *already* falling back to the
   tree-walker before this change. The test exists to pin that down as a
   permanent invariant: if a later slice (e.g. the method-call-callee
   follow-up) ever makes `super()` compilable, this test must still pass,
   which means the TDZ throw has to keep working through whatever opcode
   ends up loading `this`. No production change expected from this step
   alone; it is a regression pin, not a new-behavior driver.

4. **Refactor**: run `./scripts/lint.sh` and re-check `statement_kind`/
   `expression_kind`'s exhaustive match still compiles (it will — removing
   `Expression::This` from the catch-all doesn't remove its arm from
   `expression_kind`, which is a separate exhaustive function used only for
   bail-reason labeling and stays unchanged).

If step 1's parity test reveals `resolve_identifier`'s generic "missing
binding" behavior (throws `ReferenceError: this is not defined`) diverges
from `Expression::This`'s dedicated fallback-to-`undefined` case
(`eval.rs:456-458`, reachable only if no `"this"` binding exists *anywhere*
in the chain — which shouldn't happen given jsse installs one at every
global/module/function scope, but must be verified rather than assumed): add
a dedicated `Op::LoadThis` instead, replicating the tree-walker's exact
`env.borrow().get("this")` + `this_is_in_tdz` logic in
`src/interpreter/bytecode/vm.rs`. Document whichever path was taken, and why,
in `docs/specs/2026-09-02-bytecode-this-expression-slice.md`.

## 5. Test surface

- `src/interpreter/bytecode/tests.rs` is the primary gate — the parity tests
  in §4 directly assert tree-walker/bytecode agreement, which is an internal
  engine-consistency property, not a spec-compliance one, so it belongs here
  rather than in `test262-extra/`.
- `test262/test/language/expressions/this/` — run targeted, in both modes,
  to confirm no regression: `uv run python scripts/run-test262.py
  test262/test/language/expressions/this/` and again with `--bytecode`
  (`scripts/run-test262.py` already supports `--bytecode`, passed straight to
  the `jsse` binary — confirmed at `scripts/run-test262.py:625-627`).
- `test262/test/language/statements/class/subclass/` — targeted `--bytecode`
  run, to exercise the TDZ-`this` case at scale beyond slice 1's one
  hand-written test.
- `test262/test/language/statements/function/` — targeted `--bytecode` run
  for sloppy/strict `this`-substitution edge cases (global object
  substitution, `undefined`/`null` `thisArg` handling via `Function.prototype.
  call`/`apply` — though `.call`/`.apply` themselves are method calls and so
  still bail the *caller's* body, this only concerns the *callee* body's own
  `this` read).
- No `test262-extra/` addition needed: nothing here is spec-correct-but-
  untested behavior: it's compiling existing, already-tested tree-walker
  semantics into a second execution path.
- `cargo test --release` (per `fmt-hook-clippy-gate` — the crate is bin-only,
  so `cargo test --bin jsse`) covers `src/interpreter/bytecode/tests.rs` and
  `perf_counters.rs`'s own unit tests.

## 6. Regression risk

- **`test262-pass.txt` baseline risk is near zero for this slice.**
  `bytecode_enabled` defaults to `false` (`tests.rs:17-19`), and the tracked
  baseline is produced by the default `uv run python scripts/run-test262.py`
  invocation, which never passes `--bytecode`. This change is invisible to
  that run. Risk is scoped entirely to the `--bytecode`-gated path, covered
  by the targeted `--bytecode` reruns in §5 and the internal parity tests —
  not by the tracked baseline.
- **Shared machinery**: the new arm calls `resolve_identifier`
  (`eval.rs:7966`) — the same function every `Op::LoadName` and
  `Expression::Identifier` already calls. This slice adds a caller, not new
  behavior to that function; existing identifier-resolution tests are an
  indirect regression net for it.
- **GC rooting**: `Op::LoadName`'s existing push-to-stack path
  (`vm.rs:235-244`, `push_value`) already handles rooting for arbitrary
  `JsValue`s including objects (`this` is usually an object). No new rooting
  surface if the LoadName-reuse approach is used; if a dedicated `Op::LoadThis`
  is added instead, it must root exactly the same way — flag this explicitly
  in code review.
- **`ObjectKind`/GC exhaustive matches**: untouched — no new `JsObjectData`
  variant.
- **Bytecode fast path caching** (`BytecodeCacheState`): unaffected in shape;
  this slice only changes which bodies land in `Compiled` vs `Ineligible`,
  not the caching mechanism itself.
- **Node-compat library harnesses** (`scripts/run-library-tests.sh`): all
  currently green libraries run without `--bytecode` by default (checked:
  none of the harness configs in `scripts/libs/` pass it), so this slice
  doesn't touch their gate. Worth a manual `--bytecode` spot-check on one
  OO-heavy library (e.g. `acorn`) post-merge, but not required for this PR's
  gate.
- **`perf_counters.rs` OP_SLOTS footgun** (§3): not triggered by this slice
  (no new opcode expected on the primary path), but leave a code comment if
  a new opcode *is* added, since the existing compile-time assert won't catch
  a future overflow past the newly-added variant either.

## 7. Out of scope

Deliberately not in this PR — the measured-priority follow-up list, replacing
the issue's guessed order:

1. **Method-call callees (`obj.m()`) + bytecode-side inline caching.**
   Measured dominant bail reason on typescript-octane (92% of bails) and
   OfflineAssembler (97%). Needs the IC plumbing deferred from #398 — the
   compiled `Expression::Member` arm doesn't consume `_site_id` at all today
   (`compiler.rs:374`), so property/call-site caching for compiled code is a
   real design gap, not a small addition. Builds directly on this PR's `this`
   support.
2. **Labeled/unlabeled `break`/`continue` in compiled loops** (+ `do-while`,
   which shares the same loop-context machinery). Measured dominant bail
   reason on mandreel (`statement:Labeled`, 69.5% of exclusive work, `docs/
   perf/2026-08-26/mandreel-bytecode-work-share.md` projects ~-10% wall time).
   Needs zero new opcodes (`break`/`continue` lower to `Jump`s against a
   loop-context stack) but is a distinct, self-contained slice from `this`.
   Spec: `sec-break-statement`, `sec-continue-statement`,
   `sec-labelled-statements`.
3. **`let`/`const` (TDZ)**. Present in all three measured workloads (1 + 15 +
   1 = 17 bails) but low count everywhere; needs a compile-time
   uninitialized-binding model, not just an opcode.
4. **`new Foo()`**. 39 bails on typescript-octane (~4% of top-body work), 6 on
   mandreel (~7.6% of `initHeap`/`my_mandreel_call_constructors`'s work).
   Needs a `Construct`-shaped opcode plus object/prototype-chain creation
   reachable from the VM — genuinely new engine surface, not a lowering of
   existing machinery.
5. **`BinaryOp::In` / `BinaryOp::Instanceof`**. The only two missing arms in
   `Compiler::binary_op` (`compiler.rs:185-208`); 3 bails measured on
   OfflineAssembler (14.2% of one body's work). Each should delegate to the
   existing interpreter helpers for `HasProperty`/`OrdinaryHasInstance`
   rather than reimplementing them — likely a small slice once picked up.
6. **`nested tail call` bail relaxation**. `contains_tail_call`
   (`compiler.rs:657-669`) forces a whole-body bail whenever a `return`
   expression contains a `Call` anywhere in what it treats as tail position
   (e.g. `return cond ? f() : g()`), even though `compile_expr` already
   compiles conditionals/logicals/calls individually via ordinary `Op::Call`.
   184 bails on typescript-octane alone (the single largest reason after
   `call callee`/`This`). Needs investigation first: confirm whether the
   restriction is a genuine proper-tail-call correctness requirement or
   over-conservative, before relaxing it — not safe to assume without
   checking `Op::ReturnCall`'s exact contract.
7. **Nested `Statement::FunctionDeclaration`**. Single highest-exclusive-work
   bail body measured (`lex`, 25% of OfflineAssembler's tree-walker work, 4
   invocations). Worth its own small slice given the concentration.
8. **`throw`/`try`/`catch`/`finally`**. The issue's own guessed "biggest
   blocker" is empirically the *smallest*: 1 bail on mandreel, 1 on
   typescript-octane, 0 on OfflineAssembler, out of ~15,700 combined compile
   attempts. Still needed eventually for control-flow completeness, but not
   evidence-justified as an early slice — explicitly deprioritized here
   versus the issue's stated order.
9. **Per-statement fallback ("compile islands")**. The larger redesign
   alternative the issue names as an alternative to items 1-4. Current
   architecture doesn't support it: `Chunk` (`chunk.rs`) has no sub-chunk
   table and the VM has no suspend/resume format to hand control back to the
   tree-walker mid-body. A real architectural change, not a lowering slice —
   deliberately not attempted piecemeal inside a smaller PR.
10. **`Expression::Super` / `Expression::NewTarget`**. Same env-binding shape
    as `this` (`eval.rs:462-467`) and mechanically similar to add, but scoped
    out of this PR to keep the diff reviewable — natural immediate follow-ups
    once `this`'s approach (LoadName reuse vs. dedicated opcode) is settled.

Also explicitly out of scope: rolling `test262-pass.txt` forward
(`origin/main`-only operation), any refactor of the existing supported-subset
code in `compiler.rs` beyond adding the new arm, and reformatting or
opcode-renumbering unrelated to this slice.
