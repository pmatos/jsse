# TypeScript oracles: process groups (#1425)

Every TypeScript oracle driver starts its engine (Node or `jsse --node`) through `scripts/oracle_procs.py`, so no process of an engine's tree outlives the run. Orphaned mocha/hereby workers once spun at 50-80% CPU for hours (#1424).

## API of `scripts/oracle_procs.py`

- `group(cmd, **popen_kw)`: context manager. The command leads a new session (its own process group), is registered for cleanup (`stdin` defaults to `/dev/null`), and on leaving the block the whole group gets SIGTERM, a 2 s grace, then SIGKILL, and the leader is reaped. Raises `RuntimeError` once the oracle is shutting down, so `--jobs` workers do not start new cases after Ctrl-C.
- `run_group(cmd, cwd, env, timeout)`: `(exit code as str or "timeout", stdout, stderr)`; output goes to temporary files, so a straggler holding the pipes cannot block the reader.
- `run_measured(cmd, cwd, env, timeout, merge_stderr=False)`: the same plus wall seconds and the peak RSS of the largest process of the tree (`wait4`).
- `call(cmd, **popen_kw)`: no timeout, inherited or given streams, returns the exit status (long builds and nested oracles).
- `install_cleanup()`: call it first thing in a driver's `main()`; SIGINT/SIGTERM/SIGHUP and interpreter exit kill every live group. `kill_all` sends SIGTERM first and SIGKILL after the grace: a group leader that is itself an oracle driver (`run-hereby-local-oracle.py` runs `run-tsc-projects-oracle.py`; `run-ts-compiler-df-oracle.py` runs `run-ts-compiler-oracle.py`) then runs its own cleanup and takes the groups it started with it. SIGKILL alone would orphan them.

## Drivers

`run-hereby-oracle.py`, `run-hereby-local-oracle.py`, `run-tsc-projects-oracle.py`, `run-tsc-watch-oracle.py` (the session still ends with SIGINT, whose exit status joins the pinned record; the group is reaped afterwards), `run-tsc-selfcompile-oracle.py`, `run-ts-suite-oracle.py`, `run-ts-unittests-config-sys-oracle.py` (and its `tsc` and `watch` wrappers) and `run-ts-compiler-df-oracle.py` all go through it, as do the earlier adopters (`run-ts-compiler`, `-mocha`, `-lint`, `-project`, `-fourslash`, `-transpile`, `-unittests`, `-budget`, `run-hereby-tests`). The conformance wrappers call the compiler oracle's `main()`, which installs the cleanup.

Not covered: short helper commands that run to completion in the foreground (`cargo build`, `cp -a`, `git` queries) and the shell oracles (`run-tsc-oracle.sh`, `run-node-*.sh`).

## Guard

`scripts/test_oracle_procs_1425.py` (run by `unittest discover -s scripts`) loads each driver, runs its real launch function (`execute`, `hereby_local`, `run_case`, `run_measured`, `sh`, `launch`, `run`) on a stub engine that backgrounds a `sleep 300` grandchild and records its pid, and checks the grandchild is dead after a timeout, after a normal exit that leaves it behind, and after SIGINT or SIGTERM to the driver (exit status 128+signal). A nested case (driver, inner driver, grandchild) covers the SIGTERM-then-SIGKILL order. A static guard fails if any `run-ts-*`, `run-tsc-*` or `run-hereby*` oracle calls `subprocess.Popen`.
