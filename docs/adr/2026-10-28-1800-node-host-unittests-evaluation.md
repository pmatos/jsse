# Node host mode: TypeScript's evaluation unit tests

Status: accepted, epic #1214, issue #1262 (extends `2026-10-28-1600-node-host-unittests-core.md`).

## Context

`src/testRunner/unittests/evaluation/` holds 20 files that register 350 mocha tests on TypeScript v6.0.3 (the
`evaluation` suite of `ts-suite/oracle.json` pins the same 350). Each test hands a snippet to
`evaluator.evaluateTypeScript` (`src/testRunner/evaluator`): the compiler transpiles it with a real `Program`
over `lib.d.ts`, the emitted JavaScript is wrapped and run through `new Function`, and the test asserts on what
the emitted code observably did. They cover the downlevel and native emit of ES decorators (162 tests plus
metadata 12), `using` and `await using` (41 and 44), auto-accessors (12), async generators, `for await`,
generators, `for...of`, array spread, destructuring, object rest, optional calls, `super` in static
initializers, template literals, const enums, external modules and update expressions in modules. So the
engine runs code it did not parse itself, in a function created at run time, against TypeScript's own helpers
(`__esDecorate`, `__addDisposableResource`, `__generator`, ...).

## Result

349 of the 350 tests passed on the first run in light mode with the default bytecode VM: the host and the engine
already ran every emitted helper (`__esDecorate`, `__addDisposableResource`, `__generator`, ...) as on Node. The
one difference was `forOfEvaluation es5 over undefined with Symbol`, which matches V8's wording of the TypeError
for `undefined[Symbol.iterator]` (`undefined is not iterable (cannot read property Symbol(Symbol.iterator))`);
jsse said `Cannot read properties of undefined (reading property)`. The message is implementation-defined, so
`Interpreter::nullish_computed_read_message` (`src/interpreter/eval/access/nullish_message.rs`) now gives the V8 text for the
well-known `Symbol.iterator` key in the bytecode VM and the tree-walker; the key is still not converted
(ToObject precedes ToPropertyKey). Regression: `tests/nullish-iterator-read-message.js`. The other V8 wording
differences stay with #1419. The known-failure list stays empty (all 350 pass).

## Oracle

The 20 groups are rows of `scripts/ts_unittests_groups.py` (`evaluation-*`, batches 8 to 10), run by the same
`scripts/run-ts-unittests-oracle.py` with the same per-test comparison, ratchet and 600 s mocha `-t`.
`check_groups` now also walks `unittests/evaluation/`, so a new or renamed evaluation suite that no group selects
fails the run. The guard `tests/node_ts_unittests.rs` runs all 20 (about two minutes) when `TS_SRC_DIR` is set.

## Alternatives

* A second script for evaluation: the batching, the JSON-report split and the ratchet are the same, so a table
  was added instead of a fork.
