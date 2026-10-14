// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-getnewtarget
description: >
  new.target is read from the nearest function environment, so an arrow
  function keeps the value of the function that created it after an await,
  inside a generator resumed later and in a direct eval, and a call made from
  a constructor body, including a call that fails or is async, never changes
  what the constructor observes afterwards.
info: |
  9.4.5 GetNewTarget ( )
    1. Let envRec be GetThisEnvironment().
    3. Return envRec.[[NewTarget]].
  10.2.1.2 OrdinaryCallBindThis ( F, calleeContext, thisArgument )
  10.2.2 [[Construct]] ( argumentsList, newTarget )
flags: [async]
features: [new.target, async-functions, arrow-function, generators, Reflect.construct]
---*/

var seen = {};

function F() {
  var a = async () => { await 0; return new.target; };
  var b = async () => { await 0; await 0; return (() => new.target)(); };
  seen.sync = (() => new.target)();
  seen.promises = [a(), b()];
  seen.evalNt = eval("new.target");
  seen.evalArrow = eval("(() => new.target)")();
  seen.evalAfterAwait = (async () => { await 0; return eval("new.target"); })();
}
new F();
Reflect.construct(F, [], Object);
var C = new F();

function G() {
  seen.gen = (function* () {})();
  var gen = (function* () { yield 1; })();
  this.arrow = () => new.target;
  gen.next();
}
var g = new G();
assert.sameValue(g.arrow(), G, "arrow escaping a constructor");

class Base {
  constructor() { this.nt = new.target; }
}
class Derived extends Base {
  constructor() {
    var before = () => new.target;
    super();
    this.before = before();
    this.after = (() => new.target)();
  }
}
var d = new Derived();
assert.sameValue(d.nt, Derived, "base sees newTarget through super()");
assert.sameValue(d.before, Derived, "arrow created before super()");
assert.sameValue(d.after, Derived, "arrow created after super()");

class K {}
function CallsClass() {
  assert.throws(TypeError, function () { K(); }, "class constructor called inside a constructor");
  assert.throws(TypeError, function () { (0, K)(); }, "indirect class constructor call");
  this.nt = new.target;
}
assert.sameValue(new CallsClass().nt, CallsClass, "new.target kept after the failed call");

function AsyncInside() {
  var f = async function () {};
  f();
  async function g() { throw 1; }
  g().catch(function () {});
  this.nt = new.target;
}
assert.sameValue(new AsyncInside().nt, AsyncInside, "after calling async functions");

function BadParams() {
  var h = function (a = (function () { throw new Test262Error(); })()) {};
  try { h(); } catch (e) {}
  function* gen(a = (function () { throw new Test262Error(); })()) {}
  try { gen(); } catch (e) {}
  this.nt = new.target;
}
assert.sameValue(new BadParams().nt, BadParams, "after calls that fail binding parameters");

class P extends Promise {}
var subclassed = new P(async function (resolve) { resolve(1); });
assert.sameValue(Object.getPrototypeOf(subclassed), P.prototype, "native constructor keeps its newTarget after an async executor");

Promise.all(seen.promises.concat([seen.evalAfterAwait])).then(function (values) {
  assert.sameValue(values[0], F, "async arrow after await (construct)");
  assert.sameValue(values[1], F, "nested arrow after two awaits");
  assert.sameValue(values[2], F, "direct eval after await");
  assert.sameValue(seen.sync, F, "arrow before await");
  assert.sameValue(seen.evalNt, F, "direct eval");
  assert.sameValue(seen.evalArrow, F, "arrow created by direct eval");
}).then($DONE, $DONE);
