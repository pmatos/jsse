// Proxy `has` + `ownKeys` + `getOwnPropertyDescriptor` traps: for-in / Object.keys / `in` over a Proxy.
// Models Vue reactive() iteration (ownKeys trap + has trap) and MobX observable-object enumeration.
// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 5-run median: jsse 2.57 s / boa 1.22 s = 2.11x (jsse --no-bytecode 2.67 s).
// Models: JetStream proxy-vue (ownKeys/has traps).
"use strict";
const target = {};
for (let i = 0; i < 20; i++) target["k" + i] = i;
let calls = 0;
const p = new Proxy(target, {
    has(t, k) { calls++; return Reflect.has(t, k); },
    ownKeys(t) { calls++; return Reflect.ownKeys(t); },
    getOwnPropertyDescriptor(t, k) { calls++; return Reflect.getOwnPropertyDescriptor(t, k); },
});
let sum = 0;
const N = 12000;
for (let i = 0; i < N; i++) {
    sum += Object.keys(p).length;
    if ("k3" in p) sum++;
    if ("zz" in p) sum++;
}
console.log("proxy_ownkeys_has", sum, calls);
