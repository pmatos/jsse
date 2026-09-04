// Fixture for ./source-phase-reexport.js — the second module re-exporting the
// same `<module source>` binding as `mod` (see ...-a_FIXTURE.mjs). Because both
// resolve to the same [[Module]] + ~source~, the star combination is unambiguous.
import source mod from '<module source>';
export { mod };
