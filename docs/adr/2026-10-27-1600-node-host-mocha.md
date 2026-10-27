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
