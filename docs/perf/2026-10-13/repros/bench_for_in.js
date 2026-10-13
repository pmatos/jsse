// for-in over small objects, with and without a lexical head.
var o = { a: 1, b: 2, c: 3, d: 4, e: 5, f: 6, g: 7, h: 8 };
function run(n) {
  var s = 0;
  for (var i = 0; i < n; i++) {
    for (var k in o) s += o[k];
    for (let j in o) s -= 1;
  }
  return s;
}
console.log(run(150000));
