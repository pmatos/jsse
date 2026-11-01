# Default-mode parser conformance (#1571, part of #1214)

TypeScript's own parser tests and the `--node` syntax fixtures (#1523) found five places where
jsse accepted or rejected the opposite of what ECMAScript says. They are default-mode bugs, so
test262 is the gate; the tests live in `test262-extra/` and follow the test262 header format.

| Source | Rule | Fix |
| --- | --- | --- |
| `x = { a = 1 }`, `f({ a = 1 })`, `[{ a = 1 }]` | 13.2.5.1: CoverInitializedName is an early error unless the literal is refined to a pattern | `src/parser/cover_init.rs` |
| `class { async x }`, `class { *x }`, `class { async *x }` | 15.7: `async` and `*` introduce a method, which needs parameters and a body | `src/parser/declarations.rs` |
| module starting with `<!--` or `/* */ -->` | B.1.1: HTML-like comments are Script only | `src/parser/module_start.rs` |
| `for (let of of c)` | 14.7.5: `of` is a BindingIdentifier in a ForDeclaration | `src/parser/for_head.rs` |
| `/[\b-\d]/u` | 22.2.1.1: a class escape is not a range endpoint under `u`/`v`; `\b` is U+0008 | `resolve_class_escape` in `src/interpreter/builtins/regexp.rs` |

## CoverInitializedName

`{ a = 1 }` is parsed as an object literal, so the parser cannot know at that point whether the
literal becomes a pattern. `Parser.cover_init` holds two bits:

- `pending`: a CoverInitializedName was parsed and no pattern has claimed it.
- `defer`: the next AssignmentExpression may leave its own literal pending, because its caller
  decides. Array elements, spread elements, property values and the elements of a parenthesised
  or `async (...)` list set it through `parse_cover_element`.

`parse_assignment_expression` brackets every expression with `begin_cover_scope` and
`end_cover_scope`. When the expression ends with `pending` still set it is a SyntaxError, unless
it was a deferred element that is itself an array or object literal. The constructs that
reinterpret a literal claim the flag:

- `left = ...` with an array or object literal on the left (`claim_cover_for_pattern`, before the
  right-hand side is parsed so the right-hand side is checked on its own);
- the `)` of a parenthesised or `async (...)` list followed by `=>` (`end_cover_list`; without
  `=>` the flag is an error, which replaces the earlier per-shape checks);
- the head of `for (` when `in` or `of` follows (`parse_for_head_expression`).

A literal followed by a member access, call, template, operator or `?`, a literal in a computed
key, a template substitution, a `yield` operand or a class heritage (`parse_heritage_expression`)
is never a pattern, so the flag stays set and the enclosing expression rejects it. Both execution
engines see the same AST; only the parser changed.

## Class elements

After `async`, the parser took any following name as a field when no `(` came. Now a line
terminator after `async` makes it a field (automatic semicolon insertion, as for `get`/`set`),
and `async name` or `*name` without `(` is an error whose wording comes from `unexpected_token`,
so `--node` reports V8's `Unexpected token '}'`. A field named `async` now goes through
`finish_class_field`, which also applies the `arguments` initializer check it skipped.

## Modules and HTML-like comments

The parser lexes its first token when it is built, before it knows the goal, so a leading
`<!--` or `-->` comment was skipped in Script mode and the Module then started after it.
`parse_program_as_module` re-reads that first token with `Lexer.is_module` set, which is what
every later token already saw.

## Regular expressions

`resolve_class_escape` returned no value for `\b`, so `[\b-\d]` looked like a range with no start
and the `-` was taken as a literal. `\b` in a class is U+0008, which also makes `[\b-\x01]` and
`[\x09-\b]` range-order errors, and `\B` is the identity escape in Annex B.

## Parse time on typescript.js

`typescript@6.0.3/lib/typescript.js` (9.1 MB) wrapped in an uncalled function, parse only, the
same non-LTO release build before and after (`CARGO_PROFILE_RELEASE_LTO=off`), 40 alternating
runs on a loaded 24-core host (user plus system seconds):

| | min | p25 | median |
| --- | --- | --- | --- |
| before | 0.228 | 0.252 | 0.320 |
| after | 0.233 | 0.262 | 0.333 |

The per-expression cost is two byte moves and a flag test; the difference is inside the spread of
a shared host. Instruction counts were not available (no `perf` or `valgrind` on the box).

## Verification

- `test262-extra/`: 32 new negative files and 5 positive ones. On the previous build the
  directory fails exactly 60 scenarios, all of them new files; the rest pin behaviour that already
  held (the parenthesised, sequence and for-of member-target cases, which older per-shape checks
  caught, and the Script-mode positives). All pass now (`cover-initialized-name-*`, `class-*-without-parameters-early-error`,
  `html-*-module-start-early-error`, `regexp-*-early-error`, `cover-initialized-name-pattern-contexts`,
  `class-async-and-generator-element-forms`, `for-let-of-binding-named-of`,
  `regexp-class-range-backspace-endpoint`, `html-comment-script-start`).
- About 130 hand-picked sources compared against Node 26.8.2 (`new Function` for scripts, `.mjs`
  files for modules); all agree.
- `scripts/node-stack-fixtures-1523-tail/syntax-tail.mjs` over the five `tail-N.tsv` files gives
  the same output on `jsse --node` and on `node`, including the messages. Rows of
  `ts-suite/node-syntax-tail-1523-known-failures.txt` for these five constructs are therefore
  resolved; that file still lists them.
