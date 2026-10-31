# Library Tests: hereby stacks, builtin frames, `import()` sites and module error text (#1482, #1483, #1484)

Follow-up to [library-tests-typescript-hereby-1251.md](library-tests-typescript-hereby-1251.md), whose known-failure
table (`stack-native`, `stack-missing-import`, `stack-missing-package`, `stack-syntax`, `stack-missing-export`) these
issues closed: `ts-suite/hereby-stacks-known-failures.json` is now empty, so all 15 cases of
`scripts/run-hereby-stacks-oracle.py` must agree with Node v26.8.2 and any new difference fails the run. Design and
known gaps: `docs/adr/2026-10-31-1600-node-host-native-frames-import-sites.md`.

`TS_SRC_DIR=<TypeScript v6.0.3 checkout, npm ci done> uv run python scripts/run-hereby-stacks-oracle.py [--jobs N] [--jsse BIN] [--require] [case ...]`

The oracle additionally drops `node:diagnostics_channel` frames on both sides (`at TracingChannel.tracePromise`,
which Node's loader adds around `import()` and the embedded loader has no counterpart for), besides the `node:internal`
frames the base hereby oracle drops; every user frame, native frame and `at async` frame must match.

## Fixtures and guards

`scripts/node-stack-fixtures-1482/` holds three fixtures run by `tests/node_stacks_1482.rs` on the bytecode VM, the
tree-walker and under `JSSE_GC_STRESS=7`, against `expected-*.txt`, which is Node v26.8.2's own output with the fixture
directory removed from paths (regenerate by running the fixture on Node and removing `file://<dir>/` and `<dir>/`):

- `native.mjs`: 29 builtin frame cases (`Array` callbacks, `Array.from`, `new Promise`/`Map`/`Set`, `String.replace`
  with a function and a RegExp, `JSON.parse` reviver and syntax error, `JSON`/`Math`-style receivers named after their
  `Symbol.toStringTag`, errors thrown by builtins themselves, builtins under builtins, `Error.stackTraceLimit`, and
  `Error.captureStackTrace` with and without a constructor argument) and the builtins that have no frame in V8
  (`Reflect.apply`, `Function.prototype.call/apply`, a bound function, `new Array`).
- `import-site.mjs`: `import()` from a plain function, an `await` in async functions, a call in the specifier,
  an `await` of a chain, a missing dependency of the imported module, a syntax error, and a missing named, renamed and
  default export (decorated source line, tabs kept).
- `syntax.mjs`: the V8 text of 31 sources through `vm.Script` plus a module that does not parse.

`scripts/test_node_stacks_1482.py` covers the oracle's extra normalisation.

## What matches Node and what does not

Matches: builtin frames as above; an `import()` error's call position (`import` keyword, or the `await` in front of
it, or the last call made while evaluating its specifier); load errors (resolution, parse) carry the caller's live frames
and `at async` chain, link errors (missing export) the `at async` chain only; `Unexpected token '='` / `identifier` /
`number` / `string` / `template string` / `end of input` for the common productions; `The requested module
'./lib.mjs' does not provide an export named 'x'` with the source line and carets for named and default imports.

Does not (filed as #1523 and #1524 under #1418, see the ADR's Known gaps): parser messages of the productions that have their own V8 text;
link errors of `export ... from`; a generator body's frame (`g.next`); an `import()` nested in other expressions reports
the keyword where V8 reports the statement or latest call; `Module not found` for a module imported twice shares the
first error's stack.
