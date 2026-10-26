// plain objects, classes, functions, getters, symbols, circular references
var circ = { name: "c" };
circ.self = circ;
circ.list = [circ, { back: circ }];
var circ2 = { a: { b: {} } };
circ2.a.b.c = circ2.a;
var withGetters = {
  get plain() { return 1; },
  set onlySet(v) {},
  get both() { return 2; },
  set both(v) {},
  value: 3,
};
var withSyms = { [Symbol("k")]: 1, [Symbol.for("f")]: { deep: true }, regular: 2 };
var nullProto = Object.create(null);
nullProto.x = 1;
var nullProtoEmpty = Object.create(null);
var deepObj = { l1: { l2: { l3: { l4: { l5: 1 } } } } };
var weirdKeys = { "a-b": 1, "1x": 2, valid_id: 3, "": 4, "with space": 5, "\n": 6, 42: 7, "it's": 8, $ok: 9, "中": 10 };
class Base { constructor() { this.b = 1; } method() {} static sm() {} get acc() { return 1; } }
class Derived extends Base { constructor() { super(); this.d = 2; } }
class Empty {}
class WithPrivate { #p = 1; pub = 2; static s = 3; }
class WithToStringTag { get [Symbol.toStringTag]() { return "Custom"; } }
class Anon {}
var anonInstance = new (class {})();
var anonDerived = new (class extends Base {})();
function OldStyle() { this.x = 1; }
OldStyle.prototype.proto = function () {};
var viaCreate = Object.create({ inherited: 1 });
viaCreate.own = 2;
var proto2 = Object.create(Base.prototype);
var protoChain = Object.create(Object.create(null));
var ctorless = { constructor: undefined };
var strangeCtor = { constructor: { name: "Fake" } };
var tagged = { [Symbol.toStringTag]: "Tag", a: 1 };
var taggedSame = Object.create({ [Symbol.toStringTag]: "Foo" });
var frozen = Object.freeze({ a: 1 });
var accessorEnum = Object.defineProperty({}, "hidden", { value: 1, enumerable: false });
var propDescs = Object.defineProperties({}, {
  shown: { value: 1, enumerable: true },
  hidden: { value: 2, enumerable: false },
  g: { get: function () { return 1; }, enumerable: true },
});
showAll("obj", [{}, { a: 1 }, { a: 1, b: "two", c: [3], d: { e: null } }, circ, circ2, withGetters, withSyms, nullProto, nullProtoEmpty,
  deepObj, weirdKeys, new Base(), new Derived(), new Empty(), new WithPrivate(), new WithToStringTag(), anonInstance, anonDerived,
  new OldStyle(), viaCreate, proto2, protoChain, ctorless, strangeCtor, tagged, taggedSame, frozen, accessorEnum, propDescs]);
showAll("obj-getters", [withGetters, { get a() { throw fakeError(Error, "boom"); } }, { get a() { return { nested: 1 }; } }], { getters: true });
showAll("obj-getters-get", [withGetters], { getters: "get" });
showAll("obj-getters-set", [withGetters], { getters: "set" });
showAll("obj-hidden", [accessorEnum, propDescs, new Base(), { a: 1 }, withSyms], { showHidden: true });
showAll("obj-depth", [deepObj, circ], { depth: 1 });
showAll("obj-depth0", [deepObj, { a: {} }, { a: [] }, { a: new Map() }], { depth: 0 });
showAll("obj-depth-null", [deepObj], { depth: null });
showAll("obj-depth-inf", [deepObj], { depth: Infinity });
showAll("obj-neg-depth", [deepObj, [1]], { depth: -1 });
showAll("obj-compact", [{ a: { b: { c: {} } }, d: [1, 2, 3] }, deepObj], { compact: false });
showAll("obj-compact1", [{ a: { b: { c: {} } }, d: [1, 2, 3] }, deepObj], { compact: 1 });
showAll("obj-compact5", [{ a: { b: { c: { d: {} } } } }], { compact: 5 });
showAll("obj-break", [{ a: "x".repeat(30), b: "y".repeat(30), c: "z".repeat(30) }, { a: 1, b: 2, c: 3, d: 4, e: 5, f: 6, g: 7, h: 8, i: 9, j: 10, k: 11, l: 12, m: 13, n: 14, o: 15, p: 16, q: 17, r: 18, s: 19, t: 20, u: 21, v: 22, w: 23, x: 24, y: 25, z: 26 }], { breakLength: 40 });
showAll("obj-break80", [{ a: "x".repeat(30), b: "y".repeat(30), c: "z".repeat(30) }, { alpha: 1, beta: 2, gamma: 3, delta: 4, epsilon: 5, zeta: 6, eta: 7, theta: 8, iota: 9 }]);
show("obj-nocustom", { inspect() { return "x"; }, [inspect.custom]() { return "custom!"; } }, { customInspect: false });
show("obj-noshow", Object.create({ x: 1 }));
show("obj-big-keys", Object.fromEntries(Array.from({ length: 30 }, function (_, i) { return ["key" + i, i]; })));
show("obj-nested-arrays", { a: [[1, 2], [3, 4]], b: { c: [{ d: 1 }, { e: [1, 2, 3] }] } });
show("obj-multiline-values", { a: "line1\nline2", b: ["x\ny"], c: { d: "p\nq" } });
show("obj-long-arr-in-obj", { list: Array.from({ length: 30 }, function (_, i) { return i * 100; }), name: "n" });
show("obj-undefined-vals", { a: undefined, b: null, c: NaN, d: -0, e: 0n });
show("obj-symbol-key-desc", { [Symbol()]: 1, [Symbol("")]: 2, [Symbol("with space")]: 3 });
show("obj-protoname", { __proto__: { constructor: { name: "ViaProto" } } });
show("obj-proto-key", JSON.parse('{"__proto__": 1, "a": 2}'));
// functions
function plainFn() {}
function namedWithProps() {}
namedWithProps.prop = 1;
namedWithProps.nested = { a: 1 };
var arrow = () => {};
var asyncFn = async function () {};
var asyncArrow = async () => {};
var genFn = function* gen() {};
var asyncGen = async function* agen() {};
var boundFn = plainFn.bind(null);
var anonFn = (function () { return function () {}; })();
var classExpr = class {};
class Named {}
class WithStatic { static x = 1; static fn() {} }
class Ext extends Named {}
class ExtNull extends null {}
var methodObj = { method() {}, async am() {}, *gm() {}, get g() { return 1; }, ["comp" + "uted"]() {}, [Symbol("sy")]() {} };
var fnWithSym = function s() {};
fnWithSym[Symbol("x")] = 1;
var fnNoName = Object.defineProperty(function () {}, "name", { value: "" });
var fnNumName = Object.defineProperty(function () {}, "name", { value: 5 });
var fnProto = function () {};
Object.setPrototypeOf(fnProto, null);
showAll("fn", [plainFn, namedWithProps, arrow, asyncFn, asyncArrow, genFn, asyncGen, boundFn, anonFn, classExpr, Named, WithStatic, Ext,
  ExtNull, methodObj, fnWithSym, fnNoName, fnNumName, fnProto, Math.max, Object, Symbol, Function.prototype, class { static name = "Overridden"; },
  Object.assign(function () {}, { displayName: "ignored" }), parseInt, console.log.name ? 1 : 2]);
showAll("fn-hidden", [function strictFn() { "use strict"; }, Named], { showHidden: true });
showAll("fn-depth0", [namedWithProps, methodObj], { depth: 0 });
show("fn-in-arr", [plainFn, arrow, Named]);
show("class-extends-fn", new (class X extends OldStyle {})());
show("class-tag", class T { static [Symbol.toStringTag] = "TT"; });
show("global-ish", [Math, JSON, Reflect, Atomics, typeof Intl === "object" ? Intl : 0]);
show("proto-objs", [Object.prototype, Array.prototype, Function.prototype, Base.prototype, Derived.prototype, Map.prototype]);
show("proto-hidden", [Base.prototype], { showHidden: true });
show("symbol-proto", [Symbol.prototype, Number.prototype, String.prototype, Boolean.prototype, BigInt.prototype, Date.prototype, RegExp.prototype, Error.prototype]);
show("generator-obj", [genFn(), asyncGen(), (function* () {})()]);
show("generator-hidden", [genFn()], { showHidden: true });
show("array-iter", [[1, 2][Symbol.iterator](), "ab"[Symbol.iterator](), [1].entries(), new Map([[1, 2]]).entries(), new Set([1]).values()]);
show("iter-helpers", typeof Iterator === "function" ? [Iterator.prototype, [1, 2].values().map(function (x) { return x; })] : 0);
show("obj-with-fn-values", { f: plainFn, a: arrow, c: Named, o: { g: genFn } });
show("circ-hidden", circ, { showHidden: true });
show("circ-depth0", circ, { depth: 0 });
show("circ-map", (function () { var m = new Map(); m.set("self", m); var s = new Set(); s.add(s); return [m, s]; })());
show("circ-array", (function () { var a = [1]; a.push(a); return a; })());
show("circ-class", (function () { var d = new Derived(); d.me = d; return d; })());
