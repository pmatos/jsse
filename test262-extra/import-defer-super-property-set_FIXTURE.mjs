// Fixture for ./import-defer-super-property-set.js.
// The test262 runner recognizes the `_FIXTURE.mjs` suffix and does not collect
// this imported module as a standalone test.

globalThis.evaluations = globalThis.evaluations || [];
globalThis.evaluations.push("dep");

export let exported = 3;
