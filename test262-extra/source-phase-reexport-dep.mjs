// Fixture for ./source-phase-reexport.mjs — re-exports a source-phase binding.
// `export { x }` where `x` is bound by `import source` is reclassified to an
// indirect ExportEntry with [[ImportName]] ~source~.
import source x from '<module source>';
export { x };
