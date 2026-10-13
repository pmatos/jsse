function g(n) {
  var fs = [];
  var total = 0;
  for (let i = 0; i < n; i++) {
    const k = i + 1;
    if (i % 100 === 0) fs.push(function () { return k + i; });
    total += k;
  }
  return total + fs.length;
}
var t = 0;
for (var r = 0; r < 5; r++) t += g(300000);
print(t);
