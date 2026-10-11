// `new C()` where C declares 3 PUBLIC instance fields with initialisers (x = 0; y = 0; z = 0;) and a ctor that assigns them.
// Models JetStream raytrace-public-class-fields (Color/Vector/Ray allocation in the hot loop).
// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 5-run median: jsse 3.47 s / boa 1.72 s = 2.02x (jsse --no-bytecode 3.83 s).
"use strict";
class V { x = 0; y = 0; z = 0; constructor(x, y, z) { this.x = x; this.y = y; this.z = z; } }
let s = 0;
const N = 400000;
for (let i = 0; i < N; i++) {
    const v = new V(i, i + 1, i + 2);
    s += v.x + v.y + v.z;
}
console.log("new_class_public_fields", s);
