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

## Early errors (`scripts/node-stack-fixtures-1523-tail/` rows)

| Source | V8 | jsse | Why it stays |
| --- | --- | --- | --- |
| `x = { a = 1 }`, `f({ a = 1 })`, `[{ a = 1 }]` | `Invalid shorthand property initializer` | parses | A default-mode parser bug: a CoverInitializedName that never becomes a pattern is accepted. |
| `x = class { async x }`, `x = class { *x }` | `Unexpected token '}'` | parses | Default-mode parser bug: a method without parameters and body is accepted. |
| module `<!-- comment`, `/* */ --> comment` | `HTML comments are not allowed in modules` | loads | Default-mode bug: the module loader tokenizes HTML-like comments. |
| `for (let of of c);` | parses | `Unexpected token 'of'` | A default-mode parser bug: `let of` is a valid ForDeclaration. |
| `/[\b-\d]/u` | `Invalid character class` | accepted | Default-mode regexp parser bug. |
| `'use strict'; for (f() of c);` | parses (a runtime ReferenceError) | `Invalid left-hand side in for-loop` | V8 web compatibility; the spec makes it an early error. |
| module `import defer * as ns from './lib.mjs'` | `Unexpected token '*'` | loads | V8 does not ship import defer; gating a test262 feature off for `--node` is not wording. |
| `using x = 1` | `Unexpected identifier 'x'` | `using declaration is not allowed in this position` | V8 does not ship explicit resource management. |
