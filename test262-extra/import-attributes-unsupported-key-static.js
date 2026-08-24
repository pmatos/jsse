/*---
description: A static request with a key outside HostGetSupportedImportAttributes fails resolution
esid: sec-AllImportAttributesSupported
info: |
  The host's stable supported import-attribute key list contains only `type`.
negative:
  phase: resolution
  type: SyntaxError
flags: [module]
features: [import-attributes]
---*/

import value from './import-attributes-javascript-dep.mjs' with { unsupportedKey: 'value' };
