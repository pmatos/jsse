# Library Tests: module errors of the entry point, and where #1484, #1508, #1510 and #1524 stand

Part of epic #1214 (tracker #1418). `--node` only: default mode prints what it always did (a guard test pins it).
Follows [library-tests-typescript-hereby-stacks-1482-1484.md](library-tests-typescript-hereby-stacks-1482-1484.md) and
[node-host-syntax-link-1523.md](node-host-syntax-link-1523.md).

## What changed

The module loader made the errors of `import()` read like Node's, but an ES module that is the **entry point** (or a static
dependency of it) went through a different path and still differed:

- The launcher parses the main module itself, so a parse error printed the parser's `Debug` text
  (`SyntaxError: ParseError { message: "Unexpected token '='" }`), exit status 2, with no location.
  `Interpreter::node_entry_syntax_error` (`builtins/node_host_stack/entry_errors.rs`, called from `run_source_with_interp`)
  now builds the `SyntaxError` and delivers it through the entry-point failure path Node uses
  (`node_uncaught_entry`): `file:///f.mjs:LINE`, the source line (tabs kept; CRLF, U+2028 and non-ASCII handled), carets under
  the token the parser stopped at (none at the end of input), a blank line, then the error. Exit status 1.
- A static dependency that does not parse (`import "./dep.mjs"`) printed `[SyntaxError: ...]` without the excerpt. The
  loader (`parse_failure`, same file) gives it the same excerpt, unless an `import()` is loading it: Node's `stack` has no
  excerpt there and the report takes the caller's frames, as before.
- A link error of the entry (missing named or default export, `export { x } from`, star conflict) printed inside
  brackets, `[file:///f.mjs:1 ... SyntaxError: ...]`. `util.inspect` brackets an error whose stack has no frames, and the
  embedded loader has none of Node's `node:internal` frames. `internal/process/fatal.js` (`loaderStack`) prints the stack
  of a `SyntaxError` that starts with an excerpt and has no frames as it is.

Nothing here is observable through ECMAScript, only through the host's report, so the regression test is in `tests/`.

## Verification

`scripts/node-stack-fixtures-1484/` holds 15 entry modules (8 parse failures, a dependency that does not parse, 5 link
errors, a throwing dependency) with `expected-*.txt`: Node v26.8.2's stderr with `file://<dir>/` and `<dir>/` removed
and the lines containing `node:internal` or `node:diagnostics` dropped (the embedded loader has no such frames, and every
oracle drops them). Regenerate one by running the entry with `node` from any directory and applying that. `tests/
node_module_errors_1484.rs` replays them under the VM, the tree-walker and `JSSE_GC_STRESS=7` and checks the exit status
(1) and that default mode keeps its own text.

## Status of the issues (checked on Node v26.8.2 against `jsse --node`)

- **#1508** (`at async Promise.all (index N)`, ` [as method]`, async generator frames): every row matches. The issue's
  reproducer, `async-1508.js`, `timers-1508.js`, `generators-1529.js`, `generators-1529-gaps.js` and `positions-1548.js`
  agree with Node once `node:internal` frames are dropped.
- **#1510** (builtin frames and message text): every message and frame of the issue matches (`RangeError: Invalid count
  value: -1`, `number 5 is not a function`, `Promise resolver 1 is not a function`, `Reflect.ownKeys called on non-object`,
  `Object prototype may only be an Object or null: undefined`, the `JSON.parse` text, `Cannot convert undefined or null to
  object`; `at String.repeat (<anonymous>)`, `at Array.map`, `at new Promise`, `at JSON.parse`, `at Reflect.ownKeys`). Two
  differences remain, neither a row of the issue: V8 names some anonymous arrows of an array literal after the array
  (`at cases (f.js:4:13)`), and `delete null.x` is reported at the `delete` (column 11) where jsse reports the property.
  Further wording is #1558.
- **#1484**: `const = 1;` reads `Unexpected token '='`; a missing named import reads `The requested module './lib.mjs'
  does not provide an export named 'absent'` with the decorated source line; `stack-syntax` and `stack-missing-export` of the
  hereby stacks oracle pass (15/15). The long tail of V8 parser wording is #1523.
- **#1524**: items 1 and 2 are done (see node-host-syntax-link-1523.md). Item 3 (an `import()` nested in other
  expressions) is not: see the next section.

## Not met

- An uncaught **missing dependency** of the entry (`import "./nope.mjs"`) lacks the `node:internal/modules/esm/resolve:272`
  source excerpt (`throw new ERR_MODULE_NOT_FOUND(` and its caret) that Node's report prints from its own library source.
- The excerpt locates the token the parser stopped at, because `ParseError` carries no position. V8 marks the offending
  lexeme: when the error is about an earlier token (`let = = 2` marks `let`), is an early error found later (a duplicate
  declaration) or comes from the lexer (`"abc;`, `12abc`, whose position is the previous token's) the line, and so the
  carets, may be wrong.
- A CommonJS main script that does not parse still prints no excerpt (the loader that compiles it is the embedded
  `Module._compile`).
- **#1524 item 3.** Data for whoever takes it. V8 reports the last source position its bytecode generator emitted before the
  `import()` call (the call itself has none); on Node 26.8 that follows these rules, which reproduce about 130 probed shapes:
  1. A statement position is set at each statement (the start of an expression statement, `return`, `throw`, `if`; the
     start of each declarator's initializer) and by the comma operator for its right operand, a conditional's
     condition is not one, an object literal property value sets an expression position at its start.
  2. An expression position is **dropped** while a statement position is still pending, that is until the first bytecode
     after it is emitted. `id(import(x))` is therefore the statement start (the load of `id` is the first bytecode),
     `(await import(x))` is the statement start (the `await` position is dropped), `a && import(x)` is the statement
     start, while `0 + id(import(x))` is `id`, `gl.q = id(gl, import(x))` is `gl`, `Promise.all([import(x)])` is `all`,
     `o.m(import(x))` is `m`, `a[b](import(x))` is the `[`, `new C(import(x))` is the statement start.
  3. Positions that survive: a property load (the name; `?.` for an optional one; the `[` of a keyed one, after its key),
     a variable load, a call (after its arguments; the callee's last identifier, else the `(`), `new`, a binary operator,
     an assignment's `=` (after the right side; none for the target's own load), `await` and the tagged template's
     backtick (before the operand).
  4. `import(spec)` emits a bytecode before it reads `spec`, which is why a variable or property specifier reports the load
     (the existing special case).
  Implementing it means a position tracker in the parser (events in evaluation order, saved and restored around function
  bodies and arrow parameter lists), hooks in `expressions.rs`, `statements.rs` and `declarations.rs`, which the file
  token budget keeps from growing, and identifier positions the AST does not carry.
