# Plan: issue #809 — gate a `JSSE_GC_STRESS` run in CI

## 1. Problem restated

`JSSE_GC_STRESS` (added by #331) forces GC collections at safepoints that would
otherwise skip them, turning missing-root and missing-write-barrier bugs into
reproducible failures instead of rare heisenbugs. It was never added as a CI
gate because running it over `test262-extra/` exposed 25 real root-finding
bugs, tracked as #794–#798. All five are now closed (verified via
`gh issue view`, all `state: CLOSED`). The issue asks to (a) add a blocking
`JSSE_GC_STRESS=7 ... test262-extra/ --fail-on-failures` run (normal and
`--bytecode`) to CI now that the prerequisite bugs are fixed, (b) add a
non-blocking nightly run at higher stress intensity over a sample of the full
suite to keep hunting for new ones, and (c) "consider" an opt-in quarantine
mode that stops recycling freed arena ids so a use-after-free panics
deterministically instead of silently reading a wrong-typed object.

I validated (a) empirically before planning further: built `./target/release/jsse`
and ran both commands by hand.

```
JSSE_GC_STRESS=7 uv run python scripts/run-test262.py --jsse ./target/release/jsse \
  --test262 ./test262 test262-extra/ --fail-on-failures            # 971/971 pass
JSSE_GC_STRESS=7 uv run python scripts/run-test262.py --jsse ./target/release/jsse \
  --test262 ./test262 test262-extra/ --bytecode --fail-on-failures # 971/971 pass
```

Both are clean at 100%. Re-measured at CI-realistic parallelism
(`-j 2`, matching a standard GitHub-hosted runner's `nproc/2`, with
`--timeout 300`): normal mode 43.4s wall (slowest scenario 16.8s), bytecode
mode 18.6s wall (slowest scenario 6.4s) — ~62s combined, comfortable margin
under even the default 120s timeout. The blocking gate is safe to turn on
now.

I also sanity-checked the nightly idea's premise — that a sampled, more
intense run over the *full* suite (not just `test262-extra/`) will keep
finding new bugs, the way #794–#798 originally did — by actually running one:
`JSSE_GC_STRESS=2 uv run python scripts/run-test262.py --jsse
./target/release/jsse --test262 ./test262 --sample 0.003 --seed 1 --timeout
300 -j 12` (default paths: `language/`, `built-ins/`, `annexB/`, `intl402/`).
It found **11 new regressions** in 67s wall, none overlapping
`test262-extra/`:

```
test262/test/annexB/built-ins/RegExp/prototype/Symbol.split/toint32-limit-recompiles-source.js (+:strict)
test262/test/built-ins/AsyncIteratorPrototype/Symbol.asyncDispose/throw-return-getter.js (+:strict)
test262/test/built-ins/Iterator/prototype/toArray/next-method-returns-throwing-value.js
test262/test/built-ins/Map/groupBy/evenOdd.js (+:strict)
test262/test/language/expressions/class/async-gen-method/yield-star-getiter-async-not-callable-symbol-throw.js (+:strict)
test262/test/language/statements/const/dstr/obj-ptrn-rest-getter.js (+:strict)
```

This confirms the nightly job will find real, previously-unknown stress bugs
from its very first run — expected and desired, not a regression caused by
this PR. See §6 and §7 for how the plan handles that.

## 2. Spec basis

N/A: no JavaScript behavior change. This issue only wires an existing,
already-correct test runner invocation (`JSSE_GC_STRESS` + `--fail-on-failures`)
into `.github/workflows/`; it touches no parser, interpreter, or builtin code
and changes no observable ECMAScript syntax or semantics.

## 3. Files to touch

- `.github/workflows/ci.yml` — add two new blocking steps to the existing
  `build` job: `JSSE_GC_STRESS=7` over `test262-extra/`, normal and
  `--bytecode`, both `--fail-on-failures`, reusing the already-built
  `./target/release/jsse` binary (no new build step needed).
- `.github/workflows/nightly-gc-stress.yml` (new) — non-blocking only by
  virtue of its trigger (`schedule` + `workflow_dispatch`, never
  `pull_request`/`push`): a higher-intensity, `--sample`d stress run over the
  full `test262/` suite (normal and `--bytecode`), with the bytecode step
  gated on `if: ${{ !cancelled() }}` so it still runs after a normal-mode
  failure instead of being skipped.
- `CLAUDE.md` (repo root) — append one line each to the "GC Stress Mode"
  section documenting the new blocking CI gate and the nightly exploratory
  job, so the doc stays the authoritative description of what runs where.
- No changes under `src/`, `test262-extra/`, `test262/`, `spec/`, or
  `docs/adr/` (no architecture decision is being made — this is CI wiring,
  not an engine design choice).

## 4. TDD slices

