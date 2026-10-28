// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-copydataproperties
description: >
  Object-literal spread and object rest from a Proxy source go through the
  proxy's [[OwnPropertyKeys]], [[GetOwnProperty]] and [[Get]]. A handler
  without ownKeys / getOwnPropertyDescriptor traps forwards to the target, so
  every own enumerable target property is copied; the trap order per key is
  getOwnPropertyDescriptor then get.
info: |
  7.3.26 CopyDataProperties ( target, source, excludedItems )
  13.2.5.5 PropertyDefinitionEvaluation (spread)
  14.3.3.2 RestBindingInitialization
  10.5.5 [[GetOwnProperty]] ( P )
includes: [compareArray.js]
features: [Proxy, object-spread, object-rest, Symbol]
---*/

var sym = Symbol("s");

function check(label, obj) {
  assert.compareArray(Object.keys(obj), ["1", "a", "b"], label + " keys");
  assert.sameValue(obj[1], "one", label + " [1]");
  assert.sameValue(obj.a, 1, label + " a");
  assert.sameValue(obj.b, 2, label + " b");
  assert.sameValue(obj[sym], "S", label + " symbol");
  assert.sameValue(Object.prototype.hasOwnProperty.call(obj, "hidden"), false, label + " non-enumerable skipped");
}

function makeTarget() {
  var t = { a: 1, b: 2 };
  t[1] = "one";
  t[sym] = "S";
  Object.defineProperty(t, "hidden", { value: "H", enumerable: false, configurable: true });
  return t;
}

check("empty handler", { ...new Proxy(makeTarget(), {}) });
check("get-only handler", { ...new Proxy(makeTarget(), { get: function (t, k, r) { return Reflect.get(t, k, r); } }) });
check("set-only handler", { ...new Proxy(makeTarget(), { set: function () { return true; } }) });

var restFrom = new Proxy(makeTarget(), {});
var { a: ignored, ...rest } = restFrom;
assert.sameValue(ignored, 1, "rest destructured a");
assert.compareArray(Object.keys(rest), ["1", "b"], "rest keys exclude a");
assert.sameValue(rest[sym], "S", "rest keeps symbol");

var assigned;
({ b: ignored, ...assigned } = new Proxy(makeTarget(), {}));
assert.compareArray(Object.keys(assigned), ["1", "a"], "assignment-pattern rest keys");

var log = [];
var traced = new Proxy({ x: 1, y: 2 }, {
  ownKeys: function (t) { log.push("ownKeys"); return Reflect.ownKeys(t); },
  getOwnPropertyDescriptor: function (t, k) { log.push("gopd:" + k); return Reflect.getOwnPropertyDescriptor(t, k); },
  get: function (t, k, r) { log.push("get:" + k); return Reflect.get(t, k, r); }
});
var copy = { ...traced };
assert.compareArray(log, ["ownKeys", "gopd:x", "get:x", "gopd:y", "get:y"], "trap order");
assert.compareArray(Object.keys(copy), ["x", "y"], "traced copy keys");

var getOnly = [];
var viaGet = { ...new Proxy({ k: 1, l: 2 }, { get: function (t, key, r) { getOnly.push(key); return Reflect.get(t, key, r) * 10; } }) };
assert.sameValue(viaGet.k, 10, "get trap result is copied");
assert.sameValue(viaGet.l, 20, "get trap result is copied (2)");
assert.compareArray(getOnly, ["k", "l"], "get trap called per key");

var nested = { ...new Proxy(new Proxy({ n: 5 }, {}), {}) };
assert.sameValue(nested.n, 5, "proxy of proxy");

var revoked = Proxy.revocable({ r: 1 }, {});
revoked.revoke();
assert.throws(TypeError, function () { ({ ...revoked.proxy }); }, "revoked proxy throws");

var hiddenByTrap = { ...new Proxy({ p: 1, q: 2 }, {
  getOwnPropertyDescriptor: function (t, k) {
    var d = Reflect.getOwnPropertyDescriptor(t, k);
    if (k === "p") d.enumerable = false;
    return d;
  }
}) };
assert.compareArray(Object.keys(hiddenByTrap), ["q"], "trap-reported non-enumerable key skipped");

var fromString = { ...new String("ab") };
assert.compareArray(Object.keys(fromString), ["0", "1"], "String wrapper source");
assert.sameValue(fromString[1], "b", "String wrapper value");

var fromTyped = { ...new Uint8Array([7, 8]) };
assert.compareArray(Object.keys(fromTyped), ["0", "1"], "TypedArray source");
assert.sameValue(fromTyped[1], 8, "TypedArray value");

var proto = { enumerable: true };
var inheritedDesc = { ...new Proxy({ z: 3 }, {
  getOwnPropertyDescriptor: function (t, k) {
    var d = Object.create(proto);
    d.value = t[k];
    d.configurable = true;
    return d;
  }
}) };
assert.sameValue(inheritedDesc.z, 3, "enumerable read through the descriptor's prototype chain");
