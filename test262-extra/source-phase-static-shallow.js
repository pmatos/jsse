/*---
description: >
  Static source-phase loading does not resolve an ordinary module's transitive
  dependencies.
features: [source-phase-imports, dynamic-import, top-level-await]
flags: [module]
---*/

// Source-phase imports: the STATIC form (`import source x from '...'`) is
// shallow too — the module-graph pre-load passes must not load/link the
// source-phase target's dependency graph. A static `import source` of an
// ordinary Source Text Module whose transitive dependency is missing must
// fail to link with the source-phase SyntaxError, not the missing-dependency
// error.
//
// Companion to ./source-phase-shallow-load.js (which covers the dynamic
// `import.source()` form). Regression guard for PR #220 (pmatos/jsse#181).

// Dynamically import a module that statically source-phase-imports a target
// with a missing dependency; its link must fail with a SyntaxError.
let err = null;
try {
  await import('./source-phase-static-importer_FIXTURE.mjs');
} catch (e) {
  err = e;
}
assert(
  err instanceof SyntaxError,
  'a static `import source` of a module with a missing transitive dependency should fail to link with a SyntaxError (shallow source-phase load), got: ' + err
);
