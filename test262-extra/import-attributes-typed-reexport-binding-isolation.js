/*---
description: Typed star- and named-re-export bindings do not alias same-named local declarations
esid: sec-getexportednames
info: |
  ExportEntry Records created by `export * as ns from` and `export { x } from`
  bind no local name in the re-exporting module, so an unrelated local
  declaration may reuse the exported name. ResolveExport must still resolve the
  export to the requested module's value.
flags: [module]
features: [import-attributes, json-modules]
---*/

import { jsonNamespace, jsonDefault, localShadow } from './import-attributes-typed-reexport-binding-isolation-dep_FIXTURE.mjs';

assert.sameValue(localShadow, 'local-shadow', 'the local declaration keeps its own value');
assert.sameValue(typeof jsonNamespace, 'object', 'the star re-export resolves to a namespace');
assert.sameValue(jsonNamespace.default.answer, 42, 'the namespace exposes the parsed JSON');
assert.sameValue(jsonDefault.answer, 42, 'the named re-export resolves to the parsed JSON');
