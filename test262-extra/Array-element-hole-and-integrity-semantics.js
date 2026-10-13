/*---
description: Holes in an Array's element storage are absent properties for [[DefineOwnProperty]] and [[Set]], including on non-extensible Arrays
info: |
  10.4.2.1 Array exotic objects [[DefineOwnProperty]] ( P, Desc )
  10.1.6.3 ValidateAndApplyPropertyDescriptor ( O, P, extensible, Desc, current )
  10.1.9.2 OrdinarySetWithOwnDescriptor ( O, P, V, Receiver, ownDesc )

  An Array hole has no own property: defining it applies the absent-field
  defaults (every attribute false) and is refused on a non-extensible Array.
  Writing `undefined` over an existing element keeps the element, even when
  the Array is non-extensible, and a rejected write of `undefined` to a new
  index must not grow `length` or materialise an element.
esid: sec-array-exotic-objects-defineownproperty-p-desc
includes: []
flags: [noStrict]
features: [Reflect]
---*/

function store(a, i, v) { a[i] = v; }
function storeStrict(a, i, v) { "use strict"; a[i] = v; }

function getter() { return "g"; }

// Defining a hole starts from an absent property, not from a default data property.
(function () {
  var holey = [1, , 3];
  Object.defineProperty(holey, 1, { get: getter, configurable: true });
  var desc = Object.getOwnPropertyDescriptor(holey, 1);
  assert.sameValue(desc.enumerable, false, "accessor defined on a literal hole is not enumerable");
  assert.sameValue(desc.configurable, true, "configurable was requested");

  var deleted = [1, 2, 3];
  delete deleted[1];
  Object.defineProperty(deleted, 1, { get: getter, configurable: true });
  assert.sameValue(
    Object.getOwnPropertyDescriptor(deleted, 1).enumerable,
    false,
    "accessor defined on a deleted element is not enumerable"
  );

  var data = [1, , 3];
  Object.defineProperty(data, 1, { value: 5 });
  desc = Object.getOwnPropertyDescriptor(data, 1);
  assert.sameValue(desc.value, 5, "value");
  assert.sameValue(desc.writable, false, "writable defaults to false");
  assert.sameValue(desc.enumerable, false, "enumerable defaults to false");
  assert.sameValue(desc.configurable, false, "configurable defaults to false");

  var generic = [1, , 3];
  Object.defineProperty(generic, 1, { writable: true });
  desc = Object.getOwnPropertyDescriptor(generic, 1);
  assert.sameValue(desc.value, undefined, "generic descriptor on a hole: value");
  assert.sameValue(desc.enumerable, false, "generic descriptor on a hole: enumerable");
  assert.sameValue(desc.configurable, false, "generic descriptor on a hole: configurable");
})();

// A non-extensible Array refuses to define or assign into a hole.
(function () {
  var a = Object.preventExtensions([1, , 3]);
  assert.throws(TypeError, function () {
    Object.defineProperty(a, 1, { value: 2, writable: true, enumerable: true, configurable: true });
  }, "defineProperty on a hole of a non-extensible Array");
  assert.sameValue(Reflect.defineProperty(a, 1, { get: getter }), false, "Reflect.defineProperty on a hole");
  assert.sameValue(a.hasOwnProperty(1), false, "hole is still absent");

  store(a, 1, "x");
  assert.sameValue(a.hasOwnProperty(1), false, "sloppy write into a hole is ignored");
  assert.throws(TypeError, function () { storeStrict(a, 1, "x"); }, "strict write into a hole");

  var b = [1, , 3];
  Object.setPrototypeOf(b, { 1: "inherited" });
  Object.preventExtensions(b);
  store(b, 1, "x");
  assert.sameValue(b.hasOwnProperty(1), false, "inherited writable data property does not license creation");
  assert.sameValue(b[1], "inherited", "inherited value still visible");
  assert.throws(TypeError, function () { storeStrict(b, 1, "x"); }, "strict write over inherited data property");

  var c = Object.seal([1, , 3]);
  store(c, 1, "x");
  assert.sameValue(c.hasOwnProperty(1), false, "sealed Array hole stays absent");
  assert.sameValue(c.length, 3, "sealed length");

  var f = Object.freeze([1, , 3]);
  assert.sameValue(Reflect.set(f, 1, "x"), false, "frozen Array hole");
  assert.sameValue(f.hasOwnProperty(1), false, "frozen hole stays absent");
})();

// Writing undefined over an existing element keeps it present.
(function () {
  var a = Object.preventExtensions([1, 2, 3]);
  store(a, 1, undefined);
  assert.sameValue(a.hasOwnProperty(1), true, "element survives a write of undefined");
  assert.sameValue(a[1], undefined, "value is undefined");
  assert.sameValue(a.length, 3, "length");
  storeStrict(a, 2, undefined);
  assert.sameValue(a.hasOwnProperty(2), true, "strict write of undefined over an element");
  assert.sameValue(Object.keys(a).join(), "0,1,2", "keys");

  var holey = Object.preventExtensions([1, , 3]);
  storeStrict(holey, 2, undefined);
  assert.sameValue(holey.hasOwnProperty(2), true, "element after a hole survives");
  assert.sameValue(holey.hasOwnProperty(1), false, "the hole stays absent");
})();

// A rejected write of undefined to a new index changes nothing.
(function () {
  var a = Object.preventExtensions([1]);
  store(a, 3, undefined);
  assert.sameValue(a.length, 1, "non-extensible: length untouched");
  assert.sameValue(a.hasOwnProperty(3), false, "non-extensible: no element created");
  assert.throws(TypeError, function () { storeStrict(a, 3, undefined); }, "strict write past the end");
  assert.sameValue(a.length, 1, "non-extensible: length untouched after the strict write");

  var ro = [1, 2, 3];
  Object.defineProperty(ro, "length", { writable: false });
  store(ro, 10, undefined);
  assert.sameValue(ro.length, 3, "read-only length: length untouched");
  assert.sameValue(ro.hasOwnProperty(10), false, "read-only length: no element created");
  assert.throws(TypeError, function () { storeStrict(ro, 10, undefined); }, "strict write past read-only length");

  var frozen = Object.freeze([1, 2]);
  store(frozen, 5, undefined);
  assert.sameValue(frozen.length, 2, "frozen: length untouched");
})();
