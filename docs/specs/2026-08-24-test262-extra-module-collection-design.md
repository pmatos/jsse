# test262-extra Module Collection Design

## Problem

`scripts/run-test262.py` intentionally collects executable `.js` files and
recognizes module tests through test262 frontmatter. Executable `.mjs` files in
`test262-extra/` therefore go unreported and unexecuted, while their imported
`-dep.mjs` files do not follow the runner's existing `_FIXTURE` convention.

## Design

- Rename every executable `test262-extra/*.mjs` test to `.js` and give it real
  test262 frontmatter containing `flags: [module]`.
- Rename every imported `-dep.mjs` module to `_FIXTURE.mjs` and update all
  import specifiers and explanatory comments.
- Keep `find_tests()` focused on `.js` tests. When a selected file or directory
  contains a non-fixture `.mjs` file, raise a collection error that explains
  how to name executable module tests and fixtures.
- Permit `_FIXTURE.mjs` files in selected directories without collecting them.
- Exempt anything under the `test262/` submodule from the guard. It is
  third-party and must never be modified, so a rename is not an available
  remedy there; the guard only polices first-party test directories.
- Teach `scripts/run-custom-tests.py` the same `_FIXTURE` predicate, so both
  runners agree on which module files are fixtures rather than tests.

This uses the existing `compute_scenarios()` and `JsseAdapter` module path,
which passes `--module` and supplies the test262 harness as preludes. It also
matches ECMA-262's model: source text becomes a Source Text Module Record when
parsed with the `Module` goal, while resolving a module specifier to a Module
Record is host-defined and does not depend on a prescribed filename extension.

## Error handling

Collection errors are reported on stderr with exit status 2 before any tests
run. The message lists every offending `.mjs` path and directs authors to use a
`.js` module test or a `_FIXTURE.js`/`_FIXTURE.mjs` dependency (the fixture
predicate accepts either spelling).

## Validation

- Add runner tests for rejecting an explicit non-fixture `.mjs` path, rejecting
  one nested in a selected directory, allowing `_FIXTURE.mjs` dependencies, and
  never aborting on a `.mjs` inside the `test262/` submodule.
- Run the Python runner tests and `test262-extra/` in normal and bytecode modes.
- Run the repository's Rust quality gate and full test262 regression suite.
