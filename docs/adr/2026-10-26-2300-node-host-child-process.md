# Node host mode: child_process and process signals

Status: accepted (#1236), epic #1214. Extends `2026-10-26-0854-node-host-mode.md`.

Unix only (`cfg(unix)` floor); on other platforms the floor is absent and
`require('child_process')` fails at first use.

## Decisions

1. **Layout.** `node-lib/child_process.js` is Node v26.8.2 `lib/child_process.js`
   (MIT) without `fork`/IPC/permission code, with the argument normalisers split
   into `internal/child_process/normalize`. `internal/child_process` is
   `lib/internal/child_process.js` without the IPC channel, handle passing and
   diagnostics channels. Node's `Process` and `Pipe` wraps are
   `internal/child_process/handle` (a `Process` and the id-to-handle table the
   native events are routed through) and `internal/child_process/pipe` (a
   `Duplex` standing in for the `net.Socket` Node wraps around each stdio pipe).
   `internal/child_process/support` holds `ErrnoException`, `convertToValidSignal`
   and the path check. `process.kill` and the `SIGxxx` process events are
   `internal/process/signals`.
2. **Floor.** `node_host_child_spawn.rs` (argument decoding, `socketpair` stdio,
   `Command` with `pre_exec` for fds >= 3 and `detached`), `node_host_child.rs`
   (async spawn, read/write/shutdown/close/release/kill), `node_host_child_sync.rs`
   (`spawnSync`) and `node_host_signal.rs` (`kill`, `getpid`, signal watching).
   `scripts/node-process-signals.js` captures the primitives, deletes the globals
   and hands the binding to the builtin loader (`Symbol.for("jsse.node.child")`,
   served to builtins as the private module `internal/host/child_process`).
3. **Pipes are socketpairs**, as `uv_spawn` makes them, so a child sees a socket
   on fd 0-2 and EOF on stdin after `shutdown(SHUT_WR)`. Pipes at fd >= 3 are
   duplex. `ipc` stdio is a pipe at its slot with `NODE_CHANNEL_FD` set
   (`fork`, `2026-10-26-2330-node-host-fork-ipc.md`).
4. **Environment.** The child's environment is exactly Node's `envPairs`, built
   from `process.env` (ADR item 6), never from the OS environment. The spread
   `{ ...process.env }` of Node's source is `ObjectAssign({}, process.env)`
   because object spread of a Proxy copies nothing (#1339).
5. **Events and rooting.** Worker threads (one `waitid` per child, one reader per
   read pipe, one writer per written pipe, one signal reader) own only plain
   Rust data. Each pushes a `ChildEvent` through the host-async completion queue;
   the closure that runs it only stashes the event in `ChildState::inbox`. The
   poll phase (`node_poll`) then hands the events that were waiting to the single
   JS dispatcher installed by `__host_cp_start`, each followed by a Node
   checkpoint, so a `nextTick` queued by user code still runs before the event
   that arrived later. The dispatcher (`NodeLoopState::host_dispatch`) is the
   only GC root; the children, pipes and callbacks live in JS tables it
   reaches. Pending `Written` completions of a destroyed pipe are ignored.
6. **Liveness.** JS decides what keeps the loop alive and reports it through
   `__host_loop_ref(+1/-1)` (`NodeLoopState::external_refs`): the process handle
   from spawn until its exit event, a read pipe from its first `_read` until EOF
   or destroy, each queued stdin write, all subject to `ref()`/`unref()`.
   Signal watchers never count. `node_poll` no longer blocks once nothing ref'd
   is left alive (libuv's zero poll timeout) and does not skip the poll when an
   immediate is pending (it polls without waiting).
7. **Back-pressure.** A read pipe delivers one 64 KiB chunk per `_read`; the
   reader thread waits for a token the stream grants, so an unconsumed stream
   stops the child instead of buffering without bound. A write is first tried
   at once, like `uv_write`: what the pipe takes completes straight into the
   inbox, so it is ahead of the `Exit` of a child that read it. Only a full
   pipe sends the rest to a writer thread through a channel; `Written`
   completes each write callback in order.
   `spawnSync` feeds input and drains stdout/stderr on separate threads, so
   input larger than the pipe buffer cannot deadlock against output. It waits
   for exit and for every pipe thread to finish (so an `EPIPE` from the stdin
   writer is never lost), except that after a timeout or `maxBuffer` kill it
   stops waiting once the child is gone, even if a grandchild holds a pipe.
8. **Exit reporting.** The waiter uses `waitid(WNOWAIT)`, marks the child exited
   under the mutex `kill` also takes, then reaps, so `kill` never signals a
   recycled pid. `exitCode`/`signalCode` follow libuv (`exitCode` is `-2` after
   `ENOENT`; the signal name is the first name in `os.constants.signals`).
   `spawnSync` returns Node's key order (`error?`, `status`, `signal`, `output`,
   `pid`, `stdout`, `stderr`), `ETIMEDOUT`/`ENOBUFS` kill with `killSignal`.
9. **Signals.** `process.on('SIGxxx')` starts a watcher through a wrapper the
   shim puts on `process`'s own listener methods (they survive a later swap of
   `process`'s prototype for the real emitter); the last listener restores the
   default action. A handler writes the number to a self-pipe; a thread turns it
   into an event. `process.once` listeners are re-synced after delivery.
   `SIGKILL`/`SIGSTOP` fail with `uv_signal_start EINVAL`. `process.pid` is made
   real only if it is still the shim's `1`.

## Not done

- `process.stdin` and `net`-based stdio objects; `stdio` entries that are
  streams must carry an `fd` number.
- `uid`/`gid` are passed through; `windowsHide` and win32 command-line quoting
  are not implemented.
- `process.execArgv` is `[]`. `process.execPath` is the bare binary; since
  #1253 `spawn`/`spawnSync`/`exec*File` prepend `--node` when `file` is exactly
  `process.execPath` and the leading flags do not already include `--node` (`fork` always did),
  so build tools that spawn "node" get a Node-compatible child. A shell string
  that names the binary is not rewritten, and `spawnargs` shows the flag.

## Verification

`scripts/run-node-child-process-oracle.sh` diffs `scripts/child-process-fixtures/`
(`spawn-sync`, `spawn-async`, `exec`, `exec-sync`, `signals`, `loop`) against
Node: fixture commands (echo, cat, `sh -c` exit codes and signals, 3-6 MB
outputs and inputs by length and MD5, jsse itself as the child) and the gate that
the floor is absent without `--node`. `tests/node_child_process.rs` adds
`JSSE_GC_STRESS` runs, large concurrent transfers and liveness.
