// Array operations — push, map, reduce
var arr = [];
for (var i = 0; i < 100000; i++) {
  arr.push(i);
}
var mapped = arr.map(function(x) { return x * 2; });
var total = mapped.reduce(function(a, b) { return a + b; }, 0);
console.log(total);
