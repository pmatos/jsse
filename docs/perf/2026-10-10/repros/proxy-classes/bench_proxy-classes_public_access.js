// CONTROL (isolates plain function-call overhead; not a gap in itself).
// CONTROL for private_access: same loop with ordinary (public) properties.
// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 5-run median: jsse 2.63 s / boa 0.57 s = 4.63x (jsse --no-bytecode 2.63 s).
// Models: JetStream raytrace-public-class-fields (this.x reads/writes in methods).
"use strict";
class Acc {
    constructor() { this.a = 1; this.b = 2; this.c = 3; }
    run(n) {
        let s = 0;
        for (let i = 0; i < n; i++) {
            this.a = this.a + 1;
            s += this.a + this.b + this.c;
            this.c = s & 255;
        }
        return s;
    }
}
console.log("public_access", new Acc().run(600000));
