function makeAdder(x) { return function (y) { return x + y; }; }
function main() {
  var sum = 0;
  for (var i = 0; i < 300000; i++) { var add5 = makeAdder(i & 15); sum += add5(i & 255); }
  return sum;
}
console.log(main());
