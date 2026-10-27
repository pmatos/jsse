# Node host mode: `child_process.fork` and the IPC channel

Status: accepted (#1237), epic #1214. Builds on
`2026-10-26-2300-node-host-child-process.md` (the spawn floor, which this
extends) and the event loop. Unix only, like that floor.

## Decisions

1. **Layout.** `fork` and `_forkChild` are in `node-lib/child_process.js`;
   `internal/child_process` gained the `'ipc'` stdio entry, the
   `serialization` option and `NODE_CHANNEL_FD` /
   `NODE_CHANNEL_SERIALIZATION_MODE` in `envPairs`, as Node's. New modules:
   `internal/child_process/channel` (`setupChannel`, `Control`, from Node's
   `lib/internal/child_process.js`, MIT), `internal/child_process/ipc_pipe`
   (the libuv `Pipe(IPC)` stand-in), `internal/child_process/serialization`
   (the `'advanced'` format). `internal/child_process/handle` gained
   `adopt(fd)` and the message event. Native: `node_host_child_ipc.rs`
   (`__host_cp_adopt`, `__host_cp_ipc_read`, `__host_cp_ipc_write`), served to
   the builtins through the same private binding as the rest of the floor and
   absent without `--node`.
2. **The parent end is an ordinary pipe.** The `ipc` slot is a `socketpair`
   exactly like the other pipes (`NODE_CHANNEL_FD=<slot>`, 3 for `fork`), so
   the spawn code is unchanged. `ChildProcess.spawn` counts the channel as one
   more thing `'close'` waits for. As in Node, a child the parent disconnected
   itself never emits `'close'` (only `'exit'`): the channel's EOF path is what
   counts it.
3. **The child end is adopted.** The prelude's builtin loader sees
   `NODE_CHANNEL_FD` in `process.env` before the main program runs (CommonJS,
   ES module and `-e` alike), deletes the two variables and calls
   `child_process._forkChild`, which adopts the descriptor
   (`__host_cp_adopt`: close-on-exec, one endpoint of a pid-less handle in the
   same table spawned children use, so reads, writes and close reuse that code
   and its events reach the same JS dispatcher). `process.send`,
   `process.disconnect`, `process.connected`, `process.channel` (a `Control`
   with `ref`/`unref`) and `process._channel` exist as in Node.
4. **Native framing.** The reader thread cuts the byte stream into messages
   (a line of JSON, or for `'advanced'` a 4-byte big-endian length and that
   many bytes of text) and posts all the messages of one 64 KiB read as one
   event (`ChildEvent::Messages`, a JS array of strings); `ipc_write` frames
   one message and writes it. JS never encodes or decodes bytes: with
   `Buffer`/`TextDecoder` the JSON path cost ~430 us per message, native
   framing took 100k ping-pongs from 43 s to under 10 s. Partial reads
   accumulate in the thread (a 16 MB message is a few hundred reads); partial
   writes go to the existing writer thread, so two peers writing large
   messages at once cannot deadlock.
5. **JSON is byte-compatible with Node**: `JSON.stringify(message) + '\n'`,
   UTF-8, internal `NODE_*` commands raised as `'internalMessage'` and never as
   `'message'`. A jsse parent with a Node child and a Node parent with a jsse
   child interoperate (the oracle runs both).
6. **`'advanced'` is not V8-compatible.** jsse has no V8 ValueSerializer, so a
   frame carries a structured-clone graph as tagged JSON text
   (`internal/child_process/serialization`): primitives (`-0`, `NaN`, bigint),
   `Date`, `RegExp`, `Map`, `Set`, `ArrayBuffer` and views (the view's bytes,
   as Node's serializer copies them; `Buffer` stays `Buffer`), errors (name,
   message, stack, cause), boxed primitives, plain objects and arrays (holes
   and extra properties; an own `__proto__` key stays an own property), with
   shared references and cycles. Functions, symbols, promises, `WeakMap`,
   `WeakSet` and `WeakRef` throw a `DataCloneError`. A frame over 256 MiB
   is a `RangeError` on send and ends the channel on receive. A jsse child cannot read a Node parent's
   `'advanced'` frames and the reverse (a decoding error, not a hang); both
   ends must be jsse. Class instances arrive as plain objects, as in Node.
7. **Liveness.** The channel keeps the loop alive while it is open, reading
   and ref'd (`Control.refCounted` per message listener and pending write).
   The child end starts unref'd and is ref'd while `process` has a `'message'`
   or `'disconnect'` listener. `process` is jsse's own minimal emitter (no
   `'newListener'`), so `watchProcessListeners` wraps its emitter methods and
   compares listener counts after each call; messages that arrive before the
   first `'message'` listener are queued for it, as Node does.
   Unlike item 7 of the spawn ADR, IPC reads are not back-pressured: libuv
   reads a channel continuously and so does the thread.
8. **`fork` options.** `execArgv` defaults to `process.execArgv` (`[]`); when
   the child is jsse itself (`execPath` equal to `process.execPath`) `--node`
   is prepended unless `execArgv` has it, since a jsse child only runs Node
   programs under that flag. `execPath`, `silent`, `stdio` (an array must hold
   `'ipc'`: `ERR_CHILD_PROCESS_IPC_REQUIRED`), `cwd`, `env`, `serialization`
   (`'json'` | `'advanced'`), `signal`, `timeout` and `killSignal` are Node's.
   `modulePath` may be a URL or Buffer.
9. **Not done.** Passing handles (`send(message, sendHandle)` throws
   `ERR_INVALID_HANDLE_TYPE`; no `net`/`dgram` yet), and the V8 wire format.
   `send` reports a partial write as `kLastWriteWasAsync` does: a write the
   pipe took whole calls back on the next tick, others when the writer thread
   finishes.

## Rooting

The dispatcher installed by `__host_cp_start` is still the only GC root. The
channel, its pending write callbacks and the message listeners live in JS
(`handles`, `IpcPipe.pending`, the `Control`), reachable from the dispatcher's
table until the pipe closes. Threads own plain bytes and strings; the `Messages`
event builds its array on the interpreter thread at delivery.

## Measurements

100k JSON ping-pongs (`{n, echo}` objects, one outstanding message), a
`--release` build with LTO off on a host at load average ~90, so only ratios are
meaningful: jsse parent and child 9.4 s, Node 1.7 s. Before native framing: 43 s.
The oracle fixture
`fork-bulk` runs this at 100k plus a 20k burst and two 16 MB messages crossing
at once, in both modes.

## Verification

`scripts/run-node-child-process-oracle.sh` diffs the `fork*` fixtures
(`fork`: send/disconnect/error semantics, exit codes and signals, options;
`fork-bulk`; `fork-lifecycle`: parent death, `unref`; `fork-advanced`: jsse
against jsse and Node against Node, since the formats differ) against Node, and
reruns the JSON ones as jsse parent with a Node child and the reverse.
`tests/node_child_ipc.rs` adds GC-stress, large-message, parent-death and
liveness checks without Node.
