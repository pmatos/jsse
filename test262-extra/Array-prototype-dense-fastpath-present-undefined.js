/*---
description: >
  A present `undefined` element is not a hole. Array.prototype methods that
  move, copy or create elements in bulk must keep `undefined` values present
  (own, enumerable, visited by forEach) and must keep real holes absent.
esid: sec-array.prototype.unshift
info: |
  Also covers: sec-array.prototype.splice, sec-array.prototype.fill, sec-array.prototype.map, sec-array.prototype.filter, sec-array.prototype.slice, sec-array.prototype.concat, sec-array.prototype.sort, sec-array.prototype.reverse, sec-createdatapropertyorthrow.
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

var a = [1, 2];
a.unshift(undefined);
eq(show(a), '3:undefined,1,2', 'unshift(undefined) creates a present element');

a = [1, 2, 3];
a.splice(1, 1, undefined, undefined);
eq(show(a), '4:1,undefined,undefined,3', 'splice inserts present undefined');

a = [1, 2, 3, 4];
a.fill(undefined, 1, 3);
eq(show(a), '4:1,undefined,undefined,4', 'fill(undefined) creates present elements');

a = new Array(3);
a.fill(undefined);
eq(show(a), '3:undefined,undefined,undefined', 'fill(undefined) on a sparse receiver');
eq(Object.keys(a).join(), '0,1,2', 'filled undefined elements are enumerable');

var m = [1, 2, 3].map(function () { return undefined; });
eq(show(m), '3:undefined,undefined,undefined', 'map result keeps undefined present');

var f = [1, undefined, 3, undefined].filter(function () { return true; });
eq(show(f), '4:1,undefined,3,undefined', 'filter keeps present undefined');

var s = [1, undefined, 3].slice(0);
eq(show(s), '3:1,undefined,3', 'slice copies present undefined');
var c = [undefined].concat([undefined, 2], undefined);
eq(show(c), '4:undefined,undefined,2,undefined', 'concat copies present undefined');

var removed = [undefined, 1, undefined].splice(0, 3);
eq(show(removed), '3:undefined,1,undefined', 'splice returns present undefined');

var r = [undefined, 1, undefined, 2];
r.reverse();
eq(show(r), '4:2,undefined,1,undefined', 'reverse moves present undefined');

var srt = [3, undefined, 1, undefined, 2];
srt.sort();
eq(show(srt), '5:1,2,3,undefined,undefined', 'sort puts present undefined last and keeps them present');

var srtHole = [3, , 1, undefined, 2];
srtHole.sort();
eq(show(srtHole), '5:1,2,3,undefined,_', 'sort moves holes after undefined');

var visited = 0;
[undefined, undefined].slice(0).forEach(function () { visited++; });
eq(visited, 2, 'copies of present undefined are visited by forEach');

var shifted = [undefined, 1];
eq(shifted.shift(), undefined, 'shift returns undefined');
eq(show(shifted), '1:1', 'shift result');
var popped = [1, undefined];
eq(popped.pop(), undefined, 'pop returns undefined');
eq(show(popped), '1:1', 'pop result');

var holeLast = [1, , ];
eq(holeLast.length, 2, 'trailing elision length');
eq(holeLast.pop(), undefined, 'pop of trailing hole returns undefined');
eq(show(holeLast), '1:1', 'pop of trailing hole shrinks length');

var idx = [undefined, 1];
eq(idx.indexOf(undefined), 0, 'indexOf finds present undefined');
eq([, 1].indexOf(undefined), -1, 'indexOf skips holes');
eq([, 1].includes(undefined), true, 'includes treats holes as undefined');
eq([1, undefined].lastIndexOf(undefined), 1, 'lastIndexOf finds present undefined');
