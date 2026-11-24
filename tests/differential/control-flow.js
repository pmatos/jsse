outer: for (let i = 0; i < 3; i++) {
  for (let j = 0; j < 3; j++) {
    if (j === 1) continue outer;
    if (i === 2) break outer;
    show("loop", i, j);
  }
}
function f() {
  try { return "try"; } finally { show("finally runs"); }
}
show(f());
function g() {
  try { throw new Error("boom"); } catch ({ message }) { return message; } finally { show("g finally"); }
}
show(g());
switch (3) { case 1: show(1); case 3: show(3); case 4: show(4); break; default: show("d"); }
const fns = [];
for (let i = 0; i < 3; i++) fns.push(() => i);
var vfns = [];
for (var k = 0; k < 3; k++) vfns.push(() => k);
show(fns.map((h) => h()), vfns.map((h) => h()));
let n = 0;
do { n++; } while (n < 5);
show(n, typeof undeclared, void 0, (1, 2), null ?? "dflt", 0 || "or", "" && "and");
const opt = { a: { b: null } };
show(opt?.a?.b?.c, opt.x?.(), opt.a.b ?? "nullish");
let la = null; la ??= 5; let lb = 1; lb &&= 7; let lc = 0; lc ||= 9;
show(la, lb, lc);
