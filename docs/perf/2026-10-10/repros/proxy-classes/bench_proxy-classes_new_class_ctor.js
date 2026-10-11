// `new C()` where C is a class with a constructor that assigns 3 properties (no field declarations).
// Isolates class-construct overhead vs. new_function_ctor (same work, class instead of function).
// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 5-run median: jsse 3.38 s / boa 1.47 s = 2.30x (jsse --no-bytecode 3.98 s).
// Models: JetStream proxy-mobx/vue (class hierarchies), raytrace-*-class-fields.
"use strict";
class V { constructor(x, y, z) { this.x = x; this.y = y; this.z = z; } }
let s = 0;
const N = 400000;
for (let i = 0; i < N; i++) {
    const v = new V(i, i + 1, i + 2);
    s += v.x + v.y + v.z;
}
console.log("new_class_ctor", s);
