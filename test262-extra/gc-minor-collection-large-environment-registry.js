/*---
description: >
  A young object stored into one of many old closure environments keeps its
  identity across the minor collections of a heap whose environment registry
  has outgrown the base nursery.
info: |
  The generational collector remembers every environment a collection found
  free of nursery references and re-scans, at each minor collection, the ones
  written to since. With a registry of tens of thousands of environments the
  collector lets the nursery grow so that this scan stays cheap per allocation
  (#1490); the longer interval between collections must not lose a store. Only
  the observable result is asserted: every read returns the object last stored,
  for cells spread over the whole registry, and untouched cells keep theirs.
esid: sec-environment-records
features: [host-gc-required]
---*/

var CELLS = 40000;
var sink;

// JSON.parse allocates the objects natively: no script statement runs per
// object, which keeps the cost small under JSSE_GC_STRESS (it collects at
// every Nth statement boundary or loop back-edge).
var churnSource = "[" + "{},".repeat(12000) + "{}]";
function minor() {
  for (var j = 0; j < 8; j++) {
    sink = JSON.parse(churnSource);
  }
  sink = undefined;
}

var cells = Array.from({ length: CELLS }, (_, seed) => ({
  set: (value) => (seed = value),
  get: () => seed,
}));

// Every environment is now old, clean and registered.
$262.gc();
minor();

var stored = [];
for (var round = 0; round < 4; round++) {
  for (var k = round; k < CELLS; k += 997) {
    var value = { round: round, cell: k, payload: [k, round] };
    cells[k].set(value);
    stored[k] = value;
  }
  minor();
  if (round % 2 === 1) $262.gc();
  for (var k = round; k < CELLS; k += 997) {
    var got = cells[k].get();
    assert.sameValue(got, stored[k], "cell " + k + " round " + round + ": identity");
    assert.sameValue(got.payload[0], k, "cell " + k + " round " + round + ": payload");
  }
}

for (var k = 0; k < CELLS; k += 1013) {
  var held = cells[k].get();
  if (stored[k] === undefined) {
    assert.sameValue(held, k, "untouched cell " + k);
  } else {
    assert.sameValue(held, stored[k], "stored cell " + k + " at the end");
  }
}
