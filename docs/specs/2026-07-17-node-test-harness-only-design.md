# Node test harness `.only` design

## Scope

Make the TAP `describe`/`it` frontend in `scripts/node-test-harness.js` match
Mocha's exclusive-test selection for `describe.only()` and `it.only()`. The
QUnit adapter, engine globals, and ECMAScript implementation are unchanged.

ECMA-262 defines the language but leaves environment-specific objects and
functions outside its scope. `describe` and `it` are test-runner globals, so
Mocha's documented behavior and implementation are the semantic reference;
test262 has no applicable conformance tests for this host-side helper.

## Selection model

Each suite records the tests and immediate child suites registered through an
`.only` function. Registration still stores every child in the existing ordered
`children` list, because a later `.only` must be able to exclude earlier
siblings without changing definition-time behavior.

Immediately before execution, the root is checked recursively for exclusive
entries. If none exist, the tree is left untouched. Otherwise, filter each
suite using Mocha's precedence rules:

1. If a suite has direct exclusive tests, retain only those tests and discard
   all of that suite's child suites.
2. Otherwise, discard that suite's direct tests. Retain direct exclusive child
   suites, filtering them further only when they contain nested exclusive
   entries. Also retain ordinary child suites whose descendants contain an
   exclusive entry, after filtering those descendants.
3. Preserve the retained entries' original definition order.

An exclusive suite with no nested exclusives therefore runs all descendants.
An `it.only` inside that suite narrows it to exclusive tests. Multiple exclusive
entries at the same applicable level form a subset, matching Mocha 3 and newer.

## Skip and hook behavior

Focus determines which tree entries participate in the run; `skip` remains an
orthogonal state on retained entries. A focused test beneath `describe.skip`
is reported as skipped, and its body and hooks do not run. Skipped, unfocused
siblings disappear with other unfocused entries.

Hooks run only on retained suite paths. Existing ancestor hook ordering and
suite-hook failure containment are unchanged.

## Validation

Add a dedicated TAP harness fixture covering:

- ordinary siblings excluded by focused tests and suites;
- multiple `it.only` tests;
- all descendants of `describe.only`;
- nested focus narrowing an exclusive suite;
- direct exclusive tests taking precedence over child suites;
- focus within a skipped suite; and
- hooks on selected and unselected paths.

The fixture must fail under the current aliases and produce a deterministic
summary after the filter is implemented. Run the harness self-test plus the
repository quality gates. Because this patch changes only a prepended library
test shim, the test262 pass count and README baseline should not change.
