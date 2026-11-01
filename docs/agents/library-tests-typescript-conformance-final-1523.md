# Node host: the long tail of the early errors of #1523 (fourth pass)

Fourth pass over #1523 (epic #1214), after `library-tests-typescript-conformance-residual-1523.md` (third pass). `--node`
only unless stated: default mode keeps its own wording and accepts and rejects what it did
(`tests/node_syntax_final_1523.rs` pins both).

## Numbers

`scripts/node-stack-fixtures-1523-final/negative_parse_sweep.py` extracts the 4,454 `negative: phase: parse, type:
SyntaxError` tests of `test262/test/language` (one run per test, as written; `onlyStrict` ones get `"use strict";`,
`module` ones run as modules), the driver of `scripts/node-stack-fixtures-1523-tail/syntax-tail.mjs` runs them on Node
v26.8.2 and `jsse --node`, and `compare` groups what differs.

| | differ | wording | accepted by Node |
| --- | --- | --- | --- |
| start of this pass (main at #1578) | 315 | 273 | 42 |
| now | 43 | 1 | 42 |

(The third pass counted 298 with a slightly different scenario set; its 256 wording rows are inside the 273.)
`scripts/node-stack-fixtures-1523-final/final.tsv` has 102 hand-minimised sources, at least one for each class below,
and `expected-final.txt` is Node's output for them; `jsse --node` prints it exactly in the VM, the tree-walker and under GC
stress (`tests/node_syntax_final_1523.rs`).

## Fixed

Code lives in `src/parser/node_hooks/wording_1523_tail.rs`, `src/parser/v8_messages/tail_1523.rs` and
`src/lexer/escaped_start.rs`; the call sites are one-liners.

- `delete (this.#m)` (112 tests): V8 knows the covered form only once it resolves the name, so a name that no enclosing
  class has declared *so far* is `Private field '#m' must be declared in an enclosing class`; a declared one (and every
  uncovered `delete this.#m`) is `Private fields can not be deleted`. No deferral to the end of the class is needed: a
  name declared after the `delete` gives the undeclared text too (`private_delete_error`).
- An escape that is well formed but names no identifier start (`\u0023`, `\u{23}`, `\u0030`, `\uD800`) is `Invalid or
  unexpected token`; a malformed one keeps its own text (`\u00`: `Invalid Unicode escape sequence`, `\u{110000}`:
  `Undefined Unicode code-point`). The hashbang tests (`#\x21`, `\u0023!`) and the class/identifier tests with a ZWJ,
  ZWNJ or `\u007E` start fall here (37 tests). `Lexer::read_escaped_start` returns the reader's own error now, and the
  `\` of `#\x21` is `Invalid or unexpected '#'`. A number followed by `\` is `Identifier directly after number`
  (`0\u00620`).
- `yield` and `await` spelled with an escape as a name, in strict code or a generator (51 tests): `Unexpected strict
  mode reserved word`, `Unexpected identifier 'yield'` and `Keyword must not contain escaped characters`, by context. V8
  words `yield` by the token, except where a generator's statement starts with it (an expression statement or a label);
  `await` is always the keyword text there (`escaped_reserved_error`, `starts_statement`).
- A function declaration where a statement stands, in strict code (13 tests): `In strict mode code, functions can only be
  declared at top level or inside a block.`
- `for ('' in {} ? 0 : 0;;)` and the other invalid for-in/of targets (5 tests): V8 checks the target when it reads `in`
  or `of`, before the right-hand side. The parser builds the target pattern before it parses the rest of the head.
- Class heritage (8 tests): `class C extends () => {} {}`, `extends (o) => [o.#foo]`, `extends async (x) => 1` are `Unexpected
  token '=>'` (V8 stops at the arrow; `heritage_arrow` scans the tokens, so the body is never read and a private name in it
  is not reported).
- `({ eval = 0 } = {})` and `for ({ eval = 0 } of c)` in strict code: `Unexpected eval or arguments in strict mode`; an
  `eval`/`arguments` element of an array or object pattern (`[eval]`, `{ a: eval }`, `[...arguments]`) is `Invalid
  destructuring assignment target`, also in `for await` heads (6 tests).
- Duplicate names in a `for-in` head or a `catch` pattern are `Identifier 'x' has already been declared`; `eval` and
  `arguments` as a function or import name are `Unexpected eval or arguments in strict mode`; `async * constructor` is
  `Class constructor may not be a generator`; `\8` and `\9` in a strict string are `\8 and \9 are not allowed in strict
  mode.`; `await` as the name of an async function expression or a binding in a static block is `Unexpected reserved
  word`, and as an expression in the parameters of a function in a module too; `with ({}) class C {}` is `Unexpected token
  'class'`; `#field in () => {}` is `Malformed arrow function parameter list`; `for (#field in x;;)` is `Unexpected
  identifier '#field'` (`#x in` needs the `in` operator, which a for head's `~In` does not allow: also a default-mode
  message change, no change of what is accepted).
- `break = 1`, `continue = 1`, `break 1`, `break.x`: V8 reads the label before it looks for a loop, so the token after
  `break` is the error; `()\n=> {}` is `Unexpected token '=>'`; `({var x})` and `for ({var x};;)` are `Unexpected identifier
  'x'` (V8 stops after the keyword used as a name, unless the property ends there).
- Escaped words: `\u0061sync function f(){}` (at the start of a statement) and `class C { \u0061sync m(){} }` are `Keyword must
  not contain escaped characters`; `new.t\u0061rget`, `import.m\u0065ta`, `import * \u0061s ns` and `import {} \u0066rom`
  name the whole construct (`'new.target' must not contain escaped characters`, `'as' ...`); an escaped `of` in a head is
  `Unexpected token 'of'`, in `for await` the keyword text; a class named by an escaped strict word is the keyword text in
  strict code and the strict-word text otherwise.
- `using x = null, [] = null` and the `await using` form: `Missing initializer in using declaration`.
- `export { "a" as "<lone surrogate>" }`: V8 reports the string literal local name before the exported one.
- Regular expressions: without the modifiers V8 lacks, `(?i)`, `(?ms-i)`, `(?-s)` are `Invalid group`; `\k<..>` with a name
  that cannot be one (`\k<>`, `\k<a`, `\k<1>`) is `Invalid capture group name` (an undefined group stays `Invalid named
  capture referenced`); `\k` not followed by `<`, with named groups, is `Invalid named reference`; a malformed `\u` in a
  group name is `Invalid Unicode escape`.

## Not met

`ts-suite/node-syntax-tail-1523-known-failures.txt` has the two remaining classes:

- 42 tests: V8 accepts `new import('').prop` and `new import.source(x).prop` (it only rejects the form without the
  property access). ECMAScript forbids `new` before an ImportCall, so the engine keeps rejecting them with `Cannot use new
  with import`. No wording can match an acceptance.
- 1 test: `export var a = 0; import {a as b} from ...` is `Identifier 'a' has already been declared` in V8, which
  declares the import binding `a` before it reads the escaped `as`; jsse validates the module's names after parsing, so it
  says `Unexpected identifier 'as'`.

The issue stays open (`Part of #1523`) only because of that one wording difference and the 42 accepted forms.

## Running

`cargo test --profile release-nolto --test node_syntax_final_1523`. The sweep: `negative_parse_sweep.py extract
<test262> <out>`, run `syntax-tail.mjs <out>/sources.tsv` on Node (`node.txt`) and on `jsse --node` (`jsse.txt`), then
`negative_parse_sweep.py compare <out>`. Regenerate `expected-final.txt` with Node on `final.tsv` after adding a row.
