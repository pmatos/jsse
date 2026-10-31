# Node host: V8 syntax messages, export-from link errors and re-import (#1484, #1487, #1523, #1524)

Follow-up to [library-tests-typescript-hereby-stacks-1482-1484.md](library-tests-typescript-hereby-stacks-1482-1484.md).
`--node` only: default mode, `-e` without `--node` and test262 keep the engine's own wording (a guard test pins it).
Design record: [`docs/adr/2026-10-31-1900-node-host-v8-syntax-messages.md`](../adr/2026-10-31-1900-node-host-v8-syntax-messages.md).

## Where the wording lives

| What | Where |
| --- | --- |
| Messages that need no parser state (`Octal literals ... strict mode.`, `Invalid or unexpected token` for every lexer failure, regexp flags, numeric separators, `Cannot use import statement outside a module`, ...) | `static_text` in `src/parser/v8_messages.rs`, applied by `Parser::error`, by `ParseError::js` / `js_bare` where a `ParseError` becomes a JavaScript `SyntaxError` (`eval`, `new Function`, `$262.evalScript`, the module loader) and by `vm.Script` (`node_message`) |
| Messages that depend on the token the parser stopped at (`Function statements require a function name`, every `Expected ...` as `Unexpected <token>`, template escapes read from the raw text, `await is only valid in async functions ...`) | `Parser::v8_text` (same file); `v8_unexpected` in `node_hooks.rs` words the token |
| `++x` / `x++`, `for (1 of x)` | `validate_update_target`, `for_head` (`v8_messages.rs`) |
| Regexp literals that do not compile (`/pattern/flags: reason`) | `regexp_literal_message`, called by `validate_regexp_literal` |
| The doubled `SyntaxError: ` prefix of `eval`, `new Function`, `$262.evalScript`, the module loader | `ParseError::js` (`--node` only; default mode keeps `SyntaxError: <message>`) |
| Export-from and star-export link errors, load errors that are retried | `src/interpreter/builtins/node_host_stack/link_errors.rs` |
| `import(spec)` position for a variable or property specifier | `import_call` in `parser/node_hooks.rs` |

Everything is gated by `inferred_names::enabled()` / `node_host.stack.installed`, which only `--node` turns on.

## Link errors and `import()` of a failed module

- `export { absent } from './lib.mjs'` and `export { absent as x } from ...` read like an import of a missing name:
  `The requested module './lib.mjs' does not provide an export named 'absent'`, with Node's decorated source line and
  carets under the whole specifier (`ExportSpecifier` carries `at` and `width`). The two-hop chain reports the module
  that re-exports, as Node does, and star-export ambiguity reads `contains conflicting star exports for name 'y'`.
- Node does not cache a module that failed to **load** (a request that does not resolve or parse): every `import()` builds
  a new error with the new caller's frames. A module that failed to **link** (missing export, ambiguity) or to **evaluate**
  keeps its error object. Link errors carry a hidden `LINK_ERROR` slot; a load error removes the module from the registry
  (`forget_failed_load`) so the next `import()` loads it again.
- `import(spec)` where `spec` is a variable or a dotted property chain reports the position of that load, which is V8's
  latest emitted position (`await import(spec)` no longer reports the `await`).

## Fixtures and guard

`scripts/node-stack-fixtures-1523/` holds the fixtures run by `tests/node_syntax_link_1523.rs` (bytecode VM, tree-walker,
and both under `JSSE_GC_STRESS=7`) against `expected-*.txt`, which is Node v26.8.2's own output with the fixture directory
removed from paths (`file://<dir>/` and `<dir>/`). Regenerate an expected file by running the fixture with `node` from any
directory and applying that normalisation.

- `syntax-scripts-1.mjs`, `syntax-scripts-2.mjs`: 501 sources through `vm.Script`.
- `syntax-modules-eval.mjs`: 72 module-goal sources (`import()` of a file that does not parse), plus `eval`, indirect eval
  and `new Function` (no doubled prefix).
- `link-errors.mjs`: export-from, chain, star ambiguity, and importing twice (parse error, missing dependency, link error,
  throwing module).
- `import-specifier.mjs`: `import()` of a variable, a member chain, a literal, kept and awaited promises.

## Not met

Listed with Node's and jsse's message in `ts-suite/node-syntax-link-1523-known-failures.txt`.

- **#1523**: the productions whose V8 text is produced deep inside the parser (`Rest parameter must be last formal
  parameter`, `Rest element must be last element` in declarations, `Missing initializer in const declaration` at the end of
  input, `Unexpected strict mode reserved word` for a class name, `Invalid destructuring assignment target` in declarations,
  `import() requires a specifier`, `Illegal property in declaration context`) keep jsse's wording, and the parser accepts a
  few sources V8 rejects (see the file). The long tail of the `self.error` sites was audited against about 600 sources; the
  sites a source can reach are all in the fixtures or the file.
- **#1524 item 3**: an `import()` nested in other expressions (`id(import(x))`, `a && import(x)`, an array literal) reports the
  keyword where V8 reports the last emitted position (statement start, operator, `[`); only a variable or property
  specifier is modelled.
- **#1487**: embedded `fs` frames read `node:fs:L:C` instead of `<anonymous>:L:C`, but an uncaught error raised inside a
  builtin has no `node:fs:779` source-line header (Node prints its own `lib/fs.js` line, which the embedded library does not
  have), and the line numbers are the embedded library's.
- A CommonJS main script that does not parse prints no `file:line` header (`ParseError` has no position).