Pure CI/tooling work has no unit under test in the usual sense; "red" here
means the gate is absent (a rooting regression can land on `main`
undetected), "green" means the exact runner invocation passes and the new
YAML is syntactically/semantically sound per the existing workflow-lint gate.

1. **Blocking stress gate on `test262-extra/`.**
   - Red: `grep -n JSSE_GC_STRESS .github/workflows/ci.yml` currently matches
     nothing — no CI job ever runs under stress.
   - Change: add two steps to the `build` job in `ci.yml`, placed after the
     existing `release-checked` GC root-stack steps (end of the job, so the
     plain `release` binary built earlier in the same job is reused):
     ```yaml
     - name: Run test262-extra (GC stress, JSSE_GC_STRESS=7)
       run: JSSE_GC_STRESS=7 uv run python scripts/run-test262.py --jsse ./target/release/jsse --test262 ./test262 test262-extra/ --timeout 300 --fail-on-failures

     - name: Run test262-extra (GC stress, JSSE_GC_STRESS=7, bytecode VM)
       run: JSSE_GC_STRESS=7 uv run python scripts/run-test262.py --jsse ./target/release/jsse --test262 ./test262 test262-extra/ --bytecode --timeout 300 --fail-on-failures
     ```
     `--timeout 300` is deliberate even though the default 120s had margin
     locally: CI runners are slower/shared, and the point of this step is to
     catch rooting bugs, not to re-litigate timeout tuning under load.
   - Green: re-run both commands locally at `-j 2 --timeout 300` to match
     GitHub-hosted runner parallelism (done above: 971/971 pass both modes,
     43.4s + 18.6s wall, worst single scenario 16.8s); implementation stage
     re-verifies after rebasing in case new `test262-extra/` files landed
     since this plan was written, then confirms `workflow-lint.yml`
     (actionlint + zizmor) is clean on the edited file.

