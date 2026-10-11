// CONTROL (isolates plain function-call overhead; not a gap in itself).
// CONTROL: the proxy_set trap function called DIRECTLY (no Proxy involved, receiver = target) 1.2M times.
// Same JS body as proxy_set's trap; shows how much of proxy_set is plain function-call overhead.
// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 5-run median: jsse 3.43 s / boa 1.42 s = 2.41x (jsse --no-bytecode 3.67 s).
"use strict";
const target = { a: 0, b: 0, c: 0, d: 0 };
let trapCalls = 0;
function trap(t, key, value, receiver) {
    trapCalls++;
    return Reflect.set(t, key, value, receiver);
}
const N = 200000;
for (let i = 0; i < N; i++) {
    trap(target, "a", i, target); trap(target, "b", i + 1, target);
    trap(target, "c", i + 2, target); trap(target, "d", i + 3, target);
}
console.log("control_trap_direct", target.a + target.b + target.c + target.d, trapCalls);
