/*---
description: >
  Accessors whose whole body is `return this.name;` or `this.name = value;`
  keep sloppy-mode [[Set]] semantics: a failed store is ignored, not thrown.
esid: sec-ordinaryset
info: |
  OrdinarySetWithOwnDescriptor returns false when the receiver cannot take the
  property. The sloppy-mode assignment ignores that result, and the setter body
  `this.name = value` is itself sloppy code here, so it must not throw either.
flags: [noStrict]
---*/

function Box() {}
Object.defineProperty(Box.prototype, "x", {
  get: function () { return this._x; },
  set: function (v) { this._x = v; },
});

var frozen = Object.freeze(new Box());
frozen.x = 1;
assert.sameValue(frozen.x, undefined, "frozen receiver is unchanged");

var closed = Object.preventExtensions(new Box());
closed.x = 1;
assert.sameValue(Object.prototype.hasOwnProperty.call(closed, "_x"), false, "no property is added");

var readOnly = new Box();
Object.defineProperty(readOnly, "_x", { value: 5, writable: false, configurable: true });
readOnly.x = 6;
assert.sameValue(readOnly.x, 5, "read-only value is unchanged");

var getterOnly = { get y() { return 1; } };
getterOnly.y = 2;
assert.sameValue(getterOnly.y, 1, "assignment to a getter-only accessor is ignored");

var thrower = {
  set z(v) { throw new Test262Error("setter ran"); },
};
assert.throws(Test262Error, function () { thrower.z = 1; }, "an exception from a setter propagates");

var sloppySetter = Object.defineProperty({}, "w", {
  set: function (v) { this._w = v; },
  get: function () { return this._w; },
});
sloppySetter.w = 3;
assert.sameValue(sloppySetter.w, 3, "sloppy accessors on an ordinary object");

var strictGetterOnly = (function () {
  "use strict";
  return { get y() { return 1; } };
})();
assert.throws(TypeError, function () {
  "use strict";
  strictGetterOnly.y = 2;
}, "strict assignment to a getter-only accessor throws");
