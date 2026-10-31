// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-wrappedfunctioncreate
description: >
  The wrapped function being created stays reachable while CopyNameAndLength
  runs a getter on the target that triggers a garbage collection.
info: |
  WrappedFunctionCreate ( callerRealm, Target )

  3. Let wrapped be ... a new built-in function object ...
  7. Let result be Completion(CopyNameAndLength(wrapped, Target)).

  CopyNameAndLength ( F, Target [ , prefix [ , argCount ] ] )

  4. Let targetHasLength be ? HasOwnProperty(Target, "length").
  7. Let targetName be ? Get(Target, "name").
includes: [propertyHelper.js]
features: [ShadowRealm, host-gc-required]
---*/

const r = new ShadowRealm();
r.evaluate("(f) => { globalThis.collect = f; }")(function() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
});

const wrapped = r.evaluate(`
function fn(a, b) {}
Object.defineProperty(fn, 'name', {
  get: () => {
    collect();
    return "bar";
  },
  enumerable: false,
  configurable: true,
});
Object.defineProperty(fn, 'length', {
  get: () => {
    collect();
    return 2;
  },
  enumerable: false,
  configurable: true,
});
fn;
`);

assert.sameValue(typeof wrapped, "function", "the wrapped function is a function");
verifyProperty(wrapped, "name", {
  value: "bar",
  enumerable: false,
  writable: false,
  configurable: true,
});
verifyProperty(wrapped, "length", {
  value: 2,
  enumerable: false,
  writable: false,
  configurable: true,
});
