/*---
description: >
  An implicit global that is deleted and then defined again as an accessor is
  resolved through the new property, not through the earlier implicit binding.
info: |
  Object Environment Record DeleteBinding
  (sec-object-environment-records-deletebinding-n) calls [[Delete]] on the
  global object, and later resolutions consult HasProperty, [[Get]] and [[Set]]
  of whatever property exists then.
esid: sec-object-environment-records-deletebinding-n
flags: [noStrict]
---*/

var log = [];
d1346 = "first";
assert.sameValue(delete d1346, true, "implicit globals are deletable");
assert.sameValue(typeof d1346, "undefined", "deleted");
assert.throws(ReferenceError, function () { d1346; });

Object.defineProperty(globalThis, "d1346", {
  get: function () { return "getter"; },
  set: function (v) { log.push(v); },
  configurable: true
});
d1346 = "second";
assert.sameValue(log.join(), "second", "setter called after delete and redefine");
assert.sameValue(d1346, "getter");

assert.sameValue(delete d1346, true);
d1346 = "third";
assert.sameValue(d1346, "third", "plain data property again");
assert.sameValue(Object.getOwnPropertyDescriptor(globalThis, "d1346").value, "third");
