// Closure creation and invocation
function makeAdder(x) {
  return function(y) { return x + y; };
}
var sum = 0;
for (var i = 0; i < 500000; i++) {
  var add5 = makeAdder(5);
  sum += add5(i);
}
console.log(sum);
