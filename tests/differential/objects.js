// Property enumeration order: integer keys ascending, then strings, then symbols.
const o = { b: 1, 2: "two", a: 2, 1: "one", [Symbol("s")]: 3, "-1": "neg" };
show(Object.keys(o), JSON.stringify(o), Reflect.ownKeys(o).length);
for (const k in o) show("for-in", k);
const proto = { inherited: true };
const child = Object.create(proto, { own: { value: 1, enumerable: true }, hidden: { value: 2 } });
show(Object.keys(child), "inherited" in child, Object.getOwnPropertyNames(child), Object.entries(child));
show(Object.assign({}, { a: 1 }, null, { b: 2 }), { ...{ x: 1 }, y: 2 }, Object.fromEntries([["k", "v"]]));
const frozen = Object.freeze({ z: 1 });
"use strict";
frozen.z = 2;
show(frozen.z, Object.isFrozen(frozen), Object.isSealed({}), Object.isExtensible(Object.preventExtensions({})));
const acc = { _v: 1, get v() { return this._v * 10; }, set v(x) { this._v = x; } };
acc.v = 5;
show(acc.v, Object.getOwnPropertyDescriptor(acc, "v").set !== undefined);
show(Object.groupBy([1, 2, 3, 4], (x) => (x % 2 ? "odd" : "even")));
show(structuredClone({ d: [1, { e: 2 }] }), Object.getPrototypeOf(Object.create(null)));
