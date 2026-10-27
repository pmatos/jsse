# Node host mode: the `stream` module

Status: accepted, epic #1214, issue #1233. Ported from Node v26.8.2
(`lib/stream.js`, `lib/stream/promises.js`, `lib/internal/streams/*`). Builds on
the embedded builtin table of `2026-10-26-0854-node-host-mode.md`, the
CommonJS loader (#1219), `events` (#1223) and `util` (#1224).

## Decision

1. **Vendored, not rewritten.** `node-lib/stream.js`, `node-lib/stream/promises.js`
   and `node-lib/internal/streams/{add-abort-signal,compose,destroy,duplex,
   duplexify,duplexpair,end-of-stream,from,legacy,operators,passthrough,pipeline,
   state,transform,utils}.js` are Node's own files. They are reached as
   `require("stream")` / `require("node:stream")` and `require("stream/promises")`;
   every `internal/...` id stays private to builtins. Readable, Writable, Duplex,
   Transform, PassThrough, `pipeline`, `finished`, `compose`, `duplexPair`,
   `addAbortSignal`, the stream operators (`map`, `filter`, `take`, ...),
   `Readable.from`, async iteration, object mode, backpressure and `'drain'`
   ordering are therefore Node's code paths.
2. **`readable.js` is six files and `writable.js` four.** Upstream they are 61 KB
   and 34 KB and the budget is 20,000 bytes per file. The functions are the same
   text as upstream, cut at function boundaries:
   `readable` (the `Readable` constructor and the module export),
   `readable_state` (`ReadableState`, the bit-flag constants, shared helpers),
   `readable_push` (`push`/`unshift`/`setEncoding`, `fromList`, `endReadable`,
   `from`/`fromWeb`/`toWeb`/`wrap`), `readable_read` (`read`, `emitReadable`,
   `maybeReadMore`), `readable_pipe` (`pipe`, `unpipe`, `on`/`off`, `resume`,
   `pause`, `flow`), `readable_iter` (`wrap`, async iterator, `compose`, the
   public accessors); and `writable`, `writable_state`, `writable_write`,
   `writable_end` likewise. The split was generated mechanically from the
   upstream files: each part lists the primordials and requires it uses.
   *Cycle rule.* The parts call each other, and call `Readable`/`Writable`, so
   a part starts with `exports.f = f` for every function declaration it owns
   (hoisting makes that safe before anything else runs) and only then requires
   the others; the facade assigns `module.exports = Readable` first. Constants
   (`k...` flags, symbols) live in the `*_state` part, which exports them once
   defined and requires the other parts only afterwards, so no part ever sees
   an uninitialised constant. Do not reorder those lines when editing.
3. **What was removed or replaced.** `AsyncResource`/`AsyncContextFrame` binding
   in `end-of-stream` (no async_hooks); `isBlob` in `duplexify` (no Blob);
   the `--experimental-stream-iter` interop block of Readable and
   `getOptionValue`; `lazy_transform`, `fast-utf8-stream` and `stream/consumers`
   (nothing in this epic loads them yet). `require('buffer')` is
   `internal/host/buffer` (the global `Buffer`) and `FastBuffer` in
   `stream.js` is `Buffer.from(arrayBuffer, offset, length)`.
4. **Lazy edges.** `string_decoder` is required when a decoder is created
   (`encoding` option, `setEncoding`) and exists since #1231. `internal/webstreams/adapters` is a
   stub whose six entry points throw `ERR_METHOD_NOT_IMPLEMENTED` (`Readable.toWeb`,
   `fromWeb`, the Writable and Duplex pairs) because jsse has no `ReadableStream`.
   `process.stdout`/`stdin` are #1232 and untouched.
5. **Support in shared files.** `internal/errors.js` gains the stream codes
   (`ERR_STREAM_*`, `ERR_MULTIPLE_CALLBACK`, `ERR_METHOD_NOT_IMPLEMENTED`,
   `ERR_INVALID_RETURN_VALUE`, `ERR_INVALID_STATE`, `ERR_UNKNOWN_ENCODING`,
   `ERR_OPERATION_FAILED`) and `aggregateTwoErrors`; messages with `%s` go through
   `util.format` like upstream. `internal/util.js` gains `once` and
   `assignFunctionName`. The builtin loader's primordials table gains
   `AsyncIteratorPrototype`.

## Verification

- `./scripts/run-node-stream-oracle.sh` runs Node's own tests from
  `scripts/stream-fixtures/` (188 of the 255 `test/parallel` files matching
  `test-stream*`, `test-readable-*`, ...; verbatim) on jsse `--node` and Node. The
  fixtures use a small `assert`/`common` prelude (`harness-assert.js`,
  `harness.js`) that runs on both engines, with `mustCall` verified on
  `process.on("exit")`. `manifest.txt` lists every test and the `needs-webstreams`
  flag (run when the probe passes, skipped and reported before), and `!` lines with the reason for every Node test that is
  not vendored: `stream/iter` (experimental) and tests that need `fs`, `http`,
  `async_hooks`, `child_process`, `stream/web` or Node internals. The two largest
  (`test-stream-pipeline`, `test-stream-readable-async-iterators`, over the
  20,000-byte file budget) are among them; the corpus covers their ground.
- `corpus-{a,b,run}.js` is a differential corpus of ~250 observations (event
  and callback order with `nextTick`/microtask markers, backpressure and
  `'drain'`, `cork`/`writev`, `pipe`, `pipeline`, `finished`, `compose`,
  operators, destroy, error codes and messages, module shape). The output must be
  byte-identical on both engines and match `corpus.digest`.
- `tests/node_stream.rs` runs the manifest (default, every third test under
  `--no-bytecode`, a sample under `JSSE_GC_STRESS=50`), the corpus (default,
  tree-walker, `JSSE_GC_STRESS=7`), module-id resolution and privacy, the web
  interop stubs, and that nothing leaks into the default environment.

## Known gaps

`stream/consumers`, `stream/web`, `stream.Readable.toWeb` and friends (web
streams, #1360: the seven `needs-webstreams` tests stay skipped until then, so
the "ported tests pass" criterion of #1233 is scoped to core semantics), `fs`
streams (#1234). `process.stdout` exists (#1232), which unlocked
`test-stream2-writable`. Revisit the
`!` lines in the manifest as those land; the tests that need `fs` or `http` are
the first candidates once #1234 exists.
