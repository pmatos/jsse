# TextDecoder under `--node` (#1527)

Part of the TypeScript-on-jsse epic (#1214, tracker #1418). Not needed by `tsc`;
Node's own tests observe it.

## Where it lives

- `scripts/node-textdecoder.js` is the global `TextDecoder`: a class shaped after
  Node's `lib/internal/encoding.js` (symbol-keyed slots `decoder`, `flags`,
  `encoding`, `kIgnoreBOM`, `kFatal`, `kUTF8FastPath`, `handle`, `single-byte`,
  `chunk`, `BOM seen`; `ERR_INVALID_THIS`; `options` validated with
  `kValidateObjectAllowObjectsAndNull`; enumerable accessors; the custom inspect).
  It runs as a prelude script right after `node-buffer-shim.js` and before the
  builtin loader, so `TextDecoder` is a plain data property (writable,
  non-enumerable, configurable) from the first line of user code.
- `scripts/node-buffer-shim.js` keeps only the decoders (`utf8Decode`,
  `utf16Decode`, `windows1252Decode`) and hands them over in the hidden global
  `__node_decoders`, which the class takes and deletes.
- `internal/errors`, `internal/validators`, `internal/util/types` and
  `internal/util/inspect` are loaded on first use through the handoff object
  under `Symbol.for("jsse.node.textdecoder")`; `node-builtin-loader.js` fills in
  its `require`. A decoder that never fails loads none of them.
- `ERR_ENCODING_NOT_SUPPORTED` is defined in `node-lib/internal/errors.js`.

## Node behaviour reproduced (v26.8.2)

- `new TextDecoder("nope")` throws `RangeError [ERR_ENCODING_NOT_SUPPORTED]: The
  "nope" encoding is not supported`; the label is converted with `` `${label}` ``
  (a Symbol throws the plain TypeError) and trimmed of ASCII whitespace only.
- A fatal decoder throws a plain `TypeError` whose only own key is
  `code: 'ERR_ENCODING_INVALID_ENCODED_DATA'` (Node's native path, so the stack
  header has no `[code]`), naming `utf-8`, `utf-16le` or `utf-16be`.
- `decode()` of a non-BufferSource has three messages in Node, by path: utf-8
  without streaming or a pending tail and the utf-16 decoders use the native
  messages (`The "list" argument ...` and `The "input" argument must be an
  instance of SharedArrayBuffer, ArrayBuffer or ArrayBufferView.`, plain
  TypeError with `code`); utf-8 streaming and windows-1252 throw
  `ERR_INVALID_ARG_TYPE('input', ['ArrayBuffer', 'ArrayBufferView'], input)`.
  The issue text says `"input"` for the first; Node 26.8.2 says `"list"`.
- A decode that throws drops the pending bytes; a non-streaming decode resets the
  BOM state first (even when it throws), a streaming one keeps it. The BOM check
  for a call uses the state from before that reset.
- SharedArrayBuffer input decodes; a detached buffer decodes to `""`.

## Guards

- `tests/node_textdecoder_1527.rs` runs `tests/node_textdecoder_1527/probe.js`
  (about 125 observations) under the bytecode VM, the tree-walker and
  `JSSE_GC_STRESS=50`, and compares with `expected.txt`, which is the same script
  on Node: `node tests/node_textdecoder_1527/probe.js > tests/node_textdecoder_1527/expected.txt`.
- The web streams oracle (`./scripts/run-node-webstreams-oracle.sh`) runs Node's
  `test-stream-consumers` and `test-whatwg-encoding-custom-fatal-streaming`, and
  its corpus has the `TextDecoderStream` bad-label observation.
- `tests/node_prelude.rs` keeps `TextDecoder` undefined without `--node`.

## Not covered

Node with full ICU also decodes the legacy multi-byte and single-byte labels
(`big5`, `shift_jis`, `iso-8859-2`, ...); here they throw
`ERR_ENCODING_NOT_SUPPORTED`. `TextEncoder` is unchanged.
