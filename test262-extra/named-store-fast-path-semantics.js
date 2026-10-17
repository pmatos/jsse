/*---
description: >
  A named store `obj.name = value` (the bytecode VM's `SetProp`) behaves as
  OrdinarySet and the exotic [[Set]] methods specify, whether the receiver
  already owns the property or not: setters and read-only properties on the
  prototype chain, accessors, frozen/sealed/non-extensible receivers, Proxies
  on either side, Array `length`, TypedArray, mapped arguments and String
  wrappers all keep their semantics, including after earlier stores to the
  same site populated whatever the engine caches.
esid: sec-ordinary-object-internal-methods-and-internal-slots-set-p-v-receiver
info: |
  OrdinarySetWithOwnDescriptor ( O, P, V, Receiver, ownDesc )
    1. If ownDesc is undefined, then
      a. Let parent be ? O.[[GetPrototypeOf]]().
      b. If parent is not null, return ? parent.[[Set]](P, V, Receiver).
      c. Else, set ownDesc to the PropertyDescriptor { [[Value]]: undefined, [[Writable]]: true, ... }.
    2. If IsDataDescriptor(ownDesc) is true, then
      a. If ownDesc.[[Writable]] is false, return false.
      b. If Receiver is not an Object, return false.
      c. Let existingDescriptor be ? Receiver.[[GetOwnProperty]](P).
      d. If existingDescriptor is not undefined, then ... Return ? Receiver.[[DefineOwnProperty]](P, valueDesc).
      e. Else, return ? CreateDataProperty(Receiver, P, V).
    3. Assert: IsAccessorDescriptor(ownDesc) is true.
    4. Let setter be ownDesc.[[Set]]. If setter is undefined, return false.
    5. Perform ? Call(setter, Receiver, « V »). Return true.
features: [Proxy, TypedArray, Reflect]
flags: [noStrict]
---*/

function setX(o, v) { o.x = v; }
function setXStrict(o, v) { "use strict"; o.x = v; }

// Overwrite and add on a plain object, from one site that sees several shapes.
(function () {
  var o = { x: 1, y: 2 };
  setX(o, 10);
  assert.sameValue(o.x, 10, "overwrite own data property");
  var p = { y: 2 };
  setX(p, 5);
  assert.sameValue(p.x, 5, "add own data property");
  assert.sameValue(Object.keys(p).join(), "y,x", "creation order");
  setX(p, 6);
  assert.sameValue(p.x, 6, "overwrite the property just added");
  assert.sameValue(Object.keys(p).join(), "y,x", "order stable");
  var d = Object.getOwnPropertyDescriptor(p, "x");
  assert.sameValue(d.writable && d.enumerable && d.configurable, true, "default attributes");
})();

// Non-writable own property: ignored sloppy, TypeError strict, value kept.
(function () {
  var o = { x: 1 };
  setX(o, 2);
  Object.defineProperty(o, "x", { writable: false });
  setX(o, 3);
  assert.sameValue(o.x, 2, "sloppy store to a read-only property is ignored");
  assert.throws(TypeError, function () { setXStrict(o, 4); }, "strict store to a read-only property");
  assert.sameValue(o.x, 2, "unchanged");
})();

// Frozen, sealed and non-extensible receivers.
(function () {
  var frozen = Object.freeze({ x: 1 });
  setX(frozen, 2);
  assert.sameValue(frozen.x, 1, "frozen: overwrite ignored");
  assert.throws(TypeError, function () { setXStrict(frozen, 2); }, "frozen: strict overwrite");

  var sealed = Object.seal({ x: 1 });
  setX(sealed, 2);
  assert.sameValue(sealed.x, 2, "sealed: existing property stays writable");
  var sealedNew = Object.seal({});
  setX(sealedNew, 2);
  assert.sameValue(sealedNew.hasOwnProperty("x"), false, "sealed: no new property");
  assert.throws(TypeError, function () { setXStrict(sealedNew, 2); }, "sealed: strict add");

  var noExt = Object.preventExtensions({ y: 1 });
  setX(noExt, 2);
  assert.sameValue(noExt.hasOwnProperty("x"), false, "non-extensible: no new property");
  assert.throws(TypeError, function () { setXStrict(noExt, 2); }, "non-extensible: strict add");
  noExt.y = 5;
  assert.sameValue(noExt.y, 5, "non-extensible: existing property still writable");
})();

