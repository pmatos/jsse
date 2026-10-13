// for-of over a Map (entries destructured) and a Set, plus break out of a loop.
var m = new Map();
var st = new Set();
for (var i = 0; i < 200; i++) { m.set("k" + i, i); st.add(i); }
function run(n) {
  var s = 0;
  for (var r = 0; r < n; r++) {
    for (const [k, v] of m) s += v;
    for (const v of st) { if (v > 150) break; s += v; }
  }
  return s;
}
console.log(run(5000));
