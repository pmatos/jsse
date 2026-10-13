// try/finally with a continue and a return crossing the finalizer.
var counter = 0;
function once(i) {
  try {
    if (i % 3 === 0) return i;
    counter += 1;
  } finally {
    counter += 2;
  }
  return -i;
}
function run(n) {
  var s = 0;
  for (var i = 0; i < n; i++) {
    try {
      if ((i & 7) === 0) continue;
      s += once(i);
    } finally {
      s += 1;
    }
  }
  return s;
}
console.log(run(1000000) + counter);
