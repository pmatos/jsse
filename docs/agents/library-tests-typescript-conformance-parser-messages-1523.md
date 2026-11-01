# Node host: parser messages for the V8 productions with their own text (#1523)

Second pass over #1523 (epic #1214), after [node-host-syntax-link-1523.md](node-host-syntax-link-1523.md) (#1550, #1552).
`--node` only unless stated: default mode, `-e` without `--node` and test262 keep the engine's own wording (a guard test
pins it). Triage of the issue's rows against main at 9bb21a87:

| Row of #1523 | State |
| --- | --- |
| `function () {}` -> `Function statements require a function name` | met (#1550) |
| `let let = 1` -> `let is disallowed as a lexically bound name` | met (#1550) |
| `@` -> `Invalid or unexpected token` | met (#1550) |
| doubled `SyntaxError: ` prefix of `eval`, `new Function`, `$262.evalScript` | met (#1550, `ParseError::js`) |
| the long tail of `self.error` sites, "each checked against V8 by hand" | audited again here with 750 more sources |

## What changed

A differential driver (`vm.Script` for the script goal, `import()` of a temp file for the module goal, one
`goal<TAB>json-source` line per input) ran about 750 sources through Node v26.8.2 and `jsse --node`; 720 now agree
(`scripts/node-stack-fixtures-1523-tail/`), 19 are listed in `ts-suite/node-syntax-tail-1523-known-failures.txt`.

- **Destructuring** (`src/parser/v8_messages/patterns.rs`). V8 parses an element of a binding pattern as an expression and
  validates it afterwards, so `var [a.b]` is `Illegal property in declaration context`, `var [1]`, `var {a: (b)}`,
  `([a], 1) => 1` are `Invalid destructuring assignment target`, `var {1}` is `Unexpected number`, `var [...a, b]` is
  `Rest element must be last element` and `function f(...a = 1)` is `Rest parameter may not have a default initializer`.
  The pattern parser keeps its own errors; when an element fails the element is parsed again as an expression and
  classified (`reclassify`: a dotted property chain is an illegal property, a parenthesised name, a call, a literal,
  an `a[0]` is an invalid target, `...x = 1` is invalid). Arrow parameters read again as FormalParameters use the same
  helper, with `(a.b) => 1` an invalid target and `(...a.b) => 1` an unexpected `...`.
- **Object literals**: a key that is not followed by `:` reads as the key's token (`{ 1 }` `Unexpected number`, `{ [a] }`
  `Unexpected token '['`, `{ #a }` `Unexpected identifier '#a'`).
- **Cover grammar**: `(...a)` that is not an arrow function, `(a, ...b)`, and `(...a, b)` (`Rest parameter must be last
  formal parameter`); `async (a = await 1) => 1` is `missing ) after argument list`; `(yield) => 1` and `(await 1) => 1`
  are invalid targets while `(a = yield) => 1` keeps the yield-in-parameters text; `f(a) => 1` is `Malformed arrow
  function parameter list`.
- **Statements**: `if (1) let x = 1` is the single-statement lexical declaration error; `for (let of x)` stops at `x`; `for (let.x of c)` is rejected (spec lookahead, also in
  default mode, where it was accepted); `for (let x, y of c)` is `Must have a single binding.`; `const a, ...` reports the
  missing initializer first; a strict `let` that starts no declaration is `Unexpected strict mode reserved word`;
  `class let {}` words the name in the class's strict mode; escaped keywords read as V8 words them.
- **Modules**: two `export default` (or two declarations of one name) are `Identifier '.default' has already been
  declared`, `export 1` is `Unexpected token 'export'`, `import a, from ...` points at the comma, a string export name without
  `from` and a lone surrogate use V8's text, `await` directly in a static block is `Unexpected reserved word` (also in default
  mode, where a module accepted `class C { static { await 1 } }`), `<!--` and `-->` in module code say so.
- **Regular expressions**: `Invalid quantifier` after a lookbehind, `Incomplete quantifier` for `a{1` in unicode mode,
  `Invalid Unicode escape` for `\c`, V8's modifier-group reasons, `Invalid character class` for `[\d-a]`, and a backslash
  after the flags (`/a/g`) is `Invalid regular expression flags`. Default mode keeps the old reasons (`regexp_error`).
- **Acceptance bugs fixed in every mode** (engine behaviour, `test262-extra/`): `^*`, `$+`, `\b+`, `\B*` and an unmatched
  `)` in a non-unicode pattern are SyntaxErrors (the validator took them for atoms); `(...a)\n=> 1` is rejected; `class
  eval {}` and `class arguments {}` are rejected (class code is strict); `await` in a static block.
- **Decorators** (behaviour, not wording): V8 in Node 26 has none, so under `--node` the `@` of a decorator is rejected
  with `Invalid or unexpected token`; default mode still parses decorators.

## Fixtures and guard

`tests/node_syntax_tail_1523.rs` runs `scripts/node-stack-fixtures-1523-tail/syntax-tail.mjs tail-N.tsv` (N = 1..5, 720
sources) on the bytecode VM, the tree-walker and under `JSSE_GC_STRESS=7` against `expected-tail-N.txt`, which is Node
v26.8.2's own output (the temp directory removed from module messages). Regenerate by running the driver on Node and
`jsse --node`, keeping the sources both print identically, and writing Node's lines. It also pins default mode's wording.

## Not met

`ts-suite/node-syntax-tail-1523-known-failures.txt` lists Node's and jsse's message for each remaining difference:

- **Parser accepts what V8 rejects** (bugs, any mode): `x = { a = 1 }`, `f({ a = 1 })` and `[{ a = 1 }]` (a
  CoverInitializedName outside a pattern; only the parenthesised and statement forms are checked), `class { async x }` and
  `class { *x }`, a module whose first token is `<!--` (the lexer reads the first token before the module goal is set).
- **Features Node 26 does not ship** that jsse implements: `import defer` and `import.defer()`.
- **V8 accepts what the spec rejects**: `'use strict'; for (f() of c)`. And `for (let of of c)`, which jsse rejects.
- Wording: `using x = 1` (V8 has no `using`, so it reads `x` as the stray token), `({ 1n })`, `for await (var a = 1 of b)`, `/[\c]/u`, `let await` in a module, `class yield {}` in a generator,
  `async a\n=> 1`, and the runtime `new import.meta` TypeError.
- #1524 items (export-from edge cases, `import()` nested in expressions) and #1487 are unchanged.
