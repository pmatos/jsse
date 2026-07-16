# Luxon library harness design

## Scope

Add Luxon as one reproducible real-world library suite under issue #235. This
PR intentionally excludes Moment because the issue requires one library per PR.
Pin Luxon to 3.7.2, Jest's assertion package to 29.7.0, and both engine runs to
`TZ=America/New_York`, matching Luxon's own test script.

## Approach

Luxon's 58 test files use Jest globals, but their runner surface is limited to
`test`, `describe`, and five `test.each` tables. Bundle Jest's real `expect`
package so matcher behavior is not reimplemented. Extend the shared in-process
TAP runner with `test.each`, then generate a static entry importing every
`*.test.js` file so esbuild can bundle the suite without filesystem access at
runtime.

The shared runner will support an explicit force marker for suites whose native
CLI cannot run from a single bundle. A small prelude sets that marker before the
runner loads, allowing the same TAP runner and real `expect` implementation to
execute on both jsse and Node. Existing suites retain the current behavior:
without the marker, the shared runner remains inert on Node and their native
framework remains the reference oracle.

The generalized library runner will accept an ordered list of optional shims
and an environment array. Existing singular `LIB_SHIM` configurations remain
compatible. Luxon uses the marker prelude followed by the shared test harness,
and `LIB_ENV=(TZ=America/New_York)`.

## Data flow

1. Clone the pinned Luxon tag.
2. Replace the development dependency set with only pinned `expect`.
3. Generate a deterministic entry containing `expect` setup and sorted static
   imports of all Luxon test files.
4. Bundle the entry and its dependencies with pinned esbuild.
5. Prepend the host shims, force marker, and shared test harness.
6. Run the identical final bundle under the pinned environment on jsse and
   Node.
7. Require both runs to pass and report the pinned expanded test count.

## Failure handling

The harness treats a thrown Jest matcher error as a failed TAP test and emits
its stack for triage. A missing summary, nonzero failure count, timeout,
unexpected test count, or disagreement between engines fails the library
runner. Engine defects discovered by the suite will be reduced and tracked as
separate issues rather than hidden by broad compatibility shims.

## Verification

- Run the shared harness self-test to cover existing behavior and new
  `test.each` expansion.
- Run Luxon on Node alone, then on jsse with the Node count cross-check.
- Run formatting, linting, release build, release tests, targeted Date and
  Intl.DateTimeFormat test262 suites, and the full test262 regression check.
- Update the library harness documentation with the pinned count and result.
