/*---
description: >
  Array.prototype shift/unshift/splice/reverse/fill/slice/concat/indexOf/
  includes on a dense Array must keep the element-by-element semantics of
  their spec algorithms when a hole meets an indexed property on the prototype
  chain, when the chain holds a Proxy, and when holes are only present in the
  range the algorithm visits. A bulk fast path may only be taken when no
  inherited index can be observed.
esid: sec-array.prototype.shift
info: |
  Also covers: sec-array.prototype.unshift, sec-array.prototype.splice, sec-array.prototype.reverse, sec-array.prototype.fill, sec-array.prototype.slice, sec-array.prototype.concat, sec-array.prototype.indexof, sec-array.prototype.includes, sec-ordinaryset, sec-hasproperty.
---*/

// Built with string concatenation: helper arrays would themselves be affected
// by the indexed properties installed on Array.prototype below.
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
function withProtoIndex(proto, index, desc, body) {
  Object.defineProperty(proto, index, desc);
  try {
    body();
  } finally {
    delete proto[index];
  }
}

// Holes without any inherited index stay holes through every move.
var a = [1, , 3, , 5];
eq(a.shift(), 1, 'shift value');
eq(show(a), '4:_,3,_,5', 'shift keeps holes');
a = [1, , 3];
eq(a.unshift(0), 4, 'unshift length');
eq(show(a), '4:0,1,_,3', 'unshift keeps holes');
a = [1, , 3, 4];
eq(show(a.splice(1, 2, 'x')), '2:_,3', 'splice removed keeps hole');
eq(show(a), '3:1,x,4', 'splice result');
a = [1, , 3];
a.reverse();
eq(show(a), '3:3,_,1', 'reverse keeps hole');
a = [1, , 3, 4];
eq(show(a.slice(1, 3)), '2:_,3', 'slice keeps hole');
eq(show([1, , 3].concat([, 5])), '5:1,_,3,_,5', 'concat keeps holes');

// A hole that meets an inherited index reads through the prototype.
withProtoIndex(Array.prototype, 1, { value: 'P', writable: true, configurable: true }, function () {
  var b = [0, , 2];
  eq(b.indexOf('P'), 1, 'indexOf reads inherited value');
  eq(b.includes('P'), true, 'includes reads inherited value');
  eq(b.lastIndexOf('P'), 1, 'lastIndexOf reads inherited value');
  eq(show(b.slice(0)), '3:0,P,2', 'slice copies inherited value as own');
  eq(b.shift(), 0, 'shift value (inherited)');
  eq(show(b), '2:P,2', 'shift writes inherited value as own');
  var c = [0, , 2];
  c.reverse();
  eq(show(c), '3:2,_,0', 'reverse middle hole is untouched');
  var d = [0, , 2];
  eq(d.unshift('u'), 4, 'unshift (inherited) length');
  eq(show(d), '4:u,0,P,2', 'unshift moves inherited value');
  var e = [0, , 2];
  eq(show(e.splice(0, 1)), '1:0', 'splice removed (inherited)');
  eq(show(e), '2:P,2', 'splice moves inherited value');
  eq(show([0, , 2].concat([])), '3:0,P,2', 'concat copies inherited value of receiver');
  eq(show([0, , 2].concat([9, ,])), '5:0,P,2,9,P', 'concat copies inherited values of items');
});

// An inherited setter at an index the algorithm creates must run.
var setterCalls = [];
withProtoIndex(Array.prototype, 3, {
  get: function () { return 'G'; },
  set: function (v) { setterCalls.push(v); },
  configurable: true
}, function () {
  var b = [1, 2, 3];
  eq(b.unshift('u'), 4, 'unshift through setter length');
  eq(setterCalls.length, 1, 'unshift invokes inherited setter once');
  eq(setterCalls[0], 3, 'unshift setter receives moved value');
  eq(show(b), '4:u,1,2,_', 'unshift leaves index 3 to the setter');
  setterCalls = [];
  var c = [1, 2, 3];
  c.splice(0, 0, 'a');
  eq(setterCalls.length, 1, 'splice insert invokes inherited setter once');
  eq(setterCalls[0], 3, 'splice setter receives moved value');
  setterCalls = [];
  var f = new Array(5);
  f.fill(7, 2, 4);
  eq(setterCalls.length, 1, 'fill invokes inherited setter once');
  eq(setterCalls[0], 7, 'fill setter receives value');
  eq(show(f), '5:_,_,7,_,_', 'fill creates own element only where there is no setter');
});

