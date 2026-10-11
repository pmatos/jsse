// Proxy [[Set]] with a `set` trap that forwards via Reflect.set and bumps a counter.
// Models JetStream proxy-vue/proxy-mobx writes to reactive state.
// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 5-run median: jsse 7.24 s / boa 2.12 s = 3.41x (jsse --no-bytecode 7.60 s).
"use strict";
const target = { a: 0, b: 0, c: 0, d: 0 };
let trapCalls = 0;
const handler = {
    set(t, key, value, receiver) {
        trapCalls++;
        return Reflect.set(t, key, value, receiver);
    }
};
const p = new Proxy(target, handler);
const N = 200000;
for (let i = 0; i < N; i++) {
    p.a = i; p.b = i + 1; p.c = i + 2; p.d = i + 3;
}
console.log("proxy_set", target.a + target.b + target.c + target.d, trapCalls);
