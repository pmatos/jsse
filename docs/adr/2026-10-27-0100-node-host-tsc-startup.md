# Node host mode: running the TypeScript compiler entry

Status: accepted, epic #1214, issue #1242.

## Decision

1. **No engine or Node-surface change was needed for the startup path.** With the
   modules merged by the other children of #1214 (CommonJS loader, `path`, `os`,
   `fs`, `module`, `perf_hooks`, `crypto`, `util`, `process`, buffers), the real
   TypeScript 6.0.3 `lib/_tsc.js`, `lib/tsc.js` and `bin/tsc` (shebang entry)
   run under `jsse --node` unmodified. `ts.sys` creation, source map support
   probing, locale selection, `process.stdout.write` of the 19 KB `--all` text
   and `process.exit` codes all match Node. The deliverable is the harness that
   keeps it that way.
2. **TypeScript is not vendored.** `TSC_DIR` names an installed `typescript`
   package. The reference run used the npm tarball `typescript-6.0.3.tgz`
   (sha256 `33cd0ee1beaa8c9e9d15a9da836c62ddea4c34a42d7c2d349dbc80d94165d22a`,
   npm integrity `sha512-y2TvuxSZPDyQakkFRPZHKFm+KKVqIisdg9/CZwm9ftvKXLP8NRWj38/ODjNbr43SsoXqNuAisEf1GdCxqWcdBw==`).
   #1250 pins sources later; until then both the oracle and `tests/node_tsc.rs`
   skip with a notice when `TSC_DIR` is unset or not 6.0.3, so CI, which has no
   `TSC_DIR`, never exercises either; run them locally.
3. **The oracle** (`scripts/run-tsc-oracle.sh`) runs each scenario in a fresh copy
   of `scripts/tsc-fixtures/proj` on Node and on jsse and diffs stdout, exit code
   and every file the compiler wrote: `--version`, `-v`, `--help`, `--all`, an
   unknown flag, `--init`, `--showConfig`, a `--noLib` emit and a `--noLib`
   diagnostics run, and the `bin/tsc` shebang entry. `--noLib` with
   `globals.d.ts` keeps those compiles under a second: parsing and binding the
   real `lib.d.ts` set costs about 20 s per run on jsse, which is the subject of
   #1243 (`--full` runs those scenarios anyway). The pinned digests in
   `expected-sha256.txt` (`--pin`, TypeScript 6.0.3 only) let the Rust tests check
   `--version`/`--help`/`--all` without Node.
4. **Startup budget.** After a warm-up the oracle runs `--version` ten times
   through `lib/_tsc.js` and `bin/tsc` and requires each median under
   `TSC_STARTUP_BUDGET_MS` (default 1000) on the release profile; `--no-timing`
   skips it on loaded hosts. Measured on the fat-LTO release binary (host load
   average 14): median 369 ms through `lib/_tsc.js` and 427 ms through `bin/tsc` (min 305 ms); a `--noLib`
   diagnostics compile takes 0.49 s. The cost is parsing and evaluating the 6 MB
   `_tsc.js`; the Rust tests do not assert wall time.