// Own accessors.
(function () {
  var log = [];
  var o = {
    get x() { return "got"; },
    set x(v) { log.push(this === o, v); },
  };
  setX(o, 7);
  assert.sameValue(log.join(), "true,7", "own setter receives this and value");
  var getterOnly = { get x() { return 1; } };
  setX(getterOnly, 2);
  assert.sameValue(getterOnly.x, 1, "getter-only: ignored");
  assert.throws(TypeError, function () { setXStrict(getterOnly, 2); }, "getter-only: strict");
})();

// Prototype chain: setters, read-only data, writable data, accessor shadowing.
(function () {
  var seen = [];
  var proto = { set x(v) { seen.push(this, v); } };
  var child = Object.create(proto);
  setX(child, 3);
  assert.sameValue(seen[0], child, "inherited setter gets the receiver");
  assert.sameValue(seen[1], 3, "inherited setter gets the value");
  assert.sameValue(child.hasOwnProperty("x"), false, "no own property created");

  var ro = Object.create(Object.defineProperty({}, "x", { value: 1, writable: false }));
  setX(ro, 2);
  assert.sameValue(ro.hasOwnProperty("x"), false, "inherited read-only blocks the add");
  assert.throws(TypeError, function () { setXStrict(ro, 2); }, "strict");

  var rw = Object.create({ x: 1 });
  setX(rw, 2);
  assert.sameValue(rw.hasOwnProperty("x"), true, "inherited writable data: own property is created");
  assert.sameValue(Object.getPrototypeOf(rw).x, 1, "prototype untouched");

  var getterOnlyProto = Object.create({ get x() { return 1; } });
  setX(getterOnlyProto, 2);
  assert.sameValue(getterOnlyProto.hasOwnProperty("x"), false, "inherited getter-only blocks the add");

  // An own data property shadows an inherited setter.
  var shadow = Object.create(proto);
  Object.defineProperty(shadow, "x", { value: 0, writable: true, configurable: true });
  seen.length = 0;
  setX(shadow, 9);
  assert.sameValue(seen.length, 0, "shadowing data property: setter not called");
  assert.sameValue(shadow.x, 9, "shadowing data property written");
})();

// The prototype chain changes after stores have already happened at the site.
(function () {
  var proto = {};
  var o = Object.create(proto);
  var seen = [];
  for (var i = 0; i < 3; i++) {
    var fresh = Object.create(proto);
    setX(fresh, i);
    assert.sameValue(fresh.x, i, "plain add " + i);
  }
  Object.defineProperty(proto, "x", { set: function (v) { seen.push(v); }, configurable: true });
  var after = Object.create(proto);
  setX(after, 42);
  assert.sameValue(seen.join(), "42", "setter added to the prototype is honoured");
  assert.sameValue(after.hasOwnProperty("x"), false, "no own property");
  Object.defineProperty(proto, "x", { value: 1, writable: false, configurable: true });
  var after2 = Object.create(proto);
  setX(after2, 43);
  assert.sameValue(after2.hasOwnProperty("x"), false, "read-only added to the prototype is honoured");
  delete proto.x;
  var after3 = Object.create(proto);
  setX(after3, 44);
  assert.sameValue(after3.x, 44, "once removed, a plain add works again");

  var swap = Object.create({});
  setX(swap, 1);
  Object.setPrototypeOf(swap, { set x(v) { seen.push("swapped" + v); } });
  var other = Object.create(Object.getPrototypeOf(swap));
  setX(other, 2);
  assert.sameValue(seen[seen.length - 1], "swapped2", "a replaced prototype's setter is honoured");
  assert.sameValue(swap.x, 1, "an own property still wins over the new prototype");
})();

// defineProperty after stores changes what a later store means.
(function () {
  var o = { x: 1 };
  for (var i = 0; i < 3; i++) setX(o, i);
  Object.defineProperty(o, "x", { get: function () { return "g"; }, set: function (v) { o.log = v; }, configurable: true });
  setX(o, "via setter");
  assert.sameValue(o.log, "via setter", "data property redefined as accessor");
  Object.defineProperty(o, "x", { value: 0, writable: true });
  setX(o, 5);
  assert.sameValue(o.x, 5, "accessor redefined as data property");
  Object.defineProperty(o, "x", { writable: false });
  setX(o, 6);
  assert.sameValue(o.x, 5, "data property made read-only");
  delete o.x;
  setX(o, 7);
  assert.sameValue(o.x, 7, "deleted then re-added");
  assert.sameValue(Object.keys(o).join(), "log,x", "re-added property is last in creation order");
})();

