function h(n) {
  var s = 0;
  for (var i = 0; i < n; i++) {
    switch (i & 3) {
      case 0: { const x = i * 2; s += x; break; }
      case 1: { const y = i + 7; s -= y; break; }
      case 2: { let z = i ^ 5; s += z; break; }
      default: s += 1;
    }
  }
  return s;
}
var t = 0;
for (var r = 0; r < 5; r++) t += h(400000);
print(t);
