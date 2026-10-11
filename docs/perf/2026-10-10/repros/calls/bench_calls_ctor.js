// Constructor calls: `new F(a, b)` assigning 3 fields, function-style and class-style.
// Measured 2026-10-10 on buildbox3 (jsse 2b621c5 v0.11.0 vs boa 0.22.0, median of 5): jsse 3.03 s / boa 1.97 s = 1.53x; jsse --no-bytecode 4.18 s; loadavg 4.6.
// Models: raytrace (Vector/Color/Ray allocation), deltablue (Variable/Constraint), richards (TaskControlBlock).
function P(x, y) { this.x = x; this.y = y; this.sum = x + y; }
class Q { constructor(x, y) { this.x = x; this.y = y; this.sum = x + y; } }
function main() {
  var t = 0;
  for (var i = 0; i < 300000; i++) {
    var p = new P(i, 1);
    var q = new Q(i, 2);
    t += p.sum + q.sum;
  }
  return t;
}
console.log(main());
