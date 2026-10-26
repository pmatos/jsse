// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-getmodulenamespace
description: >
  The module namespace object a module record caches stays reachable across
  garbage collections even when nothing else references it, so importing the
  module again later returns the same, intact namespace object.
info: |
  GetModuleNamespace ( module )

  3. Let namespace be module.[[Namespace]].
  4. If namespace is empty, then ... Set module.[[Namespace]] to namespace.
  5. Return namespace.

  The module record owns [[Namespace]]; the namespace object must not be
  reclaimed while the record is alive.
flags: [module]
features: [dynamic-import, top-level-await, host-gc-required]
---*/

var keep = [];

async function load() {
  return (await import("./module-namespace-cache-gc-rooting_FIXTURE.mjs")).named;
}

assert.sameValue(await load(), "dyn-named");

for (var round = 0; round < 3; round++) {
  await null;
  $262.gc();
  for (var i = 0; i < 2000; i++) {
    keep.push(Object.create(null));
  }
}

var again = await import("./module-namespace-cache-gc-rooting_FIXTURE.mjs");
assert.sameValue(Object.prototype.toString.call(again), "[object Module]");
assert.sameValue(Object.keys(again).join(), "default,named");
assert.sameValue(again.named, "dyn-named");
