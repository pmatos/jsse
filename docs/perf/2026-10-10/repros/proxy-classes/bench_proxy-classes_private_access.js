// Private field read/write in a hot loop on ONE long-lived instance (no allocation): isolates this.#f get/set cost.
// Compare to public_access (same shape with public properties).
// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 5-run median: jsse 2.32 s / boa 0.77 s = 3.03x (jsse --no-bytecode 2.32 s).
// Models: JetStream raytrace-private-class-fields (this.#x reads/writes in methods).
"use strict";
class Acc {
    #a = 1; #b = 2; #c = 3;
    run(n) {
        let s = 0;
        for (let i = 0; i < n; i++) {
            this.#a = this.#a + 1;
            s += this.#a + this.#b + this.#c;
            this.#c = s & 255;
        }
        return s;
    }
}
console.log("private_access", new Acc().run(600000));
