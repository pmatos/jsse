/*---
description: Static imports and re-exports honor type json independently of the extension
esid: sec-HostLoadImportedModule
info: |
  Every ModuleRequest carrying `type: "json"` must receive ParseJSONModule's
  completion or a throw completion, including requests created by static
  imports and re-exports.
flags: [module]
features: [import-attributes, json-modules]
---*/

import direct from './import-attributes-valid-json-dep.mjs' with { type: 'json' };
import reexported, { jsonNamespace } from './import-attributes-json-reexport-dep.mjs';

if (
  direct.answer !== 42 ||
  reexported.answer !== 42 ||
  jsonNamespace.default.answer !== 42
) {
  throw new Error('static JSON imports and re-exports should expose the parsed default export');
}
