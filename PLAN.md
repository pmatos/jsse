# Plan: node-host ADR + `--node` script args, argv, env, cwd, exitCode (#1215)

## Problem restated
`--node` already installs a native `__host_*` floor (`src/interpreter/builtins/node_host.rs`, #229/#242) and `scripts/node-shim.js` builds a `process` on it, but that `process` is a static stub: `argv = ["node","/bundle.js"]`, `env = {}`, `cwd() = "/"`, no `exitCode`, `execPath = "/usr/bin/node"`. `src/cli.rs` is a plain clap derive, so `jsse --node app.js -- a b c` loses the `--`, and `jsse app.js --version` prints jsse's version. There is no ADR describing the Node host mode (floor + JS prelude, default globals untouched, module embedding/versioning). This issue writes that ADR and makes `process.argv/argv0/execPath/env/cwd()/chdir()/exitCode/exit()` real and Node-identical, cross-checked against Node by a runnable oracle script. Embedding the prelude into the binary is #1216 and is NOT done here.

## Spec basis
N/A: no JavaScript behavior change. Node's `process` API is host-defined and outside ECMA-262; the only language-level obligation is that the default (non-`--node`) global object is unchanged, which holds because every new global is a non-enumerable `__host_*` property installed only by `enable_node_host()` (ECMA-262 §19 "The Global Object" permits host-defined additional properties; with the floor off none exist).

## Assumptions
- #1216 (embed prelude) is a separate open issue, so the `process` surface stays in `scripts/node-shim.js` for this PR; the ADR records the target design (embedded, `include_str!`, versioned with the binary) and this PR only adds floor primitives + shim wiring. (best guess)
- Node semantics checked on Node v26.9.0 (this machine):
  - `node a.js -- x y --version` gives `argv.slice(1) = [a.js, "--", "x", "y", "--version"]`: after the script path everything is verbatim, **including a literal `--`**. "Exactly as Node would" therefore keeps the `--`. `node -- a.js -- z` consumes only the first `--`. `node -e CODE a --b -- c` gives argv `[node, a, --b, --, c]`.
  - `exitCode="7"` exits 7; `1.5` throws `ERR_OUT_OF_RANGE`; `300` exits 44 (low byte); `exit()` with no arg uses `exitCode`; `exit(0)` overrides it; `exit(-1)` is 255; an uncaught throw exits 1 even when `exitCode=4`; `exitCode=null` reads back `undefined`.
- Arg splitting applies in all modes, not only `--node` (uniform parsing; `process` is only observable under `--node`). Gate: the implementer greps `scripts/`, `tests/`, `.github/`, `fuzz/` and the test262 runner for `jsse` invocations placing jsse flags after the file path; if any exist, fix them in this PR (flags move before the file). If that proves large, fall back to splitting only when `--node` is present and record it in the ADR. (best guess)
- `process.env` is a JS-owned object seeded once from the OS environment; the OS environment is never mutated (Rust 2024 `set_var` is `unsafe`, and Atomics agent threads exist). Child processes (#1236) must take `process.env` explicitly as their env: this is the "design note only" acceptance item and goes in the ADR. Consequence: `process.env.TZ = ...` does not retarget the native time zone (#263 reads the OS `TZ` at startup); out of scope, noted in the ADR.
- Parse errors keep exit status 2 (Node uses 1); untouched.

## Key Files
| File | Role | Lines of Interest |
|------|------|-------------------|
| `src/cli.rs` | clap `Cli` struct, `run_main`, `exit_code_from_result`, `new_interp` | 8-44 (`Cli`), 93-108 (`exit_code_from_result`), 138-152 (`new_interp`), 257-297 (`run_main`) |
| `src/interpreter/builtins/node_host.rs` | `enable_node_host`, `install_host_global`, `setup_node_host` | 24-45, 47-end |
| `src/interpreter/mod.rs` | `Interpreter` fields `node_host_enabled`, `pending_exit`, `host_clock_start` + initialisers | 456-462, 889-890 |
| `src/interpreter/tests.rs` | `mod node_host_tests` (`run_node_script`, `assert_node_ok`, `host_globals_absent_when_floor_off`) | 4427-4470 |
| `scripts/node-shim.js` | `globalThis.process` stub | 997-1140 (`process`), 25-45 (host capture + `delete` pattern) |
| `scripts/run-shim-fixtures.sh`, `scripts/run-node-shim-selftest.sh` | existing jsse-vs-Node runners (template for the new oracle) | whole |
| `scripts/README.md`, `docs/agents/library-tests.md` | document shims / `LIB_ENV` ("not reflected in `process.env`" becomes stale) | `scripts/README.md:98`, `library-tests.md` |
| `docs/adr/README.md` | ADR naming: `YYYY-MM-DD-HHMM-slug.md` | |
| `CONTEXT.md` | domain vocabulary | |
| `tests/console_stderr_routing.rs` | pattern for child-process integration tests (`CARGO_BIN_EXE_jsse`) | 1-40 |
| `docs/adr/2026-10-23-1200-step-budget-uncatchable-exit.md` | ADR style + exit-status precedent (124, `pending_exit`) | |

New files (`[new]`): `docs/adr/<date -u +%Y-%m-%d-%H%M>-node-host-mode.md` (real UTC time at authoring, per `docs/adr/README.md`), `tests/node_host_process.rs`, `scripts/run-node-process-oracle.sh`, `scripts/process-fixtures/*.fixture.js`.

## Steps (TDD order; each slice red then green)

### 0. Write the ADR `[new]` `docs/adr/<date -u +%Y-%m-%d-%H%M>-node-host-mode.md` (real UTC time at authoring, per `docs/adr/README.md`)
Status: accepted (#1215), epic #1214. Sections: Context; Decision; Rejected; Consequences. Decision points:
1. Opt-in: only `--node` (never `--prelude`, which test262 uses) enables host mode; default global environment stays byte-identical (guarded by `host_globals_absent_when_floor_off` + test262).
2. Layering: small native Rust floor (`__host_*`, non-enumerable globals, JS-callable only by the prelude, captured then `delete`d like `__host_proxy_target`) + JS prelude implementing Node APIs. Native only for what JS cannot do: syscalls, entropy, clock, exit, env/cwd/argv snapshot.
3. Embedding/versioning (implemented by #1216): prelude modules are `include_str!`d from a repo directory into the binary, loaded in a fixed order by `enable_node_host`, versioned by the jsse binary version (no separate on-disk lookup, so no version skew); `scripts/node-shim.js` becomes a thin or removed wrapper once embedded. Until then the shim is prepended by harness scripts (current state).
4. Process surface semantics from the Assumptions list (argv verbatim incl. `--`, argv0/execPath, env, cwd/chdir errors, exitCode validation and precedence: `pending_exit` > uncaught-throw 1 > `process.exitCode` > 0).
5. Env design note: JS-owned object, OS env not mutated; child_process takes `process.env` explicitly. State the deliberate divergence: real Node calls `setenv`; here any future native reader of the OS env (`os.homedir`/`HOME` #1222, `TZ` #263, `child_process` without `env` #1236) sees the startup snapshot, not JS edits, and must read `process.env` through the prelude instead.
6. Rejected: native Rust `process` object (every Node surface would become Rust; slower iteration); mutating OS env via `set_var` (unsafe, thread-hostile); clap `trailing_var_arg` (strips the first `--`, cannot match Node).
Also add to `CONTEXT.md` terms **Node Host Mode** (the `--node` opt-in environment), **Host Floor** (the `__host_*` native primitives), **Node Prelude** (the JS layer built on the floor; `_Avoid_`: shim once embedded).

### 1. Floor `__host_argv`/argv0/execPath AND CLI splits script args Node-style (one slice: the splitter tests observe `__host_argv()`)
- **Test (red)** `tests/node_host_process.rs` `[new]`: spawn `CARGO_BIN_EXE_jsse` with a temp script printing `JSON.stringify(__host_argv())`:
  - `--node s.js -- a b c --version` -> tail `["--","a","b","c","--version"]`
  - `--node s.js --version`, `--node s.js -e foo --no-bytecode` -> flags verbatim
  - `--node -- s.js -- z` -> `["--","z"]` after script
  - `jsse --version` / `--help` before the script still handled by clap.
- **Code**: (after 1b) in `src/cli.rs`, add `fn split_script_args(argv: Vec<OsString>) -> (Vec<OsString>, Vec<OsString>)`. It scans from argv[1], skipping the value of value-taking options, derived from `Cli::command().get_arguments()` via clap `CommandFactory` (no hand-maintained list; handles `--opt=value`, attached `-eCODE` and clusters like `-me CODE`; unit-test those), consumes a lone leading `--` as option terminator, and the first non-option token is the script; everything after it is script args. With `-e CODE` and no script, the first leftover non-option token and everything after it are script args (Node: `node -e CODE a --b -- c` gives `[node, a, --b, --, c]`). Feed only the head to `Cli::parse_from`; keep the tail in `Cli.script_args` (set after parse, not a clap arg). Drop the first `--` only when it precedes the script (Node rule). Keep `file` positional.
- **Reuses**: `Cli` derive (`src/cli.rs:8`), `Parser::parse_from`.

### 1b. (part of slice 1) Floor: `__host_argv`, execPath, argv0 (implement first, then the splitter)
- **Test (red)**: unit test in `node_host_tests` (`src/interpreter/tests.rs:4427`) that `__host_argv()` is an array of strings after `interp.set_node_host_argv(...)`; floor-off test extended with the new names.
- **Code**: `Interpreter` field `node_host_argv: Option<NodeHostArgv { argv0, exec_path, script: Option<PathBuf>, args }>` (`src/interpreter/mod.rs:456/889`); `pub fn set_node_host_argv(...)`. In `node_host.rs` install `__host_argv()` -> `[execPath, absScript?, ...args]` (script via `std::path::absolute`, no symlink resolution; for `-e`/REPL there is no script entry, matching Node) and `__host_argv0()` -> original `argv[0]`. `execPath` = `std::env::current_exe()` falling back to argv[0]. In `cli.rs::new_interp` call `set_node_host_argv` when `node` is on, capturing `std::env::args_os().next()`.
- **Reuses**: `install_host_global` (`node_host.rs:34`), `create_array` (used at `node_host.rs:208`).

### 3. Floor: env primitives
- **Test (red)**: unit + integration: `__host_env_snapshot()` returns a flat `[k0,v0,k1,v1,...]` matching `std::env::vars_os` (lossy UTF-8; use a Command-set env var in the integration test).
- **Code**: `__host_env_snapshot()` in `node_host.rs`, returned as a flat `[k0, v0, k1, v1, ...]` array (no nested arrays). Read-only; no set/delete primitive (ADR: JS-owned object).

### 4. Floor: `__host_cwd`, `__host_chdir`
- **Test (red)**: integration: after `__host_chdir(tmp)`, `__host_cwd()` equals canonical tmp; `__host_chdir("/nonexistent")` throws an Error with `code === "ENOENT"`, `errno === -2`, `syscall === "chdir"`, `path`, `dest`, message `ENOENT: no such file or directory, chdir <cwd> -> '<arg>'` (exact text diffed against Node by the oracle in step 7).
- **Code**: `std::env::current_dir` / `set_current_dir`; map `io::ErrorKind` + `raw_os_error` to errno/code (`ENOENT`, `ENOTDIR`, `EACCES`); build error with `create_error` then set props.

### 5. Floor: exit-code sink and CLI precedence
- **Test (red)**: integration: `--node -e "__host_set_exit_code(3)"` exits 3; with `__host_set_exit_code(4); throw new Error('x')` exits 1; with `__host_set_exit_code(4); __host_exit(0)` exits 0; `__host_set_exit_code(300)` exits 44; `__host_set_exit_code(3); __host_set_exit_code(undefined)` exits 0 (undefined resets the sink to `None`; add `exitCode=3; exitCode=undefined` to the oracle exit matrix); code survives a drained timer/promise (`setTimeout(()=>__host_set_exit_code(5),5)`).
- **Code**: `Interpreter.host_exit_code: Option<i32>`; `__host_set_exit_code(n)` writes it. `cli.rs::exit_code_from_result` (`:93`): after the `pending_exit` branch, `Ok(())` returns `host_exit_code.map(|c| c as u8 via & 0xff)` else `SUCCESS`; `Err` branches unchanged. Also mirror in the prelude-run path (`run_with_preludes` uses `exit_code_from_result`, so no extra change) and the REPL exit (ignore; Node REPL differs).

### 6. Shim `process` wired to the floor (`scripts/node-shim.js:1090-1135`)
- **Test (red)**: extend `scripts/node-shim.selftest.js` (byte-identical stdout vs Node via `run-node-shim-selftest.sh`) with argv shape/types, `argv[0] === execPath`, env get/set/delete/`in`/enumeration/coercion, `cwd()`/`chdir()`, `exitCode` validation errors (printing `e.code`, `e.name`) — only machine-independent output.
- **Code** (all feature-detected; keep current stubs when the floor is absent so jsse-without-`--node` fallback is unchanged):
  - capture + `delete globalThis.__host_*` for the new primitives (existing pattern at `node-shim.js:34-45`);
  - `argv` = `hostArgv()`, `argv0`, `execPath`;
  - `env`: `Proxy` over a plain object seeded from `hostEnvSnapshot()`: `set` stringifies via `String(v)` (symbol value or key throws `TypeError`), `defineProperty` validates the same way, `deleteProperty` returns true, `ownKeys`/`has`/`get` ordinary. Prototype stays `Object.prototype`;
  - `cwd()`/`chdir(dir)`: `chdir` rejects non-strings with `TypeError [ERR_INVALID_ARG_TYPE]` (set `.code`);
  - `exitCode` accessor property (enumerable, configurable): setter validates (undefined/null -> clear; integer or integer-string accepted; else `ERR_INVALID_ARG_TYPE` / `ERR_OUT_OF_RANGE`) then calls `hostSetExitCode`; `exit(code)`: `if (code !== undefined) process.exitCode = code; hostExit(process.exitCode | 0)`.
  - leave `process.on/once/emit` stubs (#1223/#1229 own `'exit'` events).
- **Reuses**: existing `hostExit` capture (`node-shim.js:31-32`).

### 7. Oracle script `scripts/run-node-process-oracle.sh` + fixtures `[new]`
- Fixtures in `scripts/process-fixtures/*.fixture.js` (`argv`, `env`, `cwd`, `exit-code`), each with header comments `// ARGS: ...`, `// ENV: K=V ...`, `// CWD: tmp|repo`, `// EXPECT-EXIT: n`, printing a deterministic transcript to stdout (machine-specific values normalised: `argv[0]===execPath`, `argv0` basename shape, cwd relative).
- Runner (model on `run-shim-fixtures.sh`): builds the bundle `node-shim.js` + fixture at one temp path, runs the **same path** and the **identical tail** on both: `node B $ARGS` and `jsse --node B $ARGS` (never an extra `--`; put a literal `--` in a fixture's `ARGS:` to test it) with `env -i`-style controlled env and cwd; requires identical stdout and identical exit status; `--no-cross-check`, `--node` flags as in the siblings; missing `node` warns like the siblings. Covers: args with `--`, `--version`, `-e`-looking values; env set/delete/enumerate, non-string coercion; chdir error text; exitCode matrix from Assumptions (rc 3, 7, 44, 255, 1 on throw, exit() vs exit(0)).
- Wire a one-line mention in `scripts/README.md`; not added to CI (siblings are local-only; confirmed no workflow references).

### 8. Docs
- `scripts/README.md`: new runner + real `process` behaviour. `docs/agents/library-tests.md` and `scripts/README.md:98`: `LIB_ENV` note ("not reflected in `process.env`") now false under `--node`; update. `AGENTS.md` source-layout line for `node_host.rs` only if it lists it (it does not); no change.

## Testing
- Unit: `cargo test --profile release-nolto node_host` (floor-off list gets all new `__host_*` names; add ON-path cases).
- Integration: `cargo test --profile release-nolto --test node_host_process`.
- Oracle: `./scripts/run-node-process-oracle.sh`, `./scripts/run-node-shim-selftest.sh`, `./scripts/run-shim-fixtures.sh` (all jsse vs Node).
- Regression: `./scripts/run-library-tests.sh uuid` and `acorn` before/after (the shim now exposes real env/argv to library code).
- Default environment: `uv run python scripts/run-test262.py test262/test/built-ins/global/ test262/test/language/global-code/` targeted, then the full suite once at the end; no baseline update.
- New `test262-extra/`: none (no ECMAScript behavior); host-compat checks belong in `tests/` per AGENTS.md.
- Lint: `./scripts/lint.sh`; `./scripts/ensure-hooks.sh` before committing.

## Regression risk
- test262 baseline: nothing in default mode changes except argv splitting in `cli.rs`. The test262 runner invokes `jsse --prelude ... file`; verify the splitter preserves it (flags before file) and that no runner/agent test passes jsse flags after the file (grep gate above). Run `tests/*.rs` that spawn the binary.
- Shared machinery: no touches to `eval_expr`/`exec_statement`, property MOP, GC, `ObjectKind` matches or bytecode. GC: collection only fires at safepoints, never at allocation, so natives that build a `Vec<JsValue>` and call `create_array` without re-entering JS cannot lose values (precedent: `__host_array_extra_keys`, `node_host.rs:207`). Do not call back into JS mid-build.
- Library harnesses: shim `process.env`/`argv` were fake-stable before; libraries that read `DEBUG`, `NO_COLOR`, `TERM`, `TZ` etc. may now behave differently. Mitigation: run `uuid`, `acorn`, `prismjs` harnesses; the Node-compat harnesses already run those under real Node env anyway.
- Exit-status precedence: `pending_exit` (step budget 124, `process.exit`) must still win; covered by step 5 tests, and the step-budget ADR tests must still pass.
- Memory: build with caps (`CARGO_BUILD_JOBS=4`, `CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16` for iteration; use `release-nolto` for tests; keep scratch under `$TMPDIR`).

## Out of scope
- Embedding the prelude into the binary (#1216), CommonJS loader (#1219), `fs` (#1217), `events`/`process.on('exit')` (#1223/#1229), `process.versions`/`version`/`pid`/`platform` realism and `process.hrtime` changes (#1227), TZ retargeting from `process.env.TZ`, Node-style exit code 1 for syntax errors, REPL exit semantics, `child_process` itself (only the design note), baseline updates, refactors of `cli.rs` beyond the splitter.
