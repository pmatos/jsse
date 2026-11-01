// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-validateandapplypropertydescriptor
description: >
  Own properties keep their order, values and attributes while an object grows
  through every internal capacity step (inline, linear, tag-indexed, hashed),
  and a property redefined between data and accessor, or deleted and re-added,
  reads back exactly as defined.
info: |
  10.1.6.3 ValidateAndApplyPropertyDescriptor ( O, P, extensible, Desc, current )
    5. If current is a data Property and Desc is an accessor Property descriptor, then
       ... Replace the property named P of object O with an accessor property
       ... Set the [[Get]] and [[Set]] attributes ... to Desc's values or undefined.
    6. Else if current is an accessor Property and Desc is a data Property descriptor, then
       ... Replace the property named P of object O with a data property
       ... Set the [[Value]] and [[Writable]] attributes ... to Desc's values or the default.
  10.1.11.1 OrdinaryOwnPropertyKeys ( O )
    3. For each own property key P of O such that P is a String and P is not an
       array index, in ascending chronological order of property creation, ...
includes: [compareArray.js, propertyHelper.js]
features: [Reflect]
---*/

function getter() { return "g"; }
function setter(v) {}

function build(n) {
  var o = {};
  var keys = [];
  for (var i = 0; i < n; i++) {
    var k = "k" + i;
    keys.push(k);
    switch (i % 5) {
      case 0: o[k] = i; break;
      case 1: Object.defineProperty(o, k, { get: getter, set: setter, enumerable: true, configurable: true }); break;
      case 2: Object.defineProperty(o, k, { value: i, writable: false, enumerable: false, configurable: true }); break;
      case 3: Object.defineProperty(o, k, { get: getter, configurable: true }); break;
      case 4: Object.defineProperty(o, k, { set: setter, enumerable: true, configurable: false }); break;
    }
  }
  return { o: o, keys: keys };
}

function check(o, keys, label) {
  assert.compareArray(Reflect.ownKeys(o), keys, label + ": own keys");
  for (var i = 0; i < keys.length; i++) {
    var d = Object.getOwnPropertyDescriptor(o, keys[i]);
    var at = label + ": " + keys[i];
    switch (i % 5) {
      case 0:
        assert.sameValue(d.value, i, at + " value");
        assert.sameValue(d.writable && d.enumerable && d.configurable, true, at + " attributes");
        break;
      case 1:
        assert.sameValue(d.get, getter, at + " get");
        assert.sameValue(d.set, setter, at + " set");
        assert.sameValue(d.enumerable, true, at + " enumerable");
        break;
      case 2:
        assert.sameValue(d.value, i, at + " value");
        assert.sameValue(d.writable, false, at + " writable");
        assert.sameValue(d.enumerable, false, at + " enumerable");
        break;
      case 3:
        assert.sameValue(d.get, getter, at + " get");
        assert.sameValue(d.set, undefined, at + " set");
        assert.sameValue(Object.prototype.hasOwnProperty.call(d, "set"), true, at + " set present");
        break;
      case 4:
        assert.sameValue(d.get, undefined, at + " get");
        assert.sameValue(d.set, setter, at + " set");
        assert.sameValue(d.configurable, false, at + " configurable");
        break;
    }
  }
}

[0, 1, 2, 3, 4, 5, 8, 9, 10, 11, 13, 14, 17, 21, 28, 33, 64, 65, 70, 130].forEach(function (n) {
  var b = build(n);
  check(b.o, b.keys, "n=" + n);
  assert.sameValue(b.o.k0, n > 0 ? 0 : undefined, "n=" + n + ": plain read");
  if (n > 1) {
    assert.sameValue(b.o.k1, "g", "n=" + n + ": accessor read");
  }
});

// Data to accessor and back, in place, at every position of a 20-key object.
var b = build(20);
for (var i = 0; i < 20; i++) {
  var k = b.keys[i];
  if (i % 5 !== 0 && i % 5 !== 2) continue; // only data properties convert to accessors
  Object.defineProperty(b.o, k, { get: getter, enumerable: true, configurable: true });
  var d = Object.getOwnPropertyDescriptor(b.o, k);
  assert.sameValue(d.get, getter, k + " now accessor");
  assert.sameValue(d.set, undefined, k + " has no setter");
  assert.sameValue("value" in d || "writable" in d, false, k + " has no data fields");
  Object.defineProperty(b.o, k, { value: "v" + i, writable: true });
  d = Object.getOwnPropertyDescriptor(b.o, k);
  assert.sameValue(d.value, "v" + i, k + " back to data");
  assert.sameValue(d.writable, true, k + " writable");
  assert.sameValue("get" in d || "set" in d, false, k + " has no accessor fields");
  assert.sameValue(d.enumerable, true, k + " keeps enumerable");
}
assert.compareArray(Reflect.ownKeys(b.o), b.keys, "redefinition keeps creation order");

// Delete, then re-add: the new property is last.
var o = build(24).o;
delete o.k2;
delete o.k7;
o.k2 = "again";
Object.defineProperty(o, "fresh", { get: getter, enumerable: true, configurable: true });
var expected = [];
for (var i = 0; i < 24; i++) if (i !== 2 && i !== 7) expected.push("k" + i);
expected.push("k2", "fresh");
assert.compareArray(Reflect.ownKeys(o), expected, "order after delete and re-add");
verifyProperty(o, "k2", { value: "again", writable: true, enumerable: true, configurable: true });
assert.sameValue(o.fresh, "g", "accessor added after deletes");

// A value written through [[Set]] keeps the other attributes.
var p = build(12).o;
p.k0 = "w";
assert.sameValue(Reflect.set(p, "k2", "ignored"), false, "[[Set]] of a non-writable data property fails");
assert.sameValue(p.k0, "w", "writable data");
assert.sameValue(p.k2, 2, "non-writable data");
assert.sameValue(Object.getOwnPropertyDescriptor(p, "k2").enumerable, false, "still not enumerable");
