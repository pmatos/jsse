// Non-constant array literals in a hot function: every evaluation builds a
// fresh array from operands (NewArray). Was one tree-walker hand-off each.
function mk(i) {
  return [i, i + 1, i + 2, i + 3];
}
function main() {
  var s = 0;
  for (var i = 0; i < 1000000; i++) {
    var a = mk(i);
    s += a[0] + a[3];
  }
  return s;
}
console.log(main());
