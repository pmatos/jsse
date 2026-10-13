/*---
description: >
  The instance element definitions of a class (fields, private methods and
  accessors) are shared by every construction. Each construction evaluates the
  field initializers afresh and installs its own private elements; classes
  produced by one class expression evaluated repeatedly keep separate private
  names; classes that declare no initializers or only private methods still
  install their elements.
esid: sec-initializeinstanceelements
info: |
  InitializeInstanceElements ( O, constructor )
    1. Let methods be the value of constructor.[[PrivateMethods]].
    2. For each PrivateElement method of methods, do
      a. Perform ? PrivateMethodOrAccessorAdd(O, method).
    3. Let fields be the value of constructor.[[Fields]].
    4. For each element fieldRecord of fields, do
      a. Perform ? DefineField(O, fieldRecord).
features: [class-fields-public, class-fields-private, class-methods-private]
---*/

var counter = 0;
class Counted {
  id = ++counter;
  #secret = counter * 10;
  fn = () => "anon";
  #priv = function () {};
  static readSecret(o) { return o.#secret; }
  static privName(o) { return o.#priv.name; }
}

var a = new Counted();
var b = new Counted();
var c = new Counted();
assert.sameValue(a.id, 1, "first construction runs the initializer");
assert.sameValue(b.id, 2, "second construction runs it again");
assert.sameValue(c.id, 3, "third construction runs it again");
assert.sameValue(Counted.readSecret(a), 10, "private field value of the first object");
assert.sameValue(Counted.readSecret(b), 20, "private field value of the second object");
assert.sameValue(Counted.readSecret(c), 30, "private field value of the third object");
assert.sameValue(a.fn.name, "fn", "public field function name");
assert.notSameValue(a.fn, b.fn, "each construction creates its own closure");
assert.sameValue(Counted.privName(b), "#priv", "private field function name");

function makeClass() {
  return class {
    #p = 1;
    pub = 2;
    static has(o) { return #p in o; }
    get() { return this.#p; }
  };
}
var K1 = makeClass();
var K2 = makeClass();
var k1 = new K1();
var k2 = new K2();
assert.sameValue(K1.has(k1), true, "K1 brands its own instances");
assert.sameValue(K1.has(k2), false, "K1 does not brand K2 instances");
assert.sameValue(K2.has(k2), true, "K2 brands its own instances");
assert.sameValue(K2.has(k1), false, "K2 does not brand K1 instances");
assert.throws(TypeError, function () { K1.prototype.get.call(k2); }, "foreign brand check throws");
assert.sameValue(k1.get(), 1);

class OnlyMethods {
  #m() { return "m"; }
  get #acc() { return "acc"; }
  set #acc(v) { this.stored = v; }
  run() { this.#acc = 5; return this.#m() + this.#acc + this.stored; }
}
assert.sameValue(new OnlyMethods().run(), "macc5", "private methods/accessors without any initializer");
assert.sameValue(new OnlyMethods().run(), "macc5", "and again on a second construction");

class Uninit {
  #u;
  v;
  has() { return #u in this; }
  get u() { return this.#u; }
}
var u = new Uninit();
assert.sameValue(u.has(), true, "uninitialised private field is installed");
assert.sameValue(u.u, undefined);
assert.sameValue(Object.prototype.hasOwnProperty.call(u, "v"), true, "uninitialised public field is defined");

class Empty {}
assert.sameValue(Object.keys(new Empty()).length, 0, "field-less class constructs");

class Base { constructor() { this.order = ["base"]; } }
class Derived extends Base {
  #d = (this.order.push("field"), 1);
  constructor() { super(); this.order.push("ctor"); }
  d() { return this.#d; }
}
var d1 = new Derived();
var d2 = new Derived();
assert.sameValue(d1.order.join(), "base,field,ctor");
assert.sameValue(d2.order.join(), "base,field,ctor");
assert.sameValue(d2.d(), 1);

var calls = 0;
class Throwing {
  x = (function () { if (++calls === 2) throw new RangeError("second"); return calls; })();
  #y = 0;
}
new Throwing();
assert.throws(RangeError, function () { new Throwing(); }, "an initializer throw propagates");
assert.sameValue(new Throwing().x, 3, "later constructions are unaffected");

var evalNames = (function () {
  class E {
    #e = 7;
    static probe(o) { return eval("o.#e"); }
  }
  return [E.probe(new E()), E.probe(new E())];
})();
assert.sameValue(evalNames[0], 7, "direct eval resolves the shared private-name map");
assert.sameValue(evalNames[1], 7);

class Returner {
  constructor(o) { return o; }
}
class Stamp extends Returner {
  #s = 1;
  static has(o) { return #s in o; }
}
var good = {};
new Stamp(good);
assert.sameValue(Stamp.has(good), true);
assert.throws(TypeError, function () { new Stamp(good); }, "adding the same private field twice throws");
