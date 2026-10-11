/*---
description: Function this binding coercion, arrow capture and per-call isolation
info: |
  10.2.1.2 OrdinaryCallBindThis
  9.1.1.3.4 GetThisBinding

  A sloppy function replaces a nullish receiver with the global object and
  boxes a primitive receiver; a strict function keeps the receiver. Arrow
  functions have no this binding and read the enclosing function's, and
  recursive activations keep distinct bindings.
esid: sec-ordinarycallbindthis
flags: [noStrict]
---*/

var theGlobal = this;

function sloppy() {
  return this;
}
function strict() {
  "use strict";
  return this;
}

assert.sameValue(sloppy(), theGlobal, "undefined receiver becomes the global object");
assert.sameValue(sloppy.call(null), theGlobal, "null receiver becomes the global object");
assert.sameValue(typeof sloppy.call(5), "object", "primitive receiver is boxed");
assert.sameValue(strict(), undefined, "strict keeps undefined");
assert.sameValue(strict.call(5), 5, "strict keeps the primitive");

function arrowCapture() {
  var inner = () => () => this;
  return inner()();
}
var receiver = { name: "r" };
assert.sameValue(arrowCapture.call(receiver), receiver, "nested arrows read the enclosing this");

function recurse(n, expected) {
  var o = { n: n };
  if (n > 0) {
    var inner = recurse.call({ n: n - 1 }, n - 1, null);
  }
  return this.n;
}
assert.sameValue(recurse.call({ n: 7 }, 3, null), 7, "outer activation keeps its own this after inner calls");

var obj = {
  m() {
    return [1, 2].map(() => this);
  },
};
var mapped = obj.m();
assert.sameValue(mapped[0], obj);
assert.sameValue(mapped[1], obj);
