// Node-compatible diagnostics for PutValue (§6.2.5.6) should identify the
// receiver when strict assignment is rejected by an inherited, non-writable
// data property. The ECMAScript-required behavior itself is the TypeError;
// this exact message is covered here as a host-compatibility regression.

Object.defineProperty(Object.prototype, "frozenProp", {
  value: 1,
  writable: false,
  configurable: true,
});

var caught;
(function () {
  "use strict";
  try {
    var receiver = {};
    receiver.frozenProp = true;
  } catch (error) {
    caught = String(error);
  }
}());

delete Object.prototype.frozenProp;

var expected = "TypeError: Cannot assign to read only property 'frozenProp' of object '#<Object>'";
if (caught !== expected) {
  throw new Error("unexpected strict-assignment diagnostic: " + caught);
}
