/*---
description: Typed re-exports of one resource preserve the shared target binding identity
esid: sec-resolveexport
info: |
  Indirect exports through two intermediary modules resolve to the same
  synthetic module and binding. A module which star-exports both intermediaries
  must therefore not treat the shared export as ambiguous.
flags: [module]
features: [import-attributes, json-modules]
---*/

import { named } from './import-attributes-typed-reexport-shared-named-aggregator_FIXTURE.mjs';
import { namespace } from './import-attributes-typed-reexport-shared-namespace-aggregator_FIXTURE.mjs';
import direct from './import-attributes-valid-json-dep_FIXTURE.mjs' with { type: 'json' };

assert.sameValue(named, direct, 'named re-exports share the synthetic default binding');
assert.sameValue(namespace.default, direct, 'namespace re-exports share the synthetic module');
