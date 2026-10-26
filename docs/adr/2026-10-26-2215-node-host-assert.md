# Node host mode: the `assert` module

Status: accepted, epic #1214, issue #1225. Ported from Node v26.8.2
(`lib/assert.js`, `lib/assert/strict.js`, `lib/internal/assert/{assertion_error,
utils}.js`, `lib/internal/errors/error_source.js`). Builds on the embedded
builtin table of `2026-10-26-0854-node-host-mode.md`, the CommonJS loader
(#1219), and `util` (#1224), whose `internal/util/comparisons`,
`internal/assert/myers_diff` and `internal/util/inspect` are reused as they are.

## Decision

1. **Vendored, not rewritten.** `require("assert")`, `require("assert/strict")`
   and the `node:` forms reach Node's own source in `node-lib/`. Node 26 has no
   `CallTracker` (removed upstream), so `assert.CallTracker` is `undefined` here
   as well; `partialDeepStrictEqual`, the `Assert` class (`diff`, `strict`,
   `skipPrototype`) and the myers diff output are all there.
2. **`assert.js` is three files.** The upstream file is 26 KB and the budget is
   20,000 bytes per file. `assert.js` keeps `Assert`, `fail`, `ok`, the
   equality family and `partialDeepStrictEqual`; `internal/assert/exceptions.js`
   holds the `throws`/`rejects` machinery (`expectedException`, `expectsError`,
   `expectsNoError`, `getActual`, `waitForActual`); `internal/assert/methods.js`
   exports `installMethods(Assert, kOptions)`, which defines `throws`, `rejects`,
   `doesNotThrow`, `doesNotReject`, `ifError`, `match` and `doesNotMatch` on
   `Assert.prototype` (its body is upstream's, indented one level).
   `exceptions.js` drops `diff: this?.[kOptions]?.diff` (those functions run
   with `this === undefined`, so the expression was always `undefined`) and
   loads `Assert` lazily for the `rejects` / `doesNotReject` checks.
3. **`error_source.js` is a port, not a copy.** `assert(false)` builds its
   message from the failing source line ("The expression evaluated to a falsy
   value: ..."). Node asks V8 for the structured top frame and tokenizes the line
   with acorn. Here `getErrorSourcePositions` parses the first `at` frame of
   `error.stack` (the V8 error API of `2026-10-26-1000-node-host-v8-errors.md`
   reports the same line and column as V8, down to the call position of
   `assert.ok(...)` being the `ok` property), reads the file through `fs`, and a
   small tokenizer in the same file stands in for acorn: it distinguishes names,
   strings, numbers, regexps, templates, `. ?. [ ] ( ) ;` and everything else,
   which is all `getFirstExpression` looks at. `getFirstExpression` is upstream's
   body, including its `terminatingCol = token` quirk. When no file can be read
   (`-e`, `eval`) `innerOk` falls back to `<inspect(value)> == true` instead of
   throwing from `innerFail` with a `[undefined]` message.
4. **Small additions to shared modules.** `internal/errors` gains
   `ERR_AMBIGUOUS_ARGUMENT`, `ERR_CONSTRUCT_CALL_REQUIRED` and
   `isErrorStackTraceLimitWritable`; `internal/util` gains `setOwnProperty`.
   `scripts/node-v8-error.js` `setStack` now defines `stack` with a
   `__proto__: null` descriptor: with `Object.prototype.get` defined (what
   `test-assert-fail` does) reading `err.stack` threw "Invalid property
   descriptor".

## Oracle

`./scripts/run-node-assert-oracle.sh` runs 12 of Node's `test/parallel/
test-assert*.js` files verbatim through `scripts/assert-fixtures/harness.js` on
jsse `--node` and on Node (the harness supplies `../common`, `../common/
fixtures` and a `node:test` shim, since the real `node:test` does not exist
here) and diffs a corpus of about 370 failing assertions (message, code,
operator, `generatedMessage` across every operator, diff mode and message form)
against Node byte for byte, pinned by `corpus.digest`. Files over the byte budget
are stored as consecutive `.partN` slices whose concatenation has the sha256
recorded in `manifest.txt`. Guards: `tests/node_assert.rs`.

jsse-only skips and patches are listed in `manifest.txt` with reasons: no `vm`
module (two `test-assert.js` cases; `vm.runInNewContext` is `eval` in the
harness), #1334 (proxy `ownKeys` traps; `Check proxies`), #1323 (`Object.prototype.toString` of an
`AggregateError`, two cases) and #1333 (missing `at async` frames;
`assert.match(err.stack, /rejects/)` in `test-assert-async.js` is removed).
`test-assert-esm-cjs-message-verify` (child processes, ESM) and
`test-assert-myers-diff` (`--expose-internals`) are not vendored.
