// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-proxy-object-internal-methods-and-internal-slots-getownproperty-p
description: >
  The object returned by a getOwnPropertyDescriptor trap stays alive while the
  proxy target's own [[GetOwnProperty]] and [[IsExtensible]] run user code that
  collects garbage.
features: [Proxy]
includes: [compareArray.js]
---*/

var t = {};
Object.defineProperty(t, "k", { value: 1, writable: true, enumerable: true, configurable: true });

var inner = new Proxy(t, {
  getOwnPropertyDescriptor(target, key) {
    $262.gc();
    return Reflect.getOwnPropertyDescriptor(target, key);
  },
  isExtensible(target) {
    $262.gc();
    return Reflect.isExtensible(target);
  }
});

var outer = new Proxy(inner, {
  getOwnPropertyDescriptor() {
    return { value: 1, writable: true, enumerable: true, configurable: true };
  }
});

for (var i = 0; i < 5; i++) {
  var d = Object.getOwnPropertyDescriptor(outer, "k");
  assert.sameValue(d.value, 1, "value");
  assert.sameValue(d.writable, true, "writable");
  assert.sameValue(d.enumerable, true, "enumerable");
  assert.sameValue(d.configurable, true, "configurable");
}
