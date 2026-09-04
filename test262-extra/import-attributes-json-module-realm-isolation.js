/*---
description: A JSON module is instantiated per realm, not shared across realms
esid: sec-hostloadimportedmodule
info: |
  Each realm has its own module map, so a JSON module loaded inside a
  ShadowRealm must produce an object whose prototype comes from that realm's
  intrinsics rather than the outer realm's.
flags: [module]
features: [import-attributes, json-modules, ShadowRealm]
---*/

import parsed from './import-attributes-valid-json-dep_FIXTURE.mjs' with { type: 'json' };

assert.sameValue(parsed instanceof Object, true, 'the outer realm parses into its own intrinsics');

const realm = new ShadowRealm();
const check = await realm.importValue(
  './import-attributes-json-module-realm-isolation-dep_FIXTURE.mjs',
  'parsedIsOwnRealmObject'
);

assert.sameValue(check(), true, 'the ShadowRealm parses the JSON module into its own intrinsics');
