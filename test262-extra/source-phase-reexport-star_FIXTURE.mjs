// Fixture for ./source-phase-reexport.js — star-re-exports `mod` from two
// modules that both `import source mod from '<module source>'`. ResolveExport
// finds the same [[Module]] + ~source~ from both, so `mod` is unambiguous.
export * from './source-phase-reexport-a_FIXTURE.mjs';
export * from './source-phase-reexport-b_FIXTURE.mjs';
