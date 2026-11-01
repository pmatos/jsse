// V8 text of for await, yield*, calls and patterns in iteration position, parameter
// patterns, string keys in callees, Intl receivers and RegExp syntax errors (#1567, #1568,
// #1557). Run verbatim on Node and on `jsse --node`.
var o = {}, five = 5, nul = null, und, arr = [], fn = function(){}, x, a = {"]": 1};
const cases = {
 c1: () => { for (x of o.m()); },
 c2: () => { [...o.m()]; },
 c3: () => { var [a] = o.m(); },
 c4: () => { var [a] = five`t`; },
 c7: () => { for (x of 1 + 1); },
 c8: () => { for (x of 2 * 3 - 1); },
 c9: () => { (function ({ a } = nul) {})(); },
 c10: () => { (function ({ a }, [b]) {})({}); },
 c11: () => { a["]"](); },
 c12: () => { new a["]"](); },
 c13: () => { for (x of five()); },
 c14: () => { for (x of o.m.n()); },
 c15: () => { [...five`t`]; },
 c16: () => { for (x of o.m`t`); },
 c17: () => { fn(...o.m()); },
 c18: () => { var x2; [x2] = o.m(); },
 c19: () => { var [a] = new five(); },
 c20: () => { for (x of -1 + 3); },
 c21: () => { for (x of 1 << 3); },
 c22: () => { for (x of 7 / 2); },
 c23: () => { for (x of 'a' + 'b' + 1); },
 c24: () => { for (x of 2 ** 3); },
 c25: () => { for (x of 5 % 3); },
 c26: () => { for (x of (1, 2)); },
 c27: () => { for (x of !1); },
 c28: () => { for (x of 1 + five); },
 c29: () => { for (x of 0.1 + 0.2); },
 c30: () => { for (x of 1 / 0); },
 c31: () => { for (x of 1 - 1); },
 c32: () => { for (x of -(1)); },
 c33: () => { for (x of 4 | 1); },
 c34: () => { for (x of ~1); },
 c35: () => { for (x of 1 < 2); },
 c36: () => { for (x of 3 > 2 ); },
};
(async () => {
for (const [k, fnc] of Object.entries(cases)) {
  try { await fnc(); console.log(k, '=> no error'); } catch (e) { console.log(k, '=>', e && e.constructor && e.constructor.name, e && e.message); }
}
})();
