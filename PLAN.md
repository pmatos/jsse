# Plan: issue #361 — tweetnacl-js harness: exhaustive vector coverage (currently sampled)

## 0. Branch is stale — rebase first

This workspace's branch (`sym/jsse/361-...`) was created at `fdec34fd`, which
(before this plan's own commit) had **zero unique commits** and was a strict
ancestor of `origin/main` (now at `60fc5520`). The only commit this branch
now carries beyond that point is this plan's own `PLAN.md` commit, which
touches no file `origin/main` has changed, so
`git fetch origin main && git rebase origin/main` replays just that one
commit with no conflicts possible (not a literal fast-forward anymore, but
conflict-free for the same reason a fast-forward would be). This must happen
before any other step below, because `origin/main` already contains:

- `fa02cc6c` (#605): a prior triage pass on *this issue* that benchmarked the
  bytecode VM against tweetnacl, found it didn't move the workload
  (1.00-1.07x), identified that `M`/`car25519`/`sel25519` bailed out of the
  bytecode compiler on `new` and compound member assignment, split that off
  as issue **#603**, and rewrote the `scripts/libs/tweetnacl-js.sh` header +
  `scripts/README.md` section to say so. The file content this plan was
  written against is `origin/main`'s, not this stale checkout's.
- `4267239f` (#611): closed #603 — the bytecode compiler now handles `new`
  expressions and compound member-target assignment.

All quotes, line numbers, and the "current state" described below are from
`origin/main` as of `60fc5520`, fetched and inspected read-only during
planning (not merged into this branch).

## 1. Problem restated

`scripts/libs/tweetnacl-js.sh` samples the three curve-heavy tweetnacl-js
vector files (`scalarmult.random.js`, `box.random.js`, `sign.spec.js`) down
to 20 entries each because curve25519/Ed25519 field arithmetic was too slow
on jsse's tree-walker to run the full upstream counts (256/256/1024) in a
practical harness run — projected ~6h at the time of #300. Issue #361 tracks
raising those caps (or removing the sampling entirely) once the engine gets
"a meaningfully faster numeric path." #603/#611 was the prerequisite that
triage identified and has since landed. This plan's job is to re-measure
against the current engine, decide how much headroom that actually buys, and
either raise the caps by a justified amount or show why the precondition
still isn't fully met — not to guess a number.

## 2. Spec basis

**N/A: no JavaScript behavior change.** This issue only touches the
Node-compat library harness (`scripts/libs/tweetnacl-js.sh`,
`scripts/README.md`) and adds a `docs/perf/` write-up. No `src/` file is
planned to change, so no spec clause governs it. (The bytecode-compiler gap
this re-measurement surfaces, §8, is scoped out of this PR precisely because
*that* would be a `src/` behavior-neutral-but-code-changing fix needing its
own plan.)

## 3. Re-measurement already performed during planning (read-only; discard before implementation)

To avoid planning blind, I built `origin/main` (`60fc5520`) in a scratch
worktree (`cargo build --release`, outside this branch, nothing committed
here) and re-ran the existing benchmark asset
(`docs/perf/2026-09-05/bench-tweetnacl.js`, on `origin/main` only — not in
this stale checkout) plus the real harness:

- **Isolated microbench, min of 3 reps, default (non-instrumented) binary:**
  `scalarMult.base` 739.8ms/op (was 2.98s on 2026-09-05 → **~4.0x faster**),
  `scalarMult` 739.2ms/op, `sign.detached.verify` 2.54s/op (was 10.8s →
  **~4.25x faster**). This is the *default* tree-walker path — unrelated to
  `--bytecode` — so it is general interpreter perf work landed on `main`
  since 2026-09-05, not #611 specifically.
- **`--bytecode` on top of that:** a further, consistent **~1.4x** on all
  three phases (e.g. `scalarMult.base` 739.8ms → 529.2ms). This *is* #611:
  `perf-counters` confirms `M`'s `new Float64Array(31)` and `car25519`'s
  `o[i] += x` now compile. But a new wall appears: `car25519` itself (98.9%
  of remaining tree-walked AST work, all three phases) now bails with
  reason `"call callee"` — `compiler.rs:466-474`'s `compile_call` only
  accepts a bare `Expression::Identifier` callee, so `Math.floor(...)`
  (car25519's one call, every loop iteration) still falls back to the
  tree-walker. Confirmed by source inspection (`nacl.js:271-280`).
- **Real end-to-end harness, today's 20/20/20 sampled caps, cached repo,
  default build:** `./scripts/run-library-tests.sh tweetnacl-js` →
  **8m22s measured**, vs. the **~22min documented for 2026-09-05 — which
  `tweetnacl-bytecode-null-result.md` itself flags as a projection, never
  validated end-to-end** (its scope note: "neither validated against an
  end-to-end harness run"). So the honest comparison is a real measurement
  against an old projection (**~2.6x, if the projection held**), not two
  measurements — treat the 2.6x figure as a lower-confidence number than the
  isolated-microbench ~4x, and say so in the perf doc (step 6 below).
- **Not run to completion:** a 100-vector (5x) variant, meant to empirically
  calibrate scaling for a raised-cap candidate. It was still running past
  11 minutes of jsse-process CPU time (exceeding the *entire* 20-vector
  harness run) when stopped by PID. This is consistent with — but does not
  by itself establish — **near-linear scaling with vector count**, which is
  *expected* (each vector is an independent, equal-cost call with no shared
  setup) but was not actually measured to completion. Don't carry "scales
  linearly" into the implementation stage as a confirmed fact; it's the
  working assumption §5 step 3 is designed to replace with a real number.
  This attempt also left the shared `/tmp/jsse-libtests/tweetnacl-js/`
  harness cache holding a 100-vector `final.cjs` bundle — **removed** before
  this plan was committed, but any future interrupted run leaves the same
  trap (see §5 step 3's `--clean` note).
- **Confirmed the harness never passes `--bytecode` anywhere**
  (`scripts/run-library-tests.sh`, `scripts/libs/*.sh` — no occurrence). The
  ~1.4x `--bytecode` number above is informational only; it does not apply
  to this harness unless a later change opts this config into `--bytecode`
  (see §8 — not planned here).

**Net reading:** the *default* path (what the harness actually runs) is
genuinely and meaningfully faster — roughly 2.6-4x — since the 2026-09-05
baseline, with the lower end being measurement-vs-projection and the higher
end being measurement-vs-measurement (isolated microbench). That is real
headroom, but the old cost model needed ~17x (sampled→full, holding today's
wall time) or ~6x (to fit inside the existing 3600s `LIB_TIMEOUT`), both
also projections. 2.6-4x does not clear either bar. Raising caps to the
*full* upstream counts in this PR is not empirically justified yet; a
smaller, measured raise is — and §5 step 3 is designed to get a real number
instead of stacking another projection on top of these.

## 4. Files to touch

- `scripts/libs/tweetnacl-js.sh` — the `sample(data, 20)` call sites (3x, one
  `node -e` block) and the header comment's measurement paragraph.
  `LIB_EXPECT_COUNT` and `LIB_TIMEOUT` move only if the chosen caps change
  them (see §5, step 3).
- `scripts/README.md` — the "tweetnacl-js curve25519/Ed25519 corpus" section
  (currently ends by pointing at
  `docs/perf/2026-09-05/tweetnacl-bytecode-null-result.md`); update the
  measurement summary and the `docs/perf/` pointer.
- `docs/perf/<implementation-run-date>/` (new; use the actual date the
  implementation stage runs this, not this planning session's date) —
  write-up following the `2026-09-05/tweetnacl-bytecode-null-result.md`
  convention: a markdown report plus its supporting `bench-tweetnacl.js`
  (can reuse the existing one verbatim), counter dumps, and a timings table.
- No `src/` changes.

## 5. TDD slices

This is a measurement-and-documentation change, not new logic under test, so
the "red/green" here is "the harness count lock is wrong until re-measured
against the new caps, then right." Each slice is still independently
reviewable.

1. **Rebase.** `git fetch origin main && git rebase origin/main` (conflict-free
   replay of the single `PLAN.md` commit, per §0). Confirms
   `scripts/libs/tweetnacl-js.sh` matches the `origin/main`
   content quoted in §3 before editing it.
2. **Reproduce the baseline.** Run `./scripts/run-library-tests.sh
   tweetnacl-js` once at today's unchanged 20/20/20 caps. Expect green,
   `LIB_EXPECT_COUNT=5470`, cross-checked against Node, in a few minutes
   (not ~22min — confirms §3's numbers on the implementation stage's own
   checkout rather than trusting the planning-stage scratch measurement).
3. **Pick a candidate cap by arithmetic from one timestamped run, not by a
   live binary search.** Each probe run is 15-60+ minutes; this doesn't fit
   repeated iterate-and-retime inside one headless turn (a plain
   `run-library-tests.sh` invocation piped through anything buffering, e.g.
   `tail`, also hides all output until the process exits — confirmed the
   hard way during planning, §3). Instead:
   - Run the generated bundle directly the way the harness does
     (`jsse --node /tmp/jsse-libtests/tweetnacl-js/final.cjs`, cache already
     warm from step 2 — no `--clean`), with each line timestamped as it's
     produced so partial progress is visible even if the run is still going
     when checked:
     `... | while IFS= read -r l; do printf '%s %s\n' "$(date +%s)" "$l"; done > "$TMPDIR/tweetnacl-timed.log"`
     (under `$TMPDIR`, not a literal `/tmp/...` path, per the environment's
     scratch-budget rule).
   - tape prints a `# <description>` header per `test()` block, including
     one around each of `test/05-scalarmult.js`, `test/06-box.js`, and
     `test/08-sign.js`'s random/spec-vector loops. The gap between
     consecutive headers' timestamps gives real per-file wall time; dividing
     by today's cap (20) gives real per-vector cost, independently for each
     file — no live retiming needed.
   - Compute each file's projected wall time at a candidate cap `n`:
     `today's per-file time + (n - 20) × per-vector cost`, and from that the
     total projected harness wall time at that candidate.
   - Decision rule: **pick the largest per-file caps (they need not be
     equal — `sign.spec.js` has both the most upstream vectors, 1024 vs
     256/256, and the heaviest per-op cost, so expect it to land lowest)
     whose projected total stays at or below ~80% of `LIB_TIMEOUT`**,
     raising `LIB_TIMEOUT` itself only if the result is still below the
     `esprima` config's accepted ~65min precedent (`scripts/README.md`'s
     harness table) — don't make tweetnacl-js the new outlier on a
     docs-only PR without calling it out explicitly in the PR description.
   - Rough expectation to sanity-check the arithmetic against (a guide for
     spotting a bad measurement, not a number to ship without verifying):
     scalarMult costs ~0.74s/op, so scalarmult.random.js at full 256 adds
     roughly 3 minutes over today; box does 1-2 scalarMults per vector, so
     box.random.js at full 256 is roughly 3-6 minutes; sign.spec.js does a
     sign *and* a verify per vector at ~3s+ combined, so its full 1024 (1004
     more than today) is on the order of 50+ minutes alone — likely the one
     that stays capped. If scalarmult/box project comfortably to full count
     and sign.spec.js doesn't, that's the expected outcome, not a surprise.
   - If the arithmetic says all three files reach full upstream counts
     (256/256/1024) within budget: remove the `sample()` call entirely
     (delete the truncation block in `lib_prepare`) and skip straight to
     step 3a's validation run.
   - Otherwise, pick the largest caps per file that the arithmetic says fit,
     edit `scripts/libs/tweetnacl-js.sh` accordingly, **commit that edit**
     (checkpoint before the next, long-running step), then:

   **3a. Validate for real, once.** Run
   `./scripts/run-library-tests.sh tweetnacl-js --clean` (the `--clean` is
   required any time the caps change — a warm cache's data files are already
   truncated from a prior cap, and `sample()`'s own `if (arr.length <= n)
   return arr` means re-running prepare against a cache already at or below
   the new `n` is a silent no-op, not a re-sample; a fresh clone is the only
   way to re-truncate from the true upstream arrays). Background it
   (`run_in_background`) writing to an unbuffered file, and poll that file's
   mtime/size rather than waiting idle. If the real run lands meaningfully
   off the arithmetic projection, prefer the measurement and adjust the caps
   (and re-validate) rather than shipping the projected number.
   - If this confirms all three files at full upstream counts: this closes
     #361 outright.
   - If one or more files (almost certainly `sign.spec.js`) stay below full
     count: this PR is a partial step — see §5a on `Closes` vs `Refs`.
4. **Re-lock the count.** Whatever caps are chosen, run the Node reference
   (`run-library-tests.sh tweetnacl-js` already cross-checks both engines)
   to get the new exact assertion count and update `LIB_EXPECT_COUNT`.
5. **Update prose.** `scripts/libs/tweetnacl-js.sh` header comment and the
   `scripts/README.md` section: new caps, new measured wall time, new
   `docs/perf/<implementation-run-date>/...` pointer, and (only if caps are
   now full upstream) remove the sampling-note language entirely per the
   issue's ask.
6. **Write the perf doc.**
   `docs/perf/<implementation-run-date>/tweetnacl-bytecode-recheck.md` (or
   similar name): states #611 landed, the default-path speedup is real
   (~2.6-4x, flagging that the low end compares against the 2026-09-05
   projection rather than a prior measurement — §3) but the bar was ~6-17x
   (also a projection), gives the final chosen caps and the measured wall
   time that justified them, and documents the *new* bytecode gap found
   (`compile_call` identifier-only callee restriction blocking `car25519`'s
   `Math.floor` call) as the reason `--bytecode` doesn't close the remaining
   gap either — mirroring `tweetnacl-bytecode-null-result.md`'s structure
   (repro command, counter dumps, bail table).
7. **File the follow-up issue** (§8) via `gh issue create`, and link it from
   both the new perf doc and the `tweetnacl-js.sh` header, the same way
   `fa02cc6c` linked #603.

### 5a. `Closes` vs `Refs` — decide which before opening the PR

- All three files reach full upstream counts → PR title fixes #361, use
  `Closes #361` in the PR body, sampling language is fully removed.
- Any file stays sampled (even at a larger count) → the corpus is still not
  "exhaustive." Use `Refs #361` (not `Closes`), keep #361 open, and the PR
  title must still be a valid Conventional Commits subject, since the
  squash-merge takes it verbatim — follow the `test: add tweetnacl-js
  Node-compat harness (#300)` precedent, e.g. `test: raise tweetnacl-js
  curve-vector sample caps`. Put the "partial, #361 stays open" fact in the
  PR body, not the title.

## 6. Test surface

Engine-only gates don't apply (no `src/` change). The gates that do:

- `./scripts/run-library-tests.sh tweetnacl-js` — the harness itself; must
  stay green with the new `LIB_EXPECT_COUNT`, cross-checked against Node, and
  complete within whatever `LIB_TIMEOUT` ends up being.
- `./scripts/run-library-tests.sh tweetnacl-js --node` — reference run to
  derive the new locked count (step 4 above).
- No `cargo test --release` dependency — nothing under `src/` moves. Still
  worth a sanity `cargo build --release` if any shared shim file were touched
  (it isn't, per §4).
- `./scripts/lint.sh` — the repo's general gate; the new `docs/perf/` files
  are plain markdown/JS assets like every prior perf write-up, not covered
  specially.

## 7. Regression risk

- **Harness wall time / CI budget**: the main risk. A wrong cap choice could
  make `tweetnacl-js` the harness's new long pole (currently `esprima` at
  ~65min). Mitigated by the measure-then-compute-then-validate approach in
  §5 step 3 rather than jumping straight to full upstream counts.
- **`LIB_EXPECT_COUNT` drift**: if the Node reference run (step 4) isn't
  re-run against the *exact* same caps that ship, the locked count silently
  mismatches and the harness's own mismatch guard (`run-library-tests.sh`'s
  cross-check) will catch it — but only if step 4 is actually re-run, so
  call this out in the PR description as a thing to double check.
- **No shared engine machinery is touched** (tree-walker hot paths, property
  MOP, GC rooting, `ObjectKind`, bytecode compiler) — this PR only edits
  harness config and docs, so `test262-pass.txt` cannot move and there is no
  interaction with GC safepoints or the bytecode fast path to worry about.
- **Stale branch**: §0's rebase is itself a small risk surface if skipped —
  skipping it means re-deriving (and likely re-breaking) work `fa02cc6c`
  already did, and the eventual squash-merge would conflict with or silently
  overwrite that already-landed text.

## 8. Out of scope

- **Fixing the bytecode compiler's `compile_call` identifier-only callee
  restriction** (`compiler.rs:466-474`) so `car25519`'s `Math.floor(...)`
  compiles. This is a real, now well-isolated gap — confirmed via
  `--features perf-counters` to hold 98.9% of the remaining tree-walked work
  across all three curve-heavy phases — but it's a bytecode-compiler feature
  change under `src/interpreter/bytecode/`, needs its own test plan against
  the compiler's existing test suite, and (per §3) wouldn't even help this
  harness today since nothing passes `--bytecode` to it. File it as a new
  issue (mirroring how #603 was split out of the 2026-09-05 triage) rather
  than fixing it here. No open issue currently tracks it — the closest prior
  art is #524, which named "`this` and method-call callees" as a known,
  undone item.
- **Switching this harness to run under `--bytecode`.** Even though §3 found
  a real ~1.4x gain there, that's a separate, independently risky decision
  (changes what the harness is actually exercising; mandreel's #526 shows
  `--bytecode` coverage doesn't always translate to wall-clock wins) that
  deserves its own issue and isn't needed to make progress on #361's actual
  ask (raising vector counts on the default path).
- **Raising `LIB_TIMEOUT` far beyond the `esprima` precedent** (~65min) in
  pursuit of full upstream counts regardless of cost. If §5 step 3's
  arithmetic shows full counts need, say, 2+ hours, land the largest caps that fit
  comfortably instead and leave #361 open (`Refs`, not `Closes`) rather than
  normalizing a multi-hour harness run in this PR.
- **Any other library harness config** — this plan touches only
  `scripts/libs/tweetnacl-js.sh`.
