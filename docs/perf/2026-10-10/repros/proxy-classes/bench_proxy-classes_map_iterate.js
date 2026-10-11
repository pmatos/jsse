// for-of over a Map and a Set with a couple of thousand entries; jsse's iterator next() clones the entry Vec
// per step (O(N^2) iteration). Models MobX/Vue Set/Map iteration (dep sets, tag sets).
// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 5-run median: jsse 1.32 s / boa 0.17 s = 7.99x (jsse --no-bytecode 1.32 s).
"use strict";
const m = new Map();
const st = new Set();
const NKEYS = 4000;
for (let i = 0; i < NKEYS; i++) { m.set(i, i + 1); st.add(i); }
let s = 0;
for (let r = 0; r < 6; r++) {
    for (const [k, v] of m) s += k + v;
    for (const v of st) s += v;
}
console.log("map_iterate", s);
