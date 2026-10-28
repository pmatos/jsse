# Node host mode: mocha and the TypeScript unit-test runner

Status: accepted, epic #1214, issue #1255 (builds on `2026-10-27-1000-node-host-ts-suite-pin.md` and
`2026-10-27-1100-node-host-hereby.md`).

## Context

`built/local/run.js` is a 13 MB esbuild bundle of the TypeScript test harness. It is not runnable on its own,
on Node either: it registers its tests with mocha's globals (`describe`, `it`, `beforeEach`) and dies with
`ReferenceError: beforeEach is not defined` when started as `node built/local/run.js`. `hereby runtests` starts
it the way the sequential path of `scripts/build/tests.mjs` does:

    node node_modules/mocha/bin/_mocha -R scripts/failed-tests.cjs -O reporter=<r> -g <tests> -t <ms> built/local/run.js

with the selection (`runners`, `light`, `timeout`, ...) in `test.config` in the working directory. So the ticket's
"`jsse --node built/local/run.js --runner=unittests --tests=<one>`" is, on both engines, mocha's CLI with
`jsse --node` in front of `_mocha` and a `test.config` of `{"test": ["unittests:: semver"], "runners": ["unittest"]}`.
The parallel host (`run.js` forking itself over IPC) is #1258.

## What was missing

With the building blocks already in place (events, util, assert, tty, event loop, fs, child_process, hereby),
mocha 10.8.2 starts and runs the 692 `semver` unit tests on jsse with the output Node gives. One engine
difference showed in the traces, and is fixed here:

1. **Strict tail calls.** The bytecode VM (`Op::ReturnCall`) and the tree-walker (`Statement::Return`) turn a
   strict `return f(x)` into a `Completion::TailCall` that replaces the caller's frame (ES2015 proper tail
   calls). V8 never shipped them, so under `--node` the failure of `it("x", () => assert.strictEqual(1, 2))`
   lost the test's own frame (the arrow *is* in tail position), and mocha printed a trace with no line of the
   test, and a frame of mocha's own `done()` was missing from the `ERR_MOCHA_MULTIPLE_DONE` report. Both gates now
   read `Interpreter::node_host_enabled`: under `--node` a strict call in tail position is an ordinary call,
   like Node (and `Maximum call stack size exceeded` at the same kind of depth); the default environment
   keeps ES2015 tail calls, and nothing else changes there. `tests/node_ts_mocha.rs` pins both.

## Verification

`TS_SRC_DIR=<built TypeScript v6.0.3 checkout> uv run python scripts/run-ts-mocha-oracle.py [--jobs N] [glob ...]`
runs 51 cases on Node and on `jsse --node` and diffs stdout, stderr, the exit status and the files left behind
(`.failed-tests`, `TEST-results.xml`):

- the mocha CLI on `scripts/ts-mocha-fixtures` with all eleven reporters (`spec`, `dot`, `tap`, `min`, `list`,
  `nyan`, `json`, `xunit`, `markdown`, `landing`, `progress`), passing and failing: assertion diffs, thrown
  strings/objects/errors with `cause`, rejected promises, `done(err)`, failing `before`/`afterEach` hooks;
