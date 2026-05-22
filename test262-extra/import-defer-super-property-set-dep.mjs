// Dependency for ./import-defer-super-property-set.mjs.
// Imported via `import defer * as ns from "./...-dep.mjs"`. Listed in
// `run-custom-tests.py`'s skip-as-direct-test list (suffix `-dep.mjs`).

globalThis.evaluations = globalThis.evaluations || [];
globalThis.evaluations.push("dep");

export let exported = 3;
