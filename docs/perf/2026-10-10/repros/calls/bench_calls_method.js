// Method calls on class instances (prototype-chain lookup + this.field reads/writes).
// Measured 2026-10-10 on buildbox3 (jsse 2b621c5 v0.11.0 vs boa 0.22.0, median of 5): jsse 3.88 s / boa 2.27 s = 1.71x; jsse --no-bytecode 5.69 s; loadavg 9.3.
// Models: richards (scheduler/task methods), deltablue (constraint methods), raytrace (vector methods).
class Vec {
  constructor(x, y, z) { this.x = x; this.y = y; this.z = z; }
  dot(o) { return this.x * o.x + this.y * o.y + this.z * o.z; }
  scale(k) { this.x *= k; this.y *= k; this.z *= k; return this; }
  getX() { return this.x; }
}
class Vec2 extends Vec {
  len2() { return this.dot(this); }
}
function main() {
  var a = new Vec2(1, 2, 3);
  var b = new Vec2(0.5, 0.25, 0.125);
  var acc = 0;
  for (var i = 0; i < 400000; i++) {
    acc += a.dot(b) + a.len2() + b.getX();
    b.scale(1.0000001);
  }
  return acc.toFixed(3);
}
console.log(main());
