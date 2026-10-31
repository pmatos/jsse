/*---
description: >
  A module namespace exotic object answers [[GetOwnProperty]], [[HasProperty]],
  [[Get]], [[Delete]] and [[OwnPropertyKeys]] from its [[Exports]] list: the
  export names sorted by code unit order, with local exports shadowing
  `export *` names, and nothing else (near-miss names are not exports).
info: |
  Module Namespace Exotic Objects

  [[GetOwnProperty]] ( P )
  2. Let exports be O.[[Exports]].
  3. If exports does not contain P, return undefined.
  5. Return PropertyDescriptor { [[Value]]: value, [[Writable]]: true,
     [[Enumerable]]: true, [[Configurable]]: false }.

  [[HasProperty]] ( P )
  3. If exports contains P, return true.

  [[Delete]] ( P )
  3. If exports contains P, return false.
  4. Return true.

  [[OwnPropertyKeys]] ( )
  1. Let exports be O.[[Exports]].
  3. Append all of the String values of exports (in sorted order), then the
     own symbol keys.

  GetModuleNamespace ( module )
  8. Let sortedExports be a List whose elements are the elements of exports,
     sorted according to lexicographic code unit order.
esid: sec-module-namespace-exotic-objects
flags: [module]
---*/

import * as ns from "./module-namespace-export-name-lookup_FIXTURE.mjs";

var expected = ["B", "_", "a", "a b", "ab", "b", "bump", "counter", "default",
  "fromStar", "star", "é"];
assert.compareArray(Object.keys(ns), expected, "keys are the sorted export names");
assert.compareArray(Reflect.ownKeys(ns), expected.concat([Symbol.toStringTag]),
  "ownKeys: sorted names, then @@toStringTag");

expected.forEach(function (name) {
  assert.sameValue(name in ns, true, "has " + name);
  assert.sameValue(Object.prototype.hasOwnProperty.call(ns, name), true, "hasOwn " + name);
  var desc = Object.getOwnPropertyDescriptor(ns, name);
  assert.sameValue(desc.writable, true, name + " writable");
  assert.sameValue(desc.enumerable, true, name + " enumerable");
  assert.sameValue(desc.configurable, false, name + " configurable");
  assert.sameValue(Reflect.deleteProperty(ns, name), false, "delete " + name);
});

["", "A", "c", "abc", "a ", "accented", "é", "fromstar", "toString", "__proto__", "0"].forEach(function (name) {
  var shown = JSON.stringify(name);
  assert.sameValue(name in ns, false, "has " + shown);
  assert.sameValue(Object.getOwnPropertyDescriptor(ns, name), undefined, "descriptor " + shown);
  assert.sameValue(Reflect.deleteProperty(ns, name), true, "delete " + shown);
  assert.sameValue(ns[name], undefined, "get " + shown);
});

assert.sameValue(ns.b, "b", "local export shadows export * of the same name");
assert.sameValue(ns.fromStar, "fromStar", "export * name");
assert.sameValue(ns["é"], "accented", "string-literal export name");
assert.sameValue(ns.star.fromStar, "fromStar", "export * as ns");

assert.sameValue(ns.counter, 0, "initial live binding");
ns.bump();
assert.sameValue(ns.counter, 1, "live binding observed through [[Get]]");
assert.sameValue(Object.getOwnPropertyDescriptor(ns, "counter").value, 1,
  "live binding observed through [[GetOwnProperty]]");
