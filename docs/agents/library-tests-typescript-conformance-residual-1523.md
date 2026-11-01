# Node host: the residual early errors of #1523 and the #1557 close-out

Third pass over #1523 (epic #1214), after `library-tests-typescript-conformance-parser-messages-1523.md` (second pass)
and `library-tests-typescript-conformance-parser-1571.md` (#1574, the parser bugs). `--node` only unless stated:
default mode keeps its own wording, and accepts and rejects what it did (a guard test pins both).

## Rows

All 19 sources of `ts-suite/node-syntax-tail-1523-known-failures.txt` (second pass) now give Node's output on
`jsse --node`. Three were still open when this pass began; eight had been fixed by #1574 (the `{ a = 1 }` forms, the
bodiless `async x` / `*x` methods, the `<!--` module starts, `for (let of of c)`) and eight by #1567/#1568 (`{ 1n }`,
`for await (var a = 1 of b)`, `/[\c]/u`, `new import.meta`, `let await`, `async a\n=> 1` twice, `class yield` in a
generator) without the list being pruned. The three:

| Source | Was | Now |
| --- | --- | --- |
| `using x = 1` (script top level, `switch` case) | `using declaration is not allowed in this position` | `Unexpected identifier 'x'`: V8 ships `using` but reads it as a name where a declaration may not stand |
| `'use strict'; for (f() of c);` | `Invalid left-hand side in for-loop` (early error) | parses; a ReferenceError at run time, like every other call target in V8 |
| module `import defer * as ns from './lib.mjs'` | loads | `Unexpected token '*'` |

`import defer` and `import.defer()` are not shipped by V8 in Node 26 (`import source` / `import.source()` are), so
under `--node` the parser reads `defer` as a name: `import defer * as ns` and `import defer x from` are syntax errors
and `import.defer(x)` is `Unexpected identifier 'defer'`. `Parser::defer_imports` is the one switch.

## Call expressions as targets (`src/parser/node_hooks/wording_1523.rs`, `builtins/node_host_call_targets_1523.rs`)

Web compatibility already let sloppy code write `f() = 1`, `f()++` and `for (f() of c)` (the call runs, then a
ReferenceError). V8 does the same in strict code, where ECMAScript has an early error, so `Parser::call_target` accepts
a call in strict code too once the `--node` hooks are on. `f() &&= 1` stays a SyntaxError (V8 too). The run-time messages
follow V8: `Invalid left-hand side in for-loop` for both loops and `Invalid left-hand side expression in prefix
operation` / `postfix operation` for `++`/`--` (the engine's own `for-of loop`, `for-in loop` and `update expression`
stay in default mode).

Engine bug found on the way, fixed in every mode: the state machines of generators, async functions and `for await`
ignored a call as the head of a `for-of`/`for-in` that suspends (`for (f() of c) await 0`): `f` was never called and no
error was thrown. `Interpreter::assign_to_for_left` runs the call and throws, like the plain loops
(`tests/node_syntax_residual_1523.rs`, default mode; `tests/node-v8-messages-1523/call-targets.js`, `--node`).

## `using`

`Parser::using_not_allowed` skips `using` (`await using`) and reports the binding like V8 (`Unexpected identifier 'x'`);
`using_missing_init` words `using x` as `Missing initializer in using declaration` (`await using`: `... in await using
declaration`) and `for (using x in y)` as `Invalid 'using' in for-in loop`.

## Sweep of the negative parse tests

A differential driver (`scripts/node-stack-fixtures-1523-tail/syntax-tail.mjs`, one source per line) ran the 4,454
`negative: phase: parse, type: SyntaxError` tests of `test262/test/language` through Node v26.8.2 and `jsse --node`.
Before this pass 517 differed (475 wording, 42 accepted by Node); now 304 do (262 wording, 42 accepted by Node).
Fixed, with the V8 text each gets (`scripts/node-stack-fixtures-1523-residual/residual.tsv` has a source for each):

- `for await (const [...x = []] of y)` and every other error inside the pattern of a `for await` declaration:
  `Invalid left-hand side in for-await-of loop: Must have a single binding.` (72 tests; V8 reads the head again as an
  expression and words any failure alike; `for_await_pattern`, in `parse_for_declaration_list`).
- A class expression named `yield`, `let`, `static`, `implements`, ...: `Unexpected strict mode reserved word` (the
  declaration form already did); in a generator `Unexpected identifier 'yield'`; a name with an escape is a strict word.
  A strict `yield` outside a class name wins over the generator reading.
- `#constructor` as a method is `Class constructor may not be a private method`; as a reference
  `Private field '#constructor' must be declared in an enclosing class`.
- `for (const let in x)`: `let is disallowed as a lexically bound name`.
- `do async function f() {} while (0)` and the other loop bodies: `Async functions can only be declared at the top level
  or inside a block.`
- `[...x = 1] = a`, `([...x = 1]) => 1`: `Invalid destructuring assignment target`.
- `import.source()`: `import() requires a specifier`; `import.source` in a script not followed by `(`:
  `Cannot use import statement outside a module`.
- An escaped strict word as a name (`var implements`): `Keyword must not contain escaped characters`.

## Not met

`ts-suite/node-syntax-tail-1523-known-failures.txt` has the shortest source of each remaining class with Node's and
jsse's text. The larger ones:

- 112 tests: `delete this.#m` (any form) where no class declares `#m` is `Private field '#m' must be declared in an
  enclosing class` in V8 and `Private fields can not be deleted` here. The name is only known to be undeclared when the
  class body ends, so the delete error would have to be deferred to `pop_private_scope` (`parser/mod.rs`, which may not
  grow).
- 37 tests: `#\041` and the other escapes in a hashbang are `Invalid or unexpected token`; the lexer words the same
  `Invalid Unicode escape sequence` for them and for a malformed `\u` in an identifier, and a parser-level mapping cannot
  tell them apart.
- 42 tests: V8 accepts `new import.source(x)`; jsse says `Cannot use new with import`.
- Escaped keywords in labels, `async`, `of`, `as`, `from`, `meta`, `target`; regexp `\k<..>` and `(?i-)` reasons; numeric
  literals followed by an escape; `for (a ? b : c in d;;)` (V8 checks the head before the `)`).
- `for (using x = 1 of y)` and `using x = null, [] = null`, the run-time `using` TypeError text
  (`An object is expected with `using` declarations`) and the source-phase `import source x from` link error.

These keep the issue open as a long tail; none of them is an acceptance difference except the 42 above.

## #1557

Every row of #1557 is met on main: the call in an iterable position, the parenthesized initializer, `var [a] = [] = nul`,
the folded constant, the default and array-pattern parameters, the position of an uncaught destructuring error in a
parameter (#1566, merged in #1570) and the bracket in a string-literal key of a callee (`a["]"]()`, `a.] is not a
function`). The cases of the issue give identical output on Node, the VM and the tree-walker (checked by hand and by
`tests/node-v8-messages-1557`, `-1567` and `-1511`). The issue is closed.

## Running

`./scripts/run-node-v8-messages-1523-oracle.sh` (Node 26.8; `--node` regenerates `tests/node-v8-messages-1523/*.out`)
or `cargo test --profile release-nolto --test node_syntax_residual_1523`. The 176 sources of
`scripts/node-stack-fixtures-1523-residual/residual.tsv` run through the same driver as the earlier passes; its
`expected-residual.txt` is Node's output. Regenerate: run the driver on Node, keep the rows `jsse --node` prints
identically.
