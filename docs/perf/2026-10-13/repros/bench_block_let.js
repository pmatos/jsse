function f(n) {
  let s = 0;
  for (let i = 0; i < n; i++) {
    const a = i * 2;
    const b = a + 1;
    if (b % 3 === 0) { let c = b >> 1; s += c; } else { s += b; }
  }
  return s;
}
var t = 0;
for (var r = 0; r < 5; r++) t += f(400000);
print(t);
