# Known failures: V8 messages under `--node` (#1557, #1558)

Differences from Node 26.8 that the fixtures here do not cover. Fixtures
compare stdout of `node` and `jsse --node`. Everything listed is `--node` only;
default mode keeps the engine's own wording. The `for await`, `yield*`,
parameter-pattern, Intl receiver and RegExp rows (#1567, #1568) are fixed in
`tests/node-v8-messages-1567/`; what remains of them is in
`tests/node-v8-messages-1567/known-failures.md`. The position row is tracked as
#1566.

## Destructuring and iteration (#1557; #1566)

| Case | V8 | jsse |
| --- | --- | --- |
| uncaught destructuring error in a function's parameters, `(async function ({ a }) {})(null)` (sync functions too) | position of the pattern, with a frame for the function | position of the call (sync) or of the function (async); no frame for a sync function |

## Why the position row is open

A destructuring error in a parameter is thrown while the call binds its
arguments, before the callee's frame exists; V8 reports it inside the callee at
the pattern. A `Pattern` carries no source offset and the frame is pushed after
the parameters are bound (`run_activation`), so both need the same offset work
that #1511 documents for expressions.
