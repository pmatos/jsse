# Library Tests: web streams under `--node` (#1360)

Not a TypeScript oracle: web streams are what Node's `stream` interop and
`fs`/`Blob` build on, and the epic's tracker (#1418) files their gaps.

- **Oracle**: `./scripts/run-node-webstreams-oracle.sh [--node] [--no-cross-check] [--only NAME]` runs the verbatim Node v26.8.2 tests of `scripts/webstreams-fixtures/manifest.txt` on jsse `--node --expose-internals` and Node, then the differential corpus (`corpus-{a,b,run}.js`) on both, which must be byte-identical and match `corpus.digest`. The seven `needs-webstreams` tests of `scripts/stream-fixtures` are run by `./scripts/run-node-stream-oracle.sh`.
- **Guards**: `tests/node_webstreams.rs` (manifest in default mode, `--no-bytecode`, a `JSSE_GC_STRESS=50` sample; corpus plain, tree-walker and `JSSE_GC_STRESS=7`; absence without `--node`; module ids; ESM import; part sizes) and `tests/node_stream.rs` (adapter smoke).
- **Layout**: the big upstream files are directories of `partN.js` joined by `src/interpreter/builtins/node_host_builtins_webstreams.rs`; design and the list of changes from upstream: `docs/adr/2026-10-31-0900-node-host-web-streams.md`.
- **Regenerating the corpus digest**: `./scripts/run-node-webstreams-oracle.sh --node` prints Node's digest on failure; edit `corpus.digest` only when Node's output (not jsse's) changed deliberately.

## Known gaps (the `!` lines of the manifest)

| Test | Gap | Issue |
| --- | --- | --- |
| `test-stream-consumers` | engine `TextDecoder` throws plain `TypeError` for a non-BufferSource | #1527 |
| `test-whatwg-encoding-custom-fatal-streaming` | no error for a dangling byte of a fatal utf-16le stream; no `ERR_ENCODING_*` codes | #1527 |
| `test-webstreams-clone-unref` | `structuredClone(stream, { transfer })` needs `MessageChannel` | #1528 |
| `test-webstreams-pipeline` | needs `http` | none (no `http` module) |
| `test-whatwg-readablestream` | 41 KB, over the file budget; the corpus covers it | none |
