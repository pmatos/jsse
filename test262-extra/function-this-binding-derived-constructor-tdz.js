/*---
description: A derived constructor's this binding is uninitialized until super() returns, including from arrows and recycled activations
info: |
  9.1.1.3.1 BindThisValue
  9.1.1.3.4 GetThisBinding
  10.2.1.2 OrdinaryCallBindThis

  Reading `this` before `super()` throws a ReferenceError; afterwards every
  activation sees the value bound by its own super() call, never a value left
  over from an earlier call that reused backing storage.
esid: sec-function-environment-records
---*/

class Base {
  constructor() {
    this.base = true;
  }
}

class Derived extends Base {
  constructor(early) {
    if (early === "direct") {
      this;
    }
    if (early === "arrow") {
      (() => this)();
    }
    super();
    this.afterSuper = (() => this)();
  }
}

assert.throws(ReferenceError, function () {
  new Derived("direct");
}, "direct this before super()");

assert.throws(ReferenceError, function () {
  new Derived("arrow");
}, "arrow this before super()");

for (var i = 0; i < 50; i++) {
  var d = new Derived();
  assert.sameValue(d.base, true, "base constructor ran");
  assert.sameValue(d.afterSuper, d, "arrow sees the bound this after super()");
}

class Thrower extends Base {
  constructor() {
    try {
      this;
    } catch (e) {
      super();
      this.recovered = e instanceof ReferenceError;
    }
  }
}
assert.sameValue(new Thrower().recovered, true, "TDZ error is catchable, then super() binds this");
