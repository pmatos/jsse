import * as self from './import-attributes-typed-reexport-namespace-timing-dep_FIXTURE.mjs';

export { default as jsonDefault } from './import-attributes-valid-json-dep_FIXTURE.mjs' with { type: 'json' };

// The namespace above is built while this module is still loading; a typed
// re-export must already be resolvable through it.
export const seenDuringLoad = self.jsonDefault;
