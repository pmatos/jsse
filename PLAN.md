# Plan: issue #607 — interpreter depth guards don't fire before native stack overflow in debug builds

## 0. Status after prior attempt (this workspace was reused)

This workspace already carries a full implementation of this plan from an
earlier attempt, committed on this branch before this run started:

- `944508b` test(interpreter): add engine-stack regression tests for depth
  guards (slice 1 + slice 2, red phase — `deep_recursion_before_fix_call_depth`
  / `deep_recursion_before_fix_eval_depth`, confirmed release-green /
  debug-SIGABRT before the fix).
- `3f5e284` fix(interpreter): calibrate `CALL_DEPTH_*`/`EVAL_DEPTH_LIMIT` per
  build profile (slice 3 — the `_DEBUG`/`_RELEASE` split + compile-time
  coupling assertion described in §4/§5 below, landed with exactly the
  proposed values: `REARM` 120/3,000, `SOFT` 160/4,000, `HARD` 200/5,000,
  `EVAL_DEPTH_LIMIT` 2,000/50,000, `PROXY_CHAIN_DEPTH_LIMIT` unchanged at
  4,000. Also renamed the slice-2 tests off their red-phase `_before_fix`
  name to `deep_call_recursion_raises_error_before_native_overflow` /
  `deep_expression_nesting_raises_error_before_native_overflow` now that
  they're permanently green, and fixed `src/parser/mod.rs`'s stale
  forward-reference comment to #607.)
- `9d523cf` test(recursion): cover Proxy apply-trap and member-chain shapes
  for both profiles (slice 4 — extended `tests/recursion-limit-interpreter.js`
  with the two new hungry shapes and re-sized the existing additive/logical
  cases and the leak-check probe for the new debug `EVAL_DEPTH_LIMIT`).

I re-ran slice 5 (the full regression pass this plan calls for) in this
session before writing this status note, rather than trusting the commit
messages alone:

- `cargo test --release`: 650 passed, 0 failed, 1 ignored.
- `cargo test` (debug): 650 passed, 0 failed, 1 ignored.
- `deep_call_recursion_raises_error_before_native_overflow` and
  `deep_expression_nesting_raises_error_before_native_overflow` both pass on
  debug and release.
- `uv run python scripts/run-custom-tests.py --jsse ./target/debug/jsse tests/recursion-limit-interpreter.js`
  → 1/1 pass (the exact debug-build repro from the issue body, now green).
- `uv run python scripts/run-custom-tests.py tests/recursion-limit-interpreter.js`
  (release) → 1/1 pass.
- `git diff 9b9b6b7..HEAD --stat` touches exactly the four files §5 names
  (`src/interpreter/mod.rs`, `src/interpreter/tests.rs`,
  `tests/recursion-limit-interpreter.js`) plus the one-line stale-comment fix
  in `src/parser/mod.rs` that §5/§9 anticipated — nothing stray.

Everything this plan asked for is implemented and green. While implementing
slice 4, the prior attempt found a distinct, narrower native-stack gap in the
*parser* on very deep flat/member-chain expressions and filed it separately
rather than folding it into this PR: jsse#612 (open), out of scope here.

**What is not yet done, and is not something the planning stage can do:**
the branch has never been pushed to `origin` and no PR exists yet (confirmed
via `git ls-remote --heads origin <this branch>` and `gh pr list --head
<this branch> --state all`, both empty). Closing out this issue only needs
the implementation stage to push this branch and open the PR against
`main` — no further source or test changes are needed. An unrelated stray
`EVIDENCE.md` (untracked, from a separate blocked `/simplify` run against
this same reused workspace that predates a PR existing) is left in place
for that stage to see and is not part of this plan's file list.

## 1. Problem restated

`CALL_DEPTH_SOFT_LIMIT`/`CALL_DEPTH_HARD_LIMIT`/`CALL_DEPTH_REARM_LIMIT` and
`EVAL_DEPTH_LIMIT` (`src/interpreter/mod.rs`) bound native-stack-consuming
recursion in the tree-walker (`call_function_inner` for JS calls,
`eval_expr` for expression evaluation) so that exceeding them raises a
catchable `RangeError` instead of exhausting the 128 MiB engine stack
(`run_on_engine_stack`, `src/lib.rs`) and SIGABRTing. All four constants were
calibrated once, against release-optimized frame sizes. Debug frames are
several times larger, so on a debug build the native stack runs out *before*
any of these counters reach their threshold — the guard never gets a chance
to fire and the process aborts. This is the exact sibling bug #606 already
fixed for the parser's `MAX_PARSE_DEPTH`; #607 asks for the same treatment
for the interpreter's four constants, kept coupled per the issue body
(`REARM < SOFT < HARD`, and `EVAL_DEPTH_LIMIT` clear of what
`CALL_DEPTH_HARD_LIMIT` can reach through a call's own expression evaluation).

## 2. Spec basis

N/A: no JavaScript behavior change. These constants are engine-internal
resource limits, not an ECMAScript-observable syntax or semantics rule — the
spec imposes no bound on call or expression-nesting depth, and a program that
stays under the new debug limits behaves identically to today (same
`RangeError`, same recoverability). This change only moves *when*, on a debug
build, native resource exhaustion becomes a catchable error rather than a
process abort; it does not change *whether* deep recursion eventually throws,
nor the release-profile behavior at all. (Same framing #606 used for
`MAX_PARSE_DEPTH`, `PLAN.md` for #599/#606.)

## 3. Measurements taken during planning

Guard-disabled binary search (same method as #606: bump the constant under
test to a value effectively unreachable, build, binary-search the JS input
size against exit code — 0 vs SIGABRT/134), run in this workspace today.
Boundaries are `[last success, first abort]`. All source edits used for
measurement were reverted before writing this plan; the working tree is
clean.

**Debug build** (`cargo build`, `target/debug/jsse`):

| Shape | Guard it exercises | Native abort boundary |
|---|---|---|
| `function f(n){if(n<=0)return 0;return 1+f(n-1);}` | `call_depth` | 2311–2342 |
| recursive getter (`get x(){...return 1+obj.x;}`) | `call_depth` | 2386–2413 |
| `Proxy` `apply` trap forwarding to `target.apply` | `call_depth` | **1018–1046** (hungriest call shape measured) |
| flat `1+1+1+…` | `eval_depth` | 8277–8308 |
| self-referential member chain `a.b.b.b…` | `eval_depth` | **6974–7011** (hungriest pure-`eval_depth` shape measured) |
| self-referential index chain `a[0][0][0]…` | `eval_depth` | 6974–7011 |
| self-referential call chain `f()()()…` | both (compound) | 6083–6120 |
| `Proxy` prototype-chain walk (no `getPrototypeOf` trap) | `PROXY_CHAIN_DEPTH_LIMIT` | 54092–54108 |

**Release build** (`cargo build --release`, `target/release/jsse`), for
baseline confirmation:

| Shape | Guard | Native abort boundary |
|---|---|---|
| plain call recursion | `call_depth` | 30316–30409 |
| flat `1+1+1+…` | `eval_depth` | 190560–190679 |

Two findings change the shape of the fix from "recalibrate four constants" to
"recalibrate three, leave one alone, fix one stale comment":

- **`PROXY_CHAIN_DEPTH_LIMIT` (4,000) is not miscalibrated.** Its debug native
  capacity (~54,000) is nowhere near its limit (4,000) — a 13.5x margin, wider
  than any other guard in this file. It already fires safely in debug. It
  does **not** need a `cfg!(debug_assertions)` split.
- The member-access/flat-expression "parser gap" #606's commit message
  flagged (`a.b.b…`, `1+1+1…` bypass the *parser's* recursive-descent depth
  counter because those productions parse iteratively) is **already covered
  on the interpreter side**. Confirmed directly: `eval("a"+".b".repeat(80000))`
  and the existing `evalMustRangeError` calls in
  `tests/recursion-limit-interpreter.js` throw a catchable `RangeError` today,
  in release, via `EVAL_DEPTH_LIMIT` — `eval_expr` recurses once per AST node
  regardless of how that node was produced. Nothing new needed here; noted so
  it isn't reopened as a gap.

## 4. Proposed constants

Same shape as #606: preserve release's existing ratios
(`REARM = 0.6×HARD`, `SOFT = 0.8×HARD`), pick a debug `HARD` with comparable
margin under the *hungriest measured shape for that guard*, and keep the
`EVAL_DEPTH_LIMIT`-above-`CALL_DEPTH_HARD_LIMIT` coupling the issue calls out.

| Constant | Release (unchanged) | Debug (new) |
|---|---|---|
| `CALL_DEPTH_REARM_LIMIT` | 3,000 | 120 |
| `CALL_DEPTH_SOFT_LIMIT` | 4,000 | 160 |
| `CALL_DEPTH_HARD_LIMIT` | 5,000 | 200 |
| `EVAL_DEPTH_LIMIT` | 50,000 | 2,000 |
| `PROXY_CHAIN_DEPTH_LIMIT` | 4,000 | 4,000 (unchanged) |

Margins these give, against the measurements in §3:

- `CALL_DEPTH_HARD_LIMIT` debug (200) vs. the hungriest measured call shape,
  `Proxy` apply-trap forwarding (~1,020): **5.1x**. Vs. plain/getter
  recursion (~2,300–2,390): ~11.5x. (Release's own margin, plain recursion
  30,316 / 5,000, is 6.06x — the same order of magnitude.)
- `EVAL_DEPTH_LIMIT` debug (2,000) vs. the hungriest measured pure-`eval_depth`
  shape, member/index chains (~6,974–7,011): **3.5x**. (Release: 190,560 /
  50,000 = 3.81x — matched.)
- Coupling: `EVAL_DEPTH_LIMIT` debug (2,000) vs. `CALL_DEPTH_HARD_LIMIT` debug
  × a few `eval_expr` frames per call level (200 × ~5 = 1,000): **2x**
  headroom, matching release's own ratio (50,000 vs. 5,000 × ~4 = 20,000 →
  2.5x). This is what keeps ordinary deep *call* recursion bounded by
  `call_depth` (and its soft/hard recovery band) rather than tripping
  `eval_depth` first, exactly as the existing release-profile doc comment on
  `EVAL_DEPTH_LIMIT` describes.

These are the planning-stage candidates, not a promise of the final numbers.
Slice 2 below is a red/green loop specifically because call-shape stack cost
varies (plain call vs. getter vs. `Proxy` trap forwarding already showed a
2.3x spread) and an unmeasured shape (e.g. `super()` chains, `Array.prototype`
callback recursion, class field initializers, generator resumption) could be
hungrier still. If the implementation stage's regression tests (slice 2) find
any shape that still aborts, tighten the debug constant(s) and re-run the
binary search — do not ship numbers a red test contradicts.

**This debug `HARD_LIMIT` (200) sits *below* what the existing release-profile
doc comment calls "the depth any real program reaches ... depths under
~1500."** Unlike #606 (debug `MAX_PARSE_DEPTH` 400 was still comfortably above
test262's deepest real nesting, ~192 units), a legitimately-recursive JS
program between depth 200 and ~1500 that runs fine in release will now throw
`RangeError` on a debug build. This is acceptable — every automated gate
(§8) builds release, so debug is a developer/debugging profile, not a
conformance target — but it is a real, user-visible behavior difference
between profiles that the doc comment and PR description must state plainly,
not inherit release's "neither limit is near real code" framing verbatim.

## 5. Files to touch

- `src/interpreter/mod.rs` — make `CALL_DEPTH_REARM_LIMIT`,
  `CALL_DEPTH_SOFT_LIMIT`, `CALL_DEPTH_HARD_LIMIT`, `EVAL_DEPTH_LIMIT`
  `cfg!(debug_assertions)`-conditional (same `if cfg!(debug_assertions) { X } else { Y }`
  shape #606 used for `MAX_PARSE_DEPTH` in `src/parser/mod.rs:101`). Rewrite
  their doc comments in #606's structure: measured capacity per profile, the
  margin chosen, and a pointer to this issue.

  A single `if cfg!(...)` expression only ever const-evaluates the arm that
  gets compiled — `const _: () = assert!(...)` written against the public
  constant name would silently check one profile's numbers only, and since
  every CI job builds `--release` (§8), the debug arm's coupling would never
  be compile-checked anywhere. To actually check both arms on every build,
  name them explicitly and assert over the named pairs, e.g.:
  ```rust
  const CALL_DEPTH_HARD_LIMIT_DEBUG: usize = 200;
  const CALL_DEPTH_HARD_LIMIT_RELEASE: usize = 5_000;
  pub(crate) const CALL_DEPTH_HARD_LIMIT: usize =
      if cfg!(debug_assertions) { CALL_DEPTH_HARD_LIMIT_DEBUG } else { CALL_DEPTH_HARD_LIMIT_RELEASE };
  // ...same pattern for REARM/SOFT and EVAL_DEPTH_LIMIT...
  const _: () = assert!(
      CALL_DEPTH_REARM_LIMIT_DEBUG < CALL_DEPTH_SOFT_LIMIT_DEBUG
          && CALL_DEPTH_SOFT_LIMIT_DEBUG < CALL_DEPTH_HARD_LIMIT_DEBUG
          && EVAL_DEPTH_LIMIT_DEBUG > CALL_DEPTH_HARD_LIMIT_DEBUG * 5
          && CALL_DEPTH_REARM_LIMIT_RELEASE < CALL_DEPTH_SOFT_LIMIT_RELEASE
          && CALL_DEPTH_SOFT_LIMIT_RELEASE < CALL_DEPTH_HARD_LIMIT_RELEASE
          && EVAL_DEPTH_LIMIT_RELEASE > CALL_DEPTH_HARD_LIMIT_RELEASE * 5
  );
  ```
  This is more verbosity than a single assert on the public names, but it's
  what actually delivers "the coupling is checked regardless of which
  profile you happen to build" — worth it for four constants that are this
  easy to accidentally decouple later. Also fix `PROXY_CHAIN_DEPTH_LIMIT`'s doc comment (line ~427): it
  currently claims "Keeping this below the JS call-depth ceiling leaves
  enough native stack" — that relationship no longer holds once
  `CALL_DEPTH_HARD_LIMIT` drops to 200 in debug (4,000 > 200), yet the
  constant itself remains safe on its own measured margin (13.5x). Reword to
  state the real invariant: its own native capacity, not its position
  relative to `call_depth`, is what keeps it safe. No change to its value.
- `src/interpreter/tests.rs` — add an engine-stack test helper (see slice 1)
  and the Rust-level regression tests (slice 2).
- `tests/recursion-limit-interpreter.js` — extend with the two additional
  hungry shapes measured in §3 (member chain, `Proxy` apply-trap forwarding)
  alongside the existing additive/logical `evalMustRangeError` cases, mirroring
  how #606 broadened `tests/recursion-limit-parser.js`. Fix the leak-check
  probe at the bottom of the file (see slice 3) so it doesn't regress under
  the new debug `EVAL_DEPTH_LIMIT`.

No `docs/adr/` entry: this is a calibration fix following an established
pattern (#606), not a new architectural decision.

## 6. TDD slices

1. **Engine-stack test helper.** Add a helper in `src/interpreter/tests.rs`
   analogous to the parser's `parse_on_engine_stack` (`src/parser/mod.rs:1439`):
   runs a fresh `Interpreter` against `source` inside
   `crate::run_on_engine_stack`, returning a `Send`-safe verdict
   (`Result<(), String>`, using `interp.format_value(&err)` on
   `Completion::Throw` to get an owned `String` before the non-`Send`
   `Interpreter`/`Completion` are dropped inside the closure — same reason
   the parser helper can't return the `Program` across the thread boundary).
   This is needed because the *default* test-harness thread stack cannot
   safely run these tests: measured debug frame cost is roughly 128 MiB /
   2,300 ≈ 57 KB per `call_depth` level, so even the small proposed debug
   `HARD_LIMIT` × 2 (400 levels) would want ~23 MB — comfortably inside the
   128 MiB engine stack, but not inside a default 8 MiB thread stack.
   No production code changes in this slice, so there's nothing to go red
   against yet — this slice is infrastructure the next slice's tests need.
2. **Red → green: debug/release call-depth and eval-depth guards fire before
   native overflow.** Using the engine-stack helper, add tests (modeled on
   #606's `deep_nesting_raises_error_before_native_overflow`, "twice the
   limit" pattern) asserting a catchable `RangeError` — not a process abort —
   for, at minimum: plain call recursion, the `Proxy` apply-trap-forwarding
   shape, flat additive expression, and the member-access-chain shape, each
   driven to `2 × ` the relevant constant. These are red today only when run
   in a debug build (`cargo test`, no `--release`) — verify release passes
   *first* with `cargo test --release` before treating a debug failure as
   this issue's bug: only the member-access chain was actually confirmed
   safe in release in §3 (release `eval_depth` capacity ~190k vs. limit
   50,000 is comfortable); the `Proxy` apply-trap shape's release-profile
   native capacity was not measured during planning. If a shape aborts in
   release too, that is a separate, pre-existing release-profile finding —
   not something this issue's debug recalibration should paper over or that
   these tests should be widened to hide. Once release is confirmed clean,
   the debug failures go green after slice 3 lands the `cfg!(debug_assertions)`
   split. If any shape still aborts at the proposed debug numbers, this is
   where it shows up — tighten and re-measure per §4's note before moving on.

   *Two practical notes for running this slice's red phase*: (1) under plain
   `cargo test` (libtest, not nextest), a SIGABRT in one test thread takes
   down the whole test binary — scope the red-phase run to just the new
   tests (e.g. `cargo test deep_recursion_before_fix`, or whatever name
   prefix the new tests share) rather than running the full suite and losing
   everything else's output to the abort. (2) `cargo nextest run --release`
   is the only `cargo test`-family invocation in
   `.github/workflows/ci.yml`; nothing in CI runs `cargo test` on a debug
   build (nextest does isolate per-test processes, so it would tolerate this
   better anyway, but it never runs in debug here). These tests are
   exercised by local `cargo test` only — say so in the PR description,
   don't imply CI enforces this.
3. **Green: make the constants profile-aware.** Land the `cfg!` split for the
   three coupled `CALL_DEPTH_*` constants and `EVAL_DEPTH_LIMIT` per §4,
   rewrite their doc comments, add the compile-time coupling assertion, and
   fix `PROXY_CHAIN_DEPTH_LIMIT`'s stale comment (no value change). Slice 2's
   tests go green. Confirm release behavior is byte-for-byte unchanged by
   re-running the three release-profile binary searches from §3 against the
   post-change release binary and checking the boundaries match (same
   verification #606 did: "re-measured for 13 nesting shapes ... identical in
   every case").
4. **Fix the JS-level regression file for both profiles.** In
   `tests/recursion-limit-interpreter.js`:
   - Lower the leak-check probe (currently
     `eval("1"+"+1".repeat(10000)) !== 10001`, near the end of the file) to a
     depth safely under *both* profiles' `EVAL_DEPTH_LIMIT` (2,000 debug /
     50,000 release) — e.g. 500. The check's purpose is confirming
     `eval_depth` unwinds after repeated trips, not exercising a
     "realistic code depth" the way `recursion-limit-parser.js`'s 1,000-bracket
     check does; the probe depth is otherwise arbitrary, so lowering it loses
     nothing. The "trip the limit repeatedly" loop above it
     (`"+1".repeat(60000)`) already exceeds both profiles' limits and needs no
     change.
   - Add `evalMustRangeError` cases for the member-chain and `Proxy`
     apply-trap-forwarding shapes (self-referential object / forwarding proxy,
     as measured in §3), so the JS-level regression list and the Rust-level
     list from slice 2 stay in step — the same discipline #606 applied
     between `recursion-limit-parser.js` and its Rust test. Size the new
     shapes' repeat counts at roughly `2 ×` the larger of the two profiles'
     relevant limit (i.e. around 100,000, matching the existing additive/logical
     cases' `.repeat(500000)`) rather than reusing 500,000 verbatim — the
     runner's default per-test timeout is 10s
     (`scripts/run-custom-tests.py --timeout`), and a debug build spends more
     wall-clock time per source byte than release. The existing
     `.repeat(500000)` cases were not observed to time out in this session's
     measurements (a 200,000-repetition input aborted in well under 15s), but
     confirm the full file still finishes comfortably inside the timeout on a
     debug binary once the new cases are added, and trim further if not.
   - Manually verify (not a new CI job — `CLAUDE.md` mandates release-only
     test262/custom-test runs for speed, and this file is exercised by that
     runner) that
     `uv run python scripts/run-custom-tests.py --jsse ./target/debug/jsse tests/recursion-limit-interpreter.js`
     now exits 0 against a debug build, closing the exact repro in the issue.
5. **Full regression pass.** `cargo test` (debug, exercises the new debug
   constants and slice-2 tests) and `cargo test --release` (confirms release
   is untouched), plus the existing custom-test and targeted test262 runs
   listed in §7.

## 7. Test surface

- `test262/...`: not applicable to targeted re-runs — test262's deepest
  real-world nesting is 64 brackets (~192 units, per #606's PLAN), nowhere
  near any of these limits in either profile. The full suite is still run per
  standard practice (`uv run python scripts/run-test262.py`, release binary)
  to confirm no incidental regression, but no directory is specifically
  implicated by this change.
- `test262-extra/`: none added. This is a resource-limit/engine-robustness
  concern, not spec-observable behavior test262 (or a spec-clause-driven
  test262-extra case) would ever encode — per `CLAUDE.md`, "engine resource-limit
  or stress checks remain in `tests/`."
- `tests/recursion-limit-interpreter.js`: the primary regression surface,
  extended per slice 4. Run via
  `uv run python scripts/run-custom-tests.py` (release, part of standard
  practice) and manually against `./target/debug/jsse` (slice 4) since
  nothing automated currently points the custom-test runner at a debug
  binary.
- `cargo test` / `cargo test --release`: covers the new engine-stack Rust
  tests from slice 2. `cargo test --release` is what CI actually runs
  (`ci.yml`, `nightly-test262-coverage.yml`); plain `cargo test` (debug) is
  local-only but is the only thing that exercises the new debug constants —
  call this out explicitly in the PR description as a known CI gap, not
  silently rely on it.

## 8. Regression risk

- **Cannot move `test262-pass.txt`.** All release-profile constants are
  unchanged (verified against fresh measurements in §3, re-confirmed in slice
  3), and every automated gate that touches test262 or the custom-test suite
  builds `--release`: `ci.yml` (`cargo build --release`,
  `cargo nextest run --release`), `nightly-test262-coverage.yml`
  (`cargo test --release`), and the mutation-testing oracle
  (`cargo test --release` per `CLAUDE.md`). None of these can observe a
  debug-only constant change. No `--update-baseline` is needed or planned.
- **Fuzz targets checked and confirmed unaffected, not just assumed.**
  `.github/workflows/fuzz.yml`'s `cargo build --release` (line 92) builds the
  standalone jsse CLI binary used as the differential fuzz target's
  subprocess reference engine (`fuzz/fuzz_targets/differential.rs` calls
  `jsse_release_binary()` and spawns it — it never links the interpreter
  in-process). The fuzz targets themselves are built separately via
  `cargo +nightly fuzz build ... --sanitizer none`, which is cargo-fuzz's own
  profile and enables `-C debug-assertions` regardless of optimization level
  — so `parse_roundtrip` (the only in-process target, confirmed by reading
  `fuzz/fuzz_targets/parse_roundtrip.rs`: it calls `jsse::fuzz_parse_bytes`
  only) would compile with this PR's *debug* constants. That target only
  exercises the parser (`MAX_PARSE_DEPTH`, already handled by #606), never
  the interpreter, so it cannot observe `call_depth`/`eval_depth` at all.
  Net: no fuzz target links the interpreter's guarded paths in-process, so
  this change has zero fuzz exposure in either direction.
- **Bytecode fast path shares the same guard, confirmed.** The VM's `Call`
  opcode (`src/interpreter/bytecode/vm.rs:497,499`) dispatches through
  `call_function_ic_validated` / `call_function` — both funnel into the same
  guarded `call_function_inner` (`src/interpreter/eval.rs:5408`) the
  tree-walker uses. There is no separate recursion-depth accounting in
  `bytecode/vm.rs` to keep in sync; this change is automatically consistent
  across both execution paths.
- **GC rooting / `gc_safepoint()`**: untouched. This change is constants, doc
  comments, and tests — no new roots, no new `ObjectKind` variant, no new
  ephemeron.
- **Library harnesses** (`decimal.js`, `acorn`, etc., `scripts/run-library-tests.sh`):
  all run against the release binary; unaffected.
- **Call-shape variance is the main open risk**, not a baseline-move risk:
  the debug `CALL_DEPTH_HARD_LIMIT` candidate (200) is sized against the
  hungriest *measured* shape (`Proxy` apply-trap forwarding, ~1,020 native
  capacity), but shapes not measured during planning (`super()` constructor
  chains, `Array.prototype.forEach`/`map` callback recursion, class field
  initializer chains, generator/async state-machine resumption depth) could
  be hungrier. Slice 2's red/green loop is the safety net — if a test written
  against one of those shapes still aborts on the proposed debug numbers,
  tighten before merging, per §4.

## 9. Out of scope

- Changing `PROXY_CHAIN_DEPTH_LIMIT`'s *value*. Measured 13.5x margin in
  debug; only its comment is stale (§5, §8).
- Any change to `src/parser/mod.rs` or `MAX_PARSE_DEPTH` — that's #606,
  already merged. The forward-reference in its doc comment ("tracked in
  #607") is left as-is; updating it to "fixed in #607" is cosmetic and not
  worth bundling into this PR.
- Adding a debug-profile CI lane to exercise these constants automatically.
  `CLAUDE.md` mandates release-only test262/custom-test runs for speed; this
  plan does not propose an exception. The gap (debug behavior is only checked
  locally) is called out honestly in §7/§8 instead of papered over.
- Investigating or "fixing" the member-chain/flat-expression parser gap
  #606's commit message flagged. Confirmed in §3 that the *interpreter* side
  already throws catchably for these shapes via `EVAL_DEPTH_LIMIT`; nothing
  to do here beyond adding the two new regression shapes in slice 4.
- Any refactor of `call_function_inner`/`eval_expr`'s guard-check structure
  (e.g. extracting the depth check into a shared helper). #606's own
  interpreter analog (`eval_expr`'s doc comment, `src/interpreter/eval.rs:429-437`)
  explicitly warns that splitting a thin wrapper around the depth check adds
  a call frame per operand and roughly triples native stack cost per level —
  not a change to make opportunistically inside a calibration fix.
- Widening the `Proxy` apply-trap / getter / other call-shape measurements
  beyond what slice 2's tests need. Full characterization of every call
  shape's stack cost is a bigger investigation than this issue asks for;
  slice 2 measures only enough to validate the chosen constants don't abort.
