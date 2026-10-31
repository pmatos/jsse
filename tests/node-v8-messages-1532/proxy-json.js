// V8 text of proxy trap and invariant errors, circular JSON.stringify,
// `Array.from` with a non-callable @@iterator and strict writes to the global
// `undefined`/`NaN` (#1532), run verbatim on Node and on `jsse --node`.
"use strict";
const frozenX = () => {
  const target = {};
  Object.defineProperty(target, "x", { value: 1 });
  return target;
};
const cases = {
  trapNotFunction: () => new Proxy({}, { get: 1 }).x,
  trapNotFunctionHas: () => "x" in new Proxy({}, { has: 1 }),
  getInvariant: () => new Proxy(frozenX(), { get: () => 2 }).x,
  hasInvariant: () => "x" in new Proxy(frozenX(), { has: () => false }),
  ownKeysInvariant: () => Object.keys(new Proxy(frozenX(), { ownKeys: () => [] })),
  deleteInvariant: () => delete new Proxy(frozenX(), { deleteProperty: () => true }).x,
  setInvariant: () => {
    new Proxy(frozenX(), { set: () => true }).x = 2;
  },
  setFalse: () => {
    new Proxy({}, { set: () => false }).x = 2;
  },
  defineFalse: () => Object.defineProperty(new Proxy({}, { defineProperty: () => false }), "x", {}),
  defineFrozen: () => Object.defineProperties(Object.freeze({}), { z: { value: 1 } }),
  defineFrozenOne: () => Object.defineProperty(Object.freeze({}), "z", { value: 1 }),
  jsonCycle: () => {
    const a = {};
    a.a = a;
    JSON.stringify(a);
  },
  jsonCycleDeep: () => {
    const a = { b: { c: {} } };
    a.b.c.d = a;
    JSON.stringify(a);
  },
  jsonCycleLong: () => {
    const a = { b: { c: { d: { e: { f: {} } } } } };
    a.b.c.d.e.f.g = a;
    JSON.stringify(a);
  },
  jsonCycleArray: () => {
    const a = [];
    a[0] = a;
    JSON.stringify(a);
  },
  jsonCycleNested: () => {
    const a = { list: [{}] };
    a.list[0].back = a;
    JSON.stringify(a);
  },
  arrayFromIterator: () => Array.from({ [Symbol.iterator]: 1 }),
  arrayFromNull: () => Array.from(null),
  undefinedAssign: () => {
    undefined = 1;
  },
  nanAssign: () => {
    NaN = 1;
  },
  newObjectLiteralMethod: () => new ({ m() {} }).m(),
  newObjectLiteralTwo: () => new ({ a: 1, m() {} }).m(),
};
for (const [name, fn] of Object.entries(cases)) {
  try {
    fn();
    console.log(name, "-> no error");
  } catch (e) {
    console.log(name, "->", `${e.name}: ${e.message}`);
  }
}
