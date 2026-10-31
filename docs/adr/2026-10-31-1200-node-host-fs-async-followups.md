# Node host mode: async `fs` follow-ups

Status: accepted (#1341); the `cp` decision is superseded by ADR
2026-10-31-1500 (#1503); amends ADR 2026-10-26-2300 (#1234).

## Context

#1234 ran `rm`, `cp`, `truncate`, recursive `readdir` and the JS `realpath`
whole on a later loop turn, so a large `rm -r` blocked the loop and validation
failures were reported through the callback; `chmod`/`chown`/`utimes`/`link`/
`symlink`/`mkdtemp`/`statfs` ran on the loop thread; `readv`/`writev`, several
`FileHandle` methods and `signal` were missing.

## Decision

- **Probe, then replay a script of floor calls** (`internal/fs/callbacks.js`).
  The first run of the synchronous function stops at its first floor call (a
  probe), which validates the call and throws its errors synchronously, as in
  Node. The call goes to the pool; when it completes, the function runs again
  with every result so far replayed and stops at its next floor call, until a
  run completes. A function with a handful of floor calls (`rm`, `truncate`:
  open/ftruncate/close, `realpath`: one `lstat` per component) therefore never
  blocks the loop, and a `finally` that makes more floor calls while the probe
  unwinds is harmless because the first recorded call wins. Cost is one run per
  floor call, so the tree walks below are single calls.
- **Tree walks are one floor call each** (`node_host_fs_more.rs`): `rm_all`
  (`remove_all`, depth first, the first failure aborts, errors name the same
  `lstat`/`scandir`/`rmdir`/`unlink` entry the old JS loop reported) and
  `readdir_tree` (the breadth-first listing of `readdir({ recursive })`, each
  entry with its parent's queue position and whether it is descended into).
  `rmSync` and `readdirSync` call the same primitives, so the sync and async
  forms share the walk and the post-processing. `fs.promises.readdir` keeps a
  stack in Node and so lists the last directory found first; `depthFirst`
  re-orders the same tree to match.
- **Metadata calls on the pool**: `Job::{Chmod,Chown,Utimes,Link,Symlink,
  Mkdtemp,Statfs}` with the syscall names and error paths of the synchronous
  floor.
- **`cp` is validated synchronously** (`S.cpValidate`: options and both paths)
  and still runs whole on a later turn, because `cpSync` catches around its
  floor calls and cannot be probed. Superseded: #1503 replaced it with a port
  of Node's promise-based copy.
- **readv/writev** (`internal/fs/vectored.js`) are one `read`/`write` of the
  buffers' combined length on a temporary that is scattered to or gathered from
  the caller's buffers, which is what one `readv(2)`/`writev(2)` does. No new
  floor primitive. `FileHandle.readv/writev/readLines`, `openAsBlob` (eager
  copy) and the `util.promisify` shapes (`kCustomPromisifyArgsSymbol` on
  `read/write/readv/writev`, `promisify.custom` on `exists`) live there too.
- **`signal`** on `readFile`/`writeFile`/`appendFile`: validated synchronously,
  an aborted signal fails the call before it starts, and `readFile` also fails
  if the signal fires before the read completes.
- **FileHandle references** were already as in Node (a stream holds a
  reference and `close()` closes the descriptor when the last one is
  released); `scripts/fs-fixtures/async-1341.fixture.js` now pins it.

## Oracle

`scripts/fs-fixtures/async-1341.fixture.js` (through
`scripts/run-node-fs-oracle.sh`) pins the synchronous throws, the multi-step
callbacks, the pool metadata calls, readv/writev in all three forms, the
FileHandle methods, `openAsBlob`, `signal` and the promisify shapes against
Node byte for byte. `tests/node_fs_async_1341.rs` pins non-blocking behaviour
(setImmediate ticks during a recursive `rm`/`readdir`, metadata calls finishing
while the loop thread is busy), descriptor cleanup after a failed multi-step
call and GC stress.

## Known gaps

`readableWebStream` and a
lazy `openAsBlob` (#1504, blocked by web streams #1360); `signal` on the
FileHandle forms and mid-write aborts (#1505).
