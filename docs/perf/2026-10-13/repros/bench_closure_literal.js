// Function and arrow literals in a hot function, one of them assigned to a
// variable (NamedEvaluation): MakeClosure.
function main() {
  var s = 0;
  for (var i = 0; i < 500000; i++) {
    var add = function (y) { return i + y; };
    var twice = (y) => y * 2;
    s += add(1) + twice(i & 7);
  }
  return s;
}
console.log(main());
