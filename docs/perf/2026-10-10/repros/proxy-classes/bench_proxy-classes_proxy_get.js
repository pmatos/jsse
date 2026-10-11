// Proxy [[Get]] with a `get` trap that forwards via Reflect.get (the Vue/MobX reactive pattern).
// Models JetStream proxy-vue / proxy-mobx: every property read on a reactive object.
// Isolates: proxy get-trap dispatch cost (trap lookup on handler, args Vec, call, invariant check).
// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 5-run median: jsse 6.09 s / boa 2.63 s = 2.32x (jsse --no-bytecode 6.59 s).
"use strict";
let seed = 12345;
function rnd() { seed = (seed * 1103515245 + 12345) & 0x7fffffff; return seed; }

const target = { a: 1, b: 2, c: 3, d: 4 };
let trapCalls = 0;
const handler = {
    get(t, key, receiver) {
        trapCalls++;
        return Reflect.get(t, key, receiver);
    }
};
const p = new Proxy(target, handler);
const keys = ["a", "b", "c", "d"];
let sum = 0;
const N = 400000;
for (let i = 0; i < N; i++) {
    sum += p.a + p.b + p.c + p.d;
}
console.log("proxy_get", sum, trapCalls);
