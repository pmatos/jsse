# Node host mode: the `module` builtin and source maps

Status: accepted (#1220), epic #1214. Builds on `2026-10-26-1400-node-cjs-loader.md`
and `2026-10-26-1700-node-esm-loader.md`. `import.meta`, `require(esm)` and the
ESM/CommonJS interop (#1241) build on `createRequire` and `Module` from here.

## Decision

1. **`require("module")` / `node:module` is `Module`.** `node-lib/module.js` (one
   `builtin!("module")` line) exports the CommonJS loader's own `Module` after
   adding Node's remaining statics, so `require("module") === module.constructor`
   and a tool that patches `Module._extensions`, `_resolveFilename`, `_load`,
   `_compile` or `_findPath` changes the loader that runs the next `require`.
   Importing it from an ES module gives the default export plus one named export
   per own enumerable static (`import { createRequire } from "node:module"`).
2. **What lives where.** The loader file is at its byte budget, so the statics it
   does not need moved out and it only installs them:
   - `internal/modules/cjs/statics.js`: `wrap`/`wrapper` (non-enumerable
     accessors; assigning `Module.wrap` or `Module.wrapper`, or writing through
     the `wrapper` proxy, marks the loader "patched" and `wrapSafe` then compiles
     `Module.wrap(content)` through indirect eval, as Node does through `vm`),
     `createRequire` (moved from the loader, now Node's algorithm: a URL object,
     `file:` string or absolute path, `ERR_INVALID_ARG_VALUE` otherwise, a plain
     `{ href }` object is rejected as in Node), `_preloadModules`, the
     `isPreloading` getter and `runMain` (`executeUserEntryPoint`). `createRequire`
     is the one #1241 reuses for `import.meta`-style callers.
   - `module.js`: `builtinModules`, `isBuiltin`, `constants`, the compile cache
     functions, `findPackageJSON`, `stripTypeScriptTypes`,
     `syncBuiltinESMExports`, and the source map entries below. These exist after
     the first `require("module")`, not on `module.constructor` before it.
   - `internal/modules/find_package_json.js`: `findPackageJSON(specifier, base)`
     over the ESM resolver (`defaultResolve`) and the cached package.json reader;
     bare specifiers answer the `node_modules/<name>/package.json` they name.
3. **`builtinModules` and `isBuiltin` are Node's list** (66 names, frozen,
   including `node:sea`, `node:sqlite`, `node:test`, `node:test/reporters`), not
   what jsse ships: tools use the list to decide what is external, and
   `isBuiltin("http")` is `true` although `require("http")` is still
   `MODULE_NOT_FOUND` until those modules land.
4. **Compile cache.** jsse compiles nothing to disk. `enableCompileCache()`
   validates its argument like Node and returns `{ status: 3, message }`
   (`constants.compileCacheStatus.DISABLED`, the shape Node returns under
   `NODE_DISABLE_COMPILE_CACHE`); `getCompileCacheDir()` is `undefined`,
   `flushCompileCache()` and `syncBuiltinESMExports()` are no-ops (builtin ESM
   facades are snapshots, see the ESM ADR). `stripTypeScriptTypes` throws
   `ERR_METHOD_NOT_IMPLEMENTED`. `register` and `registerHooks` are absent, so
   feature tests such as `typeof module.register` take their fallback path.
5. **Source maps.** `SourceMap` (`internal/source_map/source_map.js`) is Node's
   class (payload clone, indexed `sections`, VLQ decoding, `findEntry`,
   `findOrigin` with Node's line/column offsets, `lineLengths`).
   `internal/source_map/support.js` holds the flags: `getSourceMapsSupport()` /
   `setSourceMapsSupport(enabled, { nodeModules, generatedCode })` with Node's
   validation, and `process.sourceMapsEnabled` / `process.setSourceMapsEnabled`
   (defined by `scripts/node-builtin-loader.js`, backed lazily by the support
   module, `setSourceMapsEnabled(v)` = `setSourceMapsSupport(v, { nodeModules: v,
   generatedCode: v })`). `findSourceMap(file)` answers for a CommonJS file that
   first appeared in `Module._cache` while the flag was on (and outside
   `node_modules` unless `nodeModules`), by reading its last `//# sourceMappingURL`
   (or `//@`) comment: a `data:application/json[;base64]` URL or a file next to it,
   with `sources` made absolute `file:` URLs and `sourceRoot` reset to `""` as Node
   does. It keeps answering after the flag is turned off, as Node does. The cache
   scan runs when the flags change and when `findSourceMap` is called, not at
   compile time, so a file loaded and deleted from `require.cache` between two
   calls is not tracked.
6. **Known divergences** (none is in the oracle tree):
   - Enabling source maps does not change `error.stack` or uncaught-exception
     output; there is no `--enable-source-maps` flag. Only `findSourceMap` and
     `SourceMap` observe it. `generatedCode` is recorded and ignored.
   - ES modules, `eval` and `new Function` code are never registered with
     `findSourceMap`; an unreadable or malformed map is silently `undefined`
     (Node warns).
   - `Module._stat`, `Module._readPackage`, `register`, `registerHooks`,
     `module.syncBuiltinESMExports` effects and a real compile cache are absent.
   - `Module.runMain()` loads through `Module._load` (CommonJS); it does not
     switch to the ES loader for a `.mjs` main.
   - `findPackageJSON` for a `node:` or non-`file:` specifier or base throws
     `ERR_INVALID_URL_SCHEME` as Node does, but `#imports` specifiers are treated
     as Node treats them (as bare names), so they raise `ERR_MODULE_NOT_FOUND`,
     not Node's odd `node_modules/` path.
7. **Verification.** `scripts/run-node-module-oracle.sh` runs
   `scripts/module-fixtures/proj/main.js` (104 lines: descriptors, names and
   lengths of every static, `builtinModules`/`isBuiltin`, `wrap`/`wrapper`
   patching, `createRequire` forms and errors, `findPackageJSON`, patched
   `_extensions`/`_resolveFilename`/`_load`/`_compile`, `_preloadModules`, ESM
   import) and `sm.js` (84 lines: `SourceMap` lookups on odd inputs, flag validation,
   `findSourceMap` over `proj/sm`) on jsse `--node` and Node and diffs them;
   Node's output is pinned in `expected-{main,sm}.txt`, which `tests/node_module.rs`
   (also under the tree-walker and `JSSE_GC_STRESS`) compares jsse against
   without Node. `tests/node_module.rs` also covers the intentional differences
   (compile cache, `stripTypeScriptTypes`) and a tsc-style startup.
