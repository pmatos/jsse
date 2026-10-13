// for-of over arrays with a const head, plain and with a closure capture.
function sum(a) {
  var s = 0;
  for (const x of a) s += x;
  return s;
}
function pairs(a) {
  var s = 0;
  for (const [k, v] of a) s += k * v;
  return s;
}
var nums = [];
for (var i = 0; i < 100; i++) nums.push(i);
var kv = nums.map(function (n) { return [n, n + 1]; });
var total = 0;
for (var r = 0; r < 20000; r++) total += sum(nums) + pairs(kv) % 7;
console.log(total);
