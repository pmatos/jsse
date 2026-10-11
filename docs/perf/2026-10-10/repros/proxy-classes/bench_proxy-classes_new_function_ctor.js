// CONTROL (isolates plain function-call overhead; not a gap in itself).
// CONTROL: instance creation via a plain constructor FUNCTION assigning 3 properties.
// Compare against new_class_ctor / new_class_public_fields / new_class_private_fields.
// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 5-run median: jsse 3.38 s / boa 1.37 s = 2.47x (jsse --no-bytecode 3.68 s).
// Models: JetStream raytrace-*-class-fields baseline (control).
"use strict";
function V(x, y, z) { this.x = x; this.y = y; this.z = z; }
let s = 0;
const N = 400000;
for (let i = 0; i < N; i++) {
    const v = new V(i, i + 1, i + 2);
    s += v.x + v.y + v.z;
}
console.log("new_function_ctor", s);
