/*---
description: A static type json request for JavaScript source fails during resolution
esid: sec-HostLoadImportedModule
info: |
  A ModuleRequest carrying `type: "json"` must receive ParseJSONModule's
  completion or a throw completion, regardless of the resource extension.
negative:
  phase: resolution
  type: SyntaxError
flags: [module]
features: [import-attributes, json-modules]
---*/

import value from './import-attributes-javascript-dep.mjs' with { type: 'json' };
