/*---
description: A type json request parses JSON independently of the resource extension
esid: sec-HostLoadImportedModule
info: |
  HostLoadImportedModule must use ParseJSONModule for a ModuleRequest carrying
  the `type: "json"` attribute, even when the resource does not end in `.json`.
flags: [module]
features: [dynamic-import, import-attributes, json-modules, top-level-await]
---*/

const specifier = './import-attributes-valid-json-dep_FIXTURE.mjs';
const options = { with: { type: 'json' } };
const namespace = await import(specifier, options);
if (namespace.default.answer !== 42) {
  throw new Error('expected the JSON module default export to contain answer: 42');
}
