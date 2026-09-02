# Plan: issue #332 — Node-compat tape adapter: await callback-style t.end()/plan completion

## 1. Problem restated

The shared Node-compat test-runner harness (`scripts/node-test-harness.js`) installs an
in-process `tape` adapter (`installTape`) used by tape-based library bundles on jsse. Its
`runTest` only awaits a test callback when that callback *returns* a thenable; `t.end()` is a
no-op unless handed an error, and there is no tracking of `t.plan(n)` fulfillment either. A
tape test that defers its assertions and completion signal past the callback's synchronous
return — the standard callback-style pattern `t.plan(1); setTimeout(function () { t.equal(1,
1); t.end(); }, 10);` — has its plan-mismatch check, subtree teardown, and per-test bookkeeping
run immediately after the callback returns, before the deferred assertion and `t.end()` ever
fire. This was deliberately deferred out of PR #322, whose `qs` corpus is fully synchronous
(every test signals completion inside the callback's synchronous extent) and therefore never
exercises the gap. The fix: track each tape test's completion (explicit `t.end()` call, or
`t.plan(n)` fulfillment) and `await` it, bounded by a timeout, before checking the plan,
running teardowns, and moving on to sibling/child tests — mirroring the QUnit adapter's
existing `assert.async()` completion tracking in the same file (`runTest`, ~`src` lines
893–968: `testObj.pending`, `testObj._asyncResolve`, `schedule`/`unschedule`,
`ASYNC_TIMEOUT_MS`).

## 2. Spec basis

N/A: no JavaScript behavior change. This is a change to `scripts/node-test-harness.js`, an
in-process Node-compat test-runner shim used only under jsse's `--node` host mode to run
third-party npm test suites; it does not alter any ECMAScript syntax or semantics the engine
implements. It is inert on real Node (the suite's own `tape` runs there instead), so there is
no oracle divergence risk either — this is tooling parity with real tape's own completion
semantics (a tape test is "done" only once `end()` fires or its `plan` count is reached; tape's
own runner blocks on this in the same way).

## 3. Files to touch

- `scripts/node-test-harness.js` — `installTape()`:
  - `testArgs()` (~line 1441): add `ended: false` and `_completeResolve: null` fields to the
    per-test object.
  - `makeAssert()`'s inner `assert()` function (~line 1530) and `t.skip` (~line 1562): after
    incrementing `test.assertions`, check plan fulfillment and resolve completion.
  - `t.plan` (~line 1540): also run the same plan-fulfillment check after setting `test.plan`,
    so an assert-before-plan ordering (`t.ok(1); t.plan(1);`) completes instead of waiting.
  - `t.end` (~line 1543): mark the test as ended and resolve the completion promise (in
    addition to the existing `if (error) t.error(error)` behavior).
  - `runTest()` (~line 1702): the callback invocation has three distinct completion signals to
    fold in *before* deciding whether to wait — real tape auto-ends a test once a returned
    promise settles (resolves or rejects) or once the callback throws synchronously, in addition
    to an explicit `t.end()`/plan fulfillment during the synchronous extent. Concretely: if the
    callback returned a thenable (`await result` ran) or the `catch` block ran (sync throw or
    rejected thenable, both already funneled into the existing `t.fail(...)`), set
    `test.ended = true` right there — no promise/resolve needed since the coroutine is already
    past that point. Only *after* that does the `if (!test.ended)` fast-path check apply: skip
    the wait for anything already complete (explicit `end()`/plan reached synchronously, or one
    of the three auto-end cases above); otherwise await completion, bounded by the existing
    `ASYNC_TIMEOUT_MS` via `schedule`/`unschedule` (same guarded-resolve pattern as QUnit's
    `assert.async()` timeout — null the resolver before invoking it, always `unschedule` after
    the wait settles either way), recording a failure and moving on if it elapses. This runs
    before the `test.children` loop and the `test.plan !== test.assertions` check.
- `scripts/harness-fixtures/` — new fixture(s) exercising deferred completion (see TDD slices).
- `scripts/README.md` (~lines 188–191, 222–226) — extend the existing note that QUnit's
  `assert.async()` and TAP `function (done) {...}` hooks are bounded by a 10 s timeout to also
  cover tape's callback-style `t.end()`/`t.plan()` completion, and note the adapter now tracks
  completion instead of treating `end()` as a no-op.

No `src/` changes, no `docs/adr/` entry (not an architectural decision, just closing a known
adapter gap), no `CONTEXT.md` change (no new vocabulary).

## 4. TDD slices

All slices are within `scripts/node-test-harness.js` + `scripts/harness-fixtures/*.fixture.js`,
run via `./scripts/run-harness-selftest.sh` (jsse-only; the harness is inert on Node, matching
the existing `tape.fixture.js` precedent).

0. **Pre-check (derisk, no code change):** `t.test()` subtests are tracked in `test.children`
   but `test.assertions`/the plan-fulfillment check only count the *parent's own* direct
   assertions — real tape counts a registered subtest itself as one unit against the parent's
   plan. This plan does not change that (see §7). Before implementing, grep the cached `qs`
   corpus (`/tmp/jsse-libtests/qs/`, re-cloned via `./scripts/run-library-tests.sh qs --node` if
   not present) for any test that calls both `t.plan(` and `t.test(` — if none exist (expected,
   since qs is fully synchronous and 1,013/1,013 green today), the new per-test wait never
   triggers on qs's corpus and slice 4 below is a clean regression guard. If any do exist, note
   the concrete case in the PR description rather than changing the counting semantics.
1. **Red:** Add `scripts/harness-fixtures/tape-async-end.fixture.js` with a test using the
   issue's exact motivating pattern — `t.plan(1); setTimeout(function () { t.equal(1, 1);
   t.end(); }, 10);` — followed by a second, purely synchronous sibling test that also asserts
   and ends. Declare the correct `// Expected summary: PASS: 2  FAIL: 0  TOTAL: 2` marker. On the
   current adapter this fails: TAP numbering is global across the run (`counter` in `report()`),
   so tracing it out — test 1's callback returns synchronously without yielding to the timer
   macrotask, so its plan check fires immediately (`not ok 1 plan != count`, 0 assertions vs.
   plan 1); test 2 then runs synchronously and passes (`ok 2`); `runAll` finishes and prints the
   final summary `PASS: 1  FAIL: 1  TOTAL: 2` — and only *after* that does the 10 ms timer fire,
   printing a trailing `ok 3` for the deferred assertion after the summary line already printed.
   The fixture is red against the expected `PASS: 2  FAIL: 0  TOTAL: 2` either way, but an
   implementer diffing actual output should expect `PASS: 1  FAIL: 1  TOTAL: 2` (plus a
   late-printed `ok 3`), not a collapsed `PASS: 0  FAIL: 1  TOTAL: 1`.
