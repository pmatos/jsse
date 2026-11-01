// V8 text of for await, yield*, calls and patterns in iteration position, parameter
// patterns, string keys in callees, Intl receivers and RegExp syntax errors (#1567, #1568,
// #1557). Run verbatim on Node and on `jsse --node`.
var o = {}, five = 5, nul = null, und, arr = [], x, fn = function(){ return {}; }, r5 = () => 5;
var obj = { q: { r: 1 }, m() { return 1; }, k: 5, s: 'str' };
var kinds = {
 id: 'five', idobj: 'o', num: '5', str: '"s"', objlit: '{}', arrlit: '[]', this_: 'this',
 call: 'r5()', callm: 'obj.m()', newf: 'new fn()', member: 'obj.k', index: 'obj["k"]', tagged: 'r5`a`',
 seq: '(0, five)', cond: '(five ? five : 1)', logic: '(five && five)', logicn: '(five ?? 1)', unary: '-five', unaryv: 'void 0', typ: 'typeof five',
 assign: '(x = five)', bin: '(five + 1)', binfold: '(2 + 3)', tmpl: '`a${five}`', tmpl0: '`a`', fnexpr: '(function(){})', arrow: '(() => 1)', cls: '(class {})',
 optm: 'obj?.k', optq: 'obj?.q', nullid: 'nul', undid: 'und', nulllit: 'null', await_: '(await five)', parenid: '(five)', parenmem: '(obj.k)',
 upd: 'five++', comma3: '(1, 2, five)', spreadarr: '[...arr]', regex: '/a/', big: '1n', thisk: 'this.k', newtarget: 'new.target', superm: 'obj.q.r',
};
const cases = {};
const mk = (name, body) => { try { cases[name] = new Function('five', 'o', 'nul', 'und', 'r5', 'obj', 'fn', 'arr', 'x', body).bind(obj, five, o, nul, und, r5, obj, fn, arr, x); } catch (e) {} };
for (const [k, e] of Object.entries(kinds)) {
  mk('s_' + k, `function* g() { yield* ${e} } return g().next();`);
  mk('a_' + k, `async function* g() { yield* ${e} } return g().next();`);
  mk('f_' + k, `return (async () => { for await (var v of ${e}); })();`);
}
(async () => {
for (const [k, fnc] of Object.entries(cases)) {
  try { await fnc(); console.log(k, '=> no error'); } catch (e) { console.log(k, '=>', e && e.constructor && e.constructor.name, e && e.message); }
}
})();
