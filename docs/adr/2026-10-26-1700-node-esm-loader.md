# Node host mode: the ES module loader

Status: accepted (#1240), epic #1214. Builds on `2026-10-26-0854-node-host-mode.md`
and `2026-10-26-1400-node-cjs-loader.md`. Unblocks `import.meta`, ESM/CJS interop
and `require(esm)` (#1241) and the tsc/test-runner work that imports packages.

## Decision

1. **One host hook, the engine keeps the machinery.** Source text parsing,
   linking, the Tarjan evaluation order, top-level await and namespaces stay in
   the interpreter (`src/interpreter/mod.rs`, `eval/modules.rs`). Under
   `--node` the prelude installs one JavaScript function with
   `__host_esm_install`; until then nothing changes, so test262's module tests
   run the old relative/absolute resolution and file loading (the hook state is
   `NodeEsm` in `src/interpreter/node_esm.rs`; every seam checks
   `node_host.esm.is_active()`).
2. **Three operations** (`node-lib/internal/modules/esm/host.js`):
   - `resolve(specifier, parentPath)` answers `[format, target]`, `format` one of
     `module`, `json`, `commonjs`, `builtin`; `target` is a real path, or a
     builtin id. Rust turns it into a `ModuleKey` (a path, or `node:<id>`, which
     `ModuleKey::is_builtin` keeps away from the filesystem) and memoises
     `(referrer, specifier) -> key` plus `key -> (format, target)`. Both
     `resolve_module_specifier` and its `&self` twin `resolve_module_specifier_pure`
     (used for the evaluation order) read that memo; the former fills it.
   - `check(format, target, type)` is Node's import attribute validation
     (`ERR_IMPORT_ATTRIBUTE_MISSING`, `_TYPE_INCOMPATIBLE`, `_UNSUPPORTED`). It
     runs in `load_module_for_type`, once per `(key, type)`.
   - `load(format, target)` returns a builtin's exports, or runs a CommonJS
     module through `Module._load` and returns its `module.exports`.
3. **Resolution** is `node-lib/internal/modules/esm/resolve.js`, derived from
   Node v26.8.2 `lib/internal/modules/esm/resolve.js` and `translators.js`, over
   absolute paths, and reuses the CommonJS loader's `packageExportsResolve`,
   `packageImportsResolve` and package.json reader with the ESM conditions
   `node`, `import`, `module-sync`, `node-addons` (in the order the package's
   `exports` object lists them). Builtins first (`node:` prefix, or a bare name
   `canBeRequiredByUsers`), `file:` URLs, relative and absolute specifiers
   (percent escapes decoded, query and fragment dropped), `#imports`, then the
   `node_modules` walk with self reference, `exports` or `main`/`index`. No
   extension search and no directory index for relative specifiers
   (`ERR_MODULE_NOT_FOUND`, `ERR_UNSUPPORTED_DIR_IMPORT`). Format by extension
   (`.mjs`, `.cjs`, `.json`; `.js` and extensionless by the nearest package
   `type`; anything else `ERR_UNKNOWN_FILE_EXTENSION`). A `.js` file in a
   package with no `type` is `commonjs` unless it fails to compile as a CommonJS
   body and contains `import`/`export` statements or `import.meta` (Node's
   syntax detection, approximated). `internal/errors` gained `ERR_MODULE_NOT_FOUND`,
   `ERR_UNSUPPORTED_DIR_IMPORT`, `ERR_UNKNOWN_FILE_EXTENSION`,
   `ERR_UNSUPPORTED_ESM_URL_SCHEME` and the three `ERR_IMPORT_ATTRIBUTE_*`.
4. **Records for the non-source formats** (`node_esm_synthetic`).
   - `builtin`: evaluated at link time (a builtin has no user-visible side
     effects); exports are `default` (the builtin's `module.exports`) plus each
     own enumerable string key, as Node's builtin facade, as live-bound
     bindings in the record's environment. Later changes to the builtin's
     properties are not reflected (Node's `syncBuiltinESMExports`).
   - `commonjs`: a record with the fixed exports `default` and `module.exports`,
     created at link time and **executed at evaluation time**, in graph order
     (`node_esm_execute`, called from `execute_module_body_sync_in_context`),
     so a CommonJS dependency's side effects interleave with its ES siblings as
     in Node. A throw is cached as the record's error.
   - `json`: the engine's existing typed JSON module; an untyped `.json` import
     is now `ERR_IMPORT_ATTRIBUTE_MISSING` under `--node` (the default mode keeps
     the engine's lenient behaviour).
5. **`import()` referrer.** The engine used `current_module_path`, which for code
   run through the CommonJS loader is the main file. `Expression::Import` now
   calls `dynamic_import_from`, which, only with the hook active, takes the
   referrer from the lexical environment chain first (`Environment::find_module_path`)
   and defers loading of untyped imports to a microtask, so `import(x); log()`
   logs first as in Node (`with { type }` imports load before `import()` returns).
   `import.defer()` and `import.source()` take the same referrer
   (`import_referrer`). The CommonJS wrapper (`new Function(...)`) is tagged with its file
   by `__host_tag_function`, which sets `module_path` on the function's own
   closure environment, so `import('./sibling.mjs')` in a required file, also from
   a timer callback, resolves against that file. `-e` and the REPL have no
   referrer and resolve against the working directory.
6. **The ES main module and the event loop.** The launcher already runs an ES
   main (`cli_node.rs`). New: a main module's top-level await that rejects after
   the first run is an uncaught exception (`process.on('uncaughtException')`,
   else printed, exit 1); a main still unsettled when the loop empties prints a
   warning and exits 13 (`node_host_finish`). `NodeEsm::main` records the entry
   key in `run_program_with_path`.
7. **Layout.** All new Rust is in new files because the touched engine files are
   over the staged-file token limit and may not grow: `node_esm.rs` (state and
   seams), `builtins/node_host_esm.rs` (the two floor functions), and
   `module_host.rs`, into which `resolve_module_specifier`,
   `resolve_module_specifier_pure` and the synthetic default/JSON/text/bytes
   module code moved out of `mod.rs` unchanged (apart from the hook branch).
8. **Known divergences** (none is in the oracle tree):
   - (Lifted by #1241, see `2026-10-26-2300-node-esm-interop.md`: named
     exports of CommonJS modules, `import.meta`, `require(esm)`, and a typeless
     `.js` main with ES syntax, which runs through the ES loader.) Detected
     files get no `MODULE_TYPELESS_PACKAGE_JSON` warning.
   - `data:` URLs throw `ERR_UNSUPPORTED_ESM_URL_SCHEME`; a query or fragment on
     a specifier is dropped, so `./a.mjs?x=1` is the same instance as `./a.mjs`.
   - Import attributes other than `type` (and a `type` other than `json`, `text`
     or `bytes`) are rejected by the engine before the host runs, with its
     message and no `code`; `text`/`bytes` on a file reach the host as
     `ERR_UNKNOWN_FILE_EXTENSION` or `_UNSUPPORTED` as in Node.
   - `ERR_MODULE_NOT_FOUND` carries no "Did you mean to import" hint.
   - Loading is deferred by a microtask, not by file I/O: `import(x)` and a
     `Promise.then` queued right after it order differently from Node.
   - (Lifted by #1241: `require()` matches `module-sync` too.)
   - Module namespaces are not GC roots (jsse#1327, pre-existing): the fixture
     under `JSSE_GC_STRESS` is limited to graphs that do not re-import an
     evaluated module.
   - A typeless `.js` file is classified by compiling it as CommonJS (a leading
     `#!` is tolerated) and, on a SyntaxError, looking for import/export syntax;
     a file that fails only because of a top-level `await` stays CommonJS
     (Node 22+ retries it as ESM).
   - The unsettled-top-level-await warning has no source location; `exit`
     listeners see code 13 as in Node.
   - A resolution with no referrer is never served from the memo, because it
     depends on the working directory.
9. **Verification.** `scripts/run-node-esm-oracle.sh` runs
   `scripts/esm-fixtures/proj/main.mjs` (57 scenarios: static graph evaluation
   order with a CommonJS and a top-level-await sibling, cycles and live
   bindings, builtins as default/named/namespace, JSON attributes, conditional
   exports, patterns, `imports`, self references, scoped packages, every
   resolution error code, dynamic `import()` from ES modules and CommonJS,
   including inside a timer) on jsse `--node` and Node and diffs them; Node's
   output is pinned in `expected-main.txt`, which `tests/node_esm.rs` (also
   under the tree-walker and `JSSE_GC_STRESS`) compares jsse against without
   Node.
