/*---
description: >
  Objects reachable only through a captured environment stay alive across any
  number of minor and major collections, including environments shared by
  many closures, nested chains, and environments that point back at their
  own closure.
info: |
  The collector walks each closure's environment chain once per collection and
  must visit it again in the next one: an environment seen in an earlier
  collection is not exempt from later ones. Only the observable result is
  asserted: every captured value must still hold the object last stored.
esid: sec-environment-records
---*/

function churn() {
  var junk;
  for (var k = 0; k < 200; k++) junk = { k: k, next: junk };
  return junk;
}

function makeCell(id) {
  var held = { id: id };
  return {
    get: function () { return held; },
    set: function (value) { held = value; },
  };
}

var cells = [];
for (var i = 0; i < 400; i++) cells.push(makeCell(i));

function checkCells(label, expected) {
  for (var i = 0; i < cells.length; i++) {
    var held = cells[i].get();
    assert.notSameValue(held, undefined, label + ": cell " + i + " present");
    assert.sameValue(held.id, expected[i], label + ": cell " + i);
  }
}

var expected = cells.map(function (_, i) { return i; });
checkCells("before collection", expected);

$262.gc();
checkCells("after the first collection", expected);

// Stores into environments that earlier collections already walked, with
// collections both between the stores and after them.
var serial = cells.length;
for (var round = 0; round < 6; round++) {
  for (var i = round % 3; i < cells.length; i += 3) {
    cells[i].set({ id: serial });
    expected[i] = serial++;
  }
  churn();
  if (round % 2 === 0) $262.gc();
  checkCells("round " + round, expected);
}
$262.gc();
$262.gc();
checkCells("after repeated collections", expected);

// One parent environment shared by many closures, each with its own child
// environment, mutated through a sibling after the parent was promoted.
var shared = (function () {
  var box = { id: "initial" };
  var readers = [];
  for (var i = 0; i < 300; i++) {
    (function (n) {
      var own = { n: n };
      readers.push(function () { return [box, own]; });
    })(i);
  }
  return {
    readers: readers,
    replace: function (value) { box = value; },
  };
})();

$262.gc();
$262.gc();
for (var round = 0; round < 4; round++) {
  var replacement = { id: "round" + round };
  shared.replace(replacement);
  churn();
  $262.gc();
  for (var i = 0; i < shared.readers.length; i++) {
    var pair = shared.readers[i]();
    assert.sameValue(pair[0], replacement, "shared box, round " + round + ", reader " + i);
    assert.sameValue(pair[1].n, i, "own environment, round " + round + ", reader " + i);
  }
}

// A deep chain of nested environments with a value at the outermost level.
function nest(depth) {
  var level = { depth: depth };
  if (depth === 0) {
    return {
      read: function () { return level; },
      write: function (value) { level = value; },
    };
  }
  var inner = nest(depth - 1);
  return {
    read: function () { return [level, inner.read()]; },
    write: function (value) { inner.write(value); },
  };
}

var chain = nest(60);
function deepest(value) {
  while (Array.isArray(value)) value = value[1];
  return value;
}
$262.gc();
for (var round = 0; round < 3; round++) {
  var marker = { marker: round };
  chain.write(marker);
  churn();
  $262.gc();
  assert.sameValue(deepest(chain.read()), marker, "nested chain, round " + round);
}

// An environment that holds a function whose own closure is that environment.
var selfReference = (function () {
  var self = function () { return self; };
  var payload = { payload: true };
  return {
    check: function () {
      assert.sameValue(self(), self, "self reference");
      return payload;
    },
    swap: function (value) { payload = value; },
  };
})();

$262.gc();
var fresh = { payload: "fresh" };
selfReference.swap(fresh);
churn();
$262.gc();
$262.gc();
assert.sameValue(selfReference.check(), fresh, "payload behind a self-referencing closure");
