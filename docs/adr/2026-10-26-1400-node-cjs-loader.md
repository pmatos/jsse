# Node host mode: the CommonJS loader and one builtin registry

Status: accepted (#1219), epic #1214. Builds on
`2026-10-26-0854-node-host-mode.md`. Unblocks the `module` builtin, ESM, tsc and
every test runner.

## Decision

1. **A script file is the CommonJS main module under `--node`.** `jsse --node
   app.js` runs `app.js` as Node does: wrapped in `function (exports, require,
   module, __filename, __dirname)`, `this === module.exports`, top-level `var`
   and function declarations are module-local, a leading `#!` line is ignored,
   `process.argv[1]` is the script and `require.main === module`. The file is
   compiled with the `Function` constructor, which parses the body on its own
   like Node's `compileFunction`, so the body cannot close the wrapper.
   Without `--node` nothing changes (scripts stay global scripts; test262 never
   sees the loader). With `--node` a `.mjs` file, `--module`, or a `.js` file
   whose nearest package.json (not crossing `node_modules`) says
   `"type": "module"` still runs as an ES module and gets no `require`; the
   launcher decides this (`src/cli_node.rs`); it reaches the builtins with
   `import 'node:path'` (`2026-10-26-1700-node-esm-loader.md`). `-e` and the REPL get `require`, `module`,
   `exports`, `__filename` (`[eval]`) and `__dirname` (`.`) as enumerable
   globals, as in Node. **Behaviour change for `--node` users:** library
   bundles and fixtures that relied on script-global semantics now see CommonJS
   ones, which is what the Node cross-check always ran (the uuid library shim
   wraps the module's own `require`).
2. **Launcher contract.** The floor gains `__host_main_kind()`
   (`"cjs"` | `"esm"` | `"eval"`, set by the launcher from the CLI and the package scope). The
   builtin loader (`scripts/node-builtin-loader.js`) captures and deletes it. For
   `"cjs"` it defines the hidden `__node_run_main`; the launcher does not read
   the file but runs the one-statement program
   `Interpreter::node_main_bootstrap()`, which takes that global private and
   calls it. The loader then reads `process.argv[1]`, so a missing main file
   fails with Node's `MODULE_NOT_FOUND` and an uncaught throw is still a
   `Completion::Throw` for the exit-status rules of the host-mode ADR.
3. **One registry behind `require`.** The builtin loader is the last prelude
   script before `hide-globals`, so everything has registered by then. It
   merges `globalThis.__node_builtins` (fs, #1217) and
   `globalThis[Symbol.for("jsse.node.builtins")]` (timers and
   `timers/promises`, #1229) into one private table, deletes both and the old
   `__node_builtin`, and serves the table plus `node-lib/` sources through
   `internal/modules/builtins` (`canBeRequiredByUsers`, `loadBuiltin`). A
   prelude module that wants to be requirable registers itself on either
   object before the builtin loader runs; a pure-JS module adds a
   `builtin!("<id>")` line. `require("internal/...")` and unknown names fall
   through to file resolution (`MODULE_NOT_FOUND`) as in Node;
   `require("node:<unknown or internal>")` is `ERR_UNKNOWN_BUILTIN_MODULE`.
   Node core modules jsse does not ship yet (`http`, ...) are
   `MODULE_NOT_FOUND`, not their Node behaviour, until their issues land.
4. **Layout.** `node-lib/internal/modules/cjs/loader.js` (`Module`, resolution,
   wrapper, `runMain`, `setupEval`), `.../exports.js` (package
   `exports`/`imports`, over absolute paths instead of file URLs) and
   `.../package_json_reader.js` (cached package.json reads). They are derived
   from Node v26.8.2 `lib/internal/modules/` and keep Node's function and error
   names. `internal/errors` gained `ERR_INVALID_ARG_VALUE`,
   `ERR_INVALID_MODULE_SPECIFIER`, `ERR_INVALID_PACKAGE_CONFIG`,
   `ERR_INVALID_PACKAGE_TARGET`, `ERR_PACKAGE_IMPORT_NOT_DEFINED`,
   `ERR_PACKAGE_PATH_NOT_EXPORTED` and `ERR_REQUIRE_ESM`. The `module` builtin
   will export `Module` from the loader.
5. **Resolution** follows Node: builtins first, `#imports`, self reference by
   package name, then the `node_modules` walk with `exports` (conditions
   `require`, `node`, `default`), file, extensions
   (`.js`, `.json`, `.node` in registration order), directory `package.json`
   `main` and `index`. Modules are cached by real path before they run (cycles
   see the partial `exports`), removed again if they throw, and `children`,
   `parent`, `paths`, `require.cache`, `require.main`, `require.resolve(.paths)`
   with `options.paths`, `require.extensions`, `NODE_PATH` and `$HOME/.node_*`
   all exist. `MODULE_NOT_FOUND` carries Node's message and `requireStack`.
6. **Known divergences** (none is in the oracle tree):
   - (Lifted by #1241, see `2026-10-26-2300-node-esm-interop.md`: `require()`
     of an ES module returns its namespace and the `module-sync` condition is
     matched.)
   - `.node` addons throw `ERR_DLOPEN_FAILED`; there is no `--preserve-symlinks`,
     no `module.register`, no source maps, no circular-require warning proxy
     and no deprecation warnings (for example a `main` that falls back to
     `index`).
   - `imports` targets that name another package resolve through the CommonJS
     algorithm, not the ESM one.
   - Module code is compiled with `new Function`, so a syntax error carries no
     file name or line, and a body that closes the synthetic wrapper early is
     accepted instead of rejected (jsse#1320).
   - `require('module')` is the #1220 module
     (`2026-10-26-2100-node-host-module.md`), and only the modules registered in the embedded
     table (and `fs`, `timers` and the other prelude registrations) can be
     required: `buffer` and `process` are globals with no
     requirable module yet (`require("util")` is the #1224 module; the `util` global is
     still the prelude's).
   - The loader needs `fs`, which is `cfg(unix)`, so `--node` script launches
     are unix-only for now (`tests/node_cjs.rs` is too).
   - The prefix global path (`<execPath>/../../lib/node`) is derived from the
     jsse binary, so it differs from Node's.
7. **Verification.** `scripts/run-node-cjs-oracle.sh` runs the fixture tree in
   `scripts/cjs-fixtures/proj` (100 scenarios: builtins and the `node:` prefix,
   relative/absolute/directory/extension resolution, JSON and BOM, wrapper
   semantics, cycles, cache and children, nested `node_modules`, scoped
   packages, `exports` conditions/patterns/arrays/null targets/invalid targets,
   `imports`, self references, `NODE_PATH`, `$HOME/.node_modules`,
   `require.resolve` options, error codes and messages) plus an `-e` driver on
   jsse `--node` and Node and diffs them; Node's output is pinned in
   `expected-{main,eval}.txt`, which `tests/node_cjs.rs` (also under the
   tree-walker and `JSSE_GC_STRESS`) compares jsse against without Node.
