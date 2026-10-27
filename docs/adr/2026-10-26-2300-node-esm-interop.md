# Node host mode: `import.meta`, ESM/CommonJS interop and `require(esm)`

Status: accepted (#1241), epic #1214. Builds on `2026-10-26-1700-node-esm-loader.md`
(the ES module host hook), `2026-10-26-1400-node-cjs-loader.md` and
`2026-10-26-2100-node-host-module.md` (`createRequire`). It lifts the
divergences those ADRs list for named exports of CommonJS modules, `require()`
of ES modules, the `module-sync` condition under `require()` and a typeless
`.js` main with ES syntax.

## Decision

1. **`import.meta`** (`src/interpreter/node_esm/meta.rs`, which moved out of
   `eval.rs`). Without the host hook it is the old object with only `url`.
   With it the engine creates the null-prototype object and calls the host
   function with `("meta", object, filename, isMain)`;
   `node-lib/internal/modules/esm/meta.js` assigns `dirname`, `filename`,
   `main`, `resolve` and `url` in Node's order (`url` is `pathToFileURL`, so a
   space in the path is `%20`). `resolve(specifier)` is Node's
   `createImportMetaResolve`: `defaultResolve(specifier, filename, true)`
   (the new third argument skips the format lookup, so `./x.txt` resolves),
   `error.url` returned for `ERR_MODULE_NOT_FOUND` and
   `ERR_UNSUPPORTED_DIR_IMPORT`, `node:` ids and other URL schemes returned
   as written, a second argument ignored, `length` 2. `main` is true for the
   entry module: the ES main the launcher ran or the script that a CommonJS
   main handed to the loader (item 6).
