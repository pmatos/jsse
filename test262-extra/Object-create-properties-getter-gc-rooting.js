// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-object.create
description: >
  The object created by Object.create stays reachable across the garbage
  collection that can happen while ObjectDefineProperties invokes a getter on
  the Properties argument, even though nothing JavaScript-visible refers to
  the new object yet.
info: |
  Object.create ( O, Properties )

  3. Let obj be OrdinaryObjectCreate(O).
  4. If Properties is not undefined, then
    a. Return ? ObjectDefineProperties(obj, Properties).

  ObjectDefineProperties ( O, Properties )

  3. Let keys be ? props.[[OwnPropertyKeys]]().
  5. For each element nextKey of keys, do
    a. Let propDesc be ? props.[[GetOwnProperty]](nextKey).
    b. If propDesc is not undefined and propDesc.[[Enumerable]] is true, then
      i. Let descObj be ? Get(props, nextKey).
      ii. Let desc be ? ToPropertyDescriptor(descObj).
features: [host-gc-required]
---*/

var proto = { inherited: 1 };
var calls = 0;
var props = {};
Object.defineProperty(props, "own", {
  enumerable: true,
  get: function() {
    calls++;
    $262.gc();
    var churn = [];
    for (var i = 0; i < 500; i++) {
      churn.push({ churn: i });
    }
    return { value: 2, enumerable: true };
  }
});

var obj = Object.create(proto, props);

assert.sameValue(calls, 1, "the getter ran once");
assert.sameValue(Object.getPrototypeOf(obj), proto, "the new object keeps its prototype");
assert.sameValue(obj.inherited, 1, "the prototype chain is intact");
assert.sameValue(obj.own, 2, "the described property was defined on the new object");

var descriptorHolder = {};
Object.defineProperty(descriptorHolder, "own", {
  enumerable: true,
  get: function() {
    $262.gc();
    return {
      get value() {
        $262.gc();
        return 3;
      },
      enumerable: true
    };
  }
});

var obj2 = Object.create(proto, descriptorHolder);

assert.sameValue(Object.getPrototypeOf(obj2), proto, "a getter on the descriptor object is also safe");
assert.sameValue(obj2.own, 3, "the descriptor's own getter result was used");
