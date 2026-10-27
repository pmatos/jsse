# Node host mode: `tsc --watch`

Status: accepted, epic #1214, issue #1248 (builds on the fs.watch floor of
`2026-10-27-0300-node-host-fs-watch.md` and the project oracle of
`2026-10-27-0900-node-host-tsc-projects.md`).

## Decision

1. **Own harness.** A watch session never exits by itself, so it cannot run through
   `scripts/run-tsc-projects-oracle.py` (that file is also at its size budget).
   `scripts/run-tsc-watch-oracle.py` loads the helpers of the project oracle (tree copy,
   escape expansion, snapshot, `@replace`/`@remove`, `decode_escapes`) and runs the 38 sessions
   of `scripts/tsc-fixtures/watch-cases.txt` (`name | project | tsc arguments | steps`) on Node
   and on `jsse --node` over the new `projects/watch` tree (a `noLib` project with `src/`,
   `vendor/` and a `tsconfig.opts.json` carrying `watchOptions`) and over `projects/chain`
   for `-b -w`. The file is named `watch-cases.txt`, not `cases-watch.txt`, so the project
   oracle's `cases-*.txt` glob does not try to run an endless session as an invocation.
2. **Steps.** The harness starts `lib/_tsc.js` with stdout and stderr redirected to files,
   waits for the first `Watching for file changes.`, then runs the steps: edits (`@replace`,
   `@write`, `@append`, `@remove`, `@mkdir`, `@rename`, `@touch`) happen at once, so several
   edits between two `@wait` steps are a burst that must coalesce into one cycle through
   tsc's 250 ms debounce; `@wait` blocks until another cycle finished; `@idle S` sleeps
   (used to prove that something does *not* start a cycle); `@exits` as first step marks a
   session that must end by itself (the config file was deleted: tsc prints TS5083 and exits 1).
   The session ends with SIGINT to the process group; the status (`-2` on both) is compared.
3. **Quiescence, not counting.** One edit can cost tsc two cycles: a missing-file watcher
   fires on creation and compiles before the directory watcher's events refresh the
   module-resolution cache, so a second compile follows (`Found 1 error` then `Found 0
   errors`, on Node too). A `@wait` therefore ends only when a cycle beyond the last record
   finished, every started cycle completed (`Starting compilation...`/`File change
   detected...` against `Watching for file changes.`) and the output was silent for
   `--settle` seconds (default 1.0). An earlier draft counted cycles and recorded the second
   one in a different step on each engine. The transcript is also what makes the comparison
   about the "diagnostics sequence": per record, the new stdout and stderr, then every
   file created, changed or deleted since the previous record (emitted `.js`, `.d.ts`,
   maps, `.tsbuildinfo`).
4. **Normalisation.** The project oracle's (work-tree path, `<TSC>`, clock stamps) plus
   `Elapsed:: Nms` of `--extendedDiagnostics` watch tracing and the moving numbers of
   `--diagnostics`. The clock regex has no leading `\b`: after the clear-screen escape
   `ESC[H` the time follows a word character. The clear-screen escape (`ESC[2J ESC[3J
   ESC[H`) is written on pipes too and is compared; `--preserveWatchOutput` drops it.
5. **Sessions.** edit/fix, create (imported, wildcard-matched, in a new directory), delete (an
   imported file, an included but unimported file, a whole directory, then restore), rename,
   an editor's atomic save (write a temp file, rename over), a burst, a `touch`
   (no cycle), `tsconfig.json` edits (option, `include`, broken JSON, deleted), `--preserveWatchOutput`,
   `--pretty` on and off, `--noEmit`, `--noEmitOnError`, `--listEmittedFiles`, `--listFiles`,
   `--extendedDiagnostics`, `--declaration --sourceMap`, `--incremental`, root files without
   a config, all watch strategies (`--watchFile` fixedPolling/priorityPolling/dynamicPriorityPolling/
   useFsEvents/useFsEventsOnParentDirectory, `--watchDirectory` fixedPolling/dynamicPriorityPolling/
   fixedChunkSizePolling/useFsEvents, `--fallbackPolling`, `--synchronousWatchDirectory`),
   `TSC_WATCHFILE`, `TSC_WATCHDIRECTORY`, `TSC_NONPOLLING_WATCHER`, `--excludeDirectories`,
   `--excludeFiles` and the `watchOptions` of a config, a real `--lib es2020`, and `-b -w`
   over composite projects with an error introduced and fixed.
6. **Result.** All 38 sessions are identical on Node v26.8.2 and jsse with TypeScript 6.0.3,
   repeated at 6 and 12 parallel jobs; after the last fixture change (see 7) a Node-vs-jsse run, the
   `--pin` run, a 56-session run of four copies of every polling, strategy and exclusion session
   and the `--no-node` runs (4 and 12 jobs, and `tests/node_tsc_watch.rs`) all passed; no engine change was needed, so this PR touches
   no Rust source and the default (non-`--node`) environment and the test262 baseline are
   untouched. A session takes 2 to 10 s on both engines, 20 to 60 s on jsse for `-b -w` and
   for a broken config (they load the default `lib.d.ts` set or build three projects).
7. **Races in the fixtures themselves** (all Node-vs-Node as well, so not engine bugs), and how
   the sessions avoid them: with a polling `watchDirectory` a directory change and a file
   change landing in one burst are seen by separate polls that can straddle the settle window
   (the `watchOptions` session therefore uses one trigger per step); the harness creates a
   file with `open`+`write`, so the creation event can precede the content, which the
   quiescence rule absorbs; a negative check (`@idle`) is exact only when the engine is right,
   because a late wrong cycle after the idle is attributed to the next record, which then
   differs from Node's, so an `@idle` either ends the session or is followed by a real edit and `@wait`.
8. **Pins and guards.** `--pin` (Node required) writes `watch-expected.txt` (sha256 of the exit
   status and transcript); `--no-node` compares jsse with it, and `tests/node_tsc_watch.rs`
   does so through the built binary (needs `TSC_DIR`; CI has none until #1250, like the
   other tsc guards). The `.tsbuildinfo` of the `-b -w` sessions stores the library path
   relative to the project, so, as for the older build pins, they match only on the
   directory layout they were taken on; node-vs-jsse runs are unaffected.
   `scripts/test_run_tsc_watch_oracle.py` runs in CI: it drives the harness with a fake
   watcher to prove every edit kind, that a different cycle sequence, an early exit or a
   changed digest fails, and that `watch-cases.txt`, the projects and the pins agree.

## Known gaps

- No pseudo-terminal session: the clear-screen escape is not gated on a TTY, so pipes cover
  it; the TTY-dependent `--pretty` default is covered by the diagnostics oracle.
- SIGTERM and a closed stdin are not exercised.
- Linux only (the fs.watch floor is inotify); the polling stand-in used elsewhere is not driven
  by these sessions.

## Running it

    TSC_DIR=/path/to/typescript-6.0.3 uv run python scripts/run-tsc-watch-oracle.py [--jobs N] [--ci] [--dump DIR] [case ...]