2. **Importing CommonJS** gives `default` and `module.exports` (both the
   module's `exports`, also when `__esModule` is set) plus the names Node's
   cjs-module-lexer finds. The names are fixed when the record is created
   (`node_esm_cjs_names` calls the host op `names`); the values are a snapshot
   taken after the module ran (`node_esm_execute`): `exports[name]` when it is
   an own property, a throwing getter swallowed, else `undefined`. Later
   changes to `exports` are not seen, ES to ES bindings stay live.
3. **The lexer** is Rust (`builtins/node_host_cjs_lexer.rs` for the scan,
   `node_host_cjs_patterns.rs` for the grammar; floor function
   `__host_cjs_lex(source) -> [exports, reexports]`) because the source can be
   megabytes. It skips comments, strings, templates (with `${}` nesting) and
   regular expressions (a regex-or-division decision from the previous token),
   and recognises, anywhere in the source: `exports.x =`, `exports['x'] =`,
   `module.exports.x =`, `Object.defineProperty(exports|module.exports, 'x', {
   [enumerable: true,] value | get [: function [name]] () { return a[.b|['b']]
   } })`, `module.exports = { a, b: c, 'd': e, ...x, ...require('y') }` (up to
   the first form it cannot read), `module.exports = require('y')` (which
   resets earlier re-exports), `__export(require('y'))`, `__exportStar(...)`
   and Babel's `var _y = require('y'); Object.keys(_y).forEach(function (key)
   {..})` with `default` and `__esModule` skipped. Re-exports are followed by
   `internal/modules/esm/cjs_names.js` through `Module._resolveFilename`
   (builtins and non-`.js`/`.cjs` files skipped, cycles safe). Computed keys,
   `Object.assign(exports, ..)`, arrow getters and other shapes are not
   detected, as in Node. Approximations: the Babel `keys().forEach` body is
   matched loosely (it must mention `default`, `__esModule` and `exports`),
   whitespace is ASCII, an unterminated string stops the scan, and `import` or
   `export` statements do not raise. `scripts/esm-interop-fixtures/proj/lex.mjs`
   pins 54 snippets against Node.
4. **`require()` of an ES module.** `cjs/esm_interop.js` decides the format of
   a `.js` file (`.mjs` and `"type": "module"` are ES, `.cjs` and
   `"type": "commonjs"` CommonJS, a typeless package is compiled as CommonJS
   and retried as ES when that fails with a `SyntaxError` and the source has
   ES syntax). For an ES file it calls `__host_esm_require(filename)`
   (`node_esm/require.rs`): load the graph (the same records `import` uses, so
   both share one evaluation and `require` after `import` returns the same
   namespace), check it, evaluate it, return the namespace. The check walks
   the graph before evaluating anything and answers `"cycle"` when a module is
   mid-evaluation and `"async"` when a module has top-level await; JS throws
   `ERR_REQUIRE_CYCLE_MODULE` or `ERR_REQUIRE_ASYNC_MODULE` (Node's messages,
   `requireStack`), and no module body ran. `module.exports` becomes, as in
   Node: the value of an export named `'module.exports'`; else the namespace
   when it has no `default` or has an own `__esModule`; else a facade. The
   facade is a real namespace over a module compiled from
   `export * from 'original'; export { default } from 'original'; export const
   __esModule = true;` (source held in a thread local read by
   `read_module_text`, `'original'` pre-seeded in the resolution memo), so
   bindings stay live and the object differs from the `import()` namespace.
   A failed load is not cached in `Module._cache`; a module whose body threw
   rethrows the same error. `process.features.require_module` is `true`, and
   `module-sync` is among the `require` conditions (listed order decides).
5. **Layout.** Loader budget: the `ERR_REQUIRE_ESM` branches in
   `cjs/loader.js` became calls into `esm_interop.js`; `meta.js` and
   `cjs_names.js` are new; `internal/host/module` also exports `esmRequire`,
   `esmFacade` and `cjsLex`; the host function gained the ops `meta` and
   `names`. `internal/errors` gained the two `ERR_REQUIRE_*` codes
   (`ERR_REQUIRE_ESM` stays). New Rust hangs off `node_esm.rs` as child
   modules because `mod.rs` and `eval.rs` may not grow. `LoadedModule.error` is
   now a GC root (a failed module's cached error was collected under
   `JSSE_GC_STRESS`).
6. **A typeless `.js` main with ES syntax** (`node app.js` in a package
   without `type`) is detected by the CommonJS loader like any other file; as
   the main module it is handed to `import(file URL)` instead of the
   synchronous path, so top-level await works. An exception reaches the
   uncaught-exception path (exit 1). The launcher still starts it as CommonJS.
7. **Known divergences** (none is in the oracle tree):
   - No `MODULE_TYPELESS_PACKAGE_JSON` warning, no `--trace-require-module`
     output, no `--experimental-print-required-tla`.
   - The uncaught-error report of an ES main has no source line.
   - `ERR_REQUIRE_CYCLE_MODULE` is raised when any module of the required
     graph is mid-evaluation; the message was checked against Node for the
     self-require case only.
   - `Module.runMain()` and `module.register` are unchanged (module ADR).
   - Pre-existing engine issues seen while testing: a module namespace with N
     exports costs O(N^2) to create (jsse#1347: 7,000 exports take 3 s, also
     without `--node`), and `() => {}` followed by a newline and a regular
     expression literal is a `SyntaxError` here but parses in V8 (jsse#1348).
8. **Verification.** `scripts/run-node-esm-interop-oracle.sh` runs
   `scripts/esm-interop-fixtures/proj/{main.mjs,main.cjs,lex.mjs}` (71, 30 and
   54 lines: `import.meta` shape and `resolve` over 23 specifiers,
   `createRequire(import.meta.url)`, 14 CommonJS shapes imported with names,
   default and identity, snapshot versus live bindings, the facade, `module.exports`
   export, live bindings through the facade, async/cycle/throwing/syntax-error
   graphs, `module-sync` ordering, typeless detection) on jsse `--node` and
   Node and diffs them; Node's output is pinned in `expected-*.txt`, which
   `tests/node_esm_interop.rs` compares jsse against without Node, also under
   the tree-walker and `JSSE_GC_STRESS=7`. It covers the
   `createRequire(import.meta.url)` criterion left open by #1220.
