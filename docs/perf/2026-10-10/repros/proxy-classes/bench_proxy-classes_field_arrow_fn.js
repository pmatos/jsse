// Class with an arrow-function-valued public field (`handler = (e) => {...}`), the ubiquitous
// "bound method" idiom. Every `new` evaluates the field initialiser (creating a closure).
// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 5-run median: jsse 2.12 s / boa 0.77 s = 2.77x (jsse --no-bytecode 2.12 s).
"use strict";
class Btn {
    count = 0;
    onClick = (n) => {
        let r = this.count;
        if (n > 3) { r += n * 2; } else { r += n; }
        this.count = r & 1023;
        return this.count;
    };
    name = "btn";
}
let s = 0;
const N = 150000;
for (let i = 0; i < N; i++) {
    const b = new Btn();
    s += b.onClick(i & 7);
}
console.log("field_arrow_fn", s);
