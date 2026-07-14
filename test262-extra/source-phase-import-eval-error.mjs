// Source-phase imports: `import.source()` of an ordinary Source Text Module
// must reject with the source-phase SyntaxError (GetModuleSource, §16.2.1.7.2)
// even after a prior plain dynamic import of the same module evaluated it and
// rejected. ContinueDynamicImport for the source phase never evaluates the
// module or consults [[EvaluationError]]
// (https://tc39.es/proposal-source-phase-imports/#sec-continuedynamicimport),
// so the cached evaluation failure must not leak into the source-phase result.
//
// Regression guard for the review feedback on PR #220 (pmatos/jsse#181).
// flags: [module]

// A plain dynamic import evaluates the module and rejects, caching the
// thrown value as the module's [[EvaluationError]].
let evalErr = null;
try {
  await import('./source-phase-import-eval-error-dep.mjs');
} catch (e) {
  evalErr = e;
}
if (evalErr === null) {
  throw new Error('plain import of a throwing module should reject');
}
if (evalErr instanceof SyntaxError) {
  throw new Error('plain import should reject with the thrown error, not a SyntaxError');
}

// import.source() of the same module must reject with the source-phase
// SyntaxError, NOT replay the cached evaluation error.
let srcErr = null;
try {
  await import.source('./source-phase-import-eval-error-dep.mjs');
} catch (e) {
  srcErr = e;
}
if (!(srcErr instanceof SyntaxError)) {
  throw new Error('import.source() after a cached evaluation error should reject with a SyntaxError, got: ' + srcErr);
}
