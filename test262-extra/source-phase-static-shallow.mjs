// Source-phase imports: the STATIC form (`import source x from '...'`) is
// shallow too — the module-graph pre-load passes must not load/link the
// source-phase target's dependency graph. A static `import source` of an
// ordinary Source Text Module whose transitive dependency is missing must
// fail to link with the source-phase SyntaxError, not the missing-dependency
// error.
//
// Companion to ./source-phase-shallow-load.mjs (which covers the dynamic
// `import.source()` form). Regression guard for PR #220 (pmatos/jsse#181).
// flags: [module]

// Dynamically import a module that statically source-phase-imports a target
// with a missing dependency; its link must fail with a SyntaxError.
let err = null;
try {
  await import('./source-phase-static-importer-dep.mjs');
} catch (e) {
  err = e;
}
if (!(err instanceof SyntaxError)) {
  throw new Error('a static `import source` of a module with a missing transitive dependency should fail to link with a SyntaxError (shallow source-phase load), got: ' + err);
}
