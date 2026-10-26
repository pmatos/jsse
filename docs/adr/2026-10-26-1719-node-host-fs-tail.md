# Node host mode: the long tail of synchronous `fs`

Status: accepted (#1218); extends item 7 "Synchronous `fs`" of ADR 2026-10-26-0854 (#1217).

## Context

#1217 gave `--node` the file, descriptor and basic metadata calls. TypeScript's
compiler and test runner also need `Stats`/`Dirent`, `readdir` options,
`mkdir -p`, `rm -r`, `cp`, links, permissions, timestamps and `mkdtemp`, with
Node's exact errors. The oracle is Node v26.8.2 (the installed `node`; the
oracle script runs locally, not in CI).

## Decision

- **Floor** (`node_host_fs_ops.rs`, Unix only, `libc`): `__host_fs_symlink`,
  `link`, `chmod`/`chown`/`utimes` (path, link and descriptor forms by a `kind`
  argument), `ftruncate`, `fsync`/`fdatasync`, `mkdtemp`, `statfs`, `copyfile`,
  `mkdirp`, `readdir_types`. Syscall names are libuv's (`utime`, `futime`,
  `copyfile`, `mkdtemp` with the `XXXXXX` template in the path). `mkdirp` is a
  port of `MKDirpSync` (first created directory, ENOTDIR/EEXIST decided by the
  same stack walk). `copyfile` follows `uv_fs_copyfile`: destination created
  with the source mode, same-inode copy is a no-op, destination unlinked on a
  failed copy, `copy_file_range` first (so a reflinking filesystem leaves the
  source atime alone, as Node does). `__host_fs_stat` takes a `bigint` flag and
  then returns all 18 fields as exact `BigInt`s; the number path is unchanged.
- **JS** (all registered in `node_prelude.rs`, each parking exports on the
  private `__node_fs_support`): `node-fs-stats.js` (`Stats`, `BigIntStats`,
  `StatFs`, `Dirent` after `lib/internal/fs/utils.js`: lazy own-property
  dates, `*Ms`/`*Ns`, `*Instant` accessors on Temporal), `node-fs-dirs.js`
  (readdir `withFileTypes`/`recursive`, mkdir, mkdtemp, `opendirSync`/`Dir`,
  `rmSync`, small posix path helpers because `path` is not loadable that
  early), `node-fs-ops.js` (links, chmod/chown/utimes families, truncate,
  fsync, `copyFileSync`, `statfsSync`), `node-fs-cp.js` (`cpSync`). The shim
  assembles them with one `Object.assign`.
- **`cpSync`** is `internal/fs/cp/cp-sync.js` plus the three C++ halves of
  `node_file.cc` (`cpSyncCheckPaths`, `cpSyncOverrideFile`, `cpSyncCopyDir`)
  written in JS, including their oddities (the check swaps `stat`/`lstat` when
  `dereference` is set; `ERR_FS_CP_*` come out as plain errors from the C++
  halves and `SystemError`s from the JS half).
- **Errors**: `rmSync` reports `remove_all` failures as Node's
  `ErrnoException` (`EACCES, Permission denied: p 'p'`, positive `errno`, the
  top-level path); `ERR_FS_EISDIR` and the JS `ERR_FS_CP_*` are `SystemError`s
  (`name`, `info`, positive `errno`).
- `statSync(..., {throwIfNoEntry: false})` swallows ENOENT and ENOTDIR,
  `lstatSync` only ENOENT, `fstatSync` neither, as in `node_file.cc`.

## Oracle

`scripts/fs-fixtures/{stats,dirs,ops,cp,errnos}.fixture.js` run on jsse and
Node against the seeded tree and are diffed byte for byte. `errnos` is an
operation x target matrix (missing, under-file, link, dangling, loop, long name)
plus EACCES cases (skipped as root). Fixtures avoid what Node leaves
unspecified: directory iteration order of `opendir`/`cpSync` (jsse sorts bytewise
like `readdir`), inode/device/ctime/birthtime values and free space.

## Known gaps

- `Dir` is a snapshot taken at `opendirSync`; `recursive`, `read(cb)`, `close(cb)`
  and the async iterator belong to async fs (#1234).
- `rmSync` does not sleep between `retryDelay` retries (one attempt).
- `readvSync`/`writevSync`, `globSync`, `mkdtempDisposableSync`, `openAsBlob`,
  `lchmodSync` (absent on Linux, as in Node) are not provided.
- `cpSync` edge cases not matrix-tested: sockets/FIFOs, the C++ `equivalent`
  throw on dangling destinations, `preserveTimestamps` on a read-only source dir.
- Non-bigint `Stats` carry `ino`/`dev` as doubles (exact below 2^53, as in Node).
