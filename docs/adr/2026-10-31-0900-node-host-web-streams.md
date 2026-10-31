# Node host mode: web streams

Status: accepted, epic #1214, issue #1360. Ported from Node v26.8.2
(`lib/internal/webstreams/*`, `lib/stream/web.js`, `lib/stream/consumers.js`).
Builds on the embedded builtin table of `2026-10-26-0854-node-host-mode.md`
and the `stream` module of `2026-10-26-2000-node-host-stream.md`, which left
`internal/webstreams/adapters` a stub.

## Decision

1. **Vendored, not rewritten.** `ReadableStream` (default and byte streams,
   BYOB readers, `tee`, `pipeTo`/`pipeThrough`, async iteration,
   `ReadableStream.from`), `WritableStream`, `TransformStream`, the queuing
   strategies, `TextEncoderStream`/`TextDecoderStream`,
   `CompressionStream`/`DecompressionStream`, `stream/web`, `stream/consumers`
   and the `Readable`/`Writable`/`Duplex` `toWeb`/`fromWeb` adapters are
   Node's own files, so the order of promise settlements and microtasks is
   the Streams standard's as Node implements it.
2. **Parts joined at compile time.** `readablestream.js` is 118 KB,
   `writablestream.js` 44 KB, `transformstream.js` 22 KB and `adapters.js`
   25 KB; the budget is 20,000 bytes per file. Unlike `readable.js` in the
   `stream` ADR (cut into modules that call each other), a file here is cut
   at line boundaries into `part1.js`, `part2.js`, ... in a directory named
   for the module (`node-lib/internal/webstreams/readablestream/part1.js`)
   and `src/interpreter/builtins/node_host_builtins_webstreams.rs` joins the
   parts with `concat!(include_str!(...))`. The module source is the
   (adapted) upstream text, so its classes, `const`s and functions share one
   module scope and there is no load-order cycle to manage: ES classes are
   not hoisted, which made the export-first trick of the `stream` ADR
   unworkable. A part is not valid JavaScript by itself. The cut is
   mechanical, at a blank line before a column-0 line; do not hand-edit a
   part in a way that changes the joined text beyond what upstream differs.
3. **What was changed from upstream.**
   - `internalBinding('messaging').DOMException` is
     `require('internal/per_context/domexception')`; `internalBinding('util')`
     is `internal/binding/util`; `internalBinding('buffer').copyArrayBuffer`
     is a `Uint8Array.prototype.set` copy in `webstreams/util`.
   - `adapters.js` drops `newReadableStreamFromStreamBase` and
     `newWritableStreamFromStreamBase` (libuv stream handles of `net`/`tls`,
     which jsse does not have; #1504's `FileHandle.readableWebStream` does
     not need them) and the zlib failure-code set is built on first use from
     `require('zlib').constants`.
   - The transfer protocol (`MessageChannel`, `markTransferMode`) is a
     placeholder (`internal/worker/{io,js_transferable}`): structured-clone
     transfer of a stream is #1528. `structuredClone` (used by `tee`'s clone
     mode) and `queueMicrotask` are the engine's, captured before user code
     runs (`internal/host/webstreams` in the builtin loader).
   - Everything else is verbatim; `stream/consumers` and the encoding streams
     use the engine's `TextEncoder`/`TextDecoder` through `internal/encoding`.
4. **Support in shared files.** `internal/errors` loads
   `internal/errors/webstreams` (`ERR_MISSING_OPTION` and the
   `ERR_INVALID_STATE.TypeError`/`.RangeError` and
   `ERR_INVALID_ARG_VALUE.RangeError` variants). `internal/util` gains
   `getDeprecationWarningEmitter` (exported), `normalizeEncoding` and
   `SideEffectFreeRegExpPrototypeSymbolReplace`; `internal/validators` gains
   `validateBuffer` and `kValidateObjectAllowObjectsAndNull`; `internal/webidl`
   gains `createEnumConverter`. The primordials table gains `SafePromiseAll`,
   `SafePromiseAllReturnVoid` and `SafePromisePrototypeFinally`
   (`internal/per_context/safe_promise`, Node's `SafePromise`, loaded on
   first call).
5. **Globals.** The seventeen classes are lazy globals like `AbortController`
   (`LAZY_GLOBALS` of the loader): an accessor that loads the defining module
   and settles into the writable, non-enumerable data property Node has.
   Without `--node` none of them exists. Until first touched the property is
   an accessor, where V8 reports a data property (the same for every lazy
   global).
6. **`Blob`.** `Blob.prototype.stream()` and `textStream()` read the Blob's
   bytes through a byte `ReadableStream` (they were absent until now), and
   `internal/blob` exports `isBlob`, which `Duplex.from(blob)` uses.

## Verification

- `./scripts/run-node-webstreams-oracle.sh` runs 23 of Node's own
  `test-webstreams-*`, `test-whatwg-*stream*`, `test-stream-duplex*` and
  encoding-stream tests (`scripts/webstreams-fixtures`, verbatim) on jsse
  `--node` and Node, then `corpus-{a,b,run}.js`, ~280 observations
  (chunk, settlement and microtask order, `inspect` output, error codes,
  byte streams and BYOB, `tee`, `pipeTo`, strategies, text and compression
  streams, the `toWeb`/`fromWeb` adapters, Blob streams and `stream/consumers`)
  that must be byte-identical on both engines and match `corpus.digest`.
  The `node:test` shim registers every test as a `mustCall`, so a test whose
  promise never settles fails.
- The seven `needs-webstreams` tests of `scripts/stream-fixtures` now run
  (`./scripts/run-node-stream-oracle.sh`).
- `tests/node_webstreams.rs` runs the manifest (default, tree-walker, a
  `JSSE_GC_STRESS=50` sample), the corpus (default, tree-walker,
  `JSSE_GC_STRESS=7`), global absence without `--node`, module ids, ESM import
  and the 20,000-byte budget of the parts.

## Known gaps

`!` lines of `scripts/webstreams-fixtures/manifest.txt`: the engine's
`TextDecoder` error codes and fatal utf-16le flush (#1527), structured-clone
transfer of streams (#1528), `http` (`test-webstreams-pipeline`), and the 41 KB
`test-whatwg-readablestream` over the file budget.
