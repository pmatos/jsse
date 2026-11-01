# Library Tests: MessageChannel and transferable streams under `--node` (#1528)

Not a TypeScript oracle: follow-up of the web streams port (`library-tests-typescript-conformance-webstreams.md`, tracker
#1418, epic #1214). Design and the list of differences from Node:
[`docs/adr/2026-11-01-0100-node-host-message-ports.md`](../adr/2026-11-01-0100-node-host-message-ports.md).

- **What exists**: `MessageChannel`, `MessagePort`, `MessageEvent` (lazy globals) and `worker_threads.{MessageChannel,
  MessagePort, receiveMessageOnPort, markAsUncloneable}` for one thread; `structuredClone(x, { transfer })` of
  `ReadableStream`, `WritableStream`, `TransformStream` and `MessagePort`; `structuredClone` of an untransferred stream or
  port throws `DataCloneError` (it used to return `{}`). Without `--node` none of it exists and `structuredClone` is
  unchanged.
- **Oracle**: `./scripts/run-node-messaging-oracle.sh [--node] [--no-cross-check] [--update]` runs
  `scripts/messaging-fixtures/*.fixture.js` on Node, on jsse `--node` and on jsse `--node --no-bytecode`; each must exit 0
  and print its `<name>.stdout` (Node's output). `--update` rewrites the `.stdout` files from Node. The fixtures:
  `channel` (delivery, close order, ref/unref, transfer of ports, events, tick order between messages), `clone-errors`
  (every refusal and its text), `streams` (chunks, errors, cancel, abort, close of transferred streams, streams over a
  channel), `liveness` (nothing keeps the process alive; an unref'd timer is the hang detector).
- **Vendored Node test**: `test-webstreams-clone-unref` is in `scripts/webstreams-fixtures/manifest.txt` and runs with
  `./scripts/run-node-webstreams-oracle.sh --only test-webstreams-clone-unref`.
- **Guards**: `tests/node_messaging_1528.rs` (the fixtures in default, `--no-bytecode` and `JSSE_GC_STRESS=20`; absence
  and unchanged `structuredClone` without `--node`; the mark is invisible and the `[kClone]` path; two answering ports
  do not starve timers; a message runs before an immediate; an idle ref'd port holds the loop and `close()` releases
  it). Each run has a kill deadline, so a port that holds the loop fails the test instead of hanging it.
- **Where the code is**: `src/interpreter/builtins/structured_clone/platform.rs` (mark, hook, walk),
  `node_host_loop.rs` (`__host_loop_defer`), `node_event_loop.rs` (`node_run_host_completions`),
  `node-lib/internal/worker/{io,js_transferable,message_event}.js`.

## Not done

`BroadcastChannel`, `messageerror`, `moveMessagePortToContext`, transferable `fs.promises.FileHandle` (which keeps
`test-filehandle-readablestream` in the manifest's `!` list), `Worker`, Node's "posted to itself" warning.
