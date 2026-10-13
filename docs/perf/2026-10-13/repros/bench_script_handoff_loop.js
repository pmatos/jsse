// A top-level hot loop whose body mixes native statements with ones that
// run as tree-walker units (a labelled block, a try/finally and a switch).
var total = 0;
for (var i = 0; i < 1000000; i++) {
  blk: {
    if (i & 1) break blk;
    total = (total + i) | 0;
  }
  try {
    total = (total + 1) | 0;
  } finally {
    total = (total ^ 3) | 0;
  }
  switch (i & 3) {
    case 0: total = (total + 2) | 0; break;
    case 1: total = (total - 1) | 0; break;
    default: total = (total + 5) | 0;
  }
}
if (total === 0x7fffffff) throw new Error("unreachable");
