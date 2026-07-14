// Fixture for ./source-phase-shallow-load.mjs — an ordinary Source Text Module
// whose own (transitive) dependency does not exist. Source-phase loading must
// NOT resolve/link this dependency: it should surface the source-phase
// SyntaxError for the requested module, not the missing-dependency error.
import { missing } from './source-phase-shallow-nonexistent.mjs';
export const y = missing;
