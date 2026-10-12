/*---
description: >
  Callback-taking Array.prototype methods and the argument conversions of the
  bulk-moving methods may mutate the receiver while the algorithm runs. Every
  element read must observe the receiver as it is at that moment, and a bulk
  fast path may only act on the state checked right before it starts.
esid: sec-array.prototype.foreach
info: |
  Also covers: sec-array.prototype.map, sec-array.prototype.filter, sec-array.prototype.some, sec-array.prototype.reduce, sec-array.prototype.find, sec-array.prototype.indexof, sec-array.prototype.splice, sec-array.prototype.slice, sec-array.prototype.sort.
---*/

function show(a) {
  var out = a.length + ':';
  for (var i = 0; i < a.length; i++) {
    if (i > 0) out += ',';
    out += Object.prototype.hasOwnProperty.call(a, i) ? String(a[i]) : '_';
  }
  return out;
}
function eq(actual, expected, msg) {
  if (actual !== expected) {
    throw new Test262Error(msg + ': expected ' + expected + ', got ' + actual);
  }
}

// forEach: the visited range is fixed by the initial length, but each element
// is read at visit time. Deleted elements are skipped, shrinking stops reads.
var seen = '';
var a = [1, 2, 3, 4, 5];
a.forEach(function (v, i) {
  seen += v + ';';
  if (i === 0) {
    delete a[1];
    a.length = 4;
    a[2] = 'x';
  }
});
eq(seen, '1;x;4;', 'forEach observes deletion, truncation and overwrite');

// map: elements appended during the walk are not visited; holes created by the
// callback stay holes in the result.
a = [1, 2, 3];
var mapped = a.map(function (v, i) {
  if (i === 0) {
    a.push(99);
    delete a[2];
  }
  return v * 10;
});
eq(show(mapped), '3:10,20,_', 'map result for deleted element is a hole');

// filter/some/every/find/findIndex/findLast re-read each element.
a = [1, 2, 3, 4];
var kept = a.filter(function (v, i) {
  if (i === 0) a[3] = 40;
  return v > 1;
});
eq(show(kept), '3:2,3,40', 'filter reads the overwritten element');

a = [1, 2, 3];
eq(a.some(function (v, i) { if (i === 0) a[2] = 'hit'; return v === 'hit'; }), true, 'some reads the overwritten element');
a = [1, 2, 3];
eq(a.every(function (v, i) { if (i === 0) a.length = 1; return true; }), true, 'every over truncated array');
a = [1, 2, 3];
eq(a.find(function (v, i) { if (i === 0) a[1] = 'f'; return v === 'f'; }), 'f', 'find reads the overwritten element');
a = [1, 2, 3];
eq(a.findIndex(function (v, i) { if (i === 0) a.length = 1; return v === undefined; }), 1, 'findIndex sees undefined for removed elements');
a = [1, 2, 3];
eq(a.findLast(function (v, i) { if (i === 2) a[0] = 'l'; return v === 'l'; }), 'l', 'findLast reads the overwritten element');

// reduce/reduceRight.
a = [1, 2, 3];
eq(a.reduce(function (acc, v, i) { if (i === 1) a[2] = 30; return acc + v; }), 33, 'reduce reads the overwritten element');
a = [1, 2, 3];
eq(a.reduceRight(function (acc, v, i) { if (i === 2) delete a[0]; return acc + v; }, 0), 5, 'reduceRight skips the deleted element');

// A callback that turns an element into an accessor is observed.
a = [1, 2, 3];
var getterCalls = 0;
var total = 0;
a.forEach(function (v, i) {
  if (i === 0) {
    Object.defineProperty(a, 2, { get: function () { getterCalls++; return 100; }, configurable: true });
  }
  total += v;
});
eq(total, 103, 'forEach calls the accessor installed during the walk');
eq(getterCalls, 1, 'accessor read exactly once');

// Argument conversions that mutate the receiver before the bulk step.
a = [1, 2, 3, 4];
var grow = { valueOf: function () { a.push(5, 6); return 1; } };
eq(show(a.splice(grow, 2)), '2:2,3', 'splice start/deleteCount conversion grows the receiver');
eq(show(a), '2:1,4', 'splice keeps the stale length captured before the conversion');

a = [1, 2, 3, 4];
var shrink = { valueOf: function () { a.length = 2; return 0; } };
eq(show(a.slice(shrink, 4)), '4:1,2,_,_', 'slice end conversion shrinks the receiver');

a = [1, 2, 3, 4];
var shrinkIdx = { valueOf: function () { a.length = 1; return 0; } };
eq(a.indexOf(4, shrinkIdx), -1, 'indexOf fromIndex conversion shrinks the receiver');
a = [1, 2, 3, 4];
var shrinkIncl = { valueOf: function () { a.length = 1; return 0; } };
eq(a.includes(undefined, shrinkIncl), true, 'includes sees removed elements as undefined');
a = [1, 2, 3, 4];
a.fill('f', 0, { valueOf: function () { a.length = 2; return 4; } });
eq(show(a), '4:f,f,f,f', 'fill end conversion shrinks receiver, fill re-extends it');

// A comparator that mutates the receiver: the write-back must still assign
// every sorted value by index.
a = [3, 1, 2];
a.sort(function (x, y) { a.length = 0; return x - y; });
eq(show(a), '3:1,2,3', 'sort write-back after the comparator emptied the receiver');
a = [3, 1, 2];
a.sort(function (x, y) { delete a[0]; return x - y; });
eq(show(a), '3:1,2,3', 'sort write-back after the comparator deleted an element');
