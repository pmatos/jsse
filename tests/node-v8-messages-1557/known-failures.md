# Known failures: V8 messages under `--node` (#1557, #1558)

Differences from Node 26.8 that the fixtures here do not cover. Fixtures
compare stdout of `node` and `jsse --node`. Everything listed is `--node` only;
default mode keeps the engine's own wording. The rows are tracked as sub-issues
of #1418.

## Destructuring and iteration (#1557)

| Case | V8 | jsse |
| --- | --- | --- |
| uncaught destructuring error in a function's parameters, `(async function ({ a }) {})(null)` (sync functions too) | position of the pattern, with a frame for the function | position of the call (sync) or of the function (async); no frame for a sync function |
| `for await (x of o.m())`, `for await (x of o)` | `o.m(...) is not a function or its return value is not async iterable`, `o is not async iterable` | `o.m is not a function`, the sync wording |
| `yield* o.m()` | `yield* (intermediate value) is not iterable` | `o.m is not a function` |
| `(function ({ a: { c } = {} }, [b]) {})({})` | `Cannot destructure property 'Symbol(Symbol.iterator)' of '{}' as it is undefined.` (the earlier parameter's property default names its own text) | `undefined is not iterable (cannot read property Symbol(Symbol.iterator))` |
| `(function ({ a }, [b] = f()) {})({})` with `f` returning `null` | `(intermediate value) is not iterable (cannot read property Symbol(Symbol.iterator))` | `object null is not iterable (cannot read property Symbol(Symbol.iterator))` |
| `var [x] = t\`a\`` where `t` returns `1` | `number 1 is not iterable (cannot read property Symbol(Symbol.iterator))` | `t is not a function or its return value is not iterable` |
| a string key with an escape in a callee, `o['\\']()` | `o.\ is not a function` | `undefined is not a function` |

## Builtins (#1558)

| Case | V8 | jsse |
| --- | --- | --- |
| the `format` getter of `Intl.NumberFormat` and `Intl.DateTimeFormat` on a wrong receiver | `Method UnwrapNumberFormat called on incompatible receiver undefined` | `Intl.NumberFormat.prototype method called on incompatible receiver` |
| the same for `Intl.ListFormat`, `Intl.DisplayNames`, `Intl.Segmenter`, the other `Intl.NumberFormat` methods | `Method Intl.X.prototype.m called on incompatible receiver #<Object>` | the engine's text |
| `new RegExp('(?<=a)+', 'u')` | `Invalid quantifier` | `Nothing to repeat` |

## Why the position rows are open

A destructuring error in a parameter is thrown while the call binds its
arguments, before the callee's frame exists; V8 reports it inside the callee at
the pattern. A `Pattern` carries no source offset and the frame is pushed after
the parameters are bound (`run_activation`), so both need the same offset work
that #1511 documents for expressions.