- timeouts, `done()` twice, `done` plus a returned promise, errors thrown from `nextTick`, timers and
  immediates (mocha's `uncaughtException` handler), unhandled rejections, an error landing after its test ended
  (the `ERR_MOCHA_MULTIPLE_DONE` report on stderr), `process.on("exit")`/`beforeExit` hooks, `--exit`,
  `--bail`, `-g`/`-i`/`-f`, no spec, a missing spec, an unknown reporter, `slow()`;
- the TypeScript runner: `built/local/run.js` through `scripts/failed-tests.cjs` with `dot`, `spec`, `min` and
  `mocha-fivemat-progress-reporter` on the `unittests:: semver` suite (692 passing), with no match, with
  `CI=true` writing `TEST-results.xml`, with `light`/`timeout`/`stackTraceLimit`/`skipSysTests` in `test.config`,
  with the unit tests switched off, and next to a failing fixture file (exit status 14, the failing traces
  printed, `.failed-tests` written).

Each run gets a working directory of symlinks into the checkout, so runs never write into it. Exit statuses are
the failure counts (0, 2, 7, 10, 14, 16) and agree. Without `TS_SRC_DIR` (or with another TypeScript version, or
an unbuilt checkout) the script prints `SKIP:` and exits 0; `--require` makes that an error. Guards that always
run: `tests/node_ts_mocha.rs` (tail calls; it also drives the oracle when `TS_SRC_DIR` is set) and
`scripts/test_run_ts_mocha_oracle.py` (CI `unittest discover`).

### What the oracle normalises

Paths, durations, pids, the `(Nms)` mark the spec reporter adds to slow passing tests, and four things about
stack frames that are cosmetic and not fixed here (filed as #1379): frames inside Node's own modules and
`at async` frames (#1368); V8's receiver name of an arrow function (`at Context.<anonymous> (f.js:1:2)`,
`at Timeout._onTimeout (...)`) which jsse does not record for arrows, so the line reads `at f.js:1:2`; V8's
inferred name `exports.f` for a function assigned to a member; and, in the `json` reporter only, own-key order
(`stack` before `message` in V8). The json case is compared as parsed JSON with sorted keys.

## Cost

On a quiet host the 692 tests take 0.4 s on Node. On jsse the 13 MB bundle takes tens of seconds to parse and
register its 327 suites before any test runs (about 50 s user time in the release build with LTO off, on a host
at load 60), then 16 s for the tests themselves. Startup of the runner is not part of this ticket's criteria;
the parallel host (#1258) and the full-suite tickets will want it down.

## Reporters, timeouts, grep and failure output (issue #1256)

The oracle grew from 51 to 109 cases (the table moved to `scripts/ts_mocha_cases.py`, new fixtures under
`scripts/ts-mocha-fixtures`). New coverage, all identical to Node after normalisation:

- every non-browser reporter (the `doc` and `json-stream` reporters join the eleven), `--reporter-option output=`
  for `xunit` and `json`, the TypeScript `failed-tests.cjs` wrapper around `min` and
  `mocha-fivemat-progress-reporter` on a failing suite;
- timing under the jsse timer queue: `timers.spec.js` checks inside the test that timers never fire early, fire in
  deadline then creation order, that `nextTick`, promise jobs, `queueMicrotask`, immediates and timers keep Node's
  phase order, that `setInterval`, `clearTimeout`, `refresh`, `ref`/`unref`, `timers/promises` and `AbortSignal`
  behave, and that mocha's `test.duration` brackets the sleep; `timeouts.spec.js` and `cli-timeout.spec.js` pin
  the messages and exit status of test and hook timeouts, `this.timeout(0)`, limits above 2^31-1, limits changed
  mid-test, a synchronous overrun, `done` after the timeout, per-suite and per-retry limits, `-t 60|0|1s` and the
  `speed` class from `--slow`;
- failure text: assertion diffs for objects, maps, sets, typed arrays, circular values, long lists (default,
  `--no-diff`, `--inline-diffs`, `--colors`), `--retries`, `--full-trace`, `--forbid-only`, `--forbid-pending`,
  `--check-leaks`, `--dry-run`, `--async-only`, `--delay`, `-u tdd`, `--sort`, regexp/fixed-string/inverted `-g`/`-f`.

Two engine differences surfaced and are fixed:

1. **`process.exit()` inside a builtin callback.** `mocha --list-reporters` (and `--help`/`--version` paths of other
   yargs CLIs) call `process.exit()` from a `forEach` callback. `Array.prototype.forEach` kept only
   `Completion::Throw` from the callback, so the `Completion::Exit` (#242) was dropped and the run went on: the
   second callback ran, the code after the loop ran, and mocha printed its usage error and exited 1. A sweep of
   about 120 callback sites (`sort` comparators, `JSON.stringify` replacers, Proxy traps, setters,
   `Symbol.toPrimitive`, iterator helpers, `Object.groupBy`, static blocks, class fields, ...) showed some sixty
   with the same leak. Instead of patching each, `__host_exit` now calls `Interpreter::latch_exit`, which
   latches `pending_exit` and sets the same sticky poll bit the step budget (#1173) uses: the next statement
   boundary or loop back-edge returns `Completion::Exit`, and `budget_tick` (microtask jobs, timer callbacks) does
   too. Nothing changes without `--node`, and the unset-budget fast path is untouched.
2. **`process.nextTick` callbacks had a receiver.** The tick loop called `tock.callback()`, so a failing tick shows
   `at Object.callback (f.js:1:2)`; Node calls a detached function (`at f.js:1:2`). The shim now detaches it.

The oracle's normaliser also learned that a trailing ` {` (an error's property list after the last stack frame)
belongs to the frame line whether the frame is dropped or kept, and that the `json`/`json-stream` reporters and
`--reporter-option output=report.json` are compared as parsed JSON with timings blanked even when a test printed
text before the report.

## Runner flags, sharding and `.failed-tests` (issue #1257)

`hereby runtests` is `runConsoleTests` in `scripts/build/tests.mjs` after its build tasks: minimist parses
`--runner/--ru`, `--tests/-t`, `--shards`, `--shardId`, `--light` (on by default, `--no-light`), `--failed`,
`--keepFailed`, `--colors/--no-color`, `--timeout`, `--skipSysTests`, `--stackTraceLimit`; it writes `test.config`
and starts `_mocha -R scripts/failed-tests.cjs -O reporter=<r>[,keepFailed=true] [-g <tests>] [--config
$TMPDIR/.failed-tests.json] -t <ms> built/local/run.js`; `--failed` turns `.failed-tests` into an escaped
`grep` alternation, and the reporter rewrites `.failed-tests` after the run. No engine change was needed: the
oracle's `cli-*` cases (`scripts/ts_runner_cli_cases.py`, fixtures `runtests.mjs`, `rerun.spec.js`,
`timeout.spec.js`) call that function in a work directory, on Node and on `jsse --node`, and diff stdout,
stderr, exit status, `.failed-tests` and the baselines the run wrote.

- Test lists: a `.mocharc.json` with `dry-run` makes mocha print the title of every test the selected runners
  register, so a runner (`conformance`, `compiler`, `project`, `fourslash`, `fourslash-server`,
  `fourslash-generated`, `transpile`, `unittest`), several runners, a `--tests` filter (also one matching
  nothing) and a shard give the same list as Node, line for line (90k titles for `compiler`, 106k for `unittest`).
  Shards are `idx % shards == shardId - 1` over the file list; the oracle also checks that the three shards of
  conformance, project and fourslash together are exactly the whole list.
- Real runs: `--shards` with a large count picks a few files per runner (`cli-run-*`), with `--light` and
  `--no-light` (light skips `shouldAssertInvariants`); real baselines are compared with `tests/baselines/reference`
  and the baselines written to `tests/baselines/local` are diffed between engines.
- Failed tests: a fixture spec that fails until `RERUN_FIXED` is set (test names with regexp metacharacters, a
  failing `before` hook that is recorded as its suite) shows that `--failed` re-runs exactly the recorded tests
  and that `.failed-tests` is deleted when they pass, kept by `--keepFailed`, and that `--failed` without the file
  is `ENOENT` on both. `--timeout=60` against a 250 ms test fails with mocha's timeout error.
- `hereby baseline-accept` copies seeded local baselines (nested, changed, `.delete` markers) into a private
  reference tree, never the checkout's.
- Not covered here: the parallel host (`--workers`, #1258) and `hereby runtests-watch`. The `--inspect` flags and
  coverage need a debugger or `c8`.
