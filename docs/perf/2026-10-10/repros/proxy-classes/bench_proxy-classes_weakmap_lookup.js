// WeakMap.get / has / set with a few thousand live object keys (Vue's `targetMap: WeakMap<target, depsMap>`,
// MobX's per-object administration lookups). jsse stores WeakMap/Map/Set entries in a Vec scanned
// linearly and CLONES the whole Vec on get/has, so cost is O(N) per lookup with N = live keys.
// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 5-run median: jsse 4.99 s / boa 0.22 s = 23.15x (jsse --no-bytecode 5.04 s).
"use strict";
const keys = [];
const wm = new WeakMap();
const NKEYS = 2000;
for (let i = 0; i < NKEYS; i++) { const k = { id: i }; keys.push(k); wm.set(k, i * 2); }
let s = 0;
const ROUNDS = 450;
for (let r = 0; r < ROUNDS; r++) {
    for (let i = 0; i < NKEYS; i += 7) {
        s += wm.get(keys[i]);
        if (wm.has(keys[(i * 3) % NKEYS])) s++;
    }
}
console.log("weakmap_lookup", s);
