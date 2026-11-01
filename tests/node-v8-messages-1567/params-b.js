// V8 text of for await, yield*, calls and patterns in iteration position, parameter
// patterns, string keys in callees, Intl receivers and RegExp syntax errors (#1567, #1568,
// #1557). Run verbatim on Node and on `jsse --node`.
var f = () => null, nul = null, five = 5, o = {};
const lists = [
 'z, {a}, [b]', '{a}, z, [b]', '{a}, {d}, [b]', '{a}, [b] = [], [c]', '{a} = {}, [b]', '{a} = o, [b]', '{a} = {a: 1}, [b]', '{a}, ...[b]', '{a}, [b], [c]', '[b] = [1], {a}, [c]',
 '{a, b}, [c]', '{a: {c} = {}, e: {g} = []}, [b]', '{a: {c} = {}}, {d}, [b]', '{a}, [b] = [], [c]',
 '{a}, {...r}, [b]', '{a: x}, [b]', '{ "a-b": x }, [b]', '{ 1: x }, [b]', '{a}, ...[b, c]', '{a}, ...rest, [b]',
];
for (const form of ['fn', 'arrow', 'async']) for (const l of lists) {
  let src;
  if (form === 'fn') src = `(function (${l}) {})`;
  else if (form === 'arrow') src = `((${l}) => {})`;
  else if (form === 'async') src = `(async function (${l}) {})`;
  else if (form === 'gen') src = `(function* (${l}) {})`;
  else if (form === 'method') src = `({ m(${l}) {} }).m`;
  else src = `(async (${l}) => {})`;
  let fnc;
  try { fnc = new Function('f', 'nul', 'five', 'o', 'return ' + src)(f, nul, five, o); } catch (e) { console.log(form, l, 'SYNTAX', e.message); continue; }
  let depth = 0, n = 1; for (const ch of l) { if ('([{'.includes(ch)) depth++; else if (')]}'.includes(ch)) depth--; else if (ch === ',' && depth === 0) n++; }
  const args = Array(n - 1).fill({});
  try { const r = fnc.apply({}, args); if (r && r.then) r.catch(e => console.log(form, l, '=> (async)', e.message)); else if (r && r.next) r.next(); else console.log(form, l, '=> no error'); } catch (e) { console.log(form, l, '=>', e.message); }
}
