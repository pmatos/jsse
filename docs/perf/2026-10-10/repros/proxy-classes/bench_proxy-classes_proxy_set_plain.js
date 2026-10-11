// Like proxy_set but the trap writes to the target directly (`t[key] = value; return true`) instead of
// Reflect.set(t, key, value, receiver). Separates "JS trap call overhead" from the descriptor round-trip
// (Receiver.[[GetOwnProperty]]/[[DefineOwnProperty]] on the proxy) that Reflect.set(..., receiver=proxy) performs.
// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 5-run median: jsse 2.88 s / boa 1.47 s = 1.96x (jsse --no-bytecode 3.13 s).
"use strict";
const target = { a: 0, b: 0, c: 0, d: 0 };
let trapCalls = 0;
const handler = {
    set(t, key, value, receiver) {
        trapCalls++;
        t[key] = value;
        return true;
    }
};
const p = new Proxy(target, handler);
const N = 200000;
for (let i = 0; i < N; i++) {
    p.a = i; p.b = i + 1; p.c = i + 2; p.d = i + 3;
}
console.log("proxy_set_plain", target.a + target.b + target.c + target.d, trapCalls);
