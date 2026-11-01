// V8 text of for await, yield*, calls and patterns in iteration position, parameter
// patterns, string keys in callees, Intl receivers and RegExp syntax errors (#1567, #1568,
// #1557). Run verbatim on Node and on `jsse --node`.
var nul = null, f = () => null, o = {}, und, five = 5;
const cases = {
 m1: () => { var { a: [c] = nul } = { a: 5 }; },
 m2: () => { var { a: [c] = nul } = { a: null }; },
 m3: () => { var { a: [c] = f() } = { a: null }; },
 m4: () => { var { a: [c] = five } = { a: {} }; },
 m5: () => { var { a: [c] = und } = { a: {} }; },
 m6: () => { var { a: [c] = f() } = { a: {} }; },
 m7: () => { var [[c] = nul] = [5]; },
 m8: () => { var [[c] = nul] = [null]; },
 m9: () => { var [[c] = nul] = []; },
 m10: () => { var [[c] = f()] = []; },
 m11: () => { var [[c] = five] = []; },
 m12: () => { var [{ c } = five] = []; },
 m13: () => { var [{ c } = nul] = [null]; },
 m14: () => { var { a: { c } = five } = {}; },
 m15: () => { var { a: { c } = f() } = { a: null }; },
 m16: () => { var { a: { c } = f() } = { a: undefined }; },
 m17: () => { var { a: { c, d } = nul } = {}; },
 m18: () => { var { a: { 'x': c } = nul } = {}; },
 m19: () => { var { a: { ...r } = nul } = {}; },
 m20: () => { var { a: { c = 1 } = nul } = {}; },
 m21: () => { var { a: { [five]: c } = nul } = {}; },
 m22: () => { var { a: {} = nul } = {}; },
 m23: () => { var [[...c] = nul] = []; },
 m25: () => { var { a: { c } = { b: 1 }.d } = {}; },
 m26: () => { var { a: { c } = (1, nul) } = {}; },
 m27: () => { var { a: { c } = this.x } = {}; },
 m28: () => { var { a: { c } = nul || und } = {}; },
};
for (const [k, fnc] of Object.entries(cases)) {
  try { fnc(); console.log(k, '=> no error'); } catch (e) { console.log(k, '=>', e && e.message); }
}
