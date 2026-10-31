# TypeScript on jsse: async `fs` follow-ups (#1341)

Epic #1214, tracker #1418. Design: `docs/adr/2026-10-31-1200-node-host-fs-async-followups.md`.

## Scope

Node tooling that runs on jsse (the `hereby` runner, test harness code) uses the
callback and `fs.promises` APIs to walk and clean trees. `rm({ recursive })`,
`readdir({ recursive })`, `realpath` and `truncate` now run on the worker pool
(or as one pool job per call), so a long walk no longer starves timers and I/O
callbacks, and invalid arguments throw from the call as in Node.

## Checks

- `JSSE=<release binary> ./scripts/run-node-fs-oracle.sh` runs
  `scripts/fs-fixtures/async-1341.fixture.js` on jsse `--node` and Node and
  diffs stdout/stderr. Every operation is awaited; do not add concurrent
  operations (pool completion order is not deterministic).
- `cargo test --profile release-nolto --test node_fs_async_1341` pins the
  event-loop behaviour (ticker discriminators, a busy loop thread while pool
  jobs run, descriptor cleanup, GC stress with `JSSE_GC_STRESS=1,5`).
- Existing `node_fs`, `node_fs_async` and `node_fs_watch` stay green.

## Known failures (not fixed here)

| Item | Issue |
| --- | --- |
| ~~`fs.cp` runs whole on a later turn; no async `filter`~~ | #1503, fixed (ADR 2026-10-31-1500) |
| `FileHandle.readableWebStream()` absent; `openAsBlob` copies the file eagerly | #1504 (needs #1360) |
| `signal` on the FileHandle forms and aborts between write chunks | #1505 |

## Notes for the next change

- A callback function is the synchronous function run once per floor call; a
  synchronous function that catches around a floor call cannot be added to
  `callbacks.js` (it would swallow the probe), and one with many floor calls
  should get a single floor primitive first (see `rm_all`, `readdir_tree`).
- A new floor primitive needs: the name in `scripts/node-fs-support.js`, a sync
  `host_fs_fn` and a `Job` plus `decode` arm in `node_host_fs_async.rs` (same
  argument layout as the sync call, because the probe records the sync call's
  arguments), and the name in `OFFLOADED` in `callbacks.js`.
- Node's `mkdtemp` error path carries the random suffix libc filled in before
  failing; jsse reports the `XXXXXX` template. The fixture normalises it.
