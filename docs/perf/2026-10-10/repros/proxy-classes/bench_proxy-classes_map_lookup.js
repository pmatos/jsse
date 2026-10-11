// Map.get / Map.has / Set.has with a few thousand entries (same storage design as WeakMap: Vec scan + clone).
// Models JetStream hash-map / lazy-collections style Map/Set workloads and MobX's observable Map/Set.
// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 5-run median: jsse 4.53 s / boa 0.22 s = 21.07x (jsse --no-bytecode 4.53 s).
"use strict";
const m = new Map();
const st = new Set();
const NKEYS = 2000;
for (let i = 0; i < NKEYS; i++) { m.set("key" + i, i); st.add(i * 3); }
let s = 0;
const ROUNDS = 300;
for (let r = 0; r < ROUNDS; r++) {
    for (let i = 0; i < NKEYS; i += 7) {
        s += m.get("key" + i);
        if (st.has(i * 3)) s++;
    }
}
console.log("map_lookup", s);
