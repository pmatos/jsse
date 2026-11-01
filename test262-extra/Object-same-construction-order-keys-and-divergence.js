// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-ordinaryownpropertykeys
description: >
  Objects built by the same constructor keep independent own properties and
  creation order when they diverge afterwards: extra keys, deletions of the
  last or of an earlier key, re-adding, more keys than any shared bookkeeping
  covers, and a read site that sees objects of several layouts.
info: |
  10.1.11.1 OrdinaryOwnPropertyKeys ( O )
    3. For each own property key P of O such that P is a String and P is not an
       array index, in ascending chronological order of property creation, ...
  10.1.10.1 OrdinaryDelete ( O, P )
  10.1.8.1 OrdinaryGet ( O, P, Receiver )
includes: [compareArray.js]
features: [Reflect]
---*/

function Node(kind, pos) {
  this.kind = kind;
  this.pos = pos;
  this.end = pos + 1;
  this.flags = 0;
  this.parent = undefined;
}

function readEnd(o) { return o.end; }
function readExtra(o) { return o.extra; }

var base = ["kind", "pos", "end", "flags", "parent"];
var a = new Node(1, 10), b = new Node(2, 20), c = new Node(3, 30);
assert.compareArray(Reflect.ownKeys(a), base, "fresh instance");

a.extra = "a";
b.other = "b";
b.extra = "b2";
assert.compareArray(Reflect.ownKeys(a), base.concat("extra"), "a gained extra");
assert.compareArray(Reflect.ownKeys(b), base.concat("other", "extra"), "b gained other, extra");
assert.compareArray(Reflect.ownKeys(c), base, "c is unaffected");
assert.sameValue(readExtra(a), "a", "extra at the sixth slot");
assert.sameValue(readExtra(b), "b2", "extra at the seventh slot");
assert.sameValue(readExtra(c), undefined, "no extra");

// Deleting the last key, then adding a different one.
delete a.extra;
a.second = 2;
assert.compareArray(Reflect.ownKeys(a), base.concat("second"), "last key replaced");
assert.sameValue(readExtra(a), undefined, "deleted key reads undefined at a warm site");
a.extra = "again";
assert.compareArray(Reflect.ownKeys(a), base.concat("second", "extra"), "re-added key is last");
assert.sameValue(readExtra(a), "again", "re-added key reads its new value");

// Deleting an earlier key keeps the order of the rest and frees the name.
delete b.pos;
assert.compareArray(
  Reflect.ownKeys(b),
  ["kind", "end", "flags", "parent", "other", "extra"],
  "earlier key deleted"
);
assert.sameValue(readEnd(b), 21, "later key still reads");
b.pos = 99;
assert.compareArray(
  Reflect.ownKeys(b),
  ["kind", "end", "flags", "parent", "other", "extra", "pos"],
  "deleted key re-added last"
);
assert.sameValue(b.pos, 99, "re-added value");
assert.sameValue(new Node(4, 5).pos, 5, "later instances unaffected");

// A deleted own property exposes the prototype's.
Node.prototype.end = "proto";
delete c.end;
assert.sameValue(readEnd(c), "proto", "inherited after deleting the own property");
c.end = 31;
assert.sameValue(readEnd(c), 31, "own again");
assert.sameValue(readEnd(a), 11, "other instance keeps its own");
delete Node.prototype.end;

// Many keys on one of many look-alike objects.
var objects = [];
for (var i = 0; i < 40; i++) objects.push(new Node(i, i));
var wide = objects[17];
var names = base.slice();
for (var k = 0; k < 70; k++) {
  wide["w" + k] = k;
  names.push("w" + k);
}
assert.compareArray(Reflect.ownKeys(wide), names, "70 extra keys");
for (var k = 0; k < 70; k += 7) {
  assert.sameValue(wide["w" + k], k, "w" + k);
}
assert.compareArray(Reflect.ownKeys(objects[16]), base, "neighbour unaffected");
delete wide.w3;
names.splice(names.indexOf("w3"), 1);
assert.compareArray(Reflect.ownKeys(wide), names, "after a deletion among many");
wide.w3 = "last";
names.push("w3");
assert.compareArray(Reflect.ownKeys(wide), names, "re-added among many");

// Construction orders that differ only in the order of the same keys.
function ab() { var o = {}; o.x = 1; o.y = 2; return o; }
function ba() { var o = {}; o.y = 2; o.x = 1; return o; }
function read(o) { return o.x + o.y * 10; }
var sum = 0;
for (var i = 0; i < 20; i++) sum += read(i % 2 ? ab() : ba());
assert.sameValue(sum, 420, "a read site over two key orders");
assert.compareArray(Reflect.ownKeys(ab()), ["x", "y"], "x then y");
assert.compareArray(Reflect.ownKeys(ba()), ["y", "x"], "y then x");
assert.sameValue(JSON.stringify(ab()), '{"x":1,"y":2}', "JSON x then y");
assert.sameValue(JSON.stringify(ba()), '{"y":2,"x":1}', "JSON y then x");
