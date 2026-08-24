export * as jsonNamespace from './import-attributes-valid-json-dep.mjs' with { type: 'json' };
export { default as jsonDefault } from './import-attributes-valid-json-dep.mjs' with { type: 'json' };

// `export * as ns` creates no local binding, so an unrelated local declaration
// may reuse the exported name. The typed export must not resolve to it.
let jsonNamespace = 'local-shadow';
export const localShadow = jsonNamespace;
