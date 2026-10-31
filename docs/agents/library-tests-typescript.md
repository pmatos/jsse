# Library Tests: TypeScript on jsse (index)

Epic #1214 runs TypeScript 6.0.3's `tsc` and its own test suite on `jsse --node` and checks every result against Node. This file is the index; the detail lives in the pages below. It was a 19 KB list of oracles and hit the 20,000-byte budget of authored files, so the per-oracle text moved to [library-tests-typescript-oracles.md](library-tests-typescript-oracles.md). A new page goes in a new file and gets one line here.

**Start here:** [Node host architecture](node-host.md), [running tsc and the suite locally](typescript-suite-local.md), [triage rules](typescript-suite-triage.md), [adding a core module](node-host-add-module.md). The parent harness page is [library-tests.md](library-tests.md).

## Gate, baseline and CI

| page | what |
| --- | --- |
| [pass list, ratchet, regression gate (#1296)](library-tests-typescript-pass-list-1296.md) | `scripts/run-ts-pass-list.py`, `ts-suite/jsse-pass-1296.json`, the gain-only ratchet |
| [nightly suite (#1298)](library-tests-typescript-nightly-1298.md) | the sharded nightly job and its smoke |
| [wall time and memory budget (#1297)](library-tests-typescript-budget.md) | `scripts/run-ts-budget-oracle.py`, 6x Node RSS cap |
| [process groups (#1425)](library-tests-typescript-conformance-process-groups.md) | `scripts/oracle_procs.py`: no oracle leaves a process behind |
| `ts-suite/README.md` | the pin (`pin.json`) and the Node oracle (`oracle.json`) |

## Per-oracle summaries

[library-tests-typescript-oracles.md](library-tests-typescript-oracles.md): the `tsc` oracles (startup, watch, projects, diagnostics, informational flags, emit matrix, self-compile), hereby and `hereby local`, mocha and the runner CLI, esbuild, WebAssembly, the unit-test oracles and the runner oracles in one list with commands and guards.

## tsc and hereby

| page | what |
| --- | --- |
| [tsc informational flags, second set (#1247)](library-tests-typescript-tsc-info.md) | `--listFiles`, `--showConfig`, `--traceResolution`, `--diagnostics`, ... |
| [hereby stacks (#1251)](library-tests-typescript-hereby-1251.md) | error stacks printed by hereby and its tasks |
| [`hereby tests` (#1254)](library-tests-typescript-conformance-hereby-tests.md) | the TypeScript test runner built on jsse |
| [`hereby lint` (#1295)](library-tests-typescript-conformance-lint.md) | ESLint with typescript-eslint |

## Test runners

| page | what |
| --- | --- |
| [unit tests](library-tests-typescript-unittests.md) | core, services, evaluation, tsbuild, config/sys, `tsc`, watch, tsserver |
| [`compiler` runner (#1268-#1273)](library-tests-typescript-compiler.md) | 43,656 tests, slices, control, ratchet |
| [`fourslash` runner (#1288)](library-tests-typescript-fourslash.md) | language-service tests and server tests |
| [`project` runner (#1286)](library-tests-typescript-project.md) | commonjs and amd project compiles |
| [`transpile` runner (#1287)](library-tests-typescript-transpile.md) | `transpileModule` and `transpileDeclaration` |

## `conformance` runner, by directory group

[types (#1274)](library-tests-typescript-conformance-types.md), [expressions (#1275)](library-tests-typescript-conformance-expressions.md), [statements, controlFlow, functions](library-tests-typescript-conformance-statements-controlflow-functions.md), [es6 (#1277)](library-tests-typescript-conformance-es6.md), [es5, es7, es2016-esnext (#1278)](library-tests-typescript-conformance-es.md), [modules](library-tests-typescript-conformance-modules.md), [parser, scanner, directives, pedantic, references (#1280)](library-tests-typescript-conformance-parser-scanner.md), [declarationEmit, emitter, ambient, additionalChecks (#1281)](library-tests-typescript-conformance-declaration-emit.md), [jsx, jsdoc, salsa, typings (#1282)](library-tests-typescript-conformance-jsx-jsdoc-salsa-typings.md), [classes, decorators, esDecorators, override](library-tests-typescript-conformance-classes-decorators.md), [async, asyncGenerators, generators, enums, constEnums, interfaces, Symbols (#1284)](library-tests-typescript-conformance-async-enums-interfaces.md).

## Engine and host fixes found while running the suite

| page | what |
| --- | --- |
| [stack frames (#1333, #1358, #1368, #1379)](library-tests-typescript-conformance-stack-frames-1368.md) | V8-shaped `error.stack`, async frames |
| [V8 messages and error positions (#1419)](library-tests-typescript-conformance-v8-messages-1419.md) | `o.f is not a function`, caret columns |
| [deep equality over Proxies (#1334)](library-tests-typescript-conformance-deepequal-proxy-1334.md) | `assert.deepStrictEqual` and Proxy traps |
| [async `fs` follow-ups (#1341)](library-tests-typescript-conformance-fs-async-1341.md) | non-blocking `fs` |

The source of truth for the pinned numbers is `ts-suite/oracle.json`; known failures are in `ts-suite/*known-failures*` (formats and rules: [triage](typescript-suite-triage.md)).
