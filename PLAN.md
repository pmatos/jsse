# Plan: fix(gc) — RegExp.prototype[@@split] does not root the species-constructed splitter

## 1. Problem restated

`RegExp.prototype[%Symbol.split%]` (`src/interpreter/builtins/regexp.rs`, native
closure starting at line 9451) constructs a fresh `splitter` object via
`SpeciesConstructor`/`Construct` (step 7, `splitter_id` bound at line 9513) and
then runs a `while q < size` loop (line 9565) that repeatedly calls
`spec_set(interp, splitter_id, "lastIndex", ...)` (step 15.a, can invoke a
user-defined setter) and `regexp_exec_abstract(interp, splitter_id, ...)` (step
15.b, can invoke a user-overridden `exec`). Both can run arbitrary JS, including
`$262.gc()`, and neither ever stores `splitter_id` into any traced root or
object field — it lives only as a bare `u64` local for the rest of the native
function, including the empty-string early-return branch (step 12, line 9548)
that calls `regexp_exec_abstract` before the loop even starts. A GC cycle
triggered from inside either call can collect the splitter object out from
under the loop; the next use of `splitter_id` then dereferences a recycled
arena slot.

The most directly reproducible window is actually earlier than the loop:
step 10's `ToUint32(limit)` (lines 9528-9536, `interp.to_number_value(&limit)`)
runs immediately after construction and before the loop. If `limit` is an
object, its `valueOf` runs as ordinary user JS whose call environment binds
`this` to `limit`, not to the splitter — so for the duration of that call the
splitter is reachable *only* through the bare `splitter_id` local and can be
collected before the loop ever starts. By contrast, GC triggered from inside
a user-overridden `splitter.exec()` does **not** reproduce the bug on its
own: `call_function` binds `this` to the splitter in `exec`'s own environment
record, and that environment is reachable from the active execution context
stack (which the collector traces), so the splitter stays reachable through
that particular call regardless of rooting. This is the same bug class fixed
for `@@matchAll` in #797 (commit `15563c72`, "fix(gc): root matchAll's
matcher during construction, not just at rest"), applied to a longer-lived
local: `@@split`'s splitter must stay rooted from construction (step 7)
through the function's final return — covering the limit coercion, the
step-12 empty-string early return, and the loop — not just across one
setter/getter pair.

## 2. Spec basis

