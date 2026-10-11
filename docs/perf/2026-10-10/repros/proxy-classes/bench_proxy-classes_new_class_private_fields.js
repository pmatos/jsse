// `new C()` where C declares 3 PRIVATE instance fields (#x = 0 ...), a ctor that assigns them and
// reads them back through a method. Models JetStream raytrace-private-class-fields.
// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 5-run median: jsse 4.38 s / boa 1.77 s = 2.47x (jsse --no-bytecode 4.33 s).
"use strict";
class V {
    #x = 0; #y = 0; #z = 0;
    constructor(x, y, z) { this.#x = x; this.#y = y; this.#z = z; }
    sum() { return this.#x + this.#y + this.#z; }
}
let s = 0;
const N = 400000;
for (let i = 0; i < N; i++) {
    const v = new V(i, i + 1, i + 2);
    s += v.sum();
}
console.log("new_class_private_fields", s);
