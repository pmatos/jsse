// Fixture for ./source-phase-reexport.mjs — one of two modules re-exporting
// the same `<module source>` binding as `mod`, star-combined in
// ./source-phase-reexport-star-dep.mjs.
import source mod from '<module source>';
export { mod };
