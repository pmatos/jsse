/*---
description: A static request with an unsupported type value fails during resolution
esid: sec-HostLoadImportedModule
negative:
  phase: resolution
  type: TypeError
flags: [module]
features: [import-attributes]
---*/

import value from './import-attributes-javascript-dep_FIXTURE.mjs' with { type: 'bogus' };
