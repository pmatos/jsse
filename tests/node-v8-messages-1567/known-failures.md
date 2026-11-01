# Known failures: V8 messages and early errors under `--node` (#1567, #1568, #1557, #1523)

Differences from Node 26.8 that the fixtures here and
`scripts/node-stack-fixtures-1567/` do not cover. Fixtures compare stdout of
`node` and `jsse --node`.

## Wording

| Case | V8 | jsse | Why it stays |
| --- | --- | --- | --- |
| `for await (x of obj?.m())`, `yield* obj?.m()` | `obj?.m is not a function` / `obj?.m is not iterable (...)` | the value form | V8 prints the callee of an optional call; the optional chain node has no callee text. |
| `for await (x of { [Symbol.asyncIterator]: 1 })` | `obj.ag is not async iterable` | `1 is not a function` | The call of the method throws its own TypeError first. |
| `for await (x of new f())`, `yield* new f()` with a non-constructor | `(intermediate value) is not a constructor` | `f is not a constructor` | The `new` scanner (`chain_after_new`) prints the callee; V8 prints the unnamed value in an iteration head. |
| `var [[c, [d]] = nul] = []`, `({ a: { c } = nul } = {})` | the default's text | the value form | Nested array patterns with nested patterns under a default, and the assignment form of a nested default, skip `bind_pattern`. |

## Early errors

The 19 sources of `scripts/node-stack-fixtures-1523-tail/` that still differed after #1523 (second pass) all agree with
Node now (#1574 for the parser bugs, #1523 third pass for the rest; see
`docs/agents/library-tests-typescript-conformance-residual-1523.md`). What differs on the larger sweep is in
`ts-suite/node-syntax-tail-1523-known-failures.txt`.
