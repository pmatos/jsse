# Node host mode: TypeScript's fourslash runner

Status: accepted, epic #1214, issue #1288 (extends `2026-10-28-1900-node-host-compiler-runner.md`).

## Context

`FourSlashRunner` (`src/testRunner/fourslashRunner.ts`) turns each file of `tests/cases/fourslash` into one mocha
`describe` with one test; `fourslash-server` does the same for `tests/cases/fourslash/server` with a second test per
file for the tsserver log. A test (`src/harness/fourslashImpl.ts`) parses its `////` lines into files, markers and
ranges, builds a virtual language-service host with the lib files of `built/local`, and calls `verify.*`, `edit.*`,
`goTo.*` against an in-process `LanguageService` (or tsserver for the server runner). Baseline-style verifications
compare text with `tests/baselines/reference` and write `tests/baselines/local` on a difference.
`ts-suite/oracle.json` pins 6322 passing fourslash tests and 488 fourslash-server tests, all passing on Node.

## Result

The first 200 tests (`addAllMissingImportsNoCrash.ts` onwards, the runner's sorted enumeration) pass on both
engines, 200/200. The one engine change is in `merge_sort_by` (`src/interpreter/builtins/merge_sort.rs`): the
fourslash harness sorts tooltips with `(x, y) => x.pos > y.pos ? -1 : 1`, which answers 1 for equal keys. The order
such an inconsistent comparator produces is implementation-defined (ECMA-262 SortCompare, `SortIndexedProperties`),
but V8 asks `cmp(later, earlier)` and moves an element before another only on `Less`, and the harness' output
depends on it. The sort now compares in V8's argument order; for a consistent comparator the result is the same
sort as before (still stable, same run reversal), so test262 is unchanged and the default global environment is
untouched. `ts-suite/fourslash-known-failures.txt` is empty. The failing path is exercised by a control run (five tests against corrupted references and edited test files, the
engines must fail them identically).

## Oracle

`scripts/run-ts-fourslash-oracle.py` reuses the compiler oracle rather than copying it: the private working
directory, the mocha invocation, the pinned-count check, the per-test comparison, the local-baseline comparison, the
ratchet and the known-failure file format are the compiler oracle's functions (two parameters were added to them:
the runner configuration and the `oracle.json` runner key). The fourslash-specific part is the slice table
(`scripts/ts_fourslash_slices.py`): a test is a file, a slice picks files by position, name range, shard, name or
regex, and the table already names the ranges of #1289-#1293 (a-c, d-g, h-m, n-r, s-z) so those issues add rows or
fix bugs, not machinery. `--runner fourslash-server` runs the same rows over the server directory (#1293). Tests are
batched (100 per mocha process by default, at most four processes at once over both engines): the runner bundle and
the first lib parse are paid per process, a test costs well under a second afterwards.

## Alternatives

* A generic runner-profile refactor of the compiler oracle: larger diff to an unmerged file for no gain over
  importing its functions; the profile can still be extracted once more runners exist.
* One process per test: 80 s of bundle loading each; rejected.
* Comparing only totals: a swapped failure would hide, so tests are compared by title.
