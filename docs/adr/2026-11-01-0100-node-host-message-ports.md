# Node host: message ports and the structuredClone transfer protocol (#1528)

Part of epic #1214. Follow-up of the web streams port (`2026-10-31-0900-node-host-web-streams.md`), whose
`structuredClone(stream, { transfer })` threw `DataCloneError` because there was no `MessagePort` and no way for a
platform object to say how it is transferred. `--node` only: without it no mark is ever set, `structuredClone` takes the
paths it always had, and `MessageChannel`, `MessagePort` and `MessageEvent` do not exist.

## Decision

1. **The mark and the hook live in the engine, the protocol in JS.** HTML gives platform objects `[[Clone]]` and
   `[[Transfer]]` steps; Node spells them `markTransferMode(obj, cloneable, transferable)` and the methods
   `[kClone]`, `[kTransfer]`, `[kTransferList]`, `[kDeserialize]` (`lib/internal/worker/js_transferable.js`).
   `__host_transfer_mark(obj [, mode])` stores the mode (bit 1 transferable, bit 2 cloneable) as a private element of
   the object, the engine's equivalent of V8's private symbol: invisible to `Reflect.ownKeys`, proxies and
   `util.inspect`, and traced by the collector with the object. `__host_transfer_hook(fn)` installs, once, the JS
   function the walk calls with an operation code (`[kTransferList]()`, `[kTransfer]()`, `[kClone]()`, deserialize,
   constructor name). `node-lib/internal/worker/js_transferable.js` installs it and keeps Node's `markTransferMode`
   shape, so the vendored stream classes are unchanged. The hook is pinned on the global object.
2. **`structuredClone` walks marked objects like any other.** `structured_clone/platform.rs` adds one case to
   `clone_object`: a marked object in the transfer list is transferred (the `data` of its record is cloned with the same
   memory map, so a port inside it is the cloned port, then `new Ctor()` plus `[kDeserialize](data)`); one that is not in
   the list is cloned if cloneable; otherwise `DataCloneError`, with Node's two texts ("Object that needs transfer was
   found in message but not listed in transferList", "Cannot clone object of unsupported type."). That is also the
   answer to `structuredClone(new ReadableStream())`, which used to return `{}`.
3. **The transfer list is expanded first**, breadth first through `[kTransferList]()` (a transform stream names its two
   sides, each side its new port), duplicates skipped, as Node's `AddNestedHostObjects`. Listed objects the value never
   reached are still transferred, in list order, after the walk (a stream locks, a port detaches), and are not
   deserialized. Under `--node` a non-transferable entry says "Found invalid value in transferList." as Node does.
4. **`MessageChannel` is two `PortData` records.** `node-lib/internal/worker/io.js`: `postMessage` clones through the
   engine's `structuredClone` (`{ value, ports }` with the transfer list, so the ports of the event are the clones the
   walk made), appends to the peer's queue and schedules delivery; a transferred port is detached and its record moves
   to the new object, queue included. `close()` drops the local queue, queues a close marker behind what was already
   sent to the peer and emits `'close'` on a later turn, so both ends see the messages first. Event shapes follow
   Node: `on('message')` gets the data, `onmessage` and `addEventListener` a `MessageEvent` (`message_event.js`, global
   `MessageEvent`), a listener or `onmessage` ref's and starts the port, `ref`/`unref`/`hasRef`/`start`/`close`.
5. **Delivery is a poll-phase job per message.** `__host_loop_defer(callback, front)` queues a continuation that runs
   with its own tick and microtask checkpoint, keeps the loop alive until it ran, and (`front`) runs ahead of the other
   queued jobs. A port delivers one batch (up to `max(1000, queue length)` messages) in front-queued jobs, then waits
   for the next turn. The poll phase pops only what was queued when it began plus those front-queued continuations
   (`node_run_host_completions`), so two ports answering each other cannot starve timers and immediates. Ordering
   against `setImmediate` matches Node: messages first.
6. **Liveness.** A port keeps the loop alive while it is open and ref'd (`__host_loop_ref`, through the child-process
   binding); a new or transferred port starts unref'd, a listener ref's it. The cross-realm stream sources and sinks
   unref their ports and ref them only while a read or write waits, so an unread transferred stream never holds the
   process (`test-webstreams-clone-unref`).

## Differences from Node (deliberate or left open)

- A queued message keeps the loop alive even for an unref'd port; Node lets such a port miss a message sent as the
  process exits.
- Between different ports the order of two deliveries queued in one phase follows queue order, not libuv's handle list.
- `[kTransfer]()` of a listed object runs when the walk first reaches it (or after the walk when unreferenced), not
  after the whole value was serialized; a throwing walk can leave an earlier stream locked.
- The errors the engine throws are `DataCloneError` objects of the engine's `DOMException`, not Node's class.
- No `BroadcastChannel`, `messageerror`, `moveMessagePortToContext`, `kClone` consumers (the `[kClone]` path is
  implemented, nothing uses it), transferable `FileHandle`, `Worker`, or the "posted to itself" warning.
- `receiveMessageOnPort` and the other `worker_threads` exports are what the one-thread module offers: `MessageChannel`,
  `MessagePort`, `receiveMessageOnPort`, `markAsUncloneable` (now effective).
