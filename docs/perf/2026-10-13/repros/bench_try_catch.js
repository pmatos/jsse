// try/catch in a hot loop, throwing rarely (1 in 1024) from the try block.
function run(n) {
  var s = 0;
  for (var i = 0; i < n; i++) {
    try {
      s += i & 3;
      if ((i & 1023) === 1023) throw i;
    } catch (e) {
      s -= 1;
    }
  }
  return s;
}
console.log(run(3000000));
