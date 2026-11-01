// V8 text of defaults in loop heads and catch parameters, empty nested array patterns,
// super constructors and keyword-led array literal callees (#1512).
// run verbatim on Node and on `jsse --node`.
var o = {}, five = 5, nul = null, und, fn = function(){}, x;
const cases = {
 h1: () => { for (const {a = 1} of [nul]); },
 h2: () => { try { throw nul } catch ({a = 1}) {} },
 h3: () => { for (const {a: b = 1} of [und]); },
 h4: () => { for (const {a, b = 1} of [und]); },
 h5: () => { let a; for ({a = 1} of [und]); },
 h6: () => { let a; for ({a: o.x} of [und]); },
 h7: () => { (function({a = 1}){})(nul); },
 h8: () => { (function({a: [b] = []}){})(nul); },
 sup1: () => { class A extends Object { constructor() { super(); } } Object.setPrototypeOf(A, () => {}); new A(); },
 sup2: () => { class A extends Object { constructor() { super(); } } Object.setPrototypeOf(A, function named() {}.bind()); new A(); },
 sup3: () => { class A extends Object { constructor() { super(); } } Object.setPrototypeOf(A, {}); new A(); },
 sup4: () => { class A extends Object { constructor() { super(); } } Object.setPrototypeOf(A, 5 && Math.max); new A(); },
 sup5: () => { class A extends Object { constructor() { super(); } } Object.setPrototypeOf(A, Object.setPrototypeOf(() => {}, null)); new A(); },
 ce1: () => { typeof [fn][0](); },
 ce2: () => { void [fn][0](); },
 ce3: () => { [fn][0](); },
 ce4: () => { var a = [[fn]]; a[0][0](); },
 emp1: () => { let b; ({a: [], b: {c: []}} = {a: [], b: {c: nul}}); },
 emp2: () => { let b; ({a: [], b: [] } = {a: [], b: nul}); },
 emp3: () => { let b; ({a: [] } = {a: nul}); },
 emp4: () => { let b; ({a: [] } = {a: 5}); },
};
for (const [k, f] of Object.entries(cases)) {
  try { f(); console.log(k, '=> no error'); } catch (e) { console.log(k, '=>', e && e.constructor && e.constructor.name, e && e.message); }
}
