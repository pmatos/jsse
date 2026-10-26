# Node host mode: the buffer and string_decoder modules

Status: accepted (#1231), epic #1214. Extends `2026-10-26-0854-node-host-mode.md`.

**`buffer`** and **`string_decoder`** are requireable (`require("buffer")`,
`require("node:string_decoder")`) and share the Buffer the prelude already
installs (`scripts/node-buffer-shim.js`); no second implementation exists.

1. **`buffer`** (`node-lib/buffer.js`). `Buffer`, `atob` and `btoa` are the
   prelude's global values, handed to builtins through the private
   `internal/host/buffer` (the loader seeds it with the three), so
   `require("buffer").Buffer === Buffer` even if user code reassigns the global.
   The key order, property descriptors and function lengths are Node v26.8.2's:
   `transcode`, `isUtf8`, `isAscii`, `kMaxLength`, `kStringMaxLength`, `constants`
   (non-writable, non-configurable), the `INSPECT_MAX_BYTES` accessor, `Blob`,
   `resolveObjectURL`, `File`. There is no `SlowBuffer` (removed in Node 26).
   `INSPECT_MAX_BYTES` writes through to `internal/buffer_inspect.js`, so
   `util.inspect(buf)` honours it.
2. **`transcode`** has no ICU. It decodes to a string and encodes with `?` for
   unmappable code points, and reproduces which pairs Node sends around ICU:
   latin1/ascii to ucs2 widens bytes without validation; utf8 to ucs2 and ucs2 to
   utf8 fail with `U_INVALID_CHAR_FOUND` (errno 10) on malformed input; ucs2 to
   ucs2 replaces lone surrogates and an odd tail byte with U+FFFD; other pairs
   substitute. An unsupported encoding fails with `U_ILLEGAL_ARGUMENT_ERROR`.
   `isUtf8`/`isAscii` are byte loops.
3. **`Blob`/`File`** (`node-lib/internal/blob.js`) copy their parts into one
   `Uint8Array` at construction (`endings`, `type` normalisation, nested Blobs,
   `slice`, `text()` with BOM removal, `arrayBuffer()`, `bytes()`; `File` adds
   `name`, `lastModified`). They are reached only through `require("buffer")`:
   the `Blob`/`File` globals are not installed. Gaps: `stream()`/`textStream()`
   until web streams exist, `resolveObjectURL` always returns `undefined` (no
   object URL registry), and `util.inspect` shows a nested Blob past the depth
   limit as `Blob {}` where Node shows `[Blob]` (Node's keeps symbol-keyed own
   state).
4. **`atob`/`btoa`** are the host functions. Their `InvalidCharacterError`
   messages and the no-argument error (`TypeError` without `ERR_MISSING_ARGS`)
   differ from Node's; changing them would change the default environment.
5. **`string_decoder`** (`node-lib/string_decoder.js`) is Node v26.8.2
   `lib/string_decoder.js`. The native `internalBinding('string_decoder')` is
   ported to `node-lib/internal/string_decoder.js`: the same seven-byte state
   buffer (`kMissingBytes`, `kBufferedBytes`, `kEncodingField`, four incomplete
   bytes) and the same `decode`/`flush` rules as `src/string_decoder.cc`, so
   partial UTF-8, UTF-16 surrogate and base64 tails behave as in Node, including
   its quirks (an odd UTF-16 chunk emits a lone high surrogate before the
   buffered byte). It is built over `Buffer.prototype.toString`, which does the
   per-chunk decoding. `scripts/node-string-decoder-module.js` is unchanged: it is
   the selector the library tests bundle, not the `--node` module.
6. **Stream tests.** The `needs-string-decoder` marker, the probe in
   `tests/node_stream.rs` and in `run-node-stream-oracle.sh` are gone; the 19
   vendored Node stream tests that needed `string_decoder` run unconditionally.

## Verification

`./scripts/run-node-buffer-oracle.sh` diffs `scripts/buffer-fixtures/sd-probe.js`
and `buffer-probe.js` (default and `--no-bytecode`) against Node byte for byte and
checks the digest pinned in `digests.txt`. `sd-probe.js` cuts 26 byte sequences
(valid and malformed UTF-8, surrogates, odd base64 tails) at every pair of
points for nine encodings, plus 1,500 seeded random chunkings per stateful
encoding. `buffer-probe.js` covers module shape, the 15x15 transcode matrix over
nine byte sets, `isUtf8`/`isAscii`, Blob and File. `tests/node_buffer.rs` pins the
digests (no Node needed), runs the probes under `JSSE_GC_STRESS=50`, and checks
the default environment is unchanged.
