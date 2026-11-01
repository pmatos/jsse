// V8 text of for await, yield*, calls and patterns in iteration position, parameter
// patterns, string keys in callees, Intl receivers and RegExp syntax errors (#1567, #1568,
// #1557). Run verbatim on Node and on `jsse --node`.
var nul = null, f = () => null, o = {}, und;
const cases = {
 n1: () => { var { a: { c } = nul } = {}; },
 n2: () => { var { a: { c } = f() } = {}; },
 n3: () => { var { a: { c } = und } = {}; },
 n4: () => { var { a: { c } = null } = {}; },
 n5: () => { var { a: { c } = o.p } = {}; },
 n6: () => { var { a: { c } } = {}; },
 n7: () => { var { a: { c } } = { a: null }; },
 n8: () => { var { a: { c } = o.p } = { a: null }; },
 n9: () => { var { a: [c] = nul } = {}; },
 n10: () => { var { a: [c] = f() } = {}; },
 n11: () => { var { a: [c] = und } = {}; },
 n12: () => { var { a: [c] = 5 } = {}; },
 n13: () => { var { a: [c] } = {}; },
 n14: () => { var { a: [c] = [] , b: { d } } = {}; },
 n15: () => { (function ({ a: { c } = nul }) {})({}); },
 n16: () => { (function ({ a: { c } = und }) {})({}); },
 n17: () => { (function ([a, { c } = nul]) {})([]); },
 n18: () => { var [a, { c } = nul] = []; },
 n19: () => { var [{ c } = nul] = []; },
 n20: () => { let { a: { c } = nul } = {}; },
 n22: () => { for (var { a: { c } = nul } of [{}]); },
 n23: () => { try { throw {}; } catch ({ a: { c } = nul }) {} },
};
for (const [k, fnc] of Object.entries(cases)) {
  try { fnc(); console.log(k, '=> no error'); } catch (e) { console.log(k, '=>', e && e.message); }
}
