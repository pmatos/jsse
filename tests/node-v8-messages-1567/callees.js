// V8 text of for await, yield*, calls and patterns in iteration position, parameter
// patterns, string keys in callees, Intl receivers and RegExp syntax errors (#1567, #1568,
// #1557). Run verbatim on Node and on `jsse --node`.
var o = {}, five = 5, nul = null, und, arr = [], fn = function(){}, x, f = () => null, t = () => 1;
var a = {"]": 1};
const cases = {
 t1: () => { var [x] = t`a`; },
 t2: () => { var [x] = o.m`a`; },
 t3: () => { var [x] = five`a`; },
 t4: () => { for (x of t`a`); },
 t5: () => { [...t`a`]; },
 t6: () => { var x; [x] = t`a`; },
 t7: () => { var [x] = f`a`; },
 e1: () => { o['\\'](); },
 e2: () => { o['a\nb'](); },
 e3: () => { o["a'b"](); },
 e4: () => { o['aA'](); },
 e5: () => { a["]"](); },
 e6: () => { o['\x41'](); },
 e7: () => { new o['\\'](); },
 e8: () => { o[ "\"" ](); },
 e9: () => { o['\u{1F600}'](); },
 e10: () => { o["a]"](); },
 e11: () => { o["[a"](); },
 e12: () => { o["(a"](); },
 e13: () => { o.x["]"](); },
 e14: () => { o["a"]["]"](); },
 e15: () => { o["\\"]["\\"](); },
};
(async () => {
for (const [k, fnc] of Object.entries(cases)) {
  try { await fnc(); console.log(k, '=> no error'); } catch (e) { console.log(k, '=>', e && e.constructor && e.constructor.name, e && e.message); }
}
})();
