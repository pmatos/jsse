/*---
description: >
  Bulk fast paths of Array.prototype methods must honour ArraySpeciesCreate
  results of any shape and the integrity attributes of the receiver: frozen,
  sealed and non-extensible arrays, non-writable `length`, and elements with
  non-default attributes.
esid: sec-arrayspeciescreate
info: |
  Also covers: sec-array.prototype.slice, sec-array.prototype.splice, sec-array.prototype.map, sec-array.prototype.shift, sec-array.prototype.unshift, sec-array.prototype.pop, sec-array.prototype.reverse, sec-array.prototype.fill, sec-array.prototype.sort, sec-array-exoticobject-defineownproperty-p-desc.
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
function throwsType(f, msg) {
  try {
    f();
  } catch (e) {
    if (e instanceof TypeError) return;
    throw new Test262Error(msg + ': expected TypeError, got ' + e);
  }
  throw new Test262Error(msg + ': expected TypeError, nothing thrown');
}
function species(make) {
  var a = [1, 2, 3, 4];
  a.constructor = {};
  a.constructor[Symbol.species] = make;
  return a;
}

// Subclass results keep their prototype and contents.
class Sub extends Array {}
var sub = Sub.from([1, 2, 3, 4]);
var sl = sub.slice(1, 3);
eq(sl instanceof Sub, true, 'slice result is a Sub');
eq(show(sl), '2:2,3', 'slice of subclass contents');
var sp = sub.splice(1, 2, 'a');
eq(sp instanceof Sub, true, 'splice result is a Sub');
eq(show(sp), '2:2,3', 'splice of subclass removed');
eq(show(sub), '3:1,a,4', 'splice of subclass receiver');
eq(sub.concat([5]) instanceof Sub, true, 'concat result is a Sub');
eq(sub.map(function (x) { return x; }) instanceof Sub, true, 'map result is a Sub');

// A species constructor result that is not a plain empty Array.
var made = [];
var s1 = species(function (n) { var r = ['pre']; made.push(n); return r; });
var res = s1.slice(1, 3);
eq(show(res), '2:2,3', 'slice into non-empty species result overwrites index 0');
eq(made.join(), '2', 'species constructed with the copy count');

var s2 = species(function (n) { return Object.freeze([]); });
throwsType(function () { s2.slice(0, 2); }, 'slice into frozen species result');
var s3 = species(function (n) { return Object.freeze([]); });
throwsType(function () { s3.map(function (x) { return x; }); }, 'map into frozen species result');
var s4 = species(function (n) { return Object.freeze([]); });
throwsType(function () { s4.filter(function () { return true; }); }, 'filter into frozen species result');
var s5 = species(function (n) { return Object.preventExtensions([]); });
throwsType(function () { s5.splice(0, 1); }, 'splice into non-extensible species result');

var s6 = species(function (n) {
  var r = [];
  Object.defineProperty(r, 'length', { writable: false });
  return r;
});
throwsType(function () { s6.slice(0, 2); }, 'slice into species result with read-only length');

// A species result with an accessor at an index it will receive.
var trapped = [];
var s7 = species(function (n) {
  var r = [];
  Object.defineProperty(r, 0, { set: function (v) { trapped.push(v); }, configurable: false });
  return r;
});
throwsType(function () { s7.slice(0, 2); }, 'slice cannot redefine a non-configurable accessor');
eq(trapped.length, 0, 'CreateDataProperty never invokes setters');

// Species result that is a Proxy: define traps run in order.
var defined = [];
var s8 = species(function (n) {
  return new Proxy([], {
    defineProperty: function (t, k, d) { defined.push(k); return Reflect.defineProperty(t, k, d); }
  });
});
s8.slice(0, 3);
eq(defined.join(), '0,1,2,length', 'proxy species result sees defineProperty per element and length');

// Species result that is the receiver itself.
var self = [1, 2, 3, 4];
self.constructor = {};
self.constructor[Symbol.species] = function () { return self; };
var selfRes = self.slice(1, 3);
eq(selfRes === self, true, 'species may return the receiver');
eq(show(self), '2:2,3', 'slice writes into the receiver');

// Frozen / sealed / non-extensible receivers.
throwsType(function () { Object.freeze([1, 2, 3]).shift(); }, 'shift on frozen');
throwsType(function () { Object.freeze([1, 2, 3]).pop(); }, 'pop on frozen');
throwsType(function () { Object.freeze([1, 2, 3]).unshift(0); }, 'unshift on frozen');
throwsType(function () { Object.freeze([1, 2, 3]).reverse(); }, 'reverse on frozen');
throwsType(function () { Object.freeze([1, 2, 3]).fill(0); }, 'fill on frozen');
throwsType(function () { Object.freeze([3, 1, 2]).sort(); }, 'sort on frozen');
throwsType(function () { Object.freeze([1, 2, 3]).splice(0, 1); }, 'splice on frozen');
throwsType(function () { Object.seal([1, 2, 3]).pop(); }, 'pop on sealed');
throwsType(function () { Object.seal([1, 2, 3]).shift(); }, 'shift on sealed');
throwsType(function () { Object.seal([1, 2, 3]).unshift(0); }, 'unshift on sealed');
var sealed = Object.seal([3, 1, 2]);
sealed.sort();
eq(show(sealed), '3:1,2,3', 'sort on sealed arrays (writable elements)');
sealed.reverse();
eq(show(sealed), '3:3,2,1', 'reverse on sealed arrays');
sealed.fill(0);
eq(show(sealed), '3:0,0,0', 'fill on sealed arrays');

var ne = Object.preventExtensions([1, 2, 3]);
throwsType(function () { ne.unshift(0); }, 'unshift on non-extensible');
eq(show(ne), '3:1,2,3', 'failed unshift leaves a non-extensible receiver intact before the failing index');
ne = Object.preventExtensions([1, 2, 3]);
eq(ne.shift(), 1, 'shift on non-extensible');
eq(show(ne), '2:2,3', 'shift result on non-extensible');
throwsType(function () { Object.preventExtensions([1, 2, 3]).splice(1, 0, 'x'); }, 'splice insert on non-extensible');
var neSplice = Object.preventExtensions([1, 2, 3]);
eq(show(neSplice.splice(0, 1)), '1:1', 'splice delete on non-extensible');
throwsType(function () { Object.preventExtensions(new Array(3)).fill(1); }, 'fill holes of non-extensible');

// Non-writable length: element removal happens first, then the length Set fails.
var ro = [1, 2, 3];
Object.defineProperty(ro, 'length', { writable: false });
throwsType(function () { ro.pop(); }, 'pop with read-only length');
eq(show(ro), '3:1,2,_', 'pop deleted the last element before failing');
ro = [1, 2, 3];
Object.defineProperty(ro, 'length', { writable: false });
throwsType(function () { ro.push(4); }, 'push with read-only length');
ro = [1, 2, 3];
Object.defineProperty(ro, 'length', { writable: false });
throwsType(function () { ro.unshift(0); }, 'unshift with read-only length');
ro = [3, 1, 2];
Object.defineProperty(ro, 'length', { writable: false });
ro.sort();
eq(show(ro), '3:1,2,3', 'sort with read-only length still sorts');
ro.reverse();
eq(show(ro), '3:3,2,1', 'reverse with read-only length still reverses');
ro.fill(7);
eq(show(ro), '3:7,7,7', 'fill with read-only length still fills');

// Elements with non-default attributes.
var attr = [4, 3, 2, 1];
Object.defineProperty(attr, 1, { value: 3, writable: false, enumerable: true, configurable: true });
throwsType(function () { attr.reverse(); }, 'reverse over a read-only element');
attr = [3, 2, 1];
Object.defineProperty(attr, 2, { value: 1, writable: true, enumerable: true, configurable: false });
throwsType(function () { attr.pop(); }, 'pop of a non-configurable element');
eq(show(attr), '3:3,2,1', 'failed pop leaves the receiver intact');
attr = [3, 2, 1];
Object.defineProperty(attr, 0, { value: 3, writable: true, enumerable: false, configurable: true });
var cp = attr.slice(0);
var d = Object.getOwnPropertyDescriptor(cp, 0);
eq(d.enumerable, true, 'slice copies create default attributes');
attr.fill(9);
eq(Object.getOwnPropertyDescriptor(attr, 0).enumerable, false, 'fill keeps the attributes of an existing element');

// Moving a present element onto a hole creates a property, which a
// non-extensible receiver rejects.
throwsType(function () { Object.preventExtensions([, 2, 3]).shift(); }, 'shift onto a hole of a non-extensible array');
throwsType(function () { Object.preventExtensions([1, , 3, 4]).reverse(); }, 'reverse across a hole of a non-extensible array');
throwsType(function () { Object.preventExtensions([3, , 1]).sort(); }, 'sort write-back onto a hole of a non-extensible array');
var neHole = Object.preventExtensions([1, , 3]);
eq(show(neHole.slice(0)), '3:1,_,3', 'slice of a holey non-extensible array');
