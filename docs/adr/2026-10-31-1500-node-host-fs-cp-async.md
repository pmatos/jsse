# Node host mode: non-blocking `fs.cp` with an async `filter`

Status: accepted (#1503); amends ADR 2026-10-31-1200 (#1341), whose "`cp` is
validated synchronously and still runs whole" decision it replaces.

## Context

`fs.cp` and `fs.promises.cp` ran `cpSync` whole on a later loop turn, so a large
`cp -r` blocked the event loop, `filter` could not return a promise, and the
error shapes were those of `cpSync` (a C++ walk in Node) rather than those of
Node's asynchronous copy. `cpSync` catches around its floor calls, so the
probe/replay used for `rm`/`truncate`/`realpath` cannot split it.

## Decision

- **The asynchronous copy is a port of Node's `internal/fs/cp/cp.js`**
  (`node-lib/internal/fs/cp.js`), written on `fs.promises`. Every stat, lstat,
  copyfile, mkdir, symlink, readlink, unlink, utimes and chmod is already a
  pool request there, and `copyDir` awaits each entry of the `Dir` async
  iterator, which resumes on a later loop turn, so the loop keeps running.
  Node's own port was the cheaper and more faithful route than a second
  step-wise walk of `cpSync`: the sequence of calls, their order and the
  `ERR_FS_CP_*` shapes come from the same code.
- **`filter` is awaited** (`await opts.filter(src, dest)`), so it may return a
  promise; a rejection fails the copy with its reason. `cpSync` still rejects a
  promise with `ERR_INVALID_RETURN_VALUE`.
- **Validation is synchronous** (`S.cpPrepare` in `scripts/node-fs-cp.js`:
  options, then `src`, then `dest`, as `fs.cp` does) and returns the options
  with defaults. `fs.cp` throws; `fs.promises.cp` is an `async` function and
  rejects. The callback is invoked as `util.callbackify` does, `cb(null,
  undefined)` on success.
- **Error shapes follow the asynchronous port, not `cpSync`**: lower-case
  messages (`cannot overwrite directory ... with non-directory ...`), no
  `Recursive option not enabled` check (`ERR_FS_EISDIR`, `... is a directory (not
  copied)`), no socket/FIFO special case before the walk, `errno` values from
  `internalBinding('constants')`. The oracle fixture pins them.
- **`internalModuleStat`** (a synchronous `stat` of the symlink source in
  `onLink`) is the one synchronous call, as in Node.
- Entries are visited in the order the host lists a directory; the fixture sorts
  trees and avoids asserting on which entry fails first.

## Oracle

`scripts/fs-fixtures/cp-1503.fixture.js` (through `scripts/run-node-fs-oracle.sh`)
runs about 60 copy scenarios in both the callback and promise forms (files,
directories, symlinks, option combinations, sync and async filters, rejecting
filters, error shapes, callback arguments, validation) against Node byte for
byte. `tests/node_fs_cp_1503.rs` pins that `setImmediate` ticks keep running
during a callback and a promise copy of a 4,800 entry tree, that an async
filter is awaited for every entry root first, no descriptor leaks and GC
stress.
