# Adding a core module to the Node host

How to make `require('<id>')` (and `import 'node:<id>'`) work under `--node` for a module jsse does not ship yet, or to add a `node-lib/internal/...` helper. Background and layers: [node-host.md](node-host.md); the design of the table is the "embedded builtin modules" addendum of [`docs/adr/2026-10-26-0854-node-host-mode.md`](../adr/2026-10-26-0854-node-host-mode.md). The recipe below was exercised on a throwaway module (a one-line `exports` plus a `builtin!` entry; `require('jsse_probe')`, `require('node:jsse_probe')` and the `internal/` refusal behaved as described).

## The minimal path: JavaScript only

1. **Write `node-lib/<id>.js`.** The file is the *body* of `function (exports, require, module, process, primordials)`, as in Node's bootstrap, so a port of Node's `lib/<id>.js` needs few edits. `<id>` maps to the path: `fs/promises` is `node-lib/fs/promises.js`, `internal/path/posix` is `node-lib/internal/path/posix.js`. Use `primordials` (Node's naming: `ArrayPrototypeJoin`, `ObjectKeys`, `SafeMap`, ...) rather than `Array.prototype.join`, so user code that patches intrinsics cannot break the module. If a name is missing, add it to the table in `scripts/node-builtin-loader.js` (the `primordials` section) instead of using the global. Other builtins are reached with `require('internal/errors')`, `require('internal/validators')`, `require('events')`, ...; `require` inside a builtin resolves builtin ids only.
2. **Register it** with one line in `src/interpreter/builtins/node_host_builtins.rs`:

       builtin!("<id>"),

   The macro `include_str!`s `node-lib/<id>.js` at build time, so the file must exist and the binary must be rebuilt after every edit of any `node-lib/` file. A missing file is a compile error, which is the check that the id and the path agree.
3. **Nothing else for a public id.** The loader (`scripts/node-builtin-loader.js`) evaluates the module with indirect `eval` the first time it is required, so a launch that never touches it pays nothing. `require('<id>')` and `require('node:<id>')` both work; any id starting `internal/` is refused to user code (`MODULE_NOT_FOUND`, as in Node; `--expose-internals` loads one by hand while debugging).
4. **`module.builtinModules`.** `node-lib/module.js` lists Node's public ids (`publicIds`), not what jsse ships, so an id Node already documents is there. Edit it only for an id Node itself added.

Keep every file under 20,000 bytes (a hook enforces it): split a big port into `internal/<area>/...` files, as `fs`, `stream` and `internal/path/win32_a`/`win32_b` do. Record in the file header which Node version and file it derives from (the existing ports say `Node v26.8.2`, with the MIT licence text kept where Node's is).

## When JavaScript is not enough: a floor primitive

Add native code only for a syscall or an engine internal JavaScript cannot reach.

1. Create `src/interpreter/builtins/node_host_<area>.rs` with a `setup_node_host_<area>` method that creates each function with `JsFunction::native("__host_<name>", arity, ...)` and installs it with `self.install_host_global("__host_<name>", f)` (non-enumerable, configurable).
2. Declare the module in `src/interpreter/builtins/mod.rs` (`mod node_host_<area>;`) and call the setup method from `setup_node_host` in `node_host.rs` (inside the `#[cfg(unix)]` block when it needs `libc`).
3. Take the primitive private in `scripts/node-builtin-loader.js`: capture the global, `delete` it, and seed an object as `cache["internal/host/<area>"]` (see `internal/host/os` or `internal/host/util` there). The module then does `require('internal/host/<area>')`. User code never sees a `__host_*` name; `tests/node_prelude.rs` and `host_globals_absent_when_floor_off` (`src/interpreter/tests.rs`) are the guards to extend if you add a probe name.
4. Errors follow Node's shape (`code`, `errno`, `syscall`, `path`); build them with the helpers of `node_host_fs_error.rs` or in JavaScript through `internal/errors` (`E()` registers a code; add the codes your module throws).

## A new web global

Add a line `["<builtin id>", "<GlobalName>"]` to `LAZY_GLOBALS` in the loader. It becomes a non-enumerable accessor that loads the module on first touch and then settles into a writable data property, as in Node. Do not define globals from a module's top level.

## A new prelude script (rare)

Only for something that must exist before user code runs and cannot be lazy (`process`, timers, the `fs` support scripts). Add `scripts/node-<name>.js` and an `include_str!` entry in `NODE_PRELUDE` (`src/interpreter/builtins/node_prelude.rs`) at the position its dependencies require; the list is the load order. A prelude script that publishes a public module registers it on `globalThis.__node_builtins.<id>` (or `globalThis[Symbol.for("jsse.node.builtins")]`) and the loader merges and deletes the registries.

## Tests, oracle, docs: what a module PR carries

- **Rust guard** `tests/node_<area>.rs`: spawn `env!("CARGO_BIN_EXE_jsse")` with `--node` (`tests/node_url.rs` and `tests/node_prelude.rs` are small templates). Check the opt-in gate too: without `--node` the module and its globals do not exist. Run it with `cargo test --profile release-nolto --test node_<area>` (never `cargo test --release`, which relinks with fat LTO).
- **Oracle**: a `scripts/run-node-<area>-oracle.sh` that runs `scripts/<area>-fixtures/` on `jsse --node` and Node and diffs stdout and exit status, modelled on `scripts/run-node-prelude-oracle.sh`. Fixtures must be deterministic (no timestamps or pids in the output). Port Node's own `test/parallel/test-<area>-*.js` verbatim where you can (as the `path`, `events` and `buffer` oracles do) and record in the fixture manifest any test skipped on jsse with the issue that blocks it.
- **ADR** `docs/adr/YYYY-MM-DD-HHMM-node-host-<area>.md` (UTC; naming in [`docs/adr/README.md`](../adr/README.md)), linked from the list in [node-host.md](node-host.md); the main host-mode ADR is size-capped and takes no additions.
- **Library inventory**: one bullet for the oracle in [library-tests.md](library-tests.md) (or in the TypeScript pages when the module exists for tsc).
- **Both engines**: run the guard with `--no-bytecode` as well when behaviour depends on frames, stacks or `arguments`, and under `JSSE_GC_STRESS=3` when the module holds objects across native calls (`tests/node_wasm.rs` shows the pattern).
- **test262 untouched**: after the change `uv run python scripts/run-test262.py test262/test/built-ins/Object/` must stay green; the full baseline is judged by CI.
