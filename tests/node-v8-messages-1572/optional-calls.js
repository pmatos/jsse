// V8 text of optional-call iteration operands, non-constructors and non-callable
// iterator methods in iteration heads, and nested defaults of destructuring (#1572).
// Run verbatim on Node and on `jsse --node`.
var obj = { m() { return 1; }, k: 5, n: null }, f = () => 1, x, id = (...a) => a;
var cases = {
 spread_call: () => id(...obj?.m()),
 spread_new: () => new Set(...obj?.m()),
 forof: () => { for (x of obj?.m()); },
 forof2: () => { for (x of f?.()); },
 forof3: () => { for (x of obj?.["m"]()); },
 forof4: () => { for (x of obj.m?.()); },
 forof5: () => { for (x of obj?.m().z); },
 forof6: () => { for (x of obj?.k); },
 forof7: () => { for (x of (obj?.m())); },
 forof8: () => { for (x of obj?.m(1, 2)); },
 forof9: () => { for (x of obj?.m()()); },
 forof10: () => { for (x of obj?.m?.()); },
 forof11: () => { for (x of obj?.k.z()); },
 darr: () => { var [a] = obj?.m(); },
 darr2: () => { var [a] = f?.(); },
 dobj: () => { var { a } = obj.n?.m(); },
 assign_arr: () => { [x] = obj?.m(); },
 yield_a: () => { function* g() { yield* obj?.m(); } g().next(); },
 yield_b: () => { function* g() { yield* obj.m?.(); } g().next(); },
 yield_c: () => { function* g() { yield* obj?.m().z; } g().next(); },
 yield_d: () => { function* g() { yield* obj?.["m"](); } g().next(); },
 fa_a: () => (async () => { for await (x of obj?.m().z); })(),
 fa_b: () => (async () => { for await (x of obj?.m(1)); })(),
 fa_c: () => (async () => { for await (x of obj?.m()()); })(),
 fa_d: () => (async () => { for await (x of (obj?.m())); })(),
 fa_e: () => (async () => { for await (x of obj?.m?.()); })(),
 fa_f: () => (async () => { for await (x of f?.()); })(),
 fa_g: () => (async () => { for await (x of obj?.["m"]()); })(),
};
(async () => {
for (const [k, fnc] of Object.entries(cases)) {
  try { await fnc(); console.log(k, '=> no error'); } catch (e) { console.log(k, '=>', e && e.message); }
}
})();
