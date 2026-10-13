// Constant array literals (lookup tables built per call): NewArrayConst.
function table(i) {
  var t = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
  return t[i & 15];
}
function main() {
  var s = 0;
  for (var i = 0; i < 1000000; i++) s += table(i);
  return s;
}
console.log(main());
