// Proxy with an EMPTY handler: every operation falls through to the target.
// Isolates the "look up trap on handler, find undefined, forward" overhead independent of any JS trap call.
// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 5-run median: jsse 5.74 s / boa 1.77 s = 3.24x (jsse --no-bytecode 5.84 s).
// Models: JetStream proxy-vue/proxy-mobx (Proxy with default handler behaviour).
"use strict";
const target = { a: 1, b: 2, c: 3, d: 4 };
const p = new Proxy(target, {});
let sum = 0;
const N = 500000;
for (let i = 0; i < N; i++) {
    sum += p.a + p.b + p.c + p.d;
    p.a = i & 7;
    if ("b" in p) sum++;
}
console.log("proxy_notrap", sum);
