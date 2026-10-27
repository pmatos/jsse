# Node host mode: asynchronous `fs`

Status: accepted (#1234); builds on the synchronous `fs` of ADR 2026-10-26-0854
(#1217) and 2026-10-26-1719 (#1218), the event loop of ADR 2026-10-26-0854 item 7
(#1229) and the `stream` module (#1233).

## Context

`tsc` and Node's own tooling read, write and walk files through the callback
API, `fs/promises` and `createReadStream`/`createWriteStream`. Blocking
syscalls must not stall the event loop, and a stream of a 500 MB file must run
in bounded memory.

## Decision

- **One native primitive**, `__host_fs_async(op, callback, ...args)`
  (`node_host_fs_async.rs`), taken private by `node-fs-shim.js` like the rest of
  the floor. It decodes the arguments on the main thread into an owned `Job`
  (`node_host_fs_jobs.rs`: paths, fds, byte vectors copied at submit), runs the
  job on a fixed pool of four lazily started worker threads, and calls
  `callback(err)` or `callback(null, value)` from the poll phase. The worker
  returns a `Reply` or an `AsyncError(errno, syscall, path, dest)`; the main
  thread builds the `uv_error`, the `Uint8Array`, the `Stats` array and copies a
  `read` into the caller's buffer, so no `JsValue` crosses a thread. The jobs
  are the same syscalls as the synchronous floor (they reuse `copy_file`,
  `mkdir_p`, `stat_values`, `read_to_end`): `open`, `close`, `read`, `write`,
  `read_file`, `write_file`, `stat`/`lstat`/`fstat`, `access`, `readdir`
  (with types), `readlink`, `realpath`, `mkdir`, `mkdirp`, `rmdir`, `unlink`,
  `rename`, `copyfile`, `ftruncate`, `fsync`/`fdatasync`.
- **Event-loop integration** (`node_event_loop.rs`, added state in
  `NodeLoopState`):
  - A request table `id -> [callback, destination buffer]` is registered at
    submit and visited by `for_each_root`, so the callback and a read's buffer
    are rooted for the whole operation without a Pinned Native Root (which has
    no unpin). The loop is alive while the table is non-empty, so a script whose
    only pending work is an fs callback still runs it.
  - The worker pushes a closure on the existing `agent_async_completions` queue
    (which also wakes `node_poll`'s condvar wait); that closure runs no JS, it
    only moves a continuation to `NodeLoopState::completed`. `node_poll` first
    runs the queued continuations, each followed by its own tick/microtask
    checkpoint, and returns without sleeping if any ran. JS therefore never runs
    from inside `drain_microtasks`, so completions are not nested in a timer
    callback's checkpoint, and a throw reaches `node_dispatch`, which routes it
    to the uncaught-exception path (an `exit` in a callback exits).
  - `begin_request`/`end_request`/`push_completed` are generic: later handles
    (child processes, #1236) join the loop the same way.
- **`fs` becomes a builtin module** (`node-lib/fs.js`). The prelude still
  assembles the synchronous object and now registers it as
  `internal/host/fs` together with its support bag and the async primitive.
  `fs.js` snapshots the synchronous functions (`bundle.sync`), then installs
  lazy accessors (settling into data properties) for the callback API
  (`internal/fs/callbacks`), the streams (`internal/fs/streams`), `glob`
  (`internal/fs/glob`, `glob_sync`) and the `promises` getter (`fs/promises`),
  so a program that only uses the synchronous API pays nothing for them.
- **Callbacks by probe and replay** (`internal/fs/callbacks.js`). Each
  callback function runs the matching synchronous function twice. First with
  every floor call replaced by a probe that records its arguments and throws a
  sentinel: validation, option parsing and argument errors are exactly the
  synchronous function's, and are thrown synchronously, as in Node. The
  recorded call is submitted to the pool (or, for the metadata calls not worth a
  thread - `chmod`, `chown`, `utimes`, `link`, `symlink`, `mkdtemp`, `statfs` -
  run on the next loop turn). When the result arrives the synchronous function
  runs again with that one floor call replaced by the result (or the error), so
  post-processing (`Stats`, `Dirent`, encodings, `Dir`) is shared code, not a
  copy. `read`/`write` pass `(err, n, buffer)`, void results call `cb(null)`.
  Functions that make several syscalls or are written in JS (`rm`, `cp`,
  `truncate`, recursive `readdir`, JS `realpath`) cannot be probed; they run
  whole on a later loop turn. The probe stubs are built once, but each call
  still swaps the host properties twice and runs the synchronous function
  twice, so a callback call costs a few times its synchronous counterpart; a
  synchronous function that catches around a floor call would swallow the
  sentinel, so none may.
- **Promises** (`internal/fs/promises.js`, `fs/promises`) wrap the callback
  functions, so validation failures become rejections. `FileHandle` is an
  `EventEmitter` over an fd with `read`, `write`, `readFile`, `writeFile`,
  `appendFile`, `stat`, `chmod`, `chown`, `utimes`, `truncate`, `sync`,
  `datasync`, `createReadStream`, `createWriteStream`, `close`,
  `[Symbol.asyncDispose]`. As in Node, `close()` while a stream holds a
  reference emits `close` at once but closes the descriptor only when the last
  reference is released. `promises.realpath` is the native one, as in Node.
- **Streams** (`internal/fs/streams.js`) are Node's `internal/fs/streams.js`
  minus `windowsHandle`, on `fs.open/read/write/close` and `Readable`/`Writable`
  of the merged `stream` module: 64 KiB chunks, `start`/`end`, `fd` as a number
  or a `FileHandle`, `autoClose`, `flush`. A 500 MiB sparse file streams with a
  max RSS under 100 MiB (the test allows 250 MiB).
- **`Dir`** gains `read`, `close`, the async iterator and `asyncDispose`
  (`internal/fs/dir.js`) over the snapshot `opendirSync` takes.
- **`glob`/`globSync`/`fs.promises.glob`**: Node's `internal/fs/glob.js` with
  `#private` members renamed to `_members` so the class splits into a base
  (sync walk, `glob_sync.js`) and a subclass (async walk, `glob.js`), each under
  the 20,000-byte budget. The matcher is the existing `internal/path/glob`.

## Oracle

`scripts/fs-fixtures/async.fixture.js` and `async-fd.fixture.js` run about 90 operations through the
synchronous, callback and promise forms on the seeded tree and prints each
outcome; `scripts/run-node-fs-oracle.sh` diffs it against Node byte for byte
like the other fixtures. Where Node's own forms disagree (the async `readFile`
error of a directory carries the path, the sync one does not) the fixture
prints `forms agree: false` on both engines. `tests/node_fs_async.rs` pins
liveness, ordering, per-callback checkpoints, errors, uncaught/exit paths,
sync-versus-async equality on a tree, GC stress (`JSSE_GC_STRESS=1,5,7`) and the
sparse-file RSS bound (`wait4`, Linux only).

## Known gaps

- `rm`, `cp`, `truncate`, recursive `readdir` and the JS `realpath` validate
  and run on a later loop turn and report validation failures through the
  callback; a large `rm -r`/`cp -r` blocks the loop while it runs.
- `chmod`/`chown`/`utimes`/`link`/`symlink`/`mkdtemp`/`statfs` run on the loop
  thread (on a later turn), not on the pool.
- Not provided: `readv`/`writev`, `openAsBlob`, `FileHandle.readLines` and
  `readableWebStream`, `fs.watch`/`watchFile` (now ADR 2026-10-27-0300), `lchmod` (absent on
  Linux in Node too), `util.promisify` custom hooks on `exists`.
- `readFile`/`writeFile` ignore `signal`; a stream's `encoding` option needs
  `string_decoder` (#1231).
- Pool jobs on one fd are not ordered (as in libuv); a `Job::Read` allocates its
  `length` in the worker and copies into the destination on completion.
- Unix only, like the synchronous floor.
