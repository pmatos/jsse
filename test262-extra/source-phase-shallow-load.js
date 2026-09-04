/*---
description: >
  Dynamic source-phase loading does not resolve an ordinary module's
  transitive dependencies.
features: [source-phase-imports, dynamic-import, top-level-await]
flags: [module]
---*/

// Source-phase imports: source-phase loading is *shallow* — it loads only the
// requested module's source representation and never resolves, links, or
// evaluates the target's dependency graph
// (https://tc39.es/proposal-source-phase-imports/#sec-continuedynamicimport).
// `import.source()` of an ordinary Source Text Module must reject with the
// source-phase SyntaxError even when that module imports a missing/invalid
// dependency — the transitive dependency error must not leak out.
//
// Regression guard for the review feedback on PR #220 (pmatos/jsse#181).

let err = null;
try {
  await import.source('./source-phase-shallow-target_FIXTURE.mjs');
} catch (e) {
  err = e;
}
assert(
  err instanceof SyntaxError,
  'import.source() of a module with a missing transitive dependency should reject with a SyntaxError (shallow source-phase load), got: ' + err
);