// Proxy receivers and Proxies on the prototype chain.
(function () {
  var calls = [];
  var target = {};
  var proxy = new Proxy(target, {
    set: function (t, k, v, r) { calls.push(k, v, r === proxy); return Reflect.set(t, k, v); },
  });
  setX(proxy, 1);
  assert.sameValue(calls.join(), "x,1,true", "Proxy receiver: set trap");
  assert.sameValue(target.x, 1, "forwarded to target");

  calls.length = 0;
  var child = Object.create(proxy);
  setX(child, 2);
  assert.sameValue(calls.join(), "x,2,false", "Proxy on the chain: trap with the child as receiver");
  assert.sameValue(child.hasOwnProperty("x"), false, "the trap wrote to the target, not the receiver");
  assert.sameValue(target.x, 2, "target written by the trap");

  var noTrap = Object.create(new Proxy({ set x(v) { calls.push("target setter"); } }, {}));
  calls.length = 0;
  setX(noTrap, 3);
  assert.sameValue(calls.join(), "target setter", "trap-less Proxy forwards [[Set]] to its target");

  var revoked = Proxy.revocable({}, {});
  revoked.revoke();
  assert.throws(TypeError, function () { setX(revoked.proxy, 1); }, "revoked Proxy");
  var viaRevoked = Object.create(revoked.proxy);
  assert.throws(TypeError, function () { setX(viaRevoked, 1); }, "revoked Proxy on the chain");
})();

// Array: `length` runs ArraySetLength; named properties are ordinary.
(function () {
  function setLength(a, v) { a.length = v; }
  function setFoo(a, v) { a.foo = v; }
  var a = [1, 2, 3];
  setLength(a, 1);
  assert.sameValue(a.length, 1, "length shrinks");
  assert.sameValue(a[1], undefined, "elements removed");
  setLength(a, 4);
  assert.sameValue(a.length, 4, "length grows");
  assert.throws(RangeError, function () { setLength(a, -1); }, "invalid length");
  assert.throws(RangeError, function () { setLength(a, 1.5); }, "fractional length");
  setFoo(a, "f");
  setFoo(a, "g");
  assert.sameValue(a.foo, "g", "named property on an Array");
  assert.sameValue(Object.keys(a).join(), "0,foo", "named property is enumerable and listed");
  Object.defineProperty(a, "length", { writable: false });
  setLength(a, 0);
  assert.sameValue(a.length, 4, "non-writable length rejects the store");
  assert.throws(TypeError, function () { "use strict"; a.length = 0; }, "strict");
  var frozen = Object.freeze([1, 2]);
  setLength(frozen, 0);
  assert.sameValue(frozen.length, 2, "frozen Array");
  var o = {};
  var lenCalls = 0;
  setLength(Object.create({ get length() { return 1; }, set length(v) { lenCalls++; } }), 9);
  assert.sameValue(lenCalls, 1, "an inherited `length` setter on a plain object");
})();

// Names that look like Array indices, on Arrays and plain objects.
(function () {
  var a = [];
  a["0"] = "zero";
  a["1"] = "one";
  assert.sameValue(a.length, 2, "string index keys extend the Array");
  assert.sameValue(a[0], "zero", "and are elements");
  var o = {};
  o["0"] = "z";
  o["01"] = "leading zero";
  assert.sameValue(o[0], "z", "plain object index key");
  assert.sameValue(Object.keys(o).join(), "0,01", "index keys first");
})();

// TypedArray: canonical numeric string keys never create properties.
(function () {
  var ta = new Uint8Array(2);
  function setNamed(t, v) { t.foo = v; }
  setNamed(ta, 1);
  assert.sameValue(ta.foo, 1, "ordinary name on a TypedArray");
  function setInfinity(t, v) { t.Infinity = v; t.NaN = v; }
  setInfinity(ta, 1);
  assert.sameValue(ta.hasOwnProperty("Infinity"), false, "Infinity is a canonical numeric key");
  assert.sameValue(ta.hasOwnProperty("NaN"), false, "NaN is a canonical numeric key");
  ta["0"] = 7;
  assert.sameValue(ta[0], 7, "index key writes the element");
  ta["-0"] = 9;
  assert.sameValue(ta.hasOwnProperty("-0"), false, "-0 is not a valid index");
  var viaProto = Object.create(ta);
  setNamed(viaProto, 2);
  assert.sameValue(viaProto.hasOwnProperty("foo"), true, "TypedArray on the chain: ordinary name is created on the receiver");
  function setNaN(t) { t.NaN = 5; }
  setNaN(Object.create(ta));
})();

