/*---
description: >
  A young object stored into an old closure environment stays reachable across
  several minor collections without further writes, and across the death and
  slot reuse of other environments between them.
info: |
  A minor collection re-scans the environments written since the last
  collection (#1490), found through a list the first write after a collection
  fills, not by visiting every environment. An environment that still holds a
  nursery object after a collection must be scanned by the next one too, and
  the registry slot of an environment that died is reused by the next one a
  collection registers. Only the observable result is asserted: every read
  returns the object last stored, with its payload intact.
esid: sec-environment-records
features: [host-gc-required]
---*/

var sink;

// JSON.parse allocates natively: no script statement runs per object, which
// keeps the cost small under JSSE_GC_STRESS.
var churnSource = "[" + "{},".repeat(12000) + "{}]";
function minor() {
  sink = JSON.parse(churnSource);
  sink = undefined;
}

function makeCells(count, base) {
  return Array.from({ length: count }, (_, seed) => {
    var held = base + seed;
    return {
      set: (value) => (held = value),
      get: () => held,
    };
  });
}

// Enough live objects that a major collection is not due after every minor one.
var keep = makeCells(12000, 0);
$262.gc();

// The environments are the only references to the stored objects.
for (var k = 0; k < keep.length; k += 3) {
  keep[k].set({ cell: k, payload: [k, "a"] });
}

// The stored objects are young; no store happens while the collections age
// them, and the environments holding them must be scanned each time.
for (var round = 0; round < 3; round++) {
  minor();
  for (var k = 0; k < keep.length; k += 3) {
    var got = keep[k].get();
    assert.sameValue(got.cell, k, "kept cell " + k + " after minor " + round);
    assert.sameValue(got.payload[0], k, "kept cell " + k + " payload " + round);
    assert.sameValue(got.payload[1], "a", "kept cell " + k + " payload tag " + round);
  }
}

// Environments that die, and new ones that take their registry slots.
for (var generation = 0; generation < 4; generation++) {
  var transient = makeCells(3000, 100000 * generation);
  $262.gc();
  for (var k = 0; k < transient.length; k += 2) {
    transient[k].set({ generation: generation, cell: k });
  }
  minor();
  for (var k = 0; k < transient.length; k += 2) {
    var seen = transient[k].get();
    assert.sameValue(seen.generation, generation, "transient generation " + generation);
    assert.sameValue(seen.cell, k, "transient cell " + k);
  }
  for (var k = 1; k < transient.length; k += 2) {
    assert.sameValue(transient[k].get(), 100000 * generation + k, "untouched transient " + k);
  }
  transient = undefined;
  $262.gc();

  var k2 = (generation * 7) % keep.length;
  keep[k2].set({ cell: k2, payload: [k2, "a"], replaced: generation });
  minor();
  assert.sameValue(keep[k2].get().replaced, generation, "kept cell " + k2 + " after generation " + generation);
}

for (var k = 0; k < keep.length; k += 3) {
  var last = keep[k].get();
  assert.sameValue(last.cell, k, "kept cell " + k + " at the end");
  assert.sameValue(last.payload[0], k, "kept cell " + k + " payload at the end");
}
