// Fixture for ./source-phase-reexport.mjs — star-re-exports `mod` from two
// modules that both `import source mod from '<module source>'`. ResolveExport
// finds the same [[Module]] + ~source~ from both, so `mod` is unambiguous.
export * from './source-phase-reexport-a-dep.mjs';
export * from './source-phase-reexport-b-dep.mjs';
