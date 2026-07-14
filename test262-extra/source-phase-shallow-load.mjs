// Source-phase imports: source-phase loading is *shallow* — it loads only the
// requested module's source representation and never resolves, links, or
// evaluates the target's dependency graph
// (https://tc39.es/proposal-source-phase-imports/#sec-continuedynamicimport).
// `import.source()` of an ordinary Source Text Module must reject with the
// source-phase SyntaxError even when that module imports a missing/invalid
// dependency — the transitive dependency error must not leak out.
//
// Regression guard for the review feedback on PR #220 (pmatos/jsse#181).
// flags: [module]

let err = null;
try {
  await import.source('./source-phase-shallow-target-dep.mjs');
} catch (e) {
  err = e;
}
if (!(err instanceof SyntaxError)) {
  throw new Error('import.source() of a module with a missing transitive dependency should reject with a SyntaxError (shallow source-phase load), got: ' + err);
}