2. **Green:** Implement the completion tracking described in §3 (`ended`/`_completeResolve` on
   the test object, plan-fulfillment check in `assert()`/`t.skip`, resolve-on-`end()`, and the
   bounded await in `runTest` with a fast path when already `ended`). Re-run the fixture; it
   must now report `PASS: 2  FAIL: 0  TOTAL: 2` with the deferred assertion's `ok`/`not ok` line
   printed in TAP-numbering order before the run's final `# tests`/`# pass`/summary lines.
3. **Red → Green, no-plan deferred `t.end()`:** Extend the same fixture (or add a second one,
   `tape-async-end-noplan.fixture.js`) with a test that defers only via `t.end()` with no
   `t.plan()` at all — `setTimeout(function () { t.ok(true); t.end(); }, 0);` — to prove
   completion tracking doesn't require a plan. Update the expected summary accordingly. This is
   red before slice 2's implementation (same root cause) and green after. Add two more cases to
   the same fixture that must complete *without* any wait (auto-end, no explicit `end`/`plan`):
   a promise-returning test with no `end`/`plan` call
   (`tape('promise-autoend', async function (t) { t.ok(true); });`) and a synchronously-throwing
   test with no `end`/`plan` call. Both must already pass before this issue's fix (they hit the
   pre-existing `await result`/`catch` paths in `runTest`) — the regression to guard against is
   the naive `if (!test.ended) wait` reading of this plan, which would newly stall both for the
   full `ASYNC_TIMEOUT_MS` because neither ever calls `end()`. Assert the fixture run stays
   sub-second wall-clock (e.g. via `time` in a comment-documented manual check, or by the fixture
   simply being included in `run-harness-selftest.sh`'s normal fast run) to catch a regression
   here.
4. **Regression check, nested nested/teardown fixture unaffected:** Re-run the existing
   `scripts/harness-fixtures/tape.fixture.js` (`PASS: 17 FAIL: 0 TOTAL: 17`) unchanged. Every
   test in it signals completion synchronously (`t.end()`/`st.plan()+st.end()` inside the
   callback body), so the new completion-wait must take the fast path (`ended` already true) and
   produce byte-identical output — this is the regression guard that the fast path in slice 2
   doesn't add a spurious tick/timer for already-synchronous tests.
5. **Timeout path — reasoned, not wall-clock-exercised:** Do not add a fixture that lets a test
   genuinely hang for the full `ASYNC_TIMEOUT_MS` (10 s) — `tap-suite-hook-failure.fixture.js`
   already establishes this precedent ("A hook timeout rejects through the identical path, so it
   is not re-exercised here (it would cost `ASYNC_TIMEOUT_MS` of wall-clock)"). Instead, the
   timeout branch reuses the exact `schedule`/`unschedule` + guarded-resolve pattern already
   proven correct by QUnit's `assert.async()` timeout at ~line 924–943 (same file, same
   constant) — implement it identically (guard nulls `_completeResolve` before firing so a late
   `t.end()` no-ops, and the guard is always `unschedule`d after the wait settles either way) and
   note the precedent in the code comment instead of re-testing the sleep.

## 5. Test surface

- No `test262/` directories apply — no JavaScript-observable behavior changed.
- No `test262-extra/` addition — same reason.
- Gate: `./scripts/run-harness-selftest.sh` (new fixtures from TDD slices 1–4, plus the full
  existing fixture suite as a regression guard — every fixture under
  `scripts/harness-fixtures/*.fixture.js` must stay green, in particular `tape.fixture.js` and
  `qunit.fixture.js`, since both a QUnit and the TAP describe/it adapter live in the same file
  and must not be touched by this change).
- Gate: `./scripts/run-library-tests.sh qs` — the one currently-wired tape consumer. Must
  remain at the exact cross-checked `1,013/1,013` against Node's real tape, with no wall-clock
  regression (qs is fully synchronous, so every one of its tests must take the new fast path).
- Gate: `./scripts/lint.sh` — `node-test-harness.js` is shared, non-engine JS under `scripts/`.

## 6. Regression risk

- **Scope containment:** the change is entirely inside `installTape()`'s closure in
  `scripts/node-test-harness.js`. It does not touch `installQUnit()` or the describe/it TAP
  runner (`installTap()`) in the same file, nor `src/` — no tree-walker, property-MOP, GC, or
  `ObjectKind` surface is involved, so `test262-pass.txt` cannot move.
- **Fast-path correctness is the main risk:** if the `!test.ended` check after the synchronous
  callback phase is wrong (e.g., checked before `assert()`'s plan-fulfillment update actually
  runs, or before an awaited returned thenable's own synchronous assertions land), an
  already-complete synchronous test would incorrectly pay the `schedule`/timeout wait, silently
  bloating `qs`'s ~1,013-test wall-clock. TDD slice 4 is the guard for this.
- **Timeout-guard reentrancy:** the QUnit adapter's identical pattern already handles the
  "guard fires after completion" and "completion fires after guard" races correctly (nulling
  the resolver before invoking it, and always `unschedule`ing after the await settles); the tape
  implementation must copy that exact ordering, not a reordered variant, to avoid a double-
  resolve or a dangling native timer thread (jsse spawns one OS thread per un-cancelled
  `setTimeout`, per the file's own comment at ~line 58–66) that would delay process exit.
- **No interaction with the QUnit global watchdog** (`GLOBAL_WATCHDOG_MS`, tape has no
  equivalent): out of scope for this issue, which asks only for a per-test bound, not a
  whole-run one. Not adding it is a deliberate scope limit, not an oversight.
- **Subtest/plan interaction (see §4 slice 0 and §7):** real tape counts a registered `t.test()`
  subtest as one unit toward the parent's plan; this adapter's plan-fulfillment check only
  counts the parent's own direct assertions. A test shaped like `t.plan(2); t.ok(1);
  t.test('sub', fn);` would, under this change, now pay the full `ASYNC_TIMEOUT_MS` wait before
  reporting its (pre-existing) "plan != count" mismatch, instead of failing immediately as it
  does today. The pre-check in §4 slice 0 confirms `qs` does not hit this shape; if some future
  consumer does, that consumer's tests would get slower on failure but not incorrect.

## 7. Out of scope

- **Counting a `t.test()` subtest as one unit toward the parent's plan** (real tape does this;
  see §6). This adapter keeps its pre-existing simplification of counting only the parent's own
  direct assertions. Flag as a known gap in the PR description; not building it out because
  nothing currently wired hits it (§4 slice 0 confirms this for `qs`) and it would enlarge the
  diff for a case with no live consumer.
- A tape-level *global* run watchdog analogous to QUnit's `GLOBAL_WATCHDOG_MS` — the issue asks
  for a per-test timeout on `end`/`plan` completion, not whole-suite abort semantics.
- "Assert after end" detection (real tape errors if an assertion fires after `t.end()` or after
  the plan is already fulfilled) — the current change only tracks *first* completion; assertions
  arriving after that point still increment counters as before. Not exercised by any currently-
  wired consumer; flag as a known gap in the PR description rather than building detection for a
  scenario nothing currently hits.
- `t.timeoutAfter(ms)` currently a chainable no-op (~line 1550) — wiring it to override the
  per-test `ASYNC_TIMEOUT_MS` is a natural follow-up once a real consumer needs a non-default
  timeout, but no currently-wired corpus calls it with a meaningful value, so it stays a no-op.
- Refactoring the QUnit/TAP adapters' own (already-correct) completion tracking to share a
  helper with the new tape logic — the three adapters currently duplicate the
  schedule/unschedule/guard pattern; unifying them is a legitimate later cleanup but would
  enlarge this bug-fix diff for no behavioral gain right now.
- Any `test262-pass.txt` baseline update — not applicable (no engine change) and a `main`-only
  operation regardless.
