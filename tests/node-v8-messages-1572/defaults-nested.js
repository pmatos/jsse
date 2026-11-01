// V8 text of optional-call iteration operands, non-constructors and non-callable
// iterator methods in iteration heads, and nested defaults of destructuring (#1572).
// Run verbatim on Node and on `jsse --node`.
var nul = null, f = () => null, o = {}, und, five = 5, x, c, d;
const cases = {
 n1: () => { var [[c, [d]] = nul] = []; },
 n2: () => { var [[c, [d]] = f()] = []; },
 n3: () => { var [[c, [d]] = five] = []; },
 n4: () => { var [[c, [d]] = und] = []; },
 n5: () => { var [[[d]] = nul] = []; },
 n6: () => { var [[{ d }] = nul] = []; },
 n7: () => { var [[c, { d }] = nul] = []; },
 n8: () => { var [{ c, d: [e] } = nul] = []; },
 n9: () => { var [[c, [d]] = nul.x] = []; },
 n10: () => { var [[c, [d]] = o.q] = []; },
 n11: () => { var { a: [c, [d]] = nul } = {}; },
 n12: () => { ({ a: { c } = nul } = {}); },
 n13: () => { ({ a: { c } = f() } = {}); },
 n14: () => { ({ a: { c } = five } = {}); },
 n15: () => { ({ a: [c] = nul } = {}); },
 n16: () => { ({ a: [c] = f() } = {}); },
 n17: () => { ({ a: { c } = und } = {}); },
 n18: () => { ({ a: { c, d } = nul } = {}); },
 n19: () => { ({ a: { c: { d } } = nul } = {}); },
 n20: () => { [{ c } = nul] = []; },
 n21: () => { [[c] = nul] = []; },
 n22: () => { [[c, [d]] = nul] = []; },
 n23: () => { [{ c, d: [e] } = f()] = []; },
 n24: () => { ({ a: { c } = nul.x } = {}); },
 n25: () => { ({ a: [[c]] = nul } = {}); },
 n27: () => { for ({ a: { c } = nul } of [{}]); },
 n28: () => { for ([[c, [d]] = nul] of [[]]); },
 n29: () => { var { a: { c } = nul } = {}; },
 n30: () => { (({ a: { c } = nul }) => 1)({}); },
 n31: () => { (([[c, [d]] = nul]) => 1)([]); },
 n32: () => { function g([[c, [d]] = nul]) {} g([]); },
 n33: () => { function g({ a: { c } = nul }) {} g({}); },
};
for (const [k, fnc] of Object.entries(cases)) {
  try { fnc(); console.log(k, '=> no error'); } catch (e) { console.log(k, '=>', e && e.message); }
}
