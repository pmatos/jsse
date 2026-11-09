/*---
description: >
  Accessors whose whole body is `return this.name;` or `this.name = value;`
  behave exactly like ordinary calls whenever the receiver or the property
  lookup is anything other than a plain object with a plain data property.
esid: sec-ordinaryget
info: |
  OrdinaryGet calls the getter with Receiver as its this value, and
  OrdinarySetWithOwnDescriptor calls the setter with Receiver and V. The body
  `return this.name` performs a further [[Get]] on that this value, and
  `this.name = value` a further [[Set]], so a Proxy, an inherited accessor, a
  read-only property or a non-extensible receiver must still be observed.
flags: [onlyStrict]
---*/

"use strict";

class Box {
  get x() { return this._x; }
  set x(v) { this._x = v; }
}

function plain() {
  var o = new Box();
  for (var i = 0; i < 3; i++) {
    o.x = i;
    assert.sameValue(o.x, i, "stored value is read back");
  }
  assert.sameValue(Object.keys(o).join(), "_x", "_x is an own data property");
}
plain();

var calls = [];
var inherited = Object.create(Box.prototype, {
  _x: { get: function () { calls.push(this); return "viaGetter"; }, configurable: true },
});
assert.sameValue(inherited.x, "viaGetter", "getter of the named property runs");
assert.sameValue(calls[0], inherited, "nested getter sees the original receiver");

var setterLog = [];
var withSetter = Object.create(Box.prototype, {
  _x: { set: function (v) { setterLog.push([this, v]); }, configurable: true },
});
withSetter.x = 7;
assert.sameValue(setterLog.length, 1, "nested setter ran once");
assert.sameValue(setterLog[0][0], withSetter, "nested setter sees the receiver");
assert.sameValue(setterLog[0][1], 7, "nested setter sees the value");

var trapLog = [];
var proxy = new Proxy(new Box(), {
  get: function (target, key, receiver) {
    trapLog.push("get:" + String(key));
    return Reflect.get(target, key, receiver);
  },
  set: function (target, key, value, receiver) {
    trapLog.push("set:" + String(key));
    return Reflect.set(target, key, value, receiver);
  },
});
proxy.x = 1;
assert.sameValue(proxy.x, 1, "value stored through a Proxy receiver");
assert.sameValue(
  trapLog.join(),
  "set:x,set:_x,get:x,get:_x",
  "each access reaches the Proxy traps, including the one made by the body"
);

var frozen = Object.freeze(new Box());
assert.throws(TypeError, function () { frozen.x = 1; }, "frozen receiver rejects the store");

var sealedEmpty = Object.preventExtensions(new Box());
assert.throws(TypeError, function () { sealedEmpty.x = 1; }, "non-extensible receiver cannot gain _x");
assert.sameValue(sealedEmpty.x, undefined, "missing property reads undefined");

var readOnly = new Box();
Object.defineProperty(readOnly, "_x", { value: 5, writable: false, configurable: true });
assert.throws(TypeError, function () { readOnly.x = 6; }, "read-only _x rejects the store");
assert.sameValue(readOnly.x, 5, "read-only value is unchanged");

var getterOnlyUnderneath = Object.create(Box.prototype, {
  _x: { get: function () { return 1; }, configurable: true },
});
assert.throws(TypeError, function () { getterOnlyUnderneath.x = 2; }, "accessor without a setter rejects");

var other = {};
Reflect.set(Box.prototype, "x", 9, other);
assert.sameValue(other._x, 9, "the setter stores on the explicit receiver");
assert.sameValue(Reflect.get(Box.prototype, "x", other), 9, "the getter reads the explicit receiver");
assert.sameValue(Object.prototype.hasOwnProperty.call(Box.prototype, "_x"), false, "prototype untouched");

var missing = new Box();
Object.getOwnPropertyDescriptor(Box.prototype, "x").set.call(missing);
assert.sameValue(Object.prototype.hasOwnProperty.call(missing, "_x"), true, "setter without an argument defines _x");
assert.sameValue(missing._x, undefined, "setter without an argument stores undefined");

var seenThis;
Object.defineProperty(Number.prototype, "boxed", {
  get: function () { "use strict"; seenThis = this; return this.toFixed; },
  configurable: true,
});
assert.sameValue((5).boxed, Number.prototype.toFixed, "primitive receiver reads through the prototype");
assert.sameValue(seenThis, 5, "strict getter sees the primitive this value");
delete Number.prototype.boxed;

Object.defineProperty(Number.prototype, "slot", {
  set: function (v) { "use strict"; this._slot = v; },
  configurable: true,
});
assert.throws(TypeError, function () { (5).slot = 1; }, "strict setter cannot add a property to a primitive");
delete Number.prototype.slot;

var arrowThis = { _x: "arrow" };
var arrowGetter = Object.defineProperty({ _x: "receiver" }, "x", {
  get: () => arrowThis._x,
});
assert.sameValue(arrowGetter.x, "arrow", "an arrow getter keeps its lexical this");

var sloppyHolder = {};
Object.defineProperty(sloppyHolder, "x", {
  get: new Function("return this._x;"),
  set: new Function("v", "this._x = v;"),
});
sloppyHolder.x = "sloppy";
assert.sameValue(sloppyHolder.x, "sloppy", "sloppy accessors on an ordinary object");

var sloppyAny = Object.defineProperty({}, "x", {
  get: new Function("return this._x;"),
  set: new Function("v", "this._x = v;"),
});
assert.sameValue(Reflect.get(sloppyAny, "x", 1), undefined, "sloppy getter on a primitive receiver reads the wrapper");

var order = [];
var tracked = Object.create(Box.prototype);
Object.defineProperty(tracked, "_x", {
  get: function () { order.push("get"); return 1; },
  set: function (v) { order.push("set:" + v); },
  configurable: true,
});
tracked.x = 4;
tracked.x;
assert.sameValue(order.join(), "set:4,get", "accessors named by the body run in order");

class Defaulted {
  set x(v = "default") { this._x = v; }
  get x() { return this._x; }
}
var defaulted = new Defaulted();
defaulted.x = undefined;
assert.sameValue(defaulted.x, "default", "a setter parameter default applies to undefined");
defaulted.x = 3;
assert.sameValue(defaulted.x, 3, "a setter parameter default yields to a value");
