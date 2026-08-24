/*---
description: A typed named re-export resolves through a namespace created during module loading
esid: sec-moduledeclarationinstantiation
info: |
  GetModuleNamespace snapshots a module's resolved exports. A re-export
  carrying an import attribute must therefore be resolvable before any
  namespace of the re-exporting module is created, including the namespace a
  self-import builds while the module is still loading.
flags: [module]
features: [import-attributes, json-modules]
---*/

import { jsonDefault, seenDuringLoad } from './import-attributes-typed-reexport-namespace-timing-dep.mjs';

assert.sameValue(jsonDefault.answer, 42, 'the typed re-export exposes the parsed JSON');
assert.notSameValue(seenDuringLoad, undefined, 'the self-import namespace resolved the typed re-export');
assert.sameValue(seenDuringLoad, jsonDefault, 'both views resolve to the same parsed value');
