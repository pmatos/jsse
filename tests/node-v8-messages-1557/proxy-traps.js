// V8 text of proxy trap invariants and traps that are not functions (#1558).
// run verbatim on Node and on `jsse --node`.
const frozenX = () => Object.freeze({ x: 1 });
const fixedX = () => Object.defineProperty({}, 'x', { value: 1, configurable: false });
const cases = {
 g1: () => { new Proxy({}, { getOwnPropertyDescriptor() { return 1; } }).hasOwnProperty('x'); },
 g2: () => { Object.getOwnPropertyDescriptor(new Proxy({}, { getOwnPropertyDescriptor() { return 1; } }), 'x'); },
 g3: () => { Object.getOwnPropertyDescriptor(new Proxy(frozenX(), { getOwnPropertyDescriptor() { return undefined; } }), 'x'); },
 g4: () => { Object.getOwnPropertyDescriptor(new Proxy(Object.preventExtensions({}), { getOwnPropertyDescriptor() { return { value: 1, configurable: true }; } }), 'x'); },
 g5: () => { Object.getOwnPropertyDescriptor(new Proxy({}, { getOwnPropertyDescriptor() { return { value: 1, configurable: false }; } }), 'x'); },
 g6: () => { Object.getOwnPropertyDescriptor(new Proxy(Object.defineProperty({}, 'x', { value: 1, writable: true, configurable: false }), { getOwnPropertyDescriptor() { return { value: 1, writable: false, configurable: false }; } }), 'x'); },
 g7: () => { Object.getOwnPropertyDescriptor(new Proxy(Object.preventExtensions({ x: 1 }), { getOwnPropertyDescriptor() { return undefined; } }), 'x'); },
 s1: () => { Object.setPrototypeOf(new Proxy({}, { setPrototypeOf() { return false; } }), null); },
 s2: () => { Object.setPrototypeOf(new Proxy({}, { setPrototypeOf() { return 0; } }), {}); },
 s3: () => { Object.setPrototypeOf(new Proxy(Object.preventExtensions({}), { setPrototypeOf() { return true; } }), {}); },
 e1: () => { Object.isExtensible(new Proxy({}, { isExtensible() { return false; } })); },
 e2: () => { Object.isExtensible(new Proxy(Object.preventExtensions({}), { isExtensible() { return true; } })); },
 e3: () => { Object.preventExtensions(new Proxy({}, { preventExtensions() { return false; } })); },
 e4: () => { Object.preventExtensions(new Proxy({}, { preventExtensions() { return true; } })); },
 a1: () => { new Proxy(function () {}, { apply: 1 })(); },
 a2: () => { new (new Proxy(function () {}, { construct: 1 }))(); },
 a3: () => { var p = new Proxy(function () {}, { apply: 1 }); p(); },
 a4: () => { var p = new Proxy(function () {}, { apply: 1 }); p.call(); },
 a5: () => { var p = new Proxy(function () {}, { apply: 1 }); Reflect.apply(p, null, []); },
 a6: () => { var p = new Proxy(function () {}, { construct: 1 }); new p(); },
 a7: () => { var p = new Proxy(function () {}, { construct: 1 }); Reflect.construct(p, []); },
 a8: () => { var o = { p: new Proxy(function () {}, { apply: 1 }) }; o.p(); },
 a9: () => { new (new Proxy(function () {}, { construct() { return 1; } }))(); },
 a10: () => { var p = new Proxy(function () {}, { construct() { return 'a'; } }); Reflect.construct(p, []); },
 a11: () => { new (function () {})().x(); },
 a12: () => { var p = new Proxy(function () {}, { get: 1 }); p.x; },
 k1: () => { Object.keys(new Proxy({}, { ownKeys() { return [1]; } })); },
 k2: () => { Object.keys(new Proxy({}, { ownKeys() { return [Symbol.iterator, {}]; } })); },
 k3: () => { Object.keys(new Proxy({}, { ownKeys() { return [null]; } })); },
 k4: () => { Object.keys(new Proxy({}, { ownKeys() { return ['a', 5n]; } })); },
 k5: () => { Object.keys(new Proxy(Object.preventExtensions({ a: 1 }), { ownKeys() { return []; } })); },
 k6: () => { Object.keys(new Proxy({}, { ownKeys() { return ['a', 'a']; } })); },
 d1: () => { Reflect.defineProperty(new Proxy({}, { defineProperty() { return true; } }), 'x', { value: 1, configurable: false }); },
 d2: () => { Object.defineProperty(new Proxy(Object.preventExtensions({}), { defineProperty() { return true; } }), 'x', { value: 1 }); },
 d3: () => { Object.defineProperty(new Proxy(Object.defineProperty({}, 'x', { value: 1, writable: true, configurable: false }), { defineProperty() { return true; } }), 'x', { value: 1, writable: false, configurable: false }); },
 d4: () => { Object.defineProperty(new Proxy(fixedX(), { defineProperty() { return true; } }), 'x', { value: 2, configurable: false }); },
 d5: () => { 'use strict'; delete new Proxy({}, { deleteProperty() { return false; } }).x; },
 d6: () => { delete new Proxy(frozenX(), { deleteProperty() { return true; } }).x; },
 d7: () => { 'x' in new Proxy(fixedX(), { has() { return false; } }); },
};
for (const [k, f] of Object.entries(cases)) {
  try { f(); console.log(k, '=> no error'); } catch (e) { console.log(k, '=>', e && e.constructor && e.constructor.name, e && e.message); }
}
