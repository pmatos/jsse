// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-proxy-object-internal-methods-and-internal-slots-defineownproperty-p-desc
description: >
  The descriptor object handed to a Proxy defineProperty trap is still alive
  when the invariants are checked after the trap returns, even if the trap ran
  a garbage collection, so an incompatible descriptor still throws a
  TypeError.
info: |
  [[DefineOwnProperty]] ( P, Desc )

  7. Let descObj be FromPropertyDescriptor(Desc).
  9. Let booleanTrapResult be ToBoolean(? Call(trap, handler, « target, P, descObj »)).
  10. If booleanTrapResult is false, return false.
  11. Let targetDesc be ? target.[[GetOwnProperty]](P).
  ...
  16. Else (targetDesc is not undefined),
    a. If IsCompatiblePropertyDescriptor(extensibleTarget, Desc, targetDesc)
       is false, throw a TypeError exception.
features: [Proxy, Reflect, host-gc-required]
---*/

function makeProxy() {
  var target = {};
  Object.defineProperty(target, "prop", { value: 1 });
  var handler = {
    defineProperty: function() {
      $262.gc();
      var churn = [];
      for (var i = 0; i < 500; i++) {
        churn.push({ churn: i });
      }
      return true;
    }
  };
  return new Proxy(target, handler);
}

assert.throws(TypeError, function() {
  Object.defineProperty(makeProxy(), "prop", { value: 2 });
}, "Object.defineProperty checks the invariants after a collecting trap");

assert.throws(TypeError, function() {
  Reflect.defineProperty(makeProxy(), "prop", { value: 2 });
}, "Reflect.defineProperty checks the invariants after a collecting trap");

assert.throws(TypeError, function() {
  Object.defineProperties(makeProxy(), { prop: { value: 2 } });
}, "Object.defineProperties checks the invariants after a collecting trap");

var other = $262.createRealm().global;
var crossRealm = new other.Proxy(
  Object.defineProperty({}, "prop", { value: 1 }),
  {
    defineProperty: function() {
      $262.gc();
      return true;
    }
  }
);

assert.throws(TypeError, function() {
  Object.defineProperty(crossRealm, "prop", { value: 2 });
}, "the cross-realm proxy invariant check also runs");
