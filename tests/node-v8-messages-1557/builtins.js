// V8 text of builtin errors: finally, SharedArrayBuffer, private fields, immutable prototypes,
// iterator receivers, Intl options, FinalizationRegistry, structuredClone, RegExp flags (#1558).
// run verbatim on Node and on `jsse --node`.
const f = function () {};
const cases = {
 f1: () => { Promise.prototype.finally.call({ then: 1 }, f); },
 f2: () => { Promise.prototype.finally.call({ then: 'x' }, f); },
 f3: () => { Promise.prototype.finally.call({ then: true }, f); },
 f4: () => { Promise.prototype.finally.call({ then: null }, f); return 1; },
 f5: () => { Promise.prototype.finally.call(1, f); },
 sab1: () => { new SharedArrayBuffer(1).grow(1); },
 sab2: () => { new SharedArrayBuffer(1, { maxByteLength: 4 }).grow(8); },
 sab3: () => { SharedArrayBuffer.prototype.grow.call({}, 1); },
 sab4: () => { SharedArrayBuffer.prototype.grow.call(new ArrayBuffer(1), 1); },
 sab5: () => { SharedArrayBuffer.prototype.slice.call({}, 0); },
 sab6: () => { Object.getOwnPropertyDescriptor(SharedArrayBuffer.prototype, 'byteLength').get.call({}); },
 sab7: () => { Object.getOwnPropertyDescriptor(SharedArrayBuffer.prototype, 'growable').get.call(1); },
 priv1: () => {
   class A { constructor(o) { return o; } }
   class B extends A { #y = 1; constructor(o) { super(o); } }
   const o = {}; new B(o); new B(o);
 },
 priv2: () => {
   class A { constructor(o) { return o; } }
   class B extends A { #y = 1; #w() {} constructor(o) { super(o); } }
   const o = {}; new B(o); new B(o);
 },
 priv3: () => {
   class A { constructor(o) { return o; } }
   class B extends A { get #g() { return 1; } constructor(o) { super(o); } }
   const o = {}; new B(o); new B(o);
 },
 priv4: () => {
   class A { constructor(o) { return o; } }
   class B extends A { #z = 2; #y = 1; constructor(o) { super(o); } }
   const o = {}; new B(o); new B(o);
 },
 priv5: () => {
   class A { constructor(o) { return o; } }
   const B = class extends A { #y = 1; constructor(o) { super(o); } };
   const o = {}; new B(o); new B(o);
 },
 proto1: () => { Object.prototype.__proto__ = {}; },
 proto2: () => { Object.setPrototypeOf(Object.prototype, {}); },
 proto3: () => { Reflect.setPrototypeOf(Object.prototype, {}); Object.prototype.__proto__ = {}; },
 proto4: () => { var a = {}; var b = Object.create(a); a.__proto__ = b; },
 it1: () => { [][Symbol.iterator]().next.call(1); },
 it2: () => { [][Symbol.iterator]().next.call({}); },
 it3: () => { 'x'[Symbol.iterator]().next.call({}); },
 it4: () => { 'x'.matchAll(/x/g).next.call({}); },
 it5: () => { new Map()[Symbol.iterator]().next.call({}); },
 it6: () => { Iterator.from(5); },
 it7: () => { Iterator.from(null); },
 it8: () => { Iterator.from(undefined); },
 intl1: () => { new Intl.NumberFormat('en', { style: 'x' }); },
 intl2: () => { new Intl.DateTimeFormat('en', { month: 'x' }); },
 intl3: () => { new Intl.Collator('en', { usage: 'x' }); },
 intl4: () => { new Intl.PluralRules('en', { type: 'x' }); },
 intl5: () => { new Intl.ListFormat('en', { type: 'x' }); },
 intl6: () => { new Intl.NumberFormat('en', { currency: 'x' }); },
 intl7: () => { new Intl.NumberFormat('en', { minimumFractionDigits: 200 }); },
 intl8: () => { new Intl.NumberFormat('en', { maximumSignificantDigits: 200 }); },
 intl9: () => { Object.getOwnPropertyDescriptor(Intl.Collator.prototype, 'compare').get.call({}); },
 intl10: () => { Intl.Collator.prototype.resolvedOptions.call({}); },
 intl11: () => { new Intl.RelativeTimeFormat('en', { numeric: 'x' }); },
 reg1: () => { new FinalizationRegistry(f).register(1); },
 reg2: () => { const t = {}; new FinalizationRegistry(f).register(t, t); },
 reg3: () => { new FinalizationRegistry(f).unregister(1); },
 reg4: () => { new FinalizationRegistry(f).register({}, 1, 2); },
 reg5: () => { new FinalizationRegistry(f).unregister('s'); },
 reg6: () => { new FinalizationRegistry(f).register({}, 1, {}); return 1; },
};
for (const [k, fn] of Object.entries(cases)) {
  try { fn(); console.log(k, '=> no error'); } catch (e) { console.log(k, '=>', e && e.constructor && e.constructor.name, e && e.message); }
}
try { structuredClone(); } catch (e) { console.log('clone', e.name, e.code, e.message, Object.keys(e)); }
try { structuredClone(function () {}); } catch (e) { console.log('clone2', e.name, e.message); }
for (const [pattern, flags] of [['\\', 'u'], ['(', 'g'], ['[', 'gi'], ['(?<a>x)(?<a>y)', 'v'], ['\\p{Foo}', 'u'], ['*', ''], ['a**', 'm']]) {
  try { new RegExp(pattern, flags); console.log('re', pattern, flags, '=> no error'); } catch (e) { console.log('re', pattern, flags, '=>', e.name, e.message); }
}
