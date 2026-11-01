// V8 text of optional-call iteration operands, non-constructors and non-callable
// iterator methods in iteration heads, and nested defaults of destructuring (#1572).
// Run verbatim on Node and on `jsse --node`.
var o = {}, und, c, d, nul = null, f = () => null;
const cases = {
 e1: () => { var [[c] = o.q] = []; },
 e2: () => { var [[c, [d]] = o.q] = []; },
 e3: () => { [[c] = o.q] = []; },
 e4: () => { [[c, [d]] = o.q] = []; },
 e5: () => { var [[c] = o['q']] = []; },
 e6: () => { var [[c] = nul.x] = []; },
 e7: () => { var { a: [c] = o.q } = {}; },
 e8: () => { ({ a: [c] = o.q } = {}); },
 e9: () => { ({ a: { c } = o.q } = {}); },
 e10: () => { var { a: { c } = o.q } = {}; },
 e11: () => { [[c] = f()] = []; },
 e12: () => { [{ c } = f()] = []; },
 e13: () => { var [[c] = und] = [null]; },
 e14: () => { [[c] = und] = [null]; },
 e15: () => { var [[c, [d]] = und] = [null]; },
 e16: () => { var [[c, [d]] = [1, und]] = []; },
 e17: () => { var [[c, [d]] = [1, 2]] = []; },
 e18: () => { var [[c, [d]]] = [[1, 2]]; },
 e19: () => { [[c, [d]] = [1, 2]] = []; },
 e20: () => { ({ a: [c, [d]] = [1, 2] } = {}); },
 e21: () => { ({ a: [c, [d]] } = { a: [1, 2] }); },
};
for (const [k, fnc] of Object.entries(cases)) {
  try { fnc(); console.log(k, '=> no error'); } catch (e) { console.log(k, '=>', e && e.message); }
}
