/*---
description: A type json request for JavaScript source rejects with the ParseJSONModule error
esid: sec-HostLoadImportedModule
info: |
  If a ModuleRequest has an attribute whose key is "type" and whose value is
  "json", HostLoadImportedModule must finish with either the completion
  returned by ParseJSONModule or a throw completion.
flags: [module]
features: [dynamic-import, import-attributes, json-modules, top-level-await]
---*/

let error = null;
const specifier = './import-attributes-javascript-dep_FIXTURE.mjs';
const options = { with: { type: 'json' } };
try {
  await import(specifier, options);
} catch (caught) {
  error = caught;
}

if (!(error instanceof SyntaxError)) {
  throw new Error(
    'type: "json" on JavaScript source should reject with SyntaxError, got: ' + error
  );
}
