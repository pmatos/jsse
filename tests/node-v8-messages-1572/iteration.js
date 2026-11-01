// V8 text of optional-call iteration operands, non-constructors and non-callable
// iterator methods in iteration heads, and nested defaults of destructuring (#1572).
// Run verbatim on Node and on `jsse --node`.
var obj = { m() { return 1; }, k: 5, n: null, ag: { [Symbol.asyncIterator]: 1 }, ai: { [Symbol.iterator]: 1 } };
var f = () => 1, nul = null, und, five = 5, x;
var cases = {
 fa_optm: `for await (x of obj?.m());`,
 fa_optm2: `for await (x of obj.n?.m());`,
 fa_optm3: `for await (x of obj?.k());`,
 fa_optidx: `for await (x of obj?.["m"]());`,
 fa_optcall: `for await (x of obj.m?.());`,
 fa_optcall2: `for await (x of f?.());`,
 fa_optchain3: `for await (x of obj?.k.z.y());`,
 y_optm: `yield* obj?.m();`,
 y_optcall: `yield* obj.m?.();`,
 y_optcall2: `yield* f?.();`,
 y_optidx: `yield* obj?.["m"]();`,
 fo_optm: `for (x of obj?.m());`,
 fo_optcall: `for (x of f?.());`,
 arr_optm: `var [a] = obj?.m();`,
 arr_optcall: `var [a] = f?.();`,
 fa_ag: `for await (x of obj.ag);`,
 fa_aglit: `for await (x of { [Symbol.asyncIterator]: 1 });`,
 y_aglit: `yield* { [Symbol.asyncIterator]: 1 };`,
 fo_ai: `for (x of obj.ai);`,
 fa_new: `for await (x of new f());`,
 fa_new2: `for await (x of new obj.k());`,
 fa_new3: `for await (x of new obj.m());`,
 fa_new4: `for await (x of new five());`,
 fa_new5: `for await (x of new (f)());`,
 fa_new6: `for await (x of new f);`,
 y_new: `yield* new f();`,
 y_new2: `yield* new obj.k();`,
 fo_new: `for (x of new f());`,
 fo_new2: `for (x of new obj.k());`,
 arr_new: `var [a] = new f();`,
 plain_new: `new f();`,
 plain_new2: `var z = new obj.k();`,
 spread_new: `var z = [...new f()];`,
};
(async () => {
for (const [k, e] of Object.entries(cases)) {
  const gen = k.startsWith('y_');
  const kinds = gen ? ['sync', 'async'] : ['plain'];
  for (const kind of kinds) {
    let code = e;
    if (k.startsWith('fa_')) code = `return (async () => { ${e} })();`;
    else if (gen) code = kind == 'sync' ? `function* g() { ${e} } return g().next();` : `async function* g() { ${e} } return g().next();`;
    try { await new Function('obj', 'f', 'nul', 'und', 'five', 'x', code).call({}, obj, f, nul, und, five, x); console.log(k, kind, '=> ok'); } catch (err) { console.log(k, kind, '=>', err && err.constructor && err.constructor.name, err && err.message); }
  }
}
})();
