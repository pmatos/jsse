/*---
description: >
  Objects stored into the elements of a large, long-lived Array stay reachable
  and keep their identity across minor collections, however the elements were
  written: indexed stores that overwrite, fill a hole or append, push, and
  element-moving operations (reverse, splice, sort) between those stores.
info: |
  The generational collector remembers an old Array's element writes per
  card (a fixed-size run of elements) rather than re-scanning the whole
  vector. A store that skipped its card, or an operation that moved a young
  element into a card nobody had marked, would let the collector free an
  object the Array still references: later reads would see a wrong value or
  throw. Only the observable result is asserted here: every slot must still
  hold the object last written to it.
esid: sec-array-exotic-objects
---*/

var seed = 12345;
function next(n) {
  seed = (Math.imul(seed, 1103515245) + 12345) & 0x7fffffff;
  return seed % n;
}

function churn() {
  // Short-lived garbage so the nursery fills and minor collections run.
  var junk;
  for (var k = 0; k < 64; k++) junk = { k: k, next: junk };
  return junk;
}

var N = 2500;
var table = new Array(N);
var expected = new Array(N);
for (var i = 0; i < N; i++) {
  table[i] = { id: i };
  expected[i] = i;
}

// Promote the table and its records, so that later stores are old-to-young.
$262.gc();
$262.gc();

function checkAll(label) {
  assert.sameValue(table.length, expected.length, label + ": length");
  for (var i = 0; i < expected.length; i++) {
    var record = table[i];
    assert.notSameValue(record, undefined, label + ": slot " + i + " is present");
    assert.sameValue(record.id, expected[i], label + ": slot " + i);
  }
}

var serial = N;
function overwrite(count) {
  for (var r = 0; r < count; r++) {
    var index = next(expected.length);
    table[index] = { id: serial };
    expected[index] = serial++;
    if (r % 16 === 0) churn();
  }
}

overwrite(4000);
checkAll("after random overwrites");

// Appends reach cards beyond the ones the Array had when it was promoted.
for (var a = 0; a < 600; a++) {
  table[table.length] = { id: serial };
  expected[expected.length] = serial++;
  if (a % 8 === 0) churn();
}
checkAll("after indexed appends");

for (var p = 0; p < 600; p++) {
  table.push({ id: serial }, { id: serial + 1 });
  expected.push(serial, serial + 1);
  serial += 2;
  if (p % 8 === 0) churn();
}
checkAll("after push");

// A young record is written into one card, then moved into another by an
// operation that does not name either card.
for (var round = 0; round < 3; round++) {
  overwrite(600);
  var low = next(100);
  table[low] = { id: serial };
  expected[low] = serial++;
  table.reverse();
  expected.reverse();
  churn();
  checkAll("after reverse " + round);

  var at = next(expected.length - 50);
  table[at + 1] = { id: serial };
  expected[at + 1] = serial++;
  var cutTable = table.splice(at, 5);
  var cutExpected = expected.splice(at, 5);
  assert.sameValue(cutTable.length, cutExpected.length, "splice removed count");
  for (var c = 0; c < cutTable.length; c++) {
    assert.sameValue(cutTable[c].id, cutExpected[c], "spliced element " + c);
  }
  churn();
  checkAll("after splice " + round);
}

var sortIndex = next(expected.length);
table[sortIndex] = { id: serial };
expected[sortIndex] = serial++;
table.sort(function (x, y) { return y.id - x.id; });
expected.sort(function (x, y) { return y - x; });
churn();
checkAll("after sort");

$262.gc();
checkAll("after a full collection");