// A non-writable inherited index makes the creating Set fail.
withProtoIndex(Object.prototype, 2, { value: 'ro', writable: false, configurable: true }, function () {
  var threw = false;
  try {
    [1, 2].fill(0, 0, 2); // indices 0 and 1 exist: fine
    var b = [1, 2];
    b.unshift('a'); // creates index 2, which is read-only through the chain
  } catch (e) {
    threw = e instanceof TypeError;
  }
  eq(threw, true, 'unshift into inherited read-only index throws TypeError');
  var c = new Array(4);
  threw = false;
  try {
    c.fill(1);
  } catch (e) {
    threw = e instanceof TypeError;
  }
  eq(threw, true, 'fill into inherited read-only index throws TypeError');
  eq(show(c), '4:1,1,_,_', 'fill stops at the failing index');
});

// A Proxy on the chain observes the creating Sets.
var trapped = [];
var arr = [1, 2];
Object.setPrototypeOf(arr, new Proxy(Array.prototype, {
  set: function (t, p, v, r) { trapped.push(String(p) + '=' + String(v)); return Reflect.set(t, p, v, r); },
  has: function (t, p) { return Reflect.has(t, p); }
}));
arr.unshift('x');
eq(trapped.join(), '2=2', 'unshift through Proxy prototype reaches the set trap for the new index');
eq(show(arr), '3:x,1,2', 'unshift through Proxy prototype result');
trapped = [];
var filled = new Array(3);
Object.setPrototypeOf(filled, new Proxy(Array.prototype, {
  set: function (t, p, v, r) { trapped.push(String(p)); return Reflect.set(t, p, v, r); }
}));
filled.fill(0);
eq(trapped.join(), '0,1,2', 'fill through Proxy prototype reaches the set trap for each index');

// Holes read through an inherited getter see it exactly once per visit.
var gets = 0;
withProtoIndex(Array.prototype, 0, {
  get: function () { gets++; return 'g'; },
  configurable: true
}, function () {
  var h = [, 1];
  eq(h.indexOf('g'), 0, 'indexOf finds getter value');
  eq(gets, 1, 'indexOf invokes the inherited getter once');
  gets = 0;
  var s = h.slice(0);
  eq(gets, 1, 'slice invokes the inherited getter once');
  eq(show(s), '2:g,1', 'slice copies getter value');
});

// A String wrapper on the chain exposes its characters as inherited, read-only
// index properties even though they are not stored as ordinary properties.
function holey() {
  var h = [];
  for (var i = 0; i < 4; i++) h.push(i + 10);
  delete h[1];
  return h;
}
var viaString = holey();
Object.setPrototypeOf(viaString, Object('abc'));
var sliced = Array.prototype.slice.call(viaString);
eq(show(sliced), '4:10,b,12,13', 'slice reads an inherited String index through a hole');
var concatenated = [].concat(viaString);
eq(show(concatenated), '4:10,b,12,13', 'concat reads an inherited String index through a hole');
viaString = holey();
Object.setPrototypeOf(viaString, Object('abc'));
var threw = false;
try {
  Array.prototype.reverse.call(viaString);
} catch (e) {
  threw = e instanceof TypeError;
}
eq(threw, true, 'reverse onto an inherited read-only String index throws');
viaString = holey();
Object.setPrototypeOf(viaString, Object('abc'));
threw = false;
try {
  Array.prototype.shift.call(viaString);
} catch (e) {
  threw = e instanceof TypeError;
}
eq(threw, true, 'shift onto an inherited read-only String index throws');
