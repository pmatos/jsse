// Accessor property invocation: class getters/setters called in a hot loop (the raytrace-private
// pattern `get x() { return this.#x; }`, and MobX/Vue computed-style getters).
// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 5-run median: jsse 2.93 s / boa 0.77 s = 3.82x (jsse --no-bytecode 3.22 s).
// Models: JetStream raytrace-private-class-fields (get x() { return this.#x }) and MobX/Vue computed getters.
"use strict";
class P {
    constructor() { this._x = 1; this._y = 2; }
    get x() { return this._x; }
    set x(v) { this._x = v; }
    get y() { return this._y; }
    set y(v) { this._y = v; }
}
const p = new P();
let s = 0;
const N = 300000;
for (let i = 0; i < N; i++) {
    p.x = i & 63;
    p.y = p.x + 1;
    s += p.x + p.y;
}
console.log("getter_setter", s);
