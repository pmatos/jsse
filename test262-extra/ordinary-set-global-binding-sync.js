/*---
description: >
  A successful setter call is not an OrdinarySet data-property write and must
  not mirror the assignment value into a same-named global lexical binding.
info: |
  OrdinarySetWithOwnDescriptor (sec-ordinarysetwithowndescriptor) returns true
  both after Receiver.[[DefineOwnProperty]] writes a data property and after an
  accessor setter is called. A setter can also change the property's descriptor
  before it returns, so the resulting descriptor does not identify which path
  handled the assignment.
esid: sec-ordinarysetwithowndescriptor
---*/

let noOpSetterLexical = 0;
Object.defineProperty(globalThis, "noOpSetterLexical", {
  configurable: true,
  set: function () {},
});

globalThis.noOpSetterLexical = 7;

assert.sameValue(
  noOpSetterLexical,
  0,
  "a no-op setter must not mirror its argument into the lexical binding"
);
delete globalThis.noOpSetterLexical;

let redefiningSetterLexical = 0;
Object.defineProperty(globalThis, "redefiningSetterLexical", {
  configurable: true,
  set: function () {
    Object.defineProperty(globalThis, "redefiningSetterLexical", {
      configurable: true,
      value: 99,
      writable: true,
    });
  },
});

globalThis.redefiningSetterLexical = 7;

assert.sameValue(
  redefiningSetterLexical,
  0,
  "a setter that installs a data property must not mirror its argument"
);
assert.sameValue(
  globalThis.redefiningSetterLexical,
  99,
  "the setter's own data-property value is preserved"
);
delete globalThis.redefiningSetterLexical;

let inheritedSetterLexical = 0;
Object.defineProperty(Object.prototype, "inheritedSetterLexical", {
  configurable: true,
  set: function () {
    Object.defineProperty(this, "inheritedSetterLexical", {
      configurable: true,
      value: 99,
      writable: true,
    });
  },
});

globalThis.inheritedSetterLexical = 7;

assert.sameValue(
  inheritedSetterLexical,
  0,
  "an inherited setter must retain its no-data-write outcome through prototype recursion"
);
assert.sameValue(
  globalThis.inheritedSetterLexical,
  99,
  "the inherited setter's own data-property value is preserved"
);
delete globalThis.inheritedSetterLexical;
delete Object.prototype.inheritedSetterLexical;

var ordinaryDataWriteBinding = 0;
globalThis.ordinaryDataWriteBinding = 7;

assert.sameValue(
  ordinaryDataWriteBinding,
  7,
  "an actual own data-property write still mirrors the global object binding"
);
