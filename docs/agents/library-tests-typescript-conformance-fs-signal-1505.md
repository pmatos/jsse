# TypeScript on jsse: `fs` abort signals (#1505)

Epic #1214, tracker #1418. Design: `docs/adr/2026-10-31-1700-node-host-fs-signal.md`.

## Scope

`options.signal` on `readFile`/`writeFile`/`appendFile` (callback API,
`fs.promises`, `FileHandle`, `writeFile` of an iterable), `stat`/`lstat`/`fstat`
and the stream constructors, compared with Node v26.8.2. Not needed by `tsc`;
Node tooling that cancels file work (watchers, language servers) relies on it.

## Checks

- `JSSE=<release binary> ./scripts/run-node-fs-oracle.sh` runs
  `scripts/fs-fixtures/signal-1505.fixture.js` on jsse `--node` and Node and
  diffs stdout/stderr. It has been run 10 times in a row without a difference.
  Three scenarios depend on timing and are written to survive it: the aborts
  of a large write retry until one lands midway (it prints only whether the
  size is a whole number of 512 KiB chunks and partial), the descriptor count
  waits for background closes, and streams aborted before their open returns
  print no descriptor count (Node leaks it in about half of the runs).
- `cargo test --profile release-nolto --test node_fs_signal_1505` repeats the
  deterministic cases with literal expectations, and adds a FIFO read that must
  be failed by the abort, a 32 MiB write aborted midway with the loop still
  ticking, a descriptor count after 100 aborted calls and `JSSE_GC_STRESS=1,5`.
- `node_fs`, `node_fs_async`, `node_fs_async_1341` stay green.

## Known differences from Node (not bugs to fix)

| Item | Why |
| --- | --- |
| A `readFile` blocked in `open(2)` (FIFO) is failed when the signal fires; Node calls back only when the open returns | Decided in #1341 and kept; the read finishes and closes its file behind the error. Writes are not raced. |
| `fs.readFile(fd, { signal })` with an aborted signal rejects with `AbortError`; Node 26 resolves with an empty Buffer | A Node bug (the error is dropped for a caller-owned fd); not copied. |

## Not covered (open)

| Item | Issue |
| --- | --- |
| `FileHandle.readableWebStream()` and a lazy `openAsBlob` | #1504 (needs #1360) |
| `fs.readFile`'s Node 26 `buffer` option, `FileHandle.pull/pullSync/writer` | not tracked; not used by `tsc` |
| `signal` on the other callback functions that take one in Node (`readdir` has none; `fs.promises.watch` is covered by the #1234 watch work) | none needed |

## Notes for the next change

- A function with a `signal` is not run through `run()`/the probe in
  `callbacks.js`; `withSignal` hands it to `internal/fs/signal.js` before the
  probe. A new function that takes a signal adds its name to `SIGNAL_AT` and a
  form in `signal.js` that checks at Node's points (read Node's
  `lib/fs.js` and `lib/internal/fs/promises.js` for them; `process.binding
  ('natives')` has the source of the installed Node).
- Keep `scripts/fs-fixtures/signal-1505.fixture.js` under 20,000 bytes (it is
  close); add a sibling fixture for new cases.
