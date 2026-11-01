// V8 text of for await, yield*, calls and patterns in iteration position, parameter
// patterns, string keys in callees, Intl receivers and RegExp syntax errors (#1567, #1568,
// #1557). Run verbatim on Node and on `jsse --node`.
var f = () => null, nul = null, five = 5, o = {};
const P1_ALL = ['{ a }', '{ a: { c } }', '{ a: { c } = {} }', '{ a = 1 }', '{ a: x }', '{ a: [x] }', '{ a, ...r }', '[x]', '{ a: { c } = nul }', '{ a: { c = 1 } }', '{ a: { c } = { c: 1 } }', '{ a: { c: { d } } }', '{ a: x = 1 }', 'z', 'z = 1', '{}', '{ [k]: x }', '{ a: o.p }', '{ "a": { c } = {} }', '{ a: { c } = [] }', '{ a: { c } = f() }', '{ a: x = {} }', '{ a: x = {y: 1} }', '{ a: x = [1] }', '{ a: x = "s" }', '{ a: x = 5 }', '{ a: x = f() }', '{ a: x = o.p }'];
const P2_ALL = ['[b]', '[b] = f()', '[b] = nul', '[b] = five', '{ b }', '{ b: [c] }', '[b, c]', '[]', '{ b } = nul', '[b] = []', '[b] = undefined'];
const cases = {};
const P1 = P1_ALL.filter((_, i) => i % 2 === 0 || i < 4);
const P2 = ['[b]', '[b] = f()', '{ b } = nul', '[b, c]'];
let n = 0;
for (const p1 of P1) for (const p2 of P2) {
  for (const form of ['fn']) {
    const src = form === 'fn' ? `(function (${p1}, ${p2}) {})` : `((${p1}, ${p2}) => {})`;
    try { const fnc = new Function('f', 'nul', 'five', 'o', 'k', 'return ' + src).call(null, f, nul, five, o, 'k'); cases[`${form} ${p1} | ${p2}`] = () => fnc({}); } catch (e) {}
  }
}
for (const [k, fnc] of Object.entries(cases)) {
  try { fnc(); console.log(k, '=> no error'); } catch (e) { console.log(k, '=>', e && e.message); }
}