- `spec/spec.html`, clause `sec-regexp.prototype-%symbol.split%` (oldid
  `sec-regexp.prototype-@@split`), **"RegExp.prototype [ %Symbol.split% ]
  ( _string_, _limit_ )"** — step 7 constructs `splitter` via `Construct(C, «
  rx, newFlags »)`; step 12.a and step 15.a/15.b call `RegExpExec(splitter,
  S)` and `Set(splitter, "lastIndex", ...)` respectively, both of which can
  invoke user-defined `exec`/setter traps per `RegExpExec` (`sec-regexpexec`)
  and ordinary `[[Set]]` semantics.
- This is a **GC-rooting fix, not a semantics change**: no step's observable
  behavior changes. `with_gc_root_scope`/`gc_root_id` (`src/interpreter/mod.rs`,
  documented in `docs/adr/2026-09-10-2014-gc-root-scope-guard.md`) is an
  engine-internal mechanism with no spec clause of its own — the spec clause
  above is cited only to confirm which steps can re-enter user JS and must
  therefore not leave the splitter unrooted.

## 3. Files to touch

- `src/interpreter/builtins/regexp.rs` — wrap the `@@split` native closure body
  from directly after `splitter_id` is bound (after line 9520) through the
  function's final `Completion::Normal(interp.create_array(a))` (line 9677) in
  `interp.with_gc_root_scope(|interp| { interp.gc_root_id(splitter_id); ... })`,
  mirroring the `@@matchAll` fix's shape exactly (construction-through-final-use,
  not just the loop body — the issue's own suggested fix allows either scope,
  and wrapping from construction is required to close the step-10 `lim`
  coercion window identified in section 1, which a loop-only wrap would miss).
- `test262-extra/RegExp-split-splitter-construction-gc-rooting.js` — new
  regression test, following the naming and `info:`/`features:` pattern of
  `test262-extra/RegExpStringIterator-matcher-construction-gc-rooting.js`.
- No `docs/adr/` update needed: this is an application of the already-decided
  `with_gc_root_scope` idiom (ADR 2026-09-10-2014), not a new decision.
- No `CONTEXT.md` change: no new vocabulary.

## 4. TDD slices

1. **Red:** add `test262-extra/RegExp-split-splitter-construction-gc-rooting.js`
   (naming follows the existing `RegExp-*-gc-rooting.js` / `RegExp-*.js` files
   already in `test262-extra/`). Repro: call
   `/b/[Symbol.split]("abc", { valueOf() { $262.gc(); return 10; } })` on a
   plain `RegExp` (no subclass needed — a fresh splitter is constructed via
   `SpeciesConstructor` even for the base constructor). The `limit` argument's
   `valueOf` runs after `Construct` (step 7) and before the loop; during that
   call `this` is bound to the `limit` object, not the splitter, so the
   splitter is reachable only through the unrooted `splitter_id` local when
   `$262.gc()` fires — the window identified in section 1. Assert the full
   split result (e.g. with `compareArray`) to confirm the splitter survives
   and produces the correct match, not just that nothing crashes. This must
   be deterministic with a single `$262.gc()` call and no `JSSE_GC_STRESS`
   needed (unlike the `@@matchAll` sibling test, which does rely on stress
   mode being independently viable — do not assume stress mode is required
   here; if a single `$262.gc()` does not reproduce because the freed slot
   isn't reused before the next access, add a handful of throwaway object
   allocations between the `gc()` call and `return` to force slot reuse).
   Confirm red on the current binary with a plain, non-stress run first; also
   confirm red under `JSSE_GC_STRESS=1` as a second signal. Build once first:
   `cargo build --release`.
2. **Green:** apply the `with_gc_root_scope`/`gc_root_id` wrap described in
   section 3. Rebuild and confirm the new test passes both plain and under
   `JSSE_GC_STRESS=1`:
   `JSSE_GC_STRESS=1 uv run python scripts/run-test262.py test262-extra/RegExp-split-splitter-construction-gc-rooting.js`
3. **Refactor:** none planned — the change is a single closure-body wrap with
   no duplicated logic to extract; `with_gc_root_scope` already is the shared
   abstraction.

## 5. Test surface

- Targeted test262 run (regression check, must stay green):
  `uv run python scripts/run-test262.py test262/test/built-ins/RegExp/prototype/Symbol.split/`
  (44 tests as of the pinned submodule commit, covering species-constructor
  selection, flag coercion, `lastIndex` get/set, empty-match advancing, and
  capture splicing — none of them assert GC behavior, so none are expected to
  change status, but they are the best behavioral regression net for this
  function).
- New spec-correct-but-not-in-test262 coverage:
  `test262-extra/RegExp-split-splitter-construction-gc-rooting.js` (needs
  `features: [host-gc-required]` and `$262.gc()`, same pattern as the
  `@@matchAll` sibling test), run via
  `uv run python scripts/run-test262.py test262-extra/RegExp-split-splitter-construction-gc-rooting.js`
  and again with `JSSE_GC_STRESS=1` prefixed, per `CLAUDE.md`'s GC Stress Mode
  section.
- Full regression gate before opening the PR: `cargo build --release` then
  `uv run python scripts/run-test262.py` (full suite, baseline read from
  `origin/main:test262-pass.txt`, not rewritten) and
  `uv run python scripts/run-custom-tests.py`.
- `./scripts/lint.sh` for the lint gate.

## 6. Regression risk

- **Low risk to `test262-pass.txt`.** The change only extends the lifetime of
  an existing GC root; it adds no new allocations on the non-GC-triggering
  path and changes no returned value, so no previously-passing test262 test
  is expected to change status. The only way this baseline moves is if the
  closure-body wrap is misplaced and accidentally changes control flow (e.g.
  an early return that now skips the unroot, or a moved `?`/`return` that
  changes which value is produced) — the TDD slice's green step must diff
  the targeted `Symbol.split/` directory pass count before/after, not just
  check the new test.
- **Shared machinery touched:** GC rooting (`gc_temp_roots` / `RootStack`,
  `with_gc_root_scope` in `src/interpreter/mod.rs`) and transitively
  `gc_safepoint()` via any allocation inside the loop (array pushes,
  `to_string_value`, `create_array`). No changes to `property.rs`'s MOP
  dispatch, the `ObjectKind` matches, or the bytecode VM — this native
  function runs identically under the tree-walker and the bytecode path (both
  call into the same native closure), so no `--bytecode` divergence is
  expected, but the stress run should still be repeated under
  `--bytecode` if time allows (not required by CI today — ADR and `ci.yml`
  gate test262-extra under stress for both modes already).
- **GC stress interaction:** `JSSE_GC_STRESS=1` on `test262-extra/` is the
  actual oracle for this class of bug (per the issue's own repro method and
  the `@@matchAll` fix's commit message) — a plain (non-stress) run passing is
  not sufficient evidence the fix works, since natural GC timing may not hit
  the window.

## 7. Out of scope

- Auditing other RegExp builtins (`@@match`, `@@replace`, `@@search`, the
  `RegExp.prototype.exec`/`test` fast paths, or the `RegExpStringIterator`
  `next()` method) for the same bug class — the issue explicitly notes
  `@@search`/`@@replace` are unaffected (they operate on `rx` itself, no fresh
  matcher), and no other method was flagged. A separate audit pass, if wanted,
  is a follow-up issue, not part of this fix.
- Converting any *other* multi-exit manual root frame in `regexp.rs` to
  `with_gc_root_scope` — only the `@@split` closure is touched.
- Any refactor of the `@@split` loop's control flow, naming, or the
  `advance_string_index`/`regex_input_for_value` helpers it calls — the wrap
  is additive (one closure boundary + one `gc_root_id` call), not a rewrite.
- Rolling `test262-pass.txt` forward (`--update-baseline`) — that is a
  `main`-branch operation per `CLAUDE.md` and out of scope for a feature
  branch regardless of outcome.
- **Not fixed here, flagged as a candidate follow-up issue:** two other bare
  locals in the same `@@split` closure share this bug class but are not part
  of what issue #819 reported, so fixing them is deliberately left out to keep
  this PR narrowly scoped (consistent with #797 only fixing matchAll's
  matcher, not every unrooted local nearby): (1) `z_val`/`z_id` (the per-match
  result returned by `RegExpExec`) is unrooted across the `ToLength(Get(z,
  "length"))` coercion and each `Get(z, i)` capture read inside the loop,
  either of which can run user JS if `z` is a user-exec-supplied object with
  accessor properties; (2) the `a: Vec<JsValue>` accumulator is a plain Rust
  `Vec`, not a rooted JS array, for the entire loop — any object pushed into
  it earlier (e.g. an object capture value) is unrooted while later
  iterations run user JS, unlike the `array.rs` natives in
  `docs/adr/2026-09-10-2014-gc-root-scope-guard.md`, which keep their
  accumulator alive by rooting the already-created result array object
  itself. Worth filing as its own issue after this one lands.
