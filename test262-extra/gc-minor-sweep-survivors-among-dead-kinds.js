/*---
description: >
  Objects of every common kind that survive a minor collection keep their
  contents while the dead objects around them in the nursery are freed, and
  keep them as they age into the old generation.
info: |
  The sweep of a minor collection (#1490) walks the nursery in allocation
  order, frees each object the trace did not reach and ages the rest, so a
  survivor sits between dead neighbours of other kinds (Arrays, closures,
  Arguments objects, typed arrays, Maps). Only the observable result is
  asserted: every retained object reads back what was stored, across enough
  collections to promote it.
esid: sec-weak-ref-objects
features: [host-gc-required]
---*/

// JSON.parse allocates natively: no script statement runs per object, which
// keeps the cost small under JSSE_GC_STRESS. The result is dropped at once, so
// the collection the allocation requests (at the next statement boundary)
// frees the whole churn.
var churnSource = "[" + "{},".repeat(12000) + "{}]";
function minor() {
  JSON.parse(churnSource);
}

function make(kind, i) {
  switch (kind) {
    case 0:
      return [i, i + 1, "s" + i];
    case 1:
      return { id: i, tag: "t" + i, nested: { v: i } };
    case 2:
      return function (x) {
        return x + i;
      };
    case 3:
      return (function () {
        return arguments;
      })(i, i + 1);
    case 4:
      return new Uint8Array([i & 255, 1, 2]);
    default:
      return new Map([[i, "m" + i]]);
  }
}

function verify(kind, value, i, where) {
  switch (kind) {
    case 0:
      assert.sameValue(value.length, 3, where + " array length " + i);
      assert.sameValue(value[0], i, where + " array[0] " + i);
      assert.sameValue(value[1], i + 1, where + " array[1] " + i);
      assert.sameValue(value[2], "s" + i, where + " array[2] " + i);
      break;
    case 1:
      assert.sameValue(value.id, i, where + " object id " + i);
      assert.sameValue(value.tag, "t" + i, where + " object tag " + i);
      assert.sameValue(value.nested.v, i, where + " nested " + i);
      break;
    case 2:
      assert.sameValue(value(1), i + 1, where + " closure " + i);
      break;
    case 3:
      assert.sameValue(value.length, 2, where + " arguments length " + i);
      assert.sameValue(value[0], i, where + " arguments[0] " + i);
      assert.sameValue(value[1], i + 1, where + " arguments[1] " + i);
      break;
    case 4:
      assert.sameValue(value[0], i & 255, where + " typed array " + i);
      assert.sameValue(value[2], 2, where + " typed array tail " + i);
      break;
    default:
      assert.sameValue(value.get(i), "m" + i, where + " map " + i);
  }
}

// Enough live old objects that a major collection is not due after every minor
// one, and that the sweep prefetches (it does from 65,536 live objects).
var ballast = JSON.parse("[" + "{},".repeat(70000) + "{}]");
$262.gc();

var kept = [];
var serial = 0;

for (var round = 0; round < 8; round++) {
  // Six kinds interleaved; every fifth object is retained, the others die.
  for (var n = 0; n < 1800; n++) {
    var kind = n % 6;
    var i = serial++;
    var value = make(kind, i);
    if (n % 5 === 0) {
      kept.push({ kind: kind, i: i, value: value });
    }
  }
  minor();
  for (var k = 0; k < kept.length; k++) {
    var entry = kept[k];
    verify(entry.kind, entry.value, entry.i, "round " + round + " entry " + k + ":");
  }
}

$262.gc();
for (var k = 0; k < kept.length; k++) {
  var entry = kept[k];
  verify(entry.kind, entry.value, entry.i, "after full collection, entry " + k + ":");
}
