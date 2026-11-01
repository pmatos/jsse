// V8 text of for await, yield*, calls and patterns in iteration position, parameter
// patterns, string keys in callees, Intl receivers and RegExp syntax errors (#1567, #1568,
// #1557). Run verbatim on Node and on `jsse --node`.
var o = {}, five = 5, nul = null, und, arr = [], fn = function(){}, x, f = () => null, t = () => 1;
var a = {"]": 1};
const cases = {
 fa1: async () => { for await (x of o.m()); },
 fa2: async () => { for await (x of o); },
 fa3: async () => { for await (const x of o); },
 fa4: async () => { for await (x of five); },
 fa5: async () => { for await (x of und); },
 fa6: async () => { for await (x of fn()); },
 fa7: async () => { for await (x of o.p); },
 fa8: async () => { for await (const x of o.m()); },
 fa9: async () => { for await (x of nul); },
 fa10: async () => { for await (x of new fn()); },
 fa11: async () => { for await (x of 1+1); },
 y1: () => { function* g() { yield* o.m() } return g().next(); },
 y2: () => { function* g() { yield* o } return g().next(); },
 y3: () => { function* g() { yield* five } return g().next(); },
 y4: () => { function* g() { yield* fn() } return g().next(); },
 y5: () => { function* g() { yield* und } return g().next(); },
 y6: () => { function* g() { yield* o.p } return g().next(); },
 y7: () => { function* g() { yield* {} } return g().next(); },
 y8: () => { function* g() { yield* new fn() } return g().next(); },
 y9: async () => { async function* g() { yield* o.m() } return g().next(); },
 y10: async () => { async function* g() { yield* o } return g().next(); },
 y11: async () => { async function* g() { yield* five } return g().next(); },
};
(async () => {
for (const [k, fnc] of Object.entries(cases)) {
  try { await fnc(); console.log(k, '=> no error'); } catch (e) { console.log(k, '=>', e && e.constructor && e.constructor.name, e && e.message); }
}
})();