// Mapped arguments object.
(function (a, b) {
  function setLength(args, v) { args.length = v; }
  setLength(arguments, 1);
  assert.sameValue(arguments.length, 1, "arguments.length is an ordinary data property");
  arguments["0"] = "changed";
  assert.sameValue(a, "changed", "index key writes through to the mapped parameter");
  arguments.extra = 1;
  assert.sameValue(arguments.extra, 1, "named add");
})(1, 2);

// String wrapper and String.prototype.
(function () {
  var s = new String("ab");
  function setLength(o, v) { o.length = v; }
  setLength(s, 5);
  assert.sameValue(s.length, 2, "length of a String wrapper is read-only");
  assert.throws(TypeError, function () { "use strict"; s.length = 5; }, "strict");
  s.foo = 1;
  assert.sameValue(s.foo, 1, "named add on a String wrapper");
  var viaProto = Object.create(String.prototype);
  setLength(viaProto, 4);
  assert.sameValue(viaProto.hasOwnProperty("length"), false, "String.prototype.length is read-only and inherited");
  var viaWrapper = Object.create(new String("xyz"));
  setLength(viaWrapper, 4);
  assert.sameValue(viaWrapper.hasOwnProperty("length"), false, "wrapper on the chain");
})();

// Primitive receivers.
(function () {
  function setFoo(v, x) { v.foo = x; }
  function setFooStrict(v, x) { "use strict"; v.foo = x; }
  setFoo("str", 1);
  setFoo(1, 1);
  setFoo(true, 1);
  assert.throws(TypeError, function () { setFooStrict("str", 1); }, "strict store on a string primitive");
  assert.throws(TypeError, function () { setFooStrict(1, 1); }, "strict store on a number primitive");
  var seen;
  Object.defineProperty(Number.prototype, "foo", { set: function (v) { seen = [typeof this, v]; }, configurable: true });
  try {
    setFoo(5, 6);
    assert.sameValue(seen.join(), "object,6", "a sloppy setter sees the boxed primitive");
  } finally {
    delete Number.prototype.foo;
  }
  assert.throws(TypeError, function () { setFoo(null, 1); }, "null base");
  assert.throws(TypeError, function () { setFoo(undefined, 1); }, "undefined base");
})();

// `__proto__` is an accessor on Object.prototype, not a data property.
(function () {
  function setProto(o, v) { o.__proto__ = v; }
  var o = {};
  var p = { marker: 1 };
  setProto(o, p);
  assert.sameValue(Object.getPrototypeOf(o), p, "sets [[Prototype]]");
  assert.sameValue(o.hasOwnProperty("__proto__"), false, "no own property");
  var bare = Object.create(null);
  setProto(bare, p);
  assert.sameValue(Object.getPrototypeOf(bare), null, "no setter without Object.prototype");
  assert.sameValue(bare.hasOwnProperty === undefined && Object.getOwnPropertyNames(bare).join(), "__proto__", "own data property instead");
})();

// Class fields and accessors.
(function () {
  class A {
    x = 1;
    set y(v) { this._y = v; }
    get y() { return this._y; }
  }
  var a = new A();
  function setY(o, v) { o.y = v; }
  setY(a, 3);
  assert.sameValue(a.y, 3, "class accessor on the prototype");
  assert.sameValue(a.hasOwnProperty("y"), false, "no own y");
  setX(a, 8);
  assert.sameValue(a.x, 8, "class field is a writable own data property");
  class B extends A { constructor() { super(); this.x = 2; this.z = 3; } }
  var b = new B();
  assert.sameValue(b.x + "," + b.z, "2,3", "stores in a derived constructor");
  Object.freeze(b);
  setX(b, 100);
  assert.sameValue(b.x, 2, "frozen instance");
})();

// Receiver is the global object.
(function () {
  var g = this;
  function setG(o, v) { o.namedStoreFastPathGlobal = v; }
  setG(g, 1);
  setG(g, 2);
  assert.sameValue(globalThis.namedStoreFastPathGlobal, 2, "global object property");
  assert.sameValue(delete globalThis.namedStoreFastPathGlobal, true, "configurable");
})();

// Stores into objects from other realms keep working.
(function () {
  var other = $262.createRealm().global;
  var o = new other.Object();
  setX(o, 1);
  setX(o, 2);
  assert.sameValue(o.x, 2, "cross-realm object");
})();
