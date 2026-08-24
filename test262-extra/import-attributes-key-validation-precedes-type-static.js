/*---
description: Static import attribute key validation precedes type value validation
esid: sec-InnerModuleLoading
info: |
  InnerModuleLoading applies AllImportAttributesSupported to the complete
  attribute list before HostLoadImportedModule interprets supported values.
  An unsupported key therefore produces SyntaxError even when an earlier
  `type` attribute also has an unsupported value.
negative:
  phase: resolution
  type: SyntaxError
flags: [module]
features: [import-attributes]
---*/

import value from './import-attributes-javascript-dep_FIXTURE.mjs' with {
  type: 'bogus',
  unsupportedKey: 'value'
};
