// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-topropertydescriptor
description: >
  The value read from a descriptor object stays reachable while the getters of
  the descriptor's later fields run a garbage collection, even though it is
  held only by the partially built Property Descriptor record.
info: |
  ToPropertyDescriptor ( Obj )

  7. Let hasValue be ? HasProperty(Obj, "value").
  8. If hasValue is true, then
    a. Let value be ? Get(Obj, "value").
    b. Set desc.[[Value]] to value.
  9. Let hasWritable be ? HasProperty(Obj, "writable").
  10. If hasWritable is true, then
    a. Let writable be ToBoolean(? Get(Obj, "writable")).
features: [host-gc-required]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

var target = {};
Object.defineProperty(target, "p", {
  get value() {
    return { marker: "kept" };
  },
  get writable() {
    collect();
    return true;
  },
  enumerable: true,
  configurable: true
});

assert.sameValue(typeof target.p, "object", "the defined value is an object");
assert.sameValue(target.p.marker, "kept", "the defined value was not freed");

var viaCreate = Object.create({}, {
  q: {
    get value() {
      return { marker: "created" };
    },
    get writable() {
      collect();
      return true;
    },
    enumerable: true
  }
});

assert.sameValue(viaCreate.q.marker, "created", "Object.create keeps the value too");

var viaReflect = {};
Reflect.defineProperty(viaReflect, "r", {
  get get() {
    return function() {
      return "getter";
    };
  },
  get set() {
    collect();
    return undefined;
  },
  configurable: true
});

assert.sameValue(viaReflect.r, "getter", "the accessor's getter survives the set read");
