/*---
description: >
  Assigning to an existing writable data property replaces only its value:
  the enumerable and configurable attributes are kept whatever they were, and
  a non-writable property or an accessor is not overwritten by the data path.
esid: sec-ordinarysetwithowndescriptor
info: |
  OrdinarySetWithOwnDescriptor (10.1.9.2) step 2.d.iv redefines an existing
  data property with CreateDataProperty-free `{ [[Value]]: V }` through
  Receiver.[[DefineOwnProperty]], and ValidateAndApplyPropertyDescriptor
  (10.1.6.3) keeps every field the descriptor omits.
includes: [propertyHelper.js]
---*/

var obj = {};
Object.defineProperty(obj, "hidden", { value: 1, writable: true, enumerable: false, configurable: true });
Object.defineProperty(obj, "sealed", { value: 1, writable: true, enumerable: true, configurable: false });
Object.defineProperty(obj, "frozen", { value: 1, writable: false, enumerable: true, configurable: true });
var seen;
Object.defineProperty(obj, "accessor", {
  get: function() { return 7; },
  set: function(v) { seen = v; },
  enumerable: false,
  configurable: true
});

obj.hidden = 2;
verifyProperty(obj, "hidden", { value: 2, writable: true, enumerable: false, configurable: true });

obj.sealed = 2;
verifyProperty(obj, "sealed", { value: 2, writable: true, enumerable: true, configurable: false });

assert.throws(TypeError, function() {
  "use strict";
  obj.frozen = 2;
}, "assignment to a non-writable property throws in strict code");
verifyProperty(obj, "frozen", { value: 1, writable: false, enumerable: true, configurable: true });

obj.accessor = 5;
assert.sameValue(seen, 5, "the setter ran");
assert.sameValue(obj.accessor, 7, "the getter is intact");
var desc = Object.getOwnPropertyDescriptor(obj, "accessor");
assert.sameValue(typeof desc.get, "function", "getter kept");
assert.sameValue(typeof desc.set, "function", "setter kept");
assert.sameValue(desc.enumerable, false, "accessor enumerable kept");

var arr = [1, 2, 3];
Object.defineProperty(arr, "extra", { value: "a", writable: true, enumerable: false, configurable: true });
arr.extra = "b";
verifyProperty(arr, "extra", { value: "b", writable: true, enumerable: false, configurable: true });
arr.length = 1;
assert.sameValue(arr.length, 1, "length assignment truncates");
assert.sameValue(arr[1], undefined, "truncated element is gone");
