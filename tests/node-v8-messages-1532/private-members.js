// V8 text of private brand checks and `#x in v` (#1544), run verbatim on Node
// and on `jsse --node`. A private method or accessor names the class, a field
// names itself; a null or undefined receiver reads the class name or the field.
function run(name, fn) {
  try {
    fn();
    console.log(name, "-> ok");
  } catch (e) {
    console.log(name, "->", `${e.name}: ${e.message}`);
  }
}

class A {
  #m() {}
  static #sm() {}
  get #g() { return 1; }
  set #w(v) {}
  static get #sg() { return 1; }
  #f = 1;
  static #sf = 1;
  static callM(o) { o.#m(); }
  static callSm(o) { o.#sm(); }
  static readG(o) { return o.#g; }
  static writeW(o) { o.#w = 1; }
  static readSg(o) { return o.#sg; }
  static readF(o) { return o.#f; }
  static writeF(o) { o.#f = 1; }
  static bumpF(o) { o.#f++; }
  static readSf(o) { return o.#sf; }
  static writeSf(o) { o.#sf = 1; }
  static writeM(o) { o.#m = 1; }
  static writeSm(o) { o.#sm = 1; }
  static hasM(o) { return #m in o; }
  static hasSm(o) { return #sm in o; }
  static hasG(o) { return #g in o; }
  static hasF(o) { return #f in o; }
  static hasSf(o) { return #sf in o; }
}

const names = Object.getOwnPropertyNames(A).filter((n) => !["length", "name", "prototype"].includes(n));
const receivers = [["empty", {}], ["number", 1], ["string", "abc"], ["null", null], ["undefined", undefined], ["function", function () {}], ["symbol", Symbol("q")]];
for (const name of names) {
  for (const [label, value] of receivers) {
    run(`A.${name}(${label})`, () => A[name](value));
  }
}

const Anon = class {
  #m() {}
  static #sm() {}
  #f;
  static callM(o) { o.#m(); }
  static readF(o) { return o.#f; }
  static hasM(o) { return #m in o; }
  static hasF(o) { return #f in o; }
};
for (const name of ["callM", "readF", "hasM", "hasF"]) {
  run(`Anon.${name}({})`, () => Anon[name]({}));
  run(`Anon.${name}(1)`, () => Anon[name](1));
  run(`Anon.${name}(null)`, () => Anon[name](null));
}
const AnonStatic = class {
  static #sm() {}
  static call(o) { o.#sm(); }
};
run("AnonStatic.call({})", () => AnonStatic.call({}));
run("AnonStatic.call(null)", () => AnonStatic.call(null));

class Outer {
  #m() {}
  static make() {
    return class Inner {
      #n() {}
      static viaInner(o) { o.#n(); }
      static viaOuter(o) { o.#m(); }
    };
  }
}
run("Inner.viaInner({})", () => Outer.make().viaInner({}));
run("Inner.viaOuter({})", () => Outer.make().viaOuter({}));
class Derived extends Outer {
  #d() {}
  static call(o) { o.#d(); }
}
run("Derived.call(new Outer())", () => Derived.call(new Outer()));
run("Derived.call(Proxy)", () => Derived.call(new Proxy({}, {})));
class Pair {
  #x = 1;
  static bump(o) { o.#x ??= 2; }
  static nested(o) { ({ x: o.#x } = { x: 1 }); }
  static optional(o) { return o?.#x; }
}
run("Pair.bump({})", () => Pair.bump({}));
run("Pair.bump(null)", () => Pair.bump(null));
run("Pair.nested({})", () => Pair.nested({}));
run("Pair.nested(null)", () => Pair.nested(null));
run("Pair.optional(null)", () => Pair.optional(null));
run("Pair.optional({})", () => Pair.optional({}));
