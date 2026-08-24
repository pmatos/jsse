/*---
description: The module source host resource cannot satisfy a type json request
esid: sec-HostLoadImportedModule
info: |
  The `<module source>` host resource has no JSON source text. A request for it
  carrying `type: "json"` must therefore complete abruptly.
flags: [module]
features: [dynamic-import, import-attributes, json-modules, source-phase-imports-module-source, top-level-await]
---*/

let error = null;
const specifier = '<module source>';
const options = { with: { type: 'json' } };
try {
  await import(specifier, options);
} catch (caught) {
  error = caught;
}

if (!(error instanceof TypeError)) {
  throw new Error('a Module Source resource cannot satisfy type: "json"');
}
