# Node host mode: `hereby run-eslint-rules-tests` and `worker_threads`

Status: accepted, epic #1214, issue #1294 (builds on `2026-10-27-1600-node-host-mocha.md` and
`2026-10-27-1100-node-host-hereby.md`).

## Context

`hereby run-eslint-rules-tests` is `runConsoleTests("scripts/eslint/tests", "mocha-fivemat-progress-reporter",
false)`: mocha loads the `*.cjs` files of `scripts/eslint/tests`, each of which runs `@typescript-eslint/rule-tester`
(over ESLint 10 and the TypeScript parser) on one of TypeScript's custom rules in `scripts/eslint/rules`. On Node
it runs 6 files, 79 tests, in 0.8 s.

## What was missing

1. **`node:worker_threads`.** `eslint/lib/eslint/eslint.js` destructures `SHARE_ENV` and `Worker` at load time and
   `eslint-helpers.js` reads `isMainThread` and `threadId`, so `require("eslint")` died with
   `ERR_UNKNOWN_BUILTIN_MODULE` before the first test. jsse has one thread, so `node-lib/worker_threads.js` is the
   module as a main thread without workers sees it: `isMainThread`, `threadId` 0, `threadName` `"main"`,
   `parentPort` and `workerData` null, `resourceLimits` `{}`, the `SHARE_ENV` symbol, a working environment-data
   store and untransferable marks, and a `Worker` whose constructor throws `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM`
   (ESLint only starts workers for `--concurrency`). `MessageChannel`, `MessagePort`, `BroadcastChannel`,
   `receiveMessageOnPort` and `postMessageToThread` are absent: they need a message-passing layer that nothing in
   the epic uses. Out of `--node` mode the module does not exist.
   A package that feature-detects the module and then starts a `Worker` breaks instead of falling back, which
   is what esbuild's sync API did (`new MessageChannel` is not a constructor), so the CommonJS loader refuses
   `require('worker_threads')` from `node_modules/esbuild/lib/main.js` as before (`ESBUILD_WORKER_THREADS=0`
   does the same by esbuild's own switch). Other readers of `isMainThread` (source-map-support, workerpool)
   are unaffected.
2. **Stack frames of private and symbol-keyed methods.** A failing rule test prints RuleTester's frames
   (`this.#testInvalidTemplate(...)`, `this[finalizeConfig](...)`). V8 prints `at #m` and `at [sym]` where jsse
   printed `at R.#m` and `at R.[sym]`: V8 leaves out the receiver's type for these names. And for a call through a
   private name (`this.#m(`) V8 positions the frame at the `(`, where jsse used the `#`; for `a.b(` it is the name
   (unchanged). Both are in `--node` stack formatting only (`tests/node_v8_error.rs`).

## Verification

The `eslint-*` cases of `scripts/run-ts-mocha-oracle.py` (table in `scripts/ts_eslint_cases.py`, fixtures in
`scripts/ts-eslint-fixtures`) run the real hereby CLI with the checkout's Herebyfile in a work directory whose
`scripts/eslint/tests` links the checkout's files and adds the fixtures, so nothing is written into the checkout.
Stdout, stderr, exit status and `.failed-tests` are diffed between Node and `jsse --node` for: the six real test
files with the default fivemat reporter (colour and `--no-color`), `spec`, `tap`, `dot` and `json`; `--tests`
filters (one rule, an alternation, no match); `extra-rules.test.cjs` (fixers and `output`, `filename`, options, error
locations, TypeScript-only syntax: 31 more tests, all passing on Node); and `broken.test.cjs`, which fails in every
way RuleTester reports (wrong message id, wrong count, wrong location, a valid case that reports, an invalid case
that does not, a wrong fix output, a rule that throws, a schema violation, a parse error) with the `--failed` re-run
before and after `ESLINT_FIXED=1` repairs it. The normaliser also drops builtin `(<anonymous>)` frames and the
timing-derived `speed` of the json reporter, and the ` (Nms)` mark that a slow multi-line title carries.

`tests/node_worker_threads.rs` pins the module's shape; the oracle is driven by `tests/node_ts_mocha.rs` when
`TS_SRC_DIR` is set.

## Cost

79 tests take 4 s on jsse (0.8 s on Node), most of it loading ESLint and the TypeScript parser.
