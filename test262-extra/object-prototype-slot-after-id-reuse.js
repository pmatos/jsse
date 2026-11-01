/*---
description: >
  The [[Prototype]] of an object stays the object it was set to, or null, when
  object slots are freed and reused between the writes and the reads.
esid: sec-ordinary-object-internal-methods-and-internal-slots-getprototypeof
info: |
  [[GetPrototypeOf]] returns the value of [[Prototype]]; [[SetPrototypeOf]]
  replaces it. Objects are numbered by arena slot and a collected object's
  slot is handed to the next allocation, so a prototype that was null, was set
  to a later object, or sits behind a long chain must read back unchanged
  after collections that recycle other slots.
features: [host-gc-required]
---*/

var protos = [];
for (var i = 0; i < 50; i++) {
  protos.push({ tag: i });
}

var objs = protos.map(function (p, i) {
  if (i % 3 === 0) return Object.create(p);
  if (i % 3 === 1) return Object.create(null);
  return Object.setPrototypeOf({ i: i }, p);
});

for (var round = 0; round < 3; round++) {
  var junk = [];
  for (var j = 0; j < 3000; j++) {
    junk.push(Object.create({ j: j }));
  }
  junk = undefined;
  $262.gc();
}

objs.forEach(function (o, i) {
  var expected = i % 3 === 1 ? null : protos[i];
  assert.sameValue(Object.getPrototypeOf(o), expected, "prototype of object " + i);
  if (expected) {
    assert.sameValue(o.tag, i, "inherited property " + i);
  } else {
    assert.sameValue("tag" in o, false);
  }
});

var fresh = Object.create(null);
assert.sameValue(Object.getPrototypeOf(fresh), null);
Object.setPrototypeOf(fresh, protos[7]);
assert.sameValue(Object.getPrototypeOf(fresh), protos[7]);
assert.sameValue(fresh.tag, 7);
Object.setPrototypeOf(fresh, null);
assert.sameValue(Object.getPrototypeOf(fresh), null);
assert.sameValue(Reflect.setPrototypeOf(fresh, fresh), false, "an object cannot be its own prototype");

var chain = Object.create(null);
for (var k = 0; k < 200; k++) {
  chain = Object.create(chain);
}
$262.gc();
var depth = 0;
for (var c = chain; c !== null; c = Object.getPrototypeOf(c)) {
  depth++;
}
assert.sameValue(depth, 201, "a 201-object prototype chain ends in null");
