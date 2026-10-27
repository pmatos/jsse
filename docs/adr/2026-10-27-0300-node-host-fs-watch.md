# Node host mode: `fs.watch`, `watchFile` and `fs.promises.watch`

Status: accepted (#1235); builds on the asynchronous `fs` of ADR
2026-10-26-2300-node-host-fs-async (#1234), the event loop of ADR
2026-10-26-0854 item 7 (#1229) and the host-async dispatcher of ADR
2026-10-26-2300-node-host-child-process (#1236).

## Context

`tsc --watch`, chokidar and hereby watch the file system through `fs.watch`
(directory and file events), `fs.watchFile` (polling) and, increasingly,
`fs.promises.watch`. A watcher must report events while the script is idle,
keep the process alive only while it is ref'd, and keep its callback alive
for its whole life.

## Decision

- **One inotify instance per interpreter** (`node_host_fs_watch.rs`, Linux). The floor is two
  functions taken private by `node-process-signals.js` and handed to builtins on
  the `internal/host/child_process` binding: `watchAdd(path)` returns a watch id
  or throws the `uv` error of the failed `inotify_add_watch` (`syscall` `watch`,
  `path`), `watchRemove(id)` drops it. The first `watchAdd` creates the instance
  and one reader thread that polls it and a stop pipe; dropping the interpreter
  closes the pipe's write end, and the thread closes both descriptors and exits.
  It decodes each event into
  `(wd, bits, name)` and posts a `ChildEvent::Watch` to the host-async
  completion queue; nothing JS-visible crosses a thread.
- **Libuv's event model.** The mask is libuv's (attrib, create, modify, delete,
  delete-self, move-self, moved-from, moved-to). Bit 1 is "attribute or
  modify", bit 2 is "anything else" (so a delete-self, an ignored watch or an
  event on a sub-directory is a `rename`); JS reports `rename` when bit 2 is
  set. An event without a name belongs to the watched path and carries its
  basename, with bit 4 set so JS can tell it from a child of the same name.
  Watches on one inode share a descriptor (libuv does the same), so two
  `fs.watch` of one path both see every event.
- **Delivery** reuses the child-process path: the event sits in the
  `ChildState` inbox until the poll phase, then `watch_deliver` looks the
  descriptor up, and for each watch id still registered calls the one JS
  dispatcher with kind 6, followed by a tick/microtask checkpoint. A watcher
  closed by an earlier listener in the same batch receives nothing; a listener
  that throws takes the uncaught-exception path and later events still arrive.
- **Liveness and rooting.** JS decides, as for child processes: a watch that is
  not closed and not unref'd holds `__host_loop_ref(+1)`; `unref`, `close`,
  `persistent: false` and an aborted `signal` release it. The callbacks live in
  the JS handle table `natives` of `internal/fs/watch_native`, reachable from the
  dispatcher that `NodeLoopState` roots, so no Pinned Native Root is needed and
  nothing leaks once a watcher is closed.
- **JS layout** (`node-lib/internal/fs/`): `watch_native.js` (the handle table,
  `startWatch`), `watchers.js` (`FSWatcher`, `watch`, the `ignore` matcher),
  `recursive_watch.js`, `stat_watcher.js` (`StatWatcher`, `watchFile`,
  `unwatchFile`), `watch_promises.js` (`fs.promises.watch`). `fs.js` and
  `fs/promises` load them lazily, so a program that never watches pays nothing.
- **`FSWatcher`** follows Node v26: the watch starts synchronously, errors are
  thrown (`ENOENT: no such file or directory, watch '<path>'` with `errno`,
  `syscall`, `code`, `path`, `filename`; `throwIfNoEntry: false` returns an idle
  watcher; `ENOSPC` carries Node's "System limit for number of file watchers
  reached"), `encoding` including `buffer`, `signal`, `ignore` (string globs by
  minimatch with `matchBase`, `RegExp`, function or an array), `ref`/`unref`
  returning `this`, `close` idempotent with `close` emitted on the next tick.
  Unlike Node, an invalid `listener` is rejected before the watch starts, so it
  cannot leak a persistent watcher.
- **Recursive** watching is not native on Linux. `RecursiveWatch` keeps one
  watch per directory and reports names relative to the root. A directory that
  appears is watched and everything in it is announced as a `rename`, which
  closes the window between its creation and its watch; one that vanishes
  loses its watches; sub-directory self-events are dropped because the parent
  reports them, except that a rename-type self-event (the directory was
  deleted or moved, so the kernel dropped its watch) makes the tree re-examine
  the path and watch whatever directory is there now. The parent's inode
  comparison alone cannot do this: ext4 reuses a removed directory's inode
  number for the next `mkdir`, so a replacement looked like the dead directory
  and was never watched again (#1377); symlinked directories are not followed; `ignore` also prunes
  the walk. Node's own recursive watcher (a watch per file with stat
  comparisons) reports a different multiset of events, so the oracle compares
  only that the expected name is reported.
- **`watchFile`** is libuv's `uv_fs_poll` over a timer (`stat_watcher.js`): the
  first poll reports only a failure, later polls compare ctime, mtime,
  birthtime, size, mode, uid, gid, ino and dev, a missing path reports all-zero
  `Stats`, and the per-listener ref counts of Node's `StatWatcher` are ported.
  `bigint` yields `BigIntStats`.
- **`fs.promises.watch`** is an async generator over an `FSWatcher`: validation
  and `ENOENT` surface on the first `next()`, events queue up to `maxQueue`
  (default 2048) and `overflow: 'error'` fails with
  `ERR_FS_WATCH_QUEUE_OVERFLOW` after the queue drains, `signal` aborts with
  `AbortError`, and leaving the loop closes the watcher.
- **Other hosts.** Where inotify is missing, or `JSSE_NODE_WATCH_POLL` is set,
  `watchAdd` throws `ENOSYS` and `startWatch` falls back to `PollWatch`: a
  100 ms timer that diffs a directory listing (or one file) and reports the
  difference as `rename` and `change`. kqueue and ReadDirectoryChanges are not
  implemented; the polling stand-in is the only non-Linux path.

## Oracle and tests

`scripts/fs-fixtures/watch.fixture.js` runs through `run-node-fs-oracle.sh`
against Node. It never sleeps: every step ends with a sentinel directory, and
one inotify queue is ordered, so the step is over when the sentinel's event
arrives; steps print the set of `type:name` pairs. `watch.golden.txt` is the
recorded Node v26.8.2 output; `tests/node_fs_watch.rs` diffs the fixture against
it (also under `JSSE_GC_STRESS=5`) and checks liveness with a spawned process
that is driven by its own `ready` line: persistent versus `unref`, `close`,
`persistent: false`, `signal`, `watchFile`, `promises.watch` with `break`,
throwing listeners, closing a sibling from a listener, callback rooting under
`JSSE_GC_STRESS=1`, the polling fallback and the recursive tree.

## Known gaps

- Linux only for native events; macOS and the BSDs poll every 100 ms.
- No `error` event from the kernel: inotify has none, only the recursive
  watcher can emit one (a sub-directory watch failing with `ENOSPC`/`EMFILE`).
- An event queue overflow (`IN_Q_OVERFLOW`) is dropped, as in libuv.
- `fs.watch` of a path whose inode is replaced follows the old inode, as in Node.
- A recursive watcher over a very large tree makes one watch per directory and
  counts against `fs.inotify.max_user_watches`.
