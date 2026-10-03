# Plan for issue #823: continue-to-outer-label completion value lost across inner for-of IteratorClose under GC stress

## 1. Problem restated

`exec_for_of_loop` (`src/interpreter/exec.rs`) keeps its running completion value `v` alive
across arbitrary user code (the iterator's `return()`) via a continuous-rooting discipline:
`v` is rooted once before the loop and every single reassignment of `v` must unroot the old
value and root the new one in place (stated explicitly in the comment above the `let mut v =
JsValue::UNDEFINED;` initialization). Every arm of the `match body_result` that reassigns `v`
follows this — except one. When a loop's own body produces `Completion::Continue(Some(lbl),
val)` and `lbl` matches *this* loop's own label (i.e. the continue is consumed here, iteration
just proceeds — no `IteratorClose` needed), the code does `v = v2` directly with no
`gc_unroot_value`/`gc_root_value` pair. From that point until `v` is next reassigned, `v`'s
current value is unrooted even though the loop keeps it alive across the next iteration's
safepoint.

The outer loop in the repro (`outer: for (const o of [1]) { for (const x of makeIterable()) {
...; continue outer; } }`) hits exactly this: the inner for-of's body completion
`Continue(Some("outer"), Some(payload))` propagates out of the inner `exec_for_of_loop` (its own
`IteratorClose` arm at `exec.rs:2543-2552` roots `payload` correctly across the inner iterator's
`return()` — that part is already correct, landed by #815). It becomes the *outer* loop's
`body_result`. The outer loop's `loop_label` is `Some("outer")`, so it takes the buggy
same-label arm (`exec.rs:2538-2541`) and assigns `v = payload` unrooted. The very next thing the
outer loop does is loop back to the top and call `self.gc_safepoint()` (`exec.rs:2385`) before
calling the outer iterator's `next()` — under `JSSE_GC_STRESS=1` this collects `payload`, which
nothing else references. `[1]` then reports `done`, the loop returns `Completion::Normal(v)`
with `v` now a stale/freed value, and `continued.tag` reads back as `undefined` instead of
`"continue-payload"`.

The `:strict`-only manifestation is very likely a GC-stress-parity artifact rather than evidence
of a strict/eval-specific code path: `JSSE_GC_STRESS` alternates major/minor collections per
safepoint count, and the extra `'use strict';` directive prologue shifts the safepoint parity by
one, changing whether the safepoint that lands on the unrooted window performs a major
(root-set-only) collection — which would free the young, unrooted `payload` — or a minor one that
happens to spare it for an unrelated reason (e.g. it was promoted by `$262.gc()` running inside
`return()` while it was still correctly rooted, one frame earlier). This needs to be checked
empirically in slice 1 below, not assumed; see the red test in slice 1(a), which drops `eval`
and the inner loop entirely and must fail in *both* sloppy and strict mode if the parity
hypothesis is right. If it instead only fails in strict mode even with the inner loop and `eval`
removed, the hypothesis is wrong and must be revisited before writing the fix.

## 2. Spec basis

- `sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`
  (ForIn/OfBodyEvaluation, spec.html:22388-22465): the `Repeat` loop's step "If
  `LoopContinues(result, labelSet)` is `false`, then ... `IteratorClose`" (spec.html:22455-22462)
  and the following step "If `result.[[Value]]` is not `~empty~`, set `V` to `result.[[Value]]`"
  (spec.html:22463). When `LoopContinues` is `true` — i.e. the body's `Continue` completion's
  label is in `labelSet` (this loop's own label(s), or unlabeled) — the spec does **not** call
  `IteratorClose`; it only folds the body's value into `V` and iterates. `exec_for_of_loop`'s
  `loop_label == Some(lbl.as_str())` arm is exactly this "`LoopContinues` is `true`" case. The
  bug is that the engine's representation of `V` (the local `v`) loses its GC root when this
  step runs, which spec pseudocode has no notion of — it's a bug in *how* the engine keeps `V`
  alive, not in what value `V` gets.
- `sec-loopcontinues` (LoopContinues, spec.html:21853): defines when a `Continue` completion is
  consumed by the current loop (unlabeled, or its label is in the loop's own `labelSet`) versus
  propagated outward — this is what the `loop_label == Some(lbl.as_str())` check implements.
- `sec-updateempty` (UpdateEmpty, spec.html:4325): the "if `result.[[Value]]` is not `~empty~`,
  set `V`" step above is `UpdateEmpty` applied to the loop's own running value; this is also how
  the `continue outer;` statement's own `~empty~` value picks up the preceding expression
  statement's value inside `exec_prepared_statements` (`exec.rs:382-389`) before ever reaching
  the for-of loop. No change needed there — confirmed correct by the inner loop's `IteratorClose`
  arm already surviving its own `return()` call.

No JavaScript-observable syntax or semantics change: `v`'s value and when it changes are already
spec-correct (confirmed by the three passing scenarios in the existing test262-extra file). This
is purely a GC-rooting defect in the engine's own bookkeeping — the kind of fix the project's `GC
Root-Stack Discipline` section (`CLAUDE.md`) and issue #794/#815 already establish a pattern for.

## 3. Files to touch

- `src/interpreter/exec.rs` — the fix, in `exec_for_of_loop`'s `Completion::Continue(Some(lbl),
  val)` match arm (currently `exec.rs:2537-2554`), specifically the `loop_label ==
  Some(lbl.as_str())` branch (`exec.rs:2538-2541`).
- `test262-extra/for-of-abrupt-completion-payload-gc-rooting.js` — extend with the two new red
  cases from slice 1 (self-label continue payload under stress; LIFO-assert-triggering
  multi-continue case). This is the same file issue #815 added and issue #823 says already
  covers the *outer*-label case (`continued`); the new cases isolate the *own*-label arm
  specifically, which is where the actual defect lives.
- No `docs/adr/` entry: this follows the existing, already-documented GC root-stack discipline
  (`CLAUDE.md` → `GC Root-Stack Discipline`) rather than establishing a new one.

## 4. TDD slices

1. **Isolate the defect without `eval`, without an inner loop, in both modes.** Add a case to
   `test262-extra/for-of-abrupt-completion-payload-gc-rooting.js`:
   ```js
   var selfContinued = eval(
     "outer: for (const o of [1, 2]) { ({ tag: 'self-continue-payload' }); continue outer; }"
   );
   assert.sameValue(
     selfContinued.tag,
     "self-continue-payload",
     "value threaded through a continue to the loop's own label survives the next iteration's safepoint"
   );
   ```
   `[1, 2]` (not `[1]`) so there's a second iteration whose top-of-loop `gc_safepoint()` is the
   one that must observe the still-rooted value — with a single-element array the loop would
   exit via `done` before ever re-entering the unrooted window on a different iteration's
   safepoint, same as issue #823's own repro structure.
   Run under `JSSE_GC_STRESS=1 uv run python scripts/run-test262.py test262-extra/for-of-abrupt-completion-payload-gc-rooting.js`
   first against the *current* HEAD to confirm this new case is RED in both the default run and
   a run with `'use strict';` prepended by the harness (it should already run both, since the
   file has no `onlyStrict`/`noStrict` flag) — this is the check that confirms or falsifies the
   GC-stress-parity hypothesis in §1 before any production code changes. If it's red in strict
   only, stop and re-diagnose; do not proceed to the fix assuming the current theory.
2. **Debug-assert red test independent of `JSSE_GC_STRESS`.** Add a second case to the same
   file that triggers the LIFO root-stack assert on a `release-checked` build without needing
   stress mode at all (catches the same missing-root defect through `gc_assert_root_depth`/
   `RootStack`'s balance checks mentioned in `CLAUDE.md` → `GC Root-Stack Discipline`):
   ```js
   var multiContinued = eval(
     "outer: for (const o of [1, 2, 3]) { if (o === 2) { ({ t: 2 }); continue outer; } ({ t: o }); }"
   ).t;
   assert.sameValue(multiContinued, 3, "running value survives repeated same-label continues");
   ```
   Run with `cargo build --profile release-checked` then
   `uv run python scripts/run-test262.py --binary target/release-checked/jsse test262-extra/for-of-abrupt-completion-payload-gc-rooting.js`.
   Confirm this panics (root-stack imbalance) at current HEAD — this is the fast, stress-free RED
   signal.
3. **Fix.** In `exec_for_of_loop`'s `Completion::Continue(Some(lbl), val)` arm, change the
   `loop_label == Some(lbl.as_str())` branch to match the sibling arms' pattern (e.g.
   `Completion::Normal` at `exec.rs:2481-2485`, `Completion::Continue(None, cont_val)` at
   `exec.rs:2487-2493`):
   ```rust
   if loop_label == Some(lbl.as_str()) {
       if let Some(v2) = val {
           self.gc_unroot_value(&v);
           v = v2;
           self.gc_root_value(&v);
       }
   }
   ```
   No `IteratorClose` call belongs in this branch — per `LoopContinues` (§2), the loop keeps
   iterating with the same iterator, so this arm is correctly the one place in the match that
   does *not* call `iterator_close_result`.
4. **Green.** Re-run both the stress run (slice 1) and the release-checked run (slice 2); both
   cases must now pass/not panic. Re-run the existing three scenarios in the same file
   (`returned`, `broke`, `labeledBroke`, `continued`) under the same two configurations to
   confirm no regression.
5. **Refactor check (no behavior change expected).** Re-read the full `match body_result` block
   once more to confirm every arm that can reach another iteration of `loop_label`'s own loop
   (i.e. every arm that does *not* `return`) now follows the unroot/assign/root pattern. No
   structural refactor planned — if everything already matches, this slice is a no-op
   confirmation, not new code.

## 5. Test surface

- Targeted test262 run (should be unaffected — these are read-only sanity checks that the fix
  doesn't change any observable value, only GC survival):
  `uv run python scripts/run-test262.py test262/test/language/statements/for-of/`
  `uv run python scripts/run-test262.py test262/test/language/statements/continue/`
  `uv run python scripts/run-test262.py test262/test/language/statements/labelled-statement/`
  (`cptn-*.js` under `for-of/` specifically exercise completion-value threading, including
  `cptn-decl-itr.js`/`cptn-expr-itr.js`, but none of them combine a labeled outer loop, a nested
  for-of, and GC stress — hence the new test262-extra cases.)
- `test262-extra/for-of-abrupt-completion-payload-gc-rooting.js` is the primary regression
  surface; it needs the two new cases from slices 1-2 and must be run in all of:
  - plain: `uv run python scripts/run-test262.py test262-extra/`
  - stress: `JSSE_GC_STRESS=1 uv run python scripts/run-test262.py test262-extra/ --timeout 300`
  - stress + bytecode: `JSSE_GC_STRESS=1 uv run python scripts/run-test262.py test262-extra/ --bytecode --timeout 300`
    (expected to behave identically to the non-bytecode stress run: `ForOf` is a confirmed
    compiler bail — `Statement::ForOf { .. } => "statement:ForOf"` in
    `src/interpreter/bytecode/compiler.rs:778` — so every for-of loop, labeled or not, already
    runs on the tree-walker regardless of this flag; this run exists only to confirm that
    stays true, not because the fix touches `bytecode/`)
  - release-checked (for slice 2's debug-assert case):
    `cargo build --profile release-checked` then
    `uv run python scripts/run-test262.py --binary target/release-checked/jsse test262-extra/`
- `cargo test --release` for the existing `loop_flow_tests` unit tests in `exec.rs` — unaffected
  (they test `handle_loop_body_completion`, the free function used by `while`/`do-while`/`for`/
  `for-in`, not `exec_for_of_loop`'s hand-rolled match, so no new unit test is planned there; the
  behavioral coverage lives in the test262-extra file per the project's convention that
  GC-rooting regressions get test262-extra coverage, not `tests/`, since this is spec-observable
  through `eval`'s completion value).

## 6. Regression risk

- The change touches one match arm inside `exec_for_of_loop`, used by every sync and async
  for-of/for-await-of loop (labeled or not) in the tree-walker. The new `gc_unroot_value`/
  `gc_root_value` pair is a strict no-op when `v` is a non-object primitive (both functions
  early-return via `val.as_object_id()` being `None` — see `mod.rs:1439-1443`/`1531-1535`), so
  the common case (no payload, or a primitive payload) is unaffected; only the case already
  under test (an object payload surviving a same-label `continue`) changes behavior, and only by
  fixing the dangling root.
- `test262-pass.txt` (read from `origin/main`, not updated by this plan): this fix can only move
  previously-*failing* tests to passing (fixing a use-after-free-shaped bug cannot make a
  previously-passing deterministic test fail), but run the targeted directories from §5 anyway
  to catch anything unexpected before relying on that assumption.
- Shared machinery leaned on: `gc_root_value`/`gc_unroot_value`/`gc_temp_roots` (`RootStack`,
  `src/interpreter/root_stack.rs`) and `gc_safepoint()` — the same primitives #794/#815 already
  established the discipline for; no new GC primitive is introduced. `property.rs` and the
  `ObjectKind` match are untouched. The bytecode fast path is untouched and structurally cannot
  regress here (for-of is a compiler bail, §5).
- Generator/async-function for-of loops are lowered through an entirely separate state-machine
  path (`generator_transform.rs`'s `ForOfInit`/`ForOfHead` terminators, driven by
  `generator_runtime.rs`), not through `exec_for_of_loop`. Whether that path has an analogous
  missing-root defect for a same-label `continue` is **unknown** — it was not investigated for
  this plan — and this fix does not touch it. Flagged explicitly under "Out of scope" below so
  it isn't silently assumed fixed.

## 7. Out of scope

- Any change to the generator/async-function for-of state-machine lowering
  (`generator_transform.rs`, `generator_runtime.rs`). If the same "own-label continue reassigns
  the running value without re-rooting" shape exists there, it needs its own issue — filing one
  is reasonable follow-up work but is not part of this fix, and this plan makes no claim about
  whether that path is affected.
- `handle_loop_body_completion` and the `while`/`do-while`/`for`/`for-in` loops that use it are
  not touched: they root/safepoint/unroot `v` at the top of every iteration *before* calling into
  the body (confirmed at `exec.rs:1807-1809` for `while`, `1830-1832` for `do-while`,
  `1960-1962` for `for`, and the per-key-iteration `self.gc_root_value(&v)` inside `exec_for_in`),
  so the blind `*v = val` inside the shared free function is safe there — the next iteration's
  top-of-loop root always runs before the next safepoint. This is a structurally different (and
  already safe) discipline from `exec_for_of_loop`'s continuous-rooting model; unifying the two
  loop-body-completion protocols is a refactor, not a bug fix, and is explicitly not planned
  here.
- No `--update-baseline` run against `test262-pass.txt` (that's a `main`-branch operation per
  the project's own rules).
- No formatting or unrelated cleanup inside `exec_for_of_loop` beyond the one arm's rooting
  pair.
