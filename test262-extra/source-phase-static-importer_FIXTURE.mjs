// Fixture for ./source-phase-static-shallow.js — statically source-phase
// imports a Source Text Module whose own dependency is missing. Static
// source-phase loading must be shallow (not load the target's dependency
// graph), so linking this module fails with the source-phase SyntaxError,
// NOT the target's missing-dependency error.
import source x from './source-phase-shallow-target_FIXTURE.mjs';
export const value = x;