2. **Non-blocking nightly higher-intensity sampled stress run.**
   - Red: no stress coverage exists outside `test262-extra/`; the 11
     regressions found in this plan's validation run (§1) are invisible to
     every existing CI job.
   - Change: new `.github/workflows/nightly-gc-stress.yml`:
     - Triggers: `schedule: "0 3 * * *"` (daily — the sample is cheap enough,
       see measurement below, and a 5-day cadence like
       `nightly-test262-coverage.yml` would let a freshly-introduced
       regression sit unnoticed for days) and `workflow_dispatch` for manual
       runs.
     - `permissions: contents: read`.
     - Builds `./target/release/jsse` (plain `release`; `release-checked` is
       left for a follow-up, see §7).
     - `env: GC_STRESS_SEED: ${{ github.run_id }}` so each run's sample is
       different from the last (broadening coverage over time, same
       rationale as the unseeded mutation-testing oracle) but *that run's*
       failure is still reproducible from the logged run id — an unseeded
       `--sample` would make a red nightly impossible to reproduce.
     - Two steps, normal and `--bytecode`, no `continue-on-error`: a step
       that fails should fail the job, otherwise the run reports green while
       silently hiding a stress regression (the issue's own point). Use
       `if: ${{ !cancelled() }}` on the bytecode step so it still runs even
       if the normal-mode step already failed:
       ```yaml
       - name: GC stress sample (JSSE_GC_STRESS=2)
         run: JSSE_GC_STRESS=2 uv run python scripts/run-test262.py --jsse ./target/release/jsse --test262 ./test262 --sample 0.003 --seed "$GC_STRESS_SEED" --timeout 300

       - name: GC stress sample (JSSE_GC_STRESS=2, bytecode VM)
         if: ${{ !cancelled() }}
         run: JSSE_GC_STRESS=2 uv run python scripts/run-test262.py --jsse ./target/release/jsse --test262 ./test262 --sample 0.003 --seed "$GC_STRESS_SEED" --bytecode --timeout 300
       ```
       `N=2` deliberately sits below CLAUDE.md's "use `N=16..1000` for broad
       runs" guidance. Trade-off: lower `N` collects more often and is more
       likely to surface a missing root, which is exactly what "higher
       intensity" in the issue means, but costs more wall time per test and
       risks the job being red often enough to get ignored. The §1
       measurement (`--sample 0.003`, `N=2`, 67s wall at `-j 12` for 3381
       scenarios, 11 real regressions found) is the evidence this specific
       combination is both affordable and productive; revisit `N` upward
       only if the nightly job turns out to be dominated by timeout noise
       rather than real findings.
     - No `--fail-on-failures`/baseline semantics needed beyond the runner's
       default regression check — a schedule/workflow_dispatch-only trigger
       can't block `pull_request` or `push`, so a red run here never blocks
       a merge; it only shows up in the Actions tab and (if configured)
       notifies on failure, same visibility model as
       `nightly-test262-coverage.yml`.
   - Green: implementation stage cannot use `gh workflow run` to test a
     workflow file that only exists on a feature branch (`workflow_dispatch`
     requires the file to already be on the default branch), so verification
     is: (a) the exact commands above, run locally with a small seed before
     committing — already done in §1 (`--sample 0.003 --seed 1`, found the 11
     regressions, confirming the step *can* fail and *does* report
     correctly); (b) `workflow-lint.yml` (actionlint + zizmor) clean on the
     new file. First real exercise of the `schedule`/`workflow_dispatch`
     trigger happens after merge.

3. **File tracking issues for the §1 findings.** Before or immediately after
   the nightly job's first scheduled run, file one `gh issue create` per
   distinct bug (the 6 distinct test files in §1, 11 counting `:strict`
   variants) so the first nightly red isn't an orphaned, unexplained
   failure — same pattern as #794–#798. This is a recommendation for the
   implementation stage to act on, not a code change in this plan.

4. **Doc sync.** Add one bullet under `CLAUDE.md`'s "## GC Stress Mode"
   noting: CI now runs `JSSE_GC_STRESS=7` blocking over `test262-extra/`
   (normal + bytecode), and a daily nightly non-blocking job samples the full
   suite at higher intensity (`N=2`, reproducible per-run via a logged seed).
   No code change; verified by re-reading the section for consistency with
   the actual workflow files.

## 5. Test surface

- No `test262/test/...` directory is newly exercised — this issue adds a
  stress *mode* to an existing `test262-extra/` run, not new test content.
- No spec-correct behavior is introduced, so no new `test262-extra/` or
  `tests/` file is needed for engine semantics.
- The actual gate for this change: `.github/workflows/workflow-lint.yml`
  (actionlint + zizmor) validates the new/edited YAML. Functional
  verification is running the exact `uv run python scripts/run-test262.py`
  invocations locally before committing — already done for both slices (see
  §1): blocking gate 971/971 clean at `-j 2 --timeout 300`; nightly command
  found its expected 11 regressions at `--sample 0.003 --seed 1`.
  `uv run python -m unittest discover -s scripts -p 'test_*.py'` stays green
  since no `scripts/*.py` file changes.

## 6. Regression risk

- `test262-pass.txt` cannot move: no `src/` change, and the plan does not
  touch the baseline or pass `--update-baseline` anywhere.
- No engine hot path (`eval_expr`/`exec_statement`, `property.rs`, `gc.rs`,
  `ObjectKind` matches, bytecode VM, library harnesses) is touched.
- Main real risk is CI wall-clock and flakiness, not correctness:
  - Stress mode measurably slows individual scenarios (16.8s worst case for
    the blocking gate at `-j 2`, 61.5s worst case for the nightly's broader
    `test262/` sample at `N=2`, vs. sub-second unstressed). `--timeout 300`
    on every stress step covers both with margin. If a CI runner is ever
    slow enough to blow that margin, the right fix is raising `--timeout`,
    not lowering `N` or weakening `--fail-on-failures` (matches the existing
    "Timeouts under stress are cost, not GC bugs" triage note).
  - The two new blocking steps add ~62s combined to the `build` job
    (measured: 43.4s + 18.6s wall at `-j 2 --timeout 300`); negligible next
    to the job's existing ~10+ minute wall time.
  - The nightly sampled job **will** turn up new, previously-unknown stress
    failures on `test262/` paths that `test262-extra/` doesn't cover — not a
    hypothetical, confirmed directly in §1 (11 regressions from one 67s
    sampled run). That is the intended outcome, not a regression from this
    change. Do not treat a nightly red run as something this PR broke; file
    each as its own issue per slice 2's step 3, the same way #794–#798
    originated.

## 7. Out of scope

- **Arena-id quarantine mode** (the issue's "also consider"): disabling
  `ObjectArena::free`'s id recycling under stress so a stale id reads as a
  hard "dead object id" panic instead of a silently wrong-typed live object.
  This is a real engine change (`src/interpreter/object_arena.rs`,
  `src/interpreter/gc.rs`), needs its own `$262.gc()`-based
  `test262-extra/` regression proving the deterministic panic, and is
  explicitly optional ("consider") in the issue text. Filing as a follow-up
  issue rather than bundling it into a CI-wiring PR.
- Combining `JSSE_GC_STRESS` with the `release-checked` (debug-assertions)
  binary in CI for extra rigor — not requested by the issue; a reasonable
  future enhancement, not required to close it.
- Automating issue-filing from nightly job failures — left as manual triage
  via the Actions tab, consistent with how `nightly-test262-coverage.yml`
  already works (no auto-filing there either).
- Fixing any new stress bugs the nightly sampled job surfaces once it starts
  running — out of scope for this issue; triage separately as they appear.
- Any refactor of `scripts/run-test262.py` itself — `--fail-on-failures`,
  `--bytecode`, `--sample`, and `JSSE_GC_STRESS` env passthrough already
  exist and work as needed; no script change required.
