// A Script whose top level declares a function and a class, then runs a hot
// loop over global bindings. The declarations used to keep the whole Script
// on the tree-walker.
function step(x) { return x + 1; }
class Unused { m() { return 1; } }
var total = 0;
for (var i = 0; i < 3000000; i++) {
  total = (total + step(i)) | 0;
  if ((i & 1023) === 0) total ^= i;
}
if (total === 0x7fffffff) throw new Error("unreachable");
