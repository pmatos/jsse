// Derived-class construction with super(...) call + method calls via super.method(): 3-level hierarchy.
// Models MobX/Vue class hierarchies (ObservableReminder extends Reminder: constructor(...args){super(...args)}).
// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 5-run median: jsse 3.53 s / boa 1.57 s = 2.25x (jsse --no-bytecode 3.33 s).
"use strict";
class A { constructor(x) { this.x = x; } get() { return this.x; } }
class B extends A { constructor(x, y) { super(x); this.y = y; } get() { return super.get() + this.y; } }
class C extends B { constructor(x, y, z) { super(x, y); this.z = z; } get() { return super.get() + this.z; } }
class D extends C { constructor(...args) { super(...args); } }
let s = 0;
const N = 100000;
for (let i = 0; i < N; i++) {
    const c = new D(i, 1, 2);
    s += c.get();
}
console.log("derived_super", s);
