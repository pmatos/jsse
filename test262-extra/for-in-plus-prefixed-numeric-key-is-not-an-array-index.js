// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-ordinaryownpropertykeys
description: >
  A String key such as "+1" is not an array index, so it is ordered with the
  non-index String keys by creation order and never collides with "1". This
  holds for [[OwnPropertyKeys]] and for for-in whether or not an enumerable
  property is inherited.
info: |
  10.1.11.1 OrdinaryOwnPropertyKeys ( O )
    2. For each own property key P of O such that P is an array index, in
       ascending numeric index order, do ...
    3. For each own property key P of O such that P is a String and P is not
       an array index, in ascending chronological order of property creation, do ...
includes: [compareArray.js]
---*/

function keysOf(o) {
  var out = [];
  for (var k in o) out.push(k);
  return out;
}

var o = { z: 1 };
o["+1"] = 2;
o["1"] = 3;
o["-1"] = 4;

assert.compareArray(Reflect.ownKeys(o), ["1", "z", "+1", "-1"], "Reflect.ownKeys");
assert.compareArray(Object.keys(o), ["1", "z", "+1", "-1"], "Object.keys");
assert.compareArray(Object.getOwnPropertyNames(o), ["1", "z", "+1", "-1"], "getOwnPropertyNames");
assert.compareArray(keysOf(o), ["1", "z", "+1", "-1"], "for-in, nothing inherited");
assert.compareArray(Object.keys(Object.assign({}, o)), ["1", "z", "+1", "-1"], "Object.assign");
assert.compareArray(Object.keys({ ...o }), ["1", "z", "+1", "-1"], "spread");

var child = Object.create({ inherited: true });
child.z = 1;
child["+1"] = 2;
child["1"] = 3;
assert.compareArray(keysOf(child), ["1", "z", "+1", "inherited"], "for-in, enumerable inherited key");
