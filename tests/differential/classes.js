class Base {
  #secret = 1;
  static count = 0;
  static { Base.count = 10; }
  constructor(name) { this.name = name; Base.count++; }
  get secret() { return this.#secret; }
  static isBase(o) { return #secret in o; }
  describe() { return `${this.constructor.name}(${this.name})`; }
  toString() { return "Base!"; }
}
class Derived extends Base {
  constructor(name) { super(name.toUpperCase()); }
  describe() { return "D:" + super.describe(); }
}
const d = new Derived("x");
show(d.describe(), d.secret, Base.count, Base.isBase(d), Base.isBase({}), `${d}`);
show(Object.getOwnPropertyNames(Derived.prototype), typeof Derived, Derived.name, Derived.length);
try { Base(); } catch (e) { show(e.constructor.name); }
const Anon = class {};
const obj = { method() {}, arrow: () => {}, ["comp" + "uted"]: function () {} };
show(Anon.name, obj.method.name, obj.arrow.name, obj.computed.name, (function () {}).name);
class Ext extends Array {}
const e = Ext.from([1, 2, 3]);
show(e instanceof Ext, e.map((x) => x).constructor.name, e.length);
