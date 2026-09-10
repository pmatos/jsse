# Plan: #614 — Expression drop glue still recurses per AST nesting level

## Status at re-plan (2026-09-10)

This planning stage re-ran on a workspace already containing a completed
implementation from an earlier attempt. Do not re-plan or re-implement from
scratch — carry this forward instead:

- **Implementation already landed** at commit `90e7062` ("fix(ast): make
  Expression's boxed links drop iteratively via ExprBox", `Closes #614`),
  on top of this plan commit (`045774b`). It matches Option 1 below exactly:
  `ExprBox` wrapper, the ADR at `docs/adr/2026-09-10-0836-expr-box-iterative-drop.md`,
  and the file list in section 3. `cargo check -j4` on this commit is clean.
- **Do not redo the fix.** The implementation stage should detect the
  existing fix commit via `git log`/`git status` and proceed directly to
  verification and PR creation, not re-implement `ExprBox`.
- **Quality gates not yet re-run this session**: `cargo test` /
  `cargo test --release`, `./scripts/lint.sh`, and the full `test262`
  baseline check (section 4 slice 7 / section 6) still need to be run fresh
  before pushing — they were presumably run by the attempt that produced
  `90e7062`, but that isn't verified from this workspace state alone.
- **Remaining steps for the implementation stage**: `git rm PLAN.md` and
  commit, push the branch (never pushed to `origin` — confirmed via
  `git branch -r` and no upstream configured), and `gh pr create --base main
  --head <branch> --title "fix(ast): ..." --body ...` including `Closes #614`.
- **`EVIDENCE.md`** (untracked in this workspace) is a stale artifact from a
  later `simplify`-stage run that found no PR to review (since the branch was
  never pushed) and correctly exited without committing. It documents the
  same push/PR gap noted above. Leave it untracked; do not commit it here.

## 1. Problem restated

`Expression` has no `impl Drop`, so tearing one down uses the compiler's derived
field-by-field drop glue, which is a plain (non-tail) recursive function call per
`Box<Expression>` link. Two productions — the binary/logical operator-precedence
loop and the member/call/optional-chain continuation loop in the parser — build
`Expression` chains iteratively specifically so `MAX_PARSE_DEPTH` (#599/#606)
doesn't bound them, so they can reach millions of nesting levels before any other
limit fires. #612/#613 made the three *other* mandatory (cannot-report-failure)
post-parse passes over these chains iterative (`assign_expr_sites`,
`expr_uses_arguments`, `clear_expr_ic_sites`/`clear_stmt_ic_sites`), which moved the
debug abort ceiling from ~122k to ~1–1.5M operands. Drop glue is the fourth such
pass, is unaffected by that work, and — because its recursion depth is
release/debug-independent in kind (just smaller frames on release) — is also the
thing that makes the *release* ceiling not move: both profiles abort in the same
~2–2.5M range whether or not #613 is applied. The existing test suite already
knows this and works around it: `src/ast.rs`'s `leak_deep` helper
(`std::mem::forget`) exists solely so the stack-safety regression tests for #613
don't themselves overflow the small probe stack when their deeply-nested `Body`
values go out of scope.

The fix is Option 1 from the issue: introduce a newtype that owns the
`Box<Expression>` link and gives it a hand-written, worklist-based iterative
`Drop`, while leaving `Expression` itself free of any `Drop` impl so every
existing by-value match on `Expression` (e.g. `expr_to_pattern` in
`src/parser/mod.rs`) keeps compiling. This is a mechanical, compiler-driven
refactor across every construction/match site that touches a boxed `Expression`,
not a new AST representation — Option 2 (arena/`ExprId`) is explicitly deferred as
a follow-up, matching the issue's own framing.

## 2. Spec basis

N/A: no JavaScript behavior change. This is engine-internal memory-teardown
robustness (how a Rust `Expression` value is deallocated); it changes no
observable spec algorithm, no parse result, and no evaluated value. Programs that
previously aborted the process with an uncatchable native stack overflow now run
to completion instead — that is a robustness fix, not a semantics change, and is
covered by the "Numbering is provably unchanged"-style argument #613 already used
for the sibling passes (see slice 6 below, the `Debug`-dump invariance check).

## 3. Files to touch

Engine, under `src/`:

- **`src/ast.rs`** — the core change. Define the wrapper type (`ExprBox`), give it
  `Deref`/`DerefMut`/`Clone`/`Debug` matching `Expression`'s own derives, an
  `into_expression(self) -> Expression` accessor, and the iterative `Drop` plus its
  worklist helper (`push_children`, or similar). Replace every `Box<Expression>`
  field inside `Expression` itself (lines 357–388: `Unary`, `Binary`, `Logical`,
  `Update`, `Assign`, `Conditional`, `Call`, `New`, `Member`, `OptionalChain`,
  `Spread`, `Yield`, `Await`, `TaggedTemplate`, `Typeof`, `Void`, `Delete`,
  `Import`/`ImportDefer`/`ImportSource`) with `ExprBox`. For a single consistent
  link type (one wrapper, one extraction API, no special-casing in the
  exhaustive `push_children` match), also convert the remaining `Box<Expression>`
  sites reachable from `Expression`/`Pattern`/`ExportDeclaration`: `MemberProperty::Computed`
  (397), `PropertyKey::Computed` (493), `Pattern::Assign`/`Pattern::MemberExpression`
  (293, 295), `ClassDecl`/`ClassExpr::super_class` (652, 660),
  `ExportDeclaration::Default` (226). These are all recursive-descent-parsed
  (bounded by `MAX_PARSE_DEPTH`) so are not required for the release-ceiling fix,
  but leaving them as raw `Box<Expression>` means two link types and two
  extraction idioms in the same file for no safety benefit — call this out in the
  PR description as a deliberate uniformity choice, not a silent scope creep.
  Update the existing test module: delete `leak_deep` (it becomes dead code once
  drop is safe) and let the four `#[cfg(test)]` stack-safety tests that currently
  call it (`deep_member_chain_is_numbered_without_native_recursion`,
  `deep_call_chain_is_numbered_without_native_recursion`,
  `deep_operand_chain_arguments_scan_has_no_native_recursion`,
  `deep_member_chain_is_cleared_without_native_recursion`) drop their trees
  normally at scope exit instead.
- **`src/parser/mod.rs`** — `expr_to_pattern` (~line 1270) does the one genuine
  by-value destructure of an owned `Expression` in the crate today (`*left`,
  `*inner` on `Box<Expression>` fields via `Expression::Assign`/`Spread`/`Object`
  arms). These become `left.into_expression()` / `inner.into_expression()`. This
  function is the concrete proof that `Expression` must stay `Drop`-free — it
  fails to compile (`E0509`) the moment `Expression` itself gains a `Drop` impl,
  which is exactly why the wrapper goes around the `Box`, not around `Expression`.
- **`src/parser/expressions.rs`, `src/parser/declarations.rs`,
  `src/parser/statements.rs`** — construction sites (`Box::new(...)` →
  `ExprBox::new(...)`) and a few `&**`/`.as_ref()` idioms on `Box<Expression>`
  fields (e.g. `expressions.rs:121,1566`) that need adjusting because `ExprBox`'s
  `Deref` is one level, not two, and `Box::as_ref`'s blanket `AsRef` impl isn't
  automatically inherited by a newtype.
- **`src/interpreter/generator_transform.rs`, `src/interpreter/generator_analysis.rs`**
  — heavy `Expression`-shaped rewriting (`rewrite_expr`, `extract_lhs_suspensions`,
  `oc_chain_to_regular_expr`). These currently match by reference and `.clone()`
  rather than consuming by value, so they are not blocked by `Expression` staying
  `Drop`-free, but every `Box::new(...)` construction site in them still needs the
  mechanical `ExprBox::new` rename.
- **`src/interpreter/eval.rs`, `src/interpreter/eval/*.rs`,
  `src/interpreter/bytecode/compiler.rs`, `src/interpreter/mod.rs`,
  `src/interpreter/builtins/mod.rs`, `src/interpreter/ic_store.rs`** — same
  mechanical rename wherever they build or destructure a boxed `Expression`
  (~90 more `Box::new(` sites combined; the compiler enumerates all of them once
  the field types change, so this is a fix-until-it-builds pass, not a hunt).
- **`docs/adr/0005-expr-box-iterative-drop.md`** (new) — records the wrapper-type
  decision, the invariant it protects (`Expression` must never itself implement
  `Drop`), and explicitly defers the arena/`ExprId` option (#614's Option 2) as
  the durable answer if the AST keeps accumulating mandatory passes. Precedent:
  `docs/adr/0003-nan-boxed-jsvalue.md` already documents a hand-written `Drop` for
  a different core type for a similar reason (asymmetric cost, correctness
  invariant), so this is consistent with existing ADR scope, not a new category.

No changes needed under `scripts/`, `benchmarks/`, or `.github/` — this is a pure
engine-internals fix.

## 4. TDD slices

1. **Confirm today's red is a SIGABRT, not a clean assertion failure.** Delete
   `leak_deep`'s call sites in the four existing `src/ast.rs` stack-safety tests
   (keep the tests, just let their `Body`/`Expression` values drop normally) and
   run `cargo test --release ast::` (and the debug profile) *before* touching
   `ExprBox`. Confirm the whole test binary aborts (`SIGABRT`/"stack overflow") on
   at least one of them — this is the documented, already-understood defect, not
   a new bug, so this slice is a characterization step, not real production code.
   Revert `Cargo.toml`/no code changes needed here beyond the test file; this
   slice's "production code" is slice 2's `ExprBox`.
2. **Introduce `ExprBox` and its iterative `Drop`, unused.** Add the newtype,
   `Deref`/`DerefMut`, `into_expression`, and `Drop` (with the worklist helper) in
   `src/ast.rs`, without yet changing any `Expression` field — it's dead code at
   this point (allowed transiently within one slice; the crate won't build green
   with genuine dead-code warnings past this slice, so slices 2 and 3 land as one
   commit in practice). Unit test directly against `ExprBox` in isolation: build a
   deep `ExprBox`-wrapped chain (reuse the `deep_add_chain`/`deep_member_chain`
   builders) and drop it `on_small_stack`, asserting no crash — this test can only
   pass once slice 3 rewires the real `Expression` variants to use `ExprBox`, so
   slices 2+3 are effectively one red/green pair with slice 2 as the setup half.
3. **Rewire `Expression`'s own recursive variants to `ExprBox`.** Change the
   fields listed in section 3, fix every resulting compile error
   (`Box::new` → `ExprBox::new`, `.as_ref()`/`&**` adjustments, `expr_to_pattern`'s
   `into_expression()`). Green: the crate builds, and the slice-2 drop test now
   passes for real (not vacuously). Re-enable the four existing stack-safety tests
   from slice 1 without `leak_deep` — they must now pass on `on_small_stack`
   (`SMALL_STACK = 256 KiB`, `DEEP = 100_000`) on **both** debug and release
   profiles (`cargo test` and `cargo test --release`).
4. **Add drop-specific stack-safety regressions per continuation-loop shape,**
   parallel to the existing numbering/clearing tests: `deep_add_chain_drops_without_native_recursion`
   (flat `1+1+1+…`, the #612 repro), `deep_member_chain_drops_without_native_recursion`
   (`a.b.b.b…`), `deep_call_chain_drops_without_native_recursion` (`a()()()…`).
   Each builds with the existing helpers, drops the value inside
   `on_small_stack`, and asserts no crash. Red without slice 3's rewiring (the
   whole test binary would abort); green after.
5. **Round-trip `expr_to_pattern` through `into_expression`.** Add/extend a
   parser unit test (or reuse an existing arrow-parameter-destructuring test if
   one already exercises `expr_to_pattern`) covering `Expression::Assign`,
   `Expression::Spread`, and `Expression::Object` shorthand-with-default arms —
   the three arms in `expr_to_pattern` that move a boxed child out — to confirm
   `into_expression()` produces the identical `Pattern` the old `*left`/`*inner`
   deref-move did. This is a refactor-safety check, not new behavior: the
   assertions should be on the resulting `Pattern` shape, unchanged from
   pre-refactor.
6. **`Debug`-dump invariance, mirroring #613's numbering-invariance check.**
   Confirm `ExprBox`'s hand-written `Debug` impl (delegating to the inner
   `Expression`) produces byte-identical output to the pre-refactor derived
   `Debug` for a representative corpus (reuse or extend the ~57-snippet corpus
   #613 used, if it's still in the test module, or a smaller representative
   subset covering every converted variant). This guards against `ExprBox`
   printing as `ExprBox(Binary(...))` instead of `Binary(...)`, which would be a
   silent, low-severity but real regression in anything that snapshots AST debug
   output.
7. **Full-suite gate (not a new test, the existing gate applied to this change):**
   `cargo test --release`, `cargo test` (debug), `uv run python
   scripts/run-test262.py` against the baseline read from `origin/main:test262-pass.txt`,
   and `./scripts/lint.sh`. No test262 outcome is expected to change (see
   section 6) — a moved baseline here is a signal something is wrong, not
   progress to bank.

## 5. Test surface

- No `test262/test/...` directory targets this change specifically — it changes
  no observable behavior, so the relevant gate is the **existing baseline not
  moving**, run as `uv run python scripts/run-test262.py` against
  `origin/main:test262-pass.txt` (full suite; this touches shared AST plumbing
  used by every test, so a targeted subdirectory isn't a substitute for the full
  run).
- Spec-correct behavior not covered by test262: none — this is not spec
  behavior, it's engine memory-management robustness. The right home for its
  regression coverage is `src/ast.rs`'s own `#[cfg(test)]` module (slices 1–6
  above), following the exact pattern #613 already established there
  (`on_small_stack`, `DEEP`, `SMALL_STACK`, the deep-chain builders). Do not add
  anything under `test262-extra/` for this issue — that directory is for
  spec-clause-backed behavioral gaps, and this issue has none.
- `cargo test --release` and `cargo test` are the primary gates (per section 4,
  slice 7). `./scripts/lint.sh` must stay clean — per the fmt-hook/clippy gate
  already in effect (`-D warnings`), leftover dead code like the old `leak_deep`
  or an unused intermediate helper will block the edit, not just the CI run.

## 6. Regression risk

- **Hot path cost.** `ExprBox`'s `Deref`/`DerefMut` must compile down to the same
  pointer-chasing a bare `Box<Expression>` already does — no `Option` wrapper, no
  null/tag check, no extra indirection. This is load-bearing for `eval_expr` and
  the bytecode compiler, both of which dereference boxed expression children on
  every evaluation of `Binary`/`Member`/`Call`/etc. — i.e. the tree-walker's
  hottest path. Confirm with one wall-time comparison on a **default (non-perf-counters)
  release build** (`benchmarks/scripts/bench_opmix.js` or a `gen-mandreel-phases.py`
  phase) before/after; per this repo's own rule, never take timing from an
  instrumented (`--features perf-counters`) build.
- **Drop cost shape changes.** Today's drop is "free" until it isn't (SIGABRT).
  After this change, every `Expression` drop — including the overwhelmingly
  common shallow ones — pays for a `Vec` worklist that, for shallow trees, should
  never actually allocate (each `take()`-then-push only happens for genuine
  children; a leaf `Expression::This` placeholder swap costs nothing). Confirm
  this empirically rather than assuming it: the same wall-time comparison above,
  plus watching for any regression in the GC-heavy library-test suites
  (`decimal.js`, `big.js`, `zod`, `moment` — all allocate/drop many expressions
  indirectly via bytecode compilation and generator transforms) is optional but
  recommended if the opmix numbers look off.
- **`ObjectKind`/GC interaction: none.** `Expression` (parser AST) is not a
  GC-managed heap object and does not go through `gc::trace_object_fields` or
  `gc_safepoint()` — those root `JsObjectData`/`ObjectKind`, a runtime-heap
  concept unrelated to the parser's `Expression` tree. This change has no GC
  surface.
- **`test262-pass.txt` baseline: not expected to move**, and must not be
  rewritten by this PR (`--update-baseline` is a `main`-branch operation per
  project convention). If the full-suite run in slice 7 shows any diff, that is
  a red flag to investigate, not a baseline update to bank.
- **Exhaustiveness of `push_children`.** The worklist helper must match every
  `Expression` variant with no wildcard arm (mirroring the discipline
  `gc::trace_object_fields` already uses for `ObjectKind` — "adding a new variant
  fails to compile until the walker handles it"). A future new `Expression`
  variant that's missed here would silently re-introduce native recursion for
  that variant's children rather than failing to compile, which is the one way
  this fix could regress invisibly. Prefer an exhaustive `match` (no `_ =>`) for
  exactly this reason.
- **Interaction with generator replay.** `generator_transform.rs` and
  `generator_analysis.rs` walk and rebuild `Expression` trees per the replay-based
  generator model; they're clone/reference-based today (not by-value), so they're
  not blocked by keeping `Expression` `Drop`-free, but they are heavy `Box::new(`
  call sites that need the mechanical rename — verify the generator/async test262
  subset and the custom generator tests in `tests/` still pass, since this is the
  area most likely to have a missed rename site that compiles (wrong `ExprBox`
  argument) but behaves subtly differently.

## 7. Out of scope

- **`Statement`'s own drop glue.** Block/if/loop nesting (`Box<Statement>` in
  `Labeled`, `With`; inline `Vec<Statement>` elsewhere) is built by genuine
  recursive-descent parsing, bounded by `MAX_PARSE_DEPTH` (a few thousand levels)
  — nowhere near a stack-overflow-relevant depth. Not touched.
- **The arena/`ExprId` redesign (#614 Option 2).** The issue itself frames this
  as "the durable end state if the AST keeps accumulating passes," not this PR's
  job. Recorded as the alternative in the new ADR and left as a follow-up issue,
  not implemented here.
- **Raising or removing `MAX_PARSE_DEPTH`.** Unrelated axis (#599/#606); not
  touched.
- **#607 (evaluation-depth guards) and #606/#599 (parse-depth guards).**
  Explicitly called out in the issue as related-but-distinct; not touched.
- **Any refactor of `generator_transform.rs`'s clone-heavy rewriting into
  by-value consumption.** Tempting once `Expression` is confirmed `Drop`-free
  everywhere, but it's a separate performance/cleanup change with its own risk
  surface, not required to close this issue. Left for a future PR.
- **Converting `Box<Statement>` or `Box<Pattern>` link types generally.** Only
  the `Box<Expression>` sites are converted (see section 3 for the exact list,
  including the non-`Expression`-variant ones taken for uniformity); this is not
  a general "wrap every boxed AST link" initiative.
