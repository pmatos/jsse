// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
description: >
  JSON.stringify and JSON.parse's reviver walk EnumerableOwnProperties(value,
  key): keys come from [[OwnPropertyKeys]] in spec order, and only string keys
  whose [[GetOwnProperty]] result is enumerable are visited. Ordinary objects
  and exotic objects must agree with what Proxy objects observe.
info: |
  25.5.2.5 SerializeJSONObject ( value )
    5. Else, let K be ? EnumerableOwnProperties(value, key).

  7.3.23 EnumerableOwnProperties ( O, kind )
    1. Let ownKeys be ? O.[[OwnPropertyKeys]]().
    3. For each element key of ownKeys, do
      a. If key is a String, then
        i. Let desc be ? O.[[GetOwnProperty]](key).
        ii. If desc is not undefined and desc.[[Enumerable]] is true, then
features: [Proxy, Symbol, TypedArray, Reflect]
---*/

var sym = Symbol("s");
var mixed = { b: 1, 2: "two", a: 2, 1: "one" };
Object.defineProperty(mixed, "hidden", { value: 3, enumerable: false });
Object.defineProperty(mixed, "getter", { get: function() { return 4; }, enumerable: true });
Object.defineProperty(mixed, "hiddenGetter", { get: function() { return 5; }, enumerable: false });
mixed[sym] = 6;
mixed.late = 7;
assert.sameValue(
  JSON.stringify(mixed),
  '{"1":"one","2":"two","b":1,"a":2,"getter":4,"late":7}',
  "integer keys ascending, then string keys in creation order; non-enumerable and symbol keys skipped"
);

var reconfigured = { x: 1, y: 2, z: 3 };
Object.defineProperty(reconfigured, "y", { enumerable: false });
assert.sameValue(JSON.stringify(reconfigured), '{"x":1,"z":3}', "property made non-enumerable");
Object.defineProperty(reconfigured, "y", { enumerable: true });
assert.sameValue(JSON.stringify(reconfigured), '{"x":1,"y":2,"z":3}', "property made enumerable again");

var deleted = { p: 1, q: 2, r: 3 };
delete deleted.q;
deleted.q = 4;
assert.sameValue(JSON.stringify(deleted), '{"p":1,"r":3,"q":4}', "deleted and re-added key moves to the end");

assert.sameValue(JSON.stringify(new Uint8Array([7, 8])), '{"0":7,"1":8}', "typed array indices are enumerable own keys");

var inherited = Object.create({ proto: 1 });
inherited.own = 2;
assert.sameValue(JSON.stringify(inherited), '{"own":2}', "inherited properties are not serialized");

var log = [];
var observed = new Proxy(
  { a: 1, b: 2, c: 3 },
  {
    ownKeys: function(target) {
      log.push("ownKeys");
      return Reflect.ownKeys(target);
    },
    getOwnPropertyDescriptor: function(target, key) {
      log.push("gopd:" + key);
      var desc = Reflect.getOwnPropertyDescriptor(target, key);
      if (key === "b") desc.enumerable = false;
      return desc;
    }
  }
);
assert.sameValue(JSON.stringify(observed), '{"a":1,"c":3}', "proxy getOwnPropertyDescriptor trap decides enumerability");
assert.sameValue(log.join(), "ownKeys,gopd:a,gopd:b,gopd:c", "proxy trap order");

var passthrough = new Proxy({ m: 1, n: 2 }, {});
Object.defineProperty(passthrough, "n", { enumerable: false });
assert.sameValue(JSON.stringify(passthrough), '{"m":1}', "trap-less proxy defers to target");

var revived = [];
JSON.parse('{"k2":2,"1":1,"k1":3}', function(key, value) {
  revived.push(key);
  return value;
});
assert.sameValue(revived.join(), "1,k2,k1,", "reviver visits integer keys first, then string keys in creation order");

var big = {};
for (var i = 0; i < 2000; i++) {
  big["k" + i] = i;
  if (i % 3 === 0) Object.defineProperty(big, "h" + i, { value: i, enumerable: false });
}
var expected = [];
for (var j = 0; j < 2000; j++) expected.push('"k' + j + '":' + j);
assert.sameValue(JSON.stringify(big), "{" + expected.join(",") + "}", "large object with interleaved hidden keys");
