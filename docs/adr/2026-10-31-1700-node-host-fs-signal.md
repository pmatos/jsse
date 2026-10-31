# Node host mode: `options.signal` on every `fs` form

Status: accepted (#1505); amends ADR 2026-10-31-1200 (#1341), whose "`signal` on
`readFile`/`writeFile`/`appendFile`" decision and "Known gaps" it replaces.

## Context

#1341 gave `readFile`/`writeFile`/`appendFile` a `signal` that failed an
already-aborted call before it started and, for `readFile`, one that fired before
the whole-file pool job returned. `writeFile` and `appendFile` stayed one
whole-file job, so a signal that fired after the call but before the write left
the data on disk where Node leaves an opened, empty file; the FileHandle forms
and `writeFile` of an iterable ignored the signal; `stat`/`lstat`/`fstat`, which
take a `signal` in Node 26, did not.

## Decision

- **A call that carries a signal is Node's own sequence of steps**
  (`node-lib/internal/fs/signal.js`), not one pool job; a call without a signal
  keeps the whole-file jobs of `callbacks.js` and the FileHandle code of
  `promises.js` (the only edits there are the three `hasSignal` hand-offs).
  The steps are the existing pool primitives (`open`, `fstat`, `read`, `write`,
  `fsync`, `close`), so nothing is added to the floor.
- **Callback API** (`lib/fs.js`): `readFile` and `writeFile` fail an aborted
  signal synchronously through the callback (before the path is validated for
  `readFile`, before the open for `writeFile`); `writeFile` then opens, checks
  again before every `write`, and closes a file it opened before reporting the
  error, so an abort right after the call leaves the file created and truncated
  (`size=0`), as in Node. `writeFile` writes the buffer in one call, as Node does.
  `readFile` of a file of at most 512 KiB (one pool job in Node) is checked once,
  after the read; a larger one before each 512 KiB read.
- **Promise API** (`lib/internal/fs/promises.js`): `writeFile` is Node's
  `writeFileHandle`: check, then one write of at most 512 KiB, check after it,
  for a buffer, and for each chunk of a (sync or async) iterable. `readFile`
  checks before the `fstat`, after it, and before each read.
  `FileHandle.readFile/writeFile/appendFile` and `fsp.readFile/writeFile/
  appendFile(handle)` run under `kRef`/`kUnref`, so `close()` waits for them.
  A call that finished its last step is not failed by an abort that follows.
- **`stat`/`lstat`/`fstat`** (and so `fs.promises.stat/lstat` and
  `FileHandle.stat`): validate the path, then an aborted signal fails the call
  before it starts and one that fires first fails it at once; the result that
  follows is dropped (`bindSignalToReq`).
- **Reads are failed when the abort happens**, as #1341 decided: a `readFile`
  blocked in the open of a FIFO is failed by the abort, not when the open
  returns (Node never calls back). The step that is running finishes, and
  closes its file, behind the already-delivered error. Writes are not raced.
- **Streams**: `createReadStream`/`createWriteStream` and the FileHandle
  stream constructors already honoured `signal` through the stream base class;
  `fs` option validation now names it `options.signal` as Node does.

## Differences kept on purpose

- `fs.readFile(fd, { signal })` with a signal that is already aborted returns an
  empty buffer in Node 26 (`ReadFileContext.close` drops the error for a
  caller-owned descriptor); here it fails with `AbortError`. The oracle fixture
  leaves this case out.
- A `createReadStream` whose signal fires before its open returns opens the file
  after destroying itself; Node then leaks the descriptor about half of the
  time. The port reproduces the open, not the leak's randomness, and the
  fixture prints no descriptor count for it.

## Oracle

`scripts/fs-fixtures/signal-1505.fixture.js` (through
`scripts/run-node-fs-oracle.sh`) compares, for each form, an abort at none /
before / right after the call / on the next tick, what is left on disk, whether
the callback ran synchronously, `cause === signal.reason`, that no descriptor
stays open, iterables aborted between chunks, a 80 MiB write aborted midway
(a whole number of 512 KiB chunks, not all of it), the stream forms and the
validation errors. `tests/node_fs_signal_1505.rs` repeats the deterministic
cases with literal expectations (the oracle needs Node), adds the FIFO,
event-loop, descriptor and GC-stress guards, and checks the default global
environment is unchanged.
